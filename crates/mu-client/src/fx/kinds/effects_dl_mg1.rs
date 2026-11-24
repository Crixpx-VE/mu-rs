//! Dark Lord / Magic Gladiator effects (`Effects[]`), part 1: Fire Scream (`MODEL_DARK_SCREAM`,
//! `MODEL_DARK_SCREAM_FIRE`), the Dark Lord skill flashes (`MODEL_DARKLORD_SKILL`), the Empire
//! Guardian boomerang (`MODEL_DEASULER`), the homing spirits (`MODEL_DEATH_SPI_SKILL`,
//! `MODEL_PIER_PART`), the earthquake stones (`MODEL_GROUND_STONE`, `MODEL_GROUND_STONE2`),
//! `MODEL_SKILL_JAVELIN`, `MODEL_SWORD_FORCE`, `MODEL_STUN_STONE`, `MODEL_SKIN_SHELL`,
//! `MODEL_MANA_RUNE`, `MODEL_AIR_FORCE`, `MODEL_WAVES`, `MODEL_COMBO`, `MODEL_FALL_STONE_EFFECT`,
//! the Fenrir/Twintail ground marks (`MODEL_FENRIR_FOOT_THUNDER`, `MODEL_TWINTAIL_EFFECT`),
//! `MODEL_FENRIR_SKILL_DAMAGE` and `MODEL_PROTECTGUILD`.
//!
//! Translated from Main 5.2 `ZzzEffect.cpp` (`CreateEffect`, `MoveEffect` and its common tail
//! 18060-18124, `CheckClientArrow` 6758, `RenderEffects`) and the type-specific branches of
//! `Draw_RenderObject` (`ZzzObject.cpp` 399-2680), which is what `RenderObject(o)` runs for them.
//!
//! Notes:
//! - `MODEL_FENRIR_THUNDER` is in this batch but is already registered by `objects_kanturu.rs`;
//!   it is not registered here.
//! - `RenderObject` passes with `RENDER_CHROME` (environment-mapped `BITMAP_CHROME`) and texture
//!   overrides (`RenderBody(..., Texture)`) cannot be expressed with `ModelDraw`: TODOs.
//! - Function arguments that call `rand()` are evaluated left to right (like the other kind files).

use bevy::math::Vec3;

use super::common::{self, luminosity, move_particle, move_tail, play_animation, rf};
use crate::fx::core::*;

/// `TW_NOMOVE`, `TW_NOGROUND`, `TW_WATER` (`_define.h:80-82`).
const TW_NOMOVE: u16 = 0x0004;
const TW_NOGROUND: u16 = 0x0008;
const TW_WATER: u16 = 0x0010;

/// `user` slot that carries the creating effect's `LifeTime` to a child whose `CreateEffect` reads
/// `o->Owner->LifeTime` (`MODEL_PIER_PART` / `MODEL_DEATH_SPI_SKILL` subtype 1).
const SLOT_OWNER_LIFE: usize = 7;

pub fn register(r: &mut Registry) {
    let k = |create: EffectCreate, step: EffectStep, draw: EffectDraw| EffectKind { create: Some(create), step: Some(step), draw: Some(draw) };

    r.effect(MODEL_DARK_SCREAM, k(scream_create, scream_step, scream_draw));
    r.effect(MODEL_DARK_SCREAM_FIRE, k(scream_create, scream_step, scream_draw));
    r.effect(MODEL_DARKLORD_SKILL, k(darklord_create, darklord_step, draw_plain));
    r.effect(MODEL_DEASULER, k(deasuler_create, deasuler_step, deasuler_draw));
    r.effect(MODEL_DEATH_SPI_SKILL, k(death_spi_create, death_spi_step, draw_plain));
    r.effect(MODEL_PIER_PART, k(pier_part_create, pier_part_step, draw_plain));
    r.effect(MODEL_GROUND_STONE, k(ground_stone_create, ground_stone_step, draw_plain));
    r.effect(MODEL_GROUND_STONE2, k(ground_stone_create, ground_stone_step, draw_plain));
    r.effect(MODEL_SKILL_JAVELIN, k(javelin_create, javelin_step, javelin_draw));
    r.effect(MODEL_SWORD_FORCE, k(sword_force_create, sword_force_step, sword_force_draw));
    r.effect(MODEL_STUN_STONE, k(stun_stone_create, stun_stone_step, stun_stone_draw));
    r.effect(MODEL_SKIN_SHELL, k(skin_shell_create, skin_shell_step, skin_shell_draw));
    r.effect(MODEL_MANA_RUNE, k(mana_rune_create, mana_rune_step, mana_rune_draw));
    r.effect(MODEL_AIR_FORCE, k(air_force_create, air_force_step, draw_plain));
    r.effect(MODEL_WAVES, k(waves_create, waves_step, waves_draw));
    r.effect(MODEL_COMBO, k(combo_create, combo_step, draw_plain));
    r.effect(MODEL_FALL_STONE_EFFECT, k(fall_stone_create, fall_stone_step, draw_plain));
    r.effect(MODEL_FENRIR_FOOT_THUNDER, k(foot_thunder_create, foot_thunder_step, foot_thunder_draw));
    r.effect(MODEL_TWINTAIL_EFFECT, k(twintail_create, twintail_step, twintail_draw));
    r.effect(MODEL_FENRIR_SKILL_DAMAGE, k(skill_damage_create, skill_damage_step, common::draw_default));
    r.effect(MODEL_PROTECTGUILD, k(protect_guild_create, protect_guild_step, protect_guild_draw));
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

/// `MoveHumming(Position, Angle, TargetPosition, Turn)` (`ZzzAI.cpp:131`): turns `angle` towards
/// the target (Z and X) by at most `turn` degrees and returns the 3D distance.
fn move_humming(position: Vec3, angle: &mut Vec3, target: Vec3, turn: f32) -> f32 {
    let target_angle = create_angle(position.x, position.y, target.x, target.y);
    angle.z = turn_angle(angle.z, target_angle, turn);
    let range = position - target;
    let distance = (range.x * range.x + range.y * range.y).sqrt();
    let target_angle = 360.0 - create_angle(position.z, distance, target.z, 0.0);
    angle.x = turn_angle(angle.x, target_angle, turn);
    range.length()
}

/// `o->Owner->Position` / `o->Owner->Angle` of a character or an effect owner (`None` = NULL).
fn owner_pose(ctx: &Ctx, owner: Option<Target>) -> Option<(Vec3, Vec3)> {
    match owner? {
        Target::Actor(_) => ctx.actor(owner).map(|a| (a.position, a.angle)),
        Target::Effect(id) => ctx.effects.iter().find(|i| i.id == id).map(|i| (i.position, i.angle)),
    }
}

/// `CreateEffect(ty, o->Position, o->Angle, o->Light, sub, o)` for a child whose create reads
/// `o->Owner->LifeTime`: the parent's current `LifeTime` travels in the child's
/// [`SLOT_OWNER_LIFE`].
fn create_child_with_owner_life(ctx: &mut Ctx, e: &Effect, ty: i32, sub: i32) {
    *ctx.next_effect_id += 1;
    let mut child = Effect::new(*ctx.next_effect_id, ty, e.position, e.angle, e.light, sub, me(e), -1, 0, 0, 0.0);
    // Set before its creation code runs, which reads it.
    child.user[SLOT_OWNER_LIFE] = e.life_time;
    ctx.spawn_effect(child);
}

/// `o->LifeTime = o->Owner->LifeTime` at creation.
fn owner_life_time(e: &Effect) -> f32 {
    match e.owner {
        Some(Target::Effect(_)) => e.user[SLOT_OWNER_LIFE],
        // TODO(original): the owner is a character (`Owner->LifeTime` of its OBJECT); `Actor` has
        // no LifeTime, so 0 is used (the effect dies on its first tick).
        _ => 0.0,
    }
}

/// `CheckClientArrow(o)` (ZzzEffect.cpp:6758) for `MODEL_DARK_SCREAM`.
fn check_client_arrow(e: &mut Effect, ctx: &mut Ctx) {
    // TODO(original): during Castle Siege (`battleCastle::IsBattleCastleStart()`), inside a
    // TW_NOATTACKZONE tile: Velocity = 0, Direction = 0, LifeTime /= 2 and return.
    // TODO(original): MODEL_DARK_SCREAM: AttackCharacterRange(Hero->CurrentSkill, Position, 100,
    // Hero->Object.Weapon, PKKey, AttackPoint[0]) (ZzzEffect.cpp:6773-6779).
    if e.kind == 0 || e.sub_type == 99 {
        // TODO(original): CheckCharacterRange(o, 100, PKKey); on a hit, by the hero's skill
        // (CharacterAttribute->Skill[o->Skill]): AT_SKILL_PIERCING with SubType 2 →
        // CreateJoint(BITMAP_FLARE, Position, Position, Angle, 6, o, 30); otherwise Live = false
        // (+ CreateBomb for AT_SKILL_PIERCING).
    } else {
        // `if (rand()%2 == 0)` is always evaluated.
        let _ = ctx.rand();
        // TODO(original): if Owner == Hero: AttackPoint countdown for AT_SKILL_PIERCING, else
        // AttackCharacterRange(o->Skill, Position, 100/200, Weapon, PKKey); on a hit: Piercing
        // SubType 2 → AttackPoint = 2 and CreateJoint(BITMAP_FLARE, ..., 6, o, 30); otherwise
        // Live = false and PlayBuffer(SOUND_ATTACK01 + 5 + rand()%4). (PJH_FIX_4_BUGFIX_7 is off.)
    }
}

/// Generic `CInterpolateContainer::GetCurrentValue` (w_ObjectInfo.cpp:7): the first segment
/// `(rate_start, rate_end, start, end)` with `rate_start <= rate < rate_end`, linearly
/// interpolated (`VectorInterpolation_F`); `None` if no segment matches (output left unchanged).
fn interpolate(segments: &[(f32, f32, Vec3, Vec3)], rate: f32) -> Option<Vec3> {
    let &(s, end, a, b) = segments.iter().find(|&&(s, end, _, _)| s <= rate && end > rate)?;
    let w = (rate - s) / (end - s);
    Some(a + (b - a) * w)
}

// --- Drawing helpers ------------------------------------------------------------------------------

/// `RenderObject(o)` with `o`'s state: nothing when `Alpha < 0.01` (`Calc_RenderObject`,
/// ZzzObject.cpp:245). `BodyLight(o,b)` adds the terrain light to `o->Light`.
/// TODO(original): no terrain light at draw time (only `o->Light` is used, like the other kinds);
/// `HeadAngle` (passed to `b->Animation`) is not applied.
fn object_draw(e: &Effect) -> Option<ModelDraw> {
    if e.alpha < 0.01 {
        return None;
    }
    Some(ModelDraw::of_effect(e))
}

/// `RenderObject(o)` → `RenderBody(RENDER_TEXTURE, ...)` (the generic `else` of
/// `Draw_RenderObject`, ZzzObject.cpp:2612).
fn draw_plain(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    out.models.extend(object_draw(e));
}

// --- MODEL_DARK_SCREAM / MODEL_DARK_SCREAM_FIRE (Fire Scream) ----------------------------------------

/// `user` slot: `o->AttackPoint[0]`.
const SLOT_ATTACK_POINT: usize = 0;

/// ZzzEffect.cpp:1798-1964 (the arrows' shared case) and 1864-1893.
fn scream_create(e: &mut Effect, ctx: &mut Ctx) {
    e.velocity = 1.0;
    e.life_time = 30.0;
    e.position += rotate(e.angle, Vec3::new(-10.0, -60.0, 135.0));

    // ZzzEffect.cpp:1864.
    e.life_time = 19.0;
    e.scale = 2.3;
    e.direction.y = -35.0;
    if e.ty == MODEL_DARK_SCREAM {
        e.scale = 0.9;
        e.position.z = ctx.terrain_height(e.position.x, e.position.y) + 3.0;
        let light = Vec3::new(0.0, 1.0, 0.0);
        if e.sub_type == 0 {
            ctx.create_joint_ex(BITMAP_JOINT_FORCE, e.position, e.position, e.angle, 7, me(e), 150.0, 40, 0);
        } else if e.sub_type == 1 {
            ctx.create_joint_ex(BITMAP_JOINT_FORCE, e.position, e.position, e.angle, 20, me(e), 400.0, 40, 0);
        }
        e.position.z += 20.0;
        e.position += rotate(e.angle, Vec3::new(0.0, -20.0, 0.0));
        ctx.create_particle(BITMAP_BLUE_BLUR, e.position, e.angle, light, 1, 1.0, None);
    }

    // ZzzEffect.cpp:1918.
    if e.sub_type == 2 {
        ctx.create_effect(MODEL_PIERCING, e.position, e.angle, e.light, 0, me(e), -1, 0, 0, 0.0);
        e.user[SLOT_ATTACK_POINT] = 0.0;
        e.kind = 1;
    }
    // TODO(original): o->Weapon = CharacterMachine->PacketSerial (network).
}

/// ZzzEffect.cpp:12011-12035, then the common tail (PlayAnimation + MoveParticle(o, true)).
fn scream_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    if e.ty == MODEL_DARK_SCREAM_FIRE {
        e.scale -= 0.14;
    }
    if e.ty == MODEL_DARK_SCREAM {
        e.scale -= 0.04;
    }
    if e.scale < 0.1 {
        e.scale = 0.0;
    }
    e.position.z = ctx.terrain_height(e.position.x, e.position.y) + 3.0;
    ctx.create_particle(BITMAP_FLAME, e.position, e.angle, e.light, 8, (e.scale - 0.4) * 3.5, None);
    if e.ty == MODEL_DARK_SCREAM {
        check_client_arrow(e, ctx);
    }
    move_tail(e, ctx);
}

/// ZzzObject.cpp:1537-1544: `RenderBody` with `BlendMeshLight` 1; the fire is `RENDER_BRIGHT`.
fn scream_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    let Some(mut d) = object_draw(e) else { return };
    d.blend_mesh_light = 1.0;
    d.bright = e.ty == MODEL_DARK_SCREAM_FIRE;
    out.models.push(d);
}

// --- MODEL_DARKLORD_SKILL ------------------------------------------------------------------------------

/// ZzzEffect.cpp:3675.
fn darklord_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = 10.0;
    e.scale = 0.2;
    e.velocity = 0.1;
    if e.sub_type <= 1 {
        let mut angle = 45.0 + (-90.0 * e.sub_type as f32);
        if ctx.rand() % 2 == 0 {
            angle = if e.sub_type != 0 { 45.0 - 90.0 } else { 45.0 };
        }
        e.angle = Vec3::new(45.0, angle, 0.0);
    } else if e.sub_type == 2 {
        e.life_time = 12.0;
        e.velocity = 0.4;
    }
}

/// ZzzEffect.cpp:13353. `Velocity` is also the animation speed of the common tail.
/// Drawn with `BodyLight = o->Light` (ZzzObject.cpp:960), i.e. like [`draw_plain`].
fn darklord_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    if e.sub_type <= 1 {
        e.scale += e.velocity;
        e.velocity += 0.02;
        if e.life_time < 7.0 {
            e.blend_mesh_light /= 1.8;
        }
    } else if e.sub_type == 2 {
        e.scale += e.velocity;
        e.velocity -= 0.06;
        if e.life_time < 8.0 {
            e.blend_mesh_light /= 1.5;
        }
    } else {
        e.scale += e.velocity;
        e.velocity += 0.04;
        if e.life_time < 8.0 {
            e.blend_mesh_light /= 1.8;
        }
    }
    move_tail(e, ctx);
}

// --- MODEL_DEASULER (Empire Guardian: the spinning boomerang sword) ----------------------------------

/// `ExtState` = `TOTAL_LIFETIME` (ZzzEffect.cpp:3494).
const DEASULER_TOTAL_LIFETIME: f32 = 55.0;
/// `user` slots: the first position segment's end rate, `v3PosProcess01`, `v3PosProcessFinal`
/// and `o->ExtState`.
const SLOT_RATE_FIRST: usize = 0;
const SLOT_PROCESS01: usize = 1;
const SLOT_PROCESS_FINAL: usize = 4;
const SLOT_EXT_STATE: usize = 7;

fn slot_vec(e: &Effect, slot: usize) -> Vec3 {
    Vec3::new(e.user[slot], e.user[slot + 1], e.user[slot + 2])
}

fn set_slot_vec(e: &mut Effect, slot: usize, v: Vec3) {
    e.user[slot] = v.x;
    e.user[slot + 1] = v.y;
    e.user[slot + 2] = v.z;
}

/// `o->m_Interpolates.m_vecInterpolatesAngle` (ZzzEffect.cpp:3549-3560).
fn deasuler_angle(e: &Effect, rate: f32) -> Option<Vec3> {
    let r1 = e.user[SLOT_RATE_FIRST];
    let up = Vec3::new(0.0, 90.0, 0.0);
    interpolate(&[(0.0, r1, Vec3::ZERO, up), (r1, 1.01, up, Vec3::new(0.0, 90.0, 2560.0))], rate)
}

/// `o->m_Interpolates.m_vecInterpolatesPos` (ZzzEffect.cpp:3561-3571); the start is
/// `StartPosition` (the creation position).
fn deasuler_pos(e: &Effect, rate: f32) -> Option<Vec3> {
    let r1 = e.user[SLOT_RATE_FIRST];
    let p1 = slot_vec(e, SLOT_PROCESS01);
    let p2 = slot_vec(e, SLOT_PROCESS_FINAL);
    interpolate(&[(0.0, r1, e.start_position, p1), (r1, 1.01, p1, p2)], rate)
}

/// ZzzEffect.cpp:3491. `Light` carries the target position.
fn deasuler_create(e: &mut Effect, ctx: &mut Ctx) {
    if e.sub_type != 0 {
        return;
    }
    // `o->Owner->Position`: a missing owner would crash the original; here the effect dies.
    let Some((owner_pos, _)) = owner_pose(ctx, e.owner) else {
        e.live = false;
        return;
    };
    e.user[SLOT_EXT_STATE] = DEASULER_TOTAL_LIFETIME;
    e.life_time = DEASULER_TOTAL_LIFETIME;
    e.gravity = 0.0;
    e.hidden_mesh = 1;
    e.distance = 0.0;
    e.alpha = 1.0;
    // TODO(original): o->ChromeEnable = false (no such field).
    e.head_angle = e.angle;
    e.start_position = e.position;
    e.direction = e.light - e.position;
    e.distance = e.direction.length();
    // The original divides by 0 (NaN direction) when the target is the start point; skipped here.
    if e.distance > 0.0 {
        e.direction /= e.distance;
    }

    const TOTAL_DIST: f32 = 1700.0;
    let mut modify02 = owner_pos;
    let mut modify = e.position;
    modify02.z = e.light.z;
    modify.z = e.light.z;
    let mut target_modify = e.light;
    modify.z += 100.0;
    modify02.z += 100.0;
    target_modify.z += 100.0;
    // `VectorDistNormalize` (`VectorNormalize` leaves a zero vector as is).
    let dir_modify = (target_modify - modify02).normalize_or_zero();

    let first_dist = e.distance * 0.3;
    let process01 = modify + dir_modify * first_dist;
    let process_final = modify + dir_modify * TOTAL_DIST;
    e.user[SLOT_RATE_FIRST] = first_dist / TOTAL_DIST;
    set_slot_vec(e, SLOT_PROCESS01, process01);
    set_slot_vec(e, SLOT_PROCESS_FINAL, process_final);

    // GetAngleCurrent / GetPosCurrent at rate 0 (without HeadAngle here).
    if let Some(a) = deasuler_angle(e, 0.0) {
        e.angle = a;
    }
    if let Some(p) = deasuler_pos(e, 0.0) {
        e.position = p;
    }
}

/// ZzzEffect.cpp:13086-13223, then the common tail.
fn deasuler_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    e.visible = true;
    let ext = e.user[SLOT_EXT_STATE];
    let lt = life(e) as f32;
    let current_rate = 1.0 - lt / ext;

    // The containers are only filled for subtype 0.
    let filled = e.sub_type == 0;
    if filled {
        if let Some(a) = deasuler_angle(e, current_rate) {
            e.angle = a + e.head_angle;
        }
        if let Some(p) = deasuler_pos(e, current_rate) {
            e.position = p;
        }
    }

    // iTYPESWORDFORCE == 1: blur of the sword between rate 0 and 0.9.
    let (blur_start, blur_end) = (0.0, 0.90);
    if current_rate > blur_start && current_rate < blur_end {
        let pre_rate = (1.0 - (lt + 1.0) / ext).clamp(0.0, 1.0);
        if pre_rate < current_rate {
            const ACCESS: i32 = 20;
            for _loop_bk in 0..2 {
                let start_rate = (1.0 - (lt + 2.0) / ext).clamp(0.0, 1.0);
                let end_rate = (1.0 - (lt + 1.0) / ext).clamp(0.0, 1.0);
                let unit = (end_rate - start_rate) / ACCESS as f32;
                let mut rate_unit = start_rate;
                // Uninitialized in the original if no segment matches; the effect's state is used.
                let mut blur_pos = e.position;
                for _ in 0..ACCESS {
                    rate_unit += unit;
                    if let Some(p) = deasuler_pos(e, rate_unit).filter(|_| filled) {
                        blur_pos = p;
                    }
                    // TODO(original): b->AnimationTransformOnlySelf(arrEachBonePos, angle at
                    // rate_unit + HeadAngle, blur_pos, Scale) and CreateObjectBlur(o, bone 4/5,
                    // bone 2/1, (1,1,1), 10, false, 113/114, 20): needs the effect model's bones and
                    // object blurs, which the engine does not have.
                    if ctx.rand() % 10 == 0 {
                        // `arrEachBonePos[1]`. TODO(original): the effect model's bone 1; the
                        // interpolated model origin is used instead.
                        let position = blur_pos;
                        let rand_distance = rf(ctx, 100) + 100.0;
                        let random_dir = Vec3::new(0.0, rand_distance, 0.0);
                        // `o->Light` holds the target position here, as in the original.
                        ctx.create_particle(BITMAP_FIRE + 2, position, e.angle, e.light, 17, 1.35, None);
                        let ax = rf(ctx, 360);
                        let az = rf(ctx, 360);
                        let angle = Vec3::new(ax, 0.0, az);
                        let result = position + rotate(angle, random_dir);
                        ctx.create_joint_ex(BITMAP_JOINT_THUNDER, result, position, angle, 3, None, 10.0, 10, 10);
                    }
                }
            }
        }
    }
    move_tail(e, ctx);
}

/// ZzzObject.cpp:528-533: mesh 0 textured with `BodyLight(o,b)`, mesh 1 `RENDER_BRIGHT` with
/// (0.3, 0.4, 1). `RenderMesh` ignores `HiddenMesh`. Meshes are assumed to use texture = mesh index.
fn deasuler_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    let Some(base) = object_draw(e) else { return };
    let mut mesh0 = base;
    mesh0.hidden_mesh = 1;
    // `o->Light` is the target position: `glColor` clamps the body light to [0, 1].
    mesh0.light = e.light.clamp(Vec3::ZERO, Vec3::ONE);
    out.models.push(mesh0);
    let mut mesh1 = base;
    mesh1.hidden_mesh = 0;
    mesh1.light = Vec3::new(0.3, 0.4, 1.0);
    mesh1.bright = true;
    out.models.push(mesh1);
}

// --- MODEL_DEATH_SPI_SKILL / MODEL_PIER_PART (homing spirits) -----------------------------------------

/// ZzzEffect.cpp:3576.
fn death_spi_create(e: &mut Effect, ctx: &mut Ctx) {
    if e.sub_type == 0 {
        let (angle, light) = (e.angle, e.light);
        e.life_time = 30.0;
        e.gravity = 1.0;
        e.velocity = 10.0;
        e.hidden_mesh = 1;
        e.scale = 0.3;
        e.blend_mesh = -1;
        e.alpha = 0.8;
        e.light = Vec3::ONE;
        e.direction = Vec3::new(0.0, -26.0, 0.0);
        // `Light` carries the offset from the owner to fly to.
        e.start_position = light;
        e.head_angle = e.angle;
        e.angle = Vec3::new(0.0, 0.0, angle.z);
    } else if e.sub_type == 1 {
        e.life_time = owner_life_time(e);
        e.hidden_mesh = 1;
        e.scale = 0.2;
        e.blend_mesh = -1;
        e.alpha = (20 - life(e)) as f32 / 5.0;
        e.light = Vec3::new(0.3, 0.4, 1.0);
        e.direction = Vec3::ZERO;
        ctx.create_particle(BITMAP_FIRE + 2, e.position, e.angle, e.light, 0, 1.0, None);
    }
}

/// The homing loop shared by `MODEL_DEATH_SPI_SKILL` (ZzzEffect.cpp:13226-13268) and
/// `MODEL_PIER_PART` (13279-13320) subtype 0.
fn homing_step(e: &mut Effect, ctx: &mut Ctx) {
    let Some(owner_pos) = ctx.target_position(e.owner) else { return };
    let p = owner_pos + e.start_position;
    // MoveEffect's local `Position`: uninitialized on the first pass of the loop (ZERO here), then
    // the rotated `Direction` (an offset, not a world position: the original's own behavior).
    let mut position = Vec3::ZERO;
    let mut i = 1;
    while (i as f32) < e.gravity {
        if ctx.rand() % 2 == 0 {
            if e.angle.x < -90.0 {
                e.angle.x += 20.0;
            } else {
                e.angle.x -= 20.0;
            }
        }
        let _ = move_humming(e.position, &mut e.angle, p, e.velocity);
        e.velocity += 0.4;
        if e.life_time < 10.0 {
            e.velocity += 0.1;
            if e.ty == MODEL_DEATH_SPI_SKILL {
                ctx.create_effect(BITMAP_MAGIC + 1, position, e.angle, e.light, 1, me(e), -1, 0, 0, 0.0);
            }
        }
        position = rotate(e.angle, e.direction);
        e.position += position;
        if e.ty == MODEL_DEATH_SPI_SKILL {
            ctx.create_effect(MODEL_TAIL, e.position, e.angle, e.light, 0, me(e), -1, 0, 0, 0.0);
        } else {
            create_child_with_owner_life(ctx, e, MODEL_PIER_PART, 1);
        }
        i += 1;
    }
    // `if (Distance < 40 && LifeTime == 5)` only writes a local `Position`: no effect.
    e.gravity += 0.1;
    ctx.play_sound("SOUND_ATTACK_FIRE_BUST_EXP");
}

/// ZzzEffect.cpp:13224. The common tail does nothing for this type (18067).
fn death_spi_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    if e.sub_type == 0 {
        homing_step(e, ctx);
    } else if e.sub_type == 1 && e.life_time < 5.0 {
        e.alpha /= 1.3;
    }
}

/// ZzzEffect.cpp:3607.
fn pier_part_create(e: &mut Effect, ctx: &mut Ctx) {
    let (angle, light) = (e.angle, e.light);
    match e.sub_type {
        0 => {
            e.life_time = 20.0;
            e.gravity = 2.0;
            e.velocity = 10.0;
            e.hidden_mesh = 1;
            e.scale = 1.2;
            e.light = Vec3::ONE;
            e.direction = Vec3::new(0.0, -26.0, 0.0);
            e.start_position = light;
            e.head_angle = e.angle;
            e.angle = Vec3::new(0.0, 0.0, angle.z);
        }
        1 => {
            e.life_time = owner_life_time(e);
            e.hidden_mesh = 0;
            e.scale = 0.5;
            e.alpha = (20 - life(e)) as f32 / 5.0;
            e.direction = Vec3::ZERO;
            ctx.create_particle(BITMAP_FIRE + 1, e.position, e.angle, e.light, 0, 1.0, me(e));
        }
        2 => {
            e.life_time = 20.0;
            e.velocity = 50.0;
            e.hidden_mesh = -2;
            e.position.z -= 20.0;
            e.direction = Vec3::new(0.0, -40.0, 0.0);
            e.head_angle = e.angle;
            e.angle = Vec3::new(0.0, 0.0, angle.z);
        }
        _ => {}
    }
}

/// ZzzEffect.cpp:13278. The common tail does nothing for this type (18066). Drawn with
/// `RenderObject` (RenderEffects 18572).
fn pier_part_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    match e.sub_type {
        0 => homing_step(e, ctx),
        2 => {
            if let Some(p) = ctx.target_position(e.owner) {
                let _ = move_humming(e.position, &mut e.angle, p, e.velocity);
                e.velocity += 2.4;
                e.position += rotate(e.angle, e.direction);
                if life(e) % 3 == 0 {
                    let angle = Vec3::new(-90.0, 0.0, e.angle.z);
                    ctx.create_joint(BITMAP_JOINT_FORCE, e.position, e.position, angle, 2, None, 150.0);
                }
            }
        }
        1 if e.life_time < 5.0 => e.alpha /= 1.3,
        _ => {}
    }
}

// --- MODEL_GROUND_STONE / MODEL_GROUND_STONE2 ----------------------------------------------------------

/// ZzzEffect.cpp:3706 / 3727: only on walkable ground.
fn ground_stone_create(e: &mut Effect, ctx: &mut Ctx) {
    let wall = ctx.terrain_attribute(e.position.x, e.position.y);
    if (wall & TW_NOMOVE) != TW_NOMOVE && (wall & TW_NOGROUND) != TW_NOGROUND && (wall & TW_WATER) != TW_WATER {
        e.life_time = 40.0;
        let base = if e.ty == MODEL_GROUND_STONE { 1.2 } else { 1.0 };
        e.scale = base + rf(ctx, 30) / 100.0;
        e.velocity = 0.3;
        e.angle.z = rf(ctx, 360);
        // SetAction(o, 0): already action 0.
        e.current_action = 0;
    } else {
        e.live = false;
    }
}

/// ZzzEffect.cpp:13382-13404, then the common tail.
fn ground_stone_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    if e.life_time > 32.0 && e.life_time < 37.0 {
        let position = e.position + Vec3::new(60.0, -60.0, 50.0);
        let light = Vec3::new(1.0, 0.8, 0.6);
        ctx.create_particle(BITMAP_SMOKE, position, e.angle, light, 11, 2.0, me(e));
        let ty = MODEL_STONE1 + ctx.rand() % 2;
        ctx.create_effect(ty, position, e.angle, e.light, 10, None, -1, 0, 0, 0.0);
    }
    if e.life_time < 8.0 {
        e.alpha /= 1.3;
        e.blend_mesh_light /= 2.0;
    }
    ctx.add_terrain_light(e.position.x, e.position.y, Vec3::new(0.79, 0.72, 0.49), 2.0);
    move_tail(e, ctx);
}

// --- MODEL_SKILL_JAVELIN -----------------------------------------------------------------------------

/// ZzzEffect.cpp:4473.
fn javelin_create(e: &mut Effect, ctx: &mut Ctx) {
    // `o->Owner->Position`: a missing owner would crash the original; here the effect dies.
    let Some(owner_pos) = ctx.target_position(e.owner) else {
        e.live = false;
        return;
    };
    e.life_time = 35.0;
    e.gravity = 2.0;
    e.velocity = 10.0;
    e.scale = 1.2;
    e.direction = Vec3::new(0.0, -5.0, 0.0);
    e.head_angle = e.angle;
    e.start_position = owner_pos;
    e.position.z += 150.0;
    let ang = (ctx.rand() % 80 + 10) as f32;
    e.head_angle.z += e.sub_type as f32 * ang - ang;
}

/// ZzzEffect.cpp:14647-14712; the common tail only plays the animation (18109).
fn javelin_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    if let Some(p) = ctx.target_position(e.owner) {
        e.start_position = p;
        e.start_position.z += 150.0;
    }
    e.scale += 0.015;
    if e.life_time < 25.0 {
        if e.life_time < 20.0 {
            let distance = move_humming(e.position, &mut e.head_angle, e.start_position, e.velocity);
            if distance < 100.0 {
                e.position = e.start_position;
                e.scale += 0.04;
                if life(e) % 3 == 0 {
                    ctx.create_particle(BITMAP_POUNDING_BALL, e.position, e.angle, e.light, 1, 1.0, None);
                }
            } else {
                ctx.create_particle(BITMAP_POUNDING_BALL, e.position, e.angle, e.light, 1, 1.0, None);
            }
        }
        e.velocity += 1.5;
        e.position += rotate(e.head_angle, e.direction);
        e.gravity = rf(ctx, 30) + 30.0;
        if e.direction.y > -50.0 {
            e.direction.y -= 8.0;
        }
    } else {
        e.gravity += 10.0;
        e.position += rotate(e.head_angle, e.direction);
    }
    e.angle.z += e.gravity;
    e.blend_mesh_light = life(e) as f32 / 10.0;
    e.alpha = e.blend_mesh_light;

    let height = (life(e) as f32 * 0.1).sin() * 30.0;
    if e.sub_type == 1 {
        e.position.z = e.start_position.z + height;
    } else if e.sub_type == 2 {
        e.position.z = e.start_position.z - height;
    }
    ctx.add_terrain_light(e.position.x, e.position.y, Vec3::new(1.0, 0.6, 0.3), 2.0);
    play_animation(e, ctx);
}

/// ZzzObject.cpp:1595-1600: body light (1, 0.6, 0.3), textured.
/// TODO(original): second pass `RENDER_BRIGHT|RENDER_CHROME` with HiddenMesh 1 (chrome mapping).
fn javelin_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    let Some(mut d) = object_draw(e) else { return };
    d.light = Vec3::new(1.0, 0.6, 0.3);
    out.models.push(d);
}

// --- MODEL_SWORD_FORCE ---------------------------------------------------------------------------------

/// ZzzEffect.cpp:4772 (subtypes 2/3: PBG_ADD_NEWCHAR_MONK_SKILL).
fn sword_force_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.velocity = 0.25;
    if e.sub_type == 0 || e.sub_type == 2 {
        e.life_time = 15.0;
        e.scale = 0.0;
        e.position.z += 100.0;
        e.direction = Vec3::new(0.0, -10.0, 0.0);
    } else if e.sub_type == 1 || e.sub_type == 3 {
        e.life_time = 5.0;
        e.scale = 3.5;
    }
    e.blend_mesh = 0;
    e.blend_mesh_light = 1.0;
}

/// ZzzEffect.cpp:14767-14832, then the common tail: `PlayAnimation` twice (the generic one and
/// the type's own, 18111-18118) and `MoveParticle(o, true)`.
fn sword_force_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    if e.sub_type == 0 || e.sub_type == 2 {
        if e.life_time > 12.0 {
            e.scale += 0.9;
            e.direction.y -= 2.0;
            let sub = if e.sub_type == 2 { 3 } else { 1 };
            ctx.create_effect(MODEL_SWORD_FORCE, e.position, e.angle, e.light, sub, me(e), -1, 0, 0, 0.0);
        } else {
            e.scale -= 0.05;
            e.blend_mesh_light = life(e) as f32 / 18.0;
            e.alpha = e.blend_mesh_light;
            e.light = Vec3::splat(e.alpha);
            e.direction.y -= 2.0;

            let mut position = e.position;
            position.x += rf(ctx, 30) - 15.0;
            position.y += rf(ctx, 30) - 15.0;
            position.z -= 100.0;
            for _ in 0..4 {
                let angle = Vec3::new((ctx.rand() % 60 + 60 + 90) as f32, 0.0, e.angle.z);
                ctx.create_joint(BITMAP_JOINT_SPARK, position, position, angle, 0, None, 10.0);
                let sub = if e.sub_type == 2 { 18 } else { 2 };
                ctx.create_particle(BITMAP_FIRE, position, angle, e.light, sub, 1.5, None);
            }
        }
        ctx.add_terrain_light(e.position.x, e.position.y, Vec3::new(1.0, 0.8, 0.6), 1.0);
    } else if e.sub_type == 1 || e.sub_type == 3 {
        e.blend_mesh_light = life(e) as f32 / 10.0;
        e.alpha = e.blend_mesh_light;
        e.light = Vec3::splat(e.alpha);
    }
    // TODO(original): the second PlayAnimation runs even when CurrentAction == 255.
    play_animation(e, ctx);
    e.animation_frame += e.velocity;
    move_particle(e, true);
}

/// ZzzObject.cpp:2600-2610.
/// TODO(original): subtypes 2/3 render with the texture `BITMAP_KNIGHTST_BLUE` instead of the model's.
fn sword_force_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    draw_plain(e, ctx, out);
}

// --- MODEL_STUN_STONE ----------------------------------------------------------------------------------

/// `user` slot: `o->ExtState`.
const SLOT_EXT: usize = 0;

/// ZzzEffect.cpp:4395.
fn stun_stone_create(e: &mut Effect, ctx: &mut Ctx) {
    let angle = e.angle;
    if e.sub_type == 0 {
        let px = rf(ctx, 256) / 64.0 - 2.0;
        let py = -((ctx.rand() % 200 + 64) as f32) * 0.1;
        let p1 = Vec3::new(px, py, 0.0);
        e.life_time = 40.0;
        e.scale = 1.1 + rf(ctx, 100) / 100.0;
        e.gravity = -((ctx.rand() % 16 + 10) as f32);
        e.user[SLOT_EXT] = 0.0;
        e.position.z += 600.0;
        e.start_position = rotate(angle, p1);
        e.light = Vec3::ONE;
        e.angle = Vec3::new(-20.0, 0.0, rf(ctx, 360));
        e.head_angle = Vec3::new(0.0, 0.0, angle.z);
    } else if e.sub_type == 1 {
        e.life_time = 22.0;
        e.hidden_mesh = -2;
        e.direction = Vec3::new(0.0, -25.0, 0.0);
        e.light = Vec3::splat(-1.0);
    }
}

/// ZzzEffect.cpp:14536-14577, then the common tail.
fn stun_stone_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    if e.sub_type == 0 {
        e.position.z += e.gravity;
        e.gravity -= 15.0;
        let height = ctx.terrain_height(e.position.x, e.position.y) + 50.0;
        if e.position.z <= height {
            e.position.z = height;
            if e.user[SLOT_EXT] == 0.0 {
                ctx.create_effect(BITMAP_CRATER, e.position, e.angle, e.light, 1, None, -1, 0, 0, 0.0);
                e.user[SLOT_EXT] = 1.0;
            }
        } else {
            e.position += e.start_position;
            e.start_position *= 0.9;
            e.head_angle.x -= e.scale * 32.0;
            if life(e) % 3 == 0 {
                ctx.create_particle(BITMAP_ADV_SMOKE + 1, e.position, e.angle, e.light, 1, 1.0, None);
            }
        }
        e.light = Vec3::splat(life(e) as f32 / 10.0);
        e.alpha = e.light.x;
    } else if e.sub_type == 1 {
        if life(e) % 3 == 0 {
            ctx.create_effect(MODEL_STUN_STONE, e.position, e.angle, e.light, 0, None, -1, 0, 0, 0.0);
        }
        ctx.add_terrain_light(e.position.x, e.position.y, e.light, 1.0);
    }
    move_tail(e, ctx);
}

/// ZzzObject.cpp:746-752: textured with `BodyLight(o,b)`.
/// TODO(original): second pass `RENDER_CHROME` (alpha 0.5, body light 1, `BITMAP_CHROME`, UV ×5/×2);
/// the spin is in `HeadAngle`, which `ModelDraw` does not apply.
fn stun_stone_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    draw_plain(e, ctx, out);
}

// --- MODEL_SKIN_SHELL ----------------------------------------------------------------------------------

/// ZzzEffect.cpp:4424.
fn skin_shell_create(e: &mut Effect, ctx: &mut Ctx) {
    let p1 = Vec3::new(0.0, (ctx.rand() % 128 + 32) as f32 * 0.1, 0.0);
    e.position.z += 50.0;
    e.life_time = (ctx.rand() % 16 + 32) as f32;
    e.scale = (ctx.rand() % 3 + 3) as f32 * 0.1;
    e.alpha = 0.5;
    e.angle.z = rf(ctx, 360);
    e.direction = rotate(e.angle, p1);
    e.gravity = (ctx.rand() % 5 + 2) as f32;
    e.gravity += rf(ctx, 5);
    e.angle.x = rf(ctx, 360);
    e.angle.y = rf(ctx, 360);
    e.angle.z = rf(ctx, 360);
    if e.sub_type == 0 {
        e.light = Vec3::splat(0.5);
    } else if e.sub_type == 1 {
        e.light = Vec3::new(0.1, 0.6, 1.0);
    }
}

/// ZzzEffect.cpp:14579-14599, then the common tail.
fn skin_shell_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    e.position += e.direction;
    e.direction *= 0.9;
    e.position.z += e.gravity;
    e.direction *= 1.1;
    e.gravity -= 3.0;
    let height = ctx.terrain_height(e.position.x, e.position.y);
    if e.position.z < height {
        e.position.z = height;
        e.gravity = -e.gravity * 0.2;
        e.life_time -= 5.0;
        e.angle.x -= e.scale * 128.0;
    } else {
        e.angle.x -= e.scale * 16.0;
    }
    e.alpha = life(e) as f32 / 10.0;
    move_tail(e, ctx);
}

/// ZzzObject.cpp:733-745: subtype 0 with `BodyLight = o->Light`, subtype 1 `RENDER_BRIGHT` with
/// (0.1, 0.5, 1); other subtypes draw nothing.
/// TODO(original): both render with the texture `BITMAP_CHROME` instead of the model's.
fn skin_shell_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    let Some(mut d) = object_draw(e) else { return };
    match e.sub_type {
        0 => out.models.push(d),
        1 => {
            d.light = Vec3::new(0.1, 0.5, 1.0);
            d.bright = true;
            out.models.push(d);
        }
        _ => {}
    }
}

// --- MODEL_MANA_RUNE -----------------------------------------------------------------------------------

/// ZzzEffect.cpp:4450.
fn mana_rune_create(e: &mut Effect, _ctx: &mut Ctx) {
    if e.sub_type == 0 {
        e.life_time = 50.0;
        e.scale = 0.0;
        e.gravity = 0.1;
        e.alpha = 0.3;
        e.hidden_mesh = 0;
        e.position.z += 300.0;
        e.angle = Vec3::new(0.0, 0.0, 45.0);
    } else if e.sub_type == 1 {
        e.life_time = 10.0;
        e.scale = 1.1;
        e.gravity = 0.1;
        e.hidden_mesh = -1;
        e.blend_mesh = -2;
        e.blend_mesh_light = 0.4;
    }
}

/// ZzzEffect.cpp:14601-14645, then the common tail.
fn mana_rune_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    if e.sub_type == 0 {
        if e.life_time > 43.0 {
            e.hidden_mesh = -2;
        } else if e.life_time > 40.0 {
            e.hidden_mesh = 0;
        } else if e.life_time > 30.0 {
            e.scale += e.gravity;
            e.gravity += 0.15;
            if e.scale > 1.0 {
                e.scale = 1.0;
                e.gravity = 0.01;
                ctx.create_effect(MODEL_MANA_RUNE, e.position, e.angle, e.light, 1, None, -1, 0, 0, 0.0);
            }
        } else if e.life_time < 15.0 {
            e.scale -= e.gravity;
            e.gravity += 0.2;
            if e.scale < 0.0 {
                e.scale = 0.0;
            }
            e.alpha = life(e) as f32 / 20.0;
            e.position.z -= 10.0;
        }
    } else if e.sub_type == 1 {
        e.scale -= 0.02;
        if e.scale < 1.0 {
            e.scale = 1.0;
        }
        e.alpha = life(e) as f32 / 20.0;
    }
    move_tail(e, ctx);
}

/// ZzzObject.cpp:1589-1594: subtype 0 textured with body light (0.3, 0.6, 1); the rest generic.
/// TODO(original): subtype 0's second pass `RENDER_BRIGHT|RENDER_CHROME` (chrome mapping).
fn mana_rune_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    let Some(mut d) = object_draw(e) else { return };
    if e.sub_type == 0 {
        d.light = Vec3::new(0.3, 0.6, 1.0);
    }
    out.models.push(d);
}

// --- MODEL_AIR_FORCE -----------------------------------------------------------------------------------

/// ZzzEffect.cpp:3402.
fn air_force_create(e: &mut Effect, _ctx: &mut Ctx) {
    if e.sub_type == 0 {
        e.life_time = 15.0;
        e.blend_mesh_light = 1.0;
        e.scale = 0.6;
    } else if e.sub_type == 1 {
        e.life_time = 20.0;
        e.blend_mesh_light = 1.0;
        e.scale = 1.2;
    }
}

/// ZzzEffect.cpp:13030-13054, then the common tail. Drawn with `RenderObject` (18571).
/// `TerminateOwnerEffectObject` (ZzzEffect.cpp:252) clears the owner when it goes away: here a
/// vanished owner counts as NULL.
fn air_force_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    let Some((owner_pos, owner_angle)) = owner_pose(ctx, e.owner) else {
        e.live = false;
        return;
    };
    if e.sub_type == 0 {
        e.blend_mesh_light /= 1.5;
        e.scale += 0.2;
    }
    if e.sub_type == 1 {
        e.blend_mesh_light = life(e) as f32 / 10.0;
        e.position = owner_pos;
        e.angle = owner_angle;
        let mut position = rotate(e.angle, Vec3::new(0.0, -70.0, 0.0));
        position.z += 100.0;
        e.position += position;
    }
    move_tail(e, ctx);
}

// --- MODEL_WAVES / MODEL_COMBO -------------------------------------------------------------------------

/// The start shared by `MODEL_COMBO` (ZzzEffect.cpp:3385) and `MODEL_WAVES` (3416).
fn waves_common_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = 20.0;
    e.gravity = 0.1;
    e.blend_mesh = -2;
    e.blend_mesh_light = 1.0;
    e.angle = Vec3::ZERO;
    e.start_position = e.position;
    e.position.z += 50.0;
    if e.sub_type == 0 {
        for _ in 0..60 {
            let scale = (ctx.rand() % 40 + 70) as f32;
            ctx.create_joint(BITMAP_LIGHT, e.position, e.position, e.angle, 0, None, scale);
        }
    }
}

fn combo_create(e: &mut Effect, ctx: &mut Ctx) {
    waves_common_create(e, ctx);
}

/// ZzzEffect.cpp:12962, then the common tail. ZzzObject.cpp:1510 (subtype 1 adds `o->RenderType`,
/// never set for this type): generic draw.
fn combo_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    if e.sub_type == 0 {
        if e.life_time > 4.0 {
            e.scale += e.gravity;
            e.gravity += 0.1;
        }
        e.blend_mesh_light /= 1.4;
    }
    move_tail(e, ctx);
}

/// ZzzEffect.cpp:3416-3473. `PKKey` carries the scale for subtypes 2-6.
fn waves_create(e: &mut Effect, ctx: &mut Ctx) {
    let angle_z = e.angle.z;
    let pk_key = e.pk_key;
    waves_common_create(e, ctx);
    match e.sub_type {
        1 => {
            e.life_time = 15.0;
            // o->RenderType = RENDER_NODEPTH: not used by MODEL_WAVES' RenderBody (ZzzObject.cpp:1567).
            e.position.z += 80.0;
            e.scale = 0.1 + rf(ctx, 50) / 100.0;
            e.gravity = 0.01;
            e.angle.x = 90.0;
            e.angle.z = angle_z;
            for _ in 0..2 {
                let scale = (ctx.rand() % 40 + 70) as f32;
                ctx.create_joint(BITMAP_PIERCING, e.position, e.position, e.angle, 0, None, scale);
            }
            e.position.x += rf(ctx, 100) - 50.0;
            e.position.y += rf(ctx, 100) - 50.0;
        }
        2..=4 => {
            e.life_time = 15.0;
            e.pk_key = -1;
            e.scale = pk_key as f32 * 0.05;
            e.gravity = 0.01;
            e.position.z -= 50.0;
            e.angle.x = 90.0;
            e.angle.z = angle_z;
        }
        5 | 6 => {
            e.life_time = 10.0;
            e.pk_key = -1;
            e.scale = pk_key as f32 * 0.05;
            e.gravity = 0.08;
            e.position.z -= 50.0;
            e.angle.x = -90.0;
            e.angle.z = angle_z;
        }
        _ => {}
    }
}

/// ZzzEffect.cpp:12973-13029, then the common tail.
fn waves_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    // Subtypes 1-6: (gravity step, maximum scale, BlendMeshLight divisor, spins).
    let params = match e.sub_type {
        1 => Some((0.07, 2.0, 1.5, false)),
        2 => Some((0.01, 1.5, 1.5, false)),
        3 => Some((0.005, 1.5, 1.3, true)),
        4 => Some((0.002, 2.5, 1.2, true)),
        5 | 6 => Some((0.015, 2.5, 1.3, true)),
        _ => None,
    };
    if e.sub_type == 0 {
        if e.life_time > 4.0 {
            e.scale += e.gravity;
            e.gravity += 0.1;
        }
        e.blend_mesh_light /= 1.4;
    } else if let Some((gravity_step, max_scale, divisor, spins)) = params {
        e.scale += e.gravity;
        e.gravity += gravity_step;
        if e.scale > max_scale {
            e.scale = max_scale;
        }
        e.blend_mesh_light /= divisor;
        if spins {
            e.angle.y += 45.0;
        }
    }
    move_tail(e, ctx);
}

/// ZzzObject.cpp:1567-1575: subtypes 3-5 render with HiddenMesh -1.
/// TODO(original): subtypes 3-5 use the texture `BITMAP_PINK_WAVE` instead of the model's.
fn waves_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    let Some(mut d) = object_draw(e) else { return };
    if matches!(e.sub_type, 3..=5) {
        d.hidden_mesh = -1;
    }
    out.models.push(d);
}

// --- MODEL_FALL_STONE_EFFECT ---------------------------------------------------------------------------

/// ZzzEffect.cpp:4626. `o->Scale` is the `Scale` argument (0.9 if not passed).
fn fall_stone_create(e: &mut Effect, ctx: &mut Ctx) {
    match e.sub_type {
        0 | 1 => {
            e.life_time = (ctx.rand() % 5 + 100) as f32;
            if e.sub_type == 0 {
                e.scale = (ctx.rand() % 20 + 5) as f32 * 0.005 + e.scale * 0.05;
            } else {
                e.scale = (ctx.rand() % 10 + 5) as f32 * 0.02 + e.scale * 0.05;
            }
            e.angle.x = rf(ctx, 360);
            e.angle.y = rf(ctx, 360);
            e.angle.z = rf(ctx, 360);
            e.position.x += rf(ctx, 100) - 50.0;
            e.position.y += rf(ctx, 100) - 50.0;
            e.gravity = (ctx.rand() % 10 + 10) as f32 * 0.5;
        }
        2 => {
            e.life_time = 100.0;
            e.scale += (ctx.rand() % 20 + 5) as f32 * 0.05;
            e.angle.x = rf(ctx, 360);
            e.angle.y = rf(ctx, 360);
            e.angle.z = rf(ctx, 360);
            e.gravity = (ctx.rand() % 20 + 40) as f32 * 0.5 + e.scale * 2.0;
        }
        3 => {
            e.life_time = (ctx.rand() % 5 + 15) as f32;
            // o->Scale = Scale (already).
            e.gravity = (ctx.rand() % 3 + 3) as f32;
            e.angle.z = rf(ctx, 360);
            let p1 = Vec3::new(0.0, (ctx.rand() % 60 + 30) as f32 * 0.1, 0.0);
            e.head_angle = rotate(e.angle, p1);
            e.head_angle.z += 25.0;
            e.direction = Vec3::ZERO;
        }
        _ => {}
    }
}

/// ZzzEffect.cpp:14343-14403, then the common tail. Drawn with `RenderObject` (18601).
fn fall_stone_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    match e.sub_type {
        0 | 1 => {
            e.position.z -= e.gravity;
            e.gravity += 0.1;
            e.angle += Vec3::splat(0.5);
        }
        2 => {
            e.position.z -= e.gravity;
            e.gravity += 0.1;
            let height = ctx.terrain_height(e.position.x, e.position.y);
            if e.position.z < height {
                let light = Vec3::splat(0.5);
                let scale = (ctx.rand() % 20 + 30) as f32 * 0.020;
                ctx.create_particle(BITMAP_SMOKE, e.position, e.angle, light, 11, scale, None);
                let n = ctx.rand() % 2 + 2;
                for _ in 0..n {
                    let scale = 0.03 + (ctx.rand() % 10) as f32 / 40.0 + e.scale * 0.3;
                    ctx.create_effect(MODEL_FALL_STONE_EFFECT, e.position, e.angle, e.light, 3, None, -1, 0, 0, scale);
                }
                e.live = false;
            }
        }
        3 => {
            // `HeadAngle` is the velocity here.
            e.head_angle.z -= e.gravity;
            e.position += e.head_angle;
            let height = ctx.terrain_height(e.position.x, e.position.y);
            e.angle.x += 0.5 * life(e) as f32;
            e.angle.y += 0.5 * life(e) as f32;
            if e.position.z + e.direction.z <= height {
                e.position.z = height;
                e.head_angle.x *= 0.6;
                e.head_angle.y *= 0.6;
                e.head_angle.z += 1.0 * life(e) as f32;
                if e.head_angle.z < 0.5 {
                    e.head_angle.z = 0.0;
                }
                e.alpha -= 0.15;
                e.light /= 1.08;
            }
        }
        _ => {}
    }
    move_tail(e, ctx);
}

// --- MODEL_FENRIR_FOOT_THUNDER / MODEL_TWINTAIL_EFFECT (ground marks) ----------------------------------

/// `user` slots: `o->m_iAnimation` and `o->m_dwTime` (`timeGetTime()` → `WorldTime`, ms).
const SLOT_ANIMATION: usize = 0;
const SLOT_TIME: usize = 1;

/// ZzzEffect.cpp:4673.
fn foot_thunder_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = 200.0;
    e.user[SLOT_ANIMATION] = 0.0;
    e.alpha = 1.0;
    e.user[SLOT_TIME] = ctx.world_time();
    e.position.z = 0.0;
}

/// ZzzEffect.cpp:14404-14440, then the common tail.
fn foot_thunder_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    if e.live {
        e.angle.x += 0.1;
        match e.sub_type {
            1 => {
                e.light.y -= 0.05;
                e.light.z -= 0.05;
            }
            2 => {
                e.light.x -= 0.05;
                e.light.y -= 0.05;
            }
            3 => {
                e.light.x -= 0.05;
                e.light.z -= 0.05;
            }
            4 => e.light.z -= 0.05,
            _ => {}
        }
        e.alpha -= 0.05;
        if e.alpha <= 0.0 {
            e.live = false;
        }
        if ctx.world_time() - e.user[SLOT_TIME] > 200.0 {
            e.user[SLOT_ANIMATION] += 1.0;
            if e.user[SLOT_ANIMATION] > 4.0 {
                e.live = false;
            }
            e.user[SLOT_TIME] = ctx.world_time();
        }
    }
    move_tail(e, ctx);
}

/// RenderEffects 18604-18610: `RenderTerrainAlphaBitmap(BITMAP_FENRIR_FOOT_THUNDER1 + anim%5, x, y,
/// 0.6, 0.6, Light)` with `EnableAlphaBlend`.
fn foot_thunder_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    let frame = (e.user[SLOT_ANIMATION] as i32).rem_euclid(5);
    common::terrain_bitmap(out, BITMAP_FENRIR_FOOT_THUNDER1 + frame, e.position.x, e.position.y, 0.6, 0.6, e.light, 0.0, 1.0, 5.0, Blend::Glow);
}

/// ZzzEffect.cpp:4682.
fn twintail_create(e: &mut Effect, ctx: &mut Ctx) {
    if e.sub_type == 0 {
        e.life_time = 200.0;
        e.alpha = 1.0;
        e.position.z = 0.0;
        e.user[SLOT_TIME] = ctx.world_time();
        e.user[SLOT_ANIMATION] = 0.0;
    } else if e.sub_type == 1 || e.sub_type == 2 {
        e.life_time = 50.0;
        e.scale = 3.5;
        e.alpha = 1.0;
        e.position.z = 0.0;
        e.angle.x = 0.0;
    }
}

/// ZzzEffect.cpp:14441-14506, then the common tail.
fn twintail_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    if e.sub_type == 0 {
        e.alpha -= 0.01;
        if e.alpha <= 0.0 {
            e.live = false;
        }
        e.light /= 1.02;
        if ctx.world_time() - e.user[SLOT_TIME] > 1000.0 {
            e.user[SLOT_ANIMATION] = if e.user[SLOT_ANIMATION] == 0.0 { 1.0 } else { 0.0 };
            e.user[SLOT_TIME] = ctx.world_time();
        }
        if e.user[SLOT_ANIMATION] == 0.0 {
            e.scale -= 0.015;
            if e.scale <= 0.0 {
                e.scale = 0.0;
            }
        } else {
            e.scale += 0.02;
            if e.scale >= 1.2 {
                e.scale = 1.2;
            }
        }
    } else if e.sub_type == 1 || e.sub_type == 2 {
        e.alpha -= 0.01;
        if e.alpha <= 0.0 {
            e.live = false;
        }
        e.light /= 1.04;
        if e.sub_type == 1 {
            e.angle.x = -(ctx.world_time() * 0.3);
        } else {
            e.angle.x = -(ctx.world_time() * 0.1);
        }
        e.scale -= 0.02;
    }
    move_tail(e, ctx);
}

/// RenderEffects 18611-18626 (`EnableAlphaBlend`).
fn twintail_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    if e.sub_type == 0 {
        common::terrain_bitmap(out, BITMAP_SPARK + 1, e.position.x, e.position.y, e.scale, e.scale, e.light, 0.0, e.alpha, 5.0, Blend::Glow);
    } else if e.sub_type == 1 || e.sub_type == 2 {
        common::terrain_bitmap(out, BITMAP_CLOUD, e.position.x, e.position.y, e.scale, e.scale, e.light, e.angle.x, e.alpha, 5.0, Blend::Glow);
    }
}

// --- MODEL_FENRIR_SKILL_DAMAGE -------------------------------------------------------------------------

/// ZzzEffect.cpp:4706: energy sparks on 10 random bones of the owner. `LifeTime` is not set (it
/// keeps the slot's old value, normally ≤ 0): the effect dies on its first tick.
fn skill_damage_create(e: &mut Effect, ctx: &mut Ctx) {
    match e.sub_type {
        1 => e.light = Vec3::new(0.6, 0.2, 0.2),
        2 => e.light = Vec3::new(0.2, 0.2, 0.4),
        3 => e.light = Vec3::new(0.6, 0.8, 0.6),
        _ => {}
    }
    // `Models[o->Owner->Type]`: a missing owner (or a model without bones: `rand() % 0`) would
    // crash the original; here the effect dies.
    let Some(owner) = ctx.actor(e.owner).cloned() else {
        e.live = false;
        return;
    };
    let num_bones = owner.bones.len() as i32;
    if num_bones == 0 {
        e.live = false;
        return;
    }
    for _ in 0..10 {
        let bone = (ctx.rand() % num_bones) as usize;
        e.position = owner.bone_world(bone, Vec3::ZERO);
        ctx.create_particle(BITMAP_ENERGY, e.position, e.angle, e.light, 2, 1.0, None);
    }
}

/// No `MoveEffect` case: only the common tail.
fn skill_damage_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    move_tail(e, ctx);
}

// --- MODEL_PROTECTGUILD --------------------------------------------------------------------------------

/// `b->TransformPosition(o->Owner->BoneTransform[20], (0,0,0), out, true)`: the original adds
/// `b->BodyOrigin` and then corrects by `Owner->Position - Hero->Position` (x, y) — i.e. the bone
/// in world space around the owner, which [`Actor::bone_world`] gives.
/// TODO(original): the height uses `BodyOrigin` (the hero's z) instead of the owner's z.
fn guild_bone(owner: &Actor) -> Vec3 {
    owner.bone_world(20, Vec3::ZERO)
}

/// ZzzEffect.cpp:4798.
fn protect_guild_create(e: &mut Effect, ctx: &mut Ctx) {
    e.alpha = 0.0;
    e.angle.z = 45.0;
    // `Models[o->Owner->Type]`: a missing owner would crash the original; here the effect dies.
    let Some(owner) = ctx.actor(e.owner).cloned() else {
        e.live = false;
        return;
    };
    let temp = guild_bone(&owner);
    e.position = Vec3::new(temp.x, temp.y, temp.z + 60.0);
    e.life_time = 130.0;
    e.scale = 3.2;
    e.light = Vec3::ONE;
    e.start_position = e.position;
    for _ in 0..10 {
        let f_angle = rf(ctx, 360);
        let x = e.position.x + (ctx.rand() % 20 + 15) as f32 * f_angle.sin();
        let y = e.position.y + (ctx.rand() % 20 + 15) as f32 * f_angle.cos();
        let p = Vec3::new(x, y, e.position.z);
        ctx.create_particle(BITMAP_SPARK + 1, p, e.angle, e.light, 4, 0.6, me(e));
    }
}

/// ZzzEffect.cpp:14834-14888, then the common tail.
fn protect_guild_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    let Some(owner) = ctx.actor(e.owner).cloned() else {
        // `LifeTime = 0; break;`: the tail still runs and the effect dies.
        e.life_time = 0.0;
        move_tail(e, ctx);
        return;
    };
    if e.alpha >= 1.0 {
        e.angle.z += 5.0;
    }
    if e.life_time > 120.0 {
        if e.life_time < 125.0 && e.life_time > 120.0 {
            for _ in 0..5 {
                ctx.create_particle(BITMAP_SPARK, e.position, e.angle, e.light, 6, 1.5, me(e));
            }
        }
    } else if e.life_time > 95.0 {
        if e.alpha > 1.0 {
            e.alpha = 1.0;
        } else {
            e.alpha += 0.4;
        }
    } else if e.life_time < 50.0 {
        if e.alpha < 0.0 {
            e.alpha = 0.0;
        } else {
            e.alpha -= 0.1;
        }
    }

    let temp = guild_bone(&owner);
    e.position.x = temp.x;
    e.position.y = temp.y;
    let height = temp.z + 60.0;
    if height < e.position.z {
        e.position.z += (height - e.position.z) * 0.1;
    } else {
        e.position.z += (height - e.position.z) * 0.5;
    }

    if e.life_time < 60.0 && e.life_time > 39.0 {
        let f_angle = rf(ctx, 360);
        let x = e.position.x + (ctx.rand() % 26 - 13) as f32 * f_angle.sin();
        let y = e.position.y + (ctx.rand() % 26 - 13) as f32 * f_angle.cos();
        let z = e.position.z + 48.0 - life(e) as f32 + rf(ctx, 5);
        ctx.create_particle(BITMAP_SPARK, Vec3::new(x, y, z), e.angle, e.light, 5, 2.0, me(e));
    }
    move_tail(e, ctx);
}

/// ZzzObject.cpp:1576-1581: two `RENDER_BRIGHT|RENDER_CHROME` passes with body light (0.4, 0.6, 1).
/// TODO(original): `RENDER_CHROME` replaces the texture with environment-mapped `BITMAP_CHROME`;
/// the model's own texture is used for both additive passes.
fn protect_guild_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    let Some(mut d) = object_draw(e) else { return };
    d.light = Vec3::new(0.4, 0.6, 1.0);
    d.bright = true;
    out.models.push(d);
    out.models.push(d);
}
