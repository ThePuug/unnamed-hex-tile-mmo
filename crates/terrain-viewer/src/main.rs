use std::collections::HashMap;
use std::time::Instant;

use clap::Parser;
use image::{Rgb, RgbImage};
use rayon::prelude::*;
use terrain::{
    MicroplateCache, MACRO_CELL_SIZE, regime_value_at, warp_strength_at,
};

#[derive(Parser)]
#[command(name = "terrain-viewer", about = "Render terrain Voronoi skeleton to PNG")]
struct Cli {
    /// Center x in world coordinates
    #[arg(long, default_value_t = 0.0)]
    center_x: f64,

    /// Center y in world coordinates
    #[arg(long, default_value_t = 0.0)]
    center_y: f64,

    /// Viewport radius in world units
    #[arg(long, default_value_t = 15000.0)]
    radius: f64,

    #[arg(long, default_value = "terrain.png")]
    output: String,

    /// World units per pixel
    #[arg(long, default_value_t = 8.0)]
    scale: f64,

    #[arg(long, default_value_t = 0x9E3779B97F4A7C15)]
    seed: u64,
}

// ── Terrain coloring constants ──

/// Regime value below this → water; above → land.
const MID_THRESHOLD: f64 = 0.45;

/// Warp strength above this → coastal (regime transition zone).
const COASTAL_WARP_THRESHOLD: f64 = 500.0;

/// Per-micro-cell saturation offset range (±).
const MICRO_SAT_RANGE: f64 = 0.15;

// ── Color helpers ──

/// Deterministic value in [0, 1) from a u64 ID using golden-ratio spacing.
fn id_to_shade(id: u64) -> f64 {
    let low = (id & 0xFFFF_FFFF) as f64;
    (low * 0.618033988749895) % 1.0
}

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (f64, f64, f64) {
    let s = s.clamp(0.0, 1.0);
    let v = v.clamp(0.0, 1.0);
    let c = v * s;
    let h6 = h * 6.0;
    let x = c * (1.0 - (h6 % 2.0 - 1.0).abs());
    let m = v - c;

    let (r, g, b) = if h6 < 1.0 {
        (c, x, 0.0)
    } else if h6 < 2.0 {
        (x, c, 0.0)
    } else if h6 < 3.0 {
        (0.0, c, x)
    } else if h6 < 4.0 {
        (0.0, x, c)
    } else if h6 < 5.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    (r + m, g + m, b + m)
}

/// Classify a macro plate by sampling warp strength and regime at its center.
/// Returns the plate's flat HSV color.
fn classify_plate(wx: f64, wy: f64, plate_id: u64, seed: u64) -> (f64, f64, f64) {
    let strength = warp_strength_at(wx, wy, seed);
    let regime = regime_value_at(wx, wy, seed);
    let shade = id_to_shade(plate_id);

    if strength > COASTAL_WARP_THRESHOLD {
        // Coastal — warm sand tones (high gradient = regime transition zone)
        (0.09 + shade * 0.03, 0.40 + shade * 0.10, 0.60 + shade * 0.15)
    } else if regime < MID_THRESHOLD {
        // Water — blue shades
        (0.58 + shade * 0.05, 0.50 + shade * 0.20, 0.30 + shade * 0.25)
    } else {
        // Land — green shades
        (0.28 + shade * 0.06, 0.35 + shade * 0.15, 0.30 + shade * 0.30)
    }
}

fn main() {
    let cli = Cli::parse();

    let scale = cli.scale.max(0.5);
    let origin_x = cli.center_x - cli.radius;
    let origin_y = cli.center_y - cli.radius;
    let diameter = cli.radius * 2.0;
    let width = (diameter / scale) as u32;
    let height = width;
    let w = width as usize;
    let h = height as usize;

    eprintln!(
        "Generating terrain: center=({},{}) radius={} scale={} seed={} -> {}x{} image",
        cli.center_x, cli.center_y, cli.radius, scale, cli.seed, width, height
    );

    let start = Instant::now();
    let seed = cli.seed;

    // ── Classify macro plates (once per plate, not per pixel) ──

    let plates = terrain::macro_plates_in_radius(
        cli.center_x, cli.center_y, cli.radius + MACRO_CELL_SIZE * 2.0, seed
    );
    let plate_colors: HashMap<u64, (f64, f64, f64)> = plates.iter().map(|p| {
        (p.id, classify_plate(p.wx, p.wy, p.id, seed))
    }).collect();

    eprintln!("Classified {} plates", plate_colors.len());

    // ── Pass 1: compute plate IDs per pixel (cached, parallel by row) ──

    let id_grid: Vec<(u64, u64)> = (0..h)
        .into_par_iter()
        .flat_map(|py| {
            let mut cache = MicroplateCache::new(seed);
            (0..w).map(move |px| {
                let wx = origin_x + (px as f64) * scale;
                let wy = origin_y + (py as f64) * scale;
                let (macro_plate, micro) = cache.plate_info_at(wx, wy);
                (macro_plate.id, micro.id)
            }).collect::<Vec<_>>()
        })
        .collect();

    let elapsed_ids = start.elapsed();
    let pixel_count = w * h;
    eprintln!("Pass 1 (IDs): {} pixels in {:.2}s ({:.0} px/s)",
        pixel_count,
        elapsed_ids.as_secs_f64(),
        pixel_count as f64 / elapsed_ids.as_secs_f64()
    );

    // ── Pass 2: flat plate coloring + micro offset + borders ──

    let border_radius = 3i32;
    let ids = &id_grid[..];
    let colors = &plate_colors;
    let default_hsv = (0.28, 0.40, 0.40); // fallback for edge plates

    let pixels: Vec<Rgb<u8>> = (0..h)
        .into_par_iter()
        .flat_map(|py| {
            (0..w).map(move |px| {
                let idx = py * w + px;
                let (macro_id, micro_id) = ids[idx];

                // Flat plate color + micro saturation offset
                let &(hue, sat, v) = colors.get(&macro_id).unwrap_or(&default_hsv);
                let micro_shade = id_to_shade(micro_id);
                let sat_offset = (micro_shade * 2.0 - 1.0) * MICRO_SAT_RANGE;
                let sat = (sat + sat_offset).clamp(0.0, 1.0);
                let (r, g, b) = hsv_to_rgb(hue, sat, v);
                let base = Rgb([
                    (r * 255.0).min(255.0) as u8,
                    (g * 255.0).min(255.0) as u8,
                    (b * 255.0).min(255.0) as u8,
                ]);

                // Check for macro border
                let mut is_macro_border = false;
                'macro_check: for dy in -border_radius..=border_radius {
                    for dx in -border_radius..=border_radius {
                        if dx == 0 && dy == 0 { continue; }
                        if dx * dx + dy * dy > border_radius * border_radius { continue; }
                        let nx = px as i32 + dx;
                        let ny = py as i32 + dy;
                        if nx >= 0 && nx < w as i32 && ny >= 0 && ny < h as i32 {
                            let nidx = ny as usize * w + nx as usize;
                            if ids[nidx].0 != macro_id {
                                is_macro_border = true;
                                break 'macro_check;
                            }
                        }
                    }
                }

                if is_macro_border {
                    return Rgb([220, 220, 220]);
                }

                // Check for micro border
                let is_micro_border = [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)].iter().any(|&(dx, dy)| {
                    let nx = px as i32 + dx;
                    let ny = py as i32 + dy;
                    if nx >= 0 && nx < w as i32 && ny >= 0 && ny < h as i32 {
                        let nidx = ny as usize * w + nx as usize;
                        ids[nidx].1 != micro_id
                    } else {
                        false
                    }
                });

                if is_micro_border {
                    let Rgb([r, g, b]) = base;
                    return Rgb([
                        r.saturating_add(20),
                        g.saturating_add(20),
                        b.saturating_add(20),
                    ]);
                }

                base
            }).collect::<Vec<_>>()
        })
        .collect();

    let elapsed_eval = start.elapsed();
    eprintln!("Pass 2 (coloring): {:.2}s",
        (elapsed_eval - elapsed_ids).as_secs_f64()
    );
    eprintln!("Total eval: {} pixels in {:.2}s ({:.0} px/s)",
        pixel_count,
        elapsed_eval.as_secs_f64(),
        pixel_count as f64 / elapsed_eval.as_secs_f64()
    );

    // ── Assemble image ──

    let mut img = RgbImage::new(width, height);
    for (i, color) in pixels.into_iter().enumerate() {
        let px = (i % w) as u32;
        let py = (i / w) as u32;
        img.put_pixel(px, py, color);
    }

    // Draw macro plate centers as red markers
    let dot_radius = (4.0 / scale).max(2.0) as i32;
    for plate in &plates {
        let px = ((plate.wx - origin_x) / scale) as i32;
        let py = ((plate.wy - origin_y) / scale) as i32;
        for dx in -dot_radius..=dot_radius {
            for dy in -dot_radius..=dot_radius {
                if dx * dx + dy * dy <= dot_radius * dot_radius {
                    let x = px + dx;
                    let y = py + dy;
                    if x >= 0 && x < width as i32 && y >= 0 && y < height as i32 {
                        img.put_pixel(x as u32, y as u32, Rgb([255, 50, 50]));
                    }
                }
            }
        }
    }

    // Draw microplate centers as smaller yellow dots
    let micro_dot = (2.0 / scale).max(1.0) as i32;
    for plate in &plates {
        let children = terrain::micro_cells_for_macro(plate, seed);
        for child in &children {
            let px = ((child.wx - origin_x) / scale) as i32;
            let py = ((child.wy - origin_y) / scale) as i32;
            for dx in -micro_dot..=micro_dot {
                for dy in -micro_dot..=micro_dot {
                    if dx * dx + dy * dy <= micro_dot * micro_dot {
                        let x = px + dx;
                        let y = py + dy;
                        if x >= 0 && x < width as i32 && y >= 0 && y < height as i32 {
                            img.put_pixel(x as u32, y as u32, Rgb([255, 220, 50]));
                        }
                    }
                }
            }
        }
    }

    img.save(&cli.output).expect("Failed to save PNG");
    let total = start.elapsed();
    eprintln!("Saved {} ({:.2}s total)", cli.output, total.as_secs_f64());
}
