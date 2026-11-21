//! Material de los modelos (objetos del mapa y personajes) con la luz del original: la del
//! terreno por un factor según la normal de cada vértice (ver `model_light.wesl`).
//!
//! Es un `StandardMaterial` sin luz de Bevy (textura, alfa, desplazamiento de UV...) más esa luz.

use bevy::asset::embedded_asset;
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;

pub type ModelMaterial = ExtendedMaterial<StandardMaterial, ModelLight>;

pub struct ModelLightPlugin;

impl Plugin for ModelLightPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "model_light.wesl");
        app.add_plugins(MaterialPlugin::<ModelMaterial>::default());
    }
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct ModelLight {
    /// rgb: luz del original (`BodyLight`, sin pasar a lineal); a: 1 = iluminar por la normal.
    #[uniform(100)]
    pub light: Vec4,
}

impl ModelLight {
    /// Iluminado por la normal con la luz `light` del original (0..1 por canal).
    pub fn lit(light: [f32; 3]) -> Self {
        Self { light: Vec4::new(light[0], light[1], light[2], 1.0) }
    }

    /// Sin luz (mallas aditivas: son luz propia).
    pub fn unlit() -> Self {
        Self { light: Vec4::ZERO }
    }
}

impl MaterialExtension for ModelLight {
    fn fragment_shader() -> ShaderRef {
        "embedded://mu_client/model_light.wesl".into()
    }
}

/// Material de modelo a partir de su `StandardMaterial` (que debe ser `unlit`).
pub fn model_material(base: StandardMaterial, light: ModelLight) -> ModelMaterial {
    ExtendedMaterial { base, extension: light }
}
