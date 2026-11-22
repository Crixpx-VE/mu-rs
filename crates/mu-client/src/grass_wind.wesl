// Viento de la hierba (`TerrainGrassWind`, ZzzLodTerrain.cpp:2350-2403) en el sombreador de vértices:
// los de arriba de cada mata (uv_b.y = 1) se mueven en la Y de MU (-Z de Bevy) según su columna x
// (uv_b.x). Así la malla no se vuelve a subir a la tarjeta en cada fotograma.

import bevy_pbr::render::{
    mesh_functions,
    forward_io::{Vertex, VertexOutput, decompress_vertex},
    view_transformations::position_world_to_clip,
};

struct GrassWind {
    // velocidad, escala, multiplicador de x, unidades de MU por unidad de Bevy.
    wind: vec4<f32>,
    // Karutan: velocidad y escala del segundo viento; z = 1 si se usa.
    second: vec4<f32>,
};

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> grass: GrassWind;

@vertex
fn vertex(vin: Vertex) -> VertexOutput {
    var out: VertexOutput;
    var v = decompress_vertex(vin, vin.instance_index);
    @if(VERTEX_UVS_B) {
        if v.uv_b.y > 0.5 {
            let x = v.uv_b.x;
            var w = sin(grass.wind.x + x * grass.wind.z) * grass.wind.y;
            if grass.second.z > 0.5 {
                w = sin(grass.second.x + x * 50.0) * grass.second.y;
            }
            v.position.z = v.position.z - w / grass.wind.w;
        }
    }
    let world_from_local = mesh_functions::get_world_from_local(vin.instance_index);
    out.world_position = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(v.position, 1.0));
    out.position = position_world_to_clip(out.world_position.xyz);
    @if(VERTEX_NORMALS) {
        out.world_normal = mesh_functions::mesh_normal_local_to_world(v.normal, vin.instance_index);
    }
    @if(VERTEX_UVS_A) {
        out.uv = v.uv;
    }
    @if(VERTEX_UVS_B) {
        out.uv_b = v.uv_b;
    }
    @if(VERTEX_COLORS) {
        out.color = v.color;
    }
    @if(VERTEX_OUTPUT_INSTANCE_INDEX) {
        out.instance_index = vin.instance_index;
    }
    return out;
}
