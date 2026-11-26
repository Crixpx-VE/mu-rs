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
        app.add_systems(Startup, log_start).add_systems(Update, log_status).add_systems(Last, log_hitches);
    }
}

/// A frame this long (seconds) in game is logged: a hitch the player feels.
const HITCH_SECS: f32 = 0.08;

/// Each slow frame in game, with what grew in it (new meshes and images mean something was
/// loaded; new entities, that something appeared), at most once a second.
fn log_hitches(
    time: Res<Time<Real>>,
    scene: Res<State<crate::scene::GameScene>>,
    entities: &bevy::ecs::entity::Entities,
    meshes: Res<Assets<Mesh>>,
    images: Res<Assets<Image>>,
    // Counts at the end of the last frame, what grew during it, and when a hitch was last logged.
    mut last: Local<([i64; 3], [i64; 3], f32)>,
) {
    let counts = [entities.len() as i64, meshes.len() as i64, images.len() as i64];
    let (before, grew_last_frame, logged) = *last;
    *last = (counts, [0, 1, 2].map(|i| counts[i] - before[i]), logged);
    // `delta` is measured when this frame starts: it is the length of the previous frame, the
    // one `grew_last_frame` describes.
    let now = time.elapsed_secs();
    if *scene.get() != crate::scene::GameScene::Game || time.delta_secs() < HITCH_SECS || now - logged < 1.0 {
        return;
    }
    last.2 = now;
    let [e, m, i] = grew_last_frame;
    info!(
        "fotograma lento: {:.0} ms | entidades {} ({e:+}), mallas {} ({m:+}), imágenes {} ({i:+})",
        time.delta_secs() * 1000.0,
        before[0],
        before[1],
        before[2]
    );
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

/// Every 30 s: fps (average), the character's tile, and what the frame cost depends on (window
/// size, entities, meshes drawn), so slow frames in a player's log can be explained.
#[allow(clippy::too_many_arguments)]
fn log_status(
    time: Res<Time<Real>>,
    diagnostics: Res<DiagnosticsStore>,
    players: Query<&crate::character::Player>,
    windows: Query<&Window>,
    entities: &bevy::ecs::entity::Entities,
    meshes: Query<&ViewVisibility, With<Mesh3d>>,
    mut next: Local<f32>,
) {
    let now = time.elapsed_secs();
    if now < *next {
        return;
    }
    *next = now + 30.0;
    let fps = diagnostics.get(&FrameTimeDiagnosticsPlugin::FPS).and_then(|d| d.average()).unwrap_or(0.0);
    let pos = players.single().map(|p| format!("({:.1}, {:.1})", p.pos.x, p.pos.y)).unwrap_or_default();
    let window = windows
        .single()
        .map(|w| format!("{}x{} {:?}", w.resolution.physical_width(), w.resolution.physical_height(), w.present_mode))
        .unwrap_or_default();
    let visible = meshes.iter().filter(|v| v.get()).count();
    info!(
        "estado: {fps:.0} fps, personaje en {pos}, {:.0} s de juego | ventana {window}, {} entidades, {visible}/{} mallas visibles",
        now,
        entities.len(),
        meshes.iter().len()
    );
}
