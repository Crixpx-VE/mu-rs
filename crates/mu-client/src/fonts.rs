//! Font for the whole UI: Noto Sans (OFL license, `Data/Fonts/NotoSansKR-Regular.ttf`), more
//! modern and legible than the original Gulim, with accents, ñ and Korean.
//!
//! Replaces Bevy's default font: every text that does not ask for another uses it. It is drawn
//! antialiased at the real screen size (not on the stretched virtual screen), so it does not look
//! pixelated. If the file is missing, Bevy's font stays.

use bevy::asset::AssetId;
use bevy::prelude::*;

use crate::Args;

pub struct FontsPlugin;

impl Plugin for FontsPlugin {
    fn build(&self, app: &mut App) {
        // Before the first frame: the default font is registered only once.
        let Some(dir) = app.world().get_resource::<Args>().map(|a| a.data.join("Fonts")) else { return };
        let Some(bytes) = mu_data::fs::read(&dir, "NotoSansKR-Regular.ttf").ok() else {
            warn!("sin {}: se usa la letra de Bevy", dir.join("NotoSansKR-Regular.ttf").display());
            return;
        };
        let mut fonts = app.world_mut().resource_mut::<Assets<Font>>();
        if fonts.insert(AssetId::default(), Font::from_bytes(bytes)).is_err() {
            warn!("no se pudo poner la letra de la interfaz");
        }
    }
}
