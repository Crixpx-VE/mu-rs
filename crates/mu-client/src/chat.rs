//! Chat del juego: la caja para escribir (`SEASON3B::CNewUIChatInputBox`), la ventana de mensajes
//! (`SEASON3B::CNewUIChatLogWindow`), la de mensajes del sistema del S6 (`CSystemLogWindow` de
//! MuMain) y lo que llega del servidor (`ReceiveChat`, `ReceiveChatWhisper`,
//! `ReceiveChatWhisperResult`, `ReceiveNotice` y "Welcome to ..." al entrar y al cambiar de mapa).
//! La lógica está en chat_log.rs; aquí, el teclado, el ratón, la red y el dibujo.
//!
//! - **Teclas** (`CNewUIChatInputBox::UpdateKeyEvent`, NewUIChatInputBox.cpp:366-614): Enter abre
//!   la caja y, abierta, manda; Esc la cierra; F2 solo susurros / todo; F3 quita o pone la caja del
//!   nombre para susurrar; F4 (con el fondo) cambia el tamaño; F5 (abierta) pone o quita el fondo;
//!   RePág/AvPág (con el fondo) pasan una pantalla; arriba/abajo, el historial de la caja con el
//!   foco; Tab cambia de caja. Mientras se escribe, el foco es de la caja (`GetFocus()` es su
//!   `edit`): ninguna otra ventana ni tecla rápida recibe teclas (se borran en `PreUpdate`).
//! - **Ratón**: los botones de la barra al soltar (con `SOUND_CLICK01`) y su ayuda encima; la rueda
//!   sobre los mensajes los mueve (y no acerca la cámara); con el fondo, arrastrar la barra de
//!   desplazamiento o la de tamaño; clic derecho en una línea con nombre lo pone para susurrar.
//! - **Dibujo**: en la pantalla de 640×480 (`UiScale`), pegada abajo y centrada como la barra.
//!   Barra del S6 de 281×47 en (0, 382) con sus 10 botones (normal, grupo, gremio, Gens | no
//!   susurros, sistema, chat | fondo, tamaño, transparencia); mensajes desde y = 383 hacia arriba,
//!   cada uno con su fondo del ancho del texto; mensajes del sistema en (4, 84) hacia abajo.
//! - **Sobre la cabeza**: lo que dice alguien (el héroe también) va a su caja `CHAT` de names.rs,
//!   debajo del nombre (`AssignChat` → `CreateChat`, ZzzInterface.cpp:1106-1237).
//! - **Letra**: `g_hFont` a 1024×768 (13 px, 8.125 de la pantalla de 640) con la Noto Sans de
//!   fonts.rs; los anchos se miden con la misma letra ([`TextMetrics`]) para partir las líneas.
//!
//! Lo que no hay todavía (ver el informe): grupo, gremio y Gens (sus mensajes no llegan a nadie),
//! comandos del chat del cliente salvo `/filter`, `/warp`, `/whisper on|off` y el aviso de Icarus
//! de `/move`, gestos al hablar (`CheckChatText`), filtro de palabrotas (`CheckAbuseFilter`),
//! avisos dorados del centro (`CreateNotice`), clic derecho en un jugador para susurrarle y el
//! texto de ayuda del botón de Gens (no está en Text_eng.bmd).

use std::collections::HashMap;

use ab_glyph::Font as _;
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;
use bevy::text::{FontSize, FontSmoothing, FontWeight, LineHeight};
use mu_protocol::game::{ClientMessage, ServerMessage, notice_kind};

use crate::Args;
use crate::chat_log::{ChatInput, ChatLog, InputType, LogEvent, MouseIn, MsgType, Outgoing, SystemLog, WarpName};
use crate::scene::GameScene;
use crate::sound::SoundQueue;

pub struct ChatPlugin;

/// Las teclas del chat (antes que las del minimapa y el resto).
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
            .add_systems(Update, (tick, assign_chat, draw).chain().run_if(in_state(GameScene::Game)));
    }
}

/// Lo que alguien ha dicho y aún no está en su caja sobre la cabeza: (nombre, texto, `Flag`).
#[derive(Resource, Default)]
pub struct PendingSay(Vec<(String, String, u8)>);

/// `AssignChat` (ZzzInterface.cpp:1204-1237): la caja del jugador con ese nombre (el héroe también)
/// en names.rs. Con `Flag` 1 (`#`), si lo encuentra, `AddChat` lo escribe además en el chat.
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

// --- Medidas (pantalla de 640×480) -------------------------------------------------------------

/// `g_hFont` a 1024×768: 13 px, en unidades de la pantalla de 480 de alto (como options.rs).
pub const FONT: f32 = 13.0 * 480.0 / 768.0;
const SMOOTHING: FontSmoothing = FontSmoothing::AntiAliased;
/// `Create(m_pNewUIMng, m_pNewChatLogWindow, 0, 480-51-47)` (NewUISystem.cpp:149).
const INPUT_X: i32 = 0;
const INPUT_Y: i32 = 480 - 51 - 47;
/// `CHATBOX_WIDTH` × `WND_HEIGHT` (la barra del S6).
const INPUT_W: i32 = crate::chat_log::WND_WIDTH;
const INPUT_H: i32 = 47;
const BTN_W: i32 = 27;
const BTN_H: i32 = 26;
/// Los 10 botones: de 27 en 27 con 6 entre grupos (4 | 3 | 3), como chat_input.rcss de MuMain.
const BUTTON_X: [i32; 10] = [0, 27, 54, 81, 114, 141, 168, 201, 228, 255];
const BTN_WHISPER: usize = 4;
const BTN_SYSTEM: usize = 5;
const BTN_CHATLOG: usize = 6;
const BTN_FRAME: usize = 7;
const BTN_SIZE: usize = 8;
const BTN_ALPHA: usize = 9;
/// `GlobalText[1681, 1682, 1683, —, 1684, 1685, 750, 1686, 751, 752]` (`RenderTooltip`,
/// NewUIChatInputBox.cpp:730). El de Gens no está en Text_eng.bmd.
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
/// Las cajas de texto (`SetPosition(m_WndPos.x+72, m_WndPos.y+32)` 176×14 y `+5` 50×14).
const CHAT_FIELD: (i32, i32, i32, i32) = (INPUT_X + 72, INPUT_Y + 32, 176, 14);
const WHISPER_FIELD: (i32, i32, i32, i32) = (INPUT_X + 5, INPUT_Y + 32, 50, 14);
/// `SetTextColor(255, 255, 230, 210)` y `(255, 200, 200, 200)` (ARGB).
const CHAT_COLOR: [u8; 4] = [255, 230, 210, 255];
const WHISPER_COLOR: [u8; 4] = [200, 200, 200, 255];
/// `m_iCaretBlinkTemp % 24 < 12`, un paso por fotograma del original.
const CARET_PERIOD: f32 = 24.0 / 25.0;
/// La ventana de mensajes del sistema de MuMain: (0, 80), la primera línea en (4, 84) y una cada
/// 1.2 del alto de la letra, con el fondo negro a la transparencia de siempre (0.6).
const SYSTEM_X: f32 = 4.0;
const SYSTEM_Y: f32 = 84.0;
const SYSTEM_ROW: f32 = FONT * 1.2;
/// `TICKS_PER_SECOND` del original (`ChatTime--` una vez por tick).
const TICKS: f32 = 25.0;

/// `CheckMouseIn`.
fn inside(m: (i32, i32), x: i32, y: i32, w: i32, h: i32) -> bool {
    m.0 >= x && m.0 < x + w && m.1 >= y && m.1 < y + h
}

// --- Medir texto -------------------------------------------------------------------------------

/// Anchos del texto con la letra de la interfaz (`_GetTextExtentPoint32` del original), en
/// unidades de la pantalla de 640. Sin el archivo, una aproximación de media letra por letra.
#[derive(Resource, Default)]
pub struct TextMetrics {
    font: Option<ab_glyph::FontArc>,
}

impl TextMetrics {
    /// Ancho con `g_hFont`.
    pub fn width(&self, s: &str) -> f32 {
        self.width_at(s, FONT)
    }

    /// Ancho con la letra a `size` unidades (sin el interletraje de GPOS que pone el dibujo).
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

// --- Estado ------------------------------------------------------------------------------------

/// El chat entero: la ventana de mensajes, la del sistema y la caja de escribir.
#[derive(Resource, Default)]
pub struct Chat {
    pub log: ChatLog,
    pub system: SystemLog,
    pub input: ChatInput,
    /// `m_iTooltipType`.
    tooltip: Option<usize>,
    /// Los botones de tamaño y transparencia, apretados encima (`BUTTON_STATE_DOWN`).
    size_down: bool,
    alpha_down: bool,
    caret_clock: f32,
    tick_clock: f32,
    /// `WorldActive` de lo último que se ha dicho con "Welcome to".
    map: Option<u16>,
}

impl Chat {
    /// El ratón (pantalla de 640×480) cae en la caja abierta o en lo que agarra la ventana de
    /// mensajes: un clic ahí no hace caminar.
    pub fn blocks(&self, c: Vec2) -> bool {
        let m = (c.x.floor() as i32, c.y.floor() as i32);
        (self.input.open && inside(m, INPUT_X, INPUT_Y, INPUT_W, INPUT_H)) || self.log.blocks(m)
    }

    /// `g_pChatListBox->AddText`; devuelve si suena el aviso del filtro.
    pub fn add(&mut self, id: &str, text: &str, kind: MsgType, err: MsgType, metrics: &TextMetrics) -> bool {
        self.log.add_text(id, text, kind, err, &|s| metrics.width(s))
    }
}

/// Al salir del juego (o cambiar de mapa, `HideAll`): la caja se cierra (y el fondo con ella).
fn leave_game(mut chat: ResMut<Chat>) {
    chat.input.close();
    chat.log.set_frame(false);
    chat.log.event = LogEvent::None;
    chat.tooltip = None;
}

/// `ChatTime--` y el parpadeo del cursor.
fn tick(time: Res<Time>, mut chat: ResMut<Chat>) {
    let chat = &mut *chat;
    chat.caret_clock += time.delta_secs();
    chat.tick_clock += time.delta_secs() * TICKS;
    while chat.tick_clock >= 1.0 {
        chat.tick_clock -= 1.0;
        chat.input.tick();
    }
}

// --- Red ---------------------------------------------------------------------------------------

/// `ReceiveChat`, `ReceiveChatWhisper`, `ReceiveChatWhisperResult`, `ReceiveNotice` y el "Welcome
/// to" de `ReceiveJoinMapServer` / `ReceiveTeleport`.
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
                // El prefijo dice el canal (WSclient.cpp:1435-1510).
                let (kind, text) = if let Some(t) = message.strip_prefix('~') {
                    (Some(MsgType::Party), t)
                } else if let Some(t) = message.strip_prefix("@@") {
                    (Some(MsgType::Union), t)
                } else if let Some(t) = message.strip_prefix('@') {
                    (Some(MsgType::Guild), t)
                } else if let Some(t) = message.strip_prefix('$') {
                    (Some(MsgType::Gens), t)
                } else if let Some(t) = message.strip_prefix('#') {
                    // Sin GMs: solo la caja sobre la cabeza, larga, que además lo escribe en el chat
                    // (`AssignChat(ID, Text, 1)` → `AddChat` con `Flag` 1, [`assign_chat`]).
                    bubbles.0.push((sender.clone(), t.to_string(), 1));
                    (None, t)
                } else {
                    // `AssignChat` + `TYPE_CHAT_MESSAGE` (sin GMs no hay `TYPE_GM_MESSAGE`).
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
                // En la selección de personaje el original lo pone en su caja de mensajes del
                // servidor (`CUIMng::AddServerMsg`), que aquí no hay.
                notice_kind::BLUE if *scene.get() != GameScene::CharacterSelect => chat.system.add(text, MsgType::System),
                k => info!("aviso del servidor de tipo {k} sin mostrar: {text}"),
            },
            ServerMessage::CharacterInformation(info) => welcome(chat, info.map as u16),
            ServerMessage::MapChanged { is_map_change: true, map, .. } if chat.map != Some(*map) => welcome(chat, *map),
            _ => {}
        }
    }
}

/// "Welcome to <mapa>" en los mensajes del sistema.
fn welcome(chat: &mut Chat, map: u16) {
    chat.map = Some(map);
    if let Some(text) = crate::chat_log::welcome_text(map) {
        chat.system.add(&text, MsgType::System);
    }
}

// --- Teclado -----------------------------------------------------------------------------------

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
) {
    let typed: Vec<Key> = typed.read().filter(|k| k.state == ButtonState::Pressed).map(|k| k.logical_key.clone()).collect();
    let chat = &mut *chat;
    let was_open = chat.input.open;
    // Con la caja de mensajes del sistema (orden 10) las teclas son suyas; con el chat abierto, el
    // foco es de su `edit` y solo él las recibe.
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
    // Lo que se ha usado con la caja cerrada (para que no llegue a nadie más).
    let mut used: Vec<KeyCode> = Vec::new();

    // `UpdateKeyEvent`: la primera que vale y se acaba.
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
        // Sin el Chaos Castle (ni su ventana del tiempo), siempre se abre.
        chat.input.open();
        chat.caret_clock = 0.0;
        used.extend([KeyCode::Enter, KeyCode::NumpadEnter]);
    } else if was_open && enter {
        let hero_name = hero.map_or_else(crate::account::current_account, |h| h.name.clone());
        let warps: Vec<WarpName> = warps
            .map(|w| w.0.iter().map(|m| WarpName { index: m.index, name: m.name.clone(), sub_name: m.sub_name.clone() }).collect())
            .unwrap_or_default();
        let out = chat.input.submit(&hero_name, &warps, &mut chat.log, &mut chat.system, &measure);
        // `ClosingProcess`: el fondo se va con la caja.
        chat.log.set_frame(false);
        for o in out {
            match o {
                Outgoing::Chat(message) => net.send(ClientMessage::PublicChat { name: hero_name.clone(), message }),
                Outgoing::Whisper(target, message) => net.send(ClientMessage::Whisper { target, message }),
                // Un nombre que no está manda −1 como `WORD` (0xFFFF), como el original.
                Outgoing::Warp(index) => move_window.request_warp(&mut net, index as u16),
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

    // Lo que se escribe en la caja con el foco (el `edit` de Windows).
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
        // Nada de lo pulsado mientras se escribe llega al juego (teclas rápidas, habilidades...).
        keys.clear();
    } else {
        for k in used {
            keys.clear_just_pressed(k);
        }
    }
}

// --- Ratón -------------------------------------------------------------------------------------

/// `UpdateMouseEvent` de la caja (capa 6.2) y, si no se ha quedado el ratón, el de la ventana de
/// mensajes (6.1); luego `UpdateScrollPos`.
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
    // Con la caja de mensajes del sistema, el ratón es suyo.
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
            // `MouseWheel = 0`: la cámara no se acerca.
            scroll.delta = Vec2::ZERO;
        }
        if let Some(id) = out.whisper_to {
            chat.input.whisper.set(&id);
        }
    }
    chat.log.update_scroll_pos();
}

/// `CNewUIChatInputBox::UpdateMouseEvent` (NewUIChatInputBox.cpp:231-364). `true` = se lo queda.
fn input_mouse(chat: &mut Chat, cursor: Option<(i32, i32)>, press: bool, held: bool, release: bool, sound: &mut SoundQueue) -> bool {
    let at = |x: i32, y: i32, w: i32, h: i32| cursor.is_some_and(|m| inside(m, x, y, w, h));
    let button = |i: usize| at(BUTTON_X[i], INPUT_Y, BTN_W, BTN_H);
    chat.tooltip = (0..BUTTON_X.len()).find(|&i| button(i));
    // Un clic en una caja de texto le da el foco (es un `edit` de Windows).
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
            // S6: enseña o esconde la ventana de mensajes del sistema.
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
        // `CNewUIButton::Process`: apretado encima = fotograma 1; al soltar encima, vale.
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

// --- Dibujo ------------------------------------------------------------------------------------

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

    /// `LoadImages` de la caja (NewUIChatInputBox.cpp:44-56, con el de Gens) y de la ventana
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

    /// El botón encendido de cada tipo de mensaje (`IMAGE_INPUTBOX_NORMAL_ON + m_iInputMsgType`).
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

/// Una pieza en la pantalla de 640×480.
#[derive(Clone, PartialEq, Debug)]
enum Piece {
    /// `RenderColor` con el color de `glColor4f`.
    Fill { x: f32, y: f32, w: f32, h: f32, rgba: [u8; 4] },
    /// `RenderImage(tex, x, y, ancho, alto, 0, sv)`: el trozo desde (0, sv), sin estirar;
    /// `dark` = `glColor4f(0.7, 0.7, 0.7)`.
    Image { tex: Tex, x: f32, y: f32, w: f32, h: f32, sv: f32, dark: bool },
    /// `RenderText` en (x, y) con su fondo del ancho del texto (más `pad` a cada lado).
    Text { x: f32, y: f32, text: String, color: [u8; 4], bg: Option<[u8; 4]>, bold: bool, pad: f32 },
    /// Una caja de texto: lo de antes y después del cursor, desplazado `scroll`.
    Field { x: f32, y: f32, w: f32, h: f32, before: String, after: String, color: [u8; 4], caret: bool, selected: bool, scroll: f32 },
}

fn img(tex: Tex, x: i32, y: i32, w: i32, h: i32) -> Piece {
    Piece::Image { tex, x: x as f32, y: y as f32, w: w as f32, h: h as f32, sv: 0.0, dark: false }
}

impl Chat {
    /// Lo que se dibuja, de atrás a delante: mensajes del sistema (capa 6.05), ventana de
    /// mensajes (6.1) y caja (6.2).
    fn pieces(&self, metrics: &TextMetrics) -> Vec<Piece> {
        let mut out = Vec::new();
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

    /// `CNewUIChatLogWindow::Render`: fondo, mensajes y marco (NewUIChatLogWindow.cpp:65-237).
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
        // `RenderFrame` y `RenderButtons`.
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
        // `RenderTooltip`: centrado sobre el botón, encima de la barra, con fondo negro a 180.
        if let Some(text) = self.tooltip.and_then(|i| TOOLTIP[i].map(|t| (i, t))) {
            let (i, text) = text;
            let w = metrics.width(text).floor();
            let x = ((INPUT_X + BUTTON_X[i] + 10) as f32 - (w / 2.0).floor()).max(0.0);
            let h = FONT.floor();
            out.push(Piece::Text { x, y: INPUT_Y as f32 - (h + 1.0), text: text.into(), color: [255, 255, 255, 255], bg: Some([0, 0, 0, 180]), bold: false, pad: 3.0 });
        }
        // Las cajas de texto.
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
            // Sin la caja del nombre (F3): su texto apagado y un velo rojizo encima.
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
    // La pantalla de 640×480 centrada y pegada abajo, como la barra; debajo de las ventanas.
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

/// `g_hFont` (o `g_hFontBold`).
pub fn text_font(bold: bool) -> TextFont {
    let mut f = TextFont { font_size: FontSize::Px(FONT), font_smoothing: SMOOTHING, ..default() };
    if bold {
        f.weight = FontWeight::BOLD;
    }
    f
}

/// Rehace el chat cuando cambia algo de lo que se dibuja.
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
                            // El cursor no ocupa sitio: 2 de ancho y el alto de la letra, en gris (200, 200, 200).
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
