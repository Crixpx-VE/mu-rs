//! Cliente de MU Online en Rust.
//!
//! Fase 1: visor de mapas. Carga el terreno de un `WorldN` y deja volar por él.
//!
//! Uso:
//!   cargo run -p mu-client -- [--data <ruta Data>] [--world <N>] [--screenshot <archivo.png>]
//!                             [--cam x,y,alto,mirax,miray,miraalto]   (en casillas de MU)

mod camera;
mod objects;
mod terrain;

use std::path::PathBuf;

use bevy::prelude::*;

/// Opciones de línea de comandos.
#[derive(Resource, Clone)]
pub struct Args {
    pub data: PathBuf,
    pub world: u16,
    /// Si se indica, hace una captura al arrancar y se cierra (útil para probar sin mirar).
    pub screenshot: Option<PathBuf>,
    /// Posición inicial de la cámara: ojo y punto al que mira, en casillas de MU (x, y, altura).
    pub cam: Option<[f32; 6]>,
}

impl Args {
    fn parse() -> Self {
        let mut args = Args {
            data: std::env::var_os("MU_DATA")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("../referencias/MuMain/src/bin/Data")),
            world: 1,
            screenshot: None,
            cam: None,
        };
        let mut it = std::env::args().skip(1);
        while let Some(a) = it.next() {
            match a.as_str() {
                "--data" => args.data = it.next().expect("--data <ruta>").into(),
                "--world" => args.world = it.next().expect("--world <N>").parse().expect("número"),
                "--screenshot" => args.screenshot = Some(it.next().expect("--screenshot <png>").into()),
                "--cam" => {
                    let v: Vec<f32> = it
                        .next()
                        .expect("--cam x,y,alto,mirax,miray,miraalto")
                        .split(',')
                        .map(|n| n.trim().parse().expect("número"))
                        .collect();
                    args.cam = Some(v.try_into().expect("--cam necesita 6 números"));
                }
                otro => eprintln!("argumento desconocido: {otro}"),
            }
        }
        args
    }
}

fn main() {
    let args = Args::parse();
    let title = format!("MU Rust - World{}", args.world);

    App::new()
        .insert_resource(args)
        .insert_resource(ClearColor(Color::srgb(0.45, 0.6, 0.8)))
        // Seguir dibujando aunque la ventana no tenga el foco.
        .insert_resource(bevy::winit::WinitSettings::continuous())
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title, ..default() }),
            ..default()
        }))
        .add_plugins((terrain::TerrainPlugin, objects::ObjectsPlugin, camera::FlyCameraPlugin))
        .run();
}
