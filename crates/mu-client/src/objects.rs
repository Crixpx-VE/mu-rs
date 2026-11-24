//! Map objects (trees, houses, walls...) from `EncTerrainN.obj` + `.bmd` models.
//!
//! Each model is loaded only once; each map object is an instance with its own
//! position, rotation and scale.
//!
//! Animation: as in the original, all objects start at frame 0, so those running
//! at the same speed are always in the same pose. They are grouped by (model,
//! speed) and each group shares a mesh recomputed on the CPU once per frame.
//!
//! Material effects (from the original): additive meshes that flicker like fire or pulse,
//! and scrolling textures (falling water, waterfalls).
//!
//! How each mesh is drawn comes from the type's passes (`ModelRules::passes`: the
//! `RenderBody`/`RenderMesh` calls of its draw code) and the texture scripts (`_R`, `_H`, `_S`, `_N`),
//! via `mu_data::objects::render::resolve_all`: chrome, subtract (`RENDER_DARK`), additive (`RENDER_BRIGHT`,
//! `BlendMesh`), `StreamMesh` and textures replacing the mesh's. Each pass of each mesh is
//! an entity with the same mesh; since the original draws with `GL_LEQUAL` (the last pass covers the
//! earlier ones at the same spot), each pass gets slightly more `depth_bias` than the previous one.
//!
//! Things that move on their own (Devias doors, Lost Tower skulls) and the
//! `RenderBodyShadow` shadows live in [`movers`].

mod movers;

use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::math::Affine2;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;
use mu_data::bmd::TextureScript;
use mu_data::objects::render::{Bitmap, BodyWave, ChromeMode, LookBlend, LookColor, MeshInfo, MeshLook, ObjectDraw, Saw};
use mu_data::objects::{Glow, LightAdd, Motion};
use mu_data::terrain::SIZE;
use mu_data::texture::AlphaKind;
use mu_data::{bmd, fs, objects, texture};

use crate::Args;
use crate::model_light::{ModelLight, ModelMaterial, model_material};
use crate::effects::{Emitters, PlacedEmitter};
use crate::terrain::{LoadedTerrain, MU_SCALE, to_bevy_image};

pub struct ObjectsPlugin;

impl Plugin for ObjectsPlugin {
    fn build(&self, app: &mut App) {
        // After the terrain, so its light can be queried.
        app.init_resource::<AnimatedModels>()
            .init_resource::<AnimatedMaterials>()
            .init_resource::<Roofs>()
            .init_resource::<LoginFades>()
            .init_resource::<movers::Movers>()
            .init_resource::<movers::BodyShadows>()
            .init_resource::<FxFollowers>()
            .add_systems(Update, follow_fx_objects)
            .add_systems(Update, spawn_objects.run_if(crate::scene::load_step(crate::scene::Step::Objects)))
            .add_systems(Update, (animate_objects, animate_materials, fade_roofs, fade_login_objects, light_login_objects))
            .add_systems(Update, (movers::move_objects, movers::update_shadows.after(animate_objects)));
    }
}

/// Roof materials, which fade out when entering a house.
#[derive(Resource)]
pub struct Roofs {
    materials: Vec<(Handle<ModelMaterial>, AlphaMode)>,
    alpha: f32,
}

impl Default for Roofs {
    fn default() -> Self {
        Self { materials: vec![], alpha: 1.0 }
    }
}

/// The original `Alpha()`: `alpha += (target - alpha) · 0.1` every tick (25 per second),
/// converted to real time so it takes the same time at any fps.
fn fade_roofs(
    time: Res<Time>,
    args: Res<Args>,
    terrain: Option<Res<LoadedTerrain>>,
    players: Query<&crate::character::Player>,
    mut roofs: ResMut<Roofs>,
    mut materials: ResMut<Assets<ModelMaterial>>,
) {
    let (Some(t), Ok(p)) = (terrain, players.single()) else { return };
    if roofs.materials.is_empty() {
        return;
    }
    let (x, y) = ((p.pos.x.max(0.0) as usize).min(SIZE - 1), (p.pos.y.max(0.0) as usize).min(SIZE - 1));
    let inside = objects::indoor_tile(args.world, t.0.mapping.layer1[y * SIZE + x]);
    let target = if inside { 0.0 } else { 1.0 };

    let before = roofs.alpha;
    let keep = 0.9f32.powf(time.delta_secs() * objects::TICKS_PER_SECOND);
    let mut alpha = target + (before - target) * keep;
    if (alpha - target).abs() < 0.005 {
        alpha = target;
    }
    if alpha == before {
        return;
    }
    roofs.alpha = alpha;
    for (h, original) in &roofs.materials {
        if let Some(mut m) = materials.get_mut(h) {
            m.base.base_color.set_alpha(alpha);
            // Fully opaque: as usual. While fading: with transparency.
            m.base.alpha_mode = if alpha >= 1.0 { *original } else { AlphaMode::Blend };
        }
    }
}

/// On map change: nothing from the previous one.
pub fn reset(
    mut anims: ResMut<AnimatedModels>,
    mut mats: ResMut<AnimatedMaterials>,
    mut roofs: ResMut<Roofs>,
    mut login: ResMut<LoginFades>,
    mut movers: ResMut<movers::Movers>,
    mut shadows: ResMut<movers::BodyShadows>,
    mut followers: ResMut<FxFollowers>,
) {
    followers.0.clear();
    anims.0.clear();
    mats.0.clear();
    *roofs = Roofs::default();
    *login = LoginFades::default();
    *movers = movers::Movers::default();
    *shadows = movers::BodyShadows::default();
}

/// `LoginSceneCameraDefaults::RENDER_OBJECT_DIST` (`CameraMove.h`).
const LOGIN_OBJECT_DIST: f32 = 5903.0;

/// Fade of the login objects (`WD_73NEW_LOGIN_SCENE`) by their distance to the camera.
///
/// `CullObjects` (`ZzzObject.cpp` ≈4205-4246), every frame, with the distance in the (x, y)
/// plane from the camera to the object (`TestFrustrum2D` is always true in `LOG_IN_SCENE`):
/// - Types 122-124, 126, 127, 129 and 159 closer than 2·`RENDER_OBJECT_DIST`: `BlendMeshLight` and
///   `AlphaTarget` rise by 0.03·`FPS_ANIMATION_FACTOR` up to 1, and it is drawn.
/// - The rest (type ≤ `MAX_WORLD_OBJECTS`) closer than `RENDER_OBJECT_DIST`: `AlphaTarget` rises the same way.
/// - Otherwise, if `AlphaTarget != 0` and it is farther than `RENDER_OBJECT_DIST`: `BlendMeshLight`,
///   `Alpha` and `AlphaTarget` set to 0. Not drawn (and emits no effects).
///
/// Every tick `MoveObject` does `Alpha(o)` (`Alpha += (AlphaTarget − Alpha)·0.1` and
/// `BlendMeshLight ≤ Alpha`) and, if `Alpha ≥ 0.01`, again in `GMEmpireGuardian4::MoveObject`. With
/// `Alpha < 0.01` it is not drawn; below 1 it is drawn blended (and the original alpha test,
/// (> 0.25), removes it earlier). Additive meshes do not use `Alpha`: the `_R` script ones (127, 157) glow
/// with `BlendMeshLight`; the `RENDER_BRIGHT` ones (37, 70) do not change.
#[derive(Resource, Default)]
pub struct LoginFades {
    objects: Vec<LoginFade>,
    /// Time accumulated toward the next tick (25 per second).
    tick: f32,
}

struct LoginFade {
    kind: i16,
    /// Position (x, y) in MU.
    position: Vec2,
    alpha: f32,
    /// Previous tick `Alpha`, for interpolation when drawing.
    prev_alpha: f32,
    alpha_target: f32,
    blend_light: f32,
    /// (entity, own material, original alpha mode, additive).
    parts: Vec<(Entity, Handle<ModelMaterial>, AlphaMode, bool)>,
    /// Index in `FxWorld::map_objects` if it has effects code.
    fx: Option<usize>,
    /// Last values set on the materials (to skip touching them if unchanged).
    shown: Option<(bool, f32, f32)>,
}

/// Types with a `_R` script mesh (their own `BlendMesh`, glowing with `BlendMeshLight`).
const LOGIN_SCRIPT_BRIGHT: [i16; 2] = [127, 157];

/// `BodyLight` of objects with their own material (login and selection): the ground light at their spot,
/// with this tick's dynamic lights (fires, `AddTerrainLight`), like `RequestModelTerrainLight`.
fn light_login_objects(
    terrain: Option<Res<LoadedTerrain>>,
    lights: Res<crate::terrain::TerrainLights>,
    fades: Res<LoginFades>,
    mut materials: ResMut<Assets<ModelMaterial>>,
) {
    let Some(t) = terrain else { return };
    if fades.objects.is_empty() {
        return;
    }
    for o in &fades.objects {
        let p = o.position / MU_SCALE;
        let (x, y) = (p.x.clamp(0.0, (SIZE - 2) as f32), p.y.clamp(0.0, (SIZE - 2) as f32));
        let (xi, yi) = (x as usize, y as usize);
        let (fx, fy) = (x - xi as f32, y - yi as f32);
        // Light at each tile corner with the dynamic lights, then interpolated.
        let corner = |cx: usize, cy: usize| {
            let mut l = Vec3::from_array(t.0.light[cy * SIZE + cx]);
            for light in &lights.lights {
                let d = (light.pos - Vec2::new(cx as f32, cy as f32)).length();
                if d < light.range {
                    l += Vec3::from_array(light.color) * ((light.range - d) / light.range);
                }
            }
            l.max(Vec3::ZERO)
        };
        let top = corner(xi, yi).lerp(corner(xi + 1, yi), fx);
        let bottom = corner(xi, yi + 1).lerp(corner(xi + 1, yi + 1), fx);
        let light = top.lerp(bottom, fy);
        for (_, h, _, additive) in &o.parts {
            if *additive {
                continue;
            }
            let Some(m) = materials.get(h) else { continue };
            let old = m.extension.light;
            if old.w > 0.5 && (old.truncate() - light).abs().max_element() > 0.01 {
                if let Some(mut m) = materials.get_mut(h) {
                    m.extension.light = light.extend(1.0);
                }
            }
        }
    }
}

fn fade_login_objects(
    time: Res<Time>,
    cameras: Query<&GlobalTransform, (With<Camera3d>, With<IsDefaultUiCamera>)>,
    mut fades: ResMut<LoginFades>,
    mut fx_world: ResMut<crate::fx::FxWorld>,
    mut materials: ResMut<Assets<ModelMaterial>>,
    mut visibility: Query<&mut Visibility>,
) {
    if fades.objects.is_empty() {
        return;
    }
    let Some(camera) = cameras.iter().next() else { return };
    let c = camera.translation();
    let camera_mu = Vec2::new(c.x * MU_SCALE, -c.z * MU_SCALE);

    let dt = time.delta_secs();
    // `FPS_ANIMATION_FACTOR`: the per-frame rise is proportional to the ticks it lasts.
    let factor = dt * objects::TICKS_PER_SECOND;
    let tick_len = 1.0 / objects::TICKS_PER_SECOND;
    fades.tick += dt;
    let mut ticks = 0;
    while fades.tick >= tick_len && ticks < 8 {
        fades.tick -= tick_len;
        ticks += 1;
    }
    fades.tick = fades.tick.min(tick_len);
    let between = fades.tick / tick_len;

    let ramp = |v: &mut f32| {
        if *v < 1.0 {
            *v += 0.03 * factor;
        } else {
            *v = 1.0;
        }
    };
    for f in fades.objects.iter_mut() {
        let d = (camera_mu - f.position).length();
        let special = matches!(f.kind, 122..=124 | 126 | 127 | 129 | 159);
        let drawn = if special && d < LOGIN_OBJECT_DIST * 2.0 {
            ramp(&mut f.blend_light);
            ramp(&mut f.alpha_target);
            true
        } else if d < LOGIN_OBJECT_DIST && f.kind <= 160 {
            ramp(&mut f.alpha_target);
            true
        } else {
            if f.alpha_target != 0.0 && d > LOGIN_OBJECT_DIST {
                f.blend_light = 0.0;
                f.alpha = 0.0;
                f.prev_alpha = 0.0;
                f.alpha_target = 0.0;
            }
            false
        };

        for _ in 0..ticks {
            f.prev_alpha = f.alpha;
            // `Alpha(o)` from `MoveObject` and, if it continues, the one from `GMEmpireGuardian4::MoveObject`.
            for pass in 0..2 {
                if pass == 1 && f.alpha < 0.01 {
                    break;
                }
                f.alpha += (f.alpha_target - f.alpha) * 0.1;
                if f.blend_light > f.alpha {
                    f.blend_light = f.alpha;
                }
            }
        }

        if let Some(m) = f.fx.and_then(|i| fx_world.map_objects.get_mut(i)).filter(|m| m.ty == f.kind as i32) {
            m.user[crate::fx::kinds::objects_login::SLOT_CULLED] = if drawn { 0.0 } else { 1.0 };
        }

        let alpha = f.prev_alpha + (f.alpha - f.prev_alpha) * between;
        let visible = drawn && alpha >= 0.01;
        let light = if LOGIN_SCRIPT_BRIGHT.contains(&f.kind) { f.blend_light } else { 1.0 };
        // Rounded so materials are not rebuilt for invisible changes.
        let shown = (visible, (alpha * 100.0).round() / 100.0, (light * 100.0).round() / 100.0);
        if f.shown == Some(shown) {
            continue;
        }
        f.shown = Some(shown);
        let (visible, alpha, light) = shown;
        for (entity, handle, original, additive) in &f.parts {
            let show = visible && (*additive || alpha > ALPHA_REF);
            if let Ok(mut v) = visibility.get_mut(*entity) {
                v.set_if_neq(if show { Visibility::Inherited } else { Visibility::Hidden });
            }
            if !show {
                continue;
            }
            let Some(mut m) = materials.get_mut(handle) else { continue };
            if *additive {
                m.base.base_color = Color::linear_rgb(light, light, light);
            } else if alpha >= 0.99 {
                m.base.base_color.set_alpha(1.0);
                m.base.alpha_mode = *original;
            } else {
                m.base.base_color.set_alpha(alpha);
                m.base.alpha_mode = AlphaMode::Blend;
            }
        }
    }
}

/// Animated object groups (trees, flags, windmill...).
#[derive(Resource, Default)]
pub struct AnimatedModels(Vec<AnimatedModel>);

struct AnimatedModel {
    model: Arc<bmd::Bmd>,
    /// Animation frames per second.
    fps: f32,
    frame: f32,
    /// `o->CurrentAction` (only changes for those with [`SledDog`]).
    action: usize,
    /// (mesh index in the .bmd, mesh shared by the group instances)
    parts: Vec<(usize, Handle<Mesh>)>,
    /// Bones of this frame's pose (for the `movers` shadows).
    bones: Vec<bmd::Mat34>,
    /// Devias sled dog (one group per dog).
    sled_dog: Option<SledDog>,
    /// Objects with `ModelRules::fx_animation` (one group per object): the action and frame
    /// are driven by their effects code in `FxWorld::map_objects[i]` (see [`fx_anim`]).
    fx: Option<usize>,
}

/// Free fields of `fx::core::MapObject::user` through which the effects code of an object with
/// `ModelRules::fx_animation` tells this module what to draw (written every tick).
pub mod fx_anim {
    /// `o->AnimationFrame` at the end of the tick.
    pub const FRAME: usize = 0;
    /// `o->CurrentAction` (if the model lacks it, `BMD::Animation` uses 0).
    pub const ACTION: usize = 1;
    /// `o->AnimationFrame` of the previous tick (to interpolate between ticks).
    pub const PREV_FRAME: usize = 2;
}

/// Entities of objects with `ModelRules::fx_animation`: they follow the position of their
/// `FxWorld::map_objects[i]` (e.g. the rising and falling Refuge light, type 84).
#[derive(Resource, Default)]
pub struct FxFollowers(Vec<(usize, Vec<Entity>, Vec3)>);

fn follow_fx_objects(
    fx_world: Res<crate::fx::FxWorld>,
    mut followers: ResMut<FxFollowers>,
    mut transforms: Query<&mut Transform>,
) {
    let to_bevy = Quat::from_rotation_x(-FRAC_PI_2);
    for (i, entities, shown) in followers.0.iter_mut() {
        let Some(m) = fx_world.map_objects.get(*i) else { continue };
        if m.position == *shown {
            continue;
        }
        *shown = m.position;
        let translation = to_bevy * m.position / MU_SCALE;
        for e in entities.iter() {
            if let Ok(mut t) = transforms.get_mut(*e) {
                t.translation = translation;
            }
        }
    }
}

/// `RenderObjectVisual`, Devias 103 (`ZzzObject.cpp:2824-2831`): on the last frame of the action,
/// `rand()%32 == 0` → `SetAction(o, 1)`, otherwise `SetAction(o, 0)` (which restarts the animation if it changes).
struct SledDog {
    tick: f32,
    rng: crate::fx::core::Rng,
}

/// Time-varying materials: glow of additive meshes and scrolling textures.
#[derive(Resource, Default)]
pub struct AnimatedMaterials(Vec<AnimatedMaterial>);

struct AnimatedMaterial {
    handle: Handle<ModelMaterial>,
    /// Varying `BlendMeshLight` (flicker, pulse).
    glow: Option<Glow>,
    /// Multiplied like the original does (in its color space) or, at login, as is.
    glow_srgb: bool,
    /// `BlendMeshTexCoordU/V`.
    wave: Option<[Saw; 2]>,
    /// `BlendMesh` mesh outside the login: `glColor3f(BodyLight · BlendMeshLight)` goes in the tint (which
    /// the shader clamps to [0, 1] like OpenGL), with the ground light and the object's `o->Light`.
    tint: Option<([f32; 3], Option<LightAdd>)>,
    /// Time-varying `BodyLight` ([`BodyWave`]): the wave, the object light, its `o->Timer`
    /// and whether the mesh is lit by the normal (otherwise it is the flat color).
    body: Option<(BodyWave, [f32; 3], f32, bool)>,
    /// Animated texture (`BITMAP_WATER + WaterTextureNumber`): one frame per tick.
    frames: Vec<Handle<Image>>,
}

/// `BlendMeshLight` as material color (the original multiplies in sRGB).
fn glow_color(g: f32, srgb: bool) -> Color {
    if srgb { Color::srgb(g, g, g) } else { Color::linear_rgb(g, g, g) }
}

fn animate_objects(
    time: Res<Time>,
    fixed: Res<Time<Fixed>>,
    fx_world: Res<crate::fx::FxWorld>,
    mut anims: ResMut<AnimatedModels>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let dt = time.delta_secs();
    let tick_len = 1.0 / objects::TICKS_PER_SECOND;
    for a in &mut anims.0 {
        if let Some(i) = a.fx {
            // What the effects code left in the last tick, interpolated toward the next
            // (except when it goes back: animation wrap or `SetAction`).
            let Some(m) = fx_world.map_objects.get(i) else { continue };
            let (prev, cur) = (m.user[fx_anim::PREV_FRAME], m.user[fx_anim::FRAME]);
            let t = fixed.overstep_fraction();
            let frame = if cur >= prev { prev + (cur - prev) * t } else { cur };
            let action = m.user[fx_anim::ACTION].max(0.0) as usize;
            let action = if action < a.model.actions.len() { action } else { 0 };
            if a.action == action && a.frame == frame {
                continue;
            }
            a.action = action;
            a.frame = frame;
        } else {
            a.frame += a.fps * dt;
        }
        if let Some(dog) = a.sled_dog.as_mut() {
            let frames = a.model.actions.get(a.action).map_or(1, |x| x.frames.max(1)) as f32;
            a.frame = a.frame.rem_euclid(frames);
            dog.tick += dt;
            while dog.tick >= tick_len {
                dog.tick -= tick_len;
                if a.frame.floor() as i32 == frames as i32 - 1 {
                    let want = if dog.rng.rand() % 32 == 0 { 1 } else { 0 };
                    // `SetAction`: only if the action exists and is different.
                    if want != a.action && want < a.model.actions.len() {
                        a.action = want;
                        a.frame = 0.0;
                    }
                }
            }
        }
        a.bones = a.model.bone_matrices_at(a.action, a.frame);
        for (i, handle) in &a.parts {
            let posed = a.model.pose_with(*i, &a.bones);
            if let Some(mut m) = meshes.get_mut(handle) {
                m.insert_attribute(Mesh::ATTRIBUTE_POSITION, posed.positions);
                m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, posed.normals);
            }
        }
    }
}

fn animate_materials(
    time: Res<Time>,
    anims: Res<AnimatedMaterials>,
    mut materials: ResMut<Assets<ModelMaterial>>,
    mut tick: Local<f32>,
    mut rng: Local<u32>,
) {
    let t = time.elapsed_secs();
    // The original `WorldTime`, in ms.
    let world_time = (time.elapsed_secs_f64() * 1000.0) as f32;
    // The original flicker changes on each logic update (25 per second).
    *tick += time.delta_secs();
    let new_tick = *tick >= 1.0 / objects::TICKS_PER_SECOND;
    if new_tick {
        *tick = 0.0;
    }
    for a in &anims.0 {
        let glow = match a.glow {
            None | Some(Glow::Steady(_)) => None,
            Some(Glow::Flicker { min, max }) => new_tick.then(|| min + (max - min) * random(&mut rng)),
            Some(g @ (Glow::Pulse { .. } | Glow::AbsPulse { .. })) => Some(g.at(t)),
        };
        // Time-varying `o->Light` (added to the ground light).
        let moving_light = a.tint.is_some_and(|(_, add)| matches!(add, Some(LightAdd::Wave { .. })));
        // `WaterTextureNumber`: advances one per tick, from 0 to 31.
        let frame = (!a.frames.is_empty()).then(|| (time.elapsed_secs_f64() * objects::TICKS_PER_SECOND as f64) as usize % a.frames.len());
        let new_frame = frame.is_some() && new_tick;
        // The pulsing `BodyLight`, once per tick like the original (there are hundreds of objects with their
        // own phase in Land of Trials: updating them every frame would be unnoticeable and costly).
        let body = a.body.filter(|_| new_tick);
        if glow.is_none() && a.wave.is_none() && !moving_light && body.is_none() && !new_frame {
            continue;
        }
        let Some(mut m) = materials.get_mut(&a.handle) else { continue };
        if let Some((w, object, timer, lit)) = body {
            let l = w.light(object, world_time, timer);
            if lit {
                m.extension.light = Vec4::new(l[0], l[1], l[2], 1.0);
            } else {
                m.extension.params.tint = Vec4::new(l[0], l[1], l[2], 1.0);
            }
        }
        if let Some(f) = frame.filter(|_| new_frame) {
            m.base.base_color_texture = Some(a.frames[f].clone());
        }
        if let Some((body, add)) = a.tint {
            if glow.is_some() || moving_light {
                let g = glow.unwrap_or(match a.glow {
                    Some(Glow::Steady(g)) => g,
                    _ => 1.0,
                });
                let add = add.map_or([0.0; 3], |l| l.at(world_time));
                let c = [0, 1, 2].map(|k| (body[k] + add[k]) * g);
                m.extension.params.tint = Vec4::new(c[0], c[1], c[2], 1.0);
            }
        } else if let Some(g) = glow {
            m.base.base_color = glow_color(g, a.glow_srgb);
        }
        if let Some([u, v]) = a.wave {
            m.base.uv_transform = Affine2::from_translation(Vec2::new(u.at(world_time), v.at(world_time)));
        }
    }
}

/// Pseudo-random number in [0, 1) (xorshift; nothing better is needed for a flicker).
fn random(state: &mut u32) -> f32 {
    if *state == 0 {
        *state = 0x9E37_79B9;
    }
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    (*state >> 8) as f32 / (1u32 << 24) as f32
}

/// A model mesh, ready to instance.
struct ModelPart {
    mesh: Handle<Mesh>,
    mesh_index: usize,
    /// Full texture path (cache key).
    texture: Option<PathBuf>,
    /// `m->Texture`.
    texture_index: i32,
    /// Texture name script (`_R`, `_H`, `_S`, `_N`).
    script: Option<TextureScript>,
}

/// Where a pass's texture comes from.
#[derive(Hash, PartialEq, Eq, Clone)]
enum TextureSource {
    None,
    Own(PathBuf),
    Bitmap(Bitmap),
}

#[derive(Hash, PartialEq, Eq)]
struct MaterialKey {
    texture: TextureSource,
    /// The pass's `BodyLight` (quantized).
    light: [u8; 3],
    blend: LookBlend,
    color: LookColor,
    chrome: Option<ChromeMode>,
    /// Pass order (`depth_bias`).
    pass: usize,
    /// Materials with effects belong to their (model, mesh): not shared.
    effect_owner: Option<(PathBuf, usize)>,
    /// Those that fade on their own (login objects) belong to their object.
    instance: Option<usize>,
}

#[derive(Default)]
struct Caches {
    bmds: HashMap<PathBuf, Option<Arc<bmd::Bmd>>>,
    /// (model, speed in thousandths of a frame/s; 0 = static, object with its own group) → meshes
    models: HashMap<(PathBuf, u32, Option<usize>), (Vec<ModelPart>, Option<usize>)>,
    images: HashMap<PathBuf, Option<(Handle<Image>, AlphaKind)>>,
    bitmaps: HashMap<Bitmap, Option<Handle<Image>>>,
    /// Frames of animated textures (`BITMAP_WATER`).
    frames: HashMap<Bitmap, Vec<Handle<Image>>>,
    materials: HashMap<MaterialKey, Handle<ModelMaterial>>,
}

/// Loads a global texture (`BITMAP_CHROME`...) with the original filter and wrap mode.
pub(crate) fn load_bitmap(data: &Path, b: Bitmap, images: &mut Assets<Image>) -> Option<Handle<Image>> {
    load_frame(data, b, 0, images)
}

/// Frame `i` of a global texture (the only one, except for animated ones: [`Bitmap::frame_path`]).
fn load_frame(data: &Path, b: Bitmap, i: usize, images: &mut Assets<Image>) -> Option<Handle<Image>> {
    let path = b.frame_path(i);
    let rel = Path::new(&path);
    let mut dir = data.to_path_buf();
    for part in rel.parent()?.iter() {
        dir = fs::find(&dir, part.to_str()?)?;
    }
    let name = bmd::texture_file_name(rel.file_name()?.to_str()?);
    let t = texture::load(&dir, &name).map_err(|e| warn!("{path}: {e}")).ok()?;
    let mut image = to_bevy_image(&t);
    let filter = if b.nearest() { ImageFilterMode::Nearest } else { ImageFilterMode::Linear };
    let address = if b.clamp() { ImageAddressMode::ClampToEdge } else { ImageAddressMode::Repeat };
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: address,
        address_mode_v: address,
        mag_filter: filter,
        min_filter: filter,
        mipmap_filter: filter,
        anisotropy_clamp: if b.nearest() { 1 } else { 16 },
        ..default()
    });
    Some(images.add(image))
}

fn spawn_objects(
    mut commands: Commands,
    args: Res<Args>,
    terrain: Option<Res<LoadedTerrain>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ModelMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut anims: ResMut<AnimatedModels>,
    mut mat_anims: ResMut<AnimatedMaterials>,
    mut roofs: ResMut<Roofs>,
    mut fx: ResMut<Emitters>,
    registry: Res<crate::fx::core::Registry>,
    mut fx_world: ResMut<crate::fx::FxWorld>,
    mut login_fades: ResMut<LoginFades>,
    mut operates: ResMut<crate::operate::Operates>,
    (mut movers, mut shadows, mut fx_materials, mut followers): (
        ResMut<movers::Movers>,
        ResMut<movers::BodyShadows>,
        ResMut<Assets<crate::fx::material::FxMaterial>>,
        ResMut<FxFollowers>,
    ),
) {
    // At login each object fades on its own by its distance to the camera.
    let login = args.world == crate::scene::LOGIN_WORLD;
    // Login and character selection: their additive glow is handled by `LoginFades`/`charselect` as always.
    let login_scene = matches!(args.world, 74 | 75);
    let list = match objects::load(&args.data, args.world) {
        Ok(l) => l,
        Err(e) => {
            warn!("sin objetos para World{}: {e}", args.world);
            return;
        }
    };

    let mut c = Caches::default();
    let mut spawned = 0;
    let mut missing: HashMap<i16, String> = HashMap::new();

    for (index, o) in list.iter().enumerate() {
        let rules = objects::model_rules(args.world, o.kind);
        // `o->Timer` from `CreateObject` (the pulsing lights of Land of Trials).
        let timer = objects::create_timer(args.world, o.kind, index);
        // Benches, chairs and pose boxes (`CreateOperate`), invisible ones included.
        let operate = crate::operate::rule(args.world, o.kind).map(|rule| {
            operates.0.push(crate::operate::Operate {
                rule,
                position: Vec3::from_array(o.position),
                angle: o.angle[2],
                materials: Vec::new(),
                light: Vec3::ONE,
                glow: 0.0,
            });
            operates.0.len() - 1
        });
        let mut fade = login.then(|| LoginFade {
            kind: o.kind,
            position: Vec2::new(o.position[0], o.position[1]),
            alpha: 1.0,
            prev_alpha: 1.0,
            alpha_target: 1.0,
            blend_light: 1.0,
            parts: Vec::new(),
            fx: None,
            shown: None,
        });
        let emitters = objects::emitters(args.world, o.kind);
        let mu_rotation = {
            let [ax, ay, az] = o.angle.map(f32::to_radians);
            Quat::from_euler(EulerRot::ZYX, az, ay, ax)
        };
        let place = |emitter: &objects::Emitter, model: Option<(Arc<bmd::Bmd>, f32)>| PlacedEmitter {
            emitter: *emitter,
            position: Vec3::from_array(o.position),
            rotation: mu_rotation,
            scale: o.scale,
            angle: Vec3::from_array(o.angle),
            model,
        };
        // Objects with effects code (`MoveObject`/`UpdateObjectVisual`) for the engine.
        let world_active = args.world as i32 - 1;
        let has_fx = registry.objects.contains_key(&(world_active, o.kind as i32));
        // Provisional emitters remain only for objects the engine does not translate yet.
        let emitters = if has_fx { Vec::new() } else { emitters };
        // Boneless emitters (fires, smoke) do not need the model: hidden ones too.
        for e in emitters.iter().filter(|e| e.bone.is_none()) {
            fx.0.push(place(e, None));
        }
        let fx_object = |model: Option<Arc<bmd::Bmd>>, velocity: f32| crate::fx::core::MapObject {
            ty: o.kind as i32,
            position: Vec3::from_array(o.position),
            angle: Vec3::from_array(o.angle),
            scale: o.scale,
            light: Vec3::from_array(terrain.as_ref().map_or([1.0; 3], |t| {
                let x = ((o.position[0] / MU_SCALE).max(0.0) as usize).min(SIZE - 1);
                let y = ((o.position[1] / MU_SCALE).max(0.0) as usize).min(SIZE - 1);
                t.0.light[y * SIZE + x]
            })),
            model,
            velocity,
            animation_frame: 0.0,
            bones: None,
            timer: 0.0,
            hidden_mesh: if rules.hidden { -2 } else { -1 },
            user: [0.0; 4],
            created: false,
        };
        let rel = objects::model_path(args.world, o.kind);
        let mut load_bmd = |rel: &PathBuf, missing: &mut HashMap<i16, String>| {
            c.bmds
                .entry(rel.clone())
                .or_insert_with(|| {
                    let dir = args.data.join(rel.parent().unwrap_or(Path::new("")));
                    let name = rel.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    fs::read(&dir, name)
                        .and_then(|b| bmd::read(&b))
                        .map(Arc::new)
                        .map_err(|e| missing.insert(o.kind, e.to_string()))
                        .ok()
                })
                .clone()
        };
        if rules.hidden {
            // Hidden objects are not drawn, but their effects code may use the bones.
            if has_fx {
                let model = rel.as_ref().and_then(|r| load_bmd(r, &mut missing));
                let fps = model.as_ref().map_or(0.0, |m| {
                    if m.actions.first().is_some_and(|a| a.frames > 1) { rules.velocity.fps(o.scale) } else { 0.0 }
                });
                fx_world.map_objects.push(fx_object(model, fps / objects::TICKS_PER_SECOND));
                // Their effects also depend on whether `CullObjects` draws it.
                if let Some(mut f) = fade {
                    f.fx = Some(fx_world.map_objects.len() - 1);
                    login_fades.objects.push(f);
                }
            }
            continue;
        }
        let Some(rel) = rel else {
            missing.entry(o.kind).or_insert_with(|| "sin entrada en la tabla".into());
            continue;
        };
        let Some(model) = load_bmd(&rel, &mut missing) else { continue };

        // Animation group: objects of a model running at the same speed share a mesh.
        let animated = model.actions.first().is_some_and(|a| a.frames > 1);
        let fps = if animated { rules.velocity.fps(o.scale) } else { 0.0 };
        if has_fx {
            fx_world.map_objects.push(fx_object(Some(model.clone()), fps / objects::TICKS_PER_SECOND));
            if let Some(f) = fade.as_mut() {
                f.fx = Some(fx_world.map_objects.len() - 1);
            }
        }
        // Emitters attached to a bone (fountain splashes): they follow the animation.
        for e in emitters.iter().filter(|e| e.bone.is_some()) {
            fx.0.push(place(e, Some((model.clone(), fps))));
        }
        let fps_key = (fps * 1000.0).round() as u32;
        // Those whose animation is driven by their effects code (`fx_animation`).
        let fx_driven = (rules.fx_animation && has_fx).then(|| fx_world.map_objects.len() - 1);
        // The sled dog changes action on its own: its own group (and those of `fx_driven`).
        let own_group = ((rules.motion == Some(Motion::SledDog) || fx_driven.is_some()) && fps_key > 0).then_some(index);
        let (parts, group) = c.models.entry((rel.clone(), fps_key, own_group)).or_insert_with(|| {
            let dir = args.data.join(rel.parent().unwrap_or(Path::new("")));
            let parts = build_parts(&model, &dir, &mut meshes);
            let mut group = None;
            if fps_key > 0 {
                let handles = parts.iter().map(|p| (p.mesh_index, p.mesh.clone())).collect();
                let sled_dog = own_group.filter(|_| fx_driven.is_none()).map(|i| SledDog { tick: 0.0, rng: seeded_rng(i) });
                anims.0.push(AnimatedModel {
                    model: model.clone(),
                    fps,
                    frame: 0.0,
                    action: 0,
                    parts: handles,
                    bones: model.bone_matrices(0, 0),
                    sled_dog,
                    fx: fx_driven,
                });
                group = Some(anims.0.len() - 1);
            }
            (parts, group)
        });

        // Terrain light under the object, quantized so materials can be shared.
        let light = terrain
            .as_ref()
            .map(|t| {
                let x = ((o.position[0] / MU_SCALE).max(0.0) as usize).min(SIZE - 1);
                let y = ((o.position[1] / MU_SCALE).max(0.0) as usize).min(SIZE - 1);
                t.0.light[y * SIZE + x]
            })
            .unwrap_or([1.0; 3]);
        // Fixed `b->BodyLight` of some types (instead of the terrain light).
        let light = rules.body_light.map_or(light, |l| [l; 3]);
        // Ground light without `o->Light` (for meshes whose `o->Light` changes over time).
        let ground_light = light;
        // `BodyLight(o, b)` = ground light + `o->Light` (the fixed one; the varying one is in `animate_materials`).
        let fixed_add = match rules.light_add {
            Some(LightAdd::Fixed(l)) => l,
            _ => [0.0; 3],
        };
        let raw_light = [0, 1, 2].map(|k| light[k] + fixed_add[k]);
        let quantize = |l: [f32; 3]| l.map(|v| (v.clamp(0.0, 1.0) * 15.0).round() / 15.0);
        let object_light = quantize(raw_light);
        // Fixed `IntensityTransform`: lit meshes become `BodyLight · k` (OpenGL clamps it to 1).
        let intensity_light = rules.intensity.map(|k| quantize(raw_light.map(|v| v * k)));

        let draw = ObjectDraw {
            blend_mesh: rules.additive_mesh.map_or(-1, |m| m as i32),
            stream_mesh: rules.stream_mesh.map_or(-1, |m| m as i32),
            tex_coord: rules.tex_coord,
            hidden_mesh: -1,
        };
        let transform = object_transform(o);
        let mut entities = Vec::new();
        for part in parts.iter() {
            let own_image = part.texture.as_ref().and_then(|p| {
                c.images
                    .entry(p.clone())
                    .or_insert_with(|| {
                        let dir = p.parent()?;
                        let t = texture::load(dir, p.file_name()?.to_str()?).ok()?;
                        Some((images.add(to_bevy_image(&t)), t.alpha_kind()))
                    })
                    .clone()
            });
            let info = MeshInfo {
                index: part.mesh_index,
                texture_index: part.texture_index,
                script: part.script,
                has_alpha: own_image.as_ref().is_some_and(|i| i.1 != AlphaKind::Opaque),
            };
            for (pass, look) in objects::render::resolve_all(rules.passes, &draw, &info) {
                // With a fixed `IntensityTransform` the lit mesh is flat colored (`BodyLight · k`).
                let flat_lit = intensity_light.filter(|_| look.color == LookColor::Lit && look.light.is_none());
                let look = if flat_lit.is_some() { MeshLook { color: LookColor::Flat, ..look } } else { look };
                let body = flat_lit.unwrap_or(look.light.unwrap_or(object_light));
                let blend_light = look.color == LookColor::BlendLight;
                // `BlendMeshLight` of this call (`RenderMesh(..., BlendMesh, fLumi, ...)`) or the object's.
                let glow = blend_light.then(|| rules.passes.get(pass).and_then(|p| p.glow).unwrap_or(rules.glow));
                let animated_glow = glow.filter(|g| !matches!(g, Glow::Steady(_)));
                // Time-varying `o->Light`: only counts in the flat color of the `BlendMesh`.
                let moving_add = rules.light_add.filter(|l| blend_light && matches!(l, LightAdd::Wave { .. }));
                let tint = (blend_light && !login_scene).then(|| {
                    if moving_add.is_some() { (ground_light, moving_add) } else { (body, None) }
                });
                let has_effect = animated_glow.is_some() || look.wave.is_some() || moving_add.is_some();
                // Time-varying `BodyLight` and animated textures: animated, but no per-mesh material
                // needed (except with `o->Timer`, which is per object).
                let body_anim = look.body_wave.map(|w| (w, body, timer, look.color == LookColor::Lit));
                let animated_texture = look.texture.is_some_and(|b| b.frames() > 1);
                let source = match (look.textured, look.texture, &part.texture) {
                    (false, _, _) => TextureSource::None,
                    (true, Some(b), _) => TextureSource::Bitmap(b),
                    (true, None, Some(p)) => TextureSource::Own(p.clone()),
                    (true, None, None) => TextureSource::None,
                };
                // Sitting objects have their own material: they light up on hover.
                let instance = (fade.is_some() || operate.is_some() || look.body_wave.is_some_and(|w| w.timer))
                    .then_some(index);
                // The normal lit light goes in each object's `MeshTag` (not in the material), so
                // identical objects share a material and are drawn in one go. Not for those that
                // change their light from the material (own material, animated `BodyLight`).
                let tag_light = look.color == LookColor::Lit && instance.is_none() && body_anim.is_none();
                let key = MaterialKey {
                    texture: source.clone(),
                    light: if tag_light { [0; 3] } else { body.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8) },
                    blend: look.blend,
                    color: look.color,
                    chrome: look.chrome,
                    pass,
                    // Roofs do not share materials either: they fade on their own.
                    effect_owner: (has_effect || rules.fade_indoors).then(|| (rel.clone(), part.mesh_index)),
                    instance,
                };
                let material = match c.materials.get(&key) {
                    Some(m) => m.clone(),
                    None => {
                        let image = match &source {
                            TextureSource::None => None,
                            TextureSource::Own(_) => own_image.as_ref().map(|i| i.0.clone()),
                            TextureSource::Bitmap(b) => {
                                c.bitmaps.entry(*b).or_insert_with(|| load_bitmap(&args.data, *b, &mut images)).clone()
                            }
                        };
                        let mut mat = object_material(image, &look, body, glow, pass, login_scene);
                        if tag_light {
                            let chrome = mat.extension.params.chrome;
                            let dark = mat.extension.subtract;
                            mat.extension = ModelLight::lit_by_tag().with_subtract(dark);
                            mat.extension.params.chrome = chrome;
                        }
                        // `glColor4f(..., Alpha)` with `EnableAlphaTest`: blends with what is behind.
                        if look.alpha < 0.99 && look.blend == LookBlend::AlphaTest {
                            mat.base.base_color.set_alpha(look.alpha);
                            mat.base.alpha_mode = AlphaMode::Blend;
                        }
                        let original_mode = mat.base.alpha_mode;
                        let m = materials.add(mat);
                        if rules.fade_indoors {
                            roofs.materials.push((m.clone(), original_mode));
                        }
                        if has_effect || body_anim.is_some() || animated_texture {
                            let frames = match look.texture.filter(|_| animated_texture) {
                                Some(b) => c
                                    .frames
                                    .entry(b)
                                    .or_insert_with(|| (0..b.frames()).filter_map(|i| load_frame(&args.data, b, i, &mut images)).collect())
                                    .clone(),
                                None => Vec::new(),
                            };
                            mat_anims.0.push(AnimatedMaterial {
                                handle: m.clone(),
                                glow: animated_glow.or(glow),
                                glow_srgb: !login_scene,
                                wave: look.wave,
                                tint,
                                body: body_anim,
                                frames,
                            });
                        }
                        c.materials.insert(key, m.clone());
                        m
                    }
                };
                let additive = matches!(look.blend, LookBlend::Add | LookBlend::Subtract);
                let original_mode = materials.get(&material).map_or(AlphaMode::Opaque, |m| m.base.alpha_mode);
                let mut e =
                    commands.spawn((Mesh3d(part.mesh.clone()), MeshMaterial3d(material.clone()), transform, crate::scene::WorldEntity));
                if tag_light {
                    // Unquantized: the terrain light as is (the material no longer needs it).
                    let exact = if look.light.is_none() { raw_light } else { body };
                    e.insert(bevy::mesh::MeshTag::new(crate::model_light::encode_light(exact)));
                }
                let entity = e.id();
                entities.push(entity);
                if let Some(op) = operate.and_then(|i| operates.0.get_mut(i)) {
                    // Additive and self-colored meshes do not carry the object light.
                    if look.color == LookColor::Lit && look.light.is_none() {
                        op.materials.push(material.clone());
                        op.light = Vec3::from_array(object_light);
                    }
                }
                if let Some(f) = fade.as_mut() {
                    f.parts.push((entity, material, original_mode, additive));
                }
                spawned += 1;
            }
        }
        if let Some(f) = fade {
            login_fades.objects.push(f);
        }
        if let Some(i) = fx_driven {
            followers.0.push((i, entities.clone(), Vec3::from_array(o.position)));
        }
        if let Some(motion) = rules.motion.filter(|m| *m != Motion::SledDog) {
            movers.0.push(movers::Mover::new(motion, o, entities).with_timer(timer));
        }
        if let Some(hidden) = rules.body_shadow {
            let ground = terrain.as_ref().map_or(o.position[2], |t| {
                t.0.height_at(o.position[0] / MU_SCALE, o.position[1] / MU_SCALE)
            });
            shadows.add(
                &mut commands,
                &mut meshes,
                &mut fx_materials,
                movers::ShadowSource {
                    model: model.clone(),
                    group: *group,
                    // `RenderBodyShadow(o->BlendMesh, 2)`: neither mesh 2 nor the `BlendMesh`.
                    meshes: (0..model.meshes.len())
                        .filter(|&i| i != hidden && model.meshes[i].texture_index as i32 != draw.blend_mesh)
                        .collect(),
                    position: Vec3::from_array(o.position),
                    rotation: mu_rotation,
                    scale: o.scale,
                    ground,
                },
            );
        }
    }

    // Portals `MapManager.cpp` creates in code (`CreateObject(MODEL_WARP, Pos, (0,0,10))`, z = 0):
    // their rings are effects created by the object code.
    // Raklion (`MapManager.cpp:782-810`): `MODEL_WARP4` at (171, 24) rotated 80° and, on the boss map,
    // two at (169, 24) and (170, 24) rotated 85°. Raklion's `MODEL_WARP` at (162, 83) creates nothing:
    // its `CGM_Raklion::CreateObject` code is commented out (GM_Raklion.cpp:58-72).
    use crate::fx::tables::ids::{MODEL_WARP, MODEL_WARP4};
    let warps: &[(i32, f32, f32, f32)] = match args.world {
        3 => &[(MODEL_WARP, 53.0 * MU_SCALE + 50.0, 92.0 * MU_SCALE + 20.0, 10.0)],
        4 => &[(MODEL_WARP, 223.0 * MU_SCALE, 30.0 * MU_SCALE, 10.0)],
        58 => &[(MODEL_WARP4, 171.0 * MU_SCALE, 24.0 * MU_SCALE, 80.0)],
        59 => &[(MODEL_WARP4, 169.0 * MU_SCALE, 24.0 * MU_SCALE, 85.0), (MODEL_WARP4, 170.0 * MU_SCALE, 24.0 * MU_SCALE, 85.0)],
        _ => &[],
    };
    for &(ty, x, y, angle) in warps {
        fx_world.map_objects.push(crate::fx::core::MapObject {
            ty,
            position: Vec3::new(x, y, 0.0),
            angle: Vec3::new(0.0, 0.0, angle),
            scale: 1.0,
            light: Vec3::ZERO,
            model: None,
            velocity: 0.0,
            animation_frame: 0.0,
            bones: None,
            timer: 0.0,
            hidden_mesh: -1,
            user: [0.0; 4],
            created: false,
        });
    }

    info!(
        "World{}: {} objetos, {spawned} mallas, {} modelos, {} grupos animados, {} materiales ({} con efectos)",
        args.world,
        list.len(),
        c.bmds.len(),
        anims.0.len(),
        c.materials.len(),
        mat_anims.0.len(),
    );
    // Without the file the original does not draw it either (`AccessModel` skips it silently): the S6 Data
    // puts model-less types on some maps (Lost Tower 40, login 1/48/75/160-164...).
    let (absent, broken): (Vec<_>, Vec<_>) = missing.into_iter().partition(|(_, why)| why.starts_with("no existe"));
    if !absent.is_empty() {
        let mut kinds: Vec<i16> = absent.into_iter().map(|(k, _)| k).collect();
        kinds.sort_unstable();
        info!("tipos sin modelo en la Data (tampoco se ven en el original): {kinds:?}");
    }
    for (kind, why) in broken {
        warn!("objeto tipo {kind} no cargado: {why}");
    }
}

/// Each dog's own `rand()` (so they do not all do the same thing at once).
fn seeded_rng(index: usize) -> crate::fx::core::Rng {
    let mut rng = crate::fx::core::Rng::default();
    for _ in 0..(index % 97) {
        rng.rand();
    }
    rng
}

/// Creates a Bevy mesh for each model mesh (in the initial pose).
fn build_parts(model: &bmd::Bmd, dir: &Path, meshes: &mut Assets<Mesh>) -> Vec<ModelPart> {
    (0..model.meshes.len())
        .filter_map(|i| {
            let posed = model.pose(i, 0, 0);
            if posed.positions.is_empty() {
                return None;
            }
            let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, posed.positions)
                .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, posed.normals)
                .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, posed.uvs);
            let m = &model.meshes[i];
            Some(ModelPart {
                mesh: meshes.add(mesh),
                mesh_index: i,
                texture: fs::find(dir, &bmd::texture_file_name(&m.texture)),
                // The original compares the `BlendMesh` with the mesh's texture index.
                texture_index: m.texture_index as i32,
                // `BMD::Open` (`ZzzBMD.cpp:2562-2574`): the script comes from the name stored in the model.
                script: bmd::texture_script(&m.texture),
            })
        })
        .collect()
}

/// The original alpha test threshold (`glAlphaFunc(GL_GREATER, 0.25)`).
pub const ALPHA_REF: f32 = 0.25;

/// Added to `depth_bias` for each pass: the next pass over the same mesh ends up
/// in front of the previous one, as with `GL_LEQUAL` (depth buffer units: something tiny).
const PASS_DEPTH_BIAS: f32 = 2.0;

/// Material of a mesh in a pass (`MeshLook`), with `body` = its `BodyLight` and `glow` = its
/// `BlendMeshLight` if it is the `BlendMesh`.
pub(crate) fn object_material(
    image: Option<Handle<Image>>,
    look: &MeshLook,
    body: [f32; 3],
    glow: Option<Glow>,
    pass: usize,
    login_scene: bool,
) -> ModelMaterial {
    let alpha_mode = match look.blend {
        LookBlend::Opaque => AlphaMode::Opaque,
        // The original draws alpha textures with alpha test (> 0.25) and depth
        // writes (`EnableAlphaTest`). Blending without depth makes plants and leaves
        // paint over what is in front of them.
        LookBlend::AlphaTest => AlphaMode::Mask(ALPHA_REF),
        // Subtract changes the blending in `ModelLight::specialize`.
        LookBlend::Add | LookBlend::Subtract => AlphaMode::Add,
    };
    // Initial `BlendMeshLight` (the varying ones are in `animate_materials`).
    let g = match glow {
        Some(Glow::Steady(g)) => g,
        Some(Glow::Flicker { max, .. }) => max,
        Some(Glow::Pulse { amp, base, .. } | Glow::AbsPulse { amp, base, .. }) => base + amp,
        None => 1.0,
    };
    // Outside the login the `BlendMesh`'s `BlendMeshLight` goes in the tint with `BodyLight`
    // (`glColor3f(BodyLight · BlendMeshLight)`, which OpenGL clamps to [0, 1]); at login, in the base color.
    let glow_in_tint = look.color == LookColor::BlendLight && !login_scene;
    let base = StandardMaterial {
        base_color: if glow_in_tint { Color::WHITE } else { glow_color(g, !login_scene) },
        base_color_texture: image,
        unlit: true,
        cull_mode: None,
        alpha_mode,
        depth_bias: pass as f32 * PASS_DEPTH_BIAS,
        ..default()
    };
    let light = match look.color {
        // Terrain light times the light from each vertex normal (`model_light.wesl`).
        LookColor::Lit => ModelLight::lit(body),
        // `glColor3fv(BodyLight)`.
        LookColor::Flat => ModelLight::flat(body),
        // `glColor3f(BodyLight · BlendMeshLight)`. At login and character selection the glow
        // is handled by their own systems, as before.
        LookColor::BlendLight if login_scene => ModelLight::unlit(),
        LookColor::BlendLight => ModelLight::flat(body.map(|v| v * g)),
    };
    model_material(
        base,
        light.with_chrome(look.chrome).with_subtract(look.blend == LookBlend::Subtract),
    )
}

/// Object position/rotation/scale in MU, converted to Bevy.
fn object_transform(o: &objects::MapObject) -> Transform {
    mu_transform(Vec3::from_array(o.position), Vec3::from_array(o.angle), o.scale)
}

/// Position (MU), angles (degrees) and scale of an object, in Bevy.
///
/// MU: Z up, 100 units per tile, angles in degrees applied X→Y→Z.
/// Bevy: Y up, 1 unit per tile. The axis change is a -90° rotation about X.
pub(crate) fn mu_transform(position: Vec3, angle: Vec3, scale: f32) -> Transform {
    let to_bevy = Quat::from_rotation_x(-FRAC_PI_2);
    let [ax, ay, az] = angle.to_array().map(f32::to_radians);
    let rot = Quat::from_euler(EulerRot::ZYX, az, ay, ax);
    Transform {
        translation: to_bevy * position / MU_SCALE,
        rotation: to_bevy * rot,
        scale: Vec3::splat(scale / MU_SCALE),
    }
}
