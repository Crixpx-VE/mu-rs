//! Lanzador: actualiza el cliente y lo abre.
//!
//!   mu-launcher [opciones del juego, p. ej. --world 3]
//!
//! 1. Pide la lista de archivos al servidor (dirección en `mu-launcher.txt`: `servidor=http://ip:puerto`).
//! 2. Descarga lo que falte o haya cambiado, comprobando la huella SHA-256 de cada archivo.
//! 3. Abre `mu-client.exe` con las opciones recibidas.
//!
//! Si el servidor no responde, abre la versión que ya hay. Si se actualiza a sí mismo,
//! se vuelve a abrir con la versión nueva.

use std::collections::HashMap;
use std::io::{self, BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, UNIX_EPOCH};

use mu_launcher::{DEFAULT_PORT, Entry, encode_path, hash_file, hex, parse_manifest, to_local};
use sha2::{Digest, Sha256};

const CONFIG_FILE: &str = "mu-launcher.txt";
const STATE_FILE: &str = ".mu-launcher-state";
const CLIENT_EXE: &str = "mu-client.exe";

type Res<T> = Result<T, Box<dyn std::error::Error>>;

fn main() {
    let dir = std::env::current_exe().ok().and_then(|e| Some(e.parent()?.to_path_buf())).unwrap_or_default();
    let game_args: Vec<String> = std::env::args().skip(1).collect();
    let _ = std::env::set_current_dir(&dir);
    let _ = std::fs::remove_file(dir.join(self_name()).with_extension("exe.old"));

    println!("MU Rust - lanzador {}", env!("CARGO_PKG_VERSION"));
    match server_url(&dir) {
        Some(url) => match update(&dir, &url) {
            Ok(Updated { files: 0, .. }) => println!("El cliente está al día."),
            Ok(Updated { files, relaunch }) => {
                println!("Actualizados {files} archivos.");
                if relaunch {
                    println!("El lanzador se actualizó; abriendo la versión nueva...");
                    let _ = Command::new(dir.join(self_name())).args(&game_args).spawn();
                    return;
                }
            }
            Err(e) => println!("No se pudo actualizar ({e}). Se abre la versión que tienes."),
        },
        None => println!("Sin servidor en {CONFIG_FILE}; no se buscan actualizaciones."),
    }

    run_client(&dir, &game_args);
}

fn self_name() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|e| Some(e.file_name()?.to_str()?.to_string()))
        .unwrap_or_else(|| "mu-launcher.exe".into())
}

/// `servidor=...` de `mu-launcher.txt` (o la variable `MU_UPDATE_URL`).
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

/// Lo que se sabe de cada archivo local: (tamaño, fecha) → huella. Evita recalcular 700 MB cada vez.
type State = HashMap<String, (u64, u128, String)>;

fn update(dir: &Path, url: &str) -> Res<Updated> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout_read(Duration::from_secs(30))
        .build();
    print!("Buscando actualizaciones en {url}... ");
    io::stdout().flush()?;
    let text = agent.get(&format!("{url}/manifest")).call()?.into_string()?;
    let manifest = parse_manifest(&text).ok_or("lista del servidor no válida")?;
    println!("{} archivos en el servidor.", manifest.len());

    let mut state = load_state(dir);
    let mut pending: Vec<&Entry> = vec![];
    let mut checked = 0usize;
    for (i, e) in manifest.iter().enumerate() {
        // Mostrar avance mientras se comprueban archivos que aún no conocemos (la primera vez, todos).
        if checked > 0 && (i % 250 == 0 || i + 1 == manifest.len()) {
            print!("\r  Comprobando archivos  {}  {}/{}", bar(i as u64 + 1, manifest.len() as u64), i + 1, manifest.len());
            io::stdout().flush()?;
        }
        let local = to_local(dir, &e.path);
        let current = match std::fs::metadata(&local) {
            Ok(m) if m.len() == e.size => {
                let stamp = stamp(&m);
                match state.get(&e.path) {
                    Some((size, s, sha)) if *size == m.len() && *s == stamp => Some(sha.clone()),
                    _ => {
                        checked += 1;
                        let sha = hash_file(&local)?;
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
        // Guardar ya: si se corta la descarga, no hay que volver a comprobarlo todo.
        save_state(dir, &state);
    }
    let total: u64 = pending.iter().map(|e| e.size).sum();
    if !pending.is_empty() {
        println!("Hay que descargar {} archivos nuevos o cambiados ({:.1} MB):", pending.len(), total as f64 / 1e6);
        for e in pending.iter().take(10) {
            println!("  - {}", e.path);
        }
        if pending.len() > 10 {
            println!("  ... y {} más", pending.len() - 10);
        }
    }
    let me = self_name();
    let mut relaunch = false;
    let mut done = 0u64;
    for e in &pending {
        let local = to_local(dir, &e.path);
        download(&agent, url, e, &local, &mut done, total)?;
        if e.path.eq_ignore_ascii_case(&me) {
            // Windows no deja sobrescribir un .exe abierto, pero sí renombrarlo.
            std::fs::rename(&local, local.with_extension("exe.old"))?;
            relaunch = true;
        }
        std::fs::rename(local.with_extension(part_ext(&local)), &local)?;
        let m = std::fs::metadata(&local)?;
        state.insert(e.path.clone(), (m.len(), stamp(&m), e.sha256.clone()));
        save_state(dir, &state);
    }
    if !pending.is_empty() {
        println!();
    }
    save_state(dir, &state);
    Ok(Updated { files: pending.len(), relaunch })
}

/// Descarga a `<archivo>.part` comprobando tamaño y huella; el que llama lo renombra.
fn download(agent: &ureq::Agent, url: &str, e: &Entry, local: &Path, done: &mut u64, total: u64) -> Res<()> {
    if let Some(parent) = local.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let part = local.with_extension(part_ext(local));
    let mut reader = agent.get(&format!("{url}/file/{}", encode_path(&e.path))).call()?.into_reader();
    let mut out = io::BufWriter::new(std::fs::File::create(&part)?);
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut got = 0u64;
    let mut last_shown = 0u64;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
        h.update(&buf[..n]);
        got += n as u64;
        *done += n as u64;
        if *done - last_shown > 1 << 20 || got == e.size {
            last_shown = *done;
            print!(
                "\r  Descargando  {}  {:.1}/{:.1} MB  {:<40}",
                bar(*done, total),
                *done as f64 / 1e6,
                total as f64 / 1e6,
                short(&e.path)
            );
            io::stdout().flush()?;
        }
    }
    out.flush()?;
    drop(out);
    if got != e.size || hex(&h.finalize()) != e.sha256 {
        let _ = std::fs::remove_file(&part);
        return Err(format!("{} llegó mal (¿se está publicando una versión nueva?)", e.path).into());
    }
    Ok(())
}

fn part_ext(p: &Path) -> String {
    match p.extension().and_then(|e| e.to_str()) {
        Some(ext) => format!("{ext}.part"),
        None => "part".into(),
    }
}

/// Las últimas 40 letras de una ruta (sin cortar un carácter por la mitad).
fn short(p: &str) -> &str {
    let mut start = p.len().saturating_sub(40);
    while !p.is_char_boundary(start) {
        start += 1;
    }
    &p[start..]
}

/// Barra de progreso de texto: `[██████░░░░░░]  45%`.
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

fn save_state(dir: &Path, state: &State) {
    let text: String =
        state.iter().map(|(p, (size, stamp, sha))| format!("{sha}\t{size}\t{stamp}\t{p}\n")).collect();
    let _ = std::fs::write(dir.join(STATE_FILE), text);
}

/// Abre el juego; su registro va a `mu-client.log`. Si se cierra con error, lo avisa.
fn run_client(dir: &Path, args: &[String]) {
    let exe: PathBuf = dir.join(CLIENT_EXE);
    let log = std::fs::File::create(dir.join("mu-client.log")).ok();
    let mut cmd = Command::new(&exe);
    cmd.args(args).current_dir(dir);
    if let Some(log) = log {
        if let Ok(log2) = log.try_clone() {
            cmd.stdout(Stdio::from(log)).stderr(Stdio::from(log2));
        }
    }
    println!("Abriendo el juego...");
    let ok = match cmd.status() {
        Ok(s) => s.success(),
        Err(e) => {
            println!("No se pudo abrir {}: {e}", exe.display());
            false
        }
    };
    if !ok {
        println!("\nEl juego se cerró con un error. Mira mu-client.log. Pulsa Enter para salir.");
        let _ = io::stdin().lock().lines().next();
    }
}
