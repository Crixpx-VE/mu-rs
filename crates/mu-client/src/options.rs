//! Opciones y menú de sistema dentro del juego (interfaz nueva del S6, `SEASON3B::CNewUI*`).
//!
//! - [`GameOptions`]: lo que guarda `g_pOption` (`CNewUIOptionWindow`), compartido con la ventana
//!   de opciones del login (`COptionWin`, login_ui.rs). El volumen de los efectos y el de la
//!   música son los de sound.rs (se guardan en `mu-launcher.txt`).
//! - Botón de menú de la barra → menú de ventanas (`CNewUIWindowMenu`) → "System (Esc)" → caja
//!   del menú de sistema (`CSystemMenuMsgBox`) → "Option" → ventana de opciones
//!   (`CNewUIOptionWindow`). Esc abre la caja del menú de sistema (`CNewUIHotKey::UpdateKeyEvent`)
//!   y cierra lo que esté abierto.
//!
//! **Coordenadas:** las de la pantalla de 640×480 del original, centrada y pegada abajo como la
//! barra (mainframe.rs).
//!
//! **Sin servidor:** "Exit Game" cierra el programa, "Select Server" vuelve al login y "Switch
//! Character" a la selección de personaje (el original se lo pide al servidor con
//! `SendRequestLogOut`). Del menú de ventanas solo hace algo "System"; ayuda, gremio, mover,
//! mapa y Gens no existen todavía. Ataque automático, aviso de susurros, ayuda deslizante y
//! límite de efectos se ven y se guardan en memoria, pero de momento no cambian nada.
//!
//! **Añadido:** la ventana del original solo tiene el volumen de los efectos; aquí lleva debajo
//! una fila más, igual, para el de la música (y la ventana es 40 unidades más alta).

use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;
use bevy::text::{FontSize, FontSmoothing, LineHeight};

use crate::Args;
use crate::scene::{GameScene, LoadStep};
use crate::sound::SoundQueue;

pub struct OptionsPlugin;

impl Plugin for OptionsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameOptions>()
            .init_resource::<GameUi>()
            .add_systems(OnEnter(GameScene::Game), spawn_game_ui)
            .add_systems(
                OnExit(GameScene::Game),
                (crate::scene::despawn::<GameUiRoot>, |mut ui: ResMut<GameUi>| *ui = GameUi::default()),
            )
            .add_systems(Update, (update_game_ui, draw_game_ui).chain().run_if(in_state(GameScene::Game)));
    }
}

/// Opciones de `g_pOption` (valores iniciales de `CNewUIOptionWindow`, NewUIOptionWindow.cpp:17-28).
/// El original las manda al servidor al salir (`SaveOptions`); aquí duran lo que dura el programa.
#[derive(Resource, Clone, Copy, PartialEq, Debug)]
pub struct GameOptions {
    pub auto_attack: bool,
    pub whisper_sound: bool,
    pub slide_help: bool,
    /// Límite de efectos (`m_iRenderLevel`, 0 a 4).
    pub render_level: u8,
}

impl Default for GameOptions {
    fn default() -> Self {
        Self { auto_attack: true, whisper_sound: false, slide_help: true, render_level: 4 }
    }
}

// --- Medidas (pantalla de 640×480) -------------------------------------------------------------

/// `g_hFont` a 1024×768: 13 px, en unidades de la pantalla de 480 de alto (como login_ui.rs).
const FONT: f32 = 13.0 * 480.0 / 768.0;
/// Suavizado de la letra (el original no lo tenía; con la Noto Sans de `fonts.rs` se ve nítida).
const SMOOTHING: FontSmoothing = FontSmoothing::AntiAliased;

/// `CNewUIWindowMenu`: `STANDARD_POS_X/Y` con `MENU_MAX_INDEX` = 6 (NewUIWindowMenu.cpp:56-60).
const MENU_ITEMS: usize = 6;
const MENU_X: f32 = 640.0 - 112.0;
const MENU_Y: f32 = 480.0 - 156.0 - 20.0 * (MENU_ITEMS as f32 - 4.0);
const MENU_W: f32 = 112.0;
/// `GlobalText[1741, 1742, 364, 1743, 3055, 3103]`.
const MENU_TEXT: [&str; MENU_ITEMS] = ["System (Esc)", "Help (F1)", "Guild (G)", "Move (M)", "Map (Tab)", "Gens Info (B)"];

/// `CSystemMenuMsgBox::Create` (NewUICustomMessageBox.cpp:2225-2241): `MSGBOX_WIDTH` 230,
/// arriba 67 + 5 × 15 + abajo 50, centrada y en y = 100.
const SYS_W: f32 = 230.0;
const SYS_H: f32 = 67.0 + 5.0 * 15.0 + 50.0;
const SYS_X: f32 = 640.0 / 2.0 - SYS_W / 2.0;
const SYS_Y: f32 = 100.0;
/// Botones `MSGBOX_BTN_EMPTY` de 108×29, cada 30 desde y + 23 (`SetButtonInfo`).
const SYS_BTN_W: f32 = 108.0;
const SYS_BTN_H: f32 = 29.0;
/// `GlobalText[381, 382, 383, 385, 384]`.
const SYS_TEXT: [&str; 5] = ["Exit Game", "Select Server", "Switch Character", "Option", "Cancel"];

/// `CNewUIOptionWindow` en (640/2 − 190/2, 70) (NewUISystem.cpp:107-108): 190 × (249 + 40).
const OPT_X: f32 = 640.0 / 2.0 - 190.0 / 2.0;
const OPT_Y: f32 = 70.0;
const OPT_W: f32 = 190.0;
/// Las 40 unidades de la fila de la música (añadida).
const MUSIC_ROW: f32 = 40.0;
const OPT_H: f32 = 249.0 + MUSIC_ROW;

// --- Estado ------------------------------------------------------------------------------------

/// `m_EventState` de un botón de la caja de mensajes (`CNewUIMessageBoxButton`).
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
enum BtnEvent {
    #[default]
    None,
    Hover,
    Down,
}

/// Ventanas del juego abiertas (`g_pNewUISystem->IsVisible`).
#[derive(Resource, Default)]
pub struct GameUi {
    /// `INTERFACE_WINDOW_MENU` y su fila elegida (`m_iSelectedIndex`).
    window_menu: bool,
    menu_selected: Option<usize>,
    /// La caja del menú de sistema (`CSystemMenuMsgBox`).
    sys_menu: bool,
    sys_btn: [BtnEvent; 5],
    /// `EVENT_WND_MOUSE_LBUTTON_DOWN`: se apretó dentro de la caja.
    sys_armed: bool,
    /// `INTERFACE_OPTION` y el estado de su botón de cerrar (0 normal, 1 encima, 2 apretado).
    option: bool,
    close_frame: u8,
}

impl GameUi {
    /// El menú de ventanas está abierto (para el fotograma del botón de la barra).
    pub fn window_menu_open(&self) -> bool {
        self.window_menu
    }

    /// `g_pNewUISystem->Toggle(INTERFACE_WINDOW_MENU)` (NewUIMainFrameWindow.cpp:817-821).
    pub fn toggle_window_menu(&mut self) {
        self.window_menu = !self.window_menu;
        // `OpenningProcess`.
        self.menu_selected = None;
    }

    /// Hay una caja de mensajes: se lleva todo el ratón.
    pub fn modal(&self) -> bool {
        self.sys_menu
    }

    /// El ratón (pantalla de 640×480) cae en una ventana que se lo queda: un clic ahí no hace
    /// caminar (lo que devuelven en falso los `UpdateMouseEvent`).
    pub fn blocks(&self, c: Vec2) -> bool {
        self.sys_menu
            || (self.option && inside(c, OPT_X, OPT_Y, OPT_W, OPT_H))
            || (self.window_menu && inside(c, MENU_X, MENU_Y + 20.0, MENU_W, 95.0 + 20.0 * (MENU_ITEMS as f32 - 5.0)))
    }
}

/// `CheckMouseIn(x, y, ancho, alto)`.
fn inside(c: Vec2, x: f32, y: f32, w: f32, h: f32) -> bool {
    c.x >= x && c.x < x + w && c.y >= y && c.y < y + h
}

// --- Lógica ------------------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn update_game_ui(
    mut ui: ResMut<GameUi>,
    mut opts: ResMut<GameOptions>,
    windows: Query<&Window>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    scroll: Res<AccumulatedMouseScroll>,
    audio: Option<Res<crate::sound::Audio>>,
    mut sound: ResMut<SoundQueue>,
    mut args: ResMut<Args>,
    mut load: ResMut<LoadStep>,
    mut next: ResMut<NextState<GameScene>>,
    mut exit: MessageWriter<AppExit>,
    mut net: ResMut<crate::net::Net>,
) {
    let cursor = windows.single().ok().and_then(crate::mainframe::cursor_virtual);
    let (press, held, release) =
        (mouse.just_pressed(MouseButton::Left), mouse.pressed(MouseButton::Left), mouse.just_released(MouseButton::Left));
    let at = |x: f32, y: f32, w: f32, h: f32| cursor.is_some_and(|c| inside(c, x, y, w, h));

    // Teclado (`UpdateKeyEvent`): primero la caja de mensajes, luego opciones y menú de ventanas,
    // y por último las teclas rápidas.
    if keys.just_pressed(KeyCode::Escape) {
        if ui.sys_menu {
            // `CancelBtnDown` (MSGBOX_EVENT_PRESSKEY_ESC).
            close_sys_menu(&mut ui);
        } else if ui.option {
            ui.option = false;
        } else if ui.window_menu {
            ui.window_menu = false;
        } else {
            open_sys_menu(&mut ui);
        }
        sound.play("SOUND_CLICK01");
    } else if keys.just_pressed(KeyCode::KeyU) && !ui.sys_menu {
        // `CNewUIHotKey::UpdateKeyEvent`: `U` abre o cierra el menú de ventanas.
        ui.window_menu = !ui.window_menu;
        sound.play("SOUND_CLICK01");
    }

    if ui.sys_menu {
        // `CSystemMenuMsgBox::Update` (NewUICustomMessageBox.cpp:2248-2256) y los botones
        // (`CNewUIMessageBoxButton::Update`, NewUICommonMessageBox.cpp:139-172).
        for (i, st) in ui.sys_btn.iter_mut().enumerate() {
            let (x, y) = sys_button(i);
            let over = at(x, y, SYS_BTN_W, SYS_BTN_H);
            *st = match *st {
                BtnEvent::None if !held && over => BtnEvent::Hover,
                BtnEvent::Hover if !held && !over => BtnEvent::None,
                BtnEvent::Hover if held && over => BtnEvent::Down,
                BtnEvent::Down if !held => BtnEvent::None,
                s => s,
            };
        }
        // `CNewUIMessageBoxMng::UpdateMouseEvent`: se apretó en la caja y se suelta en ella.
        let in_box = at(SYS_X, SYS_Y, SYS_W, SYS_H);
        if press && in_box {
            ui.sys_armed = true;
        }
        if release {
            let armed = std::mem::take(&mut ui.sys_armed);
            if armed && in_box {
                let clicked = (0..SYS_TEXT.len()).find(|&i| {
                    let (x, y) = sys_button(i);
                    at(x, y, SYS_BTN_W, SYS_BTN_H)
                });
                if let Some(i) = clicked {
                    sound.play("SOUND_CLICK01");
                    close_sys_menu(&mut ui);
                    use mu_protocol::game::{ClientMessage, logout_kind};
                    match i {
                        // `GameOverBtnDown`: `SendRequestLogOut(0)` y se sale.
                        0 => {
                            net.send(ClientMessage::Logout { kind: logout_kind::CLOSE_GAME });
                            net.close();
                            exit.write(AppExit::Success);
                        }
                        // `ChooseServerBtnDown`: `SendRequestLogOut(2)` → al login, sin conexión.
                        1 => {
                            net.send(ClientMessage::Logout { kind: logout_kind::BACK_TO_SERVER_SELECTION });
                            net.close();
                            crate::scene::load_world(&mut args, &mut load, crate::scene::LOGIN_WORLD, false, GameScene::Login);
                            next.set(GameScene::Loading);
                        }
                        // `ChooseCharacterBtnDown`: `SendRequestLogOut(1)` → a la selección (la
                        // conexión sigue; al entrar se pide otra vez la lista).
                        2 => {
                            net.send(ClientMessage::Logout { kind: logout_kind::BACK_TO_CHARACTER_SELECTION });
                            crate::scene::load_world(&mut args, &mut load, crate::scene::CHARACTER_WORLD, false, GameScene::CharacterSelect);
                            next.set(GameScene::Loading);
                        }
                        // `OptionBtnDown`.
                        3 => ui.option = true,
                        _ => {}
                    }
                }
            }
        }
        return;
    }

    // `CNewUIOptionWindow::UpdateMouseEvent` (NewUIOptionWindow.cpp:76-153), con la fila de la
    // música añadida debajo del volumen.
    let mut used = false;
    if ui.option {
        let (x, y) = (OPT_X, OPT_Y);
        // `m_BtnClose` (`CNewUIBaseButton::Process`): encima, apretado o clic al soltar.
        let over = at(x + 68.0, y + 209.0 + MUSIC_ROW, 54.0, 30.0);
        ui.close_frame = if over && held { 2 } else if over && !release { 1 } else { 0 };
        if over && release {
            ui.option = false;
            ui.close_frame = 0;
            used = true;
        } else {
            if press && at(x + 150.0, y + 43.0, 15.0, 15.0) {
                opts.auto_attack = !opts.auto_attack;
            }
            if press && at(x + 150.0, y + 65.0, 15.0, 15.0) {
                opts.whisper_sound = !opts.whisper_sound;
            }
            if press && at(x + 150.0, y + 127.0 + MUSIC_ROW, 15.0, 15.0) {
                opts.slide_help = !opts.slide_help;
            }
            let (sfx, music) = crate::sound::volumes(audio.as_deref(), &sound);
            if let Some(v) = volume_bar(cursor, x, y + 104.0, sfx, scroll.delta.y, held) {
                // `SetEffectVolumeLevel`.
                sound.set_sfx_volume(v);
            }
            if let Some(v) = volume_bar(cursor, x, y + 104.0 + MUSIC_ROW, music, scroll.delta.y, held) {
                sound.set_music_volume(v);
            }
            if held && at(x + 25.0, y + 168.0 + MUSIC_ROW, 141.0, 29.0) {
                let dx = cursor.map_or(0.0, |c| c.x.floor()) - (x + 25.0);
                opts.render_level = ((5.0 * dx / 141.0) as i32).clamp(0, 4) as u8;
            }
            used = at(x, y, OPT_W, OPT_H);
        }
    }

    // `CNewUIWindowMenu::UpdateMouseEvent` (NewUIWindowMenu.cpp:62-139).
    if ui.window_menu && !used {
        let (x, y) = (MENU_X, MENU_Y);
        if let Some(i) = (0..MENU_ITEMS).find(|&i| at(x, y + 20.0 + 20.0 * i as f32, MENU_W, 20.0)) {
            ui.menu_selected = Some(i);
        }
        if let Some(i) = ui.menu_selected
            && release
        {
            // Solo "System" lleva a algo: ayuda, gremio, mover, mapa y Gens aún no existen.
            if i == 0 {
                ui.window_menu = false;
                open_sys_menu(&mut ui);
                return;
            }
        }
        if !at(x, y + 20.0, MENU_W, 95.0 + 20.0 * (MENU_ITEMS as f32 - 5.0)) {
            ui.menu_selected = None;
        }
    }
}

/// Barra de volumen de 124×16 en (x + 33, y) (NewUIOptionWindow.cpp:94-130): la rueda sube o
/// baja un punto y con el botón apretado va a donde está el ratón. Devuelve el nivel si cambia.
fn volume_bar(cursor: Option<Vec2>, x: f32, y: f32, level: u8, wheel: f32, held: bool) -> Option<u8> {
    let c = cursor?;
    if !inside(c, x + 33.0 - 8.0, y, 124.0 + 8.0, 16.0) {
        return None;
    }
    let mut v = level as i32;
    if wheel > 0.0 {
        v = (v + 1).min(10);
    } else if wheel < 0.0 {
        v = (v - 1).max(0);
    }
    if held {
        let dx = c.x.floor() - (x + 33.0);
        v = if dx < 0.0 { 0 } else { ((10.0 * dx / 124.0) as i32 + 1).min(10) };
    }
    (v != level as i32).then_some(v as u8)
}

/// `CreateMessageBox(CSystemMenuMsgBoxLayout)`.
fn open_sys_menu(ui: &mut GameUi) {
    ui.sys_menu = true;
    ui.sys_btn = [BtnEvent::None; 5];
    ui.sys_armed = false;
}

/// `MSGBOX_EVENT_DESTROY`.
fn close_sys_menu(ui: &mut GameUi) {
    ui.sys_menu = false;
    ui.sys_armed = false;
}

/// Esquina del botón `i` de la caja del menú de sistema.
fn sys_button(i: usize) -> (f32, f32) {
    (SYS_X + SYS_W / 2.0 - SYS_BTN_W / 2.0, SYS_Y + 23.0 + 30.0 * i as f32)
}

// --- Dibujo ------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Tex {
    MsgBack,
    MsgTop,
    MsgMiddle,
    MsgBottom,
    BtnEmpty,
    BtnClose,
    InvBottom,
    OptTop,
    OptLeft,
    OptRight,
    OptLine,
    OptPoint,
    OptCheck,
    EffectBack,
    EffectColor,
    VolumeBack,
    VolumeColor,
    MenuTop,
    MenuMiddle,
    MenuBottom,
    MenuLine,
    ArrowL,
    ArrowR,
}

impl Tex {
    const ALL: [Tex; 23] = [
        Tex::MsgBack,
        Tex::MsgTop,
        Tex::MsgMiddle,
        Tex::MsgBottom,
        Tex::BtnEmpty,
        Tex::BtnClose,
        Tex::InvBottom,
        Tex::OptTop,
        Tex::OptLeft,
        Tex::OptRight,
        Tex::OptLine,
        Tex::OptPoint,
        Tex::OptCheck,
        Tex::EffectBack,
        Tex::EffectColor,
        Tex::VolumeBack,
        Tex::VolumeColor,
        Tex::MenuTop,
        Tex::MenuMiddle,
        Tex::MenuBottom,
        Tex::MenuLine,
        Tex::ArrowL,
        Tex::ArrowR,
    ];

    /// `CNewUIMessageBoxMng::LoadImages` (NewUIMessageBox.cpp:449-462),
    /// `CNewUIOptionWindow::LoadImages` (NewUIOptionWindow.cpp:206-221) y
    /// `CNewUIWindowMenu::LoadImages` (NewUIWindowMenu.cpp:236-245).
    fn file(self) -> &'static str {
        match self {
            Tex::MsgBack => "newui_msgbox_back.jpg",
            Tex::MsgTop => "newui_msgbox_top.tga",
            Tex::MsgMiddle => "newui_msgbox_middle.tga",
            Tex::MsgBottom => "newui_msgbox_bottom.tga",
            Tex::BtnEmpty => "newui_btn_empty.tga",
            Tex::BtnClose => "newui_button_close.tga",
            Tex::InvBottom => "newui_item_back03.tga",
            Tex::OptTop => "newui_option_top.tga",
            Tex::OptLeft => "newui_option_back06(L).tga",
            Tex::OptRight => "newui_option_back06(R).tga",
            Tex::OptLine => "newui_option_line.jpg",
            Tex::OptPoint => "newui_option_point.tga",
            Tex::OptCheck => "newui_option_check.tga",
            Tex::EffectBack => "newui_option_effect03.tga",
            Tex::EffectColor => "newui_option_effect04.tga",
            Tex::VolumeBack => "newui_option_volume01.tga",
            Tex::VolumeColor => "newui_option_volume02.tga",
            Tex::MenuTop => "newui_commamd01.tga",
            Tex::MenuMiddle => "newui_commamd02.tga",
            Tex::MenuBottom => "newui_commamd03.tga",
            Tex::MenuLine => "newui_commamd_Line.jpg",
            Tex::ArrowL => "newui_arrow(L).tga",
            Tex::ArrowR => "newui_arrow(R).tga",
        }
    }
}

#[derive(Resource)]
struct GameUiTextures(std::collections::HashMap<Tex, Handle<Image>>);

/// Una pieza en la pantalla de 640×480.
#[derive(Clone, PartialEq, Debug)]
enum Piece {
    /// `RenderImage(tex, x, y, ancho, alto, su, sv)`: el trozo de ancho × alto de la textura
    /// desde (su, sv), sin estirar.
    Image { tex: Tex, x: f32, y: f32, w: f32, h: f32, sv: f32 },
    /// Texto con la esquina de arriba a la izquierda en (x, y); con `center`, centrado en ese
    /// ancho (y en ese alto si lo hay).
    Text { x: f32, y: f32, center: Option<(f32, f32)>, text: String, color: [u8; 3] },
}

fn img(tex: Tex, x: f32, y: f32, w: f32, h: f32) -> Piece {
    Piece::Image { tex, x, y, w, h, sv: 0.0 }
}

fn text(x: f32, y: f32, s: &str) -> Piece {
    Piece::Text { x, y, center: None, text: s.to_string(), color: [255, 255, 255] }
}

impl GameUi {
    /// Lo que se dibuja, de atrás a delante (`GetLayerDepth`: menú 10.4, opciones 10.5 y la caja
    /// de mensajes encima de todo).
    fn pieces(&self, opts: &GameOptions, sfx: u8, music: u8) -> Vec<Piece> {
        let mut out = Vec::new();
        if self.window_menu {
            self.push_window_menu(&mut out);
        }
        if self.option {
            self.push_option(&mut out, opts, sfx, music);
        }
        if self.sys_menu {
            self.push_sys_menu(&mut out);
        }
        out
    }

    /// `CNewUIWindowMenu::Render` (NewUIWindowMenu.cpp:170-234).
    fn push_window_menu(&self, out: &mut Vec<Piece>) {
        let (x, y) = (MENU_X, MENU_Y);
        let h = 105.0 + (MENU_ITEMS as f32 - 4.0) * 20.0;
        out.push(img(Tex::MsgBack, x, y, MENU_W, h));
        out.push(img(Tex::MenuTop, x, y, MENU_W, 45.0));
        let middles = ((h - 90.0) / 15.0) as i32 + 1;
        for i in 0..middles {
            out.push(img(Tex::MenuMiddle, x, y + 45.0 + 15.0 * i as f32, MENU_W, 15.0));
        }
        out.push(img(Tex::MenuBottom, x, 480.0 - 50.0 - 45.0, MENU_W, 45.0));
        for i in 0..MENU_ITEMS - 1 {
            out.push(img(Tex::MenuLine, x + 15.0, y + 33.0 + 20.0 * i as f32, 82.0, 2.0));
        }
        for (i, t) in MENU_TEXT.iter().enumerate() {
            let color = if self.menu_selected == Some(i) { [255, 255, 0] } else { [255, 255, 255] };
            out.push(Piece::Text { x, y: y + 20.0 + 20.0 * i as f32, center: Some((MENU_W, 0.0)), text: t.to_string(), color });
        }
        if let Some(i) = self.menu_selected {
            let ay = y + 20.0 + 20.0 * i as f32;
            out.push(img(Tex::ArrowL, x + 16.0, ay, 6.0, 9.0));
            out.push(img(Tex::ArrowR, x + 90.0, ay, 6.0, 9.0));
        }
    }

    /// `CNewUIOptionWindow::Render` (NewUIOptionWindow.cpp:175-335) con la fila de la música.
    fn push_option(&self, out: &mut Vec<Piece>, opts: &GameOptions, sfx: u8, music: u8) {
        let (x, y) = (OPT_X, OPT_Y);
        let m = MUSIC_ROW;
        // `RenderFrame`: 14 + 4 trozos de los lados de 10 de alto.
        out.push(img(Tex::MsgBack, x, y, OPT_W, OPT_H));
        out.push(img(Tex::OptTop, x, y, OPT_W, 64.0));
        let sides = 14 + (MUSIC_ROW / 10.0) as i32;
        for i in 0..sides {
            let sy = y + 64.0 + 10.0 * i as f32;
            out.push(img(Tex::OptLeft, x, sy, 21.0, 10.0));
            out.push(img(Tex::OptRight, x + OPT_W - 21.0, sy, 21.0, 10.0));
        }
        out.push(img(Tex::InvBottom, x, y + 64.0 + 10.0 * sides as f32, OPT_W, 45.0));
        for ly in [60.0, 82.0, 122.0, 122.0 + m, 144.0 + m] {
            out.push(img(Tex::OptLine, x + 18.0, y + ly, 154.0, 2.0));
        }
        // `RenderContents`.
        for py in [46.0, 68.0, 90.0, 90.0 + m, 130.0 + m, 152.0 + m] {
            out.push(img(Tex::OptPoint, x + 20.0, y + py, 10.0, 10.0));
        }
        // `GlobalText[386, 387, 389, 919, 1840]`; "Music" es de la fila añadida.
        for (ty, t) in [
            (48.0, "Automatic Attack"),
            (70.0, "Beep sound for whispering"),
            (92.0, "Volume"),
            (92.0 + m, "Music"),
            (132.0 + m, "Slide Help"),
            (154.0 + m, "+Effect limitation"),
        ] {
            out.push(text(x + 40.0, y + ty, t));
        }
        // `RenderButtons`.
        let close = 54.0;
        out.push(Piece::Image { tex: Tex::BtnClose, x: x + 68.0, y: y + 209.0 + m, w: close, h: 30.0, sv: self.close_frame as f32 * 30.0 });
        for (on, cy) in [(opts.auto_attack, 43.0), (opts.whisper_sound, 65.0), (opts.slide_help, 127.0 + m)] {
            out.push(Piece::Image { tex: Tex::OptCheck, x: x + 150.0, y: y + cy, w: 15.0, h: 15.0, sv: if on { 0.0 } else { 15.0 } });
        }
        for (level, vy) in [(sfx, 104.0), (music, 104.0 + m)] {
            out.push(img(Tex::VolumeBack, x + 33.0, y + vy, 124.0, 16.0));
            if level > 0 {
                out.push(img(Tex::VolumeColor, x + 33.0, y + vy, 124.0 * 0.1 * level as f32, 16.0));
            }
        }
        out.push(img(Tex::EffectBack, x + 25.0, y + 168.0 + m, 141.0, 29.0));
        out.push(img(Tex::EffectColor, x + 25.0, y + 168.0 + m, 141.0 * 0.2 * (opts.render_level as f32 + 1.0), 29.0));
    }

    /// `CSystemMenuMsgBox::Render` (NewUICustomMessageBox.cpp:2258-2296).
    fn push_sys_menu(&self, out: &mut Vec<Piece>) {
        let (x, y) = (SYS_X, SYS_Y);
        // `MSGBOX_BACK_BLANK_WIDTH/HEIGHT` = 8 y 10.
        out.push(img(Tex::MsgBack, x, y + 2.0, SYS_W - 8.0, SYS_H - 10.0));
        out.push(img(Tex::MsgTop, x, y, SYS_W, 67.0));
        for i in 0..5 {
            out.push(img(Tex::MsgMiddle, x, y + 67.0 + 15.0 * i as f32, SYS_W, 15.0));
        }
        out.push(img(Tex::MsgBottom, x, y + 67.0 + 75.0, SYS_W, 50.0));
        for (i, t) in SYS_TEXT.iter().enumerate() {
            // `CNewUIMessageBoxButton::Render`: fotogramas de 29 de alto (normal, encima, apretado)
            // y el texto con `g_hFont` en blanco, centrado.
            let (bx, by) = sys_button(i);
            let frame = match self.sys_btn[i] {
                BtnEvent::None => 0.0,
                BtnEvent::Hover => 1.0,
                BtnEvent::Down => 2.0,
            };
            out.push(Piece::Image { tex: Tex::BtnEmpty, x: bx, y: by, w: SYS_BTN_W, h: SYS_BTN_H, sv: frame * SYS_BTN_H });
            out.push(Piece::Text { x: bx, y: by, center: Some((SYS_BTN_W, SYS_BTN_H)), text: t.to_string(), color: [255, 255, 255] });
        }
    }
}

#[derive(Component)]
struct GameUiRoot;

fn spawn_game_ui(mut commands: Commands, args: Res<Args>, mut images: ResMut<Assets<Image>>) {
    let textures = Tex::ALL
        .iter()
        .filter_map(|&t| crate::scene::interface_image(&args, &mut images, t.file()).map(|(h, _)| (t, h)))
        .collect();
    commands.insert_resource(GameUiTextures(textures));
    // La pantalla de 640×480 centrada y pegada abajo, como la barra; por encima de ella.
    commands.spawn((
        Name::new("Ventanas del juego"),
        GameUiRoot,
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

fn label(t: &str, color: [u8; 3]) -> impl Bundle {
    (
        Text::new(t.to_string()),
        TextFont { font_size: FontSize::Px(FONT), font_smoothing: SMOOTHING, ..default() },
        LineHeight::Px(FONT),
        TextColor(Color::srgb_u8(color[0], color[1], color[2])),
        TextLayout::no_wrap(),
    )
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

/// Rehace las ventanas cuando cambia algo de lo que se dibuja.
#[allow(clippy::too_many_arguments)]
fn draw_game_ui(
    mut commands: Commands,
    ui: Res<GameUi>,
    opts: Res<GameOptions>,
    audio: Option<Res<crate::sound::Audio>>,
    sound: Res<SoundQueue>,
    textures: Option<Res<GameUiTextures>>,
    root: Query<Entity, With<GameUiRoot>>,
    mut last: Local<(Option<Entity>, Vec<Piece>)>,
) {
    let (Ok(root), Some(textures)) = (root.single(), textures) else { return };
    let (sfx, music) = crate::sound::volumes(audio.as_deref(), &sound);
    let pieces = ui.pieces(&opts, sfx, music);
    if last.0 == Some(root) && last.1 == pieces {
        return;
    }
    commands.entity(root).despawn_children();
    commands.entity(root).with_children(|p| {
        for piece in &pieces {
            match piece {
                Piece::Image { tex, x, y, w, h, sv } => {
                    let Some(handle) = textures.0.get(tex) else { continue };
                    let mut image = ImageNode::new(handle.clone());
                    image.rect = Some(Rect::new(0.0, *sv, *w, sv + h));
                    p.spawn((absolute(*x, *y, *w, *h), image));
                }
                Piece::Text { x, y, center, text, color } => match center {
                    Some((w, h)) => {
                        let mut node = absolute(*x, *y, *w, if *h > 0.0 { *h } else { FONT });
                        node.justify_content = JustifyContent::Center;
                        node.align_items = if *h > 0.0 { AlignItems::Center } else { AlignItems::Start };
                        p.spawn(node).with_children(|c| {
                            c.spawn(label(text, *color));
                        });
                    }
                    None => {
                        p.spawn((
                            Node { position_type: PositionType::Absolute, left: Val::Px(*x), top: Val::Px(*y), ..default() },
                            label(text, *color),
                        ));
                    }
                },
            }
        }
    });
    *last = (Some(root), pieces);
}
