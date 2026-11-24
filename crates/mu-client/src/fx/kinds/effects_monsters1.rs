//! Monster/boss effects (1): the Raklion boss crack and magic circle
//! (`MODEL_RAKLION_BOSS_CRACKEFFECT`, `MODEL_RAKLION_BOSS_MAGIC`), Kanturu Maya's storm, star, falling
//! stones and hand skill (`MODEL_STORM3`, `MODEL_MAYASTAR`, `MODEL_MAYASTONE1..5`,
//! `MODEL_MAYASTONEFIRE`, `MODEL_MAYAHANDSKILL`), the Kundun body parts (`MODEL_CUNDUN_PART1..8`,
//! `MODEL_MONSTER01+64`) and the "body part" debris of dying monsters: totem golem
//! (`MODEL_TOTEMGOLEM_PART1..6`), Condra/Narcondra (`MODEL_CONDRA_*`, `MODEL_NARCONDRA_*`), the
//! doppelganger slime chips (`MODEL_DOPPELGANGER_SLIME_CHIP`), the shadow pawn/knight/rook
//! (`MODEL_SHADOW_PAWN_*`, `MODEL_SHADOW_KNIGHT_*`, `MODEL_SHADOW_ROOK_ANKLE_*/BELT/CHEST`) and the
//! ice giant (`MODEL_ICE_GIANT_PART1..4`).
//!
//! Translated from Main 5.2 `ZzzEffect.cpp` (`CreateEffect`, `MoveEffect` + its common tail
//! 18060-18124, `RenderEffects`, `RenderAfterEffects` 19203, `RenderEffectShadows` 19249) and the
//! map renderers `RenderObject(o)` reaches for these types: `GM_Kanturu_3rd.cpp`
//! (`RenderKanturu3rdObjectMesh` 397, `RenderKanturu3rdAfterObjectMesh` 569) and `GMHellas.cpp`
//! (`RenderHellasMonsterObjectMesh` 2000, called for every map).
//!
//! Notes:
//! - The shadow rook HELMET/KNEE/WRIST parts and `MODEL_ICE_GIANT_PART5/6` share the debris case
//!   but belong to the `monsters2` batch: they are NOT registered here.
//! - Several types' original `CreateEffect` read the `Scale` argument after `o->Scale` was set from
//!   it; the engine only keeps `e.scale` (where an argument ≤ 0 already became 0.9). Every caller
//!   in Main 5.2 passes a positive scale, so `e.scale` is used as the argument.
//! - Function arguments that call `rand()` are evaluated left to right (like the other kind files).

use bevy::math::Vec3;

use super::common::{self, luminosity, move_tail, rf};
use crate::fx::core::*;

/// `WD_39KANTURU_3RD` (`MapManager.h:33`).
const WD_39KANTURU_3RD: i32 = 39;
/// `MONSTER01_DIE` (`_define.h:489`).
const MONSTER01_DIE: usize = 6;

pub fn register(r: &mut Registry) {
    let k = |create: EffectCreate, step: EffectStep, draw: EffectDraw| EffectKind { create: Some(create), step: Some(step), draw: Some(draw) };

    // Raklion boss (Selupan).
    r.effect(MODEL_RAKLION_BOSS_CRACKEFFECT, k(raklion_crack_create, raklion_crack_step, common::draw_object));
    r.effect(MODEL_RAKLION_BOSS_MAGIC, k(raklion_magic_create, raklion_magic_step, raklion_magic_draw));

    // Kanturu 3rd: Maya.
    r.effect(MODEL_STORM3, k(storm3_create, storm3_step, storm3_draw));
    r.effect(MODEL_MAYASTAR, k(mayastar_create, mayastar_step, mayastar_draw));
    for ty in [MODEL_MAYASTONE1, MODEL_MAYASTONE2, MODEL_MAYASTONE3] {
        r.effect(ty, k(mayastone_create, mayastone_step, mayastone_draw));
    }
    for ty in [MODEL_MAYASTONE4, MODEL_MAYASTONE5] {
        r.effect(ty, k(mayastone45_create, mayastone45_step, mayastone_draw));
    }
    r.effect(MODEL_MAYASTONEFIRE, k(mayastonefire_create, mayastonefire_step, mayastone_draw));
    r.effect(MODEL_MAYAHANDSKILL, k(mayahand_create, mayahand_step, mayahand_draw));

    // Kundun.
    for ty in [
        MODEL_CUNDUN_PART1,
        MODEL_CUNDUN_PART2,
        MODEL_CUNDUN_PART3,
        MODEL_CUNDUN_PART4,
        MODEL_CUNDUN_PART5,
        MODEL_CUNDUN_PART6,
        MODEL_CUNDUN_PART7,
        MODEL_CUNDUN_PART8,
        MODEL_MONSTER01 + 64,
    ] {
        r.effect(ty, k(cundun_create, cundun_step, cundun_draw));
    }

    // Body part debris.
    for ty in MODEL_TOTEMGOLEM_PART1..=MODEL_TOTEMGOLEM_PART6 {
        r.effect(ty, k(totem_create, part_step_08, common::draw_object));
    }
    for ty in [
        MODEL_CONDRA_ARM_L,
        MODEL_CONDRA_ARM_L2,
        MODEL_CONDRA_SHOULDER,
        MODEL_CONDRA_ARM_R,
        MODEL_CONDRA_ARM_R2,
        MODEL_CONDRA_CONE_L,
        MODEL_CONDRA_CONE_R,
        MODEL_CONDRA_PELVIS,
        MODEL_CONDRA_STOMACH,
        MODEL_CONDRA_NECK,
        MODEL_NARCONDRA_ARM_L,
        MODEL_NARCONDRA_ARM_L2,
        MODEL_NARCONDRA_SHOULDER_L,
        MODEL_NARCONDRA_SHOULDER_R,
        MODEL_NARCONDRA_ARM_R,
        MODEL_NARCONDRA_ARM_R2,
        MODEL_NARCONDRA_ARM_R3,
        MODEL_NARCONDRA_CONE_1,
        MODEL_NARCONDRA_CONE_2,
        MODEL_NARCONDRA_CONE_3,
        MODEL_NARCONDRA_CONE_4,
        MODEL_NARCONDRA_CONE_5,
        MODEL_NARCONDRA_CONE_6,
        MODEL_NARCONDRA_PELVIS,
        MODEL_NARCONDRA_STOMACH,
        MODEL_NARCONDRA_NECK,
    ] {
        // No `case` in RenderEffects: the `default:` (RenderObject inside the skill range).
        r.effect(ty, k(condra_create, part_step_08, common::draw_default));
    }
    r.effect(MODEL_DOPPELGANGER_SLIME_CHIP, k(slime_create, slime_step, common::draw_default));
    for ty in (MODEL_SHADOW_PAWN_ANKLE_LEFT..=MODEL_SHADOW_PAWN_WRIST_RIGHT).chain(MODEL_SHADOW_KNIGHT_ANKLE_LEFT..=MODEL_SHADOW_KNIGHT_WRIST_RIGHT) {
        r.effect(ty, k(shadow_create, part_step_05, common::draw_object));
    }
    // Only ANKLE_LEFT/RIGHT, BELT and CHEST: the other rook parts are in the monsters2 batch.
    for ty in [MODEL_SHADOW_ROOK_ANKLE_LEFT, MODEL_SHADOW_ROOK_ANKLE_RIGHT, MODEL_SHADOW_ROOK_BELT, MODEL_SHADOW_ROOK_CHEST] {
        r.effect(ty, k(rook_create, part_step_05, common::draw_object));
    }
    for ty in MODEL_ICE_GIANT_PART1..=MODEL_ICE_GIANT_PART4 {
        r.effect(ty, k(shadow_create, part_step_05, common::draw_object));
    }
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

/// `user[0]` of the Kanturu types: 1 while `WorldActive == WD_39KANTURU_3RD` (the draw functions
/// cannot see the map).
fn set_kanturu3(e: &mut Effect, ctx: &Ctx) {
    e.user[0] = if ctx.world_active == WD_39KANTURU_3RD { 1.0 } else { 0.0 };
}

fn in_kanturu3(e: &Effect) -> bool {
    e.user[0] > 0.5
}

/// `(int)WorldTime % 2000 * 0.001f` with the leading minus (`-(int)WorldTime%2000*0.001f`).
fn world_scroll(ctx: &DrawCtx) -> f32 {
    (-(ctx.world_time as i32) % 2000) as f32 * 0.001
}

// --- MODEL_RAKLION_BOSS_CRACKEFFECT -------------------------------------------------------------------

/// ZzzEffect.cpp:1251.
fn raklion_crack_create(e: &mut Effect, ctx: &mut Ctx) {
    if e.sub_type == 0 {
        e.life_time = 40.0;
        // `o->Scale = Scale + 1.f` (the only callers, GM_Raklion.cpp:601/676, pass 2.0).
        e.scale += 1.0;
        e.position.z += 30.0;
        e.angle.z = rf(ctx, 360);
    }
}

/// ZzzEffect.cpp:8864. Drawn with `RenderObject` (ZzzEffect.cpp:18717).
fn raklion_crack_step(e: &mut Effect, ctx: &mut Ctx) {
    let _luminosity = luminosity(e, ctx);
    if e.sub_type == 0 {
        e.alpha -= 0.03;
    }
    move_tail(e);
}

// --- MODEL_RAKLION_BOSS_MAGIC -----------------------------------------------------------------------

/// ZzzEffect.cpp:1260. `user[0]`: the `Luminosity` of `RenderEffectShadows`.
fn raklion_magic_create(e: &mut Effect, _ctx: &mut Ctx) {
    if e.sub_type == 0 {
        e.life_time = 35.0;
        // `o->Scale = Scale` (the only caller, GM_Raklion.cpp:529, passes 1.5).
    }
}

/// ZzzEffect.cpp:8870.
fn raklion_magic_step(e: &mut Effect, ctx: &mut Ctx) {
    let _luminosity = luminosity(e, ctx);
    if e.sub_type == 0 {
        e.alpha -= 0.03;
    }
    move_tail(e);
    // `RenderEffectShadows` (ZzzEffect.cpp:19268) draws `Luminosity = (rand()%4+8)*0.1f` every
    // frame; draw functions cannot call rand(), so it is drawn here once per tick.
    e.user[0] = (ctx.rand() % 4 + 8) as f32 * 0.1;
}

/// `RenderObject` (ZzzEffect.cpp:18722) and the ground light of `RenderEffectShadows`
/// (ZzzEffect.cpp:19455, `EnableAlphaBlend`).
fn raklion_magic_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    common::draw_object(e, ctx, out);
    if e.sub_type == 0 {
        let lum = e.user[0];
        let light = Vec3::new(lum * 0.4, lum * 0.4, lum * 1.0);
        common::terrain_bitmap(out, BITMAP_LIGHT, e.position.x, e.position.y, 15.0, 15.0, light, -e.angle.z, 1.0, 5.0, Blend::Glow);
        let light = Vec3::new(lum * 1.0, lum * 1.0, lum * 1.0);
        common::terrain_bitmap(out, BITMAP_LIGHT, e.position.x, e.position.y, 5.0, 5.0, light, -e.angle.z, 1.0, 5.0, Blend::Glow);
    }
}

// --- MODEL_STORM3 (Maya's tornado, mayatonedo.bmd) ------------------------------------------------

/// ZzzEffect.cpp:2283.
fn storm3_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = 50.0;
    e.blend_mesh = 0;
    e.angle.x = 90.0;
    e.scale = 3.5;
    // `VectorCopy(Hero->Object.Position, o->StartPosition)`. Without a hero the start position
    // stays the effect's own position.
    if let Some(p) = ctx.hero_actor().map(|h| h.position) {
        e.start_position = p;
    }
    set_kanturu3(e, ctx);
}

/// ZzzEffect.cpp:10739.
fn storm3_step(e: &mut Effect, ctx: &mut Ctx) {
    let _luminosity = luminosity(e, ctx);
    set_kanturu3(e, ctx);
    if life(e) <= 30 {
        e.light /= 1.1;
    }
    e.angle.y += 30.0;
    let angle = Vec3::new(90.0, 0.0, e.angle.z);
    for _ in 0..5 {
        if ctx.rand() % 2 == 0 {
            let x = e.start_position.x + (ctx.rand() % 2000) as f32 - 1000.0;
            let y = e.start_position.y + (ctx.rand() % 2000) as f32 - 1000.0;
            let pos1 = Vec3::new(x, y, e.start_position.z);
            // `Pos2[0] = Pos1[1] + ...` (sic: the original uses Pos1's Y for both); `Pos2[2]` is
            // left uninitialized by the original: Pos1's Z is used.
            let px = pos1.y + (ctx.rand() % 200) as f32 - 100.0;
            let py = pos1.y + (ctx.rand() % 200) as f32 - 100.0;
            let pos2 = Vec3::new(px, py, pos1.z);
            ctx.create_joint(BITMAP_JOINT_THUNDER, pos1, pos2, angle, 20, me(e), 20.0);
        }
    }
    move_tail(e);
}

/// `RenderEffects` only sets `m_bRenderAfterCharacter` (ZzzEffect.cpp:18322); `RenderAfterEffects`
/// (ZzzEffect.cpp:19239, only while `IsMayaScene()`) → `RenderKanturu3rdAfterObjectMesh`
/// (GM_Kanturu_3rd.cpp:641): `BodyLight = o->Light`, mesh 0 is the blend mesh (`BlendMesh = 0`,
/// additive with `BlendMeshLight`), meshes 1 and 2 `RENDER_BRIGHT`: the whole (3-mesh) model
/// additive.
/// TODO(original): `IsMayaScene()` is also false during the Kanturu states NIGHTMARE_BATTLE, TOWER
/// and END (`CKANTURUDirection.cpp:44`); the event state does not exist in mu-rs.
fn storm3_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    if !in_kanturu3(e) {
        return;
    }
    let mut d = ModelDraw::of_effect(e);
    d.bright = true;
    d.blend_mesh = -1;
    out.models.push(d);
}

// --- MODEL_MAYASTAR (arrowsre05.bmd) ----------------------------------------------------------------

/// ZzzEffect.cpp:2292.
fn mayastar_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = 50.0;
    e.scale = 50.0;
    set_kanturu3(e, ctx);
}

/// ZzzEffect.cpp:10770.
fn mayastar_step(e: &mut Effect, ctx: &mut Ctx) {
    let _luminosity = luminosity(e, ctx);
    set_kanturu3(e, ctx);
    if life(e) <= 20 {
        e.light /= 1.2;
    }
    e.scale += (ctx.world_time() * 0.005).sin() * 2.0;
    e.angle.y += 10.0;
    // `EarthQuake = (rand()%6-6)*0.5f` (an assignment; the engine keeps the strongest of the tick).
    let quake = (ctx.rand() % 6 - 6) as f32 * 0.5;
    ctx.set_earthquake(quake);
    move_tail(e);
}

/// `RenderObject` (ZzzEffect.cpp:18328). In Kanturu 3rd it goes to `RenderKanturu3rdObjectMesh`
/// (GM_Kanturu_3rd.cpp:531): `BodyLight = o->Light`, mesh 0 `RENDER_TEXTURE|RENDER_BRIGHT` (the
/// model has one mesh). Elsewhere the plain `RenderBody`.
fn mayastar_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    if !in_kanturu3(e) {
        common::draw_object(e, ctx, out);
        return;
    }
    let mut d = ModelDraw::of_effect(e);
    d.bright = true;
    out.models.push(d);
}

// --- MODEL_MAYASTONE1..3 (falling stones) -------------------------------------------------------------

/// ZzzEffect.cpp:2298.
fn mayastone_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = 40.0;
    e.scale = 6.0 + (ctx.rand() % 8 + 15) as f32 * 0.1;
    e.direction = Vec3::new(0.0, 0.0, -60.0);
    e.head_angle = Vec3::new(0.0, 30.0, 0.0);
    ctx.create_joint(BITMAP_SMOKE, e.position, e.position, e.head_angle, 2, me(e), 100.0);
    set_kanturu3(e, ctx);
}

/// ZzzEffect.cpp:10783.
fn mayastone_step(e: &mut Effect, ctx: &mut Ctx) {
    let _luminosity = luminosity(e, ctx);
    set_kanturu3(e, ctx);
    e.angle = e.head_angle;
    let height = ctx.terrain_height(e.position.x, e.position.y);
    if e.position.z < height {
        e.position.z = height;
        e.direction = Vec3::ZERO;
        let mut position = Vec3::new(e.position.x, e.position.y, e.position.z + 80.0);

        let mut light = Vec3::new(0.0, 0.6, 1.0);
        ctx.create_effect(MODEL_SKILL_INFERNO, e.position, Vec3::ZERO, light, 2, me(e), 30, 0, 0, 0.0);

        for _ in 0..15 {
            position.x = e.position.x + (ctx.rand() % 160 - 80) as f32;
            position.y = e.position.y + (ctx.rand() % 160 - 100) as f32;
            position.z = e.position.z + 50.0;
            light = Vec3::new(0.3, 0.5, 1.0);
            let scale = (ctx.rand() % 32 + 80) as f32 * 0.025;
            ctx.create_particle(BITMAP_SMOKE, position, e.angle, light, 11, scale, None);
        }

        for _ in 0..6 {
            let ty = MODEL_MAYASTONE4 + ctx.rand() % 2;
            ctx.create_effect(ty, e.position, e.angle, light, 0, None, -1, 0, 0, 0.0);
        }

        ctx.create_particle(BITMAP_EXPLOTION, position, e.angle, light, 0, 4.0, None);
        e.live = false;
    }
    move_tail(e);
}

/// `RenderObject` (ZzzEffect.cpp:18329-18334) for every Maya stone. In Kanturu 3rd it goes to
/// `RenderKanturu3rdObjectMesh` (GM_Kanturu_3rd.cpp:538-550): `MODEL_MAYASTONE1` draws mesh 1
/// with `RENDER_TEXTURE`, then (all of them) mesh 0 with `RENDER_TEXTURE|RENDER_BRIGHT`; no other
/// mesh is drawn (`MODEL_MAYASTONE4` has a second one). Elsewhere the plain `RenderBody`.
/// The meshes are selected with `hidden_mesh`, assuming mesh n uses texture n.
fn mayastone_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    if !in_kanturu3(e) {
        common::draw_object(e, ctx, out);
        return;
    }
    if e.ty == MODEL_MAYASTONE1 {
        let mut d = ModelDraw::of_effect(e);
        d.hidden_mesh = 0;
        out.models.push(d);
    }
    let mut d = ModelDraw::of_effect(e);
    d.bright = true;
    d.hidden_mesh = 1;
    out.models.push(d);
}

// --- MODEL_MAYASTONE4/5 (stone chips) ----------------------------------------------------------------

/// ZzzEffect.cpp:2310.
fn mayastone45_create(e: &mut Effect, ctx: &mut Ctx) {
    let p1 = Vec3::new(0.0, (ctx.rand() % 256 + 64) as f32 * 0.1, 0.0);
    e.life_time = (ctx.rand() % 16 + 32) as f32;
    e.scale = (ctx.rand() % 10) as f32 / 3.0 + 1.0;
    e.angle.z = rf(ctx, 360);
    e.direction = rotate(e.angle, p1);
    e.gravity = (ctx.rand() % 16 + 8) as f32;
    set_kanturu3(e, ctx);
}

/// ZzzEffect.cpp:10828.
fn mayastone45_step(e: &mut Effect, ctx: &mut Ctx) {
    let _luminosity = luminosity(e, ctx);
    set_kanturu3(e, ctx);
    e.position += e.direction;
    e.direction *= 0.9;
    e.position.z += e.gravity;
    e.gravity -= 3.0;
    let height = ctx.terrain_height(e.position.x, e.position.y);
    if e.position.z < height {
        e.position.z = height;
        e.gravity = -e.gravity * 0.5;
        e.life_time -= 4.0;
        e.angle.x -= e.scale * 128.0;
    } else {
        e.angle.x -= e.scale * 32.0;
    }
    if ctx.rand() % 10 == 0 {
        ctx.create_particle(BITMAP_FIRE, e.position, e.angle, e.light, 15, 1.0, None);
    }
    move_tail(e);
}

// --- MODEL_MAYASTONEFIRE (blue fire around a falling stone) -------------------------------------------

/// ZzzEffect.cpp:2320. The subtype is the stone type it goes with (GM_Kanturu_3rd.cpp:1565).
fn mayastonefire_create(e: &mut Effect, ctx: &mut Ctx) {
    // `o->Scale = Scale * k` (Scale = the argument, which `e.scale` holds).
    if e.sub_type == MODEL_MAYASTONE1 {
        e.scale *= 0.8;
    } else if e.sub_type == MODEL_MAYASTONE2 {
        e.scale *= 0.6;
    } else if e.sub_type == MODEL_MAYASTONE3 {
        e.scale *= 0.5;
    }
    e.life_time = 40.0;
    e.direction = Vec3::new(0.0, 0.0, -60.0);
    e.head_angle = Vec3::new(0.0, 30.0, 0.0);
    set_kanturu3(e, ctx);
}

/// ZzzEffect.cpp:10850.
fn mayastonefire_step(e: &mut Effect, ctx: &mut Ctx) {
    let _luminosity = luminosity(e, ctx);
    set_kanturu3(e, ctx);
    e.angle = e.head_angle;
    let height = ctx.terrain_height(e.position.x, e.position.y);
    if e.position.z < height {
        e.live = false;
    }
    move_tail(e);
}

// --- MODEL_MAYAHANDSKILL (hendlight02.bmd) -------------------------------------------------------------

/// ZzzEffect.cpp:2333. `StartPosition` keeps the creation light (the smoke's color).
fn mayahand_create(e: &mut Effect, ctx: &mut Ctx) {
    if e.sub_type == 0 || e.sub_type == 1 {
        e.life_time = 20.0;
        e.start_position = e.light;
    }
    set_kanturu3(e, ctx);
}

/// ZzzEffect.cpp:10859.
fn mayahand_step(e: &mut Effect, ctx: &mut Ctx) {
    let _luminosity = luminosity(e, ctx);
    set_kanturu3(e, ctx);
    if e.sub_type == 0 {
        e.light /= 1.17;
        let mut pos = e.position;
        pos.z += 100.0;
        ctx.create_particle(BITMAP_SMOKE, pos, e.angle, e.start_position, 17, 10.0, None);

        let direction = Vec3::new(0.0, (-80.0 - (ctx.rand() % 30) as f32) * e.scale, 0.0);
        e.position += rotate(e.angle, direction);
        ctx.create_effect(MODEL_MAYAHANDSKILL, e.position, e.angle, e.start_position, 1, None, -1, 0, 0, e.scale + 0.2);

        if e.light.x <= 0.05 {
            e.live = false;
        }
    } else if e.sub_type == 1 {
        e.light /= 1.17;
        let mut pos = e.position;
        pos.z += 100.0;
        ctx.create_particle(BITMAP_SMOKE, pos, e.angle, e.start_position, 17, 10.0, None);
    }
    move_tail(e);
}

/// `RenderEffects` only sets `m_bRenderAfterCharacter` (ZzzEffect.cpp:18323); `RenderAfterEffects`
/// (ZzzEffect.cpp:19241, only while `IsMayaScene()`) → `RenderKanturu3rdAfterObjectMesh`
/// (GM_Kanturu_3rd.cpp:650): `BodyLight = o->Light`; mesh 0 additive as blend mesh 0 with V
/// scrolling, mesh 1 additive as blend mesh 1 with U scrolling. Each mesh is a separate draw
/// (the other one hidden, assuming mesh n uses texture n).
/// TODO(original): `IsMayaScene()` is also false during the Kanturu states NIGHTMARE_BATTLE, TOWER
/// and END (`CKANTURUDirection.cpp:44`); the event state does not exist in mu-rs.
fn mayahand_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    if !in_kanturu3(e) {
        return;
    }
    let scroll = world_scroll(ctx);

    let mut d = ModelDraw::of_effect(e);
    d.hidden_mesh = 1;
    d.blend_mesh = 0;
    d.blend_mesh_tex_coord.x = e.blend_mesh_tex_coord.x;
    d.blend_mesh_tex_coord.y = scroll;
    out.models.push(d);

    let mut d = ModelDraw::of_effect(e);
    d.hidden_mesh = 0;
    d.blend_mesh = 1;
    d.blend_mesh_tex_coord.x = scroll;
    d.blend_mesh_tex_coord.y = e.blend_mesh_tex_coord.y;
    out.models.push(d);
}

// --- MODEL_CUNDUN_PART1..8, MODEL_MONSTER01+64 (Kundun's body breaking apart) --------------------------

/// ZzzEffect.cpp:2502. Created by Kundun's death (GMHellas.cpp:2031-2052) with
/// `PKKey` = depth below the ground and `SkillIndex` (`o->Skill`) = falling order.
fn cundun_create(e: &mut Effect, ctx: &mut Ctx) {
    match e.sub_type {
        1 => {
            e.life_time = 100.0;
            e.scale = 2.0;
        }
        2 | 3 => {
            let height = ctx.terrain_height(e.position.x, e.position.y);
            e.start_position = Vec3::new(e.position.x, e.position.y, height - e.pk_key as f32 - 80.0);
            e.direction = Vec3::ZERO;
            e.life_time = (250 * 2) as f32;
            e.scale = 2.0;
            e.head_angle.x = rf(ctx, 2);
            if e.sub_type == 2 {
                e.head_angle.x = 0.0;
            }
        }
        4 | 5 => {
            if e.sub_type == 4 {
                let height = ctx.terrain_height(e.position.x, e.position.y);
                e.start_position = Vec3::new(e.position.x, e.position.y, height - e.pk_key as f32);
                e.direction = Vec3::ZERO;
            }
            // case 4 falls through to case 5.
            e.current_action = 0;
            e.life_time = 170.0;
            e.scale = 2.0;
        }
        _ => {}
    }
    e.pk_key = -1;
}

/// ZzzEffect.cpp:11363.
fn cundun_step(e: &mut Effect, ctx: &mut Ctx) {
    let _luminosity = luminosity(e, ctx);
    match e.sub_type {
        1 => e.alpha -= 0.01,
        2 | 3 => cundun_sink(e, ctx),
        4 => {
            if life(e) < 140 {
                e.alpha -= 0.01;
                if e.position.z > e.start_position.z {
                    e.direction.z -= 3.5;
                } else {
                    e.direction = Vec3::ZERO;
                }
            } else {
                e.animation_frame = 0.0;
            }
        }
        5 => {
            if life(e) < 150 {
                e.alpha -= 0.01;
            } else {
                e.animation_frame = 0.0;
            }
        }
        _ => {}
    }
    move_tail(e);
}

/// Subtypes 2 and 3 (ZzzEffect.cpp:11377-11484): the part waits, falls into the water and rocks
/// while it sinks.
fn cundun_sink(e: &mut Effect, ctx: &mut Ctx) {
    let lt = life(e);
    if lt < 250 + 50 + (34 - e.skill) * 5 {
        if e.position.z > 350.0 {
            if e.direction == Vec3::ZERO {
                e.direction.x = ((-1 + ctx.rand() % 2) * 2) as f32;
                e.direction.y = ((-1 + ctx.rand() % 2) * 2) as f32;
                e.direction.z = 0.0;
            }
            e.direction.z -= 6.0;
        } else {
            if e.direction.z != -3.0 {
                // TODO(original): AddWaterWave((int)(x / TERRAIN_SCALE), (int)(y / TERRAIN_SCALE), 2, -1000).
                let light = Vec3::splat(0.3);
                for _ in 0..100 {
                    let mut position = e.position;
                    // The original feeds degrees to sinf/cosf (verbatim).
                    let f_angle = rf(ctx, 360);
                    let f_length = (ctx.rand() % 30 + 30) as f32;
                    position.x += f_angle.sin() * f_length;
                    position.y += f_angle.cos() * f_length;
                    position.z -= 30.0;
                    let mut angle = e.angle;
                    angle.z += rf(ctx, 180);
                    ctx.create_particle(BITMAP_WATERFALL_5, position, angle, light, 2, 1.0, None);
                }
            }
            let mut position = e.position;
            position.x += (ctx.rand() % 40 - 20) as f32;
            position.y += (ctx.rand() % 40 - 20) as f32;
            position.z -= 50.0;
            ctx.create_particle(BITMAP_BUBBLE, position, e.angle, e.light, 4, 1.0, None);

            e.direction = Vec3::new(0.0, 0.0, -3.0);

            if e.head_angle.x == 0.0 {
                if e.angle.x > -30.0 {
                    e.angle.x -= (ctx.rand() % 3 + 1) as f32;
                } else {
                    e.head_angle.x = 1.0;
                }
            } else if e.angle.x < 30.0 {
                e.angle.x += (ctx.rand() % 3 + 1) as f32;
            } else {
                e.head_angle.x = 0.0;
            }
            if lt < 100 {
                e.alpha -= 0.1;
            }
        }
    } else if e.sub_type != 2 && lt > 200 + 50 + 135 {
        if ctx.rand() % 2 == 0 {
            let angle = Vec3::new(0.0, 0.0, rf(ctx, 360));
            let position = e.position;
            if ctx.rand() % 2 == 0 {
                ctx.create_joint_ex(BITMAP_JOINT_SPIRIT2, position, position, angle, 8, None, 50.0, 0, 0);
            }
        }
        if ctx.rand() % 3 == 0 {
            let light = Vec3::splat(0.5);
            let scale = (ctx.rand() % 32 + 48) as f32 * 0.02;
            ctx.create_particle(BITMAP_SMOKE + 3, e.position, e.angle, light, 2, scale, None);
        }
    }
}

/// `RenderObject` (ZzzEffect.cpp:18627-18635), which for these types goes through
/// `RenderHellasMonsterObjectMesh` (GMHellas.cpp:2000, on every map):
/// - `MODEL_CUNDUN_PART1..5`, `PART8` (GMHellas.cpp:2110): `RenderBody(RENDER_TEXTURE)`.
///   TODO(original): plus a second `RenderBody(RENDER_CHROME|RENDER_BRIGHT)` pass with
///   `BodyLight = (0.4, 0.4, 0.3)` (no chrome mapping in the effect model renderer).
/// - `MODEL_CUNDUN_PART6/7`: the plain `RenderBody`.
/// - `MODEL_MONSTER01+64` (GMHellas.cpp:2016): its `MONSTER01_DIE` branches never run for an
///   effect (`CurrentAction` is 0); otherwise, while `LifeTime >= 90`, `o->Alpha = 1` and the body
///   is drawn, and nothing at all below 90.
///   TODO(original): plus meshes 1-6 with `RENDER_CHROME|RENDER_BRIGHT` and `BodyLight =
///   (0.4, 0.4, 0.3)`. The `o->Alpha = 1` write only matters for this draw (nothing is drawn after
///   LifeTime < 90), so it is applied to the draw copy.
fn cundun_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    if e.ty == MODEL_MONSTER01 + 64 {
        if e.current_action == MONSTER01_DIE {
            // Never reached by an effect (see above).
            return;
        }
        if e.life_time >= 90.0 {
            let mut d = ModelDraw::of_effect(e);
            d.alpha = 1.0;
            out.models.push(d);
        }
        return;
    }
    common::draw_object(e, ctx, out);
}

// --- Body part debris: totem golem, Condra/Narcondra, shadow pawn/knight/rook, ice giant -------------

/// The shared `CreateEffect` case of the falling body parts (ZzzEffect.cpp:2587-2612,
/// 2614-2660, 2687-2721, 2722-2746): only the scale differs.
fn create_part(e: &mut Effect, ctx: &mut Ctx, scale: f32) {
    e.life_time = (30 + ctx.rand() % 30) as f32;
    e.scale = scale;
    e.velocity = 0.0;
    e.pk_key = -1;
    e.gravity = 3.5;

    e.angle.z = rf(ctx, 360);
    let p1 = Vec3::new(0.0, (ctx.rand() % 64 + 48) as f32 * 0.1, 0.0);
    e.head_angle = rotate(e.angle, p1);
    e.head_angle.z = 15.0;

    e.sub_type = ctx.rand() % 2;

    let l = 0.5 + (ctx.rand() % 6) as f32 * 0.1;
    e.light = Vec3::splat(l);
    e.direction = Vec3::ZERO;
}

/// `MODEL_TOTEMGOLEM_PART1..6` (ZzzEffect.cpp:2587). Drawn with `RenderObject` (18654).
fn totem_create(e: &mut Effect, ctx: &mut Ctx) {
    create_part(e, ctx, 0.17);
}

/// `MODEL_CONDRA_*` / `MODEL_NARCONDRA_*` (ZzzEffect.cpp:2614, `ASG_ADD_KARUTAN_MONSTERS`).
fn condra_create(e: &mut Effect, ctx: &mut Ctx) {
    create_part(e, ctx, 1.4);
}

/// `MODEL_SHADOW_PAWN_*`, `MODEL_SHADOW_KNIGHT_*`, `MODEL_ICE_GIANT_PART1..4`
/// (ZzzEffect.cpp:2687). Drawn with `RenderObject` (18662-18683).
fn shadow_create(e: &mut Effect, ctx: &mut Ctx) {
    create_part(e, ctx, 1.1);
}

/// `MODEL_SHADOW_ROOK_*` (ZzzEffect.cpp:2722). Drawn with `RenderObject` (18674).
fn rook_create(e: &mut Effect, ctx: &mut Ctx) {
    create_part(e, ctx, 1.3);
}

/// The shared `MoveEffect` case of the falling body parts: they fly, bounce on the ground (20
/// units above it), spin while in the air and fade on the ground. `damp` is the horizontal
/// slowdown on each bounce.
fn move_part(e: &mut Effect, ctx: &mut Ctx, damp: f32) {
    e.head_angle.z -= e.gravity;
    e.position += e.head_angle;

    let height = ctx.terrain_height(e.position.x, e.position.y) + 20.0;
    let lt = life(e) as f32;
    if e.position.z + e.direction.z <= height {
        e.position.z = height;
        e.head_angle.x *= damp;
        e.head_angle.y *= damp;
        e.head_angle.z += 0.6 * lt;
        if e.head_angle.z < 5.0 {
            e.head_angle.z = 0.0;
        }
        e.alpha -= 0.05;
    } else if e.sub_type == 0 {
        e.angle.x += 0.15 * lt;
        e.angle.y += 0.15 * lt;
    } else {
        e.angle.x -= 0.15 * lt;
        e.angle.y -= 0.15 * lt;
    }
}

/// Totem golem and Condra/Narcondra parts (ZzzEffect.cpp:11566-11633).
fn part_step_08(e: &mut Effect, ctx: &mut Ctx) {
    let _luminosity = luminosity(e, ctx);
    move_part(e, ctx, 0.8);
    move_tail(e);
}

/// Shadow pawn/knight/rook and ice giant parts (ZzzEffect.cpp:11677-11732).
fn part_step_05(e: &mut Effect, ctx: &mut Ctx) {
    let _luminosity = luminosity(e, ctx);
    move_part(e, ctx, 0.5);
    move_tail(e);
}

// --- MODEL_DOPPELGANGER_SLIME_CHIP --------------------------------------------------------------------

/// ZzzEffect.cpp:2662. No `case` in RenderEffects: the `default:` `RenderObject` (skill range).
fn slime_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = (30 + ctx.rand() % 10) as f32;
    e.scale = 0.1 + (ctx.rand() % 7) as f32 * 0.1;
    e.velocity = 0.0;
    e.pk_key = -1;
    e.gravity = 4.0;

    e.angle.z = rf(ctx, 360);
    let p1 = Vec3::new(0.0, (ctx.rand() % 64 + 48) as f32 * 0.1, 0.0);
    e.head_angle = rotate(e.angle, p1);
    e.head_angle.z = (35 + ctx.rand() % 5) as f32;

    e.sub_type = ctx.rand() % 2;

    // `VectorCopy(Light, o->Light)`: already the creation light.
    e.direction = Vec3::ZERO;

    e.angle.x = rf(ctx, 360);
    e.angle.y = rf(ctx, 360);
}

/// ZzzEffect.cpp:11634.
fn slime_step(e: &mut Effect, ctx: &mut Ctx) {
    let _luminosity = luminosity(e, ctx);
    e.head_angle.z -= e.gravity;
    e.position += e.head_angle;

    let height = ctx.terrain_height(e.position.x, e.position.y) + 20.0;
    let lt = life(e);
    if lt < 10 {
        e.light *= 0.8;
    }

    if e.position.z + e.direction.z <= height {
        e.position.z = height;
        e.head_angle.x *= 0.8;
        e.head_angle.y *= 0.8;
        e.head_angle.z += 1.6 * lt as f32;
        if e.head_angle.z < 5.0 {
            e.head_angle.z = 0.0;
        }
    } else {
        e.scale += (ctx.world_time() * 0.015).sin() * 0.1;
    }

    if e.sub_type == 0 {
        e.angle.x += 0.35 * lt as f32;
        e.angle.y += 0.35 * lt as f32;
    } else {
        e.angle.x -= 0.35 * lt as f32;
        e.angle.y -= 0.35 * lt as f32;
    }
    move_tail(e);
}
