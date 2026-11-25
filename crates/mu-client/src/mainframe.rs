//! Main bottom bar (main frame) of the Season 6 interface, with the original textures
//! from `Data/Interface`.
//!
//! Coordinates: the original's, in its 640×480 virtual screen. The whole interface uses
//! `UiScale = window height / 480`, so 1 Bevy UI px = 1 original unit. The
//! bar (640×51) sits at the bottom, centered: on widescreens it is not stretched (the original does stretch it).
//!
//! Pieces (positions from MuMain's "legacy" theme, which reproduces the original's):
//! - frame: `newui_menu01` (0,429) 256×51, `newui_menu02` (256,429) 128×51,
//!   `partCharge1/newui_menu03` (384,429) 256×51;
//! - orbs: life (158,432) 45×39, SD (204,431) 16×39, AG (420,431) 16×39, mana (437,432) 45×39.
//!   They fill from the bottom by cropping the image, without stretching it;
//! - experience: `newui_exbar` stretched at (2,473), 629×4 × progress;
//! - 30×41 buttons at y=429: shop 489, character 519, inventory 549, friends 579, menu 609
//!   (`partCharge1/newui_menu_Bt05/01/02/03/04`, normal/hover/open frames every 41 px);
//!   menu opens the window menu (options.rs) and inventory opens the inventory (inventory.rs);
//! - numbers: `newui_number1.tga`, 12×14 digits (`RenderNumber`).

use bevy::prelude::*;
use bevy::ui::UiScale;
use mu_data::texture;

use crate::Args;
use crate::terrain::to_bevy_image;

pub struct MainFramePlugin;

impl Plugin for MainFramePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CharacterStats>()
            .init_resource::<PointerOverUi>()
            .add_systems(OnEnter(crate::scene::GameScene::Game), spawn_main_frame)
            .add_systems(OnExit(crate::scene::GameScene::Game), crate::scene::despawn::<MainFrameRoot>)
            .add_systems(Update, (update_scale, update_gauges, update_numbers, update_buttons, pointer_over_ui));
    }
}

/// The whole bar (only exists in game).
#[derive(Component)]
pub struct MainFrameRoot;

/// Height of the original's virtual screen.
const VIRTUAL_HEIGHT: f32 = 480.0;
/// Bar height.
const FRAME_HEIGHT: f32 = 51.0;
/// The bar starts at y = 429 of the virtual screen.
const FRAME_TOP: f32 = 480.0 - FRAME_HEIGHT;

/// What the bar shows. No network yet: values of a freshly created Dark Knight.
#[derive(Resource, Clone, PartialEq)]
pub struct CharacterStats {
    pub hp: u32,
    pub hp_max: u32,
    pub mp: u32,
    pub mp_max: u32,
    pub ag: u32,
    pub ag_max: u32,
    pub sd: u32,
    pub sd_max: u32,
    /// Progress within the current tenth of the level (0..1) and which tenth (0..9).
    pub exp_progress: f32,
    pub exp_tenth: u8,
    pub poisoned: bool,
}

impl Default for CharacterStats {
    fn default() -> Self {
        Self {
            hp: 110,
            hp_max: 110,
            mp: 20,
            mp_max: 20,
            ag: 22,
            ag_max: 22,
            sd: 40,
            sd_max: 40,
            exp_progress: 0.0,
            exp_tenth: 0,
            poisoned: false,
        }
    }
}

/// Whether the mouse is over the interface (so a click there does not make the character walk).
#[derive(Resource, Default)]
pub struct PointerOverUi(pub bool);

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Gauge {
    Hp,
    Mp,
    Ag,
    Sd,
}

impl Gauge {
    /// (x, y, width, height) in the virtual screen.
    fn rect(self) -> (f32, f32, f32, f32) {
        match self {
            Gauge::Hp => (158.0, 432.0, 45.0, 39.0),
            Gauge::Sd => (204.0, 431.0, 16.0, 39.0),
            Gauge::Ag => (420.0, 431.0, 16.0, 39.0),
            Gauge::Mp => (437.0, 432.0, 45.0, 39.0),
        }
    }
    fn fraction(self, s: &CharacterStats) -> f32 {
        let (v, max) = match self {
            Gauge::Hp => (s.hp, s.hp_max),
            Gauge::Mp => (s.mp, s.mp_max),
            Gauge::Ag => (s.ag, s.ag_max),
            Gauge::Sd => (s.sd, s.sd_max),
        };
        if max == 0 { 0.0 } else { (v.min(max) as f32 / max as f32).clamp(0.0, 1.0) }
    }
    /// Number center (x) and its top (y).
    fn number_at(self) -> (f32, f32) {
        match self {
            Gauge::Hp => (180.75, 462.0),
            Gauge::Mp => (465.0, 462.0),
            Gauge::Sd => (217.0, 462.0),
            Gauge::Ag => (427.5, 462.0),
        }
    }
    fn value(self, s: &CharacterStats) -> u32 {
        match self {
            Gauge::Hp => s.hp,
            Gauge::Mp => s.mp,
            Gauge::Ag => s.ag,
            Gauge::Sd => s.sd,
        }
    }
}

#[derive(Component)]
struct GaugeFill(Gauge);

#[derive(Component)]
struct GaugeNumber(Gauge);

#[derive(Component)]
struct ExpFill;

#[derive(Component)]
struct HudButton {
    /// Position in the row (0 = shop ... 4 = menu).
    index: usize,
    open: bool,
}

/// Loaded interface images (for the numbers, which are created and destroyed).
#[derive(Resource)]
struct UiImages {
    hp: Handle<Image>,
    hp_poison: Handle<Image>,
    digits: Option<Handle<Image>>,
}

fn load_image(args: &Args, images: &mut Assets<Image>, sub: &str, name: &str) -> Option<(Handle<Image>, Vec2)> {
    let dir = args.data.join("Interface").join(sub);
    match texture::load(&dir, name) {
        Ok(t) => Some((images.add(to_bevy_image(&t)), Vec2::new(t.width as f32, t.height as f32))),
        Err(e) => {
            warn!("interfaz: falta {sub}/{name}: {e}");
            None
        }
    }
}

/// Node with an image (or part of one) at (x, y) of the virtual screen, relative to the bar.
fn piece(image: Handle<Image>, x: f32, y: f32, w: f32, h: f32, src: Option<Rect>) -> (Node, ImageNode) {
    let mut img = ImageNode::new(image);
    img.rect = src;
    (
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(x),
            top: Val::Px(y - FRAME_TOP),
            width: Val::Px(w),
            height: Val::Px(h),
            ..default()
        },
        img,
    )
}

fn spawn_main_frame(mut commands: Commands, args: Res<Args>, mut images: ResMut<Assets<Image>>) {
    let mut load = |sub: &str, name: &str| load_image(&args, &mut images, sub, name);
    let menu1 = load("", "newui_menu01.OZJ");
    let menu2 = load("", "newui_menu02.OZJ");
    let menu3 = load("partCharge1", "newui_menu03.OZJ").or_else(|| load("", "newui_menu03.OZJ"));
    let red = load("", "newui_menu_red.OZJ");
    let green = load("", "newui_menu_green.OZJ");
    let blue = load("", "newui_menu_blue.OZJ");
    let ag = load("", "newui_menu_AG.OZJ");
    let sd = load("", "newui_menu_SD.OZJ");
    let exbar = load("", "newui_Exbar.OZJ");
    let digits = load("", "newui_number1.OZT");
    // Buttons from left to right: shop, character, inventory, friends, menu.
    let buttons: Vec<_> = ["newui_menu_Bt05.OZJ", "newui_menu_Bt01.OZJ", "newui_menu_Bt02.OZJ", "newui_menu_Bt03.OZJ", "newui_menu_Bt04.OZJ"]
        .iter()
        .map(|n| load("partCharge1", n))
        .collect();

    // 640×51 bar at the bottom, centered.
    let mut bar = commands.spawn((
        Name::new("Barra principal"),
        MainFrameRoot,
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(0.0),
            left: Val::Percent(50.0),
            margin: UiRect::left(Val::Px(-320.0)),
            width: Val::Px(640.0),
            height: Val::Px(FRAME_HEIGHT),
            ..default()
        },
    ));
    bar.with_children(|b| {
        // First the frame (opaque JPG, with the empty orbs drawn) and the liquid on top.
        for (img, x) in [(&menu1, 0.0), (&menu2, 256.0), (&menu3, 384.0)] {
            if let Some((h, size)) = img {
                b.spawn(piece(h.clone(), x, FRAME_TOP, size.x, size.y, None));
            }
        }
        for (gauge, img) in [(Gauge::Hp, &red), (Gauge::Sd, &sd), (Gauge::Ag, &ag), (Gauge::Mp, &blue)] {
            if let Some((h, _)) = img {
                let (x, y, w, hgt) = gauge.rect();
                b.spawn((piece(h.clone(), x, y, w, hgt, Some(Rect::new(0.0, 0.0, w, hgt))), GaugeFill(gauge)));
            }
        }
        if let Some((h, _)) = &exbar {
            b.spawn((piece(h.clone(), 2.0, 473.0, 0.0, 4.0, None), ExpFill));
        }
        for (i, img) in buttons.iter().enumerate() {
            if let Some((h, _)) = img {
                let x = 489.0 + i as f32 * 30.0;
                b.spawn((
                    piece(h.clone(), x, FRAME_TOP, 30.0, 41.0, Some(Rect::new(0.0, 0.0, 30.0, 41.0))),
                    HudButton { index: i, open: false },
                ));
            }
        }
        for gauge in [Gauge::Hp, Gauge::Mp, Gauge::Ag, Gauge::Sd] {
            b.spawn((
                Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), ..default() },
                GaugeNumber(gauge),
            ));
        }
    });

    commands.insert_resource(UiImages {
        hp: red.map(|r| r.0).unwrap_or_default(),
        hp_poison: green.map(|g| g.0).unwrap_or_default(),
        digits: digits.map(|d| d.0),
    });
}

/// Interface scale: the whole 640×480 virtual screen must fit, so the tighter
/// limit wins (height on widescreen, width in a narrow window).
fn ui_scale(window: &Window) -> f32 {
    (window.height() / VIRTUAL_HEIGHT).min(window.width() / 640.0).max(0.25)
}

/// 1 interface px = 1 virtual screen unit.
fn update_scale(windows: Query<&Window>, mut scale: ResMut<UiScale>) {
    let Ok(w) = windows.single() else { return };
    // Minimized it measures 0: leave it as it was.
    if w.width() < 1.0 || w.height() < 1.0 {
        return;
    }
    let s = ui_scale(w);
    if (scale.0 - s).abs() > 1e-3 {
        scale.0 = s;
    }
}

/// Orbs: fill from the bottom by cropping the image (the original moves the top edge).
fn update_gauges(
    stats: Res<CharacterStats>,
    imgs: Option<Res<UiImages>>,
    mut fills: Query<(&GaugeFill, &mut Node, &mut ImageNode)>,
    mut exp: Query<&mut Node, (With<ExpFill>, Without<GaugeFill>)>,
) {
    if !stats.is_changed() {
        return;
    }
    for (GaugeFill(g), mut node, mut img) in &mut fills {
        let (_, y, w, h) = g.rect();
        let f = g.fraction(&stats);
        let empty = h * (1.0 - f);
        node.top = Val::Px(y + empty - FRAME_TOP);
        node.height = Val::Px(h * f);
        img.rect = Some(Rect::new(0.0, empty, w, h));
        if *g == Gauge::Hp {
            if let Some(imgs) = &imgs {
                img.image = if stats.poisoned { imgs.hp_poison.clone() } else { imgs.hp.clone() };
            }
        }
    }
    for mut node in &mut exp {
        node.width = Val::Px(629.0 * stats.exp_progress.clamp(0.0, 1.0));
    }
}

/// Numbers under the orbs with the `newui_number1` digits (`RenderNumber`: centered,
/// each digit advances 0.8 of its width).
fn update_numbers(
    mut commands: Commands,
    stats: Res<CharacterStats>,
    imgs: Option<Res<UiImages>>,
    numbers: Query<(Entity, &GaugeNumber)>,
) {
    let Some(digits) = imgs.as_ref().and_then(|i| i.digits.clone()) else { return };
    if !stats.is_changed() {
        return;
    }
    const GLYPH: Vec2 = Vec2::new(12.0 * 0.7, 14.0 * 0.7);
    for (e, GaugeNumber(g)) in &numbers {
        commands.entity(e).despawn_children();
        let text = g.value(&stats).to_string();
        let advance = GLYPH.x * 0.8;
        let (cx, top) = g.number_at();
        let start = cx - advance * text.len() as f32 / 2.0;
        commands.entity(e).with_children(|p| {
            for (i, ch) in text.bytes().enumerate() {
                let d = (ch - b'0') as f32;
                p.spawn(piece(
                    digits.clone(),
                    start + i as f32 * advance,
                    top,
                    GLYPH.x,
                    GLYPH.y,
                    Some(Rect::new(d * 12.0, 0.0, d * 12.0 + 12.0, 14.0)),
                ));
            }
        });
    }
}

/// Cursor in the original's 640×480 virtual screen coordinates (with the bar centered).
pub(crate) fn cursor_virtual(window: &Window) -> Option<Vec2> {
    Some(to_virtual(window, window.cursor_position()?))
}

/// A window point (logical pixels) in the 640×480 virtual screen (with the bar centered).
pub(crate) fn to_virtual(window: &Window, c: Vec2) -> Vec2 {
    let s = ui_scale(window);
    let x0 = (window.width() - 640.0 * s) / 2.0;
    // The bar sticks to the bottom: virtual y = 480 at the window's bottom edge.
    Vec2::new((c.x - x0) / s, VIRTUAL_HEIGHT - (window.height() - c.y) / s)
}

/// Rectangle (x, y, width, height) of button `i` in the virtual screen.
fn button_rect(i: usize) -> Rect {
    let x = 489.0 + i as f32 * 30.0;
    Rect::new(x, FRAME_TOP, x + 30.0, FRAME_TOP + 41.0)
}

/// Menu button (`m_BtnWindow`): the last one in the row.
const MENU_BUTTON: usize = 4;
/// Character button (`m_BtnChaInfo`): the second.
const CHAR_BUTTON: usize = 1;
/// Inventory button (`m_BtnMyInven`): the third.
const INVENTORY_BUTTON: usize = 2;

/// Button frame: normal, hovered or open (as in the original, whether the cursor is
/// inside is checked by hand).
fn update_buttons(
    windows: Query<&Window>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut game_ui: ResMut<crate::options::GameUi>,
    mut char_info: ResMut<crate::char_info::CharInfoWindow>,
    mut sound: ResMut<crate::sound::SoundQueue>,
    mut inventory: ResMut<crate::inventory::Inventory>,
    mut q: Query<(&mut HudButton, &mut ImageNode)>,
) {
    // With a message box open, the bar ignores the mouse.
    let cursor = windows.single().ok().and_then(cursor_virtual).filter(|_| !game_ui.modal());
    for (mut b, mut img) in &mut q {
        let inside = cursor.is_some_and(|c| button_rect(b.index).contains(c));
        if b.index == MENU_BUTTON || b.index == CHAR_BUTTON {
            // `m_BtnWindow` (NewUIMainFrameWindow.cpp:817-821) and `m_BtnChaInfo` (782-792): releasing
            // over them opens or closes the window menu or the character window. Open: 2
            // normal, 3 hover and 2 pressed; closed: 0, 1 and 2 (`SetBtnState`, 2636-2717).
            if inside && mouse.just_released(MouseButton::Left) {
                if b.index == MENU_BUTTON {
                    game_ui.toggle_window_menu();
                } else {
                    char_info.toggle();
                }
                sound.play("SOUND_CLICK01");
            }
            let open = if b.index == MENU_BUTTON { game_ui.window_menu_open() } else { char_info.is_open() };
            let frame = match (open, inside, mouse.pressed(MouseButton::Left)) {
                (_, true, true) => 2.0,
                (true, true, false) => 3.0,
                (true, false, _) => 2.0,
                (false, true, false) => 1.0,
                (false, false, _) => 0.0,
            };
            let rect = Rect::new(0.0, frame * 41.0, 30.0, frame * 41.0 + 41.0);
            if img.rect != Some(rect) {
                img.rect = Some(rect);
            }
            continue;
        }
        if b.index == INVENTORY_BUTTON {
            // `m_BtnMyInven` (NewUIMainFrameWindow.cpp:769-775): releasing over it opens or closes the
            // inventory (inventory.rs), with `SOUND_CLICK01`; pressed while open.
            if inside && mouse.just_released(MouseButton::Left) {
                inventory.toggle();
                sound.play("SOUND_CLICK01");
            }
            b.open = inventory.open;
        } else if inside && mouse.just_pressed(MouseButton::Left) {
            // No windows yet: only marked as open or closed.
            b.open = !b.open;
        }
        let frame = if b.open || (inside && mouse.pressed(MouseButton::Left)) {
            2.0
        } else if inside {
            1.0
        } else {
            0.0
        };
        let rect = Rect::new(0.0, frame * 41.0, 30.0, frame * 41.0 + 41.0);
        if img.rect != Some(rect) {
            img.rect = Some(rect);
        }
    }
}

/// The mouse is over the bar or over a game window (a click there does not walk).
#[allow(clippy::too_many_arguments)]
fn pointer_over_ui(
    windows: Query<&Window>,
    game_ui: Res<crate::options::GameUi>,
    move_window: Res<crate::move_window::MoveWindow>,
    chat: Res<crate::chat::Chat>,
    char_info: Res<crate::char_info::CharInfoWindow>,
    inventory: Res<crate::inventory::Inventory>,
    (shop, vault): (Res<crate::shop::NpcShop>, Res<crate::vault::Vault>),
    mut over: ResMut<PointerOverUi>,
) {
    let on_bar = windows.single().ok().and_then(cursor_virtual).is_some_and(|c| {
        (c.y >= FRAME_TOP && (0.0..640.0).contains(&c.x))
            || game_ui.blocks(c)
            || move_window.blocks(c)
            || chat.blocks(c)
            || char_info.blocks(c)
            || inventory.blocks(c)
            || shop.blocks(c)
            || vault.blocks(c)
    });
    if over.0 != on_bar {
        over.0 = on_bar;
    }
}