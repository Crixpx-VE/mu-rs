//! Dark Knight effects (`Effects[]`) translated from `ZzzEffect.cpp` (`CreateEffect`,
//! `MoveEffect`, `RenderEffects`/`DescribeEffectShadow`), `Behaviors/MoveHandlers.cpp`,
//! `Behaviors/EffectBehaviors.cpp` and `Data/Effects/EffectTypes.json`:
//! Twisting Slash (`MODEL_SKILL_WHEEL1/2`), the Death Stab/Impale emitters (`MODEL_SPEAR`,
//! `MODEL__SPEAR`), the Impale spears (`MODEL_SPEARSKILL`) and the ground mark `BITMAP_MAGIC+1`.
//!
//! Note: when a kind has a `step`, the engine skips the common `MoveEffect` tail
//! (`PlayAnimation` + `MoveParticle`), so each `step` does it by hand where needed.

use bevy::math::{Vec2, Vec3};

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

/// `Luminosity` from the start of `MoveEffect` (consumes one `rand()` per effect, like the original).
fn luminosity(e: &Effect, ctx: &mut Ctx) -> f32 {
    let mut l = (ctx.rand() % 4 + 7) as f32 * 0.1;
    if e.life_time < 5.0 {
        l -= (5.0 - e.life_time) * 0.2;
        if l < 0.0 {
            l = 0.0;
        }
    }
    l
}

/// `MoveParticle(o, true)`: move along `Direction` rotated by `Angle`.
fn move_particle_turn(e: &mut Effect) {
    e.position += rotate(e.angle, e.direction);
}

/// `RenderEffects` draws nothing for these kinds (outside `MODEL_SKILL_BEGIN..END` or `case: break`).
fn draw_nothing(_e: &Effect, _ctx: &DrawCtx, _out: &mut DrawList) {}

/// Owner entity, if it is a character.
fn owner_actor<'a>(owner: Option<Target>, actors: &'a std::collections::HashMap<bevy::prelude::Entity, Actor>) -> Option<&'a Actor> {
    match owner? {
        Target::Actor(e) => actors.get(&e),
        Target::Effect(_) => None,
    }
}

/// `WD_24HELLAS..=WD_24HELLAS_END` (24..=29) or `WD_24HELLAS_7` (36): `gMapManager.InHellas()`.
fn in_hellas(world: i32) -> bool {
    (24..=29).contains(&world) || world == 36
}

/// `MAX_ITEM_INDEX`: items per group.
const MAX_ITEM_INDEX: i32 = 512;

// --- MODEL_SKILL_WHEEL1 (Twisting Slash: the emitter) ----------------------------------------------

fn wheel1_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 5.0;
    // TODO(original): CharacterMachine->PacketSerial++ (network).
}

fn wheel1_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    // A new wheel every tick; the subtype (4 − LifeTime) gives its alpha.
    ctx.create_effect(MODEL_SKILL_WHEEL2, e.position, e.angle, e.light, 4 - e.life_time as i32, e.owner, e.pk_key, e.kind, e.skill, 0.0);
    // WHEEL1 is excluded from the common tail (no animation, no MoveParticle).
}

// --- MODEL_SKILL_WHEEL2 (Twisting Slash: the spinning ghost sword) ---------------------------------

fn wheel2_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 25.0;
    // TODO(original): o->Weapon = CharacterMachine->PacketSerial (network).
    // user[0] = Direction[2] of the previous tick; user[1] = Angle[2] after the tick (to interpolate when drawing).
    e.user[0] = e.direction.z;
    e.user[1] = e.angle.z;
}

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

    if ctx.rand_fps_check(1) {
        ctx.create_particle(BITMAP_SMOKE, e.position, e.angle, e.light, 3, 1.0, None);
        let light = Vec3::splat(luminosity * 0.3);
        ctx.add_terrain_light(e.position.x, e.position.y, light, 3.0);
    }

    let mut light = Vec3::ONE;
    let mut height = 20.0;
    if in_hellas(ctx.world_active) {
        height = 60.0;
        let mut angle = Vec3::new((ctx.rand() % 60 + 60 - 90) as f32, 0.0, (ctx.rand() % 30 + 90) as f32);
        angle += e.angle;
        let mut position = e.position;
        position.x += (ctx.rand() % 20 - 10) as f32;
        position.y += (ctx.rand() % 20 - 10) as f32;
        position.z += 120.0;
        light = Vec3::splat(0.5);
        if ctx.rand_fps_check(1) {
            ctx.create_particle(BITMAP_WATERFALL_5, position, angle, light, 3, 1.0, None);
        }
    } else {
        for _ in 0..4 {
            if ctx.rand_fps_check(1) {
                let mut angle = Vec3::new((ctx.rand() % 60 + 60 - 90) as f32, 0.0, (ctx.rand() % 30 + 90) as f32);
                angle += e.angle;
                let mut position = e.position;
                position.x += (ctx.rand() % 20 - 10) as f32;
                position.y += (ctx.rand() % 20 - 10) as f32;
                ctx.create_joint(BITMAP_JOINT_SPARK, position, position, angle, 0, None, 10.0);
                if ctx.rand() % 4 == 0 {
                    ctx.create_particle(BITMAP_SPARK, position, angle, light, 0, 1.0, None);
                }
            }
        }
    }
    let mut position = e.position;
    position.z += height;
    ctx.create_sprite(BITMAP_LIGHT, position, 2.0, Vec3::new(1.0, 0.8, 0.6), 0.0, 0);

    // TODO(original): in Hellas, every 4 ticks of life AddWaterWave(x, y, 2, -150) (no water yet).

    // `UpdateDrawnEffectSprites`: the wheel spins faster and faster.
    e.user[0] = e.direction.z;
    e.direction.z -= 30.0;
    e.user[1] = e.angle.z;
}

/// `RenderWheelWeapon`: the owner's WEAPON MODEL, rotated and raised by 100.
fn wheel2_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    let Some(actor) = owner_actor(e.owner, ctx.actors) else { return };
    let Some((model, _level)) = actor.weapon else { return };

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
    let scale = if (MODEL_SPEAR..=MODEL_PLATINA_STAFF).contains(&model) { 0.7 } else { 0.8 };
    // `RequestModelTerrainLight + o->Light(0.3)`.
    // TODO(original): no terrain light at draw time; approximated with 0.7.
    const TERRAIN_LIGHT: f32 = 0.7;
    let mut d = ModelDraw::new(model, position, angle, scale);
    d.light = Vec3::splat(TERRAIN_LIGHT + 0.3);
    d.alpha = e.alpha;
    // TODO(original): RenderPartObject with WeaponLevel (weapon level glow) and b->Skin per class.
    out.models.push(d);
}

// --- MODEL_SPEAR ("lightSpear": invisible Death Stab/Impale emitter) -------------------------------

fn spear_create(e: &mut Effect, _ctx: &mut Ctx) {
    // EffectTypes.json: lifeTime 10.
    e.life_time = 10.0;
    e.visible = false;
}

fn spear_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    // `Behaviors::MoveSpear`: leaves a BITMAP_FLARE trail that follows it.
    let me = Some(Target::Effect(e.id));
    if e.sub_type == 1 {
        if ctx.rand_fps_check(1) {
            ctx.create_joint(BITMAP_FLARE, e.position, e.position, e.angle, 12, me, 100.0);
        }
    } else if e.sub_type == 0 && ctx.rand_fps_check(1) {
        ctx.create_joint(BITMAP_FLARE, e.position, e.position, e.angle, 4, me, 50.0);
    }
    // Common tail: MoveParticle(o, true).
    move_particle_turn(e);
}

// --- MODEL__SPEAR (Impale: wind-up) ----------------------------------------------------------------

fn spear2_create(e: &mut Effect, _ctx: &mut Ctx) {
    // EffectTypes.json: lifeTime 5, angle [0, 0, 0].
    e.life_time = 5.0;
    e.angle = Vec3::ZERO;
    e.visible = false;
}

fn spear2_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    // `Move_MODEL__SPEAR`: three healing trails converging on it.
    let p = Vec3::new(0.0, -100.0, 0.0);
    for _ in 0..3 {
        let angle = Vec3::new((ctx.rand() % 90) as f32, 0.0, (ctx.rand() % 360) as f32);
        let position = e.position - rotate(angle, p);
        if ctx.rand_fps_check(1) {
            ctx.create_joint(BITMAP_JOINT_HEALING, position, e.position, angle, 6, None, 5.0);
        }
    }
    move_particle_turn(e);
}

// --- MODEL_SPEARSKILL (Impale: the spears) ---------------------------------------------------------

fn spearskill_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 20.0;
    e.scale = 1.5;
    e.blend_mesh_light = 1.0;
    let a = e.angle.z.to_radians();
    e.direction.x = 5.0 * a.sin();
    e.direction.y = -5.0 * a.cos();
}

fn spearskill_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    // Move_MODEL_SPEARSKILL does nothing; common tail: PlayAnimation and MoveParticle(o, false).
    // TODO(original): PlayAnimation stops at the end of a non-looping action (CurrentAction = 255).
    e.animation_frame += e.velocity;
    e.position += e.direction;
}

/// `RenderSkillSpear`: additive RidingSpear01 (`RENDER_BRIGHT`) with light · LifeTime · 0.05.
fn spearskill_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    let mut d = ModelDraw::of_effect(e);
    d.model = MODEL_SPEARSKILL;
    d.light = e.light * e.life_time * 0.05;
    d.alpha = 1.0;
    d.bright = true;
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

/// `CreateHealing`: three `BITMAP_JOINT_HEALING` trails descending toward the effect.
fn create_healing(e: &Effect, ctx: &mut Ctx) {
    let p = Vec3::new(0.0, -200.0, 0.0);
    for _ in 0..3 {
        let angle = Vec3::new((ctx.rand() % 90) as f32, 0.0, (ctx.rand() % 360) as f32);
        let mut position = e.position - rotate(angle, p);
        position.z += 120.0;
        let sub = if e.sub_type == 5 { 11 } else { e.sub_type };
        ctx.create_joint(BITMAP_JOINT_HEALING, position, e.position, angle, sub, e.owner, 5.0);
    }
}

fn magic1_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    let s = e.sub_type;
    // (Without ENABLE_POTION_EFFECT the `else if` hangs off the first `if`.)
    if s >= 1 && !matches!(s, 4 | 6 | 7 | 8 | 9 | 10 | 11 | 12 | 13) {
        create_healing(e, ctx);
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
                // TODO(original): Dummy bones are skipped (unknown which ones they are in Actor).
                let p = Vec3::new(
                    (-50 + ctx.rand() % 100) as f32,
                    (-10 + ctx.rand() % 20) as f32,
                    (-10 + ctx.rand() % 20) as f32,
                );
                let position = owner.bone_world(index, p) - owner.position + e.start_position;
                if ctx.rand_fps_check(1) {
                    ctx.create_particle(BITMAP_LIGHT, position, owner.angle, light, 8, 1.0, None);
                }
            }
            // TODO(original): o->Owner->Alpha = LifeTime / 80 (the character fades out).
        }
    } else if s == 9 {
        e.scale += 0.3;
        e.light *= 1.0 / 1.1;
    }
    // Common tail: MoveParticle(o, true).
    move_particle_turn(e);

    // `UpdateDrawnEffectShadowState` (after the tick's `LifeTime -= 1`).
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
        13 => e.light *= 1.0 / 1.05,
        _ => {}
    }
}

/// `DescribeEffectShadow` for `BITMAP_MAGIC+1`: the mark stuck to the ground.
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
    // Decal(BITMAP_MAGIC+1, x, y, Scale, Scale, Light, -Angle[2]): size in tiles (×100),
    // 5 above the ground. The original rotates the UV by -Angle[2], i.e. the image by +Angle[2].
    out.decals.push(DecalDraw {
        texture: BITMAP_MAGIC + 1,
        center: Vec2::new(e.position.x, e.position.y),
        width: scale * 100.0,
        height: scale * 100.0,
        color: [light.x, light.y, light.z, 1.0],
        rotation: e.angle.z,
        lift: 5.0,
        blend,
    });
}
