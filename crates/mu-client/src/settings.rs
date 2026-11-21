//! Resolución de la ventana, como en el MU original: tamaños fijos (4:3 y 5:4), sin estirar
//! la ventana a mano, para que la interfaz (pensada para 640×480) se vea siempre bien.
//!
//! - Se guarda en `mu-launcher.txt` (junto al ejecutable) como `resolucion=1024x768`. Ese archivo
//!   es de cada jugador: las actualizaciones no lo tocan.
//! - `--resolucion 1280x1024` la cambia solo para esa vez.
//! - `F10` pasa a la siguiente resolución de la lista y la guarda.

use std::path::PathBuf;

use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowResolution};

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, cycle_resolution);
    }
}

/// Resoluciones disponibles (las del MU original y alguna más grande).
pub const RESOLUTIONS: [(u32, u32); 6] = [(800, 600), (1024, 768), (1152, 864), (1280, 960), (1280, 1024), (1600, 1200)];
pub const DEFAULT_RESOLUTION: (u32, u32) = (1024, 768);

const CONFIG_FILE: &str = "mu-launcher.txt";
const KEY: &str = "resolucion=";

fn config_path() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.join(CONFIG_FILE))
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

/// Guarda la resolución en `mu-launcher.txt`, sin tocar el resto de líneas.
pub fn save_resolution((w, h): (u32, u32)) {
    let Some(path) = config_path() else { return };
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let mut lines: Vec<String> = text.lines().filter(|l| !l.trim().starts_with(KEY)).map(str::to_string).collect();
    lines.push(format!("{KEY}{w}x{h}"));
    if let Err(e) = std::fs::write(&path, lines.join("\r\n") + "\r\n") {
        warn!("no se pudo guardar la resolución en {}: {e}", path.display());
    }
}

/// Ventana de `w × h` píxeles de verdad (sin el escalado de Windows), de tamaño fijo.
pub fn window_resolution((w, h): (u32, u32)) -> WindowResolution {
    WindowResolution::new(w, h).with_scale_factor_override(1.0)
}

fn cycle_resolution(keys: Res<ButtonInput<KeyCode>>, mut windows: Query<&mut Window, With<PrimaryWindow>>) {
    if !keys.just_pressed(KeyCode::F10) {
        return;
    }
    let Ok(mut window) = windows.single_mut() else { return };
    let current = (window.resolution.physical_width(), window.resolution.physical_height());
    let next = RESOLUTIONS
        .iter()
        .position(|&r| r == current)
        .map_or(DEFAULT_RESOLUTION, |i| RESOLUTIONS[(i + 1) % RESOLUTIONS.len()]);
    window.resolution = window_resolution(next);
    save_resolution(next);
    info!("resolución: {}x{}", next.0, next.1);
}
