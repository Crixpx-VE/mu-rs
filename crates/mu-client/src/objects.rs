//! Objetos del mapa (árboles, casas, murallas...) a partir de `EncTerrainN.obj` + modelos `.bmd`.
//!
//! Cada modelo se carga una sola vez; cada objeto del mapa es una instancia con su
//! posición, rotación y escala.
//!
//! Animación: como en el original, todos los objetos empiezan en el fotograma 0, así que los
//! que van a la misma velocidad están siempre en la misma pose. Se agrupan por (modelo,
//! velocidad) y cada grupo comparte una malla que se recalcula en CPU una vez por fotograma.
//!
//! Efectos de material (del original): mallas aditivas que parpadean como el fuego o laten,
//! y texturas que se desplazan (agua que cae, cascadas).
//!
//! Cómo se dibuja cada malla sale de las pasadas del tipo (`ModelRules::passes`: las llamadas a
//! `RenderBody`/`RenderMesh` de su código de dibujo) y de los guiones de textura (`_R`, `_H`, `_S`, `_N`),
//! con `mu_data::objects::render::resolve_all`: cromado, resta (`RENDER_DARK`), suma (`RENDER_BRIGHT`,
//! `BlendMesh`), `StreamMesh` y texturas puestas en lugar de la de la malla. Cada pasada de cada malla es
//! una entidad con la misma malla; como el original dibuja con `GL_LEQUAL` (la última pasada tapa a las
//! anteriores en el mismo sitio), cada pasada lleva un poco más de `depth_bias` que la anterior.
//!
//! Lo que se mueve por su cuenta (puertas de Devias, calaveras de Lost Tower) y las sombras de
//! `RenderBodyShadow` están en [`movers`].

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
        // Después del terreno, para consultar su luz.
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

/// Materiales de los techos, que se desvanecen al entrar en una casa.
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

/// `Alpha()` del original: `alpha += (objetivo - alpha) · 0.1` en cada tick (25 por segundo),
/// pasado a tiempo real para que tarde lo mismo a cualquier fps.
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
            // Opaco del todo: como siempre. Mientras se desvanece: con transparencia.
            m.base.alpha_mode = if alpha >= 1.0 { *original } else { AlphaMode::Blend };
        }
    }
}

/// Al cambiar de mapa: nada del anterior.
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

/// Fundido de los objetos del login (`WD_73NEW_LOGIN_SCENE`) según su distancia a la cámara.
///
/// `CullObjects` (`ZzzObject.cpp` ≈4205-4246), en cada fotograma, con la distancia en el plano
/// (x, y) de la cámara al objeto (`TestFrustrum2D` siempre es verdadero en `LOG_IN_SCENE`):
/// - Tipos 122-124, 126, 127, 129 y 159 a menos de 2·`RENDER_OBJECT_DIST`: `BlendMeshLight` y
///   `AlphaTarget` suben 0.03·`FPS_ANIMATION_FACTOR` hasta 1, y se dibuja.
/// - El resto (tipo ≤ `MAX_WORLD_OBJECTS`) a menos de `RENDER_OBJECT_DIST`: sube `AlphaTarget` igual.
/// - Si no, y `AlphaTarget != 0` y está más lejos que `RENDER_OBJECT_DIST`: `BlendMeshLight`,
///   `Alpha` y `AlphaTarget` a 0. No se dibuja (ni suelta efectos).
///
/// En cada tick `MoveObject` hace `Alpha(o)` (`Alpha += (AlphaTarget − Alpha)·0.1` y
/// `BlendMeshLight ≤ Alpha`) y, si `Alpha ≥ 0.01`, otra vez en `GMEmpireGuardian4::MoveObject`. Con
/// `Alpha < 0.01` no se dibuja; por debajo de 1 se dibuja mezclando (y la prueba de alfa del original,
/// > 0.25, lo quita antes). Las mallas que suman no usan `Alpha`: las de guion `_R` (127, 157) brillan
/// con `BlendMeshLight`; las de `RENDER_BRIGHT` (37, 70) no cambian.
#[derive(Resource, Default)]
pub struct LoginFades {
    objects: Vec<LoginFade>,
    /// Tiempo acumulado hacia el próximo tick (25 por segundo).
    tick: f32,
}

struct LoginFade {
    kind: i16,
    /// Posición (x, y) en MU.
    position: Vec2,
    alpha: f32,
    /// `Alpha` del tick anterior, para interpolar al dibujar.
    prev_alpha: f32,
    alpha_target: f32,
    blend_light: f32,
    /// (entidad, material propio, modo de alfa original, suma).
    parts: Vec<(Entity, Handle<ModelMaterial>, AlphaMode, bool)>,
    /// Índice en `FxWorld::map_objects` si tiene código de efectos.
    fx: Option<usize>,
    /// Lo último que se puso en los materiales (para no tocarlos si no cambia).
    shown: Option<(bool, f32, f32)>,
}

/// Tipos con malla de guion `_R` (su propia `BlendMesh`, que brilla con `BlendMeshLight`).
const LOGIN_SCRIPT_BRIGHT: [i16; 2] = [127, 157];

/// `BodyLight` de los objetos con material propio (login y selección): la luz del suelo en su sitio,
/// con las luces dinámicas de este tick (fuegos, `AddTerrainLight`), como `RequestModelTerrainLight`.
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
        // Luz de cada esquina de la casilla con las luces dinámicas, y luego interpolada.
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
    // `FPS_ANIMATION_FACTOR`: lo que sube cada fotograma va en proporción a los ticks que dura.
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
            // `Alpha(o)` de `MoveObject` y, si sigue, el de `GMEmpireGuardian4::MoveObject`.
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
        // Se redondea para no rehacer los materiales por cambios que no se ven.
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

/// Grupos de objetos animados (árboles, banderas, molino...).
#[derive(Resource, Default)]
pub struct AnimatedModels(Vec<AnimatedModel>);

struct AnimatedModel {
    model: Arc<bmd::Bmd>,
    /// Fotogramas de animación por segundo.
    fps: f32,
    frame: f32,
    /// `o->CurrentAction` (solo cambia en los que tienen [`SledDog`]).
    action: usize,
    /// (índice de malla en el .bmd, malla compartida por las instancias del grupo)
    parts: Vec<(usize, Handle<Mesh>)>,
    /// Huesos de la pose de este fotograma (para las sombras de `movers`).
    bones: Vec<bmd::Mat34>,
    /// Perro del trineo de Devias (un grupo propio por perro).
    sled_dog: Option<SledDog>,
    /// Objetos con `ModelRules::fx_animation` (un grupo propio por objeto): la acción y el fotograma
    /// los lleva su código de efectos en `FxWorld::map_objects[i]` (ver [`fx_anim`]).
    fx: Option<usize>,
}

/// Campos libres de `fx::core::MapObject::user` con los que el código de efectos de un objeto con
/// `ModelRules::fx_animation` le dice a este módulo qué dibujar (los escribe en cada tick).
pub mod fx_anim {
    /// `o->AnimationFrame` al acabar el tick.
    pub const FRAME: usize = 0;
    /// `o->CurrentAction` (si no existe en el modelo, `BMD::Animation` usa la 0).
    pub const ACTION: usize = 1;
    /// `o->AnimationFrame` del tick anterior (para interpolar entre ticks).
    pub const PREV_FRAME: usize = 2;
}

/// Entidades de los objetos con `ModelRules::fx_animation`: siguen la posición de su
/// `FxWorld::map_objects[i]` (p. ej. la luz que sube y baja de Refuge, tipo 84).
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

/// `RenderObjectVisual`, Devias 103 (`ZzzObject.cpp:2824-2831`): en el último fotograma de la acción,
/// `rand()%32 == 0` → `SetAction(o, 1)`, si no `SetAction(o, 0)` (que reinicia la animación si cambia).
struct SledDog {
    tick: f32,
    rng: crate::fx::core::Rng,
}

/// Materiales que cambian con el tiempo: brillo de las mallas aditivas y texturas que se mueven.
#[derive(Resource, Default)]
pub struct AnimatedMaterials(Vec<AnimatedMaterial>);

struct AnimatedMaterial {
    handle: Handle<ModelMaterial>,
    /// `BlendMeshLight` que cambia (parpadeo, pulso).
    glow: Option<Glow>,
    /// Como lo multiplica el original (en su espacio de color) o, en el login, tal cual.
    glow_srgb: bool,
    /// `BlendMeshTexCoordU/V`.
    wave: Option<[Saw; 2]>,
    /// Malla `BlendMesh` fuera del login: `glColor3f(BodyLight · BlendMeshLight)` va en el tinte (que
    /// el sombreador limita a [0, 1] como OpenGL), con la luz del suelo y el `o->Light` del objeto.
    tint: Option<([f32; 3], Option<LightAdd>)>,
    /// `BodyLight` que cambia con el tiempo ([`BodyWave`]): la onda, la luz del objeto, su `o->Timer`
    /// y si la malla va iluminada por la normal (si no, es el color plano).
    body: Option<(BodyWave, [f32; 3], f32, bool)>,
    /// Textura animada (`BITMAP_WATER + WaterTextureNumber`): un fotograma por tick.
    frames: Vec<Handle<Image>>,
}

/// `BlendMeshLight` como color del material (el original multiplica en sRGB).
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
            // Lo que dejó el código de efectos en el último tick, interpolado hacia el siguiente
            // (salvo cuando vuelve atrás: vuelta de la animación o `SetAction`).
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
                    // `SetAction`: solo si existe la acción y es otra.
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
    // `WorldTime` del original, en ms.
    let world_time = (time.elapsed_secs_f64() * 1000.0) as f32;
    // El parpadeo del original cambia en cada actualización de su lógica (25 por segundo).
    *tick += time.delta_secs();
    let new_tick = *tick >= 1.0 / objects::TICKS_PER_SECOND;
    if new_tick {
        *tick = 0.0;
    }
    for a in &anims.0 {
        let glow = match a.glow {
            None | Some(Glow::Steady(_)) => None,
            Some(Glow::Flicker { min, max }) => new_tick.then(|| min + (max - min) * random(&mut rng)),
            Some(Glow::Pulse { freq, amp, base }) => Some((t * freq).sin() * amp + base),
        };
        // `o->Light` que cambia con el tiempo (se suma a la luz del suelo).
        let moving_light = a.tint.is_some_and(|(_, add)| matches!(add, Some(LightAdd::Wave { .. })));
        // `WaterTextureNumber`: avanza uno por tick, de 0 a 31.
        let frame = (!a.frames.is_empty()).then(|| (time.elapsed_secs_f64() * objects::TICKS_PER_SECOND as f64) as usize % a.frames.len());
        let new_frame = frame.is_some() && new_tick;
        // La `BodyLight` que late, una vez por tick como el original (hay cientos de objetos con fase
        // propia en Land of Trials: cambiarlos en cada fotograma no se notaría y costaría).
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

/// Número pseudoaleatorio en [0, 1) (xorshift; no hace falta nada mejor para un parpadeo).
fn random(state: &mut u32) -> f32 {
    if *state == 0 {
        *state = 0x9E37_79B9;
    }
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    (*state >> 8) as f32 / (1u32 << 24) as f32
}

/// Una malla de un modelo, lista para instanciar.
struct ModelPart {
    mesh: Handle<Mesh>,
    mesh_index: usize,
    /// Ruta completa de la textura (clave de caché).
    texture: Option<PathBuf>,
    /// `m->Texture`.
    texture_index: i32,
    /// Guion del nombre de la textura (`_R`, `_H`, `_S`, `_N`).
    script: Option<TextureScript>,
}

/// De dónde sale la textura de una pasada.
#[derive(Hash, PartialEq, Eq, Clone)]
enum TextureSource {
    None,
    Own(PathBuf),
    Bitmap(Bitmap),
}

#[derive(Hash, PartialEq, Eq)]
struct MaterialKey {
    texture: TextureSource,
    /// `BodyLight` de la pasada (cuantizada).
    light: [u8; 3],
    blend: LookBlend,
    color: LookColor,
    chrome: Option<ChromeMode>,
    /// Orden de la pasada (`depth_bias`).
    pass: usize,
    /// Los materiales con efectos son propios de su (modelo, malla): no se comparten.
    effect_owner: Option<(PathBuf, usize)>,
    /// Los que se funden por su cuenta (objetos del login) son propios de su objeto.
    instance: Option<usize>,
}

#[derive(Default)]
struct Caches {
    bmds: HashMap<PathBuf, Option<Arc<bmd::Bmd>>>,
    /// (modelo, velocidad en milésimas de fotograma/s; 0 = estático, objeto con grupo propio) → mallas
    models: HashMap<(PathBuf, u32, Option<usize>), (Vec<ModelPart>, Option<usize>)>,
    images: HashMap<PathBuf, Option<(Handle<Image>, AlphaKind)>>,
    bitmaps: HashMap<Bitmap, Option<Handle<Image>>>,
    /// Los fotogramas de las texturas animadas (`BITMAP_WATER`).
    frames: HashMap<Bitmap, Vec<Handle<Image>>>,
    materials: HashMap<MaterialKey, Handle<ModelMaterial>>,
}

/// Carga una textura global (`BITMAP_CHROME`...) con el filtro y el modo de repetición del original.
fn load_bitmap(data: &Path, b: Bitmap, images: &mut Assets<Image>) -> Option<Handle<Image>> {
    load_frame(data, b, 0, images)
}

/// El fotograma `i` de una textura global (el único, salvo en las animadas: [`Bitmap::frame_path`]).
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
    // En el login cada objeto se funde por su cuenta según la distancia a la cámara.
    let login = args.world == crate::scene::LOGIN_WORLD;
    // Login y selección de personaje: su brillo aditivo lo lleva `LoginFades`/`charselect` como siempre.
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
        // `o->Timer` de `CreateObject` (las luces que laten de Land of Trials).
        let timer = objects::create_timer(args.world, o.kind, index);
        // Bancos, sillas y cajas de pose (`CreateOperate`), también los invisibles.
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
        // Objetos con código de efectos (`MoveObject`/`UpdateObjectVisual`) para el motor.
        let world_active = args.world as i32 - 1;
        let has_fx = registry.objects.contains_key(&(world_active, o.kind as i32));
        // Los emisores provisionales solo quedan para los objetos que el motor aún no traduce.
        let emitters = if has_fx { Vec::new() } else { emitters };
        // Los emisores sin hueso (fuegos, humo) no necesitan el modelo: también los ocultos.
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
            // Los ocultos no se dibujan, pero su código de efectos puede usar los huesos.
            if has_fx {
                let model = rel.as_ref().and_then(|r| load_bmd(r, &mut missing));
                let fps = model.as_ref().map_or(0.0, |m| {
                    if m.actions.first().is_some_and(|a| a.frames > 1) { rules.velocity.fps(o.scale) } else { 0.0 }
                });
                fx_world.map_objects.push(fx_object(model, fps / objects::TICKS_PER_SECOND));
                // Sus efectos también dependen de si `CullObjects` lo dibuja.
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

        // Grupo de animación: los objetos de un modelo que van a la misma velocidad comparten malla.
        let animated = model.actions.first().is_some_and(|a| a.frames > 1);
        let fps = if animated { rules.velocity.fps(o.scale) } else { 0.0 };
        if has_fx {
            fx_world.map_objects.push(fx_object(Some(model.clone()), fps / objects::TICKS_PER_SECOND));
            if let Some(f) = fade.as_mut() {
                f.fx = Some(fx_world.map_objects.len() - 1);
            }
        }
        // Emisores pegados a un hueso (salpicaduras de la fuente): siguen la animación.
        for e in emitters.iter().filter(|e| e.bone.is_some()) {
            fx.0.push(place(e, Some((model.clone(), fps))));
        }
        let fps_key = (fps * 1000.0).round() as u32;
        // Los que llevan la animación desde su código de efectos (`fx_animation`).
        let fx_driven = (rules.fx_animation && has_fx).then(|| fx_world.map_objects.len() - 1);
        // El perro del trineo cambia de acción por su cuenta: su propio grupo (y los de `fx_driven`).
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

        // Luz del terreno bajo el objeto, cuantizada para poder compartir materiales.
        let light = terrain
            .as_ref()
            .map(|t| {
                let x = ((o.position[0] / MU_SCALE).max(0.0) as usize).min(SIZE - 1);
                let y = ((o.position[1] / MU_SCALE).max(0.0) as usize).min(SIZE - 1);
                t.0.light[y * SIZE + x]
            })
            .unwrap_or([1.0; 3]);
        // `b->BodyLight` fijo de algunos tipos (en vez de la luz del terreno).
        let light = rules.body_light.map_or(light, |l| [l; 3]);
        // Luz del suelo sin `o->Light` (para las mallas cuyo `o->Light` cambia con el tiempo).
        let ground_light = light;
        // `BodyLight(o, b)` = luz del suelo + `o->Light` (el fijo; el que cambia lo lleva `animate_materials`).
        let fixed_add = match rules.light_add {
            Some(LightAdd::Fixed(l)) => l,
            _ => [0.0; 3],
        };
        let raw_light = [0, 1, 2].map(|k| light[k] + fixed_add[k]);
        let quantize = |l: [f32; 3]| l.map(|v| (v.clamp(0.0, 1.0) * 15.0).round() / 15.0);
        let object_light = quantize(raw_light);
        // `IntensityTransform` fijo: las mallas iluminadas quedan `BodyLight · k` (OpenGL lo limita a 1).
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
                // Con `IntensityTransform` fijo la malla iluminada es de color plano (`BodyLight · k`).
                let flat_lit = intensity_light.filter(|_| look.color == LookColor::Lit && look.light.is_none());
                let look = if flat_lit.is_some() { MeshLook { color: LookColor::Flat, ..look } } else { look };
                let body = flat_lit.unwrap_or(look.light.unwrap_or(object_light));
                let blend_light = look.color == LookColor::BlendLight;
                // `BlendMeshLight` de esta llamada (`RenderMesh(..., BlendMesh, fLumi, ...)`) o el del objeto.
                let glow = blend_light.then(|| rules.passes.get(pass).and_then(|p| p.glow).unwrap_or(rules.glow));
                let animated_glow = glow.filter(|g| !matches!(g, Glow::Steady(_)));
                // `o->Light` que cambia con el tiempo: solo cuenta en el color plano de la `BlendMesh`.
                let moving_add = rules.light_add.filter(|l| blend_light && matches!(l, LightAdd::Wave { .. }));
                let tint = (blend_light && !login_scene).then(|| {
                    if moving_add.is_some() { (ground_light, moving_add) } else { (body, None) }
                });
                let has_effect = animated_glow.is_some() || look.wave.is_some() || moving_add.is_some();
                // `BodyLight` que cambia con el tiempo y texturas animadas: se animan, pero no hace falta
                // un material por malla (salvo con `o->Timer`, que es de cada objeto).
                let body_anim = look.body_wave.map(|w| (w, body, timer, look.color == LookColor::Lit));
                let animated_texture = look.texture.is_some_and(|b| b.frames() > 1);
                let source = match (look.textured, look.texture, &part.texture) {
                    (false, _, _) => TextureSource::None,
                    (true, Some(b), _) => TextureSource::Bitmap(b),
                    (true, None, Some(p)) => TextureSource::Own(p.clone()),
                    (true, None, None) => TextureSource::None,
                };
                // Los de sentarse tienen material propio: se iluminan con el ratón encima.
                let instance = (fade.is_some() || operate.is_some() || look.body_wave.is_some_and(|w| w.timer))
                    .then_some(index);
                // La luz iluminada normal va en el `MeshTag` de cada objeto (no en el material), para
                // que los objetos iguales compartan material y se dibujen de una vez. No en los que
                // cambian su luz desde el material (propios, `BodyLight` animada).
                let tag_light = look.color == LookColor::Lit && instance.is_none() && body_anim.is_none();
                let key = MaterialKey {
                    texture: source.clone(),
                    light: if tag_light { [0; 3] } else { body.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8) },
                    blend: look.blend,
                    color: look.color,
                    chrome: look.chrome,
                    pass,
                    // Los techos tampoco comparten material: se desvanecen por su cuenta.
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
                        // `glColor4f(..., Alpha)` con `EnableAlphaTest`: mezcla con lo de detrás.
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
                    // Sin cuantizar: la luz del terreno tal cual (el material ya no la necesita).
                    let exact = if look.light.is_none() { raw_light } else { body };
                    e.insert(bevy::mesh::MeshTag::new(crate::model_light::encode_light(exact)));
                }
                let entity = e.id();
                entities.push(entity);
                if let Some(op) = operate.and_then(|i| operates.0.get_mut(i)) {
                    // Las mallas aditivas y las de color propio no llevan la luz del objeto.
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
                    // `RenderBodyShadow(o->BlendMesh, 2)`: ni la malla 2 ni la `BlendMesh`.
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

    // Portales que `MapManager.cpp` crea por código (`CreateObject(MODEL_WARP, Pos, (0,0,10))`, z = 0):
    // sus anillos son efectos que crea el código del objeto.
    // Raklion (`MapManager.cpp:782-810`): `MODEL_WARP4` en (171, 24) girado 80° y, en el mapa del jefe,
    // dos en (169, 24) y (170, 24) girados 85°. El `MODEL_WARP` de (162, 83) de Raklion no crea nada:
    // su código de `CGM_Raklion::CreateObject` está comentado (GM_Raklion.cpp:58-72).
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
    // Sin archivo, el original tampoco lo dibuja (`AccessModel` lo salta sin avisar): la Data del S6
    // pone en algunos mapas tipos sin modelo (Lost Tower 40, login 1/48/75/160-164...).
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

/// `rand()` propio de cada perro (para que no hagan todos lo mismo a la vez).
fn seeded_rng(index: usize) -> crate::fx::core::Rng {
    let mut rng = crate::fx::core::Rng::default();
    for _ in 0..(index % 97) {
        rng.rand();
    }
    rng
}

/// Crea una malla de Bevy por cada malla del modelo (en la pose inicial).
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
                // El original compara la `BlendMesh` con el índice de textura de la malla.
                texture_index: m.texture_index as i32,
                // `BMD::Open` (`ZzzBMD.cpp:2562-2574`): el guion sale del nombre guardado en el modelo.
                script: bmd::texture_script(&m.texture),
            })
        })
        .collect()
}

/// Umbral de la prueba de alfa del original (`glAlphaFunc(GL_GREATER, 0.25)`).
pub const ALPHA_REF: f32 = 0.25;

/// Lo que se suma a `depth_bias` por cada pasada: la siguiente pasada sobre la misma malla queda
/// delante de la anterior, como con `GL_LEQUAL` (unidades del búfer de profundidad: algo minúsculo).
const PASS_DEPTH_BIAS: f32 = 2.0;

/// Material de una malla en una pasada (`MeshLook`), con `body` = su `BodyLight` y `glow` = su
/// `BlendMeshLight` si es la `BlendMesh`.
fn object_material(
    image: Option<Handle<Image>>,
    look: &MeshLook,
    body: [f32; 3],
    glow: Option<Glow>,
    pass: usize,
    login_scene: bool,
) -> ModelMaterial {
    let alpha_mode = match look.blend {
        LookBlend::Opaque => AlphaMode::Opaque,
        // El original dibuja las texturas con alfa con prueba de alfa (> 0.25) y escribiendo
        // profundidad (`EnableAlphaTest`). Mezclar sin profundidad hace que plantas y hojas
        // se pinten por encima de lo que tienen delante.
        LookBlend::AlphaTest => AlphaMode::Mask(ALPHA_REF),
        // La resta cambia la mezcla en `ModelLight::specialize`.
        LookBlend::Add | LookBlend::Subtract => AlphaMode::Add,
    };
    // `BlendMeshLight` inicial (los que cambian los lleva `animate_materials`).
    let g = match glow {
        Some(Glow::Steady(g)) => g,
        Some(Glow::Flicker { max, .. }) => max,
        Some(Glow::Pulse { amp, base, .. }) => base + amp,
        None => 1.0,
    };
    // Fuera del login el `BlendMeshLight` de la `BlendMesh` va en el tinte con `BodyLight`
    // (`glColor3f(BodyLight · BlendMeshLight)`, que OpenGL limita a [0, 1]); en el login, en el color base.
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
        // La luz del terreno por la de la normal de cada vértice (`model_light.wesl`).
        LookColor::Lit => ModelLight::lit(body),
        // `glColor3fv(BodyLight)`.
        LookColor::Flat => ModelLight::flat(body),
        // `glColor3f(BodyLight · BlendMeshLight)`. En el login y la selección de personaje el brillo
        // lo llevan sus propios sistemas, como antes.
        LookColor::BlendLight if login_scene => ModelLight::unlit(),
        LookColor::BlendLight => ModelLight::flat(body.map(|v| v * g)),
    };
    model_material(
        base,
        light.with_chrome(look.chrome).with_subtract(look.blend == LookBlend::Subtract),
    )
}

/// Posición/rotación/escala del objeto en MU, convertida a Bevy.
fn object_transform(o: &objects::MapObject) -> Transform {
    mu_transform(Vec3::from_array(o.position), Vec3::from_array(o.angle), o.scale)
}

/// Posición (MU), ángulos (grados) y escala de un objeto, en Bevy.
///
/// MU: Z arriba, 100 unidades por casilla, ángulos en grados aplicados X→Y→Z.
/// Bevy: Y arriba, 1 unidad por casilla. El cambio de ejes es un giro de -90° en X.
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
