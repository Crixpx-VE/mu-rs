//! Market Union Member Julia's window (`INTERFACE_UNITEDMARKETPLACE_NPC_JULIA`): the warp between
//! Lorencia and Loren Market (`WD_79UNITEDMARKETPLACE`). Ported from MuMain's
//! `CUnitedMarketPlaceWindow` (UI/Inventory/UnitedMarketPlaceWindow.cpp, its RmlUi document
//! Data/Interface/RmlUi/united_market_place.rml and the legacy theme's
//! themes/legacy/united_market_place.rcss + docked_panel_frame.rcss), cross-checked with Main 5.2's
//! `CNewUIUnitedMarketPlaceWindow` (NewUIUnitedMarketPlaceWindow.cpp), which the legacy theme
//! reproduces.
//!
//! - **Opening**: `ReceiveTalk` with `Value` 0x25 (MuMain Network/Server/WSclient.cpp:6607-6611,
//!   Main 5.2 WSclient.cpp:5967-5971) after `HideAll` (npc.rs); `Show` runs only `OpeningProcess`,
//!   which unlocks the Warp button (UnitedMarketPlaceWindow.cpp:128-131, MuMain
//!   UI/Core/WindowSystem.cpp:1111-1114). It sits at `640-190, 0` like the other right-hand
//!   windows (Main 5.2 NewUISystem.cpp:460-461; MuMain's "united_marketplace" docked slot,
//!   WindowSystem.cpp:580).
//! - **Texts** (`SyncRmlModel`, UnitedMarketPlaceWindow.cpp:222-249): seven rows; in Loren Market
//!   the way back to town (three lines, three empty rows, the question), elsewhere the market.
//! - **Warp** (`Update`, 97-110): unless locked, `LoadingWorld = 9999999`,
//!   `SendEnterMarketPlaceRequest` ([`ClientMessage::EnterMarketPlace`]) and the button locks until
//!   the window opens again. OpenMU warps through the target map's safe-zone gate and answers with
//!   `MapChanged`; the map change closes the window (`HideAll`, npc.rs).
//! - **Closing**: the exit button, the top-right corner, Esc with `SOUND_CLICK01` (`UpdateKeyEvent`,
//!   79-91, npc.rs), walking or a map change (npc.rs). `ClosingProcess` sends
//!   `SendCloseNpcRequest` in MuMain (133-136; commented out in Main 5.2): npc.rs.

use bevy::prelude::*;
use mu_protocol::game::ClientMessage;

use crate::Args;
use crate::char_info::{absolute, font_size, label};
use crate::npc::{NpcDialog, NpcDialogSystems, WindowKind};
use crate::npc_dialogue::inside;
use crate::scene::GameScene;

pub struct MarketWindowPlugin;

impl Plugin for MarketWindowPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MarketWindow>()
            .add_systems(OnEnter(GameScene::Game), spawn_root)
            .add_systems(PreUpdate, follow_dialog.after(NpcDialogSystems).after(crate::net::NetSystems))
            .add_systems(Update, (update_mouse, draw).chain().run_if(in_state(GameScene::Game)))
            .add_systems(OnExit(GameScene::Game), crate::scene::despawn::<MarketScreen>)
            .add_systems(OnExit(GameScene::Game), |mut w: ResMut<MarketWindow>| *w = MarketWindow::default());
    }
}

// --- Measurements (640×480 screen) -------------------------------------------------------------

/// `Create(..., 640-190, 0)` (Main 5.2 NewUISystem.cpp:461); `INVENTORY_WIDTH` × `INVENTORY_HEIGHT`.
const POS: Vec2 = Vec2::new(640.0 - 190.0, 0.0);
const W: f32 = 190.0;
const H: f32 = 429.0;
/// `#warp`: 53 × 23 at (68, 230) (legacy united_market_place.rcss; Main 5.2's
/// `INVENTORY_WIDTH / 2 - 27`, NewUIUnitedMarketPlaceWindow.cpp:49 and 65).
const BTN_WARP: (f32, f32, f32, f32) = (68.0, 230.0, 53.0, 23.0);
/// `#btn_exit`: 36 × 29, 13 from the left and 8 above the bottom (legacy docked_panel_frame.rcss),
/// Main 5.2's (13, 392) (NewUIUnitedMarketPlaceWindow.cpp:52).
const BTN_EXIT: (f32, f32, f32, f32) = (13.0, 392.0, 36.0, 29.0);
/// `#frame_corner_close`: 24 × 22, 1 from the top and 3 from the right (legacy base.rcss:588-596).
/// Main 5.2 checks a 13 × 12 box at (169, 7) instead (`BtnProcess`, 280-285); MuMain is followed.
const CORNER_CLOSE: (f32, f32, f32, f32) = (W - 3.0 - 24.0, 1.0, 24.0, 22.0);
/// `#title`: from x 15, centred on 160, at y 13 (legacy united_market_place.rcss; `RenderFrame`,
/// Main 5.2 NewUIUnitedMarketPlaceWindow.cpp:267-275).
const TITLE: (f32, f32, f32) = (15.0, 13.0, 160.0);
/// `.market-line`: centred on 190, every 20 from y 60 (legacy united_market_place.rcss; Main 5.2's
/// `m_Pos.y + 50 + 10, 30, ...`, NewUIUnitedMarketPlaceWindow.cpp:160-181).
const LINES_TOP: f32 = 60.0;
const LINE_GAP: f32 = 20.0;

/// `WD_79UNITEDMARKETPLACE` as the map folder number ([`Args::world`] = `WorldActive + 1`).
const LOREN_MARKET_WORLD: u16 = 79 + 1;

/// Colours (legacy united_market_place.rcss): the title and description `#dcdcdc`, the Warp label
/// white, and the locked button and its label `#646464` (Main 5.2's `RGBA(100, 100, 100, 255)`,
/// NewUIUnitedMarketPlaceWindow.cpp:184-199).
const TEXT_COLOR: [u8; 3] = [220, 220, 220];
const LABEL_COLOR: [u8; 3] = [255, 255, 255];
const LOCKED: [u8; 3] = [100, 100, 100];
/// `#btn_exit:hover { filter: brightness(1.25); }` (legacy docked_panel_frame.rcss). Main 5.2 draws
/// the button's pressed frame instead (`ChangeButtonImgState(true, ...)`, frames up 0 / down 1).
const EXIT_HOVER: f32 = 1.25;

// --- Texts -------------------------------------------------------------------------------------

/// `I18N::Game::Julia` (MuMain Localization/Game.en.resx:7666; Main 5.2's `GlobalText[3000]`).
const TITLE_TEXT: &str = "Julia";
/// `I18N::Game::Warp3016` (Game.en.resx:7714; `GlobalText[3016]`).
const WARP_TEXT: &str = "Warp";
/// `I18N::Game::Close388` (Game.en.resx:1454; Main 5.2 used `GlobalText[1002]`, also "Close").
const EXIT_TIP: &str = "Close";
/// Outside Loren Market (Game.en.resx:7675-7693; `GlobalText[3003..=3009]`).
const TO_MARKET: [&str; 7] = [
    "If you go to the market in Lorencia,",
    "you'll find many items you need",
    "available for purchase.",
    "If you have items you want to sell,",
    "you can sell them",
    "at the market.",
    "Would you like to go to the market?",
];
/// In Loren Market (Game.en.resx:7696-7705; `GlobalText[3010..=3013]`): three empty rows keep the
/// question on the seventh (UnitedMarketPlaceWindow.cpp:226-233).
const TO_TOWN: [&str; 7] = [
    "Will you be going back to town now?",
    "Have another great day",
    "and stay positive at all times!",
    "",
    "",
    "",
    "Would you like to go to town?",
];

/// The description for the map folder `world`.
fn lines(world: u16) -> &'static [&'static str; 7] {
    if world == LOREN_MARKET_WORLD { &TO_TOWN } else { &TO_MARKET }
}

// --- State -------------------------------------------------------------------------------------

/// What a click asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Click {
    /// `LoadingWorld = 9999999` and `SendEnterMarketPlaceRequest`.
    Warp,
    /// `Hide(INTERFACE_UNITEDMARKETPLACE_NPC_JULIA)`.
    Close,
}

/// The window: `IsVisible(INTERFACE_UNITEDMARKETPLACE_NPC_JULIA)` and its state.
#[derive(Resource, Default)]
pub struct MarketWindow {
    open: bool,
    /// [`NpcDialog::serial`] of the dialog shown.
    serial: u32,
    /// `m_bIsEnterButtonLocked`.
    locked: bool,
    /// The mouse in the window (window coordinates) and the button held, for the frames.
    cursor: Option<Vec2>,
    held: bool,
}

impl MarketWindow {
    /// The mouse (640×480 screen) is over the window: a click there does not walk
    /// (`UpdateMouseEvent`, UnitedMarketPlaceWindow.cpp:74-77).
    pub fn blocks(&self, c: Vec2) -> bool {
        self.open && inside(c - POS, (0.0, 0.0, W, H))
    }

    /// `Show`: `OpeningProcess` → `LockEnterButton(FALSE)`.
    fn opening(&mut self, serial: u32) {
        self.open = true;
        self.serial = serial;
        self.locked = false;
    }

    /// A release of the left button at `m` (window coordinates): the RmlUi click the original's
    /// `BtnProcess` became (`market_warp` / `market_exit`, UnitedMarketPlaceWindow.cpp:97-113).
    /// A locked Warp button does nothing; a Warp click locks it.
    fn click(&mut self, m: Vec2) -> Option<Click> {
        if inside(m, BTN_WARP) {
            if self.locked {
                return None;
            }
            self.locked = true;
            return Some(Click::Warp);
        }
        (inside(m, BTN_EXIT) || inside(m, CORNER_CLOSE)).then_some(Click::Close)
    }
}

// --- Systems -----------------------------------------------------------------------------------

/// Opens and closes with npc.rs' dialog.
fn follow_dialog(dialog: Res<NpcDialog>, mut w: ResMut<MarketWindow>) {
    let is_julia = dialog.kind() == Some(WindowKind::Julia);
    if w.open && (!is_julia || w.serial != dialog.serial) {
        w.open = false;
        info!("ventana de Julia: se cierra");
    }
    if is_julia && !w.open {
        w.opening(dialog.serial);
        info!("ventana de Julia: se abre");
    }
}

/// The clicks (`Update`, UnitedMarketPlaceWindow.cpp:93-114).
#[allow(clippy::too_many_arguments)]
fn update_mouse(
    windows: Query<&Window>,
    mouse: Res<ButtonInput<MouseButton>>,
    game_ui: Res<crate::options::GameUi>,
    real: Res<Time<Real>>,
    mut w: ResMut<MarketWindow>,
    (mut dialog, mut net, mut inv): (ResMut<NpcDialog>, ResMut<crate::net::Net>, ResMut<crate::inventory::Inventory>),
    mut gate: ResMut<crate::gate::GateState>,
) {
    if !w.open {
        return;
    }
    let cursor = windows
        .single()
        .ok()
        .and_then(crate::mainframe::cursor_virtual)
        .filter(|&c| !game_ui.modal() && !game_ui.blocks(c))
        .map(|c| (c - POS).floor());
    w.cursor = cursor;
    w.held = mouse.pressed(MouseButton::Left);
    let Some(m) = cursor else { return };
    if !mouse.just_released(MouseButton::Left) {
        return;
    }
    match w.click(m) {
        Some(Click::Warp) => {
            info!("ventana de Julia: se pide el mercado");
            gate.warp_requested(real.elapsed_secs_f64());
            if net.connected() {
                net.send(ClientMessage::EnterMarketPlace);
            }
        }
        Some(Click::Close) => crate::npc::close_dialog(&mut dialog, &mut net, &mut inv, "botón cerrar de Julia"),
        None => {}
    }
}

// --- Drawing -----------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Tex {
    Back,
    Top,
    Left,
    Right,
    Bottom,
    Button,
    Exit,
}

impl Tex {
    const ALL: [Tex; 7] = [Tex::Back, Tex::Top, Tex::Left, Tex::Right, Tex::Bottom, Tex::Button, Tex::Exit];

    /// The legacy theme's sprites (docked_panel_frame.rcss with `docked-bg-top-b`, and
    /// united_market_place.rcss), the files of Main 5.2's `LoadImages` (235-245).
    fn file(self) -> &'static str {
        match self {
            Tex::Back => "newui_msgbox_back.jpg",
            Tex::Top => "newui_item_back01.tga",
            Tex::Left => "newui_item_back02-L.tga",
            Tex::Right => "newui_item_back02-R.tga",
            Tex::Bottom => "newui_item_back03.tga",
            Tex::Button => "newui_btn_empty_very_small.tga",
            Tex::Exit => "newui_exit_00.tga",
        }
    }
}

/// A piece of the window (coordinates inside it).
#[derive(Clone, PartialEq, Debug)]
enum Piece {
    /// The w × h part of the texture from (0, sv), its colour multiplied by `tint`.
    Image { tex: Tex, x: f32, y: f32, w: f32, h: f32, sv: f32, tint: f32 },
    /// Text centred in a w × h box (h 0: one line high).
    Text { x: f32, y: f32, w: f32, h: f32, text: String, color: [u8; 3], bold: bool },
    /// The exit button's hint, above it (`DocumentHints`, MuMain UI/RmlBridge/RmlDocumentHints.h;
    /// Main 5.2's `ChangeToolTipText(GlobalText[1002], true)`).
    Tooltip { cx: f32, bottom: f32, text: String },
}

fn img(tex: Tex, x: f32, y: f32, w: f32, h: f32) -> Piece {
    Piece::Image { tex, x, y, w, h, sv: 0.0, tint: 1.0 }
}

/// What the window draws for the map folder `world`.
fn pieces(w: &MarketWindow, world: u16) -> Vec<Piece> {
    let over = |r| w.cursor.is_some_and(|c| inside(c, r));
    let text = |x, y, cw, h, t: &str, color, bold| Piece::Text { x, y, w: cw, h, text: t.to_string(), color, bold };
    // The frame (docked_panel_frame.rcss; Main 5.2 `RenderFrame`, 258-264).
    let mut out = vec![
        img(Tex::Back, 0.0, 0.0, W, H),
        img(Tex::Top, 0.0, 0.0, W, 64.0),
        img(Tex::Left, 0.0, 64.0, 21.0, 320.0),
        img(Tex::Right, W - 21.0, 64.0, 21.0, 320.0),
        img(Tex::Bottom, 0.0, H - 45.0, W, 45.0),
    ];
    let (tx, ty, tw) = TITLE;
    out.push(text(tx, ty, tw, 0.0, TITLE_TEXT, TEXT_COLOR, true));
    for (i, line) in lines(world).iter().enumerate().filter(|(_, l)| !l.is_empty()) {
        out.push(text(0.0, LINES_TOP + LINE_GAP * i as f32, W, 0.0, line, TEXT_COLOR, false));
    }
    // Warp: up / over / down rows 0, 23, 46 (`market-btn-sheet`); locked, the up row tinted.
    let (x, y, bw, bh) = BTN_WARP;
    let (sv, tint, label_color) = if w.locked {
        (0.0, LOCKED[0] as f32 / 255.0, LOCKED)
    } else if over(BTN_WARP) {
        (if w.held { 2.0 * bh } else { bh }, 1.0, LABEL_COLOR)
    } else {
        (0.0, 1.0, LABEL_COLOR)
    };
    out.push(Piece::Image { tex: Tex::Button, x, y, w: bw, h: bh, sv, tint });
    out.push(text(x, y, bw, bh, WARP_TEXT, label_color, false));
    let (x, y, bw, bh) = BTN_EXIT;
    let hover = over(BTN_EXIT);
    out.push(Piece::Image { tex: Tex::Exit, x, y, w: bw, h: bh, sv: 0.0, tint: if hover { EXIT_HOVER } else { 1.0 } });
    if hover {
        out.push(Piece::Tooltip { cx: x + bw / 2.0, bottom: y - 2.0, text: EXIT_TIP.to_string() });
    }
    out
}

/// The textures.
#[derive(Resource)]
struct MarketTextures(std::collections::HashMap<Tex, Handle<Image>>);

#[derive(Component)]
struct MarketRoot;

/// The 640×480 screen the window sits on.
#[derive(Component)]
struct MarketScreen;

fn spawn_root(mut commands: Commands, args: Res<Args>, mut images: ResMut<Assets<Image>>) {
    let textures = Tex::ALL
        .iter()
        .filter_map(|&t| crate::scene::interface_image(&args, &mut images, t.file()).map(|(h, _)| (t, h)))
        .collect();
    commands.insert_resource(MarketTextures(textures));
    // The 640×480 screen centred and bottom-aligned, like the NPC dialogue window.
    commands
        .spawn((
            Name::new("Ventana de Julia"),
            MarketScreen,
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
        ))
        .with_children(|p| {
            p.spawn((MarketRoot, absolute(POS.x, POS.y, W, H), Visibility::Hidden));
        });
}

/// Rebuilds the window when what it draws changes.
fn draw(
    mut commands: Commands,
    w: Res<MarketWindow>,
    args: Res<Args>,
    windows: Query<&Window>,
    textures: Option<Res<MarketTextures>>,
    mut root: Query<(Entity, &mut Visibility), With<MarketRoot>>,
    mut last: Local<(Option<Entity>, Vec<Piece>, f32)>,
) {
    let (Ok((root, mut visibility)), Some(textures)) = (root.single_mut(), textures) else { return };
    let shown = if w.open { Visibility::Inherited } else { Visibility::Hidden };
    if *visibility != shown {
        *visibility = shown;
    }
    if !w.open {
        return;
    }
    let pieces = pieces(&w, args.world);
    let font = font_size(windows.single().map_or(1024, |w| w.physical_width()));
    if last.0 == Some(root) && last.1 == pieces && last.2 == font {
        return;
    }
    commands.entity(root).despawn_children();
    commands.entity(root).with_children(|p| {
        for piece in &pieces {
            match piece {
                Piece::Image { tex, x, y, w, h, sv, tint } => {
                    let Some(handle) = textures.0.get(tex) else { continue };
                    let mut image = ImageNode::new(handle.clone());
                    image.rect = Some(Rect::new(0.0, *sv, *w, sv + h));
                    image.color = Color::srgb(*tint, *tint, *tint);
                    p.spawn((absolute(*x, *y, *w, *h), image));
                }
                Piece::Text { x, y, w, h, text, color, bold } => {
                    let mut node = absolute(*x, *y, *w, if *h > 0.0 { *h } else { font });
                    node.justify_content = JustifyContent::Center;
                    node.align_items = AlignItems::Center;
                    p.spawn(node).with_children(|c| {
                        c.spawn(label(text, *color, *bold, font));
                    });
                }
                Piece::Tooltip { cx, bottom, text } => {
                    // Black box with alpha 180, 6 wider than the text (`CNewUIButton::Render`,
                    // NewUIButton.cpp:439-460), centred over the button.
                    let guess = text.chars().count() as f32 * font * 0.5 + 6.0;
                    p.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(cx - guess / 2.0),
                            top: Val::Px(bottom - font),
                            padding: UiRect::horizontal(Val::Px(3.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba_u8(0, 0, 0, 180)),
                    ))
                    .with_children(|c| {
                        c.spawn(label(text, [255, 255, 255], false, font));
                    });
                }
            }
        }
    });
    *last = (Some(root), pieces, font);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(p: &[Piece]) -> Vec<(f32, String)> {
        p.iter()
            .filter_map(|p| match p {
                Piece::Text { y, text, .. } => Some((*y, text.clone())),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn texts_depend_on_the_map() {
        let w = MarketWindow { open: true, ..Default::default() };
        // Lorencia (world 1): the market's seven lines from y 60, every 20.
        let t = texts(&pieces(&w, 1));
        assert_eq!(t[0], (13.0, "Julia".to_string()));
        assert_eq!(t[1], (60.0, "If you go to the market in Lorencia,".to_string()));
        assert_eq!(t[7], (180.0, "Would you like to go to the market?".to_string()));
        assert_eq!(t[8], (230.0, "Warp".to_string()));
        // Loren Market (`WD_79UNITEDMARKETPLACE`, world 80): three lines, the question on the 7th row.
        let t = texts(&pieces(&w, 80));
        assert_eq!(t[1], (60.0, "Will you be going back to town now?".to_string()));
        assert_eq!(t[3], (100.0, "and stay positive at all times!".to_string()));
        assert_eq!(t[4], (180.0, "Would you like to go to town?".to_string()));
        assert_eq!(t.len(), 6, "title, four lines and the button label");
    }

    #[test]
    fn warp_locks_until_the_window_opens_again() {
        let mut w = MarketWindow::default();
        w.opening(1);
        let on_warp = Vec2::new(70.0, 240.0);
        assert_eq!(w.click(on_warp), Some(Click::Warp));
        assert!(w.locked);
        assert_eq!(w.click(on_warp), None, "locked: no second request");
        // Drawn grey: the up row tinted (100, 100, 100) and a grey label.
        let p = pieces(&w, 1);
        assert!(p.iter().any(|p| matches!(p, Piece::Image { tex: Tex::Button, sv, tint, .. } if *sv == 0.0 && *tint == 100.0 / 255.0)));
        assert!(p.iter().any(|p| matches!(p, Piece::Text { text, color, .. } if text == "Warp" && *color == LOCKED)));
        // `OpeningProcess` unlocks it.
        w.opening(2);
        assert!(!w.locked);
        assert_eq!(w.click(on_warp), Some(Click::Warp));
        // Outside the button, the exit button or the corner: nothing.
        assert_eq!(w.click(Vec2::new(67.0, 240.0)), None);
        assert_eq!(w.click(Vec2::new(100.0, 100.0)), None);
    }

    #[test]
    fn close_buttons() {
        let mut w = MarketWindow::default();
        w.opening(1);
        assert_eq!(w.click(Vec2::new(13.0, 392.0)), Some(Click::Close), "exit button");
        assert_eq!(w.click(Vec2::new(48.0, 420.0)), Some(Click::Close));
        assert_eq!(w.click(Vec2::new(49.0, 420.0)), None);
        assert_eq!(w.click(Vec2::new(163.0, 1.0)), Some(Click::Close), "top-right corner");
        assert_eq!(w.click(Vec2::new(186.0, 22.0)), Some(Click::Close));
        assert_eq!(w.click(Vec2::new(187.0, 22.0)), None);
        assert!(!w.locked, "closing does not lock the Warp button");
    }

    #[test]
    fn button_frames_and_hint() {
        let mut w = MarketWindow { open: true, ..Default::default() };
        let warp = |p: &[Piece]| {
            p.iter()
                .find_map(|p| match p {
                    Piece::Image { tex: Tex::Button, sv, .. } => Some(*sv),
                    _ => None,
                })
                .unwrap()
        };
        assert_eq!(warp(&pieces(&w, 1)), 0.0, "up");
        w.cursor = Some(Vec2::new(80.0, 240.0));
        assert_eq!(warp(&pieces(&w, 1)), 23.0, "over");
        w.held = true;
        assert_eq!(warp(&pieces(&w, 1)), 46.0, "down");
        w.cursor = Some(Vec2::new(20.0, 400.0));
        let p = pieces(&w, 1);
        assert!(p.iter().any(|p| matches!(p, Piece::Image { tex: Tex::Exit, tint, .. } if *tint == EXIT_HOVER)));
        assert!(p.contains(&Piece::Tooltip { cx: 31.0, bottom: 390.0, text: "Close".into() }));
    }

    #[test]
    fn blocks_the_world_only_while_open() {
        let mut w = MarketWindow::default();
        assert!(!w.blocks(Vec2::new(500.0, 100.0)));
        w.opening(1);
        assert!(w.blocks(Vec2::new(500.0, 100.0)));
        assert!(!w.blocks(Vec2::new(449.0, 100.0)));
        assert!(!w.blocks(Vec2::new(500.0, 429.0)));
    }
}
