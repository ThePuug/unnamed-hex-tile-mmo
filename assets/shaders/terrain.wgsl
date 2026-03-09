#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
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

// Hex tile rise (vertical spacing per elevation unit).
// Must match qrz::Map::new(radius, rise) in main.rs.
const RISE: f32 = 0.8;

// Number of elevation ramp stops.
const RAMP_LEN: i32 = 16;

// Elevation ramp: (elevation, R, G, B) normalized to [0,1].
// Ordered low to high. Shader interpolates between adjacent stops.
const RAMP_E: array<f32, 16> = array<f32, 16>(
    -200.0, -50.0, 0.0, 10.0, 30.0, 150.0, 400.0, 700.0,
    1200.0, 1600.0, 2000.0, 2500.0, 3000.0, 3400.0, 3700.0, 4000.0
);
const RAMP_R: array<f32, 16> = array<f32, 16>(
    0.039, 0.118, 0.275, 0.824, 0.314, 0.392, 0.510, 0.549,
    0.471, 0.412, 0.510, 0.627, 0.745, 0.863, 0.961, 1.000
);
const RAMP_G: array<f32, 16> = array<f32, 16>(
    0.078, 0.235, 0.588, 0.784, 0.627, 0.569, 0.510, 0.431,
    0.392, 0.373, 0.490, 0.608, 0.725, 0.863, 0.961, 1.000
);
const RAMP_B: array<f32, 16> = array<f32, 16>(
    0.314, 0.627, 0.627, 0.588, 0.314, 0.235, 0.196, 0.216,
    0.275, 0.333, 0.471, 0.588, 0.706, 0.863, 0.961, 1.000
);

// Per-tile brightness noise strength (±10%).
const NOISE_STRENGTH: f32 = 0.10;

// Cliff face color (stone grey), linear RGB.
const CLIFF_COLOR: vec3<f32> = vec3<f32>(0.35, 0.32, 0.28);

// Normal Y threshold: below this, the face is treated as a cliff.
const CLIFF_NORMAL_THRESHOLD: f32 = 0.3;

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

/// Cheap deterministic hash noise from tile-quantized world XZ.
/// Returns a value in [-1, 1]. Same tile always produces the same value.
fn tile_noise(world_xz: vec2<f32>) -> f32 {
    // Quantize to tile grid (radius = 1.0, hex spacing ≈ sqrt(3)/2 ≈ 0.866)
    let cell = floor(world_xz);
    let h = fract(sin(dot(cell, vec2<f32>(127.1, 311.7))) * 43758.5453);
    return h * 2.0 - 1.0;
}

@fragment
fn fragment(
    vertex_output: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var in = vertex_output;

    // Build PBR input from the base StandardMaterial
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    // Convert world Y to elevation (undo rise offset + rise-per-level scaling)
    let elevation = (in.world_position.y - RISE) / RISE;

    // Determine base color: cliff faces get stone grey, top surfaces get elevation color
    var base: vec3<f32>;
    if abs(in.world_normal.y) < CLIFF_NORMAL_THRESHOLD {
        base = CLIFF_COLOR;
    } else {
        base = elevation_color(elevation);
        // Apply per-tile brightness noise
        let noise = tile_noise(in.world_position.xz);
        let variation = 1.0 + noise * NOISE_STRENGTH;
        base = clamp(base * variation, vec3<f32>(0.0), vec3<f32>(1.0));
    }

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
