//! Cliente de MU Online en Rust.
//!
//! Por ahora, sin red: carga un mapa con sus objetos y un personaje que camina con clic.
//!
//! Uso:
//!   cargo run -p mu-client -- [opciones]
//!
//!   --data <ruta>            carpeta Data del cliente (o variable MU_DATA)
//!   --world <N>              mapa a cargar (1 = Lorencia, 2 = Dungeon, 3 = Devias, 4 = Noria...)
//!   --spawn x,y              casilla donde aparece el personaje
//!   --walk-to x,y            orden de caminar al arrancar (para probar sin ratón);
//!                            con --screenshot se da 1 s antes de capturar
//!   --cam x,y,alto,mx,my,ma  cámara libre en esa posición, mirando a ese punto (en casillas)
//!   --screenshot <png>       hace una captura y se cierra
//!   --screenshot-delay <s>   segundos de espera antes de capturar (por defecto 10)
//!   --zoom <casillas>        distancia inicial de la cámara de seguimiento (10-17, por defecto 13)
//!   --sin-vsync              sin límite de fps (para medir rendimiento)
//!   --invasion <1|3>         empezar con la invasión de dragones (1 = rojos, 3 = dorados)
//!   --resolucion <AxB>       tamaño de la ventana solo esta vez (si no, `resolucion=` de
//!                            mu-launcher.txt, o 1024x768)
//!   --fps                    contador de fotogramas
//!
//! Controles, los del original: clic izquierdo = caminar (mantenido, sigue al ratón) · 1-0 = elegir
//! habilidad · clic derecho = usarla · Esc = menú de sistema · U = menú de ventanas.

mod account;
mod boids;
mod camera;
mod character;
mod charselect;
mod chat;
mod chat_log;
mod cursor;
mod effects;
mod fish;
mod fonts;
mod fx;
mod grass;
mod hud;
mod invasion;
mod leaves;
mod logfile;
mod login;
mod login_ui;
mod mainframe;
mod minimap;
mod net;
mod model_light;
mod move_window;
mod names;
mod objects;
mod operate;
mod options;
mod overlay;
mod remote;
mod settings;
mod shadow;
mod scene;
mod sound;
mod terrain;
mod water_terrain;

use std::path::PathBuf;

use bevy::prelude::*;

/// Opciones de línea de comandos.
#[derive(Resource, Clone)]
pub struct Args {
    pub data: PathBuf,
    pub world: u16,
    pub spawn: Option<[f32; 2]>,
    pub walk_to: Option<[f32; 2]>,
    /// Si se indica, hace una captura al arrancar y se cierra (útil para probar sin mirar).
    pub screenshot: Option<PathBuf>,
    /// Segundos de espera antes de capturar.
    pub screenshot_delay: f32,
    /// Posición inicial de la cámara: ojo y punto al que mira, en casillas de MU (x, y, altura).
    pub cam: Option<[f32; 6]>,
    /// Distancia inicial de la cámara de seguimiento (casillas).
    pub zoom: Option<f32>,
    /// Sin sincronía vertical: los fps no se limitan a la frecuencia del monitor (para medir).
    pub no_vsync: bool,
    /// Empezar con la invasión de dragones activa (1 = rojos, 3 = dorados).
    pub invasion: u8,
    /// Resolución de la ventana solo para esta vez (si no, la guardada o 1024×768).
    pub resolution: Option<(u32, u32)>,
    /// Lanza todas las habilidades una tras otra (para probar sin teclado).
    pub skill_test: bool,
    /// `--world`: entrar directo al juego en ese mapa, sin título ni login.
    pub direct: bool,
    /// `--ver-titulo`: la pantalla del título se queda a medias (para comprobarla en una captura).
    pub hold_title: bool,
    /// `--fps`: contador de fotogramas arriba a la izquierda.
    pub show_fps: bool,
    /// `--salir-tras <s>`: se cierra solo a los `s` segundos (para medir: la traza se escribe entera).
    pub quit_after: Option<f32>,
}

fn parse_floats<const N: usize>(s: Option<String>, what: &str) -> [f32; N] {
    let v: Vec<f32> = s
        .unwrap_or_else(|| panic!("falta el valor de {what}"))
        .split(',')
        .map(|n| n.trim().parse().unwrap_or_else(|_| panic!("{what}: número inválido")))
        .collect();
    v.try_into().unwrap_or_else(|_| panic!("{what} necesita {N} números"))
}

/// `Data` junto al ejecutable (versión empaquetada), luego `./Data`, y si no, la de las referencias.
fn default_data_dir() -> PathBuf {
    let next_to_exe = std::env::current_exe().ok().and_then(|exe| Some(exe.parent()?.join("Data")));
    [next_to_exe, Some(PathBuf::from("Data"))]
        .into_iter()
        .flatten()
        .find(|p| p.is_dir())
        .unwrap_or_else(|| PathBuf::from("../referencias/MuMain/src/bin/Data"))
}

impl Args {
    fn parse() -> Self {
        let mut args = Args {
            data: std::env::var_os("MU_DATA").map(PathBuf::from).unwrap_or_else(default_data_dir),
            world: 1,
            spawn: None,
            walk_to: None,
            screenshot: None,
            screenshot_delay: 10.0,
            cam: None,
            zoom: None,
            no_vsync: false,
            invasion: 0,
            resolution: None,
            skill_test: false,
            direct: false,
            hold_title: false,
            show_fps: false,
            quit_after: None,
        };
        let mut it = std::env::args().skip(1);
        while let Some(a) = it.next() {
            match a.as_str() {
                "--data" => args.data = it.next().expect("--data <ruta>").into(),
                "--world" => {
                    args.world = it.next().expect("--world <N>").parse().expect("número");
                    args.direct = true;
                }
                "--spawn" => args.spawn = Some(parse_floats(it.next(), "--spawn")),
                "--walk-to" => args.walk_to = Some(parse_floats(it.next(), "--walk-to")),
                "--screenshot" => args.screenshot = Some(it.next().expect("--screenshot <png>").into()),
                "--screenshot-delay" => {
                    args.screenshot_delay = it.next().expect("--screenshot-delay <s>").parse().expect("número")
                }
                "--cam" => args.cam = Some(parse_floats(it.next(), "--cam")),
                "--zoom" => args.zoom = Some(it.next().expect("--zoom <casillas>").parse().expect("número")),
                "--sin-vsync" => args.no_vsync = true,
                "--probar-habilidades" => args.skill_test = true,
                "--ver-titulo" => args.hold_title = true,
                "--fps" => args.show_fps = true,
                "--salir-tras" => args.quit_after = it.next().and_then(|v| v.parse().ok()),
                "--resolucion" => {
                    args.resolution = it.next().as_deref().and_then(settings::parse_resolution);
                    if args.resolution.is_none() {
                        eprintln!("--resolucion: usa algo como 1024x768");
                    }
                }
                "--invasion" => args.invasion = it.next().and_then(|v| v.parse().ok()).unwrap_or(1),
                otro => eprintln!("argumento desconocido: {otro}"),
            }
        }
        args
    }
}

/// Color de fondo de cada mapa: bytes / 256. Es el que hay justo antes del `glClear` de cada fotograma
/// (`RenderScene`, ZzzScene.cpp:2347-2396 del Main 5.2). El verde de Lorencia y el azul de Devias de
/// `RenderMainScene` (2059-2066) se ponen después del `glClear` y el siguiente fotograma los vuelve a
/// pisar antes de borrar, así que no se ven nunca: esos mapas tienen fondo negro. (MuMain, con su
/// `SetWorldClearColor`, los juntó en uno y sí los pinta, pero el cliente S6 no.)
pub fn world_clear_color(world: u16) -> Color {
    let rgb8 = |r: u8, g: u8, b: u8| Color::srgb(r as f32 / 256.0, g as f32 / 256.0, b as f32 / 256.0);
    // `WorldActive` del original: nuestro número de mapa menos uno.
    match world as i32 - 1 {
        10 => rgb8(3, 25, 44),                // Heaven: azul
        24..=29 | 36 => rgb8(30, 40, 40),     // Kalima: turquesa oscuro
        45..=50 => rgb8(9, 8, 33),            // Cursed Temple: morado oscuro
        51 => rgb8(178, 178, 178),            // Elbeland: gris
        65 => rgb8(148, 179, 223),            // Doppelganger: azul claro
        _ => Color::BLACK,                    // el resto, Lorencia y Devias incluidos: negro
    }
}

fn main() {
    let args = Args::parse();
    let title = format!("MU Rust - World{}", args.world);
    let resolution = args.resolution.or_else(settings::saved_resolution).unwrap_or(settings::DEFAULT_RESOLUTION);
    let headless = args.screenshot.is_some();

    let mut app = App::new();
    app.insert_resource(ClearColor(world_clear_color(args.world)));
    if headless {
        // Modo captura: sin ventana. La cámara dibuja en una imagen en memoria
        // (funciona aunque no haya pantalla, p. ej. sesión remota desconectada).
        app.add_plugins(
            DefaultPlugins
                .set(logfile::plugin())
                .set(WindowPlugin {
                    primary_window: None,
                    exit_condition: bevy::window::ExitCondition::DontExit,
                    ..default()
                })
                .disable::<bevy::winit::WinitPlugin>(),
        )
        .add_plugins(bevy::app::ScheduleRunnerPlugin::run_loop(std::time::Duration::from_secs_f64(1.0 / 30.0)));
    } else {
        app.insert_resource(bevy::winit::WinitSettings::continuous()).add_plugins(DefaultPlugins.set(logfile::plugin()).set(
            WindowPlugin {
                primary_window: Some(Window {
                    title,
                    // Tamaño fijo, como el original: la resolución es de la configuración, no se estira.
                    resolution: settings::window_resolution(resolution),
                    resizable: false,
                    enabled_buttons: bevy::window::EnabledButtons { maximize: false, ..default() },
                    present_mode: if args.no_vsync {
                        bevy::window::PresentMode::AutoNoVsync
                    } else {
                        bevy::window::PresentMode::AutoVsync
                    },
                    ..default()
                }),
                ..default()
            },
        ));
    }

    app.insert_resource(args)
        .add_plugins((
            terrain::TerrainPlugin,
            grass::GrassPlugin,
            effects::EffectsPlugin,
            fx::FxPlugin,
            boids::BoidsPlugin,
            invasion::InvasionPlugin,
            leaves::LeavesPlugin,
            hud::HudPlugin,
            mainframe::MainFramePlugin,
            settings::SettingsPlugin,
            objects::ObjectsPlugin,
            character::CharacterPlugin,
            camera::FlyCameraPlugin,
        ))
        .add_plugins(fonts::FontsPlugin)
        .add_systems(Update, |args: Res<Args>, time: Res<Time<Real>>, mut exit: MessageWriter<AppExit>| {
            if args.quit_after.is_some_and(|s| time.elapsed_secs() >= s) {
                exit.write(AppExit::Success);
            }
        })
        .add_plugins(overlay::OverlayPlugin)
        .add_plugins((model_light::ModelLightPlugin, logfile::LogFilePlugin, shadow::ShadowPlugin, sound::SoundPlugin, scene::ScenePlugin, login::LoginPlugin, charselect::CharSelectPlugin, login_ui::LoginUiPlugin, options::OptionsPlugin, cursor::CursorPlugin, operate::OperatePlugin))
        .add_plugins((net::NetPlugin, remote::RemotePlugin, fish::FishPlugin, water_terrain::WaterTerrainPlugin))
        .add_plugins(minimap::MiniMapPlugin)
        .add_plugins(move_window::MoveWindowPlugin)
        .add_plugins(names::NamesPlugin)
        .add_plugins(chat::ChatPlugin)
        .run();
}
