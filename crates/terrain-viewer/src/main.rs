use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use clap::{Parser, ValueEnum};
use rapid_qoi::{Colors, Qoi};
use rayon::prelude::*;
use terrain::{
    MicroplateCache, PlateCache, PlateCenter, MACRO_CELL_SIZE,
    REGIME_LAND_THRESHOLD,
};

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Qoi,
    Png,
}

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

    #[arg(long)]
    output: Option<String>,

    /// Output image format
    #[arg(long, value_enum, default_value_t = Format::Qoi)]
    format: Format,

    /// World units per pixel
    #[arg(long, default_value_t = 8.0)]
    scale: f64,

    #[arg(long, default_value_t = 0x9E3779B97F4A7C15)]
    seed: u64,
}

// ── Terrain coloring constants ──

/// Warp strength above this → coastal (regime transition zone).
/// Lowered from 500 because the pre-gradient sigmoid eliminates false
/// coastal signal — only real coastlines have nonzero warp now.
const COASTAL_WARP_THRESHOLD: f64 = 150.0;

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

// ── Classification types ──

/// Base regime of a plate (from regime value, ignoring coastal promotion).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Regime { Water, Land }

/// Display type after coastal promotion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DisplayType { Water, Land, Coastal }

fn hsv_for_display(display: DisplayType, shade: f64) -> (f64, f64, f64) {
    match display {
        DisplayType::Coastal => (0.09 + shade * 0.03, 0.40 + shade * 0.10, 0.60 + shade * 0.15),
        DisplayType::Water => (0.58 + shade * 0.05, 0.50 + shade * 0.20, 0.30 + shade * 0.25),
        DisplayType::Land => (0.28 + shade * 0.06, 0.35 + shade * 0.15, 0.30 + shade * 0.30),
    }
}

/// Two-pass classification: initial (gradient-based) + coastal suspender promotion.
///
/// Pass 1: classify each plate by warp strength (coastal if high gradient)
///         and regime value (water/land).
/// Pass 2: promote land plates bordering water to coastal
///         (the "suspender" — catches sharp transitions the gradient misses).
///         Only land→coastal; water plates are never promoted (beaches are land).
fn classify_plates(
    plates: &[PlateCenter],
    plate_cache: &mut PlateCache,
) -> HashMap<u64, (Regime, DisplayType)> {
    // Pass 1: initial classification
    let mut classification: HashMap<u64, (Regime, DisplayType)> = plates.iter().map(|p| {
        let strength = plate_cache.warp_strength_at(p.wx, p.wy);
        let regime = plate_cache.regime_value_at(p.wx, p.wy);
        let base = if regime < REGIME_LAND_THRESHOLD { Regime::Water } else { Regime::Land };
        let display = if strength > COASTAL_WARP_THRESHOLD {
            DisplayType::Coastal
        } else {
            match base { Regime::Water => DisplayType::Water, Regime::Land => DisplayType::Land }
        };
        (p.id, (base, display))
    }).collect();

    // Pass 2: coastal suspender promotion — land plates touching water only.
    // Beaches are land, not water. The sandy fringe sits on the land side.
    let mut promotions = Vec::new();
    for plate in plates {
        let &(base, display) = match classification.get(&plate.id) {
            Some(c) => c,
            None => continue,
        };
        if display == DisplayType::Coastal { continue; }
        if base != Regime::Land { continue; }
        let neighbors = plate_cache.plate_neighbors(plate.wx, plate.wy);
        let should_promote = neighbors.iter().any(|nbr| {
            classification.get(&nbr.id)
                .map_or(false, |&(nbr_base, _)| nbr_base == Regime::Water)
        });
        if should_promote {
            promotions.push(plate.id);
        }
    }

    for id in promotions {
        if let Some(entry) = classification.get_mut(&id) {
            entry.1 = DisplayType::Coastal;
        }
    }

    classification
}

fn main() {
    let cli = Cli::parse();

    let output = cli.output.unwrap_or_else(|| match cli.format {
        Format::Qoi => "terrain.qoi".to_string(),
        Format::Png => "terrain.png".to_string(),
    });

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

    let seed = cli.seed;
    let pixel_count = w * h;
    let mut lap = Instant::now();

    fn log_step(name: &str, count: usize, unit: &str, elapsed: std::time::Duration) {
        let secs = elapsed.as_secs_f64();
        let rate = count as f64 / secs;
        eprintln!("{name}: {count} {unit} in {secs:.2}s ({rate:.0} {unit}/s)");
    }

    // ── Classify macro plates (once per plate, not per pixel) ──

    let mut plate_cache = PlateCache::new(seed);
    let plates = plate_cache.plates_in_radius(
        cli.center_x, cli.center_y, cli.radius * std::f64::consts::SQRT_2 + MACRO_CELL_SIZE * 2.0,
    );
    let classification = classify_plates(&plates, &mut plate_cache);
    let plate_colors: HashMap<u64, (f64, f64, f64)> = plates.iter().map(|p| {
        let shade = id_to_shade(p.id);
        let &(_, display) = classification.get(&p.id).unwrap();
        (p.id, hsv_for_display(display, shade))
    }).collect();

    log_step("Classify", plate_colors.len(), "plates", lap.elapsed());
    lap = Instant::now();

    // ── Serial pre-pass: globally correct macro assignments over the viewport ──
    //
    // populate_region populates all chunks covering the viewport plus the full
    // ORPHAN_CORRECTION_MARGIN (15 000 wu), runs fix_orphans over the combined
    // region, then marks only the core (viewport) chunks corrected. Margin chunks
    // are left uncorrected so the chunk system remains the spatial authority.
    //
    // all_macro_ids() extracts the corrected micro→macro mapping. The parallel
    // pixel pass shares this map via Arc so each thread can look up the globally-
    // correct macro assignment without re-running fix_orphans per thread.

    let mut pre_cache = MicroplateCache::new(seed);
    pre_cache.populate_region(cli.center_x, cli.center_y, cli.radius, cli.radius);
    let corrected: Arc<HashMap<u64, u64>> = Arc::new(pre_cache.all_macro_ids());

    log_step("Pre-pass", corrected.len(), "micro cells", lap.elapsed());
    lap = Instant::now();

    // ── Pass 1: compute plate IDs per pixel (cached, parallel by row) ──
    // Each thread has its own MicroplateCache for micro-cell lookup.
    // Macro assignments are overridden from the globally-corrected map
    // produced by the serial pre-pass, eliminating orphan fragments.

    let id_grid: Vec<(u64, u64)> = (0..h)
        .into_par_iter()
        .map_init(
            || MicroplateCache::new(seed),
            |cache, py| {
                (0..w).map(|px| {
                    let wx = origin_x + (px as f64) * scale;
                    let wy = origin_y + (py as f64) * scale;
                    // micro_cell_at: cached micro lookup, no correction work.
                    // Macro assignment comes from the pre-corrected map; the
                    // thread-local cache is only here for micro-cell caching.
                    let micro = cache.micro_cell_at(wx, wy);
                    let macro_id = corrected.get(&micro.id).copied()
                        .unwrap_or_else(|| terrain::macro_plate_for(&micro, seed).id);
                    (macro_id, micro.id)
                }).collect::<Vec<_>>()
            },
        )
        .flatten()
        .collect();

    log_step("IDs", pixel_count, "px", lap.elapsed());
    lap = Instant::now();

    // ── Pass 2: flat plate coloring + micro offset + borders ──

    let border_radius = 3i32;
    let ids = &id_grid[..];
    let colors = &plate_colors;
    let default_hsv = (0.28, 0.40, 0.40); // fallback for edge plates

    let pixels: Vec<[u8; 3]> = (0..h)
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
                let base = [
                    (r * 255.0).min(255.0) as u8,
                    (g * 255.0).min(255.0) as u8,
                    (b * 255.0).min(255.0) as u8,
                ];

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
                    return [220, 220, 220];
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
                    return [
                        base[0].saturating_add(20),
                        base[1].saturating_add(20),
                        base[2].saturating_add(20),
                    ];
                }

                base
            }).collect::<Vec<_>>()
        })
        .collect();

    log_step("Color", pixel_count, "px", lap.elapsed());
    lap = Instant::now();

    let mut buf: Vec<u8> = pixels.into_flattened();

    // ── Draw markers directly into the buffer ──

    let set_pixel = |buf: &mut Vec<u8>, x: i32, y: i32, rgb: [u8; 3]| {
        if x >= 0 && x < w as i32 && y >= 0 && y < h as i32 {
            let off = (y as usize * w + x as usize) * 3;
            buf[off]     = rgb[0];
            buf[off + 1] = rgb[1];
            buf[off + 2] = rgb[2];
        }
    };

    // Microplate centers — yellow (centroids from id_grid)
    let mut micro_accum: HashMap<u64, (u64, u64, u64)> = HashMap::new();
    for py in 0..h {
        for px in 0..w {
            let (_, micro_id) = ids[py * w + px];
            let entry = micro_accum.entry(micro_id).or_insert((0, 0, 0));
            entry.0 += px as u64;
            entry.1 += py as u64;
            entry.2 += 1;
        }
    }
    let micro_dot = (2.0 / scale).max(1.0) as i32;
    for &(sx, sy, count) in micro_accum.values() {
        let cx = (sx / count) as i32;
        let cy = (sy / count) as i32;
        for dy in -micro_dot..=micro_dot {
            for dx in -micro_dot..=micro_dot {
                if dx * dx + dy * dy <= micro_dot * micro_dot {
                    set_pixel(&mut buf, cx + dx, cy + dy, [255, 220, 50]);
                }
            }
        }
    }

    // Macro plate centroids — red dots at post-correction plate centers.
    // Centroid = mean of corrected micro cell positions, not the hex lattice seed.
    let dot_radius = (4.0 / scale).max(2.0) as i32;
    let mut centroid_count = 0;
    for centroid in pre_cache.centroids() {
        centroid_count += 1;
        let cx = ((centroid.wx - origin_x) / scale) as i32;
        let cy = ((centroid.wy - origin_y) / scale) as i32;
        for dy in -dot_radius..=dot_radius {
            for dx in -dot_radius..=dot_radius {
                if dx * dx + dy * dy <= dot_radius * dot_radius {
                    set_pixel(&mut buf, cx + dx, cy + dy, [255, 50, 50]);
                }
            }
        }
    }

    let marker_count = micro_accum.len() + centroid_count;
    log_step("Markers", marker_count, "dots", lap.elapsed());
    lap = Instant::now();

    // ── Encode ──

    match cli.format {
        Format::Qoi => {
            let encoded = Qoi {
                width,
                height,
                colors: Colors::Rgb,
            }.encode_alloc(&buf).expect("QOI encode failed");
            std::fs::write(&output, &encoded).expect("Failed to write QOI");
        }
        Format::Png => {
            image::save_buffer(&output, &buf, width, height, image::ColorType::Rgb8)
                .expect("Failed to save PNG");
        }
    }
    log_step("Encode", buf.len(), "bytes", lap.elapsed());
    eprintln!("Saved {output}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coastal_suspenders_catch_all_land_borders() {
        // After promotion, no non-coastal LAND plate should border a water plate.
        // Water plates are never promoted (beaches are land-side only).
        let seed = 0x9E3779B97F4A7C15u64;
        let mut plate_cache = PlateCache::new(seed);
        let plates = plate_cache.plates_in_radius(0.0, 0.0, 20000.0);
        let classification = classify_plates(&plates, &mut plate_cache);

        for plate in &plates {
            let &(base, display) = classification.get(&plate.id).unwrap();
            if display == DisplayType::Coastal { continue; }
            if base != Regime::Land { continue; }
            let neighbors = plate_cache.plate_neighbors(plate.wx, plate.wy);
            for nbr in &neighbors {
                if let Some(&(nbr_base, _)) = classification.get(&nbr.id) {
                    assert_ne!(nbr_base, Regime::Water,
                        "Land plate {} borders water plate {} but wasn't promoted to coastal",
                        plate.id, nbr.id);
                }
            }
        }
    }

    #[test]
    fn promotion_preserves_base_regime() {
        // Base regime (water/land) must never change during promotion —
        // only display_type may change to Coastal.
        let seed = 0x9E3779B97F4A7C15u64;
        let mut plate_cache = PlateCache::new(seed);
        let plates = plate_cache.plates_in_radius(0.0, 0.0, 20000.0);

        // Snapshot base regime before promotion
        let initial: HashMap<u64, Regime> = plates.iter().map(|p| {
            let regime = plate_cache.regime_value_at(p.wx, p.wy);
            let base = if regime < REGIME_LAND_THRESHOLD { Regime::Water } else { Regime::Land };
            (p.id, base)
        }).collect();

        let classification = classify_plates(&plates, &mut plate_cache);

        for (id, (base, _)) in &classification {
            assert_eq!(*base, initial[id],
                "Plate {id} base regime changed during promotion");
        }
    }
}
