//! El mar de Kalima (`CSWaterTerrain`, CSWaterTerrain.cpp; `GMHellas.cpp:42-125`).
//!
//! En Kalima 1-7 (`InHellas`) el suelo normal no se dibuja (`IsWaterTerrain`, ZzzScene.cpp:2108):
//! en su lugar, después de todo lo demás (ZzzScene.cpp:2184-2207), una rejilla de 42×42 vértices
//! cada 50 unidades (media casilla) alrededor del personaje cuya altura son ondas:
//!
//! - Dos páginas de ondas que se propagan (`calcWave`, el agua clásica de dos búferes): cada 40
//!   vueltas se suma una ola delante del personaje (`Update`, :41-46) y las piedras que caen, las
//!   habilidades... suman las suyas con [`add_water_wave`] (`AddWaterWave`).
//! - Dos oleajes de senos que dependen del tiempo (`calcBaseWave`, :224-264).
//!
//! Se dibuja en dos pasadas (`Render`, :53-92), las dos con coordenadas de textura sacadas de la
//! normal de cada vértice (`g_chrome`): `Object25\water1.tga` con prueba de alfa y color
//! (0.2, 0.5, 0.65), y encima `Object25\water2.jpg` sumando con el color `(a, 2.5a, 3a)`,
//! `a = 1 − N·(1, −1, 1)`.
//!
//! La lógica (`Update`) va a 25 ticks por segundo como el resto; el original la hace en cada
//! fotograma, que en el cliente S6 era cada vuelta de la lógica. Los oleajes de senos, que solo
//! dependen de `WorldTime`, se calculan en cada fotograma para que se muevan suaves.

use std::sync::Mutex;

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use crate::Args;
use crate::fx::core::Rng;
use crate::fx::material::{Blend, FxMaterial};
use crate::terrain::{LayerMaterial, MU_SCALE, to_bevy_image};

/// `WATER_TERRAIN_SIZE` (_define.h:560): lado de la tabla de ondas (media casilla cada celda).
pub const WATER_TERRAIN_SIZE: usize = 256;
/// `MAX_WATER_GRID` (_define.h:562): vértices por lado de la rejilla que se dibuja.
pub const MAX_WATER_GRID: usize = 42;
/// `WAVE_SCALE` (_define.h:564): unidades de MU entre dos celdas.
pub const WAVE_SCALE: f32 = 50.0;
/// Altura del agua en reposo (`+350.f` de `CreateTerrain` y `GetWaterTerrain`).
const BASE_HEIGHT: f32 = 350.0;
/// `m_vLightVector` (`Init`, :28): sin normalizar.
const LIGHT_VECTOR: Vec3 = Vec3::new(1.0, -1.0, 1.0);
/// `glColor3f(0.2f, 0.5f, 0.65f)` de la primera pasada (:72).
const FIRST_PASS_COLOR: [f32; 3] = [0.2, 0.5, 0.65];

pub struct WaterTerrainPlugin;

impl Plugin for WaterTerrainPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WaterTerrain>()
            .add_systems(Update, spawn_water.run_if(crate::scene::load_step(crate::scene::Step::Terrain)))
            // Después de los efectos: lo que piden en el tick (`AddWaterWave`) entra en la página actual.
            .add_systems(FixedUpdate, update_water.after(crate::fx::sim::tick))
            .add_systems(Update, render_water);
    }
}

/// `InHellas`: Kalima 1-6 (`WD_24HELLAS..WD_24HELLAS_END`) y la Kalima oculta (`WD_24HELLAS_7` = 36).
pub fn is_water_world(world: u16) -> bool {
    crate::boids::in_hellas(world as i32 - 1)
}

/// `AddWaterWave(x, y, range, height)` (GMHellas.cpp:69-77): ola en la casilla (x, y) del mapa.
/// Se aplica en el próximo tick del agua (el original la suma en el momento a la página actual).
pub fn add_water_wave(x: i32, y: i32, range: i32, height: i32) {
    PENDING.lock().unwrap_or_else(|e| e.into_inner()).push((x * 2, y * 2, range, height));
}

/// Olas pedidas desde fuera (efectos): (x, y en celdas, radio, altura).
static PENDING: Mutex<Vec<(i32, i32, i32, i32)>> = Mutex::new(Vec::new());

/// `g_pCSWaterTerrain`: las ondas del mapa cargado (`None` fuera de Kalima). Fuera de un recurso
/// para que lo consulten igual el motor de efectos y los objetos que flotan.
static WAVES: Mutex<Option<Box<Waves>>> = Mutex::new(None);

fn waves() -> std::sync::MutexGuard<'static, Option<Box<Waves>>> {
    WAVES.lock().unwrap_or_else(|e| e.into_inner())
}

/// `GetWaterTerrain(x, y)` (GMHellas.cpp:107-116 → CSWaterTerrain.cpp:290-310) en unidades de MU,
/// con `WorldTime` = `world_time` ms. `None` fuera de Kalima (`g_pCSWaterTerrain == NULL`).
pub fn height_at(x: f32, y: f32, world_time: f32) -> Option<f32> {
    waves().as_ref().map(|w| w.water_height(x, y, world_time))
}

/// Las mallas del agua del mapa cargado (primera y segunda pasada).
#[derive(Resource, Default)]
pub struct WaterTerrain {
    meshes: Option<[Handle<Mesh>; 2]>,
}

/// `m_iWaveHeight[0..1]` y lo que lleva `CSWaterTerrain` de un tick a otro.
pub struct Waves {
    /// Las dos páginas de ondas (`m_iWaveHeight[0]` y `[1]`).
    pages: [Vec<i32>; 2],
    /// `m_iWaterPage`.
    page: usize,
    /// `MoveSceneFrame`: ticks desde que se cargó el mapa.
    frame: u32,
    rng: Rng,
}

impl Default for Waves {
    fn default() -> Self {
        // `Init` (:26-31): todo a 0.
        let n = WATER_TERRAIN_SIZE * WATER_TERRAIN_SIZE;
        Self { pages: [vec![0; n], vec![0; n]], page: 0, frame: 0, rng: Rng::default() }
    }
}

impl Waves {
    /// `addSineWave(x, y, radiusX, radiusY, height)` (:183-222) en la página actual.
    pub fn add_sine_wave(&mut self, mut x: i32, mut y: i32, radius_x: i32, radius_y: i32, height: i32) {
        let size = WATER_TERRAIN_SIZE as i32;
        let length = (1024.0 / radius_x as f32) * (1024.0 / radius_x as f32);
        if x < 0 {
            x = 1 + radius_x + self.rng.rand() % (size - 2 * radius_x - 1);
        }
        if y < 0 {
            y = 1 + radius_y + self.rng.rand() % (size - 2 * radius_y - 1);
        }
        let radsquare = radius_x * radius_y;
        let (mut left, mut right) = (-radius_x, radius_x);
        let (mut top, mut bottom) = (-radius_y, radius_y);
        // Recorte en los bordes.
        if x - radius_x < 1 {
            left -= x - radius_x - 1;
        }
        if y - radius_y < 1 {
            top -= y - radius_y - 1;
        }
        if x + radius_x > size - 1 {
            right -= x + radius_x - size + 1;
        }
        if y + radius_y > size - 1 {
            bottom -= y + radius_y - size + 1;
        }
        let p = &mut self.pages[self.page];
        for cy in top..bottom {
            for cx in left..right {
                let square = cy * cy + cx * cx;
                if square < radsquare {
                    let dist = (square as f32 * length).sqrt();
                    p[(size * (cy + y) + cx + x) as usize] += sine_wave(dist, height);
                }
            }
        }
    }

    /// `calcWave` (:266-288): la página nueva a partir de la vieja (vecinos / 2 − la nueva, y se
    /// amortigua 1/16). Solo las celdas de dentro (sin el borde).
    fn calc_wave(&mut self) {
        let s = WATER_TERRAIN_SIZE;
        let (new, old) = if self.page == 0 {
            let (a, b) = self.pages.split_at_mut(1);
            (&mut a[0], &b[0])
        } else {
            let (a, b) = self.pages.split_at_mut(1);
            (&mut b[0], &a[0])
        };
        let end = (s - 1) * s;
        let mut count = s + 1;
        while count < end {
            let x = count + s - 2;
            while count < x {
                let newh = ((old[count + s] + old[count - s] + old[count + 1] + old[count - 1]) >> 1) - new[count];
                new[count] = newh - (newh >> 4);
                count += 1;
            }
            count += 2;
        }
    }

    /// `Update` (:33-51) sin `calcBaseWave` (ver [`base_waves`]): con el personaje en la casilla `hero`.
    fn update(&mut self, hero: IVec2) {
        if self.frame.is_multiple_of(40) {
            let wave_x = hero.x * 2 + self.rng.rand() % 30 - 15;
            let wave_y = hero.y * 2 + 25;
            self.add_sine_wave(wave_x, wave_y, 20, 2, 2000);
        }
        self.page ^= 1;
        self.calc_wave();
    }

    /// `m_iWaveHeight[m_iWaterPage][offset]`.
    fn ripple(&self, offset: usize) -> i32 {
        self.pages[self.page][offset]
    }

    /// `GetWaterTerrain` (:290-310): `(ondas + 350 + (oleaje1 + 350 + oleaje2 + 350) / 2) / 4`.
    pub fn water_height(&self, xf: f32, yf: f32, world_time: f32) -> f32 {
        // `GMHellas.cpp:109-110`: lo negativo, a 0.
        let max = WATER_TERRAIN_SIZE as i32 - 1;
        let x = ((xf.max(0.0) / MU_SCALE * 2.0) as i32).min(max);
        let y = ((yf.max(0.0) / MU_SCALE * 2.0) as i32).min(max);
        let offset = (x + y * WATER_TERRAIN_SIZE as i32) as usize;
        let height = self.ripple(offset) as f32 + BASE_HEIGHT;
        let (b2, b3) = base_waves(y, x, world_time);
        let height1 = b2 as f32 + BASE_HEIGHT + b3 as f32 + BASE_HEIGHT;
        (height + height1 / 2.0) / 4.0
    }
}

/// `(int)((cos(dist) + 0xffff) * height) >> 19` (:217), en `float` como el original.
pub fn sine_wave(dist: f32, height: i32) -> i32 {
    (((dist.cos() + 65535.0) * height as f32) as i32) >> 19
}

/// `calcBaseWave` (:256-261) de la celda (fila `i`, columna `j`): `m_iWaveHeight[2]` y `[3]`.
/// (`alpha` es siempre 0 en el original.) El original solo las rellena a menos de
/// `VIEW_WATER_GRID / 2` = 50 celdas del personaje; aquí se calculan donde se piden (la rejilla que
/// se dibuja cae siempre dentro de esa zona).
pub fn base_waves(i: i32, j: i32, world_time: f32) -> (i32, i32) {
    let (i, j) = (i as f32, j as f32);
    let max_height = ((world_time * 0.005 + i * 0.1 + j * 0.1).sin() * 50.0) as i32;
    let h2 = (max_height as f32 - (world_time * 0.003 + j * 0.1 + i * 0.5).sin() * 50.0) as i32;
    let max_height = ((world_time * 0.001 + i * 0.5 + j * 0.5).sin() * 25.0) as i32;
    let h3 = (max_height as f32 - (world_time * 0.002 + j * 1.0 + i * 0.3).sin() * 25.0) as i32;
    (h2, h3)
}

/// Los triángulos de la rejilla (`CreateTerrain`, :128-149): en las celdas con `offX` y `offY` de la
/// misma paridad la diagonal va de (0,0) a (1,1); en el resto, de (1,0) a (0,1). La última fila y
/// columna no empiezan celda.
pub fn grid_triangles() -> Vec<u32> {
    let m = MAX_WATER_GRID;
    let mut out = Vec::with_capacity((m - 1) * (m - 1) * 6);
    for off_y in 0..m - 1 {
        for off_x in 0..m - 1 {
            let o = (off_x + off_y * m) as u32;
            let m = m as u32;
            if off_x % 2 == off_y % 2 {
                out.extend_from_slice(&[o, o + 1, o + 1 + m, o, o + 1 + m, o + m]);
            } else {
                out.extend_from_slice(&[o, o + 1, o + m, o + 1, o + 1 + m, o + m]);
            }
        }
    }
    out
}

/// La rejilla de este fotograma (`CreateTerrain`, :94-181), en unidades de MU: vértices y normales.
pub struct Grid {
    pub vertices: Vec<Vec3>,
    pub normals: Vec<Vec3>,
}

/// `CreateTerrain((Hero->PositionX)*2, (Hero->PositionY)*2)` con los oleajes en `world_time`.
pub fn create_terrain(waves: &Waves, x: i32, y: i32, world_time: f32, triangles: &[u32]) -> Grid {
    let m = MAX_WATER_GRID;
    let grid = (m / 2) as i32;
    let size = WATER_TERRAIN_SIZE as i32;
    let mut vertices = vec![Vec3::ZERO; m * m];
    for off_y in 0..m {
        let i = y - grid + 6 + off_y as i32;
        for off_x in 0..m {
            let j = x - grid - 4 + off_x as i32;
            let height = if i < 0 || j < 0 || i >= size || j >= size {
                BASE_HEIGHT
            } else {
                let h = waves.ripple((j + i * size) as usize) as f32 + BASE_HEIGHT;
                let (b2, b3) = base_waves(i, j, world_time);
                let h1 = b2 as f32 + BASE_HEIGHT + b3 as f32 + BASE_HEIGHT;
                (h + h1 / 2.0) / 2.0
            };
            vertices[off_x + off_y * m] = Vec3::new(j as f32 * WAVE_SCALE, i as f32 * WAVE_SCALE, height);
        }
    }
    // Normal de cada vértice: la media de las de sus triángulos (`FaceNormalize`), normalizada.
    let mut sum = vec![Vec3::ZERO; m * m];
    for t in triangles.chunks_exact(3) {
        let (a, b, c) = (t[0] as usize, t[1] as usize, t[2] as usize);
        let n = (vertices[b] - vertices[a]).cross(vertices[c] - vertices[a]).normalize_or_zero();
        sum[a] += n;
        sum[b] += n;
        sum[c] += n;
    }
    // Dividir por el número de triángulos no cambia la dirección: basta con normalizar.
    let normals = sum.into_iter().map(|n| n.normalize_or_zero()).collect();
    Grid { vertices, normals }
}

/// `g_chrome` de `Render` (:62-67) en el orden de `glTexCoord2f(g_chrome[1], g_chrome[0])`:
/// (N.y · 0.5 + 0.5, N.z · 0.5 + 0.1).
pub fn water_uv(n: Vec3) -> [f32; 2] {
    [n.y * 0.5 + 0.5, n.z * 0.5 + 0.1]
}

/// Color de la segunda pasada (:86-87): `a = 1 − N·(1, −1, 1)`, `(a, 2.5a, 3a)` limitado a [0, 1]
/// como `glColor3f`.
pub fn second_pass_color(n: Vec3) -> [f32; 3] {
    let a = 1.0 - n.dot(LIGHT_VECTOR);
    [a, a * 2.5, a * 3.0].map(|v| v.clamp(0.0, 1.0))
}

/// `CheckGrass` (GMHellas.cpp:402-432): las hierbas que flotan (15, 29, 32) se apartan del personaje
/// mientras camina o corre (`walking`, ver [`check_grass_walking`]). `direction` es `o->Direction`
/// (x, y) y `position` la posición (x, y) del objeto. `HeadAngle` es siempre 0: el giro no cambia.
pub fn check_grass(direction: &mut Vec2, position: &mut Vec2, hero: Vec2, walking: bool) {
    if !walking {
        return;
    }
    // Solo mira la x (`o->Direction[0] < 0.1f`), como el original.
    if direction.x < 0.1 {
        let d = hero - *position;
        if d.length() < 50.0 {
            *direction = Vec2::new(-d.x * 0.6, -d.y * 0.6);
        }
    }
    *direction *= 0.6;
    *position += *direction;
}

/// La condición de `CheckGrass`: `PLAYER_WALK_MALE..=PLAYER_RUN_RIDE_WEAPON` o las carreras y pasos
/// en Fenrir (las del Rage Fighter van con `PBG_ADD_NEWCHAR_MONK_ANI`, que el Main 5.2 no define).
pub fn check_grass_walking(action: usize) -> bool {
    use crate::fx::tables::ids::*;
    let a = action as i32;
    (PLAYER_WALK_MALE..=PLAYER_RUN_RIDE_WEAPON).contains(&a)
        || (PLAYER_FENRIR_RUN..=PLAYER_FENRIR_RUN_ONE_LEFT_ELF).contains(&a)
        || (PLAYER_FENRIR_WALK..=PLAYER_FENRIR_WALK_ONE_LEFT).contains(&a)
}

/// El original multiplica en sRGB; en lineal equivale a elevar a 2.2 (como `terrain::vertex_color`).
fn linear(c: [f32; 3]) -> [f32; 4] {
    let l = |v: f32| v.clamp(0.0, 1.0).powf(2.2);
    [l(c[0]), l(c[1]), l(c[2]), 1.0]
}

/// Al cargar el terreno: el agua de Kalima (`CreateWaterTerrain`, GMHellas.cpp:42-58) o nada.
fn spawn_water(
    mut commands: Commands,
    args: Res<Args>,
    mut water: ResMut<WaterTerrain>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut layers: ResMut<Assets<LayerMaterial>>,
    mut fx_materials: ResMut<Assets<FxMaterial>>,
) {
    *water = WaterTerrain::default();
    *waves() = None;
    PENDING.lock().unwrap_or_else(|e| e.into_inner()).clear();
    if !is_water_world(args.world) {
        return;
    }
    // `MapManager.cpp:988-991`: siempre de `Object25`, sea cual sea la Kalima.
    let dir = mu_data::fs::find(&args.data, "Object25").unwrap_or_else(|| args.data.join("Object25"));
    let load = |name: &str| {
        mu_data::texture::load(&dir, name).map_err(|e| warn!("agua de Kalima: falta Object25/{name}: {e}")).ok()
    };
    // `water1.tga`: `GL_LINEAR`, `GL_REPEAT`.
    let tile0 = load("water1.OZT").map(|t| images.add(to_bevy_image(&t)));
    // `water2.jpg`: `GL_NEAREST`, `GL_CLAMP_TO_EDGE`.
    let tile1 = load("water2.OZJ").map(|t| {
        let mut image = to_bevy_image(&t);
        image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::ClampToEdge,
            address_mode_v: ImageAddressMode::ClampToEdge,
            mag_filter: ImageFilterMode::Nearest,
            min_filter: ImageFilterMode::Nearest,
            mipmap_filter: ImageFilterMode::Nearest,
            ..default()
        });
        images.add(image)
    });

    let triangles = grid_triangles();
    let n = MAX_WATER_GRID * MAX_WATER_GRID;
    let new_mesh = |colors: bool| {
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; n])
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; n])
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32; 2]; n])
            .with_inserted_indices(Indices::U32(triangles.clone()));
        if colors {
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0f32, 0.0, 0.0, 1.0]; n]);
        }
        mesh
    };
    let first = meshes.add(new_mesh(false));
    let second = meshes.add(new_mesh(true));

    // Primera pasada: `EnableAlphaTest` (mezcla con alfa, prueba > 0.25, escribe profundidad).
    if let Some(texture) = tile0 {
        let mut m = LayerMaterial::new(texture, false);
        m.params.tint = Vec4::from_array(linear(FIRST_PASS_COLOR));
        commands.spawn((
            Name::new("Agua de Kalima (water1)"),
            Mesh3d(first.clone()),
            MeshMaterial3d(layers.add(m)),
            Transform::default(),
            Visibility::default(),
            NoFrustumCulling,
            crate::scene::WorldEntity,
        ));
    }
    // Segunda pasada: `EnableAlphaBlend` (suma, sin escribir profundidad). En la pasada transparente,
    // que va después de la de prueba de alfa: queda encima de la primera, como en el original.
    if let Some(texture) = tile1 {
        commands.spawn((
            Name::new("Agua de Kalima (water2)"),
            Mesh3d(second.clone()),
            MeshMaterial3d(fx_materials.add(FxMaterial::new(Some(texture), Blend::Glow))),
            Transform::default(),
            Visibility::default(),
            NoFrustumCulling,
            crate::scene::WorldEntity,
        ));
    }
    *waves() = Some(Box::default());
    water.meshes = Some([first, second]);
    info!("World{}: mar de Kalima ({}×{} vértices)", args.world, MAX_WATER_GRID, MAX_WATER_GRID);
}

/// Casilla del personaje (`Hero->PositionX/Y`).
fn hero_tile(players: &Query<&crate::character::Player>) -> Option<IVec2> {
    players.single().ok().map(|p| IVec2::new(p.pos.x.floor() as i32, p.pos.y.floor() as i32))
}

/// Un tick del agua (`Update`): las olas pedidas, la del personaje cada 40 ticks y la propagación.
fn update_water(args: Res<Args>, players: Query<&crate::character::Player>) {
    let pending: Vec<_> = std::mem::take(&mut *PENDING.lock().unwrap_or_else(|e| e.into_inner()));
    if !is_water_world(args.world) {
        return;
    }
    let mut guard = waves();
    let Some(waves) = guard.as_mut() else { return };
    for (x, y, range, height) in pending {
        // `AddWaterWave`: `addSineWave(x*2, y*2, range, range, height)`.
        waves.add_sine_wave(x, y, range, range, height);
    }
    if let Some(hero) = hero_tile(&players) {
        waves.update(hero);
    }
    waves.frame = waves.frame.wrapping_add(1);
}

/// `Render` (:53-92): la rejilla alrededor del personaje en este fotograma.
fn render_water(
    time: Res<Time>,
    water: Res<WaterTerrain>,
    players: Query<&crate::character::Player>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut triangles: Local<Vec<u32>>,
) {
    let Some([first, second]) = water.meshes.as_ref() else { return };
    let Some(hero) = hero_tile(&players) else { return };
    if triangles.is_empty() {
        *triangles = grid_triangles();
    }
    let world_time = crate::terrain::world_ms(&time) as f32;
    let grid = {
        let guard = waves();
        let Some(waves) = guard.as_deref() else { return };
        create_terrain(waves, hero.x * 2, hero.y * 2, world_time, &triangles)
    };
    // MU (x, y, z) → Bevy (x, z, −y), una casilla = 1.
    let positions: Vec<[f32; 3]> = grid.vertices.iter().map(|v| [v.x / MU_SCALE, v.z / MU_SCALE, -v.y / MU_SCALE]).collect();
    let uvs: Vec<[f32; 2]> = grid.normals.iter().map(|&n| water_uv(n)).collect();
    if let Some(mut m) = meshes.get_mut(first) {
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions.clone());
        m.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs.clone());
    }
    if let Some(mut m) = meshes.get_mut(second) {
        let colors: Vec<[f32; 4]> = grid.normals.iter().map(|&n| linear(second_pass_color(n))).collect();
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        m.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        m.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ola_de_seno_como_el_original() {
        // (cos + 65535) · 2000 ≈ 131 070 000 → >> 19 = 249 o 250 (el coseno casi no cuenta).
        assert_eq!(sine_wave(0.0, 2000), 250);
        assert_eq!(sine_wave(std::f32::consts::PI, 2000), 249);
        // Negativa: el desplazamiento aritmético redondea hacia abajo (−65 536 000 / 2¹⁹ = −125 justo;
        // −150 · 65 536 / 2¹⁹ = −18.75 → −19).
        assert_eq!(sine_wave(0.0, -1000), -125);
        assert_eq!(sine_wave(1.0, -1000), -125);
        assert_eq!(sine_wave(0.0, -150), -19);
    }

    #[test]
    fn ola_del_personaje_y_propagacion() {
        let mut w = Waves::default();
        // `addSineWave(x, y, 20, 2, 2000)`: |cx| < √(40 − cy²) en las filas cy = −2..1.
        w.add_sine_wave(100, 100, 20, 2, 2000);
        let s = WATER_TERRAIN_SIZE;
        let at = |w: &Waves, page: usize, x: usize, y: usize| w.pages[page][y * s + x];
        assert!(at(&w, 0, 100, 100) >= 249);
        assert!(at(&w, 0, 106, 100) > 0 && at(&w, 0, 107, 100) == 0);
        // `cy < bottom`: la fila y + 2 no se toca.
        assert_eq!(at(&w, 0, 100, 102), 0);
        assert!(at(&w, 0, 100, 98) > 0);
        // Un tick: la página 1 = vecinos de la 0 / 2 − ella misma (0), amortiguado 1/16.
        let v = at(&w, 0, 100, 99) + at(&w, 0, 100, 101) + at(&w, 0, 101, 100) + at(&w, 0, 99, 100);
        let newh = v >> 1;
        w.page ^= 1;
        w.calc_wave();
        assert_eq!(at(&w, 1, 100, 100), newh - (newh >> 4));
        // El borde no se calcula.
        assert_eq!(at(&w, 1, 0, 100), 0);
    }

    #[test]
    fn oleaje_trunca_como_int() {
        // WorldTime = 0, celda (0, 0): (int)(0 - 0) = 0.
        assert_eq!(base_waves(0, 0, 0.0), (0, 0));
        // (int)sin(0.5)·50 = 23; 23 − sin(0.5)·50 = −0.97 → 0 (trunca hacia 0).
        let t = 100.0; // 0.005·100 = 0.5, 0.003·100 = 0.3
        let mh = ((0.5f32).sin() * 50.0) as i32;
        let h2 = (mh as f32 - (0.3f32).sin() * 50.0) as i32;
        assert_eq!(base_waves(0, 0, t).0, h2);
    }

    #[test]
    fn agua_quieta() {
        let w = Waves::default();
        let tri = grid_triangles();
        assert_eq!(tri.len(), (MAX_WATER_GRID - 1) * (MAX_WATER_GRID - 1) * 6);
        // Con oleaje 0 (WorldTime = 0 en la celda 0, 0 no vale para todas): en la esquina negativa
        // todo está fuera de la tabla → 350 plano.
        let g = create_terrain(&w, -100, -100, 0.0, &tri);
        assert!(g.vertices.iter().all(|v| v.z == BASE_HEIGHT));
        for &n in &g.normals {
            assert!((n - Vec3::Z).length() < 1e-5, "{n:?}");
        }
        // Normal hacia arriba: textura en (0.5, 0.6) y la segunda pasada no suma nada.
        assert_eq!(water_uv(Vec3::Z), [0.5, 0.6]);
        assert_eq!(second_pass_color(Vec3::Z), [0.0, 0.0, 0.0]);
        // La rejilla empieza en (x − 25, y − 15) celdas.
        assert_eq!(g.vertices[0].x, -125.0 * WAVE_SCALE);
        assert_eq!(g.vertices[0].y, -115.0 * WAVE_SCALE);
    }

    #[test]
    fn altura_para_los_objetos_que_flotan() {
        let w = Waves::default();
        // Sin ondas: (350 + (h2 + 350 + h3 + 350) / 2) / 4.
        let (b2, b3) = base_waves(20, 20, 1234.0);
        let expected = (350.0 + (b2 as f32 + 350.0 + b3 as f32 + 350.0) / 2.0) / 4.0;
        assert_eq!(w.water_height(1000.0, 1000.0, 1234.0), expected);
        // Inclinada hacia −x: la segunda pasada brilla.
        let n = Vec3::new(-0.3, 0.0, 0.95).normalize();
        assert!(second_pass_color(n)[0] > 0.0);
    }
}
