//! Capas que corren por toda la pantalla: la arena de Tarkan y Karutan y el humo de Crywolf, el
//! Pantano de la Calma, Raklion, Doppelganger 1 y la niebla/tormenta de Empire Guardian
//! (`RenderOutSides`, ZzzInterface.cpp:8302-8348, y las funciones de cada mapa).
//!
//! El original las pinta con `RenderBitmapUV` (ZzzOpenglUtil.cpp:1513-1541) en la pantalla virtual
//! de 640×480, en (0, 0, 640, 480 − 45), después de toda la escena 3D y antes de la interfaz
//! (`RenderInterface` va justo antes de `g_pNewUISystem->Render`, ZzzScene.cpp:2219-2230). Aquí cada
//! capa es un rectángulo pegado a la cámara, delante de todo (lo último de la pasada transparente) y
//! debajo de la interfaz de Bevy, que se dibuja después.
//!
//! `RenderBitmapUV` deforma las UV en trapecio: a la izquierda solo va de `v + 0.25·alto` a
//! `v + 0.75·alto` y a la derecha de `v` a `v + alto`, con los dos triángulos de `GL_TRIANGLE_FAN`.
//! El desplazamiento es `(WorldTime % 100000) · velocidad`, con `WorldTime` en milisegundos.
//!
//! El Valle de Loren solo tiene humo durante el asedio (`battleCastle::RenderBaseSmoke` mira
//! `IsBattleCastleStart`), que no tenemos; el Loren Market no pinta nada (`RenderFrontSideVisual` vacío).

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use mu_data::texture;

use crate::Args;
use crate::fx::material::{Blend, FxMaterial};
use crate::fx::textures::fx_image;

pub struct OverlayPlugin;

impl Plugin for OverlayPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EmpireGuardianWeather>()
            .init_resource::<CrywolfAshes>()
            .add_systems(Update, spawn_overlays.run_if(crate::scene::load_step(crate::scene::Step::Objects)))
            .add_systems(
                PostUpdate,
                (crywolf_ashes, update_overlays).chain().after(crate::terrain::TerrainCulled),
            );
    }
}

/// Las texturas `BITMAP_CHROME+2` / `+3` que carga cada mapa (MapManager.cpp).
#[derive(Clone, Copy, PartialEq, Debug)]
enum Tex {
    /// `Object9\sand01.jpg`.
    Sand01,
    /// `Object9\sand02.jpg`.
    Sand02,
    /// `Effect\Map_Smoke1.jpg`.
    Smoke1,
    /// `Effect\Map_Smoke2.tga`.
    Smoke2,
}

impl Tex {
    fn file(self) -> (&'static str, &'static str) {
        match self {
            Tex::Sand01 => ("Object9", "sand01.OZJ"),
            Tex::Sand02 => ("Object9", "sand02.OZJ"),
            Tex::Smoke1 => ("Effect", "Map_Smoke1.OZJ"),
            Tex::Smoke2 => ("Effect", "Map_Smoke2.OZT"),
        }
    }
}

/// Una llamada a `RenderBitmapUV`.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Layer {
    tex: Tex,
    /// `EnableAlphaBlend` (suma) o `EnableAlphaTest` (mezcla con alfa y prueba de alfa).
    blend: Blend,
    /// `glColor3f`.
    color: [f32; 3],
    /// UV que avanza por milisegundo (`WindX`, `WindY`).
    speed: [f32; 2],
    /// `uWidth`, `vHeight`.
    size: [f32; 2],
}

/// Cuándo se ven las capas de un mapa.
#[derive(Clone, Copy, PartialEq, Debug)]
enum When {
    Always,
    /// Crywolf: solo cuando se ha dibujado el objeto 73 con `weather == 0` (`ashies`).
    Ashes,
    /// Empire Guardian con niebla.
    Fog,
    /// Empire Guardian con tormenta: un fotograma de cada 20 al azar (`rand() % 20 == 0`).
    Storm,
}

const fn layer(tex: Tex, blend: Blend, color: [f32; 3], speed: [f32; 2], size: [f32; 2]) -> Layer {
    Layer { tex, blend, color, speed, size }
}

/// Capas de `world` (nuestro número de mapa), en el orden en que se pintan.
fn world_layers(world: u16) -> Vec<(When, Layer)> {
    use Blend::{AlphaTest, Glow};
    const SAND: [f32; 3] = [0.3, 0.3, 0.25];
    const SMOKE: [f32; 3] = [0.4, 0.4, 0.45];
    let smoke_base = |when| {
        // `M34CryWolf1st::RenderBaseSmoke` (GMCrywolf1st.cpp:2108-2118) = `GMSwampOfQuiet::RenderBaseSmoke`
        // (GMSwampOfQuiet.cpp:40-52).
        vec![
            (when, layer(Tex::Smoke2, AlphaTest, SMOKE, [0.0005, 0.0], [3.0, 2.0])),
            (when, layer(Tex::Smoke1, Glow, SMOKE, [0.0002, 0.0], [0.3, 0.3])),
        ]
    };
    match world as i32 - 1 {
        // Tarkan (ZzzInterface.cpp:8304-8314): las dos sumadas con el mismo color.
        8 => vec![
            (When::Always, layer(Tex::Sand01, Glow, SAND, [0.0002, 0.0], [0.3, 0.3])),
            (When::Always, layer(Tex::Sand02, Glow, SAND, [0.001, 0.0], [3.0, 2.0])),
        ],
        // Karutan (8316-8323, `ASG_ADD_MAP_KARUTAN`).
        80 | 81 => vec![(When::Always, layer(Tex::Sand02, Glow, SAND, [0.004, 0.0], [3.0, 2.0]))],
        34 => smoke_base(When::Ashes),
        56 => smoke_base(When::Always),
        // Raklion y Doppelganger 1 (`IsIceCity`, `CGM_Raklion::RenderBaseSmoke`, GM_Raklion.cpp:2266-2275):
        // su `BITMAP_CHROME+3` es `sand02.jpg` (MapManager.cpp:749-758).
        57 | 58 | 65 => vec![
            (When::Always, layer(Tex::Sand02, Glow, SMOKE, [0.0006, -0.0006], [3.0, 2.0])),
            (When::Always, layer(Tex::Smoke1, Glow, SMOKE, [0.0001, 0.0], [0.3, 0.3])),
        ],
        // Empire Guardian 1-4 (`GMEmpireGuardian1::RenderFrontSideVisual`, GMEmpireGuardian1.cpp:2250-2281).
        69..=72 => {
            const FOG: [f32; 3] = [0.6, 0.6, 0.9];
            const STORM: [f32; 3] = [0.7, 0.7, 0.9];
            vec![
                (When::Fog, layer(Tex::Smoke1, Glow, FOG, [0.00005, 0.00008], [2.0, 2.0])),
                (When::Fog, layer(Tex::Smoke1, Glow, FOG, [-0.00005, 0.0], [0.3, 0.3])),
                (When::Storm, layer(Tex::Smoke1, Glow, STORM, [0.0006, -0.0006], [3.0, 2.0])),
                (When::Storm, layer(Tex::Smoke1, Glow, STORM, [-0.0006, 0.0006], [3.0, 2.0])),
            ]
        }
        _ => Vec::new(),
    }
}

/// Tiempo de Empire Guardian (`m_iWeather`), que manda el servidor al entrar
/// (`ReceiveEnterEmpireGuardianEvent`). Empieza en sol (`WEATHER_SUN`), sin nada en pantalla.
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq, Debug)]
#[allow(dead_code)]
pub enum EmpireGuardianWeather {
    #[default]
    Sun,
    Rain,
    Fog,
    Storm,
}

/// Lo que decide si se ve el humo de Crywolf.
#[derive(Resource)]
pub struct CrywolfAshes {
    /// `weather` (Winmain.cpp:84): `rand() % 3` al arrancar. Con la semilla por defecto del CRT el
    /// primer `rand()` es 41, así que empieza en 2.
    weather: u32,
    /// `Change_weather`: llamadas a `RenderCryWolf1stObjectVisual`; a las 70000, `weather` cambia.
    calls: u32,
    /// `ashies`: se ha dibujado el objeto 73 con `weather == 0` en este fotograma.
    ashes: bool,
    /// Objetos del mapa (tipo, posición en casillas), solo en Crywolf.
    objects: Vec<(i16, Vec2)>,
    rng: u32,
}

impl Default for CrywolfAshes {
    fn default() -> Self {
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.subsec_nanos());
        Self { weather: 2, calls: 0, ashes: false, objects: Vec::new(), rng: seed }
    }
}

impl CrywolfAshes {
    /// `rand()` del CRT de Microsoft.
    fn rand(&mut self) -> u32 {
        self.rng = self.rng.wrapping_mul(214013).wrapping_add(2531011);
        (self.rng >> 16) & 0x7fff
    }
}

/// Una capa en pantalla.
#[derive(Component)]
struct Overlay {
    when: When,
    layer: Layer,
    mesh: Handle<Mesh>,
    /// Distancia a la cámara (en Bevy): las primeras capas, un poco más lejos, para que se pinten antes.
    depth: f32,
}

fn spawn_overlays(
    mut commands: Commands,
    args: Res<Args>,
    cameras: Query<Entity, (With<Camera3d>, With<IsDefaultUiCamera>)>,
    mut ashes: ResMut<CrywolfAshes>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<FxMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    ashes.objects.clear();
    ashes.ashes = false;
    if args.world as i32 - 1 == 34 {
        ashes.objects = mu_data::objects::load(&args.data, args.world)
            .map(|v| v.iter().map(|o| (o.kind, Vec2::new(o.position[0], o.position[1]) / 100.0)).collect())
            .unwrap_or_default();
    }
    let layers = world_layers(args.world);
    let Ok(camera) = cameras.single() else { return };
    for (i, (when, l)) in layers.into_iter().enumerate() {
        let (dir, file) = l.tex.file();
        let folder = mu_data::fs::find(&args.data, dir).unwrap_or_else(|| args.data.join(dir));
        let tex = match texture::load(&folder, file) {
            Ok(t) => t,
            Err(e) => {
                warn!("capa de pantalla: falta {dir}/{file}: {e}");
                continue;
            }
        };
        let mut m = FxMaterial::new(Some(images.add(fx_image(&tex, true))), l.blend);
        // El color del original se multiplica en sRGB: en lineal, elevado a 2.2.
        m.params.tint = Vec4::new(l.color[0].powf(2.2), l.color[1].powf(2.2), l.color[2].powf(2.2), 1.0);
        let mesh = meshes.add(
            Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; 4])
                .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 0.0, 1.0]; 4])
                .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32; 2]; 4])
                // `GL_TRIANGLE_FAN` de 0 (arriba izq.), 1 (abajo izq.), 2 (abajo der.), 3 (arriba der.).
                .with_inserted_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3])),
        );
        let depth = 0.4 - i as f32 * 0.05;
        let id = commands
            .spawn((
                Name::new(format!("Capa de pantalla {i}")),
                Overlay { when, layer: l, mesh: mesh.clone(), depth },
                Mesh3d(mesh),
                MeshMaterial3d(materials.add(m)),
                Transform::from_xyz(0.0, 0.0, -depth),
                Visibility::Hidden,
                NoFrustumCulling,
                crate::scene::WorldEntity,
            ))
            .id();
        commands.entity(camera).add_child(id);
    }
}

/// `ashies` de Crywolf: el objeto 73 se dibuja (`RenderCryWolf1stObjectVisual`, GMCrywolf1st.cpp:373-389
/// y 506-511). Se cuenta como dibujado lo que cae dentro del trapecio del suelo.
fn crywolf_ashes(mut ashes: ResMut<CrywolfAshes>, cull: Res<crate::terrain::TerrainCull>) {
    ashes.ashes = false;
    if ashes.objects.is_empty() {
        return;
    }
    let Some(shape) = cull.shape else { return };
    let mut drawn = 0u32;
    let mut seen_73 = false;
    for &(kind, p) in &ashes.objects {
        if shape.test(p.x, p.y, 0.0) {
            drawn += 1;
            seen_73 |= kind == 73;
        }
    }
    // `Change_weather++` en cada llamada; pasado de 70000, `weather = rand() % 3`.
    ashes.calls += drawn;
    if ashes.calls > 70000 {
        ashes.calls = 0;
        ashes.weather = ashes.rand() % 3;
    }
    ashes.ashes = seen_73 && ashes.weather == 0;
}

/// UV de `RenderBitmapUV` (arriba izq., abajo izq., abajo der., arriba der.).
fn bitmap_uv(u: f32, v: f32, uw: f32, vh: f32) -> [[f32; 2]; 4] {
    [[u, v + vh * 0.25], [u, v + vh - vh * 0.25], [u + uw, v + vh], [u + uw, v]]
}

/// Coloca cada capa delante de la cámara y mueve sus UV.
#[allow(clippy::too_many_arguments)]
fn update_overlays(
    time: Res<Time>,
    state: Res<State<crate::scene::GameScene>>,
    weather: Res<EmpireGuardianWeather>,
    mut ashes: ResMut<CrywolfAshes>,
    cameras: Query<&Projection, (With<Camera3d>, With<IsDefaultUiCamera>)>,
    mut overlays: Query<(&Overlay, &mut Visibility)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Ok(Projection::Perspective(p)) = cameras.single() else { return };
    let in_game = *state.get() == crate::scene::GameScene::Game;
    // Tormenta: `rand() % 20 == 0` una vez por fotograma para las dos capas.
    let storm_flash = *weather == EmpireGuardianWeather::Storm && ashes.rand() % 20 == 0;
    let t = (crate::terrain::world_ms(&time) % 100000) as f32;
    for (o, mut vis) in &mut overlays {
        let show = in_game
            && match o.when {
                When::Always => true,
                When::Ashes => ashes.ashes,
                When::Fog => *weather == EmpireGuardianWeather::Fog,
                When::Storm => storm_flash,
            };
        vis.set_if_neq(if show { Visibility::Inherited } else { Visibility::Hidden });
        if !show {
            continue;
        }
        // Pantalla virtual (0, 0)-(640, 435) → coordenadas de la vista a `depth` de la cámara.
        let half_h = o.depth * (p.fov * 0.5).tan();
        let half_w = half_h * p.aspect_ratio;
        let at = |x: f32, y: f32| [(x / 320.0 - 1.0) * half_w, (1.0 - y / 240.0) * half_h, 0.0];
        let bottom = 480.0 - 45.0;
        let pos = vec![at(0.0, 0.0), at(0.0, bottom), at(640.0, bottom), at(640.0, 0.0)];
        let l = &o.layer;
        let uv = bitmap_uv(t * l.speed[0], t * l.speed[1], l.size[0], l.size[1]).to_vec();
        if let Some(mut m) = meshes.get_mut(&o.mesh) {
            m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
            m.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uv_en_trapecio() {
        let uv = bitmap_uv(1.0, 0.0, 3.0, 2.0);
        assert_eq!(uv, [[1.0, 0.5], [1.0, 1.5], [4.0, 2.0], [4.0, 0.0]]);
    }

    #[test]
    fn capas_por_mapa() {
        assert_eq!(world_layers(9).len(), 2);
        assert_eq!(world_layers(82).len(), 1);
        assert_eq!(world_layers(57)[0].1.blend, Blend::AlphaTest);
        assert_eq!(world_layers(35)[0].0, When::Ashes);
        assert_eq!(world_layers(59)[0].1.tex, Tex::Sand02);
        assert!(world_layers(1).is_empty() && world_layers(31).is_empty());
    }
}
