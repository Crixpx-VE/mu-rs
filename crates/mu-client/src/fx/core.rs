//! Effects engine core: the same objects as the original (`PARTICLE`, `JOINT`, the effects'
//! `OBJECT` and single-frame sprites) and the context used to create and move them
//! (`CreateParticle`, `CreateJoint`, `CreateEffect`, `CreateSprite`, `rand()`...).
//!
//! Rules for porting MuMain code to this engine:
//! - Everything is in MU coordinates (Z up, 100 units per tile) and angles in degrees.
//! - Logic runs at 25 ticks per second: `FPS_ANIMATION_FACTOR` = 1 and
//!   `rand_fps_check(n)` = [`Ctx::rand_fps_check`] (1 in n ticks). Formulas are copied verbatim.
//! - `rand()` → [`Ctx::rand`]; `Random::RangeFloat/RangeInt(a, b)` → [`Ctx::range_f`] / [`Ctx::range_i`].
//! - `AngleMatrix(angle, m)` + `VectorRotate(v, m, out)` → `rotate(angle, v)`;
//!   `VectorIRotate` → `irotate(angle, v)`.
//! - `o->Target` / `o->Owner` (a character) → `Target::Actor`; look it up with [`Ctx::actor`].
//!   `b->TransformPosition(BoneTransform[i], p, out)` → [`Actor::bone_world`].
//! - `RequestTerrainHeight(x, y)` → [`Ctx::terrain_height`]; `AddTerrainLight` → [`Ctx::add_terrain_light`].
//! - `WorldTime` (ms) → [`Ctx::world_time`].

use std::sync::Arc;

use bevy::math::{EulerRot, Mat3, Quat, Vec2, Vec3};
use bevy::prelude::Entity;
use mu_data::bmd::{self, Mat34};
use mu_data::terrain::Terrain;

use crate::terrain::TerrainLights;

pub use super::tables::ids::*;
pub use super::material::Blend;

// --- Original math ------------------------------------------------------------------------------

/// `AngleMatrix`: MU rotation with angles in degrees (x, y, z) = Rz·Ry·Rx.
pub fn angle_matrix(angle: Vec3) -> Mat3 {
    let a = angle * (std::f32::consts::PI / 180.0);
    Mat3::from_quat(Quat::from_euler(EulerRot::ZYX, a.z, a.y, a.x))
}

/// `AngleMatrix(angle) + VectorRotate(v)`.
pub fn rotate(angle: Vec3, v: Vec3) -> Vec3 {
    angle_matrix(angle) * v
}

/// `AngleMatrix(angle) + VectorIRotate(v)` (inverse rotation).
pub fn irotate(angle: Vec3, v: Vec3) -> Vec3 {
    angle_matrix(angle).transpose() * v
}

/// `CreateAngle(x1, y1, x2, y2)`: MU heading in degrees (0 = towards -Y).
pub fn create_angle(x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let (dx, dy) = (x2 - x1, y2 - y1);
    if dx.abs() < f32::EPSILON && dy.abs() < f32::EPSILON {
        return 0.0;
    }
    dx.atan2(-dy).to_degrees().rem_euclid(360.0)
}

/// `TurnAngle2(angle, target, step)`: turns towards `target` by at most `step` degrees.
pub fn turn_angle(angle: f32, target: f32, step: f32) -> f32 {
    if step <= 0.0 {
        return angle.rem_euclid(360.0);
    }
    let delta = (target - angle + 540.0).rem_euclid(360.0) - 180.0;
    (angle + delta.clamp(-step, step)).rem_euclid(360.0)
}

// --- Randomness ---------------------------------------------------------------------------------

/// The original's generator: `rand()` returns 0..=32767 like MSVC's.
#[derive(Clone)]
pub struct Rng(u32);

impl Default for Rng {
    fn default() -> Self {
        Self(0x1234_5678)
    }
}

impl Rng {
    pub fn rand(&mut self) -> i32 {
        // MSVC LCG (same range as the original).
        self.0 = self.0.wrapping_mul(214_013).wrapping_add(2_531_011);
        ((self.0 >> 16) & 0x7FFF) as i32
    }
}

// --- Characters and targets -----------------------------------------------------------------------

/// Snapshot of a character (or monster) this tick, for the effects that follow it.
#[derive(Clone)]
pub struct Actor {
    pub position: Vec3,
    /// Angles in degrees (the body heading is `angle.z`).
    pub angle: Vec3,
    pub scale: f32,
    /// Model bones in their current pose (model space).
    pub bones: Arc<Vec<Mat34>>,
    pub action: usize,
    pub frame: f32,
    /// Right-hand weapon: model id (`MODEL_SWORD + n`...) and level (+0..+15).
    pub weapon: Option<(i32, u8)>,
    /// Weapon hand bone (`LinkBone`).
    pub weapon_bone: usize,
    pub live: bool,
    /// `m_vPosSword`: point computed by the current skill (e.g. Death Stab).
    pub pos_sword: Vec3,
}

impl Actor {
    /// `b->TransformPosition(BoneTransform[bone], offset, out)` with the character's position,
    /// rotation and scale: the point `offset` (in bone axes) in world space.
    pub fn bone_world(&self, bone: usize, offset: Vec3) -> Vec3 {
        let local = match self.bones.get(bone) {
            Some(m) => Vec3::from_array(bmd::transform_point(m, offset.to_array())),
            None => offset,
        };
        self.position + rotate(self.angle, local * self.scale)
    }
}

/// What an effect follows (`o->Target` / `o->Owner`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    Actor(Entity),
    /// Another engine effect, by its unique id.
    Effect(u32),
}

// --- Engine objects ---------------------------------------------------------------------------------

/// The original's `PARTICLE`.
#[derive(Clone)]
pub struct Particle {
    pub ty: i32,
    pub tex_type: i32,
    pub sub_type: i32,
    pub scale: f32,
    pub position: Vec3,
    pub angle: Vec3,
    pub light: Vec3,
    pub alpha: f32,
    pub life_time: f32,
    pub target: Option<Target>,
    pub rotation: f32,
    pub frame: i32,
    pub enable_move: bool,
    pub gravity: f32,
    pub velocity: Vec3,
    pub turning_force: Vec3,
    pub start_position: Vec3,
    pub num_bone: i32,
    pub repeatedly: bool,
    pub repeatedly_height: f32,
    /// Previous tick state, for interpolation when drawing.
    pub prev_position: Vec3,
    pub prev_scale: f32,
    pub prev_light: Vec3,
    pub live: bool,
}

impl Particle {
    pub fn new(ty: i32, position: Vec3, angle: Vec3, light: Vec3, sub_type: i32, scale: f32, target: Option<Target>) -> Self {
        Self {
            ty,
            tex_type: ty,
            sub_type,
            scale,
            position,
            angle,
            light,
            alpha: 1.0,
            life_time: 2.0,
            target,
            rotation: 0.0,
            frame: 0,
            enable_move: true,
            gravity: 0.0,
            velocity: Vec3::ZERO,
            turning_force: Vec3::ZERO,
            start_position: position,
            num_bone: 0,
            repeatedly: false,
            repeatedly_height: 0.0,
            prev_position: position,
            prev_scale: scale,
            prev_light: light,
            live: true,
        }
    }
}

/// Trail `RenderType`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum JointRender {
    /// `RENDER_TYPE_ALPHA_BLEND` → additive.
    #[default]
    AlphaBlend,
    /// `RENDER_TYPE_ALPHA_TEST`.
    AlphaTest,
    /// `RENDER_TYPE_ALPHA_BLEND_MINUS` → subtractive.
    AlphaBlendMinus,
    /// `RENDER_TYPE_ALPHA_BLEND_OTHER` → `Blend2`.
    AlphaBlendOther,
}

/// How a trail's texture scrolls when drawn (special cases of `DescribeJoint`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum JointScroll {
    #[default]
    None,
    /// `BITMAP_JOINT_THUNDER`: U·2 − scroll on one face and + scroll·2 on the other.
    Thunder,
    /// `bTileMapping`: U = (NumTails − j) / 16, ×2, − 2·scroll.
    Tile,
    /// `BITMAP_FLARE_FORCE` (subtypes 0-4, 11-13): U − scroll.
    Flare,
}

pub const RENDER_FACE_ONE: u8 = 1;
pub const RENDER_FACE_TWO: u8 = 2;
pub const MAX_TAILS: usize = 200;

/// The original's `JOINT`: a trail made of "tails" (4-point crosses) left behind as it moves.
#[derive(Clone)]
pub struct Joint {
    pub ty: i32,
    pub tex_type: i32,
    pub sub_type: i32,
    pub render_type: JointRender,
    pub render_face: u8,
    pub scale: f32,
    pub position: Vec3,
    pub start_position: Vec3,
    pub angle: Vec3,
    pub head_angle: Vec3,
    pub light: Vec3,
    pub target: Option<Target>,
    pub target_position: Vec3,
    pub only_one_render: u8,
    pub life_time: f32,
    pub collision: bool,
    pub velocity: f32,
    pub direction: Vec3,
    pub pk_key: i32,
    pub skill: i32,
    pub weapon: f32,
    pub multi_use: f32,
    pub tile_mapping: bool,
    pub reverse_uv: u8,
    pub create_tails: bool,
    pub max_tails: usize,
    /// Tails: [0] is the newest. Each is 4 points (a cross around the position).
    pub tails: Vec<[Vec3; 4]>,
    pub scroll: JointScroll,
    /// Free slots for each type's code (what the original kept in assorted fields).
    pub user: [f32; 4],
    pub live: bool,
}

impl Joint {
    pub fn new(ty: i32, position: Vec3, target_position: Vec3, angle: Vec3, sub_type: i32, target: Option<Target>, scale: f32) -> Self {
        Self {
            ty,
            tex_type: ty,
            sub_type,
            render_type: JointRender::AlphaBlend,
            render_face: RENDER_FACE_ONE | RENDER_FACE_TWO,
            scale,
            position,
            start_position: position,
            angle,
            head_angle: Vec3::ZERO,
            light: Vec3::ONE,
            target,
            target_position,
            only_one_render: 0,
            life_time: 0.0,
            collision: false,
            velocity: 0.0,
            direction: Vec3::ZERO,
            // CreateJoint defaults PKKey to -1.
            pk_key: -1,
            skill: 0,
            weapon: 0.0,
            multi_use: 0.0,
            tile_mapping: false,
            reverse_uv: 0,
            create_tails: true,
            max_tails: 1,
            tails: Vec::new(),
            scroll: JointScroll::None,
            user: [0.0; 4],
            live: true,
        }
    }

    /// `CreateTail`/`SetNewestTail`: a 4-point cross of width `scale` rotated by `angle`.
    pub fn make_tail(&self) -> [Vec3; 4] {
        let m = angle_matrix(self.angle);
        let h = self.scale * 0.5;
        [
            self.position + m * Vec3::new(-h, 0.0, 0.0),
            self.position + m * Vec3::new(h, 0.0, 0.0),
            self.position + m * Vec3::new(0.0, 0.0, -h),
            self.position + m * Vec3::new(0.0, 0.0, h),
        ]
    }

    /// `FollowTail`: tails shift by one and a new one is added at the front.
    pub fn follow_tail(&mut self) {
        let tail = self.make_tail();
        self.tails.insert(0, tail);
        let max = self.max_tails.max(1).min(MAX_TAILS);
        self.tails.truncate(max);
    }
}

/// The effects' `OBJECT` (`Effects[]`): usually a moving `.bmd` model.
#[derive(Clone)]
pub struct Effect {
    /// Unique id (so other effects can follow it with `Target::Effect`).
    pub id: u32,
    pub ty: i32,
    pub sub_type: i32,
    pub position: Vec3,
    pub start_position: Vec3,
    pub angle: Vec3,
    pub head_angle: Vec3,
    pub direction: Vec3,
    pub light: Vec3,
    pub alpha: f32,
    pub alpha_target: f32,
    pub scale: f32,
    pub life_time: f32,
    /// Model animation speed (frames per tick).
    pub velocity: f32,
    pub gravity: f32,
    pub distance: f32,
    pub timer: f32,
    pub animation_frame: f32,
    pub current_action: usize,
    pub blend_mesh: i32,
    pub blend_mesh_light: f32,
    pub blend_mesh_tex_coord: Vec2,
    pub hidden_mesh: i32,
    pub light_enable: bool,
    pub owner: Option<Target>,
    pub target: Option<Target>,
    pub pk_key: i32,
    /// The original's `Kind` (the `Skill` argument of `CreateEffect`).
    pub kind: i32,
    /// The original's `Skill` (the `SkillIndex` argument).
    pub skill: i32,
    pub weapon: i32,
    pub weapon_level: i32,
    pub ai: i32,
    pub live: bool,
    /// Drawn with its type's model (if `false`, it only emits particles or is drawn by hand).
    pub visible: bool,
    /// Previous tick state, for interpolation.
    pub prev_position: Vec3,
    pub prev_angle: Vec3,
    /// Free fields for whatever each type needs to store.
    pub user: [f32; 8],
}

impl Effect {
    pub fn new(id: u32, ty: i32, position: Vec3, angle: Vec3, light: Vec3, sub_type: i32, owner: Option<Target>, pk_key: i32, kind: i32, skill: i32, scale: f32) -> Self {
        Self {
            id,
            ty,
            sub_type,
            position,
            start_position: position,
            angle,
            head_angle: Vec3::ZERO,
            direction: Vec3::ZERO,
            light,
            alpha: 1.0,
            alpha_target: 1.0,
            scale: if scale <= 0.0 { 0.9 } else { scale },
            life_time: 0.0,
            velocity: 0.3,
            gravity: 0.0,
            distance: 0.0,
            timer: 0.0,
            animation_frame: 0.0,
            current_action: 0,
            blend_mesh: -1,
            blend_mesh_light: 1.0,
            blend_mesh_tex_coord: Vec2::ZERO,
            hidden_mesh: -1,
            light_enable: true,
            owner,
            target: None,
            pk_key,
            kind,
            skill,
            weapon: 0,
            weapon_level: 0,
            ai: 0,
            live: true,
            visible: true,
            prev_position: position,
            prev_angle: angle,
            user: [0.0; 8],
        }
    }
}

/// `CreateSprite`: a camera-facing quad that lasts one tick.
#[derive(Clone, Copy)]
pub struct SpriteFx {
    pub ty: i32,
    pub position: Vec3,
    pub scale: f32,
    pub light: Vec3,
    pub rotation: f32,
    /// 0 additive, 1 subtractive, 2 alpha test, 3 `Blend2` (`DescribeSprite`).
    pub sub_type: i32,
    pub alpha: f32,
}

// --- Map objects with effects (`MoveObject` / `UpdateObjectVisual`) ------------------------------

/// A map object that emits effects (the world objects' `OBJECT`). It is drawn by `objects.rs`;
/// here is only what its effects code needs.
#[derive(Clone)]
pub struct MapObject {
    /// `o->Type`: the object type on the map (index into `EncTerrainN.obj`).
    pub ty: i32,
    pub position: Vec3,
    /// Degrees (x, y, z), like `o->Angle`.
    pub angle: Vec3,
    pub scale: f32,
    /// `o->Light`: the terrain light where it stands.
    pub light: Vec3,
    /// Model (for effects attached to its bones) and its animation speed (frames/tick).
    pub model: Option<Arc<mu_data::bmd::Bmd>>,
    pub velocity: f32,
    /// `o->AnimationFrame` this tick.
    pub animation_frame: f32,
    /// Bones in this tick's pose (filled before calling the object's code).
    pub bones: Option<Arc<Vec<Mat34>>>,
    /// `o->Timer`, `o->HiddenMesh`... and free fields for each type's code.
    pub timer: f32,
    pub hidden_mesh: i32,
    pub user: [f32; 4],
    /// `CreateObject` work already done (effects the original creates only once at load).
    pub created: bool,
}

impl MapObject {
    /// `b->TransformPosition(BoneTransform[bone], offset, out)` + the object's position, rotation and scale.
    pub fn bone_world(&self, bone: usize, offset: Vec3) -> Vec3 {
        let local = match self.bones.as_ref().and_then(|b| b.get(bone)) {
            Some(m) => Vec3::from_array(bmd::transform_point(m, offset.to_array())),
            None => offset,
        };
        self.position + rotate(self.angle, local * self.scale)
    }
}

/// Effects code for an object type on a map: called once per tick.
pub type ObjectStep = fn(&mut MapObject, &mut Ctx);

// --- Weapon trail (`Blur`, ZzzEffectBlurSpark.cpp) ------------------------------------------------

/// `MAX_BLUR_TAILS`: maximum points of a weapon trail.
pub const MAX_BLUR_TAILS: usize = 30;
/// `MAX_BLUR_LIFETIME`: lifetime of a normal trail ("short" ones live 15).
pub const MAX_BLUR_LIFETIME: i32 = 30;

/// Weapon trail: two point lists (hilt `p1` and tip `p2`), [0] the newest.
#[derive(Clone)]
pub struct Blur {
    pub owner: Entity,
    pub p1: Vec<Vec3>,
    pub p2: Vec<Vec3>,
    pub light: Vec3,
    /// `BlurMapping`: texture `BITMAP_BLUR + n` (3 → `BLUR2`, 4 → `BLUR`, 5 → `BLUR + 3`).
    pub mapping: i32,
    pub life: i32,
    /// Points added in the last frame (the next frame replaces them unless a tick has passed).
    pub frame_samples: usize,
    pub frame: u64,
    pub live: bool,
}

impl Blur {
    /// `AddBlurSample`: each frame replaces the previous frame's points until a tick commits them.
    pub fn add_sample(&mut self, p1: Vec3, p2: Vec3, frame: u64) {
        if self.frame != frame {
            let replaced = self.frame_samples.min(self.p1.len());
            self.p1.drain(0..replaced);
            self.p2.drain(0..replaced);
            self.frame = frame;
            self.frame_samples = 0;
        }
        self.p1.insert(0, p1);
        self.p2.insert(0, p2);
        self.p1.truncate(MAX_BLUR_TAILS - 1);
        self.p2.truncate(MAX_BLUR_TAILS - 1);
        self.frame_samples = (self.frame_samples + 1).min(self.p1.len());
    }

    /// `MoveBlurs`: one tick: the oldest point is dropped and life decreases.
    pub fn age(&mut self) {
        self.frame_samples = 0;
        self.life -= 1;
        let shrink = |v: &mut Vec<Vec3>| {
            // Number -= 1 and shift: [0] is duplicated and the two oldest fall off.
            if !v.is_empty() {
                v.pop();
                if let Some(&first) = v.first() {
                    v.insert(0, first);
                    v.pop();
                }
            }
        };
        shrink(&mut self.p1);
        shrink(&mut self.p2);
        if self.life <= 0 {
            self.p1.pop();
            self.p2.pop();
            if self.p1.is_empty() {
                self.live = false;
            }
        }
    }

    /// Texture by `BlurMapping` (`RenderBlurs`).
    pub fn texture(&self) -> i32 {
        match self.mapping {
            3 => BITMAP_BLUR2,
            4 => BITMAP_BLUR,
            5 => BITMAP_BLUR + 3,
            m => BITMAP_BLUR + m,
        }
    }
}

// --- Skills ----------------------------------------------------------------------------------------

/// `g_iLimitAttackTime`: when `AttackTime` reaches this, the skill's final effect fires.
pub const LIMIT_ATTACK_TIME: i32 = 15;

/// A character's skill in progress: what the original stores in `CHARACTER`
/// (`Skill`, `AttackTime`, `TargetCharacter`, `m_vPosSword`...). Each tick: `AttackTime += 1`,
/// `stage` is called (`AttackStage`/`AttackEffect`), and on reaching [`LIMIT_ATTACK_TIME`]
/// `finish` (the `switch` in `ZzzCharacter.cpp` ≈ 4154-4437); then it ends.
#[derive(Clone)]
pub struct Cast {
    pub owner: Entity,
    pub skill: i32,
    pub attack_time: i32,
    /// Target (`TargetCharacter`) and its position (or the chosen ground point).
    pub target: Option<Target>,
    pub target_position: Vec3,
    /// `m_vPosSword` and other data the original stores in the character.
    pub pos_sword: Vec3,
    pub user: [f32; 8],
    pub live: bool,
}

impl Cast {
    pub fn new(owner: Entity, skill: i32, target: Option<Target>, target_position: Vec3) -> Self {
        Self { owner, skill, attack_time: 1, target, target_position, pos_sword: Vec3::ZERO, user: [0.0; 8], live: true }
    }
}

pub type SkillStage = fn(&mut Cast, &mut Ctx);

#[derive(Clone, Copy, Default)]
pub struct SkillKind {
    /// Every tick while it lasts (`AttackStage` + `AttackEffect`).
    pub stage: Option<SkillStage>,
    /// When `AttackTime` reaches [`LIMIT_ATTACK_TIME`].
    pub finish: Option<SkillStage>,
}

// --- Spawn requests -----------------------------------------------------------------------------

/// What is requested during a tick; created when the tick ends (with its create function).
pub enum Spawn {
    Particle(Particle),
    Joint(Joint),
    Effect(Effect),
}

// --- Context ---------------------------------------------------------------------------------------

/// What each effect type's functions see while being created or moved.
pub struct Ctx<'a> {
    pub rng: &'a mut Rng,
    /// `WorldTime`: milliseconds since the game started.
    pub world_time: f32,
    pub terrain: Option<&'a Terrain>,
    pub hero: Option<Entity>,
    pub actors: &'a std::collections::HashMap<Entity, Actor>,
    /// Positions of live effects at tick start (for `Target::Effect`).
    pub effect_positions: &'a std::collections::HashMap<u32, Vec3>,
    pub spawns: &'a mut Vec<Spawn>,
    pub sprites: &'a mut Vec<SpriteFx>,
    pub lights: &'a mut TerrainLights,
    /// `EarthQuake`: camera shake (added to the pitch).
    pub earthquake: &'a mut f32,
    pub next_effect_id: &'a mut u32,
    /// `gMapManager.WorldActive`: current map in the original's numbering (Lorencia = 0).
    pub world_active: i32,
    /// `PlayBuffer`: requested sounds (`SOUND_*`), played by `sound.rs`.
    pub sounds: &'a mut Vec<&'static str>,
}

impl Ctx<'_> {
    pub fn rand(&mut self) -> i32 {
        self.rng.rand()
    }

    /// `PlayBuffer(id)`.
    pub fn play_sound(&mut self, id: &'static str) {
        self.sounds.push(id);
    }

    /// `rand_fps_check(n)`: true 1 in `n` ticks (always if n ≤ 1).
    pub fn rand_fps_check(&mut self, n: i32) -> bool {
        n <= 1 || self.rand() % n == 0
    }

    /// `Random::RangeFloat(a, b)` (both inclusive).
    pub fn range_f(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * (self.rand() as f32 / 32767.0)
    }

    /// `Random::RangeInt(a, b)` (both inclusive).
    pub fn range_i(&mut self, a: i32, b: i32) -> i32 {
        if b <= a {
            return a;
        }
        a + self.rand() % (b - a + 1)
    }

    pub fn world_time(&self) -> f32 {
        self.world_time
    }

    /// `RequestTerrainHeight(x, y)` in MU units.
    pub fn terrain_height(&self, x: f32, y: f32) -> f32 {
        self.terrain.map_or(0.0, |t| t.height_at(x / 100.0, y / 100.0))
    }

    /// `TerrainWall[...]` (tile attributes) at an MU position.
    pub fn terrain_attribute(&self, x: f32, y: f32) -> u16 {
        let (tx, ty) = ((x / 100.0).max(0.0) as usize, (y / 100.0).max(0.0) as usize);
        self.terrain.map_or(0, |t| if tx < 256 && ty < 256 { t.attribute(tx, ty) } else { 0 })
    }

    pub fn actor(&self, t: Option<Target>) -> Option<&Actor> {
        match t? {
            Target::Actor(e) => self.actors.get(&e),
            Target::Effect(_) => None,
        }
    }

    pub fn hero_actor(&self) -> Option<&Actor> {
        self.actors.get(&self.hero?)
    }

    /// Position of the followed target (character or effect).
    pub fn target_position(&self, t: Option<Target>) -> Option<Vec3> {
        match t? {
            Target::Actor(e) => self.actors.get(&e).map(|a| a.position),
            Target::Effect(id) => self.effect_positions.get(&id).copied(),
        }
    }

    /// `AddTerrainLight(x, y, light, range, PrimaryTerrainLight)`: x, y in MU, radius in tiles.
    pub fn add_terrain_light(&mut self, x: f32, y: f32, light: Vec3, range: f32) {
        self.lights.add(Vec2::new(x / 100.0, y / 100.0), light.to_array(), range);
    }

    /// `SetEarthQuake`: the strongest shake of the tick wins.
    pub fn set_earthquake(&mut self, amount: f32) {
        if amount.abs() > self.earthquake.abs() {
            *self.earthquake = amount;
        }
    }

    /// `CreateParticle(Type, Position, Angle, Light, SubType, Scale, Owner)`.
    pub fn create_particle(&mut self, ty: i32, position: Vec3, angle: Vec3, light: Vec3, sub_type: i32, scale: f32, owner: Option<Target>) {
        self.spawns.push(Spawn::Particle(Particle::new(ty, position, angle, light, sub_type, scale, owner)));
    }

    /// `CreateJoint(Type, Position, TargetPosition, Angle, SubType, Target, Scale)`.
    #[allow(clippy::too_many_arguments)]
    pub fn create_joint(&mut self, ty: i32, position: Vec3, target_position: Vec3, angle: Vec3, sub_type: i32, target: Option<Target>, scale: f32) {
        self.spawns.push(Spawn::Joint(Joint::new(ty, position, target_position, angle, sub_type, target, scale)));
    }

    /// `CreateJoint` with `PKKey` and `SkillIndex` (which the original sometimes passes).
    #[allow(clippy::too_many_arguments)]
    pub fn create_joint_ex(&mut self, ty: i32, position: Vec3, target_position: Vec3, angle: Vec3, sub_type: i32, target: Option<Target>, scale: f32, pk_key: i32, skill: i32) {
        let mut j = Joint::new(ty, position, target_position, angle, sub_type, target, scale);
        j.pk_key = pk_key;
        j.skill = skill;
        self.spawns.push(Spawn::Joint(j));
    }

    /// `CreateEffect(Type, Position, Angle, Light, SubType, Owner, PKKey, Skill, SkillIndex, ..., Scale)`.
    /// Returns the created effect's id.
    #[allow(clippy::too_many_arguments)]
    pub fn create_effect(&mut self, ty: i32, position: Vec3, angle: Vec3, light: Vec3, sub_type: i32, owner: Option<Target>, pk_key: i32, kind: i32, skill: i32, scale: f32) -> u32 {
        *self.next_effect_id += 1;
        let id = *self.next_effect_id;
        self.spawns.push(Spawn::Effect(Effect::new(id, ty, position, angle, light, sub_type, owner, pk_key, kind, skill, scale)));
        id
    }

    /// `CreateSprite(Type, Position, Scale, Light, Owner, Rotation, SubType)`: lasts this tick.
    pub fn create_sprite(&mut self, ty: i32, position: Vec3, scale: f32, light: Vec3, rotation: f32, sub_type: i32) {
        self.sprites.push(SpriteFx { ty, position, scale, light, rotation, sub_type, alpha: 1.0 });
    }
}

// --- Drawing ---------------------------------------------------------------------------------------

/// A sprite to draw this frame (`RenderSprite`): centered at `position`, `width`×`height`
/// MU units, rotated `rotation` degrees in the screen plane.
#[derive(Clone, Copy)]
pub struct SpriteDraw {
    pub texture: i32,
    pub position: Vec3,
    pub width: f32,
    pub height: f32,
    /// Color (rgb) and alpha (a), like the original's vertex color.
    pub color: [f32; 4],
    pub rotation: f32,
    /// (u, v, width, height) in the texture; v = 0 at the top.
    pub uv: [f32; 4],
    pub blend: Blend,
}

/// Ground mark (`RenderTerrainAlphaBitmap`): a quad draped on the terrain.
#[derive(Clone, Copy)]
pub struct DecalDraw {
    pub texture: i32,
    /// Center in MU (x, y).
    pub center: Vec2,
    /// Size in MU units.
    pub width: f32,
    pub height: f32,
    pub color: [f32; 4],
    pub rotation: f32,
    /// Height above the ground (MU units).
    pub lift: f32,
    pub blend: Blend,
}

/// A `.bmd` model to draw this frame (an effect's `RenderObject`).
#[derive(Clone, Copy)]
pub struct ModelDraw {
    /// `MODEL_*` (effects, items...).
    pub model: i32,
    pub position: Vec3,
    pub angle: Vec3,
    pub scale: f32,
    /// `BodyLight`: color of the whole model.
    pub light: Vec3,
    pub alpha: f32,
    pub action: usize,
    pub frame: f32,
    /// Mesh (by texture index) drawn additively with `BlendMeshLight` and a UV offset.
    pub blend_mesh: i32,
    pub blend_mesh_light: f32,
    pub blend_mesh_tex_coord: Vec2,
    /// `HiddenMesh`: -1 none, -2 all, n = that one.
    pub hidden_mesh: i32,
    /// `RENDER_BRIGHT`: whole model additive.
    pub bright: bool,
}

impl ModelDraw {
    pub fn new(model: i32, position: Vec3, angle: Vec3, scale: f32) -> Self {
        Self {
            model,
            position,
            angle,
            scale,
            light: Vec3::ONE,
            alpha: 1.0,
            action: 0,
            frame: 0.0,
            blend_mesh: -1,
            blend_mesh_light: 1.0,
            blend_mesh_tex_coord: Vec2::ZERO,
            hidden_mesh: -1,
            bright: false,
        }
    }

    /// An effect's model with its state (what `RenderObject` does).
    pub fn of_effect(e: &Effect) -> Self {
        Self {
            model: e.ty,
            position: e.position,
            angle: e.angle,
            scale: e.scale,
            light: e.light,
            alpha: e.alpha,
            action: e.current_action,
            frame: e.animation_frame,
            blend_mesh: e.blend_mesh,
            blend_mesh_light: e.blend_mesh_light,
            blend_mesh_tex_coord: e.blend_mesh_tex_coord,
            hidden_mesh: e.hidden_mesh,
            bright: false,
        }
    }
}

/// A free quad in the world (`RenderPlane3D`, hand-built ribbons...): 4 corners in MU.
#[derive(Clone, Copy)]
pub struct QuadDraw {
    pub texture: i32,
    pub corners: [Vec3; 4],
    /// (u, v) of each corner.
    pub uv: [[f32; 2]; 4],
    pub color: [f32; 4],
    pub blend: Blend,
}

/// What is drawn this frame; filled by each type's `draw` function.
#[derive(Default)]
pub struct DrawList {
    pub sprites: Vec<SpriteDraw>,
    pub decals: Vec<DecalDraw>,
    pub quads: Vec<QuadDraw>,
    pub models: Vec<ModelDraw>,
}

/// Size in pixels and channel count (3 = JPG, 4 = TGA) of a `BITMAP_*` texture.
#[derive(Clone, Copy, Debug)]
pub struct TexInfo {
    pub width: f32,
    pub height: f32,
    pub components: u8,
}

/// What a draw function sees: texture lookup and time.
pub struct DrawCtx<'a> {
    pub world_time: f32,
    pub tex_info: &'a dyn Fn(i32) -> Option<TexInfo>,
    pub actors: &'a std::collections::HashMap<Entity, Actor>,
}

impl DrawCtx<'_> {
    pub fn tex(&self, id: i32) -> Option<TexInfo> {
        (self.tex_info)(id)
    }
}

// --- Type registry ---------------------------------------------------------------------------------

pub type ParticleCreate = fn(&mut Particle, &mut Ctx);
pub type ParticleStep = fn(&mut Particle, &mut Ctx);
pub type ParticleDraw = fn(&Particle, &DrawCtx, &mut DrawList);
pub type JointCreate = fn(&mut Joint, &mut Ctx);
pub type JointStep = fn(&mut Joint, &mut Ctx);
pub type EffectCreate = fn(&mut Effect, &mut Ctx);
pub type EffectStep = fn(&mut Effect, &mut Ctx);
pub type EffectDraw = fn(&Effect, &DrawCtx, &mut DrawList);

/// Functions of a particle type (what the original has in the `switch`es of `CreateParticle`,
/// `MoveParticles` and `DescribeParticle`). `None` = default behavior.
#[derive(Clone, Copy, Default)]
pub struct ParticleKind {
    pub create: Option<ParticleCreate>,
    pub step: Option<ParticleStep>,
    pub draw: Option<ParticleDraw>,
}

#[derive(Clone, Copy, Default)]
pub struct JointKind {
    pub create: Option<JointCreate>,
    pub step: Option<JointStep>,
}

#[derive(Clone, Copy, Default)]
pub struct EffectKind {
    pub create: Option<EffectCreate>,
    pub step: Option<EffectStep>,
    /// If `None`, the type's model (if any) is drawn with [`ModelDraw::of_effect`].
    pub draw: Option<EffectDraw>,
}

/// All registered types (each `fx/kinds/` module adds its own).
#[derive(Default, bevy::prelude::Resource)]
pub struct Registry {
    pub particles: std::collections::HashMap<i32, ParticleKind>,
    pub joints: std::collections::HashMap<i32, JointKind>,
    pub effects: std::collections::HashMap<i32, EffectKind>,
    /// Skills by `AT_SKILL_*` number.
    pub skills: std::collections::HashMap<i32, SkillKind>,
    /// Map objects by (`WorldActive`, object type).
    pub objects: std::collections::HashMap<(i32, i32), ObjectStep>,
}

impl Registry {
    /// Registers the effects code for objects `ty` on map `world_active` (Lorencia = 0).
    pub fn object(&mut self, world_active: i32, ty: i32, step: ObjectStep) {
        self.objects.insert((world_active, ty), step);
    }
    pub fn skill(&mut self, skill: i32, kind: SkillKind) {
        self.skills.insert(skill, kind);
    }
    pub fn particle(&mut self, ty: i32, kind: ParticleKind) {
        self.particles.insert(ty, kind);
    }
    pub fn joint(&mut self, ty: i32, kind: JointKind) {
        self.joints.insert(ty, kind);
    }
    pub fn effect(&mut self, ty: i32, kind: EffectKind) {
        self.effects.insert(ty, kind);
    }
}

/// Default particle drawing (`RenderParticles`): a sprite of its texture size times the scale,
/// blended by texture type (JPG = additive, TGA = alpha test).
pub fn default_particle_draw(p: &Particle, ctx: &DrawCtx, out: &mut DrawList) {
    let Some(t) = ctx.tex(p.tex_type) else { return };
    out.sprites.push(SpriteDraw {
        texture: p.tex_type,
        position: p.position,
        width: t.width * p.scale,
        height: t.height * p.scale,
        color: [p.light.x, p.light.y, p.light.z, if t.components == 3 { 1.0 } else { p.light.x }],
        rotation: p.rotation,
        uv: [0.0, 0.0, 1.0, 1.0],
        blend: default_blend(t),
    });
}

/// Default blend for particles and marks: JPG (3 channels) additive; TGA alpha test.
pub fn default_blend(t: TexInfo) -> Blend {
    if t.components == 3 { Blend::Glow } else { Blend::AlphaTest }
}
