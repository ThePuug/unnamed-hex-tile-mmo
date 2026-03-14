#import bevy_pbr::{
    mesh_functions,
    forward_io::{Vertex, VertexOutput},
    view_transformations::position_world_to_clip,
    mesh_view_bindings::view,
}

// Maximum world-space Y to push boundary-ring summary vertices beneath
// full-detail terrain. Must exceed the maximum elevation difference
// between any full-detail surface and its summary.
const MAX_TUCK_Y: f32 = 20.0;

// World-space Euclidean distance between adjacent chunk centers.
const CHUNK_EXTENT: f32 = 28.5;

// Minimum detail radius in chunks (must match chunk::FOV_CHUNK_RADIUS).
const FOV_CHUNK_RADIUS: f32 = 5.0;

@vertex
fn vertex(vertex_no_morph: Vertex) -> VertexOutput {
    var out: VertexOutput;
    var vertex = vertex_no_morph;

    // Standard world-space transform
    var world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    out.world_position = mesh_functions::mesh_position_local_to_world(
        world_from_local, vec4<f32>(vertex.position, 1.0));
    out.world_normal = mesh_functions::mesh_normal_local_to_world(
        vertex.normal, vertex.instance_index);
    out.uv = vertex.uv;
    out.instance_index = vertex.instance_index;

#ifdef VERTEX_UVS_B
    out.uv_b = vertex.uv_b;
#endif
#ifdef VERTEX_TANGENTS
    out.world_tangent = mesh_functions::mesh_tangent_local_to_world(
        world_from_local, vertex.tangent, vertex.instance_index);
#endif
#ifdef VERTEX_COLORS
    out.color = vertex.color;
#endif

    // ── Dynamic tuck for boundary-ring summary vertices ──
    // uv.y > 0.75  → tuckable (inner-facing corner)
    // uv.y ∈ (0.25, 0.75] → summary but non-tuckable (outer-facing, meets LoD1)
    // uv.y ≤ 0.25  → full-detail vertex (no tuck)
    if vertex.uv.y > 0.75 {
        // Derive detail boundary from camera altitude and vertical FOV.
        // clip_from_view[1][1] = 1/tan(fov/2) for perspective projection.
        let tan_half_fov = 1.0 / view.clip_from_view[1][1];
        let camera_alt = view.world_position.y;
        let ground_dist = camera_alt * tan_half_fov;
        let detail_radius = max(ceil(ground_dist / CHUNK_EXTENT), FOV_CHUNK_RADIUS) + 1.0;

        let inner_dist = (detail_radius - 0.5) * CHUNK_EXTENT;
        let outer_dist = (detail_radius + 0.5) * CHUNK_EXTENT;

        // XZ distance from camera (≈ player) to this vertex
        let dx = out.world_position.x - view.world_position.x;
        let dz = out.world_position.z - view.world_position.z;
        let vertex_dist = sqrt(dx * dx + dz * dz);

        // Smoothstepped tuck: max at inner_dist, zero at outer_dist
        let band = outer_dist - inner_dist;
        if band > 0.0 {
            let t = clamp((outer_dist - vertex_dist) / band, 0.0, 1.0);
            let s = t * t * (3.0 - 2.0 * t);
            out.world_position.y -= MAX_TUCK_Y * s;
        }
    }

    out.position = position_world_to_clip(out.world_position.xyz);

    return out;
}
