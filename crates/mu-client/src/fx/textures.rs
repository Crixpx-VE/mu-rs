//! `BITMAP_*` effect textures: loaded on first use, from the table generated from
//! `ZzzOpenData.cpp` (`gen/bitmaps.rs`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use mu_data::texture::{self, Texture};

use super::core::TexInfo;
use super::tables::bitmaps::BITMAPS;

/// Textures the original loads when entering a map (`MapManager.cpp`), not in `OpenBasicData`,
/// and are therefore missing from the generated table: the rain splashes (MapManager.cpp:1574-1577).
const MAP_BITMAPS: &[(i32, &str, bool)] = &[
    (super::core::BITMAP_RAIN_CIRCLE, "World1/rain02.tga", false),
    (super::core::BITMAP_RAIN_CIRCLE + 1, "World10/rain03.tga", false),
    // Barracks and Refuge of Balgass (MapManager.cpp:556, :581): the fire column flame.
    (super::core::BITMAP_FIRE_RED, "Effect/firered.jpg", false),
];

#[derive(Resource)]
pub struct FxTextures {
    data: PathBuf,
    table: HashMap<i32, (&'static str, bool)>,
    loaded: HashMap<i32, Option<(Handle<Image>, TexInfo)>>,
}

impl FxTextures {
    pub fn new(data: PathBuf) -> Self {
        let table = BITMAPS.iter().chain(MAP_BITMAPS).map(|&(id, path, rep)| (id, (path, rep))).collect();
        Self { data, table, loaded: HashMap::new() }
    }

    /// The texture `id` (loading it if needed). `None` if it does not exist or cannot be read.
    pub fn get(&mut self, id: i32, images: &mut Assets<Image>) -> Option<(Handle<Image>, TexInfo)> {
        if let Some(t) = self.loaded.get(&id) {
            return t.clone();
        }
        let result = self.table.get(&id).and_then(|&(path, repeat)| {
            let p = Path::new(path);
            let dir = self.data.join(p.parent().unwrap_or(Path::new("")));
            let name = mu_data::bmd::texture_file_name(p.file_name()?.to_str()?);
            match texture::load(&dir, &name) {
                Ok(t) => {
                    let info = TexInfo {
                        width: t.width as f32,
                        height: t.height as f32,
                        components: if name.to_ascii_lowercase().ends_with(".ozt") { 4 } else { 3 },
                    };
                    Some((images.add(fx_image(&t, repeat)), info))
                }
                Err(e) => {
                    warn!("efecto: falta la textura {id} ({path}): {e}");
                    None
                }
            }
        });
        self.loaded.insert(id, result.clone());
        result
    }

    /// Only the size (loads the texture if needed).
    pub fn info(&mut self, id: i32, images: &mut Assets<Image>) -> Option<TexInfo> {
        self.get(id, images).map(|t| t.1)
    }
}

/// Bevy image for an effect: with mipmaps, and repeated or clamped to the edge as in the original.
pub fn fx_image(tex: &Texture, repeat: bool) -> Image {
    let mut image = crate::terrain::to_bevy_image(tex);
    let mode = if repeat { ImageAddressMode::Repeat } else { ImageAddressMode::ClampToEdge };
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: mode,
        address_mode_v: mode,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..default()
    });
    image
}

/// 1×1 white image (for untextured meshes).
pub fn white_image() -> Image {
    Image::new(
        Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        TextureDimension::D2,
        vec![255, 255, 255, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}
