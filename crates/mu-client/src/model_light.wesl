// Model lighting from the original (`BMD::Transform` with `LightEnable` + `RenderMesh`):
// each vertex gets `BodyLight · max(normal·L · 0.8 + 0.4, 0.2)`, with L = (1.3, 0, 2) in MU
// (`HighLight` always on), clamped to [0, 1] like the OpenGL color. `BodyLight` is the terrain
// light where the model stands. The original multiplies in sRGB; here it is converted to linear.
//
// Also (`model_light.rs`): a flat color to multiply by (`glColor3fv`), chrome coordinates from
// the normal (`g_chrome`, `ZzzBMD.cpp:1054-1108`; copy of `render::chrome_uv`) and
// `RENDER_DARK` (the blend is set by `ModelLight::specialize`).

import bevy_pbr::render::{
    pbr_functions::{alpha_discard, main_pass_post_lighting_processing},
    pbr_fragment::pbr_input_from_standard_material,
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings::globals,
    mesh_functions,
};

struct ModelLight {
    // rgb: original light (not linearized); a: 1 = light by the normal, 0 = unlit.
    light: vec4<f32>,
};

struct ModelParams {
    // rgb: flat color (not linearized).
    tint: vec4<f32>,
    // 0 = model coordinates; 1..8 = `ChromeMode`.
    chrome: u32,
    // 1 = `RENDER_DARK`.
    dark: u32,
    // 1 = the light comes in the entity's `MeshTag` (8 bits per channel).
    tag_light: u32,
    _pad: u32,
};

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> model_light: ModelLight;
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var<uniform> params: ModelParams;

// MU's (1.3, 0, 2) in Bevy axes (x, z, -y).
const LIGHT_DIR: vec3<f32> = vec3<f32>(1.3, 2.0, 0.0);
// `LightVector` (`ZzzBMD.cpp:49`), in MU axes.
const LIGHT_VECTOR: vec3<f32> = vec3<f32>(0.0, -0.1, -0.8);

// `g_chrome` with the normal in MU axes and `WorldTime` in ms.
fn chrome_uv(mode: u32, n: vec3<f32>, world_time: f32) -> vec2<f32> {
    let wt = floor(world_time);
    let wave = (wt % 10000.0) * 0.0001;
    let wave2 = (wt % 5000.0) * 0.00024 - 0.4;
    let l = vec3<f32>(cos(world_time * 0.001), sin(world_time * 0.002), 1.0);
    switch mode {
        case 2u: { return vec2<f32>((n.z + n.x) * 0.8 + wave2 * 2.0, (n.y + n.x) + wave2 * 3.0); }
        case 3u: { return vec2<f32>(dot(n, LIGHT_VECTOR), 1.0 - dot(n, LIGHT_VECTOR)); }
        case 4u: { return vec2<f32>(dot(n, l) + n.y * 0.5 + l.y * 3.0, 1.0 - dot(n, l) - (n.z * 0.5 + wave * 3.0)); }
        case 5u: { return vec2<f32>(dot(n, l) + n.y * 3.0 + l.y * 5.0, 1.0 - dot(n, l) - (n.z * 2.5 + wave)); }
        case 6u: { let s = (n.z + n.x) * 0.8 + wave2 * 2.0; return vec2<f32>(s, s); }
        case 7u: { let s = (n.z + n.x) * 0.8 + world_time * 0.00006; return vec2<f32>(s, s); }
        case 1u: { return vec2<f32>(n.z * 0.5 + wave, n.y * 0.5 + wave * 2.0); }
        default: { return vec2<f32>(n.z * 0.5 + 0.2, n.y * 0.5 + 0.5); }
    }
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var vin = in;
    if params.chrome != 0u {
        // Unnormalized normal (interpolated like the original's coordinates), from Bevy to MU axes.
        let n = vec3<f32>(in.world_normal.x, -in.world_normal.z, in.world_normal.y);
        @if(VERTEX_UVS_A) {
            vin.uv = chrome_uv(params.chrome, n, globals.time * 1000.0);
        }
    }
    var pbr_input = pbr_input_from_standard_material(vin, is_front);
    pbr_input.material.base_color = alpha_discard(
        pbr_input.material.flags,
        pbr_input.material.alpha_cutoff,
        pbr_input.material.base_color
    );
    var color = pbr_input.material.base_color;
    if model_light.light.a > 0.5 {
        var body = model_light.light.rgb;
        if params.tag_light != 0u {
            @if(VERTEX_OUTPUT_INSTANCE_INDEX) {
                let tag = mesh_functions::get_tag(in.instance_index);
                body = vec3<f32>(f32(tag & 255u), f32((tag >> 8u) & 255u), f32((tag >> 16u) & 255u)) / 255.0;
            }
        }
        // Back faces do not flip the normal: neither does the original.
        let n = normalize(in.world_normal);
        let intensity = max(dot(n, LIGHT_DIR) * 0.8 + 0.4, 0.2);
        let l = clamp(body * intensity, vec3<f32>(0.0), vec3<f32>(1.0));
        color = vec4<f32>(color.rgb * pow(l, vec3<f32>(2.2)), color.a);
    }
    let tint = clamp(params.tint.rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    color = vec4<f32>(color.rgb * pow(tint, vec3<f32>(2.2)), color.a);
    if params.dark != 0u {
        color.a = 1.0;
    }
    var out: FragmentOutput;
    out.color = main_pass_post_lighting_processing(pbr_input, color);
    return out;
}
