//! Estelas traducidas de `ZzzEffectJoint.cpp` (`CreateJoint`, `MoveJoint` y los sprites de
//! `UpdateDrawnJointEffects`): rayos `JOINT_THUNDER + 1`, bola `JOINT_FIRE`, chispas `SPARK + 1`,
//! auras/remolinos `MODEL_SPEARSKILL`, destellos `FLARE` / `FLARE_BLUE` y `FLARE + 1`.
//!
//! Campos libres usados (`Joint::user`):
//! - `user[3]`: `iIndex` del original (posición en `Joints[]`), que algunos tipos usan como semilla
//!   de sus giros (`GetMagicScrew`). Se elige al azar al crear.
//! - `user[2]`: marca interna (≠ 0 = volver a activar `create_tails` al empezar el próximo tick;
//!   ver [`follow_with_angle`]).
//! - `user[0]`, `user[1]`: lo propio de cada subtipo (documentado donde se usa).

use bevy::math::Vec3;

use crate::fx::core::*;

pub fn register(r: &mut Registry) {
    r.joint(BITMAP_JOINT_THUNDER + 1, JointKind { create: Some(thunder_create), step: Some(thunder_step) });
    r.joint(BITMAP_JOINT_FIRE, JointKind { create: Some(fire_create), step: Some(fire_step) });
    r.joint(BITMAP_SPARK + 1, JointKind { create: Some(spark_create), step: Some(spark_step) });
    r.joint(MODEL_SPEARSKILL, JointKind { create: Some(spear_create), step: Some(spear_step) });
    r.joint(BITMAP_FLARE, JointKind { create: Some(flare_create), step: Some(flare_step) });
    r.joint(BITMAP_FLARE_BLUE, JointKind { create: Some(flare_create), step: Some(flare_step) });
    r.joint(BITMAP_FLARE + 1, JointKind { create: Some(flare1_create), step: Some(flare1_step) });
}

// --- Utilidades comunes -----------------------------------------------------------------------------

/// `MAX_JOINTS` del original (para elegir el `iIndex`).
const MAX_JOINTS: i32 = 500;
/// `MAX_SPEAR_FLARE_MOVES` (`ZzzEffect.h`).
const MAX_SPEAR_FLARE_MOVES: i32 = 256;
/// `SPEAR_FLARE_LIFE_STEP`.
const SPEAR_FLARE_LIFE_STEP: i32 = 12;

const SLOT: usize = 3;
const RESTORE_TAILS: usize = 2;

fn v(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
}

/// `iIndex` del original.
fn slot(j: &Joint) -> i32 {
    j.user[SLOT] as i32
}

fn assign_slot(j: &mut Joint, ctx: &mut Ctx) {
    j.user[SLOT] = (ctx.rand() % MAX_JOINTS) as f32;
}

/// `SetNewestTail(o, AngleMatrix(angle), 2)`: la cruz de 4 puntos con un giro cualquiera.
fn tail_with(j: &Joint, angle: Vec3) -> [Vec3; 4] {
    let m = angle_matrix(angle);
    let h = j.scale * 0.5;
    [
        j.position + m * v(-h, 0.0, 0.0),
        j.position + m * v(h, 0.0, 0.0),
        j.position + m * v(0.0, 0.0, -h),
        j.position + m * v(0.0, 0.0, h),
    ]
}

/// `CreateTail(o, AngleMatrix(angle))`: desplaza las colas y deja una nueva delante.
fn create_tail(j: &mut Joint, angle: Vec3) {
    let t = tail_with(j, angle);
    j.tails.insert(0, t);
    let max = j.max_tails.clamp(1, MAX_TAILS);
    j.tails.truncate(max);
}

/// `NumTails` del original (−1 = sin colas).
fn num_tails(j: &Joint) -> i32 {
    j.tails.len() as i32 - 1
}

/// Deshace el avance genérico por −Y·Velocity que ya hizo el núcleo (para los tipos que el
/// original excluye de ese avance al principio de `MoveJoint`).
fn undo_generic_move(j: &mut Joint) {
    if j.velocity != 0.0 {
        j.position -= rotate(j.angle, v(0.0, -j.velocity, 0.0));
    }
}

/// El avance genérico (para repetir un `MoveJoint` entero dentro del mismo tick).
fn generic_move(j: &mut Joint) {
    if j.velocity != 0.0 {
        j.position += rotate(j.angle, v(0.0, -j.velocity, 0.0));
    }
}

/// Al empezar el tick: reactivar `create_tails` si [`follow_with_angle`] lo apagó en el anterior.
fn begin_step(j: &mut Joint) {
    if j.user[RESTORE_TAILS] != 0.0 {
        j.user[RESTORE_TAILS] = 0.0;
        j.create_tails = true;
    }
}

/// El `FollowTail` final de `MoveJoint` usa la matriz calculada al principio (con el ángulo de
/// antes del `switch`). Si el tipo giró la estela durante el tick, la cola se deja aquí con el
/// ángulo viejo y se apaga `create_tails` hasta el próximo tick para que el núcleo no ponga otra.
fn follow_with_angle(j: &mut Joint, a0: Vec3) {
    if j.create_tails && j.live && j.angle != a0 {
        create_tail(j, a0);
        j.create_tails = false;
        j.user[RESTORE_TAILS] = 1.0;
    }
}

/// `MoveHumming(Position, Angle, TargetPosition, Turn)`: gira hacia el objetivo como mucho `turn`
/// grados (en Z y en X) y devuelve la distancia.
fn move_humming(pos: Vec3, angle: &mut Vec3, target: Vec3, turn: f32) -> f32 {
    let ta = create_angle(pos.x, pos.y, target.x, target.y);
    angle.z = turn_angle(angle.z, ta, turn);
    let range = pos - target;
    let distance = (range.x * range.x + range.y * range.y).sqrt();
    let ta = 360.0 - create_angle(pos.z, distance, target.z, 0.0);
    angle.x = turn_angle(angle.x, ta, turn);
    range.length()
}

/// `o->Target->Live` (sin objetivo = muerto).
fn target_live(ctx: &Ctx, t: Option<Target>) -> bool {
    match t {
        Some(Target::Actor(_)) => ctx.actor(t).is_some_and(|a| a.live),
        Some(Target::Effect(_)) => ctx.target_position(t).is_some(),
        None => false,
    }
}

/// `ReferenceFrameClock()`: `WorldTime / 40` continuo.
fn reference_clock(ctx: &Ctx) -> f64 {
    ctx.world_time as f64 / 40.0
}

/// `GetMagicScrewAt(frame, vResult, fSpeedRate)`.
fn magic_screw_at(frame: f64, speed_rate: f32) -> Vec3 {
    let r = speed_rate as f64;
    let speed = [0.048 * r, 0.0613 * r, 0.1113 * r];
    let tilt = ((frame + 55555.0) * speed[0]).sin();
    let temp = [tilt * (frame * speed[1]).cos(), tilt * (frame * speed[1]).sin(), ((frame + 55555.0) * speed[0]).cos()];
    let sin_add = ((frame + 11111.0) * speed[2]).sin();
    let cos_add = ((frame + 11111.0) * speed[2]).cos();
    Vec3::new((cos_add * temp[1] - sin_add * temp[2]) as f32, (sin_add * temp[1] + cos_add * temp[2]) as f32, temp[0] as f32)
}

/// `GetMagicScrew(iParam, vResult, fSpeedRate)`.
fn magic_screw(ctx: &Ctx, param: i32, speed_rate: f32) -> Vec3 {
    magic_screw_at(param as f64 + reference_clock(ctx), speed_rate)
}

/// `Random::Rand() % (half * 2) - half` (si `half` es 0 el original divide por cero; aquí 0).
fn rand_spread(ctx: &mut Ctx, half: i32) -> f32 {
    if half * 2 <= 0 {
        return 0.0;
    }
    (ctx.rand() % (half * 2) - half) as f32
}

/// `Random::Rand() % n - h` como float.
fn rand_off(ctx: &mut Ctx, n: i32, h: i32) -> f32 {
    (ctx.rand() % n - h) as f32
}

/// Colas tal y como quedarán tras el `FollowTail` del núcleo (lo que ve `UpdateDrawnJointEffects`,
/// que se ejecuta después de mover todas las estelas).
fn drawn_tails(j: &Joint) -> Vec<[Vec3; 4]> {
    let mut t = j.tails.clone();
    if j.create_tails && j.live {
        t.insert(0, j.make_tail());
        t.truncate(j.max_tails.clamp(1, MAX_TAILS));
    }
    t
}

/// `UpdateDrawnJointEffects`: el desvanecido de la marca del rayo que cae (`THUNDER + 1` sub 6) y
/// los sprites de `MODEL_SPEARSKILL` 0/4/9 y `FLARE + 1` 6/8.
fn update_drawn(j: &mut Joint, ctx: &mut Ctx) {
    // Tras este tick el núcleo resta 1 a la vida y la mata si queda < 0.
    if !j.live || j.life_time - 1.0 < 0.0 || j.render_face == 0 {
        return;
    }
    let tails = drawn_tails(j);
    let num = tails.len() as i32 - 1;
    if num <= 0 {
        return;
    }
    if j.ty == BITMAP_JOINT_THUNDER + 1 && j.sub_type == 6 {
        j.velocity *= 1.0 / 1.1;
    }
    let spear = j.ty == MODEL_SPEARSKILL && matches!(j.sub_type, 0 | 4 | 9);
    let flare = j.ty == BITMAP_FLARE + 1 && matches!(j.sub_type, 6 | 8);
    let center = |k: usize| (tails[k][0] + tails[k][1] + tails[k][2] + tails[k][3]) * 0.25;
    if spear {
        let k = (num / 2) as usize;
        let pos = center(k);
        let mut light = j.light;
        if let Some(tp) = ctx.target_position(j.target) {
            let h = (tails[k][0].z - (tp.z + if j.sub_type == 9 { 180.0 } else { 50.0 })) * 0.01;
            if h > 0.0 {
                light = j.light - Vec3::splat(h);
            }
        }
        ctx.create_sprite(BITMAP_FLARE_BLUE, pos, 0.7, light, 0.0, 0);
    } else if flare {
        let pos = center(0);
        let big = j.sub_type == 6;
        let r1 = (ctx.rand() % 360) as f32;
        ctx.create_sprite(BITMAP_FLARE_BLUE, pos, if big { 0.5 } else { 0.3 }, j.light, r1, 3);
        let r2 = (ctx.rand() % 360) as f32;
        ctx.create_sprite(BITMAP_SHINY + 1, pos, if big { 1.5 } else { 1.0 }, j.light, r2, 3);
    }
}

/// ¿La acción del personaje es andar/correr o una habilidad de espada? (`CurrentAction` en los rangos
/// que usa el original para cortar los destellos al moverse).
fn is_running_action(ca: i32) -> bool {
    (PLAYER_WALK_MALE..=PLAYER_RUN_RIDE_WEAPON).contains(&ca)
        || (PLAYER_ATTACK_SKILL_SWORD1..=PLAYER_ATTACK_SKILL_SWORD5).contains(&ca)
        || ca == PLAYER_RAGE_UNI_RUN
        || ca == PLAYER_RAGE_UNI_RUN_ONE_RIGHT
}

// --- BITMAP_JOINT_THUNDER + 1 -----------------------------------------------------------------------
//
// Subtipo 0: `Light[2]` del original es un contador (empieza en MaxTails) que decide qué segmentos
// se dibujan: los de índice < contador en gris 0.7, el del contador con su parte fraccionaria y los
// demás en negro (invisibles al sumar). Aquí el contador va en `user[0]`, la luz es 0.7 fija y las
// colas se recortan a `contador` (se pierde solo el desvanecido del segmento fraccionario).
// Subtipo 4: `TargetIndex[0]`/`[1]` van en `user[0]`/`user[1]`.

fn thunder_create(j: &mut Joint, ctx: &mut Ctx) {
    assign_slot(j, ctx);
    j.tails = vec![j.make_tail()];
    let tp_param = j.target_position;
    // o->Scale = Scale (ya lo es).
    j.tex_type = BITMAP_JOINT_THUNDER;
    j.scroll = JointScroll::Thunder;

    match j.sub_type {
        0 => {
            j.max_tails = 50;
            j.velocity = 80.0 + (ctx.rand() % 20) as f32;
            j.life_time = 20.0;
            // Light[2] = MaxTails: contador en user[0]; color de los segmentos 0.7.
            j.user[0] = j.max_tails as f32;
            j.light = Vec3::splat(0.7);
            j.start_position = j.position;
        }
        1 | 2 | 3 | 5 | 6 | 7 => {
            j.max_tails = 50;
            j.life_time = 20.0;
            j.position.x += rand_off(ctx, 10, 5);
            j.position.y += rand_off(ctx, 10, 5);
            if j.sub_type == 7 {
                j.create_tails = false;
                j.position.z += 1050.0;
            } else {
                j.position.z += 800.0;
            }
            j.angle = Vec3::ZERO;
            j.target_position = tp_param;
            if j.sub_type == 1 {
                j.create_tails = false;
            }
            j.start_position = j.position;
            if j.sub_type == 5 {
                j.light = v(1.0, 0.5, 0.2);
            } else if j.sub_type == 6 {
                j.velocity = 0.3;
                j.light = Vec3::ONE;
                j.position.z += 700.0;
            }
        }
        4 => {
            j.max_tails = 50;
            j.life_time = 20.0;
            j.create_tails = false;
            j.only_one_render = 2;
            // TargetIndex[0] / [1].
            j.user[0] = ((j.max_tails as f32) / 1.5) as i32 as f32;
            j.user[1] = (j.max_tails - 1) as f32;
            j.start_position.x = 1.0 / j.max_tails as f32;
            j.start_position.y = 1.0 / (j.max_tails as f32 - j.user[0] - 1.0);
            j.angle = Vec3::ZERO;
            j.light = v(1.0, 0.8, 1.0);
            j.target_position = tp_param;
            j.target_position.z += 100.0;
            // (El original calcula Angle[2] del argumento, que ya no usa.)
        }
        8 => {
            j.max_tails = 50;
            j.life_time = 20.0;
            j.create_tails = false;
            j.position.x += rand_off(ctx, 10, 5);
            j.position.y += rand_off(ctx, 10, 5);
            j.position.z += 1100.0;
            j.angle = Vec3::ZERO;
            j.target_position = tp_param;
            j.start_position = j.position;
        }
        9 => {
            j.max_tails = 50;
            j.life_time = 20.0;
            j.create_tails = false;
            j.position.x -= 50.0;
            j.angle = Vec3::ZERO;
            j.target_position = tp_param;
            j.start_position = j.position;
        }
        10 => {
            j.max_tails = 50;
            j.life_time = 20.0;
            j.create_tails = false;
            j.position.y -= 350.0;
            j.angle = Vec3::ZERO;
            j.target_position = tp_param;
            j.start_position = j.position;
        }
        11 => {
            j.velocity = 80.0 + (ctx.rand() % 20) as f32;
            j.light.z += 0.2;
            j.life_time = 5.0;
            j.max_tails = 30;
            j.start_position = j.position;
            j.target_position = tp_param;
        }
        12 => {
            j.velocity = 80.0 + (ctx.rand() % 20) as f32;
            // Light[2] = MaxTails (el MaxTails que quedaba en la ranura): ≥ 1, y el original lo
            // satura a 1 al dibujar.
            j.light.z = 1.0;
            j.max_tails = 30;
            j.life_time = 4.0;
            j.create_tails = false;
            j.start_position = j.position;
            let p2 = rotate(j.angle, v(0.0, j.scale / 1.0, 0.0)) * (j.scale / 2.0);
            j.position += p2;
            j.start_position = j.position;
            let s = (j.scale / 4.0) as i32;
            j.target_position.x = tp_param.x + rand_spread(ctx, s);
            j.target_position.y = tp_param.y + rand_spread(ctx, s);
            j.target_position.z = tp_param.z + rand_spread(ctx, s);
        }
        _ => {}
    }
}

fn thunder_step(j: &mut Joint, ctx: &mut Ctx) {
    begin_step(j);
    let a0 = j.angle;
    let max = j.max_tails as i32;
    match j.sub_type {
        0 => {
            if j.life_time > 15.0 {
                let target = v(
                    j.target_position.x + rand_off(ctx, 200, 100),
                    j.target_position.y + rand_off(ctx, 100, 50),
                    j.target_position.z,
                );
                j.position = j.start_position;
                for _ in 0..max {
                    let turn = (ctx.rand() % 80) as f32 + 60.0;
                    let mut ang = j.angle;
                    move_humming(j.position, &mut ang, target, turn);
                    j.angle = ang;
                    j.direction.x = (ctx.rand() % 1400 - 700) as f32 / j.scale;
                    j.direction.z = (ctx.rand() % 1400 - 700) as f32 / j.scale;
                    let a = j.angle + j.direction;
                    create_tail(j, a);
                    j.position += rotate(a, v(0.0, -j.velocity, 0.0));
                }
            } else {
                // Light[2] -= 10.12: los segmentos viejos se apagan.
                j.user[0] -= 10.12;
                let visible = j.user[0].trunc().max(0.0) as usize;
                j.tails.truncate(visible);
            }
        }
        1 | 2 | 3 | 5 | 6 | 7 => {
            j.position = j.start_position;
            let a = j.angle;
            for _ in 0..max - 5 {
                if j.sub_type == 6 {
                    j.position.x += rand_off(ctx, 20, 10);
                    j.position.y += rand_off(ctx, 20, 10);
                    j.position.z -= 13.0;
                } else {
                    j.position.x += rand_off(ctx, 20, 10);
                    j.position.y += rand_off(ctx, 20, 10);
                    j.position.z -= if j.sub_type == 7 { 20.0 } else { 16.0 };
                }
                create_tail(j, a);
            }
            let d = (j.target_position - j.position) * 0.2;
            for _ in max - 5..max - 1 {
                j.position += d;
                j.position.x += rand_off(ctx, 20, 10);
                j.position.y += rand_off(ctx, 20, 10);
                create_tail(j, a);
            }
            j.position = j.target_position;
            create_tail(j, a);

            if j.life_time < 4.0 {
                j.light *= 1.0 / 1.2;
            }

            if j.sub_type == 2 {
                let light = v(j.light.x * 0.5, j.light.y * 0.6, j.light.z);
                let r1 = (ctx.rand() % 360) as f32;
                ctx.create_sprite(BITMAP_SHINY + 1, j.target_position, 1.5, light, r1, 0);
                let r2 = (ctx.rand() % 360) as f32;
                ctx.create_sprite(BITMAP_SHINY + 1, j.target_position, 1.5, light, r2, 0);
                ctx.create_particle(BITMAP_TRUE_BLUE, j.target_position, j.angle, light, 0, 1.0, None);
            } else if j.sub_type == 3 {
                let light = v(j.light.x * 0.5, j.light.y * 0.6, j.light.z);
                ctx.create_effect(MODEL_ICE, j.target_position, j.angle, light, 0, None, -1, 0, 0, 0.0);
            } else if j.sub_type == 6 {
                j.target_position.z = ctx.terrain_height(j.target_position.x, j.target_position.y) + 30.0;
                // El original pasa su `Light` local sin inicializar; se usa la luz de la estela.
                ctx.create_particle(BITMAP_TRUE_BLUE, j.target_position, j.angle, j.light, 0, 2.0, None);
                // TODO(original): DescribeJoint dibuja además una marca BITMAP_MAGIC + 1 (2×2,
                // suma, luz = Velocity) en TargetPosition; falta un dibujo propio para estelas.
            }
        }
        4 => {
            if let Some(tp) = ctx.target_position(j.target) {
                j.position = tp;
                let a = j.angle;
                let ti0 = j.user[0] as i32;
                let ti1 = j.user[1] as i32;
                let d = (j.target_position - j.position) * j.start_position.x;
                for _ in 0..ti0 {
                    create_tail(j, a);
                    j.position += d;
                    j.position.x += rand_off(ctx, 20, 10);
                    j.position.y += rand_off(ctx, 20, 10);
                    j.position.z += rand_off(ctx, 20, 10);
                }
                let d = (j.target_position - j.position) * j.start_position.y;
                for _ in ti0..ti1 {
                    j.position += d;
                    create_tail(j, a);
                }
                j.position = j.target_position;
                create_tail(j, a);

                let r1 = (ctx.rand() % 360) as f32;
                ctx.create_sprite(BITMAP_SHINY + 1, j.target_position, 1.5, j.light, r1, 0);
                let r2 = (ctx.rand() % 360) as f32;
                ctx.create_sprite(BITMAP_SHINY + 1, j.target_position, 1.5, j.light, r2, 0);
                if j.life_time < 8.0 {
                    j.light *= 1.0 / 1.4;
                }
            } else {
                j.live = false;
            }
        }
        8 => {
            j.position = j.start_position;
            let a = j.angle;
            for _ in 0..max - 5 {
                j.position.x += rand_off(ctx, 10, 5);
                j.position.y += rand_off(ctx, 10, 5);
                j.position.z -= 20.0;
                create_tail(j, a);
            }
            let d = (j.target_position - j.position) * 0.2;
            for _ in max - 5..max - 1 {
                j.position += d;
                j.position.x += rand_off(ctx, 10, 5);
                j.position.y += rand_off(ctx, 10, 5);
                create_tail(j, a);
            }
            j.position = j.target_position;
            create_tail(j, a);
            if j.life_time < 4.0 {
                j.light *= 1.0 / 1.2;
            }
        }
        9 => {
            j.position = j.start_position;
            let a = j.angle;
            for _ in 0..max - 5 {
                j.position.x += 21.0;
                create_tail(j, a);
            }
            let d = (j.target_position - j.position) * 0.2;
            for _ in max - 5..max - 1 {
                j.position += d;
                j.position.y += rand_off(ctx, 20, 10);
                j.position.z += rand_off(ctx, 20, 10);
                create_tail(j, a);
            }
            j.position = j.target_position;
            create_tail(j, a);
            if j.life_time < 4.0 {
                j.light *= 1.0 / 1.2;
            }
        }
        10 => {
            j.position = j.start_position;
            let a = j.angle;
            for _ in 0..max - 5 {
                j.position.y += 20.0;
                create_tail(j, a);
            }
            let d = (j.target_position - j.position) * 0.2;
            for _ in max - 5..max - 1 {
                j.position += d;
                j.position.x += rand_off(ctx, 20, 10);
                j.position.z += rand_off(ctx, 20, 10);
                create_tail(j, a);
            }
            j.position = j.target_position;
            create_tail(j, a);
            if j.life_time < 4.0 {
                j.light *= 1.0 / 1.2;
            }
        }
        11 => {
            j.position = j.start_position;
            let a = j.angle;
            let p2 = rotate(a, v(0.0, j.scale / 1.3, 0.0));
            for _ in 0..max - 5 {
                j.position += p2;
                create_tail(j, a);
                j.target_position = j.position;
            }
            j.position = j.start_position;
            for _ in 0..max - 5 {
                j.position += p2;
                let s = (j.scale / 8.0) as i32;
                j.position.x += rand_spread(ctx, s);
                j.position.y += rand_spread(ctx, s);
                j.position.z += rand_spread(ctx, s);
                create_tail(j, a);
            }
            j.position = j.target_position;
            create_tail(j, a);
        }
        12 => {
            j.position = j.start_position;
            let a = j.angle;
            for _ in 0..max - 1 {
                let s = (j.scale / 5.0) as i32;
                j.position.x += rand_spread(ctx, s);
                j.position.y += rand_spread(ctx, s);
                j.position.z += rand_spread(ctx, s);
                create_tail(j, a);
                let d = (j.target_position - j.position) * 0.08;
                j.position += d;
            }
            j.position = j.target_position;
            create_tail(j, a);
        }
        _ => {}
    }
    // (La rama final del original para 0/2/3/4/5/6/8/9/10 que conmuta m_bCreateTails es
    // inalcanzable: todos esos subtipos entran antes en otra rama.)
    follow_with_angle(j, a0);
    update_drawn(j, ctx);
}

// --- BITMAP_JOINT_FIRE ------------------------------------------------------------------------------

fn fire_create(j: &mut Joint, ctx: &mut Ctx) {
    assign_slot(j, ctx);
    j.tails = vec![j.make_tail()];
    j.scale = 70.0;
    j.velocity = 50.0;
    j.max_tails = 8;
    j.life_time = 20.0;
    j.target_position += v(0.0, 0.0, 130.0);
    // (PlayBuffer(SOUND_MAGIC_FIRE01) está comentado en el original.)
}

fn fire_step(j: &mut Joint, ctx: &mut Ctx) {
    // MoveHumming con giro 0: no gira, solo mide la distancia.
    let mut ang = j.angle;
    let distance = move_humming(j.position, &mut ang, j.target_position, 0.0);
    j.angle = ang;
    // FollowTail dentro del caso (el núcleo pone otra al final, como el original).
    create_tail(j, j.angle);
    if distance <= j.velocity {
        j.live = false;
        ctx.create_particle(BITMAP_EXPLOTION, j.position, j.angle, j.light, 0, 1.0, None);
        return;
    }
    let lum = (ctx.rand() % 4 + 4) as f32 * 0.1;
    ctx.add_terrain_light(j.position.x, j.position.y, v(lum, lum * 0.6, lum * 0.2), 4.0);
    // Segundo avance por −Y·Velocity (además del genérico).
    j.position += rotate(j.angle, v(0.0, -j.velocity, 0.0));
}

// --- BITMAP_SPARK + 1 -------------------------------------------------------------------------------

fn spark_create(j: &mut Joint, ctx: &mut Ctx) {
    assign_slot(j, ctx);
    j.tails = vec![j.make_tail()];
    let scale = j.scale;
    j.life_time = 100.0;
    j.max_tails = 20;
    j.scale = 10.0;
    if j.sub_type == 0 {
        j.direction.z = (ctx.rand() % 20 + 35) as f32;
        j.scale = (ctx.rand() % 20) as f32 + 20.0;
        j.life_time = 25.0;
    } else if j.sub_type == 1 {
        j.scale = scale;
        j.velocity = (ctx.rand() % 55 + 14) as f32;
        j.life_time = (ctx.rand() % 10 + 8) as f32;
        j.max_tails = 4;
    }
    j.light = Vec3::ONE;
    j.target_position = j.position;
}

fn spark_step(j: &mut Joint, _ctx: &mut Ctx) {
    if j.sub_type == 0 {
        j.direction.z += 5.0;
        j.position.x = j.target_position.x;
        j.position.y = j.target_position.y;
        j.position.z += j.direction.z;
    }
    if j.sub_type == 1 {
        j.velocity += 0.1;
        j.light.x *= 1.0 / 1.1;
        j.light.y = j.light.x;
        j.light.z = j.light.x;
    } else if j.life_time < 5.0 {
        j.light *= 1.0 / 1.3;
    }
}

// --- MODEL_SPEARSKILL -------------------------------------------------------------------------------

fn spear_create(j: &mut Joint, ctx: &mut Ctx) {
    assign_slot(j, ctx);
    let sub = j.sub_type;
    // Subtipos 5-7 no dejan la cola inicial (bCreateStartTail = false).
    if !matches!(sub, 5..=7) {
        j.tails = vec![j.make_tail()];
    }
    let tp_param = j.target_position;
    if let Some(p) = ctx.target_position(j.target) {
        j.target_position = p;
    }
    match sub {
        0 => {
            j.light = Vec3::ONE;
            j.life_time = 999_999.0;
            j.max_tails = 30;
            j.tex_type = BITMAP_FLARE_BLUE;
        }
        4 | 9 => {
            j.light = v(0.4, 0.8, 0.2);
            j.life_time = 10000.0;
            j.max_tails = 30;
            j.tex_type = BITMAP_FLARE_BLUE;
            // TODO(original): guarda m_iChaIndex (el personaje) para cambiar 4↔9 según el Fenrir.
        }
        10 | 11 => {
            j.light = v(1.0, 0.6, 0.6);
            j.life_time = 10000.0;
            j.max_tails = 30;
            j.tex_type = BITMAP_LUCKY_SEAL_EFFECT;
            // TODO(original): m_iChaIndex = FindCharacterIndex(iChaIndex) para cambiar 10↔11 según la montura.
        }
        1 => {
            j.light = Vec3::splat(0.2);
            j.life_time = 999_999.0;
            j.max_tails = 30;
            j.tex_type = BITMAP_JOINT_SPIRIT;
        }
        2 => {
            // Viento de Death Stab: 5 colas, una por tick.
            j.target_position = tp_param;
            j.light = v(1.0, 0.3, 0.3);
            j.life_time = 20.0;
            j.max_tails = 5;
            j.tex_type = BITMAP_FLARE_FORCE;
        }
        3 => {
            if j.target.is_some() {
                // TODO(original): Light = Target->Light (el objetivo es un efecto; falta su luz).
            } else {
                j.light = v(0.5, 0.0, 0.0);
            }
            j.angle = v(-90.0, 0.0, 0.0);
            j.direction = v(0.0, (ctx.rand() % 500) as f32, 0.0);
            j.velocity = (ctx.rand() % 5) as f32 + 5.0;
            j.life_time = 999_999.0;
            j.max_tails = 30;
            j.tex_type = BITMAP_JOINT_SPIRIT;
        }
        5 | 6 | 7 => {
            j.render_face = RENDER_FACE_ONE;
            j.life_time = 60.0;
            j.max_tails = 30;
            j.weapon = 0.0;
            j.create_tails = false;
            j.tex_type = BITMAP_FLARE + 1;
            j.light = match sub {
                5 => v(1.0, 1.0, 0.8),
                6 => v(1.0, 0.8, 1.0),
                _ => v(0.8, 1.0, 1.0),
            };
            j.direction = v(0.0, 800.0, 0.0);
            j.start_position = tp_param;
            j.position = j.start_position + rotate(j.angle, j.direction);
            // NumTails = -1 + CreateTail: una sola cola.
            j.tails.clear();
            let a = j.angle;
            create_tail(j, a);
        }
        8 => {
            j.render_face = RENDER_FACE_ONE;
            j.life_time = 40.0;
            j.max_tails = 30;
            j.create_tails = false;
            j.light = Vec3::splat(0.5);
            j.tex_type = BITMAP_LIGHT;
            j.direction = v(0.0, -40.0, 0.0);
            j.start_position = tp_param;
            j.position = j.start_position + rotate(j.angle, j.direction);
        }
        14 => {
            // Light = vPriorColor (ya viene en la luz de la estela).
            j.life_time = 100.0;
            j.max_tails = 30;
            j.tex_type = BITMAP_LIGHT;
        }
        15 => {
            j.light = Vec3::ONE;
            j.life_time = 100.0;
            j.max_tails = 20;
            j.tex_type = BITMAP_JOINT_SPIRIT;
            // DescribeJoint lo dibuja restando.
            j.render_type = JointRender::AlphaBlendMinus;
            // TODO(original): StartPosition = Target->Owner->Position (falta el dueño del objetivo).
        }
        16 => {
            j.light = v(1.0, 1.0, 0.0);
            j.life_time = 100.0;
            j.max_tails = 30;
            j.tex_type = BITMAP_LIGHT;
        }
        17 => {
            j.light = v(0.7, 0.2, 1.0);
            j.life_time = 100.0;
            j.max_tails = 20;
            j.tex_type = BITMAP_JOINT_SPIRIT;
            // TODO(original): StartPosition = Target->Owner->Position (falta el dueño del objetivo).
        }
        _ => {}
    }
    // o->Scale = Scale (ya lo es).
}

fn spear_step(j: &mut Joint, ctx: &mut Ctx) {
    // TODO(original): con m_iChaIndex cambia 4↔9 (Fenrir fuera de zona segura) y 10↔11 (montura).
    let sub = j.sub_type;
    match sub {
        2 => {
            if !target_live(ctx, j.target) {
                j.target = None;
                update_drawn(j, ctx);
                return;
            }
            j.scale = j.life_time * 3.0;
            let rate1 = ((j.life_time - 10.0) / 10.0).clamp(0.0, 1.0);
            let rate2 = 1.0 - rate1;
            let magic = magic_screw(ctx, slot(j) * 17721, 1.4) + j.target_position;
            // TODO(original): TargetPos = Target->m_vPosSword; se aproxima con el hueso del arma.
            let sword = match ctx.actor(j.target) {
                Some(a) => a.bone_world(a.weapon_bone, Vec3::ZERO),
                None => ctx.target_position(j.target).unwrap_or(j.position),
            };
            j.position = sword * rate2 + magic * rate1;
        }
        3 => {
            if !target_live(ctx, j.target) {
                j.life_time = 0.0;
                j.live = false;
                return;
            }
            if ctx.rand_fps_check(5) {
                j.life_time -= 5.0;
            } else if ctx.rand_fps_check(2) {
                j.life_time += 1.0;
            }
            j.direction.x = ((j.life_time - j.direction.y) * 0.05).sin() * 3.0;
            j.position.x += j.direction.x;
            // TODO(original): Light = Target->Light · fAlpha, con fAlpha según Target->LifeTime
            // (> 50: (100 − vida) / 40; si no, vida / 10; máx. 1). Falta el estado del efecto objetivo.
        }
        5 | 6 | 7 => {
            // Sin avance genérico (velocidad 0 de todos modos).
            undo_generic_move(j);
            for _ in 0..3 {
                j.angle.z += 10.0;
                j.position = j.start_position + rotate(j.angle, j.direction);
                let a = j.angle;
                create_tail(j, a);

                if j.weapon == 0.0 {
                    j.direction.y *= 0.95;
                    if j.direction.y < 10.0 {
                        j.direction.y = -10.0;
                    }
                }
                if j.weapon > 40.0 {
                    j.direction.y -= 20.0;
                    j.angle.z -= 5.0;
                    j.scale += 15.0;
                }
                if j.direction.y < 0.0 {
                    if j.weapon == 40.0 && sub <= 6 {
                        ctx.create_effect(BITMAP_SHOCK_WAVE, j.start_position, j.angle, j.light, 3, None, -1, 0, 0, 0.0);
                    }
                    j.weapon += 1.0;
                    if j.weapon < 20.0 {
                        j.direction.y = -30.0;
                        j.scale = 40.0;
                    } else {
                        j.start_position.z += 5.0;
                    }
                }
            }
            if j.life_time < 10.0 {
                j.light *= 1.0 / 1.2;
            }
            j.scale -= 5.0;
        }
        8 => {
            undo_generic_move(j);
            j.angle.z += 25.0;
            j.position = j.start_position + rotate(j.angle, j.direction);
            let a = j.angle;
            create_tail(j, a);
            j.start_position.z += 15.0;
        }
        _ => {
            if !target_live(ctx, j.target) {
                j.life_time = 0.0;
                j.live = false;
                return;
            }
            if sub == 10 || sub == 11 {
                let mut lumi = (ctx.rand() % 4 + 4) as f32 * 0.1;
                ctx.add_terrain_light(j.position.x, j.position.y, v(lumi * 1.5, lumi * 0.6, lumi * 0.6), 1.0);
                if ctx.rand_fps_check(10) {
                    lumi *= 0.2;
                    let light = v(0.9 + lumi, 0.5 + lumi, 0.5 + lumi);
                    ctx.create_particle(BITMAP_SPARK + 1, j.position, j.angle, light, 19, 1.0, None);
                }
            }
            if sub == 4 || sub == 9 {
                // TODO(original): solo sigue viva mientras HasDefenseAura(Target) (buffs Defense /
                // HelpNpc sin Cloaking); sin sistema de buffs se supone activa y quien la creó
                // debe matarla al acabar el buff.
                j.life_time = 100.0;
            }

            // Las colas se mueven con el objetivo (la última, Tails[NumTails], no: igual que el original).
            let follows = matches!(sub, 0 | 4 | 9 | 10 | 11 | 14 | 16);
            let n = j.tails.len().saturating_sub(1);
            if follows {
                let tp = j.target_position;
                for t in j.tails[..n].iter_mut() {
                    for p in t.iter_mut() {
                        *p -= tp;
                    }
                }
            }

            if sub == 14 {
                let p = match ctx.actor(j.target) {
                    Some(a) => Some(a.bone_world(37, Vec3::ZERO)),
                    None => ctx.target_position(j.target),
                };
                if let Some(p) = p {
                    j.target_position = p;
                }
            } else if sub == 15 || sub == 17 {
                // TODO(original): si Target->Owner es NULL muere; mueve las colas con
                // StartPosition = Target->Owner->Position. Falta el dueño: StartPosition no cambia.
                if let Some(p) = ctx.target_position(j.target) {
                    j.target_position = p;
                }
            } else if let Some(p) = ctx.target_position(j.target) {
                j.target_position = p;
                j.target_position.z += 10.0;
            }

            if follows {
                let tp = j.target_position;
                for t in j.tails[..n].iter_mut() {
                    for p in t.iter_mut() {
                        *p += tp;
                    }
                }
            }

            let clock = reference_clock(ctx);
            let idx = slot(j);
            let frame = (if idx % 2 != 0 { clock } else { -clock }) + idx as f64 * 53731.0;
            let dir = magic_screw_at(frame, if sub == 1 { 0.5 } else { 1.0 });
            let tp = j.target_position;

            match sub {
                0 | 4 => {
                    j.position = v(tp.x + dir.x * 80.0, tp.y + dir.y * 80.0, 110.0 + tp.z + dir.z * 120.0);
                }
                9 => {
                    j.position = v(tp.x + dir.x * 80.0, tp.y + dir.y * 80.0, 140.0 + tp.z + dir.z * 120.0);
                }
                10 => {
                    j.position = v(tp.x + dir.x * 60.0, tp.y + dir.y * 60.0, 50.0 + tp.z + dir.z * 60.0);
                }
                11 => {
                    j.position = v(tp.x + dir.x * 60.0, tp.y + dir.y * 60.0, 100.0 + tp.z + dir.z * 60.0);
                }
                1 => {
                    j.position = v(tp.x + dir.x * 70.0, tp.y + dir.y * 70.0, tp.z + dir.z * 140.0);
                    j.light = v(0.2, 0.2, 0.4 + 0.2 * ((frame + 11111.0) * 0.1113 * 0.5).sin() as f32);
                }
                14 => {
                    if j.target.is_some() {
                        // (Objetivo vivo: comprobado arriba.)
                        j.life_time = 100.0;
                        j.position = tp + dir * 15.0;
                    }
                }
                15 => {
                    if j.target.is_some() {
                        j.life_time = 100.0;
                        // TODO(original): Light = (Target->Alpha, Target->Alpha, Target->Alpha).
                        j.position = tp;
                    }
                }
                16 => {
                    if j.target.is_some() {
                        j.position = v(tp.x + dir.x * 20.0, tp.y + dir.y * 20.0, 100.0 + tp.z + dir.z * 40.0);
                    }
                }
                17 => {
                    if j.target.is_some() {
                        j.life_time = 100.0;
                        j.position = tp;
                    }
                }
                _ => {}
            }
        }
    }
    update_drawn(j, ctx);
}

// --- BITMAP_FLARE / BITMAP_FLARE_BLUE ---------------------------------------------------------------
//
// `PKKey`: el original usa −1 por defecto y el `CreateJoint` de Rust deja 0; 0 (que en el original
// dividiría por cero) se trata como −1.

fn flare_create(j: &mut Joint, ctx: &mut Ctx) {
    assign_slot(j, ctx);
    j.tails = vec![j.make_tail()];
    let tp_param = j.target_position;
    let scale = j.scale;
    let sub = j.sub_type;
    j.life_time = 100.0;
    j.max_tails = 20;
    j.scale = 10.0;

    match sub {
        0 | 18 => {
            if j.ty == BITMAP_FLARE && sub == 18 {
                j.render_type = JointRender::AlphaBlendOther;
            }
            j.scale = scale;
            j.direction.z = (ctx.rand() % 150) as f32 / 100.0;
            j.direction.y = (ctx.rand() % 500 - 250) as f32;
            j.velocity = 40.0;
            if j.scale > 10.0 {
                j.life_time = 50.0;
                j.direction.z = (ctx.rand() % 250 + 200) as f32 / 100.0;
            }
            // TODO(original): Light = Target->Light (falta la luz del personaje).
        }
        20 => {
            if j.target.is_none() {
                j.live = false;
                return;
            }
            j.life_time = 30.0;
            j.max_tails = 10;
            j.scale = scale;
            j.tex_type = BITMAP_FIRECRACKER;
            j.light = v(0.8, 0.3, 1.0);
            if let Some(a) = ctx.actor(j.target) {
                j.position = a.bone_world(33, Vec3::ZERO);
            }
            if j.ty == BITMAP_FLARE_BLUE {
                // DescribeJoint: FLARE_BLUE sub 20 se dibuja con Blend2.
                j.render_type = JointRender::AlphaBlendOther;
            }
        }
        10 => {
            j.scale = scale;
            j.direction.z = (ctx.rand() % 150) as f32 / 100.0;
            j.direction.y = (ctx.rand() % 500 - 250) as f32;
            // (El original comprueba aquí SubType == 25, imposible: siempre 40.)
            j.velocity = 40.0;
            if j.scale > 10.0 {
                j.life_time = 50.0;
                j.direction.z = (ctx.rand() % 250 + 200) as f32 / 100.0;
            }
            // TODO(original): Light = Target->Light.
        }
        14 | 15 => {
            j.render_type = JointRender::AlphaBlendOther;
            j.scale = scale;
            j.tex_type = BITMAP_JOINT_SPIRIT;
            j.direction.z = (ctx.rand() % 150) as f32 / 100.0;
            j.direction.y = (ctx.rand() % 500 - 250) as f32;
            j.velocity = 30.0;
            j.light = v(0.2, 0.4, 0.8);
            if j.scale > 10.0 {
                j.direction.z = (ctx.rand() % 250 + 150) as f32 / 100.0;
            }
            j.life_time = 20.0;
            j.position.z += (ctx.rand() % 25) as f32;
        }
        16 => {
            j.life_time = 60.0;
            j.create_tails = false;
            j.light = Vec3::splat(0.1);
            j.target_position = tp_param;
            j.scale = scale;
            let a = j.angle;
            for _ in 0..j.max_tails {
                j.position.z += 50.0;
                create_tail(j, a);
            }
        }
        17 => {
            j.position.x += rand_off(ctx, 500, 500);
            j.position.y += rand_off(ctx, 500, 500);
            j.direction = Vec3::ZERO;
            j.velocity = (ctx.rand() % 200 + 10) as f32 / 25.0;
            j.scale = scale;
            j.life_time = (20 + ctx.rand() % 10) as f32;
            j.light = v(0.0, 0.0, 1.0);
        }
        2 | 24 | 50 | 51 => {
            if sub == 24 {
                j.tex_type = BITMAP_FLARE_RED;
            } else if sub == 50 || sub == 51 {
                j.tex_type = BITMAP_FLARE_BLUE;
            }
            j.direction.z = (ctx.rand() % 20 + 35) as f32;
            j.scale = scale;
            j.life_time = (25 + ctx.rand() % 50) as f32;
            j.light = Vec3::ONE;
        }
        // (21, 22 y 40 solo existen con GUILD_WAR_EVENT, que no está definido: caen en el `else`.)
        41 => {
            j.scale = scale;
            j.life_time = 40.0;
            j.max_tails = 50;
            let rbias = (ctx.rand() % 300) as f32 / 1000.0;
            let gbias = (ctx.rand() % 300) as f32 / 1000.0;
            j.light = v(1.0 - rbias, 1.0 - gbias, 1.0);
            j.direction.z = (ctx.rand() % 5 + 5) as f32;
        }
        42 => {
            j.direction.y = -15.0;
            j.position += rotate(j.angle, j.direction);
            j.start_position = j.position;
            j.direction.y = -50.0;
            j.create_tails = false;
            j.scale = scale;
            j.light = Vec3::ONE;
        }
        19 => {
            j.direction.z = -((ctx.rand() % 20 + 35) as f32);
            j.scale = scale;
            j.life_time = (25 + ctx.rand() % 50) as f32;
            j.light = Vec3::ONE;
        }
        3 => {
            j.velocity = 50.0;
            j.scale = scale;
            j.life_time = 5.0;
            j.max_tails = 10;
            j.angle = v(0.0, 45.0, -90.0);
            j.position += rotate(j.angle, v(0.0, 100.0, 0.0));
            let s = (ctx.rand() % 8 + 8) as f32 * 0.3;
            let r = (ctx.rand() % 360) as f32;
            ctx.create_sprite(BITMAP_SHINY + 1, j.position, s, j.light, r, 0);
            j.light = Vec3::ONE;
            j.angle = -j.angle;
        }
        4 | 6 | 12 => {
            j.scale = scale;
            let rad = -j.angle.z * std::f32::consts::PI / 180.0;
            if sub == 12 {
                j.life_time = 70.0;
                j.max_tails = 50;
                j.light = v(0.1, 0.1, 1.0);
                j.direction.x = (ctx.rand() % 360) as f32;
                j.direction.y = -4.0 * rad.sin();
                j.direction.z = -4.0 * rad.cos();
            } else {
                if sub == 6 {
                    j.life_time = 20.0;
                    j.max_tails = 30;
                    if j.ty == BITMAP_FLARE_BLUE {
                        j.life_time = 15.0;
                        j.max_tails = 15;
                    }
                    j.light = Vec3::ONE;
                } else {
                    j.life_time = 110.0;
                    j.max_tails = 200;
                    // TODO(original): Light = Target->Light.
                }
                j.direction.x = (ctx.rand() % 360) as f32;
                j.direction.y = -2.0 * rad.sin();
                j.direction.z = -2.0 * rad.cos();
            }
        }
        5 => {
            j.life_time = 2.0;
            j.max_tails = 3;
            j.direction.z = -((ctx.rand() % 3 + 40) as f32);
            j.light = Vec3::ONE;
        }
        7 => {
            j.light = v(0.2, 0.2, 1.0);
            j.multi_use = (ctx.rand() % 10) as f32;
            j.life_time = 30.0 + j.multi_use;
            j.max_tails = 15;
            j.direction.x = (ctx.rand() % 3000) as f32;
            j.scale = 30.0;
        }
        8 => {
            j.angle = Vec3::ZERO;
            j.direction = Vec3::ZERO;
            j.start_position = j.position;
            // AngleMatrix(o->TargetPosition): la posición objetivo hace de ángulo.
            j.position = j.start_position + rotate(j.target_position, v(0.0, -50.0, 0.0));
        }
        9 => {
            j.life_time = 0.0;
            j.scale = scale;
            j.max_tails = 10;
            j.angle = Vec3::ZERO;
            j.light = v(0.3, 0.3, 1.0);
            j.start_position = (tp_param - j.position) / j.max_tails as f32;
            let a = j.angle;
            for _ in 0..j.max_tails {
                j.position += j.start_position;
                create_tail(j, a);
            }
        }
        11 => {
            j.light = v(0.2, 0.2, 1.0);
            j.multi_use = 0.0;
            j.life_time = 30.0;
            j.max_tails = 15;
            j.direction.x = (ctx.rand() % 3000) as f32;
            j.scale = 30.0;
        }
        25 => {
            j.light = v(0.9, 0.4, 0.6);
            j.multi_use = 0.0;
            j.life_time = 30.0;
            j.max_tails = 15;
            j.direction.x = (ctx.rand() % 3000) as f32;
            j.scale = 30.0;
        }
        13 => {
            j.direction.z = (ctx.rand() % 20 + 35) as f32;
            j.scale = scale;
            j.life_time = (25 + ctx.rand() % 50) as f32;
            j.light = Vec3::splat(0.5);
        }
        23 => {
            let pk = j.pk_key;
            j.life_time = (20 + (4 - pk)) as f32;
            j.scale = scale;
            j.max_tails = 15;
            // NumTails = -1: sin colas.
            j.tails.clear();
            j.velocity = 0.0;
            j.direction = v(1.0, 5.0, 1.0);
            j.create_tails = false;
            j.head_angle = j.angle;
            j.multi_use = pk as f32;
            match pk {
                0 => {
                    j.head_angle.z += 90.0;
                    j.position.z += 200.0;
                }
                1 => {
                    j.head_angle.z += 90.0;
                    j.position.z += 10.0;
                }
                2 => {
                    j.head_angle.z -= 90.0;
                    j.position.z += 200.0;
                }
                3 => {
                    j.head_angle.z -= 90.0;
                    j.position.z += 10.0;
                }
                4 => {
                    j.head_angle.z += 180.0;
                    j.position.z += 100.0;
                }
                5 => {
                    j.head_angle.x = 90.0;
                    j.position.z += 100.0;
                    j.life_time = 10.0;
                    j.max_tails = 20;
                    j.render_face = RENDER_FACE_TWO;
                }
                _ => {}
            }
            j.start_position = j.position;
        }
        43 => {
            j.life_time = 100.0;
            j.max_tails = 0;
            j.create_tails = false;
        }
        44 => {
            j.scale = scale;
            j.life_time = 15.0;
            j.max_tails = 30;
            j.create_tails = true;
            j.direction.z = (ctx.rand() % 2 + 2) as f32;
        }
        45 | 46 => {
            j.light = v(0.2, 0.2, 1.0);
            j.multi_use = (ctx.rand() % 10) as f32;
            j.life_time = 30.0 + j.multi_use;
            j.max_tails = 15;
            j.direction.x = (ctx.rand() % 3000) as f32;
            j.scale = 30.0;
        }
        47 => {
            j.light = v(0.7, 0.7, 1.0);
            j.multi_use = (ctx.rand() % 10) as f32;
            j.life_time = 30.0 + j.multi_use;
            j.max_tails = 15;
            j.direction.x = (ctx.rand() % 3000) as f32;
            j.scale = 30.0;
        }
        _ => {
            j.create_tails = false;
        }
    }
    j.target_position = tp_param;
}

/// Destellos de Impale (4) y Death Stab (12): se mueven varias veces por tick hasta que su vida
/// es múltiplo de [`SPEAR_FLARE_LIFE_STEP`] (`MoveJointForFrame`).
fn is_spear_flare(j: &Joint) -> bool {
    (j.ty == BITMAP_FLARE || j.ty == BITMAP_FLARE_BLUE) && (j.sub_type == 4 || j.sub_type == 12)
}

fn flare_step(j: &mut Joint, ctx: &mut Ctx) {
    flare_move(j, ctx);
    let mut moves = 1;
    while moves < MAX_SPEAR_FLARE_MOVES && is_spear_flare(j) {
        // Vida tras el `LifeTime -= 1` que hará el núcleo al acabar este movimiento.
        let after = j.life_time - 1.0;
        if !(j.live && after >= 0.0 && (after as i32) % SPEAR_FLARE_LIFE_STEP != 0) {
            break;
        }
        // Terminar este movimiento aquí (lo que el núcleo hace tras `step`)...
        if j.create_tails {
            j.follow_tail();
        }
        j.life_time = after;
        // ...y hacer otro entero; el núcleo cerrará el último.
        generic_move(j);
        flare_move(j, ctx);
        moves += 1;
    }
}

/// Un `MoveJoint` de `FLARE` / `FLARE_BLUE` (sin el avance genérico ni el `FollowTail` final).
fn flare_move(j: &mut Joint, ctx: &mut Ctx) {
    let sub = j.sub_type;
    // El original no aplica el avance genérico a los subtipos 1 (ambos) ni al 42 (FLARE).
    if sub == 1 || (j.ty == BITMAP_FLARE && sub == 42) {
        undo_generic_move(j);
    }

    if sub == 0 || sub == 10 || sub == 18 {
        let count = if j.pk_key != -1 && j.pk_key != 0 {
            (j.direction.y + j.life_time) / j.pk_key as f32
        } else if sub == 18 {
            let c = (j.direction.y + j.life_time * 2.0) * 0.1;
            if j.skill == 1 { -c } else { c }
        } else {
            (j.direction.y + j.life_time) * 0.1
        };
        j.position.x = j.target_position.x + count.cos() * j.velocity;
        j.position.y = j.target_position.y - count.sin() * j.velocity;
        if sub == 0 {
            j.position.z += j.direction.z;
        } else if sub == 10 {
            j.position.z -= j.direction.z;
        } else {
            j.position.z += j.direction.z * 1.1;
            j.velocity += 0.1;
            j.scale += 0.2;
        }
    } else if sub == 20 {
        if j.target.is_none() {
            j.live = false;
            return;
        }
        if let Some(a) = ctx.actor(j.target) {
            j.position = a.bone_world(33, Vec3::ZERO);
        }
    } else if sub == 14 || sub == 15 {
        if j.target.is_some() {
            let mut count = j.direction.y * 0.1;
            if sub == 15 {
                count = -count;
            }
            j.position.x = j.target_position.x + count.cos() * j.velocity;
            j.position.y = j.target_position.y - count.sin() * j.velocity;
            j.position.z += j.direction.z;
            if j.life_time < 20.0 {
                j.velocity += 5.0;
            }
            j.scale += 1.0;
            j.direction.y += 4.0;
            j.direction.z += (ctx.rand() % 200 + 200) as f32 / 100.0;
        }
    } else if sub == 2 || sub == 24 || sub == 50 || sub == 51 {
        if sub == 51 {
            j.direction.z += 10.0;
            j.position.x = j.target_position.x;
            j.position.y = j.target_position.y;
            j.position.z += j.direction.z;
        }
        if j.life_time <= 25.0 || sub == 50 {
            j.direction.z += 5.0;
            j.position.x = j.target_position.x;
            j.position.y = j.target_position.y;
            j.position.z += j.direction.z;
        }
    } else if sub == 41 {
        if j.life_time <= 20.0 {
            j.direction.z += 1.0;
            j.position.x = j.target_position.x;
            j.position.y = j.target_position.y;
            j.position.z += j.direction.z;
            j.light *= 1.0 / 1.1;
        }
        ctx.add_terrain_light(j.position.x, j.position.y, j.light, 1.0);
    } else if sub == 42 {
        j.angle.z += 15.0;
        if target_live(ctx, j.target) {
            j.life_time = 100.0;
        } else {
            // DeleteJoint(BITMAP_FLARE, Target, 42): cada una se mata al ver muerto su objetivo.
            j.live = false;
            return;
        }
        j.position = j.start_position + rotate(j.angle, j.direction);
        let bias = ctx.rand() % 100;
        if (j.angle.z as i32) % 45 == 0 && bias <= 50 {
            ctx.create_joint(BITMAP_FLARE, j.position, j.position, Vec3::ZERO, 41, None, j.scale);
        }
        ctx.add_terrain_light(j.target_position.x, j.target_position.y, j.light, 2.0);
    } else if sub == 19 {
        if j.life_time <= 25.0 {
            j.direction.z -= 5.0;
            j.position.x = j.target_position.x;
            j.position.y = j.target_position.y;
            j.position.z += j.direction.z;
        }
    } else if sub == 4 || sub == 6 || sub == 12 {
        let (vx, vy);
        if sub == 6 {
            let count = if j.ty == BITMAP_FLARE_BLUE {
                j.direction.x + j.life_time * 0.8
            } else {
                j.direction.x + j.life_time * 0.5
            };
            let r = (j.life_time as i32).max(1) as f32 * 2.0;
            vx = -count.cos() * r;
            vy = count.sin() * r;
        } else if sub == 12 {
            let count = (j.direction.x + j.life_time) * 0.1;
            vx = -count.cos() * (40.0 * 0.65);
            vy = count.sin() * (40.0 * 0.65) - (90.0 - j.life_time) * 0.3;
        } else {
            let count = (j.direction.x + j.life_time) * 0.1;
            let r = ((j.life_time + 40.0) as i32).max(10) as f32 * 0.65;
            vx = -count.cos() * r;
            vy = count.sin() * r;
        }
        j.target_position.x += j.direction.y;
        j.target_position.y += j.direction.z;
        let rad = (90.0 - j.angle.z) * std::f32::consts::PI / 180.0;
        j.position.x = vx * rad.sin() + j.target_position.x;
        j.position.y = vx * rad.cos() + j.target_position.y;
        j.position.z = vy + j.target_position.z;
    } else if sub == 5 {
        j.direction.z -= 60.0;
        j.position.x = j.target_position.x;
        j.position.y = j.target_position.y;
        j.position.z += j.direction.z;
        j.scale = (ctx.rand() % 4) as f32 + 6.0;
    } else if matches!(sub, 7 | 11 | 25 | 45 | 46 | 47) {
        let homing = sub == 11 || sub == 25;
        if homing && !target_live(ctx, j.target) {
            j.live = false;
            j.life_time = -1.0;
            return;
        }
        let clock = reference_clock(ctx);
        let idx = slot(j);
        let frame = (if idx % 2 != 0 { clock } else { -clock }) + idx as f64 * 53731.0;
        let dir = magic_screw_at(frame, if sub == 11 { 1.5 } else { 1.0 });

        let f_life = j.life_time * 40.0 / 30.0;
        let mut f_pos = if homing {
            if f_life < 20.0 { f_life * 4.0 } else { f_life + 60.0 }
        } else if f_life < 10.0 {
            f_life * 7.0
        } else {
            f_life + 60.0
        };
        f_pos = f_pos / (30.0 + j.multi_use) * 30.0;
        if j.life_time as i32 == 30 {
            ctx.play_sound("SOUND_METEORITE01");
        }
        let f_circle = if homing {
            ((f_life - 10.0).max(0.0) * 5.0).min(150.0)
        } else {
            ((40.0 - f_life).max(0.0) * 15.0).min(150.0)
        };
        j.position = j.target_position + dir * f_circle;

        // 11/25 usan Target->StartPosition. TODO(original): falta el estado del objetivo; se usa
        // su posición. Sin objetivo (el original fallaría) se usa TargetPosition.
        let tpos = ctx.target_position(j.target).unwrap_or(j.target_position);
        for k in 0..3 {
            let wobble = 25.0 * ((idx as f64 * 51231.0 + k as f64 * 3711.0 + frame / 10.0) * 0.01).cos() as f32;
            let last = (100.0 - f_pos) * (tpos[k] + wobble);
            j.position[k] = (f_pos * j.position[k] + last) * 0.01;
        }
        if !homing {
            j.position.z += 100.0;
        }

        let mut light = v(0.5, 0.5, 1.0);
        if sub == 11 {
            light = v(0.3, 0.3, 0.5);
            if j.skill == 1 {
                // TODO(original): mueve el objetivo a esta posición (Target->Position) y CheckClientArrow.
            }
        }
        if sub == 25 {
            light = v(0.9, 0.4, 0.6);
            if j.skill == 1 {
                // TODO(original): mueve el objetivo a esta posición (Target->Position) y CheckClientArrow.
            }
        }
        if sub == 45 && j.life_time <= 1.0 {
            if let Some(a) = ctx.actor(j.target) {
                let (p, ang) = (a.position, a.angle);
                // TODO(original): con Target->Light (falta la luz del personaje).
                ctx.create_effect(MODEL_CHANGE_UP_EFF, p, ang, Vec3::ONE, 1, j.target, -1, 0, 0, 0.0);
                ctx.create_effect(MODEL_CHANGE_UP_CYLINDER, p, ang, Vec3::ONE, 1, j.target, -1, 0, 0, 0.0);
            }
        }
        if sub != 47 {
            let s = (ctx.rand() % 2 + 8) as f32 * 0.10;
            let r = (ctx.rand() % 360) as f32;
            ctx.create_sprite(BITMAP_SHINY + 1, j.position, s, light, r, 0);
            let s = (ctx.rand() % 2 + 8) as f32 * 0.18;
            let r = (ctx.rand() % 360) as f32;
            ctx.create_sprite(BITMAP_LIGHT, j.position, s, light, r, 0);
            let s = (ctx.rand() % 2 + 8) as f32 * 0.18;
            let r = (ctx.rand() % 360) as f32;
            ctx.create_sprite(BITMAP_LIGHT, j.position, s, light, r, 0);
        }
        if sub != 11 && j.life_time as i32 == 1 && sub != 45 && sub != 46 && sub != 47 {
            ctx.create_particle(BITMAP_EXPLOTION, j.position, Vec3::ZERO, Vec3::ONE, 1, 0.6, None);
        }
    } else if sub == 8 {
        j.position = j.start_position + rotate(j.target_position, v(0.0, -50.0, 0.0));
        j.target_position.z += 10.0;
    } else if sub == 9 {
        let a = j.angle;
        for _ in 0..j.max_tails {
            j.position += j.start_position;
            create_tail(j, a);
        }
    } else if sub == 13 {
        if j.life_time <= 25.0 {
            j.direction.z += 1.0;
            j.position.x = j.target_position.x;
            j.position.y = j.target_position.y;
            j.position.z += j.direction.z;
        }
    } else if sub == 17 {
        // (CA >= WALK_MALE) || (CA en SWORD1..5 && CA != RAGE_UNI_STOP_ONE_RIGHT), como el original.
        let moving = ctx.actor(j.target).is_some_and(|a| {
            let ca = a.action as i32;
            ca >= PLAYER_WALK_MALE
                || ((PLAYER_ATTACK_SKILL_SWORD1..=PLAYER_ATTACK_SKILL_SWORD5).contains(&ca) && ca != PLAYER_RAGE_UNI_STOP_ONE_RIGHT)
        });
        if moving {
            j.life_time -= 10.0;
        } else {
            j.direction.x += ((ctx.rand() % 360) as f32 * 0.1).sin() * 4.0;
            j.direction.y += ((ctx.rand() % 360) as f32 * 0.1).cos() * 4.0;
            j.direction.z += j.velocity;
            j.position = j.target_position + j.direction;
        }
        if j.life_time < 12.0 {
            j.light *= 1.0 / 1.3;
        }
    } else if sub == 16 && j.target.is_some() {
        if let Some(tp) = ctx.target_position(j.target) {
            let delta = j.target_position - tp;
            j.target_position = tp;
            let n = j.tails.len().saturating_sub(1);
            for t in j.tails[..n].iter_mut() {
                for p in t.iter_mut() {
                    *p -= delta;
                }
            }
        }
        if j.life_time < 20.0 {
            j.light *= 1.0 / 1.1;
        } else if j.life_time < 40.0 {
            j.light = Vec3::splat(0.25);
        } else {
            j.light *= 1.04;
        }
    } else if sub == 23 {
        if j.life_time < 20.0 {
            j.velocity = 200.0;
            if j.life_time < 8.0 {
                j.velocity = 0.0;
                let a = j.head_angle;
                create_tail(j, a);
            } else {
                let mut i = 0.0;
                while i < j.direction.z {
                    j.direction.y += 1.5;
                    match j.multi_use as i32 {
                        0 => {
                            j.head_angle.x += 3.0;
                            j.head_angle.z -= j.direction.y;
                        }
                        1 => {
                            j.head_angle.x -= 3.0;
                            j.head_angle.z -= j.direction.y;
                        }
                        2 => {
                            j.head_angle.x += 3.0;
                            j.head_angle.z += j.direction.y;
                        }
                        3 => {
                            j.head_angle.x -= 3.0;
                            j.head_angle.z += j.direction.y;
                        }
                        4 => j.head_angle.z -= j.direction.y,
                        5 => j.head_angle.x -= j.direction.y,
                        _ => {}
                    }
                    if j.direction.y > 15.0 {
                        j.direction.y = 15.0;
                    }
                    let a = j.head_angle;
                    j.position = j.start_position + rotate(a, v(0.0, -j.velocity, 0.0));
                    create_tail(j, a);
                    if num_tails(j) >= j.max_tails as i32 - 1 {
                        j.velocity = 0.0;
                        break;
                    }
                    i += 1.0;
                }
                j.direction.z += 4.0;
            }
        }
    } else if sub == 43 {
        if target_live(ctx, j.target) {
            j.life_time = 100.0;
            if ctx.rand_fps_check(3) {
                if let Some(tp) = ctx.target_position(j.target) {
                    j.position = tp;
                }
                j.position.z += 30.0;
                j.position.x += rand_off(ctx, 60, 30);
                j.position.y += rand_off(ctx, 60, 30);
                // CreateJoint(..., 44, Target, Scale, PKKey 0, ..., vPriorColor = Light).
                let mut nj = Joint::new(BITMAP_FLARE, j.position, j.position, j.angle, 44, j.target, j.scale);
                nj.pk_key = 0;
                nj.light = j.light;
                ctx.spawns.push(Spawn::Joint(nj));
            }
        } else {
            j.live = false;
            return;
        }
    } else if sub == 44 {
        if !target_live(ctx, j.target) {
            j.live = false;
            return;
        }
        if j.life_time <= 20.0 {
            j.direction.z += 1.0;
            j.position.x = j.target_position.x;
            j.position.y = j.target_position.y;
            j.position.z += j.direction.z;
            j.light *= 1.0 / 1.05;
            ctx.create_sprite(BITMAP_PIN_LIGHT, j.position, 1.5, j.light, 0.0, 0);
            ctx.create_sprite(BITMAP_PIN_LIGHT, j.position, 0.5, j.light, 0.0, 0);
            if j.position.z >= 400.0 {
                j.live = false;
            }
        }
    }

    let sub = j.sub_type;
    if (sub != 5 && sub != 7 && sub != 11) || sub == 14 || sub == 15 || sub == 16 {
        if j.life_time < 10.0 {
            j.light *= 1.0 / 1.3;
        }
    }

    // Al echar a andar o usar una habilidad de espada, los destellos pequeños se apagan.
    if (sub == 0 || sub == 10 || sub == 18) && j.scale < 40.0 {
        if let Some(a) = ctx.actor(j.target) {
            if is_running_action(a.action as i32) {
                j.sub_type = 1;
                j.create_tails = false;
                j.life_time = (j.life_time as i32).min(10) as f32;
            }
        }
    }
}

// --- BITMAP_FLARE + 1 -------------------------------------------------------------------------------
//
// Varios subtipos siguen a un efecto y usan su `StartPosition` y `HeadAngle`, que el motor aún no
// expone: se aproxima `Target->StartPosition` con la `start_position` de la estela (la posición del
// objetivo al crearla) y `Target->HeadAngle` con cero.

fn flare1_create(j: &mut Joint, ctx: &mut Ctx) {
    assign_slot(j, ctx);
    let sub = j.sub_type;
    let tp_param = j.target_position;
    let scale = j.scale;
    // Subtipo 8: NumTails = -1 (sin cola inicial). Para que el núcleo no ponga una al crear,
    // create_tails se apaga ahora y se reactiva al empezar el primer tick.
    if sub == 8 {
        j.tails.clear();
        j.create_tails = false;
        j.user[RESTORE_TAILS] = 1.0;
    } else {
        j.tails = vec![j.make_tail()];
    }
    j.life_time = j.pk_key as f32;
    j.pk_key = 0;
    j.max_tails = 50;
    match j.skill {
        0 => {
            j.scale = 20.0;
            j.light = Vec3::splat(0.5);
        }
        1 => {
            j.scale = 40.0;
            j.light = v(1.0, 0.5, 0.0);
        }
        3 => {
            j.scale = 20.0;
            // TODO(original): Light = Target->Light (falta la luz del efecto objetivo).
        }
        _ => {}
    }
    match sub {
        0 => {
            j.velocity = 0.0;
            j.direction = v(scale * 1.5, 0.0, 0.0);
        }
        1 => {
            j.velocity = 90.0;
            j.direction = v(scale * 1.5, 0.0, 0.0);
        }
        2 => {
            j.velocity = 180.0;
            j.direction = v(scale * 1.5, 0.0, 0.0);
        }
        3 => {
            j.velocity = 240.0;
            j.direction = v(scale * 1.5, 0.0, 0.0);
        }
        4 => {
            j.render_type = JointRender::AlphaBlendMinus;
            j.life_time = 0.0;
            j.scale = scale;
            j.max_tails = 10;
            j.angle = Vec3::ZERO;
            j.light = Vec3::ONE;
            j.start_position = (tp_param - j.position) / j.max_tails as f32;
            let a = j.angle;
            for _ in 0..j.max_tails {
                j.position += j.start_position;
                create_tail(j, a);
            }
        }
        5 => {
            j.light = Vec3::ONE;
            j.render_type = JointRender::AlphaBlendOther;
            j.render_face = RENDER_FACE_TWO;
            j.life_time = 50.0;
            j.max_tails = 8;
            j.velocity = 3.0;
            j.only_one_render = 2;
        }
        6 => {
            j.light = Vec3::ONE;
            j.render_type = JointRender::AlphaBlendOther;
            j.scale = scale;
            j.max_tails = 16;
            j.velocity = 5.0;
        }
        7 => {
            j.light = Vec3::ONE;
            j.direction = Vec3::ZERO;
            j.render_type = JointRender::AlphaBlendOther;
            j.scale = scale;
            j.max_tails = 16;
            j.velocity = 10.0;
            j.tex_type = BITMAP_FLARE;
            j.direction.x = 15.0;
            // cos/sin de un número de 0 a 359 tomado como radianes, como el original.
            j.position.x = tp_param.x + ((ctx.rand() % 360) as f32).cos() * 40.0;
            j.position.y = tp_param.y - ((ctx.rand() % 360) as f32).sin() * 40.0;
            j.start_position = j.position;
        }
        8 | 9 | 10 | 11 => {
            if sub == 8 || sub == 9 {
                j.render_face = RENDER_FACE_TWO;
            }
            j.render_type = JointRender::AlphaBlendOther;
            j.scale = scale;
            j.max_tails = 20;
            j.life_time = 100.0;
            j.velocity = 0.0;
            j.only_one_render = 1;
            j.multi_use = j.skill as f32;
            if sub == 10 || sub == 11 {
                j.light = Vec3::splat(0.7);
                j.max_tails = 10;
            } else {
                j.light = Vec3::ONE;
            }
            j.direction = Vec3::ZERO;
            if j.target.is_none() {
                j.live = false;
                return;
            }
            if let Some(p) = ctx.target_position(j.target) {
                j.position = p;
            }
            if let Some(a) = ctx.actor(j.target) {
                j.angle = a.angle;
            }
            if sub == 8 {
                j.position.z = 300.0;
            } else if sub == 9 {
                j.position.z += j.multi_use;
            }
        }
        12 => {
            j.light = v(0.6, 0.2, 0.8);
            j.render_type = JointRender::AlphaBlendOther;
            j.scale = scale;
            j.max_tails = 16;
            j.velocity = 70.0;
        }
        13 => {
            j.light = v(0.7, 0.7, 0.3);
            j.render_type = JointRender::AlphaBlendOther;
            j.scale = scale;
            j.max_tails = 16;
            j.velocity = 70.0;
        }
        14 | 17 => {
            j.light = Vec3::ONE;
            j.render_type = JointRender::AlphaBlend;
            j.scale = scale;
            j.max_tails = 16;
            j.velocity = 70.0;
        }
        15 => {
            j.light = v(0.4, 0.9, 0.5);
            j.render_type = JointRender::AlphaBlend;
            j.scale = scale;
            j.max_tails = 16;
            j.velocity = 70.0;
        }
        16 => {
            j.render_type = JointRender::AlphaBlendOther;
            j.render_face = RENDER_FACE_TWO;
            j.life_time = 35.0;
            j.max_tails = 8;
            j.velocity = 7.0;
            j.only_one_render = 2;
            j.light = v(1.0, 0.5, 0.3);
        }
        18 => {
            // Light = vPriorColor (si se dio, ya viene en la luz de la estela).
            j.render_type = JointRender::AlphaBlend;
            j.scale = scale;
            j.max_tails = 7;
            j.velocity = 70.0;
        }
        19 | 20 => {
            j.light = v(0.5, 0.5, 1.0);
            j.render_type = JointRender::AlphaBlend;
            j.scale = scale;
            j.max_tails = 14;
            j.velocity = 30.0;
            j.life_time = 50.0;
        }
        _ => {}
    }
    j.target_position = tp_param;
}

fn flare1_step(j: &mut Joint, ctx: &mut Ctx) {
    begin_step(j);
    let a0 = j.angle;
    let sub = j.sub_type;
    match sub {
        5 | 16 => {
            if j.life_time >= 25.0 {
                j.angle.z -= 10.0;
                j.position.z += 2.5;
            } else {
                j.velocity += 5.0;
                if j.velocity >= 30.0 {
                    j.velocity = 30.0;
                }
                let old = j.angle.z;
                let target = if sub == 5 {
                    j.target_position
                } else {
                    // Target->Position (sin objetivo el original fallaría; se usa TargetPosition).
                    ctx.target_position(j.target).unwrap_or(j.target_position)
                };
                let mut ang = j.angle;
                let distance = move_humming(j.position, &mut ang, target, j.velocity);
                j.angle = ang;
                if distance <= 70.0 && (old - j.angle.z).abs() > 20.0 && j.velocity >= 20.0 {
                    j.velocity -= if sub == 5 { 5.0 } else { 10.0 };
                }
                if j.life_time < 10.0 {
                    j.light.x *= 1.0 / 1.2;
                    j.light.y = j.light.x;
                    j.light.z = j.light.x;
                }
            }
            if sub == 5 {
                let lum = (ctx.rand() % 4 + 8) as f32 * 0.03;
                ctx.add_terrain_light(j.position.x, j.position.y, v(lum * 0.4, lum * 0.8, lum * 0.4), 2.0);
            } else {
                ctx.add_terrain_light(j.position.x, j.position.y, Vec3::ONE, 1.0);
            }
        }
        6 => {
            if let Some(tp) = ctx.target_position(j.target) {
                if j.life_time >= 30.0 {
                    j.angle.z -= 10.0;
                    j.position.z += 2.5;
                }
                j.velocity += 5.0;
                if j.velocity >= 30.0 {
                    j.velocity = 30.0;
                }
                let old = j.angle.z;
                let target = tp + v(0.0, 0.0, 150.0);
                let mut ang = j.angle;
                let distance = move_humming(j.position, &mut ang, target, j.velocity);
                j.angle = ang;
                if distance <= 70.0 && (old - j.angle.z).abs() > 20.0 && j.velocity >= 20.0 {
                    j.velocity -= 10.0;
                }
                if j.life_time < 15.0 {
                    j.light.x *= 1.0 / 1.2;
                    j.light.y = j.light.x;
                    j.light.z = j.light.x;
                }
                let lum = (ctx.rand() % 4 + 8) as f32 * 0.03;
                ctx.add_terrain_light(j.position.x, j.position.y, v(lum * 0.4, lum, lum * 0.8), 2.0);
            } else {
                j.live = false;
            }
        }
        7 => {
            j.position.x = j.start_position.x + (j.life_time / 2.5).sin() * j.direction.x;
            j.position.y = j.start_position.y;
            j.position.z += j.velocity;
            j.velocity -= 0.1;
            j.direction.x -= 0.1;
            if j.life_time < 15.0 {
                j.light *= 1.0 / 1.2;
            }
        }
        8 | 9 => {
            if j.target.is_none() {
                j.live = false;
            } else {
                j.live = target_live(ctx, j.target);
                if let Some(p) = ctx.target_position(j.target) {
                    j.position = p;
                }
                if sub == 8 {
                    j.position.z = 300.0;
                } else {
                    j.position.z += j.multi_use;
                }
            }
        }
        12 | 13 | 14 | 17 | 15 | 18 => {
            if j.target.is_some() {
                if !target_live(ctx, j.target) {
                    j.live = false;
                }
                if sub == 17 && j.live {
                    j.light = v(1.0, 1.0, 0.2);
                }
                if sub == 14 && j.live {
                    ctx.create_sprite(BITMAP_SPARK + 1, j.position, 5.0, j.light, 0.0, 0);
                    ctx.create_particle(BITMAP_SMOKE, j.position, j.angle, j.light, 34, 1.0, None);
                    ctx.create_particle(BITMAP_SMOKE, j.position, j.angle, j.light, 35, 1.0, None);
                    j.light = v(0.7, 0.3, 0.3);
                    ctx.create_particle(BITMAP_SPARK + 1, j.position, j.angle, j.light, 10, 4.0, None);
                    j.light = v(1.0, 1.0, 0.2);
                } else if sub == 15 && j.live {
                    ctx.create_particle(BITMAP_SHINY, j.position, j.angle, j.light, 3, 0.5, None);
                    ctx.create_particle(BITMAP_SHINY, j.position, j.angle, j.light, 3, 0.5, None);
                }
                if j.life_time < 15.0 {
                    j.light *= 1.0 / 1.2;
                }
                if sub != 18 {
                    let lum = (ctx.rand() % 4 + 8) as f32 * 0.03;
                    ctx.add_terrain_light(j.position.x, j.position.y, v(lum * 0.4, lum, lum * 0.8), 2.0);
                }
            } else {
                j.live = false;
            }
        }
        19 | 20 => {
            // Sin objetivo el original fallaría: se deja como está.
            if let Some(tpos) = ctx.target_position(j.target) {
                let tstart = j.start_position;
                let pos = (tpos - tstart) / 3.0;
                j.target_position = tstart;
                let mat = j.angle;
                // `Angle` local sin inicializar en el original: se parte de cero.
                let mut ang = Vec3::ZERO;
                for _ in 0..3 {
                    if sub == 19 {
                        ang.y += 30.0;
                    }
                    j.target_position += pos;
                    j.position = j.target_position + rotate(ang, j.direction);
                    if num_tails(j) < j.max_tails as i32 - 1 || j.skill != 0 {
                        create_tail(j, mat);
                    }
                }
            }
        }
        _ => {
            // 0-4, 10, 11...: dibuja tres colas por tick a lo largo del recorrido del objetivo.
            if target_live(ctx, j.target) {
                let tpos = ctx.target_position(j.target).unwrap_or(j.target_position);
                // TODO(original): Target->StartPosition y Target->HeadAngle (ver arriba).
                let tstart = j.start_position;
                let pos = (tpos - tstart) / 3.0;
                j.target_position = tstart;
                let mat = j.angle;
                let mut ang = Vec3::ZERO;
                ang.y += j.velocity - 90.0;
                for _ in 0..3 {
                    ang.y += 30.0;
                    j.target_position += pos;
                    j.position = j.target_position + rotate(ang, j.direction);
                    if num_tails(j) < j.max_tails as i32 - 1 || j.skill != 0 {
                        create_tail(j, mat);
                    }
                }
                if j.life_time < 15.0 {
                    j.light *= 1.0 / 1.5;
                }
            } else {
                j.life_time = -1.0;
                j.position = j.target_position;
                j.position.z += 50.0;
            }
        }
    }
    follow_with_angle(j, a0);
    update_drawn(j, ctx);
}
