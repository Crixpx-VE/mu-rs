//! Game log in `mu-client.log` (or the name in `MU_LOG`), next to the executable (overwritten
//! on every start).
//!
//! Stores everything printed to the console (warnings, graphics card errors, shaders...),
//! crashes that close the game (with where they happened) and, every 30 s, the fps and the
//! character's position. Lets players send the file when something goes wrong.

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::log::tracing_subscriber::{Layer, fmt};
use bevy::log::{BoxedLayer, LogPlugin};
use bevy::prelude::*;

/// Log path: next to the executable (or in the current folder if unknown). The launcher
/// passes the name in `MU_LOG`, different for each game instance open at once.
pub fn path() -> PathBuf {
    let name = std::env::var("MU_LOG")
        .ok()
        .filter(|n| !n.is_empty() && !n.contains(['/', '\\', ':']))
        .unwrap_or_else(|| "mu-client.log".into());
    std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.join(&name)))
        .unwrap_or_else(|| PathBuf::from(&name))
}

/// Writer shared by the messages and the panic hook.
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

/// `tracing` layer that copies messages to the file (for `LogPlugin::custom_layer`).
fn layer(_app: &mut App) -> Option<BoxedLayer> {
    let f = file()?;
    Some(fmt::layer().with_ansi(false).with_writer(move || f.clone()).boxed())
}

/// `LogPlugin` with the log file and the panic hook.
pub fn plugin() -> LogPlugin {
    install_panic_hook();
    LogPlugin { custom_layer: layer, ..default() }
}

/// Panics are logged with their location and backtrace before exiting.
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

/// Every 30 s: fps (average) and the character's tile.
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
