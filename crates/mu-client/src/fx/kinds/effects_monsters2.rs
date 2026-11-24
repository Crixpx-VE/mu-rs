//! Monster / boss / siege effects (2): Shadow Rook and Ice Giant pieces
//! (`MODEL_SHADOW_ROOK_HELMET/KNEE_*/WRIST_*`, `MODEL_ICE_GIANT_PART5/6`), Sapitres
//! (`MODEL_EFFECT_SAPITRES_ATTACK/_1/_2`), broken ice and Karutan Condra stones
//! (`MODEL_EFFECT_BROKEN_ICE0..3`, `MODEL_CONDRA_STONE..5`, `MODEL_NARCONDRA_STONE..3`), Kundun's
//! skill (`MODEL_CUNDUN_DRAGON_HEAD/PHOENIX/GHOST/SKILL`), the Castle Siege catapult and its debris
//! (`MODEL_MONSTER01+77`, `MODEL_FLY_BIG_STONE1/2`, `MODEL_BIG_STONE_PART1/2`, `MODEL_WALL_PART1/2`,
//! `MODEL_GOLEM_STONE`, `MODEL_GATE_PART1..3`, `MODEL_TOWER_GATE_PLANE`), Napin thunder, Skura
//! (cherry blossom item), `MODEL_EFFECT_TRACE`, the ice / fire breaths, lava giant footprints,
//! `MODEL_EFFECT_FIRE_HIK3_MONO`, `MODEL_PROJECTILE`, the door / statue crush pieces, and Empire
//! Guardian (Gaion's swords and frame strike, blow of destruction, umbrella, guardian defender).
//! The Gaion swords live in `effects_monsters2_b.rs`.
//!
//! Translated from Main 5.2 `ZzzEffect.cpp` (`CreateEffect`, `MoveEffect` and its common tail
//! 18060-18124, `CheckTargetRange` 366, `RenderEffects`, `RenderEffectShadows`) and the type
//! branches of `Draw_RenderObject` (`ZzzObject.cpp`) that `RenderObject(o)` runs for them.
//!
//! Notes:
//! - `o->Visible` (set by `RenderEffects`' frustum test) is taken as true in `MoveEffect`.
//! - The `Scale` argument of `CreateEffect` is read from `e.scale`: the engine already turned a
//!   0 argument into 0.9 (the original uses the raw 0 where it reads `Scale` again).
//! - `rand()` inside `RenderEffects`/`RenderEffectShadows` cannot touch the logic generator from a
//!   draw function: [`draw_rand`] gives a per-frame value instead.

use bevy::math::Vec3;
use mu_data::bmd;

use super::common::{self, luminosity, move_tail, rf};
use crate::fx::core::*;

#[path = "effects_monsters2_b.rs"]
mod b;

/// `MODEL_MONSTER01+77`: the siege catapult (MapManager: `Monster78`).
const MODEL_CATAPULT: i32 = MODEL_MONSTER01 + 77;

pub fn register(r: &mut Registry) {
    let k = |create: Option<EffectCreate>, step: EffectStep, draw: EffectDraw| EffectKind { create, step: Some(step), draw: Some(draw) };

    // Shadow Rook / Ice Giant pieces (the Pawn / Knight / Rook ankle..chest and Ice Giant 1-4
    // share this code but belong to the other monsters batch).
    for ty in [
        MODEL_SHADOW_ROOK_HELMET,
        MODEL_SHADOW_ROOK_KNEE_LEFT,
        MODEL_SHADOW_ROOK_KNEE_RIGHT,
        MODEL_SHADOW_ROOK_WRIST_LEFT,
        MODEL_SHADOW_ROOK_WRIST_RIGHT,
    ] {
        r.effect(ty, k(Some(piece_create), piece_step, common::draw_object));
    }
    // ZzzEffect.cpp: no `case` in CreateEffect for PART5/6 (LifeTime is whatever the slot had).
    r.effect(MODEL_ICE_GIANT_PART5, k(None, piece_step, common::draw_object));
    r.effect(MODEL_ICE_GIANT_PART6, k(None, piece_step, common::draw_object));

    r.effect(MODEL_EFFECT_SAPITRES_ATTACK, k(Some(sapitres_create), sapitres_step, common::draw_default));
    r.effect(MODEL_EFFECT_SAPITRES_ATTACK_1, k(Some(sapitres1_create), sapitres1_step, common::draw_object));
    r.effect(MODEL_EFFECT_SAPITRES_ATTACK_2, k(Some(sapitres2_create), sapitres2_step, common::draw_object));

    for ty in MODEL_EFFECT_BROKEN_ICE0..=MODEL_EFFECT_BROKEN_ICE3 {
        r.effect(ty, k(Some(broken_ice_create), broken_ice_step, broken_ice_draw));
    }
    for ty in (MODEL_CONDRA_STONE..=MODEL_CONDRA_STONE5).chain(MODEL_NARCONDRA_STONE..=MODEL_NARCONDRA_STONE3) {
        r.effect(ty, k(Some(broken_ice_create), broken_ice_step, common::draw_default));
    }

    r.effect(MODEL_CUNDUN_DRAGON_HEAD, k(Some(dragon_head_create), dragon_head_step, common::draw_object));
    r.effect(MODEL_CUNDUN_PHOENIX, k(Some(phoenix_create), phoenix_step, common::draw_object));
    r.effect(MODEL_CUNDUN_GHOST, k(Some(ghost_create), ghost_step, common::draw_object));
    r.effect(MODEL_CUNDUN_SKILL, k(Some(cundun_skill_create), cundun_skill_step, common::draw_default));

    r.effect(MODEL_CATAPULT, k(Some(catapult_create), catapult_step, common::draw_nothing));
    r.effect(MODEL_FLY_BIG_STONE1, k(Some(fly_stone_create), fly_stone1_step, fly_stone1_draw));
    r.effect(MODEL_FLY_BIG_STONE2, k(Some(fly_stone_create), fly_stone2_step, fly_stone2_draw));
    r.effect(MODEL_BIG_STONE_PART1, k(Some(stone_part_create), stone_part_step, big_stone_part_draw));
    r.effect(MODEL_BIG_STONE_PART2, k(Some(stone_part_create), stone_part_step, big_stone_part_draw));
    r.effect(MODEL_WALL_PART1, k(Some(stone_part_create), stone_part_step, common::draw_default));
    r.effect(MODEL_WALL_PART2, k(Some(stone_part_create), stone_part_step, common::draw_default));
    r.effect(MODEL_GOLEM_STONE, k(Some(stone_part_create), stone_part_step, common::draw_default));
    for ty in MODEL_GATE_PART1..=MODEL_GATE_PART3 {
        r.effect(ty, k(Some(gate_part_create), gate_part_step, common::draw_default));
    }
    r.effect(MODEL_TOWER_GATE_PLANE, k(Some(tower_gate_create), tower_gate_step, tower_gate_draw));

    r.effect(MODEL_EFFECT_THUNDER_NAPIN_ATTACK_1, k(Some(napin_create), napin_step, common::draw_default));
    r.effect(MODEL_EFFECT_SKURA_ITEM, k(Some(skura_create), skura_step, skura_draw));
    r.effect(MODEL_EFFECT_TRACE, k(Some(trace_create), tail_only_step, common::draw_default));
    r.effect(MODEL_STREAMOFICEBREATH, k(Some(ice_breath_create), ice_breath_step, common::draw_default));
    r.effect(MODEL_1_STREAMBREATHFIRE, k(Some(fire_breath_create), fire_breath_step, common::draw_default));
    r.effect(MODEL_LAVAGIANT_FOOTPRINT_R, k(Some(footprint_create), footprint_step, footprint_draw));
    r.effect(MODEL_LAVAGIANT_FOOTPRINT_V, k(Some(footprint_create), footprint_step, footprint_draw));
    r.effect(MODEL_EFFECT_FIRE_HIK3_MONO, k(Some(fire_hik3_create), tail_only_step, common::draw_default));
    r.effect(MODEL_PROJECTILE, k(Some(projectile_create), projectile_step, common::draw_object));

    for ty in (MODEL_DOOR_CRUSH_EFFECT_PIECE01..=MODEL_DOOR_CRUSH_EFFECT_PIECE09)
        .chain(MODEL_DOOR_CRUSH_EFFECT_PIECE11..=MODEL_DOOR_CRUSH_EFFECT_PIECE13)
        .chain(MODEL_STATUE_CRUSH_EFFECT_PIECE01..=MODEL_STATUE_CRUSH_EFFECT_PIECE03)
    {
        r.effect(ty, k(Some(crush_piece_create), crush_piece_step, common::draw_object));
    }
    r.effect(MODEL_STATUE_CRUSH_EFFECT_PIECE04, k(Some(crush_ruin_create), crush_ruin_step, common::draw_object));
    r.effect(MODEL_DOOR_CRUSH_EFFECT_PIECE10, k(Some(crush_ruin_create), crush_ruin_step, common::draw_object));
    r.effect(MODEL_DOOR_CRUSH_EFFECT, k(Some(door_crush_create), tail_only_step, common::draw_default));
    r.effect(MODEL_STATUE_CRUSH_EFFECT, k(Some(statue_crush_create), tail_only_step, common::draw_default));

    r.effect(MODEL_EMPIREGUARDIAN_BLOW_OF_DESTRUCTION, k(Some(blow_create), blow_step, blow_draw));
    r.effect(MODEL_EFFECT_UMBRELLA_DIE, k(Some(umbrella_die_create), umbrella_die_step, common::draw_default));
    r.effect(MODEL_EFFECT_UMBRELLA_GOLD, k(Some(umbrella_gold_create), umbrella_gold_step, umbrella_gold_draw));
    r.effect(MODEL_EFFECT_EG_GUARDIANDEFENDER_ATTACK2, k(Some(defender_create), defender_step, common::draw_default));

    b::register(r);
}

// --- Helpers ----------------------------------------------------------------------------------------

/// `o` as the owner/target of what this effect creates.
fn me(e: &Effect) -> Option<Target> {
    Some(Target::Effect(e.id))
}

/// The original's integer `LifeTime`.
fn life(e: &Effect) -> i32 {
    e.life_time as i32
}

/// `Random`-free stand-in for `rand()` inside `RenderEffects`/`RenderEffectShadows`: fixed per
/// frame, effect and salt (it must not touch the logic `rand()`).
fn draw_rand(ctx: &DrawCtx, e: &Effect, salt: u32) -> i32 {
    let frame = (ctx.world_time / 40.0) as u64;
    let mut x = frame ^ ((salt as u64) << 32) ^ (e.id as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    (x & 0x7fff) as i32
}

/// `CreateSprite(...)` from a draw function (sprite subtype 0 → additive).
fn sprite_glow(out: &mut DrawList, ctx: &DrawCtx, texture: i32, position: Vec3, scale: f32, light: Vec3, rotation: f32) {
    common::sprite(out, ctx, texture, position, scale, light, rotation, Some(Blend::Glow));
}

/// `VectorNormalize`: leaves a zero vector as is.
fn normalize_mu(v: Vec3) -> Vec3 {
    let len = v.length();
    if len == 0.0 { v } else { v / len }
}

/// `Hero->Object.Position` (zero when there is no hero, e.g. in tests).
fn hero_position(ctx: &Ctx) -> Vec3 {
    ctx.hero_actor().map_or(Vec3::ZERO, |h| h.position)
}

/// `o->Owner == &Hero->Object`.
fn owner_is_hero(e: &Effect, ctx: &Ctx) -> bool {
    ctx.hero.is_some() && e.owner == ctx.hero.map(Target::Actor)
}

/// `CheckTargetRange(o)` (ZzzEffect.cpp:366) for the types of this file: within 100 units (2D) of
/// a live owner, `LifeTime = 1` (none of these types has extra work in its `switch`).
/// TODO(original): `to->Visible` (frustum) is taken as true.
fn check_target_range(e: &mut Effect, ctx: &Ctx) {
    let live = match e.owner {
        Some(Target::Actor(_)) => ctx.actor(e.owner).is_some_and(|a| a.live),
        Some(Target::Effect(id)) => ctx.effects.iter().any(|x| x.id == id),
        None => false,
    };
    if !live {
        return;
    }
    let Some(p) = ctx.target_position(e.owner) else { return };
    let (dx, dy) = (e.position.x - p.x, e.position.y - p.y);
    if (dx * dx + dy * dy).sqrt() <= 100.0 {
        e.life_time = 1.0;
    }
}

/// Only `Luminosity` + the common tail (types with no `case` in `MoveEffect`).
fn tail_only_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    move_tail(e);
}

// --- MODEL_SHADOW_ROOK_* / MODEL_ICE_GIANT_PART5/6: falling body pieces ----------------------------

/// ZzzEffect.cpp:2722-2746.
fn piece_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = (30 + ctx.rand() % 30) as f32;
    e.scale = 1.3;
    e.velocity = 0.0;
    e.pk_key = -1;
    e.gravity = 3.5;
    e.angle.z = rf(ctx, 360);
    let p1 = Vec3::new(0.0, (ctx.rand() % 64 + 48) as f32 * 0.1, 0.0);
    e.head_angle = rotate(e.angle, p1);
    e.head_angle.z = 15.0;
    e.sub_type = ctx.rand() % 2;
    e.light = Vec3::splat(0.5 + (ctx.rand() % 6) as f32 * 0.1);
    e.direction = Vec3::ZERO;
}

/// ZzzEffect.cpp:11677-11732.
fn piece_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    e.head_angle.z -= e.gravity;
    e.position += e.head_angle;
    let height = ctx.terrain_height(e.position.x, e.position.y) + 20.0;
    if e.position.z + e.direction.z <= height {
        e.position.z = height;
        e.head_angle.x *= 0.5;
        e.head_angle.y *= 0.5;
        e.head_angle.z += 0.6 * life(e) as f32;
        if e.head_angle.z < 5.0 {
            e.head_angle.z = 0.0;
        }
        e.alpha -= 0.05;
    } else if e.sub_type == 0 {
        e.angle.x += 0.15 * life(e) as f32;
        e.angle.y += 0.15 * life(e) as f32;
    } else {
        e.angle.x -= 0.15 * life(e) as f32;
        e.angle.y -= 0.15 * life(e) as f32;
    }
    move_tail(e);
}

// --- MODEL_EFFECT_SAPITRES_ATTACK / _1 / _2 ------------------------------------------------------------

/// ZzzEffect.cpp:4870.
fn sapitres_create(e: &mut Effect, ctx: &mut Ctx) {
    if e.sub_type == 0 {
        e.life_time = 20.0;
        for _ in 0..10 {
            ctx.create_effect(MODEL_EFFECT_SAPITRES_ATTACK_2, e.position, e.angle, e.light, 14, None, -1, 0, 0, 0.0);
        }
    }
}

/// ZzzEffect.cpp:14974.
fn sapitres_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    if life(e) % 6 == 0 {
        e.position.x += (ctx.rand() % 120 - 60) as f32;
        e.position.y += (ctx.rand() % 120 - 60) as f32;
        ctx.create_effect(MODEL_EFFECT_SAPITRES_ATTACK_1, e.position, e.angle, e.light, 0, e.owner, -1, 0, 0, 0.0);
    }
    move_tail(e);
}

/// ZzzEffect.cpp:4883.
fn sapitres1_create(e: &mut Effect, ctx: &mut Ctx) {
    if e.sub_type == 0 {
        e.blend_mesh = 0;
        e.blend_mesh_light = 1.0;
        e.position.z += 100.0;
        e.life_time = 17.0;
        e.scale = 1.1;
        // The original requires an owner here.
        if let Some(owner) = ctx.target_position(e.owner) {
            e.direction = normalize_mu(owner - e.position);
            e.angle.z = create_angle(e.position.x, e.position.y, owner.x, owner.y);
        }
    }
}

/// ZzzEffect.cpp:14985.
fn sapitres1_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    if e.sub_type == 0 {
        let speed = 40.0;
        e.position.x += e.direction.x * speed;
        e.position.y += e.direction.y * speed;
        let light = Vec3::new(0.1, 0.3, 1.0);
        ctx.create_sprite(BITMAP_LIGHT, e.position, 3.0, light, 0.0, 0);
        check_target_range(e, ctx);
        if life(e) == 1 {
            for _ in 0..3 {
                ctx.create_effect(MODEL_EFFECT_SAPITRES_ATTACK_2, e.position, e.angle, light, 0, None, -1, 0, 0, 0.0);
            }
        }
    }
    move_tail(e);
}

/// The `MODEL_EFFECT_SAPITRES_ATTACK_2` path of the `MODEL_ICE_SMALL` case (ZzzEffect.cpp:2939-3014).
fn sapitres2_create(e: &mut Effect, ctx: &mut Ctx) {
    e.blend_mesh = 0;
    e.blend_mesh_light = 1.0;
    let p1 = Vec3::new(0.0, (ctx.rand() % 256 + 64) as f32 * 0.1, 0.0);
    e.position.z += 50.0;
    if e.sub_type == 13 {
        e.life_time = (ctx.rand() % 16 + 100) as f32;
        e.scale = (ctx.rand() % 4 + 8) as f32 * 0.1;
        e.angle.z = rf(ctx, 360);
        e.direction = rotate(e.angle, p1);
        e.gravity = (ctx.rand() % 16 + 8) as f32;
    } else if e.sub_type == 14 {
        e.life_time = (ctx.rand() % 5 + 15) as f32;
        e.angle.x = (ctx.rand() % 60 - 30) as f32;
        e.angle.y = (ctx.rand() % 60 - 30) as f32;
        e.angle.z = rf(ctx, 360);
        e.direction = normalize_mu(rotate(e.angle, p1));
        e.gravity = 0.0;
    } else {
        e.life_time = (ctx.rand() % 16 + 32) as f32;
        e.scale = (ctx.rand() % 4 + 8) as f32 * 0.1;
        e.angle.z = rf(ctx, 360);
        e.direction = rotate(e.angle, p1);
        e.gravity = (ctx.rand() % 16 + 8) as f32;
    }
}

/// The `MODEL_EFFECT_SAPITRES_ATTACK_2` path of the `MODEL_ICE_SMALL` case (ZzzEffect.cpp:11113-11173).
fn sapitres2_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    let sub = e.sub_type;
    if sub == 0 || sub == 10 || sub == 12 {
        e.position += e.direction;
    } else if sub == 14 {
        let speed = (ctx.rand() % 5 + 25) as f32;
        e.position -= e.direction * speed;
        if life(e) <= 10 {
            e.blend_mesh_light -= 0.1;
        }
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
        // The rand() is drawn, but only the ice / meteor / stone types create something.
        if sub == 0 || sub == 12 || sub == 13 {
            let _ = ctx.rand() % 10;
        }
    } else if sub == 1 {
        e.gravity += 0.5;
    }
    move_tail(e);
}

// --- MODEL_EFFECT_BROKEN_ICE0..3 / MODEL_CONDRA_STONE* / MODEL_NARCONDRA_STONE* ---------------------

/// ZzzEffect.cpp:3016-3062.
fn broken_ice_create(e: &mut Effect, ctx: &mut Ctx) {
    match e.sub_type {
        0 => {
            e.direction = Vec3::ZERO;
            e.life_time = (ctx.rand() % 16 + 35) as f32;
            e.scale = (ctx.rand() % 13 + 3) as f32 * 0.2;
            e.gravity = (ctx.rand() % 3 + 3) as f32;
            e.angle.z = rf(ctx, 360);
            let p1 = Vec3::new(0.0, (ctx.rand() % 128 + 64) as f32 * 0.1, 0.0);
            e.head_angle = rotate(e.angle, p1);
            e.head_angle.z += 25.0;
        }
        1 => {
            e.life_time = 40.0;
            // `Scale + ...` (the argument).
            e.scale += (ctx.rand() % 8 + 15) as f32 * 0.1;
            e.direction = Vec3::new(0.0, 0.0, -60.0);
            e.head_angle = Vec3::new(0.0, 30.0, 0.0);
        }
        2 => {
            e.life_time = 100.0;
            e.gravity = 20.0 + (ctx.rand() % 20) as f32 * 0.5;
        }
        _ => {}
    }
}

/// ZzzEffect.cpp:11174-11276.
fn broken_ice_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    match e.sub_type {
        0 => {
            e.head_angle.z -= e.gravity;
            e.position += e.head_angle;
            let height = ctx.terrain_height(e.position.x, e.position.y);
            e.angle.x += 0.5 * life(e) as f32;
            e.angle.y += 0.5 * life(e) as f32;
            if e.position.z + e.direction.z <= height {
                e.position.z = height;
                e.head_angle.x *= 0.6;
                e.head_angle.y *= 0.6;
                e.head_angle.z += 1.0 * life(e) as f32;
                if e.head_angle.z < 0.5 {
                    e.head_angle.z = 0.0;
                }
                e.alpha -= 0.1;
            }
            if ctx.rand() % 30 == 0 {
                ctx.create_particle(BITMAP_SMOKE, e.position, e.angle, e.light, 0, 1.0, None);
            }
        }
        1 => {
            e.angle = e.head_angle;
            let height = ctx.terrain_height(e.position.x, e.position.y);
            if e.position.z < height {
                e.position.z = height;
                e.direction = Vec3::ZERO;
                let mut pos = Vec3::new(e.position.x, e.position.y, e.position.z + 80.0);
                ctx.create_effect(MODEL_SKILL_INFERNO, e.position, Vec3::ZERO, Vec3::new(0.0, 0.6, 1.0), 2, me(e), -1, 0, 0, 0.0);
                let light = Vec3::new(0.2, 0.4, 0.8);
                for _ in 0..8 {
                    pos.x = e.position.x + (ctx.rand() % 80 - 40) as f32;
                    pos.y = e.position.y + (ctx.rand() % 80 - 40) as f32;
                    pos.z = e.position.z + 50.0;
                    let scale = (ctx.rand() % 40 + 60) as f32 * 0.025;
                    ctx.create_particle(BITMAP_SMOKE, pos, e.angle, light, 11, scale, None);
                }
                for _ in 0..6 {
                    let ty = MODEL_EFFECT_BROKEN_ICE0 + ctx.rand() % 3;
                    ctx.create_effect(ty, pos, e.angle, light, 0, None, -1, 0, 0, 0.0);
                }
                let pos = Vec3::new(e.position.x, e.position.y, e.position.z + 100.0);
                ctx.create_particle(BITMAP_EXPLOTION_MONO, pos, e.angle, Vec3::new(0.6, 0.6, 1.0), 1, 1.5, None);
                e.live = false;
            }
        }
        2 => {
            e.position.z -= e.gravity;
            let height = ctx.terrain_height(e.position.x, e.position.y);
            if e.position.z < height {
                ctx.create_effect(e.ty, e.position, e.angle, e.light, 0, None, -1, 0, 0, 0.0);
                e.live = false;
            }
        }
        _ => {}
    }
    move_tail(e);
}

/// `RenderObject` (ZzzEffect.cpp:18496, ZzzObject.cpp:1796: `BodyLight = Light`,
/// `RENDER_TEXTURE|RENDER_BRIGHT`) and the ground light of subtype 1 (ZzzEffect.cpp:19466).
fn broken_ice_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    let mut d = ModelDraw::of_effect(e);
    d.bright = true;
    // TODO(original): the BlendMesh argument 0 makes mesh 0 use `BlendMeshLight` instead of `Alpha`.
    out.models.push(d);
    if e.sub_type == 1 {
        let lum = (draw_rand(ctx, e, 1) % 4 + 8) as f32 * 0.1;
        let light = Vec3::splat(lum * e.light.x);
        common::terrain_bitmap(out, BITMAP_LIGHT, e.position.x, e.position.y, 5.0, 5.0, light, -e.angle.z, 1.0, 5.0, Blend::Glow);
    }
}

// --- MODEL_CUNDUN_DRAGON_HEAD / PHOENIX / GHOST / SKILL ----------------------------------------------

/// ZzzEffect.cpp:3890.
fn dragon_head_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = 30.0;
    e.blend_mesh = -2;
    e.scale = 1.0;
    e.light = Vec3::splat(0.5);
    e.alpha = 0.0;
    e.position.z += 100.0;
    // sinf/cosf of a value in degrees used as radians, like the original.
    let f_angle = rf(ctx, 360);
    let f_distance = (ctx.rand() % 600 + 200) as f32;
    let pos = Vec3::new(e.position.x + f_angle.sin() * f_distance, e.position.y + f_angle.cos() * f_distance, 0.0);
    ctx.create_joint_ex(BITMAP_JOINT_SPIRIT2, pos, pos, e.angle, 18, me(e), 100.0, 0, 0);
    e.angle.x = -30.0;
    e.angle.y = 0.0;
    e.angle.z = rf(ctx, 360);
}

/// ZzzEffect.cpp:13636.
fn dragon_head_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    e.angle.z += (10 + ctx.rand() % 10) as f32;
    if e.position.z > 300.0 && e.position.z < 600.0 {
        let pos = Vec3::new(e.position.x, e.position.y, 350.0);
        if ctx.rand() % 2 == 0 {
            let angle = Vec3::new(0.0, 0.0, (ctx.rand() % 24 * 30) as f32);
            ctx.create_joint_ex(BITMAP_JOINT_SPIRIT2, pos, pos, angle, 14, None, 100.0, 0, 0);
        }
    }
    move_tail(e);
}

/// ZzzEffect.cpp:3912.
fn phoenix_create(e: &mut Effect, ctx: &mut Ctx) {
    e.velocity = 0.34;
    e.life_time = 20.0;
    e.blend_mesh = -2;
    e.scale = 0.7;
    e.light = Vec3::splat(0.5);
    e.alpha = 0.0;
    let angle = Vec3::new(0.0, 0.0, e.angle.z * std::f32::consts::PI / 180.0);
    ctx.create_joint_ex(BITMAP_JOINT_SPIRIT, e.position, e.position, angle, 15, me(e), 100.0, 0, 0);
    e.animation_frame = rf(ctx, 5);
}

/// ZzzEffect.cpp:13653.
fn phoenix_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    if e.life_time < 5.0 {
        e.alpha = life(e) as f32 * 0.2;
    }
    move_tail(e);
}

/// ZzzEffect.cpp:3929.
fn ghost_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.velocity = 0.08;
    e.life_time = 200.0;
    e.blend_mesh = -2;
    e.scale = 1.8;
    e.light = Vec3::splat(0.5);
}

/// `o->Owner->PKKey` of the ghost's owner, the `MODEL_CUNDUN_SKILL` (subtype 2) that created it.
/// TODO(original): the engine cannot read another effect's `PKKey`. That skill sets it from 0 to 1
/// in its move with `LifeTime` 9 (its 32nd tick); it moves before the ghost it created in the same
/// tick, so from the ghost's 32nd tick (its `LifeTime` ≤ 169) the owner's `PKKey` is no longer 0.
fn ghost_owner_pk_zero(e: &Effect) -> bool {
    e.life_time > 169.0
}

/// ZzzEffect.cpp:13659.
fn ghost_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    if e.owner.is_some() {
        if ghost_owner_pk_zero(e) {
            e.animation_frame = 0.0;
        } else {
            e.owner = None;
        }
    } else if e.animation_frame > 6.0 {
        e.pk_key += 1;
        e.position.z += e.pk_key as f32 * 0.8;
        let mut pos = e.position;
        pos.x += (ctx.rand() % 120 - 60) as f32;
        pos.y += (ctx.rand() % 120 - 60) as f32;
        pos.z += (ctx.rand() % 60) as f32;
        for _ in 0..3 {
            ctx.create_particle(BITMAP_SMOKE, pos, e.angle, Vec3::ONE, 20, 10.0, None);
        }
    } else {
        e.skill += 1;
        let s = e.skill as f32;
        if e.angle.z < 45.0 {
            e.angle.z += s * 0.3;
        }
        if e.angle.z > 45.0 {
            e.angle.z -= s * 0.3;
        }
        let hero = hero_position(ctx);
        if e.position.x < hero.x + 300.0 {
            e.position.x += s * 0.2;
        }
        if e.position.x > hero.x + 300.0 {
            e.position.x -= s * 0.2;
        }
        if e.position.y < hero.y - 300.0 {
            e.position.y += s * 0.2;
        }
        if e.position.y > hero.y - 300.0 {
            e.position.y -= s * 0.2;
        }
        let mut pos = e.position;
        pos.x += (ctx.rand() % 1200 - 600) as f32;
        pos.y += (ctx.rand() % 1200 - 600) as f32;
        let angle = Vec3::new(0.0, 0.0, (ctx.rand() % 10) as f32 * 20.0);
        ctx.create_effect(MODEL_FIRE, pos, angle, Vec3::ONE, 0, None, 0, 0, 0, 0.0);
        e.scale += 0.02;
        let quake = (ctx.rand() % 8 - 8) as f32 * 0.1;
        ctx.set_earthquake(quake);
    }
    move_tail(e);
}

/// ZzzEffect.cpp:3937.
fn cundun_skill_create(e: &mut Effect, ctx: &mut Ctx) {
    match e.sub_type {
        0 | 1 => e.life_time = 30.0,
        2 => {
            e.life_time = 40.0;
            e.pk_key = 0;
            ctx.create_effect(MODEL_CUNDUN_GHOST, e.position, e.angle, e.light, 0, me(e), -1, 0, 0, 0.0);
        }
        _ => {}
    }
}

/// ZzzEffect.cpp:13708.
fn cundun_skill_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    match e.sub_type {
        0 if life(e) == 30 => {
            for i in 0..10 {
                let angle = Vec3::new(0.0, 0.0, e.angle.z + 320.0 + (i * 8) as f32);
                ctx.create_effect(MODEL_CUNDUN_PHOENIX, e.position, angle, e.light, 0, None, -1, 0, 0, 0.0);
            }
        }
        1 => {
            if life(e) == 30 {
                for _ in 0..20 {
                    let angle = Vec3::new(0.0, 0.0, (e.pk_key * 30) as f32);
                    ctx.create_effect(MODEL_CUNDUN_DRAGON_HEAD, e.position, angle, e.light, 0, None, -1, 0, 0, 0.0);
                }
            }
            if e.pk_key > 5 && e.pk_key < 10 {
                let angle = Vec3::new(0.0, 0.0, (e.pk_key * 30) as f32);
                ctx.create_effect(MODEL_CUNDUN_DRAGON_HEAD, e.position, angle, e.light, 0, None, -1, 0, 0, 0.0);
            }
            e.pk_key += 1;
        }
        2 if e.pk_key == 0 && e.life_time < 10.0 => e.pk_key = 1,
        _ => {}
    }
    move_tail(e);
}

// --- MODEL_MONSTER01+77 (catapult) ------------------------------------------------------------------

/// ZzzEffect.cpp:4156.
fn catapult_create(e: &mut Effect, _ctx: &mut Ctx) {
    if e.sub_type == 0 {
        e.life_time = 20.0;
        e.hidden_mesh = 3;
        e.velocity = 0.33;
        // SetAction(o, 2).
        e.current_action = 2;
        e.animation_frame = 0.0;
        e.angle = Vec3::ZERO;
    }
}

/// ZzzEffect.cpp:13756. Outside `MODEL_BIRD01..MODEL_SKILL_END`: the tail only moves it.
fn catapult_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    if e.sub_type == 0 && life(e) % 5 == 0 {
        let mut pos = e.position;
        pos.x += (ctx.rand() % 100) as f32 - 50.0;
        pos.y -= (ctx.rand() % 30) as f32 + 15.0;
        ctx.create_effect(MODEL_FLY_BIG_STONE1, pos, e.angle, e.light, 2, None, -1, 0, 0, 0.0);
    }
    if e.life_time < 5.0 {
        e.alpha /= 1.3;
    }
    move_tail(e);
}

// --- MODEL_FLY_BIG_STONE1 / 2: catapult stones --------------------------------------------------------

/// `o->DamageTime` → `user[0]`.
const DAMAGE_TIME: usize = 0;

/// ZzzEffect.cpp:4194 / 4241.
fn fly_stone_create(e: &mut Effect, ctx: &mut Ctx) {
    if e.sub_type <= 1 {
        e.life_time = 300.0;
        e.velocity = (ctx.rand() % 200) as f32 / 100.0 + 28.0;
        e.gravity = 2.0;
        e.scale = 2.0;
        let dead = if e.pk_key == 1 {
            e.velocity = 29.0;
            // m_vDeadPosition = Angle (the target point travels in the Angle argument).
            e.angle
        } else {
            let hero = hero_position(ctx);
            let x = hero.x + ((ctx.rand() % 16 - 8) * 100) as f32;
            let y = if e.ty == MODEL_FLY_BIG_STONE1 {
                if ctx.rand() % 5 == 0 { 114.0 * 100.0 } else { ((ctx.rand() % 33 + 98) * 100) as f32 }
            } else {
                ((ctx.rand() % 30 + 80) * 100) as f32
            };
            Vec3::new(x, y, 100.0)
        };
        e.angle = Vec3::ZERO;
        e.light = Vec3::ONE;
        e.start_position = e.position;
        e.direction = (dead - e.start_position) * 0.001;
        e.direction.z = 5.0;
    } else if e.sub_type == 2 && e.ty == MODEL_FLY_BIG_STONE1 {
        e.life_time = 50.0;
        e.scale = (ctx.rand() % 8) as f32 / 20.0 + 0.7;
        e.gravity = 10.0;
        e.angle.x = rf(ctx, 360);
        e.direction = Vec3::ZERO;
        e.head_angle = Vec3::new(0.0, 0.0, rf(ctx, 360));
    }
}

/// The flight of subtypes 0/1, shared by both stones (ZzzEffect.cpp:13871-13911 / 14021-14065).
/// `fire` = `MODEL_FLY_BIG_STONE1` (fire trail) or `MODEL_FLY_BIG_STONE2` (smoke trail).
fn fly_stone_flight(e: &mut Effect, ctx: &mut Ctx, lum: f32, fire: bool) {
    e.position.x += e.direction.x * e.velocity;
    e.position.y += e.direction.y * e.velocity;
    e.start_position.z += e.direction.z;
    if e.position.z > 1000.0 {
        e.position.z = 1000.0;
        e.hidden_mesh = -2;
    } else {
        e.hidden_mesh = -1;
        if fire {
            let light = Vec3::new(lum, lum * 0.5, lum * 0.1);
            ctx.create_particle(BITMAP_FIRE, e.position, e.angle, light, 5, 2.0, None);
        } else if ctx.rand() % 2 == 0 {
            ctx.create_particle(BITMAP_ADV_SMOKE + 1, e.position, e.angle, Vec3::ONE, 1, 2.0, None);
        }
    }
    e.velocity -= 0.45;
    if e.velocity < 5.0 {
        e.velocity = 5.0;
    }
    e.direction.z += e.gravity;
    e.gravity -= 0.1;
    e.angle.x += 5.0;
    e.angle.y += 5.0;
    if e.sub_type == 0 {
        e.position.z = e.start_position.z;
    } else if owner_is_hero(e, ctx) {
        e.position.z = e.start_position.z;
        if e.position.z > 800.0 {
            e.position.z = 800.0;
        }
        // TODO(original): g_pCatapultWindow->SetCameraPos(Position) (catapult camera).
    }
}

/// The landing, shared by both stones. Returns the light of the final `AddTerrainLight` if the
/// shock wave changed it (MODEL_FLY_BIG_STONE2 reuses the variable).
fn fly_stone_landing(e: &mut Effect, ctx: &mut Ctx) -> Option<Vec3> {
    let mut light_out = None;
    let height = ctx.terrain_height(e.position.x, e.position.y);
    let early = e.ty == MODEL_FLY_BIG_STONE2 || life(e) < 250;
    if early && e.position.z < height + 300.0 && e.sub_type == 1 && e.kind != 0 && e.skill != 0 {
        // TODO(original): SendWeaponExplosion(Kind, Skill) (network).
        e.kind = 0;
        e.skill = 0;
    }
    if e.position.z < height {
        e.position.z = height - 5.0;
        if e.sub_type == 0 {
            e.life_time = 15.0;
            e.user[DAMAGE_TIME] = 0.0;
            e.sub_type = 88;
        } else if e.sub_type == 1 {
            e.life_time = 40.0;
            e.user[DAMAGE_TIME] = 25.0;
            e.sub_type = 99;
        }
        if e.hidden_mesh != 99 {
            ctx.create_effect(BITMAP_CRATER, e.position, e.angle, e.light, 0, None, -1, 0, 0, 0.0);
            ctx.play_sound("SOUND_BC_CATAPULT_HIT");
            // TODO(original): battleCastle::CollisionEffectToObject(o, 350, 250, true) (stone 1 only).
            if e.sub_type == 88 && e.owner.is_none() {
                // TODO(original): battleCastle::CollisionHeroCharacter(Position, 350, PLAYER_HIGH_SHOCK).
            } else if e.sub_type == 99 || (e.sub_type == 88 && e.owner.is_some()) {
                let light = Vec3::new(1.0, 0.3, 0.1);
                ctx.create_effect(BITMAP_SHOCK_WAVE, e.position, e.angle, light, 7, None, -1, 0, 0, 0.0);
                light_out = Some(light);
            }
        }
        e.hidden_mesh = 99;
        if height > 150.0 {
            let collision_y = (e.position.y / 100.0) as i32;
            let mut quake = (ctx.rand() % 8 - 8) as f32 * 0.1;
            if collision_y < 104 {
                // TODO(original): AddTerrainHeight(x, y, -30, 2, BackTerrainHeight) +
                // CreateTerrainNormal_Part / CreateTerrainLight_Part (crater in the terrain).
                quake /= 2.0;
            }
            ctx.set_earthquake(quake);
        }
    } else if e.ty == MODEL_FLY_BIG_STONE1 {
        // TODO(original): battleCastle::CollisionEffectToObject(o, 200, 250, false[, true]) and
        // PlayBuffer(SOUND_BC_CATAPULT_HIT) when it hits a castle object.
    }
    light_out
}

/// ZzzEffect.cpp:13831.
fn fly_stone1_step(e: &mut Effect, ctx: &mut Ctx) {
    let lum = luminosity(e, ctx);
    fly_stone_render_mutation(e);
    if e.sub_type == 2 {
        e.position.z -= e.gravity;
        e.gravity += 1.9;
        e.angle.x += (ctx.rand() % 20) as f32 + 20.0;
        e.angle.y += (ctx.rand() % 5) as f32;
        let height = ctx.terrain_height(e.position.x, e.position.y);
        if e.position.z < height {
            e.position.z = height + 50.0;
            e.gravity = -e.gravity * 0.2;
            e.life_time -= 5.0;
            e.angle.x -= e.scale * 128.0;
            e.direction = Vec3::new(0.0, 5.0, 0.0);
        } else {
            e.angle.x -= e.scale * 16.0;
        }
        e.position += rotate(e.head_angle, e.direction);
        if e.life_time < 10.0 {
            e.alpha /= 1.5;
        }
        if life(e) % 3 == 0 {
            ctx.create_particle(BITMAP_ADV_SMOKE + 1, e.position, e.angle, Vec3::new(1.0, 0.6, 0.2), 1, 0.2, None);
        }
    } else {
        if e.sub_type <= 1 {
            fly_stone_flight(e, ctx, lum, true);
        }
        if e.sub_type == 88 || e.sub_type == 99 {
            if e.sub_type == 99 && owner_is_hero(e, ctx) {
                // TODO(original): g_pCatapultWindow->SetCameraPos(Position).
            }
            if e.life_time > e.user[DAMAGE_TIME] {
                let light = Vec3::new(1.0, 0.6, 0.2);
                ctx.create_particle(BITMAP_ADV_SMOKE + 1, e.position, e.angle, light, 0, 1.0, None);
                ctx.create_particle(BITMAP_FIRE, e.position, e.angle, light, 2, 2.0, None);
                // TODO(original): battleCastle::CollisionHeroCharacter((15-LifeTime)*20+350) for
                // 88 with an owner, CollisionTempCharacter((40-LifeTime)*7+350) for 99.
            }
        } else {
            fly_stone_landing(e, ctx);
        }
        ctx.add_terrain_light(e.position.x, e.position.y, Vec3::splat(-0.5), 4.0);
    }
    // Tail: PlayAnimation only (no MoveParticle for the flying stones, ZzzEffect.cpp:18105).
    common::play_animation(e);
}

/// ZzzEffect.cpp:14019.
fn fly_stone2_step(e: &mut Effect, ctx: &mut Ctx) {
    let lum = luminosity(e, ctx);
    fly_stone_render_mutation(e);
    if e.sub_type <= 1 {
        fly_stone_flight(e, ctx, lum, false);
    }
    let mut light = Vec3::splat(-0.5);
    if e.sub_type == 88 || e.sub_type == 99 {
        if e.sub_type == 99 && owner_is_hero(e, ctx) {
            // TODO(original): g_pCatapultWindow->SetCameraPos(Position).
        }
        if e.life_time > e.user[DAMAGE_TIME] {
            ctx.create_particle(BITMAP_ADV_SMOKE + 1, e.position, e.angle, e.light, 0, 1.0, None);
            // TODO(original): battleCastle::CollisionHeroCharacter / CollisionTempCharacter.
        }
    } else if let Some(l) = fly_stone_landing(e, ctx) {
        light = l;
    }
    ctx.add_terrain_light(e.position.x, e.position.y, light, 4.0);
    common::play_animation(e);
}

/// What `Draw_RenderObject` does to a landed stone (ZzzObject.cpp:1453/1471), moved here from the
/// draw: the frame after `HiddenMesh = 99` it bursts and hides.
fn fly_stone_render_mutation(e: &mut Effect) {
    if e.hidden_mesh == 99 {
        e.hidden_mesh = -2;
        // TODO(original): RenderMeshEffect(0, MODEL_BIG_STONE_PART1/2): for every corner of every
        // triangle of mesh 0 (world space), `rand()%3==0` → CreateEffect(MODEL_BIG_STONE_PART1 +
        // rand()%2, vertex, (0,0,0), (1,1,1), 1 for stone 1 / 0 for stone 2). Needs the model's
        // mesh in the step.
    }
}

/// ZzzObject.cpp:1451.
fn fly_stone1_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    if e.hidden_mesh == 99 || e.hidden_mesh == -2 {
        return;
    }
    let mut d = ModelDraw::of_effect(e);
    d.light = Vec3::ONE;
    out.models.push(d);
    // TODO(original): subtypes 0/1 add a RENDER_CHROME|RENDER_BRIGHT pass with BodyLight (1, 0.2, 0.1).
}

/// ZzzObject.cpp:1469.
fn fly_stone2_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    if e.hidden_mesh == 99 || e.hidden_mesh == -2 {
        return;
    }
    out.models.push(ModelDraw::of_effect(e));
}

// --- MODEL_BIG_STONE_PART1/2, MODEL_WALL_PART1/2, MODEL_GOLEM_STONE: debris --------------------------

/// ZzzEffect.cpp:4275-4322.
fn stone_part_create(e: &mut Effect, ctx: &mut Ctx) {
    let p1;
    if e.ty == MODEL_WALL_PART1 || e.ty == MODEL_WALL_PART2 {
        p1 = Vec3::new(0.0, (ctx.rand() % 256 + 128) as f32 * 0.1, 0.0);
        e.scale = (ctx.rand() % 2 + 7) as f32 * 0.1;
        e.gravity = (ctx.rand() % 5 + 6) as f32;
    } else if e.ty == MODEL_GOLEM_STONE {
        p1 = Vec3::new(0.0, (ctx.rand() % 128 + 32) as f32 * 0.1, 0.0);
        e.scale = (ctx.rand() % 2 + 10) as f32 * 0.25;
        e.gravity = (ctx.rand() % 5 + 2) as f32;
    } else if e.sub_type == 2 {
        p1 = Vec3::new(0.0, (ctx.rand() % 128 + 32) as f32 * 0.1, 0.0);
        e.scale = 0.6 + (ctx.rand() % 2 + 4) as f32 * 0.12;
        e.gravity = (ctx.rand() % 5 + 2) as f32;
    } else if e.ty == MODEL_BIG_STONE_PART2 && e.sub_type == 3 {
        e.life_time = (ctx.rand() % 16 + 32) as f32;
        e.scale = 1.2 + (ctx.rand() % 3 + 2) as f32 * 0.12;
        e.direction.z = -(ctx.rand() % 5 + 20) as f32;
        e.velocity = 1.8;
        return;
    } else {
        p1 = Vec3::new(0.0, (ctx.rand() % 128 + 128) as f32 * 0.1, 0.0);
        e.scale = (ctx.rand() % 7 + 10) as f32 * 0.1;
        e.gravity = (ctx.rand() % 5 + 2) as f32;
    }
    e.life_time = (ctx.rand() % 16 + 32) as f32;
    e.angle.z = rf(ctx, 360);
    e.direction = rotate(e.angle, p1);
    e.angle.x = rf(ctx, 360);
    e.angle.y = rf(ctx, 360);
    e.angle.z = rf(ctx, 360);
    e.position.z += 50.0;
}

/// ZzzEffect.cpp:14152-14225.
fn stone_part_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    if e.ty == MODEL_BIG_STONE_PART2 && e.sub_type == 3 {
        e.direction.z -= e.velocity;
        e.velocity += 0.3;
        let height = ctx.terrain_height(e.position.x, e.position.y);
        if e.position.z < height {
            e.position.z = height;
            e.direction = Vec3::ZERO;
            e.angle = Vec3::ZERO;
            e.hidden_mesh = 0;
            if ctx.rand() % 10 == 0 {
                ctx.create_particle(BITMAP_SMOKE, e.position, e.angle, e.light, 24, 1.25 * e.scale, None);
            }
        } else if ctx.rand() % 2 == 0 {
            ctx.create_particle(BITMAP_SMOKE, e.position, e.angle, e.light, 24, 0.2, None);
        }
        move_tail(e);
        return;
    }
    e.position += e.direction;
    e.direction *= 0.9;
    if e.ty == MODEL_WALL_PART1 || e.ty == MODEL_WALL_PART2 {
        e.gravity -= 6.0;
    } else {
        e.gravity -= 3.0;
    }
    e.position.z += e.gravity;
    let height = ctx.terrain_height(e.position.x, e.position.y);
    if e.position.z < height {
        e.position.z = height;
        e.gravity = -e.gravity * 0.2;
        e.life_time -= 5.0;
        e.angle.x -= e.scale * 128.0;
    } else {
        e.angle.x -= e.scale * 16.0;
    }
    e.alpha = life(e) as f32 / 10.0;
    // `Light` is MoveEffect's local, still (1, 1, 1) here.
    let light = Vec3::ONE;
    if e.ty == MODEL_GOLEM_STONE {
        if ctx.rand() % 4 == 0 {
            ctx.create_particle(BITMAP_TRUE_FIRE, e.position, e.angle, light, 5, 2.8, None);
            ctx.create_particle(BITMAP_SMOKE, e.position, e.angle, light, 21, 1.8, None);
        }
        if ctx.rand() % 10 == 0 {
            ctx.create_particle(BITMAP_SMOKE + 1, e.position, e.angle, light, 0, 1.0, None);
        }
    } else if e.ty == MODEL_BIG_STONE_PART1 && e.sub_type == 2 && ctx.rand() % 10 == 0 {
        e.position.z = height;
        ctx.create_particle(BITMAP_SMOKE, e.position, e.angle, Vec3::new(0.2, 0.5, 0.35), 11, 2.0, None);
    }
    move_tail(e);
}

/// ZzzObject.cpp:1481.
fn big_stone_part_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    let mut d = ModelDraw::of_effect(e);
    match e.sub_type {
        1 => {
            d.light = Vec3::ONE;
            // TODO(original): plus a RENDER_CHROME|RENDER_BRIGHT pass with BodyLight (1, 0.2, 0.1).
        }
        2 | 3 => d.light = Vec3::new(0.5, 1.0, 0.3),
        _ => {}
    }
    out.models.push(d);
}

// --- MODEL_GATE_PART1..3 ---------------------------------------------------------------------------

/// ZzzEffect.cpp:4324.
fn gate_part_create(e: &mut Effect, ctx: &mut Ctx) {
    let p1 = Vec3::new(0.0, (ctx.rand() % 128 + 64) as f32 * 0.1, 0.0);
    e.position.z += 50.0;
    e.life_time = (ctx.rand() % 16 + 32) as f32;
    e.scale = (ctx.rand() % 2 + 7) as f32 * 0.1;
    e.angle.z = rf(ctx, 360);
    e.direction = rotate(e.angle, p1);
    e.gravity = (ctx.rand() % 5 + 2) as f32;
    if e.ty == MODEL_GATE_PART1 && e.sub_type == 0 {
        e.sub_type = 1;
        e.gravity += (ctx.rand() % 5) as f32;
    } else if e.sub_type == 2 {
        e.scale *= 0.6;
    }
    e.angle.x = rf(ctx, 360);
    e.angle.y = rf(ctx, 360);
    e.angle.z = rf(ctx, 360);
}

/// ZzzEffect.cpp:14227.
fn gate_part_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
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
        // The original passes MoveEffect's uninitialized local `Position`; the stone's position
        // is the closest meaningful point.
        ctx.create_particle(BITMAP_SMOKE + 1, e.position, e.angle, Vec3::ONE, 0, 1.0, None);
    }
    move_tail(e);
}

// --- MODEL_TOWER_GATE_PLANE --------------------------------------------------------------------------

/// ZzzEffect.cpp:4357.
fn tower_gate_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 100.0;
    e.blend_mesh_light = 0.3;
    e.start_position = e.position;
}

/// ZzzEffect.cpp:14507.
fn tower_gate_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    e.life_time = 100.0;
    if e.owner.is_some() && e.live {
        e.position.z = e.start_position.z + (ctx.world_time() * 0.001).sin() * 200.0 + 200.0;
    }
    move_tail(e);
}

/// ZzzEffect.cpp:18585: drawn at its position and mirrored around `StartPosition.z + 200`.
/// The original leaves the mirrored height in `Position` after drawing (both heights get drawn
/// every frame either way), so nothing is mutated here.
fn tower_gate_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    out.models.push(ModelDraw::of_effect(e));
    let mut d = ModelDraw::of_effect(e);
    d.position.z = e.start_position.z + 400.0 - (e.position.z - e.start_position.z);
    out.models.push(d);
}

// --- MODEL_EFFECT_THUNDER_NAPIN_ATTACK_1 ---------------------------------------------------------------

/// ZzzEffect.cpp:4898.
fn napin_create(e: &mut Effect, _ctx: &mut Ctx) {
    if e.sub_type == 0 || e.sub_type == 1 {
        e.life_time = 30.0;
    }
}

/// ZzzEffect.cpp:15009.
fn napin_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    let light = Vec3::new(0.4, 0.7, 1.0);
    let flare = Vec3::new(0.1, 0.2, 0.8);
    let jitter = |ctx: &mut Ctx| 3.0 * ((ctx.rand() % 40) as f32 - 20.0);
    if e.sub_type == 0 {
        let mut pos = e.position;
        if life(e) >= 4 {
            e.scale = 0.8 + (ctx.rand() % 10) as f32 * 0.1;
            pos.x += jitter(ctx);
            pos.y += jitter(ctx);
            pos.z += jitter(ctx);
            let ty = BITMAP_LIGHTNING_MEGA1 + ctx.rand() % 3;
            ctx.create_particle(ty, pos, e.angle, light, 0, e.scale, None);
        }
        ctx.create_sprite(BITMAP_LIGHT, e.position, 8.0, flare, 0.0, 0);
        ctx.create_sprite(BITMAP_LIGHT, e.position, 8.0, flare, 0.0, 0);
    } else if e.sub_type == 1 {
        let offset = |ctx: &mut Ctx, p: Vec3| {
            let x = p.x + ((ctx.rand() % 200) as f32 - 100.0);
            let y = p.y + ((ctx.rand() % 200) as f32 - 100.0);
            let z = p.z + ((ctx.rand() % 100) as f32 - 50.0);
            Vec3::new(x, y, z)
        };
        let mut pos = offset(ctx, e.position);
        if life(e) >= 4 {
            e.scale = 0.8 + (ctx.rand() % 10) as f32 * 0.1;
            pos.x += jitter(ctx);
            pos.y += jitter(ctx);
            pos.z += jitter(ctx);
            let ty = BITMAP_LIGHTNING_MEGA1 + ctx.rand() % 3;
            ctx.create_particle(ty, pos, e.angle, light, 0, e.scale, None);
        }
        let pos = offset(ctx, e.position);
        ctx.create_sprite(BITMAP_LIGHT, pos, 8.0, flare, 0.0, 0);
        ctx.create_sprite(BITMAP_LIGHT, pos, 8.0, flare, 0.0, 0);
    }
    move_tail(e);
}

// --- MODEL_EFFECT_SKURA_ITEM (cherry blossom item) -----------------------------------------------------

/// ZzzEffect.cpp:4911.
fn skura_create(e: &mut Effect, _ctx: &mut Ctx) {
    if e.sub_type == 0 || e.sub_type == 1 {
        e.life_time = 52.0;
    }
}

/// ZzzEffect.cpp:15059.
fn skura_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    if (e.sub_type == 0 || e.sub_type == 1) && e.live {
        // The original requires a character owner.
        if let Some(owner) = ctx.actor(e.owner).filter(|a| a.live).cloned() {
            e.position = owner.position;
            e.light = Vec3::new(1.0, 0.6, 0.8);
            let light1 = Vec3::splat(0.3);
            let light2 = Vec3::new(1.0, 0.6, 0.8);
            let rot = ctx.world_time() * 0.08;
            // TODO(original): bones 1 and 2 of the item's own model (posed with its AnimationFrame);
            // the engine has no effect-model bones in the step, so its origin is used.
            for (bone_pos, rotation) in [(e.position, rot), (e.position, -rot)] {
                ctx.create_sprite(BITMAP_SHINY + 1, bone_pos, 1.8, e.light, rotation, 0);
                for _ in 0..7 {
                    ctx.create_particle(BITMAP_SHINY + 1, bone_pos, e.angle, e.light, 5, 0.8, me(e));
                    let l = if ctx.rand() % 7 == 3 { light2 } else { light1 };
                    ctx.create_particle(BITMAP_CHERRYBLOSSOM_EVENT_PETAL, bone_pos, e.angle, l, 0, 0.5, None);
                }
            }
            let lt = life(e);
            if lt == 30 || lt == 15 || lt == 4 {
                // Owner bone 20, posed with BodyOrigin = Position (= the owner's position).
                let mut pos = owner.bone_world(20, Vec3::ZERO);
                if ctx.rand() % 2 == 0 {
                    pos.x += (ctx.rand() % 40 + 10) as f32;
                } else {
                    pos.x -= (ctx.rand() % 30 + 10) as f32;
                }
                pos.z += (ctx.rand() % 110 + 50) as f32;
                let light1 = Vec3::new(0.7, 0.71, 1.0);
                let light2 = Vec3::new(0.8, 0.85, 1.0);
                for _ in 0..70 {
                    if ctx.rand() % 3 == 0 {
                        let l = if ctx.rand() % 7 == 3 { light2 } else { light1 };
                        ctx.create_particle(BITMAP_CHERRYBLOSSOM_EVENT_PETAL, pos, e.angle, l, 0, 0.4, None);
                    }
                    let l = if ctx.rand() % 4 == 3 { light2 } else { light1 };
                    ctx.create_particle(BITMAP_CHERRYBLOSSOM_EVENT_FLOWER, pos, e.angle, l, 0, 0.4, None);
                }
                ctx.create_particle(BITMAP_SHOCK_WAVE, pos, Vec3::new(0.0, 1.0, 0.0), Vec3::new(1.0, 0.6, 0.8), 4, 0.005, None);
                let mut pos = owner.bone_world(20, Vec3::ZERO);
                pos.z += 70.0;
                ctx.create_sprite(BITMAP_SHINY + 1, pos, 1.8, e.light, -ctx.world_time() * 0.08, 0);
                // TODO(original): StopBuffer(SOUND_CHERRYBLOSSOM_EFFECT1, true) before playing it again.
                ctx.play_sound("SOUND_CHERRYBLOSSOM_EFFECT1");
            }
            // b->PlayAnimation(..., Velocity / 3).
            e.animation_frame += e.velocity / 3.0;
        }
    }
    move_tail(e);
}

/// `RenderObject` → ZzzObject.cpp:715: `RenderBody(RENDER_COLOR, ...)`.
fn skura_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    // TODO(original): RENDER_COLOR draws the meshes untextured in the body light color; drawn
    // textured here.
    out.models.push(ModelDraw::of_effect(e));
}

// --- MODEL_EFFECT_TRACE ------------------------------------------------------------------------------

/// ZzzEffect.cpp:4920 (no `case` in MoveEffect / RenderEffects).
fn trace_create(e: &mut Effect, ctx: &mut Ctx) {
    if e.sub_type == 0 {
        e.life_time = 50.0;
        // `o->EyeLeft = Position` → user[0..3].
        // TODO(original): the trail reads `Target->EyeLeft`; the engine only exposes the position.
        e.user[0] = e.position.x;
        e.user[1] = e.position.y;
        e.user[2] = e.position.z;
        // CreateJoint(BITMAP_JOINT_ENERGY, ..., 17, o, Scale, -1, 0, 0, -1, o->Light): the joint
        // starts with this effect's light.
        let mut j = Joint::new(BITMAP_JOINT_ENERGY, e.position, e.position, e.angle, 17, me(e), e.scale);
        j.pk_key = -1;
        j.light = e.light;
        ctx.spawns.push(Spawn::Joint(j));
    }
}

// --- MODEL_STREAMOFICEBREATH / MODEL_1_STREAMBREATHFIRE ----------------------------------------------

/// ZzzEffect.cpp:5107.
fn ice_breath_create(e: &mut Effect, ctx: &mut Ctx) {
    const LENSCALAR: f32 = 50.0;
    const RANDOMOFFSET_ANGLE: f32 = 5.0;
    const RANDOMOFFSET_SCALE: f32 = 0.01;
    const RANDOMOFFSET_POSITION: f32 = 20.0;
    let randnum = (ctx.rand() % 2000 - 1000) as f32 * 0.001;
    e.life_time = 17.0;
    e.light = Vec3::splat(0.6);
    let len = rotate(e.angle, Vec3::new(0.0, -1.0, 0.0));
    let basis = e.position + len * LENSCALAR;
    e.start_position = basis;
    for _ in 0..4 {
        let off_scale = RANDOMOFFSET_SCALE * randnum;
        let off_pos = RANDOMOFFSET_POSITION * randnum;
        let off_angle = RANDOMOFFSET_ANGLE * randnum;
        let pos = basis + Vec3::splat(off_pos);
        let angle = e.angle + Vec3::splat(off_angle);
        ctx.create_particle(BITMAP_RAKLION_CLOUDS, pos, angle, e.light, 0, e.scale + off_scale, None);
    }
}

/// ZzzEffect.cpp:15643.
fn ice_breath_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    if e.sub_type == 0 && life(e) > 1 {
        for _ in 0..4 {
            const RANDOMOFFSET_ANGLE: f32 = 15.0;
            const RANDOMOFFSET_SCALE: f32 = 0.01;
            const RANDOMOFFSET_POSITION: f32 = 20.0;
            let randnum = (ctx.rand() % 2000 - 1000) as f32 * 0.001;
            let pos = e.start_position + Vec3::splat(RANDOMOFFSET_POSITION * randnum);
            let angle = e.angle + Vec3::splat(RANDOMOFFSET_ANGLE * randnum);
            let scale = e.scale + RANDOMOFFSET_SCALE * randnum;
            ctx.create_particle(BITMAP_RAKLION_CLOUDS, pos, angle, Vec3::new(0.5, 0.6, 0.94), 1, scale, None);
        }
    }
    move_tail(e);
}

/// ZzzEffect.cpp:5158.
fn fire_breath_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 30.0;
}

/// ZzzEffect.cpp:15682.
fn fire_breath_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    let pos = e.position;
    ctx.create_particle(BITMAP_WATERFALL_3, pos, e.angle, e.light, 11, 0.6, None);
    ctx.create_particle(BITMAP_SMOKE, pos, e.angle, e.light, 52, 0.6, None);
    if life(e) == 15 {
        let dir = rotate(e.angle, Vec3::new(0.0, -20.0, 0.0));
        // The original passes the direction as the Angle argument.
        ctx.create_effect(MODEL_MOONHARVEST_MOON, e.position, dir, e.light, 1, None, -1, 0, 0, 0.3);
    }
    move_tail(e);
}

// --- MODEL_LAVAGIANT_FOOTPRINT_R / V ---------------------------------------------------------------

/// ZzzEffect.cpp:5163.
fn footprint_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 200.0;
    e.alpha = 1.0;
    e.position.z = 0.0;
    // o->m_dwTime = timeGetTime() is never read for these types.
}

/// ZzzEffect.cpp:15780.
fn footprint_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    e.angle.x += 0.1;
    e.alpha -= 0.01;
    if e.alpha <= 0.0 {
        e.live = false;
    }
    e.light *= 0.93;
    move_tail(e);
}

/// ZzzEffect.cpp:18727: `EnableAlphaBlend` + `RenderTerrainAlphaBitmap` (default rotation, alpha
/// and height).
fn footprint_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    let tex = if e.ty == MODEL_LAVAGIANT_FOOTPRINT_R { BITMAP_LAVAGIANT_FOOTPRINT_R } else { BITMAP_LAVAGIANT_FOOTPRINT_V };
    common::terrain_bitmap(out, tex, e.position.x, e.position.y, e.scale, e.scale, e.light, 0.0, 1.0, 5.0, Blend::Glow);
}

// --- MODEL_EFFECT_FIRE_HIK3_MONO ------------------------------------------------------------------------

/// ZzzEffect.cpp:5175 (LifeTime is not set: whatever the slot had).
fn fire_hik3_create(e: &mut Effect, ctx: &mut Ctx) {
    if ctx.rand() % 100 > 20 {
        ctx.create_particle(BITMAP_FIRE_HIK3_MONO, e.position, e.angle, e.light, 1, e.scale, None);
    }
}

// --- MODEL_PROJECTILE ------------------------------------------------------------------------------

/// ZzzEffect.cpp:5185.
fn projectile_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 50.0;
    e.gravity = 0.0;
    e.velocity = 70.0;
    e.start_position = e.position;
}

/// ZzzEffect.cpp:15794.
fn projectile_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    if life(e) > 38 {
        // Owner bone 9 posed with the owner's angle, without its scale or origin
        // (`Animation(..., false, false)`), plus the owner's position.
        if let Some(owner) = ctx.actor(e.owner) {
            let local = owner.bones.get(9).map_or(Vec3::ZERO, |m| Vec3::from_array(bmd::transform_point(m, [0.0; 3])));
            e.position = owner.position + rotate(owner.angle, local);
        }
    } else {
        if e.gravity == 0.0 {
            e.gravity = 0.1;
            e.direction = normalize_mu(e.position - e.start_position);
        }
        e.angle.x += 0.5;
        e.velocity += 0.2;
        e.position.x += e.direction.x * e.velocity;
        e.position.y += e.direction.y * e.velocity;
        e.position.z += e.direction.z;
    }
    move_tail(e);
}

// --- Door / statue crush pieces ----------------------------------------------------------------------

/// ZzzEffect.cpp:5193-5228.
fn crush_piece_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = (30 + ctx.rand() % 30) as f32;
    e.velocity = 0.0;
    e.pk_key = -1;
    e.gravity = 2.3;
    e.angle.z = rf(ctx, 360);
    let p1 = Vec3::new(0.0, (ctx.rand() % 64 + 48) as f32 * 0.1, 0.0);
    e.head_angle = rotate(e.angle, p1);
    e.head_angle.z = 15.0;
    e.sub_type = ctx.rand() % 2;
    e.light = Vec3::splat(0.5 + (ctx.rand() % 6) as f32 * 0.1);
    e.direction = Vec3::ZERO;
}

/// ZzzEffect.cpp:15828-15894.
fn crush_piece_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    e.head_angle.z -= e.gravity;
    e.position += e.head_angle;
    let height = ctx.terrain_height(e.position.x, e.position.y) + 20.0;
    if e.position.z + e.direction.z <= height {
        e.position.z = height;
        e.head_angle.x *= 0.8;
        e.head_angle.y *= 0.8;
        e.head_angle.z += 0.6 * life(e) as f32;
        if e.head_angle.z < 5.0 {
            e.head_angle.z = 0.0;
        }
        e.alpha -= 0.05;
        if ctx.rand() % 4 == 0 {
            let p = Vec3::new(0.0, (ctx.rand() % 80) as f32 + 50.0, 0.0);
            let angle = Vec3::new(0.0, 0.0, rf(ctx, 360));
            let pos = rotate(angle, p) + e.position;
            let ty = MODEL_STONE1 + ctx.rand() % 2;
            ctx.create_effect(ty, pos, e.angle, e.light, 0, None, -1, 0, 0, 0.0);
        }
    } else if e.sub_type == 0 {
        e.angle.x += 0.15 * life(e) as f32;
        e.angle.y += 0.15 * life(e) as f32;
    } else {
        e.angle.x -= 0.15 * life(e) as f32;
        e.angle.y -= 0.15 * life(e) as f32;
    }
    if ctx.rand() % 3 == 0 {
        ctx.create_particle(BITMAP_SMOKE + 1, e.position, e.angle, e.light, 6, 1.0, None);
    }
    move_tail(e);
}

/// ZzzEffect.cpp:5229 (`Scale` is the argument).
fn crush_ruin_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 100.0;
    e.pk_key = -1;
}

/// ZzzEffect.cpp:15895.
fn crush_ruin_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    e.life_time -= 1.0;
    if e.life_time < 40.0 {
        e.alpha -= 0.1;
    }
    move_tail(e);
}

/// ZzzEffect.cpp:5239 (LifeTime is not set: whatever the slot had).
fn door_crush_create(e: &mut Effect, ctx: &mut Ctx) {
    let first = match e.sub_type {
        0 => MODEL_DOOR_CRUSH_EFFECT_PIECE01,
        1 => MODEL_DOOR_CRUSH_EFFECT_PIECE11,
        _ => return,
    };
    for i in 0..9 {
        let x = e.position.x + (ctx.rand() % 200 - 100) as f32;
        let y = e.position.y + (ctx.rand() % 200 - 100) as f32;
        let z = e.position.z + (ctx.rand() % 200 - 100) as f32;
        let ty = if e.sub_type == 0 { first + i } else { first + i % 3 };
        ctx.create_effect(ty, Vec3::new(x, y, z), e.angle, e.light, 0, e.owner, 0, 0, 0, 0.0);
    }
}

/// ZzzEffect.cpp:5271 (LifeTime is not set: whatever the slot had).
fn statue_crush_create(e: &mut Effect, ctx: &mut Ctx) {
    for i in 0..6 {
        ctx.create_effect(MODEL_STATUE_CRUSH_EFFECT_PIECE01 + i % 3, e.position, e.angle, e.light, 0, e.owner, 0, 0, 0, 0.0);
    }
}

// --- MODEL_EMPIREGUARDIAN_BLOW_OF_DESTRUCTION -----------------------------------------------------------

/// ZzzEffect.cpp:5997.
fn blow_create(e: &mut Effect, ctx: &mut Ctx) {
    match e.sub_type {
        0 => {
            e.life_time = 40.0;
            // The original requires a character owner.
            if let Some(owner_angle) = ctx.actor(e.owner).map(|a| a.angle) {
                e.position += rotate(owner_angle, Vec3::new(-20.0, -100.0, 0.0));
            }
            e.start_position = e.light;
            e.light = Vec3::splat(1.2);
            ctx.create_effect(MODEL_EMPIREGUARDIAN_BLOW_OF_DESTRUCTION, e.start_position, e.angle, e.light, 1, e.owner, -1, 0, 0, e.scale);
        }
        1 => {
            e.life_time = 40.0;
            e.light = Vec3::splat(1.2);
            e.position.z = 150.0;
            e.scale *= 2.0;
        }
        _ => {}
    }
}

/// ZzzEffect.cpp:16834.
fn blow_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    let lt = life(e);
    if e.sub_type == 0 && lt <= 24 {
        if lt >= 15 {
            let quake = (ctx.rand() % 8 - 4) as f32 * 0.1;
            ctx.set_earthquake(quake);
        }
        // else: EarthQuake = 0 (no shake this tick).
        if lt == 23 {
            let light = Vec3::new(0.3, 0.3, 1.0);
            // (PKKey, SkillIndex = o->Kind, Skill = o->Skill): the engine's `kind` is the `Skill`
            // argument and its `skill` the `SkillIndex` one.
            for _ in 0..2 {
                ctx.create_effect(MODEL_NIGHTWATER_01, e.position, e.angle, light, 0, me(e), e.pk_key, e.skill, e.kind, e.scale);
            }
            ctx.create_effect(MODEL_KNIGHT_PLANCRACK_A, e.position, e.angle, light, 0, me(e), 0, 0, 0, e.scale * 0.2);
            let dir_full = e.start_position - e.position;
            let length = dir_full.length();
            let dir = normalize_mu(dir_full);
            let num = (length / 100.0) as i32 + 1;
            let owner_angle = ctx.actor(e.owner).map_or(Vec3::ZERO, |a| a.angle);
            for i in 0..num {
                let pos = e.position + dir * (55.0 * i as f32);
                let mut angle = owner_angle;
                if i % 2 == 0 {
                    angle.z += (ctx.rand() % 20 + 10) as f32;
                } else {
                    angle.z -= (ctx.rand() % 20 + 10) as f32;
                }
                ctx.create_effect(MODEL_KNIGHT_PLANCRACK_B, pos, angle, light, 0, me(e), 0, 0, 0, e.scale);
            }
        }
        e.light /= 1.05;
    } else if e.sub_type == 1 && lt <= 24 {
        if lt >= 15 {
            for _ in 0..15 {
                let mut pos = e.position;
                pos.x += (ctx.rand() % 400 - 200) as f32;
                pos.y += (ctx.rand() % 400 - 200) as f32;
                pos.z += (ctx.rand() % 400 - 200) as f32;
                let scale = e.scale * 1.6 + (ctx.rand() % 10) as f32 * 0.1;
                let index = if ctx.rand() % 2 == 0 { BITMAP_WATERFALL_5 } else { BITMAP_WATERFALL_3 };
                ctx.create_particle(index, pos, e.angle, Vec3::new(0.3, 0.3, 1.0), 8, scale, None);
                if ctx.rand() % 2 == 0 {
                    ctx.create_particle(BITMAP_SMOKE, pos, e.angle, Vec3::ONE, 55, e.scale * 0.9, None);
                }
            }
        }
        if lt == 23 {
            let light = Vec3::new(0.3, 0.3, 1.0);
            ctx.create_effect(MODEL_NIGHTWATER_01, e.position, e.angle, light, 0, me(e), -1, 0, 0, e.scale * 1.1);
            ctx.create_effect(MODEL_NIGHTWATER_01, e.position, e.angle, light, 0, me(e), -1, 0, 0, e.scale * 1.2);
            ctx.create_effect(MODEL_RAKLION_BOSS_CRACKEFFECT, e.position, e.angle, light, 0, me(e), -1, 0, 0, e.scale * 0.4);
            // `i < 5 + rand()%3` is evaluated (with a new rand()) before every iteration.
            let mut i = 0;
            while i < 5 + ctx.rand() % 3 {
                let p = Vec3::new(0.0, rf(ctx, 150), 0.0);
                let angle = Vec3::new(0.0, 0.0, rf(ctx, 360));
                let pos = rotate(angle, p) + e.position;
                let ty = MODEL_STONE1 + ctx.rand() % 2;
                ctx.create_effect(ty, pos, e.angle, e.light, 13, None, -1, 0, 0, 0.0);
                i += 1;
            }
        }
        e.light /= 1.05;
    }
    move_tail(e);
}

/// ZzzEffect.cpp:19058.
fn blow_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    if life(e) > 24 {
        return;
    }
    if e.sub_type == 0 {
        let pos = e.position + Vec3::new(0.0, 0.0, 65.0);
        let scale = (draw_rand(ctx, e, 1) % 10) as f32 * 0.011 + 3.0;
        sprite_glow(out, ctx, BITMAP_SWORD_EFFECT_MONO, pos, scale, Vec3::new(0.5, 0.5, 1.0), 0.0);
    } else if e.sub_type == 1 {
        let light = Vec3::new(e.light.x * 0.5, e.light.y * 0.5, e.light.z);
        let pos = e.position + Vec3::new(0.0, 0.0, 100.0);
        sprite_glow(out, ctx, BITMAP_LIGHT, pos, e.scale, light, 0.0);
    }
}

// --- MODEL_EFFECT_UMBRELLA_DIE / GOLD -------------------------------------------------------------------

/// ZzzEffect.cpp:6041.
fn umbrella_die_create(e: &mut Effect, _ctx: &mut Ctx) {
    if e.sub_type == 0 {
        e.life_time = 30.0;
    }
}

/// ZzzEffect.cpp:16963.
fn umbrella_die_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    let lt = life(e);
    if e.sub_type == 0 && (lt == 28 || lt == 18 || lt == 8) {
        // The original requires an owner.
        if let Some(pos) = ctx.target_position(e.owner) {
            ctx.create_effect(BITMAP_RING_OF_GRADATION, pos, e.angle, Vec3::new(1.0, 0.2, 0.5), 0, e.owner, 0, 0, 0, 1.2);
        }
    }
    move_tail(e);
}

/// ZzzEffect.cpp:6048.
fn umbrella_gold_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = (ctx.rand() % 10 + 50) as f32;
    e.scale = 2.0 + (ctx.rand() % 10 - 5) as f32 * 0.2;
    e.angle.x = rf(ctx, 360);
    e.angle.y = rf(ctx, 360);
    e.angle.z = rf(ctx, 360);
    e.gravity = (ctx.rand() % 10 + 10) as f32;
    let x = (ctx.rand() % 10 - 5) as f32 * 0.1;
    let y = (ctx.rand() % 60 - 30) as f32 * 0.1;
    let p = Vec3::new(x, y, 0.0) * 1.2;
    e.direction = rotate(e.angle, p);
    e.position.z = ctx.terrain_height(e.position.x, e.position.y) + 100.0;
}

/// ZzzEffect.cpp:16975.
fn umbrella_gold_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    e.angle.x += 10.0;
    e.position.x += e.direction.x * 2.2;
    e.position.y += e.direction.y * 2.2;
    e.position.z += e.gravity * 1.5;
    e.gravity -= 1.5;
    let height = ctx.terrain_height(e.position.x, e.position.y);
    if e.position.z < height {
        e.position.z = height;
        e.gravity = -e.gravity * 0.3;
        e.life_time -= 2.0;
    }
    move_tail(e);
}

/// ZzzObject.cpp:2397.
fn umbrella_gold_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    out.models.push(ModelDraw::of_effect(e));
    // TODO(original): plus mesh 0 with RENDER_BRIGHT|RENDER_CHROME.
}

// --- MODEL_EFFECT_EG_GUARDIANDEFENDER_ATTACK2 -----------------------------------------------------------

/// ZzzEffect.cpp:6065.
fn defender_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 20.0;
    e.scale = 0.9;
}

/// ZzzEffect.cpp:16991.
fn defender_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    if life(e) <= 18 {
        e.light /= 1.4;
        e.scale *= 1.1;
    }
    ctx.create_sprite(BITMAP_SHOCK_WAVE, e.position, e.scale, e.light, 0.0, 0);
    ctx.create_particle(BITMAP_FIRE + 2, e.position, e.angle, e.light, 16, 2.5, None);
    move_tail(e);
}
