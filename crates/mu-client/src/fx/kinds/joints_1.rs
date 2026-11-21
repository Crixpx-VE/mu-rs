//! Estelas (1): cola de escorpión, energía, curación, fantasma de dos líneas, espíritus, láser,
//! chispas, humo, rayo de Fenrir, láser/desenfoque +1 y rayos. Traducido de `ZzzEffectJoint.cpp`
//! (`CreateJoint`, `MoveJoint` y lo que `UpdateDrawnJointEffects` hace con ellas).
//!
//! `FPS_ANIMATION_FACTOR` = 1 (25 ticks/s): las multiplicaciones por él se han quitado.
//!
//! Colas: estos tipos dejan sus colas a mano en vez de con el `follow_tail` del núcleo, porque el
//! original las deja con la matriz del ángulo que tenía la estela al EMPEZAR `MoveJoint` (antes de
//! que `MoveHumming` y compañía la giren), algunos subtipos no tienen cola inicial
//! (`NumTails = -1`) y la energía 54 usa la cola con desenfoque. El `m_bCreateTails` del original
//! se guarda en `user[3]` y `joint.create_tails` se deja a `false` al salir de `create`/`step`
//! para que el núcleo no añada otra cola.

use bevy::math::Vec3;

use crate::fx::core::*;

/// `EndlessLifeTime`: estela que vive hasta que algo la quita.
const ENDLESS_LIFE_TIME: f32 = 1.0e9;
/// Campo `user` donde se guarda `m_bCreateTails`.
const TAILS_FLAG: usize = 3;
/// Campo `user` con un índice de "ranura" para `GetMagicScrew(iIndex)` (los espíritus 3 y 13).
const SLOT_INDEX: usize = 0;
/// `MAX_JOINTS` del original (para el índice de ranura).
const MAX_JOINTS: i32 = 500;

pub fn register(r: &mut Registry) {
    let k = |create: JointCreate, step: JointStep| JointKind { create: Some(create), step: Some(step) };
    r.joint(BITMAP_SCOLPION_TAIL, k(create_scolpion_tail, step_scolpion_tail));
    r.joint(BITMAP_JOINT_ENERGY, k(create_energy, step_energy));
    r.joint(BITMAP_JOINT_HEALING, k(create_healing, step_healing));
    r.joint(BITMAP_2LINE_GHOST, k(create_2line_ghost, step_2line_ghost));
    r.joint(BITMAP_JOINT_SPIRIT, k(create_spirit, step_spirit));
    r.joint(BITMAP_JOINT_SPIRIT2, k(create_spirit, step_spirit));
    r.joint(BITMAP_JOINT_LASER, k(create_laser, step_laser));
    r.joint(BITMAP_JOINT_SPARK, k(create_spark, step_spark));
    r.joint(BITMAP_SMOKE, k(create_smoke, step_smoke));
    r.joint(MODEL_FENRIR_SKILL_THUNDER, k(create_fenrir_thunder, step_fenrir_thunder));
    r.joint(BITMAP_JOINT_LASER + 1, k(create_blur1, step_blur1));
    r.joint(BITMAP_BLUR + 1, k(create_blur1, step_blur1));
    r.joint(BITMAP_JOINT_THUNDER, k(create_thunder, step_thunder));
}

// --- Utilidades -------------------------------------------------------------------------------------

/// `rand() % n` como `f32`.
fn rf(ctx: &mut Ctx, n: i32) -> f32 {
    (ctx.rand() % n) as f32
}

/// `IsValidJointPosition`.
fn valid(p: Vec3) -> bool {
    p != Vec3::ZERO
}

/// `IsValidTailQuad`.
fn valid_tail(t: &[Vec3; 4]) -> bool {
    ((t[0] + t[1]) * 0.5).length_squared() >= 1.0
}

/// `SetNewestTail(o, AngleMatrix(angle), 2)`: la cruz de 4 puntos en `position`.
fn tail_at(position: Vec3, angle: Vec3, scale: f32) -> [Vec3; 4] {
    let h = scale * 0.5;
    [
        position + rotate(angle, Vec3::new(-h, 0.0, 0.0)),
        position + rotate(angle, Vec3::new(h, 0.0, 0.0)),
        position + rotate(angle, Vec3::new(0.0, 0.0, -h)),
        position + rotate(angle, Vec3::new(0.0, 0.0, h)),
    ]
}

/// `CreateTail(o, AngleMatrix(angle))`: desplaza las colas y deja una nueva delante.
fn create_tail(j: &mut Joint, angle: Vec3) {
    let t = tail_at(j.position, angle, j.scale);
    j.tails.insert(0, t);
    j.tails.truncate(j.max_tails.clamp(1, MAX_TAILS));
}

/// `CreateTail(o, Matrix, true)`: dos colas; la primera, media entre la nueva y la anterior.
fn create_tail_blur(j: &mut Joint, angle: Vec3) {
    let t = tail_at(j.position, angle, j.scale);
    let max = j.max_tails.clamp(1, MAX_TAILS);
    j.tails.insert(0, t);
    j.tails.truncate(max);
    if j.tails.len() > 2 {
        for c in 0..4 {
            j.tails[0][c] = (t[c] + j.tails[1][c]) / 2.0;
        }
    }
    j.tails.insert(0, t);
    j.tails.truncate(max);
}

/// Comienzo de `create`: la cola inicial (`bCreateStartTail`) con la posición y el ángulo de
/// antes del `switch` del original.
fn begin(j: &mut Joint, start_tail: bool) {
    if start_tail {
        let t = tail_at(j.position, j.angle, j.scale);
        j.tails.push(t);
    }
}

/// Fin de `create`/`step`: guarda `m_bCreateTails` y desactiva las colas del núcleo.
fn finish(j: &mut Joint) {
    j.user[TAILS_FLAG] = if j.create_tails { 1.0 } else { 0.0 };
    j.create_tails = false;
}

/// Lo común de `MoveJoint` alrededor del código de cada tipo: `FollowTail` con la matriz del
/// ángulo inicial si `m_bCreateTails` y la estela sigue viva.
fn run(j: &mut Joint, ctx: &mut Ctx, f: fn(&mut Joint, &mut Ctx)) {
    j.create_tails = j.user[TAILS_FLAG] != 0.0;
    let matrix_angle = j.angle;
    f(j, ctx);
    if j.live && j.create_tails {
        if j.ty == BITMAP_JOINT_ENERGY && j.sub_type == 54 {
            create_tail_blur(j, matrix_angle);
        } else {
            create_tail(j, matrix_angle);
        }
    }
    finish(j);
}

/// Posición de antes del avance genérico por −Y·Velocity que el núcleo ya aplicó este tick
/// (el original calcula `dx`, `dy` antes de moverla).
fn pre_move_position(j: &Joint) -> Vec3 {
    j.position - rotate(j.angle, Vec3::new(0.0, -j.velocity, 0.0))
}

/// `MoveHumming(Position, Angle, TargetPosition, Turn)`: gira hacia el objetivo y devuelve la
/// distancia.
fn move_humming(position: Vec3, angle: &mut Vec3, target: Vec3, turn: f32) -> f32 {
    let turn = if turn.is_nan() { 0.0 } else { turn };
    let ta = create_angle(position.x, position.y, target.x, target.y);
    angle.z = turn_angle(angle.z, ta, turn);
    let range = position - target;
    let d = (range.x * range.x + range.y * range.y).sqrt();
    let ta = 360.0 - create_angle(position.z, d, target.z, 0.0);
    angle.x = turn_angle(angle.x, ta, turn);
    range.length()
}

/// `GetMagicScrewAt(frame, vResult, fSpeedRate)` (en doble precisión como el original).
fn magic_screw_at(frame: f64, speed_rate: f32) -> Vec3 {
    let sr = speed_rate as f64;
    let speed = [0.048 * sr, 0.0613 * sr, 0.1113 * sr];
    let tilt = ((frame + 55555.0) * speed[0]).sin();
    let temp = [tilt * (frame * speed[1]).cos(), tilt * (frame * speed[1]).sin(), ((frame + 55555.0) * speed[0]).cos()];
    let sin_add = ((frame + 11111.0) * speed[2]).sin();
    let cos_add = ((frame + 11111.0) * speed[2]).cos();
    Vec3::new(
        (cos_add * temp[1] - sin_add * temp[2]) as f32,
        (sin_add * temp[1] + cos_add * temp[2]) as f32,
        temp[0] as f32,
    )
}

/// `GetMagicScrew(iParam, vResult, fSpeedRate)`: con el reloj `WorldTime / 40`.
fn magic_screw(param: i32, speed_rate: f32, world_time: f32) -> Vec3 {
    magic_screw_at(param as f64 + world_time as f64 / 40.0, speed_rate)
}

/// `o->Target->Live`.
fn target_live(ctx: &Ctx, t: Option<Target>) -> bool {
    match t {
        Some(Target::Actor(e)) => ctx.actors.get(&e).is_some_and(|a| a.live),
        Some(Target::Effect(id)) => ctx.effect_positions.contains_key(&id),
        None => false,
    }
}

/// `TransformByObjectBone` / `TransformPosition(o->Target->BoneTransform[bone], offset)`.
fn target_bone(ctx: &Ctx, t: Option<Target>, bone: usize, offset: Vec3) -> Option<Vec3> {
    ctx.actor(t).map(|a| a.bone_world(bone, offset))
}

/// Puntos de los monstruos que el original guarda en el `OBJECT` (`EyeLeft`, `SetTrailLeft`...).
#[derive(Clone, Copy)]
enum Eye {
    Left,
    Right,
    Left2,
    Right2,
    Left3,
    Right3,
    TrailLeft,
    TrailRight,
}

/// `o->Target->EyeLeft`...
fn eye(ctx: &Ctx, t: Option<Target>, which: Eye) -> Option<Vec3> {
    // TODO(original): EyeLeft/EyeRight/EyeLeft2/EyeRight2/EyeLeft3/EyeRight3/SetTrailLeft/Right los
    // rellena el dibujo de cada monstruo concreto; `Actor` aún no los tiene (sin ellos valen 0).
    let _ = (ctx, t, which);
    None
}

/// `UpdateJointTargetPosition(o, eyePos)`: al ojo si existe; si no, a la posición del objetivo.
fn update_joint_target_position(j: &mut Joint, ctx: &Ctx, which: Eye) {
    if let Some(p) = eye(ctx, j.target, which).filter(|p| valid(*p)) {
        j.position = p;
    } else if let Some(p) = ctx.target_position(j.target).filter(|p| valid(*p)) {
        j.position = p;
    }
}

/// `VectorCopy(o->Target->EyeXxx, o->Position)` sin comprobar nada.
fn copy_eye(j: &mut Joint, ctx: &Ctx, which: Eye) {
    if let Some(p) = eye(ctx, j.target, which) {
        j.position = p;
    }
}

/// `CreateBomb(p, true, 4)` de `ZzzEffect.cpp`.
fn create_bomb4(ctx: &mut Ctx, p: Vec3) {
    let position = p + Vec3::new(0.0, 0.0, 80.0);
    let mut angle;
    let mut j = 0;
    loop {
        angle = Vec3::new(rf(ctx, 60) + 60.0 + 90.0, 0.0, rf(ctx, 30));
        if j == 5 {
            break;
        }
        ctx.create_particle(BITMAP_SPARK, position, angle, Vec3::new(1.0, 0.5, 0.3), 8, 1.0, None);
        j += 1;
    }
    ctx.create_particle(BITMAP_EXPLOTION_MONO, position, angle, Vec3::new(1.0, 0.4, 0.2), 0, 0.5, None);
    ctx.create_particle(BITMAP_EXPLOTION_MONO, position, angle, Vec3::new(1.0, 0.6, 0.2), 0, 0.3, None);
}

// --- BITMAP_SCOLPION_TAIL ----------------------------------------------------------------------------

fn create_scolpion_tail(j: &mut Joint, _ctx: &mut Ctx) {
    begin(j, false);
    j.life_time = 120.0;
    j.max_tails = 28;
    j.velocity = 3.0;
    finish(j);
}

fn step_scolpion_tail(j: &mut Joint, ctx: &mut Ctx) {
    run(j, ctx, move_scolpion_tail);
}

fn move_scolpion_tail(j: &mut Joint, ctx: &mut Ctx) {
    if let Some(p) = eye(ctx, j.target, Eye::Left).filter(|p| valid(*p)) {
        j.position = p;
    }
    if !target_live(ctx, j.target) {
        j.live = false;
    }
}

// --- BITMAP_JOINT_ENERGY -----------------------------------------------------------------------------

fn create_energy(j: &mut Joint, ctx: &mut Ctx) {
    begin(j, !matches!(j.sub_type, 10 | 11 | 14 | 15 | 55 | 56 | 57));
    // `vPriorColor`: no se pasa a `create_joint`; se toma como dado si quien la creó cambió la luz.
    let prior_color = j.light != Vec3::ONE;
    let sub = j.sub_type;
    match sub {
        0 | 1 | 6 | 7 | 12 | 13 | 16 | 44 | 45 | 46 => {
            j.life_time = 120.0;
            j.max_tails = 8;
            j.velocity = 3.0;
            if sub == 44 {
                j.life_time = 45.0;
                j.max_tails = 20;
            }
            if matches!(sub, 12 | 13 | 16 | 46) {
                j.multi_use = if ctx.rand() % 2 != 0 { 1.0 } else { -1.0 };
                j.velocity = 10.0;
                j.angle.x = rf(ctx, 45);
                j.angle.y = rf(ctx, 45);
            }
        }
        9 => {
            j.light = Vec3::splat(0.2);
            j.life_time = 120.0;
            j.max_tails = 8;
            j.velocity = 3.0;
        }
        17 => {
            j.velocity = 0.0;
            j.life_time = 12.0;
            copy_eye(j, ctx, Eye::Left);
        }
        47 => {
            j.velocity = 0.0;
            j.life_time = 12.0;
            copy_eye(j, ctx, Eye::Right);
        }
        2..=5 | 8 | 10 | 11 | 14 | 15 | 18..=21 | 26..=33 => {
            j.velocity = 0.0;
            j.life_time = ENDLESS_LIFE_TIME;
            j.max_tails = match sub {
                5 => 10,
                10 | 11 => 5,
                14 | 15 => 15,
                _ => 20,
            };
            let which = match sub {
                3 | 11 | 15 | 19 | 29 => Some(Eye::Right),
                18 | 28 => Some(Eye::Left),
                20 | 30 => Some(Eye::Left2),
                21 | 31 => Some(Eye::Right2),
                26 | 32 => Some(Eye::Left3),
                27 | 33 => Some(Eye::Right3),
                _ => None,
            };
            match which {
                Some(e) => {
                    j.max_tails = 8;
                    update_joint_target_position(j, ctx, e);
                }
                None => update_joint_target_position(j, ctx, Eye::Left),
            }
            j.tex_type = BITMAP_JOINT_ENERGY;
            if (28..=33).contains(&sub) {
                j.tex_type = BITMAP_FLARE;
            }
        }
        22 | 24 => {
            j.velocity = 0.0;
            j.life_time = ENDLESS_LIFE_TIME;
            j.max_tails = 10;
            update_joint_target_position(j, ctx, Eye::Left);
        }
        23 => {
            j.velocity = 0.0;
            j.life_time = ENDLESS_LIFE_TIME;
            j.max_tails = 10;
            update_joint_target_position(j, ctx, Eye::Right);
        }
        25 => {
            j.velocity = 0.0;
            j.life_time = ENDLESS_LIFE_TIME;
            j.max_tails = 10;
            copy_eye(j, ctx, Eye::Right);
        }
        40 | 41 => {
            j.velocity = 0.0;
            j.life_time = ENDLESS_LIFE_TIME;
            // (el original no da MaxTails a estos subtipos: queda el de la ranura anterior)
            j.position = j.target_position;
        }
        42 => {
            j.life_time = 100.0;
            j.max_tails = 6;
            j.velocity = 3.0;
        }
        43 => {
            j.life_time = 100.0;
            j.max_tails = 6;
            j.velocity = 3.0;
        }
        48..=53 => {
            j.life_time = ENDLESS_LIFE_TIME;
            j.max_tails = 3;
            j.light = Vec3::new(0.5, 0.5, 0.9);
        }
        54 => {
            j.velocity = 0.0;
            j.life_time = ENDLESS_LIFE_TIME;
            j.max_tails = 30;
            j.tex_type = BITMAP_SPARK + 1;
            match j.pk_key {
                0 => copy_eye(j, ctx, Eye::Right2),
                1 => copy_eye(j, ctx, Eye::Left2),
                2 => copy_eye(j, ctx, Eye::Right3),
                3 => copy_eye(j, ctx, Eye::Left3),
                _ => {}
            }
        }
        55 | 56 => {
            j.velocity = 0.0;
            j.max_tails = 8;
            j.life_time = 10.0;
            j.tex_type = BITMAP_JOINT_ENERGY;
            let e = if sub == 55 { Eye::TrailLeft } else { Eye::TrailRight };
            update_joint_target_position(j, ctx, e);
        }
        57 => {
            j.velocity = 0.0;
            // MaxTails = iChaIndex: `create_joint` no lo lleva; quien la cree debe poner `max_tails`.
            j.life_time = 10.0;
            j.tex_type = BITMAP_JOINT_ENERGY;
        }
        _ => {}
    }

    if !prior_color {
        let l = match sub {
            0 | 6 => Some(Vec3::new(0.4, 0.3, 0.2)),
            1 => Some(Vec3::new(0.1, 0.1, 0.5)),
            2 | 3 | 5 => Some(Vec3::new(0.5, 0.1, 1.0)),
            4 => Some(Vec3::new(0.3, 0.15, 0.1)),
            8 => Some(Vec3::new(1.0, 0.0, 0.5)),
            10 | 11 => Some(Vec3::new(1.0, 0.3, 0.1)),
            12 | 13 => Some(Vec3::new(0.7, 0.3, 1.0)),
            14 | 15 => Some(Vec3::new(1.0, 0.1, 0.1)),
            16 => Some(Vec3::new(0.4, 0.2, 0.4)),
            17 => Some(Vec3::new(0.8, 0.2, 1.0)),
            18..=21 | 26 | 27 => Some(Vec3::new(0.8, 0.5, 1.0)),
            42 => Some(Vec3::new(0.0, 0.0, 0.0)),
            43 => Some(Vec3::new(2.5, 0.0, 0.0)),
            46 => Some(Vec3::new(0.1, 0.25, 0.1)),
            47 => Some(Vec3::new(0.9, 0.0, 0.0)),
            _ => None,
        };
        if let Some(l) = l {
            j.light = l;
        }
    }
    finish(j);
}

fn step_energy(j: &mut Joint, ctx: &mut Ctx) {
    run(j, ctx, move_energy);
    // TODO(original): el subtipo 54 no se dibuja si el objetivo no está en MONSTER01_ATTACK1.
}

fn move_energy(j: &mut Joint, ctx: &mut Ctx) {
    let sub = j.sub_type;
    match sub {
        2..=5 | 8 | 10 | 11 | 14 | 15 | 17..=33 | 47..=57 => {
            if !target_live(ctx, j.target) {
                j.live = false;
                return;
            }
            let bone = |b: usize, j: &mut Joint, ctx: &Ctx| {
                if let Some(p) = target_bone(ctx, j.target, b, Vec3::ZERO) {
                    j.position = p;
                }
            };
            match sub {
                // Izquierda
                2 | 4 | 5 | 8 | 10 | 14 | 17 | 18 | 28 | 22 | 24 => {
                    update_joint_target_position(j, ctx, Eye::Left);
                    if sub == 8 {
                        j.scale += 10.1;
                    }
                }
                20 | 30 => update_joint_target_position(j, ctx, Eye::Left2),
                26 | 32 => update_joint_target_position(j, ctx, Eye::Left3),
                // Derecha
                3 | 11 | 15 | 19 | 29 | 23 | 25 | 47 => update_joint_target_position(j, ctx, Eye::Right),
                21 | 31 => update_joint_target_position(j, ctx, Eye::Right2),
                54 => match j.pk_key {
                    0 => update_joint_target_position(j, ctx, Eye::Right2),
                    1 => update_joint_target_position(j, ctx, Eye::Left2),
                    2 => update_joint_target_position(j, ctx, Eye::Right3),
                    3 => update_joint_target_position(j, ctx, Eye::Left3),
                    _ => {}
                },
                27 | 33 => update_joint_target_position(j, ctx, Eye::Right3),
                55 => update_joint_target_position(j, ctx, Eye::TrailLeft),
                56 => update_joint_target_position(j, ctx, Eye::TrailRight),
                57 => bone(j.pk_key.max(0) as usize, j, ctx),
                48 => bone(24, j, ctx),
                49 => bone(28, j, ctx),
                50 => bone(32, j, ctx),
                51 => bone(44, j, ctx),
                52 => bone(48, j, ctx),
                53 => bone(52, j, ctx),
                _ => {}
            }
            let wt = ctx.world_time;
            match sub {
                2 | 3 => {
                    let l = (wt * 0.002).sin() * 0.3 + 0.8;
                    let light = Vec3::new(l * 0.5, l * 0.1, l) * j.light;
                    ctx.create_particle(BITMAP_LIGHTNING + 1, j.position, j.angle, light, 0, 1.0, None);
                }
                14 | 15 => {
                    let l = (wt * 0.002).sin() * 0.3 + 0.8;
                    let light = Vec3::new(l, l * 0.1, l * 0.1);
                    ctx.create_particle(BITMAP_LIGHTNING + 1, j.position, j.angle, light, 4, 1.0, None);
                }
                22 | 23 => {
                    let l = (wt * 0.002).sin() * 0.2 + 0.8;
                    let light = Vec3::splat(l) * j.light;
                    ctx.create_particle(BITMAP_LIGHTNING + 1, j.position, j.angle, light, 4, 1.0, None);
                }
                24 | 25 => {
                    let l = (wt * 0.002).sin() * 0.1 + 0.2;
                    let light = Vec3::splat(l) * j.light;
                    ctx.create_sprite(BITMAP_FLARE + 1, j.position, l, light, 0.0, 0);
                }
                54 => {
                    let l = (wt * 0.002).sin() * 0.1;
                    let light = Vec3::splat(l) * j.light;
                    ctx.create_sprite(BITMAP_BLUR, j.position, l, light, 0.0, 0);
                }
                _ => {}
            }
        }
        0 | 1 | 6 | 9 | 12 | 13 | 16 | 44 | 45 | 46 => {
            let life = j.life_time;
            if life >= 100.0 && sub != 44 {
                match sub {
                    0 | 45 => {
                        j.angle.z += 10.0;
                        j.position.z += 6.0;
                    }
                    12 => {
                        j.angle.z += 15.0;
                        j.position.x += j.multi_use * rf(ctx, 5);
                        j.position.y += j.multi_use * rf(ctx, 5);
                        j.position.z += 6.0;
                    }
                    16 | 46 => {
                        j.angle.z -= j.multi_use * 20.0;
                        j.position.x += j.multi_use * (rf(ctx, 5) + 3.0);
                        j.position.y += j.multi_use * (rf(ctx, 5) + 3.0);
                        j.position.z += 8.0;
                    }
                    13 => {
                        j.angle.z -= 20.0;
                        j.position.x += j.multi_use * rf(ctx, 5);
                        j.position.y += j.multi_use * rf(ctx, 5);
                        j.position.z += 6.0;
                    }
                    // (la rama del 44 de aquí es inalcanzable en el original)
                    _ => {
                        j.angle.z -= 10.0;
                        j.position.z += 5.0;
                    }
                }
            } else if sub == 44 && life <= 120.0 && life >= 10.0 {
                if life >= 20.0 {
                    j.velocity = (j.velocity + 10.0).min(20.0);
                    if let Some(tp) = ctx.target_position(j.target) {
                        j.target_position = tp + Vec3::new(0.0, 0.0, 120.0);
                    }
                    move_humming(j.position, &mut j.angle, j.target_position, j.velocity);
                } else {
                    j.velocity = (j.velocity + 12.0).min(20.0);
                    let a = std::f32::consts::PI / (rf(ctx, 180) + 150.0);
                    j.position.x += a.cos();
                    let a = std::f32::consts::PI / (rf(ctx, 180) + 150.0);
                    j.position.y += a.sin();
                    move_humming(j.position, &mut j.angle, j.target_position, j.velocity);
                }
            } else {
                if matches!(sub, 12 | 13 | 16 | 46) {
                    j.velocity = (j.velocity + 7.0).min(40.0);
                } else {
                    j.velocity = (j.velocity + 5.0).min(30.0);
                }
                let old_angle = j.angle.z;
                // (la rama del subtipo 7 es inalcanzable: no está en este `case`)
                let distance = if sub == 6 || sub == 9 {
                    move_humming(j.position, &mut j.angle, j.target_position, j.velocity)
                } else {
                    if let Some(tp) = ctx.target_position(j.target) {
                        j.target_position = tp + Vec3::new(0.0, 0.0, 120.0);
                    }
                    move_humming(j.position, &mut j.angle, j.target_position, j.velocity)
                };
                if distance <= 35.0 {
                    j.live = false;
                    ctx.create_particle(BITMAP_LIGHTNING + 1, j.position, j.angle, j.light, 1, 1.0, None);
                    // TODO(original): PlayBuffer(SOUND_GET_ENERGY).
                } else if distance <= 70.0 && (old_angle - j.angle.z).abs() > 20.0 && j.velocity >= 20.0 {
                    j.velocity -= 10.0;
                }
            }
            let l = (ctx.rand() % 4 + 8) as f32 * 0.03;
            ctx.add_terrain_light(j.position.x, j.position.y, Vec3::new(l * 0.4, l, l * 0.8), 2.0);

            match sub {
                6 | 9 => ctx.create_particle(BITMAP_LIGHTNING + 1, j.position, j.angle, j.light, 3, 0.05, None),
                12 | 13 => {
                    ctx.create_sprite(BITMAP_LIGHT, j.position, 1.0, j.light, 0.0, 0);
                    let rot = rf(ctx, 360);
                    ctx.create_sprite(BITMAP_SHINY + 1, j.position, 1.0, j.light, rot, 0);
                }
                44 => {
                    ctx.create_sprite(BITMAP_LIGHT, j.position, 0.2, j.light, 0.0, 0);
                    let rot = rf(ctx, 360);
                    ctx.create_sprite(BITMAP_SHINY + 1, j.position, 0.1, j.light, rot, 0);
                }
                16 => ctx.create_particle(BITMAP_SMOKE, j.position, j.angle, Vec3::new(0.4, 0.8, 1.0), 31, 0.8, None),
                46 => ctx.create_particle(BITMAP_SMOKE, j.position, j.angle, Vec3::new(0.4, 1.0, 0.4), 31, 0.8, None),
                45 => ctx.create_sprite(BITMAP_FLARE_RED, j.position, 0.3, Vec3::ONE, 0.0, 0),
                _ => ctx.create_particle(BITMAP_LIGHTNING + 1, j.position, j.angle, j.light, 0, 1.0, None),
            }
        }
        42 => {
            if j.life_time >= 60.0 {
                j.position.z += 1.0;
                j.light += Vec3::new(0.4 * 0.005, 0.3 * 0.005, 2.2 * 0.005);
            } else {
                j.light = Vec3::new(0.4, 0.3, 2.2);
                j.velocity = (j.velocity + 5.0).min(30.0);
            }
            let old_angle = j.angle.z;
            if let Some(tp) = ctx.target_position(j.target) {
                j.target_position = tp + Vec3::new(0.0, 0.0, 260.0);
            }
            let distance = move_humming(j.position, &mut j.angle, j.target_position, j.velocity);
            if distance <= 35.0 {
                j.live = false;
                // TODO(original): PlayBuffer(SOUND_GET_ENERGY).
            } else if distance <= 70.0 && (old_angle - j.angle.z).abs() > 20.0 && j.velocity >= 20.0 {
                j.velocity -= 10.0;
            }
            let l = (ctx.rand() % 4 + 8) as f32 * 0.03;
            ctx.add_terrain_light(j.position.x, j.position.y, Vec3::new(l * 0.4, l, l * 0.8), 2.0);
            ctx.create_particle(BITMAP_LIGHTNING + 1, j.position, j.angle, j.light, 5, 0.12, None);
        }
        43 => {
            j.velocity = (j.velocity + 5.0).min(40.0);
            if let Some(tp) = ctx.target_position(j.target) {
                j.target_position = tp + Vec3::new(0.0, 0.0, 100.0);
            }
            j.angle.z = create_angle(j.position.x, j.position.y, j.target_position.x, j.target_position.y);
            let distance = move_humming(j.position, &mut j.angle, j.target_position, j.velocity);
            if distance <= 60.0 {
                j.live = false;
                create_bomb4(ctx, j.position - Vec3::new(0.0, 0.0, 60.0));
            } else if distance >= 550.0 {
                j.live = false;
            }
            let l = (ctx.rand() % 4 + 8) as f32 * 0.03;
            ctx.add_terrain_light(j.position.x, j.position.y, Vec3::new(l, l * 0.4, l * 0.3), 1.0);

            let wt = ctx.world_time;
            let pos = j.position;
            let f = ((wt * 0.002).sin() + 1.0) * 0.1;
            ctx.create_sprite(BITMAP_POUNDING_BALL, pos, 0.7 + f, Vec3::new(2.0 + f, 1.0 + f, 1.0 + f), wt / 10.0, 0);
            let light = Vec3::new(2.0 + rf(ctx, 10) * 0.03, 0.4 + rf(ctx, 10) * 0.03, 0.4 + rf(ctx, 10) * 0.03);
            ctx.create_sprite(BITMAP_LIGHT, pos, 2.0, light, -(wt * 0.1), 0);
            ctx.create_sprite(BITMAP_LIGHT, pos, 2.0, light, wt * 0.12, 0);
            ctx.create_particle(BITMAP_SMOKE, pos, j.angle, Vec3::new(1.0, 0.6, 0.4), 31, 1.0, None);
        }
        _ => {}
    }
}

// --- BITMAP_JOINT_HEALING ----------------------------------------------------------------------------

fn create_healing(j: &mut Joint, ctx: &mut Ctx) {
    begin(j, true);
    j.life_time = 12.0;
    j.max_tails = 2;
    j.velocity = 0.0;
    match j.sub_type {
        4 => {
            j.life_time = 30.0;
            j.target_position = Vec3::new(rf(ctx, 64) - 32.0, -10.0, 0.0);
        }
        5 => {
            j.scale += (ctx.rand() % 10 - 5) as f32;
            j.velocity = rf(ctx, 20) + 6.0;
            j.life_time = rf(ctx, 8) + 8.0;
            j.max_tails = 8;
            j.light = Vec3::ONE;
        }
        6 => {
            j.max_tails = 4;
            j.light = Vec3::new(1.0, 1.0, 0.5);
        }
        7 => {
            j.max_tails = 4;
            j.light = Vec3::new(1.0, 1.0, 0.0);
        }
        8 => {
            j.life_time = 17.0;
            j.max_tails = 3;
            j.light = Vec3::new(0.5, 0.5, rf(ctx, 128) / 255.0 + 0.5);
            j.target_position.z += 100.0;
        }
        9 | 10 => {
            let sub10 = j.sub_type == 10;
            j.life_time = if sub10 { 80.0 } else { 90.0 };
            j.max_tails = 20;
            j.multi_use = (j.angle.z as i32) as f32;
            j.velocity = 0.0;
            j.direction.x = if sub10 { 80.0 } else { 50.0 };
            if sub10 {
                j.collision = j.multi_use == 225.0 || j.multi_use == 405.0;
            }
            j.light = Vec3::new(1.0 / 11.0, 0.5 / 11.0, 1.0 / 11.0);
            if let Some(tp) = ctx.target_position(j.target) {
                j.target_position = tp;
            }
            j.position = j.target_position;
        }
        13 => {
            j.life_time = 17.0;
            j.max_tails = 10;
            j.light = Vec3::new(1.0, 0.3, 0.2);
            j.target_position.z += 200.0;
        }
        14 => {
            j.life_time = 10.0;
            j.max_tails = 10;
            j.light = Vec3::new(0.8, 1.0, 0.8);
        }
        15 => {
            j.life_time = 10.0;
            j.max_tails = 2;
            j.velocity = 0.0;
        }
        16 => {
            j.life_time = 10.0;
            j.max_tails = 2;
            j.velocity = 0.0;
            j.render_type = JointRender::AlphaBlendMinus;
        }
        17 => {
            j.life_time = 17.0;
            j.max_tails = 3;
            j.light = Vec3::new(rf(ctx, 128) / 255.0 + 0.6, 0.1, 0.0);
            j.target_position.z += 100.0;
        }
        _ => {}
    }
    // TODO(original): el subtipo 8 se dibuja sin prueba de profundidad.
    finish(j);
}

fn step_healing(j: &mut Joint, ctx: &mut Ctx) {
    run(j, ctx, move_healing);
    if matches!(j.sub_type, 9 | 10) {
        healing_drawn_sprites(j, ctx);
    }
}

fn move_healing(j: &mut Joint, ctx: &mut Ctx) {
    let sub = j.sub_type;
    let target_pos = ctx.target_position(j.target);
    match sub {
        4 => {
            if let Some(tp) = target_pos {
                j.position = tp + j.target_position;
            }
            j.position.z += j.velocity;
            j.velocity += 10.0;
            let l = (12.0 - j.life_time) * 0.1;
            j.light = Vec3::new(l * 0.4, l * 0.6, l);
        }
        5 => {
            let l = (12.0 - j.life_time) * 0.1;
            j.light = Vec3::new(l * 0.4, l * 0.6, l);
        }
        9 | 10 => {
            // Las colas siguen al objetivo (todas menos la última, como el original).
            let new_target = target_pos.unwrap_or(j.target_position);
            let d = j.target_position - new_target;
            j.target_position = new_target;
            let n = j.tails.len().saturating_sub(1);
            for t in j.tails.iter_mut().take(n) {
                for c in t.iter_mut() {
                    *c -= d;
                }
            }
            j.position.x = j.target_position.x + (j.multi_use * 0.1).sin() * j.direction.x;
            j.position.y = j.target_position.y + (j.multi_use * 0.1).cos() * j.direction.x;
            if sub == 9 {
                j.position.z = j.target_position.z + 10.0 + (90.0 - j.life_time);
                j.multi_use += 2.0;
            } else {
                j.position.z = j.target_position.z + 300.0 * (j.life_time / 80.0);
                j.multi_use += if j.collision { 2.0 } else { -2.0 };
            }
            j.direction.x -= 1.0;
            if j.direction.x < 0.0 {
                j.direction.x = 0.0;
                j.life_time = 0.0;
            }
            let (low, high, up) = if sub == 9 { (50.0, 80.0, 1.25) } else { (40.0, 68.0, 1.2) };
            if j.life_time < low {
                j.light *= 1.0 / 1.1;
            } else if j.life_time > high {
                j.light *= up;
            }
        }
        14 => {
            if j.life_time < 5.0 {
                j.light *= j.life_time / 5.0;
            }
            j.scale *= 1.05;
            if let Some(tp) = target_pos {
                j.position = tp;
            }
        }
        15 | 16 => {
            j.velocity += 4.0;
            let p = target_pos.unwrap_or(j.target_position);
            move_humming(j.position, &mut j.angle, p, 10.0);
            j.light *= 1.0 / 1.08;
            let scale = (ctx.rand() % 4 + 4) as f32 * 0.2;
            let rot = rf(ctx, 360);
            ctx.create_sprite(BITMAP_SHINY + 1, p, scale, j.light, rot, if sub == 16 { 1 } else { 0 });
        }
        _ => {
            j.velocity += if sub == 6 || sub == 7 { 2.0 } else { 4.0 };
            let p;
            if matches!(sub, 6 | 7 | 12) {
                p = j.target_position;
            } else if matches!(sub, 8 | 13 | 17) {
                p = j.target_position;
                if j.life_time > 10.0 {
                    move_humming(j.position, &mut j.angle, p, 10.0);
                } else if j.life_time < 6.0 {
                    j.light *= 1.0 / 1.8;
                }
                ctx.create_sprite(BITMAP_LIGHT, j.position, 1.0, j.light, 0.0, 0);
            } else {
                p = target_pos.unwrap_or(j.target_position) + Vec3::new(0.0, 0.0, 120.0);
                move_humming(j.position, &mut j.angle, p, 10.0);
            }
            let l = (12.0 - j.life_time) * 0.1;
            match sub {
                1 => j.light = Vec3::new(l * 0.4, l * 0.6, l),
                2 => j.light = Vec3::new(l * 0.4, l, l * 0.6),
                3 => j.light = Vec3::new(l, l * 0.6, l * 0.4),
                11 => j.light = Vec3::new(l * 0.9, l * 0.49, l * 0.04),
                12 => j.light = Vec3::new(l * 0.9, l * 0.39, l * 0.03),
                _ => {}
            }
            if sub == 6 {
                if j.life_time <= 10.0 {
                    let a = (6.0 - (j.life_time - 6.0).abs()) * 0.15;
                    let scale = (ctx.rand() % 8 + 8) as f32 * 0.05;
                    let rot = rf(ctx, 360);
                    ctx.create_sprite(BITMAP_SHINY + 1, p, scale, Vec3::splat(a), rot, 0);
                }
            } else if sub != 7 && sub != 8 && sub != 12 {
                if j.life_time as i32 == 1 {
                    let scale = (ctx.rand() % 8 + 8) as f32 * 0.2;
                    let rot = rf(ctx, 360);
                    ctx.create_sprite(BITMAP_SHINY + 1, p, scale, j.light, rot, 0);
                }
                if sub == 11 {
                    let l = (ctx.rand() % 4 + 4) as f32 * 0.01;
                    ctx.add_terrain_light(p.x, p.y, Vec3::splat(l), 1.0);
                }
            }
        }
    }
}

/// `UpdateDrawnJointEffects` para la curación 9/10: un destello por segmento y la luz que se
/// apaga un poco por cada segmento dibujado.
fn healing_drawn_sprites(j: &mut Joint, ctx: &mut Ctx) {
    // Se hace tras mover; la estela que muere este tick (vida < 0 tras restar 1) no se dibuja.
    if !j.live || j.life_time - 1.0 < 0.0 || j.tails.len() < 2 || j.render_face == 0 {
        return;
    }
    let num = j.tails.len() - 1;
    for k in 0..num {
        if k == num - 1 {
            continue;
        }
        let (cur, next) = (j.tails[k], j.tails[k + 1]);
        if !valid_tail(&cur) || !valid_tail(&next) {
            continue;
        }
        let pos = (cur[0] + cur[1] + cur[2] + cur[3]) * 0.25;
        if j.target.is_none() {
            continue;
        }
        let h = k as f32 * 0.01;
        j.light *= 0.9978;
        let light = j.light - Vec3::splat(h);
        ctx.create_sprite(BITMAP_FLARE_BLUE, pos, if j.sub_type == 9 { 0.5 } else { 0.7 }, light, 0.0, 0);
    }
    // TODO(original): DescribeJoint no dibuja el último segmento de estos subtipos.
}

// --- BITMAP_2LINE_GHOST ------------------------------------------------------------------------------

fn create_2line_ghost(j: &mut Joint, ctx: &mut Ctx) {
    begin(j, true);
    if j.sub_type == 0 {
        j.render_type = JointRender::AlphaBlendMinus;
        j.velocity = 40.0 + rf(ctx, 20);
        j.life_time = if ctx.rand() % 2 == 0 { 67.0 } else { 75.0 };
        j.max_tails = 26;
        j.scale += (ctx.rand() % 200 + 1) as f32;
        j.direction.x = 0.0;
        // (m_sTargetIndex = 2: solo lo usa el paso como temporal)
        if ctx.rand() % 3 < 2 {
            j.light = Vec3::splat(0.5);
            // TODO(original): CreateEffect(MODEL_DESAIR, ...) que va montado en esta estela (por su
            // índice en Joints[]); el núcleo no permite que un efecto siga a una estela.
        }
    } else if j.sub_type == 1 {
        j.render_type = JointRender::AlphaBlend;
        j.life_time = 25.0 + rf(ctx, 10);
        j.max_tails = 15;
        j.velocity = 20.0 + rf(ctx, 10);
        j.target_position = j.position;

        let v_angle = Vec3::new(0.0, 0.0, rf(ctx, 360));
        let rand_pos = rotate(v_angle, Vec3::new(0.0, 300.0, 0.0));
        j.position.x += rand_pos.x;
        j.position.y += rand_pos.y;
        j.position.z = rf(ctx, 100) + 200.0;
        let a = (ctx.rand() % 100 - 100) as f32;
        let s = if ctx.rand() % 2 == 0 { 1.0 } else { -1.0 };
        j.target_position.z = j.position.z - a * s;
        j.direction = j.target_position - j.position;
        j.angle.z = create_angle(j.position.x, j.position.y, j.target_position.x, j.target_position.y);
    }
    finish(j);
}

fn step_2line_ghost(j: &mut Joint, ctx: &mut Ctx) {
    run(j, ctx, move_2line_ghost);
}

fn move_2line_ghost(j: &mut Joint, ctx: &mut Ctx) {
    if j.sub_type == 0 {
        let turn = if (j.life_time as i32) % 16 <= 7 { 10.0 } else { -10.0 };
        j.angle.z += turn;
        ctx.create_particle(BITMAP_SMOKE, j.position, j.angle, j.light, 59, 1.0, None);
    } else if j.sub_type == 1 {
        let h = create_angle(j.position.x, j.position.y, j.target_position.x, j.target_position.y);
        j.angle.z = h - 65.0;
        let range = j.position - j.target_position;
        let distance = (range.x * range.x + range.y * range.y).sqrt();
        if distance > 60.0 {
            let v = 360.0 - create_angle(j.position.z, distance, j.target_position.z, 0.0);
            j.angle.x = turn_angle(j.angle.x, v, 3.0);
        } else {
            j.angle.z = h - 90.0;
        }
    }
}

// --- BITMAP_JOINT_SPIRIT / BITMAP_JOINT_SPIRIT2 ------------------------------------------------------

fn create_spirit(j: &mut Joint, ctx: &mut Ctx) {
    begin(j, true);
    let spirit = j.ty == BITMAP_JOINT_SPIRIT;
    // `iIndex` (su ranura en Joints[]) para GetMagicScrew: aquí, una al azar.
    j.user[SLOT_INDEX] = (ctx.rand() % MAX_JOINTS) as f32;
    j.render_type = JointRender::AlphaBlendMinus;
    match j.sub_type {
        0 => {
            // TODO(original): Weapon = CharacterMachine->PacketSerial.
            j.velocity = 70.0;
            j.life_time = 49.0;
            j.max_tails = 6;
        }
        1 => {
            j.velocity = 70.0;
            j.life_time = 49.0;
            j.max_tails = 6;
            // TODO(original): PlayBuffer(SOUND_BRANDISH_SWORD03).
        }
        2 | 21 => {
            j.render_type = JointRender::AlphaBlend;
            j.render_face = RENDER_FACE_TWO;
            j.velocity = 50.0;
            j.life_time = 20.0;
            j.max_tails = 3;
            j.light = Vec3::splat(0.5);
            j.start_position = j.position;
        }
        3 => {
            j.velocity = 140.0;
            j.life_time = 49.0;
            j.max_tails = 10;
            if !spirit {
                j.light = Vec3::ONE;
            } else {
                j.render_type = JointRender::AlphaBlend;
                j.light = Vec3::new(1.0, 0.5, 0.1);
            }
        }
        4 => {
            j.render_type = JointRender::AlphaBlend;
            j.life_time = 0.0;
            j.max_tails = 10;
            j.angle = Vec3::ZERO;
            j.light = Vec3::new(0.3, 0.3, 1.0);
            // Rayo hecho de golpe desde la posición hasta TargetPosition.
            let sp = (j.target_position - j.position) / j.max_tails as f32;
            j.start_position = sp;
            let mut p = sp;
            for _ in 0..j.max_tails - 1 {
                p.x = sp.x;
                p.y = sp.y;
                j.position += p;
                create_tail(j, j.angle);
                j.position.x += -p.x + sp.x;
                j.position.y += -p.y + sp.y;
            }
            j.position = j.target_position;
        }
        5 => {
            j.render_type = JointRender::AlphaBlend;
            // TODO(original): Weapon = CharacterMachine->PacketSerial.
            j.velocity = 30.0;
            j.life_time = 49.0;
            j.max_tails = 12;
        }
        6 => {
            j.render_type = JointRender::AlphaBlend;
            j.render_face = RENDER_FACE_TWO;
            j.velocity = 50.0;
            j.life_time = 20.0;
            j.max_tails = 5;
            j.pk_key = ctx.rand() % 5;
            if j.pk_key != 0 {
                j.create_tails = false;
            }
            match j.skill {
                0 => j.light = Vec3::new(0.3, 0.3, 1.0),
                1 => {
                    j.velocity = 10.0;
                    j.light = Vec3::splat(0.5);
                }
                _ => {}
            }
            j.start_position = j.position;
        }
        7 => {
            j.render_face = 0;
            j.velocity = 10.0;
            j.life_time = 20.0;
            j.max_tails = 5;
            match j.skill {
                0 => j.light = Vec3::new(0.3, 0.3, 1.0),
                1 => {
                    j.velocity = 10.0;
                    j.light = Vec3::splat(0.5);
                }
                _ => {}
            }
            j.start_position = j.position;
        }
        8 => {
            j.life_time = 49.0;
            j.max_tails = 15;
        }
        9 => {
            j.render_face = RENDER_FACE_TWO;
            j.velocity = 10.0;
            j.life_time = 20.0;
            j.max_tails = 15;
            j.light = Vec3::ONE;
            j.angle = Vec3::new(-90.0, 0.0, 0.0);
            j.direction = Vec3::ZERO;
            j.start_position = j.position;
        }
        10 => {
            j.life_time = 20.0;
            j.max_tails = 6;
        }
        11 => {
            j.velocity = 0.0;
            j.life_time = 50.0;
            j.max_tails = 1;
        }
        12 => {
            j.velocity = 0.0;
            j.life_time = 30.0;
            j.max_tails = 1;
        }
        13 => {
            if spirit {
                j.render_type = JointRender::AlphaBlend;
            }
            j.velocity = 40.0;
            j.life_time = 20.0;
            j.max_tails = 10;
            j.light = Vec3::new(1.0, 0.6, 0.4);
        }
        14 => {
            j.velocity = 0.0;
            j.life_time = 10.0;
            j.max_tails = 1;
            j.angle.x = 20.0;
        }
        15 => {
            j.velocity = 0.0;
            j.life_time = 20.0;
            j.max_tails = 1;
        }
        16 => {
            j.velocity = 0.0;
            j.life_time = 50.0;
            j.max_tails = 1;
        }
        17 => {
            j.velocity = 0.0;
            j.life_time = 100.0;
            j.max_tails = 1;
        }
        18 => {
            if !spirit {
                j.render_type = JointRender::AlphaBlend;
            }
            j.velocity = 50.0;
            j.life_time = 39.0;
            j.max_tails = 15;
            j.light = Vec3::splat(0.7);
        }
        19 => {
            if spirit {
                j.render_type = JointRender::AlphaBlend;
            }
            j.velocity = 70.0;
            j.life_time = 49.0;
            j.max_tails = 9;
            j.light = Vec3::new(0.1, 0.5, 0.2);
        }
        20 => {
            if !spirit {
                j.render_type = JointRender::AlphaBlend;
            }
            j.life_time = 49.0;
            j.max_tails = 15;
        }
        22 => {
            j.render_type = JointRender::AlphaBlend;
            j.render_face = RENDER_FACE_TWO;
            j.velocity = 60.0;
            j.life_time = 20.0;
            j.max_tails = 0;
            j.start_position = j.position;
        }
        23 => {
            j.render_face = 0;
            j.velocity = 10.0;
            j.life_time = 20.0;
            j.max_tails = 0;
            j.start_position = j.position;
        }
        24 => {
            j.render_type = JointRender::AlphaBlend;
            j.render_face = RENDER_FACE_TWO;
            j.velocity = 10.0;
            j.life_time = 160.0;
            j.max_tails = 40;
            j.light = Vec3::ZERO;
            j.position.x += (ctx.rand() % 300 - 150) as f32;
            j.position.y += (ctx.rand() % 300 - 150) as f32;
            let rargle_scale = j.scale / 70.0;
            if 0.9 <= rargle_scale {
                // TODO(original): CreateEffect(MODEL_SUMMONER_SUMMON_LAGUL, ..., luz (0,0,0.1), 1,
                // dueño = esta estela, escala rargle_scale); el efecto sigue a la estela y el núcleo
                // no permite que un efecto tenga una estela por dueño.
            }
        }
        25 => {
            j.render_type = JointRender::AlphaBlend;
            j.tex_type = BITMAP_SHINY;
            j.velocity = 9.0;
            j.life_time = 26.0;
            j.max_tails = 30;
            // (Skill = SkillIndex: lo pone quien la crea)
            j.light = Vec3::new(0.9, 0.8, 1.0);
            j.angle = Vec3::new(-90.0, 0.0, 0.0);
            j.direction = Vec3::ZERO;
            j.start_position = j.position;
        }
        _ => {}
    }
    finish(j);
}

fn step_spirit(j: &mut Joint, ctx: &mut Ctx) {
    run(j, ctx, move_spirit);
}

fn move_spirit(j: &mut Joint, ctx: &mut Ctx) {
    let sub = j.sub_type;
    match sub {
        0 | 5 | 19 => {
            let pre = pre_move_position(j);
            let (dx, dy) = (pre.x - j.target_position.x, pre.y - j.target_position.y);
            if j.scale == 80.0 {
                let laser_sub = if sub == 5 { 3 } else { 0 };
                ctx.create_effect(MODEL_LASER, j.position, j.angle, j.light, laser_sub, None, -1, 0, 0, 0.0);
                // TODO(original): en Battle Castle, dentro de zona sin ataque, Velocity = 0 y
                // LifeTime /= 5 (y no hace nada más este tick).
                // TODO(original): cada 15 ticks, si el objetivo es el héroe, SpinMonstersInTornado(150).
            } else if sub == 19 {
                ctx.create_effect(MODEL_SKULL, j.position, j.angle, j.light, 0, None, -1, 0, 0, 0.0);
            }
            if let Some(tp) = ctx.target_position(j.target) {
                j.target_position = tp + Vec3::new(0.0, 0.0, 80.0);
            }
            let distance = (dx * dx + dy * dy).sqrt();
            let turn = if sub == 5 { 2.0 } else { 10.0 };
            move_humming(j.position, &mut j.angle, j.target_position, turn);
            if !j.collision && distance <= j.velocity * 2.0 {
                j.collision = true;
            }
            if sub != 5 {
                j.direction.x += (ctx.rand() % 32 - 16) as f32 * 0.2;
                j.direction.z += (ctx.rand() % 32 - 16) as f32 * 0.8;
                j.angle.x += j.direction.x;
                j.angle.z += j.direction.z;
                j.direction.x *= 0.6;
                j.direction.z *= 0.8;
            }
            let height = ctx.terrain_height(j.position.x, j.position.y);
            if j.position.z < height + 100.0 {
                j.direction.x = 0.0;
                j.angle.x = -5.0;
            }
            if j.position.z > height + 400.0 {
                j.direction.x = 0.0;
                j.angle.x = 5.0;
            }
            if sub != 19 {
                j.light = Vec3::splat(j.life_time * 0.1);
                let l = -((ctx.rand() % 4 + 4) as f32) * 0.01;
                ctx.add_terrain_light(j.position.x, j.position.y, Vec3::splat(l), 4.0);
            }
        }
        1 => {
            let rot = rf(ctx, 360);
            ctx.create_sprite(BITMAP_LIGHT, j.position, 4.0, Vec3::new(0.8, 0.4, 1.0), rot, 0);
        }
        3 | 13 => {
            let mut scale = 3.0;
            // `Light` sin inicializar en el original si no entra en ninguna rama.
            let mut light = j.light;
            if sub != 13 || j.life_time > 28.0 {
                let v = magic_screw(j.user[SLOT_INDEX] as i32, 1.0, ctx.world_time) * 50.0;
                j.position += v;
                light = Vec3::new(1.0, 0.5, 0.1);
            } else if let Some(tp) = ctx.target_position(j.target) {
                if ctx.rand_fps_check(2) {
                    if j.angle.x < -90.0 {
                        j.angle.x += 20.0;
                    } else {
                        j.angle.x -= 20.0;
                    }
                }
                let distance = if sub == 13 {
                    let p = tp + Vec3::new(rf(ctx, 150) - 70.0, rf(ctx, 150) - 70.0, 50.0);
                    move_humming(j.position, &mut j.angle, p, j.velocity - 20.0)
                } else {
                    move_humming(j.position, &mut j.angle, tp, j.velocity - 20.0)
                };
                if j.velocity > 50.0 {
                    j.velocity = 50.0;
                } else {
                    j.velocity += 1.0;
                }
                if distance < 50.0 {
                    j.life_time -= 1.0;
                }
                if j.life_time < 10.0 {
                    j.light *= 1.0 / 1.2;
                    light = j.light;
                }
                if sub == 13 {
                    light = j.light;
                    scale = 2.0 * (j.life_time / 10.0) + 2.0;
                }
            }
            let rot = rf(ctx, 360);
            ctx.create_sprite(BITMAP_LIGHT, j.position, scale, light, rot, 0);
            let rot = rf(ctx, 360);
            ctx.create_sprite(BITMAP_SHINY + 1, j.position, scale / 2.0, light, rot, 0);
        }
        2 | 6 | 7 | 21 | 22 | 23 => {
            // `Light` sin inicializar en el original para un Skill que no sea 0 ni 1.
            let mut light = j.light;
            match sub {
                2 => light = Vec3::new(1.0, 0.5, 0.1),
                21 => light = Vec3::new(0.1, 0.5, 1.0),
                22 | 23 => {
                    if j.skill == 0 || j.skill == 1 {
                        light = Vec3::ONE;
                    }
                }
                6 | 7 => match j.skill {
                    0 => light = Vec3::new(0.3, 0.3, 1.0),
                    1 => light = Vec3::splat(0.5),
                    _ => {}
                },
                _ => {}
            }
            if j.life_time < 10.0 {
                j.light *= 1.0 / 1.2;
                light = j.light;
            } else if j.life_time > 18.0 && j.life_time < 20.0 && (sub == 2 || sub == 21) {
                let angle = Vec3::new(0.0, 0.0, rf(ctx, 360));
                let x = j.start_position.x + (ctx.rand() % 200 - 100) as f32;
                let y = j.start_position.y + (ctx.rand() % 200 - 100) as f32;
                let position = Vec3::new(x, y, j.start_position.z - 200.0);
                if sub == 2 {
                    ctx.create_joint(BITMAP_FLARE, position, position, angle, 2, None, 40.0);
                }
            }
            let rot = rf(ctx, 360);
            if sub == 22 || sub == 23 {
                let s = j.scale + (20.0 - j.life_time) / 5.0;
                ctx.create_sprite(BITMAP_FLAME, j.position, s, light, rot, 0);
            } else {
                let s = 4.0 + (20.0 - j.life_time) / 5.0;
                ctx.create_sprite(BITMAP_LIGHT, j.position, s, light, rot, 0);
            }
            j.velocity += 5.0;
        }
        8 | 20 => {
            if j.life_time < 42.0 {
                j.velocity = 50.0;
                if j.angle.x <= 180.0 {
                    j.angle.x = 180.0;
                }
                if j.angle.x <= 360.0 {
                    j.angle.x += rf(ctx, 3) + 2.0;
                }
            } else {
                j.velocity = 10.0;
                j.angle.x = 110.0;
            }
        }
        10 => {
            j.velocity = 50.0;
            j.angle.z -= 50.0;
        }
        9 => {
            j.position.x += rf(ctx, 10) - 5.0;
            j.position.y += rf(ctx, 10) - 5.0;
            j.position.z += j.velocity;
            j.velocity += 1.0;
            if j.life_time < 10.0 {
                j.light *= 1.0 / 1.15;
            }
        }
        11 => {
            let position = j.position;
            j.angle.x = j.life_time;
            ctx.create_particle(BITMAP_FIRE + 1, position, j.angle, Vec3::ONE, 5, 0.9, None);
            if ctx.rand_fps_check(200) {
                ctx.create_joint(BITMAP_JOINT_SPIRIT, position, position, j.angle, 12, None, 0.9);
            }
            // (el original usa el ángulo en grados como radianes)
            j.position.x += j.angle.z.cos() * 20.0;
            j.position.y += j.angle.z.sin() * 20.0;
        }
        12 => {
            let position = j.position;
            let scale = (ctx.rand() % 32 + 48) as f32 * 0.01;
            ctx.create_particle(BITMAP_SMOKE + 3, position, j.angle, Vec3::new(1.0, 0.2, 0.1), 0, scale, None);
            ctx.create_particle(BITMAP_FIRE + 1, position, j.angle, Vec3::ONE, 6, 0.6, None);
        }
        14 => {
            j.angle.x -= 2.0;
            let r = ctx.rand() % 5;
            if r == 0 {
                ctx.create_particle(BITMAP_WATERFALL_3, j.position, j.angle, j.light, 2, 1.0, None);
            } else if r <= 2 {
                ctx.create_particle(BITMAP_WATERFALL_4, j.position, j.angle, j.light, 2, 1.0, None);
            } else {
                ctx.create_particle(BITMAP_WATERFALL_5, j.position, j.angle, j.light, 4, 1.0, None);
            }
        }
        15 => {
            let mut position = j.position;
            j.pk_key += 30;
            let pk = j.pk_key as f32;
            if j.life_time < 35.0 {
                ctx.create_particle(BITMAP_FIRE, position, j.angle, Vec3::ONE, 7, 1.0, None);
                if ctx.rand_fps_check(2) {
                    position.x -= j.angle.z.sin() * (40.0 - pk * 0.1);
                    position.y += j.angle.z.cos() * (40.0 - pk * 0.1);
                    position.z = 350.0;
                    for i in 0..2 {
                        let angle = Vec3::new(0.0, 0.0, i as f32 * 3.0);
                        ctx.create_joint(BITMAP_JOINT_SPIRIT, position, position, angle, 14, None, 0.9);
                    }
                }
                // TODO(original): si hay objetivo (un OBJECT), le pone Angle[0] = Angle[1] = 0,
                // Alpha = 1 si su LifeTime > 5 y lo coloca delante de la estela
                // (Position + (sin, −cos)(Angle[2])·(30 + PKKey·0.1)); el núcleo no deja escribir en él.
            }
            j.position.x += j.angle.z.sin() * (30.0 + pk * 0.1);
            j.position.y -= j.angle.z.cos() * (30.0 + pk * 0.1);
        }
        16 => {
            j.angle.x = j.life_time;
            ctx.create_particle(BITMAP_WATERFALL_5, j.position, j.angle, j.light, 4, 1.0, None);
            j.position.x += j.angle.z.cos() * 30.0;
            j.position.y += j.angle.z.sin() * 30.0;
        }
        17 => {
            if j.life_time as i32 == 100 {
                let position = j.position;
                for i in 0..120 {
                    let angle = Vec3::new(0.0, 0.0, i as f32 * 3.0);
                    ctx.create_joint(BITMAP_JOINT_SPIRIT, position, j.position, angle, 16, None, 0.9);
                }
            }
        }
        18 => {
            j.velocity += 3.0;
            j.angle.x = -90.0;
            let add = (j.life_time as i32) % 12;
            if add <= 5 {
                j.pk_key += 6;
            } else {
                j.pk_key -= 6;
            }
            j.angle.x -= j.pk_key as f32;
            j.angle.z += 10.0;
            // TODO(original): si hay objetivo (un OBJECT), Alpha = 1 si su LifeTime > 5 y se mueve a
            // la posición de la estela; el núcleo no deja escribir en él.
            ctx.create_particle(BITMAP_SMOKE, j.position, j.angle, Vec3::ONE, 19, 7.0, None);
        }
        24 => {
            if !target_live(ctx, j.target) {
                j.live = false;
                return;
            }
            let v_pos = j.target_position + Vec3::new(0.0, 0.0, 80.0);
            move_humming(j.position, &mut j.angle, v_pos, 5.0);
            j.direction.x += (ctx.rand() % 32 - 16) as f32 * 0.2;
            j.direction.z += (ctx.rand() % 32 - 16) as f32 * 0.8;
            j.angle.x += j.direction.x;
            j.angle.z += j.direction.z;
            j.direction.x *= 0.6;
            j.direction.z *= 0.8;
            let l = if j.life_time < 30.0 {
                j.life_time / 30.0
            } else if j.life_time > 144.0 {
                (160.0 - j.life_time) / 15.0
            } else {
                1.0
            };
            j.light = Vec3::new(0.7 * l, 0.7 * l, 0.9 * l);
            // TODO(original): copia su posición y ángulo al objetivo (un OBJECT); el núcleo no deja
            // escribir en él.
        }
        25 => {
            if j.life_time < 10.0 {
                j.light *= 1.0 / 1.45;
                if j.light.x < 0.2 {
                    j.live = false;
                }
            } else {
                j.position.x += rf(ctx, 16) - 8.0;
                j.position.y += rf(ctx, 16) - 8.0;
                j.position.z += j.velocity;
                j.scale += 3.0;
            }
            ctx.create_sprite(BITMAP_LIGHT, j.position, 0.5, j.light, 0.0, 0);
            ctx.create_sprite(BITMAP_DS_SHOCK, j.position, 0.15, j.light, 0.0, 0);
            if j.life_time as i32 == 10 {
                ctx.create_effect(BITMAP_FIRECRACKER0002, j.position, j.angle, j.light, j.skill, None, -1, 0, 0, 0.0);
            }
        }
        _ => {}
    }
}

// --- BITMAP_JOINT_LASER ------------------------------------------------------------------------------

fn create_laser(j: &mut Joint, _ctx: &mut Ctx) {
    begin(j, true);
    j.tile_mapping = true;
    j.scroll = JointScroll::Tile;
    j.velocity = 70.0;
    j.life_time = 49.0;
    j.max_tails = 6;
    finish(j);
}

fn step_laser(j: &mut Joint, ctx: &mut Ctx) {
    run(j, ctx, move_laser);
}

fn move_laser(j: &mut Joint, ctx: &mut Ctx) {
    let pre = pre_move_position(j);
    let (dx, dy) = (pre.x - j.target_position.x, pre.y - j.target_position.y);
    if let Some(tp) = ctx.target_position(j.target) {
        j.target_position = tp + Vec3::new(0.0, 0.0, 130.0);
    }
    let distance = (dx * dx + dy * dy).sqrt();
    move_humming(j.position, &mut j.angle, j.target_position, 3000.0 / distance);
    if !j.collision && distance <= j.velocity * 2.0 {
        j.collision = true;
        j.life_time = 5.0;
    }
    j.light = Vec3::splat(j.life_time * 0.1);
    let l = -((ctx.rand() % 4 + 4) as f32) * 0.01;
    ctx.add_terrain_light(j.position.x, j.position.y, Vec3::splat(l), 4.0);
}

// --- BITMAP_JOINT_SPARK ------------------------------------------------------------------------------

fn create_spark(j: &mut Joint, ctx: &mut Ctx) {
    begin(j, true);
    match j.sub_type {
        0 | 3 => {
            j.scale = 2.0;
            j.velocity = rf(ctx, 20) + 6.0;
            j.life_time = rf(ctx, 8) + 8.0;
            j.max_tails = 2;
            j.light = Vec3::ONE;
        }
        1 => {
            j.scale = 2.0;
            j.velocity = rf(ctx, 20) + 16.0;
            j.life_time = rf(ctx, 4) + 4.0;
            j.max_tails = 2;
            j.light = Vec3::ONE;
        }
        2 => {
            j.scale = 4.0;
            j.velocity = 30.0;
            j.life_time = rf(ctx, 4) + 4.0;
            j.max_tails = 2;
            j.light = Vec3::new(0.3, 0.3, 1.0);
        }
        4 => {
            j.scale = j.scale * 2.0 + 4.0;
            j.velocity = rf(ctx, 5) + 4.0;
            j.life_time = rf(ctx, 10) + 8.0;
            j.max_tails = 4;
            j.light = Vec3::new(1.0, 0.2, 0.2);
        }
        5 => {
            j.scale = 2.0;
            j.velocity = rf(ctx, 20) + 6.0;
            j.life_time = rf(ctx, 8) + 8.0;
            j.max_tails = 2;
            j.light = Vec3::new(0.7, 0.1, 0.1);
        }
        _ => {}
    }
    finish(j);
}

fn step_spark(j: &mut Joint, ctx: &mut Ctx) {
    run(j, ctx, move_spark);
}

fn move_spark(j: &mut Joint, _ctx: &mut Ctx) {
    let (dv, fade) = match j.sub_type {
        1 => (0.3, 1.0 / 1.4),
        3 | 4 => (0.1, 1.0 / 1.1),
        _ => return,
    };
    j.velocity += dv;
    j.light.x *= fade;
    j.light.y = j.light.x;
    j.light.z = j.light.x;
}

// --- BITMAP_SMOKE --------------------------------------------------------------------------------------

fn create_smoke(j: &mut Joint, _ctx: &mut Ctx) {
    begin(j, true);
    j.light = Vec3::ONE;
    match j.sub_type {
        0 => {
            j.max_tails = 20;
            j.life_time = 20.0;
            j.velocity = 0.0;
            j.tex_type = BITMAP_FLARE;
            // TODO(original): DescribeJoint no dibuja el primer segmento de este subtipo.
        }
        1 => {
            j.max_tails = 15;
            j.life_time = 15.0;
            j.velocity = 0.0;
        }
        2 => {
            j.max_tails = 25;
            j.life_time = 22.0;
            j.velocity = 0.0;
            j.tex_type = BITMAP_FLARE;
            j.light = Vec3::new(0.1, 0.3, 1.0);
        }
        _ => {}
    }
    finish(j);
}

fn step_smoke(j: &mut Joint, ctx: &mut Ctx) {
    run(j, ctx, move_smoke);
}

fn move_smoke(j: &mut Joint, ctx: &mut Ctx) {
    // El original solo sigue a objetivos vivos de tipo MODEL_MAYASTONE1..3 o MODEL_FIRE.
    // TODO(original): el núcleo no dice el tipo del efecto seguido: se sigue a cualquier efecto.
    if let Some(Target::Effect(id)) = j.target {
        if let Some(p) = ctx.effect_positions.get(&id) {
            j.position = *p;
        }
    }
}

// --- MODEL_FENRIR_SKILL_THUNDER ----------------------------------------------------------------------

fn create_fenrir_thunder(j: &mut Joint, _ctx: &mut Ctx) {
    begin(j, true);
    j.max_tails = 50;
    j.velocity = 50.0;
    j.life_time = 20.0;
    j.tile_mapping = true;
    j.scroll = JointScroll::Tile;
    let (tex, light) = match j.sub_type {
        0 => (BITMAP_JOINT_THUNDER, Vec3::new(0.7, 1.0, 0.7)),
        1 => (BITMAP_JOINT_THUNDER, Vec3::new(1.0, 0.6, 0.6)),
        2 => (BITMAP_JOINT_THUNDER, Vec3::new(0.7, 0.7, 1.0)),
        3 => (BITMAP_JOINT_THUNDER, Vec3::new(0.9, 0.9, 0.3)),
        4 => (BITMAP_FLASH, Vec3::new(0.1, 0.8, 0.1)),
        5 => (BITMAP_FLASH, Vec3::new(1.0, 0.3, 0.2)),
        6 => (BITMAP_FLASH, Vec3::new(0.2, 0.3, 1.0)),
        7 => (BITMAP_FLASH, Vec3::new(0.8, 0.8, 0.1)),
        _ => (j.tex_type, j.light),
    };
    j.tex_type = tex;
    j.light = light;
    finish(j);
}

fn step_fenrir_thunder(j: &mut Joint, ctx: &mut Ctx) {
    run(j, ctx, move_fenrir_thunder);
}

fn move_fenrir_thunder(j: &mut Joint, ctx: &mut Ctx) {
    // El rayo se rehace entero cada tick: MaxTails colas de golpe.
    for _ in 0..j.max_tails {
        if let Some(tp) = ctx.target_position(j.target) {
            j.target_position = tp + Vec3::new(0.0, 0.0, 80.0);
        }
        let distance = move_humming(j.position, &mut j.angle, j.target_position, 50.0);
        j.direction.x = (ctx.rand() % 1024 - 512) as f32 / j.scale;
        j.direction.z = (ctx.rand() % 1024 - 512) as f32 / j.scale;
        let angle = j.angle + j.direction;
        create_tail(j, angle);

        if distance < j.velocity * 1.5 {
            if j.scale == 50.0 {
                ctx.create_particle(BITMAP_ENERGY, j.position, j.angle, j.light, 0, 1.0, None);
                if ctx.rand_fps_check(8) {
                    // (posición al azar que el original calcula y no usa)
                    let _ = (ctx.rand(), ctx.rand(), ctx.rand());
                    ctx.create_particle(BITMAP_SMOKE, j.position, j.angle, j.light, 0, 1.0, None);
                }
            }
            break;
        }
        if j.scale >= 50.0 {
            let l = (ctx.rand() % 4 + 4) as f32 * 0.04;
            ctx.add_terrain_light(j.position.x, j.position.y, Vec3::splat(l * 0.4), 2.0);
        }
        j.position += rotate(angle, Vec3::new(0.0, -j.velocity, 0.0));
    }
}

// --- BITMAP_JOINT_LASER + 1 / BITMAP_BLUR + 1 --------------------------------------------------------

fn create_blur1(j: &mut Joint, _ctx: &mut Ctx) {
    begin(j, true);
    if j.ty == BITMAP_JOINT_LASER + 1 {
        j.tile_mapping = true;
        j.scroll = JointScroll::Tile;
    }
    j.scale = 60.0;
    j.velocity = 40.0;
    j.max_tails = 50;
    j.life_time = 2.0;
    if j.sub_type == 0 {
        j.light = Vec3::ONE;
    } else if j.sub_type == 3 {
        j.life_time = 20.0;
        j.velocity = 0.0;
        j.light = Vec3::ONE;
    } else if j.ty == BITMAP_JOINT_LASER + 1 {
        j.light = Vec3::new(0.35, 0.1, 1.0);
    } else {
        j.light = Vec3::new(0.0, 0.3, 1.0);
    }
    finish(j);
}

fn step_blur1(j: &mut Joint, ctx: &mut Ctx) {
    run(j, ctx, move_blur1);
}

fn move_blur1(j: &mut Joint, ctx: &mut Ctx) {
    for _ in 0..j.max_tails {
        if j.sub_type != 2 {
            if let Some(tp) = ctx.target_position(j.target) {
                j.target_position = tp + Vec3::new(0.0, 0.0, 80.0);
            }
        }
        let distance = move_humming(j.position, &mut j.angle, j.target_position, 25.0);
        j.direction.x += (ctx.rand() % 256 - 128) as f32 / j.scale;
        j.direction.z += (ctx.rand() % 256 - 128) as f32 / j.scale;
        j.direction *= 0.8;
        let angle = j.angle + j.direction;
        create_tail(j, angle);

        if distance <= j.velocity * 2.0 {
            if ctx.rand_fps_check(2) {
                // (posición al azar que el original calcula y no usa)
                let _ = (ctx.rand(), ctx.rand(), ctx.rand());
                ctx.create_particle(BITMAP_FIRE, j.position, j.angle, j.light, 0, 1.0, None);
            }
            break;
        }
        let l = (ctx.rand() % 4 + 4) as f32 * 0.05;
        let light = if j.ty == BITMAP_JOINT_LASER + 1 {
            if j.sub_type == 1 { Vec3::new(l, l * 0.1, l * 0.1) } else { Vec3::new(l, l * 0.6, l * 0.3) }
        } else {
            Vec3::new(0.0, l * 0.1, l * 0.2)
        };
        ctx.add_terrain_light(j.position.x, j.position.y, light, 2.0);
        j.position += rotate(angle, Vec3::new(0.0, -j.velocity, 0.0));
    }
}

// --- BITMAP_JOINT_THUNDER ----------------------------------------------------------------------------

/// Los subtipos 4, 10 y 14 del rayo hecho de golpe hasta TargetPosition (`jitter` = 0, 8 o 16).
fn thunder_bolt_tails(j: &mut Joint, ctx: &mut Ctx, jitter: i32) {
    let sp = (j.target_position - j.position) / j.max_tails as f32;
    j.start_position = sp;
    let mut p = sp;
    for _ in 0..j.max_tails.saturating_sub(1) {
        if jitter > 0 {
            let n = if j.target.is_none() { jitter } else { jitter * 2 };
            p.x = sp.x + (ctx.rand() % n - n / 2) as f32;
            p.y = sp.y + (ctx.rand() % n - n / 2) as f32;
        } else {
            p.x = sp.x;
            p.y = sp.y;
        }
        j.position += p;
        create_tail(j, j.angle);
        j.position.x += -p.x + sp.x;
        j.position.y += -p.y + sp.y;
    }
    j.position = j.target_position;
}

fn create_thunder(j: &mut Joint, ctx: &mut Ctx) {
    begin(j, true);
    j.max_tails = 50;
    j.velocity = 50.0;
    match j.sub_type {
        0 | 20 => j.life_time = 2.0,
        1 => {
            j.velocity = 20.0 + rf(ctx, 10);
            j.life_time = rf(ctx, 8) + 8.0;
        }
        2 | 21 => {
            j.life_time = 2.0;
            j.light = if j.sub_type == 2 { Vec3::new(1.0, 0.1, 0.0) } else { Vec3::new(1.0, 0.5, 0.4) };
            if let Some(p) = target_bone(ctx, j.target, 33, Vec3::new(0.0, -150.0, 0.0)) {
                j.target_position = p;
            }
        }
        3 => {
            // Velocity/MaxTails = SkillIndex y LifeTime = PKKey: los pone quien la crea en
            // `skill` y `pk_key`.
            let pk = j.pk_key;
            j.velocity = j.skill as f32;
            j.pk_key = -1;
            j.life_time = pk as f32;
            j.max_tails = j.skill.max(0) as usize;
            j.light = Vec3::new(0.5, 0.5, 1.0);
            j.start_position = j.position;
        }
        4 | 5 => {
            j.velocity = if j.sub_type == 4 { 60.0 } else { -60.0 };
            j.life_time = 20.0;
            j.max_tails = 10;
            j.direction = Vec3::ZERO;
            j.light = Vec3::new(0.5, 0.5, 1.0);
            j.start_position = j.position;
            if j.sub_type == 5 {
                j.target_position.z += 600.0;
            }
        }
        6 => {
            j.life_time = rf(ctx, 20) + 6.0;
            j.create_tails = false;
            j.velocity = 15.0 + rf(ctx, 10);
            j.light = Vec3::splat(rf(ctx, 10) / 15.0 + 0.1);
            j.start_position = j.position;
        }
        7 => {
            j.tile_mapping = true;
            j.velocity = 5.0;
            j.max_tails = 3;
            j.life_time = 1.0;
            j.light = Vec3::new(0.5, 0.5, 1.0);
            j.start_position = j.position;
            move_humming(j.position, &mut j.angle, j.target_position, 360.0);
        }
        8 => {
            j.scale = 5.0;
            j.velocity = rf(ctx, 10) + 6.0;
            j.life_time = rf(ctx, 8) + 8.0;
            j.max_tails = 2;
            j.light = Vec3::ONE;
        }
        9 => {
            j.velocity = 80.0 + rf(ctx, 20);
            j.life_time = 30.0;
            j.light = Vec3::splat(rf(ctx, 10) / 15.0 + 0.1);
            j.start_position = j.position;
        }
        10 => {
            j.tile_mapping = true;
            j.life_time = 0.0;
            j.max_tails = 10;
            j.angle = Vec3::ZERO;
            j.light = Vec3::new(0.3, 0.3, 1.0);
            thunder_bolt_tails(j, ctx, 8);
        }
        11 => {
            j.light = Vec3::new(1.0, 0.5, 0.1);
            j.life_time = 2.0;
        }
        12 => {
            j.light = Vec3::new(1.0, 0.1, 0.1);
            j.life_time = 10.0;
        }
        13 => {
            j.light = Vec3::new(1.0, 0.1, 0.1);
            j.life_time = 2.0;
            j.sub_type = 11;
        }
        14 => {
            j.life_time = 0.0;
            j.max_tails = 10;
            j.angle = Vec3::ZERO;
            j.light = Vec3::new(0.3, 0.3, 1.0);
            thunder_bolt_tails(j, ctx, 0);
        }
        15 => {
            j.render_face = 0;
            j.life_time = 80.0;
            j.max_tails = 0;
            j.multi_use = 0.0;
            // TODO(original): guarda en TargetIndex[] los monstruos vivos y visibles a ≤ 400 (que
            // no sean Target); no hay lista de monstruos, así que no encuentra ninguno.
            j.weapon = j.multi_use * 15.0;
            j.multi_use = 0.0;
            j.start_position = j.target_position + Vec3::new(0.0, 0.0, 150.0);
        }
        16 => {
            j.velocity = 20.0 + rf(ctx, 10);
            j.life_time = rf(ctx, 2) + 2.0;
        }
        17 => {
            j.velocity = 20.0 + rf(ctx, 10);
            j.life_time = 10.0;
        }
        18 => {
            j.velocity = 80.0;
            j.max_tails = 5;
            j.life_time = 10.0;
            j.light = Vec3::ONE;
            j.start_position = j.position;
        }
        19 => {
            j.max_tails = 30;
            j.life_time = 2.0;
            j.velocity = j.scale * 3.0;
            j.start_position = j.position;
        }
        22..=24 => {
            j.life_time = 15.0;
            j.scale += (ctx.rand() % 50 + 50) as f32 * 0.1;
            j.max_tails = 30;
            j.velocity = 20.0;
        }
        25 => {
            j.life_time = 20.0;
            j.max_tails = 8;
            j.velocity = 20.0 + rf(ctx, 10);
        }
        26 => {
            j.velocity = 20.0 + rf(ctx, 10);
            j.life_time = rf(ctx, 8) + 8.0;
        }
        27 | 28 => {
            j.life_time = 2.0;
            // (Light = vPriorColor: la luz que trae)
        }
        33 => {
            j.light = Vec3::new(0.3, 0.3, 1.0);
            j.velocity = 20.0 + rf(ctx, 10);
            j.life_time = rf(ctx, 2) + 2.0;
        }
        _ => {}
    }
    // El original suma el desplazamiento del rayo y, si hay bTileMapping, también el de mosaico;
    // el núcleo solo admite uno: con mosaico se usa el de mosaico.
    j.scroll = if j.tile_mapping { JointScroll::Tile } else { JointScroll::Thunder };
    finish(j);
}

fn step_thunder(j: &mut Joint, ctx: &mut Ctx) {
    run(j, ctx, move_thunder);
}

fn move_thunder(j: &mut Joint, ctx: &mut Ctx) {
    let sub = j.sub_type;
    if sub == 15 {
        // El subtipo 15 no avanza con su velocidad: se deshace lo que movió el núcleo.
        j.position = pre_move_position(j);
    }
    if (sub == 6 && j.life_time > 4.0) || sub == 8 {
        return;
    }
    // `Position` local del original (se reutiliza entre casos e iteraciones).
    let mut position = Vec3::ZERO;
    match sub {
        4 | 5 => {
            j.position = j.target_position;
            if sub == 4 {
                let p = j.target_position + Vec3::new(0.0, 0.0, 30.0);
                let scale = (ctx.rand() % 8 + 8) as f32 * 0.2;
                let rot = rf(ctx, 360);
                ctx.create_sprite(BITMAP_SHINY + 1, p, scale, j.light, rot, 0);
            }
        }
        9 | 6 | 7 | 18 | 19 => {
            if sub == 9 {
                position.x = j.target_position.x + (ctx.rand() % 200 - 100) as f32;
                position.y = j.target_position.y + (ctx.rand() % 200 - 100) as f32;
                position.z = j.target_position.z;
            }
            j.position = j.start_position;
        }
        _ => {}
    }

    // `Distance` sin inicializar en el original para el subtipo 12.
    let mut distance = f32::MAX;
    for k in 0..j.max_tails {
        if sub == 15 {
            break;
        }
        match sub {
            0 | 1 | 2 | 16 | 21 | 27 | 28 | 33 => {
                if let Some(tp) = ctx.target_position(j.target) {
                    j.target_position = tp + Vec3::new(0.0, 0.0, 80.0);
                }
                distance = move_humming(j.position, &mut j.angle, j.target_position, 50.0);
            }
            3 => distance = move_humming(j.position, &mut j.angle, j.target_position, 50.0),
            4 => {
                position = j.target_position - Vec3::new(0.0, 0.0, 300.0);
                distance = move_humming(j.position, &mut j.angle, position, -10.0);
            }
            5 => {
                position = j.target_position + Vec3::new(0.0, 0.0, 600.0);
                distance = move_humming(position, &mut j.angle, j.position, -10.0);
            }
            6 => {
                if let Some(tp) = ctx.target_position(j.target) {
                    j.target_position = tp;
                    j.target_position.x += 2050.0 + rf(ctx, 200);
                    j.target_position.y += 2050.0 + rf(ctx, 200);
                    j.target_position.z -= 10000.0;
                }
                let turn = rf(ctx, 100) + 50.0;
                distance = move_humming(j.position, &mut j.angle, j.target_position, turn);
            }
            7 => {
                let turn = rf(ctx, 100) + 50.0;
                distance = move_humming(j.position, &mut j.angle, j.target_position, turn);
            }
            9 => {
                let turn = rf(ctx, 80) + 60.0;
                distance = move_humming(j.position, &mut j.angle, position, turn);
            }
            12 => {
                // TODO(original): Position = TransformPosition(g_fBoneSave[3], (0, -100, 0)) del
                // modelo del objetivo (huesos guardados por el dibujo de un monstruo concreto).
            }
            18 => distance = move_humming(j.position, &mut j.angle, j.target_position, 110.0),
            19 => {
                let turn = 25.0 + j.scale;
                distance = move_humming(j.position, &mut j.angle, j.target_position, turn);
            }
            20 => {
                j.target_position.z += 100.0;
                distance = move_humming(j.position, &mut j.angle, j.target_position, 100.0);
            }
            // ChainLighting
            22..=24 => {
                if j.target.is_some() {
                    if k == 0 {
                        let v_pos = match sub {
                            22 => target_bone(ctx, j.target, 37, Vec3::ZERO),
                            23 => target_bone(ctx, j.target, 28, Vec3::ZERO),
                            _ => ctx.target_position(j.target).map(|p| p + Vec3::new(0.0, 0.0, 80.0)),
                        };
                        // (sic: rand() % 1024 − 512 / Scale, por la precedencia del original)
                        j.direction.x = (ctx.rand() % 1024) as f32 - 512.0 / j.scale;
                        j.direction.z = (ctx.rand() % 1024) as f32 - 512.0 / j.scale;
                        let angle = j.angle + j.direction;
                        create_tail(j, angle);
                        if let Some(p) = v_pos {
                            j.position = p;
                        }
                        // TODO(original): TargetPosition = posición del personaje m_sTargetIndex + 80;
                        // no hay índice de personajes: se mantiene el TargetPosition con que se creó.
                    }
                    distance = move_humming(j.position, &mut j.angle, j.target_position, j.velocity);
                }
            }
            25 => {
                if let Some(tp) = ctx.target_position(j.target) {
                    j.target_position = tp + Vec3::new(0.0, 0.0, 80.0);
                }
                distance = move_humming(j.position, &mut j.angle, j.target_position, 40.0);
            }
            _ => distance = move_humming(j.position, &mut j.angle, j.target_position, 25.0),
        }

        match sub {
            1 | 28 => {
                j.direction.x = (ctx.rand() % 256 - 128) as f32;
                j.direction.z = (ctx.rand() % 256 - 128) as f32;
            }
            4 => j.direction.x = (ctx.rand() % 64 - 32) as f32,
            5 => j.direction.x = (ctx.rand() % 32 - 16) as f32,
            6 | 7 => {
                j.direction.x = rf(ctx, 100) + 20.0;
                j.direction.z = rf(ctx, 100) + 20.0;
            }
            11 => {
                j.direction.x = (ctx.rand() % 1024 - 512) as f32 * 0.7 / j.scale;
                j.direction.z = (ctx.rand() % 1024 - 512) as f32 * 0.7 / j.scale;
            }
            18 => {
                j.direction.x = (ctx.rand() % 64 - 32) as f32;
                j.direction.z = (ctx.rand() % 64 - 32) as f32;
            }
            19 => {
                j.direction.x = (ctx.rand() % 1024 - 512) as f32 / j.scale;
                j.direction.y = (ctx.rand() % 1024 - 512) as f32 / j.scale;
                j.direction.z = (ctx.rand() % 1024 - 512) as f32 / j.scale;
            }
            _ => {
                j.direction.x = (ctx.rand() % 1024 - 512) as f32 / j.scale;
                j.direction.z = (ctx.rand() % 1024 - 512) as f32 / j.scale;
            }
        }

        let angle = j.angle + j.direction;
        create_tail(j, angle);

        if sub == 3 {
            if distance > 150.0 {
                j.life_time = 0.0;
            }
        } else if distance < j.velocity * 1.5 && !matches!(sub, 6 | 9 | 7 | 18 | 19) {
            if j.scale == 50.0 {
                ctx.create_particle(BITMAP_ENERGY, j.position, j.angle, j.light, 0, 1.0, None);
                if ctx.rand_fps_check(8) {
                    // (posición al azar que el original calcula y no usa)
                    let _ = (ctx.rand(), ctx.rand(), ctx.rand());
                    ctx.create_particle(BITMAP_SMOKE, j.position, j.angle, j.light, 0, 1.0, None);
                }
                if (sub == 0 || sub == 27) && (ctx.world_time as i32) % 1000 < 500 && ctx.rand_fps_check(16) {
                    let p = Vec3::new(rf(ctx, 100) - 50.0, rf(ctx, 100) - 50.0, rf(ctx, 120) - 60.0) + j.target_position;
                    let scale = 6.0 + rf(ctx, 8);
                    // CreateJoint(..., 28, Target, Scale, -1, 0, 0, 0, o->Light): con la luz de esta.
                    let mut nj = Joint::new(BITMAP_JOINT_THUNDER, p, j.target_position, j.angle, 28, j.target, scale);
                    nj.pk_key = -1;
                    nj.light = j.light;
                    ctx.spawns.push(Spawn::Joint(nj));
                }
            }
            break;
        }

        if j.scale >= 50.0 && sub != 4 && sub != 7 && matches!(sub, 0 | 11 | 2 | 18 | 27) {
            let l = (ctx.rand() % 4 + 4) as f32 * 0.04;
            let light = if sub == 27 {
                j.light
            } else if matches!(sub, 0 | 11 | 18) {
                Vec3::new(l * 0.1, l * 0.1, l * 0.5)
            } else {
                Vec3::new(l * 0.4, l * 0.1, l * 0.1)
            };
            ctx.add_terrain_light(j.position.x, j.position.y, light, 2.0);
        }

        position = Vec3::new(0.0, -j.velocity, 0.0);
        j.position += rotate(angle, position);
    }

    if sub == 7 || sub == 18 {
        j.position = j.target_position;
    } else if sub == 15 && (j.life_time as i32) % 2 == 0 {
        ctx.create_joint(BITMAP_JOINT_THUNDER, j.position, j.start_position, j.angle, 0, None, 50.0);
        ctx.create_joint(BITMAP_JOINT_THUNDER, j.position, j.start_position, j.angle, 0, None, 10.0);
        // (`Light` sin inicializar en el original)
        ctx.create_particle(BITMAP_ENERGY, j.position, j.angle, j.light, 0, 1.0, None);
        // TODO(original): rayos desde StartPosition+100 encadenados a cada monstruo de
        // TargetIndex[] (uno cada 15 de MultiUse) si sigue vivo y visible; no hay lista de monstruos.
        if j.multi_use < j.weapon && (j.life_time as i32) % 2 == 0 {
            j.multi_use += 1.0;
        }
    }
}
