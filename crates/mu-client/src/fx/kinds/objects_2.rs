//! Objetos del mapa con efectos (2): Exile, Arena, Atlans, Tarkan, Devil Square, Heaven y Blood
//! Castle 1-8. Traducido de `ZzzObject.cpp`: `MoveObject`, `MoveObjectSetting` /
//! `MoveHeavenThunder` / `MoveObjectOnEffect` (de `MoveObjects`) y `UpdateObjectVisual`.
//! `CreateObject` no crea efectos en estos mapas.
//!
//! Lo que el original hace una vez por tick para todo el mapa (`MoveObjectSetting`: truenos y
//! nubes de Heaven, destellos de Blood Castle) se hace aquí desde el primer objeto registrado de
//! ese mapa que se mueve en el tick (se detecta el tick nuevo por `world_time`). Por eso en
//! Heaven y Blood Castle se registran todos los tipos de objeto del mapa, aunque no tengan código
//! propio. El trueno de Devil Square va al principio de `MoveObject` y el original lo tira una vez
//! por objeto visible: aquí igual, con todos los tipos de Devil Square registrados.
//!
//! También van aquí dos efectos que solo crean estos mapas: `MODEL_CLOUD` (el relámpago de nube
//! de Heaven) y `BITMAP_LIGHT` (subtipo 0: las chispas que suben en Heaven), y un efecto propio
//! invisible ([`FX_MAP_ANCHOR`]) que hace de `o->Target` para las nubes de Heaven.

use std::sync::Mutex;

use bevy::math::Vec3;

use crate::fx::core::*;
use crate::fx::sim::OBJECT_FX_DISTANCE;

// `gMapManager.WorldActive` de estos mapas.
const WD_4LOSTTOWER: i32 = 4;
const WD_6STADIUM: i32 = 6;
const WD_7ATLANSE: i32 = 7;
const WD_8TARKAN: i32 = 8;
const WD_9DEVILSQUARE: i32 = 9;
const WD_10HEAVEN: i32 = 10;
const WD_11BLOODCASTLE1: i32 = 11;
const WD_11BLOODCASTLE_END: i32 = 17;
const WD_52BLOODCASTLE_MASTER_LEVEL: i32 = 52;

/// Efecto propio (no existe en el original): marca invisible que sigue viva mientras su objeto
/// del mapa se mueve (el héroe a menos de [`OBJECT_FX_DISTANCE`]). Las nubes de Heaven la usan
/// como `o->Target`, que en el original es el objeto del mapa.
pub const FX_MAP_ANCHOR: i32 = -2_000_001;

/// `Random::Rand() % n` en coma flotante.
fn rf(ctx: &mut Ctx, n: i32) -> f32 {
    (ctx.rand() % n) as f32
}

fn in_blood_castle(world: i32) -> bool {
    (WD_11BLOODCASTLE1..=WD_11BLOODCASTLE_END).contains(&world) || world == WD_52BLOODCASTLE_MASTER_LEVEL
}

pub fn register(r: &mut Registry) {
    // Arena (WD_6STADIUM).
    r.object(WD_6STADIUM, 9, stadium_9);
    // En `UpdateObjectVisual` el caso de Lost Tower no tiene `break` y cae en el de la Arena:
    // los objetos 9 de Lost Tower también encienden esta luz (si nadie lo ha registrado ya).
    if !r.objects.contains_key(&(WD_4LOSTTOWER, 9)) {
        r.object(WD_4LOSTTOWER, 9, stadium_9);
    }

    // Atlans (WD_7ATLANSE): burbujas.
    r.object(WD_7ATLANSE, 22, atlans_22);

    // Tarkan (WD_8TARKAN).
    for ty in [4, 7, 61, 65, 66, 60, 63, 64, 70, 76, 83] {
        r.object(WD_8TARKAN, ty, tarkan);
    }

    // Devil Square (WD_9DEVILSQUARE): el trueno va en todos los objetos.
    for ty in 0..=10 {
        r.object(WD_9DEVILSQUARE, ty, devil_square);
    }

    // Heaven (WD_10HEAVEN): todos los tipos (para `MoveObjectSetting` y `MoveObjectOnEffect`).
    for ty in 0..=15 {
        r.object(WD_10HEAVEN, ty, heaven);
    }

    // Blood Castle 1-7 y 8 (WD_52): todos los tipos (para `MoveObjectSetting`).
    for world in (WD_11BLOODCASTLE1..=WD_11BLOODCASTLE_END).chain([WD_52BLOODCASTLE_MASTER_LEVEL]) {
        for ty in 0..=40 {
            r.object(world, ty, blood_castle);
        }
    }

    r.effect(MODEL_CLOUD, EffectKind { create: Some(cloud_create), step: None, draw: None });
    r.effect(BITMAP_LIGHT, EffectKind { create: Some(light_create), step: Some(light_step), draw: Some(draw_nothing) });
    r.effect(FX_MAP_ANCHOR, EffectKind { create: Some(anchor_create), step: Some(anchor_step), draw: Some(draw_nothing) });
}

// --- Estado por tick de todo el mapa (`MoveObjects`) ---------------------------------------------

struct TickState {
    /// `world_time` del último tick en que se hizo `MoveObjectSetting`.
    time: f32,
    /// `objectCount` de este tick (lo que devuelve `MoveHeavenThunder`).
    obj_count: i32,
    /// `visibleObject`: se cuenta en este tick; `MoveHeavenThunder` usa el del anterior.
    visible: i32,
}

static TICK: Mutex<TickState> = Mutex::new(TickState { time: f32::NAN, obj_count: 0, visible: 0 });

/// `MoveObjectSetting` la primera vez que se llama en cada tick.
fn object_setting(ctx: &mut Ctx) {
    let prev_visible = {
        let mut s = TICK.lock().unwrap_or_else(|e| e.into_inner());
        if s.time == ctx.world_time {
            return;
        }
        s.time = ctx.world_time;
        s.obj_count = 0;
        // `visibleObject = 0` va después de `MoveObjectSetting`: este usa el del tick anterior.
        std::mem::take(&mut s.visible)
    };
    // TODO(original): MoveChaosCastleObjectSetting / Hellas / BattleCastle (otros mapas).
    if ctx.world_active == WD_10HEAVEN {
        let count = move_heaven_thunder(ctx, prev_visible);
        TICK.lock().unwrap_or_else(|e| e.into_inner()).obj_count = count;

        if ctx.rand() % 10 == 0 {
            let Some(hero) = ctx.hero_actor().map(|a| a.position) else { return };
            let x = hero.x + (ctx.rand() % 5000 - 2500) as f32;
            let y = hero.y + (ctx.rand() % 5000 - 2500) as f32;
            let position = Vec3::new(x, y, hero.z - 1000.0);
            ctx.create_effect(BITMAP_LIGHT, position, Vec3::ZERO, Vec3::ONE, 0, None, -1, 0, 0, 0.0);
        }
    } else if in_blood_castle(ctx.world_active) {
        let Some(hero) = ctx.hero_actor().map(|a| a.position) else { return };
        let x = hero.x + (ctx.rand() % 900 - 300) as f32;
        let y = hero.y + (ctx.rand() % 900 - 300) as f32;
        let z = hero.z + (ctx.rand() % 50) as f32 + 250.0;
        if ctx.rand_fps_check(4) {
            ctx.create_particle(BITMAP_FLARE, Vec3::new(x, y, z), Vec3::ZERO, Vec3::ONE, 3, 0.19, None);
        }
    }
}

/// `MoveHeavenThunder`: relámpago entre las nubes alrededor del héroe.
fn move_heaven_thunder(ctx: &mut Ctx, visible_object: i32) -> i32 {
    let mut object_count = 0;
    if !ctx.rand_fps_check(50) {
        return object_count;
    }
    let Some(hero) = ctx.hero_actor().map(|a| a.position) else { return object_count };

    let px = hero.x + (ctx.rand() % 300) as f32 - 150.0;
    let py = hero.y;
    let _pz = hero.z + (ctx.rand() % 300) as f32 - 150.0;

    let luminosity = (ctx.rand() % 4 + 4) as f32 * 0.05;
    let light = Vec3::new(luminosity * 0.3, luminosity * 0.3, luminosity * 0.081);
    ctx.add_terrain_light(px, py, light, 2.0);
    ctx.create_effect(MODEL_CLOUD, hero, Vec3::ZERO, light, 0, None, -1, 0, 0, 0.0);

    if visible_object != 0 {
        object_count = ctx.rand() % visible_object;
    }

    if ctx.rand() % 5 == 0 {
        let angle1 = Vec3::new(0.0, 0.0, -45.0);
        let (pos, angle2, offset) = match ctx.rand() % 4 {
            0 => (hero + rotate(angle1, Vec3::new(-400.0, -1000.0, 0.0)), 240.0, Vec3::new(-200.0, -1000.0, 0.0)),
            1 => (hero - rotate(angle1, Vec3::new(-300.0, -400.0, 0.0)), 210.0, Vec3::new(-500.0, -1000.0, 0.0)),
            2 => (hero + rotate(angle1, Vec3::new(-200.0, -400.0, 0.0)), 235.0, Vec3::new(-1000.0, -1500.0, 0.0)),
            _ => (hero + rotate(angle1, Vec3::new(-200.0, 400.0, 0.0)), 200.0, Vec3::new(-600.0, -1200.0, 0.0)),
        };
        let offset = rotate(Vec3::new(0.0, 0.0, angle2), offset);
        let mut position2 = pos - offset;
        let mut position = pos + offset;
        position.z -= 300.0;
        position2.z -= 300.0;
        // `angle[2] = 0`: el ángulo queda (0, 0, 0).
        let angle = Vec3::ZERO;
        for _ in 0..2 {
            let scale = 40.0 + rf(ctx, 10);
            ctx.create_joint(BITMAP_JOINT_THUNDER + 1, position, position2, angle, 0, None, scale);
        }
    }
    object_count
}

// --- Arena (WD_6STADIUM) ------------------------------------------------------------------------

fn stadium_9(o: &mut MapObject, ctx: &mut Ctx) {
    // `Luminosity` del principio de `UpdateObjectVisual`.
    let luminosity = (ctx.rand() % 30 + 70) as f32 * 0.01;
    let scale = luminosity * 5.0;
    let light = Vec3::new(luminosity * 0.6, luminosity * 0.3, luminosity * 0.1);
    // El original pasa `Position` sin inicializar como desplazamiento: se toma (0, 0, 0).
    let p = o.bone_world(1, Vec3::ZERO);
    ctx.create_sprite(BITMAP_LIGHT, p, scale, light, 0.0, 0);
}

// --- Atlans (WD_7ATLANSE) -----------------------------------------------------------------------

fn atlans_22(o: &mut MapObject, ctx: &mut Ctx) {
    o.hidden_mesh = -2;
    o.timer += 0.1;
    if o.timer > 10.0 {
        o.timer = 0.0;
    }
    if o.timer > 5.0 {
        ctx.create_particle(BITMAP_BUBBLE, o.position, o.angle, o.light, 0, 1.0, None);
    }
}

// --- Tarkan (WD_8TARKAN) ------------------------------------------------------------------------

fn tarkan(o: &mut MapObject, ctx: &mut Ctx) {
    let wt = ctx.world_time;
    // MoveObject: luces del suelo (el brillo de las mallas lo hace objects.rs).
    match o.ty {
        4 => {
            let sine = (wt * 0.002).sin() * 0.35 + 0.65;
            ctx.add_terrain_light(o.position.x, o.position.y, Vec3::splat(sine), 3.0);
        }
        7 => {
            let sine = ((wt + o.angle.z * 100.0) * 0.002).sin() * 0.35 + 0.65;
            ctx.add_terrain_light(o.position.x, o.position.y, Vec3::new(sine, sine * 0.6, sine * 0.2), 3.0);
        }
        61 | 65 | 66 => {
            let l = (wt * 0.002).sin() * 0.35 + 0.65;
            ctx.add_terrain_light(o.position.x, o.position.y, Vec3::new(l, l * 0.6, l * 0.2), 2.0);
        }
        _ => {}
    }

    // UpdateObjectVisual.
    match o.ty {
        60 => {
            // `if (HiddenMesh != -2)`: solo la primera vez (objects.rs ya lo crea oculto).
            // El humo 6 no muere nunca.
            if !o.created {
                for _ in 0..20 {
                    ctx.create_particle(BITMAP_SMOKE, o.position, o.angle, o.light, 6, o.scale, None);
                }
            }
            o.hidden_mesh = -2;
        }
        63 | 64 => {
            let luminosity = ((wt + o.angle.z * 5.0) * 0.002).sin() * 0.3 + 0.7;
            let scale = luminosity * 1.5;
            let light = if o.ty == 63 {
                Vec3::new(luminosity / 1.7, luminosity, luminosity)
            } else {
                Vec3::new(luminosity, luminosity * 0.32, luminosity * 0.32)
            };
            // Hueso 2 del modelo: objects.rs no carga el modelo de los objetos ocultos (ver notas).
            let position = o.bone_world(2, Vec3::ZERO);
            ctx.create_sprite(BITMAP_IMPACT, position, scale, light, 0.0, 0);
        }
        70 => {
            o.hidden_mesh = -2;
            if ctx.rand_fps_check(5) {
                ctx.create_particle(BITMAP_SMOKE, o.position, o.angle, o.light, 7, o.scale, None);
            }
        }
        76 => {
            o.hidden_mesh = -2;
            let smoke = (wt as i32) % 5000 > 4500;
            if smoke {
                ctx.create_particle(BITMAP_SMOKE, o.position, o.angle, o.light, 4, o.scale, None);
            }
        }
        83 => {
            o.hidden_mesh = -2;
            let inter = (o.angle.z as i32) * 10;
            let timing = (wt as i32) % 10000;
            let smoke = timing > 3500 + inter && timing < 4000 + inter;
            if smoke {
                ctx.create_particle(BITMAP_SMOKE, o.position, o.angle, o.light, 8, o.scale, None);
                if ctx.rand_fps_check(3) {
                    let x = o.position.x + (ctx.rand() % 128 - 64) as f32;
                    let y = o.position.y + (ctx.rand() % 128 - 64) as f32;
                    let position = Vec3::new(x, y, o.position.z);
                    ctx.create_particle(BITMAP_SMOKE, position, o.angle, o.light, 4, o.scale * 0.5, None);
                    let ty = MODEL_STONE1 + ctx.rand() % 2;
                    ctx.create_effect(ty, o.position, o.angle, o.light, 0, None, -1, 0, 0, 0.0);
                }
            }
        }
        _ => {}
    }
}

// --- Devil Square (WD_9DEVILSQUARE) -------------------------------------------------------------

fn devil_square(o: &mut MapObject, ctx: &mut Ctx) {
    // Principio de MoveObject: relámpagos de la tormenta, una tirada por objeto.
    if (ctx.world_time as i32) % 4000 < 1000 && ctx.rand_fps_check(100) {
        let luminosity = (ctx.rand() % 12 + 4) as f32 * 0.1;
        let light = Vec3::new(luminosity * 0.2, luminosity * 0.3, luminosity * 0.5);
        if let Some(hero) = ctx.hero_actor().map(|a| a.position) {
            let x = hero.x + (ctx.rand() % 1200) as f32 - 600.0;
            let y = hero.y + (ctx.rand() % 1200) as f32 - 600.0;
            ctx.add_terrain_light(x, y, light, 12.0);
        }
        // TODO(original): PlayBuffer(SOUND_THUNDER01) (comentado en el original).
    }
    // TODO(original): PlayBuffer(SOUND_RAIN01, NULL, true).

    // UpdateObjectVisual: gotas que caen de la estatua.
    if o.ty == 2 {
        let light = Vec3::ONE;
        let p = Vec3::new(-15.0, 0.0, 0.0);
        if ctx.rand_fps_check(4) {
            let position = o.bone_world(23, p);
            ctx.create_particle(BITMAP_RAIN_CIRCLE + 1, position, o.angle, light, 0, 1.0, None);
        }
        if ctx.rand_fps_check(4) {
            let position = o.bone_world(31, p);
            ctx.create_particle(BITMAP_RAIN_CIRCLE + 1, position, o.angle, light, 0, 1.0, None);
        }
        let position = o.bone_world(23, p);
        ctx.create_particle(BITMAP_RAIN_CIRCLE + 1, position, o.angle, light, 0, 1.0, None);
    }
}

// --- Heaven (WD_10HEAVEN) -----------------------------------------------------------------------

fn heaven(o: &mut MapObject, ctx: &mut Ctx) {
    object_setting(ctx);

    // MoveObjectOnEffect: con trueno en este tick, rayos sobre algunas nubes.
    let obj_count = {
        let mut s = TICK.lock().unwrap_or_else(|e| e.into_inner());
        s.visible += 1;
        s.obj_count
    };
    if obj_count != 0 {
        if ctx.rand_fps_check(10) && (0..=5).contains(&o.ty) {
            let light = Vec3::new(rf(ctx, 10) / 50.0, rf(ctx, 10) / 50.0, rf(ctx, 10) / 50.0);
            ctx.create_sprite(BITMAP_CLOUD + 1, o.position, 0.5, light, 0.0, 0);
            for _ in 0..2 {
                let scale = rf(ctx, 20) + 10.0;
                ctx.create_joint(BITMAP_JOINT_THUNDER, o.position, o.position, o.angle, 6, None, scale);
            }
        } else {
            TICK.lock().unwrap_or_else(|e| e.into_inner()).visible -= 1;
        }
    }

    // UpdateObjectVisual.
    match o.ty {
        0..=5 => heaven_clouds(o, ctx),
        10 => {
            let position = o.bone_world(3, Vec3::ZERO);
            ctx.create_particle(BITMAP_LIGHT, position, o.angle, Vec3::ONE, 0, 1.0, None);
        }
        _ => {}
    }
}

/// Nubes 0-5: el original las crea cuando `HiddenMesh != -2` y las deja seguir al objeto
/// (`o->Target`) mientras se ve; al morir ponen `HiddenMesh = 0` y se vuelven a crear la próxima
/// vez que se dibuja. Aquí las sigue una marca ([`FX_MAP_ANCHOR`]) que muere cuando el objeto deja
/// de moverse, y se vuelven a crear cuando el objeto vuelve a moverse tras un hueco.
fn heaven_clouds(o: &mut MapObject, ctx: &mut Ctx) {
    // `user[0]`: `world_time` de la última vez que se movió este objeto.
    let gap = ctx.world_time - o.user[0];
    o.user[0] = ctx.world_time;
    if !o.created || gap > 100.0 {
        let anchor = ctx.create_effect(FX_MAP_ANCHOR, o.position, Vec3::ZERO, Vec3::ZERO, 0, None, -1, 0, 0, 0.0);
        let light = Vec3::splat(0.1);
        let count = if o.ty <= 2 { 20 } else { 10 };
        for _ in 0..count {
            ctx.create_particle(BITMAP_CLOUD, o.position, o.angle, light, o.ty, o.scale, Some(Target::Effect(anchor)));
        }
    }
    o.hidden_mesh = -2;
}

// --- Blood Castle (WD_11BLOODCASTLE1..7, WD_52BLOODCASTLE_MASTER_LEVEL) -------------------------

fn blood_castle(o: &mut MapObject, ctx: &mut Ctx) {
    object_setting(ctx);

    let wt = ctx.world_time;
    match o.ty {
        11 => {
            const INDEX_LIGHT: [usize; 7] = [1, 2, 4, 6, 9, 10, 11];
            let luminosity = ((o.angle.z * 20.0 + wt) * 0.001).sin() * 0.5 + 0.5;
            let light = Vec3::new(luminosity, luminosity * 0.5, 0.0);
            for bone in INDEX_LIGHT {
                let position = o.bone_world(bone, Vec3::new(0.0, 0.0, 2.0));
                ctx.create_sprite(BITMAP_LIGHT, position, 0.5, light, 0.0, 0);
            }
        }
        13 => {
            let luminosity = (wt * 0.001).sin() * 0.3 + 0.7;
            let position = o.bone_world(3, Vec3::ZERO);
            ctx.create_sprite(BITMAP_FLARE, position, luminosity + 0.5, Vec3::splat(luminosity), 0.0, 0);
        }
        37 => {
            let light = Vec3::ONE;
            if ctx.rand_fps_check(2) {
                // `(int)((o->Timer++) + 2) % 4`: con el valor de antes de sumar.
                let t = o.timer;
                o.timer += 1.0;
                if ((t + 2.0) as i32) % 4 == 0 {
                    ctx.create_particle(BITMAP_ADV_SMOKE + 1, o.position, o.angle, light, 0, 1.0, None);
                    ctx.create_particle(BITMAP_ADV_SMOKE, o.position, o.angle, light, 0, 1.0, None);
                }
            }
            if ctx.rand_fps_check(2) {
                let t = o.timer;
                o.timer += 1.0;
                if (t as i32) % 4 == 0 {
                    ctx.create_particle(BITMAP_CLOUD, o.position, o.angle, light, 6, 1.0, None);
                    ctx.create_particle(BITMAP_ADV_SMOKE, o.position, o.angle, light, 1, 1.0, None);
                    let light = Vec3::new(1.0, 0.8, 0.8);
                    ctx.create_particle(BITMAP_FLARE, o.position, o.angle, light, 4, 0.19, None);
                }
            }
        }
        _ => {}
    }
}

// --- Efectos ------------------------------------------------------------------------------------

fn draw_nothing(_: &Effect, _: &DrawCtx, _: &mut DrawList) {}

/// `MODEL_CLOUD` (EffectTypes.json): dura 2 ticks, escala 10, malla 0 sumando, sin luz del
/// terreno y desplazado (0, 200, -190). Se mueve con el paso por defecto (anima y no avanza).
fn cloud_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 2.0;
    e.scale = 10.0;
    e.blend_mesh = 0;
    e.light_enable = false;
    e.position += Vec3::new(0.0, 200.0, -190.0);
}

/// `CreateEffect(BITMAP_LIGHT)` (ZzzEffect.cpp ≈787). Solo el subtipo 0 (Heaven).
fn light_create(e: &mut Effect, ctx: &mut Ctx) {
    if e.sub_type == 0 {
        e.light = Vec3::splat(0.3);
        let f_angle = 70.0_f32.to_radians();
        let f_angle2 = 30.0_f32;
        let f_speed = (9 + ctx.rand() % 5) as f32 * 0.5;
        e.direction.x = f_speed * f_angle.cos() * f_angle2.to_radians().sin();
        e.direction.y = f_speed * f_angle.cos() * f_angle2.to_radians().cos();
        e.direction.z = f_speed * f_angle.sin();
        e.scale = 3.0;
        e.life_time = 400.0;
    }
    // TODO(original): subtipos 1 y 2 (buff de vida, con dueño personaje) y el resto.
}

/// `MoveEffect` de `BITMAP_LIGHT` subtipo 0 (≈6145) + la cola común (`MoveParticle(o, true)`).
/// El original, si sigue vivo, se vuelve a mover en el mismo tick 1 de cada 2 veces (≈7720),
/// y cada movimiento resta 1 de vida.
fn light_step(e: &mut Effect, ctx: &mut Ctx) {
    loop {
        light_move(e, ctx);
        // La última resta de vida la hace el núcleo después de esta función.
        if e.sub_type != 0 || e.life_time - 1.0 <= 0.0 || !ctx.rand_fps_check(2) {
            break;
        }
        e.life_time -= 1.0;
    }
}

fn light_move(e: &mut Effect, ctx: &mut Ctx) {
    // `Luminosity` del principio de `MoveEffect` (gasta un rand()).
    let _ = ctx.rand();
    if e.sub_type == 0 {
        e.position += e.direction;
        e.direction.z -= 0.01;
        ctx.create_particle(BITMAP_LIGHT, e.position, e.angle, e.light, 1, e.scale, None);
        if e.direction.z < -2.0 {
            e.life_time = 0.0;
        }
    }
    // Cola común: `MoveParticle(o, true)`.
    e.position += rotate(e.angle, e.direction);
}

/// Marca de un objeto del mapa: vive mientras el héroe está a menos de [`OBJECT_FX_DISTANCE`]
/// (lo mismo que decide si el objeto se mueve).
fn anchor_create(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 2.0;
    e.visible = false;
}

fn anchor_step(e: &mut Effect, ctx: &mut Ctx) {
    let far = ctx
        .hero_actor()
        .is_some_and(|h| (e.position.truncate() - h.position.truncate()).length() > OBJECT_FX_DISTANCE);
    if far {
        e.live = false;
    } else {
        e.life_time = 2.0;
    }
}
