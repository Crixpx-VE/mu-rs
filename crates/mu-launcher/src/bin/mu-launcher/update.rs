//! Keeping the client up to date. Runs on its own thread and reports to the window through
//! [`Status`].
//!
//! 1. Requests the file list from the server (address in `mu-launcher.txt`: `servidor=http://ip:port`).
//! 2. Downloads whatever is missing or changed (compressed, several at once), checking each
//!    file's SHA-256 hash. If something fails, retries until it has everything.
//!
//! The game only starts with all files present. If the server doesn't respond, the existing version
//! may start only if the last update completed fully. If the launcher itself changed, it restarts
//! with the new version (before downloading the rest).

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, UNIX_EPOCH};

use mu_launcher::{
    DEFAULT_PORT, Entry, encode_path, excluded, hash_file, hex, is_safe_path, parse_manifest, parse_manifest2,
    to_local,
};
use sha2::{Digest, Sha256};

pub const CONFIG_FILE: &str = "mu-launcher.txt";
const STATE_FILE: &str = ".mu-launcher-state";
/// State line saying the last update completed fully.
const COMPLETE_MARK: &str = "#completo";
/// Concurrent downloads.
const WORKERS: usize = 6;
/// Attempts per file in each round (then the list is requested again).
const ATTEMPTS: u32 = 4;
/// Wait before retrying the whole update.
const RETRY_WAIT: Duration = Duration::from_secs(5);

type Res<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Connection with the update server, as shown in the window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Server {
    #[default]
    Connecting,
    Online,
    Offline,
    /// No `servidor=` in `mu-launcher.txt`.
    None,
}

/// What the updater is doing, for the window.
#[derive(Clone, Debug, Default)]
pub struct Status {
    pub server: Server,
    /// Main line ("Comprobando archivos", "Descargando"...).
    pub title: String,
    /// Second line (sizes, speed, current file, errors).
    pub detail: String,
    /// 0..1 while checking or downloading.
    pub progress: Option<f32>,
    /// The client is complete (or may start anyway) and the game can be opened.
    pub ready: bool,
}

pub type Shared = Arc<Mutex<Status>>;

fn set(status: &Shared, f: impl FnOnce(&mut Status)) {
    if let Ok(mut s) = status.lock() {
        f(&mut s);
    }
}

pub fn self_name() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|e| Some(e.file_name()?.to_str()?.to_string()))
        .unwrap_or_else(|| "mu-launcher.exe".into())
}

/// `servidor=...` from `mu-launcher.txt` (or the `MU_UPDATE_URL` variable).
pub fn server_url(dir: &Path) -> Option<String> {
    if let Ok(u) = std::env::var("MU_UPDATE_URL") {
        return Some(u);
    }
    let text = std::fs::read_to_string(dir.join(CONFIG_FILE)).ok()?;
    let url = text.lines().find_map(|l| l.trim().strip_prefix("servidor="))?.trim().trim_end_matches('/');
    Some(if url.contains("://") { url.to_string() } else { format!("http://{url}") })
        .map(|u| if u.rsplit(':').next().is_some_and(|p| p.parse::<u16>().is_ok()) { u } else { format!("{u}:{DEFAULT_PORT}") })
}

/// The update loop: retries until the client is complete (or may start offline). If the launcher
/// itself was updated, starts the new one with the same arguments and exits.
pub fn run(dir: &Path, url: Option<String>, game_args: &[String], status: &Shared) {
    let Some(url) = url else {
        set(status, |s| {
            s.server = Server::None;
            s.title = "Listo para jugar".into();
            s.detail = format!("Sin servidor en {CONFIG_FILE}: no se buscan actualizaciones.");
            s.ready = true;
        });
        return;
    };
    loop {
        match update(dir, &url, status) {
            Ok(Updated { files, relaunch: false }) => {
                set(status, |s| {
                    s.server = Server::Online;
                    s.title = "Listo para jugar".into();
                    s.detail = if files == 0 {
                        "El cliente está al día.".into()
                    } else {
                        format!("Se actualizaron {files} archivos. El cliente está completo.")
                    };
                    s.progress = Some(1.0);
                    s.ready = true;
                });
                return;
            }
            Ok(Updated { relaunch: true, .. }) => {
                set(status, |s| s.title = "Abriendo el lanzador nuevo...".into());
                let _ = Command::new(dir.join(self_name())).args(game_args).spawn();
                std::process::exit(0);
            }
            Err(Failure::Offline(e)) if load_complete(dir) => {
                set(status, |s| {
                    s.server = Server::Offline;
                    s.title = "Sin conexión con el servidor".into();
                    s.detail = format!("Puedes jugar con la versión que tienes (la última actualización terminó entera). ({e})");
                    s.progress = None;
                    s.ready = true;
                });
                return;
            }
            Err(Failure::Offline(e) | Failure::Download(e)) => {
                for left in (1..=RETRY_WAIT.as_secs()).rev() {
                    set(status, |s| {
                        s.title = format!("No se pudo terminar la actualización; se reintenta en {left} s");
                        s.detail = e.clone();
                    });
                    std::thread::sleep(Duration::from_secs(1));
                }
            }
        }
    }
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

pub fn agent(read_timeout: Duration) -> ureq::Agent {
    ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(5)).timeout_read(read_timeout).build()
}

fn update(dir: &Path, url: &str, status: &Shared) -> Result<Updated, Failure> {
    let agent = agent(Duration::from_secs(30));
    set(status, |s| {
        s.server = Server::Connecting;
        s.title = "Buscando actualizaciones...".into();
        s.detail = String::new();
        s.progress = None;
    });
    let manifest = fetch_manifest(&agent, url).map_err(|e| {
        set(status, |s| s.server = Server::Offline);
        Failure::Offline(e)
    })?;
    set(status, |s| s.server = Server::Online);

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
        for p in &gone {
            let _ = std::fs::remove_file(to_local(dir, p));
            state.remove(p);
        }
        save_state(dir, &state, false);
    }
    let mut pending: Vec<&Entry> = vec![];
    for (i, e) in manifest.iter().enumerate() {
        if i % 100 == 0 || i + 1 == manifest.len() {
            set(status, |s| {
                s.title = "Comprobando archivos".into();
                s.detail = format!("{} de {}", i + 1, manifest.len());
                s.progress = Some((i + 1) as f32 / manifest.len().max(1) as f32);
            });
        }
        let local = to_local(dir, &e.path);
        let current = match std::fs::metadata(&local) {
            Ok(m) if m.len() == e.size => {
                let stamp = stamp(&m);
                match state.get(&e.path) {
                    Some((size, s, sha)) if *size == m.len() && *s == stamp => Some(sha.clone()),
                    _ => {
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
    if pending.is_empty() {
        save_state(dir, &state, true);
        return Ok(Updated { files: 0, relaunch: false });
    }
    // From here until it finishes, the client is not complete.
    save_state(dir, &state, false);

    // If there is a new launcher, that first: the new one downloads the rest (maybe differently).
    let me = self_name();
    if let Some(e) = pending.iter().find(|e| e.path.eq_ignore_ascii_case(&me)) {
        set(status, |s| s.title = "Actualizando el lanzador".into());
        let done = download_all(dir, url, &agent, &[e], &mut state, &me, status)?;
        return Ok(Updated { files: done, relaunch: true });
    }

    let files = download_all(dir, url, &agent, &pending, &mut state, &me, status)?;
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
    status: &Shared,
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
            let file = short(&current.lock().unwrap()).to_string();
            set(status, |s| {
                s.title = format!("Descargando {} de {} archivos", n.min(pending.len()), pending.len());
                s.detail = format!(
                    "{:.1} / {:.1} MB   ·   {:.1} MB/s   ·   {file}",
                    got as f64 / 1e6,
                    total as f64 / 1e6,
                    got as f64 / 1e6 / secs
                );
                s.progress = Some(if total == 0 { 1.0 } else { (got as f64 / total as f64).min(1.0) as f32 });
            });
            if n >= pending.len() {
                break;
            }
            if last_save.elapsed() > Duration::from_secs(3) {
                save_state(dir, &shared.lock().unwrap(), false);
                last_save = Instant::now();
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    });
    *state = shared.into_inner().unwrap();
    save_state(dir, state, false);
    let errors = errors.into_inner().unwrap();
    if errors.is_empty() {
        Ok(pending.len())
    } else {
        let first = errors.first().cloned().unwrap_or_default();
        Err(Failure::Download(format!("faltan {} de {} archivos ({first})", errors.len(), pending.len())))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_keeps_the_end() {
        assert_eq!(short("abc"), "abc");
        let long = format!("{}ñfin.bmd", "x".repeat(50));
        assert!(short(&long).ends_with("ñfin.bmd"));
        assert!(short(&long).len() <= 40);
    }
}
