//! Cámara libre para el visor.
//!
//! - Botón derecho del ratón + mover: mirar alrededor
//! - WASD: moverse · Q/E: bajar/subir · Shift: rápido
//!
//! Con `--screenshot`, coloca la cámara, hace una captura y cierra la app.

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};

use crate::Args;

pub struct FlyCameraPlugin;

impl Plugin for FlyCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera)
            .add_systems(Update, (fly, auto_screenshot));
    }
}

#[derive(Component)]
struct FlyCamera {
    yaw: f32,
    pitch: f32,
}

fn spawn_camera(mut commands: Commands, args: Res<Args>) {
    // Por defecto: sobre la ciudad de Lorencia (casilla ~135,128), mirando hacia el norte.
    let [ex, ey, eh, tx, ty, th] = args.cam.unwrap_or([135.0, 112.0, 14.0, 135.0, 130.0, 2.0]);
    // MU (x, y, alto) → Bevy (x, alto, -y)
    let eye = Vec3::new(ex, eh, -ey);
    let target = Vec3::new(tx, th, -ty);
    let transform = Transform::from_translation(eye).looking_at(target, Vec3::Y);
    let (yaw, pitch, _) = transform.rotation.to_euler(EulerRot::YXZ);

    commands.spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: 45f32.to_radians(),
            near: 0.1,
            far: 400.0,
            ..default()
        }),
        transform,
        FlyCamera { yaw, pitch },
    ));
}

fn fly(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    mouse: Res<AccumulatedMouseMotion>,
    mut q: Query<(&mut Transform, &mut FlyCamera)>,
) {
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

/// Espera unos fotogramas a que todo se suba a la GPU, captura y cierra
/// cuando el archivo ya está guardado (o tras ~10 s de espera).
fn auto_screenshot(mut commands: Commands, args: Res<Args>, mut frame: Local<u32>, mut exit: MessageWriter<AppExit>) {
    let Some(path) = &args.screenshot else { return };
    *frame += 1;
    if *frame == 20 {
        let _ = std::fs::remove_file(path);
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path.clone()));
    }
    if *frame > 20 && (path.exists() || *frame > 2000) {
        exit.write(AppExit::Success);
    }
}
