//! Objetos del mapa (árboles, casas, murallas...) a partir de `EncTerrainN.obj` + modelos `.bmd`.
//!
//! Cada modelo se carga una sola vez; cada objeto del mapa es una instancia con su
//! posición, rotación y escala. Por ahora se dibujan estáticos (fotograma 0 de la animación 0).

use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;
use std::path::{Path, PathBuf};

use bevy::asset::RenderAssetUsages;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;
use mu_data::terrain::SIZE;
use mu_data::texture::AlphaKind;
use mu_data::{bmd, fs, objects, texture};

use crate::Args;
use crate::terrain::{LoadedTerrain, MU_SCALE, to_bevy_image};

pub struct ObjectsPlugin;

impl Plugin for ObjectsPlugin {
    fn build(&self, app: &mut App) {
        // PostStartup: el terreno (Startup) ya está cargado para consultar su luz.
        app.add_systems(PostStartup, spawn_objects);
    }
}

/// Una malla de un modelo, lista para instanciar.
struct ModelPart {
    mesh: Handle<Mesh>,
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
}

#[derive(Default)]
struct Caches {
    models: HashMap<PathBuf, Option<Vec<ModelPart>>>,
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
        let parts = c.models.entry(rel.clone()).or_insert_with(|| {
            match load_model(&args.data, &rel, rules.additive_mesh, &mut meshes) {
                Ok(p) => Some(p),
                Err(e) => {
                    missing.insert(o.kind, e);
                    None
                }
            }
        });
        let Some(parts) = parts else { continue };

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
            let key = MaterialKey { texture: part.texture.clone(), light, additive: part.additive };
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
                    c.materials.insert(key, m.clone());
                    m
                }
            };
            commands.spawn((Mesh3d(part.mesh.clone()), MeshMaterial3d(material), transform));
            spawned += 1;
        }
    }

    info!(
        "World{}: {} objetos, {spawned} mallas, {} modelos, {} materiales",
        args.world,
        list.len(),
        c.models.len(),
        c.materials.len()
    );
    for (kind, why) in missing {
        warn!("objeto tipo {kind} no cargado: {why}");
    }
}

/// Carga un `.bmd` y crea una malla de Bevy por cada malla del modelo.
fn load_model(
    data: &Path,
    rel: &Path,
    additive_mesh: Option<usize>,
    meshes: &mut Assets<Mesh>,
) -> Result<Vec<ModelPart>, String> {
    let dir = data.join(rel.parent().unwrap_or(Path::new("")));
    let name = rel.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let model = fs::read(&dir, name).and_then(|b| bmd::read(&b)).map_err(|e| e.to_string())?;

    Ok((0..model.meshes.len())
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
                texture: fs::find(&dir, &bmd::texture_file_name(&model.meshes[i].texture)),
                additive: additive_mesh == Some(i),
            })
        })
        .collect())
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
