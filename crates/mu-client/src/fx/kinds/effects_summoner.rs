//! Summoner effects: the wrist ring (`MODEL_SUMMONER_WRISTRING_EFFECT`), the summon heads that
//! orbit the owner (`MODEL_SUMMONER_EQUIP_HEAD_SAHAMUTT/NEIL/LAGUL`), the casting circles
//! (`MODEL_SUMMONER_CASTING_EFFECT1..4`), the summons Sahamutt, Neil (+ knives and ground blades)
//! and Lagul, Lightning Orb (`MODEL_LIGHTNING_ORB`), Chain Lightning (`MODEL_CHAIN_LIGHTNING`),
//! Drain Life (`MODEL_ALICE_DRAIN_LIFE`), the Alice buff/debuff rings
//! (`MODEL_ALICE_BUFFSKILL_EFFECT`/`2`), Lightning Shock (`MODEL_LIGHTNING_SHOCK`) and Swell of
//! Magic Power (`MODEL_SWELL_OF_MAGICPOWER` + `MODEL_SWELL_OF_MAGICPOWER_BUFF_EFF`).
//!
//! Translated from Main 5.2 `ZzzEffect.cpp` (`EffectDestructor` 240, `CheckTargetRange` 411,
//! `CreateEffect` 796-1109, `MoveEffect` 7856-8233, 8953-9593, 15395-15561 and its common tail
//! 18060-18124, `RenderEffects` 18427-18495, 18708, 18853) and the type branches of
//! `Draw_RenderObject` (`ZzzObject.cpp` 612, 1718-1795, 1913), which is what `RenderObject(o)` runs
//! for them.
//!
//! Notes:
//! - `HeadTargetAngle` (the summons' target point) lives in `user[0..3]`.
//! - The original uses the effect models' OWN bones in a few places (Sahamutt's fire, Neil's blade
//!   sprites, Lagul's sparkles); the engine has no access to them: see [`own_bone`].
//! - `MODEL_ARROWSRE06` (created here by Swell of Magic Power) belongs to another batch.

use bevy::math::Vec3;

use super::common::{create_bomb3, draw_nothing, luminosity, move_tail, owner_actor, rf, sprite};
use crate::fx::core::*;

pub fn register(r: &mut Registry) {
    let k = |create: EffectCreate, step: EffectStep, draw: EffectDraw| EffectKind { create: Some(create), step: Some(step), draw: Some(draw) };
    r.effect(MODEL_SUMMONER_WRISTRING_EFFECT, k(wristring_create, wristring_step, draw_plain));
    r.effect(MODEL_SUMMONER_EQUIP_HEAD_SAHAMUTT, k(head_sahamutt_create, head_sahamutt_step, head_draw));
    r.effect(MODEL_SUMMONER_EQUIP_HEAD_NEIL, k(head_neil_create, head_neil_step, head_draw));
    // RenderEffects has no case for it: the default (`RenderObject`, MODEL_SKILL_BEGIN..END).
    r.effect(MODEL_SUMMONER_EQUIP_HEAD_LAGUL, k(head_lagul_create, head_lagul_step, draw_plain));
    for ty in [
        MODEL_SUMMONER_CASTING_EFFECT1,
        MODEL_SUMMONER_CASTING_EFFECT11,
        MODEL_SUMMONER_CASTING_EFFECT111,
        MODEL_SUMMONER_CASTING_EFFECT2,
        MODEL_SUMMONER_CASTING_EFFECT22,
        MODEL_SUMMONER_CASTING_EFFECT222,
    ] {
        r.effect(ty, k(casting_create, casting_step, casting_draw));
    }
    r.effect(MODEL_SUMMONER_CASTING_EFFECT4, k(casting4_create, casting_step, casting_draw));
    r.effect(MODEL_SUMMONER_SUMMON_SAHAMUTT, k(sahamutt_create, sahamutt_step, sahamutt_draw));
    r.effect(MODEL_SUMMONER_SUMMON_NEIL, k(neil_create, neil_step, neil_draw));
    for ty in [MODEL_SUMMONER_SUMMON_NEIL_NIFE1, MODEL_SUMMONER_SUMMON_NEIL_NIFE2, MODEL_SUMMONER_SUMMON_NEIL_NIFE3] {
        r.effect(ty, k(nife_create, nife_step, nife_draw));
    }
    for ty in [MODEL_SUMMONER_SUMMON_NEIL_GROUND1, MODEL_SUMMONER_SUMMON_NEIL_GROUND2, MODEL_SUMMONER_SUMMON_NEIL_GROUND3] {
        r.effect(ty, k(ground_create, ground_step, ground_draw));
    }
    r.effect(MODEL_SUMMONER_SUMMON_LAGUL, k(lagul_create, lagul_step, lagul_draw));
    // Types below MODEL_SKILL_BEGIN with no RenderEffects case: nothing is drawn.
    r.effect(MODEL_LIGHTNING_ORB, k(orb_create, orb_step, draw_nothing));
    r.effect(MODEL_CHAIN_LIGHTNING, k(chain_create, chain_step, draw_nothing));
    r.effect(MODEL_ALICE_DRAIN_LIFE, k(drain_create, drain_step, draw_nothing));
    r.effect(MODEL_ALICE_BUFFSKILL_EFFECT, k(alice_buff_create, alice_buff_step, alice_buff_draw));
    r.effect(MODEL_ALICE_BUFFSKILL_EFFECT2, k(alice_buff2_create, alice_buff_step, alice_buff_draw));
    r.effect(MODEL_LIGHTNING_SHOCK, k(shock_create, shock_step, draw_nothing));
    // RenderEffects default (`RenderObject`, MODEL_SKILL_BEGIN..END) + Draw_RenderObject 1913.
    r.effect(MODEL_SWELL_OF_MAGICPOWER, k(swell_create, swell_step, swell_draw));
    r.effect(MODEL_SWELL_OF_MAGICPOWER_BUFF_EFF, k(swell_buff_create, swell_buff_step, swell_buff_draw));
}

// --- Helpers ----------------------------------------------------------------------------------------

/// `o->LifeTime` as the original's `int`.
fn life(e: &Effect) -> i32 {
    e.life_time as i32
}

/// `o->HeadTargetAngle` (user[0..3]).
fn head_target(e: &Effect) -> Vec3 {
    Vec3::new(e.user[0], e.user[1], e.user[2])
}

fn set_head_target(e: &mut Effect, v: Vec3) {
    e.user[0] = v.x;
    e.user[1] = v.y;
    e.user[2] = v.z;
}

/// `SetAction(o, Action)` (ZzzAI.cpp:444).
/// TODO(original): SetAction returns without changes when `Action >= NumActions` of the model
/// (unknown here); the summon models have the actions used.
fn set_action(e: &mut Effect, action: usize) {
    if e.current_action != action {
        e.current_action = action;
        e.animation_frame = 0.0;
    }
}

/// `b->TransformPosition(BoneTransform[bone], p, out)` with the effect's OWN model bones, placed at
/// `origin` (`BodyOrigin`).
/// TODO(original): the engine gives no access to an effect model's bones; the bone is taken as an
/// identity bone at the model origin (`origin + Angle·(p·Scale)`).
fn own_bone(e: &Effect, origin: Vec3, _bone: usize, p: Vec3) -> Vec3 {
    origin + rotate(e.angle, p * e.scale)
}

/// The character owner (or target), cloned so the context can be used mutably afterwards.
fn actor_of(ctx: &Ctx, t: Option<Target>) -> Option<Actor> {
    ctx.actor(t).cloned()
}

/// `CreateJoint(..., PKKey, SkillIndex, SkillSerialNum, iChaIndex, vPriorColor)`: a trail with
/// its starting light (`vPriorColor`).
#[allow(clippy::too_many_arguments)]
fn joint_lit(ctx: &mut Ctx, ty: i32, pos: Vec3, target_pos: Vec3, angle: Vec3, sub: i32, target: Option<Target>, scale: f32, pk_key: i32, skill: i32, light: Vec3) {
    let mut j = Joint::new(ty, pos, target_pos, angle, sub, target, scale);
    j.pk_key = pk_key;
    j.skill = skill;
    j.light = light;
    ctx.spawns.push(Spawn::Joint(j));
}

/// `RenderObject(o)`: nothing when `Alpha < 0.01` (`Calc_RenderObject`, ZzzObject.cpp:245).
fn model(e: &Effect) -> Option<ModelDraw> {
    if e.alpha < 0.01 {
        return None;
    }
    Some(ModelDraw::of_effect(e))
}

/// `RenderObject(o)` through the generic `RenderBody(RENDER_TEXTURE, ...)` of `Draw_RenderObject`.
fn draw_plain(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    if let Some(m) = model(e) {
        out.models.push(m);
    }
}

/// A `CreateSprite` (subtype 0: additive) made while rendering.
fn render_sprite(out: &mut DrawList, ctx: &DrawCtx, texture: i32, position: Vec3, scale: f32, light: Vec3) {
    sprite(out, ctx, texture, position, scale, light, 0.0, Some(Blend::Glow));
}

// --- MODEL_SUMMONER_WRISTRING_EFFECT ------------------------------------------------------------------

/// ZzzEffect.cpp:827.
fn wristring_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.blend_mesh = -2;
    e.life_time = 100.0;
    e.scale = 0.7;
    e.direction = Vec3::ZERO;
}

/// ZzzEffect.cpp:7856: follows the owner's bone 37 (left wrist).
fn wristring_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    // A missing owner would crash the original; here the ring just stays.
    if let Some(a) = actor_of(ctx, e.owner) {
        e.position = a.bone_world(37, Vec3::ZERO);
        if a.live {
            e.life_time = 100.0;
        }
    }
    // b->PlayAnimation(..., o->Velocity / 5, ...)
    // TODO(original): PlayAnimation wraps the frame at the action length (unknown here).
    e.animation_frame += e.velocity / 5.0;
    move_tail(e);
}

// --- MODEL_SUMMONER_EQUIP_HEAD_SAHAMUTT / NEIL / LAGUL --------------------------------------------------

/// The heads' starting point around the owner (ZzzEffect.cpp:842-844, 859-861, 876-878).
fn head_start(e: &mut Effect, ctx: &Ctx) {
    e.scale = 0.8;
    e.direction = Vec3::ZERO;
    e.alpha = 0.0;
    // A missing owner would crash the original; here the head stays where it was created.
    if let Some(p) = ctx.actor(e.owner).map(|a| a.position) {
        let t = ctx.world_time();
        e.position.x = p.x + (t * 0.003).cos() * 40.0;
        e.position.y = p.y + (t * 0.003).sin() * 40.0;
        e.position.z = p.z + ((t * 0.0010).sin() + 2.0) * 80.0 - 60.0;
    }
}

/// The heads' orbit around the owner, fading in and out (ZzzEffect.cpp:7877-7903, 7928-7954,
/// 7979-8005). Returns the owner if it exists.
fn head_orbit(e: &mut Effect, ctx: &mut Ctx) -> Option<Actor> {
    let a = actor_of(ctx, e.owner)?;
    let t = ctx.world_time();
    let s = e.skill as f32 * 0.024;
    e.position.x = a.position.x + (t * 0.003 + s).cos() * 60.0;
    e.position.y = a.position.y + (t * 0.003 + s).sin() * 60.0;
    e.position.z = a.position.z + ((t * 0.0010 + s).sin() + 2.0) * 80.0 - 60.0;

    let rel = e.position - a.position;
    if e.start_position.x != rel.x {
        let angle = create_angle(e.start_position.x, e.start_position.y, rel.x, rel.y);
        e.angle.z = angle + 0.0;
    }
    e.start_position = rel;

    if e.kind == 1 {
        if e.alpha > 0.0 {
            e.alpha -= 0.03;
        } else {
            ctx.delete_effect(e.ty, e.owner, -1);
        }
    } else {
        // TODO(original): also `Hero->SafeZone` (the hero standing in a safe zone) sets Kind = 1.
        if (t * 0.0004 + s).sin() < 0.3 {
            e.kind = 1;
        }
        if e.alpha < 1.0 {
            e.alpha += 0.03;
        }
    }

    if a.live {
        e.life_time = 100.0;
    }
    Some(a)
}

/// ZzzEffect.cpp:834.
fn head_sahamutt_create(e: &mut Effect, ctx: &mut Ctx) {
    if e.sub_type == 0 {
        e.life_time = 100.0;
        head_start(e, ctx);
    } else if e.sub_type == 1 {
        e.life_time = 100000.0;
    }
}

/// ZzzEffect.cpp:7874.
fn head_sahamutt_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    if e.sub_type == 0 {
        if head_orbit(e, ctx).is_some() {
            let light = Vec3::splat(e.alpha * 0.3);
            ctx.create_particle(BITMAP_FIRE_CURSEDLICH, e.position, e.angle, light, 1, 1.0, e.owner);
        }
    } else if e.sub_type == 1 && ctx.rand() % 2 == 0 {
        match actor_of(ctx, e.owner) {
            Some(a) if a.live => {
                let num_bones = a.bones.len() as i32;
                if num_bones > 0 {
                    let bone = ctx.rand() % num_bones;
                    // TODO(original): the light is the owner's `o->Light` (not in `Actor`).
                    ctx.create_effect(BITMAP_FIRE_CURSEDLICH, a.position, a.angle, Vec3::ONE, 0, e.owner, -1, 0, bone, 0.0);
                }
            }
            // `pObject->Live == false` (a missing owner counts as dead): `o->Live = false; break;`
            _ => e.live = false,
        }
    }
    move_tail(e);
}

/// ZzzEffect.cpp:851.
fn head_neil_create(e: &mut Effect, ctx: &mut Ctx) {
    if e.sub_type == 0 {
        e.life_time = 100.0;
        head_start(e, ctx);
        ctx.create_joint_ex(MODEL_SPEARSKILL, e.position, e.position, e.angle, 15, Some(Target::Effect(e.id)), 40.0, -1, 0);
    } else if e.sub_type == 1 {
        e.life_time = 100000.0;
    }
}

/// ZzzEffect.cpp:7925.
fn head_neil_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    if e.sub_type == 0 {
        if head_orbit(e, ctx).is_some() {
            // (vLight = Alpha is computed but the particles use o->Light.)
            ctx.create_particle(BITMAP_LIGHT + 2, e.position, e.angle, e.light, 3, 0.30, e.owner);
            if ctx.rand() % 2 == 0 {
                ctx.create_particle(BITMAP_LIGHT + 2, e.position, e.angle, e.light, 3, 0.30, e.owner);
            }
        }
    } else if e.sub_type == 1 {
        match actor_of(ctx, e.owner) {
            Some(a) if a.live => {
                ctx.create_particle(BITMAP_LIGHT + 2, a.position, a.angle, Vec3::ONE, 5, 0.1, e.owner);
            }
            _ => e.live = false,
        }
    }
    move_tail(e);
}

/// RenderEffects 18430/18436: only SubType 0.
fn head_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    if e.sub_type == 0 {
        draw_plain(e, ctx, out);
    }
}

/// ZzzEffect.cpp:869.
fn head_lagul_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = 100.0;
    head_start(e, ctx);
    ctx.create_joint_ex(MODEL_SPEARSKILL, e.position, e.position, e.angle, 17, Some(Target::Effect(e.id)), 40.0, -1, 0);
}

/// ZzzEffect.cpp:7977. The case has no `break`: it falls through into the casting effects' code
/// (8011), whose inner switch does nothing for this type (MuMain keeps the fall-through too).
fn head_lagul_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    if head_orbit(e, ctx).is_some() {
        let light = Vec3::new(e.alpha * 0.7, e.alpha * 0.3, e.alpha * 1.0);
        ctx.create_particle(BITMAP_CLUD64, e.position, e.angle, light, 10, 1.0, e.owner);
    }
    casting_move(e);
    move_tail(e);
}

// --- MODEL_SUMMONER_CASTING_EFFECT1..4 ---------------------------------------------------------------

/// ZzzEffect.cpp:882-897.
fn casting_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 40.0;
    // `if (o->SubType = 0) o->Scale = 1.0f;`: an assignment, so SubType becomes 0 and the scale
    // is never set (it keeps the CreateEffect argument, or 0.9).
    e.sub_type = 0;
    e.alpha = 1.0;
    e.direction = Vec3::ZERO;
    e.blend_mesh = 0;
    e.blend_mesh_light = 0.0;
}

/// ZzzEffect.cpp:898.
fn casting4_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 25.0;
    e.scale = 1.0;
    e.alpha = 1.0;
    e.direction = Vec3::ZERO;
    e.blend_mesh = 0;
    e.blend_mesh_light = 0.0;
}

/// ZzzEffect.cpp:8018-8046.
fn casting_move(e: &mut Effect) {
    if life(e) < 20 {
        e.blend_mesh_light -= 0.03;
    } else if e.blend_mesh_light < 0.5 {
        e.blend_mesh_light += 0.05;
    }
    match e.ty {
        MODEL_SUMMONER_CASTING_EFFECT1 | MODEL_SUMMONER_CASTING_EFFECT111 | MODEL_SUMMONER_CASTING_EFFECT22 => e.angle.z -= 3.0,
        MODEL_SUMMONER_CASTING_EFFECT11 | MODEL_SUMMONER_CASTING_EFFECT2 | MODEL_SUMMONER_CASTING_EFFECT222 => e.angle.z += 3.0,
        MODEL_SUMMONER_CASTING_EFFECT4 => e.scale += 0.6,
        _ => {}
    }
}

/// ZzzEffect.cpp:8011.
fn casting_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    casting_move(e);
    move_tail(e);
}

/// RenderEffects 18442 + Draw_RenderObject (ZzzObject.cpp:1718):
/// `RenderBody(RENDER_TEXTURE|RENDER_BRIGHT, Alpha, BlendMesh, BlendMeshLight, U, V)`.
/// TODO(original): the meshes other than the blend mesh are drawn additive (RENDER_BRIGHT); the
/// engine's `bright` would override the blend mesh, so they are drawn normally.
fn casting_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    draw_plain(e, ctx, out);
}

// --- MODEL_SUMMONER_SUMMON_SAHAMUTT --------------------------------------------------------------------

/// ZzzEffect.cpp:906. `Light` carries the target point (`HeadTargetAngle`).
fn sahamutt_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 80.0;
    if e.sub_type == 2 {
        e.scale = 0.7;
    } else if e.sub_type == 1 {
        e.scale = 0.5;
    } else if e.sub_type == 0 {
        e.scale = 0.35;
    }
    e.alpha = 0.0;
    e.velocity = 0.5;
    let target = e.light;
    set_head_target(e, target);
    e.light = Vec3::ONE;
}

/// `o->Angle[2]` and `o->Distance` toward the target point (ZzzEffect.cpp:8069-8072).
fn aim_at_target(e: &mut Effect) {
    let t = head_target(e);
    e.angle.z = create_angle(e.position.x, e.position.y, t.x, t.y);
    let dx = t.x - e.position.x;
    let dy = t.y - e.position.y;
    e.distance = (dx * dx + dy * dy).sqrt();
}

/// ZzzEffect.cpp:8055: appears, leaps at the target point and explodes.
fn sahamutt_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    let start = e.position;

    if life(e) < 20 {
        set_action(e, 0);
        e.alpha -= 0.05;
    } else if e.current_action == 0 && e.alpha < 0.3 {
        e.alpha += 0.05;
        aim_at_target(e);
    } else {
        set_action(e, 1);
        if e.animation_frame >= 12.0 {
            e.animation_frame = 12.0;
        }

        if e.animation_frame >= 11.0 {
            if e.alpha > 0.0 {
                e.alpha -= 0.3;
            } else {
                e.alpha = 0.0;
            }
        } else if e.animation_frame < 3.0 && e.alpha < 0.7 {
            e.alpha += 0.05;
        }

        if e.animation_frame > 4.0 && e.animation_frame < 12.0 {
            let dir = if e.animation_frame < 10.0 { Vec3::new(0.0, e.distance / -13.0, 0.0) } else { Vec3::new(0.0, e.distance / -45.0, 0.0) };
            e.position += rotate(e.angle, dir);
        }
        if e.animation_frame <= 4.0 {
            aim_at_target(e);
        }
    }
    e.position.z = ctx.terrain_height(e.position.x, e.position.y);

    let temp = start - e.position;

    if e.animation_frame >= 11.0 {
        create_bomb3(ctx, e.position, e.sub_type, 1.0);
        if ctx.rand() % 2 == 0 {
            ctx.play_sound("SOUND_SUMMON_EXPLOSION");
        }
    }

    // Fire on 8 bones of its own model: `pModel->Animation(BoneTransform, ...)` (bones in world
    // space at the model's last `BodyOrigin`, i.e. before this tick's move) + vTempPosition.
    let light = Vec3::splat(e.alpha * 0.3);
    for bone in [13, 23, 39, 49, 3, 4, 5, 61] {
        let p = own_bone(e, start, bone, Vec3::ZERO) + temp;
        let me = Some(Target::Effect(e.id));
        ctx.create_particle(BITMAP_FIRE_CURSEDLICH, p, e.angle, light, 2, 5.0, me);
        ctx.create_particle(BITMAP_FIRE_CURSEDLICH, p, e.angle, light, 2, 4.0, me);
        ctx.create_particle(BITMAP_FIRE_CURSEDLICH, p, e.angle, light, 2, 3.0, me);
    }
    move_tail(e);
}

/// RenderEffects 18451 + Draw_RenderObject (ZzzObject.cpp:1722): `RenderBody(RENDER_TEXTURE)`.
/// TODO(original): a second pass `RenderBody(RENDER_BRIGHT|RENDER_CHROME7, Alpha, ..., BlendMeshLight
/// = Alpha)` adds a chrome shine (no chrome rendering in the engine).
fn sahamutt_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    draw_plain(e, ctx, out);
}

// --- MODEL_SUMMONER_SUMMON_NEIL ---------------------------------------------------------------------------

/// ZzzEffect.cpp:920. `Light` carries the target point (`HeadTargetAngle`).
fn neil_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 80.0;
    e.scale = 1.0;
    e.alpha = 0.0;
    e.velocity = 0.35;
    e.skill = 0;
    e.position.z += 10.0;
    let target = e.light;
    set_head_target(e, target);
    e.light = Vec3::ONE;
}

/// ZzzEffect.cpp:8145: throws the knives and plants the ground blades.
fn neil_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    if life(e) < 20 {
        e.alpha -= 0.05;
    } else if e.alpha < 0.7 {
        e.alpha += 0.04;
    }

    let target = head_target(e);
    if e.animation_frame > 8.0 && e.skill == 0 {
        e.skill = 1;
        ctx.create_effect(MODEL_SUMMONER_SUMMON_NEIL_NIFE1, target, e.angle, e.light, e.sub_type, None, -1, 0, 0, 0.0);
        if e.sub_type >= 1 {
            ctx.create_effect(MODEL_SUMMONER_SUMMON_NEIL_NIFE2, target, e.angle, e.light, e.sub_type, None, -1, 0, 0, 0.0);
        }
        if e.sub_type >= 2 {
            ctx.create_effect(MODEL_SUMMONER_SUMMON_NEIL_NIFE3, target, e.angle, e.light, e.sub_type, None, -1, 0, 0, 0.0);
        }
    }
    if e.animation_frame > 10.0 && e.skill == 1 {
        e.skill = 2;
        let p = e.position + rotate(e.angle, Vec3::new(0.0, -60.0, 0.0));
        ctx.create_effect(MODEL_SUMMONER_SUMMON_NEIL_GROUND1, p, e.angle, e.light, e.sub_type, Some(Target::Effect(e.id)), -1, 0, 0, 0.0);

        ctx.create_effect(MODEL_SUMMONER_SUMMON_NEIL_GROUND1, target, e.angle, e.light, e.sub_type, None, -1, 0, 0, 0.0);
        if e.sub_type >= 1 {
            ctx.create_effect(MODEL_SUMMONER_SUMMON_NEIL_GROUND2, target, e.angle, e.light, e.sub_type, None, -1, 0, 0, 0.0);
        }
        if e.sub_type >= 2 {
            ctx.create_effect(MODEL_SUMMONER_SUMMON_NEIL_GROUND3, target, e.angle, e.light, e.sub_type, None, -1, 0, 0, 0.0);
        }
        ctx.play_sound("SOUND_SUMMON_REQUIEM");
    }
    move_tail(e);
}

/// RenderEffects 18454 + Draw_RenderObject (ZzzObject.cpp:1727): meshes 0 and 2 additive with
/// `BlendMeshLight = Alpha` while `Alpha < 0.7` (normal afterwards); mesh 1 additive red
/// (`BodyLight = (1, 0, 0)`, `BlendMeshLight = Alpha`); red sprites on bones 51..59.
/// TODO(original): mesh 1 is tinted (1, 0, 0) (one light per model draw here); the blade trail
/// `CreateObjectBlur(o, bone 51, bone 59, (1, 0, 0), 2)` sampled 10 times per frame while
/// `AnimationFrame <= 10` is not drawn (no blurs on effects).
fn neil_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    let Some(mut m) = model(e) else { return };
    if e.alpha < 0.7 {
        m.blend_mesh = -2;
        m.blend_mesh_light = e.alpha;
    } else {
        m.blend_mesh = 1;
        m.blend_mesh_light = e.alpha;
    }
    out.models.push(m);
    for bone in 51..=59 {
        let p = own_bone(e, e.position, bone, Vec3::ZERO);
        render_sprite(out, ctx, BITMAP_LIGHT, p, 1.0, Vec3::new(1.0, 0.0, 0.0));
    }
}

// --- MODEL_SUMMONER_SUMMON_NEIL_NIFE1..3 -------------------------------------------------------------------

/// ZzzEffect.cpp:931.
fn nife_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 50.0;
    e.scale = 1.0;
    e.alpha = 1.0;
}

/// ZzzEffect.cpp:8182.
fn nife_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    if life(e) < 20 {
        e.alpha -= 0.05;
    } else if e.alpha < 1.0 {
        e.alpha += 0.05;
    }
    move_tail(e);
}

/// RenderEffects 18457 + Draw_RenderObject (ZzzObject.cpp:1779): meshes 0 and 2 normal, mesh 1
/// additive (`BlendMesh 1`, `BlendMeshLight = Alpha`).
/// TODO(original): mesh 1 is tinted (1, 0, 0) and RENDER_BRIGHT; meshes beyond 2 are not drawn.
fn nife_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    let Some(mut m) = model(e) else { return };
    m.blend_mesh = 1;
    m.blend_mesh_light = e.alpha;
    out.models.push(m);
}

// --- MODEL_SUMMONER_SUMMON_NEIL_GROUND1..3 -----------------------------------------------------------------

/// ZzzEffect.cpp:938.
fn ground_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 50.0;
    e.scale = 1.0;
    e.alpha = 0.0;
}

/// ZzzEffect.cpp:8190.
fn ground_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    if life(e) < 20 {
        e.alpha -= 0.05;
    } else if e.alpha < 1.0 {
        e.alpha += 0.3;
    }
    move_tail(e);
}

/// RenderEffects 18462 + Draw_RenderObject (ZzzObject.cpp:1786): meshes 0 and 1 additive with
/// `BlendMeshLight = Alpha`; mesh 1 scrolls its U by `-WorldTime·0.001`. Drawn as two passes,
/// each hiding the other mesh.
/// TODO(original): only meshes 0 and 1 are drawn; a model with more meshes would show them twice.
fn ground_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    let Some(mut m) = model(e) else { return };
    m.blend_mesh = -2;
    m.blend_mesh_light = e.alpha;
    let mut m0 = m;
    m0.hidden_mesh = 1;
    out.models.push(m0);
    let mut m1 = m;
    m1.hidden_mesh = 0;
    m1.blend_mesh_tex_coord.x = -ctx.world_time * 0.001;
    out.models.push(m1);
}

// --- MODEL_SUMMONER_SUMMON_LAGUL ---------------------------------------------------------------------------

/// ZzzEffect.cpp:945.
fn lagul_create(e: &mut Effect, ctx: &mut Ctx) {
    if e.sub_type == 0 {
        e.life_time = 160.0;
        e.scale = 1.0;
        e.velocity = 0.5;
        let p = e.position;
        set_head_target(e, p);
    } else if e.sub_type == 1 {
        e.life_time = 160.0;
        e.alpha = 0.0;
        // TODO(original): the owner is a JOINT: `((JOINT*)o->Owner)->Target = o` makes that trail
        // follow this effect (the engine's owners are characters or effects only).
        e.owner = None;
        for i in 48..=53 {
            ctx.create_joint(BITMAP_JOINT_ENERGY, e.position, e.position, e.angle, i, Some(Target::Effect(e.id)), 10.0);
        }
    }
}

/// ZzzEffect.cpp:8198 (+ the particles RenderEffects 18484 creates for SubType 1, moved here, and
/// `EffectDestructor` 240).
fn lagul_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    if e.sub_type == 0 {
        // anEffectVolume[o->PKKey] (PKKey = summon level 0..2).
        let volume = usize::try_from(e.pk_key).ok().and_then(|i| [5, 4, 3].get(i).copied());
        if let Some(volume) = volume
            && ctx.rand() % volume == 0
        {
            let t = head_target(e);
            let mut p = t;
            p.x += rf(ctx, 500) - 250.0;
            p.y += rf(ctx, 500) - 250.0;
            ctx.create_particle(BITMAP_SMOKE, p, e.angle, Vec3::ONE, 57, 3.5, None);

            let mut p = t;
            p.x += rf(ctx, 400) - 200.0;
            p.y += rf(ctx, 400) - 200.0;
            let light = Vec3::new(0.6, 0.1, 1.0);
            ctx.create_effect(BITMAP_CLOUD, p, e.angle, light, 0, None, -1, 0, 0, 2.0);
            ctx.create_particle(BITMAP_TWINTAIL_WATER, p, e.angle, light, 1, 1.0, None);
        }
    } else if e.sub_type == 1 {
        set_action(e, 0);
        let lt = life(e);
        if lt < 30 {
            e.alpha = lt as f32 / 40.0;
        } else if lt > 144 {
            e.alpha = (160 - lt) as f32 / 20.0;
        }

        // RenderEffects (ZzzEffect.cpp:18484), done once per rendered frame: moved here.
        if ctx.rand() % 5 == 0 {
            // rand() % NumBones of its own model (unknown: see own_bone).
            let _bone = ctx.rand();
            let p = own_bone(e, e.position, 0, Vec3::ZERO);
            ctx.create_particle(BITMAP_SMOKE, p, e.angle, Vec3::new(0.6, 0.5, 0.9), 57, 2.0, None);
            ctx.create_particle(BITMAP_TWINTAIL_WATER, p, e.angle, Vec3::new(0.7, 0.7, 1.0), 1, 1.0, None);
        }
    }
    move_tail(e);

    // EffectDestructor (ZzzEffect.cpp:240) when it dies this tick.
    // TODO(original): also when another effect deletes it (`DeleteEffect`), which the engine
    // applies without calling the kind.
    if !e.live || e.life_time - 1.0 <= 0.0 {
        for i in 48..=53 {
            ctx.delete_joint(BITMAP_JOINT_ENERGY, Some(Target::Effect(e.id)), i);
        }
    }
}

/// RenderEffects 18467 (only SubType 1): sparkles on bones 54..59, then `RenderObject`
/// (Draw_RenderObject, ZzzObject.cpp:1791: meshes 0 and 1 additive with `BlendMeshLight = Alpha`).
/// TODO(original): only meshes 0 and 1 are drawn.
fn lagul_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    if e.sub_type != 1 {
        return;
    }
    let light = Vec3::new(0.2, 0.3, 1.0);
    for bone in 54..=59 {
        let p = own_bone(e, e.position, bone, Vec3::ZERO);
        render_sprite(out, ctx, BITMAP_SHINY + 6, p, e.scale * 0.3, light);
    }
    if let Some(mut m) = model(e) {
        m.blend_mesh = -2;
        m.blend_mesh_light = e.alpha;
        out.models.push(m);
    }
}

// --- MODEL_LIGHTNING_ORB -----------------------------------------------------------------------------------

/// ZzzEffect.cpp:974.
fn orb_create(e: &mut Effect, _ctx: &mut Ctx) {
    if e.sub_type == 0 {
        e.life_time = 20.0;
        e.direction = Vec3::new(0.0, -60.0, 0.0);
        e.position.z += 100.0;
    } else if e.sub_type == 1 {
        e.life_time = 18.0;
    }
}

/// `CheckTargetRange(o)` for `MODEL_LIGHTNING_ORB` (ZzzEffect.cpp:366, 411): on reaching the
/// target (the owner) it ends and bursts.
fn orb_check_target_range(e: &mut Effect, ctx: &mut Ctx) {
    // TODO(original): also requires `to->Visible` (frustum test of the target).
    let Some(to) = ctx.actor(e.owner).filter(|a| a.live).map(|a| a.position) else { return };
    let dx = e.position.x - to.x;
    let dy = e.position.y - to.y;
    let range = (dx * dx + dy * dy).sqrt();
    if range <= 100.0 {
        e.life_time = 1.0;
        ctx.create_effect(MODEL_LIGHTNING_ORB, e.position, e.angle, e.light, 1, None, -1, 0, 0, 0.0);
    }
}

/// ZzzEffect.cpp:8953.
fn orb_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    let rot = (ctx.world_time() * 0.0006) * 360.0;
    if e.sub_type == 0 {
        // shiny
        let light = Vec3::new(0.1, 0.7, 1.5);
        ctx.create_sprite(BITMAP_SHINY + 1, e.position, 4.0, light, rot, 0);
        ctx.create_sprite(BITMAP_SHINY + 1, e.position, 3.0, light, -rot, 0);
        // magic_ground
        let light = Vec3::new(0.1, 0.1, 1.5);
        ctx.create_sprite(BITMAP_MAGIC, e.position, 1.0, light, rot, 0);
        ctx.create_sprite(BITMAP_MAGIC, e.position, 0.5, light, -rot, 0);
        // pin_light
        let light = Vec3::new(0.5, 0.5, 1.5);
        let r = rf(ctx, 360);
        ctx.create_sprite(BITMAP_PIN_LIGHT, e.position, 2.0, light, r, 0);
        let r = rf(ctx, 360);
        ctx.create_sprite(BITMAP_PIN_LIGHT, e.position, 2.0, light, r, 0);

        ctx.create_particle(BITMAP_MAGIC, e.position, e.angle, Vec3::new(0.4, 0.4, 1.5), 0, 1.0, None);
        for _ in 0..3 {
            ctx.create_particle(BITMAP_SPARK + 1, e.position, e.angle, e.light, 13, 1.0, None);
        }
        orb_check_target_range(e, ctx);
    } else if e.sub_type == 1 {
        let lt = life(e);
        if lt >= 5 {
            let light = Vec3::new(0.1, 0.5, 1.5);
            ctx.create_sprite(BITMAP_SHINY + 5, e.position, 3.0, light, rot, 0);
            ctx.create_sprite(BITMAP_SHINY + 5, e.position, 2.0, light, -rot, 0);
            // pin_light
            let light = Vec3::new(e.light.x * 0.3, e.light.y * 0.3, e.light.z * 1.0);
            let r = rf(ctx, 360);
            ctx.create_sprite(BITMAP_PIN_LIGHT, e.position, 4.0, light, r, 0);
            let r = rf(ctx, 360);
            ctx.create_sprite(BITMAP_PIN_LIGHT, e.position, 4.0, light, r, 0);
            // thunder
            ctx.create_sprite(BITMAP_ENERGY, e.position, 4.0, e.light, rot, 0);
        }
        if lt >= 15 {
            for _ in 0..5 {
                ctx.create_particle(BITMAP_SPARK + 1, e.position, e.angle, e.light, 20, 1.0, None);
            }
        }
        // shockwave
        if lt >= 14 {
            let light = Vec3::new(0.4, 0.3, 1.0);
            ctx.create_particle(BITMAP_SHOCK_WAVE, e.position, e.angle, light, 0, 0.3, None);
            ctx.create_particle(BITMAP_SHOCK_WAVE, e.position, e.angle, light, 0, 0.3, None);
        }
        let light = Vec3::new(0.2, 0.2, 1.0);
        for _ in 0..2 {
            ctx.create_effect(MODEL_FENRIR_THUNDER, e.position, e.angle, light, 3, Some(Target::Effect(e.id)), -1, 0, 0, 0.0);
        }
        if lt <= 5 {
            for _ in 0..2 {
                ctx.create_particle(BITMAP_SMOKE, e.position, e.angle, e.light, 40, 1.0, None);
            }
        }
        e.light /= 1.08;
    }
    move_tail(e);
}

// --- MODEL_CHAIN_LIGHTNING -----------------------------------------------------------------------------------

/// ZzzEffect.cpp:989.
fn chain_create(e: &mut Effect, _ctx: &mut Ctx) {
    if (0..=2).contains(&e.sub_type) {
        e.life_time = 20.0;
    }
}

/// ZzzEffect.cpp:9036: thunder trails from the caster (or the previous victim) to the target.
/// The target is `e.target`.
/// TODO(original): the original finds it with `FindCharacterIndex(o->m_sTargetIndex)` (the last
/// `CreateEffect` argument); whoever creates the effect must set `target`.
fn chain_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    if (0..=2).contains(&e.sub_type) {
        chain_move(e, ctx);
    }
    move_tail(e);
}

fn chain_move(e: &mut Effect, ctx: &mut Ctx) {
    let source = actor_of(ctx, e.owner);
    let target = actor_of(ctx, e.target);
    // TODO(original): also ends when `pTargetChar->Dead`.
    let (Some(src), Some(tgt)) = (source.filter(|a| a.live), target.filter(|a| a.live)) else {
        e.life_time = 0.0;
        e.live = false;
        return;
    };

    if (e.sub_type == 1 || e.sub_type == 2) && e.owner == e.target {
        return;
    }

    // vAngle keeps its last value for the bone sparks below (uninitialized for SubType 1/2).
    let mut angle = Vec3::ZERO;
    let me = e.target;
    if e.sub_type == 0 {
        let light = Vec3::new(0.4, 0.4, 1.0);
        let p = src.bone_world(37, Vec3::ZERO);
        angle = Vec3::new(-60.0, 0.0, src.angle.z);
        joint_lit(ctx, BITMAP_JOINT_THUNDER, p, tgt.position, angle, 0, me, 50.0, -1, 0, light);
        joint_lit(ctx, BITMAP_JOINT_THUNDER, p, tgt.position, angle, 0, me, 10.0, -1, 0, light);
        angle = Vec3::new(0.0, 0.0, src.angle.z + 60.0);
        joint_lit(ctx, BITMAP_JOINT_THUNDER, p, tgt.position, angle, 0, me, 50.0, -1, 0, light);
        joint_lit(ctx, BITMAP_JOINT_THUNDER, p, tgt.position, angle, 0, me, 10.0, -1, 0, light);

        let p = src.bone_world(28, Vec3::ZERO);
        angle = Vec3::new(-60.0, 0.0, src.angle.z);
        joint_lit(ctx, BITMAP_JOINT_THUNDER, p, tgt.position, angle, 0, me, 50.0, -1, 0, light);
        joint_lit(ctx, BITMAP_JOINT_THUNDER, p, tgt.position, angle, 0, me, 10.0, -1, 0, light);
        angle = Vec3::new(0.0, 0.0, src.angle.z - 60.0);
        joint_lit(ctx, BITMAP_JOINT_THUNDER, p, tgt.position, angle, 0, me, 50.0, -1, 0, light);
        joint_lit(ctx, BITMAP_JOINT_THUNDER, p, tgt.position, angle, 0, me, 10.0, -1, 0, light);
    } else {
        let mut p = src.position;
        p.z += 80.0;
        // TODO(original): vLight (the trails' vPriorColor) is uninitialized for SubType 1/2;
        // passed here as no prior color.
        ctx.create_joint_ex(BITMAP_JOINT_THUNDER, p, tgt.position, tgt.angle, 0, me, 50.0, -1, 0);
        ctx.create_joint_ex(BITMAP_JOINT_THUNDER, p, tgt.position, tgt.angle, 0, me, 10.0, -1, 0);
    }

    if life(e) == 15 {
        let light = Vec3::new(0.2, 0.2, 0.8);
        for i in 0..tgt.bones.len() {
            let p = tgt.bone_world(i, Vec3::ZERO);
            let scale = 3.0 + (ctx.rand() % 20 - 10) as f32 * 0.1;
            ctx.create_particle(BITMAP_LIGHT, p, angle, light, 5, scale, None);
        }
    }
}

// --- MODEL_ALICE_DRAIN_LIFE ------------------------------------------------------------------------------------

/// ZzzEffect.cpp:1004.
fn drain_create(e: &mut Effect, _ctx: &mut Ctx) {
    if e.sub_type == 0 {
        e.life_time = 70.0;
    }
}

/// ZzzEffect.cpp:9122: red sparks on both, ghosts flying from the caster to the victim and energy
/// trails back. The victim is `e.target`.
/// TODO(original): the original reads it as `o->Owner->Owner` (the caster's `Owner`, set by
/// ZzzCharacter.cpp:5064); whoever creates the effect must set `target`.
fn drain_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    if let (Some(src), Some(tgt)) = (actor_of(ctx, e.owner).filter(|a| a.live), actor_of(ctx, e.target).filter(|a| a.live)) {
        drain_move(e, ctx, &src, &tgt);
    }
    move_tail(e);
}

fn drain_move(e: &Effect, ctx: &mut Ctx, src: &Actor, tgt: &Actor) {
    let lt = life(e);
    let cnt = match ctx.rand() % 10 {
        0 => 0,
        1..=7 => 1,
        _ => 2,
    };
    let red = Vec3::new(1.0, 0.2, 0.2);
    for _ in 0..cnt {
        let mut p = src.position;
        p.x += (ctx.rand() % 80 - 40) as f32;
        p.y += (ctx.rand() % 60 - 30) as f32;
        p.z += 80.0 + (ctx.rand() % 180 - 100) as f32;
        ctx.create_particle(BITMAP_LIGHT + 2, p, src.angle, red, 7, 1.8, None);
    }

    if lt <= 60 {
        for _ in 0..cnt {
            let mut p = tgt.position;
            p.x += (ctx.rand() % 80 - 40) as f32;
            p.y += (ctx.rand() % 60 - 30) as f32;
            p.z += 80.0 + (ctx.rand() % 180 - 100) as f32;
            ctx.create_particle(BITMAP_LIGHT + 2, p, tgt.angle, red, 7, 1.8, None);
        }
    }

    if (66..=70).contains(&lt) {
        // (cases 4 and 5 fall through to 3)
        let cnt2 = match ctx.rand() % 10 {
            0 => 0,
            1..=3 => 1,
            4..=8 => 3,
            _ => 4,
        };
        for _ in 0..cnt2 {
            let mut sp = src.bone_world(18, Vec3::ZERO);
            let dir = rotate(src.angle, Vec3::Y).normalize_or_zero();
            sp.x = sp.x + dir.x * 100.0 + ((ctx.rand() % 10) * 5) as f32;
            sp.y = sp.y + dir.y * 100.0 + ((ctx.rand() % 10) * 5) as f32;
            sp.z += ((ctx.rand() % 10) * 5) as f32;

            let mut tp = tgt.position;
            tp.z += 100.0 + ((ctx.rand() % 10 - 5) * 4) as f32;

            joint_lit(ctx, BITMAP_DRAIN_LIFE_GHOST, sp, tp, e.angle, 0, e.owner, 40.0, 0, 0, Vec3::new(0.8, 0.1, 0.2));
        }
    }

    if lt == 64 {
        // (vSourcePos from bone 18 is computed and then overwritten)
        for i in 0..tgt.bones.len() {
            let tp = tgt.bone_world(i, Vec3::ZERO);
            if ctx.rand() % 2 == 0 {
                let mut sp = src.position;
                sp.z += 80.0;
                joint_lit(ctx, BITMAP_JOINT_ENERGY, tp, sp, tgt.angle, 45, e.owner, 10.0, -1, 0, Vec3::new(1.0, 0.0, 0.1));
            }
        }
    }
}

// --- MODEL_ALICE_BUFFSKILL_EFFECT / MODEL_ALICE_BUFFSKILL_EFFECT2 -----------------------------------------------

/// ZzzEffect.cpp:1012.
fn alice_buff_create(e: &mut Effect, _ctx: &mut Ctx) {
    if (0..=2).contains(&e.sub_type) {
        e.life_time = 34.0;
        e.position.z += 100.0;
        e.alpha = 0.0;
        e.blend_mesh_light = 0.0;
        e.angle.z = 0.0;
        e.scale = 0.1;
    } else if e.sub_type == 3 || e.sub_type == 4 {
        e.life_time = 100.0;
        if e.sub_type == 3 {
            e.scale = 1.5;
        } else {
            e.scale = 1.0;
        }
    }
}

/// ZzzEffect.cpp:1042.
fn alice_buff2_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 35.0;
    e.position.z += 100.0;
    e.alpha = 0.0;
    e.blend_mesh_light = 0.0;
    e.angle.z = 0.0;
    e.scale = 0.15;
}

/// ZzzEffect.cpp:9265.
fn alice_buff_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    if (0..=2).contains(&e.sub_type) {
        alice_ring_move(e, ctx);
    } else if e.sub_type == 3 || e.sub_type == 4 {
        alice_debuff_move(e, ctx);
    }
    move_tail(e);
}

/// SubTypes 0-2: the spinning ring over the target (ZzzEffect.cpp:9268-9388).
fn alice_ring_move(e: &mut Effect, ctx: &mut Ctx) {
    match ctx.actor(e.owner).map(|a| (a.live, a.position)) {
        Some((live, p)) => {
            if !live {
                e.live = false;
            }
            e.position = p;
            e.position.z += 100.0;
        }
        // (NULL owner: the original crashes right after.)
        None => e.live = false,
    }
    if life(e) > 20 {
        e.alpha += 0.05;
        e.blend_mesh_light += 0.05;
    } else {
        e.alpha -= 0.05;
        e.blend_mesh_light -= 0.05;
        if e.alpha < 0.0 {
            e.live = false;
        }
    }

    if e.ty == MODEL_ALICE_BUFFSKILL_EFFECT {
        e.angle.z += 8.0;
    } else if e.ty == MODEL_ALICE_BUFFSKILL_EFFECT2 {
        e.angle.z -= 8.0;
    }
    e.scale += 0.035;

    let rot = (ctx.world_time() * 0.0006) * 360.0;
    let a = e.alpha;

    // flare01
    let light = match e.sub_type {
        0 => Vec3::new(0.8 * a, 0.1 * a, 0.9 * a),
        1 => Vec3::splat(a),
        _ => Vec3::new(0.8 * a, 0.5 * a, 0.2 * a),
    };
    let sub = if e.sub_type == 1 { 1 } else { 0 };
    ctx.create_sprite(BITMAP_LIGHT, e.position, 5.0, light, 0.0, sub);
    ctx.create_sprite(BITMAP_LIGHT, e.position, 5.0, light, 0.0, sub);

    // shiny04
    let light = match e.sub_type {
        0 => Vec3::new(0.7 * a, 0.6 * a, 0.9 * a),
        1 => Vec3::splat(a),
        _ => Vec3::new(0.8 * a, 0.5 * a, 0.2 * a),
    };
    ctx.create_sprite(BITMAP_SHINY + 5, e.position, 2.0, light, rot, sub);
    ctx.create_sprite(BITMAP_SHINY + 5, e.position, 1.0, light, -rot, sub);

    let p = Vec3::new(0.0, -200.0, 0.0);
    for _ in 0..3 {
        let angle = Vec3::new(rf(ctx, 90), 0.0, rf(ctx, 360));
        let world = e.position - rotate(angle, p);
        let (light, jsub) = match e.sub_type {
            0 => (Vec3::new(0.7, 0.5, 0.7), 15),
            1 => (Vec3::ONE, 16),
            _ => (Vec3::new(0.8, 0.5, 0.2), 15),
        };
        joint_lit(ctx, BITMAP_JOINT_HEALING, world, e.position, angle, jsub, Some(Target::Effect(e.id)), 5.0, 0, 0, light);
    }
}

/// SubTypes 3 (blind) and 4 (sleep): sparks on random bones of the target
/// (ZzzEffect.cpp:9389-9431).
fn alice_debuff_move(e: &mut Effect, ctx: &mut Ctx) {
    let Some(a) = actor_of(ctx, e.owner).filter(|a| a.live) else {
        e.live = false;
        return;
    };
    e.life_time = 100.0;
    let num_bones = a.bones.len() as i32;
    if num_bones == 0 {
        return;
    }
    let bone = ctx.rand() % num_bones;
    // TODO(original): skipped when `pModel->Bones[iBone].Dummy` (not known here).
    let mut p = a.bone_world(bone as usize, Vec3::ZERO);
    if e.sub_type == 3 {
        ctx.create_particle(BITMAP_LIGHT + 2, p, e.angle, e.light, 6, e.scale, None);
        let bone = ctx.rand() % num_bones;
        let p = a.bone_world(bone as usize, Vec3::ZERO);
        ctx.create_particle(BITMAP_LIGHT + 2, p, e.angle, e.light, 6, e.scale, None);
    } else if e.sub_type == 4 && ctx.rand() % 2 == 0 {
        p.z -= 20.0;
        ctx.create_particle(BITMAP_TWINTAIL_WATER, p, e.angle, e.light, 2, 1.0, None);
    }
}

/// RenderEffects 18708 (only SubTypes 0-2) + Draw_RenderObject (ZzzObject.cpp:612):
/// `RenderBody(RENDER_TEXTURE|RENDER_BRIGHT, Alpha, BlendMesh 0, BlendMeshLight, U, V)`
/// (`RENDER_DARK` for SubType 1).
/// TODO(original): SubType 1 is subtractive (RENDER_DARK → `EnableAlphaBlendMinus` on the blend
/// mesh), drawn additive here; meshes other than mesh 0 are drawn normally instead of bright.
fn alice_buff_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    if !(0..=2).contains(&e.sub_type) {
        return;
    }
    let Some(mut m) = model(e) else { return };
    m.blend_mesh = 0;
    out.models.push(m);
}

// --- MODEL_LIGHTNING_SHOCK ---------------------------------------------------------------------------------------

/// ZzzEffect.cpp:1054.
fn shock_create(e: &mut Effect, ctx: &mut Ctx) {
    if e.sub_type == 0 {
        e.life_time = 20.0;
        e.position.z += 280.0;
        e.velocity = 0.0;
        e.gravity = 1.0;
    } else if e.sub_type == 1 {
        e.life_time = 12.0;
        ctx.create_effect(BITMAP_DAMAGE_01_MONO, e.position, e.angle, Vec3::new(1.0, 0.8, 0.5), 1, None, -1, 0, 0, 0.0);
        ctx.create_effect(BITMAP_DAMAGE_01_MONO, e.position, e.angle, Vec3::new(1.0, 0.0, 0.0), 1, None, -1, 0, 0, 0.0);

        // magic_ground
        let light = Vec3::new(1.0, 0.2, 0.05);
        for i in 0..5 {
            let angle = Vec3::new(0.0, 0.0, e.angle.z + i as f32 * 72.0);
            let p = rotate(angle, Vec3::new(0.0, 150.0, 0.0)) + e.position;
            ctx.create_effect(BITMAP_MAGIC, p, e.angle, light, 12, None, -1, 0, 0, 0.0);
        }

        let light = Vec3::new(1.0, 0.4, 0.2);
        let mut p = e.position;
        p.z = ctx.terrain_height(p.x, p.y) + 10.0;
        let me = Some(Target::Effect(e.id));
        for _ in 0..3 {
            let scale = (ctx.rand() % 4) as f32 * 0.1 + 1.0;
            ctx.create_effect(MODEL_KNIGHT_PLANCRACK_A, p, e.angle, light, 1, me, 0, 0, 0, scale);
        }
    } else if e.sub_type == 2 {
        e.life_time = 15.0;
        let light = Vec3::new(1.0, 0.4, 0.2);
        // (vPosition at the ground is computed but the cracks use o->Position)
        let me = Some(Target::Effect(e.id));
        for _ in 0..3 {
            let scale = (ctx.rand() % 4) as f32 * 0.1 + 0.5;
            ctx.create_effect(MODEL_KNIGHT_PLANCRACK_A, e.position, e.angle, light, 1, me, 0, 0, 0, scale);
        }
    }
}

/// `BITMAP_LIGHTNING_MEGA1 + rand() % 3` sparks in a 70-unit cube around the position
/// (ZzzEffect.cpp:9474-9482 and 9510-9516); `flare` adds the `BITMAP_LIGHT` sprite of SubType 0.
fn shock_sparks(e: &Effect, ctx: &mut Ctx, flare: bool) {
    let light = Vec3::new(1.0, 0.7, 0.4);
    let light_flare = Vec3::new(1.0, 0.2, 0.1);
    for _ in 0..11 {
        let scale = (ctx.rand() % 80 + 32) as f32 * 0.01 * 1.0;
        let x = e.position.x + (ctx.rand() % 70 - 35) as f32 * 1.0;
        let y = e.position.y + (ctx.rand() % 70 - 35) as f32 * 1.0;
        let z = e.position.z + (ctx.rand() % 70 - 35) as f32 * 1.0;
        let p = Vec3::new(x, y, z);
        if flare {
            ctx.create_sprite(BITMAP_LIGHT, p, 2.2, light_flare, 0.0, 0);
            if ctx.rand() % 3 > 0 {
                continue;
            }
        }
        let ty = BITMAP_LIGHTNING_MEGA1 + ctx.rand() % 3;
        ctx.create_particle(ty, p, e.angle, light, 0, scale, None);
    }
}

/// ZzzEffect.cpp:9435.
fn shock_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    match e.sub_type {
        0 => shock_fall(e, ctx),
        1 => shock_impact(e, ctx),
        2 => shock_burn(e, ctx),
        _ => {}
    }
    move_tail(e);
}

/// SubType 0: the red orb falls from above the caster (ZzzEffect.cpp:9439-9501).
fn shock_fall(e: &mut Effect, ctx: &mut Ctx) {
    let owner = actor_of(ctx, e.owner);
    // (A missing owner would crash the original; here only the LifeTime condition counts.)
    if owner.as_ref().is_some_and(|a| a.frame > 6.0) || life(e) < 15 {
        e.direction = Vec3::new(0.0, -20.0, -75.0 - e.velocity);
        e.gravity += 0.1;
        e.velocity += e.gravity;
    }

    let rot = (ctx.world_time() * 0.0006) * 360.0;
    let fs = 0.8;
    // shiny
    let light = Vec3::new(1.0, 0.4, 0.4);
    ctx.create_sprite(BITMAP_SHINY + 1, e.position, 4.0 * fs, light, rot, 0);
    ctx.create_sprite(BITMAP_SHINY + 1, e.position, 3.0 * fs, light, -rot, 0);
    // magic_ground
    let light = Vec3::new(1.0, 0.2, 0.2);
    ctx.create_sprite(BITMAP_MAGIC, e.position, 1.0 * fs, light, rot, 0);
    ctx.create_sprite(BITMAP_MAGIC, e.position, 0.5 * fs, light, -rot, 0);
    // pin_light
    let light = Vec3::new(1.0, 0.4, 0.4);
    let r = rf(ctx, 360);
    ctx.create_sprite(BITMAP_PIN_LIGHT, e.position, 2.0 * fs, light, r, 0);
    let r = rf(ctx, 360);
    ctx.create_sprite(BITMAP_PIN_LIGHT, e.position, 2.0 * fs, light, r, 0);

    ctx.create_particle(BITMAP_MAGIC, e.position, e.angle, Vec3::new(1.0, 0.3, 0.3), 0, 1.0 * fs, None);

    shock_sparks(e, ctx, true);

    let light = Vec3::new(1.0, 0.5, 0.4);
    for _ in 0..2 {
        let scale = (ctx.rand() % 60 + 22) as f32 * 0.01 * 1.0;
        let bone = (ctx.rand() % 41) as usize;
        let rx = (ctx.rand() % 30 - 15) as f32 * 1.0;
        let ry = (ctx.rand() % 30 - 15) as f32 * 1.0;
        let rz = (ctx.rand() % 30 - 15) as f32 * 1.0;
        let rel = Vec3::new(rx, ry, rz);
        let p = match &owner {
            Some(a) => a.bone_world(bone, rel),
            None => e.position + rel,
        };
        let ty = BITMAP_LIGHTNING_MEGA1 + ctx.rand() % 3;
        ctx.create_particle(ty, p, e.angle, light, 0, scale, None);
    }

    let height = ctx.terrain_height(e.position.x, e.position.y);
    if e.position.z < height {
        ctx.create_effect(MODEL_LIGHTNING_SHOCK, e.position, e.angle, e.light, 1, Some(Target::Effect(e.id)), -1, 0, 0, 0.0);
        // EffectDestructor(o)
        e.live = false;
    }
}

/// SubType 1: the impact on the ground (ZzzEffect.cpp:9502-9552).
fn shock_impact(e: &mut Effect, ctx: &mut Ctx) {
    shock_sparks(e, ctx, false);

    let light = Vec3::new(1.0, 0.0, 0.0);
    for _ in 0..6 {
        let scale = (ctx.rand() % 60 + 22) as f32 * 0.01 * 1.0;
        let dir = Vec3::new(0.0, (ctx.rand() % 400) as f32, 0.0);
        let angle = Vec3::new(0.0, 0.0, e.angle.z + (ctx.rand() % 360) as f32);
        let mut p = rotate(angle, dir) + e.position;
        p.z = ctx.terrain_height(p.x, p.y) + 20.0;
        let ty = BITMAP_LIGHTNING_MEGA1 + ctx.rand() % 3;
        ctx.create_particle(ty, p, e.angle, light, 0, scale, None);
    }

    let mut p = e.position;
    p.z = ctx.terrain_height(p.x, p.y) + 10.0;
    for _ in 0..2 {
        ctx.create_particle(BITMAP_SMOKE, p, e.angle, light, 58, 1.0, None);
    }
    if ctx.rand() % 2 == 0 {
        ctx.create_particle(BITMAP_SMOKE, p, e.angle, light, 54, 2.8, None);
    }
    let ty = MODEL_STONE1 + ctx.rand() % 2;
    ctx.create_effect(ty, p, e.angle, light, 13, Some(Target::Effect(e.id)), -1, 0, 0, 0.0);
}

/// SubType 2: the burning debuff on the victim's bones (ZzzEffect.cpp:9553-9591).
fn shock_burn(e: &mut Effect, ctx: &mut Ctx) {
    let Some(a) = actor_of(ctx, e.owner) else { return };
    // vAngle is uninitialized in the original.
    let angle = Vec3::ZERO;
    for i in 0..a.bones.len() {
        let p = a.bone_world(i, Vec3::ZERO);
        if ctx.rand() % 60 == 0 {
            let scale = 5.0 + (ctx.rand() % 20 - 10) as f32 * 0.1;
            ctx.create_particle(BITMAP_LIGHT, p, angle, Vec3::new(0.8, 0.0, 0.0), 5, scale, None);
        }
        if ctx.rand() % 5 == 0 {
            let scale = (ctx.rand() % 70 + 22) as f32 * 0.01 * 1.0;
            let ty = BITMAP_LIGHTNING_MEGA1 + ctx.rand() % 3;
            ctx.create_particle(ty, p, angle, Vec3::new(1.0, 0.8, 0.4), 0, scale, None);
        }
    }
    let light = Vec3::new(1.0, 0.0, 0.0);
    ctx.create_particle(BITMAP_SMOKE, e.position, e.angle, light, 58, 1.0, None);
    if ctx.rand() % 4 == 0 {
        ctx.create_particle(BITMAP_SMOKE, e.position, e.angle, light, 54, 2.8, None);
    }
}

// --- MODEL_SWELL_OF_MAGICPOWER --------------------------------------------------------------------------------------

/// ZzzEffect.cpp:796.
fn swell_create(e: &mut Effect, _ctx: &mut Ctx) {
    if e.sub_type == 0 {
        e.life_time = 45.0;
    }
}

/// ZzzEffect.cpp:15395 (+ the `BlendMeshLight` fade of Draw_RenderObject, ZzzObject.cpp:1918).
fn swell_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    if e.sub_type == 0
        && let Some(a) = actor_of(ctx, e.owner).filter(|a| a.ty == MODEL_PLAYER)
    {
        swell_move(e, ctx, &a);
    }
    // Draw_RenderObject (ZzzObject.cpp:1918) does `o->BlendMeshLight *= 0.86f` each rendered frame
    // while `LifeTime <= 20`: moved here, with the LifeTime the render sees (after this tick's
    // decrement).
    if e.life_time - 1.0 <= 20.0 {
        e.blend_mesh_light *= 0.86;
    }
    move_tail(e);
}

fn swell_move(e: &mut Effect, ctx: &mut Ctx, a: &Actor) {
    let lt = life(e);
    e.position = a.position;
    let me = Some(Target::Effect(e.id));

    let light = Vec3::new(0.4, 0.3, 0.9);
    if lt == 45 || lt == 35 || lt == 25 {
        ctx.create_effect(BITMAP_SHOCK_WAVE, e.position, e.angle, light, 14, me, -1, 0, 0, 5.0);
        ctx.create_effect(BITMAP_SHOCK_WAVE, e.position, e.angle, light, 14, me, -1, 0, 0, 5.0);
        ctx.create_effect(BITMAP_TWLIGHT, e.position, e.angle, light, 3, me, -1, 0, 0, 6.0);
    }

    let light = Vec3::new(0.7, 0.3, 0.9);
    if lt <= 20 {
        let dl = light * (lt as f32 * 0.05);
        for i in 0..a.bones.len() {
            let p = a.bone_world(i, Vec3::ZERO);
            ctx.create_sprite(BITMAP_LIGHT, p, 1.5, dl, 0.0, 0);
        }
    }

    let light = Vec3::new(0.2, 0.2, 0.9);
    if lt == 45 {
        // CreateEffect(MODEL_ARROWSRE06, vPos, Angle, Light, 1, o->Owner, PKKey = bone).
        let p = a.bone_world(28, Vec3::ZERO);
        ctx.create_effect(MODEL_ARROWSRE06, p, e.angle, light, 1, e.owner, 28, 0, 0, 0.0);
        let p = a.bone_world(37, Vec3::ZERO);
        ctx.create_effect(MODEL_ARROWSRE06, p, e.angle, light, 1, e.owner, 37, 0, 0, 0.0);
    }

    if lt >= 30 {
        let light = Vec3::new(0.3, 0.2, 0.9);
        for _ in 0..2 {
            let scale = 20.0 + rf(ctx, 10);
            joint_lit(ctx, BITMAP_2LINE_GHOST, e.position, e.position, e.angle, 1, me, scale, -1, 0, light);
        }
    }
}

/// RenderEffects default (`RenderObject`) + Draw_RenderObject (ZzzObject.cpp:1913):
/// `BlendMesh = 0`, `BodyLight = (0.7, 0.4, 0.9)`, `RenderBody(RENDER_TEXTURE, ...)`.
fn swell_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    let Some(mut m) = model(e) else { return };
    m.blend_mesh = 0;
    m.light = Vec3::new(0.7, 0.4, 0.9);
    out.models.push(m);
}

// --- MODEL_SWELL_OF_MAGICPOWER_BUFF_EFF ------------------------------------------------------------------------------

/// ZzzEffect.cpp:819.
fn swell_buff_create(e: &mut Effect, ctx: &mut Ctx) {
    if e.sub_type == 0 {
        e.life_time = 999.0;
        e.timer = ctx.world_time();
    }
}

/// ZzzEffect.cpp:15537: every 6 s, a flash on both hands.
fn swell_buff_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    if e.sub_type == 0 {
        // (A missing owner would crash the original; here the effect stays where it is.)
        if let Some(p) = ctx.actor(e.owner).map(|a| a.position) {
            e.position = p;
        }
        let past = (ctx.world_time() - e.timer) as i32;
        if past >= 6000 {
            let light = Vec3::new(0.2, 0.2, 0.9);
            // Bip01 R Hand / Bip01 L Hand (PKKey = bone).
            ctx.create_effect(MODEL_ARROWSRE06, e.position, e.angle, light, 1, e.owner, 28, 0, 0, 0.0);
            ctx.create_effect(MODEL_ARROWSRE06, e.position, e.angle, light, 1, e.owner, 37, 0, 0, 0.0);
            e.timer = ctx.world_time();
        }
        e.life_time = 999.0;
    }
    move_tail(e);
}

/// RenderEffects 18853: a pulsing purple light on every bone of the owner.
fn swell_buff_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    if e.sub_type != 0 {
        return;
    }
    let Some(a) = owner_actor(e.owner, ctx.actors) else { return };
    if a.ty != MODEL_PLAYER {
        return;
    }
    let lumi = ((ctx.world_time * 0.001).sin().abs() + 0.2) * 0.5;
    let light = Vec3::new(lumi * 0.7, lumi * 0.3, lumi * 0.9);
    for i in 0..a.bones.len() {
        let p = a.bone_world(i, Vec3::ZERO);
        render_sprite(out, ctx, BITMAP_LIGHT, p, 1.8, light);
    }
}
