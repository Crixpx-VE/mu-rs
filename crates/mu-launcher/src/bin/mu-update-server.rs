//! Servidor de actualizaciones del cliente.
//!
//!   mu-update-server --dir <carpeta_del_cliente> [--listen 0.0.0.0:44480]
//!
//! Publica todos los archivos de `--dir` (menos registros y temporales). Solo entrega los
//! archivos que están en la lista, así que no se puede pedir nada de fuera de la carpeta.
//! Cuando cambia un archivo (por ejemplo, al publicar un `mu-client.exe` nuevo), la lista se
//! recalcula sola; para evitar entregar un archivo a medio copiar, publícalo con un nombre
//! temporal (`.tmp`) y renómbralo al final.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use mu_launcher::{DEFAULT_PORT, Entry, decode_path, format_manifest, hash_file, scan, to_local};
use tiny_http::{Header, Method, Request, Response, Server};

/// Descargas a la vez; el resto recibe "503, inténtalo luego".
const MAX_ACTIVE: usize = 32;
/// Cada cuánto se vuelve a mirar la carpeta como mucho.
const RESCAN_EVERY: Duration = Duration::from_secs(2);

struct Catalog {
    dir: PathBuf,
    /// Huella calculada de cada archivo: (tamaño, fecha) → no hace falta recalcularla.
    hashes: HashMap<String, (u64, SystemTime, String)>,
    entries: HashMap<String, Entry>,
    text: String,
    scanned: Option<Instant>,
}

impl Catalog {
    fn refresh(&mut self) -> std::io::Result<()> {
        if self.scanned.is_some_and(|t| t.elapsed() < RESCAN_EVERY) {
            return Ok(());
        }
        let found = scan(&self.dir)?;
        let mut list = Vec::with_capacity(found.len());
        let mut hashes = HashMap::with_capacity(found.len());
        let mut changed = 0;
        for f in found {
            let sha = match self.hashes.get(&f.rel) {
                Some((size, modified, sha)) if *size == f.size && *modified == f.modified => sha.clone(),
                _ => {
                    changed += 1;
                    hash_file(&f.path)?
                }
            };
            hashes.insert(f.rel.clone(), (f.size, f.modified, sha.clone()));
            list.push(Entry { path: f.rel, size: f.size, sha256: sha });
        }
        if changed > 0 || hashes.len() != self.hashes.len() {
            let total: u64 = list.iter().map(|e| e.size).sum();
            println!("lista: {} archivos, {} MB ({changed} nuevos o cambiados)", list.len(), total / 1_000_000);
        }
        self.text = format_manifest(&list);
        self.entries = list.into_iter().map(|e| (e.path.clone(), e)).collect();
        self.hashes = hashes;
        self.scanned = Some(Instant::now());
        Ok(())
    }
}

fn main() {
    let mut dir = None;
    let mut listen = format!("0.0.0.0:{DEFAULT_PORT}");
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--dir" => dir = args.next().map(PathBuf::from),
            "--listen" => listen = args.next().expect("--listen <ip:puerto>"),
            _ => {
                eprintln!("uso: mu-update-server --dir <carpeta> [--listen 0.0.0.0:{DEFAULT_PORT}]");
                std::process::exit(2);
            }
        }
    }
    let Some(dir) = dir.filter(|d| d.is_dir()) else {
        eprintln!("falta --dir con la carpeta del cliente");
        std::process::exit(2);
    };

    let mut catalog =
        Catalog { dir, hashes: HashMap::new(), entries: HashMap::new(), text: String::new(), scanned: None };
    catalog.refresh().expect("no se pudo leer la carpeta");
    let catalog = Arc::new(Mutex::new(catalog));

    let server = Server::http(&listen).unwrap_or_else(|e| panic!("no se pudo escuchar en {listen}: {e}"));
    println!("servidor de actualizaciones en {listen}");
    let active = Arc::new(AtomicUsize::new(0));

    for req in server.incoming_requests() {
        if active.fetch_add(1, Ordering::SeqCst) >= MAX_ACTIVE {
            active.fetch_sub(1, Ordering::SeqCst);
            let _ = req.respond(Response::from_string("ocupado").with_status_code(503));
            continue;
        }
        let (catalog, active) = (catalog.clone(), active.clone());
        std::thread::spawn(move || {
            handle(req, &catalog);
            active.fetch_sub(1, Ordering::SeqCst);
        });
    }
}

fn handle(req: Request, catalog: &Mutex<Catalog>) {
    let who = req.remote_addr().map(|a| a.ip().to_string()).unwrap_or_default();
    if !matches!(req.method(), Method::Get) {
        let _ = req.respond(Response::from_string("").with_status_code(405));
        return;
    }
    let url = req.url().to_string();

    if url == "/manifest" {
        let text = {
            let mut c = catalog.lock().unwrap();
            if let Err(e) = c.refresh() {
                eprintln!("error leyendo la carpeta: {e}");
            }
            c.text.clone()
        };
        println!("{who} pide la lista");
        let _ = req.respond(Response::from_string(text).with_header(header("Content-Type", "text/plain; charset=utf-8")));
        return;
    }

    let Some(rel) = url.strip_prefix("/file/").and_then(decode_path) else {
        let _ = req.respond(Response::from_string("no existe").with_status_code(404));
        return;
    };
    // Solo archivos de la lista: la ruta ya pasó `is_safe_path` al hacer la lista.
    let found = {
        let c = catalog.lock().unwrap();
        c.entries.get(&rel).map(|e| (to_local(&c.dir, &e.path), e.size))
    };
    let Some((path, size)) = found else {
        let _ = req.respond(Response::from_string("no existe").with_status_code(404));
        return;
    };
    match open_if_unchanged(&path, size) {
        Some(file) => {
            println!("{who} descarga {rel} ({} KB)", size / 1000);
            let _ = req.respond(Response::from_file(file).with_header(header("Content-Type", "application/octet-stream")));
        }
        // Cambió desde que se hizo la lista: que el lanzador pida la lista otra vez.
        None => {
            let _ = req.respond(Response::from_string("cambiando").with_status_code(503));
        }
    }
}

fn open_if_unchanged(path: &Path, size: u64) -> Option<std::fs::File> {
    let f = std::fs::File::open(path).ok()?;
    (f.metadata().ok()?.len() == size).then_some(f)
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("cabecera válida")
}
