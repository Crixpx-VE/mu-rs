//! Trails (`JOINT`) from `ZzzEffectJoint.cpp`: `BITMAP_JOINT_FORCE`, `BITMAP_LIGHT`, `BITMAP_PIERCING`,
//! `BITMAP_FLARE_FORCE`, `BITMAP_FLASH`, `BITMAP_DRAIN_LIFE_GHOST`, `BITMAP_PIN_LIGHT`,
//! `BITMAP_FORCEPILLAR`, `BITMAP_SWORDEFF`, `BITMAP_GROUND_WIND` and `BITMAP_LAVA`.
//!
//! Creation = its case in `CreateJoint`; movement = its case in `MoveJoint`.
//!
//! Common notes:
//! - The original does NOT apply the generic −Y·Velocity advance to `JOINT_FORCE` sub 0, `LIGHT`,
//!   `PIERCING`, `FLARE_FORCE` or `FLASH` sub 6; the core does, so those types undo it at the
//!   start of their `step` ([`undo_generic_move`]).
//! - The original's `NumTails` = `tails.len() - 1` (−1 = no tails).
//! - The initial `CreateJoint` tail (`bCreateStartTail`) uses the incoming position and angle,
//!   before the type changes them.
//! - Trails that compute a per-segment luminosity in `DescribeJoint` (`JOINT_FORCE` 0 and 1,
//!   `FLARE_FORCE` 0-4/11-13) keep their "logical" light in `user[0..3]` and set `light` to the
//!   logical light times the segments' average luminosity.

use bevy::math::{Mat3, Vec3};

use crate::fx::core::*;

/// The original's `Q_PI`.
const Q_PI: f32 = 3.141_592_7;

pub fn register(r: &mut Registry) {
    r.joint(BITMAP_JOINT_FORCE, JointKind { create: Some(joint_force_create), step: Some(joint_force_step) });
    r.joint(BITMAP_LIGHT, JointKind { create: Some(light_create), step: Some(light_step) });
    r.joint(BITMAP_PIERCING, JointKind { create: Some(piercing_create), step: Some(piercing_step) });
    r.joint(BITMAP_FLARE_FORCE, JointKind { create: Some(flare_force_create), step: Some(flare_force_step) });
    r.joint(BITMAP_FLASH, JointKind { create: Some(flash_create), step: Some(flash_step) });
    r.joint(BITMAP_DRAIN_LIFE_GHOST, JointKind { create: Some(drain_life_create), step: Some(drain_life_step) });
    r.joint(BITMAP_PIN_LIGHT, JointKind { create: Some(pin_light_create), step: Some(pin_light_step) });
    r.joint(BITMAP_FORCEPILLAR, JointKind { create: Some(force_pillar_create), step: Some(force_pillar_step) });
    r.joint(BITMAP_SWORDEFF, JointKind { create: Some(sword_eff_create), step: Some(sword_eff_step) });
    r.joint(BITMAP_GROUND_WIND, JointKind { create: Some(ground_wind_create), step: Some(ground_wind_step) });
    r.joint(BITMAP_LAVA, JointKind { create: Some(lava_create), step: Some(lava_step) });
}

// --- Helpers ----------------------------------------------------------------------------------------

/// Undoes the generic −Y·Velocity advance the core did before `step` (the original skips it for
/// these types). The angle has not changed yet, so this is exact.
fn undo_generic_move(j: &mut Joint) {
    j.position -= rotate(j.angle, Vec3::new(0.0, -j.velocity, 0.0));
}

/// The original's `NumTails`.
fn num_tails(j: &Joint) -> i32 {
    j.tails.len() as i32 - 1
}

/// Pushes a new tail at the front (like `CreateTail`: at most `MaxTails` tails).
fn push_tail(j: &mut Joint, t: [Vec3; 4]) {
    j.tails.insert(0, t);
    let max = j.max_tails.clamp(1, MAX_TAILS);
    j.tails.truncate(max);
}

/// `CreateTail(o, Matrix)` with an arbitrary matrix (cross on the X and Z axes of `m`).
fn create_tail_m(j: &mut Joint, m: Mat3) {
    let (h, p) = (j.scale * 0.5, j.position);
    push_tail(
        j,
        [
            p + m * Vec3::new(-h, 0.0, 0.0),
            p + m * Vec3::new(h, 0.0, 0.0),
            p + m * Vec3::new(0.0, 0.0, -h),
            p + m * Vec3::new(0.0, 0.0, h),
        ],
    );
}

/// `CreateTailAxis(o, Matrix, 1)` / `FollowTailAxis(o, Matrix, 1)`: cross on the X and Y axes of `m`.
fn create_tail_axis_y(j: &mut Joint, m: Mat3) {
    let (h, p) = (j.scale * 0.5, j.position);
    push_tail(
        j,
        [
            p + m * Vec3::new(-h, 0.0, 0.0),
            p + m * Vec3::new(h, 0.0, 0.0),
            p + m * Vec3::new(0.0, -h, 0.0),
            p + m * Vec3::new(0.0, h, 0.0),
        ],
    );
}

/// "Logical" light stored in `user[0..3]` (what the original keeps in `o->Light`).
fn logic_light(j: &Joint) -> Vec3 {
    Vec3::new(j.user[0], j.user[1], j.user[2])
}

fn set_logic_light(j: &mut Joint, l: Vec3) {
    j.user[0] = l.x;
    j.user[1] = l.y;
    j.user[2] = l.z;
}

/// World rotation of a character bone: what the original uses in
/// `VectorRotate(v, o->Target->BoneTransform[bone])` (there the bones already include the object rotation).
fn bone_rotation(a: &Actor, bone: usize) -> Mat3 {
    let r = a.bones.get(bone).map_or(Mat3::IDENTITY, |m| {
        Mat3::from_cols(
            Vec3::new(m[0][0], m[1][0], m[2][0]),
            Vec3::new(m[0][1], m[1][1], m[2][1]),
            Vec3::new(m[0][2], m[1][2], m[2][2]),
        )
    });
    angle_matrix(a.angle) * r
}

/// `TransformByObjectBone(out, o->Target, bone)`: position of the target's bone. `None` if it no
/// longer exists or is dead (`!o->Target->Live`).
fn target_bone(ctx: &Ctx, t: Option<Target>, bone: usize) -> Option<Vec3> {
    match t? {
        Target::Actor(_) => {
            let a = ctx.actor(t)?;
            a.live.then(|| a.bone_world(bone, Vec3::ZERO))
        }
        // TODO(original): the "dummies" (MODEL_DOWN_ATTACK_DUMMY, MODEL_DRAGON_KICK_DUMMY...) are
        // effects with bones; here we only have their position.
        Target::Effect(_) => ctx.target_position(t),
    }
}

/// Position, angle (if a character) and `Live` of what the trail follows.
fn follow_target(ctx: &Ctx, t: Option<Target>) -> Option<(Vec3, Option<Vec3>, bool)> {
    match t? {
        Target::Actor(_) => ctx.actor(t).map(|a| (a.position, Some(a.angle), a.live)),
        Target::Effect(_) => ctx.target_position(t).map(|p| (p, None, true)),
    }
}

/// `MoveHumming(Position, Angle, TargetPosition, Turn)` (`ZzzAI.cpp`): turns towards the target and
/// returns the distance.
fn move_humming(position: Vec3, angle: &mut Vec3, target: Vec3, turn: f32) -> f32 {
    let target_angle = create_angle(position.x, position.y, target.x, target.y);
    angle.z = turn_angle(angle.z, target_angle, turn);
    let range = position - target;
    let distance = (range.x * range.x + range.y * range.y).sqrt();
    let target_angle = 360.0 - create_angle(position.z, distance, target.z, 0.0);
    angle.x = turn_angle(angle.x, target_angle, turn);
    range.length()
}

/// `CreateJoint(...)` with `PKKey` and `SkillIndex` (which `Ctx::create_joint` does not take).
#[allow(clippy::too_many_arguments)]
fn create_joint_pk(ctx: &mut Ctx, ty: i32, pos: Vec3, target_pos: Vec3, angle: Vec3, sub: i32, scale: f32, pk: i32, skill: i32) {
    let mut nj = Joint::new(ty, pos, target_pos, angle, sub, None, scale);
    nj.pk_key = pk;
    nj.skill = skill;
    ctx.spawn_joint(nj);
}

// --- BITMAP_JOINT_FORCE ------------------------------------------------------------------------------

fn joint_force_create(j: &mut Joint, ctx: &mut Ctx) {
    let start = j.make_tail();
    match j.sub_type {
        // 0, 10 and 8: circle around the start position (tails built by hand in step).
        0 | 10 | 8 => {
            j.life_time = 20.0;
            j.create_tails = false;
            j.velocity = 0.0;
            j.max_tails = 18;
            // NumTails = -1: no tails.
            j.tails.clear();
            j.target_position = j.position;
            j.angle.z += 30.0;
            j.direction = j.angle;
            j.position += rotate(j.angle, Vec3::new(0.0, -180.0, 0.0));
            if j.sub_type == 0 {
                set_logic_light(j, j.light);
                // `DescribeJoint` only draws the face of points 0-1 for subtype 0.
                j.render_face = RENDER_FACE_TWO;
            }
        }
        1 => {
            j.max_tails = 15;
            j.life_time = 30.0;
            j.velocity = 0.0;
            j.only_one_render = 1;
            j.weapon = 0.0;
            j.light = Vec3::ONE;
            j.direction = Vec3::ZERO;
            set_logic_light(j, j.light);
            match follow_target(ctx, j.target) {
                Some((pos, angle, _)) => {
                    j.position = pos;
                    if let Some(a) = angle {
                        j.angle = a;
                    }
                }
                None => j.live = false,
            }
            j.tails = vec![start];
        }
        2..=6 => {
            j.life_time = 20.0;
            j.velocity = 8.0;
            j.direction.x = 3.5;
            j.direction.z = 1.0;
            j.max_tails = 12;
            j.multi_use = 5.0;
            match j.sub_type {
                3 => {
                    j.tex_type = BITMAP_FLARE;
                    j.multi_use = 10.0;
                    j.velocity = 10.0;
                    j.direction.z = 2.0;
                    j.reverse_uv = 0;
                    j.direction.x = 10.5;
                }
                4 => {
                    j.life_time = 20.0;
                    j.max_tails = 13;
                    j.tex_type = BITMAP_HOLE;
                    j.direction.x = 15.5;
                    j.direction.z = 3.0;
                    j.multi_use = 10.0;
                }
                5 => j.tex_type = BITMAP_LAVA,
                6 => {
                    j.tex_type = BITMAP_LAVA;
                    j.velocity = 20.0;
                    j.life_time = 16.0;
                }
                _ => {
                    // Subtype 2.
                    j.position.z += 10.0;
                    j.tex_type = BITMAP_INFERNO;
                    j.life_time = 15.0;
                    j.multi_use = 5.0;
                    j.direction.x = 5.0;
                    j.direction.z = 5.0;
                }
            }
            j.head_angle = Vec3::new(0.0, 0.0, j.angle.z);
            j.start_position = j.position;
            start_without_tails(j);
        }
        7 | 20 => {
            j.direction.x = 3.5;
            j.direction.z = 1.0;
            j.max_tails = if j.sub_type == 7 { 13 } else { 15 };
            j.multi_use = 5.0;
            j.tex_type = BITMAP_INFERNO;
            j.velocity = 10.0;
            j.life_time = 20.0;
            start_without_tails(j);
        }
        _ => j.tails = vec![start],
    }
}

/// `NumTails = -1` with `m_bCreateTails = true`: no initial tail. Core tails are disabled until
/// the first `step` (otherwise the core would add an initial tail); `user[3]` = pending.
fn start_without_tails(j: &mut Joint) {
    j.tails.clear();
    j.create_tails = false;
    j.user[3] = 1.0;
}

fn joint_force_step(j: &mut Joint, ctx: &mut Ctx) {
    if j.user[3] != 0.0 {
        j.user[3] = 0.0;
        j.create_tails = true;
    }
    match j.sub_type {
        0 | 10 | 8 => {
            // Subtype 0: the original skips the generic advance (with Velocity = 0 nothing changes).
            if j.sub_type == 0 {
                undo_generic_move(j);
            }
            let p = Vec3::new(0.0, -145.0, 0.0);
            for k in 0..8 {
                if num_tails(j) < j.max_tails as i32 - 1 {
                    let prev = j.position;
                    j.position = j.target_position + rotate(j.direction, p);
                    // CreateTail(o, AngleMatrix(o->Angle)).
                    j.follow_tail();
                    j.direction.z -= 11.0;
                    if j.sub_type == 8 {
                        let s = (ctx.rand() % 10) as f32 + 5.0;
                        create_joint_pk(ctx, BITMAP_JOINT_THUNDER, j.position, j.position, j.angle, 3, s, 5, 10);
                        let s = (ctx.rand() % 8) as f32 + 4.0;
                        create_joint_pk(ctx, BITMAP_JOINT_THUNDER, j.position, j.position, j.angle, 3, s, 5, 10);
                    } else {
                        let light = if j.sub_type == 0 { logic_light(j) } else { j.light };
                        if ctx.rand_fps_check(2) {
                            ctx.create_particle(BITMAP_FIRE, j.position, j.angle, light, 0, 1.0, None);
                        }
                        let s = (ctx.rand() % 10) as f32 + 5.0;
                        create_joint_pk(ctx, BITMAP_JOINT_THUNDER, prev, j.position, j.angle, 3, s, 5, 10);
                        let s = (ctx.rand() % 8) as f32 + 4.0;
                        create_joint_pk(ctx, BITMAP_JOINT_THUNDER, prev, j.position, j.angle, 3, s, 5, 10);
                    }
                }
                // Subtypes 0 and 10 on the hero, LifeTime > 18, laps 0 and 5 (MuMain
                // ZzzEffectJoint.cpp:6334-6352, 6406-6427; `SV_RANGE_ATTACK_CHECK` is not defined).
                // TODO(original): in Castle Siege during the battle, in a `TW_NOATTACKZONE` tile,
                // `Velocity = 0`, `LifeTime /= 5` and the loop breaks first (no siege state yet).
                if matches!(j.sub_type, 0 | 10) && ctx.is_hero(j.target) && j.life_time > 18.0 && k % 5 == 0 {
                    let range = if j.sub_type == 0 { 150.0 } else { 225.0 };
                    ctx.attack_character_range(j.skill, j.position, range, j.weapon as i32, j.pk_key);
                }
            }
            match j.sub_type {
                10 => {
                    // TODO(original): with m_bySkillSerialNum != 0 it only divides the light by 1.5
                    // if LifeTime < 10 (assumed 0 here).
                    let lum = if j.life_time < 10.0 { j.light.x / 1.5 } else { j.life_time / 30.0 };
                    j.light = Vec3::splat(lum);
                }
                0 => {
                    let lum = if j.life_time < 10.0 { j.user[0] / 1.5 } else { j.life_time / 30.0 };
                    set_logic_light(j, Vec3::splat(lum));
                    // DescribeJoint: (MaxTails − j) / MaxTails · 2 · Light[0] per segment → average.
                    let n = num_tails(j).max(0) as f32;
                    let max = j.max_tails as f32;
                    let avg = if n > 0.0 { 2.0 * (max - (n - 1.0) * 0.5) / max } else { 0.0 };
                    j.light = Vec3::splat(lum * avg);
                }
                _ => {}
            }
        }
        1 => {
            let Some((pos, _, live)) = follow_target(ctx, j.target) else {
                j.live = false;
                return;
            };
            j.live = live;
            j.position = pos;
            j.scale -= 1.0;
            j.position.z += 100.0;
            let mut l = logic_light(j);
            if j.life_time < 15.0 {
                l *= 1.0 / 1.3;
            }
            set_logic_light(j, l);
            // DescribeJoint: (1 − (NumTails − j) / NumTails) · 2 per segment; average = (N − 1) / N
            // with the N there will be after the core's FollowTail.
            let n = (j.tails.len() + 1).min(j.max_tails.clamp(1, MAX_TAILS)) as f32 - 1.0;
            let avg = if n > 0.0 { (n - 1.0) / n } else { 0.0 };
            j.light = l * avg;
        }
        2..=6 => {
            j.velocity += j.direction.z;
            if j.life_time < 15.0 {
                j.direction.z += 0.5;
            } else {
                j.direction.z += j.direction.x;
            }
            if j.sub_type != 4 && num_tails(j) >= j.max_tails as i32 - 1 {
                j.create_tails = false;
            }
            if j.life_time < j.multi_use {
                j.light *= 1.0 / 1.3;
            }
            if (j.life_time as i32) % 5 == 0 && j.sub_type == 2 {
                ctx.create_effect(MODEL_SKILL_INFERNO, j.start_position, j.head_angle, j.light, 6, None, 20, 0, 0, 0.0);
            } else if j.sub_type == 4 {
                let light = Vec3::new(0.1, 0.6, 1.0);
                ctx.create_joint(BITMAP_FLARE_BLUE, j.position, j.position, j.angle, 6, None, 30.0);
                ctx.create_sprite(BITMAP_LIGHT, j.position, 1.6, light, 0.0, 0);
                ctx.create_sprite(BITMAP_SHINY + 1, j.position, 1.5, light, ctx.world_time * 0.1, 0);
            }
        }
        7 | 20 => {
            j.velocity += j.direction.z;
            if j.life_time < 20.0 {
                j.direction.z += 0.5;
            } else {
                j.direction.z += j.direction.x;
            }
            if num_tails(j) >= j.max_tails as i32 - 1 {
                j.create_tails = false;
            }
            if j.life_time < j.multi_use {
                j.light *= 1.0 / 1.3;
            }
            j.position.z = ctx.terrain_height(j.position.x, j.position.y) + 3.0;
        }
        _ => {}
    }
}

// --- BITMAP_LIGHT ------------------------------------------------------------------------------------

fn light_create(j: &mut Joint, ctx: &mut Ctx) {
    let start = j.make_tail();
    j.create_tails = false;
    match j.sub_type {
        0 => {
            j.life_time = 20.0;
            j.max_tails = 30;
            j.velocity = (ctx.rand() % 10) as f32 + 10.0;
            let ax = -((ctx.rand() % 40) as f32) + 30.0;
            let az = (ctx.rand() % 360) as f32;
            j.angle = Vec3::new(ax, 0.0, az);
            j.direction = Vec3::ZERO;
            j.light = Vec3::new(0.1, 0.5, 1.0);
            j.start_position = j.position;
        }
        1 => {
            j.life_time = 10.0;
            j.max_tails = 20;
            j.skill = ctx.rand() % 5;
            j.velocity = (ctx.rand() % 5) as f32 + 1.0;
            j.angle = Vec3::new(30.0, 0.0, (ctx.rand() % 360) as f32);
            j.direction = Vec3::ZERO;
            j.light = Vec3::new(1.0, 0.8, 0.6);
            j.start_position = j.position;
        }
        _ => {}
    }
    j.tails = vec![start];
}

fn light_step(j: &mut Joint, _ctx: &mut Ctx) {
    undo_generic_move(j);
    match j.sub_type {
        0 => {
            if j.life_time > 16.0 {
                for _ in 0..10 {
                    j.follow_tail();
                    j.position += rotate(j.angle, Vec3::new(0.0, -j.velocity, 0.0));
                }
            }
        }
        1 => {
            if j.skill == 0 || j.life_time > 3.0 {
                for _ in 0..5 {
                    j.follow_tail();
                    j.position += rotate(j.angle, Vec3::new(0.0, -j.velocity, 0.0));
                    j.velocity += 0.5;
                }
            }
        }
        _ => {}
    }
    j.light *= 1.0 / 1.2;
}

// --- BITMAP_PIERCING ---------------------------------------------------------------------------------

fn piercing_create(j: &mut Joint, ctx: &mut Ctx) {
    let start = j.make_tail();
    j.create_tails = false;
    if j.sub_type == 0 || j.sub_type == 1 {
        j.life_time = 10.0;
        if j.sub_type == 1 {
            j.tile_mapping = true;
            j.scroll = JointScroll::Tile;
        }
        j.max_tails = 30;
        j.velocity = 20.0;
        j.angle = Vec3::new(-90.0, 0.0, j.angle.z);
        j.direction = Vec3::ZERO;
        j.light = if ctx.rand() % 2 == 0 { Vec3::new(1.0, 0.8, 0.6) } else { Vec3::ONE };
        j.start_position = j.position;
    }
    j.tails = vec![start];
}

fn piercing_step(j: &mut Joint, _ctx: &mut Ctx) {
    undo_generic_move(j);
    if j.sub_type == 0 || j.sub_type == 1 {
        if j.life_time > 9.0 {
            for _ in 0..30 {
                j.follow_tail();
                j.position += rotate(j.angle, Vec3::new(0.0, 0.0, -j.velocity));
            }
        }
        j.velocity -= 2.0;
        j.light *= 1.0 / 1.4;
    }
}

// --- BITMAP_FLARE_FORCE ------------------------------------------------------------------------------

/// Subtypes with the per-segment luminosity and UV scroll of `DescribeJoint`.
fn flare_force_lit(sub: i32) -> bool {
    (0..=4).contains(&sub) || (11..=13).contains(&sub)
}

fn flare_force_create(j: &mut Joint, ctx: &mut Ctx) {
    let start = j.make_tail();
    // `o->Skill = SkillIndex` in the common part: the creator puts the index in `j.skill`.
    let skill_index = j.skill;
    j.life_time = 20.0;
    j.create_tails = false;
    j.max_tails = 30;
    j.multi_use = 1.0;
    j.velocity = -3.0;
    j.position.z += 150.0;
    j.direction = Vec3::ZERO;
    j.target_position = Vec3::ZERO;
    j.light = Vec3::new(1.0, 0.8, 1.0);
    j.start_position = j.position;
    j.weapon = 0.0;

    let sub = j.sub_type;
    if (1..=4).contains(&sub) || (11..=13).contains(&sub) {
        j.tex_type = BITMAP_JOINT_THUNDER;
        j.velocity = -3.0;
        j.target_position.x = 80.0;
        j.target_position.y = 180.0;
        j.weapon = (ctx.rand() % 3 + 2) as f32;
        if sub == 2 || sub == 4 || (11..=13).contains(&sub) {
            match sub {
                11 => j.light = Vec3::new(0.7, 1.0, 0.7),
                12 => j.light = Vec3::new(1.0, 0.6, 0.6),
                13 => j.light = Vec3::new(0.7, 0.7, 1.0),
                _ => {}
            }
            j.target_position.y = -180.0;
        }
        if sub == 3 || sub == 4 {
            j.weapon = 0.0;
        }
        j.life_time += j.weapon;
        j.position = j.start_position + rotate(j.target_position, Vec3::new(0.0, 0.0, j.target_position.x));
    } else if sub != 0 {
        j.life_time = 15.0;
        j.skill = 0;
        j.max_tails = 1;
        j.weapon = 30.0;
        // Character bone it comes out of (5-7).
        j.multi_use = skill_index as f32;
        j.velocity = 0.0;
        j.target_position.x = 30.0;
        j.target_position.z = (sub * 90) as f32;
        j.light = Vec3::new(1.0, 0.8, 1.0);
        j.direction = Vec3::new(0.0, -4.0, 0.0);
        j.start_position = j.position;
        if j.pk_key != -1 {
            j.direction.y = 0.0;
        }
    }

    if (5..=7).contains(&sub) {
        j.tex_type = BITMAP_FIRECRACKER;
    } else if sub == 0 {
        j.tex_type = BITMAP_JOINT_THUNDER;
    }
    if flare_force_lit(sub) {
        j.scroll = JointScroll::Flare;
        set_logic_light(j, j.light);
    }
    // Subtypes 5-7 have no initial tail.
    if !(5..=7).contains(&sub) {
        j.tails = vec![start];
    }
}

fn flare_force_step(j: &mut Joint, ctx: &mut Ctx) {
    undo_generic_move(j);
    let sub = j.sub_type;
    if flare_force_lit(sub) {
        j.light = logic_light(j);
    }
    if (5..=7).contains(&sub) {
        let actor = ctx.actor(j.target).cloned();
        if let Some(a) = actor {
            let bone = j.multi_use as usize;
            let mut direction = j.direction;
            j.start_position = a.bone_world(bone, Vec3::new(0.0, 20.0, 0.0));
            j.position = j.start_position;

            // NumTails = 0: all tails are rebuilt every tick.
            j.tails.clear();
            let max_tails = (j.max_tails as i32).min(j.weapon as i32).max(0);
            j.max_tails = max_tails as usize;
            j.target_position.y = j.target_position.z;
            let rot_bone = bone_rotation(&a, bone);
            let rot_bone0 = bone_rotation(&a, 0);

            for i in 0..max_tails {
                j.start_position += rot_bone * direction;
                if sub % 2 != 0 {
                    j.target_position.y += 40.0;
                } else {
                    j.target_position.y -= 40.0;
                }
                let angle = Vec3::new(j.target_position.y, 0.0, 0.0);
                j.position = j.start_position + rotate(angle, Vec3::new(0.0, 0.0, j.target_position.x));

                // CreateTail(o, o->Target->BoneTransform[0]).
                create_tail_m(j, rot_bone0);

                j.target_position.x -= 0.15;
                if j.pk_key == -1 {
                    direction.y -= 0.1;
                }
                if i % 2 == 0 {
                    ctx.create_sprite(BITMAP_FLARE, j.start_position, j.light.x / 2.0, j.light, 0.0, 0);
                }
            }
            j.max_tails += 1;
        } else {
            j.live = false;
        }
        if j.life_time < 7.0 {
            j.light *= 1.0 / 1.5;
        }
    } else if num_tails(j) < j.max_tails as i32 - 1 {
        if sub == 0 {
            let mut i = 1;
            while (i as f32) < j.multi_use {
                j.follow_tail();
                j.start_position += rotate(j.angle, Vec3::new(0.0, j.velocity, 0.0));
                j.position = j.start_position;
                j.velocity -= 2.0;
                i += 1;
            }
            j.multi_use += 2.0;
        } else if (1..=4).contains(&sub) || (11..=13).contains(&sub) {
            if j.weapon <= 0.0 {
                let mut i = 1;
                while (i as f32) < j.multi_use {
                    j.follow_tail();
                    j.start_position += rotate(j.angle, Vec3::new(0.0, j.velocity, 0.0));
                    // (Both branches of the original subtract the same.)
                    j.target_position.y -= 20.0;
                    let angle = Vec3::new(0.0, j.target_position.y, j.angle.z);
                    let off = rotate(angle, Vec3::new(0.0, 0.0, j.target_position.x));
                    j.position = if sub == 3 || sub == 4 { j.start_position - off } else { j.start_position + off };
                    j.velocity -= 2.0;
                    j.target_position.x -= 2.5;
                    i += 1;
                }
                j.multi_use += 2.0;
            } else {
                j.weapon -= 1.0;
            }
        }
    }

    if flare_force_lit(sub) {
        if j.life_time < 10.0 {
            j.light *= 1.0 / 1.3;
        }
        set_logic_light(j, j.light);
        // DescribeJoint: (NumTails − 1 − j) / MaxTails · 2 per segment; average = (N − 1) / MaxTails.
        let n = num_tails(j).max(0) as f32;
        let avg = if n > 0.0 { (n - 1.0) / j.max_tails as f32 } else { 0.0 };
        j.light *= avg;
    }
}

// --- BITMAP_FLASH ------------------------------------------------------------------------------------

fn flash_create(j: &mut Joint, ctx: &mut Ctx) {
    let start = j.make_tail();
    let sub = j.sub_type;
    if sub <= 3 || sub == 5 {
        j.max_tails = 10;
        j.life_time = 40.0;
        j.velocity = 70.0;
        j.reverse_uv = (ctx.rand() % 2) as u8;
        j.multi_use = 150.0;
        j.pk_key = 0;
        if sub == 2 {
            j.velocity = 30.0;
            j.multi_use = 300.0;
            j.tex_type = BITMAP_FLARE_FORCE;
        } else if sub == 3 {
            j.velocity = 30.0;
            j.multi_use = 300.0;
            j.tex_type = BITMAP_FLARE_BLUE;
        }
        if sub == 5 {
            j.tex_type = BITMAP_FLARE;
            j.max_tails = 15;
            j.velocity = (ctx.rand() % 20) as f32 + 10.0;
            j.reverse_uv = 2;
        } else {
            j.angle = Vec3::new(90.0, 0.0, 0.0);
        }
        j.head_angle = j.angle;
        // `m_bCreateTails` in user[0]: the original lays two tails per tick (one with Angle and one
        // with HeadAngle), so tails are built by hand in step.
        j.user[0] = 1.0;
        j.tails = vec![start];
    } else if sub == 4 {
        j.max_tails = 20;
        j.life_time = 30.0;
        j.velocity = 0.0;
        j.only_one_render = 1;
        j.weapon = 0.0;
        j.tex_type = BITMAP_FLARE_BLUE;
        j.light = Vec3::ONE;
        j.direction = Vec3::ZERO;
        match follow_target(ctx, j.target) {
            Some((pos, angle, _)) => {
                j.position = pos;
                if let Some(a) = angle {
                    j.angle = a;
                }
            }
            None => j.live = false,
        }
        j.tails = vec![start];
    } else if sub == 6 {
        j.max_tails = 30;
        j.life_time = 25.0;
        j.weapon = j.life_time;
        j.velocity = (70 + ctx.rand() % 3) as f32 * Q_PI / 180.0;
        j.light = Vec3::new(0.8, 0.8, 1.0);
        j.direction = Vec3::new(0.0, -(40.0 + (ctx.rand() % 4) as f32), j.velocity.sin() * 10.0);
        j.position.y -= 20.0;
        j.position.z += 130.0;
        j.start_position = j.position;
        j.create_tails = false;
        j.reverse_uv = 3;
        // No initial tail; CreateTailAxis + NumTails = 0 → a single tail.
        j.tails.clear();
        create_tail_axis_y(j, angle_matrix(j.angle));
    } else if sub == 7 {
        // Spawns three subtype 6 bolts and dies.
        j.create_tails = false;
        j.life_time = 0.0;
        for i in 0..3 {
            let off = rotate(j.angle, Vec3::new((i - 1) as f32 * 100.0, -50.0, 0.0));
            let pos = off + j.position;
            ctx.create_joint(BITMAP_FLASH, pos, pos, j.angle, 6, None, 40.0);
        }
        j.tails = vec![start];
    } else {
        j.tails = vec![start];
    }
}

fn flash_step(j: &mut Joint, ctx: &mut Ctx) {
    let sub = j.sub_type;
    if sub <= 3 || sub == 5 {
        // This step builds the tails (user[0] = m_bCreateTails).
        j.create_tails = false;
        let mut lay = j.user[0] != 0.0;
        if sub == 2 {
            j.angle.z += (ctx.rand() % 10) as f32 + 10.0;
        }
        j.velocity += 10.0;
        if lay {
            // FollowTail(o, AngleMatrix(o->Angle)).
            j.follow_tail();
        }
        j.velocity += 10.0;
        j.position += rotate(j.head_angle, Vec3::new(0.0, -j.velocity, 0.0));

        let height = ctx.terrain_height(j.position.x, j.position.y) - j.multi_use;
        if j.position.z < height {
            j.position.z = height;
            if lay && j.pk_key != 1 {
                if sub < 2 {
                    // TODO(original): with Target, animates MODEL_SHADOW_BODY with the target's angle
                    // and SpawnMeshEffects(0, MODEL_SKIN_SHELL[, 1]) (skin shells over the mesh).
                } else {
                    let light = if sub == 5 { Vec3::new(1.0, 0.8, 0.3) } else { Vec3::new(0.3, 0.8, 1.0) };
                    ctx.create_effect(BITMAP_SHOCK_WAVE, j.position, j.angle, light, 4, None, -1, 0, 0, 0.0);
                }
            }
            if sub == 5 {
                j.pk_key = 1;
                j.velocity = -20.0;
            } else {
                lay = false;
            }
            j.light *= 1.0 / 1.2;
        }
        let rot = (ctx.rand() % 360) as f32;
        if sub < 2 {
            ctx.create_sprite(BITMAP_FLARE, j.position, 1.0, j.light, rot, 0);
        } else if sub == 5 {
            ctx.create_sprite(BITMAP_FLARE, j.position, 2.5, j.light, rot, 0);
        } else {
            ctx.create_sprite(BITMAP_SHINY + 2, j.position, 1.6, Vec3::new(0.0, 1.0, 1.0), rot, 0);
        }
        // Final MoveJoint FollowTail with the last matrix (the HeadAngle one).
        if lay {
            create_tail_m(j, angle_matrix(j.head_angle));
        }
        j.user[0] = if lay { 1.0 } else { 0.0 };
    } else if sub == 4 {
        let Some((pos, _, live)) = follow_target(ctx, j.target) else {
            j.live = false;
            return;
        };
        j.live = live;
        j.position = pos;
        if j.life_time < 15.0 {
            j.light *= 1.0 / 1.3;
        }
    } else if sub == 6 {
        undo_generic_move(j);
        let fr = ctx.rand() % 5 - 2;
        j.direction.y += fr as f32;
        j.direction.z -= 10.0;
        let m = angle_matrix(j.angle);
        j.position += m * j.direction;

        if (j.weapon - j.life_time) < 10.0 {
            ctx.create_particle(BITMAP_EXPLOTION + 1, j.start_position, j.angle, j.light, 0, 3.0, None);
            j.scale = 100.0;
        } else {
            j.scale = 40.0;
        }

        let height = ctx.terrain_height(j.position.x, j.position.y);
        if j.position.z <= height {
            let mut t_pos = j.position;
            ctx.create_joint(BITMAP_JOINT_THUNDER, j.position, j.position, j.angle, 4, None, 60.0);
            if j.direction.z <= 0.0 {
                j.direction.z = j.velocity.sin() * 20.0;
            }
            j.position.z = height + 5.0;
            t_pos.z = j.position.z + 30.0;
            ctx.create_particle(BITMAP_ADV_SMOKE + 1, t_pos, j.angle, j.light, 2, 3.0, None);
        }

        // FollowTailAxis(o, Matrix, 1).
        create_tail_axis_y(j, m);

        if j.life_time < j.weapon / 2.0 {
            j.light *= 1.0 / 1.3;
        }
    }
}

// --- BITMAP_DRAIN_LIFE_GHOST -------------------------------------------------------------------------

fn drain_life_create(j: &mut Joint, ctx: &mut Ctx) {
    if j.sub_type == 0 {
        j.render_type = JointRender::AlphaBlend;
        if ctx.rand() % 2 == 0 {
            j.angle.z += 90.0;
        } else {
            j.angle.z -= 90.0;
        }
        j.angle.x += (ctx.rand() % 100 - 50) as f32;
        j.angle.y += (ctx.rand() % 100 - 50) as f32;
        j.angle.z += (ctx.rand() % 100 - 50) as f32;
        j.velocity = 1.0 + (ctx.rand() % 10) as f32 * 0.2;
        j.life_time = (30 + (ctx.rand() % 20 - 10)) as f32;
        j.scale += (ctx.rand() % 60 - 30) as f32;
        j.max_tails = (20 + (ctx.rand() % 10 - 5)) as usize;
    }
}

fn drain_life_step(j: &mut Joint, _ctx: &mut Ctx) {
    if j.sub_type == 0 {
        // TODO(original): the destination is `o->Target->Owner->Position` (with TargetPosition's Z);
        // without the target's owner it heads to the TargetPosition it was created with.
        let target = j.target_position;
        let mut angle = j.angle;
        move_humming(j.position, &mut angle, target, j.velocity);
        j.angle = angle;
        j.velocity += 2.0;
        if j.life_time < 10.0 {
            j.light *= 1.0 / 1.2;
        }
    }
}

// --- BITMAP_PIN_LIGHT --------------------------------------------------------------------------------

fn pin_light_create(j: &mut Joint, ctx: &mut Ctx) {
    j.velocity = (ctx.rand() % 60 + 15) as f32;
    j.life_time = (ctx.rand() % 3 + 1) as f32;
    j.max_tails = 5;
}

fn pin_light_step(j: &mut Joint, _ctx: &mut Ctx) {
    j.velocity += 0.1;
    j.light *= 0.9;
}

// --- BITMAP_FORCEPILLAR / BITMAP_SWORDEFF / BITMAP_GROUND_WIND / BITMAP_LAVA -------------------------

fn force_pillar_create(j: &mut Joint, _ctx: &mut Ctx) {
    j.render_type = JointRender::AlphaBlend;
    j.render_face = RENDER_FACE_TWO;
    j.life_time = 7.0;
    j.max_tails = 5;
    j.create_tails = true;
    j.light = Vec3::new(0.95, 0.72, 0.48);
}

fn force_pillar_step(j: &mut Joint, ctx: &mut Ctx) {
    // TODO(original): also dies if the target is not MODEL_DOWN_ATTACK_DUMMY_L/_R, and re-animates
    // its model before reading the bone.
    match target_bone(ctx, j.target, 0) {
        Some(p) => j.position = p,
        None => j.live = false,
    }
}

fn sword_eff_create(j: &mut Joint, _ctx: &mut Ctx) {
    j.render_type = JointRender::AlphaBlend;
    j.render_face = RENDER_FACE_TWO;
    j.life_time = 10.0;
    j.max_tails = 50;
    j.create_tails = true;
    j.light = Vec3::new(0.6, 0.6, 1.0);
}

fn sword_eff_step(j: &mut Joint, ctx: &mut Ctx) {
    match target_bone(ctx, j.target, 5) {
        Some(p) => j.position = p,
        None => j.live = false,
    }
}

fn ground_wind_create(j: &mut Joint, _ctx: &mut Ctx) {
    j.render_type = JointRender::AlphaBlend;
    j.render_face = RENDER_FACE_TWO;
    j.life_time = 10.0;
    j.max_tails = 8;
    j.create_tails = true;
    j.light = Vec3::new(0.7, 0.7, 1.0);
}

fn ground_wind_step(j: &mut Joint, ctx: &mut Ctx) {
    // TODO(original): dies if the target is not MODEL_DRAGON_KICK_DUMMY, and animates its model at
    // frame AnimationFrame + SubType · AlphaTarget before reading bone 2.
    match target_bone(ctx, j.target, 2) {
        Some(p) => j.position = p,
        None => {
            j.live = false;
            return;
        }
    }
    j.position.z += 20.0;
    j.light *= 0.7;
}

fn lava_create(j: &mut Joint, _ctx: &mut Ctx) {
    j.render_type = JointRender::AlphaBlend;
    j.render_face = RENDER_FACE_TWO;
    j.life_time = 10.0;
    j.max_tails = 10;
    j.create_tails = true;
    j.light = Vec3::ONE;
    // TODO(original): Velocity = PlaySpeed of the target model's current action (only used to
    // delay the animation frame; the position is read from the bone each tick).
    j.velocity = 0.0;
}

fn lava_step(j: &mut Joint, ctx: &mut Ctx) {
    // TODO(original): animates the target model at AnimationFrame − ((Velocity·10 − (SubType % 7))
    // · 0.1) before reading the bone (each subtype lags slightly behind).
    let bone = if j.sub_type >= 7 { 28 } else { 36 };
    match target_bone(ctx, j.target, bone) {
        Some(p) => j.position = p,
        None => {
            j.live = false;
            return;
        }
    }
    j.light *= 0.8;
}
