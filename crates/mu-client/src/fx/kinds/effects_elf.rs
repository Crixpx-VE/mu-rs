//! Elf / bow effects (`Effects[]`): the arrows (`MODEL_ARROW` and the types that share its code:
//! `MODEL_ARROW_STEEL/THUNDER/LASER/V/SAW/NATURE/WING`, `MODEL_LACEARROW`, `MODEL_ARROW_SPARK/RING/
//! TANKER/DARKSTINGER/GAMBLE`, `MODEL_ARROW_BOMB`), Multi-Shot (`MODEL_MULTI_SHOT1..3`), Infinity
//! Arrow (`MODEL_ARROW_AUTOLOAD`, `MODEL_INFINITY_ARROW`, `MODEL_INFINITY_ARROW1..4`,
//! `MODEL_ARROWSRE06`), Penetration (`MODEL_PIERCING`, `MODEL_PIERCING+1`, `MODEL_PIERCING2`), the
//! crossbow/holy/double/drill/impact arrows (`MODEL_ARROW_BEST_CROSSBOW`, `MODEL_ARROW_HOLY`,
//! `MODEL_ARROW_DOUBLE`, `MODEL_ARROW_DRILL`, `MODEL_ARROW_IMPACT`, `MODEL_ARROW_TANKER_HIT`), the
//! monster arrows (`MODEL_IRON_RIDER_ARROW`, `MODEL_KENTAUROS_ARROW`), `MODEL_SUMMON`,
//! `MODEL_DARK_ELF_SKILL`, `MODEL_MAGIC1`, `MODEL_MAGIC2` and `MODEL_SAW`.
//!
//! Translated from Main 5.2 `ZzzEffect.cpp` (`CreateEffect`, `MoveEffect` and its common tail
//! 18060-18124, `CheckClientArrow` 6758, `RenderEffects`) and the type-specific branches of
//! `Draw_RenderObject` (`ZzzObject.cpp` 399-2680), which is what `RenderObject(o)` runs for them.
//!
//! Notes:
//! - Client-side hits: [`check_client_arrow`] is `CheckClientArrow` (ZzzEffect.cpp:6758) in full
//!   (stop / explode / pierce on a character, the hero's `AttackCharacterRange`), except the Castle
//!   Siege no-attack zone. `CheckTargetRange` (ZzzEffect.cpp:366) lists the arrow types only in a
//!   `break` case, and its only call for them (ZzzEffect.cpp:1929) is commented out: nothing to do.
//! - `Draw_RenderObject` of some types creates particles/sprites/trails while drawing. In the
//!   original that happens after `MoveEffect` of a frame and before the next one, so here it runs at
//!   the START of the next `step` (before `Luminosity`), with the state the draw saw. Mutations done
//!   *before* the draw (e.g. `MODEL_ARROWSRE06`'s `BlendMeshLight *= 0.8`) run at the END of `step`.
//! - Function arguments that call `rand()` are evaluated left to right (like the other kind files).

use bevy::math::Vec3;

use super::common::{self, luminosity, move_tail, rf};
use crate::fx::core::*;

/// `TW_NOGROUND` (`_define.h`).
const TW_NOGROUND: u16 = 0x0008;
/// `WD_39KANTURU_3RD` (`MapManager.h:33`).
const WD_39KANTURU_3RD: i32 = 39;
/// `AT_SKILL_PARALYZE` (Main 5.2 `SkillManager.h:43`; MuMain calls it `AT_SKILL_ICE_ARROW`).
const AT_SKILL_PARALYZE: i32 = 51;
/// `AT_SKILL_PIERCING` (Main 5.2 `SkillManager.h:44`; MuMain calls it `AT_SKILL_PENETRATION`).
const AT_SKILL_PIERCING: i32 = 52;
const _: () = assert!(AT_SKILL_PARALYZE == AT_SKILL_ICE_ARROW && AT_SKILL_PIERCING == AT_SKILL_PENETRATION);
/// `SOUND_ATTACK01+5` .. `SOUND_ATTACK01+8` (ZzzOpenData.cpp:4624-4627: `eMissileHit1..4.wav`), by
/// their names in the sound table.
const SOUND_MISSILE_HIT: [&str; 4] =
    ["SOUND_ATTACK_MISSILE_HIT1", "SOUND_ATTACK_MISSILE_HIT2", "SOUND_ATTACK_MISSILE_HIT3", "SOUND_ATTACK_MISSILE_HIT4"];
/// `user[]` slot of `o->AttackPoint[0]` (an `int`; 0 at creation, ZzzEffect.cpp:490). Slots 0..3
/// hold `EyeLeft`.
const ATTACK_POINT: usize = 3;

pub fn register(r: &mut Registry) {
    let k = |create: EffectCreate, step: EffectStep, draw: EffectDraw| EffectKind { create: Some(create), step: Some(step), draw: Some(draw) };

    r.effect(MODEL_ARROW_AUTOLOAD, k(autoload_create, autoload_step, autoload_draw));
    r.effect(MODEL_INFINITY_ARROW, k(infinity_create, infinity_step, common::draw_nothing));
    for ty in [MODEL_INFINITY_ARROW1, MODEL_INFINITY_ARROW2, MODEL_INFINITY_ARROW3] {
        r.effect(ty, k(infinity_n_create, infinity_n_step, draw_bright));
    }
    r.effect(MODEL_INFINITY_ARROW4, k(infinity4_create, infinity4_step, draw_bright));
    r.effect(MODEL_ARROWSRE06, k(arrowsre06_create, arrowsre06_step, draw_plain));

    r.effect(MODEL_PIERCING, k(piercing_create, piercing_step, piercing_draw));
    r.effect(MODEL_PIERCING + 1, k(piercing1_create, piercing1_step, draw_plain));
    r.effect(MODEL_PIERCING2, k(piercing2_create, piercing2_step, piercing2_draw));

    r.effect(MODEL_ARROW_BEST_CROSSBOW, k(crossbow_create, crossbow_step, draw_plain));
    r.effect(MODEL_ARROW_DOUBLE, k(crossbow_create, double_step, draw_plain));
    r.effect(MODEL_ARROW_HOLY, k(holy_create, holy_step, common::draw_nothing));
    r.effect(MODEL_ARROW_DRILL, k(drill_create, drill_step, draw_plain));
    r.effect(MODEL_ARROW_BOMB, k(bomb_create, bomb_step, draw_plain));
    r.effect(MODEL_ARROW_IMPACT, k(impact_create, impact_step, draw_plain));
    r.effect(MODEL_ARROW_TANKER_HIT, k(tanker_hit_create, tanker_hit_step, tanker_draw));

    for ty in [MODEL_MULTI_SHOT1, MODEL_MULTI_SHOT2, MODEL_MULTI_SHOT3] {
        r.effect(ty, k(arrow_create, multi_shot_step, draw_bright));
    }
    for ty in [
        MODEL_ARROW,
        MODEL_ARROW_STEEL,
        MODEL_ARROW_THUNDER,
        MODEL_ARROW_LASER,
        MODEL_ARROW_V,
        MODEL_ARROW_SAW,
        MODEL_ARROW_NATURE,
        MODEL_ARROW_WING,
        MODEL_LACEARROW,
        MODEL_ARROW_RING,
    ] {
        r.effect(ty, k(arrow_create, arrow_step, draw_plain));
    }
    r.effect(MODEL_ARROW_SPARK, k(arrow_create, arrow_step, spark_draw));
    r.effect(MODEL_ARROW_TANKER, k(arrow_create, arrow_step, tanker_draw));
    r.effect(MODEL_ARROW_DARKSTINGER, k(arrow_create, arrow_step, darkstinger_draw));
    r.effect(MODEL_ARROW_GAMBLE, k(arrow_create, arrow_step, gamble_draw));

    r.effect(MODEL_IRON_RIDER_ARROW, k(iron_rider_create, iron_rider_step, draw_bright));
    r.effect(MODEL_KENTAUROS_ARROW, k(kentauros_create, kentauros_step, draw_bright));
    r.effect(MODEL_SUMMON, k(summon_create, summon_step, summon_draw));
    r.effect(MODEL_DARK_ELF_SKILL, k(dark_elf_create, dark_elf_step, draw_plain));
    r.effect(MODEL_MAGIC1, k(magic1_create, magic1_step, draw_plain));
    r.effect(MODEL_MAGIC2, k(magic2_create, magic2_step, draw_plain));
    r.effect(MODEL_SAW, k(saw_create, saw_step, draw_plain));
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

/// `(int)WorldTime*0.1f`: sprite rotation of several arrows.
fn world_rot(ctx: &Ctx) -> f32 {
    (ctx.world_time() as i32) as f32 * 0.1
}

/// `o->EyeLeft` (also in `user[0..3]`, which this file reads); the trails that follow `o` read it
/// through `Effect::eye_left`.
fn set_eye_left(e: &mut Effect, p: Vec3) {
    e.eye_left = p;
    e.user[0] = p.x;
    e.user[1] = p.y;
    e.user[2] = p.z;
}

/// `(rand()%a - a/2, rand()%b - b/2, rand()%a - a/2)` offset around the effect (`Vector` macro:
/// left to right).
fn jitter(ctx: &mut Ctx, a: i32, b: i32) -> Vec3 {
    let x = (ctx.rand() % a - a / 2) as f32;
    let y = (ctx.rand() % b - b / 2) as f32;
    let z = (ctx.rand() % a - a / 2) as f32;
    Vec3::new(x, y, z)
}

/// The arrows' start offset (`Vector(-10,-60,135)` rotated by `Angle`, ZzzEffect.cpp:1826).
fn arrow_offset(e: &mut Effect) {
    e.position += rotate(e.angle, Vec3::new(-10.0, -60.0, 135.0));
}

/// `b->TransformPosition(BoneTransform[bone], p, out)` with the effect's OWN model bones as left by
/// `RenderObject`'s `Animation` (translated: world space), or `TransformByObjectBone(out, o, bone)`
/// after an untranslated `Animation` (the same point). Without the model file (or that bone) the
/// bone is taken as an identity bone at the model origin (`Position + Angle·(p·Scale)`); no
/// `rand()` depends on it.
fn own_bone(e: &Effect, ctx: &mut Ctx, bone: usize, p: Vec3) -> Vec3 {
    ctx.effect_bone_world(e, bone, p).unwrap_or_else(|| e.position + rotate(e.angle, p * e.scale))
}

/// `o->Owner == &Hero->Object` (a missing owner is never the hero).
fn owner_is_hero(e: &Effect, ctx: &Ctx) -> bool {
    e.owner.is_some() && e.owner == ctx.hero.map(Target::Actor)
}

/// `o->Type==MODEL_ARROW_BOMB || o->Type==MODEL_ARROW_HOLY` (the types `CheckClientArrow` explodes).
fn bomb_or_holy(e: &Effect) -> bool {
    e.ty == MODEL_ARROW_BOMB || e.ty == MODEL_ARROW_HOLY
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

/// `CheckClientArrow(o)` (ZzzEffect.cpp:6758-6911): the arrow stops (`Live = false`), explodes
/// (`CreateBomb`) or pierces (`BITMAP_FLARE` + the `AttackPoint[0]` countdown) when it reaches a
/// character. The rest of the type's `MoveEffect` (and the common tail) still runs after a hit,
/// like the original. (The `MODEL_DARK_SCREAM` `AttackCharacterRange` at its top, 6773-6779, is in
/// `effects_dl_mg1.rs`: no type of this file has it.)
fn check_client_arrow(e: &mut Effect, ctx: &mut Ctx) {
    // TODO(original): during Castle Siege (`battleCastle::IsBattleCastleStart()`), inside a
    // TW_NOATTACKZONE tile: Velocity = 0, Direction = 0, LifeTime /= 2 and return (6761-6771).
    // There is no Castle Siege state yet.
    if e.kind == 0 || e.sub_type == 99 {
        // Another character's arrow (or a `Skill == 2` one, SubType 99): it stops on whoever it
        // reaches (6781-6823). `CheckCharacterRange(o, 100.f, o->PKKey)` (Kind 0).
        if ctx.check_character_range(e.position, e.owner, 100.0, 0) {
            match ctx.hero_skill(e.skill) {
                AT_SKILL_PIERCING => {
                    if e.sub_type == 2 {
                        ctx.create_joint(BITMAP_FLARE, e.position, e.position, e.angle, 6, me(e), 30.0);
                    } else {
                        e.live = false;
                        common::create_bomb(ctx, e.position, true, 0);
                    }
                }
                AT_SKILL_PARALYZE => {
                    e.live = false;
                    if bomb_or_holy(e) {
                        common::create_bomb(ctx, e.position, true, 0);
                    }
                }
                _ => {
                    e.live = false;
                    // (Holy SubType 1 does not explode here.)
                    if bomb_or_holy(e) && e.sub_type != 1 {
                        common::create_bomb(ctx, e.position, true, 0);
                    }
                }
            }
        }
    } else if ctx.rand() % 2 == 0 && owner_is_hero(e, ctx) {
        // The hero's arrow: the client asks for the damage (6824-6905).
        let range = 100.0;
        let skill = ctx.hero_skill(e.skill);
        if skill == AT_SKILL_PIERCING && e.ty == MODEL_ARROW_BOMB {
            // The hero exists here (it owns the arrow).
            if let Some(h) = ctx.hero_actor() {
                let (dx, dy) = (e.position.x - h.position.x, e.position.y - h.position.y);
                let distance = (dx * dx + dy * dy).sqrt();
                if distance > 500.0 {
                    return;
                }
            }
        }
        let attack_point = e.user[ATTACK_POINT] as i32;
        // (`PJH_FIX_4_BUGFIX_7`, which adds AT_SKILL_DARK_SCREAM here, is not defined.)
        if skill == AT_SKILL_PIERCING && attack_point > 0 {
            // `range = 200.f` is set here but never read (the attack is the `else if`).
            e.user[ATTACK_POINT] = (attack_point - 1) as f32;
        } else if ctx.attack_character_range(e.skill, e.position, range, e.weapon, e.pk_key) {
            match skill {
                AT_SKILL_PIERCING => {
                    if e.sub_type == 2 {
                        let n = if (e.ty == MODEL_ARROW_HOLY && life(e) > 14) || e.ty == MODEL_ARROW_BOMB { 5 } else { 2 };
                        e.user[ATTACK_POINT] = n as f32;
                        ctx.create_joint(BITMAP_FLARE, e.position, e.position, e.angle, 6, me(e), 30.0);
                    } else {
                        e.live = false;
                        common::create_bomb(ctx, e.position, true, 0);
                    }
                }
                AT_SKILL_PARALYZE => {
                    e.live = false;
                    if bomb_or_holy(e) {
                        common::create_bomb(ctx, e.position, true, 0);
                    }
                }
                _ => {
                    e.live = false;
                    // PlayBuffer(SOUND_ATTACK01+5+rand()%4, o).
                    let n = ctx.rand() % 4;
                    ctx.play_sound_object(SOUND_MISSILE_HIT[n as usize]);
                    if bomb_or_holy(e) {
                        common::create_bomb(ctx, e.position, true, 0);
                    }
                }
            }
        }
    }
}

// --- Drawing helpers ------------------------------------------------------------------------------

/// `RENDER_TEXTURE|RENDER_BRIGHT`.
const TEXTURE_BRIGHT: u32 = RENDER_TEXTURE | RENDER_BRIGHT;

/// `RenderObject(o)` → `RenderBody(flags, ...)` with `o`'s state (`BodyLight` = `o->Light`); nothing
/// when `Alpha < 0.01` (`Calc_RenderObject`, ZzzObject.cpp:245). `flags` 0 = `RENDER_TEXTURE`.
fn render_object(e: &Effect, out: &mut DrawList, flags: u32) {
    if e.alpha < 0.01 {
        return;
    }
    let mut d = ModelDraw::of_effect(e);
    d.render_flags = flags;
    out.models.push(d);
}

/// `RenderObject(o)` with `RenderBody(RENDER_TEXTURE, ...)` (the `default:` of `RenderEffects` for
/// `MODEL_SKILL_BEGIN..MODEL_SKILL_END`, or an explicit `case`).
fn draw_plain(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    render_object(e, out, 0);
}

/// `RenderObject(o)` whose `Draw_RenderObject` branch does `VectorCopy(o->Light, b->BodyLight)` +
/// `RenderBody(RENDER_TEXTURE|RENDER_BRIGHT, ...)` (multi-shot ZzzObject.cpp:1526, infinity arrows
/// 1634, iron rider 1644, kentauros 1655).
fn draw_bright(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    render_object(e, out, TEXTURE_BRIGHT);
}

/// `MODEL_ARROW_SPARK` (ZzzObject.cpp:1545): `RenderBody(RENDER_TEXTURE|RENDER_CHROME5|RENDER_BRIGHT,
/// Alpha, BlendMesh, 1.f, ...)`.
fn spark_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    if e.alpha < 0.01 {
        return;
    }
    let mut d = ModelDraw::of_effect(e);
    d.render_flags = RENDER_TEXTURE | RENDER_CHROME5 | RENDER_BRIGHT;
    d.blend_mesh_light = 1.0;
    out.models.push(d);
}

/// `MODEL_ARROW_GAMBLE` (ZzzObject.cpp:1829): `RenderMesh(0, RENDER_TEXTURE|RENDER_BRIGHT, Alpha, 0,
/// Alpha, U, V, HiddenMesh)`. The last argument is `RenderMesh`'s `MeshTexture`; this type's
/// `HiddenMesh` is always -1 (no texture override).
fn gamble_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    if e.alpha < 0.01 {
        return;
    }
    let mut d = ModelDraw::of_effect(e);
    d.mesh = 0;
    d.render_flags = TEXTURE_BRIGHT;
    d.blend_mesh = 0;
    d.blend_mesh_light = e.alpha;
    d.hidden_mesh = -1;
    d.texture = e.hidden_mesh;
    out.models.push(d);
}

// --- MODEL_ARROW_AUTOLOAD ----------------------------------------------------------------------------

/// ZzzEffect.cpp:521.
fn autoload_create(e: &mut Effect, _ctx: &mut Ctx) {
    if e.sub_type == 0 {
        e.life_time = 40.0;
    } else if e.sub_type == 1 {
        e.life_time = 40.0;
        e.scale = 1.2;
        e.light = Vec3::new(1.0, 0.8, 0.2);
        e.direction = e.light;
    }
}

/// ZzzEffect.cpp:7002 (+ the particles `Draw_RenderObject` creates, ZzzObject.cpp:1606).
fn autoload_step(e: &mut Effect, ctx: &mut Ctx) {
    // Draw_RenderObject (ZzzObject.cpp:1614), only for SubType 1 (RenderEffects 18689).
    if e.sub_type == 1 && e.alpha >= 0.01 && life(e) > 15 {
        let light = Vec3::new(1.0, 0.8, 0.3);
        let p = own_bone(e, ctx, 1, Vec3::ZERO);
        ctx.create_particle(BITMAP_LIGHT + 1, p, e.angle, light, 5, 0.6, None);
        let p = own_bone(e, ctx, 3, Vec3::ZERO);
        ctx.create_particle(BITMAP_LIGHT + 1, p, e.angle, light, 5, 0.8, None);
    }

    let _ = luminosity(e, ctx);
    if e.sub_type == 0 {
        if life(e) % 10 == 0 {
            // `pOwn->Light` is overwritten by the SubType 1 create, so this effect's light is passed.
            if let Some(a) = ctx.actor(e.owner).map(|a| (a.position, a.angle)) {
                ctx.create_effect(MODEL_ARROW_AUTOLOAD, a.0, a.1, e.light, 1, e.owner, -1, 0, 0, 0.0);
            }
        }
    } else if e.sub_type == 1 {
        if life(e) < 20 {
            let flumi = (life(e) - 20) as f32 / 20.0;
            e.light = e.direction * flumi;
        }
        if let Some(a) = ctx.actor(e.owner) {
            e.position = a.bone_world(47, Vec3::new(-10.0, 5.0, 10.0));
            e.angle = a.angle + Vec3::new(85.0, -17.0, 20.0);
        }
    }
    move_tail(e, ctx);
}

/// RenderEffects 18689: only SubType 1. ZzzObject.cpp:1606: RENDER_TEXTURE|RENDER_BRIGHT.
fn autoload_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    if e.sub_type == 1 {
        render_object(e, out, TEXTURE_BRIGHT);
    }
}

// --- MODEL_INFINITY_ARROW ------------------------------------------------------------------------------

/// ZzzEffect.cpp:536.
fn infinity_create(e: &mut Effect, ctx: &mut Ctx) {
    if e.sub_type == 0 {
        e.life_time = 40.0;
        // The original dereferences Owner; without one nothing is created.
        if let Some(op) = ctx.target_position(e.owner) {
            ctx.create_effect(MODEL_INFINITY_ARROW, op, e.angle, e.light, 1, e.owner, -1, 0, 0, 0.0);
            ctx.create_effect(MODEL_INFINITY_ARROW1, op, e.angle, e.light, 0, e.owner, -1, 0, 0, 0.0);
            ctx.create_effect(MODEL_INFINITY_ARROW2, op, e.angle, e.light, 0, e.owner, -1, 0, 0, 0.0);
            ctx.create_effect(MODEL_INFINITY_ARROW3, op, e.angle, e.light, 0, e.owner, -1, 0, 0, 0.0);
            ctx.create_effect(MODEL_INFINITY_ARROW4, op, e.angle, e.light, 0, e.owner, -1, 0, 0, 0.0);
        }
    } else if e.sub_type == 1 {
        e.life_time = 40.0;
        e.scale = 1.0;
        e.light = Vec3::ONE;
        e.direction = e.light;
    }
}

/// ZzzEffect.cpp:7032 (+ the trails `Draw_RenderObject` creates, ZzzObject.cpp:1622).
fn infinity_step(e: &mut Effect, ctx: &mut Ctx) {
    // Draw_RenderObject (ZzzObject.cpp:1627): RenderEffects (18693) only renders SubType 1, and the
    // model itself is not drawn, only these trails from bones 1..9 (except 5) to the position.
    if e.sub_type == 1 && e.alpha >= 0.01 {
        for idx in 1..=9usize {
            if idx == 5 {
                continue;
            }
            let p2 = own_bone(e, ctx, idx, Vec3::ZERO);
            ctx.create_joint(BITMAP_FLARE + 1, p2, e.position, e.angle, 16, me(e), 20.0);
        }
    }

    let _ = luminosity(e, ctx);
    if e.sub_type == 1 {
        e.light *= 0.95;
    }
    if let Some(a) = ctx.actor(e.owner) {
        e.position = a.bone_world(29, Vec3::ZERO);
        e.angle = a.angle;
    }
    move_tail(e, ctx);
}

// --- MODEL_INFINITY_ARROW1..3 ------------------------------------------------------------------------

/// ZzzEffect.cpp:562.
fn infinity_n_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 40.0;
    e.scale = 1.0;
    e.light = Vec3::ONE;
    e.direction = e.light;
    if matches!(e.sub_type, 1..=3) {
        e.life_time = 60.0;
        e.scale = 1.0;
    }
}

/// ZzzEffect.cpp:7049.
fn infinity_n_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    match e.sub_type {
        0 => e.light *= 0.95,
        1 => {
            e.light *= 0.98;
            if life(e) == 40 {
                ctx.create_effect(MODEL_INFINITY_ARROW3, e.position, e.angle, e.light, 2, me(e), -1, 0, 0, 0.0);
            } else if life(e) == 20 {
                ctx.create_effect(MODEL_INFINITY_ARROW3, e.position, e.angle, e.light, 3, me(e), -1, 0, 0, 0.0);
            }
        }
        2 | 3 => e.light *= 0.98,
        _ => {}
    }
    if (0..=3).contains(&e.sub_type)
        && let Some(p) = ctx.target_position(e.owner)
    {
        e.position = p;
    }
    move_tail(e, ctx);
}

// --- MODEL_INFINITY_ARROW4 -----------------------------------------------------------------------------

/// ZzzEffect.cpp:579.
fn infinity4_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 15.0;
    e.scale = 1.0;
    e.light = Vec3::new(1.0, 0.5, 0.3);
    e.direction = e.light;
}

/// ZzzEffect.cpp:7088.
fn infinity4_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    e.scale *= 1.2;
    e.light *= 0.9;
    if let Some(a) = ctx.actor(e.owner) {
        e.position = a.bone_world(29, Vec3::ZERO);
        e.angle = a.angle;
    }
    move_tail(e, ctx);
}

// --- MODEL_ARROWSRE06 ----------------------------------------------------------------------------------

/// ZzzEffect.cpp:803. `Draw_RenderObject` (ZzzObject.cpp:1926) sets `BlendMesh = 0` before every
/// render, so it is set here already.
fn arrowsre06_create(e: &mut Effect, _ctx: &mut Ctx) {
    match e.sub_type {
        0 => e.life_time = 30.0,
        1 => e.life_time = 40.0,
        // `VectorCopy(Position, o->Position)`: already so.
        2 => e.life_time = 30.0,
        _ => {}
    }
    e.blend_mesh = 0;
}

/// ZzzEffect.cpp:15447 (SubType 2 is `PJH_ADD_PANDA_CHANGERING`, active).
fn arrowsre06_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    let owner = ctx.actor(e.owner).filter(|a| a.ty == MODEL_PLAYER).cloned();
    if let Some(a) = owner {
        // TransformByObjectBone(vPos, Owner, PKKey).
        let bone = usize::try_from(e.pk_key).unwrap_or(usize::MAX);
        match e.sub_type {
            0 => {
                e.position = a.bone_world(bone, Vec3::ZERO);
                e.scale -= 1.0;
                if life(e) <= 15 {
                    e.scale += 0.5;
                }
            }
            1 => {
                e.position = a.bone_world(bone, Vec3::ZERO);
                if life(e) >= 15 {
                    e.scale *= 1.05;
                } else {
                    e.scale *= 0.95;
                }
                ctx.create_sprite(BITMAP_LIGHT, e.position, e.scale, e.light, 0.0, 0);
                ctx.create_sprite(BITMAP_LIGHT, e.position, e.scale * 0.8, e.light, 0.0, 0);
                if life(e) <= 10 {
                    e.alpha *= 0.95;
                }
            }
            2 => {
                if life(e) > 15 {
                    e.scale *= 0.9;
                } else if life(e) == 15 {
                    e.scale = 2.0;
                } else {
                    e.scale *= 0.65;
                }
                let rel = if e.pk_key == 0 { Vec3::new(13.0, 14.0, -10.0) } else { Vec3::new(13.0, 14.0, 10.0) };
                // TransformPosition(Owner->BoneTransform[20], rel, out) (not translated, not scaled)
                // + Owner->Position.
                let s = if a.scale != 0.0 { a.scale } else { 1.0 };
                e.position = a.position + (a.bone_world(20, rel) - a.position) / s;
                ctx.create_sprite(BITMAP_LIGHT, e.position, e.scale, e.light, 0.0, 0);
                ctx.create_sprite(BITMAP_LIGHT, e.position, e.scale * 0.8, e.light, 0.0, 0);
                if life(e) <= 10 {
                    e.alpha *= 0.95;
                }
            }
            _ => {}
        }
    }
    move_tail(e, ctx);
    // Draw_RenderObject (ZzzObject.cpp:1929), before the render that follows this tick (LifeTime
    // there is already decremented).
    if e.alpha >= 0.01 && life(e) - 1 > 0 && life(e) - 1 <= 10 {
        e.blend_mesh_light *= 0.8;
    }
}

// --- MODEL_PIERCING ----------------------------------------------------------------------------------

/// ZzzEffect.cpp:1668. The owner is the arrow that created it, at the same position.
fn piercing_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = 100.0;
    e.blend_mesh = 0;
    e.blend_mesh_light = 1.0;
    e.gravity = 0.0;
    match e.sub_type {
        0 => {
            e.scale = 12.0;
            // `VectorCopy(o->Owner->Position, o->StartPosition)`: created at the owner's position.
            e.start_position = e.position;
        }
        1 => {
            e.hidden_mesh = 0;
            e.scale = 24.0;
        }
        2 => {
            e.hidden_mesh = 0;
            e.scale = 12.0;
            e.life_time = 10.0;
        }
        3 => {
            e.scale = 10.0;
            e.light = Vec3::new(0.9, 0.4, 0.6);
            e.start_position = e.position;
        }
        _ => {}
    }
    for sub in 0..4 {
        ctx.create_joint_ex(BITMAP_FLARE + 1, e.position, e.position, e.angle, sub, me(e), e.scale, 30, e.sub_type);
    }
    ctx.play_sound("SOUND_FLASH");
    e.scale = 1.0;
}

/// ZzzEffect.cpp:11771.
fn piercing_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    if !owner_live(e, ctx) {
        // `LifeTime = -1; break;` (the common tail still runs).
        e.life_time = -1.0;
        move_tail(e, ctx);
        return;
    }
    e.gravity += 90.0;
    // `VectorCopy(o->Owner->Angle, o->Angle/HeadAngle)`: the owner is usually the arrow effect
    // (its angle and position as of the tick start).
    let owner_angle = match e.owner {
        Some(Target::Effect(id)) => ctx.effects.iter().find(|i| i.id == id).map(|i| i.angle),
        _ => ctx.actor(e.owner).map(|a| a.angle),
    };
    if let Some(a) = owner_angle {
        e.angle = a;
    }
    e.head_angle = e.angle;
    e.start_position = e.position;
    if let Some(p) = ctx.target_position(e.owner) {
        e.position = p;
    }
    if e.sub_type == 1 {
        ctx.create_joint_ex(BITMAP_JOINT_THUNDER, e.position, e.position, e.angle, 3, None, 20.0, 7, 0);
        let scale = (ctx.rand() % 8 + 8) as f32 * 0.2;
        let rot = rf(ctx, 360);
        ctx.create_sprite(BITMAP_SHINY + 1, e.position, scale, Vec3::ONE, rot, 0);
    }
    if common::in_hellas(ctx.world_active) {
        // TODO(original): if Owner->Owner == Hero: AddWaterWave((int)(x / TERRAIN_SCALE),
        // (int)(y / TERRAIN_SCALE), 2, -200) (ZzzEffect.cpp:11792-11800): the engine has no water
        // waves.
    }
    e.head_angle.y = e.gravity;
    move_tail(e, ctx);
}

/// RenderEffects 18580: nothing for SubType 3.
/// TODO(original): `HeadAngle` (spun by Gravity) is passed to `BMD::Animation`; not reproduced.
fn piercing_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    if e.sub_type != 3 {
        render_object(e, out, 0);
    }
}

// --- MODEL_PIERCING+1 ----------------------------------------------------------------------------------

/// ZzzEffect.cpp:1708.
fn piercing1_create(e: &mut Effect, ctx: &mut Ctx) {
    e.velocity = 1.0;
    e.life_time = 30.0;
    arrow_offset(e);
    e.scale = 0.8;
    e.direction.y = -70.0;
    ctx.create_effect(MODEL_PIERCING, e.position, e.angle, e.light, e.sub_type, me(e), -1, 0, 0, 0.0);
}

/// ZzzEffect.cpp:11807.
fn piercing1_step(e: &mut Effect, ctx: &mut Ctx) {
    let lum = luminosity(e, ctx);
    let light = Vec3::ONE;
    if ctx.rand() % 2 == 0 {
        ctx.create_particle(BITMAP_SMOKE, e.position, e.angle, light, 0, 1.0, None);
    }
    ctx.create_particle(BITMAP_FIRE, e.position, e.angle, light, 5, 1.0, None);
    ctx.create_particle(BITMAP_FIRE, e.position, e.angle, light, 5, 1.0, None);
    let light = Vec3::new(lum * 0.6, lum * 0.8, lum * 0.8);
    ctx.add_terrain_light(e.position.x, e.position.y, light, 2.0);
    move_tail(e, ctx);
}

// --- MODEL_PIERCING2 -----------------------------------------------------------------------------------

/// ZzzEffect.cpp:3474.
fn piercing2_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.scale = 2.0;
    if e.sub_type == 1 {
        e.life_time = 6.0;
    } else if e.sub_type == 2 {
        e.scale = 3.0;
        e.life_time = 10.0;
    } else {
        e.life_time = 10.0;
    }
    e.blend_mesh = -2;
    e.direction = Vec3::new(0.0, -60.0, 0.0);
    e.start_position = e.position;
    e.position.z += 130.0;
}

/// ZzzEffect.cpp:13055 (the rotated `Position` it computes is unused).
fn piercing2_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    e.direction.y += 12.0;
    if e.direction.y >= 0.0 {
        e.direction.y = 0.0;
    }
    let limit = if e.sub_type == 1 || e.sub_type == 2 { 1 } else { 5 };
    if life(e) > limit {
        e.blend_mesh_light /= 1.6;
        ctx.create_effect(MODEL_WAVES, e.position, e.angle, e.light, 2, None, life(e), 0, 0, 0.0);
    }
    move_tail(e, ctx);
}

/// RenderEffects 18575: nothing for SubTypes 1 and 2.
fn piercing2_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    if e.sub_type != 1 && e.sub_type != 2 {
        render_object(e, out, 0);
    }
}

// --- MODEL_ARROW_BEST_CROSSBOW / MODEL_ARROW_DOUBLE --------------------------------------------------

/// ZzzEffect.cpp:1722 (`MODEL_ARROW_DOUBLE` 1741 is identical).
fn crossbow_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = 30.0;
    e.blend_mesh = -2;
    e.scale = 1.0;
    e.position.z += 130.0;
    e.direction = Vec3::new(0.0, -70.0, 0.0);
    set_eye_left(e, e.position);
    ctx.create_joint(BITMAP_JOINT_ENERGY, e.position, e.position, e.angle, 5, me(e), 100.0);
    if e.sub_type != 0 {
        ctx.create_effect(MODEL_PIERCING, e.position, e.angle, e.light, 0, me(e), -1, 0, 0, 0.0);
        e.user[ATTACK_POINT] = 0.0;
        e.kind = 1;
    }
    e.weapon = ctx.packet_serial();
}

/// ZzzEffect.cpp:11819.
fn crossbow_step(e: &mut Effect, ctx: &mut Ctx) {
    let lum = luminosity(e, ctx);
    if life(e) > 25 {
        e.blend_mesh_light = (30 - life(e)) as f32 / 40.0;
    } else {
        e.blend_mesh_light = 1.0;
    }
    e.angle.y += rf(ctx, 60) + 30.0;
    let eye = e.position + rotate(e.angle, e.direction);
    set_eye_left(e, eye);
    let light = Vec3::new(lum, lum * 0.4, lum * 0.2);
    ctx.add_terrain_light(e.position.x, e.position.y, light, 2.0);
    check_client_arrow(e, ctx);
    move_tail(e, ctx);
}

/// ZzzEffect.cpp:11841.
fn double_step(e: &mut Effect, ctx: &mut Ctx) {
    let lum = luminosity(e, ctx);
    e.angle.y += 30.0;
    set_eye_left(e, e.position);
    let light = Vec3::new(lum * 0.2, lum * 0.4, lum);
    ctx.add_terrain_light(e.position.x, e.position.y, light, 2.0);
    check_client_arrow(e, ctx);
    move_tail(e, ctx);
}

// --- MODEL_ARROW_HOLY ----------------------------------------------------------------------------------

/// ZzzEffect.cpp:1760.
fn holy_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = 30.0;
    e.blend_mesh = -2;
    e.scale = 1.0;
    e.position.z += 130.0;
    e.direction = Vec3::new(0.0, -60.0, 0.0);
    e.start_position = e.position + rotate(e.angle, Vec3::new(-10.0, -100.0, 15.0));
    let angle = Vec3::new(0.0, 0.0, e.angle.z);
    let (n, sub) = if e.sub_type == 1 { (3, 25) } else { (4, 11) };
    for i in 0..n {
        if i == 1 {
            ctx.create_joint_ex(BITMAP_FLARE, e.start_position, e.start_position, angle, sub, me(e), 50.0, -1, 1);
        } else {
            ctx.create_joint(BITMAP_FLARE, e.start_position, e.start_position, angle, sub, me(e), 50.0);
        }
    }
    e.weapon = ctx.packet_serial();
}

/// ZzzEffect.cpp:11848; the common tail (18092) moves `StartPosition`, not `Position`. Its
/// `BITMAP_FLARE` 11/25 trails follow that `StartPosition` (`Ctx::target_start_position`).
fn holy_step(e: &mut Effect, ctx: &mut Ctx) {
    let lum = luminosity(e, ctx);
    if e.sub_type == 1 {
        e.angle.y += 60.0;
        if life(e) == 13 {
            ctx.create_effect(MODEL_PIERCING, e.position, e.angle, e.light, 3, me(e), -1, 0, 0, 0.0);
        }
        check_client_arrow(e, ctx);
        let light = Vec3::new(lum * 0.9, lum * 0.4, lum * 0.6);
        ctx.add_terrain_light(e.position.x, e.position.y, light, 2.0);
    } else {
        e.angle.y += 30.0;
        let light = Vec3::new(lum * 0.2, lum * 0.4, lum);
        ctx.add_terrain_light(e.start_position.x, e.start_position.y, light, 2.0);
        if e.sub_type != 0 {
            if life(e) == 13 {
                ctx.create_effect(MODEL_PIERCING, e.position, e.angle, e.light, 0, me(e), -1, 0, 0, 0.0);
            } else if life(e) == 30 {
                e.user[ATTACK_POINT] = 0.0;
                e.kind = 1;
            }
        }
    }
    common::play_animation(e, ctx);
    e.start_position += rotate(e.angle, e.direction);
}

// --- The shared arrow case (MODEL_ARROW ... MODEL_ARROW_GAMBLE, MODEL_MULTI_SHOT1..3) -------------------

/// ZzzEffect.cpp:1798-1964. (`MODEL_DARK_SCREAM`/`_FIRE` share this case but are not in this file.)
fn arrow_create(e: &mut Effect, ctx: &mut Ctx) {
    let ty = e.ty;
    if ty == MODEL_ARROW {
        e.blend_mesh = 1;
    } else if ty == MODEL_ARROW_NATURE {
        e.blend_mesh = -2;
    } else if matches!(ty, MODEL_ARROW_THUNDER | MODEL_ARROW_LASER | MODEL_ARROW_V | MODEL_ARROW_WING) {
        e.blend_mesh = 0;
    }
    e.velocity = 1.0;
    e.life_time = 30.0;
    arrow_offset(e);
    if ty == MODEL_ARROW_WING {
        e.scale = 1.8;
        e.direction.y = -50.0;
        e.gravity = -10.0;
    } else if ty == MODEL_ARROW_TANKER {
        e.angle.x -= 18.0;
        e.scale = 1.0;
        e.direction.y = -42.0;
        e.direction.z = 4.0;
    } else if ty == MODEL_MULTI_SHOT1 {
        e.life_time = 16.0;
        e.blend_mesh = -2;
        e.scale = 0.0;
    } else if ty == MODEL_MULTI_SHOT2 {
        e.life_time = 13.0;
        e.blend_mesh = -2;
        e.scale = 0.0;
    } else if ty == MODEL_MULTI_SHOT3 {
        e.life_time = 12.0;
        e.blend_mesh = -2;
        e.scale = 0.0;
    } else if ty == MODEL_ARROW_SPARK {
        e.scale = 1.0;
        e.direction.y = -70.0;
        ctx.create_joint_ex(BITMAP_FLARE + 1, e.position, e.position, e.angle, 14, me(e), 50.0, 40, 0);
    } else if ty == MODEL_ARROW_DARKSTINGER || ty == MODEL_ARROW_GAMBLE {
        e.scale = 0.8;
        e.direction.y = -70.0;
        e.life_time = 30.0;
    } else {
        e.scale = 0.8;
        e.direction.y = -70.0;
    }

    if e.sub_type == 2 {
        ctx.create_effect(MODEL_PIERCING, e.position, e.angle, e.light, 0, me(e), -1, 0, 0, 0.0);
        e.user[ATTACK_POINT] = 0.0;
        e.kind = 1;
    }
    if ty == MODEL_ARROW_NATURE && e.sub_type == 1 {
        ctx.create_joint_ex(BITMAP_FLARE + 1, e.position, e.position, e.angle, 13, me(e), 20.0, 40, 0);
    }

    if ty == MODEL_ARROW && (e.sub_type == 3 || e.sub_type == 4) {
        e.life_time = 40.0;
        e.scale = 1.5;
        e.gravity = (ctx.rand() % 100 + 50) as f32 / 15.0;
        if let Some(h) = ctx.hero_actor() {
            e.start_position = h.position;
        }
        if e.sub_type == 3 {
            e.direction.y = -(rf(ctx, 30) + 50.0);
            e.angle.x = -(rf(ctx, 20) + 45.0);
        } else {
            e.direction.y = -(rf(ctx, 15) + 55.0);
            e.angle.x = -10.0;
        }
    } else if ty == MODEL_LACEARROW {
        e.blend_mesh = 1;
        e.angle.y = rf(ctx, 360);
        ctx.create_joint_ex(BITMAP_FLARE + 1, e.position, e.position, e.angle, 12, me(e), 40.0, 80, 0);
    } else if ty == MODEL_ARROW_RING {
        e.blend_mesh = 0;
        e.scale = 1.0;
        e.direction.y = -70.0;
        e.angle.y = rf(ctx, 360);
        ctx.create_joint_ex(BITMAP_FLARE + 1, e.position, e.position, e.angle, 15, me(e), 50.0, 40, 0);
    }
    e.weapon = ctx.packet_serial();
}

/// `MODEL_MULTI_SHOT1..3`: ZzzEffect.cpp:7208-7222, plus the `PlayAnimation(Velocity/6)` that
/// RenderEffects (18305) does before drawing. These types are outside `MODEL_BIRD01..MODEL_SKILL_END`,
/// so the common tail only moves them.
fn multi_shot_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    e.scale += match e.ty {
        MODEL_MULTI_SHOT1 => 0.2,
        MODEL_MULTI_SHOT2 => 0.3,
        _ => 0.25,
    };
    e.blend_mesh_light = life(e) as f32 / 18.0;
    e.alpha = e.blend_mesh_light;
    move_tail(e, ctx);
    // RenderEffects 18303 (before the draw): b->PlayAnimation(..., Velocity/6, ...).
    e.animation_frame += e.velocity / 6.0;
}

/// `MoveEffect` of the shared arrow types (ZzzEffect.cpp:11881-12236).
fn arrow_step(e: &mut Effect, ctx: &mut Ctx) {
    if e.ty == MODEL_ARROW_TANKER {
        tanker_render_fx(e, ctx);
    }
    let lum = luminosity(e, ctx);
    match e.ty {
        MODEL_ARROW => arrow_plain(e, ctx, lum),
        MODEL_ARROW_STEEL | MODEL_ARROW_THUNDER | MODEL_ARROW_LASER | MODEL_ARROW_V | MODEL_ARROW_SAW | MODEL_ARROW_NATURE
        | MODEL_ARROW_WING | MODEL_LACEARROW => arrow_group(e, ctx, lum),
        // ZzzEffect.cpp:12117.
        MODEL_ARROW_SPARK => {
            e.angle.y += 35.0;
            check_client_arrow(e, ctx);
        }
        // ZzzEffect.cpp:12123.
        MODEL_ARROW_RING => {
            set_eye_left(e, e.position);
            e.light = Vec3::new(0.0, 1.0, 0.1);
            ctx.create_effect(MODEL_WAVES, e.position, e.angle, e.light, 4, None, 0, 0, 0, 0.0);
            let light = Vec3::new(lum * 0.6, lum * 0.2, lum * 0.8);
            ctx.add_terrain_light(e.position.x, e.position.y, light, 2.0);
            check_client_arrow(e, ctx);
        }
        // ZzzEffect.cpp:12135.
        MODEL_ARROW_TANKER => {
            e.light = Vec3::new(1.0, 0.0, 0.0);
            ctx.add_terrain_light(e.position.x, e.position.y, e.light, 2.0);
        }
        MODEL_ARROW_DARKSTINGER => darkstinger(e, ctx),
        MODEL_ARROW_GAMBLE => gamble(e, ctx, lum),
        _ => {}
    }
    move_tail(e, ctx);
}

/// `MODEL_ARROW` (ZzzEffect.cpp:11881).
fn arrow_plain(e: &mut Effect, ctx: &mut Ctx, lum: f32) {
    let light = Vec3::new(lum * 0.8, lum * 0.5, lum * 0.2);
    ctx.add_terrain_light(e.position.x, e.position.y, light, 2.0);
    if e.sub_type == 3 || e.sub_type == 4 {
        if e.sub_type == 3 {
            e.angle.x += e.gravity;
        }
        if e.angle.x > 50.0 {
            e.angle.x = 50.0;
        }
        let wall = ctx.terrain_attribute(e.position.x, e.position.y) & TW_NOGROUND;
        if wall != TW_NOGROUND {
            let height = ctx.terrain_height(e.position.x, e.position.y);
            if e.position.z < height {
                e.position.z = height + 10.0;
                e.direction = Vec3::ZERO;
            }
        }
        ctx.create_particle(BITMAP_FIRE, e.position, e.angle, light, 5, 1.0, None);
    } else if e.sub_type == 5 {
        let light = Vec3::new(lum * 0.1, lum * 0.6, lum * 0.3);
        ctx.create_particle(BITMAP_FIRE, e.position, e.angle, light, 0, 1.0, None);
        check_client_arrow(e, ctx);
    } else {
        ctx.create_particle(BITMAP_FIRE, e.position, e.angle, light, 0, 1.0, None);
        check_client_arrow(e, ctx);
    }
}

/// `MODEL_ARROW_STEEL` ... `MODEL_LACEARROW` (ZzzEffect.cpp:11924-12010).
fn arrow_group(e: &mut Effect, ctx: &mut Ctx, lum: f32) {
    let rot = world_rot(ctx);
    if e.ty == MODEL_ARROW_NATURE {
        if e.sub_type == 1 {
            let light = Vec3::new(0.1, 0.4, 0.1);
            ctx.create_sprite(BITMAP_LIGHTNING + 1, e.position, 0.3, light, rot, 0);
            ctx.create_sprite(BITMAP_LIGHTNING + 1, e.position, 0.7, light, -rot, 0);
            let light = Vec3::new(lum * 0.2, lum * 0.8, lum * 0.2);
            ctx.add_terrain_light(e.position.x, e.position.y, light, 2.0);
            e.angle.y += 60.0;
            if life(e) > 25 && life(e) % 2 == 0 {
                for _ in 0..2 {
                    let p = jitter(ctx, 32, 64) + e.position;
                    ctx.create_particle(BITMAP_FLARE, p, e.angle, Vec3::new(0.4, 1.0, 0.2), 5, 0.2, None);
                }
            }
        } else {
            ctx.create_sprite(BITMAP_LIGHTNING + 1, e.position, 0.5, e.light, rot, 0);
            ctx.create_sprite(BITMAP_LIGHTNING + 1, e.position, 1.0, e.light, -rot, 0);
            for _ in 0..4 {
                let p = jitter(ctx, 32, 64) + e.position;
                let ty = BITMAP_FLOWER01 + ctx.rand() % 3;
                ctx.create_particle(ty, p, e.angle, e.light, 0, 1.0, None);
            }
            e.angle.y += 30.0;
            let light = Vec3::new(lum * 0.6, lum * 0.8, lum * 0.8);
            ctx.add_terrain_light(e.position.x, e.position.y, light, 2.0);
        }
    } else if e.ty == MODEL_LACEARROW {
        e.angle.y += 60.0;
        if life(e) % 2 == 0 {
            for _ in 0..3 {
                let p = jitter(ctx, 32, 64) + e.position;
                ctx.create_particle(BITMAP_FLARE, p, e.angle, Vec3::new(0.4, 0.2, 1.0), 5, 0.2, None);
            }
        }
        set_eye_left(e, e.position);
        ctx.create_effect(MODEL_WAVES, e.position, e.angle, e.light, 3, None, 0, 0, 0, 0.0);
        let light = Vec3::new(lum * 0.6, lum * 0.2, lum * 0.8);
        ctx.add_terrain_light(e.position.x, e.position.y, light, 2.0);
    } else if e.ty == MODEL_ARROW_WING {
        ctx.create_sprite(BITMAP_LIGHTNING + 1, e.position, 0.5, e.light, rot, 0);
        ctx.create_sprite(BITMAP_LIGHTNING + 1, e.position, 1.0, e.light, -rot, 0);
        for _ in 0..4 {
            let p = jitter(ctx, 16, 16) + e.position;
            ctx.create_particle(BITMAP_BUBBLE, p, e.angle, e.light, 1, 1.0, None);
        }
        ctx.create_particle(BITMAP_SMOKE, e.position, e.angle, e.light, 0, 1.0, None);
        let light = Vec3::new(lum * 0.6, lum * 0.8, lum * 0.8);
        ctx.add_terrain_light(e.position.x, e.position.y, light, 2.0);
    }
    check_client_arrow(e, ctx);
}

/// `MODEL_ARROW_DARKSTINGER` (ZzzEffect.cpp:12167).
fn darkstinger(e: &mut Effect, ctx: &mut Ctx) {
    check_client_arrow(e, ctx);
    // `int iNumCreateFeather = rand()%2;` (unused, shadowed below).
    let _ = ctx.rand();
    let light = Vec3::new(0.6, 0.7, 0.9);
    // pModel->Animation(...) + TransformByObjectBone(vPos, o, 0), then + Angle·Direction.
    let v_pos = own_bone(e, ctx, 0, Vec3::ZERO) + rotate(e.angle, e.direction);
    ctx.create_sprite(BITMAP_LIGHT, v_pos, 3.0, light, 0.0, 0);
    ctx.create_sprite(BITMAP_LIGHT, v_pos, 2.0, light, 0.0, 0);
    let lt = life(e);
    if matches!(lt, 30 | 28 | 26 | 24) {
        let n = ctx.rand() % 3;
        let v_pos = own_bone(e, ctx, 1, Vec3::ZERO);
        for _ in 0..n {
            ctx.create_effect(MODEL_FEATHER, v_pos, e.angle, light, 0, None, -1, 0, 0, 0.6);
            ctx.create_effect(MODEL_FEATHER, v_pos, e.angle, light, 1, None, -1, 0, 0, 0.6);
        }
    }
    if lt == 30 {
        let v_pos = own_bone(e, ctx, 1, Vec3::ZERO);
        // CreateJoint(BITMAP_FLARE+1, vPos, vPos, Angle, 18, o, 90, 40, 0, 0, -1, Light): the
        // trail's colour comes from the last argument.
        let mut j = Joint::new(BITMAP_FLARE + 1, v_pos, v_pos, e.angle, 18, me(e), 90.0);
        j.pk_key = 40;
        j.light = Vec3::new(0.4, 0.4, 0.9);
        ctx.spawn_joint(j);
    }
}

/// `MODEL_ARROW_GAMBLE` (ZzzEffect.cpp:12214).
fn gamble(e: &mut Effect, ctx: &mut Ctx, lum: f32) {
    let v_light = Vec3::new(0.2, 0.8, 0.5);
    e.angle.y += 60.0;
    for _ in 0..2 {
        let p = jitter(ctx, 32, 64) + e.position;
        ctx.create_particle(BITMAP_SPARK + 1, p, e.angle, v_light, 30, 1.0, None);
    }
    set_eye_left(e, e.position);
    ctx.create_particle(BITMAP_SMOKE, e.position, e.angle, v_light, 24, 1.0, None);
    let light = Vec3::new(lum * 0.2, lum * 0.8, lum * 0.5);
    ctx.add_terrain_light(e.position.x, e.position.y, light, 2.0);
    check_client_arrow(e, ctx);
}

/// ZzzObject.cpp:1806: `RenderMesh(0, RENDER_TEXTURE, Alpha, 0, 1.f, U, V)` with BodyLight
/// (0.7, 0.7, 0.9) and `RenderMesh(1, RENDER_TEXTURE, Alpha, 1, 1.f, U, V)` with (0.3, 0.4, 0.9).
fn darkstinger_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    if e.alpha < 0.01 {
        return;
    }
    for (mesh, light) in [(0, Vec3::new(0.7, 0.7, 0.9)), (1, Vec3::new(0.3, 0.4, 0.9))] {
        let mut d = ModelDraw::of_effect(e);
        d.mesh = mesh;
        d.render_flags = RENDER_TEXTURE;
        d.light = light;
        d.blend_mesh = mesh;
        d.blend_mesh_light = 1.0;
        d.hidden_mesh = -1;
        out.models.push(d);
    }
}

// --- MODEL_ARROW_TANKER / MODEL_ARROW_TANKER_HIT ----------------------------------------------------

/// What `Draw_RenderObject` does for both types after drawing (ZzzObject.cpp:2652-2679); it runs
/// here, at the start of the next tick.
fn tanker_render_fx(e: &mut Effect, ctx: &mut Ctx) {
    if e.alpha < 0.01 {
        return;
    }
    e.light = Vec3::ONE;
    // (The bone-2 position it computes is unused.)
    ctx.create_particle(BITMAP_FIRE + 1, e.position, e.angle, e.light, 8, e.scale - 0.4, me(e));
    ctx.create_particle(BITMAP_FIRE + 1, e.position, e.angle, e.light, 8, e.scale - 0.4, me(e));
    ctx.create_particle(BITMAP_SMOKE, e.position, e.angle, e.light, 38, e.scale, me(e));
    e.light = Vec3::new(1.0, 0.4, 0.0);
    ctx.create_particle(BITMAP_FIRE + 1, e.position, e.angle, e.light, 9, e.scale - 0.4, me(e));
    for i in 0..10 {
        let fi = i as f32;
        e.light = Vec3::new(1.0 - fi * 0.1, 0.4 - fi * 0.04, 0.0);
        let p2 = own_bone(e, ctx, 1, Vec3::new(15.0 * fi, 1.5, 0.0));
        ctx.create_sprite(BITMAP_SPARK + 1, p2, 3.0 + fi * 0.9, e.light, 0.0, 0);
    }
}

/// ZzzObject.cpp:2614 `RenderBody(RENDER_TEXTURE, ...)` + 2658 `RenderBody(RENDER_TEXTURE|
/// RENDER_BRIGHT, Alpha, -2, 1.f, U, V, HiddenMesh)` with BodyLight 1.
fn tanker_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    if e.alpha < 0.01 {
        return;
    }
    out.models.push(ModelDraw::of_effect(e));
    let mut d = ModelDraw::of_effect(e);
    d.render_flags = TEXTURE_BRIGHT;
    d.light = Vec3::ONE;
    d.blend_mesh = -2;
    d.blend_mesh_light = 1.0;
    out.models.push(d);
}

/// ZzzEffect.cpp:4167. (`m_vDeadPosition = Angle` is not read by these types.)
fn tanker_hit_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 100.0;
    e.scale = 1.0;
    e.light = Vec3::ONE;
    e.start_position = e.position;
    let az = match e.sub_type {
        0 => Some(90.0),
        1 => Some(-90.0),
        2 => Some(0.0),
        _ => None,
    };
    if let Some(az) = az {
        e.angle = Vec3::new(40.0, 0.0, az);
        e.direction = Vec3::new(0.0, -80.0, 0.0);
    }
}

/// ZzzEffect.cpp:13773.
fn tanker_hit_step(e: &mut Effect, ctx: &mut Ctx) {
    tanker_render_fx(e, ctx);
    let _ = luminosity(e, ctx);
    let height = ctx.terrain_height(e.position.x, e.position.y);
    if e.position.z < height + 80.0 {
        if e.alpha <= 0.1 {
            e.life_time = 0.0;
        }
        e.alpha /= 1.05;
        e.direction = Vec3::new(0.0, 0.0, -30.0);
        ctx.create_particle(BITMAP_ADV_SMOKE + 1, e.position, e.angle, Vec3::new(1.0, 0.6, 0.2), 0, 1.0, None);
        if e.hidden_mesh != 99 {
            common::create_inferno(ctx, e.position, 1);
            e.start_position = e.position;
            ctx.create_effect(BITMAP_CRATER, e.position, e.angle, e.light, 0, None, -1, 0, 0, 0.0);
        }
        let quake = (ctx.rand() % 8 - 8) as f32 * 0.1 / 2.0;
        ctx.set_earthquake(quake);
        e.hidden_mesh = 99;
    }
    if e.hidden_mesh == 99 {
        e.position.x += (ctx.rand() % 100 - 50) as f32;
        e.position.y += (ctx.rand() % 100 - 50) as f32;
        e.position.z += (ctx.rand() % 100 - 50) as f32;
        e.scale = 2.2;
    }
    if e.visible {
        e.light = Vec3::ONE;
        ctx.create_particle(BITMAP_FIRE + 1, e.position, e.angle, e.light, 8, e.scale - 0.4, me(e));
        ctx.create_particle(BITMAP_SMOKE, e.position, e.angle, e.light, 38, e.scale, me(e));
        e.light = Vec3::new(1.0, 0.4, 0.0);
        ctx.create_particle(BITMAP_FIRE + 1, e.position, e.angle, e.light, 9, e.scale - 0.4, me(e));
    }
    if e.hidden_mesh == 99 {
        e.scale = 1.0;
        e.position = e.start_position;
    }
    ctx.add_terrain_light(e.position.x, e.position.y, Vec3::new(1.0, 0.0, 0.0), 4.0);
    move_tail(e, ctx);
}

// --- MODEL_ARROW_BOMB --------------------------------------------------------------------------------

/// ZzzEffect.cpp:1980.
fn bomb_create(e: &mut Effect, ctx: &mut Ctx) {
    e.velocity = 1.0;
    e.life_time = 30.0;
    arrow_offset(e);
    e.blend_mesh = 0;
    e.scale = 1.0;
    e.direction.y = -30.0;
    e.gravity = -10.0;
    e.life_time = 40.0;
    if e.sub_type == 2 {
        e.user[ATTACK_POINT] = 0.0;
        e.kind = 1;
    }
    e.weapon = ctx.packet_serial();
}

/// ZzzEffect.cpp:12141.
fn bomb_step(e: &mut Effect, ctx: &mut Ctx) {
    let lum = luminosity(e, ctx);
    let light = Vec3::new(1.0, 0.6, 0.4);
    let rot = world_rot(ctx);
    ctx.create_sprite(BITMAP_LIGHT, e.position, 1.0, light, rot, 0);
    ctx.create_sprite(BITMAP_LIGHT, e.position, 2.0, light, -rot, 0);
    for _ in 0..4 {
        let p = jitter(ctx, 16, 16) + e.position;
        ctx.create_particle(BITMAP_BUBBLE, p, e.angle, e.light, 1, 1.0, None);
    }
    common::move_jump(e, ctx);
    if life(e) == 1 {
        common::create_bomb(ctx, e.position, true, 0);
        if owner_is_hero(e, ctx) && e.sub_type != 99 {
            ctx.attack_character_range(e.skill, e.position, 100.0, e.weapon, e.pk_key);
        }
    }
    let light = Vec3::new(lum * 0.6, lum * 0.8, lum * 0.8);
    ctx.add_terrain_light(e.position.x, e.position.y, light, 2.0);
    check_client_arrow(e, ctx);
    move_tail(e, ctx);
}

// --- MODEL_ARROW_DRILL -------------------------------------------------------------------------------

/// ZzzEffect.cpp:3361 (only SubTypes 0 and 2 set anything; others die on their first tick).
fn drill_create(e: &mut Effect, ctx: &mut Ctx) {
    if e.sub_type == 0 || e.sub_type == 2 {
        e.life_time = 30.0;
        e.blend_mesh = -2;
        e.scale = 1.0;
        e.gravity = -10.0;
        e.position.z += 130.0;
        if e.sub_type != 0 {
            ctx.create_effect(MODEL_PIERCING, e.position, e.angle, e.light, 0, me(e), -1, 0, 0, 0.0);
            e.user[ATTACK_POINT] = 0.0;
            e.kind = 1;
        }
        e.direction = Vec3::new(0.0, -70.0, 0.0);
        set_eye_left(e, e.position);
        ctx.create_joint(BITMAP_JOINT_ENERGY, e.position, e.position, e.angle, 5, me(e), 100.0);
        e.weapon = ctx.packet_serial();
    }
}

/// ZzzEffect.cpp:12936; SubType 3 skips the common tail (18065).
fn drill_step(e: &mut Effect, ctx: &mut Ctx) {
    // Draw_RenderObject after drawing (ZzzObject.cpp:2640).
    if e.alpha >= 0.01 {
        let light = Vec3::new(1.0, 0.6, 0.4);
        let p = own_bone(e, ctx, 1, Vec3::ZERO);
        let size = (ctx.world_time() * 0.002).sin() * 0.3 + 1.3;
        let rot = rf(ctx, 360);
        ctx.create_sprite(BITMAP_SHINY + 1, p, size, light, rot, 0);
        ctx.create_sprite(BITMAP_LIGHT, p, 1.5, light, 90.0, 0);
        let rot = rf(ctx, 360);
        ctx.create_sprite(BITMAP_SHINY + 1, p, size, light, rot, 0);
        ctx.create_particle(BITMAP_SPARK, p, e.angle, light, 0, 1.0, None);
    }

    let lum = luminosity(e, ctx);
    if e.sub_type == 0 || e.sub_type == 2 {
        e.angle.y += 30.0;
        for _ in 0..3 {
            ctx.create_particle(BITMAP_SMOKE, e.position, e.angle, Vec3::ONE, 13, 1.0, None);
        }
        if life(e) == 1 {
            common::create_bomb(ctx, e.position, true, 0);
        }
        let eye = e.position + rotate(e.angle, e.direction);
        set_eye_left(e, eye);
        let light = Vec3::new(lum, lum * 0.4, lum * 0.2);
        ctx.add_terrain_light(e.position.x, e.position.y, light, 2.0);
        check_client_arrow(e, ctx);
    }
    if e.sub_type != 3 {
        move_tail(e, ctx);
    }
}

// --- MODEL_ARROW_IMPACT ------------------------------------------------------------------------------

/// ZzzEffect.cpp:4734.
fn impact_create(e: &mut Effect, ctx: &mut Ctx) {
    e.velocity = 1.0;
    e.life_time = 20.0;
    e.scale = 1.8;
    e.blend_mesh = -2;
    e.direction.y = -30.0;
    e.light = Vec3::new(0.3, 0.8, 1.0);
    e.position += rotate(e.angle, Vec3::new(-10.0, -80.0, 200.0));
    e.angle.x = -30.0;
    ctx.create_joint_ex(BITMAP_FLASH, e.position, e.position, e.angle, 4, me(e), 40.0, 50, 0);
    e.weapon = ctx.packet_serial();
}

/// ZzzEffect.cpp:14714.
fn impact_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    e.angle.x -= 5.0;
    e.direction.y -= 8.0;
    let owner_pos = ctx.target_position(e.owner);
    if life(e) < 2
        && e.sub_type == 0
        && let Some(op) = owner_pos
    {
        e.angle.x = 90.0;
        e.sub_type = 1;
        let mut p = op;
        p.x += rf(ctx, 100) - 50.0;
        p.y += rf(ctx, 100) - 50.0;
        p.z += 1200.0;
        ctx.create_joint(BITMAP_FLASH, p, p, e.angle, 2, me(e), 50.0);
        ctx.create_joint(BITMAP_FLASH, p, p, e.angle, 3, me(e), 50.0);
    } else if e.sub_type == 1 {
        e.live = false;
    }
    move_tail(e, ctx);
}

// --- MODEL_IRON_RIDER_ARROW --------------------------------------------------------------------------

/// ZzzEffect.cpp:621. `Velocity` is the flight speed here (and also the animation speed).
fn iron_rider_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = 12.0;
    e.scale = 0.5;
    e.velocity = 70.0;
    e.light = Vec3::new(0.8, 1.0, 0.8);
    e.direction = rotate(e.angle, Vec3::new(0.0, -1.0, 0.0));
    ctx.create_joint(BITMAP_JOINT_HEALING, e.position, e.position, e.angle, 14, me(e), 30.0);
}

/// ZzzEffect.cpp:7188.
fn iron_rider_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    e.scale *= 1.05;
    if life(e) < 5 {
        e.light *= 0.7;
    }
    e.position += e.direction * e.velocity;
    ctx.create_sprite(BITMAP_LIGHT + 3, e.position, 4.0, e.light, 0.0, 0);
    ctx.create_sprite(BITMAP_DS_EFFECT, e.position, 2.5, e.light, 0.0, 0);
    ctx.create_particle(BITMAP_SPARK + 1, e.position, e.angle, e.light, 10, 3.0, None);
    set_eye_left(e, e.position);
    ctx.create_effect(MODEL_WAVES, e.position, e.angle, e.light, 3, None, 0, 0, 0, 0.0);
    move_tail(e, ctx);
}

// --- MODEL_KENTAUROS_ARROW ---------------------------------------------------------------------------

/// ZzzEffect.cpp:647.
fn kentauros_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 34.0;
    e.scale = 0.7;
    e.velocity = 70.0;
    e.alpha = 0.0;
    e.light = Vec3::ONE;
}

/// ZzzEffect.cpp:7307.
fn kentauros_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    if life(e) == 24 {
        e.direction = rotate(e.angle, Vec3::new(0.0, -1.0, 0.0));
        ctx.create_joint_ex(BITMAP_FLARE + 1, e.position, e.position, e.angle, 17, me(e), 15.0, 40, 0);
    }
    if life(e) <= 24 {
        e.scale *= 1.05;
        let v_light = Vec3::new(0.3, 0.5, 1.0);
        e.position += e.direction * e.velocity;
        ctx.create_particle(BITMAP_SMOKE, e.position, e.angle, v_light, 26, 0.2, None);
        ctx.create_particle(BITMAP_SMOKE, e.position, e.angle, v_light, 26, 0.2, None);
    }
    e.alpha += 0.001;
    move_tail(e, ctx);
}

// --- MODEL_SUMMON ------------------------------------------------------------------------------------

/// ZzzEffect.cpp:2248.
fn summon_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = 60.0;
    e.blend_mesh = 0;
    e.scale = if e.sub_type == 0 { 0.7 } else { 1.2 };
    e.angle.z += rf(ctx, 360);
}

/// ZzzEffect.cpp:10646.
fn summon_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    if life(e) >= 30 {
        e.light *= 1.03;
    } else {
        e.light /= 1.08;
    }
    e.angle.z += 1.0;
    move_tail(e, ctx);
}

/// RenderEffects 18326 → ZzzObject.cpp:515: nothing in Kanturu 3rd
/// (`M39Kanturu3rd::IsInKanturu3rd()`); else `RenderMesh(0, RENDER_TEXTURE, ...)` and
/// `RenderMesh(1 / 2, RENDER_TEXTURE|RENDER_BRIGHT, ...)` with `BodyLight = o->Light`, each with
/// `Alpha, BlendMesh, BlendMeshLight, U, V`.
fn summon_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    if ctx.world_active == WD_39KANTURU_3RD || e.alpha < 0.01 {
        return;
    }
    for (mesh, flags) in [(0, RENDER_TEXTURE), (1, TEXTURE_BRIGHT), (2, TEXTURE_BRIGHT)] {
        let mut d = ModelDraw::of_effect(e);
        d.mesh = mesh;
        d.render_flags = flags;
        d.hidden_mesh = -1;
        out.models.push(d);
    }
}

// --- MODEL_DARK_ELF_SKILL ----------------------------------------------------------------------------

/// ZzzEffect.cpp:2179.
fn dark_elf_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.blend_mesh = -2;
    e.life_time = 30.0;
    e.scale = 0.0;
    e.direction = Vec3::new(0.0, -40.0, 0.0);
}

/// ZzzEffect.cpp:10499.
fn dark_elf_step(e: &mut Effect, ctx: &mut Ctx) {
    let lum = luminosity(e, ctx);
    if e.scale < 0.8 {
        e.scale += 0.2;
    } else {
        e.scale -= 0.4;
    }
    e.blend_mesh_light = life(e) as f32 * 0.1;
    e.blend_mesh_tex_coord.x = -(life(e) as f32) * 0.2;
    let mut angle = e.angle;
    angle.z += 7.0;
    let position = rotate(angle, Vec3::new(0.0, -500.0, 0.0)) + e.position;
    for _ in 0..6 {
        ctx.create_particle(BITMAP_SMOKE, position, angle, e.light, 25, 1.0, None);
    }
    let light = Vec3::new(lum * 0.3, lum * 0.6, lum);
    ctx.add_terrain_light(position.x, position.y, light, 3.0);
    move_tail(e, ctx);
}

// --- MODEL_MAGIC1 ------------------------------------------------------------------------------------

/// ZzzEffect.cpp:2035.
fn magic1_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 20.0;
    e.blend_mesh = 0;
}

/// ZzzEffect.cpp:9881.
fn magic1_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    // (The original dereferences Owner unconditionally.)
    if let Some(p) = ctx.target_position(e.owner) {
        e.position = p;
        e.position.z += 100.0;
    }
    e.angle.y += 20.0;
    e.blend_mesh_light = life(e) as f32 * 0.1;
    move_tail(e, ctx);
}

// --- MODEL_MAGIC2 ------------------------------------------------------------------------------------

/// ZzzEffect.cpp:2185.
fn magic2_create(e: &mut Effect, ctx: &mut Ctx) {
    e.blend_mesh = 0;
    e.life_time = 20.0;
    e.direction = Vec3::new(0.0, -60.0, 0.0);
    if e.sub_type == 2 {
        e.weapon = ctx.next_packet_serial();
    }
}

/// ZzzEffect.cpp:10526.
fn magic2_step(e: &mut Effect, ctx: &mut Ctx) {
    let lum = luminosity(e, ctx);
    e.blend_mesh_light = life(e) as f32 * 0.1;
    e.blend_mesh_tex_coord.x = -(life(e) as f32) * 0.2;
    for _ in 0..4 {
        ctx.create_particle(BITMAP_SMOKE, e.position, e.angle, e.light, 3, 1.0, None);
    }
    let light = Vec3::new(lum * 0.3, lum * 0.6, lum);
    ctx.add_terrain_light(e.position.x, e.position.y, light, 3.0);
    if e.sub_type == 2 {
        e.hidden_mesh = 0;
        let light2 = Vec3::ONE;
        let mut position = e.position;
        // `Angle[2] += rand()%10-5;` (the local Angle is not used afterwards).
        let _ = ctx.rand();
        let scale = (ctx.rand() % 32 + 80) as f32 * 0.015;
        ctx.create_particle(BITMAP_SMOKE, e.position, e.angle, light, 11, scale, None);
        position.z += 50.0;
        let rot = rf(ctx, 360);
        ctx.create_sprite(BITMAP_SHINY + 1, position, 1.5, light2, rot, 0);
        let rot = rf(ctx, 360);
        ctx.create_sprite(BITMAP_SHINY + 1, position, 1.5, light2, rot, 0);
        let rot = rf(ctx, 360);
        ctx.create_sprite(BITMAP_LIGHT, position, 3.5, light, rot, 0);
    }
    move_tail(e, ctx);
}

// --- MODEL_SAW ---------------------------------------------------------------------------------------

/// ZzzEffect.cpp:2002.
fn saw_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 10.0;
    e.position.z += 130.0;
    e.direction = rotate(e.angle, Vec3::new(0.0, -60.0, 0.0));
}

/// ZzzEffect.cpp:9871; the common tail moves it with `MoveParticle(o, false)` (18088).
fn saw_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    e.angle.z -= 30.0;
    common::play_animation(e, ctx);
    common::move_particle(e, false);
}

// --- Tests: client-side arrow hits (`CheckClientArrow`) ---------------------------------------------

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bevy::math::Vec3;
    use bevy::prelude::Entity;

    use super::*;
    use crate::fx::sim::{self, Actors, FxWorld};
    use crate::terrain::TerrainLights;

    /// Where the hero stands (the arrows are shot from here, angle 0: they fly towards -Y).
    const HERO_AT: Vec3 = Vec3::new(12800.0, 12800.0, 0.0);
    const MONSTER_KEY: i32 = 7;

    fn registry() -> Registry {
        let mut r = Registry::default();
        crate::fx::kinds::register_all(&mut r);
        r
    }

    /// The hero at [`HERO_AT`] and, if `monster_y` is given, a monster at (x, `monster_y`) on the
    /// arrows' path (they start at x - 10).
    fn scene(monster_y: Option<f32>) -> (Actors, Entity) {
        let hero = Entity::PLACEHOLDER;
        let identity: mu_data::bmd::Mat34 = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]];
        let mut a = Actors::default();
        a.map.insert(hero, Actor { position: HERO_AT, bones: Arc::new(vec![identity; 80]), live: true, ..Default::default() });
        a.hero = Some(hero);
        if let Some(y) = monster_y {
            let monster = Actor { kind: KIND_MONSTER, key: MONSTER_KEY, position: Vec3::new(HERO_AT.x - 10.0, y, 0.0), ..Default::default() };
            a.map.insert(Entity::from_bits(1 << 32 | 5), monster);
        }
        (a, hero)
    }

    /// A world with one effect `ty` shot by the hero (`kind` = the original `Skill` argument, 1 for
    /// the hero's arrows, `CreateArrows` ZzzEffect.h:90).
    fn shoot(ty: i32, sub_type: i32, kind: i32, hero: Entity) -> FxWorld {
        let e = Effect::new(1, ty, HERO_AT, Vec3::ZERO, Vec3::ONE, sub_type, Some(Target::Actor(hero)), -1, kind, 0, 0.0);
        FxWorld { next_effect_id: 1, pending: vec![Spawn::Effect(e)], ..Default::default() }
    }

    /// Ticks `world` `ticks` times (the first tick creates the pending effect); returns the tick
    /// at which effect 1 disappeared, if it did.
    fn run(world: &mut FxWorld, actors: &Actors, ticks: usize) -> Option<usize> {
        let registry = registry();
        let mut lights = TerrainLights::default();
        for t in 0..ticks {
            sim::step(world, &registry, actors, None, &mut lights, t as f32 * 40.0, 0);
            if t > 0 && !world.effects.iter().any(|e| e.id == 1) {
                return Some(t);
            }
        }
        None
    }

    fn has_particle(world: &FxWorld, ty: i32) -> bool {
        world.particles.iter().any(|p| p.ty == ty)
    }

    /// Another character's arrow (`Kind` 0) stops on the monster in its path
    /// (`CheckCharacterRange`, ZzzEffect.cpp:6781-6823), long before its 30 ticks.
    #[test]
    fn arrow_dies_on_a_monster() {
        let (actors, hero) = scene(Some(HERO_AT.y - 400.0));
        let mut world = shoot(MODEL_ARROW, 0, 0, hero);
        let died = run(&mut world, &actors, 30).expect("the arrow must stop on the monster");
        assert!(died < 10, "died at tick {died}");
        // A plain arrow does not explode.
        assert!(!has_particle(&world, BITMAP_EXPLOTION));
    }

    /// With nothing in range the arrow flies on along -Y.
    #[test]
    fn arrow_flies_on_without_targets() {
        let (actors, hero) = scene(None);
        let mut world = shoot(MODEL_ARROW, 0, 0, hero);
        assert_eq!(run(&mut world, &actors, 10), None);
        let e = world.effects.iter().find(|e| e.id == 1).expect("the arrow is alive");
        // Created at y - 60, then 9 moves of 70.
        assert!((e.position.y - (HERO_AT.y - 60.0 - 9.0 * 70.0)).abs() < 0.5, "{:?}", e.position);
        // A monster far to the side does not stop it either.
        let (mut actors, hero) = scene(None);
        actors.map.insert(Entity::from_bits(1 << 32 | 6), Actor { kind: KIND_MONSTER, position: HERO_AT + Vec3::new(500.0, -300.0, 0.0), ..Default::default() });
        let mut world = shoot(MODEL_ARROW, 0, 0, hero);
        assert_eq!(run(&mut world, &actors, 10), None);
    }

    /// A bomb arrow explodes where it hits (`CreateBomb(o->Position, true)`), not only at the end
    /// of its life.
    #[test]
    fn bomb_arrow_explodes_on_a_monster() {
        let (actors, hero) = scene(Some(HERO_AT.y - 200.0));
        let mut world = shoot(MODEL_ARROW_BOMB, 0, 0, hero);
        let died = run(&mut world, &actors, 40).expect("the bomb arrow must stop on the monster");
        assert!(died < 10, "died at tick {died}");
        assert!(has_particle(&world, BITMAP_EXPLOTION), "no explosion");
    }

    /// The hero's arrow (`Kind` 1, owner = Hero) only stops on a hit `AttackCharacterRange` reports,
    /// and MuMain's never reports one (ZzzEffect.cpp:118-121, 5726): the server decides the damage
    /// and the arrow flies on past the monster.
    #[test]
    fn hero_holy_arrow_flies_on_past_the_monster() {
        let (actors, hero) = scene(Some(HERO_AT.y - 50.0));
        let mut world = shoot(MODEL_ARROW_HOLY, 1, 1, hero);
        assert_eq!(run(&mut world, &actors, 5), None, "the hero's arrow does not stop on the monster");
    }

    /// The same holy arrow shot by someone else (`Kind` 0, SubType 1) stops without exploding.
    #[test]
    fn other_holy_arrow_subtype_1_does_not_explode() {
        let (actors, hero) = scene(Some(HERO_AT.y - 50.0));
        let mut world = shoot(MODEL_ARROW_HOLY, 1, 0, hero);
        assert_eq!(run(&mut world, &actors, 30), Some(1));
        assert!(!has_particle(&world, BITMAP_EXPLOTION));
    }

    /// Penetration (`AT_SKILL_PIERCING`, SubType 2) goes through the monster and keeps flying.
    #[test]
    fn piercing_arrow_goes_through() {
        let (actors, hero) = scene(Some(HERO_AT.y - 400.0));
        let mut world = shoot(MODEL_ARROW, 2, 0, hero);
        world.hero_skills = vec![AT_SKILL_PIERCING];
        assert_eq!(run(&mut world, &actors, 12), None, "the piercing arrow must not stop");
        let e = world.effects.iter().find(|e| e.id == 1).expect("alive");
        assert!(e.position.y < HERO_AT.y - 500.0, "{:?}", e.position);
    }
}