// Material de los efectos: textura × color del vértice × tinte, con las mezclas del original
// (se eligen en `FxMaterial::specialize`). Sin luz: los efectos de MU no se iluminan.

#import bevy_pbr::forward_io::VertexOutput

struct FxParams {
    tint: vec4<f32>,
    uv_offset: vec2<f32>,
    alpha_ref: f32,
    _pad: f32,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: FxParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var fx_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var fx_sampler: sampler;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    var c = textureSample(fx_texture, fx_sampler, in.uv + params.uv_offset) * params.tint;
#ifdef VERTEX_COLORS
    c = c * in.color;
#endif
#ifdef FX_ALPHA_TEST
    if (c.a <= params.alpha_ref) {
        discard;
    }
#endif
    return c;
}
