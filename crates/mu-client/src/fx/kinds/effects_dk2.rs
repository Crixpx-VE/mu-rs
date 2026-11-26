//! DK effects (2): Rageful Blow (`MODEL_SKILL_FURY_STRIKE` and its +1..+8: the weapon thrown in
//! an arc, the impact with waves, rings and cracks), `MODEL_WAVE` (flashing.bmd), `MODEL_TAIL`
//! (tail.bmd) and the bouncing stones (`MODEL_STONE1`/`MODEL_STONE2`).
//!
//! Ported from MuMain `ZzzEffect.cpp` (`CreateEffect` ≈1490-1566 and ≈2192-2324, `MoveEffect`
//! ≈6261-6466 and the common tail ≈7646-7711, `RenderFuryStrike` ≈7788) and checked against Main 5.2
//! (cited per function), `Behaviors/MoveHandlers.cpp`
//! (`Move_MODEL_SKILL_FURY_STRIKE`, `Move_MODEL_WAVE`, `Move_MODEL_TAIL`, `Move_MODEL_ICE_SMALL`) and
//! `Data/Effects/EffectTypes.json` (creation of `MODEL_WAVE` and `MODEL_TAIL`).
//!
//! `FPS_ANIMATION_FACTOR` = 1. Since every type has a `step`, the core does not run the common
//! `MoveEffect` tail (`PlayAnimation` + `MoveParticle(o, true)`): `common::move_tail` does it where
//! the original does. `MoveEffect` always draws one `rand()` for `Luminosity` at the start.
//! Main 5.2 line numbers are cited as `ZzzEffect.cpp:NNNN`; where it differs from MuMain, Main 5.2
//! is followed.

use bevy::math::Vec3;

use super::common::{self, luminosity, move_tail, rf};
use crate::fx::core::*;

/// Tile attributes (`_define.h`).
const TW_NOMOVE: u16 = 0x0004;
const TW_NOGROUND: u16 = 0x0008;
const TW_WATER: u16 = 0x0010;

pub fn register(r: &mut Registry) {
    let k = |create: EffectCreate, step: EffectStep, draw: Option<EffectDraw>| EffectKind { create: Some(create), step: Some(step), draw };
    r.effect(MODEL_SKILL_FURY_STRIKE, k(create_fury, step_fury, Some(draw_fury)));
    for n in 1..=8 {
        r.effect(MODEL_SKILL_FURY_STRIKE + n, k(create_fury_part, step_fury_part, Some(draw_fury_part)));
    }
    r.effect(MODEL_WAVE, k(create_wave, step_wave, None));
    r.effect(MODEL_TAIL, k(create_tail, step_tail, None));
    r.effect(MODEL_STONE1, k(create_stone, step_stone, None));
    r.effect(MODEL_STONE2, k(create_stone, step_stone, None));
}

// --- Helpers ----------------------------------------------------------------------------------------

/// `o->Owner->Type == MODEL_MONSTER01+95` (`MODEL_WEREWOLF_HERO`, the Kanturu werewolf hero).
fn owner_is_werewolf(e: &Effect, ctx: &Ctx) -> bool {
    ctx.actor(e.owner).is_some_and(|a| a.ty == MODEL_WEREWOLF_HERO)
}

/// Crack light (ZzzEffect.cpp:10289-10296): red, blue if the owner is `MODEL_MONSTER01+95`, and the
/// `Light` (1, 1, 1) set at the start of `MoveEffect` without an owner. (The owner of `+2` is the
/// `MODEL_SKILL_FURY_STRIKE` effect itself, so it is always red.)
fn crack_light(e: &Effect, ctx: &Ctx, lum: f32) -> Vec3 {
    if e.owner.is_some() {
        if owner_is_werewolf(e, ctx) { Vec3::new(0.0, 0.0, lum) } else { Vec3::new(lum, 0.0, 0.0) }
    } else {
        Vec3::ONE
    }
}

/// Stone bouncing out of a crack (`+2` and `+5`).
fn spawn_crack_stone(e: &Effect, ctx: &mut Ctx) {
    let p = Vec3::new(0.0, rf(ctx, 150), 0.0);
    let angle = Vec3::new(0.0, 0.0, rf(ctx, 360));
    let pos = rotate(angle, p) + e.position;
    let ty = MODEL_STONE1 + ctx.rand() % 2;
    ctx.create_effect(ty, pos, e.angle, e.light, 0, None, -1, 0, 0, 0.0);
}

// --- MODEL_SKILL_FURY_STRIKE: the weapon in an arc --------------------------------------------------

/// ZzzEffect.cpp:2047-2061.
fn create_fury(e: &mut Effect, ctx: &mut Ctx) {
    e.head_angle = e.angle;
    e.start_position = e.position;
    e.life_time = 20.0;
    e.sub_type = ctx.rand() % 100;
    e.angle.z += 330.0;
    e.head_angle.x += 80.0;
    e.head_angle.z += 180.0;
    e.gravity = 50.0;
    e.weapon = ctx.next_packet_serial();
}

/// ZzzEffect.cpp:10016-10241.
fn step_fury(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    let light = Vec3::ONE;
    let mut angle = Vec3::ZERO;
    let mut scale: i32 = 150;
    let lt = e.life_time as i32;
    let owner_angle = ctx.actor(e.owner).map(|a| a.angle);
    let hellas = common::in_hellas(ctx.world_active);

    if lt == 11 {
        // The hit: explosion, sparks, wave and cracks.
        let light1 = Vec3::ONE;
        if e.kind != 3
            && let Some(oa) = owner_angle
        {
            e.start_position = rotate(oa, Vec3::new(-25.0, -80.0, 0.0)) + e.position;
        }
        let mut add_height = 25.0;
        if hellas {
            if e.kind == 0 || e.kind == 2 {
                // TODO(original): AddWaterWave(x / TERRAIN_SCALE, y / TERRAIN_SCALE, 2, -1000).
            } else {
                add_height = 100.0;
            }
        }
        e.start_position.z = ctx.terrain_height(e.start_position.x, e.start_position.y) + add_height;
        add_height = 3.0;

        ctx.create_particle(BITMAP_EXPLOTION, e.start_position, angle, light1, 0, 0.5, None);

        if e.kind == 0 {
            for j in 0..8 {
                let ax = (ctx.rand() % 60) as f32 - 60.0;
                let az = (ctx.rand() % 30 + 90) as f32;
                angle = Vec3::new(ax, 0.0, az) + e.angle;
                let mut pos = e.start_position;
                pos.x += (ctx.rand() % 20 - 10) as f32;
                pos.y += (ctx.rand() % 20 - 10) as f32;
                ctx.create_joint(BITMAP_JOINT_SPARK, pos, pos, angle, 0, None, 10.0);
                // `if(j==0)` (ZzzEffect.cpp:10072); MuMain uses `rand()%8==0` instead.
                if j == 0 {
                    ctx.create_particle(BITMAP_SPARK, pos, angle, light, 0, 1.0, None);
                }
            }
        }
        angle = Vec3::ZERO;

        if e.kind == 0 {
            ctx.create_effect(MODEL_WAVE, e.start_position, angle, e.light, 0, None, -1, 0, 0, 0.0);
        }

        e.start_position.z -= 27.0;

        // Not for the werewolf hero (`Owner->Type != MODEL_MONSTER01+95`, ZzzEffect.cpp:10084).
        if e.owner.is_some() && !owner_is_werewolf(e, ctx) {
            // Owner = this effect; the scale goes in PKKey (150 → 1.5).
            let me = Some(Target::Effect(e.id));
            ctx.create_effect(MODEL_SKILL_FURY_STRIKE + 3, e.start_position, angle, e.light, 0, me, scale, 0, 0, 0.0);
            ctx.create_effect(MODEL_SKILL_FURY_STRIKE + 1, e.start_position, angle, e.light, 0, me, scale, 0, 0, 0.0);
            ctx.create_effect(MODEL_SKILL_FURY_STRIKE + 2, e.start_position, angle, e.light, 0, me, scale, 0, 0, 0.0);
        }

        // Five cracks around.
        for i in 0..5 {
            let p = Vec3::new(0.0, (ctx.rand() % 150 + 100) as f32, 0.0);
            angle = Vec3::new(0.0, 0.0, e.sub_type as f32 + i as f32 * 72.0);
            let mut pos = rotate(angle, p) + e.start_position;
            pos.z += 3.0;
            scale = ctx.rand() % 50 + 40;
            angle = Vec3::new(0.0, 0.0, 45.0 + (ctx.rand() % 30 - 15) as f32);

            let wall = ctx.terrain_attribute(pos.x, pos.y);
            if wall & TW_NOMOVE != TW_NOMOVE && wall & TW_NOGROUND != TW_NOGROUND && wall & TW_WATER != TW_WATER {
                if hellas {
                    add_height = 100.0;
                }
                pos.z = ctx.terrain_height(pos.x, pos.y) + add_height;
                ctx.create_effect(MODEL_SKILL_FURY_STRIKE + 4, pos, angle, e.light, 0, e.owner, scale, 0, 0, 0.0);
                ctx.create_effect(MODEL_SKILL_FURY_STRIKE + 5, pos, angle, e.light, 0, e.owner, scale, 0, 0, 0.0);
            }
        }
    } else if lt == 10 {
        // Cracks branching in five directions (4 segments each).
        let mut pos = [e.start_position; 5];
        let mut ang = [0.0f32; 5];
        let mut count: i32 = 0;
        for j in 0..4 {
            let p = Vec3::new(0.0, (ctx.rand() % 15) as f32 + 85.0, 0.0);
            if j >= 3 {
                count = ctx.rand();
            }
            for i in 0..5 {
                let random = if count % 2 == 0 { ctx.rand() % 30 + 50 } else { -(ctx.rand() % 30 + 50) };
                ang[i] += random as f32;
                angle.z = ang[i] + (i as i32 * (ctx.rand() % 10 + 62)) as f32;
                pos[i] += rotate(angle, p);

                let wall = ctx.terrain_attribute(pos[i].x, pos[i].y);
                // (the original uses || here: almost always true)
                if wall & TW_NOMOVE != TW_NOMOVE || wall & TW_NOGROUND != TW_NOGROUND || wall & TW_WATER != TW_WATER {
                    pos[i].z = ctx.terrain_height(pos[i].x, pos[i].y) + if hellas { 100.0 } else { 3.0 };
                    angle.z += 270.0;
                    ctx.create_effect(MODEL_SKILL_FURY_STRIKE + 7, pos[i], angle, e.light, 0, e.owner, 100, 0, 0, 0.0);
                    ctx.create_effect(MODEL_SKILL_FURY_STRIKE + 8, pos[i], angle, e.light, 0, e.owner, 100, 0, 0, 0.0);
                }
            }
            count += 1;
        }
        e.life_time = 0.0;
        ctx.play_sound("SOUND_FURY_STRIKE3");
    } else {
        // The weapon rises and falls in an arc.
        let mut count = e.life_time;
        let mut add_angle = 15.0;
        if e.life_time > 9.0 && e.life_time < 16.0 {
            count = 12.5;
            add_angle = 18.0;
            if lt == 15 {
                e.gravity *= -1.0;
            }
        }

        if e.kind == 0 && lt == 13 {
            // Trails (`MODEL_TAIL`) behind the weapon.
            angle = Vec3::ZERO;
            let mut p0 = e.position;
            if let Some(oa) = owner_angle {
                p0 = rotate(oa, Vec3::new(-25.0, -40.0, 0.0)) + e.position;
            }
            for i in 0..4 {
                let position = Vec3::new(p0.x, p0.y, p0.z - (i * 50) as f32);
                ctx.create_effect(MODEL_TAIL, position, angle, e.light, 0, None, -1, 0, 0, 0.0);
            }
            p0.x += (ctx.rand() % 30 + 20) as f32;
            p0.z += (ctx.rand() % 500 - 250) as f32;
            for i in 0..4 {
                let position = Vec3::new(p0.x, p0.y, p0.z - (i * 30) as f32);
                ctx.create_effect(MODEL_TAIL, position, angle, e.light, 0, None, -1, 0, 0, 0.0);
            }
            ctx.play_sound("SOUND_FURY_STRIKE2");
        }

        let a = (20.0 - count) * add_angle;
        e.angle.x += 80.0;
        e.direction.y = (a * std::f32::consts::PI / 180.0).sin() * 260.0;
        e.position = e.start_position;
        if count != 12.5 {
            e.gravity += 8.0;
        } else {
            e.gravity -= 8.0;
        }
        e.position = rotate(e.head_angle, e.direction) + e.start_position;
        e.position.z += e.gravity;
        e.position.z += 200.0;
    }
    // No common tail (`MODEL_SKILL_FURY_STRIKE` is excluded).
}

/// `RenderFuryStrike` (ZzzEffect.cpp:18203-18234): the owner's weapon, with its level glow, while
/// `LifeTime > 10` and `Kind == 0`.
/// TODO(original): its `ItemObjectAttribute(o)` leaves the effect with Light 0.3, Alpha 1, Scale
/// 0.8/0.7, AnimationFrame/Velocity 0 after each render (only `Gravity` is restored), so the
/// `+1..+8` parts created at `LifeTime` 11/10 get Light 0.3; MuMain draws a copy and keeps the
/// effect as is, like here.
fn draw_fury(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    if !(e.life_time > 10.0 && e.kind == 0) {
        return;
    }
    let Some(actor) = common::owner_actor(e.owner, ctx.actors) else { return };
    let Some((model, level)) = actor.weapon else { return };
    // `RequestTerrainLight + o->Light` (0.3 after ItemObjectAttribute).
    let light = ctx.terrain_light(e.position.x, e.position.y) + Vec3::splat(0.3);
    super::effects_dk1::render_weapon(out, ctx, model, level, e.position, e.angle, super::effects_dk1::weapon_scale(model), light, e.alpha);
}

// --- MODEL_SKILL_FURY_STRIKE+1..+8: impact rings, cracks and waves -----------------------------------

/// ZzzEffect.cpp:2062-2123. `o->RenderType = RENDER_DARK` (subtype 1 of `+2`, `+5`, `+8`) is
/// read by [`draw_fury_part`] from the subtype.
fn create_fury_part(e: &mut Effect, _ctx: &mut Ctx) {
    let n = e.ty - MODEL_SKILL_FURY_STRIKE;
    let scale = e.pk_key as f32 / 100.0;
    match n {
        1 => {
            if e.sub_type == 0 {
                e.life_time = 35.0;
                e.scale = scale;
                e.blend_mesh = 0;
            } else if e.sub_type == 1 {
                e.life_time = 60.0;
                e.scale = scale;
                e.blend_mesh = 0;
            }
        }
        2 => {
            if e.sub_type == 0 || e.sub_type == 1 {
                e.life_time = 20.0;
                e.scale = scale;
                e.blend_mesh = 0;
            } else if e.sub_type == 2 {
                e.life_time = 50.0;
                e.scale = scale;
                e.blend_mesh = 0;
            }
        }
        3 | 6 => {
            e.life_time = 35.0;
            e.scale = scale;
        }
        4 => {
            e.life_time = 35.0;
            e.scale = scale;
            e.blend_mesh = 0;
        }
        5 | 8 => {
            e.life_time = 40.0;
            e.scale = scale;
            e.blend_mesh = 0;
        }
        7 => {
            e.life_time = 40.0;
            e.scale = scale;
            e.blend_mesh = 0;
        }
        _ => {}
    }
}

/// ZzzEffect.cpp:10242-10390.
fn step_fury_part(e: &mut Effect, ctx: &mut Ctx) {
    let lum = luminosity(e, ctx);
    let n = e.ty - MODEL_SKILL_FURY_STRIKE;
    match n {
        1 => {
            e.blend_mesh_light = (e.life_time * 0.1) / 3.0;
            if e.life_time < 10.0 {
                e.position.z -= 0.5;
            }
            if e.life_time > 15.0 && (e.life_time as i32) % 3 == 0 {
                // `EarthQuake = ...` (ZzzEffect.cpp:10250); MuMain fades it with the distance
                // to the hero (`SetEarthQuakeAt`).
                let amount = (ctx.rand() % 8 - 4) as f32 * 0.1;
                ctx.set_earthquake(amount);
            }
        }
        3 | 6 => {
            e.blend_mesh_light = (e.life_time * 0.1) / 10.0;
            if e.life_time < 13.0 {
                e.position.z -= 0.5;
            }
        }
        2 => {
            if e.scale > 50.0 {
                e.life_time = 0.0;
            }
            if e.life_time >= 10.0 {
                e.blend_mesh_light = (20.0 - e.life_time) * 0.1;
            } else {
                if e.life_time < 5.0 {
                    e.position.z -= 0.5;
                }
                e.blend_mesh_light = e.life_time * 0.1;
                if e.life_time >= 5.0 && ctx.rand_fps_check(10) {
                    spawn_crack_stone(e, ctx);
                }
            }
            e.blend_mesh_tex_coord.x = -((e.life_time as i32) as f32) * 0.01;
            let light = crack_light(e, ctx, lum);
            ctx.add_terrain_light(e.position.x, e.position.y, light, 1.0);
        }
        4 | 7 => {
            e.blend_mesh_light = (e.life_time * 0.1) / 3.0;
            if e.life_time < 10.0 {
                e.position.z -= 0.5;
            }
        }
        5 => {
            if e.life_time >= 30.0 {
                e.blend_mesh_light = (40.0 - e.life_time) * 0.1;
            } else {
                if e.life_time < 15.0 {
                    e.position.z -= 0.5;
                }
                e.blend_mesh_light = e.life_time * 0.1;
                if e.life_time >= 5.0 && ctx.rand_fps_check(15) {
                    spawn_crack_stone(e, ctx);
                }
            }
            e.blend_mesh_tex_coord.x = -((e.life_time as i32) as f32) * 0.01;
            let light = crack_light(e, ctx, lum);
            ctx.add_terrain_light(e.position.x, e.position.y, light, 1.0);
        }
        8 => {
            if e.life_time >= 30.0 {
                e.blend_mesh_light = (40.0 - e.life_time) * 0.1;
            } else {
                e.blend_mesh_light = e.life_time * 0.1;
            }
            if e.life_time < 15.0 {
                e.position.z -= 0.5;
            }
            // The original computes a position and discards it (but consumes the rand).
            if e.life_time >= 13.0 {
                let _ = ctx.rand_fps_check(5);
            }
            e.blend_mesh_tex_coord.x = -((e.life_time as i32) as f32) * 0.01;
            let light = crack_light(e, ctx, lum);
            ctx.add_terrain_light(e.position.x, e.position.y, light, 1.0);
        }
        _ => {}
    }
    move_tail(e, ctx);
}

/// `RenderObject(o)` (the `default:` of RenderEffects): `o->RenderType == RENDER_DARK` draws
/// `RenderBody(RENDER_TEXTURE|RENDER_DARK, ...)` (ZzzObject.cpp:489-492), the rest as usual.
fn draw_fury_part(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    if !e.visible {
        return;
    }
    let n = e.ty - MODEL_SKILL_FURY_STRIKE;
    let mut d = ModelDraw::of_effect(e);
    if matches!(n, 2 | 5 | 8) && e.sub_type == 1 {
        d.render_flags = RENDER_TEXTURE | RENDER_DARK;
    }
    out.models.push(d);
}

// --- MODEL_WAVE (flashing.bmd) ------------------------------------------------------------------------

/// ZzzEffect.cpp:1494-1502.
fn create_wave(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 15.0;
    e.scale = 0.5;
    e.blend_mesh = 0;
    e.blend_mesh_light = 1.5;
    e.light = Vec3::ONE;
    e.position.z += -15.0;
}

/// ZzzEffect.cpp:9716-9733.
fn step_wave(e: &mut Effect, ctx: &mut Ctx) {
    let lum = luminosity(e, ctx);
    if e.scale > 2.0 {
        e.scale += 0.1;
        e.position.x -= 1.0;
        e.position.z -= 1.5;
        e.blend_mesh_light = e.life_time / 30.0;
    } else {
        e.scale += 1.2;
        e.position.x -= 1.2;
        e.position.z -= 1.8;
    }
    // Darkens the ground around it.
    let light = Vec3::splat(-lum * 0.5);
    ctx.add_terrain_light(e.position.x, e.position.y, light, 5.0);
    move_tail(e, ctx);
}

// --- MODEL_TAIL (tail.bmd) ----------------------------------------------------------------------------

/// ZzzEffect.cpp:1503-1510. `BlendMesh = -2`: `RenderObject` draws every mesh additive with
/// `Light · BlendMeshLight` (models.rs handles `blend_mesh <= -2`).
fn create_tail(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 6.0;
    e.scale = 1.0;
    e.gravity = 80.0;
    e.blend_mesh = -2;
    e.light = Vec3::splat(0.5);
    e.angle = Vec3::new(0.0, 0.0, 45.0);
}

/// ZzzEffect.cpp:9734-9738.
fn step_tail(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    e.position.z -= e.gravity;
    e.gravity += 60.0;
    e.blend_mesh_light = e.life_time / 20.0;
    move_tail(e, ctx);
}

// --- MODEL_STONE1 / MODEL_STONE2: bouncing stones -----------------------------------------------------

/// ZzzEffect.cpp:2877-2937 (and the `MODEL_ICE_SMALL` case it falls into).
fn create_stone(e: &mut Effect, ctx: &mut Ctx) {
    match e.sub_type {
        5 => {
            e.life_time = 60.0;
            e.scale = (ctx.rand() % 4 + 8) as f32 * 0.1;
        }
        11 => {
            e.life_time = 40.0;
            e.scale = (ctx.rand() % 4 + 8) as f32 * 0.8;
            e.angle.z = rf(ctx, 360);
        }
        10 => {
            let p1 = Vec3::new(0.0, (ctx.rand() % 256 + 64) as f32 * 0.2, 0.0);
            e.life_time = (ctx.rand() % 16 + 32) as f32;
            e.scale = (ctx.rand() % 4 + 15) as f32 * 0.05;
            e.angle.z = rf(ctx, 360);
            e.direction = rotate(e.angle, p1);
            e.gravity = (ctx.rand() % 16 + 28) as f32;
        }
        12 => {
            let p1 = Vec3::new(0.0, (ctx.rand() % 256 + 64) as f32 * 0.1, 0.0);
            e.life_time = (ctx.rand() % 16 + 32) as f32;
            e.scale = (ctx.rand() % 4 + 15) as f32 * 0.1;
            e.angle.z = rf(ctx, 360);
            e.direction = rotate(e.angle, p1);
            e.gravity = (ctx.rand() % 16 + 8) as f32;
        }
        13 | 14 => {
            e.direction = Vec3::ZERO;
            e.life_time = (ctx.rand() % 16 + 20) as f32;
            // `o->Scale *= Scale` (Scale = the argument; the core already turned 0 into 0.9).
            let call_scale = e.scale;
            e.scale = (ctx.rand() % 13 + 3) as f32 * 0.08;
            e.scale *= call_scale;
            e.gravity = (ctx.rand() % 3 + 3) as f32;
            e.angle.z = rf(ctx, 360);
            let p1 = Vec3::new(0.0, (ctx.rand() % 128 + 64) as f32 * 0.1, 0.0);
            e.head_angle = rotate(e.angle, p1);
            e.head_angle.z += 15.0;
        }
        _ => {
            // Falls through to the MODEL_ICE_SMALL/METEO case.
            let p1 = if e.sub_type == 1 { Vec3::ZERO } else { Vec3::new(0.0, (ctx.rand() % 256 + 64) as f32 * 0.1, 0.0) };
            e.life_time = (ctx.rand() % 16 + 32) as f32;
            e.scale = (ctx.rand() % 4 + 8) as f32 * 0.1;
            e.angle.z = rf(ctx, 360);
            e.direction = rotate(e.angle, p1);
            e.gravity = (ctx.rand() % 16 + 8) as f32;
        }
    }
}

/// ZzzEffect.cpp:11055-11173.
fn step_stone(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    match e.sub_type {
        5 => {
            // Stuck to the owner; no common tail (ZzzEffect.cpp:18064).
            if let Some(p) = ctx.target_position(e.owner) {
                e.position = p + e.start_position;
            }
            return;
        }
        11 => {
            if e.life_time < 10.0 {
                e.alpha *= 0.8;
            }
        }
        13 | 14 => {
            e.head_angle.z -= e.gravity;
            e.position += e.head_angle;
            let height = ctx.terrain_height(e.position.x, e.position.y);
            e.angle.x += 0.5 * e.life_time;
            e.angle.y += 0.5 * e.life_time;
            if e.position.z + e.direction.z <= height {
                e.position.z = height;
                e.head_angle.x *= 0.6;
                e.head_angle.y *= 0.6;
                e.head_angle.z += 1.0 * e.life_time;
                if e.head_angle.z < 0.5 {
                    e.head_angle.z = 0.0;
                }
                e.alpha -= 0.1;
            }
        }
        _ => move_ice_small(e, ctx),
    }
    move_tail(e, ctx);
}

/// `Move_MODEL_ICE_SMALL` (the part the stones use).
fn move_ice_small(e: &mut Effect, ctx: &mut Ctx) {
    let sub = e.sub_type;
    if sub == 0 || sub == 10 || sub == 12 {
        e.position += e.direction;
    } else {
        e.position += rotate(e.angle, e.direction);
    }
    e.direction *= 0.9;
    e.position.z += e.gravity;
    if sub == 0 || sub == 10 || sub == 12 || sub == 13 {
        e.gravity -= 3.0;
        let height = ctx.terrain_height(e.position.x, e.position.y);
        if e.position.z < height {
            e.position.z = height;
            e.gravity = -e.gravity * 0.5;
            e.life_time -= 4.0;
            e.angle.x -= e.scale * 128.0;
        } else {
            e.angle.x -= e.scale * 32.0;
        }
        if (sub == 0 || sub == 12 || sub == 13) && ctx.rand_fps_check(10) {
            let st = 1 + ctx.rand() % 3;
            ctx.create_particle(BITMAP_FIRE, e.position, e.angle, e.light, st, 1.0, None);
        }
    } else if sub == 1 {
        e.gravity += 0.5;
    } else {
        e.angle.z += 20.0;
        e.gravity += 0.5;
    }
}
