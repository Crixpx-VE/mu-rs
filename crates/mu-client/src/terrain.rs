//! Converts the `mu-data` terrain into Bevy meshes.
//!
//! Like the original client (`RenderTerrainFace`, ZzzLodTerrain.cpp:1514-1565), each tile
//! is drawn in two passes:
//! 1. Base: if all four corners are fully on layer 2 (alpha 1), the layer 2 texture;
//!    otherwise layer 1 (opaque, or alpha tested for the `RenderFace` textures).
//! 2. On top, if any corner has some layer 2: in Atlans (and Doppelganger 3), where layer 2
//!    is water, the animated `Object8\wtNN` caustics, additive; elsewhere, if the tile was not
//!    full, layer 2 with per-vertex transparency (`RenderFaceAlpha`: blend + alpha test).
//!
//! Water (`TileWater01`, and the Vulcanus lava) scrolls in U (`WaterMove`) and waves in V with the
//! grass wind (`FaceTexture`, 1474-1508). Only tiles inside the original's ground trapezoid are
//! drawn (`CreateFrustrum2D` / `TestFrustrum2D` on the 4×4 blocks and on each tile, within the
//! `CreateFrustrum` bounds): see [`TerrainCull`].
//!
//! Tiles are grouped by pass and texture in 16×16 chunks.
//!
//! Axes: MU uses Z up and Bevy uses Y up, so
//! MU (x, y, z) → Bevy (x, z, -y). A tile is 1 unit (100 in MU).

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::VisibilitySystems;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology};
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, Extent3d, RenderPipelineDescriptor,
    SpecializedMeshPipelineError, TextureDimension, TextureFormat,
};
use bevy::shader::ShaderRef;
use mu_data::terrain::{self, NO_TEXTURE, SIZE, Terrain};
use mu_data::texture::Texture;

use crate::Args;
use crate::fx::material::FxParams;

/// MU units per Bevy unit (one tile = 100 in MU).
pub const MU_SCALE: f32 = 100.0;

/// The loaded terrain, so other systems (camera, characters) can query heights.
#[derive(Resource)]
pub struct LoadedTerrain(pub Terrain);

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        // The effects shader, registered here already: this plugin runs before the effects one and
        // the material requests it on creation (registering it twice is harmless).
        bevy::asset::embedded_asset!(app, "fx/fx.wesl");
        app.add_plugins(MaterialPlugin::<LayerMaterial>::default())
            .init_resource::<TerrainLights>()
            .init_resource::<TerrainCull>()
            .init_resource::<Caustics>()
            .add_systems(Update, spawn_terrain.run_if(crate::scene::load_step(crate::scene::Step::Terrain)))
            // Lights are cleared at the start of each tick; emitters refill them afterwards.
            .add_systems(FixedFirst, clear_lights)
            // `WaterTextureNumber++` on each logic loop (ZzzScene.cpp:2315-2316).
            .add_systems(FixedUpdate, |mut c: ResMut<Caustics>| c.frame = (c.frame + 1) % CAUSTIC_FRAMES)
            .add_systems(Update, (apply_lights, show_caustics))
            // With the camera already placed this frame and before deciding what is visible.
            .add_systems(
                PostUpdate,
                (update_cull, cull_chunks, move_water)
                    .chain()
                    .in_set(TerrainCulled)
                    .after(crate::camera::CameraMoved)
                    .before(VisibilitySystems::VisibilityPropagate),
            );
    }
}

/// When the drawn ground tiles of this frame are decided (the grass comes after).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct TerrainCulled;

/// Terrain texture that is water on every map (`TileWater01`).
const WATER_TEXTURE: u8 = 5;
/// The lava of Vulcanus and Doppelganger 2 (`Object64\song_lava1.jpg`), which flows like water.
const LAVA_TEXTURE: u8 = 11;
/// `BITMAP_WATER`: 32 textures `Object8\wt00-31.jpg`.
const CAUSTIC_FRAMES: u32 = 32;

/// The original's `WorldActive`: our map number minus one.
fn active(world: u16) -> i32 {
    world as i32 - 1
}

/// Vulcanus (`IsPKField`) or Doppelganger 2 (`IsDoppelGanger2`).
pub fn pk_field(world: u16) -> bool {
    matches!(active(world), 63 | 66)
}

/// Maps with caustics over layer 2 water: Atlans and Doppelganger 3 (ZzzLodTerrain.cpp:1547).
fn caustic_world(world: u16) -> bool {
    matches!(active(world), 7 | 67)
}

/// Icarus and Kalima: the original does not draw the ground (`RenderTerrain` is not called, ZzzScene.cpp:2108-2116).
/// In Kalima (`IsWaterTerrain`) the sea of [`crate::water_terrain`] goes in its place.
pub fn hidden_terrain(world: u16) -> bool {
    active(world) == 10 || crate::water_terrain::is_water_world(world)
}

/// `WaterMove` (`RenderTerrain`, ZzzLodTerrain.cpp:2589-2602): fraction of the texture the water
/// has advanced after `ms` milliseconds. Tarkan wraps around in 40 s, Balgass Barracks (`WD_42`) in
/// 50 s and the rest in 20 s.
pub fn water_move(world: u16, ms: i64) -> f32 {
    match active(world) {
        8 => (ms % 40000) as f32 * 0.000025,
        42 => (ms % 50000) as f32 * 0.00002,
        _ => (ms % 20000) as f32 * 0.00005,
    }
}

/// Milliseconds like `WorldTime` (`timeGetTime`).
pub fn world_ms(time: &Time) -> i64 {
    (time.elapsed_secs_f64() * 1000.0) as i64
}

// --- Overlay layer material -----------------------------------------------------------------------

/// Material for what goes over the base ground: layer 2 (`EnableAlphaTest`: alpha blend and alpha
/// test > 0.25, writes depth) or something additive (`EnableAlphaBlend`: ONE, ONE, no depth
/// write), like the caustics or the Vulcanus grass. Reuses the effects shader
/// (texture × vertex color × tint). Goes in Bevy's alpha-mask pass, right after the opaque
/// ground and before everything transparent, which is the original's order (terrain first).
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
#[bind_group_data(LayerKey)]
pub struct LayerMaterial {
    #[uniform(0)]
    pub params: FxParams,
    #[texture(1)]
    #[sampler(2)]
    pub texture: Option<Handle<Image>>,
    pub additive: bool,
}

impl LayerMaterial {
    pub fn new(texture: Handle<Image>, additive: bool) -> Self {
        Self { params: FxParams::default(), texture: Some(texture), additive }
    }
}

#[repr(C)]
#[derive(Eq, PartialEq, Hash, Copy, Clone)]
pub struct LayerKey {
    additive: bool,
}

impl From<&LayerMaterial> for LayerKey {
    fn from(m: &LayerMaterial) -> Self {
        Self { additive: m.additive }
    }
}

impl Material for LayerMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://mu_client/fx/fx.wesl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Mask(crate::objects::ALPHA_REF)
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        use BlendFactor::*;
        let additive = key.bind_group_data.additive;
        descriptor.primitive.cull_mode = None;
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_write_enabled = Some(!additive);
            // Same geometry as the tile below: nudged towards the camera to avoid z-fighting.
            depth.bias.constant = 1;
        }
        let (src, dst) = if additive { (One, One) } else { (SrcAlpha, OneMinusSrcAlpha) };
        if let Some(fragment) = descriptor.fragment.as_mut() {
            if !additive {
                fragment.shader_defs.push("FX_ALPHA_TEST".into());
            }
            for target in fragment.targets.iter_mut().flatten() {
                target.blend = Some(BlendState {
                    color: BlendComponent { src_factor: src, dst_factor: dst, operation: BlendOperation::Add },
                    alpha: BlendComponent { src_factor: One, dst_factor: OneMinusSrcAlpha, operation: BlendOperation::Add },
                });
            }
        }
        Ok(())
    }
}

// --- Atlans caustics -----------------------------------------------------------------------------

/// Caustics of the loaded map: the material and its 32 frames. `frame` is `WaterTextureNumber`.
#[derive(Resource, Default)]
pub struct Caustics {
    material: Option<Handle<LayerMaterial>>,
    frames: Vec<Handle<Image>>,
    frame: u32,
    shown: Option<u32>,
}

/// Sets the tick's frame on the caustics material.
fn show_caustics(mut c: ResMut<Caustics>, mut materials: ResMut<Assets<LayerMaterial>>) {
    let Some(h) = c.material.clone() else { return };
    if c.shown == Some(c.frame) || c.frames.is_empty() {
        return;
    }
    let frame = c.frame;
    c.shown = Some(frame);
    if let Some(mut m) = materials.get_mut(&h) {
        m.texture = Some(c.frames[frame as usize % c.frames.len()].clone());
    }
}

// --- Building -------------------------------------------------------------------------------------

/// Which pass draws a chunk.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Pass {
    /// `RenderFace`: the tile's base texture.
    Base,
    /// `RenderFaceAlpha`: layer 2 with its transparency.
    Alpha,
    /// Atlans `RenderFaceBlend`: additive caustics, with the alpha as color.
    Caustic,
}

/// What each tile draws, decided like `RenderTerrainFace` (ZzzLodTerrain.cpp:1514-1565).
#[derive(Clone, Copy, PartialEq, Debug)]
struct TileDraw {
    /// Texture of the base pass and whether it is water (scrolls).
    base: Option<(u8, bool)>,
    /// The overlay pass: caustics, or layer 2 with whether it is water.
    over: Option<(Pass, u8, bool)>,
}

/// Decides what tile (x, y) draws: `alpha` are the four corners (0..1) in the original's order.
fn tile_draw(world: u16, layer1: u8, layer2: u8, alpha: [f32; 4]) -> TileDraw {
    let full = alpha.iter().all(|&a| a >= 1.0);
    let any = alpha.iter().any(|&a| a > 0.0);
    // Full tile: the layer 2 texture, no water. Otherwise layer 1 (water: 5, and lava
    // in Vulcanus / Doppelganger 2).
    let (base, water) = if full && layer2 != NO_TEXTURE {
        (layer2, false)
    } else {
        (layer1, layer1 == WATER_TEXTURE || (layer1 == LAVA_TEXTURE && pk_field(world)))
    };
    // Kanturu 3: 100 is not drawn in the normal pass (`RenderFace`, 1336-1337).
    let base = (!(active(world) == 39 && base == 100)).then_some((base, water));
    let over = if !any {
        None
    } else if caustic_world(world) && layer2 == WATER_TEXTURE {
        Some((Pass::Caustic, layer2, false))
    } else if !full && layer2 != NO_TEXTURE {
        // Layer 2 is only water if the base was too.
        Some((Pass::Alpha, layer2, water && layer2 == WATER_TEXTURE))
    } else {
        None
    };
    TileDraw { base, over }
}

fn spawn_terrain(
    mut commands: Commands,
    args: Res<Args>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut layers: ResMut<Assets<LayerMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut caustics: ResMut<Caustics>,
) {
    let t = match terrain::load(&args.data, args.world) {
        Ok(t) => t,
        Err(e) => {
            error!("no se pudo cargar World{}: {e}", args.world);
            return;
        }
    };
    info!("World{} cargado desde {}", args.world, t.dir.display());
    if hidden_terrain(args.world) {
        // Heights are still needed (characters, effects), but the ground is not visible.
        commands.insert_resource(LoadedTerrain(t));
        return;
    }

    // Image and UV scale (`64 / width`, `FaceTexture`) of each used texture.
    let mut tex: HashMap<u8, (Handle<Image>, Vec2)> = HashMap::new();
    for (index, texture) in t.textures.iter().enumerate() {
        let Some(texture) = texture else { continue };
        let uv = Vec2::new(64.0 / texture.width as f32, 64.0 / texture.height as f32);
        tex.insert(index as u8, (images.add(to_bevy_image(texture)), uv));
    }

    // Caustics: `Object8\wtNN.jpg` (MapManager.cpp:132-147), UV of `16 / width` (`FaceTexture` with `Scale`).
    let mut caustic_uv = Vec2::ONE;
    if caustic_world(args.world) {
        let dir = mu_data::fs::find(&args.data, "Object8").unwrap_or_else(|| args.data.join("Object8"));
        caustics.frames = (0..CAUSTIC_FRAMES)
            .filter_map(|i| mu_data::texture::load(&dir, &format!("wt{i:02}.OZJ")).ok())
            .enumerate()
            .map(|(i, texture)| {
                if i == 0 {
                    caustic_uv = Vec2::new(16.0 / texture.width as f32, 16.0 / texture.height as f32);
                }
                images.add(to_bevy_image(&texture))
            })
            .collect();
        if let Some(first) = caustics.frames.first() {
            caustics.material = Some(layers.add(LayerMaterial::new(first.clone(), true)));
            caustics.shown = None;
        } else {
            warn!("faltan las cáusticas Object8/wtNN");
        }
    }

    let mut base_mats: HashMap<u8, Handle<StandardMaterial>> = HashMap::new();
    let mut alpha_mats: HashMap<u8, Handle<LayerMaterial>> = HashMap::new();
    let alpha_tested = terrain::alpha_tested_tile(args.world);

    for cy in (0..SIZE - 1).step_by(CHUNK) {
        for cx in (0..SIZE - 1).step_by(CHUNK) {
            let area = [cx, cy, (cx + CHUNK).min(SIZE - 1), (cy + CHUNK).min(SIZE - 1)];
            for ((pass, texture, water), quads) in chunk_groups(&t, args.world, area) {
                let (uv_scale, material) = match pass {
                    Pass::Caustic => {
                        let Some(m) = caustics.material.clone() else { continue };
                        (caustic_uv, MaterialChoice::Layer(m))
                    }
                    Pass::Base => {
                        let Some((image, uv)) = tex.get(&texture) else { continue };
                        let m = base_mats.entry(texture).or_insert_with(|| {
                            materials.add(StandardMaterial {
                                base_color_texture: Some(image.clone()),
                                unlit: true,
                                cull_mode: None,
                                // `RenderFace`: this texture is alpha tested (fences, holes).
                                alpha_mode: if alpha_tested == Some(texture) {
                                    AlphaMode::Mask(crate::objects::ALPHA_REF)
                                } else {
                                    AlphaMode::Opaque
                                },
                                ..default()
                            })
                        });
                        (*uv, MaterialChoice::Base(m.clone()))
                    }
                    Pass::Alpha => {
                        let Some((image, uv)) = tex.get(&texture) else { continue };
                        let m = alpha_mats
                            .entry(texture)
                            .or_insert_with(|| layers.add(LayerMaterial::new(image.clone(), false)));
                        (*uv, MaterialChoice::Layer(m.clone()))
                    }
                };
                let (mesh, chunk) = build_mesh(&t, (pass, texture, water), uv_scale, &quads, area);
                let handle = meshes.add(mesh);
                let mut e = commands.spawn((
                    Name::new(format!("Terreno {pass:?} textura {texture} ({cx},{cy})")),
                    Mesh3d(handle.clone()),
                    TerrainChunk { mesh: handle, ..chunk },
                    Visibility::default(),
                    crate::scene::WorldEntity,
                ));
                match material {
                    MaterialChoice::Base(m) => e.insert(MeshMaterial3d(m)),
                    MaterialChoice::Layer(m) => e.insert(MeshMaterial3d(m)),
                };
            }
        }
    }

    commands.insert_resource(LoadedTerrain(t));
}

enum MaterialChoice {
    Base(Handle<StandardMaterial>),
    Layer(Handle<LayerMaterial>),
}

/// Tiles of `area` grouped by (pass, texture, water), in order.
fn chunk_groups(t: &Terrain, world: u16, area: [usize; 4]) -> Vec<((Pass, u8, bool), Vec<[usize; 2]>)> {
    let m = &t.mapping;
    let mut groups: Vec<((Pass, u8, bool), Vec<[usize; 2]>)> = Vec::new();
    let mut push = |key: (Pass, u8, bool), x: usize, y: usize| match groups.iter_mut().find(|g| g.0 == key) {
        Some(g) => g.1.push([x, y]),
        None => groups.push((key, vec![[x, y]])),
    };
    for y in area[1]..area[3] {
        for x in area[0]..area[2] {
            // `RenderTerrainTile`: `TW_NOGROUND` tiles are not drawn.
            if t.attribute(x, y) & terrain::attr::NO_GROUND != 0 {
                continue;
            }
            let i = y * SIZE + x;
            let d = tile_draw(world, m.layer1[i], m.layer2[i], corner_alpha(t, x, y));
            if let Some((texture, water)) = d.base {
                push((Pass::Base, texture, water), x, y);
            }
            if let Some((pass, texture, water)) = d.over {
                push((pass, texture, water), x, y);
            }
        }
    }
    groups
}

/// Tile corners in the original's order (`TerrainIndex1..4`): (x, y), (x+1, y), (x+1, y+1), (x, y+1).
fn corners(x: usize, y: usize) -> [(usize, usize); 4] {
    [(x, y), (x + 1, y), (x + 1, y + 1), (x, y + 1)]
}

fn corner_alpha(t: &Terrain, x: usize, y: usize) -> [f32; 4] {
    corners(x, y).map(|(cx, cy)| t.mapping.alpha[cy * SIZE + cx] as f32 / 255.0)
}

/// Terrain chunk side, in tiles.
const CHUNK: usize = 16;

/// A terrain chunk, with what is needed to recompute its light, its water and which tiles are visible.
#[derive(Component)]
pub struct TerrainChunk {
    mesh: Handle<Mesh>,
    pass: Pass,
    /// Corner (x, y) of each vertex (4 per tile, in order).
    corners: Vec<[u8; 2]>,
    /// Per-vertex transparency (layer 2).
    alpha: Vec<f32>,
    /// Tiles covered: [x0, y0, x1, y1].
    area: [usize; 4],
    /// Had dynamic lights in the last update (must be restored to its normal light).
    lit: bool,
    /// Unscrolled UVs, if water (`FaceTexture` with `Water`).
    water_uv: Option<Vec<[f32; 2]>>,
    /// Whether it is `TileWater01` water (in Valley of Loren it flows backwards; lava does not).
    water_tile: bool,
    /// Tiles drawn right now (one per four vertices).
    shown: Vec<bool>,
    /// [`TerrainCull`] generation already applied.
    cull_generation: u64,
}

fn build_mesh(
    t: &Terrain,
    (pass, texture, water): (Pass, u8, bool),
    uv_scale: Vec2,
    quads: &[[usize; 2]],
    area: [usize; 4],
) -> (Mesh, TerrainChunk) {
    let mut pos = Vec::with_capacity(quads.len() * 4);
    let mut uv = Vec::with_capacity(quads.len() * 4);
    let mut color = Vec::with_capacity(quads.len() * 4);
    let mut corners_out = Vec::with_capacity(quads.len() * 4);
    let mut alpha_out = Vec::with_capacity(quads.len() * 4);
    for &[x, y] in quads {
        let alpha = corner_alpha(t, x, y);
        for (k, (cx, cy)) in corners(x, y).into_iter().enumerate() {
            let h = t.height(cx, cy) / MU_SCALE;
            pos.push([cx as f32, h, -(cy as f32)]);
            uv.push([cx as f32 * uv_scale.x, cy as f32 * uv_scale.y]);
            let a = if pass == Pass::Base { 1.0 } else { alpha[k] };
            color.push(match pass {
                // `VertexBlend`: the color is the alpha, unlit.
                Pass::Caustic => vertex_color([a; 3], 1.0),
                _ => vertex_color(t.light[cy * SIZE + cx], a),
            });
            corners_out.push([cx as u8, cy as u8]);
            alpha_out.push(a);
        }
    }
    let n = pos.len();
    let idx: Vec<u32> = (0..quads.len() as u32).flat_map(|q| quad_indices(q * 4)).collect();
    let water_uv = water.then(|| uv.clone());
    let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; n])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, color)
        .with_inserted_indices(Indices::U32(idx));
    let chunk = TerrainChunk {
        mesh: Handle::default(),
        pass,
        corners: corners_out,
        alpha: alpha_out,
        area,
        lit: false,
        water_uv,
        water_tile: texture == WATER_TEXTURE,
        shown: vec![true; quads.len()],
        cull_generation: 0,
    };
    (mesh, chunk)
}

/// The two triangles of the tile starting at vertex `base` (`GL_TRIANGLE_FAN` 0-1-2-3).
pub fn quad_indices(base: u32) -> [u32; 6] {
    [base, base + 1, base + 2, base, base + 2, base + 3]
}

/// On map change: nothing from the previous one.
pub fn reset(mut lights: ResMut<TerrainLights>, mut caustics: ResMut<Caustics>, mut cull: ResMut<TerrainCull>) {
    lights.lights.clear();
    *caustics = Caustics::default();
    cull.shape = None;
    cull.generation += 1;
}

// --- Water ----------------------------------------------------------------------------------------

/// Scrolls the water: `u ± WaterMove` and `v + TerrainGrassWind · 0.002` on each vertex (`FaceTexture`).
fn move_water(
    time: Res<Time>,
    args: Res<Args>,
    invasion: Res<crate::invasion::Invasion>,
    chunks: Query<(&TerrainChunk, &Visibility)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let ms = world_ms(&time);
    let shift = water_move(args.world, ms);
    let wind = crate::grass::Wind::new(args.world, ms, invasion.active());
    for (c, vis) in &chunks {
        let Some(base) = &c.water_uv else { continue };
        if *vis == Visibility::Hidden {
            continue;
        }
        // Valley of Loren (`WD_30BATTLECASTLE`) flows backwards, only on texture 5 (1489-1492).
        let du = if active(args.world) == 30 && c.water_tile { -shift } else { shift };
        let uv: Vec<[f32; 2]> = base
            .iter()
            .zip(&c.corners)
            .map(|(&[u, v], &[x, _])| [u + du, v + wind.terrain(x as f32) * 0.002])
            .collect();
        if let Some(mut m) = meshes.get_mut(&c.mesh) {
            m.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
        }
    }
}

// --- Which tiles are visible ------------------------------------------------------------------------

/// The original's ground trapezoid for this frame (`None` = no culling: outside the game).
#[derive(Resource, Default)]
pub struct TerrainCull {
    pub shape: Option<CullShape>,
    /// Changes every time `shape` changes.
    pub generation: u64,
}

impl TerrainCull {
    /// Whether tile (x, y) is drawn (`RenderTerrainFrustrum` + `RenderTerrainBlock`).
    pub fn tile_visible(&self, x: usize, y: usize) -> bool {
        self.shape.as_ref().is_none_or(|s| s.tile_visible(x, y))
    }

    /// Whether something of `area` ([x0, y0, x1, y1), in tiles) can be visible.
    pub fn area_may_be_visible(&self, area: [usize; 4]) -> bool {
        self.shape.as_ref().is_none_or(|s| s.overlaps(area))
    }
}

/// `CreateFrustrum2D` (in tiles) and the block bounds of `CreateFrustrum`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CullShape {
    /// `FrustrumX` / `FrustrumY`: the four trapezoid corners, in tiles.
    pub fx: [f32; 4],
    pub fy: [f32; 4],
    /// `FrustrumBoundMinX`, `MinY`, `MaxX`, `MaxY`: first and last 4×4 block traversed.
    pub bound: [i32; 4],
}

impl CullShape {
    /// `TestFrustrum2D` (ZzzLodTerrain.cpp:2141-2157).
    pub fn test(&self, x: f32, y: f32, range: f32) -> bool {
        let mut j = 3;
        for i in 0..4 {
            let d = (self.fx[i] - x) * (self.fy[j] - y) - (self.fx[j] - x) * (self.fy[i] - y);
            if d <= range {
                return false;
            }
            j = i;
        }
        true
    }

    /// `RenderTerrainFrustrum`: 4×4 blocks from `FrustrumBoundMin` to `Max` with
    /// `TestFrustrum2D(center, -40)` (`g_fFrustumRange`), and in each block the tiles with
    /// `TestFrustrum2D(center, 0)` (`RenderTerrainBlock`, 2485-2540).
    pub fn tile_visible(&self, x: usize, y: usize) -> bool {
        let [min_x, min_y, max_x, max_y] = self.bound;
        let (x, y) = (x as i32, y as i32);
        if x < min_x || y < min_y {
            return false;
        }
        let bx = min_x + (x - min_x) / 4 * 4;
        let by = min_y + (y - min_y) / 4 * 4;
        bx <= max_x
            && by <= max_y
            && self.test(bx as f32 + 2.0, by as f32 + 2.0, -40.0)
            && self.test(x as f32 + 0.5, y as f32 + 0.5, 0.0)
    }

    /// Quick rejection of a whole chunk.
    fn overlaps(&self, [x0, y0, x1, y1]: [usize; 4]) -> bool {
        let [min_x, min_y, max_x, max_y] = self.bound;
        let min = |v: [f32; 4]| v.iter().cloned().fold(f32::MAX, f32::min);
        let max = |v: [f32; 4]| v.iter().cloned().fold(f32::MIN, f32::max);
        (x1 as i32) > min_x
            && (x0 as i32) < max_x + 4
            && (y1 as i32) > min_y
            && (y0 as i32) < max_y + 4
            && (x1 as f32) >= min(self.fx) - 1.0
            && (x0 as f32) <= max(self.fx) + 1.0
            && (y1 as f32) >= min(self.fy) - 1.0
            && (y0 as f32) <= max(self.fy) + 1.0
    }
}

/// `CreateFrustrum2D` trapezoid measurements in MU: (far, near, target, far width, near width).
/// `hero` is the character's MU position. With `g_shCameraLevel` as `MoveMainCamera` leaves it
/// (5 in Vulcanus and Doppelganger 1 and 2, 0 elsewhere) and `GetScreenWidth() = 640`.
pub fn frustum2d_params(world: u16, hero: Vec2) -> [f32; 5] {
    let ratio = |far: f32| [far, far * 0.19, far * 0.47];
    match active(world) {
        // Valley of Loren (`InBattleCastle`, 1968-1987).
        30 => {
            if crate::camera::in_battle_castle2(hero) && (hero.x < 17100.0 || hero.x > 18300.0) {
                let [f, n, t] = ratio(5100.0);
                [f, n, t, 2250.0, 540.0]
            } else {
                let [f, n, t] = ratio(3300.0);
                [f, n, t, 1300.0, 580.0]
            }
        }
        // Santa Town (1988-1997): near and target come from 2400 and far is later changed to 2650.
        62 => {
            let [_, n, t] = ratio(2400.0);
            [2650.0, n, t, 1250.0, 540.0]
        }
        // Vulcanus and Doppelganger 2 (1998-2007).
        63 | 66 => [3300.0, 55.0, 830.0, 1900.0, 600.0],
        // Doppelganger 1: `g_shCameraLevel = 5` (ZzzScene.cpp:1725-1728) → case 4/5 (2091-2099).
        65 => {
            let width = 1.0 + 0.1;
            let [f, n, t] = ratio(3400.0);
            [f, n, t, 1600.0 * width, 660.0 * width]
        }
        // By the culling `CameraLevel` (2010-2099). Level 0: Kanturu 3 with 2000 · 10 · 0.115, the
        // rest 2400; the others use `Width = 1 + 0.1`.
        a => {
            let wide = 1.0 + 0.1;
            match crate::camera::cull_level(world) {
                1 => {
                    let [f, n, t] = ratio(2700.0);
                    [f, n, t, 1200.0 * wide, 540.0 * wide]
                }
                2 => {
                    let [f, n, t] = ratio(3000.0);
                    [f, n, t, 1300.0 * wide, 540.0 * wide]
                }
                3 => {
                    let [f, n, t] = ratio(3300.0);
                    [f, n, t, 1500.0 * wide, 580.0 * wide]
                }
                4 | 5 => {
                    let [f, n, t] = ratio(3400.0);
                    [f, n, t, 1600.0 * wide, 660.0 * wide]
                }
                _ => {
                    let far = if a == 39 { 2000.0 * 10.0 * 0.115 } else { 2400.0 };
                    let [f, n, t] = ratio(far);
                    [f, n, t, 1190.0, 540.0]
                }
            }
        }
    }
}

/// Trapezoid corners in tiles around `hero` (MU), rotated 45° (`Vector(0, 0, 45)`).
pub fn trapezoid(world: u16, hero: Vec2) -> ([f32; 4], [f32; 4]) {
    let [far, near, target, wf, wn] = frustum2d_params(world, hero);
    let p = [[-wf, far - target], [wf, far - target], [wn, near - target], [-wn, near - target]];
    let (s, c) = 45f32.to_radians().sin_cos();
    let mut fx = [0.0; 4];
    let mut fy = [0.0; 4];
    for (i, [x, y]) in p.into_iter().enumerate() {
        // `AngleMatrix` with only Z rotation + `VectorRotate`.
        fx[i] = (x * c - y * s + hero.x) * 0.01;
        fy[i] = (x * s + y * c + hero.y) * 0.01;
    }
    (fx, fy)
}

/// `FrustrumBoundMin/Max` of `CreateFrustrum` (ZzzLodTerrain.cpp:2159-2201): the box of the camera
/// frustum up to `CameraViewFar · 0.9`, in blocks of 4 tiles with one of margin.
pub fn frustum_bounds(camera: &Transform, view_far: f32) -> [i32; 4] {
    let distance = view_far * 0.9;
    let width = (crate::camera::mu_fov() * 0.5).tan() * distance + 100.0;
    let height = width * 3.0 / 4.0;
    let points = [
        Vec3::ZERO,
        Vec3::new(-width, height, -distance),
        Vec3::new(width, height, -distance),
        Vec3::new(width, -height, -distance),
        Vec3::new(-width, -height, -distance),
    ];
    let (mut min, mut max) = (Vec2::splat((SIZE as f32) * MU_SCALE), Vec2::ZERO);
    for p in points {
        // View (x right, y up, -z forward) → Bevy → MU (x, -z).
        let w = camera.translation + camera.rotation * (p / MU_SCALE);
        let mu = Vec2::new(w.x, -w.z) * MU_SCALE;
        min = min.min(mu);
        max = max.max(mu);
    }
    let tile = 4;
    let mask = SIZE as i32 - 1;
    let lo = |v: f32| (((v / MU_SCALE) as i32) / tile * tile - tile).clamp(0, mask - tile);
    let hi = |v: f32| (((v / MU_SCALE) as i32) / tile * tile + tile).clamp(0, mask - tile);
    [lo(min.x), lo(min.y), hi(max.x), hi(max.y)]
}

/// Computes this frame's trapezoid (only in game, with a character and outside Icarus).
fn update_cull(
    args: Res<Args>,
    state: Res<State<crate::scene::GameScene>>,
    players: Query<&Transform, With<crate::character::Player>>,
    cameras: Query<&Transform, (With<Camera3d>, With<IsDefaultUiCamera>, Without<crate::character::Player>)>,
    mut cull: ResMut<TerrainCull>,
) {
    let shape = match (*state.get() == crate::scene::GameScene::Game, players.single(), cameras.single()) {
        (true, Ok(p), Ok(cam)) => {
            let hero = Vec2::new(p.translation.x, -p.translation.z) * MU_SCALE;
            let (fx, fy) = trapezoid(args.world, hero);
            let bound = frustum_bounds(cam, crate::camera::view_far(args.world, hero));
            Some(CullShape { fx, fy, bound })
        }
        _ => None,
    };
    if cull.shape != shape {
        cull.shape = shape;
        cull.generation += 1;
    }
}

/// Leaves in each chunk only the tiles that are drawn.
fn cull_chunks(cull: Res<TerrainCull>, mut chunks: Query<(&mut TerrainChunk, &mut Visibility)>, mut meshes: ResMut<Assets<Mesh>>) {
    for (mut c, mut vis) in &mut chunks {
        if c.cull_generation == cull.generation {
            continue;
        }
        c.cull_generation = cull.generation;
        let may = cull.area_may_be_visible(c.area);
        let shown: Vec<bool> = (0..c.shown.len())
            .map(|q| {
                let [x, y] = c.corners[q * 4];
                may && cull.tile_visible(x as usize, y as usize)
            })
            .collect();
        let any = shown.iter().any(|&s| s);
        vis.set_if_neq(if any { Visibility::Inherited } else { Visibility::Hidden });
        if shown == c.shown {
            continue;
        }
        if any && let Some(mut m) = meshes.get_mut(&c.mesh) {
            let idx: Vec<u32> = (0..shown.len() as u32).filter(|&q| shown[q as usize]).flat_map(|q| quad_indices(q * 4)).collect();
            m.insert_indices(Indices::U32(idx));
        }
        if any {
            c.shown = shown;
        }
    }
}

// --- Dynamic lights -------------------------------------------------------------------------------

/// A light added to the terrain during one update (`AddTerrainLight`).
#[derive(Clone, Copy)]
pub struct DynamicLight {
    /// Position in tiles.
    pub pos: Vec2,
    pub color: [f32; 3],
    /// Radius in tiles.
    pub range: f32,
}

/// Dynamic lights of the current update. Filled by the `FixedUpdate` systems
/// (fires, events...) after [`clear_lights`] empties it at the start of the tick.
#[derive(Resource, Default)]
pub struct TerrainLights {
    pub lights: Vec<DynamicLight>,
    /// Changes every tick, to know whether to recompute.
    generation: u64,
    applied: u64,
}

impl TerrainLights {
    pub fn add(&mut self, pos: Vec2, color: [f32; 3], range: f32) {
        self.lights.push(DynamicLight { pos, color, range });
    }
}

/// At the start of each tick, the previous tick's lights go away (like `InitTerrainLight`).
pub fn clear_lights(mut lights: ResMut<TerrainLights>) {
    lights.lights.clear();
    lights.generation += 1;
}

/// Recomputes the color of the chunks touched by this tick's lights (or that had them before).
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
        // Caustics are unlit (`VertexBlend`).
        if c.pass == Pass::Caustic {
            continue;
        }
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

/// The original multiplies the light in sRGB space; in linear that means raising it to 2.2.
fn vertex_color(l: [f32; 3], alpha: f32) -> [f32; 4] {
    let c = |v: f32| v.clamp(0.0, 1.0).powf(2.2);
    [c(l[0]), c(l[1]), c(l[2]), alpha]
}

/// `mu-data` texture → repeating Bevy image, with mipmaps
/// (without mipmaps the distant ground flickers).
pub fn to_bevy_image(tex: &Texture) -> Image {
    let (data, levels) = build_mipmaps(tex);
    let mut image = Image::new(
        Extent3d { width: tex.width, height: tex.height, depth_or_array_layers: 1 },
        TextureDimension::D2,
        tex.rgba.clone(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    // `Image::new` only accepts level 0; the mipmaps all follow right after.
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

/// All mipmap levels back to back, halving with a 2x2 average.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn casillas_como_render_terrain_face() {
        // Partial blend: base layer 1, layer 2 on top.
        let d = tile_draw(1, 0, 2, [0.0, 0.5, 1.0, 0.0]);
        assert_eq!(d.base, Some((0, false)));
        assert_eq!(d.over, Some((Pass::Alpha, 2, false)));
        // Full: only layer 2, opaque and without water even if it is 5.
        let d = tile_draw(1, 0, WATER_TEXTURE, [1.0; 4]);
        assert_eq!(d.base, Some((WATER_TEXTURE, false)));
        assert_eq!(d.over, None);
        // Layer 2 water over water: scrolls; over something else, it does not.
        assert_eq!(tile_draw(1, 5, 5, [0.5; 4]).over, Some((Pass::Alpha, 5, true)));
        assert_eq!(tile_draw(1, 0, 5, [0.5; 4]).over, Some((Pass::Alpha, 5, false)));
        // Atlans: caustics where layer 2 is water (also if full), without layer 2 on top.
        let d = tile_draw(8, 0, 5, [0.3; 4]);
        assert_eq!(d.base, Some((0, false)));
        assert_eq!(d.over, Some((Pass::Caustic, 5, false)));
        assert_eq!(tile_draw(8, 0, 5, [1.0; 4]).over, Some((Pass::Caustic, 5, false)));
        // No layer 2: nothing on top.
        assert_eq!(tile_draw(1, 3, NO_TEXTURE, [0.0; 4]).over, None);
        // Vulcanus lava.
        assert_eq!(tile_draw(64, 11, NO_TEXTURE, [0.0; 4]).base, Some((11, true)));
        assert_eq!(tile_draw(1, 11, NO_TEXTURE, [0.0; 4]).base, Some((11, false)));
        // Kanturu 3: 100 is not in the normal pass.
        assert_eq!(tile_draw(40, 100, NO_TEXTURE, [0.0; 4]).base, None);
    }

    #[test]
    fn agua_por_mapa() {
        assert!((water_move(1, 10000) - 0.5).abs() < 1e-6);
        assert!((water_move(9, 20000) - 0.5).abs() < 1e-6);
        assert!((water_move(43, 25000) - 0.5).abs() < 1e-6);
        assert!(water_move(1, 20000).abs() < 1e-6);
    }

    #[test]
    fn trapecio_alrededor_del_personaje() {
        let hero = Vec2::new(13000.0, 13000.0);
        let (fx, fy) = trapezoid(1, hero);
        let s = CullShape { fx, fy, bound: [0, 0, 251, 251] };
        // The character and what is in front (towards -x, +y) are visible; very far things are not.
        assert!(s.test(130.5, 130.5, 0.0));
        assert!(s.test(130.0 - 8.0, 130.0 + 8.0, 0.0));
        assert!(!s.test(130.0 - 12.0, 130.0 + 12.0, 0.0));
        // Behind only up to 672 units.
        assert!(s.test(130.0 + 4.0, 130.0 - 4.0, 0.0));
        assert!(!s.test(130.0 + 5.5, 130.0 - 5.5, 0.0));
        assert!(s.tile_visible(130, 130));
        // Outside the `CreateFrustrum` block bounds, nothing.
        let s = CullShape { fx, fy, bound: [128, 128, 132, 132] };
        assert!(!s.tile_visible(120, 138));
    }
}
