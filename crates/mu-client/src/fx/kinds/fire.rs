//! `MODEL_FIRE` (`Skill/Fire01.bmd`): el meteorito de la invasión de dragones (subtipo 3).
//!
//! Traducido de `ZzzEffect.cpp`: `CreateEffect` (≈2075-2175), `MoveEffect` (≈6898-7148) y el
//! segundo movimiento del subtipo 3 (≈7726). `GOBoid.cpp` (≈1493) lo crea 1 de cada 40 ticks
//! durante la invasión a 300 por encima del héroe.
//!
//! El subtipo 3 no dibuja la roca: oculta la malla 0 (`HiddenMesh = 0`) y la malla de fuego
//! (`BlendMesh = 1`) queda con `BlendMeshLight = 0`. Lo que se ve es una bola de `BITMAP_FIRE`
//! subtipo 8 con destellos `BITMAP_SHINY + 1` que gira en espiral (`GetMagicScrew`), y una luz
//! roja en el suelo. Avanza 12 hacia -Y en horizontal y se apaga a los 80 movimientos.
//!
//! Solo está el subtipo 3; los demás (habilidades de otras clases y monstruos) quedan por traducir.

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
        // TODO(original): subtipos 0-2 y 4-9.
        _ => e.life_time = 0.0,
    }
}

/// Luminosity del principio de `MoveEffect`.
fn luminosity(e: &Effect, ctx: &mut Ctx) -> f32 {
    let mut l = (ctx.rand() % 4 + 7) as f32 * 0.1;
    if e.life_time < 5.0 {
        l = (l - (5.0 - e.life_time) * 0.2).max(0.0);
    }
    l
}

/// Un `MoveEffect` del subtipo 3 + la cola común (`MoveParticle(o, true)`).
fn fire_move(e: &mut Effect, ctx: &mut Ctx) {
    let lum = luminosity(e, ctx);
    e.hidden_mesh = 0;
    e.blend_mesh_light = 0.0;
    // Mallas ocultas o negras: no hay nada que dibujar del modelo.
    e.visible = false;
    ctx.add_terrain_light(e.position.x, e.position.y, Vec3::new(lum, lum * 0.1, 0.0), 2.0);

    // `GetMagicScrew(iIndex * 5371)`: el original usa el hueco del efecto; aquí su número.
    let slot = (e.id % 300) as i32;
    let pos = e.position + magic_screw(slot * 5371, 1.0, ctx.world_time) * 50.0;
    ctx.create_particle(BITMAP_FIRE, pos, Vec3::ZERO, Vec3::ONE, 8, e.scale * 2.5, None);
    let rotation = (ctx.rand() % 360) as f32;
    ctx.create_sprite(BITMAP_SHINY + 1, pos, e.scale * 3.0, Vec3::ONE, rotation, 0);

    e.animation_frame += e.velocity;
    e.position += rotate(e.angle, e.direction);
}

/// El subtipo 3, si sigue vivo, se vuelve a mover en el mismo tick 1 de cada 2 veces
/// (cada movimiento resta 1 de vida; la última resta la hace el núcleo).
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
