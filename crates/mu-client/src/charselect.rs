//! Selección de personaje (`CHARACTER_SCENE`) del S6: el mapa 75 (`WD_74NEW_CHARACTER_SCENE`, rama
//! `PJH_NEW_SERVER_SELECT_MAP` del Main 5.2) visto con una cámara fija, los personajes de la cuenta
//! en sus huecos, un globo con nombre, gremio, clase y nivel sobre cada uno y la barra de abajo
//! (Crear, Menú, Conectar, Borrar).
//!
//! Sin servidor todavía: la cuenta tiene un solo personaje, un Dark Knight de nivel 1 con el nombre
//! del usuario guardado (`usuario=` de `mu-launcher.txt`) o "mu-rs".
//!
//! - **Cámara** (`MoveMainCamera`, ZzzScene.cpp:1768-1776): ángulos (-84.5, 0, -75) en
//!   (9758.93, 18913.11, 675.5), `CameraFOV` 35, near 20 y alcance 3500 (`g_shCameraLevel` 0,
//!   ZzzScene.cpp:1676-1679) × 1.4. Sin niebla (`FogEnable = false`, ZzzScene.cpp:1043).
//!   El 3D se dibuja solo en la franja (0, 25)-(640, 455) (`BeginOpengl(0,25,640,430)`,
//!   ZzzScene.cpp:1064): aquí la cámara ocupa toda la ventana con el campo de visión agrandado para que
//!   esa franja vea los mismos 35°, y dos bandas negras tapan lo de fuera.
//! - **Personajes** (`ReceiveCharacterList`, WSclient.cpp:497-502): hueco, posición y giro; altura
//!   fija 169.5 (ZzzScene.cpp:1104-1105), escala 1.2 (ZzzCharacter.cpp:11736-11747). De pie con la
//!   espada en la mano (`SetPlayerStop`: fuera de zona segura y con arma de una mano →
//!   `PLAYER_STOP_SWORD`, ZzzCharacter.cpp:300-305), a 0.26 fotogramas por tick (ZzzOpenData.cpp:332).
//!   Luz fija 0.4 (0.5 el del hueco 0, que es `Hero`) más 1 el seleccionado (ZzzScene.cpp:1071-1085,
//!   ZzzCharacter.cpp:8900-8909 y 8986-8990).
//! - **Seleccionado** (ZzzScene.cpp:1077-1150): luz blanca en el suelo, dos marcas `gmmzine` girando
//!   en sentidos contrarios (1.8 y 1.2 casillas, brillo `sin(WorldTime·0.0015)·0.3 + 0.5`) y cada
//!   tick las partículas `BITMAP_EFFECT` 4 (rayo) y 5 (chispas que suben).
//! - **Entrada** (`NewMoveCharacterScene`, ZzzScene.cpp:976-1027): clic en un personaje = elegirlo
//!   (en el vacío, ninguno); doble clic o Enter = entrar; los clics sobre la barra no cuentan.
//! - **Interfaz** (`CUIMng::CreateCharacterScene`, UIMng.cpp:255-300; `CCharSelMainWin`,
//!   `CCharInfoBalloon`): en el original va en píxeles de la ventana sin escalar; aquí, como el resto,
//!   en la pantalla virtual (640×480 a lo alto, `UiScale`), con la barra pegada a los lados.
//! - **Música**: sigue `login_theme`, la del login (ZzzScene.cpp:1220); la para la carga.

use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;
use bevy::text::{FontSize, FontSource};
use bevy::ui::UiScale;

use crate::Args;
use crate::fx::core::{BITMAP_EFFECT, BITMAP_GM_AURORA, DecalDraw, Particle, PLAYER_STOP_SWORD, Spawn};
use crate::fx::material::Blend;
use crate::model_light::ModelMaterial;
use crate::scene::{GameScene, LoadStep, WorldEntity, load_world};
use crate::terrain::{LoadedTerrain, MU_SCALE};

pub struct CharSelectPlugin;

impl Plugin for CharSelectPlugin {
    fn build(&self, app: &mut App) {
        let here = || in_state(GameScene::CharacterSelect);
        app.add_systems(OnEnter(GameScene::CharacterSelect), (setup_camera, spawn_characters, spawn_ui))
            .add_systems(
                OnExit(GameScene::CharacterSelect),
                (restore_camera, crate::scene::despawn::<CharSelectUi>, |mut c: Commands| c.remove_resource::<CharSelect>()),
            )
            .add_systems(Update, (animate, input, music, layout_ui, light_characters).chain().run_if(here()))
            // Donde se coloca la cámara del juego: así manda esta aunque se cambie a la libre (Tab).
            .add_systems(
                PostUpdate,
                place_camera.in_set(crate::camera::CameraMoved).before(TransformSystems::Propagate).run_if(here()),
            )
            .add_systems(FixedUpdate, selected_effects.before(crate::fx::sim::tick).run_if(here()))
            .add_systems(
                PostUpdate,
                aurora.after(crate::fx::draw::collect).before(crate::fx::draw::render_quads).run_if(here()),
            );
    }
}

// --- Cámara ----------------------------------------------------------------------------------------

/// `CameraPosition` y `CameraAngle` de la selección (ZzzScene.cpp:1771-1776).
const CAMERA_EYE: Vec3 = Vec3::new(9758.93, 18913.11, 675.5);
const CAMERA_PITCH: f32 = -84.5;
const CAMERA_YAW: f32 = -75.0;
/// `CameraFOV = 35` (vertical, `gluPerspective`) fuera del paseo del login (ZzzScene.cpp:1552).
const VFOV: f32 = 35.0;
/// `CameraViewNear` (ZzzOpenglUtil.cpp:19: nadie lo cambia) y `CameraViewFar` × 1.4.
const NEAR: f32 = 20.0;
const FAR: f32 = 3500.0 * 1.4;
/// La franja del 3D en la pantalla virtual: de y = 25 a 455 (`BeginOpengl(0,25,640,430)`).
const VIEW_TOP: f32 = 25.0;
const VIEW_HEIGHT: f32 = 430.0;

/// Campo de visión vertical de toda la ventana para que la franja del 3D vea `VFOV`.
fn full_vfov() -> f32 {
    2.0 * ((VFOV / 2.0).to_radians().tan() * 480.0 / VIEW_HEIGHT).atan()
}

fn setup_camera(mut commands: Commands, mut cameras: Query<(Entity, &mut Transform, &mut Projection), With<Camera3d>>) {
    for (e, mut tf, mut proj) in &mut cameras {
        commands.entity(e).remove::<DistanceFog>();
        *tf = crate::camera::mu_view(CAMERA_EYE, CAMERA_PITCH, CAMERA_YAW);
        if let Projection::Perspective(p) = &mut *proj {
            p.fov = full_vfov();
            p.near = NEAR / MU_SCALE;
            p.far = FAR / MU_SCALE;
        }
    }
}

/// La cámara no se mueve: se vuelve a poner cada fotograma (como el original), por si algo la tocó.
fn place_camera(mut cameras: Query<&mut Transform, With<Camera3d>>) {
    let view = crate::camera::mu_view(CAMERA_EYE, CAMERA_PITCH, CAMERA_YAW);
    for mut tf in &mut cameras {
        tf.set_if_neq(view);
    }
}

/// Al salir: el campo de visión y el alcance del juego.
fn restore_camera(mut cameras: Query<&mut Projection, With<Camera3d>>) {
    for mut proj in &mut cameras {
        if let Projection::Perspective(p) = &mut *proj {
            p.fov = crate::camera::mu_fov();
            p.near = 0.2;
            p.far = 400.0;
        }
    }
}

// --- Personajes ------------------------------------------------------------------------------------

/// Huecos de la cuenta: (x, y, giro en grados), WSclient.cpp:497-502.
const SLOTS: [(f32, f32, f32); 5] =
    [(8008.0, 18885.0, 115.0), (7986.0, 19145.0, 90.0), (8046.0, 19400.0, 75.0), (8133.0, 19645.0, 60.0), (8282.0, 19845.0, 35.0)];
/// Altura de los personajes sin montura (ZzzScene.cpp:1105).
const CHARACTER_Z: f32 = 169.5;
/// `Object.Scale` en la selección (ZzzCharacter.cpp:11743).
const CHARACTER_SCALE: f32 = 1.2;
/// `PlaySpeed` de `PLAYER_STOP_SWORD`: fotogramas por tick de 25 Hz (ZzzOpenData.cpp:332).
const IDLE_PLAY_SPEED: f32 = 0.26;
/// Caja para elegir con el ratón: `BoundingBoxMin/Max` de `MODEL_PLAYER` (ZzzCharacter.cpp:11397-11401),
/// sin girar ni escalar, desde los pies.
const PICK_MIN: Vec3 = Vec3::new(-60.0, -60.0, 0.0);
const PICK_MAX: Vec3 = Vec3::new(40.0, 40.0, 120.0);

/// Un personaje de la cuenta (lo que manda la lista del servidor).
struct AccountCharacter {
    slot: usize,
    name: String,
    level: u32,
    /// `GetCharacterClassText` (texto 21 de `text_eng.bmd`).
    class: &'static str,
    /// `GuildStatus`: 255 = sin gremio (`G_NONE`).
    guild_status: u8,
}

/// La lista de personajes sin servidor: un Dark Knight recién creado.
fn offline_characters() -> Vec<AccountCharacter> {
    let name = crate::settings::saved_value("usuario").filter(|n| !n.is_empty()).unwrap_or_else(|| "mu-rs".to_string());
    vec![AccountCharacter { slot: 0, name, level: 1, class: "Dark Knight", guild_status: 255 }]
}

/// Un personaje en su hueco.
#[derive(Component)]
struct SlotCharacter {
    slot: usize,
    name: String,
    level: u32,
    class: &'static str,
    guild_status: u8,
    /// `AnimationFrame` (avanza con el tiempo, sin saltos).
    frame: f32,
    materials: Vec<Handle<ModelMaterial>>,
    /// Luz puesta en los materiales (para no tocarlos si no cambia).
    light: f32,
}

impl SlotCharacter {
    /// Posición en MU (x, y, z).
    fn position(&self) -> Vec3 {
        let (x, y, _) = SLOTS[self.slot];
        Vec3::new(x, y, CHARACTER_Z)
    }
    /// `Angle[2]` en grados.
    fn angle(&self) -> f32 {
        SLOTS[self.slot].2
    }
}

fn spawn_characters(
    mut commands: Commands,
    args: Res<Args>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ModelMaterial>>,
    mut images: ResMut<Assets<Image>>,
    shadow_material: Res<crate::shadow::ShadowMaterial>,
) {
    for c in offline_characters() {
        let Some(model) =
            crate::character::load_dark_knight(&args, PLAYER_STOP_SWORD as usize, &mut meshes, &mut materials, &mut images)
        else {
            return;
        };
        let (x, y, angle) = SLOTS[c.slot];
        let transform = Transform {
            translation: Vec3::new(x, CHARACTER_Z, -y) / MU_SCALE,
            rotation: Quat::from_rotation_x(-FRAC_PI_2) * Quat::from_rotation_z(angle.to_radians()),
            scale: Vec3::splat(CHARACTER_SCALE / MU_SCALE),
        };
        let shadow = crate::shadow::spawn(&mut commands, &mut meshes, &shadow_material);
        let mut e = commands.spawn((
            Name::new(format!("Personaje {} ({})", c.slot, c.name)),
            WorldEntity,
            transform,
            Visibility::default(),
            SlotCharacter {
                slot: c.slot,
                name: c.name,
                level: c.level,
                class: c.class,
                guild_status: c.guild_status,
                frame: 0.0,
                materials: model.materials,
                light: -1.0,
            },
            model.skinned,
            shadow,
        ));
        for child in model.children {
            e.with_child(child);
        }
    }
}

/// Animación de reposo y sombra.
fn animate(
    time: Res<Time>,
    terrain: Option<Res<LoadedTerrain>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut q: Query<(&mut SlotCharacter, &crate::character::Skinned, &crate::shadow::Shadow)>,
    mut visibility: Query<&mut Visibility, Without<SlotCharacter>>,
) {
    for (mut c, skinned, shadow) in &mut q {
        c.frame += IDLE_PLAY_SPEED * crate::fx::sim::TICKS_PER_SECOND * time.delta_secs();
        let mut sh = crate::shadow::Builder::new(
            terrain.as_deref().map(|t| &t.0),
            c.position(),
            Quat::from_rotation_z(c.angle().to_radians()),
            CHARACTER_SCALE,
        );
        // Fuera de zona segura: la espada en la mano.
        skinned.pose(PLAYER_STOP_SWORD as usize, c.frame, false, &mut meshes, &mut sh);
        sh.finish(shadow, &mut meshes, visibility.get_mut(shadow.1).ok());
    }
}

/// `PlaceCharactersForDraw`: luz 0.4 (0.5 el `Hero`, hueco 0) y +1 el seleccionado.
fn light_characters(
    sel: Option<Res<CharSelect>>,
    mut q: Query<&mut SlotCharacter>,
    mut materials: ResMut<Assets<ModelMaterial>>,
) {
    let selected = sel.as_ref().and_then(|s| s.selected);
    for mut c in &mut q {
        let base = if c.slot == 0 { 0.5 } else { 0.4 };
        let light = base + if selected == Some(c.slot) { 1.0 } else { 0.0 };
        if c.light == light {
            continue;
        }
        c.light = light;
        for h in &c.materials {
            if let Some(mut m) = materials.get_mut(h) {
                m.extension.light = Vec4::new(light, light, light, 1.0);
            }
        }
    }
}

/// Cada tick, del seleccionado: luz blanca en el suelo (`AddTerrainLight(..., 1)`) y las partículas
/// `BITMAP_EFFECT` 4 y 5 con luz 0.15 (ZzzScene.cpp:1077-1083, 1147-1150).
fn selected_effects(
    sel: Option<Res<CharSelect>>,
    q: Query<&SlotCharacter>,
    mut world: ResMut<crate::fx::FxWorld>,
    mut lights: ResMut<crate::terrain::TerrainLights>,
) {
    let Some(slot) = sel.and_then(|s| s.selected) else { return };
    let Some(c) = q.iter().find(|c| c.slot == slot) else { return };
    let pos = c.position();
    lights.add(pos.truncate() / MU_SCALE, [1.0; 3], 1.0);
    let angle = Vec3::new(0.0, 0.0, c.angle());
    for sub_type in [4, 5] {
        crate::fx::request(&mut world, Spawn::Particle(Particle::new(BITMAP_EFFECT, pos, angle, Vec3::splat(0.15), sub_type, 1.0, None)));
    }
}

/// Las dos marcas `BITMAP_GM_AURORA` (`Skill/gmmzine`) bajo el seleccionado, sumando
/// (`RenderTerrainAlphaBitmap`, ZzzScene.cpp:1130-1139). El original gira la textura: aquí se gira
/// el cuadrado al revés, que es lo mismo.
fn aurora(
    sel: Option<Res<CharSelect>>,
    time: Res<Time>,
    q: Query<&SlotCharacter>,
    mut frame: ResMut<crate::fx::draw::FrameDraw>,
) {
    let Some(slot) = sel.and_then(|s| s.selected) else { return };
    let Some(c) = q.iter().find(|c| c.slot == slot) else { return };
    // `WorldTime` en milisegundos.
    let world_time = time.elapsed_secs() * 1000.0;
    let lumi = (world_time * 0.0015).sin() * 0.3 + 0.5;
    for (size, rotation) in [(1.8, world_time * 0.01), (1.2, -world_time * 0.01)] {
        frame.0.decals.push(DecalDraw {
            texture: BITMAP_GM_AURORA,
            center: c.position().truncate(),
            width: size * MU_SCALE,
            height: size * MU_SCALE,
            color: [lumi, lumi, lumi, 1.0],
            rotation: -rotation,
            // `Height` por defecto de `RenderTerrainAlphaBitmap`.
            lift: 5.0,
            blend: Blend::Glow,
        });
    }
}

// --- Interfaz --------------------------------------------------------------------------------------

/// Estado de la pantalla.
#[derive(Resource, Default)]
struct CharSelect {
    /// `SelectedHero`: hueco elegido.
    selected: Option<usize>,
    /// `SelectedCharacter`: hueco bajo el cursor.
    hovered: Option<usize>,
    /// `CButton::m_pBtnHeld`: botón pulsado y aún no soltado.
    held: Option<CsButton>,
    /// `CInput`: hora (s) del último clic y si puede ser el primero de un doble clic.
    last_press: f32,
    former_down: bool,
    /// Ya se ha pedido entrar al juego.
    starting: bool,
}

#[derive(Component)]
struct CharSelectUi;

/// Botones de `CCharSelMainWin`.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
enum CsButton {
    Create,
    Menu,
    Connect,
    Delete,
}

impl CsButton {
    const ALL: [CsButton; 4] = [CsButton::Create, CsButton::Menu, CsButton::Connect, CsButton::Delete];

    /// Textura (`OpenCharacterSceneData`, ZzzOpenData.cpp:4908-4911).
    fn texture(self) -> &'static str {
        match self {
            CsButton::Create => "b_create.tga",
            CsButton::Menu => "server_menu_b_all.tga",
            CsButton::Connect => "b_connect.tga",
            CsButton::Delete => "b_delete.tga",
        }
    }

    /// Fotograma de deshabilitado (`CButton::Create`, CharSelMainWin.cpp:41-44): el de Menú no
    /// tiene (siempre está habilitado).
    fn disabled_frame(self) -> Option<f32> {
        (self != CsButton::Menu).then_some(3.0)
    }
}

/// Fotogramas de los botones (`CButton::Create`: arriba normal, 1 encima, 2 pulsado, 3 deshabilitado).
const BTN_W: f32 = 54.0;
const BTN_H: f32 = 30.0;
/// Globo de `character_ex` (118×54) con el punto de anclaje abajo en el centro (CharInfoBalloon.cpp:25).
const BALLOON_W: f32 = 118.0;
const BALLOON_H: f32 = 54.0;
/// Altura sobre los pies del punto del globo (CharInfoBalloon.cpp:43).
const BALLOON_LIFT: f32 = 350.0;
/// `g_hFixFont`: 13 − 1 = 12 px (Winmain.cpp:1464-1474), cara `GlobalText[18]` = "Gulim".
const FONT_PX: f32 = 12.0;
/// `CLRDW_BR_ORANGE` (UIBaseDef.h:15).
const BR_ORANGE: Color = Color::srgb(255.0 / 255.0, 217.0 / 255.0, 39.0 / 255.0);
/// Fondo de la barra de información: negro con alfa 143 (CharSelMainWin.cpp:37-38).
const INFO_ALPHA: f32 = 143.0 / 255.0;

/// Partes de la interfaz que se colocan cada fotograma.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum UiPiece {
    TopBand,
    BottomBand,
    Deco,
    Info,
    Button(CsButton),
    Balloon(usize),
}

/// Colocación de `CCharSelMainWin` en una pantalla virtual de `size` (UIMng.cpp:284-285,
/// CharSelMainWin.cpp:32-48 y 74-88). Ancho de la ventana: 4 botones + la barra de información
/// (`ancho − 266`) + 6 = `ancho − 44`, en x = 22 e `y = 567/600 · alto − 30 − 11`.
struct Layout {
    size: Vec2,
    /// Rectángulo de toda la ventana (`CWin`: lo que cuenta como "ratón sobre la interfaz").
    window: Rect,
    deco: Rect,
    info: Rect,
}

impl Layout {
    fn new(size: Vec2) -> Self {
        let y = (567.0 / 600.0 * size.y).floor() - BTN_H - 11.0;
        let window = Rect::new(22.0, y, size.x - 22.0, y + BTN_H);
        // `SetPosition`: Menú a la derecha de Crear (+1), la barra a +2 de Menú y 5 más abajo; Borrar
        // pegado a la derecha y Conectar a su izquierda (−1); el adorno asoma 22 por la derecha y 59 por arriba.
        let info_x = 22.0 + 2.0 * BTN_W + 1.0 + 2.0;
        Self {
            size,
            window,
            deco: Rect::new(window.max.x - (189.0 - 22.0), y - 59.0, window.max.x + 22.0, y - 59.0 + 103.0),
            info: Rect::new(info_x, y + 5.0, info_x + (size.x - 266.0), y + 5.0 + 21.0),
        }
    }

    fn button(&self, b: CsButton) -> Rect {
        let (x0, y) = (self.window.min.x, self.window.min.y);
        let x = match b {
            CsButton::Create => x0,
            CsButton::Menu => x0 + BTN_W + 1.0,
            CsButton::Connect => self.window.max.x - (BTN_W * 2.0 + 1.0),
            CsButton::Delete => self.window.max.x - BTN_W,
        };
        Rect::new(x, y, x + BTN_W, y + BTN_H)
    }

    /// Alto de cada banda negra fuera de la franja del 3D.
    fn band(&self) -> f32 {
        VIEW_TOP * self.size.y / 480.0
    }
}

/// Tamaño de la pantalla virtual: la ventana entre `UiScale` (640×480 a lo alto en 4:3).
fn virtual_size(window: Option<&Window>, scale: f32) -> Vec2 {
    match window {
        Some(w) if w.width() >= 1.0 && w.height() >= 1.0 && scale > 0.0 => Vec2::new(w.width(), w.height()) / scale,
        _ => Vec2::new(640.0, 480.0),
    }
}

/// La letra de la interfaz (`fonts.rs`: Noto Sans en lugar de la Gulim del original).
fn load_font(_fonts: &mut Assets<Font>) -> FontSource {
    FontSource::default()
}

/// La fuente, cargada una sola vez.
#[derive(Resource, Clone)]
struct UiFont(FontSource);

/// Nodo con la parte `src` de una imagen de `Data/Interface` (transparente si falta el archivo).
fn image_piece(image: Option<(Handle<Image>, Vec2)>, src: Rect) -> impl Bundle {
    let node = Node { position_type: PositionType::Absolute, ..default() };
    let img = match image {
        Some((h, _)) => ImageNode { rect: Some(src), ..ImageNode::new(h) },
        None => ImageNode { color: Color::NONE, ..default() },
    };
    (node, img)
}

/// Una línea del globo centrada en `width` desde `left` (`RenderText(..., RT3_SORT_CENTER)`).
fn balloon_line(font: &FontSource, text: String, color: Color, left: f32, top: f32, width: f32) -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(left),
            top: Val::Px(top),
            width: Val::Px(width),
            justify_content: JustifyContent::Center,
            ..default()
        },
        children![(
            Text::new(text),
            TextFont { font: font.clone(), font_size: FontSize::Px(FONT_PX), ..default() },
            TextColor(color),
            TextLayout::no_wrap(),
        )],
    )
}

/// `CCharInfoBalloon::SetInfo` (CharInfoBalloon.cpp:84-124): nombre, gremio y "clase nivel".
fn guild_text(status: u8) -> &'static str {
    match status {
        255 => "(Commoner)",
        0 => "(Members)",
        32 => "(Battle M.)",
        64 => "(Assist. M.)",
        128 => "(Master)",
        _ => "()",
    }
}

fn spawn_ui(
    mut commands: Commands,
    args: Res<Args>,
    mut images: ResMut<Assets<Image>>,
    mut fonts: ResMut<Assets<Font>>,
    font: Option<Res<UiFont>>,
    characters: Query<&SlotCharacter>,
) {
    commands.insert_resource(CharSelect::default());
    let font = match font {
        Some(f) => f.0.clone(),
        None => {
            let f = load_font(&mut fonts);
            commands.insert_resource(UiFont(f.clone()));
            f
        }
    };
    let mut load = |n: &str| crate::scene::interface_image(&args, &mut images, n);
    let balloon = load("character_ex.tga");
    let deco = load("deco.tga");
    let buttons: Vec<_> = CsButton::ALL.iter().map(|b| (*b, load(b.texture()))).collect();

    let band = || (Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), ..default() }, BackgroundColor(Color::BLACK));
    let mut root = commands.spawn((
        Name::new("Selección de personaje"),
        CharSelectUi,
        Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
    ));
    root.with_children(|r| {
        // Fuera de la franja del 3D (`BeginOpengl(0,25,640,430)`) solo se ve el borrado negro.
        r.spawn((band(), UiPiece::TopBand));
        r.spawn((band(), UiPiece::BottomBand));
        // Los globos, por debajo de las ventanas (`CUIMng::Render`, UIMng.cpp:719).
        for c in &characters {
            r.spawn((image_piece(balloon.clone(), Rect::new(0.0, 0.0, BALLOON_W, BALLOON_H)), UiPiece::Balloon(c.slot)))
                .with_children(|b| {
                    // `RenderText` a +7, +23 y +39; la clase en un hueco 10 más a la izquierda y 30 más ancho.
                    b.spawn(balloon_line(&font, c.name.clone(), Color::WHITE, 0.0, 7.0, BALLOON_W));
                    b.spawn(balloon_line(&font, guild_text(c.guild_status).to_string(), Color::WHITE, 0.0, 23.0, BALLOON_W));
                    b.spawn(balloon_line(&font, format!("{} {}", c.class, c.level), BR_ORANGE, -10.0, 39.0, BALLOON_W + 30.0));
                });
        }
        // `RenderControls`: primero el adorno y la barra, luego los botones.
        r.spawn((image_piece(deco, Rect::new(0.0, 0.0, 189.0, 103.0)), UiPiece::Deco));
        r.spawn((
            Node { position_type: PositionType::Absolute, ..default() },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, INFO_ALPHA)),
            UiPiece::Info,
        ));
        for (b, img) in buttons {
            r.spawn((image_piece(img, Rect::new(0.0, 0.0, BTN_W, BTN_H)), UiPiece::Button(b), b));
        }
    });
}

/// Pone cada pieza en su sitio (la barra según el tamaño de la ventana, los globos sobre los personajes).
#[allow(clippy::type_complexity)]
fn layout_ui(
    windows: Query<&Window>,
    scale: Res<UiScale>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    characters: Query<&SlotCharacter>,
    mut pieces: Query<(&UiPiece, &mut Node, &mut Visibility)>,
) {
    let layout = Layout::new(virtual_size(windows.single().ok(), scale.0));
    let camera = cameras.single().ok();
    let place = |node: &mut Node, r: Rect| {
        let (l, t, w, h) = (Val::Px(r.min.x), Val::Px(r.min.y), Val::Px(r.width()), Val::Px(r.height()));
        if node.left != l || node.top != t || node.width != w || node.height != h {
            node.left = l;
            node.top = t;
            node.width = w;
            node.height = h;
        }
    };
    for (piece, mut node, mut vis) in &mut pieces {
        match *piece {
            UiPiece::TopBand => place(&mut node, Rect::new(0.0, 0.0, layout.size.x, layout.band())),
            UiPiece::BottomBand => place(&mut node, Rect::new(0.0, layout.size.y - layout.band(), layout.size.x, layout.size.y)),
            UiPiece::Deco => place(&mut node, layout.deco),
            UiPiece::Info => place(&mut node, layout.info),
            UiPiece::Button(b) => place(&mut node, layout.button(b)),
            UiPiece::Balloon(slot) => {
                // `Projection` del punto 350 por encima de los pies; esquina = punto − (59, 54).
                let anchor = characters.iter().find(|c| c.slot == slot).and_then(|c| {
                    let (cam, gt) = camera?;
                    let p = c.position() + Vec3::Z * BALLOON_LIFT;
                    let px = cam.world_to_viewport(gt, Vec3::new(p.x, p.z, -p.y) / MU_SCALE).ok()?;
                    Some((px / scale.0).floor())
                });
                match anchor {
                    Some(a) => {
                        let min = a - Vec2::new(BALLOON_W / 2.0, BALLOON_H);
                        place(&mut node, Rect::from_corners(min, min + Vec2::new(BALLOON_W, BALLOON_H)));
                        vis.set_if_neq(Visibility::Inherited);
                    }
                    None => {
                        vis.set_if_neq(Visibility::Hidden);
                    }
                }
            }
        }
    }
}

/// Rayo (en MU) bajo el cursor contra la caja de cada personaje: el más cercano a la cámara
/// (`SelectCharacter`, ZzzInterface.cpp:7704-7792).
fn pick(camera: (&Camera, &GlobalTransform), cursor: Vec2, characters: &Query<&SlotCharacter>) -> Option<usize> {
    let (cam, gt) = camera;
    let ray = cam.viewport_to_world(gt, cursor).ok()?;
    let to_mu = |v: Vec3| Vec3::new(v.x, -v.z, v.y);
    let origin = to_mu(ray.origin) * MU_SCALE;
    let dir = to_mu(*ray.direction);
    let eye = to_mu(gt.translation()) * MU_SCALE;
    let mut best: Option<(f32, usize)> = None;
    for c in characters {
        let pos = c.position();
        if !ray_hits_box(origin, dir, pos + PICK_MIN, pos + PICK_MAX) {
            continue;
        }
        let d = (pos - eye).length_squared();
        if best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, c.slot));
        }
    }
    best.map(|(_, s)| s)
}

/// Corte de un rayo (hacia delante) con una caja alineada con los ejes.
fn ray_hits_box(origin: Vec3, dir: Vec3, min: Vec3, max: Vec3) -> bool {
    let (mut t0, mut t1) = (0.0f32, f32::INFINITY);
    for k in 0..3 {
        if dir[k].abs() < 1e-8 {
            if origin[k] < min[k] || origin[k] > max[k] {
                return false;
            }
            continue;
        }
        let (a, b) = ((min[k] - origin[k]) / dir[k], (max[k] - origin[k]) / dir[k]);
        t0 = t0.max(a.min(b));
        t1 = t1.min(a.max(b));
        if t0 > t1 {
            return false;
        }
    }
    true
}

/// Ratón y teclado: botones (`CButton::Update`), elegir con clic, doble clic y Enter para entrar.
#[allow(clippy::too_many_arguments)]
fn input(
    windows: Query<&Window>,
    scale: Res<UiScale>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time<Real>>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    characters: Query<&SlotCharacter>,
    mut buttons: Query<(&CsButton, &mut ImageNode)>,
    sel: Option<ResMut<CharSelect>>,
    mut sound: ResMut<crate::sound::SoundQueue>,
    mut args: ResMut<Args>,
    mut load: ResMut<LoadStep>,
    mut next: ResMut<NextState<GameScene>>,
) {
    let Some(mut sel) = sel else { return };
    let window = windows.single().ok();
    let layout = Layout::new(virtual_size(window, scale.0));
    let cursor_px = window.and_then(|w| w.cursor_position());
    let cursor = cursor_px.map(|c| c / scale.0);
    let down = mouse.just_pressed(MouseButton::Left);
    let up = mouse.just_released(MouseButton::Left);

    // `CInput`: un clic es doble si llega antes de `GetDoubleClickTime` (500 ms) tras el anterior.
    let mut double = false;
    if down {
        let now = time.elapsed_secs();
        double = sel.former_down && now - sel.last_press <= DOUBLE_CLICK_SECS;
        sel.former_down = !double;
        sel.last_press = now;
    }

    // `CCharSelMainWin::UpdateDisplay`: Crear si queda algún hueco libre; Conectar y Borrar con
    // un personaje elegido; Menú siempre.
    let free_slot = characters.iter().count() < SLOTS.len();
    let chosen = sel.selected.is_some();
    let enabled = |b: CsButton| match b {
        CsButton::Create => free_slot,
        CsButton::Menu => true,
        CsButton::Connect | CsButton::Delete => chosen,
    };

    // Botones: se agarra al pulsar encima y cuenta al soltar encima del mismo.
    let inside = |b: CsButton| cursor.is_some_and(|c| layout.button(b).contains(c));
    let mut clicked = None;
    for b in CsButton::ALL {
        if !enabled(b) {
            if up && sel.held == Some(b) {
                sel.held = None;
            }
            continue;
        }
        if inside(b) && down {
            sel.held = Some(b);
        }
        if up {
            if inside(b) && sel.held == Some(b) {
                clicked = Some(b);
                sound.play("SOUND_CLICK01");
            }
            if sel.held == Some(b) {
                sel.held = None;
            }
        }
    }
    for (b, mut img) in &mut buttons {
        let frame = if !enabled(*b) {
            b.disabled_frame().unwrap_or(0.0)
        } else if inside(*b) && sel.held.is_none() {
            1.0
        } else if inside(*b) && sel.held == Some(*b) {
            2.0
        } else {
            0.0
        };
        let rect = Rect::new(0.0, frame * BTN_H, BTN_W, frame * BTN_H + BTN_H);
        if img.rect != Some(rect) {
            img.rect = Some(rect);
        }
    }
    // `CCharSelMainWin::UpdateWhileActive`.
    match clicked {
        Some(CsButton::Connect) => start_game(&mut sel, &mut args, &mut load, &mut next, &mut sound),
        // Menú (`m_SysMenuWin`), Crear (`m_CharMakeWin`) y Borrar (`DeleteCharacter`): sin hacer todavía.
        Some(b) => info!("selección de personaje: botón {b:?} sin hacer todavía"),
        None => {}
    }

    // `SelectObjects`: solo con el ratón fuera de la interfaz y por encima de y = 429.
    let on_ui = cursor.is_some_and(|c| layout.window.contains(c));
    sel.hovered = match (cursor_px, cursor, cameras.single()) {
        (Some(px), Some(c), Ok(cam)) if !on_ui && c.y < 429.0 * layout.size.y / 480.0 => pick(cam, px, &characters),
        _ => None,
    };

    if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter) {
        if sel.selected.is_some() {
            sound.play("SOUND_CLICK01");
            if let Some(h) = sel.hovered {
                sel.selected = Some(h);
            }
            start_game(&mut sel, &mut args, &mut load, &mut next, &mut sound);
        }
        return;
    }
    if on_ui {
        return;
    }
    if down && double {
        if let Some(h) = sel.hovered {
            sel.selected = Some(h);
            start_game(&mut sel, &mut args, &mut load, &mut next, &mut sound);
        }
    } else if down {
        sel.selected = sel.hovered;
    }
}

/// `PlayMp3(login_theme)` del login: sigue sonando aquí (salvo al entrar al juego, que se corta).
fn music(sel: Option<Res<CharSelect>>, mut sound: ResMut<crate::sound::SoundQueue>) {
    if !sel.is_some_and(|s| s.starting) {
        sound.music("login_theme");
    }
}

/// `GetDoubleClickTime` por defecto de Windows.
const DOUBLE_CLICK_SECS: f32 = 0.5;

/// `StartGame` (ZzzScene.cpp:854-871): con un personaje elegido, a la carga del juego (Lorencia).
/// `LoadingScene` para la música del login (ZzzScene.cpp:1503).
fn start_game(
    sel: &mut CharSelect,
    args: &mut Args,
    load: &mut LoadStep,
    next: &mut NextState<GameScene>,
    sound: &mut crate::sound::SoundQueue,
) {
    if sel.starting || sel.selected.is_none() {
        return;
    }
    sel.starting = true;
    sound.stop_music();
    load_world(args, load, 1, true, GameScene::Game);
    next.set(GameScene::Loading);
}
