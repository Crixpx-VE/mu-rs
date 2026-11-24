//! Dark Wizard skills and older model effects (`Effects[]`): ice (`MODEL_ICE`, `MODEL_ICE_SMALL`),
//! snow balls and their pieces (`MODEL_SNOW1..3`), meteors (`MODEL_METEO1/2`,
//! `MODEL_BIG_METEO1..3`), twisters (`MODEL_STORM`, `MODEL_STORM2`), `MODEL_LASER`, Inferno
//! (`MODEL_SKILL_INFERNO`), `MODEL_SKILL_BLAST`, `MODEL_POISON`, `MODEL_BLIZZARD`, Hellfire circles
//! (`MODEL_CIRCLE`, `MODEL_CIRCLE_LIGHT`), `MODEL_MAGIC_CIRCLE1`, `MODEL_PROTECT`,
//! `MODEL_WAVE_FORCE`, the falling staff (`MODEL_STAFF+8`), `MODEL_SKULL`, `MODEL_WARCRAFT`, gate
//! and coffin pieces (`MODEL_GATE`, `MODEL_STONE_COFFIN` and their +1), `MODEL_SHINE`,
//! `MODEL_BLOOD`, `MODEL_DRAGON`, `MODEL_DESAIR`, `MODEL_GHOST`, `MODEL_TREE_ATTACK`,
//! `MODEL_MAGIC_CAPSULE2`, bones and big stones (`MODEL_BONE1/2`, `MODEL_BIG_STONE1/2`),
//! `MODEL_WOOSISTONE`, `MODEL_WATER_WAVE` and `MODEL_BOSS_ATTACK`.
//!
//! Translated from Main 5.2 `ZzzEffect.cpp` (`CheckTargetRange`, `CreateEffect`, `MoveEffect` and
//! `RenderEffects`) and `ZzzObject.cpp` (`RenderObject` special cases and `BodyLight`).
//!
//! Every kind has a `step`, so the engine skips the common `MoveEffect` tail: each step calls
//! [`common::luminosity`] first and [`common::move_tail`] where the original runs the tail.

use bevy::math::Vec3;

use super::common::{self, rf};
use crate::fx::core::*;

/// Tile attribute `TW_NOGROUND` (`_define.h`).
const TW_NOGROUND: u16 = 0x0008;

pub fn register(r: &mut Registry) {
    let k = |create: EffectCreate, step: EffectStep, draw: EffectDraw| EffectKind { create: Some(create), step: Some(step), draw: Some(draw) };
    let dflt: EffectDraw = common::draw_default;
    let obj: EffectDraw = common::draw_object;

    r.effect(MODEL_ICE, k(ice_create, ice_step, dflt));
    r.effect(MODEL_SNOW1, k(snow1_create, snow1_step, dflt));
    for ty in [MODEL_SNOW2, MODEL_SNOW3, MODEL_ICE_SMALL, MODEL_METEO1, MODEL_METEO2, MODEL_BONE1, MODEL_BONE2, MODEL_BIG_STONE1, MODEL_BIG_STONE2] {
        r.effect(ty, k(chain_create, chain_step, dflt));
    }
    r.effect(MODEL_BOSS_ATTACK, k(boss_attack_create, chain_step, dflt));
    for ty in [MODEL_BIG_METEO1, MODEL_BIG_METEO2, MODEL_BIG_METEO3] {
        r.effect(ty, k(big_meteo_create, big_meteo_step, dflt));
    }
    r.effect(MODEL_STORM, k(storm_create, storm_step, storm_draw));
    r.effect(MODEL_STORM2, k(storm2_create, storm2_step, obj));
    r.effect(MODEL_LASER, k(laser_create, laser_step, laser_draw));
    r.effect(MODEL_SKILL_INFERNO, k(inferno_create, inferno_step, dflt));
    r.effect(MODEL_SKILL_BLAST, k(blast_create, blast_step, dflt));
    r.effect(MODEL_POISON, k(poison_create, poison_step, dflt));
    r.effect(MODEL_BLIZZARD, k(blizzard_create, blizzard_step, dflt));
    r.effect(MODEL_CIRCLE, k(circle_create, circle_step, circle_draw));
    r.effect(MODEL_CIRCLE_LIGHT, k(circle_light_create, circle_light_step, circle_light_draw));
    r.effect(MODEL_MAGIC_CIRCLE1, k(magic_circle_create, magic_circle_step, dflt));
    r.effect(MODEL_PROTECT, k(protect_create, protect_step, protect_draw));
    r.effect(MODEL_WAVE_FORCE, k(wave_force_create, wave_force_step, obj));
    r.effect(MODEL_STAFF + 8, k(staff8_create, staff8_step, obj));
    r.effect(MODEL_SKULL, k(skull_create, skull_step, skull_draw));
    r.effect(MODEL_WARCRAFT, k(warcraft_create, warcraft_step, warcraft_draw));
    r.effect(MODEL_GATE, k(gate_create, gate_step, obj));
    r.effect(MODEL_GATE + 1, k(gate_create, gate_step, obj));
    r.effect(MODEL_STONE_COFFIN, k(coffin_create, coffin_step, coffin_draw));
    r.effect(MODEL_STONE_COFFIN + 1, k(coffin_create, coffin_step, coffin_draw));
    r.effect(MODEL_SHINE, k(shine_create, shine_step, common::draw_nothing));
    r.effect(MODEL_BLOOD, k(blood_create, plain_step, dflt));
    r.effect(MODEL_DRAGON, k(dragon_create, dragon_step, obj));
    r.effect(MODEL_DESAIR, k(desair_create, desair_step, desair_draw));
    r.effect(MODEL_GHOST, k(ghost_create, ghost_step, obj));
    r.effect(MODEL_TREE_ATTACK, k(tree_attack_create, tree_attack_step, obj));
    r.effect(MODEL_MAGIC_CAPSULE2, k(capsule_create, capsule_step, capsule_draw));
    r.effect(MODEL_WOOSISTONE, k(woosistone_create, woosistone_step, dflt));
    r.effect(MODEL_WATER_WAVE, k(water_wave_create, water_wave_step, dflt));
}

// --- Helpers ----------------------------------------------------------------------------------------

/// `o->Owner->Live` (an effect owner is live while it exists; no owner counts as dead).
fn owner_live(ctx: &Ctx, owner: Option<Target>) -> bool {
    match owner {
        Some(Target::Actor(_)) => ctx.actor(owner).is_some_and(|a| a.live),
        Some(Target::Effect(id)) => ctx.effects.iter().any(|i| i.id == id),
        None => false,
    }
}

/// `o->Owner->Visible`. A character's comes from its snapshot; an effect owner's `Visible` is
/// not in `EffectInfo`, so an existing effect counts as visible.
fn owner_visible(ctx: &Ctx, owner: Option<Target>) -> bool {
    match owner {
        Some(Target::Actor(_)) => ctx.actor(owner).is_some_and(|a| a.visible),
        Some(Target::Effect(id)) => ctx.effects.iter().any(|i| i.id == id),
        None => false,
    }
}

/// `o->Owner == &Hero->Object` (`Hero` is never NULL in the original, so no owner is not the hero).
fn owner_is_hero(e: &Effect, ctx: &Ctx) -> bool {
    ctx.hero.is_some() && e.owner == ctx.hero.map(Target::Actor)
}

/// `g_isCharacterBuff(o->Owner, eDeBuff_Harden)` (an effect owner has no buffs).
fn owner_hardened(ctx: &Ctx, owner: Option<Target>) -> bool {
    ctx.actor(owner).is_some_and(|a| a.has_buff(eDeBuff_Harden))
}

/// `o->Owner->m_bySkillCount` (Hellfire charge). The original dereferences the owner without a
/// check; with no character owner (or an effect owner) 0 is used.
fn owner_skill_count(ctx: &Ctx, owner: Option<Target>) -> i32 {
    ctx.actor(owner).map_or(0, |a| a.skill_count as i32)
}

/// `MoveHumming(Position, Angle, TargetPosition, Turn)` (ZzzAI.cpp:131).
fn move_humming(position: Vec3, angle: &mut Vec3, target: Vec3, turn: f32) -> f32 {
    let ta = create_angle(position.x, position.y, target.x, target.y);
    angle.z = turn_angle(angle.z, ta, turn);
    let range = position - target;
    let distance = (range.x * range.x + range.y * range.y).sqrt();
    let ta = 360.0 - create_angle(position.z, distance, target.z, 0.0);
    angle.x = turn_angle(angle.x, ta, turn);
    range.length()
}

/// `(float)(rand()%256+64)*k` in local Y.
fn rand_forward(ctx: &mut Ctx, k: f32) -> Vec3 {
    Vec3::new(0.0, (ctx.rand() % 256 + 64) as f32 * k, 0.0)
}

/// `CheckTargetRange(o)` (ZzzEffect.cpp:366), the parts for `MODEL_SNOW1` and `MODEL_WOOSISTONE`.
fn check_target_range(e: &mut Effect, ctx: &mut Ctx) {
    if !owner_live(ctx, e.owner) || !owner_visible(ctx, e.owner) {
        return;
    }
    let Some(to) = ctx.target_position(e.owner) else { return };
    let dx = e.position.x - to.x;
    let dy = e.position.y - to.y;
    let range = (dx * dx + dy * dy).sqrt();
    if range > 100.0 {
        return;
    }
    e.life_time = 1.0;
    match e.ty {
        // ZzzEffect.cpp:417
        MODEL_SNOW1 => {
            for _ in 0..2 {
                let ty = MODEL_SNOW2 + ctx.rand() % 2;
                ctx.create_effect(ty, e.position, e.angle, e.light, 0, None, -1, 0, 0, 0.0);
                ctx.create_particle(BITMAP_SMOKE, e.position, e.angle, e.light, 0, 1.0, None);
            }
            ctx.play_sound("SOUND_BREAK01");
        }
        // ZzzEffect.cpp:427
        MODEL_WOOSISTONE => {
            if range <= 30.0 {
                for _ in 0..20 {
                    ctx.create_effect(MODEL_WOOSISTONE, e.position, e.angle, e.light, 1, None, -1, 0, 0, 0.0);
                    ctx.create_particle(BITMAP_FIRE, e.position, e.angle, e.light, 0, 1.0, Some(Target::Effect(e.id)));
                }
                ctx.play_sound("SOUND_BREAK01");
            } else {
                e.life_time = 100.0;
            }
        }
        _ => {}
    }
}

/// The `MoveEffect` cases that only fall into the common tail.
fn plain_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    common::move_tail(e, ctx);
}

// --- MODEL_ICE ------------------------------------------------------------------------------------

/// ZzzEffect.cpp:2401.
fn ice_create(e: &mut Effect, ctx: &mut Ctx) {
    match e.sub_type {
        0 => {
            e.life_time = 50.0;
            e.scale = 0.8;
            e.velocity = 1.0;
            e.angle.x = 0.0;
            e.blend_mesh = 0;
        }
        1 | 2 => {
            e.life_time = 20.0;
            e.scale = 0.8;
            e.angle.x = -20.0;
            e.blend_mesh = 0;
            e.blend_mesh_light = 0.5;
            e.gravity = 5.0;
            e.head_angle = Vec3::new(0.0, 0.0, rf(ctx, 360));
            for _ in 0..3 {
                let x = e.position.x + (ctx.rand() % 64 - 32) as f32;
                let y = e.position.y + (ctx.rand() % 64 - 32) as f32;
                let z = e.position.z + (ctx.rand() % 128 + 32) as f32;
                ctx.create_particle(BITMAP_SMOKE, Vec3::new(x, y, z), e.angle, e.light, 0, 1.0, None);
            }
            ctx.add_terrain_light(e.position.x, e.position.y, Vec3::new(0.4, 0.3, 0.2), 2.0);
            e.start_position = e.position;
        }
        _ => {}
    }
}

/// ZzzEffect.cpp:12266.
fn ice_step(e: &mut Effect, ctx: &mut Ctx) {
    let luminosity = common::luminosity(e, ctx);
    match e.sub_type {
        0 => {
            if e.animation_frame >= 5.0 {
                e.velocity = 0.0;
                if ctx.rand() % 2 == 0 {
                    let x = e.position.x + (ctx.rand() % 64 - 32) as f32;
                    let y = e.position.y + (ctx.rand() % 64 - 32) as f32;
                    let z = e.position.z + (ctx.rand() % 128 + 32) as f32;
                    ctx.create_particle(BITMAP_SMOKE, Vec3::new(x, y, z), e.angle, e.light, 0, 1.0, None);
                }
                e.alpha -= 0.05;
                if e.alpha < 0.0 {
                    e.live = false;
                }
            }
            let light = Vec3::new(-luminosity * 0.4, -luminosity * 0.3, -luminosity * 0.2);
            ctx.add_terrain_light(e.position.x, e.position.y, light, 2.0);
        }
        1 | 2 => match ctx.target_position(e.owner) {
            // `o->Owner == NULL` (or it vanished).
            None => e.live = false,
            Some(owner_pos) => {
                if owner_live(ctx, e.owner) && owner_hardened(ctx, e.owner) {
                    if let Some((ty, action)) = ctx.actor(e.owner).map(|a| (a.ty, a.action as i32)) {
                        if ty == MODEL_PLAYER {
                            let moving = (PLAYER_WALK_MALE..=PLAYER_RUN_RIDE_WEAPON).contains(&action)
                                || (PLAYER_FLY_RIDE..=PLAYER_FLY_RIDE_WEAPON).contains(&action)
                                || action == PLAYER_RAGE_UNI_RUN
                                || action == PLAYER_RAGE_UNI_RUN_ONE_RIGHT;
                            if moving {
                                e.animation_frame = 0.0;
                                e.hidden_mesh = -2;
                            } else {
                                e.hidden_mesh = -1;
                            }
                        } else if action != 2 {
                            e.hidden_mesh = -1;
                        } else {
                            e.animation_frame = 0.0;
                            e.hidden_mesh = -2;
                        }
                    }
                    e.position = owner_pos;
                    if e.animation_frame >= 4.0 {
                        e.animation_frame = 4.0;
                    }
                    e.life_time = 10.0;
                } else {
                    e.alpha -= 0.05;
                }
                if e.sub_type == 1 {
                    e.head_angle.z += 20.0;
                    e.gravity = (ctx.world_time() * 0.01).sin() * 20.0 + 30.0;
                    let mut position = rotate(e.head_angle, Vec3::new(60.0, 0.0, 0.0)) + owner_pos;
                    position.z += e.gravity;
                    ctx.create_particle(BITMAP_FIRE + 2, position, e.angle, Vec3::ONE, 10, 2.0, None);
                }
            }
        },
        _ => {}
    }
    common::move_tail(e, ctx);
}

// --- MODEL_SNOW1 / MODEL_WOOSISTONE ---------------------------------------------------------------

/// ZzzEffect.cpp:2437.
fn snow1_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.position += rotate(e.angle, Vec3::new(0.0, -40.0, 150.0));
    e.direction.y = -40.0;
    e.direction.z = 10.0;
    e.life_time = 20.0;
    e.scale = 1.2;
}

/// ZzzEffect.cpp:11277.
fn snow1_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    e.direction.z -= 2.5;
    check_target_range(e, ctx);
    common::move_tail(e, ctx);
}

/// ZzzEffect.cpp:2447.
fn woosistone_create(e: &mut Effect, ctx: &mut Ctx) {
    if e.sub_type == 1 {
        e.direction = Vec3::ZERO;
        e.life_time = (ctx.rand() % 16 + 20) as f32;
        e.scale = (ctx.rand() % 13 + 3) as f32 * 0.04;
        e.gravity = (ctx.rand() % 3 + 3) as f32;
        e.angle.z = rf(ctx, 360);
        let p1 = Vec3::new(0.0, (ctx.rand() % 128 + 64) as f32 * 0.1, 0.0);
        e.head_angle = rotate(e.angle, p1);
        e.head_angle.z += 15.0;
    } else {
        e.position += rotate(e.angle, Vec3::new(0.0, -60.0, 150.0));
        e.direction.y = -40.0;
        e.direction.z = 10.0;
        e.life_time = 20.0;
        e.scale = 1.0;
    }
}

/// Bouncing piece (`HeadAngle` as velocity): ZzzEffect.cpp:11284-11303 (also 11076-11095).
fn move_bouncing_piece(e: &mut Effect, ctx: &mut Ctx) {
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

/// ZzzEffect.cpp:11281.
fn woosistone_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    if e.sub_type == 1 {
        move_bouncing_piece(e, ctx);
    } else {
        e.direction.z -= 2.0;
        check_target_range(e, ctx);
    }
    common::move_tail(e, ctx);
}

// --- Fall-through chain: MODEL_BONE1/2, MODEL_BIG_STONE1/2, MODEL_SNOW2/3, MODEL_ICE_SMALL,
// --- MODEL_METEO1/2 (and MODEL_BOSS_ATTACK in MoveEffect) -------------------------------------------

/// `CreateEffect` ZzzEffect.cpp:2861-3014: the `case` labels fall through from `MODEL_BONE1` down
/// to `MODEL_ICE_SMALL`; each type enters at its own label.
fn chain_create(e: &mut Effect, ctx: &mut Ctx) {
    let ty = e.ty;
    let bone_or_big = matches!(ty, MODEL_BONE1 | MODEL_BONE2 | MODEL_BIG_STONE1 | MODEL_BIG_STONE2);
    // MODEL_BONE1 (2861) and MODEL_BONE2 (2863).
    if ty == MODEL_BONE1 {
        e.position.z += 50.0;
    }
    if ty == MODEL_BONE1 || ty == MODEL_BONE2 {
        e.position.z += 100.0;
    }
    // MODEL_BIG_STONE1/2 (2865).
    if bone_or_big && e.sub_type == 5 {
        e.life_time = 60.0;
        e.scale = (ctx.rand() % 4 + 8) as f32 * 0.1;
        e.start_position = e.position;
        if let Some(p) = ctx.target_position(e.owner) {
            e.position = p;
        }
        return;
    }
    // MODEL_SNOW2/3 label (2875), shared with MODEL_STONE1/2.
    if bone_or_big || ty == MODEL_SNOW2 || ty == MODEL_SNOW3 {
        match e.sub_type {
            5 => {
                e.life_time = 60.0;
                e.scale = (ctx.rand() % 4 + 8) as f32 * 0.1;
                return;
            }
            11 => {
                e.life_time = 40.0;
                e.scale = (ctx.rand() % 4 + 8) as f32 * 0.8;
                e.angle.z = rf(ctx, 360);
                return;
            }
            10 => {
                let p1 = rand_forward(ctx, 0.2);
                e.life_time = (ctx.rand() % 16 + 32) as f32;
                e.scale = (ctx.rand() % 4 + 15) as f32 * 0.05;
                e.angle.z = rf(ctx, 360);
                e.direction = rotate(e.angle, p1);
                e.gravity = (ctx.rand() % 16 + 28) as f32;
                return;
            }
            12 => {
                let p1 = rand_forward(ctx, 0.1);
                e.life_time = (ctx.rand() % 16 + 32) as f32;
                e.scale = (ctx.rand() % 4 + 15) as f32 * 0.1;
                e.angle.z = rf(ctx, 360);
                e.direction = rotate(e.angle, p1);
                e.gravity = (ctx.rand() % 16 + 8) as f32;
                return;
            }
            13 | 14 => {
                e.direction = Vec3::ZERO;
                e.life_time = (ctx.rand() % 16 + 20) as f32;
                // `o->Scale *= Scale` (PBG_ADD_NEWCHAR_MONK_SKILL) with the Scale argument; the
                // engine already turned an argument of 0 into 0.9, so that value is used.
                let call_scale = e.scale;
                e.scale = (ctx.rand() % 13 + 3) as f32 * 0.08;
                e.scale *= call_scale;
                e.gravity = (ctx.rand() % 3 + 3) as f32;
                e.angle.z = rf(ctx, 360);
                let p1 = Vec3::new(0.0, (ctx.rand() % 128 + 64) as f32 * 0.1, 0.0);
                e.head_angle = rotate(e.angle, p1);
                e.head_angle.z += 15.0;
                return;
            }
            _ => {}
        }
    }
    // MODEL_ICE_SMALL / MODEL_METEO1 / MODEL_METEO2 label (2939).
    if ty == MODEL_BIG_STONE1 || ty == MODEL_BIG_STONE2 {
        let x = rf(ctx, 128) - 64.0;
        let y = rf(ctx, 128) - 64.0;
        let z = rf(ctx, 180);
        e.position += Vec3::new(x, y, z);
    }
    let mut p1;
    if ty == MODEL_ICE_SMALL {
        e.blend_mesh = 0;
        e.blend_mesh_light = 0.3;
        p1 = rand_forward(ctx, 0.1);
        e.position.z += 50.0;
        if e.sub_type == 13 {
            p1 = rand_forward(ctx, 0.2);
            e.life_time = (ctx.rand() % 16 + 32) as f32;
            e.scale = (ctx.rand() % 4 + 15) as f32 * 0.05;
            e.angle.z = rf(ctx, 360);
            e.direction = rotate(e.angle, p1);
            e.gravity = (ctx.rand() % 16 + 28) as f32;
        }
    } else if e.sub_type == 1 {
        p1 = Vec3::ZERO;
    } else {
        p1 = rand_forward(ctx, 0.1);
    }
    if e.sub_type == 13 {
        e.life_time = (ctx.rand() % 16 + 100) as f32;
    } else {
        e.life_time = (ctx.rand() % 16 + 32) as f32;
    }
    e.scale = (ctx.rand() % 4 + 8) as f32 * 0.1;
    e.angle.z = rf(ctx, 360);
    e.direction = rotate(e.angle, p1);
    e.gravity = (ctx.rand() % 16 + 8) as f32;
}

/// `MODEL_BOSS_ATTACK` (ZzzEffect.cpp:1624).
fn boss_attack_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 50.0;
    e.scale = 0.8;
    e.velocity = 1.0;
    e.angle.x = 0.0;
    e.blend_mesh = 0;
    e.direction.y = -50.0;
    e.gravity = -10.0;
}

/// `MoveEffect` ZzzEffect.cpp:11053-11173 (the same fall-through chain), then the common tail.
fn chain_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    let ty = e.ty;
    let sub = e.sub_type;
    if ty == MODEL_SNOW2 || ty == MODEL_SNOW3 {
        // ZzzEffect.cpp:11057
        if sub == 5 {
            if let Some(p) = ctx.target_position(e.owner) {
                e.position = p + e.start_position;
            }
            common::move_tail(e, ctx);
            return;
        } else if sub == 11 {
            if e.life_time < 10.0 {
                e.alpha *= 0.8;
            }
            common::move_tail(e, ctx);
            return;
        } else if sub == 13 || sub == 14 {
            move_bouncing_piece(e, ctx);
            common::move_tail(e, ctx);
            return;
        }
    }
    // MODEL_BONE1/2, MODEL_BIG_STONE1/2 (11103).
    if matches!(ty, MODEL_BONE1 | MODEL_BONE2 | MODEL_BIG_STONE1 | MODEL_BIG_STONE2) && sub == 5 {
        if !owner_live(ctx, e.owner) {
            e.live = false;
        }
        if let Some(p) = ctx.target_position(e.owner) {
            e.position = p + e.start_position;
        }
        common::move_tail(e, ctx);
        return;
    }
    // MODEL_ICE_SMALL / METEO1 / METEO2 / BOSS_ATTACK (11113).
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
        if (sub == 0 || sub == 12 || sub == 13) && ctx.rand() % 10 == 0 && matches!(ty, MODEL_ICE_SMALL | MODEL_METEO1 | MODEL_METEO2) {
            ctx.create_particle(BITMAP_SMOKE, e.position, e.angle, e.light, 0, 1.0, None);
        }
    } else if sub == 1 {
        e.gravity += 0.5;
    } else {
        e.angle.z += 20.0;
        e.gravity += 0.5;
    }
    common::move_tail(e, ctx);
}

// --- MODEL_BIG_METEO1..3 ------------------------------------------------------------------------------

/// ZzzEffect.cpp:1654.
fn big_meteo_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = 100.0;
    e.scale = (ctx.rand() % 10 + 4) as f32 * 0.1;
    e.direction = Vec3::new(0.0, -15.0 / e.scale, -30.0 / e.scale);
    e.sub_type = 1;
}

/// ZzzEffect.cpp:9835.
fn big_meteo_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    if ctx.terrain_attribute(e.position.x, e.position.y) & TW_NOGROUND != TW_NOGROUND {
        let height = ctx.terrain_height(e.position.x, e.position.y);
        if e.position.z < height {
            e.position.z = height;
            e.live = false;
            let quake = (ctx.rand() % 8 - 8) as f32 * 0.1;
            ctx.set_earthquake(quake);
            for _ in 0..5 {
                let ty = MODEL_METEO1 + ctx.rand() % 2;
                ctx.create_effect(ty, e.position, e.angle, e.light, 0, None, -1, 0, 0, 0.0);
            }
        }
    }
    e.position += e.direction;
    e.angle.x += 10.0 / e.scale;
    ctx.create_particle(BITMAP_SMOKE, e.position, e.angle, e.light, 3, 1.0, None);
    // `return;`: no common tail and no `LifeTime--`.
    e.life_time += 1.0;
}

// --- MODEL_STORM (Twister) --------------------------------------------------------------------------

/// ZzzEffect.cpp:2194.
fn storm_create(e: &mut Effect, ctx: &mut Ctx) {
    match e.sub_type {
        0 => {
            e.life_time = 59.0;
            e.blend_mesh = 0;
            e.direction = Vec3::new(0.0, -10.0, 0.0);
            e.position.z = ctx.terrain_height(e.position.x, e.position.y);
            // TODO(original): o->Weapon = CharacterMachine->PacketSerial++ (network packet
            // counter, not in the engine); `weapon` stays 0 and is the serial of the attack.
        }
        1 => {
            e.life_time = 30.0;
            e.hidden_mesh = -2;
            e.scale = 1.0;
            e.blend_mesh = 0;
            e.direction = Vec3::ZERO;
        }
        2 => {
            e.life_time = 59.0;
            e.hidden_mesh = -2;
            e.scale = 1.0;
            e.blend_mesh = 0;
            e.direction = Vec3::new(0.0, -10.0, 0.0);
            e.light = Vec3::ONE;
        }
        3..=7 => {
            e.life_time = 60.0;
            e.hidden_mesh = -2;
            e.scale = 1.0;
            e.direction = match e.sub_type {
                3 => Vec3::new(0.0, -12.0, 0.0),
                4 => Vec3::new(7.0, -6.0, 0.0),
                5 => Vec3::new(-7.0, -6.0, 0.0),
                6 => Vec3::new(4.0, 5.0, 0.0),
                _ => Vec3::new(-4.0, 5.0, 0.0),
            };
        }
        8 => {
            e.life_time = 100.0;
            e.blend_mesh = 0;
            e.scale = 2.0;
        }
        _ => {}
    }
}

/// ZzzEffect.cpp:10554.
fn storm_step(e: &mut Effect, ctx: &mut Ctx) {
    let luminosity = common::luminosity(e, ctx);
    match e.sub_type {
        0 => {
            e.blend_mesh_light = e.life_time * 0.1;
            e.blend_mesh_tex_coord.x = -e.life_time * 0.1;
            ctx.create_particle(BITMAP_SMOKE, e.position, e.angle, e.light, 3, 1.0, None);
            let angle = Vec3::new(90.0, 0.0, e.angle.z);
            let me = Some(Target::Effect(e.id));
            if ctx.rand() % 2 == 0 {
                let p = Vec3::new(e.position.x - 200.0, e.position.y, e.position.z + 700.0);
                ctx.create_joint(BITMAP_JOINT_THUNDER, p, e.position, angle, 0, me, 10.0);
            }
            if ctx.rand() % 2 == 0 {
                let p = Vec3::new(e.position.x + 200.0, e.position.y, e.position.z + 700.0);
                ctx.create_joint(BITMAP_JOINT_THUNDER, p, e.position, angle, 0, me, 10.0);
            }
            e.position.z = ctx.terrain_height(e.position.x, e.position.y);
            if ctx.rand() % 4 == 0 {
                let ty = MODEL_STONE1 + ctx.rand() % 2;
                ctx.create_effect(ty, e.position, e.angle, e.light, 2, None, -1, 0, 0, 0.0);
            }
            let light = Vec3::new(-luminosity * 0.4, -luminosity * 0.3, -luminosity * 0.2);
            ctx.add_terrain_light(e.position.x, e.position.y, light, 5.0);
            // TODO(original): during Castle Siege (battleCastle::IsBattleCastleStart) on a
            // TW_NOATTACKZONE tile: Velocity = 0, Direction = 0, LifeTime /= 5 and `break` before
            // the attack below (ZzzEffect.cpp:10579-10589); there is no siege state yet.
            // ZzzEffect.cpp:10590-10592 (the result is not used).
            if (e.life_time as i32) % 15 == 0 && owner_is_hero(e, ctx) {
                ctx.attack_character_range(e.skill, e.position, 150.0, e.weapon, e.pk_key);
            }
        }
        1 => {
            e.blend_mesh_light = e.life_time * 0.01;
            e.blend_mesh_tex_coord.x = -e.life_time * 0.1;
            e.angle.z += (ctx.rand() % 30) as f32 + 30.0;
            let mut position = e.position;
            position.z += 100.0;
            ctx.create_particle(BITMAP_SMOKE, position, e.angle, e.light, 3, 1.0, None);
            ctx.create_particle(BITMAP_BUBBLE, position, e.angle, e.light, 3, 0.1, None);
            // (the original builds a blue Light here but passes o->Light)
            ctx.add_terrain_light(e.position.x, e.position.y, e.light, 5.0);
        }
        2 => {
            e.blend_mesh_light = e.life_time * 0.1;
            e.blend_mesh_tex_coord.x = -e.life_time * 0.1;
            e.gravity = rf(ctx, 360);
            let mut position = e.position;
            position.z += 100.0;
            ctx.create_particle(BITMAP_SMOKE, position, e.angle, e.light, 3, 1.0, None);
            ctx.create_particle(BITMAP_BUBBLE, position, e.angle, e.light, 3, 0.1, None);
            ctx.add_terrain_light(e.position.x, e.position.y, e.light, 5.0);
        }
        3..=7 => {
            let quake = (ctx.rand() % 8 - 4) as f32 * 0.1;
            ctx.set_earthquake(quake);
            ctx.create_particle(BITMAP_SMOKE, e.position, e.angle, e.light, 28, 1.0, None);
            ctx.create_particle(BITMAP_SMOKE, e.position, e.angle, e.light, 29, 1.0, None);
            ctx.create_particle(BITMAP_SMOKE, e.position, e.angle, e.light, 30, 1.0, None);
            if ctx.rand() % 2 == 0 {
                let ty = MODEL_STONE1 + ctx.rand() % 2;
                ctx.create_effect(ty, e.position, e.angle, e.light, 2, None, -1, 0, 0, 0.0);
            }
        }
        8 => {
            e.blend_mesh_light = e.life_time * 0.1;
            e.blend_mesh_tex_coord.x = -e.life_time * 0.1;
            ctx.add_terrain_light(e.position.x, e.position.y, e.light, 5.0);
        }
        _ => {}
    }
    common::move_tail(e, ctx);
}

/// `RenderEffects` ZzzEffect.cpp:18309: subtypes 3-7 are a ground mark, the rest `RenderObject`.
fn storm_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    if (3..=7).contains(&e.sub_type) {
        let rotation = ctx.world_time;
        common::terrain_bitmap(out, BITMAP_POUNDING_BALL, e.position.x, e.position.y, 2.0, 2.0, Vec3::splat(0.3), rotation, 1.0, 5.0, Blend::Glow);
    } else {
        out.models.push(ModelDraw::of_effect(e));
    }
}

// --- MODEL_STORM2 ---------------------------------------------------------------------------------

/// ZzzEffect.cpp:2259.
fn storm2_create(e: &mut Effect, ctx: &mut Ctx) {
    match e.sub_type {
        0 => {
            e.angle.x = 90.0;
            e.life_time = 35.0;
            e.blend_mesh = 0;
            e.scale = 0.5 + (ctx.rand() % 10) as f32 / 100.0;
        }
        1 => {
            e.life_time = 60.0;
            e.blend_mesh = 0;
            e.gravity = (ctx.rand() % 10) as f32 + 30.0;
        }
        2 => {
            e.life_time = 100.0;
            e.angle.x = 90.0;
            e.blend_mesh = 0;
            e.scale = 1.0;
        }
        _ => {}
    }
}

/// ZzzEffect.cpp:10663.
fn storm2_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    match e.sub_type {
        0 | 2 => {
            e.angle.y += 40.0;
            if e.life_time <= 5.0 {
                e.light /= 1.01;
            }
            if e.sub_type == 2 {
                let ty = MODEL_STONE1 + ctx.rand() % 2;
                ctx.create_effect(ty, e.position, e.angle, e.light, 2, None, -1, 0, 0, 0.0);
            }
        }
        1 => {
            let light = Vec3::new(0.3, 0.5, 0.6);
            for i in 0..2 {
                let angle = Vec3::new(e.angle.x, e.angle.y, e.angle.z / 3.0 + 180.0 * i as f32);
                let position = rotate(angle, Vec3::new(0.0, 200.0, 0.0)) + e.position;
                ctx.create_particle(BITMAP_CLUD64, position, e.angle, light, 1, 1.0, None);
            }
            let angle = Vec3::new(90.0, 0.0, 0.0);
            let p1x = e.position.x + (ctx.rand() % 600) as f32 - 300.0;
            let p1y = e.position.y + (ctx.rand() % 600) as f32 - 300.0;
            let pos1 = Vec3::new(p1x, p1y, e.position.z);
            let p2x = pos1.x + (ctx.rand() % 100) as f32 - 50.0;
            let p2y = pos1.y + (ctx.rand() % 100) as f32 - 50.0;
            let pos2 = Vec3::new(p2x, p2y, pos1.z);
            ctx.create_joint(BITMAP_JOINT_THUNDER, pos1, pos2, angle, 20, Some(Target::Effect(e.id)), 20.0);
            let p2x = pos1.x + (ctx.rand() % 100) as f32 - 50.0;
            let p2y = pos1.y + (ctx.rand() % 100) as f32 - 50.0;
            let pos2 = Vec3::new(p2x, p2y, pos1.z);
            let ty = MODEL_STONE1 + ctx.rand() % 2;
            ctx.create_effect(ty, pos2, e.angle, light, 2, None, -1, 0, 0, 0.0);
            e.angle.z += e.gravity;
            e.light /= 1.02;
            e.alpha += 0.01;
            if e.light.x <= 0.05 {
                e.live = false;
            }
        }
        _ => {}
    }
    common::move_tail(e, ctx);
}

// --- MODEL_LASER ------------------------------------------------------------------------------------

/// ZzzEffect.cpp:2009. Both branches set `RenderType = RENDER_DARK` (see [`laser_draw`]).
fn laser_create(e: &mut Effect, ctx: &mut Ctx) {
    if e.sub_type == 0 || e.sub_type == 3 {
        e.life_time = 1.0;
        e.blend_mesh = 0;
        e.blend_mesh_light = e.light.x;
        e.scale = 1.3;
    } else {
        e.life_time = 30.0;
        e.velocity = 1.0;
        e.blend_mesh = 0;
        e.blend_mesh_light = 1.0;
        e.scale = 1.3;
        e.position.z += 150.0;
        e.direction = Vec3::new(0.0, -1.0, 0.0);
        e.light = Vec3::new(1.0, 0.0, 0.0);
        e.angle = Vec3::new(30.0, 0.0, e.angle.z);
        ctx.create_joint(BITMAP_JOINT_FORCE, e.position, e.position, e.angle, 1, Some(Target::Effect(e.id)), 180.0);
    }
}

/// ZzzEffect.cpp:9874.
fn laser_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    if e.sub_type != 0 && e.sub_type != 3 {
        e.direction.y -= e.velocity;
        e.velocity += 1.0;
    }
    common::move_tail(e, ctx);
}

/// `RenderEffects` `default:` → `RenderObject`, whose first branch is `o->RenderType ==
/// RENDER_DARK`: `RenderBody(RENDER_TEXTURE|RENDER_DARK, ..., HiddenMesh)` (ZzzObject.cpp:489-492).
fn render_dark(e: &Effect, out: &mut DrawList) {
    let mut d = ModelDraw::of_effect(e);
    d.render_flags = RENDER_TEXTURE | RENDER_DARK;
    out.models.push(d);
}

/// `MODEL_LASER` is always `RENDER_DARK` (ZzzEffect.cpp:2016, 2025).
fn laser_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    render_dark(e, out);
}

// --- MODEL_SKILL_INFERNO ----------------------------------------------------------------------------

/// ZzzEffect.cpp:1534.
fn inferno_create(e: &mut Effect, ctx: &mut Ctx) {
    let pk_key = e.pk_key;
    let skill_index = e.skill;
    e.blend_mesh = -2;
    e.scale = 0.9;
    e.velocity = 0.5;
    // Subtypes 2, 6, 8 and 10: HiddenMesh = SkillIndex, Distance = PKKey, Scale = PKKey/100.
    let from_args = |e: &mut Effect| {
        e.hidden_mesh = skill_index;
        e.skill = 0;
        e.pk_key = -1;
        e.distance = pk_key as f32;
        e.scale = pk_key as f32 / 100.0;
    };
    match e.sub_type {
        0 => {
            e.light = Vec3::splat(0.8);
            e.life_time = 15.0;
        }
        1 => {
            e.light = Vec3::new(1.0, 0.5, 0.2);
            e.life_time = 35.0;
        }
        2 => {
            e.life_time = 12.0;
            from_args(e);
            e.blend_mesh_light = 0.1;
        }
        8 => {
            e.life_time = 12.0;
            from_args(e);
            e.blend_mesh_light = 0.1;
            e.velocity *= 4.0;
        }
        3 => {
            e.light = Vec3::splat(0.8);
            e.life_time = 4.0;
            e.hidden_mesh = 1;
            e.scale = 0.02;
            e.velocity = 1.0;
            e.angle = Vec3::new(90.0, 0.0, 0.0);
        }
        4 => {
            e.light = Vec3::splat(0.8);
            e.life_time = 35.0;
            e.hidden_mesh = 1;
            e.scale = 0.1;
            e.gravity = 0.0;
        }
        5 => {
            e.light = Vec3::splat(0.8);
            e.life_time = 15.0;
        }
        6 => {
            e.life_time = 5.0;
            from_args(e);
            e.velocity = 0.1;
            e.blend_mesh_light = 0.1;
        }
        9 => {
            e.life_time = 13.0;
            e.light = Vec3::new(0.1, 1.0, 0.2);
            e.scale = 0.45;
            e.blend_mesh_light = 0.1;
            e.gravity = 2.0;
        }
        10 => {
            e.life_time = 12.0;
            from_args(e);
            e.blend_mesh_light = 0.1;
        }
        _ => {}
    }
    // ZzzEffect.cpp:1617-1622 (the result is not used).
    if owner_is_hero(e, ctx) && e.sub_type < 2 {
        // TODO(original): o->Weapon = CharacterMachine->PacketSerial (network packet counter,
        // not in the engine); `weapon` stays 0.
        ctx.attack_character_range(e.skill, e.position, 400.0, e.weapon, e.pk_key);
    }
}

/// ZzzEffect.cpp:9744.
fn inferno_step(e: &mut Effect, ctx: &mut Ctx) {
    let luminosity = common::luminosity(e, ctx);
    match e.sub_type {
        2 => e.scale += 0.04,
        8 => {
            e.scale += 0.04;
            if e.life_time < 10.0 {
                e.light *= 0.8;
            }
        }
        3 => {
            if let Some(p) = ctx.target_position(e.owner) {
                e.position = p;
            }
            // `o->Owner->Owner->Position[0] + 150`.
            if let Some(Target::Effect(id)) = e.owner {
                let owner_owner = ctx.effects.iter().find(|i| i.id == id).and_then(|i| i.owner);
                if let Some(p) = ctx.target_position(owner_owner) {
                    e.position.x = p.x + 150.0;
                }
            }
        }
        4 => {
            if let Some(p) = ctx.target_position(e.owner) {
                e.position = p;
            }
            e.scale += 0.003;
            e.gravity += 0.8 * e.scale * 30.0;
            e.position.z += e.gravity;
        }
        5 => {
            e.position.z += 2.0;
            e.angle.z += 20.0;
            e.blend_mesh_light = e.life_time / 20.0;
            let light = Vec3::new(luminosity * 0.1, luminosity * 0.3, luminosity * 0.8);
            ctx.add_terrain_light(e.position.x, e.position.y, light, 5.0);
        }
        6 => {
            e.scale += 0.01;
            e.blend_mesh_light = e.life_time / 5.0 * 0.1;
            let light = Vec3::new(luminosity * 0.8, luminosity * 0.3, luminosity * 0.1);
            ctx.add_terrain_light(e.position.x, e.position.y, light, 2.0);
        }
        _ => {}
    }
    if e.sub_type < 4 {
        e.blend_mesh_light = e.life_time / 20.0;
        if e.sub_type != 2 {
            let light = Vec3::splat(-luminosity * 0.5);
            ctx.add_terrain_light(e.position.x, e.position.y, light, 5.0);
        }
    } else if e.sub_type == 8 {
        e.blend_mesh_light = e.life_time / 20.0;
    } else if e.sub_type == 9 {
        e.blend_mesh_light = e.life_time / 80.0;
        e.position.z += e.gravity;
        e.light.x /= 1.04;
        e.light.z /= 1.04;
        e.alpha -= 0.01;
    } else if e.sub_type == 10 {
        e.scale += 0.04;
        // `VectorCopy(o->Light, b->BodyLight)`: the draw already uses o->Light as body light.
    }
    common::move_tail(e, ctx);
}

// --- MODEL_SKILL_BLAST (Lightning/Meteor-like blast from the sky) -------------------------------

/// ZzzEffect.cpp:1511.
fn blast_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = 30.0;
    e.blend_mesh = 0;
    e.scale = (ctx.rand() % 8 + 10) as f32 * 0.1;
    e.position.x += (ctx.rand() % 100 + 200) as f32;
    e.position.y += (ctx.rand() % 100 - 50) as f32;
    e.position.z += (ctx.rand() % 500 + 300) as f32;
    e.direction = Vec3::new(0.0, 0.0, -50.0 - (ctx.rand() % 50) as f32);
    e.angle = Vec3::new(0.0, 20.0, 0.0);
    // (`EyeLeft = Position`: only read by the joint, which follows the effect position here.)
    ctx.create_joint(BITMAP_JOINT_ENERGY, e.position, e.position, e.angle, 5, Some(Target::Effect(e.id)), 100.0);
    // TODO(original): o->Weapon = CharacterMachine->PacketSerial (network packet counter, not in
    // the engine); `weapon` stays 0 and is the serial of the attack in `blast_step`.
}

/// ZzzEffect.cpp:9693.
fn blast_step(e: &mut Effect, ctx: &mut Ctx) {
    let luminosity = common::luminosity(e, ctx);
    let height = ctx.terrain_height(e.position.x, e.position.y);
    if e.position.z < height {
        e.position.z = height;
        e.direction = Vec3::ZERO;
        let position = Vec3::new(e.position.x, e.position.y, e.position.z + 80.0);
        for _ in 0..6 {
            let ty = MODEL_STONE1 + ctx.rand() % 2;
            ctx.create_effect(ty, e.position, e.angle, e.light, 0, None, -1, 0, 0, 0.0);
        }
        ctx.create_particle(BITMAP_SHINY + 4, position, e.angle, Vec3::ONE, 0, 1.0, None);
        ctx.create_particle(BITMAP_EXPLOTION, position, e.angle, Vec3::ONE, 0, 1.0, None);
        // ZzzEffect.cpp:9707-9708 (the result is not used).
        if owner_is_hero(e, ctx) {
            ctx.attack_character_range(e.skill, e.position, 150.0, e.weapon, e.pk_key);
        }
        e.live = false;
    }
    let light = Vec3::new(luminosity * 0.2, luminosity * 0.4, luminosity * 1.0);
    ctx.add_terrain_light(e.position.x, e.position.y, light, 2.0);
    common::move_tail(e, ctx);
}

// --- MODEL_POISON -------------------------------------------------------------------------------------

/// ZzzEffect.cpp:3113.
fn poison_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.blend_mesh = 1;
    e.life_time = 40.0;
    e.scale = 1.0;
}

/// ZzzEffect.cpp:9865.
fn poison_step(e: &mut Effect, ctx: &mut Ctx) {
    let luminosity = common::luminosity(e, ctx);
    e.blend_mesh_light = e.life_time * 0.1;
    e.alpha = e.life_time * 0.1;
    let light = Vec3::new(luminosity * 0.3, luminosity * 1.0, luminosity * 0.6);
    ctx.add_terrain_light(e.position.x, e.position.y, light, 2.0);
    common::move_tail(e, ctx);
}

// --- MODEL_BLIZZARD -----------------------------------------------------------------------------------

/// ZzzEffect.cpp:3324.
fn blizzard_create(e: &mut Effect, ctx: &mut Ctx) {
    if e.sub_type == 0 || e.sub_type == 2 {
        e.blend_mesh = -2;
        e.life_time = (ctx.rand() % 15 + 15) as f32;
        e.gravity = -20.0;
        e.velocity = rf(ctx, 360);
        e.light = Vec3::ZERO;
        let (range_x, range_y) = (300, 150);
        e.scale = 0.5;
        e.gravity -= (ctx.rand() % 30 + 10) as f32;
        e.position.x = e.position.x + (ctx.rand() % range_x) as f32 - range_y as f32;
        e.position.y = e.position.y + (ctx.rand() % range_x) as f32 - range_y as f32;
        e.position.z += 600.0;
        e.position.x += 100.0;
        e.start_position = e.position;
        if e.sub_type == 2 {
            let me = Some(Target::Effect(e.id));
            let scale = 60.0 + rf(ctx, 10);
            ctx.create_joint(BITMAP_JOINT_THUNDER + 1, e.position, e.position, e.angle, 4, me, scale);
            if ctx.rand() % 2 == 0 {
                let scale = 60.0 + rf(ctx, 10);
                ctx.create_joint(BITMAP_JOINT_THUNDER + 1, e.position, e.position, e.angle, 4, me, scale);
            }
        }
    } else if e.sub_type == 1 {
        e.blend_mesh = -2;
        e.life_time = 20.0;
        e.velocity = 0.0;
    }
}

/// ZzzEffect.cpp:12859.
fn blizzard_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    let light = Vec3::ONE;
    match e.sub_type {
        0 => {
            e.position.x = e.start_position.x + (rf(ctx, 1000) * 0.01).sin() * 10.0;
            e.position.y = e.start_position.y + (rf(ctx, 1000) * 0.01).sin() * 10.0;
            e.position.z += e.gravity;
            e.gravity -= rf(ctx, 5);
            e.start_position.x -= 10.0;
            ctx.create_particle(BITMAP_SMOKE, e.position, e.angle, light, 0, 1.5, None);
            if ctx.rand() % 2 == 0 {
                ctx.create_particle(BITMAP_ENERGY, e.position, e.angle, light, 1, 0.5, None);
            } else {
                ctx.create_particle(BITMAP_FIRE + 2, e.position, e.angle, light, 7, e.scale, None);
            }
            e.light.x += 0.1;
            e.light.y = e.light.x;
            e.light.z = e.light.x;
            let s = (ctx.rand() % 4 + 4) as f32 * 0.2;
            let rot = rf(ctx, 360);
            ctx.create_sprite(BITMAP_SHINY + 1, e.position, s, e.light, rot, 0);
            let rot = rf(ctx, 360);
            ctx.create_sprite(BITMAP_LIGHT, e.position, 1.0, e.light, rot, 0);
            let height = ctx.terrain_height(e.position.x, e.position.y);
            if e.position.z < height {
                let smoke = Vec3::new(0.24, 0.28, 0.8);
                let mut position = e.position;
                position.z += 50.0;
                let s = (ctx.rand() % 32 + 80) as f32 * 0.025;
                ctx.create_particle(BITMAP_SMOKE, position, e.angle, smoke, 11, s, None);
                if ctx.rand() % 5 == 0 {
                    ctx.create_effect(MODEL_ICE_SMALL, position, e.angle, e.light, 0, None, -1, 0, 0, 0.0);
                }
                ctx.create_effect(MODEL_BLIZZARD, position, e.angle, e.light, 1, None, e.pk_key, 0, 0, 0.0);
                e.live = false;
            }
        }
        1 => {
            e.blend_mesh_light /= 1.1;
            if e.life_time as i32 == 18 && e.pk_key == 1 {
                ctx.play_sound("SOUND_SUDDEN_ICE2");
            }
        }
        2 => {
            e.position.x = e.start_position.x + (rf(ctx, 1000) * 0.01).sin() * 50.0;
            e.position.y = e.start_position.y + (rf(ctx, 1000) * 0.01).sin() * 50.0;
            e.position.z += e.gravity;
            e.gravity -= rf(ctx, 5);
            e.start_position.x -= 10.0;
            let height = ctx.terrain_height(e.position.x, e.position.y);
            if e.position.z > height {
                if ctx.rand() % 2 == 0 {
                    ctx.create_particle(BITMAP_FIRE + 2, e.position, e.angle, light, 7, e.scale, None);
                }
                e.light.x += 0.1;
                e.light.y = e.light.x;
                e.light.z = e.light.x;
                let s = (ctx.rand() % 4 + 4) as f32 * 0.2;
                let rot = rf(ctx, 360);
                ctx.create_sprite(BITMAP_SHINY + 1, e.position, s, e.light, rot, 0);
                let rot = rf(ctx, 360);
                ctx.create_sprite(BITMAP_LIGHT, e.position, 1.0, e.light, rot, 0);
            }
        }
        _ => {}
    }
    common::move_tail(e, ctx);
}

// --- MODEL_CIRCLE (Hellfire) ----------------------------------------------------------------------

/// ZzzEffect.cpp:2347.
fn circle_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = 45.0;
    e.blend_mesh = 0;
    match e.sub_type {
        0 => {
            // ZzzEffect.cpp:2352-2356 (the result is not used).
            if owner_is_hero(e, ctx) {
                // TODO(original): o->Weapon = CharacterMachine->PacketSerial++ (network packet
                // counter, not in the engine); `weapon` stays 0.
                ctx.attack_character_range(e.skill, e.position, 300.0, e.weapon, e.pk_key);
            }
        }
        1 | 4 => {
            e.life_time = 45.0;
            e.scale = 1.0;
            e.hidden_mesh = -2;
        }
        2 => {
            e.life_time = 250.0;
            e.scale = 1.0;
        }
        3 => {
            e.life_time = 30.0;
            e.scale = 1.0;
        }
        _ => {}
    }
}

/// ZzzEffect.cpp:10895.
fn circle_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    let lt = e.life_time as i32;
    e.blend_mesh_light = e.life_time * 0.1;
    let me = Some(Target::Effect(e.id));
    match e.sub_type {
        1 => {
            let count = owner_skill_count(ctx, e.owner);
            if lt > 44 - count {
                let mut position = e.position;
                position.z += 100.0;
                for i in 0..36 {
                    let mut angle = Vec3::new(-10.0, 0.0, i as f32 * (10.0 + rf(ctx, 10)));
                    ctx.create_joint_ex(BITMAP_JOINT_SPIRIT, position, position, angle, 6, me, 60.0, 0, 0);
                    if lt == 44 - count + 1 {
                        angle.z = i as f32 * 10.0;
                        ctx.create_joint_ex(BITMAP_JOINT_SPIRIT, position, position, angle, 7, me, 60.0, 0, 0);
                    }
                }
            }
        }
        2 => {
            e.blend_mesh_light = if lt > 240 { (250 - lt) as f32 * 0.1 } else { e.life_time * 0.1 };
        }
        3 => {
            e.blend_mesh_light = if lt > 10 { (30 - lt) as f32 * 0.1 } else { e.life_time * 0.1 };
        }
        4 => {
            let count = owner_skill_count(ctx, e.owner);
            if lt > 44 - count {
                let mut position = e.position;
                position.z += 100.0;
                for i in 0..36 {
                    let angle = Vec3::new(-10.0, 0.0, i as f32 * 10.0);
                    ctx.create_joint_ex(BITMAP_JOINT_SPIRIT, position, position, angle, 22, me, 2.0, 0, 0);
                    ctx.create_joint_ex(BITMAP_JOINT_SPIRIT, position, position, angle, 23, me, 1.75, 0, 0);
                }
            }
        }
        _ => {}
    }
    common::move_tail(e, ctx);
}

/// `RenderObject` (ZzzObject.cpp:1520-1525): subtypes 2 and 3 are
/// `RenderBody(RENDER_TEXTURE, ..., HiddenMesh, BITMAP_MAGIC_EMBLEM)` with body light (0.5, 0.5, 1).
fn circle_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    let mut d = ModelDraw::of_effect(e);
    if e.sub_type == 2 || e.sub_type == 3 {
        d.light = Vec3::new(0.5, 0.5, 1.0);
        d.render_flags = RENDER_TEXTURE;
        d.texture = BITMAP_MAGIC_EMBLEM;
    }
    out.models.push(d);
}

// --- MODEL_CIRCLE_LIGHT -----------------------------------------------------------------------------

/// ZzzEffect.cpp:2375.
fn circle_light_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 40.0;
    e.blend_mesh = 0;
    match e.sub_type {
        // `RenderType = RENDER_DARK` (ZzzEffect.cpp:2379-2380): see `circle_light_draw`.
        1 => {}
        2 => {
            e.life_time = 40.0;
            e.scale = 1.0;
        }
        3 => {
            e.life_time = 250.0;
            e.scale = 1.0;
        }
        4 => {
            e.life_time = 20.0;
            e.scale = 1.0;
        }
        _ => {}
    }
}

/// ZzzEffect.cpp:10962.
fn circle_light_step(e: &mut Effect, ctx: &mut Ctx) {
    let luminosity = common::luminosity(e, ctx);
    let lt = e.life_time as i32;
    if e.sub_type != 3 && e.sub_type != 4 {
        let value = 4;
        e.blend_mesh_light = if lt >= 30 { (40 - lt) as f32 * 0.1 } else { e.life_time * 0.1 };
        e.blend_mesh_tex_coord.x = -e.life_time * 0.01;
        let quake = (ctx.rand() % 6 - 6) as f32 * 0.1;
        ctx.set_earthquake(quake);
        if ctx.rand() % value == 0 {
            let p = Vec3::new(0.0, rf(ctx, 300), 0.0);
            let angle = Vec3::new(0.0, 0.0, rf(ctx, 360));
            let position = rotate(angle, p) + e.position;
            let ty = MODEL_STONE1 + ctx.rand() % 2;
            ctx.create_effect(ty, position, e.angle, e.light, 1, None, -1, 0, 0, 0.0);
        }
        let light = Vec3::new(luminosity * 1.0, luminosity * 0.8, luminosity * 0.2);
        ctx.add_terrain_light(e.position.x, e.position.y, light, 4.0);
    } else if e.sub_type == 3 {
        let value = 2;
        e.blend_mesh_light = if lt >= 240 { (250 - lt) as f32 * 0.1 } else { e.life_time * 0.1 };
        e.blend_mesh_light = e.blend_mesh_light.min(0.5);
        e.blend_mesh_tex_coord.x = -e.life_time * 0.01;
        if lt > 30 && ctx.rand() % value == 0 {
            let p = Vec3::new(0.0, rf(ctx, 200), 0.0);
            let mut angle = Vec3::new(0.0, 0.0, rf(ctx, 360));
            let mut position = rotate(angle, p) + e.position;
            ctx.create_particle(BITMAP_FLARE_BLUE, position, e.angle, e.light, 0, 1.0, None);
            if lt > 40 {
                position.z += 600.0;
                angle.z = 45.0;
                ctx.create_joint(BITMAP_FLARE_BLUE, position, position, angle, 19, None, 40.0);
            }
        }
        let light = Vec3::splat(e.blend_mesh_light);
        ctx.add_terrain_light(e.position.x, e.position.y, light, 4.0);
    } else {
        let value = 5;
        e.blend_mesh_light = if lt >= 10 { (20 - lt) as f32 * 0.1 } else { e.life_time * 0.1 };
        e.blend_mesh_light = e.blend_mesh_light.min(0.5);
        e.blend_mesh_tex_coord.x = -e.life_time * 0.01;
        if ctx.rand() % value == 0 && lt > 5 {
            let p = Vec3::new(0.0, rf(ctx, 100), 0.0);
            let mut angle = Vec3::new(0.0, 0.0, rf(ctx, 360));
            let mut position = rotate(angle, p) + e.position;
            ctx.create_particle(BITMAP_FLARE_BLUE, position, e.angle, e.light, 0, 1.0, None);
            position.z += 600.0;
            angle.z = 45.0;
            ctx.create_joint(BITMAP_FLARE_BLUE, position, position, angle, 19, None, 40.0);
        }
        let light = Vec3::splat(e.blend_mesh_light);
        ctx.add_terrain_light(e.position.x, e.position.y, light, 4.0);
    }
    common::move_tail(e, ctx);
}

/// `RenderObject`: subtype 1 has `RenderType = RENDER_DARK` (ZzzObject.cpp:489-492); subtypes 3
/// and 4 have body light (0.1, 0.1, 10) (ZzzObject.cpp:1514-1519, `RenderType` 0 there).
fn circle_light_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    if e.sub_type == 1 {
        render_dark(e, out);
        return;
    }
    let mut d = ModelDraw::of_effect(e);
    if e.sub_type == 3 || e.sub_type == 4 {
        d.light = Vec3::new(0.1, 0.1, 10.0);
    }
    out.models.push(d);
}

// --- MODEL_MAGIC_CIRCLE1 ----------------------------------------------------------------------------

/// ZzzEffect.cpp:1633.
fn magic_circle_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 30.0;
    e.scale = 0.7;
    e.blend_mesh = -2;
    if e.sub_type == 2 {
        e.life_time = 15.0;
        e.velocity = 0.3;
        e.scale = 0.7;
    } else if e.sub_type == 1 {
        e.life_time = 20.0;
        e.velocity = 0.1;
        e.hidden_mesh = 0;
    } else {
        e.velocity = 0.1;
    }
}

/// ZzzEffect.cpp:9816.
fn magic_circle_step(e: &mut Effect, ctx: &mut Ctx) {
    let luminosity = common::luminosity(e, ctx);
    if e.sub_type == 2 {
        e.scale += 0.015;
        if let Some(p) = ctx.target_position(e.owner) {
            e.position = p;
        }
        e.light = Vec3::new(0.1, 0.0, 0.0);
    } else if e.sub_type == 1 {
        e.scale += 0.01;
        let light = Vec3::new(luminosity * 1.0, luminosity * 0.0, luminosity * 0.0);
        ctx.add_terrain_light(e.position.x, e.position.y, light, 3.0);
    } else {
        if let Some(p) = ctx.target_position(e.owner) {
            e.position = p;
        }
        e.scale += 0.01;
    }
    common::move_tail(e, ctx);
}

// --- MODEL_PROTECT ------------------------------------------------------------------------------------

/// ZzzEffect.cpp:1662.
fn protect_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 10000.0;
    e.velocity = 0.3;
    e.blend_mesh = 0;
}

/// ZzzEffect.cpp:9857.
fn protect_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    e.blend_mesh_light = 1.3;
    if let Some(p) = ctx.target_position(e.owner) {
        e.position = p;
    }
    e.angle.z += 10.0;
    if !owner_live(ctx, e.owner) || !owner_visible(ctx, e.owner) {
        e.live = false;
    }
    common::move_tail(e, ctx);
}

/// `BodyLight` (ZzzObject.cpp:213): pulsing white, ignoring `o->Light`.
fn protect_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    let mut d = ModelDraw::of_effect(e);
    let l = (ctx.world_time * 0.003).sin() * 0.5 + 0.5;
    d.light = Vec3::splat(l);
    out.models.push(d);
}

// --- MODEL_WAVE_FORCE ---------------------------------------------------------------------------------

/// ZzzEffect.cpp:1524.
fn wave_force_create(e: &mut Effect, _ctx: &mut Ctx) {
    let pk_key = e.pk_key;
    e.blend_mesh = -2;
    e.scale = 0.9;
    e.velocity = 0.5;
    e.life_time = 12.0;
    e.skill = 0;
    e.pk_key = -1;
    e.scale = pk_key as f32 / 100.0;
    e.blend_mesh_light = 0.1;
}

/// ZzzEffect.cpp:9739.
fn wave_force_step(e: &mut Effect, ctx: &mut Ctx) {
    let luminosity = common::luminosity(e, ctx);
    e.blend_mesh_light = e.life_time / 20.0;
    let light = Vec3::splat(-luminosity * 0.5);
    ctx.add_terrain_light(e.position.x, e.position.y, light, 5.0);
    common::move_tail(e, ctx);
}

// --- MODEL_STAFF+8 (the falling staff) ----------------------------------------------------------------

/// ZzzEffect.cpp:1486.
fn staff8_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 30.0;
    e.blend_mesh = -2;
    e.scale = 1.0;
    e.position.z += 280.0;
    e.angle.x += 20.0;
    e.direction = Vec3::new(0.0, -80.0, -10.0);
}

/// ZzzEffect.cpp:11753. (Outside `MODEL_BIRD01..MODEL_SKILL_END`: the tail only moves it.)
fn staff8_step(e: &mut Effect, ctx: &mut Ctx) {
    let luminosity = common::luminosity(e, ctx);
    let height = ctx.terrain_height(e.position.x, e.position.y);
    if e.position.z < height {
        let mut position = e.position;
        position.z += 80.0;
        ctx.create_particle(BITMAP_EXPLOTION, position, e.angle, Vec3::ONE, 0, 1.0, None);
        for _ in 0..6 {
            let ty = MODEL_STONE1 + ctx.rand() % 2;
            ctx.create_effect(ty, e.position, e.angle, e.light, 0, None, -1, 0, 0, 0.0);
        }
        e.live = false;
    }
    let light = Vec3::new(luminosity * 0.2, luminosity * 0.4, luminosity * 1.0);
    ctx.add_terrain_light(e.position.x, e.position.y, light, 2.0);
    common::move_tail(e, ctx);
}

// --- MODEL_SKULL ----------------------------------------------------------------------------------------

/// ZzzEffect.cpp:2477. `user[0..3]` = `m_vDeadPosition`.
fn skull_create(e: &mut Effect, ctx: &mut Ctx) {
    if e.sub_type == 0 {
        e.life_time = 1.0;
        e.scale = 2.3;
    } else if e.sub_type == 1 {
        e.life_time = 49.0;
        e.scale = 2.0;
        e.blend_mesh = -2;
        e.blend_mesh_light = 0.5;
        e.head_angle = Vec3::ZERO;
        e.direction = Vec3::new(0.0, -45.0, 0.0);
        e.start_position = e.position;
        set_dead_position(e);
        // (`EyeLeft`/`EyeRight` = Position: read by the joints, which follow the effect here.)
        let me = Some(Target::Effect(e.id));
        ctx.create_joint(BITMAP_JOINT_ENERGY, e.position, e.position, e.angle, 10, me, 30.0);
        ctx.create_joint(BITMAP_JOINT_ENERGY, e.position, e.position, e.angle, 11, me, 30.0);
    }
}

/// `VectorCopy(o->Position, o->m_vDeadPosition)` into `user[0..3]`.
fn set_dead_position(e: &mut Effect) {
    e.user[0] = e.position.x;
    e.user[1] = e.position.y;
    e.user[2] = e.position.z;
}

/// ZzzEffect.cpp:11311.
fn skull_step(e: &mut Effect, ctx: &mut Ctx) {
    let luminosity = common::luminosity(e, ctx);
    if e.sub_type == 1 && e.owner.is_some() {
        if (e.life_time as i32) % 10 == 0 {
            set_dead_position(e);
        }
        if let Some(p) = ctx.target_position(e.owner) {
            e.start_position = p;
        }
        e.start_position.z += 200.0;
        move_humming(e.position, &mut e.angle, e.start_position, 10.0);
        e.head_angle.x += (ctx.rand() % 32 - 16) as f32 * 0.2;
        e.head_angle.z += (ctx.rand() % 32 - 16) as f32 * 0.8;
        e.angle.x += e.head_angle.x;
        e.angle.z += e.head_angle.z;
        e.head_angle.x *= 0.6;
        e.head_angle.z *= 0.8;
        if e.life_time < 10.0 {
            e.alpha /= 1.1;
            e.blend_mesh_light /= 1.2;
            if e.angle.x <= 80.0 {
                e.angle.x += 5.0;
            }
        } else {
            let height = ctx.terrain_height(e.position.x, e.position.y);
            if e.position.z < height + 100.0 {
                e.head_angle.x = 0.0;
                e.angle.x = -5.0;
            }
            if e.position.z > height + 400.0 {
                e.head_angle.x = 0.0;
                e.angle.x = 5.0;
            }
        }
        ctx.create_particle(BITMAP_FIRE, e.position, e.angle, Vec3::ONE, 5, 1.2, None);
        let light = Vec3::new(luminosity, luminosity * 0.3, luminosity * 0.1);
        ctx.add_terrain_light(e.position.x, e.position.y, light, 4.0);
    }
    common::move_tail(e, ctx);
}

/// `RenderObject` (ZzzObject.cpp:1549-1566): orange body light and two red glowing eyes at bones 2
/// and 3 of the skull model (`EyeLeft`/`EyeRight`, `TransformPosition(BoneTransform[2|3], 0)`).
fn skull_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    let mut d = ModelDraw::of_effect(e);
    d.light = Vec3::new(1.0, 0.6, 0.3);
    out.models.push(d);
    // Without the model file the eyes fall back to the effect position.
    let eye_left = ctx.effect_bone_world(e, 2, Vec3::ZERO).unwrap_or(e.position);
    let eye_right = ctx.effect_bone_world(e, 3, Vec3::ZERO).unwrap_or(e.position);
    for eye in [eye_left, eye_right] {
        common::sprite(out, ctx, BITMAP_LIGHT, eye, 1.0, Vec3::new(1.0, 0.0, 0.0), 0.0, Some(Blend::Glow));
    }
    // The BITMAP_SHINY+1 sprites use rotation rand()%360; a draw function cannot call rand()
    // (it would change the simulation's sequence), so 0 is used.
    for eye in [eye_left, eye_right] {
        common::sprite(out, ctx, BITMAP_SHINY + 1, eye, 0.5, Vec3::splat(0.5), 0.0, Some(Blend::Glow));
    }
}

// --- MODEL_WARCRAFT (Hell Gate) -------------------------------------------------------------------------

/// ZzzEffect.cpp:3104.
fn warcraft_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 50.0;
    e.scale = 0.7;
    e.blend_mesh = if e.sub_type == 0 { -3 } else { -4 };
    e.angle = Vec3::new(0.0, 0.0, 45.0);
}

/// ZzzEffect.cpp:12614.
fn warcraft_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    if let Some(p) = ctx.target_position(e.owner) {
        e.position = p;
    }
    common::move_tail(e, ctx);
}

/// `RenderEffects` `default:` → `RenderObject` → `RenderHellasObjectMesh` (GMHellas.cpp:522-534,
/// called from ZzzObject.cpp:1128 on every map): body light (1, 0.1, 0.1) for subtype 1, else 1,
/// and `RenderBody(RENDER_TEXTURE, Alpha, BlendMesh, BlendMeshLight, U, V)` without `HiddenMesh`.
/// (The mesh-0-only `RenderMesh` of ZzzObject.cpp:10406 is `RenderPartObjectEdge`, the selection
/// outline, which effects do not use.)
fn warcraft_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    let mut d = ModelDraw::of_effect(e);
    d.light = if e.sub_type == 1 { Vec3::new(1.0, 0.1, 0.1) } else { Vec3::ONE };
    d.hidden_mesh = -1;
    d.render_flags = RENDER_TEXTURE;
    out.models.push(d);
}

// --- MODEL_GATE / MODEL_GATE+1 (castle gate pieces) ----------------------------------------------------

/// ZzzEffect.cpp:3264.
fn gate_create(e: &mut Effect, ctx: &mut Ctx) {
    let p1 = Vec3::new(0.0, (ctx.rand() % 128 + 64) as f32 * 0.1, 0.0);
    e.position.z += 50.0;
    e.life_time = (ctx.rand() % 16 + 32) as f32;
    e.scale = (ctx.rand() % 4 + 8) as f32 * 0.1;
    e.angle.z = rf(ctx, 360);
    e.direction = rotate(e.angle, p1);
    e.gravity = (ctx.rand() % 5 + 2) as f32;
    if e.ty == MODEL_GATE && e.sub_type == 0 {
        e.sub_type = 1;
        e.gravity += rf(ctx, 5);
    } else if e.sub_type == 2 {
        e.scale *= 0.6;
    }
    e.angle.x = rf(ctx, 360);
    e.angle.y = rf(ctx, 360);
    e.angle.z = rf(ctx, 360);
}

/// ZzzEffect.cpp:12788.
fn gate_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    e.position += e.direction;
    e.direction *= 0.9;
    e.position.z += e.gravity;
    if e.sub_type == 1 {
        e.direction *= 1.1;
    }
    e.gravity -= 4.0;
    let height = ctx.terrain_height(e.position.x, e.position.y);
    if e.position.z < height {
        e.position.z = height;
        e.gravity = -e.gravity * 0.5;
        e.life_time -= 5.0;
        e.angle.x -= e.scale * 128.0;
    } else {
        e.angle.x -= e.scale * 16.0;
    }
    if ctx.rand() % 10 == 0 {
        // The original passes MoveEffect's uninitialized local `Position`; the piece's own
        // position is used instead.
        ctx.create_particle(BITMAP_SMOKE + 1, e.position, e.angle, Vec3::ONE, 0, 1.0, None);
    }
    common::move_tail(e, ctx);
}

// --- MODEL_STONE_COFFIN / MODEL_STONE_COFFIN+1 ---------------------------------------------------------

/// ZzzEffect.cpp:3289.
fn coffin_create(e: &mut Effect, ctx: &mut Ctx) {
    let p1 = Vec3::new(0.0, (ctx.rand() % 128 + 32) as f32 * 0.1, 0.0);
    e.position.z += 50.0;
    e.life_time = (ctx.rand() % 16 + 32) as f32;
    e.scale = (ctx.rand() % 4 + 8) as f32 * 0.1;
    e.angle.z = rf(ctx, 360);
    e.direction = rotate(e.angle, p1);
    e.gravity = (ctx.rand() % 5 + 2) as f32;
    if e.ty == MODEL_STONE_COFFIN + 1 && e.sub_type == 0 {
        e.sub_type = 1;
        e.gravity += rf(ctx, 5);
    }
    e.angle.x = rf(ctx, 360);
    e.angle.y = rf(ctx, 360);
    e.angle.z = rf(ctx, 360);
}

/// ZzzEffect.cpp:12817.
fn coffin_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    e.position += e.direction;
    e.direction *= 0.9;
    e.position.z += e.gravity;
    if e.sub_type == 1 {
        e.direction *= 1.1;
    }
    e.gravity -= 3.0;
    let height = ctx.terrain_height(e.position.x, e.position.y);
    if e.position.z < height {
        e.position.z = height;
        e.gravity = -e.gravity * 0.2;
        e.life_time -= 5.0;
        e.angle.x -= e.scale * 128.0;
    } else {
        e.angle.x -= e.scale * 16.0;
    }
    e.alpha = e.life_time / 10.0;
    if ctx.rand() % 10 == 0 {
        // The original passes MoveEffect's uninitialized local `Position`; see `gate_step`.
        ctx.create_particle(BITMAP_SMOKE + 1, e.position, e.angle, Vec3::ONE, 0, 1.0, None);
    }
    common::move_tail(e, ctx);
}

/// `RenderObject` (ZzzObject.cpp:1428-1450).
fn coffin_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    let base = ModelDraw::of_effect(e);
    let pass = |light: Vec3, flags: u32, texture: i32| ModelDraw { light, render_flags: flags, texture, ..base };
    if e.sub_type == 2 || e.sub_type == 3 {
        let light = if e.sub_type == 2 { Vec3::new(0.1, 0.3, 0.6) } else { Vec3::new(0.1, 0.6, 0.3) };
        out.models.push(pass(light, RENDER_BRIGHT | RENDER_CHROME, BITMAP_CHROME));
    } else {
        out.models.push(pass(base.light, RENDER_TEXTURE, -1));
        out.models.push(pass(Vec3::ONE, RENDER_BRIGHT | RENDER_CHROME, BITMAP_CHROME));
        out.models.push(pass(Vec3::new(0.3, 0.3, 1.0), RENDER_BRIGHT | RENDER_METAL, BITMAP_CHROME));
    }
}

// --- MODEL_SHINE --------------------------------------------------------------------------------------

/// ZzzEffect.cpp:3309.
fn shine_create(e: &mut Effect, ctx: &mut Ctx) {
    if e.sub_type == 0 {
        e.life_time = 50.0;
        for j in 0..10 {
            let pos = Vec3::new(
                e.position.x + (j - 5) as f32 * 12.0 - 30.0,
                e.position.y + (j - 5) as f32 * 12.0 + 30.0,
                e.position.z - 300.0,
            );
            ctx.create_joint(BITMAP_FLARE, pos, e.position, e.angle, 16, e.owner, 120.0);
        }
    }
}

/// ZzzEffect.cpp:12846.
fn shine_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    if e.sub_type == 0 && e.owner.is_some() && e.life_time > 10.0 {
        let x = rf(ctx, 128) - 64.0 - 50.0;
        let mut position = Vec3::new(x, x + 100.0, 0.0);
        if let Some(p) = ctx.target_position(e.owner) {
            position += p;
        }
        position.z += 360.0;
        let scale = (ctx.rand() % 30 + 50) as f32 / 100.0;
        ctx.create_particle(BITMAP_SHINY, position, e.angle, e.light, 2, scale, None);
    }
    common::move_tail(e, ctx);
}

// --- MODEL_BLOOD --------------------------------------------------------------------------------------

/// ZzzEffect.cpp:2397 (no `MoveEffect` case: only the common tail).
fn blood_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 10.0;
    e.blend_mesh = 0;
}

// --- MODEL_DRAGON ---------------------------------------------------------------------------------------

/// ZzzEffect.cpp:504. `user[0]` = `CollisionRange`.
fn dragon_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.user[0] = 1.0;
    e.kind = 0;
    e.timer = 0.0;
    e.velocity = 1.0;
    e.angle.y = 0.0;
    e.life_time = 1000.0;
    e.gravity = 1.0;
    e.distance = 1.0;
    e.position.z += 3400.0;
    e.direction = Vec3::new(0.0, -35.0, 0.0);
    e.start_position = e.position;
    e.light = Vec3::ONE;
}

/// ZzzEffect.cpp:6933.
fn dragon_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    let temp = (e.position - e.start_position).length();
    if e.angle.z >= 360.0 || e.angle.z <= -360.0 {
        e.angle.z = 0.0;
    }
    if e.angle.y >= 360.0 || e.angle.y <= -360.0 {
        e.angle.y = 0.0;
    }
    e.timer = match e.kind {
        0 => 30.0,
        1 => -30.0,
        _ => 0.0,
    };
    if e.timer > e.angle.y {
        e.angle.y += 0.5;
    }
    if e.timer < e.angle.y {
        e.angle.y -= 0.5;
    }
    if (-800.0..=800.0).contains(&temp) {
        e.kind = 2;
        e.distance = 0.0;
        e.angle.z += 0.5 * e.distance / 2.0;
        e.user[0] = 1.0;
    } else {
        if e.user[0] != 0.0 {
            e.timer = e.angle.z;
            e.gravity *= -1.0;
            e.user[0] = 0.0;
        }
        if e.timer >= e.angle.z - 60.0 {
            e.angle.z += 0.5 * e.gravity * 3.0;
            e.kind = 1;
        } else if e.timer <= e.angle.z + 60.0 {
            e.angle.z += 0.5 * e.gravity * 3.0;
            e.kind = 0;
        } else {
            e.user[0] = 1.0;
            e.kind = 2;
        }
    }
    if (e.life_time as i32) <= 10 {
        e.life_time = 1000.0;
    }
    if e.animation_frame >= 40.0 {
        e.animation_frame = 0.0;
        e.current_action = 0;
    }
    common::move_tail(e, ctx);
}

// --- MODEL_DESAIR ---------------------------------------------------------------------------------------

/// ZzzEffect.cpp:556.
fn desair_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 52.0;
    e.scale = 1.4;
}

/// ZzzEffect.cpp:8500.
fn desair_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    // TODO(original): while `Joints[o->m_sTargetIndex]` is live, copies that joint's Position
    // and Angle every tick and only then emits the feathers; the engine cannot address a joint
    // by index, so the effect stays where it was created and the joint is assumed live.
    if (e.life_time as i32) % 10 == 0 {
        ctx.create_effect(MODEL_FEATHER, e.position, e.angle, e.light, 2, None, -1, 0, 0, 1.4);
        ctx.create_effect(MODEL_FEATHER, e.position, e.angle, e.light, 3, None, -1, 0, 0, 1.4);
    }
    common::move_tail(e, ctx);
    // `Vector(1.f,1.f,1.f,o->Light)` of its `RenderObject` (ZzzObject.cpp:1533), which runs after
    // each tick: the feathers of the next ticks get light 1.
    e.light = Vec3::ONE;
}

/// `RenderObject` (ZzzObject.cpp:1531-1536): `o->Light` = 1 (moved to `desair_step`), body light
/// 1 and `RenderBody(RENDER_TEXTURE|RENDER_DARK, ..., HiddenMesh)`.
fn desair_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    let mut d = ModelDraw::of_effect(e);
    d.light = Vec3::ONE;
    d.render_flags = RENDER_TEXTURE | RENDER_DARK;
    out.models.push(d);
}

// --- MODEL_GHOST (Aida) ------------------------------------------------------------------------------------

/// ZzzEffect.cpp:687.
fn ghost_create(e: &mut Effect, ctx: &mut Ctx) {
    e.kind = 0;
    e.blend_mesh_light = 1.0;
    e.velocity = 1.0;
    e.life_time = 1000.0;
    e.gravity = 1.0;
    e.distance = 1.0;
    e.position.z += rf(ctx, 200) - 100.0;
    e.position.y += rf(ctx, 200) - 100.0;
    e.position.x += rf(ctx, 200) - 100.0;
    e.angle.z += rf(ctx, 360);
    e.direction = Vec3::new(0.0, rf(ctx, 8) * 0.1 - 5.0, 0.0);
    e.scale += rf(ctx, 5) / 40.0;
    e.start_position = e.position;
    e.light = Vec3::new(0.01, 0.01, 0.03);
}

/// ZzzEffect.cpp:7357.
fn ghost_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    if ctx.rand() % 40 == 1 {
        e.velocity *= -1.0;
        e.gravity = e.velocity * (rf(ctx, 3) + 5.0);
    }
    if ctx.rand() % 40 == 1 {
        e.velocity *= -1.0;
        e.distance = e.velocity * (rf(ctx, 3) + 1.0);
    }
    let temp = (e.position - e.start_position).length();
    if temp >= 500.0 && e.kind == 0 {
        e.life_time = 50.0;
        e.kind = 1;
    }
    let wave = (((ctx.world_time() as i32) % 10000) as f32 * 0.0001).sin();
    e.angle.z += wave * e.gravity;
    e.angle.x += wave * e.distance / 3.0;
    e.light = Vec3::new(e.blend_mesh_light / 10.0, e.blend_mesh_light / 10.0, e.blend_mesh_light / 4.0);
    ctx.create_particle(BITMAP_LIGHT, e.position, e.angle, e.light, 9, e.scale, None);
    common::move_tail(e, ctx);
}

// --- MODEL_TREE_ATTACK (Aida) ------------------------------------------------------------------------------

/// ZzzEffect.cpp:705.
fn tree_attack_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.position.z += 20.0;
    e.life_time = 20.0;
    e.light_enable = false;
    e.scale = 0.05;
}

/// ZzzEffect.cpp:7388.
fn tree_attack_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    e.scale += 0.1;
    if e.scale >= 1.0 {
        e.alpha -= 0.07;
        e.scale = 1.0;
    }
    common::move_tail(e, ctx);
}

// --- MODEL_MAGIC_CAPSULE2 ----------------------------------------------------------------------------------

/// ZzzEffect.cpp:751.
fn capsule_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 20.0;
    e.blend_mesh = 0;
    e.blend_mesh_light = 1.0;
}

/// ZzzEffect.cpp:7544.
fn capsule_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    if let Some(p) = ctx.target_position(e.owner) {
        e.position = p;
    }
    if e.life_time < 10.0 {
        e.blend_mesh_light = (e.life_time as i32) as f32 * 0.1;
    }
    common::move_tail(e, ctx);
}

/// `RenderEffects` ZzzEffect.cpp:18568 → `RenderObject`: subtype 1 (ZzzObject.cpp:1601-1605) first
/// draws `RENDER_BRIGHT|RENDER_CHROME4` with `HiddenMesh` 1, then `RENDER_TEXTURE` with its own.
fn capsule_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    let d = ModelDraw::of_effect(e);
    if e.sub_type == 1 {
        out.models.push(ModelDraw { hidden_mesh: 1, render_flags: RENDER_BRIGHT | RENDER_CHROME4, ..d });
        out.models.push(ModelDraw { render_flags: RENDER_TEXTURE, ..d });
    } else {
        out.models.push(d);
    }
}

// --- MODEL_WATER_WAVE (Kalima sea monster) -------------------------------------------------------------------

/// ZzzEffect.cpp:2748.
fn water_wave_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.blend_mesh = -2;
    e.blend_mesh_light = 0.2;
    e.scale = 0.4;
    e.life_time = 20.0;
    e.gravity = 120.0;
    e.velocity = -40.0;
    e.direction = Vec3::new(0.0, e.velocity, 0.0);
    e.light = Vec3::ONE;
    e.start_position = e.position;
}

/// ZzzEffect.cpp:11734.
fn water_wave_step(e: &mut Effect, ctx: &mut Ctx) {
    let luminosity = common::luminosity(e, ctx);
    e.scale += 0.02;
    e.position.z = e.start_position.z + e.gravity;
    e.velocity -= 10.0;
    e.gravity -= 20.0;
    if e.gravity < 60.0 {
        e.gravity = 60.0;
    }
    e.direction = Vec3::new(0.0, e.velocity, 0.0);
    let light = Vec3::new(luminosity * 0.1, luminosity * 0.3, luminosity * 0.6);
    for _ in 0..4 {
        ctx.create_particle(BITMAP_WATERFALL_5, e.position, e.angle, light, 1, 1.0, None);
    }
    ctx.add_terrain_light(e.position.x, e.position.y, light, 3.0);
    common::move_tail(e, ctx);
}
