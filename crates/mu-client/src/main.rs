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
//!   --zoom <casillas>        distancia inicial de la cámara de seguimiento (por defecto 12)
//!
//! Controles: clic izquierdo = caminar · rueda = zoom · Tab = cámara libre/seguir.

mod camera;
mod character;
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
}

fn parse_floats<const N: usize>(s: Option<String>, what: &str) -> [f32; N] {
    let v: Vec<f32> = s
        .unwrap_or_else(|| panic!("falta el valor de {what}"))
        .split(',')
        .map(|n| n.trim().parse().unwrap_or_else(|_| panic!("{what}: número inválido")))
        .collect();
    v.try_into().unwrap_or_else(|_| panic!("{what} necesita {N} números"))
}

impl Args {
    fn parse() -> Self {
        let mut args = Args {
            data: std::env::var_os("MU_DATA")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("../referencias/MuMain/src/bin/Data")),
            world: 1,
            spawn: None,
            walk_to: None,
            screenshot: None,
            screenshot_delay: 10.0,
            cam: None,
            zoom: None,
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
                primary_window: Some(Window { title, resolution: (1280, 720).into(), ..default() }),
                ..default()
            },
        ));
    }

    app.insert_resource(args)
        .add_plugins((
            terrain::TerrainPlugin,
            objects::ObjectsPlugin,
            character::CharacterPlugin,
            camera::FlyCameraPlugin,
        ))
        .run();
}
