// Vignette post-processing shader
// Creates a radial darkening effect from screen edges (combat indicator)

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var texture_sampler: sampler;

struct VignetteSettings {
    intensity: f32,
    time: f32,
}

@group(0) @binding(2) var<uniform> settings: VignetteSettings;

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    // Sample the screen texture
    let color = textureSample(screen_texture, texture_sampler, in.uv);

    // Calculate distance from center (0.5, 0.5)
    let center = vec2<f32>(0.5, 0.5);
    let dist = distance(in.uv, center);

    // Vignette parameters - adjusted to only affect edges (10% penetration)
    // radius: where vignette starts (0.0 = center, 1.0 = edge)
    // softness: how gradually the vignette fades in
    let radius = 0.58;     // Start darkening at 58% from center
    let softness = 0.35;   // Very smooth gradient (gentle falloff)

    // Calculate vignette factor (0.0 = full vignette, 1.0 = no vignette)
    // smoothstep creates a smooth transition between radius and radius + softness
    let vignette = smoothstep(radius, radius - softness, dist);

    // Pulsing effect: oscillate intensity using sin wave
    // Pulse speed: 4.0 Hz (4 pulses per second - twice as fast)
    // Pulse depth: 0.1 (intensity varies by ±10%, very gentle)
    let pulse = sin(settings.time * 4.0) * 0.1 + 1.0; // Range: 0.9 to 1.1
    let pulsed_intensity = settings.intensity * pulse;

    // Apply red tint to the darkened areas for combat indication
    let red_tint = vec3<f32>(1.0, 0.4, 0.4); // Softer reddish tint

    // Mix between original color and tinted darker color based on vignette
    // When intensity is 0.0: no effect
    // When intensity is 1.0: full vignette with red tint and pulsing
    let darkened = color.rgb * red_tint * 0.65; // Much lighter darkening
    let final_color = mix(color.rgb, mix(darkened, color.rgb, vignette), pulsed_intensity);

    return vec4<f32>(final_color, color.a);
}
