//! Event effects (`Effects[]`): Halloween (`MODEL_HALLOWEEN_EX`, `MODEL_HALLOWEEN_CANDY_BLUE/
//! ORANGE/YELLOW/RED/HOBAK/STAR`), Christmas (`MODEL_XMAS_EVENT_BOX/CANDY/TREE/SOCKS`,
//! `MODEL_XMAS_EVENT_ICEHEART`, `MODEL_XMAS2008_SNOWMAN_HEAD/BODY`), New Year's day
//! (`MODEL_NEWYEARSDAY_EVENT_BEKSULKI/CANDY/MONEY/HOTPEPPER_GREEN/HOTPEPPER_RED/YUT/PIG`), Moon
//! harvest (`MODEL_MOONHARVEST_MOON`, `MODEL_MOONHARVEST_GAM/SONGPUEN1/SONGPUEN2`), Cursed Temple
//! (`MODEL_CURSEDTEMPLE_HOLYITEM`, `MODEL_CURSEDTEMPLE_PRODECTION_SKILL`,
//! `MODEL_CURSEDTEMPLE_RESTRAINT_SKILL`, `MODEL_CURSEDTEMPLE_STATUE_PART1/2`) and the PK field
//! assassin death (`MODEL_PKFIELD_ASSASSIN_EFFECT_GREEN/RED_HEAD`, `..._GREEN/RED_BODY`).
//!
//! Translated from Main 5.2 `ZzzEffect.cpp` (`CreateEffect`, `MoveEffect` and its common tail
//! 18060-18124, `RenderEffects`) and the type-specific branches of `Draw_RenderObject`
//! (`ZzzObject.cpp`, what `RenderObject(o)` runs for them) and `GM_PK_Field.cpp:1024-1035`.
//!
//! Notes:
//! - Every type here is either in `MODEL_BIRD01..MODEL_SKILL_END` (so the common tail plays its
//!   animation) or a monster/item model above it (`MODEL_XMAS2008_SNOWMAN_*`: no animation in the
//!   tail); all of them get the tail's `MoveParticle(o, true)`, so the falling items move by
//!   `Direction` twice per tick (once by hand, once rotated by `Angle`), like the original.
//! - `Draw_RenderObject` of `MODEL_CURSEDTEMPLE_HOLYITEM` creates a particle while drawing: here it
//!   runs at the START of the next `step` (before `Luminosity`), with the state the draw saw.
//!   `MODEL_MOONHARVEST_MOON`'s `o->m_iAnimation++` (done in `RenderEffects` before its sprites) runs
//!   at the END of `step`. Sprites created while drawing are pushed directly by the draw function.
//! - `CreateEffect` called from a `CreateEffect` (`MODEL_HALLOWEEN_EX`) runs the child's creation
//!   code at once, like the original, so each candy's `rand()` calls come right after the
//!   `rand()%8` that chose it.

use bevy::math::{Vec2, Vec3};

use super::common::{self, luminosity, move_tail, rf};
use crate::fx::core::*;

/// `MONSTER01_DIE` (`_define.h:489`).
const MONSTER01_DIE: usize = 6;

pub fn register(r: &mut Registry) {
    let k = |create: EffectCreate, step: EffectStep, draw: EffectDraw| EffectKind { create: Some(create), step: Some(step), draw: Some(draw) };

    // Halloween.
    r.effect(MODEL_HALLOWEEN_EX, k(halloween_ex_create, halloween_ex_step, common::draw_nothing));
    for ty in [
        MODEL_HALLOWEEN_CANDY_BLUE,
        MODEL_HALLOWEEN_CANDY_ORANGE,
        MODEL_HALLOWEEN_CANDY_YELLOW,
        MODEL_HALLOWEEN_CANDY_RED,
        MODEL_HALLOWEEN_CANDY_HOBAK,
        MODEL_HALLOWEEN_CANDY_STAR,
    ] {
        r.effect(ty, k(candy_create, candy_step, render_object));
    }

    // Christmas.
    for ty in [MODEL_XMAS_EVENT_BOX, MODEL_XMAS_EVENT_CANDY, MODEL_XMAS_EVENT_TREE, MODEL_XMAS_EVENT_SOCKS] {
        r.effect(ty, k(xmas_create, xmas_step, render_object));
    }
    r.effect(MODEL_XMAS_EVENT_ICEHEART, k(iceheart_create, iceheart_step, iceheart_draw));
    r.effect(MODEL_XMAS2008_SNOWMAN_HEAD, k(snowman_head_create, snowman_head_step, render_object));
    r.effect(MODEL_XMAS2008_SNOWMAN_BODY, k(snowman_body_create, snowman_body_step, snowman_body_draw));

    // New Year's day.
    for ty in [
        MODEL_NEWYEARSDAY_EVENT_BEKSULKI,
        MODEL_NEWYEARSDAY_EVENT_CANDY,
        MODEL_NEWYEARSDAY_EVENT_MONEY,
        MODEL_NEWYEARSDAY_EVENT_HOTPEPPER_GREEN,
        MODEL_NEWYEARSDAY_EVENT_HOTPEPPER_RED,
        MODEL_NEWYEARSDAY_EVENT_YUT,
    ] {
        r.effect(ty, k(newyear_create, newyear_step, render_object));
    }
    r.effect(MODEL_NEWYEARSDAY_EVENT_PIG, k(newyear_create, newyear_step, pig_draw));

    // Moon harvest.
    r.effect(MODEL_MOONHARVEST_MOON, k(moon_create, moon_step, moon_draw));
    for ty in [MODEL_MOONHARVEST_GAM, MODEL_MOONHARVEST_SONGPUEN1, MODEL_MOONHARVEST_SONGPUEN2] {
        r.effect(ty, k(songpyeon_create, songpyeon_step, render_object));
    }

    // Cursed Temple.
    r.effect(MODEL_CURSEDTEMPLE_HOLYITEM, k(cursed_skill_create, holyitem_step, holyitem_draw));
    r.effect(MODEL_CURSEDTEMPLE_PRODECTION_SKILL, k(cursed_skill_create, protection_step, protection_draw));
    r.effect(MODEL_CURSEDTEMPLE_RESTRAINT_SKILL, k(cursed_skill_create, restraint_step, restraint_draw));
    for ty in [MODEL_CURSEDTEMPLE_STATUE_PART1, MODEL_CURSEDTEMPLE_STATUE_PART2] {
        r.effect(ty, k(statue_part_create, statue_part_step, render_object));
    }

    // PK field assassin. No `case` in `RenderEffects`: the `default:` renders them because they are
    // inside `MODEL_SKILL_BEGIN..MODEL_SKILL_END` (`RenderBody(RENDER_TEXTURE)`, GM_PK_Field.cpp:1024).
    for ty in [MODEL_PKFIELD_ASSASSIN_EFFECT_GREEN_HEAD, MODEL_PKFIELD_ASSASSIN_EFFECT_RED_HEAD] {
        r.effect(ty, k(assassin_head_create, assassin_head_step, render_object));
    }
    for ty in [MODEL_PKFIELD_ASSASSIN_EFFECT_GREEN_BODY, MODEL_PKFIELD_ASSASSIN_EFFECT_RED_BODY] {
        r.effect(ty, k(assassin_body_create, assassin_body_step, render_object));
    }
}

// --- Common ----------------------------------------------------------------------------------------

/// `RenderObject(o)` (ZzzObject.cpp:2684): nothing when `Alpha < 0.01` (`Calc_RenderObject`), else
/// the model with the effect's state.
fn render_object(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    if e.alpha < 0.01 {
        return;
    }
    out.models.push(ModelDraw::of_effect(e));
}

/// `(rand()%10-5)*0.02f`: the small random scale offset of the falling items.
fn scale_jitter(ctx: &mut Ctx) -> f32 {
    (ctx.rand() % 10 - 5) as f32 * 0.02
}

/// `o->Angle[i] = (float)(rand()%360)` for the three axes, in order.
fn random_angle(ctx: &mut Ctx) -> Vec3 {
    let x = rf(ctx, 360);
    let y = rf(ctx, 360);
    let z = rf(ctx, 360);
    Vec3::new(x, y, z)
}

/// `o->Angle[o->m_iAnimation]`: the axis the falling items spin around (`user[0]` = `m_iAnimation`,
/// always `rand()%3`).
fn spin_axis(e: &Effect) -> usize {
    (e.user[0].max(0.0) as usize).min(2)
}

/// Owner's `BoneTransform[bone]` applied to `offset` (`b->TransformPosition(o->Owner->BoneTransform
/// [bone], offset, out, true)`), if the owner is a live character.
fn owner_bone(e: &Effect, ctx: &Ctx, bone: usize, offset: Vec3) -> Option<(Vec3, Vec3, Vec3)> {
    let a = ctx.actor(e.owner).filter(|a| a.live)?;
    Some((a.bone_world(bone, offset), a.position, a.angle))
}

// --- MODEL_HALLOWEEN_EX (spawns the Halloween candies) ---------------------------------------------

/// ZzzEffect.cpp:3961.
fn halloween_ex_create(e: &mut Effect, ctx: &mut Ctx) {
    if e.sub_type == 0 {
        e.life_time = 0.0;
        for _ in 0..24 {
            let ty = match ctx.rand() % 8 {
                0 => MODEL_HALLOWEEN_CANDY_BLUE,
                1 => MODEL_HALLOWEEN_CANDY_ORANGE,
                2 => MODEL_HALLOWEEN_CANDY_YELLOW,
                3 => MODEL_HALLOWEEN_CANDY_RED,
                4 | 6 => MODEL_HALLOWEEN_CANDY_HOBAK,
                _ => MODEL_HALLOWEEN_CANDY_STAR,
            };
            ctx.create_effect(ty, e.position, e.angle, e.light, 0, None, -1, 0, 0, 0.0);
        }
    }
}

/// ZzzEffect.cpp:7612: the case only breaks (it dies on its first tick, `LifeTime` is 0).
fn halloween_ex_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    move_tail(e, ctx);
}

// --- MODEL_HALLOWEEN_CANDY_BLUE/ORANGE/YELLOW/RED/HOBAK/STAR ----------------------------------------

/// ZzzEffect.cpp:3988-4031. `user[0]` = `m_iAnimation`.
fn candy_create(e: &mut Effect, ctx: &mut Ctx) {
    if e.sub_type == 0 {
        if e.ty == MODEL_HALLOWEEN_CANDY_HOBAK || e.ty == MODEL_HALLOWEEN_CANDY_STAR {
            e.scale = 2.0 + scale_jitter(ctx);
        } else {
            e.scale = 0.6 + scale_jitter(ctx);
        }
        e.life_time = (ctx.rand() % 10 + 50) as f32;
        e.angle = random_angle(ctx);
        e.gravity = (ctx.rand() % 10 + 10) as f32;
        let px = (ctx.rand() % 60 - 30) as f32 * 0.1;
        let py = (ctx.rand() % 60 - 30) as f32 * 0.1;
        let p = Vec3::new(px, py, 0.0) * 2.0;
        e.direction = rotate(e.angle, p);
        e.user[0] = (ctx.rand() % 3) as f32;
    } else if e.sub_type == 1 {
        e.scale = 2.0 + scale_jitter(ctx);
        e.life_time = (ctx.rand() % 10 + 40) as f32;
        e.angle = random_angle(ctx);
        e.gravity = (ctx.rand() % 10 + 10) as f32;
        let px = (ctx.rand() % 60 - 30) as f32 * 0.1;
        let py = (ctx.rand() % 60 - 30) as f32 * 0.1;
        let p = Vec3::new(px, py, -1.0) * 1.5;
        e.direction = rotate(e.angle, p);
        e.user[0] = (ctx.rand() % 3) as f32;
    }
}

/// ZzzEffect.cpp:7580-7610.
fn candy_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    if e.sub_type == 0 || e.sub_type == 1 {
        let axis = spin_axis(e);
        e.angle[axis] += 20.0;
        e.position.z += e.gravity * 0.5;
        e.gravity -= 1.5;

        let height = ctx.terrain_height(e.position.x, e.position.y);
        if e.position.z < height {
            e.position.z = height;
            e.gravity = -e.gravity * 0.3;
            e.life_time -= 2.0;
        }

        e.position += e.direction;

        if e.ty == MODEL_HALLOWEEN_CANDY_HOBAK && ctx.rand() % 3 == 0 {
            let scale = 0.4 + (ctx.rand() % 10) as f32 * 0.01;
            ctx.create_particle(BITMAP_FIRE, e.position, e.angle, e.light, 5, scale, None);
        }
    }
    move_tail(e, ctx);
}

// --- MODEL_XMAS_EVENT_BOX/CANDY/TREE/SOCKS ---------------------------------------------------------

/// ZzzEffect.cpp:4033-4055. `user[0]` = `m_iAnimation`.
fn xmas_create(e: &mut Effect, ctx: &mut Ctx) {
    e.scale = 0.7 + scale_jitter(ctx);
    if e.ty == MODEL_XMAS_EVENT_BOX {
        e.scale += 0.3;
    }
    e.life_time = (ctx.rand() % 10 + 50) as f32;
    e.angle = random_angle(ctx);
    e.gravity = (ctx.rand() % 5 + 15) as f32;
    let px = (ctx.rand() % 60 - 30) as f32 * 0.1;
    let py = (ctx.rand() % 60 - 30) as f32 * 0.1;
    let p = Vec3::new(px, py, -1.0) * 1.5;
    e.direction = rotate(e.angle, p);
    e.user[0] = (ctx.rand() % 3) as f32;
}

/// ZzzEffect.cpp:7615-7632.
fn xmas_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    let axis = spin_axis(e);
    e.angle[axis] += 20.0;
    e.position.z += e.gravity * 0.5;
    e.gravity -= 1.5;

    let height = ctx.terrain_height(e.position.x, e.position.y);
    if e.position.z < height + 3.0 {
        e.position.z = height + 3.0;
        e.gravity = -e.gravity * 0.3;
        e.life_time -= 2.0;
    }

    e.position += e.direction;
    move_tail(e, ctx);
}

// --- MODEL_XMAS_EVENT_ICEHEART (Santa transformation) -----------------------------------------------

/// ZzzEffect.cpp:4056.
fn iceheart_create(e: &mut Effect, ctx: &mut Ctx) {
    e.scale = 4.0 + scale_jitter(ctx);
    e.life_time = 100.0;
}

/// ZzzEffect.cpp:7634-7643: dies when the owner leaves the `PLAYER_SANTA_2` action.
fn iceheart_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    e.angle.z += 10.0;
    if e.owner.is_some() {
        // A vanished owner cannot be in PLAYER_SANTA_2 either.
        let santa = ctx.actor(e.owner).is_some_and(|a| a.action as i32 == PLAYER_SANTA_2);
        if !santa {
            e.live = false;
        }
    }
    move_tail(e, ctx);
}

/// ZzzEffect.cpp:18360 → `RenderObject` → ZzzObject.cpp:1683-1689: `BodyLight` (1, 0.4, 0.4),
/// `RenderBody(RENDER_TEXTURE|RENDER_BRIGHT, ...)` with `StreamMesh = 0` and
/// U = `-(int)WorldTime%2000*0.0005f`.
/// `ModelDraw` has no `StreamMesh`: the stream mesh 0 (flat `BodyLight`, additive, offset UVs,
/// ZzzBMD.cpp:970-986, 1193-1196) is drawn as the blend mesh with `BlendMeshLight` 1, which gives
/// the same color, blending and UVs (the model's only mesh uses texture 0; `o->BlendMesh` is -1).
fn iceheart_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    if e.alpha < 0.01 {
        return;
    }
    let mut d = ModelDraw::of_effect(e);
    d.light = Vec3::new(1.0, 0.4, 0.4);
    d.render_flags = RENDER_TEXTURE | RENDER_BRIGHT;
    d.blend_mesh = 0;
    d.blend_mesh_light = 1.0;
    let u = -(ctx.world_time as i32) % 2000;
    d.blend_mesh_tex_coord = Vec2::new(u as f32 * 0.0005, e.blend_mesh_tex_coord.y);
    out.models.push(d);
}

// --- MODEL_XMAS2008_SNOWMAN_HEAD (the dying snowman's head) -----------------------------------------

/// ZzzEffect.cpp:2558-2572. `user[0]` = `m_iAnimation` (unused by its move code).
fn snowman_head_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = 50.0;
    e.scale = 1.3;
    // The original always has an owner (the snowman, ZzzObject.cpp:1865).
    if let Some(a) = ctx.actor(e.owner) {
        e.angle.z = a.angle.z;
    }
    e.gravity = 5.0;
    let px = (ctx.rand() % 10 - 5) as f32 * 0.1;
    let py = (ctx.rand() % 60 - 40) as f32 * 0.1;
    let p = Vec3::new(px, py, 0.0) * 1.3;
    e.direction = rotate(e.angle, p);
    e.user[0] = (ctx.rand() % 3) as f32;
}

/// ZzzEffect.cpp:11539-11559. Not in `MODEL_BIRD01..MODEL_SKILL_END`: the tail only moves it.
fn snowman_head_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    if e.life_time < 45.0 {
        e.position.x += e.direction.x * 1.2;
        e.position.y += e.direction.y * 1.2;
        e.position.z += e.gravity * 1.5;
        e.gravity -= 1.5;

        let height = ctx.terrain_height(e.position.x, e.position.y);
        if e.position.z < height {
            e.position.z = height;
            e.gravity = -e.gravity * 0.3;
            e.life_time -= 2.0;
        }

        e.position += e.direction;
    }
    move_tail(e, ctx);
}

// --- MODEL_XMAS2008_SNOWMAN_BODY (the dying snowman's body) -----------------------------------------

/// ZzzEffect.cpp:2573-2586.
fn snowman_body_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = 50.0;
    e.scale = 1.3;
    e.velocity = 1.2;
    e.pk_key = -1;
    // `o->Direction = o->Owner->Direction` (the dying snowman, ZzzObject.cpp:1865); the original
    // always has an owner.
    if let Some(a) = ctx.actor(e.owner) {
        e.direction = a.direction;
    }
}

/// ZzzEffect.cpp:11560-11565: `PlayAnimation` of its own model at `Velocity / 5` (result unused).
/// Not in `MODEL_BIRD01..MODEL_SKILL_END`: the tail only moves it.
fn snowman_body_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    let speed = e.velocity / 5.0;
    let _ = common::play_model_animation(e, ctx, speed);
    move_tail(e, ctx);
}

/// ZzzEffect.cpp:18646 → `RenderObject` → ZzzObject.cpp:1899-1905: `RenderMesh` 0, 2 and 3 (this
/// model has no mesh 3: nothing is drawn for it) with the effect's state, then mesh 1 with
/// `BlendMesh` 1 and `BlendMeshLight` 0.5. The last `RenderMesh` argument is `MeshTexture`, to
/// which the original passes `o->HiddenMesh` (-1 here: the mesh's own texture).
fn snowman_body_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    if e.alpha < 0.01 {
        return;
    }
    for (mesh, blend_mesh, blend_mesh_light) in [(0, e.blend_mesh, e.blend_mesh_light), (2, e.blend_mesh, e.blend_mesh_light), (3, e.blend_mesh, e.blend_mesh_light), (1, 1, 0.5)] {
        let mut d = ModelDraw::of_effect(e);
        d.render_flags = RENDER_TEXTURE;
        d.mesh = mesh;
        d.hidden_mesh = -1;
        d.texture = e.hidden_mesh;
        d.blend_mesh = blend_mesh;
        d.blend_mesh_light = blend_mesh_light;
        out.models.push(d);
    }
}

// --- MODEL_NEWYEARSDAY_EVENT_BEKSULKI/CANDY/MONEY/HOTPEPPER_GREEN/HOTPEPPER_RED/YUT/PIG ------------

/// ZzzEffect.cpp:4063-4109. The peppers may swap colour (`o->Type` changes, so it is moved and
/// drawn as the other one). `user[0]` = `m_iAnimation`.
fn newyear_create(e: &mut Effect, ctx: &mut Ctx) {
    // Exactly one rand() for either pepper (the guard runs only when its pattern matches).
    match e.ty {
        MODEL_NEWYEARSDAY_EVENT_HOTPEPPER_GREEN if ctx.rand() % 2 == 0 => e.ty = MODEL_NEWYEARSDAY_EVENT_HOTPEPPER_RED,
        MODEL_NEWYEARSDAY_EVENT_HOTPEPPER_RED if ctx.rand() % 2 == 0 => e.ty = MODEL_NEWYEARSDAY_EVENT_HOTPEPPER_GREEN,
        _ => {}
    }

    e.life_time = (ctx.rand() % 10 + 50) as f32;
    e.scale = 1.6 + scale_jitter(ctx);

    if e.ty == MODEL_NEWYEARSDAY_EVENT_BEKSULKI {
        e.scale = 2.5 + scale_jitter(ctx);
    }
    if e.ty == MODEL_NEWYEARSDAY_EVENT_CANDY {
        e.scale = 3.0 + scale_jitter(ctx);
    } else if e.ty == MODEL_NEWYEARSDAY_EVENT_PIG {
        e.scale = 1.0 + scale_jitter(ctx);
    }

    e.angle = random_angle(ctx);
    e.gravity = (ctx.rand() % 10 + 10) as f32;
    let px = (ctx.rand() % 10 - 5) as f32 * 0.1;
    let py = (ctx.rand() % 60 - 40) as f32 * 0.1;
    let p = Vec3::new(px, py, 0.0) * 1.3;
    e.direction = rotate(e.angle, p);
    e.user[0] = (ctx.rand() % 3) as f32;
}

/// ZzzEffect.cpp:7645-7667.
fn newyear_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    let axis = spin_axis(e);
    e.angle[axis] += 10.0;
    e.position.x += e.direction.x * 1.2;
    e.position.y += e.direction.y * 1.2;
    e.position.z += e.gravity * 1.5;
    e.gravity -= 1.5;

    let height = ctx.terrain_height(e.position.x, e.position.y);
    if e.position.z < height {
        e.position.z = height;
        e.gravity = -e.gravity * 0.3;
        e.life_time -= 2.0;
    }

    e.position += e.direction;
    move_tail(e, ctx);
}

/// ZzzEffect.cpp:18369 → `RenderObject` → ZzzObject.cpp:1694-1700: `RENDER_TEXTURE`, then a
/// `RENDER_CHROME3|RENDER_BRIGHT` pass with `BodyLight` (1, 0.4, 0.2).
fn pig_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    if e.alpha < 0.01 {
        return;
    }
    let mut d = ModelDraw::of_effect(e);
    d.render_flags = RENDER_TEXTURE;
    out.models.push(d);
    d.light = Vec3::new(1.0, 0.4, 0.2);
    d.render_flags = RENDER_CHROME3 | RENDER_BRIGHT;
    out.models.push(d);
}

// --- MODEL_MOONHARVEST_MOON ------------------------------------------------------------------------

/// ZzzEffect.cpp:4111-4130. `user[0]` = `m_iAnimation` (subtype 0: frames rendered).
fn moon_create(e: &mut Effect, _ctx: &mut Ctx) {
    match e.sub_type {
        0 => {
            e.alpha = 0.6;
            e.life_time = 70.0;
            e.user[0] = 0.0;
        }
        1 => {
            e.life_time = 50.0;
            e.direction = e.angle;
            e.angle = Vec3::ZERO;
        }
        2 => e.life_time = 1.0,
        _ => {}
    }
}

/// ZzzEffect.cpp:7668-7695.
fn moon_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    match e.sub_type {
        0 => {
            e.angle.z += 5.0;
            if e.life_time < 25.0 {
                e.light -= Vec3::new(0.05, 0.06, 0.05);
            }
        }
        1 => {
            e.position += e.direction;
            let ty = BITMAP_SMOKELINE1 + ctx.rand() % 3;
            ctx.create_particle(ty, e.position, e.angle, e.light, 0, 1.0, None);
            ctx.create_particle(BITMAP_SMOKE, e.position, e.angle, e.light, 11, 1.0, None);
            if ctx.rand() % 2 == 0 {
                ctx.create_particle(BITMAP_WATERFALL_3, e.position, e.angle, e.light, 3, 3.0, None);
            }
        }
        2 => e.scale += 0.01,
        _ => {}
    }
    move_tail(e, ctx);
    // RenderEffects (ZzzEffect.cpp:18380): `o->m_iAnimation++` once per rendered frame.
    if e.sub_type == 0 {
        e.user[0] += 1.0;
    }
}

/// ZzzEffect.cpp:18373-18401: sprites (`CreateSprite` while rendering, subtype 0 = additive) and
/// `RenderObject`.
fn moon_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    let glow = Some(Blend::Glow);
    match e.sub_type {
        0 => {
            common::sprite(out, ctx, BITMAP_SHOCK_WAVE, e.position, 0.8, Vec3::new(0.6, 0.8, 0.6), -(e.user[0] * 3.0), glow);
            common::sprite(out, ctx, BITMAP_LIGHT, e.position, 5.0, Vec3::new(0.8, 0.6, 0.0), 0.0, glow);
            render_object(e, ctx, out);
        }
        1 => {
            common::sprite(out, ctx, BITMAP_LIGHT, e.position, 5.0, e.light, 0.0, glow);
            common::sprite(out, ctx, BITMAP_LIGHT, e.position, 3.0, e.light, 0.0, glow);
            common::sprite(out, ctx, BITMAP_LIGHT, e.position, 3.0, e.light, 0.0, glow);
            common::sprite(out, ctx, BITMAP_SHINY + 6, e.position, 2.0, e.light, 0.0, glow);
            render_object(e, ctx, out);
        }
        2 => render_object(e, ctx, out),
        _ => {}
    }
}

// --- MODEL_MOONHARVEST_GAM/SONGPUEN1/SONGPUEN2 -----------------------------------------------------

/// ZzzEffect.cpp:4132-4155.
fn songpyeon_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = (ctx.rand() % 10 + 50) as f32;
    if e.ty == MODEL_MOONHARVEST_GAM {
        e.scale = 0.5 + scale_jitter(ctx);
    } else {
        e.scale = 0.8 + scale_jitter(ctx);
    }
    e.angle = random_angle(ctx);
    e.gravity = (ctx.rand() % 10 + 10) as f32;
    let px = (ctx.rand() % 10 - 5) as f32 * 0.1;
    let py = (ctx.rand() % 60 - 30) as f32 * 0.1;
    let p = Vec3::new(px, py, 0.0) * 1.2;
    e.direction = rotate(e.angle, p);
}

/// ZzzEffect.cpp:7696-7712.
fn songpyeon_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    e.angle.x += 10.0;
    e.position.x += e.direction.x * 2.2;
    e.position.y += e.direction.y * 2.2;
    e.position.z += e.gravity * 1.5;
    e.gravity -= 1.5;

    let height = ctx.terrain_height(e.position.x, e.position.y);
    if e.position.z < height {
        e.position.z = height;
        e.gravity = -e.gravity * 0.3;
        e.life_time -= 2.0;
    }
    move_tail(e, ctx);
}

// --- MODEL_CURSEDTEMPLE_HOLYITEM / PRODECTION_SKILL / RESTRAINT_SKILL ---------------------------------

/// ZzzEffect.cpp:1965-1979: they last until deleted (the move code keeps renewing `LifeTime`).
fn cursed_skill_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 9_999_999.0;
}

/// ZzzEffect.cpp:12036-12061: the holy item floats at the carrier's bone 20.
fn holyitem_step(e: &mut Effect, ctx: &mut Ctx) {
    // Draw_RenderObject (ZzzObject.cpp:631), from the last draw: skipped when Alpha < 0.01.
    if e.alpha >= 0.01 {
        let light = Vec3::new(0.9, 0.6, 0.6);
        ctx.create_particle(BITMAP_POUNDING_BALL, e.position, e.angle, light, 2, 0.1, Some(Target::Effect(e.id)));
    }

    let _ = luminosity(e, ctx);
    match owner_bone(e, ctx, 20, Vec3::new(70.0, 0.0, 0.0)) {
        // `if (!o->Owner->Live) { o->Live = false; break; }` (a vanished owner counts as dead).
        None => e.live = false,
        Some((world, _, _)) => {
            if e.life_time < 10.0 {
                e.life_time = 999_999.0;
            }
            e.light = Vec3::ONE;
            e.position = world;
            play_owner_animation(e, ctx, e.velocity / 3.0);
        }
    }
    move_tail(e, ctx);
}

/// `Models[o->Owner->Type].PlayAnimation(&o->AnimationFrame, ..., speed, ...)`
/// (ZzzEffect.cpp:12046, 12059): the effect's frame advanced with the CARRIER's model and the
/// action that model was last animated with (`b->CurrentAction`, the last character of that type
/// drawn; here the owner's own action, the closest the snapshots give). `Actions[].Loop` is set on
/// `MODEL_PLAYER`'s `PLAYER_DIE1/DIE2/COME_UP/SKILL_HELL_BEGIN` (ZzzOpenData.cpp:380-384). Without
/// the model file the frame still advances (see `common::play_model_animation`).
fn play_owner_animation(e: &mut Effect, ctx: &mut Ctx, speed: f32) {
    let Some((ty, action)) = ctx.actor(e.owner).map(|a| (a.ty, a.action)) else { return };
    let Some(m) = ctx.model(ty) else {
        e.animation_frame += speed;
        return;
    };
    let Some(a) = m.actions.get(action) else { return };
    let hold = ty == MODEL_PLAYER && [PLAYER_DIE1, PLAYER_DIE2, PLAYER_COME_UP, PLAYER_SKILL_HELL_BEGIN].contains(&(action as i32));
    let _ = crate::fx::model_data::play_animation(&mut e.animation_frame, speed, a.frames, a.lock_positions.is_some(), hold);
}

/// ZzzEffect.cpp:18411 → `RenderObject` → ZzzObject.cpp:623-635: two sprites, the body without
/// mesh 0, then mesh 0 additive (alpha 0.7, `BlendMeshLight · 3`, V scrolling with `WorldTime`).
fn holyitem_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    if e.alpha < 0.01 {
        return;
    }
    let wt = ctx.world_time;
    let lum = (wt * 0.0002).sin() * 0.002;
    let glow = Some(Blend::Glow);
    common::sprite(out, ctx, BITMAP_LIGHT, e.position, 3.0, Vec3::new(0.9, 0.4, 0.4), 0.0, glow);
    common::sprite(out, ctx, BITMAP_POUNDING_BALL, e.position, 0.9 + lum, Vec3::new(0.9, 0.6, 0.6), wt / 10.0, glow);

    // RenderBody(RENDER_TEXTURE, Alpha, BlendMesh, BlendMeshLight, U, V, HiddenMesh = 0).
    let mut body = ModelDraw::of_effect(e);
    body.render_flags = RENDER_TEXTURE;
    body.hidden_mesh = 0;
    out.models.push(body);

    // RenderMesh(0, RENDER_TEXTURE, 0.7, BlendMesh 0, BlendMeshLight·3, U, -WorldTime·0.004).
    let mut glow_mesh = ModelDraw::of_effect(e);
    glow_mesh.render_flags = RENDER_TEXTURE;
    glow_mesh.mesh = 0;
    glow_mesh.hidden_mesh = -1;
    glow_mesh.alpha = 0.7;
    glow_mesh.blend_mesh = 0;
    glow_mesh.blend_mesh_light = e.blend_mesh_light * 3.0;
    glow_mesh.blend_mesh_tex_coord = Vec2::new(e.blend_mesh_tex_coord.x, -wt * 0.004);
    out.models.push(glow_mesh);
}

/// ZzzEffect.cpp:12062-12092: the shield around the carrier (bone 2) and a shock wave every tick.
fn protection_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    match owner_bone(e, ctx, 2, Vec3::new(-23.0, 0.0, 0.0)) {
        None => e.live = false,
        Some((world, owner_position, owner_angle)) => {
            if e.life_time < 10.0 {
                e.life_time = 999_999.0;
            }
            e.light = Vec3::ONE;
            e.position = world;
            e.angle = owner_angle;
            e.alpha = 0.3;
            e.scale = 1.0;
            // `effbmd->PlayAnimation(..., Velocity/3, ...)` of its own model, result unused (the
            // tail plays it again at Velocity).
            let speed = e.velocity / 3.0;
            let _ = common::play_model_animation(e, ctx, speed);
            ctx.create_effect(BITMAP_SHOCK_WAVE, owner_position, e.angle, e.light, 10, e.owner, -1, 0, 0, 0.0);
        }
    }
    move_tail(e, ctx);
}

/// ZzzEffect.cpp:18412 → `RenderObject` → ZzzObject.cpp:636-641: `BodyLight` (0.3, 0.3, 1),
/// `RenderMesh(0, RENDER_TEXTURE, Alpha, BlendMesh 0, BlendMeshLight, U, -WorldTime·0.0004)`.
fn protection_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    if e.alpha < 0.01 {
        return;
    }
    let mut d = ModelDraw::of_effect(e);
    d.light = Vec3::new(0.3, 0.3, 1.0);
    d.render_flags = RENDER_TEXTURE;
    d.mesh = 0;
    d.hidden_mesh = -1;
    d.blend_mesh = 0;
    d.blend_mesh_tex_coord = Vec2::new(e.blend_mesh_tex_coord.x, -ctx.world_time * 0.0004);
    out.models.push(d);
}

/// ZzzEffect.cpp:12093-12116: the restraint ropes on the target.
fn restraint_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    match ctx.actor(e.owner).filter(|a| a.live).map(|a| (a.position, a.angle)) {
        None => e.live = false,
        Some((position, angle)) => {
            if e.life_time < 10.0 {
                e.life_time = 999_999.0;
            }
            e.light = Vec3::ONE;
            e.position = position;
            e.angle = angle;
            e.alpha = 0.6;
            e.scale = 1.0;
            // `b->PlayAnimation(..., Velocity/3, ...)` of its own model, result unused.
            let speed = e.velocity / 3.0;
            let _ = common::play_model_animation(e, ctx, speed);
        }
    }
    move_tail(e, ctx);
}

/// ZzzEffect.cpp:18417 → `RenderObject` → ZzzObject.cpp:642-646:
/// `RenderMesh(0, RENDER_TEXTURE, 0.7, BlendMesh 0, 0.35 + fLuminosity, -WT·0.0004, WT·0.0004)`.
fn restraint_draw(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    if e.alpha < 0.01 {
        return;
    }
    let wt = ctx.world_time;
    let lum = (wt * 0.0002).sin() * 0.002;
    let mut d = ModelDraw::of_effect(e);
    d.render_flags = RENDER_TEXTURE;
    d.mesh = 0;
    d.hidden_mesh = -1;
    d.alpha = 0.7;
    d.blend_mesh = 0;
    d.blend_mesh_light = 0.35 + lum;
    d.blend_mesh_tex_coord = Vec2::new(-wt * 0.0004, wt * 0.0004);
    out.models.push(d);
}

// --- MODEL_CURSEDTEMPLE_STATUE_PART1/2 (pieces of the broken statue) --------------------------------

/// ZzzEffect.cpp:2542-2557.
fn statue_part_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = (180 + ctx.rand() % 40) as f32;
    e.scale = 0.1 + (ctx.rand() % 6) as f32 * 0.1;
    e.velocity = 0.0;
    e.pk_key = -1;
    e.gravity = 2.5;
    e.direction.x = (-15 + ctx.rand() % 30) as f32;
    e.direction.y = (-15 + ctx.rand() % 30) as f32;
    e.direction.z = 0.0;
    e.angle.x = 0.0;
    e.angle.y = 0.0;
    e.angle.z = rf(ctx, 360);
    e.light = Vec3::splat(0.5 + (ctx.rand() % 6) as f32 * 0.1);
}

/// ZzzEffect.cpp:11518-11538: falls until z = 290, shakes the camera, then fades out.
fn statue_part_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    if e.position.z > 290.0 {
        e.direction.z -= e.gravity;
        e.angle.z -= rf(ctx, 10);
    } else if e.position.z < 290.0 {
        e.position.z = 290.0;
        e.direction = Vec3::ZERO;
        let quake = (ctx.rand() % 4 + 1) as f32 * 0.1;
        ctx.set_earthquake(quake);
    } else {
        e.alpha -= 0.01;
    }
    move_tail(e, ctx);
}

// --- MODEL_PKFIELD_ASSASSIN_EFFECT_GREEN_HEAD / RED_HEAD --------------------------------------------

/// ZzzEffect.cpp:3063-3085: the head flies off (`HeadAngle` is its velocity).
fn assassin_head_create(e: &mut Effect, ctx: &mut Ctx) {
    e.life_time = (50 + ctx.rand() % 30) as f32;
    e.scale = 1.0;
    e.velocity = 1.0;
    e.pk_key = -1;
    e.gravity = 3.5;

    e.angle.z = rf(ctx, 360);
    let p1 = Vec3::new(0.0, (ctx.rand() % 64 + 25) as f32 * 0.2, 0.0);
    e.head_angle = rotate(e.angle, p1);
    e.head_angle.z = 25.0;

    e.sub_type = ctx.rand() % 2;
    e.direction = Vec3::ZERO;
}

/// ZzzEffect.cpp:15715-15768.
fn assassin_head_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    e.head_angle.z -= e.gravity;
    e.position += e.head_angle;

    let height = ctx.terrain_height(e.position.x, e.position.y) + 20.0;
    if e.position.z + e.direction.z <= height {
        e.position.z = height;
        e.head_angle.x *= 0.8;
        e.head_angle.y *= 0.8;
        e.head_angle.z += 0.6 * e.life_time;
        if e.head_angle.z < 5.0 {
            e.head_angle.z = 0.0;
        }
        e.alpha -= 0.05;
    } else if e.sub_type == 0 {
        e.angle.x += 0.15 * e.life_time;
        e.angle.y += 0.15 * e.life_time;
    } else {
        e.angle.x -= 0.15 * e.life_time;
        e.angle.y -= 0.15 * e.life_time;
    }

    // vPosition = Position + Angle·(0, 0, 0) = Position.
    let sub = if e.ty == MODEL_PKFIELD_ASSASSIN_EFFECT_GREEN_HEAD { 4 } else { 3 };
    ctx.create_effect(BITMAP_FIRE_CURSEDLICH, e.position, e.angle, Vec3::ONE, sub, Some(Target::Effect(e.id)), -1, 0, 0, 0.0);
    move_tail(e, ctx);
}

// --- MODEL_PKFIELD_ASSASSIN_EFFECT_GREEN_BODY / RED_BODY --------------------------------------------

/// ZzzEffect.cpp:3086-3094: the body plays its death animation.
fn assassin_body_create(e: &mut Effect, ctx: &mut Ctx) {
    e.scale = 1.0;
    e.life_time = (40 + ctx.rand() % 30) as f32;
    e.velocity = 0.4;
    e.current_action = MONSTER01_DIE;
}

/// ZzzEffect.cpp:15769-15779. `user[0]` = `m_iAnimation` (counted, never read).
fn assassin_body_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = luminosity(e, ctx);
    e.user[0] += 1.0;
    if e.life_time < 20.0 {
        e.alpha = 0.1 * e.life_time;
    }
    move_tail(e, ctx);
}
