//! Character files read while playing, parsed once for the whole run.
//!
//! Every character that appears (other players, guards, the hero after a map change or a gear
//! change) used to read and parse `player.bmd` (3.3 MB, 284 actions), its pieces and weapons, and
//! decode and upload each texture again: tens of milliseconds on the main thread each time, the
//! hitch when someone comes into view. The original keeps them loaded too (`Models[]` and
//! `Bitmaps` are opened once, ZzzOpenData.cpp). `Data` does not change while the game runs, so
//! nothing here is ever invalidated; a texture whose image was dropped is just loaded again.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use bevy::prelude::*;
use mu_data::bmd::{self, Bmd};
use mu_data::objects::render::Bitmap;
use mu_data::texture::{self, AlphaKind};
use mu_data::fs;

static MODELS: LazyLock<Mutex<HashMap<PathBuf, Arc<Bmd>>>> = LazyLock::new(Default::default);
type Texture = Option<(Handle<Image>, AlphaKind)>;
static TEXTURES: LazyLock<Mutex<HashMap<PathBuf, Texture>>> = LazyLock::new(Default::default);
static BITMAPS: LazyLock<Mutex<HashMap<Bitmap, Option<Handle<Image>>>>> = LazyLock::new(Default::default);

/// `fs::read` + `bmd::read` of `dir/name`, once (failures are not kept, like before).
pub fn bmd(dir: &Path, name: &str) -> mu_data::Result<Arc<Bmd>> {
    let key = dir.join(name);
    if let Some(m) = MODELS.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
        return Ok(m.clone());
    }
    let model = Arc::new(bmd::read(&fs::read(dir, name)?)?);
    MODELS.lock().unwrap_or_else(|e| e.into_inner()).insert(key, model.clone());
    Ok(model)
}

/// The texture `dir/file` uploaded as an image, with its alpha kind (`None` if missing).
pub fn texture(dir: &Path, file: &str, images: &mut Assets<Image>) -> Texture {
    let key = dir.join(file);
    let mut map = TEXTURES.lock().unwrap_or_else(|e| e.into_inner());
    match map.get(&key) {
        Some(Some((h, kind))) if images.contains(h.id()) => return Some((h.clone(), *kind)),
        Some(None) => return None,
        _ => {}
    }
    let loaded = texture::load(dir, file).ok().map(|t| (images.add(crate::terrain::to_bevy_image(&t)), t.alpha_kind()));
    map.insert(key, loaded.clone());
    loaded
}

/// `objects::load_bitmap` (a global `BITMAP_*` texture), once.
pub fn bitmap(data: &Path, b: Bitmap, images: &mut Assets<Image>) -> Option<Handle<Image>> {
    let mut map = BITMAPS.lock().unwrap_or_else(|e| e.into_inner());
    match map.get(&b) {
        Some(Some(h)) if images.contains(h.id()) => return Some(h.clone()),
        Some(None) => return None,
        _ => {}
    }
    let loaded = crate::objects::load_bitmap(data, b, images);
    map.insert(b, loaded.clone());
    loaded
}
