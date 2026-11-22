//! Servidor de actualizaciones del cliente.
//!
//!   mu-update-server --dir <carpeta_del_cliente> [--listen 0.0.0.0:44480]
//!
//! Publica todos los archivos de `--dir` (menos registros y temporales). Solo entrega los
//! archivos que están en la lista, así que no se puede pedir nada de fuera de la carpeta.
//! Cuando cambia un archivo (por ejemplo, al publicar un `mu-client.exe` nuevo), la lista se
//! recalcula sola; para evitar entregar un archivo a medio copiar, publícalo con un nombre
//! temporal (`.tmp`) y renómbralo al final.
//!
//! También recibe los registros del juego que suben los lanzadores (`POST /log`) y los guarda
//! en `registros/` junto a este programa (como mucho 8 MB cada uno y los 300 últimos).
//!
//! Cada archivo publicado se comprime una vez con zstd (nivel 19) en `comprimidos/<sha256>.zst`
//! junto a este programa, en segundo plano; mientras no está listo se entrega sin comprimir.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use mu_launcher::{DEFAULT_PORT, Entry, decode_path, format_manifest, format_manifest2, hash_file, hex, is_sha256, scan, to_local};
use sha2::{Digest, Sha256};
use tiny_http::{Header, Method, Request, Response, Server};

/// Descargas a la vez; el resto recibe "503, inténtalo luego" (el lanzador reintenta).
const MAX_ACTIVE: usize = 64;
/// Cada cuánto se vuelve a mirar la carpeta como mucho.
const RESCAN_EVERY: Duration = Duration::from_secs(2);
/// Nivel de zstd: lento al comprimir (se hace una vez), igual de rápido al descomprimir.
const ZSTD_LEVEL: i32 = 19;

struct Catalog {
    dir: PathBuf,
    /// Huella calculada de cada archivo: (tamaño, fecha) → no hace falta recalcularla.
    hashes: HashMap<String, (u64, SystemTime, String)>,
    /// Por ruta, en orden.
    list: Vec<Entry>,
    entries: HashMap<String, Entry>,
    text: String,
    scanned: Option<Instant>,
    /// Carpeta de los comprimidos (`<sha256>.zst`).
    cache: PathBuf,
    /// Huella → tamaño comprimido (0: no compensa comprimirlo y se entrega tal cual).
    packed: HashMap<String, u64>,
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
            list.push(Entry { path: f.rel, size: f.size, sha256: sha, packed: 0 });
        }
        if changed > 0 || hashes.len() != self.hashes.len() {
            let total: u64 = list.iter().map(|e| e.size).sum();
            println!("lista: {} archivos, {} MB ({changed} nuevos o cambiados)", list.len(), total / 1_000_000);
        }
        self.text = format_manifest(&list);
        self.entries = list.iter().map(|e| (e.path.clone(), e.clone())).collect();
        self.list = list;
        self.hashes = hashes;
        self.scanned = Some(Instant::now());
        Ok(())
    }

    /// La lista con el tamaño comprimido de lo que ya está comprimido.
    fn text2(&self) -> String {
        let list: Vec<Entry> = self
            .list
            .iter()
            .map(|e| Entry { packed: self.packed.get(&e.sha256).copied().unwrap_or(0), ..e.clone() })
            .collect();
        format_manifest2(&list)
    }
}

/// Carpeta de los archivos comprimidos: `comprimidos` junto a este programa (fuera de la publicada).
fn cache_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|e| Some(e.parent()?.join("comprimidos")))
        .unwrap_or_else(|| PathBuf::from("comprimidos"))
}

/// En segundo plano: comprime lo que se publica y borra los comprimidos que ya no se usan.
fn compressor(catalog: Arc<Mutex<Catalog>>) {
    let workers = std::thread::available_parallelism().map_or(2, |n| (n.get() / 2).max(1));
    loop {
        let (todo, cache) = {
            let mut c = catalog.lock().unwrap();
            if let Err(e) = c.refresh() {
                eprintln!("error leyendo la carpeta: {e}");
            }
            let mut seen = HashSet::new();
            let todo: Vec<(String, PathBuf, u64)> = c
                .list
                .iter()
                .filter(|e| !c.packed.contains_key(&e.sha256) && seen.insert(e.sha256.clone()))
                .map(|e| (e.sha256.clone(), to_local(&c.dir, &e.path), e.size))
                .collect();
            (todo, c.cache.clone())
        };
        if todo.is_empty() {
            std::thread::sleep(Duration::from_secs(10));
            continue;
        }
        let _ = std::fs::create_dir_all(&cache);
        let started = Instant::now();
        let next = AtomicUsize::new(0);
        let totals = Mutex::new((0usize, 0u64, 0u64));
        std::thread::scope(|s| {
            for _ in 0..workers {
                s.spawn(|| {
                    while let Some((sha, path, size)) = todo.get(next.fetch_add(1, Ordering::SeqCst)) {
                        let Some(packed) = pack(&cache, sha, path) else { continue };
                        catalog.lock().unwrap().packed.insert(sha.clone(), packed);
                        let mut t = totals.lock().unwrap();
                        t.0 += 1;
                        t.1 += size;
                        t.2 += if packed > 0 { packed } else { *size };
                        if t.0.is_multiple_of(1000) {
                            println!("comprimidos {}/{} archivos", t.0, todo.len());
                        }
                    }
                });
            }
        });
        let (n, raw, out) = *totals.lock().unwrap();
        if n > 0 {
            println!(
                "comprimidos {n} archivos: {} MB → {} MB en {} s",
                raw / 1_000_000,
                out / 1_000_000,
                started.elapsed().as_secs()
            );
        }
        prune_cache(&catalog);
        // Lo que falló (p. ej. cambió mientras se comprimía) se reintenta en la próxima vuelta.
        std::thread::sleep(Duration::from_secs(5));
    }
}

/// Comprime un archivo en `<cache>/<sha>.zst` (o deja `<sha>.raw` si no compensa). Devuelve el
/// tamaño comprimido (0 si no compensa), o nada si el archivo cambió mientras se comprimía.
fn pack(cache: &Path, sha: &str, path: &Path) -> Option<u64> {
    let zst = cache.join(format!("{sha}.zst"));
    let raw = cache.join(format!("{sha}.raw"));
    if let Ok(m) = std::fs::metadata(&zst) {
        return Some(m.len());
    }
    if raw.exists() {
        return Some(0);
    }
    let result = (|| -> std::io::Result<Option<u64>> {
        let mut input = HashingReader { inner: std::fs::File::open(path)?, hash: Sha256::new(), size: 0 };
        let tmp = cache.join(format!("{sha}.zst.tmp"));
        let mut enc = zstd::stream::write::Encoder::new(std::fs::File::create(&tmp)?, ZSTD_LEVEL)?;
        std::io::copy(&mut input, &mut enc)?;
        enc.finish()?.flush()?;
        // Lo comprimido tiene que ser justo el archivo de la lista.
        if hex(&input.hash.finalize()) != sha {
            let _ = std::fs::remove_file(&tmp);
            return Ok(None);
        }
        let packed = std::fs::metadata(&tmp)?.len();
        if packed + 64 < input.size {
            std::fs::rename(&tmp, &zst)?;
            Ok(Some(packed))
        } else {
            let _ = std::fs::remove_file(&tmp);
            std::fs::write(&raw, b"")?;
            Ok(Some(0))
        }
    })();
    result.unwrap_or_else(|e| {
        eprintln!("no se pudo comprimir {}: {e}", path.display());
        None
    })
}

struct HashingReader<R> {
    inner: R,
    hash: Sha256,
    size: u64,
}

impl<R: Read> Read for HashingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.hash.update(&buf[..n]);
        self.size += n as u64;
        Ok(n)
    }
}

/// Borra los comprimidos de versiones que ya no se publican.
fn prune_cache(catalog: &Mutex<Catalog>) {
    let (cache, live) = {
        let mut c = catalog.lock().unwrap();
        let live: HashSet<String> = c.list.iter().map(|e| e.sha256.clone()).collect();
        c.packed.retain(|sha, _| live.contains(sha));
        (c.cache.clone(), live)
    };
    let Ok(read) = std::fs::read_dir(&cache) else { return };
    for f in read.flatten() {
        let name = f.file_name().to_string_lossy().to_string();
        let sha = name.split('.').next().unwrap_or("");
        if !live.contains(sha) {
            let _ = std::fs::remove_file(f.path());
        }
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

    let mut catalog = Catalog {
        dir,
        hashes: HashMap::new(),
        list: vec![],
        entries: HashMap::new(),
        text: String::new(),
        scanned: None,
        cache: cache_dir(),
        packed: HashMap::new(),
    };
    catalog.refresh().expect("no se pudo leer la carpeta");
    let catalog = Arc::new(Mutex::new(catalog));
    {
        let catalog = catalog.clone();
        std::thread::spawn(move || compressor(catalog));
    }

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

/// Tamaño máximo de un registro subido.
const MAX_LOG: usize = 8 << 20;
/// Registros que se guardan como mucho (se borran los más viejos).
const MAX_LOGS: usize = 300;

/// Carpeta de los registros que suben los lanzadores: `registros` junto a este programa
/// (fuera de la carpeta publicada).
fn logs_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|e| Some(e.parent()?.join("registros")))
        .unwrap_or_else(|| PathBuf::from("registros"))
}

/// `POST /log`: el lanzador sube el registro del juego al cerrarse.
fn receive_log(mut req: Request, who: &str) {
    let mut body = Vec::new();
    let read = std::io::Read::take(req.as_reader(), MAX_LOG as u64 + 1).read_to_end(&mut body);
    if read.is_err() || body.len() > MAX_LOG {
        let _ = req.respond(Response::from_string("demasiado grande").with_status_code(413));
        return;
    }
    let dir = logs_dir();
    let saved = std::fs::create_dir_all(&dir).and_then(|_| {
        let secs = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let name = format!("{secs}-{}.log", who.replace([':', '.'], "_"));
        std::fs::write(dir.join(&name), &body).map(|_| name)
    });
    match saved {
        Ok(name) => {
            println!("{who} sube un registro ({} KB): registros/{name}", body.len() / 1000);
            prune_logs(&dir);
            let _ = req.respond(Response::from_string("ok"));
        }
        Err(e) => {
            eprintln!("no se pudo guardar el registro de {who}: {e}");
            let _ = req.respond(Response::from_string("error").with_status_code(500));
        }
    }
}

fn prune_logs(dir: &Path) {
    let Ok(read) = std::fs::read_dir(dir) else { return };
    let mut names: Vec<_> = read.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "log")).collect();
    if names.len() <= MAX_LOGS {
        return;
    }
    // Los nombres empiezan por la hora: el orden alfabético es el de llegada.
    names.sort();
    for old in &names[..names.len() - MAX_LOGS] {
        let _ = std::fs::remove_file(old);
    }
}

fn handle(req: Request, catalog: &Mutex<Catalog>) {
    let who = req.remote_addr().map(|a| a.ip().to_string()).unwrap_or_default();
    if matches!(req.method(), Method::Post) && req.url() == "/log" {
        receive_log(req, &who);
        return;
    }
    if !matches!(req.method(), Method::Get) {
        let _ = req.respond(Response::from_string("").with_status_code(405));
        return;
    }
    let url = req.url().to_string();

    if url == "/manifest" || url == "/manifest2" {
        let text = {
            let mut c = catalog.lock().unwrap();
            if let Err(e) = c.refresh() {
                eprintln!("error leyendo la carpeta: {e}");
            }
            if url == "/manifest" { c.text.clone() } else { c.text2() }
        };
        println!("{who} pide la lista");
        let _ = req.respond(Response::from_string(text).with_header(header("Content-Type", "text/plain; charset=utf-8")));
        return;
    }

    if let Some(sha) = url.strip_prefix("/z/").filter(|s| is_sha256(s)) {
        let found = {
            let c = catalog.lock().unwrap();
            c.packed.get(sha).filter(|&&n| n > 0).map(|&n| (c.cache.join(format!("{sha}.zst")), n))
        };
        match found.and_then(|(path, size)| Some((open_if_unchanged(&path, size)?, size))) {
            Some((file, size)) => {
                println!("{who} descarga {sha} comprimido ({} KB)", size / 1000);
                let _ =
                    req.respond(Response::from_file(file).with_header(header("Content-Type", "application/zstd")));
            }
            None => {
                let _ = req.respond(Response::from_string("no existe").with_status_code(404));
            }
        }
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
