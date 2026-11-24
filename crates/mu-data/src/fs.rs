//! Case-insensitive file access.
//!
//! MU data mixes `.OZJ`, `.ozj`, `World1`, `world55`... Windows does not care, but
//! Linux/Android/iOS do, so names are matched ignoring case.

use std::path::{Path, PathBuf};

use crate::{Error, Result};

/// Returns the real path of `dir/name` even if the case does not match.
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

/// Reads all of `dir/name`, ignoring case in the name.
pub fn read(dir: &Path, name: &str) -> Result<Vec<u8>> {
    let path = find(dir, name).ok_or_else(|| Error::NotFound(dir.join(name)))?;
    std::fs::read(&path).map_err(|e| Error::Io(path, e))
}
