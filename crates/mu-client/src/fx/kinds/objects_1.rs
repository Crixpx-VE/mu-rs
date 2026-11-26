//! Map objects with effects (1): Lorencia, Dungeon, Devias, Noria and Lost Tower.
//!
//! Translated from `ZzzObject.cpp`: `MoveObject` (≈4746-4990), `UpdateObjectVisual` (≈3608-3790) and
//! what `CreateObject` (≈5550-5728) creates only once (the portals). `CreateFire` comes from
//! `ZzzEffectFireLeave.cpp` (≈66-97). Only what creates effects or lights is here: the
//! flickering, UV scrolling and hidden meshes are done by `objects.rs`.
//!
//! Also here are the effects used only by these objects that did not exist in the engine:
//! `MODEL_DUNGEON_STONE01` (Dungeon stones), `MODEL_WARP/WARP2/WARP3` (Devias and Noria
//! portals) and the `BITMAP_FLAME` effect (Lost Tower flames; also used by skills).
//! Translated from `ZzzEffect.cpp` (`CreateEffect` ≈535-547, ≈822-851, ≈2405-2414; common tail
//! ≈7646-7711; `BITMAP_FLAME` drawing ≈9305), `Behaviors/MoveHandlers.cpp` (`Move_MODEL_WARP3`,
//! `Move_BITMAP_FLAME`, `Move_MODEL_DUNGEON_STONE01`) and `EffectTypes.json` (`warp3`).

use bevy::math::{Vec2, Vec3};

use crate::fx::core::*;

// `gMapManager.WorldActive`.
const WD_0LORENCIA: i32 = 0;
const WD_1DUNGEON: i32 = 1;
const WD_2DEVIAS: i32 = 2;
const WD_3NORIA: i32 = 3;
const WD_4LOSTTOWER: i32 = 4;

pub fn register(r: &mut Registry) {
    // --- Lorencia ---
    r.object(WD_0LORENCIA, MODEL_LIGHT01, lorencia_light01);
    r.object(WD_0LORENCIA, MODEL_LIGHT01 + 1, lorencia_light02);
    r.object(WD_0LORENCIA, MODEL_LIGHT01 + 2, lorencia_light03);
    r.object(WD_0LORENCIA, MODEL_BRIDGE, lorencia_bridge);
    r.object(WD_0LORENCIA, MODEL_DUNGEON_GATE, lorencia_dungeon_gate);
    r.object(WD_0LORENCIA, MODEL_FIRE_LIGHT01, lorencia_fire_light01);
    r.object(WD_0LORENCIA, MODEL_FIRE_LIGHT01 + 1, lorencia_fire_light02);
    r.object(WD_0LORENCIA, MODEL_STREET_LIGHT, lorencia_street_light);
    r.object(WD_0LORENCIA, MODEL_BONFIRE, lorencia_bonfire);
    r.object(WD_0LORENCIA, MODEL_CANDLE, lorencia_candle);
    r.object(WD_0LORENCIA, MODEL_WATERSPOUT, lorencia_waterspout);
    r.object(WD_0LORENCIA, MODEL_MERCHANT_ANIMAL01, lorencia_merchant_animal);

    // --- Dungeon ---
    r.object(WD_1DUNGEON, 52, dungeon_52);
    r.object(WD_1DUNGEON, 41, dungeon_41);
    r.object(WD_1DUNGEON, 42, dungeon_42);

    // --- Devias ---
    r.object(WD_2DEVIAS, 30, devias_30);
    r.object(WD_2DEVIAS, 66, devias_66);
    r.object(WD_2DEVIAS, 100, devias_100);
    r.object(WD_2DEVIAS, MODEL_WARP, devias_warp);

    // --- Noria ---
    r.object(WD_3NORIA, MODEL_WARP, noria_warp);
    r.object(WD_3NORIA, 1, noria_1);
    r.object(WD_3NORIA, 9, noria_9);
    r.object(WD_3NORIA, 17, noria_17);
    r.object(WD_3NORIA, 35, noria_35);
    r.object(WD_3NORIA, 39, noria_39);

    // --- Lost Tower ---
    r.object(WD_4LOSTTOWER, 9, losttower_9);
    r.object(WD_4LOSTTOWER, 19, losttower_19_20);
    r.object(WD_4LOSTTOWER, 20, losttower_19_20);
    r.object(WD_4LOSTTOWER, 24, losttower_24);
    r.object(WD_4LOSTTOWER, 40, losttower_40);
    // TODO(original): 38/39 CheckSkull (skulls move when stepped on; creates no effects).

    // --- Effects used only by these objects ---
    r.effect(MODEL_DUNGEON_STONE01, EffectKind { create: Some(dungeon_stone_create), step: Some(dungeon_stone_step), draw: None });
    r.effect(MODEL_WARP, EffectKind { create: Some(warp_create), step: Some(warp_step), draw: None });
    r.effect(MODEL_WARP2, EffectKind { create: Some(warp_create), step: Some(warp_step), draw: None });
    r.effect(MODEL_WARP3, EffectKind { create: Some(warp3_create), step: Some(warp_step), draw: None });
    r.effect(BITMAP_FLAME, EffectKind { create: Some(flame_create), step: Some(flame_step), draw: Some(flame_draw) });
}

// --- Common ------------------------------------------------------------------------------------------

/// `rand() % n` as `f32`.
fn rf(ctx: &mut Ctx, n: i32) -> f32 {
    (ctx.rand() % n) as f32
}

/// `Luminosity` at the start of `UpdateObjectVisual` (always consumes one `rand()`).
fn visual_luminosity(ctx: &mut Ctx) -> f32 {
    (ctx.rand() % 30 + 70) as f32 * 0.01
}

/// `(float)((int)(WorldTime * 0.1f) % 360)`.
fn time_rotation(ctx: &Ctx) -> f32 {
    ((ctx.world_time * 0.1) as i32 % 360) as f32
}

/// `CreateFire(Type, o, x, y, z)`: flame (0), smoke (1) or dark smoke (2) at a point of the object.
fn create_fire(ty: i32, o: &MapObject, ctx: &mut Ctx, x: f32, y: f32, z: f32) {
    let mut position = rotate(o.angle, Vec3::new(x, y, z)) + o.position;
    position.x += ctx.range_f(-8.0, 7.0);
    position.y += ctx.range_f(-8.0, 7.0);
    position.z += ctx.range_f(-8.0, 7.0);
    match ty {
        0 => {
            let l = ctx.range_f(6.0, 11.0) * 0.1;
            let light = Vec3::new(l, l * 0.6, l * 0.4);
            if ctx.rand_fps_check(2) {
                let sub = ctx.range_i(0, 3);
                ctx.create_particle(BITMAP_FIRE, position, o.angle, light, sub, 1.0, None);
            }
            ctx.add_terrain_light(position.x, position.y, light, 4.0);
        }
        1 => {
            if ctx.rand_fps_check(2) {
                ctx.create_particle(BITMAP_SMOKE, position, o.angle, o.light, 0, 1.0, None);
            }
        }
        2 => {
            if ctx.rand_fps_check(2) {
                ctx.create_particle(BITMAP_SMOKE, position, o.angle, o.light, 2, 1.0, None);
            }
        }
        _ => {}
    }
}

/// Sprite on an object bone (`b->TransformPosition(bones()[i], p, Position)` + `CreateSprite`).
fn bone_sprite(o: &MapObject, ctx: &mut Ctx, bone: usize, p: Vec3, ty: i32, scale: f32, light: Vec3, rotation: f32) {
    let pos = o.bone_world(bone, p);
    ctx.create_sprite(ty, pos, scale, light, rotation, 0);
}

// --- Lorencia (MoveObject) ---------------------------------------------------------------------------

fn lorencia_light01(o: &mut MapObject, ctx: &mut Ctx) {
    create_fire(0, o, ctx, 0.0, 0.0, 0.0);
    o.hidden_mesh = -2;
}

fn lorencia_light02(o: &mut MapObject, ctx: &mut Ctx) {
    create_fire(1, o, ctx, 0.0, 0.0, 0.0);
    o.hidden_mesh = -2;
}

fn lorencia_light03(o: &mut MapObject, ctx: &mut Ctx) {
    create_fire(2, o, ctx, 0.0, 0.0, 0.0);
    o.hidden_mesh = -2;
}

fn lorencia_bridge(o: &mut MapObject, ctx: &mut Ctx) {
    create_fire(0, o, ctx, 90.0, -200.0, 30.0);
    create_fire(0, o, ctx, 90.0, 200.0, 30.0);
}

fn lorencia_dungeon_gate(o: &mut MapObject, ctx: &mut Ctx) {
    create_fire(0, o, ctx, -150.0, -150.0, 140.0);
    create_fire(0, o, ctx, 150.0, -150.0, 140.0);
}

fn lorencia_fire_light01(o: &mut MapObject, ctx: &mut Ctx) {
    create_fire(0, o, ctx, 0.0, 0.0, 200.0);
}

fn lorencia_fire_light02(o: &mut MapObject, ctx: &mut Ctx) {
    create_fire(0, o, ctx, 0.0, -30.0, 60.0);
}

/// Street lamp: warm radius-3 light that flickers a bit.
fn lorencia_street_light(o: &mut MapObject, ctx: &mut Ctx) {
    let l = (ctx.rand() % 2 + 6) as f32 * 0.1;
    let light = Vec3::new(l, l * 0.8, l * 0.6);
    ctx.add_terrain_light(o.position.x, o.position.y, light, 3.0);
}

fn lorencia_bonfire(o: &mut MapObject, ctx: &mut Ctx) {
    create_fire(0, o, ctx, 0.0, 0.0, 60.0);
    // BlendMeshLight = (rand()%6+4)*0.1: drawn by objects.rs; here only the rand() is consumed.
    let _ = ctx.rand();
}

/// Candle: orange radius-3 light.
fn lorencia_candle(o: &mut MapObject, ctx: &mut Ctx) {
    let l = (ctx.rand() % 4 + 3) as f32 * 0.1;
    let light = Vec3::new(l, l * 0.6, l * 0.2);
    ctx.add_terrain_light(o.position.x, o.position.y, light, 3.0);
}

// --- Lorencia (UpdateObjectVisual) -------------------------------------------------------------------

/// Fountain: smoke splashes on bones 1 and 4 of the jet.
fn lorencia_waterspout(o: &mut MapObject, ctx: &mut Ctx) {
    let _lum = visual_luminosity(ctx);
    if ctx.rand_fps_check(2) {
        let px = rf(ctx, 32) - 16.0;
        let pz = rf(ctx, 32) - 16.0;
        let pos = o.bone_world(1, Vec3::new(px, -20.0, pz));
        ctx.create_particle(BITMAP_SMOKE, pos, o.angle, o.light, 0, 1.0, None);
        let px = rf(ctx, 32) - 16.0;
        let pz = rf(ctx, 32) - 16.0;
        let pos = o.bone_world(4, Vec3::new(px, -80.0, pz));
        ctx.create_particle(BITMAP_SMOKE, pos, o.angle, o.light, 0, 1.0, None);
    }
}

/// Merchant's animal: two lights on bones 48 and 57.
fn lorencia_merchant_animal(o: &mut MapObject, ctx: &mut Ctx) {
    let lum = visual_luminosity(ctx);
    let scale = lum * 5.0;
    let light = Vec3::new(lum * 0.6, lum * 0.3, lum * 0.1);
    // The original passes an uninitialized `Position` as offset: taken as (0, 0, 0).
    bone_sprite(o, ctx, 48, Vec3::ZERO, BITMAP_LIGHT, scale, light, 0.0);
    bone_sprite(o, ctx, 57, Vec3::ZERO, BITMAP_LIGHT, scale, light, 0.0);
}

// --- Dungeon -------------------------------------------------------------------------------------------

/// Stones falling from the ceiling.
fn dungeon_52(o: &mut MapObject, ctx: &mut Ctx) {
    if ctx.rand_fps_check(3) {
        ctx.create_effect(MODEL_DUNGEON_STONE01, o.position, o.angle, o.light, 0, None, -1, 0, 0, 0.0);
    }
    o.hidden_mesh = -2;
}

fn dungeon_41(o: &mut MapObject, ctx: &mut Ctx) {
    create_fire(0, o, ctx, 0.0, -30.0, 240.0);
}

fn dungeon_42(o: &mut MapObject, ctx: &mut Ctx) {
    create_fire(0, o, ctx, 0.0, 0.0, 190.0);
}

// --- Devias --------------------------------------------------------------------------------------------

/// Brazier: real fire and smoke at height 160.
fn devias_30(o: &mut MapObject, ctx: &mut Ctx) {
    let position = Vec3::new(o.position.x, o.position.y, o.position.z + 160.0);
    ctx.create_particle(BITMAP_TRUE_FIRE, position, o.angle, o.light, 0, o.scale, None);
    let scale = 0.5 + rf(ctx, 9) * 0.1;
    ctx.create_particle(BITMAP_SMOKE, position, o.angle, o.light, 21, scale, None);
}

fn devias_66(o: &mut MapObject, ctx: &mut Ctx) {
    create_fire(0, o, ctx, 0.0, 0.0, 50.0);
}

/// Two beams spinning in opposite directions 150 above bone 0 (hidden object).
fn devias_100(o: &mut MapObject, ctx: &mut Ctx) {
    let lum = visual_luminosity(ctx);
    let light = Vec3::splat(lum);
    let rotation = time_rotation(ctx);
    let pos = o.bone_world(0, Vec3::new(0.0, 0.0, 150.0));
    ctx.create_sprite(BITMAP_LIGHTNING + 1, pos, 2.5, light, rotation, 0);
    ctx.create_sprite(BITMAP_LIGHTNING + 1, pos, 2.5, light, -rotation, 0);
}

/// Devias portal (`CreateObject`): three rings at height 360, SubType 1.
fn devias_warp(o: &mut MapObject, ctx: &mut Ctx) {
    if o.created {
        return;
    }
    // In `CreateObject` the object light is still (0, 0, 0); the effect sets it every tick.
    let light = Vec3::ZERO;
    let p = o.position;
    ctx.create_effect(MODEL_WARP, Vec3::new(p.x, p.y, p.z + 360.0), o.angle, light, 1, None, -1, 0, 0, 0.0);
    ctx.create_effect(MODEL_WARP2, Vec3::new(p.x, p.y + 4.0, p.z + 360.0), o.angle, light, 1, None, -1, 0, 0, 0.0);
    ctx.create_effect(MODEL_WARP3, Vec3::new(p.x, p.y + 20.0, p.z + 360.0), o.angle, light, 1, None, -1, 0, 0, 0.0);
}

// --- Noria ---------------------------------------------------------------------------------------------

/// Noria portal: five rings on load (`CreateObject`) and sparks every tick (`MoveObject`).
fn noria_warp(o: &mut MapObject, ctx: &mut Ctx) {
    let p = o.position;
    if !o.created {
        let light = Vec3::ZERO;
        ctx.create_effect(MODEL_WARP, Vec3::new(p.x, p.y, p.z + 350.0), o.angle, light, 0, None, -1, 0, 0, 0.0);
        ctx.create_effect(MODEL_WARP2, Vec3::new(p.x, p.y + 4.0, p.z + 350.0), o.angle, light, 0, None, -1, 0, 0, 0.0);
        ctx.create_effect(MODEL_WARP, Vec3::new(p.x, p.y + 8.0, p.z + 350.0), o.angle, light, 0, None, -1, 0, 0, 0.0);
        ctx.create_effect(MODEL_WARP2, Vec3::new(p.x, p.y + 12.0, p.z + 350.0), o.angle, light, 0, None, -1, 0, 0, 0.0);
        ctx.create_effect(MODEL_WARP3, Vec3::new(p.x, p.y + 20.0, p.z + 350.0), o.angle, light, 0, None, -1, 0, 0, 0.0);
    }
    let position = Vec3::new(p.x, p.y - 50.0, p.z + 350.0);
    ctx.create_particle(BITMAP_SPARK + 1, position, o.angle, Vec3::splat(0.5), 9, 1.4, None);
}

/// Bluish light of the Noria flowers/lamps.
fn noria_light(lum: f32) -> Vec3 {
    Vec3::new(lum * 0.4, lum * 0.7, lum * 1.0)
}

fn noria_9(o: &mut MapObject, ctx: &mut Ctx) {
    let light = noria_light(visual_luminosity(ctx));
    bone_sprite(o, ctx, 1, Vec3::ZERO, BITMAP_LIGHT, 1.5, light, 0.0);
}

fn noria_35(o: &mut MapObject, ctx: &mut Ctx) {
    let light = noria_light(visual_luminosity(ctx));
    bone_sprite(o, ctx, 3, Vec3::ZERO, BITMAP_LIGHT, 1.5, light, 0.0);
}

fn noria_1(o: &mut MapObject, ctx: &mut Ctx) {
    let light = noria_light(visual_luminosity(ctx));
    for bone in [2, 4, 6] {
        bone_sprite(o, ctx, bone, Vec3::ZERO, BITMAP_LIGHT, 0.5, light, 0.0);
    }
}

fn noria_17(o: &mut MapObject, ctx: &mut Ctx) {
    let light = noria_light(visual_luminosity(ctx));
    for bone in [4, 7, 10, 13] {
        bone_sprite(o, ctx, bone, Vec3::ZERO, BITMAP_LIGHT, 1.0, light, 0.0);
    }
}

/// The big Noria tree/fountain: spinning beams, lights with flares and jumping sparks.
fn noria_39(o: &mut MapObject, ctx: &mut Ctx) {
    let lum = visual_luminosity(ctx);
    let light = Vec3::new(lum * 0.4, lum * 0.8, lum * 1.0);
    let rotation = time_rotation(ctx);
    let pos = o.bone_world(57, Vec3::ZERO);
    ctx.create_sprite(BITMAP_LIGHTNING + 1, pos, 1.0, light, rotation, 0);
    ctx.create_sprite(BITMAP_LIGHTNING + 1, pos, 1.0, light, -rotation, 0);

    let light = Vec3::ONE;
    for bone in 61..=65 {
        let pos = o.bone_world(bone, Vec3::ZERO);
        ctx.create_sprite(BITMAP_LIGHT, pos, 1.0, light, 0.0, 0);
        if ctx.rand_fps_check(32) {
            ctx.create_particle(BITMAP_SHINY, pos, o.angle, light, 0, 1.0, None);
            ctx.create_particle(BITMAP_SHINY, pos, o.angle, light, 1, 1.0, None);
        }
    }
    let pos = o.bone_world(58, Vec3::ZERO);
    if ctx.rand_fps_check(8) {
        for _ in 0..8 {
            let ax = rf(ctx, 60) + 60.0;
            let az = rf(ctx, 30);
            let angle = Vec3::new(ax, 90.0 + 50.0, az);
            ctx.create_joint(BITMAP_JOINT_SPARK, pos, pos, angle, 0, None, 10.0);
            ctx.create_particle(BITMAP_SPARK, pos, angle, light, 0, 1.0, None);
        }
    }
}

// --- Lost Tower ------------------------------------------------------------------------------------------

/// Statue orbs: red (19, `BITMAP_MAGIC+1`) or blue (20, `BITMAP_LIGHTNING+1`).
fn losttower_19_20(o: &mut MapObject, ctx: &mut Ctx) {
    let lum = visual_luminosity(ctx);
    let (bitmap, light) = if o.ty == 19 {
        (BITMAP_MAGIC + 1, Vec3::new(lum * 1.0, lum * 0.2, lum * 0.0))
    } else {
        (BITMAP_LIGHTNING + 1, Vec3::new(lum * 0.4, lum * 0.8, lum * 1.0))
    };
    let rotation = time_rotation(ctx);
    for (bone, scale) in [(15, 0.3), (19, 0.3), (21, 1.5)] {
        let pos = o.bone_world(bone, Vec3::ZERO);
        ctx.create_sprite(bitmap, pos, scale, light, rotation, 0);
        ctx.create_sprite(bitmap, pos, scale, light, -rotation, 0);
    }
    // (no `break` in the original: falls into the Stadium `switch`, which has none of these types)
}

/// Beams spinning 260 above the object.
fn losttower_40(o: &mut MapObject, ctx: &mut Ctx) {
    let lum = visual_luminosity(ctx);
    let light = Vec3::splat(lum);
    let rotation = time_rotation(ctx);
    let pos = o.position + Vec3::new(0.0, 0.0, 260.0);
    ctx.create_sprite(BITMAP_LIGHTNING + 1, pos, 2.5, light, rotation, 0);
    ctx.create_sprite(BITMAP_LIGHTNING + 1, pos, 2.5, light, -rotation, 0);
}

/// `case WD_4LOSTTOWER` has no `break` and falls into Stadium's: type 9 lights a light
/// on its bone 1 (like the Stadium torch).
fn losttower_9(o: &mut MapObject, ctx: &mut Ctx) {
    let lum = visual_luminosity(ctx);
    let scale = lum * 5.0;
    let light = Vec3::new(lum * 0.6, lum * 0.3, lum * 0.1);
    // Uninitialized `Position` as offset in the original: (0, 0, 0).
    bone_sprite(o, ctx, 1, Vec3::ZERO, BITMAP_LIGHT, scale, light, 0.0);
}

/// Ground flames (hidden object): a `BITMAP_FLAME` every now and then.
fn losttower_24(o: &mut MapObject, ctx: &mut Ctx) {
    o.hidden_mesh = -2;
    if ctx.rand_fps_check(64) {
        ctx.create_effect(BITMAP_FLAME, o.position, o.angle, o.light, 0, None, -1, 0, 0, 0.0);
    }
}

// --- Effects: common ------------------------------------------------------------------------------------

/// `Luminosity` at the start of `MoveEffect` (always consumes one `rand()`).
fn effect_luminosity(e: &Effect, ctx: &mut Ctx) -> f32 {
    let mut l = (ctx.rand() % 4 + 7) as f32 * 0.1;
    if e.life_time < 5.0 {
        l -= (5.0 - e.life_time) * 0.2;
        if l < 0.0 {
            l = 0.0;
        }
    }
    l
}

/// Common tail of `MoveEffect`: `PlayAnimation` (advance the frame, models only) and
/// `MoveParticle(o, true)`.
fn move_tail(e: &mut Effect) {
    if e.ty >= MODEL_BIRD01 && e.ty < MODEL_SKILL_END {
        e.animation_frame += e.velocity;
    }
    e.position += rotate(e.angle, e.direction);
}

/// `Random::DrawRand(o, salt)`: fixed randomness per frame, effect and salt (does not touch the logic `rand()`).
fn draw_rand(ctx: &DrawCtx, e: &Effect, salt: u32) -> i32 {
    let frame = (ctx.world_time / 40.0) as u64;
    let mut x = frame ^ ((salt as u64) << 32) ^ (e.id as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    (x & 0x7fff) as i32
}

// --- MODEL_DUNGEON_STONE01: stones falling in the Dungeon ---------------------------------------------

fn dungeon_stone_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = (ctx.rand() % 16 + 24) as f32;
    e.scale = (ctx.rand() % 8 + 6) as f32 * 0.1;
    e.gravity = -rf(ctx, 4);
    let x = (ctx.rand() % 64 - 32) as f32;
    let y = -((ctx.rand() % 32 + 50) as f32);
    let z = (ctx.rand() % 128 + 200) as f32;
    e.position += rotate(e.angle, Vec3::new(x, y, z));
}

fn dungeon_stone_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = effect_luminosity(e, ctx);
    e.position.z += e.gravity;
    e.gravity -= 1.0;
    let height = ctx.terrain_height(e.position.x, e.position.y);
    if e.position.z < height {
        e.position.z = height;
        e.gravity = -e.gravity * 0.4;
        e.life_time -= 4.0;
        e.direction.y -= 2.0;
    }
    move_tail(e);
}

// --- MODEL_WARP / WARP2 / WARP3: portal rings -----------------------------------------------

fn warp_create(e: &mut Effect, ctx: &mut Ctx) {
    // (the original zeroes the `Light` argument, not `o->Light`: no effect)
    e.blend_mesh = -2;
    e.life_time = 0xffffff as f32;
    e.scale = 1.3 + rf(ctx, 50) / 100.0;
    e.gravity = rf(ctx, 80) / 10.0;
    e.velocity = rf(ctx, 100) / 1000.0 + 0.01;
}

/// Creation from `EffectTypes.json` ("warp3").
fn warp3_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 16_777_215.0;
    e.scale = 0.6;
    e.blend_mesh = -2;
    // `Gravity` is not initialized in the original (left over from the slot's previous effect): 0.
    e.gravity = 0.0;
}

/// `Move_MODEL_WARP3`: oscillating color and rotation about Y.
fn warp_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = effect_luminosity(e, ctx);
    let t = ctx.world_time;
    if e.sub_type == 0 {
        let t1 = (t * 0.0011).sin() * 0.2;
        let t2 = (t * 0.0017).sin() * 0.2;
        let t3 = (t * 0.0013).sin() * 0.2;
        e.light = Vec3::new(t1 + 0.01, t2 + 0.01, t3 + 0.01);
        e.angle.y += 4.0 + e.gravity;
    } else if e.sub_type == 1 {
        let t1 = (t * 0.0011).sin() * 0.05;
        e.light = Vec3::new(0.0 + t1, 0.2 + t1, 0.1 + t1);
        e.angle.y += 2.0 + e.gravity;
    }
    move_tail(e);
}

// --- BITMAP_FLAME (effect): burning ground ------------------------------------------------------------

fn flame_create(e: &mut Effect, ctx: &mut Ctx) {
    match e.sub_type {
        0 => {
            e.life_time = 40.0;
            e.weapon = ctx.packet_serial();
        }
        1 | 2 => {
            e.life_time = 10.0;
            e.angle = Vec3::ZERO;
        }
        3 => e.life_time = 15.0,
        4 => {
            e.scale = 0.01;
            e.life_time = 10.0;
            e.angle = Vec3::ZERO;
        }
        5 => e.life_time = 20.0,
        6 => e.life_time = 40.0,
        _ => {}
    }
}

fn flame_step(e: &mut Effect, ctx: &mut Ctx) {
    let lum = effect_luminosity(e, ctx);
    let white = Vec3::ONE;
    match e.sub_type {
        0 => {
            for _ in 0..6 {
                let x = (ctx.rand() % 50 - 25) as f32;
                let y = (ctx.rand() % 50 - 25) as f32;
                let pos = Vec3::new(x, y, 0.0) + e.position;
                ctx.create_particle(BITMAP_FLAME, pos, e.angle, white, 0, 1.0, None);
            }
            let ty = MODEL_STONE1 + ctx.rand() % 2;
            ctx.create_effect(ty, e.position, e.angle, e.light, 0, None, -1, 0, 0, 0.0);
            let light = Vec3::new(lum * 1.0, lum * 0.4, lum * 0.0);
            ctx.add_terrain_light(e.position.x, e.position.y, light, 3.0);
            // Main 5.2 ZzzEffect.cpp:8796-8798 (MuMain MoveHandlers.cpp:1832-1836 also sets
            // `LifeTime = (LifeTime / 20) · 19.9` so its sub-tick frames do not hit twice; with
            // whole 25 Hz ticks that is not needed). `MoveEffect` runs before `LifeTime -= 1`.
            if ctx.is_hero(e.owner) && (e.life_time as i32) % 20 == 0 {
                ctx.attack_character_range(e.skill, e.position, 150.0, e.weapon, e.pk_key);
            }
        }
        1 | 2 => {
            for j in 0..18 {
                if ctx.rand_fps_check(1) {
                    let angle = Vec3::new(0.0, 0.0, j as f32 * 20.0);
                    let mut pos = rotate(angle, Vec3::new(0.0, 250.0, 0.0)) + e.position;
                    pos.x += (ctx.rand() % 64 - 32) as f32;
                    pos.y += (ctx.rand() % 64 - 32) as f32;
                    if e.sub_type == 1 {
                        ctx.create_particle(BITMAP_FLAME, pos, e.angle, white, 0, 1.2, None);
                    } else {
                        ctx.create_particle(BITMAP_FIRE + 3, pos, e.angle, white, 13, 2.5, None);
                    }
                }
            }
        }
        3 => {
            for _ in 0..3 {
                if ctx.rand_fps_check(1) {
                    ctx.create_particle(BITMAP_FLAME, e.position, e.angle, white, 6, 1.0, None);
                    let x = (ctx.rand() % 10 - 5) as f32;
                    let y = (ctx.rand() % 10 - 5) as f32;
                    let pos = Vec3::new(x, y, 40.0) + e.position;
                    ctx.create_particle(BITMAP_TRUE_FIRE, pos, e.angle, white, 0, 2.8, None);
                }
            }
            let x = (ctx.rand() % 10 - 5) as f32;
            let y = (ctx.rand() % 10 - 5) as f32;
            let pos = Vec3::new(x, y, -40.0) + e.position;
            ctx.create_particle(BITMAP_SMOKE, pos, e.angle, white, 21, 0.8, None);
            let ty = MODEL_STONE1 + ctx.rand() % 2;
            ctx.create_effect(ty, e.position, e.angle, e.light, 12, None, -1, 0, 0, 0.0);
            let light = Vec3::new(lum * 1.0, lum * 0.4, lum * 0.0);
            ctx.add_terrain_light(e.position.x, e.position.y, light, 3.0);
        }
        4 => {
            e.scale += 20.0;
            for j in 0..18 {
                if ctx.rand_fps_check(1) {
                    let angle = Vec3::new(0.0, 0.0, j as f32 * 20.0);
                    let mut pos = rotate(angle, Vec3::new(0.0, 150.0 - e.scale, 0.0)) + e.position;
                    pos.x += (ctx.rand() % 64 - 32) as f32;
                    pos.y += (ctx.rand() % 64 - 32) as f32;
                    ctx.create_particle(BITMAP_FLAME, pos, e.angle, white, 0, 1.2, None);
                }
            }
            let ty = MODEL_STONE1 + ctx.rand() % 2;
            ctx.create_effect(ty, e.position, e.angle, e.light, 12, None, -1, 0, 0, 0.0);
        }
        5 => {
            let x = (ctx.rand() % 32 - 16) as f32;
            let y = (ctx.rand() % 32 - 16) as f32;
            let pos = Vec3::new(x, y, 0.0) + e.position;
            ctx.create_particle(BITMAP_FLAME, pos, e.angle, white, 0, e.scale, None);
        }
        6 => {
            let x = (ctx.rand() % 32 - 16) as f32;
            let y = (ctx.rand() % 32 - 16) as f32;
            let pos = Vec3::new(x, y, 0.0) + e.position;
            ctx.create_particle(BITMAP_FLAME, pos, e.angle, e.light, 12, e.scale, None);
        }
        _ => {}
    }
    move_tail(e);
}

/// Fire mark on the ground (2×2 tiles) except in subtypes 3 and 6.
fn flame_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    if e.sub_type == 3 || e.sub_type == 6 {
        return;
    }
    let l = (draw_rand(ctx, e, 2) % 4 + 8) as f32 * 0.1;
    // Decal(BITMAP_FLAME, x, y, 2, 2, Light, -Angle[2]): size in tiles (×100). The original
    // rotates the UV by -Angle[2], i.e. the image by +Angle[2].
    out.decals.push(DecalDraw {
        texture: BITMAP_FLAME,
        center: Vec2::new(e.position.x, e.position.y),
        width: 200.0,
        height: 200.0,
        color: [l, l, l, 1.0],
        rotation: e.angle.z,
        lift: 5.0,
        blend: Blend::Glow,
    });
}
