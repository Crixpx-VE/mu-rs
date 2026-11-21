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
use crate::terrain::{LoadedTerrain, MU_SCALE, to_bevy_image};

pub struct ObjectsPlugin;

impl Plugin for ObjectsPlugin {
    fn build(&self, app: &mut App) {
        // PostStartup: el terreno (Startup) ya está cargado para consultar su luz.
        app.init_resource::<AnimatedModels>()
            .init_resource::<AnimatedMaterials>()
            .add_systems(PostStartup, spawn_objects)
            .add_systems(Update, (animate_objects, animate_materials));
    }
}

/// Grupos de objetos animados (árboles, banderas, molino...).
#[derive(Resource, Default)]
struct AnimatedModels(Vec<AnimatedModel>);

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
struct AnimatedMaterials(Vec<AnimatedMaterial>);

struct AnimatedMaterial {
    handle: Handle<StandardMaterial>,
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
    mut materials: ResMut<Assets<StandardMaterial>>,
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
            m.base_color = Color::linear_rgb(g, g, g);
        }
        if a.scroll != [0.0; 2] {
            let offset = Vec2::new((a.scroll[0] * t).fract(), (a.scroll[1] * t).fract());
            m.uv_transform = Affine2::from_translation(offset);
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
}

#[derive(Default)]
struct Caches {
    bmds: HashMap<PathBuf, Option<Arc<bmd::Bmd>>>,
    /// (modelo, velocidad en milésimas de fotograma/s; 0 = estático) → mallas
    models: HashMap<(PathBuf, u32), Vec<ModelPart>>,
    images: HashMap<PathBuf, Option<(Handle<Image>, AlphaKind)>>,
    materials: HashMap<MaterialKey, Handle<StandardMaterial>>,
}

fn spawn_objects(
    mut commands: Commands,
    args: Res<Args>,
    terrain: Option<Res<LoadedTerrain>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut anims: ResMut<AnimatedModels>,
    mut mat_anims: ResMut<AnimatedMaterials>,
) {
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

    for o in &list {
        let rules = objects::model_rules(args.world, o.kind);
        if rules.hidden {
            continue;
        }
        let Some(rel) = objects::model_path(args.world, o.kind) else {
            missing.entry(o.kind).or_insert_with(|| "sin entrada en la tabla".into());
            continue;
        };
        let model = c.bmds.entry(rel.clone()).or_insert_with(|| {
            let dir = args.data.join(rel.parent().unwrap_or(Path::new("")));
            let name = rel.file_name().and_then(|n| n.to_str()).unwrap_or("");
            fs::read(&dir, name)
                .and_then(|b| bmd::read(&b))
                .map(Arc::new)
                .map_err(|e| missing.insert(o.kind, e.to_string()))
                .ok()
        });
        let Some(model) = model.clone() else { continue };

        // Grupo de animación: los objetos de un modelo que van a la misma velocidad comparten malla.
        let animated = model.actions.first().is_some_and(|a| a.frames > 1);
        let fps = if animated { rules.velocity.fps(o.scale) } else { 0.0 };
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
        let light = light.map(|v| (v.clamp(0.0, 1.0) * 15.0).round() as u8);

        let transform = object_transform(o);
        for part in parts.iter() {
            let effect = part_effect(&rules, part);
            let key = MaterialKey {
                texture: part.texture.clone(),
                light,
                additive: part.additive,
                effect_owner: effect.map(|_| (rel.clone(), part.mesh_index)),
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
                    let m = materials.add(object_material(image, part.additive, light));
                    if let Some((glow, scroll)) = effect {
                        mat_anims.0.push(AnimatedMaterial { handle: m.clone(), glow, scroll });
                    }
                    c.materials.insert(key, m.clone());
                    m
                }
            };
            commands.spawn((Mesh3d(part.mesh.clone()), MeshMaterial3d(material), transform));
            spawned += 1;
        }
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

fn object_material(image: Option<(Handle<Image>, AlphaKind)>, additive: bool, light: [u8; 3]) -> StandardMaterial {
    // Misma conversión que el terreno: la luz del original se multiplica en sRGB.
    let l = light.map(|v| (v as f32 / 15.0).powf(2.2));
    let alpha_mode = match (additive, image.as_ref().map(|i| i.1)) {
        (true, _) => AlphaMode::Add,
        (_, Some(AlphaKind::Cutout)) => AlphaMode::Mask(0.5),
        (_, Some(AlphaKind::Blend)) => AlphaMode::Blend,
        _ => AlphaMode::Opaque,
    };
    StandardMaterial {
        // Las mallas aditivas son luz propia: no las oscurece la sombra del terreno.
        base_color: if additive { Color::WHITE } else { Color::linear_rgb(l[0], l[1], l[2]) },
        base_color_texture: image.map(|i| i.0),
        unlit: true,
        cull_mode: None,
        alpha_mode,
        ..default()
    }
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
