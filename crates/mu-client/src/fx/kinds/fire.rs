//! `MODEL_FIRE` (`Skill/Fire01.bmd`): falling and flying fire rocks, all subtypes.
//!
//! Translated from Main 5.2 `ZzzEffect.cpp`: `CreateEffect` (2760-2860), `MoveEffect` (12364-12613),
//! the `MODEL_FIRE` part of `CheckTargetRange` (366-445) and the second move of subtype 3 at the
//! end of `MoveEffect` (18140-18146). `RenderEffects` has no `case` for it, so it is drawn by the
//! `default:` `RenderObject(o)` (19191; `ZzzObject.cpp` has no special case for this type).
//!
//! Subtypes (and who creates them):
//! - 0: the Dark Wizard's Meteorite (`ZzzCharacter.cpp:1566`; also `ZzzEffect.cpp:13701`): a rock
//!   that appears 400 above and 130-161 to the side and falls diagonally (`Angle = (0, 20, 0)`)
//!   until it hits the ground: 6 stones and an explosion.
//! - 1: a fireball that flies along its angle (`ZzzCharacter.cpp:1830`, 4866) and bursts into 2
//!   stones when it reaches its owner (`CheckTargetRange`).
//! - 2: falls straight down with the caller's angle (`ZzzCharacter.cpp:1810-1814`).
//! - 3: the dragon invasion meteor (`GOBoid.cpp:1210`): only a `BITMAP_FIRE` ball with flares.
//! - 4: a bigger meteor that shakes the camera when it lands.
//! - 5: bounces along the ground (`ZzzCharacter.cpp:1953`).
//! - 6-8: Hellas / poison meteors (`ZzzCharacter.cpp:1915`, 4256; `GMHellas.cpp:725`, 1235):
//!   only lights and sprites, with an inferno ring and smoke on landing.
//! - 9: a big burning rock (`ZzzObject.cpp:3259`) with a smoke trail.
//! - other: like 1 without the owner check.

use bevy::math::Vec3;

use super::common::{self, luminosity};
use crate::fx::core::*;
use crate::fx::kinds::joints_1::magic_screw;

/// `TW_NOGROUND` (`_define.h:81`).
const TW_NOGROUND: u16 = 0x0008;

pub fn register(r: &mut Registry) {
    r.effect(MODEL_FIRE, EffectKind { create: Some(fire_create), step: Some(fire_step), draw: Some(common::draw_default) });
}

/// The effect itself as an `OBJECT *` argument (`o`).
fn me(e: &Effect) -> Option<Target> {
    Some(Target::Effect(e.id))
}

// --- CreateEffect ----------------------------------------------------------------------------------

/// ZzzEffect.cpp:2760-2860.
fn fire_create(e: &mut Effect, ctx: &mut Ctx) {
    e.blend_mesh = 1;
    match e.sub_type {
        0 => {
            e.life_time = 40.0;
            e.scale = (ctx.rand() % 8 + 10) as f32 * 0.1;
            e.position.x += 130.0 + (ctx.rand() % 32) as f32;
            e.position.z += 400.0;
            e.direction = Vec3::new(0.0, 0.0, -50.0);
            e.angle = Vec3::new(0.0, 20.0, 0.0);
        }
        2 => {
            e.life_time = 40.0;
            e.scale = (ctx.rand() % 8 + 10) as f32 * 0.1;
            e.direction = Vec3::new(0.0, 0.0, -50.0);
        }
        3 => {
            e.life_time = 80.0;
            e.scale = 0.3;
            e.direction = Vec3::new(0.0, -12.0, 0.0);
        }
        4 => {
            e.life_time = 40.0;
            e.scale = (ctx.rand() % 10 + 15) as f32 * 0.1;
            e.position.x += 130.0 + (ctx.rand() % 32) as f32;
            e.position.z += 400.0;
            // `Vector` is a macro: its arguments run in order (ZzzMathLib.h:25).
            let y = -((ctx.rand() % 20) as f32 + 10.0);
            let z = -((ctx.rand() % 10) as f32 + 20.0);
            e.direction = Vec3::new(0.0, y, z);
            e.angle = Vec3::new(0.0, 20.0, 0.0);
        }
        5 => {
            e.life_time = 40.0;
            e.gravity = 5.0;
            e.direction = Vec3::new(0.0, -30.0, 0.0);
        }
        6..=8 => {
            let sub = e.sub_type;
            e.life_time = 40.0;
            e.hidden_mesh = -2;
            e.scale = (ctx.rand() % 8 + 15) as f32 * 0.1;
            // `o->PKKey = -1; o->Velocity = PKKey;` (the argument): the animation speed.
            let pk_key_arg = e.pk_key;
            e.pk_key = -1;
            e.velocity = pk_key_arg as f32;
            // Subtype 8 has these two lines commented out (ZzzEffect.cpp:2835-2836).
            if sub != 8 {
                e.position.x += (ctx.rand() % 100 + 200) as f32;
                e.position.y += (ctx.rand() % 100 - 50) as f32;
            }
            e.position.z += (ctx.rand() % 300 + 500) as f32;
            e.direction = Vec3::new(0.0, 0.0, -50.0 - (ctx.rand() % 50) as f32);
            match sub {
                6 => {
                    e.head_angle = Vec3::new(0.0, 20.0, 0.0);
                    ctx.create_joint(BITMAP_SMOKE, e.position, e.position, e.head_angle, 0, me(e), 100.0);
                }
                7 => e.head_angle = Vec3::new(0.0, 20.0, 0.0),
                _ => {
                    e.head_angle = Vec3::new(0.0, (5 + ctx.rand() % 5) as f32, 0.0);
                    ctx.create_joint(BITMAP_SMOKE, e.position, e.position, e.head_angle, 0, me(e), 60.0);
                }
            }
        }
        9 => {
            e.blend_mesh_light = 0.0;
            e.life_time = 600.0;
            e.scale = (ctx.rand() % 10 + 20) as f32 * 0.13;
            e.position.x += 130.0 + (ctx.rand() % 32) as f32;
            e.position.z += 400.0 + (ctx.rand() % 32) as f32;
            // `-rand()%5-8.0f`: the remainder of the negated int (C truncation, -4..0).
            let y = (-ctx.rand() % 5) as f32 - 8.0;
            let z = -10.0 - (ctx.rand() % 10) as f32;
            e.direction = Vec3::new(0.0, y, z);
            e.angle = Vec3::ZERO;
        }
        _ => {
            e.life_time = 60.0;
            e.scale = (ctx.rand() % 4 + 8) as f32 * 0.1;
            e.position.z += 120.0;
            e.direction = Vec3::new(0.0, -50.0, 0.0);
        }
    }
}

// --- MoveEffect ------------------------------------------------------------------------------------

/// `CreateEffect(MODEL_STONE1 + rand() % 2, o->Position, o->Angle, light)`, `n` times.
fn stones(e: &Effect, ctx: &mut Ctx, light: Vec3, n: usize) {
    for _ in 0..n {
        let ty = MODEL_STONE1 + ctx.rand() % 2;
        ctx.create_effect(ty, e.position, e.angle, light, 0, None, -1, 0, 0, 0.0);
    }
}

/// Subtypes 0, 2, 4, 6, 7, 8: fall until the ground (ZzzEffect.cpp:12365-12494). `light` is
/// `MoveEffect`'s `Light` local.
fn fall(e: &mut Effect, ctx: &mut Ctx, light: &mut Vec3) {
    let sub = e.sub_type;
    let mut success = false;
    let mut height = 0.0;
    if sub == 4 {
        let wall = ctx.terrain_attribute(e.position.x, e.position.y) & TW_NOGROUND;
        if wall != TW_NOGROUND {
            height = ctx.terrain_height(e.position.x, e.position.y);
            if e.position.z < height {
                success = true;
                // `EarthQuake = ...` (Main 5.2 sets it directly; MuMain uses `SetEarthQuakeAt`).
                let quake = (ctx.rand() % 4 - 4) as f32 * 0.1;
                ctx.set_earthquake(quake);
                // TODO(original): every character other than the hero that is `Live && Visible
                // && !Dead` within 200 (2D) plays `PLAYER_SHOCK` (players; `SetAction_Fenrir_Damage`
                // when riding a Fenrir) or `MONSTER01_SHOCK` (ZzzEffect.cpp:12384-12405). Actors
                // are read-only snapshots: the effect cannot set their action.
            }
        } else if e.position.z < -200.0 {
            success = true;
            height = e.position.z;
        }
    } else {
        if matches!(sub, 6..=8) {
            *light = Vec3::new(0.5, 0.5, 1.0);
            ctx.create_sprite(BITMAP_LIGHT, e.position, 3.0, *light, 0.0, 0);
            *light = Vec3::ONE;
            let rotation = (ctx.rand() % 360) as f32;
            ctx.create_sprite(BITMAP_SHINY + 1, e.position, 3.0, *light, rotation, 1);
            let rotation = (ctx.rand() % 360) as f32;
            ctx.create_sprite(BITMAP_SHINY + 1, e.position, 4.0, e.light, rotation, 0);
            e.angle = e.head_angle;
        }
        let add_height = if common::in_hellas(ctx.world_active) { 50.0 } else { 0.0 };
        height = ctx.terrain_height(e.position.x, e.position.y) + add_height;
        if e.position.z < height {
            success = true;
        }
    }

    if success {
        e.position.z = height;
        e.direction = Vec3::ZERO;
        // The block's own `Position` (ZzzEffect.cpp:12445): the smoke loop below moves it, so
        // the explosion of subtypes 6-8 appears at the last smoke puff.
        let mut position = e.position + Vec3::new(0.0, 0.0, 80.0);
        if matches!(sub, 6..=8) {
            let smoke_num = match sub {
                7 => 7,
                8 => 5,
                _ => 15,
            };
            *light = Vec3::new(0.0, 0.5, 0.0);
            ctx.create_effect(MODEL_SKILL_INFERNO, e.position, Vec3::ZERO, *light, 2, me(e), 30, 0, 0, 0.0);
            for _ in 0..smoke_num {
                position.x = e.position.x + (ctx.rand() % 160 - 80) as f32;
                position.y = e.position.y + (ctx.rand() % 160 - 100) as f32;
                position.z = e.position.z + 50.0;
                *light = Vec3::new(0.1, 0.5, 0.1);
                let scale = (ctx.rand() % 32 + 80) as f32 * 0.025;
                ctx.create_particle(BITMAP_SMOKE, position, e.angle, *light, 11, scale, None);
            }
            if sub != 7 && sub != 8 {
                stones(e, ctx, *light, 6);
            }
            ctx.play_sound("SOUND_DEATH_POISON2");
        } else {
            stones(e, ctx, e.light, 6);
        }
        ctx.create_particle(BITMAP_EXPLOTION, position, e.angle, *light, 0, 1.0, None);
        e.live = false;
    }
}

/// Subtype 5: bounces along the ground (ZzzEffect.cpp:12495-12528).
fn bounce(e: &mut Effect, ctx: &mut Ctx, light: Vec3) {
    e.position.z += e.gravity;
    e.gravity -= 2.0;
    let height = ctx.terrain_height(e.position.x, e.position.y);
    if e.position.z < height {
        e.gravity = 10.0;
        e.position.z += 10.0;
        e.direction.y *= 0.5;
        e.scale *= 1.1;
    }
    let wall = ctx.terrain_attribute(e.position.x, e.position.y) & TW_NOGROUND;
    if e.life_time as i32 == 1 || wall == TW_NOGROUND {
        e.position.z = height;
        e.direction = Vec3::ZERO;
        let position = e.position + Vec3::new(0.0, 0.0, 80.0);
        ctx.create_particle(BITMAP_EXPLOTION, position, e.angle, light, 0, 1.0, None);
        stones(e, ctx, e.light, 6);
        e.live = false;
    }
}

/// `CheckTargetRange(o)` (ZzzEffect.cpp:366-445), `MODEL_FIRE` part: when the owner is `Live &&
/// Visible` and within 100 (2D), the effect ends (`LifeTime = 1`) and subtype 1 leaves 2 stones.
/// The callers (`ZzzCharacter.cpp:1830`, 4866) pass a character as owner; an effect owner is not
/// looked up (the engine has no `Live`/`Visible` of other effects).
fn check_target_range(e: &mut Effect, ctx: &mut Ctx) {
    let Some((position, live, visible)) = ctx.actor(e.owner).map(|to| (to.position, to.live, to.visible)) else { return };
    if !(live && visible) {
        return;
    }
    let (dx, dy) = (e.position.x - position.x, e.position.y - position.y);
    let range = (dx * dx + dy * dy).sqrt();
    if range <= 100.0 {
        e.life_time = 1.0;
        if e.sub_type == 1 {
            stones(e, ctx, e.light, 2);
        }
    }
}

/// One `MoveEffect` of `MODEL_FIRE` (ZzzEffect.cpp:6923-6929 + 12364-12613) with the common tail
/// (`PlayAnimation` + `MoveParticle(o, true)`, 18060-18124).
fn fire_move(e: &mut Effect, ctx: &mut Ctx) {
    let lum = luminosity(e, ctx);
    // `Vector(1.f, 1.f, 1.f, Light)` at the start of `MoveEffect` (ZzzEffect.cpp:6929).
    let mut light = Vec3::ONE;
    let sub = e.sub_type;
    match sub {
        0 | 2 | 4 | 6 | 7 | 8 => fall(e, ctx, &mut light),
        5 => bounce(e, ctx, light),
        9 => {
            ctx.create_particle(BITMAP_SMOKE, e.position, e.angle, e.direction, 36, 1.0 + e.scale, me(e));
            ctx.create_particle(BITMAP_SMOKE, e.position, e.angle, e.direction, 37, 2.0 + e.scale, me(e));
            e.light = Vec3::new(1.0, 0.2, 0.2);
            ctx.add_terrain_light(e.position.x, e.position.y, e.light, 4.0);
            let height = ctx.terrain_height(e.position.x, e.position.y);
            if e.position.z < height {
                e.live = false;
                // `break;` out of the switch: the rest of the case is skipped, the tail runs.
                common::move_tail(e, ctx);
                return;
            }
        }
        _ => {}
    }

    // ZzzEffect.cpp:12544-12612 (also on the tick the rock lands).
    if sub == 5 {
        e.hidden_mesh = 0;
        e.blend_mesh_light = 0.0;
        light = Vec3::new(lum, lum * 0.1, 0.0);
        ctx.add_terrain_light(e.position.x, e.position.y, light, 2.0);
        if ctx.rand() % 5 != 0 {
            ctx.create_particle(BITMAP_POUNDING_BALL, e.position, e.angle, light, 0, 1.0, None);
        }
    } else {
        match sub {
            3 => {
                e.hidden_mesh = 0;
                e.blend_mesh_light = 0.0;
            }
            0 | 6 | 7 => e.blend_mesh_light = (ctx.rand() % 4 + 4) as f32 * 0.1,
            _ => e.blend_mesh_light = 0.0,
        }
        light = if matches!(sub, 6..=8) { Vec3::new(lum * 0.1, lum, 0.0) } else { Vec3::new(lum, lum * 0.1, 0.0) };
        ctx.add_terrain_light(e.position.x, e.position.y, light, 2.0);
        if sub == 6 || sub == 7 {
            for _ in 0..2 {
                let position = Vec3::new(e.position.x + (ctx.rand() % 50 - 25) as f32, e.position.y, e.position.z);
                ctx.create_particle(BITMAP_FIRE, position, e.angle, light, 5, 1.0, None);
            }
        } else if sub != 3 {
            ctx.create_particle(BITMAP_FIRE, e.position, e.angle, light, 5, 1.0, None);
        } else {
            // `GetMagicScrew(iIndex * 5371)`: the original uses the effect's slot in `Effects[]`;
            // the engine has no slots, so the effect's id modulo the pool size stands in for it.
            let slot = (e.id as usize % crate::fx::sim::MAX_EFFECTS) as i32;
            let pos = e.position + magic_screw(slot * 5371, 1.0, ctx.world_time) * 50.0;
            ctx.create_particle(BITMAP_FIRE, pos, Vec3::ZERO, Vec3::ONE, 8, e.scale * 2.5, None);
            let rotation = (ctx.rand() % 360) as f32;
            ctx.create_sprite(BITMAP_SHINY + 1, pos, e.scale * 3.0, Vec3::ONE, rotation, 0);
        }
        if sub == 1 {
            check_target_range(e, ctx);
        }
    }

    common::move_tail(e, ctx);
}

/// `MoveEffect`, plus the second move of subtype 3: it moves again in the same tick when the
/// `LifeTime` left after a move is odd (`if (0 != (o->LifeTime % 2)) MoveEffect(o, iIndex)`,
/// ZzzEffect.cpp:18140-18146; MuMain uses `rand_fps_check(2)` instead). Each move subtracts 1
/// life; the core does the last subtraction.
fn fire_step(e: &mut Effect, ctx: &mut Ctx) {
    fire_move(e, ctx);
    if e.sub_type != 3 {
        return;
    }
    loop {
        let left = e.life_time - 1.0;
        if left <= 0.0 || (left as i32) % 2 == 0 {
            break;
        }
        e.life_time = left;
        fire_move(e, ctx);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bevy::math::Vec3;
    use bevy::prelude::Entity;

    use crate::fx::core::*;
    use crate::fx::sim::{self, Actors, FxWorld};
    use crate::terrain::TerrainLights;

    fn registry() -> Registry {
        let mut r = Registry::default();
        crate::fx::kinds::register_all(&mut r);
        r
    }

    /// The hero standing at (12800, 12800, 0).
    fn actors() -> (Actors, Entity) {
        let e = Entity::PLACEHOLDER;
        let identity: mu_data::bmd::Mat34 = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]];
        let actor = Actor { position: Vec3::new(12800.0, 12800.0, 0.0), bones: Arc::new(vec![identity; 80]), ..Default::default() };
        let mut a = Actors::default();
        a.map.insert(e, actor);
        a.hero = Some(e);
        (a, e)
    }

    /// A world with one `MODEL_FIRE` of `sub` requested at `pos` (created during the first tick).
    fn world_with(sub: i32, pos: Vec3, owner: Option<Target>) -> FxWorld {
        FxWorld {
            next_effect_id: 1,
            pending: vec![Spawn::Effect(Effect::new(1, MODEL_FIRE, pos, Vec3::ZERO, Vec3::ONE, sub, owner, 0, 0, 0, 0.0))],
            ..Default::default()
        }
    }

    fn tick(world: &mut FxWorld, registry: &Registry, actors: &Actors, t: usize) {
        let mut lights = TerrainLights::default();
        sim::step(world, registry, actors, None, &mut lights, t as f32 * 40.0, 0);
    }

    fn fire(world: &FxWorld) -> Option<&Effect> {
        world.effects.iter().find(|e| e.id == 1)
    }

    #[test]
    fn every_subtype_runs_and_ends() {
        let registry = registry();
        let (actors, hero) = actors();
        for sub in 0..=10 {
            for owner in [None, Some(Target::Actor(hero))] {
                let mut world = world_with(sub, Vec3::new(13000.0, 13000.0, 0.0), owner);
                tick(&mut world, &registry, &actors, 0);
                assert!(fire(&world).is_some(), "sub {sub}: not created");
                let mut ended = false;
                for t in 1..700 {
                    tick(&mut world, &registry, &actors, t);
                    match fire(&world) {
                        Some(e) => assert!(e.position.is_finite(), "sub {sub}: {:?}", e.position),
                        None => {
                            ended = true;
                            break;
                        }
                    }
                }
                assert!(ended, "sub {sub}: never ended");
            }
        }
    }

    /// Subtype 0 (Meteorite): appears 400 above, falls 50·cos 20° per tick and, on reaching the
    /// ground, leaves 6 stones there and dies, long before its 40 ticks of life.
    #[test]
    fn meteor_falls_and_dies_at_the_ground() {
        let registry = registry();
        let (actors, _) = actors();
        let target = Vec3::new(13000.0, 13000.0, 0.0);
        let mut world = world_with(0, target, None);
        tick(&mut world, &registry, &actors, 0);
        let start = fire(&world).expect("created").position;
        assert_eq!(start.z, 400.0);
        assert!(start.x >= target.x + 130.0 && start.x < target.x + 162.0);
        let mut z = start.z;
        let mut ticks = 1;
        loop {
            let stones_before = world.effects.iter().filter(|e| e.ty == MODEL_STONE1 || e.ty == MODEL_STONE2).count();
            tick(&mut world, &registry, &actors, ticks);
            ticks += 1;
            match fire(&world) {
                Some(e) => {
                    assert!((z - e.position.z - 50.0 * 20f32.to_radians().cos()).abs() < 1e-3, "falls at a constant speed");
                    z = e.position.z;
                }
                None => {
                    let stones: Vec<&Effect> = world.effects.iter().filter(|e| e.ty == MODEL_STONE1 || e.ty == MODEL_STONE2).collect();
                    assert_eq!(stones.len() - stones_before, 6, "6 stones on landing");
                    for s in stones {
                        // Created at the landing point: on the ground, near where it was aimed.
                        assert_eq!(s.start_position.z, 0.0);
                        assert!((s.start_position.x - target.x).abs() < 60.0, "{:?}", s.start_position);
                    }
                    break;
                }
            }
            assert!(ticks < 20, "the meteor never landed");
        }
        assert!(z < 0.0, "it dies once below the ground (z {z})");
    }

    /// Subtype 1 ends with 2 stones as soon as it is within 100 of its owner (`CheckTargetRange`).
    #[test]
    fn subtype_1_bursts_at_its_owner() {
        let registry = registry();
        let (actors, hero) = actors();
        let mut world = world_with(1, Vec3::new(12850.0, 12800.0, 0.0), Some(Target::Actor(hero)));
        tick(&mut world, &registry, &actors, 0);
        assert!(fire(&world).is_some());
        tick(&mut world, &registry, &actors, 1);
        assert!(fire(&world).is_none());
        assert_eq!(world.effects.iter().filter(|e| e.ty == MODEL_STONE1 || e.ty == MODEL_STONE2).count(), 2);
    }

    /// Subtype 3 moves twice per tick (12 towards -Y each) and lives 40 ticks.
    #[test]
    fn subtype_3_moves_twice_per_tick() {
        let registry = registry();
        let (actors, _) = actors();
        let mut world = world_with(3, Vec3::new(13000.0, 13000.0, 300.0), None);
        tick(&mut world, &registry, &actors, 0);
        let y0 = fire(&world).expect("created").position.y;
        tick(&mut world, &registry, &actors, 1);
        let e = fire(&world).expect("alive");
        assert!((y0 - e.position.y - 24.0).abs() < 1e-3);
        assert_eq!(e.life_time, 78.0);
        let mut alive = 1;
        for t in 2..60 {
            tick(&mut world, &registry, &actors, t);
            if fire(&world).is_none() {
                break;
            }
            alive += 1;
        }
        assert_eq!(alive, 39);
    }
}
