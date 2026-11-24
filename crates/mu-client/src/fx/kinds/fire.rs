//! `MODEL_FIRE` (`Skill/Fire01.bmd`): the dragon invasion meteor (subtype 3).
//!
//! Ported from `ZzzEffect.cpp`: `CreateEffect` (≈2075-2175), `MoveEffect` (≈6898-7148) and the
//! second move of subtype 3 (≈7726). `GOBoid.cpp` (≈1493) creates it 1 in 40 ticks during the
//! invasion, 300 above the hero.
//!
//! Subtype 3 does not draw the rock: it hides mesh 0 (`HiddenMesh = 0`) and the fire mesh
//! (`BlendMesh = 1`) stays at `BlendMeshLight = 0`. What shows is a `BITMAP_FIRE` subtype 8 ball
//! with `BITMAP_SHINY + 1` flares spiralling (`GetMagicScrew`), and a red light on the ground.
//! It moves 12 towards -Y horizontally and dies after 80 moves.
//!
//! Only subtype 3 is ported; the rest (other classes' and monsters' skills) are still to do.

use bevy::math::Vec3;

use crate::fx::core::*;
use crate::fx::kinds::joints_1::magic_screw;

pub fn register(r: &mut Registry) {
    r.effect(MODEL_FIRE, EffectKind { create: Some(fire_create), step: Some(fire_step), draw: None });
}

fn fire_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.blend_mesh = 1;
    match e.sub_type {
        3 => {
            e.life_time = 80.0;
            e.scale = 0.3;
            e.direction = Vec3::new(0.0, -12.0, 0.0);
        }
        // TODO(original): subtypes 0-2 and 4-9.
        _ => e.life_time = 0.0,
    }
}

/// Luminosity from the start of `MoveEffect`.
fn luminosity(e: &Effect, ctx: &mut Ctx) -> f32 {
    let mut l = (ctx.rand() % 4 + 7) as f32 * 0.1;
    if e.life_time < 5.0 {
        l = (l - (5.0 - e.life_time) * 0.2).max(0.0);
    }
    l
}

/// One subtype 3 `MoveEffect` + the common tail (`MoveParticle(o, true)`).
fn fire_move(e: &mut Effect, ctx: &mut Ctx) {
    let lum = luminosity(e, ctx);
    e.hidden_mesh = 0;
    e.blend_mesh_light = 0.0;
    // Meshes hidden or black: nothing of the model to draw.
    e.visible = false;
    ctx.add_terrain_light(e.position.x, e.position.y, Vec3::new(lum, lum * 0.1, 0.0), 2.0);

    // `GetMagicScrew(iIndex * 5371)`: the original uses the effect's slot; here its id.
    let slot = (e.id % 300) as i32;
    let pos = e.position + magic_screw(slot * 5371, 1.0, ctx.world_time) * 50.0;
    ctx.create_particle(BITMAP_FIRE, pos, Vec3::ZERO, Vec3::ONE, 8, e.scale * 2.5, None);
    let rotation = (ctx.rand() % 360) as f32;
    ctx.create_sprite(BITMAP_SHINY + 1, pos, e.scale * 3.0, Vec3::ONE, rotation, 0);

    e.animation_frame += e.velocity;
    e.position += rotate(e.angle, e.direction);
}

/// Subtype 3, while alive, moves again in the same tick 1 in 2 times
/// (each move subtracts 1 life; the core does the last subtraction).
fn fire_step(e: &mut Effect, ctx: &mut Ctx) {
    if e.sub_type != 3 {
        return;
    }
    loop {
        fire_move(e, ctx);
        if e.life_time - 1.0 <= 0.0 || !ctx.rand_fps_check(2) {
            break;
        }
        e.life_time -= 1.0;
    }
}
