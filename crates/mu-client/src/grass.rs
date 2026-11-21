//! Hierba del terreno que se mece con el viento (`RenderTerrainFace` con `TERRAIN_MAP_GRASS`
//! en `ZzzLodTerrain.cpp`).
//!
//! En cada casilla de hierba "pura" (textura base 0, 1 o 2 y sin mezcla con la segunda capa)
//! el original levanta un plano vertical en diagonal, de la esquina (x, y) a la (x+1, y+1),
//! con la textura `TileGrass0N.tga`:
//!
//! - alto = alto de la textura en píxeles × 2 (unidades de MU), y la parte de arriba
//!   desplazada 50 unidades hacia -X;
//! - U = x · 0.25 + un desfase al azar por fila (0, 0.25, 0.5 o 0.75); V de 0 (arriba) a 1;
//! - viento: los vértices de arriba se mueven en Y `sin(2·t + 5·x) · 10` (t en segundos).
//!
//! El viento depende del tiempo real, así que va igual a cualquier fps. Los vértices se
//! recalculan en CPU cada fotograma (solo cambia la Y de MU de los de arriba).

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use mu_data::terrain::{SIZE, Terrain};
use mu_data::texture;

use crate::Args;
use crate::terrain::{LoadedTerrain, MU_SCALE, to_bevy_image};

pub struct GrassPlugin;

impl Plugin for GrassPlugin {
    fn build(&self, app: &mut App) {
        // PostStartup: el terreno se carga en Startup.
        app.add_systems(PostStartup, spawn_grass).add_systems(Update, wave_grass);
    }
}

/// Texturas de terreno que llevan hierba encima (`BITMAP_MAPGRASS + capa1`, solo hay 3).
const GRASS_TEXTURES: u8 = 3;
/// Amplitud del viento en unidades de MU (`WindScale`).
const WIND_SCALE: f32 = 10.0;
/// Velocidad del viento en radianes por segundo (`WorldTime · 0.002`, con el tiempo en ms).
const WIND_SPEED: f32 = 2.0;

#[derive(Component)]
struct GrassMesh {
    mesh: Handle<Mesh>,
    /// Posiciones sin viento (Bevy).
    base: Vec<[f32; 3]>,
    /// Para cada vértice de arriba: (índice del vértice, x de la casilla cuyo viento usa).
    tops: Vec<(u32, u16)>,
    /// Multiplicador de x en la fase del viento (5; Tarkan usa 50).
    wave_x: f32,
}

fn spawn_grass(
    mut commands: Commands,
    args: Res<Args>,
    terrain: Option<Res<LoadedTerrain>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(terrain) = terrain else { return };
    let t = &terrain.0;
    // Atlans (bajo el agua) no tiene hierba.
    if args.world == 8 {
        return;
    }
    let wave_x = if args.world == 9 { 50.0 } else { 5.0 };
    let row_offset = row_offsets();

    for index in 0..GRASS_TEXTURES {
        let name = format!("TileGrass{:02}.ozt", index + 1);
        let Ok(tex) = texture::load(&t.dir, &name) else { continue };
        let height = tex.height as f32 * 2.0;
        let Some((mesh, base, tops)) = build_grass(t, index, height, &row_offset) else { continue };
        let handle = meshes.add(mesh);
        let material = materials.add(StandardMaterial {
            base_color_texture: Some(images.add(to_bevy_image(&tex))),
            unlit: true,
            cull_mode: None,
            // Prueba de alfa del original (`AlphaRef` 0.25).
            alpha_mode: AlphaMode::Mask(0.25),
            ..default()
        });
        info!("hierba {name}: {} matas", base.len() / 4);
        commands.spawn((
            Name::new(format!("Hierba {name}")),
            Mesh3d(handle.clone()),
            MeshMaterial3d(material),
            GrassMesh { mesh: handle, base, tops, wave_x },
        ));
    }
}

/// `TerrainGrassTexture`: desfase de U al azar por fila (0, 0.25, 0.5, 0.75).
fn row_offsets() -> Vec<f32> {
    let mut s: u32 = 0x1234_5678;
    (0..SIZE)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            (s % 4) as f32 / 4.0
        })
        .collect()
}

type GrassData = (Mesh, Vec<[f32; 3]>, Vec<(u32, u16)>);

fn build_grass(t: &Terrain, texture: u8, height: f32, row_offset: &[f32]) -> Option<GrassData> {
    let m = &t.mapping;
    let mut pos = Vec::new();
    let mut uv = Vec::new();
    let mut color = Vec::new();
    let mut idx: Vec<u32> = Vec::new();
    let mut tops = Vec::new();
    // MU (x, y, z) → Bevy (x, z, -y), 100 unidades por casilla.
    let to_bevy = |x: f32, y: f32, z: f32| [x / MU_SCALE, z / MU_SCALE, -y / MU_SCALE];
    let light = |x: usize, y: usize| {
        let l = t.light[y * SIZE + x];
        [l[0].powf(2.2), l[1].powf(2.2), l[2].powf(2.2), 1.0]
    };

    for y in 0..SIZE - 1 {
        for x in 0..SIZE - 1 {
            if m.layer1[y * SIZE + x] != texture {
                continue;
            }
            // Solo casillas sin mezcla con la segunda capa.
            let corners = [(x, y), (x + 1, y), (x + 1, y + 1), (x, y + 1)];
            if corners.iter().any(|&(cx, cy)| m.alpha[cy * SIZE + cx] > 0) {
                continue;
            }
            let (xf, yf) = (x as f32 * MU_SCALE, y as f32 * MU_SCALE);
            let h0 = t.height(x, y);
            let h2 = t.height(x + 1, y + 1);
            let su = x as f32 * 0.25 + row_offset[y];

            let base = pos.len() as u32;
            // 0: arriba en (x, y)   1: arriba en (x+1, y+1)   2: abajo en (x+1, y+1)   3: abajo en (x, y)
            pos.push(to_bevy(xf - 50.0, yf, h0 + height));
            pos.push(to_bevy(xf + MU_SCALE - 50.0, yf + MU_SCALE, h2 + height));
            pos.push(to_bevy(xf + MU_SCALE, yf + MU_SCALE, h2));
            pos.push(to_bevy(xf, yf, h0));
            uv.extend_from_slice(&[[su, 0.0], [su + 0.25, 0.0], [su + 0.25, 1.0], [su, 1.0]]);
            // Misma luz por vértice que el original (esquinas de la casilla en orden).
            color.extend_from_slice(&[light(x, y), light(x + 1, y), light(x + 1, y + 1), light(x, y + 1)]);
            idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            // Viento de la casilla x para el vértice 0 y de la x+1 para el 1.
            tops.push((base, x as u16));
            tops.push((base + 1, x as u16 + 1));
        }
    }
    if idx.is_empty() {
        return None;
    }
    let n = pos.len();
    let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos.clone())
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; n])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, color)
        .with_inserted_indices(Indices::U32(idx));
    Some((mesh, pos, tops))
}

fn wave_grass(time: Res<Time>, grass: Query<&GrassMesh>, mut meshes: ResMut<Assets<Mesh>>) {
    let t = time.elapsed_secs();
    for g in &grass {
        let wind: Vec<f32> =
            (0..SIZE).map(|x| (t * WIND_SPEED + x as f32 * g.wave_x).sin() * WIND_SCALE / MU_SCALE).collect();
        let mut pos = g.base.clone();
        for &(v, x) in &g.tops {
            // +Y de MU es -Z en Bevy.
            pos[v as usize][2] -= wind[x as usize];
        }
        if let Some(mut m) = meshes.get_mut(&g.mesh) {
            m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
        }
    }
}
