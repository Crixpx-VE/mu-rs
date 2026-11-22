//! Barra principal de abajo (main frame) de la interfaz Season 6, con las texturas originales
//! de `Data/Interface`.
//!
//! Coordenadas: las del original, en su pantalla virtual de 640×480. Toda la interfaz usa
//! `UiScale = alto de la ventana / 480`, así que 1 px de Bevy UI = 1 unidad del original. La
//! barra (640×51) va abajo y centrada: en pantallas panorámicas no se estira (el original sí).
//!
//! Piezas (posiciones del tema "legacy" de MuMain, que reproduce las del original):
//! - marco: `newui_menu01` (0,429) 256×51, `newui_menu02` (256,429) 128×51,
//!   `partCharge1/newui_menu03` (384,429) 256×51;
//! - esferas: vida (158,432) 45×39, SD (204,431) 16×39, AG (420,431) 16×39, maná (437,432) 45×39.
//!   Se llenan desde abajo recortando la imagen, sin estirarla;
//! - experiencia: `newui_exbar` estirada en (2,473), 629×4 × progreso;
//! - botones 30×41 en y=429: tienda 489, personaje 519, inventario 549, amigos 579, menú 609
//!   (`partCharge1/newui_menu_Bt05/01/02/03/04`, fotogramas normal/encima/abierto cada 41 px);
//!   el de menú abre el menú de ventanas (options.rs);
//! - números: `newui_number1.tga`, dígitos de 12×14 (`RenderNumber`).

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

/// La barra entera (solo existe en el juego).
#[derive(Component)]
pub struct MainFrameRoot;

/// Alto de la pantalla virtual del original.
const VIRTUAL_HEIGHT: f32 = 480.0;
/// Alto de la barra.
const FRAME_HEIGHT: f32 = 51.0;
/// La barra empieza en y = 429 de la pantalla virtual.
const FRAME_TOP: f32 = 480.0 - FRAME_HEIGHT;

/// Lo que muestra la barra. Sin red todavía: valores de un Dark Knight recién creado.
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
    /// Progreso dentro de la décima de nivel actual (0..1) y qué décima (0..9).
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

/// Si el ratón está sobre la interfaz (para que un clic ahí no haga caminar al personaje).
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
    /// (x, y, ancho, alto) en la pantalla virtual.
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
    /// Centro del número (x) y su parte de arriba (y).
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
    /// Posición en la fila (0 = tienda ... 4 = menú).
    index: usize,
    open: bool,
}

/// Imágenes de la interfaz cargadas (para los números, que se crean y destruyen).
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

/// Nodo con una imagen (o parte de ella) en (x, y) de la pantalla virtual, relativo a la barra.
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
    // Botones en el orden de izquierda a derecha: tienda, personaje, inventario, amigos, menú.
    let buttons: Vec<_> = ["newui_menu_Bt05.OZJ", "newui_menu_Bt01.OZJ", "newui_menu_Bt02.OZJ", "newui_menu_Bt03.OZJ", "newui_menu_Bt04.OZJ"]
        .iter()
        .map(|n| load("partCharge1", n))
        .collect();

    // Barra de 640×51 abajo y centrada.
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
        // Primero el marco (JPG opaco, con las esferas vacías dibujadas) y encima el líquido.
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

/// Escala de la interfaz: la pantalla virtual de 640×480 tiene que caber entera, así que manda
/// lo que limite más (el alto en panorámico, el ancho en una ventana estrecha).
fn ui_scale(window: &Window) -> f32 {
    (window.height() / VIRTUAL_HEIGHT).min(window.width() / 640.0).max(0.25)
}

/// 1 px de interfaz = 1 unidad de la pantalla virtual.
fn update_scale(windows: Query<&Window>, mut scale: ResMut<UiScale>) {
    let Ok(w) = windows.single() else { return };
    // Minimizada mide 0: se deja como estaba.
    if w.width() < 1.0 || w.height() < 1.0 {
        return;
    }
    let s = ui_scale(w);
    if (scale.0 - s).abs() > 1e-3 {
        scale.0 = s;
    }
}

/// Esferas: se llenan desde abajo recortando la imagen (el original mueve el borde de arriba).
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

/// Números bajo las esferas con los dígitos de `newui_number1` (`RenderNumber`: centrados,
/// cada dígito avanza 0.8 de su ancho).
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

/// Cursor en coordenadas de la pantalla virtual 640×480 del original (con la barra centrada).
pub(crate) fn cursor_virtual(window: &Window) -> Option<Vec2> {
    Some(to_virtual(window, window.cursor_position()?))
}

/// Un punto de la ventana (píxeles lógicos) en la pantalla virtual 640×480 (con la barra centrada).
pub(crate) fn to_virtual(window: &Window, c: Vec2) -> Vec2 {
    let s = ui_scale(window);
    let x0 = (window.width() - 640.0 * s) / 2.0;
    // La barra va pegada abajo: y virtual = 480 en el borde inferior de la ventana.
    Vec2::new((c.x - x0) / s, VIRTUAL_HEIGHT - (window.height() - c.y) / s)
}

/// Rectángulo (x, y, ancho, alto) del botón `i` en la pantalla virtual.
fn button_rect(i: usize) -> Rect {
    let x = 489.0 + i as f32 * 30.0;
    Rect::new(x, FRAME_TOP, x + 30.0, FRAME_TOP + 41.0)
}

/// Botón de menú (`m_BtnWindow`): el último de la fila.
const MENU_BUTTON: usize = 4;

/// Fotograma del botón: normal, ratón encima o abierto (como en el original, se comprueba
/// a mano si el cursor está dentro).
fn update_buttons(
    windows: Query<&Window>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut game_ui: ResMut<crate::options::GameUi>,
    mut sound: ResMut<crate::sound::SoundQueue>,
    mut q: Query<(&mut HudButton, &mut ImageNode)>,
) {
    // Con una caja de mensajes abierta, la barra no recibe el ratón.
    let cursor = windows.single().ok().and_then(cursor_virtual).filter(|_| !game_ui.modal());
    for (mut b, mut img) in &mut q {
        let inside = cursor.is_some_and(|c| button_rect(b.index).contains(c));
        if b.index == MENU_BUTTON {
            // `m_BtnWindow` (NewUIMainFrameWindow.cpp:817-821): al soltar encima abre o cierra el
            // menú de ventanas. Abierto: 2 normal, 3 encima y 2 apretado; cerrado: 0, 1 y 2
            // (`SetButtonInfo`, NewUIMainFrameWindow.cpp:2700-2717).
            if inside && mouse.just_released(MouseButton::Left) {
                game_ui.toggle_window_menu();
                sound.play("SOUND_CLICK01");
            }
            let open = game_ui.window_menu_open();
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
        if inside && mouse.just_pressed(MouseButton::Left) {
            // Sin ventanas todavía: solo se marca como abierto o cerrado.
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

/// El ratón está sobre la barra o sobre una ventana del juego (un clic ahí no hace caminar).
fn pointer_over_ui(
    windows: Query<&Window>,
    game_ui: Res<crate::options::GameUi>,
    move_window: Res<crate::move_window::MoveWindow>,
    chat: Res<crate::chat::Chat>,
    mut over: ResMut<PointerOverUi>,
) {
    let on_bar = windows.single().ok().and_then(cursor_virtual).is_some_and(|c| {
        (c.y >= FRAME_TOP && (0.0..640.0).contains(&c.x)) || game_ui.blocks(c) || move_window.blocks(c) || chat.blocks(c)
    });
    if over.0 != on_bar {
        over.0 = on_bar;
    }
}