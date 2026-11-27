//! Cloth capes and hair (`CPhysicsCloth`, MuMain PhysicsManager.cpp), created and stepped by
//! `UpdateCloakCloth` (ZzzCharacter.cpp:9132-9427) and drawn by `RenderCharacter`
//! (ZzzCharacter.cpp:10295-10304): the Magic Gladiator's robe, the Dark Lord's hair, skirt and capes
//! (Cape of Lord, Cape of Emperor) and the Rage Fighter's capes (Cape of Fighter, Cloak of
//! Overrule). The capes that are only cloth (`MODEL_HELPER+30`, `MODEL_WING+49/+130/+135`) have no
//! model drawn (`RenderLinkObject`, ZzzCharacter.cpp:7294-7310); the Cape of Emperor and the Cloak
//! of Overrule draw their model too (character.rs).
//!
//! The original steps each cloth once per drawn frame (`Move2(0.005f, 5)`, with the forces scaled
//! by `FPS_ANIMATION_FACTOR`); here once per 25 Hz tick, its reference rate (factor 1), and the
//! drawing interpolates between the last two ticks. The fixed top row follows the pose being drawn,
//! so the cloth never comes loose from the body between ticks.
//!
//! Positions are MU world coordinates. `TransformPosition(BoneTransform[b], v, true)` is `bone · v`
//! through the character's `Transform`: its rotation is `o->Angle`, its scale `o->Scale` and its
//! translation `o->Position` (`BodyScale` / `BodyOrigin`, ZzzCharacter.cpp:12113-12119).
//!
//! Not done: the duel, soccer and guild-war tints (`CloakOf`, ZzzCharacter.cpp:9030-9086), the
//! transformation rings that hide the hair or the robe (`CheckDarkLordHair`, `CheckDarkCloak`),
//! the Cursed Temple players, the Lord Emperor's own skirt (no class evolution yet), the
//! `eBuff_Cloaking`, stun and sleep buffs, and the monsters with a cloth (Death King, Nightmare).

use std::collections::HashMap;

use bevy::prelude::*;
use mu_data::bmd::{self, Mat34};

use crate::account::Class;
use crate::fx::draw::{linear, mu_to_bevy};
use crate::fx::material::{Blend, FxMaterial};
use crate::fx::tables::ids;
use crate::terrain::MU_SCALE;

pub struct ClothPlugin;

impl Plugin for ClothPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ClothWorld>().add_systems(
            PostUpdate,
            update_cloth.before(crate::fx::draw::collect).before(TransformSystems::Propagate),
        );
    }
}

/// What decides a character's cloths: its class, `Wing.Type` as (group, index), and whether its
/// armor is the class's own (`BodyPart[BODYPART_ARMOR].Type == MODEL_BODY_ARMOR + class`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CloakGear {
    pub class: Class,
    pub wing: Option<(u8, u16)>,
    pub class_armor: bool,
}

use mu_data::items::group::{HELPER, WING};
/// `MODEL_CAPE_OF_LORD` = `MODEL_HELPER + 30` (_enum.h:1889).
const CAPE_OF_LORD: (u8, u16) = (HELPER, 30);
/// `MODEL_CAPE_OF_EMPEROR`, `MODEL_SMALL_CAPE_OF_LORD`, `MODEL_CAPE_OF_FIGHTER`,
/// `MODEL_CAPE_OF_OVERRULE`, `MODEL_LITTLE_WARRIORS_CLOAK` and `MODEL_WING_OF_RUIN`
/// (`MODEL_WING` + 40, 130, 49, 50, 135, 39).
const CAPE_OF_EMPEROR: (u8, u16) = (WING, 40);
const SMALL_CAPE_OF_LORD: (u8, u16) = (WING, 130);
const CAPE_OF_FIGHTER: (u8, u16) = (WING, 49);
const CAPE_OF_OVERRULE: (u8, u16) = (WING, 50);
const LITTLE_WARRIORS_CLOAK: (u8, u16) = (WING, 135);
const WING_OF_RUIN: (u8, u16) = (WING, 39);

// `PCT_*` / `PLS_*` (PhysicsManager.h:121-161).
const PCT_MASK_SHAPE: u32 = 0x3;
const PCT_FLAT: u32 = 0x0;
const PCT_CURVED: u32 = 0x1;
const PCT_STICKED: u32 = 0x2;
const PCT_MASK_SHAPE_EXT: u32 = 0xC;
const PCT_SHAPE_NORMAL: u32 = 0x0;
const PCT_SHORT_SHOULDER: u32 = 0x4;
const PCT_CYLINDER: u32 = 0x8;
const PCT_MASK_SHAPE_EXT2: u32 = 0x30;
const PCT_SHAPE_HALLOWEEN: u32 = 0x10;
const PCT_MASK_ELASTIC: u32 = 0x300;
const PCT_COTTON: u32 = 0x0;
const PCT_RUBBER: u32 = 0x100;
const PCT_RUBBER2: u32 = 0x200;
const PCT_MASK_WEIGHT: u32 = 0xC00;
const PCT_NORMAL_THICKNESS: u32 = 0x0;
const PCT_HEAVY: u32 = 0x400;
const PCT_MASK_DRAW: u32 = 0x3000;
const PCT_MASK_ALPHA: u32 = 0x1000;
const PCT_MASK_BLEND: u32 = 0x2000;
const PCT_MASK_ELASTIC_EXT: u32 = 0xE000;
const PCT_ELASTIC_HALLOWEEN: u32 = 0x4000;
const PCT_ELASTIC_RAGE_L: u32 = 0x8000;
const PCT_ELASTIC_RAGE_R: u32 = 0xC000;
const PCT_OPT_CORRECTEDFORCE: u32 = 0x2000_0000;
const PCT_MASK_LIGHT: u32 = 0x4000_0000;
const PCT_OPT_HAIR: u32 = 0x8000_0000;
const PLS_LOOSEDISTANCE: u8 = 0x1;
const PLS_SPRING: u8 = 0x2;
const PLS_STRICTDISTANCE: u32 = 0x4;

/// `RATE_SHORT_SHOULDER` (PhysicsManager.cpp:18).
const RATE_SHORT_SHOULDER: f32 = 0.6;
/// `CPhysicsVertex::s_Gravity`, `s_fMass`, `s_fInvOfMass` (PhysicsManager.cpp:20-22).
const GRAVITY: f32 = 9.8;
const MASS: f32 = 0.0025;
const INV_MASS: f32 = 400.0;
/// `Move2(Flag = 0.005f, 5)` (ZzzCharacter.cpp:9413-9418).
const STEP_TIME: f32 = 0.005;
const STEPS: usize = 5;
/// `WD_55LOGINSCENE` (MapManager.h:51): its own wind (PhysicsManager.cpp:595-596).
const WD_55LOGINSCENE: i32 = 55;

/// `CPhysicsVertex`.
#[derive(Clone, Copy, Default)]
struct Vertex {
    force: Vec3,
    vel: Vec3,
    pos: Vec3,
    /// `PVS_FIXEDPOS`.
    fixed: bool,
    one_time: Vec3,
    one_time_count: i32,
}

impl Vertex {
    /// `UpdateForce` (PhysicsManager.cpp:52-110), `s_vWind` = `wind_vec`.
    fn update_force(&mut self, key: u32, ty: u32, wind: f32, wind_vec: Vec3) {
        if self.fixed {
            self.force = Vec3::ZERO;
            return;
        }
        let t = (5 - key as i32).clamp(0, 4);
        let rand = if t == 0 { 0.0 } else { (t + 2) as f32 };
        self.force = rand * wind_vec - self.vel * 0.01;
        match ty & PCT_MASK_ELASTIC {
            PCT_RUBBER => self.force.z += rand * (wind + 0.1),
            PCT_RUBBER2 => self.force.z += rand * wind,
            _ => {}
        }
        match ty & PCT_MASK_ELASTIC_EXT {
            // The Halloween cloth also reads `WorldTime`; no character wears it here.
            PCT_ELASTIC_RAGE_L => self.force.x += -(rand * wind * 0.8),
            PCT_ELASTIC_RAGE_R => self.force.x -= -(rand * wind * 0.8),
            _ => {}
        }
        self.force.z -= GRAVITY * MASS * if ty & PCT_MASK_WEIGHT == PCT_HEAVY { 180.0 } else { 100.0 };
    }

    /// `Move` (PhysicsManager.cpp:119-131).
    fn advance(&mut self, dt: f32) {
        if self.fixed {
            return;
        }
        self.vel += self.force * INV_MASS * dt;
        self.pos += self.vel * dt;
    }

    /// `DoOneTimeMove` (PhysicsManager.cpp:191-209): a fixed vertex drops its move (but keeps the count).
    fn do_one_time_move(&mut self) {
        if self.fixed {
            self.one_time = Vec3::ZERO;
            return;
        }
        if self.one_time_count > 0 {
            self.pos += self.one_time / self.one_time_count as f32;
            self.one_time = Vec3::ZERO;
            self.one_time_count = 0;
        }
    }

    fn add_one_time_move(&mut self, v: Vec3) {
        self.one_time += v;
        self.one_time_count += 1;
    }
}

/// `St_PhysicsLink`.
#[derive(Clone, Copy)]
struct Link {
    v: [usize; 2],
    dist: [f32; 2],
    style: u8,
}

/// `CPhysicsColSphere`.
#[derive(Clone, Copy)]
struct Sphere {
    /// `m_vCenterBeforeTransform`.
    local: Vec3,
    radius: f32,
    bone: usize,
}

/// Where a character is this frame: its `Transform` (MU model → Bevy world) and its bones.
pub struct Pose<'a> {
    transform: &'a Transform,
    bones: &'a [Mat34],
}

/// Bevy world → MU world.
fn to_mu(b: Vec3) -> Vec3 {
    Vec3::new(b.x, -b.z, b.y) * MU_SCALE
}

impl Pose<'_> {
    /// A point of the model's own frame (rotated by `o->Angle` only) in the world.
    fn local(&self, v: Vec3) -> Vec3 {
        to_mu(self.transform.transform_point(v))
    }

    /// `TransformPosition(BoneTransform[bone], v, true)`.
    fn on_bone(&self, bone: usize, v: Vec3) -> Vec3 {
        let p = self.bones.get(bone).map_or(v, |m| Vec3::from_array(bmd::transform_point(m, v.to_array())));
        self.local(p)
    }

    /// `o->Angle[2]` in degrees.
    fn angle_z(&self) -> f32 {
        let x = self.transform.rotation * Vec3::X;
        let mu = Vec3::new(x.x, -x.z, x.y);
        mu.y.atan2(mu.x).to_degrees()
    }
}

/// `(x, y, z)` → `(z, -y, x)`: how the fixed row and the collision spheres are given in bone space
/// (PhysicsManager.cpp:552-556, 773-779).
fn swizzle(v: Vec3) -> Vec3 {
    Vec3::new(v.z, -v.y, v.x)
}

/// `CPhysicsCloth`.
#[derive(Clone)]
struct Cloth {
    bone: usize,
    tex_front: i32,
    tex_back: i32,
    ty: u32,
    offset: Vec3,
    width: f32,
    hor: usize,
    ver: usize,
    unit_width: f32,
    verts: Vec<Vertex>,
    links: Vec<Link>,
    wind: f32,
    wind_max: u8,
    wind_min: u8,
    spheres: Vec<Sphere>,
    /// Positions before the last tick (the drawing interpolates from them).
    prev: Vec<Vec3>,
}

/// The original's `rand()` (MSVC's generator); cloths use it only through `%`.
struct Rand(u32);

impl Rand {
    fn rand(&mut self) -> i32 {
        self.0 = self.0.wrapping_mul(214_013).wrapping_add(2_531_011);
        ((self.0 >> 16) & 0x7fff) as i32
    }
}

/// What a step needs besides the cloth.
struct StepCtx<'a, 'b> {
    pose: &'a Pose<'b>,
    /// `CPhysicsManager::s_fWind`.
    global_wind: f32,
    world_active: i32,
    world_time: f32,
}

impl Cloth {
    /// `Create` (PhysicsManager.cpp:329-486). `None` if the character has no such bone.
    #[allow(clippy::too_many_arguments)]
    fn create(
        pose: &Pose,
        bone: usize,
        offset: [f32; 3],
        hor: usize,
        ver: usize,
        width: f32,
        height: f32,
        tex_front: i32,
        tex_back: i32,
        ty: u32,
    ) -> Option<Self> {
        let bone_pos = pose.bones.get(bone).map(|m| Vec3::new(m[0][3], m[1][3], m[2][3]))?;
        let unit_width = width / (hor - 1) as f32;
        let unit_height = height / (ver - 1) as f32;
        // `AngleMatrix(o->Angle)` with the bone's position: the model's frame, moved to the bone.
        let place = |v: Vec3| pose.local(v + bone_pos);
        let mut verts = vec![Vertex::default(); hor * ver];
        let mut cylinder = false;
        for j in 0..ver {
            let (mut w, mut uw) = (width, unit_width);
            match ty & PCT_MASK_SHAPE_EXT {
                PCT_SHORT_SHOULDER => {
                    w *= RATE_SHORT_SHOULDER + (1.0 - RATE_SHORT_SHOULDER) * j as f32 / (ver - 1) as f32;
                    uw = w / (hor - 1) as f32;
                }
                PCT_CYLINDER => cylinder = true,
                _ => {}
            }
            for i in 0..hor {
                let mut v = if cylinder {
                    let y = (i as f32 / (hor - 1) as f32 * std::f32::consts::PI).sin() * 100.0;
                    Vec3::new(uw * i as f32, y - 30.0, unit_height * j as f32)
                } else {
                    Vec3::new(uw * i as f32 - 0.5 * w, 20.0, -unit_height * j as f32)
                };
                match ty & PCT_MASK_SHAPE {
                    PCT_CURVED => {
                        let m = 2.0 * (i as f32 / (hor - 1) as f32 - 0.5).abs();
                        v.y -= 10.0 * m * m;
                    }
                    PCT_STICKED => v.y = 0.0,
                    _ => {}
                }
                verts[hor * j + i].pos = place(v);
            }
        }
        let (mut ver_style, mut cross_style) = (0u8, 0u8);
        if ty & PCT_MASK_ELASTIC != PCT_RUBBER {
            ver_style |= PLS_STRICTDISTANCE as u8;
            cross_style |= PLS_LOOSEDISTANCE;
        }
        let mut links = Vec::new();
        let mut link = |a: usize, b: usize, style: u8| {
            let d = (verts[a].pos - verts[b].pos).length();
            links.push(Link { v: [a, b], dist: [d * 0.8, d], style });
        };
        for j in 0..ver {
            for i in 0..hor {
                let v = hor * j + i;
                if j < ver - 1 {
                    link(v, v + hor, PLS_SPRING | ver_style);
                }
                if i < hor - 1 {
                    link(v, v + 1, PLS_SPRING | PLS_LOOSEDISTANCE);
                    if j < ver - 1 {
                        link(v, v + 1 + hor, cross_style);
                    }
                    // `j > 1`, not `j > 0` (PhysicsManager.cpp:474).
                    if j > 1 {
                        link(v, v + 1 - hor, cross_style);
                    }
                }
            }
        }
        let mut cloth = Cloth {
            bone,
            tex_front,
            tex_back,
            ty,
            offset: Vec3::from_array(offset),
            width,
            hor,
            ver,
            unit_width,
            verts,
            links,
            wind: 0.0,
            // `Clear` (PhysicsManager.cpp:325-326).
            wind_max: 1,
            wind_min: 1,
            spheres: Vec::new(),
            prev: Vec::new(),
        };
        cloth.set_fixed(place);
        cloth.prev = cloth.verts.iter().map(|v| v.pos).collect();
        Some(cloth)
    }

    fn with_wind(mut self, min: u8, max: u8) -> Self {
        self.wind_min = min;
        self.wind_max = max;
        self
    }

    /// `AddCollisionSphere`.
    fn with_sphere(mut self, x: f32, y: f32, z: f32, radius: f32, bone: usize) -> Self {
        self.spheres.push(Sphere { local: Vec3::new(x, y, z), radius, bone });
        self
    }

    /// The fixed row in bone space (`SetFixedVertices`, PhysicsManager.cpp:504-562), before `place`.
    fn fixed_row(&self) -> impl Iterator<Item = Vec3> + '_ {
        let mut cylinder = false;
        (0..self.hor).map(move |i| {
            let (mut w, mut uw) = (self.width, self.unit_width);
            match self.ty & PCT_MASK_SHAPE_EXT {
                PCT_SHORT_SHOULDER => {
                    w *= RATE_SHORT_SHOULDER;
                    uw = w / (self.hor - 1) as f32;
                }
                PCT_CYLINDER => cylinder = true,
                _ => {}
            }
            let mut v = Vec3::new(uw * i as f32 - 0.5 * w + self.offset.x, self.offset.y, self.offset.z);
            if cylinder {
                v.y = (i as f32 / (self.hor - 1) as f32 * std::f32::consts::PI).sin() * 130.0 - 110.0;
            }
            if self.ty & PCT_MASK_SHAPE_EXT2 == PCT_SHAPE_HALLOWEEN {
                v.x += 5.0;
                v.y += 10.0;
            }
            if self.ty & PCT_MASK_SHAPE == PCT_CURVED {
                let m = 2.0 * (i as f32 / (self.hor - 1) as f32 - 0.5).abs();
                v.y -= 10.0 * m * m;
            }
            swizzle(v)
        })
    }

    /// `SetFixedVertices`: `Init(..., TRUE)` keeps the velocity.
    fn set_fixed(&mut self, place: impl Fn(Vec3) -> Vec3) {
        let row: Vec<Vec3> = self.fixed_row().map(place).collect();
        for (v, p) in self.verts.iter_mut().zip(row) {
            v.pos = p;
            v.fixed = true;
        }
    }

    /// `Move2(fTime, iCount)` (PhysicsManager.cpp:574-585). `false` when the cloth tore.
    fn step(&mut self, ctx: &StepCtx, rng: &mut Rand) -> bool {
        self.prev = self.verts.iter().map(|v| v.pos).collect();
        (0..STEPS).all(|_| self.move_once(ctx, rng))
    }

    /// `Move` (PhysicsManager.cpp:587-624).
    fn move_once(&mut self, ctx: &StepCtx, rng: &mut Rand) -> bool {
        self.wind = match self.ty & PCT_MASK_ELASTIC {
            PCT_RUBBER2 => (rng.rand() % self.wind_max as i32 + self.wind_min as i32) as f32 / 100.0,
            _ if ctx.world_active == WD_55LOGINSCENE => (rng.rand() % 40 + 10) as f32 / 50.0,
            _ => ctx.global_wind,
        };
        if self.ty & PCT_MASK_SHAPE_EXT == PCT_CYLINDER {
            self.wind = (rng.rand() % 10 + 25) as f32 / 50.0;
        }
        // `s_vWind[2]` is never written: it stays 0 (PhysicsManager.cpp:613-614, 1322).
        let a = (180.0 + ctx.pose.angle_z()).to_radians();
        let wind_vec = Vec3::new(self.wind * a.sin(), -self.wind * a.cos(), 0.0);
        self.move_vertices(ctx, wind_vec);
        self.prevent_from_stretching(ctx.pose)
    }

    /// `MoveVertices` (PhysicsManager.cpp:706-761) with `InitForces` (:696-704).
    fn move_vertices(&mut self, ctx: &StepCtx, wind_vec: Vec3) {
        let n = self.verts.len() as i32;
        let seed = ((ctx.world_time / 400.0) as i32).wrapping_mul(101) % n;
        let hor = self.hor as i32;
        for (k, v) in self.verts.iter_mut().enumerate() {
            let k = k as i32;
            let key = (seed % hor - k % hor).abs() + (seed / hor - k / hor).abs();
            v.update_force(key as u32, self.ty, self.wind, wind_vec);
        }
        for l in &self.links {
            if l.style & PLS_SPRING == 0 {
                continue;
            }
            let d = self.verts[l.v[0]].pos - self.verts[l.v[1]].pos;
            let len = d.length().max(0.001);
            if len <= l.dist[1] + 0.01 {
                continue;
            }
            let mut f = (len - l.dist[1]) * d / len;
            if self.ty & PCT_OPT_CORRECTEDFORCE != 0 {
                f *= l.dist[1] / 32.0;
            }
            if self.ty & PCT_MASK_ELASTIC == PCT_RUBBER {
                f *= 3.0;
            }
            if self.ty & PCT_MASK_ELASTIC_EXT == PCT_ELASTIC_HALLOWEEN {
                f *= 2.0;
            }
            self.verts[l.v[0]].force -= f;
            self.verts[l.v[1]].force += f;
        }
        let bone = self.bone;
        self.set_fixed(|v| ctx.pose.on_bone(bone, v));
        for v in &mut self.verts {
            v.advance(STEP_TIME);
        }
    }

    /// `PreventFromStretching` (PhysicsManager.cpp:763-824) with `ProcessCollision` (:1004-1025).
    fn prevent_from_stretching(&mut self, pose: &Pose) -> bool {
        if !self.spheres.is_empty() {
            for s in &self.spheres {
                let center = pose.on_bone(s.bone, swizzle(s.local));
                // `CPhysicsColSphere::ProcessCollision` (PhysicsManager.cpp:280-297).
                for v in &mut self.verts {
                    let mut d = v.pos - center;
                    let mut len = d.length();
                    if len < 0.01 {
                        len = 0.01;
                        d = Vec3::new(len, 0.0, 0.0);
                    }
                    if len < s.radius {
                        v.add_one_time_move(d * ((s.radius - len) / len));
                    }
                }
            }
            for v in &mut self.verts {
                v.do_one_time_move();
            }
        }
        for l in &self.links {
            if l.style & PLS_LOOSEDISTANCE == 0 {
                continue;
            }
            // `AddOneTimeMoveToKeepLength` (PhysicsManager.cpp:180-189).
            let d = self.verts[l.v[0]].pos - self.verts[l.v[1]].pos;
            let len = d.length().max(0.001);
            let d = d * ((len - l.dist[1]) * 0.5 / len);
            self.verts[l.v[0]].add_one_time_move(-d);
            self.verts[l.v[1]].add_one_time_move(d);
        }
        for v in &mut self.verts {
            v.do_one_time_move();
        }
        for l in &self.links {
            if l.v[1] < self.hor || l.style & PLS_STRICTDISTANCE as u8 == 0 {
                continue;
            }
            // `pVertex2->KeepLength(pVertex1, m_fDistance)` (PhysicsManager.cpp:151-178).
            let other = self.verts[l.v[0]].pos;
            let v = &mut self.verts[l.v[1]];
            if v.fixed {
                continue;
            }
            let d = v.pos - other;
            let len = d.length().max(0.001);
            if len > l.dist[1] * 20.0 {
                return false;
            }
            if len > l.dist[1] {
                v.pos -= d * ((len - l.dist[1]) / len);
            } else if len < l.dist[0] {
                v.pos -= d * ((len - l.dist[0]) / len);
            }
        }
        true
    }

    /// `SpawnLights` (PhysicsManager.cpp:826-851): `CreateSprite(BITMAP_LIGHT, ...)` on every
    /// vertex of a glowing cloth. The index runs through the vertices in order while `i` counts
    /// the columns of the outer loop, as in the original.
    fn spawn_lights(&self, world_time: f32, sprites: &mut Vec<crate::fx::core::SpriteFx>) {
        if self.ty & PCT_MASK_LIGHT == 0 {
            return;
        }
        let lum = (world_time * 0.001).sin() * 0.1 + 0.4;
        let mut k = 0;
        for i in 0..self.hor {
            for _ in 0..self.ver {
                let l = lum * (self.hor - i) as f32 / 5.0;
                sprites.push(crate::fx::core::SpriteFx {
                    ty: ids::BITMAP_LIGHT,
                    position: self.verts[k].pos,
                    scale: i as f32 * 0.2 + 0.4,
                    light: Vec3::splat(l),
                    rotation: 0.0,
                    sub_type: 0,
                    alpha: 1.0,
                });
                k += 1;
            }
        }
    }

    /// The drawn positions: interpolated between the last two ticks, the fixed row in the pose
    /// being drawn.
    fn drawn(&self, pose: &Pose, a: f32) -> Vec<Vec3> {
        let mut out: Vec<Vec3> = self.prev.iter().zip(&self.verts).map(|(p, v)| p.lerp(v.pos, a)).collect();
        for (o, p) in out.iter_mut().zip(self.fixed_row()) {
            *o = pose.on_bone(self.bone, p);
        }
        out
    }

    /// `Render` / `RenderFace` (PhysicsManager.cpp:853-965) into the batches.
    fn draw(&self, pos: &[Vec3], world_time: f32, out: &mut HashMap<BatchKey, Batch>) {
        let light = self.ty & PCT_MASK_LIGHT != 0;
        let draw = self.ty & PCT_MASK_DRAW;
        // `DisableAlphaBlend` (with `EnableCullFace`), `EnableAlphaTest` or `EnableAlphaBlend`;
        // a glowing cloth always ends in `EnableAlphaBlend`.
        let (blend, cull) = match draw {
            _ if light => (Blend::Glow, false),
            PCT_MASK_ALPHA => (Blend::AlphaTest, false),
            PCT_MASK_BLEND => (Blend::Glow, false),
            _ => (Blend::Opaque, true),
        };
        let c = if light { (world_time * 0.001).sin() * 0.1 + 0.4 } else { 1.0 };
        let color = linear([c, c, c, 1.0]);
        let (hor, ver) = (self.hor, self.ver);
        let uv = |i: usize, j: usize| [i as f32 / (hor - 1) as f32, (j as f32 / (ver - 1) as f32).min(0.99)];
        let at = |i: usize, j: usize| pos[hor * j + i];
        let mut face = |front: bool, texture: i32| {
            let b = out.entry(BatchKey { texture, blend, cull }).or_default();
            for j in 0..ver - 1 {
                for i in 0..hor - 1 {
                    let q = if front {
                        [(i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1)]
                    } else {
                        [(i, j), (i, j + 1), (i + 1, j + 1), (i + 1, j)]
                    };
                    b.quad(q.map(|(x, y)| at(x, y)), q.map(|(x, y)| uv(x, y)), color);
                }
            }
        };
        face(true, self.tex_front);
        // Without culling, an alpha-tested back face with the same texture draws the same pixels
        // again; the additive ones add up (they are drawn twice, as in the original).
        let same = blend == Blend::AlphaTest && self.tex_back == self.tex_front;
        if (draw != PCT_MASK_BLEND || !light) && !same {
            face(false, self.tex_back);
        }
    }
}

/// `UpdateCloakCloth`'s creation (ZzzCharacter.cpp:9138-9401) and `IsCloakClothHidden`
/// (:9091-9130) with the white `cloak.light`: the hidden cloths are neither stepped nor drawn,
/// so they are not built. The Dark Lord's skirt (`pCloth[3]`) as it ends up after the next frame's
/// checks (:9334-9346): without the Cape of Emperor `m_byNumCloth` is 3 once created, so it is
/// created again with `PCT_CURVED` added to the type it already had (`m_dwType |= dwType`,
/// PhysicsManager.cpp:337), which turns its shape bits to 3 (neither curved nor sticked).
fn build(g: &CloakGear, pose: &Pose) -> Vec<Cloth> {
    use ids::{BITMAP_DARK_LOAD_SKIRT, BITMAP_MANTO01, BITMAP_MANTOE, BITMAP_NCCAPE, BITMAP_ROBE};
    let mut out = Vec::new();
    let mut add = |c: Option<Cloth>| out.extend(c);
    let cape_spheres = |c: Cloth, r1: f32, r2: f32| {
        c.with_sphere(-10.0, -10.0, -10.0, r1, 17)
            .with_sphere(10.0, -10.0, -10.0, r1, 17)
            .with_sphere(-10.0, -10.0, 20.0, r2, 17)
            .with_sphere(10.0, -10.0, 20.0, r2, 17)
    };
    let hair_spheres = |c: Cloth| c.with_sphere(-10.0, 20.0, 20.0, 27.0, 17).with_sphere(10.0, 20.0, 20.0, 27.0, 17);
    match g.class {
        Class::Lord => {
            let emperor = g.wing == Some(CAPE_OF_EMPEROR);
            add(Cloth::create(pose, 20, [0.0, 0.0, 20.0], 6, 5, 30.0, 70.0, BITMAP_ROBE + 6, BITMAP_ROBE + 6,
                PCT_CURVED | PCT_RUBBER2 | PCT_MASK_LIGHT | PLS_STRICTDISTANCE | PCT_SHORT_SHOULDER
                    | PCT_NORMAL_THICKNESS | PCT_OPT_HAIR)
                .map(|c| hair_spheres(c.with_wind(10, 50))));
            add(Cloth::create(pose, 20, [0.0, 5.0, 18.0], 5, 5, 30.0, 70.0, BITMAP_ROBE + 6, BITMAP_ROBE + 6,
                PCT_CURVED | PCT_RUBBER2 | PCT_MASK_BLEND | PLS_STRICTDISTANCE | PCT_SHORT_SHOULDER
                    | PCT_NORMAL_THICKNESS | PCT_OPT_HAIR)
                .map(|c| hair_spheres(c.with_wind(8, 40))));
            let cape = match g.wing {
                Some(CAPE_OF_EMPEROR) => Some((180.0, BITMAP_ROBE + 9, PCT_HEAVY)),
                Some(SMALL_CAPE_OF_LORD) => Some((100.0, BITMAP_ROBE + 7, 0)),
                Some(CAPE_OF_LORD) => Some((180.0, BITMAP_ROBE + 7, 0)),
                _ => None,
            };
            if let Some((size, tex, heavy)) = cape {
                add(Cloth::create(pose, 19, [0.0, 8.0, 10.0], 10, 10, size, size, tex, tex,
                    PCT_CURVED | PCT_SHORT_SHOULDER | heavy | PCT_MASK_ALPHA)
                    .map(|c| cape_spheres(c, 25.0, 27.0)));
            }
            if g.class_armor {
                let skirt = PCT_MASK_ALPHA | PCT_HEAVY | PCT_STICKED | PCT_SHORT_SHOULDER;
                let (y, ty) = if emperor { (10.0, skirt) } else { (8.0, skirt | PCT_CURVED) };
                add(Cloth::create(pose, 18, [0.0, y, -5.0], 5, 5, 50.0, 90.0, BITMAP_DARK_LOAD_SKIRT,
                    BITMAP_DARK_LOAD_SKIRT, ty)
                    .map(|c| c.with_sphere(0.0, -15.0, -20.0, 30.0, 2)));
            }
            if emperor {
                for (x, y) in [(30.0, 15.0), (-30.0, 20.0)] {
                    add(Cloth::create(pose, 19, [x, y, 10.0], 2, 5, 12.0, 200.0, BITMAP_ROBE + 10, BITMAP_ROBE + 10,
                        PCT_FLAT | PCT_SHAPE_NORMAL | PCT_COTTON | PCT_MASK_ALPHA)
                        .map(|c| c.with_sphere(0.0, -15.0, -20.0, 30.0, 2).with_sphere(0.0, 0.0, 0.0, 35.0, 17)));
                }
            }
        }
        Class::Fighter => {
            let tex = match g.wing {
                Some(CAPE_OF_OVERRULE) => Some((180.0, 170.0, BITMAP_MANTOE)),
                Some(LITTLE_WARRIORS_CLOAK) => Some((150.0, 130.0, BITMAP_NCCAPE)),
                Some(CAPE_OF_FIGHTER) => Some((180.0, 170.0, BITMAP_NCCAPE)),
                _ => None,
            };
            if let Some((w, h, tex)) = tex {
                add(Cloth::create(pose, 19, [0.0, 15.0, 5.0], 10, 10, w, h, tex, tex,
                    PCT_CURVED | PCT_SHORT_SHOULDER | PCT_HEAVY | PCT_MASK_ALPHA)
                    .map(|c| cape_spheres(c, 35.0, 37.0)));
            }
            if g.wing == Some(CAPE_OF_OVERRULE) {
                for (x, side, r) in [(25.0, PCT_ELASTIC_RAGE_L, 45.0), (-25.0, PCT_ELASTIC_RAGE_R, 50.0)] {
                    add(Cloth::create(pose, 19, [x, 15.0, 2.0], 2, 5, 12.0, 180.0, BITMAP_MANTO01, BITMAP_MANTO01,
                        PCT_FLAT | PCT_SHAPE_NORMAL | PCT_COTTON | side | PCT_MASK_ALPHA)
                        .map(|c| c.with_sphere(0.0, -15.0, -20.0, 35.0, 2).with_sphere(0.0, 0.0, 0.0, r, 17)));
                }
            }
        }
        // `c->Class == CLASS_DARK` (ZzzCharacter.cpp:9005).
        Class::Gladiator => {
            if g.wing == Some(WING_OF_RUIN) {
                add(Cloth::create(pose, 19, [0.0, 15.0, 0.0], 10, 10, 120.0, 120.0, BITMAP_ROBE + 8, BITMAP_ROBE + 8,
                    PCT_CURVED | PCT_SHORT_SHOULDER | PCT_HEAVY | PCT_MASK_ALPHA));
            } else {
                add(Cloth::create(pose, 19, [0.0, 10.0, 0.0], 10, 10, 75.0, 120.0, BITMAP_ROBE, BITMAP_ROBE + 1,
                    PCT_CURVED | PCT_SHORT_SHOULDER | PCT_HEAVY));
            }
        }
        _ => {}
    }
    out
}

/// A character's cloths and what they were built for.
#[derive(Component, Default)]
pub struct Cloak {
    gear: Option<CloakGear>,
    cloths: Vec<Cloth>,
}

/// `(texture, blending, back faces culled)` of a cloth mesh.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct BatchKey {
    texture: i32,
    blend: Blend,
    cull: bool,
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

/// The wind (`CPhysicsManager::s_fWind`), the generator, the last tick stepped and the meshes.
#[derive(Resource)]
pub struct ClothWorld {
    wind: f32,
    rng: Rand,
    last_tick: Option<u64>,
    batches: HashMap<BatchKey, (Handle<Mesh>, Entity)>,
}

impl Default for ClothWorld {
    fn default() -> Self {
        Self { wind: 0.0, rng: Rand(1), last_tick: None, batches: HashMap::new() }
    }
}

/// `CPhysicsManager::Move` (PhysicsManager.cpp:1338-1342): the wind wanders every frame.
fn wander_wind(w: &mut ClothWorld) {
    let plus = ((w.rng.rand() % 200) - 100) as f32 * 0.001;
    w.wind = (w.wind + plus).clamp(-0.2, 1.0);
}

/// `CMapManager::InChaosCastle` (MapManager.cpp:1413-1425): no capes there (ZzzCharacter.cpp:9025-9028).
fn in_chaos_castle(world: i32) -> bool {
    (18..=23).contains(&world) || world == 53
}

/// Builds, steps (once per 25 Hz tick) and draws every character's cloths.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn update_cloth(
    mut commands: Commands,
    args: Res<crate::Args>,
    fixed: Res<Time<Fixed>>,
    mut cw: ResMut<ClothWorld>,
    mut fx: ResMut<crate::fx::FxWorld>,
    mut characters: Query<
        (Entity, &crate::character::Skinned, &Transform, &InheritedVisibility, Option<&mut Cloak>),
        Without<crate::monsters::Monster>,
    >,
    textures: Option<ResMut<crate::fx::textures::FxTextures>>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<FxMaterial>>,
    mut visibility: Query<&mut Visibility>,
) {
    let tick = fixed.elapsed().as_nanos() / fixed.timestep().as_nanos().max(1);
    let tick = tick as u64;
    let steps = cw.last_tick.map_or(0, |last| tick.saturating_sub(last).min(5));
    cw.last_tick = Some(tick);
    let world_time = fixed.elapsed_secs() * 1000.0;
    let world_active = args.world as i32 - 1;
    for _ in 0..steps {
        wander_wind(&mut cw);
    }
    let global_wind = cw.wind;
    let a = fixed.overstep_fraction();
    let mut out: HashMap<BatchKey, Batch> = HashMap::new();

    for (entity, skinned, transform, vis, cloak) in &mut characters {
        let Some((gear, bones)) = skinned.cloak() else { continue };
        let Some(mut cloak) = cloak else {
            commands.entity(entity).insert(Cloak::default());
            continue;
        };
        if !vis.get() || in_chaos_castle(world_active) {
            continue;
        }
        let pose = Pose { transform, bones: &bones };
        if cloak.gear != Some(gear) {
            cloak.cloths = build(&gear, &pose);
            cloak.gear = Some(gear);
        } else if steps > 0 {
            let ctx = StepCtx { pose: &pose, global_wind, world_active, world_time };
            let mut torn = false;
            let cw = &mut *cw;
            for c in &mut cloak.cloths {
                for _ in 0..steps {
                    if !c.step(&ctx, &mut cw.rng) {
                        torn = true;
                        break;
                    }
                }
                if !torn {
                    c.spawn_lights(world_time, &mut fx.sprites);
                }
            }
            // `DeleteCloth` (ZzzCharacter.cpp:9418-9421): built again next frame.
            if torn {
                cloak.gear = None;
                cloak.cloths.clear();
                continue;
            }
        }
        for c in &cloak.cloths {
            let pos = c.drawn(&pose, a);
            c.draw(&pos, world_time, &mut out);
        }
    }

    // Upload into the meshes (creating missing ones); batches with nothing this frame are hidden.
    for (key, (_, e)) in &cw.batches {
        if let Ok(mut v) = visibility.get_mut(*e) {
            v.set_if_neq(if out.contains_key(key) { Visibility::Inherited } else { Visibility::Hidden });
        }
    }
    let Some(mut textures) = textures else { return };
    for (key, b) in out {
        let (mesh, _) = cw.batches.entry(key).or_insert_with(|| {
            let texture = textures.get(key.texture, &mut images).map(|t| t.0);
            let mesh = meshes.add(crate::fx::draw::empty_mesh());
            let material = FxMaterial { cull_back: key.cull, ..FxMaterial::new(texture, key.blend) };
            let e = commands
                .spawn((
                    Name::new(format!("Tela {} {:?}", key.texture, key.blend)),
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(materials.add(material)),
                    Transform::default(),
                    // Rebuilt every frame: no bounding-box culling.
                    bevy::camera::visibility::NoFrustumCulling,
                ))
                .id();
            (mesh, e)
        });
        if let Some(mut m) = meshes.get_mut(&*mesh) {
            let n = b.pos.len();
            m.insert_attribute(Mesh::ATTRIBUTE_POSITION, b.pos);
            m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; n]);
            m.insert_attribute(Mesh::ATTRIBUTE_UV_0, b.uv);
            m.insert_attribute(Mesh::ATTRIBUTE_COLOR, b.color);
            m.insert_indices(bevy::mesh::Indices::U32(b.idx));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bones() -> Vec<Mat34> {
        let id = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 100.0]];
        vec![id; 48]
    }

    fn gear(class: Class, wing: Option<(u8, u16)>, class_armor: bool) -> CloakGear {
        CloakGear { class, wing, class_armor }
    }

    /// Like `player_transform`: MU model → Bevy world, scale 1.
    fn transform() -> Transform {
        Transform {
            rotation: Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
            scale: Vec3::splat(1.0 / MU_SCALE),
            ..default()
        }
    }

    #[test]
    fn cloths_per_class() {
        let (t, b) = (transform(), bones());
        let pose = Pose { transform: &t, bones: &b };
        // Dark Lord: hair x2, + cape, + skirt with its own armor, + 2 strips with the Cape of Emperor.
        assert_eq!(build(&gear(Class::Lord, None, false), &pose).len(), 2);
        assert_eq!(build(&gear(Class::Lord, Some(CAPE_OF_LORD), true), &pose).len(), 4);
        assert_eq!(build(&gear(Class::Lord, Some(CAPE_OF_EMPEROR), true), &pose).len(), 6);
        // Rage Fighter: only with a cape.
        assert_eq!(build(&gear(Class::Fighter, None, true), &pose).len(), 0);
        assert_eq!(build(&gear(Class::Fighter, Some(CAPE_OF_FIGHTER), true), &pose).len(), 1);
        assert_eq!(build(&gear(Class::Fighter, Some(CAPE_OF_OVERRULE), true), &pose).len(), 3);
        // Magic Gladiator: always its robe.
        assert_eq!(build(&gear(Class::Gladiator, None, true), &pose).len(), 1);
        assert_eq!(build(&gear(Class::Knight, Some(CAPE_OF_LORD), true), &pose).len(), 0);
    }

    #[test]
    fn cape_hangs_and_holds() {
        let (t, b) = (transform(), bones());
        assert!(Pose { transform: &t, bones: &b }.angle_z().abs() < 1e-3);
        let pose = Pose { transform: &t, bones: &b };
        let mut cloths = build(&gear(Class::Lord, Some(CAPE_OF_LORD), false), &pose);
        let cape = &mut cloths[2];
        let ctx = StepCtx { pose: &pose, global_wind: 0.0, world_active: 0, world_time: 0.0 };
        let mut rng = Rand(1);
        let top = cape.verts[0].pos.z;
        for _ in 0..200 {
            assert!(cape.step(&ctx, &mut rng), "the cape must not tear when standing still");
        }
        let bottom = cape.verts[cape.verts.len() - 1].pos;
        assert!(bottom.is_finite());
        assert!(bottom.z < top, "the free edge hangs below the fixed one ({} vs {top})", bottom.z);
    }
}
