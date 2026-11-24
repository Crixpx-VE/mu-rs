//! Launcher: a window that updates the client and starts it.
//!
//!   mu-launcher [game options, e.g. --world 3]
//!
//! With game options (the "Jugar - ..." shortcuts) it starts the game as soon as it is up to date;
//! otherwise the player presses PLAY. While the game runs the launcher stays minimized; when the
//! game closes it uploads the game log to the update server and closes too.

// No console window.
#![windows_subsystem = "windows"]

mod game;
mod ui;
mod update;

fn main() {
    let dir = std::env::current_exe().ok().and_then(|e| Some(e.parent()?.to_path_buf())).unwrap_or_default();
    let game_args: Vec<String> = std::env::args().skip(1).collect();
    let _ = std::env::set_current_dir(&dir);
    let _ = std::fs::remove_file(dir.join(update::self_name()).with_extension("exe.old"));
    // Without a console, errors and panics go to `mu-launcher.log`.
    let log = dir.join("mu-launcher.log");
    {
        let log = log.clone();
        std::panic::set_hook(Box::new(move |info| {
            let _ = std::fs::write(&log, format!("el lanzador falló: {info}\n"));
        }));
    }
    let url = update::server_url(&dir);
    if let Err(e) = ui::run(dir, url, game_args) {
        let _ = std::fs::write(&log, format!("no se pudo abrir la ventana del lanzador: {e}\n"));
    }
}
