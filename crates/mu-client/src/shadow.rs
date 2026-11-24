//! Model shadows on the ground (`BMD::RenderBodyShadow` / `CalcShadowPosition`):
//! characters, birds and dragons. Map objects have none: their shadows are already
//! baked into the terrain light.
//!
//! Each model vertex, relative to its origin, is sheared towards -X by its height and
//! flattened onto the ground, 5 units above it:
//!
//! ```text
//! x += z · (x + 2000) / (z − 4000)
//! z  = terrain height at (x, y) + 5
//! ```
//!
//! How it is drawn depends on the map (`RenderPartObjectEffect`, ZzzObject.cpp:9336-9358):
//! - on almost all, opaque black (`DisableAlphaBlend` + `glColor3f(0, 0, 0)`); done with `Blend::Shadow`
//!   at opacity 1, which is the same;
//! - in Atlans (`WorldActive == 7`), 20 % black (`EnableAlphaTest` + `glColor4f(0, 0, 0, 0.2)`; the
//!   alpha test is removed by `DisableTexture`) without depth writes (`RenderBodyShadow`), so
//!   overlapping pieces get darker: `Blend::Blend3`;
//! - in Kalima (`InHellas`) and Icarus (`WD_10HEAVEN`), none.

use std::sync::atomic::{AtomicBool, Ordering};

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;
use mu_data::terrain::Terrain;

use crate::fx::material::{Blend, FxMaterial};
use crate::terrain::MU_SCALE;

/// Shadow opacity in Atlans (`glColor4f(0, 0, 0, 0.2)`).
const ATLANS_OPACITY: f32 = 0.2;
/// Projection `shearX` and `shearHeight`.
const SHEAR_X: f32 = 2000.0;
const SHEAR_HEIGHT: f32 = 4000.0;
/// Height above the ground.
const LIFT: f32 = 5.0;

pub struct ShadowPlugin;

impl Plugin for ShadowPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, |mut commands: Commands, mut materials: ResMut<Assets<FxMaterial>>| {
            let mut m = FxMaterial::new(None, Blend::Shadow);
            m.params.tint = Vec4::new(0.0, 0.0, 0.0, 1.0);
            commands.insert_resource(ShadowMaterial(materials.add(m)));
        })
        .add_systems(Update, world_shadow);
    }
}

/// Whether the loaded map has shadows (read by [`Builder::finish`]).
static ENABLED: AtomicBool = AtomicBool::new(true);

/// What shadows look like in `world` (our map number): `None` = none; otherwise blend and opacity.
fn world_style(world: u16) -> Option<(Blend, f32)> {
    match world as i32 - 1 {
        // Kalima 1-7 (`InHellas`) and Icarus.
        24..=29 | 36 | 10 => None,
        7 => Some((Blend::Blend3, ATLANS_OPACITY)),
        _ => Some((Blend::Shadow, 1.0)),
    }
}

/// On map change, the material and whether they are drawn.
fn world_shadow(
    args: Res<crate::Args>,
    material: Option<Res<ShadowMaterial>>,
    mut materials: ResMut<Assets<FxMaterial>>,
    mut current: Local<Option<u16>>,
) {
    let Some(material) = material else { return };
    if *current == Some(args.world) {
        return;
    }
    *current = Some(args.world);
    let style = world_style(args.world);
    ENABLED.store(style.is_some(), Ordering::Relaxed);
    if let Some((blend, alpha)) = style
        && let Some(mut m) = materials.get_mut(&material.0)
    {
        m.blend = blend;
        m.params.tint = Vec4::new(0.0, 0.0, 0.0, alpha);
    }
}

#[derive(Resource)]
pub struct ShadowMaterial(pub Handle<FxMaterial>);

/// A model's shadow mesh (in world coordinates; rebuilt every frame) and its entity.
#[derive(Component, Clone)]
pub struct Shadow(pub Handle<Mesh>, pub Entity);

/// Spawns an empty shadow entity and returns its mesh.
pub fn spawn(commands: &mut Commands, meshes: &mut Assets<Mesh>, material: &ShadowMaterial) -> Shadow {
    // A hidden degenerate triangle: Bevy warns about every empty mesh it uploads.
    let mesh = meshes.add(build(vec![[0.0; 3]; 3]));
    let entity = commands
        .spawn((
            Name::new("Sombra"),
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.0.clone()),
            Transform::default(),
            Visibility::Hidden,
            crate::scene::WorldEntity,
            // The mesh moves every frame: its initial bounding box is useless.
            NoFrustumCulling,
        ))
        .id();
    Shadow(mesh, entity)
}

fn build(positions: Vec<[f32; 3]>) -> Mesh {
    let n = positions.len();
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; n])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0]; n])
}

/// Collects a model's shadow triangles. `origin` is the model's position in MU and
/// `rotation` takes the model's vertices (MU, already posed) to its world orientation.
pub struct Builder<'a> {
    terrain: Option<&'a Terrain>,
    origin: Vec3,
    rotation: Quat,
    scale: f32,
    out: Vec<[f32; 3]>,
}

impl<'a> Builder<'a> {
    pub fn new(terrain: Option<&'a Terrain>, origin: Vec3, rotation: Quat, scale: f32) -> Self {
        Self { terrain, origin, rotation, scale, out: Vec::new() }
    }

    /// Adds the vertices (loose triangles) of a posed mesh.
    pub fn add(&mut self, positions: &[[f32; 3]]) {
        if !ENABLED.load(Ordering::Relaxed) {
            return;
        }
        self.out.reserve(positions.len());
        for p in positions {
            let mut r = self.rotation * (Vec3::from_array(*p) * self.scale);
            // `CalcShadowPosition` with the X axis (Classic).
            r.x += r.z * (r.x + SHEAR_X) / (r.z - SHEAR_HEIGHT);
            let (x, y) = (self.origin.x + r.x, self.origin.y + r.y);
            let z = self.terrain.map_or(self.origin.z, |t| t.height_at(x / MU_SCALE, y / MU_SCALE)) + LIFT;
            // MU → Bevy.
            self.out.push([x / MU_SCALE, z / MU_SCALE, -y / MU_SCALE]);
        }
    }

    /// Puts the shadow in its mesh and shows or hides it (if empty, hides it without touching the mesh).
    pub fn finish(self, shadow: &Shadow, meshes: &mut Assets<Mesh>, visibility: Option<Mut<Visibility>>) {
        let visible = !self.out.is_empty();
        if visible && let Some(mut m) = meshes.get_mut(&shadow.0) {
            *m = build(self.out);
        }
        if let Some(mut v) = visibility {
            v.set_if_neq(if visible { Visibility::Inherited } else { Visibility::Hidden });
        }
    }
}
