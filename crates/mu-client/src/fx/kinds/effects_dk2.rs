//! Efectos del DK (2): Rageful Blow (`MODEL_SKILL_FURY_STRIKE` y sus +1..+8: el arma lanzada en
//! arco, el impacto con ondas, anillos y grietas), `MODEL_WAVE` (flashing.bmd), `MODEL_TAIL`
//! (tail.bmd) y las piedras que saltan (`MODEL_STONE1`/`MODEL_STONE2`).
//!
//! Traducido de `ZzzEffect.cpp` (`CreateEffect` ≈1490-1566 y ≈2192-2324, `MoveEffect` ≈6261-6466
//! y la cola común ≈7646-7711, `RenderFuryStrike` ≈7788), `Behaviors/MoveHandlers.cpp`
//! (`Move_MODEL_SKILL_FURY_STRIKE`, `Move_MODEL_WAVE`, `Move_MODEL_TAIL`, `Move_MODEL_ICE_SMALL`) y
//! `Data/Effects/EffectTypes.json` (creación de `MODEL_WAVE` y `MODEL_TAIL`).
//!
//! `FPS_ANIMATION_FACTOR` = 1. Como cada tipo tiene `step`, el núcleo no hace la cola común de
//! `MoveEffect` (`PlayAnimation` + `MoveParticle(o, true)`): la hace [`move_tail`] donde el original
//! la hace. `MoveEffect` saca siempre un `rand()` para `Luminosity` al empezar: [`luminosity`].

use bevy::math::Vec3;

use crate::fx::core::*;

/// Atributos de casilla (`_define.h`).
const TW_NOMOVE: u16 = 0x0004;
const TW_NOGROUND: u16 = 0x0008;
const TW_WATER: u16 = 0x0010;

pub fn register(r: &mut Registry) {
    let k = |create: EffectCreate, step: EffectStep, draw: Option<EffectDraw>| EffectKind { create: Some(create), step: Some(step), draw };
    r.effect(MODEL_SKILL_FURY_STRIKE, k(create_fury, step_fury, Some(draw_fury)));
    for n in 1..=8 {
        r.effect(MODEL_SKILL_FURY_STRIKE + n, k(create_fury_part, step_fury_part, None));
    }
    r.effect(MODEL_WAVE, k(create_wave, step_wave, None));
    r.effect(MODEL_TAIL, k(create_tail, step_tail, Some(draw_blend_all)));
    r.effect(MODEL_STONE1, k(create_stone, step_stone, None));
    r.effect(MODEL_STONE2, k(create_stone, step_stone, None));
}

// --- Utilidades -------------------------------------------------------------------------------------

/// `rand() % n` como `f32`.
fn rf(ctx: &mut Ctx, n: i32) -> f32 {
    (ctx.rand() % n) as f32
}

/// `Luminosity` del principio de `MoveEffect` (siempre gasta un `rand()`).
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

/// Cola común de `MoveEffect`: `PlayAnimation` (aprox.: avanzar el fotograma) y `MoveParticle(o, true)`.
fn move_tail(e: &mut Effect) {
    e.animation_frame += e.velocity;
    e.position += rotate(e.angle, e.direction);
}

/// `gMapManager.InHellas()`: Kalima 1-6 (24..=29) y la oculta (36).
fn in_hellas(world: i32) -> bool {
    (24..=29).contains(&world) || world == 36
}

/// `SetEarthQuakeAt`: temblor que se apaga con la distancia al héroe (12 casillas).
fn set_earthquake_at(ctx: &mut Ctx, pos: Vec3, magnitude: f32) {
    let Some(hero) = ctx.hero_actor().map(|h| h.position) else { return };
    const RADIUS: f32 = 100.0 * 12.0;
    let (dx, dy) = (pos.x - hero.x, pos.y - hero.y);
    let dist_sq = dx * dx + dy * dy;
    if dist_sq >= RADIUS * RADIUS {
        return;
    }
    ctx.set_earthquake(magnitude * (1.0 - dist_sq.sqrt() / RADIUS));
}

/// Luz roja de las grietas (azul si el dueño es `MODEL_WEREWOLF_HERO`).
fn crack_light(e: &Effect, lum: f32) -> Vec3 {
    if e.owner.is_some() {
        // TODO(original): Owner->Type == MODEL_WEREWOLF_HERO → (0, 0, Luminosity).
        Vec3::new(lum, 0.0, 0.0)
    } else {
        Vec3::ONE
    }
}

/// Piedra que salta desde una grieta (`+2` y `+5`).
fn spawn_crack_stone(e: &Effect, ctx: &mut Ctx) {
    let p = Vec3::new(0.0, rf(ctx, 150), 0.0);
    let angle = Vec3::new(0.0, 0.0, rf(ctx, 360));
    let pos = rotate(angle, p) + e.position;
    let ty = MODEL_STONE1 + ctx.rand() % 2;
    ctx.create_effect(ty, pos, e.angle, e.light, 0, None, -1, 0, 0, 0.0);
}

// --- MODEL_SKILL_FURY_STRIKE: el arma en arco -------------------------------------------------------

fn create_fury(e: &mut Effect, ctx: &mut Ctx) {
    e.head_angle = e.angle;
    e.start_position = e.position;
    e.life_time = 20.0;
    e.sub_type = ctx.rand() % 100;
    e.angle.z += 330.0;
    e.head_angle.x += 80.0;
    e.head_angle.z += 180.0;
    e.gravity = 50.0;
    // TODO(original): o->Weapon = CharacterMachine->PacketSerial++ (red).
}

fn step_fury(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    let light = Vec3::ONE;
    let mut angle = Vec3::ZERO;
    let mut scale: i32 = 150;
    let lt = e.life_time as i32;
    let owner_angle = ctx.actor(e.owner).map(|a| a.angle);
    let hellas = in_hellas(ctx.world_active);

    if lt == 11 {
        // El golpe: explosión, chispas, onda y las grietas.
        let light1 = Vec3::ONE;
        if e.kind != 3 {
            if let Some(oa) = owner_angle {
                e.start_position = rotate(oa, Vec3::new(-25.0, -80.0, 0.0)) + e.position;
            }
        }
        let mut add_height = 25.0;
        if hellas {
            if e.kind == 0 || e.kind == 2 {
                // TODO(original): AddWaterWave(x / TERRAIN_SCALE, y / TERRAIN_SCALE, 2, -1000).
            } else {
                add_height = 100.0;
            }
        }
        e.start_position.z = ctx.terrain_height(e.start_position.x, e.start_position.y) + add_height;
        add_height = 3.0;

        ctx.create_particle(BITMAP_EXPLOTION, e.start_position, angle, light1, 0, 0.5, None);

        if e.kind == 0 {
            for _ in 0..8 {
                if ctx.rand_fps_check(1) {
                    let ax = (ctx.rand() % 60) as f32 - 60.0;
                    let az = (ctx.rand() % 30 + 90) as f32;
                    angle = Vec3::new(ax, 0.0, az) + e.angle;
                    let mut pos = e.start_position;
                    pos.x += (ctx.rand() % 20 - 10) as f32;
                    pos.y += (ctx.rand() % 20 - 10) as f32;
                    ctx.create_joint(BITMAP_JOINT_SPARK, pos, pos, angle, 0, None, 10.0);
                    if ctx.rand() % 8 == 0 {
                        ctx.create_particle(BITMAP_SPARK, pos, angle, light, 0, 1.0, None);
                    }
                }
            }
        }
        angle = Vec3::ZERO;

        if e.kind == 0 {
            ctx.create_effect(MODEL_WAVE, e.start_position, angle, e.light, 0, None, -1, 0, 0, 0.0);
        }

        e.start_position.z -= 27.0;

        if e.owner.is_some() {
            // TODO(original): no se crean si Owner->Type == MODEL_WEREWOLF_HERO.
            // Dueño = este efecto; la escala va en PKKey (150 → 1.5).
            let me = Some(Target::Effect(e.id));
            ctx.create_effect(MODEL_SKILL_FURY_STRIKE + 3, e.start_position, angle, e.light, 0, me, scale, 0, 0, 0.0);
            ctx.create_effect(MODEL_SKILL_FURY_STRIKE + 1, e.start_position, angle, e.light, 0, me, scale, 0, 0, 0.0);
            ctx.create_effect(MODEL_SKILL_FURY_STRIKE + 2, e.start_position, angle, e.light, 0, me, scale, 0, 0, 0.0);
        }

        // Cinco grietas alrededor.
        for i in 0..5 {
            let p = Vec3::new(0.0, (ctx.rand() % 150 + 100) as f32, 0.0);
            angle = Vec3::new(0.0, 0.0, e.sub_type as f32 + i as f32 * 72.0);
            let mut pos = rotate(angle, p) + e.start_position;
            pos.z += 3.0;
            scale = ctx.rand() % 50 + 40;
            angle = Vec3::new(0.0, 0.0, 45.0 + (ctx.rand() % 30 - 15) as f32);

            let wall = ctx.terrain_attribute(pos.x, pos.y);
            if wall & TW_NOMOVE != TW_NOMOVE && wall & TW_NOGROUND != TW_NOGROUND && wall & TW_WATER != TW_WATER {
                if hellas {
                    add_height = 100.0;
                }
                pos.z = ctx.terrain_height(pos.x, pos.y) + add_height;
                ctx.create_effect(MODEL_SKILL_FURY_STRIKE + 4, pos, angle, e.light, 0, e.owner, scale, 0, 0, 0.0);
                ctx.create_effect(MODEL_SKILL_FURY_STRIKE + 5, pos, angle, e.light, 0, e.owner, scale, 0, 0, 0.0);
            }
        }
    } else if lt == 10 {
        // Grietas que se ramifican en cinco direcciones (4 tramos cada una).
        let mut pos = [e.start_position; 5];
        let mut ang = [0.0f32; 5];
        let mut count: i32 = 0;
        for j in 0..4 {
            let p = Vec3::new(0.0, (ctx.rand() % 15) as f32 + 85.0, 0.0);
            if j >= 3 {
                count = ctx.rand();
            }
            for i in 0..5 {
                let random = if count % 2 == 0 { ctx.rand() % 30 + 50 } else { -(ctx.rand() % 30 + 50) };
                ang[i] += random as f32;
                angle.z = ang[i] + (i as i32 * (ctx.rand() % 10 + 62)) as f32;
                pos[i] += rotate(angle, p);

                let wall = ctx.terrain_attribute(pos[i].x, pos[i].y);
                // (el original usa || aquí: casi siempre se cumple)
                if wall & TW_NOMOVE != TW_NOMOVE || wall & TW_NOGROUND != TW_NOGROUND || wall & TW_WATER != TW_WATER {
                    pos[i].z = ctx.terrain_height(pos[i].x, pos[i].y) + if hellas { 100.0 } else { 3.0 };
                    angle.z += 270.0;
                    ctx.create_effect(MODEL_SKILL_FURY_STRIKE + 7, pos[i], angle, e.light, 0, e.owner, 100, 0, 0, 0.0);
                    ctx.create_effect(MODEL_SKILL_FURY_STRIKE + 8, pos[i], angle, e.light, 0, e.owner, 100, 0, 0, 0.0);
                }
            }
            count += 1;
        }
        e.life_time = 0.0;
        // TODO(original): PlayBuffer(SOUND_FURY_STRIKE3).
    } else {
        // El arma sube y baja en arco.
        let mut count = e.life_time;
        let mut add_angle = 15.0;
        if e.life_time > 9.0 && e.life_time < 16.0 {
            count = 12.5;
            add_angle = 18.0;
            if lt == 15 {
                e.gravity *= -1.0;
            }
        }

        if e.kind == 0 && lt == 13 {
            // Estelas (`MODEL_TAIL`) detrás del arma.
            angle = Vec3::ZERO;
            let mut p0 = e.position;
            if let Some(oa) = owner_angle {
                p0 = rotate(oa, Vec3::new(-25.0, -40.0, 0.0)) + e.position;
            }
            for i in 0..4 {
                let position = Vec3::new(p0.x, p0.y, p0.z - (i * 50) as f32);
                ctx.create_effect(MODEL_TAIL, position, angle, e.light, 0, None, -1, 0, 0, 0.0);
            }
            p0.x += (ctx.rand() % 30 + 20) as f32;
            p0.z += (ctx.rand() % 500 - 250) as f32;
            for i in 0..4 {
                let position = Vec3::new(p0.x, p0.y, p0.z - (i * 30) as f32);
                ctx.create_effect(MODEL_TAIL, position, angle, e.light, 0, None, -1, 0, 0, 0.0);
            }
            // TODO(original): PlayBuffer(SOUND_FURY_STRIKE2).
        }

        let a = (20.0 - count) * add_angle;
        e.angle.x += 80.0;
        e.direction.y = (a * std::f32::consts::PI / 180.0).sin() * 260.0;
        e.position = e.start_position;
        if count != 12.5 {
            e.gravity += 8.0;
        } else {
            e.gravity -= 8.0;
        }
        e.position = rotate(e.head_angle, e.direction) + e.start_position;
        e.position.z += e.gravity;
        e.position.z += 200.0;
    }
    // Sin cola común (`MODEL_SKILL_FURY_STRIKE` está excluido).
}

/// `RenderFuryStrike`: el arma del dueño mientras `LifeTime > 10`.
fn draw_fury(e: &Effect, ctx: &DrawCtx, out: &mut DrawList) {
    if !(e.life_time > 10.0 && e.kind == 0) {
        return;
    }
    let Some(Target::Actor(owner)) = e.owner else { return };
    let Some(actor) = ctx.actors.get(&owner) else { return };
    let Some((model, _level)) = actor.weapon else { return };
    // ItemObjectAttribute: escala 0.8 (lanzas a bastones 0.7).
    let scale = if (MODEL_SPEAR..=MODEL_PLATINA_STAFF).contains(&model) { 0.7 } else { 0.8 };
    let mut d = ModelDraw::new(model, e.position, e.angle, scale);
    // TODO(original): luz = RequestModelTerrainLight + 0.3 (ItemObjectAttribute); brillo por nivel
    // del arma (RenderPartObject con WeaponLevel).
    d.light = Vec3::ONE;
    d.alpha = e.alpha;
    out.models.push(d);
}

// --- MODEL_SKILL_FURY_STRIKE+1..+8: anillos, grietas y ondas del impacto -----------------------------

fn create_fury_part(e: &mut Effect, _ctx: &mut Ctx) {
    let n = e.ty - MODEL_SKILL_FURY_STRIKE;
    let scale = e.pk_key as f32 / 100.0;
    match n {
        1 => {
            if e.sub_type == 0 {
                e.life_time = 35.0;
                e.scale = scale;
                e.blend_mesh = 0;
            } else if e.sub_type == 1 {
                e.life_time = 60.0;
                e.scale = scale;
                e.blend_mesh = 0;
            }
        }
        2 => {
            if e.sub_type == 0 || e.sub_type == 1 {
                e.life_time = 20.0;
                e.scale = scale;
                e.blend_mesh = 0;
                // TODO(original): SubType 1 → RenderType = RENDER_DARK.
            } else if e.sub_type == 2 {
                e.life_time = 50.0;
                e.scale = scale;
                e.blend_mesh = 0;
            }
        }
        3 | 6 => {
            e.life_time = 35.0;
            e.scale = scale;
        }
        4 => {
            e.life_time = 35.0;
            e.scale = scale;
            e.blend_mesh = 0;
        }
        5 | 8 => {
            e.life_time = 40.0;
            e.scale = scale;
            e.blend_mesh = 0;
            // TODO(original): SubType 1 → RenderType = RENDER_DARK.
        }
        7 => {
            e.life_time = 40.0;
            e.scale = scale;
            e.blend_mesh = 0;
        }
        _ => {}
    }
}

fn step_fury_part(e: &mut Effect, ctx: &mut Ctx) {
    let lum = luminosity(e, ctx);
    let n = e.ty - MODEL_SKILL_FURY_STRIKE;
    match n {
        1 => {
            e.blend_mesh_light = (e.life_time * 0.1) / 3.0;
            if e.life_time < 10.0 {
                e.position.z -= 0.5;
            }
            if e.life_time > 15.0 && (e.life_time as i32) % 3 == 0 {
                let amount = (ctx.rand() % 8 - 4) as f32 * 0.1;
                set_earthquake_at(ctx, e.position, amount);
            }
        }
        3 | 6 => {
            e.blend_mesh_light = (e.life_time * 0.1) / 10.0;
            if e.life_time < 13.0 {
                e.position.z -= 0.5;
            }
        }
        2 => {
            if e.scale > 50.0 {
                e.life_time = 0.0;
            }
            if e.life_time >= 10.0 {
                e.blend_mesh_light = (20.0 - e.life_time) * 0.1;
            } else {
                if e.life_time < 5.0 {
                    e.position.z -= 0.5;
                }
                e.blend_mesh_light = e.life_time * 0.1;
                if e.life_time >= 5.0 && ctx.rand_fps_check(10) {
                    spawn_crack_stone(e, ctx);
                }
            }
            e.blend_mesh_tex_coord.x = -((e.life_time as i32) as f32) * 0.01;
            let light = crack_light(e, lum);
            ctx.add_terrain_light(e.position.x, e.position.y, light, 1.0);
        }
        4 | 7 => {
            e.blend_mesh_light = (e.life_time * 0.1) / 3.0;
            if e.life_time < 10.0 {
                e.position.z -= 0.5;
            }
        }
        5 => {
            if e.life_time >= 30.0 {
                e.blend_mesh_light = (40.0 - e.life_time) * 0.1;
            } else {
                if e.life_time < 15.0 {
                    e.position.z -= 0.5;
                }
                e.blend_mesh_light = e.life_time * 0.1;
                if e.life_time >= 5.0 && ctx.rand_fps_check(15) {
                    spawn_crack_stone(e, ctx);
                }
            }
            e.blend_mesh_tex_coord.x = -((e.life_time as i32) as f32) * 0.01;
            let light = crack_light(e, lum);
            ctx.add_terrain_light(e.position.x, e.position.y, light, 1.0);
        }
        8 => {
            if e.life_time >= 30.0 {
                e.blend_mesh_light = (40.0 - e.life_time) * 0.1;
            } else {
                e.blend_mesh_light = e.life_time * 0.1;
            }
            if e.life_time < 15.0 {
                e.position.z -= 0.5;
            }
            // El original calcula una posición y no hace nada con ella (pero gasta el rand).
            if e.life_time >= 13.0 {
                let _ = ctx.rand_fps_check(5);
            }
            e.blend_mesh_tex_coord.x = -((e.life_time as i32) as f32) * 0.01;
            let light = crack_light(e, lum);
            ctx.add_terrain_light(e.position.x, e.position.y, light, 1.0);
        }
        _ => {}
    }
    move_tail(e);
}

// --- MODEL_WAVE (flashing.bmd) ------------------------------------------------------------------------

/// Creación desde `EffectTypes.json` ("wave").
fn create_wave(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 15.0;
    e.scale = 0.5;
    e.blend_mesh = 0;
    e.blend_mesh_light = 1.5;
    e.light = Vec3::ONE;
    e.position.z += -15.0;
}

fn step_wave(e: &mut Effect, ctx: &mut Ctx) {
    let lum = luminosity(e, ctx);
    if e.scale > 2.0 {
        e.scale += 0.1;
        e.position.x -= 1.0;
        e.position.z -= 1.5;
        e.blend_mesh_light = e.life_time / 30.0;
    } else {
        e.scale += 1.2;
        e.position.x -= 1.2;
        e.position.z -= 1.8;
    }
    // Oscurece el suelo alrededor.
    let light = Vec3::splat(-lum * 0.5);
    ctx.add_terrain_light(e.position.x, e.position.y, light, 5.0);
    move_tail(e);
}

// --- MODEL_TAIL (tail.bmd) ----------------------------------------------------------------------------

/// Creación desde `EffectTypes.json` ("tail").
fn create_tail(e: &mut Effect, _ctx: &mut Ctx) {
    e.life_time = 6.0;
    e.scale = 1.0;
    e.gravity = 80.0;
    e.blend_mesh = -2;
    e.light = Vec3::splat(0.5);
    e.angle = Vec3::new(0.0, 0.0, 45.0);
}

fn step_tail(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    e.position.z -= e.gravity;
    e.gravity += 60.0;
    e.blend_mesh_light = e.life_time / 20.0;
    move_tail(e);
}

/// `BlendMesh <= -2`: todas las mallas sumando con `light · BlendMeshLight` (models.rs solo
/// compara el índice, así que se dibuja como `bright` con alfa = `BlendMeshLight`).
fn draw_blend_all(e: &Effect, _ctx: &DrawCtx, out: &mut DrawList) {
    let mut d = ModelDraw::of_effect(e);
    d.bright = true;
    d.alpha = e.blend_mesh_light;
    d.blend_mesh = -1;
    out.models.push(d);
}

// --- MODEL_STONE1 / MODEL_STONE2: piedras que saltan --------------------------------------------------

fn create_stone(e: &mut Effect, ctx: &mut Ctx) {
    match e.sub_type {
        5 => {
            e.life_time = 60.0;
            e.scale = (ctx.rand() % 4 + 8) as f32 * 0.1;
        }
        11 => {
            e.life_time = 40.0;
            e.scale = (ctx.rand() % 4 + 8) as f32 * 0.8;
            e.angle.z = rf(ctx, 360);
        }
        10 => {
            let p1 = Vec3::new(0.0, (ctx.rand() % 256 + 64) as f32 * 0.2, 0.0);
            e.life_time = (ctx.rand() % 16 + 32) as f32;
            e.scale = (ctx.rand() % 4 + 15) as f32 * 0.05;
            e.angle.z = rf(ctx, 360);
            e.direction = rotate(e.angle, p1);
            e.gravity = (ctx.rand() % 16 + 28) as f32;
        }
        12 => {
            let p1 = Vec3::new(0.0, (ctx.rand() % 256 + 64) as f32 * 0.1, 0.0);
            e.life_time = (ctx.rand() % 16 + 32) as f32;
            e.scale = (ctx.rand() % 4 + 15) as f32 * 0.1;
            e.angle.z = rf(ctx, 360);
            e.direction = rotate(e.angle, p1);
            e.gravity = (ctx.rand() % 16 + 8) as f32;
        }
        13 | 14 => {
            e.direction = Vec3::ZERO;
            e.life_time = (ctx.rand() % 16 + 20) as f32;
            // `o->Scale *= Scale` (Scale = el argumento; el núcleo ya cambió 0 por 0.9).
            let call_scale = e.scale;
            e.scale = (ctx.rand() % 13 + 3) as f32 * 0.08;
            e.scale *= call_scale;
            e.gravity = (ctx.rand() % 3 + 3) as f32;
            e.angle.z = rf(ctx, 360);
            let p1 = Vec3::new(0.0, (ctx.rand() % 128 + 64) as f32 * 0.1, 0.0);
            e.head_angle = rotate(e.angle, p1);
            e.head_angle.z += 15.0;
        }
        _ => {
            // Sigue en el caso de MODEL_ICE_SMALL/METEO.
            let p1 = if e.sub_type == 1 { Vec3::ZERO } else { Vec3::new(0.0, (ctx.rand() % 256 + 64) as f32 * 0.1, 0.0) };
            e.life_time = (ctx.rand() % 16 + 32) as f32;
            e.scale = (ctx.rand() % 4 + 8) as f32 * 0.1;
            e.angle.z = rf(ctx, 360);
            e.direction = rotate(e.angle, p1);
            e.gravity = (ctx.rand() % 16 + 8) as f32;
        }
    }
}

fn step_stone(e: &mut Effect, ctx: &mut Ctx) {
    let _lum = luminosity(e, ctx);
    match e.sub_type {
        5 => {
            // Pegada al dueño; sin cola común.
            if let Some(p) = ctx.target_position(e.owner) {
                e.position = p + e.start_position;
            }
            return;
        }
        11 => {
            if e.life_time < 10.0 {
                e.alpha *= 0.8;
            }
        }
        13 | 14 => {
            e.head_angle.z -= e.gravity;
            e.position += e.head_angle;
            let height = ctx.terrain_height(e.position.x, e.position.y);
            e.angle.x += 0.5 * e.life_time;
            e.angle.y += 0.5 * e.life_time;
            if e.position.z + e.direction.z <= height {
                e.position.z = height;
                e.head_angle.x *= 0.6;
                e.head_angle.y *= 0.6;
                e.head_angle.z += 1.0 * e.life_time;
                if e.head_angle.z < 0.5 {
                    e.head_angle.z = 0.0;
                }
                e.alpha -= 0.1;
            }
        }
        _ => move_ice_small(e, ctx),
    }
    move_tail(e);
}

/// `Move_MODEL_ICE_SMALL` (la parte que usan las piedras).
fn move_ice_small(e: &mut Effect, ctx: &mut Ctx) {
    let sub = e.sub_type;
    if sub == 0 || sub == 10 || sub == 12 {
        e.position += e.direction;
    } else {
        e.position += rotate(e.angle, e.direction);
    }
    e.direction *= 0.9;
    e.position.z += e.gravity;
    if sub == 0 || sub == 10 || sub == 12 || sub == 13 {
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
        if (sub == 0 || sub == 12 || sub == 13) && ctx.rand_fps_check(10) {
            let st = 1 + ctx.rand() % 3;
            ctx.create_particle(BITMAP_FIRE, e.position, e.angle, e.light, st, 1.0, None);
        }
    } else if sub == 1 {
        e.gravity += 0.5;
    } else {
        e.angle.z += 20.0;
        e.gravity += 0.5;
    }
}
