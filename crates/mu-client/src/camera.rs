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
use crate::terrain::MU_SCALE;

pub struct FlyCameraPlugin;

impl Plugin for FlyCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera)
            .add_systems(Update, (toggle_mode, fly, auto_screenshot))
            // Después de mover al personaje (Update), para que la cámara no vaya un fotograma tarde.
            .add_systems(PostUpdate, follow.in_set(CameraMoved).before(TransformSystems::Propagate));
    }
}

/// Cuándo queda colocada la cámara en cada fotograma (para lo que se orienta hacia ella).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct CameraMoved;

#[derive(Component)]
struct FlyCamera {
    yaw: f32,
    pitch: f32,
}

#[derive(Component)]
struct FollowCamera {
    /// Distancia al personaje, en casillas (escalón de zoom del original / 100).
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
            fov: mu_fov(),
            near: 0.2,
            far: 400.0,
            ..default()
        }),
        transform,
        FlyCamera { yaw, pitch },
        FollowCamera { distance: args.zoom.unwrap_or(ZOOM_DEFAULT) },
        // No usamos luces dinámicas (la luz viene precalculada en el mapa), así que
        // no hace falta repartir luces por zonas de pantalla ("clustering").
        // (`None` falla en Bevy 0.20 con una textura de tamaño cero; `Single` es lo mínimo.)
        bevy::light::cluster::ClusterConfig::Single,
    ));

    if args.screenshot.is_some() {
        // 4:3 como las resoluciones del juego, y con la interfaz dibujada en la captura.
        let image = images.add(Image::new_target_texture(1024, 768, TextureFormat::Rgba8UnormSrgb, None));
        cam.insert((RenderTarget::Image(image.clone().into()), IsDefaultUiCamera));
        commands.insert_resource(bevy::ui::UiScale(768.0 / 480.0));
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
    // Zoom por escalones de 100 unidades de MU (1000…1700 en el original).
    if scroll.delta.y != 0.0 {
        cam.distance = (cam.distance - scroll.delta.y.signum()).clamp(ZOOM_MIN, ZOOM_MAX);
    }
    *tf = mu_camera(player.translation, cam.distance * MU_SCALE);
}

/// Escalones de zoom del original (`PLAYER_ZOOM_LADDER`), en casillas: 10…17, por defecto 13.
const ZOOM_MIN: f32 = 10.0;
const ZOOM_MAX: f32 = 17.0;
pub const ZOOM_DEFAULT: f32 = 13.0;

/// Campo de visión vertical del original: 40° horizontales a 4:3 (`HFovToVFov`), unos 30.5°.
pub fn mu_fov() -> f32 {
    let h_half = 20f32.to_radians();
    2.0 * (h_half.tan() / (4.0 / 3.0)).atan()
}

/// Cámara del juego tal como la calcula el original (`DefaultCamera::Update`):
///
/// 1. Con los ángulos base (0, 0, -45°) se calcula la posición: `héroe + AngleMatrixᵀ·(0, -distancia, 0)`
///    en horizontal, y en altura `héroe.z + distancia - 150`.
/// 2. Después `SetCameraAngle` pone la inclinación de -48.5°, que solo afecta a la vista
///    (`Ry(a1)·Rx(a0)·Rz(a2)` al estilo OpenGL).
///
/// Resultado: la cámara queda al sureste del personaje mirando al noroeste, inclinada 41.5°
/// hacia abajo, y el personaje sale justo en el centro de la pantalla.
/// `hero` va en coordenadas de Bevy; `distance` en unidades de MU.
fn mu_camera(hero: Vec3, distance: f32) -> Transform {
    let (s0, c0) = (-48.5f32).to_radians().sin_cos();
    let (s2, c2) = (-45f32).to_radians().sin_cos();
    // En coordenadas de MU (x, y, z arriba):
    let forward = Vec3::new(-s0 * s2, -s0 * c2, -c0);
    let up = Vec3::new(c0 * s2, c0 * c2, -s0);
    // Posición con inclinación 0 (paso 1): solo gira el yaw.
    let offset = Vec3::new(-s2 * distance, -c2 * distance, distance - 150.0);
    // MU (x, y, z) → Bevy (x, z, -y), 100 unidades por casilla.
    let to_bevy = |v: Vec3| Vec3::new(v.x, v.z, -v.y);
    Transform::from_translation(hero + to_bevy(offset) / MU_SCALE).looking_to(to_bevy(forward), to_bevy(up))
}

/// Vista del original desde `eye` (MU) con inclinación `pitch` y giro `yaw` en grados (los mismos ejes
/// que [`mu_camera`]: `Ry(a1)·Rx(a0)·Rz(a2)` al estilo OpenGL).
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
