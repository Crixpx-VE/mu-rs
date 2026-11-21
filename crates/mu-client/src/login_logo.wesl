// Logo MU de la pantalla de login (`NewRenderLogInScene` de ZzzScene.cpp): textura × color.
// El brillo se suma (`EnableAlphaBlend`, ONE/ONE: la mezcla la pone `LogoMaterial::specialize`);
// el logo se mezcla por alfa y descarta lo que no pasa de 0.25 (`EnableAlphaTest`, `glAlphaFunc`).

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
