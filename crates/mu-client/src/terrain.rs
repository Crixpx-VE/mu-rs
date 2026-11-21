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
        app.init_resource::<WaterMaterials>()
            .init_resource::<TerrainLights>()
            .add_systems(Startup, spawn_terrain)
            // Las luces se vacían al empezar cada tick; los emisores las rellenan después.
            .add_systems(FixedFirst, clear_lights)
            .add_systems(Update, (move_water, apply_lights));
    }
}

/// Textura del terreno que es agua en todos los mapas (`TileWater01`).
const WATER_TEXTURE: usize = 5;
/// El agua avanza una textura entera cada 20 s (`WaterMove` en `ZzzLodTerrain.cpp`).
const WATER_PERIOD_SECS: f32 = 20.0;

/// Materiales de las casillas de agua, cuya textura se desplaza.
#[derive(Resource, Default)]
struct WaterMaterials(Vec<Handle<StandardMaterial>>);

fn move_water(time: Res<Time>, water: Res<WaterMaterials>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let offset = (time.elapsed_secs() / WATER_PERIOD_SECS).fract();
    for h in &water.0 {
        if let Some(mut m) = materials.get_mut(h) {
            m.uv_transform = bevy::math::Affine2::from_translation(Vec2::new(offset, 0.0));
        }
    }
}

fn spawn_terrain(
    mut commands: Commands,
    args: Res<Args>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut water: ResMut<WaterMaterials>,
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
            let material = materials.add(StandardMaterial {
                base_color_texture: Some(image.clone()),
                unlit: true,
                cull_mode: None,
                alpha_mode: if layer == 1 { AlphaMode::Opaque } else { AlphaMode::Blend },
                depth_bias: if layer == 1 { 0.0 } else { 1.0 },
                ..default()
            });
            if index == WATER_TEXTURE {
                water.0.push(material.clone());
            }
            // Un trozo por bloque de 16×16 casillas: así las luces dinámicas solo rehacen lo que tocan.
            for cy in (0..SIZE - 1).step_by(CHUNK) {
                for cx in (0..SIZE - 1).step_by(CHUNK) {
                    let area = [cx, cy, (cx + CHUNK).min(SIZE - 1), (cy + CHUNK).min(SIZE - 1)];
                    let Some((mesh, chunk)) = build_layer_mesh(&t, index as u8, layer, uv_scale, area) else {
                        continue;
                    };
                    let handle = meshes.add(mesh);
                    commands.spawn((
                        Name::new(format!("Terreno capa {layer} textura {index} ({cx},{cy})")),
                        Mesh3d(handle.clone()),
                        MeshMaterial3d(material.clone()),
                        TerrainChunk { mesh: handle, ..chunk },
                    ));
                }
            }
        }
    }

    commands.insert_resource(LoadedTerrain(t));
}

/// Lado de los trozos del terreno, en casillas.
const CHUNK: usize = 16;

/// Un trozo del terreno, con lo necesario para recalcular su luz.
#[derive(Component)]
pub struct TerrainChunk {
    mesh: Handle<Mesh>,
    /// Esquina (x, y) de cada vértice.
    corners: Vec<[u8; 2]>,
    /// Transparencia de cada vértice (capa 2).
    alpha: Vec<f32>,
    /// Casillas que cubre: [x0, y0, x1, y1].
    area: [usize; 4],
    /// Tenía luces dinámicas en la última actualización (hay que devolverlo a su luz normal).
    lit: bool,
}

/// Una luz que se suma al terreno durante una actualización (`AddTerrainLight`).
#[derive(Clone, Copy)]
pub struct DynamicLight {
    /// Posición en casillas.
    pub pos: Vec2,
    pub color: [f32; 3],
    /// Radio en casillas.
    pub range: f32,
}

/// Luces dinámicas de la actualización actual. Las rellenan los sistemas de `FixedUpdate`
/// (fuegos, eventos...) después de que [`clear_lights`] la vacíe al empezar el tick.
#[derive(Resource, Default)]
pub struct TerrainLights {
    pub lights: Vec<DynamicLight>,
    /// Cambia en cada tick, para saber si hay que recalcular.
    generation: u64,
    applied: u64,
}

impl TerrainLights {
    pub fn add(&mut self, pos: Vec2, color: [f32; 3], range: f32) {
        self.lights.push(DynamicLight { pos, color, range });
    }
}

/// Al empezar cada tick, las luces del anterior desaparecen (como `InitTerrainLight`).
pub fn clear_lights(mut lights: ResMut<TerrainLights>) {
    lights.lights.clear();
    lights.generation += 1;
}

/// Recalcula el color de los trozos que tocan las luces de este tick (o que las tenían antes).
fn apply_lights(
    terrain: Option<Res<LoadedTerrain>>,
    mut lights: ResMut<TerrainLights>,
    mut chunks: Query<&mut TerrainChunk>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Some(t) = terrain else { return };
    if lights.applied == lights.generation {
        return;
    }
    lights.applied = lights.generation;
    for mut c in &mut chunks {
        let [x0, y0, x1, y1] = c.area;
        let touching: Vec<&DynamicLight> = lights
            .lights
            .iter()
            .filter(|l| {
                l.pos.x + l.range >= x0 as f32
                    && l.pos.x - l.range <= x1 as f32
                    && l.pos.y + l.range >= y0 as f32
                    && l.pos.y - l.range <= y1 as f32
            })
            .collect();
        if touching.is_empty() && !c.lit {
            continue;
        }
        c.lit = !touching.is_empty();
        let colors: Vec<[f32; 4]> = c
            .corners
            .iter()
            .zip(&c.alpha)
            .map(|(&[x, y], &a)| {
                let mut l = t.0.light[y as usize * SIZE + x as usize];
                for light in &touching {
                    let d = (light.pos - Vec2::new(x as f32, y as f32)).length();
                    let falloff = (light.range - d) / light.range;
                    if falloff > 0.0 {
                        for k in 0..3 {
                            l[k] = (l[k] + light.color[k] * falloff).max(0.0);
                        }
                    }
                }
                vertex_color(l, a)
            })
            .collect();
        if let Some(mut m) = meshes.get_mut(&c.mesh) {
            m.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
        }
    }
}

/// El original multiplica la luz en espacio sRGB; en lineal equivale a elevarla a 2.2.
fn vertex_color(l: [f32; 3], alpha: f32) -> [f32; 4] {
    let c = |v: f32| v.clamp(0.0, 1.0).powf(2.2);
    [c(l[0]), c(l[1]), c(l[2]), alpha]
}

/// Malla con las casillas de `area` que usan `texture` en la capa indicada.
fn build_layer_mesh(
    t: &Terrain,
    texture: u8,
    layer: u8,
    uv_scale: Vec2,
    area: [usize; 4],
) -> Option<(Mesh, TerrainChunk)> {
    let m = &t.mapping;
    let mut pos = Vec::new();
    let mut uv = Vec::new();
    let mut color = Vec::new();
    let mut idx: Vec<u32> = Vec::new();
    let mut corners_out = Vec::new();
    let mut alpha_out = Vec::new();

    for y in area[1]..area[3] {
        for x in area[0]..area[2] {
            let i = y * SIZE + x;
            // `RenderTerrainTile`: las casillas `TW_NOGROUND` no se dibujan.
            if t.attribute(x, y) & terrain::attr::NO_GROUND != 0 {
                continue;
            }
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
                let a = if layer == 1 { 1.0 } else { alpha[k] };
                color.push(vertex_color(t.light[cy * SIZE + cx], a));
                corners_out.push([cx as u8, cy as u8]);
                alpha_out.push(a);
            }
            idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }

    if idx.is_empty() {
        return None;
    }
    let n = pos.len();
    let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; n])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, color)
        .with_inserted_indices(Indices::U32(idx));
    let chunk = TerrainChunk { mesh: Handle::default(), corners: corners_out, alpha: alpha_out, area, lit: false };
    Some((mesh, chunk))
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
