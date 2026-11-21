//! Material de los modelos (objetos del mapa y personajes) con la luz del original: la del
//! terreno por un factor según la normal de cada vértice (ver `model_light.wesl`).
//!
//! Es un `StandardMaterial` sin luz de Bevy (textura, alfa, desplazamiento de UV...) más esa luz y,
//! para las pasadas de `RenderMesh` del original (`ZzzBMD.cpp` ≈937-1354):
//! - [`ModelLight::tint`]: un color por el que se multiplica (como `glColor`, en el espacio de color del
//!   original: se pasa a lineal en el sombreador). Para `glColor3fv(BodyLight)` de la `StreamMesh` y el
//!   cromado, y `BodyLight · BlendMeshLight` de la `BlendMesh`.
//! - [`ModelLight::chrome`]: coordenadas de textura calculadas con la normal (`g_chrome`, los
//!   `RENDER_CHROME*`); la fórmula es [`mu_data::objects::render::chrome_uv`].
//! - [`ModelLight::subtract`]: `RENDER_DARK` (`EnableAlphaBlendMinus`: ZERO, ONE_MINUS_SRC_COLOR). Va con
//!   `AlphaMode::Add` en el `StandardMaterial` (pasada transparente, sin escribir profundidad) y aquí se
//!   cambia la mezcla.

use bevy::asset::embedded_asset;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, RenderPipelineDescriptor, ShaderType,
    SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;
use mu_data::objects::render::ChromeMode;

pub type ModelMaterial = ExtendedMaterial<StandardMaterial, ModelLight>;

pub struct ModelLightPlugin;

impl Plugin for ModelLightPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "model_light.wesl");
        app.add_plugins(MaterialPlugin::<ModelMaterial>::default());
    }
}

/// Lo que va al sombreador además de la luz.
#[derive(Clone, Copy, Debug, ShaderType, Reflect)]
pub struct ModelParams {
    /// rgb: color por el que se multiplica (sin pasar a lineal, se limita a [0, 1] como `glColor`).
    pub tint: Vec4,
    /// [`ChromeMode`] (0 = coordenadas del modelo).
    pub chrome: u32,
    /// 1 = `RENDER_DARK`: el alfa de la textura no cuenta (`GL_ONE_MINUS_SRC_COLOR` solo mira el color).
    pub dark: u32,
    pub _pad: UVec2,
}

impl Default for ModelParams {
    fn default() -> Self {
        Self { tint: Vec4::ONE, chrome: 0, dark: 0, _pad: UVec2::ZERO }
    }
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
#[bind_group_data(ModelLightKey)]
pub struct ModelLight {
    /// rgb: luz del original (`BodyLight`, sin pasar a lineal); a: 1 = iluminar por la normal.
    #[uniform(100)]
    pub light: Vec4,
    #[uniform(101)]
    pub params: ModelParams,
    /// `RENDER_DARK` (resta).
    pub subtract: bool,
}

impl Default for ModelLight {
    fn default() -> Self {
        Self::unlit()
    }
}

#[repr(C)]
#[derive(Eq, PartialEq, Hash, Copy, Clone)]
pub struct ModelLightKey {
    subtract: bool,
}

impl From<&ModelLight> for ModelLightKey {
    fn from(m: &ModelLight) -> Self {
        Self { subtract: m.subtract }
    }
}

impl ModelLight {
    /// Iluminado por la normal con la luz `light` del original (0..1 por canal).
    pub fn lit(light: [f32; 3]) -> Self {
        Self { light: Vec4::new(light[0], light[1], light[2], 1.0), params: ModelParams::default(), subtract: false }
    }

    /// Sin luz (mallas aditivas: son luz propia).
    pub fn unlit() -> Self {
        Self { light: Vec4::ZERO, params: ModelParams::default(), subtract: false }
    }

    /// Color plano `glColor3fv(color)`, sin luz por la normal (`StreamMesh`, cromado, `BlendMesh`).
    pub fn flat(color: [f32; 3]) -> Self {
        Self::unlit().with_tint(color)
    }

    /// Multiplica además por `color` (en el espacio de color del original).
    pub fn with_tint(mut self, color: [f32; 3]) -> Self {
        self.params.tint = Vec4::new(color[0], color[1], color[2], 1.0);
        self
    }

    /// Coordenadas de cromado (`RENDER_CHROME*`).
    pub fn with_chrome(mut self, mode: Option<ChromeMode>) -> Self {
        self.params.chrome = mode.map_or(0, |m| m as u32);
        self
    }

    /// `RENDER_DARK`. El `StandardMaterial` debe ir con `AlphaMode::Add`.
    pub fn with_subtract(mut self, on: bool) -> Self {
        self.subtract = on;
        self.params.dark = on as u32;
        self
    }
}

impl MaterialExtension for ModelLight {
    fn fragment_shader() -> ShaderRef {
        "embedded://mu_client/model_light.wesl".into()
    }

    fn specialize(
        _pipeline: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        if key.bind_group_data.subtract
            && let Some(fragment) = descriptor.fragment.as_mut()
        {
            // `glBlendFunc(GL_ZERO, GL_ONE_MINUS_SRC_COLOR)` (`EnableAlphaBlendMinus`).
            let component = |src, dst| BlendComponent { src_factor: src, dst_factor: dst, operation: BlendOperation::Add };
            for target in fragment.targets.iter_mut().flatten() {
                target.blend = Some(BlendState {
                    color: component(BlendFactor::Zero, BlendFactor::OneMinusSrc),
                    alpha: component(BlendFactor::Zero, BlendFactor::One),
                });
            }
        }
        Ok(())
    }
}

/// Material de modelo a partir de su `StandardMaterial` (que debe ser `unlit`).
pub fn model_material(base: StandardMaterial, light: ModelLight) -> ModelMaterial {
    ExtendedMaterial { base, extension: light }
}
