// Luz de los modelos del original (`BMD::Transform` con `LightEnable` + `RenderMesh`):
// cada vértice recibe `BodyLight · max(normal·L · 0.8 + 0.4, 0.2)`, con L = (1.3, 0, 2) en MU
// (`HighLight` siempre activo), limitado a [0, 1] como el color de OpenGL. `BodyLight` es la luz
// del terreno donde está el modelo. El original multiplica en sRGB: aquí se pasa a lineal.

import bevy_pbr::render::{
    pbr_functions::{alpha_discard, main_pass_post_lighting_processing},
    pbr_fragment::pbr_input_from_standard_material,
    forward_io::{VertexOutput, FragmentOutput},
};

struct ModelLight {
    // rgb: luz del original (sin pasar a lineal); a: 1 = iluminar por la normal, 0 = sin luz.
    light: vec4<f32>,
};

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> model_light: ModelLight;

// (1.3, 0, 2) de MU en ejes de Bevy (x, z, -y).
const LIGHT_DIR: vec3<f32> = vec3<f32>(1.3, 2.0, 0.0);

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(
        pbr_input.material.flags,
        pbr_input.material.alpha_cutoff,
        pbr_input.material.base_color
    );
    var color = pbr_input.material.base_color;
    if model_light.light.a > 0.5 {
        // Sin girar la normal en las caras de atrás: el original tampoco lo hace.
        let n = normalize(in.world_normal);
        let intensity = max(dot(n, LIGHT_DIR) * 0.8 + 0.4, 0.2);
        let l = clamp(model_light.light.rgb * intensity, vec3<f32>(0.0), vec3<f32>(1.0));
        color = vec4<f32>(color.rgb * pow(l, vec3<f32>(2.2)), color.a);
    }
    var out: FragmentOutput;
    out.color = main_pass_post_lighting_processing(pbr_input, color);
    return out;
}
