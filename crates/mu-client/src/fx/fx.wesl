// Effect material: texture × vertex color × tint, with the original's blend modes
// (chosen in `FxMaterial::specialize`). Unlit: MU effects are not lit. With `FX_UNTEXTURED` the
// texture is not sampled.

import bevy_pbr::render::forward_io::VertexOutput;

struct FxParams {
    tint: vec4<f32>,
    uv_offset: vec2<f32>,
    alpha_ref: f32,
    _pad: f32,
};

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> params: FxParams;
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var fx_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var fx_sampler: sampler;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // `DisableTexture` (`RENDER_COLOR`, untextured `RENDER_BRIGHT`): the color alone.
    var c = params.tint;
    @if(!FX_UNTEXTURED)
    {
        c = textureSample(fx_texture, fx_sampler, in.uv + params.uv_offset) * params.tint;
    }
    @if(VERTEX_COLORS)
    {
        c = c * in.color;
    }
    @if(FX_ALPHA_TEST)
    {
        if (c.a <= params.alpha_ref) {
            discard;
        }
    }
    return c;
}
