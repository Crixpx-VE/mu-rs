//! Hierba del terreno que se mece con el viento (`RenderTerrainFace` con `TERRAIN_MAP_GRASS`
//! en `ZzzLodTerrain.cpp`, 1566-1633).
//!
//! En cada casilla de hierba "pura" (textura base 0, 1 o 2 y sin mezcla con la segunda capa)
//! el original levanta un plano vertical en diagonal, de la esquina (x, y) a la (x+1, y+1),
//! con la textura `TileGrass0N.tga`:
//!
//! - alto = alto de la textura en píxeles × 2 (unidades de MU), y la parte de arriba
//!   desplazada 50 unidades hacia -X;
//! - U = x · 0.25 + un desfase al azar por fila (0, 0.25, 0.5 o 0.75); V de 0 (arriba) a 1;
//! - viento: los vértices de arriba se mueven en Y con `TerrainGrassWind` ([`Wind`]).
//!
//! No hay hierba en Atlans ni Doppelganger 3 (`RenderTerrain`, 2625), en Chaos Castle ni el Valle de
//! Loren (`TerrainGrassEnable`, 333-342), en Blood Castle (1573) ni en Icarus (sin terreno). En Vulcanus
//! y Doppelganger 2 la primera es `TileGrass01_R.jpg` y toda la hierba va sumada (`EnableAlphaBlend`,
//! MapManager.cpp:1541-1545 y ZzzLodTerrain.cpp:1584-1585).
//!
//! Solo se ve la de las casillas que dibuja el suelo ([`crate::terrain::TerrainCull`]). El viento
//! depende del tiempo real, así que va igual a cualquier fps. Los vértices se recalculan en CPU cada
//! fotograma (solo cambia la Y de MU de los de arriba), solo en los bloques que se ven.

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::VisibilitySystems;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use mu_data::terrain::{SIZE, Terrain};
use mu_data::texture;

use crate::Args;
use crate::terrain::{LayerMaterial, LoadedTerrain, MU_SCALE, TerrainCull, quad_indices, to_bevy_image};

pub struct GrassPlugin;

impl Plugin for GrassPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (spawn_grass.run_if(crate::scene::load_step(crate::scene::Step::Grass)), wave_grass))
            .add_systems(
                PostUpdate,
                cull_grass.after(crate::terrain::TerrainCulled).before(VisibilitySystems::VisibilityPropagate),
            );
    }
}

/// Texturas de terreno que llevan hierba encima (`BITMAP_MAPGRASS + capa1`, solo hay 3).
const GRASS_TEXTURES: u8 = 3;

/// `TerrainGrassWind` (y en Karutan `g_fTerrainGrassWind1`) de este momento (`InitTerrainLight`,
/// ZzzLodTerrain.cpp:2350-2403): `sin(WindSpeed + x · mult) · WindScale`, que solo depende de la x.
pub struct Wind {
    speed: f32,
    scale: f32,
    x_mult: f32,
    /// Karutan: segundo viento de la hierba (velocidad, escala).
    second: Option<(f32, f32)>,
}

impl Wind {
    /// `ms` es `WorldTime`; `event`, la invasión (`EnableEvent`).
    pub fn new(world: u16, ms: i64, event: bool) -> Self {
        let speed = if event { (ms % 36000) as f32 * 0.01 } else { (ms % (360000 * 2)) as f32 * 0.002 };
        let (scale, x_mult) = match world as i32 - 1 {
            // Tarkan y Karutan: x · 50.
            8 | 80 | 81 => (10.0, 50.0),
            // Raklion (`WD_57ICECITY`, `WD_58ICECITY_BOSS`): escala 60.
            57 | 58 => (60.0, 50.0),
            _ => (10.0, 5.0),
        };
        let second = matches!(world as i32 - 1, 80 | 81).then(|| ((ms % 36000) as f32 * 0.008, 15.0));
        Self { speed, scale, x_mult, second }
    }

    /// `TerrainGrassWind[x]` (también ondula el agua).
    pub fn terrain(&self, x: f32) -> f32 {
        (self.speed + x * self.x_mult).sin() * self.scale
    }

    /// El viento que mueve la hierba: en Karutan el segundo (`g_fTerrainGrassWind1`, 1600-1613).
    pub fn grass(&self, x: f32) -> f32 {
        match self.second {
            Some((speed, scale)) => (speed + x * 50.0).sin() * scale,
            None => self.terrain(x),
        }
    }
}

/// Mapas sin hierba (ver la documentación del módulo).
fn no_grass(world: u16) -> bool {
    matches!(world as i32 - 1,
        // Atlans y Doppelganger 3.
        7 | 67
        // Icarus: no se dibuja el terreno.
        | 10
        // Blood Castle 1-7 y el de nivel maestro.
        | 11..=17 | 52
        // Chaos Castle 1-6 y el de nivel maestro.
        | 18..=23 | 53
        // Valle de Loren.
        | 30)
}

#[derive(Component)]
struct GrassMesh {
    mesh: Handle<Mesh>,
    /// Posiciones sin viento (Bevy).
    base: Vec<[f32; 3]>,
    /// Para cada vértice de arriba: (índice del vértice, x de la casilla cuyo viento usa).
    tops: Vec<(u32, u16)>,
    /// Casilla de cada mata (una por cada cuatro vértices).
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
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut layers: ResMut<Assets<LayerMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
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
            // `EnableAlphaBlend`: sumada, sin escribir profundidad.
            Err(layers.add(LayerMaterial::new(image, true)))
        } else {
            Ok(materials.add(StandardMaterial {
                base_color_texture: Some(image),
                unlit: true,
                cull_mode: None,
                // Prueba de alfa del original (`AlphaRef` 0.25).
                alpha_mode: AlphaMode::Mask(0.25),
                ..default()
            }))
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

/// Lado de cada bloque de hierba, en casillas.
const CHUNK: usize = 16;

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

type GrassData = (Mesh, Vec<[f32; 3]>, Vec<(u32, u16)>, Vec<[u8; 2]>);

fn build_grass(t: &Terrain, texture: u8, height: f32, row_offset: &[f32], area: [usize; 4]) -> Option<GrassData> {
    let m = &t.mapping;
    let mut pos = Vec::new();
    let mut uv = Vec::new();
    let mut color = Vec::new();
    let mut idx: Vec<u32> = Vec::new();
    let mut tops = Vec::new();
    let mut tiles = Vec::new();
    // MU (x, y, z) → Bevy (x, z, -y), 100 unidades por casilla.
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
            // Sin suelo (`TW_NOGROUND`): ni terreno ni hierba.
            if t.attribute(x, y) & mu_data::terrain::attr::NO_GROUND != 0 {
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
            idx.extend_from_slice(&quad_indices(base));
            // Viento de la casilla x para el vértice 0 y de la x+1 para el 1.
            tops.push((base, x as u16));
            tops.push((base + 1, x as u16 + 1));
            tiles.push([x as u8, y as u8]);
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
    Some((mesh, pos, tops, tiles))
}

/// La hierba va en la misma pasada que el suelo (`RenderTerrainFrustrum` con `TERRAIN_MAP_GRASS`):
/// solo la de las casillas que se dibujan.
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
    grass: Query<(&GrassMesh, &ViewVisibility)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let wind = Wind::new(args.world, crate::terrain::world_ms(&time), invasion.active());
    let mut per_x: Option<Vec<f32>> = None;
    // Solo los bloques que se vieron en el último fotograma.
    for (g, visible) in &grass {
        if !visible.get() {
            continue;
        }
        let w = per_x.get_or_insert_with(|| (0..=SIZE).map(|x| wind.grass(x as f32) / MU_SCALE).collect());
        let mut pos = g.base.clone();
        for &(v, x) in &g.tops {
            // +Y de MU es -Z en Bevy.
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
        // Raklion: escala 60.
        assert!((Wind::new(58, 0, false).terrain(1.0) - 50f32.sin() * 60.0).abs() < 1e-4);
        // Karutan: la hierba usa el segundo viento.
        let k = Wind::new(81, 1000, false);
        assert!((k.grass(1.0) - (8.0f32 + 50.0).sin() * 15.0).abs() < 1e-4);
        // El tiempo da la vuelta cada 720 s.
        assert!((Wind::new(1, 720_000, false).terrain(0.0)).abs() < 1e-6);
        assert!(no_grass(8) && no_grass(12) && no_grass(19) && no_grass(31) && !no_grass(1));
    }
}
