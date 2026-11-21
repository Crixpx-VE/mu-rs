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
//!
//! Controles: clic izquierdo = caminar · rueda = zoom · Tab = cámara libre/seguir · F3 = fps ·
//! F9 = invasión de dragones (apagada → rojos → dorados).

mod boids;
mod camera;
mod character;
mod effects;
mod grass;
mod hud;
mod invasion;
mod mainframe;
mod objects;
mod terrain;

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
    /// Empezar con la invasión de dragones activa (1 = rojos, 3 = dorados). En el juego: F9.
    pub invasion: u8,
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
        };
        let mut it = std::env::args().skip(1);
        while let Some(a) = it.next() {
            match a.as_str() {
                "--data" => args.data = it.next().expect("--data <ruta>").into(),
                "--world" => args.world = it.next().expect("--world <N>").parse().expect("número"),
                "--spawn" => args.spawn = Some(parse_floats(it.next(), "--spawn")),
                "--walk-to" => args.walk_to = Some(parse_floats(it.next(), "--walk-to")),
                "--screenshot" => args.screenshot = Some(it.next().expect("--screenshot <png>").into()),
                "--screenshot-delay" => {
                    args.screenshot_delay = it.next().expect("--screenshot-delay <s>").parse().expect("número")
                }
                "--cam" => args.cam = Some(parse_floats(it.next(), "--cam")),
                "--zoom" => args.zoom = Some(it.next().expect("--zoom <casillas>").parse().expect("número")),
                "--sin-vsync" => args.no_vsync = true,
                "--invasion" => args.invasion = it.next().and_then(|v| v.parse().ok()).unwrap_or(1),
                otro => eprintln!("argumento desconocido: {otro}"),
            }
        }
        args
    }
}

fn main() {
    let args = Args::parse();
    let title = format!("MU Rust - World{}", args.world);
    let headless = args.screenshot.is_some();

    let mut app = App::new();
    app.insert_resource(ClearColor(Color::srgb(0.45, 0.6, 0.8)));
    if headless {
        // Modo captura: sin ventana. La cámara dibuja en una imagen en memoria
        // (funciona aunque no haya pantalla, p. ej. sesión remota desconectada).
        app.add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: None,
                    exit_condition: bevy::window::ExitCondition::DontExit,
                    ..default()
                })
                .disable::<bevy::winit::WinitPlugin>(),
        )
        .add_plugins(bevy::app::ScheduleRunnerPlugin::run_loop(std::time::Duration::from_secs_f64(1.0 / 30.0)));
    } else {
        app.insert_resource(bevy::winit::WinitSettings::continuous()).add_plugins(DefaultPlugins.set(
            WindowPlugin {
                primary_window: Some(Window {
                    title,
                    resolution: (1280, 720).into(),
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
            boids::BoidsPlugin,
            invasion::InvasionPlugin,
            hud::HudPlugin,
            mainframe::MainFramePlugin,
            objects::ObjectsPlugin,
            character::CharacterPlugin,
            camera::FlyCameraPlugin,
        ))
        .run();
}
