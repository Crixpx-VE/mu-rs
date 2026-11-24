//! Particles (family 5): torch, ghost clouds, `BITMAP_LIGHT`, pounding ball, smoke,
//! true fire, hole, waterfalls, cross and shock wave.
//! Translated from `ZzzEffectParticle.cpp` (`CreateParticle`, `MoveParticles`, `DescribeParticle`).

use std::sync::Mutex;

use bevy::math::Vec3;

use crate::fx::core::*;

/// `Random::Rand() % n`.
fn r(ctx: &mut Ctx, n: i32) -> i32 {
    ctx.rand() % n
}

/// `(float)(Random::Rand() % n)`.
fn rf(ctx: &mut Ctx, n: i32) -> f32 {
    (ctx.rand() % n) as f32
}

// --- Particle wind --------------------------------------------------------------------------------

/// `g_vParticleWind` / `g_vParticleWindVelo`: the original updates them once at the start of
/// `MoveParticles`. Only `BITMAP_LIGHT` uses them, so they live here and are updated the first time
/// they are requested in each tick (the tick is detected by `world_time`).
struct Wind {
    time: f32,
    velo: [f32; 2],
    wind: [f32; 2],
}

static WIND: Mutex<Wind> = Mutex::new(Wind { time: f32::NAN, velo: [0.0; 2], wind: [0.0; 2] });

fn particle_wind(ctx: &mut Ctx) -> Vec3 {
    let mut w = WIND.lock().unwrap_or_else(|e| e.into_inner());
    if w.time != ctx.world_time {
        w.time = ctx.world_time;
        for i in 0..2 {
            w.velo[i] += (ctx.rand() % 2001 - 1000) as f32 * (0.001 * 0.6);
            w.velo[i] = w.velo[i].clamp(-0.6, 0.6);
        }
        for i in 0..2 {
            w.wind[i] += w.velo[i];
            w.wind[i] = w.wind[i].clamp(-1.7, 1.7);
        }
    }
    Vec3::new(w.wind[0], w.wind[1], 0.0)
}

// --- Drawing ------------------------------------------------------------------------------------

/// `RenderSprite` with the blend of `RenderParticles` (or the one the case sets).
#[allow(clippy::too_many_arguments)]
fn sprite(out: &mut DrawList, ctx: &DrawCtx, tex: i32, pos: Vec3, w: f32, h: f32, light: Vec3, rot: f32, blend: Option<Blend>) {
    let Some(t) = ctx.tex(tex) else { return };
    out.sprites.push(SpriteDraw {
        texture: tex,
        position: pos,
        width: w,
        height: h,
        color: [light.x, light.y, light.z, if t.components == 3 { 1.0 } else { light.x }],
        rotation: rot,
        uv: [0.0, 0.0, 1.0, 1.0],
        blend: blend.unwrap_or(default_blend(t)),
    });
}

/// Width/Height of `RenderParticles`: size of the `TexType` texture × `Scale`.
fn size(p: &Particle, ctx: &DrawCtx) -> Option<(f32, f32)> {
    ctx.tex(p.tex_type).map(|t| (t.width * p.scale, t.height * p.scale))
}

// --- BITMAP_TORCH_FIRE ------------------------------------------------------------------------------

fn torch_fire_create(p: &mut Particle, ctx: &mut Ctx) {
    let scale = p.scale;
    p.life_time = (r(ctx, 5) + 60) as f32;
    p.scale = (r(ctx, 30) + 30) as f32 * 0.02 + scale / 3.0;

    p.position.x += (r(ctx, 4) - 2) as f32 * 0.25;
    p.position.y += (r(ctx, 4) - 2) as f32 * 0.25;
    p.position.z += (r(ctx, 10) - 5) as f32;

    p.gravity = (r(ctx, 2) + 10) as f32 * 0.5;
}

fn torch_fire_step(p: &mut Particle, _ctx: &mut Ctx) {
    if p.life_time <= 0.0 {
        p.live = false;
    } else {
        p.position.z += p.gravity;
        if p.position.z >= 500.0 {
            p.live = false;
        }
        if p.life_time >= 20.0 {
            p.scale -= 0.02;
            if p.scale <= 0.02 {
                p.live = false;
            }
        }
    }
}

/// Three stacked sprites, each 10 units lower.
fn torch_fire_draw(p: &Particle, ctx: &DrawCtx, out: &mut DrawList) {
    let Some((w, h)) = size(p, ctx) else { return };
    let mut pos = p.position;
    for _ in 0..3 {
        sprite(out, ctx, p.ty, pos, w, h, p.light, p.rotation, None);
        pos.z -= 10.0;
    }
}

// --- BITMAP_GHOST_CLOUD1 / 2 ----------------------------------------------------------------------

fn ghost_cloud_create(p: &mut Particle, ctx: &mut Ctx) {
    let scale = p.scale;
    if p.sub_type == 0 {
        p.life_time = 500.0;
        p.position.x += (r(ctx, 400) - 200) as f32;
        p.position.y += (r(ctx, 400) - 200) as f32;
        p.position.z += (r(ctx, 400) - 200) as f32;
        p.start_position = p.position;
        p.scale = (r(ctx, 15) + 15) as f32 / 30.0 + scale;
        let f = (r(ctx, 10) + 5) as f32 * 0.08;
        p.velocity = Vec3::new(f, f, 0.0);

        // TurningForce stores the final color; starts dark.
        p.turning_force = p.light;
        p.light = Vec3::ZERO;
    }
}

fn ghost_cloud_step(p: &mut Particle, ctx: &mut Ctx) {
    if p.sub_type == 0 {
        p.position += p.velocity;

        let dist = (p.position - p.start_position).length();

        let s = (ctx.world_time * 0.001).sin();
        if s > 0.0 {
            p.rotation += 0.2 + rf(ctx, 4) * 0.1;
        } else {
            p.rotation -= 0.2 + rf(ctx, 4) * 0.1;
        }

        if dist <= 20.0 {
            for i in 0..3 {
                p.light[i] = p.turning_force[i] * 0.1 * (500.0 - p.life_time);
                if p.light[i] > p.turning_force[i] {
                    p.light[i] = p.turning_force[i];
                }
            }
        } else {
            p.light *= 1.0 / 1.02;
        }

        if dist > 500.0 {
            p.live = false;
        }
    }
}

// --- BITMAP_LIGHT -------------------------------------------------------------------------------------

fn light_create(p: &mut Particle, ctx: &mut Ctx) {
    let scale = p.scale;
    let (position, light) = (p.position, p.light);
    match p.sub_type {
        0 | 8 => {
            p.life_time = (r(ctx, 10) + 10) as f32;
            p.gravity = rf(ctx, 10) + 10.0;
            p.scale = (rf(ctx, 50) + 50.0) / 100.0 * scale;
        }
        9 => p.life_time = 40.0 * p.scale,
        7 => {
            p.life_time = (r(ctx, 10) + 30) as f32;
            p.gravity = rf(ctx, 10) + 10.0;
            p.scale = (rf(ctx, 15) + 30.0) / 100.0 * scale;
        }
        // (the original later has another `SubType == 6` with LifeTime = 2 that is never reached)
        6 => {
            p.life_time = (r(ctx, 10) + 10) as f32;
            p.gravity = rf(ctx, 10) + 10.0;
            p.scale = (rf(ctx, 50) + 50.0) / 100.0 * scale;
        }
        1 => p.life_time = 50.0,
        2 => {
            p.life_time = (r(ctx, 10) + 10) as f32;
            p.gravity = rf(ctx, 10) + 10.0;
            p.scale = (rf(ctx, 50) + 50.0) / 100.0 * scale;
        }
        3 => p.life_time = 10.0,
        4 => {
            p.life_time = 10.0;
            p.gravity = 0.0;
            // Rotation stores the character's bone number (passed in Scale).
            p.rotation = scale;
            p.scale = 2.0;
        }
        5 => p.life_time = 50.0,
        10 => {
            p.life_time = 100.0;
            p.scale = scale + rf(ctx, 10) * 0.02;
        }
        11 => {
            p.life_time = (r(ctx, 10) + 130) as f32;
            p.gravity = rf(ctx, 2) + 2.0;
            p.scale = (rf(ctx, 20) + 20.0) / 100.0 * scale;
        }
        12 | 13 => {
            p.life_time = (r(ctx, 10) + 100) as f32;
            p.gravity = rf(ctx, 2) + 2.0;
            p.scale = (rf(ctx, 20) + 20.0) / 100.0 * scale;
        }
        14 => {
            p.life_time = 50.0;
            p.scale = scale + rf(ctx, 10) * 0.02;
        }
        15 => {
            p.life_time = (r(ctx, 10) + 100) as f32;
            p.gravity = rf(ctx, 3) + 2.0;
            p.scale = (rf(ctx, 20) + 20.0) / 100.0 * scale;
            p.start_position = position;
            p.start_position.z = 0.0;
            p.turning_force = light;
            p.alpha = 0.0;
        }
        _ => {}
    }
}

fn light_step(p: &mut Particle, ctx: &mut Ctx) {
    let st = p.sub_type;
    if st == 0 || st == 2 || st == 7 || st == 8 {
        let mut f = if st == 0 || st == 8 { 0.5 } else { 4.0 };

        if st == 7 {
            f = (ctx.world_time + p.life_time).sin() * 5.0 + 10.0;
            p.position.z += f;
        } else {
            let mut v = Vec3::new(
                (ctx.rand() % 2001 - 1000) as f32 * (0.001 * 0.4),
                (ctx.rand() % 2001 - 1000) as f32 * (0.001 * 0.4),
                0.0,
            );
            v += particle_wind(ctx);
            v *= f;
            p.position += v;
            p.position.z += 5.0 * f;
        }
        if st == 0 || st == 8 {
            p.scale -= 0.05;
        } else if st == 7 {
            p.rotation += 30.0;
        } else if st == 1 {
            p.scale *= 0.98;
        } else if st == 5 {
            p.scale *= 0.95;
        } else {
            p.scale *= 0.95;
        }
        if st == 7 {
            if p.life_time < 10.0 {
                p.light *= 1.0 / 1.2;
            }
        } else {
            p.light = Vec3::splat(p.scale);
        }
        if st == 1 || st == 5 {
            p.light = Vec3::splat(0.3 * p.scale);
        }
        if ((st == 0 || st == 8) && p.scale <= 0.1) || (st == 7 && p.scale <= 0.1) || (st == 2 && p.scale <= 0.3) {
            p.life_time = -1.0;
            p.live = false;
        }
        if st == 8 {
            p.light = Vec3::new(1.0 * p.scale, 0.5 * p.scale, 0.3 * p.scale);
        }
    } else if st == 6 {
        let f = 0.5;
        let mut v = Vec3::new(
            (ctx.rand() % 2001 - 1000) as f32 * (0.001 * 0.4),
            (ctx.rand() % 2001 - 1000) as f32 * (0.001 * 0.4),
            0.0,
        );
        v *= f;
        p.position += v;
        p.position.z += 5.0 * f;
        p.scale *= 0.95;

        p.light *= 0.95;
        if p.scale <= 0.1 {
            p.life_time = -1.0;
            p.live = false;
        }
        // TODO(original): RenderParticles enables the depth test when drawing this subtype
        // (and it stays enabled for the following particles).
    } else if st == 1 || st == 5 {
        p.light *= 0.9;
        p.scale *= 0.95;
    } else if st == 3 {
        p.light *= 0.9;
        p.scale *= 0.85;
        p.gravity += 5.0;

        if let Some(tp) = ctx.target_position(p.target) {
            p.position = tp;
        }

        p.position.x += 2.0;
        p.position.y -= 2.0;
        p.position.z += p.gravity;
    } else if st == 4 {
        // TODO(original): it also requires Target->Type == MODEL_PLAYER && Kind == KIND_PLAYER.
        // Angle is the offset along the bone axes and Rotation the bone number.
        match ctx.actor(p.target) {
            Some(a) => p.position = a.bone_world(p.rotation.max(0.0) as usize, p.angle),
            None => p.live = false,
        }

        p.gravity += (r(ctx, 40) + 60) as f32 / 100.0 * 9.5;
        p.scale -= (r(ctx, 400) + 400) as f32 / 10000.0;
        p.position.z += p.gravity;

        p.light *= 1.0 / 1.35;
    } else if st == 9 {
        p.scale -= 0.011;
        if !p.live {
            p.life_time = 0.0;
        }
        if p.scale <= 0.0 {
            p.life_time = 0.0;
        }

        if p.life_time >= 75.0 {
            p.light *= 1.05;
        } else {
            p.light *= 1.0 / 1.05;
        }
    } else if st == 10 {
        p.rotation -= 10.0;
        if let Some(a) = ctx.actor(p.target) {
            if a.action != PLAYER_SANTA_2 as usize {
                p.live = false;
            }
        }
    } else if st == 11 {
        p.position.z += p.gravity;
        p.scale -= 0.0005;
    } else if st == 12 {
        if p.life_time > 80.0 {
            p.scale += 0.02;
        } else {
            p.scale -= 0.005;
        }
    } else if st == 13 {
        p.position.y -= p.gravity;
        p.position.z += p.gravity;
        p.scale -= 0.001;
    } else if st == 14 {
        p.light *= 0.98;
        p.scale *= 0.95;
    } else if st == 15 {
        p.start_position.z += (r(ctx, 10) + 5) as f32 * 0.01;
        p.position.x = p.start_position.x + p.start_position.z.sin() * p.gravity * 2.0;
        p.position.y = p.start_position.y + p.start_position.z.cos() * p.gravity * 2.0;
        p.position.z += p.gravity;
        p.scale -= 0.001;
        if p.life_time > 20.0 {
            p.alpha += 0.1;
            if p.alpha > 1.0 {
                p.alpha = 1.0;
            }
        } else {
            p.scale -= 0.01;
            p.alpha -= 0.1;
            if p.alpha > 1.0 {
                p.alpha = 1.0;
            }
        }
        p.light = p.turning_force * p.alpha;
    }
}

// --- BITMAP_POUNDING_BALL ---------------------------------------------------------------------------

fn pounding_ball_create(p: &mut Particle, ctx: &mut Ctx) {
    let scale = p.scale;
    let light = p.light;
    match p.sub_type {
        0 => {
            p.life_time = 24.0;
            p.velocity = Vec3::new(0.0, -((r(ctx, 16) + 32) as f32) * 0.1, 0.0);
            p.scale = (r(ctx, 128) + 128) as f32 * 0.01;
            p.rotation = rf(ctx, 360);
            p.light = Vec3::splat(0.5);
        }
        1 => {
            p.life_time = 15.0;
            p.velocity = Vec3::new(0.0, -((r(ctx, 16) + 32) as f32) * 0.1, 0.0);
            p.scale = (r(ctx, 128) + 128) as f32 * 0.01;
            p.rotation = rf(ctx, 360);
            p.light = Vec3::splat(0.5);
        }
        2 => {
            p.life_time = 20.0;
            p.scale = (r(ctx, 128) + 80) as f32 * 0.009;
            p.rotation = rf(ctx, 360);
            p.gravity = 0.0;
            p.velocity = Vec3::ZERO;
            p.light = Vec3::splat(0.5);
        }
        3 => {
            p.life_time = (r(ctx, 5) + 47) as f32;
            p.scale = (r(ctx, 72) + 72) as f32 * 0.01 * scale;
            p.rotation = rf(ctx, 360);
            p.gravity = (r(ctx, 24) + 64) as f32 * 0.1;
            p.alpha = 0.0;
            p.turning_force = light;
            p.light = Vec3::ZERO;
        }
        _ => {}
    }
}

#[allow(clippy::nonminimal_bool, clippy::overly_complex_bool_expr)]
fn pounding_ball_step(p: &mut Particle, ctx: &mut Ctx) {
    // Impossible condition in the original (SubType == 0 && SubType == 1): kept as is.
    if p.sub_type == 0 && p.sub_type == 1 {
        p.gravity += 0.004;
        p.scale -= 0.02;

        p.frame = ((23.0 - p.life_time) / 6.0) as i32;
        p.position.z += p.gravity * 10.0;

        if p.sub_type == 1 {
            p.light = Vec3::splat(p.life_time / 10.0);
        }
    } else if p.sub_type == 2 {
        if let Some(a) = ctx.actor(p.target) {
            p.angle = a.angle;
        }

        p.scale -= 0.06;
        p.position.z += p.gravity * 10.0;
        p.light = Vec3::splat(p.life_time / 10.0);
    } else if p.sub_type == 3 {
        if p.life_time < 15.0 {
            p.alpha -= 0.2;
        } else if p.alpha < 1.0 {
            p.alpha += (r(ctx, 2) + 2) as f32 * 0.1;
        } else {
            p.alpha = 1.0;
        }

        if p.alpha < 0.1 {
            p.live = false;
        }
        let lum = p.alpha;
        p.light = p.turning_force * lum;
        if p.scale > 0.0 {
            p.scale -= (r(ctx, 3) + 5) as f32 * 0.01;
        } else {
            p.live = false;
        }
        p.position.z += p.gravity;
        p.rotation += 3.0;
    }
}

// --- BITMAP_ADV_SMOKE ---------------------------------------------------------------------------------

fn adv_smoke_create(p: &mut Particle, ctx: &mut Ctx) {
    let scale = p.scale;
    p.life_time = (20 + r(ctx, 5)) as f32;
    p.rotation = 0.0;
    match p.sub_type {
        0 => {
            p.scale = 0.5 + rf(ctx, 10) * 0.02;
            p.velocity.x = (r(ctx, 10) + 5) as f32 * 0.4;
            p.velocity.y = (r(ctx, 10) - 5) as f32 * 0.4;
            p.velocity.z = (r(ctx, 10) + 5) as f32 * 0.2;
        }
        2 => {
            p.scale = scale * 0.5 + rf(ctx, 10) * 0.02;
            p.velocity.x = (r(ctx, 10) + 5) as f32 * 0.4;
            p.velocity.y = (r(ctx, 10) - 5) as f32 * 0.4;
            p.velocity.z = (r(ctx, 10) + 5) as f32 * 0.2;
        }
        3 => {
            p.scale = scale * 0.5 + rf(ctx, 10) * 0.02;
            p.alpha = 0.5;
            p.velocity.x = 0.0;
            p.velocity.y = (r(ctx, 10) - 5) as f32 * 0.4;
            p.velocity.z = 0.0;
        }
        _ => {
            p.scale = 1.0 + rf(ctx, 10) * 0.1;
            p.velocity.x = (r(ctx, 10) + 5) as f32 * 0.2;
            p.velocity.y = (r(ctx, 10) - 5) as f32 * 0.2;
            p.velocity.z = (r(ctx, 10) + 5) as f32 * 0.1;
        }
    }
}

fn adv_smoke_step(p: &mut Particle, _ctx: &mut Ctx) {
    p.position += p.velocity;
    p.velocity.x *= 0.95;
    p.velocity.y *= 0.95;
    p.light = Vec3::splat(p.life_time / 10.0);
    match p.sub_type {
        0 | 2 => {
            p.velocity.z += 0.3;
            p.scale += 0.07;
        }
        3 => {
            p.velocity.y += 0.1;
            p.scale += 0.05;
            p.alpha -= 0.2;
        }
        _ => {
            p.velocity.z += 0.3;
            p.scale += 0.09;
        }
    }
}

// --- BITMAP_ADV_SMOKE + 1 -------------------------------------------------------------------------------

fn adv_smoke1_create(p: &mut Particle, ctx: &mut Ctx) {
    let scale = p.scale;
    p.life_time = (25 + r(ctx, 5)) as f32;
    p.rotation = rf(ctx, 360);
    p.scale = 0.5;
    if p.sub_type == 1 {
        p.scale *= scale;
    }
    p.velocity.x = (r(ctx, 10) + 5) as f32 * 0.4;
    p.velocity.y = 0.0;
    p.velocity.z = (r(ctx, 10) + 5) as f32 * 0.2;
    // The original has no `break`: it falls through into the BITMAP_TRUE_FIRE code (which sets
    // LifeTime and Velocity again).
    true_fire_create_body(p, ctx, scale);
}

fn adv_smoke1_step(p: &mut Particle, ctx: &mut Ctx) {
    p.position += p.velocity;
    p.velocity.x *= 0.95;
    p.velocity.y *= 0.95;
    p.velocity.z += 0.6;
    p.position.x += (r(ctx, 4) - 2) as f32;
    p.position.y += (r(ctx, 4) - 2) as f32;
    p.position.z += (r(ctx, 4) - 2) as f32 * 0.8;
    p.scale += 0.05;
    if p.sub_type == 2 {
        p.light = Vec3::splat(p.life_time / 25.0 * 2.0 - 1.0);
    } else {
        p.light = Vec3::splat(p.life_time / 25.0);
    }
    p.rotation += 1.0 + rf(ctx, 2);
}

/// `Blend3`; subtype 2 also adds the texture to the color.
fn adv_smoke1_draw(p: &Particle, ctx: &DrawCtx, out: &mut DrawList) {
    let Some((w, h)) = size(p, ctx) else { return };
    // TODO(original): subtype 2 with `additiveTexture` (GL_ADD: texture + vertex color);
    // the material does not support it yet, so it is drawn like the rest with Blend3.
    sprite(out, ctx, p.tex_type, p.position, w, h, p.light, p.rotation, Some(Blend::Blend3));
}

// --- BITMAP_TRUE_FIRE / BITMAP_TRUE_BLUE ----------------------------------------------------------------

fn true_fire_create(p: &mut Particle, ctx: &mut Ctx) {
    let scale = p.scale;
    true_fire_create_body(p, ctx, scale);
}

/// `scale` is the `Scale` argument of `CreateParticle`.
fn true_fire_create_body(p: &mut Particle, ctx: &mut Ctx, scale: f32) {
    match p.sub_type {
        7 => {
            p.scale = scale + rf(ctx, 30) / 100.0;
            p.life_time = 15.0;
            p.position.x += rf(ctx, 10) / 10.0 - 0.5;
            p.position.y += rf(ctx, 10) / 10.0 - 0.5;
            p.position.z += rf(ctx, 10) / 10.0 - 0.5;
        }
        5 => p.life_time = 20.0,
        6 => p.life_time = 32.0,
        8 => p.life_time = 15.0,
        9 => p.life_time = 20.0,
        _ => p.life_time = 24.0,
    }

    if p.sub_type == 8 || p.sub_type == 9 {
        p.velocity.x = (r(ctx, 4) - 2) as f32 * 0.4;
        p.velocity.y = 0.0;
        p.velocity.z = (r(ctx, 4) + 2) as f32 * 0.2;
    } else {
        p.velocity.x = (r(ctx, 10) - 5) as f32 * 0.4;
        p.velocity.y = 0.0;
        p.velocity.z = (r(ctx, 10) + 5) as f32 * 0.2;
    }

    p.start_position = p.position;
}

fn true_fire_step(p: &mut Particle, ctx: &mut Ctx) {
    if (p.sub_type == 1 || p.sub_type == 2) && p.target.is_some() {
        if let Some(a) = ctx.actor(p.target) {
            if a.action == 1 {
                p.life_time -= 2.0;
            }
        }
    }

    if p.sub_type >= 3 && p.sub_type < 5 && p.target.is_some() {
        // TODO(original): BoneManager::GetBonePosition(Target, "Monster82_LHand"/"Monster82_RHand"):
        // with the distance d to that bone, Scale -= d·0.003 and Light -= d·0.08.
    }

    if p.sub_type == 8 {
        // (without a Target the original uses an uninitialized vPos; here it does not move)
        if let Some(a) = ctx.actor(p.target) {
            let v = a.bone_world(20, Vec3::new(6.0, 6.0, 0.0));
            p.position.x = v.x;
            p.position.y = v.y;
        }
    }

    if p.sub_type == 7 {
        p.scale -= 0.1;
        p.position.z += 2.0;
    } else {
        p.scale -= 0.02;
    }

    if p.scale < 0.0 {
        p.scale = 0.0;
    }

    p.velocity.x *= 0.95;
    p.velocity.y *= 0.95;

    // (the original compares `Type == 6`, which is never true)
    if p.ty == 6 {
        p.position.z += 2.0;
    } else {
        p.position.z += 1.0;
    }

    p.light = Vec3::splat(p.life_time / 25.0);
}

// --- BITMAP_HOLE ----------------------------------------------------------------------------------------

fn hole_create(p: &mut Particle, ctx: &mut Ctx) {
    p.life_time = 30.0;
    p.rotation = rf(ctx, 360);
    p.light = Vec3::splat(0.1);
}

fn hole_step(p: &mut Particle, _ctx: &mut Ctx) {
    p.rotation += 5.0;
    if p.life_time < 20.0 {
        p.scale -= 0.08;
        if p.life_time < 10.0 {
            p.light *= 1.0 / 1.3;
        }
    } else if p.life_time < 30.0 {
        p.light *= 1.1;
    }
}

// --- BITMAP_WATERFALL_1 -----------------------------------------------------------------------------------

fn waterfall1_create(p: &mut Particle, ctx: &mut Ctx) {
    let scale = p.scale;
    p.life_time = 30.0;
    p.rotation = rf(ctx, 360);
    p.velocity.z = -((r(ctx, 5) + 7) as f32);

    p.light = Vec3::splat(0.2);

    match p.sub_type {
        0 => p.scale = 1.6,
        1 => {
            p.scale = (r(ctx, 20) + 80) as f32 * 0.01;
            p.tex_type = BITMAP_CLOUD + 2;
        }
        2 => p.scale = 1.6 + scale,
        _ => {}
    }
}

fn waterfall1_step(p: &mut Particle, _ctx: &mut Ctx) {
    p.scale -= 0.005;
    if p.life_time < 5.0 {
        p.light *= 1.0 / 1.2;
    } else if p.life_time > 20.0 {
        p.light *= 1.1;
    }
    p.velocity.z += 0.1;

    if p.sub_type == 1 {
        if p.light.x > 0.5 {
            p.light = Vec3::splat(0.5);
        }
        p.rotation += 1.0;
        p.scale += 0.01;
        p.velocity.z -= 0.4;
    }
}

// --- BITMAP_WATERFALL_5 -----------------------------------------------------------------------------------

fn waterfall5_create(p: &mut Particle, ctx: &mut Ctx) {
    let scale = p.scale;
    match p.sub_type {
        0 => {
            p.life_time = 30.0;
            p.rotation = rf(ctx, 360);
            p.scale = 1.6;
            p.velocity.z = -((r(ctx, 5) + 7) as f32);
            p.light = Vec3::splat(0.2);
        }
        1 => {
            p.life_time = 20.0;
            p.rotation = rf(ctx, 360);
            p.scale = 1.0;
            p.position.z += 50.0;
        }
        2 => {
            p.life_time = 20.0;
            p.rotation = rf(ctx, 360);
            p.scale = 0.5;
            p.position.z += 50.0;
            p.velocity.z = (r(ctx, 5) + 10) as f32;
        }
        3 => {
            p.life_time = 20.0;
            p.rotation = rf(ctx, 360);
            p.scale = 1.6;
            p.velocity.z = -((r(ctx, 5) + 7) as f32);
        }
        4 => {
            p.life_time = 6.0;
            p.gravity = (rf(ctx, 100) / 100.0) * 4.0 + p.angle.x * 1.2;
            p.scale = scale + rf(ctx, 6) * 0.20;
            p.rotation = rf(ctx, 360);
            p.start_position = p.position;
        }
        5 => {
            p.life_time = 30.0;
            p.rotation = rf(ctx, 360);
            p.scale = 1.6 * scale;
            p.velocity.z = -((r(ctx, 5) + 10) as f32);
            p.light = Vec3::splat(0.2);
        }
        6 => {
            p.life_time = 30.0;
            p.rotation = rf(ctx, 360);
            p.scale = scale + 0.6;
            p.velocity.z = -((r(ctx, 5) + 12) as f32);
            p.light = Vec3::splat(0.2);
        }
        7 => {
            p.tex_type = if r(ctx, 2) == 0 { BITMAP_WATERFALL_4 } else { BITMAP_WATERFALL_5 };
            p.life_time = 30.0;
            p.rotation = rf(ctx, 360);
            p.scale = rf(ctx, 50) * 0.05 + scale;
            p.velocity.x = -((r(ctx, 2) + 1) as f32);
            p.velocity.y = -((r(ctx, 2) + 3) as f32);
            p.velocity.z = (r(ctx, 3) + 3) as f32;
        }
        8 => {
            p.life_time = 30.0;
            p.velocity.z = (r(ctx, 3) + 1) as f32;
            p.scale = (r(ctx, 5) + 5) as f32 * 0.05 + scale;
        }
        9 => {
            p.life_time = 30.0;
            p.rotation = rf(ctx, 360);
            p.scale = 0.6 + scale;
            p.velocity.z = -((r(ctx, 5) + 7) as f32);
            p.light = Vec3::splat(0.2);
        }
        _ => {}
    }
}

fn waterfall5_step(p: &mut Particle, ctx: &mut Ctx) {
    match p.sub_type {
        0 | 5 | 9 => {
            p.scale -= 0.005;
            p.velocity.z += 0.1;
        }
        1 => {
            p.scale += 0.1;
            p.position.x += rf(ctx, 10) - 5.0;
            p.position.y += rf(ctx, 10) - 5.0;
        }
        2 => {
            if p.scale < 1.0 {
                p.scale += 0.1;
            }
            p.position.x += rf(ctx, 10) - 5.0;
            p.position.y += rf(ctx, 10) - 5.0;
            p.velocity.z -= 1.0;
        }
        3 => {
            p.scale -= 0.005;
            p.rotation += 4.0;
            p.velocity.z += 0.1;
        }
        4 => {
            // (cos/sin of the degrees as is, like the original)
            p.position.x += p.angle.z.cos() * 20.0;
            p.position.y += p.angle.z.sin() * 20.0;
            p.position.z += p.gravity;
            p.rotation += 1.0;
        }
        7 => {
            p.scale -= 0.005;
            p.rotation += 1.0;
            p.position += p.velocity;
            p.light *= 1.0 / 1.1;
        }
        8 => {
            p.scale += 0.05;
            p.velocity.z -= 0.6;
            p.light *= 1.0 / 1.1;
            return;
        }
        _ => {}
    }

    if p.life_time < 8.0 {
        p.light *= 1.0 / 1.2;
    } else if p.life_time > 20.0 {
        p.light *= 1.1;
    }
}

// --- BITMAP_PLUS ------------------------------------------------------------------------------------------

fn plus_create(p: &mut Particle, ctx: &mut Ctx) {
    p.life_time = 20.0;
    // (o->Scale = Scale: already is)
    p.position.x += rf(ctx, 30) - 15.0;
    p.position.y += rf(ctx, 30) - 15.0;
    p.position.z += 240.0;
}

fn plus_step(p: &mut Particle, ctx: &mut Ctx) {
    p.scale -= 0.01;
    p.position.x += (r(ctx, 2) - 1) as f32;
    p.position.y += (r(ctx, 2) - 1) as f32;
    p.position.z += 2.0;

    p.light = Vec3::splat(p.life_time / 20.0);
}

// --- BITMAP_WATERFALL_2 -----------------------------------------------------------------------------------

fn waterfall2_create(p: &mut Particle, ctx: &mut Ctx) {
    let scale = p.scale;
    let light = p.light;
    let st = p.sub_type;
    p.life_time = 30.0;
    p.rotation = rf(ctx, 360);

    if st == 2 {
        p.life_time = 40.0;
        p.scale = (r(ctx, 6) + 6) as f32 * 0.1 + scale;
    } else if st == 6 {
        p.life_time = (r(ctx, 50) + 20) as f32;
        p.gravity = (r(ctx, 20) + 10) as f32;
        p.scale *= 0.2;
        p.start_position = p.position;
    } else {
        p.scale = (r(ctx, 6) + 6) as f32 * 0.1;
    }

    p.velocity.z = -((r(ctx, 3) + 3) as f32);

    // TODO(original): on the character select screen (SceneFlag == CHARACTER_SCENE) the light is 0.25.
    p.light = Vec3::splat(0.4);

    p.position.x += rf(ctx, 20) - 10.0;
    p.position.y += rf(ctx, 20) - 10.0;
    p.position.z += rf(ctx, 40) - 20.0;

    if st == 1 {
        p.light = Vec3::new(0.0, 0.4, 0.4);
        p.life_time = 50.0;
        p.velocity.z = -((r(ctx, 3) + 3) as f32);
    }
    if st == 3 {
        p.tex_type = BITMAP_LIGHT + 2;
        p.light = light;
        p.life_time = 12.0;
        p.rotation = rf(ctx, 360);
        p.scale = (r(ctx, 3) + 3) as f32 * 0.18;
        p.velocity.z = 2.0;

        p.position.x += rf(ctx, 20) - 10.0;
        p.position.y += rf(ctx, 20) - 10.0;
        p.position.z += rf(ctx, 40) - 20.0;
    }
    if st == 4 {
        p.life_time = 70.0;
        p.scale = (r(ctx, 6) + 6) as f32 * 0.1 + scale;
    }
    if st == 5 {
        p.life_time = 40.0;
        p.rotation = rf(ctx, 360);
        p.gravity = (r(ctx, 2) + 2) as f32;
        p.scale = (r(ctx, 6) + 6) as f32 * 0.1 + scale;
        p.velocity.x = -((r(ctx, 2) + 2) as f32);
        p.velocity.y = -((r(ctx, 2) + 2) as f32);
        p.velocity.z = (r(ctx, 2) + 1) as f32;
        p.position.x += rf(ctx, 60) - 30.0;
        p.position.y += rf(ctx, 60) - 30.0;
        p.position.z += rf(ctx, 10);
        return;
    }
    if st == 11 {
        p.life_time = 30.0;
        p.velocity.z = (r(ctx, 5) + 5) as f32;
        p.scale = (r(ctx, 10) + 10) as f32 * 0.05 * scale;
    }
}

fn waterfall2_step(p: &mut Particle, ctx: &mut Ctx) {
    let st = p.sub_type;
    if st == 5 {
        p.scale += 0.03;
        p.velocity.z += 0.1;

        p.position.x += p.velocity.x;
        p.position.y += p.velocity.y;
        p.position.z += p.gravity + p.velocity.z;

        p.light *= 1.0 / 1.1;
        return;
    }

    p.scale += 0.03;
    p.velocity.x = (r(ctx, 20) - 10) as f32 * 0.1;
    p.velocity.y = (r(ctx, 20) - 10) as f32 * 0.1;
    p.velocity.z += 0.1;
    if p.life_time < 10.0 {
        p.light *= 1.0 / 1.1;
    }

    if st == 1 {
        p.light *= 1.0 / 1.05;
    }
    if st == 3 {
        p.scale -= 0.038;
        p.velocity.z -= 0.02;
        p.light *= 1.0 / 1.02;
    }
    if st == 4 {
        p.rotation -= 1.1;
    }
    if st == 6 {
        p.gravity += rf(ctx, 5) + 1.5 * (p.life_time / 5.0);

        p.velocity.x += 1.0;

        p.velocity.x -= 100.0 * (20.0 / p.life_time);
        p.rotation += 50.0;
        let tp = rotate(p.angle, p.velocity);
        p.position = p.start_position + tp;
        p.scale += (1.0 / p.life_time) * 2.0;

        p.light.x *= 0.8;
        p.light.y *= 0.77;
        p.light.z *= 0.77;

        // (the original immediately overwrites the position computed above)
        p.position = p.start_position;
        p.start_position.z = p.start_position.z + rf(ctx, 5) + 10.0;
        p.position.z = p.start_position.z + p.gravity;
    }
}

// --- BITMAP_WATERFALL_3 / BITMAP_WATERFALL_4 ---------------------------------------------------------------

fn waterfall3_create(p: &mut Particle, ctx: &mut Ctx) {
    let scale = p.scale;
    match p.sub_type {
        0 => {
            p.life_time = 20.0;
            p.velocity.z = (r(ctx, 5) + 2) as f32;
        }
        1 => {
            p.life_time = 10.0;
            p.velocity.z = (r(ctx, 2) + 2) as f32;
        }
        2 => {
            p.life_time = 6.0;
            p.gravity = (rf(ctx, 100) / 100.0) * 4.0 + p.angle.x * 1.2;
            p.scale = scale + rf(ctx, 6) * 0.10;
            p.rotation = rf(ctx, 360);
            p.start_position = p.position;
            return;
        }
        3 => {
            p.life_time = 60.0;
            p.velocity.z = -1.0;
            p.scale *= (r(ctx, 10) + 15) as f32 * 0.02;
            p.rotation = rf(ctx, 360);
            p.position.x += rf(ctx, 40) - 20.0;
            p.position.z += rf(ctx, 25) - 10.0;
            return;
        }
        12 => {
            p.rotation = rf(ctx, 360);
            p.life_time = ((r(ctx, 2) - 1) + 5) as f32;
            let interval = scale * 0.3;
            p.scale += ((r(ctx, 20) - 10) as f32 / 2.0) * (interval / 2.0);
            p.position.x += rf(ctx, 20) - 10.0;
            p.position.y += rf(ctx, 20) - 10.0;
            p.position.z += rf(ctx, 20) - 10.0;
            return;
        }
        // ASG_ADD_MAP_KARUTAN
        16 => {
            p.life_time = 30.0;
            p.rotation = rf(ctx, 360);
            p.scale = 0.6 + scale;
            p.velocity.z = (r(ctx, 3) + 5) as f32;
            return;
        }
        _ => {}
    }

    p.rotation = rf(ctx, 360);
    p.scale = (r(ctx, 10) + 10) as f32 * 0.02;

    match p.sub_type {
        4 => {
            p.life_time = 20.0;
            p.velocity.z = (r(ctx, 5) + 2) as f32;
            p.scale = (r(ctx, 10) + 10) as f32 * 0.02 + scale;
        }
        5 | 6 | 15 => {
            p.life_time = 20.0;
            p.gravity = (rf(ctx, 100) / 100.0) * 2.0;
            p.scale = (r(ctx, 10) + 10) as f32 * 0.02 + scale;
            p.rotation = rf(ctx, 360);
            p.position.x += rf(ctx, 40) - 20.0;
            p.position.y += rf(ctx, 40) - 20.0;
            p.position.z -= 50.0;
            return;
        }
        7 => {
            p.life_time = 60.0;
            p.velocity.z = rf(ctx, 4) * 0.1 + 0.8;
            p.scale *= (r(ctx, 10) + 15) as f32 * 0.02;
            p.rotation = rf(ctx, 360);
            p.position.x += rf(ctx, 18) - 9.0;
            p.position.z += rf(ctx, 18) - 9.0;
            return;
        }
        8 => {
            p.life_time = 30.0;
            p.velocity.z = (r(ctx, 5) + 5) as f32;
            p.scale = (r(ctx, 10) + 10) as f32 * 0.05;
        }
        9 => {
            p.life_time = 30.0;
            p.velocity.z = (r(ctx, 5) + 2) as f32;
            p.scale = (r(ctx, 5) + 5) as f32 * 0.05 + scale;
        }
        10 => {
            p.life_time = 20.0;
            p.gravity = (rf(ctx, 200) / 100.0) + 2.0;
            p.scale = (r(ctx, 10) + 10) as f32 * 0.02 + scale;
            p.rotation = rf(ctx, 360);
            p.velocity.x = -((r(ctx, 2) + 1) as f32);
            p.velocity.y = -((r(ctx, 2) + 2) as f32);
        }
        11 => {
            p.life_time = 20.0;
            p.scale = scale + (r(ctx, 10) - 5) as f32 * 0.05;
            p.rotation = rf(ctx, 360);

            let a = p.angle.z + (r(ctx, 90) - 45) as f32 + 150.0;
            p.velocity = rotate(Vec3::new(0.0, 0.0, a), Vec3::new(0.0, 5.0, 0.0));
            return;
        }
        13 => {
            p.life_time = 30.0;
            p.scale = scale;
            p.rotation = rf(ctx, 360);
            p.gravity = 2.5 + rf(ctx, 10) * 0.1;
            return;
        }
        14 => {
            p.life_time = 30.0;
            p.velocity.z = (r(ctx, 5) + 5) as f32;
            p.scale = (r(ctx, 10) + 10) as f32 * 0.05 * scale;
        }
        _ => {}
    }
    p.position.x += rf(ctx, 40) - 20.0;
    p.position.y += rf(ctx, 40) - 20.0;
    p.position.z += rf(ctx, 20) - 10.0;
}

fn waterfall3_step(p: &mut Particle, ctx: &mut Ctx) {
    match p.sub_type {
        2 => {
            p.position.x += p.angle.z.cos() * 20.0;
            p.position.y += p.angle.z.sin() * 20.0;
            p.position.z += p.gravity;
            p.rotation += 1.0;
            p.light *= 1.0 / 1.1;
            return;
        }
        3 => {
            p.scale += 0.005;
            p.velocity.z -= 0.05;
            p.light *= 0.97;
            return;
        }
        5 => {
            p.position.z -= p.gravity;
            p.gravity -= 0.05;
            p.light *= 1.0 / 1.1;
            return;
        }
        6 => {
            p.position.z += p.gravity;
            p.gravity += 0.05;
            p.alpha -= 0.01;
            p.light *= 1.0 / 1.1;
            return;
        }
        7 => {
            p.velocity.z += 0.01;
            p.light *= 0.97;
            return;
        }
        8 => {
            p.scale += 0.05;
            p.velocity.z -= 0.6;
            p.light *= 1.0 / 1.1;
            return;
        }
        10 => {
            p.position.x += p.velocity.x;
            p.position.y += p.velocity.y;
            p.position.z -= p.gravity;
            p.gravity -= 0.05;
            p.light *= 1.0 / 1.1;
            return;
        }
        11 => {
            p.position += p.velocity;
            p.scale += 0.01;
            p.light *= 1.0 / 1.1;
            return;
        }
        12 => {
            p.rotation += (ctx.world_time as i32 % 360) as f32 * 0.01;
            p.scale *= 0.92;
            p.light *= 0.92;
            p.alpha *= 0.8;
            return;
        }
        13 => {
            p.position.z -= p.gravity;
            p.scale += 0.001;
            p.light *= 1.0 / 1.1;
            return;
        }
        14 => {
            // (no `break` in the original: falls through into the common code below)
            p.scale += 0.05;
            p.velocity.z -= 0.6;
            p.light *= 1.0 / 1.1;
        }
        15 => {
            p.position.z -= p.gravity;
            p.gravity -= 0.05;
            p.scale -= 0.05;
            p.light *= 1.0 / 1.1;
            return;
        }
        // ASG_ADD_MAP_KARUTAN
        16 => {
            p.scale -= 0.005;
            p.velocity.z -= 0.1;
            if p.life_time < 8.0 {
                p.light *= 1.0 / 1.2;
            } else if p.life_time > 20.0 {
                p.light *= 1.1;
            }
            return;
        }
        _ => {}
    }
    p.scale += 0.005;
    p.velocity.z -= 2.5;
    p.light *= 1.0 / 1.1;
}

// --- BITMAP_SHOCK_WAVE --------------------------------------------------------------------------------------

fn shock_wave_create(p: &mut Particle, _ctx: &mut Ctx) {
    // (Scale and Light are already the CreateParticle arguments)
    if p.sub_type == 3 || p.sub_type == 0 {
        p.life_time = 7.0;
    }
    if p.sub_type == 4 {
        p.alpha = 1.0;
        p.life_time = 7.0;
        p.gravity = 6.0;
    }
}

fn shock_wave_step(p: &mut Particle, _ctx: &mut Ctx) {
    if p.sub_type == 3 {
        p.light *= 1.0 / 1.8;
        p.scale += 0.3;
    } else if p.sub_type == 0 {
        p.light *= 1.0 / 1.5;
        p.scale += 0.8;
    }
    if p.sub_type == 4 {
        p.alpha -= 0.001;
        if p.alpha <= 0.0 {
            p.live = false;
        }
        p.light.x *= 1.0 / 1.01;
        p.light.y *= 1.0 / 1.03;
        p.light.z *= 1.0 / 1.03;
        p.scale += p.gravity * 0.015;
        p.gravity += 2.4;
    }
}

// --- Registration ---------------------------------------------------------------------------------------------

pub fn register(r: &mut Registry) {
    let k = |create: ParticleCreate, step: ParticleStep| ParticleKind { create: Some(create), step: Some(step), draw: None };

    r.particle(BITMAP_TORCH_FIRE, ParticleKind { create: Some(torch_fire_create), step: Some(torch_fire_step), draw: Some(torch_fire_draw) });
    r.particle(BITMAP_GHOST_CLOUD1, k(ghost_cloud_create, ghost_cloud_step));
    r.particle(BITMAP_GHOST_CLOUD2, k(ghost_cloud_create, ghost_cloud_step));
    r.particle(BITMAP_LIGHT, k(light_create, light_step));
    r.particle(BITMAP_POUNDING_BALL, k(pounding_ball_create, pounding_ball_step));
    r.particle(BITMAP_ADV_SMOKE, k(adv_smoke_create, adv_smoke_step));
    r.particle(BITMAP_ADV_SMOKE + 1, ParticleKind { create: Some(adv_smoke1_create), step: Some(adv_smoke1_step), draw: Some(adv_smoke1_draw) });
    r.particle(BITMAP_TRUE_FIRE, k(true_fire_create, true_fire_step));
    r.particle(BITMAP_TRUE_BLUE, k(true_fire_create, true_fire_step));
    r.particle(BITMAP_HOLE, k(hole_create, hole_step));
    r.particle(BITMAP_WATERFALL_1, k(waterfall1_create, waterfall1_step));
    r.particle(BITMAP_WATERFALL_5, k(waterfall5_create, waterfall5_step));
    r.particle(BITMAP_PLUS, k(plus_create, plus_step));
    r.particle(BITMAP_WATERFALL_2, k(waterfall2_create, waterfall2_step));
    r.particle(BITMAP_WATERFALL_3, k(waterfall3_create, waterfall3_step));
    r.particle(BITMAP_WATERFALL_4, k(waterfall3_create, waterfall3_step));
    r.particle(BITMAP_SHOCK_WAVE, k(shock_wave_create, shock_wave_step));
}
