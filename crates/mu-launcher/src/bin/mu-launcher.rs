//! Launcher: updates the client and starts it.
//!
//!   mu-launcher [game options, e.g. --world 3]
//!
//! 1. Requests the file list from the server (address in `mu-launcher.txt`: `servidor=http://ip:port`).
//! 2. Downloads whatever is missing or changed (compressed, several at once), checking each
//!    file's SHA-256 hash. If something fails, retries until it has everything.
//! 3. Starts `mu-client.exe` with the received options.
//! 4. When the game closes, uploads its log (`mu-client.log`) to the server (`POST /log`).
//!
//! The game only starts with all files present. If the server doesn't respond, the existing version
//! is started only if the last update completed fully. If it updates itself, it
//! restarts with the new version (before downloading the rest).

use std::collections::HashMap;
use std::io::{self, BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant, UNIX_EPOCH};

use mu_launcher::{
    DEFAULT_PORT, Entry, encode_path, excluded, hash_file, hex, is_safe_path, parse_manifest, parse_manifest2,
    to_local,
};
use sha2::{Digest, Sha256};

const CONFIG_FILE: &str = "mu-launcher.txt";
const STATE_FILE: &str = ".mu-launcher-state";
/// State line saying the last update completed fully.
const COMPLETE_MARK: &str = "#completo";
const CLIENT_EXE: &str = "mu-client.exe";
/// What the game prints to the console (the game's own log is `mu-client.log`).
const CONSOLE_LOG: &str = "mu-client-consola.log";
/// Concurrent downloads.
const WORKERS: usize = 6;
/// Attempts per file in each round (then the list is requested again).
const ATTEMPTS: u32 = 4;
/// Wait before retrying the whole update.
const RETRY_WAIT: Duration = Duration::from_secs(5);

type Res<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn main() {
    let dir = std::env::current_exe().ok().and_then(|e| Some(e.parent()?.to_path_buf())).unwrap_or_default();
    let game_args: Vec<String> = std::env::args().skip(1).collect();
    let _ = std::env::set_current_dir(&dir);
    let _ = std::fs::remove_file(dir.join(self_name()).with_extension("exe.old"));

    println!("MU Rust - lanzador {}", env!("CARGO_PKG_VERSION"));
    let url = server_url(&dir);
    match url.clone() {
        Some(url) => loop {
            match update(&dir, &url) {
                Ok(Updated { files: 0, .. }) => {
                    println!("El cliente está al día.");
                    break;
                }
                Ok(Updated { files, relaunch: false }) => {
                    println!("Actualizados {files} archivos. El cliente está completo.");
                    break;
                }
                Ok(Updated { relaunch: true, .. }) => {
                    println!("El lanzador se actualizó; abriendo la versión nueva...");
                    let code = Command::new(dir.join(self_name()))
                        .args(&game_args)
                        .status()
                        .map_or(1, |s| s.code().unwrap_or(1));
                    std::process::exit(code);
                }
                Err(Failure::Offline(e)) if load_complete(&dir) => {
                    println!("\nNo se pudo conectar con el servidor de actualizaciones ({e}).");
                    println!("Se abre la versión que tienes (la última actualización terminó entera).");
                    break;
                }
                Err(Failure::Offline(e) | Failure::Download(e)) => {
                    println!("\nNo se pudo terminar la actualización: {e}");
                    println!(
                        "El juego no se abre hasta tener todos los archivos. Se reintenta en {} s (cierra la ventana para salir).",
                        RETRY_WAIT.as_secs()
                    );
                    std::thread::sleep(RETRY_WAIT);
                }
            }
        },
        None => println!("Sin servidor en {CONFIG_FILE}; no se buscan actualizaciones."),
    }

    let (slot, console) = log_slot(&dir);
    let ok = run_client(&dir, &game_args, &slot, console);
    if let Some(url) = url {
        send_log(&dir, &url, &slot);
    }
    if !ok {
        println!("\nEl juego se cerró con un error. Mira {}. Pulsa Enter para salir.", slot.game);
        let _ = io::stdin().lock().lines().next();
    }
}

/// Uploads the game log (`mu-client.log`) and its console output to the server, so
/// what happened can be seen without the player having to copy it.
fn send_log(dir: &Path, url: &str, slot: &LogSlot) {
    let read = |name: &str| std::fs::read(dir.join(name)).unwrap_or_default();
    let mut body = format!("=== {} ===\n", slot.game).into_bytes();
    body.extend(read(&slot.game));
    let console = read(&slot.console);
    if !console.is_empty() {
        body.extend(b"\n=== consola ===\n");
        body.extend(console);
    }
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout_read(Duration::from_secs(20))
        .build();
    match agent.post(&format!("{url}/log")).send_bytes(&body) {
        Ok(_) => println!("Registro del juego enviado."),
        Err(e) => println!("No se pudo enviar el registro del juego ({e})."),
    }
}

fn self_name() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|e| Some(e.file_name()?.to_str()?.to_string()))
        .unwrap_or_else(|| "mu-launcher.exe".into())
}

/// `servidor=...` from `mu-launcher.txt` (or the `MU_UPDATE_URL` variable).
fn server_url(dir: &Path) -> Option<String> {
    if let Ok(u) = std::env::var("MU_UPDATE_URL") {
        return Some(u);
    }
    let text = std::fs::read_to_string(dir.join(CONFIG_FILE)).ok()?;
    let url = text.lines().find_map(|l| l.trim().strip_prefix("servidor="))?.trim().trim_end_matches('/');
    Some(if url.contains("://") { url.to_string() } else { format!("http://{url}") })
        .map(|u| if u.rsplit(':').next().is_some_and(|p| p.parse::<u16>().is_ok()) { u } else { format!("{u}:{DEFAULT_PORT}") })
}

struct Updated {
    files: usize,
    relaunch: bool,
}

enum Failure {
    /// The list could not be requested.
    Offline(String),
    /// Files still left to download.
    Download(String),
}

/// What is known about each local file: (size, date) → hash. Avoids rehashing 1 GB every time.
type State = HashMap<String, (u64, u128, String)>;

/// The server list: with compressed sizes if the server provides them, otherwise the classic one.
fn fetch_manifest(agent: &ureq::Agent, url: &str) -> Result<Vec<Entry>, String> {
    // Ok(None): the server is old and has no `/manifest2`.
    let get = |path: &str| -> Result<Option<String>, String> {
        match agent.get(&format!("{url}{path}")).call() {
            Ok(r) => r.into_string().map(Some).map_err(|e| e.to_string()),
            Err(ureq::Error::Status(404, _)) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    };
    let invalid = || "lista del servidor no válida".to_string();
    match get("/manifest2")? {
        Some(text) => parse_manifest2(&text).ok_or_else(invalid),
        None => parse_manifest(&get("/manifest")?.ok_or_else(invalid)?).ok_or_else(invalid),
    }
}

fn update(dir: &Path, url: &str) -> Result<Updated, Failure> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout_read(Duration::from_secs(30))
        .build();
    print!("Buscando actualizaciones en {url}... ");
    let _ = io::stdout().flush();
    let manifest = fetch_manifest(&agent, url).map_err(Failure::Offline)?;
    println!("{} archivos en el servidor.", manifest.len());

    let mut state = load_state(dir);
    // What the launcher installed (it's in its state) and the server no longer publishes: deleted.
    // Only those files: the player's settings and logs are never in the state.
    let published: std::collections::HashSet<&str> = manifest.iter().map(|e| e.path.as_str()).collect();
    let gone: Vec<String> = state
        .keys()
        .filter(|p| !published.contains(p.as_str()) && is_safe_path(p) && !excluded(p))
        .cloned()
        .collect();
    if !gone.is_empty() {
        println!("Quitando {} archivos que ya no forman parte del cliente.", gone.len());
        for p in &gone {
            let _ = std::fs::remove_file(to_local(dir, p));
            state.remove(p);
        }
        save_state(dir, &state, false);
    }
    let mut pending: Vec<&Entry> = vec![];
    let mut checked = 0usize;
    for (i, e) in manifest.iter().enumerate() {
        // Show progress while checking files not yet known (the first time, all of them).
        if checked > 0 && (i % 250 == 0 || i + 1 == manifest.len()) {
            print!("\r  Comprobando archivos  {}  {}/{}", bar(i as u64 + 1, manifest.len() as u64), i + 1, manifest.len());
            let _ = io::stdout().flush();
        }
        let local = to_local(dir, &e.path);
        let current = match std::fs::metadata(&local) {
            Ok(m) if m.len() == e.size => {
                let stamp = stamp(&m);
                match state.get(&e.path) {
                    Some((size, s, sha)) if *size == m.len() && *s == stamp => Some(sha.clone()),
                    _ => {
                        checked += 1;
                        let sha = hash_file(&local).map_err(|e| Failure::Download(e.to_string()))?;
                        state.insert(e.path.clone(), (m.len(), stamp, sha.clone()));
                        Some(sha)
                    }
                }
            }
            _ => None,
        };
        if current.as_deref() != Some(e.sha256.as_str()) {
            pending.push(e);
        }
    }
    if checked > 0 {
        println!(" listo.");
    }
    if pending.is_empty() {
        save_state(dir, &state, true);
        return Ok(Updated { files: 0, relaunch: false });
    }
    // From here until it finishes, the client is not complete.
    save_state(dir, &state, false);

    // If there is a new launcher, that first: the new one downloads the rest (maybe differently).
    let me = self_name();
    if let Some(e) = pending.iter().find(|e| e.path.eq_ignore_ascii_case(&me)) {
        println!("Hay un lanzador nuevo; se descarga primero.");
        let done = download_all(dir, url, &agent, &[e], &mut state, &me)?;
        return Ok(Updated { files: done, relaunch: true });
    }

    let wire: u64 = pending.iter().map(|e| wire_size(e)).sum();
    let total: u64 = pending.iter().map(|e| e.size).sum();
    println!(
        "Hay que descargar {} archivos nuevos o cambiados ({:.1} MB, {:.1} MB comprimidos):",
        pending.len(),
        total as f64 / 1e6,
        wire as f64 / 1e6
    );
    for e in pending.iter().take(10) {
        println!("  - {}", e.path);
    }
    if pending.len() > 10 {
        println!("  ... y {} más", pending.len() - 10);
    }
    let files = download_all(dir, url, &agent, &pending, &mut state, &me)?;
    save_state(dir, &state, true);
    Ok(Updated { files, relaunch: false })
}

/// What travels over the network for a file.
fn wire_size(e: &Entry) -> u64 {
    if e.packed > 0 { e.packed } else { e.size }
}

/// Downloads `pending` over several connections at once. Each file is tried several times; if
/// any doesn't arrive, returns an error (and the caller requests the list again and retries).
fn download_all(
    dir: &Path,
    url: &str,
    agent: &ureq::Agent,
    pending: &[&Entry],
    state: &mut State,
    me: &str,
) -> Result<usize, Failure> {
    let total: u64 = pending.iter().map(|e| wire_size(e)).sum();
    let wire = AtomicU64::new(0);
    let next = AtomicUsize::new(0);
    let finished = AtomicUsize::new(0);
    let shared = Mutex::new(std::mem::take(state));
    let errors: Mutex<Vec<String>> = Mutex::new(vec![]);
    let current: Mutex<String> = Mutex::new(String::new());
    let started = Instant::now();

    std::thread::scope(|s| {
        for _ in 0..WORKERS.min(pending.len()) {
            s.spawn(|| {
                while let Some(e) = pending.get(next.fetch_add(1, Ordering::SeqCst)) {
                    *current.lock().unwrap() = e.path.clone();
                    let local = to_local(dir, &e.path);
                    let mut result = Err("sin intentos".to_string());
                    for attempt in 0..ATTEMPTS {
                        if attempt > 0 {
                            std::thread::sleep(Duration::from_secs(1 << attempt));
                        }
                        result = download(agent, url, e, &local, &wire).and_then(|()| install(e, &local, me));
                        if result.is_ok() {
                            break;
                        }
                    }
                    match result {
                        Ok(()) => {
                            if let Ok(m) = std::fs::metadata(&local) {
                                shared.lock().unwrap().insert(e.path.clone(), (m.len(), stamp(&m), e.sha256.clone()));
                            }
                        }
                        Err(err) => errors.lock().unwrap().push(format!("{}: {err}", e.path)),
                    }
                    finished.fetch_add(1, Ordering::SeqCst);
                }
            });
        }
        // Progress, and save the state now and then in case of interruption.
        let mut last_save = Instant::now();
        loop {
            let n = finished.load(Ordering::SeqCst);
            let got = wire.load(Ordering::SeqCst);
            let secs = started.elapsed().as_secs_f64().max(0.1);
            print!(
                "\r  Descargando  {}  {:.1}/{:.1} MB  {:.1} MB/s  {}/{}  {:<40}",
                bar(got, total),
                got as f64 / 1e6,
                total as f64 / 1e6,
                got as f64 / 1e6 / secs,
                n,
                pending.len(),
                short(&current.lock().unwrap())
            );
            let _ = io::stdout().flush();
            if n >= pending.len() {
                break;
            }
            if last_save.elapsed() > Duration::from_secs(3) {
                save_state(dir, &shared.lock().unwrap(), false);
                last_save = Instant::now();
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    });
    println!();
    *state = shared.into_inner().unwrap();
    save_state(dir, state, false);
    let errors = errors.into_inner().unwrap();
    if errors.is_empty() {
        Ok(pending.len())
    } else {
        for e in errors.iter().take(5) {
            println!("  falló {e}");
        }
        Err(Failure::Download(format!("faltan {} de {} archivos", errors.len(), pending.len())))
    }
}

/// Counts what arrives over the network (for progress); if the download fails, subtracts it.
struct Counting<'a, R> {
    inner: R,
    wire: &'a AtomicU64,
    got: u64,
}

impl<R: Read> Read for Counting<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.got += n as u64;
        self.wire.fetch_add(n as u64, Ordering::Relaxed);
        Ok(n)
    }
}

impl<R> Drop for Counting<'_, R> {
    fn drop(&mut self) {
        // Progress counts what downloaded correctly: a failed attempt's bytes are removed, and the
        // good file counts what the list says (no more, no less).
        self.wire.fetch_sub(self.got, Ordering::Relaxed);
    }
}

/// Downloads to `<file>.part` (decompressing if compressed) and checks size and hash.
fn download(agent: &ureq::Agent, url: &str, e: &Entry, local: &Path, wire: &AtomicU64) -> Result<(), String> {
    let run = || -> Res<()> {
        if let Some(parent) = local.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let part = local.with_extension(part_ext(local));
        let path = if e.packed > 0 { format!("/z/{}", e.sha256) } else { format!("/file/{}", encode_path(&e.path)) };
        let body = agent.get(&format!("{url}{path}")).call()?.into_reader();
        let counting = Counting { inner: body, wire, got: 0 };
        let mut reader: Box<dyn Read + '_> =
            if e.packed > 0 { Box::new(zstd::stream::read::Decoder::new(counting)?) } else { Box::new(counting) };
        let mut out = io::BufWriter::new(std::fs::File::create(&part)?);
        let mut h = Sha256::new();
        let mut buf = vec![0u8; 1 << 16];
        let mut got = 0u64;
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            got += n as u64;
            if got > e.size {
                break;
            }
            out.write_all(&buf[..n])?;
            h.update(&buf[..n]);
        }
        drop(reader);
        out.flush()?;
        drop(out);
        if got != e.size || hex(&h.finalize()) != e.sha256 {
            let _ = std::fs::remove_file(&part);
            return Err("llegó mal (¿se está publicando una versión nueva?)".into());
        }
        wire.fetch_add(wire_size(e), Ordering::Relaxed);
        Ok(())
    };
    run().map_err(|e| e.to_string())
}

/// Moves the already verified `.part` into place.
fn install(e: &Entry, local: &Path, me: &str) -> Result<(), String> {
    let run = || -> io::Result<()> {
        if e.path.eq_ignore_ascii_case(me) && local.exists() {
            // Windows doesn't allow overwriting an open .exe, but does allow renaming it.
            let old = local.with_extension("exe.old");
            let _ = std::fs::remove_file(&old);
            std::fs::rename(local, old)?;
        }
        std::fs::rename(local.with_extension(part_ext(local)), local)
    };
    run().map_err(|e| e.to_string())
}

fn part_ext(p: &Path) -> String {
    match p.extension().and_then(|e| e.to_str()) {
        Some(ext) => format!("{ext}.part"),
        None => "part".into(),
    }
}

/// The last 40 characters of a path (without splitting a character).
fn short(p: &str) -> &str {
    let mut start = p.len().saturating_sub(40);
    while !p.is_char_boundary(start) {
        start += 1;
    }
    &p[start..]
}

/// Text progress bar: `[██████░░░░░░]  45%`.
fn bar(done: u64, total: u64) -> String {
    const WIDTH: u64 = 30;
    let frac = if total == 0 { 1.0 } else { (done as f64 / total as f64).min(1.0) };
    let filled = (frac * WIDTH as f64).round() as u64;
    format!(
        "[{}{}] {:3}%",
        "█".repeat(filled as usize),
        "░".repeat((WIDTH - filled) as usize),
        (frac * 100.0).floor() as u64
    )
}

fn stamp(m: &std::fs::Metadata) -> u128 {
    m.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos())
}

fn load_state(dir: &Path) -> State {
    let Ok(text) = std::fs::read_to_string(dir.join(STATE_FILE)) else { return State::new() };
    text.lines()
        .filter_map(|l| {
            let mut it = l.splitn(4, '\t');
            let sha = it.next()?.to_string();
            let size = it.next()?.parse().ok()?;
            let stamp = it.next()?.parse().ok()?;
            Some((it.next()?.to_string(), (size, stamp, sha)))
        })
        .collect()
}

/// Did the last update complete fully?
fn load_complete(dir: &Path) -> bool {
    std::fs::read_to_string(dir.join(STATE_FILE)).is_ok_and(|t| t.lines().next() == Some(COMPLETE_MARK))
}

fn save_state(dir: &Path, state: &State, complete: bool) {
    let mut text = if complete { format!("{COMPLETE_MARK}\n") } else { String::new() };
    text.extend(state.iter().map(|(p, (size, stamp, sha))| format!("{sha}\t{size}\t{stamp}\t{p}\n")));
    let _ = std::fs::write(dir.join(STATE_FILE), text);
}

/// The two logs of a session: the game's and its console output.
struct LogSlot {
    game: String,
    console: String,
}

/// With several games open at once, each has its own logs: the first
/// `mu-client.log` and `mu-client-consola.log`, the second `mu-client-2.log` and
/// `mu-client-consola-2.log`... A console log opened by another launcher is in use
/// (opened without letting anyone else write), so the next one is used.
fn log_slot(dir: &Path) -> (LogSlot, Option<std::fs::File>) {
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

/// Starts the game and waits for it to close. Returns whether it ended well. The game writes its
/// log to `slot.game` (told via `MU_LOG`); its console output goes to `console`.
fn run_client(dir: &Path, args: &[String], slot: &LogSlot, log: Option<std::fs::File>) -> bool {
    let exe: PathBuf = dir.join(CLIENT_EXE);
    let mut cmd = Command::new(&exe);
    cmd.args(args).current_dir(dir).env("MU_LOG", &slot.game);
    if let Some(log) = log {
        if let Ok(log2) = log.try_clone() {
            cmd.stdout(Stdio::from(log)).stderr(Stdio::from(log2));
        }
    }
    println!("Abriendo el juego...");
    match cmd.status() {
        Ok(s) => s.success(),
        Err(e) => {
            println!("No se pudo abrir {}: {e}", exe.display());
            false
        }
    }
}
