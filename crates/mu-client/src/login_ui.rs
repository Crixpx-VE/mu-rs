//! 2D interface of the login screen, like Main 5.2's (S6):
//!
//! - MU logo with its glow and the copyright and version lines (`NewRenderLogInScene`, ZzzScene.cpp);
//! - bottom bar with the Menu and Credit buttons (`CLoginMainWin`);
//! - server selection: groups and servers with their load bar (`CServerSelWin`);
//! - login window: account, password in asterisks, OK and Cancel (`CLoginWin`);
//! - message box (`CMsgWin`) and Esc system menu (`CSysMenuWin`);
//! - options (`COptionWin`) with their checkboxes and sliders (`CSlider`): the effects and music
//!   volumes go to sound.rs; the rest is stored in `options::GameOptions` (no effect
//!   yet). The music slider is an addition: the original only has the effects one;
//! - credits (`CCreditWin`) with `Data/Local/credit.bmd`, the fading `im*_*.jpg`
//!   illustrations and the `Mutheme` music;
//! - the window manager (`CUIMng`): active window, front-to-back order and buttons (`CButton`).
//!
//! **Coordinates.** The logo and the bottom lines live on the 640×480 screen stretched to the
//! window. The original's windows, however, use real window pixels, unscaled
//! (centered using the screen width and height). Here the pixels of a 1024×768 reference
//! window are used ([`REF_HEIGHT`]): at 1024×768 it matches the original pixel for pixel, and at
//! other resolutions it scales like the rest of the interface (1 Bevy UI px = 1 unit
//! of the 480-high virtual screen, see mainframe.rs). Texts use the original's same
//! conversions (`int(x / g_fScreenRate_x)`).
//!
//! **No server yet:** the server list is a single group, "mu-rs", with one server
//! (Non-PvP, 0 % load); choosing it opens the login window, which accepts any non-empty account and
//! password and moves on to character selection. The last user is saved in
//! `mu-launcher.txt` (`usuario=`).
//!
//! Logic runs in `Update` (buttons react on mouse release, like `CButton::Update`); the
//! logo fade at 25 ticks per second, interpolated.

use std::collections::HashMap;

use bevy::asset::embedded_asset;
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

/// Character selection windows (create character, delete, that screen's messages).
#[path = "login_ui_char.rs"]
mod character;
use crate::sound::SoundQueue;
use crate::terrain::to_bevy_image;

pub struct LoginUiPlugin;

impl Plugin for LoginUiPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "login_logo.wesl");
        let ui_scene = || in_state(GameScene::Login).or_else(in_state(GameScene::CharacterSelect));
        let leave = |mut c: Commands, ui: Option<Res<LoginUi>>| {
            // The chosen server is kept for the character window (`g_ServerListManager`).
            if let Some((name, index, non_pvp)) = ui.and_then(|u| u.selected_server.clone()) {
                c.insert_resource(crate::char_info::SelectedServer { name, index, non_pvp });
            }
            c.remove_resource::<LoginUi>();
            c.remove_resource::<LoginTextures>();
        };
        app.add_plugins(UiMaterialPlugin::<LogoMaterial>::default())
            .add_systems(OnEnter(GameScene::Login), spawn_login_ui)
            .add_systems(OnExit(GameScene::Login), (crate::scene::despawn::<LoginUiRoot>, leave))
            // In character selection, the same manager (`CUIMng::CreateCharacterScene`).
            .add_systems(OnEnter(GameScene::CharacterSelect), character::spawn_character_ui)
            .add_systems(OnExit(GameScene::CharacterSelect), (crate::scene::despawn::<LoginUiRoot>, leave))
            // Tab belongs to the text boxes.
            .add_systems(PreUpdate, keep_tab.after(bevy::input::InputSystems).run_if(ui_scene()))
            .add_systems(FixedUpdate, logo_tick.run_if(in_state(GameScene::Login)))
            .add_systems(Update, (update_ui, draw_windows).chain().after(crate::charselect::CharSelectSystems).run_if(ui_scene()))
            .add_systems(Update, draw_logo.after(draw_windows).run_if(in_state(GameScene::Login)))
            .add_systems(
                Update,
                (
                    character::before_update.after(crate::charselect::CharSelectSystems).before(update_ui),
                    character::after_update.after(update_ui),
                )
                    .run_if(in_state(GameScene::CharacterSelect)),
            )
            // The credits music overrides the login one (login.rs requests it every frame).
            .add_systems(
                PostUpdate,
                credit_music.before(crate::sound::SoundSystems).run_if(in_state(GameScene::Login)),
            );
    }
}

/// `PlayMp3(MUSIC_MUTHEME)` while the credits are shown (LoginMainWin.cpp:107-116); when
/// they close, `login_theme` comes back.
fn credit_music(ui: Option<Res<LoginUi>>, mut sound: ResMut<SoundQueue>) {
    if ui.is_some_and(|ui| ui.is_shown(Win::Credit)) {
        sound.music("Mutheme");
    }
}

// --- Sizes ---------------------------------------------------------------------------------------

/// Reference window height for window pixels (1024×768).
const REF_HEIGHT: f32 = 768.0;
/// Interface units (480-high virtual screen) per original pixel.
const K: f32 = 480.0 / REF_HEIGHT;
/// `g_hFixFont`: height 13 − 1 = 12 px, at any resolution (Winmain.cpp:1464-1474).
const FIX_FONT: f32 = 12.0;
/// `g_hFont` at 1024×768: `FontHeight` 14 − 1 = 13 px (Winmain.cpp:1456-1471).
const FONT: f32 = 13.0;
/// Font smoothing (the original draws without it, `NONANTIALIASED_QUALITY`; with the Noto Sans
/// from `fonts.rs` it looks sharp).
const SMOOTHING: FontSmoothing = FontSmoothing::AntiAliased;

/// `CServerSelWin` (ServerSelWin.h:26-29, ServerSelWin.cpp:19-22).
const GROUP_W: i32 = 108;
const GROUP_H: i32 = 26;
const SERVER_W: i32 = 193;
const SERVER_H: i32 = 26;
const SSW_GAP_W: i32 = 28;
const SSW_GAP_H: i32 = 5;
/// Load bar inside the server button (`SSW_GB_POS_X/Y`) and its size.
const GAUGE_X: i32 = 16;
const GAUGE_Y: i32 = 19;
const GAUGE_W: i32 = 160;
const GAUGE_H: i32 = 4;
/// Height of the description box (`CWinEx` with `server_ex01/02/03` and 10 lines: 6 + 6 + 4×10).
/// The original counts it to place the buttons, but never draws it.
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
/// `CSysMenuWin` on the login: 213 wide and `SetLine(6)`: 64 + 43 + 8 × 6 (SysMenuWin.cpp:41-75).
const SYS_W: i32 = 213;
const SYS_H: i32 = 64 + 43 + 8 * 6;
/// `CLoginMainWin`: 54×30 buttons.
const BAR_BTN_W: i32 = 54;
const BAR_BTN_H: i32 = 30;

/// `COptionWin` (OptionWin.cpp:35-77): 213-wide `CWinEx` with the `op2_back1` header
/// (213×65), the `op1_back2` footer (213×43) and `SetLine(30)` 8-high sides. Here 8 more lines (64) for
/// the added music slider.
const OPT_W: i32 = 213;
const OPT_EXTRA: i32 = 8 * 8;
const OPT_SIDE: i32 = 8 * 30 + OPT_EXTRA;
const OPT_H: i32 = 65 + 43 + OPT_SIDE;
/// `OW_BTN_GAP` and `OW_SLD_GAP`.
const OW_BTN_GAP: i32 = 25;
const OW_SLD_GAP: i32 = 48;
/// 16×16 checkboxes (`BITMAP_CHECK_BTN`).
const CHECK: i32 = 16;
/// `CSlider`: 13×13 thumb, 98×13 background and the bar at (3, 3)-(95, 10) (OptionWin.cpp:63-74).
const SLD_W: i32 = 98;
const SLD_H: i32 = 13;
const THUMB: i32 = 13;
const THUMB_RANGE: i32 = SLD_W - THUMB;
const GAUGE_RECT: (i32, i32, i32, i32) = (3, 3, 95, 10);
/// Sliders: effects volume, music volume (added) and effects limit.
const SLD_EFFECT_VOL: usize = 0;
const SLD_MUSIC_VOL: usize = 1;
const SLD_RENDER_LV: usize = 2;
const SLD_MAX: usize = 3;
/// `SetSlideRange`: 9 for the volume (also music) and 4 for the effects limit.
const SLD_RANGE: [i32; SLD_MAX] = [9, 9, 4];
/// With the button held to one side of the thumb, the original moves it one step per frame
/// (it ran at 25): here one step per tick at 25 per second.
const SLD_STEP: f32 = 1.0 / 25.0;

/// `CCreditWin` (CreditWin.cpp:19-26): times in ms.
const CRW_ILLUST_FADE_TIME: f32 = 2000.0;
const CRW_ILLUST_SHOW_TIME: f32 = 22000.0;
const CRW_TEXT_FADE_TIME: f32 = 300.0;
const CRW_NAME_SHOW_TIME: f32 = 2300.0;
const CRW_ILLUST_MAX: u8 = 8;
/// `SCreditItem`: 1 class byte and 32 name bytes, 400 in `credit.bmd` (CreditWin.h:20-40).
const CRW_NAME_MAX: usize = 32;
const CRW_ITEM_MAX: usize = 400;
/// `CreateFont(18, ..., FW_BOLD, ...)` at 1024 wide (CreditWin.cpp:83-90).
const CREDIT_FONT: f32 = 18.0;
/// The `m_apszIllustPath` illustrations (CreditWin.cpp:66-81).
const ILLUST: [&str; CRW_ILLUST_MAX as usize * 2] = [
    "im1_1.jpg", "im1_2.jpg", "im2_1.jpg", "im2_2.jpg", "im3_1.jpg", "im3_2.jpg", "im4_1.jpg", "im4_2.jpg",
    "im5_1.jpg", "im5_2.jpg", "im6_1.jpg", "im6_2.jpg", "im7_1.jpg", "im7_2.jpg", "im8_1.jpg", "im8_2.jpg",
];
/// The only line added to Webzen's credits: at the end, as one more "THANKS TO" name.
const CREDIT_PORT: &str = "Port: crixpx";

/// Text box limits (`MAX_ID_SIZE`, `MAX_PASSWORD_SIZE`, _define.h).
const MAX_ID: usize = 10;
const MAX_PASSWORD: usize = 20;
/// Text box size: `Init(g_hWnd, 140, 14, ...)`, on the 640×480 screen.
const INPUT_W: f32 = 140.0;
const INPUT_H: f32 = 14.0;
/// The text caret blinks with `m_iCaretBlinkTemp % 24 < 12`, one step per original frame.
const CARET_PERIOD: f32 = 24.0 / 25.0;

/// Original colors (UIBaseDef.h).
const WHITE: [u8; 3] = [255, 255, 255];
const BR_GRAY: [u8; 3] = [226, 226, 226];
const YELLOW: [u8; 3] = [255, 255, 121];
const BR_YELLOW: [u8; 3] = [255, 238, 193];
const ORANGE: [u8; 3] = [255, 180, 0];
const BR_ORANGE: [u8; 3] = [255, 217, 39];
/// `SetTextColor(255, 255, 230, 210)` of the account and password boxes (ARGB).
const INPUT_COLOR: [u8; 3] = [255, 230, 210];

/// Texts (`GlobalText` from the S6 Text_eng.bmd).
const TEXT_COPYRIGHT: &str = "(c) Copyright 2001 Webzen"; // 454
const TEXT_RIGHTS: &str = " All Rights Reserved."; // 455
const TEXT_ACCOUNT: &str = "Account"; // 450
const TEXT_PASSWORD: &str = "Password"; // 451
const TEXT_EXIT_GAME: &str = "Exit Game"; // 381
const TEXT_OPTION: &str = "Option"; // 385
const TEXT_SELECT_SERVER: &str = "Select Server"; // 382
const TEXT_CLOSE: &str = "Close"; // 388
const TEXT_AUTO_ATTACK: &str = "Automatic Attack"; // 386
const TEXT_WHISPER: &str = "Beep sound for whispering"; // 387
const TEXT_VOLUME: &str = "Volume"; // 389
const TEXT_SLIDE_HELP: &str = "Slide Help"; // 919
const TEXT_EFFECT_LIMIT: &str = "+Effect limitation"; // 1840
/// The added music slider (Text_eng.bmd has no text for it).
const TEXT_MUSIC: &str = "Music";
const TEXT_SERVER_FULL: &str = "The server is full"; // 416
const TEXT_ENTER_ACCOUNT: &str = "Enter your account"; // 403
const TEXT_ENTER_PASSWORD: &str = "Enter your password"; // 404
/// 565-567: warning for groups without any Non-PvP server.
const TEXT_PVP: [&str; 3] = ["Since Helheim server ", "tends to be crowded", "we recommend that you use other servers."];

/// Screen in reference window pixels.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Screen {
    w: i32,
    h: i32,
}

impl Screen {
    /// `g_fScreenRate_x` / `g_fScreenRate_y`: pixels per unit of the 640×480 screen.
    fn rate_x(self) -> f32 {
        self.w as f32 / 640.0
    }
    fn rate_y(self) -> f32 {
        self.h as f32 / 480.0
    }
    /// `int(x / g_fScreenRate_x)`: texts are placed in whole 640×480 units.
    fn snap_x(self, x: f32) -> f32 {
        (x / self.rate_x()).floor() * self.rate_x()
    }
    fn snap_y(self, y: f32) -> f32 {
        (y / self.rate_y()).floor() * self.rate_y()
    }
}

/// Rectangle (x, y, width, height) in pixels; containment like `PtInRect` (excluding the right and bottom edges).
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

/// `CUIMng::CreateLoginScene` (UIMng.cpp:233-234): `int(567 / 600 × height) − 30 − 11`.
fn bar_y(s: Screen) -> i32 {
    (567.0 / 600.0 * s.h as f32) as i32 - BAR_BTN_H - 11
}

/// `CLoginMainWin`: the bar is the screen width minus 30 on each side (LoginMainWin.cpp:46).
fn bar_rect(s: Screen) -> IRect {
    IRect::new(30, bar_y(s), s.w - 60, BAR_BTN_H)
}

fn menu_rect(s: Screen) -> IRect {
    IRect::new(30, bar_y(s), BAR_BTN_W, BAR_BTN_H)
}

/// Flush with the bar's right edge (LoginMainWin.cpp:65-67).
fn credit_rect(s: Screen) -> IRect {
    let bar = bar_rect(s);
    IRect::new(bar.x + bar.w - BAR_BTN_W, bar.y, BAR_BTN_W, BAR_BTN_H)
}

/// `CServerSelWin`, centered (UIMng.cpp:238-240).
fn ssw_rect(s: Screen) -> IRect {
    IRect::new((s.w - SSW_W) / 2, (s.h - SSW_H) / 2, SSW_W, SSW_H)
}

/// Group button `i` (`CServerSelWin::SetPosition`, ServerSelWin.cpp:108-144): 0 (test
/// server) centered at the bottom, 1 to 10 in the left column and 11 to 20 in the right one.
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

/// Server `j` of `n` (`SetServerBtnPosition`, ServerSelWin.cpp:146-168): between the two columns,
/// from the height of the first left group (or higher if they do not fit in 10 rows).
fn server_rect(s: Screen, j: usize, n: usize) -> IRect {
    let g = group_rect(s, 1);
    let left_sum = GROUP_H * 10;
    let sum = SERVER_H * n as i32;
    let base = if left_sum > sum { g.y } else { g.y - (sum - left_sum) };
    IRect::new(g.x + GROUP_W + SSW_GAP_W, base + SERVER_H * j as i32, SERVER_W, SERVER_H)
}

/// `CLoginWin`: centered horizontally and at 2/3 vertically (UIMng.cpp:244-246).
fn login_rect(s: Screen) -> IRect {
    IRect::new((s.w - LOGIN_W) / 2, (s.h - LOGIN_H) * 2 / 3, LOGIN_W, LOGIN_H)
}

/// Frame of the account (0) or password (1) text box (LoginWin.cpp:92-93).
fn field_rect(s: Screen, field: Field) -> IRect {
    let l = login_rect(s);
    let dy = if field == Field::Id { 106 } else { 131 };
    IRect::new(l.x + 109, l.y + dy, 156, 23)
}

/// Where the box text starts: `int((frame + 6) / g_fScreenRate)` (LoginWin.cpp:97-101).
fn input_origin(s: Screen, field: Field) -> Vec2 {
    let f = field_rect(s, field);
    Vec2::new(s.snap_x((f.x + 6) as f32), s.snap_y((f.y + 6) as f32))
}

/// Area that takes focus on click: `CheckMouseIn(x, y − 4, width, height + 8)` in 640×480 (UIControls.cpp:3628).
fn input_click_rect(s: Screen, field: Field) -> IRect {
    let o = input_origin(s, field);
    let (rx, ry) = (s.rate_x(), s.rate_y());
    IRect::new(o.x as i32, (o.y - 4.0 * ry) as i32, (INPUT_W * rx) as i32, ((INPUT_H + 8.0) * ry) as i32)
}

fn msg_rect(s: Screen) -> IRect {
    IRect::new((s.w - MSG_W) / 2, (s.h - MSG_H) / 2, MSG_W, MSG_H)
}

/// Centered `CSysMenuWin`: `SetLine(6)` on the login and `SetLine(10)` in character selection.
fn sys_rect_for(s: Screen, scene: UiScene) -> IRect {
    let h = if scene == UiScene::Character { 64 + 43 + 8 * 10 } else { SYS_H };
    IRect::new((s.w - SYS_W) / 2, (s.h - h) / 2, SYS_W, h)
}

/// `COptionWin`'s `CWinEx`, centered (OptionWin.cpp:75-76).
fn opt_rect(s: Screen) -> IRect {
    IRect::new((s.w - OPT_W) / 2, (s.h - OPT_H) / 2, OPT_W, OPT_H)
}

/// Checkbox `i` (`COptionWin::SetPosition`, OptionWin.cpp:86-105): at x + 52, from y + 52 every 41.
fn check_rect(s: Screen, i: usize) -> IRect {
    let r = opt_rect(s);
    IRect::new(r.x + 52, r.y + 52 + i as i32 * (OW_BTN_GAP + CHECK), CHECK, CHECK)
}

/// Corner of slider `i`: 48 below the last checkbox and every 61.
fn slider_origin(s: Screen, i: usize) -> IVec2 {
    let last = check_rect(s, 2);
    IVec2::new(last.x, last.y + CHECK + OW_SLD_GAP + i as i32 * (OW_SLD_GAP + SLD_H))
}

/// Options Close button, centered at y + 301 (moved down by the window's growth).
fn opt_close_rect(s: Screen) -> IRect {
    let r = opt_rect(s);
    IRect::new(r.x + (OPT_W - 108) / 2, r.y + 301 + OPT_EXTRA, 108, 30)
}

/// `CCreditWin::SetPosition` (CreditWin.cpp:107-124): the ornament at the bottom right, with its foot at
/// 527/600 of the height, and the close button inside it at (122, 63).
fn credit_deco_rect(s: Screen) -> IRect {
    let base = (527.0 / 600.0 * s.h as f32) as i32;
    IRect::new(s.w - 189, base - 103, 189, 103)
}

fn credit_close_rect(s: Screen) -> IRect {
    let d = credit_deco_rect(s);
    IRect::new(d.x + 122, d.y + 63, 54, 30)
}

// --- State ---------------------------------------------------------------------------------------

/// Login windows (those of `CUIMng::CreateLoginScene` that exist for now).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Win {
    MainBar,
    ServerSel,
    Login,
    SysMenu,
    Option,
    Credit,
    Msg,
    /// `CCharMakeWin` (character selection).
    CharMake,
}

/// Manager scene (`UIM_SCENE_LOGIN` / `UIM_SCENE_CHARACTER`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum UiScene {
    Login,
    Character,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Btn {
    /// "Select Server" of the system menu (character selection only).
    SysServerSelect,
    /// Class of the create window.
    Job(crate::account::Class),
    MakeOk,
    MakeCancel,
    Menu,
    Credit,
    /// Group button by its position (`m_iBtnPos`).
    Group(usize),
    /// Server of the chosen group, by order.
    Server(usize),
    LoginOk,
    LoginCancel,
    MsgOk,
    MsgCancel,
    SysExit,
    SysOption,
    SysClose,
    /// `COptionWin` checkboxes: auto attack, whisper alert and sliding help.
    OptAutoAttack,
    OptWhisper,
    OptSlideHelp,
    OptClose,
    /// Thumb of slider `i` (`CSlider::m_btnThumb`).
    OptThumb(usize),
    CreditClose,
}

/// Horizontal `CSlider` (Slider.cpp). Positions are relative to the slider's corner,
/// which is recomputed every frame (the window can be resized).
#[derive(Clone, Debug)]
struct Slider {
    /// `m_nSlidePos` and `m_nSlideRange`.
    pos: i32,
    range: i32,
    /// Thumb: x from the slider's corner.
    thumb: i32,
    /// `m_byState`: `SLD_STATE_UP` (1), `SLD_STATE_DN` (2) and `SLD_STATE_THUMB_DRG` (0x10).
    state: u8,
    capture: i32,
    limit: i32,
    /// Time until the next step with the button held to one side of the thumb.
    step_clock: f32,
}

const SLD_STATE_UP: u8 = 0x01;
const SLD_STATE_DN: u8 = 0x02;
const SLD_STATE_THUMB_DRG: u8 = 0x10;

impl Slider {
    fn new(range: i32) -> Self {
        Self { pos: 0, range: range.max(1), thumb: 0, state: 0, capture: 0, limit: 0, step_clock: SLD_STEP }
    }

    fn thumb_at(&self, pos: i32) -> i32 {
        (THUMB_RANGE as f32 / self.range as f32 * pos as f32) as i32
    }

    /// `SetThumbPosition`.
    fn set_thumb(&mut self) {
        self.thumb = self.thumb_at(self.pos);
    }

    /// `SetSlidePos`.
    fn set_pos(&mut self, pos: i32) {
        self.pos = pos.clamp(0, self.range);
        self.set_thumb();
    }

    /// `LineUp`.
    fn line_up(&mut self) {
        self.pos -= 1;
        self.thumb = self.thumb_at(self.pos);
    }

    /// `LineDown`.
    fn line_down(&mut self) {
        self.pos += 1;
        self.thumb = if self.pos == self.range { THUMB_RANGE } else { self.thumb_at(self.pos) };
    }

    /// Width of the colored bar (`CGaugeBar::SetValue`).
    fn gauge(&self) -> i32 {
        self.pos.clamp(0, self.range) * (GAUGE_RECT.2 - GAUGE_RECT.0) / self.range
    }

    /// `CSlider::Update` with the mouse relative to the slider's corner.
    fn update(&mut self, cursor: Option<IVec2>, held: bool, up: bool, dt: f32) {
        if up {
            self.set_thumb();
            self.state = 0;
        }
        let Some(c) = cursor else { return };
        let in_y = c.y >= 0 && c.y < SLD_H;
        let upper = in_y && c.x >= 0 && c.x < self.thumb;
        let under = in_y && c.x >= self.thumb + THUMB && c.x < SLD_W;
        if held && (upper || under) {
            self.step_clock += dt;
            if self.step_clock >= SLD_STEP {
                self.step_clock = 0.0;
                if upper {
                    self.line_up();
                    self.state |= SLD_STATE_UP;
                } else {
                    self.line_down();
                    self.state |= SLD_STATE_DN;
                }
            }
        } else {
            self.step_clock = SLD_STEP;
        }
        let on_thumb = in_y && c.x >= self.thumb && c.x < self.thumb + THUMB;
        if on_thumb && held {
            self.capture = c.x;
            self.limit = self.capture - self.thumb;
            self.state |= SLD_STATE_THUMB_DRG;
        }
        if self.state & SLD_STATE_THUMB_DRG != 0 {
            let thumb = if self.limit < c.x && self.limit + THUMB_RANGE > c.x {
                let t = self.thumb + c.x - self.capture;
                self.capture = c.x;
                t
            } else if self.limit >= c.x {
                self.capture = self.limit;
                0
            } else {
                self.capture = self.limit + THUMB_RANGE;
                THUMB_RANGE
            };
            self.thumb = thumb;
            let per_pos = THUMB_RANGE as f32 / self.range as f32;
            self.pos = ((self.thumb as f32 + per_pos / 2.0) / per_pos) as i32;
        }
    }
}

/// Credits `SHOW_STATE`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Fade {
    Hide,
    In,
    Show,
    Out,
}

/// `SCreditItem`: class (1 department, 2 team, 3 name) and name.
#[derive(Clone, Debug)]
struct CreditItem {
    class: u8,
    name: String,
}

/// `CRW_INDEX_*`: department, team and the names.
const CRW_INDEX_DEPARTMENT: usize = 0;
const CRW_INDEX_TEAM: usize = 1;
const CRW_INDEX_NAME: usize = 2;
const CRW_INDEX_MAX: usize = 6;

/// `CCreditWin` state.
struct Credits {
    items: Vec<CreditItem>,
    illust_state: Fade,
    illust_clock: f32,
    illust: u8,
    illust_alpha: f32,
    now: usize,
    name_count: usize,
    index: [usize; CRW_INDEX_MAX],
    text_state: [Fade; CRW_INDEX_NAME + 1],
    text_clock: f32,
    /// Alpha of the three black strips covering the lines (`CRW_SPR_TXT_HIDE0..2`); they start
    /// covering and are not reset when reopened.
    hide_alpha: [f32; CRW_INDEX_NAME + 1],
}

impl Credits {
    fn new(items: Vec<CreditItem>) -> Self {
        Self {
            items,
            illust_state: Fade::Hide,
            illust_clock: 0.0,
            illust: 0,
            illust_alpha: 0.0,
            now: 0,
            name_count: 0,
            index: [0; CRW_INDEX_MAX],
            text_state: [Fade::Hide; CRW_INDEX_NAME + 1],
            text_clock: 0.0,
            hide_alpha: [255.0; CRW_INDEX_NAME + 1],
        }
    }

    fn class(&self, i: usize) -> u8 {
        self.items.get(i).map_or(0, |c| c.class)
    }

    fn name(&self, slot: usize) -> &str {
        self.items.get(self.index[slot]).map_or("", |c| c.name.as_str())
    }

    /// `CCreditWin::Init` (CreditWin.cpp:237-250). Returns whether it must close.
    fn init(&mut self) -> bool {
        self.illust_state = Fade::In;
        self.illust_clock = 0.0;
        self.illust = 0;
        // `LoadIllust`: the two new ones start transparent.
        self.illust_alpha = 0.0;
        self.text_state = [Fade::In; CRW_INDEX_NAME + 1];
        self.text_clock = 0.0;
        self.now = 0;
        self.name_count = 0;
        self.set_text_index()
    }

    /// `CCreditWin::SetTextIndex` (CreditWin.cpp:338-370): the next department, team and
    /// up to 4 names. A class 0 ends the credits (returns whether it must close).
    ///
    /// The original counts one name less when there are 4 in a row (the fourth was never shown);
    /// here all are counted so that no credit is missing.
    fn set_text_index(&mut self) -> bool {
        if self.class(self.now) == 0 {
            return true;
        }
        if self.class(self.now) == 1 {
            self.index[CRW_INDEX_DEPARTMENT] = self.now;
            self.now += 1;
        }
        if self.class(self.now) == 2 {
            self.index[CRW_INDEX_TEAM] = self.now;
            self.now += 1;
        }
        let mut count = 0;
        for i in 0..4 {
            if self.class(self.now) != 3 {
                break;
            }
            self.index[CRW_INDEX_NAME + i] = self.now;
            self.now += 1;
            count += 1;
        }
        self.name_count = count;
        false
    }

    /// `AnimationIllust` (CreditWin.cpp:273-314), `dt` in ms.
    fn animate_illust(&mut self, dt: f32) {
        match self.illust_state {
            Fade::In => {
                self.illust_alpha += 255.0 * dt / CRW_ILLUST_FADE_TIME;
                if self.illust_alpha >= 255.0 {
                    self.illust_state = Fade::Show;
                    self.illust_alpha = 255.0;
                }
            }
            Fade::Show => {
                self.illust_clock += dt;
                if self.illust_clock > CRW_ILLUST_SHOW_TIME {
                    self.illust_state = Fade::Out;
                    self.illust_clock = 0.0;
                }
            }
            Fade::Out => {
                self.illust_alpha -= 255.0 * dt / CRW_ILLUST_FADE_TIME;
                if self.illust_alpha <= 0.0 {
                    self.illust_state = Fade::In;
                    self.illust_alpha = 0.0;
                    self.illust = (self.illust + 1) % CRW_ILLUST_MAX;
                }
            }
            Fade::Hide => {}
        }
    }

    /// `AnimationText` (CreditWin.cpp:372-430): the black strip fades out (the line appears) or
    /// darkens (it disappears). Returns whether it must close.
    fn animate_text(&mut self, class: usize, dt: f32) -> bool {
        match self.text_state[class] {
            Fade::In => {
                self.hide_alpha[class] -= 255.0 * dt / CRW_TEXT_FADE_TIME;
                if self.hide_alpha[class] <= 0.0 {
                    self.text_state[class] = Fade::Show;
                    self.hide_alpha[class] = 0.0;
                }
            }
            Fade::Show => {
                if class != CRW_INDEX_NAME {
                    return false;
                }
                self.text_clock += dt;
                if self.text_clock > CRW_NAME_SHOW_TIME {
                    self.text_state[CRW_INDEX_NAME] = Fade::Out;
                    self.text_clock = 0.0;
                    if self.class(self.now) != 3 {
                        self.text_state[CRW_INDEX_TEAM] = Fade::Out;
                        if self.class(self.now) != 2 {
                            self.text_state[CRW_INDEX_DEPARTMENT] = Fade::Out;
                        }
                    }
                }
            }
            Fade::Out => {
                self.hide_alpha[class] += 255.0 * dt / CRW_TEXT_FADE_TIME;
                if self.hide_alpha[class] >= 255.0 {
                    self.text_state[class] = Fade::In;
                    self.hide_alpha[class] = 255.0;
                    if class == CRW_INDEX_NAME {
                        return self.set_text_index();
                    }
                }
            }
            Fade::Hide => {}
        }
        false
    }
}

/// `CCreditWin::LoadText` (CreditWin.cpp:318-336): `Data/Local/credit.bmd`, 400 × 33 bytes with
/// `BuxConvert`, and after the last entry, the port's own line.
fn load_credits(data: &std::path::Path) -> Vec<CreditItem> {
    let path = mu_data::fs::find(data, "Local").and_then(|d| mu_data::fs::find(&d, "credit.bmd"));
    let Some(Ok(mut bytes)) = path.map(std::fs::read) else {
        warn!("créditos: no está Data/Local/credit.bmd");
        return Vec::new();
    };
    const BUX: [u8; 3] = [0xfc, 0xcf, 0xab];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b ^= BUX[i % 3];
    }
    let mut items: Vec<CreditItem> = bytes
        .as_chunks::<{ 1 + CRW_NAME_MAX }>()
        .0
        .iter()
        .take(CRW_ITEM_MAX)
        .map(|c| {
            let name = &c[1..];
            let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
            // Windows-1252 (English text is plain ASCII).
            CreditItem { class: c[0], name: name[..end].iter().map(|&b| b as char).collect() }
        })
        .collect();
    // The file ends in padding (class 205): what is shown ends there. The own line goes
    // as one more name with a blank team, like "All of WEBZEN STAFF".
    let end = items.iter().position(|c| !(1..=3).contains(&c.class)).unwrap_or(items.len());
    items.insert(end, CreditItem { class: 2, name: " ".into() });
    items.insert(end + 1, CreditItem { class: 3, name: CREDIT_PORT.into() });
    items
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Field {
    Id,
    Pass,
    /// New character's name.
    CharName,
    /// Password for deleting.
    DelPass,
}

/// `MSG_WIN_TYPE`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum MsgKind {
    None,
    Cancel,
    Ok,
    /// `MWT_BTN_BOTH`: OK and Cancel.
    Both,
    /// `MWT_STR_INPUT`: password box, OK and Cancel.
    Input,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum MsgCode {
    ServerBusy,
    InputId,
    InputPassword,
    GameEndCountdown,
    // Character selection (MsgWin.cpp).
    MinLength,
    IdSpaceError,
    SpecialName,
    DeleteGuildWarning,
    DeleteConfirm,
    DeleteResident,
    DeleteSuccess,
    ResidentWrong,
    CreateFail,
    /// `MESSAGE_WAIT`: waiting for the server (no buttons).
    Wait,
    // Login with the server (`ReceiveLogIn` / `ReceiveJoinServer`, WSclient.cpp:12760).
    ServerLost,
    LoginFailPassword,
    LoginFailId,
    LoginFailConnected,
    LoginFailServerBusy,
    LoginFailBlocked,
    LoginFailVersion,
    LoginFailConnect,
    LoginFailError,
}

/// What the manager asks of the network (sent by `update_ui`).
#[derive(Clone, Debug, PartialEq)]
enum NetOut {
    /// `SendRequestServerList` to the Connect Server.
    ServerList,
    /// `SendRequestServerAddress`: connect to the Game Server with that id.
    Connect(u16),
    /// A message to the Game Server.
    Game(mu_protocol::game::ClientMessage),
    /// Close the Game Server connection.
    Close,
}

struct Message {
    code: Option<MsgCode>,
    kind: MsgKind,
    lines: Vec<String>,
}

/// `CServerGroup::SBP_*`: column of the group button. The right one and the center (test
/// server) will come with the connect server list.
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum GroupPos {
    Left,
    Right,
    Center,
}

struct ServerInfo {
    /// Id in the Connect Server.
    id: u16,
    /// Server number within the group (`m_iIndex`, from 1).
    index: i32,
    /// Load (`m_iPercent`): 100 = full, 128 or more = getting ready.
    percent: i32,
    /// `m_byNonPvP`: 0 normal, 1 Non-PvP, 2 Gold PvP, 3 Gold.
    non_pvp: u8,
}

struct ServerGroup {
    name: String,
    pos: GroupPos,
    /// Button it goes on (`m_iBtnPos`), set by `UpdateDisplay`.
    btn: usize,
    /// No Non-PvP server (`m_bPvPServer`).
    pvp: bool,
    servers: Vec<ServerInfo>,
}

impl ServerInfo {
    /// `CServerListManager::InsertServer` (ServerListManager.cpp:177-212) with texts 560-562.
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

    /// `adwServerBtnClr[m_byNonPvP]`: normal, pressed and hover (ServerSelWin.cpp:200-206).
    fn colors(&self) -> [[u8; 3]; 3] {
        match self.non_pvp {
            1 => [YELLOW, YELLOW, BR_YELLOW],
            2 | 3 => [ORANGE, ORANGE, BR_ORANGE],
            _ => [BR_GRAY, BR_GRAY, WHITE],
        }
    }
}

/// Text box (`CUITextInputBox`, a Windows `edit` underneath).
struct TextBox {
    text: String,
    /// Caret position, in characters.
    caret: usize,
    /// Everything selected (`GiveFocus(TRUE)`): the next thing typed replaces it.
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

    /// Removes the selection (everything). Returns whether there was anything.
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

    /// Ctrl+V: the clipboard text at the caret (until the box is full).
    fn paste(&mut self, text: &str) {
        self.take_selection();
        for c in crate::clipboard::paste_text(text) {
            self.insert(c);
        }
    }

    /// Ctrl+C: the selection or, with no selection, everything. Passwords are not copied (`ES_PASSWORD`).
    fn copy_text(&self) -> Option<&str> {
        (!self.password).then_some(self.text.as_str())
    }

    /// Ctrl+X: copies it and empties the box (nothing for passwords).
    fn cut(&mut self) -> Option<String> {
        if self.password {
            return None;
        }
        self.select_all = false;
        self.caret = 0;
        Some(std::mem::take(&mut self.text))
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

    /// Moves the caret (and clears the selection, like the `edit`).
    fn move_to(&mut self, caret: usize) {
        self.select_all = false;
        self.caret = caret.min(self.len());
    }

    /// What is shown: the password as asterisks (`ES_PASSWORD`).
    fn shown(&self) -> String {
        if self.password { "*".repeat(self.len()) } else { self.text.clone() }
    }
}

/// What happens outside the interface.
enum Outcome {
    /// Login accepted with this account.
    Enter(String),
    /// End of the "Exit Game" countdown.
    Exit,
    /// "Select Server" from character selection: back to the login.
    ServerSelect,
}

#[derive(Resource)]
struct LoginUi {
    /// Windows front to back (`m_WinList`): the first one is the one that can be active.
    order: Vec<Win>,
    shown: Vec<Win>,
    /// The first of `order` is active (`m_bActive`).
    active: bool,
    /// Activate the first one on the next pass (`m_bWinActive`).
    pending_active: bool,
    /// `m_bSysMenuWinShow`: toggles with each Esc.
    sys_flag: bool,
    /// Held button (`CButton::m_pBtnHeld`).
    held: Option<Btn>,
    /// Window under the mouse: only its buttons respond (`ActiveBtns`).
    btn_window: Option<Win>,
    cursor: Option<IVec2>,

    groups: Vec<ServerGroup>,
    /// Chosen group button (`m_iSelectServerBtnIndex`).
    selected_group: Option<usize>,
    /// Server chosen for the login window title: group, number, Non-PvP.
    selected_server: Option<(String, i32, u8)>,

    id: TextBox,
    pass: TextBox,
    focus: Option<Field>,
    /// The login window has not been drawn yet (`FirstLoad`).
    first_load: bool,
    caret_clock: f32,

    msg: Message,
    /// "Exit Game" countdown (`m_nGameExit`, `m_dDeltaTickSum`).
    exit_count: i32,
    exit_clock: f32,

    sounds: u32,
    outcome: Option<Outcome>,

    /// Logo fade (`g_fMULogoAlpha`) this tick and the previous one.
    logo: f32,
    prev_logo: f32,

    /// `g_pOption` (copied from `options::GameOptions` before each pass and returned afterwards).
    opts: crate::options::GameOptions,
    /// Effects and music volume (sound.rs) and the requested changes.
    sfx: u8,
    music: u8,
    set_sfx: Option<u8>,
    set_music: Option<u8>,
    sliders: [Slider; SLD_MAX],
    credits: Credits,

    scene: UiScene,
    /// Create window: the name, the chosen class (`m_nSelJob`) and how many times one has been
    /// chosen (the preview restarts).
    char_name: TextBox,
    job: crate::account::Class,
    job_picks: u32,
    /// Password for deleting a character (`MWT_STR_INPUT`).
    del_pass: TextBox,
    /// What charselect.rs reports: the account, how many characters it has and the selected one.
    account: String,
    char_count: usize,
    char_selected: Option<(usize, String, u8)>,
    /// For charselect.rs (character created or deleted).
    scene_events: Vec<crate::charselect::SceneEvent>,
    /// For the network.
    net_out: Vec<NetOut>,
    /// Classes the server allows creating (`CharacterCreationUnlockFlags`).
    class_unlock: u8,
}

impl LoginUi {
    fn new(saved_id: &str, credits: Vec<CreditItem>) -> Self {
        let mut ui = Self {
            // Creation order (`AddHead`, UIMng.cpp:207-252): the last one created ends up in front.
            order: vec![Win::Credit, Win::Login, Win::ServerSel, Win::MainBar, Win::Option, Win::SysMenu, Win::Msg],
            shown: Vec::new(),
            active: false,
            pending_active: false,
            sys_flag: false,
            held: None,
            btn_window: None,
            cursor: None,
            groups: Vec::new(),
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
            opts: crate::options::GameOptions::default(),
            sfx: 0,
            music: 0,
            set_sfx: None,
            set_music: None,
            sliders: SLD_RANGE.map(Slider::new),
            credits: Credits::new(credits),
            scene: UiScene::Login,
            char_name: TextBox::new("", crate::account::MAX_NAME, false),
            job: crate::account::Class::Knight,
            job_picks: 0,
            del_pass: TextBox::new("", MAX_PASSWORD, true),
            account: String::new(),
            char_count: 0,
            char_selected: None,
            scene_events: Vec::new(),
            // `ConnectConnectionServer` on entering the login.
            net_out: vec![NetOut::ServerList],
            class_unlock: 0,
        };
        ui.update_display();
        ui
    }

    fn sys_rect(&self, s: Screen) -> IRect {
        sys_rect_for(s, self.scene)
    }

    fn is_shown(&self, w: Win) -> bool {
        self.shown.contains(&w)
    }

    fn is_active(&self, w: Win) -> bool {
        self.active && self.order.first() == Some(&w)
    }

    /// `CUIMng::SetActiveWin`: the front one stops being active and `w` moves to the front.
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

    /// `CUIMng::HideWin`: hidden and moved to the end; the new first one becomes active.
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

    /// `CServerSelWin::UpdateDisplay`: distributes the groups over the buttons.
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
            // `GlobalText` 390, 1715, 391, 1654, 1712, 1713, 1714, 401 and 1716.
            MsgCode::MinLength => (MsgKind::Ok, "Type more than 4 letters".into()),
            MsgCode::IdSpaceError => (MsgKind::Ok, "It contains prohibited words.".into()),
            MsgCode::SpecialName => (MsgKind::Ok, "Cannot use symbols.".into()),
            MsgCode::DeleteGuildWarning => (MsgKind::Ok, "You can't delete the character that belongs to the guild".into()),
            MsgCode::DeleteConfirm => {
                let name = self.char_selected.as_ref().map_or(String::new(), |s| s.1.clone());
                (MsgKind::Both, format!("Would you like to delete {name} character?"))
            }
            MsgCode::DeleteResident => {
                // `InitResidentNumInput`: the box empty and focused.
                self.del_pass = TextBox::new("", MAX_PASSWORD, true);
                self.give_focus(Field::DelPass, false);
                (MsgKind::Input, "Please enter your WEBZEN.COM password.".into())
            }
            MsgCode::DeleteSuccess => (MsgKind::Ok, "Character was deleted successfully.".into()),
            MsgCode::ResidentWrong => (MsgKind::Ok, "The password you have entered is incorrect.".into()),
            MsgCode::CreateFail => (MsgKind::Ok, "Incorrect character name was entered or same character name exists.".into()),
            MsgCode::Wait => (MsgKind::None, "Please wait".into()),
            // `GlobalText` 402, 407, 414, 415, 416, 417, 405 + 406, 408 and 409.
            MsgCode::ServerLost => (MsgKind::Ok, "You are disconnected from the server.".into()),
            MsgCode::LoginFailPassword => (MsgKind::Ok, "Password is incorrect".into()),
            MsgCode::LoginFailId => (MsgKind::Ok, "Your account is invalid".into()),
            MsgCode::LoginFailConnected => (MsgKind::Ok, "Your account is already connected".into()),
            MsgCode::LoginFailServerBusy => (MsgKind::Ok, "The server is full".into()),
            MsgCode::LoginFailBlocked => (MsgKind::Ok, "This account is blocked".into()),
            MsgCode::LoginFailVersion => {
                // Two lines (`lpszMsg`, `lpszMsg2`).
                self.msg = Message {
                    code: Some(code),
                    kind: MsgKind::Ok,
                    lines: vec!["New version of game is required".into(), "Please download the new version".into()],
                };
                self.show_win(Win::Msg);
                return;
            }
            MsgCode::LoginFailConnect => (MsgKind::Ok, "Connection error".into()),
            MsgCode::LoginFailError => (MsgKind::Ok, "Connection closed due to 3 failed attempts.".into()),
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
            Field::CharName => &mut self.char_name,
            Field::DelPass => &mut self.del_pass,
        }
    }

    fn field(&self, field: Field) -> &TextBox {
        match field {
            Field::Id => &self.id,
            Field::Pass => &self.pass,
            Field::CharName => &self.char_name,
            Field::DelPass => &self.del_pass,
        }
    }

    /// Buttons of a window, in the order they are registered and drawn.
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
                        // `Create(108, 26, BITMAP_LOG_IN, 4, 2, 1, -1, 3)`: normal 0, hover 1,
                        // pressed 2 and selected 3.
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
                    MsgKind::Both => {
                        out.push(ok(98));
                        out.push(cancel(200));
                    }
                    MsgKind::Input => {
                        out.push(ok(209));
                        out.push(cancel(264));
                    }
                    MsgKind::None => {}
                }
            }
            Win::CharMake => self.char_make_buttons(s, &mut out),
            Win::SysMenu => {
                // `CSysMenuWin::SetPosition` (SysMenuWin.cpp:83-96). "Select Server" is
                // disabled on the login and has no disabled frame: it is not drawn; in
                // character selection all four are there.
                let r = self.sys_rect(s);
                let x = r.x + (SYS_W - 108) / 2;
                let gap = 4 + 30;
                let close_y = r.y + r.h - 52;
                let mut list = vec![(Btn::SysExit, r.y + 33, TEXT_EXIT_GAME)];
                if self.scene == UiScene::Character {
                    list.push((Btn::SysServerSelect, r.y + 33 + gap, TEXT_SELECT_SERVER));
                }
                list.extend([(Btn::SysOption, close_y - gap, TEXT_OPTION), (Btn::SysClose, close_y, TEXT_CLOSE)]);
                for (id, y, text) in list {
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
            Win::Option => {
                // Checkboxes: `Create(16, 16, BITMAP_CHECK_BTN, 2, 0, 0, -1, 1, 1, 1)`: 0 unchecked
                // and 1 checked in all three states (OptionWin.cpp:52-56).
                let checks = [
                    (Btn::OptAutoAttack, self.opts.auto_attack),
                    (Btn::OptWhisper, self.opts.whisper_sound),
                    (Btn::OptSlideHelp, self.opts.slide_help),
                ];
                for (i, (id, checked)) in checks.into_iter().enumerate() {
                    out.push(ButtonDef {
                        id,
                        rect: check_rect(s, i),
                        tex: Tex::CheckBtn,
                        frames: [0, 0, 0],
                        check_frame: Some(1),
                        checked,
                        text: None,
                    });
                }
                out.push(ButtonDef {
                    id: Btn::OptClose,
                    rect: opt_close_rect(s),
                    tex: Tex::TextBtn,
                    frames: [0, 2, 1],
                    check_frame: None,
                    checked: false,
                    text: Some((TEXT_CLOSE.to_string(), [BR_GRAY, BR_GRAY, WHITE])),
                });
            }
            Win::Credit => {
                // `m_btnClose.Create(54, 30, BITMAP_BUTTON+2, 3, 2, 1)`.
                out.push(ButtonDef::image(Btn::CreditClose, credit_close_rect(s), Tex::CloseBtn));
            }
        }
        out
    }

    /// Thumb of slider `i` (a one-frame `CButton`; the `CSlider` updates and draws it).
    fn thumb_button(&self, s: Screen, i: usize) -> ButtonDef {
        let o = slider_origin(s, i);
        ButtonDef {
            id: Btn::OptThumb(i),
            rect: IRect::new(o.x + self.sliders[i].thumb, o.y, THUMB, THUMB),
            tex: Tex::SliderThumb,
            frames: [0, 0, 0],
            check_frame: None,
            checked: false,
            text: None,
        }
    }

    /// `COptionWin::UpdateDisplay` (OptionWin.cpp:132-139): the sliders take the current values.
    fn option_display(&mut self) {
        let values = [self.sfx as i32, self.music as i32, self.opts.render_level as i32];
        for (slider, v) in self.sliders.iter_mut().zip(values) {
            slider.set_pos(v);
            slider.state = 0;
        }
    }

    /// `CWin::CursorInWin(WA_ALL)`. The message box, system menu, options and
    /// credits cover the whole screen (`CWin::Create(width, height)` of the screen).
    fn win_rect(s: Screen, w: Win) -> IRect {
        match w {
            Win::MainBar => bar_rect(s),
            Win::ServerSel => ssw_rect(s),
            Win::Login => login_rect(s),
            Win::SysMenu | Win::Option | Win::Credit | Win::Msg | Win::CharMake => IRect::new(0, 0, s.w, s.h),
        }
    }

    /// The first visible window (front to back) under the mouse.
    fn window_under(&self, s: Screen, cursor: Option<IVec2>) -> Option<Win> {
        self.order.iter().copied().find(|&w| self.is_shown(w) && Self::win_rect(s, w).contains(cursor))
    }

    /// Button state for drawing (`CButton::Update`): hover, pressed or normal.
    fn button_state(&self, b: &ButtonDef, w: Win) -> BtnState {
        let inside = self.btn_window == Some(w) && b.rect.contains(self.cursor);
        match self.held {
            None if inside => BtnState::Active,
            Some(h) if inside && h == b.id => BtnState::Down,
            _ => BtnState::Up,
        }
    }

    /// `CButton::Update`: pressed when the mouse button goes down, a click if released over it.
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

    /// One pass of `CUIMng::Update` and `NewMoveLogInScene`.
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
            // The create window's box and the delete password box.
            if (w == Win::CharMake && self.is_active(w)) || (w == Win::Msg && self.msg.kind == MsgKind::Input) {
                self.type_keys(input);
            }
            if self.is_active(w) {
                self.while_active(w, &clicks, input);
            }
        }

        // `NewMoveCharacterScene` (ZzzScene.cpp:940-1027): Esc opens the system menu if no
        // other window is in front.
        if self.scene == UiScene::Character {
            if input.esc
                && !(self.is_shown(Win::Msg)
                    || self.is_shown(Win::CharMake)
                    || self.is_shown(Win::SysMenu)
                    || self.is_shown(Win::Option))
                && self.sys_flag
            {
                self.sounds += 1;
                self.show_win(Win::SysMenu);
            }
        }
        // `NewMoveLogInScene` (ZzzScene.cpp:1254-1267): Esc opens the system menu from the list.
        else if input.esc
            && !(self.is_shown(Win::Msg)
                || self.is_shown(Win::Login)
                || self.is_shown(Win::SysMenu)
                || self.is_shown(Win::Option)
                || self.is_shown(Win::Credit))
            && self.is_shown(Win::MainBar)
            && self.is_shown(Win::ServerSel)
            && self.sys_flag
        {
            self.sounds += 1;
            self.show_win(Win::SysMenu);
        }
        // The original only releases the button from the button itself; if its window was hidden
        // meanwhile, it would stay pressed forever.
        if input.up {
            self.held = None;
        }
        self.caret_clock += input.dt;
    }

    /// `UpdateWhileActive` of each window.
    fn while_active(&mut self, w: Win, clicks: &[Btn], input: &FrameInput) {
        match w {
            Win::MainBar => {
                if clicks.contains(&Btn::Menu) {
                    self.show_win(Win::SysMenu);
                    self.sys_flag = true;
                } else if clicks.contains(&Btn::Credit) {
                    // LoginMainWin.cpp:107-116: the credits (and their music, `credit_music`). The
                    // requested server list is not shown while they are open.
                    self.show_win(Win::Credit);
                    if self.credits.init() {
                        self.close_credits();
                    }
                }
            }
            Win::ServerSel => {
                // ServerSelWin.cpp:403-472.
                for &c in clicks {
                    if let Btn::Group(i) = c {
                        self.selected_group = Some(i);
                        // `SendRequestServerList`: without a server, the reply arrives immediately.
                        self.server_list_received();
                    }
                }
                let Some(g) = self.selected() else { return };
                let chosen = clicks.iter().find_map(|c| match c {
                    Btn::Server(j) => {
                        g.servers.get(*j).map(|info| (g.name.clone(), info.index, info.percent, info.non_pvp, info.id))
                    }
                    _ => None,
                });
                if let Some((name, index, percent, non_pvp, id)) = chosen {
                    if percent < 100 {
                        self.hide_win(Win::ServerSel);
                        self.selected_server = Some((name, index, non_pvp));
                        // `SendRequestServerAddress` → the game server greets (`ReceiveJoinServer`)
                        // and then the login window opens.
                        self.net_out.push(NetOut::Connect(id));
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
                } else if clicks.contains(&Btn::SysServerSelect) {
                    // `SendRequestLogOut(2)` → `ReceiveLogOut`: back to the server list.
                    self.hide_win(Win::SysMenu);
                    self.outcome = Some(Outcome::ServerSelect);
                } else if clicks.contains(&Btn::SysOption) {
                    self.hide_win(Win::SysMenu);
                    self.show_win(Win::Option);
                    self.option_display();
                } else if clicks.contains(&Btn::SysClose) {
                    self.sys_flag = false;
                    self.hide_win(Win::SysMenu);
                } else if input.esc && !self.sys_flag {
                    self.sounds += 1;
                    self.hide_win(Win::SysMenu);
                }
            }
            Win::Option => self.option_while_active(clicks, input),
            Win::CharMake => self.char_make_while_active(clicks, input),
            Win::Credit => {
                // CreditWin.cpp:155-169.
                if clicks.contains(&Btn::CreditClose) {
                    self.close_credits();
                } else if input.esc {
                    self.sounds += 1;
                    self.close_credits();
                    self.sys_flag = false;
                }
                if !self.is_shown(Win::Credit) {
                    return;
                }
                let dt = input.dt * 1000.0;
                for class in 0..=CRW_INDEX_NAME {
                    if self.credits.animate_text(class, dt) {
                        // Class 0: `PlayBuffer(SOUND_CLICK01)` and `CloseWin`.
                        self.sounds += 1;
                        self.close_credits();
                        return;
                    }
                }
                self.credits.animate_illust(dt);
            }
        }
    }

    /// `CCreditWin::CloseWin` (CreditWin.cpp:227-235): requests the server list again (which
    /// shows its window and the bar) and the login music (`credit_music` stops requesting `Mutheme`).
    fn close_credits(&mut self) {
        self.hide_win(Win::Credit);
        self.credits.illust_state = Fade::Hide;
        self.server_list_received();
    }

    /// `COptionWin::UpdateWhileActive` (OptionWin.cpp:141-187).
    fn option_while_active(&mut self, clicks: &[Btn], input: &FrameInput) {
        let s = input.screen;
        for i in 0..SLD_MAX {
            // `m_btnThumb.Update()`: the thumb sounds when released over it.
            let thumb = self.thumb_button(s, i);
            self.update_button(&thumb, Win::Option, input.down, input.up);
            let o = slider_origin(s, i);
            let rel = input.cursor.map(|c| c - o);
            self.sliders[i].update(rel, input.held, input.up, input.dt);
        }
        // The checkboxes already changed when pressed (`m_bCheck = !m_bCheck`).
        if clicks.contains(&Btn::OptAutoAttack) {
            self.opts.auto_attack = !self.opts.auto_attack;
        } else if clicks.contains(&Btn::OptWhisper) {
            self.opts.whisper_sound = !self.opts.whisper_sound;
        } else if clicks.contains(&Btn::OptSlideHelp) {
            self.opts.slide_help = !self.opts.slide_help;
        } else if clicks.contains(&Btn::OptClose) {
            self.hide_win(Win::Option);
            self.sys_flag = false;
        } else if self.sliders[SLD_EFFECT_VOL].state != 0 {
            // `SetEffectVolumeLevel`.
            let pos = self.sliders[SLD_EFFECT_VOL].pos.clamp(0, 10) as u8;
            if self.sfx != pos {
                self.sfx = pos;
                self.set_sfx = Some(pos);
            }
        } else if self.sliders[SLD_MUSIC_VOL].state != 0 {
            let pos = self.sliders[SLD_MUSIC_VOL].pos.clamp(0, 10) as u8;
            if self.music != pos {
                self.music = pos;
                self.set_music = Some(pos);
            }
        } else if self.sliders[SLD_RENDER_LV].state != 0 {
            let pos = self.sliders[SLD_RENDER_LV].pos.clamp(0, 4) as u8;
            if self.opts.render_level != pos {
                self.opts.render_level = pos;
            }
        } else if input.esc {
            self.sounds += 1;
            self.hide_win(Win::Option);
            self.sys_flag = false;
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
            // `SendRequestLogIn`: the reply arrives in `net_message` (`ReceiveLogIn`).
            let tick = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u32);
            self.net_out.push(NetOut::Game(mu_protocol::game::ClientMessage::login(&self.id.text, &self.pass.text, tick)));
        }
    }

    /// `CLoginWin::CancelLogin`: `ConnectConnectionServer`, the server list again.
    fn cancel_login(&mut self) {
        self.hide_win(Win::Login);
        self.net_out.push(NetOut::Close);
        self.net_out.push(NetOut::ServerList);
    }

    /// `ReceiveServerList`: the Connect Server list in one "mu-rs" group (Non-PvP). Each
    /// server's number in its group is `id % 20 + 1`.
    fn receive_server_list(&mut self, list: &[(u16, u8)]) {
        let servers: Vec<ServerInfo> =
            list.iter().map(|&(id, load)| ServerInfo { id, index: (id % 20) as i32 + 1, percent: load as i32, non_pvp: 1 }).collect();
        let pvp = !servers.iter().any(|s| s.non_pvp & 1 != 0);
        self.groups = vec![ServerGroup { name: "mu-rs".into(), pos: GroupPos::Left, btn: usize::MAX, pvp, servers }];
        self.server_list_received();
    }

    /// What arrives from the network.
    fn net_status(&mut self, status: &crate::net::NetStatus) {
        use crate::net::NetStatus;
        match status {
            NetStatus::ServerList(list) if self.scene == UiScene::Login => self.receive_server_list(list),
            NetStatus::ConnectFailed(why) if self.scene == UiScene::Login => {
                warn!("Connect Server: {why}");
                self.popup(MsgCode::ServerLost);
            }
            NetStatus::GameLost(_) => {
                if !self.is_shown(Win::Msg) || self.msg.code != Some(MsgCode::ServerLost) {
                    self.hide_win(Win::Login);
                    self.popup(MsgCode::ServerLost);
                }
            }
            _ => {}
        }
    }

    fn net_message(&mut self, msg: &mu_protocol::game::ServerMessage) {
        use mu_protocol::game::{LoginResult, ServerMessage};
        match msg {
            ServerMessage::GameServerEntered { success, .. } if self.scene == UiScene::Login => {
                if *success {
                    self.show_win(Win::Login);
                    self.first_load = true;
                } else {
                    self.popup(MsgCode::ServerLost);
                }
            }
            ServerMessage::LoginResponse(r) if self.scene == UiScene::Login => match r {
                LoginResult::Okay => self.outcome = Some(Outcome::Enter(self.id.text.clone())),
                LoginResult::InvalidPassword => self.popup(MsgCode::LoginFailPassword),
                LoginResult::AccountInvalid => self.popup(MsgCode::LoginFailId),
                LoginResult::AccountAlreadyConnected => self.popup(MsgCode::LoginFailConnected),
                LoginResult::ServerIsFull => self.popup(MsgCode::LoginFailServerBusy),
                LoginResult::AccountBlocked | LoginResult::TemporaryBlocked => self.popup(MsgCode::LoginFailBlocked),
                LoginResult::WrongVersion => self.popup(MsgCode::LoginFailVersion),
                LoginResult::ConnectionClosed3Fails => self.popup(MsgCode::LoginFailError),
                _ => self.popup(MsgCode::LoginFailConnect),
            },
            _ if self.scene == UiScene::Character => self.char_net_message(msg),
            _ => {}
        }
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
            // Wrong password or account, already connected...: the login window again.
            Some(
                MsgCode::LoginFailPassword
                | MsgCode::LoginFailId
                | MsgCode::LoginFailConnected
                | MsgCode::LoginFailBlocked,
            ) => {
                self.show_win(Win::Login);
                self.give_focus(Field::Pass, true);
            }
            // No connection: to the server list (`ConnectConnectionServer`).
            Some(
                MsgCode::ServerLost
                | MsgCode::LoginFailServerBusy
                | MsgCode::LoginFailVersion
                | MsgCode::LoginFailConnect
                | MsgCode::LoginFailError,
            ) if self.scene == UiScene::Login => {
                self.net_out.push(NetOut::Close);
                self.net_out.push(NetOut::ServerList);
            }
            Some(MsgCode::ServerLost) => self.outcome = Some(Outcome::ServerSelect),
            Some(MsgCode::DeleteConfirm) => self.popup(MsgCode::DeleteResident),
            Some(MsgCode::DeleteResident) => self.request_delete(),
            Some(MsgCode::CreateFail) => self.show_char_make(),
            _ => {}
        }
    }

    fn msg_cancel(&mut self) {
        self.msg.code = None;
        self.hide_win(Win::Msg);
    }

    /// `CLoginWin::UpdateWhileShow`: the text boxes (click to focus and keyboard).
    fn login_while_shown(&mut self, s: Screen, input: &FrameInput) {
        if self.first_load {
            // `FirstLoad` (LoginWin.cpp:165-172): with a saved account, go to the password.
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
        self.type_keys(input);
    }

    /// Keys for the focused box (Windows `edit`).
    fn type_keys(&mut self, input: &FrameInput) {
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
                Key::Paste => b.paste(&crate::clipboard::get()),
                Key::Copy => {
                    if let Some(t) = b.copy_text() {
                        crate::clipboard::set(t);
                    }
                }
                Key::Cut => {
                    if let Some(t) = b.cut() {
                        crate::clipboard::set(&t);
                    }
                }
                Key::Select => b.select_all = !b.text.is_empty(),
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
                    // `SetTabTarget`: the other box, fully selected (login only).
                    match field {
                        Field::Id => self.give_focus(Field::Pass, true),
                        Field::Pass => self.give_focus(Field::Id, true),
                        _ => continue,
                    }
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

/// `SeparateTextIntoLines` with `MW_MSG_ROW_MAX` (52) and at most 2 lines: splits at spaces.
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


#[derive(Clone, Copy, PartialEq, Eq)]
enum BtnState {
    Up,
    Down,
    Active,
}

/// Button (`CButton`): rectangle, texture with the frames stacked vertically and optional text.
struct ButtonDef {
    id: Btn,
    rect: IRect,
    tex: Tex,
    /// Normal, pressed and hover frames.
    frames: [u32; 3],
    /// Selected frame (`nCheckUpFrame`), for all three states.
    check_frame: Option<u32>,
    checked: bool,
    /// Text (with `g_hFixFont`, centered) and its normal, pressed and hover colors.
    text: Option<(String, [[u8; 3]; 3])>,
}

impl ButtonDef {
    /// 3-frame button with the text painted on the texture (`Create(w, h, tex, 3, 2, 1)`).
    fn image(id: Btn, rect: IRect, tex: Tex) -> Self {
        Self { id, rect, tex, frames: [0, 2, 1], check_frame: None, checked: false, text: None }
    }
}

// --- Input ---------------------------------------------------------------------------------------

/// What was pressed this frame (`CInput`).
struct FrameInput {
    /// Mouse in reference screen pixels.
    cursor: Option<IVec2>,
    /// Reference screen of this frame.
    screen: Screen,
    down: bool,
    /// Left button held (`IsLBtnHeldDn`).
    held: bool,
    up: bool,
    esc: bool,
    enter: bool,
    ctrl: bool,
    /// Keys for the text boxes (with repeat).
    keys: Vec<Key>,
    dt: f32,
}

/// Size of the virtual screen (interface units) and of the reference one.
fn screen_size(window: Option<&Window>, scale: f32) -> (Vec2, Screen) {
    let ui = window
        .filter(|w| w.width() >= 1.0 && w.height() >= 1.0)
        .map_or(Vec2::new(640.0, 480.0), |w| Vec2::new(w.width(), w.height()) / scale.max(1e-3));
    // Rounded: 1024 / 1.6 can come out as 639.99994.
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
    audio: Option<Res<crate::sound::Audio>>,
    mut opts: ResMut<crate::options::GameOptions>,
    mut args: ResMut<Args>,
    mut load: ResMut<LoadStep>,
    mut next: ResMut<NextState<GameScene>>,
    mut exit: MessageWriter<AppExit>,
    (mut net, mut status, mut from): (
        ResMut<crate::net::Net>,
        MessageReader<crate::net::NetStatus>,
        MessageReader<crate::net::FromServer>,
    ),
) {
    // What arrived from the network, before the pass (`ProtocolCompiler`).
    for st in status.read() {
        ui.net_status(st);
    }
    for m in from.read() {
        ui.net_message(&m.0);
    }
    let window = windows.single().ok();
    let (_, s) = screen_size(window, scale.0);
    let cursor = window
        .and_then(|w| w.cursor_position())
        .map(|c| (c / scale.0.max(1e-3) / K).floor().as_ivec2());
    // `g_pOption` and the sound.rs volume (the game options can change it).
    (ui.sfx, ui.music) = crate::sound::volumes(audio.as_deref(), &sound);
    ui.opts = *opts;
    let input = FrameInput {
        cursor,
        screen: s,
        down: mouse.just_pressed(MouseButton::Left),
        held: mouse.pressed(MouseButton::Left),
        up: mouse.just_released(MouseButton::Left),
        esc: keys.just_pressed(KeyCode::Escape),
        enter: keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter),
        ctrl: keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]),
        keys: crate::clipboard::typed_keys(typed.read().cloned(), &keys),
        dt: time.delta_secs(),
    };
    ui.update(s, &input);
    for _ in 0..std::mem::take(&mut ui.sounds) {
        // `PlayBuffer(SOUND_CLICK01)`.
        sound.play("SOUND_CLICK01");
    }
    if let Some(v) = ui.set_sfx.take() {
        sound.set_sfx_volume(v);
    }
    if let Some(v) = ui.set_music.take() {
        sound.set_music_volume(v);
    }
    if *opts != ui.opts {
        *opts = ui.opts;
    }
    for out in std::mem::take(&mut ui.net_out) {
        match out {
            NetOut::ServerList => net.request_server_list(),
            NetOut::Connect(id) => net.connect_to_server(id),
            NetOut::Game(msg) => net.send(msg),
            NetOut::Close => net.close(),
        }
    }
    match ui.outcome.take() {
        Some(Outcome::Enter(id)) => {
            crate::settings::save_value("usuario", &id);
            crate::scene::load_world(&mut args, &mut load, crate::scene::CHARACTER_WORLD, false, GameScene::CharacterSelect);
            next.set(GameScene::Loading);
        }
        Some(Outcome::Exit) => {
            // `SendRequestLogOut(0)`.
            net.send(mu_protocol::game::ClientMessage::Logout { kind: 0 });
            net.close();
            exit.write(AppExit::Success);
        }
        Some(Outcome::ServerSelect) => {
            // `SendRequestLogOut(2)`: back to the server list, disconnected.
            net.send(mu_protocol::game::ClientMessage::Logout { kind: 2 });
            net.close();
            crate::scene::load_world(&mut args, &mut load, crate::scene::LOGIN_WORLD, false, GameScene::Login);
            next.set(GameScene::Loading);
        }
        None => {}
    }
}

// --- Textures ------------------------------------------------------------------------------------

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
    OptTop,
    CheckBtn,
    SliderThumb,
    SliderGauge,
    SliderBack,
    CloseBtn,
    CreditLogo,
    /// Credits illustration (`ILLUST`), loaded when needed (`LoadIllust`).
    Illust(u8),
    /// Delete password box (`BITMAP_MSG_WIN_INPUT`).
    MsgInput,
    /// Name background in the create window (character selection's `BITMAP_LOG_IN`).
    CharId,
    /// Create window preview (the image is set by charselect.rs).
    Preview,
}

impl Tex {
    const ALL: [Tex; 26] = [
        Tex::MsgInput,
        Tex::CharId,
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
        Tex::OptTop,
        Tex::CheckBtn,
        Tex::SliderThumb,
        Tex::SliderGauge,
        Tex::SliderBack,
        Tex::CloseBtn,
        Tex::CreditLogo,
    ];

    /// `OpenLogoSceneData` (ZzzOpenData.cpp:4866-4890) and the common ones (ZzzOpenData.cpp:5254-5269).
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
            Tex::OptTop => "op2_back1.tga",
            Tex::CheckBtn => "op2_ch.tga",
            Tex::SliderThumb => "op2_volume3.tga",
            Tex::SliderGauge => "op2_volume2.jpg",
            Tex::SliderBack => "op2_volume1.tga",
            Tex::CloseBtn => "message_close_b_all.tga",
            Tex::CreditLogo => "cr_mu_lo.tga",
            Tex::Illust(i) => ILLUST[i as usize % ILLUST.len()],
            Tex::MsgInput => "delete_secret_number.tga",
            Tex::CharId => "cha_id.tga",
            Tex::Preview => "",
        }
    }
}

#[derive(Resource)]
struct LoginTextures(HashMap<Tex, Handle<Image>>);

// --- Logo ------------------------------------------------------------------------------------------

/// Logo and glow with the original's blending: the glow is additive (`additive`) and the logo discards
/// anything with alpha 0.25 or less.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
#[bind_group_data(LogoKey)]
struct LogoMaterial {
    /// `glColor4f` (the color already converted to linear).
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
            // `EnableAlphaTest`: alpha blending and `glAlphaFunc(GL_GREATER, 0.25)`.
            fragment.shader_defs.push("LOGO_ALPHA_TEST".into());
        }
    }
}

#[derive(Component)]
struct LogoPart {
    glow: bool,
    material: Handle<LogoMaterial>,
}

/// `g_fMULogoAlpha += 0.02` up to 10 (ZzzScene.cpp:1405-1406), per tick.
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
        // `glColor4f(a − 0.3, …)` for the glow and `glColor4f(a, …)` for the logo (clamped to 0..1).
        // The original blends in sRGB and Bevy in linear: the color is converted to linear so the
        // result looks the same.
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

// --- Creation ------------------------------------------------------------------------------------

#[derive(Component)]
struct LoginUiRoot;

/// Window layer: its children are rebuilt when what must be drawn changes.
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
    commands.insert_resource(LoginUi::new(&saved, load_credits(&args.data)));

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
            // Logo: (320 − 128 × 0.8, 25) and 256 × 0.8 by 128 × 0.8 on 640×480 (ZzzScene.cpp:1415-1423).
            // First the glow (additive) and the logo on top.
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

            // Bottom lines (ZzzScene.cpp:1430-1449): `g_hFont`, white on 50 % black,
            // with the text baseline 1 unit from the edge: the copyright ends at x = 335 and the
            // next part starts there; the version, on the left.
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
            // `GlobalText[456]` = "Ver %s" with the executable's version.
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

// --- Window drawing ------------------------------------------------------------------------------

/// One piece of what must be drawn, in reference screen pixels.
#[derive(Clone, PartialEq, Debug)]
enum Item {
    /// Image (or the `src` part) in a rectangle; `tile` repeats the texture instead of stretching it.
    Image { tex: Tex, x: f32, y: f32, w: f32, h: f32, src: Option<Rect>, tile: bool, alpha: u8 },
    /// Credits line: 18-size bold font, centered in `w`.
    BigText { x: f32, y: f32, w: f32, text: String, color: [u8; 3] },
    /// Color rectangle (`RenderColor`).
    Fill { x: f32, y: f32, w: f32, h: f32, color: [u8; 4] },
    /// Text with its top-left corner at (x, y); with `center`, centered in that width.
    Text { x: f32, y: f32, center: Option<f32>, text: String, color: [u8; 3] },
    /// Text box: what is before and after the caret, whether everything is selected and whether the caret shows.
    Input { x: f32, y: f32, w: f32, h: f32, before: String, after: String, selected: bool, caret: bool },
}

impl LoginUi {
    /// What is drawn, from the back window to the front one (`CUIMng::Render`).
    fn items(&self, s: Screen) -> Vec<Item> {
        let mut out = Vec::new();
        for &w in self.order.iter().rev() {
            if !self.is_shown(w) {
                continue;
            }
            match w {
                Win::MainBar => {
                    // `m_sprDeco` with its anchor at (105, 59), behind the Credit button.
                    let c = credit_rect(s);
                    out.push(image(Tex::Deco, IRect::new(c.x - 105, c.y - 59, 189, 103)));
                    self.push_buttons(s, w, &mut out);
                }
                Win::ServerSel => {
                    self.push_buttons(s, w, &mut out);
                    if let Some(g) = self.selected() {
                        // 160×4 `CGaugeBar`: the left part is shown, `percent` % of the texture.
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
                                    alpha: 255,
                                });
                            }
                        }
                        if g.pvp {
                            // On the 640×480 screen, not in the window (ServerSelWin.cpp:491-497).
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
                    // `CWinEx`: stone tiled inside (3 px from the edge), header, footer and sides.
                    let r = self.sys_rect(s);
                    let side = r.h - 64 - 43;
                    out.push(tiled(Tex::SysStone, IRect::new(r.x + 3, r.y + 3, r.w - 6, r.h - 6)));
                    out.push(image(Tex::SysTop, IRect::new(r.x, r.y, 213, 64)));
                    out.push(image(Tex::SysBottom, IRect::new(r.x, r.y + 64 + side, 213, 43)));
                    out.push(tiled(Tex::SysLeft, IRect::new(r.x, r.y + 64, 5, side)));
                    out.push(tiled(Tex::SysRight, IRect::new(r.x + r.w - 5, r.y + 64, 5, side)));
                    self.push_buttons(s, w, &mut out);
                }
                Win::Option => self.push_option(s, &mut out),
                Win::Credit => self.push_credits(s, &mut out),
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
                    if self.msg.kind == MsgKind::Input {
                        // `m_sprInput` (171×23) and the password box at (+10, +8) from it.
                        let r = IRect::new(m.x + 32, m.y + 72 + 4, 171, 23);
                        out.push(image(Tex::MsgInput, r));
                        out.push(self.input_item(s, Field::DelPass, Vec2::new(s.snap_x((r.x + 10) as f32), s.snap_y((r.y + 8) as f32))));
                    }
                    self.push_buttons(s, w, &mut out);
                }
                Win::CharMake => self.push_char_make(s, &mut out),
            }
        }
        out
    }

    /// Text box `field` with the text starting at `origin`.
    fn input_item(&self, s: Screen, field: Field, origin: Vec2) -> Item {
        let b = self.field(field);
        let shown = b.shown();
        let caret_at = shown.char_indices().nth(b.caret).map_or(shown.len(), |(i, _)| i);
        let focused = self.focus == Some(field);
        Item::Input {
            x: origin.x,
            y: origin.y,
            w: INPUT_W * s.rate_x(),
            h: INPUT_H * s.rate_y(),
            before: shown[..caret_at].to_string(),
            after: shown[caret_at..].to_string(),
            selected: focused && b.select_all,
            caret: focused && (self.caret_clock % CARET_PERIOD) < CARET_PERIOD / 2.0,
        }
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
            out.push(Item::Image { tex: b.tex, x, y, w, h, src: Some(Rect::new(0.0, top, w, top + h)), tile: false, alpha: 255 });
            if let Some((text, colors)) = &b.text {
                // `CButton::Render` (Button.cpp:200-209): centered in the width, at half height and
                // half a point lower (a point and a half when pressed) on the 640×480 screen.
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

    /// `COptionWin::RenderControls` (OptionWin.cpp:189-227).
    fn push_option(&self, s: Screen, out: &mut Vec<Item>) {
        // Full-screen `CWin`: 50 % black.
        out.push(Item::Fill { x: 0.0, y: 0.0, w: s.w as f32, h: s.h as f32, color: [0, 0, 0, 128] });
        // `CWinEx`: the stone tiled inside, `op2_back1` header, footer and sides.
        let r = opt_rect(s);
        out.push(tiled(Tex::SysStone, IRect::new(r.x + 3, r.y + 3, r.w - 6, r.h - 6)));
        out.push(image(Tex::OptTop, IRect::new(r.x, r.y, OPT_W, 65)));
        out.push(image(Tex::SysBottom, IRect::new(r.x, r.y + 65 + OPT_SIDE, OPT_W, 43)));
        out.push(tiled(Tex::SysLeft, IRect::new(r.x, r.y + 65, 5, OPT_SIDE)));
        out.push(tiled(Tex::SysRight, IRect::new(r.x + r.w - 5, r.y + 65, 5, OPT_SIDE)));

        let label = |x: i32, y: i32, text: String| Item::Text {
            x: s.snap_x(x as f32),
            y: s.snap_y(y as f32),
            center: None,
            text,
            color: WHITE,
        };
        // `GlobalText[385]` centered at the top.
        out.push(Item::Text {
            x: s.snap_x(r.x as f32),
            y: s.snap_y((r.y + 10) as f32),
            center: Some((r.w as f32 / s.rate_x()).floor() * s.rate_x()),
            text: TEXT_OPTION.into(),
            color: WHITE,
        });
        for (i, t) in [TEXT_AUTO_ATTACK, TEXT_WHISPER, TEXT_SLIDE_HELP].into_iter().enumerate() {
            let c = check_rect(s, i);
            out.push(label(c.x + 24, c.y + 4, t.into()));
        }
        // Sliders: the name 18 above, the value on the right (the effects limit is
        // shown as `level × 2 + 5`), and the slider: background, tiled color and thumb.
        let values = [self.sfx as i32, self.music as i32, self.opts.render_level as i32 * 2 + 5];
        for (i, (name, value)) in [TEXT_VOLUME, TEXT_MUSIC, TEXT_EFFECT_LIMIT].into_iter().zip(values).enumerate() {
            let o = slider_origin(s, i);
            out.push(label(o.x, o.y - 18, name.into()));
            out.push(label(o.x + 85, o.y - 18, value.to_string()));
            let slider = &self.sliders[i];
            out.push(image(Tex::SliderBack, IRect::new(o.x, o.y, SLD_W, SLD_H)));
            let gauge = slider.gauge();
            if gauge > 0 {
                let (gx, gy, _, gb) = GAUGE_RECT;
                out.push(tiled(Tex::SliderGauge, IRect::new(o.x + gx, o.y + gy, gauge, gb - gy)));
            }
            out.push(image(Tex::SliderThumb, IRect::new(o.x + slider.thumb, o.y, THUMB, THUMB)));
        }
        self.push_buttons(s, Win::Option, out);
    }

    /// `CCreditWin::RenderControls` (CreditWin.cpp:171-225).
    fn push_credits(&self, s: Screen, out: &mut Vec<Item>) {
        let c = &self.credits;
        // `SetBgAlpha(255)`: opaque black full-screen background.
        out.push(Item::Fill { x: 0.0, y: 0.0, w: s.w as f32, h: s.h as f32, color: [0, 0, 0, 255] });
        // 400×400 illustrations at (0, 126) and (400, 126) of the 800×600 screen.
        if c.illust_state != Fade::Hide {
            let alpha = c.illust_alpha.clamp(0.0, 255.0) as u8;
            for side in 0..2u8 {
                let tex = Tex::Illust(c.illust * 2 + side);
                out.push(sprite800(s, tex, 400.0 * side as f32, 126.0, 400.0, 400.0, alpha));
            }
        }
        out.push(image(Tex::Deco, credit_deco_rect(s)));
        out.push(sprite800(s, Tex::CreditLogo, 241.0, 549.0, 290.0, 41.0, 255));

        // Department and team in light gray and the names in light yellow, centered on the
        // 640×480 screen (or in columns of a quarter or a third of the width).
        let (rx, ry) = (s.rate_x(), s.rate_y());
        let line = |x: f32, y: f32, w: f32, text: &str, color| Item::BigText { x: x * rx, y: y * ry, w, text: text.into(), color };
        let full = s.w as f32;
        out.push(line(0.0, 20.0, full, c.name(CRW_INDEX_DEPARTMENT), BR_GRAY));
        out.push(line(0.0, 46.0, full, c.name(CRW_INDEX_TEAM), BR_GRAY));
        let columns: &[f32] = match c.name_count {
            1 => &[0.0],
            2 => &[160.0, 320.0],
            3 => &[0.0, 213.0, 426.0],
            4 => &[0.0, 160.0, 320.0, 480.0],
            _ => &[],
        };
        let w = match c.name_count {
            1 => full,
            3 => full / 3.0,
            _ => full / 4.0,
        };
        for (i, &x) in columns.iter().enumerate() {
            out.push(line(x, 72.0, w, c.name(CRW_INDEX_NAME + i), BR_YELLOW));
        }
        // The three 800×42 black strips that cover (and uncover) each line.
        for (i, a) in c.hide_alpha.iter().enumerate() {
            let (sx, sy) = (s.w as f32 / 800.0, s.h as f32 / 600.0);
            out.push(Item::Fill {
                x: 0.0,
                y: 42.0 * i as f32 * sy,
                w: 800.0 * sx,
                h: 42.0 * sy,
                color: [0, 0, 0, a.clamp(0.0, 255.0) as u8],
            });
        }
        self.push_buttons(s, Win::Credit, out);
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
        // `GlobalText[461]` "[%s-%d Server]" or `[460]` "[%s-%d(Non-PvP) Server]".
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
    Item::Image { tex, x: r.x as f32, y: r.y as f32, w: r.w as f32, h: r.h as f32, src: None, tile: false, alpha: 255 }
}

fn tiled(tex: Tex, r: IRect) -> Item {
    Item::Image { tex, x: r.x as f32, y: r.y as f32, w: r.w as f32, h: r.h as f32, src: None, tile: true, alpha: 255 }
}

/// `CSprite` of `org_w`×`org_h` (the top-left part of the texture) on the stretched
/// 800×600 screen (`SPR_SIZING_DATUMS_LT` with `fScaleX/Y` = screen / 800×600).
fn sprite800(s: Screen, tex: Tex, x: f32, y: f32, org_w: f32, org_h: f32, alpha: u8) -> Item {
    let (sx, sy) = (s.w as f32 / 800.0, s.h as f32 / 600.0);
    Item::Image {
        tex,
        x: x * sx,
        y: y * sy,
        w: org_w * sx,
        h: org_h * sy,
        src: Some(Rect::new(0.0, 0.0, org_w, org_h)),
        tile: false,
        alpha,
    }
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
        Item::Image { tex, x, y, w, h, src, tile, alpha } => {
            let Some(handle) = textures.0.get(tex) else { return };
            let mut img = ImageNode::new(handle.clone());
            img.rect = *src;
            if *alpha < 255 {
                img.color = Color::srgba_u8(255, 255, 255, *alpha);
            }
            if *tile {
                // One repetition per original texture pixel.
                img.image_mode = NodeImageMode::Tiled { tile_x: true, tile_y: true, stretch_value: K };
            }
            p.spawn((absolute(*x, *y, *w, *h), img));
        }
        Item::Fill { x, y, w, h, color: [r, g, b, a] } => {
            p.spawn((absolute(*x, *y, *w, *h), BackgroundColor(Color::srgba_u8(*r, *g, *b, *a))));
        }
        Item::BigText { x, y, w, text, color } => {
            let mut node = absolute(*x, *y, *w, CREDIT_FONT);
            node.justify_content = JustifyContent::Center;
            let (mut font, line) = text_font(CREDIT_FONT);
            font.weight = FontWeight::BOLD;
            p.spawn(node).with_children(|c| {
                c.spawn((Text::new(text.clone()), font, line, TextColor(srgb(*color)), TextLayout::no_wrap()));
            });
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
                // The caret takes no space: 2 px wide and the font height, in gray (200, 200, 200).
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

/// Rebuilds the windows when something drawn changes.
#[allow(clippy::too_many_arguments)]
fn draw_windows(
    mut commands: Commands,
    ui: Res<LoginUi>,
    mut textures: ResMut<LoginTextures>,
    args: Res<Args>,
    mut images: ResMut<Assets<Image>>,
    windows: Query<&Window>,
    scale: Res<UiScale>,
    layer: Query<Entity, With<WindowsLayer>>,
    mut last: Local<(Option<Entity>, Vec<Item>)>,
    mut missing: Local<Vec<Tex>>,
) {
    let Ok(layer) = layer.single() else { return };
    let (_, s) = screen_size(windows.single().ok(), scale.0);
    let items = ui.items(s);
    // The layer is a different one each time the login is entered.
    if last.0 == Some(layer) && last.1 == items {
        return;
    }
    // `LoadIllust`: the credits illustrations are loaded when needed (once).
    for item in &items {
        if let Item::Image { tex: tex @ Tex::Illust(_), .. } = item
            && !textures.0.contains_key(tex)
            && !missing.contains(tex)
        {
            match crate::scene::interface_image(&args, &mut images, tex.file()) {
                Some((h, _)) => {
                    textures.0.insert(*tex, h);
                }
                None => missing.push(*tex),
            }
        }
    }
    commands.entity(layer).despawn_children();
    commands.entity(layer).with_children(|p| {
        for item in &items {
            spawn_item(p, item, &textures);
        }
    });
    *last = (Some(layer), items);
}
