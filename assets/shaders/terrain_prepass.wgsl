// Depth-only passes (shadow maps, any prepass) apply the same band cut as
// terrain.wgsl, so a plate the main pass drops casts no shadow and writes
// no depth. Normals are the only prepass target produced; motion-vector
// and deferred prepasses are not supported.
#import bevy_pbr::prepass_io

// Matches TerrainCut in terrain.wgsl.
struct TerrainCut {
    center: vec2<f32>,
    inner: f32,
    outer: f32,
}
@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> terrain_cut: TerrainCut;

fn band_cut(world_xz: vec2<f32>) {
    let d = length(world_xz - terrain_cut.center);
    if d < terrain_cut.inner || d > terrain_cut.outer {
        discard;
    }
}

#ifdef PREPASS_FRAGMENT
@fragment
fn fragment(in: prepass_io::VertexOutput) -> prepass_io::FragmentOutput {
    band_cut(in.world_position.xz);

    var out: prepass_io::FragmentOutput;
#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    out.frag_depth = in.unclipped_depth;
#endif
#ifdef NORMAL_PREPASS
    out.normal = vec4(in.world_normal * 0.5 + vec3(0.5), 1.0);
#endif
    return out;
}
#else
@fragment
fn fragment(in: prepass_io::VertexOutput) {
    band_cut(in.world_position.xz);
}
#endif
