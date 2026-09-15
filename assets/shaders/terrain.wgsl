#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
    mesh_view_bindings::view,
}

#ifdef PREPASS_PIPELINE
#import bevy_pbr::{
    prepass_io::{VertexOutput, FragmentOutput},
    pbr_deferred_functions::deferred_output,
}
#else
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
}
#endif

// Band cut for this material's LoD level (client TerrainCut; the same
// declaration lives in terrain_prepass.wgsl). Fragments whose ground
// distance from `center` lies outside [inner, outer] are dropped: regions
// are built whole, and this is what confines a level to its band.
struct TerrainCut {
    center: vec2<f32>,
    inner: f32,
    outer: f32,
}
@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> terrain_cut: TerrainCut;

// Surface albedos (client TerrainExtension), repeated over world space
// since the terrain carries no per-tile UVs: grass and scree over XZ on the
// tops, stone over the vertical planes on the faces. Each is an array of
// three seeds of the same tile. A triangle lattice covers the surface, and
// each lattice vertex picks a seed and its own offset, rotation and scale
// for the tile; a fragment samples once per vertex of its triangle and
// blends by barycentric weight, so no two patches of ground repeat each
// other. Every seed tiles with itself, so a cross-fade shows no seam.
// Loaded PNGs have no mipmaps, so the repeats are sized to put a texel
// near a screen pixel at gameplay height; a finer repeat shimmers with
// distance.
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var grass_texture: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var grass_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var cliff_texture: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var cliff_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(105) var scree_texture: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(106) var scree_sampler: sampler;
const GRASS_REPEAT_WU: f32 = 8.0;
const CLIFF_REPEAT_WU: f32 = 8.0;
const SCREE_REPEAT_WU: f32 = 8.0;
// Lattice spacing for the per-vertex seed and transform, about one repeat,
// so neighbouring repeats never share a transform. Weights are sharpened
// by this power so the blend between vertices stays a narrow band and the
// tile's detail is not averaged away over most of the ground.
const BOMB_CELL_WU: f32 = 10.0;
const BOMB_SHARPNESS: f32 = 4.0;
// Scale range around the base repeat, and rotation range on the faces,
// where up must stay up.
const BOMB_SCALE_MIN: f32 = 0.75;
const BOMB_SCALE_MAX: f32 = 1.3;
const BOMB_FACE_TILT: f32 = 0.25;

fn band_cut(world_xz: vec2<f32>) {
    let d = length(world_xz - terrain_cut.center);
    if d < terrain_cut.inner || d > terrain_cut.outer {
        discard;
    }
}

// Horizon haze color (light blue-grey, linear RGB).
const HORIZON_COLOR: vec3<f32> = vec3<f32>(0.72, 0.78, 0.85);

// Camera geometry constants (must match camera.rs).
// CAMERA_HEIGHT approximates camera_total_height at sea level
// (camera_height(60°) = 84, floored at 90 for zoom independence).
const CAMERA_HEIGHT: f32 = 90.0;

// Atmospheric fade tied to the LoD band horizon (same formula as the band
// math: far_ground = camera_total_height / tan(HORIZON_MARGIN_DEG = 5°)).
// Haze completes slightly inside the horizon so the geometry frontier pops
// behind full haze; everything nearer renders visibly.
const INV_TAN_HORIZON_MARGIN: f32 = 11.43;  // 1 / tan(5°)
const FADE_END_FRAC: f32 = 0.92;            // full haze at 92% of horizon
const FADE_START_FRAC: f32 = 0.80;          // fade begins at 80% of fade_end

// Hex tile rise (vertical spacing per elevation unit).
// Must match common::camera::RISE.
const RISE: f32 = 0.8;

// Number of elevation ramp stops.
const RAMP_LEN: i32 = 14;

// Elevation ramp: (elevation, R, G, B) normalized to [0,1].
// Ordered low to high. Shader interpolates between adjacent stops.
//
// Anchored to the substrate and orogen scales, not to spine cones:
//   -200  SEA_MAX_DEPTH, the abyssal plain
//    -50  shelf break, a quarter of that depth
//      0  sea level, the substrate's datum
//      5  beach — the substrate's land p10 is 8.7, so the sand band is thin
//     20  substrate land p50: the median land tile is green plain
//     45  CONTINENT_MAX_RISE — the top of land the substrate makes alone, so
//         everything above this stop is orogen and everything below is plain
//    120  10% of OROGEN_MAX_RISE, where belt relief starts to read as upland
//    300  25% — dry olive
//    600  50% — brown mountain flank
//    800  grey begins; below the 75% stop so rock precedes the last quarter
//    950  75% of ceiling, rock
//   1050  bare pale rock
//   1150  snowline: the top 4% of the ceiling only
//   1200  OROGEN_MAX_RISE, snow
const RAMP_E: array<f32, 14> = array<f32, 14>(
    -200.0, -50.0, 0.0, 5.0, 20.0, 45.0, 120.0, 300.0,
    600.0, 800.0, 950.0, 1050.0, 1150.0, 1200.0
);
const RAMP_R: array<f32, 14> = array<f32, 14>(
    0.039, 0.118, 0.275, 0.824, 0.314, 0.290, 0.353, 0.510,
    0.549, 0.471, 0.510, 0.647, 0.863, 1.000
);
const RAMP_G: array<f32, 14> = array<f32, 14>(
    0.078, 0.235, 0.588, 0.784, 0.627, 0.580, 0.569, 0.510,
    0.431, 0.392, 0.490, 0.635, 0.863, 1.000
);
const RAMP_B: array<f32, 14> = array<f32, 14>(
    0.314, 0.627, 0.627, 0.588, 0.314, 0.290, 0.267, 0.196,
    0.216, 0.275, 0.471, 0.620, 0.863, 1.000
);

// Cliff face colour (stone grey), linear RGB: the mean the stone tile is
// pinned to in texgen, so the faces keep this colour at a distance.
const CLIFF_COLOR: vec3<f32> = vec3<f32>(0.35, 0.32, 0.28);

// Slope band over which a surface turns to stone, as normal Y: all stone
// from 50° (0.64), none below 40° (0.77). Straddles the 45° at which two
// tiles differ by more steps than can be stepped up, so a face that blocks
// reads as rock; corners are shared, so normals vary smoothly and the
// band blends rather than cuts.
const CLIFF_NORMAL_FULL: f32 = 0.64;
const CLIFF_NORMAL_NONE: f32 = 0.77;

/// How much of a top is grass: the ramp's green band, full between the
/// plain and continent stops, growing in over the beach and thinning out
/// toward the upland.
fn grass_weight(elev: f32) -> f32 {
    return smoothstep(RAMP_E[3], RAMP_E[4], elev) * (1.0 - smoothstep(RAMP_E[5], RAMP_E[6], elev));
}

/// How much of a top is scree: the ramp's mountain band, growing in from
/// the dry olive stop to the brown flank and thinning out where bare pale
/// rock begins.
fn scree_weight(elev: f32) -> f32 {
    return smoothstep(RAMP_E[7], RAMP_E[8], elev) * (1.0 - smoothstep(RAMP_E[10], RAMP_E[11], elev));
}

/// Stone on a face: the tile projected along each horizontal axis, with
/// world up as the tile's up (image rows run downward, hence -y), the two
/// blended by how squarely the face meets each axis. The lattice lies on
/// the face too, so the rotation is kept to a tilt.
fn cliff_albedo(world_position: vec3<f32>, world_normal: vec3<f32>) -> vec3<f32> {
    let n = abs(world_normal.xz);
    let w = n / max(n.x + n.y, 1e-3);
    let along_x = sample_bomb(cliff_texture, cliff_sampler, bomb(vec2<f32>(world_position.z, -world_position.y), CLIFF_REPEAT_WU, BOMB_FACE_TILT));
    let along_z = sample_bomb(cliff_texture, cliff_sampler, bomb(vec2<f32>(world_position.x, -world_position.y), CLIFF_REPEAT_WU, BOMB_FACE_TILT));
    return along_x * w.x + along_z * w.y;
}

fn luminance(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

/// Interpolate the elevation color ramp.
fn elevation_color(elev: f32) -> vec3<f32> {
    // Clamp to ramp bounds
    if elev <= RAMP_E[0] {
        return vec3<f32>(RAMP_R[0], RAMP_G[0], RAMP_B[0]);
    }
    if elev >= RAMP_E[RAMP_LEN - 1] {
        return vec3<f32>(RAMP_R[RAMP_LEN - 1], RAMP_G[RAMP_LEN - 1], RAMP_B[RAMP_LEN - 1]);
    }

    // Find the ramp segment and interpolate
    for (var i = 0; i < RAMP_LEN - 1; i++) {
        if elev >= RAMP_E[i] && elev < RAMP_E[i + 1] {
            let t = (elev - RAMP_E[i]) / (RAMP_E[i + 1] - RAMP_E[i]);
            return vec3<f32>(
                mix(RAMP_R[i], RAMP_R[i + 1], t),
                mix(RAMP_G[i], RAMP_G[i + 1], t),
                mix(RAMP_B[i], RAMP_B[i + 1], t),
            );
        }
    }

    return vec3<f32>(0.5, 0.5, 0.5);
}

/// Cheap deterministic hash of a lattice point to [0, 1).
fn cell_hash(cell: vec2<f32>) -> f32 {
    return fract(sin(dot(cell, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

/// One texture lookup per vertex of the lattice triangle a point falls in:
/// the tile coordinates under that vertex's transform, the seed it picked,
/// and the sharpened barycentric weight.
struct Bomb {
    uv0: vec2<f32>,
    uv1: vec2<f32>,
    uv2: vec2<f32>,
    layer: vec3<i32>,
    weight: vec3<f32>,
}

/// Tile coordinates for `p` (already in repeats) under lattice vertex `v`:
/// scaled, rotated and offset by the vertex's own hashes. `tilt` bounds
/// the rotation, in radians either way.
fn bomb_uv(p: vec2<f32>, v: vec2<f32>, tilt: f32) -> vec2<f32> {
    let scale = mix(BOMB_SCALE_MIN, BOMB_SCALE_MAX, cell_hash(v + vec2<f32>(17.3, 5.1)));
    let angle = (cell_hash(v + vec2<f32>(41.7, 23.9)) - 0.5) * 2.0 * tilt;
    let offset = vec2<f32>(cell_hash(v + vec2<f32>(3.3, 71.1)), cell_hash(v + vec2<f32>(59.9, 13.7)));
    let c = cos(angle);
    let s = sin(angle);
    return mat2x2<f32>(vec2<f32>(c, s), vec2<f32>(-s, c)) * (p * scale) + offset;
}

fn bomb_layer(v: vec2<f32>) -> i32 {
    return i32(cell_hash(v + vec2<f32>(29.1, 83.3)) * 3.0) % 3;
}

/// The lattice triangle around `surface` (in world units on the surface
/// being textured) and the three lookups it calls for. `repeat` is the
/// tile's base size in world units.
fn bomb(surface: vec2<f32>, repeat: f32, tilt: f32) -> Bomb {
    // Skew the plane so unit squares become pairs of equilateral triangles.
    let skewed = mat2x2<f32>(vec2<f32>(1.0, 0.0), vec2<f32>(-0.57735027, 1.15470054)) * (surface / BOMB_CELL_WU);
    let base = floor(skewed);
    let f = fract(skewed);
    var v0: vec2<f32>;
    var v1: vec2<f32>;
    var v2: vec2<f32>;
    var w: vec3<f32>;
    if f.x + f.y < 1.0 {
        v0 = base;
        v1 = base + vec2<f32>(1.0, 0.0);
        v2 = base + vec2<f32>(0.0, 1.0);
        w = vec3<f32>(1.0 - f.x - f.y, f.x, f.y);
    } else {
        v0 = base + vec2<f32>(1.0, 1.0);
        v1 = base + vec2<f32>(1.0, 0.0);
        v2 = base + vec2<f32>(0.0, 1.0);
        w = vec3<f32>(f.x + f.y - 1.0, 1.0 - f.y, 1.0 - f.x);
    }
    w = pow(w, vec3<f32>(BOMB_SHARPNESS));
    w = w / (w.x + w.y + w.z);
    let p = surface / repeat;
    var out: Bomb;
    out.uv0 = bomb_uv(p, v0, tilt);
    out.uv1 = bomb_uv(p, v1, tilt);
    out.uv2 = bomb_uv(p, v2, tilt);
    out.layer = vec3<i32>(bomb_layer(v0), bomb_layer(v1), bomb_layer(v2));
    out.weight = w;
    return out;
}

/// The three lookups of a bomb, blended.
fn sample_bomb(tex: texture_2d_array<f32>, samp: sampler, b: Bomb) -> vec3<f32> {
    return textureSample(tex, samp, b.uv0, b.layer.x).rgb * b.weight.x
        + textureSample(tex, samp, b.uv1, b.layer.y).rgb * b.weight.y
        + textureSample(tex, samp, b.uv2, b.layer.z).rgb * b.weight.z;
}

@fragment
fn fragment(
    vertex_output: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var in = vertex_output;

    band_cut(in.world_position.xz);

    // Build PBR input from the base StandardMaterial
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    // Every layer is sampled; the slope blends them below.
    let grass = sample_bomb(grass_texture, grass_sampler, bomb(in.world_position.xz, GRASS_REPEAT_WU, 3.14159265));
    let scree = sample_bomb(scree_texture, scree_sampler, bomb(in.world_position.xz, SCREE_REPEAT_WU, 3.14159265));
    let cliff = cliff_albedo(in.world_position.xyz, in.world_normal);

    // Convert world Y to elevation (undo rise offset + rise-per-level scaling).
    let elevation = (in.world_position.y - RISE) / RISE;

    // Faces wear the stone tile, which averages to the cliff grey. Tops take
    // the ramp, with the grass tile over its green band and the scree tile
    // over its mountain band. Grass averages to the plain stop, so it is
    // scaled to the ramp's brightness at this elevation and the ramp keeps
    // its own light-to-dark across the band; scree is its own colour and
    // stands in for the ramp where it is full.
    let ramp = elevation_color(elevation);
    let plain = vec3<f32>(RAMP_R[4], RAMP_G[4], RAMP_B[4]);
    let lit = grass * (luminance(ramp) / luminance(plain));
    var top = mix(ramp, lit, grass_weight(elevation));
    top = mix(top, scree, scree_weight(elevation));
    let stone = 1.0 - smoothstep(CLIFF_NORMAL_FULL, CLIFF_NORMAL_NONE, abs(in.world_normal.y));
    var base = mix(top, cliff, stone);

    // Atmospheric fade: derive the visual horizon from camera altitude with
    // the same formula the LoD band math uses, then blend toward haze in the
    // outer portion so the geometry frontier is always behind full haze.
    let camera_alt = max(view.world_position.y, CAMERA_HEIGHT);
    let fade_end = camera_alt * INV_TAN_HORIZON_MARGIN * FADE_END_FRAC;
    let fade_start = fade_end * FADE_START_FRAC;
    let dist = length(in.world_position.xz - view.world_position.xz);
    let fade_t = smoothstep(fade_start, fade_end, dist);
    base = mix(base, HORIZON_COLOR, fade_t);

    pbr_input.material.base_color = vec4<f32>(base, 1.0);

    // Alpha discard (standard pipeline step)
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

#ifdef PREPASS_PIPELINE
    let out = deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif

    return out;
}
