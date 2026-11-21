//! Actualizaciones del cliente.
//!
//! El servidor (`mu-update-server`) publica una lista con la huella SHA-256 de cada archivo
//! de la carpeta del cliente. El lanzador (`mu-launcher`) compara esa lista con lo que tiene,
//! descarga solo lo que cambió, comprueba la huella de cada descarga y arranca el juego.
//!
//! Protocolo (HTTP público, como el launcher de cualquier servidor de MU):
//!
//! ```text
//! GET /manifest        → una línea por archivo: <sha256>\t<tamaño>\t<ruta con '/'>
//! GET /file/<ruta>     → el archivo (la ruta va codificada con %XX); solo si está en la lista
//! ```
//!
//! La carpeta que se publica debe contener solo el juego: todo lo que haya dentro se puede descargar.

use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use sha2::{Digest, Sha256};

/// Puerto por defecto del servidor de actualizaciones.
pub const DEFAULT_PORT: u16 = 44480;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Ruta relativa con `/` (p. ej. `Data/World1/EncTerrain1.map`).
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

/// Huella SHA-256 de un archivo, en hexadecimal.
pub fn hash_file(path: &Path) -> io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex(&h.finalize()))
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn format_manifest(entries: &[Entry]) -> String {
    entries.iter().map(|e| format!("{}\t{}\t{}\n", e.sha256, e.size, e.path)).collect()
}

/// Lee la lista. Rechaza la lista entera si alguna ruta no es segura.
pub fn parse_manifest(text: &str) -> Option<Vec<Entry>> {
    text.lines()
        .filter(|l| !l.is_empty())
        .map(|l| {
            let mut it = l.splitn(3, '\t');
            let sha256 = it.next()?.to_string();
            let size = it.next()?.parse().ok()?;
            let path = it.next()?.to_string();
            (sha256.len() == 64 && sha256.bytes().all(|c| c.is_ascii_hexdigit()) && is_safe_path(&path))
                .then_some(Entry { path, size, sha256 })
        })
        .collect()
}

/// Ruta relativa sin trucos: nada de `..`, rutas absolutas, unidades (`C:`), `\` ni partes vacías.
/// Así ni el servidor entrega archivos de fuera de su carpeta ni el lanzador escribe fuera de la suya.
pub fn is_safe_path(p: &str) -> bool {
    !p.is_empty()
        && p.len() < 1024
        && !p.contains(['\\', ':', '\0'])
        && p.split('/').all(|part| !part.is_empty() && part != "." && part != "..")
}

pub fn to_local(base: &Path, rel: &str) -> PathBuf {
    rel.split('/').fold(base.to_path_buf(), |p, part| p.join(part))
}

/// Archivos que no se publican ni se tocan: registros, descargas a medias y la configuración local.
pub fn excluded(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel).to_ascii_lowercase();
    name.ends_with(".log")
        || name.ends_with(".part")
        || name.ends_with(".tmp")
        || name.ends_with(".old")
        || name == "mu-launcher.txt"
        || name == ".mu-launcher-state"
}

/// Un archivo encontrado al recorrer una carpeta.
pub struct Found {
    pub rel: String,
    pub path: PathBuf,
    pub size: u64,
    pub modified: SystemTime,
}

/// Todos los archivos de `dir` (recursivo), menos los excluidos.
pub fn scan(dir: &Path) -> io::Result<Vec<Found>> {
    fn walk(base: &Path, dir: &Path, prefix: &str, out: &mut Vec<Found>) -> io::Result<()> {
        for e in std::fs::read_dir(dir)? {
            let e = e?;
            let Some(name) = e.file_name().to_str().map(str::to_string) else { continue };
            let rel = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
            let ft = e.file_type()?;
            if ft.is_dir() {
                walk(base, &e.path(), &rel, out)?;
            } else if ft.is_file() && is_safe_path(&rel) && !excluded(&rel) {
                let m = e.metadata()?;
                out.push(Found { rel, path: e.path(), size: m.len(), modified: m.modified()? });
            }
        }
        Ok(())
    }
    let mut out = vec![];
    walk(dir, dir, "", &mut out)?;
    out.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(out)
}

/// Codifica una ruta para la URL (`%XX` todo lo que no sea letra, número, `-_.~/`).
pub fn encode_path(p: &str) -> String {
    p.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

pub fn decode_path(p: &str) -> Option<String> {
    let b = p.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let h = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(h, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rutas_seguras() {
        assert!(is_safe_path("Data/World1/EncTerrain1.map"));
        assert!(is_safe_path("Jugar - Lorencia.bat"));
        for malo in ["", "../x", "a/../b", "/etc/passwd", "C:/x", "a\\b", "a//b", "./a", "a/."] {
            assert!(!is_safe_path(malo), "{malo:?}");
        }
    }

    #[test]
    fn lista_ida_y_vuelta() {
        let e = vec![Entry { path: "Jugar - Lorencia.bat".into(), size: 12, sha256: "a".repeat(64) }];
        assert_eq!(parse_manifest(&format_manifest(&e)), Some(e));
        assert_eq!(parse_manifest(&format!("{}\t1\t../x\n", "a".repeat(64))), None);
    }

    #[test]
    fn codificar_rutas() {
        let p = "Jugar - Lorencia.bat";
        assert_eq!(encode_path(p), "Jugar%20-%20Lorencia.bat");
        assert_eq!(decode_path(&encode_path(p)).as_deref(), Some(p));
        assert_eq!(decode_path("%2"), None);
    }

    #[test]
    fn excluidos() {
        assert!(excluded("mu-client.log"));
        assert!(excluded("mu-launcher.txt"));
        assert!(!excluded("mu-client.exe"));
    }
}
