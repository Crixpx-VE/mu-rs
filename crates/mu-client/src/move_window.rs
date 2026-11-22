//! Ventana de mover (`M`): `SEASON3B::CNewUIMoveCommandWindow` (NewUIMoveCommandWindow.cpp) y el
//! cambio de mapa que le contesta el servidor (`ReceiveTeleport`, WSclient.cpp:1698).
//!
//! - **Lista**: `MOVEREQINFO` de `mu_data::warps::load` (el `movereq_eng.bmd` del original o, en
//!   los datos de `dist`, el `Maps/Warps.json` de MuMain), en el orden del archivo.
//! - **Abrir**: `M` (`CNewUIHotKey::UpdateKeyEvent`, NewUIHotKey.cpp:268-274, con
//!   `SOUND_CLICK01`) o "Move (M)" del menú de ventanas (NewUIWindowMenu.cpp:107-112, sin sonido).
//!   Esc la cierra (`UpdateKeyEvent`, con `SOUND_CLICK01`; la cadena de Esc está en options.rs).
//! - **Dibujo y ratón**: `SetPos`, `RenderFrame`, `Render` y `BtnProcess` tal cual, en la pantalla
//!   de 640×480 en (1, 1) (`Create(m_pNewUIMng, 1, 1)`, NewUISystem.cpp:327-329). Las columnas, el
//!   ancho y el alto de las filas dependen de `WindowWidth` (800, 1024 o 1280; con otra resolución
//!   el original las deja a cero: aquí se usa la más cercana de las tres).
//! - **Se puede ir** (`SettingCanMoveMap`): nivel (2/3 para MG, DL y RF salvo los de 400), zen y no
//!   ser asesino (`PK < PVP_MURDERER1`); Icarus pide alas o montura voladora (sin equipo todavía:
//!   nunca) y Atlans no deja con Uniria (sin equipo: siempre deja). El servidor se toma como PvP
//!   (`IsNonPvP` falso): Vulcanus (42) es mapa de batalla y pide Gens, que aún no hay.
//! - **Clic** en un destino permitido (al soltar): `SendRequestMoveMap(GetMoveCommandKey(), index)`
//!   y se cierra.
//! - **Respuesta**: `MapChanged`. Si cambia de mapa se carga el nuevo en la casilla que dice (sin
//!   la pantalla de carga: el original carga de golpe con `LoadWorld`) y, al terminar,
//!   `ClientReadyAfterMapChange` (`SendRequestFinishLoading`). En el mismo mapa se borran los demás
//!   jugadores (`ClearCharacters`), se coloca al personaje y se contesta enseguida. Los jugadores
//!   del mapa nuevo llegan después con `AddCharactersToScope` (remote.rs).

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

/// `MOVECOMMAND_MAX_RENDER_TEXTLINE`: filas a la vista.
const MAX_LINES: i32 = 31;
/// `MOVECOMMAND_SCROLLBTN_WIDTH/HEIGHT`, `SCROLLBAR_TOP_WIDTH/HEIGHT` y `SCROLLBAR_MIDDLE_HEIGHT`.
const BTN_W: i32 = 15;
const BTN_H: i32 = 30;
const BAR_W: i32 = 7;
const BAR_TOP_H: i32 = 3;
const BAR_MIDDLE_H: i32 = 15;
/// `m_Pos`.
const POS: (i32, i32) = (1, 1);
/// `PVP_MURDERER1` (_define.h:466).
const PVP_MURDERER1: u8 = 5;
/// El índice del mapa de batalla (`anStrifeIndex`, NewUIMoveCommandWindow.cpp:286): Vulcanus.
const STRIFE_INDEX: i32 = 42;
/// `g_ServerListManager->IsNonPvP()`: nuestro servidor se toma como normal (PvP).
const NON_PVP: bool = false;
/// `GlobalText[933, 2988, 934, 935, 936, 2987, 1002]` (Text_eng.bmd).
const TITLE: &str = "Warp Command Window";
const HEAD_STRIFE: &str = "Battle Zone";
const HEAD_MAP: &str = "Map";
const HEAD_LEVEL: &str = "Min. Level";
const HEAD_ZEN: &str = "Cost";
const STRIFE_MARK: &str = "(Battle)";
const CLOSE: &str = "Close";
/// `GlobalText[55]` (Icarus), `[37]` (Atlans) y `[2686]` (Vulcanus).
const ICARUS: &str = "Icarus";
const ATLANS: &str = "Atlans";
const VULCANUS: &str = "Vulcanus";

/// Colores de `Render` y `RenderFrame`.
const WHITE: [u8; 3] = [255, 255, 255];
const TITLE_COLOR: [u8; 3] = [255, 204, 26];
const HEAD_COLOR: [u8; 3] = [127, 178, 255];
/// No se puede ir.
const DENIED: [u8; 3] = [164, 39, 17];
/// No llega a ese nivel o a ese zen.
const MISSING: [u8; 3] = [255, 51, 26];

// --- Medidas -----------------------------------------------------------------------------------

/// Medidas de `SetPos` para un `WindowWidth` (NewUIMoveCommandWindow.cpp:111-188).
#[derive(Clone, Copy, PartialEq, Debug)]
struct Layout {
    /// `m_MapNameUISize`.
    w: i32,
    h: i32,
    /// `m_iRealFontHeight` (`FontHeight * 640 / WindowWidth + 2`): alto de una fila.
    fh: i32,
    /// `m_StrifePos.x`, `m_MapNamePos.x`, `m_ReqLevelPos.x` y `m_ReqZenPos.x`.
    strife_x: i32,
    map_x: i32,
    level_x: i32,
    zen_x: i32,
    /// `g_hFont` (`CreateFont(FontHeight - 1)`, Winmain.cpp:1456-1471) en unidades de la pantalla
    /// de 640.
    font: f32,
}

impl Layout {
    fn new(window_width: u32) -> Self {
        // La resolución más cercana de las del `switch (WindowWidth)`.
        let ww = [800, 1024, 1280].into_iter().min_by_key(|&c| (c - window_width as i32).abs()).unwrap_or(1024);
        // (ancho, columnas, `FontHeight` de Winmain.cpp:1458-1461).
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
    /// `m_ScrollBarPos` (con `m_Pos.y` dos veces, como el original).
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
    /// La barra roja de "Close": `RenderColor(m_StartMapNamePos.x, m_MapNameUISize.y - fh - 6, ...)`
    /// (sin `m_Pos.y`); el clic mira x = 3 (NewUIMoveCommandWindow.cpp:524, 718).
    fn close_bar(&self) -> (i32, i32, i32, i32) {
        (self.rows_x(), self.h - self.fh - 6, self.w - 5, self.fh)
    }
}

// --- Barra de desplazamiento ---------------------------------------------------------------------

/// Los contadores de la barra de `SetPos` y lo que mueven `ScrollUp`, `ScrollDown` y
/// `RecursiveCalcScroll` (NewUIMoveCommandWindow.cpp:156-187, 574-708).
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
    /// La parte de la barra de `SetPos` (y `OpenningProcess`, que vuelve a llamarla).
    fn new(layout: &Layout, rows: usize) -> Self {
        let total_steps = rows as i32 - MAX_LINES;
        let total_px = layout.bar_h() - BTN_H;
        // Con 31 destinos o menos el original divide por cero o le sale negativo: sin desplazamiento.
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
            // `m_iNumPage = 1 + n / 31`: solo se enciende con más de una página.
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

    /// `RecursiveCalcScroll`, con sus rarezas (al acabarse los pasos, lo movido pasa a ser lo que
    /// queda del valor).
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

    /// `UpdateScrolling`: las filas [inicio, fin) a la vista.
    fn visible(&self, rows: usize) -> (usize, usize) {
        let start = (self.total_steps - self.remain_steps).max(0) as usize;
        (start.min(rows), (start + MAX_LINES as usize).min(rows))
    }
}

// --- Estado ------------------------------------------------------------------------------------

/// Un destino (`MOVEINFODATA`).
#[derive(Clone, Debug, PartialEq)]
struct Row {
    req: MoveReq,
    can_move: bool,
    strife: bool,
    selected: bool,
}

/// La lista de los datos (`CMoveCommandData::GetMoveCommandDatalist`); el chat la mira para
/// `/warp` y `/move` (chat.rs).
#[derive(Resource, Default)]
pub(crate) struct MoveList(pub Vec<MoveReq>);

/// `INTERFACE_MOVEMAP` y lo que guarda `CNewUIMoveCommandWindow`.
#[derive(Resource, Default)]
pub struct MoveWindow {
    open: bool,
    /// Se acaba de abrir: falta `OpenningProcess`.
    opening: bool,
    rows: Vec<Row>,
    scroll: Scroll,
    layout: Option<Layout>,
    /// `m_dwMoveCommandKey`: la de `ReceiveMoveMapChecksum` y luego las que se van generando.
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

    /// El ratón (pantalla de 640×480) cae en la ventana o arrastra su barra: un clic no hace
    /// caminar (`UpdateMouseEvent` devuelve falso).
    pub fn blocks(&self, c: Vec2) -> bool {
        let Some(l) = self.layout.filter(|_| self.open) else { return false };
        self.scroll.clicked || inside(c, POS.0, POS.1, l.w, l.h)
    }

    /// `GetMoveCommandKey`: la clave siguiente, que se queda como la última.
    fn next_key(&mut self) -> u32 {
        self.key = mu_protocol::game::map_move_key(self.key);
        self.key
    }

    /// `/warp <mapa>` del chat: `SendRequestMoveMap(GetMoveCommandKey(), índice)` sin la ventana
    /// (NewUIChatInputBox.cpp:518-529). La respuesta llega como la de la ventana.
    pub fn request_warp(&mut self, net: &mut crate::net::Net, index: u16) {
        let key = self.next_key();
        info!("/warp: índice {index}");
        net.send(ClientMessage::WarpCommandRequest { key, index });
    }
}

/// `CheckMouseIn` (con el ratón en enteros, `MouseX`/`MouseY`).
fn inside(c: Vec2, x: i32, y: i32, w: i32, h: i32) -> bool {
    let (mx, my) = (c.x.floor() as i32, c.y.floor() as i32);
    mx >= x && mx < x + w && my >= y && my < y + h
}

/// Hay que mandar `ClientReadyAfterMapChange` cuando acabe de cargar el mapa.
#[derive(Resource, Default)]
struct MapChangePending(bool);

fn load_list(args: Res<Args>, mut commands: Commands) {
    let list = mu_data::warps::load(&args.data).unwrap_or_else(|e| {
        warn!("ventana de mover sin destinos: {e}");
        Vec::new()
    });
    info!("ventana de mover: {} destinos", list.len());
    commands.insert_resource(MoveList(list));
}

/// Lo que mira `SettingCanMoveMap` del personaje (`CharacterAttribute`, `CharacterMachine`, `Hero`).
struct HeroState {
    level: i32,
    money: i64,
    class: Class,
    pk: u8,
}


/// Nivel que pide un destino a esta clase (NewUIMoveCommandWindow.cpp:337-345 y 808-816).
fn required_level(class: Class, level: i32) -> i32 {
    if matches!(class, Class::Gladiator | Class::Lord | Class::Fighter) && level != 400 {
        (level as f32 * 2.0 / 3.0) as i32
    } else {
        level
    }
}

/// `SettingCanMoveMap` de una fila (NewUIMoveCommandWindow.cpp:308-405).
fn can_move(req: &MoveReq, strife: bool, hero: &HeroState) -> bool {
    let mut can = false;
    if hero.level >= required_level(hero.class, req.level) && hero.money >= req.zen as i64 && hero.pk < PVP_MURDERER1 {
        can = if req.name == ICARUS {
            // Alas, Dinorant o Fenrir, sin Uniria ni anillo de transformación: sin equipo, no.
            false
        } else if req.name == ATLANS {
            // `strncmp(..., GlobalText[37], 8)`: solo "Atlans" (8 pasa del final de la palabra).
            // Con Uniria o Dinorant no; sin equipo, sí.
            true
        } else {
            !(NON_PVP && req.name == VULCANUS)
        };
    }
    // Mapa de batalla: solo con Gens (`m_byGensInfluence`), que aún no hay.
    can && !strife
}

// --- Lógica ------------------------------------------------------------------------------------

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
    // `M` (con la caja del menú de sistema o el minimapa abiertos las teclas rápidas no llegan,
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
        // `OpenningProcess`: `SetPos`, `SetStrifeMap` (y `SettingCanMoveMap`, más abajo).
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

    // `BtnProcess` (NewUIMoveCommandWindow.cpp:407-532): primero el botón de la barra.
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

    // `SettingCanMoveMap` de las filas a la vista.
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
        // (El original guarda las opciones antes si el destino es de otro servidor: aquí hay uno.)
        let key = w.next_key();
        info!("mover a {name} (índice {index})");
        net.send(ClientMessage::WarpCommandRequest { key, index: index as u16 });
        w.close();
    } else if close {
        w.close();
    }
    // `Update` → `UpdateScrolling`: lo hace `Scroll::visible` al usarlo.
}

/// `ReceiveMoveMapChecksum`, el zen (`ReceiveGetItem` con `GET_ITEM_ZEN`) y `ReceiveTeleport`.
#[allow(clippy::too_many_arguments)]
fn net_messages(
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
                    // `Flag` 0: teletransporte en el mismo mapa (`CreateTeleportEnd`).
                    for mut p in &mut players {
                        place(&mut p, x, y, facing);
                    }
                    continue;
                }
                // `ClearCharacters`, `HideAll` y, si es otro mapa, `LoadWorld`.
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

/// `Hero->PositionX/Y`, `SetPlayerStop` y el ángulo.
fn place(p: &mut crate::character::Player, x: u8, y: u8, facing: f32) {
    p.pos = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
    p.path.clear();
    p.operate = None;
    p.facing = facing;
}

/// Al terminar de cargar el mapa nuevo: `SendRequestFinishLoading` (que además apunta cuándo, para
/// las puertas: gate.rs).
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

// --- Dibujo ------------------------------------------------------------------------------------

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

/// Una pieza en la pantalla de 640×480.
#[derive(Clone, PartialEq, Debug)]
enum Piece {
    /// `RenderColor` con el color de `glColor4f`.
    Fill { x: i32, y: i32, w: i32, h: i32, rgba: [u8; 4] },
    /// `RenderImage(tex, x, y, ancho, alto)`: el trozo de arriba a la izquierda, sin estirar;
    /// `dark` = `glColor4f(0.7, 0.7, 0.7)`.
    Image { tex: Tex, x: i32, y: i32, w: i32, h: i32, dark: bool },
    /// `RenderText(x, y, texto, 0, 0, RT3_WRITE_CENTER)`: centrado en x, con y arriba.
    Text { cx: i32, y: i32, text: String, color: [u8; 3], bold: bool },
}

fn text(cx: i32, y: i32, s: &str, color: [u8; 3]) -> Piece {
    Piece::Text { cx, y, text: s.to_string(), color, bold: false }
}

impl MoveWindow {
    /// `Render` y `RenderFrame` (NewUIMoveCommandWindow.cpp:710-878).
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
            // El original lo pone sobre el último trozo entero (`icntText`), no después.
            out.push(Piece::Image { tex: Tex::BarMiddle, x: bx, y: by + BAR_TOP_H + last * BAR_MIDDLE_H, w: BAR_W, h: remainder, dark: false });
        }
        out.push(Piece::Image { tex: Tex::BarBottom, x: bx, y: by + l.bar_h() - BAR_TOP_H, w: BAR_W, h: BAR_TOP_H, dark: false });
        let s = &self.scroll;
        let tex = if s.active { Tex::BtnOn } else { Tex::BtnOff };
        out.push(Piece::Image { tex, x: l.btn_x(), y: s.btn_y, w: BTN_W, h: BTN_H, dark: s.active && s.clicked });
        // Título (`g_hFontBold`) y cabeceras.
        let head_y = POS.1 + 4 + 20;
        out.push(Piece::Text { cx: POS.0 + l.w / 2, y: POS.1 + 4, text: TITLE.into(), color: TITLE_COLOR, bold: true });
        out.push(text(l.strife_x, head_y, HEAD_STRIFE, HEAD_COLOR));
        out.push(text(l.map_x, head_y, HEAD_MAP, HEAD_COLOR));
        out.push(text(l.level_x, head_y, HEAD_LEVEL, HEAD_COLOR));
        out.push(text(l.zen_x, head_y, HEAD_ZEN, HEAD_COLOR));

        // `Render`: las filas.
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
                    // Encima del texto: `glColor4f(0.8, 0.8, 0.1, 0.6)`.
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
        // "Close", en x = ancho / 2 (sin `m_Pos.x`).
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
    // La pantalla de 640×480 centrada y pegada abajo, como la barra y las demás ventanas.
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

/// Ancho de la caja donde se centra un texto (sobra para cualquier nombre de 31 letras).
const TEXT_BOX: f32 = 300.0;

/// Rehace la ventana cuando cambia algo de lo que se dibuja.
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
        // 6 pasos en 280 píxeles: 46 o 47 por paso, 4 de 47 y 2 de 46.
        assert_eq!((s.total_steps, s.total_px, s.min, s.max, s.num_min, s.cur), (6, 280, 46, 47, 2, 47));
        assert!(s.active);
        assert_eq!(s.visible(37), (0, 31));
        // La rueda baja de un paso cada vez; arriba del todo no sube.
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
        // Con 31 o menos, sin barra (el original dividiría por cero).
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
