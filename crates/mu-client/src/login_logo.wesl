// MU logo of the login screen (`NewRenderLogInScene` of ZzzScene.cpp): texture × color.
// The glow is additive (`EnableAlphaBlend`, ONE/ONE: the blend is set by `LogoMaterial::specialize`);
// the logo is alpha-blended and discards what does not exceed 0.25 (`EnableAlphaTest`, `glAlphaFunc`).

import bevy_ui_render::ui_vertex_output::UiVertexOutput;

struct LogoParams {
    color: vec4<f32>,
};

@group(1) @binding(0) var<uniform> params: LogoParams;
@group(1) @binding(1) var logo_texture: texture_2d<f32>;
@group(1) @binding(2) var logo_sampler: sampler;

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let c = textureSample(logo_texture, logo_sampler, in.uv) * params.color;
    @if(LOGO_ALPHA_TEST)
    {
        if (c.a <= 0.25) {
            discard;
        }
    }
    return c;
}
