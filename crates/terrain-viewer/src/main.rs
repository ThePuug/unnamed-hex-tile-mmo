use std::time::Instant;

use clap::{Parser, ValueEnum};
use image::{Rgb, RgbImage};
use rayon::prelude::*;
use terrain::{hex_to_world, Terrain, ThermalChunkCache};

const SQRT_3: f64 = 1.7320508075688772;

fn cart_to_hex(x: f64, y: f64) -> (f64, f64) {
    let r = y * 2.0 / SQRT_3;
    let q = x - r * 0.5;
    (q, r)
}

/// Round fractional hex coordinates to the nearest hex tile.
/// Uses cube coordinate rounding (q + r + s = 0 constraint) instead of
/// independent axis rounding, which would shear tile boundaries along the
/// 60° hex axes.
fn hex_round(q: f64, r: f64) -> (i32, i32) {
    let s = -q - r;
    let mut rq = q.round();
    let mut rr = r.round();
    let rs = s.round();

    let dq = (rq - q).abs();
    let dr = (rr - r).abs();
    let ds = (rs - s).abs();

    if dq > dr && dq > ds {
        rq = -rr - rs;
    } else if dr > ds {
        rr = -rq - rs;
    }

    (rq as i32, rr as i32)
}

#[derive(Parser)]
#[command(name = "terrain-viewer", about = "Render terrain heightmaps to PNG")]
struct Cli {
    #[arg(long, default_value_t = 0)]
    center_q: i32,

    #[arg(long, default_value_t = 0)]
    center_r: i32,

    #[arg(long, default_value_t = 15000)]
    radius: i32,

    #[arg(long, default_value = "terrain.png")]
    output: String,

    #[arg(long, default_value_t = 2)]
    scale: i32,

    #[arg(long, default_value_t = 0)]
    seed: u64,

    #[arg(long, value_enum, default_value_t = Mode::Material)]
    mode: Mode,

    #[arg(long, default_value_t = 0)]
    tick: u64,
}

#[derive(Clone, Copy, ValueEnum)]
enum Mode {
    /// Primordial material density: dense = dark red/brown, light = white/gray
    Material,
    /// Additive Gaussian surface temperature: hot plumes → red/yellow, cool → dark
    Thermal,
    /// Raw hotspot-layer cells before surface expansion (diagnostic)
    Hotspots,
}

/// Material density → color: dense = dark red/brown, light = white/gray
fn material_color(density: f64) -> Rgb<u8> {
    // Remap from expected range [0.1, 0.9] to [0, 1] for visual contrast
    let t = ((density - 0.1) / 0.8).clamp(0.0, 1.0);

    let (r, g, b) = if t > 0.75 {
        // Densest — dark red/brown (resists convection)
        let s = (t - 0.75) / 0.25;
        (0.4 + s * 0.2, 0.1 - s * 0.05, 0.05)
    } else if t > 0.5 {
        // Medium-dense — warm brown/tan
        let s = (t - 0.5) / 0.25;
        (0.6 - s * 0.2, 0.35 - s * 0.25, 0.15 - s * 0.1)
    } else if t > 0.25 {
        // Light — neutral gray to warm brown
        let s = (t - 0.25) / 0.25;
        (0.6 - s * 0.0, 0.6 - s * 0.25, 0.6 - s * 0.45)
    } else {
        // Lightest — white to neutral gray
        let s = t / 0.25;
        (1.0 - s * 0.4, 1.0 - s * 0.4, 1.0 - s * 0.4)
    };

    Rgb([
        (r * 255.0) as u8,
        (g * 255.0) as u8,
        (b * 255.0) as u8,
    ])
}

/// Temperature → heat ramp: 0.0 (cool boundary) = dark blue, 1.0 (hot center) = bright yellow
fn thermal_color(temperature: f64) -> Rgb<u8> {
    let t = temperature.clamp(0.0, 1.0);

    // 5-stop ramp: black-blue → blue → red → orange → yellow
    let (r, g, b) = if t < 0.25 {
        let s = t / 0.25;
        (0.0, 0.0, s * 0.6)
    } else if t < 0.5 {
        let s = (t - 0.25) / 0.25;
        (s * 0.8, 0.0, 0.6 - s * 0.3)
    } else if t < 0.75 {
        let s = (t - 0.5) / 0.25;
        (0.8 + s * 0.2, s * 0.5, 0.3 - s * 0.3)
    } else {
        let s = (t - 0.75) / 0.25;
        (1.0, 0.5 + s * 0.5, s * 0.2)
    };

    Rgb([
        (r * 255.0) as u8,
        (g * 255.0) as u8,
        (b * 255.0) as u8,
    ])
}

fn main() {
    let cli = Cli::parse();

    let terrain = Terrain::with_tick(cli.seed, cli.tick);
    let scale = cli.scale.max(1);

    let radius_f = cli.radius as f64;
    let scale_f = scale as f64;
    let (center_x, center_y) = hex_to_world(cli.center_q, cli.center_r);
    let pixel_diameter = (radius_f * 2.0 / scale_f) as u32;
    let width = pixel_diameter;
    let height = pixel_diameter;

    eprintln!(
        "Generating {} mode: center=({},{}) radius={} scale={} seed={} tick={} -> {}x{} image",
        mode_name(cli.mode), cli.center_q, cli.center_r, cli.radius, scale, cli.seed, cli.tick, width, height
    );

    let start = Instant::now();

    let mode = cli.mode;
    let seed = cli.seed;
    let tick = cli.tick;
    let terrain_ref = &terrain;
    let pixels: Vec<(u32, u32, Rgb<u8>)> = (0..height)
        .into_par_iter()
        .flat_map(|py| {
            // Per-row thermal cache — amortizes chunk computation across pixels in the same row
            let mut thermal_cache = ThermalChunkCache::new(seed, tick);
            (0..width).map(move |px| {
                let cart_x = center_x - radius_f + (px as f64) * scale_f;
                let cart_y = center_y - radius_f + (py as f64) * scale_f;
                let (hq, hr) = cart_to_hex(cart_x, cart_y);
                let (q, r) = hex_round(hq, hr);

                let color = match mode {
                    Mode::Material => material_color(terrain_ref.material_density(q, r)),
                    Mode::Thermal => {
                        let temp = thermal_cache.temperature_at_tile(q, r);
                        thermal_color(temp)
                    }
                    Mode::Hotspots => thermal_color(terrain_ref.hotspot_temperature(q, r)),
                };

                (px, py, color)
            }).collect::<Vec<_>>()
        })
        .collect();

    let elapsed_eval = start.elapsed();
    let tile_count = (width as u64) * (height as u64);
    eprintln!("Evaluated {} tiles in {:.2}s ({:.0} tiles/sec)",
        tile_count,
        elapsed_eval.as_secs_f64(),
        tile_count as f64 / elapsed_eval.as_secs_f64()
    );

    let mut img = RgbImage::new(width, height);
    for (px, py, color) in pixels {
        img.put_pixel(px, py, color);
    }

    img.save(&cli.output).expect("Failed to save PNG");
    let total = start.elapsed();
    eprintln!("Saved {} ({:.2}s total)", cli.output, total.as_secs_f64());
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Material => "material",
        Mode::Thermal => "thermal",
        Mode::Hotspots => "hotspots",
    }
}
