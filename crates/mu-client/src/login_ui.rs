//! Interfaz 2D de la pantalla de login, como la del Main 5.2 (S6):
//!
//! - logo MU con su brillo y las líneas de copyright y versión (`NewRenderLogInScene`, ZzzScene.cpp);
//! - barra de abajo con los botones Menu y Credit (`CLoginMainWin`);
//! - selección de servidor: grupos y servidores con su barra de carga (`CServerSelWin`);
//! - ventana de login: cuenta, contraseña en asteriscos, OK y Cancel (`CLoginWin`);
//! - caja de mensajes (`CMsgWin`) y menú de sistema de Esc (`CSysMenuWin`);
//! - el gestor de ventanas (`CUIMng`): ventana activa, orden de delante a atrás y botones (`CButton`).
//!
//! **Coordenadas.** El logo y las líneas de abajo van en la pantalla de 640×480 estirada a la
//! ventana. Las ventanas del original, en cambio, van en píxeles reales de la ventana, sin escalar
//! (se centran con el ancho y el alto de la pantalla). Aquí se toman los píxeles de una ventana de
//! referencia de 1024×768 ([`REF_HEIGHT`]): a 1024×768 sale píxel a píxel como el original, y en
//! las demás resoluciones se escala igual que el resto de la interfaz (1 px de Bevy UI = 1 unidad
//! de la pantalla virtual de 480 de alto, ver mainframe.rs). Los textos usan las mismas
//! conversiones del original (`int(x / g_fScreenRate_x)`).
//!
//! **Sin servidor todavía:** la lista de servidores es un solo grupo, "mu-rs", con un servidor
//! (Non-PvP, 0 % de carga); al elegirlo se abre la ventana de login, que acepta cualquier cuenta y
//! contraseña no vacías y pasa a la selección de personaje. El último usuario se guarda en
//! `mu-launcher.txt` (`usuario=`).
//!
//! Lógica en `Update` (los botones reaccionan al soltar el ratón, como `CButton::Update`); el
//! fundido del logo, a 25 ticks por segundo e interpolado.

use std::collections::HashMap;

use bevy::asset::embedded_asset;
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, RenderPipelineDescriptor,
};
use bevy::shader::ShaderRef;
use bevy::text::{FontSize, FontSmoothing, LineHeight};
use bevy::ui::UiScale;
use bevy::ui_render::prelude::{MaterialNode, UiMaterial, UiMaterialKey, UiMaterialPlugin};

use crate::Args;
use crate::scene::{GameScene, LoadStep};
use crate::sound::SoundQueue;
use crate::terrain::to_bevy_image;

pub struct LoginUiPlugin;

impl Plugin for LoginUiPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "login_logo.wesl");
        app.add_plugins(UiMaterialPlugin::<LogoMaterial>::default())
            .add_systems(OnEnter(GameScene::Login), spawn_login_ui)
            .add_systems(
                OnExit(GameScene::Login),
                (crate::scene::despawn::<LoginUiRoot>, |mut c: Commands| {
                    c.remove_resource::<LoginUi>();
                    c.remove_resource::<LoginTextures>();
                }),
            )
            // Tab es de las cajas de texto: que no llegue al cambio de cámara (camera.rs).
            .add_systems(PreUpdate, keep_tab.after(bevy::input::InputSystems).run_if(in_state(GameScene::Login)))
            .add_systems(FixedUpdate, logo_tick.run_if(in_state(GameScene::Login)))
            .add_systems(Update, (update_ui, draw_windows, draw_logo).chain().run_if(in_state(GameScene::Login)));
    }
}

// --- Medidas ---------------------------------------------------------------------------------------

/// Alto de la ventana de referencia para los píxeles de las ventanas (1024×768).
const REF_HEIGHT: f32 = 768.0;
/// Unidades de la interfaz (pantalla virtual de 480 de alto) por píxel del original.
const K: f32 = 480.0 / REF_HEIGHT;
/// `g_hFixFont`: alto 13 − 1 = 12 px, en cualquier resolución (Winmain.cpp:1464-1474).
const FIX_FONT: f32 = 12.0;
/// `g_hFont` a 1024×768: `FontHeight` 14 − 1 = 13 px (Winmain.cpp:1456-1471).
const FONT: f32 = 13.0;
/// El original pinta las letras sin suavizar (`NONANTIALIASED_QUALITY`).
const SMOOTHING: FontSmoothing = FontSmoothing::None;

/// `CServerSelWin` (ServerSelWin.h:26-29, ServerSelWin.cpp:19-22).
const GROUP_W: i32 = 108;
const GROUP_H: i32 = 26;
const SERVER_W: i32 = 193;
const SERVER_H: i32 = 26;
const SSW_GAP_W: i32 = 28;
const SSW_GAP_H: i32 = 5;
/// Barra de carga dentro del botón del servidor (`SSW_GB_POS_X/Y`) y su tamaño.
const GAUGE_X: i32 = 16;
const GAUGE_Y: i32 = 19;
const GAUGE_W: i32 = 160;
const GAUGE_H: i32 = 4;
/// Alto de la caja de descripción (`CWinEx` con `server_ex01/02/03` y 10 líneas: 6 + 6 + 4×10).
/// El original la cuenta para colocar los botones, pero no la llega a dibujar.
const DESC_H: i32 = 52;
const SSW_SERVER_MAX: i32 = 16;
/// `(108 + 28) × 2 + 193` × `26 × 16 + 5 × 2 + 26 + 52` (ServerSelWin.cpp:87).
const SSW_W: i32 = (GROUP_W + SSW_GAP_W) * 2 + SERVER_W;
const SSW_H: i32 = SERVER_H * SSW_SERVER_MAX + SSW_GAP_H * 2 + GROUP_H + DESC_H;
const SSW_LEFT_MAX: usize = 10;
const SSW_RIGHT_MAX: usize = 10;

/// `CLoginWin` (LoginWin.cpp:46).
const LOGIN_W: i32 = 329;
const LOGIN_H: i32 = 245;
/// `CMsgWin` (MsgWin.cpp:44).
const MSG_W: i32 = 352;
const MSG_H: i32 = 113;
/// `CSysMenuWin` en el login: 213 de ancho y `SetLine(6)`: 64 + 43 + 8 × 6 (SysMenuWin.cpp:41-75).
const SYS_W: i32 = 213;
const SYS_H: i32 = 64 + 43 + 8 * 6;
/// `CLoginMainWin`: botones de 54×30.
const BAR_BTN_W: i32 = 54;
const BAR_BTN_H: i32 = 30;

/// Máximos de las cajas de texto (`MAX_ID_SIZE`, `MAX_PASSWORD_SIZE`, _define.h).
const MAX_ID: usize = 10;
const MAX_PASSWORD: usize = 20;
/// Tamaño de la caja de texto: `Init(g_hWnd, 140, 14, ...)`, en la pantalla de 640×480.
const INPUT_W: f32 = 140.0;
const INPUT_H: f32 = 14.0;
/// El cursor de texto parpadea con `m_iCaretBlinkTemp % 24 < 12`, un paso por fotograma del original.
const CARET_PERIOD: f32 = 24.0 / 25.0;

/// Colores del original (UIBaseDef.h).
const WHITE: [u8; 3] = [255, 255, 255];
const BR_GRAY: [u8; 3] = [226, 226, 226];
const YELLOW: [u8; 3] = [255, 255, 121];
const BR_YELLOW: [u8; 3] = [255, 238, 193];
const ORANGE: [u8; 3] = [255, 180, 0];
const BR_ORANGE: [u8; 3] = [255, 217, 39];
/// `SetTextColor(255, 255, 230, 210)` de las cajas de la cuenta y la contraseña (ARGB).
const INPUT_COLOR: [u8; 3] = [255, 230, 210];

/// Textos (`GlobalText` de Text_eng.bmd del S6).
const TEXT_COPYRIGHT: &str = "(c) Copyright 2001 Webzen"; // 454
const TEXT_RIGHTS: &str = " All Rights Reserved."; // 455
const TEXT_ACCOUNT: &str = "Account"; // 450
const TEXT_PASSWORD: &str = "Password"; // 451
const TEXT_EXIT_GAME: &str = "Exit Game"; // 381
const TEXT_OPTION: &str = "Option"; // 385
const TEXT_CLOSE: &str = "Close"; // 388
const TEXT_SERVER_FULL: &str = "The server is full"; // 416
const TEXT_ENTER_ACCOUNT: &str = "Enter your account"; // 403
const TEXT_ENTER_PASSWORD: &str = "Enter your password"; // 404
/// 565-567: aviso de los grupos sin ningún servidor Non-PvP.
const TEXT_PVP: [&str; 3] = ["Since Helheim server ", "tends to be crowded", "we recommend that you use other servers."];

/// Pantalla en píxeles de la ventana de referencia.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Screen {
    w: i32,
    h: i32,
}

impl Screen {
    /// `g_fScreenRate_x` / `g_fScreenRate_y`: píxeles por unidad de la pantalla de 640×480.
    fn rate_x(self) -> f32 {
        self.w as f32 / 640.0
    }
    fn rate_y(self) -> f32 {
        self.h as f32 / 480.0
    }
    /// `int(x / g_fScreenRate_x)`: los textos se colocan en unidades enteras de 640×480.
    fn snap_x(self, x: f32) -> f32 {
        (x / self.rate_x()).floor() * self.rate_x()
    }
    fn snap_y(self, y: f32) -> f32 {
        (y / self.rate_y()).floor() * self.rate_y()
    }
}

/// Rectángulo (x, y, ancho, alto) en píxeles; dentro como `PtInRect` (sin el borde derecho ni el de abajo).
#[derive(Clone, Copy, PartialEq, Debug)]
struct IRect {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

impl IRect {
    const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }
    fn contains(self, p: Option<IVec2>) -> bool {
        p.is_some_and(|p| p.x >= self.x && p.x < self.x + self.w && p.y >= self.y && p.y < self.y + self.h)
    }
}

/// `CUIMng::CreateLoginScene` (UIMng.cpp:233-234): `int(567 / 600 × alto) − 30 − 11`.
fn bar_y(s: Screen) -> i32 {
    (567.0 / 600.0 * s.h as f32) as i32 - BAR_BTN_H - 11
}

/// `CLoginMainWin`: la barra mide el ancho de la pantalla menos 30 a cada lado (LoginMainWin.cpp:46).
fn bar_rect(s: Screen) -> IRect {
    IRect::new(30, bar_y(s), s.w - 60, BAR_BTN_H)
}

fn menu_rect(s: Screen) -> IRect {
    IRect::new(30, bar_y(s), BAR_BTN_W, BAR_BTN_H)
}

/// Pegado al borde derecho de la barra (LoginMainWin.cpp:65-67).
fn credit_rect(s: Screen) -> IRect {
    let bar = bar_rect(s);
    IRect::new(bar.x + bar.w - BAR_BTN_W, bar.y, BAR_BTN_W, BAR_BTN_H)
}

/// `CServerSelWin`, centrada (UIMng.cpp:238-240).
fn ssw_rect(s: Screen) -> IRect {
    IRect::new((s.w - SSW_W) / 2, (s.h - SSW_H) / 2, SSW_W, SSW_H)
}

/// Botón de grupo `i` (`CServerSelWin::SetPosition`, ServerSelWin.cpp:108-144): el 0 (servidor de
/// pruebas) centrado abajo, del 1 al 10 en la columna izquierda y del 11 al 20 en la derecha.
fn group_rect(s: Screen, i: usize) -> IRect {
    let r = ssw_rect(s);
    let base = r.y + r.h - (GROUP_H * 11 + SSW_GAP_H * 2 + DESC_H);
    let (x, y) = match i {
        0 => (r.x + (r.w - GROUP_W) / 2, r.y + r.h - GROUP_H - SSW_GAP_H - DESC_H),
        1..=SSW_LEFT_MAX => (r.x, base + GROUP_H * (i as i32 - 1)),
        _ => (r.x + GROUP_W + SERVER_W + SSW_GAP_W * 2, base + GROUP_H * (i as i32 - 1 - SSW_LEFT_MAX as i32)),
    };
    IRect::new(x, y, GROUP_W, GROUP_H)
}

/// Servidor `j` de `n` (`SetServerBtnPosition`, ServerSelWin.cpp:146-168): entre las dos columnas,
/// desde la altura del primer grupo de la izquierda (o más arriba si no caben en 10 filas).
fn server_rect(s: Screen, j: usize, n: usize) -> IRect {
    let g = group_rect(s, 1);
    let left_sum = GROUP_H * 10;
    let sum = SERVER_H * n as i32;
    let base = if left_sum > sum { g.y } else { g.y - (sum - left_sum) };
    IRect::new(g.x + GROUP_W + SSW_GAP_W, base + SERVER_H * j as i32, SERVER_W, SERVER_H)
}

/// `CLoginWin`: centrada en horizontal y a 2/3 en vertical (UIMng.cpp:244-246).
fn login_rect(s: Screen) -> IRect {
    IRect::new((s.w - LOGIN_W) / 2, (s.h - LOGIN_H) * 2 / 3, LOGIN_W, LOGIN_H)
}

/// Marco de la caja de texto de la cuenta (0) o la contraseña (1) (LoginWin.cpp:92-93).
fn field_rect(s: Screen, field: Field) -> IRect {
    let l = login_rect(s);
    let dy = if field == Field::Id { 106 } else { 131 };
    IRect::new(l.x + 109, l.y + dy, 156, 23)
}

/// Donde empieza el texto de la caja: `int((marco + 6) / g_fScreenRate)` (LoginWin.cpp:97-101).
fn input_origin(s: Screen, field: Field) -> Vec2 {
    let f = field_rect(s, field);
    Vec2::new(s.snap_x((f.x + 6) as f32), s.snap_y((f.y + 6) as f32))
}

/// Zona que da el foco al pulsar: `CheckMouseIn(x, y − 4, ancho, alto + 8)` en 640×480 (UIControls.cpp:3628).
fn input_click_rect(s: Screen, field: Field) -> IRect {
    let o = input_origin(s, field);
    let (rx, ry) = (s.rate_x(), s.rate_y());
    IRect::new(o.x as i32, (o.y - 4.0 * ry) as i32, (INPUT_W * rx) as i32, ((INPUT_H + 8.0) * ry) as i32)
}

fn msg_rect(s: Screen) -> IRect {
    IRect::new((s.w - MSG_W) / 2, (s.h - MSG_H) / 2, MSG_W, MSG_H)
}

fn sys_rect(s: Screen) -> IRect {
    IRect::new((s.w - SYS_W) / 2, (s.h - SYS_H) / 2, SYS_W, SYS_H)
}

// --- Estado ----------------------------------------------------------------------------------------

/// Ventanas del login (las de `CUIMng::CreateLoginScene` que hay de momento).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Win {
    MainBar,
    ServerSel,
    Login,
    SysMenu,
    Msg,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Btn {
    Menu,
    Credit,
    /// Botón de grupo por su posición (`m_iBtnPos`).
    Group(usize),
    /// Servidor del grupo elegido, por orden.
    Server(usize),
    LoginOk,
    LoginCancel,
    MsgOk,
    MsgCancel,
    SysExit,
    SysOption,
    SysClose,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Field {
    Id,
    Pass,
}

/// `MSG_WIN_TYPE` (sin `MWT_BTN_BOTH` ni `MWT_STR_INPUT`, que son de la selección de personaje).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum MsgKind {
    None,
    Cancel,
    Ok,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum MsgCode {
    ServerBusy,
    InputId,
    InputPassword,
    GameEndCountdown,
}

struct Message {
    code: Option<MsgCode>,
    kind: MsgKind,
    lines: Vec<String>,
}

/// `CServerGroup::SBP_*`: columna del botón del grupo. La derecha y el centro (servidor de
/// pruebas) llegarán con la lista del connect server.
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum GroupPos {
    Left,
    Right,
    Center,
}

struct ServerInfo {
    /// Número del servidor dentro del grupo (`m_iIndex`, desde 1).
    index: i32,
    /// Carga (`m_iPercent`): 100 = lleno, 128 o más = preparándose.
    percent: i32,
    /// `m_byNonPvP`: 0 normal, 1 Non-PvP, 2 Gold PvP, 3 Gold.
    non_pvp: u8,
}

struct ServerGroup {
    name: String,
    pos: GroupPos,
    /// Botón donde va (`m_iBtnPos`), lo pone `UpdateDisplay`.
    btn: usize,
    /// Ningún servidor Non-PvP (`m_bPvPServer`).
    pvp: bool,
    servers: Vec<ServerInfo>,
}

impl ServerInfo {
    /// `CServerListManager::InsertServer` (ServerListManager.cpp:177-212) con los textos 560-562.
    fn label(&self, group: &str) -> String {
        let state = if self.percent >= 128 {
            "Now preparing"
        } else if self.percent >= 100 {
            "(Full)"
        } else {
            "Connect"
        };
        match self.non_pvp {
            1 => format!("{group}-{}(Non-PVP) {state}", self.index),
            2 => format!("{group}-{}(Gold PVP) {state}", self.index),
            3 => format!("{group}-{}(Gold) {state}", self.index),
            _ => format!("{group}-{} {state}", self.index),
        }
    }

    /// `adwServerBtnClr[m_byNonPvP]`: normal, pulsado y ratón encima (ServerSelWin.cpp:200-206).
    fn colors(&self) -> [[u8; 3]; 3] {
        match self.non_pvp {
            1 => [YELLOW, YELLOW, BR_YELLOW],
            2 | 3 => [ORANGE, ORANGE, BR_ORANGE],
            _ => [BR_GRAY, BR_GRAY, WHITE],
        }
    }
}

/// Caja de texto (`CUITextInputBox`, un `edit` de Windows por debajo).
struct TextBox {
    text: String,
    /// Posición del cursor, en caracteres.
    caret: usize,
    /// Todo seleccionado (`GiveFocus(TRUE)`): lo siguiente que se escriba lo sustituye.
    select_all: bool,
    max: usize,
    password: bool,
}

impl TextBox {
    fn new(text: &str, max: usize, password: bool) -> Self {
        let text: String = text.chars().take(max).collect();
        Self { caret: text.chars().count(), text, select_all: false, max, password }
    }

    fn len(&self) -> usize {
        self.text.chars().count()
    }

    fn byte_at(&self, i: usize) -> usize {
        self.text.char_indices().nth(i).map_or(self.text.len(), |(b, _)| b)
    }

    /// Quita lo seleccionado (todo). Devuelve si había algo.
    fn take_selection(&mut self) -> bool {
        if !self.select_all {
            return false;
        }
        self.select_all = false;
        let had = !self.text.is_empty();
        self.text.clear();
        self.caret = 0;
        had
    }

    fn insert(&mut self, c: char) {
        self.take_selection();
        if self.len() >= self.max {
            return;
        }
        let b = self.byte_at(self.caret);
        self.text.insert(b, c);
        self.caret += 1;
    }

    fn backspace(&mut self) {
        if self.take_selection() || self.caret == 0 {
            return;
        }
        let b = self.byte_at(self.caret - 1);
        self.text.remove(b);
        self.caret -= 1;
    }

    fn delete(&mut self) {
        if self.take_selection() || self.caret >= self.len() {
            return;
        }
        let b = self.byte_at(self.caret);
        self.text.remove(b);
    }

    /// Mueve el cursor (y quita la selección, como el `edit`).
    fn move_to(&mut self, caret: usize) {
        self.select_all = false;
        self.caret = caret.min(self.len());
    }

    /// Lo que se ve: la contraseña en asteriscos (`ES_PASSWORD`).
    fn shown(&self) -> String {
        if self.password { "*".repeat(self.len()) } else { self.text.clone() }
    }
}

/// Lo que pasa fuera de la interfaz.
enum Outcome {
    /// Login aceptado con esta cuenta.
    Enter(String),
    /// Fin de la cuenta atrás de "Exit Game".
    Exit,
}

#[derive(Resource)]
struct LoginUi {
    /// Ventanas de delante a atrás (`m_WinList`): la primera es la que puede estar activa.
    order: Vec<Win>,
    shown: Vec<Win>,
    /// La primera de `order` está activa (`m_bActive`).
    active: bool,
    /// Activar la primera en la próxima pasada (`m_bWinActive`).
    pending_active: bool,
    /// `m_bSysMenuWinShow`: cambia con cada Esc.
    sys_flag: bool,
    /// Botón apretado (`CButton::m_pBtnHeld`).
    held: Option<Btn>,
    /// Ventana bajo el ratón: solo sus botones responden (`ActiveBtns`).
    btn_window: Option<Win>,
    cursor: Option<IVec2>,

    groups: Vec<ServerGroup>,
    /// Botón de grupo elegido (`m_iSelectServerBtnIndex`).
    selected_group: Option<usize>,
    /// Servidor elegido para el título de la ventana de login: grupo, número, Non-PvP.
    selected_server: Option<(String, i32, u8)>,

    id: TextBox,
    pass: TextBox,
    focus: Option<Field>,
    /// La ventana de login aún no se ha dibujado (`FirstLoad`).
    first_load: bool,
    caret_clock: f32,

    msg: Message,
    /// Cuenta atrás de "Exit Game" (`m_nGameExit`, `m_dDeltaTickSum`).
    exit_count: i32,
    exit_clock: f32,

    sounds: u32,
    outcome: Option<Outcome>,

    /// Fundido del logo (`g_fMULogoAlpha`) en este tick y en el anterior.
    logo: f32,
    prev_logo: f32,
}

impl LoginUi {
    fn new(saved_id: &str) -> Self {
        let mut ui = Self {
            // Orden de creación (`AddHead`): la última creada queda delante.
            order: vec![Win::Login, Win::ServerSel, Win::MainBar, Win::SysMenu, Win::Msg],
            shown: Vec::new(),
            active: false,
            pending_active: false,
            sys_flag: false,
            held: None,
            btn_window: None,
            cursor: None,
            groups: offline_server_list(),
            selected_group: None,
            selected_server: None,
            id: TextBox::new(saved_id, MAX_ID, false),
            pass: TextBox::new("", MAX_PASSWORD, true),
            focus: None,
            first_load: true,
            caret_clock: 0.0,
            msg: Message { code: None, kind: MsgKind::None, lines: Vec::new() },
            exit_count: 0,
            exit_clock: 0.0,
            sounds: 0,
            outcome: None,
            logo: 0.0,
            prev_logo: 0.0,
        };
        ui.update_display();
        // Sin servidor: la lista llega enseguida (`ReceiveServerList`).
        ui.server_list_received();
        ui
    }

    fn is_shown(&self, w: Win) -> bool {
        self.shown.contains(&w)
    }

    fn is_active(&self, w: Win) -> bool {
        self.active && self.order.first() == Some(&w)
    }

    /// `CUIMng::SetActiveWin`: la de delante deja de estar activa y `w` pasa delante.
    fn set_active_win(&mut self, w: Win) {
        self.active = false;
        if self.is_shown(w) {
            self.order.retain(|&o| o != w);
            self.order.insert(0, w);
            self.pending_active = true;
        }
    }

    fn show_win(&mut self, w: Win) {
        if !self.is_shown(w) {
            self.shown.push(w);
        }
        self.set_active_win(w);
    }

    /// `CUIMng::HideWin`: se oculta y pasa al final; la nueva primera se activa.
    fn hide_win(&mut self, w: Win) {
        if self.order.first() == Some(&w) {
            self.active = false;
        }
        self.shown.retain(|&o| o != w);
        self.order.retain(|&o| o != w);
        self.order.push(w);
        if self.is_shown(self.order[0]) {
            self.pending_active = true;
        }
    }

    /// `ReceiveServerList` (WSclient.cpp:311-317).
    fn server_list_received(&mut self) {
        self.show_win(Win::ServerSel);
        self.update_display();
        self.show_win(Win::MainBar);
    }

    /// `CServerSelWin::UpdateDisplay`: reparte los grupos en los botones.
    fn update_display(&mut self) {
        let (mut test, mut left, mut right) = (false, 0, 0);
        for g in &mut self.groups {
            g.btn = usize::MAX;
            match g.pos {
                GroupPos::Center if !test => {
                    g.btn = 0;
                    test = true;
                }
                GroupPos::Left if left < SSW_LEFT_MAX => {
                    left += 1;
                    g.btn = left;
                }
                GroupPos::Right if right < SSW_RIGHT_MAX => {
                    right += 1;
                    g.btn = SSW_LEFT_MAX + right;
                }
                _ => {}
            }
        }
    }

    fn selected(&self) -> Option<&ServerGroup> {
        let btn = self.selected_group?;
        self.groups.iter().find(|g| g.btn == btn)
    }

    fn popup(&mut self, code: MsgCode) {
        let (kind, text) = match code {
            MsgCode::ServerBusy => (MsgKind::Ok, TEXT_SERVER_FULL.to_string()),
            MsgCode::InputId => (MsgKind::Ok, TEXT_ENTER_ACCOUNT.to_string()),
            MsgCode::InputPassword => (MsgKind::Ok, TEXT_ENTER_PASSWORD.to_string()),
            MsgCode::GameEndCountdown => {
                self.exit_count = 5;
                self.exit_clock = 0.0;
                (MsgKind::None, countdown_text(self.exit_count))
            }
        };
        self.msg = Message { code: Some(code), kind, lines: separate_lines(&text) };
        self.show_win(Win::Msg);
    }

    fn give_focus(&mut self, field: Field, select_all: bool) {
        self.focus = Some(field);
        self.caret_clock = 0.0;
        let b = self.field_mut(field);
        b.select_all = select_all && !b.text.is_empty();
        let n = b.len();
        b.caret = n;
    }

    fn field_mut(&mut self, field: Field) -> &mut TextBox {
        match field {
            Field::Id => &mut self.id,
            Field::Pass => &mut self.pass,
        }
    }

    fn field(&self, field: Field) -> &TextBox {
        match field {
            Field::Id => &self.id,
            Field::Pass => &self.pass,
        }
    }

    /// Botones de una ventana, en el orden en que se registran y se dibujan.
    fn buttons(&self, s: Screen, w: Win) -> Vec<ButtonDef> {
        let mut out = Vec::new();
        match w {
            Win::MainBar => {
                out.push(ButtonDef::image(Btn::Menu, menu_rect(s), Tex::MenuBtn));
                out.push(ButtonDef::image(Btn::Credit, credit_rect(s), Tex::CreditBtn));
            }
            Win::ServerSel => {
                let mut groups: Vec<&ServerGroup> = self.groups.iter().filter(|g| g.btn != usize::MAX).collect();
                groups.sort_by_key(|g| g.btn);
                for g in groups {
                    out.push(ButtonDef {
                        id: Btn::Group(g.btn),
                        rect: group_rect(s, g.btn),
                        tex: Tex::GroupBtn,
                        // `Create(108, 26, BITMAP_LOG_IN, 4, 2, 1, -1, 3)`: normal 0, encima 1,
                        // pulsado 2 y elegido 3.
                        frames: [0, 2, 1],
                        check_frame: Some(3),
                        checked: self.selected_group == Some(g.btn),
                        text: Some((g.name.clone(), [BR_GRAY, BR_GRAY, WHITE])),
                    });
                }
                if let Some(g) = self.selected() {
                    let n = g.servers.len().min(SSW_SERVER_MAX as usize);
                    for (j, info) in g.servers.iter().take(n).enumerate() {
                        out.push(ButtonDef {
                            id: Btn::Server(j),
                            rect: server_rect(s, j, n),
                            tex: Tex::ServerBtn,
                            frames: [0, 2, 1],
                            check_frame: None,
                            checked: false,
                            text: Some((info.label(&g.name), info.colors())),
                        });
                    }
                }
            }
            Win::Login => {
                let l = login_rect(s);
                out.push(ButtonDef::image(Btn::LoginOk, IRect::new(l.x + 150, l.y + 178, 54, 30), Tex::OkBtn));
                out.push(ButtonDef::image(Btn::LoginCancel, IRect::new(l.x + 211, l.y + 178, 54, 30), Tex::CancelBtn));
            }
            Win::Msg => {
                // `CMsgWin::SetCtrlPosition` (MsgWin.cpp:75-104).
                let m = msg_rect(s);
                let y = m.y + 72;
                let ok = |x| ButtonDef::image(Btn::MsgOk, IRect::new(m.x + x, y, 54, 30), Tex::OkBtn);
                let cancel = |x| ButtonDef::image(Btn::MsgCancel, IRect::new(m.x + x, y, 54, 30), Tex::CancelBtn);
                match self.msg.kind {
                    MsgKind::Ok => out.push(ok(149)),
                    MsgKind::Cancel => out.push(cancel(149)),
                    MsgKind::None => {}
                }
            }
            Win::SysMenu => {
                // `CSysMenuWin::SetPosition` (SysMenuWin.cpp:83-96). "Select Server" está
                // deshabilitado en el login y sin fotograma de deshabilitado: no se dibuja.
                let r = sys_rect(s);
                let x = r.x + (SYS_W - 108) / 2;
                let gap = 4 + 30;
                let close_y = r.y + r.h - 52;
                for (id, y, text) in [
                    (Btn::SysExit, r.y + 33, TEXT_EXIT_GAME),
                    (Btn::SysOption, close_y - gap, TEXT_OPTION),
                    (Btn::SysClose, close_y, TEXT_CLOSE),
                ] {
                    out.push(ButtonDef {
                        id,
                        rect: IRect::new(x, y, 108, 30),
                        tex: Tex::TextBtn,
                        frames: [0, 2, 1],
                        check_frame: None,
                        checked: false,
                        text: Some((text.to_string(), [BR_GRAY, BR_GRAY, WHITE])),
                    });
                }
            }
        }
        out
    }

    /// `CWin::CursorInWin(WA_ALL)`. La caja de mensajes y el menú de sistema ocupan toda la pantalla.
    fn win_rect(s: Screen, w: Win) -> IRect {
        match w {
            Win::MainBar => bar_rect(s),
            Win::ServerSel => ssw_rect(s),
            Win::Login => login_rect(s),
            Win::SysMenu | Win::Msg => IRect::new(0, 0, s.w, s.h),
        }
    }

    /// La primera ventana visible (de delante a atrás) bajo el ratón.
    fn window_under(&self, s: Screen, cursor: Option<IVec2>) -> Option<Win> {
        self.order.iter().copied().find(|&w| self.is_shown(w) && Self::win_rect(s, w).contains(cursor))
    }

    /// Estado de un botón para dibujarlo (`CButton::Update`): encima, apretado o normal.
    fn button_state(&self, b: &ButtonDef, w: Win) -> BtnState {
        let inside = self.btn_window == Some(w) && b.rect.contains(self.cursor);
        match self.held {
            None if inside => BtnState::Active,
            Some(h) if inside && h == b.id => BtnState::Down,
            _ => BtnState::Up,
        }
    }

    /// `CButton::Update`: se aprieta al bajar el botón del ratón y es un clic si se suelta encima.
    fn update_button(&mut self, b: &ButtonDef, w: Win, down: bool, up: bool) -> bool {
        let inside = self.btn_window == Some(w) && b.rect.contains(self.cursor);
        if inside && down {
            self.held = Some(b.id);
        }
        let mut click = false;
        if up {
            if inside && self.held == Some(b.id) {
                click = true;
                self.sounds += 1;
            }
            if self.held == Some(b.id) {
                self.held = None;
            }
        }
        click
    }

    /// Una pasada de `CUIMng::Update` y de `NewMoveLogInScene`.
    fn update(&mut self, s: Screen, input: &FrameInput) {
        self.cursor = input.cursor;
        if self.pending_active && self.order.first().is_some_and(|&w| self.is_shown(w)) {
            self.active = true;
            self.pending_active = false;
        }
        if input.esc {
            self.sys_flag = !self.sys_flag;
        }
        if input.down {
            match self.window_under(s, input.cursor) {
                Some(w) => self.set_active_win(w),
                None => self.active = false,
            }
        }
        self.btn_window = self.window_under(s, input.cursor);

        let order = self.order.clone();
        for w in order {
            if !self.is_shown(w) {
                continue;
            }
            let mut clicks = Vec::new();
            for b in self.buttons(s, w) {
                if self.update_button(&b, w, input.down, input.up) {
                    clicks.push(b.id);
                }
            }
            if w == Win::Login {
                self.login_while_shown(s, input);
            }
            if self.is_active(w) {
                self.while_active(w, &clicks, input);
            }
        }

        // `NewMoveLogInScene` (ZzzScene.cpp:1254-1267): Esc abre el menú de sistema desde la lista.
        if input.esc
            && !(self.is_shown(Win::Msg) || self.is_shown(Win::Login) || self.is_shown(Win::SysMenu))
            && self.is_shown(Win::MainBar)
            && self.is_shown(Win::ServerSel)
            && self.sys_flag
        {
            self.sounds += 1;
            self.show_win(Win::SysMenu);
        }
        // El original solo suelta el botón desde el propio botón; si su ventana se ha ocultado
        // mientras tanto, se quedaría apretado para siempre.
        if input.up {
            self.held = None;
        }
        self.caret_clock += input.dt;
    }

    /// `UpdateWhileActive` de cada ventana.
    fn while_active(&mut self, w: Win, clicks: &[Btn], input: &FrameInput) {
        match w {
            Win::MainBar => {
                if clicks.contains(&Btn::Menu) {
                    self.show_win(Win::SysMenu);
                    self.sys_flag = true;
                }
                // Credit: los créditos (`CCreditWin`) aún no están hechos.
            }
            Win::ServerSel => {
                // ServerSelWin.cpp:403-472.
                for &c in clicks {
                    if let Btn::Group(i) = c {
                        self.selected_group = Some(i);
                        // `SendRequestServerList`: sin servidor, la respuesta llega ya.
                        self.server_list_received();
                    }
                }
                let Some(g) = self.selected() else { return };
                let chosen = clicks.iter().find_map(|c| match c {
                    Btn::Server(j) => g.servers.get(*j).map(|info| (g.name.clone(), info.index, info.percent, info.non_pvp)),
                    _ => None,
                });
                if let Some((name, index, percent, non_pvp)) = chosen {
                    if percent < 100 {
                        self.hide_win(Win::ServerSel);
                        self.selected_server = Some((name, index, non_pvp));
                        // `SendRequestServerAddress` → el servidor de juego saluda (`ReceiveJoinServer`).
                        self.show_win(Win::Login);
                    } else if percent < 128 {
                        self.popup(MsgCode::ServerBusy);
                    }
                }
            }
            Win::Login => {
                // LoginWin.cpp:135-154.
                if clicks.contains(&Btn::LoginOk) {
                    self.request_login();
                } else if clicks.contains(&Btn::LoginCancel) {
                    self.cancel_login();
                } else if input.enter {
                    self.sounds += 1;
                    self.request_login();
                } else if input.esc {
                    self.sounds += 1;
                    self.cancel_login();
                    self.sys_flag = false;
                }
            }
            Win::Msg => {
                // MsgWin.cpp:156-214.
                if input.enter {
                    if self.msg.kind > MsgKind::Cancel {
                        self.sounds += 1;
                        self.msg_ok();
                    } else if self.msg.kind == MsgKind::Cancel {
                        self.sounds += 1;
                        self.msg_cancel();
                    }
                } else if input.esc {
                    if self.msg.kind == MsgKind::Ok {
                        self.sounds += 1;
                        self.msg_ok();
                    } else if self.msg.kind > MsgKind::None {
                        self.sounds += 1;
                        self.msg_cancel();
                    }
                    self.sys_flag = false;
                } else if clicks.contains(&Btn::MsgOk) {
                    self.msg_ok();
                } else if clicks.contains(&Btn::MsgCancel) {
                    self.msg_cancel();
                } else if self.msg.code == Some(MsgCode::GameEndCountdown) && self.exit_count > 0 {
                    self.exit_clock += input.dt;
                    if self.exit_clock > 1.0 {
                        self.exit_clock = 0.0;
                        self.exit_count -= 1;
                        if self.exit_count == 0 {
                            self.outcome = Some(Outcome::Exit);
                        } else {
                            self.msg.lines = separate_lines(&countdown_text(self.exit_count));
                        }
                    }
                }
            }
            Win::SysMenu => {
                // SysMenuWin.cpp:120-153.
                if clicks.contains(&Btn::SysExit) {
                    self.popup(MsgCode::GameEndCountdown);
                } else if clicks.contains(&Btn::SysOption) {
                    // Opciones (`COptionWin`): aún no están hechas.
                } else if clicks.contains(&Btn::SysClose) {
                    self.sys_flag = false;
                    self.hide_win(Win::SysMenu);
                } else if input.esc && !self.sys_flag {
                    self.sounds += 1;
                    self.hide_win(Win::SysMenu);
                }
            }
        }
    }

    /// `CLoginWin::RequestLogin` (LoginWin.cpp:201-235).
    fn request_login(&mut self) {
        self.hide_win(Win::Login);
        if self.id.text.is_empty() {
            self.popup(MsgCode::InputId);
        } else if self.pass.text.is_empty() {
            self.popup(MsgCode::InputPassword);
        } else {
            // Sin servidor: cualquier cuenta vale (`RECEIVE_LOG_IN_SUCCESS`).
            self.outcome = Some(Outcome::Enter(self.id.text.clone()));
        }
    }

    /// `CLoginWin::CancelLogin`: vuelve a pedir la lista al connect server.
    fn cancel_login(&mut self) {
        self.hide_win(Win::Login);
        self.server_list_received();
    }

    /// `CMsgWin::ManageOKClick` (MsgWin.cpp:447-494).
    fn msg_ok(&mut self) {
        self.hide_win(Win::Msg);
        match self.msg.code {
            Some(MsgCode::InputId) => {
                self.show_win(Win::Login);
                self.give_focus(Field::Id, true);
            }
            Some(MsgCode::InputPassword) => {
                self.show_win(Win::Login);
                self.give_focus(Field::Pass, true);
            }
            _ => {}
        }
    }

    fn msg_cancel(&mut self) {
        self.msg.code = None;
        self.hide_win(Win::Msg);
    }

    /// `CLoginWin::UpdateWhileShow`: las cajas de texto (clic para el foco y teclado).
    fn login_while_shown(&mut self, s: Screen, input: &FrameInput) {
        if self.first_load {
            // `FirstLoad` (LoginWin.cpp:165-172): con la cuenta guardada, a la contraseña.
            self.first_load = false;
            let field = if self.id.text.is_empty() { Field::Id } else { Field::Pass };
            self.give_focus(field, false);
        }
        if input.down {
            for field in [Field::Id, Field::Pass] {
                if input_click_rect(s, field).contains(input.cursor) {
                    self.give_focus(field, true);
                }
            }
        }
        for key in &input.keys {
            let Some(field) = self.focus else { return };
            let b = self.field_mut(field);
            match key {
                Key::Character(t) => {
                    if input.ctrl {
                        continue;
                    }
                    for c in t.chars().filter(|c| !c.is_control()) {
                        b.insert(c);
                    }
                }
                Key::Space => b.insert(' '),
                Key::Backspace => b.backspace(),
                Key::Delete => b.delete(),
                Key::ArrowLeft => {
                    let c = if b.select_all { 0 } else { b.caret.saturating_sub(1) };
                    b.move_to(c);
                }
                Key::ArrowRight => {
                    let c = if b.select_all { b.len() } else { b.caret + 1 };
                    b.move_to(c);
                }
                Key::Home => b.move_to(0),
                Key::End => {
                    let n = b.len();
                    b.move_to(n);
                }
                Key::Tab => {
                    // `SetTabTarget`: la otra caja, con todo seleccionado.
                    let other = if field == Field::Id { Field::Pass } else { Field::Id };
                    self.give_focus(other, true);
                }
                _ => continue,
            }
            self.caret_clock = 0.0;
        }
    }
}

/// `GlobalText[380]`.
fn countdown_text(n: i32) -> String {
    format!("You will exit game in {n} seconds.")
}

/// `SeparateTextIntoLines` con `MW_MSG_ROW_MAX` (52) y 2 líneas como mucho: corta por espacios.
fn separate_lines(text: &str) -> Vec<String> {
    const ROW: usize = 51;
    let mut lines = vec![String::new()];
    for word in text.split(' ') {
        let full = lines.len() >= 2;
        let last = lines.last_mut().unwrap();
        if !last.is_empty() && last.chars().count() + 1 + word.chars().count() > ROW && !full {
            lines.push(word.to_string());
        } else {
            if !last.is_empty() {
                last.push(' ');
            }
            last.push_str(word);
        }
    }
    lines
}

/// La lista de servidores sin connect server: un grupo "mu-rs" a la izquierda con un servidor
/// Non-PvP vacío (en `ServerList.bmd` irían el nombre, la columna y los Non-PvP de cada grupo).
fn offline_server_list() -> Vec<ServerGroup> {
    let servers = vec![ServerInfo { index: 1, percent: 0, non_pvp: 1 }];
    // `MakeServerGroup`: es PvP si ningún servidor lleva el bit Non-PvP.
    let pvp = !servers.iter().any(|s| s.non_pvp & 1 != 0);
    vec![ServerGroup { name: "mu-rs".into(), pos: GroupPos::Left, btn: usize::MAX, pvp, servers }]
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BtnState {
    Up,
    Down,
    Active,
}

/// Botón (`CButton`): rectángulo, textura con los fotogramas en vertical y texto opcional.
struct ButtonDef {
    id: Btn,
    rect: IRect,
    tex: Tex,
    /// Fotograma normal, pulsado y con el ratón encima.
    frames: [u32; 3],
    /// Fotograma de elegido (`nCheckUpFrame`), para los tres estados.
    check_frame: Option<u32>,
    checked: bool,
    /// Texto (con `g_hFixFont`, centrado) y sus colores normal, pulsado y encima.
    text: Option<(String, [[u8; 3]; 3])>,
}

impl ButtonDef {
    /// Botón de 3 fotogramas con el texto pintado en la textura (`Create(w, h, tex, 3, 2, 1)`).
    fn image(id: Btn, rect: IRect, tex: Tex) -> Self {
        Self { id, rect, tex, frames: [0, 2, 1], check_frame: None, checked: false, text: None }
    }
}

// --- Entrada ---------------------------------------------------------------------------------------

/// Lo que se ha pulsado en este fotograma (`CInput`).
struct FrameInput {
    /// Ratón en píxeles de la pantalla de referencia.
    cursor: Option<IVec2>,
    down: bool,
    up: bool,
    esc: bool,
    enter: bool,
    ctrl: bool,
    /// Teclas para las cajas de texto (con repetición).
    keys: Vec<Key>,
    dt: f32,
}

/// Tamaño de la pantalla virtual (unidades de la interfaz) y de referencia.
fn screen_size(window: Option<&Window>, scale: f32) -> (Vec2, Screen) {
    let ui = window
        .filter(|w| w.width() >= 1.0 && w.height() >= 1.0)
        .map_or(Vec2::new(640.0, 480.0), |w| Vec2::new(w.width(), w.height()) / scale.max(1e-3));
    // Redondeado: 1024 / 1.6 puede salir 639.99994.
    (ui, Screen { w: (ui.x / K).round() as i32, h: (ui.y / K).round() as i32 })
}

fn keep_tab(mut keys: ResMut<ButtonInput<KeyCode>>) {
    keys.clear_just_pressed(KeyCode::Tab);
}

#[allow(clippy::too_many_arguments)]
fn update_ui(
    mut ui: ResMut<LoginUi>,
    windows: Query<&Window>,
    scale: Res<UiScale>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut typed: MessageReader<KeyboardInput>,
    time: Res<Time>,
    mut sound: ResMut<SoundQueue>,
    mut args: ResMut<Args>,
    mut load: ResMut<LoadStep>,
    mut next: ResMut<NextState<GameScene>>,
    mut exit: MessageWriter<AppExit>,
) {
    let window = windows.single().ok();
    let (_, s) = screen_size(window, scale.0);
    let cursor = window
        .and_then(|w| w.cursor_position())
        .map(|c| (c / scale.0.max(1e-3) / K).floor().as_ivec2());
    let input = FrameInput {
        cursor,
        down: mouse.just_pressed(MouseButton::Left),
        up: mouse.just_released(MouseButton::Left),
        esc: keys.just_pressed(KeyCode::Escape),
        enter: keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter),
        ctrl: keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]),
        keys: typed.read().filter(|k| k.state == ButtonState::Pressed).map(|k| k.logical_key.clone()).collect(),
        dt: time.delta_secs(),
    };
    ui.update(s, &input);
    for _ in 0..std::mem::take(&mut ui.sounds) {
        // `PlayBuffer(SOUND_CLICK01)`.
        sound.play("SOUND_CLICK01");
    }
    match ui.outcome.take() {
        Some(Outcome::Enter(id)) => {
            crate::settings::save_value("usuario", &id);
            crate::scene::load_world(&mut args, &mut load, crate::scene::CHARACTER_WORLD, false, GameScene::CharacterSelect);
            next.set(GameScene::Loading);
        }
        Some(Outcome::Exit) => {
            exit.write(AppExit::Success);
        }
        None => {}
    }
}

// --- Texturas --------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Tex {
    GroupBtn,
    ServerBtn,
    ServerGauge,
    MenuBtn,
    CreditBtn,
    Deco,
    LoginBack,
    LoginField,
    OkBtn,
    CancelBtn,
    MsgBack,
    SysStone,
    SysTop,
    SysBottom,
    SysLeft,
    SysRight,
    TextBtn,
}

impl Tex {
    const ALL: [Tex; 17] = [
        Tex::GroupBtn,
        Tex::ServerBtn,
        Tex::ServerGauge,
        Tex::MenuBtn,
        Tex::CreditBtn,
        Tex::Deco,
        Tex::LoginBack,
        Tex::LoginField,
        Tex::OkBtn,
        Tex::CancelBtn,
        Tex::MsgBack,
        Tex::SysStone,
        Tex::SysTop,
        Tex::SysBottom,
        Tex::SysLeft,
        Tex::SysRight,
        Tex::TextBtn,
    ];

    /// `OpenLogoSceneData` (ZzzOpenData.cpp:4866-4890) y las comunes (ZzzOpenData.cpp:5254-5264).
    fn file(self) -> &'static str {
        match self {
            Tex::GroupBtn => "cha_bt.tga",
            Tex::ServerBtn => "server_b2_all.tga",
            Tex::ServerGauge => "server_b2_loding.jpg",
            Tex::MenuBtn => "server_menu_b_all.tga",
            Tex::CreditBtn => "server_credit_b_all.tga",
            Tex::Deco => "deco.tga",
            Tex::LoginBack => "login_back.tga",
            Tex::LoginField => "login_me.tga",
            Tex::OkBtn => "message_ok_b_all.tga",
            Tex::CancelBtn => "loding_cancel_b_all.tga",
            Tex::MsgBack => "message_back.tga",
            Tex::SysStone => "op1_stone.jpg",
            Tex::SysTop => "op1_back1.tga",
            Tex::SysBottom => "op1_back2.tga",
            Tex::SysLeft => "op1_back3.jpg",
            Tex::SysRight => "op1_back4.jpg",
            Tex::TextBtn => "op1_b_all.tga",
        }
    }
}

#[derive(Resource)]
struct LoginTextures(HashMap<Tex, Handle<Image>>);

// --- Logo ------------------------------------------------------------------------------------------

/// Logo y brillo con las mezclas del original: el brillo suma (`additive`) y el logo descarta
/// lo que tiene un alfa de 0.25 o menos.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
#[bind_group_data(LogoKey)]
struct LogoMaterial {
    /// `glColor4f` (el color ya pasado a lineal).
    #[uniform(0)]
    color: Vec4,
    #[texture(1)]
    #[sampler(2)]
    texture: Handle<Image>,
    additive: bool,
}

#[repr(C)]
#[derive(Eq, PartialEq, Hash, Copy, Clone)]
struct LogoKey {
    additive: bool,
}

impl From<&LogoMaterial> for LogoKey {
    fn from(m: &LogoMaterial) -> Self {
        Self { additive: m.additive }
    }
}

impl UiMaterial for LogoMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://mu_client/login_logo.wesl".into()
    }

    fn specialize(descriptor: &mut RenderPipelineDescriptor, key: UiMaterialKey<Self>) {
        let Some(fragment) = descriptor.fragment.as_mut() else { return };
        if key.bind_group_data.additive {
            // `EnableAlphaBlend`: glBlendFunc(GL_ONE, GL_ONE).
            let add = |src, dst| BlendComponent { src_factor: src, dst_factor: dst, operation: BlendOperation::Add };
            for target in fragment.targets.iter_mut().flatten() {
                target.blend = Some(BlendState {
                    color: add(BlendFactor::One, BlendFactor::One),
                    alpha: add(BlendFactor::One, BlendFactor::OneMinusSrcAlpha),
                });
            }
        } else {
            // `EnableAlphaTest`: mezcla por alfa y `glAlphaFunc(GL_GREATER, 0.25)`.
            fragment.shader_defs.push("LOGO_ALPHA_TEST".into());
        }
    }
}

#[derive(Component)]
struct LogoPart {
    glow: bool,
    material: Handle<LogoMaterial>,
}

/// `g_fMULogoAlpha += 0.02` hasta 10 (ZzzScene.cpp:1405-1406), por tick.
fn logo_tick(mut ui: ResMut<LoginUi>) {
    ui.prev_logo = ui.logo;
    ui.logo = (ui.logo + 0.02).min(10.0);
}

fn draw_logo(
    ui: Res<LoginUi>,
    fixed: Res<Time<Fixed>>,
    parts: Query<&LogoPart>,
    mut materials: ResMut<Assets<LogoMaterial>>,
) {
    let a = ui.prev_logo + (ui.logo - ui.prev_logo) * fixed.overstep_fraction();
    for part in &parts {
        // `glColor4f(a − 0.3, …)` para el brillo y `glColor4f(a, …)` para el logo (se recorta a 0..1).
        // El original mezcla en sRGB y Bevy en lineal: el color se pasa a lineal para que el
        // resultado se vea igual.
        let level = if part.glow { a - 0.3 } else { a }.clamp(0.0, 1.0);
        let lin = level.powf(2.2);
        let color = Vec4::new(lin, lin, lin, level);
        if materials.get(&part.material).is_some_and(|m| m.color != color) {
            if let Some(mut m) = materials.get_mut(&part.material) {
                m.color = color;
            }
        }
    }
}

// --- Creación --------------------------------------------------------------------------------------

#[derive(Component)]
struct LoginUiRoot;

/// Capa de las ventanas: sus hijos se rehacen cuando cambia lo que hay que dibujar.
#[derive(Component)]
struct WindowsLayer;

fn text_font(size: f32) -> (TextFont, LineHeight) {
    (
        TextFont { font_size: FontSize::Px(size * K), font_smoothing: SMOOTHING, ..default() },
        LineHeight::Px(size * K),
    )
}

fn srgb([r, g, b]: [u8; 3]) -> Color {
    Color::srgb_u8(r, g, b)
}

fn spawn_login_ui(
    mut commands: Commands,
    args: Res<Args>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<LogoMaterial>>,
) {
    let saved = crate::settings::saved_value("usuario").unwrap_or_default();
    commands.insert_resource(LoginUi::new(&saved));

    let textures: HashMap<Tex, Handle<Image>> = Tex::ALL
        .iter()
        .filter_map(|&t| crate::scene::interface_image(&args, &mut images, t.file()).map(|(h, _)| (t, h)))
        .collect();
    commands.insert_resource(LoginTextures(textures));

    let logo_dir = args.data.join("Logo");
    let mut logo = |name: &str| match mu_data::texture::load(&logo_dir, name) {
        Ok(t) => Some(images.add(to_bevy_image(&t))),
        Err(e) => {
            warn!("login: falta Logo/{name}: {e}");
            None
        }
    };
    let glow = logo("MU-logo_g.OZJ");
    let mark = logo("MU-logo.OZT");

    commands
        .spawn((
            Name::new("Interfaz del login"),
            LoginUiRoot,
            Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
        ))
        .with_children(|root| {
            // Logo: (320 − 128 × 0.8, 25) y 256 × 0.8 por 128 × 0.8 en 640×480 (ZzzScene.cpp:1415-1423).
            // Primero el brillo (suma) y encima el logo.
            for (texture, glow) in [(glow, true), (mark, false)] {
                let Some(texture) = texture else { continue };
                let material = materials.add(LogoMaterial { color: Vec4::new(0.0, 0.0, 0.0, 0.0), texture, additive: glow });
                root.spawn((
                    MaterialNode(material.clone()),
                    LogoPart { glow, material },
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Percent((320.0 - 128.0 * 0.8) / 640.0 * 100.0),
                        top: Val::Percent(25.0 / 480.0 * 100.0),
                        width: Val::Percent(256.0 * 0.8 / 640.0 * 100.0),
                        height: Val::Percent(128.0 * 0.8 / 480.0 * 100.0),
                        ..default()
                    },
                ));
            }

            // Líneas de abajo (ZzzScene.cpp:1430-1449): `g_hFont`, blanco sobre negro al 50 %,
            // con el pie del texto a 1 unidad del borde: el copyright acaba en x = 335 y lo
            // siguiente empieza ahí; la versión, a la izquierda.
            let line = |text: String| {
                (
                    Text::new(text),
                    text_font(FONT),
                    TextColor(Color::WHITE),
                    TextLayout::no_wrap(),
                    BackgroundColor(Color::srgba_u8(0, 0, 0, 128)),
                )
            };
            let bottom = Val::Percent(100.0 / 480.0);
            root.spawn((
                line(TEXT_COPYRIGHT.into()),
                Node { position_type: PositionType::Absolute, right: Val::Percent((640.0 - 335.0) / 640.0 * 100.0), bottom, ..default() },
            ));
            root.spawn((
                line(TEXT_RIGHTS.into()),
                Node { position_type: PositionType::Absolute, left: Val::Percent(335.0 / 640.0 * 100.0), bottom, ..default() },
            ));
            // `GlobalText[456]` = "Ver %s" con la versión del ejecutable.
            root.spawn((
                line(format!("Ver {}", env!("CARGO_PKG_VERSION"))),
                Node { position_type: PositionType::Absolute, left: Val::Px(0.0), bottom, ..default() },
            ));

            root.spawn((
                WindowsLayer,
                Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
            ));
        });
}

// --- Dibujo de las ventanas ------------------------------------------------------------------------

/// Una pieza de lo que hay que dibujar, en píxeles de la pantalla de referencia.
#[derive(Clone, PartialEq, Debug)]
enum Item {
    /// Imagen (o el trozo `src`) en un rectángulo; `tile` repite la textura en vez de estirarla.
    Image { tex: Tex, x: f32, y: f32, w: f32, h: f32, src: Option<Rect>, tile: bool },
    /// Rectángulo de color (`RenderColor`).
    Fill { x: f32, y: f32, w: f32, h: f32, color: [u8; 4] },
    /// Texto con la esquina de arriba a la izquierda en (x, y); con `center`, centrado en ese ancho.
    Text { x: f32, y: f32, center: Option<f32>, text: String, color: [u8; 3] },
    /// Caja de texto: lo que hay antes y después del cursor, si está todo seleccionado y si se ve el cursor.
    Input { x: f32, y: f32, w: f32, h: f32, before: String, after: String, selected: bool, caret: bool },
}

impl LoginUi {
    /// Lo que se dibuja, de la ventana de atrás a la de delante (`CUIMng::Render`).
    fn items(&self, s: Screen) -> Vec<Item> {
        let mut out = Vec::new();
        for &w in self.order.iter().rev() {
            if !self.is_shown(w) {
                continue;
            }
            match w {
                Win::MainBar => {
                    // `m_sprDeco` con el punto de anclaje en (105, 59), detrás del botón Credit.
                    let c = credit_rect(s);
                    out.push(image(Tex::Deco, IRect::new(c.x - 105, c.y - 59, 189, 103)));
                    self.push_buttons(s, w, &mut out);
                }
                Win::ServerSel => {
                    self.push_buttons(s, w, &mut out);
                    if let Some(g) = self.selected() {
                        // `CGaugeBar` de 160×4: se ve la parte izquierda, `percent` % de la textura.
                        let n = g.servers.len().min(SSW_SERVER_MAX as usize);
                        for (j, info) in g.servers.iter().take(n).enumerate() {
                            let r = server_rect(s, j, n);
                            let fill = (info.percent.clamp(0, 100) * GAUGE_W / 100) as f32;
                            if fill > 0.0 {
                                out.push(Item::Image {
                                    tex: Tex::ServerGauge,
                                    x: (r.x + GAUGE_X) as f32,
                                    y: (r.y + GAUGE_Y) as f32,
                                    w: fill,
                                    h: GAUGE_H as f32,
                                    src: Some(Rect::new(0.0, 0.0, fill, GAUGE_H as f32)),
                                    tile: false,
                                });
                            }
                        }
                        if g.pvp {
                            // En la pantalla de 640×480, no en la ventana (ServerSelWin.cpp:491-497).
                            for (i, text) in TEXT_PVP.iter().enumerate() {
                                out.push(Item::Text {
                                    x: 90.0 * s.rate_x(),
                                    y: (104.0 + 15.0 * i as f32) * s.rate_y(),
                                    center: None,
                                    text: text.to_string(),
                                    color: WHITE,
                                });
                            }
                        }
                    }
                }
                Win::Login => self.push_login(s, &mut out),
                Win::SysMenu => {
                    out.push(Item::Fill { x: 0.0, y: 0.0, w: s.w as f32, h: s.h as f32, color: [0, 0, 0, 128] });
                    // `CWinEx`: piedra repetida dentro (a 3 px del borde), cabecera, pie y laterales.
                    let r = sys_rect(s);
                    let side = r.h - 64 - 43;
                    out.push(tiled(Tex::SysStone, IRect::new(r.x + 3, r.y + 3, r.w - 6, r.h - 6)));
                    out.push(image(Tex::SysTop, IRect::new(r.x, r.y, 213, 64)));
                    out.push(image(Tex::SysBottom, IRect::new(r.x, r.y + 64 + side, 213, 43)));
                    out.push(tiled(Tex::SysLeft, IRect::new(r.x, r.y + 64, 5, side)));
                    out.push(tiled(Tex::SysRight, IRect::new(r.x + r.w - 5, r.y + 64, 5, side)));
                    self.push_buttons(s, w, &mut out);
                }
                Win::Msg => {
                    out.push(Item::Fill { x: 0.0, y: 0.0, w: s.w as f32, h: s.h as f32, color: [0, 0, 0, 128] });
                    let m = msg_rect(s);
                    out.push(image(Tex::MsgBack, m));
                    // `CMsgWin::RenderControls` (MsgWin.cpp:216-269).
                    let with_buttons = self.msg.kind != MsgKind::None;
                    if self.msg.lines.len() == 1 {
                        let y = m.y + if with_buttons { 38 } else { 54 };
                        out.push(Item::Text {
                            x: s.snap_x(m.x as f32),
                            y: s.snap_y(y as f32),
                            center: Some(m.w as f32),
                            text: self.msg.lines[0].clone(),
                            color: WHITE,
                        });
                    } else {
                        let ys = if with_buttons { [32, 51] } else { [44, 66] };
                        for (line, dy) in self.msg.lines.iter().zip(ys) {
                            out.push(Item::Text {
                                x: s.snap_x((m.x + 25) as f32),
                                y: s.snap_y((m.y + dy) as f32),
                                center: None,
                                text: line.clone(),
                                color: WHITE,
                            });
                        }
                    }
                    self.push_buttons(s, w, &mut out);
                }
            }
        }
        out
    }

    fn push_buttons(&self, s: Screen, w: Win, out: &mut Vec<Item>) {
        for b in self.buttons(s, w) {
            let state = self.button_state(&b, w);
            let frame = match (b.checked, b.check_frame) {
                (true, Some(f)) => f,
                _ => match state {
                    BtnState::Up => b.frames[0],
                    BtnState::Down => b.frames[1],
                    BtnState::Active => b.frames[2],
                },
            };
            let (x, y, w, h) = (b.rect.x as f32, b.rect.y as f32, b.rect.w as f32, b.rect.h as f32);
            let top = frame as f32 * h;
            out.push(Item::Image { tex: b.tex, x, y, w, h, src: Some(Rect::new(0.0, top, w, top + h)), tile: false });
            if let Some((text, colors)) = &b.text {
                // `CButton::Render` (Button.cpp:200-209): centrado en el ancho, a media altura y
                // medio punto más abajo (punto y medio al apretarlo) en la pantalla de 640×480.
                let (color, add) = match state {
                    BtnState::Up => (colors[0], 0.5),
                    BtnState::Down => (colors[1], 1.5),
                    BtnState::Active => (colors[2], 0.5),
                };
                let ty = (((y + (h - FIX_FONT) / 2.0) / s.rate_y()) + add).floor() * s.rate_y();
                out.push(Item::Text { x: s.snap_x(x), y: ty, center: Some(w), text: text.clone(), color });
            }
        }
    }

    /// `CLoginWin::RenderControls` (LoginWin.cpp:162-199).
    fn push_login(&self, s: Screen, out: &mut Vec<Item>) {
        let l = login_rect(s);
        out.push(image(Tex::LoginBack, l));
        self.push_buttons(s, Win::Login, out);
        for field in [Field::Id, Field::Pass] {
            out.push(image(Tex::LoginField, field_rect(s, field)));
        }
        for field in [Field::Id, Field::Pass] {
            let b = self.field(field);
            let shown = b.shown();
            let caret_at = shown.char_indices().nth(b.caret).map_or(shown.len(), |(i, _)| i);
            let focused = self.focus == Some(field);
            let o = input_origin(s, field);
            out.push(Item::Input {
                x: o.x,
                y: o.y,
                w: INPUT_W * s.rate_x(),
                h: INPUT_H * s.rate_y(),
                before: shown[..caret_at].to_string(),
                after: shown[caret_at..].to_string(),
                selected: focused && b.select_all,
                caret: focused && (self.caret_clock % CARET_PERIOD) < CARET_PERIOD / 2.0,
            });
        }
        let label = |dx: i32, dy: i32, text: String| Item::Text {
            x: s.snap_x((l.x + dx) as f32),
            y: s.snap_y((l.y + dy) as f32),
            center: None,
            text,
            color: WHITE,
        };
        out.push(label(30, 113, TEXT_ACCOUNT.into()));
        out.push(label(30, 139, TEXT_PASSWORD.into()));
        // `GlobalText[461]` "[%s-%d Server]" o `[460]` "[%s-%d(Non-PvP) Server]".
        if let Some((name, index, non_pvp)) = &self.selected_server {
            let title = match non_pvp {
                1 => format!("[{name}-{index}(Non-PvP) Server]"),
                2 => format!("[{name}-{index}(Gold PvP) Server]"),
                3 => format!("[{name}-{index}(Gold) Server]"),
                _ => format!("[{name}-{index} Server]"),
            };
            out.push(label(111, 80, title));
        }
    }
}

fn image(tex: Tex, r: IRect) -> Item {
    Item::Image { tex, x: r.x as f32, y: r.y as f32, w: r.w as f32, h: r.h as f32, src: None, tile: false }
}

fn tiled(tex: Tex, r: IRect) -> Item {
    Item::Image { tex, x: r.x as f32, y: r.y as f32, w: r.w as f32, h: r.h as f32, src: None, tile: true }
}

fn absolute(x: f32, y: f32, w: f32, h: f32) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: Val::Px(x * K),
        top: Val::Px(y * K),
        width: Val::Px(w * K),
        height: Val::Px(h * K),
        ..default()
    }
}

fn spawn_item(p: &mut ChildSpawnerCommands, item: &Item, textures: &LoginTextures) {
    match item {
        Item::Image { tex, x, y, w, h, src, tile } => {
            let Some(handle) = textures.0.get(tex) else { return };
            let mut img = ImageNode::new(handle.clone());
            img.rect = *src;
            if *tile {
                // Una repetición por cada píxel de la textura del original.
                img.image_mode = NodeImageMode::Tiled { tile_x: true, tile_y: true, stretch_value: K };
            }
            p.spawn((absolute(*x, *y, *w, *h), img));
        }
        Item::Fill { x, y, w, h, color: [r, g, b, a] } => {
            p.spawn((absolute(*x, *y, *w, *h), BackgroundColor(Color::srgba_u8(*r, *g, *b, *a))));
        }
        Item::Text { x, y, center, text, color } => {
            let label = (Text::new(text.clone()), text_font(FIX_FONT), TextColor(srgb(*color)), TextLayout::no_wrap());
            match center {
                Some(w) => {
                    let mut node = absolute(*x, *y, *w, FIX_FONT);
                    node.justify_content = JustifyContent::Center;
                    p.spawn(node).with_children(|c| {
                        c.spawn(label);
                    });
                }
                None => {
                    p.spawn((
                        Node { position_type: PositionType::Absolute, left: Val::Px(x * K), top: Val::Px(y * K), ..default() },
                        label,
                    ));
                }
            }
        }
        Item::Input { x, y, w, h, before, after, selected, caret } => {
            let mut node = absolute(*x, *y, *w, *h);
            node.flex_direction = FlexDirection::Row;
            node.overflow = Overflow::clip();
            let selected = *selected;
            let caret = *caret;
            p.spawn(node).with_children(|row| {
                let part = |text: &str| {
                    (Text::new(text.to_string()), text_font(FIX_FONT), TextColor(srgb(INPUT_COLOR)), TextLayout::no_wrap())
                };
                let mut first = row.spawn(part(before));
                if selected {
                    // `m_dwSelectBackColor` (150, 150, 150).
                    first.insert(BackgroundColor(Color::srgb_u8(150, 150, 150)));
                }
                // El cursor no ocupa sitio: 2 px de ancho y el alto de la letra, en gris (200, 200, 200).
                row.spawn(Node { width: Val::Px(0.0), height: Val::Percent(100.0), ..default() }).with_children(|c| {
                    if caret {
                        c.spawn((absolute(0.0, 0.0, 2.0, FIX_FONT), BackgroundColor(Color::srgb_u8(200, 200, 200))));
                    }
                });
                row.spawn(part(after));
            });
        }
    }
}

/// Rehace las ventanas cuando cambia algo de lo que se dibuja.
fn draw_windows(
    mut commands: Commands,
    ui: Res<LoginUi>,
    textures: Res<LoginTextures>,
    windows: Query<&Window>,
    scale: Res<UiScale>,
    layer: Query<Entity, With<WindowsLayer>>,
    mut last: Local<(Option<Entity>, Vec<Item>)>,
) {
    let Ok(layer) = layer.single() else { return };
    let (_, s) = screen_size(windows.single().ok(), scale.0);
    let items = ui.items(s);
    // La capa es otra cada vez que se entra en el login.
    if last.0 == Some(layer) && last.1 == items {
        return;
    }
    commands.entity(layer).despawn_children();
    commands.entity(layer).with_children(|p| {
        for item in &items {
            spawn_item(p, item, &textures);
        }
    });
    *last = (Some(layer), items);
}
