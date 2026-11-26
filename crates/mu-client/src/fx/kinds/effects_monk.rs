//! Rage Fighter (monk) effects (`Effects[]`): Killing Blow's wolf head (`MODEL_WOLF_HEAD_EFFECT`,
//! `BITMAP_SBUMB`), Beast Uppercut's punch dummies (`MODEL_DOWN_ATTACK_DUMMY_L/R`), Chain Drive /
//! Giant Swing (`BITMAP_SWORDEFF`, `MODEL_SHOCKWAVE01`, `MODEL_SHOCKWAVE02`, `MODEL_SHOCKWAVE_SPIN01`,
//! `MODEL_WOLF_HEAD_EFFECT2`, `BITMAP_DAMAGE1`, `BITMAP_SHINY+4`, `BITMAP_SWORD_EFFECT_MONO`), the
//! party buffs (`MODEL_WINDFOCE`, `MODEL_WINDFOCE_MIRROR`, `BITMAP_LIGHT_RED`, `BITMAP_EVENT_CLOUD`),
//! Dragon Roar / Dragon Slasher (`MODEL_SHOCKWAVE_GROUND01`, `MODEL_DRAGON_KICK_DUMMY`, `BITMAP_LAVA`,
//! `MODEL_DRAGON_LOWER_DUMMY`), the target mark (`MODEL_TARGETMON_EFFECT`) and the volcano
//! (`MODEL_VOLCANO_OF_MONK`, `MODEL_VOLCANO_STONE`).
//!
//! All of it is under `#ifdef PBG_ADD_NEWCHAR_MONK_SKILL` in Main 5.2, which is compiled out there
//! but treated as ACTIVE here (MuMain compiles it in; the S6 servers have the class), together with
//! `PBG_MOD_RAGEFIGHTERSOUND`.
//!
//! Translated from Main 5.2 `ZzzEffect.cpp` (`CreateEffect` 6085-6481, `MoveEffect` 17055-18057 plus
//! the common tail 18060-18124, `RenderEffects` 19090-19190), the monk branches of `Draw_RenderObject`
//! (`ZzzObject.cpp` 2485-2611, what `RenderObject(o)` runs for these models) and the Rage Fighter
//! action speeds of `SetAttackSpeed` (`ZzzCharacter.cpp` 893-901). MuMain (`MoveHandlers.cpp`) matches.
//!
//! Engine gaps (each one has a `TODO(original)` where it is used):
//! - `o->Owner->Velocity += ..`: there is no `ActorChange` for it (the client's characters have no
//!   `o->Velocity` of their own, and the Rage Fighter skills are not cast yet). The owner angle
//!   shake of the wolf heads is reproduced locally (the original restores it in the same block).
//! - Where the original dereferences `o->Owner` without checking it (it would crash), a missing
//!   owner character kills the effect.
//! - The owner's pose at Dragon Kick's sub-frames (needs `PriorAction`/`PriorAnimationFrame`).
//!
//! Without the game data (tests) the model-dependent parts fall back: action lengths are unknown
//! (no action ends early), an effect model's own bones are taken at its origin, and the owner's
//! bone count comes from its snapshot. None of them changes the number of `rand()` calls.

use bevy::math::Vec3;

use super::common::{self, luminosity, move_tail, rf};
use crate::fx::core::*;

pub fn register(r: &mut Registry) {
    let k = |create: EffectCreate, step: EffectStep, draw: EffectDraw| EffectKind { create: Some(create), step: Some(step), draw: Some(draw) };

    r.effect(MODEL_WOLF_HEAD_EFFECT, k(wolf_head_create, wolf_head_step, draw_bright));
    r.effect(BITMAP_SBUMB, k(sbumb_create, sbumb_step, common::draw_nothing));
    r.effect(MODEL_DOWN_ATTACK_DUMMY_L, k(down_dummy_create, down_dummy_l_step, draw_bright));
    r.effect(MODEL_DOWN_ATTACK_DUMMY_R, k(down_dummy_create, down_dummy_r_step, draw_bright));
    r.effect(BITMAP_SWORDEFF, k(swordeff_create, swordeff_step, common::draw_nothing));
    r.effect(MODEL_SHOCKWAVE01, k(shockwave01_create, shockwave01_step, shockwave01_draw));
    r.effect(MODEL_SHOCKWAVE02, k(shockwave02_create, shockwave02_step, shockwave02_draw));
    r.effect(BITMAP_DAMAGE1, k(damage1_create, damage1_step, common::draw_nothing));
    r.effect(MODEL_SHOCKWAVE_SPIN01, k(spin01_create, spin01_step, spin01_draw));
    r.effect(BITMAP_EVENT_CLOUD, k(event_cloud_create, event_cloud_step, event_cloud_draw));
    r.effect(MODEL_WINDFOCE, k(windfoce_create, windfoce_step, draw_light6));
    r.effect(BITMAP_SHINY + 4, k(shiny4_create, shiny4_step, common::draw_nothing));
    r.effect(BITMAP_LIGHT_RED, k(light_red_create, light_red_step, light_red_draw));
    r.effect(MODEL_WINDFOCE_MIRROR, k(mirror_create, mirror_step, draw_light6));
    r.effect(BITMAP_SWORD_EFFECT_MONO, k(sword_mono_create, sword_mono_step, common::draw_nothing));
    r.effect(MODEL_WOLF_HEAD_EFFECT2, k(wolf_head2_create, wolf_head2_step, wolf_head2_draw));
    r.effect(MODEL_SHOCKWAVE_GROUND01, k(ground01_create, ground01_step, ground01_draw));
    r.effect(MODEL_DRAGON_KICK_DUMMY, k(dragon_kick_create, dragon_kick_step, draw_bright));
    r.effect(BITMAP_LAVA, k(lava_create, lava_step, common::draw_nothing));
    r.effect(MODEL_DRAGON_LOWER_DUMMY, k(dragon_lower_create, dragon_lower_step, dragon_lower_draw));
    // No model is loaded for MODEL_TARGETMON_EFFECT (ZzzOpenData.cpp:4427-4464): its default
    // `RenderObject` draws nothing.
    r.effect(MODEL_TARGETMON_EFFECT, k(targetmon_create, targetmon_step, common::draw_nothing));
    r.effect(MODEL_VOLCANO_OF_MONK, k(volcano_create, volcano_step, volcano_draw));
    r.effect(MODEL_VOLCANO_STONE, k(volcano_stone_create, volcano_stone_step, volcano_stone_draw));
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

/// A float assigned to the integer `LifeTime` (truncated).
fn int_life(x: f32) -> f32 {
    x as i32 as f32
}

/// `CharacterAttribute->AttackSpeed` (the hero's, from combat.rs's `SetAttackSpeed`).
fn hero_attack_speed(_ctx: &Ctx) -> f32 {
    crate::combat::hero_speeds().0 as f32
}

/// `Models[MODEL_PLAYER].Actions[a].PlaySpeed` of a Rage Fighter skill action as set by
/// `SetAttackSpeed` (ZzzCharacter.cpp:893-901): `base + AttackSpeed * 0.002`.
/// TODO(original): the original reads `Models[o->Owner->Type]`, i.e. another table for a
/// non-player owner; the player's values are used.
fn rage_speed(ctx: &Ctx, base: f32) -> f32 {
    base + hero_attack_speed(ctx) * 0.002
}

/// ZzzCharacter.cpp:895.
const SPEED_THRUST: f32 = 0.4;
/// ZzzCharacter.cpp:896.
const SPEED_STAMP: f32 = 0.4;
/// ZzzCharacter.cpp:897.
const SPEED_GIANTSWING: f32 = 0.4;
/// ZzzCharacter.cpp:898.
const SPEED_DARKSIDE_READY: f32 = 0.3;
/// ZzzCharacter.cpp:900.
const SPEED_DRAGONKICK: f32 = 0.4;
/// ZzzCharacter.cpp:901.
const SPEED_DRAGONLORE: f32 = 0.3;

/// `CharactersClient[o->Owner->m_sTargetIndex].Object.Position`, or `None` for
/// `o->Owner->m_sTargetIndex < 0` (no owner character, no target, or a target the client does not
/// have in `CharactersClient[]`).
fn owner_target_position(e: &Effect, ctx: &Ctx) -> Option<Vec3> {
    let target = ctx.actor(e.owner)?.target?;
    ctx.actors.get(&target).map(|t| t.position)
}

/// `g_isCharacterBuff(o->Owner, buff)`. An effect owner (`OBJECT` of `Effects[]`) never has buffs.
fn has_buff(ctx: &Ctx, owner: Option<Target>, buff: i32) -> bool {
    ctx.actor(owner).is_some_and(|a| a.has_buff(buff))
}

/// `o->Owner->AnimationFrame > Models[o->Owner->Type].Actions[action].NumAnimationKeys`
/// (`false` when the owner's model or that action is not available).
fn past_action_end(ctx: &mut Ctx, a: &Actor, action: i32) -> bool {
    usize::try_from(action)
        .ok()
        .and_then(|action| ctx.action_frames(a.ty, action))
        .is_some_and(|keys| a.frame > keys as f32)
}

/// `o->Owner->Live` (an effect owner is alive while it is still in `Effects[]`); no owner = dead.
fn owner_live(e: &Effect, ctx: &Ctx) -> bool {
    match e.owner {
        Some(Target::Actor(_)) => ctx.actor(e.owner).is_some_and(|a| a.live),
        Some(Target::Effect(id)) => ctx.effect_positions.contains_key(&id),
        None => false,
    }
}

/// `TransformByObjectBone(out, o, 0)` after `Models[o->Type].Animation(BoneTransform, ...)` on the
/// effect's own model (an effect has no `BoneTransform` of its own, so the global one is read).
/// Without the model file the bone is taken at the model origin (`o->Position`).
fn own_bone0(e: &Effect, ctx: &mut Ctx) -> Vec3 {
    ctx.effect_bone_world(e, 0, Vec3::ZERO).unwrap_or(e.position)
}

/// `vPos = Position; vDir = from`, both lowered to the terrain, `VectorNormalize(vPos - vDir)`
/// (a zero vector stays zero).
fn ground_dir(ctx: &Ctx, pos: Vec3, from: Vec3) -> Vec3 {
    let p = Vec3::new(pos.x, pos.y, ctx.terrain_height(pos.x, pos.y));
    let f = Vec3::new(from.x, from.y, ctx.terrain_height(from.x, from.y));
    (p - f).normalize_or_zero()
}

/// `switch(rand()%3)`: `BITMAP_FIRE_HIK1` / `BITMAP_FIRE_CURSEDLICH` / `BITMAP_FIRE_HIK3` with the
/// given subtypes.
fn fire(ctx: &mut Ctx, pos: Vec3, angle: Vec3, light: Vec3, subs: [i32; 3], scale: f32) {
    let (ty, sub) = match ctx.rand() % 3 {
        0 => (BITMAP_FIRE_HIK1, subs[0]),
        1 => (BITMAP_FIRE_CURSEDLICH, subs[1]),
        _ => (BITMAP_FIRE_HIK3, subs[2]),
    };
    ctx.create_particle(ty, pos, angle, light, sub, scale, None);
}

/// The owner angle shake of the wolf heads: the original writes `o->Owner->Angle`, creates trails
/// and effects with it and restores it at the end of the block, so a local copy is equivalent.
fn shaken_angle(ctx: &mut Ctx, angle: Vec3, base: f32) -> Vec3 {
    let n = ctx.rand() % 6;
    let value = base + (ctx.rand() % 3) as f32;
    let d = value * (n % 3 * 2 + 1) as f32;
    let mut a = angle;
    if n / 3 != 0 {
        a.z = angle.z + d;
    } else {
        a.z = angle.z - d;
    }
    let t = ctx.rand() % 3;
    if t == 1 {
        a.x = angle.x + d;
    } else if t == 2 {
        a.x = angle.x - d;
    }
    a
}

// --- Drawing helpers ------------------------------------------------------------------------------

/// `RENDER_TEXTURE|RENDER_BRIGHT`.
const TEX_BRIGHT: u32 = RENDER_TEXTURE | RENDER_BRIGHT;

/// `RenderObject(o)` → `b->RenderBody(flags, o->Alpha, o->BlendMesh, o->BlendMeshLight,
/// o->BlendMeshTexCoordU, o->BlendMeshTexCoordV, o->HiddenMesh)`: nothing when `Alpha < 0.01`
/// (`Calc_RenderObject`, ZzzObject.cpp:245); else the effect's model with `BodyLight` = `light`.
fn model(e: &Effect, light: Vec3, flags: u32) -> Option<ModelDraw> {
    if e.alpha < 0.01 {
        return None;
    }
    let mut d = ModelDraw::of_effect(e);
    d.light = light;
    d.render_flags = flags;
    Some(d)
}

/// `b->RenderMesh(i, RENDER_TEXTURE|RENDER_BRIGHT, o->Alpha, i, o->BlendMeshLight,
/// o->BlendMeshTexCoordU, o->BlendMeshTexCoordV, texture)` with `BodyLight` = `light`: mesh `i`
/// alone, being its own blend mesh (`texture` -1 = the mesh's own).
fn mesh(e: &Effect, light: Vec3, i: i32, texture: i32) -> Option<ModelDraw> {
    let mut d = model(e, light, TEX_BRIGHT)?;
    d.mesh = i;
    d.blend_mesh = i;
    d.texture = texture;
    Some(d)
}

/// ZzzObject.cpp:2538-2542: `RenderBody(RENDER_TEXTURE|RENDER_BRIGHT, ...)` (wolf head, punch
/// dummies, dragon kick dummy).
fn draw_bright(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    out.models.extend(model(e, e.light, TEX_BRIGHT));
}

/// ZzzObject.cpp:2543-2554: `BodyLight = Light * 6`, `RenderBody(RENDER_TEXTURE|RENDER_BRIGHT, ...)`
/// (`MODEL_WINDFOCE`, `MODEL_WINDFOCE_MIRROR`).
fn draw_light6(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    out.models.extend(model(e, e.light * 6.0, TEX_BRIGHT));
}

// --- MODEL_WOLF_HEAD_EFFECT ------------------------------------------------------------------------

/// ZzzEffect.cpp:6086.
fn wolf_head_create(e: &mut Effect, ctx: &mut Ctx) {
    let Some(p) = ctx.actor(e.owner).map(|a| a.bone_world(27, Vec3::ZERO)) else {
        e.live = false;
        return;
    };
    e.position = p;
    match e.sub_type {
        0 => {
            e.scale = 1.0;
            e.life_time = int_life(10.0 / rage_speed(ctx, SPEED_THRUST));
        }
        1 => {
            e.velocity = 0.1;
            e.scale = 1.2 + rf(ctx, 5) * 0.1;
            e.direction.y = -50.0;
            e.life_time = (2 + ctx.rand() % 2) as f32;
        }
        2 => {
            e.velocity = 0.1;
            e.scale = 0.8 + rf(ctx, 3) * 0.1;
            e.direction.y = -40.0;
            e.life_time = (3 + ctx.rand() % 3) as f32;
            e.angle.z += rf(ctx, 10) - 5.0;
        }
        _ => {}
    }
}

fn wolf_head_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    wolf_head_move(e, ctx);
    move_tail(e, ctx);
}

/// ZzzEffect.cpp:17056.
fn wolf_head_move(e: &mut Effect, ctx: &mut Ctx) {
    let Some(a) = ctx.actor(e.owner).cloned() else {
        e.live = false;
        return;
    };
    match e.sub_type {
        0 => {
            e.blend_mesh_light = life(e) as f32 / 20.0;
            e.alpha = e.blend_mesh_light;
            if life(e) >= 3 {
                let angle = shaken_angle(ctx, a.angle, 7.0);
                for _ in 0..3 {
                    // The owner's BoneTransform was computed with its unshaken angle.
                    let mut p = a.bone_world(27, Vec3::ZERO);
                    e.position = p;
                    e.scale = 1.5;
                    p.y += (ctx.rand() % 20 - 10) as f32;
                    p.z += (ctx.rand() % 28 + 6) as f32;
                    ctx.create_joint(BITMAP_PIN_LIGHT, p, p, angle, 0, e.owner, 5.0);
                }
                ctx.create_effect(MODEL_WOLF_HEAD_EFFECT, a.position, angle, a.light, 1, e.owner, -1, 0, 0, 0.0);
                ctx.create_effect(MODEL_WOLF_HEAD_EFFECT, a.position, angle, a.light, 2, e.owner, -1, 0, 0, 0.0);
            }
        }
        1 => {
            // TODO(original): o->Owner->Velocity += 2.5f.
        }
        2 => {
            // TODO(original): o->Owner->Velocity += 5.5f.
        }
        _ => {}
    }
}

// --- BITMAP_SBUMB -----------------------------------------------------------------------------------

/// ZzzEffect.cpp:6113.
fn sbumb_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = 20.0;
    // o->Scale = Scale (already set by the engine).
    let Some(p) = ctx.actor(e.owner).map(|a| a.bone_world(27, Vec3::ZERO)) else {
        e.live = false;
        return;
    };
    e.position.z = p.z;
}

fn sbumb_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    sbumb_move(e, ctx);
    move_tail(e, ctx);
}

/// ZzzEffect.cpp:17106.
fn sbumb_move(e: &mut Effect, ctx: &mut Ctx) {
    if life(e) < 17 {
        let light = Vec3::ONE;
        let Some(t) = owner_target_position(e, ctx) else { return };
        let x = t.x + (ctx.rand() % 80 - 40) as f32;
        let y = t.y + (ctx.rand() % 80 - 40) as f32;
        let z = t.z + (ctx.rand() % 120 + 30) as f32;
        let scale = 1.5 + rf(ctx, 10) * 0.02;
        ctx.create_particle(BITMAP_SBUMB, Vec3::new(x, y, z), e.angle, light, 0, scale * e.scale, e.owner);
    }
    if matches!(life(e), 15 | 9 | 6) {
        ctx.stop_sound("SOUND_RAGESKILL_THRUST_ATTACK");
        ctx.play_sound("SOUND_RAGESKILL_THRUST_ATTACK");
    }
}

// --- MODEL_DOWN_ATTACK_DUMMY_L / MODEL_DOWN_ATTACK_DUMMY_R ------------------------------------------

/// ZzzEffect.cpp:6122-6131.
fn down_dummy_create(e: &mut Effect, ctx: &mut Ctx) {
    if ctx.actor(e.owner).is_none() {
        e.live = false;
        return;
    }
    // Position = Position (already).
    let speed = rage_speed(ctx, SPEED_STAMP);
    e.velocity = speed * 2.0;
    e.life_time = 100.0;
    // Models[MODEL_DOWN_ATTACK_DUMMY_L/R].Actions[0].PlaySpeed = speed: not read by these types
    // (PlayAnimation takes o->Velocity).
}

fn down_dummy_l_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    down_dummy_move(e, ctx, false);
    move_tail(e, ctx);
}

fn down_dummy_r_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    down_dummy_move(e, ctx, true);
    move_tail(e, ctx);
}

/// ZzzEffect.cpp:17133 (left) and 17175 (right).
fn down_dummy_move(e: &mut Effect, ctx: &mut Ctx, right: bool) {
    let Some(a) = ctx.actor(e.owner).filter(|a| a.live).cloned() else {
        e.live = false;
        return;
    };
    // pModel->Animation(BoneTransform, o->AnimationFrame, ...): the dummy's own pose (see own_bone0).
    let (end_frame, hit_frame) = if right { (6.0, 3.0) } else { (8.0, 5.0) };
    if e.animation_frame >= end_frame {
        e.live = false;
        if right {
            ctx.create_effect(MODEL_DOWN_ATTACK_DUMMY_L, a.position, a.angle, a.light, 0, e.owner, -1, 0, 0, 0.0);
        }
    }
    if e.animation_frame >= hit_frame && e.life_time > 50.0 {
        let Some(t) = owner_target_position(e, ctx) else { return };
        common::create_bomb3(ctx, t, 3, 0.9);
        ctx.stop_sound("SOUND_RAGESKILL_STAMP_ATTACK");
        ctx.play_sound("SOUND_RAGESKILL_STAMP_ATTACK");
        e.life_time = 40.0;
    }
    let start = own_bone0(e, ctx);
    ctx.create_joint(BITMAP_FORCEPILLAR, start, start, e.angle, if right { 1 } else { 0 }, me(e), 50.0);
    // The second TransformByObjectBone reads the same global BoneTransform (posed with the old
    // o->Angle) and only the new o->Position.
    let end = start - e.position + a.position;
    e.angle = a.angle;
    e.position = a.position;
    ctx.create_sprite(BITMAP_FLARE, end, 5.0, Vec3::new(0.47, 0.36, 0.24), 0.0, 0);
}

// --- BITMAP_SWORDEFF --------------------------------------------------------------------------------

/// ZzzEffect.cpp:6132.
fn swordeff_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 200.0;
}

fn swordeff_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    swordeff_move(e, ctx);
    move_tail(e, ctx);
}

/// ZzzEffect.cpp:17220.
fn swordeff_move(e: &mut Effect, ctx: &mut Ctx) {
    let Some(a) = ctx.actor(e.owner).cloned() else {
        e.live = false;
        return;
    };
    if past_action_end(ctx, &a, PLAYER_SKILL_GIANTSWING) {
        e.live = false;
    }
    let start = a.bone_world(5, Vec3::ZERO);
    let end = a.bone_world(4, Vec3::ZERO);
    ctx.create_joint(BITMAP_SWORDEFF, start, end, a.angle, 0, e.owner, 100.0);

    if a.frame > 6.0 && e.life_time > 50.0 {
        let light = Vec3::new(0.4, 0.5, 1.0);
        let position = a.bone_world(0, Vec3::new(-180.0, 100.0, 0.0));
        let angle = Vec3::new(60.0, 0.0, a.angle.z - 180.0);
        ctx.create_effect(MODEL_SHOCKWAVE01, position, angle, light, 0, e.owner, -1, 0, 0, 1.0);
        ctx.create_effect(MODEL_WOLF_HEAD_EFFECT2, e.position, e.angle, e.light, 3, e.owner, -1, 0, 0, 0.0);
        let Some(t) = owner_target_position(e, ctx) else { return };
        ctx.create_effect(BITMAP_DAMAGE1, t, angle, light, 0, e.owner, -1, 0, 0, 1.4);
        ctx.stop_sound("SOUND_RAGESKILL_GIANTSWING_ATTACK");
        ctx.play_sound("SOUND_RAGESKILL_GIANTSWING_ATTACK");
        e.life_time = 45.0;
    }

    if a.frame > 11.0 && e.life_time > 20.0 {
        let mut light = Vec3::new(0.5, 0.5, 1.0);
        let position = a.bone_world(4, Vec3::new(-80.0, 120.0, 0.0));
        let mut angle = Vec3::new(90.0, 0.0, a.angle.z - 180.0);
        ctx.create_effect(MODEL_SHOCKWAVE02, position, angle, light, 0, e.owner, -1, 0, 0, 0.5);
        ctx.create_effect(MODEL_WOLF_HEAD_EFFECT2, e.position, e.angle, e.light, 5, e.owner, -1, 0, 0, 0.0);

        light = Vec3::ONE;
        let p = a.bone_world(4, Vec3::new(-80.0, 20.0, 0.0));
        angle = Vec3::new(90.0, angle.y, angle.z);
        ctx.create_effect(MODEL_SHOCKWAVE_SPIN01, p, angle, light, 0, e.owner, -1, 0, 0, 0.5);
        ctx.create_effect(MODEL_SHOCKWAVE_SPIN01, p, angle, light, 1, e.owner, -1, 0, 0, 0.9);

        let Some(t) = owner_target_position(e, ctx) else { return };
        ctx.create_effect(BITMAP_SHINY + 4, t, e.angle, light, 0, e.owner, -1, 0, 0, 1.9);
        ctx.stop_sound("SOUND_RAGESKILL_GIANTSWING_ATTACK");
        ctx.play_sound("SOUND_RAGESKILL_GIANTSWING_ATTACK");
        e.life_time = 20.0;
    }
}

// --- MODEL_SHOCKWAVE01 ------------------------------------------------------------------------------

/// ZzzEffect.cpp:6137.
fn shockwave01_create(e: &mut Effect, ctx: &mut Ctx) {
    if e.owner.is_none() {
        e.live = false;
        return;
    }
    // o->Scale = Scale and o->Light = Light (already).
    match e.sub_type {
        1 => {
            e.velocity = rage_speed(ctx, SPEED_DRAGONLORE);
            e.life_time = int_life(5.0 / e.velocity);
        }
        2 => {
            e.velocity = 0.3;
            e.life_time = int_life(5.0 / e.velocity);
        }
        3 => {
            e.velocity = rage_speed(ctx, SPEED_DARKSIDE_READY) * 2.0;
            e.life_time = int_life(5.0 / e.velocity);
            let from = ctx.target_start_position(e.owner);
            let dir = from.map_or(Vec3::ZERO, |f| ground_dir(ctx, e.position, f)) * 30.0;
            e.start_position = dir;
            e.position += dir * 5.0;
        }
        4 => {
            e.velocity = 0.6;
            e.life_time = 12.0;
            let from = ctx.target_start_position(e.owner);
            let dir = from.map_or(Vec3::ZERO, |f| ground_dir(ctx, e.position, f)) * 30.0;
            e.start_position = dir;
            e.position += dir.normalize_or_zero() * 150.0;
        }
        _ => {
            let Some(owner_pos) = ctx.actor(e.owner).map(|a| a.position) else {
                e.live = false;
                return;
            };
            e.velocity = rage_speed(ctx, SPEED_GIANTSWING) * 2.0;
            e.life_time = int_life(5.0 / e.velocity);
            e.start_position = ground_dir(ctx, e.position, owner_pos) * 30.0;
        }
    }
}

/// ZzzEffect.cpp:17288.
fn shockwave01_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    match e.sub_type {
        1 | 2 => {
            e.alpha = 1.0;
            e.scale *= 1.24;
            e.position.z *= 1.19;
            e.light *= 0.45;
            if e.light.x < 0.001 {
                e.light = Vec3::ZERO;
            }
        }
        3 => {
            e.scale *= 1.15;
            e.start_position *= 0.99;
            e.position -= e.start_position;
            e.light *= 0.7;
            if e.light.x > 0.6 {
                e.sub_type = 5;
            }
            e.position.z = 180.0 + e.scale * 80.0;
        }
        4 => {
            e.scale *= 1.15;
            e.start_position *= 1.04;
            e.position -= e.start_position;
            e.light *= 1.5;
            if e.light.x > 0.6 {
                e.sub_type = 5;
            }
            e.position.z = 160.0 + e.scale * 10.0;
        }
        5 => {
            e.scale *= 1.15;
            e.start_position *= 1.04;
            e.position -= e.start_position;
            e.light *= 0.5;
            e.position.z = 160.0 + e.scale * 10.0;
        }
        _ => {
            e.alpha = 1.0;
            e.scale += 0.2;
            e.start_position *= 0.99;
            e.position -= e.start_position;
            e.light *= 0.65;
            e.position.z = 220.0 + e.scale * 80.0;
        }
    }
    move_tail(e, ctx);
}

/// ZzzObject.cpp:2494-2526: `RenderMesh(0, RENDER_TEXTURE|RENDER_BRIGHT, Alpha, 0, ...)` once or
/// twice with `BodyLight = Light * k`, subtypes 1, 3, 4 and 5 with the texture `BITMAP_DAMAGE2`.
fn shockwave01_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    let (k, times, texture) = match e.sub_type {
        1 => (5.0, 1, BITMAP_DAMAGE2),
        2 => (15.0, 2, -1),
        3 => (1.0, 2, BITMAP_DAMAGE2),
        4 | 5 => (1.0, 1, BITMAP_DAMAGE2),
        _ => (1.0, 2, -1),
    };
    let Some(d) = mesh(e, e.light * k, 0, texture) else { return };
    for _ in 0..times {
        out.models.push(d);
    }
}

// --- MODEL_SHOCKWAVE02 ------------------------------------------------------------------------------

/// ZzzEffect.cpp:6213.
fn shockwave02_create(e: &mut Effect, ctx: &mut Ctx) {
    let Some(owner_pos) = ctx.actor(e.owner).map(|a| a.position) else {
        e.live = false;
        return;
    };
    // o->Scale = Scale and o->Light = Light (already).
    e.velocity = rage_speed(ctx, SPEED_GIANTSWING) * 2.0;
    e.life_time = int_life(14.0 / e.velocity);
    e.start_position = ground_dir(ctx, e.position, owner_pos) * 25.0;
}

/// ZzzEffect.cpp:17344.
fn shockwave02_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    e.alpha = 1.0;
    e.light *= 0.8;
    e.scale += 0.1;
    e.start_position *= 0.95;
    e.position -= e.start_position;
    move_tail(e, ctx);
}

/// RenderEffects ZzzEffect.cpp:19098 → ZzzObject.cpp:2527: `BodyLight = Light`, `RenderBody`
/// (`RENDER_TEXTURE|RENDER_BRIGHT`) twice.
fn shockwave02_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    let Some(d) = model(e, e.light, TEX_BRIGHT) else { return };
    out.models.push(d);
    out.models.push(d);
}

// --- BITMAP_DAMAGE1 ---------------------------------------------------------------------------------

/// ZzzEffect.cpp:6231.
fn damage1_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = 20.0;
    // o->Scale = Scale (already).
    let Some(p) = ctx.actor(e.owner).map(|a| a.bone_world(27, Vec3::ZERO)) else {
        e.live = false;
        return;
    };
    e.position.z = p.z;
}

fn damage1_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    damage1_move(e, ctx);
    move_tail(e, ctx);
}

/// ZzzEffect.cpp:17353.
fn damage1_move(e: &mut Effect, ctx: &mut Ctx) {
    if life(e) < 17 {
        if life(e) % 4 != 0 {
            return;
        }
        let light = Vec3::new(0.6, 0.94, 1.0);
        let Some(t) = owner_target_position(e, ctx) else { return };
        let x = t.x + (ctx.rand() % 90 - 40) as f32;
        let y = t.y + (ctx.rand() % 90 - 40) as f32;
        let z = t.z + (ctx.rand() % 100 + 50) as f32;
        let scale = 1.5 + rf(ctx, 5) * 0.01;
        ctx.create_particle(BITMAP_DAMAGE1, Vec3::new(x, y, z), e.angle, light, 0, scale * e.scale, e.owner);
    }
}

// --- MODEL_SHOCKWAVE_SPIN01 -------------------------------------------------------------------------

/// ZzzEffect.cpp:6240.
fn spin01_create(e: &mut Effect, ctx: &mut Ctx) {
    // Models[o->Owner->Type] with a NULL owner would crash.
    if e.owner.is_none() {
        e.live = false;
        return;
    }
    // o->Scale = Scale and o->Light = Light (already).
    e.velocity = rage_speed(ctx, SPEED_GIANTSWING);
    e.life_time = int_life(8.0 / e.velocity);
    if e.sub_type == 0 {
        e.velocity *= 2.0;
    } else if e.sub_type == 1 {
        e.velocity *= 3.0;
    }
}

/// ZzzEffect.cpp:17373.
fn spin01_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    e.scale *= 1.01;
    e.angle.y -= 10.0;
    if e.sub_type == 1 {
        e.angle.y += 20.0;
    }
    e.light *= 0.9;
    e.alpha = 1.0;
    move_tail(e, ctx);
}

/// ZzzObject.cpp:2533: `BodyLight = Light`, `RenderBody(RENDER_TEXTURE|RENDER_BRIGHT, ..., BITMAP_DAMAGE1)`.
fn spin01_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    let Some(mut d) = model(e, e.light, TEX_BRIGHT) else { return };
    d.texture = BITMAP_DAMAGE1;
    out.models.push(d);
}

// --- BITMAP_EVENT_CLOUD -----------------------------------------------------------------------------

/// ZzzEffect.cpp:6256.
fn event_cloud_create(e: &mut Effect, _ctx: &mut Ctx) {
    // o->Scale = Scale and o->Light = Light (already).
    e.life_time = 30.0;
    if e.sub_type == 1 {
        e.life_time = 30.0;
    }
}

/// ZzzEffect.cpp:17385.
fn event_cloud_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    let max = if e.sub_type == 1 { 5.0 } else { 3.0 };
    e.scale *= 1.08;
    if e.scale > max {
        e.scale = max;
    }
    e.angle.x = (ctx.world_time() as i32).wrapping_neg() as f32 * 0.5;
    e.light *= 0.9;
    move_tail(e, ctx);
}

/// RenderEffects ZzzEffect.cpp:19091: `EnableAlphaBlend` + `RenderTerrainAlphaBitmap`.
fn event_cloud_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    common::terrain_bitmap(out, BITMAP_EVENT_CLOUD, e.position.x, e.position.y, e.scale, e.scale, e.light, e.angle.x, e.alpha, 5.0, Blend::Glow);
}

// --- MODEL_WINDFOCE ---------------------------------------------------------------------------------

/// ZzzEffect.cpp:6267.
fn windfoce_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 50.0;
    // o->Light = Light and o->Scale = Scale (already).
    match e.sub_type {
        1 => e.life_time = 999.0,
        2 | 3 => e.life_time = 70.0,
        _ => {}
    }
}

fn windfoce_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    windfoce_move(e, ctx);
    move_tail(e, ctx);
}

/// ZzzEffect.cpp:17409.
fn windfoce_move(e: &mut Effect, ctx: &mut Ctx) {
    let wt = ctx.world_time() as i32;
    e.angle.z = wt.wrapping_neg() as f32 * 0.3;
    match e.sub_type {
        0 | 4 | 5 => {
            e.scale += 0.1;
            if e.scale > 3.0 {
                e.scale = 3.0;
            }
            e.alpha *= 0.92;
            e.light *= 0.9;
            if life(e) == 40 || life(e) == 30 {
                let light = match e.sub_type {
                    0 => Vec3::new(0.5, 0.55, 1.0),
                    4 => Vec3::new(0.85, 0.2, 1.0),
                    _ => Vec3::new(1.0, 0.2, 0.0),
                };
                ctx.create_effect(MODEL_WINDFOCE, e.position, e.angle, light, 3, me(e), -1, 0, 0, 1.0);
            }
        }
        1 => {
            let buffed = has_buff(ctx, e.owner, eBuff_Att_up_Ourforces)
                || has_buff(ctx, e.owner, eBuff_Hp_up_Ourforces)
                || has_buff(ctx, e.owner, eBuff_Def_up_Ourforces);
            e.life_time = if owner_live(e, ctx) && buffed { 10.0 } else { 0.0 };
            if has_buff(ctx, e.owner, eBuff_Cloaking) {
                return;
            }
            let fade = (ctx.world_time() * 0.001).sin() + 1.0;
            e.light = Vec3::new(0.6, 0.6, 1.0) * (fade * 0.25 + 0.2);
            e.scale = 1.0;
            if let Some(p) = ctx.target_position(e.owner) {
                e.position = p;
            }
        }
        2 | 3 => {
            e.angle.z = wt.wrapping_neg() as f32 * 0.9;
            e.scale += 0.1;
            if e.scale > 3.0 {
                e.scale = 3.0;
            }
            e.alpha *= 0.92;
            e.light *= 0.85;
            if e.sub_type == 2 && (life(e) == 60 || life(e) == 50) {
                ctx.create_effect(MODEL_WINDFOCE, e.position, e.angle, Vec3::new(1.0, 0.5, 0.0), 3, me(e), -1, 0, 0, 1.0);
            }
        }
        _ => {}
    }
}

// --- BITMAP_SHINY+4 ---------------------------------------------------------------------------------

/// ZzzEffect.cpp:6287.
fn shiny4_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 16.0;
    // o->Scale = Scale (already).
}

/// ZzzEffect.cpp:17494.
fn shiny4_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    let x = e.position.x + (ctx.rand() % 100 - 40) as f32;
    let y = e.position.y + (ctx.rand() % 100 - 40) as f32;
    let z = e.position.z + (ctx.rand() % 100 + 50) as f32;
    ctx.create_particle(BITMAP_SHINY + 4, Vec3::new(x, y, z), e.angle, e.light, 2, e.scale, None);
    move_tail(e, ctx);
}

// --- BITMAP_LIGHT_RED -------------------------------------------------------------------------------
//
// user[0..3]: the owner's position at the end of the last tick (for the subtype 3 ground mark, whose
// owner is the volcano effect); user[3]: 1 when it is known.

/// ZzzEffect.cpp:6293.
fn light_red_create(e: &mut Effect, ctx: &mut Ctx) {
    let scale_arg = e.scale;
    e.life_time = 9999.0;
    e.velocity = 0.0;
    e.scale = 1.5;
    e.light = Vec3::ONE;
    // Position = Position (already).
    if e.sub_type == 1 {
        e.life_time = 50.0;
    } else if e.sub_type == 3 || e.sub_type == 4 {
        e.scale = scale_arg;
        e.life_time = if e.sub_type == 4 { 50.0 } else { 100.0 };
        let lum = rf(ctx, 5) * 0.01;
        let light = Vec3::new(lum + 1.0, lum + 0.2, lum + 0.2);
        let range = if e.sub_type == 4 { 4.0 } else { 2.0 };
        ctx.add_terrain_light(e.position.x, e.position.y, light, range);
    }
}

fn light_red_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    light_red_move(e, ctx);
    move_tail(e, ctx);
    // What RenderEffects (ZzzEffect.cpp:19158) does while drawing subtype 0: VectorCopy(o->Owner->
    // Position, o->Position). Moved here (once per tick).
    if let Some(p) = ctx.target_position(e.owner) {
        if e.sub_type == 0 {
            e.position = p;
        }
        e.user[0] = p.x;
        e.user[1] = p.y;
        e.user[2] = p.z;
        e.user[3] = 1.0;
    }
}

/// ZzzEffect.cpp:17503.
fn light_red_move(e: &mut Effect, ctx: &mut Ctx) {
    let att = has_buff(ctx, e.owner, eBuff_Att_up_Ourforces);
    let hp = has_buff(ctx, e.owner, eBuff_Hp_up_Ourforces);
    let def = has_buff(ctx, e.owner, eBuff_Def_up_Ourforces);
    if owner_live(e, ctx) && (hp || att || def) {
        e.life_time = 10.0;
        if def && ctx.rand() % 70 == 7 && !ctx.search_effect(MODEL_WINDFOCE_MIRROR, e.owner, -1) {
            // (The owner is alive, so its position is known.)
            let p = ctx.target_position(e.owner).unwrap_or(e.position);
            ctx.create_effect(MODEL_WINDFOCE_MIRROR, p, e.angle, Vec3::new(0.6, 0.6, 1.0), 0, e.owner, -1, 0, 0, 1.0);
        }
    } else if e.sub_type == 3 || e.sub_type == 4 {
        let lum = rf(ctx, 5) * 0.1;
        let light = Vec3::new(lum + 0.4, lum, lum);
        let range = if e.sub_type == 4 { 3.0 } else { 2.0 };
        ctx.add_terrain_light(e.position.x, e.position.y, light, range);
    } else {
        e.life_time = 0.0;
    }

    if has_buff(ctx, e.owner, eBuff_Cloaking) && e.sub_type != 3 {
        return;
    }

    // `if (att || SubType == 1) { if (att) { if (LifeTime % 2 == 0) ... } }`: the outer test only
    // guards the inner ones.
    if att && life(e) % 2 == 0 {
        let light = Vec3::new(1.0, 0.12, 0.0);
        let bone = if ctx.rand() % 2 == 0 { 26 } else { 35 };
        if let Some(p) = ctx.actor(e.owner).map(|a| a.bone_world(bone, Vec3::ZERO)) {
            ctx.create_particle(BITMAP_SMOKELINE1, p, e.angle, light, 5, 0.005, e.owner);
        }
    }
}

/// The 17 owner bones lit by subtype 0 (ZzzEffect.cpp:19150).
const LIGHT_RED_BONES: [usize; 17] = [12, 17, 5, 10, 36, 27, 37, 28, 11, 35, 2, 3, 36, 20, 27, 4, 26];

/// RenderEffects ZzzEffect.cpp:19111: ground marks (`EnableAlphaBlend`) and, for subtype 0, glow
/// sprites on 17 of the owner's bones (`CreateSprite` while rendering: drawn this frame).
fn light_red_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    let wt = ctx.world_time;
    let mut scale = 3.5;
    let mut fade = ((wt * 0.0005).sin() + 1.0) * 0.25 + 0.2;
    if e.sub_type == 3 || e.sub_type == 4 {
        scale = e.scale;
        fade = life(e) as f32 * 0.01;
    }
    let light = Vec3::ONE * fade;
    let owner = common::owner_actor(e.owner, ctx.actors);
    let mark = |out: &mut DrawList, x: f32, y: f32| {
        common::terrain_bitmap(out, BITMAP_LIGHT_RED, x, y, scale, scale, light, e.angle.x, e.alpha, 5.0, Blend::Glow);
    };
    match e.sub_type {
        0 => {
            if let Some(a) = owner {
                mark(out, a.position.x, a.position.y);
                mark(out, a.position.x, a.position.y);
            }
        }
        3 => {
            if e.user[3] != 0.0 {
                mark(out, e.user[0], e.user[1]);
            }
        }
        4 => {
            mark(out, e.position.x, e.position.y);
            mark(out, e.position.x, e.position.y);
        }
        _ => {}
    }

    if e.sub_type == 0 {
        let Some(a) = owner else { return };
        for (i, &bone) in LIGHT_RED_BONES.iter().enumerate() {
            let fade = match i {
                0..=2 => ((wt * 0.01).sin() + 1.0) * 0.25 + 0.2,
                5 | 7 | 8 | 9 | 12 | 14 | 15 | 16 => ((wt * 0.005).sin() + 1.0) * 0.25 + 0.5,
                _ => ((wt * 0.01).sin() + 1.0) * 0.3 + 0.3,
            };
            let p = a.bone_world(bone, Vec3::ZERO);
            common::sprite(out, ctx, BITMAP_LIGHT_RED, p, 1.5, Vec3::ONE * fade, 0.0, Some(Blend::Glow));
        }
    }
}

// --- MODEL_WINDFOCE_MIRROR --------------------------------------------------------------------------

/// ZzzEffect.cpp:6324.
fn mirror_create(e: &mut Effect, _ctx: &mut Ctx) {
    // o->Light = Light and o->Scale = Scale (already).
    e.life_time = 50.0;
}

fn mirror_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    mirror_move(e, ctx);
    move_tail(e, ctx);
}

/// ZzzEffect.cpp:17565.
fn mirror_move(e: &mut Effect, ctx: &mut Ctx) {
    if !owner_live(e, ctx) || !has_buff(ctx, e.owner, eBuff_Def_up_Ourforces) {
        e.live = false;
        return;
    }
    e.angle.z = (ctx.world_time() as i32) as f32 * 0.4;
    e.scale *= 1.015;
    if e.scale > 1.5 {
        e.scale = 1.5;
    }
    e.light *= 0.9;
    if let Some(p) = ctx.target_position(e.owner) {
        e.position = p;
    }
}

// --- BITMAP_SWORD_EFFECT_MONO -----------------------------------------------------------------------

/// ZzzEffect.cpp:6331.
fn sword_mono_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 15.0;
    // o->Light = Light and o->Scale = Scale (already).
}

/// ZzzEffect.cpp:17584.
fn sword_mono_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    // (vLight = Light * 0.1 is computed and never used.)
    if life(e) % 3 != 0 {
        ctx.create_particle(BITMAP_SWORD_EFFECT_MONO, e.position, e.angle, e.light, 0, e.scale, me(e));
    }
    if life(e) == 14 {
        let light = match e.sub_type {
            1 => Vec3::new(0.7, 0.5, 1.0),
            2 => Vec3::new(1.0, 0.12, 0.0),
            _ => Vec3::new(0.4, 0.4, 1.0),
        };
        ctx.create_effect(MODEL_SHOCKWAVE01, e.position, e.angle, light, 2, e.owner, -1, 0, 0, 1.0);
    }
    move_tail(e, ctx);
}

// --- MODEL_WOLF_HEAD_EFFECT2 ------------------------------------------------------------------------

/// ZzzEffect.cpp:6338.
fn wolf_head2_create(e: &mut Effect, ctx: &mut Ctx) {
    let Some(a) = ctx.actor(e.owner).cloned() else {
        e.live = false;
        return;
    };
    e.position = a.bone_world(27, Vec3::ZERO);
    match e.sub_type {
        3 | 5 => {
            e.position = a.bone_world(5, Vec3::ZERO);
            e.scale = if e.sub_type == 5 { 1.5 } else { 1.0 };
            e.life_time = int_life(4.0 / rage_speed(ctx, SPEED_GIANTSWING));
        }
        4 => {
            e.velocity = 0.1;
            e.scale = 0.7 + rf(ctx, 3) * 0.1;
            e.direction.y = -40.0;
            e.life_time = (5 + ctx.rand() % 2) as f32;
        }
        6 => {
            e.velocity = 0.1;
            e.scale = 1.2 + rf(ctx, 5) * 0.1;
            e.direction.y = -50.0;
            e.life_time = (2 + ctx.rand() % 2) as f32;
        }
        _ => {}
    }
    e.light = Vec3::new(0.4, 0.4, 0.6);
}

fn wolf_head2_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    wolf_head2_move(e, ctx);
    move_tail(e, ctx);
}

/// ZzzEffect.cpp:17611. (Its `else if (o->SubType == 4)` branch is unreachable: 4 is taken by the
/// first test.)
fn wolf_head2_move(e: &mut Effect, ctx: &mut Ctx) {
    let Some(a) = ctx.actor(e.owner).cloned() else {
        e.live = false;
        return;
    };
    match e.sub_type {
        4 | 6 => {
            // TODO(original): o->Owner->Velocity += 2.5f.
        }
        3 => {
            e.blend_mesh_light = life(e) as f32 / 20.0;
            e.alpha = e.blend_mesh_light;
            e.angle = a.angle;
            if life(e) >= 3 {
                let angle = shaken_angle(ctx, a.angle, 2.0);
                // `SubType == 5 ? 27 : 4` inside the subtype 3 branch: always bone 4.
                e.position = a.bone_world(4, Vec3::ZERO);
                ctx.create_effect(MODEL_WOLF_HEAD_EFFECT2, a.position, angle, a.light, 4, e.owner, -1, 0, 0, 0.0);
                ctx.create_effect(MODEL_WOLF_HEAD_EFFECT2, a.position, angle, a.light, 6, e.owner, -1, 0, 0, 0.0);
            }
        }
        5 => {
            e.position = a.bone_world(27, Vec3::ZERO);
            e.angle = a.angle;
        }
        _ => {}
    }
}

/// ZzzObject.cpp:2486: `BodyLight = (0.4, 0.4, 0.6) * 0.3`, `RenderBody(RENDER_TEXTURE|RENDER_BRIGHT,
/// ..., BlendMeshLight * 0.5, ...)`. (Its `o->Light = (0.4, 0.4, 0.6)` is already set at creation.)
fn wolf_head2_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    let Some(mut d) = model(e, Vec3::new(0.4, 0.4, 0.6) * 0.3, TEX_BRIGHT) else { return };
    d.blend_mesh_light = e.blend_mesh_light * 0.5;
    out.models.push(d);
}

// --- MODEL_SHOCKWAVE_GROUND01 -----------------------------------------------------------------------

/// ZzzEffect.cpp:6372.
fn ground01_create(e: &mut Effect, _ctx: &mut Ctx) {
    // o->Light = Light and o->Scale = Scale (already).
    e.life_time = match e.sub_type {
        0 => 20.0,
        2 => 10.0,
        _ => 50.0,
    };
}

/// ZzzEffect.cpp:17672.
fn ground01_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    e.scale *= 1.2;
    e.light *= match e.sub_type {
        1 => 0.9,
        2 => 0.7,
        _ => 0.8,
    };
    move_tail(e, ctx);
}

/// ZzzObject.cpp:2555: `BodyLight = Light * 6`, `RenderMesh(0, RENDER_TEXTURE|RENDER_BRIGHT, Alpha,
/// 0, ...)`, subtype 1 with the texture `BITMAP_KWAVE2`.
fn ground01_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    let texture = if e.sub_type == 1 { BITMAP_KWAVE2 } else { -1 };
    out.models.extend(mesh(e, e.light * 6.0, 0, texture));
}

// --- MODEL_DRAGON_KICK_DUMMY ------------------------------------------------------------------------
//
// user[0]: `Models[MODEL_DRAGON_KICK_DUMMY].Actions[0].PlaySpeed`.

/// ZzzEffect.cpp:6394.
fn dragon_kick_create(e: &mut Effect, ctx: &mut Ctx) {
    if ctx.actor(e.owner).is_none() {
        e.live = false;
        return;
    }
    // Position = Position and o->Light = Light (already).
    e.life_time = 200.0;
    e.user[0] = rage_speed(ctx, SPEED_DRAGONKICK);
    e.velocity = e.user[0] * 2.0;
}

fn dragon_kick_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    dragon_kick_move(e, ctx);
    move_tail(e, ctx);
}

/// ZzzEffect.cpp:17691.
fn dragon_kick_move(e: &mut Effect, ctx: &mut Ctx) {
    let Some(a) = ctx.actor(e.owner).cloned() else {
        e.life_time = 0.0;
        e.live = false;
        return;
    };
    // (The `o->Owner->Live` test comes after the action test in the original; both kill it alike.)
    if past_action_end(ctx, &a, PLAYER_SKILL_DRAGONKICK) || a.action != PLAYER_SKILL_DRAGONKICK as usize || !a.live {
        e.life_time = 0.0;
        e.live = false;
        return;
    }

    if a.frame > 4.0 && a.frame < 5.0 {
        let delay = e.velocity * 10.0;
        let action_speed = e.user[0];
        let per_frame = action_speed / delay;
        e.alpha_target = per_frame * (e.velocity / e.user[0]);
        // The owner model is animated into the global BoneTransform at frame
        // `AnimationFrame - PlaySpeed + i * SpeedPerFrame` and bones 1/2 are read through `o`
        // (o->Position + Owner->Angle-rotated bone).
        // TODO(original): `BMD::Animation` (ZzzBMD.cpp:53-173) blends the key
        // `(int)o->Owner->PriorAnimationFrame` of `o->Owner->PriorAction` with the key
        // `(int)fAnimationFrame` of `Models[o->Owner->Type].CurrentAction` by the fraction of
        // `fAnimationFrame`; `Actor` has no PriorAction/PriorAnimationFrame, so the owner's
        // current pose is used for every sample.
        let start = e.position + (a.bone_world(1, Vec3::ZERO) - a.position);
        let end = e.position + (a.bone_world(2, Vec3::ZERO) - a.position);
        let mut i = 0;
        while (i as f32) < delay {
            ctx.create_joint(BITMAP_GROUND_WIND, start, end, e.angle, i, me(e), 150.0);
            i += 1;
        }
    }

    let Some(target) = owner_target_position(e, ctx) else { return };
    if a.frame > 5.0 {
        for _ in 0..2 {
            let lum = (ctx.world_time() * 0.002).sin() * 0.2;
            let mut p = target;
            p.z += 30.0;
            let scale = ((ctx.rand() % 40 + 50) as f32 * 0.01 * 1.5) * e.scale * 1.8;
            for i in 0..4 {
                ctx.create_particle(BITMAP_ENERGY, p, e.angle, e.light, 7, scale, None);
                ctx.create_particle(BITMAP_LIGHTNING_MEGA1, p, e.angle, e.light, 0, scale * 0.9, None);
                ctx.create_particle(BITMAP_SPARK + 1, p, e.angle, e.light, 3, 1.0 + lum * 0.05, None);
                ctx.create_particle(BITMAP_SPARK + 1, p, e.angle, e.light, 8, 1.0 + lum * 0.05, None);
                if i % 2 == 1 {
                    ctx.create_particle(BITMAP_SPARK, p, e.angle, e.light, 13, 1.0, None);
                }
                if i % 2 == 1 {
                    let x = p.x + rf(ctx, 40) + 20.0;
                    let y = p.y + rf(ctx, 30) + 10.0;
                    let z = p.z + rf(ctx, 30) + 25.0;
                    p = Vec3::new(x, y, z);
                } else {
                    let x = p.x - rf(ctx, 20) - 10.0;
                    let y = p.y + rf(ctx, 30) - 10.0;
                    let z = p.z + rf(ctx, 30) + 30.0;
                    p = Vec3::new(x, y, z);
                }
            }
        }
    }
    e.angle = a.angle;
    e.position = a.position;
    if a.frame > 8.0 {
        e.life_time = 150.0;
    }
    if life(e) == 150 {
        ctx.play_sound("SOUND_RAGESKILL_DRAGONKICK_ATTACK");
    }
}

// --- BITMAP_LAVA ------------------------------------------------------------------------------------

/// ZzzEffect.cpp:6404.
fn lava_create(e: &mut Effect, ctx: &mut Ctx) {
    let Some(owner_pos) = ctx.actor(e.owner).map(|a| a.position) else {
        e.live = false;
        return;
    };
    e.life_time = 999.0;
    e.start_position = owner_pos;
    // o->Scale = Scale (already).
    e.velocity = rage_speed(ctx, SPEED_DRAGONLORE);
}

fn lava_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    lava_move(e, ctx);
    move_tail(e, ctx);
}

/// ZzzEffect.cpp:17785.
fn lava_move(e: &mut Effect, ctx: &mut Ctx) {
    let Some(a) = ctx.actor(e.owner).cloned() else {
        e.live = false;
        e.life_time = 0.0;
        return;
    };
    if past_action_end(ctx, &a, PLAYER_SKILL_DRAGONLORE) || a.action != PLAYER_SKILL_DRAGONLORE as usize {
        e.live = false;
        e.life_time = 0.0;
        return;
    }
    if a.frame > 2.0 && a.frame < 8.0 {
        let delay = e.velocity * 10.0;
        // The loop animates the owner model into the global BoneTransform at sub-frames, but the
        // bones are read with TransformByObjectBone(o->Owner), which uses the owner's own
        // BoneTransform: every sample is the owner's current pose.
        let start = a.bone_world(36, Vec3::ZERO);
        let end = a.bone_world(28, Vec3::ZERO);
        let mut i = 0;
        while (i as f32) < delay {
            ctx.create_joint(BITMAP_LAVA, start, start, a.angle, i, e.owner, 20.0);
            ctx.create_joint(BITMAP_LAVA, end, end, a.angle, 7 + i, e.owner, 20.0);
            i += 1;
        }
    }
    let angle = a.angle;

    if a.frame > 5.0 && e.life_time > 100.0 {
        let light = Vec3::new(1.0, 0.12, 0.0);
        ctx.create_effect(MODEL_SHOCKWAVE01, a.position, angle, light, 1, e.owner, -1, 0, 0, 1.0);
        e.life_time = 95.0;
    }

    if a.frame > 5.0 && e.life_time > 80.0 {
        let light = Vec3::new(1.0, 0.1, 0.0);
        ctx.create_effect(MODEL_SHOCKWAVE_GROUND01, e.position, e.angle, light, 1, me(e), -1, 0, 0, 1.0);
        e.life_time = 80.0;
    }

    if a.frame > 5.0 {
        let mut light = Vec3::ONE;
        let scale = e.scale * (ctx.rand() % 5 + 18) as f32 * 0.1;
        let mut p = e.position;
        let d = (80 - life(e)) as f32;
        p.z += d * 1.0 * 10.0 - d * 3.0;
        fire(ctx, p, e.angle, light, [0, 4, 0], scale);

        let temp = p;
        light *= 0.5;
        for i in 0..8 {
            if i % 2 == 0 {
                continue;
            }
            let x = (ctx.rand() % 100 - 50) as f32;
            let y = (ctx.rand() % 100 - 50) as f32;
            let pos_l = Vec3::new(x, y, 0.0);
            let ang = Vec3::new(rf(ctx, 90), 0.0, 45.0 * i as f32);
            p += rotate(ang, pos_l);
            p.z = temp.z;
            ctx.create_particle(BITMAP_FIRE_HIK1, p, e.angle, light, 5, scale, None);
            let mut t = p;
            t.z = 250.0;
            ctx.create_particle(BITMAP_SPARK, t, e.angle, e.light, 12, 1.5, None);
        }
    }
}

// --- MODEL_DRAGON_LOWER_DUMMY -----------------------------------------------------------------------

/// ZzzEffect.cpp:6412.
fn dragon_lower_create(e: &mut Effect, ctx: &mut Ctx) {
    // o->Light = Light and o->Scale = Scale (already).
    e.velocity = 0.3;
    e.life_time = 300.0;
    e.alpha = 1.0;
    e.position.z = ctx.terrain_height(e.position.x, e.position.y) + 10.0;
    e.angle.z = 45.0 + rf(ctx, 180);
    // Models[o->Type].Actions[o->CurrentAction].PlaySpeed = 0.03f and o->m_iAnimation = 0: not read
    // by this type (PlayAnimation takes o->Velocity).
}

/// ZzzEffect.cpp:17879, plus what RenderEffects (19103) and Draw_RenderObject (ZzzObject.cpp:2570)
/// change while drawing, moved to the end of the tick.
fn dragon_lower_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    // pModel->PlayAnimation(..., o->Velocity, ...) inside the case, the raw BMD call (the common
    // tail plays it again and ends the action). With CurrentAction 255 the original would read
    // Actions[255]; nothing is played then.
    if e.current_action != 255 {
        let speed = e.velocity;
        common::play_model_animation(e, ctx, speed);
    }
    if e.animation_frame > 1.0 && e.animation_frame < 6.0 {
        let mut temp = e.position;
        temp.z = 250.0;
        ctx.create_particle(BITMAP_SPARK, temp, e.angle, e.light, 12, 1.5, None);
    }
    if e.animation_frame > 1.0 && e.life_time > 80.0 {
        ctx.create_effect(BITMAP_LIGHT_RED, e.position, e.angle, Vec3::ONE, 1, me(e), -1, 0, 0, 15.0);
        e.life_time = 80.0;
    }
    e.alpha *= 0.96;
    move_tail(e, ctx);

    // RenderEffects 19105-19107: the raw PlayAnimation(..., o->Velocity / 6, ...) every rendered
    // frame (wrapping like the BMD call; CurrentAction 255 as above).
    if e.current_action != 255 {
        let speed = e.velocity / 6.0;
        common::play_model_animation(e, ctx, speed);
    }
    // Draw_RenderObject 2572/2579 (only when Calc_RenderObject passes, Alpha >= 0.01) leaves
    // o->Light = (1, 0.2, 0.1), which the next tick's sparks use.
    if e.alpha >= 0.01 {
        e.light = Vec3::new(1.0, 0.2, 0.1);
    }
}

/// ZzzObject.cpp:2570: meshes 0 and 2 (grandmark2.jpg) with `BodyLight = (1, 0.8, 0.2) * Alpha` and
/// mesh 1 (lines2.jpg) with `(1, 0.2, 0.1) * Alpha`, all `RENDER_TEXTURE|RENDER_BRIGHT`, each mesh its
/// own blend mesh scrolling V by `-(int)WorldTime % 2000 * 0.0001`.
fn dragon_lower_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    let v = ((ctx.world_time as i32).wrapping_neg() % 2000) as f32 * 0.0001;
    for (i, light) in [(0, Vec3::new(1.0, 0.8, 0.2)), (2, Vec3::new(1.0, 0.8, 0.2)), (1, Vec3::new(1.0, 0.2, 0.1))] {
        let Some(mut d) = mesh(e, light * e.alpha, i, -1) else { return };
        d.blend_mesh_tex_coord.y = v;
        out.models.push(d);
    }
}

// --- MODEL_TARGETMON_EFFECT -------------------------------------------------------------------------

/// ZzzEffect.cpp:6428.
fn targetmon_create(e: &mut Effect, _ctx: &mut Ctx) {
    // o->Scale = Scale and o->Light = Light (already).
    e.life_time = 100.0;
}

fn targetmon_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    targetmon_move(e, ctx);
    move_tail(e, ctx);
}

/// ZzzEffect.cpp:17903.
fn targetmon_move(e: &mut Effect, ctx: &mut Ctx) {
    let Some(a) = ctx.actor(e.owner).filter(|a| a.live).cloned() else {
        e.live = false;
        e.life_time = 0.0;
        return;
    };
    if e.sub_type == 1 && owner_target_position(e, ctx).is_none() {
        return;
    }
    // rand() % (Models[o->Owner->Type].NumBones - 1) (the owner snapshot's bone count without the
    // model file): undefined for a model with one bone; bone 0 then.
    let n = ctx.model_bone_count(a.ty).unwrap_or(a.bones.len()) as i32 - 1;
    let r = ctx.rand();
    let bone = if n > 0 { r % n } else { 0 };
    let p = a.bone_world(bone as usize, Vec3::ZERO);
    let light = Vec3::ONE;
    let scale = e.scale * (ctx.rand() % 5 + 5) as f32 * 0.1;
    e.position = p;
    for _ in 0..3 {
        fire(ctx, e.position, e.angle, light, [0, 4, 0], scale);
    }
}

// --- MODEL_VOLCANO_OF_MONK --------------------------------------------------------------------------

/// ZzzEffect.cpp:6435.
fn volcano_create(e: &mut Effect, ctx: &mut Ctx) {
    match e.sub_type {
        1 => {
            // o->Scale = Scale, o->Light = Light, Position = Position (already).
            e.position.z = ctx.terrain_height(e.position.x, e.position.y);
            // Models[o->Type].Actions[0].PlaySpeed = 0.1f: not read (PlayAnimation takes o->Velocity).
            e.life_time = 100.0;
            e.velocity = 0.6;
            for _ in 0..4 {
                ctx.create_effect(MODEL_VOLCANO_STONE, e.position, e.angle, Vec3::ONE, e.sub_type, me(e), -1, 0, 0, 1.0);
            }
            ctx.create_effect(BITMAP_LIGHT_RED, e.position, e.angle, Vec3::ONE, 3, me(e), -1, 0, 0, 3.0);
        }
        2 | 3 => {
            e.life_time = (20 + ctx.rand() % 10) as f32;
            if e.sub_type == 3 {
                e.life_time = (30 + ctx.rand() % 20) as f32;
            }
            // o->Light = Light, Position = Position (already).
        }
        _ => {}
    }
}

fn volcano_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    volcano_move(e, ctx);
    move_tail(e, ctx);
}

/// ZzzEffect.cpp:17942.
fn volcano_move(e: &mut Effect, ctx: &mut Ctx) {
    match e.sub_type {
        1 => {
            let light = Vec3::ONE;
            let mut p = e.position;
            p.z += (80 - life(e)) as f32 * (0.9 + rf(ctx, 5) * 0.1);
            p.z += e.sub_type as f32 * 20.0;
            if p.z > 400.0 {
                return;
            }
            let scale = e.scale * (ctx.rand() % 5 + 8) as f32 * 0.19;
            fire(ctx, p, e.angle, light, [6, 9, 6], scale);

            if life(e) == 10 {
                for _ in 0..4 {
                    ctx.create_effect(MODEL_VOLCANO_STONE, e.position, e.angle, light, 0, me(e), -1, 0, 0, 1.0);
                }
                ctx.create_effect(MODEL_SHOCKWAVE_GROUND01, e.position, e.angle, Vec3::new(1.0, 0.1, 0.0), 2, me(e), -1, 0, 0, 1.0);
            }
            if life(e) < 20 {
                e.alpha = life(e) as f32 * 0.1;
            }
            if life(e) < 30 {
                e.blend_mesh_light = life(e) as f32 * 0.03;
            }
            let lum = rf(ctx, 5) * 0.01;
            ctx.add_terrain_light(e.position.x, e.position.y, Vec3::new(lum + 1.0, lum + 0.4, lum + 0.4), 1.0);
        }
        2 | 3 => {
            if !owner_live(e, ctx) {
                e.live = false;
                e.life_time = 0.0;
                return;
            }
            if life(e) == 2 {
                ctx.create_effect(MODEL_VOLCANO_OF_MONK, e.position, e.angle, e.light, 1, me(e), -1, 0, 0, 1.0);
                e.life_time = 0.0;
                e.live = false;
            }
            e.position.z = -100.0;
        }
        _ => {}
    }
}

/// ZzzObject.cpp:2585: only subtype 1 is drawn: `BodyLight = Light`, `RenderBody(RENDER_TEXTURE, ...)`
/// and `RenderMesh(0, RENDER_TEXTURE|RENDER_BRIGHT, Alpha, 0, ...)` with the texture `BITMAP_VOLCANO_CORE`.
fn volcano_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    if e.sub_type == 1 {
        out.models.extend(model(e, e.light, RENDER_TEXTURE));
        out.models.extend(mesh(e, e.light, 0, BITMAP_VOLCANO_CORE));
    }
}

// --- MODEL_VOLCANO_STONE ----------------------------------------------------------------------------

/// ZzzEffect.cpp:6466.
fn volcano_stone_create(e: &mut Effect, ctx: &mut Ctx) {
    // o->Scale = Scale + ... (Scale is the engine's e.scale; a Scale <= 0 argument becomes 0.9 there).
    e.scale += (ctx.rand() % 20 + 5) as f32 * 0.06;
    // o->Light = Light, Position = Position (already).
    e.direction = Vec3::ZERO;
    e.life_time = (ctx.rand() % 10 + 30) as f32;
    e.gravity = (ctx.rand() % 2 + 2) as f32;
    e.angle.z = rf(ctx, 360);
    let p1 = Vec3::new(0.0, (ctx.rand() % 128 + 64) as f32 * 0.1, 0.0);
    e.head_angle = rotate(e.angle, p1);
    e.head_angle.z += 25.0;
}

/// ZzzEffect.cpp:18014.
fn volcano_stone_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    e.head_angle.z -= e.gravity;
    e.position += e.head_angle;
    let height = ctx.terrain_height(e.position.x, e.position.y);
    let lt = life(e) as f32;
    e.angle.x += 0.3 * lt;
    e.angle.y += 0.3 * lt;
    if e.position.z + e.direction.z <= height {
        e.position.z = height;
        e.head_angle.x *= 0.6;
        e.head_angle.y *= 0.6;
        e.head_angle.z += 1.0 * lt;
        if e.head_angle.z < 0.5 {
            e.head_angle.z = 0.0;
        }
        e.alpha -= 0.05;
    }
    e.scale -= 0.03;

    let scale = e.scale * (ctx.rand() % 5 + 5) as f32 * 0.1;
    fire(ctx, e.position, e.angle, Vec3::ONE, [0, 4, 0], scale);
    move_tail(e, ctx);
}

/// ZzzObject.cpp:2595: `BodyLight = Light`, `RenderBody(RENDER_TEXTURE, ...)`.
fn volcano_stone_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    out.models.extend(model(e, e.light, RENDER_TEXTURE));
}
