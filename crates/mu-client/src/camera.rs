//! Cámara.
//!
//! - **Seguir** (por defecto): vista en diagonal desde arriba, como en MU. Rueda = zoom.
//! - **Libre**: botón derecho + ratón para mirar, WASD para moverse, Q/E bajar/subir, Shift rápido.
//!
//! `Tab` cambia entre las dos. Con `--cam` se empieza en modo libre en esa posición.
//! Con `--screenshot`, hace una captura al arrancar y cierra la app.

use bevy::camera::RenderTarget;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};

use crate::Args;
use crate::character::Player;

pub struct FlyCameraPlugin;

impl Plugin for FlyCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera)
            .add_systems(Update, (toggle_mode, fly, auto_screenshot))
            // Después de mover al personaje (Update), para que la cámara no vaya un fotograma tarde.
            .add_systems(PostUpdate, follow.before(TransformSystems::Propagate));
    }
}

#[derive(Component)]
struct FlyCamera {
    yaw: f32,
    pitch: f32,
}

#[derive(Component)]
struct FollowCamera {
    /// Distancia al personaje, en casillas.
    distance: f32,
}

#[derive(Resource, PartialEq, Eq, Clone, Copy)]
enum CameraMode {
    Follow,
    Free,
}

/// Imagen en la que dibuja la cámara en modo captura (sin ventana).
#[derive(Resource)]
struct OffscreenTarget(Handle<Image>);

fn spawn_camera(mut commands: Commands, args: Res<Args>, mut images: ResMut<Assets<Image>>) {
    let [ex, ey, eh, tx, ty, th] = args.cam.unwrap_or([135.0, 112.0, 14.0, 135.0, 130.0, 2.0]);
    // MU (x, y, alto) → Bevy (x, alto, -y)
    let eye = Vec3::new(ex, eh, -ey);
    let target = Vec3::new(tx, th, -ty);
    let transform = Transform::from_translation(eye).looking_at(target, Vec3::Y);
    let (yaw, pitch, _) = transform.rotation.to_euler(EulerRot::YXZ);

    commands.insert_resource(if args.cam.is_some() { CameraMode::Free } else { CameraMode::Follow });
    let mut cam = commands.spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: 35f32.to_radians(),
            near: 0.1,
            far: 400.0,
            ..default()
        }),
        transform,
        FlyCamera { yaw, pitch },
        FollowCamera { distance: args.zoom.unwrap_or(12.0) },
        // No usamos luces dinámicas (la luz viene precalculada en el mapa), así que
        // no hace falta repartir luces por zonas de pantalla ("clustering").
        // (`None` falla en Bevy 0.20 con una textura de tamaño cero; `Single` es lo mínimo.)
        bevy::light::cluster::ClusterConfig::Single,
    ));

    if args.screenshot.is_some() {
        let image = images.add(Image::new_target_texture(960, 540, TextureFormat::Rgba8UnormSrgb, None));
        cam.insert(RenderTarget::Image(image.clone().into()));
        commands.insert_resource(OffscreenTarget(image));
    }
}

fn toggle_mode(keys: Res<ButtonInput<KeyCode>>, mut mode: ResMut<CameraMode>, mut q: Query<(&Transform, &mut FlyCamera)>) {
    if keys.just_pressed(KeyCode::Tab) {
        *mode = if *mode == CameraMode::Follow { CameraMode::Free } else { CameraMode::Follow };
        // Al pasar a libre, seguir mirando hacia donde miraba.
        if let Ok((tf, mut fly)) = q.single_mut() {
            let (yaw, pitch, _) = tf.rotation.to_euler(EulerRot::YXZ);
            fly.yaw = yaw;
            fly.pitch = pitch;
        }
    }
}

fn follow(
    mode: Res<CameraMode>,
    scroll: Res<AccumulatedMouseScroll>,
    players: Query<&Transform, (With<Player>, Without<FollowCamera>)>,
    mut cams: Query<(&mut Transform, &mut FollowCamera)>,
) {
    if *mode != CameraMode::Follow {
        return;
    }
    let (Ok(player), Ok((mut tf, mut cam))) = (players.single(), cams.single_mut()) else { return };
    cam.distance = (cam.distance - scroll.delta.y * 0.8).clamp(4.0, 30.0);

    // Como en MU: mirando hacia el noroeste del mapa desde arriba (unos 48° de inclinación).
    let pitch = 48f32.to_radians();
    let yaw = (-45f32).to_radians();
    let back = Quat::from_rotation_y(yaw) * Vec3::new(0.0, pitch.sin(), pitch.cos());
    let target = player.translation + Vec3::Y * 0.9;
    *tf = Transform::from_translation(target + back * cam.distance).looking_at(target, Vec3::Y);
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

/// Espera a que todo esté en la GPU (los shaders se compilan en segundo plano y, sin
/// tarjeta gráfica, tardan), captura y cierra cuando el archivo ya está guardado.
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
