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
//! Y la vista (ver [`view`]): la de MuMain, con FOV de 90° en horizontal, niebla negra y alcance
//! de 3995, mirando 17.5° hacia abajo en la dirección del recorrido.
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
        app.add_systems(OnEnter(GameScene::Login), (start_tour, spawn_login_ui, add_fog))
            .add_systems(OnExit(GameScene::Login), restore_camera)
            .add_systems(FixedUpdate, step_tour.run_if(in_state(GameScene::Login)))
            .add_systems(Update, (place_camera, login_input).run_if(in_state(GameScene::Login)))
            .add_systems(OnExit(GameScene::Login), (crate::scene::despawn::<LoginUi>, |mut c: Commands| c.remove_resource::<Tour>()));
    }
}

/// `kTourBlendDistance`.
const BLEND_DISTANCE: f32 = 300.0;
/// `kMinTourAccel` / `kMaxTourAccel`.
const MIN_ACCEL: f32 = 0.1;
const MAX_ACCEL: f32 = 100.0;
/// Giro de la cámara (`kTourMaxRotateSpeed` 1° y `diferencia / 30`): MuMain lo aplica una vez por
/// fotograma, sin ajustarlo a los fps. Aquí va por tick (25 por segundo) con lo que da MuMain a 60 fps:
/// como mucho 2.4° por tick y `diferencia / 12.5`.
const MAX_ROTATE: f32 = 1.0 * 60.0 / 25.0;
const ROTATE_DIVISOR: f32 = 30.0 * 25.0 / 60.0;
/// `kTourCameraZPosition`.
const TOUR_Z: f32 = -300.0;
/// `TOUR_BASE_DISTANCE`.
const BASE_DISTANCE: f32 = 1100.0;
/// `SetAngleFrustum(-112.5)`.
const FRUSTUM_ANGLE: f32 = -112.5;
/// MuMain: `LoginSceneCameraDefaults` y `RENDER_TERRAIN_DIST` (alcance; la niebla va de ahí a ×1.25).
const MUMAIN_OFFSET: Vec3 = Vec3::new(-300.0, 650.0, 950.0);
const MUMAIN_PITCH: f32 = 40.0;
const MUMAIN_YAW: f32 = -5.0;
const MUMAIN_FAR: f32 = 3995.0;
/// FOV del paseo: 90° en horizontal a 4:3 (`CameraConfig::ForLoginScene`) → 73.74° en vertical.
const TOUR_HFOV: f32 = 90.0;

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
    /// Rumbo del primer tramo (referencia del desplazamiento de la cámara).
    ref_angle: f32,
    level: f32,
    /// Estado del tick anterior y de este (recorrido, nivel de distancia, ángulo), para interpolar.
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
    let current = start.pos;
    let tour = start.pos.extend(TOUR_Z);
    let dir = (target.pos - start.pos).normalize_or_zero();
    let angle = create_angle(0.0, 0.0, dir.x, -dir.y);
    let mut t = Tour { points, index: 0, delay: 0, current, tour, angle, target_angle: angle, ref_angle: angle, level: start.distance, prev: (Vec3::ZERO, 0.0, 0.0), now: (Vec3::ZERO, 0.0, 0.0) };
    t.now = (t.current.extend(TOUR_Z), t.level, t.angle);
    t.prev = t.now;
    commands.insert_resource(t);
}

/// Ojo, inclinación, giro y alcance de la vista: los de MuMain (`DefaultCamera` en modo paseo).
///
/// `ojo = recorrido + (-300, 650) + AngleMatrixᵀ(0, 0, -45°)·(0, -D, 0)` (un desplazamiento fijo en el
/// mundo), altura -300 + 950 = 650, inclinación -112.5° + 40° = -72.5° y giro = ángulo - 5°.
fn view(tour: Vec3, level: f32, angle: f32) -> (Vec3, f32, f32, f32) {
    let d = BASE_DISTANCE * level * 0.1;
    let (s, c) = (-45f32).to_radians().sin_cos();
    let eye = (tour.truncate() + Vec2::new(-s * d, -c * d) + MUMAIN_OFFSET.truncate()).extend(TOUR_Z + MUMAIN_OFFSET.z);
    (eye, FRUSTUM_ANGLE + MUMAIN_PITCH, angle + MUMAIN_YAW, MUMAIN_FAR)
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
            let step = (delta.abs() / ROTATE_DIVISOR).clamp(0.0, MAX_ROTATE);
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
    // La vista va por el recorrido exacto (`current`): el camino suavizado del original (`tour`) se
    // desvía unas 135 unidades por vuelta y no se corrige nunca. Las curvas las suaviza el giro.
    t.now = (t.current.extend(TOUR_Z), t.level, t.angle);
}

/// Coloca la cámara entre el tick anterior y este.
fn place_camera(
    tour: Option<Res<Tour>>,
    fixed: Res<Time<Fixed>>,
    mut cameras: Query<(&mut Transform, &mut Projection, Option<&mut DistanceFog>), With<Camera3d>>,
) {
    let Some(t) = tour else { return };
    let a = fixed.overstep_fraction();
    let tour_pos = t.prev.0.lerp(t.now.0, a);
    let level = t.prev.1 + (t.now.1 - t.prev.1) * a;
    let angle = t.prev.2 + signed_delta(t.prev.2, t.now.2) * a;
    let (eye, pitch, yaw, far) = view(tour_pos, level, angle);
    let vfov = 2.0 * ((TOUR_HFOV * 0.5).to_radians().tan() / (4.0 / 3.0)).atan();
    for (mut tf, mut proj, fog) in &mut cameras {
        *tf = crate::camera::mu_view(eye, pitch, yaw);
        if let Projection::Perspective(p) = &mut *proj {
            // `gluPerspective(fov, aspecto, near 100, far × 1.4)`.
            p.fov = vfov;
            p.near = 100.0 / MU_SCALE;
            p.far = far * 1.4 / MU_SCALE;
        }
        if let Some(mut fog) = fog {
            // Niebla lineal negra de `far` a `far × 1.25`.
            fog.falloff = bevy::pbr::FogFalloff::Linear { start: far / MU_SCALE, end: far * 1.25 / MU_SCALE };
        }
    }
}

/// Niebla negra del login (`FogEnable = true`; la distancia la pone `place_camera`).
fn add_fog(mut commands: Commands, cameras: Query<Entity, With<Camera3d>>) {
    for e in &cameras {
        commands.entity(e).insert(DistanceFog { color: Color::BLACK, ..default() });
    }
}

/// Al salir: sin niebla y con el campo de visión y el alcance del juego.
fn restore_camera(mut commands: Commands, mut cameras: Query<(Entity, &mut Projection), With<Camera3d>>) {
    for (e, mut proj) in &mut cameras {
        commands.entity(e).remove::<DistanceFog>();
        if let Projection::Perspective(p) = &mut *proj {
            p.fov = crate::camera::mu_fov();
            p.near = 0.2;
            p.far = 400.0;
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
) {
    sound.music("login_theme");
    if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter) {
        load_world(&mut args, &mut load, 1, true, GameScene::Game);
        next.set(GameScene::Loading);
    }
}
