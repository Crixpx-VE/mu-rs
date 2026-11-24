//! Client updates.
//!
//! The server (`mu-update-server`) publishes a list with the SHA-256 hash of every file
//! in the client folder. The launcher (`mu-launcher`) compares that list with what it has,
//! downloads only what changed, verifies the hash of each download and starts the game.
//!
//! Protocol (public HTTP, like the launcher of any MU server):
//!
//! ```text
//! GET /manifest        → one line per file: <sha256>\t<size>\t<path with '/'>
//! GET /file/<path>     → the file (path encoded with %XX); only if it is in the list
//! GET /manifest2       → same, with the compressed size: <sha256>\t<size>\t<packed>\t<path>
//! GET /z/<sha256>      → the file compressed with zstd (if <packed> is not 0)
//! ```
//!
//! Old launchers still use `/manifest` and `/file`.
//!
//! The published folder must contain only the game: everything inside it can be downloaded.

use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use sha2::{Digest, Sha256};

/// Default port of the update server.
pub const DEFAULT_PORT: u16 = 44480;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Relative path with `/` (e.g. `Data/World1/EncTerrain1.map`).
    pub path: String,
    pub size: u64,
    pub sha256: String,
    /// Size compressed with zstd (`/z/<sha256>`); 0 if not served compressed.
    pub packed: u64,
}

/// Is it a hexadecimal SHA-256 hash?
pub fn is_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit())
}

/// SHA-256 hash of a file, in hexadecimal.
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

/// Reads the list. Rejects the whole list if any path is unsafe.
pub fn parse_manifest(text: &str) -> Option<Vec<Entry>> {
    text.lines()
        .filter(|l| !l.is_empty())
        .map(|l| {
            let mut it = l.splitn(3, '\t');
            let sha256 = it.next()?.to_string();
            let size = it.next()?.parse().ok()?;
            let path = it.next()?.to_string();
            (is_sha256(&sha256) && is_safe_path(&path)).then_some(Entry { path, size, sha256, packed: 0 })
        })
        .collect()
}

/// List with the compressed size (`/manifest2`).
pub fn format_manifest2(entries: &[Entry]) -> String {
    entries.iter().map(|e| format!("{}\t{}\t{}\t{}\n", e.sha256, e.size, e.packed, e.path)).collect()
}

pub fn parse_manifest2(text: &str) -> Option<Vec<Entry>> {
    text.lines()
        .filter(|l| !l.is_empty())
        .map(|l| {
            let mut it = l.splitn(4, '\t');
            let sha256 = it.next()?.to_string();
            let size = it.next()?.parse().ok()?;
            let packed = it.next()?.parse().ok()?;
            let path = it.next()?.to_string();
            (is_sha256(&sha256) && is_safe_path(&path)).then_some(Entry { path, size, sha256, packed })
        })
        .collect()
}

/// Relative path with no tricks: no `..`, absolute paths, drives (`C:`), `\` or empty parts.
/// That way the server never serves files outside its folder and the launcher never writes outside its own.
pub fn is_safe_path(p: &str) -> bool {
    !p.is_empty()
        && p.len() < 1024
        && !p.contains(['\\', ':', '\0'])
        && p.split('/').all(|part| !part.is_empty() && part != "." && part != "..")
}

pub fn to_local(base: &Path, rel: &str) -> PathBuf {
    rel.split('/').fold(base.to_path_buf(), |p, part| p.join(part))
}

/// Files that are never published or touched: logs, partial downloads and the local config.
pub fn excluded(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel).to_ascii_lowercase();
    name.ends_with(".log")
        || name.ends_with(".part")
        || name.ends_with(".tmp")
        || name.ends_with(".old")
        || name == "mu-launcher.txt"
        || name == ".mu-launcher-state"
}

/// A file found while walking a folder.
pub struct Found {
    pub rel: String,
    pub path: PathBuf,
    pub size: u64,
    pub modified: SystemTime,
}

/// All files in `dir` (recursive), except the excluded ones.
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

/// Encodes a path for the URL (`%XX` for everything except letters, digits and `-_.~/`).
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
        let e = vec![Entry { path: "Jugar - Lorencia.bat".into(), size: 12, sha256: "a".repeat(64), packed: 0 }];
        assert_eq!(parse_manifest(&format_manifest(&e)), Some(e));
        assert_eq!(parse_manifest(&format!("{}\t1\t../x\n", "a".repeat(64))), None);
    }

    #[test]
    fn lista2_ida_y_vuelta() {
        let e = vec![
            Entry { path: "Data/a b.bmd".into(), size: 1200, sha256: "b".repeat(64), packed: 800 },
            Entry { path: "x.mp3".into(), size: 5, sha256: "c".repeat(64), packed: 0 },
        ];
        assert_eq!(parse_manifest2(&format_manifest2(&e)), Some(e));
        assert_eq!(parse_manifest2(&format!("{}\t1\t1\t../x\n", "a".repeat(64))), None);
        assert_eq!(parse_manifest2(&format!("{}\t1\tx\ty\n", "a".repeat(64))), None);
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
