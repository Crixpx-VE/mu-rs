//! Material de los efectos con los modos de mezcla exactos del original (`ZzzOpenglUtil.cpp`):
//!
//! | `Blend`     | función del original     | mezcla OpenGL                         | profundidad |
//! |-------------|--------------------------|---------------------------------------|-------------|
//! | `Glow`      | `EnableAlphaBlend`       | ONE, ONE (suma)                        | no escribe  |
//! | `Subtract`  | `EnableAlphaBlendMinus`  | ZERO, ONE_MINUS_SRC_COLOR (oscurece)   | no escribe  |
//! | `Blend2`    | `EnableAlphaBlend2`      | ONE_MINUS_SRC_COLOR, ONE               | no escribe  |
//! | `Blend3`    | `EnableAlphaBlend3`      | SRC_ALPHA, ONE_MINUS_SRC_ALPHA         | no escribe  |
//! | `Blend4`    | `EnableAlphaBlend4`      | ONE, ONE_MINUS_SRC_ALPHA               | no escribe  |
//! | `AlphaTest` | `EnableAlphaTest`        | SRC_ALPHA, ONE_MINUS_SRC_ALPHA + alfa > 0.25 | escribe |
//! | `Opaque`    | `DisableAlphaBlend`      | sin mezcla                             | escribe     |
//! | `LightMap`  | `EnableLightMap`         | ZERO, SRC_COLOR (multiplica)           | escribe     |
//!
//! Siempre sin recorte de caras ocultas (el original las desactiva en casi todos los modos).

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

/// Modo de mezcla del original.
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
}

impl Blend {
    /// Escribe en el búfer de profundidad.
    pub fn writes_depth(self) -> bool {
        matches!(self, Blend::AlphaTest | Blend::Opaque | Blend::LightMap)
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
        // Todo lo que mezcla va a la pasada transparente (ordenada de lejos a cerca).
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
        }
        let state = match blend {
            Blend::Glow => Some((One, One)),
            Blend::Subtract => Some((Zero, OneMinusSrc)),
            Blend::Blend2 => Some((OneMinusSrc, One)),
            Blend::Blend3 | Blend::AlphaTest => Some((SrcAlpha, OneMinusSrcAlpha)),
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
