use std::time::Instant;

use clap::{Parser, ValueEnum};
use image::{Rgb, RgbImage};
use rayon::prelude::*;
use terrain::{BoundaryKind, BoundaryScale, Terrain, TerrainEval};

const SQRT_3: f64 = 1.7320508075688772;

/// Convert hex axial coordinates to isotropic Cartesian (unit-hex-spacing).
fn hex_to_pixel(q: f64, r: f64) -> (f64, f64) {
    (q + r * 0.5, r * SQRT_3 / 2.0)
}

/// Convert isotropic Cartesian back to hex axial coordinates.
fn pixel_to_hex(x: f64, y: f64) -> (f64, f64) {
    let r = y * 2.0 / SQRT_3;
    let q = x - r * 0.5;
    (q, r)
}

#[derive(Parser)]
#[command(name = "terrain-viewer", about = "Render terrain heightmaps to PNG")]
struct Cli {
    /// Center Q coordinate (hex axial)
    #[arg(long, default_value_t = 0)]
    center_q: i32,

    /// Center R coordinate (hex axial)
    #[arg(long, default_value_t = 0)]
    center_r: i32,

    /// Radius in tiles from center
    #[arg(long, default_value_t = 15000)]
    radius: i32,

    /// Output file path
    #[arg(long, default_value = "terrain.png")]
    output: String,

    /// Tiles per pixel (downsampling)
    #[arg(long, default_value_t = 2)]
    scale: i32,

    /// World seed
    #[arg(long, default_value_t = 0)]
    seed: u64,

    /// Visualization mode
    #[arg(long, value_enum, default_value_t = Mode::Elevation)]
    mode: Mode,
}

#[derive(Clone, Copy, ValueEnum)]
enum Mode {
    /// Height mapped to topographic color ramp
    Elevation,
    /// Each plate a distinct color
    Plates,
    /// Convergent (red), divergent (blue), transform (yellow), interior (gray)
    BoundaryType,
    /// Continental (green) vs oceanic (blue)
    PlateCharacter,
    /// Gradient magnitude: white = steep, black = flat
    Slope,
}

/// Topographic color ramp anchored to expected elevation ranges.
/// Deep blue (ocean) → cyan (coast) → green (lowland) → yellow (mid) → brown (high) → white (peak)
fn elevation_color(height: i32) -> Rgb<u8> {
    let h = height as f64;
    // Ramp stops: (elevation, R, G, B)
    const RAMP: &[(f64, u8, u8, u8)] = &[
        (-300.0, 10, 20, 80),     // deep ocean
        (50.0, 30, 60, 160),      // ocean
        (150.0, 60, 130, 200),    // shallow ocean
        (250.0, 80, 180, 180),    // coastal
        (400.0, 80, 160, 80),     // lowland green
        (700.0, 160, 180, 60),    // mid elevation yellow-green
        (1000.0, 200, 170, 50),   // yellow
        (1400.0, 170, 120, 50),   // brown
        (2000.0, 200, 190, 180),  // light brown / gray
        (3000.0, 255, 255, 255),  // white peaks
    ];

    if h <= RAMP[0].0 {
        return Rgb([RAMP[0].1, RAMP[0].2, RAMP[0].3]);
    }
    if h >= RAMP[RAMP.len() - 1].0 {
        let last = RAMP[RAMP.len() - 1];
        return Rgb([last.1, last.2, last.3]);
    }

    for i in 0..RAMP.len() - 1 {
        let (e0, r0, g0, b0) = RAMP[i];
        let (e1, r1, g1, b1) = RAMP[i + 1];
        if h >= e0 && h < e1 {
            let t = (h - e0) / (e1 - e0);
            let r = (r0 as f64 + (r1 as f64 - r0 as f64) * t) as u8;
            let g = (g0 as f64 + (g1 as f64 - g0 as f64) * t) as u8;
            let b = (b0 as f64 + (b1 as f64 - b0 as f64) * t) as u8;
            return Rgb([r, g, b]);
        }
    }
    Rgb([128, 128, 128])
}

/// Hash a plate ID to a distinct color.
/// Continental plates use full saturation, regional plates use lighter tones.
fn plate_color(cell_q: i32, cell_r: i32, is_continental: bool) -> Rgb<u8> {
    let h = (cell_q as u64)
        .wrapping_mul(0x517cc1b727220a95)
        .wrapping_add(cell_r as u64)
        .wrapping_mul(0xff51afd7ed558ccd);
    // Use golden ratio offsets in HSV for perceptually distinct colors
    let hue = ((h >> 16) & 0xFFFF) as f64 / 65536.0;
    if is_continental {
        let sat = 0.5 + ((h >> 8) & 0xFF) as f64 / 512.0;
        let val = 0.5 + (h & 0xFF) as f64 / 512.0;
        hsv_to_rgb(hue, sat, val)
    } else {
        // Lighter/pastel tones for regional plates
        let sat = 0.3 + ((h >> 8) & 0xFF) as f64 / 1024.0;
        let val = 0.7 + (h & 0xFF) as f64 / 1024.0;
        hsv_to_rgb(hue, sat, val)
    }
}

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> Rgb<u8> {
    let h = h * 6.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    Rgb([
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    ])
}

/// Map boundary type to color with brightness from intensity.
fn boundary_type_color(eval: &TerrainEval) -> Rgb<u8> {
    // Interior: far from boundary → gray
    let max_dist = eval.continental_boundary.kind.max_distance(BoundaryScale::Continental);
    if eval.continental_boundary.distance >= max_dist {
        return Rgb([60, 60, 60]);
    }

    let proximity = 1.0 - (eval.continental_boundary.distance / max_dist);
    let brightness = (proximity * eval.continental_boundary.intensity * 255.0) as u8;

    match eval.continental_boundary.kind {
        BoundaryKind::Convergent => Rgb([brightness, brightness / 4, brightness / 4]),
        BoundaryKind::Divergent => Rgb([brightness / 4, brightness / 4, brightness]),
        BoundaryKind::Transform => Rgb([brightness, brightness, brightness / 4]),
    }
}

/// Continental (green) vs oceanic (blue).
fn plate_character_color(eval: &TerrainEval) -> Rgb<u8> {
    if eval.is_continental {
        Rgb([40, 140, 60])
    } else {
        Rgb([40, 60, 160])
    }
}

/// Slope: height difference to neighbors. White = steep, black = flat.
fn slope_color(q: i32, r: i32, terrain: &Terrain) -> Rgb<u8> {
    let h = terrain.get_height(q, r) as f64;
    // Check 6 hex neighbors (pointy-top axial directions)
    let neighbors = [
        (q + 1, r), (q - 1, r),
        (q, r + 1), (q, r - 1),
        (q + 1, r - 1), (q - 1, r + 1),
    ];
    let max_diff = neighbors.iter()
        .map(|&(nq, nr)| (terrain.get_height(nq, nr) as f64 - h).abs())
        .fold(0.0f64, f64::max);

    // Clamp to [0, 5] for visualization, map to brightness
    let brightness = ((max_diff / 5.0).min(1.0) * 255.0) as u8;
    Rgb([brightness, brightness, brightness])
}

fn main() {
    let cli = Cli::parse();

    let terrain = Terrain::new(cli.seed);
    let scale = cli.scale.max(1);

    // Calculate image dimensions using Cartesian projection
    let radius_f = cli.radius as f64;
    let scale_f = scale as f64;
    let (center_x, center_y) = hex_to_pixel(cli.center_q as f64, cli.center_r as f64);
    let pixel_diameter = (radius_f * 2.0 / scale_f) as u32;
    let width = pixel_diameter;
    let height = pixel_diameter;

    eprintln!(
        "Generating {} mode: center=({},{}) radius={} scale={} seed={} → {}x{} image",
        mode_name(cli.mode), cli.center_q, cli.center_r, cli.radius, scale, cli.seed, width, height
    );

    let start = Instant::now();

    // Generate pixel data in parallel (Terrain is Sync — safe to share across threads)
    let mode = cli.mode;
    let terrain_ref = &terrain;
    let pixels: Vec<(u32, u32, Rgb<u8>)> = (0..height)
        .into_par_iter()
        .flat_map(|py| {
            (0..width).map(move |px| {
                let cart_x = center_x - radius_f + (px as f64) * scale_f;
                let cart_y = center_y - radius_f + (py as f64) * scale_f;
                let (hq, hr) = pixel_to_hex(cart_x, cart_y);
                let q = hq.round() as i32;
                let r = hr.round() as i32;

                let color = match mode {
                    Mode::Slope => slope_color(q, r, terrain_ref),
                    _ => {
                        let eval = terrain_ref.evaluate(q, r);
                        match mode {
                            Mode::Elevation => elevation_color(eval.height),
                            Mode::Plates => {
                                if let Some(ref rp) = eval.regional_plate {
                                    plate_color(rp.cell_q, rp.cell_r, false)
                                } else {
                                    plate_color(eval.continental_plate.cell_q, eval.continental_plate.cell_r, true)
                                }
                            }
                            Mode::BoundaryType => boundary_type_color(&eval),
                            Mode::PlateCharacter => plate_character_color(&eval),
                            Mode::Slope => unreachable!(),
                        }
                    }
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

    // Build image
    let mut img = RgbImage::new(width, height);
    for (px, py, color) in pixels {
        img.put_pixel(px, py, color);
    }

    // Save
    img.save(&cli.output).expect("Failed to save PNG");
    let total = start.elapsed();
    eprintln!("Saved {} ({:.2}s total)", cli.output, total.as_secs_f64());
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Elevation => "elevation",
        Mode::Plates => "plates",
        Mode::BoundaryType => "boundary-type",
        Mode::PlateCharacter => "plate-character",
        Mode::Slope => "slope",
    }
}
