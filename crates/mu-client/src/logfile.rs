//! Registro del juego en `mu-client.log`, junto al ejecutable (se reescribe en cada arranque).
//!
//! Guarda todo lo que sale por consola (avisos, errores de la tarjeta gráfica, shaders...),
//! los fallos que cierran el juego (con dónde pasaron) y, cada 30 s, los fps y dónde está el
//! personaje. Sirve para que los jugadores manden el archivo cuando algo va mal.

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::log::tracing_subscriber::{Layer, fmt};
use bevy::log::{BoxedLayer, LogPlugin};
use bevy::prelude::*;

/// Ruta del registro: junto al ejecutable (o en la carpeta actual si no se sabe).
pub fn path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.join("mu-client.log")))
        .unwrap_or_else(|| PathBuf::from("mu-client.log"))
}

/// Escritor compartido por los mensajes y el gancho de fallos.
#[derive(Clone)]
struct LogFile(Arc<Mutex<File>>);

impl Write for LogFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut f = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let n = f.write(buf)?;
        f.flush()?;
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).flush()
    }
}

static FILE: std::sync::OnceLock<Option<LogFile>> = std::sync::OnceLock::new();

fn file() -> Option<LogFile> {
    FILE.get_or_init(|| File::create(path()).ok().map(|f| LogFile(Arc::new(Mutex::new(f))))).clone()
}

/// Capa de `tracing` que copia los mensajes al archivo (para `LogPlugin::custom_layer`).
fn layer(_app: &mut App) -> Option<BoxedLayer> {
    let f = file()?;
    Some(fmt::layer().with_ansi(false).with_writer(move || f.clone()).boxed())
}

/// `LogPlugin` con el archivo de registro y el gancho de fallos.
pub fn plugin() -> LogPlugin {
    install_panic_hook();
    LogPlugin { custom_layer: layer, ..default() }
}

/// Los fallos (`panic`) se apuntan con su sitio y la pila antes de cerrar.
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(mut f) = file() {
            let backtrace = std::backtrace::Backtrace::force_capture();
            let _ = writeln!(f, "\n*** EL JUEGO SE HA CERRADO POR UN FALLO ***\n{info}\n{backtrace}");
        }
        previous(info);
    }));
}

pub struct LogFilePlugin;

impl Plugin for LogFilePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<FrameTimeDiagnosticsPlugin>() {
            app.add_plugins(FrameTimeDiagnosticsPlugin::default());
        }
        app.add_systems(Startup, log_start).add_systems(Update, log_status);
    }
}

fn log_start(args: Res<crate::Args>) {
    info!(
        "mu-client {} | World{} | datos {} | argumentos: {:?}",
        env!("CARGO_PKG_VERSION"),
        args.world,
        args.data.display(),
        std::env::args().skip(1).collect::<Vec<_>>()
    );
    info!("registro en {}", path().display());
}

/// Cada 30 s: fps (media) y casilla del personaje.
fn log_status(
    time: Res<Time<Real>>,
    diagnostics: Res<DiagnosticsStore>,
    players: Query<&crate::character::Player>,
    mut next: Local<f32>,
) {
    let now = time.elapsed_secs();
    if now < *next {
        return;
    }
    *next = now + 30.0;
    let fps = diagnostics.get(&FrameTimeDiagnosticsPlugin::FPS).and_then(|d| d.average()).unwrap_or(0.0);
    let pos = players.single().map(|p| format!("({:.1}, {:.1})", p.pos.x, p.pos.y)).unwrap_or_default();
    info!("estado: {fps:.0} fps, personaje en {pos}, {:.0} s de juego", now);
}
