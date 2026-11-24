//! 3D items inside windows: the original `CNewUI3DCamera` camera
//! (NewUI3DRenderMng.cpp:94-130) and `RenderItem3D` / `RenderObjectScreen` (ZzzInventory.cpp).
//!
//! - **Camera**: the original's is the identity with `gluPerspective2(1°, width/height, 20, 2000)` over
//!   the whole screen, and items live in its space (`CreateScreenVector` with `MousePosition` = 0):
//!   each at depth 0.1 · 2000 (0.07 for the one on the cursor) on the ray through its slot's
//!   point. Here that camera renders into an image the size of the 640×480 screen (in real
//!   pixels), shown by the window above its background and below the info boxes, matching the
//!   original's layer order (window 4.2, camera 5.5, its 2D effects afterwards).
//! - **Axes**: the original camera space (x right, y up, -z forward) is mapped to Bevy with the
//!   same rotation as the rest of the game (MU (x, y, z) → Bevy (x, z, -y)), and the Bevy camera carries
//!   that rotation: so the normal the shader reads for chrome (`model_light.wesl`) is the original's.
//! - **Model**: the item's, in pose 0 of its animation 0; armors use the
//!   `player.bmd` skeleton (`ObjectSelect.Type = MODEL_PLAYER`) and its `BodyHeight`. Unlit (`LightEnable = false`:
//!   flat `BodyLight` color), without the skin meshes (`HideSkin`).
//! - **Glow**: the passes of [`mu_data::items::glow::item_passes`] resolved mesh by mesh like map
//!   objects; the pulsing ones (excellent, +3 to +6, ancient) change color every frame.
//! - **Hover**: the item spins (`ObjectSelect.Angle[1] = WorldTime · 0.45`).

use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::camera::RenderTarget;
use bevy::camera::visibility::RenderLayers;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::ui::UiScale;
use mu_data::bmd::{self, Bmd};
use mu_data::items::glow::{GlowLight, item_passes};
use mu_data::items::model::{self, ITEMVIEW_FAR, ITEMVIEW_FOV_DEG, ITEMVIEW_NEAR, ModelFile, ScreenPose};
use mu_data::items::{Item, ItemTable};
use mu_data::objects::render::{self, Bitmap, LookColor, MeshInfo, ObjectDraw, Pass, Saw};
use mu_data::texture::AlphaKind;
use mu_data::{fs, texture};

use crate::Args;
use crate::model_light::ModelMaterial;
use crate::terrain::to_bevy_image;

/// Systems that draw what was requested in [`ItemViews`] (windows request it earlier).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ItemViewSet;

/// Layer of window items (layer 1 is the character creation preview).
pub const ITEM_LAYER: usize = 2;

pub struct ItemViewPlugin;

impl Plugin for ItemViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ItemViews>()
            .init_resource::<ItemData>()
            .init_resource::<ViewCache>()
            .add_systems(Startup, load_item_data)
            .add_systems(Update, (ensure_target, sync_views).chain().in_set(ItemViewSet));
    }
}

/// What is known about items: the table (`ItemAttribute`), skill mana and the MuMain
/// models for what is missing from the original's table.
#[derive(Resource, Default)]
pub struct ItemData {
    pub table: Option<ItemTable>,
    pub skill_mana: Option<Vec<u16>>,
    pub models_json: HashMap<usize, String>,
}

impl ItemData {
    pub fn attr(&self, item: &Item) -> Option<&mu_data::items::ItemAttribute> {
        self.table.as_ref()?.by_kind(item.kind())
    }

    /// Model path relative to `Data`: the original's table, falling back to MuMain's.
    pub fn model_path(&self, item: &Item) -> Option<PathBuf> {
        match model::model_file(item.group, item.index) {
            Some(ModelFile { dir, name }) => Some(Path::new(dir).join(name)),
            None => self.models_json.get(&item.kind()).map(PathBuf::from),
        }
    }
}

fn load_item_data(args: Res<Args>, mut data: ResMut<ItemData>) {
    match mu_data::items::load(&args.data) {
        Ok(t) => {
            info!("objetos: {} tipos", t.count());
            data.table = Some(t);
        }
        Err(e) => warn!("objetos: sin la tabla de objetos: {e}"),
    }
    data.skill_mana = mu_data::items::load_skill_mana(&args.data).ok().map(|l| l.into_iter().map(|(_, m)| m).collect());
    if let Some(dir) = fs::find(&args.data, "Items").and_then(|d| fs::find(&d, "Models")) {
        for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let Ok(text) = std::fs::read_to_string(e.path()) else { continue };
            for (g, i, file) in model::read_model_json(&text) {
                data.models_json.insert(mu_data::items::kind(g, i), file);
            }
        }
    }
}

/// An item to draw this frame, in rectangle `rect` (x, y, width, height) of the 640×480
/// screen (what is passed to `RenderItem3D`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ItemView {
    /// Identifies the item across frames (its place in the window).
    pub key: u32,
    pub item: Item,
    pub rect: [f32; 4],
    /// On the cursor (`PickUp`: closer to the camera).
    pub picked: bool,
    /// Hovered (`Select`: spins).
    pub hovered: bool,
}

/// What the windows request and the image it is drawn into.
#[derive(Resource, Default)]
pub struct ItemViews {
    pub views: Vec<ItemView>,
    /// `g_pOption->GetRenderLevel()` (0 to 4).
    pub render_level: u8,
    pub image: Option<Handle<Image>>,
    size: UVec2,
}

#[derive(Component)]
struct ItemCamera;

/// A drawn item.
#[derive(Component)]
struct ItemEntity {
    key: u32,
    item: Item,
    render_level: u8,
    pose: ScreenPose,
    /// Materials whose color changes over time.
    animated: Vec<(Handle<ModelMaterial>, GlowLight)>,
}

/// An already posed model mesh.
struct Part {
    mesh: Handle<Mesh>,
    info: MeshInfo,
    image: Option<Handle<Image>>,
}

#[derive(Resource, Default)]
struct ViewCache {
    player: Option<Option<Bmd>>,
    /// (path, character skeleton, `BodyHeight`) → meshes.
    models: HashMap<(PathBuf, bool, i32), Option<Arc<Vec<Part>>>>,
    images: HashMap<PathBuf, Option<(Handle<Image>, AlphaKind)>>,
    bitmaps: HashMap<Bitmap, Option<Handle<Image>>>,
}

/// The image the size of the 640×480 screen in real pixels, and the camera that renders into it.
fn ensure_target(
    mut commands: Commands,
    mut views: ResMut<ItemViews>,
    mut images: ResMut<Assets<Image>>,
    windows: Query<&Window>,
    scale: Res<UiScale>,
    mut cameras: Query<(Entity, &mut Camera), With<ItemCamera>>,
) {
    // Without a window (capture mode) the UI uses pixel scale 1 (camera.rs).
    let s = scale.0 * windows.single().map_or(1.0, |w| w.scale_factor());
    let size = UVec2::new((640.0 * s).round().max(1.0) as u32, (480.0 * s).round().max(1.0) as u32);
    let active = !views.views.is_empty();
    if views.image.is_none() || views.size != size {
        let image = images.add(Image::new_target_texture(size.x, size.y, TextureFormat::Rgba8UnormSrgb, None));
        for (e, _) in &cameras {
            commands.entity(e).despawn();
        }
        commands.spawn((
            Name::new("Cámara de los objetos"),
            ItemCamera,
            Camera3d::default(),
            Msaa::Off,
            Camera { order: -2, clear_color: ClearColorConfig::Custom(Color::NONE), is_active: active, ..default() },
            RenderTarget::Image(image.clone().into()),
            bevy::core_pipeline::tonemapping::Tonemapping::None,
            bevy::core_pipeline::tonemapping::DebandDither::Disabled,
            bevy::light::cluster::ClusterConfig::Single,
            Projection::Perspective(PerspectiveProjection {
                fov: ITEMVIEW_FOV_DEG.to_radians(),
                near: ITEMVIEW_NEAR,
                far: ITEMVIEW_FAR,
                ..default()
            }),
            // The original camera (identity) with the MU-to-Bevy rotation.
            Transform::from_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
            RenderLayers::layer(ITEM_LAYER),
        ));
        views.image = Some(image);
        views.size = size;
        return;
    }
    for (_, mut cam) in &mut cameras {
        if cam.is_active != active {
            cam.is_active = active;
        }
    }
}

/// Item position, rotation and scale (in the original camera space, mapped to Bevy).
fn view_transform(pose: &ScreenPose, view: &ItemView, world_time: f32) -> Transform {
    let p = model::item_position(pose, view.rect, view.picked);
    let mut angle = pose.angle;
    if view.hovered {
        // `if (1 == Select) ObjectSelect.Angle[1] = WorldTime * 0.45f`.
        angle[1] = world_time * 0.45;
    }
    let to_bevy = Quat::from_rotation_x(-FRAC_PI_2);
    let [ax, ay, az] = angle.map(f32::to_radians);
    Transform {
        translation: to_bevy * Vec3::from_array(p),
        rotation: to_bevy * Quat::from_euler(EulerRot::ZYX, az, ay, ax),
        scale: Vec3::splat(pose.scale),
    }
}

#[allow(clippy::too_many_arguments)]
fn sync_views(
    mut commands: Commands,
    args: Res<Args>,
    time: Res<Time<Real>>,
    views: Res<ItemViews>,
    data: Res<ItemData>,
    mut cache: ResMut<ViewCache>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ModelMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut existing: Query<(Entity, &ItemEntity, &mut Transform)>,
) {
    let world_time = time.elapsed_secs() * 1000.0;
    let mut seen = Vec::new();
    for (e, ent, mut tf) in &mut existing {
        let Some(view) = views.views.iter().find(|v| v.key == ent.key) else {
            commands.entity(e).despawn();
            continue;
        };
        if view.item != ent.item || views.render_level != ent.render_level {
            commands.entity(e).despawn();
            continue;
        }
        seen.push(ent.key);
        *tf = view_transform(&ent.pose, view, world_time);
        for (h, light) in &ent.animated {
            if let Some(mut m) = materials.get_mut(h) {
                let c = light.at(world_time);
                m.extension.params.tint = Vec4::new(c[0], c[1], c[2], 1.0);
            }
        }
    }
    for view in views.views.iter().filter(|v| !seen.contains(&v.key)) {
        let two_hand = data.attr(&view.item).is_some_and(|a| a.two_hand);
        let pose = model::screen_pose(&view.item, two_hand);
        let Some(path) = data.model_path(&view.item) else { continue };
        let Some(parts) = cache.parts(&args.data, &path, &pose, &mut meshes, &mut images) else { continue };
        let passes: Vec<(Pass, GlowLight)> = item_passes(&view.item, [1.0; 3], views.render_level)
            .into_iter()
            .map(|p| (Pass::body(p.flags), p.light))
            .collect();
        let only: Vec<Pass> = passes.iter().map(|p| p.0).collect();
        let draw = ObjectDraw { blend_mesh: -1, stream_mesh: -1, tex_coord: [Saw::ZERO; 2], hidden_mesh: -1 };
        let mut animated = Vec::new();
        let mut children = Vec::new();
        for part in parts.iter() {
            for (k, look) in render::resolve_all(&only, &draw, &part.info) {
                let light = passes[k].1;
                // `LightEnable = false`: normal-based light becomes flat `BodyLight` color.
                let look = if look.color == LookColor::Lit { render::MeshLook { color: LookColor::Flat, ..look } } else { look };
                let image = match (look.textured, look.texture) {
                    (false, _) => None,
                    (true, Some(b)) => cache.bitmap(&args.data, b, &mut images),
                    (true, None) => part.image.clone(),
                };
                let body = light.at(world_time);
                let mat = crate::objects::object_material(image, &look, body, None, k, false);
                let h = materials.add(mat);
                if light.animated() {
                    animated.push((h.clone(), light));
                }
                children.push((Mesh3d(part.mesh.clone()), MeshMaterial3d(h), RenderLayers::layer(ITEM_LAYER)));
            }
        }
        let mut e = commands.spawn((
            Name::new(format!("Objeto {},{}", view.item.group, view.item.index)),
            view_transform(&pose, view, world_time),
            Visibility::default(),
            RenderLayers::layer(ITEM_LAYER),
            ItemEntity { key: view.key, item: view.item, render_level: views.render_level, pose, animated },
        ));
        for c in children {
            e.with_child(c);
        }
    }
}

impl ViewCache {
    fn bitmap(&mut self, data: &Path, b: Bitmap, images: &mut Assets<Image>) -> Option<Handle<Image>> {
        self.bitmaps.entry(b).or_insert_with(|| crate::objects::load_bitmap(data, b, images)).clone()
    }

    /// The meshes of model `path` (relative to `Data`), posed as `pose` asks.
    fn parts(
        &mut self,
        data: &Path,
        path: &Path,
        pose: &ScreenPose,
        meshes: &mut Assets<Mesh>,
        images: &mut Assets<Image>,
    ) -> Option<Arc<Vec<Part>>> {
        let key = (path.to_path_buf(), pose.player_skeleton, pose.body_height as i32);
        if let Some(p) = self.models.get(&key) {
            return p.clone();
        }
        let loaded = self.load_parts(data, path, pose, meshes, images).map(Arc::new);
        if loaded.is_none() {
            warn!("objeto: no se pudo cargar {}", path.display());
        }
        self.models.insert(key, loaded.clone());
        loaded
    }

    fn load_parts(
        &mut self,
        data: &Path,
        path: &Path,
        pose: &ScreenPose,
        meshes: &mut Assets<Mesh>,
        images: &mut Assets<Image>,
    ) -> Option<Vec<Part>> {
        // Real folder (`Item`, `Player`, `Item/partCharge1`...) case-insensitively.
        let mut dir = data.to_path_buf();
        for c in path.parent()?.iter() {
            dir = fs::find(&dir, c.to_str()?)?;
        }
        let name = path.file_name()?.to_str()?;
        let model = fs::read(&dir, name).and_then(|b| bmd::read(&b)).ok()?;
        let bones = if pose.player_skeleton {
            if self.player.is_none() {
                let skel = fs::read(&data.join("Player"), "player.bmd").and_then(|b| bmd::read(&b)).ok();
                self.player = Some(skel);
            }
            player_bones(self.player.as_ref()?.as_ref()?, pose.body_height)
        } else {
            model.bone_matrices(0, 0)
        };
        let mut out = Vec::new();
        for (i, m) in model.meshes.iter().enumerate() {
            if hidden_texture(&m.texture) {
                continue;
            }
            let posed = model.pose_with(i, &bones);
            if posed.positions.is_empty() {
                continue;
            }
            let mesh = meshes.add(
                Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
                    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, posed.positions)
                    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, posed.normals)
                    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, posed.uvs),
            );
            let tex_path = item_texture(data, &dir, &bmd::texture_file_name(&m.texture));
            let own = tex_path.and_then(|p| {
                self.images
                    .entry(p.clone())
                    .or_insert_with(|| {
                        let t = texture::load(p.parent()?, p.file_name()?.to_str()?).ok()?;
                        Some((images.add(to_bevy_image(&t)), t.alpha_kind()))
                    })
                    .clone()
            });
            out.push(Part {
                mesh,
                info: MeshInfo {
                    index: i,
                    texture_index: m.texture_index as i32,
                    script: bmd::texture_script(&m.texture),
                    has_alpha: own.as_ref().is_some_and(|o| o.1 != AlphaKind::Opaque),
                },
                image: own.map(|o| o.0),
            });
        }
        Some(out)
    }
}

/// Where an item model's texture is. The model comes from its own folder (`AccessModel`, e.g. the
/// pets `Helper01-04.bmd` from `Player`), but its textures are opened from the folder given to
/// `OpenTexture`: `Item\` for almost every item (ZzzOpenData.cpp:1390-1403), `Skill\` or `Npc\` for a
/// few (1344-1378). So: next to the model first, then those folders.
fn item_texture(data: &Path, model_dir: &Path, name: &str) -> Option<std::path::PathBuf> {
    fs::find(model_dir, name).or_else(|| {
        ["Item", "Skill", "Npc"].iter().find_map(|folder| fs::find(&fs::find(data, folder)?, name))
    })
}

/// `OpenTexture` (LoadData.cpp:46-71): textures starting with `ski` or `level` are skin
/// (`BITMAP_SKIN`) and those with `hair` are hair (`BITMAP_HAIR`), which `HideSkin` does not draw; `hid`
/// ones (`BITMAP_HIDE`) are never drawn.
fn hidden_texture(name: &str) -> bool {
    let file = name.rsplit(['\\', '/']).next().unwrap_or(name);
    file.starts_with("ski") || file.to_ascii_lowercase().starts_with("level") || file.starts_with("hid") || file.starts_with("hair")
}

/// `player.bmd` bones in pose 0 with `BodyHeight` (`BMD::Animation`, ZzzBMD.cpp:128-134: with the
/// animation locked, bone 0 drops by `BodyHeight`, and with it all bones hanging from it).
fn player_bones(skel: &Bmd, body_height: f32) -> Vec<bmd::Mat34> {
    let mut bones = skel.bone_matrices(0, 0);
    let locked = skel.actions.first().is_some_and(|a| a.lock_positions.is_some());
    if locked && body_height != 0.0 {
        let mut in_tree = vec![false; skel.bones.len()];
        for (i, b) in skel.bones.iter().enumerate() {
            in_tree[i] = i == 0 || usize::try_from(b.parent).ok().filter(|&p| p < i).is_some_and(|p| in_tree[p]);
            if in_tree[i] && !b.dummy {
                bones[i][2][3] += body_height;
            }
        }
    }
    bones
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pets' models are in `Player` but their textures in `Item` (they showed up white).
    #[test]
    fn pet_textures_come_from_item() {
        let data = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../referencias/MuMain/src/bin/Data");
        let Some(player) = fs::find(&data, "Player") else { return };
        for i in 1..=4 {
            let name = format!("Helper0{i}.bmd");
            let Ok(model) = fs::read(&player, &name).and_then(|b| bmd::read(&b)) else { continue };
            for m in &model.meshes {
                if hidden_texture(&m.texture) {
                    continue;
                }
                let tex = bmd::texture_file_name(&m.texture);
                assert!(item_texture(&data, &player, &tex).is_some(), "{name}: no texture {tex}");
            }
        }
    }

    #[test]
    fn texturas_escondidas() {
        assert!(hidden_texture("skin_barbarian_01.jpg"));
        assert!(hidden_texture("Level_man01.jpg"));
        assert!(hidden_texture("hide.jpg"));
        assert!(!hidden_texture("Sword01.jpg"));
        // `ski` is case-sensitive in the original.
        assert!(!hidden_texture("Skin.jpg"));
    }

    /// The dragon armor with the character skeleton fits its slot (2×3 = 40×60 of the
    /// 640 screen): its height at inventory scale, in pixels of that screen, is about the
    /// slot's.
    #[test]
    fn armadura_del_tamano_de_su_casilla() {
        let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../referencias/MuMain/src/bin/Data");
        let (Ok(skel), Ok(armor)) = (
            fs::read(&data.join("Player"), "player.bmd").and_then(|b| bmd::read(&b)),
            fs::read(&data.join("Player"), "ArmorMale02.bmd").and_then(|b| bmd::read(&b)),
        ) else {
            return;
        };
        let pose = model::screen_pose(&Item::new(8, 1), false);
        let bones = player_bones(&skel, pose.body_height);
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for i in 0..armor.meshes.len() {
            for p in armor.pose_with(i, &bones).positions {
                lo = lo.min(p[2]);
                hi = hi.max(p[2]);
            }
        }
        // Camera units per 640-screen unit at depth 200.
        let unit = 200.0 * 0.5f32.to_radians().tan() / 240.0;
        let px = (hi - lo) * pose.scale / unit;
        eprintln!("armadura: alto {:.0} → {px:.1} de la pantalla de 640 (z {lo:.0}..{hi:.0})", hi - lo);
        assert!((20.0..120.0).contains(&px), "{px}");
    }
}
