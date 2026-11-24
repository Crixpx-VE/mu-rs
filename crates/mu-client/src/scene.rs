//! Game screens and map loading.
//!
//! Like the original (`SceneManager.cpp`): title with loading bar (`WebzenScene`) → login
//! (`LOG_IN_SCENE`, map 74) → character selection (`CHARACTER_SCENE`, map 75) → loading
//! (`LoadingScene`) → game (`MAIN_SCENE`).
//!
//! - [`Args::world`] is always the loaded map: [`load_world`] changes it when switching screens.
//! - Each map loads in steps, one per frame ([`LoadStep`]): terrain, grass, objects, critters and,
//!   in game, the character. That way the loading bars follow the real loading.
//! - Everything belonging to a map has [`WorldEntity`]: it is despawned when another loads.
//! - `--world N` goes straight into the game on that map (for testing), without title or login.

use bevy::prelude::*;
use mu_data::{bmd, texture};

use crate::Args;
use crate::terrain::to_bevy_image;

pub struct ScenePlugin;

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<GameScene>()
            .init_resource::<LoadStep>()
            .init_resource::<TitleClock>()
            .add_systems(Startup, start)
            .add_systems(OnEnter(GameScene::Title), spawn_title)
            .add_systems(Update, update_title_gauge.run_if(in_state(GameScene::Title)))
            .add_systems(OnExit(GameScene::Title), despawn::<TitleUi>)
            .add_systems(OnEnter(GameScene::Loading), spawn_loading)
            .add_systems(OnExit(GameScene::Loading), despawn::<LoadingUi>)
            // One load step per frame, at the end, once this frame's step has run.
            // Unloading sits next to advancing: if a load starts mid-frame, its first step
            // (unload) still runs before moving to the next one.
            .add_systems(Last, (unload_world, advance_load).chain());
    }
}

/// Login screen map (`WD_73NEW_LOGIN_SCENE` = 73, folder `World74`).
pub const LOGIN_WORLD: u16 = 74;
/// Character selection map (`WD_74NEW_CHARACTER_SCENE`, folder `World75`).
pub const CHARACTER_WORLD: u16 = 75;

#[derive(States, Default, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum GameScene {
    #[default]
    Title,
    Login,
    CharacterSelect,
    Loading,
    Game,
}

/// Marker for everything that belongs to the loaded map.
#[derive(Component, Default)]
pub struct WorldEntity;

/// Map loading steps, in order. Each loading system runs in its step.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    /// Unload the previous map.
    Unload,
    Terrain,
    Grass,
    Objects,
    Boids,
    Player,
}

impl Step {
    const ALL: [Step; 6] = [Step::Unload, Step::Terrain, Step::Grass, Step::Objects, Step::Boids, Step::Player];
}

/// Load in progress: the map and the current step (`None` = nothing loading).
#[derive(Resource, Default)]
pub struct LoadStep {
    index: Option<usize>,
    /// This load's steps (without the character outside the game).
    steps: Vec<Step>,
    /// Screen to switch to when done.
    then: Option<GameScene>,
    /// In-game map change ([`change_map`]): without the loading screen.
    map_change: bool,
}

impl LoadStep {
    /// Fraction done (0..1).
    pub fn progress(&self) -> f32 {
        match self.index {
            None => 1.0,
            // The current step has already run by the time this frame is drawn.
            Some(i) => ((i + 1) as f32 / self.steps.len().max(1) as f32).min(1.0),
        }
    }

    fn current(&self) -> Option<Step> {
        self.index.and_then(|i| self.steps.get(i).copied())
    }
}

/// Run condition: the current load step is `step`.
pub fn load_step(step: Step) -> impl Fn(Res<LoadStep>) -> bool + Clone {
    move |l: Res<LoadStep>| l.current() == Some(step)
}

/// Starts loading `world` (unloading the previous one) and switches to `then` when done.
pub fn load_world(args: &mut Args, load: &mut LoadStep, world: u16, with_player: bool, then: GameScene) {
    args.world = world;
    load.steps = Step::ALL.iter().copied().filter(|s| with_player || *s != Step::Player).collect();
    load.index = Some(0);
    load.then = Some(then);
    load.map_change = false;
    info!("cargando World{world}");
}

/// In-game map change (`ReceiveTeleport` → `LoadWorld`, WSclient.cpp:1756-1760): the original
/// loads all at once, without `LoadingScene`; here it loads in steps with a black screen.
pub fn change_map(args: &mut Args, load: &mut LoadStep, world: u16) {
    load_world(args, load, world, true, GameScene::Game);
    load.map_change = true;
}

/// Unloads the current map: its entities and what each system's resources hold.
fn unload_world(world: &mut World) {
    if world.resource::<LoadStep>().current() != Some(Step::Unload) {
        return;
    }
    let entities: Vec<Entity> = world.query_filtered::<Entity, With<WorldEntity>>().iter(world).collect();
    for e in entities {
        if let Ok(e) = world.get_entity_mut(e) {
            e.despawn();
        }
    }
    world.remove_resource::<crate::terrain::LoadedTerrain>();
    let map = world.resource::<Args>().world;
    world.insert_resource(ClearColor(crate::world_clear_color(map)));
    // The window title shows the loaded map.
    let mut windows = world.query::<&mut Window>();
    for mut w in windows.iter_mut(world) {
        w.title = match map {
            LOGIN_WORLD | CHARACTER_WORLD => "MU Rust".to_string(),
            _ => format!("MU Rust - World{map}"),
        };
    }
    world.run_system_cached(crate::terrain::reset).ok();
    world.run_system_cached(crate::objects::reset).ok();
    world.run_system_cached(crate::effects::reset).ok();
    world.run_system_cached(crate::fx::reset).ok();
    world.run_system_cached(crate::leaves::reset).ok();
    world.run_system_cached(crate::sound::reset).ok();
    world.run_system_cached(crate::operate::reset).ok();
}

fn advance_load(
    mut load: ResMut<LoadStep>,
    mut next: ResMut<NextState<GameScene>>,
    args: Res<Args>,
    scene: Res<State<GameScene>>,
    clock: Res<TitleClock>,
) {
    let Some(i) = load.index else { return };
    if args.hold_title && *scene.get() == GameScene::Title && i + 1 >= load.steps.len() / 2 {
        return;
    }
    // The title shows for at least `TITLE_SECS`: its bar never runs ahead of elapsed time. While
    // waiting, the index stays past the last step (which is not repeated).
    if *scene.get() == GameScene::Title && i + 1 >= load.steps.len() && clock.elapsed < TITLE_SECS {
        load.index = Some(load.steps.len());
        return;
    }
    if i + 1 < load.steps.len() {
        load.index = Some(i + 1);
    } else {
        load.index = None;
        if let Some(then) = load.then.take() {
            next.set(then);
        }
    }
}

/// At startup: with `--world`, straight into the game; otherwise title, then login.
fn start(mut args: ResMut<Args>, mut load: ResMut<LoadStep>, mut next: ResMut<NextState<GameScene>>) {
    if args.direct && args.world == CHARACTER_WORLD {
        // `--world 75`: straight to character selection.
        load_world(&mut args, &mut load, CHARACTER_WORLD, false, GameScene::CharacterSelect);
        next.set(GameScene::Loading);
    } else if args.direct {
        let world = args.world;
        load_world(&mut args, &mut load, world, true, GameScene::Game);
        next.set(GameScene::Loading);
    } else {
        // The title loads the login map while showing the bar.
        load_world(&mut args, &mut load, LOGIN_WORLD, false, GameScene::Login);
    }
}

/// Despawns the entities with marker `M`.
pub fn despawn<M: Component>(mut commands: Commands, q: Query<Entity, With<M>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

// --- Title (startup loading bar) ------------------------------------------------------------------

/// `TitleSceneUI`: 800×600 virtual screen (the center backgrounds, 1280×1024).
#[derive(Component)]
struct TitleUi;

#[derive(Component)]
struct TitleGauge;

/// Image from `Data/Interface` (`name.jpg` → `name.OZJ`, `.tga` → `.OZT`) and its size.
pub fn interface_image(args: &Args, images: &mut Assets<Image>, name: &str) -> Option<(Handle<Image>, Vec2)> {
    let file = bmd::texture_file_name(name);
    match texture::load(&args.data.join("Interface"), &file) {
        Ok(t) => Some((images.add(to_bevy_image(&t)), Vec2::new(t.width as f32, t.height as f32))),
        Err(e) => {
            warn!("interfaz: falta {file}: {e}");
            None
        }
    }
}

/// `CSprite` of `w`×`h` (the texture's top-left part) at (x, y), in coordinates of a screen
/// `ref_w` wide. The UI uses the 640×480 one (`UiScale`).
fn sprite(image: Handle<Image>, x: f32, y: f32, w: f32, h: f32, ref_w: f32) -> (Node, ImageNode) {
    let k = 640.0 / ref_w;
    let mut img = ImageNode::new(image);
    img.rect = Some(Rect::new(0.0, 0.0, w, h));
    (
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(x * k),
            top: Val::Px(y * k),
            width: Val::Px(w * k),
            height: Val::Px(h * k),
            ..default()
        },
        img,
    )
}

fn spawn_title(mut commands: Commands, args: Res<Args>, mut images: ResMut<Assets<Image>>) {
    let mut load = |n: &str| interface_image(&args, &mut images, n).map(|i| i.0);
    // `SelectBackgroundTheme`: the classic one 71 times in 100 (`rand() % 100 <= 70`).
    let classic = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos()) % 100) <= 70;
    let back = |i: u32| if classic { format!("lo_back_im0{i}.jpg") } else { format!("lo_back_s5_im0{i}.jpg") };

    let mut root = commands.spawn((
        Name::new("Título"),
        TitleUi,
        // Centered 640×480 virtual screen (over the map loading behind it).
        Node {
            position_type: PositionType::Absolute,
            left: Val::Percent(50.0),
            top: Val::Percent(50.0),
            margin: UiRect { left: Val::Px(-320.0), top: Val::Px(-240.0), ..default() },
            width: Val::Px(640.0),
            height: Val::Px(480.0),
            ..default()
        },
        BackgroundColor(Color::BLACK),
        GlobalZIndex(100),
    ));
    let pieces = [
        // (file, x, y, width, height, reference screen width)
        ("New_lo_back_01.jpg".to_string(), 0.0, 0.0, 400.0, 69.0, 800.0),
        ("New_lo_back_02.jpg".to_string(), 400.0, 0.0, 400.0, 69.0, 800.0),
        ("lo_back_s5_03.jpg".to_string(), 0.0, 500.0, 400.0, 100.0, 800.0),
        ("lo_back_s5_04.jpg".to_string(), 400.0, 500.0, 400.0, 100.0, 800.0),
        (back(1), 0.0, 119.0, 512.0, 512.0, 1280.0),
        (back(2), 512.0, 119.0, 512.0, 512.0, 1280.0),
        (back(3), 1024.0, 119.0, 256.0, 512.0, 1280.0),
        (back(4), 0.0, 631.0, 512.0, 223.0, 1280.0),
        (back(5), 512.0, 631.0, 512.0, 223.0, 1280.0),
        (back(6), 1024.0, 631.0, 256.0, 223.0, 1280.0),
    ];
    let gauge = load("lo_lo.jpg");
    root.with_children(|r| {
        for (name, x, y, w, h, ref_w) in pieces {
            if let Some(img) = load(&name) {
                // 1280 wide is stretched to 800 horizontally and 1024 to 600 vertically, the same
                // ratio: uniform scale.
                r.spawn(sprite(img, x, y, w, h, ref_w));
            }
        }
        // Bar: `lo_lo.jpg` (4×15) stretched at (72, 540) up to 656×15 × progress.
        if let Some(img) = gauge {
            let (mut node, mut image) = sprite(img, 72.0, 540.0, 4.0, 15.0, 800.0);
            node.width = Val::Px(0.0);
            image.rect = None;
            r.spawn((node, image, TitleGauge));
        }
    });
}

/// How long the title has been on screen (since its first frame, not since the program started).
#[derive(Resource, Default)]
pub struct TitleClock {
    start: Option<f32>,
    elapsed: f32,
}

fn update_title_gauge(
    load: Res<LoadStep>,
    time: Res<Time<Real>>,
    mut clock: ResMut<TitleClock>,
    mut q: Query<&mut Node, With<TitleGauge>>,
) {
    let now = time.elapsed_secs();
    let start = *clock.start.get_or_insert(now);
    clock.elapsed = now - start;
    let shown = load.progress().min(clock.elapsed / TITLE_SECS).clamp(0.0, 1.0);
    for mut n in &mut q {
        n.width = Val::Px(656.0 * 0.8 * shown);
    }
}

/// Minimum time the title screen is shown.
const TITLE_SECS: f32 = 3.0;

// --- Loading screen ------------------------------------------------------------------------------

#[derive(Component)]
struct LoadingUi;

/// `LoadingScene` (Main 5.2, `ZzzScene.cpp`): when entering the game, `Interface/LSBg01-04.jpg` on
/// the 800×600 screen (two 400×512 on top and two 400×88 below) and the login music stops. From
/// login to character selection the original does not show it: just black.
fn spawn_loading(
    mut commands: Commands,
    args: Res<Args>,
    load: Res<LoadStep>,
    mut images: ResMut<Assets<Image>>,
    mut sound: ResMut<crate::sound::SoundQueue>,
) {
    let mut root = commands.spawn((
        Name::new("Cargando"),
        LoadingUi,
        Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
        BackgroundColor(Color::BLACK),
        GlobalZIndex(100),
    ));
    if load.then != Some(GameScene::Game) || load.map_change {
        return;
    }
    sound.stop_music();
    let pieces = [
        ("LSBg01.jpg", 0.0, 0.0, 512.0),
        ("LSBg02.jpg", 400.0, 0.0, 512.0),
        ("LSBg03.jpg", 0.0, 512.0, 88.0),
        ("LSBg04.jpg", 400.0, 512.0, 88.0),
    ];
    let loaded: Vec<_> = pieces.iter().filter_map(|&(n, x, y, h)| interface_image(&args, &mut images, n).map(|i| (i.0, x, y, h))).collect();
    root.with_children(|r| {
        // Centered 640×480 virtual screen, like the title.
        r.spawn(Node {
            position_type: PositionType::Absolute,
            left: Val::Percent(50.0),
            top: Val::Percent(50.0),
            margin: UiRect { left: Val::Px(-320.0), top: Val::Px(-240.0), ..default() },
            width: Val::Px(640.0),
            height: Val::Px(480.0),
            ..default()
        })
        .with_children(|v| {
            for (img, x, y, h) in loaded {
                v.spawn(sprite(img, x, y, 400.0, h, 800.0));
            }
        });
    });
}
