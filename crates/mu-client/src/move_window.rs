//! Move window (`M`): `SEASON3B::CNewUIMoveCommandWindow` (NewUIMoveCommandWindow.cpp) and the
//! map change the server replies with (`ReceiveTeleport`, WSclient.cpp:1698).
//!
//! - **List**: `MOVEREQINFO` from `mu_data::warps::load` (the original `movereq_eng.bmd` or, in
//!   the `dist` data, MuMain's `Maps/Warps.json`), in file order.
//! - **Open**: `M` (`CNewUIHotKey::UpdateKeyEvent`, NewUIHotKey.cpp:268-274, with
//!   `SOUND_CLICK01`) or "Move (M)" in the window menu (NewUIWindowMenu.cpp:107-112, silent).
//!   Esc closes it (`UpdateKeyEvent`, with `SOUND_CLICK01`; the Esc chain is in options.rs).
//! - **Drawing and mouse**: `SetPos`, `RenderFrame`, `Render` and `BtnProcess` as is, on the
//!   640×480 screen at (1, 1) (`Create(m_pNewUIMng, 1, 1)`, NewUISystem.cpp:327-329). The columns,
//!   width and row height depend on `WindowWidth` (800, 1024 or 1280; at other resolutions the
//!   original leaves them at zero: here the closest of the three is used).
//! - **Can go** (`SettingCanMoveMap`): level (2/3 for MG, DL and RF except the 400 ones), zen and
//!   not being a murderer (`PK < PVP_MURDERER1`); Icarus needs wings or a flying mount (no
//!   equipment yet: never) and Atlans refuses Uniria (no equipment: always allows). The server is
//!   treated as PvP (`IsNonPvP` false): Vulcanus (42) is a battle map and needs Gens, not implemented yet.
//! - **Click** on an allowed destination (on release): `SendRequestMoveMap(GetMoveCommandKey(), index)`
//!   and the window closes.
//! - **Reply**: `MapChanged`. On a map change the new map is loaded at the given tile (without
//!   the loading screen: the original loads at once with `LoadWorld`) and, when done,
//!   `ClientReadyAfterMapChange` (`SendRequestFinishLoading`) is sent. On the same map the other
//!   players are cleared (`ClearCharacters`), the character is placed and the reply goes out right
//!   away. Players of the new map arrive later with `AddCharactersToScope` (remote.rs).

use std::collections::HashMap;

use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;
use bevy::text::{FontSize, FontSmoothing, FontWeight, LineHeight};
use mu_data::warps::MoveReq;
use mu_protocol::game::{ClientMessage, ServerMessage};

use crate::Args;
use crate::account::Class;
use crate::scene::{GameScene, LoadStep};
use crate::sound::SoundQueue;

pub struct MoveWindowPlugin;

impl Plugin for MoveWindowPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MoveWindow>()
            .init_resource::<MapChangePending>()
            .add_systems(Startup, load_list)
            .add_systems(PreUpdate, net_messages.after(crate::net::NetSystems))
            .add_systems(OnEnter(GameScene::Game), (spawn_root, finish_map_change))
            .add_systems(
                OnExit(GameScene::Game),
                (crate::scene::despawn::<MoveWindowRoot>, |mut w: ResMut<MoveWindow>| w.open = false),
            )
            .add_systems(Update, (update, draw).chain().run_if(in_state(GameScene::Game)));
    }
}

/// `MOVECOMMAND_MAX_RENDER_TEXTLINE`: visible rows.
const MAX_LINES: i32 = 31;
/// `MOVECOMMAND_SCROLLBTN_WIDTH/HEIGHT`, `SCROLLBAR_TOP_WIDTH/HEIGHT` and `SCROLLBAR_MIDDLE_HEIGHT`.
const BTN_W: i32 = 15;
const BTN_H: i32 = 30;
const BAR_W: i32 = 7;
const BAR_TOP_H: i32 = 3;
const BAR_MIDDLE_H: i32 = 15;
/// `m_Pos`.
const POS: (i32, i32) = (1, 1);
/// `PVP_MURDERER1` (_define.h:466).
const PVP_MURDERER1: u8 = 5;
/// The battle map index (`anStrifeIndex`, NewUIMoveCommandWindow.cpp:286): Vulcanus.
const STRIFE_INDEX: i32 = 42;
/// `g_ServerListManager->IsNonPvP()`: our server is treated as normal (PvP).
const NON_PVP: bool = false;
/// `GlobalText[933, 2988, 934, 935, 936, 2987, 1002]` (Text_eng.bmd).
const TITLE: &str = "Warp Command Window";
const HEAD_STRIFE: &str = "Battle Zone";
const HEAD_MAP: &str = "Map";
const HEAD_LEVEL: &str = "Min. Level";
const HEAD_ZEN: &str = "Cost";
const STRIFE_MARK: &str = "(Battle)";
const CLOSE: &str = "Close";
/// `GlobalText[55]` (Icarus), `[37]` (Atlans) and `[2686]` (Vulcanus).
const ICARUS: &str = "Icarus";
const ATLANS: &str = "Atlans";
const VULCANUS: &str = "Vulcanus";

/// `Render` and `RenderFrame` colors.
const WHITE: [u8; 3] = [255, 255, 255];
const TITLE_COLOR: [u8; 3] = [255, 204, 26];
const HEAD_COLOR: [u8; 3] = [127, 178, 255];
/// Cannot go.
const DENIED: [u8; 3] = [164, 39, 17];
/// Level or zen not reached.
const MISSING: [u8; 3] = [255, 51, 26];

// --- Layout ------------------------------------------------------------------------------------

/// `SetPos` measurements for a `WindowWidth` (NewUIMoveCommandWindow.cpp:111-188).
#[derive(Clone, Copy, PartialEq, Debug)]
struct Layout {
    /// `m_MapNameUISize`.
    w: i32,
    h: i32,
    /// `m_iRealFontHeight` (`FontHeight * 640 / WindowWidth + 2`): row height.
    fh: i32,
    /// `m_StrifePos.x`, `m_MapNamePos.x`, `m_ReqLevelPos.x` and `m_ReqZenPos.x`.
    strife_x: i32,
    map_x: i32,
    level_x: i32,
    zen_x: i32,
    /// `g_hFont` (`CreateFont(FontHeight - 1)`, Winmain.cpp:1456-1471) in units of the 640
    /// screen.
    font: f32,
}

impl Layout {
    fn new(window_width: u32) -> Self {
        // The closest resolution of the `switch (WindowWidth)`.
        let ww = [800, 1024, 1280].into_iter().min_by_key(|&c| (c - window_width as i32).abs()).unwrap_or(1024);
        // (width, columns, `FontHeight` of Winmain.cpp:1458-1461).
        let (w, map, level, zen, font_height) = match ww {
            800 => (200, 69, 129, 174, 13),
            1024 => (180, 64, 119, 159, 14),
            _ => (160, 59, 104, 139, 15),
        };
        let fh = font_height * 640 / ww + 2;
        Self {
            w: w + 10,
            h: 60 + fh * MAX_LINES,
            fh,
            strife_x: POS.0 + 20,
            map_x: POS.0 + map,
            level_x: POS.0 + level,
            zen_x: POS.0 + zen,
            font: (font_height - 1) as f32 * 640.0 / ww as f32,
        }
    }

    /// `m_StartMapNamePos`.
    fn rows_x(&self) -> i32 {
        POS.0 + 2
    }
    fn rows_y(&self) -> i32 {
        POS.1 + 38
    }
    /// `m_ScrollBarPos` (with `m_Pos.y` twice, like the original).
    fn bar(&self) -> (i32, i32) {
        (POS.0 + self.w - 14, POS.1 + self.rows_y() - BAR_TOP_H)
    }
    /// `m_ScrollBtnStartPos.x`.
    fn btn_x(&self) -> i32 {
        self.bar().0 - (BTN_W / 2 - BAR_W / 2)
    }
    /// `m_iScrollBarHeightPixel`.
    fn bar_h(&self) -> i32 {
        MAX_LINES * self.fh
    }
    /// The red "Close" bar: `RenderColor(m_StartMapNamePos.x, m_MapNameUISize.y - fh - 6, ...)`
    /// (without `m_Pos.y`); the click checks x = 3 (NewUIMoveCommandWindow.cpp:524, 718).
    fn close_bar(&self) -> (i32, i32, i32, i32) {
        (self.rows_x(), self.h - self.fh - 6, self.w - 5, self.fh)
    }
}

// --- Scroll bar ----------------------------------------------------------------------------------

/// The scroll bar counters of `SetPos` and what `ScrollUp`, `ScrollDown` and
/// `RecursiveCalcScroll` move (NewUIMoveCommandWindow.cpp:156-187, 574-708).
#[derive(Clone, Copy, Default, Debug, PartialEq)]
struct Scroll {
    /// `m_ScrollBtnPos.y`.
    btn_y: i32,
    total_steps: i32,
    remain_steps: i32,
    total_px: i32,
    remain_px: i32,
    /// `m_icurMoveScrBtnPixelperStep`, `m_iMaxMoveScrBtnPixelperStep`, `m_iMinMoveScrBtnPixelperStep`.
    cur: i32,
    max: i32,
    min: i32,
    /// `m_iTotalNumMinMoveScrBtnperStep`.
    num_min: i32,
    /// `m_iAcumMoveMouseScrollPixel`.
    acc: i32,
    /// `m_iMousePosY`.
    mouse_y: i32,
    /// `m_iScrollBtnMouseEvent == MOVECOMMAND_MOUSEBTN_CLICKED`.
    clicked: bool,
    /// `m_bScrollBtnActive`.
    active: bool,
}

impl Scroll {
    /// The scroll bar part of `SetPos` (and `OpenningProcess`, which calls it again).
    fn new(layout: &Layout, rows: usize) -> Self {
        let total_steps = rows as i32 - MAX_LINES;
        let total_px = layout.bar_h() - BTN_H;
        // With 31 destinations or fewer the original divides by zero or gets a negative value: no scrolling.
        let min = if total_steps > 0 { total_px / total_steps } else { 0 };
        let max = if total_steps > 0 { min + 1 } else { 0 };
        let num_max = total_px - total_steps * min;
        Self {
            btn_y: layout.bar().1,
            total_steps,
            remain_steps: total_steps,
            total_px,
            remain_px: total_px,
            cur: max,
            max,
            min,
            num_min: total_steps - num_max,
            acc: 0,
            mouse_y: 0,
            clicked: false,
            // `m_iNumPage = 1 + n / 31`: only enabled with more than one page.
            active: 1 + rows as i32 / MAX_LINES > 1,
        }
    }

    /// `ScrollUp`.
    fn up(&mut self, v: i32) {
        if self.remain_steps < self.total_steps {
            self.acc -= v;
            if -self.acc < self.cur {
                return;
            }
            let mut moved = 0;
            self.recursive(self.acc, &mut moved, false);
            self.btn_y += moved;
            self.acc -= moved;
        }
    }

    /// `ScrollDown`.
    fn down(&mut self, v: i32) {
        if self.remain_steps > 0 {
            self.acc += v;
            if self.acc < self.cur {
                return;
            }
            let mut moved = 0;
            self.recursive(self.acc, &mut moved, true);
            self.acc -= moved;
            self.btn_y += moved;
        }
    }

    /// `RecursiveCalcScroll`, with its quirks (when the steps run out, the moved amount becomes what
    /// is left of the value).
    fn recursive(&mut self, mut value: i32, moved: &mut i32, down: bool) {
        if down {
            if self.remain_steps > 0 {
                self.remain_steps -= 1;
                self.remain_px -= self.cur;
                value -= self.cur;
                *moved += self.cur;
                self.cur = if self.remain_steps > self.num_min { self.max } else { self.min };
                if value >= self.cur {
                    self.recursive(value, moved, down);
                }
            } else {
                *moved = value;
            }
        } else if self.remain_steps < self.total_steps {
            self.remain_steps += 1;
            self.remain_px += self.cur;
            value += self.cur;
            *moved -= self.cur;
            self.cur = if self.remain_steps >= self.num_min { self.max } else { self.min };
            if -value >= self.cur {
                self.recursive(value, moved, down);
            }
        } else {
            *moved = value;
        }
    }

    /// `UpdateScrolling`: the visible rows [start, end).
    fn visible(&self, rows: usize) -> (usize, usize) {
        let start = (self.total_steps - self.remain_steps).max(0) as usize;
        (start.min(rows), (start + MAX_LINES as usize).min(rows))
    }
}

// --- State -------------------------------------------------------------------------------------

/// A destination (`MOVEINFODATA`).
#[derive(Clone, Debug, PartialEq)]
struct Row {
    req: MoveReq,
    can_move: bool,
    strife: bool,
    selected: bool,
}

/// The data list (`CMoveCommandData::GetMoveCommandDatalist`); the chat reads it for
/// `/warp` and `/move` (chat.rs).
#[derive(Resource, Default)]
pub(crate) struct MoveList(pub Vec<MoveReq>);

/// `INTERFACE_MOVEMAP` and what `CNewUIMoveCommandWindow` stores.
#[derive(Resource, Default)]
pub struct MoveWindow {
    open: bool,
    /// Just opened: `OpenningProcess` is pending.
    opening: bool,
    rows: Vec<Row>,
    scroll: Scroll,
    layout: Option<Layout>,
    /// `m_dwMoveCommandKey`: the one from `ReceiveMoveMapChecksum` and then the generated ones.
    key: u32,
}

impl MoveWindow {
    /// `g_pNewUISystem->Toggle(INTERFACE_MOVEMAP)`.
    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.opening = self.open;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// `Hide(INTERFACE_MOVEMAP)`.
    pub fn close(&mut self) {
        self.open = false;
        self.scroll.clicked = false;
    }

    /// The mouse (640×480 screen) is over the window or dragging its bar: a click does not
    /// walk (`UpdateMouseEvent` returns false).
    pub fn blocks(&self, c: Vec2) -> bool {
        let Some(l) = self.layout.filter(|_| self.open) else { return false };
        self.scroll.clicked || inside(c, POS.0, POS.1, l.w, l.h)
    }

    /// `GetMoveCommandKey`: the next key, which is kept as the last one.
    fn next_key(&mut self) -> u32 {
        self.key = mu_protocol::game::map_move_key(self.key);
        self.key
    }

    /// Chat `/warp <map>`: `SendRequestMoveMap(GetMoveCommandKey(), index)` without the window
    /// (NewUIChatInputBox.cpp:518-529). The reply arrives like the window's.
    pub fn request_warp(&mut self, net: &mut crate::net::Net, index: u16) {
        let key = self.next_key();
        info!("/warp: índice {index}");
        net.send(ClientMessage::WarpCommandRequest { key, index });
    }
}

/// `CheckMouseIn` (with integer mouse coordinates, `MouseX`/`MouseY`).
fn inside(c: Vec2, x: i32, y: i32, w: i32, h: i32) -> bool {
    let (mx, my) = (c.x.floor() as i32, c.y.floor() as i32);
    mx >= x && mx < x + w && my >= y && my < y + h
}

/// `ClientReadyAfterMapChange` must be sent once the map finishes loading.
#[derive(Resource, Default)]
pub(crate) struct MapChangePending(bool);

fn load_list(args: Res<Args>, mut commands: Commands) {
    let list = mu_data::warps::load(&args.data).unwrap_or_else(|e| {
        warn!("ventana de mover sin destinos: {e}");
        Vec::new()
    });
    info!("ventana de mover: {} destinos", list.len());
    commands.insert_resource(MoveList(list));
}

/// What `SettingCanMoveMap` reads from the character (`CharacterAttribute`, `CharacterMachine`, `Hero`).
struct HeroState {
    level: i32,
    money: i64,
    class: Class,
    pk: u8,
}


/// Level a destination requires for this class (NewUIMoveCommandWindow.cpp:337-345 and 808-816).
fn required_level(class: Class, level: i32) -> i32 {
    if matches!(class, Class::Gladiator | Class::Lord | Class::Fighter) && level != 400 {
        (level as f32 * 2.0 / 3.0) as i32
    } else {
        level
    }
}

/// `SettingCanMoveMap` of a row (NewUIMoveCommandWindow.cpp:308-405).
fn can_move(req: &MoveReq, strife: bool, hero: &HeroState) -> bool {
    let mut can = false;
    if hero.level >= required_level(hero.class, req.level) && hero.money >= req.zen as i64 && hero.pk < PVP_MURDERER1 {
        can = if req.name == ICARUS {
            // Wings, Dinorant or Fenrir, without Uniria or transformation ring: no equipment, so no.
            false
        } else if req.name == ATLANS {
            // `strncmp(..., GlobalText[37], 8)`: only "Atlans" (8 goes past the end of the word).
            // Not with Uniria or Dinorant; without equipment, yes.
            true
        } else {
            !(NON_PVP && req.name == VULCANUS)
        };
    }
    // Battle map: only with Gens (`m_byGensInfluence`), not implemented yet.
    can && !strife
}

// --- Logic -------------------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn update(
    mut w: ResMut<MoveWindow>,
    list: Option<Res<MoveList>>,
    windows: Query<&Window>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    scroll: Res<AccumulatedMouseScroll>,
    game_ui: Res<crate::options::GameUi>,
    hero: Option<Res<crate::account::Hero>>,
    info: Option<Res<crate::account::HeroInfo>>,
    mut sound: ResMut<SoundQueue>,
    mut net: ResMut<crate::net::Net>,
) {
    // `M` (hotkeys don't arrive while the system menu box or the minimap is open,
    // NewUIHotKey.cpp:134-137).
    if keys.just_pressed(KeyCode::KeyM) && !game_ui.modal() && !game_ui.minimap {
        w.toggle();
        sound.play("SOUND_CLICK01");
    }
    if !w.open {
        return;
    }
    let window = windows.single().ok();
    let l = Layout::new(window.map_or(1024, |w| w.physical_width()));
    let w = &mut *w;
    if w.opening || w.layout != Some(l) {
        // `OpenningProcess`: `SetPos`, `SetStrifeMap` (and `SettingCanMoveMap`, below).
        w.opening = false;
        let list = list.as_ref().map_or(&[][..], |l| &l.0[..]);
        w.rows = list
            .iter()
            .map(|req| Row { req: req.clone(), can_move: false, strife: !NON_PVP && req.index == STRIFE_INDEX, selected: false })
            .collect();
        w.scroll = Scroll::new(&l, w.rows.len());
        w.layout = Some(l);
    }
    let cursor = window.and_then(crate::mainframe::cursor_virtual);
    let at = |x: i32, y: i32, wd: i32, h: i32| cursor.is_some_and(|c| inside(c, x, y, wd, h));
    let mouse_y = cursor.map_or(0, |c| c.y.floor() as i32);
    let (press, release) = (mouse.just_pressed(MouseButton::Left), mouse.just_released(MouseButton::Left));

    // `BtnProcess` (NewUIMoveCommandWindow.cpp:407-532): the scroll bar button first.
    let s = &mut w.scroll;
    if at(l.btn_x(), s.btn_y, BTN_W, BTN_H) && press {
        s.clicked = true;
        s.mouse_y = mouse_y;
        s.acc = 0;
    }
    if release {
        s.clicked = false;
        s.acc = 0;
    }
    if s.clicked && s.cur > 0 {
        let mut v = mouse_y - s.mouse_y;
        if v < 0 {
            if mouse_y <= s.btn_y + BTN_H / 2 {
                if -v > s.total_px - s.remain_px {
                    v = -(s.total_px - s.remain_px);
                }
                s.up(-v);
            }
        } else if v > 0 && mouse_y >= s.btn_y + BTN_H / 2 {
            if v > s.remain_px {
                v = s.remain_px;
            }
            s.down(v);
        }
        s.mouse_y = mouse_y;
    }

    // `SettingCanMoveMap` of the visible rows.
    let hero_state = HeroState {
        level: hero.as_ref().map_or(1, |h| h.level as i32),
        money: info.as_ref().map_or(0, |i| i.0.money as i64),
        class: hero.as_ref().map_or(Class::Knight, |h| h.class),
        pk: info.as_ref().map_or(0, |i| i.0.hero_state),
    };
    let (start, end) = w.scroll.visible(w.rows.len());
    for r in &mut w.rows[start..end] {
        r.can_move = can_move(&r.req, r.strife, &hero_state);
        r.selected = false;
    }

    let mut warp_to = None;
    let mut close = false;
    if at(POS.0, POS.1, l.w, l.h) {
        let s = &mut w.scroll;
        if s.cur > 0 {
            if scroll.delta.y > 0.0 {
                s.up(s.cur);
            } else if scroll.delta.y < 0.0 {
                s.down(s.cur);
            }
        }
        for (k, r) in w.rows[start..end].iter_mut().enumerate() {
            let y = l.rows_y() + l.fh * k as i32;
            if at(l.rows_x(), y, l.w - 22, l.fh) && r.can_move {
                r.selected = true;
                if release {
                    warp_to = Some((r.req.index, r.req.name.clone()));
                    break;
                }
            }
            if release && at(3, l.close_bar().1, l.w - 5, l.fh) {
                close = true;
                break;
            }
        }
    }
    if let Some((index, name)) = warp_to {
        // (The original saves the options first if the destination is on another server: here there is one.)
        let key = w.next_key();
        info!("mover a {name} (índice {index})");
        net.send(ClientMessage::WarpCommandRequest { key, index: index as u16 });
        w.close();
    } else if close {
        w.close();
    }
    // `Update` → `UpdateScrolling`: done by `Scroll::visible` when used.
}

/// `ReceiveMoveMapChecksum`, the zen (`ReceiveGetItem` with `GET_ITEM_ZEN`) and `ReceiveTeleport`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn net_messages(
    mut commands: Commands,
    mut from: MessageReader<crate::net::FromServer>,
    mut w: ResMut<MoveWindow>,
    mut info: Option<ResMut<crate::account::HeroInfo>>,
    scene: Res<State<GameScene>>,
    mut args: ResMut<Args>,
    mut load: ResMut<LoadStep>,
    mut next: ResMut<NextState<GameScene>>,
    mut pending: ResMut<MapChangePending>,
    mut players: Query<&mut crate::character::Player>,
    remotes: Query<(Entity, &crate::shadow::Shadow), With<crate::remote::RemotePlayer>>,
    mut game_ui: ResMut<crate::options::GameUi>,
    mut net: ResMut<crate::net::Net>,
    mut gate: ResMut<crate::gate::GateState>,
    real: Res<Time<Real>>,
) {
    for m in from.read() {
        match m.0 {
            ServerMessage::MapMoveChecksumKey { key } => w.key = key,
            ServerMessage::InventoryMoneyUpdate { money } => {
                if let Some(info) = info.as_mut() {
                    info.0.money = money;
                }
            }
            ServerMessage::MapChanged { is_map_change, map, x, y, rotation } => {
                if *scene.get() != GameScene::Game {
                    continue;
                }
                info!("el servidor nos lleva al mapa {map} ({x}, {y})");
                let facing = crate::character::direction_facing(rotation);
                if !is_map_change {
                    // `Flag` 0: teleport within the same map (`CreateTeleportEnd`).
                    for mut p in &mut players {
                        place(&mut p, x, y, facing);
                    }
                    continue;
                }
                // `ClearCharacters`, `HideAll` and, for another map, `LoadWorld`.
                w.close();
                game_ui.hide_all();
                let mut new_info = info.as_deref().map(|i| i.0).unwrap_or_default();
                (new_info.map, new_info.x, new_info.y, new_info.direction) = (map as u8, x, y, rotation);
                match info.as_mut() {
                    Some(i) => i.0 = new_info,
                    None => commands.insert_resource(crate::account::HeroInfo(new_info)),
                }
                let world = map + 1;
                if world != args.world {
                    crate::scene::change_map(&mut args, &mut load, world);
                    next.set(GameScene::Loading);
                    pending.0 = true;
                } else {
                    for (e, shadow) in &remotes {
                        commands.entity(e).despawn();
                        commands.entity(shadow.1).despawn();
                    }
                    for mut p in &mut players {
                        place(&mut p, x, y, facing);
                    }
                    net.send(ClientMessage::ClientReadyAfterMapChange);
                    gate.finish_loading(real.elapsed_secs_f64());
                }
            }
            _ => {}
        }
    }
}

/// `Hero->PositionX/Y`, `SetPlayerStop` and the angle.
fn place(p: &mut crate::character::Player, x: u8, y: u8, facing: f32) {
    p.pos = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
    p.path.clear();
    p.operate = None;
    p.facing = facing;
}

/// When the new map finishes loading: `SendRequestFinishLoading` (which also records when, for
/// the gates: gate.rs).
fn finish_map_change(
    mut pending: ResMut<MapChangePending>,
    mut net: ResMut<crate::net::Net>,
    mut gate: ResMut<crate::gate::GateState>,
    real: Res<Time<Real>>,
) {
    if std::mem::take(&mut pending.0) {
        net.send(ClientMessage::ClientReadyAfterMapChange);
        gate.finish_loading(real.elapsed_secs_f64());
    }
}

// --- Drawing -----------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Tex {
    BarTop,
    BarMiddle,
    BarBottom,
    BtnOn,
    BtnOff,
}

impl Tex {
    const ALL: [Tex; 5] = [Tex::BarTop, Tex::BarMiddle, Tex::BarBottom, Tex::BtnOn, Tex::BtnOff];

    /// `LoadImages` (NewUIMoveCommandWindow.cpp:908-915).
    fn file(self) -> &'static str {
        match self {
            Tex::BarTop => "newui_scrollbar_up.tga",
            Tex::BarMiddle => "newui_scrollbar_m.tga",
            Tex::BarBottom => "newui_scrollbar_down.tga",
            Tex::BtnOn => "newui_scroll_on.tga",
            Tex::BtnOff => "newui_scroll_off.tga",
        }
    }
}

#[derive(Resource)]
struct MoveTextures(HashMap<Tex, Handle<Image>>);

/// A piece on the 640×480 screen.
#[derive(Clone, PartialEq, Debug)]
enum Piece {
    /// `RenderColor` with the `glColor4f` color.
    Fill { x: i32, y: i32, w: i32, h: i32, rgba: [u8; 4] },
    /// `RenderImage(tex, x, y, width, height)`: the top-left part, unstretched;
    /// `dark` = `glColor4f(0.7, 0.7, 0.7)`.
    Image { tex: Tex, x: i32, y: i32, w: i32, h: i32, dark: bool },
    /// `RenderText(x, y, text, 0, 0, RT3_WRITE_CENTER)`: centered on x, with y at the top.
    Text { cx: i32, y: i32, text: String, color: [u8; 3], bold: bool },
}

fn text(cx: i32, y: i32, s: &str, color: [u8; 3]) -> Piece {
    Piece::Text { cx, y, text: s.to_string(), color, bold: false }
}

impl MoveWindow {
    /// `Render` and `RenderFrame` (NewUIMoveCommandWindow.cpp:710-878).
    fn pieces(&self, hero: &HeroState) -> Vec<Piece> {
        let Some(l) = self.layout.filter(|_| self.open && !self.opening) else { return Vec::new() };
        let mut out = Vec::new();
        // `RenderFrame`.
        out.push(Piece::Fill { x: POS.0, y: POS.1, w: l.w, h: l.h, rgba: [0, 0, 0, 204] });
        let (cx, cy, cw, ch) = l.close_bar();
        out.push(Piece::Fill { x: cx, y: cy, w: cw, h: ch, rgba: [153, 0, 0, 255] });
        let (bx, by) = l.bar();
        out.push(Piece::Image { tex: Tex::BarTop, x: bx, y: by, w: BAR_W, h: BAR_TOP_H, dark: false });
        let middles = (l.bar_h() - BAR_TOP_H * 2) / BAR_MIDDLE_H;
        let remainder = (l.bar_h() - BAR_TOP_H * 2) % BAR_MIDDLE_H;
        let mut last = 0;
        for i in 0..middles {
            last = i;
            out.push(Piece::Image { tex: Tex::BarMiddle, x: bx, y: by + BAR_TOP_H + i * BAR_MIDDLE_H, w: BAR_W, h: BAR_MIDDLE_H, dark: false });
        }
        if remainder > 0 {
            // The original places it over the last whole piece (`icntText`), not after it.
            out.push(Piece::Image { tex: Tex::BarMiddle, x: bx, y: by + BAR_TOP_H + last * BAR_MIDDLE_H, w: BAR_W, h: remainder, dark: false });
        }
        out.push(Piece::Image { tex: Tex::BarBottom, x: bx, y: by + l.bar_h() - BAR_TOP_H, w: BAR_W, h: BAR_TOP_H, dark: false });
        let s = &self.scroll;
        let tex = if s.active { Tex::BtnOn } else { Tex::BtnOff };
        out.push(Piece::Image { tex, x: l.btn_x(), y: s.btn_y, w: BTN_W, h: BTN_H, dark: s.active && s.clicked });
        // Title (`g_hFontBold`) and headers.
        let head_y = POS.1 + 4 + 20;
        out.push(Piece::Text { cx: POS.0 + l.w / 2, y: POS.1 + 4, text: TITLE.into(), color: TITLE_COLOR, bold: true });
        out.push(text(l.strife_x, head_y, HEAD_STRIFE, HEAD_COLOR));
        out.push(text(l.map_x, head_y, HEAD_MAP, HEAD_COLOR));
        out.push(text(l.level_x, head_y, HEAD_LEVEL, HEAD_COLOR));
        out.push(text(l.zen_x, head_y, HEAD_ZEN, HEAD_COLOR));

        // `Render`: the rows.
        let (start, end) = s.visible(self.rows.len());
        for (k, r) in self.rows[start..end].iter().enumerate() {
            let y = l.rows_y() + l.fh * k as i32;
            let level = required_level(hero.class, r.req.level);
            let (level_text, zen_text) = (level.to_string(), r.req.zen.to_string());
            if r.can_move {
                if r.strife {
                    out.push(text(l.strife_x, y, STRIFE_MARK, WHITE));
                }
                out.push(text(l.map_x, y, &r.req.name, WHITE));
                out.push(text(l.level_x, y, &level_text, WHITE));
                out.push(text(l.zen_x, y, &zen_text, WHITE));
                if r.selected {
                    // Over the text: `glColor4f(0.8, 0.8, 0.1, 0.6)`.
                    out.push(Piece::Fill { x: l.rows_x(), y: y - 1, w: l.w - 22, h: l.fh, rgba: [204, 204, 26, 153] });
                }
            } else {
                if r.strife {
                    out.push(text(l.strife_x, y, STRIFE_MARK, DENIED));
                }
                out.push(text(l.map_x, y, &r.req.name, DENIED));
                out.push(text(l.level_x, y, &level_text, if level > hero.level { MISSING } else { DENIED }));
                out.push(text(l.zen_x, y, &zen_text, if r.req.zen as i64 > hero.money { MISSING } else { DENIED }));
            }
        }
        // "Close", at x = width / 2 (without `m_Pos.x`).
        out.push(text(l.w / 2, l.h - l.fh - 5, CLOSE, WHITE));
        out
    }
}

#[derive(Component)]
struct MoveWindowRoot;

fn spawn_root(mut commands: Commands, args: Res<Args>, mut images: ResMut<Assets<Image>>) {
    let textures = Tex::ALL
        .iter()
        .filter_map(|&t| crate::scene::interface_image(&args, &mut images, t.file()).map(|(h, _)| (t, h)))
        .collect();
    commands.insert_resource(MoveTextures(textures));
    // The 640×480 screen centered and bottom-aligned, like the bar and the other windows.
    commands.spawn((
        Name::new("Ventana de mover"),
        MoveWindowRoot,
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(0.0),
            left: Val::Percent(50.0),
            margin: UiRect::left(Val::Px(-320.0)),
            width: Val::Px(640.0),
            height: Val::Px(480.0),
            ..default()
        },
        GlobalZIndex(1),
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

/// Width of the box a text is centered in (enough for any 31-letter name).
const TEXT_BOX: f32 = 300.0;

/// Rebuilds the window when something it draws changes.
#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    w: Res<MoveWindow>,
    hero: Option<Res<crate::account::Hero>>,
    info: Option<Res<crate::account::HeroInfo>>,
    textures: Option<Res<MoveTextures>>,
    root: Query<Entity, With<MoveWindowRoot>>,
    mut last: Local<(Option<Entity>, Vec<Piece>)>,
) {
    let (Ok(root), Some(textures)) = (root.single(), textures) else { return };
    let hero_state = HeroState {
        level: hero.as_ref().map_or(1, |h| h.level as i32),
        money: info.as_ref().map_or(0, |i| i.0.money as i64),
        class: hero.as_ref().map_or(Class::Knight, |h| h.class),
        pk: 0,
    };
    let pieces = w.pieces(&hero_state);
    if last.0 == Some(root) && last.1 == pieces {
        return;
    }
    let font = w.layout.map_or(13.0 * 640.0 / 1024.0, |l| l.font);
    commands.entity(root).despawn_children();
    commands.entity(root).with_children(|p| {
        for piece in &pieces {
            match piece {
                Piece::Fill { x, y, w, h, rgba: [r, g, b, a] } => {
                    p.spawn((absolute(*x as f32, *y as f32, *w as f32, *h as f32), BackgroundColor(Color::srgba_u8(*r, *g, *b, *a))));
                }
                Piece::Image { tex, x, y, w, h, dark } => {
                    let Some(handle) = textures.0.get(tex) else { continue };
                    let mut image = ImageNode::new(handle.clone());
                    image.rect = Some(Rect::new(0.0, 0.0, *w as f32, *h as f32));
                    if *dark {
                        image.color = Color::srgb(0.7, 0.7, 0.7);
                    }
                    p.spawn((absolute(*x as f32, *y as f32, *w as f32, *h as f32), image));
                }
                Piece::Text { cx, y, text, color, bold } => {
                    let mut node = absolute(*cx as f32 - TEXT_BOX / 2.0, *y as f32, TEXT_BOX, font);
                    node.justify_content = JustifyContent::Center;
                    let mut text_font = TextFont { font_size: FontSize::Px(font), font_smoothing: FontSmoothing::AntiAliased, ..default() };
                    if *bold {
                        text_font.weight = FontWeight::BOLD;
                    }
                    p.spawn(node).with_children(|c| {
                        c.spawn((
                            Text::new(text.clone()),
                            text_font,
                            LineHeight::Px(font),
                            TextColor(Color::srgb_u8(color[0], color[1], color[2])),
                            TextLayout::no_wrap(),
                        ));
                    });
                }
            }
        }
    });

    *last = (Some(root), pieces);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(name: &str, index: i32, level: i32, zen: i32) -> MoveReq {
        MoveReq { index, name: name.into(), sub_name: name.into(), level, max_level: 400, zen, gate: 0 }
    }

    #[test]
    fn medidas_como_set_pos() {
        let l = Layout::new(1024);
        // 14 * 640 / 1024 + 2 = 10; 60 + 31 × 10.
        assert_eq!((l.w, l.h, l.fh), (190, 370, 10));
        assert_eq!((l.strife_x, l.map_x, l.level_x, l.zen_x), (21, 65, 120, 160));
        assert_eq!(l.bar(), (177, 37));
        assert_eq!(l.btn_x(), 173);
        let l = Layout::new(800);
        assert_eq!((l.w, l.h, l.fh), (210, 432, 12));
        let l = Layout::new(1280);
        assert_eq!((l.w, l.h, l.fh), (170, 339, 9));
        assert_eq!(Layout::new(1920), l, "la más cercana: 1280");
        assert_eq!(Layout::new(640), Layout::new(800));
    }

    #[test]
    fn barra_con_37_destinos() {
        let l = Layout::new(1024);
        let mut s = Scroll::new(&l, 37);
        // 6 steps in 280 pixels: 46 or 47 per step, 4 of 47 and 2 of 46.
        assert_eq!((s.total_steps, s.total_px, s.min, s.max, s.num_min, s.cur), (6, 280, 46, 47, 2, 47));
        assert!(s.active);
        assert_eq!(s.visible(37), (0, 31));
        // The wheel moves down one step at a time; at the very top it does not go up.
        s.up(s.cur);
        assert_eq!(s.visible(37), (0, 31));
        let mut ys = vec![s.btn_y];
        for _ in 0..8 {
            let cur = s.cur;
            s.down(cur);
            ys.push(s.btn_y);
        }
        assert_eq!(s.visible(37), (6, 37));
        assert_eq!(ys, [37, 84, 131, 178, 225, 271, 317, 317, 317]);
        assert_eq!(s.btn_y - 37, 280, "el botón llega abajo del todo");
        for _ in 0..8 {
            let cur = s.cur;
            s.up(cur);
        }
        assert_eq!((s.visible(37), s.btn_y), ((0, 31), 37));
        // With 31 or fewer, no bar (the original would divide by zero).
        let s = Scroll::new(&l, 20);
        assert_eq!((s.cur, s.active, s.visible(20)), (0, false, (0, 20)));
    }

    #[test]
    fn se_puede_ir() {
        let dk = HeroState { level: 50, money: 5000, class: Class::Knight, pk: 0 };
        assert!(can_move(&req("Lorencia", 2, 10, 2000), false, &dk));
        assert!(!can_move(&req("Lorencia", 2, 10, 6000), false, &dk), "sin zen");
        assert!(!can_move(&req("Atlans", 11, 70, 4000), false, &dk), "sin nivel");
        assert!(!can_move(&req("Icarus", 23, 10, 10), false, &dk), "sin alas");
        assert!(!can_move(&req("Vulcanus", 42, 30, 1000), true, &dk), "batalla sin Gens");
        let dl = HeroState { level: 47, class: Class::Lord, ..dk };
        assert!(can_move(&req("Atlans", 11, 70, 4000), false, &dl), "DL: 2/3 de 70 = 46");
        assert_eq!(required_level(Class::Gladiator, 400), 400);
        let pk = HeroState { pk: PVP_MURDERER1, ..dk };
        assert!(!can_move(&req("Lorencia", 2, 10, 2000), false, &pk), "asesino");
    }
}
