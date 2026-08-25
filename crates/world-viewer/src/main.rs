//! World Viewer — renders the world event composite to image.

//! Instantiates the same Composite + event stack the server uses.
//! Same seed = same output.

use std::collections::HashMap;
use std::time::Instant;

use clap::Parser;
use rapid_qoi::{Colors, Qoi};
use rayon::prelude::*;

use common::{PlateTag, TagSet};
use world::events::Composite;
use world::events::motion::{
    BoundaryRegime, BoundarySegment, MarginClass, MotionEvent, PlateBoundaryIndex,
};
use world::events::orogen::{OrogenEvent, OrogenSwathIndex, Swath};
use world::events::plates::{PlateEvent, PlateCentroidIndex};
use world::events::tilt::TiltEvent;
use world::events::spines::{SpineEvent, SpineInstanceIndex};
use world::{Cirque, CirqueProbe, Outflow, RIDGE_PEAK_ELEVATION};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Layer {
    /// Crustal substrate, coloured by elevation against sea level.
    Plates,
    /// Height tinting with slope shading.
    Elevation,
    /// Ridge/Highland/Foothills coloring from spine tags.
    Spines,
    /// Macro plate centroid markers (red dots).
    Centroids,
    /// Spine epicenter markers (yellow dots).
    SpinePeaks,
    /// Glacial bowls, drawn as floor / headwall / lip / outlet.
    Cirques,
    /// Plate boundaries, drawn by what the motion resolves them to.
    Boundaries,
    /// Orogen crest lines, with the steep flank shaded so vergence reads.
    OrogenCrests,
    /// PROTOTYPE: orogen as a field — hillshaded surface, composite bypassed.
    OrogenField,
    /// Regional continental tilt as a diverging ramp, with lean arrows.
    Tilt,
    /// PROTOTYPE: belt mask over the substrate coastline — belt shape alone.
    OrogenBelts,
    /// PROTOTYPE: cross-section through the orogen field along the midline.
    OrogenSection,
}

fn parse_layers(s: &str) -> Vec<Layer> {
    s.split(',')
        .map(|name| match name.trim() {
            "plates" => Layer::Plates,
            "elevation" => Layer::Elevation,
            "spines" => Layer::Spines,
            "centroids" => Layer::Centroids,
            "spine-peaks" => Layer::SpinePeaks,
            "cirques" => Layer::Cirques,
            "boundaries" => Layer::Boundaries,
            "orogen-crests" => Layer::OrogenCrests,
            "orogen-field" => Layer::OrogenField,
            "orogen-section" => Layer::OrogenSection,
            "orogen-belts" => Layer::OrogenBelts,
            "tilt" => Layer::Tilt,
            other => {
                eprintln!(
                    "Unknown layer: {other:?}. Valid: plates, elevation, spines, \
                     centroids, spine-peaks, cirques, boundaries, orogen-crests, \n                     orogen-field, orogen-section"
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
    /// Available: plates, elevation, spines, centroids, spine-peaks, cirques,
    /// boundaries, orogen-crests
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

/// Substrate colour: the crust, graded, with sea level as the only edge in it.
/// Land and water are the same field either side of zero, so the ramp is
/// continuous through the datum and the coastline is where it crosses.
fn substrate_color(elevation: f64) -> (f64, f64, f64) {
    const DEEP: (f64, f64, f64) = (0.20, 0.25, 0.45);
    const SHALLOW: (f64, f64, f64) = (0.35, 0.48, 0.62);
    const SHORE: (f64, f64, f64) = (0.70, 0.65, 0.50);
    const INTERIOR: (f64, f64, f64) = (0.30, 0.50, 0.30);

    if elevation < 0.0 {
        lerp_rgb(SHALLOW, DEEP, -elevation / world::SEA_MAX_DEPTH)
    } else {
        lerp_rgb(SHORE, INTERIOR, elevation / world::CONTINENT_MAX_RISE)
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

/// Convergence magnitude that saturates a boundary's colour and width. Above
/// the bulk of the distribution, so ordinary boundaries stay legible against
/// each other instead of all clipping to full intensity.
const BOUNDARY_FULL_SCALE: f64 = 0.25;

/// Boundaries read by what the motion resolves them to.
///
/// Hue carries the one thing the layer is being judged on — whether a chain of
/// boundaries holds one sign along its length — so the sign owns it outright
/// and transform-dominance is drawn as a dash instead of a colour. Margins take
/// their own two hues, because whether a coast carries a range or a plain is
/// the other question being validated.
fn boundary_color(seg: &BoundarySegment) -> [u8; 3] {
    match seg.margin {
        MarginClass::Active => [255, 130, 20],  // orange — arc margin
        // Violet: a passive margin has to be visibly absent, which means a hue
        // no other class uses and one that reads on both the sea and the land.
        MarginClass::Passive => [175, 140, 210],
        MarginClass::Interior => {
            let t = (seg.convergence.abs() / BOUNDARY_FULL_SCALE).clamp(0.0, 1.0);
            if seg.convergence > 0.0 {
                [255, (110.0 - 70.0 * t) as u8, (110.0 - 90.0 * t) as u8]
            } else {
                [(110.0 - 90.0 * t) as u8, (150.0 - 30.0 * t) as u8, 255]
            }
        }
    }
}

/// Outlets read by what leaves them, so a glance separates a closed tarn from
/// one that falls away and one that drains down a valley.
fn cirque_color(part: CirqueProbe, outflow: Outflow) -> [u8; 3] {
    match part {
        CirqueProbe::Floor => [200, 228, 240],   // pale ice
        CirqueProbe::Headwall => [55, 55, 75],   // dark slate
        CirqueProbe::Lip => [190, 165, 120],     // tan bar
        CirqueProbe::Outlet => match outflow {
            Outflow::Impounded => [90, 60, 160],  // violet — nothing leaves
            Outflow::Fall => [255, 40, 200],      // magenta — leaves over a wall
            Outflow::Ravine => [255, 210, 40],    // amber — leaves down a valley
        },
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
            Layer::SpinePeaks => "spine-peaks",
            Layer::Cirques => "cirques",
            Layer::Boundaries => "boundaries",
            Layer::OrogenCrests => "orogen-crests",
            Layer::OrogenField => "orogen-field",
            Layer::OrogenSection => "orogen-section",
            Layer::OrogenBelts => "orogen-belts",
            Layer::Tilt => "tilt",
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


    // PROTOTYPE: the orogen field is point-evaluable, so these modes render
    // straight from the field and never touch the composite.
    if layers.contains(&Layer::OrogenField) || layers.contains(&Layer::OrogenSection)
        || layers.contains(&Layer::OrogenBelts) || layers.contains(&Layer::Tilt) {
        let t = Instant::now();
        let buf = if layers.contains(&Layer::Tilt) {
            render_tilt(&cli, w, h, scale)
        } else if layers.contains(&Layer::OrogenBelts) {
            render_orogen_belts(&cli, w, h, scale)
        } else if layers.contains(&Layer::OrogenSection) {
            render_orogen_section(&cli, w, h, scale)
        } else {
            render_orogen_field(&cli, w, h, scale)
        };
        log::info!("Field: {}x{} in {:.2}s", w, h, t.elapsed().as_secs_f64());
        let output = cli.output.clone().unwrap_or_else(|| "orogen.png".into());
        image::save_buffer(&output, &buf, width, height, image::ColorType::Rgb8)
            .expect("Failed to write PNG");
        log::info!("Saved {output}");
        return;
    }
    // Only add events needed for the requested layers
    let needs_spines = layers.iter().any(|l| {
        matches!(l, Layer::Elevation | Layer::Spines | Layer::SpinePeaks | Layer::Cirques)
    });
    let needs_boundaries = layers.contains(&Layer::Boundaries);
    let needs_orogen = layers.contains(&Layer::OrogenCrests);

    let plate_cache = std::sync::Arc::new(world::PlateCache::new(cli.seed));
    let mut composite = Composite::new(cli.seed);
    composite.add_event(Box::new(PlateEvent::with_cache(plate_cache.clone())));
    composite.add_event(Box::new(TiltEvent::new()));
    if needs_boundaries || needs_orogen {
        composite.add_event(Box::new(MotionEvent::with_cache(plate_cache.clone(), cli.seed)));
    }
    if needs_orogen {
        composite.add_event(Box::new(OrogenEvent::new()));
    }
    if needs_spines {
        composite.add_event(Box::new(SpineEvent::with_cache(plate_cache, cli.seed)));
        composite.add_event(Box::new(world::events::slope_form::SlopeFormEvent::new()));
    }

    // ── Phase 1: Materialize unique hex tiles visible in the pixel grid ──

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

    // Materializing the tiles deforms every layer under them, which is what
    // fills the indexes the marker layers below read.
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

    let cirques: Vec<Cirque> = if layers.contains(&Layer::Cirques) {
        composite.with_indexes(|indexes| {
            indexes
                .get::<SpineInstanceIndex>()
                .map(|idx| {
                    idx.cells
                        .values()
                        .flat_map(|v| v.iter())
                        .flat_map(|inst| inst.cirques.iter().cloned())
                        .collect()
                })
                .unwrap_or_default()
        })
    } else {
        vec![]
    };

    let swaths: Vec<Swath> = if needs_orogen {
        composite.with_indexes(|indexes| {
            indexes
                .get::<OrogenSwathIndex>()
                .map(|idx| {
                    idx.cells.values()
                        .flat_map(|v| v.iter().map(|s| (**s).clone()))
                        .collect()
                })
                .unwrap_or_default()
        })
    } else {
        vec![]
    };

    let boundaries: Vec<BoundarySegment> = if needs_boundaries {
        composite.with_indexes(|indexes| {
            indexes
                .get::<PlateBoundaryIndex>()
                .map(|idx| idx.cells.values().flat_map(|v| v.iter().cloned()).collect())
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
                                color = substrate_color(elevation);
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

    // `dash` of 0 draws solid; anything else leaves every other run of that many
    // pixels blank.
    let draw_line = |buf: &mut Vec<u8>,
                     x0: f64, y0: f64, x1: f64, y1: f64,
                     half_width: i32, dash: i32, rgb: [u8; 3]| {
        let px0 = (x0 - origin_x) / scale;
        let py0 = (y0 - origin_y) / scale;
        let px1 = (x1 - origin_x) / scale;
        let py1 = (y1 - origin_y) / scale;
        let steps = (px1 - px0).abs().max((py1 - py0).abs()).ceil().max(1.0) as i32;
        for i in 0..=steps {
            if dash > 0 && (i / dash) % 2 == 1 { continue; }
            let t = i as f64 / steps as f64;
            let x = (px0 + (px1 - px0) * t).round() as i32;
            let y = (py0 + (py1 - py0) * t).round() as i32;
            for dy in -half_width..=half_width {
                for dx in -half_width..=half_width {
                    set_pixel(buf, x + dx, y + dy, rgb);
                }
            }
        }
    };

    if needs_boundaries {
        // Each segment is drawn as the Voronoi edge it stands for: through the
        // midpoint, along strike. The edge of a hexagonal cell is the spacing
        // over sqrt(3), so drawing the full separation overshoots by 70% and
        // buries the network under its own overdraw.
        const EDGE_OF_SPACING: f64 = 0.577;
        for seg in &boundaries {
            let half = seg.separation() * EDGE_OF_SPACING * 0.5;
            let rgb = boundary_color(seg);
            let strong = (seg.convergence.abs() / BOUNDARY_FULL_SCALE).clamp(0.0, 1.0);
            let hw = (strong * 2.0).round() as i32;
            let dash = if seg.regime() == BoundaryRegime::Transform {
                (3.0 / scale).max(2.0) as i32
            } else {
                0
            };
            draw_line(
                &mut buf,
                seg.mx - seg.strike_x * half, seg.my - seg.strike_y * half,
                seg.mx + seg.strike_x * half, seg.my + seg.strike_y * half,
                hw, dash, rgb,
            );
            // Vergence tick on the steep flank. White so the side reads
            // independently of whatever the segment itself is coloured.
            if seg.vergence_x != 0.0 || seg.vergence_y != 0.0 {
                let tick = seg.separation() * 0.22;
                draw_line(
                    &mut buf,
                    seg.mx, seg.my,
                    seg.mx + seg.vergence_x * tick, seg.my + seg.vergence_y * tick,
                    0, 0, [255, 255, 255],
                );
            }
        }
        let tally = |f: &dyn Fn(&BoundarySegment) -> bool| {
            boundaries.iter().filter(|s| f(s)).count()
        };
        log::info!(
            "Boundaries: {} segments — {} convergent, {} divergent, {} transform-dominant; \
             margins {} active / {} passive",
            boundaries.len(),
            tally(&|s| s.regime() == BoundaryRegime::Convergent),
            tally(&|s| s.regime() == BoundaryRegime::Divergent),
            tally(&|s| s.regime() == BoundaryRegime::Transform),
            tally(&|s| s.margin == MarginClass::Active),
            tally(&|s| s.margin == MarginClass::Passive),
        );
        log::info!(
            "  legend: red=convergent, blue=divergent (width and saturation by magnitude), \
             orange=active margin, violet=passive margin; dashed=transform-dominant; \
             white tick=vergence, on the steep flank"
        );
    }

    if needs_orogen {
        // Vergence is shaded rather than ticked. A belt that holds its polarity
        // shows the dark flank on one side for its whole length and a flip
        // shows it jumping across the crest, which a per-segment tick cannot
        // convey at continental zoom.
        for s in &swaths {
            let (a, b) = s.crest_ends();
            let bands = ((s.steep_width / scale) as i32).clamp(1, 48);
            for i in 1..=bands {
                let off = s.steep_width * i as f64 / bands as f64;
                let t = 1.0 - i as f64 / bands as f64;
                let shade = (25.0 + 50.0 * t) as u8;
                draw_line(
                    &mut buf,
                    a.0 + s.vergence_x * off, a.1 + s.vergence_y * off,
                    b.0 + s.vergence_x * off, b.1 + s.vergence_y * off,
                    0, 0, [shade + 25, shade, shade + 35],
                );
            }
            // Toe of the graded flank, so the long side reads against the short.
            draw_line(
                &mut buf,
                a.0 - s.vergence_x * s.graded_width, a.1 - s.vergence_y * s.graded_width,
                b.0 - s.vergence_x * s.graded_width, b.1 - s.vergence_y * s.graded_width,
                0, (6.0 / scale).max(2.0) as i32, [95, 85, 70],
            );
        }
        // Crests last, over every flank, so a crossing never hides one.
        for s in &swaths {
            let (a, b) = s.crest_ends();
            let c = lerp_rgb((0.55, 0.24, 0.16), (1.0, 0.95, 0.72), s.drive);
            let hw = (s.drive * 2.0).round() as i32;
            draw_line(
                &mut buf, a.0, a.1, b.0, b.1, hw, 0,
                [(c.0 * 255.0) as u8, (c.1 * 255.0) as u8, (c.2 * 255.0) as u8],
            );
        }
        let active = swaths.iter().filter(|s| s.margin == MarginClass::Active).count();
        let mean_drive = swaths.iter().map(|s| s.drive).sum::<f64>()
            / swaths.len().max(1) as f64;
        log::info!(
            "Orogen: {} swaths ({} on active margins), mean drive {mean_drive:.3}",
            swaths.len(), active,
        );
        log::info!(
            "  legend: crest brightens with drive; dark band = steep flank (vergence side); \
             dashed line = graded flank toe"
        );
    }

    if layers.contains(&Layer::Centroids) {
        let dot_r = (4.0 / scale).max(2.0) as i32;
        for &(cwx, cwy) in &centroids {
            draw_dot(&mut buf, cwx, cwy, dot_r, [255, 50, 50]);
        }
        log::info!("Centroids: {} markers", centroids.len());
    }

    if layers.contains(&Layer::SpinePeaks) {
        let dot_r = (5.0 / scale).max(3.0) as i32;
        for &(pwx, pwy) in &spine_epicenters {
            draw_dot(&mut buf, pwx, pwy, dot_r, [255, 220, 50]);
        }
        log::info!("Spine epicenters: {} markers", spine_epicenters.len());
    }

    if layers.contains(&Layer::Cirques) {
        // Rasterize each footprint rather than probing every pixel: bowls cover
        // a small share of a viewport, so the cost tracks their area, not the
        // image's.
        for c in &cirques {
            let px0 = ((c.cx - c.radius - origin_x) / scale).floor() as i32;
            let px1 = ((c.cx + c.radius - origin_x) / scale).ceil() as i32;
            let py0 = ((c.cy - c.radius - origin_y) / scale).floor() as i32;
            let py1 = ((c.cy + c.radius - origin_y) / scale).ceil() as i32;
            for py in py0..=py1 {
                for px in px0..=px1 {
                    let wx = origin_x + px as f64 * scale;
                    let wy = origin_y + py as f64 * scale;
                    if let Some(part) = c.probe(wx, wy) {
                        set_pixel(&mut buf, px, py, cirque_color(part, c.outflow));
                    }
                }
            }
        }
        let tally = |o: Outflow| cirques.iter().filter(|c| c.outflow == o).count();
        log::info!(
            "Cirques: {} bowls ({} impounded, {} falls, {} ravines)",
            cirques.len(),
            tally(Outflow::Impounded), tally(Outflow::Fall), tally(Outflow::Ravine),
        );
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

// ── Orogen field prototype ──────────────────────────────────────────────────
//
// PROTOTYPE ONLY. The orogen field is point-evaluable, so these two modes
// bypass the composite entirely rather than materialising tiles. Nothing here
// is part of the event stack and nothing else in the viewer depends on it.

/// Mirrors the terrain shader's elevation ramp, so a viewer render and the game
/// read the same heights the same way. See `assets/shaders/terrain.wgsl` for
/// what each stop is anchored to.
fn orogen_ramp(z: f64) -> (f64, f64, f64) {
    const STOPS: [(f64, (f64, f64, f64)); 14] = [
        (-200.0, (0.039, 0.078, 0.314)),
        (-50.0, (0.118, 0.235, 0.627)),
        (0.0, (0.275, 0.588, 0.627)),
        (5.0, (0.824, 0.784, 0.588)),
        (20.0, (0.314, 0.627, 0.314)),
        (45.0, (0.290, 0.580, 0.290)),
        (120.0, (0.353, 0.569, 0.267)),
        (300.0, (0.510, 0.510, 0.196)),
        (600.0, (0.549, 0.431, 0.216)),
        (800.0, (0.471, 0.392, 0.275)),
        (950.0, (0.510, 0.490, 0.471)),
        (1050.0, (0.647, 0.635, 0.620)),
        (1150.0, (0.863, 0.863, 0.863)),
        (1200.0, (1.0, 1.0, 1.0)),
    ];
    if z <= STOPS[0].0 { return STOPS[0].1 }
    for i in 0..STOPS.len() - 1 {
        let (a, ca) = STOPS[i];
        let (b, cb) = STOPS[i + 1];
        if z >= a && z < b { return lerp_rgb(ca, cb, (z - a) / (b - a)) }
    }
    STOPS[STOPS.len() - 1].1
}

/// Hillshaded surface, so the fold texture reads independently of how the
/// absolute vertical scale happens to be calibrated.
fn render_orogen_field(cli: &Cli, w: usize, h: usize, scale: f64) -> Vec<u8> {
    let origin_x = cli.center_x - cli.radius;
    let origin_y = cli.center_y - cli.radius;
    let seed = cli.seed;
    // Light from the north-west, low, so ridge lines throw shadow across strike.
    let (lx, ly, lz) = (-0.55f64, -0.55, 0.63);
    let d = scale.max(1.0);

    (0..h).into_par_iter().flat_map(|py| {
        (0..w).flat_map(move |px| {
            let wx = origin_x + px as f64 * scale;
            let wy = origin_y + py as f64 * scale;
            let z = world::orogen_field::surface(wx, wy, seed);
            let zx = world::orogen_field::surface(wx + d, wy, seed);
            let zy = world::orogen_field::surface(wx, wy + d, seed);

            // Sea covers anything below the datum; the seafloor still shades.
            let base = orogen_ramp(z);
            // RISE converts a z-level to world units of height.
            let (gx, gy) = ((zx - z) * 0.8 / d, (zy - z) * 0.8 / d);
            let inv = 1.0 / (gx * gx + gy * gy + 1.0).sqrt();
            let (nx, ny, nz) = (-gx * inv, -gy * inv, inv);
            let lambert = (nx * lx + ny * ly + nz * lz).clamp(0.0, 1.0);
            let shade = 0.35 + 0.85 * lambert;
            let c = (
                (base.0 * shade).clamp(0.0, 1.0),
                (base.1 * shade).clamp(0.0, 1.0),
                (base.2 * shade).clamp(0.0, 1.0),
            );
            [(c.0 * 255.0) as u8, (c.1 * 255.0) as u8, (c.2 * 255.0) as u8]
        }).collect::<Vec<u8>>()
    }).collect()
}

/// Cross-section along the viewport's horizontal midline: substrate, surface,
/// sea level, and which way the wedge leans.
fn render_orogen_section(cli: &Cli, w: usize, h: usize, scale: f64) -> Vec<u8> {
    let seed = cli.seed;
    // Cut ACROSS the local fold axis, not along the image. A section taken
    // along the axis runs down a ridge and shows no ridge-and-valley at all.
    let (ux, uy) = world::orogen_field::project_to_crest(cli.center_x, cli.center_y, seed)
        .map(|c| c.axis)
        .unwrap_or((1.0, 0.0));
    let (ax, ay) = (-uy, ux);
    let at = |px: usize| {
        let t = (px as f64 - w as f64 * 0.5) * scale;
        (cli.center_x + ax * t, cli.center_y + ay * t)
    };
    let surf: Vec<f64> = (0..w)
        .map(|px| { let (x, y) = at(px); world::orogen_field::surface(x, y, seed) })
        .collect();
    let subs: Vec<f64> = (0..w)
        .map(|px| { let (x, y) = at(px); world::substrate_elevation_at(x, y, seed) })
        .collect();

    let hi = surf.iter().cloned().fold(f64::MIN, f64::max).max(50.0);
    let lo = subs.iter().cloned().fold(f64::MAX, f64::min).min(-50.0);
    let span = (hi - lo) * 1.08;
    let to_py = |z: f64| {
        let t = (hi + span * 0.04 - z) / span;
        ((t * h as f64) as i64).clamp(0, h as i64 - 1) as usize
    };

    let mut buf = vec![18u8; w * h * 3];
    let put = |buf: &mut Vec<u8>, px: usize, py: usize, c: [u8; 3]| {
        let k = (py * w + px) * 3;
        buf[k] = c[0]; buf[k + 1] = c[1]; buf[k + 2] = c[2];
    };

    // Sea level, then the two profiles filled from below.
    let sea = to_py(0.0);
    for px in 0..w {
        for py in sea..h { put(&mut buf, px, py, [22, 30, 52]) }
        let sp = to_py(subs[px]);
        for py in sp..h { put(&mut buf, px, py, [52, 44, 34]) }
        let fp = to_py(surf[px]);
        for py in fp..sp.max(fp) { put(&mut buf, px, py, [120, 96, 66]) }
    }
    for px in 0..w { put(&mut buf, px, sea, [90, 130, 190]) }
    for px in 0..w {
        let fp = to_py(surf[px]);
        for dy in 0..2 {
            if fp + dy < h { put(&mut buf, px, fp + dy, [240, 232, 210]) }
        }
    }

    // Vergence at the centre, as a bar in the top-left: which way ridges lean.
    // Vergence side, from the drift and the same axis the section was cut on.
    // The least-curvature eigenvector carries an arbitrary sign per call, so
    // mixing an axis from one call with an `across` from another says nothing.
    let (ddx, ddy) = world::events::motion::plate_drift(cli.center_x, cli.center_y, seed);
    let dir = if ddx * ax + ddy * ay < 0.0 { 1i64 } else { -1 };
    let (bx, by) = (40i64, 40i64);
    for t in 0..60i64 {
        let px = bx + dir * t;
        if px >= 0 && (px as usize) < w {
            for dy in 0..3 { put(&mut buf, px as usize, (by + dy) as usize, [255, 170, 60]) }
        }
    }
    for t in 0..12i64 {
        let px = bx + dir * (60 - t);
        if px >= 0 && (px as usize) < w {
            for dy in -(t / 2)..=(t / 2) {
                let py = by + 1 + dy;
                if py >= 0 && (py as usize) < h { put(&mut buf, px as usize, py as usize, [255, 170, 60]) }
            }
        }
    }
    log::info!("section: across the fold axis; surface max {hi:.0} z, substrate min {lo:.0} z, vergence toward {}", if dir > 0 { "+x" } else { "-x" });
    buf
}

/// Belt mask over the substrate coastline. Shape only: the thickening field
/// thresholded, drawn on the land it stands on, so belt outline can be read
/// without the vertical scale swamping it.
fn render_orogen_belts(cli: &Cli, w: usize, h: usize, scale: f64) -> Vec<u8> {
    let origin_x = cli.center_x - cli.radius;
    let origin_y = cli.center_y - cli.radius;
    let seed = cli.seed;
    (0..h).into_par_iter().flat_map(|py| {
        (0..w).flat_map(move |px| {
            let wx = origin_x + px as f64 * scale;
            let wy = origin_y + py as f64 * scale;
            let e = world::substrate_elevation_at(wx, wy, seed);
            let t = world::orogen_field::relief(wx, wy, seed)
                / world::orogen_field::OROGEN_MAX_RISE;
            let base = if e >= 0.0 { (0.31, 0.40, 0.29) } else { (0.09, 0.13, 0.26) };
            // Four bands, so the threshold sweep in the probe reads off the image.
            let c = match t {
                _ if t >= 0.70 => lerp_rgb(base, (1.00, 0.94, 0.80), 0.95),
                _ if t >= 0.55 => lerp_rgb(base, (0.95, 0.63, 0.25), 0.90),
                _ if t >= 0.40 => lerp_rgb(base, (0.78, 0.34, 0.20), 0.80),
                _ if t >= 0.25 => lerp_rgb(base, (0.45, 0.20, 0.28), 0.70),
                _ => base,
            };
            [(c.0 * 255.0) as u8, (c.1 * 255.0) as u8, (c.2 * 255.0) as u8]
        }).collect::<Vec<u8>>()
    }).collect()
}

/// Regional tilt: a diverging ramp over the land/ocean base, with arrows on a
/// coarse grid showing which way each landmass leans.
///
/// Reads the tilt field directly rather than through the composite. `TiltEvent`
/// is a pure function of position and the substrate beneath it, so this is the
/// same number the event returns, at a fraction of the cost.
fn render_tilt(cli: &Cli, w: usize, h: usize, scale: f64) -> Vec<u8> {
    use world::events::tilt::{TILT_AMPLITUDE, potential, tilt_at};
    let origin_x = cli.center_x - cli.radius;
    let origin_y = cli.center_y - cli.radius;
    let seed = cli.seed;

    let mut buf: Vec<u8> = (0..h).into_par_iter().flat_map(|py| {
        (0..w).flat_map(move |px| {
            let wx = origin_x + px as f64 * scale;
            let wy = origin_y + py as f64 * scale;
            let e = world::substrate_elevation_at(wx, wy, seed);
            if e < 0.0 {
                // Ocean, desaturated — tilt does nothing here and the layer
                // should show that rather than implying it does.
                let d = (1.0 + e / 200.0).clamp(0.0, 1.0);
                let v = 0.10 + 0.10 * d;
                return [(v * 210.0) as u8, (v * 225.0) as u8, (v * 255.0) as u8];
            }
            let t = tilt_at(wx, wy, e, seed) / TILT_AMPLITUDE; // -1 .. 1
            // Diverging: down is blue, up is amber, neutral is a pale grey so
            // zero reads as zero rather than as a colour.
            let c = if t >= 0.0 {
                lerp_rgb((0.88, 0.88, 0.86), (0.85, 0.45, 0.10), t.min(1.0))
            } else {
                lerp_rgb((0.88, 0.88, 0.86), (0.10, 0.35, 0.75), (-t).min(1.0))
            };
            [(c.0 * 255.0) as u8, (c.1 * 255.0) as u8, (c.2 * 255.0) as u8]
        }).collect::<Vec<u8>>()
    }).collect();

    // Lean arrows, downslope, on a coarse grid — magnitude alone cannot show
    // whether a landmass leans one way or several.
    let spacing = (w / 26).max(24);
    let arm = (spacing as f64 * 0.42) as i64;
    let d = scale * 4.0;
    let mut put = |buf: &mut Vec<u8>, x: i64, y: i64| {
        if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 { return }
        let k = (y as usize * w + x as usize) * 3;
        buf[k] = 20; buf[k + 1] = 20; buf[k + 2] = 24;
    };
    for gy in (spacing / 2..h).step_by(spacing) {
        for gx in (spacing / 2..w).step_by(spacing) {
            let wx = origin_x + gx as f64 * scale;
            let wy = origin_y + gy as f64 * scale;
            if world::substrate_elevation_at(wx, wy, seed) < 0.0 { continue }
            let ddx = potential(wx + d, wy, seed) - potential(wx - d, wy, seed);
            let ddy = potential(wx, wy + d, seed) - potential(wx, wy - d, seed);
            let m = ddx.hypot(ddy);
            if m < 1e-12 { continue }
            // Downslope: the way water would run.
            let (ux, uy) = (-ddx / m, -ddy / m);
            for t in 0..=arm {
                put(&mut buf, gx as i64 + (ux * t as f64) as i64,
                              gy as i64 + (uy * t as f64) as i64);
            }
            // Head.
            let (tipx, tipy) = (gx as i64 + (ux * arm as f64) as i64,
                                gy as i64 + (uy * arm as f64) as i64);
            for t in 0..=(arm / 3) {
                let b = t as f64;
                for s in [-1.0f64, 1.0] {
                    put(&mut buf,
                        tipx - (ux * b) as i64 + (-uy * s * b * 0.6) as i64,
                        tipy - (uy * b) as i64 + (ux * s * b * 0.6) as i64);
                }
            }
        }
    }
    buf
}
