//! Dark Knight effects (`Effects[]`) translated from `ZzzEffect.cpp` (`CreateEffect`,
//! `MoveEffect`, `RenderEffects`/`DescribeEffectShadow`), `Behaviors/MoveHandlers.cpp`,
//! `Behaviors/EffectBehaviors.cpp` and `Data/Effects/EffectTypes.json`:
//! Twisting Slash (`MODEL_SKILL_WHEEL1/2`), the Death Stab/Impale emitters (`MODEL_SPEAR`,
//! `MODEL__SPEAR`), the Impale spears (`MODEL_SPEARSKILL`) and the ground mark `BITMAP_MAGIC+1`.
//!
//! Note: when a kind has a `step`, the engine skips the common `MoveEffect` tail
//! (`PlayAnimation` + `MoveParticle`), so each `step` does it by hand where needed.
//! Main 5.2 line numbers are cited as `ZzzEffect.cpp:NNNN`; where it differs from MuMain, Main 5.2
//! is followed.

use bevy::math::{Vec2, Vec3};
use mu_data::items::Item;
use mu_data::items::glow::{ItemCtx, ItemPass, item_passes};
use mu_data::items::model::item_object_meshes;

use super::common::{self, draw_nothing, luminosity, owner_actor};
use crate::fx::core::*;

pub fn register(r: &mut Registry) {
    r.effect(MODEL_SKILL_WHEEL1, EffectKind { create: Some(wheel1_create), step: Some(wheel1_step), draw: Some(draw_nothing) });
    r.effect(MODEL_SKILL_WHEEL2, EffectKind { create: Some(wheel2_create), step: Some(wheel2_step), draw: Some(wheel2_draw) });
    r.effect(MODEL_SPEAR, EffectKind { create: Some(spear_create), step: Some(spear_step), draw: Some(draw_nothing) });
    r.effect(MODEL__SPEAR, EffectKind { create: Some(spear2_create), step: Some(spear2_step), draw: Some(draw_nothing) });
    r.effect(
        MODEL_SPEARSKILL,
        EffectKind { create: Some(spearskill_create), step: Some(spearskill_step), draw: Some(spearskill_draw) },
    );
    r.effect(BITMAP_MAGIC + 1, EffectKind { create: Some(magic1_create), step: Some(magic1_step), draw: Some(magic1_draw) });
}

// --- Common ----------------------------------------------------------------------------------------

/// `MAX_ITEM_INDEX`: items per group.
const MAX_ITEM_INDEX: i32 = 512;

/// `ItemObjectAttribute(o)` on the effect drawn as a weapon (ZzzObject.cpp:5306-5309): scale 0.8,
/// 0.7 for `MODEL_SPEAR..=MODEL_STAFF+13`.
pub(super) fn weapon_scale(model: i32) -> f32 {
    if (MODEL_SPEAR..=MODEL_STAFF + 13).contains(&model) { 0.7 } else { 0.8 }
}

/// `RenderPartObject(o, Type, NULL, Light, Alpha, Owner->WeaponLevel<<3, 0, 0, true, true, true)`
/// (ZzzObject.cpp:10469) for the owner's weapon drawn by an effect (`RenderWheelWeapon`,
/// `RenderFuryStrike`): nothing at `Alpha <= 0.01`, then the general branch of
/// `RenderPartObjectEffect` (ZzzObject.cpp:10132-10275, `mu_data::items::glow::item_passes`) with
/// no excellent/ancient option. `RenderPartObjectBody`/`BodyColor` draw at `Alpha`,
/// `RenderPartObjectBodyColor2` (the CHROME2/CHROME4 passes) at 1 (ZzzObject.cpp:10229, 10245);
/// `o->BlendMesh` is the one `ItemObjectAttribute` gives the weapon.
/// The per-item branches of `RenderPartObjectBody` come with `item_passes` (`mu_data::items::body`).
/// TODO(original): the pulsing/random `BlendMeshLight` of `ItemObjectAttribute` is not reproduced, nor `b->Skin =
/// GetBaseClass(Hero->Class)` (class-dependent textures).
#[allow(clippy::too_many_arguments)]
pub(super) fn render_weapon(out: &mut DrawList, ctx: &DrawCtx, model: i32, level: u8, position: Vec3, angle: Vec3, scale: f32, light: Vec3, alpha: f32) {
    if alpha <= 0.01 {
        return;
    }
    let rel = model - MODEL_ITEM;
    if rel < 0 {
        return;
    }
    let item = Item { level, ..Item::new((rel / MAX_ITEM_INDEX) as u8, (rel % MAX_ITEM_INDEX) as u16) };
    let (blend_mesh, hidden_mesh) = item_object_meshes(&item);
    // `num_meshes` only matters for the inventory armour textures (`body.rs`), never a weapon.
    let item_ctx = ItemCtx { hide_skin: false, num_meshes: 0, white_light: false };
    for ItemPass { pass, light: pass_light, .. } in item_passes(&item, light.to_array(), ctx.render_level, &item_ctx) {
        // A pass whose texture has no `BITMAP_*` id here is skipped rather than drawn with the
        // wrong texture (only the inventory armour skins, never a weapon).
        let texture = match pass.texture {
            None => -1,
            Some(b) => match bitmap_id(b) {
                Some(id) => id,
                None => continue,
            },
        };
        let mut d = ModelDraw::new(model, position, angle, scale);
        d.texture = texture;
        d.light = Vec3::from_array(pass_light.at(ctx.world_time));
        d.render_flags = pass.flags;
        d.alpha = if pass.flags & (RENDER_CHROME2 | RENDER_CHROME4) != 0 { 1.0 } else { pass.alpha.unwrap_or(alpha) };
        d.blend_mesh = pass.blend_mesh.unwrap_or(blend_mesh);
        d.hidden_mesh = pass.hidden_mesh.unwrap_or(hidden_mesh);
        d.mesh = pass.mesh.map_or(-1, |m| m as i32);
        if let Some([u, v]) = pass.tex_coord {
            d.blend_mesh_tex_coord = Vec2::new(u.at(ctx.world_time), v.at(ctx.world_time));
        }
        out.models.push(d);
    }
}

/// The `BITMAP_*` id of a texture `RenderPartObjectBody` swaps in (`mu_data::objects::render::Bitmap`).
/// `None` for the ones the effect textures table does not hold (the per-file inventory skins,
/// the map caustics and `BITMAP_CHROME8`, which only some maps load).
fn bitmap_id(b: mu_data::objects::render::Bitmap) -> Option<i32> {
    use mu_data::objects::render::Bitmap;
    Some(match b {
        Bitmap::Chrome => BITMAP_CHROME,
        Bitmap::Chrome2 => BITMAP_CHROME2,
        Bitmap::Chrome3 => BITMAP_CHROME3,
        Bitmap::Chrome6 => BITMAP_CHROME6,
        Bitmap::Chrome7 => BITMAP_CHROME7,
        Bitmap::Shiny => BITMAP_SHINY,
        Bitmap::Chrome8 | Bitmap::Water | Bitmap::File { .. } => return None,
    })
}

// --- MODEL_SKILL_WHEEL1 (Twisting Slash: the emitter) ----------------------------------------------

/// ZzzEffect.cpp:2039-2042.
fn wheel1_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = 5.0;
    ctx.next_packet_serial();
}

/// ZzzEffect.cpp:9887-9889.
fn wheel1_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    // A new wheel every tick; the subtype (4 − LifeTime) gives its alpha.
    ctx.create_effect(MODEL_SKILL_WHEEL2, e.position, e.angle, e.light, 4 - e.life_time as i32, e.owner, e.pk_key, e.kind, e.skill, 0.0);
    // WHEEL1 is excluded from the common tail (no animation, no MoveParticle, ZzzEffect.cpp:18061).
}

// --- MODEL_SKILL_WHEEL2 (Twisting Slash: the spinning ghost sword) ---------------------------------

/// ZzzEffect.cpp:2043-2046.
fn wheel2_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = 25.0;
    e.weapon = ctx.packet_serial();
    // user[0] = Direction[2] of the previous tick; user[1] = Angle[2] after the tick (to interpolate when drawing).
    e.user[0] = e.direction.z;
    e.user[1] = e.angle.z;
}

/// ZzzEffect.cpp:9890-9952.
fn wheel2_step(e: &mut Effect, ctx: &mut Ctx) {
    let luminosity = luminosity(e, ctx);
    match e.sub_type {
        1 => e.alpha = 0.6,
        2 => e.alpha = 0.5,
        3 => e.alpha = 0.4,
        4 => e.alpha = 0.3,
        _ => {}
    }
    let owner = owner_actor(e.owner, ctx.actors).map(|a| (a.position, a.weapon));
    // Spears orbit farther out.
    let is_spear = owner
        .and_then(|(_, w)| w)
        .is_some_and(|(m, _)| (MODEL_SPEAR..MODEL_SPEAR + MAX_ITEM_INDEX).contains(&m));
    let p = if is_spear { Vec3::new(0.0, -180.0, 0.0) } else { Vec3::new(0.0, -150.0, 0.0) };

    let position = rotate(e.angle, p);
    if let Some((owner_pos, _)) = owner {
        e.position = owner_pos + position;
    }
    e.angle.z -= 18.0;

    ctx.create_particle(BITMAP_SMOKE, e.position, e.angle, e.light, 3, 1.0, None);
    let light = Vec3::splat(luminosity * 0.3);
    ctx.add_terrain_light(e.position.x, e.position.y, light, 3.0);

    let light = Vec3::ONE;
    let mut height = 20.0;
    if common::in_hellas(ctx.world_active) {
        height = 60.0;
        let mut angle = Vec3::new((ctx.rand() % 60 + 60 - 90) as f32, 0.0, (ctx.rand() % 30 + 90) as f32);
        angle += e.angle;
        let mut position = e.position;
        position.x += (ctx.rand() % 20 - 10) as f32;
        position.y += (ctx.rand() % 20 - 10) as f32;
        position.z += 120.0;
        ctx.create_particle(BITMAP_WATERFALL_5, position, angle, Vec3::splat(0.5), 3, 1.0, None);
    } else {
        for j in 0..4 {
            let mut angle = Vec3::new((ctx.rand() % 60 + 60 - 90) as f32, 0.0, (ctx.rand() % 30 + 90) as f32);
            angle += e.angle;
            let mut position = e.position;
            position.x += (ctx.rand() % 20 - 10) as f32;
            position.y += (ctx.rand() % 20 - 10) as f32;
            ctx.create_joint(BITMAP_JOINT_SPARK, position, position, angle, 0, None, 10.0);
            // `if(j==0)` (ZzzEffect.cpp:9938); MuMain uses `rand()%4==0` instead.
            if j == 0 {
                ctx.create_particle(BITMAP_SPARK, position, angle, light, 0, 1.0, None);
            }
        }
    }
    let mut position = e.position;
    position.z += height;
    ctx.create_sprite(BITMAP_LIGHT, position, 2.0, Vec3::new(1.0, 0.8, 0.6), 0.0, 0);

    // TODO(original): in Hellas, every 4 ticks of life AddWaterWave(x, y, 2, -150) (no water yet).

    // `RenderWheelWeapon` (ZzzEffect.cpp:18175) does `o->Direction[2] -= 30` once per rendered
    // frame (MuMain: `UpdateDrawnEffectSprites`): done here once per tick.
    // TODO(original): its `ItemObjectAttribute(o)` (ZzzEffect.cpp:18191) also leaves the effect
    // with Light 0.3, Alpha 1, Scale 0.8/0.7, AnimationFrame/Velocity 0 after each render (the
    // smoke above then uses Light 0.3); MuMain draws a copy and keeps the effect as is, like here.
    e.user[0] = e.direction.z;
    e.direction.z -= 30.0;
    e.user[1] = e.angle.z;
}

/// `RenderWheelWeapon` (ZzzEffect.cpp:18169-18201): the owner's WEAPON MODEL, rotated and raised by
/// 100, with its level glow.
fn wheel2_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    let Some(actor) = owner_actor(e.owner, ctx.actors) else { return };
    let Some((model, level)) = actor.weapon else { return };

    // Direction[2] interpolated with the same fraction as the angle (user[0] = previous, user[1] = final Angle[2]).
    let wrap = |d: f32| (d + 540.0).rem_euclid(360.0) - 180.0;
    let full = wrap(e.user[1] - e.prev_angle.z);
    let part = wrap(e.angle.z - e.prev_angle.z);
    let f = if full.abs() > 1e-3 { (part / full).clamp(0.0, 1.0) } else { 1.0 };
    let dir_z = e.user[0] + (e.direction.z - e.user[0]) * f;

    let mut angle = e.angle;
    angle.z += dir_z;
    angle.y = 90.0;
    let mut position = e.position;
    position.z += 100.0;

    // `ItemObjectAttribute`: scale 0.8 (0.7 for spears through staffs), light 0.3, frame 0.
    let scale = weapon_scale(model);
    // `RequestTerrainLight + o->Light(0.3)`.
    let light = ctx.terrain_light(e.position.x, e.position.y) + Vec3::splat(0.3);
    // `Alpha` = `o->Alpha` saved before `ItemObjectAttribute`.
    render_weapon(out, ctx, model, level, position, angle, scale, light, e.alpha);
}

// --- MODEL_SPEAR ("lightSpear": invisible Death Stab/Impale emitter) -------------------------------

/// ZzzEffect.cpp:756-758.
fn spear_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 10.0;
    // An item model with no `case` in RenderEffects: nothing is drawn.
    e.visible = false;
}

/// ZzzEffect.cpp:7551-7560: leaves a BITMAP_FLARE trail that follows it.
fn spear_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    let me = Some(Target::Effect(e.id));
    if e.sub_type == 1 {
        ctx.create_joint(BITMAP_FLARE, e.position, e.position, e.angle, 12, me, 100.0);
    } else if e.sub_type == 0 {
        ctx.create_joint(BITMAP_FLARE, e.position, e.position, e.angle, 4, me, 50.0);
    }
    // Common tail (not in MODEL_BIRD01..MODEL_SKILL_END: MoveParticle(o, true) only).
    common::move_tail(e, ctx);
}

// --- MODEL__SPEAR (Impale: wind-up) ----------------------------------------------------------------

/// ZzzEffect.cpp:759-762.
fn spear2_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 5.0;
    e.angle = Vec3::ZERO;
    e.visible = false;
}

/// ZzzEffect.cpp:7561-7578: three healing trails converging on it.
fn spear2_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    let p = Vec3::new(0.0, -100.0, 0.0);
    for _ in 0..3 {
        let angle = Vec3::new((ctx.rand() % 90) as f32, 0.0, (ctx.rand() % 360) as f32);
        let position = e.position - rotate(angle, p);
        ctx.create_joint(BITMAP_JOINT_HEALING, position, e.position, angle, 6, None, 5.0);
    }
    common::move_tail(e, ctx);
}

// --- MODEL_SPEARSKILL (Impale: the spears) ---------------------------------------------------------

/// ZzzEffect.cpp:764-769. (`o->BlendMeshLight = 1` is set by `RenderSkillSpear` while drawing,
/// ZzzEffect.cpp:18243; nothing else reads it.)
fn spearskill_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 20.0;
    e.scale = 1.5;
    e.blend_mesh_light = 1.0;
    let a = e.angle.z.to_radians();
    e.direction.x = 5.0 * a.sin();
    e.direction.y = -5.0 * a.cos();
}

/// ZzzEffect.cpp:7713 (the case only breaks), then the common tail: `PlayAnimation` (ending the
/// action with `CurrentAction = 255`) and `MoveParticle(o, false)` (ZzzEffect.cpp:18074-18090).
fn spearskill_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    common::play_animation(e, ctx);
    common::move_particle(e, false);
}

/// `RenderSkillSpear` (ZzzEffect.cpp:18237-18248): `RenderPartObject(o, MODEL_SPEARSKILL, NULL,
/// Light · LifeTime · 0.05, 1, ..., RENDER_BRIGHT|RENDER_TEXTURE)`, level 0 (the body pass only).
fn spearskill_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    let mut d = ModelDraw::of_effect(e);
    d.model = MODEL_SPEARSKILL;
    d.light = e.light * e.life_time * 0.05;
    d.alpha = 1.0;
    d.render_flags = RENDER_TEXTURE | RENDER_BRIGHT;
    out.models.push(d);
}

// --- BITMAP_MAGIC+1 (Magic_Ground2 ground mark) ----------------------------------------------------

fn magic1_create(e: &mut Effect, ctx: &mut Ctx) {
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
            e.angle.z = (ctx.rand() % 360) as f32;
        }
        8 => e.life_time = 20.0,
        9 => {
            e.life_time = 10.0;
            e.scale = 0.1;
        }
        11 | 12 => e.life_time = 20.0,
        // o->Scale = Scale (already is).
        13 => e.life_time = 40.0,
        _ => {}
    }
    e.visible = false;
}

/// ZzzEffect.cpp:8348-8394.
fn magic1_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    let s = e.sub_type;
    // (Without ENABLE_POTION_EFFECT the `else if` hangs off the first `if`.)
    if s >= 1 && !matches!(s, 4 | 6 | 7 | 8 | 9 | 10 | 11 | 12 | 13) {
        common::create_healing(e, ctx);
    } else if s == 6 {
        // Light sparks along the owner's bones (BodyOrigin = StartPosition).
        e.blend_mesh_light = (ctx.rand() % 10) as f32 * 0.1;
        if let Some(owner) = owner_actor(e.owner, ctx.actors).cloned() {
            let num_bones = owner.bones.len() as i32;
            let light = Vec3::ONE;
            let mut i = 0;
            while (i as f32) < e.life_time && num_bones > 0 {
                i += 1;
                let index = (ctx.rand() % num_bones) as usize;
                // `if (b->Bones[iIndex].Dummy) continue;` (ZzzEffect.cpp:8379).
                if owner.is_dummy_bone(index) {
                    continue;
                }
                let p = Vec3::new(
                    (-50 + ctx.rand() % 100) as f32,
                    (-10 + ctx.rand() % 20) as f32,
                    (-10 + ctx.rand() % 20) as f32,
                );
                let position = owner.bone_world(index, p) - owner.position + e.start_position;
                ctx.create_particle(BITMAP_LIGHT, position, owner.angle, light, 8, 1.0, None);
            }
            // TODO(original): o->Owner->Alpha = LifeTime / 80 (the character fades out); the owner
            // is a read-only snapshot.
        }
    } else if s == 9 {
        e.scale += 0.3;
        e.light /= 1.1;
    }
    // Common tail (not in MODEL_BIRD01..MODEL_SKILL_END: MoveParticle(o, true) only).
    common::move_tail(e, ctx);

    // What `RenderEffectShadows` changes while drawing (ZzzEffect.cpp:19373-19437; MuMain:
    // `UpdateDrawnEffectShadowState`), once per tick with the `LifeTime` the draw will see.
    let lt = e.life_time - 1.0;
    if lt <= 0.0 {
        return;
    }
    match s {
        7 => {
            if let Some(p) = ctx.target_position(e.owner) {
                e.position = p;
            }
            e.angle.z += 10.0;
        }
        6 if lt as i32 % 25 == 0 => {
            ctx.create_effect(BITMAP_MAGIC + 1, e.position, e.angle, e.light, 7, e.owner, -1, 0, 0, 0.0);
        }
        8 if lt as i32 % 2 == 0 => {
            ctx.create_effect(BITMAP_MAGIC + 1, e.position, e.angle, e.light, 9, e.owner, -1, 0, 0, 0.0);
        }
        13 => e.light /= 1.05,
        _ => {}
    }
}

/// `RenderEffectShadows` for `BITMAP_MAGIC+1` (ZzzEffect.cpp:19333-19438): the mark stuck to the ground.
fn magic1_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    let s = e.sub_type;
    let mut luminosity = 1.0;
    if e.life_time < 5.0 {
        luminosity -= (5.0 - e.life_time) * 0.2;
    } else if s == 7 && e.life_time > 30.0 {
        luminosity = (40.0 - e.life_time) * 0.1;
    }

    let scale = match s {
        4 | 10 => {
            if luminosity == 1.0 {
                luminosity = ((60.0 - e.life_time) * 0.05).sin() * 1.0 + 0.5;
            }
            e.scale
        }
        6 => (80.0 - e.life_time) * 0.05,
        7 => e.life_time * 0.07,
        9 | 13 => e.scale,
        _ => (20.0 - e.life_time) * 0.15,
    };
    if matches!(s, 5 | 6 | 8) {
        return;
    }
    let l = luminosity;
    let mut blend = Blend::Glow;
    let light = match s {
        0 | 1 => Vec3::new(l * 0.4, l * 0.6, l * 1.0),
        2 => Vec3::new(l * 0.4, l * 1.0, l * 0.6),
        3 => Vec3::new(l * 1.0, l * 0.6, l * 0.4),
        4 => Vec3::new(l * 1.0, l * 0.5, l * 0.1),
        7 | 9 => Vec3::new(l * 0.9, l * 0.4, l * 0.2),
        10 => Vec3::new(l * 0.1, l * 0.5, l * 1.0),
        11 => e.light * l,
        12 => {
            blend = Blend::Subtract;
            e.light * l
        }
        13 => e.light,
        // In the original `Light` is left uninitialized.
        _ => Vec3::ZERO,
    };
    // RenderTerrainAlphaBitmap(BITMAP_MAGIC+1, x, y, Scale, Scale, Light, -Angle[2]) (ZzzEffect.cpp:19428).
    common::terrain_bitmap(out, BITMAP_MAGIC + 1, e.position.x, e.position.y, scale, scale, light, -e.angle.z, 1.0, 5.0, blend);
}
