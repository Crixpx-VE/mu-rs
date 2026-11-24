//! Particles (1): entry effect, flowers, blue flares, bubbles, lightning, fires, flames and
//! rain circles. Translated from `ZzzEffectParticle.cpp` (`CreateParticle`, `MoveParticles` and
//! `DescribeParticle`).

use bevy::math::Vec3;

use crate::fx::core::*;

/// The original `FPS_ANIMATION_FACTOR` at 25 ticks/s.
const FPS: f32 = 1.0;
/// `REFERENCE_FRAME_STEP`: reference ticks per tick (1 at 25 ticks/s).
const REFERENCE_FRAME_STEP: i32 = 1;
/// `WD_34CRYWOLF_1ST`.
const WD_34CRYWOLF_1ST: i32 = 34;

pub fn register(r: &mut Registry) {
    r.particle(BITMAP_EFFECT, ParticleKind { create: Some(create_effect), step: Some(step_effect), draw: None });
    for t in [BITMAP_FLOWER01, BITMAP_FLOWER01 + 1, BITMAP_FLOWER01 + 2] {
        r.particle(t, ParticleKind { create: Some(create_flower), step: Some(step_flower), draw: None });
    }
    r.particle(
        BITMAP_FLARE_BLUE,
        ParticleKind { create: Some(create_flare_blue), step: Some(step_flare_blue), draw: Some(draw_flare_blue) },
    );
    r.particle(
        BITMAP_FLARE + 1,
        ParticleKind { create: Some(create_flare1), step: Some(step_flare1), draw: Some(draw_flare1) },
    );
    r.particle(BITMAP_BUBBLE, ParticleKind { create: Some(create_bubble), step: Some(step_bubble), draw: Some(draw_bubble) });
    r.particle(BITMAP_LIGHTNING + 1, ParticleKind { create: Some(create_lightning1), step: Some(step_lightning1), draw: None });
    r.particle(
        BITMAP_LIGHTNING,
        ParticleKind { create: Some(create_lightning), step: Some(step_lightning), draw: Some(draw_lightning) },
    );
    r.particle(
        BITMAP_CHROME_ENERGY2,
        ParticleKind { create: Some(create_chrome_energy2), step: Some(step_chrome_energy2), draw: Some(draw_chrome_energy2) },
    );
    for t in [BITMAP_FIRE_CURSEDLICH, BITMAP_FIRE_HIK2_MONO] {
        r.particle(t, ParticleKind { create: Some(create_cursedlich), step: Some(step_cursedlich), draw: None });
    }
    r.particle(BITMAP_LEAF_TOTEMGOLEM, ParticleKind { create: Some(create_leaf), step: Some(step_leaf), draw: None });
    for t in [BITMAP_FIRE, BITMAP_FIRE + 2, BITMAP_FIRE + 3] {
        r.particle(t, ParticleKind { create: Some(create_fire), step: Some(step_fire), draw: Some(draw_fire) });
    }
    r.particle(BITMAP_FIRE + 1, ParticleKind { create: Some(create_fire1), step: Some(step_fire1), draw: None });
    r.particle(BITMAP_FLAME, ParticleKind { create: Some(create_flame), step: Some(step_flame), draw: None });
    r.particle(BITMAP_FIRE_RED, ParticleKind { create: Some(create_fire_red), step: Some(step_fire_red), draw: None });
    for t in [BITMAP_RAIN_CIRCLE, BITMAP_RAIN_CIRCLE + 1] {
        r.particle(t, ParticleKind { create: Some(create_rain_circle), step: Some(step_rain_circle), draw: None });
    }
}

// --- Helpers ----------------------------------------------------------------------------------------

/// `rand() % n` as `f32`.
fn rf(ctx: &mut Ctx, n: i32) -> f32 {
    (ctx.rand() % n) as f32
}

/// `o->Target->Position` and `o->Target->Live` (None if there is no target).
fn target_state(ctx: &Ctx, t: Option<Target>) -> Option<(Vec3, bool)> {
    match t? {
        Target::Actor(_) => ctx.actor(t).map(|a| (a.position, a.live)),
        Target::Effect(_) => ctx.target_position(t).map(|p| (p, true)),
    }
}

/// `o->Frame = (a - o->LifeTime) / b` (float truncated to int).
fn frame_of(a: f32, life: f32, b: f32) -> i32 {
    ((a - life) / b) as i32
}

/// `Sprite(tex, pos, W, H, Light, Rot, u, v, uw, vh)` with the `RenderParticles` blending
/// (JPG → additive; TGA → alpha test) or the one set by the case.
#[allow(clippy::too_many_arguments)]
fn sprite(
    out: &mut DrawList,
    t: TexInfo,
    texture: i32,
    position: Vec3,
    width: f32,
    height: f32,
    light: Vec3,
    rotation: f32,
    uv: [f32; 4],
    blend: Option<Blend>,
) {
    out.sprites.push(SpriteDraw {
        texture,
        position,
        width,
        height,
        color: [light.x, light.y, light.z, if t.components == 3 { 1.0 } else { light.x }],
        rotation,
        uv,
        blend: blend.unwrap_or_else(|| default_blend(t)),
    });
}

const FULL_UV: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

// --- BITMAP_EFFECT ----------------------------------------------------------------------------------

fn create_effect(p: &mut Particle, ctx: &mut Ctx) {
    let base = p.position;
    match p.sub_type {
        1 => {
            p.tex_type = BITMAP_EXT_LOG_IN + 2;
            p.scale = (rf(ctx, 10) + 20.0) * 0.01;
            p.gravity = (rf(ctx, 10) + 20.0) * 0.05;
        }
        2 => {
            p.gravity = 0.0;
            p.scale = (rf(ctx, 5) + 12.0) * 0.1;
        }
        3 => {
            p.tex_type = BITMAP_CLUD64;
            p.scale = (rf(ctx, 5) + 10.0) * 0.01;
            p.gravity = (rf(ctx, 10) + 20.0) * 0.05;
        }
        0 => {
            p.gravity = 0.0;
            p.scale = (rf(ctx, 5) + 20.0) * 0.1;
        }
        _ => {}
    }
    p.life_time = 20.0 + rf(ctx, 3);
    if p.sub_type == 2 {
        p.life_time = 40.0 + rf(ctx, 3);
    }
    p.position.x += rf(ctx, 50) - 25.0;
    p.position.y += rf(ctx, 50) - 25.0;
    p.position.z += (rf(ctx, 200) - 100.0) + 250.0;

    match p.sub_type {
        // Selected character's lightning: the `PJH_NEW_SERVER_SELECT_MAP` branch of Main 5.2 (S6),
        // with ×1.3 (ZzzEffectParticle.cpp:104-116).
        4 => {
            p.gravity = 0.0;
            p.scale = (rf(ctx, 5) + 20.0) * 0.1 * 1.3;
            p.position.x = base.x + (rf(ctx, 80) - 40.0) * 1.3;
            p.position.z -= 100.0;
        }
        5 => {
            p.tex_type = BITMAP_EXT_LOG_IN + 2;
            p.scale = (rf(ctx, 10) + 20.0) * 0.01 * 1.3;
            p.gravity = (rf(ctx, 10) + 20.0) * 0.05 * 1.3;
            p.position.z -= 100.0;
        }
        6 => {
            p.gravity = (rf(ctx, 20) + 80.0) * 0.1;
            p.scale = (rf(ctx, 5) + 20.0) * 0.1;
            p.position.z -= 200.0;
        }
        7 => {
            p.tex_type = BITMAP_EXT_LOG_IN + 2;
            p.scale = (rf(ctx, 10) + 20.0) * 0.01;
            p.gravity = (rf(ctx, 20) + 80.0) * 0.1;
            p.position.z -= 100.0;
        }
        _ => {}
    }
}

fn step_effect(p: &mut Particle, _ctx: &mut Ctx) {
    let f = if p.life_time >= 10.0 {
        if p.sub_type == 2 { 1.0 / 1.03 } else { 1.16 }
    } else if p.sub_type == 2 {
        1.0 / 1.03
    } else {
        1.0 / 1.16
    };
    p.light *= f;
    p.position.z += p.gravity * FPS;
}

// --- BITMAP_FLOWER01 (+1, +2) -----------------------------------------------------------------------

fn create_flower(p: &mut Particle, ctx: &mut Ctx) {
    p.life_time = 15.0 + rf(ctx, 10);
    p.scale = (rf(ctx, 8) + 4.0) * 0.03;
    p.light = Vec3::ONE;
    p.velocity.x = (rf(ctx, 32) - 16.0) * 0.1;
    p.velocity.y = (rf(ctx, 32) - 16.0) * 0.1;
    p.velocity.z = (rf(ctx, 32) - 32.0) * 0.1;
}

fn step_flower(p: &mut Particle, ctx: &mut Ctx) {
    if p.frame == 0 {
        if p.sub_type == 0 {
            p.velocity.x += (rf(ctx, 32) - 16.0) * 0.1 * FPS;
            p.velocity.y += (rf(ctx, 32) - 16.0) * 0.1 * FPS;
            p.velocity.z += (rf(ctx, 32) - 16.0) * 0.1 * FPS;
        } else if p.sub_type == 1 {
            p.velocity.x += (rf(ctx, 8) - 4.0) * 0.05 * FPS;
            p.velocity.y += (rf(ctx, 8) - 4.0) * 0.05 * FPS;
            p.velocity.z -= rf(ctx, 8) * 0.05 * FPS;
        }
        // VectorAddScaled: extra unrotated advance (on top of the common MovePosition).
        p.position += p.velocity * FPS;
    }
    let height = ctx.terrain_height(p.position.x, p.position.y);
    if p.position.z < height {
        p.position.z = height;
        p.velocity = Vec3::ZERO;
        p.frame = 1;
    }
    p.rotation += rf(ctx, 16) * FPS;
}

// --- BITMAP_FLARE_BLUE ------------------------------------------------------------------------------

fn create_flare_blue(p: &mut Particle, ctx: &mut Ctx) {
    let scale = p.scale;
    if p.sub_type == 0 {
        p.life_time = 30.0 + rf(ctx, 10);
        p.light = Vec3::ONE;
        p.velocity = Vec3::new(0.0, 0.0, rf(ctx, 100) / 50.0);
    } else if p.sub_type == 1 {
        p.life_time = 26.0 + rf(ctx, 2);
        p.gravity = 0.0;
        p.velocity.x = 0.0;
        p.scale = scale + rf(ctx, 6) * 0.1;
        p.rotation = rf(ctx, 360);
    }
}

fn step_flare_blue(p: &mut Particle, _ctx: &mut Ctx) {
    if p.sub_type == 0 {
        p.scale = 0.2;
        p.velocity.z += 0.4 * FPS;
        p.velocity.z = (8.0 * FPS).min(p.velocity.z);
        if p.life_time < 5.0 {
            // As in the original: 0.98 + (0.02·1 − FPS_ANIMATION_FACTOR) (= 0 at 25 ticks/s).
            p.light *= 0.98 + (0.02 * 1.0 - FPS);
        }
    } else if p.sub_type == 1 {
        // As in the original: 0.99 + (0.01·1 − FPS_ANIMATION_FACTOR) (= 0 at 25 ticks/s).
        p.light.x *= 0.99 + (0.01 * 1.0 - FPS);
        p.light.y *= 1.0 / 1.1;
        p.light.z *= 1.0 / 1.1;
        p.scale += 1.5 * FPS;
    }
}

fn draw_flare_blue(p: &Particle, ctx: &DrawCtx, out: &mut DrawList) {
    let Some(t) = ctx.tex(p.tex_type) else { return };
    if p.sub_type == 0 {
        sprite(out, t, p.tex_type, p.position, t.width * p.scale, t.height * p.scale, p.light, p.rotation, FULL_UV, None);
    } else if p.sub_type == 1 {
        let w = t.width * 0.2 * p.scale;
        let h = t.height * 0.3;
        sprite(out, t, p.tex_type, p.position, w, h, p.light, p.rotation, FULL_UV, None);
    }
}

// --- BITMAP_FLARE + 1 -------------------------------------------------------------------------------

fn create_flare1(p: &mut Particle, ctx: &mut Ctx) {
    let scale = p.scale;
    if p.sub_type == 0 {
        p.life_time = 110.0 + rf(ctx, 10);
        p.gravity = (rf(ctx, 100) / 100.0) * 2.0 + 1.0;
        p.velocity.x = rf(ctx, 100) - 50.0;
        p.scale = scale + rf(ctx, 2) * 0.01;
        p.rotation = rf(ctx, 360);
        p.angle = Vec3::ZERO;
        p.start_position = p.position;
    }
}

fn step_flare1(p: &mut Particle, _ctx: &mut Ctx) {
    if p.sub_type == 0 {
        let count = (p.velocity.x + p.life_time) * 0.05;
        let r = 105.0 + p.scale * -250.0;
        p.position.x = p.start_position.x + count.sin() * r;
        p.position.y = p.start_position.y - count.cos() * r;
        p.position.z += p.gravity * FPS;
        p.scale -= 0.0008 * FPS;
    }
}

fn draw_flare1(p: &Particle, ctx: &DrawCtx, out: &mut DrawList) {
    if p.sub_type == 0 {
        default_particle_draw(p, ctx, out);
    }
}

// --- BITMAP_BUBBLE ----------------------------------------------------------------------------------

fn create_bubble(p: &mut Particle, ctx: &mut Ctx) {
    let scale = p.scale;
    match p.sub_type {
        0 | 1 | 4 => {
            p.life_time = 30.0 + rf(ctx, 10);
            p.scale = (rf(ctx, 6) + 4.0) * 0.03;
            p.light = Vec3::ONE;
        }
        2 => {
            p.life_time = 30.0 + rf(ctx, 10);
            p.scale = (rf(ctx, 6) + 4.0) * 0.03;
            p.light = Vec3::ONE;
            ctx.play_sound("SOUND_DEATH_BUBBLE");
        }
        3 => {
            p.life_time = 30.0 + rf(ctx, 10);
            p.scale = (rf(ctx, 6) + 4.0) * 0.03 + scale;
            p.gravity = (rf(ctx, 6) + 4.0) * 0.03;
            p.light = Vec3::ONE;
        }
        5 => {
            p.life_time = 30.0 + rf(ctx, 10);
            p.scale = (rf(ctx, 6) + 4.0) * 0.04;
            p.light = Vec3::ONE;
        }
        _ => {}
    }
}

fn step_bubble(p: &mut Particle, ctx: &mut Ctx) {
    match p.sub_type {
        0 | 5 => {
            p.frame += REFERENCE_FRAME_STEP;
            p.position.x += (rf(ctx, 20) - 10.0) * 2.5 * p.scale * FPS;
            p.position.y += (rf(ctx, 20) - 10.0) * 2.5 * p.scale * FPS;
            p.position.z += (rf(ctx, 20) + 10.0) * 2.5 * p.scale * FPS;
            if p.sub_type == 5 && p.position.z > 550.0 {
                p.live = false;
            }
        }
        1 => {
            p.frame += REFERENCE_FRAME_STEP;
            p.position.x += (rf(ctx, 30) - 15.0) * 1.0 * p.scale * FPS;
            p.position.y += (rf(ctx, 30) - 15.0) * 1.0 * p.scale * FPS;
            p.position.z += (rf(ctx, 20) + 10.0) * 0.3 * p.scale * FPS;
            p.scale += FPS * 0.002;
        }
        2 => {
            p.position.z += (rf(ctx, 20) + 10.0) * 0.3 * FPS;
            p.scale += FPS * 0.005;
        }
        3 => {
            p.frame += REFERENCE_FRAME_STEP;
            p.position.x += (rf(ctx, 20) - 10.0) * 2.5 * p.gravity * FPS;
            p.position.y += (rf(ctx, 20) - 10.0) * 2.5 * p.gravity * FPS;
            p.position.z += (rf(ctx, 20) + 10.0) * 2.5 * p.gravity * FPS;
        }
        4 => {
            p.position.z += (rf(ctx, 20) + 10.0) * 0.3 * FPS;
            p.scale += FPS * 0.005;
            if p.position.z > 350.0 {
                p.position.z = 350.0;
                p.life_time = 0.0;
            }
        }
        _ => {}
    }
}

fn draw_bubble(p: &Particle, ctx: &DrawCtx, out: &mut DrawList) {
    let Some(t) = ctx.tex(p.tex_type) else { return };
    let frame = p.frame % 9;
    let uv = [
        (frame % 3) as f32 * 0.25 + 0.005,
        (frame / 3) as f32 * 0.25 + 0.005,
        0.25 - 0.01,
        0.25 - 0.01,
    ];
    sprite(out, t, p.tex_type, p.position, t.width * p.scale, t.height * p.scale, p.light, 0.0, uv, None);
}

// --- BITMAP_LIGHTNING + 1 ---------------------------------------------------------------------------

fn create_lightning1(p: &mut Particle, _ctx: &mut Ctx) {
    let (scale, light) = (p.scale, p.light);
    match p.sub_type {
        0 => p.scale = 0.15,
        4 => p.scale = 0.25,
        1 => {
            p.life_time = 10.0;
            p.scale = 0.0;
        }
        2 => {
            p.life_time = 20.0;
            p.scale = 1.0;
        }
        3 => {
            p.life_time = 1.0;
            p.scale = scale;
        }
        5 => p.scale = scale,
        _ => {}
    }
    if p.sub_type == 4 || p.sub_type == 5 {
        p.light = light;
    } else {
        p.light = light + Vec3::splat(0.5);
    }
}

fn step_lightning1(p: &mut Particle, ctx: &mut Ctx) {
    // std::max<int>(0, LifeTime): life truncated to an integer.
    let life = (p.life_time as i32).max(0) as f32 * FPS;
    p.rotation = ((ctx.world_time as i32) % 1000) as f32 * 0.001;
    let lum = life / 10.0;
    let light = Vec3::new(lum * 0.5, lum * 1.0, lum * 0.8);
    ctx.add_terrain_light(p.position.x, p.position.y, light, 3.0);
    if p.sub_type == 2 {
        p.scale += FPS * 0.1;
        p.light = light;
        if let Some((tp, _)) = target_state(ctx, p.target) {
            p.position = tp;
            p.position.z += 80.0 * FPS;
        }
    }
}

// --- BITMAP_LIGHTNING -------------------------------------------------------------------------------

fn create_lightning(p: &mut Particle, ctx: &mut Ctx) {
    p.life_time = 10.0;
    p.scale = 1.8;
    p.angle.x = 30.0;
    let r = (rf(ctx, 4) + 6.0) * 0.1;
    let g = (rf(ctx, 4) + 6.0) * 0.1;
    let b = (rf(ctx, 4) + 6.0) * 0.1;
    p.light = Vec3::new(r, g, b);
    p.position.z += 260.0;
}

fn step_lightning(p: &mut Particle, ctx: &mut Ctx) {
    p.frame = ctx.rand() % 4;
    let lum = p.life_time / 5.0;
    p.light = Vec3::splat(lum);
    ctx.add_terrain_light(p.position.x, p.position.y, Vec3::splat(-lum * 0.6), 6.0);
    ctx.add_terrain_light(p.position.x, p.position.y, Vec3::new(lum * 0.2, lum * 0.4, lum * 1.0), 4.0);
}

fn draw_lightning(p: &Particle, ctx: &DrawCtx, out: &mut DrawList) {
    let Some(t) = ctx.tex(p.tex_type) else { return };
    let uv = [(p.frame % 4) as f32 * 0.25, 0.0, 0.25, 1.0];
    sprite(out, t, p.tex_type, p.position, t.width * p.scale * 0.25, t.height * p.scale, p.light, p.angle.x, uv, None);
}

// --- BITMAP_CHROME_ENERGY2 --------------------------------------------------------------------------

fn create_chrome_energy2(p: &mut Particle, ctx: &mut Ctx) {
    let scale = p.scale;
    p.life_time = 8.0;
    p.velocity = Vec3::ZERO;
    p.scale = scale * (rf(ctx, 64) + 128.0) * 0.01;
    p.rotation = rf(ctx, 360);
}

fn step_chrome_energy2(p: &mut Particle, _ctx: &mut Ctx) {
    p.gravity = 0.0;
    p.scale -= 0.04 * FPS;
    p.rotation += 5.0 * FPS;
    p.frame = frame_of(23.0, p.life_time, 6.0);
}

fn draw_chrome_energy2(p: &Particle, ctx: &DrawCtx, out: &mut DrawList) {
    let Some(t) = ctx.tex(p.tex_type) else { return };
    let uv = [(p.frame % 4) as f32 * 0.25, 0.0, 0.25, 1.0];
    sprite(out, t, p.tex_type, p.position, t.width * p.scale * 0.25, t.height * p.scale, p.light, p.rotation, uv, None);
}

// --- BITMAP_FIRE_CURSEDLICH / BITMAP_FIRE_HIK2_MONO -------------------------------------------------

fn create_cursedlich(p: &mut Particle, ctx: &mut Ctx) {
    let (scale, light) = (p.scale, p.light);
    match p.sub_type {
        0 => {
            p.life_time = rf(ctx, 12) + 8.0;
            p.velocity = Vec3::ZERO;
            p.scale = (rf(ctx, 30) + 20.0) * 0.01;
            p.rotation = rf(ctx, 360);
            p.gravity = (rf(ctx, 15) + 15.0) * 0.01;
        }
        1 => {
            p.position.x += (rf(ctx, 10) - 5.0) * 0.2;
            p.position.y += (rf(ctx, 10) - 5.0) * 0.2;
            p.position.z += (rf(ctx, 10) - 5.0) * 0.2;
            p.life_time = rf(ctx, 28) + 4.0;
            p.velocity = Vec3::ZERO;
            p.scale = (rf(ctx, 5) + 50.0) * 0.016;
            p.rotation = rf(ctx, 360);
            p.gravity = (rf(ctx, 5) + 10.0) * 0.01;
            if let Some((tp, _)) = target_state(ctx, p.target) {
                p.start_position = tp;
            }
        }
        2 => {
            p.life_time = rf(ctx, 12) + 8.0;
            p.velocity = Vec3::ZERO;
            p.scale = (rf(ctx, 30) + 20.0) * 0.01 * scale;
            p.rotation = rf(ctx, 360);
            p.gravity = (rf(ctx, 15) + 15.0) * 0.01;
        }
        3 => {
            p.life_time = rf(ctx, 12) + 16.0;
            p.velocity = Vec3::ZERO;
            p.scale = (rf(ctx, 30) + 20.0) * 0.02;
            p.rotation = rf(ctx, 360);
            p.gravity = (rf(ctx, 15) + 15.0) * 0.02;
            if let Some((tp, _)) = target_state(ctx, p.target) {
                p.start_position = tp;
            }
        }
        4 | 9 => {
            p.life_time = rf(ctx, 5) + 12.0;
            p.scale = (rf(ctx, 72) + 72.0) * 0.01 * scale;
            p.rotation = rf(ctx, 360);
            p.gravity = (rf(ctx, 24) + 64.0) * 0.1;
            p.alpha = 0.0;
            p.turning_force = light;
            p.light = Vec3::ZERO;
        }
        5 => {
            p.life_time = rf(ctx, 5) + 24.0;
            p.scale = (rf(ctx, 72) + 72.0) * 0.01 * scale;
            p.rotation = rf(ctx, 360);
            p.gravity = (rf(ctx, 24) + 74.0) * 0.1;
            p.alpha = 0.0;
            p.turning_force = light;
            p.light = Vec3::ZERO;
        }
        6 => {
            p.life_time = rf(ctx, 5) + 24.0;
            p.scale = (rf(ctx, 72) + 72.0) * 0.01 * scale;
            p.rotation = rf(ctx, 360);
            p.gravity = (rf(ctx, 24) + 100.0) * 0.1;
            let f_rand = rf(ctx, 50) * 0.03;
            p.velocity = Vec3::new(2.5 + f_rand, -5.0 - f_rand, 0.0);
            p.alpha = 0.0;
            p.turning_force = light;
            p.light = Vec3::ZERO;
        }
        7 => {
            p.life_time = rf(ctx, 5) + 12.0;
            p.scale = (rf(ctx, 72) + 52.0) * 0.01 * scale;
            p.rotation = rf(ctx, 360);
            p.gravity = (rf(ctx, 14) + 44.0) * 0.1;
            p.alpha = 0.0;
            p.turning_force = light;
            p.light = Vec3::ZERO;
        }
        8 => {
            p.life_time = 8.0;
            p.scale = (rf(ctx, 42) + 12.0) * 0.01 * scale;
            p.rotation = rf(ctx, 360);
            p.gravity = (rf(ctx, 14) + 44.0) * 0.1;
            p.alpha = 0.0;
            p.turning_force = light;
            p.light = Vec3::ZERO;
            if let Some((tp, _)) = target_state(ctx, p.target) {
                p.start_position = tp;
            }
        }
        _ => {}
    }
}

/// Fades in (alpha rises 0.2-0.3 per tick) and fades out below `fade_life` (subtypes 4-8).
fn cursedlich_alpha(p: &mut Particle, ctx: &mut Ctx, fade_life: f32, fade: f32) {
    if p.life_time < fade_life {
        p.alpha -= FPS * fade;
    } else if p.alpha < 1.0 {
        p.alpha += FPS * (rf(ctx, 2) + 2.0) * 0.1;
    } else {
        p.alpha = 1.0;
    }
    if p.alpha < 0.1 {
        p.live = false;
    }
    let lum = p.alpha;
    p.light = p.turning_force * lum;
}

fn step_cursedlich(p: &mut Particle, ctx: &mut Ctx) {
    match p.sub_type {
        0 => {
            p.scale -= (rf(ctx, 20) + 10.0) * 0.001 * FPS;
            p.position.z += p.gravity * 10.0 * FPS;
        }
        1 => {
            p.scale -= (rf(ctx, 5) + 15.0) * 0.0016 * FPS;
            p.position.z += p.gravity * 10.0 * FPS;
            match target_state(ctx, p.target) {
                Some((tp, live)) => {
                    if p.scale < 0.0 || !live {
                        p.live = false;
                    }
                    p.position = p.position - p.start_position + tp;
                    p.start_position = tp;
                }
                None => p.live = false,
            }
        }
        2 => {
            p.scale -= (rf(ctx, 20) + 10.0) * 0.004 * FPS;
            p.position.z += p.gravity * 25.0 * FPS;
        }
        3 => {
            p.scale -= 0.03 * FPS;
            p.position.z += p.gravity * 10.0 * FPS;
            if p.scale < 0.0 {
                p.live = false;
            }
            if let Some((tp, true)) = target_state(ctx, p.target) {
                p.position = p.position - p.start_position + tp;
                p.start_position = tp;
            }
        }
        4 | 9 => {
            cursedlich_alpha(p, ctx, 10.0, 0.2);
            if p.scale > 0.0 {
                p.scale -= (rf(ctx, 3) + 6.0) * 0.01 * FPS;
            } else {
                p.live = false;
            }
            p.position.z += p.gravity * FPS;
            p.rotation += 3.0 * FPS;
            if p.sub_type == 9 {
                p.rotation = 0.0;
                p.position.z += p.gravity * 1.2 * FPS;
            }
        }
        5 => {
            cursedlich_alpha(p, ctx, 15.0, 0.2);
            if p.scale > 0.0 {
                p.scale -= (rf(ctx, 2) + 5.0) * 0.01 * FPS;
            } else {
                p.live = false;
            }
            p.position.z += p.gravity * FPS;
            p.rotation += 3.0 * FPS;
        }
        6 => {
            cursedlich_alpha(p, ctx, 10.0, 0.2);
            if p.scale > 0.0 {
                p.scale -= (rf(ctx, 3) + 6.0) * 0.01 * FPS;
            } else {
                p.live = false;
            }
            p.position.x += p.velocity.x * FPS;
            p.position.y += p.velocity.y * FPS;
            p.position.z += p.gravity * FPS;
            p.rotation += 3.0 * FPS;
        }
        7 => {
            cursedlich_alpha(p, ctx, 10.0, 0.2);
            if p.scale > 0.0 {
                p.scale -= (rf(ctx, 3) + 3.0) * 0.01 * FPS;
            } else {
                p.live = false;
            }
            p.position.z += p.gravity * FPS;
            p.rotation += 3.0 * FPS;
        }
        8 => {
            cursedlich_alpha(p, ctx, 5.0, 0.1);
            p.scale += (rf(ctx, 3) + 8.0) * 0.01 * FPS;
            if p.life_time < 3.0 {
                p.scale += (10.0 - p.life_time) * 0.02 * FPS;
            }
            // Moves 8 units along its local −Y.
            p.position += rotate(p.angle, Vec3::new(0.0, -1.0, 0.0)) * 8.0;
            if let Some((tp, _)) = target_state(ctx, p.target) {
                p.position = p.position - p.start_position + tp;
                p.start_position = tp;
            }
            p.rotation += 5.0 * FPS;
        }
        _ => {}
    }
}

// --- BITMAP_LEAF_TOTEMGOLEM -------------------------------------------------------------------------

fn create_leaf(p: &mut Particle, ctx: &mut Ctx) {
    p.life_time = rf(ctx, 10) + 40.0;
    p.velocity = Vec3::ZERO;
    p.scale = (rf(ctx, 20) + 10.0) * 0.04;
    p.rotation = rf(ctx, 360);
    p.gravity = (rf(ctx, 40) + 30.0) * -0.05;
    p.velocity.x = rf(ctx, 10) - 5.0;
    p.velocity.y = rf(ctx, 10) - 5.0;
    p.velocity.z = rf(ctx, 5) + 10.0;
    p.alpha = 1.0;
}

fn step_leaf(p: &mut Particle, ctx: &mut Ctx) {
    p.velocity.z += p.gravity * FPS;
    let height = ctx.terrain_height(p.position.x, p.position.y) + 20.0;
    if p.position.z <= height {
        p.velocity = Vec3::ZERO;
        p.position.z = height;
        p.alpha -= FPS * 0.05;
        p.light = Vec3::splat(p.alpha);
    } else {
        p.scale += FPS * 0.01;
    }
}

// --- BITMAP_FIRE (+2, +3) ---------------------------------------------------------------------------

fn create_fire(p: &mut Particle, ctx: &mut Ctx) {
    let scale = p.scale;
    match p.sub_type {
        0 => {
            p.life_time = 24.0;
            p.velocity = Vec3::new(0.0, -(rf(ctx, 16) + 32.0) * 0.1, 0.0);
            p.scale = (rf(ctx, 64) + 128.0) * 0.01;
            p.rotation = rf(ctx, 360);
        }
        1 => {
            p.life_time = 24.0;
            p.velocity = Vec3::new(0.0, -(rf(ctx, 16) + 32.0) * 0.1, 0.0);
            p.scale = (rf(ctx, 4) + 10.0) * 0.01;
            p.rotation = rf(ctx, 360);
        }
        17 | 5 => {
            p.life_time = if p.sub_type == 17 { 10.0 } else { 24.0 };
            p.velocity = Vec3::new(0.0, -(rf(ctx, 16) + 32.0) * 0.1, 0.0);
            p.scale = scale * (rf(ctx, 64) + 128.0) * 0.01;
            p.rotation = rf(ctx, 360);
        }
        7 => {
            p.life_time = 24.0;
            p.velocity = Vec3::new(0.0, -(rf(ctx, 16) + 32.0) * 0.1, 0.0);
            p.velocity = Vec3::new(0.0, -(rf(ctx, 32) - 16.0) * 0.1, 0.0);
            p.scale = (rf(ctx, 64) + 128.0) * 0.008 + scale;
            p.rotation = rf(ctx, 360);
        }
        9 => {
            p.life_time = 24.0;
            p.velocity = Vec3::new(0.0, -(rf(ctx, 16) + 32.0) * 0.1, 0.0);
            let range = ctx.rand() % 60 - 30;
            // StartPosition = offset from the target.
            p.start_position = Vec3::new(range as f32, range as f32, 190.0 - range.abs() as f32 * 1.5);
            p.rotation = rf(ctx, 360);
        }
        10 => {
            p.life_time = 24.0;
            p.velocity = Vec3::new(0.0, -(rf(ctx, 16) + 32.0) * 0.1, 0.0);
            p.rotation = rf(ctx, 360);
        }
        11 => {
            p.life_time = 24.0;
            p.velocity = Vec3::new(0.0, -(rf(ctx, 32) - 16.0) * 0.1, 0.0);
            p.scale = (rf(ctx, 64) + 128.0) * 0.008 + scale;
            p.rotation = rf(ctx, 360);
        }
        12 => {
            p.life_time = 24.0;
            p.velocity = Vec3::new(0.0, -(rf(ctx, 16) + 32.0) * 0.1, 0.0);
            p.rotation = rf(ctx, 360);
            p.scale = scale * (rf(ctx, 16) + 150.0) * 0.012;
        }
        13 => {
            p.life_time = 20.0;
            p.velocity = Vec3::new(0.0, 0.0, (rf(ctx, 128) + 128.0) * 0.15);
            p.rotation = rf(ctx, 360);
            p.scale = (rf(ctx, 16) + 150.0) * 0.012;
        }
        14 => {
            p.life_time = 24.0;
            p.scale = 1.5;
            p.tex_type = BITMAP_CLUD64;
            p.velocity = Vec3::ZERO;
            p.rotation = rf(ctx, 360);
        }
        15 => {
            p.life_time = 10.0;
            p.scale = (rf(ctx, 64) + 128.0) * 0.01;
            p.tex_type = BITMAP_CLUD64;
            p.velocity = Vec3::ZERO;
            p.rotation = rf(ctx, 360);
        }
        16 => {
            p.life_time = 4.0;
        }
        18 => {
            p.life_time = 24.0;
            p.velocity = Vec3::new(0.0, -(rf(ctx, 32) - 16.0) * 0.1, 0.0);
            p.rotation = rf(ctx, 360);
            p.tex_type = BITMAP_GROUND_SMOKE;
            p.light = Vec3::ONE;
        }
        _ => {
            p.life_time = 24.0;
            p.velocity = Vec3::new(0.0, -(rf(ctx, 32) - 16.0) * 0.1, 0.0);
            p.rotation = rf(ctx, 360);
        }
    }
}

fn step_fire(p: &mut Particle, ctx: &mut Ctx) {
    match p.sub_type {
        0 => {
            p.gravity += 0.004 * FPS;
            p.scale -= 0.04 * FPS;
            p.frame = frame_of(23.0, p.life_time, 6.0);
            p.position.z += p.gravity * 10.0 * FPS;
        }
        17 | 5 | 6 => {
            p.gravity += 0.004 * FPS;
            p.scale -= 0.04 * FPS;
            p.rotation += 5.0 * FPS;
            p.frame = frame_of(23.0, p.life_time, 6.0);
            p.position.z += p.gravity * 10.0 * FPS;
        }
        7 => {
            p.gravity += 0.004 * FPS;
            p.frame = frame_of(15.0, p.life_time, 6.0);
            p.scale -= 0.04 * FPS;
            p.frame = frame_of(23.0, p.life_time, 6.0);
            p.position.z += p.gravity * 10.0 * FPS;
        }
        8 => {
            p.gravity += 0.004 * FPS;
            p.scale *= 0.95;
            p.rotation += 5.0 * FPS;
            p.frame = frame_of(23.0, p.life_time, 6.0);
            p.position.z += p.gravity * 10.0 * FPS;
        }
        9 => {
            p.gravity += 0.004 * FPS;
            if let Some((tp, _)) = target_state(ctx, p.target) {
                p.position = tp + p.start_position;
            }
            // The original's integer division: adds 0 or 1.
            p.gravity += ((ctx.rand() % 60 + 60) / 100) as f32 * FPS;
            p.scale -= p.gravity * FPS / 90.0;
            p.frame = frame_of(23.0, p.life_time, 6.0);
            p.position.z += p.gravity * 10.0 * FPS;
        }
        11 => {
            p.gravity += 0.004 * FPS;
            if p.life_time > 12.0 {
                p.frame = frame_of(24.0, p.life_time, 3.0);
            }
            p.scale += FPS * 0.04;
            p.frame = frame_of(23.0, p.life_time, 6.0);
            p.position.z += p.gravity * 10.0 * FPS;
        }
        10 => {
            p.gravity += 0.004 * FPS;
            p.scale *= 0.95;
            p.frame = frame_of(23.0, p.life_time, 6.0);
            p.position.z += p.gravity * 10.0 * FPS;
        }
        12 | 13 => {
            p.gravity += 0.004 * FPS;
            let lum = p.life_time / 24.0;
            p.rotation += (rf(ctx, 10) + 10.0) * FPS;
            p.scale -= 0.04 * FPS;
            p.light = Vec3::splat(lum);
            p.frame = frame_of(23.0, p.life_time, 6.0);
            p.position.z += p.gravity * 10.0 * FPS;
        }
        14 => {
            p.gravity += 0.004 * FPS;
            p.rotation += p.gravity * FPS;
            p.scale += FPS * 0.03;
            p.light *= 1.0 / 1.1;
            if p.light.z <= 0.05 || p.scale <= 0.0 {
                p.live = false;
            }
            p.frame = frame_of(23.0, p.life_time, 6.0);
            p.position.z += p.gravity * 10.0 * FPS;
        }
        15 => {
            p.gravity += 0.004 * FPS;
            p.scale -= 0.04 * FPS;
            p.light *= 1.0 / 1.1;
            if p.light.z <= 0.05 || p.scale <= 0.0 {
                p.live = false;
            }
            p.frame = frame_of(23.0, p.life_time, 6.0);
            p.position.z += p.gravity * 10.0 * FPS;
        }
        16 => {
            p.frame = (3.0 - p.life_time) as i32;
            p.light *= 1.0 / 1.7;
            p.scale *= 1.1;
        }
        18 => {
            p.gravity += 0.004 * FPS;
            p.scale += p.gravity * FPS;
            p.velocity *= 0.98;
            p.frame = frame_of(23.0, p.life_time, 6.0);
            p.position.z += p.gravity * 10.0 * FPS;
            p.light *= 0.95;
        }
        _ => {
            p.gravity += 0.004 * FPS;
            p.scale += p.gravity * FPS;
            p.velocity *= 0.98;
            p.frame = frame_of(23.0, p.life_time, 6.0);
            p.position.z += p.gravity * 10.0 * FPS;
        }
    }
}

fn draw_fire(p: &Particle, ctx: &DrawCtx, out: &mut DrawList) {
    let Some(t) = ctx.tex(p.tex_type) else { return };
    let (w, h) = (t.width * p.scale, t.height * p.scale);
    let strip = [(p.frame % 4) as f32 * 0.25, 0.0, 0.25, 1.0];
    match p.sub_type {
        17 | 5 | 7 | 8 | 11 | 12 | 13 => {
            sprite(out, t, p.tex_type, p.position, w * 0.25, h, p.light, p.rotation, strip, None)
        }
        18 => sprite(out, t, p.tex_type, p.position, w, h, p.light, p.rotation, FULL_UV, Some(Blend::Blend3)),
        14 | 15 => sprite(out, t, p.tex_type, p.position, w, h, p.light, p.rotation, FULL_UV, None),
        _ => sprite(out, t, p.tex_type, p.position, w * 0.25, h, p.light, p.angle.x, strip, None),
    }
}

// --- BITMAP_FIRE + 1 --------------------------------------------------------------------------------

fn create_fire1(p: &mut Particle, ctx: &mut Ctx) {
    let scale = p.scale;
    match p.sub_type {
        1 => {
            p.life_time = 3.0;
            p.velocity = Vec3::new(0.0, -(rf(ctx, 8) + 32.0) * 0.3, 0.0);
        }
        2 => {
            p.life_time = 5.0;
            p.velocity = Vec3::new(0.0, -(rf(ctx, 8) + 32.0) * 0.1, 0.0);
        }
        3 => {
            p.life_time = 7.0;
            p.velocity = Vec3::new(0.0, -(rf(ctx, 8) + 32.0) * 0.1, 0.0);
        }
        4 => {
            p.life_time = 100.0;
            p.gravity = (rf(ctx, 100) / 100.0) * 4.0 + 1.0;
            p.velocity.x = rf(ctx, 300) - 150.0;
            p.scale = scale + rf(ctx, 6) * 0.15;
            p.rotation = rf(ctx, 360);
            p.angle = Vec3::ZERO;
            p.start_position = p.position;
        }
        5 => {
            p.life_time = 6.0;
            p.gravity = (rf(ctx, 100) / 100.0) * 4.0 + p.angle.x * 1.2;
            p.scale = scale + rf(ctx, 6) * 0.20;
            p.rotation = rf(ctx, 360);
            p.start_position = p.position;
        }
        6 => {
            p.life_time = 6.0;
            p.gravity = (rf(ctx, 100) / 100.0) * 4.0 + 5.0;
            p.scale = scale + rf(ctx, 6) * 0.10;
            p.rotation = rf(ctx, 360);
            p.start_position = p.position;
        }
        7 => {
            p.life_time = 10.0;
        }
        8 => {
            p.position.x += rf(ctx, 20) - 10.0;
            p.position.y += rf(ctx, 20) - 10.0;
            p.position.z += rf(ctx, 20) - 10.0;
            p.life_time = 32.0;
            p.rotation = rf(ctx, 360);
            p.gravity = (rf(ctx, 5) + 5.0) / 5.0;
        }
        9 => {
            p.tex_type = BITMAP_LIGHT + 2;
            p.life_time = 32.0;
            p.rotation = rf(ctx, 360);
            p.gravity = (rf(ctx, 5) + 5.0) / 10.0;
        }
        0 => {
            p.life_time = 12.0;
            p.velocity = Vec3::new(0.0, -(rf(ctx, 8) + 32.0) * 0.3, 0.0);
            p.scale = 0.8;
        }
        _ => {}
    }
    p.rotation = rf(ctx, 360);
}

fn step_fire1(p: &mut Particle, _ctx: &mut Ctx) {
    match p.sub_type {
        1 => {
            p.velocity *= 1.05;
            p.position.z += p.gravity * 20.0 * FPS;
            p.light = Vec3::splat(p.life_time * 0.2);
        }
        2 => {
            p.light = Vec3::splat(p.life_time * 0.2);
        }
        3 | 0 => {
            p.gravity += 0.02 * FPS;
            p.scale += p.gravity * FPS;
            p.velocity *= 1.05;
            p.position.z += p.gravity * 20.0 * FPS;
            p.light = Vec3::splat(p.life_time * 0.2);
        }
        4 => {
            let count = (p.velocity.x + p.life_time) * 0.1;
            p.position.x = p.start_position.x + count.sin() * 120.0;
            p.position.y = p.start_position.y - count.cos() * 120.0;
            p.position.z += p.gravity * FPS;
            p.scale -= 0.002 * FPS;
        }
        5 => {
            // cos/sin of Angle[2] in radians, as in the original.
            p.position.x += p.angle.z.cos() * 20.0 * FPS;
            p.position.y += p.angle.z.sin() * 20.0 * FPS;
            p.position.z += p.gravity * FPS;
            p.rotation += 1.0 * FPS;
        }
        6 => {
            p.position.z += p.gravity * FPS;
            p.rotation += 4.0 * FPS;
        }
        7 => {
            p.velocity *= 1.05;
            p.light = Vec3::splat(p.life_time * 0.2);
        }
        8 | 9 => {
            p.scale += FPS * 0.13;
            p.rotation += p.gravity * FPS;
            p.light *= 1.0 / 1.13;
        }
        _ => {}
    }
}

// --- BITMAP_FLAME -----------------------------------------------------------------------------------

fn create_flame(p: &mut Particle, ctx: &mut Ctx) {
    let (scale, light) = (p.scale, p.light);
    match p.sub_type {
        0 => {
            p.life_time = 20.0;
            p.velocity = Vec3::new(0.0, 0.0, (rf(ctx, 128) + 128.0) * 0.15);
            p.scale = scale * (rf(ctx, 64) + 64.0) * 0.01;
        }
        6 => {
            p.life_time = 10.0;
            p.velocity = Vec3::new(0.0, 0.0, (rf(ctx, 128) + 256.0) * 0.12);
            p.scale = scale * (rf(ctx, 64) + 64.0) * 0.01;
        }
        8 | 7 => {
            p.life_time = 33.0;
            p.scale = -rf(ctx, 20) / 100.0 + p.scale;
            p.rotation = rf(ctx, 360) - 180.0;
            p.start_position.x = p.position.x;
            p.position.x += (rf(ctx, 2) / 2.0 - 0.0) * p.scale;
            p.gravity = (rf(ctx, 100) / 100.0 + 1.8) * p.scale;
            let mut f_temp = 0.0;
            if ctx.rand() % 10 >= 3 {
                f_temp = (rf(ctx, 20) / 10.0 - 1.0) * p.scale;
            }
            p.velocity = Vec3::new(0.0, f_temp, 0.0);
            p.position.y += p.position.x - p.start_position.x;
        }
        1 => {
            p.life_time = 15.0;
            p.scale += (rf(ctx, 32) + 32.0) * 0.01;
            p.velocity = Vec3::new(0.0, (rf(ctx, 4) + 4.0) * 0.15, 0.0);
        }
        5 => {
            p.life_time = 4.0;
            p.velocity = Vec3::new(0.0, (rf(ctx, 4) + 4.0) * 0.15, 0.0);
        }
        2 => {
            p.life_time = 10.0;
            p.scale += (rf(ctx, 32) + 32.0) * 0.01;
            let mut inter = light.x * rf(ctx, 80) / 100.0;
            p.velocity = Vec3::new(0.0, inter, 0.0);
            // MovePosition: an initial jump in its direction.
            p.position += rotate(p.angle, p.velocity);
            inter = (light.x - inter) / 15.0;
            p.velocity = Vec3::new(0.0, inter, 0.0);
            let lum = (ctx.world_time * 0.002).sin() * 0.3 + 0.7;
            p.light = Vec3::new(lum, lum * 0.5, lum * 0.5);
        }
        3 => {
            p.life_time = 10.0;
            p.scale += (rf(ctx, 32) + 32.0) * 0.01;
            p.velocity = Vec3::ZERO;
            let lum = (ctx.world_time * 0.002).sin() * 0.3 + 0.7;
            p.light = Vec3::new(lum, lum * 0.5, lum * 0.5);
        }
        4 => {
            p.life_time = 5.0;
            p.scale += (rf(ctx, 32) + 32.0) * 0.01;
            p.gravity = 10.0;
            if let Some((tp, _)) = target_state(ctx, p.target) {
                p.start_position = tp;
            }
            let lum = (ctx.world_time * 0.002).sin() * 0.3 + 0.7;
            p.light = Vec3::new(lum, lum * 0.5, lum * 0.5);
        }
        9 => {
            p.life_time = 40.0;
            p.scale = -rf(ctx, 20) / 100.0 + p.scale;
            p.rotation = rf(ctx, 360) - 180.0;
            p.start_position.x = p.position.x;
            p.position.x += (rf(ctx, 2) / 2.0 - 0.0) * p.scale;
            p.gravity = (rf(ctx, 100) / 100.0 + 1.8) * p.scale + 2.0;
            let mut f_temp = 0.0;
            if ctx.rand() % 10 >= 3 {
                f_temp = (rf(ctx, 20) / 10.0 - 1.0) * p.scale;
            }
            p.velocity = Vec3::new(0.0, f_temp * 2.0, 0.0);
            p.position.y += p.position.x - p.start_position.x;
        }
        10 => {
            p.life_time = 20.0 + rf(ctx, 5);
            p.rotation = 0.0;
            p.scale = scale * 0.5 + rf(ctx, 10) * 0.02;
            p.velocity.x = (rf(ctx, 10) + 5.0) * 0.4;
            p.velocity.y = (rf(ctx, 10) - 5.0) * 0.4;
            p.velocity.z = (rf(ctx, 10) + 5.0) * 0.2;
        }
        11 => {
            p.life_time = 20.0;
            p.rotation = rf(ctx, 360);
            p.tex_type = BITMAP_FIRE_HIK2_MONO;
            p.velocity = Vec3::new(0.0, 0.0, (rf(ctx, 128) + 128.0) * 0.15);
            p.scale = scale * (rf(ctx, 64) + 64.0) * 0.01;
        }
        12 => {
            p.life_time = 10.0;
            p.velocity = Vec3::new(0.0, 0.0, (rf(ctx, 128) + 128.0) * 0.15);
            p.scale = scale * (rf(ctx, 64) + 64.0) * 0.01;
        }
        _ => {}
    }
}

fn step_flame(p: &mut Particle, ctx: &mut Ctx) {
    if p.life_time <= 0.0 {
        p.live = false;
        return;
    }
    match p.sub_type {
        1 => {
            if p.life_time as i32 == 10 {
                p.velocity.y += 32.0 * 0.2 * FPS;
                p.scale -= 0.15 * FPS;
            }
            p.rotation = rf(ctx, 360);
            p.light -= Vec3::splat(0.05 * FPS);
        }
        5 => {
            if p.life_time as i32 == 10 {
                p.velocity.y += 32.0 * 0.2 * FPS;
                p.scale *= 0.8;
            }
            p.rotation = rf(ctx, 360);
            p.light -= Vec3::splat(0.05 * FPS);
        }
        2 => {
            p.rotation = rf(ctx, 360);
            p.light -= Vec3::splat(0.05 * FPS);
        }
        3 => {
            p.rotation = rf(ctx, 360);
        }
        4 => {
            if let Some((tp, _)) = target_state(ctx, p.target) {
                p.position.x += (tp.x - p.start_position.x) * FPS;
                p.position.y += (tp.y - p.start_position.y) * FPS;
                p.position.z += tp.z - p.start_position.z + p.gravity * FPS;
                p.start_position = tp;
            } else {
                // No target: only the rise from gravity.
                p.position.z += p.gravity * FPS;
            }
            p.gravity += 0.1 * FPS;
            p.rotation = rf(ctx, 360);
            p.light -= Vec3::splat(0.05 * FPS);
        }
        8 => {
            if p.start_position.x < p.position.x {
                p.rotation += 2.0 * FPS;
            } else {
                p.rotation -= 2.0 * FPS;
            }
            p.position.z += (p.gravity / 2.0) * FPS;
            p.scale -= p.gravity * FPS / 95.0;
            if p.scale <= 0.0 {
                p.live = false;
            }
            p.light *= 1.0 / 1.007;
        }
        7 => {
            if p.start_position.x < p.position.x {
                p.rotation += 2.0 * FPS;
            } else {
                p.rotation -= 2.0 * FPS;
            }
            p.position.z += p.gravity * FPS;
            p.scale -= p.gravity * FPS / 95.0;
            if p.scale <= 0.0 {
                p.live = false;
            }
            p.light *= 1.0 / 1.007;
        }
        9 => {
            if p.start_position.x < p.position.x {
                p.rotation += 0.5 * FPS;
            } else {
                p.rotation -= 0.5 * FPS;
            }
            p.position.z += p.gravity * 1.2 * FPS;
            p.scale -= p.gravity * FPS / 98.0;
            if p.scale <= 0.0 {
                p.live = false;
            }
            p.light *= 1.0 / 1.008;
        }
        10 => {
            // VectorAddScaled: extra unrotated advance.
            p.position += p.velocity * FPS;
            p.velocity.x *= 0.95;
            p.velocity.y *= 0.95;
            p.light = Vec3::splat(p.life_time / 10.0);
            p.velocity.z += 0.3 * FPS;
            p.scale += FPS * 0.07;
        }
        11 => {
            p.light *= 1.0 / 1.008;
        }
        _ => {}
    }
}

// --- BITMAP_FIRE_RED --------------------------------------------------------------------------------

fn create_fire_red(p: &mut Particle, ctx: &mut Ctx) {
    let scale = p.scale;
    p.life_time = 20.0;
    p.velocity = Vec3::new(0.0, 0.0, (rf(ctx, 100) + 100.0) * 0.15);
    p.scale = scale * (rf(ctx, 64) + 64.0) * 0.01;
}

fn step_fire_red(p: &mut Particle, _ctx: &mut Ctx) {
    if p.life_time <= 0.0 {
        p.live = false;
    }
}

// --- BITMAP_RAIN_CIRCLE (+1) ------------------------------------------------------------------------

fn create_rain_circle(p: &mut Particle, ctx: &mut Ctx) {
    let scale = p.scale;
    p.life_time = 20.0;
    p.scale = (rf(ctx, 6) + 8.0) * 0.1;
    if p.ty == BITMAP_RAIN_CIRCLE + 1 || p.sub_type == 1 {
        p.position.x += (rf(ctx, 10) - 5.0) * 1.0;
        p.position.y += (rf(ctx, 10) - 5.0) * 1.0;
        p.position.z += (rf(ctx, 10) - 5.0) * 1.0;
    }
    if p.sub_type == 1 {
        p.scale = scale;
        p.gravity = rf(ctx, 100) / 1000.0;
        p.velocity.y = 1.0;
    }
}

fn step_rain_circle(p: &mut Particle, ctx: &mut Ctx) {
    if p.life_time <= 0.0 {
        p.live = false;
    }
    if p.sub_type == 1 {
        p.scale += p.gravity * FPS;
    } else {
        p.scale += FPS * 0.03;
    }
    if ctx.world_active == WD_34CRYWOLF_1ST {
        p.light = Vec3::new(1.0, 1.0, 0.7);
    } else if p.sub_type == 2 {
        p.light -= Vec3::splat(0.03) * FPS;
    } else {
        p.light -= Vec3::splat(0.05) * FPS;
    }
}
