//! Letra de toda la interfaz: Noto Sans (licencia OFL, `Data/Fonts/NotoSansKR-Regular.ttf`), más
//! moderna y legible que la Gulim del original, con tildes, ñ y coreano.
//!
//! Sustituye a la fuente por defecto de Bevy: todos los textos que no piden otra la usan. Se
//! dibuja suavizada y al tamaño real de la pantalla (no en la pantalla virtual estirada), así que
//! no se ve pixelada. Si falta el archivo, queda la de Bevy.

use bevy::asset::AssetId;
use bevy::prelude::*;

use crate::Args;

pub struct FontsPlugin;

impl Plugin for FontsPlugin {
    fn build(&self, app: &mut App) {
        // Antes del primer fotograma: la fuente por defecto se registra una sola vez.
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
