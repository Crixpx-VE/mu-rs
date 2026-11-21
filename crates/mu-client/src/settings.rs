//! Resolución de la ventana, como en el MU original: tamaños fijos (4:3 y 5:4), sin estirar
//! la ventana a mano, para que la interfaz (pensada para 640×480) se vea siempre bien.
//!
//! - Se guarda en `mu-launcher.txt` (junto al ejecutable) como `resolucion=1024x768`. Ese archivo
//!   es de cada jugador: las actualizaciones no lo tocan.
//! - `--resolucion 1280x1024` la cambia solo para esa vez.
//! - Como en el original (que la elige en su configurador), no hay tecla para cambiarla en el juego.

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

/// Valor de `clave=` en `mu-launcher.txt` (la clave sin el `=`).
pub fn saved_value(key: &str) -> Option<String> {
    let text = std::fs::read_to_string(config_path()?).ok()?;
    let prefix = format!("{key}=");
    text.lines().find_map(|l| l.trim().strip_prefix(&prefix).map(|v| v.trim().to_string()))
}

/// Guarda `clave=valor` en `mu-launcher.txt`, sin tocar el resto de líneas.
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

/// La resolución guardada, si la hay.
pub fn saved_resolution() -> Option<(u32, u32)> {
    let text = std::fs::read_to_string(config_path()?).ok()?;
    text.lines().find_map(|l| l.trim().strip_prefix(KEY)).and_then(parse_resolution)
}

/// Ventana de `w × h` píxeles de verdad (sin el escalado de Windows), de tamaño fijo.
pub fn window_resolution((w, h): (u32, u32)) -> WindowResolution {
    WindowResolution::new(w, h).with_scale_factor_override(1.0)
}

