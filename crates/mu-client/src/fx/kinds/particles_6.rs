//! Partículas traducidas de `ZzzEffectParticle.cpp` (`CreateParticle`, `MoveParticles` y
//! `DescribeParticle`): aurora de GM, máscara del Templo Maldito, nubes de Raklion, `CHROME2`,
//! aura de AG adicional, explosión `SBUMB`, golpes `DAMAGE1`/`DAMAGE2` y `SWORD_EFFECT_MONO`.

use bevy::math::Vec3;

use crate::fx::core::*;

pub fn register(r: &mut Registry) {
    r.particle(BITMAP_GM_AURORA, ParticleKind { create: Some(gm_aurora_create), step: Some(gm_aurora_step), draw: None });
    r.particle(
        BITMAP_CURSEDTEMPLE_EFFECT_MASKER,
        ParticleKind { create: Some(masker_create), step: Some(masker_step), draw: None },
    );
    r.particle(
        BITMAP_RAKLION_CLOUDS,
        ParticleKind { create: Some(raklion_clouds_create), step: Some(raklion_clouds_step), draw: None },
    );
    r.particle(BITMAP_CHROME2, ParticleKind { create: Some(chrome2_create), step: Some(chrome2_step), draw: None });
    r.particle(
        BITMAP_AG_ADDITION_EFFECT,
        ParticleKind { create: Some(ag_addition_create), step: Some(ag_addition_step), draw: None },
    );
    r.particle(BITMAP_SBUMB, ParticleKind { create: Some(sbumb_create), step: Some(sbumb_step), draw: Some(sbumb_draw) });
    r.particle(BITMAP_DAMAGE1, ParticleKind { create: Some(damage1_create), step: Some(damage1_step), draw: None });
    r.particle(
        BITMAP_SWORD_EFFECT_MONO,
        ParticleKind { create: Some(sword_mono_create), step: Some(sword_mono_step), draw: Some(sword_mono_draw) },
    );
    r.particle(BITMAP_DAMAGE2, ParticleKind { create: Some(damage2_create), step: Some(damage2_step), draw: Some(damage2_draw) });
}

// --- Dibujo común ---------------------------------------------------------------------------------

/// `out.DrawSprite(Sprite(TexType, pos, W, H, light, rot, u, v, uw, vh))` con la mezcla de
/// `RenderParticles` (JPG suma, TGA prueba de alfa) y alfa = 1 (JPG) o `light[0]` (TGA).
#[allow(clippy::too_many_arguments)]
fn push_sprite(out: &mut DrawList, t: TexInfo, tex: i32, pos: Vec3, w: f32, h: f32, light: Vec3, rot: f32, uv: [f32; 4]) {
    out.sprites.push(SpriteDraw {
        texture: tex,
        position: pos,
        width: w,
        height: h,
        color: [light.x, light.y, light.z, if t.components == 3 { 1.0 } else { light.x }],
        rotation: rot,
        uv,
        blend: default_blend(t),
    });
}

// --- BITMAP_GM_AURORA -------------------------------------------------------------------------------

fn gm_aurora_create(p: &mut Particle, _ctx: &mut Ctx) {
    p.life_time = 20.0;
}

fn gm_aurora_step(p: &mut Particle, _ctx: &mut Ctx) {
    let luminosity = p.life_time / 8.0 * 0.04;
    p.light -= Vec3::splat(luminosity);
}

// --- BITMAP_CURSEDTEMPLE_EFFECT_MASKER ------------------------------------------------------------

fn masker_create(p: &mut Particle, _ctx: &mut Ctx) {
    p.life_time = 30.0;
}

fn masker_step(p: &mut Particle, ctx: &mut Ctx) {
    p.light *= 1.0 / 1.1;
    p.scale += 0.02;

    if p.sub_type == 0 {
        let light = Vec3::new(0.8, 0.3, 0.3);
        if ctx.rand_fps_check(2) {
            ctx.create_particle(BITMAP_CURSEDTEMPLE_EFFECT_MASKER, p.start_position, p.angle, light, 1, 1.3, None);
        }
    }
}

// --- BITMAP_RAKLION_CLOUDS ------------------------------------------------------------------------

fn raklion_clouds_create(p: &mut Particle, ctx: &mut Ctx) {
    p.alpha = 1.0;
    p.life_time = 32.0;
    p.rotation = (ctx.rand() % 360) as f32;
}

fn raklion_clouds_step(p: &mut Particle, ctx: &mut Ctx) {
    const FORCESCALAR: f32 = 13.0;
    const SCALEFACTOR: f32 = 0.065;
    const LIGHTDIVIDEDFACTOR: f32 = 1.6;
    const EPSILON_: f32 = 0.01;

    // Empuje por su −Y local.
    let force = rotate(p.angle, Vec3::new(0.0, -1.0, 0.0)) * FORCESCALAR;
    p.position += force;
    p.scale += SCALEFACTOR;
    p.light /= LIGHTDIVIDEDFACTOR;

    if p.light.x + p.light.y + p.light.z <= EPSILON_ {
        p.live = false;
    }

    if ctx.rand_fps_check(10) {
        let base_range = 70.0 * p.scale;
        let r1 = ((ctx.rand() % 2000) - 1000) as f32 * 0.001;
        let r2 = ((ctx.rand() % 2000) - 1000) as f32 * 0.001;
        let r3 = ((ctx.rand() % 2000) - 1000) as f32 * 0.001;
        let offset = Vec3::new(base_range * r1, base_range * r2, base_range * r3);
        ctx.create_particle(BITMAP_SHINY + 6, p.position + offset, p.angle, p.light, 0, 0.5, None);
    }
}

// --- BITMAP_CHROME2 -------------------------------------------------------------------------------

fn chrome2_create(p: &mut Particle, ctx: &mut Ctx) {
    p.life_time = (ctx.rand() % 5 + 5) as f32;
    p.rotation = (ctx.rand() % 360) as f32;
    // StartPosition = Position (ya lo pone Particle::new).
    p.start_position = p.position;
    p.scale *= 1.0 + (ctx.rand() % 10) as f32 * 0.03;
}

fn chrome2_step(p: &mut Particle, ctx: &mut Ctx) {
    if p.scale > 0.0 {
        p.scale -= 0.1;
    } else {
        p.scale = 0.0;
    }

    p.rotation += 1.0;
    p.alpha -= 0.05;
    // Igual que el original: posición = la del objetivo − StartPosition.
    if let Some(tp) = ctx.target_position(p.target) {
        p.position = tp - p.start_position;
    }
}

// --- BITMAP_AG_ADDITION_EFFECT --------------------------------------------------------------------

fn ag_addition_create(p: &mut Particle, ctx: &mut Ctx) {
    let mut scale = p.scale;
    if p.sub_type == 0 {
        p.life_time = (33 + ctx.rand() % 5) as f32;
        p.rotation = (ctx.rand() % 90) as f32 + 270.0;
        scale = ((ctx.rand() % 20) as f32 + 20.0) / 50.0 * 0.5;
    } else if p.sub_type == 1 {
        p.life_time = (27 + ctx.rand() % 5) as f32;
        p.rotation = (ctx.rand() % 90) as f32;
        scale = ((ctx.rand() % 20) as f32 + 20.0) / 50.0 * 1.5;
    } else if p.sub_type == 2 {
        p.life_time = (38 + ctx.rand() % 5) as f32;
        p.rotation = (ctx.rand() % 90) as f32 + 135.0;
        scale = ((ctx.rand() % 20) as f32 + 20.0) / 50.0 * 1.0;
    }
    p.scale = scale;
    p.gravity = (ctx.rand() % 16 + 12) as f32 * 0.1;
    p.alpha = 0.0;
    // El color base (1, 0, 0.6) se guarda en TurningForce; la luz empieza apagada.
    p.turning_force = Vec3::new(1.0, 0.0, 0.6);
    p.light = Vec3::ZERO;
}

fn ag_addition_step(p: &mut Particle, ctx: &mut Ctx) {
    if p.life_time < 10.0 {
        p.alpha -= 0.1;
    } else {
        p.alpha = 0.7;
    }

    if p.alpha < 0.1 {
        p.live = false;
    }

    let luminosity = p.alpha;
    p.light = p.turning_force * luminosity;

    // `TransformByObjectBone(Temp_Pos, Target, 18)`: el hueso 18 del objetivo.
    // (El original no comprueba Target; sin personaje se deja la posición como está.)
    let Some(mut temp_pos) = ctx.actor(p.target).map(|a| a.bone_world(18, Vec3::ZERO)) else { return };

    if p.sub_type == 0 {
        p.scale *= 1.05;
        temp_pos.z -= 10.0;
    } else if p.sub_type == 1 {
        p.scale *= 1.02;
        temp_pos.z += 10.0;
    } else if p.sub_type == 2 {
        p.scale *= 1.03;
        temp_pos.z += 25.0;
    }
    p.position = temp_pos;

    // TODO(original): si muere y el objetivo sigue con el buff eBuff_AG_Addition
    // (g_isCharacterBuff), crea otra BITMAP_AG_ADDITION_EFFECT del mismo subtipo en Temp_Pos con
    // o->Angle, o->Light, escala 1.0 y el mismo Target (además calcula un _Scale que no usa).
    // Falta el sistema de buffs.
}

// --- BITMAP_SBUMB ---------------------------------------------------------------------------------

fn sbumb_create(p: &mut Particle, _ctx: &mut Ctx) {
    p.life_time = 4.0;
    // o->Scale = Scale (ya lo es).
}

fn sbumb_step(p: &mut Particle, _ctx: &mut Ctx) {
    p.frame = 4 - p.life_time as i32;
}

fn sbumb_draw(p: &Particle, ctx: &DrawCtx, out: &mut DrawList) {
    let Some(t) = ctx.tex(p.tex_type) else { return };
    let (w, h) = (t.width * p.scale, t.height * p.scale);
    let u = (p.frame % 4) as f32 * 0.25 + 0.005;
    push_sprite(out, t, p.tex_type, p.position, w * 0.25, h, p.light, 0.0, [u, 0.0, 0.25 - 0.01, 1.0]);
}

// --- BITMAP_DAMAGE1 -------------------------------------------------------------------------------

fn damage1_create(p: &mut Particle, _ctx: &mut Ctx) {
    p.life_time = 5.0;
}

fn damage1_step(p: &mut Particle, _ctx: &mut Ctx) {
    p.scale *= 1.2;
    p.light *= 0.8;
    // Al final de MoveParticles el original dobla la luz de las DAMAGE1 que siguen vivas.
    if p.live {
        p.light *= 2.0;
    }
}

// --- BITMAP_SWORD_EFFECT_MONO ---------------------------------------------------------------------

fn sword_mono_create(p: &mut Particle, _ctx: &mut Ctx) {
    p.life_time = 20.0;
}

fn sword_mono_step(p: &mut Particle, _ctx: &mut Ctx) {
    if p.sub_type == 1 {
        p.scale *= 0.8;
    } else if p.sub_type == 0 {
        p.scale *= 1.2;
    }
    if p.scale > 6.0 {
        p.sub_type = 1;
        return;
    } else if p.sub_type == 1 && p.scale < 1.0 {
        p.live = false;
        return;
    }

    p.light *= 0.94;
}

fn sword_mono_draw(p: &Particle, ctx: &DrawCtx, out: &mut DrawList) {
    let Some(t) = ctx.tex(p.tex_type) else { return };
    let (w, h) = (t.width * p.scale, t.height * p.scale);
    let mut pos = p.position;
    pos.z += 31.0 * p.scale;
    push_sprite(out, t, p.tex_type, pos, w * 0.9, h * 1.1, p.light, p.rotation, [0.0, 0.0, 1.0, 1.0]);
}

// --- BITMAP_DAMAGE2 -------------------------------------------------------------------------------

fn damage2_create(p: &mut Particle, ctx: &mut Ctx) {
    p.life_time = 15.0;
    p.scale = p.scale * 0.9 + (ctx.rand() % 2 + 2) as f32 * 0.1;
    p.position.z += 80.0 + (ctx.rand() % 20) as f32;
}

fn damage2_step(p: &mut Particle, _ctx: &mut Ctx) {
    p.scale *= 1.2;
    p.light *= 0.55;
}

fn damage2_draw(p: &Particle, ctx: &DrawCtx, out: &mut DrawList) {
    let Some(t) = ctx.tex(p.tex_type) else { return };
    let (w, h) = (t.width * p.scale, t.height * p.scale);
    let light = p.light * 1.4;
    push_sprite(out, t, p.tex_type, p.position, w, h, light, p.rotation, [0.0, 0.0, 1.0, 1.0]);
}
