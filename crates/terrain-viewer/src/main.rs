use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use clap::Parser;
use rapid_qoi::{Colors, Qoi};
use rayon::prelude::*;
use terrain::{
    MicroCellGeometry, PlateCentroid, PlateCenter,
    PlateTag, Tagged, SpineInstance, RavineProbe,
    RIDGE_PEAK_ELEVATION, REGIME_LAND_THRESHOLD,
    regime_value_at,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Format {
    Qoi,
    Png,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Layer {
    /// Base Sea/Coast/Inland tag colors. Fills every pixel — put this first.
    Plates,
    /// Dot markers at micro and macro plate centers.
    Centroids,
    /// Categorical spine tag colors (Ridge/Highland/Foothills) overdraw where tags exist.
    Spines,
    /// Elevation blend: tints the underlying color based on plate elevation.
    Elevation,
    /// Raw regime value as grayscale (0=black, 1=white) with red contour at land threshold.
    /// Fills every pixel. When combined with other layers, use this first — plates overdraw on top.
    Regime,
    /// Ravine debug: valley floor (blue), valley wall (red), ridge path (yellow).
    /// Transparent where no stream influence — shows layer below.
    Ravines,
}

fn parse_format(s: &str) -> Format {
    match s {
        "qoi" => Format::Qoi,
        "png" => Format::Png,
        other => {
            eprintln!("Unknown format: {other:?}. Valid: qoi, png");
            std::process::exit(1);
        }
    }
}

fn parse_layers(s: &str) -> Vec<Layer> {
    s.split(',').map(|name| match name.trim() {
        "plates"    => Layer::Plates,
        "centroids" => Layer::Centroids,
        "spines"    => Layer::Spines,
        "elevation" => Layer::Elevation,
        "regime"    => Layer::Regime,
        "ravines"   => Layer::Ravines,
        other => {
            eprintln!("Unknown layer: {other:?}. Valid: plates, centroids, spines, elevation, regime, ravines");
            std::process::exit(1);
        }
    }).collect()
}

#[derive(Parser)]
#[command(name = "terrain-viewer", about = "Render terrain Voronoi skeleton to image")]
struct Cli {
    /// Center x in world coordinates
    #[arg(long, default_value_t = 0.0, allow_hyphen_values = true)]
    center_x: f64,

    /// Center y in world coordinates
    #[arg(long, default_value_t = 0.0, allow_hyphen_values = true)]
    center_y: f64,

    /// Viewport radius in world units
    #[arg(long, default_value_t = 15000.0)]
    radius: f64,

    #[arg(long)]
    output: Option<String>,

    /// Output image format: qoi or png
    #[arg(long, default_value = "qoi")]
    format: String,

    /// World units per pixel
    #[arg(long, default_value_t = 8.0)]
    scale: f64,

    #[arg(long, default_value_t = 0x9E3779B97F4A7C15)]
    seed: u64,

    /// Comma-separated layer stack drawn bottom to top.
    /// Available: plates, centroids, spines, elevation.
    /// Example: --layers plates,elevation,spines
    #[arg(long, default_value = "plates,elevation")]
    layers: String,
}

// ── Terrain coloring constants ──

/// Half-band around REGIME_LAND_THRESHOLD rendered as a red contour in the regime layer.
/// Pixels with |regime - threshold| < this value are colored red.
const REGIME_CONTOUR_HALF_BAND: f64 = 0.015;

// ── Color helpers ──

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

/// Base HSV color from Sea/Coast/Inland tag.
fn hsv_for_tag(plate: &PlateCenter, shade: f64) -> (f64, f64, f64) {
    if plate.has_tag(&PlateTag::Coast) {
        (0.105 + shade * 0.02, 0.33 + shade * 0.10, 0.82 + shade * 0.08)
    } else if plate.has_tag(&PlateTag::Inland) {
        (0.28 + shade * 0.06, 0.35 + shade * 0.15, 0.35 + shade * 0.25)
    } else {
        (0.60 + shade * 0.04, 0.55 + shade * 0.20, 0.30 + shade * 0.20)
    }
}

/// Blend two RGB tuples by `t` (0 = a, 1 = b).
fn lerp_rgb(a: (f64, f64, f64), b: (f64, f64, f64), t: f64) -> (f64, f64, f64) {
    let t = t.clamp(0.0, 1.0);
    (
        a.0 + (b.0 - a.0) * t,
        a.1 + (b.1 - a.1) * t,
        a.2 + (b.2 - a.2) * t,
    )
}

/// Spine elevation overlay with slope gradient.
/// `elev` is terraced elevation in world units.
/// `slope` is max elevation difference per world unit to a neighbor pixel.
fn elevation_overlay(base: (f64, f64, f64), elev: f64, slope: f64) -> (f64, f64, f64) {
    // Global brightness from elevation
    let norm = (elev / RIDGE_PEAK_ELEVATION).clamp(0.0, 1.0);
    let height_color = lerp_rgb(base, (1.0, 1.0, 1.0), norm);

    // Slope coloring: flat → height_color, steep → dark rock brown
    // slope of ~10+ z/wu is a serious cliff face
    let cliff_t = (slope / 10.0).clamp(0.0, 1.0);
    let cliff_color = (0.25, 0.20, 0.15); // dark rock
    lerp_rgb(height_color, cliff_color, cliff_t)
}

/// Spine tag categorical color. Returns `None` if the plate has no spine tag,
/// allowing the caller to show through the underlying layer.
fn spine_rgb(plate: &PlateCenter, shade: f64) -> Option<(f64, f64, f64)> {
    if plate.has_tag(&PlateTag::Ridge) {
        let v = 0.75 + shade * 0.25;
        Some((v, v, v))
    } else if plate.has_tag(&PlateTag::Highland) {
        Some(hsv_to_rgb(0.07, 0.55 + shade * 0.10, 0.55 + shade * 0.10))
    } else if plate.has_tag(&PlateTag::Foothills) {
        Some(hsv_to_rgb(0.09, 0.30 + shade * 0.10, 0.78 + shade * 0.10))
    } else {
        None
    }
}

/// Compute the final RGB color for a plate by compositing the given layer stack.
/// Layers are applied in order: each layer reads the current `color` and optionally
/// replaces or blends it. `Centroids` is a no-op here (handled post-pixel).
fn compute_plate_color(plate: &PlateCenter, shade: f64, layers: &[Layer]) -> (f64, f64, f64) {
    let mut color = (0.0, 0.0, 0.0);
    for &layer in layers {
        match layer {
            Layer::Plates => {
                let (h, s, v) = hsv_for_tag(plate, shade);
                color = hsv_to_rgb(h, s, v);
            }
            Layer::Elevation => {} // per-pixel via transform functions, not plate-level
            Layer::Spines => {
                if let Some(sc) = spine_rgb(plate, shade) {
                    color = sc;
                }
                // No spine tag → show through unchanged.
            }
            Layer::Centroids => {} // dot markers drawn into pixel buffer after coloring
            Layer::Regime    => {} // rendered per-pixel in pass 2, not via plate lookup
            Layer::Ravines   => {} // rendered per-pixel overlay, not via plate lookup
        }
    }
    color
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug"))
        .init();
    let cli = Cli::parse();
    let format = parse_format(&cli.format);
    let layers = parse_layers(&cli.layers);

    let output = cli.output.unwrap_or_else(|| match format {
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

    let layer_names: Vec<&str> = layers.iter().map(|l| match l {
        Layer::Plates    => "plates",
        Layer::Centroids => "centroids",
        Layer::Spines    => "spines",
        Layer::Elevation => "elevation",
        Layer::Regime    => "regime",
        Layer::Ravines   => "ravines",
    }).collect();

    log::info!(
        "Generating terrain: center=({},{}) radius={} scale={} seed={} layers=[{}] -> {}x{} image",
        cli.center_x, cli.center_y, cli.radius, scale, cli.seed,
        layer_names.join(","),
        width, height
    );

    let seed = cli.seed;
    let pixel_count = w * h;
    let mut lap = Instant::now();

    fn log_step(name: &str, count: usize, unit: &str, elapsed: std::time::Duration) {
        let secs = elapsed.as_secs_f64();
        let rate = count as f64 / secs;
        log::info!("{name}: {count} {unit} in {secs:.2}s ({rate:.0} {unit}/s)");
    }

    // Determine which pipeline(s) are needed.
    let needs_plates_pipeline = layers.iter().any(|l| matches!(
        l, Layer::Plates | Layer::Elevation | Layer::Spines | Layer::Centroids | Layer::Ravines
    ));

    // ── Plate-dependent pipeline (classify, micro-pass, ID grid, coloring) ──

    struct PlateData {
        id_grid: Vec<(u64, u64)>,
        plate_colors: HashMap<u64, (f64, f64, f64)>,
        plate_centroids: Vec<PlateCentroid>,
        spine_instances: Arc<Vec<SpineInstance>>,
    }

    let plate_data: Option<PlateData> = if needs_plates_pipeline {
        let needs_spines = layers.contains(&Layer::Spines) || layers.contains(&Layer::Elevation) || layers.contains(&Layer::Ravines);
        let region = terrain::generate_region(seed, cli.center_x, cli.center_y, cli.radius, needs_spines);

        for m in &region.metrics {
            let secs = m.duration.as_secs_f64();
            let rate = m.count as f64 / secs;
            log::info!("{}: {} {} in {secs:.2}s ({rate:.0} {}/s)", m.label, m.count, m.unit, m.unit);
        }

        let plate_colors: HashMap<u64, (f64, f64, f64)> = region.plates.iter().map(|p| {
            (p.id, compute_plate_color(p, 0.5, &layers))
        }).collect();

        let corrected: Arc<HashMap<u64, u64>> = Arc::new(region.macro_ids);
        let shared_geometry: Arc<MicroCellGeometry> = Arc::new(region.geometry);

        // ── Pass 1: compute plate IDs per pixel (parallel by row) ──

        lap = Instant::now();

        let id_grid: Vec<(u64, u64)> = (0..h)
            .into_par_iter()
            .map_init(
                || Arc::clone(&shared_geometry),
                |geom, py| {
                    (0..w).map(|px| {
                        let wx = origin_x + (px as f64) * scale;
                        let wy = origin_y + (py as f64) * scale;
                        let micro = geom.lookup(wx, wy);
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

        Some(PlateData {
            id_grid,
            plate_colors,
            plate_centroids: region.centroids,
            spine_instances: Arc::new(region.spine_instances),
        })
    } else {
        None
    };

    // ── Pass 2: per-pixel coloring ──

    let pixels: Vec<[u8; 3]> = if let Some(ref pd) = plate_data {
        // Plate-based coloring + optional regime base

        // Scale macro border with zoom: ~3 px at scale ≤ 10, thins to 1 px at high scale.
        let border_radius = ((30.0 / scale) as i32).clamp(1, 3);
        let ids = &pd.id_grid[..];
        let colors = &pd.plate_colors;
        let default_color = (0.28f64, 0.40f64, 0.40f64);
        let has_regime_base = layers.first() == Some(&Layer::Regime);
        let has_elevation = layers.contains(&Layer::Elevation);
        let has_ravines = layers.contains(&Layer::Ravines);
        let spines = &pd.spine_instances;

        // Pre-compute elevation grid for slope gradient visualization.
        let elev_grid: Vec<f64> = if has_elevation {
            (0..h)
                .into_par_iter()
                .flat_map(|py| {
                    (0..w).map(move |px| {
                        let wx = origin_x + (px as f64) * scale;
                        let wy = origin_y + (py as f64) * scale;
                        let mut max_elev = 0.0f64;
                        for inst in spines.iter() {
                            let e = inst.elevation_at(wx, wy);
                            if e > max_elev { max_elev = e; }
                        }
                        max_elev
                    })
                    .collect::<Vec<_>>()
                })
                .collect()
        } else {
            Vec::new()
        };

        let elev_ref = &elev_grid;

        (0..h)
            .into_par_iter()
            .flat_map(|py| {
                (0..w).map(move |px| {
                    let idx = py * w + px;
                    let (macro_id, micro_id) = ids[idx];

                    let &(r_base, g_base, b_base) = colors.get(&macro_id).unwrap_or(&default_color);

                    // Macro plate border
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

                    // Per-pixel elevation + slope gradient from precomputed grid.
                    let color = if has_elevation {
                        let idx = py * w + px;
                        let elev = elev_ref[idx];
                        if elev > 0.0 {
                            // Slope: max elevation difference to 4-neighbors
                            let mut max_diff = 0.0f64;
                            for &(dx, dy) in &[(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                                let nx = px as i32 + dx;
                                let ny = py as i32 + dy;
                                if nx >= 0 && nx < w as i32 && ny >= 0 && ny < h as i32 {
                                    let nidx = ny as usize * w + nx as usize;
                                    let diff = (elev - elev_ref[nidx]).abs();
                                    if diff > max_diff { max_diff = diff; }
                                }
                            }
                            // Normalize slope: diff per pixel in world units.
                            // At scale=1, 1px = 1wu. Steep cliff ≈ 75 z-levels
                            // over a few pixels.
                            let slope = max_diff / scale;
                            elevation_overlay((r_base, g_base, b_base), elev, slope)
                        } else {
                            (r_base, g_base, b_base)
                        }
                    } else {
                        (r_base, g_base, b_base)
                    };

                    // Ravine overlay: color by stream relationship
                    let color = if has_ravines {
                        let wx = origin_x + (px as f64) * scale;
                        let wy = origin_y + (py as f64) * scale;
                        let mut probe_result = None;
                        for inst in spines.iter() {
                            if let Some(p) = inst.ravine_probe(wx, wy) {
                                probe_result = Some(p);
                                break;
                            }
                        }
                        match probe_result {
                            Some(RavineProbe::Floor)   => (0.2, 0.3, 0.9),  // blue
                            Some(RavineProbe::Wall(_)) => (0.9, 0.2, 0.2),  // red
                            Some(RavineProbe::Path)    => (0.9, 0.9, 0.2),  // yellow
                            None => color,
                        }
                    } else {
                        color
                    };

                    let base = [
                        (color.0 * 255.0).min(255.0) as u8,
                        (color.1 * 255.0).min(255.0) as u8,
                        (color.2 * 255.0).min(255.0) as u8,
                    ];

                    // Micro plate border
                    let is_micro_border = [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)].iter().any(|&(dx, dy)| {
                        let nx = px as i32 + dx;
                        let ny = py as i32 + dy;
                        if nx >= 0 && nx < w as i32 && ny >= 0 && ny < h as i32 {
                            ids[ny as usize * w + nx as usize].1 != micro_id
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

                    // Regime base layer — unused when plates overdraw, but warn-free
                    let _ = has_regime_base;

                    base
                }).collect::<Vec<_>>()
            })
            .collect()
    } else {
        // ── Regime-only path: grayscale + threshold contour, no plate pipeline ──
        (0..h)
            .into_par_iter()
            .flat_map(|py| {
                (0..w).map(move |px| {
                    let wx = origin_x + (px as f64) * scale;
                    let wy = origin_y + (py as f64) * scale;
                    let v = regime_value_at(wx, wy, seed);
                    let dist = (v - REGIME_LAND_THRESHOLD).abs();
                    if dist < REGIME_CONTOUR_HALF_BAND {
                        [255u8, 0, 0]
                    } else {
                        let grey = (v * 255.0).min(255.0) as u8;
                        [grey, grey, grey]
                    }
                }).collect::<Vec<_>>()
            })
            .collect()
    };

    log_step("Color", pixel_count, "px", lap.elapsed());
    lap = Instant::now();

    let mut buf: Vec<u8> = pixels.into_flattened();

    // ── Centroid layer: dot markers into pixel buffer ──

    if layers.contains(&Layer::Centroids) {
        if let Some(ref pd) = plate_data {
            let set_pixel = |buf: &mut Vec<u8>, x: i32, y: i32, rgb: [u8; 3]| {
                if x >= 0 && x < w as i32 && y >= 0 && y < h as i32 {
                    let off = (y as usize * w + x as usize) * 3;
                    buf[off]     = rgb[0];
                    buf[off + 1] = rgb[1];
                    buf[off + 2] = rgb[2];
                }
            };

            // Microplate centers — yellow
            let mut micro_accum: HashMap<u64, (u64, u64, u64)> = HashMap::new();
            for py in 0..h {
                for px in 0..w {
                    let (_, micro_id) = pd.id_grid[py * w + px];
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

            // Macro plate centroids — red
            let dot_radius = (4.0 / scale).max(2.0) as i32;
            let mut centroid_count = 0;
            for centroid in &pd.plate_centroids {
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
            log_step("Centroids", marker_count, "dots", lap.elapsed());
            lap = Instant::now();
        }
    }

    // ── Encode ──

    match format {
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
    log::info!("Saved {output}");
}
