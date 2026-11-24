//! `Effects[]` whose type is a `BITMAP_*` texture (part 1): emitters, ground marks and sprites used
//! by many class and monster skills. None of them draws a model: their visuals are the particles,
//! joints and sprites they create, plus the ground marks of `RenderEffectShadows`.
//!
//! Types: `BITMAP_SKULL`, `BITMAP_FIRE_CURSEDLICH`, `BITMAP_ENERGY`, `BITMAP_SPARK+1`,
//! `BITMAP_BOSS_LASER..+2`, `BITMAP_LIGHTNING+1`, `BITMAP_FIRE`, `BITMAP_FIRE+1`,
//! `BITMAP_FIRE_HIK2_MONO`, `BITMAP_CLOUD`, `BITMAP_IMPACT`, `BITMAP_MAGIC`, `BITMAP_MAGIC+2`,
//! `BITMAP_OUR_INFLUENCE_GROUND`, `BITMAP_ENEMY_INFLUENCE_GROUND`, `BITMAP_MAGIC_ZIN`,
//! `BITMAP_SHINY+6`, `BITMAP_PIN_LIGHT`, `BITMAP_ORORA`, `BITMAP_SPARK+2`, `BITMAP_LIGHT_MARKS`.
//!
//! Translated from Main 5.2 `ZzzEffect.cpp`: `CreateEffect` (≈733-1450), `CheckTargetRange` (366),
//! `MoveEffect` (≈7464-9610, common tail 18060-18124), `RenderEffects` (18425, 18512, 18823) and
//! `RenderEffectShadows` (19274-19573), plus `RenderCircle` (`ZzzEffectMagicSkill.cpp:19`).
//!
//! Render-time mutations: the original changes some effects while drawing them (`o->Scale +=`,
//! `o->Light /=`, sprites created from `RenderEffects`). Draw functions here cannot mutate, so:
//! - pure state changes made by `RenderEffectShadows` (after that frame's `MoveEffect`) are applied
//!   at the START of the next `step` (the original renders once between two moves);
//! - `RenderEffects` code that creates sprites or terrain lights runs at the END of `step`, with
//!   the `LifeTime` it would see after the tick's `LifeTime--`, so the sprites show this tick.
//!
//! `RenderEffectShadows` draws one `rand()` per effect per frame (`Luminosity`); draw functions use
//! [`draw_rand`] (per frame and effect) instead, which does not touch the logic `rand()`.

use std::f32::consts::PI;

use bevy::math::Vec3;

use super::common::{self, rf};
use crate::fx::core::*;

pub fn register(r: &mut Registry) {
    let k = |create: EffectCreate, step: EffectStep, draw: EffectDraw| EffectKind { create: Some(create), step: Some(step), draw: Some(draw) };
    let nothing = common::draw_nothing;
    r.effect(BITMAP_SKULL, k(skull_create, skull_step, nothing));
    r.effect(BITMAP_FIRE_CURSEDLICH, k(cursedlich_create, cursedlich_step, nothing));
    r.effect(BITMAP_ENERGY, k(energy_create, energy_step, nothing));
    r.effect(BITMAP_SPARK + 1, k(spark1_create, spark1_step, nothing));
    for n in 0..3 {
        r.effect(BITMAP_BOSS_LASER + n, k(boss_laser_create, boss_laser_step, boss_laser_draw));
    }
    r.effect(BITMAP_LIGHTNING + 1, k(lightning1_create, lightning1_step, lightning1_draw));
    r.effect(BITMAP_FIRE, k(fire_create, fire_step, nothing));
    r.effect(BITMAP_FIRE + 1, k(fire1_create, fire1_step, nothing));
    r.effect(BITMAP_FIRE_HIK2_MONO, k(hik2_create, hik2_step, hik2_draw));
    r.effect(BITMAP_CLOUD, k(cloud_create, cloud_step, cloud_draw));
    r.effect(BITMAP_IMPACT, k(impact_create, impact_step, nothing));
    r.effect(BITMAP_MAGIC, k(magic_create, magic_step, magic_draw));
    r.effect(BITMAP_MAGIC + 2, k(magic2_create, magic2_step, magic2_draw));
    r.effect(BITMAP_OUR_INFLUENCE_GROUND, k(influence_create, influence_step, our_influence_draw));
    r.effect(BITMAP_ENEMY_INFLUENCE_GROUND, k(influence_create, influence_step, enemy_influence_draw));
    r.effect(BITMAP_MAGIC_ZIN, k(zin_create, zin_step, zin_draw));
    r.effect(BITMAP_SHINY + 6, k(shiny6_create, shiny6_step, nothing));
    r.effect(BITMAP_PIN_LIGHT, k(pin_light_create, pin_light_step, nothing));
    r.effect(BITMAP_ORORA, k(orora_create, orora_step, nothing));
    r.effect(BITMAP_SPARK + 2, k(spark2_create, spark2_step, nothing));
    r.effect(BITMAP_LIGHT_MARKS, k(light_marks_create, light_marks_step, nothing));
}

// --- Helpers ----------------------------------------------------------------------------------------

/// `o->LifeTime` as the original's `int`.
fn lt(e: &Effect) -> i32 {
    e.life_time as i32
}

/// `o->Owner != NULL && o->Owner->Live`. A character owner is live while its snapshot exists and
/// says so; an effect owner while it is in the live effects list.
fn owner_live(ctx: &Ctx, owner: Option<Target>) -> bool {
    match owner {
        Some(Target::Actor(_)) => ctx.actor(owner).is_some_and(|a| a.live),
        Some(Target::Effect(id)) => ctx.effects.iter().any(|x| x.id == id),
        None => false,
    }
}

/// `rand() % Models[o->Owner->Type].NumBones` and `TransformByObjectBone(out, o->Owner, bone)`.
/// The original divides by zero on a model without bones; here no `rand()` is drawn then.
fn random_bone(ctx: &mut Ctx, owner: &Actor) -> Option<usize> {
    let n = owner.bones.len() as i32;
    if n <= 0 {
        return None;
    }
    Some((ctx.rand() % n) as usize)
}

/// Fixed randomness per frame, effect and salt for draw functions (does not touch the logic
/// `rand()`); same scheme as `objects_1.rs`.
fn draw_rand(ctx: &DrawCtx, e: &Effect, salt: u32) -> i32 {
    let frame = (ctx.world_time / 40.0) as u64;
    let mut x = frame ^ ((salt as u64) << 32) ^ (e.id as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    (x & 0x7fff) as i32
}

/// `float Luminosity = (float)(rand()%4+8)*0.1f;` at the start of each `RenderEffectShadows`
/// iteration (ZzzEffect.cpp:19268).
fn shadow_luminosity(ctx: &DrawCtx, e: &Effect) -> f32 {
    (draw_rand(ctx, e, 0) % 4 + 8) as f32 * 0.1
}

/// `RenderTerrainAlphaBitmap(tex, o->Position[0], o->Position[1], sx, sy, light, rotation)` with
/// the default alpha (1) and height (5).
#[allow(clippy::too_many_arguments)]
fn decal(out: &mut DrawList, tex: i32, e: &Effect, sx: f32, sy: f32, light: Vec3, rotation: f32, blend: Blend) {
    common::terrain_bitmap(out, tex, e.position.x, e.position.y, sx, sy, light, rotation, 1.0, 5.0, blend);
}

/// `RenderCircle(Type, Position, ScaleBottom, ScaleTop, Height, Rotation, LightTop, TextureV)`
/// (ZzzEffectMagicSkill.cpp:19): 12 quads forming a cone/cylinder around `position`, the bottom
/// ring lit 1 and the top ring `LightTop`. `QuadDraw` has one color per quad, so each segment is
/// split in 4 horizontal bands lit with the interpolated light at their middle (the original
/// interpolates it per vertex).
#[allow(clippy::too_many_arguments)]
fn render_circle(out: &mut DrawList, ty: i32, position: Vec3, scale_bottom: f32, scale_top: f32, height: f32, rotation: f32, light_top: f32, texture_v: f32) {
    const NUM: f32 = 12.0;
    const BANDS: usize = 4;
    for x in 0..12 {
        let x = x as f32;
        let (u0, u1) = (x * (1.0 / NUM), (x + 1.0) * (1.0 / NUM));
        // AngleIMatrix + VectorRotate = inverse rotation.
        let a1 = Vec3::new(0.0, 0.0, x * 30.0 + rotation);
        let a2 = Vec3::new(0.0, 0.0, (x + 1.0) * 30.0 + rotation);
        let p0 = position + irotate(a1, Vec3::new(0.0, scale_bottom, 0.0));
        let p1 = position + irotate(a2, Vec3::new(0.0, scale_bottom, 0.0));
        let p2 = position + irotate(a2, Vec3::new(0.0, scale_top, height));
        let p3 = position + irotate(a1, Vec3::new(0.0, scale_top, height));
        for b in 0..BANDS {
            let f0 = b as f32 / BANDS as f32;
            let f1 = (b + 1) as f32 / BANDS as f32;
            let l = 1.0 + (light_top - 1.0) * (f0 + f1) * 0.5;
            out.quads.push(QuadDraw {
                texture: ty,
                corners: [p0.lerp(p3, f0), p1.lerp(p2, f0), p1.lerp(p2, f1), p0.lerp(p3, f1)],
                uv: [[u0, 1.0 - f0 + texture_v], [u1, 1.0 - f0 + texture_v], [u1, 1.0 - f1 + texture_v], [u0, 1.0 - f1 + texture_v]],
                color: [l, l, l, 1.0],
                blend: Blend::Glow,
            });
        }
    }
}

// --- BITMAP_SKULL ----------------------------------------------------------------------------------

/// ZzzEffect.cpp:733.
fn skull_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 1000.0;
    if e.sub_type == 1 {
        e.life_time -= 60.0;
    } else if e.sub_type == 2 {
        e.life_time = 8.0;
        e.position.z += 150.0;
    } else if e.sub_type == 4 {
        e.life_time = 20.0;
        e.velocity = 2.0;
        e.direction = Vec3::new(0.0, -10.0, 0.0);
    }
}

/// ZzzEffect.cpp:7464.
fn skull_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    let light = Vec3::ONE;
    if e.sub_type == 4 {
        e.direction.y -= e.velocity;
        e.angle.x -= 1.0;
        ctx.create_sprite(BITMAP_SKULL, e.position, 10.0, e.light, 0.0, 0);
    } else {
        // `!o->Owner->Live` (the original crashes without an owner; here that ends the effect).
        // The owner is a character: an effect owner is treated as gone.
        let owner = ctx.actor(e.owner).filter(|a| a.live).map(|a| (a.position, a.scale));
        match owner {
            None => {
                e.life_time = 0.0;
                e.live = false;
            }
            Some((owner_pos, owner_scale)) => match e.sub_type {
                0 | 1 | 3 => {
                    // TODO(original): subtype 0: if g_isCharacterBuff(Owner, eDeBuff_Defense) then
                    // LifeTime = 10, and if also eBuff_Cloaking then Visible = false and no skulls
                    // this tick; otherwise it falls through to subtypes 1/3 (done here).
                    for i in 0..3 {
                        let mut param = i as f32 * PI * 2.0 / 3.0 + lt(e) as f32 * 0.17;
                        if e.sub_type == 3 {
                            param = -param;
                        }
                        let dist = 50.0 + 20.0 * (i as f32 * 15.37 + ctx.world_time() * 0.0031).sin();
                        let height = if e.sub_type == 3 { 250.0 } else { 200.0 };
                        let position = Vec3::new(
                            owner_pos.x + dist * param.sin(),
                            owner_pos.y + dist * param.cos(),
                            owner_pos.z + height * owner_scale,
                        );
                        ctx.create_sprite(BITMAP_SKULL, position, 1.0, light, 0.0, 0);
                    }
                }
                5 => {
                    for i in 0..3 {
                        let param = i as f32 * PI * 2.0 / 3.0 + lt(e) as f32 * 0.17;
                        let t = ctx.world_time() * 0.003;
                        let position = Vec3::new(
                            owner_pos.x + 50.0 * (param + t).sin(),
                            owner_pos.y + 50.0 * (param + t).cos(),
                            owner_pos.z + (i + 1) as f32 * 50.0 * owner_scale,
                        );
                        ctx.create_particle(BITMAP_LIGHT, position, e.angle, Vec3::new(0.6, 0.6, 1.0), 5, 0.7, None);
                    }
                }
                2 => {
                    let l = Vec3::splat(0.1 * lt(e) as f32);
                    ctx.create_sprite(BITMAP_SKULL, e.position, 1.5, l, 0.0, 0);
                }
                _ => {}
            },
        }
    }
    common::move_tail(e, ctx);
}

// --- BITMAP_FIRE_CURSEDLICH ------------------------------------------------------------------------

/// ZzzEffect.cpp:770. (Other subtypes, e.g. 4, keep `LifeTime` 0: they move once and die.)
fn cursedlich_create(e: &mut Effect, _ctx: &mut Ctx) {
    match e.sub_type {
        0 => {
            e.blend_mesh = -2;
            e.life_time = 10.0;
            e.scale = 0.7;
            e.direction = Vec3::ZERO;
        }
        1 => e.life_time = 50.0,
        2 => e.life_time = 20.0,
        3 => e.life_time = 10.0,
        12 => e.life_time = 20.0,
        _ => {}
    }
}

/// `switch(rand()%3)` between the three fire particles (`hik1`, `cursed`, `hik3`) with their subtypes.
#[allow(clippy::too_many_arguments)]
fn fire_trio(ctx: &mut Ctx, tys: [i32; 3], subs: [i32; 3], position: Vec3, angle: Vec3, light: Vec3, scale: f32) {
    let n = (ctx.rand() % 3) as usize;
    ctx.create_particle(tys[n], position, angle, light, subs[n], scale, None);
}

const FIRE_TRIO: [i32; 3] = [BITMAP_FIRE_HIK1, BITMAP_FIRE_CURSEDLICH, BITMAP_FIRE_HIK3];

/// ZzzEffect.cpp:7715. RenderEffects (18425) draws nothing.
fn cursedlich_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    match e.sub_type {
        0 => {
            // Follows bone `o->Skill` of the owner.
            match ctx.actor(e.owner).filter(|a| a.live) {
                None => e.live = false,
                Some(owner) => {
                    e.position = owner.bone_world(e.skill.max(0) as usize, Vec3::ZERO);
                    let light = Vec3::splat(e.alpha * 0.3);
                    ctx.create_particle(BITMAP_FIRE_CURSEDLICH, e.position, e.angle, light, 3, 1.0, e.owner);
                }
            }
        }
        1 => {
            let fire = Vec3::new(
                e.position.x + (ctx.rand() % 100 - 50) as f32 * 1.0,
                e.position.y + (ctx.rand() % 100 - 50) as f32 * 1.0,
                e.position.z + (ctx.rand() % 10 + 5) as f32 * 1.0,
            );
            let scale = (ctx.rand() % 5 + 13) as f32 * 0.1;
            fire_trio(ctx, FIRE_TRIO, [1, 5, 1], fire, e.angle, e.light, scale);
        }
        12 => {
            let fire = Vec3::new(
                e.position.x + (ctx.rand() % 50 - 25) as f32 * 1.0,
                e.position.y + (ctx.rand() % 50 - 25) as f32 * 1.0,
                e.position.z + (ctx.rand() % 10 + 5) as f32 * 1.0,
            );
            let scale = ((ctx.rand() % 5 + 13) as f32 * 0.05) * e.scale;
            fire_trio(ctx, FIRE_TRIO, [1, 5, 1], fire, e.angle, e.light, scale);
        }
        2 => {
            if e.owner.is_none() {
                e.live = false;
            } else if let Some(owner) = ctx.actor(e.owner).cloned() {
                for _ in 0..8 {
                    let Some(bone) = random_bone(ctx, &owner) else { break };
                    let fire = owner.bone_world(bone, Vec3::ZERO);
                    ctx.create_sprite(BITMAP_LIGHT, fire, 4.0, Vec3::new(1.0, 0.2, 0.0), 0.0, 0);
                    let scale = (ctx.rand() % 5 + 13) as f32 * 0.1;
                    fire_trio(ctx, FIRE_TRIO, [0, 4, 0], fire, e.angle, Vec3::ONE, scale);
                }
            }
        }
        3 => {
            for _ in 0..2 {
                let scale = (ctx.rand() % 5 + 18) as f32 * 0.03;
                fire_trio(ctx, FIRE_TRIO, [0, 4, 0], e.position, e.angle, e.light, scale);
            }
        }
        4 => {
            for _ in 0..2 {
                let scale = (ctx.rand() % 5 + 18) as f32 * 0.03;
                e.light = Vec3::new(0.6, 0.9, 0.1);
                let mono = [BITMAP_FIRE_HIK1_MONO, BITMAP_FIRE_HIK2_MONO, BITMAP_FIRE_HIK3_MONO];
                fire_trio(ctx, mono, [0, 4, 0], e.position, e.angle, e.light, scale);
            }
        }
        _ => {}
    }
    common::move_tail(e, ctx);
}

// --- BITMAP_ENERGY ---------------------------------------------------------------------------------

/// ZzzEffect.cpp:966.
fn energy_create(e: &mut Effect, _ctx: &mut Ctx) {
    if e.sub_type == 0 {
        e.life_time = 20.0;
        e.direction = Vec3::new(0.0, -60.0, 0.0);
        e.position.z += 100.0;
    }
}

/// ZzzEffect.cpp:8941.
fn energy_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    if e.sub_type == 0 {
        let lum = lt(e) as f32 * 0.2;
        let light = Vec3::splat(lum);
        ctx.create_particle(BITMAP_ENERGY, e.position, e.angle, light, 0, 1.0, None);
        ctx.create_particle(BITMAP_SPARK + 1, e.position, e.angle, light, 0, 4.0, None);
        ctx.add_terrain_light(e.position.x, e.position.y, Vec3::new(lum * 0.2, lum * 0.4, lum * 1.0), 2.0);
        check_target_range(e, ctx);
    }
    common::move_tail(e, ctx);
}

/// `CheckTargetRange(o)` for `BITMAP_ENERGY` (ZzzEffect.cpp:366, 407): on reaching its owner
/// (within 100 units) it ends with a spark.
fn check_target_range(e: &mut Effect, ctx: &mut Ctx) {
    // TODO(original): also requires Owner->Visible (frustum), not known here.
    let Some(to) = ctx.actor(e.owner).filter(|a| a.live) else { return };
    let dx = e.position.x - to.position.x;
    let dy = e.position.y - to.position.y;
    if (dx * dx + dy * dy).sqrt() <= 100.0 {
        e.life_time = 1.0;
        ctx.create_particle(BITMAP_SPARK + 1, e.position, e.angle, Vec3::ONE, 1, 6.0, None);
    }
}

// --- BITMAP_SPARK+1 --------------------------------------------------------------------------------

/// ZzzEffect.cpp:1111.
fn spark1_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 10.0;
}

/// ZzzEffect.cpp:8924: a column of 18 sparks.
fn spark1_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    let mut position = e.position;
    let lum = lt(e) as f32 * 0.1;
    let light = Vec3::splat(lum);
    let scale = 6.0;
    for j in 0..18 {
        position.z += scale * 4.0;
        let s = if j == 0 { scale * 2.0 } else { scale };
        ctx.create_particle(BITMAP_SPARK + 1, position, e.angle, light, 1, s, None);
    }
    common::move_tail(e, ctx);
}

// --- BITMAP_BOSS_LASER..+2 -------------------------------------------------------------------------

/// ZzzEffect.cpp:1116.
fn boss_laser_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 20.0;
    let p1 = match e.ty - BITMAP_BOSS_LASER {
        0 => {
            e.light = Vec3::new(0.5, 0.7, 1.0);
            e.scale = 16.0;
            if e.sub_type == 1 {
                e.scale = 3.0;
                e.light = Vec3::ONE;
            } else if e.sub_type == 2 {
                e.life_time = 35.0;
                e.scale = 2.5;
                e.light = Vec3::ONE;
            }
            Vec3::new(0.0, -50.0, 0.0)
        }
        1 => {
            e.light = Vec3::new(1.0, 0.4, 0.2);
            e.scale = 16.0;
            Vec3::new(0.0, -50.0, 0.0)
        }
        _ => {
            e.light = Vec3::new(1.0, 0.4, 0.2);
            e.scale = 5.0;
            Vec3::new(0.0, -15.0, 0.0)
        }
    };
    e.direction = rotate(e.angle, p1);
    // TODO(original): if Owner == Hero, AttackCharacterRange(o->Skill, P, 150, o->Weapon, o->PKKey)
    // at 4 points P stepping by (0, -150, 0) rotated by Angle from the position (damage system).
}

/// No `MoveEffect` case; the common tail only animates (no `MoveParticle` for these types).
/// Then the `RenderEffects` part (ZzzEffect.cpp:18512), run once per tick: 20 sprites along
/// `Direction`, each lighting the terrain.
fn boss_laser_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    common::move_tail(e, ctx);

    // RenderEffects only draws live effects: the tick's `LifeTime--` comes after this.
    if e.life_time - 1.0 <= 0.0 {
        return;
    }
    let mut position = e.position;
    for _ in 0..20 {
        if e.sub_type == 1 {
            e.angle.z -= 0.1;
            e.light = Vec3::ONE;
            ctx.create_sprite(BITMAP_FIRE + 1, position, e.scale, e.light, e.angle.z, 0);
            // (The BITMAP_SMOKE ground mark drawn here is in `boss_laser_draw`.)
            e.light = Vec3::new(0.2, 0.0, 0.0);
        } else if e.sub_type == 2 {
            e.light = Vec3::new(0.0, 0.2, 1.0);
            e.angle.z -= 0.1;
            ctx.create_sprite(BITMAP_FIRE + 1, position, e.scale, e.light, e.angle.z, 0);
        } else {
            ctx.create_sprite(BITMAP_SPARK + 1, position, e.scale, e.light, 0.0, 0);
        }
        position += e.direction;
        // TODO(original): during the Castle Siege (battleCastle::IsBattleCastleStart), stop the
        // whole loop when this point is on a TW_NOATTACKZONE tile.
        ctx.add_terrain_light(position.x, position.y, e.light, 2.0);
    }
}

/// Subtype 1: `RenderTerrainAlphaBitmap(BITMAP_SMOKE, ..., 4, 4, (0.2, 0, 0), -Angle[2])` at each
/// of the 20 points (ZzzEffect.cpp:18525-18528). `Angle[2]` dropped 0.1 per point during the
/// tick's 20 steps; here it is rebuilt from the final value.
fn boss_laser_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    if e.sub_type != 1 {
        return;
    }
    let light = Vec3::new(0.2, 0.0, 0.0);
    let mut position = e.position;
    for j in 0..20 {
        let angle = e.angle.z + 0.1 * (19 - j) as f32;
        common::terrain_bitmap(out, BITMAP_SMOKE, position.x, position.y, 4.0, 4.0, light, -angle, 1.0, 5.0, Blend::Glow);
        position += e.direction;
    }
}

// --- BITMAP_LIGHTNING+1 ----------------------------------------------------------------------------

/// ZzzEffect.cpp:1171.
fn lightning1_create(e: &mut Effect, _ctx: &mut Ctx) {
    if e.sub_type == 0 {
        e.life_time = 10.0;
        e.scale = 1.5;
    } else if e.sub_type == 1 {
        e.life_time = 50.0;
        e.alpha = 0.01;
    }
}

/// ZzzEffect.cpp:9595.
fn lightning1_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    // RenderEffectShadows (19570) grows it after drawing it each frame.
    e.scale += 0.2;
    if e.sub_type == 0 {
        let lum = lt(e) as f32 * 0.2;
        ctx.add_terrain_light(e.position.x, e.position.y, Vec3::new(lum * 0.2, lum * 0.5, lum * 1.0), 2.0);
    } else if e.sub_type == 1 {
        e.alpha += 0.01;
        if e.alpha >= 1.0 {
            e.alpha = 1.0;
        }
    }
    common::move_tail(e, ctx);
}

/// RenderEffectShadows (ZzzEffect.cpp:19569).
fn lightning1_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    let lum = lt(e) as f32 * 0.1;
    decal(out, BITMAP_LIGHTNING + 1, e, e.scale, e.scale, Vec3::splat(lum), -e.angle.z, Blend::Glow);
}

// --- BITMAP_FIRE / BITMAP_FIRE+1 -------------------------------------------------------------------

/// ZzzEffect.cpp:1210.
fn fire_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 1000.0;
}

/// ZzzEffect.cpp:8234.
fn fire_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    ctx.create_particle(BITMAP_FIRE, e.position, e.angle, Vec3::ONE, 9, 1.0, e.owner);
    common::move_tail(e, ctx);
}

/// ZzzEffect.cpp:1213.
fn fire1_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 10.0;
    e.position += rotate(e.angle, Vec3::new(0.0, -60.0, 0.0));
    e.position.z += 130.0;
}

/// ZzzEffect.cpp:8238.
fn fire1_step(e: &mut Effect, ctx: &mut Ctx) {
    let lum = common::luminosity(e, ctx);
    ctx.create_particle(BITMAP_FIRE + 1, e.position, e.angle, Vec3::ONE, 1, 1.0, None);
    ctx.add_terrain_light(e.position.x, e.position.y, Vec3::new(lum * 1.0, lum * 0.6, lum * 0.3), 2.0);
    common::move_tail(e, ctx);
}

// --- BITMAP_FIRE_HIK2_MONO -------------------------------------------------------------------------

/// ZzzEffect.cpp:1267.
fn hik2_create(e: &mut Effect, _ctx: &mut Ctx) {
    if e.sub_type == 0 {
        e.life_time = 60.0;
    } else if e.sub_type == 1 {
        e.life_time = 20.0;
        e.angle = Vec3::ZERO;
    }
}

/// ZzzEffect.cpp:8876.
fn hik2_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    if e.sub_type == 0 {
        for _ in 0..2 {
            let p = Vec3::new((ctx.rand() % 30 - 15) as f32, (ctx.rand() % 30 - 15) as f32, 0.0);
            let position = p + e.position;
            if ctx.rand() % 3 != 0 {
                ctx.create_particle(BITMAP_FLAME, position, e.angle, e.light, 11, 1.4, None);
            }
            if ctx.rand() % 8 == 0 {
                ctx.create_effect(MODEL_ICE_SMALL, position, e.angle, e.light, 0, None, -1, 0, 0, 0.0);
            }
        }
    } else if e.sub_type == 1 {
        for j in 0..9 {
            let angle = Vec3::new(0.0, 0.0, j as f32 * 40.0);
            let mut position = rotate(angle, Vec3::new(0.0, 250.0, 0.0)) + e.position;
            position.x += (ctx.rand() % 64 - 32) as f32;
            position.y += (ctx.rand() % 64 - 32) as f32;
            ctx.create_particle(BITMAP_FIRE + 3, position, e.angle, Vec3::ONE, 13, 2.5, None);
        }
    }
    common::move_tail(e, ctx);
}

/// RenderEffectShadows (ZzzEffect.cpp:19440).
fn hik2_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    if e.sub_type == 0 {
        let l = shadow_luminosity(ctx, e);
        decal(out, BITMAP_LIGHTNING + 1, e, 2.0, 2.0, Vec3::splat(l), -e.angle.z, Blend::Glow);
    }
}

// --- BITMAP_CLOUD ----------------------------------------------------------------------------------

/// ZzzEffect.cpp:1278 (`o->Scale = Scale`: the argument, already set).
fn cloud_create(e: &mut Effect, ctx: &mut Ctx) {
    if e.sub_type == 0 {
        e.life_time = 60.0;
        e.angle.z = rf(ctx, 360);
    }
}

/// ZzzEffect.cpp:8909.
fn cloud_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    if e.sub_type == 0 {
        e.light /= 1.05;
        e.scale += 0.03;
    }
    common::move_tail(e, ctx);
}

/// RenderEffectShadows (ZzzEffect.cpp:19448).
fn cloud_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    if e.sub_type == 0 {
        let l = shadow_luminosity(ctx, e);
        decal(out, BITMAP_CLOUD, e, e.scale, e.scale, e.light * l, -e.angle.z, Blend::Glow);
    }
}

// --- BITMAP_IMPACT ---------------------------------------------------------------------------------

/// ZzzEffect.cpp:1289.
fn impact_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 80.0;
    e.scale = 0.0;
    e.blend_mesh = -2;
}

/// ZzzEffect.cpp:8740: a growing glow in front of the owner's chest.
fn impact_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    let scale_bk = if e.scale < 2.0 {
        e.scale += 0.1;
        e.scale
    } else {
        if e.scale < 2.4 {
            e.scale += 0.02;
        } else {
            e.scale = 2.0;
        }
        if e.scale >= 2.2 { 2.2 - (e.scale - 2.2) } else { e.scale }
    };
    if lt(e) <= 40 {
        e.alpha -= 0.1;
        e.blend_mesh_light -= 0.1;
    }
    let light = e.light * e.alpha;
    // (The original dereferences the owner without checking it; without one it stays in place.)
    if let Some(owner) = ctx.actor(e.owner) {
        e.position = rotate(owner.angle, Vec3::new(-10.0, -30.0, 0.0)) + owner.position;
        e.position.z += 130.0;
    }
    let spin = ctx.world_time() * 0.08;
    ctx.create_sprite(BITMAP_IMPACT, e.position, scale_bk, light, 0.0, 0);
    ctx.create_sprite(BITMAP_SHINY + 1, e.position, scale_bk, light, -spin, 0);
    if e.scale > 1.0 {
        ctx.create_sprite(BITMAP_ORORA, e.position, scale_bk, light, -spin, 0);
    }
    if e.scale > 2.0 {
        ctx.create_sprite(BITMAP_ORORA, e.position, scale_bk, light, spin, 0);
    }
    common::move_tail(e, ctx);
}

// --- BITMAP_MAGIC ----------------------------------------------------------------------------------

/// ZzzEffect.cpp:1296.
fn magic_create(e: &mut Effect, _ctx: &mut Ctx) {
    // The `Scale` argument (the engine turns 0 into 0.9, so subtype 12 created without a scale
    // starts at 0.09 instead of 0).
    let arg_scale = e.scale;
    e.life_time = 20.0;
    e.scale = 0.5;
    match e.sub_type {
        0 => e.life_time = 15.0,
        8 => {
            e.life_time = 30.0;
            e.scale = 1.0;
        }
        9 => {
            e.life_time = 40.0;
            e.scale = 2.4;
            e.head_angle = Vec3::ZERO;
        }
        10 => {
            e.life_time = 44.0;
            e.scale = 12.0;
            e.alpha = 0.0;
        }
        11 => {
            e.life_time = 24.0;
            e.scale = 0.8;
            e.head_angle = Vec3::ZERO;
        }
        12 => {
            e.life_time = 20.0;
            e.scale = arg_scale * 0.1;
        }
        // PBG_ADD_NEWCHAR_MONK_SKILL (`o->Light = Light`: already).
        13 | 14 => {
            e.life_time = 30.0;
            e.scale = 1.0;
        }
        _ => {}
    }
}

/// ZzzEffect.cpp:8244.
fn magic_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    // RenderEffectShadows (19279-19282) dims and grows subtype 1 after drawing it each frame.
    if e.sub_type == 1 {
        e.light /= 1.1;
        e.scale += 0.05;
    }
    let me = Some(Target::Effect(e.id));
    let l = lt(e);
    match e.sub_type {
        0 => {
            ctx.create_effect(BITMAP_MAGIC, e.position, e.angle, e.light, 1, None, -1, 0, 0, 0.0);
            if l > 5 && l < 10 {
                ctx.create_particle(BITMAP_FLARE, e.position, e.angle, e.light, 0, 0.19, me);
            }
        }
        2 | 3 | 7 => {
            if l > 5 && l < 10 {
                let sub = if e.sub_type == 3 { 10 } else { 0 };
                ctx.create_particle(BITMAP_FLARE, e.position, e.angle, e.light, sub, 0.19, me);
            }
        }
        4 => ctx.create_particle(BITMAP_FLARE, e.position, e.angle, e.light, 12, 0.19, me),
        8 => e.scale += 1.8,
        9 => {
            if l < 20 {
                e.alpha -= 0.05;
            } else if e.alpha < 1.0 {
                e.alpha += 0.05;
            }
            e.head_angle.x += 4.0;
            e.head_angle.y -= 8.0;
            e.head_angle.z += 4.0;
        }
        10 => {
            if l < 20 {
                e.alpha -= 0.03;
            } else if e.alpha < 1.0 {
                e.alpha += 0.05;
            }
        }
        11 => {
            e.head_angle.x += 2.0;
            e.head_angle.y -= 2.0;
            e.head_angle.z += 2.0;
            if l <= 10 {
                e.alpha -= 0.05;
                e.light *= e.alpha;
            }
        }
        12 => {
            if e.alpha > 0.0 {
                e.alpha -= 0.05;
            } else {
                e.alpha = 0.0;
            }
            e.scale += e.alpha * 1.0;
            if e.scale > 5.0 {
                e.light *= 0.5;
            }
        }
        // PBG_ADD_NEWCHAR_MONK_SKILL.
        13 | 14 => {
            e.scale *= 1.1;
            e.light *= 0.95;
            if e.scale > 8.0 {
                e.live = false;
            }
            if e.scale > 4.0 {
                e.light *= 0.5;
            }
        }
        _ => {}
    }
    common::move_tail(e, ctx);
}

/// RenderEffectShadows (ZzzEffect.cpp:19274).
fn magic_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    let ty = BITMAP_MAGIC;
    match e.sub_type {
        1 => decal(out, ty, e, e.scale, e.scale, e.light, -e.angle.z, Blend::Glow),
        8 => {
            for s in [1.0, 0.8, 1.2] {
                decal(out, ty, e, e.scale * s, e.scale * s, e.light, -e.angle.z, Blend::Glow);
            }
        }
        9 | 11 => {
            let light = e.light * e.alpha;
            decal(out, BITMAP_SUMMON_IMPACT, e, e.scale, e.scale, light, e.head_angle.y, Blend::Glow);
            decal(out, BITMAP_SUMMON_IMPACT, e, e.scale * 1.2, e.scale * 1.2, light, e.head_angle.z, Blend::Glow);
        }
        10 => {
            // EnableAlphaBlendMinus: the layers stack as the life runs out.
            let light = e.light * e.alpha;
            let l = lt(e);
            for i in 0..5 {
                if (l < 3 && i > 0) || (l < 6 && i > 1) || (l < 9 && i > 2) || (l < 12 && i > 3) || (l < 16 && i > 4) {
                    continue;
                }
                decal(out, BITMAP_LIGHT, e, e.scale, e.scale, light, -e.angle.z, Blend::Subtract);
            }
        }
        12..=14 => {
            for _ in 0..2 {
                decal(out, ty, e, e.scale, e.scale, e.light, -e.angle.z, Blend::Glow);
            }
        }
        _ => {}
    }
}

// --- BITMAP_MAGIC+2 --------------------------------------------------------------------------------

/// ZzzEffect.cpp:1341 (shared with `BITMAP_MAGIC+1`).
fn magic2_create(e: &mut Effect, ctx: &mut Ctx) {
    // `o->Scale = Scale` (subtype 13): the argument, already set.
    e.life_time = 20.0;
    match e.sub_type {
        4 | 10 => {
            e.life_time = 40.0;
            e.scale = ((ctx.rand() % 50) + 50) as f32 / 100.0 * 4.0;
        }
        6 => {
            e.life_time = 60.0;
            e.scale = ((ctx.rand() % 50) + 50) as f32 / 100.0 * 4.0;
            e.start_position = e.position;
        }
        7 => {
            e.life_time = 40.0;
            e.angle.z = rf(ctx, 360);
        }
        8 => e.life_time = 20.0,
        9 => {
            e.life_time = 10.0;
            e.scale = 0.1;
        }
        11 | 12 => e.life_time = 20.0,
        13 => e.life_time = 40.0,
        _ => {}
    }
}

/// ZzzEffect.cpp:8348 (shared with `BITMAP_MAGIC+1`). Without `ENABLE_POTION_EFFECT` the
/// `else if` chain hangs off the healing `if`.
fn magic2_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    let s = e.sub_type;
    if s >= 1 && !matches!(s, 4 | 6 | 7 | 8 | 9 | 10 | 11 | 12 | 13) {
        common::create_healing(e, ctx);
    } else if s == 6 {
        e.blend_mesh_light = rf(ctx, 10) * 0.1;
        // (The original dereferences the owner without checking it.)
        if let Some(owner) = ctx.actor(e.owner).cloned() {
            let num_bones = owner.bones.len() as i32;
            let mut i = 0;
            while i < lt(e) && num_bones > 0 {
                i += 1;
                let index = (ctx.rand() % num_bones) as usize;
                // TODO(original): bones flagged `Dummy` are skipped (`continue`, no more rand()):
                // Actor does not say which ones they are.
                let p = Vec3::new((-50 + ctx.rand() % 100) as f32, (-10 + ctx.rand() % 20) as f32, (-10 + ctx.rand() % 20) as f32);
                // TransformPosition(..., true): BodyOrigin = StartPosition.
                let position = owner.bone_world(index, p) - owner.position + e.start_position;
                ctx.create_particle(BITMAP_LIGHT, position, owner.angle, Vec3::ONE, 8, 1.0, None);
            }
            // TODO(original): o->Owner->Alpha = LifeTime / 80 (the character fades out).
        }
    } else if s == 9 {
        e.scale += 0.3;
        e.light /= 1.1;
    }
    common::move_tail(e, ctx);
}

/// RenderEffectShadows (ZzzEffect.cpp:19478): two spinning cylinders (`RenderCircle`) and the
/// `BITMAP_MAGIC+1` mark.
fn magic2_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    let rotation = ((ctx.world_time as i32) % 3600) as f32 / 10.0;
    let l = lt(e);
    let mut lum = 1.0;
    let scale = if e.sub_type != 2 {
        render_circle(out, BITMAP_MAGIC + 2, e.position, 90.0, 130.0, 200.0, rotation, 0.0, 0.0);
        render_circle(out, BITMAP_MAGIC + 2, e.position, 90.0, 130.0, 200.0, -rotation, 0.0, 0.0);
        if l < 5 {
            lum -= (5 - l) as f32 * 0.2;
        }
        (20 - l) as f32 * 0.15
    } else if l > 10 {
        (20 - l) as f32 * 0.55
    } else {
        lum -= (10 - l) as f32 * 0.1;
        // The original leaves `Scale` uninitialized here (a stale stack value); the mark keeps
        // the last size it had (LifeTime 11) while it fades.
        (20 - 11) as f32 * 0.55
    };
    let light = Vec3::new(lum * 1.0, lum * 0.4, lum * 0.2);
    decal(out, BITMAP_MAGIC + 1, e, scale, scale, light, -e.angle.z, Blend::Glow);
}

// --- BITMAP_OUR_INFLUENCE_GROUND / BITMAP_ENEMY_INFLUENCE_GROUND (Gens marks) ----------------------

/// ZzzEffect.cpp:1383.
fn influence_create(e: &mut Effect, _ctx: &mut Ctx) {
    if e.sub_type == 0 {
        e.life_time = 50.0;
        e.scale = 0.6;
        e.alpha = 1.0;
        e.alpha_target = 0.75;
    }
}

/// ZzzEffect.cpp:8395: pulses under the owner forever.
fn influence_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    if e.sub_type == 0 {
        if !owner_live(ctx, e.owner) {
            e.live = false;
        }
        if let Some(p) = ctx.target_position(e.owner) {
            e.position = p;
        }
        e.alpha -= 0.02;
        e.scale += 0.01;
        if e.alpha < 0.0 {
            e.alpha = 1.0;
            e.scale = 0.6;
        }
        if lt(e) < 25 {
            e.alpha_target -= 0.02;
        } else if e.alpha_target < 1.0 {
            e.alpha_target += 0.02;
        }
        if lt(e) == 1 {
            e.life_time = 50.0;
        }
    }
    common::move_tail(e, ctx);
}

/// RenderEffectShadows (ZzzEffect.cpp:19526, `ASG_ADD_INFLUENCE_GROUND_EFFECT`).
fn our_influence_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    if e.sub_type != 0 {
        return;
    }
    let (a, t) = (e.alpha, e.alpha_target);
    decal(out, e.ty, e, e.scale, e.scale, Vec3::new(0.6, 0.9, 1.0) * a, 45.0, Blend::Glow);
    decal(out, e.ty, e, 0.8, 0.8, Vec3::new(0.6, 0.9, 1.0) * t, 45.0, Blend::Glow);
    let light = Vec3::new(0.2, 0.8, 1.0) * t;
    decal(out, BITMAP_LIGHT, e, 2.0, 2.0, light, 0.0, Blend::Glow);
    decal(out, BITMAP_LIGHT, e, 2.0, 2.0, light, 0.0, Blend::Glow);
}

/// RenderEffectShadows (ZzzEffect.cpp:19543, `ASG_ADD_INFLUENCE_GROUND_EFFECT`).
fn enemy_influence_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    if e.sub_type != 0 {
        return;
    }
    let (a, t) = (e.alpha, e.alpha_target);
    decal(out, e.ty, e, e.scale * 1.6, e.scale * 1.6, Vec3::new(1.0, 0.3, 0.2) * a, 0.0, Blend::Glow);
    decal(out, e.ty, e, 1.15, 1.15, Vec3::new(1.0, 0.3, 0.2) * t, 0.0, Blend::Glow);
    decal(out, BITMAP_LIGHT_RED, e, 1.8, 1.8, Vec3::splat(t), 0.0, Blend::Glow);
    decal(out, BITMAP_LIGHT, e, 2.0, 2.0, Vec3::new(t, 0.0, 0.0), 0.0, Blend::Glow);
}

// --- BITMAP_MAGIC_ZIN ------------------------------------------------------------------------------

/// ZzzEffect.cpp:1393.
fn zin_create(e: &mut Effect, _ctx: &mut Ctx) {
    match e.sub_type {
        0 => e.life_time = 50.0,
        1 => {
            e.life_time = 40.0;
            e.alpha = 0.0;
        }
        2 => e.life_time = 30.0,
        _ => {}
    }
}

/// ZzzEffect.cpp:8423.
fn zin_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    let l = lt(e);
    match e.sub_type {
        0 => {
            if l < 20 {
                e.alpha -= 0.05;
            } else if e.alpha < 1.0 {
                e.alpha += 0.05;
            }
        }
        1 => {
            if l < 20 {
                e.alpha -= 0.03;
            } else if e.alpha < 0.7 {
                e.alpha += 0.06;
            }
        }
        2 => {
            if e.scale < 3.5 {
                e.scale += 0.1;
            }
            if l < 20 {
                e.alpha -= 0.05;
            } else if e.alpha < 1.0 {
                e.alpha += 0.05;
            }
        }
        _ => {}
    }
    common::move_tail(e, ctx);
}

/// RenderEffectShadows (ZzzEffect.cpp:19506). (Other subtypes leave `vLight` uninitialized in the
/// original; nothing is drawn for them here.)
fn zin_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    let light = match e.sub_type {
        0 => e.light * e.alpha * 2.0,
        1 => e.light * e.alpha / 2.5,
        2 => e.light * e.alpha,
        _ => return,
    };
    decal(out, e.ty, e, e.scale, e.scale, light, e.head_angle.y, Blend::Glow);
}

// --- BITMAP_SHINY+6 --------------------------------------------------------------------------------

/// ZzzEffect.cpp:1404. (Subtype 3 sets `RenderType = RENDER_TYPE_ALPHA_BLEND_MINUS`, which only
/// matters for a model: nothing is drawn.)
fn shiny6_create(e: &mut Effect, _ctx: &mut Ctx) {
    match e.sub_type {
        0 | 3 => e.life_time = 24.0,
        1 | 2 => e.life_time = 100.0,
        _ => {}
    }
}

/// Subtypes 1/2 of `BITMAP_SHINY+6` and `BITMAP_PIN_LIGHT` (ZzzEffect.cpp:8458, 8532): sparkles
/// on random bones of the owner while it lives.
fn bone_sparkles(e: &mut Effect, ctx: &mut Ctx) {
    if !owner_live(ctx, e.owner) {
        e.live = false;
        return;
    }
    e.life_time = 100.0;
    let Some(owner) = ctx.actor(e.owner).cloned() else { return };
    let Some(bone) = random_bone(ctx, &owner) else { return };
    // TODO(original): nothing is created on bones flagged `Dummy` (unknown in Actor).
    let mut world = owner.bone_world(bone, Vec3::new(0.0, 0.0, 100.0));
    if ctx.rand() % 2 == 0 {
        world.z -= 20.0;
        ctx.create_particle(e.ty, world, e.angle, e.light, 0, e.scale, None);
    }
}

/// ZzzEffect.cpp:8449.
fn shiny6_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    match e.sub_type {
        0 => {
            let position = Vec3::new(
                e.position.x + (ctx.rand() % 500 - 250) as f32,
                e.position.y + (ctx.rand() % 500 - 250) as f32,
                e.position.z - (ctx.rand() % 100) as f32 + 150.0,
            );
            ctx.create_particle(e.ty, position, e.angle, e.light, 0, e.scale, None);
        }
        1 | 2 => bone_sparkles(e, ctx),
        3 => {
            // (The original does not check the owner.)
            let owner = ctx.actor(e.owner).cloned();
            if let Some((owner, bone)) = owner.and_then(|o| random_bone(ctx, &o).map(|b| (o, b))) {
                let fire = owner.bone_world(bone, Vec3::ZERO);
                let position = rotate(owner.angle, Vec3::new(0.0, -20.0, 0.0)) + fire;
                ctx.create_sprite(BITMAP_SHINY + 6, position, 2.0, e.light, 0.0, 1);
            }
        }
        _ => {}
    }
    common::move_tail(e, ctx);
}

// --- BITMAP_PIN_LIGHT ------------------------------------------------------------------------------

/// ZzzEffect.cpp:1417.
fn pin_light_create(e: &mut Effect, ctx: &mut Ctx) {
    match e.sub_type {
        0 | 3 => e.life_time = 40.0,
        1 | 2 => e.life_time = 100.0,
        4 => {
            e.alpha = 1.0;
            e.life_time = 30.0;
            e.scale += (ctx.rand() % 5) as f32 / 10.0;
            e.angle.y = rf(ctx, 360);
        }
        _ => {}
    }
}

/// ZzzEffect.cpp:8516.
fn pin_light_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    match e.sub_type {
        0 | 3 => {
            let position = Vec3::new(
                e.position.x + (ctx.rand() % 500 - 250) as f32,
                e.position.y + (ctx.rand() % 500 - 250) as f32,
                e.position.z - (ctx.rand() % 100) as f32 + 150.0,
            );
            if ctx.rand() % 2 == 0 {
                let sub = if e.sub_type == 3 { 1 } else { 0 };
                ctx.create_particle(e.ty, position, e.angle, e.light, sub, e.scale, None);
            }
        }
        1 | 2 => bone_sparkles(e, ctx),
        4 => {
            if !owner_live(ctx, e.owner) || e.alpha <= 0.0 {
                e.live = false;
            } else {
                e.scale -= 0.02;
                e.alpha -= 0.001;
                // TODO(original): skipped if bone 11 of the owner's model is `Dummy`.
                if let Some(owner) = ctx.actor(e.owner) {
                    let world = owner.bone_world(11, Vec3::ZERO);
                    e.position = world;
                    ctx.create_sprite(BITMAP_PIN_LIGHT, world, e.scale, e.light, e.angle.y, 0);
                }
            }
        }
        _ => {}
    }
    common::move_tail(e, ctx);
}

// --- BITMAP_ORORA ----------------------------------------------------------------------------------

/// ZzzEffect.cpp:1433: also creates the particle of the same type.
fn orora_create(e: &mut Effect, ctx: &mut Ctx) {
    match e.sub_type {
        0 | 1 => e.life_time = 100.0,
        2 | 3 => e.life_time = 25.0,
        _ => {}
    }
    ctx.create_particle(e.ty, e.position, e.angle, e.light, e.sub_type, 1.0, e.owner);
}

/// ZzzEffect.cpp:8584: re-creates itself (and its particle) while the owner lives.
fn orora_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    if !owner_live(ctx, e.owner) {
        e.live = false;
    } else if lt(e) <= 5 {
        e.live = false;
        ctx.create_effect(e.ty, e.position, e.angle, e.light, e.sub_type, e.owner, -1, 0, 0, 0.0);
    }
    common::move_tail(e, ctx);
}

// --- BITMAP_SPARK+2 --------------------------------------------------------------------------------

/// ZzzEffect.cpp:1444.
fn spark2_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 100.0;
}

/// ZzzEffect.cpp:8597.
fn spark2_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    if !owner_live(ctx, e.owner) {
        e.live = false;
    } else {
        e.life_time = 100.0;
        if ctx.rand() % 60 == 0 {
            ctx.create_particle(e.ty, e.position, e.angle, e.light, e.sub_type, 0.5, e.owner);
        }
    }
    common::move_tail(e, ctx);
}

// --- BITMAP_LIGHT_MARKS ----------------------------------------------------------------------------

/// ZzzEffect.cpp:1448.
fn light_marks_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 65.0;
}

/// Bones and sizes of the marks (ZzzEffect.cpp:18841-18845).
const LIGHT_MARKS_BONES: [usize; 14] = [20, 20, 19, 18, 17, 2, 35, 26, 36, 27, 37, 28, 39, 30];
const LIGHT_MARKS_SCALES: [f32; 14] = [1.5, 1.5, 0.6, 1.1, 0.9, 0.8, 0.6, 0.6, 0.8, 0.8, 0.8, 0.8, 0.7, 0.7];

/// ZzzEffect.cpp:8609, then the `RenderEffects` part (18823), once per tick: the light pulses
/// and 14 sprites are put on the owner's bones.
fn light_marks_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = common::luminosity(e, ctx);
    if !owner_live(ctx, e.owner) {
        e.live = false;
    } else if lt(e) <= 5 {
        e.life_time = 65.0;
    }
    common::move_tail(e, ctx);

    if !e.live {
        return;
    }
    // RenderEffects sees the LifeTime after the tick's `LifeTime--`.
    let lt_after = lt(e) - 1;
    let f_light = 1.035;
    if lt_after >= 35 {
        e.light /= f_light;
    } else {
        e.light *= f_light;
    }
    let Some(owner) = ctx.actor(e.owner).cloned() else { return };
    for (bone, s) in LIGHT_MARKS_BONES.iter().zip(LIGHT_MARKS_SCALES) {
        let pos = owner.bone_world(*bone, Vec3::ZERO);
        ctx.create_sprite(e.ty, pos, e.scale * s, e.light, 0.0, 0);
    }
}
