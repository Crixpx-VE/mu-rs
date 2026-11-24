//! Particles ported from `ZzzEffectParticle.cpp` (`CreateParticle`, `MoveParticles`,
//! `DescribeParticle`): smokes (`BITMAP_SMOKE+1..+4`, `BITMAP_SMOKELINE1..3`), lightning
//! (`BITMAP_LIGHTNING_MEGA1..3`), fires (`BITMAP_FIRE_HIK1/3` and their `_MONO`), `BITMAP_LIGHT+1`
//! and sparks (`BITMAP_SPARK`, `BITMAP_SPARK+1`).

use bevy::math::Vec3;

use crate::fx::core::*;

/// `WD_63PK_FIELD` (`MapManager.h`).
const WD_63PK_FIELD: i32 = 63;

/// `Bitmaps[BITMAP_SMOKE+1].Height` and `Bitmaps[BITMAP_SMOKE+4].Height` (smoke02/smoke05.tga: 64×64).
/// TODO(core): the simulation `Ctx` does not expose texture sizes.
const SMOKE_TEX_HEIGHT: f32 = 64.0;

const PI: f32 = std::f32::consts::PI;

pub fn register(r: &mut Registry) {
    let smoke1 = ParticleKind { create: Some(create_smoke1), step: Some(step_smoke1), draw: Some(draw_smoke1) };
    r.particle(BITMAP_SMOKE + 1, smoke1);
    r.particle(BITMAP_SMOKE + 4, smoke1);
    r.particle(BITMAP_SMOKE + 2, ParticleKind { create: Some(create_smoke2), step: Some(step_smoke2), draw: None });
    r.particle(BITMAP_SMOKE + 3, ParticleKind { create: Some(create_smoke3), step: Some(step_smoke3), draw: Some(draw_smoke3) });

    let smokeline = ParticleKind { create: Some(create_smokeline), step: Some(step_smokeline), draw: None };
    r.particle(BITMAP_SMOKELINE1, smokeline);
    r.particle(BITMAP_SMOKELINE2, ParticleKind { draw: Some(draw_smokeline2), ..smokeline });
    r.particle(BITMAP_SMOKELINE3, smokeline);

    let mega = ParticleKind { create: Some(create_lightning_mega), step: Some(step_lightning_mega), draw: None };
    r.particle(BITMAP_LIGHTNING_MEGA1, mega);
    r.particle(BITMAP_LIGHTNING_MEGA2, mega);
    r.particle(BITMAP_LIGHTNING_MEGA3, mega);

    let hik1 = ParticleKind { create: Some(create_fire_hik1), step: Some(step_fire_hik1), draw: None };
    r.particle(BITMAP_FIRE_HIK1, hik1);
    r.particle(BITMAP_FIRE_HIK1_MONO, hik1);
    let hik3 = ParticleKind { create: Some(create_fire_hik3), step: Some(step_fire_hik3), draw: None };
    r.particle(BITMAP_FIRE_HIK3, hik3);
    r.particle(BITMAP_FIRE_HIK3_MONO, hik3);

    r.particle(BITMAP_LIGHT + 1, ParticleKind { create: Some(create_light1), step: Some(step_light1), draw: None });
    r.particle(BITMAP_SPARK, ParticleKind { create: Some(create_spark), step: Some(step_spark), draw: Some(draw_spark) });
    r.particle(BITMAP_SPARK + 1, ParticleKind { create: Some(create_spark1), step: Some(step_spark1), draw: None });
}

// --- Helpers ------------------------------------------------------------------------------------

/// `(float)(rand() % m)`.
fn rf(ctx: &mut Ctx, m: i32) -> f32 {
    (ctx.rand() % m) as f32
}

/// `o->Target->Live`: the character is still in the world and alive (or the effect still exists).
fn target_live(ctx: &Ctx, t: Option<Target>) -> bool {
    match t {
        Some(Target::Actor(_)) => ctx.actor(t).is_some_and(|a| a.live),
        Some(Target::Effect(id)) => ctx.effect_positions.contains_key(&id),
        None => false,
    }
}

/// `Position -= StartPosition; StartPosition = Target->Position; Position += StartPosition`:
/// the particle is dragged along with what it follows.
fn follow_target(p: &mut Particle, ctx: &Ctx) {
    if let Some(t) = ctx.target_position(p.target) {
        p.position = p.position - p.start_position + t;
        p.start_position = t;
    }
}

/// Fades in and out with `Alpha` (common smoke/fire pattern) and `Light = TurningForce·Alpha`.
fn fade(p: &mut Particle, ctx: &mut Ctx, below: f32, dec: f32, max: f32, rmod: i32, radd: i32) {
    if p.life_time < below {
        p.alpha -= dec;
    } else if p.alpha < max {
        p.alpha += (ctx.rand() % rmod + radd) as f32 * 0.1;
    } else {
        p.alpha = max;
    }
    if p.alpha < 0.1 {
        p.live = false;
    }
    p.light = p.turning_force * p.alpha;
}

/// `if (Scale > 0) Scale -= (rand()%rmod+radd)*0.01f; else Live = false;`
fn shrink(p: &mut Particle, ctx: &mut Ctx, rmod: i32, radd: i32) {
    if p.scale > 0.0 {
        p.scale -= (ctx.rand() % rmod + radd) as f32 * 0.01;
    } else {
        p.live = false;
    }
}

/// `DescribeParticle` sprite (`Sprite(TexType, Position, W, H, Light, Rotation)`) with the
/// `RenderParticles` blend (JPG additive, TGA alpha test) or the one the case sets.
fn draw_with(p: &Particle, ctx: &DrawCtx, out: &mut DrawList, blend: Option<Blend>) {
    let Some(t) = ctx.tex(p.tex_type) else { return };
    out.sprites.push(SpriteDraw {
        texture: p.tex_type,
        position: p.position,
        width: t.width * p.scale,
        height: t.height * p.scale,
        color: [p.light.x, p.light.y, p.light.z, if t.components == 3 { 1.0 } else { p.light.x }],
        rotation: p.rotation,
        uv: [0.0, 0.0, 1.0, 1.0],
        blend: blend.unwrap_or_else(|| default_blend(t)),
    });
}

// --- BITMAP_SMOKE+1 / BITMAP_SMOKE+4 --------------------------------------------------------------

fn create_smoke1(p: &mut Particle, ctx: &mut Ctx) {
    let scale = p.scale;
    let angle = p.angle;
    p.life_time = 32.0;
    p.position.x += (ctx.rand() % 16 - 8) as f32;
    p.position.y += (ctx.rand() % 16 - 8) as f32;
    // (+4 and +1 compute the same; only the order of operations differs)
    p.scale = (ctx.rand() % 32 + 32) as f32 * 0.01 * scale;

    p.velocity = if p.sub_type == 0 {
        rotate(angle, Vec3::new(0.0, 3.0, 0.0))
    } else {
        p.angle.z = rf(ctx, 360);
        rotate(p.angle, Vec3::new(0.0, 15.0, 0.0))
    };
    p.angle.x = rf(ctx, 360);

    if p.sub_type == 1 || p.sub_type == 6 {
        p.life_time = 40.0;
        p.velocity = Vec3::ZERO;
        p.position.z += (ctx.rand() % 16 - 8) as f32;
    }
}

fn step_smoke1(p: &mut Particle, ctx: &mut Ctx) {
    let lum = p.life_time / 32.0;
    p.light = Vec3::splat(lum);
    p.scale += 0.08;
    p.position += p.velocity;
    p.velocity *= 0.9;

    if p.sub_type == 6 {
        p.scale -= 0.05;
        p.position.z += 4.0;
    } else if p.sub_type != 5 {
        p.position.z = ctx.terrain_height(p.position.x, p.position.y) + SMOKE_TEX_HEIGHT * p.scale * 0.5;
    }
}

fn draw_smoke1(p: &Particle, ctx: &DrawCtx, out: &mut DrawList) {
    draw_with(p, ctx, out, Some(Blend::Blend3));
}

// --- BITMAP_SMOKE+2 -------------------------------------------------------------------------------

fn create_smoke2(p: &mut Particle, ctx: &mut Ctx) {
    p.life_time = 50.0;
    p.scale = (ctx.rand() % 32 + 64) as f32 * 0.01;
    p.angle = Vec3::new(0.0, 0.0, rf(ctx, 360));
    let y = rf(ctx, 64);
    let z = (ctx.rand() % 16 + 16) as f32;
    // StartPosition = offset from the target.
    p.start_position = rotate(p.angle, Vec3::new(0.0, y, z));
    p.angle = Vec3::ZERO;
}

fn step_smoke2(p: &mut Particle, ctx: &mut Ctx) {
    let mut lum = 1.0;
    if p.life_time < 10.0 {
        lum -= (10.0 - p.life_time) * 0.1;
    }
    p.light = Vec3::splat(lum);
    let offset = rotate(p.angle, p.start_position);
    if let Some(t) = ctx.target_position(p.target) {
        p.position = t + offset;
    }
    p.angle.y += 5.0;
    p.scale -= 0.01;
}

// --- BITMAP_SMOKE+3 -------------------------------------------------------------------------------

fn create_smoke3(p: &mut Particle, ctx: &mut Ctx) {
    match p.sub_type {
        0 | 2 => {
            p.turning_force = p.light;
            p.life_time = 55.0;
            p.angle.x = -2.0 + rf(ctx, 4);
            p.rotation = (ctx.world_time as i32 % 360) as f32;
        }
        1 => {
            p.turning_force = p.light;
            p.life_time = 30.0;
            p.angle.x = -2.0 + rf(ctx, 4);
            p.rotation = (ctx.world_time as i32 % 360) as f32;
        }
        3 | 4 => {
            if p.sub_type == 3 {
                p.tex_type = BITMAP_CLUD64;
                p.life_time = 60.0;
            } else {
                p.tex_type = BITMAP_WATERFALL_3;
                p.life_time = 30.0;
            }
            p.scale *= (ctx.rand() % 64 + 64) as f32 * 0.02;
            p.rotation = rf(ctx, 360);
            p.position.x += (ctx.rand() % 250 - 125) as f32;
            p.position.y += (ctx.rand() % 250 - 125) as f32;
            p.position.z -= rf(ctx, 45);
            p.gravity = (ctx.rand() % 100 - 50) as f32 * 0.1;
            p.light = Vec3::ONE;
        }
        _ => {}
    }
}

fn step_smoke3(p: &mut Particle, _ctx: &mut Ctx) {
    match p.sub_type {
        0 => {
            if p.turning_force.x > 0.1 {
                p.turning_force.x -= 0.015;
            }
            // (sic) the original subtracts from [0] even though it checks [1].
            if p.turning_force.y > 0.1 {
                p.turning_force.x -= 0.005;
            }
            let lum = p.life_time / 55.0;
            p.light = p.turning_force * lum;
            p.gravity += 0.2;
            p.position.z += p.gravity;
            p.scale += 0.03;
            p.rotation += p.angle.x;
        }
        1 => {
            let lum = p.life_time / 55.0;
            p.light = p.turning_force * lum;
            p.gravity += 0.1;
            p.position.z += p.gravity;
            p.scale += 0.03;
            p.rotation += p.angle.x;
        }
        2 => {
            if p.turning_force.y > 0.1 {
                p.turning_force.y -= 0.005;
            }
            if p.turning_force.z > 0.1 {
                p.turning_force.z -= 0.015;
            }
            let lum = p.life_time / 55.0;
            p.light = p.turning_force * lum;
            p.gravity += 0.2;
            p.position.z += p.gravity;
            p.scale += 0.03;
            p.rotation += p.angle.x;
        }
        3 | 4 => {
            if p.light.x <= 0.05 {
                p.live = false;
            }
            p.light *= 1.0 / 1.012;
            p.scale += 0.01;
            p.rotation += p.gravity / 2.0;
            if p.gravity <= 0.0 {
                p.gravity = -p.gravity;
            }
            p.position.z += p.gravity;
        }
        _ => {}
    }
}

fn draw_smoke3(p: &Particle, ctx: &DrawCtx, out: &mut DrawList) {
    let blend = if p.sub_type == 3 || p.sub_type == 4 { Blend::Subtract } else { Blend::Blend3 };
    draw_with(p, ctx, out, Some(blend));
}

// --- BITMAP_SMOKELINE1..3 -------------------------------------------------------------------------

fn create_smokeline(p: &mut Particle, ctx: &mut Ctx) {
    let scale = p.scale;
    let light = p.light;
    match p.sub_type {
        0 | 5 => {
            p.life_time = if ctx.world_active == WD_63PK_FIELD { 25.0 } else { 35.0 };
            p.scale = (ctx.rand() % 96 + 96) as f32 * 0.01 * scale;
            p.rotation = rf(ctx, 360);
            p.gravity = (ctx.rand() % 16 + 12) as f32 * 0.1;
            p.alpha = 0.0;
            p.turning_force = light;
            p.light = Vec3::ZERO;
            if p.sub_type == 5 {
                if let Some(t) = ctx.target_position(p.target) {
                    p.start_position = t;
                }
            }
        }
        1 => {
            p.life_time = 25.0;
            p.scale = (ctx.rand() % 50 + 55) as f32 * 0.01 * scale;
            p.rotation = rf(ctx, 360);
            p.gravity = (ctx.rand() % 4 + 35) as f32 * 0.1;
            p.alpha = 0.0;
            p.turning_force = light;
            p.light = Vec3::ZERO;
            if let Some(t) = ctx.target_position(p.target) {
                p.start_position = t;
            }
        }
        2 | 3 => {
            p.life_time = 45.0;
            p.scale = (ctx.rand() % 96 + 96) as f32 * 0.01 * scale;
            p.rotation = rf(ctx, 360);
            p.gravity = (ctx.rand() % 16 + 12) as f32 * 0.1;
            p.alpha = 0.0;
            p.turning_force = light;
            p.light = Vec3::ZERO;
        }
        4 => {
            p.position.x += (ctx.rand() % 20 - 10) as f32 * 2.0;
            p.position.y += (ctx.rand() % 20 - 10) as f32 * 2.0;
            p.position.z += (ctx.rand() % 20 - 10) as f32 * 2.0;
            p.scale += (ctx.rand() % 20 - 10) as f32 * (p.scale * 0.03);
            p.life_time = 45.0;
            p.alpha = 1.0;
            p.gravity = 0.2;
            p.rotation = rf(ctx, 360);
        }
        _ => {}
    }
}

fn step_smokeline(p: &mut Particle, ctx: &mut Ctx) {
    match p.sub_type {
        0 | 5 => {
            fade(p, ctx, 10.0, 0.1, 0.7, 5, 2);
            p.scale += (ctx.rand() % 3 + 6) as f32 * 0.01;
            p.position.z += p.gravity;
            if p.sub_type == 5 {
                follow_target(p, ctx);
            }
        }
        1 => {
            fade(p, ctx, 10.0, 0.2, 1.0, 5, 2);
            if p.scale > 0.0 {
                p.scale -= (ctx.rand() % 10 + 10) as f32 * 0.001;
            } else {
                p.live = false;
            }
            p.position.z += p.gravity;
            follow_target(p, ctx);
        }
        2 | 3 => {
            fade(p, ctx, 10.0, 0.1, 0.7, 5, 2);
            p.scale += (ctx.rand() % 3 + 6) as f32 * 0.01;
            p.position.z += p.gravity;
        }
        4 => {
            p.position.z += p.gravity;
            p.rotation += 0.8;
            p.scale += 0.01;
            p.alpha -= 0.01;
            if p.alpha < 0.1 {
                p.live = false;
            }
            p.light *= p.alpha;
        }
        _ => {}
    }
}

fn draw_smokeline2(p: &Particle, ctx: &DrawCtx, out: &mut DrawList) {
    draw_with(p, ctx, out, (p.sub_type == 3).then_some(Blend::Subtract));
}

// --- BITMAP_LIGHTNING_MEGA1..3 --------------------------------------------------------------------

fn create_lightning_mega(p: &mut Particle, ctx: &mut Ctx) {
    if p.sub_type == 0 {
        p.life_time = 5.0;
        p.rotation = rf(ctx, 360);
        p.gravity = 0.0;
        p.alpha = 1.0;
        p.turning_force = p.light;
        p.light = Vec3::ZERO;
    }
}

fn step_lightning_mega(p: &mut Particle, _ctx: &mut Ctx) {
    if p.sub_type == 0 {
        p.alpha -= 0.15;
        if p.alpha < 0.1 {
            p.live = false;
        }
        p.light = p.turning_force * p.alpha;
    }
}

// --- BITMAP_FIRE_HIK1 / BITMAP_FIRE_HIK1_MONO -----------------------------------------------------

/// Common fire part: random rotation, starts invisible and stores the color in `TurningForce`.
fn fire_common(p: &mut Particle, light: Vec3) {
    p.alpha = 0.0;
    p.turning_force = light;
    p.light = Vec3::ZERO;
}

fn create_fire_hik1(p: &mut Particle, ctx: &mut Ctx) {
    let scale = p.scale;
    let light = p.light;
    match p.sub_type {
        0 | 6 | 5 => {
            p.life_time = (ctx.rand() % 5 + 27) as f32;
            p.scale = (ctx.rand() % 72 + 72) as f32 * 0.01 * scale;
            p.rotation = rf(ctx, 360);
            p.gravity = (ctx.rand() % 24 + 64) as f32 * 0.1;
            fire_common(p, light);
        }
        1 => {
            p.life_time = (ctx.rand() % 5 + 54) as f32;
            p.scale = (ctx.rand() % 72 + 72) as f32 * 0.01 * scale;
            p.rotation = rf(ctx, 360);
            p.gravity = (ctx.rand() % 24 + 74) as f32 * 0.1;
            fire_common(p, light);
        }
        2 => {
            p.life_time = (ctx.rand() % 5 + 27) as f32;
            p.scale = (ctx.rand() % 72 + 72) as f32 * 0.01 * scale;
            p.rotation = rf(ctx, 360);
            p.gravity = (ctx.rand() % 24 + 100) as f32 * 0.1;
            let f = rf(ctx, 50) * 0.03;
            p.velocity = Vec3::new(2.5 + f, -7.0 - f, 0.0);
            fire_common(p, light);
        }
        3 => {
            p.life_time = (ctx.rand() % 5 + 27) as f32;
            p.scale = (ctx.rand() % 72 + 52) as f32 * 0.01 * scale;
            p.rotation = rf(ctx, 360);
            p.gravity = (ctx.rand() % 14 + 44) as f32 * 0.1;
            fire_common(p, light);
        }
        4 => {
            p.life_time = 8.0;
            p.scale = (ctx.rand() % 42 + 12) as f32 * 0.01 * scale;
            p.rotation = rf(ctx, 360);
            p.gravity = (ctx.rand() % 14 + 44) as f32 * 0.1;
            fire_common(p, light);
            if let Some(t) = ctx.target_position(p.target) {
                p.start_position = t;
            }
        }
        10 => {
            p.life_time = (ctx.rand() % 5 + 47) as f32;
            p.scale = (ctx.rand() % 72 + 72) as f32 * 0.01 * scale;
            p.rotation = rf(ctx, 360);
            p.gravity = (ctx.rand() % 24 + 64) as f32 * 0.1;
            fire_common(p, light);
        }
        _ => {}
    }
}

/// Moves 8 units per tick towards local −Y, dragged with the target (HIK1 sub 4, HIK3 sub 5).
fn fire_burst_step(p: &mut Particle, ctx: &mut Ctx, below: f32) {
    fade(p, ctx, below, 0.1, 1.0, 2, 2);
    p.scale += (ctx.rand() % 3 + 8) as f32 * 0.01;
    if p.life_time < 3.0 {
        p.scale += (10.0 - p.life_time) * 0.02;
    }
    p.position += rotate(p.angle, Vec3::new(0.0, -1.0, 0.0)) * 8.0;
    follow_target(p, ctx);
    p.rotation += 5.0;
}

fn step_fire_hik1(p: &mut Particle, ctx: &mut Ctx) {
    match p.sub_type {
        0 | 6 => {
            fade(p, ctx, 15.0, 0.2, 1.0, 2, 2);
            shrink(p, ctx, 3, 5);
            p.position.z += p.gravity;
            p.rotation += 3.0;
            if p.sub_type == 6 {
                p.rotation = 0.0;
                p.position.z += p.gravity * 1.2;
            }
        }
        1 => {
            fade(p, ctx, 20.0, 0.2, 1.0, 2, 2);
            shrink(p, ctx, 2, 4);
            p.position.z += p.gravity;
            p.rotation += 3.0;
        }
        2 => {
            fade(p, ctx, 10.0, 0.2, 1.0, 2, 2);
            shrink(p, ctx, 3, 7);
            p.position.x += p.velocity.x;
            p.position.y += p.velocity.y;
            p.position.z += p.gravity;
            p.rotation += 3.0;
        }
        10 | 5 => {
            fade(p, ctx, 15.0, 0.2, 1.0, 2, 2);
            shrink(p, ctx, 3, 5);
            p.position.z += p.gravity;
            p.rotation += 3.0;
        }
        3 => {
            fade(p, ctx, 15.0, 0.2, 1.0, 2, 2);
            shrink(p, ctx, 3, 2);
            p.position.z += p.gravity;
            p.rotation += 3.0;
        }
        4 => fire_burst_step(p, ctx, 5.0),
        _ => {}
    }
}

// --- BITMAP_FIRE_HIK3 / BITMAP_FIRE_HIK3_MONO -----------------------------------------------------

fn create_fire_hik3(p: &mut Particle, ctx: &mut Ctx) {
    let scale = p.scale;
    let light = p.light;
    match p.sub_type {
        0 | 6 => {
            p.life_time = (ctx.rand() % 5 + 17) as f32;
            p.scale = (ctx.rand() % 72 + 72) as f32 * 0.01 * scale;
            p.rotation = rf(ctx, 360);
            p.gravity = (ctx.rand() % 24 + 64) as f32 * 0.1;
            fire_common(p, light);
        }
        1 => {
            p.life_time = (ctx.rand() % 5 + 34) as f32;
            p.scale = (ctx.rand() % 72 + 72) as f32 * 0.01 * scale;
            p.rotation = rf(ctx, 360);
            p.gravity = (ctx.rand() % 24 + 74) as f32 * 0.1;
            fire_common(p, light);
        }
        2 => {
            p.life_time = (ctx.rand() % 5 + 17) as f32;
            p.scale = (ctx.rand() % 72 + 72) as f32 * 0.01 * scale;
            p.rotation = rf(ctx, 360);
            p.gravity = (ctx.rand() % 24 + 100) as f32 * 0.1;
            let f = rf(ctx, 50) * 0.03;
            p.velocity = Vec3::new(2.5 + f, -5.0 - f, 0.0);
            fire_common(p, light);
        }
        3 => {
            p.life_time = (ctx.rand() % 5 + 17) as f32;
            p.scale = (ctx.rand() % 72 + 52) as f32 * 0.01 * scale;
            p.rotation = rf(ctx, 360);
            p.gravity = (ctx.rand() % 14 + 44) as f32 * 0.1;
            fire_common(p, light);
        }
        4 => {
            p.life_time = (ctx.rand() % 5 + 34) as f32;
            p.scale = (ctx.rand() % 72 + 72) as f32 * 0.01 * scale;
            p.rotation = rf(ctx, 360);
            p.gravity = (ctx.rand() % 10 + 40) as f32 * 0.1;
            fire_common(p, light);
        }
        5 => {
            p.life_time = 8.0;
            p.scale = (ctx.rand() % 42 + 12) as f32 * 0.01 * scale;
            p.rotation = rf(ctx, 360);
            p.gravity = (ctx.rand() % 14 + 44) as f32 * 0.1;
            fire_common(p, light);
            if let Some(t) = ctx.target_position(p.target) {
                p.start_position = t;
            }
        }
        _ => {}
    }
}

fn step_fire_hik3(p: &mut Particle, ctx: &mut Ctx) {
    match p.sub_type {
        0 | 6 => {
            fade(p, ctx, 10.0, 0.2, 1.0, 2, 2);
            shrink(p, ctx, 3, 7);
            p.position.z += p.gravity;
            p.rotation += 3.0;
            if p.sub_type == 6 {
                p.rotation = 0.0;
                p.position.z += p.gravity * 1.2;
            }
        }
        1 => {
            fade(p, ctx, 15.0, 0.2, 1.0, 2, 2);
            shrink(p, ctx, 2, 6);
            p.position.z += p.gravity;
            p.rotation += 3.0;
        }
        2 => {
            fade(p, ctx, 10.0, 0.2, 1.0, 2, 2);
            shrink(p, ctx, 3, 7);
            p.position.x += p.velocity.x;
            p.position.y += p.velocity.y;
            p.position.z += p.gravity;
            p.rotation += 3.0;
        }
        3 => {
            fade(p, ctx, 10.0, 0.2, 1.0, 2, 2);
            shrink(p, ctx, 3, 7);
            p.position.z += p.gravity;
            p.rotation += 3.0;
        }
        4 => {
            fade(p, ctx, 15.0, 0.2, 1.0, 2, 2);
            shrink(p, ctx, 4, 5);
            p.position.z += p.gravity;
            p.rotation += 3.0;
        }
        5 => fire_burst_step(p, ctx, 4.0),
        _ => {}
    }
}

// --- BITMAP_LIGHT+1 -------------------------------------------------------------------------------

fn create_light1(p: &mut Particle, ctx: &mut Ctx) {
    p.life_time = (20 + ctx.rand() % 8) as f32;
    match p.sub_type {
        1 => {
            p.life_time = 5.0;
            p.angle.x = -2.0 + rf(ctx, 4);
        }
        2 => {
            p.life_time = 19.0;
            p.position.y += -30.0 + rf(ctx, 60);
        }
        3 => {
            p.life_time = 7.0;
            p.tex_type = BITMAP_SHINY + 1;
        }
        4 => {
            p.life_time = 4.0;
            p.rotation = -360.0 + rf(ctx, 180);
            p.tex_type = BITMAP_SHINY + 1;
        }
        5 => p.life_time = 3.0,
        _ => {}
    }
}

fn step_light1(p: &mut Particle, _ctx: &mut Ctx) {
    match p.sub_type {
        2 => p.scale *= 0.92,
        3 => {
            p.scale *= 1.3;
            p.light *= 0.9;
        }
        5 => {
            // Stays alive while it exists (the original renews it every tick).
            p.life_time = 5.0;
            p.light *= 0.7;
        }
        4 => {}
        _ => {
            p.position.z -= p.gravity;
            p.position.y -= 1.0;
            p.gravity += 1.0;
            p.scale *= 0.95;
        }
    }
}

// --- BITMAP_SPARK ---------------------------------------------------------------------------------

fn create_spark(p: &mut Particle, ctx: &mut Ctx) {
    let scale = p.scale;
    p.scale = (ctx.rand() % 4 + 4) as f32 * 0.1;
    match p.sub_type {
        0 | 2 | 3 | 4 | 6 | 11 | 13 => {
            p.life_time = (ctx.rand() % 16 + 24) as f32;
            p.angle.z = rf(ctx, 360);
            p.gravity = (ctx.rand() % 16 + 6) as f32;
            let mut v = Vec3::new(0.0, (ctx.rand() % 20 + 20) as f32 * 0.1, 0.0);
            match p.sub_type {
                2 => {
                    p.scale *= 2.0;
                    v *= 3.0;
                }
                4 => {
                    p.scale *= 3.0;
                    v *= 10.0;
                }
                6 => {
                    p.life_time = (ctx.rand() % 4 + 4) as f32;
                    p.scale = scale;
                    p.gravity = 0.0;
                    p.tex_type = BITMAP_SPARK + 1;
                    v = Vec3::new(0.0, (ctx.rand() % 20 + 20) as f32 * 0.2, 0.0);
                }
                11 => p.light = Vec3::new(1.0, 0.3, 0.3),
                13 => {
                    p.scale *= 1.6;
                    v *= 1.5;
                    p.tex_type = BITMAP_SPARK + 1;
                }
                _ => {}
            }
            p.velocity = rotate(p.angle, v);
        }
        7 => p.life_time = (ctx.rand() % 12 + 12) as f32,
        8 | 10 => {
            p.tex_type = if p.sub_type == 10 { BITMAP_CLUD64 } else { BITMAP_SPARK + 1 };
            p.life_time = (ctx.rand() % 16 + 24) as f32;
            p.angle.z = rf(ctx, 360);
            p.gravity = (ctx.rand() % 16 + 6) as f32;
            let v = Vec3::new(0.0, (ctx.rand() % 60 - 30) as f32 * 0.1, 0.0);
            p.scale *= 1.5;
            p.velocity = rotate(p.angle, v * 3.0);
        }
        9 => {
            p.scale = scale * 1.2 + (ctx.rand() % 4 + 4) as f32 * 0.1;
            p.life_time = (ctx.rand() % 16 + 50) as f32;
            p.angle.z = rf(ctx, 360);
            p.gravity = rf(ctx, 2);
            let v = Vec3::new(0.0, (ctx.rand() % 10 + 10) as f32 * 0.05, 0.0);
            p.velocity = rotate(p.angle, v);
        }
        12 => {
            p.scale = scale + (ctx.rand() % 4 + 4) as f32 * 0.1;
            p.life_time = (ctx.rand() % 16 + 24) as f32;
            p.angle.z = rf(ctx, 360);
            p.gravity = (ctx.rand() % 16 + 6) as f32;
            let v = Vec3::new(0.0, (ctx.rand() % 20 + 20) as f32 * 0.1, 0.0);
            p.velocity = rotate(p.angle, v);
        }
        _ => {
            if p.sub_type == 5 {
                if let Some(t) = ctx.target_position(p.target) {
                    p.start_position = t;
                }
                p.scale = scale;
                p.life_time = (ctx.rand() % 4 + 12) as f32;
                p.gravity = 0.0;
                p.tex_type = BITMAP_SPARK + 1;
            } else {
                p.life_time = (ctx.rand() % 8 + 8) as f32;
            }
            // (the original's `if SubType == 7` is unreachable here: 7 is handled above)
        }
    }
}

fn step_spark(p: &mut Particle, ctx: &mut Ctx) {
    let lum = p.life_time / 16.0;
    if p.life_time < 0.0 {
        p.live = false;
    }
    if p.sub_type == 11 {
        p.light = Vec3::new(lum, lum * 0.3, lum * 0.3);
    } else if p.sub_type != 8 {
        p.light = Vec3::splat(lum);
    }
    p.position.z += p.gravity;
    match p.sub_type {
        5 => p.gravity -= 0.3,
        6 => {
            p.gravity -= 0.3;
            p.light = Vec3::splat(0.9);
        }
        8 | 10 => p.light *= 1.0 / 1.02,
        _ => {}
    }
    p.gravity -= match p.sub_type {
        7 => 0.6,
        9 => 0.4,
        _ => 2.0,
    };

    let height = ctx.terrain_height(p.position.x, p.position.y);
    if p.position.z < height {
        p.position.z = height;
        p.gravity = -p.gravity * 0.6;
        p.life_time -= 4.0;
    }
    p.position += p.velocity;
    if p.sub_type == 5 || p.sub_type == 6 {
        if let Some(t) = ctx.target_position(p.target) {
            let d = p.start_position - t;
            p.start_position = t;
            p.position -= d;
        }
    }
}

fn draw_spark(p: &Particle, ctx: &DrawCtx, out: &mut DrawList) {
    draw_with(p, ctx, out, (p.sub_type == 10).then_some(Blend::Subtract));
}

// --- BITMAP_SPARK+1 -------------------------------------------------------------------------------

/// Random `vSpeed` in [-8, 8)³, normalized and scaled (subtypes 21, 22, 27).
fn random_speed(ctx: &mut Ctx, len: f32) -> Vec3 {
    let x = -8.0 + rf(ctx, 16);
    let y = -8.0 + rf(ctx, 16);
    let z = -8.0 + rf(ctx, 16);
    Vec3::new(x, y, z).normalize_or_zero() * len
}

/// `Vector((rand()%40-20)*0.1f, (rand()%40-20)*0.1f, 0) * 2.5` rotated by `Angle`.
fn random_planar_velocity(p: &mut Particle, ctx: &mut Ctx) {
    let x = (ctx.rand() % 40 - 20) as f32 * 0.1;
    let y = (ctx.rand() % 40 - 20) as f32 * 0.1;
    p.velocity = rotate(p.angle, Vec3::new(x, y, 0.0) * 2.5);
}

fn create_spark1(p: &mut Particle, ctx: &mut Ctx) {
    let scale = p.scale;
    let angle = p.angle;
    match p.sub_type {
        1 => {
            let (x, y, z) = (rf(ctx, 360), rf(ctx, 360), rf(ctx, 360));
            p.angle = Vec3::new(x, y, z);
            p.velocity = rotate(p.angle, Vec3::new(0.0, -50.0, 0.0));
        }
        2 | 4 | 26 => {
            let (life, div) = match p.sub_type {
                2 => (15.0, 10.0),
                4 => (10.0, 6.0),
                _ => (20.0, 20.0),
            };
            p.life_time = life;
            if p.sub_type == 4 {
                p.alpha = 0.1;
            }
            p.angle = Vec3::ZERO;
            p.start_position = p.position;
            if let Some(t) = ctx.target_position(p.target) {
                p.velocity = (t - p.position) / div;
            }
        }
        3 => p.life_time = 5.0,
        5 => {
            p.life_time = 10.0;
            p.angle = Vec3::new(0.0, 0.0, angle.z);
            let v = Vec3::new(-((ctx.rand() % 2) as f32 + 2.0), 0.0, 0.0);
            p.velocity = rotate(p.angle, v);
        }
        6 => {
            if ctx.rand() % 50 == 0 {
                p.life_time = 50.0;
                p.gravity = 1.0 + rf(ctx, 20) / 5.0;
                p.scale = rf(ctx, 5) / 10.0 + 1.0;
            } else {
                p.life_time = 0.0;
            }
        }
        7 => {
            p.alpha = 1.0;
            p.position.x += -30.0 + rf(ctx, 60);
            p.position.y += -30.0 + rf(ctx, 60);
            p.position.z += -30.0 + rf(ctx, 60);
            p.life_time = 30.0;
            p.gravity = 1.3;
            p.scale = rf(ctx, 5) / 10.0 + 1.0;
        }
        8 => p.life_time = 10.0,
        9 => {
            p.life_time = 30.0;
            p.angle = Vec3::new(0.0, rf(ctx, 360), 0.0);
            let v = rotate(p.angle, Vec3::new(0.0, 0.0, rf(ctx, 10) + 160.0));
            p.position += v;
            p.velocity = Vec3::new(12.0, 0.0, -2.0);
        }
        10 => {
            p.alpha = 1.0;
            p.velocity.x = -4.0 + rf(ctx, 8);
            p.velocity.y = 0.0;
            p.velocity.z = -4.0 + rf(ctx, 8);
            p.life_time = 20.0;
            p.scale = rf(ctx, 5) / 10.0 + 1.0;
        }
        11 | 13 => {
            p.alpha = 1.0;
            p.velocity.x = -4.0 + rf(ctx, 8);
            p.velocity.y = rf(ctx, 4);
            p.velocity.z = rf(ctx, 4);
            p.life_time = 30.0;
            p.scale = rf(ctx, 5) / 10.0 + 1.0;
        }
        12 => {
            p.tex_type = BITMAP_SHINY;
            p.alpha = 1.0;
            p.position += rotate(p.angle, Vec3::new(0.0, -100.0, 0.0));
            p.position.x += -8.0 + rf(ctx, 16);
            p.position.y += -8.0 + rf(ctx, 16);
            p.position.z += 150.0;
            p.velocity.x = -1.0 + rf(ctx, 20) / 10.0;
            p.velocity.y = -1.0 + rf(ctx, 20) / 10.0;
            p.velocity.z = -rf(ctx, 3) - 3.0;
            p.life_time = 40.0;
            p.scale = -rf(ctx, 15) / 10.0 + scale;
        }
        14 => {
            if ctx.rand() % 2 == 0 {
                p.alpha = 1.0;
                p.velocity.x = -4.0 + rf(ctx, 4);
                p.velocity.y = rf(ctx, 4);
                p.velocity.z = rf(ctx, 4);
                p.life_time = 30.0;
                p.scale = rf(ctx, 5) / 10.0 + 0.4;
            }
        }
        15 => {
            if ctx.rand() % 3 == 0 {
                p.alpha = 1.0;
                p.life_time = 35.0;
                p.velocity.z = -(rf(ctx, 5) + 5.0) * 0.5;
            }
        }
        16 | 18 => {
            p.alpha = 1.0;
            p.life_time = if p.sub_type == 18 { 80.0 } else { 100.0 };
            p.frame = 77;
            p.scale = -rf(ctx, 3) / 3.0 + scale + 0.01;
            p.velocity.x = rf(ctx, 50) / 10.0 - 2.5;
            p.velocity.y = rf(ctx, 4) / 10.0 - 0.2;
            if p.sub_type == 18 {
                p.velocity.y = rf(ctx, 50) / 10.0 - 2.5;
            }
            p.velocity.z = rf(ctx, 50) / 10.0 - 2.5;
            let p1 = Vec3::new(p.velocity.x * 10.0, p.velocity.y * 10.0, p.velocity.z * 10.0 + 100.0);
            p.position += rotate(p.angle, p1);
            p.gravity = 3.5;
        }
        17 => {
            p.life_time = 200.0;
            p.scale += rf(ctx, 3) * 0.1;
            p.gravity = rf(ctx, 5) * 0.3 + 2.0;
            // Rotation is used as the sway frequency (not drawn rotated: it is nearly 0).
            p.rotation = rf(ctx, 20) * 0.0001 + 0.002;
            p.alpha = rf(ctx, 10) * 0.1 + 0.5;
            p.start_position = p.position;
        }
        19 => {
            p.life_time = 50.0;
            p.gravity = 1.0 + rf(ctx, 20) * 0.1;
            p.position.z = ctx.terrain_height(p.position.x, p.position.y) - 5.0;
            p.scale = rf(ctx, 10) * 0.08 + 0.8;
            // (VectorCopy(Light, o->Light): already set)
        }
        20 => {
            p.scale = scale + rf(ctx, 10) * 0.02;
            p.life_time = (ctx.rand() % 10 + 60) as f32;
            p.angle.z = rf(ctx, 360);
            p.gravity = (ctx.rand() % 10 + 15) as f32;
            random_planar_velocity(p, ctx);
        }
        21 => {
            p.velocity = random_speed(ctx, 12.0);
            p.life_time = (ctx.rand() % 10 + 20) as f32;
            p.scale = rf(ctx, 20) / 20.0 + 1.0;
        }
        22 => {
            // (sic) the random velocity is later overwritten by the rotation one.
            p.velocity = random_speed(ctx, 14.0);
            p.scale = rf(ctx, 20) / 20.0 + 1.0;
            p.life_time = (ctx.rand() % 10 + 20) as f32;
            p.angle.z = rf(ctx, 360);
            p.gravity = 20.0;
            random_planar_velocity(p, ctx);
        }
        23 | 24 => {
            let (k, life, base) = if p.sub_type == 23 { (1.5, 20.0, 0.8) } else { (2.0, 16.0, 0.4) };
            p.alpha = 1.0;
            p.velocity.x = (-1 + ctx.rand() % 3) as f32 * k;
            p.velocity.y = (-1 + ctx.rand() % 3) as f32 * k;
            p.velocity.z = (-1 + ctx.rand() % 3) as f32 * k;
            p.life_time = life;
            p.rotation = 0.0;
            p.scale = rf(ctx, 5) / 10.0 + base;
        }
        25 => {
            p.alpha = 1.0;
            p.velocity.x = -2.0 + rf(ctx, 6);
            p.velocity.y = rf(ctx, 4);
            p.velocity.z = rf(ctx, 4);
            p.life_time = 20.0;
        }
        27 => {
            p.velocity = random_speed(ctx, 12.0);
            p.life_time = (ctx.rand() % 10 + 10) as f32;
            p.scale = rf(ctx, 20) / 20.0 + 1.0;
        }
        28 | 29 => {
            p.scale = rf(ctx, 20) / 20.0 + if p.sub_type == 28 { 1.0 } else { 0.5 };
            p.life_time = (ctx.rand() % 10 + if p.sub_type == 28 { 20 } else { 10 }) as f32;
            p.angle.z = rf(ctx, 360);
            p.gravity = if p.sub_type == 28 { 20.0 } else { -20.0 };
            random_planar_velocity(p, ctx);
        }
        30 => {
            p.life_time = (ctx.rand() % 5 + 50) as f32;
            p.scale = rf(ctx, 10) / 10.0 + 0.2;
            p.velocity.x = -2.0 + rf(ctx, 5);
            p.velocity.y = -2.0 + rf(ctx, 5);
            p.velocity.z = -2.0 + rf(ctx, 5);
            p.gravity = 0.5;
        }
        31 => {
            p.scale = (ctx.rand() % 4 + 4) as f32 * 0.1;
            p.life_time = (ctx.rand() % 16 + 24) as f32;
            p.angle.z = rf(ctx, 360);
            p.gravity = (ctx.rand() % 16 + 6) as f32;
            let v = Vec3::new(0.0, (ctx.rand() % 20 + 20) as f32 * 0.1, 0.0) * 3.0;
            p.velocity = rotate(p.angle, v);
        }
        _ => {}
    }
}

/// Ground bounce for `BITMAP_SPARK+1` subtypes 20, 22, 28 and 29.
fn spark1_bounce(p: &mut Particle, ctx: &mut Ctx) {
    p.position.z += p.gravity * 0.5;
    p.gravity -= 1.5;
    let height = ctx.terrain_height(p.position.x, p.position.y);
    if p.position.z < height + 3.0 {
        p.position.z = height + 3.0;
        p.gravity = -p.gravity * 0.3;
        p.life_time -= 2.0;
    }
}

fn step_spark1(p: &mut Particle, ctx: &mut Ctx) {
    match p.sub_type {
        0 | 8 => {
            p.scale -= 0.5;
            if p.scale < 0.2 {
                p.live = false;
            }
        }
        1 => {
            p.scale -= 2.0;
            if p.scale < 0.2 {
                p.live = false;
            }
        }
        2 | 4 => {
            if p.life_time <= 0.0 {
                p.live = false;
            }
            if p.life_time > 5.0 && !target_live(ctx, p.target) {
                p.life_time = 5.0;
            }
            p.light = Vec3::splat(p.life_time / 5.0);
            p.scale += if p.sub_type == 2 { 0.05 } else { 0.08 };
            p.position = p.start_position + p.velocity;
            p.start_position = p.position;
            if p.sub_type == 4 {
                // TODO(original): Position += Target->Position - Target->StartPosition (how far the
                // emitting effect has moved); the Ctx does not expose effects' start positions.
            }
        }
        3 => p.scale *= 0.8,
        5 => {
            p.scale *= 0.9;
            p.light *= 1.0 / 1.03;
        }
        6 => {
            if p.life_time <= 0.0 {
                p.live = false;
            }
            p.position.z -= p.gravity;
            p.position.y -= 1.0;
            p.gravity += 0.1;
            let f = rf(ctx, 10) / 100.0 + 0.7;
            p.light.y = f;
        }
        7 => {
            p.scale -= 0.02;
            p.alpha -= 0.001;
            if p.alpha <= 0.0 {
                p.live = false;
            }
            p.position.z -= p.gravity;
            let f = rf(ctx, 50) / 100.0 + 0.2;
            p.light = Vec3::new(0.0, f, 0.0);
        }
        9 => {
            p.velocity.x -= 1.2;
            p.velocity.z -= 1.0;
            p.scale -= 0.08;
            if p.life_time <= 0.0 || p.scale < 0.2 {
                p.live = false;
            }
        }
        10 => {
            p.light *= 1.0 / 1.08;
            p.scale -= 0.03;
            p.alpha -= 0.001;
            if p.alpha <= 0.0 {
                p.live = false;
            }
        }
        11 | 13 => {
            p.light *= 1.0 / 1.05;
            p.scale -= 0.04;
            p.alpha -= 0.001;
            if p.alpha <= 0.0 {
                p.live = false;
            }
        }
        12 => {
            let f = rf(ctx, 2);
            p.light = Vec3::new(f - 0.6, f - 0.6, f - 0.8);
            p.scale -= 0.04;
            p.alpha -= 0.001;
            if p.alpha <= 0.0 {
                p.live = false;
            }
        }
        14 => {
            p.light *= 1.0 / 1.05;
            p.scale -= 0.01;
            p.alpha -= 0.001;
            if p.alpha <= 0.0 {
                p.live = false;
            }
        }
        15 => {
            p.light *= 1.0 / 1.05;
            p.alpha -= 0.001;
            if p.alpha <= 0.0 {
                p.live = false;
            }
        }
        16 | 18 => {
            let height = ctx.terrain_height(p.position.x, p.position.y);
            if height + 2.0 >= p.position.z {
                p.frame = 88;
            }
            if p.frame == 77 {
                p.gravity *= 1.03;
                p.position.z -= p.gravity;
            } else if p.frame == 88 {
                p.gravity *= 1.0 / 1.2;
                if p.gravity <= 0.1 {
                    p.gravity = 0.1;
                    p.frame = 99;
                }
                p.position.z += p.gravity;
            } else if p.frame == 99 {
                p.gravity *= 1.2;
                p.position.z -= p.gravity;
            }
            p.alpha -= 0.007;
            if p.alpha <= 0.0 {
                p.live = false;
            }
        }
        17 => {
            if p.position.z >= p.start_position.z + 350.0 {
                p.light *= 1.0 / 1.05;
            }
            if p.light.x <= 0.05 {
                p.live = false;
            }
            p.position.x += (ctx.world_time * p.rotation).sin() * p.gravity * 0.3;
            p.position.z += p.gravity;
        }
        19 => {
            if p.life_time <= 0.0 {
                p.live = false;
            }
            p.position.z += p.gravity;
            p.gravity += 0.1;
            p.light *= 1.0 / 1.1;
        }
        20 | 22 => {
            p.light *= 1.0 / 1.02;
            if p.light.x <= 0.05 {
                p.live = false;
            }
            spark1_bounce(p, ctx);
        }
        21 => p.light *= 1.0 / 1.02,
        23 => {
            if p.life_time < 10.0 && p.rotation == 0.0 {
                p.velocity = -p.velocity;
                p.rotation = 1.0;
            }
            p.light *= 1.0 / 1.05;
            p.scale -= 0.02;
            p.alpha -= 0.0001;
            if p.alpha <= 0.0 {
                p.live = false;
            }
        }
        24 => {
            if p.life_time < 10.0 && p.rotation == 0.0 {
                p.velocity = -p.velocity;
                p.rotation = 1.0;
            }
            if p.life_time <= 10.0 {
                p.alpha -= 0.05;
                p.light *= p.alpha;
            }
            p.scale -= 0.02;
        }
        25 => {
            if p.life_time as i32 == 19 {
                p.velocity -= Vec3::ONE;
            }
            p.light *= 1.0 / 1.05;
            p.scale = (p.life_time * (PI / 40.0)).sin();
            p.alpha -= 0.001;
            if p.alpha <= 0.0 {
                p.live = false;
            }
        }
        26 => {
            if p.life_time <= 0.0 {
                p.live = false;
            }
            p.light *= 1.0 / 1.1;
            p.scale -= 0.05;
            p.position = p.start_position + p.velocity;
            p.start_position = p.position;
        }
        27 => {
            p.light *= 1.0 / 1.02;
            p.scale *= 1.0 / 1.03;
        }
        28 | 29 => {
            p.light *= 1.0 / 1.02;
            p.scale *= 1.0 / 1.01;
            if p.light.x <= 0.03 {
                p.live = false;
            }
            spark1_bounce(p, ctx);
        }
        30 => {
            if p.life_time <= 45.0 {
                p.velocity.x += (PI / 180.0 * rf(ctx, 360)).sin();
                p.velocity.y += (PI / 180.0 * rf(ctx, 360)).cos();
            }
            p.light *= 1.0 / 1.05;
            p.scale -= 0.01;
            p.life_time -= 1.0;
            if p.life_time <= 0.0 {
                p.live = false;
            }
        }
        31 => {
            p.position.z += p.gravity;
            p.gravity -= 2.0;
            let height = ctx.terrain_height(p.position.x, p.position.y);
            if p.position.z < height {
                p.position.z = height;
                p.gravity = -p.gravity * 0.6;
                p.life_time -= 4.0;
            }
            p.position += p.velocity;
        }
        _ => {}
    }
}
