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
//! Cómo se pinta depende del mapa (`RenderPartObjectEffect`, ZzzObject.cpp:9336-9358):
//! - en casi todos, negro opaco (`DisableAlphaBlend` + `glColor3f(0, 0, 0)`); va con `Blend::Shadow`
//!   a opacidad 1, que es lo mismo;
//! - en Atlans (`WorldActive == 7`), negro al 20 % (`EnableAlphaTest` + `glColor4f(0, 0, 0, 0.2)`; la
//!   prueba de alfa la quita `DisableTexture`) y sin escribir profundidad (`RenderBodyShadow`), así que
//!   donde se solapan dos trozos oscurece más: `Blend::Blend3`;
//! - en Kalima (`InHellas`) e Icarus (`WD_10HEAVEN`), ninguna.

use std::sync::atomic::{AtomicBool, Ordering};

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;
use mu_data::terrain::Terrain;

use crate::fx::material::{Blend, FxMaterial};
use crate::terrain::MU_SCALE;

/// Opacidad de la sombra en Atlans (`glColor4f(0, 0, 0, 0.2)`).
const ATLANS_OPACITY: f32 = 0.2;
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
            m.params.tint = Vec4::new(0.0, 0.0, 0.0, 1.0);
            commands.insert_resource(ShadowMaterial(materials.add(m)));
        })
        .add_systems(Update, world_shadow);
    }
}

/// Si el mapa cargado tiene sombras (lo consulta [`Builder::finish`]).
static ENABLED: AtomicBool = AtomicBool::new(true);

/// Cómo son las sombras en `world` (nuestro número de mapa): `None` = no hay; si no, mezcla y opacidad.
fn world_style(world: u16) -> Option<(Blend, f32)> {
    match world as i32 - 1 {
        // Kalima 1-7 (`InHellas`) e Icarus.
        24..=29 | 36 | 10 => None,
        7 => Some((Blend::Blend3, ATLANS_OPACITY)),
        _ => Some((Blend::Shadow, 1.0)),
    }
}

/// Al cambiar de mapa, el material y si se dibujan.
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
        if !ENABLED.load(Ordering::Relaxed) {
            return;
        }
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
