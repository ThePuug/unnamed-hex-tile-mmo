//! World Viewer — renders the world event composite to image.
//!
//! Instantiates the same Composite + event stack the server uses.
//! Same seed = same output.

use std::collections::HashMap;
use std::time::Instant;

use clap::Parser;
use rapid_qoi::{Colors, Qoi};
use rayon::prelude::*;

use common::{PlateTag, TagSet};
use world::events::Composite;
use world::events::plates::{PlateEvent, PlateCentroidIndex};
use world::events::spawner::{
    SpawnerArchetype, SpawnerEvent, SpawnerPlacementIndex, archetype_for_tagset,
};
use world::events::spines::{SpineEvent, SpineInstanceIndex};
use world::RIDGE_PEAK_ELEVATION;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Layer {
    /// Base Sea/Coast/Inland tag colors.
    Plates,
    /// Height tinting with slope shading.
    Elevation,
    /// Ridge/Highland/Foothills coloring from spine tags.
    Spines,
    /// Macro plate centroid markers (red dots).
    Centroids,
    /// Spawner placement markers, colored by archetype.
    Spawners,
    /// Spine epicenter markers (yellow dots).
    SpinePeaks,
}

fn parse_layers(s: &str) -> Vec<Layer> {
    s.split(',')
        .map(|name| match name.trim() {
            "plates" => Layer::Plates,
            "elevation" => Layer::Elevation,
            "spines" => Layer::Spines,
            "centroids" => Layer::Centroids,
            "spawners" => Layer::Spawners,
            "spine-peaks" => Layer::SpinePeaks,
            other => {
                eprintln!(
                    "Unknown layer: {other:?}. Valid: plates, elevation, spines, \
                     centroids, spawners, spine-peaks"
                );
                std::process::exit(1);
            }
        })
        .collect()
}

#[derive(Parser)]
#[command(name = "world-viewer", about = "Render world event composite to image")]
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
    /// Available: plates, elevation, spines, centroids, spawners, spine-peaks
    #[arg(long, default_value = "plates,elevation")]
    layers: String,
}

// ── Color helpers ──

fn lerp_rgb(a: (f64, f64, f64), b: (f64, f64, f64), t: f64) -> (f64, f64, f64) {
    let t = t.clamp(0.0, 1.0);
    (
        a.0 + (b.0 - a.0) * t,
        a.1 + (b.1 - a.1) * t,
        a.2 + (b.2 - a.2) * t,
    )
}

fn plate_color(tags: &TagSet) -> (f64, f64, f64) {
    if tags.has(PlateTag::Inland) {
        (0.30, 0.50, 0.30) // green
    } else if tags.has(PlateTag::Coast) {
        (0.70, 0.65, 0.50) // sandy
    } else {
        (0.20, 0.25, 0.45) // deep blue
    }
}

fn spine_color(tags: &TagSet) -> Option<(f64, f64, f64)> {
    if tags.has(PlateTag::Ridge) {
        Some((0.75, 0.75, 0.75))
    } else if tags.has(PlateTag::Highland) {
        Some((0.55, 0.40, 0.30))
    } else if tags.has(PlateTag::Foothills) {
        Some((0.65, 0.55, 0.40))
    } else {
        None
    }
}

fn elevation_overlay(base: (f64, f64, f64), elev: f64, slope: f64) -> (f64, f64, f64) {
    let norm = (elev / RIDGE_PEAK_ELEVATION).clamp(0.0, 1.0);
    // Brighten toward white at peaks, darken slightly at low elevation
    let height_color = lerp_rgb(base, (0.95, 0.95, 0.90), norm * 0.7);
    // Slope shading: scale threshold to elevation range (steep = 5% of peak per tile)
    let cliff_t = (slope / (RIDGE_PEAK_ELEVATION * 0.05)).clamp(0.0, 0.6);
    lerp_rgb(height_color, (0.30, 0.25, 0.20), cliff_t)
}

fn spawner_marker_color(arch: SpawnerArchetype) -> [u8; 3] {
    match arch {
        SpawnerArchetype::Berserker => [255, 50, 50],  // red
        SpawnerArchetype::Juggernaut => [50, 50, 255], // blue
        SpawnerArchetype::Kiter => [50, 255, 50],      // green
        SpawnerArchetype::Defender => [255, 165, 50],   // orange
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let cli = Cli::parse();
    let layers = parse_layers(&cli.layers);

    let scale = cli.scale.max(0.5);
    let origin_x = cli.center_x - cli.radius;
    let origin_y = cli.center_y - cli.radius;
    let diameter = cli.radius * 2.0;
    let width = (diameter / scale) as u32;
    let height = width;
    let w = width as usize;
    let h = height as usize;

    let layer_names: Vec<&str> = layers
        .iter()
        .map(|l| match l {
            Layer::Plates => "plates",
            Layer::Elevation => "elevation",
            Layer::Spines => "spines",
            Layer::Centroids => "centroids",
            Layer::Spawners => "spawners",
            Layer::SpinePeaks => "spine-peaks",
        })
        .collect();

    log::info!(
        "world-viewer: center=({},{}) radius={} scale={} seed={:#x} layers=[{}] -> {}x{}",
        cli.center_x,
        cli.center_y,
        cli.radius,
        scale,
        cli.seed,
        layer_names.join(","),
        width,
        height
    );

    // Only add events needed for the requested layers
    let needs_spines = layers.iter().any(|l| {
        matches!(l, Layer::Elevation | Layer::Spines | Layer::SpinePeaks)
    });
    let needs_spawners = layers.contains(&Layer::Spawners);

    let plate_cache = std::sync::Arc::new(world::PlateCache::new(cli.seed));
    let mut composite = Composite::new(cli.seed);
    composite.add_event(Box::new(PlateEvent::with_cache(plate_cache.clone())));
    if needs_spines || needs_spawners {
        composite.add_event(Box::new(SpineEvent::with_cache(plate_cache, cli.seed)));
    }
    if needs_spawners {
        composite.add_event(Box::new(SpawnerEvent::new(cli.seed)));
    }

    // ── Phase 1: Materialize unique hex tiles visible in the pixel grid ──
    //
    // Each pixel maps to a world coordinate → nearest hex tile. At scale=8,
    // ~8 pixels share one hex tile, so the unique tile count ≈ w*h/64.
    // We collect unique tiles by scanning pixel positions, not by iterating
    // the full hex bounding box (which would be billions of tiles at large radii).

    let lap = Instant::now();
    let mut tile_set: std::collections::HashSet<(i32, i32)> = std::collections::HashSet::new();
    for py in 0..h {
        for px in 0..w {
            let wx = origin_x + (px as f64) * scale;
            let wy = origin_y + (py as f64) * scale;
            tile_set.insert(world::world_to_hex(wx, wy));
        }
    }

    let coords: Vec<(i32, i32)> = tile_set.into_iter().collect();
    log::info!("Materializing {} unique tiles (from {}x{} pixels)...", coords.len(), w, h);

    let views = composite.tiles_at(&coords);
    let tile_cache: HashMap<(i32, i32), (TagSet, f64)> = views
        .into_iter()
        .map(|((q, r), v)| ((q, r), (v.tags, v.elevation)))
        .collect();

    let tile_secs = lap.elapsed().as_secs_f64();
    log::info!(
        "Tiles: {} in {tile_secs:.2}s ({:.0} tiles/s)",
        coords.len(),
        coords.len() as f64 / tile_secs
    );

    // ── Phase 2: Read indexes for marker layers ──

    let centroids: Vec<(f64, f64)> = if layers.contains(&Layer::Centroids) {
        composite.with_indexes(|indexes| {
            indexes
                .get::<PlateCentroidIndex>()
                .map(|idx| {
                    idx.cells
                        .values()
                        .flat_map(|v| v.iter())
                        .map(|e| (e.wx, e.wy))
                        .collect()
                })
                .unwrap_or_default()
        })
    } else {
        vec![]
    };

    let spawner_markers: Vec<(f64, f64, SpawnerArchetype)> = if layers.contains(&Layer::Spawners) {
        composite.with_indexes(|indexes| {
            indexes
                .get::<SpawnerPlacementIndex>()
                .map(|idx| {
                    idx.cells
                        .values()
                        .flat_map(|v| v.iter())
                        .map(|p| {
                            // Resolve real archetype from composite tags
                            let arch = tile_cache
                                .get(&(p.q, p.r))
                                .and_then(|(t, _)| archetype_for_tagset(t))
                                .unwrap_or(p.archetype);
                            let (wx, wy) = world::hex_to_world(p.q, p.r);
                            (wx, wy, arch)
                        })
                        .collect()
                })
                .unwrap_or_default()
        })
    } else {
        vec![]
    };

    let spine_epicenters: Vec<(f64, f64)> = if layers.contains(&Layer::SpinePeaks) {
        composite.with_indexes(|indexes| {
            indexes
                .get::<SpineInstanceIndex>()
                .map(|idx| {
                    idx.cells
                        .values()
                        .flat_map(|v| v.iter())
                        .map(|inst| inst.bounding_center)
                        .collect()
                })
                .unwrap_or_default()
        })
    } else {
        vec![]
    };

    // ── Phase 3: Render pixels (parallel by row) ──

    let lap = Instant::now();
    let tc = &tile_cache;
    let layer_slice = &layers;
    let pixels: Vec<[u8; 3]> = (0..h)
        .into_par_iter()
        .flat_map(|py| {
            (0..w)
                .map(move |px| {
                    let wx = origin_x + (px as f64) * scale;
                    let wy = origin_y + (py as f64) * scale;
                    let (q, r) = world::world_to_hex(wx, wy);

                    let (tags, elevation) = tc
                        .get(&(q, r))
                        .copied()
                        .unwrap_or((TagSet::new(), 0.0));

                    let mut color = (0.0f64, 0.0, 0.0);

                    for &layer in layer_slice {
                        match layer {
                            Layer::Plates => {
                                color = plate_color(&tags);
                            }
                            Layer::Elevation => {
                                if elevation > 0.0 {
                                    // Slope from 6 hex neighbors
                                    let max_diff = [
                                        (1, 0),
                                        (-1, 0),
                                        (0, 1),
                                        (0, -1),
                                        (1, -1),
                                        (-1, 1),
                                    ]
                                    .iter()
                                    .map(|&(dq, dr)| {
                                        let ne =
                                            tc.get(&(q + dq, r + dr)).map_or(0.0, |v| v.1);
                                        (ne - elevation).abs()
                                    })
                                    .fold(0.0f64, f64::max);

                                    color = elevation_overlay(color, elevation, max_diff);
                                }
                            }
                            Layer::Spines => {
                                if let Some(c) = spine_color(&tags) {
                                    color = c;
                                }
                            }
                            _ => {} // marker layers rendered as dot overdraw
                        }
                    }

                    [
                        (color.0 * 255.0).min(255.0) as u8,
                        (color.1 * 255.0).min(255.0) as u8,
                        (color.2 * 255.0).min(255.0) as u8,
                    ]
                })
                .collect::<Vec<_>>()
        })
        .collect();

    log::info!("Pixels: {} in {:.2}s", w * h, lap.elapsed().as_secs_f64());

    let mut buf: Vec<u8> = pixels.into_flattened();

    // ── Phase 4: Marker overlays ──

    let set_pixel = |buf: &mut Vec<u8>, x: i32, y: i32, rgb: [u8; 3]| {
        if x >= 0 && x < w as i32 && y >= 0 && y < h as i32 {
            let off = (y as usize * w + x as usize) * 3;
            buf[off] = rgb[0];
            buf[off + 1] = rgb[1];
            buf[off + 2] = rgb[2];
        }
    };

    let draw_dot = |buf: &mut Vec<u8>, dwx: f64, dwy: f64, radius: i32, rgb: [u8; 3]| {
        let cx = ((dwx - origin_x) / scale) as i32;
        let cy = ((dwy - origin_y) / scale) as i32;
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy <= radius * radius {
                    set_pixel(buf, cx + dx, cy + dy, rgb);
                }
            }
        }
    };

    if layers.contains(&Layer::Centroids) {
        let dot_r = (4.0 / scale).max(2.0) as i32;
        for &(cwx, cwy) in &centroids {
            draw_dot(&mut buf, cwx, cwy, dot_r, [255, 50, 50]);
        }
        log::info!("Centroids: {} markers", centroids.len());
    }

    if layers.contains(&Layer::Spawners) {
        let dot_r = (3.0 / scale).max(2.0) as i32;
        for &(swx, swy, arch) in &spawner_markers {
            draw_dot(&mut buf, swx, swy, dot_r, spawner_marker_color(arch));
        }
        log::info!("Spawners: {} placements", spawner_markers.len());
    }

    if layers.contains(&Layer::SpinePeaks) {
        let dot_r = (5.0 / scale).max(3.0) as i32;
        for &(pwx, pwy) in &spine_epicenters {
            draw_dot(&mut buf, pwx, pwy, dot_r, [255, 220, 50]);
        }
        log::info!("Spine epicenters: {} markers", spine_epicenters.len());
    }

    // ── Phase 5: Encode ──

    let output = cli.output.unwrap_or_else(|| match cli.format.as_str() {
        "png" => "world.png".to_string(),
        _ => "world.qoi".to_string(),
    });

    let lap = Instant::now();
    match cli.format.as_str() {
        "png" => {
            image::save_buffer(&output, &buf, width, height, image::ColorType::Rgb8)
                .expect("Failed to save PNG");
        }
        _ => {
            let encoded = Qoi {
                width,
                height,
                colors: Colors::Rgb,
            }
            .encode_alloc(&buf)
            .expect("QOI encode failed");
            std::fs::write(&output, &encoded).expect("Failed to write QOI");
        }
    }
    log::info!("Encode: {:.2}s", lap.elapsed().as_secs_f64());
    log::info!("Saved {output}");
}
