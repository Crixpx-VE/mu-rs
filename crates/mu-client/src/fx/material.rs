//! Effects material with the original's exact blend modes (`ZzzOpenglUtil.cpp`):
//!
//! | `Blend`     | original function        | OpenGL blend                          | depth       |
//! |-------------|--------------------------|---------------------------------------|-------------|
//! | `Glow`      | `EnableAlphaBlend`       | ONE, ONE (additive)                    | no write    |
//! | `Subtract`  | `EnableAlphaBlendMinus`  | ZERO, ONE_MINUS_SRC_COLOR (darkens)    | no write    |
//! | `Blend2`    | `EnableAlphaBlend2`      | ONE_MINUS_SRC_COLOR, ONE               | no write    |
//! | `Blend3`    | `EnableAlphaBlend3`      | SRC_ALPHA, ONE_MINUS_SRC_ALPHA         | no write    |
//! | `Blend4`    | `EnableAlphaBlend4`      | ONE, ONE_MINUS_SRC_ALPHA               | no write    |
//! | `AlphaTest` | `EnableAlphaTest`        | SRC_ALPHA, ONE_MINUS_SRC_ALPHA + alpha > 0.25 | write |
//! | `Opaque`    | `DisableAlphaBlend`      | no blending                            | write       |
//! | `LightMap`  | `EnableLightMap`         | ZERO, SRC_COLOR (multiply)             | write       |
//! | `Shadow`    | `RenderBodyShadow`       | SRC_ALPHA, ONE_MINUS_SRC_ALPHA         | write, never paints the same point twice |
//!
//! Back-face culling is always off (the original disables it in almost every mode).

use bevy::asset::embedded_asset;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, RenderPipelineDescriptor, ShaderType,
    SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;

pub struct FxMaterialPlugin;

impl Plugin for FxMaterialPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "fx.wesl");
        app.add_plugins(MaterialPlugin::<FxMaterial>::default());
    }
}

/// The original's blend mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum Blend {
    #[default]
    Glow,
    Subtract,
    Blend2,
    Blend3,
    Blend4,
    AlphaTest,
    Opaque,
    LightMap,
    /// Model shadows on the ground: blended once per point (fragments at the same depth as
    /// something already drawn are discarded), so overlapping pieces do not darken further.
    Shadow,
}

impl Blend {
    /// Writes to the depth buffer.
    pub fn writes_depth(self) -> bool {
        matches!(self, Blend::AlphaTest | Blend::Opaque | Blend::LightMap | Blend::Shadow)
    }
}

#[derive(Clone, Copy, Debug, ShaderType)]
pub struct FxParams {
    pub tint: Vec4,
    pub uv_offset: Vec2,
    pub alpha_ref: f32,
    pub _pad: f32,
}

impl Default for FxParams {
    fn default() -> Self {
        Self { tint: Vec4::ONE, uv_offset: Vec2::ZERO, alpha_ref: 0.25, _pad: 0.0 }
    }
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
#[bind_group_data(FxMaterialKey)]
pub struct FxMaterial {
    #[uniform(0)]
    pub params: FxParams,
    #[texture(1)]
    #[sampler(2)]
    pub texture: Option<Handle<Image>>,
    pub blend: Blend,
}

impl FxMaterial {
    pub fn new(texture: Option<Handle<Image>>, blend: Blend) -> Self {
        Self { params: FxParams::default(), texture, blend }
    }
}

#[repr(C)]
#[derive(Eq, PartialEq, Hash, Copy, Clone)]
pub struct FxMaterialKey {
    blend: Blend,
}

impl From<&FxMaterial> for FxMaterialKey {
    fn from(m: &FxMaterial) -> Self {
        Self { blend: m.blend }
    }
}

fn component(src: BlendFactor, dst: BlendFactor) -> BlendComponent {
    BlendComponent { src_factor: src, dst_factor: dst, operation: BlendOperation::Add }
}

impl Material for FxMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://mu_client/fx/fx.wesl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        // Everything that blends goes to the transparent pass (sorted back to front).
        match self.blend {
            Blend::Opaque => AlphaMode::Opaque,
            _ => AlphaMode::Blend,
        }
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        use BlendFactor::*;
        let blend = key.bind_group_data.blend;
        descriptor.primitive.cull_mode = None;
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_write_enabled = Some(blend.writes_depth());
            if blend == Blend::Shadow {
                // Bevy uses reversed depth: "closer" is greater. Strict, so the same point never passes twice.
                depth.depth_compare = Some(bevy::render::render_resource::CompareFunction::Greater);
            }
        }
        let state = match blend {
            Blend::Glow => Some((One, One)),
            Blend::Subtract => Some((Zero, OneMinusSrc)),
            Blend::Blend2 => Some((OneMinusSrc, One)),
            Blend::Blend3 | Blend::AlphaTest | Blend::Shadow => Some((SrcAlpha, OneMinusSrcAlpha)),
            Blend::Blend4 => Some((One, OneMinusSrcAlpha)),
            Blend::LightMap => Some((Zero, Src)),
            Blend::Opaque => None,
        };
        if let Some(fragment) = descriptor.fragment.as_mut() {
            if blend == Blend::AlphaTest {
                fragment.shader_defs.push("FX_ALPHA_TEST".into());
            }
            for target in fragment.targets.iter_mut().flatten() {
                target.blend = state.map(|(src, dst)| BlendState {
                    color: component(src, dst),
                    alpha: component(One, OneMinusSrcAlpha),
                });
            }
        }
        Ok(())
    }
}
