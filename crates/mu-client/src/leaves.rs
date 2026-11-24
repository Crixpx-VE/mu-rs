//! Leaves, rain, snow and embers falling around the character: the original's `Leaves[MAX_LEAVES]`
//! (ZzzEffectFireLeave.cpp:118-559: `MoveLeaves`, `RenderLeaves` and their `Create*` /
//! `Move*`), which `MainScene` moves in ZzzScene.cpp:1963-1986 and draws in ZzzScene.cpp:2143/2169.
//!
//! What each map does (`WorldActive`; our folder is +1):
//!
//! | map | slots | what falls | drawing |
//! |------|----------|---------|--------|
//! | 0 Lorencia (not on `HeroTile` 4) | 80 | leaves (`CreateLorenciaLeaf`, moved by `MoveHeavenRain`) | 3×3 plane, alpha test |
//! | 2 Devias (not on `HeroTile` 3 or ≥ 10) | 80 | snow (`CreateDeviasSnow`), `leaf01` ×5 / 1 in 10 `leaf02` ×10 | sprite, additive; indoors neither moved nor drawn |
//! | 3 Noria, 7 Atlans | 80 | leaves (`CreateAtlanseLeaf`, `MoveEtcLeaf`) | 3×3 plane; Noria alpha test, Atlans additive |
//! | 9 Devil Square | 200 | rain with splashes (`CreateDevilSquareRain`) | 1×20 plane, alpha test |
//! | 10 Icarus | 80 | rain growing up to `RainCurrent` = 100, no splash (`CreateHeavenRain`) | 1×20 plane, additive |
//! | 18-23, 53 Chaos Castle | 80 | rain (`CreateChaosCastleRain`), splashes except on `TW_NOGROUND` | `TurningForce` plane, alpha test |
//! | 30 Valley of Loren | 40 | embers only during siege (`battleCastle::CreateFireSnuff`) | additive |
//! | 31 Land of Trials | 80 | `BITMAP_CLOUD` fog in zone 2 (`M31HuntingGround::CreateMist`) | drawn by the effects engine |
//! | 34 Crywolf | 60 / 50 / 80 | `weather` 1: rain (`rain011`) with splashes; 2: fog; 0: nothing | alpha test |
//! | 42 Balgass Refuge | 80 | embers (`CGM3rdChangeUp::CreateFireSnuff`) | scale×2 × scale×4 plane, additive |
//! | 57, 58 Raklion, 65 Doppelganger 1 | 80 | snow (`CGM_Raklion::CreateSnow`; `IsIceCity` includes 65) | sprite, additive |
//! | 62 Santa Town | 80 | snow (`CGMSantaTown::CreateSnow`) | sprite, additive |
//! | 63 Vulcanus, 66 Doppelganger 2 | 80 | embers (`CreateFireSpark`) | additive |
//! | 69-74 Imperial Guardian | 80 | rain only in rain/storm weather (sent by the server on entry) | 69-71 additive, 72-74 alpha test |
//! | 79 Loren Market | 80 | rain with subtype 2 splashes (`GMUnitedMarketPlace`) | additive |
//!
//! Original details copied as is:
//! - The slots are always the same and only get `Live = false` on map change
//!   (MapManager.cpp:1633): the rest (type, angle, position) stays from before. If no `Create*`
//!   function takes a free slot, it stays "alive" with what it had and `MoveEtcLeaf` moves it
//!   until it hits the ground (happens in Aida, Land of Trials, Crywolf with fog...).
//! - `RainSpeed`/`RainAngle` use `(int)sinf(...)`, which is almost always 0.
//! - Planes are drawn with `glColor3f(1,1,1)`: the slot's light only counts for sprites.
//! - `RenderLeaves` of ZzzScene.cpp:2143 (planes) and the one of 2169 (sprites, inside `BeginSprite`):
//!   each kind only shows in one of the two (in the other the matrix is wrong and it falls out of view).
//!
//! `InChaosCastle()` / `InBattleCastle()` without argument always return false in Main 5.2
//! (MapManager.cpp:1647, `if (iMap == -1) return false`); here they work as in MuMain
//! (MapManager.cpp:1410, which uses `WorldActive`), which is what the real client does.

use std::collections::HashMap;
use std::path::Path;

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use crate::Args;
use crate::character::Player;
use crate::fx::core::{angle_matrix, rotate, Particle, Rng, Spawn, BITMAP_CLOUD, BITMAP_RAIN_CIRCLE};
use crate::fx::draw::mu_to_bevy;
use crate::fx::material::{Blend, FxMaterial};
use crate::scene::{GameScene, WorldEntity};
use crate::terrain::{LoadedTerrain, MU_SCALE};

/// `MAX_LEAVES` (_define.h:430).
pub const MAX_LEAVES: usize = 200;

// `WorldActive` of the maps that have something here (MapManager.h).
const WD_0LORENCIA: i32 = 0;
const WD_2DEVIAS: i32 = 2;
const WD_3NORIA: i32 = 3;
const WD_7ATLANSE: i32 = 7;
const WD_9DEVILSQUARE: i32 = 9;
const WD_10HEAVEN: i32 = 10;
const WD_30BATTLECASTLE: i32 = 30;
const WD_31HUNTING_GROUND: i32 = 31;
const WD_33AIDA: i32 = 33;
const WD_34CRYWOLF_1ST: i32 = 34;
const WD_42CHANGEUP3RD_2ND: i32 = 42;
const WD_55LOGINSCENE: i32 = 55;
const WD_62SANTA_TOWN: i32 = 62;
const WD_63PK_FIELD: i32 = 63;
const WD_66DOPPLEGANGER2: i32 = 66;
const WD_79UNITEDMARKETPLACE: i32 = 79;

/// `WEATHER_TYPE` of GMEmpireGuardian1.h:45.
const WEATHER_SUN: i32 = 0;
const WEATHER_RAIN: i32 = 1;
const WEATHER_STORM: i32 = 3;

/// `InChaosCastle()` (with the MuMain fix, see above): `WD_18CHAOS_CASTLE`..`+5` and 53.
fn in_chaos_castle(w: i32) -> bool {
    (18..=23).contains(&w) || w == 53
}

/// `IsIceCity()` (GM_Raklion.cpp:2277): Raklion, its boss map and Doppelganger 1.
fn is_ice_city(w: i32) -> bool {
    matches!(w, 57 | 58 | 65)
}

/// `IsEmpireGuardian1..4()` (MapManager.cpp:1700-1722; 4 includes maps 73 and 74).
fn is_empire_guardian(w: i32) -> bool {
    (69..=74).contains(&w)
}

/// `M31HuntingGround::IsInHuntingGroundSection2` (GMHuntingGround.cpp:22).
fn in_hunting_ground_section2(p: Vec3) -> bool {
    p.x > 5449.0 && p.x < 17822.0 && p.y > 6784.0 && p.y < 22419.0
}

/// The condition of ZzzScene.cpp:1963-1985 to call `MoveLeaves` (`HeroTile` = terrain layer 1
/// under the character, ZzzInterface.cpp:7643).
pub fn leaves_move(w: i32, hero_tile: u8) -> bool {
    match w {
        WD_0LORENCIA => hero_tile != 4,
        WD_2DEVIAS => hero_tile != 3 && hero_tile < 10,
        WD_3NORIA | WD_7ATLANSE | WD_9DEVILSQUARE | WD_10HEAVEN | WD_30BATTLECASTLE | WD_31HUNTING_GROUND
        | WD_33AIDA | WD_34CRYWOLF_1ST | WD_42CHANGEUP3RD_2ND | WD_62SANTA_TOWN | WD_63PK_FIELD
        | WD_66DOPPLEGANGER2 | WD_79UNITEDMARKETPLACE => true,
        w => in_chaos_castle(w) || is_ice_city(w) || is_empire_guardian(w),
    }
}

/// Blend of `RenderLeaves` (ZzzEffectFireLeave.cpp:480-500): `EnableAlphaBlend` (additive) or
/// `EnableAlphaTest`.
fn render_blend(w: i32) -> Blend {
    let glow = matches!(w, WD_2DEVIAS | WD_7ATLANSE | WD_10HEAVEN | WD_62SANTA_TOWN | WD_63PK_FIELD | WD_66DOPPLEGANGER2 | 69 | 70 | 71 | WD_79UNITEDMARKETPLACE)
        || is_ice_city(w);
    if glow {
        Blend::Glow
    } else if in_chaos_castle(w) {
        Blend::AlphaTest
    } else if matches!(w, WD_30BATTLECASTLE | WD_55LOGINSCENE | WD_42CHANGEUP3RD_2ND) {
        Blend::Glow
    } else {
        Blend::AlphaTest
    }
}

/// Maps whose slots are sprites (`RenderSprite`, ZzzEffectFireLeave.cpp:522) instead of planes.
fn sprite_world(w: i32) -> bool {
    w == WD_2DEVIAS || is_ice_city(w) || w == WD_62SANTA_TOWN
}

/// Sprites only show in the `RenderLeaves` of ZzzScene.cpp:2169, which in Devias requires being
/// outdoors (`HeroTile != 3 && HeroTile < 10`). Planes, in the one of 2143, always.
fn leaves_visible(w: i32, hero_tile: u8) -> bool {
    !sprite_world(w) || w != WD_2DEVIAS || (hero_tile != 3 && hero_tile < 10)
}

/// `o->Type` of a slot: the texture it uses.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum LeafType {
    /// Never given a type (the global `Leaves[]` starts zeroed): not drawn.
    #[default]
    None,
    /// `BITMAP_LEAF1`: `WorldN\leaf01`.
    Leaf1,
    /// `BITMAP_LEAF2`: `WorldN\leaf02.jpg`.
    Leaf2,
    /// `BITMAP_RAIN`: `World1\rain01.tga` (`rain011.tga` in Crywolf).
    Rain,
    /// `BITMAP_FIRE_SNUFF`: `Effect\FireSnuff.jpg`.
    FireSnuff,
}

/// A slot of `Leaves[]` (the `PARTICLE` fields this code uses).
#[derive(Clone, Default)]
pub struct Leaf {
    pub live: bool,
    pub ty: LeafType,
    pub position: Vec3,
    pub start_position: Vec3,
    pub velocity: Vec3,
    pub angle: Vec3,
    pub turning_force: Vec3,
    pub light: Vec3,
    pub scale: f32,
    /// Previous tick state, for interpolation when drawing.
    prev_position: Vec3,
    prev_angle: Vec3,
}

/// What `MoveLeaves` reads from the rest of the game this tick.
pub struct Env<'a> {
    /// `gMapManager.WorldActive`.
    pub world: i32,
    /// `Hero->Object.Position` (MU units).
    pub hero: Vec3,
    /// `Hero->Object.Angle[2]` in degrees.
    pub hero_angle: f32,
    /// `CharacterMoveSpeed(Hero)` while walking (`Hero->Movement`), 0 when standing.
    pub hero_speed: f32,
    /// `CameraPosition[1]`.
    pub camera_y: f32,
    /// `WorldTime` (ms).
    pub world_time: f32,
    /// `RequestTerrainHeight(x, y)`.
    pub height: &'a dyn Fn(f32, f32) -> f32,
    /// `TERRAIN_ATTRIBUTE(x, y) & TW_NOGROUND`.
    pub no_ground: &'a dyn Fn(f32, f32) -> bool,
}

/// `Leaves[]` and the rain globals (ZzzEffectFireLeave.cpp:28-33).
#[derive(Resource)]
pub struct Leaves {
    pub slots: Vec<Leaf>,
    /// `RainTarget`: the server sets it in Lorencia (`ReceiveWeather`, WSclient.cpp:6353; no
    /// network yet), Icarus and Chaos Castle fix it at 100. Not cleared on map change.
    pub rain_target: i32,
    /// `RainCurrent`: moves towards `RainTarget` one step per tick.
    pub rain_current: i32,
    rain_speed: i32,
    rain_angle: i32,
    rain_position: i32,
    /// Crywolf `weather` (Winmain.cpp:84, `rand() % 3` in a global initializer: before
    /// `srand`, MSVC's first `rand()` gives 41 → 2, fog). Changes in
    /// `RenderCryWolf1stObjectVisual` (GMCrywolf1st.cpp:381-389), see [`crywolf_weather_clock`].
    pub weather: i32,
    /// `GMEmpireGuardian1::m_iWeather` (GMEmpireGuardian1.cpp:30): sun until the server sends
    /// another one on entry (WSclient.cpp:11982).
    eg_weather: i32,
    /// `IsBattleCastleStart() && InBattleCastle3(hero)`: only during the siege (without a server, never).
    siege_fire: bool,
    /// `Change_weather` (GMCrywolf1st.cpp:381).
    change_weather: u32,
    rng: Rng,
}

impl Default for Leaves {
    fn default() -> Self {
        Self {
            slots: vec![Leaf::default(); MAX_LEAVES],
            rain_target: 0,
            rain_current: 0,
            rain_speed: 30,
            rain_angle: 0,
            rain_position: 0,
            weather: 2,
            eg_weather: WEATHER_SUN,
            siege_fire: false,
            change_weather: 0,
            rng: Rng::default(),
        }
    }
}

/// One `MoveLeaves` tick with its `Create*` and `Move*`.
struct Sim<'a, 'b> {
    rng: &'a mut Rng,
    env: &'a Env<'b>,
    spawns: &'a mut Vec<Spawn>,
    rain_current: i32,
    rain_speed: i32,
    rain_angle: i32,
    rain_position: i32,
    weather: i32,
    eg_weather: i32,
    siege_fire: bool,
}

impl Sim<'_, '_> {
    fn rand(&mut self) -> i32 {
        self.rng.rand()
    }

    /// `(float)(rand() % n)`.
    fn rf(&mut self, n: i32) -> f32 {
        (self.rand() % n) as f32
    }

    /// `Hero->Object.Position + (rand()%nx + ox, rand()%ny + oy, rand()%nz + oz)` (in this order).
    fn around_hero(&mut self, (nx, ox): (i32, i32), (ny, oy): (i32, i32), (nz, oz): (i32, i32)) -> Vec3 {
        let x = self.rf(nx) + ox as f32;
        let y = self.rf(ny) + oy as f32;
        let z = self.rf(nz) + oz as f32;
        self.env.hero + Vec3::new(x, y, z)
    }

    fn height(&self, p: Vec3) -> f32 {
        (self.env.height)(p.x, p.y)
    }

    /// `CreateParticle(ty, o->Position, o->Angle, o->Light, sub_type)`.
    fn particle(&mut self, ty: i32, o: &Leaf, sub_type: i32) {
        self.spawns.push(Spawn::Particle(Particle::new(ty, o.position, o.angle, o.light, sub_type, 1.0, None)));
    }

    /// The rain splash: 1 in 4 `BITMAP_RAIN_CIRCLE`, otherwise `BITMAP_RAIN_CIRCLE + 1`.
    fn splash(&mut self, o: &Leaf, sub_type: i32) {
        let ty = if self.rand() % 4 == 0 { BITMAP_RAIN_CIRCLE } else { BITMAP_RAIN_CIRCLE + 1 };
        self.particle(ty, o, sub_type);
    }

    /// Angle and velocity of a drop: `AngleMatrix(Angle)` + `VectorRotate((0, 0, -speed))`.
    fn fall(o: &mut Leaf, angle_x: f32, speed: f32) {
        o.angle = Vec3::new(angle_x, 0.0, 0.0);
        o.velocity = rotate(o.angle, Vec3::new(0.0, 0.0, -speed));
    }

    /// Shared part of `CreateDevilSquareRain` (ZzzEffectFireLeave.cpp:118) and `CreateChaosCastleRain`
    /// (:164); `extra` is the Chaos Castle +20 speed.
    fn create_square_rain(&mut self, o: &mut Leaf, index: usize, extra: i32) {
        o.ty = LeafType::Rain;
        o.position = if index < 300 {
            self.around_hero((1600, -800), (1400, -500), (200, 300))
        } else {
            let x = self.rf(1600) - 800.0;
            let y = self.rf(300) + 1000.0 - self.rain_position as f32;
            let z = self.rf(200) + 300.0;
            self.env.hero + Vec3::new(x, y, z)
        };
        let angle = if self.rand() % 2 == 0 {
            -((self.rand() % 20 + 20) as f32)
        } else {
            -((self.rand() % 20 + 30 + self.rain_angle) as f32)
        };
        let speed = (self.rand() % 40 + self.rain_speed + extra) as f32;
        Self::fall(o, angle, speed);
    }

    /// `CreateLorenciaLeaf` (:205) / `CreateAtlanseLeaf` (:272) and the embers (`CreateFireSnuff`,
    /// `CreateFireSpark`): spawns near the character and drifts towards −X (or +X if in front of the
    /// camera) with a random spin. `flip` is 3.2 (leaves, siege) or 2.2 (embers).
    fn create_drift(&mut self, o: &mut Leaf, z: (i32, i32), flip: f32) {
        let position = self.around_hero((1600, -800), (1400, -500), z);
        o.position = position;
        o.start_position = position;
        o.velocity.x = -(self.rf(64) + 64.0) * 0.1;
        if position.y < self.env.camera_y + 400.0 {
            o.velocity.x = -o.velocity.x + flip;
        }
        o.velocity.y = (self.rf(32) - 16.0) * 0.1;
        o.velocity.z = (self.rf(32) - 16.0) * 0.1;
        o.turning_force.x = (self.rf(16) - 8.0) * 0.1;
        o.turning_force.y = (self.rf(64) - 32.0) * 0.1;
        o.turning_force.z = (self.rf(16) - 8.0) * 0.1;
    }

    /// Ember: `o->Scale = rand()%50/100.f + base` and then [`Self::create_drift`].
    fn create_fire_snuff(&mut self, o: &mut Leaf, base: f32, z: (i32, i32), flip: f32) {
        o.ty = LeafType::FireSnuff;
        o.scale = self.rf(50) / 100.0 + base;
        self.create_drift(o, z, flip);
    }

    /// `M31HuntingGround::CreateMist` (GMHuntingGround.cpp:915) and `M34CryWolf1st::CreateMist`
    /// (GMCrywolf1st.cpp:2039): a `BITMAP_CLOUD` subtype 8 cloud in one of 8 directions, further
    /// ahead if the character is walking. `ff` is what Crywolf subtracts from the distances (200); in
    /// Crywolf the height stays at 400 and in Land of Trials it sits on the ground.
    fn create_mist(&mut self, o: &mut Leaf, light: Vec3, ff: f32, on_ground: bool) {
        let hero = self.env.hero;
        let mut target = Vec3::new(0.0, 0.0, if on_ground { 0.0 } else { 400.0 });
        let d = |s: &mut Self, base: f32| base - ff + s.rf(250);
        match self.rand() % 8 {
            0 => {
                target.x = hero.x + d(self, 300.0);
                target.y = hero.y + d(self, 300.0);
            }
            1 => {
                target.x = hero.x + d(self, 250.0);
                target.y = hero.y - d(self, 250.0);
            }
            2 => {
                target.x = hero.x - d(self, 200.0);
                target.y = hero.y + d(self, 200.0);
            }
            3 => {
                target.x = hero.x - d(self, 300.0);
                target.y = hero.y - d(self, 300.0);
            }
            4 => {
                target.x = hero.x + d(self, 400.0);
                target.y = hero.y;
            }
            5 => {
                target.x = hero.x - d(self, 400.0);
                target.y = hero.y;
            }
            6 => {
                target.x = hero.x;
                target.y = hero.y + d(self, 400.0);
            }
            _ => {
                target.x = hero.x;
                target.y = hero.y - d(self, 400.0);
            }
        }
        let moving = self.env.hero_speed > 0.0;
        if moving {
            target += rotate(Vec3::new(0.0, 0.0, self.env.hero_angle), Vec3::new(0.0, -45.0 * self.env.hero_speed, 0.0));
        }
        if moving || self.rand() % 2 == 0 {
            if on_ground {
                target.z = self.rf(20) + (self.env.height)(target.x, target.y);
            }
            self.spawns.push(Spawn::Particle(Particle::new(BITMAP_CLOUD, target, Vec3::ZERO, light, 8, 0.4, None)));
        }
        o.live = false;
    }

    /// The `Create*` chain of `MoveLeaves` (ZzzEffectFireLeave.cpp:445-465). False if none
    /// takes the slot (which then stays "alive" with what it had).
    fn create(&mut self, o: &mut Leaf, index: usize) -> bool {
        let w = self.env.world;
        // `CreateDevilSquareRain`: also Crywolf with rain.
        if w == WD_9DEVILSQUARE || (w == WD_34CRYWOLF_1ST && self.weather == 1) {
            self.create_square_rain(o, index, 0);
            return true;
        }
        if in_chaos_castle(w) {
            o.turning_force.x = 1.0;
            o.turning_force.y = 30.0 + self.rf(10);
            self.create_square_rain(o, index, 20);
            return true;
        }
        if w == WD_0LORENCIA {
            o.ty = LeafType::Leaf1;
            self.create_drift(o, (300, 50), 3.2);
            return true;
        }
        // `CreateHeavenRain` (:227): only the first `RainCurrent·MAX_LEAVES/100` slots.
        if w == WD_10HEAVEN {
            let rainly = self.rain_current * MAX_LEAVES as i32 / 100;
            if (index as i32) < rainly {
                o.ty = LeafType::Rain;
                o.position = self.around_hero((1600, -800), (1400, -500), (200, 200));
                let speed = self.rf(24) + 20.0;
                Self::fall(o, -30.0, speed);
            }
            return true;
        }
        // `CreateDeviasSnow` (:248).
        if w == WD_2DEVIAS {
            o.ty = LeafType::Leaf1;
            o.scale = 5.0;
            if self.rand() % 10 == 0 {
                o.ty = LeafType::Leaf2;
                o.scale = 10.0;
            }
            o.position = self.around_hero((1600, -800), (1400, -500), (200, 200));
            let speed = self.rf(16) + 8.0;
            Self::fall(o, -30.0, speed);
            return true;
        }
        if w == WD_3NORIA || w == WD_7ATLANSE {
            o.ty = LeafType::Leaf1;
            self.create_drift(o, (300, 50), 3.2);
            return true;
        }
        // `battleCastle::CreateFireSnuff` (GMBattleCastle.cpp:580): always in the old login; in
        // Valley of Loren only during siege and inside the castle. Red light (not visible: white plane).
        if w == WD_55LOGINSCENE || (w == WD_30BATTLECASTLE && self.siege_fire) {
            let z = if w == WD_55LOGINSCENE { (1000, 50) } else { (300, 50) };
            self.create_fire_snuff(o, 0.5, z, 3.2);
            o.light = Vec3::new(1.0, 0.0, 0.0);
            return true;
        }
        if w == WD_31HUNTING_GROUND && in_hunting_ground_section2(self.env.hero) && self.rand() % 30 == 0 {
            self.create_mist(o, Vec3::new(0.05, 0.05, 0.1), 0.0, true);
            return true;
        }
        // `M33Aida::CreateMist` (GMAida.cpp:1566) always returns false.
        if self.weather == 2 && w == WD_34CRYWOLF_1ST && self.rand() % 30 == 0 {
            self.create_mist(o, Vec3::splat(0.07), 200.0, false);
            return true;
        }
        // `CGM3rdChangeUp::CreateFireSnuff` (GM3rdChangeUp.cpp:330): Balgass Refuge.
        if w == WD_42CHANGEUP3RD_2ND {
            self.create_fire_snuff(o, 0.4, (300, 50), 2.2);
            return true;
        }
        // `CGM_Raklion::CreateSnow` (GM_Raklion.cpp:2240).
        if is_ice_city(w) {
            o.ty = LeafType::Leaf1;
            o.scale = self.rf(10) + 3.0;
            if self.rand() % 10 == 0 {
                o.scale = self.rf(3) + 10.0;
            }
            o.position = self.around_hero((1600, -800), (1400, -500), (200, 200));
            let angle = -(self.rf(30) + 50.0);
            let speed = self.rf(20) + 30.0;
            Self::fall(o, angle, speed);
            return true;
        }
        // `CGMSantaTown::CreateSnow` (GMSantaTown.cpp:204).
        if w == WD_62SANTA_TOWN {
            o.ty = LeafType::Leaf1;
            o.scale = self.rf(10) + 5.0;
            if self.rand() % 10 == 0 {
                o.ty = LeafType::Leaf2;
                o.scale = 12.0;
            }
            o.position = self.around_hero((1600, -800), (1400, -500), (200, 200));
            let speed = self.rf(8) + 4.0;
            Self::fall(o, -20.0, speed);
            return true;
        }
        // `CGM_PK_Field::CreateFireSpark` (GM_PK_Field.cpp:1040) and `CGMDoppelGanger2::CreateFireSpark`
        // (GMDoppelGanger2.cpp:637).
        if w == WD_63PK_FIELD || w == WD_66DOPPLEGANGER2 {
            self.create_fire_snuff(o, 0.4, (300, 50), 2.2);
            o.light = Vec3::new(1.0, 0.0, 0.0);
            return true;
        }
        // `GMEmpireGuardian1::CreateRain` (GMEmpireGuardian1.cpp:2208): spawns, advances one step and
        // half die right away with a splash. Returns true even when sunny.
        if is_empire_guardian(w) {
            if self.eg_weather == WEATHER_RAIN || self.eg_weather == WEATHER_STORM {
                o.ty = LeafType::Rain;
                o.position = self.around_hero((800, -300), (800, -400), (200, 400));
                let speed = self.rf(24) + 30.0;
                Self::fall(o, -30.0, speed);
                o.position += o.velocity;
                let h = self.height(o.position);
                if self.rand() % 2 == 0 {
                    o.live = false;
                    o.position.z = h + 10.0;
                    self.splash(o, 0);
                }
            }
            return true;
        }
        // `GMUnitedMarketPlace::CreateRain` (GMUnitedMarketPlace.cpp:517).
        if w == WD_79UNITEDMARKETPLACE {
            o.ty = LeafType::Rain;
            o.position = self.around_hero((1600, -800), (1400, -500), (200, 200));
            let speed = self.rf(24) + 20.0;
            Self::fall(o, -30.0, speed);
            return true;
        }
        false
    }

    /// Leaf flying with a spin (`MoveHeavenRain` :354-371, `GMUnitedMarketPlace::MoveRain`):
    /// random speed and spin, dies at √200000 ≈ 447 units from where it started.
    fn tumble(&mut self, o: &mut Leaf) {
        o.velocity.x += (self.rf(16) - 8.0) * 0.1;
        o.velocity.y += (self.rf(16) - 8.0) * 0.1;
        o.velocity.z += (self.rf(16) - 8.0) * 0.1;
        o.position += o.velocity;
        o.turning_force.x += (self.rf(8) - 4.0) * 0.02;
        o.turning_force.y += (self.rf(16) - 8.0) * 0.02;
        o.turning_force.z += (self.rf(8) - 4.0) * 0.02;
        o.angle += o.turning_force;
        if (o.start_position - o.position).length_squared() >= 200_000.0 {
            o.live = false;
        }
    }

    /// The `Move*` chain of `MoveLeaves` (ZzzEffectFireLeave.cpp:468-473).
    fn step(&mut self, o: &mut Leaf) {
        let w = self.env.world;
        // `MoveDevilSquareRain` (:295) and `MoveChaosCastleRain` (:321).
        let square = w == WD_9DEVILSQUARE || (w == WD_34CRYWOLF_1ST && self.weather == 1);
        if square || in_chaos_castle(w) {
            o.position += o.velocity;
            let h = self.height(o.position);
            if o.position.z < h && (square || !(self.env.no_ground)(o.position.x, o.position.y)) {
                o.live = false;
                o.position.z = h + 10.0;
                self.splash(o, 0);
            }
            return;
        }
        // `MoveHeavenRain` (:339): Lorencia and Icarus; the Icarus rain does not splash.
        // `GMUnitedMarketPlace::MoveRain` (GMUnitedMarketPlace.cpp:538): same, with splash 2.
        if w == WD_0LORENCIA || w == WD_10HEAVEN || w == WD_79UNITEDMARKETPLACE {
            if o.ty == LeafType::Rain {
                o.position += o.velocity;
                let h = self.height(o.position);
                if o.position.z < h {
                    o.live = false;
                    o.position.z = h + 10.0;
                    if w == WD_79UNITEDMARKETPLACE {
                        self.splash(o, 2);
                    } else if w != WD_10HEAVEN {
                        self.particle(BITMAP_RAIN_CIRCLE, o, 0);
                    }
                }
            } else {
                self.tumble(o);
            }
            return;
        }
        // `MoveEtcLeaf` (:376): tumbles down and on the ground fades out in 20 ticks.
        let h = self.height(o.position);
        if o.position.z <= h {
            o.position.z = h;
            o.light -= Vec3::splat(0.05);
            if o.light.x <= 0.0 {
                o.live = false;
            }
        } else {
            o.velocity.x += (self.rf(16) - 8.0) * 0.1;
            o.velocity.y += (self.rf(16) - 8.0) * 0.1;
            o.velocity.z += (self.rf(16) - 8.0) * 0.1;
            o.position += o.velocity;
        }
    }
}

impl Leaves {
    /// `iMaxLeaves` and `RainTarget` of `MoveLeaves` (ZzzEffectFireLeave.cpp:399-425).
    fn max_leaves(&mut self, w: i32) -> usize {
        let mut max = if w == WD_9DEVILSQUARE { MAX_LEAVES } else { 80 };
        if w == WD_10HEAVEN {
            self.rain_target = MAX_LEAVES as i32 / 2;
        } else if in_chaos_castle(w) {
            self.rain_target = MAX_LEAVES as i32 / 2;
            max = 80;
        } else if w == WD_30BATTLECASTLE {
            max = 40;
        } else if w == WD_55LOGINSCENE {
            max = 80;
        } else if w == WD_34CRYWOLF_1ST {
            if self.weather == 1 {
                max = 60;
            } else if self.weather == 2 {
                max = 50;
            }
        }
        max
    }

    /// `MoveLeaves` (ZzzEffectFireLeave.cpp:397). What it asks the effects engine to create
    /// (splashes, fog) goes to `spawns`.
    pub fn tick(&mut self, env: &Env, spawns: &mut Vec<Spawn>) {
        let max = self.max_leaves(env.world);
        if self.rain_current > self.rain_target {
            self.rain_current -= 1;
        } else if self.rain_current < self.rain_target {
            self.rain_current += 1;
        }
        // `(int)sinf(...)`: the sine is truncated, not the product.
        self.rain_speed = ((env.world_time * 0.001).sin() as i32) * 10 + 30;
        self.rain_angle = ((env.world_time * 0.0005 + 50.0).sin() as i32) * 20;
        self.rain_position = (self.rain_position + 20) % 2000;

        let Leaves { slots, rng, rain_current, rain_speed, rain_angle, rain_position, weather, eg_weather, siege_fire, .. } = self;
        let mut sim = Sim {
            rng,
            env,
            spawns,
            rain_current: *rain_current,
            rain_speed: *rain_speed,
            rain_angle: *rain_angle,
            rain_position: *rain_position,
            weather: *weather,
            eg_weather: *eg_weather,
            siege_fire: *siege_fire,
        };
        for (i, o) in slots.iter_mut().take(max).enumerate() {
            if !o.live {
                o.light = Vec3::ONE;
                o.live = true;
                sim.create(o, i);
                // Freshly created: no interpolation from where it was before.
                o.prev_position = o.position;
                o.prev_angle = o.angle;
            } else {
                sim.step(o);
            }
        }
    }
}

/// On map change (MapManager.cpp:1633): all slots dead (the rest stays) and the previous map's
/// textures and meshes dropped.
pub fn reset(mut leaves: ResMut<Leaves>, mut render: ResMut<LeafRender>) {
    for o in &mut leaves.slots {
        o.live = false;
    }
    *render = LeafRender::default();
}

pub struct LeavesPlugin;

impl Plugin for LeavesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Leaves>()
            .init_resource::<LeafRender>()
            .add_systems(FixedUpdate, step_leaves.before(crate::fx::sim::tick).run_if(in_state(GameScene::Game)))
            .add_systems(Update, crywolf_weather_clock.run_if(in_state(GameScene::Game)))
            .add_systems(
                PostUpdate,
                draw_leaves
                    .run_if(in_state(GameScene::Game))
                    .after(crate::camera::CameraMoved)
                    .before(TransformSystems::Propagate),
            );
    }
}

/// `HeroTile`: the layer 1 texture under the character (ZzzInterface.cpp:7640-7643).
fn hero_tile(t: &LoadedTerrain, pos: Vec2) -> u8 {
    let (x, y) = ((pos.x.max(0.0) as usize).min(255), (pos.y.max(0.0) as usize).min(255));
    t.0.mapping.layer1.get(y * 256 + x).copied().unwrap_or(0)
}

/// MU camera (x, y, z up) from the Bevy one.
fn camera_mu(t: &Transform) -> Vec3 {
    Vec3::new(t.translation.x, -t.translation.z, t.translation.y) * MU_SCALE
}

#[allow(clippy::too_many_arguments)]
fn step_leaves(
    args: Res<Args>,
    mut leaves: ResMut<Leaves>,
    players: Query<&Player>,
    terrain: Option<Res<LoadedTerrain>>,
    cameras: Query<&Transform, (With<Camera3d>, With<IsDefaultUiCamera>)>,
    time: Res<Time<Fixed>>,
    mut fx: ResMut<crate::fx::FxWorld>,
    mut last_hero: Local<Option<Vec2>>,
) {
    // Drawing interpolates from here: what does not move this tick stays still.
    for o in &mut leaves.slots {
        o.prev_position = o.position;
        o.prev_angle = o.angle;
    }
    let (Ok(p), Some(t)) = (players.single(), terrain) else { return };
    // `CharacterMoveSpeed`: MU units per tick, measured (a jump of more than one tile is a
    // teleport, not a step).
    let moved = last_hero.map_or(0.0, |l| (p.pos - l).length() * MU_SCALE);
    *last_hero = Some(p.pos);
    let speed = if moved <= MU_SCALE { moved } else { 0.0 };
    let world = args.world as i32 - 1;
    if !leaves_move(world, hero_tile(&t, p.pos)) {
        return;
    }
    let height = |x: f32, y: f32| t.0.height_at(x / MU_SCALE, y / MU_SCALE);
    let no_ground = |x: f32, y: f32| {
        let (tx, ty) = ((x / MU_SCALE).max(0.0) as usize, (y / MU_SCALE).max(0.0) as usize);
        t.0.attribute(tx, ty) & mu_data::terrain::attr::NO_GROUND != 0
    };
    let env = Env {
        world,
        hero: Vec3::new(p.pos.x * MU_SCALE, p.pos.y * MU_SCALE, t.0.height_at(p.pos.x, p.pos.y)),
        hero_angle: p.facing.to_degrees(),
        hero_speed: speed,
        camera_y: cameras.single().map_or(0.0, |c| camera_mu(c).y),
        world_time: time.elapsed_secs() * 1000.0,
        height: &height,
        no_ground: &no_ground,
    };
    let mut spawns = Vec::new();
    leaves.tick(&env, &mut spawns);
    for s in spawns {
        crate::fx::request(&mut fx, s);
    }
}

/// `Change_weather` of `RenderCryWolf1stObjectVisual` (GMCrywolf1st.cpp:381-389): adds one per
/// map object drawn each frame and at 70000 rolls `weather = rand() % 3`. Here the visible map
/// meshes of this frame are counted (approximation: the original counts objects with its own
/// culling).
fn crywolf_weather_clock(
    args: Res<Args>,
    mut leaves: ResMut<Leaves>,
    visible: Query<&ViewVisibility, (With<WorldEntity>, With<Mesh3d>, Without<LeafBatch>)>,
) {
    if args.world as i32 - 1 != WD_34CRYWOLF_1ST {
        return;
    }
    let n = visible.iter().filter(|v| v.get()).count() as u32;
    for _ in 0..n {
        leaves.change_weather += 1;
        if leaves.change_weather > 70_000 {
            leaves.weather = leaves.rng.rand() % 3;
            leaves.change_weather = 0;
        }
    }
}

// --- Drawing (`RenderLeaves`, ZzzEffectFireLeave.cpp:478) ------------------------------------------

/// Mesh of the slots of one type (rebuilt every frame).
#[derive(Component)]
struct LeafBatch;

/// Textures of the loaded map (`BITMAP_LEAF1`, `LEAF2`, `RAIN`, `FIRE_SNUFF`) and one mesh per type.
#[derive(Resource, Default)]
pub struct LeafRender {
    world: Option<u16>,
    /// (image, channels: 3 = JPG, 4 = TGA) of each type.
    textures: HashMap<LeafType, Option<(Handle<Image>, u8)>>,
    batches: HashMap<LeafType, (Handle<Mesh>, Entity)>,
}

/// `LoadBitmap(folder\file, ..., filter, GL_CLAMP_TO_EDGE)`: no mipmaps, like the original.
fn load_texture(data: &Path, folder: &str, file: &str, nearest: bool, images: &mut Assets<Image>) -> Option<(Handle<Image>, u8)> {
    let dir = mu_data::fs::find(data, folder)?;
    let name = mu_data::bmd::texture_file_name(file);
    let t = match mu_data::texture::load(&dir, &name) {
        Ok(t) => t,
        Err(e) => {
            debug!("hojas: no hay {folder}/{name}: {e}");
            return None;
        }
    };
    let mut image = crate::terrain::to_bevy_image(&t);
    let filter = if nearest { ImageFilterMode::Nearest } else { ImageFilterMode::Linear };
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: filter,
        min_filter: filter,
        mipmap_filter: ImageFilterMode::Nearest,
        lod_max_clamp: 0.0,
        ..default()
    });
    let components = if name.to_ascii_lowercase().ends_with(".ozt") { 4 } else { 3 };
    Some((images.add(image), components))
}

/// `BITMAP_LEAF1` file (MapManager.cpp:1557-1560): Main 5.2 loads `leaf01.tga` and then
/// `leaf01.jpg`, which replaces it (or deletes it if missing: `CGlobalBitmap::LoadImage`). MuMain
/// (MapManager.cpp, `Map==0||Map==3||Map==63`) keeps the `.tga` in Lorencia, Noria and 63,
/// which is what the client shows: Lorencia leaves with their silhouette and Noria with leaves.
fn leaf1_file(w: i32) -> &'static str {
    if matches!(w, WD_0LORENCIA | WD_3NORIA | WD_63PK_FIELD) { "leaf01.tga" } else { "leaf01.jpg" }
}

fn load_textures(args: &Args, images: &mut Assets<Image>) -> HashMap<LeafType, Option<(Handle<Image>, u8)>> {
    let w = args.world as i32 - 1;
    let folder = format!("World{}", mu_data::terrain::data_world(args.world));
    let rain = if w == WD_34CRYWOLF_1ST { "rain011.tga" } else { "rain01.tga" };
    HashMap::from([
        (LeafType::Leaf1, load_texture(&args.data, &folder, leaf1_file(w), true, images)),
        (LeafType::Leaf2, load_texture(&args.data, &folder, "leaf02.jpg", true, images)),
        (LeafType::Rain, load_texture(&args.data, "World1", rain, true, images)),
        // ZzzOpenData.cpp:5080, `GL_LINEAR`.
        (LeafType::FireSnuff, load_texture(&args.data, "Effect", "FireSnuff.jpg", false, images)),
    ])
}

#[derive(Default)]
struct Batch {
    pos: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    color: Vec<[f32; 4]>,
    idx: Vec<u32>,
}

impl Batch {
    fn quad(&mut self, p: [Vec3; 4], uv: [[f32; 2]; 4], color: [f32; 4]) {
        let base = self.pos.len() as u32;
        for (k, v) in p.iter().enumerate() {
            self.pos.push(mu_to_bevy(*v).to_array());
            self.uv.push(uv[k]);
            self.color.push(color);
        }
        self.idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

/// Original color (sRGB, clamped to [0, 1] like `PackABGR`) → linear.
fn linear(c: [f32; 4]) -> [f32; 4] {
    let f = |v: f32| v.clamp(0.0, 1.0).powf(2.2);
    [f(c[0]), f(c[1]), f(c[2]), c[3].clamp(0.0, 1.0)]
}

/// `RenderPlane3D(width, height, AngleMatrix(angle))` translated to `position`
/// (ZzzOpenglUtil.cpp:930): corners (−w,−w,−h), (w,w,−h), (w,w,h), (−w,−w,h).
fn plane_corners(position: Vec3, angle: Vec3, w: f32, h: f32) -> [Vec3; 4] {
    let m = angle_matrix(angle);
    [Vec3::new(-w, -w, -h), Vec3::new(w, w, -h), Vec3::new(w, w, h), Vec3::new(-w, -w, h)].map(|v| position + m * v)
}

/// Plane size of a slot (ZzzEffectFireLeave.cpp:531-553); `None` = not drawn.
fn plane_size(w: i32, weather: i32, o: &Leaf) -> Option<(f32, f32)> {
    if in_chaos_castle(w) {
        return Some((o.turning_force.x, o.turning_force.y));
    }
    match o.ty {
        LeafType::Rain if w == WD_34CRYWOLF_1ST && weather != 1 => None,
        LeafType::Rain => Some((1.0, 20.0)),
        LeafType::FireSnuff => Some((o.scale * 2.0, o.scale * 4.0)),
        _ => Some((3.0, 3.0)),
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_leaves(
    mut commands: Commands,
    args: Res<Args>,
    leaves: Res<Leaves>,
    mut render: ResMut<LeafRender>,
    players: Query<&Player>,
    terrain: Option<Res<LoadedTerrain>>,
    fixed: Res<Time<Fixed>>,
    cameras: Query<&Transform, (With<Camera3d>, With<IsDefaultUiCamera>)>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<FxMaterial>>,
    mut visibility: Query<&mut Visibility, With<LeafBatch>>,
) {
    let (Ok(cam), Ok(p), Some(t)) = (cameras.single(), players.single(), terrain) else { return };
    let render = &mut *render;
    if render.world != Some(args.world) {
        // The previous map's meshes were already despawned with it (`WorldEntity`).
        render.world = Some(args.world);
        render.textures = load_textures(&args, &mut images);
        render.batches.clear();
        let mut found: Vec<String> = render.textures.iter().filter(|(_, t)| t.is_some()).map(|(ty, _)| format!("{ty:?}")).collect();
        found.sort();
        info!("hojas: World{} con texturas {}", args.world, found.join(", "));
    }
    let w = args.world as i32 - 1;
    let blend = render_blend(w);
    let sprites = sprite_world(w);
    let mut out: HashMap<LeafType, Batch> = HashMap::new();
    if leaves_visible(w, hero_tile(&t, p.pos)) {
        let a = fixed.overstep_fraction();
        let to_mu = |v: Vec3| Vec3::new(v.x, -v.z, v.y);
        let (right, up) = (to_mu(cam.right().as_vec3()), to_mu(cam.up().as_vec3()));
        for o in leaves.slots.iter().filter(|o| o.live) {
            let Some(Some((_, components))) = render.textures.get(&o.ty) else { continue };
            let position = o.prev_position.lerp(o.position, a);
            if sprites {
                // `RenderSprite(Type, Position, Scale, Scale, Light)` (ZzzOpenglUtil.cpp:963).
                let s = o.scale * 0.5;
                let corners = [(-s, -s), (s, -s), (s, s), (-s, s)].map(|(dx, dy)| position + right * dx + up * dy);
                let alpha = if *components == 3 { 1.0 } else { o.light.x };
                let color = linear([o.light.x, o.light.y, o.light.z, alpha]);
                out.entry(o.ty).or_default().quad(corners, [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]], color);
            } else if let Some((pw, ph)) = plane_size(w, leaves.weather, o) {
                let angle = o.prev_angle.lerp(o.angle, a);
                let corners = plane_corners(position, angle, pw, ph);
                out.entry(o.ty).or_default().quad(corners, [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]], [1.0; 4]);
            }
        }
    }

    // Empty batches are hidden (Bevy warns about every empty mesh it uploads).
    for (ty, (_, entity)) in &render.batches {
        if let Ok(mut v) = visibility.get_mut(*entity) {
            v.set_if_neq(if out.contains_key(ty) { Visibility::Inherited } else { Visibility::Hidden });
        }
    }
    for (ty, b) in out {
        let texture = render.textures.get(&ty).cloned().flatten().map(|t| t.0);
        let (mesh, _) = render.batches.entry(ty).or_insert_with(|| {
            let mesh = meshes.add(
                Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
                    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, Vec::<[f32; 3]>::new())
                    .with_inserted_indices(Indices::U32(vec![])),
            );
            let entity = commands
                .spawn((
                    Name::new(format!("Hojas {ty:?}")),
                    LeafBatch,
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(materials.add(FxMaterial::new(texture, blend))),
                    Transform::default(),
                    bevy::camera::visibility::NoFrustumCulling,
                    WorldEntity,
                ))
                .id();
            (mesh, entity)
        });
        if let Some(mut m) = meshes.get_mut(&*mesh) {
            let n = b.pos.len();
            m.insert_attribute(Mesh::ATTRIBUTE_POSITION, b.pos);
            m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; n]);
            m.insert_attribute(Mesh::ATTRIBUTE_UV_0, b.uv);
            m.insert_attribute(Mesh::ATTRIBUTE_COLOR, b.color);
            m.insert_indices(Indices::U32(b.idx));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(_: f32, _: f32) -> f32 {
        0.0
    }
    fn ground(_: f32, _: f32) -> bool {
        false
    }

    fn env(world: i32) -> Env<'static> {
        Env {
            world,
            hero: Vec3::new(12_800.0, 12_800.0, 0.0),
            hero_angle: 0.0,
            hero_speed: 0.0,
            camera_y: 12_000.0,
            world_time: 0.0,
            height: &flat,
            no_ground: &ground,
        }
    }

    fn live(l: &Leaves) -> usize {
        l.slots.iter().filter(|o| o.live).count()
    }

    #[test]
    fn casillas_por_mapa() {
        let mut l = Leaves::default();
        assert_eq!(l.max_leaves(WD_9DEVILSQUARE), 200);
        assert_eq!(l.max_leaves(WD_0LORENCIA), 80);
        assert_eq!(l.max_leaves(WD_30BATTLECASTLE), 40);
        assert_eq!(l.max_leaves(20), 80);
        assert_eq!(l.rain_target, 100);
        l.weather = 1;
        assert_eq!(l.max_leaves(WD_34CRYWOLF_1ST), 60);
        l.weather = 2;
        assert_eq!(l.max_leaves(WD_34CRYWOLF_1ST), 50);
        l.weather = 0;
        assert_eq!(l.max_leaves(WD_34CRYWOLF_1ST), 80);
    }

    #[test]
    fn lorencia_hojas_alrededor_del_personaje() {
        let mut l = Leaves::default();
        let e = env(WD_0LORENCIA);
        let mut s = vec![];
        l.tick(&e, &mut s);
        assert_eq!(live(&l), 80);
        for o in l.slots.iter().take(80) {
            assert_eq!(o.ty, LeafType::Leaf1);
            let d = o.position - e.hero;
            assert!((-800.0..800.0).contains(&d.x) && (-500.0..900.0).contains(&d.y) && (50.0..350.0).contains(&d.z));
            // In front of the camera (y < camera + 400) it drifts towards +X.
            if o.position.y < e.camera_y + 400.0 {
                assert!(o.velocity.x > 0.0);
            } else {
                assert!(o.velocity.x < 0.0);
            }
        }
        assert!(l.slots[80..].iter().all(|o| !o.live));
    }

    #[test]
    fn lorencia_hoja_muere_lejos_de_donde_salio() {
        let mut l = Leaves::default();
        let e = env(WD_0LORENCIA);
        let mut s = vec![];
        l.tick(&e, &mut s);
        for _ in 0..2000 {
            let before = l.slots[0].clone();
            l.tick(&e, &mut s);
            if !l.slots[0].live {
                assert!((before.start_position - before.position).length() < 500.0);
                return;
            }
            if l.slots[0].start_position != before.start_position {
                break;
            }
        }
        panic!("la hoja no muere");
    }

    #[test]
    fn devias_nieve_cae_inclinada() {
        let mut l = Leaves::default();
        let mut s = vec![];
        l.tick(&env(WD_2DEVIAS), &mut s);
        for o in l.slots.iter().take(80) {
            assert!((o.ty == LeafType::Leaf1 && o.scale == 5.0) || (o.ty == LeafType::Leaf2 && o.scale == 10.0));
            let speed = o.velocity.length();
            assert!((8.0..24.0).contains(&speed));
            // −30° rotation around X: (0, −0.5, −0.866)·v.
            assert!((o.velocity.z / speed + 30f32.to_radians().cos()).abs() < 1e-4);
            assert!((o.velocity.y / speed + 0.5).abs() < 1e-4);
        }
    }

    #[test]
    fn en_el_suelo_se_apaga_y_muere() {
        let mut l = Leaves::default();
        let mut s = vec![];
        let e = env(WD_2DEVIAS);
        l.tick(&e, &mut s);
        l.slots[0].position.z = -1.0;
        let mut ticks = 0;
        while l.slots[0].live {
            Sim {
                rng: &mut l.rng,
                env: &e,
                spawns: &mut s,
                rain_current: 0,
                rain_speed: 30,
                rain_angle: 0,
                rain_position: 0,
                weather: 2,
                eg_weather: WEATHER_SUN,
                siege_fire: false,
            }
            .step(&mut l.slots[0]);
            ticks += 1;
            assert!(ticks <= 21);
        }
        assert!(ticks >= 20);
    }

    #[test]
    fn icarus_lluvia_crece_poco_a_poco() {
        let mut l = Leaves::default();
        let mut s = vec![];
        l.tick(&env(WD_10HEAVEN), &mut s);
        // RainCurrent = 1 → 1·200/100 = 2 drops; the others stay "alive" without type.
        assert_eq!(l.rain_current, 1);
        assert_eq!(l.slots.iter().filter(|o| o.ty == LeafType::Rain).count(), 2);
        assert_eq!(live(&l), 80);
        for _ in 0..200 {
            l.tick(&env(WD_10HEAVEN), &mut s);
        }
        assert_eq!(l.rain_current, 100);
        // No splashes in Icarus.
        assert!(s.is_empty());
    }

    #[test]
    fn devil_square_salpica_al_tocar_el_suelo() {
        let mut l = Leaves::default();
        let mut s = vec![];
        for _ in 0..40 {
            l.tick(&env(WD_9DEVILSQUARE), &mut s);
        }
        assert!(l.slots.iter().all(|o| o.ty == LeafType::Rain));
        assert!(!s.is_empty());
        for sp in &s {
            let Spawn::Particle(p) = sp else { panic!("solo partículas") };
            assert!(p.ty == BITMAP_RAIN_CIRCLE || p.ty == BITMAP_RAIN_CIRCLE + 1);
            assert_eq!(p.sub_type, 0);
            assert_eq!(p.position.z, 10.0);
        }
    }

    #[test]
    fn mercado_salpica_con_subtipo_2() {
        let mut l = Leaves::default();
        let mut s = vec![];
        for _ in 0..40 {
            l.tick(&env(WD_79UNITEDMARKETPLACE), &mut s);
        }
        assert!(!s.is_empty());
        assert!(s.iter().all(|sp| matches!(sp, Spawn::Particle(p) if p.sub_type == 2)));
    }

    #[test]
    fn crywolf_niebla_crea_nubes() {
        let mut l = Leaves::default();
        assert_eq!(l.weather, 2);
        let mut s = vec![];
        for _ in 0..200 {
            l.tick(&env(WD_34CRYWOLF_1ST), &mut s);
        }
        assert!(!s.is_empty());
        for sp in &s {
            let Spawn::Particle(p) = sp else { panic!() };
            assert_eq!((p.ty, p.sub_type, p.scale, p.position.z), (BITMAP_CLOUD, 8, 0.4, 400.0));
        }
    }

    #[test]
    fn pavesas_de_vulcanus() {
        let mut l = Leaves::default();
        let mut s = vec![];
        l.tick(&env(WD_63PK_FIELD), &mut s);
        for o in l.slots.iter().take(80) {
            assert_eq!(o.ty, LeafType::FireSnuff);
            assert!((0.4..0.9).contains(&o.scale));
            assert_eq!(o.light, Vec3::new(1.0, 0.0, 0.0));
        }
        assert_eq!(plane_size(WD_63PK_FIELD, 0, &l.slots[0]), Some((l.slots[0].scale * 2.0, l.slots[0].scale * 4.0)));
    }

    #[test]
    fn imperial_guardian_con_sol_no_llueve() {
        let mut l = Leaves::default();
        let mut s = vec![];
        l.tick(&env(69), &mut s);
        assert!(l.slots.iter().all(|o| o.ty == LeafType::None));
        assert!(s.is_empty());
    }

    #[test]
    fn cuando_se_mueve_y_se_ve() {
        assert!(!leaves_move(WD_0LORENCIA, 4));
        assert!(leaves_move(WD_0LORENCIA, 0));
        assert!(!leaves_move(WD_2DEVIAS, 3));
        assert!(!leaves_move(WD_2DEVIAS, 10));
        assert!(leaves_move(WD_2DEVIAS, 2));
        assert!(leaves_move(65, 0) && leaves_move(73, 0) && leaves_move(53, 0));
        assert!(!leaves_move(1, 0) && !leaves_move(8, 0));
        // Lorencia indoors: still but visible; Devias indoors: not visible.
        assert!(leaves_visible(WD_0LORENCIA, 4));
        assert!(!leaves_visible(WD_2DEVIAS, 3));
        assert!(leaves_visible(57, 3));
    }

    #[test]
    fn mezcla_por_mapa() {
        for w in [2, 7, 10, 57, 58, 65, 62, 63, 66, 69, 70, 71, 79, 30, 42, 55] {
            assert_eq!(render_blend(w), Blend::Glow, "mapa {w}");
        }
        for w in [0, 3, 9, 18, 53, 34, 72, 31] {
            assert_eq!(render_blend(w), Blend::AlphaTest, "mapa {w}");
        }
    }

    #[test]
    fn plano_como_render_plane_3d() {
        let c = plane_corners(Vec3::ZERO, Vec3::ZERO, 1.0, 20.0);
        assert_eq!(c[0], Vec3::new(-1.0, -1.0, -20.0));
        assert_eq!(c[2], Vec3::new(1.0, 1.0, 20.0));
        let mut o = Leaf { ty: LeafType::Rain, ..default() };
        assert_eq!(plane_size(WD_34CRYWOLF_1ST, 2, &o), None);
        assert_eq!(plane_size(WD_34CRYWOLF_1ST, 1, &o), Some((1.0, 20.0)));
        o.turning_force = Vec3::new(1.0, 33.0, 0.0);
        assert_eq!(plane_size(20, 0, &o), Some((1.0, 33.0)));
    }
}
