//! Sombras de los modelos en el suelo (`BMD::RenderBodyShadow` / `CalcShadowPosition`):
//! personajes, pájaros y dragones. Los objetos del mapa no tienen: sus sombras ya vienen
//! pintadas en la luz del terreno.
//!
//! Cada vértice del modelo, relativo a su origen, se inclina hacia -X según su altura y se
//! aplasta contra el suelo, 5 unidades por encima:
//!
//! ```text
//! x += z · (x + 2000) / (z − 4000)
//! z  = altura del terreno en (x, y) + 5
//! ```
//!
//! Se dibuja en negro semitransparente sin pintar dos veces el mismo punto (`Blend::Shadow`), para
//! que los trozos que se solapan no queden más oscuros. Opacidad: la de la versión de referencia (0.6).

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;
use mu_data::terrain::Terrain;

use crate::fx::material::{Blend, FxMaterial};
use crate::terrain::MU_SCALE;

/// Opacidad de la sombra (`kDefaultGroundShadowOpacity`).
const OPACITY: f32 = 0.6;
/// `shearX` y `shearHeight` de la proyección.
const SHEAR_X: f32 = 2000.0;
const SHEAR_HEIGHT: f32 = 4000.0;
/// Altura sobre el suelo.
const LIFT: f32 = 5.0;

pub struct ShadowPlugin;

impl Plugin for ShadowPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, |mut commands: Commands, mut materials: ResMut<Assets<FxMaterial>>| {
            let mut m = FxMaterial::new(None, Blend::Shadow);
            m.params.tint = Vec4::new(0.0, 0.0, 0.0, OPACITY);
            commands.insert_resource(ShadowMaterial(materials.add(m)));
        });
    }
}

#[derive(Resource)]
pub struct ShadowMaterial(pub Handle<FxMaterial>);

/// La malla de sombra de un modelo (en coordenadas del mundo; se rehace cada fotograma) y su entidad.
#[derive(Component, Clone)]
pub struct Shadow(pub Handle<Mesh>, pub Entity);

/// Crea la entidad de una sombra vacía y devuelve su malla.
pub fn spawn(commands: &mut Commands, meshes: &mut Assets<Mesh>, material: &ShadowMaterial) -> Shadow {
    // Un triángulo degenerado y oculta: Bevy avisa de cada malla vacía que sube.
    let mesh = meshes.add(build(vec![[0.0; 3]; 3]));
    let entity = commands
        .spawn((
            Name::new("Sombra"),
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.0.clone()),
            Transform::default(),
            Visibility::Hidden,
            crate::scene::WorldEntity,
            // La malla cambia de sitio cada fotograma: su caja inicial no vale.
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

/// Junta los triángulos de sombra de un modelo. `origin` es la posición del modelo en MU y
/// `rotation` lleva los vértices del modelo (MU, ya posados) a su giro en el mundo.
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

    /// Añade los vértices (triángulos sueltos) de una malla posada.
    pub fn add(&mut self, positions: &[[f32; 3]]) {
        self.out.reserve(positions.len());
        for p in positions {
            let mut r = self.rotation * (Vec3::from_array(*p) * self.scale);
            // `CalcShadowPosition` con el eje X (Classic).
            r.x += r.z * (r.x + SHEAR_X) / (r.z - SHEAR_HEIGHT);
            let (x, y) = (self.origin.x + r.x, self.origin.y + r.y);
            let z = self.terrain.map_or(self.origin.z, |t| t.height_at(x / MU_SCALE, y / MU_SCALE)) + LIFT;
            // MU → Bevy.
            self.out.push([x / MU_SCALE, z / MU_SCALE, -y / MU_SCALE]);
        }
    }

    /// Pone la sombra en su malla y la muestra u oculta (sin nada, se oculta sin tocar la malla).
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
