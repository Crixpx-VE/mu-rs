//! Convierte el terreno de `mu-data` en mallas de Bevy.
//!
//! Igual que el cliente original (`RenderTerrainFace`, ZzzLodTerrain.cpp:1514-1565), cada casilla
//! se dibuja en dos pasadas:
//! 1. Base: si las cuatro esquinas tienen la capa 2 entera (alfa 1), la textura de la capa 2;
//!    si no, la de la capa 1 (opaca, o con prueba de alfa en las texturas de `RenderFace`).
//! 2. Encima, si alguna esquina tiene algo de capa 2: en Atlans (y Doppelganger 3) donde la capa 2
//!    es agua, las cáusticas animadas `Object8\wtNN` sumadas; en el resto, si la casilla no era
//!    entera, la capa 2 con la transparencia de cada vértice (`RenderFaceAlpha`: mezcla + prueba de alfa).
//!
//! El agua (`TileWater01`, y la lava de Vulcanus) avanza en U (`WaterMove`) y se ondula en V con el
//! viento de la hierba (`FaceTexture`, 1474-1508). Solo se dibujan las casillas que caen dentro del
//! trapecio del suelo del original (`CreateFrustrum2D` / `TestFrustrum2D` en los bloques de 4×4 y
//! en cada casilla, dentro de los límites de `CreateFrustrum`): ver [`TerrainCull`].
//!
//! Se agrupan las casillas por pasada y textura en trozos de 16×16.
//!
//! Ejes: MU usa Z hacia arriba y Bevy usa Y hacia arriba, así que
//! MU (x, y, z) → Bevy (x, z, -y). Una casilla mide 1 unidad (100 en MU).

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

/// Unidades de MU por unidad de Bevy (una casilla = 100 en MU).
pub const MU_SCALE: f32 = 100.0;

/// El terreno cargado, para que otros sistemas (cámara, personajes) consulten alturas.
#[derive(Resource)]
pub struct LoadedTerrain(pub Terrain);

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        // El sombreador de los efectos, registrado ya aquí: este plugin va antes que el de los efectos y
        // el material lo pide al crearse (registrarlo dos veces no pasa nada).
        bevy::asset::embedded_asset!(app, "fx/fx.wesl");
        app.add_plugins(MaterialPlugin::<LayerMaterial>::default())
            .init_resource::<TerrainLights>()
            .init_resource::<TerrainCull>()
            .init_resource::<Caustics>()
            .add_systems(Update, spawn_terrain.run_if(crate::scene::load_step(crate::scene::Step::Terrain)))
            // Las luces se vacían al empezar cada tick; los emisores las rellenan después.
            .add_systems(FixedFirst, clear_lights)
            // `WaterTextureNumber++` en cada vuelta de la lógica (ZzzScene.cpp:2315-2316).
            .add_systems(FixedUpdate, |mut c: ResMut<Caustics>| c.frame = (c.frame + 1) % CAUSTIC_FRAMES)
            .add_systems(Update, (apply_lights, show_caustics))
            // Con la cámara ya colocada en este fotograma y antes de decidir qué se ve.
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

/// Cuándo queda decidido qué casillas del suelo se dibujan en este fotograma (la hierba va después).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct TerrainCulled;

/// Textura del terreno que es agua en todos los mapas (`TileWater01`).
const WATER_TEXTURE: u8 = 5;
/// La lava de Vulcanus y Doppelganger 2 (`Object64\song_lava1.jpg`), que corre como el agua.
const LAVA_TEXTURE: u8 = 11;
/// `BITMAP_WATER`: 32 texturas `Object8\wt00-31.jpg`.
const CAUSTIC_FRAMES: u32 = 32;

/// `WorldActive` del original: nuestro número de mapa menos uno.
fn active(world: u16) -> i32 {
    world as i32 - 1
}

/// Vulcanus (`IsPKField`) o Doppelganger 2 (`IsDoppelGanger2`).
pub fn pk_field(world: u16) -> bool {
    matches!(active(world), 63 | 66)
}

/// Mapas con cáusticas sobre el agua de la capa 2: Atlans y Doppelganger 3 (ZzzLodTerrain.cpp:1547).
fn caustic_world(world: u16) -> bool {
    matches!(active(world), 7 | 67)
}

/// Icarus y Kalima: el original no dibuja el suelo (`RenderTerrain` no se llama, ZzzScene.cpp:2108-2116).
/// En Kalima (`IsWaterTerrain`) va en su lugar el mar de [`crate::water_terrain`].
pub fn hidden_terrain(world: u16) -> bool {
    active(world) == 10 || crate::water_terrain::is_water_world(world)
}

/// `WaterMove` (`RenderTerrain`, ZzzLodTerrain.cpp:2589-2602): fracción de textura que ha avanzado el
/// agua a los `ms` milisegundos. Tarkan da la vuelta en 40 s, Barracas de Balgass (`WD_42`) en 50 s y
/// el resto en 20 s.
pub fn water_move(world: u16, ms: i64) -> f32 {
    match active(world) {
        8 => (ms % 40000) as f32 * 0.000025,
        42 => (ms % 50000) as f32 * 0.00002,
        _ => (ms % 20000) as f32 * 0.00005,
    }
}

/// Milisegundos como `WorldTime` (`timeGetTime`).
pub fn world_ms(time: &Time) -> i64 {
    (time.elapsed_secs_f64() * 1000.0) as i64
}

// --- Material de las capas de encima --------------------------------------------------------------

/// Material de lo que va encima del suelo base: la capa 2 (`EnableAlphaTest`: mezcla con alfa y prueba
/// de alfa > 0.25, escribe profundidad) o algo sumado (`EnableAlphaBlend`: ONE, ONE, sin escribir
/// profundidad), como las cáusticas o la hierba de Vulcanus. Reutiliza el sombreador de los efectos
/// (textura × color del vértice × tinte). Va en la pasada de recorte de Bevy, justo después del suelo
/// opaco y antes de todo lo transparente, que es el orden del original (el terreno va lo primero).
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
            // Misma geometría que la casilla de debajo: un poco hacia la cámara para que no parpadee.
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

// --- Cáusticas de Atlans -------------------------------------------------------------------------

/// Cáusticas del mapa cargado: el material y sus 32 fotogramas. `frame` es `WaterTextureNumber`.
#[derive(Resource, Default)]
pub struct Caustics {
    material: Option<Handle<LayerMaterial>>,
    frames: Vec<Handle<Image>>,
    frame: u32,
    shown: Option<u32>,
}

/// Pone el fotograma del tick en el material de las cáusticas.
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

// --- Construcción ---------------------------------------------------------------------------------

/// Qué pasada dibuja un trozo.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Pass {
    /// `RenderFace`: la textura base de la casilla.
    Base,
    /// `RenderFaceAlpha`: la capa 2 con su transparencia.
    Alpha,
    /// `RenderFaceBlend` de Atlans: las cáusticas sumadas, con el alfa como color.
    Caustic,
}

/// Lo que dibuja cada casilla, decidido como `RenderTerrainFace` (ZzzLodTerrain.cpp:1514-1565).
#[derive(Clone, Copy, PartialEq, Debug)]
struct TileDraw {
    /// Textura de la pasada base y si es agua (se mueve).
    base: Option<(u8, bool)>,
    /// La pasada de encima: cáusticas, o la capa 2 con si es agua.
    over: Option<(Pass, u8, bool)>,
}

/// Decide qué dibuja la casilla (x, y): `alpha` son las cuatro esquinas (0..1) en el orden del original.
fn tile_draw(world: u16, layer1: u8, layer2: u8, alpha: [f32; 4]) -> TileDraw {
    let full = alpha.iter().all(|&a| a >= 1.0);
    let any = alpha.iter().any(|&a| a > 0.0);
    // Casilla entera: la textura de la capa 2, sin agua. Si no, la de la capa 1 (agua: la 5, y la lava
    // en Vulcanus / Doppelganger 2).
    let (base, water) = if full && layer2 != NO_TEXTURE {
        (layer2, false)
    } else {
        (layer1, layer1 == WATER_TEXTURE || (layer1 == LAVA_TEXTURE && pk_field(world)))
    };
    // Kanturu 3: la 100 no se dibuja en la pasada normal (`RenderFace`, 1336-1337).
    let base = (!(active(world) == 39 && base == 100)).then_some((base, water));
    let over = if !any {
        None
    } else if caustic_world(world) && layer2 == WATER_TEXTURE {
        Some((Pass::Caustic, layer2, false))
    } else if !full && layer2 != NO_TEXTURE {
        // La capa 2 solo es agua si la base también lo era.
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
        // Las alturas siguen haciendo falta (personajes, efectos), pero el suelo no se ve.
        commands.insert_resource(LoadedTerrain(t));
        return;
    }

    // Imagen y escala de UV (`64 / ancho`, `FaceTexture`) de cada textura usada.
    let mut tex: HashMap<u8, (Handle<Image>, Vec2)> = HashMap::new();
    for (index, texture) in t.textures.iter().enumerate() {
        let Some(texture) = texture else { continue };
        let uv = Vec2::new(64.0 / texture.width as f32, 64.0 / texture.height as f32);
        tex.insert(index as u8, (images.add(to_bevy_image(texture)), uv));
    }

    // Cáusticas: `Object8\wtNN.jpg` (MapManager.cpp:132-147), UV de `16 / ancho` (`FaceTexture` con `Scale`).
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
                                // `RenderFace`: esta textura va con prueba de alfa (rejas, huecos).
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

/// Casillas de `area` agrupadas por (pasada, textura, agua), en orden.
fn chunk_groups(t: &Terrain, world: u16, area: [usize; 4]) -> Vec<((Pass, u8, bool), Vec<[usize; 2]>)> {
    let m = &t.mapping;
    let mut groups: Vec<((Pass, u8, bool), Vec<[usize; 2]>)> = Vec::new();
    let mut push = |key: (Pass, u8, bool), x: usize, y: usize| match groups.iter_mut().find(|g| g.0 == key) {
        Some(g) => g.1.push([x, y]),
        None => groups.push((key, vec![[x, y]])),
    };
    for y in area[1]..area[3] {
        for x in area[0]..area[2] {
            // `RenderTerrainTile`: las casillas `TW_NOGROUND` no se dibujan.
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

/// Esquinas de la casilla en el orden del original (`TerrainIndex1..4`): (x, y), (x+1, y), (x+1, y+1), (x, y+1).
fn corners(x: usize, y: usize) -> [(usize, usize); 4] {
    [(x, y), (x + 1, y), (x + 1, y + 1), (x, y + 1)]
}

fn corner_alpha(t: &Terrain, x: usize, y: usize) -> [f32; 4] {
    corners(x, y).map(|(cx, cy)| t.mapping.alpha[cy * SIZE + cx] as f32 / 255.0)
}

/// Lado de los trozos del terreno, en casillas.
const CHUNK: usize = 16;

/// Un trozo del terreno, con lo necesario para recalcular su luz, su agua y qué casillas se ven.
#[derive(Component)]
pub struct TerrainChunk {
    mesh: Handle<Mesh>,
    pass: Pass,
    /// Esquina (x, y) de cada vértice (4 por casilla, en orden).
    corners: Vec<[u8; 2]>,
    /// Transparencia de cada vértice (capa 2).
    alpha: Vec<f32>,
    /// Casillas que cubre: [x0, y0, x1, y1].
    area: [usize; 4],
    /// Tenía luces dinámicas en la última actualización (hay que devolverlo a su luz normal).
    lit: bool,
    /// UV sin mover, si es agua (`FaceTexture` con `Water`).
    water_uv: Option<Vec<[f32; 2]>>,
    /// Si es agua de `TileWater01` (en el Valle de Loren corre al revés; la lava no).
    water_tile: bool,
    /// Casillas que se dibujan ahora mismo (una por cada cuatro vértices).
    shown: Vec<bool>,
    /// Generación de [`TerrainCull`] ya aplicada.
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
                // `VertexBlend`: el color es el alfa, sin luz.
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

/// Los dos triángulos de la casilla que empieza en el vértice `base` (`GL_TRIANGLE_FAN` 0-1-2-3).
pub fn quad_indices(base: u32) -> [u32; 6] {
    [base, base + 1, base + 2, base, base + 2, base + 3]
}

/// Al cambiar de mapa: nada del anterior.
pub fn reset(mut lights: ResMut<TerrainLights>, mut caustics: ResMut<Caustics>, mut cull: ResMut<TerrainCull>) {
    lights.lights.clear();
    *caustics = Caustics::default();
    cull.shape = None;
    cull.generation += 1;
}

// --- Agua -----------------------------------------------------------------------------------------

/// Mueve el agua: `u ± WaterMove` y `v + TerrainGrassWind · 0.002` en cada vértice (`FaceTexture`).
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
        // El Valle de Loren (`WD_30BATTLECASTLE`) corre al revés, solo en la textura 5 (1489-1492).
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

// --- Qué casillas se ven ----------------------------------------------------------------------------

/// El trapecio del suelo del original para este fotograma (`None` = sin recortar: fuera del juego).
#[derive(Resource, Default)]
pub struct TerrainCull {
    pub shape: Option<CullShape>,
    /// Cambia cada vez que cambia `shape`.
    pub generation: u64,
}

impl TerrainCull {
    /// Si la casilla (x, y) se dibuja (`RenderTerrainFrustrum` + `RenderTerrainBlock`).
    pub fn tile_visible(&self, x: usize, y: usize) -> bool {
        self.shape.as_ref().is_none_or(|s| s.tile_visible(x, y))
    }

    /// Si algo de `area` ([x0, y0, x1, y1), en casillas) puede verse.
    pub fn area_may_be_visible(&self, area: [usize; 4]) -> bool {
        self.shape.as_ref().is_none_or(|s| s.overlaps(area))
    }
}

/// `CreateFrustrum2D` (en casillas) y los límites de bloques de `CreateFrustrum`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CullShape {
    /// `FrustrumX` / `FrustrumY`: las cuatro esquinas del trapecio, en casillas.
    pub fx: [f32; 4],
    pub fy: [f32; 4],
    /// `FrustrumBoundMinX`, `MinY`, `MaxX`, `MaxY`: primer y último bloque de 4×4 que se recorre.
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

    /// `RenderTerrainFrustrum`: bloques de 4×4 desde `FrustrumBoundMin` hasta `Max` con
    /// `TestFrustrum2D(centro, -40)` (`g_fFrustumRange`), y en cada bloque las casillas con
    /// `TestFrustrum2D(centro, 0)` (`RenderTerrainBlock`, 2485-2540).
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

    /// Descarte rápido de un trozo entero.
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

/// Medidas del trapecio de `CreateFrustrum2D` en MU: (far, near, target, ancho lejos, ancho cerca).
/// `hero` es la posición del personaje en MU. Con `g_shCameraLevel` como lo deja `MoveMainCamera`
/// (5 en Vulcanus y los Doppelganger 1 y 2, 0 en el resto) y `GetScreenWidth() = 640`.
pub fn frustum2d_params(world: u16, hero: Vec2) -> [f32; 5] {
    let ratio = |far: f32| [far, far * 0.19, far * 0.47];
    match active(world) {
        // Valle de Loren (`InBattleCastle`, 1968-1987).
        30 => {
            if crate::camera::in_battle_castle2(hero) && (hero.x < 17100.0 || hero.x > 18300.0) {
                let [f, n, t] = ratio(5100.0);
                [f, n, t, 2250.0, 540.0]
            } else {
                let [f, n, t] = ratio(3300.0);
                [f, n, t, 1300.0, 580.0]
            }
        }
        // Santa Town (1988-1997): near y target salen de 2400 y el far se cambia luego a 2650.
        62 => {
            let [_, n, t] = ratio(2400.0);
            [2650.0, n, t, 1250.0, 540.0]
        }
        // Vulcanus y Doppelganger 2 (1998-2007).
        63 | 66 => [3300.0, 55.0, 830.0, 1900.0, 600.0],
        // Doppelganger 1: `g_shCameraLevel = 5` (ZzzScene.cpp:1725-1728) → caso 4/5 (2091-2099).
        65 => {
            let width = 1.0 + 0.1;
            let [f, n, t] = ratio(3400.0);
            [f, n, t, 1600.0 * width, 660.0 * width]
        }
        // Según el `CameraLevel` del recorte (2010-2099). Nivel 0: Kanturu 3 con 2000 · 10 · 0.115, el
        // resto 2400; los demás llevan `Width = 1 + 0.1`.
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

/// Esquinas del trapecio en casillas alrededor de `hero` (MU), giradas 45° (`Vector(0, 0, 45)`).
pub fn trapezoid(world: u16, hero: Vec2) -> ([f32; 4], [f32; 4]) {
    let [far, near, target, wf, wn] = frustum2d_params(world, hero);
    let p = [[-wf, far - target], [wf, far - target], [wn, near - target], [-wn, near - target]];
    let (s, c) = 45f32.to_radians().sin_cos();
    let mut fx = [0.0; 4];
    let mut fy = [0.0; 4];
    for (i, [x, y]) in p.into_iter().enumerate() {
        // `AngleMatrix` solo con giro en Z + `VectorRotate`.
        fx[i] = (x * c - y * s + hero.x) * 0.01;
        fy[i] = (x * s + y * c + hero.y) * 0.01;
    }
    (fx, fy)
}

/// `FrustrumBoundMin/Max` de `CreateFrustrum` (ZzzLodTerrain.cpp:2159-2201): la caja de la pirámide de
/// la cámara hasta `CameraViewFar · 0.9`, en bloques de 4 casillas con uno de margen.
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
        // Vista (x derecha, y arriba, -z delante) → Bevy → MU (x, -z).
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

/// Calcula el trapecio de este fotograma (solo en el juego, con personaje y fuera de Icarus).
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

/// Deja en cada trozo solo las casillas que se dibujan.
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

// --- Luces dinámicas ------------------------------------------------------------------------------

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
        // Las cáusticas no llevan luz (`VertexBlend`).
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

/// El original multiplica la luz en espacio sRGB; en lineal equivale a elevarla a 2.2.
fn vertex_color(l: [f32; 3], alpha: f32) -> [f32; 4] {
    let c = |v: f32| v.clamp(0.0, 1.0).powf(2.2);
    [c(l[0]), c(l[1]), c(l[2]), alpha]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn casillas_como_render_terrain_face() {
        // Mezcla parcial: base capa 1, encima capa 2.
        let d = tile_draw(1, 0, 2, [0.0, 0.5, 1.0, 0.0]);
        assert_eq!(d.base, Some((0, false)));
        assert_eq!(d.over, Some((Pass::Alpha, 2, false)));
        // Entera: solo la capa 2, opaca y sin agua aunque sea la 5.
        let d = tile_draw(1, 0, WATER_TEXTURE, [1.0; 4]);
        assert_eq!(d.base, Some((WATER_TEXTURE, false)));
        assert_eq!(d.over, None);
        // Agua de capa 2 sobre agua: se mueve; sobre otra cosa, no.
        assert_eq!(tile_draw(1, 5, 5, [0.5; 4]).over, Some((Pass::Alpha, 5, true)));
        assert_eq!(tile_draw(1, 0, 5, [0.5; 4]).over, Some((Pass::Alpha, 5, false)));
        // Atlans: cáusticas donde la capa 2 es agua (también si es entera), sin la capa 2 encima.
        let d = tile_draw(8, 0, 5, [0.3; 4]);
        assert_eq!(d.base, Some((0, false)));
        assert_eq!(d.over, Some((Pass::Caustic, 5, false)));
        assert_eq!(tile_draw(8, 0, 5, [1.0; 4]).over, Some((Pass::Caustic, 5, false)));
        // Sin capa 2: nada encima.
        assert_eq!(tile_draw(1, 3, NO_TEXTURE, [0.0; 4]).over, None);
        // Lava de Vulcanus.
        assert_eq!(tile_draw(64, 11, NO_TEXTURE, [0.0; 4]).base, Some((11, true)));
        assert_eq!(tile_draw(1, 11, NO_TEXTURE, [0.0; 4]).base, Some((11, false)));
        // Kanturu 3: la 100 no va en la pasada normal.
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
        // El personaje y lo que tiene delante (hacia -x, +y) se ven; lo de muy lejos, no.
        assert!(s.test(130.5, 130.5, 0.0));
        assert!(s.test(130.0 - 8.0, 130.0 + 8.0, 0.0));
        assert!(!s.test(130.0 - 12.0, 130.0 + 12.0, 0.0));
        // Detrás solo hasta 672 unidades.
        assert!(s.test(130.0 + 4.0, 130.0 - 4.0, 0.0));
        assert!(!s.test(130.0 + 5.5, 130.0 - 5.5, 0.0));
        assert!(s.tile_visible(130, 130));
        // Fuera de los límites de bloques de `CreateFrustrum`, nada.
        let s = CullShape { fx, fy, bound: [128, 128, 132, 132] };
        assert!(!s.tile_visible(120, 138));
    }
}
