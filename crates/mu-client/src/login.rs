//! Pantalla de login: el mapa 74 recorrido por una cámara de paseo, con la música `login_theme`.
//!
//! El recorrido es el del original: los puntos de `Data/World74/CWScript74.cws` (firma `CWS\0`,
//! número de puntos u32 y por punto 28 bytes: índice i32, x, y, z f32, pausa en ticks i32,
//! velocidad f32 y nivel de distancia f32), en bucle y a la velocidad de cada punto.
//!
//! El movimiento es moderno: en vez de ir en línea recta de punto a punto y girar con retraso
//! (`CCameraMove::UpdateTourWayPoint`, que además saca la cámara del centro de los pasillos en
//! las curvas), la cámara va por una curva suave (Catmull-Rom centrípeta) que pasa por todos los
//! puntos, siempre por el centro del pasillo, y mira a un punto algo más adelante de la curva:
//! empieza a girar antes de cada esquina y nunca da tirones. Se calcula a 25 ticks por segundo y
//! se dibuja interpolado, igual a cualquier fps.
//!
//! La vista es la del S6 original (`MoveMainCamera` de `ZzzScene.cpp` del Main 5.2): altura del ojo
//! `-300 - 100 + D - 150` con `D = 1100 · nivel · 0.1` (`110 · nivel - 550`), inclinación -112.5° (mira
//! 22.5° hacia arriba), campo de visión de 65° en vertical y alcance `390 · nivel`. El nivel de
//! distancia se interpola entre el de cada punto. La niebla negra va del alcance a ×1.25.
//!
//! Por ahora Enter entra al juego (sin servidor todavía).

use bevy::prelude::*;

use crate::Args;
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

/// `kMinTourAccel` / `kMaxTourAccel`: velocidad por tick.
const MIN_ACCEL: f32 = 0.1;
const MAX_ACCEL: f32 = 100.0;
/// `SetAngleFrustum(-112.5)`: 22.5° hacia arriba.
const PITCH: f32 = -112.5;
/// `CameraFOV = 65` (vertical, `gluPerspective`) en el login con el paseo.
const TOUR_VFOV: f32 = 65.0;
/// La cámara mira a este punto de la curva por delante de ella (unidades de MU).
const LOOK_AHEAD: f32 = 600.0;
/// Muestras por tramo para medir la curva.
const SAMPLES: usize = 64;

#[derive(Clone, Copy)]
struct Waypoint {
    pos: Vec2,
    delay: i32,
    accel: f32,
    /// `fCameraDistanceLevel`.
    level: f32,
}

/// La curva cerrada que pasa por los puntos, medida por longitud.
struct Path {
    points: Vec<Waypoint>,
    /// Longitud acumulada al principio de cada tramo (`len[i]` = inicio del tramo i → i+1) y total.
    starts: Vec<f32>,
    total: f32,
    /// Por tramo: (t, longitud acumulada desde el inicio del tramo) de cada muestra.
    tables: Vec<Vec<(f32, f32)>>,
}

impl Path {
    fn new(points: Vec<Waypoint>) -> Self {
        let n = points.len();
        let mut starts = Vec::with_capacity(n);
        let mut tables = Vec::with_capacity(n);
        let mut total = 0.0;
        for i in 0..n {
            starts.push(total);
            let mut table = vec![(0.0, 0.0)];
            let mut acc = 0.0;
            let mut last = segment_point(&points, i, 0.0);
            for k in 1..=SAMPLES {
                let t = k as f32 / SAMPLES as f32;
                let p = segment_point(&points, i, t);
                acc += (p - last).length();
                table.push((t, acc));
                last = p;
            }
            total += acc;
            tables.push(table);
        }
        Self { points, starts, total, tables }
    }

    /// Tramo y parámetro `t` a la distancia `s` del principio de la curva.
    fn locate(&self, s: f32) -> (usize, f32) {
        let s = s.rem_euclid(self.total.max(1.0));
        let i = self.starts.partition_point(|&st| st <= s).saturating_sub(1);
        let local = s - self.starts[i];
        let table = &self.tables[i];
        let k = table.partition_point(|&(_, d)| d <= local).clamp(1, table.len() - 1);
        let ((t0, d0), (t1, d1)) = (table[k - 1], table[k]);
        let f = if d1 > d0 { (local - d0) / (d1 - d0) } else { 0.0 };
        (i, t0 + (t1 - t0) * f)
    }

    fn at(&self, s: f32) -> Vec2 {
        let (i, t) = self.locate(s);
        segment_point(&self.points, i, t)
    }

    /// Nivel de distancia en `s`: de el del punto de salida al del de llegada del tramo.
    fn level(&self, s: f32) -> f32 {
        let (i, t) = self.locate(s);
        let n = self.points.len();
        self.points[i].level + (self.points[(i + 1) % n].level - self.points[i].level) * t
    }

    /// Velocidad en `s`: la del punto al que va el tramo (`targetWaypoint->fCameraMoveAccel`).
    fn speed(&self, s: f32) -> f32 {
        let (i, _) = self.locate(s);
        self.points[(i + 1) % self.points.len()].accel.clamp(MIN_ACCEL, MAX_ACCEL)
    }
}

/// Catmull-Rom centrípeta del tramo `i` → `i+1` (bucle cerrado), `t` de 0 a 1.
fn segment_point(points: &[Waypoint], i: usize, t: f32) -> Vec2 {
    let n = points.len();
    let p = |k: usize| points[k % n].pos;
    let (p0, p1, p2, p3) = (p(i + n - 1), p(i), p(i + 1), p(i + 2));
    // Nudos con alfa 0.5 (no se pasa de las esquinas como la uniforme).
    let knot = |a: Vec2, b: Vec2| (a - b).length().sqrt().max(1e-3);
    let (t0, t1) = (0.0, knot(p0, p1));
    let t2 = t1 + knot(p1, p2);
    let t3 = t2 + knot(p2, p3);
    let u = t1 + (t2 - t1) * t;
    let a1 = p0 * ((t1 - u) / (t1 - t0)) + p1 * ((u - t0) / (t1 - t0));
    let a2 = p1 * ((t2 - u) / (t2 - t1)) + p2 * ((u - t1) / (t2 - t1));
    let a3 = p2 * ((t3 - u) / (t3 - t2)) + p3 * ((u - t2) / (t3 - t2));
    let b1 = a1 * ((t2 - u) / (t2 - t0)) + a2 * ((u - t0) / (t2 - t0));
    let b2 = a2 * ((t3 - u) / (t3 - t1)) + a3 * ((u - t1) / (t3 - t1));
    b1 * ((t2 - u) / (t2 - t1)) + b2 * ((u - t1) / (t2 - t1))
}

#[derive(Resource)]
struct Tour {
    path: Path,
    /// Distancia recorrida por la curva, en este tick y en el anterior (para interpolar).
    s: f32,
    prev_s: f32,
    /// Pausa en el punto al que se llegó (ticks que quedan).
    wait: i32,
    /// Tramo del último punto pasado (para las pausas).
    segment: usize,
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
                level: f32_at(o + 24)?,
            })
        })
        .collect()
}

/// Empieza en el último punto, yendo hacia el primero (`SetTourMode(TRUE, FALSE, 0)`).
fn start_tour(mut commands: Commands, args: Res<Args>) {
    let file = format!("CWScript{}.cws", args.world);
    let points = mu_data::fs::read(&args.data.join(format!("World{}", args.world)), &file)
        .ok()
        .and_then(|b| read_script(&b))
        .filter(|p| p.len() > 2);
    let Some(points) = points else {
        warn!("login: sin recorrido de cámara ({file})");
        return;
    };
    let n = points.len();
    let path = Path::new(points);
    let s = path.starts[n - 1];
    commands.insert_resource(Tour { path, s, prev_s: s, wait: 0, segment: n - 1 });
}

/// Un tick: avanza por la curva a la velocidad del tramo (y espera en los puntos con pausa).
fn step_tour(tour: Option<ResMut<Tour>>) {
    let Some(mut t) = tour else { return };
    let t = &mut *t;
    t.prev_s = t.s;
    if t.wait > 0 {
        t.wait -= 1;
        return;
    }
    let speed = t.path.speed(t.s);
    t.s += speed;
    if t.s >= t.path.total {
        t.s -= t.path.total;
        t.prev_s -= t.path.total;
    }
    let (segment, _) = t.path.locate(t.s);
    if segment != t.segment {
        t.segment = segment;
        t.wait = t.path.points[segment].delay.max(0);
    }
}

/// Coloca la cámara entre el tick anterior y este.
fn place_camera(
    tour: Option<Res<Tour>>,
    fixed: Res<Time<Fixed>>,
    mut cameras: Query<(&mut Transform, &mut Projection, Option<&mut DistanceFog>), With<Camera3d>>,
) {
    let Some(t) = tour else { return };
    let s = t.prev_s + (t.s - t.prev_s) * fixed.overstep_fraction();
    let eye = t.path.at(s);
    let ahead = t.path.at(s + LOOK_AHEAD) - eye;
    // Rumbo de MU (0 = +Y, 90 = +X), el mismo que da `CreateAngle`.
    let yaw = ahead.x.atan2(ahead.y).to_degrees();
    let level = t.path.level(s);
    // `CameraPosition[2] = -300 - 100 + D - 150`.
    let height = -300.0 - 100.0 + 1100.0 * level * 0.1 - 150.0;
    // `CameraViewFar = 390 · nivel`.
    let far = 390.0 * level;
    for (mut tf, mut proj, fog) in &mut cameras {
        *tf = crate::camera::mu_view(eye.extend(height), PITCH, yaw);
        if let Projection::Perspective(p) = &mut *proj {
            // `gluPerspective(65, aspecto, near 20, far × 1.4)`.
            p.fov = TOUR_VFOV.to_radians();
            p.near = 20.0 / MU_SCALE;
            p.far = far * 1.4 / MU_SCALE;
        }
        if let Some(mut fog) = fog {
            // Niebla lineal negra del alcance a ×1.25.
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
