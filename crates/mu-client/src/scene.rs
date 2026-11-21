//! Pantallas del juego y carga de mapas.
//!
//! Como el original (`SceneManager.cpp`): título con barra de carga (`WebzenScene`) → login
//! (`LOG_IN_SCENE`, mapa 74) → selección de personaje (`CHARACTER_SCENE`, mapa 75) → carga
//! (`LoadingScene`) → juego (`MAIN_SCENE`).
//!
//! - [`Args::world`] es siempre el mapa cargado: lo cambia [`load_world`] al pasar de pantalla.
//! - Cada mapa se carga por pasos, uno por fotograma ([`LoadStep`]): terreno, hierba, objetos,
//!   bichos y, en el juego, el personaje. Así las barras de carga avanzan con la carga real.
//! - Todo lo de un mapa lleva [`WorldEntity`]: se borra al cargar otro.
//! - Con `--world N` se entra directo al juego en ese mapa (para probar), sin título ni login.

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
            // Un paso de carga por fotograma, al final, cuando ya se ha hecho el de este.
            // El borrado va en el mismo sitio que el avance: si una carga empieza a mitad de fotograma,
            // su primer paso (borrar) se hace igualmente antes de pasar al siguiente.
            .add_systems(Last, (unload_world, advance_load).chain());
    }
}

/// Mapa de la pantalla de login (`WD_73NEW_LOGIN_SCENE` = 73, carpeta `World74`).
pub const LOGIN_WORLD: u16 = 74;
/// Mapa de la selección de personaje (`WD_74NEW_CHARACTER_SCENE`, carpeta `World75`).
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

/// Marca de todo lo que pertenece al mapa cargado.
#[derive(Component, Default)]
pub struct WorldEntity;

/// Pasos de la carga del mapa, en orden. Cada sistema de carga corre en su paso.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    /// Borrar el mapa anterior.
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

/// Carga en curso: el mapa y por qué paso va (`None` = no se está cargando nada).
#[derive(Resource, Default)]
pub struct LoadStep {
    index: Option<usize>,
    /// Pasos de esta carga (sin el personaje fuera del juego).
    steps: Vec<Step>,
    /// A qué pantalla se pasa al terminar.
    then: Option<GameScene>,
    /// Cambio de mapa dentro del juego ([`change_map`]): sin la pantalla de carga.
    map_change: bool,
}

impl LoadStep {
    /// Fracción hecha (0..1).
    pub fn progress(&self) -> f32 {
        match self.index {
            None => 1.0,
            // El paso actual ya se ha hecho cuando se dibuja este fotograma.
            Some(i) => ((i + 1) as f32 / self.steps.len().max(1) as f32).min(1.0),
        }
    }

    fn current(&self) -> Option<Step> {
        self.index.and_then(|i| self.steps.get(i).copied())
    }
}

/// Condición: el paso de carga actual es `step`.
pub fn load_step(step: Step) -> impl Fn(Res<LoadStep>) -> bool + Clone {
    move |l: Res<LoadStep>| l.current() == Some(step)
}

/// Empieza a cargar `world` (borrando el anterior) y pasa a `then` al terminar.
pub fn load_world(args: &mut Args, load: &mut LoadStep, world: u16, with_player: bool, then: GameScene) {
    args.world = world;
    load.steps = Step::ALL.iter().copied().filter(|s| with_player || *s != Step::Player).collect();
    load.index = Some(0);
    load.then = Some(then);
    load.map_change = false;
    info!("cargando World{world}");
}

/// Cambio de mapa dentro del juego (`ReceiveTeleport` → `LoadWorld`, WSclient.cpp:1756-1760): el
/// original carga de golpe, sin la `LoadingScene`; aquí se carga por pasos con la pantalla en negro.
pub fn change_map(args: &mut Args, load: &mut LoadStep, world: u16) {
    load_world(args, load, world, true, GameScene::Game);
    load.map_change = true;
}

/// Borra el mapa cargado: sus entidades y lo que guardan los recursos de cada sistema.
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
    // El título de la ventana dice el mapa cargado.
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
    // El título se ve al menos `TITLE_SECS`: su barra no pasa de lo que lleva de tiempo. Mientras
    // espera, el índice queda pasado el último paso (que no se repite).
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

/// Al arrancar: con `--world` directo al juego; si no, título y luego login.
fn start(mut args: ResMut<Args>, mut load: ResMut<LoadStep>, mut next: ResMut<NextState<GameScene>>) {
    if args.direct && args.world == CHARACTER_WORLD {
        // `--world 75`: directo a la selección de personaje.
        load_world(&mut args, &mut load, CHARACTER_WORLD, false, GameScene::CharacterSelect);
        next.set(GameScene::Loading);
    } else if args.direct {
        let world = args.world;
        load_world(&mut args, &mut load, world, true, GameScene::Game);
        next.set(GameScene::Loading);
    } else {
        // El título carga el mapa del login mientras enseña la barra.
        load_world(&mut args, &mut load, LOGIN_WORLD, false, GameScene::Login);
    }
}

/// Borra las entidades con la marca `M`.
pub fn despawn<M: Component>(mut commands: Commands, q: Query<Entity, With<M>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

// --- Título (barra de carga del arranque) ---------------------------------------------------------

/// `TitleSceneUI`: pantalla virtual de 800×600 (los fondos del centro, de 1280×1024).
#[derive(Component)]
struct TitleUi;

#[derive(Component)]
struct TitleGauge;

/// Imagen de `Data/Interface` (`nombre.jpg` → `nombre.OZJ`, `.tga` → `.OZT`) y su tamaño.
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

/// `CSprite` de `w`×`h` (la parte de arriba a la izquierda de la textura) en (x, y), con
/// coordenadas de una pantalla de `ref_w` de ancho. La interfaz usa la de 640×480 (`UiScale`).
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
    // `SelectBackgroundTheme`: el clásico 71 de cada 100 veces (`rand() % 100 <= 70`).
    let classic = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos()) % 100) <= 70;
    let back = |i: u32| if classic { format!("lo_back_im0{i}.jpg") } else { format!("lo_back_s5_im0{i}.jpg") };

    let mut root = commands.spawn((
        Name::new("Título"),
        TitleUi,
        // Pantalla virtual de 640×480 centrada (encima del mapa que se carga detrás).
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
        // (archivo, x, y, ancho, alto, pantalla de referencia)
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
                // El ancho de 1280 se estira a 800 en horizontal y 1024 a 600 en vertical, que es
                // la misma proporción: escala uniforme.
                r.spawn(sprite(img, x, y, w, h, ref_w));
            }
        }
        // Barra: `lo_lo.jpg` (4×15) estirada en (72, 540) hasta 656×15 × progreso.
        if let Some(img) = gauge {
            let (mut node, mut image) = sprite(img, 72.0, 540.0, 4.0, 15.0, 800.0);
            node.width = Val::Px(0.0);
            image.rect = None;
            r.spawn((node, image, TitleGauge));
        }
    });
}

/// Tiempo que lleva en pantalla el título (desde su primer fotograma, no desde que se abrió el programa).
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

/// Lo mínimo que se ve la pantalla del título.
const TITLE_SECS: f32 = 3.0;

// --- Carga (provisional: pantalla negra) ---------------------------------------------------------

#[derive(Component)]
struct LoadingUi;

/// `LoadingScene` (Main 5.2, `ZzzScene.cpp`): al entrar al juego, `Interface/LSBg01-04.jpg` en la
/// pantalla de 800×600 (dos de 400×512 arriba y dos de 400×88 abajo) y se corta la música del
/// login. Del login a la selección de personaje el original no la enseña: solo negro.
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
        // Pantalla virtual de 640×480 centrada, como el título.
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
