//! Objects placed on a map (`EncTerrainN.obj`) and which model each type uses.

use std::path::{Path, PathBuf};

use crate::reader::Reader;
use crate::{Error, Result, crypt, fs};

pub mod render;
mod rules_elbeland;
mod rules_raklion;
mod rules_kalima;
mod rules_kanturu;
pub use rules_kalima::create_timer;

use render::{Bitmap, Pass, Saw, flags};

#[derive(Debug, Clone)]
pub struct MapObject {
    /// Model type (index into the map's model table).
    pub kind: i16,
    /// Position in MU units (100 = one tile), Z up.
    pub position: [f32; 3],
    /// Rotation in degrees (x, y, z).
    pub angle: [f32; 3],
    pub scale: f32,
}

pub fn read(buf: &[u8]) -> Result<Vec<MapObject>> {
    let dec = crypt::map_file_decrypt(buf);
    let mut r = Reader::new(&dec);
    let version = r.u8()?;
    let _map_number = r.u8()?;
    let count = r.i16()?.max(0) as usize;

    // Each version appends bytes to the end of each object (data we do not use yet).
    let extra = match version {
        0 => 0,
        1 => 2,
        2 => 3,
        3 => 3 + 12,
        4 => 3 + 12 + 1,
        5 => 3 + 12 + 1 + 8,
        v => return Err(Error::Format(format!(".obj versión {v} no soportada"))),
    };

    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let kind = r.i16()?;
        let position = r.vec3()?;
        let angle = r.vec3()?;
        let scale = r.f32()?;
        r.skip(extra)?;
        out.push(MapObject { kind, position, angle, scale });
    }
    Ok(out)
}

/// Loads the objects from `data_dir/World{N}/EncTerrain{N}.obj`, with `N` the map's data folder
/// (`terrain::data_world`).
pub fn load(data_dir: &Path, world: u16) -> Result<Vec<MapObject>> {
    let folder = crate::terrain::data_world(world);
    let dir = fs::find(data_dir, &format!("World{folder}"))
        .ok_or_else(|| Error::NotFound(data_dir.join(format!("World{folder}"))))?;
    read(&fs::read(&dir, &format!("EncTerrain{folder}.obj"))?)
}

/// Lorencia (World1) models: (first type, name, count).
/// The file is `Data/Object1/{name}{n:02}.bmd`. Taken from the original `MapManager.cpp`.
const LORENCIA_MODELS: &[(i16, &str, i16)] = &[
    (0, "Tree", 13),
    (20, "Grass", 8),
    (30, "Stone", 5),
    (40, "StoneStatue", 3),
    (43, "SteelStatue", 1),
    (44, "Tomb", 3),
    (50, "FireLight", 2),
    (52, "Bonfire", 1),
    (55, "DoungeonGate", 1),
    (56, "MerchantAnimal", 2),
    (58, "TreasureDrum", 1),
    (59, "TreasureChest", 1),
    (60, "Ship", 1),
    (65, "SteelWall", 3),
    (68, "SteelDoor", 1),
    (69, "StoneWall", 6),
    (75, "StoneMuWall", 4),
    (80, "Bridge", 1),
    (81, "Fence", 4),
    (85, "BridgeStone", 1),
    (90, "StreetLight", 1),
    (91, "Cannon", 3),
    (95, "Curtain", 1),
    (96, "Sign", 2),
    (98, "Carriage", 4),
    (102, "Straw", 2),
    (105, "Waterspout", 1),
    (106, "Well", 4),
    (110, "Hanging", 1),
    (111, "Stair", 1),
    (115, "House", 5),
    (120, "Tent", 1),
    (121, "HouseWall", 6),
    (127, "HouseEtc", 3),
    (130, "Light", 3),
    (133, "PoseBox", 1),
    (140, "Furniture", 7),
    (150, "Candle", 1),
    (151, "Beer", 3),
];

/// Default object animation speed (`o->Velocity` in `CreateObject`).
pub const DEFAULT_VELOCITY: f32 = 0.16;
/// Logic updates per second in the original: `Velocity` is frames per update.
pub const TICKS_PER_SECOND: f32 = 25.0;

/// Animation speed of an object.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Velocity {
    /// Animation frames per update (25 per second).
    Fixed(f32),
    /// `k / scale`: big trees sway more slowly.
    PerScale(f32),
}

impl Velocity {
    /// Animation frames per second for an object of scale `scale`.
    pub fn fps(self, scale: f32) -> f32 {
        let v = match self {
            Velocity::Fixed(v) => v,
            Velocity::PerScale(k) => k / scale.max(0.01),
        };
        v * TICKS_PER_SECOND
    }
}

/// Brightness of the additive mesh (`BlendMeshLight`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Glow {
    Steady(f32),
    /// Fire flicker: a random value between `min` and `max` on each update.
    Flicker { min: f32, max: f32 },
    /// Smooth pulse: `sin(t · freq) · amp + base`, with `t` in seconds.
    Pulse { freq: f32, amp: f32, base: f32 },
}

/// Special drawing rules for an object type.
///
/// Taken from `CreateObject`, `MoveObject` and `UpdateObjectVisual` (the original `ZzzObject.cpp`).
/// Whatever creates particles or lights (fire, smoke, sparks) is not here.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelRules {
    /// Not drawn: only marks a point (fire, smoke, sitting spot...).
    pub hidden: bool,
    /// `o->BlendMesh`: meshes whose texture index is this one are drawn additively (lights, water,
    /// fire); and the mesh with this index scrolls with [`ModelRules::tex_coord`].
    pub additive_mesh: Option<usize>,
    /// `o->BlendMeshLight`.
    pub glow: Glow,
    /// `o->BlendMeshTexCoordU/V` (set by `MoveObject` every tick).
    pub tex_coord: [Saw; 2],
    /// `Models[type].StreamMesh`: mesh without normal lighting (color `BodyLight`) that scrolls.
    pub stream_mesh: Option<usize>,
    pub velocity: Velocity,
    /// Roof: fades out when the character enters the house (see [`indoor_tile`]).
    pub fade_indoors: bool,
    /// Fixed (gray) `b->BodyLight` instead of the terrain light under the object.
    pub body_light: Option<f32>,
    /// The `RenderBody`/`RenderMesh` calls of its drawing code, in order
    /// ([`render::DEFAULT_PASSES`] = one `RenderBody(RENDER_TEXTURE)`).
    pub passes: &'static [Pass],
    /// `RenderBodyShadow` after drawing it: black shadow on the ground, without this mesh (`o->HiddenMesh`).
    pub body_shadow: Option<usize>,
    /// Moves on its own (doors, skulls, the sled dog).
    pub motion: Option<Motion>,
    /// `o->Light` set by the map code: `BodyLight(o, b)` (`ZzzObject.cpp:206-234`) is the
    /// ground light **plus** `o->Light` (which starts at 0 for map objects).
    pub light_add: Option<LightAdd>,
    /// Fixed `IntensityTransform` on all vertices (instead of `normal·L · 0.8 + 0.4`): the color
    /// of lit meshes becomes `BodyLight · k`, flat.
    pub intensity: Option<f32>,
    /// Action, frame and height are driven by its effects code (`fx::core::MapObject`,
    /// every tick): `SetAction` / `CurrentAction` changing at random, animations that stop...
    /// The object has its own animation group.
    pub fx_animation: bool,
}

/// `o->Light` that a map's code sets on an object (added to the ground light).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LightAdd {
    Fixed([f32; 3]),
    /// Per channel: `sin(WorldTime · freq_ms[c]) · amp + base`.
    Wave { freq_ms: [f32; 3], amp: f32, base: f32 },
}

impl LightAdd {
    /// `o->Light` with `WorldTime` in ms.
    pub fn at(self, world_time: f32) -> [f32; 3] {
        match self {
            LightAdd::Fixed(l) => l,
            LightAdd::Wave { freq_ms, amp, base } => freq_ms.map(|f| (world_time * f).sin() * amp + base),
        }
    }
}

/// Objects that `MoveObject` / `RenderObjectVisual` move on their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    /// Devias door that swings open when the character approaches (`ZzzObject.cpp:4004-4047`).
    SwingDoor,
    /// Devias door that slides open (type 86).
    SlidingDoor,
    /// Lost Tower skull that gets kicked away when stepped on (`CheckSkull`, `ZzzEffectFireLeave.cpp:88`).
    Skull,
    /// Devias sled dog: when the animation ends, 1 in 32 times it does action 1
    /// (`RenderObjectVisual`, `ZzzObject.cpp:2824-2831`).
    SledDog,
    /// Grass floating on the Kalima sea (15, 29, 32): moves away from the character (`CheckGrass`) and
    /// bobs with the waves (`GetWaterTerrain + 180`, `GMHellas.cpp:452-466`).
    KalimaFloat,
    /// Land of Trials 27: `Position[2] += sin(Timer + WorldTime·0.0024)·0.3` every tick
    /// (`MoveHuntingGroundObject`, `GMHuntingGround.cpp:91-96`).
    HuntingGroundVibrate,
}

/// Whether ground texture `tile` (layer 1) under the character is "inside a house"
/// (`HeroTile` in `MoveObject`): if so, roofs fade out.
pub fn indoor_tile(world: u16, tile: u8) -> bool {
    match world {
        1 => tile == 4,                // Lorencia
        3 => tile == 3 || tile >= 10,  // Devias
        _ => false,
    }
}

impl Default for ModelRules {
    fn default() -> Self {
        Self {
            hidden: false,
            additive_mesh: None,
            glow: Glow::Steady(1.0),
            tex_coord: [Saw::ZERO; 2],
            stream_mesh: None,
            velocity: Velocity::Fixed(DEFAULT_VELOCITY),
            fade_indoors: false,
            body_light: None,
            passes: render::DEFAULT_PASSES,
            body_shadow: None,
            motion: None,
            light_add: None,
            intensity: None,
            fx_animation: false,
        }
    }
}

impl ModelRules {
    fn hidden() -> Self {
        Self { hidden: true, ..Self::default() }
    }
    fn add(mesh: usize) -> Self {
        Self { additive_mesh: Some(mesh), ..Self::default() }
    }
    /// `o->BlendMeshTexCoordU = k · ((int)WorldTime % period)`.
    fn tex_u(mut self, period: f32, k: f32) -> Self {
        self.tex_coord[0] = Saw { period, k };
        self
    }
    /// `o->BlendMeshTexCoordV = k · ((int)WorldTime % period)`.
    fn tex_v(mut self, period: f32, k: f32) -> Self {
        self.tex_coord[1] = Saw { period, k };
        self
    }
    /// `Models[type].StreamMesh = mesh`.
    fn stream(mut self, mesh: usize) -> Self {
        self.stream_mesh = Some(mesh);
        self
    }
    fn passes(mut self, passes: &'static [Pass]) -> Self {
        self.passes = passes;
        self
    }
    fn motion(mut self, m: Motion) -> Self {
        self.motion = Some(m);
        self
    }
    fn flicker(mut self, min: f32, max: f32) -> Self {
        self.glow = Glow::Flicker { min, max };
        self
    }
    /// `sin(WorldTime · freq_ms) · amp + base`, with the original's frequency (per millisecond).
    fn pulse(mut self, freq_ms: f32, amp: f32, base: f32) -> Self {
        self.glow = Glow::Pulse { freq: freq_ms * 1000.0, amp, base };
        self
    }
    fn speed(mut self, v: f32) -> Self {
        self.velocity = Velocity::Fixed(v);
        self
    }
    fn body_light(mut self, l: f32) -> Self {
        self.body_light = Some(l);
        self
    }
}

/// Rules for an object type. `world` is the folder number (`World1` = Lorencia).
///
/// Texture scrolling: `tex_u(period, k)` / `tex_v(period, k)` are the original's
/// `k · ((int)WorldTime % period)` as is (sawtooth, with its jump).
pub fn model_rules(world: u16, kind: i16) -> ModelRules {
    let r = ModelRules::default;
    let add = ModelRules::add;
    let hidden = ModelRules::hidden;
    let roof = || ModelRules { fade_indoors: true, ..r() };
    // Elbeland, Swamp of Calmness, Santa Village, Duel Arena and Loren Market.
    if let Some(rules) = rules_elbeland::rules(world, kind) {
        return rules;
    }
    // Raklion, Vulcanus, Balgass Barracks/Refuge, Karutan and Chaos Castle.
    if let Some(rules) = rules_raklion::rules(world, kind) {
        return rules;
    }
    // Kalima 1-7 and Land of Trials.
    if let Some(rules) = rules_kalima::rules(world, kind) {
        return rules;
    }
    // Aida, Crywolf, Kanturu 1-3 and the GM map.
    if let Some(rules) = rules_kanturu::rules(world, kind) {
        return rules;
    }
    match (world, kind) {
        // Roofs that disappear when entering the house.
        (1, 125 | 126) => roof(),                // Lorencia: HouseWall05-06
        (3, 81 | 82 | 96 | 98 | 99) => roof(),   // Devias

        // Lorencia (WD_0LORENCIA)
        (1, 0 | 1) => ModelRules { velocity: Velocity::PerScale(0.4), ..r() }, // Tree01-02 (sway)
        (1, 52) => add(1).flicker(0.4, 0.9),                                   // Bonfire
        (1, 59) => r().speed(0.0),                                             // TreasureChest
        (1, 90) => add(1).speed(0.3),                                          // StreetLight
        (1, 96 | 97) => r().speed(0.3),                                        // Sign01-02
        (1, 98) => add(2),                                                     // Carriage01
        (1, 105) => add(3).tex_v(1000.0, -0.001),                                 // Waterspout (falling water)
        (1, 117) => add(4).flicker(0.4, 0.7),                                  // House03 (forge)
        (1, 118) => add(8).tex_v(1000.0, -0.001),                                 // House04
        (1, 119) => add(2).tex_v(1000.0, -0.001),                                 // House05 (mill)
        (1, 122) => add(4).flicker(0.4, 0.7),                                  // HouseWall02
        (1, 130..=133) => hidden(),                                            // Light01-03 (fire), PoseBox
        (1, 150) => add(1).speed(0.3),                                         // Candle

        // Dungeon (WD_1DUNGEON)
        (2, 22..=24) => r().stream(1).tex_v(1000.0, -0.001),
        (2, 39 | 40 | 51 | 52 | 60) => hidden(),

        // Devias (WD_2DEVIAS)
        (3, 19 | 92 | 93) => add(0),
        (3, 54 | 56) => add(1),
        (3, 78) => add(3).flicker(0.4, 0.7),
        (3, 91 | 100) => hidden(),
        // Doors (`CreateObject` 4793-4800 and `MoveObject` 4004-4047) and sled dog (2824-2831).
        (3, 20 | 65 | 88) => r().motion(Motion::SwingDoor),
        (3, 86) => r().motion(Motion::SlidingDoor),
        (3, 103) => r().motion(Motion::SledDog),

        // Noria (WD_3NORIA)
        (4, 1) => add(1),
        (4, 9) => add(3),
        (4, 17 | 19 | 37) => add(0),
        (4, 18) => add(2).tex_v(1000.0, 0.001),
        (4, 38) => hidden(), // sitting spot
        (4, 39) => add(1),
        (4, 41) => add(0).tex_v(2000.0, 0.0005),
        (4, 42) => r().stream(0).tex_u(500.0, -0.002),
        (4, 43) => r().stream(0).tex_u(500.0, 0.002),

        // Lost Tower (WD_4LOSTTOWER)
        (5, 18 | 23) => add(1),
        // `RenderObject` (`ZzzObject.cpp:1031-1050`): first everything with `BITMAP_CHROME` and `BodyLight`
        // (1, 0.2, 0.1), with `StreamMesh` 2 (19/20) or 1 (3/4); then normal. The red shows through the gaps
        // of alpha textures and in the `BlendMesh`. `MoveObject` (4086-4094): U = -(WT%1000)·0.001.
        (5, 3 | 4) => r().tex_u(1000.0, -0.001).passes(LOST_TOWER_3_4),
        (5, 19 | 20) => add(4).tex_u(1000.0, -0.001).passes(LOST_TOWER_19_20),
        (5, 24 | 25) => hidden(),
        // `CheckSkull` (`MoveObject` 4082-4085).
        (5, 38 | 39) => r().motion(Motion::Skull),

        // Exile (WD_5UNKNOWN)
        (6, 2) => add(0),
        (6, 3) => add(0).flicker(0.6, 0.9),

        // Arena (WD_6STADIUM)
        (7, 21) => add(3).tex_v(1000.0, -0.001),
        (7, 38) => hidden(),

        // Atlans (WD_7ATLANSE)
        (8, 22 | 39) => hidden(),
        (8, 23) => add(0).pulse(0.002, 0.3, 0.5),
        (8, 32 | 34) => add(1).pulse(0.004, 0.5, 0.5),
        (8, 38) => add(0),
        (8, 40) => add(0).pulse(0.004, 0.3, 0.5).speed(0.05),

        // Tarkan (WD_8TARKAN)
        (9, 2) => add(0).tex_u(1000.0, -0.001),
        (9, 4) => add(0).pulse(0.002, 0.35, 0.65).tex_v(10000.0, -0.0001),
        (9, 7) => add(0).pulse(0.002, 0.35, 0.65),
        (9, 8) => r().speed(DEFAULT_VELOCITY * 4.0),
        (9, 60 | 63 | 64 | 70 | 76 | 83) => hidden(),
        (9, 61 | 65 | 66) => add(1).tex_v(1000.0, -0.001).pulse(0.002, 0.35, 0.65),
        (9, 72) => add(0).tex_v(10000.0, -0.0002),
        (9, 82) => add(0),
        // `MapManager.cpp:1168-1173`: `StreamMesh = 0` for 11, 12, 13, 73, 75 and 79 (no normal lighting);
        // `MoveObject` (4204-4256) scrolls them.
        (9, 11 | 13 | 73 | 75 | 79) => r().stream(0).tex_v(10000.0, -0.0002),
        (9, 12) => r().stream(0).tex_u(50000.0, -0.00005).tex_v(50000.0, -0.00005),
        // `RenderObject` (1052-1058): mesh 0 normal, then additive chrome (`BITMAP_CHROME`).
        (9, 81) => r().passes(TARKAN_81),

        // Blood Castle (`InBloodCastle`: WD_11BLOODCASTLE1..7 and WD_52, World12-18 and World53): 28/29 with
        // `RenderBodyShadow` without mesh 2 (`RenderObject` 1059-1076).
        (12..=18 | 53, 28 | 29) => ModelRules { body_shadow: Some(2), ..r() },

        // Heaven (WD_10HEAVEN): clouds
        (11, 0..=5) => hidden(),

        // Login (WD_73NEW_LOGIN_SCENE, World74) and character selection (WD_74NEW_CHARACTER_SCENE,
        // World75): `GMNewTown.cpp` forwards everything to `GMEmpireGuardian4.cpp` (and that one to
        // `GMEmpireGuardian1::RenderObjectMesh`). The effects are in `fx/kinds/objects_login.rs`.
        // The animation advances twice per tick: `PlayAnimation(Velocity)` in `MoveObject` and again
        // in `GMEmpireGuardian4::MoveObject` with `Velocity · factor` (×1 except the ones below).
        // Fire, water and smoke points (`MoveObject` / `GMNewTown::CreateObject`).
        (74 | 75, 79 | 80 | 82..=86 | 129..=132) => hidden(),
        // `GMUnitedMarketPlace::CreateObject` (the `MapProcess::CreateObject` chain does not check the map).
        (74 | 75, 67) => hidden(),
        (74 | 75, 20) => r().speed(DEFAULT_VELOCITY * 3.0),
        (74 | 75, 122..=124) => r().speed(DEFAULT_VELOCITY * 4.0),
        (74 | 75, 128) => r().speed(DEFAULT_VELOCITY * 7.0),
        // `o->Velocity = 0.02` in `MoveObject` (both passes).
        (74 | 75, 36) => r().speed(0.04),
        // `RenderAfterObjectMesh`: mesh 2 with `BlendMesh` 2 and light `(sin(WT·0.0015) + 1)·0.4 + 0.2`.
        (74 | 75, 0 | 1 | 3 | 44) => add(2).pulse(0.0015, 0.4, 0.6).speed(DEFAULT_VELOCITY * 2.0),
        // `GMEmpireGuardian1::RenderObjectMesh`: one mesh with `RENDER_BRIGHT` (additive).
        (74 | 75, 37) => add(1).speed(DEFAULT_VELOCITY * 2.0),
        // 41: `BodyLight · (sin(WT·0.002)·0.5 + 0.6)` on the additive mesh.
        (74 | 75, 41) => add(1).pulse(0.002, 0.5, 0.6).speed(DEFAULT_VELOCITY * 2.0),
        (74 | 75, 49 | 70) => add(0).speed(DEFAULT_VELOCITY * 2.0),
        // 64: mesh 1 with `RENDER_BRIGHT | RENDER_CHROME` twice and `BodyLight` (1, 0, 0); `MoveObject`
        // sets `Velocity = 0.64` (both passes: 1.28).
        // TODO(original): the chrome (`BITMAP_CHROME`, UV from the normal) in red; here it adds its texture.
        (74 | 75, 64) => add(1).speed(1.28),
        // `GMEmpireGuardian4::RenderObjectMesh`: `BodyLight` = 0.170382 (dark planes and trees).
        (74 | 75, 96 | 97 | 100) => r().body_light(0.170382).speed(DEFAULT_VELOCITY * 2.0),
        // Textures with an `_R` suffix (`TextureScript` → `getBright`): that mesh is its own `BlendMesh`.
        (74 | 75, 127) => add(0).speed(DEFAULT_VELOCITY * 2.0), // Kinglight_R
        (74 | 75, 157) => add(1).speed(DEFAULT_VELOCITY * 2.0), // Spider_R
        // TODO(original): 47/48 (`BodyLight` 1.2/1.1 on one mesh) and 115/117 (two `StreamMesh` with
        // `WorldTime % 25000 · ±0.0004`): none of them exist in World74/75.
        (74 | 75, _) => r().speed(DEFAULT_VELOCITY * 2.0),

        _ => r(),
    }
}

/// `b->BodyLight` of the first pass of Lost Tower 3/4/19/20.
const LOST_TOWER_RED: [f32; 3] = [1.0, 0.2, 0.1];
const LOST_TOWER_3_4: &[Pass] = &[
    Pass::body(flags::TEXTURE).texture(Bitmap::Chrome).light(LOST_TOWER_RED).stream(1),
    Pass::body(flags::TEXTURE).stream(-1),
];
const LOST_TOWER_19_20: &[Pass] = &[
    Pass::body(flags::TEXTURE).texture(Bitmap::Chrome).light(LOST_TOWER_RED).stream(2),
    Pass::body(flags::TEXTURE).stream(-1),
];
const TARKAN_81: &[Pass] = &[
    Pass::mesh(0, flags::TEXTURE),
    Pass::mesh(0, flags::CHROME | flags::BRIGHT).texture(Bitmap::Chrome),
];

/// What a particle emitter emits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmitKind {
    /// `CreateFire(0)`: flames (`BITMAP_FIRE`) with orange light.
    Fire,
    /// `CreateFire(1)`: light smoke (`BITMAP_SMOKE` subtype 0).
    Smoke,
    /// `CreateFire(2)`: dark smoke (`BITMAP_SMOKE` subtype 2, drawn subtractively).
    DarkSmoke,
}

/// A point on an object that emits particles every update (25 per second).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Emitter {
    pub kind: EmitKind,
    /// Offset from the object, in its own axes (MU units).
    pub offset: [f32; 3],
    /// Which model bone (possibly animated) it is attached to, if any.
    pub bone: Option<usize>,
    /// Random offset in X and Z of ± this value (the fountain splashes over an area).
    pub spread: f32,
}

impl Emitter {
    const fn at(kind: EmitKind, x: f32, y: f32, z: f32) -> Self {
        Self { kind, offset: [x, y, z], bone: None, spread: 0.0 }
    }
}

/// Particle emitters of an object type (the original's `MoveObject` / `UpdateObjectVisual`).
pub fn emitters(world: u16, kind: i16) -> Vec<Emitter> {
    use EmitKind::*;
    const fn fire(x: f32, y: f32, z: f32) -> Emitter {
        Emitter::at(Fire, x, y, z)
    }
    match (world, kind) {
        // Lorencia
        (1, 50) => vec![fire(0.0, 0.0, 200.0)],                                 // FireLight01
        (1, 51) => vec![fire(0.0, -30.0, 60.0)],                                // FireLight02
        (1, 52) => vec![fire(0.0, 0.0, 60.0)],                                  // Bonfire
        (1, 55) => vec![fire(-150.0, -150.0, 140.0), fire(150.0, -150.0, 140.0)], // DungeonGate
        (1, 80) => vec![fire(90.0, -200.0, 30.0), fire(90.0, 200.0, 30.0)],     // Bridge
        (1, 105) => vec![
            // Waterspout: splashes attached to bones 1 and 4 of the jet (1 in 2 ticks).
            Emitter { kind: Smoke, offset: [0.0, -20.0, 0.0], bone: Some(1), spread: 16.0 },
            Emitter { kind: Smoke, offset: [0.0, -80.0, 0.0], bone: Some(4), spread: 16.0 },
        ],
        (1, 130) => vec![fire(0.0, 0.0, 0.0)],                // Light01
        (1, 131) => vec![Emitter::at(Smoke, 0.0, 0.0, 0.0)],     // Light02
        (1, 132) => vec![Emitter::at(DarkSmoke, 0.0, 0.0, 0.0)], // Light03
        // Dungeon
        (2, 41) => vec![fire(0.0, -30.0, 240.0)],
        (2, 42) => vec![fire(0.0, 0.0, 190.0)],
        // Devias
        (3, 66) => vec![fire(0.0, 0.0, 50.0)],
        _ => vec![],
    }
}

/// Model path (`.bmd`) of an object type on a map, relative to `Data/`.
///
/// Lorencia uses its own names; the other maps use `Object{map}/Object{type+1:02}.bmd`.
pub fn model_path(world: u16, kind: i16) -> Option<PathBuf> {
    if world == 1 {
        let &(first, name, _) =
            LORENCIA_MODELS.iter().find(|&&(first, _, n)| kind >= first && kind < first + n)?;
        Some(PathBuf::from("Object1").join(format!("{name}{:02}.bmd", kind - first + 1)))
    } else {
        let folder = crate::terrain::data_world(world);
        Some(PathBuf::from(format!("Object{folder}")).join(format!("Object{:02}.bmd", kind + 1)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn objetos_y_modelos_de_lorencia() {
        let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../referencias/MuMain/src/bin/Data");
        if !data.exists() {
            return;
        }
        let objs = load(&data, 1).expect("leer EncTerrain1.obj");
        assert!(objs.len() > 100, "pocos objetos: {}", objs.len());
        // A misaligned read would produce absurd values (1e30, NaN...).
        // Some real objects sit slightly outside the edge, so we leave a margin.
        let dentro = |v: f32| (-2000.0..=27600.0).contains(&v);
        for o in &objs {
            assert!(dentro(o.position[0]) && dentro(o.position[1]) && o.scale > 0.0 && o.scale < 50.0, "{o:?}");
        }

        let mut por_tipo: BTreeMap<i16, usize> = BTreeMap::new();
        for o in &objs {
            *por_tipo.entry(o.kind).or_default() += 1;
        }
        let mut fallos = vec![];
        let mut sin_textura = vec![];
        for (&kind, &n) in &por_tipo {
            let Some(rel) = model_path(1, kind) else {
                fallos.push(format!("tipo {kind} ({n}x): sin modelo en la tabla"));
                continue;
            };
            let dir = data.join(rel.parent().unwrap());
            let name = rel.file_name().unwrap().to_str().unwrap();
            match crate::fs::read(&dir, name).and_then(|b| crate::bmd::read(&b)) {
                Ok(m) => {
                    for (i, mesh) in m.meshes.iter().enumerate() {
                        let posed = m.pose(i, 0, 0);
                        assert_eq!(posed.positions.len() % 3, 0);
                        let tex = crate::bmd::texture_file_name(&mesh.texture);
                        if crate::fs::find(&dir, &tex).is_none() {
                            sin_textura.push(format!("{name}: {} → {tex}", mesh.texture));
                        }
                    }
                }
                Err(e) => fallos.push(format!("tipo {kind} {name}: {e}")),
            }
        }
        // Diagnostics: meshes with an almost black texture (candidates for additive blending).
        for (&kind, &n) in &por_tipo {
            let Some(rel) = model_path(1, kind) else { continue };
            let dir = data.join(rel.parent().unwrap());
            let name = rel.file_name().unwrap().to_str().unwrap();
            let Ok(m) = crate::fs::read(&dir, name).and_then(|b| crate::bmd::read(&b)) else { continue };
            for (i, mesh) in m.meshes.iter().enumerate() {
                let tex = crate::bmd::texture_file_name(&mesh.texture);
                if let Ok(t) = crate::texture::load(&dir, &tex) {
                    let sum: u64 = t.rgba.chunks_exact(4).map(|p| p[0] as u64 + p[1] as u64 + p[2] as u64).sum();
                    let avg = sum as f64 / (t.rgba.len() / 4) as f64 / 765.0;
                    if avg < 0.15 {
                        eprintln!("oscura: tipo {kind} {name} malla {i} {tex} brillo {avg:.2} ({n} objetos)");
                    }
                }
            }
        }
        eprintln!("{} objetos, {} tipos", objs.len(), por_tipo.len());
        eprintln!("texturas no encontradas: {sin_textura:#?}");
        assert!(fallos.is_empty(), "{fallos:#?}");
    }
}
