//! S6 character selection (`CHARACTER_SCENE`): map 75 (`WD_74NEW_CHARACTER_SCENE`, Main 5.2
//! `PJH_NEW_SERVER_SELECT_MAP` branch) seen from a fixed camera, the account's characters in their
//! slots, a balloon with name, guild, class and level above each, and the bottom bar
//! (Create, Menu, Connect, Delete).
//!
//! - **Camera** (`MoveMainCamera`, ZzzScene.cpp:1768-1776): angles (-84.5, 0, -75) at
//!   (9758.93, 18913.11, 675.5), `CameraFOV` 35, near 20 and range 3500 (`g_shCameraLevel` 0,
//!   ZzzScene.cpp:1676-1679) × 1.4. No fog (`FogEnable = false`, ZzzScene.cpp:1043).
//!   3D is drawn only in the strip (0, 25)-(640, 455) (`BeginOpengl(0,25,640,430)`,
//!   ZzzScene.cpp:1064): here the camera covers the whole window with a wider field of view so that
//!   strip sees the same 35°, and two black bands cover the rest.
//! - **Characters** (`ReceiveCharacterList`, WSclient.cpp:497-502): slot, position and rotation; fixed
//!   height 169.5 (ZzzScene.cpp:1104-1105), scale 1.2 (ZzzCharacter.cpp:11736-11747). Standing with
//!   the sword in hand (`SetPlayerStop`: outside a safe zone with a one-handed weapon →
//!   `PLAYER_STOP_SWORD`, ZzzCharacter.cpp:300-305), at 0.26 frames per tick (ZzzOpenData.cpp:332).
//!   Fixed light 0.4 (0.5 for slot 0, which is `Hero`) plus 1 for the selected one (ZzzScene.cpp:1071-1085,
//!   ZzzCharacter.cpp:8900-8909 and 8986-8990).
//! - **Selected** (ZzzScene.cpp:1077-1150): white light on the ground, two `gmmzine` marks spinning
//!   in opposite directions (1.8 and 1.2 tiles, brightness `sin(WorldTime·0.0015)·0.3 + 0.5`) and every
//!   tick the `BITMAP_EFFECT` particles 4 (bolt) and 5 (rising sparks).
//! - **Input** (`NewMoveCharacterScene`, ZzzScene.cpp:976-1027): click a character = select it
//!   (empty space, none); double click or Enter = enter; clicks on the bar do not count.
//! - **UI** (`CUIMng::CreateCharacterScene`, UIMng.cpp:255-300; `CCharSelMainWin`,
//!   `CCharInfoBalloon`): the original uses unscaled window pixels; here, like the rest, the virtual
//!   screen (640×480 by height, `UiScale`), with the bar stretched to the sides.
//! - **Music**: keeps `login_theme`, the login one (ZzzScene.cpp:1220); loading stops it.

use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;
use bevy::text::{FontSize, FontSource};
use bevy::ui::UiScale;

use crate::Args;
use crate::account::{Class, StoredCharacter};
use crate::fx::core::{BITMAP_EFFECT, BITMAP_GM_AURORA, DecalDraw, Particle, Spawn};
use crate::fx::material::Blend;
use crate::model_light::ModelMaterial;
use crate::scene::{GameScene, LoadStep, WorldEntity, load_world};
use crate::terrain::{LoadedTerrain, MU_SCALE};

pub struct CharSelectPlugin;

/// This screen's systems: they run before the window manager (login_ui.rs), so it receives the
/// bar buttons in the same frame and does not share with the bar the Enter or click that closes
/// a window.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct CharSelectSystems;

impl Plugin for CharSelectPlugin {
    fn build(&self, app: &mut App) {
        let here = || in_state(GameScene::CharacterSelect);
        app.add_systems(OnEnter(GameScene::CharacterSelect), (setup_camera, spawn_characters, spawn_ui))
            .add_systems(
                OnExit(GameScene::CharacterSelect),
                (restore_camera, crate::scene::despawn::<CharSelectUi>, |mut c: Commands| {
                    c.remove_resource::<CharSelect>();
                    c.remove_resource::<CharSceneLink>();
                    c.remove_resource::<BalloonAssets>();
                }),
            )
            .add_systems(
                Update,
                (net_messages, scene_events, sync_balloons, animate, input, music, layout_ui, light_characters, preview)
                    .chain()
                    .in_set(CharSelectSystems)
                    .run_if(here()),
            )
            // Where the game camera is placed, so this one wins.
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

// --- Camera ----------------------------------------------------------------------------------------

/// Selection `CameraPosition` and `CameraAngle` (ZzzScene.cpp:1771-1776).
const CAMERA_EYE: Vec3 = Vec3::new(9758.93, 18913.11, 675.5);
const CAMERA_PITCH: f32 = -84.5;
const CAMERA_YAW: f32 = -75.0;
/// `CameraFOV = 35` (vertical, `gluPerspective`) outside the login fly-through (ZzzScene.cpp:1552).
const VFOV: f32 = 35.0;
/// `CameraViewNear` (ZzzOpenglUtil.cpp:19: never changed) and `CameraViewFar` × 1.4.
const NEAR: f32 = 20.0;
const FAR: f32 = 3500.0 * 1.4;
/// The 3D strip on the virtual screen: y = 25 to 455 (`BeginOpengl(0,25,640,430)`).
const VIEW_TOP: f32 = 25.0;
const VIEW_HEIGHT: f32 = 430.0;

/// Vertical field of view of the whole window so the 3D strip sees `VFOV`.
fn full_vfov() -> f32 {
    2.0 * ((VFOV / 2.0).to_radians().tan() * 480.0 / VIEW_HEIGHT).atan()
}

fn setup_camera(mut commands: Commands, mut cameras: Query<(Entity, &mut Transform, &mut Projection), (With<Camera3d>, With<IsDefaultUiCamera>, Without<PreviewCamera>)>) {
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

/// The camera does not move: it is reset every frame (like the original) in case something touched it.
fn place_camera(mut cameras: Query<&mut Transform, (With<Camera3d>, With<IsDefaultUiCamera>, Without<PreviewCamera>)>) {
    let view = crate::camera::mu_view(CAMERA_EYE, CAMERA_PITCH, CAMERA_YAW);
    for mut tf in &mut cameras {
        tf.set_if_neq(view);
    }
}

/// On exit: restores the game field of view and range.
fn restore_camera(mut cameras: Query<&mut Projection, (With<Camera3d>, With<IsDefaultUiCamera>, Without<PreviewCamera>)>) {
    for mut proj in &mut cameras {
        if let Projection::Perspective(p) = &mut *proj {
            p.fov = crate::camera::mu_fov();
            p.near = 0.2;
            p.far = 400.0;
        }
    }
}

// --- Characters ------------------------------------------------------------------------------------

/// Account slots: (x, y, rotation in degrees), WSclient.cpp:497-502.
const SLOTS: [(f32, f32, f32); 5] =
    [(8008.0, 18885.0, 115.0), (7986.0, 19145.0, 90.0), (8046.0, 19400.0, 75.0), (8133.0, 19645.0, 60.0), (8282.0, 19845.0, 35.0)];
/// Height of unmounted characters (ZzzScene.cpp:1105).
const CHARACTER_Z: f32 = 169.5;
/// Mouse picking box: `MODEL_PLAYER` `BoundingBoxMin/Max` (ZzzCharacter.cpp:11397-11401),
/// unrotated and unscaled, from the feet.
const PICK_MIN: Vec3 = Vec3::new(-60.0, -60.0, 0.0);
const PICK_MAX: Vec3 = Vec3::new(40.0, 40.0, 120.0);

/// A character in its slot.
#[derive(Component)]
struct SlotCharacter {
    slot: usize,
    name: String,
    level: u32,
    class: Class,
    guild_status: u8,
    /// `SetPlayerStop`: idle animation.
    action: usize,
    /// `AnimationFrame` (advances with time, without jumps).
    frame: f32,
    materials: Vec<Handle<ModelMaterial>>,
    /// Light set on the materials (to avoid touching them when unchanged).
    light: f32,
}

impl SlotCharacter {
    /// Position in MU (x, y, z).
    fn position(&self) -> Vec3 {
        let (x, y, _) = SLOTS[self.slot];
        Vec3::new(x, y, CHARACTER_Z)
    }
    /// `Angle[2]` in degrees.
    fn angle(&self) -> f32 {
        SLOTS[self.slot].2
    }
}

/// On enter: requests the character list (`SendRequestCharacterList`); it arrives in `net_messages`.
fn spawn_characters(mut commands: Commands, mut net: ResMut<crate::net::Net>) {
    commands.insert_resource(CharSceneLink { account: crate::account::current_account(), ..default() });
    net.send(mu_protocol::game::ClientMessage::RequestCharacterList { language: 0 });
}

/// Server messages: the list (`ReceiveCharacterList`) and, on connect, the character information
/// (`ReceiveJoinMapServer`), which leads to the character's map.
#[allow(clippy::too_many_arguments)]
fn net_messages(
    mut commands: Commands,
    mut from: MessageReader<crate::net::FromServer>,
    mut args: ResMut<Args>,
    mut load: ResMut<LoadStep>,
    mut next: ResMut<NextState<GameScene>>,
    sel: Option<ResMut<CharSelect>>,
    existing: Query<(Entity, &crate::shadow::Shadow), With<SlotCharacter>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ModelMaterial>>,
    mut images: ResMut<Assets<Image>>,
    shadow_material: Res<crate::shadow::ShadowMaterial>,
) {
    use mu_protocol::game::{self, ServerMessage};
    for m in from.read() {
        match &m.0 {
            ServerMessage::CharacterList { characters, .. } => {
                for (e, shadow) in &existing {
                    commands.entity(e).despawn();
                    commands.entity(shadow.1).despawn();
                }
                for c in characters {
                    let c = StoredCharacter {
                        slot: c.slot as usize,
                        name: c.name.clone(),
                        class: Class::from_index(game::base_class_index(c.class)),
                        level: c.level as u32,
                        guild_status: c.guild_position,
                    };
                    if c.slot < SLOTS.len() {
                        spawn_slot_character(&mut commands, &args, &mut meshes, &mut materials, &mut images, &shadow_material, &c);
                    }
                }
            }
            ServerMessage::CharacterInformation(info) => {
                if !sel.as_ref().is_some_and(|s| s.starting) {
                    continue;
                }
                commands.insert_resource(crate::account::HeroInfo(info.clone()));
                // `MapId` is `WorldActive`: the folder is the next number.
                load_world(&mut args, &mut load, info.map as u16 + 1, true, GameScene::Game);
                next.set(GameScene::Loading);
            }
            _ => {}
        }
    }
}

/// An account character in its slot (`CreateCharacterPointer` from `ReceiveCharacterList`).
fn spawn_slot_character(
    commands: &mut Commands,
    args: &Args,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ModelMaterial>,
    images: &mut Assets<Image>,
    shadow_material: &crate::shadow::ShadowMaterial,
    c: &StoredCharacter,
) -> Option<Entity> {
    // Selection is not a safe zone: the Dark Knight holds the sword in hand.
    let action = crate::character::stop_action(c.class, c.class == Class::Knight);
    let model = crate::character::load_character(args, c.class, action, meshes, materials, images)?;
    let (x, y, angle) = SLOTS[c.slot];
    let transform = Transform {
        translation: Vec3::new(x, CHARACTER_Z, -y) / MU_SCALE,
        rotation: Quat::from_rotation_x(-FRAC_PI_2) * Quat::from_rotation_z(angle.to_radians()),
        scale: Vec3::splat(c.class.select_scale() / MU_SCALE),
    };
    let shadow = crate::shadow::spawn(commands, meshes, shadow_material);
    let mut e = commands.spawn((
        Name::new(format!("Personaje {} ({})", c.slot, c.name)),
        WorldEntity,
        transform,
        Visibility::default(),
        SlotCharacter {
            slot: c.slot,
            name: c.name.clone(),
            level: c.level,
            class: c.class,
            guild_status: c.guild_status,
            action,
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
    Some(e.id())
}

/// Idle animation and shadow.
fn animate(
    time: Res<Time>,
    terrain: Option<Res<LoadedTerrain>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut q: Query<(&mut SlotCharacter, &crate::character::Skinned, &crate::shadow::Shadow)>,
    mut visibility: Query<&mut Visibility, Without<SlotCharacter>>,
) {
    for (mut c, skinned, shadow) in &mut q {
        c.frame += crate::character::play_speed(c.action) * crate::fx::sim::TICKS_PER_SECOND * time.delta_secs();
        let mut sh = crate::shadow::Builder::new(
            terrain.as_deref().map(|t| &t.0),
            c.position(),
            Quat::from_rotation_z(c.angle().to_radians()),
            c.class.select_scale(),
        );
        // Outside a safe zone: sword in hand.
        skinned.pose(c.action, c.frame, false, &mut meshes, &mut sh);
        sh.finish(shadow, &mut meshes, visibility.get_mut(shadow.1).ok());
    }
}

/// `PlaceCharactersForDraw`: light 0.4 (0.5 for `Hero`, slot 0) and +1 for the selected one.
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

/// Every tick, for the selected one: white ground light (`AddTerrainLight(..., 1)`) and the
/// `BITMAP_EFFECT` 4 and 5 particles with light 0.15 (ZzzScene.cpp:1077-1083, 1147-1150).
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

/// The two `BITMAP_GM_AURORA` marks (`Skill/gmmzine`) under the selected one, additive
/// (`RenderTerrainAlphaBitmap`, ZzzScene.cpp:1130-1139). The original rotates the texture; here the
/// quad is rotated the opposite way, which is equivalent.
fn aurora(
    sel: Option<Res<CharSelect>>,
    time: Res<Time>,
    q: Query<&SlotCharacter>,
    mut frame: ResMut<crate::fx::draw::FrameDraw>,
) {
    let Some(slot) = sel.and_then(|s| s.selected) else { return };
    let Some(c) = q.iter().find(|c| c.slot == slot) else { return };
    // `WorldTime` in milliseconds.
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
            // Default `Height` of `RenderTerrainAlphaBitmap`.
            lift: 5.0,
            blend: Blend::Glow,
        });
    }
}

// --- Create-window preview -------------------------------------------------------------------------

/// Layer seen only by the preview camera.
const PREVIEW_LAYER: usize = 1;

#[derive(Component)]
struct PreviewCamera;

/// Face of the chosen class (`CharacterView` with `MODEL_FACE + class` = `Logo/NewFaceNN.bmd`).
#[derive(Component)]
struct PreviewFace {
    choice: (Class, u32),
    model: mu_data::bmd::Bmd,
    meshes: Vec<(usize, Handle<Mesh>)>,
    /// Action 1 once (when the class is chosen), then action 0 looping.
    action: usize,
    frame: f32,
}

/// `RenderCreateCharacter` (CharMakeWin.cpp:388-445): rotation, scale and offset of each face.
/// The Rage Fighter branch (from MuMain) sets no rotation: the previous face's rotation stays.
fn face_pose(class: Class) -> (Option<Vec3>, f32, Vec3) {
    match class {
        Class::Wizard => (Some(Vec3::new(0.0, 0.0, -40.0)), 5.9, Vec3::ZERO),
        Class::Knight => (Some(Vec3::new(0.0, 0.0, -12.0)), 6.05, Vec3::ZERO),
        Class::Elf => (Some(Vec3::new(8.0, 0.0, 5.0)), 9.1, Vec3::new(4.8, 0.0, 0.0)),
        Class::Gladiator => (Some(Vec3::new(8.0, 0.0, -13.0)), 6.0, Vec3::new(0.0, 0.0, 1.8)),
        Class::Lord => (Some(Vec3::new(8.0, 0.0, -18.0)), 6.0, Vec3::ZERO),
        Class::Summoner => (Some(Vec3::new(2.0, 0.0, 2.0)), 9.1, Vec3::new(4.8, 0.0, 4.0)),
        Class::Fighter => (None, 6.0, Vec3::new(9.8, 0.0, -7.5)),
    }
}

/// `CharacterAnimation` with `PlaySpeed` 0.3 (Summoner action 0: 0.25), × 2 because it is a face
/// and × 2 again because it is animated twice per tick while the window is active.
fn face_keys_per_tick(class: Class, action: usize) -> f32 {
    let speed = if class == Class::Summoner && action == 0 { 0.25 } else { 0.3 };
    speed * 2.0 * 2.0
}

/// The camera (`MoveCharacterCamera` at (10, -500, 48), angle (-90, 0, 0), `CameraFOV` 10)
/// renders the face into a transparent image that the create window shows in place.
#[allow(clippy::too_many_arguments)]
fn preview(
    mut commands: Commands,
    args: Res<Args>,
    time: Res<Time>,
    link: Option<ResMut<CharSceneLink>>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ModelMaterial>>,
    mut cameras: Query<(Entity, &mut Camera), With<PreviewCamera>>,
    mut faces: Query<(Entity, &mut PreviewFace)>,
    mut last_angle: Local<Vec3>,
) {
    use bevy::camera::RenderTarget;
    use bevy::camera::visibility::RenderLayers;
    use bevy::render::render_resource::TextureFormat;
    let Some(mut link) = link else { return };
    let size = link.preview_size.max(UVec2::ONE);
    // The image, sized like the on-screen preview.
    let image_ok = link.preview_image.as_ref().and_then(|h| images.get(h)).is_some_and(|i| i.size() == size);
    if !image_ok {
        let image = images.add(Image::new_target_texture(size.x, size.y, TextureFormat::Rgba8UnormSrgb, None));
        link.preview_image = Some(image.clone());
        for (e, _) in &cameras {
            commands.entity(e).despawn();
        }
        commands.spawn((
            Name::new("Cámara de la vista previa"),
            PreviewCamera,
            WorldEntity,
            Camera3d::default(),
            Msaa::Off,
            Camera { order: -1, clear_color: ClearColorConfig::Custom(Color::NONE), is_active: false, ..default() },
            RenderTarget::Image(image.into()),
            bevy::core_pipeline::tonemapping::Tonemapping::None,
            bevy::core_pipeline::tonemapping::DebandDither::Disabled,
            bevy::light::cluster::ClusterConfig::Single,
            Projection::Perspective(PerspectiveProjection {
                fov: 10f32.to_radians(),
                near: NEAR / MU_SCALE,
                far: 2000.0 / MU_SCALE,
                ..default()
            }),
            crate::camera::mu_view(Vec3::new(10.0, -500.0, 48.0), -90.0, 0.0),
            RenderLayers::layer(PREVIEW_LAYER),
        ));
        return;
    }
    let shown = link.preview;
    for (_, mut cam) in &mut cameras {
        if cam.is_active != shown.is_some() {
            cam.is_active = shown.is_some();
        }
    }
    let Some(choice) = shown else {
        for (e, _) in &faces {
            commands.entity(e).despawn();
        }
        return;
    };

    // Another class (or the same one chosen again): new face, starting at action 1.
    if faces.iter().any(|(_, f)| f.choice != choice) || faces.is_empty() {
        for (e, _) in &faces {
            commands.entity(e).despawn();
        }
        let class = choice.0;
        let dir = args.data.join("Logo");
        let name = format!("NewFace{:02}.bmd", class.index() + 1);
        let model = match mu_data::fs::read(&dir, &name).and_then(|b| mu_data::bmd::read(&b)) {
            Ok(m) => m,
            Err(e) => {
                warn!("vista previa: falta Logo/{name}: {e}");
                return;
            }
        };
        let bones = model.bone_matrices_at(1, 0.0);
        let (angle, scale, offset) = face_pose(class);
        // `CharacterView.Object.Angle` starts at (0, 0, 0).
        let angle = angle.unwrap_or(*last_angle);
        *last_angle = angle;
        let [ax, ay, az] = angle.to_array().map(f32::to_radians);
        let to_bevy = Quat::from_rotation_x(-FRAC_PI_2);
        let transform = Transform {
            translation: to_bevy * offset / MU_SCALE,
            rotation: to_bevy * Quat::from_euler(EulerRot::ZYX, az, ay, ax),
            scale: Vec3::splat(scale / MU_SCALE),
        };
        let mut handles = Vec::new();
        let mut children = Vec::new();
        for i in 0..model.meshes.len() {
            let posed = model.pose_with(i, &bones);
            if posed.positions.is_empty() {
                continue;
            }
            let mesh = meshes.add(
                Mesh::new(bevy::mesh::PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::default())
                    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, posed.positions)
                    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, posed.normals)
                    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, posed.uvs),
            );
            let mut mat = crate::character::part_material(&dir, &model.meshes[i].texture, &mut images);
            // `Draw_RenderObject` draws faces with `BodyLight` (4.8, 4.8, 4.8).
            mat.extension.light = Vec4::new(4.8, 4.8, 4.8, 1.0);
            children.push((Mesh3d(mesh.clone()), MeshMaterial3d(materials.add(mat)), RenderLayers::layer(PREVIEW_LAYER)));
            handles.push((i, mesh));
        }
        let mut e = commands.spawn((
            Name::new(format!("Vista previa {}", class.name())),
            WorldEntity,
            transform,
            Visibility::default(),
            RenderLayers::layer(PREVIEW_LAYER),
            PreviewFace { choice, model, meshes: handles, action: 1, frame: 0.0 },
        ));
        for c in children {
            e.with_child(c);
        }
        return;
    }

    for (_, mut f) in &mut faces {
        let class = f.choice.0;
        f.frame += face_keys_per_tick(class, f.action) * crate::fx::sim::TICKS_PER_SECOND * time.delta_secs();
        let frames = f.model.actions.get(f.action).map_or(1, |a| a.frames.max(1)) as f32;
        if f.frame >= frames {
            // `CharacterAnimation` returns false when it wraps: switch to action 0.
            f.frame -= frames;
            if f.action == 1 {
                f.action = 0;
            }
            let n = f.model.actions.first().map_or(1, |a| a.frames.max(1)) as f32;
            f.frame = f.frame.rem_euclid(n);
        }
        let bones = f.model.bone_matrices_at(f.action, f.frame);
        for (i, h) in &f.meshes {
            let posed = f.model.pose_with(*i, &bones);
            if let Some(mut m) = meshes.get_mut(h) {
                m.insert_attribute(Mesh::ATTRIBUTE_POSITION, posed.positions);
                m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, posed.normals);
            }
        }
    }
}

// --- Link with the windows (login_ui.rs) -----------------------------------------------------------

/// What this screen (3D, bar and balloons) and the login_ui.rs window manager (create character,
/// messages, system menu and options) tell each other; in the original they all belong to `CUIMng`.
#[derive(Resource, Default)]
pub struct CharSceneLink {
    pub account: String,
    /// Characters on the account.
    pub count: usize,
    /// `SelectedHero`: slot, name and `GuildStatus`.
    pub selected: Option<(usize, String, u8)>,
    /// A full-screen window is open: this screen's bar, characters and keyboard do not respond.
    pub blocked: bool,
    /// Bar buttons for the windows.
    pub to_ui: Vec<UiRequest>,
    /// What the windows did.
    pub to_scene: Vec<SceneEvent>,
    /// Create-window preview (`RenderCreateCharacter`) while shown: the class and how many times
    /// one has been chosen (on change, the animation restarts).
    pub preview: Option<(Class, u32)>,
    /// Preview size in real pixels (410×335 of the reference window).
    pub preview_size: UVec2,
    /// Image it is drawn into (created by this screen; shown by the create window).
    pub preview_image: Option<Handle<Image>>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UiRequest {
    /// Menu: `m_SysMenuWin`.
    SysMenu,
    /// Create: `m_CharMakeWin`.
    Create,
    /// Delete: `DeleteCharacter` (the warnings and the confirmation).
    Delete,
}

#[derive(Clone, Debug)]
pub enum SceneEvent {
    /// Successful `ReceiveCreateCharacter`.
    Created(StoredCharacter),
    /// Successful `ReceiveDeleteCharacter`.
    Deleted(String),
}

// --- UI --------------------------------------------------------------------------------------------

/// Screen state.
#[derive(Resource, Default)]
struct CharSelect {
    /// `SelectedHero`: selected slot.
    selected: Option<usize>,
    /// `SelectedCharacter`: slot under the cursor.
    hovered: Option<usize>,
    /// `CButton::m_pBtnHeld`: button pressed and not yet released.
    held: Option<CsButton>,
    /// `CInput`: time (s) of the last click and whether it can be the first of a double click.
    last_press: f32,
    former_down: bool,
    /// Entering the game has already been requested.
    starting: bool,
}

#[derive(Component)]
struct CharSelectUi;

/// `CCharSelMainWin` buttons.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
enum CsButton {
    Create,
    Menu,
    Connect,
    Delete,
}

impl CsButton {
    const ALL: [CsButton; 4] = [CsButton::Create, CsButton::Menu, CsButton::Connect, CsButton::Delete];

    /// Texture (`OpenCharacterSceneData`, ZzzOpenData.cpp:4908-4911).
    fn texture(self) -> &'static str {
        match self {
            CsButton::Create => "b_create.tga",
            CsButton::Menu => "server_menu_b_all.tga",
            CsButton::Connect => "b_connect.tga",
            CsButton::Delete => "b_delete.tga",
        }
    }

    /// Disabled frame (`CButton::Create`, CharSelMainWin.cpp:41-44): Menu has none (it is always
    /// enabled).
    fn disabled_frame(self) -> Option<f32> {
        (self != CsButton::Menu).then_some(3.0)
    }
}

/// Button frames (`CButton::Create`: top normal, 1 hover, 2 pressed, 3 disabled).
const BTN_W: f32 = 54.0;
const BTN_H: f32 = 30.0;
/// `character_ex` balloon (118×54) anchored at the bottom center (CharInfoBalloon.cpp:25).
const BALLOON_W: f32 = 118.0;
const BALLOON_H: f32 = 54.0;
/// Height of the balloon anchor above the feet (CharInfoBalloon.cpp:43).
const BALLOON_LIFT: f32 = 350.0;
/// `g_hFixFont`: 13 − 1 = 12 px (Winmain.cpp:1464-1474), face `GlobalText[18]` = "Gulim".
const FONT_PX: f32 = 12.0;
/// `CLRDW_BR_ORANGE` (UIBaseDef.h:15).
const BR_ORANGE: Color = Color::srgb(255.0 / 255.0, 217.0 / 255.0, 39.0 / 255.0);
/// Info bar background: black with alpha 143 (CharSelMainWin.cpp:37-38).
const INFO_ALPHA: f32 = 143.0 / 255.0;

/// UI pieces placed every frame.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum UiPiece {
    TopBand,
    BottomBand,
    Deco,
    Info,
    Button(CsButton),
    Balloon(usize),
}

/// `CCharSelMainWin` layout on a virtual screen of `size` (UIMng.cpp:284-285,
/// CharSelMainWin.cpp:32-48 and 74-88). Window width: 4 buttons + the info bar
/// (`width − 266`) + 6 = `width − 44`, at x = 22 and `y = 567/600 · height − 30 − 11`.
struct Layout {
    size: Vec2,
    /// Rectangle of the whole window (`CWin`: what counts as "mouse over the UI").
    window: Rect,
    deco: Rect,
    info: Rect,
}

impl Layout {
    fn new(size: Vec2) -> Self {
        let y = (567.0 / 600.0 * size.y).floor() - BTN_H - 11.0;
        let window = Rect::new(22.0, y, size.x - 22.0, y + BTN_H);
        // `SetPosition`: Menu right of Create (+1), the bar +2 after Menu and 5 lower; Delete flush
        // right and Connect to its left (−1); the decoration sticks out 22 on the right and 59 above.
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

    /// Height of each black band outside the 3D strip.
    fn band(&self) -> f32 {
        VIEW_TOP * self.size.y / 480.0
    }
}

/// UI units (480-high virtual screen) per original pixel: windows are laid out in pixels of a
/// 1024×768 reference window, like the login ones (login_ui.rs).
pub const K: f32 = 480.0 / 768.0;

/// Screen size in reference-window pixels (the window divided by `UiScale` and `K`).
fn virtual_size(window: Option<&Window>, scale: f32) -> Vec2 {
    let ui = match window {
        Some(w) if w.width() >= 1.0 && w.height() >= 1.0 && scale > 0.0 => Vec2::new(w.width(), w.height()) / scale,
        _ => Vec2::new(640.0, 480.0),
    };
    (ui / K).round()
}

/// UI font (`fonts.rs`: Noto Sans instead of the original Gulim).
fn load_font(_fonts: &mut Assets<Font>) -> FontSource {
    FontSource::default()
}

/// The font, loaded only once.
#[derive(Resource, Clone)]
struct UiFont(FontSource);

/// Node showing the `src` part of a `Data/Interface` image (transparent if the file is missing).
fn image_piece(image: Option<(Handle<Image>, Vec2)>, src: Rect) -> impl Bundle {
    let node = Node { position_type: PositionType::Absolute, ..default() };
    let img = match image {
        Some((h, _)) => ImageNode { rect: Some(src), ..ImageNode::new(h) },
        None => ImageNode { color: Color::NONE, ..default() },
    };
    (node, img)
}

/// A balloon line centered in `width` from `left` (`RenderText(..., RT3_SORT_CENTER)`).
fn balloon_line(font: &FontSource, text: String, color: Color, left: f32, top: f32, width: f32) -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(left * K),
            top: Val::Px(top * K),
            width: Val::Px(width * K),
            justify_content: JustifyContent::Center,
            ..default()
        },
        children![(
            Text::new(text),
            TextFont {
                font: font.clone(),
                font_size: FontSize::Px(FONT_PX * K),
                font_smoothing: bevy::text::FontSmoothing::AntiAliased,
                ..default()
            },
            TextColor(color),
            TextLayout::no_wrap(),
        )],
    )
}

/// `CCharInfoBalloon::SetInfo` (CharInfoBalloon.cpp:84-124): name, guild and "class level".
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
        // Outside the 3D strip (`BeginOpengl(0,25,640,430)`) only the black clear shows.
        r.spawn((band(), UiPiece::TopBand));
        r.spawn((band(), UiPiece::BottomBand));
        // `RenderControls`: decoration and bar first, then the buttons. Above the balloons
        // (`CUIMng::Render`, UIMng.cpp:719), which are added with the characters.
        r.spawn((image_piece(deco, Rect::new(0.0, 0.0, 189.0, 103.0)), UiPiece::Deco, ZIndex(1)));
        r.spawn((
            Node { position_type: PositionType::Absolute, ..default() },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, INFO_ALPHA)),
            UiPiece::Info,
            ZIndex(1),
        ));
        for (b, img) in buttons {
            r.spawn((image_piece(img, Rect::new(0.0, 0.0, BTN_W, BTN_H)), UiPiece::Button(b), b, ZIndex(1)));
        }
    });
    commands.insert_resource(BalloonAssets { image: balloon, font });
}

/// Balloon texture and font.
#[derive(Resource)]
struct BalloonAssets {
    image: Option<(Handle<Image>, Vec2)>,
    font: FontSource,
}

/// One balloon (`CCharInfoBalloon`) per character: appears on creation and goes away on deletion.
fn sync_balloons(
    mut commands: Commands,
    assets: Option<Res<BalloonAssets>>,
    root: Query<Entity, With<CharSelectUi>>,
    characters: Query<&SlotCharacter>,
    balloons: Query<(Entity, &UiPiece)>,
) {
    let (Some(assets), Ok(root)) = (assets, root.single()) else { return };
    let have: Vec<(Entity, usize)> =
        balloons.iter().filter_map(|(e, p)| if let UiPiece::Balloon(s) = p { Some((e, *s)) } else { None }).collect();
    for &(e, slot) in &have {
        if !characters.iter().any(|c| c.slot == slot) {
            commands.entity(e).despawn();
        }
    }
    for c in &characters {
        if have.iter().any(|&(_, s)| s == c.slot) {
            continue;
        }
        let font = &assets.font;
        let balloon = commands
            .spawn((
                image_piece(assets.image.clone(), Rect::new(0.0, 0.0, BALLOON_W, BALLOON_H)),
                UiPiece::Balloon(c.slot),
                ZIndex(0),
                Visibility::Hidden,
            ))
            .with_children(|b| {
                // `RenderText` at +7, +23 and +39; the class in a box 10 further left and 30 wider.
                b.spawn(balloon_line(font, c.name.clone(), Color::WHITE, 0.0, 7.0, BALLOON_W));
                b.spawn(balloon_line(font, guild_text(c.guild_status).to_string(), Color::WHITE, 0.0, 23.0, BALLOON_W));
                b.spawn(balloon_line(font, format!("{} {}", c.class.name(), c.level), BR_ORANGE, -10.0, 39.0, BALLOON_W + 30.0));
            })
            .id();
        commands.entity(root).add_child(balloon);
    }
}

/// Places each piece (the bar by window size, the balloons above the characters).
#[allow(clippy::type_complexity)]
fn layout_ui(
    windows: Query<&Window>,
    scale: Res<UiScale>,
    cameras: Query<(&Camera, &GlobalTransform), (With<Camera3d>, With<IsDefaultUiCamera>, Without<PreviewCamera>)>,
    characters: Query<&SlotCharacter>,
    mut pieces: Query<(&UiPiece, &mut Node, &mut Visibility)>,
) {
    let layout = Layout::new(virtual_size(windows.single().ok(), scale.0));
    let camera = cameras.single().ok();
    let place = |node: &mut Node, r: Rect| {
        let (l, t, w, h) = (Val::Px(r.min.x * K), Val::Px(r.min.y * K), Val::Px(r.width() * K), Val::Px(r.height() * K));
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
                // `Projection` of the point 350 above the feet; corner = point − (59, 54).
                let anchor = characters.iter().find(|c| c.slot == slot).and_then(|c| {
                    let (cam, gt) = camera?;
                    let p = c.position() + Vec3::Z * BALLOON_LIFT;
                    let px = cam.world_to_viewport(gt, Vec3::new(p.x, p.z, -p.y) / MU_SCALE).ok()?;
                    Some((px / scale.0 / K).floor())
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

/// Ray (in MU) under the cursor against each character's box: the closest to the camera wins
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

/// Forward ray vs. axis-aligned box intersection.
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

/// Mouse and keyboard: buttons (`CButton::Update`), click to select, double click or Enter to enter.
#[allow(clippy::too_many_arguments)]
fn input(
    windows: Query<&Window>,
    scale: Res<UiScale>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time<Real>>,
    cameras: Query<(&Camera, &GlobalTransform), (With<Camera3d>, With<IsDefaultUiCamera>, Without<PreviewCamera>)>,
    characters: Query<&SlotCharacter>,
    mut buttons: Query<(&CsButton, &mut ImageNode)>,
    sel: Option<ResMut<CharSelect>>,
    link: Option<ResMut<CharSceneLink>>,
    mut commands: Commands,
    mut sound: ResMut<crate::sound::SoundQueue>,
    mut net: ResMut<crate::net::Net>,
) {
    let (Some(mut sel), Some(mut link)) = (sel, link) else { return };
    let window = windows.single().ok();
    let layout = Layout::new(virtual_size(window, scale.0));
    // With a full-screen window in front, nothing on this screen responds (`CUIMng` only
    // handles the buttons of the window under the mouse).
    let blocked = link.blocked;
    let cursor_px = window.and_then(|w| w.cursor_position()).filter(|_| !blocked);
    let cursor = cursor_px.map(|c| c / scale.0 / K);
    let down = mouse.just_pressed(MouseButton::Left) && !blocked;
    let up = mouse.just_released(MouseButton::Left);

    // `CInput`: a click is a double click if it comes within `GetDoubleClickTime` (500 ms) of the previous one.
    let mut double = false;
    if down {
        let now = time.elapsed_secs();
        double = sel.former_down && now - sel.last_press <= DOUBLE_CLICK_SECS;
        sel.former_down = !double;
        sel.last_press = now;
    }

    // `CCharSelMainWin::UpdateDisplay`: Create if a slot is free; Connect and Delete with a
    // character selected; Menu always.
    let free_slot = characters.iter().count() < SLOTS.len();
    let chosen = sel.selected.is_some();
    let enabled = |b: CsButton| match b {
        CsButton::Create => free_slot,
        CsButton::Menu => true,
        CsButton::Connect | CsButton::Delete => chosen,
    };

    // Buttons: grabbed on press over it, triggered on release over the same one.
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
        Some(CsButton::Connect) => start_game(&mut sel, &characters, &mut commands, &mut net, &mut sound),
        Some(CsButton::Menu) => link.to_ui.push(UiRequest::SysMenu),
        Some(CsButton::Create) => link.to_ui.push(UiRequest::Create),
        Some(CsButton::Delete) => link.to_ui.push(UiRequest::Delete),
        None => {}
    }

    // `SelectObjects`: only with the mouse outside the UI and above y = 429.
    let on_ui = cursor.is_some_and(|c| layout.window.contains(c));
    sel.hovered = match (cursor_px, cursor, cameras.single()) {
        (Some(px), Some(c), Ok(cam)) if !on_ui && c.y < 429.0 * layout.size.y / 480.0 => pick(cam, px, &characters),
        _ => None,
    };

    if !blocked && (keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter)) {
        if sel.selected.is_some() {
            sound.play("SOUND_CLICK01");
            if let Some(h) = sel.hovered {
                sel.selected = Some(h);
            }
            start_game(&mut sel, &characters, &mut commands, &mut net, &mut sound);
        }
    } else if !on_ui && down && double {
        if let Some(h) = sel.hovered {
            sel.selected = Some(h);
            start_game(&mut sel, &characters, &mut commands, &mut net, &mut sound);
        }
    } else if !on_ui && down {
        sel.selected = sel.hovered;
    }
    link.count = characters.iter().count();
    link.selected = sel
        .selected
        .and_then(|s| characters.iter().find(|c| c.slot == s))
        .map(|c| (c.slot, c.name.clone(), c.guild_status));
}

/// What the windows did: a created character appears in its slot and becomes selected; a deleted
/// one disappears (`ReceiveCreateCharacter` / `ReceiveDeleteCharacter`).
#[allow(clippy::too_many_arguments)]
fn scene_events(
    mut commands: Commands,
    args: Res<Args>,
    link: Option<ResMut<CharSceneLink>>,
    sel: Option<ResMut<CharSelect>>,
    characters: Query<(Entity, &SlotCharacter, &crate::shadow::Shadow)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ModelMaterial>>,
    mut images: ResMut<Assets<Image>>,
    shadow_material: Res<crate::shadow::ShadowMaterial>,
) {
    let (Some(mut link), Some(mut sel)) = (link, sel) else { return };
    for ev in std::mem::take(&mut link.to_scene) {
        match ev {
            SceneEvent::Created(c) => {
                spawn_slot_character(&mut commands, &args, &mut meshes, &mut materials, &mut images, &shadow_material, &c);
                sel.selected = Some(c.slot);
            }
            SceneEvent::Deleted(name) => {
                for (e, c, shadow) in &characters {
                    if c.name == name {
                        commands.entity(e).despawn();
                        commands.entity(shadow.1).despawn();
                        if sel.selected == Some(c.slot) {
                            sel.selected = None;
                        }
                    }
                }
            }
        }
    }
}

/// Login `PlayMp3(login_theme)`: keeps playing here (except when entering the game, which stops it).
fn music(sel: Option<Res<CharSelect>>, mut sound: ResMut<crate::sound::SoundQueue>) {
    if !sel.is_some_and(|s| s.starting) {
        sound.music("login_theme");
    }
}

/// Windows default `GetDoubleClickTime`.
const DOUBLE_CLICK_SECS: f32 = 0.5;

/// `StartGame` (ZzzScene.cpp:854-871): with a character selected, `SendRequestJoinMapServer`; the
/// character information (with its map) arrives in `net_messages` and then the map loads.
/// `LoadingScene` stops the login music (ZzzScene.cpp:1503).
fn start_game(
    sel: &mut CharSelect,
    characters: &Query<&SlotCharacter>,
    commands: &mut Commands,
    net: &mut crate::net::Net,
    sound: &mut crate::sound::SoundQueue,
) {
    if sel.starting {
        return;
    }
    let Some(c) = sel.selected.and_then(|s| characters.iter().find(|c| c.slot == s)) else { return };
    commands.insert_resource(crate::account::Hero { name: c.name.clone(), class: c.class, level: c.level });
    sel.starting = true;
    sound.stop_music();
    net.send(mu_protocol::game::ClientMessage::SelectCharacter { name: c.name.clone() });
}
