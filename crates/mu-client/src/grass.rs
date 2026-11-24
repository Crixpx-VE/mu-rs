//! Terrain grass swaying in the wind (`RenderTerrainFace` with `TERRAIN_MAP_GRASS`
//! in `ZzzLodTerrain.cpp`, 1566-1633).
//!
//! On every "pure" grass tile (base texture 0, 1 or 2 and no blend with the second layer)
//! the original raises a diagonal vertical quad, from corner (x, y) to (x+1, y+1),
//! with the `TileGrass0N.tga` texture:
//!
//! - height = texture height in pixels × 2 (MU units), with the top edge
//!   shifted 50 units toward -X;
//! - U = x · 0.25 + a random per-row offset (0, 0.25, 0.5 or 0.75); V from 0 (top) to 1;
//! - wind: the top vertices move in Y with `TerrainGrassWind` ([`Wind`]).
//!
//! No grass in Atlans or Doppelganger 3 (`RenderTerrain`, 2625), in Chaos Castle or Valley of
//! Loren (`TerrainGrassEnable`, 333-342), in Blood Castle (1573) or in Icarus (no terrain). In Vulcanus
//! and Doppelganger 2 the first one is `TileGrass01_R.jpg` and all grass is additive (`EnableAlphaBlend`,
//! MapManager.cpp:1541-1545 and ZzzLodTerrain.cpp:1584-1585).
//!
//! Only the grass on tiles the ground draws is visible ([`crate::terrain::TerrainCull`]). The wind
//! depends on real time, so it runs the same at any fps. The vertex shader applies it
//! (`grass_wind.wesl`): the mesh does not change, only the per-frame wind numbers. On
//! additive-grass maps (Vulcanus, Doppelganger 2), with a different material, it is recomputed on the CPU.

use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::camera::visibility::VisibilitySystems;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use mu_data::terrain::{SIZE, Terrain};
use mu_data::texture;

use crate::Args;
use crate::terrain::{LayerMaterial, LoadedTerrain, MU_SCALE, TerrainCull, quad_indices, to_bevy_image};

pub struct GrassPlugin;

/// Grass with the wind in the shader.
pub type GrassMaterial = ExtendedMaterial<StandardMaterial, GrassWind>;

/// This frame's wind numbers ([`Wind`]).
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct GrassWind {
    /// x: speed, y: scale, z: x multiplier, w: `MU_SCALE`.
    #[uniform(100)]
    pub wind: Vec4,
    /// Karutan: x speed, y scale of the second wind, z = 1 if used.
    #[uniform(100)]
    pub second: Vec4,
}

impl MaterialExtension for GrassWind {
    fn vertex_shader() -> ShaderRef {
        "embedded://mu_client/grass_wind.wesl".into()
    }
}

/// The map's grass materials (to feed them the wind each frame).
#[derive(Resource, Default)]
struct GrassMaterials(Vec<Handle<GrassMaterial>>);

impl Plugin for GrassPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "grass_wind.wesl");
        app.add_plugins(MaterialPlugin::<GrassMaterial>::default())
            .init_resource::<GrassMaterials>()
            .add_systems(Update, (spawn_grass.run_if(crate::scene::load_step(crate::scene::Step::Grass)), wave_grass))
            .add_systems(
                PostUpdate,
                cull_grass.after(crate::terrain::TerrainCulled).before(VisibilitySystems::VisibilityPropagate),
            );
    }
}

/// Terrain textures that carry grass on top (`BITMAP_MAPGRASS + layer1`, there are only 3).
const GRASS_TEXTURES: u8 = 3;

/// `TerrainGrassWind` (and in Karutan `g_fTerrainGrassWind1`) at this moment (`InitTerrainLight`,
/// ZzzLodTerrain.cpp:2350-2403): `sin(WindSpeed + x · mult) · WindScale`, which only depends on x.
pub struct Wind {
    speed: f32,
    scale: f32,
    x_mult: f32,
    /// Karutan: second grass wind (speed, scale).
    second: Option<(f32, f32)>,
}

impl Wind {
    /// `ms` is `WorldTime`; `event`, the invasion (`EnableEvent`).
    pub fn new(world: u16, ms: i64, event: bool) -> Self {
        let speed = if event { (ms % 36000) as f32 * 0.01 } else { (ms % (360000 * 2)) as f32 * 0.002 };
        let (scale, x_mult) = match world as i32 - 1 {
            // Tarkan and Karutan: x · 50.
            8 | 80 | 81 => (10.0, 50.0),
            // Raklion (`WD_57ICECITY`, `WD_58ICECITY_BOSS`): scale 60.
            57 | 58 => (60.0, 50.0),
            _ => (10.0, 5.0),
        };
        let second = matches!(world as i32 - 1, 80 | 81).then(|| ((ms % 36000) as f32 * 0.008, 15.0));
        Self { speed, scale, x_mult, second }
    }

    /// Same, for the shader ([`GrassWind`]).
    fn uniforms(&self) -> (Vec4, Vec4) {
        let second = self.second.map_or(Vec4::ZERO, |(s, k)| Vec4::new(s, k, 1.0, 0.0));
        (Vec4::new(self.speed, self.scale, self.x_mult, MU_SCALE), second)
    }

    /// `TerrainGrassWind[x]` (also ripples the water).
    pub fn terrain(&self, x: f32) -> f32 {
        (self.speed + x * self.x_mult).sin() * self.scale
    }

    /// The wind that moves the grass: in Karutan the second one (`g_fTerrainGrassWind1`, 1600-1613).
    pub fn grass(&self, x: f32) -> f32 {
        match self.second {
            Some((speed, scale)) => (speed + x * 50.0).sin() * scale,
            None => self.terrain(x),
        }
    }
}

/// Maps without grass (see the module docs).
fn no_grass(world: u16) -> bool {
    matches!(world as i32 - 1,
        // Atlans and Doppelganger 3.
        7 | 67
        // Icarus and Kalima 1-7: the terrain is not drawn (`IsWaterTerrain`, ZzzScene.cpp:2108).
        | 10 | 24..=29 | 36
        // Blood Castle 1-7 and the master-level one.
        | 11..=17 | 52
        // Chaos Castle 1-6 and the master-level one.
        | 18..=23 | 53
        // Valley of Loren.
        | 30)
}

#[derive(Component)]
struct GrassMesh {
    mesh: Handle<Mesh>,
    /// Positions without wind (Bevy).
    base: Vec<[f32; 3]>,
    /// For each top vertex: (vertex index, x of the tile whose wind it uses).
    tops: Vec<(u32, u16)>,
    /// Tile of each tuft (one per four vertices).
    tiles: Vec<[u8; 2]>,
    area: [usize; 4],
    shown: Vec<bool>,
    cull_generation: u64,
}

fn spawn_grass(
    mut commands: Commands,
    args: Res<Args>,
    terrain: Option<Res<LoadedTerrain>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<GrassMaterial>>,
    mut layers: ResMut<Assets<LayerMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut grass_materials: ResMut<GrassMaterials>,
) {
    grass_materials.0.clear();
    let Some(terrain) = terrain else { return };
    let t = &terrain.0;
    if no_grass(args.world) {
        return;
    }
    let pk = crate::terrain::pk_field(args.world);
    let row_offset = row_offsets();

    for index in 0..GRASS_TEXTURES {
        let name = if pk && index == 0 { "TileGrass01_R.OZJ".to_string() } else { format!("TileGrass{:02}.ozt", index + 1) };
        let Ok(tex) = texture::load(&t.dir, &name) else { continue };
        let height = tex.height as f32 * 2.0;
        let image = images.add(to_bevy_image(&tex));
        let material = if pk {
            // `EnableAlphaBlend`: additive, no depth write.
            Err(layers.add(LayerMaterial::new(image, true)))
        } else {
            let m = materials.add(ExtendedMaterial {
                base: StandardMaterial {
                    base_color_texture: Some(image),
                    unlit: true,
                    cull_mode: None,
                    // The original's alpha test (`AlphaRef` 0.25).
                    alpha_mode: AlphaMode::Mask(0.25),
                    ..default()
                },
                extension: GrassWind::default(),
            });
            grass_materials.0.push(m.clone());
            Ok(m)
        };
        let mut total = 0;
        for cy in (0..SIZE).step_by(CHUNK) {
            for cx in (0..SIZE).step_by(CHUNK) {
                let area = [cx, cy, (cx + CHUNK).min(SIZE - 1), (cy + CHUNK).min(SIZE - 1)];
                let Some((mesh, base, tops, tiles)) = build_grass(t, index, height, &row_offset, area) else { continue };
                total += tiles.len();
                let handle = meshes.add(mesh);
                let shown = vec![true; tiles.len()];
                let mut e = commands.spawn((
                    Name::new(format!("Hierba {name} ({cx},{cy})")),
                    Mesh3d(handle.clone()),
                    GrassMesh { mesh: handle, base, tops, tiles, area, shown, cull_generation: 0 },
                    Visibility::default(),
                    crate::scene::WorldEntity,
                ));
                match &material {
                    Ok(m) => e.insert(MeshMaterial3d(m.clone())),
                    Err(m) => e.insert(MeshMaterial3d(m.clone())),
                };
            }
        }
        info!("hierba {name}: {total} matas");
    }
}

/// Side of each grass chunk, in tiles.
const CHUNK: usize = 16;

/// `TerrainGrassTexture`: random per-row U offset (0, 0.25, 0.5, 0.75).
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

type GrassData = (Mesh, Vec<[f32; 3]>, Vec<(u32, u16)>, Vec<[u8; 2]>);

fn build_grass(t: &Terrain, texture: u8, height: f32, row_offset: &[f32], area: [usize; 4]) -> Option<GrassData> {
    let m = &t.mapping;
    let mut pos = Vec::new();
    let mut uv = Vec::new();
    let mut color = Vec::new();
    let mut idx: Vec<u32> = Vec::new();
    let mut tops = Vec::new();
    let mut tiles = Vec::new();
    // MU (x, y, z) → Bevy (x, z, -y), 100 units per tile.
    let to_bevy = |x: f32, y: f32, z: f32| [x / MU_SCALE, z / MU_SCALE, -y / MU_SCALE];
    let light = |x: usize, y: usize| {
        let l = t.light[y * SIZE + x];
        [l[0].powf(2.2), l[1].powf(2.2), l[2].powf(2.2), 1.0]
    };

    for y in area[1]..area[3] {
        for x in area[0]..area[2] {
            if m.layer1[y * SIZE + x] != texture {
                continue;
            }
            // No ground (`TW_NOGROUND`): neither terrain nor grass.
            if t.attribute(x, y) & mu_data::terrain::attr::NO_GROUND != 0 {
                continue;
            }
            // Only tiles with no blend with the second layer.
            let corners = [(x, y), (x + 1, y), (x + 1, y + 1), (x, y + 1)];
            if corners.iter().any(|&(cx, cy)| m.alpha[cy * SIZE + cx] > 0) {
                continue;
            }
            let (xf, yf) = (x as f32 * MU_SCALE, y as f32 * MU_SCALE);
            let h0 = t.height(x, y);
            let h2 = t.height(x + 1, y + 1);
            let su = x as f32 * 0.25 + row_offset[y];

            let base = pos.len() as u32;
            // 0: top at (x, y)   1: top at (x+1, y+1)   2: bottom at (x+1, y+1)   3: bottom at (x, y)
            pos.push(to_bevy(xf - 50.0, yf, h0 + height));
            pos.push(to_bevy(xf + MU_SCALE - 50.0, yf + MU_SCALE, h2 + height));
            pos.push(to_bevy(xf + MU_SCALE, yf + MU_SCALE, h2));
            pos.push(to_bevy(xf, yf, h0));
            uv.extend_from_slice(&[[su, 0.0], [su + 0.25, 0.0], [su + 0.25, 1.0], [su, 1.0]]);
            // Same per-vertex light as the original (tile corners in order).
            color.extend_from_slice(&[light(x, y), light(x + 1, y), light(x + 1, y + 1), light(x, y + 1)]);
            idx.extend_from_slice(&quad_indices(base));
            // Wind of tile x for vertex 0 and of x+1 for vertex 1.
            tops.push((base, x as u16));
            tops.push((base + 1, x as u16 + 1));
            tiles.push([x as u8, y as u8]);
        }
    }
    if idx.is_empty() {
        return None;
    }
    let n = pos.len();
    // For the shader: (x of the column whose wind it uses, 1 = top vertex).
    let mut wind = vec![[0.0f32, 0.0]; n];
    for &(v, x) in &tops {
        wind[v as usize] = [x as f32, 1.0];
    }
    let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos.clone())
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; n])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, wind)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, color)
        .with_inserted_indices(Indices::U32(idx));
    Some((mesh, pos, tops, tiles))
}

/// The grass goes in the same pass as the ground (`RenderTerrainFrustrum` with `TERRAIN_MAP_GRASS`):
/// only on the tiles being drawn.
fn cull_grass(cull: Res<TerrainCull>, mut grass: Query<(&mut GrassMesh, &mut Visibility)>, mut meshes: ResMut<Assets<Mesh>>) {
    for (mut g, mut vis) in &mut grass {
        if g.cull_generation == cull.generation {
            continue;
        }
        g.cull_generation = cull.generation;
        let may = cull.area_may_be_visible(g.area);
        let shown: Vec<bool> = g.tiles.iter().map(|&[x, y]| may && cull.tile_visible(x as usize, y as usize)).collect();
        let any = shown.iter().any(|&s| s);
        vis.set_if_neq(if any { Visibility::Inherited } else { Visibility::Hidden });
        if !any || shown == g.shown {
            continue;
        }
        if let Some(mut m) = meshes.get_mut(&g.mesh) {
            let idx: Vec<u32> =
                (0..shown.len() as u32).filter(|&q| shown[q as usize]).flat_map(|q| quad_indices(q * 4)).collect();
            m.insert_indices(Indices::U32(idx));
        }
        g.shown = shown;
    }
}

fn wave_grass(
    time: Res<Time>,
    args: Res<Args>,
    invasion: Res<crate::invasion::Invasion>,
    grass_materials: Res<GrassMaterials>,
    mut wind_materials: ResMut<Assets<GrassMaterial>>,
    grass: Query<(&GrassMesh, &ViewVisibility, Has<MeshMaterial3d<LayerMaterial>>)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let wind = Wind::new(args.world, crate::terrain::world_ms(&time), invasion.active());
    // Normal grass: only the wind numbers.
    let (w, second) = wind.uniforms();
    for h in &grass_materials.0 {
        if let Some(mut m) = wind_materials.get_mut(h) {
            m.extension.wind = w;
            m.extension.second = second;
        }
    }
    let mut per_x: Option<Vec<f32>> = None;
    // Additive grass (different material): on the CPU, only the chunks seen in the last frame.
    for (g, visible, layered) in &grass {
        if !visible.get() || !layered {
            continue;
        }
        let w = per_x.get_or_insert_with(|| (0..=SIZE).map(|x| wind.grass(x as f32) / MU_SCALE).collect());
        let mut pos = g.base.clone();
        for &(v, x) in &g.tops {
            // MU +Y is Bevy -Z.
            pos[v as usize][2] -= w[x as usize];
        }
        if let Some(mut m) = meshes.get_mut(&g.mesh) {
            m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viento_por_mapa() {
        // Lorencia: sin(0.002 · t + 5x) · 10.
        let w = Wind::new(1, 1000, false);
        assert!((w.terrain(1.0) - (2.0f32 + 5.0).sin() * 10.0).abs() < 1e-4);
        // Tarkan: x · 50.
        assert!((Wind::new(9, 0, false).terrain(1.0) - 50f32.sin() * 10.0).abs() < 1e-4);
        // Raklion: scale 60.
        assert!((Wind::new(58, 0, false).terrain(1.0) - 50f32.sin() * 60.0).abs() < 1e-4);
        // Karutan: the grass uses the second wind.
        let k = Wind::new(81, 1000, false);
        assert!((k.grass(1.0) - (8.0f32 + 50.0).sin() * 15.0).abs() < 1e-4);
        // Time wraps every 720 s.
        assert!((Wind::new(1, 720_000, false).terrain(0.0)).abs() < 1e-6);
        assert!(no_grass(8) && no_grass(12) && no_grass(19) && no_grass(31) && !no_grass(1));
    }
}
