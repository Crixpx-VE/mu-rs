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

/// `o->Kind` of a character (`_define.h:142-149`).
pub const KIND_PLAYER: i32 = 1;
pub const KIND_MONSTER: i32 = 2;
pub const KIND_NPC: i32 = 4;
pub const KIND_TRAP: i32 = 8;

/// Snapshot of a character (or monster) this tick, for the effects that follow it and for the
/// client-side range tests (`CharactersClient[]` as `CheckCharacterRange` sees it).
#[derive(Clone)]
pub struct Actor {
    /// `o->Type`: `MODEL_PLAYER` for characters, `MODEL_MONSTER01 + n` for monsters and NPCs.
    pub ty: i32,
    /// `o->Kind`: [`KIND_PLAYER`], [`KIND_MONSTER`], [`KIND_NPC`]...
    pub kind: i32,
    /// `c->Key`: the server's object index (-1 if unknown, e.g. the hero before joining).
    pub key: i32,
    pub position: Vec3,
    /// Angles in degrees (the body heading is `angle.z`).
    pub angle: Vec3,
    pub scale: f32,
    /// Model bones in their current pose (model space).
    pub bones: Arc<Vec<Mat34>>,
    /// `Bones[i].Dummy` of the model (bones the original skips when spreading effects).
    pub dummy_bones: Arc<Vec<bool>>,
    pub action: usize,
    pub frame: f32,
    /// Right-hand weapon: model id (`MODEL_SWORD + n`...) and level (+0..+15).
    pub weapon: Option<(i32, u8)>,
    /// Weapon hand bone (`LinkBone`).
    pub weapon_bone: usize,
    /// `o->Live`.
    pub live: bool,
    /// `o->Visible` (in the view frustum; the snapshot treats every character in scope as visible).
    pub visible: bool,
    /// `c->Dead`.
    pub dead: bool,
    /// `m_vPosSword`: point computed by the current skill (e.g. Death Stab).
    pub pos_sword: Vec3,
    /// `o->Light` (the character's body light).
    pub light: Vec3,
    /// `o->Alpha`.
    pub alpha: f32,
    /// `o->Velocity` (animation speed this tick).
    pub velocity: f32,
    /// `o->Direction`.
    pub direction: Vec3,
    /// `o->StartPosition`.
    pub start_position: Vec3,
    /// `c->m_sTargetIndex` resolved to the targeted character, if the client knows it.
    pub target: Option<Entity>,
    /// Active buffs (`eBuffState` values, `g_isCharacterBuff`); empty until the client tracks them.
    pub buffs: Arc<Vec<i32>>,
    /// `c->m_bySkillCount`.
    pub skill_count: u8,
    /// `c->SafeZone`.
    pub safe_zone: bool,
    /// `o->BoundingBoxMax`: `PlayerBoxMax` (40, 40, 120) for players, the model's box for
    /// monsters (ZzzCharacter.cpp:12728-12733).
    pub bbox_max: Vec3,
}

impl Default for Actor {
    fn default() -> Self {
        Self {
            ty: MODEL_PLAYER,
            kind: KIND_PLAYER,
            key: -1,
            position: Vec3::ZERO,
            angle: Vec3::ZERO,
            scale: 1.0,
            bones: Arc::new(Vec::new()),
            dummy_bones: Arc::new(Vec::new()),
            action: 0,
            frame: 0.0,
            weapon: None,
            weapon_bone: 0,
            live: true,
            visible: true,
            dead: false,
            pos_sword: Vec3::ZERO,
            light: Vec3::ONE,
            alpha: 1.0,
            velocity: 0.0,
            direction: Vec3::ZERO,
            start_position: Vec3::ZERO,
            target: None,
            buffs: Arc::new(Vec::new()),
            skill_count: 0,
            safe_zone: false,
            bbox_max: Vec3::new(40.0, 40.0, 120.0),
        }
    }
}

impl Actor {
    /// `g_isCharacterBuff(o, buff)`.
    pub fn has_buff(&self, buff: i32) -> bool {
        self.buffs.contains(&buff)
    }

    /// `Bones[i].Dummy`.
    pub fn is_dummy_bone(&self, bone: usize) -> bool {
        self.dummy_bones.get(bone).copied().unwrap_or(false)
    }

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
    /// `TargetIndex[5]`: characters a trail chains to (`BITMAP_JOINT_THUNDER` 15).
    pub targets: Vec<Entity>,
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
            targets: Vec::new(),
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
    /// `o->EyeLeft` (a point some effects keep for the trails that follow them).
    pub eye_left: Vec3,
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
            eye_left: Vec3::ZERO,
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

/// What the kinds can see of a live effect (the `Effects[]` lookups: `SearchEffect`...),
/// as it was at the start of the tick.
#[derive(Clone, Copy, Debug)]
pub struct EffectInfo {
    pub id: u32,
    pub ty: i32,
    pub sub_type: i32,
    pub owner: Option<Target>,
    pub position: Vec3,
    pub angle: Vec3,
    pub life_time: f32,
    pub pk_key: i32,
    pub kind: i32,
    pub skill: i32,
    pub light: Vec3,
    pub alpha: f32,
    pub start_position: Vec3,
    pub head_angle: Vec3,
    pub eye_left: Vec3,
}

impl EffectInfo {
    pub fn of(e: &Effect) -> Self {
        Self {
            id: e.id,
            ty: e.ty,
            sub_type: e.sub_type,
            owner: e.owner,
            position: e.position,
            angle: e.angle,
            life_time: e.life_time,
            pk_key: e.pk_key,
            kind: e.kind,
            skill: e.skill,
            light: e.light,
            alpha: e.alpha,
            start_position: e.start_position,
            head_angle: e.head_angle,
            eye_left: e.eye_left,
        }
    }
}

/// A change the effects make to a character. The original writes `CharactersClient[]` directly;
/// here actors are read-only snapshots, so the change is queued in [`Ctx::actor_requests`] and
/// applied by the character systems (`fx::apply_actor_requests`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ActorChange {
    /// `c->StormTime = 10` (`SpinMonstersInTornado`, MuMain ZzzEffect.cpp:106-116): the monster
    /// spins for 10 ticks (`PushingCharacter`). Monsters that must not turn are skipped by
    /// monsters.rs (`CanSpinInTornado`).
    Spin,
    /// `SetPlayerStop(c)`: back to the standing action.
    Stop,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActorRequest {
    pub actor: Entity,
    pub change: ActorChange,
}

/// A sound request of the effects (`PlayBuffer` / `StopBuffer`), for `sound.rs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FxSound {
    /// `PlayBuffer(id)`.
    Play(&'static str),
    /// `PlayBuffer(id, o)`.
    Object(&'static str),
    /// `PlayBuffer(id, NULL, true)`.
    Loop(&'static str),
    /// `StopBuffer(id, true)`.
    Stop(&'static str),
}

impl FxSound {
    pub fn id(&self) -> &'static str {
        match *self {
            Self::Play(id) | Self::Object(id) | Self::Loop(id) | Self::Stop(id) => id,
        }
    }
}

/// Deletions requested during a tick (`DeleteEffect`, `DeleteJoint`, `DeleteParticle`). The
/// original deletes at once; here they are applied when the tick ends.
#[derive(Clone, Copy, Debug)]
pub enum Delete {
    /// `DeleteEffect(Type, Owner, SubType)`: every live effect of that type and owner
    /// (`sub_type` -1 = any subtype).
    Effect { ty: i32, owner: Option<Target>, sub_type: i32 },
    /// One effect by id (`EffectDestructor` of another effect).
    EffectId(u32),
    /// `DeleteJoint(Type, Target, SubType)` (`sub_type` -1 = any subtype, `target` `None` = any).
    Joint { ty: i32, target: Option<Target>, sub_type: i32 },
    /// `DeleteParticle(Type)`: the first live particle of that type.
    Particle(i32),
}

// --- Context ---------------------------------------------------------------------------------------

/// How deep creation code may nest (a kind that creates itself forever stops here).
pub const MAX_CREATE_DEPTH: u32 = 16;

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
    /// `PlayBuffer` / `StopBuffer`: requested sounds (`SOUND_*`), played by `sound.rs`.
    pub sounds: &'a mut Vec<FxSound>,
    /// Live effects at tick start (for `SearchEffect` and friends).
    pub effects: &'a [EffectInfo],
    /// Deletions requested this tick (applied when it ends).
    pub deletes: &'a mut Vec<Delete>,
    /// The kinds, so `CreateParticle`/`CreateJoint`/`CreateEffect` run their creation code at once,
    /// in the middle of the caller's code, like the original (same `rand()` order).
    pub registry: &'a Registry,
    /// Effect model files (`Models[o->Type]`: own bones, vertices, action lengths).
    pub models: &'a mut super::model_data::ModelData,
    /// `CharacterAttribute->Skill[]`: the hero's skill list by slot (empty until the client gets it).
    pub hero_skills: &'a [i32],
    /// `CharacterMachine->PacketSerial` (ZzzInfomation.h:68): a `BYTE` the hero's skills and some
    /// projectiles step and copy into `o->Weapon`, which becomes `AttackCharacterRange`'s `Serial`.
    pub packet_serial: &'a mut u8,
    /// `Hero->CurrentSkill`: the hero's selected skill slot (what `Fire Scream` hits with).
    pub hero_current_skill: i32,
    /// Changes to characters requested this tick (see [`ActorChange`]).
    pub actor_requests: &'a mut Vec<ActorRequest>,
    /// Free slots left this tick in `Particles[]`, `Joints[]` and `Effects[]` (the original creates
    /// nothing, and runs no creation code, when the pool is full).
    pub free: [usize; 3],
    /// Nesting of creation code (a guard against kinds that create themselves forever).
    pub depth: u32,
}

impl Ctx<'_> {
    pub fn rand(&mut self) -> i32 {
        self.rng.rand()
    }

    /// `PlayBuffer(id)`.
    pub fn play_sound(&mut self, id: &'static str) {
        self.sounds.push(FxSound::Play(id));
    }

    /// `PlayBuffer(id, o)`: bound to an object, so it does not restart while it plays
    /// (`sound::PlayKind::Object`; the client has no 3D sound, so the object's position is not used).
    pub fn play_sound_object(&mut self, id: &'static str) {
        self.sounds.push(FxSound::Object(id));
    }

    /// `PlayBuffer(id, NULL, true)`: loops until stopped.
    pub fn play_sound_loop(&mut self, id: &'static str) {
        self.sounds.push(FxSound::Loop(id));
    }

    /// `StopBuffer(id, true)`. `sound.rs` applies the stops of a frame before its plays, so
    /// `StopBuffer(id); PlayBuffer(id)` restarts the sound, like the original.
    pub fn stop_sound(&mut self, id: &'static str) {
        self.sounds.push(FxSound::Stop(id));
    }

    /// Whether `PlayBuffer(id)` was already requested this tick.
    pub fn sound_requested(&self, id: &'static str) -> bool {
        self.sounds.contains(&FxSound::Play(id))
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

    /// The state of a live effect (`Effects[]` as the original reads it through `o->Target` /
    /// `o->Owner`): as it was at the start of the tick, or as it was created during this tick.
    /// `None` for the effect whose creation code is running (it is not in `Effects[]` yet here).
    pub fn effect_state(&self, id: u32) -> Option<EffectInfo> {
        self.effects.iter().find(|e| e.id == id).copied().or_else(|| {
            self.spawns.iter().rev().find_map(|s| match s {
                Spawn::Effect(e) if e.id == id => Some(EffectInfo::of(e)),
                _ => None,
            })
        })
    }

    /// `o->Target->Light` / `o->Owner->Light`: a character's body light or an effect's light.
    pub fn target_light(&self, t: Option<Target>) -> Option<Vec3> {
        match t? {
            Target::Actor(e) => self.actors.get(&e).map(|a| a.light),
            Target::Effect(id) => self.effect_state(id).map(|e| e.light),
        }
    }

    /// `o->Target->Alpha`.
    pub fn target_alpha(&self, t: Option<Target>) -> Option<f32> {
        match t? {
            Target::Actor(e) => self.actors.get(&e).map(|a| a.alpha),
            Target::Effect(id) => self.effect_state(id).map(|e| e.alpha),
        }
    }

    /// `o->Target->StartPosition`.
    pub fn target_start_position(&self, t: Option<Target>) -> Option<Vec3> {
        match t? {
            Target::Actor(e) => self.actors.get(&e).map(|a| a.start_position),
            Target::Effect(id) => self.effect_state(id).map(|e| e.start_position),
        }
    }

    /// `o->Target->Owner` of an effect target (characters have no owner here).
    pub fn target_owner(&self, t: Option<Target>) -> Option<Target> {
        match t? {
            Target::Actor(_) => None,
            Target::Effect(id) => self.effect_state(id)?.owner,
        }
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
        self.spawn_particle(Particle::new(ty, position, angle, light, sub_type, scale, owner));
    }

    /// `CreateJoint(Type, Position, TargetPosition, Angle, SubType, Target, Scale)`.
    #[allow(clippy::too_many_arguments)]
    pub fn create_joint(&mut self, ty: i32, position: Vec3, target_position: Vec3, angle: Vec3, sub_type: i32, target: Option<Target>, scale: f32) {
        self.spawn_joint(Joint::new(ty, position, target_position, angle, sub_type, target, scale));
    }

    /// `CreateJoint` with `PKKey` and `SkillIndex` (which the original sometimes passes).
    #[allow(clippy::too_many_arguments)]
    pub fn create_joint_ex(&mut self, ty: i32, position: Vec3, target_position: Vec3, angle: Vec3, sub_type: i32, target: Option<Target>, scale: f32, pk_key: i32, skill: i32) {
        let mut j = Joint::new(ty, position, target_position, angle, sub_type, target, scale);
        j.pk_key = pk_key;
        j.skill = skill;
        self.spawn_joint(j);
    }

    /// `CreateEffect(Type, Position, Angle, Light, SubType, Owner, PKKey, Skill, SkillIndex, ..., Scale)`.
    /// Returns the created effect's id.
    #[allow(clippy::too_many_arguments)]
    pub fn create_effect(&mut self, ty: i32, position: Vec3, angle: Vec3, light: Vec3, sub_type: i32, owner: Option<Target>, pk_key: i32, kind: i32, skill: i32, scale: f32) -> u32 {
        *self.next_effect_id += 1;
        let id = *self.next_effect_id;
        self.spawn_effect(Effect::new(id, ty, position, angle, light, sub_type, owner, pk_key, kind, skill, scale));
        id
    }

    /// Creates an already built particle: its kind's creation code runs now (as inside the
    /// original `CreateParticle`), then it joins `Particles[]` at the end of the tick.
    pub fn spawn_particle(&mut self, mut p: Particle) {
        if self.free[0] == 0 || self.depth > MAX_CREATE_DEPTH {
            return;
        }
        self.free[0] -= 1;
        if let Some(create) = self.registry.particles.get(&p.ty).and_then(|k| k.create) {
            self.depth += 1;
            create(&mut p, self);
            self.depth -= 1;
        }
        p.prev_position = p.position;
        p.prev_scale = p.scale;
        p.prev_light = p.light;
        if p.live {
            self.spawns.push(Spawn::Particle(p));
        }
    }

    /// Creates an already built trail (e.g. with its light set): see [`Ctx::spawn_particle`].
    pub fn spawn_joint(&mut self, mut j: Joint) {
        if self.free[1] == 0 || self.depth > MAX_CREATE_DEPTH {
            return;
        }
        self.free[1] -= 1;
        if let Some(create) = self.registry.joints.get(&j.ty).and_then(|k| k.create) {
            self.depth += 1;
            create(&mut j, self);
            self.depth -= 1;
        }
        if j.create_tails && j.tails.is_empty() {
            j.tails.push(j.make_tail());
        }
        if j.live {
            self.spawns.push(Spawn::Joint(j));
        }
    }

    /// Creates an already built effect: see [`Ctx::spawn_particle`].
    pub fn spawn_effect(&mut self, mut e: Effect) {
        if self.free[2] == 0 || self.depth > MAX_CREATE_DEPTH {
            return;
        }
        self.free[2] -= 1;
        if let Some(create) = self.registry.effects.get(&e.ty).and_then(|k| k.create) {
            self.depth += 1;
            create(&mut e, self);
            self.depth -= 1;
        }
        e.prev_position = e.position;
        e.prev_angle = e.angle;
        if e.live {
            self.spawns.push(Spawn::Effect(e));
        }
    }

    /// Creates whatever a [`Spawn`] holds (requests from outside the engine).
    pub fn spawn(&mut self, s: Spawn) {
        match s {
            Spawn::Particle(p) => self.spawn_particle(p),
            Spawn::Joint(j) => self.spawn_joint(j),
            Spawn::Effect(e) => self.spawn_effect(e),
        }
    }

    // --- Characters in range (`CharactersClient[]`) ----------------------------------------------

    /// Characters in a fixed order (the original walks `CharactersClient[]` by slot; here by
    /// server key, then entity).
    pub fn characters(&self) -> Vec<(Entity, &Actor)> {
        let mut v: Vec<(Entity, &Actor)> = self.actors.iter().map(|(e, a)| (*e, a)).collect();
        v.sort_by_key(|(e, a)| (a.key, e.to_bits()));
        v
    }

    /// `CheckCharacterRange(so, Range, PKKey, Kind)` (ZzzEffect.cpp:43-65): whether a live,
    /// visible, not dead character other than the effect's owner is within `range` (2D) of
    /// `position`. The hero only counts when `kind` is `KIND_MONSTER`; `kind` != 0 also skips the
    /// characters of that kind. (`PKKey` is not used by the original.)
    pub fn check_character_range(&self, position: Vec3, owner: Option<Target>, range: f32, kind: i32) -> bool {
        self.characters().into_iter().any(|(e, o)| {
            if !o.live || (kind != 0 && kind == o.kind) || owner == Some(Target::Actor(e)) {
                return false;
            }
            let (dx, dy) = (position.x - o.position.x, position.y - o.position.y);
            let distance = (dx * dx + dy * dy).sqrt();
            o.visible && (Some(e) != self.hero || kind == KIND_MONSTER) && !o.dead && distance <= range
        })
    }

    /// `FindHotKey(Skill)` (ZzzInterface.cpp:7995-8009): the hero's slot of that skill, 0 if it has none.
    pub fn find_hot_key(&self, skill: i32) -> i32 {
        self.hero_skills.iter().take(64).position(|&s| s == skill).unwrap_or(0) as i32
    }

    /// `CharacterMachine->PacketSerial` (read: `o->Weapon = PacketSerial`).
    pub fn packet_serial(&self) -> i32 {
        *self.packet_serial as i32
    }

    /// `CharacterMachine->PacketSerial++` (a `BYTE`: wraps at 256). Returns the value before the
    /// increment, like the post-increment the original copies into `o->Weapon`.
    pub fn next_packet_serial(&mut self) -> i32 {
        let s = *self.packet_serial;
        *self.packet_serial = s.wrapping_add(1);
        s as i32
    }

    /// Queues a change to a character (see [`ActorChange`]).
    pub fn change_actor(&mut self, actor: Entity, change: ActorChange) {
        self.actor_requests.push(ActorRequest { actor, change });
    }

    /// `SpinMonstersInTornado(position, range)` (MuMain ZzzEffect.cpp:106-116): every live,
    /// visible, not dying monster within `range` (2D) spins. MuMain moved this out of
    /// `AttackCharacterRange` (Main 5.2 ZzzEffect.cpp:142-158 set `StormTime` on the hit
    /// characters of Twister / Evil Spirit), and only monsters spin there.
    pub fn spin_monsters_in_tornado(&mut self, position: Vec3, range: f32) {
        let hits: Vec<Entity> = self
            .characters()
            .into_iter()
            .filter(|(_, o)| {
                let (dx, dy) = (position.x - o.position.x, position.y - o.position.y);
                o.kind == KIND_MONSTER && o.live && o.visible && !o.dead && dx * dx + dy * dy <= range * range
            })
            .map(|(e, _)| e)
            .collect();
        for e in hits {
            self.change_actor(e, ActorChange::Spin);
        }
    }

    /// `o == &Hero->Object` for an effect's owner or target.
    pub fn is_hero(&self, t: Option<Target>) -> bool {
        self.hero.is_some_and(|h| t == Some(Target::Actor(h)))
    }

    /// `CharacterAttribute->Skill[index]`: the hero's skill in that slot (0 if unknown).
    pub fn hero_skill(&self, index: i32) -> i32 {
        usize::try_from(index).ok().and_then(|i| self.hero_skills.get(i)).copied().unwrap_or(0)
    }

    /// `AttackCharacterRange(Index, Position, Range, Serial, PKKey, SkillSerialNum)`: MuMain
    /// (ZzzEffect.cpp:118-121) stubs it, `return false; // we don't send this packet anymore`. The
    /// client no longer says whom its projectiles hit (Main 5.2 sent `SendRequestMagicAttack` with
    /// up to 5 keys): the server works out the area hits itself, so a modified client cannot pick
    /// its victims. OpenMU agrees: it only takes client hits for `AreaSkillExplicitHits` skills
    /// (`AreaSkillHitAction.cs:19-25`), and none of its Season 6 skills is one.
    /// The tornado spin Main 5.2 did here is `spin_monsters_in_tornado` (MuMain).
    pub fn attack_character_range(&mut self, _index: i32, _position: Vec3, _range: f32, _serial: i32, _pk_key: i32) -> bool {
        false
    }

    // --- Effect models (`Models[o->Type]`) ---------------------------------------------------------

    /// `Models[ty]` (its `.bmd`), loaded on first use.
    pub fn model(&mut self, ty: i32) -> Option<std::sync::Arc<bmd::Bmd>> {
        self.models.get(ty)
    }

    /// `Models[ty].Actions[action].NumAnimationKeys`.
    pub fn action_frames(&mut self, ty: i32, action: usize) -> Option<usize> {
        self.model(ty)?.actions.get(action).map(|a| a.frames)
    }

    /// The bones of an effect's own model in its current pose (`b->Animation(BoneTransform,
    /// o->AnimationFrame, ...)`), in model space.
    pub fn effect_bones(&mut self, e: &Effect) -> Option<Vec<Mat34>> {
        let m = self.model(e.ty)?;
        if m.bones.is_empty() {
            return None;
        }
        let action = if e.current_action < m.actions.len() { e.current_action } else { 0 };
        Some(m.bone_matrices_at(action, e.animation_frame))
    }

    /// `b->TransformPosition(BoneTransform[bone], offset, out)` on an effect's own model, with the
    /// effect's position, angle and scale (the same convention as [`Actor::bone_world`]).
    pub fn effect_bone_world(&mut self, e: &Effect, bone: usize, offset: Vec3) -> Option<Vec3> {
        let bones = self.effect_bones(e)?;
        let m = bones.get(bone)?;
        let local = Vec3::from_array(bmd::transform_point(m, offset.to_array()));
        Some(e.position + rotate(e.angle, local * e.scale))
    }

    /// `Models[ty].NumBones`.
    pub fn model_bone_count(&mut self, ty: i32) -> Option<usize> {
        Some(self.model(ty)?.bones.len())
    }

    /// `CreateSprite(Type, Position, Scale, Light, Owner, Rotation, SubType)`: lasts this tick.
    pub fn create_sprite(&mut self, ty: i32, position: Vec3, scale: f32, light: Vec3, rotation: f32, sub_type: i32) {
        self.sprites.push(SpriteFx { ty, position, scale, light, rotation, sub_type, alpha: 1.0 });
    }

    /// `SearchEffect(Type, Owner, SubType)` / `FindSameEffectOfSameOwner` (`sub_type` -1 = any):
    /// whether a live effect of that type and owner exists (as of tick start).
    pub fn search_effect(&self, ty: i32, owner: Option<Target>, sub_type: i32) -> bool {
        self.effects.iter().any(|e| e.ty == ty && e.owner == owner && (sub_type == -1 || e.sub_type == sub_type))
    }

    /// `DeleteEffect(Type, Owner, SubType)` (`sub_type` -1 = any). Applied at the end of the tick.
    pub fn delete_effect(&mut self, ty: i32, owner: Option<Target>, sub_type: i32) {
        self.deletes.push(Delete::Effect { ty, owner, sub_type });
    }

    /// `EffectDestructor` of another effect, by id. Applied at the end of the tick.
    pub fn delete_effect_id(&mut self, id: u32) {
        self.deletes.push(Delete::EffectId(id));
    }

    /// `DeleteJoint(Type, Target, SubType)` (`sub_type` -1 = any). Applied at the end of the tick.
    pub fn delete_joint(&mut self, ty: i32, target: Option<Target>, sub_type: i32) {
        self.deletes.push(Delete::Joint { ty, target, sub_type });
    }

    /// `DeleteParticle(Type)`: the first live particle of that type. Applied at the end of the tick.
    pub fn delete_particle(&mut self, ty: i32) {
        self.deletes.push(Delete::Particle(ty));
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
    /// Extra `RENDER_*` flags of `RenderMesh`/`RenderPartObject` (ZzzBMD.h:10-30): [`RENDER_CHROME`]
    /// (and CHROME2..7), [`RENDER_METAL`], [`RENDER_DARK`] (subtractive), [`RENDER_BRIGHT`]... 0 = the
    /// default (`RENDER_TEXTURE`, plus what `bright` and `blend_mesh` say).
    pub render_flags: u32,
    /// `RenderMesh`'s `Texture` argument: a `BITMAP_*` drawn instead of the mesh's own texture
    /// (-1 = the mesh's texture).
    pub texture: i32,
    /// Draw only this mesh, by mesh index (`RenderMesh(i, ...)`); -1 = every mesh. Push several
    /// `ModelDraw`s to give meshes their own light or flags.
    pub mesh: i32,
}

/// `RENDER_*` flags (ZzzBMD.h:10-30).
pub const RENDER_COLOR: u32 = 0x0000_0001;
pub const RENDER_TEXTURE: u32 = 0x0000_0002;
pub const RENDER_CHROME: u32 = 0x0000_0004;
pub const RENDER_METAL: u32 = 0x0000_0008;
pub const RENDER_LIGHTMAP: u32 = 0x0000_0010;
pub const RENDER_BRIGHT: u32 = 0x0000_0040;
pub const RENDER_DARK: u32 = 0x0000_0080;
pub const RENDER_EXTRA: u32 = 0x0000_0100;
pub const RENDER_CHROME2: u32 = 0x0000_0200;
pub const RENDER_WAVE: u32 = 0x0000_0400;
pub const RENDER_CHROME3: u32 = 0x0000_0800;
pub const RENDER_CHROME4: u32 = 0x0000_1000;
pub const RENDER_NODEPTH: u32 = 0x0000_2000;
pub const RENDER_CHROME5: u32 = 0x0000_4000;
pub const RENDER_OIL: u32 = 0x0000_8000;
pub const RENDER_CHROME6: u32 = 0x0001_0000;
pub const RENDER_CHROME7: u32 = 0x0002_0000;
pub const RENDER_DOPPELGANGER: u32 = 0x0004_0000;

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
            render_flags: 0,
            texture: -1,
            mesh: -1,
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
            render_flags: 0,
            texture: -1,
            mesh: -1,
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
    /// `gMapManager.WorldActive` (Lorencia = 0).
    pub world_active: i32,
    /// Effect model files already loaded by the simulation.
    pub models: &'a super::model_data::ModelData,
    /// The map and this tick's terrain lights, for `RequestTerrainLight` while drawing.
    pub terrain: Option<(&'a Terrain, &'a TerrainLights)>,
    /// `g_pOption->GetRenderLevel()` (0 to 4, `options::GameOptions::render_level`).
    pub render_level: u8,
}

impl DrawCtx<'_> {
    pub fn tex(&self, id: i32) -> Option<TexInfo> {
        (self.tex_info)(id)
    }

    /// `b->BodyLight` of `RenderObject` for an effect (MuMain ZzzObject.cpp:272-285): the terrain
    /// light plus `o->Light`, the terrain light at a tenth without `LightEnable`.
    /// (`RequestModelTerrainLight` is `RequestTerrainLight` without MuMain's per-pixel lights.)
    pub fn body_light(&self, e: &Effect) -> Vec3 {
        let t = self.terrain_light(e.position.x, e.position.y);
        if e.light_enable { t + e.light } else { t * 0.1 + e.light }
    }

    /// `RequestTerrainLight(x, y)` with x, y in MU (`terrain::request_terrain_light`); (0, 0, 0)
    /// without a map, like the original off the terrain.
    pub fn terrain_light(&self, x: f32, y: f32) -> Vec3 {
        self.terrain.map_or(Vec3::ZERO, |(t, l)| crate::terrain::request_terrain_light(t, l, x, y))
    }

    /// The bones of an effect's own model in its (interpolated) pose, if the simulation has
    /// loaded the model (see [`Ctx::effect_bones`]).
    pub fn effect_bones(&self, e: &Effect) -> Option<Vec<Mat34>> {
        let m = self.models.loaded(e.ty)?;
        if m.bones.is_empty() {
            return None;
        }
        let action = if e.current_action < m.actions.len() { e.current_action } else { 0 };
        Some(m.bone_matrices_at(action, e.animation_frame))
    }

    /// `b->TransformPosition(BoneTransform[bone], offset, out)` on an effect's own model while drawing.
    pub fn effect_bone_world(&self, e: &Effect, bone: usize, offset: Vec3) -> Option<Vec3> {
        let bones = self.effect_bones(e)?;
        let local = Vec3::from_array(bmd::transform_point(bones.get(bone)?, offset.to_array()));
        Some(e.position + rotate(e.angle, local * e.scale))
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
    /// Kinds registered twice (the later one wins): ('p'/'j'/'e', type). Checked by the tests.
    pub duplicates: Vec<(char, i32)>,
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
        if self.particles.insert(ty, kind).is_some() {
            self.duplicates.push(('p', ty));
        }
    }
    pub fn joint(&mut self, ty: i32, kind: JointKind) {
        if self.joints.insert(ty, kind).is_some() {
            self.duplicates.push(('j', ty));
        }
    }
    pub fn effect(&mut self, ty: i32, kind: EffectKind) {
        if self.effects.insert(ty, kind).is_some() {
            self.duplicates.push(('e', ty));
        }
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
