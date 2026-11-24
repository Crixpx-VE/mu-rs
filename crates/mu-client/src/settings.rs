//! Window resolution, as in the original MU: fixed sizes (4:3 and 5:4), no manual window
//! resizing, so the UI (designed for 640×480) always looks right.
//!
//! - Saved in `mu-launcher.txt` (next to the executable) as `resolucion=1024x768`. That file
//!   belongs to each player: updates do not touch it.
//! - `--resolucion 1280x1024` changes it for that run only.
//! - Like the original (which picks it in its configurator), there is no in-game key to change it.

use std::path::PathBuf;

use bevy::prelude::*;
use bevy::window::WindowResolution;

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, _app: &mut App) {}
}

pub const DEFAULT_RESOLUTION: (u32, u32) = (1024, 768);

const CONFIG_FILE: &str = "mu-launcher.txt";
const KEY: &str = "resolucion=";

fn config_path() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.join(CONFIG_FILE))
}

/// Value of `key=` in `mu-launcher.txt` (the key without the `=`).
pub fn saved_value(key: &str) -> Option<String> {
    let text = std::fs::read_to_string(config_path()?).ok()?;
    let prefix = format!("{key}=");
    text.lines().find_map(|l| l.trim().strip_prefix(&prefix).map(|v| v.trim().to_string()))
}

/// Saves `key=value` in `mu-launcher.txt`, leaving the other lines alone.
pub fn save_value(key: &str, value: &str) {
    let Some(path) = config_path() else { return };
    let prefix = format!("{key}=");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let mut lines: Vec<String> = text.lines().filter(|l| !l.trim().starts_with(&prefix)).map(str::to_string).collect();
    lines.push(format!("{prefix}{value}"));
    if let Err(e) = std::fs::write(&path, lines.join("\r\n") + "\r\n") {
        warn!("no se pudo guardar {key} en {}: {e}", path.display());
    }
}

/// `1024x768` → (1024, 768).
pub fn parse_resolution(s: &str) -> Option<(u32, u32)> {
    let (w, h) = s.trim().split_once(['x', 'X', '*'])?;
    let (w, h) = (w.trim().parse().ok()?, h.trim().parse().ok()?);
    ((320..=7680).contains(&w) && (240..=4320).contains(&h)).then_some((w, h))
}

/// The saved resolution, if any.
pub fn saved_resolution() -> Option<(u32, u32)> {
    let text = std::fs::read_to_string(config_path()?).ok()?;
    text.lines().find_map(|l| l.trim().strip_prefix(KEY)).and_then(parse_resolution)
}

/// Fixed-size window of `w × h` real pixels (without Windows scaling).
pub fn window_resolution((w, h): (u32, u32)) -> WindowResolution {
    WindowResolution::new(w, h).with_scale_factor_override(1.0)
}

