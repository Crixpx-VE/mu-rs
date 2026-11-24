//! Starting the game, and uploading its log to the server when it closes (`POST /log`), so what
//! happened can be seen without the player having to copy it.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

const CLIENT_EXE: &str = "mu-client.exe";
/// What the game prints to the console (the game's own log is `mu-client.log`).
const CONSOLE_LOG: &str = "mu-client-consola.log";

/// The two logs of a session: the game's and its console output.
pub struct LogSlot {
    pub game: String,
    pub console: String,
}

/// With several games open at once, each has its own logs: the first `mu-client.log` and
/// `mu-client-consola.log`, the second `mu-client-2.log` and `mu-client-consola-2.log`... A console
/// log opened by another launcher is in use (opened without letting anyone else write), so the next
/// one is used.
pub fn log_slot(dir: &Path) -> (LogSlot, Option<std::fs::File>) {
    for n in 1..=16 {
        let suffix = if n == 1 { String::new() } else { format!("-{n}") };
        let slot = LogSlot { game: format!("mu-client{suffix}.log"), console: format!("mu-client-consola{suffix}.log") };
        if let Ok(f) = open_exclusive(&dir.join(&slot.console)) {
            return (slot, Some(f));
        }
    }
    (LogSlot { game: "mu-client.log".into(), console: CONSOLE_LOG.into() }, None)
}

#[cfg(windows)]
fn open_exclusive(path: &Path) -> io::Result<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    // FILE_SHARE_READ: others can read it but not open it for writing.
    std::fs::OpenOptions::new().write(true).create(true).truncate(true).share_mode(1).open(path)
}

#[cfg(not(windows))]
fn open_exclusive(path: &Path) -> io::Result<std::fs::File> {
    std::fs::File::create(path)
}

/// Starts the game and waits for it to close. `Ok(true)` if it ended well. The game writes its log
/// to `slot.game` (told via `MU_LOG`); its console output goes to `console`.
pub fn run_client(dir: &Path, args: &[String], slot: &LogSlot, log: Option<std::fs::File>) -> Result<bool, String> {
    let exe: PathBuf = dir.join(CLIENT_EXE);
    let mut cmd = Command::new(&exe);
    cmd.args(args).current_dir(dir).env("MU_LOG", &slot.game);
    if let Some(log) = log
        && let Ok(log2) = log.try_clone()
    {
        cmd.stdout(Stdio::from(log)).stderr(Stdio::from(log2));
    }
    cmd.status().map(|s| s.success()).map_err(|e| format!("no se pudo abrir {}: {e}", exe.display()))
}

/// Uploads the game log and its console output to the update server.
pub fn send_log(dir: &Path, url: &str, slot: &LogSlot) {
    let read = |name: &str| std::fs::read(dir.join(name)).unwrap_or_default();
    let mut body = format!("=== {} ===\n", slot.game).into_bytes();
    body.extend(read(&slot.game));
    let console = read(&slot.console);
    if !console.is_empty() {
        body.extend(b"\n=== consola ===\n");
        body.extend(console);
    }
    let _ = crate::update::agent(Duration::from_secs(20)).post(&format!("{url}/log")).send_bytes(&body);
}
