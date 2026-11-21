//! Objetos del mapa con efectos de Raklion (`WD_57ICECITY`, `WD_58ICECITY_BOSS`), Vulcanus
//! (`WD_63PK_FIELD`), Barracks y Refuge de Balgass (`WD_41CHANGEUP3RD_1ST`, `WD_42CHANGEUP3RD_2ND`),
//! Karutan 1 y 2 (`WD_80KARUTAN1`, `WD_81KARUTAN2`) y Chaos Castle (`WD_18CHAOS_CASTLE..`).
//!
//! Traducido de `GM_Raklion.cpp`, `GM_PK_Field.cpp`, `GM3rdChangeUp.cpp`, `GMKarutan1.cpp` y
//! `CSChaosCastle.cpp` (Main 5.2): `CreateObject`, `MoveObject` (con `PlayObjectSound` /
//! `PlayEffectSound`, que van antes), lo que `RenderObjectMesh` crea (partículas, sprites, cambios de
//! acción) y `RenderObjectVisual` (que empieza siempre con el `rand()` de `Luminosity`,
//! `ZzzObject.cpp:2775`). El dibujo de las mallas está en `mu_data::objects::rules_raklion`.
//!
//! Animación: los objetos con `ModelRules::fx_animation` (pingüinos y orcas de Raklion, insecto 66
//! de Karutan, la luz 84 de Refuge) llevan aquí su `o->AnimationFrame` / `o->CurrentAction` con
//! `BMD::PlayAnimation` (`ZzzBMD.cpp:408-479`) y `SetAction` (`ZzzObject.cpp:444-459`) y lo dejan en
//! `user[`[`fx_anim`]`::*]` para que `objects.rs` dibuje lo mismo.
//!
//! También van aquí los efectos que solo crean estos objetos: `MODEL_WARP4/5/6` (el portal de Raklion,
//! `ZzzEffect.cpp:668-686`, `:7334-7360`, y su dibujo en `CGM_Raklion::RenderObjectMesh`,
//! GM_Raklion.cpp:1393-1419) y `BITMAP_FIRE_RED` (la columna de fuego de Refuge, `ZzzEffect.cpp:1286`,
//! `:8918-8923`).
//!
//! Sin traducir (dependen de eventos del servidor; se deja el estado inicial):
//! - Raklion: `MoveEffect` / `CreateMapEffect` (hielo que cae y temblor al llamar al jefe,
//!   GM_Raklion.cpp:2801-2929) y `m_bCanGoBossMap` (empieza en `true`: el portal se ve y la puerta 82
//!   se anima). `BITMAP_GATHERING` del 57 está comentado en el original (GM_Raklion.cpp:1358).
//! - Chaos Castle: el derrumbe por niveles (`g_byCurrCastleLevel`, `MoveChaosCastleAllObject`,
//!   `RenderTerrainVisual`) y el parpadeo de los objetos al caer el héroe (`m_bActionStart`).
//!   `InChaosCastle()` sin argumento siempre es falso en Main 5.2 (MapManager.cpp:1649): aquí va con
//!   `WorldActive`, como MuMain y el cliente de verdad (igual que `leaves.rs`).

use std::sync::Mutex;

use bevy::math::Vec3;

use crate::fx::core::*;
use crate::fx::kinds::objects_2::FX_MAP_ANCHOR;
use crate::objects::fx_anim;

// `gMapManager.WorldActive`.
const WD_18CHAOS_CASTLE: i32 = 18;
const WD_18CHAOS_CASTLE_END: i32 = 23;
const WD_41CHANGEUP3RD_1ST: i32 = 41;
const WD_42CHANGEUP3RD_2ND: i32 = 42;
const WD_57ICECITY: i32 = 57;
const WD_58ICECITY_BOSS: i32 = 58;
const WD_63PK_FIELD: i32 = 63;
const WD_80KARUTAN1: i32 = 80;
const WD_81KARUTAN2: i32 = 81;

/// `BodyLight` (0.5, 0.6, 1) por `BlendMeshLight` 0.8 de `MODEL_WARP4/5/6` (GM_Raklion.cpp:1400-1403).
const WARP4_LIGHT: Vec3 = Vec3::new(0.5, 0.6, 1.0);
const WARP4_BLEND_LIGHT: f32 = 0.8;

pub fn register(r: &mut Registry) {
    // --- Raklion ---
    for world in [WD_57ICECITY, WD_58ICECITY_BOSS] {
        for ty in [16, 17] {
            r.object(world, ty, raklion_penguin);
        }
        for ty in [19, 20, 21] {
            r.object(world, ty, raklion_whale);
        }
        r.object(world, 70, raklion_fire);
        r.object(world, 80, raklion_fire);
        r.object(world, MODEL_WARP4, raklion_warp);
    }
    let warp = EffectKind { create: Some(warp_create), step: Some(warp_step), draw: Some(warp_draw) };
    r.effect(MODEL_WARP4, warp);
    r.effect(MODEL_WARP5, warp);
    r.effect(MODEL_WARP6, EffectKind { create: Some(warp6_create), ..warp });

    // --- Vulcanus ---
    for ty in [0, 1, 2, 3, 4, 5, 6, 68] {
        r.object(WD_63PK_FIELD, ty, vulcanus);
    }

    // --- Barracks y Refuge ---
    for world in [WD_41CHANGEUP3RD_1ST, WD_42CHANGEUP3RD_2ND] {
        for ty in [2, 3, 5, 6, 58, 59, 60, 74, 75, 79, 84, 85, 88, 89, 90, 92] {
            r.object(world, ty, balgass);
        }
    }
    r.effect(BITMAP_FIRE_RED, EffectKind { create: Some(fire_red_create), step: Some(fire_red_step), draw: Some(draw_nothing) });

    // --- Karutan ---
    for world in [WD_80KARUTAN1, WD_81KARUTAN2] {
        for ty in [58, 66, 72, 113, 114, 115, 116, 118] {
            r.object(world, ty, karutan);
        }
    }

    // --- Chaos Castle: todos los tipos (para `MoveChaosCastleObjectSetting`, una vez por tick) ---
    for world in WD_18CHAOS_CASTLE..=WD_18CHAOS_CASTLE_END {
        for ty in 0..=35 {
            r.object(world, ty, chaos_castle);
        }
    }
}

// --- Común -------------------------------------------------------------------------------------------

/// `rand() % n` como `f32`.
fn rf(ctx: &mut Ctx, n: i32) -> f32 {
    (ctx.rand() % n) as f32
}

/// El `rand()` de `Luminosity` al principio de `RenderObjectVisual` (`ZzzObject.cpp:2775`).
fn visual_luminosity(ctx: &mut Ctx) {
    let _ = ctx.rand();
}

/// Pide un sonido de un búfer de un canal: varios objetos en el mismo tick son una sola petición.
fn play_once(ctx: &mut Ctx, id: &'static str) {
    if !ctx.sounds.contains(&id) {
        ctx.play_sound(id);
    }
}

/// `(TerrainWall[héroe] & TW_SAFEZONE) == TW_SAFEZONE`: `Hero->SafeZone`.
fn hero_in_safe_zone(ctx: &Ctx) -> bool {
    ctx.hero_actor()
        .map(|h| h.position)
        .is_some_and(|p| ctx.terrain_attribute(p.x, p.y) & mu_data::terrain::attr::SAFE_ZONE != 0)
}

/// Nubes que siguen al objeto (`CreateParticle(BITMAP_CLOUD, ..., o)` mientras `HiddenMesh != -2`,
/// y `HiddenMesh = -2`). Las nubes de los subtipos 0-5 y 7 viven mientras su objeto se ve y al morir
/// le ponen `HiddenMesh = 0` para que las vuelva a crear (`ZzzEffectParticle.cpp`). Como las nubes de
/// Heaven (`objects_2.rs`): las sigue una marca ([`FX_MAP_ANCHOR`]) que muere cuando el objeto deja de
/// moverse, y se vuelven a crear cuando vuelve a moverse tras un hueco (`user[3]`: último tick).
fn object_clouds(o: &mut MapObject, ctx: &mut Ctx, light: Vec3, sub_types: &[i32], count: usize) {
    let gap = ctx.world_time - o.user[3];
    o.user[3] = ctx.world_time;
    if !o.created || gap > 100.0 {
        let anchor = ctx.create_effect(FX_MAP_ANCHOR, o.position, Vec3::ZERO, Vec3::ZERO, 0, None, -1, 0, 0, 0.0);
        for _ in 0..count {
            for &sub in sub_types {
                ctx.create_particle(BITMAP_CLOUD, o.position, o.angle, light, sub, o.scale, Some(Target::Effect(anchor)));
            }
        }
    }
    o.hidden_mesh = -2;
}

/// Fuego de tres tipos al azar con su resplandor rojo (Vulcanus 6, Karutan 113).
fn fire_point(o: &MapObject, ctx: &mut Ctx) {
    ctx.create_sprite(BITMAP_LIGHT, o.position, 2.0 * o.scale, Vec3::new(1.0, 0.2, 0.0), 0.0, 0);
    let light = Vec3::ONE;
    match ctx.rand() % 3 {
        0 => ctx.create_particle(BITMAP_FIRE_HIK1, o.position, o.angle, light, 0, o.scale, None),
        1 => ctx.create_particle(BITMAP_FIRE_CURSEDLICH, o.position, o.angle, light, 4, o.scale, None),
        _ => ctx.create_particle(BITMAP_FIRE_HIK3, o.position, o.angle, light, 0, o.scale, None),
    }
}

/// Chispas que saltan (Vulcanus 4, Balgass 90).
fn sparks(o: &MapObject, ctx: &mut Ctx) {
    let light = Vec3::new(1.0, 0.4, 0.4);
    if ctx.rand() % 3 == 0 {
        let ax = (ctx.rand() % 40 + 120) as f32;
        let az = rf(ctx, 30);
        let angle = Vec3::new(ax, 0.0, az) + o.angle;
        // `o` como `Target`: el subtipo 4 no lo usa.
        ctx.create_joint(BITMAP_JOINT_SPARK, o.position, o.position, angle, 4, None, o.scale);
        ctx.create_particle(BITMAP_SPARK, o.position, angle, light, 9, o.scale, None);
    }
}

/// Humo rojo que sale del suelo (Vulcanus 3, Balgass 89).
fn red_mist(o: &MapObject, ctx: &mut Ctx) {
    if ctx.rand() % 4 == 0 {
        let red = rf(ctx, 3) * 0.01 + 0.015;
        // `o` como dueño: el subtipo 11 no lo usa.
        ctx.create_particle(BITMAP_CLOUD, o.position, o.angle, Vec3::new(red, 0.0, 0.0), 11, o.scale, None);
    }
}

// --- Animación de los objetos con `fx_animation` -----------------------------------------------------

/// `BMD::PlayAnimation` (`ZzzBMD.cpp:408-479`) con `Loop = false`, que es como quedan todas las
/// acciones al cargar (`ZzzBMD.cpp:2587`): `keys` fotogramas, `lock` = `LockPositions`.
fn play_animation(frame: f32, speed: f32, keys: usize, lock: bool) -> f32 {
    if keys <= 1 {
        return frame;
    }
    let mut f = frame + speed;
    if f <= 0.0 {
        f += keys as f32 - 1.0;
    }
    let key = if lock { keys as i32 - 1 } else { keys as i32 };
    if f >= key as f32 {
        let whole = f as i32;
        f = (whole % key) as f32 + (f - whole as f32);
    }
    f
}

/// Fotogramas y `LockPositions` de la acción `action` del modelo del objeto.
fn action_keys(o: &MapObject, action: usize) -> Option<(usize, bool)> {
    let a = o.model.as_ref()?.actions.get(action)?;
    Some((a.frames, a.lock_positions.is_some()))
}

/// Estado de la animación propia del objeto: (fotograma, acción).
fn anim_state(o: &MapObject) -> (f32, usize) {
    (o.user[fx_anim::FRAME], o.user[fx_anim::ACTION].max(0.0) as usize)
}

/// `MoveObject`: `PlayAnimation(&o->AnimationFrame, ..., fSpeed)` (`ZzzObject.cpp:3830`), que no hace
/// nada si la acción no existe en el modelo; luego los huesos de esta pose (`Calc_RenderObject`:
/// `BMD::Animation` usa la acción 0 si la pedida no existe).
fn anim_advance(o: &mut MapObject, speed: f32) {
    let (frame, action) = anim_state(o);
    o.user[fx_anim::PREV_FRAME] = frame;
    let frame = match action_keys(o, action) {
        Some((keys, lock)) => play_animation(frame, speed, keys, lock),
        None => frame,
    };
    o.user[fx_anim::FRAME] = frame;
    o.animation_frame = frame;
    let pose_action = if action_keys(o, action).is_some() { action } else { 0 };
    o.bones = o.model.as_ref().map(|m| std::sync::Arc::new(m.bone_matrices_at(pose_action, frame)));
}

/// `SetAction(o, Action)` (`ZzzObject.cpp:444-459`): si la acción existe y es otra, empieza en 0.
fn set_action(o: &mut MapObject, action: usize) {
    let (_, current) = anim_state(o);
    if action_keys(o, action).is_none() || current == action {
        return;
    }
    o.user[fx_anim::ACTION] = action as f32;
    o.user[fx_anim::FRAME] = 0.0;
    o.animation_frame = 0.0;
}

/// `o->CurrentAction = action` a secas (las orcas): no toca el fotograma.
fn put_action(o: &mut MapObject, action: usize) {
    o.user[fx_anim::ACTION] = action as f32;
}

/// `o->AnimationFrame` dentro de la acción 0 de los objetos animados por grupos (`velocity · ticks`,
/// como lo deja `PlayAnimation`, que da la vuelta).
fn group_frame(o: &MapObject) -> f32 {
    let Some((keys, lock)) = action_keys(o, 0) else { return 0.0 };
    let keys = if lock { keys.saturating_sub(1) } else { keys };
    if keys <= 1 {
        return 0.0;
    }
    o.animation_frame.rem_euclid(keys as f32)
}

// --- Raklion (`GM_Raklion.cpp`) -----------------------------------------------------------------------

/// Pingüinos 16 y 17 (`RenderObjectMesh` 1120-1164) con la `PlaySpeed` de cada acción
/// (`MapManager.cpp:1190-1195`: 0.8, 0.8, 1, 1; `ZzzObject.cpp:3819-3822`).
fn raklion_penguin(o: &mut MapObject, ctx: &mut Ctx) {
    let (_, action) = anim_state(o);
    let speed = if action >= 2 { 1.0 } else { 0.8 };
    anim_advance(o, speed);

    let (frame, action) = anim_state(o);
    if o.ty == 16 {
        if frame >= 19.0 {
            let a = (ctx.rand() % 2) as usize;
            set_action(o, a);
        }
    } else if action <= 1 && frame >= 19.0 {
        let i = ctx.rand() % 100;
        let a = if i < 90 { (i % 2) as usize } else { 2 + (i % 2) as usize };
        set_action(o, a);
    } else if action == 2 && frame >= 97.0 {
        let a = (ctx.rand() % 2) as usize;
        set_action(o, a);
    } else if action == 3 && frame >= 98.0 {
        let a = (ctx.rand() % 2) as usize;
        set_action(o, a);
    }
    visual_luminosity(ctx);
}

/// Orcas 19, 20 y 21 (`RenderObjectMesh` 1165-1344): agua al saltar. Al acabar el salto pasan a la
/// acción 1 (`o->CurrentAction = 1`, que el modelo no tiene: la animación se queda quieta) y vuelven a
/// la 0 cuando `rand() % 40` da su número. Velocidad: `o->Velocity` (0.16).
fn raklion_whale(o: &mut MapObject, ctx: &mut Ctx) {
    anim_advance(o, o.velocity);
    let (frame, action) = anim_state(o);
    let white = Vec3::ONE;
    let bones = o.bones.as_ref().map_or(0, |b| b.len());
    let at = |o: &MapObject, bone: usize| o.bone_world(bone, Vec3::ZERO);
    match (o.ty, action) {
        (19, 0) => {
            let range: Option<Vec<usize>> = if (2.0..=5.0).contains(&frame) {
                Some((0..bones).collect())
            } else if (11.0..=12.0).contains(&frame) {
                Some(vec![6, 7, 8])
            } else if (13.0..=14.0).contains(&frame) {
                Some(vec![9, 10, 11])
            } else if (15.0..=17.0).contains(&frame) {
                Some((12..20).collect())
            } else {
                if frame >= 19.5 {
                    put_action(o, 1);
                }
                None
            };
            for bone in range.unwrap_or_default() {
                ctx.create_particle(BITMAP_WATERFALL_5, at(o, bone), o.angle, white, 7, 1.0, None);
            }
            for bone in 0..bones {
                ctx.create_particle(BITMAP_WATERFALL_3, at(o, bone), o.angle, white, 8, 1.5, None);
            }
        }
        (20, 0) => {
            ctx.create_particle(BITMAP_WATERFALL_3, at(o, 5), o.angle, white, 8, 2.0, None);
            if frame <= 8.0 {
                ctx.create_particle(BITMAP_WATERFALL_3, at(o, 6), o.angle, white, 8, 1.5, None);
                ctx.create_particle(BITMAP_WATERFALL_5, at(o, 6), o.angle, white, 7, 0.1, None);
            }
            if frame >= 12.0 {
                ctx.create_particle(BITMAP_WATERFALL_3, at(o, 6), o.angle, white, 8, 2.0, None);
                ctx.create_particle(BITMAP_WATERFALL_5, at(o, 6), o.angle, white, 7, 0.1, None);
            }
            if frame <= 15.0 {
                for bone in [8, 11] {
                    ctx.create_particle(BITMAP_WATERFALL_3, at(o, bone), o.angle, white, 9, 2.0, None);
                    ctx.create_particle(BITMAP_WATERFALL_5, at(o, bone), o.angle, white, 7, 0.1, None);
                }
            }
            if frame >= 19.0 {
                put_action(o, 1);
            }
        }
        (21, 0) => {
            if (4.0..=8.0).contains(&frame) {
                ctx.create_particle(BITMAP_WATERFALL_3, at(o, 7), o.angle, white, 9, 0.5, None);
                ctx.create_particle(BITMAP_WATERFALL_5, at(o, 7), o.angle, white, 7, 1.0, None);
                for (bone, scale) in [(7, 1.0), (16, 0.5), (17, 0.7), (20, 0.3), (21, 0.3), (22, 0.5), (23, 0.3), (24, 0.3)] {
                    ctx.create_particle(BITMAP_WATERFALL_2, at(o, bone), o.angle, white, 5, scale, None);
                }
            }
            if frame >= 19.0 {
                put_action(o, 1);
            }
        }
        (19 | 20 | 21, 1) => {
            let back = match o.ty {
                19 => 30,
                20 => 25,
                _ => 19,
            };
            if ctx.rand() % 40 == back {
                put_action(o, 0);
            }
        }
        _ => {}
    }
    visual_luminosity(ctx);
}

/// Fuego azul (70, mapa del jefe) y rojo (80): `RenderObjectVisual` 1720-1771.
fn raklion_fire(o: &mut MapObject, ctx: &mut Ctx) {
    o.hidden_mesh = -2;
    visual_luminosity(ctx);
    let light = if o.ty == 70 { Vec3::new(0.1, 0.4, 1.0) } else { Vec3::new(0.7, 0.2, 0.1) };
    match ctx.rand() % 3 {
        0 => ctx.create_particle(BITMAP_FIRE_HIK1_MONO, o.position, o.angle, light, 2, o.scale, None),
        1 => ctx.create_particle(BITMAP_FIRE_HIK2_MONO, o.position, o.angle, light, 6, o.scale, None),
        _ => ctx.create_particle(BITMAP_FIRE_HIK3_MONO, o.position, o.angle, light, 2, o.scale, None),
    }
    ctx.create_particle(BITMAP_FIRE_HIK1_MONO, o.position, o.angle, light, 2, o.scale, None);
}

/// Portal al jefe (`CreateObject` 73-85): tres anillos a 520 de altura (subtipo 1). El objeto
/// `MODEL_WARP4` en sí está a z = 0 (`MapManager.cpp:791-809`), bajo el suelo.
/// TODO(original): el propio modelo `warp01` del objeto (`RenderObjectMesh` 1393-1419) no se dibuja.
fn raklion_warp(o: &mut MapObject, ctx: &mut Ctx) {
    if o.created {
        return;
    }
    // En `CreateObject` la luz del objeto es (0, 0, 0); el efecto la pone en cada tick.
    let light = Vec3::ZERO;
    let p = o.position;
    ctx.create_effect(MODEL_WARP4, Vec3::new(p.x, p.y - 40.0, p.z + 520.0), o.angle, light, 1, None, -1, 0, 0, 0.0);
    ctx.create_effect(MODEL_WARP5, Vec3::new(p.x, p.y - 36.0, p.z + 520.0), o.angle, light, 1, None, -1, 0, 0, 0.0);
    ctx.create_effect(MODEL_WARP6, Vec3::new(p.x, p.y - 20.0, p.z + 520.0), o.angle, light, 1, None, -1, 0, 0, 0.0);
}

/// `CreateEffect(MODEL_WARP4/5)` (`ZzzEffect.cpp:674-686`).
fn warp_create(e: &mut Effect, ctx: &mut Ctx) {
    e.blend_mesh = -2;
    e.life_time = 0xffffff as f32;
    e.scale = 1.3 + rf(ctx, 50) / 100.0;
    e.gravity = rf(ctx, 80) / 10.0;
    e.velocity = rf(ctx, 100) / 1000.0 + 0.01;
}

/// `CreateEffect(MODEL_WARP6)` (`ZzzEffect.cpp:668-673`): `Gravity` no se inicializa (0).
fn warp6_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 0xffffff as f32;
    e.blend_mesh = -2;
    e.scale = 0.6;
    e.gravity = 0.0;
}

/// `MoveEffect` de `MODEL_WARP*` (`ZzzEffect.cpp:7334-7360`) + la cola común (`PlayAnimation`,
/// `MoveParticle(o, true)`).
fn warp_step(e: &mut Effect, ctx: &mut Ctx) {
    // `Luminosity` del principio de `MoveEffect`.
    let _ = ctx.rand();
    let t = ctx.world_time;
    if e.sub_type == 0 {
        let (t1, t2, t3) = ((t * 0.0011).sin() * 0.2, (t * 0.0017).sin() * 0.2, (t * 0.0013).sin() * 0.2);
        e.light = Vec3::new(t1 + 0.01, t2 + 0.01, t3 + 0.01);
        e.angle.y += 4.0 + e.gravity;
    } else if e.sub_type == 1 {
        let t1 = (t * 0.0011).sin() * 0.05;
        e.light = Vec3::new(t1, 0.2 + t1, 0.1 + t1);
        e.angle.y += 2.0 + e.gravity;
    }
    e.animation_frame += e.velocity;
    e.position += rotate(e.angle, e.direction);
}

/// `CGM_Raklion::RenderObjectMesh` (1393-1419): `BodyLight = (0.5, 0.6, 1)`, `BlendMeshLight = 0.8` y
/// `RenderBody(RENDER_TEXTURE)` (4), `RenderBody(RENDER_BRIGHT)` (5) o `RenderMesh(0)` (6); con
/// `BlendMesh = -2` todas las mallas suman con `BodyLight · BlendMeshLight`.
fn warp_draw(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    let mut d = ModelDraw::of_effect(e);
    d.light = WARP4_LIGHT;
    d.blend_mesh = -2;
    d.blend_mesh_light = WARP4_BLEND_LIGHT;
    out.models.push(d);
}

// --- Vulcanus (`GM_PK_Field.cpp`) ------------------------------------------------------------------

fn vulcanus(o: &mut MapObject, ctx: &mut Ctx) {
    if o.ty <= 6 {
        // `MoveObject` (318-342).
        o.hidden_mesh = -2;
    }
    if o.ty == 68 {
        vulcanus_fish(o, ctx);
    }
    visual_luminosity(ctx);
    // `RenderObjectVisual` (344-462).
    match o.ty {
        0 => {
            let lumi = ((ctx.world_time * 0.001).sin() + 1.0) * 0.5 * 100.0;
            let delay = (o.position.x as i32) % 3 + 1;
            let start = 30 * delay;
            let gap = if start != 90 { 40 } else { 10 };
            if lumi >= start as f32 && lumi <= (start + gap) as f32 {
                for _ in 0..20 {
                    // `o` como dueño: el subtipo 6 no lo usa.
                    ctx.create_particle(BITMAP_WATERFALL_2, o.position, o.angle, Vec3::ONE, 6, o.scale, None);
                }
            }
        }
        1 => {
            if ctx.rand() % 3 == 0 {
                ctx.create_particle(BITMAP_SMOKE, o.position, o.angle, Vec3::ONE, 60, o.scale, None);
            }
        }
        2 => {
            if ctx.rand() % 3 == 0 {
                ctx.create_particle(BITMAP_CLOUD, o.position, o.angle, Vec3::ZERO, 16, o.scale, None);
            }
        }
        3 => red_mist(o, ctx),
        4 => sparks(o, ctx),
        5 => {
            if ctx.rand() % 3 == 0 {
                // `o->Light = (0.3, 0.3, 0.3)`.
                ctx.create_particle(BITMAP_SMOKE, o.position, o.angle, Vec3::splat(0.3), 21, o.scale, None);
            }
        }
        6 => fire_point(o, ctx),
        _ => {}
    }
}

/// Pez de lava 68 (`RenderObjectMesh` 536-568): dos resplandores rojos en el hueso 6 y humo del
/// hueso 4 en los fotogramas 7-13. (El 67 no se dibuja: ver `rules_raklion`.)
fn vulcanus_fish(o: &MapObject, ctx: &mut Ctx) {
    let fire = Vec3::new(1.0, 0.0, 0.0);
    ctx.create_sprite(BITMAP_LIGHT, o.bone_world(6, Vec3::ZERO), o.scale * 5.0, fire, 0.0, 0);
    ctx.create_sprite(BITMAP_LIGHT, o.bone_world(6, Vec3::new(0.0, 0.0, -350.0)), o.scale * 5.0, fire, 0.0, 0);
    let px = rf(ctx, 20) - 30.0;
    let py = rf(ctx, 20) - 30.0;
    let pos = o.bone_world(4, Vec3::new(px, py, 0.0));
    let frame = group_frame(o);
    if (7.0..13.0).contains(&frame) {
        ctx.create_particle(BITMAP_SMOKE, pos, o.angle, Vec3::new(0.3, 0.1, 0.1), 18, o.scale * 1.5, None);
    }
}

// --- Barracks y Refuge de Balgass (`GM3rdChangeUp.cpp`) ---------------------------------------------

fn balgass(o: &mut MapObject, ctx: &mut Ctx) {
    // `PlayEffectSound` (356-375), al principio de `MoveObject`.
    match o.ty {
        74 | 75 => {
            let id = if ctx.rand() % 2 == 0 { "SOUND_3RD_CHANGE_UP_BG_CAGE1" } else { "SOUND_3RD_CHANGE_UP_BG_CAGE2" };
            play_once(ctx, id);
        }
        79 => play_once(ctx, "SOUND_3RD_CHANGE_UP_BG_VOLCANO"),
        92 => {
            if fire_pillar_sin(ctx) > 0.9 {
                play_once(ctx, "SOUND_3RD_CHANGE_UP_BG_FIREPILLAR");
            }
        }
        _ => {}
    }

    // `MoveObject` (77-117).
    match o.ty {
        2 | 5 | 58 | 59 | 60 | 85..=93 => o.hidden_mesh = -2,
        3 => {
            let l = (ctx.rand() % 4 + 3) as f32 * 0.1;
            ctx.add_terrain_light(o.position.x, o.position.y, Vec3::new(l, l * 0.6, l * 0.2), 3.0);
            o.hidden_mesh = -2;
        }
        84 => {
            o.position.z = ctx.terrain_height(o.position.x, o.position.y) + (ctx.world_time * 0.0005).sin() * 150.0 - 100.0;
        }
        _ => {}
    }

    visual_luminosity(ctx);
    // `RenderObjectVisual` (122-259).
    let white = Vec3::ONE;
    match o.ty {
        2 => {
            if ctx.rand() % 3 == 0 {
                ctx.create_particle(BITMAP_SMOKE, o.position, o.angle, white, 13, o.scale, None);
            }
        }
        3 => {
            if ctx.rand() % 3 == 0 {
                ctx.create_particle(BITMAP_TRUE_FIRE, o.position, o.angle, white, 0, o.scale, None);
            }
        }
        5 => {
            if ctx.rand() % 2 == 0 {
                // `(int)((o->Timer++) + 2) % 4`: con el valor de antes de sumar.
                let t = o.timer;
                o.timer += 1.0;
                if ((t + 2.0) as i32) % 4 == 0 {
                    ctx.create_particle(BITMAP_ADV_SMOKE + 1, o.position, o.angle, white, 0, 1.0, None);
                    ctx.create_particle(BITMAP_ADV_SMOKE, o.position, o.angle, white, 0, 1.0, None);
                }
            }
            if ctx.rand() % 2 == 0 {
                let t = o.timer;
                o.timer += 1.0;
                if (t as i32) % 4 == 0 {
                    ctx.create_particle(BITMAP_CLOUD, o.position, o.angle, white, 6, 1.0, None);
                    ctx.create_particle(BITMAP_ADV_SMOKE, o.position, o.angle, white, 1, 1.0, None);
                }
            }
        }
        6 => object_clouds(o, ctx, Vec3::splat(0.2), &[1, 2, 3, 4], 1),
        58 => {
            if ctx.rand() % 2 == 0 {
                ctx.create_particle(BITMAP_WATERFALL_1, o.position, o.angle, white, 2, o.scale, None);
            }
        }
        59 => {
            if ctx.rand() % 3 == 0 {
                ctx.create_particle(BITMAP_WATERFALL_2, o.position, o.angle, white, 1, o.scale, None);
            }
        }
        60 => ctx.create_particle(BITMAP_WATERFALL_3, o.position, o.angle, white, 3, o.scale, None),
        85 => {
            if ctx.rand() % 2 == 0 {
                let t = o.timer;
                o.timer += 1.0;
                if ((t + 2.0) as i32) % 4 == 0 {
                    if ctx.rand() % 2 == 0 {
                        ctx.create_particle(BITMAP_ADV_SMOKE + 1, o.position, o.angle, Vec3::ZERO, 1, o.scale, None);
                    }
                    ctx.create_particle(BITMAP_ADV_SMOKE, o.position, o.angle, Vec3::new(1.0, 0.4, 0.4), 2, o.scale, None);
                }
            }
            let light = Vec3::new(1.0, 0.4, 0.4);
            if ctx.rand() % 2 == 0 {
                let t = o.timer;
                o.timer += 1.0;
                if (t as i32) % 4 == 0 {
                    ctx.create_particle(BITMAP_CLOUD, o.position, o.angle, light, 14, o.scale, None);
                    ctx.create_particle(BITMAP_ADV_SMOKE, o.position, o.angle, light, 2, o.scale * 2.0, None);
                }
            }
        }
        88 => {
            if ctx.rand() % 4 == 0 {
                // `o` como dueño: el subtipo 10 no lo usa.
                ctx.create_particle(BITMAP_CLOUD, o.position, o.angle, white, 10, o.scale, None);
            }
        }
        89 => red_mist(o, ctx),
        90 => sparks(o, ctx),
        92 => {
            if fire_pillar_sin(ctx) > 0.9 {
                for _ in 0..2 {
                    // `CreateEffect(..., 0, NULL, -1, 0, o->Scale)`: la escala cae en `Skill` (WORD); la del
                    // efecto queda la de por defecto (0.9).
                    let kind = (o.scale as i32) & 0xFFFF;
                    ctx.create_effect(BITMAP_FIRE_RED, o.position, o.angle, Vec3::new(1.0, 0.4, 0.4), 0, None, -1, kind, 0, 0.0);
                }
            }
        }
        _ => {}
    }
}

/// `(sin(WorldTime·0.0005) + 1)·0.5` de la columna de fuego (92).
fn fire_pillar_sin(ctx: &Ctx) -> f32 {
    ((ctx.world_time * 0.0005).sin() + 1.0) * 0.5
}

fn draw_nothing(_: &Effect, _: &DrawCtx, _: &mut DrawList) {}

/// `CreateEffect(BITMAP_FIRE_RED)` (`ZzzEffect.cpp:1286-1288`): 40 ticks, sin modelo.
fn fire_red_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 40.0;
    e.visible = false;
}

/// `MoveEffect` de `BITMAP_FIRE_RED` (`ZzzEffect.cpp:8918-8923`): una llama roja al azar cerca.
fn fire_red_step(e: &mut Effect, ctx: &mut Ctx) {
    // `Luminosity` del principio de `MoveEffect`.
    let _ = ctx.rand();
    let x = (ctx.rand() % 32 - 16) as f32;
    let y = (ctx.rand() % 32 - 16) as f32;
    let position = e.position + Vec3::new(x, y, 0.0);
    ctx.create_particle(BITMAP_FIRE_RED, position, e.angle, Vec3::new(1.0, 0.4, 0.4), 0, e.scale, None);
    e.position += rotate(e.angle, e.direction);
}

// --- Karutan (`GMKarutan1.cpp`) ---------------------------------------------------------------------

fn karutan(o: &mut MapObject, ctx: &mut Ctx) {
    // `PlayObjectSound` (353-362), antes de `MoveObject` (`w_MapProcess.cpp:237`).
    if matches!(o.ty, 58 | 66) {
        play_once(ctx, "SOUND_KARUTAN_INSECT_ENV");
    }
    // `MoveObject` (46-69).
    match o.ty {
        113 => {
            let l = (ctx.rand() % 4 + 3) as f32 * 0.1;
            ctx.add_terrain_light(o.position.x, o.position.y, Vec3::new(l, l * 0.6, l * 0.2), 3.0);
            o.hidden_mesh = -2;
        }
        114..=116 | 118 => o.hidden_mesh = -2,
        // 66: `PlaySpeed` 0.15 en las dos acciones (`MapManager.cpp:1202-1203`); `RenderObjectMesh`
        // (192-198) cambia de acción al llegar al fotograma 19.
        66 => {
            anim_advance(o, 0.15);
            if anim_state(o).0 >= 19.0 {
                let a = if ctx.rand() % 10 == 0 { 1 } else { 0 };
                set_action(o, a);
            }
        }
        _ => {}
    }

    visual_luminosity(ctx);
    // `RenderObjectVisual` (71-171).
    match o.ty {
        66 => {
            let lumi = (ctx.world_time * 0.0004).sin().abs() * 0.4;
            let light = Vec3::new(lumi * 1.9, lumi * 1.1, lumi * 1.1);
            ctx.create_sprite(BITMAP_SHINY + 5, o.bone_world(13, Vec3::new(10.0, 0.0, -6.0)), 1.2, light, 0.0, 0);
            ctx.create_sprite(BITMAP_SHINY + 5, o.bone_world(14, Vec3::new(10.0, 0.0, 6.0)), 1.2, light, 0.0, 0);
        }
        72 => {
            let lumi = (ctx.world_time * 0.0004).sin().abs() * 0.4;
            let light0 = Vec3::new(lumi * 1.3, lumi * 1.3, lumi * 1.9);
            let light1 = Vec3::splat(0.15);
            let p = Vec3::new(20.0, 0.0, 0.0);
            let pos = o.bone_world(11, p);
            ctx.create_sprite(BITMAP_LIGHT, pos, 2.0, light0, 0.0, 0);
            ctx.create_sprite(BITMAP_SPARK + 1, pos, 1.5, light1, 0.0, 0);
            let pos = o.bone_world(7, p);
            ctx.create_sprite(BITMAP_SPARK + 1, pos, 4.0, light0, 0.0, 0);
            ctx.create_sprite(BITMAP_SPARK + 1, pos, 1.5, light1, 0.0, 0);
        }
        113 => fire_point(o, ctx),
        114 => {
            if ctx.rand() % 3 == 0 {
                ctx.create_particle(BITMAP_WATERFALL_3, o.position, o.angle, Vec3::splat(0.2), 16, o.scale, None);
            }
        }
        115 => object_clouds(o, ctx, Vec3::splat(0.04), &[0], 20),
        116 => {
            if ctx.rand() % 8 == 0 {
                ctx.create_particle(BITMAP_SMOKE, o.position, o.angle, Vec3::new(0.5, 0.9, 0.5), 69, o.scale, None);
                // `o->Light` de un objeto del mapa: (0, 0, 0). `o` como dueño: el subtipo 13 no lo usa.
                ctx.create_particle(BITMAP_SMOKE, o.position, o.angle, Vec3::ZERO, 13, o.scale * 2.0, None);
            }
        }
        118 => object_clouds(o, ctx, Vec3::new(0.27, 0.2, 0.1), &[0], 4),
        _ => {}
    }
}

// --- Chaos Castle (`CSChaosCastle.cpp`) -------------------------------------------------------------

/// Estado de todo el mapa (`MoveObjects` / `MoveChaosCastleObjectSetting`).
struct CastleTick {
    /// `world_time` del último tick en que se hizo `MoveChaosCastleObjectSetting`.
    time: f32,
    /// `objectCount` de `MoveObjects` en este tick.
    obj_count: i32,
    /// `visibleObject`: los objetos 3 de este tick (el `MoveObjectSetting` del siguiente usa este).
    visible: i32,
    /// `g_bActionMatch` (empieza en `true` y no se vuelve a poner nunca).
    action_match: bool,
}

static CASTLE: Mutex<CastleTick> = Mutex::new(CastleTick { time: f32::NAN, obj_count: 0, visible: 0, action_match: true });

/// `MoveChaosCastleObjectSetting` (97-119) la primera vez que se llama en cada tick.
fn castle_setting(ctx: &mut Ctx) {
    let visible = {
        let mut s = CASTLE.lock().unwrap_or_else(|e| e.into_inner());
        if s.time == ctx.world_time {
            return;
        }
        s.time = ctx.world_time;
        s.obj_count = 0;
        // `visibleObject = 0` va después de `MoveObjectSetting`: este usa el del tick anterior.
        std::mem::take(&mut s.visible)
    };
    if ctx.rand() % 10 == 0 && visible != 0 {
        let count = ctx.rand() % visible;
        CASTLE.lock().unwrap_or_else(|e| e.into_inner()).obj_count = count;
        return;
    }
    if !hero_in_safe_zone(ctx) && ctx.rand() % 10 == 0 {
        let Some(hero) = ctx.hero_actor().map(|a| (a.position, a.angle)) else { return };
        let x = hero.0.x + (ctx.rand() % 800) as f32 - 400.0;
        let y = hero.0.y + (ctx.rand() % 800) as f32 - 400.0;
        let position = Vec3::new(x, y, hero.0.z - 150.0);
        let scale = (ctx.rand() % 10) as f32 + 50.0;
        ctx.create_joint(BITMAP_JOINT_SPIRIT2, position, position, hero.1, 9, None, scale);
    }
}

/// `o->LifeTime` y `o->PKKey` del rayo de los objetos 0-3 (`user[0]`, `user[1]`).
const CASTLE_LIFE: usize = 0;
const CASTLE_PKKEY: usize = 1;

fn chaos_castle(o: &mut MapObject, ctx: &mut Ctx) {
    castle_setting(ctx);

    // `MoveChaosCastleObject` (121-152, desde `MoveObjectOnEffect`): el rayo cae sobre un objeto 3 de
    // los que se ven.
    if o.ty == 3 {
        let mut s = CASTLE.lock().unwrap_or_else(|e| e.into_inner());
        s.visible += 1;
        if s.action_match {
            o.user[CASTLE_LIFE] = 10.0;
            o.user[CASTLE_PKKEY] = 1.0;
            s.action_match = false;
        } else if s.obj_count != 0 {
            o.user[CASTLE_LIFE] = 10.0;
            o.user[CASTLE_PKKEY] = 0.0;
            s.obj_count -= 1;
            if s.obj_count == 0 {
                o.user[CASTLE_PKKEY] = 1.0;
            }
        }
    }

    visual_luminosity(ctx);
    // `RenderChaosCastleVisual` (303-488), con `g_byCurrCastleLevel` = 255 (sin evento).
    let mist = Vec3::new(0.05, 0.05, 0.1);
    match o.ty {
        6 => object_clouds(o, ctx, mist, &[0], 10),
        7 => object_clouds(o, ctx, mist, &[1], 10),
        8 => object_clouds(o, ctx, mist, &[2], 10),
        9 => object_clouds(o, ctx, mist, &[3], 5),
        10 => object_clouds(o, ctx, mist, &[4], 5),
        11 => object_clouds(o, ctx, mist, &[5], 5),
        12 => object_clouds(o, ctx, Vec3::splat(0.3), &[7], 7),
        18..=21 | 24..=35 => o.hidden_mesh = -2,
        0..=3 => {
            if o.user[CASTLE_PKKEY] != 0.0 && o.hidden_mesh != -2 {
                let position = o.bone_world(1, Vec3::ZERO);
                if o.user[CASTLE_LIFE] == 10.0 {
                    let scale = 60.0 + rf(ctx, 10);
                    ctx.create_joint(BITMAP_JOINT_THUNDER + 1, position, position, o.angle, 2, None, scale);
                    let id = if ctx.rand() % 2 == 0 { "SOUND_CHAOS_THUNDER01" } else { "SOUND_CHAOS_THUNDER02" };
                    play_once(ctx, id);
                }
                if o.user[CASTLE_LIFE] < 5.0 {
                    o.user[CASTLE_PKKEY] = 0.0;
                } else {
                    o.user[CASTLE_LIFE] -= 1.0;
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::play_animation;

    #[test]
    fn play_animation_como_el_original() {
        // 20 fotogramas sin `LockPositions`: 19.2 + 0.8 = 20 → vuelve a 0.
        assert!((play_animation(19.2, 0.8, 20, false) - 0.0).abs() < 1e-4);
        assert!((play_animation(19.5, 0.8, 20, false) - 0.3).abs() < 1e-4);
        // Con `LockPositions` la vuelta es en `keys - 1`.
        assert!((play_animation(18.5, 0.8, 20, true) - 0.3).abs() < 1e-4);
        // Un solo fotograma: no se mueve.
        assert_eq!(play_animation(0.0, 0.8, 1, false), 0.0);
        // Velocidad 5 (Karutan 107) con 30 fotogramas.
        assert!((play_animation(28.0, 5.0, 30, false) - 3.0).abs() < 1e-4);
    }
}
