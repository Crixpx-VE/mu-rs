//! Material for models (map objects and characters) with the original lighting: the
//! terrain light times a factor from each vertex normal (see `model_light.wesl`).
//!
//! It is an unlit Bevy `StandardMaterial` (texture, alpha, UV offset...) plus that light and,
//! for the original's `RenderMesh` passes (`ZzzBMD.cpp` ≈937-1354):
//! - [`ModelLight::tint`]: a color to multiply by (like `glColor`, in the original's color
//!   space: converted to linear in the shader). Used for `glColor3fv(BodyLight)` of the `StreamMesh` and
//!   chrome, and `BodyLight · BlendMeshLight` of the `BlendMesh`.
//! - [`ModelLight::chrome`]: texture coordinates computed from the normal (`g_chrome`, the
//!   `RENDER_CHROME*` modes); the formula is [`mu_data::objects::render::chrome_uv`].
//! - [`ModelLight::subtract`]: `RENDER_DARK` (`EnableAlphaBlendMinus`: ZERO, ONE_MINUS_SRC_COLOR). Used with
//!   `AlphaMode::Add` on the `StandardMaterial` (transparent pass, no depth write) and the blend is
//!   changed here.

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

/// What goes to the shader besides the light.
#[derive(Clone, Copy, Debug, ShaderType, Reflect)]
pub struct ModelParams {
    /// rgb: color to multiply by (not converted to linear, clamped to [0, 1] like `glColor`).
    pub tint: Vec4,
    /// [`ChromeMode`] (0 = model coordinates).
    pub chrome: u32,
    /// 1 = `RENDER_DARK`: the texture alpha does not count (`GL_ONE_MINUS_SRC_COLOR` only looks at color).
    pub dark: u32,
    /// 1 = the light (`BodyLight`) goes in each object's `MeshTag` ([`encode_light`]) instead of the
    /// material: so identical objects share a material and are batched together.
    pub tag_light: u32,
    pub _pad: u32,
}

impl Default for ModelParams {
    fn default() -> Self {
        Self { tint: Vec4::ONE, chrome: 0, dark: 0, tag_light: 0, _pad: 0 }
    }
}

/// `BodyLight` (0..1 per channel) in a `MeshTag`: 8 bits per channel.
pub fn encode_light(light: [f32; 3]) -> u32 {
    let b = light.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u32);
    b[0] | (b[1] << 8) | (b[2] << 16)
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
#[bind_group_data(ModelLightKey)]
pub struct ModelLight {
    /// rgb: original light (`BodyLight`, not converted to linear); a: 1 = light by the normal.
    #[uniform(100)]
    pub light: Vec4,
    #[uniform(101)]
    pub params: ModelParams,
    /// `RENDER_DARK` (subtractive).
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
    /// Lit by the normal with the original's `light` (0..1 per channel).
    pub fn lit(light: [f32; 3]) -> Self {
        Self { light: Vec4::new(light[0], light[1], light[2], 1.0), params: ModelParams::default(), subtract: false }
    }

    /// Unlit (additive meshes: they are their own light).
    pub fn unlit() -> Self {
        Self { light: Vec4::ZERO, params: ModelParams::default(), subtract: false }
    }

    /// Flat color `glColor3fv(color)`, no normal lighting (`StreamMesh`, chrome, `BlendMesh`).
    pub fn flat(color: [f32; 3]) -> Self {
        Self::unlit().with_tint(color)
    }

    /// Also multiplies by `color` (in the original's color space).
    pub fn with_tint(mut self, color: [f32; 3]) -> Self {
        self.params.tint = Vec4::new(color[0], color[1], color[2], 1.0);
        self
    }

    /// Chrome coordinates (`RENDER_CHROME*`).
    pub fn with_chrome(mut self, mode: Option<ChromeMode>) -> Self {
        self.params.chrome = mode.map_or(0, |m| m as u32);
        self
    }

    /// Lit by the normal with the light from each entity's `MeshTag` ([`encode_light`]).
    pub fn lit_by_tag() -> Self {
        let mut m = Self::lit([1.0; 3]);
        m.params.tag_light = 1;
        m
    }

    /// `RENDER_DARK`. The `StandardMaterial` must use `AlphaMode::Add`.
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
        // The fragment shader reads the `MeshTag` using the instance index.
        descriptor.vertex.shader_defs.push("VERTEX_OUTPUT_INSTANCE_INDEX".into());
        if let Some(fragment) = descriptor.fragment.as_mut() {
            fragment.shader_defs.push("VERTEX_OUTPUT_INSTANCE_INDEX".into());
        }
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

/// Model material from its `StandardMaterial` (which must be `unlit`).
pub fn model_material(base: StandardMaterial, light: ModelLight) -> ModelMaterial {
    ExtendedMaterial { base, extension: light }
}
