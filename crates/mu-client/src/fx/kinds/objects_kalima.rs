//! Map objects with effects for Kalima 1-7 and Land of Trials.
//!
//! - Kalima (`InHellas`: `WD_24HELLAS..WD_24HELLAS_END` and `WD_24HELLAS_7`), `GMHellas.cpp`:
//!   `MoveHellasObjectSetting` (:312-362, once per tick for the whole map: ambient sound,
//!   sparkles around the character and stones falling into the water), `MoveHellasVisual` (:386-400) and
//!   `RenderHellasVisual` (:434-511: lights, waterfalls, the grass floating with `CheckGrass`).
//!   The `MoveHellasObjectSetting` work runs from the first map object that moves in the tick
//!   (like `objects_2.rs` with Heaven): that is why every Kalima type is registered.
//! - Land of Trials (`WD_31HUNTING_GROUND`), `GMHuntingGround.cpp`: `MoveHuntingGroundObject`
//!   (:44-113: butterflies, ground light, the ambient sound every 5 minutes for any object) and
//!   `RenderHuntingGroundObjectVisual` (:115-153). The mist (`CreateMist`, :915) lives in `leaves.rs`.
//!
//! Plus the two effects only these maps create: the Kalima stone (`CreateEffect(9, ...)`, map
//! model 9 = `MODEL_KALIMA_FALLING_STONE`; ZzzEffect.cpp:726-732 and 7448-7462) and the
//! Land of Trials butterflies (`MODEL_BUTTERFLY01`, ZzzEffect.cpp:711-725 and 7399-7447).
//!
//! How the meshes are drawn (the water reflection, the chrome of 34, the pulsing lights of Land
//! of Trials) is in `mu-data/src/objects/rules_kalima.rs`; the water, in `water_terrain.rs`.

use std::sync::Mutex;

use bevy::math::{Vec2, Vec3};

use crate::fx::core::*;
use crate::water_terrain;

// `gMapManager.WorldActive` of these maps.
const WD_24HELLAS: i32 = 24;
const WD_24HELLAS_END: i32 = 29;
const WD_24HELLAS_7: i32 = 36;
const WD_31HUNTING_GROUND: i32 = 31;

/// `Object25/Object01..59.bmd`: Kalima types 0-58.
const KALIMA_TYPES: i32 = 59;
/// `Object32/Object01..57.bmd`: Land of Trials types 0-56.
const HUNTING_GROUND_TYPES: i32 = 57;

pub fn register(r: &mut Registry) {
    for world in (WD_24HELLAS..=WD_24HELLAS_END).chain([WD_24HELLAS_7]) {
        for ty in 0..KALIMA_TYPES {
            r.object(world, ty, kalima);
        }
    }
    // All of them: the `MoveHuntingGroundObject` ambient sound comes last, for any type.
    for ty in 0..HUNTING_GROUND_TYPES {
        r.object(WD_31HUNTING_GROUND, ty, hunting_ground);
    }
    r.effect(MODEL_KALIMA_FALLING_STONE, EffectKind { create: Some(falling_stone_create), step: Some(falling_stone_step), draw: None });
    r.effect(MODEL_BUTTERFLY01, EffectKind { create: Some(butterfly_create), step: Some(butterfly_step), draw: None });
}

/// `Random::Rand() % n` as a float.
fn rf(ctx: &mut Ctx, n: i32) -> f32 {
    (ctx.rand() % n) as f32
}

// --- Kalima ----------------------------------------------------------------------------------------

struct HellasTick {
    /// `world_time` of the last tick in which `MoveHellasObjectSetting` ran.
    time: f32,
    /// `MoveSceneFrame`: ticks counted here.
    frame: u32,
}

static HELLAS: Mutex<HellasTick> = Mutex::new(HellasTick { time: f32::NAN, frame: 0 });

/// `MoveHellasObjectSetting` (GMHellas.cpp:312-362), the first time it is called each tick.
fn hellas_setting(ctx: &mut Ctx) {
    let frame = {
        let mut s = HELLAS.lock().unwrap_or_else(|e| e.into_inner());
        if s.time == ctx.world_time {
            return;
        }
        s.time = ctx.world_time;
        s.frame = s.frame.wrapping_add(1);
        s.frame
    };
    ctx.play_sound("SOUND_KALIMA_AMBIENT");
    if frame.is_multiple_of(100) {
        let id = if ctx.rand() % 2 == 0 { "SOUND_KALIMA_AMBIENT2" } else { "SOUND_KALIMA_AMBIENT3" };
        ctx.play_sound(id);
    }
    // TODO(original): with `GetHellasLevel(class, level) == KUNDUN_ZONE` (character level ≥ 350, or
    // ≥ 350 for the `startIndex = 7` classes) and the character in (25..=51, 44..=119), every 50 ticks
    // `SOUND_KUNDUN_AMBIENT1 + rand()%2`. The engine does not have the character's class or level.

    // `if ((rand()%10)==0 && object)`: `object` is `visibleObject`, which nobody counts in Kalima
    // (`MoveObjectOnEffect` only does it in Heaven, Chaos Castle and Battle Castle): always 0, but the
    // `rand()` is consumed anyway.
    let _ = ctx.rand() % 10;

    if ctx.rand() % 5 == 0 {
        let Some((hero_pos, hero_angle)) = ctx.hero_actor().map(|a| (a.position, a.angle)) else { return };
        let light = Vec3::new(0.3, 0.8, 1.0);
        let x = hero_pos.x + (ctx.rand() % 800) as f32 - 400.0;
        let y = hero_pos.y + (ctx.rand() % 800) as f32 - 400.0;
        let mut position = Vec3::new(x, y, hero_pos.z + 50.0);
        let hero = ctx.hero.map(Target::Actor);
        ctx.create_particle(BITMAP_LIGHT, position, hero_angle, light, 7, 1.0, hero);

        if ctx.rand() % 15 == 0 {
            position.z = hero_pos.z + 800.0;
            ctx.create_effect(MODEL_KALIMA_FALLING_STONE, position, Vec3::ZERO, light, 0, None, -1, 0, 0, 0.0);
            ctx.play_sound("SOUND_KALIMA_FALLING_STONE");
        }
    }
}

fn kalima(o: &mut MapObject, ctx: &mut Ctx) {
    hellas_setting(ctx);

    let wt = ctx.world_time;
    // `RenderHellasVisual` (:434-511). 37-40 are hidden by `MoveHellasVisual` (the mesh rules).
    match o.ty {
        12 => {
            let luminosity = (wt * 0.001).sin() * 0.3 + 0.7;
            let position = o.bone_world(5, Vec3::ZERO);
            ctx.create_sprite(BITMAP_LIGHT, position, luminosity + 0.2, Vec3::new(0.6, 0.6, 1.0), 0.0, 0);
        }
        15 | 29 | 32 => {
            float_on_water(o, ctx);
            if o.ty == 32 {
                let luminosity = (wt * 0.001).sin() * 0.3 + 0.7;
                let position = o.bone_world(5, Vec3::ZERO);
                ctx.create_sprite(BITMAP_LIGHT, position, luminosity + 0.2, Vec3::new(0.6, 0.6, 1.0), 0.0, 0);
            }
        }
        35 => {
            // The owner is the map object: the particle does not use it.
            ctx.create_particle(BITMAP_LIGHT, o.position, o.angle, Vec3::new(0.3, 0.6, 1.0), 6, 1.0, None);
            o.hidden_mesh = -2;
        }
        36 => {
            ctx.create_particle(BITMAP_TRUE_BLUE, o.position, o.angle, Vec3::ONE, 0, 1.0, None);
            // `o->Scale = 0.5f`: unused (the particle uses scale 1).
            o.hidden_mesh = -2;
        }
        37 => {
            ctx.create_particle(BITMAP_WATERFALL_5, o.position, o.angle, Vec3::ONE, 0, 1.0, None);
            ctx.play_sound("SOUND_KALIMA_WATER_FALL");
        }
        38 => {
            if ctx.rand() % 2 == 0 {
                ctx.create_particle(BITMAP_WATERFALL_1, o.position, o.angle, Vec3::ONE, 0, 1.0, None);
            }
            ctx.play_sound("SOUND_KALIMA_WATER_FALL");
        }
        39 => {
            let ty = BITMAP_WATERFALL_3 + ctx.rand() % 2;
            ctx.create_particle(ty, o.position, o.angle, Vec3::ONE, 0, 1.0, None);
        }
        40 => {
            if ctx.rand() % 4 == 0 {
                ctx.create_particle(BITMAP_WATERFALL_2, o.position, o.angle, Vec3::ONE, 0, 1.0, None);
            }
        }
        _ => {}
    }
    if (37..=40).contains(&o.ty) {
        o.hidden_mesh = -2;
    }
}

/// 15, 29 and 32: `CheckGrass` and `o->Position[2] = GetWaterTerrain(x, y) + 180` (:453-465). The mesh is
/// moved the same way by `objects/movers.rs`; this is done here so the effects (the light of 32) follow it.
/// `user[0..2]` is `o->Direction` (x, y).
fn float_on_water(o: &mut MapObject, ctx: &mut Ctx) {
    if let Some(hero) = ctx.hero_actor().map(|a| (a.position.truncate(), a.action)) {
        let mut direction = Vec2::new(o.user[0], o.user[1]);
        let mut position = o.position.truncate();
        water_terrain::check_grass(&mut direction, &mut position, hero.0, water_terrain::check_grass_walking(hero.1));
        o.user[0] = direction.x;
        o.user[1] = direction.y;
        o.position.x = position.x;
        o.position.y = position.y;
    }
    if let Some(h) = water_terrain::height_at(o.position.x, o.position.y, ctx.world_time) {
        o.position.z = h + 180.0;
    }
}

/// `CreateEffect(9)` (ZzzEffect.cpp:726-732): the stone falling from the Kalima sky.
fn falling_stone_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = 30.0;
    e.scale = (ctx.rand() % 3) as f32 / 10.0 + 0.6;
    e.velocity = 10.0;
    e.pk_key = 0;
    e.direction = Vec3::new(-0.1, -0.1, -0.5);
}

/// `MoveEffect` for type 9 (ZzzEffect.cpp:7448-7462) + the common tail (`MoveParticle(o, true)`,
/// 18119-18121; type 9 is below `MODEL_BIRD01`: not animated).
fn falling_stone_step(e: &mut Effect, ctx: &mut Ctx) {
    // `Luminosity` from the start of `MoveEffect` (consumes a rand()).
    let _ = ctx.rand();
    e.position += e.direction * e.velocity;
    e.velocity += 3.0;
    e.angle.x += 5.0 * e.velocity;
    if e.position.z < 350.0 && e.pk_key == 0 {
        e.pk_key = 1;
        let x = (e.position.x / 100.0) as i32;
        let y = (e.position.y / 100.0) as i32;
        water_terrain::add_water_wave(x, y, 2, -1000);
    }
    e.position += rotate(e.angle, e.direction);
}

// --- Land of Trials --------------------------------------------------------------------------------

/// `g_MusicStartStamp` (GMHuntingGround.cpp:17): `timeGetTime()` of the last time the ambient sound played.
static MUSIC_STAMP: Mutex<Option<u64>> = Mutex::new(None);

/// `timeGetTime()`: wall-clock milliseconds (not the tick time, which advances in steps of 40).
fn time_get_time() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

fn hunting_ground(o: &mut MapObject, ctx: &mut Ctx) {
    // `MoveHuntingGroundObject` (:44-113).
    match o.ty {
        1 | 44 | 45 => {
            // `timeGetTime()%1024 < 10`: with the real clock, like the original (the tick time
            // advances in steps of 40 ms and would give a different frequency).
            if time_get_time() % 1024 < 10 {
                let sub_type = match o.ty {
                    1 => 0,
                    44 => 1,
                    _ => 2,
                };
                // The owner is the object: `CreateEffect` only uses its scale (`Direction[1] = -5·Scale`),
                // passed in the scale parameter (see `butterfly_create`).
                ctx.create_effect(MODEL_BUTTERFLY01, o.position, o.angle, Vec3::ONE, sub_type, None, -1, 0, 0, o.scale);
            }
            o.hidden_mesh = -2;
        }
        3 | 53 => o.hidden_mesh = -2,
        // 27: `Position[2] += sin(Timer + WorldTime·0.0024)·0.3` moves the mesh: `objects/movers.rs`.
        42 => {
            let luminosity = (ctx.rand() % 4 + 3) as f32 * 0.1;
            let light = Vec3::new(luminosity * 0.9, luminosity * 0.2, luminosity * 0.1);
            ctx.add_terrain_light(o.position.x, o.position.y, light, 3.0);
            o.hidden_mesh = -2;
        }
        _ => {}
    }
    // The ambient sound every 300 s (the first as soon as an object moves: `g_MusicStartStamp` starts at 0).
    let now = time_get_time();
    {
        let mut stamp = MUSIC_STAMP.lock().unwrap_or_else(|e| e.into_inner());
        if stamp.is_none_or(|s| now.saturating_sub(s) > 300_000) {
            *stamp = Some(now);
            ctx.play_sound("SOUND_BC_HUNTINGGROUND_AMBIENT");
        }
    }

    // `RenderHuntingGroundObjectVisual` (:115-153).
    match o.ty {
        3 => {
            if ctx.rand() % 3 == 0 {
                ctx.create_particle(BITMAP_WATERFALL_3, o.position, o.angle, Vec3::ONE, 3, o.scale, None);
            }
        }
        53 => {
            if ctx.rand() % 3 == 0 {
                ctx.create_particle(BITMAP_SMOKE, o.position, o.angle, Vec3::ONE, 22, o.scale, None);
            }
        }
        49 => {
            let position = o.bone_world(3, Vec3::new(0.0, -10.0, 0.0));
            let red = Vec3::new(1.0, 0.0, 0.0);
            let luminosity = (ctx.world_time * 0.002).sin() * 0.35 + 0.65;
            ctx.create_sprite(BITMAP_LIGHT, position, luminosity * 1.2 + 0.3, red, 0.0, 0);
            ctx.create_sprite(BITMAP_LIGHT, position, 0.5, red, 0.0, 0);
            let rotation = rf(ctx, 360);
            ctx.create_sprite(BITMAP_SHINY + 1, position, 1.2, Vec3::splat(0.3), rotation, 0);
        }
        _ => {}
    }
}

/// `CreateEffect(MODEL_BUTTERFLY01)` (ZzzEffect.cpp:711-725). `e.scale` carries the scale of the
/// owner object (`o->Owner->Scale`).
fn butterfly_create(e: &mut Effect, ctx: &mut Ctx) {
    let owner_scale = e.scale;
    // TODO(original): in character selection (`SceneFlag == CHARACTER_SCENE`) `HiddenMesh = -2`.
    e.life_time = 250.0;
    e.velocity = 0.3;
    e.light_enable = false;
    e.kind = ctx.rand() % 2;
    e.direction = Vec3::new(0.0, -5.0 * owner_scale, 0.0);
    e.scale = if e.sub_type == 3 { 0.9 } else { 0.25 };
}

/// `MoveEffect` for `MODEL_BUTTERFLY01` (ZzzEffect.cpp:7399-7447) + the common tail: animates
/// (`PlayAnimation` with `Velocity`) and moves along its direction (`MoveParticle(o, true)`).
fn butterfly_step(e: &mut Effect, ctx: &mut Ctx) {
    // `Luminosity` from the start of `MoveEffect`.
    let _ = ctx.rand();
    if e.kind > 0 {
        e.angle.z += rf(ctx, 10);
    } else {
        e.angle.z -= rf(ctx, 10);
    }
    if ctx.rand() % 32 == 0 {
        e.direction.z = (ctx.rand() % 15 - 7) as f32;
    }
    e.direction.z += (ctx.rand() % 15 - 7) as f32 * 0.2;
    let height = ctx.terrain_height(e.position.x, e.position.y);
    if e.position.z < height + 50.0 {
        e.direction.z *= 0.8;
        e.direction.z += 1.0;
    }
    if e.position.z > height + 150.0 {
        e.direction.z *= 0.8;
        e.direction.z -= 1.0;
    }
    e.position.z += (ctx.rand() % 15 - 7) as f32 * 0.3;

    let luminosity = (ctx.rand() % 32 + 64) as f32 * 0.01;
    let light = match e.sub_type {
        0 => Vec3::new(luminosity * 0.4, luminosity * 0.8, luminosity * 0.6),
        1 => Vec3::new(luminosity * 0.4, luminosity * 0.6, luminosity * 0.8),
        2 => Vec3::new(luminosity * 0.6, luminosity * 0.8, luminosity * 0.4),
        3 => {
            if ctx.rand() % 2 == 0 {
                let offset = Vec3::new(rf(ctx, 16) - 8.0, rf(ctx, 16) - 8.0, rf(ctx, 16) - 8.0);
                ctx.create_particle(BITMAP_SPARK, e.position + offset, e.angle, e.light, 7, 1.0, None);
            }
            Vec3::new(luminosity * 0.7, luminosity * 0.9, luminosity * 0.5)
        }
        // `Light` stays at (1, 1, 1).
        _ => Vec3::ONE,
    };
    ctx.create_sprite(BITMAP_LIGHT, e.position, 1.0, light, 0.0, 0);

    e.animation_frame += e.velocity;
    e.position += rotate(e.angle, e.direction);
}
