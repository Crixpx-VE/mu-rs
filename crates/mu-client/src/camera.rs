//! Cámara.
//!
//! - **Seguir** (la del juego): vista en diagonal desde arriba. La rueda (o Ctrl + rueda, como
//!   `SetViewPortLevel`, ZzzScene.cpp:493) cambia `g_shCameraLevel` de 0 a 4: distancia de 1000 a 1400,
//!   con su alcance (`CameraViewFar`) y su recorte del suelo (`CreateFrustrum2D`). La distancia se
//!   acerca un tercio cada tick (`CameraDistance += (Target - CameraDistance) / 3`). El Main 5.2
//!   pone el nivel a 0 en cada fotograma, así que en el original no se puede alejar; aquí sí.
//! - **Libre** (solo para probar, con `--cam`): botón derecho + ratón para mirar, WASD para moverse,
//!   Q/E bajar/subir, Shift rápido.
//! Con `--screenshot`, hace una captura al arrancar y cierra la app.

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
            // Después de mover al personaje (Update), para que la cámara no vaya un fotograma tarde.
            .add_systems(PostUpdate, follow.in_set(CameraMoved).before(TransformSystems::Propagate))
            .add_systems(
                PostUpdate,
                game_far_plane.after(CameraMoved).run_if(in_state(crate::scene::GameScene::Game)),
            );
    }
}

/// `battleCastle::InBattleCastle2` (GMBattleCastle.h:11): el castillo del Valle de Loren, en MU.
pub fn in_battle_castle2(p: Vec2) -> bool {
    (16100.0..=19000.0).contains(&p.x) && (18900.0..=21700.0).contains(&p.y)
}

/// `g_shCameraLevel` (0 a 4) que ha elegido el jugador con la rueda.
static CAMERA_LEVEL: AtomicU8 = AtomicU8::new(0);
/// El `static CameraLevel` del recorte del suelo (ZzzLodTerrain.cpp:2010-2013): solo sigue al nivel
/// cuando la cámara ya no está más lejos que su objetivo (al acercarse, mantiene el recorte grande
/// hasta llegar).
static CULL_LEVEL: AtomicU8 = AtomicU8::new(0);

/// Nivel de cámara en un mapa: Vulcanus y los Doppelganger 1 y 2 lo fuerzan a 5
/// (ZzzScene.cpp:1721-1729); en el resto, el de la rueda.
fn level_in(world: u16) -> u8 {
    match world as i32 - 1 {
        63 | 65 | 66 => 5,
        _ => CAMERA_LEVEL.load(Ordering::Relaxed),
    }
}

/// Nivel de cámara para el recorte del suelo (terrain.rs).
pub fn cull_level(world: u16) -> u8 {
    match world as i32 - 1 {
        63 | 65 | 66 => 5,
        _ => CULL_LEVEL.load(Ordering::Relaxed),
    }
}

/// `CameraDistanceTarget` del nivel, en MU (ZzzScene.cpp:1844-1852). Con nivel 5 el original usa
/// `g_Direction.m_fCameraViewFar` (de los eventos); sin ellos, 1000.
fn distance_target(level: u8) -> f32 {
    match level {
        1 => 1100.0,
        2 => 1200.0,
        3 => 1300.0,
        4 => 1400.0,
        _ => 1000.0,
    }
}

/// `CameraViewFar` del juego en MU (`MoveMainCamera`, ZzzScene.cpp:1652-1698), según el mapa y el
/// `g_shCameraLevel`. `hero` es la posición del personaje en MU.
///
/// La comprobación de `InBattleCastle2` va sin mirar el mapa, como en el original.
pub fn view_far(world: u16, hero: Vec2) -> f32 {
    match world as i32 - 1 {
        _ if in_battle_castle2(hero) => 3000.0,
        // Valle de Loren (`InBattleCastle() && MAIN_SCENE`).
        30 => 2500.0,
        // Elbeland (`WD_51HOME_6TH_CHAR`).
        51 => 2800.0 * 1.15,
        // Vulcanus y Doppelganger 2.
        63 | 66 => 3700.0,
        // Doppelganger 1: nivel 5 → `case 4/5`.
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

/// `SetViewPortLevel`: la rueda sube o baja el nivel (hacia ti acerca, hacia fuera aleja), fuera de
/// la interfaz.
fn zoom_wheel(
    scroll: Res<bevy::input::mouse::AccumulatedMouseScroll>,
    over_ui: Res<crate::mainframe::PointerOverUi>,
    game_ui: Res<crate::options::GameUi>,
    windows: Query<&Window>,
) {
    if scroll.delta.y == 0.0 || over_ui.0 {
        return;
    }
    // Sobre una ventana del juego (las opciones, con su barra de volumen) la rueda es de la ventana.
    let on_window =
        windows.single().ok().and_then(crate::mainframe::cursor_virtual).is_some_and(|c| game_ui.blocks(c));
    if on_window {
        return;
    }
    let level = CAMERA_LEVEL.load(Ordering::Relaxed) as i32;
    let next = if scroll.delta.y > 0.0 { level - 1 } else { level + 1 };
    CAMERA_LEVEL.store(next.clamp(0, 4) as u8, Ordering::Relaxed);
}

/// Alcance de la cámara del juego: `gluPerspective(..., CameraViewNear, CameraViewFar · 1.4)`
/// (`BeginOpengl`, ZzzOpenglUtil.cpp:643). Lo que queda más lejos no se dibuja y se ve el fondo.
fn game_far_plane(
    args: Res<Args>,
    players: Query<&Transform, (With<Player>, Without<FollowCamera>)>,
    mut cams: Query<&mut Projection, With<FollowCamera>>,
) {
    let hero = players.single().map_or(Vec2::ZERO, |p| Vec2::new(p.translation.x, -p.translation.z) * MU_SCALE);
    let far = view_far(args.world, hero) * 1.4 / MU_SCALE;
    for mut proj in &mut cams {
        // Sin tocarla si ya está bien (cambiarla rehace el frustum).
        if matches!(&*proj, Projection::Perspective(p) if p.far != far)
            && let Projection::Perspective(p) = &mut *proj
        {
            p.far = far;
        }
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
        // Colores tal cual, como el OpenGL del original: sin el `TonyMcMapface` que Bevy pone por
        // defecto, que aclara las sombras y quita contraste.
        bevy::core_pipeline::tonemapping::Tonemapping::None,
        bevy::core_pipeline::tonemapping::DebandDither::Disabled,
        // Sin el antialiasing 4× que Bevy pone por defecto: el original no tiene, y cuesta mucho
        // en tarjetas modestas.
        Msaa::Off,
        Projection::Perspective(PerspectiveProjection {
            fov: mu_fov(),
            near: 0.2,
            // Fuera del juego (login, selección) cada pantalla pone el suyo; en el juego, `game_far_plane`.
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
        // La interfaz va en esta cámara aunque haya otras (la vista previa de crear personaje).
        IsDefaultUiCamera,
    ));

    if args.screenshot.is_some() {
        // 4:3 como las resoluciones del juego, y con la interfaz dibujada en la captura.
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
    // `--zoom` deja la distancia fija; si no, la del nivel, acercándose un tercio por tick (25 Hz).
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

/// `CameraDistance` del juego en casillas (1000 unidades de MU, `g_shCameraLevel` 0).
pub const ZOOM_DEFAULT: f32 = 10.0;

/// Campo de visión vertical del juego: `CameraFOV = 35` (`MoveMainCamera` del Main 5.2, en
/// `gluPerspective`). MuMain lo cambió a 40° en horizontal (unos 30.5° en vertical).
pub fn mu_fov() -> f32 {
    35f32.to_radians()
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
