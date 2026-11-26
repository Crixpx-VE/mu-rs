//! Dark Lord / Magic Gladiator / Blade Knight master effects (2): the class change-up effects
//! (`MODEL_CHANGE_UP_EFF`, `MODEL_CHANGE_UP_NASA`, `MODEL_CHANGE_UP_CYLINDER`), Balgas' fissure
//! (`MODEL_SKILL_FISSURE`, `MODEL_FISSURE`, `MODEL_FISSURE_LIGHT`) and wave (`MODEL_BALGAS_SKILL`),
//! Shield Crash (`MODEL_SHIELD_CRASH`, `MODEL_SHIELD_CRASH2`), `MODEL_BLADE_SKILL`, Blow of
//! Destruction (`MODEL_BLOW_OF_DESTRUCTION`, `MODEL_NIGHTWATER_01`, `MODEL_KNIGHT_PLANCRACK_A/B`),
//! Flame Strike (`MODEL_EFFECT_FLAME_STRIKE`), the Dark Raven feathers and stars
//! (`MODEL_STAR_SHINE`, `MODEL_FEATHER`, `MODEL_FEATHER_FOREIGN`), the move-click marker
//! (`MODEL_MOVE_TARGETPOSITION_EFFECT`), the SD aura (`MODEL_EFFECT_SD_AURA`) and the castle
//! guardian aura (`MODEL_AURORA`).
//!
//! Translated from Main 5.2 `ZzzEffect.cpp` (`CreateEffect`, `MoveEffect` and its common tail
//! 18060-18124, `EffectDestructor` 231, `RenderEffects`, `RenderEffectShadows`) and the
//! type-specific branches of `Draw_RenderObject` (`ZzzObject.cpp` 399-2680), which is what
//! `RenderObject(o)` runs for them. MuMain (`MoveHandlers.cpp`) was only used to cross-check.
//!
//! Notes:
//! - Every kind has a `step`, so the engine does not run the common tail: [`move_tail`] is called
//!   where the original runs it (`MODEL_CHANGE_UP_EFF` is excluded from it, 18068).
//! - `CreateSprite` calls with `rand()` inside `RenderEffects` (Blow of Destruction) run at the END
//!   of `step`, with the `LifeTime` the following render sees (after the engine's `LifeTime -= 1`).
//! - Function arguments that call `rand()` are evaluated left to right (like the other kind files).

use bevy::math::{EulerRot, Mat3, Quat, Vec2, Vec3};

use super::common::{self, luminosity, move_tail, rf};
use crate::fx::core::*;

/// Bones of `bossrock.bmd` (`MODEL_FISSURE`): 149, none of them dummy (read with mu-data's
/// `bmdinfo` example). Only the fallback when the model file is missing: the bone loop of
/// `MoveEffect` (ZzzEffect.cpp:10005) calls `rand()` once per non-dummy bone, so the count keeps
/// the `rand()` sequence; with the file, its own bones are used.
const FISSURE_BONES: usize = 149;

pub fn register(r: &mut Registry) {
    let k = |create: EffectCreate, step: EffectStep, draw: EffectDraw| EffectKind { create: Some(create), step: Some(step), draw: Some(draw) };

    r.effect(MODEL_CHANGE_UP_EFF, k(change_up_eff_create, change_up_eff_step, change_up_eff_draw));
    r.effect(MODEL_CHANGE_UP_NASA, k(nasa_create, nasa_step, nasa_draw));
    r.effect(MODEL_CHANGE_UP_CYLINDER, k(cylinder_create, cylinder_step, cylinder_draw));

    // MODEL_SKILL_FISSURE has no model (ZzzOpenData.cpp loads none): RenderObject draws nothing.
    r.effect(MODEL_SKILL_FISSURE, k(skill_fissure_create, skill_fissure_step, common::draw_nothing));
    r.effect(MODEL_FISSURE, k(fissure_create, fissure_step, draw_plain));
    r.effect(MODEL_FISSURE_LIGHT, k(fissure_create, fissure_light_step, draw_plain));
    r.effect(MODEL_BALGAS_SKILL, k(balgas_create, balgas_step, draw_plain));

    r.effect(MODEL_SHIELD_CRASH, k(shield_crash_create, shield_crash_step, draw_plain));
    r.effect(MODEL_SHIELD_CRASH2, k(shield_crash2_create, shield_crash2_step, draw_plain));
    r.effect(MODEL_BLADE_SKILL, k(blade_create, blade_step, blade_draw));

    r.effect(MODEL_BLOW_OF_DESTRUCTION, k(blow_create, blow_step, blow_draw));
    r.effect(MODEL_NIGHTWATER_01, k(nightwater_create, fade_step, nightwater_draw));
    r.effect(MODEL_KNIGHT_PLANCRACK_A, k(plancrack_a_create, plancrack_a_step, plancrack_draw));
    r.effect(MODEL_KNIGHT_PLANCRACK_B, k(plancrack_b_create, fade_step, plancrack_draw));
    r.effect(MODEL_EFFECT_FLAME_STRIKE, k(flame_strike_create, flame_strike_step, flame_strike_draw));

    // MODEL_STAR_SHINE has no model (ZzzOpenData.cpp loads none): only its sprites are seen.
    r.effect(MODEL_STAR_SHINE, k(star_shine_create, star_shine_step, common::draw_nothing));
    r.effect(MODEL_FEATHER, k(feather_create, feather_step, feather_draw));
    r.effect(MODEL_FEATHER_FOREIGN, k(feather_foreign_create, feather_foreign_step, feather_draw));

    r.effect(MODEL_MOVE_TARGETPOSITION_EFFECT, k(move_target_create, move_target_step, move_target_draw));
    r.effect(MODEL_EFFECT_SD_AURA, k(sd_aura_create, sd_aura_step, sd_aura_draw));
    r.effect(MODEL_AURORA, k(aurora_create, aurora_step, draw_plain));
}

// --- Helpers ----------------------------------------------------------------------------------------

/// `o` as the owner/target of what this effect creates.
fn me(e: &Effect) -> Option<Target> {
    Some(Target::Effect(e.id))
}

/// The original's integer `LifeTime`.
fn life(e: &Effect) -> i32 {
    e.life_time as i32
}

/// `o->Owner->Position` and `o->Owner->Angle` (a character, or another effect as of tick start).
/// `None` when the owner is missing (the original would dereference NULL).
fn owner_pos_angle(owner: Option<Target>, ctx: &Ctx) -> Option<(Vec3, Vec3)> {
    match owner? {
        Target::Actor(_) => ctx.actor(owner).map(|a| (a.position, a.angle)),
        Target::Effect(id) => ctx.effects.iter().find(|i| i.id == id).map(|i| (i.position, i.angle)),
    }
}

/// `o->Owner->Live` (an effect owner is alive while it is still in `Effects[]`). A missing owner
/// would crash the original; here it counts as dead.
fn owner_live(e: &Effect, ctx: &Ctx) -> bool {
    match e.owner {
        Some(Target::Actor(_)) => ctx.actor(e.owner).is_some_and(|a| a.live),
        Some(Target::Effect(id)) => ctx.effect_positions.contains_key(&id),
        None => false,
    }
}

/// `VectorNormalize`: a zero vector stays zero.
fn normalize(v: Vec3) -> Vec3 {
    v.normalize_or_zero()
}

// --- Drawing helpers ------------------------------------------------------------------------------

/// `RenderObject(o)`: nothing when `Alpha < 0.01` (`Calc_RenderObject`, ZzzObject.cpp:245).
fn object(e: &Effect) -> Option<ModelDraw> {
    (e.alpha >= 0.01).then(|| ModelDraw::of_effect(e))
}

/// `RenderObject(o)` with `RenderBody(RENDER_TEXTURE, Alpha, BlendMesh, BlendMeshLight, U, V,
/// HiddenMesh)`: the last `else` of `Draw_RenderObject` (ZzzObject.cpp:2614), or the
/// `MODEL_SHIELD_CRASH/2` branch (ZzzObject.cpp:1639, `BodyLight = Light`, the same call). Used by
/// the `default:` of `RenderEffects` for `MODEL_SKILL_BEGIN..MODEL_SKILL_END`.
fn draw_plain(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    if let Some(mut d) = object(e) {
        d.render_flags = RENDER_TEXTURE;
        out.models.push(d);
    }
}

/// `RenderObject(o)` whose `Draw_RenderObject` branch sets a fixed `BodyLight` and then
/// `RenderBody(RENDER_TEXTURE, Alpha, BlendMesh, BlendMeshLight, U, V)` (the change-up effects,
/// ZzzObject.cpp:493-514; no `HiddenMesh` argument, so -1).
fn draw_with_light(e: &Effect, out: &mut DrawList, light: Vec3) {
    if let Some(mut d) = object(e) {
        d.light = light;
        d.hidden_mesh = -1;
        d.render_flags = RENDER_TEXTURE;
        out.models.push(d);
    }
}

// --- MODEL_CHANGE_UP_EFF ------------------------------------------------------------------------------

/// ZzzEffect.cpp:2137.
fn change_up_eff_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.blend_mesh = -2;
    e.life_time = 100.0;
    e.scale = 0.7;
    // `o->Position = Position + (0, 0, 22)`.
    e.position.z += 22.0;
    e.direction = Vec3::ZERO;
    if e.sub_type == 1 {
        e.scale = 0.4;
        e.life_time = 10.0;
        e.blend_mesh_light = 0.7;
    }
}

/// ZzzEffect.cpp:10404. No common tail: `MODEL_CHANGE_UP_EFF` is excluded from it (18068).
fn change_up_eff_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    // `Light` is MoveEffect's local, (1, 1, 1) (ZzzEffect.cpp:6929).
    let light = Vec3::ONE;
    if ctx.rand() % 2 == 0 {
        if e.sub_type == 2
            && let Some(p) = ctx.target_position(e.owner)
        {
            e.position = p;
            e.position.z = ctx.terrain_height(p.x, p.y) + 22.0;
        }
        let x = e.position.x + rf(ctx, 200) - 100.0;
        let y = e.position.y + rf(ctx, 200) - 100.0;
        let loc = Vec3::new(x, y, e.position.z - 200.0);
        if e.sub_type == 0 || e.sub_type == 2 {
            ctx.create_joint(BITMAP_FLARE, loc, loc, e.angle, 50, None, 40.0);
        }
        if life(e) > 40 {
            // PKKey = 4 (`4.0f` passed as the short PKKey).
            ctx.create_effect(BITMAP_MAGIC, e.position, e.angle, light, 4, me(e), 4, 0, 0, 0.0);
        }
    }
    if life(e) <= 1 && e.sub_type == 0 {
        // SetPlayerStop(Hero): the hero returns to the standing action.
        if let Some(hero) = ctx.hero {
            ctx.change_actor(hero, ActorChange::Stop);
        }
    }
    // `b->PlayAnimation(..., Velocity/3, ...)` (10437-10446); its result is not used. (The
    // original does not set `b->CurrentAction` first; this effect's action 0 is used.)
    if e.sub_type == 0 || e.sub_type == 2 {
        let speed = e.velocity / 3.0;
        let _ = common::play_model_animation(e, ctx, speed);
    }
}

/// RenderEffects 18408 → ZzzObject.cpp:493: BodyLight (0.1, 0.4, 0.6) for SubType 0/1, else
/// (0.1, 0.2, 0.9); `BlendMesh = -2` (all meshes additive with `BodyLight · BlendMeshLight`).
fn change_up_eff_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    let light = if e.sub_type == 0 || e.sub_type == 1 { Vec3::new(0.1, 0.4, 0.6) } else { Vec3::new(0.1, 0.2, 0.9) };
    draw_with_light(e, out, light);
}

// --- MODEL_CHANGE_UP_NASA -----------------------------------------------------------------------------

/// ZzzEffect.cpp:2152.
fn nasa_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.blend_mesh = -2;
    e.life_time = 100.0;
    e.scale = 0.9;
    if (1..=3).contains(&e.sub_type) {
        e.life_time = 80.0;
    }
    e.position.z += 12.0;
    e.direction = Vec3::ZERO;
}

/// ZzzEffect.cpp:10449.
fn nasa_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    let lt = life(e);
    if e.sub_type == 0 {
        if lt == 100 {
            ctx.create_effect(MODEL_CHANGE_UP_NASA, e.position, e.angle, e.light, 1, me(e), -1, 0, 0, 0.0);
        } else if lt == 70 {
            ctx.create_effect(MODEL_CHANGE_UP_NASA, e.position, e.angle, e.light, 2, me(e), -1, 0, 0, 0.0);
        }
        if lt == 40 {
            ctx.create_effect(MODEL_CHANGE_UP_NASA, e.position, e.angle, e.light, 3, me(e), -1, 0, 0, 0.0);
        }
    }
    if (1..=3).contains(&e.sub_type) {
        e.scale += 0.02;
        e.blend_mesh_light = lt as f32 * 0.005;
    } else if e.sub_type == 0 {
        e.blend_mesh_light = lt as f32 * 0.01;
    }
    move_tail(e, ctx);
}

/// Default RenderObject → ZzzObject.cpp:505: BodyLight (0.5, 0.5, 0.9).
fn nasa_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    draw_with_light(e, out, Vec3::new(0.5, 0.5, 0.9));
}

// --- MODEL_CHANGE_UP_CYLINDER -------------------------------------------------------------------------

/// ZzzEffect.cpp:2165.
fn cylinder_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.blend_mesh = -2;
    e.life_time = 100.0;
    e.scale = 0.9;
    e.direction = Vec3::new(0.0, 0.0, 1.0);
    if e.sub_type == 1 {
        e.light = Vec3::ZERO;
        e.blend_mesh = -2;
        e.life_time = 10.0;
        e.scale = 0.1;
    }
}

/// ZzzEffect.cpp:10477. (The original also rotates an uninitialized `Go` by an uninitialized
/// `Angle` into a position it never uses; nothing to translate.)
fn cylinder_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    if e.sub_type == 2
        && let Some(p) = ctx.target_position(e.owner)
    {
        e.position = p;
        e.position.z = ctx.terrain_height(p.x, p.y);
    }
    if e.sub_type == 1 {
        e.blend_mesh_light = life(e) as f32 * 0.015;
        e.scale += 0.08;
    }
    move_tail(e, ctx);
}

/// Default RenderObject → ZzzObject.cpp:510: BodyLight (0.4, 0.5, 1).
fn cylinder_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    draw_with_light(e, out, Vec3::new(0.4, 0.5, 1.0));
}

// --- MODEL_SKILL_FISSURE ------------------------------------------------------------------------------

/// ZzzEffect.cpp:2124.
fn skill_fissure_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 20.0;
}

/// ZzzEffect.cpp:9953.
fn skill_fissure_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    let lt = life(e);
    if lt == 8 {
        for i in 0..16 {
            let angle = Vec3::new(-10.0, 0.0, i as f32 * 10.0);
            let mut position = e.position;
            position.z += 100.0;
            position.x += rf(ctx, 600) - 200.0;
            position.y += rf(ctx, 600) - 400.0;
            ctx.create_joint(BITMAP_FLARE, position, position, angle, 24, None, 90.0);
        }
    } else if lt == 2 {
        let angle = Vec3::new(0.0, 0.0, rf(ctx, 360));
        ctx.create_effect(MODEL_FISSURE, e.position, angle, e.light, 0, me(e), -1, 0, 0, 0.0);
        ctx.create_effect(MODEL_FISSURE_LIGHT, e.position, angle, e.light, 0, me(e), -1, 0, 0, 0.0);
        e.live = false;
    }
    move_tail(e, ctx);
}

// --- MODEL_FISSURE / MODEL_FISSURE_LIGHT --------------------------------------------------------------

/// ZzzEffect.cpp:2127.
fn fissure_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 120.0;
    e.scale = 0.8;
}

/// ZzzEffect.cpp:9983.
fn fissure_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    // `double fLevel = sinf(Q_PI * double(LifeTime) / 120.f) * -0.5f`.
    let level = ((std::f64::consts::PI * e.life_time as f64 / 120.0) as f32).sin() * -0.5;
    let light = Vec3::new(level, level, level + 0.1);
    ctx.add_terrain_light(e.position.x, e.position.y, light, 10.0);

    // `EarthQuake = (rand()%4-2)*0.1f`.
    let quake = (ctx.rand() % 4 - 2) as f32 * 0.1;
    ctx.set_earthquake(quake);

    if ctx.rand() % 2 == 0 {
        let p = Vec3::new(0.0, rf(ctx, 150) + 300.0, 0.0);
        let angle = Vec3::new(0.0, 0.0, rf(ctx, 360));
        let position = rotate(angle, p) + e.position;
        let ty = MODEL_STONE1 + ctx.rand() % 2;
        ctx.create_effect(ty, position, e.angle, e.light, 0, None, -1, 0, 0, 0.0);
    }

    // Fire and smoke on random bones of the rock model (10002-10013): `pModel->Animation(
    // BoneTransform, o->AnimationFrame, ..., o->Angle, ...)` once, then for every bone
    // `!Bones[i].Dummy && rand()%12 == 0` (no `rand()` for dummy bones) and
    // `TransformPosition(BoneTransform[i], 0, Position, true)` (`BodyScale`/`BodyOrigin` of the
    // model's last render, i.e. this effect's `Scale` and `Position`).
    // Without the model file: [`FISSURE_BONES`] non-dummy bones (keeps the `rand()` sequence) at
    // the effect's origin.
    let dummy: Vec<bool> = match ctx.model(e.ty) {
        Some(m) => m.bones.iter().map(|b| b.dummy).collect(),
        None => vec![false; FISSURE_BONES],
    };
    let bones = ctx.effect_bones(e);
    for (i, &is_dummy) in dummy.iter().enumerate() {
        if !is_dummy && ctx.rand() % 12 == 0 {
            let position = bones.as_ref().and_then(|b| b.get(i)).map_or(e.position, |m| {
                let local = Vec3::from_array(mu_data::bmd::transform_point(m, [0.0; 3]));
                e.position + rotate(e.angle, local * e.scale)
            });
            ctx.create_particle(BITMAP_TRUE_FIRE, position, e.angle, Vec3::ONE, 3, 4.0, me(e));
            ctx.create_particle(BITMAP_SMOKE, position, e.angle, Vec3::ONE, 21, 1.0, None);
        }
    }
    move_tail(e, ctx);
}

/// `MODEL_FISSURE_LIGHT` has no `case` in `MoveEffect`: only the common tail.
fn fissure_light_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    move_tail(e, ctx);
}

// --- MODEL_BALGAS_SKILL --------------------------------------------------------------------------------

/// ZzzEffect.cpp:2132.
fn balgas_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.blend_mesh = 0;
    e.life_time = 20.0;
    e.scale = 1.0;
}

/// ZzzEffect.cpp:10392.
fn balgas_step(e: &mut Effect, ctx: &mut Ctx) {
    let lum = luminosity(e, ctx);
    e.blend_mesh_light = life(e) as f32 * 0.1;
    let light = Vec3::new(lum * 0.3, lum * 0.6, lum);
    // `b->CurrentAction = o->CurrentAction; b->PlayAnimation(..., Velocity*2, ...)` (10398-10400);
    // `Models[MODEL_BALGAS_SKILL].Actions[0].Loop = false` (ZzzOpenData.cpp:4176). Its result is
    // not used.
    let speed = e.velocity * 2.0;
    let _ = common::play_model_animation(e, ctx, speed);
    ctx.add_terrain_light(e.position.x, e.position.y, light, 3.0);
    move_tail(e, ctx);
}

// --- MODEL_SHIELD_CRASH / MODEL_SHIELD_CRASH2 ---------------------------------------------------------

/// ZzzEffect.cpp:588.
fn shield_crash_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = 24.0;
    e.scale = 1.1;
    e.gravity = e.velocity;
    e.light = Vec3::new(0.5, 0.5, 1.0);
    e.direction = e.light;
    if e.sub_type == 0 {
        if let Some((p, a)) = owner_pos_angle(e.owner, ctx) {
            ctx.create_effect(MODEL_SHIELD_CRASH2, p, a, e.light, 0, e.owner, -1, 0, 0, 0.0);
        }
    } else {
        if e.sub_type == 2 {
            e.scale = 0.7;
        }
        e.hidden_mesh = 0;
    }
}

/// Light ramp shared by both types (ZzzEffect.cpp:7109-7119 / 7171-7181): `Direction` holds the
/// base color.
fn shield_crash_light(e: &mut Effect) {
    let lt = life(e);
    if (0..8).contains(&lt) {
        let f = lt as f32 / 8.0;
        e.light = e.direction * f;
    }
    if (8..24).contains(&lt) {
        let f = 1.0 - (lt - 24) as f32 / 16.0;
        e.light = e.direction * f;
    }
}

/// ZzzEffect.cpp:7104.
fn shield_crash_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    shield_crash_light(e);
    if let Some((owner_pos, owner_angle)) = owner_pos_angle(e.owner, ctx) {
        if life(e) == 23 {
            let color = match e.sub_type {
                1 => Vec3::new(0.3, 0.3, 0.8),
                2 => Vec3::new(0.8, 0.3, 0.3),
                _ => Vec3::ONE,
            };
            ctx.create_effect(BITMAP_SHOCK_WAVE, owner_pos, owner_angle, color, 9, e.owner, -1, 0, 0, 0.0);
        }
        e.position = owner_pos;
        let mut n_pos = owner_pos;
        match e.sub_type {
            1 => {
                ctx.create_effect(MODEL_SKILL_INFERNO, n_pos, e.angle, Vec3::new(0.0, 1.0, 5.0), 10, me(e), 30, 0, 0, 0.0);
                n_pos.z += 120.0;
                ctx.create_particle(BITMAP_SMOKE, n_pos, e.angle, e.light, 11, 2.0, None);
            }
            2 => {
                ctx.create_effect(MODEL_SKILL_INFERNO, n_pos, e.angle, Vec3::new(0.8, 0.3, 0.3), 10, me(e), 30, 0, 0, 0.0);
            }
            0 => {
                ctx.create_effect(MODEL_SKILL_INFERNO, n_pos, e.angle, Vec3::new(0.0, 0.8, 1.5), 2, me(e), 30, 0, 0, 0.0);
                n_pos.z += 120.0;
                ctx.create_particle(BITMAP_SMOKE, n_pos, e.angle, e.light, 11, 2.0, None);
            }
            _ => {}
        }
    }
    // The common tail then moves it by `Direction` (the color) rotated by `Angle`, as the original.
    move_tail(e, ctx);
}

/// ZzzEffect.cpp:611.
fn shield_crash2_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 24.0;
    e.scale = 1.1;
    e.gravity = e.velocity;
    e.light = Vec3::new(0.5, 0.5, 1.0);
    e.direction = e.light;
}

/// ZzzEffect.cpp:7168.
fn shield_crash2_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    shield_crash_light(e);
    if let Some(p) = ctx.target_position(e.owner) {
        e.position = p;
    }
    move_tail(e, ctx);
}

// --- MODEL_BLADE_SKILL ------------------------------------------------------------------------------

/// ZzzEffect.cpp:636. `user[0]` = `m_sTargetIndex` (the `sTargetIndex` argument, -1 by default;
/// no caller passes one).
fn blade_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 10.0;
    e.scale = 1.5;
    if e.sub_type == 1 {
        e.scale = 1.0;
        e.life_time = 14.0;
    }
    e.user[0] = -1.0;
}

/// ZzzEffect.cpp:7223.
fn blade_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    if e.sub_type == 0 {
        e.scale -= 0.1;
        if e.scale < 0.8 {
            e.scale = 0.8;
        }
    } else if e.sub_type == 1 {
        e.blend_mesh = -2;
        e.blend_mesh_light = life(e) as f32 / 14.0;
        e.alpha = e.blend_mesh_light;

        let lt = life(e);
        if lt >= 3
            && lt % 2 == 0
            && let Some((owner_pos, owner_angle)) = owner_pos_angle(e.owner, ctx)
        {
            // `CHARACTER *c = &CharactersClient[o->PKKey]` (only used by CreateArrow below).
            let previous = e.user[0] as i32;
            let mut ran = ctx.rand() % 6;
            if previous > -1 {
                loop {
                    ran = ctx.rand() % 6;
                    if ran != previous {
                        break;
                    }
                }
            }
            e.user[0] = ran as f32;
            let turn = match ran {
                0 => 7.0,
                1 => 21.0,
                2 => 35.0,
                3 => -7.0,
                4 => -21.0,
                _ => -35.0,
            };
            // TODO(original): `o->Owner->Angle[2] = Angle[2] + turn` while the arrows below are
            // created, restored afterwards (actors are read-only snapshots here). The trails
            // below use the turned angle, like the original.
            let mut turned = owner_angle;
            turned.z = owner_angle.z + turn;
            for _ in 0..3 {
                let mut p = e.position;
                p.y += (ctx.rand() % 200 - 100) as f32;
                p.z += (ctx.rand() % 200 - 100) as f32;
                let scale = (ctx.rand() % 8 + 8) as f32 * 0.2;
                let rotation = rf(ctx, 360);
                ctx.create_sprite(BITMAP_SHINY + 1, p, scale, Vec3::new(0.2, 0.3, 1.0), rotation, 0);
                let scale = (ctx.rand() % 8 + 8) as f32 * 0.07;
                let rotation = rf(ctx, 360);
                ctx.create_sprite(BITMAP_SHINY + 1, p, scale, Vec3::ONE, rotation, 0);
                let mut p = owner_pos;
                p.z += ((ctx.rand() % 80 - 40) + 200) as f32;
                ctx.create_joint(BITMAP_SPARK + 1, p, p, turned, 1, e.owner, 18.7);
            }
            // TODO(original): CreateArrow(c, o->Owner, NULL, FindHotKey(o->Skill), 1, 0): the
            // owner shoots its bow's arrows towards the turned angle (needs the character's
            // equipment and skill tables, not available to effects).
        }
    }
    move_tail(e, ctx);
}

/// Default RenderObject → ZzzObject.cpp:1649: `BodyLight = Light` and three passes of
/// `RenderBody(RENDER_TEXTURE|RENDER_BRIGHT, ...)`.
fn blade_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    if let Some(mut d) = object(e) {
        d.render_flags = RENDER_TEXTURE | RENDER_BRIGHT;
        for _ in 0..3 {
            out.models.push(d);
        }
    }
}

// --- MODEL_BLOW_OF_DESTRUCTION -------------------------------------------------------------------------

/// ZzzEffect.cpp:5024. SubType 0: the caller passes the target point in `Light`
/// (ZzzCharacter.cpp:3962-3966), kept in `StartPosition`.
fn blow_create(e: &mut Effect, ctx: &mut Ctx) {
    match e.sub_type {
        0 => {
            e.life_time = 40.0;
            if let Some((_, owner_angle)) = owner_pos_angle(e.owner, ctx) {
                e.position += rotate(owner_angle, Vec3::new(-20.0, -100.0, 0.0));
            }
            e.start_position = e.light;
            e.light = Vec3::splat(1.2);
            ctx.create_effect(MODEL_BLOW_OF_DESTRUCTION, e.start_position, e.angle, e.light, 1, e.owner, -1, 0, 0, 0.0);
        }
        1 => {
            e.life_time = 40.0;
            e.light = Vec3::splat(1.2);
            e.position.z = 150.0;
            e.scale = 5.0;
        }
        2 => {
            // PBG_ADD_NEWCHAR_MONK_SKILL (Rage Fighter).
            e.life_time = 50.0;
            // `Position += Owner->Angle · (0, 0, 0)`: no change.
            // `if (Owner->m_sTargetIndex < 0) break;` then StartPosition = the target's position
            // and Scale = 0.5. A target that is not in scope counts as no target.
            let target = ctx.actor(e.owner).and_then(|a| a.target).and_then(|t| ctx.actors.get(&t)).map(|t| t.position);
            if let Some(p) = target {
                e.start_position = p;
                e.scale = 0.5;
            }
        }
        _ => {}
    }
}

/// ZzzEffect.cpp:15146.
fn blow_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    let lt = life(e);
    match e.sub_type {
        0 => {
            if lt <= 24 {
                if lt >= 15 {
                    let quake = (ctx.rand() % 8 - 4) as f32 * 0.1;
                    ctx.set_earthquake(quake);
                }
                // (else `EarthQuake = 0`: the engine keeps the strongest shake of the tick.)
                if lt == 23 {
                    let light = Vec3::new(0.3, 0.3, 1.0);
                    ctx.create_effect(MODEL_NIGHTWATER_01, e.position, e.angle, light, 0, me(e), -1, 0, 0, 0.0);
                    ctx.create_effect(MODEL_NIGHTWATER_01, e.position, e.angle, light, 0, me(e), -1, 0, 0, 0.0);
                    ctx.create_effect(MODEL_KNIGHT_PLANCRACK_A, e.position, e.angle, light, 0, me(e), 0, 0, 0, 1.2);

                    let dir = e.start_position - e.position;
                    let length = dir.length();
                    let dir = normalize(dir);
                    let num = (length / 100.0) as i32 + 1;
                    let owner_angle = owner_pos_angle(e.owner, ctx).map(|(_, a)| a);
                    for i in 0..num {
                        let pos = e.position + dir * (55.0 * i as f32);
                        let turn = (ctx.rand() % 20 + 10) as f32;
                        if let Some(mut angle) = owner_angle {
                            if i % 2 == 0 {
                                angle.z += turn;
                            } else {
                                angle.z -= turn;
                            }
                            ctx.create_effect(MODEL_KNIGHT_PLANCRACK_B, pos, angle, light, 0, me(e), 0, 0, 0, 1.0);
                        }
                    }
                }
                e.light /= 1.05;
            }
        }
        1 => {
            if lt <= 24 {
                if lt >= 15 {
                    for _ in 0..15 {
                        let mut pos = e.position;
                        pos.x += (ctx.rand() % 300 - 150) as f32;
                        pos.y += (ctx.rand() % 300 - 150) as f32;
                        pos.z += (ctx.rand() % 300 - 150) as f32;
                        let scale = 1.6 + (ctx.rand() % 10) as f32 * 0.1;
                        let light = Vec3::new(0.5, 0.5, 1.0);
                        let index = if ctx.rand() % 2 == 0 { BITMAP_WATERFALL_5 } else { BITMAP_WATERFALL_3 };
                        ctx.create_particle(index, pos, e.angle, light, 8, scale, None);
                        if ctx.rand() % 2 == 0 {
                            ctx.create_particle(BITMAP_SMOKE, pos, e.angle, Vec3::ONE, 55, 1.0, None);
                        }
                    }
                }
                if lt == 23 {
                    let light = Vec3::new(0.3, 0.3, 1.0);
                    ctx.create_effect(MODEL_NIGHTWATER_01, e.position, e.angle, light, 0, me(e), -1, 0, 0, 2.0);
                    ctx.create_effect(MODEL_NIGHTWATER_01, e.position, e.angle, light, 0, me(e), -1, 0, 0, 1.0);
                    ctx.create_effect(MODEL_RAKLION_BOSS_CRACKEFFECT, e.position, e.angle, light, 0, me(e), -1, 0, 0, 0.2);
                    // `for (i = 0; i < 5 + rand()%3; ++i)`: the bound is drawn again on every check.
                    let mut i = 0;
                    while i < 5 + ctx.rand() % 3 {
                        let p = Vec3::new(0.0, rf(ctx, 150), 0.0);
                        let angle = Vec3::new(0.0, 0.0, rf(ctx, 360));
                        let result = rotate(angle, p) + e.position;
                        let ty = MODEL_STONE1 + ctx.rand() % 2;
                        ctx.create_effect(ty, result, e.angle, e.light, 13, None, -1, 0, 0, 0.0);
                        i += 1;
                    }
                }
                e.light /= 1.05;
            }
        }
        2 => {
            // PBG_ADD_NEWCHAR_MONK_SKILL (Rage Fighter).
            if (15..=35).contains(&lt) {
                let quake = (ctx.rand() % 8 - 4) as f32 * 0.1;
                ctx.set_earthquake(quake);
            }
            if lt == 30 {
                let light = Vec3::ONE;
                let dir = normalize(e.start_position - e.position);
                let pos = e.position + dir * 55.0;
                let turn = (ctx.rand() % 40 + 140) as f32;
                if let Some((_, mut angle)) = owner_pos_angle(e.owner, ctx) {
                    angle.z += turn;
                    ctx.create_effect(MODEL_DRAGON_LOWER_DUMMY, pos, angle, light, 0, me(e), 0, 0, 0, 0.8);
                    ctx.create_effect(BITMAP_LIGHT_RED, pos, angle, light, 4, me(e), -1, 0, 0, 3.0);
                }
            }
        }
        _ => {}
    }
    move_tail(e, ctx);

    // RenderEffects 18760 (SubType 0): a sprite whose scale uses rand(), drawn by the render that
    // follows this tick (LifeTime already decremented).
    let render_life = life(e) - 1;
    if e.live && e.sub_type == 0 && render_life > 0 && render_life <= 24 {
        let mut pos = e.position;
        pos.z += 65.0;
        let scale = ((ctx.rand() % 10) as f32 * 0.011) + 3.0;
        ctx.create_sprite(BITMAP_SWORD_EFFECT_MONO, pos, scale, Vec3::new(0.5, 0.5, 1.0), 0.0, 0);
    }
}

/// RenderEffects 18760 (SubType 1; SubType 0's sprite is created at the end of `blow_step`) and
/// RenderEffectShadows 19604 (`EnableAlphaBlend` + `RenderTerrainAlphaBitmap(BITMAP_FLARE_BLUE)`).
/// No model is loaded for this type.
fn blow_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    if life(e) > 24 {
        return;
    }
    match e.sub_type {
        0 => {
            common::terrain_bitmap(out, BITMAP_FLARE_BLUE, e.position.x, e.position.y, 4.0, 4.0, e.light, -e.angle.z, 1.0, 5.0, Blend::Glow);
        }
        1 => {
            let light = Vec3::new(e.light.x * 0.5, e.light.y * 0.5, e.light.z * 1.0);
            let mut pos = e.position;
            pos.z += 100.0;
            common::sprite(out, ctx, BITMAP_LIGHT, pos, e.scale, light, 0.0, Some(Blend::Glow));
            common::terrain_bitmap(out, BITMAP_FLARE_BLUE, e.position.x, e.position.y, 6.0, 6.0, e.light, -e.angle.z, 1.0, 5.0, Blend::Glow);
        }
        _ => {}
    }
}

// --- MODEL_NIGHTWATER_01 / MODEL_KNIGHT_PLANCRACK_A / MODEL_KNIGHT_PLANCRACK_B ------------------------

/// ZzzEffect.cpp:5064.
fn nightwater_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = 25.0;
    e.alpha = 1.0;
    e.angle.z = rf(ctx, 360);
}

/// ZzzEffect.cpp:5071. `o->Scale = Scale + rand()%10*0.05f` uses the `Scale` argument; the engine
/// already turned a 0 argument into 0.9, but every caller passes a positive one.
fn plancrack_a_create(e: &mut Effect, ctx: &mut Ctx) {
    if e.sub_type == 0 || e.sub_type == 1 {
        e.life_time = if e.sub_type == 0 { 25.0 } else { 20.0 };
        e.alpha = 1.0;
        e.angle.z = rf(ctx, 360);
        e.scale += (ctx.rand() % 10) as f32 * 0.05;
        e.position.z += 10.0;
    }
}

/// ZzzEffect.cpp:5089.
fn plancrack_b_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 25.0;
    e.alpha = 1.0;
    e.position.z += 15.0;
    e.angle.z += 90.0;
}

/// `MODEL_NIGHTWATER_01` (15284) and `MODEL_KNIGHT_PLANCRACK_B` (15303): `Alpha -= 0.04`.
fn fade_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    e.alpha -= 0.04;
    move_tail(e, ctx);
}

/// ZzzEffect.cpp:15289.
fn plancrack_a_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    if e.sub_type == 0 {
        e.alpha -= 0.04;
    } else if e.sub_type == 1 {
        e.alpha *= 0.9;
    }
    move_tail(e, ctx);
}

/// RenderEffects 18787 → ZzzObject.cpp:594: `BodyLight = Light · Alpha` and
/// `RenderBody(RENDER_TEXTURE|RENDER_BRIGHT, Alpha, BlendMesh, BlendMeshLight, U, V)`.
fn nightwater_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    if let Some(mut d) = object(e) {
        d.light = e.light * e.alpha;
        d.hidden_mesh = -1;
        d.render_flags = RENDER_TEXTURE | RENDER_BRIGHT;
        out.models.push(d);
    }
}

/// RenderEffects 18788 → ZzzObject.cpp:600/606: `BodyLight = Light · Alpha` and
/// `RenderBody(RENDER_TEXTURE|RENDER_BRIGHT, Alpha, 1, BlendMeshLight, U,
/// -(int)WorldTime%2000*0.0001f)`.
fn plancrack_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    let Some(mut d) = object(e) else { return };
    d.light = e.light * e.alpha;
    d.hidden_mesh = -1;
    d.render_flags = RENDER_TEXTURE | RENDER_BRIGHT;
    d.blend_mesh = 1;
    // `-(int)WorldTime%2000*0.0001f`: integer negate and remainder, then the float product.
    let v = ((-(ctx.world_time as i32)) % 2000) as f32 * 0.0001;
    d.blend_mesh_tex_coord = Vec2::new(e.blend_mesh_tex_coord.x, v);
    out.models.push(d);
}

// --- MODEL_EFFECT_FLAME_STRIKE --------------------------------------------------------------------------

/// ZzzEffect.cpp:5097. `user[0]` = `m_iAnimation` (blur ids).
fn flame_strike_create(e: &mut Effect, ctx: &mut Ctx) {
    if e.sub_type == 0 {
        e.alpha = 0.0;
        e.life_time = 35.0;
        // `Velocity = Models[Owner->Type].Actions[PLAYER_SKILL_FLAMESTRIKE].PlaySpeed`: `0.69 +
        // MagicSpeed2` as `SetAttackSpeed` sets it (ZzzCharacter.cpp:870; combat.rs). Its owner is a player.
        e.velocity = crate::combat::attack_play_speed(PLAYER_SKILL_FLAMESTRIKE as usize).unwrap_or(0.69);
        e.ai = 0;
        e.user[0] = ctx.rand() as f32;
    }
}

/// ZzzEffect.cpp:15308.
/// TODO(original): `EffectDestructor` (ZzzEffect.cpp:235) removes this effect's object blurs 1-3
/// (`RemoveObjectBlurs`) when it dies; there is no object blur system.
fn flame_strike_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    if e.sub_type == 0 {
        let owner = ctx.actor(e.owner).map(|a| (a.frame, a.action));
        match owner {
            None => e.live = false,
            Some(_) if e.life_time < 20.0 && e.alpha < 0.1 => e.live = false,
            Some((owner_frame, owner_action)) => {
                if e.life_time < 20.0 {
                    e.alpha -= 0.1;
                } else if e.alpha < 1.0 {
                    e.alpha += 0.1;
                }
                // Blur window: owner frames 5..=13.
                const END_FRAME: f32 = 13.0;
                if owner_frame > END_FRAME || e.ai == 1 {
                    e.ai = 1;
                } else if owner_action == PLAYER_SKILL_FLAMESTRIKE as usize {
                    e.animation_frame = owner_frame;
                    // TODO(original): 10 sub-steps between the previous and current frames of the
                    // owner (bone 33 as parent) and of this model: while the owner's frame is in
                    // 5..=13, CreateObjectBlur(o, bone 9, bone 6, (1,1,1), 2, false, m_iAnimation+1
                    // and +2) and CreateObjectBlur(o, bone 8, bone 5, (1,1,1), 5, false,
                    // m_iAnimation+3). There is no object blur system.
                }
            }
        }
    }
    move_tail(e, ctx);
}

/// RenderEffects 18794: the model hangs from the owner's bone 33 (`RotationPosition` sets the
/// parent matrix; `BodyOrigin = Owner->Position`; `Angle = 0`), mesh 0 with
/// `RENDER_TEXTURE|RENDER_BRIGHT` and BlendMesh 0 (`BodyLight · Alpha`), mesh 1 with BlendMesh 1
/// and V = `(((int)(WorldTime*0.05)%16)/4)*0.25f`.
fn flame_strike_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    if e.sub_type != 0 {
        return;
    }
    let Some(owner) = common::owner_actor(e.owner, ctx.actors) else { return };
    let Some(bone) = owner.bones.get(33) else { return };
    // World transform = owner position + owner rotation · bone 33 · (Scale · local).
    let bone_rot = Mat3::from_cols(
        Vec3::new(bone[0][0], bone[1][0], bone[2][0]),
        Vec3::new(bone[0][1], bone[1][1], bone[2][1]),
        Vec3::new(bone[0][2], bone[1][2], bone[2][2]),
    );
    let bone_t = Vec3::new(bone[0][3], bone[1][3], bone[2][3]);
    let rot = angle_matrix(owner.angle) * bone_rot;
    let (z, y, x) = Quat::from_mat3(&rot).normalize().to_euler(EulerRot::ZYX);
    let angle = Vec3::new(x, y, z) * (180.0 / std::f32::consts::PI);
    let position = owner.position + rotate(owner.angle, bone_t * e.scale);

    // `RenderMesh(0, RENDER_TEXTURE|RENDER_BRIGHT, Alpha, 0, Alpha, U, V, -1)`.
    let mut mesh0 = ModelDraw::new(MODEL_EFFECT_FLAME_STRIKE, position, angle, e.scale);
    mesh0.light = e.light;
    mesh0.alpha = e.alpha;
    mesh0.action = e.current_action;
    mesh0.frame = e.animation_frame;
    mesh0.render_flags = RENDER_TEXTURE | RENDER_BRIGHT;
    mesh0.mesh = 0;
    mesh0.blend_mesh = 0;
    mesh0.blend_mesh_light = e.alpha;
    mesh0.blend_mesh_tex_coord = e.blend_mesh_tex_coord;
    out.models.push(mesh0);

    // `RenderMesh(1, RENDER_TEXTURE|RENDER_BRIGHT, Alpha, 1, Alpha, U, fV)`.
    let fv = ((((ctx.world_time as f64 * 0.05) as i32) % 16) / 4) as f32 * 0.25;
    let mut mesh1 = mesh0;
    mesh1.mesh = 1;
    mesh1.blend_mesh = 1;
    mesh1.blend_mesh_tex_coord = Vec2::new(e.blend_mesh_tex_coord.x, fv);
    out.models.push(mesh1);
}

// --- MODEL_STAR_SHINE -------------------------------------------------------------------------------

/// ZzzEffect.cpp:4929.
fn star_shine_create(e: &mut Effect, ctx: &mut Ctx) {
    if e.sub_type == 0 {
        e.life_time = 30.0;
        e.alpha = 0.2;
        e.angle.x = rf(ctx, 360);
    }
}

/// ZzzEffect.cpp:15562.
fn star_shine_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    if e.sub_type == 0 {
        let lt = life(e);
        if lt <= 10 {
            e.scale *= 0.9;
            e.alpha *= 0.9;
            e.light *= 0.9;
        } else if lt <= 20 {
            ctx.create_sprite(BITMAP_SHINY, e.position, e.scale, e.light, e.angle.x, 0);
        } else if lt <= 30 {
            e.scale *= 1.1;
            e.alpha *= 1.1;
            e.light *= 1.1;
        }
        ctx.create_sprite(BITMAP_SHINY, e.position, e.scale, e.light, e.angle.x, 0);
    }
    move_tail(e, ctx);
}

// --- MODEL_FEATHER / MODEL_FEATHER_FOREIGN ----------------------------------------------------------

/// `EyeRight` → `user[0..3]`.
fn eye_right(e: &Effect) -> Vec3 {
    Vec3::new(e.user[0], e.user[1], e.user[2])
}

/// The scatter shared by both feathers (ZzzEffect.cpp:4950-4964 / 4993-5007): a random offset,
/// `Direction` back towards the origin and the scale jitter.
fn feather_scatter(e: &mut Effect, ctx: &mut Ctx) {
    let origin = e.position;
    e.position.x += (ctx.rand() % 20 - 10) as f32 * 4.0;
    e.position.y += (ctx.rand() % 20 - 10) as f32 * 4.0;
    e.position.z += (ctx.rand() % 20 - 10) as f32 * 4.0;
    e.direction = normalize(origin - e.position);
    // `int iAddDirection = (rand()%10-5)*0.08f`: truncated to an int, always 0.
    let add = ((ctx.rand() % 10 - 5) as f32 * 0.08) as i32 as f32;
    e.direction += Vec3::splat(add);
    e.scale += (ctx.rand() % 20 - 10) as f32 * (e.scale * 0.03);
}

/// Random spin and `EyeRight` (angle added every tick).
fn feather_spin(e: &mut Effect, ctx: &mut Ctx) {
    e.angle.x = rf(ctx, 360);
    e.angle.y = rf(ctx, 360);
    e.angle.z = rf(ctx, 360);
    e.user[0] = (ctx.rand() % 10 - 5) as f32;
    e.user[1] = (ctx.rand() % 10 - 5) as f32;
    e.user[2] = (ctx.rand() % 10 - 5) as f32;
}

/// ZzzEffect.cpp:4941.
fn feather_create(e: &mut Effect, ctx: &mut Ctx) {
    if (0..=3).contains(&e.sub_type) {
        feather_scatter(e, ctx);
        e.life_time = (30 + (ctx.rand() % 20 - 10)) as f32;
        if e.sub_type == 2 || e.sub_type == 3 {
            e.life_time = 100.0;
        }
        e.alpha = 0.6 + (ctx.rand() % 10) as f32 * 0.02;
        e.gravity = 0.1;
        if e.sub_type == 2 || e.sub_type == 3 {
            e.gravity = 0.0;
        }
        feather_spin(e, ctx);
    }
}

/// ZzzEffect.cpp:15590.
fn feather_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    if (0..=3).contains(&e.sub_type) {
        e.light *= 0.97;
        e.alpha *= 0.97;
        match e.sub_type {
            2 | 3 => e.scale *= 0.97,
            1 => e.scale *= 0.99,
            _ => {}
        }
        e.angle += eye_right(e);
    }
    move_tail(e, ctx);
}

/// ZzzEffect.cpp:4987.
fn feather_foreign_create(e: &mut Effect, ctx: &mut Ctx) {
    if e.sub_type == 4 {
        feather_scatter(e, ctx);
        e.life_time = 25.0;
        e.alpha = 0.3;
        e.gravity = 0.1;
        feather_spin(e, ctx);
    }
}

/// ZzzEffect.cpp:12237.
fn feather_foreign_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    if e.sub_type == 4 {
        e.light *= 0.97;
        e.alpha *= 0.97;
        e.angle += eye_right(e);
    }
    move_tail(e, ctx);
}

/// Default RenderObject → ZzzObject.cpp:1822 / 1906: SubType 2/3 `RenderBody(RENDER_TEXTURE|
/// RENDER_DARK, Alpha, BlendMesh, BlendMeshLight, U, V, HiddenMesh)` (subtractive); the others
/// `RenderMesh(0, RENDER_TEXTURE, Alpha, 0, Alpha, U, V, HiddenMesh)` (BlendMesh 0 with
/// `BlendMeshLight = Alpha`; `HiddenMesh` lands in `RenderMesh`'s texture argument, -1 = none).
fn feather_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    let Some(mut d) = object(e) else { return };
    if e.sub_type == 2 || e.sub_type == 3 {
        d.render_flags = RENDER_TEXTURE | RENDER_DARK;
    } else {
        d.render_flags = RENDER_TEXTURE;
        d.mesh = 0;
        d.blend_mesh = 0;
        d.blend_mesh_light = e.alpha;
        d.texture = e.hidden_mesh;
        d.hidden_mesh = -1;
    }
    out.models.push(d);
}

// --- MODEL_MOVE_TARGETPOSITION_EFFECT -------------------------------------------------------------------

/// ZzzEffect.cpp:4825.
fn move_target_create(e: &mut Effect, ctx: &mut Ctx) {
    if e.sub_type != 0 {
        return;
    }
    e.life_time = 30.0;
    e.blend_mesh = 0;
    let light = Vec3::new(1.0, 0.7, 0.3);
    let mut pos = e.position;
    pos.z += 85.0;

    // DeleteEffect(BITMAP_MAGIC, NULL, 11), DeleteEffect(BITMAP_TARGET_POSITION_EFFECT2, NULL, 0),
    // DeleteEffect(BITMAP_TARGET_POSITION_EFFECT1, NULL, 0) and DeleteJoint(MODEL_SPEARSKILL, NULL,
    // 16) (a NULL joint target matches any). The engine applies deletions at the end of the tick,
    // which would also remove what is created below, so:
    // - what is alive since tick start: the effects by id, and the trails of the markers (their
    //   only creator is this code) by target;
    // - what was created earlier in this tick (e.g. by the previous marker's `step`, which runs
    //   before click requests) is not in `Effects[]` yet: it is dropped from the pending spawns.
    let deleted = |ty: i32, sub_type: i32, owner: Option<Target>| {
        owner.is_none() && ((ty == BITMAP_MAGIC && sub_type == 11) || ((ty == BITMAP_TARGET_POSITION_EFFECT2 || ty == BITMAP_TARGET_POSITION_EFFECT1) && sub_type == 0))
    };
    let old: Vec<(u32, i32)> = ctx
        .effects
        .iter()
        .filter(|i| deleted(i.ty, i.sub_type, i.owner) || i.ty == MODEL_MOVE_TARGETPOSITION_EFFECT)
        .map(|i| (i.id, i.ty))
        .collect();
    for (id, ty) in old {
        if ty == MODEL_MOVE_TARGETPOSITION_EFFECT {
            ctx.delete_joint(MODEL_SPEARSKILL, Some(Target::Effect(id)), 16);
        } else {
            ctx.delete_effect_id(id);
        }
    }
    // Their pool slots become free again, as `EffectDestructor` / `DeleteJoint` free them.
    let (mut freed_joints, mut freed_effects) = (0, 0);
    ctx.spawns.retain(|s| {
        let drop = match s {
            Spawn::Effect(x) => deleted(x.ty, x.sub_type, x.owner),
            Spawn::Joint(j) => j.ty == MODEL_SPEARSKILL && j.sub_type == 16,
            Spawn::Particle(_) => false,
        };
        match s {
            Spawn::Effect(_) if drop => freed_effects += 1,
            Spawn::Joint(_) if drop => freed_joints += 1,
            _ => {}
        }
        !drop
    });
    ctx.free[1] += freed_joints;
    ctx.free[2] += freed_effects;

    ctx.create_effect(BITMAP_MAGIC, e.position, e.angle, light, 11, None, -1, 0, 0, 0.0);
    ctx.create_effect(BITMAP_TARGET_POSITION_EFFECT2, e.position, e.angle, light, 0, None, -1, 0, 0, 0.0);
    for _ in 0..4 {
        ctx.create_joint(MODEL_SPEARSKILL, pos, pos, e.angle, 16, me(e), 5.0);
    }
}

/// ZzzEffect.cpp:14890.
fn move_target_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    if e.sub_type == 0 {
        let light = Vec3::new(1.0, 0.7, 0.3);
        let mut pos = e.position;
        pos.z += 110.0;
        ctx.create_particle(BITMAP_SPARK + 1, pos, e.angle, light, 24, 1.0, me(e));
        if life(e) % 15 == 0 {
            ctx.create_effect(BITMAP_TARGET_POSITION_EFFECT1, e.position, Vec3::ZERO, light, 0, None, -1, 0, 0, 0.0);
        }
        if life(e) <= 10 {
            e.blend_mesh_light -= 0.05;
            e.light *= e.blend_mesh_light;
        }
    }
    move_tail(e, ctx);
}

/// RenderEffects 18749 → ZzzObject.cpp:1801: `BodyLight = (1, 0.7, 0.3)` and `RenderMesh(0,
/// RENDER_TEXTURE|RENDER_BRIGHT, Alpha, BlendMesh, BlendMeshLight)` (U, V = 0; `o->Light` is not
/// used).
fn move_target_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    if let Some(mut d) = object(e) {
        d.light = Vec3::new(1.0, 0.7, 0.3);
        d.render_flags = RENDER_TEXTURE | RENDER_BRIGHT;
        d.mesh = 0;
        d.hidden_mesh = -1;
        d.blend_mesh_tex_coord = Vec2::ZERO;
        out.models.push(d);
    }
}

// --- MODEL_EFFECT_SD_AURA ---------------------------------------------------------------------------

/// ZzzEffect.cpp:6070.
fn sd_aura_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 1000.0;
    e.scale = 1.0;
}

/// ZzzEffect.cpp:17004: follows the owner's bone 18 and never dies (WSclient deletes it when the
/// SD buff ends). A missing owner would crash the original; here the effect ends.
fn sd_aura_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    match ctx.actor(e.owner) {
        Some(owner) => {
            let mut pos = owner.bone_world(18, Vec3::ZERO);
            pos.z -= 30.0;
            e.position = pos;
            e.life_time = 100.0;
        }
        None => e.live = false,
    }
    move_tail(e, ctx);
}

/// Default RenderObject → ZzzObject.cpp:2402: `RenderBody(RENDER_TEXTURE|RENDER_BRIGHT, Alpha,
/// BlendMesh 0, BlendMeshLight, U, WorldTime*0.0006f)`: its only mesh is the blend mesh (additive
/// `BodyLight · BlendMeshLight`, scrolling V).
fn sd_aura_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    if let Some(mut d) = object(e) {
        d.render_flags = RENDER_TEXTURE | RENDER_BRIGHT;
        d.hidden_mesh = -1;
        d.blend_mesh = 0;
        d.blend_mesh_tex_coord = Vec2::new(e.blend_mesh_tex_coord.x, ctx.world_time * 0.0006);
        out.models.push(d);
    }
}

// --- MODEL_AURORA -------------------------------------------------------------------------------------

/// ZzzEffect.cpp:4350.
fn aurora_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 100.0;
    e.scale = e.pk_key as f32 / 100.0;
    e.blend_mesh = 0;
    e.blend_mesh_light = 0.0;
}

/// ZzzEffect.cpp:14257.
fn aurora_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    if owner_live(e, ctx) {
        // TODO(original): HiddenMesh = -1 while Castle Siege runs (battleCastle::IsBattleCastleStart());
        // there is no siege state, so it is the "not started" case.
        e.hidden_mesh = -2;
        e.life_time = 2.0;
        e.blend_mesh_light = (ctx.world_time() * 0.001).sin() * 0.1 + 0.2;
    } else {
        e.live = false;
    }
    e.blend_mesh_tex_coord.x = ctx.world_time() * 0.0005;
    move_tail(e, ctx);
}
