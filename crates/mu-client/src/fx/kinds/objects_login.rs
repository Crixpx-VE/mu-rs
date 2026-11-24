//! Map objects with effects on the login screen (`WD_73NEW_LOGIN_SCENE`, folder World74)
//! and the character selection (`WD_74NEW_CHARACTER_SCENE`, folder World75).
//!
//! In the original these two maps have no code of their own: `GMNewTown.cpp` (`IsNewMap73_74`) forwards
//! `CreateObject`, `MoveObject`, `UpdateObjectEffects`, `RenderObject` and
//! `RenderObjectAfterCharacter` to `GMEmpireGuardian4.cpp`, because `CMapManager::IsEmpireGuardian4()`
//! is also true for 73 and 74. This holds the effect-creating parts of `GMEmpireGuardian4::CreateObject`
//! (≈38-67), `MoveObject` (≈84-176) and `UpdateObjectEffects` (≈670-984); drawing (hidden meshes,
//! additive meshes, body light, animation speed) is in `mu_data::objects::model_rules` (maps 74
//! and 75), and the login camera distance fade (`CullObjects`, `ZzzObject.cpp` ≈4205-4246) in
//! `objects.rs`.
//!
//! Sounds: none. `GMNewTown::PlayObjectSound` is only called in `WD_51HOME_6TH_CHAR`, and
//! `GMEmpireGuardian4::PlayObjectSound` (`SOUND_EMPIREGUARDIAN_INDOOR_SOUND`) is only called by
//! `MapProcess::MoveObject` for registered maps, and 73/74 are not registered (only `WD_72`).
//! `Data/Maps/ObjectSounds.json` has nothing for 73/74 either.
//!
//! On the login screen the original only releases effects for objects drawn that frame
//! (`MarkDrawn`: within `RENDER_OBJECT_DIST` of the camera): `objects.rs` records it in
//! `user[`[`SLOT_CULLED`]`]` (1 = not drawn) and nothing is created meanwhile.

use bevy::math::Vec3;

use crate::fx::core::*;

// `gMapManager.WorldActive`.
const WD_73NEW_LOGIN_SCENE: i32 = 73;
const WD_74NEW_CHARACTER_SCENE: i32 = 74;

/// Free field of [`MapObject::user`] written by `objects.rs` on the login screen: 1 if `CullObjects`
/// doesn't draw the object this frame (far from the camera), 0 if it does.
pub const SLOT_CULLED: usize = 3;

/// Types with code in `GMEmpireGuardian4::MoveObject` / `UpdateObjectEffects` / `CreateObject`.
/// TODO(original): 10 (`SubType = rand() % 50` in `CreateObject`, twice because the
/// `MapProcess::CreateObject` chain also calls it; then `MoveObject` keeps the animation stopped at
/// frame 0 for those ticks): `objects.rs` animates objects in groups and can't delay one.
/// TODO(original): 115/117 (`g_EmpireGuardian1.UpdateProjectileLauncher`: creates `MODEL_PROJECTILE`
/// with `SubType` 100/101), not in the engine; there are none in World74/75.
/// 157 (lamp with candles) does nothing in 73/74: `UpdateObjectEffects` returns early.
const TYPES: [i32; 17] = [12, 20, 37, 50, 64, 79, 80, 82, 83, 84, 85, 86, 129, 130, 131, 132, 158];

pub fn register(r: &mut Registry) {
    for world in [WD_73NEW_LOGIN_SCENE, WD_74NEW_CHARACTER_SCENE] {
        for ty in TYPES {
            r.object(world, ty, empire_guardian4);
        }
    }
}

/// `Random::Rand() % n` as floating point.
fn rf(ctx: &mut Ctx, n: i32) -> f32 {
    (ctx.rand() % n) as f32
}

/// `o->AnimationFrame` within action 0 (as left by `PlayAnimation`, which wraps around).
fn anim_frame(o: &MapObject) -> f32 {
    let Some(act) = o.model.as_ref().and_then(|m| m.actions.first()) else { return 0.0 };
    let frames = if act.lock_positions.is_some() { act.frames.saturating_sub(1) } else { act.frames };
    if frames <= 1 {
        return 0.0;
    }
    o.animation_frame.rem_euclid(frames as f32)
}

fn empire_guardian4(o: &mut MapObject, ctx: &mut Ctx) {
    // CreateObject (once): 129-132 keep the rotation in whole degrees (`(int)Angle[2] % 360`).
    if !o.created && (129..=132).contains(&o.ty) {
        o.angle.z = ((o.angle.z as i32) % 360) as f32;
    }

    // MoveObject: fire, water and smoke points are not drawn.
    if matches!(o.ty, 79 | 80 | 82..=86 | 129..=132) {
        o.hidden_mesh = -2;
    }

    // UpdateObjectVisual: only objects drawn this frame.
    if ctx.world_active == WD_73NEW_LOGIN_SCENE && o.user[SLOT_CULLED] != 0.0 {
        return;
    }
    // `Luminosity` at the start of `UpdateObjectVisual` (always consumes a `rand()`).
    let _luminosity = rf(ctx, 30);

    let wt = ctx.world_time;
    match o.ty {
        12 => {
            // `BITMAP_SHINY + 5` (without `LDS_FIX_ACCESS_INDEXNUMBER_ALREADY_LOADTEXTURE`).
            let flumi = (wt * 0.0008).sin().abs() * 0.9 + 0.1;
            let scale = o.scale * 0.3 * flumi;
            let light1 = Vec3::splat(flumi);
            let light2 = Vec3::new(0.9, 0.1, 0.1);
            let pos = o.bone_world(2, Vec3::new(8.0, -3.0, -3.0));
            ctx.create_sprite(BITMAP_SHINY + 5, pos, 0.5, light2, 0.0, 0);
            ctx.create_sprite(BITMAP_SHINY + 5, pos, scale, light1, 0.0, 0);
            let pos = o.bone_world(3, Vec3::new(3.0, -3.0, -3.5));
            ctx.create_sprite(BITMAP_SHINY + 5, pos, 0.5, light2, 0.0, 0);
            ctx.create_sprite(BITMAP_SHINY + 5, pos, scale, light1, 0.0, 0);
        }
        20 => {
            // Sparks on hit (two animation moments, one rotated 180°).
            let frame = anim_frame(o);
            let turn = if frame > 5.4 && frame < 6.5 {
                Some(180.0)
            } else if frame > 15.4 && frame < 16.5 {
                Some(0.0)
            } else {
                None
            };
            if let Some(turn) = turn {
                let mut angle = Vec3::ZERO;
                for _ in 0..4 {
                    angle = Vec3::new((ctx.rand() % 60 + 60 + 90) as f32, 0.0, o.angle.z + turn);
                    // `o` as `Target`: subtype 5 doesn't use it.
                    ctx.create_joint(BITMAP_JOINT_SPARK, o.position, o.position, angle, 5, None, 10.0);
                }
                ctx.create_particle(BITMAP_SPARK, o.position, angle, o.light, 11, 1.0, None);
            }
        }
        37 => {
            let pos = o.bone_world(1, Vec3::ZERO);
            let lumi = ((wt * 0.039).sin() + 1.0) * 0.2 + 0.6;
            ctx.create_sprite(BITMAP_FLARE, pos, 4.0 * o.scale, Vec3::splat(lumi * 0.7), 0.0, 0);
        }
        50 => {
            let angle = Vec3::new(0.0, -1.0, 0.0);
            let light1 = Vec3::new(0.05, 0.1, 0.3);
            let light2 = Vec3::ONE;
            for bone in 2..=7 {
                let pos = o.bone_world(bone, Vec3::new(0.0, 0.0, 5.0));
                ctx.create_particle(BITMAP_FIRE_HIK3_MONO, pos, angle, light1, 4, o.scale * 0.6, None);
                ctx.create_particle(BITMAP_FIRE_HIK3_MONO, pos, angle, light2, 4, o.scale * 0.3, None);
            }
        }
        64 => {
            let frame = anim_frame(o);
            if (frame > 9.5 && frame < 11.5) || (frame > 23.5 && frame < 25.5) {
                let angle = Vec3::new(0.0, 0.0, o.angle.z + 90.0);
                let position = rotate(angle, Vec3::new(0.0, 30.0, 0.0)) + o.position;
                let light = Vec3::new(0.04, 0.03, 0.02);
                for _ in 0..3 {
                    // `o` as owner: subtype 22 doesn't use it.
                    ctx.create_particle(BITMAP_CLOUD, position, o.angle, light, 22, o.scale, None);
                }
            }
        }
        79 => {
            // Fire: a red glow and a flame of three random kinds.
            ctx.create_sprite(BITMAP_LIGHT, o.position, 2.0 * o.scale, Vec3::new(1.0, 0.2, 0.0), 0.0, 0);
            let light = Vec3::ONE;
            match ctx.rand() % 3 {
                0 => ctx.create_particle(BITMAP_FIRE_HIK1, o.position, o.angle, light, 0, o.scale, None),
                1 => ctx.create_particle(BITMAP_FIRE_CURSEDLICH, o.position, o.angle, light, 4, o.scale, None),
                _ => ctx.create_particle(BITMAP_FIRE_HIK3, o.position, o.angle, light, 0, o.scale, None),
            }
        }
        80 => {
            let lumi = ((wt * 0.04).sin() + 1.0) * 0.3 + 0.4;
            let light = Vec3::new(lumi * 0.1, lumi * 0.1, lumi * 0.5);
            ctx.create_sprite(BITMAP_LIGHT, o.position, 8.0 * o.scale, light, 0.0, 0);
        }
        82 => {
            // Uninitialized `Light` in the original: subtype 9 sets its own.
            ctx.create_particle(BITMAP_WATERFALL_5, o.position, o.angle, Vec3::ZERO, 9, o.scale, None);
        }
        83 => {
            ctx.create_particle(BITMAP_WATERFALL_3, o.position, o.angle, Vec3::ONE, 14, o.scale, None);
        }
        84 => {
            if ctx.rand_fps_check(8) {
                ctx.create_particle(BITMAP_WATERFALL_2, o.position, o.angle, Vec3::ONE, 4, o.scale, None);
            }
        }
        85 => {
            let light = Vec3::splat(0.5);
            let remainder = (wt as i32) % 200;
            if (0..=1).contains(&remainder) {
                let n = ctx.rand() % 4 + 4;
                for _ in 0..n {
                    // `CreateEffectFpsChecked(BITMAP_FLAME, ..., 6, NULL, -1, 0, o->Scale)`: the scale
                    // lands in the `Skill` argument (WORD); the effect keeps the default scale.
                    let kind = (o.scale as i32) & 0xFFFF;
                    ctx.create_effect(BITMAP_FLAME, o.position, o.angle, light, 6, None, -1, kind, 0, 0.0);
                }
            }
        }
        86 | 129 | 130 => {
            // Colored mist rising from the ground.
            if ctx.rand_fps_check(6) {
                let light = match o.ty {
                    86 => Vec3::new(0.05, 0.02, 0.01),
                    129 => Vec3::new(0.01, 0.02, 0.05),
                    _ => Vec3::new(0.01, 0.05, 0.02),
                };
                // `o` as owner: subtype 21 doesn't use it.
                ctx.create_particle(BITMAP_CLOUD, o.position, o.angle, light, 21, o.scale, None);
            }
        }
        131 => {
            if ctx.rand_fps_check(3) {
                ctx.create_particle(BITMAP_SMOKE, o.position, o.angle, Vec3::ONE, 22, o.scale, None);
                ctx.create_particle(BITMAP_SMOKE, o.position, o.angle, Vec3::ONE, 21, o.scale * 2.0, None);
            }
        }
        132 => {
            if ctx.rand_fps_check(3) {
                ctx.create_particle(BITMAP_SMOKE, o.position, o.angle, Vec3::ONE, 60, o.scale, None);
                ctx.create_particle(BITMAP_SMOKE, o.position, o.angle, Vec3::ONE, 60, o.scale, None);
                ctx.create_particle(BITMAP_SMOKE, o.position, o.angle, Vec3::ONE, 21, o.scale * 2.0, None);
            }
        }
        158 => {
            // Uninitialized `Light` in the original: subtype 64 computes its own while moving.
            ctx.create_particle(BITMAP_SMOKE, o.position, o.angle, Vec3::ZERO, 64, o.scale, None);
        }
        _ => {}
    }
}
