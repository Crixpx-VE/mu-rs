//! Effect `.bmd` models (`RenderObject` of `Effects[]`): spinning swords, cracks, waves,
//! meteors... Each model has a pool of instances reused every frame.
//!
//! Per-mesh rules (like `BMD::RenderMesh`):
//! - `hidden_mesh` = -2 hides all; n hides the mesh with texture n.
//! - `bright` (`RENDER_BRIGHT`): the whole model additive with `light · alpha`.
//! - the mesh whose texture is `blend_mesh`: additive with `light · blend_mesh_light` and offset UVs.
//! - the rest: texture with alpha → alpha test; alpha < 1 → normal blend; otherwise opaque.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;
use mu_data::bmd::{self, Bmd};
use mu_data::texture::{self, AlphaKind};
use mu_data::fs;

use super::core::*;
use super::draw::{FrameDraw, mu_to_bevy};
use super::tables::models::MODELS;
use super::material::{FxMaterial, FxParams};
use super::textures::fx_image;
use crate::terrain::MU_SCALE;

struct LoadedModel {
    bmd: Arc<Bmd>,
    /// Per mesh: texture and whether it has transparency.
    textures: Vec<(Option<Handle<Image>>, bool)>,
}

struct Part {
    mesh_index: usize,
    texture_index: i32,
    mesh: Handle<Mesh>,
    material: Handle<FxMaterial>,
    entity: Entity,
}

struct Instance {
    root: Entity,
    parts: Vec<Part>,
}

#[derive(Resource)]
pub struct FxModels {
    data: PathBuf,
    table: HashMap<i32, (&'static str, &'static str, &'static str)>,
    loaded: HashMap<i32, Option<Arc<LoadedModel>>>,
    pools: HashMap<i32, Vec<Instance>>,
}

impl FxModels {
    pub fn new(data: PathBuf) -> Self {
        Self {
            data,
            table: MODELS.iter().map(|&(id, dir, file, tex)| (id, (dir, file, tex))).collect(),
            loaded: HashMap::new(),
            pools: HashMap::new(),
        }
    }

    /// Folder, file and texture folder of model `id` (effects from the generated table, or
    /// items: `MODEL_ITEM + group·512 + n` → `Item/{Group}{n+1:02}.bmd`).
    fn locate(&self, id: i32) -> Option<(String, String, String)> {
        if let Some(&(d, f, t)) = self.table.get(&id) {
            return Some((d.to_string(), f.to_string(), t.to_string()));
        }
        // Models the original loads in loops (the generator does not see them).
        // Map object models used by effects (`OpenWorldModels` / `MapManager`).
        let fixed = match id {
            MODEL_WARP..=MODEL_WARP3 => Some(("Npc", format!("warp{:02}.bmd", id - MODEL_WARP + 1))),
            // Raklion (`MapManager.cpp:769-774`): the same models under other numbers.
            MODEL_WARP4..=MODEL_WARP6 => Some(("Npc", format!("warp{:02}.bmd", id - MODEL_WARP4 + 1))),
            MODEL_DUNGEON_STONE01 => Some(("Object2", "DungeonStone01.bmd".into())),
            MODEL_CLOUD => Some(("Object11", "cloud.bmd".into())),
            // Kalima `CreateEffect(9)`: map model 9 (`Object25`, the same in all 7 Kalimas).
            MODEL_KALIMA_FALLING_STONE => Some(("Object25", "Object10.bmd".into())),
            // `MapManager.cpp:293-294` (and other maps): `Object1\Butterfly01`.
            MODEL_BUTTERFLY01 => Some(("Object1", "Butterfly01.bmd".into())),
            _ => None,
        };
        if let Some((dir, file)) = fixed {
            return Some((dir.into(), file, dir.into()));
        }
        if (MODEL_STONE1..=MODEL_STONE2).contains(&id) {
            return Some(("Skill".into(), format!("Stone{:02}.bmd", id - MODEL_STONE1 + 1), "Skill".into()));
        }
        if (MODEL_ITEM..MODEL_ITEM + 16 * 512).contains(&id) {
            const GROUPS: [&str; 7] = ["Sword", "Axe", "Mace", "Spear", "Bow", "Staff", "Shield"];
            let rel = id - MODEL_ITEM;
            let name = GROUPS.get((rel / 512) as usize)?;
            return Some(("Item".into(), format!("{name}{:02}.bmd", rel % 512 + 1), "Item".into()));
        }
        None
    }

    fn load(&mut self, id: i32, images: &mut Assets<Image>) -> Option<Arc<LoadedModel>> {
        if let Some(m) = self.loaded.get(&id) {
            return m.clone();
        }
        let result = self.locate(id).and_then(|(dir, file, texdir)| {
            let d = self.data.join(&dir);
            let bmd = match fs::read(&d, &file).and_then(|b| bmd::read(&b)) {
                Ok(b) => Arc::new(b),
                Err(e) => {
                    warn!("efecto: no se pudo cargar el modelo {id} ({dir}/{file}): {e}");
                    return None;
                }
            };
            let td = self.data.join(&texdir);
            let textures = bmd
                .meshes
                .iter()
                .map(|m| match texture::load(&td, &bmd::texture_file_name(&m.texture)) {
                    Ok(t) => {
                        let alpha = t.alpha_kind() != AlphaKind::Opaque;
                        (Some(images.add(fx_image(&t, true))), alpha)
                    }
                    Err(_) => (None, false),
                })
                .collect();
            Some(Arc::new(LoadedModel { bmd, textures }))
        });
        self.loaded.insert(id, result.clone());
        result
    }
}

/// Step 3: effect models.
#[allow(clippy::too_many_arguments)]
pub fn render_models(
    mut commands: Commands,
    frame: Res<FrameDraw>,
    mut models: ResMut<FxModels>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<FxMaterial>>,
    mut q: Query<(&mut Transform, &mut Visibility)>,
) {
    let mut used: HashMap<i32, usize> = HashMap::new();
    let to_bevy_rot = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);

    for d in &frame.0.models {
        let Some(model) = models.load(d.model, &mut images) else { continue };
        let slot = used.entry(d.model).or_insert(0);
        let index = *slot;
        *slot += 1;

        let pool = models.pools.entry(d.model).or_default();
        if pool.len() <= index {
            pool.push(spawn_instance(&mut commands, &model, d.model, &mut meshes, &mut materials));
        }
        let inst = &pool[index];

        if let Ok((mut tf, mut vis)) = q.get_mut(inst.root) {
            let a = d.angle * (std::f32::consts::PI / 180.0);
            *tf = Transform {
                translation: mu_to_bevy(d.position),
                rotation: to_bevy_rot * Quat::from_euler(EulerRot::ZYX, a.z, a.y, a.x),
                scale: Vec3::splat(d.scale / MU_SCALE),
            };
            *vis = Visibility::Inherited;
        }

        let animated = model.bmd.actions.get(d.action).is_some_and(|x| x.frames > 1);
        let bones = if animated { Some(model.bmd.bone_matrices_at(d.action, d.frame)) } else { None };
        for part in &inst.parts {
            if let Some(bones) = &bones {
                let posed = model.bmd.pose_with(part.mesh_index, bones);
                if let Some(mut m) = meshes.get_mut(&part.mesh) {
                    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, posed.positions);
                    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, posed.normals);
                }
            }
            let hidden = d.hidden_mesh == -2 || d.hidden_mesh == part.texture_index;
            if let Ok((_, mut vis)) = q.get_mut(part.entity) {
                let want = if hidden { Visibility::Hidden } else { Visibility::Inherited };
                if *vis != want {
                    *vis = want;
                }
            }
            if hidden {
                continue;
            }
            let has_alpha = model.textures.get(part.mesh_index).is_some_and(|t| t.1);
            let lin = |v: f32| v.max(0.0).powf(2.2);
            let (blend, tint, uv) = if d.bright {
                let l = d.light * d.alpha;
                (Blend::Glow, Vec4::new(lin(l.x), lin(l.y), lin(l.z), 1.0), Vec2::ZERO)
            } else if d.blend_mesh <= -2 || part.texture_index == d.blend_mesh {
                let l = d.light * d.blend_mesh_light;
                (Blend::Glow, Vec4::new(lin(l.x), lin(l.y), lin(l.z), 1.0), d.blend_mesh_tex_coord)
            } else {
                let blend = if has_alpha {
                    Blend::AlphaTest
                } else if d.alpha < 0.99 {
                    Blend::Blend3
                } else {
                    Blend::Opaque
                };
                (blend, Vec4::new(lin(d.light.x), lin(d.light.y), lin(d.light.z), d.alpha), Vec2::ZERO)
            };
            if let Some(m) = materials.get(&part.material)
                && (m.blend != blend || m.params.tint != tint || m.params.uv_offset != uv)
                && let Some(mut m) = materials.get_mut(&part.material)
            {
                m.blend = blend;
                m.params = FxParams { tint, uv_offset: uv, ..m.params };
            }
        }
    }

    // Hide instances unused this frame.
    for (id, pool) in &models.pools {
        let n = used.get(id).copied().unwrap_or(0);
        for inst in pool.iter().skip(n) {
            if let Ok((_, mut vis)) = q.get_mut(inst.root)
                && *vis != Visibility::Hidden
            {
                *vis = Visibility::Hidden;
            }
        }
    }
}

fn spawn_instance(
    commands: &mut Commands,
    model: &LoadedModel,
    id: i32,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<FxMaterial>,
) -> Instance {
    let mut parts = vec![];
    let root = commands.spawn((Name::new(format!("Efecto modelo {id}")), Transform::default(), Visibility::Hidden)).id();
    for (i, m) in model.bmd.meshes.iter().enumerate() {
        let posed = model.bmd.pose(i, 0, 0);
        if posed.positions.is_empty() {
            continue;
        }
        let mesh = meshes.add(
            Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, posed.positions)
                .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, posed.normals)
                .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, posed.uvs),
        );
        let texture = model.textures.get(i).and_then(|t| t.0.clone());
        let material = materials.add(FxMaterial::new(texture, Blend::Opaque));
        let entity = commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), ChildOf(root))).id();
        parts.push(Part { mesh_index: i, texture_index: m.texture_index as i32, mesh, material, entity });
    }
    Instance { root, parts }
}
