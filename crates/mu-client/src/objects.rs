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

use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::math::Affine2;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;
use mu_data::objects::{Glow, ModelRules};
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
            .add_systems(Update, spawn_objects.run_if(crate::scene::load_step(crate::scene::Step::Objects)))
            .add_systems(Update, (animate_objects, animate_materials, fade_roofs, fade_login_objects, light_login_objects));
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
) {
    anims.0.clear();
    mats.0.clear();
    *roofs = Roofs::default();
    *login = LoginFades::default();
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
    cameras: Query<&GlobalTransform, With<Camera3d>>,
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
    /// (índice de malla en el .bmd, malla compartida por las instancias del grupo)
    parts: Vec<(usize, Handle<Mesh>)>,
}

/// Materiales que cambian con el tiempo: brillo de las mallas aditivas y texturas que se mueven.
#[derive(Resource, Default)]
pub struct AnimatedMaterials(Vec<AnimatedMaterial>);

struct AnimatedMaterial {
    handle: Handle<ModelMaterial>,
    glow: Glow,
    /// Unidades de textura por segundo (u, v).
    scroll: [f32; 2],
}

fn animate_objects(time: Res<Time>, mut anims: ResMut<AnimatedModels>, mut meshes: ResMut<Assets<Mesh>>) {
    for a in &mut anims.0 {
        a.frame += a.fps * time.delta_secs();
        let bones = a.model.bone_matrices_at(0, a.frame);
        for (i, handle) in &a.parts {
            let posed = a.model.pose_with(*i, &bones);
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
    // El parpadeo del original cambia en cada actualización de su lógica (25 por segundo).
    *tick += time.delta_secs();
    let new_tick = *tick >= 1.0 / objects::TICKS_PER_SECOND;
    if new_tick {
        *tick = 0.0;
    }
    for a in &anims.0 {
        let glow = match a.glow {
            Glow::Steady(_) if a.scroll == [0.0; 2] => continue,
            Glow::Flicker { .. } if !new_tick && a.scroll == [0.0; 2] => continue,
            Glow::Steady(g) => Some(g),
            Glow::Flicker { min, max } => new_tick.then(|| min + (max - min) * random(&mut rng)),
            Glow::Pulse { freq, amp, base } => Some((t * freq).sin() * amp + base),
        };
        let Some(mut m) = materials.get_mut(&a.handle) else { continue };
        if let Some(g) = glow {
            m.base.base_color = Color::linear_rgb(g, g, g);
        }
        if a.scroll != [0.0; 2] {
            let offset = Vec2::new((a.scroll[0] * t).fract(), (a.scroll[1] * t).fract());
            m.base.uv_transform = Affine2::from_translation(offset);
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
    /// Se dibuja sumando su color (luces, fuego, agua).
    additive: bool,
}

#[derive(Hash, PartialEq, Eq)]
struct MaterialKey {
    texture: Option<PathBuf>,
    light: [u8; 3],
    additive: bool,
    /// Los materiales con efectos son propios de su (modelo, malla): no se comparten.
    effect_owner: Option<(PathBuf, usize)>,
    /// Los que se funden por su cuenta (objetos del login) son propios de su objeto.
    instance: Option<usize>,
}

#[derive(Default)]
struct Caches {
    bmds: HashMap<PathBuf, Option<Arc<bmd::Bmd>>>,
    /// (modelo, velocidad en milésimas de fotograma/s; 0 = estático) → mallas
    models: HashMap<(PathBuf, u32), Vec<ModelPart>>,
    images: HashMap<PathBuf, Option<(Handle<Image>, AlphaKind)>>,
    materials: HashMap<MaterialKey, Handle<ModelMaterial>>,
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
) {
    // En el login cada objeto se funde por su cuenta según la distancia a la cámara.
    let login = args.world == crate::scene::LOGIN_WORLD;
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
        let parts = c.models.entry((rel.clone(), fps_key)).or_insert_with(|| {
            let dir = args.data.join(rel.parent().unwrap_or(Path::new("")));
            let parts = build_parts(&model, &dir, rules.additive_mesh, &mut meshes);
            if fps_key > 0 {
                let handles = parts.iter().map(|p| (p.mesh_index, p.mesh.clone())).collect();
                anims.0.push(AnimatedModel { model: model.clone(), fps, frame: 0.0, parts: handles });
            }
            parts
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
        let light = light.map(|v| (v.clamp(0.0, 1.0) * 15.0).round() as u8);

        let transform = object_transform(o);
        for part in parts.iter() {
            let effect = part_effect(&rules, part);
            let key = MaterialKey {
                texture: part.texture.clone(),
                light,
                additive: part.additive,
                // Los techos tampoco comparten material: se desvanecen por su cuenta.
                effect_owner: (effect.is_some() || rules.fade_indoors).then(|| (rel.clone(), part.mesh_index)),
                instance: fade.is_some().then_some(index),
            };
            let material = match c.materials.get(&key) {
                Some(m) => m.clone(),
                None => {
                    let image = part.texture.as_ref().and_then(|p| {
                        c.images
                            .entry(p.clone())
                            .or_insert_with(|| {
                                let dir = p.parent()?;
                                let t = texture::load(dir, p.file_name()?.to_str()?).ok()?;
                                Some((images.add(to_bevy_image(&t)), t.alpha_kind()))
                            })
                            .clone()
                    });
                    let mat = object_material(image, part.additive, light);
                    let original_mode = mat.base.alpha_mode;
                    let m = materials.add(mat);
                    if rules.fade_indoors {
                        roofs.materials.push((m.clone(), original_mode));
                    }
                    if let Some((glow, scroll)) = effect {
                        mat_anims.0.push(AnimatedMaterial { handle: m.clone(), glow, scroll });
                    }
                    c.materials.insert(key, m.clone());
                    m
                }
            };
            let original_mode = materials.get(&material).map_or(AlphaMode::Opaque, |m| m.base.alpha_mode);
            let entity = commands
                .spawn((Mesh3d(part.mesh.clone()), MeshMaterial3d(material.clone()), transform, crate::scene::WorldEntity))
                .id();
            if let Some(f) = fade.as_mut() {
                f.parts.push((entity, material, original_mode, part.additive));
            }
            spawned += 1;
        }
        if let Some(f) = fade {
            login_fades.objects.push(f);
        }
    }

    // Portales que `MapManager.cpp` crea por código (`CreateObject(MODEL_WARP, Pos, (0,0,10))`, z = 0):
    // sus anillos son efectos que crea el código del objeto.
    let warp = match args.world {
        3 => Some([53.0 * MU_SCALE + 50.0, 92.0 * MU_SCALE + 20.0]),
        4 => Some([223.0 * MU_SCALE, 30.0 * MU_SCALE]),
        _ => None,
    };
    if let Some([x, y]) = warp {
        fx_world.map_objects.push(crate::fx::core::MapObject {
            ty: crate::fx::tables::ids::MODEL_WARP,
            position: Vec3::new(x, y, 0.0),
            angle: Vec3::new(0.0, 0.0, 10.0),
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
    for (kind, why) in missing {
        warn!("objeto tipo {kind} no cargado: {why}");
    }
}

/// Efecto de material de una malla, si tiene: (brillo, desplazamiento de textura por segundo).
fn part_effect(rules: &ModelRules, part: &ModelPart) -> Option<(Glow, [f32; 2])> {
    let scroll = rules.scroll.filter(|(m, _)| *m == part.mesh_index).map_or([0.0; 2], |(_, s)| s);
    let glow = if part.additive { rules.glow } else { Glow::Steady(1.0) };
    (glow != Glow::Steady(1.0) || scroll != [0.0; 2]).then_some((glow, scroll))
}

/// Crea una malla de Bevy por cada malla del modelo (en la pose inicial).
fn build_parts(model: &bmd::Bmd, dir: &Path, additive_mesh: Option<usize>, meshes: &mut Assets<Mesh>) -> Vec<ModelPart> {
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
            Some(ModelPart {
                mesh: meshes.add(mesh),
                mesh_index: i,
                texture: fs::find(dir, &bmd::texture_file_name(&model.meshes[i].texture)),
                // El original compara con el índice de textura de la malla, no con el de malla.
                additive: additive_mesh.is_some_and(|a| model.meshes[i].texture_index as usize == a),
            })
        })
        .collect()
}

/// Umbral de la prueba de alfa del original (`glAlphaFunc(GL_GREATER, 0.25)`).
pub const ALPHA_REF: f32 = 0.25;

fn object_material(image: Option<(Handle<Image>, AlphaKind)>, additive: bool, light: [u8; 3]) -> ModelMaterial {
    // La luz del terreno por la de la normal de cada vértice (`model_light.wesl`).
    let l = light.map(|v| v as f32 / 15.0);
    let alpha_mode = match (additive, image.as_ref().map(|i| i.1)) {
        (true, _) => AlphaMode::Add,
        // El original dibuja las texturas con alfa con prueba de alfa (> 0.25) y escribiendo
        // profundidad (`EnableAlphaTest`). Mezclar sin profundidad hace que plantas y hojas
        // se pinten por encima de lo que tienen delante.
        (_, Some(AlphaKind::Cutout | AlphaKind::Blend)) => AlphaMode::Mask(ALPHA_REF),
        _ => AlphaMode::Opaque,
    };
    let base = StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: image.map(|i| i.0),
        unlit: true,
        cull_mode: None,
        alpha_mode,
        ..default()
    };
    // Las mallas aditivas son luz propia: no las oscurece la sombra del terreno
    // (el original no ilumina las `BlendMesh`).
    model_material(base, if additive { ModelLight::unlit() } else { ModelLight::lit(l) })
}

/// Posición/rotación/escala del objeto en MU, convertida a Bevy.
///
/// MU: Z arriba, 100 unidades por casilla, ángulos en grados aplicados X→Y→Z.
/// Bevy: Y arriba, 1 unidad por casilla. El cambio de ejes es un giro de -90° en X.
fn object_transform(o: &objects::MapObject) -> Transform {
    let to_bevy = Quat::from_rotation_x(-FRAC_PI_2);
    let [ax, ay, az] = o.angle.map(f32::to_radians);
    let rot = Quat::from_euler(EulerRot::ZYX, az, ay, ax);
    let pos = Vec3::from_array(o.position);
    Transform {
        translation: to_bevy * pos / MU_SCALE,
        rotation: to_bevy * rot,
        scale: Vec3::splat(o.scale / MU_SCALE),
    }
}
