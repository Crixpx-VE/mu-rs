//! Convierte el terreno de `mu-data` en mallas de Bevy.
//!
//! Igual que el cliente original, se dibuja en dos pasadas:
//! 1. Capa 1: cada casilla con su textura base (opaca).
//! 2. Capa 2: encima, la segunda textura con la transparencia de cada vértice.
//!
//! Se agrupan las casillas por textura: una malla por textura y capa.
//!
//! Ejes: MU usa Z hacia arriba y Bevy usa Y hacia arriba, así que
//! MU (x, y, z) → Bevy (x, z, -y). Una casilla mide 1 unidad (100 en MU).

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use mu_data::terrain::{self, NO_TEXTURE, SIZE, Terrain};
use mu_data::texture::Texture;

use crate::Args;

/// Unidades de MU por unidad de Bevy (una casilla = 100 en MU).
pub const MU_SCALE: f32 = 100.0;

/// El terreno cargado, para que otros sistemas (cámara, personajes) consulten alturas.
#[derive(Resource)]
pub struct LoadedTerrain(pub Terrain);

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_terrain);
    }
}

fn spawn_terrain(
    mut commands: Commands,
    args: Res<Args>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let t = match terrain::load(&args.data, args.world) {
        Ok(t) => t,
        Err(e) => {
            error!("no se pudo cargar World{}: {e}", args.world);
            return;
        }
    };
    info!("World{} cargado desde {}", args.world, t.dir.display());

    for (index, tex) in t.textures.iter().enumerate() {
        let Some(tex) = tex else { continue };
        let image = images.add(to_bevy_image(tex));
        let uv_scale = Vec2::new(64.0 / tex.width as f32, 64.0 / tex.height as f32);

        for layer in [1, 2] {
            let Some(mesh) = build_layer_mesh(&t, index as u8, layer, uv_scale) else { continue };
            let material = materials.add(StandardMaterial {
                base_color_texture: Some(image.clone()),
                unlit: true,
                cull_mode: None,
                alpha_mode: if layer == 1 { AlphaMode::Opaque } else { AlphaMode::Blend },
                depth_bias: if layer == 1 { 0.0 } else { 1.0 },
                ..default()
            });
            commands.spawn((
                Name::new(format!("Terreno capa {layer} textura {index}")),
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(material),
            ));
        }
    }

    commands.insert_resource(LoadedTerrain(t));
}

/// Malla con todas las casillas que usan `texture` en la capa indicada.
fn build_layer_mesh(t: &Terrain, texture: u8, layer: u8, uv_scale: Vec2) -> Option<Mesh> {
    let m = &t.mapping;
    let mut pos = Vec::new();
    let mut uv = Vec::new();
    let mut color = Vec::new();
    let mut idx: Vec<u32> = Vec::new();

    for y in 0..SIZE - 1 {
        for x in 0..SIZE - 1 {
            let i = y * SIZE + x;
            let corners = [(x, y), (x + 1, y), (x + 1, y + 1), (x, y + 1)];
            let alpha = corners.map(|(cx, cy)| m.alpha[cy * SIZE + cx] as f32 / 255.0);

            let used = if layer == 1 {
                m.layer1[i] == texture
            } else {
                m.layer2[i] == texture && m.layer2[i] != NO_TEXTURE && alpha.iter().any(|&a| a > 0.0)
            };
            if !used {
                continue;
            }

            let base = pos.len() as u32;
            for (k, &(cx, cy)) in corners.iter().enumerate() {
                let h = t.height(cx, cy) / MU_SCALE;
                pos.push([cx as f32, h, -(cy as f32)]);
                uv.push([cx as f32 * uv_scale.x, cy as f32 * uv_scale.y]);
                let l = t.light[cy * SIZE + cx];
                let a = if layer == 1 { 1.0 } else { alpha[k] };
                // El original multiplica en espacio sRGB; en lineal equivale a elevar la luz a 2.2.
                color.push([l[0].powf(2.2), l[1].powf(2.2), l[2].powf(2.2), a]);
            }
            idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }

    if idx.is_empty() {
        return None;
    }
    let n = pos.len();
    Some(
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; n])
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, color)
            .with_inserted_indices(Indices::U32(idx)),
    )
}

/// Textura de `mu-data` → imagen de Bevy que se repite, con mipmaps
/// (sin mipmaps el suelo lejano "parpadea").
pub fn to_bevy_image(tex: &Texture) -> Image {
    let (data, levels) = build_mipmaps(tex);
    let mut image = Image::new(
        Extent3d { width: tex.width, height: tex.height, depth_or_array_layers: 1 },
        TextureDimension::D2,
        tex.rgba.clone(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    // `Image::new` solo acepta el nivel 0; los mipmaps van todos seguidos detrás.
    image.data = Some(data);
    image.texture_descriptor.mip_level_count = levels;
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 16,
        ..default()
    });
    image
}

/// Todos los niveles de mipmap seguidos, reduciendo a la mitad con media de 2x2.
fn build_mipmaps(tex: &Texture) -> (Vec<u8>, u32) {
    let mut data = tex.rgba.clone();
    let (mut w, mut h) = (tex.width as usize, tex.height as usize);
    let mut prev = tex.rgba.clone();
    let mut levels = 1;
    while w > 1 || h > 1 {
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let mut next = vec![0u8; nw * nh * 4];
        for y in 0..nh {
            for x in 0..nw {
                for c in 0..4 {
                    let s = |sx: usize, sy: usize| prev[((sy.min(h - 1)) * w + sx.min(w - 1)) * 4 + c] as u32;
                    let sum = s(2 * x, 2 * y) + s(2 * x + 1, 2 * y) + s(2 * x, 2 * y + 1) + s(2 * x + 1, 2 * y + 1);
                    next[(y * nw + x) * 4 + c] = (sum / 4) as u8;
                }
            }
        }
        data.extend_from_slice(&next);
        prev = next;
        (w, h) = (nw, nh);
        levels += 1;
    }
    (data, levels)
}
