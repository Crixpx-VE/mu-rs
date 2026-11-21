//! Acceso a archivos sin distinguir mayúsculas/minúsculas.
//!
//! Los datos de MU mezclan `.OZJ`, `.ozj`, `World1`, `world55`... En Windows da igual,
//! pero en Linux/Android/iOS no, así que buscamos el nombre ignorando mayúsculas.

use std::path::{Path, PathBuf};

use crate::{Error, Result};

/// Devuelve la ruta real de `dir/name` aunque las mayúsculas no coincidan.
pub fn find(dir: &Path, name: &str) -> Option<PathBuf> {
    let exact = dir.join(name);
    if exact.exists() {
        return Some(exact);
    }
    let wanted = name.to_ascii_lowercase();
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .find(|e| e.file_name().to_string_lossy().to_ascii_lowercase() == wanted)
        .map(|e| e.path())
}

/// Lee `dir/name` completo, ignorando mayúsculas en el nombre.
pub fn read(dir: &Path, name: &str) -> Result<Vec<u8>> {
    let path = find(dir, name).ok_or_else(|| Error::NotFound(dir.join(name)))?;
    std::fs::read(&path).map_err(|e| Error::Io(path, e))
}
