//! Helpers shared by the `Effects[]` kinds, translated from Main 5.2 `ZzzEffect.cpp`: the start of
//! `MoveEffect` (`Luminosity`), its common tail (`PlayAnimation` + `MoveParticle`), `MoveJump`,
//! `CreateBomb`/`CreateBomb2`/`CreateBomb3`, `CreateInferno`, `CreateHealing`, `CreateForce`, and
//! the drawing helpers for `RenderObject` and `RenderTerrainAlphaBitmap`.
//!
//! Older kind files keep private copies of some of these; new code uses this module.

use bevy::math::{Vec2, Vec3};
use bevy::prelude::Entity;

use crate::fx::core::*;

/// `rand() % n` as `f32`.
pub fn rf(ctx: &mut Ctx, n: i32) -> f32 {
    (ctx.rand() % n) as f32
}

/// `Luminosity` from the start of `MoveEffect` (ZzzEffect.cpp:6923-6928). It always consumes one
/// `rand()`, like the original, so call it first in every `step`.
pub fn luminosity(e: &Effect, ctx: &mut Ctx) -> f32 {
    let mut l = (ctx.rand() % 4 + 7) as f32 * 0.1;
    if e.life_time < 5.0 {
        l -= (5.0 - e.life_time) * 0.2;
        if l < 0.0 {
            l = 0.0;
        }
    }
    l
}

/// `MoveParticle(o, Turn)` (ZzzEffect.cpp:6489): `Direction` rotated by `Angle` (`turn`) or as is.
pub fn move_particle(e: &mut Effect, turn: bool) {
    if turn {
        e.position += rotate(e.angle, e.direction);
    } else {
        e.position += e.direction;
    }
}

/// `MoveParticle(o, angle)` (ZzzEffect.cpp:6507): `Direction` rotated by another angle.
pub fn move_particle_angle(e: &mut Effect, angle: Vec3) {
    e.position += rotate(angle, e.direction);
}

/// `b->PlayAnimation(...)` of the common tail (ZzzEffect.cpp:18073-18081), only for
/// `MODEL_BIRD01 <= Type < MODEL_SKILL_END` and while `CurrentAction != 255`: at the end of the
/// action the frame stays on `NumAnimationKeys` and `CurrentAction` becomes 255.
pub fn play_animation(e: &mut Effect, ctx: &mut Ctx) {
    if (MODEL_BIRD01..MODEL_SKILL_END).contains(&e.ty) && e.current_action != 255 {
        let speed = e.velocity;
        if !play_model_animation(e, ctx, speed)
            && let Some(frames) = ctx.action_frames(e.ty, e.current_action)
        {
            e.animation_frame = frames as f32;
            e.current_action = 255;
        }
    }
}

/// `Models[o->Type].PlayAnimation(&o->AnimationFrame, ..., speed, ...)` (ZzzBMD.cpp:406-478):
/// `false` when the action ends or wraps. Without the model file (or with an action it does not
/// have) the original returns `true`; without the file the frame still advances so the kind keeps
/// running.
pub fn play_model_animation(e: &mut Effect, ctx: &mut Ctx, speed: f32) -> bool {
    let Some(m) = ctx.model(e.ty) else {
        e.animation_frame += speed;
        return true;
    };
    let Some(a) = m.actions.get(e.current_action) else { return true };
    // `Actions[0].Loop = true` is only set for MODEL_CUNDUN_PHOENIX among effect models
    // (MapManager.cpp:1019).
    let hold = e.ty == MODEL_CUNDUN_PHOENIX && e.current_action == 0;
    crate::fx::model_data::play_animation(&mut e.animation_frame, speed, a.frames, a.lock_positions.is_some(), hold)
}

/// The common `MoveEffect` tail for the `default:` case (ZzzEffect.cpp:18060-18124):
/// `PlayAnimation` and `MoveParticle(o, true)`. The `BITMAP_BOSS_LASER..+2` types skip the
/// `MoveParticle`; the special cases (`MODEL_SAW`, `MODEL_ARROW_HOLY`...) are done by their kinds.
pub fn move_tail(e: &mut Effect, ctx: &mut Ctx) {
    play_animation(e, ctx);
    if e.ty < BITMAP_BOSS_LASER || e.ty > BITMAP_BOSS_LASER + 2 {
        move_particle(e, true);
    }
}

/// `MoveJump` (ZzzEffect.cpp:6518): falls with gravity and bounces on the ground.
pub fn move_jump(e: &mut Effect, ctx: &mut Ctx) -> bool {
    e.position.z += e.gravity;
    e.gravity -= 1.0;
    let height = ctx.terrain_height(e.position.x, e.position.y);
    if e.position.z < height {
        e.position.z = height;
        e.gravity = -e.gravity * 0.4;
        e.direction.y *= 0.4;
        return true;
    }
    false
}

/// `gMapManager.InHellas()`: Kalima 1-6 (24..=29) and the hidden one (36).
pub fn in_hellas(world: i32) -> bool {
    (24..=29).contains(&world) || world == 36
}

/// `CreateBomb(p, Exp, SubType)` (ZzzEffect.cpp:6533).
pub fn create_bomb(ctx: &mut Ctx, p: Vec3, exp: bool, sub_type: i32) {
    let mut position = p;
    position.z += 80.0;
    let mut angle = Vec3::ZERO;
    let mut light = Vec3::ONE;
    if sub_type != 5 {
        for j in 0..20 {
            angle = Vec3::new(rf(ctx, 60) + 60.0 + 90.0, 0.0, rf(ctx, 30));
            match sub_type {
                2 => {
                    light = Vec3::new(0.7, 0.7, 1.0);
                    ctx.create_particle(BITMAP_SPARK, position, angle, light, 8, 1.0, None);
                }
                3 => {
                    light = Vec3::new(1.0, 0.7, 0.4);
                    ctx.create_particle(BITMAP_SPARK, position, angle, light, 8, 1.0, None);
                }
                4 => {
                    if j == 5 {
                        break;
                    }
                    light = Vec3::new(1.0, 0.5, 0.3);
                    ctx.create_particle(BITMAP_SPARK, position, angle, light, 8, 1.0, None);
                }
                6 => {
                    light = Vec3::splat(0.3);
                    ctx.create_particle(BITMAP_SPARK, position, angle, light, 10, 1.0, None);
                }
                _ => ctx.create_particle(BITMAP_SPARK, position, angle, light, 2, 1.0, None),
            }
        }
    }
    if exp {
        match sub_type {
            2 | 5 | 6 => {
                light = if sub_type == 6 { Vec3::splat(0.3) } else { Vec3::new(0.3, 0.6, 1.0) };
                ctx.create_particle(BITMAP_EXPLOTION_MONO, position, angle, light, 0, 1.0, None);
            }
            3 => ctx.create_particle(BITMAP_EXPLOTION_MONO, position, angle, Vec3::new(1.0, 0.6, 0.3), 0, 1.0, None),
            4 => {
                ctx.create_particle(BITMAP_EXPLOTION_MONO, position, angle, Vec3::new(1.0, 0.4, 0.2), 0, 0.5, None);
                ctx.create_particle(BITMAP_EXPLOTION_MONO, position, angle, Vec3::new(1.0, 0.6, 0.2), 0, 0.3, None);
            }
            _ => ctx.create_particle(BITMAP_EXPLOTION, position, angle, Vec3::splat(0.7), 0, 1.0, None),
        }
    }
}

/// `CreateBomb2(p, Exp, SubType, Scale)` (ZzzEffect.cpp:6607).
pub fn create_bomb2(ctx: &mut Ctx, p: Vec3, exp: bool, _sub_type: i32, scale: f32) {
    let mut position = p;
    position.z += 30.0;
    for _ in 0..20 {
        let angle = Vec3::new(rf(ctx, 60) + 60.0 + 90.0, 0.0, rf(ctx, 30));
        ctx.create_particle(BITMAP_SPARK, position, angle, Vec3::ONE, 2, 1.0, None);
    }
    if exp {
        ctx.create_particle(BITMAP_EXPLOTION_MONO, position, Vec3::ZERO, Vec3::ONE, 0, 4.0 + scale, None);
    } else {
        ctx.create_particle(BITMAP_EXPLOTION + 1, position, Vec3::ZERO, Vec3::ONE, 0, 4.0, None);
    }
}

/// `CreateBomb3(vPos, iSubType, fScale)` (ZzzEffect.cpp:6631, the `PBG_ADD_NEWCHAR_MONK_SKILL`
/// version, which MuMain compiles in). Callers of the 2-argument version pass `scale = 1.0`.
pub fn create_bomb3(ctx: &mut Ctx, pos: Vec3, sub_type: i32, scale: f32) {
    let angle = Vec3::new(rf(ctx, 60) + 60.0 + 90.0, 0.0, rf(ctx, 30));
    // The original leaves vBombPos uninitialized when the loop breaks at once; use the center.
    let mut bomb = pos;
    for i in 0..2 {
        match sub_type {
            2 => {
                if ctx.rand() % 10 == 0 {
                    break;
                }
                bomb = Vec3::new(pos.x + rf(ctx, 150) - 75.0, pos.y + rf(ctx, 150) - 75.0, pos.z + rf(ctx, 130) + 30.0);
            }
            1 => {
                if i == 1 || ctx.rand() % 5 == 0 {
                    break;
                }
                bomb = Vec3::new(pos.x + rf(ctx, 80) - 40.0, pos.y + rf(ctx, 80) - 40.0, pos.z + rf(ctx, 120) + 30.0);
            }
            3 => {
                bomb = Vec3::new(pos.x + rf(ctx, 90) - 40.0, pos.y + rf(ctx, 90) - 40.0, pos.z + rf(ctx, 100) + 70.0);
            }
            _ => {
                if i == 1 || ctx.rand() % 3 != 0 {
                    break;
                }
                bomb = Vec3::new(pos.x + rf(ctx, 30) - 15.0, pos.y + rf(ctx, 30) - 15.0, pos.z + rf(ctx, 80) + 30.0);
            }
        }
        let s = if scale != 1.0 { scale } else { 0.15 * rf(ctx, 10) };
        ctx.create_particle(BITMAP_SUMMON_SAHAMUTT_EXPLOSION, bomb, angle, Vec3::ONE, 0, s, None);
    }
    let s = if scale != 1.0 { 2.2 * scale } else { 1.0 };
    ctx.create_particle(BITMAP_MAGIC + 1, bomb, angle, Vec3::new(1.0, 0.5, 0.2), 0, s, None);
    let light = Vec3::new(1.0, 0.6, 0.2);
    for _ in 0..3 {
        let ty = MODEL_STONE1 + ctx.rand() % 2;
        if scale != 1.0 {
            ctx.create_effect(ty, bomb, angle, light, 14, None, -1, 0, 0, 2.2 * scale);
        } else {
            ctx.create_effect(ty, bomb, angle, light, 13, None, -1, 0, 0, 0.0);
        }
    }
    let n = if sub_type == 3 { 20 } else { 5 };
    for _ in 0..n {
        ctx.create_particle(BITMAP_SPARK, bomb, angle, Vec3::ONE, 2, 1.0, None);
    }
}

/// `CreateInferno(Position, SubType)` (ZzzEffect.cpp:6715): a ring of 8 bombs with stones.
pub fn create_inferno(ctx: &mut Ctx, position: Vec3, sub_type: i32) {
    for j in 0..8 {
        let p = if matches!(sub_type, 2 | 3 | 5) { Vec3::new(0.0, -240.0, 0.0) } else { Vec3::new(0.0, -220.0, 0.0) };
        let angle = Vec3::new(0.0, 0.0, j as f32 * 45.0);
        let p2 = position + rotate(angle, p);
        create_bomb(ctx, p2, true, sub_type);
        let stone = |ctx: &mut Ctx, light: Vec3, sub: i32| {
            let ty = MODEL_STONE1 + ctx.rand() % 2;
            ctx.create_effect(ty, p2, angle, light, sub, None, -1, 0, 0, 0.0);
        };
        match sub_type {
            1 => {
                stone(ctx, Vec3::ONE, 0);
                stone(ctx, Vec3::ONE, 0);
            }
            2 | 3 => {
                for _ in 0..3 {
                    stone(ctx, Vec3::ONE, 10);
                }
            }
            5 => {
                let light = Vec3::new(0.3, 0.6, 1.0);
                for _ in 0..3 {
                    let ty = MODEL_EFFECT_BROKEN_ICE0 + ctx.rand() % 3;
                    ctx.create_effect(ty, p2, angle, light, 0, None, -1, 0, 0, 0.0);
                }
            }
            _ => {
                stone(ctx, Vec3::ONE, 1);
                stone(ctx, Vec3::ONE, 0);
            }
        }
    }
}

/// `CreateHealing(o)` (ZzzEffect.cpp:191): three `BITMAP_JOINT_HEALING` trails toward the effect.
pub fn create_healing(e: &Effect, ctx: &mut Ctx) {
    let p = Vec3::new(0.0, -200.0, 0.0);
    for _ in 0..3 {
        let angle = Vec3::new(rf(ctx, 90), 0.0, rf(ctx, 360));
        let mut position = e.position - rotate(angle, p);
        position.z += 120.0;
        let sub = if e.sub_type == 5 { 11 } else { e.sub_type };
        ctx.create_joint(BITMAP_JOINT_HEALING, position, e.position, angle, sub, e.owner, 5.0);
    }
}

/// `CreateForce(o, Pos)` (MuMain ZzzEffect.cpp:146-160): three healing trails (subtype 8) toward
/// `pos`, following `o` (a character or an effect).
pub fn create_force(ctx: &mut Ctx, o: Option<Target>, pos: Vec3) {
    let p = Vec3::new(0.0, -500.0, 0.0);
    for _ in 0..3 {
        let angle = Vec3::new(rf(ctx, 90), 0.0, rf(ctx, 360));
        let mut position = pos - rotate(angle, p);
        position.z += 120.0;
        ctx.create_joint(BITMAP_JOINT_HEALING, position, pos, angle, 8, o, 10.0);
    }
}

/// `SetEarthQuakeAt`: shake that fades with the distance to the hero (12 tiles).
pub fn set_earthquake_at(ctx: &mut Ctx, pos: Vec3, magnitude: f32) {
    let Some(hero) = ctx.hero_actor().map(|h| h.position) else { return };
    const RADIUS: f32 = 100.0 * 12.0;
    let (dx, dy) = (pos.x - hero.x, pos.y - hero.y);
    let dist_sq = dx * dx + dy * dy;
    if dist_sq >= RADIUS * RADIUS {
        return;
    }
    ctx.set_earthquake(magnitude * (1.0 - dist_sq.sqrt() / RADIUS));
}

/// The owner (or any target) if it is a character, from a draw function.
pub fn owner_actor(owner: Option<Target>, actors: &std::collections::HashMap<Entity, Actor>) -> Option<&Actor> {
    match owner? {
        Target::Actor(e) => actors.get(&e),
        Target::Effect(_) => None,
    }
}

// --- Drawing ---------------------------------------------------------------------------------------

/// `RenderEffects` draws nothing for this kind (its `case` only breaks, or it is outside
/// `MODEL_SKILL_BEGIN..MODEL_SKILL_END` and has no `case`).
pub fn draw_nothing(_e: &Effect, _ctx: &DrawCtx, _out: &mut DrawList) {}

/// The `default:` of `RenderEffects` (ZzzEffect.cpp:19191): `RenderObject(o)` only for
/// `MODEL_SKILL_BEGIN <= Type < MODEL_SKILL_END`.
pub fn draw_default(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    if (MODEL_SKILL_BEGIN..MODEL_SKILL_END).contains(&e.ty) {
        out.models.push(ModelDraw::of_effect(e));
    }
}

/// `RenderObject(o)` with the effect's own state (any model type).
pub fn draw_object(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    out.models.push(ModelDraw::of_effect(e));
}

/// `RenderTerrainAlphaBitmap(Texture, x, y, SizeX, SizeY, Light, Rotation, Alpha, Height)`
/// (ZzzLodTerrain.cpp:1886): sizes in tiles, height above the ground (default 5). The original
/// rotates the texture coordinates by `Rotation`, i.e. the image by `-Rotation`. The blend is the
/// one enabled before the call (`EnableAlphaBlend` → [`Blend::Glow`], `EnableAlphaBlendMinus` →
/// [`Blend::Subtract`]...).
#[allow(clippy::too_many_arguments)]
pub fn terrain_bitmap(out: &mut DrawList, texture: i32, x: f32, y: f32, size_x: f32, size_y: f32, light: Vec3, rotation: f32, alpha: f32, height: f32, blend: Blend) {
    out.decals.push(DecalDraw {
        texture,
        center: Vec2::new(x, y),
        width: size_x * 100.0,
        height: size_y * 100.0,
        color: [light.x, light.y, light.z, alpha],
        rotation: -rotation,
        lift: height,
        blend,
    });
}

/// `CreateSprite`-like sprite drawn from a draw function (`RenderSprite`/`DrawSprite`): size is the
/// texture size times `scale`, blend by texture type (JPG additive, TGA alpha test) unless given.
#[allow(clippy::too_many_arguments)]
pub fn sprite(out: &mut DrawList, ctx: &DrawCtx, texture: i32, position: Vec3, scale: f32, light: Vec3, rotation: f32, blend: Option<Blend>) {
    let Some(t) = ctx.tex(texture) else { return };
    out.sprites.push(SpriteDraw {
        texture,
        position,
        width: t.width * scale,
        height: t.height * scale,
        color: [light.x, light.y, light.z, if t.components == 3 { 1.0 } else { light.x }],
        rotation,
        uv: [0.0, 0.0, 1.0, 1.0],
        blend: blend.unwrap_or_else(|| default_blend(t)),
    });
}
