//! Login screen: map 74 traversed by a tour camera, with the `login_theme` music.
//!
//! The route is the original's: the points of `Data/World74/CWScript74.cws` (signature `CWS\0`,
//! point count u32 and 28 bytes per point: index i32, x, y, z f32, pause in ticks i32,
//! speed f32 and distance level f32), looped, at each point's speed.
//!
//! The motion is modern: instead of going in a straight line from point to point and turning late
//! (`CCameraMove::UpdateTourWayPoint`, which also pulls the camera off the corridor center in
//! curves), the camera follows a smooth curve (centripetal Catmull-Rom) through all the
//! points, always along the corridor center, and looks at a point a bit further along the curve:
//! it starts turning before each corner and never jerks. Computed at 25 ticks per second and
//! drawn interpolated, the same at any fps.
//!
//! The view is the original S6 one (`MoveMainCamera` in Main 5.2's `ZzzScene.cpp`): eye height
//! `-300 - 100 + D - 150` with `D = 1100 · level · 0.1` (`110 · level - 550`), pitch -112.5° (looks
//! 22.5° up), 65° vertical field of view and range `390 · level`. The distance
//! level is interpolated between the points' levels. The black fog (from MuMain; the original has it off) goes from the range to ×1.25.
//!
//! The 2D interface (logo, servers and login window) is in login_ui.rs.

use bevy::prelude::*;

use crate::Args;
use crate::scene::GameScene;
use crate::terrain::MU_SCALE;

pub struct LoginPlugin;

impl Plugin for LoginPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameScene::Login), (start_tour, add_fog))
            .add_systems(OnExit(GameScene::Login), restore_camera)
            .add_systems(FixedUpdate, step_tour.run_if(in_state(GameScene::Login)))
            .add_systems(Update, (place_camera, login_music).run_if(in_state(GameScene::Login)))
            .add_systems(OnExit(GameScene::Login), |mut c: Commands| c.remove_resource::<Tour>());
    }
}

/// `kMinTourAccel` / `kMaxTourAccel`: speed per tick.
const MIN_ACCEL: f32 = 0.1;
const MAX_ACCEL: f32 = 100.0;
/// `SetAngleFrustum(-112.5)`: 22.5° up.
const PITCH: f32 = -112.5;
/// `CameraFOV = 65` (vertical, `gluPerspective`) on the login tour.
const TOUR_VFOV: f32 = 65.0;
/// The original's turning (`UpdateTourWayPoint`): at most 1° per tick and `difference / 30`, at the fixed 25 fps
/// of the time. Sharp curves take a while (the first is about 117°).
const MAX_ROTATE: f32 = 1.0;
const ROTATE_DIVISOR: f32 = 30.0;
/// Samples per segment to measure the curve.
const SAMPLES: usize = 64;

#[derive(Clone, Copy)]
struct Waypoint {
    pos: Vec2,
    delay: i32,
    accel: f32,
    /// `fCameraDistanceLevel`.
    level: f32,
}

/// The closed curve through the points, measured by length.
struct Path {
    points: Vec<Waypoint>,
    /// Accumulated length at the start of each segment (`len[i]` = start of segment i → i+1) and total.
    starts: Vec<f32>,
    total: f32,
    /// Per segment: (t, accumulated length from the segment start) of each sample.
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

    /// Segment and parameter `t` at distance `s` from the start of the curve.
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

    /// Distance level at `s`: from the segment's start point level to its end point level.
    fn level(&self, s: f32) -> f32 {
        let (i, t) = self.locate(s);
        let n = self.points.len();
        self.points[i].level + (self.points[(i + 1) % n].level - self.points[i].level) * t
    }

    /// Speed at `s`: that of the point the segment heads to (`targetWaypoint->fCameraMoveAccel`).
    fn speed(&self, s: f32) -> f32 {
        let (i, _) = self.locate(s);
        self.points[(i + 1) % self.points.len()].accel.clamp(MIN_ACCEL, MAX_ACCEL)
    }
}

/// Centripetal Catmull-Rom of segment `i` → `i+1` (closed loop), `t` from 0 to 1.
fn segment_point(points: &[Waypoint], i: usize, t: f32) -> Vec2 {
    let n = points.len();
    let p = |k: usize| points[k % n].pos;
    let (p0, p1, p2, p3) = (p(i + n - 1), p(i), p(i + 1), p(i + 2));
    // Knots with alpha 0.5 (does not overshoot corners like the uniform one).
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
    /// Distance traveled along the curve, this tick and the previous one (for interpolation).
    s: f32,
    prev_s: f32,
    /// Pause at the reached point (ticks left).
    wait: i32,
    /// Segment of the last point passed (for pauses).
    segment: usize,
    /// Camera yaw (MU degrees) this tick and the previous one.
    yaw: f32,
    prev_yaw: f32,
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

/// Starts at the first point, heading to the second (the original's `SetTourMode(TRUE, FALSE, 1)`).
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
    let path = Path::new(points);
    let s = path.starts[0];
    let yaw = look_heading(&path, s);
    commands.insert_resource(Tour { path, s, prev_s: s, wait: 0, segment: 0, yaw, prev_yaw: yaw });
}

/// Camera distance behind the tour point: the original's `D = 1100 · level · 0.1`.
fn behind(path: &Path, s: f32) -> f32 {
    110.0 * path.level(s)
}

/// MU heading (0 = +Y, 90 = +X) from the camera (`D` behind on the path) to the tour
/// point: as in the original, the point turns corners before the camera and the camera starts
/// turning before arriving, but here the camera stays on the path center.
fn look_heading(path: &Path, s: f32) -> f32 {
    let d = path.at(s) - path.at(s - behind(path, s));
    d.x.atan2(d.y).to_degrees().rem_euclid(360.0)
}

/// One tick: advances along the curve at the segment's speed (and waits at points with a pause).
fn step_tour(tour: Option<ResMut<Tour>>) {
    let Some(mut t) = tour else { return };
    let t = &mut *t;
    t.prev_s = t.s;
    t.prev_yaw = t.yaw;
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
    let target = look_heading(&t.path, t.s);
    let delta = signed_delta(t.yaw, target);
    let step = (delta.abs() / ROTATE_DIVISOR).min(MAX_ROTATE);
    t.yaw = if delta.abs() <= step { target } else { (t.yaw + step * delta.signum()).rem_euclid(360.0) };
}

fn signed_delta(from: f32, to: f32) -> f32 {
    (to - from + 540.0).rem_euclid(360.0) - 180.0
}

/// Places the camera between the previous tick and this one.
fn place_camera(
    tour: Option<Res<Tour>>,
    fixed: Res<Time<Fixed>>,
    mut cameras: Query<(&mut Transform, &mut Projection, Option<&mut DistanceFog>), (With<Camera3d>, With<IsDefaultUiCamera>)>,
) {
    let Some(t) = tour else { return };
    let s = t.prev_s + (t.s - t.prev_s) * fixed.overstep_fraction();
    let a = fixed.overstep_fraction();
    let eye = t.path.at(s - behind(&t.path, s));
    let yaw = t.prev_yaw + signed_delta(t.prev_yaw, t.yaw) * a;
    let level = t.path.level(s);
    // `CameraPosition[2] = -300 - 100 + D - 150`.
    let height = -300.0 - 100.0 + 1100.0 * level * 0.1 - 150.0;
    // `CameraViewFar = 390 · level`.
    let far = 390.0 * level;
    for (mut tf, mut proj, fog) in &mut cameras {
        *tf = crate::camera::mu_view(eye.extend(height), PITCH, yaw);
        if let Some(mut fog) = fog {
            fog.falloff = bevy::pbr::FogFalloff::Linear { start: far / MU_SCALE, end: far * 1.25 / MU_SCALE };
        }
        if let Projection::Perspective(p) = &mut *proj {
            // `gluPerspective(65, aspect, near 20, far × 1.4)`.
            p.fov = TOUR_VFOV.to_radians();
            p.near = 20.0 / MU_SCALE;
            p.far = far * 1.4 / MU_SCALE;
        }
    }
}
/// Black login fog (its distance is set by `place_camera`).
fn add_fog(mut commands: Commands, cameras: Query<Entity, (With<Camera3d>, With<IsDefaultUiCamera>)>) {
    for e in &cameras {
        commands.entity(e).insert(DistanceFog { color: Color::BLACK, ..default() });
    }
}

/// On exit: no fog, and the game's field of view and range.
fn restore_camera(mut commands: Commands, mut cameras: Query<(Entity, &mut Projection), (With<Camera3d>, With<IsDefaultUiCamera>)>) {
    for (e, mut proj) in &mut cameras {
        commands.entity(e).remove::<DistanceFog>();
        if let Projection::Perspective(p) = &mut *proj {
            p.fov = crate::camera::mu_fov();
            p.near = 0.2;
            p.far = 400.0;
        }
    }
}

/// `PlayMp3(MUSIC_LOGIN_THEME)`.
fn login_music(mut sound: ResMut<crate::sound::SoundQueue>) {
    sound.music("login_theme");
}
