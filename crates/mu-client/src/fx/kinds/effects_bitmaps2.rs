//! `Effects[]` whose type is a `BITMAP_*` texture (part 2): emitters, ground marks and sprites.
//! `BITMAP_GATHERING`, `BITMAP_JOINT_THUNDER`, `BITMAP_JOINT_THUNDER+1`, the firecrackers
//! (`BITMAP_FIRECRACKERRISE`, `BITMAP_FIRECRACKER`, `BITMAP_FIRECRACKER0001..0003`),
//! `BITMAP_SWORD_FORCE`, `BITMAP_BLIZZARD`, `BITMAP_SHOTGUN`, `BITMAP_FLARE_FORCE`, `BITMAP_TWLIGHT`,
//! `BITMAP_SHOCK_WAVE`, `BITMAP_DAMAGE_01_MONO`, `BITMAP_FLARE`, `BITMAP_CRATER`,
//! `BITMAP_CHROME_ENERGY2`, `BITMAP_JOINT_FORCE`, `BITMAP_TARGET_POSITION_EFFECT1/2`,
//! `BITMAP_RING_OF_GRADATION` and `BITMAP_WATERFALL_4`.
//!
//! Translated from Main 5.2 `ZzzEffect.cpp` (`CreateEffect`, `MoveEffect`, `RenderEffects`,
//! `RenderEffectShadows`); MuMain `Behaviors/MoveHandlers.cpp` only to confirm.
//!
//! None of these types draws a model (`RenderEffects` has no `case` for them and they are outside
//! `MODEL_SKILL_BEGIN..MODEL_SKILL_END`): what is seen are the particles, trails and sprites they
//! create and the ground marks of `RenderEffectShadows` (drawn here with [`common::terrain_bitmap`]).
//! `RenderEffectShadows` calls `EnableAlphaBlend()` before its switch → [`Blend::Glow`] unless a
//! case enables something else.
//!
//! Function arguments with several `rand()` calls are evaluated right to left, like the 32-bit
//! MSVC build of the original does (e.g. `CreateSprite(.., rand()%8.., .., o, rand()%360)` draws
//! the rotation first).

use bevy::math::Vec3;

use super::common::{self, rf};
use crate::fx::core::*;

pub fn register(r: &mut Registry) {
    let k = |create: EffectCreate, step: EffectStep, draw: EffectDraw| EffectKind { create: Some(create), step: Some(step), draw: Some(draw) };
    let nothing: EffectDraw = common::draw_nothing;
    r.effect(BITMAP_GATHERING, k(gathering_create, gathering_step, nothing));
    r.effect(BITMAP_JOINT_THUNDER, k(joint_thunder_create, joint_thunder_step, nothing));
    r.effect(BITMAP_JOINT_THUNDER + 1, k(joint_thunder1_create, joint_thunder1_step, nothing));
    r.effect(BITMAP_FIRECRACKERRISE, k(firecracker_rise_create, firecracker_rise_step, nothing));
    r.effect(BITMAP_FIRECRACKER, k(firecracker_create, firecracker_step, nothing));
    r.effect(BITMAP_FIRECRACKER0001, k(firecracker1_create, firecracker1_step, nothing));
    r.effect(BITMAP_FIRECRACKER0002, k(firecracker2_create, firecracker2_step, nothing));
    r.effect(BITMAP_FIRECRACKER0003, k(firecracker3_create, firecracker3_step, nothing));
    r.effect(BITMAP_SWORD_FORCE, k(sword_force_create, sword_force_step, nothing));
    r.effect(BITMAP_BLIZZARD, k(blizzard_create, blizzard_step, nothing));
    r.effect(BITMAP_SHOTGUN, k(shotgun_create, shotgun_step, nothing));
    r.effect(BITMAP_FLARE_FORCE, k(flare_force_create, flare_force_step, nothing));
    r.effect(BITMAP_TWLIGHT, k(twilight_create, twilight_step, shock_wave_draw));
    r.effect(BITMAP_SHOCK_WAVE, k(shock_wave_create, shock_wave_step, shock_wave_draw));
    r.effect(BITMAP_DAMAGE_01_MONO, k(damage_mono_create, damage_mono_step, damage_mono_draw));
    r.effect(BITMAP_FLARE, k(flare_create, flare_step, nothing));
    r.effect(BITMAP_CRATER, k(crater_create, crater_step, crater_draw));
    r.effect(BITMAP_CHROME_ENERGY2, k(chrome_energy2_create, chrome_energy2_step, chrome_energy2_draw));
    r.effect(BITMAP_JOINT_FORCE, k(joint_force_create, joint_force_step, nothing));
    r.effect(BITMAP_TARGET_POSITION_EFFECT1, k(target_pos1_create, target_pos1_step, target_pos_draw));
    r.effect(BITMAP_TARGET_POSITION_EFFECT2, k(target_pos2_create, target_pos2_step, target_pos_draw));
    r.effect(BITMAP_RING_OF_GRADATION, k(ring_gradation_create, ring_gradation_step, ring_gradation_draw));
    r.effect(BITMAP_WATERFALL_4, k(waterfall4_create, waterfall4_step, nothing));
}

// --- Helpers ----------------------------------------------------------------------------------------

/// `o` passed as `Owner`/`Target` of what an effect creates.
fn me(e: &Effect) -> Option<Target> {
    Some(Target::Effect(e.id))
}

/// `o->Owner->Angle`: a character's angle, or the angle of an owner effect (as of tick start).
fn owner_angle(ctx: &Ctx, owner: Option<Target>) -> Option<Vec3> {
    match owner? {
        Target::Actor(a) => ctx.actors.get(&a).map(|a| a.angle),
        Target::Effect(id) => ctx.effects.iter().find(|i| i.id == id).map(|i| i.angle),
    }
}

/// The local `Light` of `MoveEffect` (ZzzEffect.cpp:6929): `Vector(1.f,1.f,1.f,Light)`.
const MOVE_LIGHT: Vec3 = Vec3::ONE;

/// The `EyeRight` fade of `BITMAP_TWLIGHT` subtype 3 (ZzzEffect.cpp:13426-13446) and
/// `BITMAP_SHOCK_WAVE` subtype 14 (ZzzEffect.cpp:13525-13544): `user[0..3]` = `EyeRight`,
/// `pk_key` = `PKKey` (used as a counter).
fn eye_fade(e: &mut Effect, lt: i32) {
    let eye = Vec3::new(e.user[0], e.user[1], e.user[2]);
    if lt >= 20 {
        e.alpha += 0.1;
        e.pk_key += 1;
        e.light = eye * (e.pk_key as f32 * 0.1);
    } else if lt <= 10 {
        e.pk_key -= 1;
        e.alpha -= 0.1;
        e.light = eye * (e.pk_key as f32 * 0.1);
    }
}

// --- BITMAP_GATHERING --------------------------------------------------------------------------------

/// ZzzEffect.cpp:1451-1477.
fn gathering_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 10.0;
    match e.sub_type {
        0 => {
            e.position += rotate(e.angle, Vec3::new(0.0, -100.0, 0.0));
            e.position.z += 150.0;
        }
        1 | 2 => {
            // VectorCopy(Position, o->StartPosition): the argument, equal to o->Position here.
            e.start_position = e.position;
            e.life_time = 20.0;
        }
        3 => {
            e.life_time = 10.0;
            e.position += rotate(e.angle, Vec3::new(-10.0, 10.0, 0.0));
        }
        _ => {}
    }
}

/// ZzzEffect.cpp:8618-8671.
fn gathering_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    let lt = e.life_time as i32;
    if e.sub_type == 1 || e.sub_type == 2 {
        // BodyOrigin = StartPosition; TransformPosition(Owner->BoneTransform[33], (0,0,10), Position, true).
        // The original dereferences o->Owner unconditionally; without a character owner it stays put.
        if let Some(a) = common::owner_actor(e.owner, ctx.actors) {
            e.position = a.bone_world(33, Vec3::new(0.0, 0.0, 10.0)) - a.position + e.start_position;
        }
    }
    for _ in 0..3 {
        let p = if e.sub_type == 3 { Vec3::new(0.0, 25.0, 0.0) } else { Vec3::new(0.0, 120.0, 0.0) };
        let angle = Vec3::new(rf(ctx, 360), 0.0, rf(ctx, 360));
        let position = e.position + rotate(angle, p);
        match e.sub_type {
            1 => {
                if lt % 2 == 0 {
                    ctx.create_joint_ex(BITMAP_JOINT_THUNDER, position, e.position, angle, 3, None, 10.0, 10, 10);
                } else {
                    let s = (ctx.rand() % 50 + 10) as f32 / 100.0;
                    ctx.create_particle(BITMAP_SPARK + 1, position, angle, MOVE_LIGHT, 2, s, me(e));
                }
                let rotation = rf(ctx, 360);
                let scale = (ctx.rand() % 8 + 8) as f32 * 0.2;
                ctx.create_sprite(BITMAP_SHINY + 1, e.position, scale, e.light, rotation, 0);
            }
            2 => {
                if lt % 2 == 0 {
                    ctx.create_joint_ex(BITMAP_JOINT_THUNDER, position, e.position, angle, 3, None, 10.0, 10, 10);
                }
                let rotation = rf(ctx, 360);
                let scale = (ctx.rand() % 8 + 8) as f32 * 0.2;
                ctx.create_sprite(BITMAP_SHINY + 1, e.position, scale, e.light, rotation, 0);
            }
            3 => {
                let s = (ctx.rand() % 10 + 5) as f32 / 25.0;
                ctx.create_particle(BITMAP_SPARK + 1, position, angle, e.light, 26, s, me(e));
            }
            _ => {
                let rotation = rf(ctx, 360);
                let scale = (ctx.rand() % 8 + 8) as f32 * 0.3;
                ctx.create_sprite(BITMAP_SHINY + 1, e.position, scale, e.light, rotation, 0);
                let s = (ctx.rand() % 50 + 10) as f32 / 100.0;
                ctx.create_particle(BITMAP_SPARK + 1, position, angle, MOVE_LIGHT, 2, s, me(e));
            }
        }
    }
    common::move_tail(e, ctx);
}

// --- BITMAP_JOINT_THUNDER (lightning storm emitter) ----------------------------------------------------

/// ZzzEffect.cpp:1478-1485.
fn joint_thunder_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 20.0;
    e.start_position = e.position;
    e.start_position.z += 800.0;
}

/// ZzzEffect.cpp:8672-8739.
fn joint_thunder_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    let lt = e.life_time as i32;
    let f_scale = (ctx.rand() % 400 + 100) as f32 / 10.0;
    let mut v_start = e.start_position;
    let mut v_pos = e.position;
    v_start.x += (ctx.rand() % 100 - 50) as f32;
    v_start.y += (ctx.rand() % 100 - 50) as f32;
    v_pos.x += (ctx.rand() % 100 - 50) as f32;
    v_pos.y += (ctx.rand() % 100 - 50) as f32;

    if ctx.rand() % 10 > 4 {
        let sub = if e.sub_type == 1 { 33 } else { 16 };
        ctx.create_joint(BITMAP_JOINT_THUNDER, v_start, v_pos, e.angle, sub, None, f_scale);
    }

    let mut v_light = Vec3::new(0.45, 0.45, 0.7);
    if lt > 10 && ctx.rand() % 2 == 0 {
        ctx.create_effect(BITMAP_MAGIC + 1, v_pos, e.angle, v_light, 11, me(e), -1, 0, 0, 0.0);
    }
    if ctx.rand() % 4 == 0 {
        ctx.create_particle(BITMAP_SMOKE, v_pos, e.angle, v_light, 54, 2.8, None);
    }
    if ctx.rand() % 4 == 0 {
        let ty = MODEL_STONE1 + ctx.rand() % 2;
        ctx.create_effect(ty, e.position, e.angle, v_light, 13, None, -1, 0, 0, 0.0);
    }
    if lt > 5 {
        v_light = Vec3::new(0.15, 0.15, 0.4);
        if ctx.rand() % 5 == 0 {
            ctx.create_effect(BITMAP_CHROME_ENERGY2, v_pos, e.angle, v_light, 0, None, -1, 0, 0, 0.0);
        }
    }

    // Sparks of light on the owner's bones (`o->Owner->BoneTransform != NULL`: only characters
    // carry bones here).
    if e.owner.is_some()
        && ctx.rand() % 5 == 0
        && let Some(owner) = common::owner_actor(e.owner, ctx.actors).cloned()
    {
        let n = owner.bones.len() as i32;
        for i in 0..n {
            // The original's `if / else if / else if ... continue` chain (same rand() order).
            let skip = (n > 100 && ctx.rand() % n > n / 10)
                || (n > 50 && ctx.rand() % n > n / 5)
                || (n > 20 && ctx.rand() % n > n / 2);
            if skip {
                continue;
            }
            // BodyOrigin = Owner->Position; TransformPosition(BoneTransform[i], (0,0,0), vPos, true).
            let v_pos = owner.bone_world(i as usize, Vec3::ZERO);
            let random = 3.0 + (ctx.rand() % 20 - 10) as f32 * 0.1;
            // vAngle is left uninitialized in the original; zero here.
            ctx.create_particle(BITMAP_LIGHT, v_pos, Vec3::ZERO, Vec3::new(0.2, 0.2, 0.8), 5, random, None);
        }
    }
    common::move_tail(e, ctx);
}

// --- BITMAP_JOINT_THUNDER+1 ------------------------------------------------------------------------------

/// ZzzEffect.cpp:3876-3889.
fn joint_thunder1_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = 10.0;
    if e.owner.is_some()
        && e.sub_type == 0
        && let Some(angle) = owner_angle(ctx, e.owner)
    {
        e.position += rotate(angle, Vec3::new(-25.0, -80.0, 0.0));
    }
}

/// ZzzEffect.cpp:13624-13634.
fn joint_thunder1_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    let lt = e.life_time as i32;
    if lt < 5 && lt % 2 == 0 && e.sub_type == 0 {
        let mut position = e.position;
        position.x += (ctx.rand() % 64) as f32 - 32.0;
        position.y += (ctx.rand() % 64) as f32 - 32.0;
        let scale = 50.0 + (ctx.rand() % 10) as f32;
        ctx.create_joint(BITMAP_JOINT_THUNDER + 1, position, position, e.angle, 5, None, scale);
    }
    common::move_tail(e, ctx);
}

// --- BITMAP_FIRECRACKERRISE ------------------------------------------------------------------------------

/// ZzzEffect.cpp:3118-3122.
fn firecracker_rise_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.angle = Vec3::ZERO;
    e.position.z = 100.0;
    e.life_time = 15.0 * 5.0;
}

/// ZzzEffect.cpp:12617-12622.
fn firecracker_rise_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    if e.life_time as i32 % 5 == 0 && ctx.rand() % 3 == 0 {
        ctx.create_effect(BITMAP_FIRECRACKER, e.position, e.angle, e.light, 0, None, -1, 0, 0, 0.0);
    }
    common::move_tail(e, ctx);
}

// --- BITMAP_FIRECRACKER (a rocket) -----------------------------------------------------------------------

/// ZzzEffect.cpp:3123-3133.
fn firecracker_create(e: &mut Effect, ctx: &mut Ctx) {
    e.light = Vec3::ONE;
    e.direction.x = (ctx.rand() % 9 - 4) as f32;
    e.direction.y = (ctx.rand() % 9 - 4) as f32;
    e.direction.z = 26.0;
    e.life_time = if e.sub_type == 1 { (4 + ctx.rand() % 3) as f32 } else { 12.0 };
    ctx.play_sound("SOUND_FIRECRACKER1");
}

/// ZzzEffect.cpp:12623-12639.
fn firecracker_step(e: &mut Effect, ctx: &mut Ctx) {
    let luminosity = common::luminosity(e, ctx);
    if e.life_time as i32 == 1 {
        let sub = ctx.rand() % 30;
        let r = (ctx.rand() % 3) as f32 * 0.3 + 0.4;
        let g = (ctx.rand() % 4) as f32 * 0.1;
        let light = Vec3::new(r, g, 0.0);
        for _ in 0..80 {
            ctx.create_particle(BITMAP_FIRECRACKER, e.position, e.angle, light, sub, 1.0, None);
        }
        ctx.play_sound("SOUND_FIRECRACKER2");
    }
    ctx.create_particle(BITMAP_FIRECRACKER, e.position, e.angle, e.light, -1, 1.0, None);
    let light = Vec3::new(luminosity * 0.4, luminosity * 0.3, luminosity * 0.2);
    ctx.add_terrain_light(e.position.x, e.position.y, light, 1.0);
    common::move_tail(e, ctx);
}

// --- BITMAP_FIRECRACKER0001 (Christmas firecracker: spirit trails) ---------------------------------------

/// ZzzEffect.cpp:3134-3139.
fn firecracker1_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 31.0;
    e.direction = Vec3::ZERO;
}

/// ZzzEffect.cpp:12640-12650.
fn firecracker1_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    if matches!(e.life_time as i32, 1 | 9 | 17 | 24 | 31) {
        let x = e.position.x + (ctx.rand() % 200 - 100) as f32;
        let y = e.position.y + (ctx.rand() % 200 - 100) as f32;
        let position = Vec3::new(x, y, e.position.z);
        ctx.create_joint_ex(BITMAP_JOINT_SPIRIT, position, position, e.angle, 25, me(e), 1.0, -1, e.sub_type);
    }
    common::move_tail(e, ctx);
}

// --- BITMAP_FIRECRACKER0002 (Christmas firecracker: the burst) -------------------------------------------

/// ZzzEffect.cpp:3140-3178.
fn firecracker2_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = 30.0;
    ctx.create_particle(BITMAP_EXPLOTION_MONO, e.position, e.angle, e.light, 0, 0.6, None);
    let mut v_light = Vec3::ONE;
    for _ in 0..60 {
        ctx.create_particle(BITMAP_SPARK + 1, e.position, e.angle, v_light, 27, 1.0, None);
    }
    for _ in 0..30 {
        ctx.create_particle(BITMAP_SPARK + 1, e.position, e.angle, v_light, 28, 1.0, None);
    }
    let x = e.position.x + (ctx.rand() % 100 - 50) as f32;
    let y = e.position.y + (ctx.rand() % 100 - 50) as f32;
    let position = Vec3::new(x, y, e.position.z);
    let scale = (ctx.rand() % 10) as f32 * 0.1 + 1.5;
    ctx.create_sprite(BITMAP_DS_SHOCK, position, scale, e.light, 0.0, 0);
    for _ in 0..60 {
        let r = 0.3 + (ctx.rand() % 700) as f32 * 0.001;
        let g = 0.3 + (ctx.rand() % 700) as f32 * 0.001;
        let b = 0.3 + (ctx.rand() % 700) as f32 * 0.001;
        v_light = Vec3::new(r, g, b);
        ctx.create_particle(BITMAP_SHINY, e.position, e.angle, v_light, 6, 1.0, None);
    }
    if e.sub_type == 1 {
        ctx.create_effect(MODEL_HALLOWEEN_CANDY_STAR, e.position, e.angle, v_light, 1, None, -1, 0, 0, 0.0);
        for _ in 0..2 {
            let ty = ctx.rand() % 4 + MODEL_XMAS_EVENT_BOX;
            ctx.create_effect(ty, e.position, e.angle, v_light, 0, me(e), -1, 0, 0, 0.0);
        }
    }
    ctx.play_sound("SOUND_XMAS_FIRECRACKER");
}

/// ZzzEffect.cpp:12651-12663.
fn firecracker2_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    if ctx.rand() % 5 == 0 {
        let r = 0.3 + (ctx.rand() % 700) as f32 * 0.001;
        let g = 0.3 + (ctx.rand() % 700) as f32 * 0.001;
        let b = 0.3 + (ctx.rand() % 700) as f32 * 0.001;
        let v_light = Vec3::new(r, g, b);
        let x = e.position.x + (ctx.rand() % 300 - 150) as f32;
        let y = e.position.y + (ctx.rand() % 300 - 150) as f32;
        let position = Vec3::new(x, y, e.position.z);
        let scale = 0.5 + (ctx.rand() % 5) as f32 * 0.1;
        ctx.create_effect(BITMAP_FIRECRACKER0003, position, e.angle, v_light, 0, me(e), -1, 0, 0, scale);
    }
    common::move_tail(e, ctx);
}

// --- BITMAP_FIRECRACKER0003 (Christmas firecracker: animated star sprite) --------------------------------

/// ZzzEffect.cpp:3179-3184.
fn firecracker3_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = 15.0;
    e.angle.z = rf(ctx, 360);
}

/// ZzzEffect.cpp:12664-12688: frames `BITMAP_FIRECRACKER0001..0007`.
fn firecracker3_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    let lt = e.life_time as i32;
    let frame = if lt > 15 - 8 {
        // (15 - LifeTime) / 8.0f * 7 truncated to int.
        (((15 - lt) as f32 / 8.0 * 7.0) as i32).min(6)
        // (iFrame == 2: the original's CreateSprite(BITMAP_DS_SHOCK) is commented out.)
    } else {
        e.position.z -= 1.0;
        e.light /= 1.05;
        e.scale *= 1.02;
        6
    };
    ctx.create_sprite(BITMAP_FIRECRACKER0001 + frame, e.position, e.scale, e.light, e.angle.z, 0);
    common::move_tail(e, ctx);
}

// --- BITMAP_SWORD_FORCE (emitter of the BITMAP_JOINT_FORCE sword trail) ----------------------------------

/// ZzzEffect.cpp:3185-3191.
fn sword_force_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 30.0;
    if e.sub_type == 0 || e.sub_type == 1 {
        e.light = Vec3::splat(0.8);
    }
    e.head_angle = Vec3::new(0.0, 0.0, e.angle.z + 45.0);
    e.start_position = e.position;
}

/// ZzzEffect.cpp:12689-12706.
fn sword_force_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    if e.life_time as i32 == 30 {
        let mut position = e.position;
        position.z += 100.0;
        // Subtype 1 passes vPriorColor (1,1,1), which is the joint's default light anyway.
        let sub = match e.sub_type {
            1 => 10,
            0 => 0,
            _ => 8,
        };
        ctx.create_joint_ex(BITMAP_JOINT_FORCE, position, position, e.head_angle, sub, e.owner, 150.0, e.pk_key, e.skill);
    }
    common::move_tail(e, ctx);
}

// --- BITMAP_BLIZZARD (falling ice meteors) ---------------------------------------------------------------

/// ZzzEffect.cpp:3200-3228.
fn blizzard_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = (ctx.rand() % 15 + 15) as f32;
    e.gravity = -20.0;
    // o->Velocity (the animation speed field) is used as a free value here.
    e.velocity = rf(ctx, 360);
    e.light = Vec3::ZERO;
    // (rangeZ is set but unused in the original.)
    let (range_x, range_y) = if e.sub_type == 1 {
        e.scale = 0.5;
        e.gravity -= (ctx.rand() % 20 + 10) as f32;
        (300, 150)
    } else {
        e.scale = 0.0;
        (200, 100)
    };
    e.position.x = e.position.x + (ctx.rand() % range_x) as f32 - range_y as f32;
    e.position.y = e.position.y + (ctx.rand() % range_x) as f32 - range_y as f32;
    e.position.z += 500.0;
    e.position.x += 100.0;
    e.start_position = e.position;
    ctx.play_sound("SOUND_METEORITE01");
}

/// ZzzEffect.cpp:12708-12758.
fn blizzard_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    if e.life_time as i32 <= 15 && (e.sub_type == 0 || e.sub_type == 1) {
        if e.sub_type == 0 {
            e.position.x = e.start_position.x + ((ctx.rand() % 1000) as f32 * 0.01).sin() * 10.0;
            e.position.y = e.start_position.y + ((ctx.rand() % 1000) as f32 * 0.01).sin() * 10.0;
        } else {
            e.position.x = e.start_position.x;
            e.position.y = e.start_position.y;
        }
        e.position.z += e.gravity;
        e.gravity -= 2.0;
        e.start_position.x -= 10.0;

        let fire_sub = if e.sub_type == 0 { 7 } else { 11 };
        ctx.create_particle(BITMAP_FIRE + 2, e.position, e.angle, MOVE_LIGHT, fire_sub, e.scale, None);

        e.light.x += 0.1;
        e.light.y = e.light.x;
        e.light.z = e.light.x;
        let rotation = rf(ctx, 360);
        let scale = (ctx.rand() % 4 + 4) as f32 * 0.2;
        ctx.create_sprite(BITMAP_SHINY + 1, e.position, scale, e.light, rotation, 0);
        let rotation = rf(ctx, 360);
        ctx.create_sprite(BITMAP_LIGHT, e.position, 1.0, e.light, rotation, 0);

        if e.sub_type == 1 {
            let height = ctx.terrain_height(e.position.x, e.position.y);
            if e.position.z < height {
                let light = Vec3::new(0.24, 0.28, 0.8);
                let mut position = e.position;
                position.z += 50.0;
                let s = (ctx.rand() % 32 + 80) as f32 * 0.025;
                ctx.create_particle(BITMAP_SMOKE, position, e.angle, light, 11, s, None);
                if ctx.rand() % 5 == 0 {
                    ctx.create_effect(MODEL_ICE_SMALL, position, e.angle, e.light, 0, None, -1, 0, 0, 0.0);
                }
            }
        }
    }
    common::move_tail(e, ctx);
}

// --- BITMAP_SHOTGUN --------------------------------------------------------------------------------------

/// ZzzEffect.cpp:3229-3263.
fn shotgun_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = 10.0;
    e.velocity = 1.0;
    e.direction = Vec3::new(0.0, -30.0, 0.0);
    // `Position` (the argument) is the position before this offset.
    let origin = e.position;
    e.position += rotate(e.angle, Vec3::new(0.0, -20.0, 50.0));

    for p1 in [Vec3::new(-20.0, -20.0, 60.0), Vec3::new(30.0, -20.0, 60.0)] {
        let pos = origin + rotate(e.angle, p1);
        let mut angle = e.angle;
        for i in 0..20 {
            angle.x = e.angle.x + (ctx.rand() % 20) as f32 + 5.0;
            angle.y += (i * 18) as f32;
            ctx.create_joint(BITMAP_JOINT_SPARK, pos, pos, angle, 1, None, 10.0);
        }
    }
}

/// ZzzEffect.cpp:12759-12786.
fn shotgun_step(e: &mut Effect, ctx: &mut Ctx) {
    let luminosity = common::luminosity(e, ctx);
    let lt = e.life_time as i32;
    let light = Vec3::new(luminosity * 0.5, luminosity * 0.5, luminosity * 0.8);
    ctx.add_terrain_light(e.position.x, e.position.y, light, 2.0);

    let light = Vec3::ONE;
    let mut angle = e.angle;
    angle.z += 90.0;
    e.scale = ((15 - lt) as f32 / 20.0) * (ctx.rand() % 3 + 2) as f32;

    e.start_position = e.position + rotate(angle, Vec3::new(0.0, 20.0, 0.0));
    ctx.create_particle(BITMAP_FIRE + 2, e.start_position, e.angle, light, 10, e.scale, None);
    e.start_position = e.position + rotate(angle, Vec3::new(0.0, -20.0, 0.0));
    ctx.create_particle(BITMAP_FIRE + 2, e.start_position, e.angle, light, 10, e.scale, None);

    if lt == 1 {
        common::create_bomb2(ctx, e.position, false, 0, 0.0);
    }
    common::move_tail(e, ctx);
}

// --- BITMAP_FLARE_FORCE (emitter of BITMAP_FLARE_FORCE trails) -------------------------------------------

/// ZzzEffect.cpp:3644-3673. Only subtype 0 sets `LifeTime` (to 0); the others keep whatever the
/// slot had, which the engine starts at 0: every subtype lives a single `MoveEffect`.
fn flare_force_create(e: &mut Effect, ctx: &mut Ctx) {
    // `Position`/`Angle` (the arguments) equal o->Position/o->Angle here.
    let (p, a) = (e.position, e.angle);
    match e.sub_type {
        0 => {
            e.life_time = 0.0;
            if e.owner.is_some() {
                ctx.create_joint(BITMAP_FLARE_FORCE, p, p, a, 1, e.owner, 100.0);
                ctx.create_joint(BITMAP_FLARE_FORCE, p, p, a, 0, e.owner, 250.0);
                ctx.create_joint(BITMAP_FLARE_FORCE, p, p, a, 2, e.owner, 100.0);
                ctx.create_joint(BITMAP_FLARE_FORCE, p, p, a, 3, e.owner, 100.0);
                ctx.create_joint(BITMAP_FLARE_FORCE, p, p, a, 4, e.owner, 100.0);
            }
        }
        1 => {
            for sub in 5..=7 {
                ctx.create_joint_ex(BITMAP_FLARE_FORCE, p, p, a, sub, e.owner, 20.0, e.pk_key, e.skill);
            }
        }
        2..=4 => {
            ctx.create_joint(BITMAP_FLARE_FORCE, p, p, a, 8 + (e.sub_type - 2), e.owner, 100.0);
            ctx.create_joint(BITMAP_FLARE_FORCE, p, p, a, 0, e.owner, 150.0);
        }
        5..=7 => {
            ctx.create_joint(BITMAP_FLARE_FORCE, p, p, a, 11 + (e.sub_type - 5), e.owner, 100.0);
            ctx.create_joint(BITMAP_FLARE_FORCE, p, p, a, 1, e.owner, 100.0);
        }
        _ => {}
    }
}

/// ZzzEffect.cpp:13350-13351: nothing but the common tail.
fn flare_force_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    common::move_tail(e, ctx);
}

// --- BITMAP_TWLIGHT (twilight ground ring) ---------------------------------------------------------------

/// ZzzEffect.cpp:3749-3764.
fn twilight_create(e: &mut Effect, _ctx: &mut Ctx) {
    match e.sub_type {
        0..=2 => {
            e.life_time = 80.0;
            e.scale = 8.0;
        }
        3 => {
            e.life_time = 30.0;
            e.alpha = 0.0;
            e.pk_key = 0;
            // user[0..3] = EyeRight.
            e.user[0] = e.light.x;
            e.user[1] = e.light.y;
            e.user[2] = e.light.z;
        }
        _ => {}
    }
}

/// ZzzEffect.cpp:13405-13449.
fn twilight_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    let lt = e.life_time as i32;
    match e.sub_type {
        0 => {
            e.scale -= 0.1;
            e.angle.z += 10.0;
        }
        1 => {
            e.scale -= 0.1;
            e.angle.z += 5.0;
        }
        2 => {
            e.scale -= 0.1;
            e.angle.z += 15.0;
        }
        3 => {
            // The original dereferences o->Owner unconditionally.
            if let Some(p) = ctx.target_position(e.owner) {
                e.position = p;
            }
            e.scale -= 0.15;
            e.angle.z += 10.0;
            eye_fade(e, lt);
        }
        _ => {}
    }
    common::move_tail(e, ctx);
}

// --- BITMAP_SHOCK_WAVE (shock wave ground ring) ----------------------------------------------------------

/// ZzzEffect.cpp:3765-3847.
fn shock_wave_create(e: &mut Effect, ctx: &mut Ctx) {
    match e.sub_type {
        0 => {
            e.life_time = 30.0;
            e.scale = 20.0;
        }
        1 | 2 | 11 => {
            e.life_time = 20.0;
            e.scale = ((ctx.rand() % 10) as f32 + 10.0) / 10.0;
        }
        3 => {
            e.life_time = 15.0;
            e.scale = 1.0;
        }
        4 => {
            e.life_time = 10.0;
            e.scale = ((ctx.rand() % 6) as f32 + 6.0) / 10.0;
        }
        5 => {
            e.life_time = 20.0;
            e.scale = 9.0;
        }
        6 => {
            e.life_time = 50.0;
            e.scale = 9.0;
        }
        7 => {
            e.life_time = 10.0;
            e.scale = 1.0;
        }
        8 => {
            e.life_time = 10.0;
            e.scale = ((ctx.rand() % 10) as f32 + 10.0) / 5.0;
        }
        9 => {
            e.life_time = 5.0;
            e.scale = 5.0;
        }
        10 => {
            e.life_time = 1.0;
            e.scale = 2.0;
            e.light *= 2.3;
        }
        12 => {
            e.life_time = 10.0;
            // o->Scale = Scale: the engine already holds the argument (0.9 if it was <= 0, where
            // the original would keep 0).
        }
        13 => {
            e.life_time = 40.0;
            e.scale = ((ctx.rand() % 10) as f32 + 20.0) / 7.0;
        }
        14 => {
            e.life_time = 30.0;
            e.alpha = 0.0;
            e.pk_key = 0;
            // user[0..3] = EyeRight.
            e.user[0] = e.light.x;
            e.user[1] = e.light.y;
            e.user[2] = e.light.z;
        }
        _ => {}
    }
}

/// ZzzEffect.cpp:13450-13573.
fn shock_wave_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    let lt = e.life_time as i32;
    // Subtypes 6 and 14 `break` out of the case before the scale clamp and the light code.
    let mut done = false;
    match e.sub_type {
        0 => e.scale -= 1.0,
        1 | 11 => {
            e.scale += (ctx.rand() % 5) as f32 / 10.0;
            e.position.x += (ctx.rand() % 8) as f32 - 4.0;
            e.position.y += (ctx.rand() % 8) as f32 - 4.0;
        }
        2 => {
            e.scale += (ctx.rand() % 5) as f32 / 40.0;
            e.position.x += (ctx.rand() % 8) as f32 - 4.0;
            e.position.y += (ctx.rand() % 8) as f32 - 4.0;
        }
        3 => e.scale += 2.0,
        4 => e.scale += 0.3,
        5 => e.scale -= 0.4,
        6 => {
            if lt % 8 == 0 {
                ctx.create_effect(BITMAP_SHOCK_WAVE, e.position, e.angle, e.light, 5, None, -1, 0, 0, 0.0);
            }
            done = true;
        }
        7 => e.scale += 2.5,
        8 => e.scale += 1.0,
        9 => {
            e.scale += 0.8;
            // The original dereferences o->Owner unconditionally.
            if let Some(p) = ctx.target_position(e.owner) {
                e.position = p;
            }
        }
        10 => {
            e.scale -= 0.02;
            if let Some(p) = ctx.target_position(e.owner) {
                e.position = p;
            }
        }
        12 => {
            if lt as f32 > 4.0 {
                e.scale += (lt as f32 - 4.0) * 0.25;
            }
        }
        13 => e.scale += 0.08,
        14 => {
            if let Some(p) = ctx.target_position(e.owner) {
                e.position = p;
            }
            e.scale -= 0.15;
            eye_fade(e, lt);
            done = true;
        }
        _ => {}
    }
    if !done {
        if e.scale < 0.0 {
            e.scale = 0.0;
        }
        if (0..=3).contains(&e.sub_type) {
            let l = if lt <= 20 { lt as f32 / 20.0 } else { (40 - lt) as f32 / 20.0 };
            e.light = Vec3::splat(l);
        } else if lt < 6 {
            e.light /= 1.3;
        }
    }
    common::move_tail(e, ctx);
}

/// `RenderEffectShadows` for `BITMAP_TWLIGHT` and `BITMAP_SHOCK_WAVE` (ZzzEffect.cpp:19575-19587).
fn shock_wave_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    if e.ty == BITMAP_SHOCK_WAVE && common::in_hellas(ctx.world_active) && e.sub_type != 6 {
        // TODO(original): in Hellas the ring is drawn with RenderWaterTerrain (ZzzEffect.cpp:19577-19580,
        // depth mask off): `CSWaterTerrain::RenderWaterAlphaBitmap` (CSWaterTerrain.cpp:312) lays
        // the same tiles on the Kalima water, each vertex at `m_iWaveHeight[0][cell] + 400`.
        // `water_terrain.rs` does not expose the ripple page 0 to the effects; drawn on the
        // ground here.
    }
    common::terrain_bitmap(out, e.ty, e.position.x, e.position.y, e.scale, e.scale, e.light, -e.angle.z, 1.0, 5.0, Blend::Glow);
}

// --- BITMAP_DAMAGE_01_MONO -------------------------------------------------------------------------------

/// ZzzEffect.cpp:3848-3860.
fn damage_mono_create(e: &mut Effect, _ctx: &mut Ctx) {
    if e.sub_type == 0 {
        e.life_time = 20.0;
        // o->Scale = Scale: the engine already holds the argument (0.9 if it was <= 0, where the
        // original would keep 0).
    } else if e.sub_type == 1 {
        e.life_time = 10.0;
        e.scale = 0.1;
        e.alpha = 1.0;
    }
}

/// ZzzEffect.cpp:13574-13592.
fn damage_mono_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    if e.sub_type == 0 {
        e.scale += 5.0;
    } else if e.sub_type == 1 {
        e.scale += 0.5;
        if e.scale > 3.5 {
            e.light *= 0.5;
        }
    }
    common::move_tail(e, ctx);
}

/// ZzzEffect.cpp:19589-19598.
fn damage_mono_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    if e.sub_type == 0 || e.sub_type == 1 {
        common::terrain_bitmap(out, e.ty, e.position.x, e.position.y, e.scale, e.scale, e.light, -e.angle.z, 1.0, 5.0, Blend::Glow);
    }
}

// --- BITMAP_FLARE (emitter) ------------------------------------------------------------------------------

/// ZzzEffect.cpp:3861-3875.
fn flare_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = if e.sub_type == 3 { 60.0 } else { 30.0 };
}

/// ZzzEffect.cpp:13593-13623.
fn flare_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    let lt = e.life_time as i32;
    match e.sub_type {
        1 => ctx.create_particle(BITMAP_FLARE, e.position, e.angle, MOVE_LIGHT, 11, 1.0, None),
        2 => ctx.create_particle(BITMAP_FLARE_BLUE, e.position, e.angle, MOVE_LIGHT, 1, 1.0, None),
        3 => {
            let light = Vec3::new(0.9, 0.4, 0.1);
            let random = 3.5 + (ctx.rand() % 20 - 10) as f32 * 0.1;
            ctx.create_particle(BITMAP_LIGHT, e.position, e.angle, light, 5, random, None);
        }
        _ => {
            if lt % 2 == 0 {
                let mut position = e.position;
                position.z += 100.0;
                ctx.create_joint(BITMAP_FLARE_BLUE, position, position, e.angle, 14, me(e), 40.0);
                ctx.create_joint(BITMAP_FLARE_BLUE, position, position, e.angle, 15, me(e), 40.0);
                if ctx.rand() % 2 == 0 && lt > 5 && lt < 15 {
                    let position = Vec3::new(e.position.x - 200.0, e.position.y, e.position.z + 700.0);
                    // `Angle` is MoveEffect's uninitialized local here; zero.
                    ctx.create_joint(BITMAP_JOINT_THUNDER, position, e.position, Vec3::ZERO, 0, me(e), 20.0);
                }
            }
        }
    }
    common::move_tail(e, ctx);
}

// --- BITMAP_CRATER (ground crater mark) ------------------------------------------------------------------

/// ZzzEffect.cpp:4363-4384. `StartPosition[0..2]` holds the mark size (tiles). Other subtypes keep
/// the slot's old StartPosition in the original; here it is the position (they are never created).
fn crater_create(e: &mut Effect, _ctx: &mut Ctx) {
    match e.sub_type {
        0 => {
            e.life_time = 60.0;
            e.start_position.x = 4.5;
            e.start_position.y = 4.5;
        }
        1 => {
            e.life_time = 30.0;
            e.start_position.x = 2.5;
            e.start_position.y = 2.5;
        }
        2 => {
            e.life_time = 40.0;
            e.start_position.x = 3.0;
            e.start_position.y = 3.0;
        }
        _ => {}
    }
    e.light = Vec3::ONE;
}

/// ZzzEffect.cpp:14515-14525.
fn crater_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    let lt = e.life_time as i32;
    if lt < 10 {
        e.light.x = lt as f32 / 10.0;
        e.light.y = e.light.x;
        e.light.z = e.light.x;
        e.alpha = e.light.x;
    }
    let range = (e.start_position.x - 1.0) as i32 as f32;
    ctx.add_terrain_light(e.position.x, e.position.y, Vec3::splat(-0.5), range);
    common::move_tail(e, ctx);
}

/// ZzzEffect.cpp:19600-19603: `EnableAlphaTest` and the mark sized by `StartPosition`.
fn crater_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    common::terrain_bitmap(out, e.ty, e.position.x, e.position.y, e.start_position.x, e.start_position.y, e.light, 0.0, 1.0, 5.0, Blend::AlphaTest);
}

// --- BITMAP_CHROME_ENERGY2 (ground glow) -----------------------------------------------------------------

/// ZzzEffect.cpp:4386-4393. `StartPosition[0..2]` holds the mark size (tiles).
fn chrome_energy2_create(e: &mut Effect, _ctx: &mut Ctx) {
    if e.sub_type == 0 {
        e.life_time = 30.0;
        e.start_position.x = 1.5;
        e.start_position.y = 1.5;
    }
}

/// ZzzEffect.cpp:14527-14534.
fn chrome_energy2_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    if (e.life_time as i32) < 10 {
        e.light *= 0.8;
    }
    common::move_tail(e, ctx);
}

/// ZzzEffect.cpp:19620-19623.
fn chrome_energy2_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    common::terrain_bitmap(out, e.ty, e.position.x, e.position.y, e.start_position.x, e.start_position.y, e.light, 0.0, 1.0, 5.0, Blend::Glow);
}

// --- BITMAP_JOINT_FORCE (lightning/flash strike emitter) -------------------------------------------------

/// ZzzEffect.cpp:4753-4771.
fn joint_force_create(e: &mut Effect, ctx: &mut Ctx) {
    let angle_z = e.angle.z;
    match e.sub_type {
        0 => {
            e.life_time = 20.0;
            e.direction = Vec3::new(0.0, -550.0, 0.0);
            e.angle = Vec3::new(-90.0, 0.0, angle_z);
            e.head_angle = Vec3::new(0.0, 0.0, rf(ctx, 360));
            ctx.create_joint(BITMAP_JOINT_FORCE, e.position, e.position, e.angle, 3, None, 80.0);
            e.start_position = e.position;
        }
        1 => {
            e.life_time = 20.0;
            e.direction = Vec3::new(0.0, -250.0, 0.0);
            e.angle = Vec3::new(-90.0, 0.0, angle_z);
            e.head_angle = Vec3::new(0.0, 0.0, rf(ctx, 360));
            e.start_position = e.position;
        }
        _ => {}
    }
}

/// ZzzEffect.cpp:14736-14765, which has no `break` and falls through into the `MODEL_SWORD_FORCE`
/// case (14767-14832; MuMain `Move_BITMAP_JOINT_FORCE` calls `Move_MODEL_SWORD_FORCE` too).
fn joint_force_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    let lt = e.life_time as i32;
    if (e.sub_type == 0 || e.sub_type == 1) && lt < 11 && lt % 2 == 0 {
        e.angle = Vec3::new(90.0, 0.0, 0.0);
        e.head_angle.z += 72.0;
        let mut position = e.start_position + rotate(e.head_angle, e.direction);
        if e.sub_type == 0 {
            position.z += (ctx.rand() % 400) as f32 + 700.0;
            ctx.create_joint(BITMAP_FLASH, position, position, e.angle, 5, me(e), 110.0);
        } else {
            position.z += 100.0;
            ctx.create_joint(BITMAP_JOINT_THUNDER + 1, position, position, e.angle, 6, me(e), 80.0);
            ctx.create_joint(BITMAP_JOINT_THUNDER + 1, position, position, e.angle, 6, me(e), 80.0);
        }
    }
    sword_force_fallthrough(e, ctx);
    common::move_tail(e, ctx);
}

/// The `MODEL_SWORD_FORCE` case of `MoveEffect` (ZzzEffect.cpp:14767-14832, with the
/// `PBG_ADD_NEWCHAR_MONK_SKILL` subtypes 2 and 3), reached by `BITMAP_JOINT_FORCE` through the
/// missing `break`. Same code as the case part of `sword_force_step` in `effects_dl_mg1.rs`
/// (private copy: that one also runs `MODEL_SWORD_FORCE`'s own tail, which a `BITMAP_*` type does
/// not have; here the common tail is only `MoveParticle(o, true)`).
fn sword_force_fallthrough(e: &mut Effect, ctx: &mut Ctx) {
    let lt = e.life_time as i32;
    if e.sub_type == 0 || e.sub_type == 2 {
        if lt > 12 {
            e.scale += 0.9;
            e.direction.y -= 2.0;
            let sub = if e.sub_type == 2 { 3 } else { 1 };
            ctx.create_effect(MODEL_SWORD_FORCE, e.position, e.angle, e.light, sub, me(e), -1, 0, 0, 0.0);
        } else {
            e.scale -= 0.05;
            e.blend_mesh_light = lt as f32 / 18.0;
            e.alpha = e.blend_mesh_light;
            e.light = Vec3::splat(e.alpha);
            e.direction.y -= 2.0;

            let mut position = e.position;
            position.x += (ctx.rand() % 30) as f32 - 15.0;
            position.y += (ctx.rand() % 30) as f32 - 15.0;
            position.z -= 100.0;
            for _ in 0..4 {
                let angle = Vec3::new((ctx.rand() % 60 + 60 + 90) as f32, 0.0, e.angle.z);
                ctx.create_joint(BITMAP_JOINT_SPARK, position, position, angle, 0, None, 10.0);
                let sub = if e.sub_type == 2 { 18 } else { 2 };
                ctx.create_particle(BITMAP_FIRE, position, angle, e.light, sub, 1.5, None);
            }
        }
        ctx.add_terrain_light(e.position.x, e.position.y, Vec3::new(1.0, 0.8, 0.6), 1.0);
    } else if e.sub_type == 1 || e.sub_type == 3 {
        e.blend_mesh_light = lt as f32 / 10.0;
        e.alpha = e.blend_mesh_light;
        e.light = Vec3::splat(e.alpha);
    }
}

// --- BITMAP_TARGET_POSITION_EFFECT1/2 (cursor pin on the ground) -----------------------------------------

/// ZzzEffect.cpp:4852-4860.
fn target_pos1_create(e: &mut Effect, _ctx: &mut Ctx) {
    if e.sub_type == 0 {
        e.life_time = 20.0;
        e.scale = 1.2;
    }
}

/// ZzzEffect.cpp:14922-14941.
fn target_pos1_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    if e.sub_type == 0 {
        e.scale -= 0.04;
        if e.scale <= 0.2 {
            e.live = false;
        }
        if (e.life_time as i32) <= 10 {
            e.alpha -= 0.05;
            e.light *= e.alpha;
        }
    }
    common::move_tail(e, ctx);
}

/// ZzzEffect.cpp:4861-4869. `user[0]` = `m_iAnimation` (0 shrinking, 1 growing).
fn target_pos2_create(e: &mut Effect, _ctx: &mut Ctx) {
    if e.sub_type == 0 {
        e.life_time = 30.0;
        e.scale = 1.8;
    }
}

/// ZzzEffect.cpp:14942-14973.
fn target_pos2_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    if e.sub_type == 0 {
        if e.scale >= 1.8 {
            e.user[0] = 0.0;
        } else if e.scale <= 0.8 {
            e.user[0] = 1.0;
        }
        if e.user[0] == 0.0 {
            e.scale -= 0.15;
        } else if e.user[0] == 1.0 {
            e.scale += 0.15;
        }
        if (e.life_time as i32) <= 10 {
            e.alpha -= 0.05;
            e.light *= e.alpha;
        }
    }
    common::move_tail(e, ctx);
}

/// ZzzEffect.cpp:19624-19639.
fn target_pos_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    if e.sub_type == 0 {
        common::terrain_bitmap(out, e.ty, e.position.x, e.position.y, e.scale, e.scale, e.light, 0.0, 1.0, 5.0, Blend::Glow);
    }
}

// --- BITMAP_RING_OF_GRADATION ----------------------------------------------------------------------------

/// ZzzEffect.cpp:6034-6040.
fn ring_gradation_create(e: &mut Effect, _ctx: &mut Ctx) {
    if e.sub_type == 0 {
        e.life_time = 20.0;
    }
}

/// ZzzEffect.cpp:16947-16962.
fn ring_gradation_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    if e.sub_type == 0 && e.life_time as i32 != 20 {
        e.scale += 0.1;
        e.light /= 1.1;
        e.alpha /= 1.1;
    }
    common::move_tail(e, ctx);
}

/// ZzzEffect.cpp:19640-19646.
fn ring_gradation_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    if e.sub_type == 0 {
        common::terrain_bitmap(out, e.ty, e.position.x, e.position.y, e.scale, e.scale, e.light, 0.0, 1.0, 5.0, Blend::Glow);
    }
}

// --- BITMAP_WATERFALL_4 (three orbiting sprites around an owner's bone) ----------------------------------

/// ZzzEffect.cpp:6076-6084.
fn waterfall4_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = 80.0;
    e.scale = (ctx.rand() % 20) as f32 * 0.01 + 0.1;
    e.angle.x = rf(ctx, 360);
    e.distance = (ctx.rand() % 10) as f32 + 5.0;
    e.timer = rf(ctx, 360);
}

/// ZzzEffect.cpp:17014-17054. The owner's bone is `PKKey`. `RenderEffects` (19085-19089) draws
/// nothing (its `CreateSprite` is commented out).
fn waterfall4_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    // TransformByObjectBone(Temp_Pos, o->Owner, o->PKKey).
    // TODO(original): the only caller (GMSwampOfQuiet.cpp) passes a map object as owner, which
    // the engine cannot reference; only character owners work here. With no owner or a bad bone
    // the original leaves Temp_Pos uninitialized: no sprites are made then.
    let bone = e.pk_key;
    let temp = common::owner_actor(e.owner, ctx.actors)
        .filter(|a| bone >= 0 && (bone as usize) < a.bones.len())
        .map(|a| a.bone_world(bone as usize, Vec3::ZERO));

    e.timer += 0.1;
    e.distance += 2.1;
    e.angle.x += 0.7;
    e.light *= 0.95;

    if let Some(t) = temp {
        let (s, c) = e.timer.sin_cos();
        let d = e.distance;
        for i in 0..3 {
            let v_pos = match i {
                0 => Vec3::new(c * d + t.x, t.y, s * d + t.z),
                1 => Vec3::new(s * d + t.x, c * d + t.y, t.z),
                _ => Vec3::new(t.x, s * d + t.y, c * d + t.z),
            };
            ctx.create_sprite(BITMAP_WATERFALL_4, v_pos, e.scale, e.light, e.angle.x, 0);
        }
    }
    common::move_tail(e, ctx);
}
