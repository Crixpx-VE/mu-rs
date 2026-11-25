//! In-game chat: the input box (`SEASON3B::CNewUIChatInputBox`), the message window
//! (`SEASON3B::CNewUIChatLogWindow`), the S6 system message window (MuMain's `CSystemLogWindow`)
//! and what comes from the server (`ReceiveChat`, `ReceiveChatWhisper`,
//! `ReceiveChatWhisperResult`, `ReceiveNotice` and "Welcome to ..." on entering and changing maps).
//! The logic is in chat_log.rs; here, keyboard, mouse, network and drawing.
//!
//! - **Keys** (`CNewUIChatInputBox::UpdateKeyEvent`, NewUIChatInputBox.cpp:366-614): Enter opens
//!   the box and, when open, sends; Esc closes it; F2 whispers only / all; F3 hides or shows the
//!   whisper name box; F4 (with the background) resizes; F5 (open) toggles the background;
//!   PgUp/PgDn (with the background) scroll one screen; up/down, the history of the focused
//!   box; Tab switches box. While typing, the box has the focus (`GetFocus()` is its
//!   `edit`): no other window or hotkey gets keys (they are cleared in `PreUpdate`).
//! - **Mouse**: the bar buttons on release (with `SOUND_CLICK01`) and their tooltip on hover; the wheel
//!   over the messages scrolls them (and does not zoom the camera); with the background, drag the
//!   scroll bar or the resize bar; right click on a line with a name sets it for whispering.
//! - **Drawing**: on the 640×480 screen (`UiScale`), stuck to the bottom and centered like the bar.
//!   S6 bar of 281×47 at (0, 382) with its 10 buttons (normal, party, guild, Gens | no
//!   whispers, system, chat | background, size, transparency); messages from y = 383 upwards,
//!   each with a background as wide as its text; system messages at (4, 84) downwards.
//! - **Overhead**: what someone says (the hero too) goes to their `CHAT` box in names.rs,
//!   below the name (`AssignChat` → `CreateChat`, ZzzInterface.cpp:1106-1237).
//! - **Font**: `g_hFont` at 1024×768 (13 px, 8.125 on the 640 screen) with the Noto Sans from
//!   fonts.rs; widths are measured with the same font ([`TextMetrics`]) to wrap lines.
//!
//! Not implemented yet: party, guild and Gens (their messages reach nobody),
//! client chat commands except `/filter`, `/warp`, `/whisper on|off` and the Icarus warning
//! of `/move`, transformation ring gestures (the other gestures when talking, from
//! `CheckChatText`, are: chat_log.rs and character.rs), swear filter (`CheckAbuseFilter`),
//! right click on a player to whisper them and the
//! Gens button tooltip (not in Text_eng.bmd).

use std::collections::HashMap;

use ab_glyph::Font as _;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;
use bevy::text::{FontSize, FontSmoothing, FontWeight, LineHeight};
use mu_protocol::game::{ClientMessage, ServerMessage, notice_kind};

use crate::Args;
use crate::chat_log::{ChatInput, ChatLog, InputType, LogEvent, MouseIn, MsgType, Outgoing, SystemLog, WarpName};
use crate::notices::NoticeColor;
use crate::scene::GameScene;
use crate::sound::SoundQueue;

pub struct ChatPlugin;

/// Chat keys (before the minimap's and the rest).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ChatKeys;

impl Plugin for ChatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Chat>()
            .init_resource::<TextMetrics>()
            .init_resource::<PendingSay>()
            .add_systems(Startup, load_metrics)
            .add_systems(PreUpdate, net_messages.after(crate::net::NetSystems))
            .add_systems(
                PreUpdate,
                (chat_keys.in_set(ChatKeys), chat_mouse).chain().after(bevy::input::InputSystems).run_if(in_state(GameScene::Game)),
            )
            .add_systems(OnEnter(GameScene::Game), spawn_root)
            .add_systems(OnExit(GameScene::Game), (crate::scene::despawn::<ChatRoot>, leave_game))
            .add_systems(Update, (tick, object_messages, assign_chat, draw).chain().run_if(in_state(GameScene::Game)));
    }
}

/// What someone said that is not yet in their overhead box: (name, text, `Flag`).
#[derive(Resource, Default)]
pub struct PendingSay(Vec<(String, String, u8)>);

/// `ReceiveChatKey` (WSclient.cpp:1553-1575): `CreateChat(ID, ChatText, &CharactersClient[Index])`,
/// the text over the head of the character with that scope id (NPCs and monsters too, e.g. a quest
/// NPC turning the hero away). (The guild master case at 1559-1572 needs the guild window.)
fn object_messages(
    mut from: MessageReader<crate::net::FromServer>,
    mut boxes: ResMut<crate::names::NameBoxes>,
    monsters: Query<(Entity, &crate::monsters::Monster)>,
    players: Query<(Entity, &crate::remote::RemotePlayer)>,
) {
    for m in from.read() {
        let ServerMessage::ObjectMessage { id, message } = &m.0 else { continue };
        // `FindCharacterIndex(((KeyH) << 8) + KeyL)`; the viewport keys have no flag bit.
        let key = id & 0x7FFF;
        let owner = monsters
            .iter()
            .find(|(_, m)| m.key == key)
            .map(|(e, _)| e)
            .or_else(|| players.iter().find(|(_, p)| p.id == key).map(|(e, _)| e));
        match owner {
            Some(e) => {
                boxes.say(e, message, 0);
            }
            None => info!("chat: mensaje para {key}, que no está a la vista: {message}"),
        }
    }
}

/// `AssignChat` (ZzzInterface.cpp:1204-1237): the box of the player with that name (the hero too)
/// in names.rs. With `Flag` 1 (`#`), if found, `AddChat` also writes it to the chat.
fn assign_chat(
    mut pending: ResMut<PendingSay>,
    mut boxes: ResMut<crate::names::NameBoxes>,
    mut chat: ResMut<Chat>,
    metrics: Res<TextMetrics>,
    hero_name: Option<Res<crate::account::Hero>>,
    hero: Query<Entity, With<crate::character::Player>>,
    others: Query<(Entity, &crate::names::Selectable)>,
) {
    if pending.0.is_empty() {
        return;
    }
    let hero_name = hero_name.map_or_else(crate::account::current_account, |h| h.name.clone());
    for (name, text, flag) in std::mem::take(&mut pending.0) {
        let find = |kind: crate::names::Kind| others.iter().find(|(_, s)| s.kind == kind && s.name == name && !s.dead).map(|(e, _)| e);
        let owner = if name == hero_name { hero.single().ok() } else { None }
            .or_else(|| find(crate::names::Kind::Player))
            .or_else(|| find(crate::names::Kind::Monster));
        let Some(owner) = owner else { continue };
        if boxes.say(owner, &text, flag) && flag == 1 {
            chat.add(&name, &text, MsgType::Chat, MsgType::All, &metrics);
        }
    }
}

// --- Sizes (640×480 screen) -------------------------------------------------------------------

/// `g_hFont` at 1024×768: 13 px, in units of the 480-high screen (like options.rs).
pub const FONT: f32 = 13.0 * 480.0 / 768.0;
const SMOOTHING: FontSmoothing = FontSmoothing::AntiAliased;
/// `Create(m_pNewUIMng, m_pNewChatLogWindow, 0, 480-51-47)` (NewUISystem.cpp:149).
const INPUT_X: i32 = 0;
const INPUT_Y: i32 = 480 - 51 - 47;
/// `CHATBOX_WIDTH` × `WND_HEIGHT` (the S6 bar).
const INPUT_W: i32 = crate::chat_log::WND_WIDTH;
const INPUT_H: i32 = 47;
const BTN_W: i32 = 27;
const BTN_H: i32 = 26;
/// The 10 buttons: every 27 with 6 between groups (4 | 3 | 3), like MuMain's chat_input.rcss.
const BUTTON_X: [i32; 10] = [0, 27, 54, 81, 114, 141, 168, 201, 228, 255];
const BTN_WHISPER: usize = 4;
const BTN_SYSTEM: usize = 5;
const BTN_CHATLOG: usize = 6;
const BTN_FRAME: usize = 7;
const BTN_SIZE: usize = 8;
const BTN_ALPHA: usize = 9;
/// `GlobalText[1681, 1682, 1683, —, 1684, 1685, 750, 1686, 751, 752]` (`RenderTooltip`,
/// NewUIChatInputBox.cpp:730). The Gens one is not in Text_eng.bmd.
const TOOLTIP: [Option<&str>; 10] = [
    Some("Regular chat"),
    Some("Party chat"),
    Some("Guilt chat"),
    None,
    Some("Whisper block: On/Off"),
    Some("System message pop-up"),
    Some("Chat display On/Off (F2)"),
    Some("Chat window background On/Off (F5)"),
    Some("Size Adjustment (F4)"),
    Some("Transparency Adjustment"),
];
/// The text boxes (`SetPosition(m_WndPos.x+72, m_WndPos.y+32)` 176×14 and `+5` 50×14).
const CHAT_FIELD: (i32, i32, i32, i32) = (INPUT_X + 72, INPUT_Y + 32, 176, 14);
const WHISPER_FIELD: (i32, i32, i32, i32) = (INPUT_X + 5, INPUT_Y + 32, 50, 14);
/// `SetTextColor(255, 255, 230, 210)` and `(255, 200, 200, 200)` (ARGB).
const CHAT_COLOR: [u8; 4] = [255, 230, 210, 255];
const WHISPER_COLOR: [u8; 4] = [200, 200, 200, 255];
/// `m_iCaretBlinkTemp % 24 < 12`, one step per original frame.
const CARET_PERIOD: f32 = 24.0 / 25.0;
/// MuMain's system message window: (0, 80), the first line at (4, 84) and one every
/// 1.2 font heights, with the black background at the usual transparency (0.6).
const SYSTEM_X: f32 = 4.0;
const SYSTEM_Y: f32 = 84.0;
const SYSTEM_ROW: f32 = FONT * 1.2;
/// The original's `TICKS_PER_SECOND` (`ChatTime--` once per tick).
const TICKS: f32 = 25.0;

/// `CheckMouseIn`.
fn inside(m: (i32, i32), x: i32, y: i32, w: i32, h: i32) -> bool {
    m.0 >= x && m.0 < x + w && m.1 >= y && m.1 < y + h
}

// --- Text measuring ---------------------------------------------------------------------------

/// Text widths with the interface font (the original's `_GetTextExtentPoint32`), in
/// units of the 640 screen. Without the file, an approximation of half a font size per character.
#[derive(Resource, Default)]
pub struct TextMetrics {
    font: Option<ab_glyph::FontArc>,
}

impl TextMetrics {
    /// Width with `g_hFont`.
    pub fn width(&self, s: &str) -> f32 {
        self.width_at(s, FONT)
    }

    /// Width with the font at `size` units (without the GPOS kerning that drawing applies).
    pub fn width_at(&self, s: &str, size: f32) -> f32 {
        let Some(f) = &self.font else { return s.chars().count() as f32 * size * 0.5 };
        let upem = f.units_per_em().unwrap_or(1000.0);
        let mut w = 0.0;
        let mut prev = None;
        for c in s.chars() {
            let g = f.glyph_id(c);
            if let Some(p) = prev {
                w += f.kern_unscaled(p, g);
            }
            w += f.h_advance_unscaled(g);
            prev = Some(g);
        }
        w * size / upem
    }
}

fn load_metrics(args: Res<Args>, mut metrics: ResMut<TextMetrics>) {
    let dir = args.data.join("Fonts");
    match mu_data::fs::read(&dir, "NotoSansKR-Regular.ttf").ok().map(ab_glyph::FontArc::try_from_vec) {
        Some(Ok(f)) => metrics.font = Some(f),
        _ => warn!("chat: sin la letra para medir el texto, se aproxima"),
    }
}

// --- State -------------------------------------------------------------------------------------

/// The whole chat: the message window, the system one and the input box.
#[derive(Resource, Default)]
pub struct Chat {
    pub log: ChatLog,
    pub system: SystemLog,
    pub input: ChatInput,
    /// `m_iTooltipType`.
    tooltip: Option<usize>,
    /// The size and transparency buttons, pressed over them (`BUTTON_STATE_DOWN`).
    size_down: bool,
    alpha_down: bool,
    caret_clock: f32,
    tick_clock: f32,
    /// `WorldActive` of the last "Welcome to" message.
    map: Option<u16>,
    /// Center notices (notices.rs).
    pub notices: crate::notices::Notices,
}

impl Chat {
    /// The mouse (640×480 screen) is over the open box or something the message window
    /// grabs: a click there does not walk.
    pub fn blocks(&self, c: Vec2) -> bool {
        let m = (c.x.floor() as i32, c.y.floor() as i32);
        (self.input.open && inside(m, INPUT_X, INPUT_Y, INPUT_W, INPUT_H)) || self.log.blocks(m)
    }

    /// `g_pChatListBox->AddText`; returns whether the filter alert sounds.
    pub fn add(&mut self, id: &str, text: &str, kind: MsgType, err: MsgType, metrics: &TextMetrics) -> bool {
        self.log.add_text(id, text, kind, err, &|s| metrics.width(s))
    }
}

/// On leaving the game (or changing maps, `HideAll`): the box closes (and its background with it).
fn leave_game(mut chat: ResMut<Chat>) {
    chat.input.close();
    chat.log.set_frame(false);
    chat.log.event = LogEvent::None;
    chat.tooltip = None;
}

/// `ChatTime--`, `MoveNotices` and the caret blink.
fn tick(time: Res<Time>, mut chat: ResMut<Chat>, metrics: Res<TextMetrics>) {
    let chat = &mut *chat;
    chat.caret_clock += time.delta_secs();
    chat.tick_clock += time.delta_secs() * TICKS;
    while chat.tick_clock >= 1.0 {
        chat.tick_clock -= 1.0;
        chat.input.tick();
        chat.notices.tick(&|s| metrics.width(s));
    }
}

// --- Network -----------------------------------------------------------------------------------

/// `ReceiveChat`, `ReceiveChatWhisper`, `ReceiveChatWhisperResult`, `ReceiveNotice` and the "Welcome
/// to" of `ReceiveJoinMapServer` / `ReceiveTeleport`.
#[allow(clippy::too_many_arguments)]
fn net_messages(
    mut from: MessageReader<crate::net::FromServer>,
    mut chat: ResMut<Chat>,
    metrics: Res<TextMetrics>,
    opts: Res<crate::options::GameOptions>,
    scene: Res<State<GameScene>>,
    mut bubbles: ResMut<PendingSay>,
    mut sound: ResMut<SoundQueue>,
) {
    let chat = &mut *chat;
    for m in from.read() {
        match &m.0 {
            ServerMessage::ChatMessage { sender, message } => {
                // The prefix gives the channel (WSclient.cpp:1435-1510).
                let (kind, text) = if let Some(t) = message.strip_prefix('~') {
                    (Some(MsgType::Party), t)
                } else if let Some(t) = message.strip_prefix("@@") {
                    (Some(MsgType::Union), t)
                } else if let Some(t) = message.strip_prefix('@') {
                    (Some(MsgType::Guild), t)
                } else if let Some(t) = message.strip_prefix('$') {
                    (Some(MsgType::Gens), t)
                } else if let Some(t) = message.strip_prefix('#') {
                    // No GMs: only the long overhead box, which also writes it to the chat
                    // (`AssignChat(ID, Text, 1)` → `AddChat` with `Flag` 1, [`assign_chat`]).
                    bubbles.0.push((sender.clone(), t.to_string(), 1));
                    (None, t)
                } else {
                    // `AssignChat` + `TYPE_CHAT_MESSAGE` (without GMs there is no `TYPE_GM_MESSAGE`).
                    bubbles.0.push((sender.clone(), message.clone(), 0));
                    (Some(MsgType::Chat), message.as_str())
                };
                if let Some(kind) = kind
                    && chat.add(sender, text, kind, MsgType::All, &metrics)
                    && opts.whisper_sound
                {
                    sound.play("SOUND_WHISPER");
                }
            }
            ServerMessage::WhisperMessage { sender, message } => {
                if chat.input.block_whisper {
                    continue;
                }
                if opts.whisper_sound {
                    sound.play("SOUND_WHISPER");
                }
                chat.add(sender, message, MsgType::Whisper, MsgType::All, &metrics);
            }
            ServerMessage::WhisperResult { success: false } => {
                let id = chat.input.whisper_id.clone();
                chat.add(&id, crate::chat_log::TEXT_NO_USERS, MsgType::Error, MsgType::Whisper, &metrics);
            }
            ServerMessage::Notice { kind, text } => match *kind {
                notice_kind::GOLDEN_CENTER => chat.notices.create(text, NoticeColor::Golden, &|s| metrics.width(s)),
                // `sprintf(Text, GlobalText[483], Notice)` (plus the guild window, which does not exist).
                notice_kind::GUILD => {
                    let t = crate::notices::TEXT_GUILD_NOTICE.replace("%s", text);
                    chat.notices.create(&t, NoticeColor::Guild, &|s| metrics.width(s));
                }
                // In character selection the original puts it in its server message box
                // (`CUIMng::AddServerMsg`), which does not exist here.
                notice_kind::BLUE if *scene.get() != GameScene::CharacterSelect => chat.system.add(text, MsgType::System),
                k => info!("aviso del servidor de tipo {k} sin mostrar: {text}"),
            },
            ServerMessage::CharacterInformation(info) => {
                // `ClearNotice` (WSclient.cpp:662).
                chat.notices.clear();
                welcome(chat, info.map as u16)
            }
            ServerMessage::MapChanged { is_map_change: true, map, .. } if chat.map != Some(*map) => welcome(chat, *map),
            _ => {}
        }
    }
}

/// "Welcome to <map>" in the system messages.
fn welcome(chat: &mut Chat, map: u16) {
    chat.map = Some(map);
    if let Some(text) = crate::chat_log::welcome_text(map) {
        chat.system.add(&text, MsgType::System);
    }
}

// --- Keyboard ----------------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn chat_keys(
    mut chat: ResMut<Chat>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut typed: MessageReader<KeyboardInput>,
    game_ui: Res<crate::options::GameUi>,
    metrics: Res<TextMetrics>,
    hero: Option<Res<crate::account::Hero>>,
    warps: Option<Res<crate::move_window::MoveList>>,
    mut move_window: ResMut<crate::move_window::MoveWindow>,
    mut net: ResMut<crate::net::Net>,
    mut sound: ResMut<SoundQueue>,
    mut players: Query<&mut crate::character::Player>,
) {
    let typed: Vec<Key> = crate::clipboard::typed_keys(typed.read().cloned(), &keys);
    let chat = &mut *chat;
    let was_open = chat.input.open;
    // With the system message box (order 10) the keys are its own; with the chat open, the
    // focus is on its `edit` and only it gets them.
    if !was_open && game_ui.modal() {
        return;
    }
    let jp = |k: KeyCode| keys.just_pressed(k);
    let (f2, f3, f4, f5) = (jp(KeyCode::F2), jp(KeyCode::F3), jp(KeyCode::F4), jp(KeyCode::F5));
    let (page_up, page_down) = (jp(KeyCode::PageUp), jp(KeyCode::PageDown));
    let enter = jp(KeyCode::Enter) || jp(KeyCode::NumpadEnter);
    let (up, down, esc) = (jp(KeyCode::ArrowUp), jp(KeyCode::ArrowDown), jp(KeyCode::Escape));
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let measure = |s: &str| metrics.width(s);
    let frame = chat.log.show_frame();
    // What was used with the box closed (so it reaches nobody else).
    let mut used: Vec<KeyCode> = Vec::new();

    // `UpdateKeyEvent`: the first one that applies, and done.
    if f2 {
        chat.input.whisper_only = !chat.input.whisper_only;
        chat.log.change_message(if chat.input.whisper_only { MsgType::Whisper } else { MsgType::All });
        sound.play("SOUND_CLICK01");
        used.push(KeyCode::F2);
    } else if f3 {
        chat.input.whisper_send = !chat.input.whisper_send;
        if !chat.input.whisper_send {
            chat.input.whisper_focus = false;
        }
        used.push(KeyCode::F3);
    } else if frame && f4 {
        chat.log.size_auto();
        sound.play("SOUND_CLICK01");
        used.push(KeyCode::F4);
    } else if f5 {
        if was_open {
            chat.log.set_frame(!frame);
        }
        sound.play("SOUND_CLICK01");
        used.push(KeyCode::F5);
    } else if frame && page_up {
        chat.log.scrolling(chat.log.end_line() - chat.log.showing());
    } else if frame && page_down {
        chat.log.scrolling(chat.log.end_line() + chat.log.showing());
    } else if !was_open && enter {
        // Without Chaos Castle (nor its timer window), it always opens.
        chat.input.open();
        chat.caret_clock = 0.0;
        used.extend([KeyCode::Enter, KeyCode::NumpadEnter]);
    } else if was_open && enter {
        let hero_name = hero.map_or_else(crate::account::current_account, |h| h.name.clone());
        let warps: Vec<WarpName> = warps
            .map(|w| w.0.iter().map(|m| WarpName { index: m.index, name: m.name.clone(), sub_name: m.sub_name.clone() }).collect())
            .unwrap_or_default();
        let out = chat.input.submit(&hero_name, &warps, &mut chat.log, &mut chat.system, &measure);
        // `ClosingProcess`: the background goes away with the box.
        chat.log.set_frame(false);
        for o in out {
            match o {
                Outgoing::Chat(message) => net.send(ClientMessage::PublicChat { name: hero_name.clone(), message }),
                Outgoing::Whisper(target, message) => net.send(ClientMessage::Whisper { target, message }),
                // An unknown name sends −1 as a `WORD` (0xFFFF), like the original.
                Outgoing::Warp(index) => move_window.request_warp(&mut net, index as u16),
                Outgoing::Emote(action) => {
                    if let Ok(mut p) = players.single_mut() {
                        crate::character::chat_emote(&mut p, action, &mut net);
                    }
                }
            }
        }
    } else if was_open && (up || down) {
        let input = &mut chat.input;
        if !input.whisper_focus && !input.chat_history.is_empty() {
            crate::chat_log::step_history(&mut input.cur_chat, input.chat_history.len(), up);
            let t = input.chat_history[input.cur_chat as usize].clone();
            input.chat.set(&t);
        } else if input.whisper_focus && input.whisper_send && !input.whisper_history.is_empty() {
            crate::chat_log::step_history(&mut input.cur_whisper, input.whisper_history.len(), up);
            let t = input.whisper_history[input.cur_whisper as usize].clone();
            input.whisper.set(&t);
        }
    } else if was_open && esc {
        chat.input.close();
        chat.log.set_frame(false);
        sound.play("SOUND_CLICK01");
    }

    // What is typed into the focused box (the Windows `edit`).
    if was_open && chat.input.open {
        let mut changed = false;
        for key in &typed {
            match key {
                Key::Character(t) if !ctrl => {
                    for c in t.chars().filter(|c| !c.is_control()) {
                        chat.input.focused().insert(c);
                    }
                }
                Key::Space => chat.input.focused().insert(' '),
                Key::Backspace => chat.input.focused().backspace(),
                Key::Delete => chat.input.focused().delete(),
                Key::ArrowLeft => chat.input.focused().left(),
                Key::ArrowRight => chat.input.focused().right(),
                Key::Home => chat.input.focused().move_to(0),
                Key::End => {
                    let b = chat.input.focused();
                    let n = b.len();
                    b.move_to(n);
                }
                Key::Tab => chat.input.tab(),
                Key::Paste => chat.input.focused().paste(&crate::clipboard::get()),
                Key::Copy => crate::clipboard::set(chat.input.focused().copy_text()),
                Key::Cut => crate::clipboard::set(&chat.input.focused().cut()),
                Key::Select => chat.input.focused().select_everything(),
                _ => continue,
            }
            changed = true;
        }
        if changed {
            chat.caret_clock = 0.0;
        }
    }
    chat.input.chat.follow_caret(CHAT_FIELD.2 as f32, &measure);
    chat.input.whisper.follow_caret(WHISPER_FIELD.2 as f32, &measure);

    if was_open {
        // Nothing pressed while typing reaches the game (hotkeys, skills...).
        keys.clear();
    } else {
        for k in used {
            keys.clear_just_pressed(k);
        }
    }
}

// --- Mouse -------------------------------------------------------------------------------------

/// `UpdateMouseEvent` of the box (layer 6.2) and, if it did not take the mouse, that of the message
/// window (6.1); then `UpdateScrollPos`.
fn chat_mouse(
    mut chat: ResMut<Chat>,
    windows: Query<&Window>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut scroll: ResMut<AccumulatedMouseScroll>,
    game_ui: Res<crate::options::GameUi>,
    mut sound: ResMut<SoundQueue>,
) {
    let chat = &mut *chat;
    let cursor = windows.single().ok().and_then(crate::mainframe::cursor_virtual).map(|c| (c.x.floor() as i32, c.y.floor() as i32));
    // With the system message box open, the mouse is its own.
    if game_ui.modal() {
        chat.tooltip = None;
        return;
    }
    let (held, release) = (mouse.pressed(MouseButton::Left), mouse.just_released(MouseButton::Left));
    let consumed = if chat.input.open {
        input_mouse(chat, cursor, mouse.just_pressed(MouseButton::Left), held, release, &mut sound)
    } else {
        chat.tooltip = None;
        chat.size_down = false;
        chat.alpha_down = false;
        false
    };
    if !consumed {
        let out = chat.log.update_mouse(&MouseIn {
            pos: cursor,
            held,
            release,
            right_press: mouse.just_pressed(MouseButton::Right),
            wheel: scroll.delta.y,
        });
        if out.wheel_used {
            // `MouseWheel = 0`: the camera does not zoom.
            scroll.delta = Vec2::ZERO;
        }
        if let Some(id) = out.whisper_to {
            chat.input.whisper.set(&id);
        }
    }
    chat.log.update_scroll_pos();
}

/// `CNewUIChatInputBox::UpdateMouseEvent` (NewUIChatInputBox.cpp:231-364). `true` = it takes the mouse.
fn input_mouse(chat: &mut Chat, cursor: Option<(i32, i32)>, press: bool, held: bool, release: bool, sound: &mut SoundQueue) -> bool {
    let at = |x: i32, y: i32, w: i32, h: i32| cursor.is_some_and(|m| inside(m, x, y, w, h));
    let button = |i: usize| at(BUTTON_X[i], INPUT_Y, BTN_W, BTN_H);
    chat.tooltip = (0..BUTTON_X.len()).find(|&i| button(i));
    // A click on a text box gives it the focus (it is a Windows `edit`).
    if press {
        let (x, y, w, h) = WHISPER_FIELD;
        if chat.input.whisper_send && at(x, y, w, h) {
            chat.input.whisper_focus = true;
        }
        let (x, y, w, h) = CHAT_FIELD;
        if at(x, y, w, h) {
            chat.input.whisper_focus = false;
        }
    }
    let click = |sound: &mut SoundQueue| sound.play("SOUND_CLICK01");
    if release {
        if let Some(i) = (0..InputType::ALL.len()).find(|&i| button(i)) {
            chat.input.msg_type = InputType::ALL[i];
            click(sound);
            return true;
        }
        if button(BTN_WHISPER) {
            chat.input.block_whisper = !chat.input.block_whisper;
            click(sound);
            return true;
        }
        if button(BTN_SYSTEM) {
            // S6: shows or hides the system message window.
            chat.input.show_system = !chat.input.show_system;
            click(sound);
            return true;
        }
        if button(BTN_CHATLOG) {
            chat.input.show_chat_log = !chat.input.show_chat_log;
            if chat.input.show_chat_log {
                chat.log.show_chat_log();
            } else {
                chat.log.hide_chat_log();
            }
            click(sound);
            return true;
        }
        if button(BTN_FRAME) {
            let on = !chat.log.show_frame();
            chat.log.set_frame(on);
            click(sound);
            return true;
        }
    }
    if chat.log.show_frame() {
        // `CNewUIButton::Process`: pressed over it = frame 1; released over it, it fires.
        chat.size_down = held && button(BTN_SIZE);
        chat.alpha_down = held && button(BTN_ALPHA);
        if release && button(BTN_SIZE) {
            chat.log.size_auto();
            click(sound);
            return true;
        }
        if release && button(BTN_ALPHA) {
            chat.log.back_alpha_auto();
            click(sound);
            return true;
        }
    } else {
        chat.size_down = false;
        chat.alpha_down = false;
    }
    at(INPUT_X, INPUT_Y, INPUT_W, INPUT_H)
}

// --- Drawing -----------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Tex {
    Back,
    NormalOn,
    PartyOn,
    GuildOn,
    GensOn,
    WhisperOn,
    SystemOn,
    ChatOn,
    FrameOn,
    BtnSize,
    BtnAlpha,
    ScrollTop,
    ScrollMiddle,
    ScrollBottom,
    ScrollOn,
    Stretch,
}

impl Tex {
    const ALL: [Tex; 16] = [
        Tex::Back,
        Tex::NormalOn,
        Tex::PartyOn,
        Tex::GuildOn,
        Tex::GensOn,
        Tex::WhisperOn,
        Tex::SystemOn,
        Tex::ChatOn,
        Tex::FrameOn,
        Tex::BtnSize,
        Tex::BtnAlpha,
        Tex::ScrollTop,
        Tex::ScrollMiddle,
        Tex::ScrollBottom,
        Tex::ScrollOn,
        Tex::Stretch,
    ];

    /// `LoadImages` of the box (NewUIChatInputBox.cpp:44-56, with the Gens one) and of the window
    /// (NewUIChatLogWindow.cpp:45-53).
    fn file(self) -> &'static str {
        match self {
            Tex::Back => "newui_chat_back.jpg",
            Tex::NormalOn => "newui_chat_normal_on.jpg",
            Tex::PartyOn => "newui_chat_party_on.jpg",
            Tex::GuildOn => "newui_chat_guild_on.jpg",
            Tex::GensOn => "newui_chat_gens_on.jpg",
            Tex::WhisperOn => "newui_chat_whisper_on.jpg",
            Tex::SystemOn => "newui_chat_system_on.jpg",
            Tex::ChatOn => "newui_chat_chat_on.jpg",
            Tex::FrameOn => "newui_chat_frame_on.jpg",
            Tex::BtnSize => "newui_chat_btn_size.jpg",
            Tex::BtnAlpha => "newui_chat_btn_alpha.jpg",
            Tex::ScrollTop => "newui_scrollbar_up.tga",
            Tex::ScrollMiddle => "newui_scrollbar_m.tga",
            Tex::ScrollBottom => "newui_scrollbar_down.tga",
            Tex::ScrollOn => "newui_scroll_on.tga",
            Tex::Stretch => "newui_Scrollbar_stretch.jpg",
        }
    }

    /// The lit button of each message type (`IMAGE_INPUTBOX_NORMAL_ON + m_iInputMsgType`).
    fn type_on(t: InputType) -> Tex {
        match t {
            InputType::Normal => Tex::NormalOn,
            InputType::Party => Tex::PartyOn,
            InputType::Guild => Tex::GuildOn,
            InputType::Gens => Tex::GensOn,
        }
    }
}

#[derive(Resource)]
struct ChatTextures(HashMap<Tex, Handle<Image>>);

/// A piece on the 640×480 screen.
#[derive(Clone, PartialEq, Debug)]
enum Piece {
    /// `RenderColor` with the `glColor4f` color.
    Fill { x: f32, y: f32, w: f32, h: f32, rgba: [u8; 4] },
    /// `RenderImage(tex, x, y, width, height, 0, sv)`: the part from (0, sv), without stretching;
    /// `dark` = `glColor4f(0.7, 0.7, 0.7)`.
    Image { tex: Tex, x: f32, y: f32, w: f32, h: f32, sv: f32, dark: bool },
    /// `RenderText` at (x, y) with a background as wide as the text (plus `pad` on each side).
    Text { x: f32, y: f32, text: String, color: [u8; 4], bg: Option<[u8; 4]>, bold: bool, pad: f32 },
    /// `RenderText(320, y, ..., RT3_WRITE_CENTER)` in bold with its background (center notices).
    Centered { y: f32, text: String, color: [u8; 4], bg: [u8; 4] },
    /// A text box: what is before and after the caret, shifted by `scroll`.
    Field { x: f32, y: f32, w: f32, h: f32, before: String, after: String, color: [u8; 4], caret: bool, selected: bool, scroll: f32 },
}

fn img(tex: Tex, x: i32, y: i32, w: i32, h: i32) -> Piece {
    Piece::Image { tex, x: x as f32, y: y as f32, w: w as f32, h: h as f32, sv: 0.0, dark: false }
}

impl Chat {
    /// What is drawn, back to front: system messages (layer 6.05), message
    /// window (6.1) and box (6.2).
    fn pieces(&self, metrics: &TextMetrics) -> Vec<Piece> {
        let mut out = Vec::new();
        // `RenderNotices` (before the interface, ZzzScene.cpp:585).
        for (i, n) in self.notices.lines().iter().enumerate() {
            if n.text.is_empty() {
                continue;
            }
            out.push(Piece::Centered {
                y: 300.0 + 13.0 * i as f32,
                text: n.text.clone(),
                color: self.notices.text_color(n.color),
                bg: [0, 0, 0, 128],
            });
        }
        if self.input.show_system {
            for (i, line) in self.system.lines().iter().enumerate() {
                let [r, g, b] = SystemLog::color(line.kind);
                out.push(Piece::Text {
                    x: SYSTEM_X,
                    y: SYSTEM_Y + SYSTEM_ROW * i as f32,
                    text: line.text.clone(),
                    color: [r, g, b, 255],
                    bg: Some([0, 0, 0, 153]),
                    bold: false,
                    pad: 0.0,
                });
            }
        }
        self.push_log(&mut out);
        if self.input.open {
            self.push_input(&mut out, metrics);
        }
        out
    }

    /// `CNewUIChatLogWindow::Render`: background, messages and frame (NewUIChatLogWindow.cpp:65-237).
    fn push_log(&self, out: &mut Vec<Piece>) {
        use crate::chat_log::*;
        let log = &self.log;
        let (cy, top) = (log.wnd_height(), log.wnd_top());
        let frame = log.show_frame();
        if frame {
            let a = (log.back_alpha().clamp(0.0, 1.0) * 255.0) as u8;
            out.push(Piece::Fill { x: LOG_X as f32, y: top as f32, w: WND_WIDTH as f32, h: cy as f32, rgba: [0, 0, 0, a] });
        }
        if log.show_log() {
            for (i, line, y) in log.visible() {
                if line.text.is_empty() {
                    continue;
                }
                let pointed = log.pointed() == Some(i) && !line.id.is_empty();
                let Some((bg, color, bold)) = line_colors(line.kind, frame, pointed) else { continue };
                out.push(Piece::Text {
                    x: (LOG_X + WND_LEFT_RIGHT_EDGE) as f32,
                    y: y as f32,
                    text: line.shown(),
                    color,
                    bg: Some(bg),
                    bold,
                    pad: 0.0,
                });
            }
        }
        if frame {
            let mut stretch = img(Tex::Stretch, LOG_X, top - RESIZING_BTN_HEIGHT, WND_WIDTH, RESIZING_BTN_HEIGHT);
            if let Piece::Image { dark, .. } = &mut stretch {
                *dark = log.event == LogEvent::ResizeDown;
            }
            out.push(stretch);
            let bx = LOG_X + WND_WIDTH - SCROLL_BAR_WIDTH - WND_LEFT_RIGHT_EDGE;
            out.push(img(Tex::ScrollTop, bx, top + WND_TOP_BOTTOM_EDGE, SCROLL_BAR_WIDTH, SCROLL_TOP_BOTTOM_PART_HEIGHT));
            for i in 0..log.showing() {
                let y = top + WND_TOP_BOTTOM_EDGE + i * SCROLL_MIDDLE_PART_HEIGHT + SCROLL_TOP_BOTTOM_PART_HEIGHT;
                out.push(img(Tex::ScrollMiddle, bx, y, SCROLL_BAR_WIDTH, SCROLL_MIDDLE_PART_HEIGHT));
            }
            let by = LOG_BOTTOM - WND_TOP_BOTTOM_EDGE - SCROLL_TOP_BOTTOM_PART_HEIGHT;
            out.push(img(Tex::ScrollBottom, bx, by, SCROLL_BAR_WIDTH, SCROLL_TOP_BOTTOM_PART_HEIGHT));
            let (tx, ty) = log.scroll_btn();
            out.push(Piece::Image {
                tex: Tex::ScrollOn,
                x: tx as f32,
                y: ty as f32,
                w: SCROLL_BTN_WIDTH as f32,
                h: SCROLL_BTN_HEIGHT as f32,
                sv: 0.0,
                dark: log.event == LogEvent::ScrollBtnDown,
            });
        }
    }

    /// `CNewUIChatInputBox::Render` (NewUIChatInputBox.cpp:653-748).
    fn push_input(&self, out: &mut Vec<Piece>, metrics: &TextMetrics) {
        let input = &self.input;
        // `RenderFrame` and `RenderButtons`.
        out.push(img(Tex::Back, INPUT_X, INPUT_Y, INPUT_W, INPUT_H));
        let t = input.msg_type as usize;
        out.push(img(Tex::type_on(input.msg_type), INPUT_X + BUTTON_X[t], INPUT_Y, BTN_W, BTN_H));
        if input.block_whisper {
            out.push(img(Tex::WhisperOn, INPUT_X + BUTTON_X[BTN_WHISPER], INPUT_Y, BTN_W, BTN_H));
        }
        if input.show_system {
            out.push(img(Tex::SystemOn, INPUT_X + BUTTON_X[BTN_SYSTEM], INPUT_Y, BTN_W, BTN_H));
        }
        if input.show_chat_log {
            out.push(img(Tex::ChatOn, INPUT_X + BUTTON_X[BTN_CHATLOG], INPUT_Y, BTN_W, BTN_H));
        }
        if self.log.show_frame() {
            out.push(img(Tex::FrameOn, INPUT_X + BUTTON_X[BTN_FRAME], INPUT_Y, BTN_W, BTN_H));
            for (tex, i, down) in [(Tex::BtnSize, BTN_SIZE, self.size_down), (Tex::BtnAlpha, BTN_ALPHA, self.alpha_down)] {
                out.push(Piece::Image {
                    tex,
                    x: (INPUT_X + BUTTON_X[i]) as f32,
                    y: INPUT_Y as f32,
                    w: BTN_W as f32,
                    h: BTN_H as f32,
                    sv: if down { BTN_H as f32 } else { 0.0 },
                    dark: false,
                });
            }
        }
        // `RenderTooltip`: centered over the button, above the bar, with a black background at 180.
        if let Some(text) = self.tooltip.and_then(|i| TOOLTIP[i].map(|t| (i, t))) {
            let (i, text) = text;
            let w = metrics.width(text).floor();
            let x = ((INPUT_X + BUTTON_X[i] + 10) as f32 - (w / 2.0).floor()).max(0.0);
            let h = FONT.floor();
            out.push(Piece::Text { x, y: INPUT_Y as f32 - (h + 1.0), text: text.into(), color: [255, 255, 255, 255], bg: Some([0, 0, 0, 180]), bold: false, pad: 3.0 });
        }
        // The text boxes.
        let caret_on = (self.caret_clock % CARET_PERIOD) < CARET_PERIOD / 2.0;
        let field = |(x, y, w, h): (i32, i32, i32, i32), b: &crate::chat_log::TextBox, color: [u8; 4], focused: bool| {
            let (before, after) = b.split();
            Piece::Field {
                x: x as f32,
                y: y as f32,
                w: w as f32,
                h: h as f32,
                before: before.to_string(),
                after: after.to_string(),
                color,
                caret: focused && caret_on,
                selected: focused && b.select_all,
                scroll: b.scroll,
            }
        };
        out.push(field(CHAT_FIELD, &input.chat, CHAT_COLOR, !input.whisper_focus));
        if input.whisper_send {
            out.push(field(WHISPER_FIELD, &input.whisper, WHISPER_COLOR, input.whisper_focus));
        } else {
            // Without the name box (F3): its text dimmed and a reddish veil on top.
            out.push(Piece::Text {
                x: WHISPER_FIELD.0 as f32,
                y: WHISPER_FIELD.1 as f32,
                text: input.whisper.text.clone(),
                color: [255, 255, 255, 100],
                bg: None,
                bold: false,
                pad: 0.0,
            });
            out.push(Piece::Fill { x: (INPUT_X + 2) as f32, y: (INPUT_Y + 28) as f32, w: 61.0, h: 17.0, rgba: [128, 51, 51, 51] });
        }
    }
}

#[derive(Component)]
struct ChatRoot;

fn spawn_root(mut commands: Commands, args: Res<Args>, mut images: ResMut<Assets<Image>>) {
    let textures = Tex::ALL
        .iter()
        .filter_map(|&t| crate::scene::interface_image(&args, &mut images, t.file()).map(|(h, _)| (t, h)))
        .collect();
    commands.insert_resource(ChatTextures(textures));
    // The 640×480 screen centered and stuck to the bottom, like the bar; below the windows.
    commands.spawn((
        Name::new("Chat"),
        ChatRoot,
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(0.0),
            left: Val::Percent(50.0),
            margin: UiRect::left(Val::Px(-320.0)),
            width: Val::Px(640.0),
            height: Val::Px(480.0),
            ..default()
        },
        GlobalZIndex(0),
    ));
}

fn absolute(x: f32, y: f32, w: f32, h: f32) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: Val::Px(x),
        top: Val::Px(y),
        width: Val::Px(w),
        height: Val::Px(h),
        ..default()
    }
}

fn rgba(c: [u8; 4]) -> Color {
    Color::srgba_u8(c[0], c[1], c[2], c[3])
}

/// `g_hFont` (or `g_hFontBold`).
pub fn text_font(bold: bool) -> TextFont {
    let mut f = TextFont { font_size: FontSize::Px(FONT), font_smoothing: SMOOTHING, ..default() };
    if bold {
        f.weight = FontWeight::BOLD;
    }
    f
}

/// Rebuilds the chat when something drawn changes.
fn draw(
    mut commands: Commands,
    chat: Res<Chat>,
    metrics: Res<TextMetrics>,
    textures: Option<Res<ChatTextures>>,
    root: Query<Entity, With<ChatRoot>>,
    mut last: Local<(Option<Entity>, Vec<Piece>)>,
) {
    let (Ok(root), Some(textures)) = (root.single(), textures) else { return };
    let pieces = chat.pieces(&metrics);
    if last.0 == Some(root) && last.1 == pieces {
        return;
    }
    commands.entity(root).despawn_children();
    commands.entity(root).with_children(|p| {
        for piece in &pieces {
            match piece {
                Piece::Fill { x, y, w, h, rgba: c } => {
                    p.spawn((absolute(*x, *y, *w, *h), BackgroundColor(rgba(*c))));
                }
                Piece::Image { tex, x, y, w, h, sv, dark } => {
                    let Some(handle) = textures.0.get(tex) else { continue };
                    let mut image = ImageNode::new(handle.clone());
                    image.rect = Some(Rect::new(0.0, *sv, *w, sv + h));
                    if *dark {
                        image.color = Color::srgb(0.7, 0.7, 0.7);
                    }
                    p.spawn((absolute(*x, *y, *w, *h), image));
                }
                Piece::Text { x, y, text, color, bg, bold, pad } => {
                    let node = Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(*x),
                        top: Val::Px(*y),
                        height: Val::Px(FONT),
                        padding: UiRect::horizontal(Val::Px(*pad)),
                        ..default()
                    };
                    let mut e = p.spawn((
                        node,
                        Text::new(text.clone()),
                        text_font(*bold),
                        LineHeight::Px(FONT),
                        TextColor(rgba(*color)),
                        TextLayout::no_wrap(),
                    ));
                    if let Some(bg) = bg {
                        e.insert(BackgroundColor(rgba(*bg)));
                    }
                }
                Piece::Centered { y, text, color, bg } => {
                    // A full-width row that centers the text (with a background as wide as the text).
                    let row = Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(0.0),
                        top: Val::Px(*y),
                        width: Val::Px(640.0),
                        height: Val::Px(FONT),
                        justify_content: JustifyContent::Center,
                        ..default()
                    };
                    p.spawn(row).with_children(|r| {
                        r.spawn((
                            Text::new(text.clone()),
                            text_font(true),
                            LineHeight::Px(FONT),
                            TextColor(rgba(*color)),
                            TextLayout::no_wrap(),
                            BackgroundColor(rgba(*bg)),
                        ));
                    });
                }
                Piece::Field { x, y, w, h, before, after, color, caret, selected, scroll } => {
                    let mut node = absolute(*x, *y, *w, *h);
                    node.overflow = Overflow::clip();
                    let (color, caret, selected, scroll, h) = (*color, *caret, *selected, *scroll, *h);
                    p.spawn(node).with_children(|f| {
                        let row = Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(-scroll),
                            top: Val::Px(0.0),
                            height: Val::Px(h),
                            flex_direction: FlexDirection::Row,
                            ..default()
                        };
                        f.spawn(row).with_children(|row| {
                            let part = |t: &str| (Text::new(t.to_string()), text_font(false), LineHeight::Px(FONT), TextColor(rgba(color)), TextLayout::no_wrap());
                            let mut first = row.spawn(part(before));
                            if selected {
                                // `m_dwSelectBackColor`.
                                first.insert(BackgroundColor(Color::srgb_u8(150, 150, 150)));
                            }
                            // The caret takes no space: 2 wide and the font height, in gray (200, 200, 200).
                            row.spawn(Node { width: Val::Px(0.0), height: Val::Percent(100.0), ..default() }).with_children(|c| {
                                if caret {
                                    c.spawn((absolute(0.0, 0.0, 2.0, FONT), BackgroundColor(Color::srgb_u8(200, 200, 200))));
                                }
                            });
                            row.spawn(part(after));
                        });
                    });
                }
            }
        }
    });
    *last = (Some(root), pieces);
}
