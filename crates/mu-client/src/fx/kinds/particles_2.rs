//! Particles translated from `ZzzEffectParticle.cpp` (`CreateParticle`, `MoveParticles` and
//! `DescribeParticle`): BITMAP_ENERGY, BITMAP_MAGIC, BITMAP_FLARE, BITMAP_LIGHT+2, BITMAP_MAGIC+1,
//! BITMAP_BLUE_BLUR, BITMAP_CLUD64, BITMAP_LIGHT+3, BITMAP_TWINTAIL_WATER and BITMAP_SMOKE.

use std::f32::consts::PI;

use bevy::math::Vec3;

use crate::fx::core::*;

pub fn register(r: &mut Registry) {
    r.particle(BITMAP_ENERGY, ParticleKind { create: Some(energy_create), step: Some(energy_step), draw: None });
    r.particle(BITMAP_MAGIC, ParticleKind { create: Some(magic_create), step: Some(magic_step), draw: None });
    r.particle(BITMAP_FLARE, ParticleKind { create: Some(flare_create), step: Some(flare_step), draw: Some(flare_draw) });
    r.particle(BITMAP_LIGHT + 2, ParticleKind { create: Some(light2_create), step: Some(light2_step), draw: Some(light2_draw) });
    r.particle(BITMAP_MAGIC + 1, ParticleKind { create: Some(magic1_create), step: Some(magic1_step), draw: None });
    r.particle(BITMAP_BLUE_BLUR, ParticleKind { create: Some(blue_blur_create), step: Some(blue_blur_step), draw: None });
    r.particle(BITMAP_CLUD64, ParticleKind { create: Some(clud64_create), step: Some(clud64_step), draw: Some(clud64_draw) });
    // `DescribeParticle` draws `o->Type` instead of `TexType`, but this type never changes texture.
    r.particle(BITMAP_LIGHT + 3, ParticleKind { create: Some(light3_create), step: Some(light3_step), draw: None });
    r.particle(
        BITMAP_TWINTAIL_WATER,
        ParticleKind { create: Some(twintail_create), step: Some(twintail_step), draw: Some(twintail_draw) },
    );
    r.particle(BITMAP_SMOKE, ParticleKind { create: Some(smoke_create), step: Some(smoke_step), draw: Some(smoke_draw) });
}

// --- Utilities -----------------------------------------------------------------------------------

/// `(float)(rand() % n)`.
fn rf(ctx: &mut Ctx, n: i32) -> f32 {
    (ctx.rand() % n) as f32
}

/// `(float)((int)WorldTime % 360)`.
fn wt_rot(ctx: &Ctx) -> f32 {
    (ctx.world_time as i32 % 360) as f32
}

/// Position and `Live` of the followed character (if it exists).
fn target_state(ctx: &Ctx, p: &Particle) -> Option<(Vec3, bool)> {
    ctx.actor(p.target).map(|a| (a.position, a.live))
}

/// `Position -= StartPosition; StartPosition = Target->Position; Position += StartPosition`.
fn follow(p: &mut Particle, target: Option<(Vec3, bool)>) {
    if let Some((pos, _)) = target {
        p.position = p.position - p.start_position + pos;
        p.start_position = pos;
    }
}

/// `RenderSprite` of the particle with its texture; `blend` = the case's `SetState` (otherwise that
/// of `RenderParticles`: JPG additive, TGA alpha test).
fn sprite(p: &Particle, ctx: &DrawCtx, out: &mut DrawList, w: Option<(f32, f32)>, rotation: f32, blend: Option<Blend>) {
    let Some(t) = ctx.tex(p.tex_type) else { return };
    let (width, height) = w.unwrap_or((t.width * p.scale, t.height * p.scale));
    out.sprites.push(SpriteDraw {
        texture: p.tex_type,
        position: p.position,
        width,
        height,
        color: [p.light.x, p.light.y, p.light.z, if t.components == 3 { 1.0 } else { p.light.x }],
        rotation,
        uv: [0.0, 0.0, 1.0, 1.0],
        blend: blend.unwrap_or(default_blend(t)),
    });
}

// --- BITMAP_ENERGY ------------------------------------------------------------------------------

fn energy_create(p: &mut Particle, ctx: &mut Ctx) {
    let scale = p.scale;
    p.scale = scale * (ctx.rand() % 8 + 6) as f32 * 0.1;
    p.rotation = rf(ctx, 360);
    p.gravity = 20.0;
    match p.sub_type {
        1 => {
            p.life_time = 10.0;
            p.light = Vec3::splat(0.5);
        }
        2 => {
            p.tex_type = BITMAP_MAGIC + 1;
            p.life_time = 10.0;
            p.scale = 0.3 + scale * (ctx.rand() % 8 + 6) as f32 * 0.1;
            // (The original does `VectorCopy(o->Light, Light)` on the argument: it has no effect.)
            p.rotation = 0.0;
            p.gravity = 0.0;
        }
        // ChainLighting
        3 | 4 | 5 => p.life_time = 25.0,
        6 => p.life_time = 20.0,
        7 => p.life_time = 5.0,
        _ => {}
    }
}

fn energy_step(p: &mut Particle, ctx: &mut Ctx) {
    p.rotation += p.gravity;
    match p.sub_type {
        1 => {
            p.light = Vec3::splat(p.life_time / 15.0);
            p.scale += 0.1;
        }
        2 => p.light -= Vec3::splat(0.01),
        // ChainLighting: attached to a bone of the target (or above it).
        3 | 4 | 5 => {
            if let Some(a) = ctx.actor(p.target) {
                match p.sub_type {
                    3 => p.position = a.bone_world(37, Vec3::ZERO),
                    4 => p.position = a.bone_world(28, Vec3::ZERO),
                    _ => {
                        p.position = a.position;
                        p.position.z += 80.0;
                    }
                }
            }
            p.light *= 1.0 / 1.01;
        }
        7 => p.light *= 0.97,
        _ => {}
    }
}

// --- BITMAP_MAGIC -------------------------------------------------------------------------------

fn magic_create(p: &mut Particle, _ctx: &mut Ctx) {
    if p.sub_type == 0 {
        p.life_time = 10.0;
        // `o->Scale = Scale` (already is).
    }
}

fn magic_step(p: &mut Particle, _ctx: &mut Ctx) {
    if p.sub_type == 0 {
        p.scale -= 0.05;
        p.light -= Vec3::splat(0.01);
    }
}

// --- BITMAP_FLARE -------------------------------------------------------------------------------

fn flare_create(p: &mut Particle, ctx: &mut Ctx) {
    let scale = p.scale;
    p.life_time = 60.0;
    match p.sub_type {
        0 | 3 | 6 | 10 => p.gravity = (ctx.rand() % 100) as f32 / 100.0 * 4.0 + 1.0,
        2 => p.gravity = (ctx.rand() % 100) as f32 / 100.0 * 5.0 + 5.0,
        4 | 5 => {
            p.life_time = 40.0;
            p.gravity = (ctx.rand() % 100) as f32 / 100.0 * 5.0 + 1.0;
        }
        _ => {}
    }
    p.velocity.x = (ctx.rand() % 300 - 150) as f32;
    p.scale = scale + rf(ctx, 6) * 0.01;
    p.rotation = rf(ctx, 360);
    p.angle = Vec3::ZERO;
    p.start_position = p.position;
    if p.sub_type == 10 {
        let count = (p.velocity.x + p.life_time) * 0.1;
        p.start_position.x += count.sin() * 40.0;
        p.start_position.y -= count.cos() * 40.0;
    } else if p.sub_type == 11 {
        p.life_time = (26 + ctx.rand() % 2) as f32;
        p.gravity = 0.0;
        p.velocity.x = 0.0;
        p.scale = scale + rf(ctx, 6) * 0.1;
        p.rotation = rf(ctx, 360);
    } else if p.sub_type == 12 {
        p.life_time = 80.0;
        p.gravity = (ctx.rand() % 100) as f32 / 100.0 * 4.0 + 1.0;
        p.light = Vec3::new(0.2, 0.6, 1.0);
    }
}

fn flare_step(p: &mut Particle, ctx: &mut Ctx) {
    let fade = 1.0 / 1.1;
    match p.sub_type {
        0 | 3 | 6 | 10 => {
            let count = (p.velocity.x + p.life_time) * 0.1;
            if p.sub_type == 10 {
                p.position.x = p.start_position.x;
                p.position.y = p.start_position.y;
            } else {
                p.position.x = p.start_position.x + count.sin() * 40.0;
                p.position.y = p.start_position.y - count.cos() * 40.0;
            }
            p.position.z += p.gravity;
            p.scale -= 0.002;
        }
        2 => {
            p.position.x = p.start_position.x;
            p.position.y = p.start_position.y;
            p.position.z += p.gravity * ((60.0 - p.life_time) / 10.0);
            p.scale -= 0.002;
        }
        5 => {
            let count = (p.velocity.x + p.life_time) * 0.1;
            p.position.x = p.start_position.x + count.sin() * 40.0;
            p.position.y = p.start_position.y - count.cos() * 40.0;
            p.position.z -= p.gravity;
            p.scale -= 0.002;
            p.start_position.x += 2.5;
        }
        _ => {}
    }

    if p.sub_type == 4 {
        let count = (p.velocity.x + p.life_time) * 0.1;
        p.position.x = p.start_position.x + count.sin() * 40.0;
        p.position.y = p.start_position.y - count.cos() * 40.0;
        p.position.z += p.gravity;
        p.scale -= 0.004;
        if p.life_time <= 30.0 {
            p.light *= fade;
        }
    } else if p.life_time <= 20.0 {
        p.light *= fade;
    } else if p.sub_type == 11 {
        p.light *= fade;
        p.scale += 1.5;
    } else if p.sub_type == 12 {
        let count = (p.velocity.x + p.life_time) * 0.1;
        p.position.x = p.start_position.x + count.sin() * 110.0;
        p.position.y = p.start_position.y - count.cos() * 110.0;
        p.position.z += p.gravity + 0.1;
        p.scale -= 0.002;
        if p.life_time <= 35.0 {
            p.light *= fade;
        }
    }

    if p.sub_type == 0 {
        // If the target walks/runs or uses certain skills, the spark stays still and fades out.
        let moving = ctx.actor(p.target).is_some_and(|a| {
            let act = a.action as i32;
            (PLAYER_WALK_MALE..=PLAYER_RUN_RIDE_WEAPON).contains(&act)
                || (PLAYER_ATTACK_SKILL_SWORD1..=PLAYER_ATTACK_SKILL_SWORD5).contains(&act)
                || act == PLAYER_RAGE_UNI_RUN
                || act == PLAYER_RAGE_UNI_RUN_ONE_RIGHT
        });
        if moving {
            p.sub_type = 1;
            p.life_time = 20_i32.min(p.life_time as i32) as f32;
            p.velocity = Vec3::ZERO;
            p.start_position = p.position;
        }
    }
}

fn flare_draw(p: &Particle, ctx: &DrawCtx, out: &mut DrawList) {
    if p.sub_type == 11 {
        let Some(t) = ctx.tex(p.tex_type) else { return };
        sprite(p, ctx, out, Some((t.width * 0.5 * p.scale, t.height * 0.4)), p.rotation, None);
    } else if p.sub_type != 4 {
        // Freshly created (before its first tick) it is not drawn.
        if p.life_time != 60.0 {
            sprite(p, ctx, out, None, p.rotation, None);
        }
    } else {
        sprite(p, ctx, out, None, p.rotation, None);
    }
}

// --- BITMAP_LIGHT + 2 ---------------------------------------------------------------------------

fn light2_create(p: &mut Particle, ctx: &mut Ctx) {
    let scale = p.scale;
    match p.sub_type {
        0 | 4 => {
            p.life_time = 16.0;
            p.scale += (ctx.rand() % 32 + 48) as f32 * 0.01;
            p.angle.x = rf(ctx, 360);
            p.rotation = wt_rot(ctx);
        }
        1 => {
            p.tex_type = BITMAP_SMOKE;
            p.life_time = 20.0;
            p.scale += (ctx.rand() % 10 + 10) as f32 * 0.01;
            p.angle.x = rf(ctx, 360);
            p.rotation = wt_rot(ctx);
            p.gravity = (ctx.rand() % 10 + 40) as f32 * 0.04;
            p.position.y += 10.0;
        }
        2 => {
            p.life_time = 10.0;
            p.scale -= (ctx.rand() % 40 + 48) as f32 * 0.008;
            p.angle.x = rf(ctx, 360);
            p.rotation = wt_rot(ctx);
            p.gravity = 0.0;
        }
        3 => {
            p.life_time = 20.0;
            p.scale += (ctx.rand() % 32 + 48) as f32 * 0.008;
            p.angle.x = rf(ctx, 360);
            p.rotation = wt_rot(ctx);
            if let Some((pos, _)) = target_state(ctx, p) {
                p.start_position = pos;
            }
        }
        5 => {
            p.tex_type = BITMAP_ADV_SMOKE;
            // Spawns on a random bone of the target.
            if let Some(n) = ctx.actor(p.target).map(|a| a.bones.len().max(1) as i32) {
                let bone = (ctx.rand() % n) as usize;
                if let Some(a) = ctx.actor(p.target) {
                    p.position = a.bone_world(bone, Vec3::ZERO);
                }
            }
            p.velocity = Vec3::ZERO;
            p.life_time = 21.0;
            p.scale = (ctx.rand() % 4 + 8) as f32 * 0.01;
            p.gravity = (ctx.rand() % 1 + 4) as f32 * 0.1;
            p.angle.z = rf(ctx, 360);
            let v = Vec3::new(0.0, (ctx.rand() % 2 + 60) as f32 * 0.1, 0.0);
            p.turning_force = rotate(p.angle, v);
            p.turning_force.z += 4.0;
            p.rotation = wt_rot(ctx);
            p.alpha = 1.0;
        }
        6 | 7 => {
            p.life_time = if p.sub_type == 6 { 21.0 } else { 18.0 };
            p.scale = scale + (ctx.rand() % 4 + 8) as f32 * 0.01;
            p.gravity = (ctx.rand() % 1 + 1) as f32 * 0.1;
            p.angle.z = rf(ctx, 360);
            let v = Vec3::new(0.0, (ctx.rand() % 2 + 60) as f32 * 0.1, 0.0);
            p.turning_force = rotate(p.angle, v);
            p.turning_force.z += 2.0;
            p.rotation = wt_rot(ctx);
            p.alpha = 1.0;
            p.velocity = Vec3::ZERO;
        }
        _ => {}
    }
}

/// Common part of subtypes 5, 6 and 7 (smoke thrown out that falls).
fn light2_burst(p: &mut Particle) {
    p.turning_force.z -= p.gravity;
    p.position += p.turning_force;
    p.angle.x += 0.5 * p.life_time;
    p.angle.y += 0.5 * p.life_time;
    p.scale += 0.04;
}

/// Common ending of subtypes 5, 6 and 7 (after the light).
fn light2_burst_fade(p: &mut Particle) {
    if p.life_time < 10.0 {
        p.alpha -= 0.1;
    }
    if p.life_time < 20.0 {
        p.turning_force.x *= 0.6;
        p.turning_force.y *= 0.6;
    }
}

fn light2_step(p: &mut Particle, ctx: &mut Ctx) {
    match p.sub_type {
        3 => {
            p.gravity += 0.2;
            p.scale -= 0.03;
            let t = target_state(ctx, p);
            if p.scale < 0.0 || !t.is_some_and(|t| t.1) {
                p.live = false;
            }
            follow(p, t);
        }
        5 => {
            light2_burst(p);
            light2_burst_fade(p);
            p.light = Vec3::new(p.alpha * 1.0, p.alpha * 0.0, p.alpha * 0.6);
            let t = target_state(ctx, p);
            if p.alpha < 0.0 || !t.is_some_and(|t| t.1) {
                p.live = false;
            }
            follow(p, t);
            p.velocity.x += 0.05;
            p.velocity.y += 0.05;
            p.rotation += 1.0;
        }
        6 => {
            light2_burst(p);
            light2_burst_fade(p);
            p.light = Vec3::splat(p.alpha);
            if p.alpha < 0.0 {
                p.live = false;
            }
            p.velocity.x += 0.05;
            p.velocity.y += 0.05;
            p.rotation += 2.0;
        }
        7 => {
            light2_burst(p);
            p.tex_type = BITMAP_LIGHT;
            light2_burst_fade(p);
            p.light *= p.alpha;
            if p.alpha < 0.0 {
                p.live = false;
            }
            p.velocity.x += 0.05;
            p.velocity.y += 0.05;
            p.rotation += 2.0;
            let lum = p.life_time / 8.0 * 0.02;
            p.light += Vec3::splat(lum);
        }
        _ => {
            let lum = p.life_time / 8.0 * 0.02;
            p.light += Vec3::splat(lum);
            p.gravity += 0.2;
            p.position.z += p.gravity;
            p.scale -= 0.1;
        }
    }
}

fn light2_draw(p: &Particle, ctx: &DrawCtx, out: &mut DrawList) {
    let blend = matches!(p.sub_type, 3 | 4 | 6).then_some(Blend::Subtract);
    sprite(p, ctx, out, None, p.rotation, blend);
}

// --- BITMAP_MAGIC + 1 ---------------------------------------------------------------------------

fn magic1_create(p: &mut Particle, ctx: &mut Ctx) {
    p.life_time = 10.0;
    p.scale += (ctx.rand() % 32 + 48) as f32 * 0.001;
    p.angle.x = rf(ctx, 360);
    p.rotation = wt_rot(ctx);
}

fn magic1_step(p: &mut Particle, _ctx: &mut Ctx) {
    p.light *= 0.9;
    p.scale += 0.1;
}

// --- BITMAP_BLUE_BLUR ---------------------------------------------------------------------------

fn blue_blur_create(p: &mut Particle, ctx: &mut Ctx) {
    match p.sub_type {
        0 | 1 => {
            p.life_time = 30.0;
            p.scale = (ctx.rand() % 64 + 64) as f32 * 0.01;
            p.rotation = rf(ctx, 360);
            p.gravity = (ctx.rand() % 32 + 60) as f32 * 0.1;
            if p.sub_type == 1 {
                p.position.z -= 45.0;
                p.tex_type = BITMAP_POUNDING_BALL;
            }
        }
        _ => {}
    }
}

fn blue_blur_step(p: &mut Particle, _ctx: &mut Ctx) {
    if matches!(p.sub_type, 0 | 1) {
        p.light = Vec3::splat(p.life_time / 20.0);
        if p.sub_type == 1 {
            p.scale += 0.19;
            p.position.z += 5.0;
        } else {
            p.scale += 0.05;
        }
    }
}

// --- BITMAP_CLUD64 ------------------------------------------------------------------------------

fn clud64_create(p: &mut Particle, ctx: &mut Ctx) {
    let scale = p.scale;
    let light = p.light;
    match p.sub_type {
        0 | 2 => {
            if ctx.rand() % 4 != 0 {
                p.tex_type = BITMAP_SMOKE;
            }
            p.life_time = 40.0;
            p.scale = (ctx.rand() % 20 + 250) as f32 * 0.01;
            p.rotation = rf(ctx, 360);
            p.position.x += (ctx.rand() % 20 - 10) as f32;
            p.position.y += (ctx.rand() % 20 - 10) as f32;
            p.position.z -= 20.0;
            p.gravity = (ctx.rand() % 10 + 5) as f32 * 0.1;
        }
        1 => {
            p.life_time = 12.0;
            p.scale += rf(ctx, 30) * 0.01;
            p.rotation = rf(ctx, 360);
            p.position.z += 30.0;
        }
        3 | 4 | 5 => {
            if ctx.rand() % 2 != 0 {
                p.tex_type = BITMAP_SMOKE;
            }
            p.life_time = if p.sub_type == 4 { 4.0 } else { 50.0 };
            p.scale = scale + (ctx.rand() % 10 + 10) as f32 * 0.01;
            p.rotation = rf(ctx, 360);
            p.position.x += (ctx.rand() % 20 - 10) as f32;
            p.position.y += (ctx.rand() % 20 - 10) as f32;
            p.position.z -= 20.0;
            p.gravity = (ctx.rand() % 10 + 5) as f32 * 0.1;
        }
        6 => {
            p.life_time = 25.0;
            p.scale = (ctx.rand() % 8 + 50) as f32 * 0.01 * scale;
            p.rotation = rf(ctx, 360);
            p.gravity = (ctx.rand() % 10 + 35) as f32 * 0.1;
            p.alpha = 0.0;
            // `TurningForce` stores the requested color; the light rises with the alpha.
            p.turning_force = light;
            p.light = Vec3::ZERO;
            if let Some((pos, _)) = target_state(ctx, p) {
                p.start_position = pos;
            }
        }
        7 | 8 => {
            p.life_time = if p.sub_type == 7 { 40.0 } else { 20.0 };
            p.velocity.x = (ctx.rand() % 20 - 10) as f32 * 0.1;
            p.velocity.y = (ctx.rand() % 20 - 10) as f32 * 0.1;
            p.position.z += 80.0;
            p.gravity = 0.0;
            p.alpha = 0.5;
        }
        9 => {
            if ctx.rand() % 2 != 0 {
                p.tex_type = BITMAP_SMOKE;
            }
            p.life_time = 40.0;
            p.alpha = 0.5;
            let interval = scale * 0.1;
            p.scale += ((ctx.rand() % 20 - 10) as f32 / 2.0) * (interval / 2.0);
            p.rotation = rf(ctx, 360);
            p.velocity.x = (ctx.rand() % 20 - 10) as f32 * 0.03;
            p.velocity.y = (ctx.rand() % 20 - 10) as f32 * 0.03;
            p.velocity.z = -(1.2 + (ctx.rand() % 20 - 10) as f32 * 0.025);
            p.gravity = 2.0 + (ctx.rand() % 20 - 10) as f32 * 0.05;
        }
        10 => {
            p.position.x += (ctx.rand() % 10 - 5) as f32 * 0.2;
            p.position.y += (ctx.rand() % 10 - 5) as f32 * 0.2;
            p.position.z += (ctx.rand() % 10 - 5) as f32 * 0.2;
            p.life_time = (ctx.rand() % 28 + 4) as f32;
            p.velocity = Vec3::ZERO;
            p.scale = (ctx.rand() % 5 + 70) as f32 * 0.016;
            p.rotation = rf(ctx, 360);
            p.gravity = (ctx.rand() % 5 + 10) as f32 * 0.01;
            if let Some((pos, _)) = target_state(ctx, p) {
                p.start_position = pos;
            }
        }
        11 => {
            if ctx.rand() % 4 != 0 {
                p.tex_type = BITMAP_SMOKE;
            }
            p.life_time = 30.0;
            p.scale = scale;
            p.rotation = rf(ctx, 360);
            p.position.x += (ctx.rand() % 20 - 10) as f32;
            p.position.y += (ctx.rand() % 20 - 10) as f32;
            p.position.z += (ctx.rand() % 20 - 10) as f32;
            p.gravity = -1.5;
        }
        _ => {}
    }
}

fn clud64_step(p: &mut Particle, ctx: &mut Ctx) {
    let fade = 1.0 / 1.1;
    match p.sub_type {
        0 => {
            p.light = Vec3::splat(p.life_time / 10.0);
            p.gravity -= 0.01;
            p.position.z -= p.gravity;
            p.scale -= 0.03;
            p.alpha -= 0.05;
        }
        1 | 2 => {
            p.scale += 0.5;
            p.light *= fade;
            if p.light.x <= 0.05 {
                p.live = false;
            }
        }
        3 | 5 => {
            p.scale += 0.08;
            p.light *= fade;
            if p.light.x <= 0.05 {
                p.live = false;
            }
        }
        6 => {
            if p.life_time < 10.0 {
                p.alpha -= 0.2;
            } else if p.alpha < 1.0 {
                p.alpha += (ctx.rand() % 2 + 3) as f32 * 0.1;
            } else {
                p.alpha = 1.0;
            }
            if p.alpha < 0.1 {
                p.live = false;
            }
            p.light = p.turning_force * p.alpha;
            if p.scale > 0.0 {
                p.scale -= (ctx.rand() % 5 + 55) as f32 * 0.001;
            } else if p.scale < 0.1 {
                p.live = false;
            }
            p.position.z += p.gravity;
            let t = target_state(ctx, p);
            follow(p, t);
        }
        7 | 8 => {
            if p.life_time < 10.0 {
                p.alpha -= 0.1;
                p.light *= 0.9;
            } else if p.alpha < 1.0 {
                p.alpha += rf(ctx, 20) * 0.005;
            }
            p.position += p.velocity;
            p.scale += rf(ctx, 10) * 0.01;
        }
        9 => {
            p.scale *= 0.98 + (ctx.rand() % 20 - 10) as f32 * 0.002;
            p.light.x *= 0.95 + (ctx.rand() % 20 - 10) as f32 * 0.002;
            p.light.y *= 0.95 + (ctx.rand() % 20 - 10) as f32 * 0.002;
            p.light.z *= 0.95 + (ctx.rand() % 20 - 10) as f32 * 0.002;
            p.alpha *= 0.95 + (ctx.rand() % 20 - 10) as f32 * 0.002;
        }
        10 => {
            p.scale -= (ctx.rand() % 5 + 15) as f32 * 0.0016;
            p.position.z += p.gravity * 10.0;
            let t = target_state(ctx, p);
            if p.scale < 0.0 || !t.is_some_and(|t| t.1) {
                p.live = false;
            }
            follow(p, t);
        }
        11 => {
            p.light = Vec3::splat(p.life_time / 10.0);
            p.gravity -= 0.01;
            p.position.z -= p.gravity;
            p.scale -= 0.03;
            p.alpha -= 0.05;
        }
        _ => {}
    }
}

fn clud64_draw(p: &Particle, ctx: &DrawCtx, out: &mut DrawList) {
    let blend = matches!(p.sub_type, 0 | 5 | 11).then_some(Blend::Subtract);
    sprite(p, ctx, out, None, p.rotation, blend);
}

// --- BITMAP_LIGHT + 3 ---------------------------------------------------------------------------

fn light3_create(p: &mut Particle, ctx: &mut Ctx) {
    let scale = p.scale;
    match p.sub_type {
        0 => {
            p.life_time = (ctx.rand() % 10 + 140) as f32;
            p.gravity = rf(ctx, 10) + 5.0;
            p.scale = (rf(ctx, 20) + 20.0) / 100.0 * scale;
            p.position.x += (ctx.rand() % 100 - 50) as f32;
            p.position.y += (ctx.rand() % 100 - 50) as f32;
        }
        1 => {
            p.scale = 0.1;
            p.start_position.x = p.scale;
            p.life_time = (ctx.rand() % 10 + 20) as f32;
            p.position.x += (ctx.rand() % 50 - 25) as f32;
            p.position.y += (ctx.rand() % 50 - 25) as f32;
            p.position.z += (ctx.rand() % 50 - 25) as f32;
            p.gravity = 1.0;
            p.angle.x = rf(ctx, 360);
            p.angle.y = rf(ctx, 360);
            p.angle.z = rf(ctx, 360);
            let v = Vec3::splat(0.5) * 0.8;
            p.velocity = rotate(p.angle, v);
            p.rotation = (ctx.rand() % 20 - 10) as f32;
        }
        _ => {}
    }
}

fn light3_step(p: &mut Particle, _ctx: &mut Ctx) {
    match p.sub_type {
        0 => {
            let a = (p.velocity.x + p.life_time) * 0.1;
            p.position.x = p.start_position.x + a.sin() * 35.0;
            p.position.y = p.start_position.y - a.cos() * 35.0;
            p.position.z += p.gravity * 0.1;
            p.scale -= 0.001;
        }
        // PJH_ADD_PANDA_PET
        1 => p.scale = (p.life_time * 2.0 * (PI / 180.0)).sin(),
        _ => {}
    }
}

// --- BITMAP_TWINTAIL_WATER ----------------------------------------------------------------------

fn twintail_create(p: &mut Particle, ctx: &mut Ctx) {
    match p.sub_type {
        0 => {
            p.life_time = (50 + ctx.rand() % 10) as f32;
            p.scale = (ctx.rand() % 32 + 140) as f32 * 0.01;
            p.position.x += (ctx.rand() % 120 - 60) as f32;
            p.position.y += (ctx.rand() % 100 - 50) as f32;
            p.position.z += rf(ctx, 100);
        }
        1 => {
            p.life_time = (20 + ctx.rand() % 10) as f32;
            p.scale = (ctx.rand() % 32 + 50) as f32 * 0.01;
            p.position.x += (ctx.rand() % 80 - 40) as f32;
            p.position.y += (ctx.rand() % 80 - 40) as f32;
            p.position.z += rf(ctx, 80);
        }
        2 => {
            p.life_time = (60 + ctx.rand() % 10) as f32;
            p.scale = (ctx.rand() % 32 + 50) as f32 * 0.02;
            p.position.x += (ctx.rand() % 40 - 20) as f32;
            p.position.y += (ctx.rand() % 40 - 20) as f32;
            p.position.z += rf(ctx, 40);
        }
        _ => {}
    }
    p.angle.x = rf(ctx, 360);
    p.angle.z = rf(ctx, 360);
    p.gravity = (ctx.rand() % 10 + 20) as f32 * 0.1;
}

fn twintail_step(p: &mut Particle, _ctx: &mut Ctx) {
    match p.sub_type {
        0 | 1 => p.scale -= 0.013,
        2 => p.scale -= 0.026,
        _ => {}
    }
    p.position.z += p.gravity;
    p.alpha -= 0.05;
    p.light *= 1.0 / 1.02;
}

fn twintail_draw(p: &Particle, ctx: &DrawCtx, out: &mut DrawList) {
    sprite(p, ctx, out, None, p.rotation, Some(Blend::Glow));
}

// --- BITMAP_SMOKE -------------------------------------------------------------------------------

fn smoke_create(p: &mut Particle, ctx: &mut Ctx) {
    let scale = p.scale;
    let light = p.light;
    match p.sub_type {
        0 | 61 | 4 | 9 | 23 => {
            p.life_time = 16.0;
            // (The original compares `o->Type` with MODEL_SLAUGHTERER: never true.)
            if p.ty == MODEL_SLAUGHTERER {
                p.scale = (ctx.rand() % 3 + 28) as f32 * 0.01;
            } else {
                p.scale = (ctx.rand() % 32 + 48) as f32 * 0.01;
            }
            p.angle.x = rf(ctx, 360);
            p.rotation = wt_rot(ctx);
        }
        42 => {
            p.life_time = 16.0;
            p.scale = (ctx.rand() % 32 + 48) as f32 * 0.006;
            p.angle.x = rf(ctx, 360);
            p.rotation = wt_rot(ctx);
        }
        17 => {
            p.turning_force = p.light;
            p.life_time = 12.0;
            p.scale = scale * (ctx.rand() % 32 + 8) as f32 * 0.01;
            p.angle.x = rf(ctx, 360);
            p.rotation = wt_rot(ctx);
        }
        36 | 37 => {
            if p.sub_type == 37 {
                p.turning_force = p.light;
                p.position.z -= 15.0 * p.turning_force.z;
                p.position.y -= 15.0 * p.turning_force.y;
                p.light = Vec3::ONE;
            } else {
                p.light = Vec3::new(1.0, 0.3, 0.0);
            }
            p.life_time = 25.0;
            p.rotation = rf(ctx, 360);
            p.angle.x = rf(ctx, 360);
            // `StartPosition` = offset from the target (used by subtype 36).
            let x = (ctx.rand() % 50 - 25) as f32;
            let y = (ctx.rand() % 50 - 25) as f32;
            let z = -rf(ctx, 50);
            p.start_position = Vec3::new(x, y, z);
        }
        38 => {
            p.tex_type = BITMAP_WATERFALL_2;
            p.life_time = 35.0;
            p.rotation = rf(ctx, 360);
        }
        34 | 35 => {
            p.light = Vec3::new(1.0, 0.2, 0.2);
            p.life_time = 30.0;
            p.rotation = rf(ctx, 360);
            p.velocity = if p.sub_type == 35 { Vec3::new(0.0, -5.0, 0.0) } else { Vec3::ZERO };
        }
        33 => {
            p.life_time = rf(ctx, 150);
            p.scale = scale + 0.1;
            p.angle.x = (ctx.rand() % 90 - 45) as f32;
            p.angle.z = rf(ctx, 360);
            p.velocity = Vec3::new(0.0, -rf(ctx, 8), 0.0);
        }
        32 | 3 => {
            p.life_time = 10.0;
            p.scale = (ctx.rand() % 32 + 80) as f32 * 0.01;
            p.angle.x = (ctx.rand() % 90 - 45) as f32;
            p.angle.z = rf(ctx, 360);
            p.velocity = Vec3::new(0.0, -((ctx.rand() % 8 + 40) as f32), 0.0);
        }
        10 => p.life_time = 16.0,
        12 | 13 | 18 => {
            p.life_time = 20.0;
            p.scale = (ctx.rand() % 64 + 64) as f32 * 0.01;
            p.rotation = rf(ctx, 360);
            p.gravity = (ctx.rand() % 32 + 60) as f32 * 0.1;
        }
        11 | 14 => {
            p.life_time = 50.0;
            p.position.x += (ctx.rand() % 64 - 32) as f32;
            p.position.y += (ctx.rand() % 64 - 32) as f32;
            p.position.z += (ctx.rand() % 64 + 32) as f32;
            p.angle.x = (ctx.rand() % 90 - 45) as f32;
            p.angle.z = rf(ctx, 360);
            p.rotation = rf(ctx, 360);
            p.turning_force = p.light;
            p.velocity = Vec3::new(0.0, -((ctx.rand() % 8 + 40) as f32), 0.0);
        }
        15 => {
            p.life_time = 80.0;
            p.angle.x = rf(ctx, 360);
            p.rotation = wt_rot(ctx);
        }
        53 | 56 | 1 | 40 => {
            p.life_time = 50.0;
            p.scale = (ctx.rand() % 32 + 80) as f32 * 0.01;
            p.position.x += (ctx.rand() % 64 - 32) as f32;
            p.position.y += (ctx.rand() % 64 - 32) as f32;
            p.position.z += (ctx.rand() % 64 + 32) as f32;
            p.angle.x = (ctx.rand() % 90 - 45) as f32;
            p.angle.z = rf(ctx, 360);
            p.velocity = Vec3::new(0.0, -((ctx.rand() % 8 + 40) as f32), 0.0);
        }
        2 | 16 => {
            p.life_time = 50.0;
            p.scale = (ctx.rand() % 64 + 64) as f32 * 0.01;
            p.rotation = rf(ctx, 360);
            p.gravity = (ctx.rand() % 32 + 60) as f32 * 0.1;
        }
        5 => {
            p.life_time = 20.0;
            p.scale = (ctx.rand() % 64 + 98) as f32 * 0.01;
            p.rotation = rf(ctx, 360);
            p.gravity = (ctx.rand() % 32 + 60) as f32 * 0.1;
        }
        6 => {
            p.life_time = 30.0;
            p.gravity = rf(ctx, 1000);
            p.scale = (ctx.rand() % 20 + 180) as f32 * 0.01;
            p.position.x += (ctx.rand() % 200 - 100) as f32 * scale;
            p.position.y += (ctx.rand() % 200 - 100) as f32 * scale;
            p.position.z += (ctx.rand() % 20 + 20) as f32;
            // `Rotation` stores the base height (drawn without rotation).
            p.rotation = p.position.z;
            p.velocity = Vec3::ZERO;
        }
        7 => {
            p.life_time = 30.0;
            p.velocity = Vec3::new(0.0, (ctx.rand() % 4 + 6) as f32 * p.scale, 0.0);
            p.gravity = (ctx.rand() % 200) as f32 / 200.0;
            p.scale *= (ctx.rand() % 20 + 120) as f32 * 0.01;
            p.rotation = rf(ctx, 360);
        }
        8 => {
            p.life_time = 24.0;
            p.velocity = Vec3::new(0.0, -((ctx.rand() % 8 + 32) as f32) * 0.3, 0.0);
            p.scale *= 0.8;
            p.rotation = rf(ctx, 360);
        }
        19 | 20 => {
            if p.sub_type == 19 {
                p.turning_force = p.light;
            }
            p.life_time = 40.0;
            p.scale = scale * (ctx.rand() % 32 + 8) as f32 * 0.01;
            p.angle.x = rf(ctx, 360);
            p.rotation = wt_rot(ctx);
        }
        21 | 22 => {
            p.life_time = if p.sub_type == 21 { 80.0 } else { 60.0 };
            p.scale *= (ctx.rand() % 64 + 64) as f32 * 0.005;
            p.rotation = rf(ctx, 360);
            p.gravity = (ctx.rand() % 32 + 60) as f32 * 0.1;
        }
        24 | 57 => {
            p.life_time = 32.0;
            p.scale = scale + (ctx.rand() % 32 + 48) as f32 * 0.01 * scale;
            p.angle.x = rf(ctx, 360);
            p.rotation = wt_rot(ctx);
        }
        25 | 26 => {
            p.life_time = 25.0;
            p.scale = (ctx.rand() % 64 + 64) as f32 * 0.01;
            p.rotation = rf(ctx, 360);
            p.gravity = (ctx.rand() % 32 + 60) as f32 * 0.1;
        }
        27 => {
            p.life_time = 30.0;
            p.rotation = rf(ctx, 360);
            p.tex_type = BITMAP_CHROME3;
        }
        28 | 29 => {
            p.life_time = 12.0;
            p.scale = 2.3;
            p.rotation = rf(ctx, 360);
            p.gravity = 33.0;
            p.position.z = ctx.terrain_height(p.position.x, p.position.y) + 15.0;
            if p.sub_type == 28 {
                p.tex_type = BITMAP_POUNDING_BALL;
            }
        }
        30 => {
            p.life_time = 4.0;
            p.scale = 1.8;
            p.gravity = 15.0;
            p.rotation = rf(ctx, 360);
            p.position.z = ctx.terrain_height(p.position.x, p.position.y) + 15.0;
            p.position.x += ctx.world_time.sin() * 5.0;
            p.position.y += ctx.world_time.sin() * 5.0;
        }
        31 => {
            p.life_time = 15.0;
            p.scale = scale;
            p.rotation = rf(ctx, 360);
            p.gravity = (ctx.rand() % 32 + 60) as f32 * 0.1;
        }
        41 => {
            p.life_time = 50.0;
            p.scale = (ctx.rand() % 32 + 80) as f32 * 0.01;
            p.position.x += (ctx.rand() % 64 - 32) as f32;
            p.position.y += (ctx.rand() % 64 - 32) as f32;
            p.position.z += (ctx.rand() % 64 + 32) as f32;
            p.angle.x = (ctx.rand() % 90 - 45) as f32;
            p.angle.z = rf(ctx, 360);
            p.velocity = Vec3::new(0.0, -((ctx.rand() % 8 + 40) as f32), 0.0);
            p.gravity = (ctx.rand() % 100 - 50) as f32 * 0.1;
        }
        43 => {
            p.tex_type = BITMAP_CLUD64;
            p.life_time = 40.0;
            p.rotation = rf(ctx, 360);
            p.angle.x = 90.0;
            p.velocity = Vec3::new((ctx.rand() % 20) as f32 / 10.0 - 1.0, 2.0, 15.0);
        }
        44 => {
            p.life_time = 60.0;
            p.scale += rf(ctx, 10) / 2.0;
            p.position.x += rf(ctx, 500) - 250.0;
            p.position.y += rf(ctx, 700) - 350.0;
            p.position.z += rf(ctx, 100) + 150.0;
            p.rotation = rf(ctx, 360);
            p.gravity = rf(ctx, 50) - 25.0;
            p.light = Vec3::new(0.1, 0.2, 0.3);
        }
        45 => {
            p.tex_type = BITMAP_LIGHT + 2;
            p.life_time = 15.0;
            p.scale += rf(ctx, 10) / 20.0;
            p.position.x += rf(ctx, 40) - 20.0;
            p.position.y += rf(ctx, 40) - 20.0;
            p.position.z += rf(ctx, 40) - 20.0;
        }
        46 => {
            p.life_time = (60 + (p.scale * 10.0) as i32) as f32;
            p.scale += rf(ctx, 50) * 0.01;
            p.rotation = rf(ctx, 360);
            p.gravity = (ctx.rand() % 30 + 50) as f32 * 0.05;
        }
        47 => {
            p.tex_type = BITMAP_MAGIC + 1;
            p.life_time = 5.0;
            p.scale += rf(ctx, 50) * 0.01;
            p.rotation = rf(ctx, 360);
        }
        59 | 48 => {
            p.life_time = 40.0;
            p.scale = (ctx.rand() % 64 + 64) as f32 * 0.01;
            p.rotation = rf(ctx, 360);
            p.gravity = (ctx.rand() % 32 + 60) as f32 * 0.1;
        }
        49 => {
            p.tex_type = BITMAP_CLOUD;
            p.life_time = (ctx.rand() % 50 + 100) as f32;
            p.rotation = rf(ctx, 360);
            p.scale = scale * 0.4;
            // `Angle[0]` = rotation direction (±1).
            p.angle.x = if ctx.rand() % 2 == 0 { 1.0 } else { -1.0 };
        }
        50 => {
            p.life_time = 20.0;
            p.scale = (ctx.rand() % 32 + 32) as f32 * 0.01 * scale;
            p.rotation = rf(ctx, 360);
            p.gravity = (ctx.rand() % 16 + 16) as f32 * 0.1;
            p.alpha = 0.0;
            p.turning_force = light;
            p.light = Vec3::ZERO;
        }
        51 => {
            p.life_time = 25.0;
            p.scale = (ctx.rand() % 64 + 64) as f32 * 0.01;
            p.rotation = rf(ctx, 360);
            p.gravity = (ctx.rand() % 32 + 60) as f32 * 0.1;
            p.turning_force = light;
        }
        52 => {
            p.life_time = 40.0;
            p.scale = scale + (ctx.rand() % 10 + 10) as f32 * 0.05;
            p.rotation = rf(ctx, 360);
            let f_angle = p.angle.z + (ctx.rand() % 100 - 50) as f32 + 150.0;
            p.velocity = rotate(Vec3::new(0.0, 0.0, f_angle), Vec3::new(0.0, 10.0, 0.0));
        }
        54 => {
            p.life_time = ((p.scale * 8.0) as i32) as f32;
            p.scale += rf(ctx, 50) * 0.01;
            p.rotation = rf(ctx, 360);
            p.gravity = (ctx.rand() % 30 + 50) as f32 * 0.05;
        }
        55 => {
            p.life_time = 30.0;
            p.scale = scale + (ctx.rand() % 32 + 48) as f32 * 0.01 * scale;
            p.angle.x = rf(ctx, 360);
            p.rotation = wt_rot(ctx);
        }
        58 => {
            p.life_time = 50.0;
            p.scale = (ctx.rand() % 32 + 80) as f32 * 0.01;
            p.position.x += (ctx.rand() % 64 - 32) as f32;
            p.position.y += (ctx.rand() % 64 - 32) as f32;
            p.position.z += (ctx.rand() % 64 + 32) as f32;
            p.angle.x = (ctx.rand() % 90 - 45) as f32;
            p.angle.z = rf(ctx, 360);
            p.velocity = Vec3::new(0.0, -((ctx.rand() % 8 + 40) as f32), 0.0);
            // `StartPosition` stores the requested color.
            p.start_position = p.light;
        }
        60 => {
            p.light = Vec3::splat(0.4);
            p.life_time = 60.0;
            p.scale *= (ctx.rand() % 64 + 64) as f32 * 0.005;
            p.rotation = rf(ctx, 360);
            p.gravity = (ctx.rand() % 32 + 60) as f32 * 0.1;
        }
        62 => {
            p.life_time = 50.0;
            p.position.x += (ctx.rand() % 64 - 32) as f32;
            p.position.y += (ctx.rand() % 64 - 32) as f32;
            p.angle.x = (ctx.rand() % 90 - 45) as f32;
            p.angle.z = rf(ctx, 360);
            p.velocity = Vec3::new(0.0, -rf(ctx, 8), 0.0);
        }
        63 => {
            p.tex_type = BITMAP_CLUD64;
            p.life_time = 10.0;
            p.rotation = rf(ctx, 360);
            p.angle.x = 90.0;
            p.velocity = Vec3::new(0.0, 30.0, 15.0);
        }
        64 => {
            p.life_time = 30.0;
            p.scale *= 0.3;
            p.velocity = Vec3::new(0.0, 10.0, 0.0);
            const ANG_REVISION: f32 = 20.0;
            let f_ang = (ctx.rand() % ANG_REVISION as i32 + 10) as f32;
            p.angle.x += f_ang;
        }
        65 => {
            p.life_time = 45.0;
            p.scale *= (ctx.rand() % 64 + 64) as f32 * 0.005;
            p.rotation = rf(ctx, 360);
            p.gravity = (ctx.rand() % 10 + 18) as f32 * 0.1;
        }
        66 => {
            p.life_time = ((p.scale * 30.0) as i32) as f32;
            p.scale += rf(ctx, 50) * 0.01;
            p.rotation = rf(ctx, 360);
            p.gravity = (ctx.rand() % 30 + 50) as f32 * 0.01;
            let i_angle = (ctx.rand() % 360) as f32;
            // (Original formula as is: it does not convert to radians correctly.)
            p.turning_force.x = (i_angle / PI * 180.0).sin();
            p.turning_force.y = (i_angle / PI * 180.0).cos();
        }
        67 => {
            p.life_time = 45.0;
            p.alpha = 1.0;
            p.rotation = rf(ctx, 360);
            p.gravity = (ctx.rand() % 32 + 60) as f32 * 0.1;
        }
        68 => {
            p.life_time = 45.0;
            p.scale *= (ctx.rand() % 64 + 64) as f32 * 0.005;
            p.rotation = rf(ctx, 360);
            p.gravity = (ctx.rand() % 10 + 18) as f32 * 0.1;
            p.alpha = 1.0;
            let dir = rotate(p.angle, Vec3::new(0.0, 0.0, 1.0));
            p.velocity = dir * 0.6;
        }
        // ASG_ADD_MAP_KARUTAN
        69 => {
            p.life_time = 60.0;
            p.scale *= (ctx.rand() % 64 + 64) as f32 * 0.005;
            p.rotation = rf(ctx, 360);
            p.gravity = (ctx.rand() % 16 + 30) as f32 * 0.1;
            // `Angle` stores the requested color (velocity is zero, so it does not affect movement).
            p.angle = light;
        }
        _ => {}
    }
}

fn smoke_step(p: &mut Particle, ctx: &mut Ctx) {
    let wt = ctx.world_time;
    match p.sub_type {
        0 => {
            p.light = Vec3::splat(p.life_time / 8.0);
            p.gravity += 0.2;
            p.position.z += p.gravity;
            p.scale += 0.05;
        }
        36 => {
            if p.life_time < 23.0 {
                p.gravity += 0.1 * p.scale;
                p.position.z += p.gravity;
                p.position.y += p.gravity;
                p.scale -= 0.08;
            } else if let Some((pos, _)) = target_state(ctx, p) {
                p.position = pos + p.start_position;
            }
            p.rotation += 0.01;
        }
        37 => {
            p.position.z -= p.turning_force.z * 0.8;
            p.position.y -= p.turning_force.y * 0.8;
            p.scale += 0.08;
            p.rotation += 0.01;
            p.light *= 1.0 / 1.03;
        }
        38 => {
            p.scale += 0.09;
            p.light *= 1.0 / 1.04;
        }
        33 => {
            let l = p.life_time / 8.0;
            p.light = Vec3::new(l * 0.4, l * 0.4, l);
            p.velocity *= 0.001;
            p.scale += 0.01 * 2.0;
            p.position.z += p.scale;
        }
        32 | 3 => {
            let l = p.life_time / 8.0;
            p.light = Vec3::new(l * 0.8, l * 0.8, l);
            p.velocity *= 0.4;
            p.scale += 0.1;
        }
        23 => {
            let l = p.life_time / 8.0;
            p.light = Vec3::new(l * 0.1, l, l * 0.6);
            p.gravity += 0.2;
            p.position.z += p.gravity;
            p.scale += 0.05;
        }
        11 | 14 => {
            let l = p.life_time / 50.0;
            p.light = p.turning_force * l;
            p.velocity *= 0.4;
            p.scale += 0.05;
            p.position.z -= 1.0;
        }
        17 => {
            let l = p.life_time / 8.0;
            let l = if l > 1.0 { 1.0 - l } else { l };
            p.light = p.turning_force * l;
            p.gravity += 0.2;
            p.position.z += p.gravity;
            p.scale += 0.05;
        }
        15 | 19 | 20 => {
            let l = p.life_time / 40.0;
            p.light = if p.sub_type == 20 { Vec3::splat(l) } else { Vec3::splat(l * 0.5) };
            p.gravity += (ctx.rand() % 20) as f32 / 500.0;
            p.position.x += (ctx.rand() % 100 - 20) as f32 / 100.0;
            p.position.y += (ctx.rand() % 100 - 20) as f32 / 100.0;
            p.position.z += p.gravity;
            p.scale += rf(ctx, 10) / 1000.0;
            p.rotation = p.scale;
        }
        1 => {
            let l = p.life_time / 50.0;
            p.light = Vec3::new(l * 0.5, l * 1.0, l * 0.8);
            p.velocity *= 0.4;
            p.scale += 0.05;
        }
        2 | 16 | 21 => {
            p.light = Vec3::splat(p.life_time / 50.0);
            p.gravity -= 0.1;
            p.position.x -= p.gravity * 0.2;
            p.position.z += p.gravity;
            p.scale -= 0.01;
        }
        12 => {
            p.light = Vec3::splat(p.life_time / 40.0);
            p.scale += 0.05;
        }
        13 => {
            let l = p.life_time / 20.0;
            p.light = Vec3::new(l * 1.0, l * 0.5, l * 0.1);
            p.scale += 0.09;
        }
        18 | 25 => {
            p.light = Vec3::splat(p.life_time / 20.0);
            p.scale += 0.09;
        }
        4 => {
            let l = p.life_time / 8.0;
            p.light = Vec3::new(l * 120.0 / 255.0, l * 100.7 / 255.0, l * 80.0 / 255.0);
            p.gravity += 0.2;
            p.position.z += p.gravity;
            p.scale += 0.05;
        }
        5 => {
            p.light = Vec3::splat(p.life_time / 10.0);
            p.gravity -= 0.1;
            p.position.x -= p.gravity * (0.2 * (ctx.rand() % 2 + 1) as f32);
            p.position.z += p.gravity;
            p.scale -= 0.01;
        }
        6 => {
            let l = 0.6;
            p.life_time = 10.0;
            p.position.z = p.rotation + ((wt + p.gravity) / 5000.0).sin() * 20.0;
            p.scale = (((p.gravity + wt) as i32 % 1800) as f32 * 0.1 * (PI / 180.0)).sin() * 0.5 + 1.8;
            p.light = Vec3::new(l * 0.6, l * 0.5, l * 0.4);
        }
        7 => {
            let mut l = 1.0;
            p.scale += 0.03;
            p.gravity += 1.0;
            p.velocity.y -= 0.1;
            p.position.z -= p.gravity;
            if p.life_time < 5.0 {
                l = p.life_time / 8.0;
                p.scale -= 0.1;
            }
            p.light = Vec3::new(l * 0.725, l * 0.572, l * 0.333);
        }
        8 => {
            p.gravity += 0.02;
            if p.life_time > 5.0 {
                p.scale += p.gravity;
                p.position.z += p.gravity * 20.0;
                p.velocity *= 1.05;
                p.light = Vec3::splat(p.life_time / 24.0);
                p.velocity *= 0.4;
            } else {
                p.light /= 2.0;
                p.velocity = Vec3::ZERO;
            }
        }
        9 => {
            let l = p.life_time / 10.0;
            p.light = Vec3::new(l * 250.0 / 255.0, l * 156.7 / 255.0, 0.0);
            p.gravity += 0.5;
            p.position.z += p.gravity;
            p.scale += 0.1;
        }
        10 => {
            let l = p.life_time / 16.0;
            p.light = Vec3::new(l * 1.0, l * 0.1, l * 0.1);
            p.scale += 0.05;
        }
        22 => {
            let l = p.life_time / 50.0;
            p.light = Vec3::new(l * 0.9, l * 0.5, l * 0.5);
            p.gravity -= 0.1;
            p.position.x += p.gravity * wt.sin() * 0.05;
            p.position.z += p.gravity;
            p.scale += 0.04;
        }
        24 => {
            let l = p.life_time / 32.0;
            p.light = Vec3::new(l * 0.2, l * 0.5, l * 0.35);
            p.gravity += 0.1;
            p.position.z += p.gravity;
            p.scale += 0.05;
        }
        35 | 34 => {
            p.light *= 1.0 / 1.04;
            p.scale += 0.06;
        }
        26 => {
            let l = p.life_time / 20.0;
            p.light *= l;
            p.scale += 0.15;
        }
        27 => {
            p.light *= 0.92;
            p.scale -= 0.05;
        }
        28 | 29 => {
            p.position.z += p.gravity;
            if p.life_time >= 9.0 {
                p.scale -= 0.4;
            } else {
                p.scale *= 1.2;
            }
            if p.sub_type == 28 {
                p.light *= 0.92;
            } else {
                p.light = Vec3::ONE;
            }
        }
        30 => {
            p.scale += 0.15;
            p.position.z += p.gravity;
            p.light = Vec3::new(1.0, 3.0, 1.0);
        }
        31 => {
            p.scale -= 0.05;
            p.light *= 1.0 / 1.1;
            if p.scale <= 0.0 && p.light.z <= 0.0 {
                p.live = false;
            }
        }
        40 => {
            let l = p.life_time / 50.0;
            p.light = Vec3::new(l * 0.5, l * 0.5, l * 1.0);
            p.velocity *= 0.4;
            p.scale += 0.05;
        }
        41 => {
            let l = p.life_time / 50.0;
            p.light = Vec3::new(l * 0.5, l * 1.0, l * 0.8);
            p.velocity *= 0.4;
            p.scale += 0.05;
            p.gravity += 0.2;
            p.position.z += p.gravity;
        }
        42 => {
            let l = p.life_time / 8.0;
            p.light = Vec3::new(l * 127.0 / 255.0, l * 255.0 / 255.0, l * 200.0 / 255.0);
            p.gravity += 0.2;
            p.position.z += p.gravity;
            p.scale += 0.05;
        }
        43 => {
            p.light *= 1.0 / 1.02;
            p.scale += 0.08;
        }
        44 => {
            if p.life_time >= 30.0 {
                p.light *= 1.07;
            } else {
                p.light *= 1.0 / 1.07;
            }
            p.scale += 0.02;
            p.position.z += 0.8;
            p.rotation += p.gravity / 50.0;
        }
        45 => p.light *= 1.0 / 1.1,
        46 | 54 => {
            p.light *= 1.0 / 1.02;
            if p.light.x <= 0.05 {
                p.life_time = 0.0;
            }
            p.gravity -= 0.05;
            p.position.z += (p.scale + p.gravity) * 1.5;
            p.scale += p.scale / 100.0;
        }
        47 => {
            p.light *= 1.0 / 1.02;
            if p.light.x <= 0.05 {
                p.life_time = 0.0;
            }
            p.scale += 1.0;
        }
        59 => {
            p.light = Vec3::splat(p.life_time / 40.0 * 0.9);
            p.scale += 0.09;
        }
        48 => {
            p.light = Vec3::splat(p.life_time / 40.0 * 0.4);
            p.scale += 0.09;
        }
        49 => {
            if p.life_time < 100.0 {
                p.light *= 0.97;
            } else {
                p.light *= 1.03;
            }
            if p.light.x <= 0.01 {
                p.life_time = 0.0;
            }
            p.velocity *= 0.001;
            p.scale += (ctx.rand() % 15 + 4) as f32 * 0.001;
            p.position.z += p.scale * 6.0;
            p.rotation += (ctx.rand() % 10 + 10) as f32 * 0.1 * p.angle.x;
        }
        50 => {
            if p.life_time < 10.0 {
                p.alpha -= 0.1;
            } else if p.alpha < 0.5 {
                p.alpha += (ctx.rand() % 3 + 2) as f32 * 0.1;
            } else {
                p.alpha = 0.5;
            }
            if p.alpha < 0.1 {
                p.live = false;
            }
            p.light = p.turning_force * p.alpha;
            p.scale += 0.02;
            p.position.z += p.gravity;
        }
        51 => {
            p.light = p.turning_force * (p.life_time / 20.0);
            p.scale += 0.09;
        }
        52 => {
            p.position += p.velocity;
            p.scale += 0.06;
            p.light *= 1.0 / 1.07;
        }
        53 => {
            p.light = Vec3::splat(p.life_time / 50.0);
            p.velocity *= 0.4;
            p.scale += 0.05;
        }
        55 => {
            p.gravity += 0.1;
            p.position.z += p.gravity;
            p.scale += 0.05;
            p.light *= 1.0 / 1.08;
        }
        56 => {
            let l = p.life_time / 50.0;
            p.light = Vec3::new(l * 0.5, l * 0.1, l * 0.8);
            p.velocity *= 0.4;
            p.scale += 0.05;
        }
        57 => {
            let l = p.life_time / 32.0;
            p.light = Vec3::new(l * 0.5, l * 0.1, l * 0.8);
            p.gravity += 0.1;
            p.position.z += p.gravity;
            p.scale += 0.05;
        }
        58 => {
            p.light = p.start_position * (p.life_time / 50.0);
            p.velocity *= 0.4;
            p.scale += 0.05;
        }
        60 => {
            p.light = Vec3::splat(p.life_time / 50.0 * 0.4);
            p.gravity -= 0.1;
            p.position.x += p.gravity * wt.sin() * 0.05;
            p.position.z += p.gravity;
            p.scale += 0.04;
        }
        61 => {
            // (Computes `Luminosity` but does not use it: the light stays as created.)
            p.gravity += 0.2;
            p.position.z += p.gravity;
            p.scale += 0.05;
        }
        62 => {
            p.light = Vec3::splat(p.life_time / 50.0 * 0.9);
            p.velocity *= 0.001;
            p.scale += 0.03;
        }
        63 => {
            p.light *= 1.0 / 1.1;
            p.scale += 0.5;
            p.velocity.y += 1.5;
        }
        64 => {
            let l = p.life_time / 24.0;
            p.light = Vec3::new(l * 0.1, l * 0.7, l * 0.4);
            // Second `MovePosition` of the tick (the core already did the common one).
            p.position += rotate(p.angle, p.velocity);
            p.rotation += 0.05;
            p.gravity -= 0.1;
            p.position.x += p.gravity * wt.sin() * 0.05;
            p.position.z += p.gravity;
            p.scale += (ctx.rand() % 15 + 15) as f32 * 0.01;
            if 16.0 > p.life_time {
                p.alpha -= 0.1;
            }
        }
        65 => {
            p.light = Vec3::splat(p.life_time / 40.0 * 0.4);
            p.gravity -= 0.05;
            p.position.x += p.gravity * wt.sin() * 0.05;
            p.position.z += p.gravity;
        }
        66 => {
            p.light *= 1.0 / 1.02;
            if p.light.x <= 0.05 {
                p.life_time = 0.0;
            }
            p.gravity -= 0.05;
            p.position.x += p.turning_force.x * (p.scale + p.gravity) * 1.0;
            p.position.y += p.turning_force.y * (p.scale + p.gravity) * 1.0;
            p.scale += p.scale / 100.0;
        }
        67 => {
            p.alpha -= 0.01;
            if p.alpha < 0.1 {
                p.live = false;
            }
            p.light *= p.alpha;
            p.scale += 0.05;
            p.rotation += 0.01;
        }
        68 => {
            p.light = Vec3::splat(p.life_time / 40.0 * 0.4);
            p.velocity += p.velocity * 0.03;
            // Second `MovePosition` of the tick.
            p.position += rotate(p.angle, p.velocity);
            if 30.0 > p.life_time {
                p.alpha -= 0.02;
            }
        }
        // ASG_ADD_MAP_KARUTAN: `Angle` stores the color.
        69 => {
            p.light = p.angle * (p.life_time / 50.0);
            p.gravity -= 0.04;
            p.position.x += p.gravity * wt.sin() * 0.05;
            p.position.z += p.gravity;
            p.scale += 0.04;
        }
        _ => {}
    }
}

fn smoke_draw(p: &Particle, ctx: &DrawCtx, out: &mut DrawList) {
    let subtract = matches!(p.sub_type, 2 | 5 | 12 | 14 | 15 | 20 | 21 | 29 | 37 | 38 | 59);
    let blend = subtract.then_some(Blend::Subtract);
    let rotation = if p.sub_type == 6 { 0.0 } else { p.rotation };
    sprite(p, ctx, out, None, rotation, blend);
}
