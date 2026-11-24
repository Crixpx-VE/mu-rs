//! Empire Guardian boss Gaion's swords and frame strike (`MODEL_SWORDLEFT01/LEFT02/RIGHT01/
//! RIGHT02/MAIN01_EMPIREGUARDIAN_BOSS_GAION_`, `MODEL_EMPIREGUARDIANBOSS_FRAMESTRIKE`): one shared
//! `case` in `CreateEffect` (ZzzEffect.cpp:5279-5996), `MoveEffect` (15906-16833) and
//! `RenderEffects` (18904-19057). Part of `effects_monsters2`.
//!
//! The original drives these with a `CInterpolateContainer` (w_ObjectInfo.cpp): lists of
//! (rate start, rate end, start value, end value) for angle, position, scale and alpha, sampled at
//! `rate = 1 - LifeTime / ExtState`. The lists are rebuilt here from the type, subtype and a few
//! values saved at creation ([`tracks`]); `ExtState` and `ChromeEnable` follow from the type and
//! subtype ([`ext_state`], [`chrome`]). Saved values (`user`):
//! - subtype 1, LEFT01/RIGHT01: [0] first-segment rate, [1..4] first point, [4..7] last point.
//! - subtype 1, LEFT02/RIGHT02: [0..3] first point, [3..6] last point.
//! - subtype 3: [0..3] apex, [3..6] apex + horizontal distance, [6] landing height.
//! - subtype 20: [0..3] angle, [3..6] position, [6] scale.
//!
//! The models' own bones: `AnimationTransformOnlySelf` (ZzzBMD.cpp:587/610) and
//! `AnimationTransformWithAttachHighModel[_usingGlobalTM]` (ZzzBMD.cpp:480/536) → [`self_bone_points`]
//! and [`attached_bone_points`]. Without the model files every bone point falls back to the attach
//! point or the effect's position (no loop count depends on the model except the frame strike's
//! `NumBones`, which falls back to [`FRAMESTRIKE_NUM_BONES`]).
//!
//! Not available in the engine (TODO(original) at each place): object blurs (`CreateObjectBlur`,
//! `RemoveObjectBlurs`).

use bevy::math::{EulerRot, Mat3, Quat, Vec3};
use mu_data::bmd::{self, Mat34};

use super::super::common::{self, luminosity, move_tail, rf};
use super::{life, me, normalize_mu, sprite_glow};
use crate::fx::core::*;

const LEFT01: i32 = MODEL_SWORDLEFT01_EMPIREGUARDIAN_BOSS_GAION_;
const LEFT02: i32 = MODEL_SWORDLEFT02_EMPIREGUARDIAN_BOSS_GAION_;
const RIGHT01: i32 = MODEL_SWORDRIGHT01_EMPIREGUARDIAN_BOSS_GAION_;
const RIGHT02: i32 = MODEL_SWORDRIGHT02_EMPIREGUARDIAN_BOSS_GAION_;
const MAIN01: i32 = MODEL_SWORDMAIN01_EMPIREGUARDIAN_BOSS_GAION_;
const FRAMESTRIKE: i32 = MODEL_EMPIREGUARDIANBOSS_FRAMESTRIKE;

/// `Models[MODEL_EMPIREGUARDIANBOSS_FRAMESTRIKE].NumBones` of Data/Effect/Karanebos_sword_framestrike.bmd,
/// used when the file is missing so the `rand()` count of its fire loop stays the same.
const FRAMESTRIKE_NUM_BONES: usize = 10;

pub fn register(r: &mut Registry) {
    for ty in [LEFT01, LEFT02, RIGHT01, RIGHT02, MAIN01] {
        r.effect(ty, EffectKind { create: Some(gaion_create), step: Some(gaion_step), draw: Some(sword_draw) });
    }
    // ZzzEffect.cpp:19053: RenderObject(o).
    r.effect(FRAMESTRIKE, EffectKind { create: Some(gaion_create), step: Some(gaion_step), draw: Some(common::draw_object) });
}

// --- Interpolation (CInterpolateContainer) -----------------------------------------------------------

#[derive(Clone, Copy)]
struct Seg3 {
    r0: f32,
    r1: f32,
    a: Vec3,
    b: Vec3,
}

#[derive(Clone, Copy)]
struct SegF {
    r0: f32,
    r1: f32,
    a: f32,
    b: f32,
}

#[derive(Default)]
struct Tracks {
    angle: Vec<Seg3>,
    pos: Vec<Seg3>,
    scale: Vec<SegF>,
    alpha: Vec<SegF>,
}

fn s3(r0: f32, r1: f32, a: Vec3, b: Vec3) -> Seg3 {
    Seg3 { r0, r1, a, b }
}

fn sf(r0: f32, r1: f32, a: f32, b: f32) -> SegF {
    SegF { r0, r1, a, b }
}

/// `GetCurrentValue` (w_ObjectInfo.cpp:7): the first segment with `start <= rate < end`; `None`
/// (output untouched) if there is none.
fn get3(segs: &[Seg3], rate: f32) -> Option<Vec3> {
    let s = segs.iter().find(|s| s.r0 <= rate && s.r1 > rate)?;
    let w = (rate - s.r0) / (s.r1 - s.r0);
    Some(s.a + (s.b - s.a) * w)
}

/// `GetCurrentValueF` (w_ObjectInfo.cpp:40).
fn getf(segs: &[SegF], rate: f32) -> Option<f32> {
    let s = segs.iter().find(|s| s.r0 <= rate && s.r1 > rate)?;
    Some(s.a + (s.b - s.a) * ((rate - s.r0) / (s.r1 - s.r0)))
}

fn user3(e: &Effect, i: usize) -> Vec3 {
    Vec3::new(e.user[i], e.user[i + 1], e.user[i + 2])
}

fn set_user3(e: &mut Effect, i: usize, v: Vec3) {
    e.user[i] = v.x;
    e.user[i + 1] = v.y;
    e.user[i + 2] = v.z;
}

/// The `m_Interpolates` lists the creation code fills, rebuilt from the saved values.
fn tracks(e: &Effect) -> Tracks {
    let mut t = Tracks::default();
    let start = e.start_position;
    let x90 = Vec3::new(90.0, 0.0, 0.0);
    match e.sub_type {
        // ZzzEffect.cpp:5341-5380 (only the second alpha factor is pushed).
        3 => {
            let off = 0.03 * (e.ty - LEFT01) as f32;
            let (p1, p2) = (user3(e, 0), user3(e, 3));
            let p3 = Vec3::new(p2.x, p2.y, e.user[6]);
            t.pos = vec![
                s3(0.0 + off, 0.22 + off, start, p1),
                s3(0.22 + off, 0.35 + off, p1, p2),
                s3(0.35 + off, 0.50 + off, p2, p3),
                s3(0.50 + off, 1.01, p3, p3),
            ];
            t.alpha = vec![sf(0.50, 1.01, 1.0, 0.0)];
        }
        // ZzzEffect.cpp:5397-5408.
        11 => t.alpha = vec![sf(0.0, 0.61, 0.0, 1.0), sf(0.61, 1.01, 1.0, 1.0)],
        // ZzzEffect.cpp:5420-5425.
        12 => t.alpha = vec![sf(0.0, 1.01, 0.0, 1.0)],
        // ZzzEffect.cpp:5453-5478.
        20 => {
            let (a0, p0, s0) = (user3(e, 0), user3(e, 3), e.user[6]);
            t.angle = vec![s3(0.0, 1.0, a0, a0)];
            t.pos = vec![s3(0.0, 1.0, p0, p0)];
            t.alpha = vec![sf(0.0, 1.0, 0.7, 0.0)];
            t.scale = vec![sf(0.0, 1.0, s0, s0)];
        }
        1 => match e.ty {
            // ZzzEffect.cpp:5540-5563 / 5624-5647.
            RIGHT01 | LEFT01 => {
                let r1 = e.user[0];
                let z_end = if e.ty == RIGHT01 { 1560.0 } else { -1560.0 };
                let (p1, pf) = (user3(e, 1), user3(e, 4));
                t.angle = vec![s3(0.0, r1, Vec3::ZERO, x90), s3(r1, 1.01, x90, Vec3::new(90.0, 0.0, z_end))];
                t.pos = vec![s3(0.0, r1, start, p1), s3(r1, 1.01, p1, pf)];
            }
            // ZzzEffect.cpp:5712-5753.
            RIGHT02 => {
                let (p1, pf) = (user3(e, 0), user3(e, 3));
                t.angle = vec![s3(0.0, 0.32, Vec3::ZERO, x90), s3(0.32, 1.01, x90, Vec3::new(90.0, 0.0, 340.0))];
                t.pos = vec![s3(0.0, 0.32, start, p1), s3(0.32, 1.01, p1, pf)];
                t.alpha = vec![sf(0.0, 0.15, 1.0, 1.0), sf(0.15, 0.75, 1.0, 1.0), sf(0.75, 1.01, 1.0, 0.0)];
            }
            // ZzzEffect.cpp:5817-5859.
            LEFT02 => {
                let (p1, pf) = (user3(e, 0), user3(e, 3));
                let x110 = Vec3::new(110.0, 0.0, 0.0);
                t.angle = vec![s3(0.0, 0.4, Vec3::ZERO, x110), s3(0.4, 1.01, x110, Vec3::new(60.0, 0.0, -300.0))];
                t.pos = vec![s3(0.0, 0.33, start, p1), s3(0.33, 1.01, p1, pf)];
                t.alpha = vec![sf(0.0, 0.15, 1.0, 1.0), sf(0.15, 0.75, 1.0, 1.0), sf(0.75, 1.01, 1.0, 0.0)];
            }
            // ZzzEffect.cpp:5880-5885.
            MAIN01 => t.alpha = vec![sf(0.0, 1.01, 1.0, 0.0)],
            // ZzzEffect.cpp:5937-5985 (StartPosition holds v3PosProcess01).
            FRAMESTRIKE => {
                let xm90 = Vec3::new(-90.0, 0.0, 0.0);
                t.angle = vec![
                    s3(0.0, 0.28, Vec3::ZERO, Vec3::ZERO),
                    s3(0.28, 0.36, Vec3::ZERO, xm90),
                    s3(0.36, 1.01, xm90, Vec3::new(-90.0, 0.0, 880.0)),
                ];
                t.pos = vec![s3(0.0, 1.01, start, start)];
                t.scale = vec![sf(0.0, 1.01, 1.0, 1.0)];
                t.alpha = vec![sf(0.0, 0.15, 0.0, 1.0), sf(0.15, 0.75, 1.0, 1.0), sf(0.75, 1.01, 1.0, 0.0)];
            }
            _ => {}
        },
        _ => {}
    }
    t
}

/// `o->ExtState` as the creation code sets it.
fn ext_state(e: &Effect) -> Option<i32> {
    match e.sub_type {
        3 => Some(60),
        11 => Some(24),
        12 => Some(14),
        20 => Some(10),
        1 => match e.ty {
            RIGHT01 | LEFT01 => Some(35),
            RIGHT02 | LEFT02 => Some(30),
            MAIN01 => Some(10),
            FRAMESTRIKE => Some(36),
            _ => None,
        },
        _ => None,
    }
}

/// `o->ChromeEnable`: the sword is drawn attached to its owner's bone.
fn chrome(e: &Effect) -> bool {
    match e.sub_type {
        0 | 11 | 12 => true,
        1 => e.ty == MAIN01,
        _ => false,
    }
}

/// `arrBoneIdxs[] = { 4, 2, 8, 10, 6 }[Type - MODEL_SWORDLEFT01...]`: the owner bone each model
/// hangs from (the frame strike is out of the array).
fn gaion_bone(ty: i32) -> Option<usize> {
    [4usize, 2, 8, 10, 6].get(usize::try_from(ty - LEFT01).ok()?).copied()
}

/// `AnimationTransformWithAttachHighModel(_usingGlobalTM)` (ZzzBMD.cpp:480/536): the owner's bone
/// as a world position (translation × owner scale + owner position) and rotation (owner angle ·
/// bone rotation).
fn owner_attach(owner: Option<Target>, actors: &std::collections::HashMap<bevy::prelude::Entity, Actor>, bone: usize) -> Option<(Vec3, Mat3)> {
    let actor = common::owner_actor(owner, actors)?;
    let m = actor.bones.get(bone)?;
    let local = Mat3::from_cols(
        Vec3::new(m[0][0], m[1][0], m[2][0]),
        Vec3::new(m[0][1], m[1][1], m[2][1]),
        Vec3::new(m[0][2], m[1][2], m[2][2]),
    );
    Some((actor.bone_world(bone, Vec3::ZERO), angle_matrix(actor.angle) * local))
}

/// The model-space position of each bone (`BoneMatrix[i][0..3][3]`).
fn bone_origins(bones: &[Mat34]) -> impl Iterator<Item = Vec3> + '_ {
    bones.iter().map(|m| Vec3::from_array(bmd::transform_point(m, [0.0; 3])))
}

/// The sword's own bones hung from the owner's bone: the sword model at frame 0 of its
/// `CurrentAction` with no angle (`Animation(..., 0, 0, 0, Temp, Temp, false, false)`), each bone
/// composed with the owner's bone (translation × owner scale + owner position, ZzzBMD.cpp:556-581).
fn attach_bones(bones: Option<Vec<Mat34>>, attach: Option<(Vec3, Mat3)>) -> Option<Vec<Vec3>> {
    let (p, rot) = attach?;
    Some(bone_origins(&bones?).map(|t| p + rot * t).collect())
}

/// The sword at frame 0, for [`attach_bones`].
fn frame0(e: &Effect) -> Effect {
    let mut f = e.clone();
    f.animation_frame = 0.0;
    f
}

/// `arrEachBonePos[0..11]` of `AnimationTransformWithAttachHighModel` (MoveEffect, ZzzEffect.cpp:
/// 16329, 16664, 16768): the loops read bones 0..10 whatever `NumBones` is. A bone the model does
/// not have (or no model / owner) falls back to the attach point, or the effect's position.
fn attached_bone_points(e: &Effect, ctx: &mut Ctx) -> Vec<Vec3> {
    let attach = gaion_bone(e.ty).and_then(|b| owner_attach(e.owner, ctx.actors, b));
    let fallback = attach.map_or(e.position, |(p, _)| p);
    let points = attach_bones(ctx.effect_bones(&frame0(e)), attach).unwrap_or_default();
    (0..11).map(|j| points.get(j).copied().unwrap_or(fallback)).collect()
}

/// `b->AnimationTransformOnlySelf(arr, Angle, Position, Scale[, o])` (ZzzBMD.cpp:610-680): the
/// model's bones posed with `Angle` at `frame` (0 without the reference object, its
/// `AnimationFrame` with it). The `Position` and `Scale` arguments are copied but never used
/// (ZzzBMD.cpp:626-643): `Animation` places the bones with the model's `BodyOrigin` / `BodyScale`,
/// which the effect's previous `RenderObject` left: `origin` / `scale` are the effect's position
/// and scale before this move. `None` without the model file.
fn self_bone_points(ctx: &mut Ctx, e: &Effect, angle: Vec3, frame: f32, origin: Vec3, scale: f32) -> Option<Vec<Vec3>> {
    let mut posed = e.clone();
    posed.animation_frame = frame;
    let bones = ctx.effect_bones(&posed)?;
    Some(bone_origins(&bones).map(|t| origin + rotate(angle, t * scale)).collect())
}

/// Applies the angle / position / scale / alpha lists at `rate` (MoveEffect). `relative`: the
/// angle is added to `HeadAngle` (subtypes 1 and 3). `alpha_by_scale`: subtype 1 only reads the
/// alpha list when the *scale* list is not empty (ZzzEffect.cpp:15947).
fn apply_tracks(e: &mut Effect, t: &Tracks, rate: f32, relative: bool, alpha_by_scale: bool) {
    if !t.angle.is_empty()
        && let Some(rel) = get3(&t.angle, rate)
    {
        e.angle = if relative { rel + e.head_angle } else { rel };
    }
    if let Some(p) = get3(&t.pos, rate) {
        e.position = p;
    }
    if let Some(s) = getf(&t.scale, rate) {
        e.scale = s;
    }
    let read_alpha = if alpha_by_scale { !t.scale.is_empty() } else { !t.alpha.is_empty() };
    if read_alpha && let Some(a) = getf(&t.alpha, rate) {
        e.alpha = a;
    }
}

/// `GetAngleCurrent/GetPosCurrent/GetAlphaCurrent/GetScaleCurrent(..., 0)` at the end of creation.
fn apply_initial(e: &mut Effect, angle: bool, alpha: bool, scale: bool) {
    let t = tracks(e);
    if angle && let Some(a) = get3(&t.angle, 0.0) {
        e.angle = a;
    }
    if let Some(p) = get3(&t.pos, 0.0) {
        e.position = p;
    }
    if alpha && let Some(a) = getf(&t.alpha, 0.0) {
        e.alpha = a;
    }
    if scale && let Some(s) = getf(&t.scale, 0.0) {
        e.scale = s;
    }
}

// --- CreateEffect (ZzzEffect.cpp:5279-5996) ----------------------------------------------------------

fn gaion_create(e: &mut Effect, ctx: &mut Ctx) {
    // TODO(original): RemoveObjectBlurs(&Hero->Object, 113..=155) (object blurs do not exist).
    match e.sub_type {
        0 => e.life_time = 2.0,
        3 => create_sub3(e, ctx),
        // `Scale = Scale` (the argument).
        11 => e.life_time = 24.0,
        12 => e.life_time = 14.0,
        20 => {
            e.life_time = match e.ty {
                RIGHT02 | LEFT02 | FRAMESTRIKE => 5.0,
                _ => 10.0,
            };
            let (a, p) = (e.angle, e.position);
            set_user3(e, 0, a);
            set_user3(e, 3, p);
            e.user[6] = e.scale;
        }
        1 => match e.ty {
            RIGHT01 | LEFT01 => create_sub1_01(e, ctx),
            RIGHT02 | LEFT02 => create_sub1_02(e),
            MAIN01 => e.life_time = 10.0,
            FRAMESTRIKE => create_sub1_framestrike(e, ctx),
            _ => {}
        },
        _ => {}
    }
}

/// Subtype 3 (ZzzEffect.cpp:5297-5386): up into the air, across and down onto `Light` (the target).
fn create_sub3(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = 60.0;
    e.gravity = 0.0;
    e.hidden_mesh = 1;
    e.distance = 0.0;
    e.alpha = 1.0;
    e.head_angle = e.angle;
    e.start_position = e.position;
    let start = e.position;
    let target = e.light;
    let height_target = ctx.terrain_height(target.x, target.y);
    let target_mod = Vec3::new(target.x, target.y, start.z);
    let dir_ad = target_mod - start;
    let dist_ab = 900.0 + (ctx.rand() % 100 - 50) as f32;
    let x = start.x + (ctx.rand() % 200 - 100) as f32;
    let y = start.y + (ctx.rand() % 200 - 100) as f32;
    let p1 = Vec3::new(x, y, start.z + dist_ab);
    let p2 = p1 + dir_ad;
    let z3 = height_target + 100.0 * 1.2 + (ctx.rand() % 100 - 50) as f32;
    set_user3(e, 0, p1);
    set_user3(e, 3, p2);
    e.user[6] = z3;
    apply_initial(e, true, true, false);
    ctx.create_joint_ex(BITMAP_FLARE + 1, e.position, e.position, e.angle, 20, me(e), 160.0, 40, 0);
}

/// Subtype 1 of LEFT01/RIGHT01 (ZzzEffect.cpp:5484-5567 / 5570-5651).
fn create_sub1_01(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = 35.0;
    e.gravity = 0.0;
    e.hidden_mesh = 1;
    e.alpha = 1.0;
    e.head_angle = e.angle;
    e.start_position = e.position;
    let dir = e.light - e.position;
    e.distance = dir.length();
    e.direction = if e.distance != 0.0 { dir / e.distance } else { dir };
    // The original requires an owner (`o->Owner->Position`).
    let owner = ctx.target_position(e.owner).unwrap_or(e.position);
    let modify02 = Vec3::new(owner.x, owner.y, e.light.z + 100.0);
    let modify = Vec3::new(e.position.x, e.position.y, e.light.z + 100.0);
    let target_mod = e.light + Vec3::new(0.0, 0.0, 100.0);
    let dir_mod = normalize_mu(target_mod - modify02);
    let total = 1700.0;
    let first = e.distance * 0.3;
    let p1 = modify + dir_mod * first;
    let pf = modify + dir_mod * total;
    e.user[0] = first / total;
    set_user3(e, 1, p1);
    set_user3(e, 4, pf);
    apply_initial(e, true, false, false);
}

/// Subtype 1 of LEFT02/RIGHT02 (ZzzEffect.cpp:5654-5758 / 5761-5864).
fn create_sub1_02(e: &mut Effect) {
    e.life_time = 30.0;
    e.gravity = 0.2;
    e.hidden_mesh = 1;
    e.alpha = 1.0;
    e.head_angle = e.angle;
    e.start_position = e.position;
    let start = e.start_position;
    let end_mod = e.light;
    let start_mod = Vec3::new(start.x, start.y, end_mod.z);
    let dir_mod = normalize_mu(end_mod - start_mod);
    let dir = e.light - start;
    e.distance = dir.length();
    e.direction = if e.distance != 0.0 { dir / e.distance } else { dir };
    let (a1, a2, drop) = if e.ty == RIGHT02 { (-40.0, 24.0, 290.0) } else { (50.0, -26.0, 280.0) };
    let mut p1 = start + rotate(Vec3::new(0.0, 0.0, a1), dir_mod) * (e.distance * 0.4);
    p1.z -= 100.0;
    let mut pf = start + rotate(Vec3::new(0.0, 0.0, a2), dir_mod) * (e.distance + 700.0);
    pf.z -= drop;
    set_user3(e, 0, p1);
    set_user3(e, 3, pf);
    apply_initial(e, true, true, false);
}

/// Subtype 1 of the frame strike (ZzzEffect.cpp:5889-5991).
fn create_sub1_framestrike(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = 36.0;
    e.gravity = 0.0;
    e.hidden_mesh = 1;
    e.alpha = 1.0;
    e.scale = 1.0;
    e.velocity = 0.4;
    e.head_angle = e.angle;
    let dir = e.light - e.position;
    e.distance = dir.length();
    e.direction = if e.distance != 0.0 { dir / e.distance } else { dir };
    // The original requires an owner (`o->Owner->Position`).
    let owner = ctx.target_position(e.owner).unwrap_or(e.position);
    let modify = Vec3::new(owner.x, owner.y, e.light.z + 100.0);
    let target_mod = e.light + Vec3::new(0.0, 0.0, 100.0);
    let dir_mod = normalize_mu(modify - target_mod);
    e.start_position = target_mod + dir_mod * 200.0;
    apply_initial(e, true, true, true);
}

// --- MoveEffect (ZzzEffect.cpp:15906-16833) -----------------------------------------------------------

fn gaion_step(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    if e.ty != FRAMESTRIKE {
        // RenderEffects (ZzzEffect.cpp:18950/18952) leaves `o->Alpha = 1` after drawing a sword;
        // moved here from the draw.
        e.alpha = 1.0;
    }
    // `BodyOrigin` / `BodyScale` of the model as the previous render left them (see
    // `self_bone_points`).
    let body = (e.position, e.scale);
    match e.sub_type {
        1 => step_sub1(e, ctx, body),
        3 => step_sub3(e, ctx),
        11 => step_sub11(e, ctx),
        12 => step_sub12(e, ctx),
        20 => step_sub20(e),
        _ => {}
    }
    move_tail(e, ctx);
}

fn rate_of(e: &Effect, ext: i32) -> f32 {
    1.0 - life(e) as f32 / ext as f32
}

/// Subtype 1: the attack itself (ZzzEffect.cpp:15916-16464). `body`: the model's `BodyOrigin` and
/// `BodyScale` before this move.
fn step_sub1(e: &mut Effect, ctx: &mut Ctx, body: (Vec3, f32)) {
    let (body_origin, body_scale) = body;
    let Some(ext) = ext_state(e) else { return };
    let lt = life(e);
    let rate = rate_of(e, ext);
    let t = tracks(e);
    apply_tracks(e, &t, rate, true, true);

    let (blur_start, blur_end) = match e.ty {
        RIGHT01 | LEFT01 => (0.1, 0.90),
        RIGHT02 | LEFT02 => (0.3, 1.01),
        FRAMESTRIKE => (0.10, 1.01),
        _ => (0.0, 0.0),
    };
    let force = matches!(e.ty, FRAMESTRIKE | MAIN01 | LEFT01 | RIGHT01);
    let shadow = matches!(e.ty, LEFT02 | RIGHT02);

    if force && rate > blur_start && rate < blur_end {
        let clamp = |v: f32| v.clamp(0.0, 1.0);
        let pre = clamp(1.0 - (lt + 1) as f32 / ext as f32);
        if pre < rate {
            let start = clamp(1.0 - (lt + 2) as f32 / ext as f32);
            let end = clamp(1.0 - (lt + 1) as f32 / ext as f32);
            let access = match e.ty {
                RIGHT01 | LEFT01 => 20,
                RIGHT02 | LEFT02 => 3,
                FRAMESTRIKE => 6,
                _ => 10,
            };
            let unit = (end - start) / access as f32;
            let mut cur = start;
            match e.ty {
                RIGHT01 | LEFT01 => {
                    let spark_bone = if e.ty == RIGHT01 { 12 } else { 1 };
                    for _ in 0..access {
                        cur += unit;
                        // `AnimationTransformOnlySelf(arr, angle(cur) + HeadAngle, pos(cur), Scale)`
                        // (ZzzEffect.cpp:16126-16132; the angle segments cover every rate of this
                        // loop).
                        let angle = get3(&t.angle, cur).unwrap_or(Vec3::ZERO) + e.head_angle;
                        // TODO(original): CreateObjectBlur(o, arr[3 / 13], arr[12 / 1], (1,1,1), 13,
                        // false, 113 / 119, 25) (RIGHT01 / LEFT01); object blurs do not exist.
                        if ctx.rand() % 2 == 0 {
                            // The spark sits on bone 12 (RIGHT01) / 1 (LEFT01); without the model
                            // file, at the interpolated position.
                            let point = self_bone_points(ctx, e, angle, 0.0, body_origin, body_scale)
                                .and_then(|b| b.get(spark_bone).copied())
                                .unwrap_or_else(|| get3(&t.pos, cur).unwrap_or(e.position));
                            let dist = (ctx.rand() % 100) as f32 + 100.0;
                            ctx.create_particle(BITMAP_FIRE, point, e.angle, e.light, 5, 1.35, None);
                            let angle = Vec3::new(rf(ctx, 360), 0.0, rf(ctx, 360));
                            let from = point + rotate(angle, Vec3::new(0.0, dist, 0.0));
                            ctx.create_joint_ex(BITMAP_JOINT_THUNDER, from, point, angle, 3, None, 10.0, 10, 10);
                        }
                    }
                }
                FRAMESTRIKE => {
                    // TODO(original): 6 samples of AnimationTransformOnlySelf(arr, angle(cur) +
                    // HeadAngle, pos(cur), Scale, o, Velocity, i/6) (ZzzEffect.cpp:16225-16240), only
                    // to feed CreateObjectBlur(o, arr[4], arr[9], (1,1,1), 13, false, 157, 11); object
                    // blurs do not exist (no rand() in the loop).
                    let _ = cur;
                }
                _ => {}
            }
        }
    }

    if shadow && rate > 0.3 && rate < 1.01 {
        ctx.create_effect(e.ty, e.position, e.angle, e.light, 20, me(e), -1, 0, 0, e.scale);
    }

    match e.ty {
        RIGHT01 | LEFT01 | RIGHT02 | LEFT02 => {
            ctx.add_terrain_light(e.position.x, e.position.y, Vec3::new(0.0, 0.2, 0.9), 2.0);
        }
        MAIN01 => {
            // ZzzEffect.cpp:16303-16352.
            if (0.0..=0.6).contains(&rate) {
                let points = attached_bone_points(e, ctx);
                ctx.add_terrain_light(e.position.x, e.position.y, Vec3::new(0.9, 0.4, 0.1), 2.0);
                for point in points {
                    if ctx.rand() % 20 == 0 {
                        ctx.create_effect(BITMAP_FIRE_CURSEDLICH, point, e.angle, Vec3::ONE, 12, me(e), -1, 0, 0, e.scale);
                    }
                }
            }
        }
        FRAMESTRIKE => {
            // ZzzEffect.cpp:16354-16389: `AnimationTransformOnlySelf(arr, Angle, Position, Scale, o)`
            // (at its AnimationFrame) and one try per bone (`NumBones`).
            if (0.0..=0.5).contains(&rate) {
                let count = ctx.model_bone_count(FRAMESTRIKE).unwrap_or(FRAMESTRIKE_NUM_BONES);
                let points = self_bone_points(ctx, e, e.angle, e.animation_frame, body_origin, body_scale).unwrap_or_default();
                for i in 0..count {
                    let base = points.get(i).copied().unwrap_or(e.position);
                    let x = base.x + (ctx.rand() % 80 - 40) as f32;
                    let y = base.y + (ctx.rand() % 80 - 40) as f32;
                    let z = base.z + (ctx.rand() % 80 - 40) as f32;
                    if ctx.rand() % 10 == 0 {
                        ctx.create_effect(BITMAP_FIRE_CURSEDLICH, Vec3::new(x, y, z), e.angle, Vec3::ONE, 12, me(e), -1, 0, 0, e.scale * 1.5);
                    }
                }
            }
            ctx.add_terrain_light(e.position.x, e.position.y, Vec3::new(0.9, 0.4, 0.1), 2.0);
        }
        _ => {}
    }

    match e.ty {
        LEFT01 | RIGHT01 if lt == 5 => {
            ctx.create_effect(e.ty, e.position, e.angle, e.light, 11, e.owner, -1, 0, 0, e.scale);
        }
        RIGHT02 | LEFT02 if lt == 3 => {
            ctx.create_effect(e.ty, e.position, e.angle, e.light, 11, e.owner, -1, 0, 0, e.scale);
        }
        FRAMESTRIKE if lt == 10 => {
            ctx.create_effect(MAIN01, e.position, e.angle, e.light, 11, e.owner, -1, 0, 0, e.scale);
        }
        MAIN01 if lt == 1 => {
            ctx.create_effect(FRAMESTRIKE, e.position, e.angle, e.light, 1, e.owner, -1, 0, 0, e.scale);
        }
        _ => {}
    }

    match e.ty {
        RIGHT01 | LEFT01 if lt == ext - 1 => ctx.play_sound("SOUND_BLOODATTACK"),
        RIGHT02 | LEFT02 if lt == ext - 1 => ctx.play_sound("SOUND_ATTACK_MELEE_HIT5") /* SOUND_ATTACK01+4: eMeleeHit5.wav (ZzzOpenData.cpp:4623) */,
        FRAMESTRIKE if lt == ext - 5 => ctx.play_sound("SOUND_SKILL_FLAME_STRIKE"),
        _ => {}
    }
}

/// Subtype 3: the falling sword (ZzzEffect.cpp:16465-16629).
fn step_sub3(e: &mut Effect, ctx: &mut Ctx) {
    let ext = 60;
    let lt = life(e);
    let rate = rate_of(e, ext);
    let t = tracks(e);
    apply_tracks(e, &t, rate, true, false);

    match e.ty {
        RIGHT01 | LEFT01 | RIGHT02 | LEFT02 => {
            // `if (o->LifeTime == 0)` (ZzzEffect.cpp:16509) never holds inside MoveEffect: an
            // effect with LifeTime 0 is already dead.
            ctx.add_terrain_light(e.position.x, e.position.y, Vec3::new(0.0, 0.2, 0.9), 2.0);
        }
        MAIN01 if lt == 40 => {
            // The impact at the target (`o->Light`), ZzzEffect.cpp:16529-16599.
            let target = e.light;
            for i in 0..5 {
                let angle = Vec3::new(0.0, 0.0, e.angle.z + i as f32 * 72.0);
                let pos = rotate(angle, Vec3::new(0.0, 200.0, 0.0)) + target;
                ctx.create_effect(BITMAP_JOINT_THUNDER, pos, e.angle, Vec3::ONE, 1, None, -1, 0, 0, 0.0);
            }
            ctx.create_effect(BITMAP_CRATER, target, e.angle, Vec3::ONE, 2, None, -1, 0, 0, 1.5);
            for _ in 0..20 {
                ctx.create_effect(MODEL_STONE2, target, e.angle, Vec3::ONE, 0, None, -1, 0, 0, 0.0);
            }
            let light = Vec3::new(0.7, 0.7, 1.0);
            for _ in 0..6 {
                let x = target.x + ((ctx.rand() % 200) as f32 - 100.0);
                let y = target.y + ((ctx.rand() % 200) as f32 - 100.0);
                let z = target.z + ((ctx.rand() % 200) as f32 - 100.0);
                ctx.create_particle(BITMAP_CLUD64, Vec3::new(x, y, z), e.angle, light, 7, 2.0, None);
            }
            let light = Vec3::new(0.3, 0.2, 1.0);
            ctx.create_effect(BITMAP_SHOCK_WAVE, target, e.angle, light, 11, None, -1, 0, 0, 0.0);
            ctx.create_effect(BITMAP_SHOCK_WAVE, target, e.angle, light, 11, None, -1, 0, 0, 0.0);
            let pos = target + Vec3::new(0.0, 0.0, 100.0);
            ctx.create_effect(MODEL_EFFECT_THUNDER_NAPIN_ATTACK_1, pos, e.angle, Vec3::new(0.0, 0.2, 1.0), 1, None, -1, 0, 0, 1.0);
        }
        _ => {}
    }

    if lt == 15 {
        ctx.create_effect(e.ty, e.position, e.angle, e.light, 12, e.owner, -1, 0, 0, e.scale);
    }
    let on_the_land = ext / 2 + 5;
    if lt <= on_the_land && lt > 10 {
        let quake = (ctx.rand() % 2 - 2) as f32 * 0.5;
        ctx.set_earthquake(quake);
    }
    if lt == ext {
        ctx.play_sound("SOUND_ASSASSIN");
    }
    if lt == ext / 2 + 2 {
        ctx.play_sound("SOUND_SKILL_GIGANTIC_STORM");
    }
    if lt == ext / 2 + 5 {
        ctx.play_sound("SOUND_FURY_STRIKE2");
    }
}

/// Subtype 11: appearing at the owner's hand (ZzzEffect.cpp:16630-16732).
fn step_sub11(e: &mut Effect, ctx: &mut Ctx) {
    let rate = rate_of(e, 24);
    let t = tracks(e);
    // `if (scale list not empty) GetAlphaCurrent` (empty for subtype 11).
    if !t.scale.is_empty()
        && let Some(a) = getf(&t.alpha, rate)
    {
        e.alpha = a;
    }
    if (0.0..=0.5).contains(&rate) {
        let points = attached_bone_points(e, ctx);
        ctx.add_terrain_light(e.position.x, e.position.y, Vec3::new(0.9, 0.4, 0.1), 2.0);
        match e.ty {
            LEFT01 | RIGHT01 => {
                for &point in &points {
                    if ctx.rand() % 2 == 0 {
                        ctx.create_particle(BITMAP_POUNDING_BALL, point, e.angle, Vec3::ONE, 3, 1.3, None);
                    }
                }
            }
            LEFT02 | RIGHT02 => {
                let light = Vec3::new(0.3, 0.4, 1.0);
                for &point in &points {
                    if ctx.rand() % 2 == 0 {
                        ctx.create_particle(BITMAP_FIRE_HIK1_MONO, point, e.angle, light, 10, e.scale, None);
                    }
                }
            }
            MAIN01 => {
                for &point in &points {
                    if ctx.rand() % 20 == 0 {
                        ctx.create_effect(BITMAP_FIRE_CURSEDLICH, point, e.angle, Vec3::ONE, 12, me(e), -1, 0, 0, e.scale);
                    }
                }
            }
            _ => {}
        }
    }
    if let Some(a) = getf(&t.alpha, rate) {
        e.alpha = a;
    }
}

/// Subtype 12: the end flash (ZzzEffect.cpp:16733-16796).
fn step_sub12(e: &mut Effect, ctx: &mut Ctx) {
    let rate = rate_of(e, 14);
    let t = tracks(e);
    if !t.scale.is_empty()
        && let Some(a) = getf(&t.alpha, rate)
    {
        e.alpha = a;
    }
    if (0.0..=0.6).contains(&rate) {
        let points = attached_bone_points(e, ctx);
        ctx.add_terrain_light(e.position.x, e.position.y, Vec3::new(0.9, 0.4, 0.1), 2.0);
        let light = Vec3::new(0.3, 0.4, 1.0);
        for point in points {
            if ctx.rand() % 2 == 0 {
                ctx.create_particle(BITMAP_FIRE_HIK1_MONO, point, e.angle, light, 10, e.scale, None);
            }
        }
    }
    if let Some(a) = getf(&t.alpha, rate) {
        e.alpha = a;
    }
}

/// Subtype 20: the after-image (ZzzEffect.cpp:16797-16830); its rate is `LifeTime / ExtState`.
fn step_sub20(e: &mut Effect) {
    let rate = life(e) as f32 / 10.0;
    let t = tracks(e);
    apply_tracks(e, &t, rate, false, false);
}

// --- RenderEffects (ZzzEffect.cpp:18904-19052) --------------------------------------------------------

/// `arrBoneIdxs_SwordEffectRed01/02` and `arrBoneIdxs_SwordEffectEdge01..09`
/// (ZzzEffect.cpp:18969-18979), by `Type - MODEL_SWORDLEFT01...`: the two jewel bones, then the
/// nine edge bones.
const SWORD_LIGHT_BONES: [[usize; 5]; 11] = [
    [2, 9, 1, 3, 10],
    [3, 10, 2, 10, 11],
    [13, 13, 3, 4, 1],
    [12, 6, 4, 12, 2],
    [5, 1, 5, 2, 3],
    [6, 7, 6, 9, 4],
    [1, 2, 12, 5, 5],
    [8, 4, 8, 7, 6],
    [9, 3, 9, 6, 7],
    [10, 11, 10, 8, 8],
    [4, 8, 7, 1, 9],
];

/// The swords (ZzzEffect.cpp:18904-19052): attached to the owner's bone when `ChromeEnable`
/// (`AnimationTransformWithAttachHighModel_usingGlobalTM`: the vertices follow the owner's bone,
/// without any body scale), otherwise at the effect (`Animation` with `BodyOrigin = Position`,
/// `BodyScale = Scale`). Mesh 0 `RENDER_TEXTURE` and mesh 1 `RENDER_TEXTURE|RENDER_BRIGHT` with
/// `BodyLight = 1` and `o->Alpha`, mesh 2 `RENDER_TEXTURE|RENDER_BRIGHT` with `BodyLight = (0.2,
/// 0.2, 1)` and Alpha 1 (the `o->Alpha = 1` writes are done in the move). Then, except the
/// after-image, red lights on the two jewel bones and blue ones on the nine edge bones.
fn sword_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    let (position, angle, scale, frame, bones) = if chrome(e) {
        // The original requires an owner here.
        let Some(bone) = gaion_bone(e.ty) else { return };
        let Some((p, rot)) = owner_attach(e.owner, ctx.actors, bone) else { return };
        let (z, y, x) = Quat::from_mat3(&rot).to_euler(EulerRot::ZYX);
        let deg = 180.0 / std::f32::consts::PI;
        let bones = attach_bones(ctx.effect_bones(&frame0(e)), Some((p, rot)));
        (p, Vec3::new(x, y, z) * deg, 1.0, 0.0, bones)
    } else {
        // `AnimationTransformOnlySelf(arr, o)` (ZzzBMD.cpp:587): the bones as just posed.
        let bones = ctx.effect_bones(e).map(|b| bone_origins(&b).map(|t| e.position + rotate(e.angle, t * e.scale)).collect());
        (e.position, e.angle, e.scale, e.animation_frame, bones)
    };
    let mesh = |i: i32, light: Vec3, alpha: f32, flags: u32| {
        let mut d = ModelDraw::new(e.ty, position, angle, scale);
        d.light = light;
        d.alpha = alpha;
        d.action = e.current_action;
        d.frame = frame;
        d.blend_mesh = e.blend_mesh;
        d.blend_mesh_light = e.blend_mesh_light;
        d.blend_mesh_tex_coord = e.blend_mesh_tex_coord;
        d.mesh = i;
        d.render_flags = flags;
        d
    };
    out.models.push(mesh(0, Vec3::ONE, e.alpha, RENDER_TEXTURE));
    out.models.push(mesh(1, Vec3::ONE, e.alpha, RENDER_TEXTURE | RENDER_BRIGHT));
    out.models.push(mesh(2, Vec3::new(0.2, 0.2, 1.0), 1.0, RENDER_TEXTURE | RENDER_BRIGHT));

    if e.sub_type != 20 {
        let Ok(index) = usize::try_from(e.ty - LEFT01) else { return };
        // Without the model file the lights sit at the sword's origin.
        let at = |k: usize| {
            let bone = SWORD_LIGHT_BONES[k].get(index).copied();
            bone.and_then(|b| bones.as_ref()?.get(b).copied()).unwrap_or(position)
        };
        let red = Vec3::new(1.0, 0.4, 0.1);
        for k in 0..2 {
            sprite_glow(out, ctx, BITMAP_LIGHT_RED, at(k), 1.3, red, 0.0);
        }
        let blue = Vec3::new(0.3, 0.3, 1.0);
        for k in 2..9 {
            sprite_glow(out, ctx, BITMAP_LIGHT, at(k), 1.5, blue, 0.0);
        }
        for k in 9..11 {
            sprite_glow(out, ctx, BITMAP_LIGHT, at(k), 1.0, blue, 0.0);
        }
    }
}
