//! Camera.
//!
//! - **Follow** (the game camera): diagonal top-down view. The wheel (or Ctrl + wheel, like
//!   `SetViewPortLevel`, ZzzScene.cpp:493) changes `g_shCameraLevel` from 0 to 4: distance 1000 to 1400,
//!   with its view range (`CameraViewFar`) and ground culling (`CreateFrustrum2D`). The distance
//!   closes a third of the gap each tick (`CameraDistance += (Target - CameraDistance) / 3`). Main 5.2
//!   resets the level to 0 every frame, so the original cannot zoom out; here it can.
//! - **Free** (testing only, with `--cam`): right button + mouse to look, WASD to move,
//!   Q/E down/up, Shift for speed.
//! With `--screenshot`, takes a capture at startup and quits.

use std::sync::atomic::{AtomicU8, Ordering};

use bevy::camera::RenderTarget;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};

use crate::Args;
use crate::character::Player;
use crate::terrain::MU_SCALE;

pub struct FlyCameraPlugin;

impl Plugin for FlyCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera)
            .add_systems(Update, (fly, auto_screenshot))
            .add_systems(Update, zoom_wheel.run_if(in_state(crate::scene::GameScene::Game)))
            // After the character moves (Update), so the camera is not one frame late.
            .add_systems(PostUpdate, follow.in_set(CameraMoved).before(TransformSystems::Propagate))
            .add_systems(
                PostUpdate,
                game_far_plane.after(CameraMoved).run_if(in_state(crate::scene::GameScene::Game)),
            );
    }
}

/// `battleCastle::InBattleCastle2` (GMBattleCastle.h:11): the Valley of Loren castle, in MU units.
pub fn in_battle_castle2(p: Vec2) -> bool {
    (16100.0..=19000.0).contains(&p.x) && (18900.0..=21700.0).contains(&p.y)
}

/// `g_shCameraLevel` (0 to 4) chosen by the player with the wheel.
static CAMERA_LEVEL: AtomicU8 = AtomicU8::new(0);
/// The ground-culling `static CameraLevel` (ZzzLodTerrain.cpp:2010-2013): it only follows the level
/// once the camera is no farther than its target (when zooming in, it keeps the large culling area
/// until it arrives).
static CULL_LEVEL: AtomicU8 = AtomicU8::new(0);

/// Camera level on a map: Vulcanus and Doppelganger 1 and 2 force it to 5
/// (ZzzScene.cpp:1721-1729); elsewhere, the wheel level.
fn level_in(world: u16) -> u8 {
    match world as i32 - 1 {
        63 | 65 | 66 => 5,
        _ => CAMERA_LEVEL.load(Ordering::Relaxed),
    }
}

/// Camera level for ground culling (terrain.rs).
pub fn cull_level(world: u16) -> u8 {
    match world as i32 - 1 {
        63 | 65 | 66 => 5,
        _ => CULL_LEVEL.load(Ordering::Relaxed),
    }
}

/// `CameraDistanceTarget` for the level, in MU units (ZzzScene.cpp:1844-1852). At level 5 the original
/// uses `g_Direction.m_fCameraViewFar` (from events); without them, 1000.
fn distance_target(level: u8) -> f32 {
    match level {
        1 => 1100.0,
        2 => 1200.0,
        3 => 1300.0,
        4 => 1400.0,
        _ => 1000.0,
    }
}

/// Game `CameraViewFar` in MU units (`MoveMainCamera`, ZzzScene.cpp:1652-1698), by map and
/// `g_shCameraLevel`. `hero` is the character position in MU units.
///
/// The `InBattleCastle2` check ignores the map, as in the original.
pub fn view_far(world: u16, hero: Vec2) -> f32 {
    match world as i32 - 1 {
        _ if in_battle_castle2(hero) => 3000.0,
        // Valley of Loren (`InBattleCastle() && MAIN_SCENE`).
        30 => 2500.0,
        // Elbeland (`WD_51HOME_6TH_CHAR`).
        51 => 2800.0 * 1.15,
        // Vulcanus and Doppelganger 2.
        63 | 66 => 3700.0,
        // Doppelganger 1: level 5 → `case 4/5`.
        65 => 3200.0,
        _ => match level_in(world) {
            1 => 2500.0,
            2 => 2600.0,
            3 => 2950.0,
            4 | 5 => 3200.0,
            _ => 2000.0,
        },
    }
}

/// `SetViewPortLevel`: the wheel raises or lowers the level (towards you zooms in, away zooms out),
/// outside the UI.
fn zoom_wheel(
    scroll: Res<bevy::input::mouse::AccumulatedMouseScroll>,
    over_ui: Res<crate::mainframe::PointerOverUi>,
    game_ui: Res<crate::options::GameUi>,
    windows: Query<&Window>,
) {
    if scroll.delta.y == 0.0 || over_ui.0 {
        return;
    }
    // Over a game window (options, with its volume slider) the wheel belongs to the window.
    let on_window =
        windows.single().ok().and_then(crate::mainframe::cursor_virtual).is_some_and(|c| game_ui.blocks(c));
    if on_window {
        return;
    }
    let level = CAMERA_LEVEL.load(Ordering::Relaxed) as i32;
    let next = if scroll.delta.y > 0.0 { level - 1 } else { level + 1 };
    CAMERA_LEVEL.store(next.clamp(0, 4) as u8, Ordering::Relaxed);
}

/// Game camera range: `gluPerspective(..., CameraViewNear, CameraViewFar · 1.4)`
/// (`BeginOpengl`, ZzzOpenglUtil.cpp:643). Anything farther is not drawn and the background shows.
fn game_far_plane(
    args: Res<Args>,
    players: Query<&Transform, (With<Player>, Without<FollowCamera>)>,
    mut cams: Query<&mut Projection, With<FollowCamera>>,
) {
    let hero = players.single().map_or(Vec2::ZERO, |p| Vec2::new(p.translation.x, -p.translation.z) * MU_SCALE);
    let far = view_far(args.world, hero) * 1.4 / MU_SCALE;
    for mut proj in &mut cams {
        // Leave it alone if already right (changing it rebuilds the frustum).
        if matches!(&*proj, Projection::Perspective(p) if p.far != far)
            && let Projection::Perspective(p) = &mut *proj
        {
            p.far = far;
        }
    }
}

/// When the camera is placed each frame (for things that face it).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct CameraMoved;

#[derive(Component)]
struct FlyCamera {
    yaw: f32,
    pitch: f32,
}

#[derive(Component)]
struct FollowCamera {
    /// Distance to the character, in tiles (original zoom step / 100).
    distance: f32,
}

#[derive(Resource, PartialEq, Eq, Clone, Copy)]
enum CameraMode {
    Follow,
    Free,
}

/// Image the camera renders to in capture mode (no window).
#[derive(Resource)]
struct OffscreenTarget(Handle<Image>);

fn spawn_camera(mut commands: Commands, args: Res<Args>, mut images: ResMut<Assets<Image>>) {
    let [ex, ey, eh, tx, ty, th] = args.cam.unwrap_or([135.0, 112.0, 14.0, 135.0, 130.0, 2.0]);
    // MU (x, y, height) → Bevy (x, height, -y)
    let eye = Vec3::new(ex, eh, -ey);
    let target = Vec3::new(tx, th, -ty);
    let transform = Transform::from_translation(eye).looking_at(target, Vec3::Y);
    let (yaw, pitch, _) = transform.rotation.to_euler(EulerRot::YXZ);

    commands.insert_resource(if args.cam.is_some() { CameraMode::Free } else { CameraMode::Follow });
    let mut cam = commands.spawn((
        Camera3d::default(),
        // Raw colors, like the original's OpenGL: without Bevy's default `TonyMcMapface`, which
        // brightens shadows and lowers contrast.
        bevy::core_pipeline::tonemapping::Tonemapping::None,
        bevy::core_pipeline::tonemapping::DebandDither::Disabled,
        // Without Bevy's default 4× antialiasing: the original has none, and it is expensive
        // on modest GPUs.
        Msaa::Off,
        Projection::Perspective(PerspectiveProjection {
            fov: mu_fov(),
            near: 0.2,
            // Outside the game (login, selection) each screen sets its own; in game, `game_far_plane`.
            far: 400.0,
            ..default()
        }),
        transform,
        FlyCamera { yaw, pitch },
        FollowCamera { distance: args.zoom.unwrap_or(ZOOM_DEFAULT) },
        // No dynamic lights (lighting is baked into the map), so no light clustering is needed.
        // (`None` fails in Bevy 0.20 with a zero-size texture; `Single` is the minimum.)
        bevy::light::cluster::ClusterConfig::Single,
        // The UI goes on this camera even when others exist (the character-creation preview).
        IsDefaultUiCamera,
    ));

    if args.screenshot.is_some() {
        // 4:3 like the game resolutions, with the UI drawn into the capture.
        let image = images.add(Image::new_target_texture(1024, 768, TextureFormat::Rgba8UnormSrgb, None));
        cam.insert(RenderTarget::Image(image.clone().into()));
        commands.insert_resource(bevy::ui::UiScale(768.0 / 480.0));
        commands.insert_resource(OffscreenTarget(image));
    }
}


fn follow(
    mode: Res<CameraMode>,
    args: Res<Args>,
    time: Res<Time>,
    players: Query<&Transform, (With<Player>, Without<FollowCamera>)>,
    mut cams: Query<(&mut Transform, &mut FollowCamera)>,
) {
    if *mode != CameraMode::Follow {
        return;
    }
    let (Ok(player), Ok((mut tf, mut cam))) = (players.single(), cams.single_mut()) else { return };
    // `--zoom` fixes the distance; otherwise the level's, closing a third of the gap per tick (25 Hz).
    if args.zoom.is_none() {
        let target = distance_target(level_in(args.world)) / MU_SCALE;
        let k = 1.0 - (2.0f32 / 3.0).powf(time.delta_secs() * crate::fx::sim::TICKS_PER_SECOND);
        cam.distance += (target - cam.distance) * k;
        if (cam.distance - target).abs() < 1e-3 {
            cam.distance = target;
        }
        if target >= cam.distance {
            CULL_LEVEL.store(CAMERA_LEVEL.load(Ordering::Relaxed), Ordering::Relaxed);
        }
    }
    *tf = mu_camera(player.translation, cam.distance * MU_SCALE);
}

/// Game `CameraDistance` in tiles (1000 MU units, `g_shCameraLevel` 0).
pub const ZOOM_DEFAULT: f32 = 10.0;

/// Game vertical field of view: `CameraFOV = 35` (Main 5.2 `MoveMainCamera`, in
/// `gluPerspective`). MuMain changed it to 40° horizontal (about 30.5° vertical).
pub fn mu_fov() -> f32 {
    35f32.to_radians()
}

/// Game camera as the original computes it (`DefaultCamera::Update`):
///
/// 1. With the base angles (0, 0, -45°) the position is computed: `hero + AngleMatrixᵀ·(0, -distance, 0)`
///    horizontally, and height `hero.z + distance - 150`.
/// 2. Then `SetCameraAngle` sets the -48.5° pitch, which only affects the view
///    (`Ry(a1)·Rx(a0)·Rz(a2)` OpenGL-style).
///
/// Result: the camera sits south-east of the character looking north-west, tilted 41.5°
/// down, with the character exactly in the screen center.
/// `hero` is in Bevy coordinates; `distance` in MU units.
fn mu_camera(hero: Vec3, distance: f32) -> Transform {
    let (s0, c0) = (-48.5f32).to_radians().sin_cos();
    let (s2, c2) = (-45f32).to_radians().sin_cos();
    // In MU coordinates (x, y, z up):
    let forward = Vec3::new(-s0 * s2, -s0 * c2, -c0);
    let up = Vec3::new(c0 * s2, c0 * c2, -s0);
    // Position with zero pitch (step 1): only the yaw rotates.
    let offset = Vec3::new(-s2 * distance, -c2 * distance, distance - 150.0);
    // MU (x, y, z) → Bevy (x, z, -y), 100 units per tile.
    let to_bevy = |v: Vec3| Vec3::new(v.x, v.z, -v.y);
    Transform::from_translation(hero + to_bevy(offset) / MU_SCALE).looking_to(to_bevy(forward), to_bevy(up))
}

/// Original view from `eye` (MU) with `pitch` and `yaw` in degrees (same axes as
/// [`mu_camera`]: `Ry(a1)·Rx(a0)·Rz(a2)` OpenGL-style).
pub fn mu_view(eye: Vec3, pitch: f32, yaw: f32) -> Transform {
    let (s0, c0) = pitch.to_radians().sin_cos();
    let (s2, c2) = yaw.to_radians().sin_cos();
    let forward = Vec3::new(-s0 * s2, -s0 * c2, -c0);
    let up = Vec3::new(c0 * s2, c0 * c2, -s0);
    let to_bevy = |v: Vec3| Vec3::new(v.x, v.z, -v.y);
    Transform::from_translation(to_bevy(eye) / MU_SCALE).looking_to(to_bevy(forward), to_bevy(up))
}

fn fly(
    mode: Res<CameraMode>,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    mouse: Res<AccumulatedMouseMotion>,
    mut q: Query<(&mut Transform, &mut FlyCamera)>,
) {
    if *mode != CameraMode::Free {
        return;
    }
    let Ok((mut tf, mut cam)) = q.single_mut() else { return };

    if buttons.pressed(MouseButton::Right) {
        cam.yaw -= mouse.delta.x * 0.003;
        cam.pitch = (cam.pitch - mouse.delta.y * 0.003).clamp(-1.54, 1.54);
        tf.rotation = Quat::from_euler(EulerRot::YXZ, cam.yaw, cam.pitch, 0.0);
    }

    let mut dir = Vec3::ZERO;
    let fwd = tf.forward().as_vec3();
    let right = tf.right().as_vec3();
    if keys.pressed(KeyCode::KeyW) { dir += fwd; }
    if keys.pressed(KeyCode::KeyS) { dir -= fwd; }
    if keys.pressed(KeyCode::KeyD) { dir += right; }
    if keys.pressed(KeyCode::KeyA) { dir -= right; }
    if keys.pressed(KeyCode::KeyE) { dir += Vec3::Y; }
    if keys.pressed(KeyCode::KeyQ) { dir -= Vec3::Y; }

    let speed = if keys.pressed(KeyCode::ShiftLeft) { 60.0 } else { 15.0 };
    tf.translation += dir.normalize_or_zero() * speed * time.delta_secs();
}

/// Waits until everything is on the GPU (shaders compile in the background and are slow without
/// a graphics card), captures, and quits once the file is saved.
fn auto_screenshot(
    mut commands: Commands,
    args: Res<Args>,
    target: Option<Res<OffscreenTarget>>,
    time: Res<Time<Real>>,
    mut frame: Local<u32>,
    mut taken: Local<bool>,
    mut exit: MessageWriter<AppExit>,
) {
    let (Some(path), Some(target)) = (&args.screenshot, target) else { return };
    *frame += 1;
    if !*taken && *frame >= 3 && time.elapsed_secs() >= args.screenshot_delay {
        *taken = true;
        let _ = std::fs::remove_file(path);
        commands.spawn(Screenshot::image(target.0.clone())).observe(save_to_disk(path.clone()));
    }
    if *taken && path.exists() {
        exit.write(AppExit::Success);
    }
}
