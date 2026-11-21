//! Pantalla de login: el mapa 74 recorrido por la cámara de paseo del original (`CCameraMove` en
//! modo tour, `DefaultCamera` con `IsTourMode`), con la música `login_theme`.
//!
//! El recorrido sale de `Data/World74/CWScript74.cws`: firma `CWS\0`, número de puntos (u32) y
//! por punto 28 bytes: índice (i32), x, y, z (f32), pausa en ticks (i32), velocidad (f32) y
//! nivel de distancia (f32). Va por los puntos en bucle, a 25 ticks por segundo, y se dibuja
//! interpolado entre ticks.
//!
//! Lo que hace cada tick (`UpdateTourWayPoint`):
//! - avanza hacia el punto siguiente su velocidad (entre 0.1 y 100) y, al llegar, pasa al otro;
//! - a menos de 300 del punto (o del anterior) el rumbo se mezcla con el del tramo siguiente
//!   (o el anterior) para girar suave;
//! - el ángulo de la cámara gira hacia el rumbo como mucho 1° por tick (diferencia / 30);
//! - el nivel de distancia se interpola entre el del punto anterior y el del siguiente.
//!
//! Y la vista (`CalculateCameraPosition` + `SetCameraAngle`):
//! - ojo = recorrido + ajuste del mapa (-300, 650, 950; la altura del recorrido es -300) +
//!   `AngleMatrixᵀ(0, 0, -45°)·(0, -D, 0)`, con `D = 1100 · nivel · 0.1`;
//! - mira con inclinación -112.5° + 40° = -72.5° y giro = ángulo del recorrido - 5°.
//!
//! Por ahora Enter entra al juego (sin servidor todavía).

use bevy::prelude::*;

use crate::Args;
use crate::fx::core::create_angle;
use crate::scene::{GameScene, LoadStep, load_world};
use crate::terrain::MU_SCALE;

pub struct LoginPlugin;

impl Plugin for LoginPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameScene::Login), (start_tour, spawn_login_ui))
            .add_systems(FixedUpdate, step_tour.run_if(in_state(GameScene::Login)))
            .init_resource::<Tuning>()
            .add_systems(Update, (tune, place_camera, login_input).chain().run_if(in_state(GameScene::Login)))
            .add_systems(OnExit(GameScene::Login), (crate::scene::despawn::<LoginUi>, |mut c: Commands| c.remove_resource::<Tour>()));
    }
}

/// `kTourBlendDistance`.
const BLEND_DISTANCE: f32 = 300.0;
/// `kMinTourAccel` / `kMaxTourAccel`.
const MIN_ACCEL: f32 = 0.1;
const MAX_ACCEL: f32 = 100.0;
/// `kTourMaxRotateSpeed` (grados por tick).
const MAX_ROTATE: f32 = 1.0;
/// `kTourCameraZPosition`.
const TOUR_Z: f32 = -300.0;
/// `TOUR_BASE_DISTANCE`.
const BASE_DISTANCE: f32 = 1100.0;
/// `LoginSceneCameraDefaults`: el ajuste del recorrido al mapa actual.
const OFFSET: Vec3 = Vec3::new(-300.0, 650.0, 950.0);
const ANGLE_PITCH: f32 = 40.0;
const ANGLE_YAW: f32 = -5.0;
/// `SetAngleFrustum(-112.5)`.
const FRUSTUM_ANGLE: f32 = -112.5;
/// `RENDER_OBJECT_DIST`: lo más lejos que se ve en el login.
const VIEW_FAR: f32 = 5903.0;

#[derive(Clone, Copy)]
struct Waypoint {
    pos: Vec2,
    delay: i32,
    accel: f32,
    distance: f32,
}

#[derive(Resource)]
struct Tour {
    points: Vec<Waypoint>,
    /// Índice del punto al que va.
    index: usize,
    delay: i32,
    /// `m_CurrentCameraPos` (va por los puntos) y `m_vTourCameraPos` (lo que sigue la vista).
    current: Vec2,
    tour: Vec3,
    angle: f32,
    target_angle: f32,
    level: f32,
    /// Vista del tick anterior y de este (ojo, ángulo), para interpolar.
    prev: (Vec3, f32, f32),
    now: (Vec3, f32, f32),
}

fn read_script(bytes: &[u8]) -> Option<Vec<Waypoint>> {
    let u32_at = |o: usize| bytes.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let f32_at = |o: usize| u32_at(o).map(f32::from_bits);
    if u32_at(0)? != 0x0053_5743 {
        return None;
    }
    let n = u32_at(4)? as usize;
    (0..n)
        .map(|i| {
            let o = 8 + i * 28;
            Some(Waypoint {
                pos: Vec2::new(f32_at(o + 4)?, f32_at(o + 8)?),
                delay: u32_at(o + 16)? as i32,
                accel: f32_at(o + 20)?,
                distance: f32_at(o + 24)?,
            })
        })
        .collect()
}

/// `PlayCameraWalk` + `SetTourMode(TRUE, FALSE, 0)`.
fn start_tour(mut commands: Commands, args: Res<Args>) {
    let file = format!("CWScript{}.cws", args.world);
    let points = mu_data::fs::read(&args.data.join(format!("World{}", args.world)), &file)
        .ok()
        .and_then(|b| read_script(&b))
        .filter(|p| p.len() > 1);
    let Some(points) = points else {
        warn!("login: sin recorrido de cámara ({file})");
        return;
    };
    let (start, target) = (points[points.len() - 1], points[0]);
    // El original aplica el ajuste a las dos posiciones al empezar (y otra vez al dibujar).
    let current = start.pos + OFFSET.truncate();
    let tour = start.pos.extend(0.0) + OFFSET;
    let dir = (target.pos - start.pos).normalize_or_zero();
    let angle = create_angle(0.0, 0.0, dir.x, -dir.y);
    let mut t = Tour { points, index: 0, delay: 0, current, tour, angle, target_angle: angle, level: start.distance, prev: (Vec3::ZERO, 0.0, 0.0), now: (Vec3::ZERO, 0.0, 0.0) };
    t.now = view(&t);
    t.prev = t.now;
    commands.insert_resource(t);
}

/// Ajustes de prueba de la cámara (teclas en el login), para compararla con el cliente original.
#[derive(Resource, Clone, Copy, PartialEq)]
struct Tuning {
    distance: f32,
    offset: Vec3,
    pitch: f32,
}

impl Default for Tuning {
    fn default() -> Self {
        Self { distance: 1.0, offset: Vec3::ZERO, pitch: 0.0 }
    }
}

/// RePág/AvPág: distancia ×1.1 / ÷1.1 · flechas: mover 50 · Inicio/Fin: altura ±50 ·
/// Insert/Supr: inclinación ±2.5°. Cada cambio se apunta en el registro.
fn tune(keys: Res<ButtonInput<KeyCode>>, mut tuning: ResMut<Tuning>) {
    let before = *tuning;
    let t = &mut *tuning;
    let pressed = |k: KeyCode| keys.just_pressed(k);
    if pressed(KeyCode::PageUp) { t.distance *= 1.1; }
    if pressed(KeyCode::PageDown) { t.distance /= 1.1; }
    if pressed(KeyCode::ArrowLeft) { t.offset.x -= 50.0; }
    if pressed(KeyCode::ArrowRight) { t.offset.x += 50.0; }
    if pressed(KeyCode::ArrowUp) { t.offset.y += 50.0; }
    if pressed(KeyCode::ArrowDown) { t.offset.y -= 50.0; }
    if pressed(KeyCode::Home) { t.offset.z += 50.0; }
    if pressed(KeyCode::End) { t.offset.z -= 50.0; }
    if pressed(KeyCode::Insert) { t.pitch += 2.5; }
    if pressed(KeyCode::Delete) { t.pitch -= 2.5; }
    if *t != before {
        info!("login: cámara de prueba: distancia x{:.2}, desplazamiento {:?}, inclinación {:+.1}°", t.distance, t.offset, t.pitch);
    }
}

/// Ojo, inclinación y giro de la vista de este tick.
fn view(t: &Tour) -> (Vec3, f32, f32) {
    let d = BASE_DISTANCE * t.level * 0.1;
    // `AngleMatrixᵀ(0, 0, -45°)·(0, -D, 0)`.
    let (s, c) = (-45f32).to_radians().sin_cos();
    let offset = Vec3::new(-s * d, -c * d, 0.0);
    let mut eye = t.tour + offset + OFFSET;
    eye.z = TOUR_Z + OFFSET.z;
    (eye, FRUSTUM_ANGLE + ANGLE_PITCH, t.angle + ANGLE_YAW)
}

fn signed_delta(from: f32, to: f32) -> f32 {
    let d = to.rem_euclid(360.0) - from.rem_euclid(360.0);
    if d > 180.0 {
        d - 360.0
    } else if d < -180.0 {
        d + 360.0
    } else {
        d
    }
}

/// `UpdateTourWayPoint`, un tick.
fn step_tour(tour: Option<ResMut<Tour>>) {
    let Some(mut t) = tour else { return };
    let t = &mut *t;
    t.prev = t.now;
    let n = t.points.len();
    loop {
        let target = t.points[t.index];
        let origin = t.points[(t.index + n - 1) % n];
        let target_acc = target.accel.clamp(MIN_ACCEL, MAX_ACCEL);
        if t.delay >= target.delay {
            let to_target = target.pos - t.current;
            let to_origin = origin.pos - t.current;
            let (dist_target, dist_origin) = (to_target.length(), to_origin.length());
            let forward = to_target.normalize_or_zero();
            let mut dir = forward;
            if dist_target <= BLEND_DISTANCE {
                let next = t.points[(t.index + 1) % n];
                let seg = next.pos - target.pos;
                if seg.length() > f32::EPSILON {
                    let rate = dist_target / BLEND_DISTANCE * 0.5 + 0.5;
                    dir = (seg.normalize() * (1.0 - rate) + forward * rate).normalize_or_zero();
                }
            } else if dist_origin <= BLEND_DISTANCE {
                let prev = t.points[(t.index + n - 2) % n];
                let seg = origin.pos - prev.pos;
                if seg.length() > f32::EPSILON {
                    let rate = dist_origin / BLEND_DISTANCE * 0.5 + 0.5;
                    dir = (seg.normalize() * (1.0 - rate) + forward * rate).normalize_or_zero();
                }
            }
            // Llegó al punto: al siguiente (en el mismo tick).
            if dist_target <= target_acc {
                t.index = (t.index + 1) % n;
                t.delay = 0;
                continue;
            }
            if dist_target > 5.0 && dist_origin > 5.0 {
                t.target_angle = create_angle(0.0, 0.0, dir.x, -dir.y);
            }
            let delta = signed_delta(t.angle, t.target_angle);
            let step = (delta.abs() / 30.0).clamp(0.0, MAX_ROTATE);
            t.angle = if delta.abs() <= step { t.target_angle.rem_euclid(360.0) } else { (t.angle + step * delta.signum()).rem_euclid(360.0) };
            t.current += forward * target_acc;
            t.tour += (dir * target_acc).extend(0.0);
            t.tour.z = TOUR_Z;
            let total = dist_origin + dist_target;
            t.level = if total > f32::EPSILON {
                origin.distance * dist_target / total + target.distance * dist_origin / total
            } else {
                target.distance
            };
        }
        t.delay += 1;
        break;
    }
    t.now = view(t);
}

/// Coloca la cámara entre el tick anterior y este.
fn place_camera(
    tour: Option<Res<Tour>>,
    tuning: Res<Tuning>,
    fixed: Res<Time<Fixed>>,
    mut cameras: Query<(&mut Transform, &mut Projection), With<Camera3d>>,
) {
    let Some(t) = tour else { return };
    let a = fixed.overstep_fraction();
    let mut eye = t.prev.0.lerp(t.now.0, a);
    let pitch = t.prev.1 + (t.now.1 - t.prev.1) * a + tuning.pitch;
    // Ajuste de prueba: la distancia se alarga sobre la diagonal del original.
    if tuning.distance != 1.0 {
        let d = BASE_DISTANCE * t.level * 0.1 * (tuning.distance - 1.0);
        let (s, c) = (-45f32).to_radians().sin_cos();
        eye += Vec3::new(-s * d, -c * d, 0.0);
    }
    eye += tuning.offset;
    let yaw = t.prev.2 + signed_delta(t.prev.2, t.now.2) * a;
    for (mut tf, mut proj) in &mut cameras {
        *tf = crate::camera::mu_view(eye, pitch, yaw);
        if let Projection::Perspective(p) = &mut *proj {
            p.far = VIEW_FAR / MU_SCALE;
        }
    }
}

#[derive(Component)]
struct LoginUi;

fn spawn_login_ui(mut commands: Commands) {
    commands.spawn((
        Name::new("Login"),
        LoginUi,
        Text::new("Pulsa Enter para entrar"),
        TextFont { font_size: bevy::text::FontSize::Px(10.0), ..default() },
        TextColor(Color::WHITE),
        TextShadow { offset: Vec2::splat(0.6), color: Color::BLACK },
        Node { position_type: PositionType::Absolute, bottom: Val::Px(40.0), left: Val::Percent(50.0), margin: UiRect::left(Val::Px(-55.0)), ..default() },
    ));
}

fn login_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut args: ResMut<Args>,
    mut load: ResMut<LoadStep>,
    mut next: ResMut<NextState<GameScene>>,
    mut sound: ResMut<crate::sound::SoundQueue>,
    mut cameras: Query<&mut Projection, With<Camera3d>>,
) {
    sound.music("login_theme");
    if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter) {
        for mut proj in &mut cameras {
            if let Projection::Perspective(p) = &mut *proj {
                p.far = 400.0;
            }
        }
        load_world(&mut args, &mut load, 1, true, GameScene::Game);
        next.set(GameScene::Loading);
    }
}
