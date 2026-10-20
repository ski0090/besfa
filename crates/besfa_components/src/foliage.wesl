// Sways foliage in the wind. Each vertex moves downwind as far as the square
// of its height above its mesh's origin, so the root stays put and the tips
// move most. Gusts roll across the ground along the wind and each plant
// flutters on its own, so neighbours do not move in step.
//
// Otherwise `bevy_pbr`'s mesh vertex shader, less skinning and morph targets.

import bevy_pbr::render::{
    forward_io::{Vertex, VertexOutput, decompress_vertex},
    mesh_functions,
    mesh_view_bindings::globals,
    view_transformations::position_world_to_clip,
};

// How far, in meters, a point a meter above the root sways.
@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> wind: f32;

const DOWNWIND: vec2<f32> = vec2(0.8, 0.6);

@vertex
fn vertex(in: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let vertex = decompress_vertex(in, in.instance_index);
    let world_from_local = mesh_functions::get_world_from_local(in.instance_index);

    out.world_position = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0));
    let root = world_from_local[3].xyz;
    let height = max(out.world_position.y - root.y, 0.0);
    let time = globals.time;
    // 0 to 1: a gust some 12 m from crest to crest, sweeping downwind.
    let gust = sin(dot(root.xz, DOWNWIND) * 0.5 - time * 1.5) * 0.5 + 0.5;
    let flutter = sin(time * 3.1 + root.x * 7.3 + root.z * 4.7);
    let sway = DOWNWIND * wind * height * height * (gust + 0.3 * flutter);
    out.world_position += vec4(sway.x, 0.0, sway.y, 0.0);
    out.position = position_world_to_clip(out.world_position.xyz);

@if(VERTEX_NORMALS)
    out.world_normal = mesh_functions::mesh_normal_local_to_world(vertex.normal, in.instance_index);
@if(VERTEX_UVS_A)
    out.uv = vertex.uv;
@if(VERTEX_UVS_B)
    out.uv_b = vertex.uv_b;
@if(VERTEX_TANGENTS)
    out.world_tangent = mesh_functions::mesh_tangent_local_to_world(world_from_local, vertex.tangent, in.instance_index);
@if(VERTEX_COLORS)
    out.color = vertex.color;
@if(VERTEX_OUTPUT_INSTANCE_INDEX)
    out.instance_index = in.instance_index;
@if(VISIBILITY_RANGE_DITHER)
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(in.instance_index, world_from_local[3]);

    return out;
}
