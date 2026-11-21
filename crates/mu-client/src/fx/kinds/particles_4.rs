//! Partículas de `ZzzEffectParticle.cpp` (familia 4): chispas de explosión, explosiones animadas,
//! brillos de arma (`BITMAP_SHINY*`, `BITMAP_FLARE_RED`), flores de cerezo, nieve, sangre,
//! fuegos artificiales, fuerza de espada y nubes (`BITMAP_CLOUD`, todos sus subtipos).

use std::f32::consts::PI;

use bevy::math::Vec3;

use crate::fx::core::*;

const SPARK_2: i32 = BITMAP_SPARK + 2;
const EXPLOTION_1: i32 = BITMAP_EXPLOTION + 1;
const SHINY_1: i32 = BITMAP_SHINY + 1;
const SHINY_2: i32 = BITMAP_SHINY + 2;
const SHINY_4: i32 = BITMAP_SHINY + 4;
const SHINY_6: i32 = BITMAP_SHINY + 6;
const BLOOD_1: i32 = BITMAP_BLOOD + 1;

/// `Q_PI / 180`.
const DEG: f32 = PI / 180.0;

pub fn register(r: &mut Registry) {
    let k = |create: ParticleCreate, step: ParticleStep, draw: Option<ParticleDraw>| ParticleKind {
        create: Some(create),
        step: Some(step),
        draw,
    };
    r.particle(SPARK_2, k(create_spark2, step_spark2, Some(draw_spark2)));
    r.particle(BITMAP_EXPLOTION_MONO, k(create_explotion, step_explotion, Some(draw_explotion)));
    r.particle(BITMAP_EXPLOTION, k(create_explotion, step_explotion, Some(draw_explotion)));
    r.particle(BITMAP_SUMMON_SAHAMUTT_EXPLOSION, k(create_sahamutt, step_sahamutt, Some(draw_explotion)));
    r.particle(BITMAP_SPOT_WATER, k(create_spot_water, step_spot_water, Some(draw_spot_water)));
    r.particle(BITMAP_FLARE_RED, k(create_flare_red, step_flare_red, None));
    r.particle(EXPLOTION_1, k(create_explotion1, step_explotion1, Some(draw_explotion1)));
    r.particle(BITMAP_SHINY, k(create_shiny, step_shiny, None));
    r.particle(BITMAP_CHERRYBLOSSOM_EVENT_PETAL, k(create_petal, step_petal, None));
    r.particle(BITMAP_CHERRYBLOSSOM_EVENT_FLOWER, k(create_flower, step_flower, None));
    r.particle(SHINY_1, k(create_shiny1, step_shiny1, None));
    r.particle(SHINY_2, k(create_shiny2, step_shiny2, None));
    r.particle(SHINY_4, k(create_shiny4, step_shiny4, None));
    r.particle(SHINY_6, k(create_shiny6, step_shiny6, Some(draw_shiny6)));
    r.particle(BITMAP_PIN_LIGHT, k(create_pin_light, step_pin_light, None));
    r.particle(BITMAP_ORORA, k(create_orora, step_orora, None));
    r.particle(BITMAP_SNOW_EFFECT_1, k(create_snow, step_snow, None));
    r.particle(BITMAP_SNOW_EFFECT_2, k(create_snow, step_snow, None));
    r.particle(BITMAP_DS_EFFECT, k(create_ds_effect, step_ds_effect, None));
    r.particle(BITMAP_BLOOD, k(create_blood, step_blood, None));
    r.particle(BLOOD_1, k(create_blood, step_blood, Some(draw_blood1)));
    r.particle(BITMAP_FIRECRACKER, k(create_firecracker, step_firecracker, Some(draw_firecracker)));
    r.particle(BITMAP_SWORD_FORCE, k(create_sword_force, step_sword_force, None));
    r.particle(BITMAP_CLOUD, k(create_cloud, step_cloud, Some(draw_cloud)));
}

// --- Utilidades ------------------------------------------------------------------------------------

/// `rand() % n` como float.
fn rf(ctx: &mut Ctx, n: i32) -> f32 {
    (ctx.rand() % n) as f32
}

/// `VectorRotate(v, Matrix, v)` con entrada y salida iguales: el original reutiliza las
/// componentes ya giradas (aliasing), así que se calcula componente a componente.
fn rotate_in_place(angle: Vec3, v: Vec3) -> Vec3 {
    let m = angle_matrix(angle);
    let mut o = v;
    o.x = m.row(0).dot(o);
    o.y = m.row(1).dot(o);
    o.z = m.row(2).dot(o);
    o
}

/// `HandPosition`: coloca la partícula en el hueso del arma de su dueño.
fn hand_position(p: &mut Particle, ctx: &Ctx) {
    let offset = if p.ty == BITMAP_FLARE_RED || p.ty == SHINY_2 { Vec3::new(0.0, -120.0, 0.0) } else { Vec3::ZERO };
    if let Some(a) = ctx.actor(p.target) {
        // TODO(original): usa `Hero->Weapon[SubType % 2].LinkBone` (mano derecha o izquierda);
        // aquí solo hay un hueso de arma.
        p.position = a.bone_world(a.weapon_bone, offset);
    }
}

/// `CreateBomb(p, Exp, 0)` de `ZzzEffect.cpp` (solo el subtipo 0, el único que se usa aquí).
fn create_bomb(ctx: &mut Ctx, p: Vec3, exp: bool) {
    let mut position = p;
    position.z += 80.0;
    let mut angle = Vec3::ZERO;
    let light = Vec3::ONE;
    for _ in 0..20 {
        let x = (ctx.rand() % 60 + 60 + 90) as f32;
        let z = rf(ctx, 30);
        angle = Vec3::new(x, 0.0, z);
        ctx.create_particle(BITMAP_SPARK, position, angle, light, 2, 1.0, None);
    }
    if exp {
        ctx.create_particle(BITMAP_EXPLOTION, position, angle, Vec3::splat(0.7), 0, 1.0, None);
    }
}

/// Un sprite de `RenderSprite`: alfa 1 si la textura es JPG (o `BITMAP_BLOOD + 1`), si no `Light[0]`.
#[allow(clippy::too_many_arguments)]
fn sprite(out: &mut DrawList, ctx: &DrawCtx, tex: i32, pos: Vec3, w: f32, h: f32, light: Vec3, rot: f32, uv: [f32; 4], blend: Blend) {
    let Some(t) = ctx.tex(tex) else { return };
    let alpha = if t.components == 3 || tex == BLOOD_1 { 1.0 } else { light.x };
    out.sprites.push(SpriteDraw {
        texture: tex,
        position: pos,
        width: w,
        height: h,
        color: [light.x, light.y, light.z, alpha],
        rotation: rot,
        uv,
        blend,
    });
}

/// Width/Height y mezcla de `RenderParticles` (tamaño de `TexType` × escala).
fn size(p: &Particle, ctx: &DrawCtx) -> Option<(f32, f32, Blend)> {
    let t = ctx.tex(p.tex_type)?;
    Some((t.width * p.scale, t.height * p.scale, default_blend(t)))
}

const FULL_UV: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

// --- BITMAP_SPARK + 2 ------------------------------------------------------------------------------

fn create_spark2(p: &mut Particle, ctx: &mut Ctx) {
    let scale = p.scale;
    match p.sub_type {
        0 => {
            p.life_time = 16.0;
            p.scale = scale + (ctx.rand() % 10 - 5) as f32 * 0.03;
            p.position.x += (ctx.rand() % 10 - 5) as f32;
            p.position.y += (ctx.rand() % 10 - 5) as f32;
            p.position.z += (ctx.rand() % 10 - 5) as f32;
        }
        1 => p.life_time = 12.0,
        2 | 3 => {
            p.scale = 0.4;
            p.life_time = 16.0;
        }
        _ => {}
    }
}

fn step_spark2(p: &mut Particle, ctx: &mut Ctx) {
    if p.sub_type == 0 {
        p.frame = ((16.0 - p.life_time) / 4.0) as i32;
    } else if p.sub_type == 1 {
        if (p.life_time as i32) % 3 == 0 {
            let mut v = p.position;
            v.x += (ctx.rand() % 100 - 50) as f32;
            v.y += (ctx.rand() % 100 - 50) as f32;
            v.z += (ctx.rand() % 100 - 50) as f32;
            create_bomb(ctx, v, true);
        }
    } else if (p.sub_type == 2 || p.sub_type == 3) && p.target.is_some() {
        p.frame = ((16.0 - p.life_time) / 4.0) as i32;
        // Mano izquierda (37) o derecha (28) del jugador.
        let bone = if p.sub_type == 2 { 37 } else { 28 };
        if let Some(a) = ctx.actor(p.target) {
            p.position = a.bone_world(bone, Vec3::ZERO);
        }
    }
}

fn draw_spark2(p: &Particle, ctx: &DrawCtx, out: &mut DrawList) {
    if p.sub_type == 0 || p.sub_type == 2 || p.sub_type == 3 {
        let Some((w, h, blend)) = size(p, ctx) else { return };
        let uv = [(p.frame % 2) as f32 * 0.5, (p.frame / 2) as f32 * 0.5, 0.5, 0.5];
        sprite(out, ctx, p.tex_type, p.position, w, h, p.light, 0.0, uv, blend);
    }
}

// --- BITMAP_EXPLOTION_MONO / BITMAP_EXPLOTION ------------------------------------------------------

fn create_explotion(p: &mut Particle, _ctx: &mut Ctx) {
    p.life_time = 20.0;
    if p.sub_type == 2 {
        p.life_time = 30.0;
    }
    // TODO(original): PlayBuffer(SOUND_MOONRABBIT_EXPLOSION) si SubType == 10; si no,
    // PlayBuffer(SOUND_EXPLOTION01) (salvo en la escena de login).
}

/// Anillo de 18 humos + explosiones a `dist` del centro (subtipo 2).
fn explotion_ring(p: &Particle, ctx: &mut Ctx, dist: f32, smoke_light: Vec3, exp_light: Vec3) {
    for j in 0..18 {
        let pos = rotate(Vec3::new(0.0, 0.0, j as f32 * 20.0), Vec3::new(0.0, dist, 0.0)) + p.position;
        ctx.create_particle(BITMAP_SMOKE, pos, p.angle, smoke_light, 35, 2.5, None);
        ctx.create_particle(BITMAP_EXPLOTION, pos, p.angle, exp_light, 1, 1.0, None);
    }
}

fn step_explotion(p: &mut Particle, ctx: &mut Ctx) {
    p.frame = ((20.0 - p.life_time) / 2.0) as i32;
    if p.sub_type == 2 {
        let life = if p.life_time > 10.0 { p.life_time - 10.0 } else { 1.0 };
        p.frame = ((20.0 - life) / 2.0) as i32;
        let lt = p.life_time as i32;
        if lt == 20 {
            p.life_time = 19.9;
            explotion_ring(p, ctx, 200.0, Vec3::ONE, Vec3::ONE);
        } else if lt == 10 {
            p.life_time = 9.9;
            explotion_ring(p, ctx, 400.0, Vec3::splat(0.6), Vec3::splat(0.6));
        } else if lt == 1 {
            p.life_time = 0.99;
            explotion_ring(p, ctx, 600.0, Vec3::splat(0.3), Vec3::splat(0.3));
        }
    }
    if p.sub_type != 1 {
        let l = p.life_time / 20.0;
        ctx.add_terrain_light(p.position.x, p.position.y, Vec3::new(l * 0.5, l * 0.3, l * 0.1), 4.0);
    }
}

/// Atlas 4×4 (`BITMAP_EXPLOTION*`, `BITMAP_SUMMON_SAHAMUTT_EXPLOSION`).
fn draw_explotion(p: &Particle, ctx: &DrawCtx, out: &mut DrawList) {
    let Some((w, h, blend)) = size(p, ctx) else { return };
    let uv = [
        (p.frame % 4) as f32 * 0.25 + 0.005,
        (p.frame / 4) as f32 * 0.25 + 0.005,
        0.25 - 0.01,
        0.25 - 0.01,
    ];
    sprite(out, ctx, p.tex_type, p.position, w, h, p.light, 0.0, uv, blend);
}

// --- BITMAP_SUMMON_SAHAMUTT_EXPLOSION --------------------------------------------------------------

fn create_sahamutt(p: &mut Particle, _ctx: &mut Ctx) {
    p.life_time = 16.0;
}

fn step_sahamutt(p: &mut Particle, _ctx: &mut Ctx) {
    p.frame = (16.0 - p.life_time) as i32;
}

// --- BITMAP_SPOT_WATER -----------------------------------------------------------------------------

fn create_spot_water(p: &mut Particle, ctx: &mut Ctx) {
    p.life_time = 32.0;
    p.scale = (ctx.rand() % 170 + 1) as f32 * 0.01;
}

fn step_spot_water(p: &mut Particle, _ctx: &mut Ctx) {
    p.frame = ((32.0 - p.life_time) / 4.0) as i32;
}

fn draw_spot_water(p: &Particle, ctx: &DrawCtx, out: &mut DrawList) {
    let Some((w, h, blend)) = size(p, ctx) else { return };
    let uv = [0.0, (p.frame % 8) as f32 * 0.125, 1.0, 0.125];
    sprite(out, ctx, p.tex_type, p.position, w, h * 0.125, p.light, p.angle.x, uv, blend);
}

// --- BITMAP_FLARE_RED ------------------------------------------------------------------------------

fn create_flare_red(p: &mut Particle, ctx: &mut Ctx) {
    p.life_time = 18.0;
    hand_position(p, ctx);
}

fn step_flare_red(p: &mut Particle, ctx: &mut Ctx) {
    p.scale = (p.life_time * 10.0 * DEG).sin() * 3.0;
    hand_position(p, ctx);
}

// --- BITMAP_EXPLOTION + 1 --------------------------------------------------------------------------

fn create_explotion1(p: &mut Particle, _ctx: &mut Ctx) {
    p.life_time = 12.0;
    // TODO(original): PlayBuffer(SOUND_EXPLOTION01).
}

fn step_explotion1(p: &mut Particle, _ctx: &mut Ctx) {
    p.frame = ((12.0 - p.life_time) / 3.0) as i32;
}

fn draw_explotion1(p: &Particle, ctx: &DrawCtx, out: &mut DrawList) {
    let Some((w, h, blend)) = size(p, ctx) else { return };
    let uv = [(p.frame % 4) as f32 * 0.25, 0.0, 0.25, 1.0];
    sprite(out, ctx, p.tex_type, p.position, w * 0.25, h, p.light, p.angle.x, uv, blend);
}

// --- BITMAP_SHINY ----------------------------------------------------------------------------------

fn create_shiny(p: &mut Particle, ctx: &mut Ctx) {
    let scale = p.scale;
    p.life_time = 18.0;
    p.angle.x = 45.0;
    match p.sub_type {
        2 => {
            p.life_time = 25.0;
            p.rotation = rf(ctx, 360);
            p.velocity = Vec3::new(-0.1, -0.5, -1.0);
        }
        3 => {
            p.alpha = 1.0;
            p.velocity.x = -4.0 + rf(ctx, 8);
            p.velocity.y = rf(ctx, 4);
            p.velocity.z = rf(ctx, 4);
            p.life_time = 30.0;
            p.scale = rf(ctx, 10) / 20.0 + 0.5;
        }
        4 => {
            p.scale = scale + rf(ctx, 10) * 0.02;
            p.life_time = (ctx.rand() % 10 + 15) as f32;
            p.angle.z = rf(ctx, 360);
            p.gravity = (ctx.rand() % 10 + 10) as f32;
            let x = (ctx.rand() % 40 - 20) as f32 * 0.1;
            let y = (ctx.rand() % 40 - 20) as f32 * 0.1;
            p.velocity = rotate(p.angle, Vec3::new(x, y, 0.0) * 2.0);
        }
        5 => {
            p.life_time = (ctx.rand() % 10 + 10) as f32;
            p.scale = scale + rf(ctx, 10) * 0.02;
            p.position.x += (ctx.rand() % 10 - 5) as f32;
            p.position.y += (ctx.rand() % 10 - 5) as f32;
            p.gravity = -rf(ctx, 10) * 0.3;
        }
        6 | 9 => {
            p.scale = rf(ctx, 5) / 10.0 + 0.5;
            p.start_position.x = p.scale;
            p.life_time = (ctx.rand() % 10 + 60) as f32;
            p.gravity = 20.0;
            p.angle.x = rf(ctx, 360);
            p.angle.y = rf(ctx, 360);
            p.angle.z = rf(ctx, 360);
            p.velocity = rotate(p.angle, Vec3::ONE * 8.0);
            p.rotation = (ctx.rand() % 20 - 10) as f32;
        }
        7 => {
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
            p.velocity = rotate(p.angle, Vec3::splat(0.5) * 0.8);
            p.rotation = (ctx.rand() % 20 - 10) as f32;
        }
        8 => {
            p.scale = rf(ctx, 5) / 10.0 + 0.5;
            p.start_position.x = p.scale;
            p.life_time = (ctx.rand() % 10 + 40) as f32;
            p.gravity = 20.0;
            p.velocity = Vec3::new(0.0, 0.0, 1.0);
            p.rotation = (ctx.rand() % 20 - 10) as f32;
        }
        _ => {}
    }
}

/// Parte común de los subtipos 6, 8 y 9: se apaga, parpadea y cae.
fn shiny_fade_fall(p: &mut Particle, ctx: &mut Ctx) {
    p.light *= 1.0 / 1.01;
    if p.light.x < 0.2 {
        p.live = false;
    }
    p.start_position.x *= 1.0 / 1.01;
    if p.life_time < 60.0 && ctx.rand_fps_check(2) {
        p.scale = 0.0;
    } else {
        p.scale = p.start_position.x;
    }
}

fn step_shiny(p: &mut Particle, ctx: &mut Ctx) {
    match p.sub_type {
        2 => {
            p.rotation -= 12.0;
            p.velocity.z -= rf(ctx, 8) * 0.05;
            p.position += p.velocity;
            p.light *= 1.0 / 1.05;
            ctx.create_sprite(BITMAP_LIGHT, p.position, p.scale / 1.5, p.light, 0.0, 0);
        }
        3 => {
            p.light *= 1.0 / 1.05;
            p.scale -= 0.04;
            p.alpha -= 0.001;
            if p.alpha <= 0.0 {
                p.live = false;
            }
        }
        4 => {
            p.rotation += 20.0;
            p.position.z += p.gravity * 0.5;
            p.light *= 1.0 / 1.02;
            p.gravity -= 1.5;
            p.scale -= 0.01;
            p.position += p.velocity;
        }
        5 => {
            p.rotation += 10.0;
            p.position.z += p.gravity;
            p.light *= 1.0 / 1.02;
            p.scale -= 0.01;
        }
        6 | 8 => {
            shiny_fade_fall(p, ctx);
            if p.life_time < 60.0 {
                p.velocity *= 1.0 / 1.04;
                p.position.z -= p.gravity * 0.1;
                p.gravity += 0.5;
            }
        }
        7 => {
            p.scale = (p.life_time * 2.0 * DEG).sin();
        }
        9 => {
            shiny_fade_fall(p, ctx);
            if p.life_time < 60.0 {
                p.velocity *= 1.0 / 1.04;
                p.position.z -= p.gravity * 0.1;
                p.gravity += 0.5;
                if ctx.rand_fps_check(5) {
                    let x = p.position.x + (ctx.rand() % 40) as f32 - 20.0;
                    let y = p.position.y + (ctx.rand() % 40) as f32 - 20.0;
                    let pos = Vec3::new(x, y, p.position.z);
                    let r = 0.8 + rf(ctx, 200) * 0.001;
                    let g = 0.5 + rf(ctx, 200) * 0.001;
                    let b = 0.1 + rf(ctx, 100) * 0.001;
                    ctx.create_particle(BITMAP_SHINY, pos, p.angle, Vec3::new(r, g, b), 8, 1.0, None);
                }
            }
        }
        _ => {
            p.scale = (p.life_time * 10.0 * DEG).sin();
            if p.sub_type == 1 {
                p.scale *= 0.75;
                p.rotation -= 12.0;
            }
        }
    }
}

// --- BITMAP_CHERRYBLOSSOM_EVENT_PETAL --------------------------------------------------------------

fn create_petal(p: &mut Particle, ctx: &mut Ctx) {
    if p.sub_type == 0 {
        p.alpha = 1.0;
        p.life_time = 30.0;
        p.scale = rf(ctx, 10) / 20.0 + 0.5;
        p.rotation = rf(ctx, 360);
        p.gravity = -((ctx.rand() % 10 + 10) as f32);
        p.angle = Vec3::ZERO;
        p.velocity.z = -4.0 + rf(ctx, 8);
        p.velocity.y = rf(ctx, 4);
        p.velocity.x = rf(ctx, 4);
    } else if p.sub_type == 1 {
        p.alpha = 1.0;
        p.life_time = 70.0;
        p.velocity.x = -2.0 + rf(ctx, 6);
        p.velocity.y = rf(ctx, 4);
        p.velocity.z = rf(ctx, 4);
        p.rotation = rf(ctx, 360);
    }
}

fn step_petal(p: &mut Particle, ctx: &mut Ctx) {
    if p.sub_type == 0 {
        p.rotation = rf(ctx, 360);
        p.scale = (p.life_time * DEG).sin();
        p.alpha -= 0.001;
        if p.alpha <= 0.0 {
            p.live = false;
        }
    } else if p.sub_type == 1 {
        if p.life_time as i32 == 69 {
            p.velocity -= Vec3::ONE;
        }
        p.light *= 1.0 / 1.01;
        p.alpha -= 0.001;
        if p.alpha <= 0.0 {
            p.live = false;
        }
    }
}

// --- BITMAP_CHERRYBLOSSOM_EVENT_FLOWER -------------------------------------------------------------

fn create_flower(p: &mut Particle, ctx: &mut Ctx) {
    if p.sub_type == 0 {
        // vSpeed: el original lo calcula y lo copia a Velocity, pero luego lo pisa el giro de abajo
        // (se consumen igual los rand()).
        let _ = (ctx.rand(), ctx.rand(), ctx.rand());
        p.alpha = 1.0;
        p.life_time = (ctx.rand() % 30 + 20) as f32;
        p.angle.z = rf(ctx, 360);
        p.rotation = rf(ctx, 360);
        p.gravity = 20.0;
        let x = (ctx.rand() % 40 - 20) as f32 * 0.1;
        let y = (ctx.rand() % 40 - 20) as f32 * 0.1;
        p.velocity = rotate(p.angle, Vec3::new(x, y, 0.0) * 2.5);
    }
}

fn step_flower(p: &mut Particle, ctx: &mut Ctx) {
    if p.sub_type == 0 {
        p.scale = (p.life_time * DEG).sin();
        p.alpha -= 0.002;
        if p.alpha <= 0.0 {
            p.live = false;
        }
        p.position.z += p.gravity * 0.5;
        p.gravity -= 1.5;
        p.rotation = rf(ctx, 360);
        let height = ctx.terrain_height(p.position.x, p.position.y);
        if p.position.z < height + 3.0 {
            p.position.z = height + 3.0;
            p.gravity = -p.gravity * 0.3;
            p.life_time -= 2.0;
        }
    }
}

// --- BITMAP_SHINY + 1 ------------------------------------------------------------------------------

fn create_shiny1(p: &mut Particle, ctx: &mut Ctx) {
    if p.sub_type == 5 {
        p.life_time = 14.0;
        p.rotation = rf(ctx, 360);
        p.gravity = 0.0;
        p.velocity = Vec3::ZERO;
    } else {
        p.life_time = 36.0;
        p.angle.x = 45.0;
        if p.sub_type != 99 {
            hand_position(p, ctx);
        }
    }
}

fn step_shiny1(p: &mut Particle, ctx: &mut Ctx) {
    if p.sub_type == 99 {
        if let Some(tp) = ctx.target_position(p.target) {
            p.position = tp;
        }
        p.position.z += 50.0;
        p.rotation -= 1.0;
    } else if p.sub_type == 5 {
        if let Some(a) = ctx.actor(p.target) {
            p.angle = a.angle;
        }
        p.scale -= 0.06;
        p.position.z += p.gravity * 10.0;
    } else {
        p.scale = (p.life_time * 5.0 * DEG).sin() * 5.0;
        if p.sub_type >= 2 {
            p.scale *= 0.75;
            p.rotation -= 12.0;
        }
        hand_position(p, ctx);
    }
}

// --- BITMAP_SHINY + 2 ------------------------------------------------------------------------------

fn create_shiny2(p: &mut Particle, ctx: &mut Ctx) {
    p.life_time = 18.0;
    hand_position(p, ctx);
}

fn step_shiny2(p: &mut Particle, ctx: &mut Ctx) {
    p.scale = (p.life_time * 10.0 * DEG).sin() * 3.0;
    hand_position(p, ctx);
}

// --- BITMAP_SHINY + 4 ------------------------------------------------------------------------------

fn create_shiny4(p: &mut Particle, ctx: &mut Ctx) {
    p.life_time = 20.0;
    if p.sub_type == 1 {
        p.life_time = 15.0;
        p.gravity = 0.0;
        p.rotation = rf(ctx, 360);
    } else if p.sub_type == 2 {
        p.life_time = 8.0;
    }
}

fn step_shiny4(p: &mut Particle, _ctx: &mut Ctx) {
    match p.sub_type {
        0 => {
            p.scale = (p.life_time * 10.0 * DEG).sin() * 3.0 + 2.0;
            let l = p.life_time / 5.0;
            p.light = Vec3::splat(l);
        }
        1 => {
            p.scale = (p.gravity * DEG).sin() * 3.0 + 1.5;
            p.gravity += (15.0 - p.life_time) * 6.0;
            if p.gravity > 90.0 {
                p.gravity = 90.0;
            }
            if p.life_time < 6.0 {
                p.light *= 0.5;
            }
        }
        2 => {
            p.scale *= 1.1;
            p.light *= 0.9;
        }
        _ => {}
    }
}

// --- BITMAP_SHINY + 6 ------------------------------------------------------------------------------

fn create_shiny6(p: &mut Particle, ctx: &mut Ctx) {
    if p.sub_type == 0 {
        p.alpha = 1.0;
        p.position.x += -5.0 + rf(ctx, 10);
        p.position.y += -5.0 + rf(ctx, 10);
        p.position.z += -5.0 + rf(ctx, 10);
        p.life_time = 30.0;
        p.gravity = 1.3;
        p.scale += rf(ctx, 3) / 10.0;
    } else if p.sub_type == 1 {
        p.alpha = 1.0;
        p.life_time = (ctx.rand() % 3 + 10) as f32;
        p.scale = 0.3;
        p.position.x += (ctx.rand() % 50 - 25) as f32;
        p.position.y += (ctx.rand() % 50 - 25) as f32;
        p.position.z += (ctx.rand() % 50 - 25) as f32;
        p.rotation = (ctx.rand() % 20 - 10) as f32;
    }
}

/// `fLight = rand() % 10 / 100 - 0.05` sumado a los tres canales.
fn flicker_light(p: &mut Particle, ctx: &mut Ctx) {
    let f = rf(ctx, 10) / 100.0 - 0.05;
    p.light += Vec3::splat(f);
}

fn step_shiny6(p: &mut Particle, ctx: &mut Ctx) {
    if p.sub_type == 0 {
        p.rotation += 5.0;
        p.scale -= 0.02;
        p.alpha -= 0.001;
        if p.alpha <= 0.0 {
            p.live = false;
        }
        p.position.z -= p.gravity;
        flicker_light(p, ctx);
    } else if p.sub_type == 1 {
        p.scale -= 0.01;
        p.rotation -= 5.0;
    }
}

/// Su textura y encima `BITMAP_LIGHT` del mismo tamaño (con la mezcla de la primera).
fn draw_shiny6(p: &Particle, ctx: &DrawCtx, out: &mut DrawList) {
    let Some((w, h, blend)) = size(p, ctx) else { return };
    sprite(out, ctx, p.tex_type, p.position, w, h, p.light, p.rotation, FULL_UV, blend);
    sprite(out, ctx, BITMAP_LIGHT, p.position, w, h, p.light, p.rotation, FULL_UV, blend);
}

// --- BITMAP_PIN_LIGHT ------------------------------------------------------------------------------

fn create_pin_light(p: &mut Particle, ctx: &mut Ctx) {
    p.alpha = 1.0;
    p.position.x += -5.0 + rf(ctx, 10);
    p.position.y += -5.0 + rf(ctx, 10);
    p.position.z += -5.0 + rf(ctx, 10);
    p.life_time = 30.0;
    p.gravity = 10.0;
    p.scale += rf(ctx, 5) / 10.0;
    if p.sub_type == 2 {
        p.gravity = -5.0;
    } else if p.sub_type == 3 {
        p.gravity = -5.0;
        p.tex_type = SHINY_6;
        p.rotation = rf(ctx, 360);
    }
}

fn step_pin_light(p: &mut Particle, ctx: &mut Ctx) {
    p.scale -= 0.02;
    p.alpha -= 0.001;
    if p.alpha <= 0.0 {
        p.live = false;
    }
    if p.sub_type == 1 {
        // El original gira la propia posición con su ángulo (VectorRotate(Position, M, Position)).
        p.position = rotate_in_place(p.angle, p.position);
    } else {
        p.position.z -= p.gravity;
    }
    flicker_light(p, ctx);
}

// --- BITMAP_ORORA ----------------------------------------------------------------------------------

fn create_orora(p: &mut Particle, _ctx: &mut Ctx) {
    p.scale = 0.2;
    match p.sub_type {
        0 | 1 => p.life_time = 100.0,
        2 | 3 => p.life_time = 25.0,
        _ => {}
    }
}

fn step_orora(p: &mut Particle, ctx: &mut Ctx) {
    // Mano izquierda (37) o derecha (28) del dueño.
    let (fscale, flight, bone) = match p.sub_type {
        0 => {
            p.rotation += 5.0;
            (0.01, 1.05, 37)
        }
        1 => {
            p.rotation -= 5.0;
            (0.01, 1.05, 28)
        }
        2 => {
            p.rotation += 20.0;
            (0.04, 1.33, 37)
        }
        3 => {
            p.rotation -= 20.0;
            (0.04, 1.33, 28)
        }
        _ => return,
    };
    if let Some(a) = ctx.actor(p.target) {
        p.position = a.bone_world(bone, Vec3::ZERO);
    }
    p.scale += fscale;
    if p.scale >= 0.8 {
        p.light *= 1.0 / flight;
    }
}

// --- BITMAP_SNOW_EFFECT_1 / 2 ----------------------------------------------------------------------

fn create_snow(p: &mut Particle, ctx: &mut Ctx) {
    let scale = p.scale;
    p.scale = scale + rf(ctx, 10) * 0.02;
    p.life_time = (ctx.rand() % 10 + 30) as f32;
    p.angle.z = rf(ctx, 360);
    p.gravity = (ctx.rand() % 10 + 20) as f32;
    let x = (ctx.rand() % 40 - 20) as f32 * 0.1;
    let y = (ctx.rand() % 40 - 20) as f32 * 0.1;
    p.velocity = rotate(p.angle, Vec3::new(x, y, 0.0) * 1.5);
}

fn step_snow(p: &mut Particle, _ctx: &mut Ctx) {
    p.rotation += 20.0;
    p.position.z += p.gravity * 0.5;
    p.light *= 1.0 / 1.02;
    p.gravity -= 1.5;
    if p.life_time < 10.0 {
        p.scale += 0.01;
    } else {
        p.scale -= 0.01;
    }
    p.position += p.velocity;
}

// --- BITMAP_DS_EFFECT ------------------------------------------------------------------------------

fn create_ds_effect(p: &mut Particle, ctx: &mut Ctx) {
    let scale = p.scale;
    p.scale = scale + rf(ctx, 10) * 0.02;
    p.life_time = 100.0;
}

fn step_ds_effect(p: &mut Particle, ctx: &mut Ctx) {
    p.rotation += 10.0;
    if p.target.is_some() {
        // Si el dueño ya no existe se trata como si hubiera cambiado de acción.
        let santa = ctx.actor(p.target).is_some_and(|a| a.action as i32 == PLAYER_SANTA_2);
        if !santa {
            p.live = false;
        }
    }
}

// --- BITMAP_BLOOD / BITMAP_BLOOD + 1 ---------------------------------------------------------------

fn create_blood(p: &mut Particle, ctx: &mut Ctx) {
    // `AngleMatrix(Angle)` usa el ángulo recibido, no el ya modificado.
    let angle = p.angle;
    p.life_time = 12.0;
    p.scale = (ctx.rand() % 4 + 8) as f32 * 0.05;
    p.angle.x = rf(ctx, 360);
    let y = -((ctx.rand() % 16 + 8) as f32);
    let z = (ctx.rand() % 6 - 3) as f32;
    p.velocity = rotate(angle, Vec3::new(0.0, y, z));
    if p.ty == BLOOD_1 {
        p.light = Vec3::new(0.1, 0.0, 0.0);
    }
}

fn step_blood(p: &mut Particle, _ctx: &mut Ctx) {
    p.frame = ((12.0 - p.life_time) / 3.0) as i32;
    p.velocity *= 0.95;
    p.position += p.velocity;
}

fn draw_blood1(p: &Particle, ctx: &DrawCtx, out: &mut DrawList) {
    let Some((w, h, blend)) = size(p, ctx) else { return };
    let uv = [(p.frame % 2) as f32 * 0.5, (p.frame / 2) as f32 * 0.5, 0.5, 0.5];
    sprite(out, ctx, p.tex_type, p.position, w, h, p.light, 0.0, uv, blend);
}

// --- BITMAP_FIRECRACKER ----------------------------------------------------------------------------

fn create_firecracker(p: &mut Particle, ctx: &mut Ctx) {
    p.angle = Vec3::ZERO;
    p.rotation = rf(ctx, 360);
    if p.sub_type == -1 {
        p.life_time = 2.0;
        p.scale = 0.25;
    } else {
        let size = 15 + p.sub_type;
        p.velocity.x = (ctx.rand() % size - size / 2) as f32 * 0.15;
        p.velocity.y = (ctx.rand() % size - size / 2) as f32 * 0.15;
        p.velocity.z = (ctx.rand() % size - size / 2) as f32 * 0.15 + 10.0;
        p.life_time = 50.0;
        p.scale = 0.10;
    }
    // `frame` (no lo usa este tipo) guarda el equivalente de `i % 8` (índice de la ranura del
    // original), que decide la longitud de la estela al dibujar.
    p.frame = ctx.rand() % 8;
}

fn step_firecracker(p: &mut Particle, _ctx: &mut Ctx) {
    p.velocity.z -= 0.5;
}

/// Estela de `iCount` sprites hacia atrás con el color que va rotando entre canales.
fn draw_firecracker(p: &Particle, ctx: &DrawCtx, out: &mut DrawList) {
    let Some((w, h, blend)) = size(p, ctx) else { return };
    let count = p.frame.rem_euclid(8) + 22;
    let temp = (p.life_time / 4.0 + p.sub_type as f32) as i32;
    let color = temp / 10;
    let change = temp % 10;
    let l = p.light.to_array();
    let life = (p.life_time as i32).min(10) as f32 * 0.001;
    for j in (0..=count).rev() {
        let mut pos = [0.0f32; 3];
        let mut light = [0.0f32; 3];
        let v = p.velocity.to_array();
        let o = p.position.to_array();
        for k in 0..3usize {
            pos[k] = o[k] - j as f32 * v[k] * 0.1;
            let a = l[(k as i32 + color).rem_euclid(3) as usize];
            let b = l[(k as i32 + color + 1).rem_euclid(3) as usize];
            light[k] = (count - j).min(10) as f32 * (a * (10 - change) as f32 + b * change as f32) * life;
        }
        sprite(out, ctx, p.tex_type, Vec3::from_array(pos), w, h, Vec3::from_array(light), p.rotation, FULL_UV, blend);
    }
}

// --- BITMAP_SWORD_FORCE ----------------------------------------------------------------------------

fn create_sword_force(p: &mut Particle, _ctx: &mut Ctx) {
    p.life_time = 10.0;
    p.gravity = 0.1;
    p.rotation = p.angle.z + 135.0;
}

fn step_sword_force(p: &mut Particle, _ctx: &mut Ctx) {
    p.gravity += 0.01;
    p.scale += p.gravity;
    let l = p.life_time / 10.0;
    p.light = Vec3::splat(l);
}

// --- BITMAP_CLOUD ----------------------------------------------------------------------------------

fn create_cloud(p: &mut Particle, ctx: &mut Ctx) {
    let scale = p.scale;
    let position = p.position;
    match p.sub_type {
        6 => {
            p.life_time = (25 + ctx.rand() % 5) as f32;
            p.rotation = rf(ctx, 360);
            p.scale = 0.2;
            p.velocity.x = (ctx.rand() % 10 + 5) as f32 * 0.4;
            p.velocity.y = 0.0;
            p.velocity.z = (ctx.rand() % 10 + 5) as f32 * 0.2;
        }
        8 => {
            p.life_time = 300.0;
            p.gravity = rf(ctx, 1000);
            p.alpha = 0.1;
            p.position.x += (ctx.rand() % 500 - 250) as f32;
            p.position.y += (ctx.rand() % 500 - 250) as f32;
            p.position.z += (ctx.rand() % 20 - 20) as f32;
            p.start_position.y = rf(ctx, 100) / 100.0;
            p.start_position.z = p.position.z;
            p.scale = (ctx.rand() % 20 + 180) as f32 * 0.01;
            p.turning_force.x = scale + rf(ctx, 30) / 100.0;
            p.velocity = Vec3::ZERO;
        }
        9 | 20 => {
            p.life_time = 300.0;
            p.gravity = rf(ctx, 1000);
            p.alpha = 0.1;
            p.position.x += (ctx.rand() % 500 - 250) as f32;
            p.position.y += (ctx.rand() % 500 - 250) as f32;
            p.position.z += (ctx.rand() % 20 - 20) as f32;
            p.start_position.y = rf(ctx, 100) / 100.0;
            p.start_position.z = p.position.z;
            p.scale = (ctx.rand() % 20 + 180) as f32 * 0.01 * p.scale;
            p.turning_force.x = rf(ctx, 10) / 100.0;
            p.velocity = Vec3::ZERO;
        }
        10 => {
            p.light = Vec3::ZERO;
            p.life_time = 500.0;
            p.position.x += (ctx.rand() % 400 - 200) as f32;
            p.position.y += (ctx.rand() % 400 - 200) as f32;
            p.position.z += (ctx.rand() % 400 - 200) as f32;
            p.start_position = p.position;
            p.scale = (ctx.rand() % 15 + 10) as f32 / 15.0 + scale;
            let t = (ctx.rand() % 10 + 5) as f32 * 0.12;
            p.velocity = Vec3::new(t, t, 0.0);
        }
        11 => {
            p.life_time = 500.0;
            p.position.x += (ctx.rand() % 400 - 200) as f32;
            p.position.y += (ctx.rand() % 400 - 200) as f32;
            p.position.z += (ctx.rand() % 400 - 200) as f32;
            p.start_position = p.position;
            p.scale = (ctx.rand() % 15 + 15) as f32 / 30.0 + scale;
            let t = (ctx.rand() % 10 + 5) as f32 * 0.12;
            p.velocity = Vec3::new(t, t, 0.0);
        }
        12 => {
            p.life_time = 30.0;
            p.gravity = rf(ctx, 1000);
            p.position.x += (ctx.rand() % 500 - 250) as f32;
            p.position.y += (ctx.rand() % 500 - 250) as f32;
            p.position.z += (ctx.rand() % 20 + 20) as f32;
            p.start_position.y = rf(ctx, 100) / 100.0;
            p.start_position.z = p.position.z;
            p.scale = (ctx.rand() % 20 + 180) as f32 * 0.01;
            p.turning_force.x = scale + rf(ctx, 30) / 100.0;
            p.velocity = Vec3::ZERO;
        }
        13 => {
            p.life_time = 30.0;
            p.rotation = rf(ctx, 360);
            p.gravity = (ctx.rand() % 50 - 25) as f32 / 10.0;
        }
        14 => {
            // El original pone a cero el `Light` local (argumento), no `o->Light`: sin efecto.
            p.life_time = (25 + ctx.rand() % 5) as f32;
            p.rotation = rf(ctx, 360);
            p.scale = scale * 0.2;
            p.velocity.x = (ctx.rand() % 10 + 5) as f32 * 0.4;
            p.velocity.y = 0.0;
            p.velocity.z = (ctx.rand() % 10 + 5) as f32 * 0.2;
        }
        15 => {
            p.life_time = 500.0;
            p.position.x += (ctx.rand() % 400 - 200) as f32;
            p.position.y += (ctx.rand() % 400 - 200) as f32;
            p.position.z += (ctx.rand() % 400 - 200) as f32;
            p.start_position = p.position;
            p.rotation = rf(ctx, 360);
            p.scale = (ctx.rand() % 15 + 15) as f32 / 30.0 + scale;
            let t = (ctx.rand() % 10 + 5) as f32 * 0.08;
            p.velocity = Vec3::new(t, t, 0.0);
            // TurningForce guarda el color final; la luz empieza apagada.
            p.turning_force = p.light;
            p.light = Vec3::ZERO;
        }
        16 => {
            p.light = Vec3::ZERO;
            p.life_time = 500.0;
            p.position.x += (ctx.rand() % 400 - 200) as f32;
            p.position.y += (ctx.rand() % 400 - 200) as f32;
            p.position.z += (ctx.rand() % 400 - 200) as f32;
            p.start_position = p.position;
            p.rotation = rf(ctx, 360);
            p.scale = (ctx.rand() % 15 + 10) as f32 / 15.0 + scale;
            let t = (ctx.rand() % 10 + 5) as f32 * 0.05;
            p.velocity = Vec3::new(t, t, 0.0);
        }
        17 => {
            p.tex_type = BITMAP_EVENT_CLOUD;
            p.life_time = 500.0;
            p.position.x += (ctx.rand() % 900 - 450) as f32;
            p.position.y += (ctx.rand() % 900 - 450) as f32;
            p.position.z += (ctx.rand() % 20 + 50) as f32;
            p.start_position = p.position;
            p.rotation = rf(ctx, 360);
            p.scale = (ctx.rand() % 20 + 20) as f32 / 80.0 + scale / 3.0;
            let t = (ctx.rand() % 10 + 5) as f32 * 0.006;
            p.velocity = Vec3::new(t, t, 0.0);
            p.turning_force = p.light;
            p.light = Vec3::ZERO;
        }
        18 => {
            p.tex_type = BITMAP_CHROME + 2;
            p.life_time = 160.0;
            p.light = Vec3::ZERO;
            p.gravity = rf(ctx, 1000);
            p.position = position;
            p.position.z += rf(ctx, 80);
            p.rotation = rf(ctx, 360);
            p.start_position.x = (ctx.rand() % 200 - 10) as f32 / 10.0;
            p.start_position.y = (ctx.rand() % 200 - 10) as f32 / 10.0;
            p.scale = (ctx.rand() % 70 + 5) as f32 * 0.02;
            p.turning_force.x = (ctx.rand() % 40 + 10) as f32 / 10000.0;
            p.turning_force.y = (ctx.rand() % 120 + 80) as f32;
            p.velocity = Vec3::ZERO;
        }
        19 => {
            p.tex_type = BITMAP_CHROME + 2;
            p.life_time = 60.0;
            p.light = Vec3::ZERO;
            p.gravity = rf(ctx, 1000);
            p.position.x += (ctx.rand() % 500 - 250) as f32;
            p.position.y += (ctx.rand() % 500 - 250) as f32;
            p.position.z += (ctx.rand() % 20 + 20) as f32;
            p.rotation = rf(ctx, 360);
            p.start_position.y = rf(ctx, 100) / 100.0;
            p.start_position.z = p.position.z;
            p.scale = (ctx.rand() % 90 + 220) as f32 * 0.02;
            p.turning_force.x = scale + rf(ctx, 20) / 100.0;
            p.velocity = Vec3::ZERO;
        }
        21 => {
            p.life_time = 100.0;
            p.gravity = rf(ctx, 1000);
            p.alpha = 0.6;
            p.position.x += (ctx.rand() % 200 - 100) as f32;
            p.position.y += (ctx.rand() % 200 - 100) as f32;
            p.position.z += (ctx.rand() % 20 - 20) as f32;
            p.start_position.y = rf(ctx, 100) / 100.0;
            p.start_position.z = p.position.z;
            p.scale = (ctx.rand() % 20 + 180) as f32 * 0.01 * p.scale;
            p.turning_force.x = rf(ctx, 10) / 100.0;
            p.velocity = Vec3::ZERO;
        }
        22 => {
            p.life_time = 80.0;
            p.gravity = rf(ctx, 1000);
            p.alpha = 0.1;
            p.position.x += (ctx.rand() % 20 - 10) as f32;
            p.position.y += (ctx.rand() % 20 - 10) as f32;
            p.position.z += (ctx.rand() % 20 - 20) as f32;
            p.scale = (ctx.rand() % 20 + 50) as f32 * 0.003 * p.scale;
            p.turning_force = Vec3::ZERO;
            let a = p.angle.z + rf(ctx, 360);
            let d = rf(ctx, 10) * 0.1 + 0.2;
            p.velocity = rotate(Vec3::new(0.0, 0.0, a), Vec3::new(0.0, d, 0.0));
            p.velocity.z = 0.0;
        }
        23 => {
            p.life_time = 80.0;
            p.gravity = rf(ctx, 1000);
            p.alpha = 0.1;
            p.position.x += (ctx.rand() % 6 - 3) as f32;
            p.position.y += (ctx.rand() % 6 - 3) as f32;
            p.position.z += (ctx.rand() % 6 - 3) as f32;
            p.scale = 0.0;
            p.turning_force = Vec3::ZERO;
            p.turning_force.x = if ctx.rand() % 2 == 0 { 1.0 } else { -1.0 };
            let a = p.angle.z + 90.0 + rf(ctx, 40) - 20.0;
            let d = rf(ctx, 10) * 0.2 + 1.0;
            p.velocity = rotate(Vec3::new(0.0, 0.0, a), Vec3::new(0.0, d, 0.0));
            p.velocity.z = 0.0;
        }
        _ => {
            p.life_time = 30.0;
            p.gravity = rf(ctx, 1000);
            p.position.x += (ctx.rand() % 500 - 250) as f32;
            p.position.y += (ctx.rand() % 500 - 250) as f32;
            p.position.z += (ctx.rand() % 20 + 20) as f32;
            p.start_position.y = rf(ctx, 100) / 100.0;
            p.start_position.z = p.position.z;
            p.scale = (ctx.rand() % 20 + 180) as f32 * 0.01;
            p.turning_force.x = scale + rf(ctx, 30) / 100.0;
            p.velocity = Vec3::ZERO;
        }
    }
    // `frame` (no lo usa este tipo) guarda la paridad de la ranura del original (`i % 2`), que
    // decide el sentido de giro al dibujar los subtipos 0, 3, 8 y 18.
    p.frame = ctx.rand() % 2;
}

/// Subtipos 6 y 14: humo que sube y se apaga.
fn cloud_smoke(p: &mut Particle, ctx: &mut Ctx) {
    p.position += p.velocity;
    p.velocity.x *= 0.95;
    p.velocity.y *= 0.95;
    p.velocity.z += 0.6;
    p.position.x += (ctx.rand() % 4 - 2) as f32;
    p.position.y += (ctx.rand() % 4 - 2) as f32;
    p.position.z += (ctx.rand() % 4 - 2) as f32 * 0.8;
    p.scale += 0.05;
    p.light = Vec3::splat(p.life_time / 50.0);
    p.rotation += (1 + ctx.rand() % 2) as f32;
}

/// Subtipos 8, 9, 20 y 21: aparece subiendo y luego baja desvaneciéndose.
fn cloud_fade(p: &mut Particle, limit: f32, up: f32, fade: f32, down: f32) {
    if p.life_time <= 0.0 {
        p.live = false;
    } else if p.life_time > limit {
        if p.alpha < 1.0 {
            p.alpha += 0.04;
        }
        p.position.z += up;
    } else {
        if p.alpha > 0.1 {
            p.alpha -= fade;
        }
        p.position.z -= down;
    }
}

/// Encendido gradual hacia `TurningForce` (subtipos 15 y 17).
fn cloud_light_in(p: &mut Particle, k: f32) {
    let tf = p.turning_force.to_array();
    let mut l = p.light.to_array();
    for i in 0..3 {
        l[i] = tf[i] * k * (500.0 - p.life_time);
        if l[i] > tf[i] {
            l[i] = tf[i];
        }
    }
    p.light = Vec3::from_array(l);
}

fn step_cloud(p: &mut Particle, ctx: &mut Ctx) {
    match p.sub_type {
        6 | 14 => cloud_smoke(p, ctx),
        8 | 9 | 20 => cloud_fade(p, 150.0, 1.0, 0.025, 0.5),
        10 => {
            p.position += p.velocity;
            p.turning_force.x = 1.5;
            let d = (p.position - p.start_position).length();
            if d <= 300.0 {
                p.light += Vec3::splat(0.01);
                if p.light.x >= 0.15 {
                    p.light = Vec3::splat(0.15);
                }
            } else if d <= 500.0 {
                p.light *= 1.0 / 1.05;
            } else {
                p.live = false;
            }
        }
        11 => {
            p.position += p.velocity;
            p.turning_force.x = 1.5;
            let d = (p.position - p.start_position).length();
            p.rotation += 0.5 + (ctx.rand() % 2) as f32;
            if d > 500.0 {
                p.live = false;
            }
        }
        13 => p.rotation += p.gravity * 1.5,
        15 => {
            if p.life_time <= 10.0 {
                p.live = false;
            }
            p.position += p.velocity;
            let d = (p.position - p.start_position).length();
            p.rotation += 0.5 + (ctx.rand() % 2) as f32;
            if d <= 50.0 {
                cloud_light_in(p, 0.1);
            } else {
                p.light *= 1.0 / 1.02;
                if p.light.x < 0.001 {
                    p.live = false;
                }
            }
            if d > 500.0 {
                p.live = false;
            }
        }
        16 => {
            if p.life_time <= 10.0 {
                p.live = false;
            }
            p.position += p.velocity;
            let d = (p.position - p.start_position).length();
            p.rotation += 0.5 + (ctx.rand() % 2) as f32;
            if d <= 50.0 {
                p.light += Vec3::splat(0.01);
                if p.light.x >= 0.2 {
                    p.light = Vec3::splat(0.2);
                }
            } else {
                p.light *= 1.0 / 1.05;
                if p.light.x < 0.001 {
                    p.live = false;
                }
            }
            if d >= 100.0 {
                p.live = false;
            }
        }
        17 => {
            if p.life_time <= 10.0 {
                p.live = false;
            }
            p.position += p.velocity;
            let d = (p.position - p.start_position).length();
            p.rotation += 0.08 + (ctx.rand() % 2) as f32 * 0.15;
            if p.life_time >= 400.0 {
                cloud_light_in(p, 0.02);
            } else {
                p.light *= 1.0 / 1.02;
                if p.light.x < 0.015 {
                    p.live = false;
                }
            }
            if d > 500.0 {
                p.live = false;
            }
        }
        18 => {
            if p.life_time > 90.0 {
                p.light += Vec3::new(0.002, 0.002, 0.0017);
            } else if p.life_time < 30.0 {
                p.light -= Vec3::new(0.004, 0.004, 0.0033);
            }
            if p.scale > 0.0 {
                p.scale -= 0.001;
            }
            if p.turning_force.y > 0.0 {
                p.turning_force.y -= 0.5;
            }
            let t = (ctx.world_time + p.rotation) * p.turning_force.x;
            if let Some(tp) = ctx.target_position(p.target) {
                p.position.x = tp.x + p.start_position.x + t.cos() * p.turning_force.y;
                p.position.y = tp.y + p.start_position.y + t.sin() * p.turning_force.y;
            }
            p.position.z += rf(ctx, 10) / 10.0;
            if p.life_time <= 0.0 {
                // TODO(original): o->Target->HiddenMesh = 0.
                p.live = false;
            }
        }
        19 => {
            if p.life_time > 30.0 {
                p.light += Vec3::splat(0.01);
                p.scale += 0.01;
            } else {
                p.light -= Vec3::splat(0.01);
            }
            p.position.y += 8.0 + rf(ctx, 200) * 0.1;
            if p.life_time <= 0.0 {
                // TODO(original): o->Target->HiddenMesh = 0.
                p.live = false;
            }
        }
        21 => cloud_fade(p, 50.0, 2.0, 0.005, 1.0),
        22 => {
            if p.life_time <= 0.0 {
                p.live = false;
            } else if p.life_time > 40.0 {
                if p.alpha < 1.0 {
                    p.alpha += 0.1;
                }
                p.position.z += 0.5;
            } else {
                if p.alpha > 0.0 {
                    p.alpha -= 0.025;
                } else {
                    p.live = false;
                }
                p.position.z -= 0.5;
            }
            p.scale += 0.001;
        }
        23 => {
            if p.life_time <= 0.0 {
                p.live = false;
            } else if p.life_time > 40.0 {
                if p.alpha < 1.0 {
                    p.alpha += 0.2;
                }
                p.scale += 0.003;
            } else {
                p.velocity.x *= 0.7;
                p.velocity.y *= 0.7;
                if p.alpha > 0.0 {
                    p.alpha -= rf(ctx, 10) * 0.01;
                } else {
                    p.live = false;
                }
                p.position.z += 0.2;
            }
            p.rotation += p.turning_force.x * 5.0;
        }
        _ => {
            // TODO(original): `o->Target->Visible` (objeto visible en pantalla). Aquí se toma como
            // visible si lo que sigue sigue existiendo.
            if ctx.target_position(p.target).is_some() {
                p.life_time = 50.0;
            }
            if p.life_time <= 0.0 {
                // TODO(original): o->Target->HiddenMesh = 0.
                p.live = false;
            }
            p.position.z = p.start_position.z + ((ctx.world_time + p.gravity) / 5000.0).sin() * 20.0;
        }
    }
    match p.sub_type {
        1 | 4 => p.rotation = ctx.world_time * 0.02 * p.turning_force.x + p.start_position.y,
        2 | 5 => p.rotation = ctx.world_time * -0.02 * p.turning_force.x + p.start_position.y,
        _ => {}
    }
}

fn draw_cloud(p: &Particle, ctx: &DrawCtx, out: &mut DrawList) {
    let Some((w, h, mut blend)) = size(p, ctx) else { return };
    let mut rotation = p.rotation;
    match p.sub_type {
        10 | 12 | 7 | 14 | 16 => blend = Blend::Subtract,
        0 | 8 | 3 | 18 => {
            // Paridad de la ranura guardada en `frame` (ver `create_cloud`).
            let dir = if p.frame % 2 == 0 { 0.02 } else { -0.02 };
            rotation = ctx.world_time * dir * p.turning_force.x + p.start_position.y;
        }
        _ => {}
    }
    let light = if matches!(p.sub_type, 8 | 9 | 20 | 21) { p.light * p.alpha } else { p.light };
    sprite(out, ctx, p.tex_type, p.position, w, h, light, rotation, FULL_UV, blend);
}
