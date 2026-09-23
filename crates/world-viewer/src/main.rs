//! World viewer: the event stack, or one event's product, rendered to an image.
//!
//! Two kinds of view, and a view is one or the other. A composite view draws
//! what the stack composes at each tile, read through the composite and
//! nothing else. An event view draws one event's own product: an index read
//! from the registry, or a field read through the event's own functions. The
//! stack built here is the server's stack, so same seed, same image. Which
//! views exist, and when one is added or removed, is the README.

use std::collections::HashMap;
use std::time::Instant;

use clap::Parser;
use rapid_qoi::{Colors, Qoi};
use rayon::prelude::*;

use world::events::Composite;
use world::events::motion::{BoundaryRegime, BoundarySegment, MarginClass, PlateBoundaryIndex};
use world::events::thrusting::{Outlines, CONVERGENCE_FULL};
use world::events::dissection::Valleys;
use world::events::drainage::{surface_at, DrainageIndex};
use world::events::forest;
use world::events::lithology::{rock_on, Rock};
use world::events::migration::ChannelIndex;
use world::events::plates::{unwarp, Coasts, PlateEdgeIndex};
use world::lattice::node_world;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Layer {
    /// Plate index: the substrate from the coasts under the viewport, coloured by elevation against sea level.
    Plates,
    /// Plate field: each plate's age as a grey ramp, black new, white aged: how far erosion has carried each plate.
    Age,
    /// Composite: height on the terrain shader's ramp, with slope shading.
    Elevation,
    /// Plate index: every edge of the plate graph along its chain, coasts white and interior edges grey, a dot at each seed.
    Edges,
    /// Motion index: plate boundaries, drawn by what the motion resolves
    /// them to.
    Boundaries,
    /// Thickening field: the plateau on the substrate, hillshaded.
    ThickeningField,
    /// Lithology field: the rock at the surface by kind, the cuestas
    /// hillshaded over it.
    LithologyField,
    /// Dissection field: the cut on its own, hillshaded.
    DissectionField,
    /// Water field: the dissected ground hillshaded, and every surface
    /// standing over it in blue, darker with depth.
    WaterField,
    /// Tilt field: a diverging ramp, with lean arrows.
    Tilt,
    /// Thrusting index: the deformation fronts, the convergent edges' chains facing the overriding plate.
    Fronts,
    /// Drainage index: every reach as its node chain, width by catchment.
    Reaches,
    /// Channel index: every channel as its train across its flow line, or
    /// the line where it holds it, width by catchment.
    Channels,
    /// Composite: each tile's cover over whatever is drawn beneath, the
    /// canopy's green by fullness and the kinds' shares.
    Forest,
    /// Stand index: the density the stands give each position, in four
    /// plain bands over whatever is drawn beneath, open ground showing it.
    Stands,
    /// Moisture field: what the sky gives each position, the sea's share
    /// less the belts' shadow, on a dry-to-wet ramp; the wind is logged.
    MoistureField,
}

/// Every view by its command-line name. The one list: parsing, the help text
/// and the error message all read it.
const LAYERS: &[(&str, Layer)] = &[
    ("plates", Layer::Plates),
    ("age", Layer::Age),
    ("elevation", Layer::Elevation),
    ("plate-edges", Layer::Edges),
    ("boundaries", Layer::Boundaries),
    ("tilt", Layer::Tilt),
    ("thickening-field", Layer::ThickeningField),
    ("lithology-field", Layer::LithologyField),
    ("dissection-field", Layer::DissectionField),
    ("water-field", Layer::WaterField),
    ("thrusting-fronts", Layer::Fronts),
    ("drainage-reaches", Layer::Reaches),
    ("channels", Layer::Channels),
    ("forest", Layer::Forest),
    ("stands", Layer::Stands),
    ("moisture-field", Layer::MoistureField),
];

impl Layer {
    fn name(self) -> &'static str {
        LAYERS.iter().find(|(_, l)| *l == self).map(|(n, _)| *n).unwrap()
    }

    /// A field view paints every pixel from one field and owns the image, so
    /// it cannot stack with anything.
    fn is_whole_image(self) -> bool {
        matches!(
            self,
            Layer::Tilt
                | Layer::ThickeningField
                | Layer::LithologyField
                | Layer::DissectionField
                | Layer::WaterField
                | Layer::MoistureField
        )
    }
}

fn layer_names() -> String {
    LAYERS.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ")
}

fn layer_help() -> String {
    format!(
        "Comma-separated views drawn bottom to top. Available: {}",
        layer_names()
    )
}

fn parse_layers(s: &str) -> Vec<Layer> {
    s.split(',')
        .map(|name| {
            let name = name.trim();
            LAYERS
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, l)| *l)
                .unwrap_or_else(|| {
                    eprintln!("Unknown layer: {name:?}. Valid: {}", layer_names());
                    std::process::exit(1);
                })
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

    #[arg(long, default_value = "plates,elevation", help = layer_help())]
    layers: String,

    /// Render as the client draws a distance band: summaries of this
    /// radius (0 is the tiles; the ladder is 1, 4, 13, 40), each read from
    /// its seven sample tiles through the whole stack, ground, water and
    /// canopy, in place of the views, with the cost logged. What a band
    /// costs, and what survives it.
    #[arg(long)]
    lod: Option<u32>,
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

/// Darken by local steepness, so relief reads as relief rather than as colour
/// alone. Height is the ramp's job — this only shades.
///
/// The threshold is a share of the ceiling per tile: a belt climbing its full
/// [`thickening::OROGEN_MAX_RISE`] across one half-width averages under half
/// a z per tile, so a tile stepping several z is genuinely steep ground.
fn slope_shade(base: (f64, f64, f64), slope: f64) -> (f64, f64, f64) {
    const STEEP_PER_TILE: f64 = 4.0;
    let t = (slope / STEEP_PER_TILE).clamp(0.0, 0.6);
    lerp_rgb(base, (0.30, 0.25, 0.20), t)
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

    let layer_names: Vec<&str> = layers.iter().map(|l| l.name()).collect();

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


    if let Some(r) = cli.lod {
        let t = Instant::now();
        let buf = render_summaries(&cli, w, h, scale, r);
        log::info!("LoD: {}x{} in {:.2}s", w, h, t.elapsed().as_secs_f64());
        save(&cli, &buf, width, height);
        return;
    }

    if let Some(&view) = layers.iter().find(|l| l.is_whole_image()) {
        if layers.len() > 1 {
            eprintln!(
                "{} paints the whole image and cannot stack with other views",
                view.name()
            );
            std::process::exit(1);
        }
        let t = Instant::now();
        let buf = match view {
            Layer::Tilt => render_tilt(&cli, w, h, scale),
            Layer::DissectionField => render_dissection_field(&cli, w, h, scale),
            Layer::WaterField => render_water_field(&cli, w, h, scale),
            Layer::LithologyField => render_lithology_field(&cli, w, h, scale),
            Layer::MoistureField => render_moisture_field(&cli, w, h, scale),
            _ => render_thickening_field(&cli, w, h, scale),
        };
        log::info!("Field: {}x{} in {:.2}s", w, h, t.elapsed().as_secs_f64());
        save(&cli, &buf, width, height);
        return;
    }
    let needs_boundaries = layers.contains(&Layer::Boundaries);

    // The whole stack, in the order the server builds it, so what the viewer
    // draws is what the world generates.
    let composite = Composite::standard(cli.seed);

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

    let tile_cache: HashMap<(i32, i32), (f64, common::Cover)> = views
        .into_iter()
        .map(|((q, r), v)| ((q, r), (v.elevation, v.cover)))
        .collect();

    let tile_secs = lap.elapsed().as_secs_f64();
    log::info!(
        "Tiles: {} in {tile_secs:.2}s ({:.0} tiles/s)",
        coords.len(),
        coords.len() as f64 / tile_secs
    );

    // ── Phase 2: Read indexes for marker layers ──

    let edges: Vec<world::tectonic::Edge> = if layers.contains(&Layer::Edges) {
        composite.with_indexes(|indexes| {
            indexes
                .get::<PlateEdgeIndex>()
                .map(|idx| idx.cells.values().flat_map(|v| v.iter().cloned()).collect())
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

    // A reach as its node positions plus the node it joins, and the catchment
    // at its last node.
    let reaches: Vec<(Vec<(f64, f64)>, f64)> = if layers.contains(&Layer::Reaches) {
        composite.with_indexes(|indexes| {
            indexes
                .get::<DrainageIndex>()
                .map(|idx| {
                    let at = |k: &(i32, i32)| idx.node(*k).map(|n| (n.wx, n.wy));
                    idx.cells
                        .values()
                        .flat_map(|c| c.reaches.iter())
                        .map(|reach| {
                            let mut pts: Vec<(f64, f64)> = reach.nodes.iter().filter_map(at).collect();
                            pts.extend(reach.joins.as_ref().and_then(at));
                            let catchment = reach.nodes.last().and_then(|k| idx.node(*k)).map_or(1.0, |n| n.catchment);
                            (pts, catchment)
                        })
                        .collect()
                })
                .unwrap_or_default()
        })
    } else {
        vec![]
    };

    // Every convergent edge the deformed cells resolved, for the overlay:
    // its chain, the way onto the overriding plate, and its convergence.
    let fronts: Vec<(Vec<(f64, f64)>, (f64, f64), f64)> = if layers.contains(&Layer::Fronts) {
        composite.with_indexes(|indexes| {
            indexes
                .get::<PlateBoundaryIndex>()
                .map(|idx| {
                    idx.cells
                        .values()
                        .flat_map(|v| v.iter())
                        .filter(|s| s.regime() == BoundaryRegime::Convergent)
                        .map(|s| {
                            let over = s.overriding();
                            let (mx, my) = s.mid();
                            let (dx, dy) = (over.wx - mx, over.wy - my);
                            let l = dx.hypot(dy).max(1.0);
                            let chain = drawn_chain(&s.edge.chain, cli.seed);
                            (chain, (dx / l, dy / l), (s.convergence / CONVERGENCE_FULL).clamp(0.0, 1.0))
                        })
                        .collect()
                })
                .unwrap_or_default()
        })
    } else {
        vec![]
    };

    // Every stand the deformed cells published, as the tiles read them.
    let stands: Option<forest::Reach> = layers.contains(&Layer::Stands).then(|| {
        composite.with_indexes(|indexes| {
            let all: Vec<forest::Stand> = indexes
                .get::<forest::StandIndex>()
                .map(|idx| idx.cells.values().flat_map(|c| c.stands.iter().cloned()).collect())
                .unwrap_or_default();
            forest::Reach::new(all.into_iter())
        })
    });
    let stands = stands.as_ref();

    // ── Phase 3: Render pixels (parallel by row) ──

    let lap = Instant::now();
    let tc = &tile_cache;
    let layer_slice = &layers;
    let seed = cli.seed;
    let coasts_box = Coasts::in_box(cli.center_x, cli.center_y, cli.radius, seed);
    let coasts = &coasts_box;
    let pixels: Vec<[u8; 3]> = (0..h)
        .into_par_iter()
        .flat_map(|py| {
            (0..w)
                .map(move |px| {
                    let wx = origin_x + (px as f64) * scale;
                    let wy = origin_y + (py as f64) * scale;
                    let (q, r) = world::world_to_hex(wx, wy);

                    let (elevation, cover) = tc.get(&(q, r)).copied().unwrap_or((0.0, common::Cover::NONE));

                    let mut color = (0.0f64, 0.0, 0.0);

                    for &layer in layer_slice {
                        match layer {
                            Layer::Plates => {
                                // The substrate itself, which is what this layer
                                // names — its ramp spans the substrate's own
                                // range and nothing above it.
                                color = substrate_color(
                                    world::substrate_on(wx, wy, coasts, seed));
                            }
                            Layer::Age => {
                                let v = 0.15 + 0.8 * world::tectonic::plate_at(wx, wy, seed).age;
                                color = (v, v, v);
                            }
                            Layer::Elevation => {
                                // The composite surface, on the same ramp the
                                // terrain shader uses: dense stops through the
                                // substrate's 45 z of freeboard, then eight more
                                // out to the orogen ceiling at 1,200.
                                color = orogen_ramp(elevation);
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
                                            tc.get(&(q + dq, r + dr)).map_or(0.0, |t| t.0);
                                        (ne - elevation).abs()
                                    })
                                    .fold(0.0f64, f64::max);

                                    color = slope_shade(color, max_diff);
                                }
                            }
                            Layer::Forest => {
                                color = cover_color(color, cover);
                            }
                            Layer::Stands => {
                                if let Some(reach) = stands {
                                    color = stand_color(color, reach.at(wx, wy).0);
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
        // Each edge is drawn along its chain, where the layers above read
        // it through the warp.
        for seg in &boundaries {
            let rgb = boundary_color(seg);
            let strong = (seg.convergence.abs() / BOUNDARY_FULL_SCALE).clamp(0.0, 1.0);
            let hw = (strong * 2.0).round() as i32;
            let dash = if seg.regime() == BoundaryRegime::Transform {
                (3.0 / scale).max(2.0) as i32
            } else {
                0
            };
            for w in drawn_chain(&seg.edge.chain, seed).windows(2) {
                draw_line(&mut buf, w[0].0, w[0].1, w[1].0, w[1].1, hw, dash, rgb);
            }
            // Vergence tick from the edge's midpoint toward the plate going
            // under. White so the side reads independently of the edge's hue.
            if seg.vergence_x != 0.0 || seg.vergence_y != 0.0 {
                let (mx, my) = seg.mid();
                let tick = seg.edge.length() * 0.22;
                draw_line(
                    &mut buf,
                    mx, my,
                    mx + seg.vergence_x * tick, my + seg.vergence_y * tick,
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


    if layers.contains(&Layer::Channels) {
        // Each channel along its train, or its flow line where the river
        // holds it; width by the catchment at its start node, as a reach.
        let drawn: Vec<(Vec<(f64, f64)>, f64, bool)> = composite.with_indexes(|indexes| {
            let (Some(channels), Some(drainage)) = (indexes.get::<ChannelIndex>(), indexes.get::<DrainageIndex>()) else {
                return Vec::new();
            };
            channels
                .cells
                .values()
                .flat_map(|c| c.channels.iter())
                .map(|ch| {
                    let catchment = drainage.node(ch.from).map_or(1.0, |n| n.catchment);
                    match &ch.train {
                        Some(train) => (train.pts.clone(), catchment, true),
                        None => (ch.axis.clone(), catchment, false),
                    }
                })
                .collect()
        });
        let mut trains = 0;
        for (pts, catchment, meanders) in &drawn {
            let half_width = 2.0 * catchment.sqrt() / scale;
            if half_width < 0.5 {
                continue;
            }
            trains += *meanders as usize;
            let hw = half_width.round().min(24.0) as i32;
            let rgb = if *meanders { [60, 170, 255] } else { [120, 200, 255] };
            for w in pts.windows(2) {
                draw_line(&mut buf, w[0].0, w[0].1, w[1].0, w[1].1, hw, 0, rgb);
            }
        }
        log::info!("Channels: {} of which {trains} meander; paler where the river holds its flow line", drawn.len());
    }

    if layers.contains(&Layer::Reaches) {
        // Width grows with the square root of catchment, the way a channel's
        // does with discharge, so a trunk reads as a trunk.
        let mut largest = 0.0f64;
        let mut drawn = 0;
        for (pts, catchment) in &reaches {
            largest = largest.max(*catchment);
            let half_width = 2.0 * catchment.sqrt() / scale;
            // A channel narrower than a pixel is not drawn at this scale.
            if half_width < 0.5 {
                continue;
            }
            drawn += 1;
            let hw = half_width.round().min(24.0) as i32;
            for w in pts.windows(2) {
                draw_line(&mut buf, w[0].0, w[0].1, w[1].0, w[1].1, hw, 0, [60, 170, 255]);
            }
        }
        log::info!(
            "Reaches: {} of {} wide enough to draw at this scale; width by catchment, largest {:.0} nodes",
            drawn,
            reaches.len(),
            largest
        );
    }

    if layers.contains(&Layer::Fronts) {
        // The front as a line, with a tick into the belt so the side reads.
        for (chain, (tx, ty), converge) in &fronts {
            for w in chain.windows(2) {
                draw_line(&mut buf, w[0].0, w[0].1, w[1].0, w[1].1, 0, 0, [255, 235, 90]);
            }
            for &(x0, y0) in chain.iter().skip(1).step_by(2) {
                let tick = 60.0 + 240.0 * converge;
                draw_line(&mut buf, x0, y0, x0 + tx * tick, y0 + ty * tick, 0, 0, [255, 235, 90]);
            }
        }
        log::info!("Fronts: {} convergent edges; ticks point onto the overriding plate, longer for a harder edge", fronts.len());
    }

    if layers.contains(&Layer::Edges) {
        // A coast in white, an interior edge in grey, each along its chain
        // where a tile reads it; a dot at every plate seed.
        let dot_r = (4.0 / scale).max(2.0) as i32;
        let mut seeds: std::collections::HashSet<world::tectonic::PlateId> = std::collections::HashSet::new();
        for e in &edges {
            let rgb = if e.is_coast() { [240, 240, 240] } else { [140, 140, 140] };
            for w in drawn_chain(&e.chain, seed).windows(2) {
                draw_line(&mut buf, w[0].0, w[0].1, w[1].0, w[1].1, 0, 0, rgb);
            }
            for p in [&e.a, &e.b] {
                if seeds.insert(p.id) {
                    draw_dot(&mut buf, p.wx, p.wy, dot_r, if p.continental { [255, 50, 50] } else { [50, 90, 255] });
                }
            }
        }
        log::info!("Plate edges: {} edges, {} coasts; red seeds continental, blue oceanic", edges.len(), edges.iter().filter(|e| e.is_coast()).count());
    }



    save(&cli, &buf, width, height);
}

/// Encode an RGB buffer in the requested format. The default output name
/// follows the format, so `world.qoi` and `world.png` never hold the other's
/// bytes.
/// A chain as a tile sees it: each segment in eight, every point moved to
/// where a tile stands to read it, since the layers read the chain through
/// the plate layer's warp and the drawn line has to lie on what they draw.
fn drawn_chain(chain: &[world::lattice::NodeKey], seed: u64) -> Vec<(f64, f64)> {
    let mut out = Vec::with_capacity(8 * chain.len());
    for (i, w) in chain.windows(2).enumerate() {
        let (x0, y0) = node_world(w[0]);
        let (x1, y1) = node_world(w[1]);
        let last = if i + 2 == chain.len() { 8 } else { 7 };
        for k in 0..=last {
            let t = k as f64 / 8.0;
            out.push(unwarp(x0 + (x1 - x0) * t, y0 + (y1 - y0) * t, seed));
        }
    }
    out
}

fn save(cli: &Cli, buf: &[u8], width: u32, height: u32) {
    let output = cli.output.clone().unwrap_or_else(|| match cli.format.as_str() {
        "png" => "world.png".to_string(),
        _ => "world.qoi".to_string(),
    });

    let lap = Instant::now();
    match cli.format.as_str() {
        "png" => {
            image::save_buffer(&output, buf, width, height, image::ColorType::Rgb8)
                .expect("Failed to save PNG");
        }
        _ => {
            let encoded = Qoi {
                width,
                height,
                colors: Colors::Rgb,
            }
            .encode_alloc(buf)
            .expect("QOI encode failed");
            std::fs::write(&output, &encoded).expect("Failed to write QOI");
        }
    }
    log::info!("Encode: {:.2}s", lap.elapsed().as_secs_f64());
    log::info!("Saved {output}");
}

// ── Field views ─────────────────────────────────────────────────────────────
//
// Point-evaluable fields, read through the event's own functions rather than
// through the composite: no tile is materialised, and the number drawn is the
// number the event returns, never a re-derivation of it.

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

/// The cut on its own, hillshaded: every valley as a depression in a flat
/// sheet, darker the deeper, so the network's shape reads without the
/// envelope under it. Routes the drainage cells under the viewport itself,
/// as the event's own prepare reads them.
fn render_dissection_field(cli: &Cli, w: usize, h: usize, scale: f64) -> Vec<u8> {
    let origin_x = cli.center_x - cli.radius;
    let origin_y = cli.center_y - cli.radius;
    let seed = cli.seed;
    let (lx, ly, lz) = (-0.55f64, -0.55, 0.63);
    let d = scale.max(1.0);
    let valleys = Valleys::in_box(cli.center_x, cli.center_y, cli.radius, seed);
    let valleys = &valleys;
    let outlines = Outlines::in_box(cli.center_x, cli.center_y, cli.radius, seed);
    let outlines = &outlines;
    let coasts = Coasts::in_box(cli.center_x, cli.center_y, cli.radius, seed);
    let coasts = &coasts;
    let cut = move |x: f64, y: f64| {
        let envelope = surface_at(x, y, seed, coasts, outlines);
        valleys.cut_at(x, y, envelope)
    };

    (0..h).into_par_iter().flat_map(|py| {
        (0..w).flat_map(move |px| {
            let wx = origin_x + px as f64 * scale;
            let wy = origin_y + py as f64 * scale;
            let z = -cut(wx, wy);
            let zx = -cut(wx + d, wy);
            let zy = -cut(wx, wy + d);
            // Depth on a grey ramp: white at the envelope, dark at 100 z down.
            let tone = 0.95 - 0.7 * (-z / 100.0).clamp(0.0, 1.0);
            let (gx, gy) = ((zx - z) * world::RISE / d, (zy - z) * world::RISE / d);
            let inv = 1.0 / (gx * gx + gy * gy + 1.0).sqrt();
            let (nx, ny, nz) = (-gx * inv, -gy * inv, inv);
            let lambert = (nx * lx + ny * ly + nz * lz).clamp(0.0, 1.0);
            let shade = 0.35 + 0.85 * lambert;
            let c = (tone * shade).clamp(0.0, 1.0);
            [(c * 255.0) as u8, (c * 255.0) as u8, ((c * 0.92) * 255.0) as u8]
        }).collect::<Vec<u8>>()
    }).collect()
}

/// The dissected ground hillshaded in grey, and every surface standing over
/// it in blue, darker with depth: the sea and the channels.
/// Routes the drainage cells under the viewport as the event's prepare does.
fn render_water_field(cli: &Cli, w: usize, h: usize, scale: f64) -> Vec<u8> {
    let origin_x = cli.center_x - cli.radius;
    let origin_y = cli.center_y - cli.radius;
    let seed = cli.seed;
    let (lx, ly, lz) = (-0.55f64, -0.55, 0.63);
    let d = scale.max(1.0);
    let valleys = Valleys::in_box(cli.center_x, cli.center_y, cli.radius, seed);
    let valleys = &valleys;
    let outlines = Outlines::in_box(cli.center_x, cli.center_y, cli.radius, seed);
    let outlines = &outlines;
    let coasts = Coasts::in_box(cli.center_x, cli.center_y, cli.radius, seed);
    let coasts = &coasts;
    // The ground after the cuts, and the surface over it as the tile rule
    // reads it: rounded to steps, dry where the surface's step is not above
    // the ground's.
    let sample = move |x: f64, y: f64| -> (f64, Option<f64>) {
        let envelope = surface_at(x, y, seed, coasts, outlines);
        let cuts = valleys.cuts_at(x, y, envelope);
        let ground = envelope - cuts.valley - cuts.channel;
        let water = valleys
            .surface_at(ground, cuts)
            .map(|s| s.round())
            .filter(|s| *s > ground.round());
        (ground, water)
    };

    (0..h).into_par_iter().flat_map(|py| {
        (0..w).flat_map(move |px| {
            let wx = origin_x + px as f64 * scale;
            let wy = origin_y + py as f64 * scale;
            let (z, water) = sample(wx, wy);
            let (zx, _) = sample(wx + d, wy);
            let (zy, _) = sample(wx, wy + d);
            let (gx, gy) = ((zx - z) * world::RISE / d, (zy - z) * world::RISE / d);
            let inv = 1.0 / (gx * gx + gy * gy + 1.0).sqrt();
            let (nx, ny, nz) = (-gx * inv, -gy * inv, inv);
            let lambert = (nx * lx + ny * ly + nz * lz).clamp(0.0, 1.0);
            let shade = 0.35 + 0.85 * lambert;
            match water {
                Some(s) => {
                    // Depth on a blue ramp: pale at a step deep, deep blue at 30.
                    let t = ((s - z) / 30.0).clamp(0.0, 1.0);
                    let c = lerp_rgb((0.55, 0.75, 0.95), (0.05, 0.15, 0.45), t);
                    [(c.0 * 255.0) as u8, (c.1 * 255.0) as u8, (c.2 * 255.0) as u8]
                }
                None => {
                    let c = (0.85 * shade).clamp(0.0, 1.0);
                    [(c * 255.0) as u8, (c * 0.97 * 255.0) as u8, (c * 0.9 * 255.0) as u8]
                }
            }
        }).collect::<Vec<u8>>()
    }).collect()
}

/// The viewport as one of the client's distance bands draws it: summaries
/// of radius `r`, each the height and water the seven-sample rule selects
/// from tiles read through the whole stack, on the elevation ramp with
/// water in blue by depth. What the band costs is logged: the first tile,
/// which opens the cells, and then the summaries, each seven tiles.
fn render_summaries(cli: &Cli, w: usize, h: usize, scale: f64, r: u32) -> Vec<u8> {
    use common::summary::{scale as summary_scale, summarize};
    let origin_x = cli.center_x - cli.radius;
    let origin_y = cli.center_y - cli.radius;
    let s = summary_scale(r) as f64;
    // The summary lattice is the tile grid scaled by the summary's width, so
    // the tile conversion at that scale finds a pixel's summary.
    let cell_of = |wx: f64, wy: f64| world::world_to_hex(wx / s, wy / s);
    let mut cells: std::collections::HashSet<(i32, i32)> = std::collections::HashSet::new();
    for py in 0..h {
        for px in 0..w {
            cells.insert(cell_of(origin_x + px as f64 * scale, origin_y + py as f64 * scale));
        }
    }
    let cells: Vec<(i32, i32)> = cells.into_iter().collect();
    let composite = Composite::standard(cli.seed);
    let composite = &composite;

    let t = Instant::now();
    let (sq, sr) = cells[0];
    composite.tile_at(sq * s as i32, sr * s as i32);
    let first = t.elapsed();
    let t = Instant::now();
    let summaries: HashMap<(i32, i32), common::summary::SummaryCell> = cells
        .par_iter()
        .map(|&(sq, sr)| ((sq, sr), summarize(r, sq, sr, composite).expect("the composite has every tile")))
        .collect();
    let took = t.elapsed();
    let wet = summaries.values().filter(|c| c.water.is_some()).count();
    let wooded = summaries.values().filter(|c| !c.canopy.is_empty()).count();
    log::info!(
        "LoD r={r} (summaries {s} tiles wide): {} summaries, {} of them water, {} wooded, from {} samples; first tile {:.0} ms, then {:.2} s wall over every core ({:.0} µs per summary, {:.1} per sample)",
        summaries.len(),
        wet,
        wooded,
        summaries.len() * 7,
        first.as_secs_f64() * 1e3,
        took.as_secs_f64(),
        took.as_secs_f64() * 1e6 / summaries.len() as f64,
        took.as_secs_f64() * 1e6 / (summaries.len() * 7) as f64
    );

    let summaries = &summaries;
    (0..h)
        .into_par_iter()
        .flat_map(|py| {
            (0..w)
                .flat_map(move |px| {
                    let cell = summaries[&cell_of(origin_x + px as f64 * scale, origin_y + py as f64 * scale)];
                    let c = match cell.water {
                        // Depth on the water field's blue ramp: pale at a
                        // step deep, deep blue at 30.
                        Some(surface) => lerp_rgb((0.55, 0.75, 0.95), (0.05, 0.15, 0.45), ((surface - cell.z) as f64 / 30.0).clamp(0.0, 1.0)),
                        None => canopy_color(orogen_ramp(cell.z as f64), cell.canopy),
                    };
                    [(c.0 * 255.0).min(255.0) as u8, (c.1 * 255.0).min(255.0) as u8, (c.2 * 255.0).min(255.0) as u8]
                })
                .collect::<Vec<u8>>()
        })
        .collect()
}

/// The plateau on the substrate, hillshaded, so the thickening's shape reads
/// independently of how the vertical scale is calibrated. Marches the fronts
/// under the viewport itself, since the plateau rises with the wedge.
/// A kind's green: pine blue-green, deciduous green, brush olive.
fn kind_color(kind: common::Slot) -> (f64, f64, f64) {
    match kind {
        common::Slot::Pine => (0.05, 0.30, 0.22),
        common::Slot::Deciduous => (0.16, 0.48, 0.12),
        common::Slot::Brush => (0.45, 0.50, 0.18),
        common::Slot::Empty => (0.0, 0.0, 0.0),
    }
}

/// Trees over the colour beneath them: the kinds' greens blended by their
/// shares, laid over the ground by the density.
fn trees_color(ground: (f64, f64, f64), kinds: impl Iterator<Item = common::Slot>, density: f64) -> (f64, f64, f64) {
    let (mut canopy, mut n) = ((0.0, 0.0, 0.0), 0.0);
    for kind in kinds {
        let c = kind_color(kind);
        canopy = (canopy.0 + c.0, canopy.1 + c.1, canopy.2 + c.2);
        n += 1.0;
    }
    if n == 0.0 {
        return ground;
    }
    lerp_rgb(ground, (canopy.0 / n, canopy.1 / n, canopy.2 / n), 0.35 + 0.65 * density)
}

/// A tile's cover over the colour beneath it, by its fullness.
fn cover_color(ground: (f64, f64, f64), cover: common::Cover) -> (f64, f64, f64) {
    trees_color(ground, cover.filled().map(|(_, s)| s), cover.fullness() as f64 / common::SLOTS.len() as f64)
}

/// A stand's density over the colour beneath it, in four bands by its
/// share of a closed stand's: open shows the ground, then thin, half and
/// closed, one green darkening. Bands, not a ramp, so a boundary reads as
/// an edge and the tile hash never shows.
fn stand_color(ground: (f64, f64, f64), density: f64) -> (f64, f64, f64) {
    let share = density / forest::DENSITY_MAX;
    if share < 0.05 {
        ground
    } else if share < 1.0 / 3.0 {
        (0.60, 0.70, 0.35)
    } else if share < 2.0 / 3.0 {
        (0.30, 0.52, 0.20)
    } else {
        (0.08, 0.30, 0.10)
    }
}

/// A summary's canopy over the colour beneath it, by its density.
fn canopy_color(ground: (f64, f64, f64), canopy: common::Canopy) -> (f64, f64, f64) {
    let kinds = [common::Slot::Pine, common::Slot::Deciduous, common::Slot::Brush];
    trees_color(ground, kinds.into_iter().flat_map(|k| std::iter::repeat(k).take(canopy.count(k) as usize)), canopy.density())
}

/// What the sky gives each position, on a ramp from the dry ground's tan
/// through the interior's grey-green to the wet coast's deep green, the
/// sea blue. Reads the forest layer's own functions off the coasts and
/// outlines under the viewport, so the rain shadow is the one the stands
/// read.
fn render_moisture_field(cli: &Cli, w: usize, h: usize, scale: f64) -> Vec<u8> {
    let origin_x = cli.center_x - cli.radius;
    let origin_y = cli.center_y - cli.radius;
    let seed = cli.seed;
    let wind = forest::wind(seed);
    log::info!(
        "wind blows toward ({:.2}, {:.2}), bearing {:.0}° from +x",
        wind.0,
        wind.1,
        wind.1.atan2(wind.0).to_degrees()
    );
    let outlines = Outlines::in_box(cli.center_x, cli.center_y, cli.radius, seed);
    let outlines = &outlines;
    let coasts = Coasts::in_box(cli.center_x, cli.center_y, cli.radius, seed);
    let coasts = &coasts;

    (0..h).into_par_iter().flat_map(|py| {
        (0..w).flat_map(move |px| {
            let wx = origin_x + px as f64 * scale;
            let wy = origin_y + py as f64 * scale;
            let c = if world::substrate_on(wx, wy, coasts, seed) <= 0.0 {
                (0.18, 0.30, 0.50)
            } else {
                let m = forest::sky_moisture(wx, wy, wind, coasts, outlines);
                let dry = (0.78, 0.66, 0.42);
                let mid = (0.55, 0.60, 0.35);
                let wet = (0.08, 0.40, 0.18);
                if m < forest::MOISTURE_CLOSED {
                    lerp_rgb(dry, mid, m / forest::MOISTURE_CLOSED)
                } else {
                    lerp_rgb(mid, wet, (m - forest::MOISTURE_CLOSED) / (1.0 - forest::MOISTURE_CLOSED))
                }
            };
            [(c.0 * 255.0) as u8, (c.1 * 255.0) as u8, (c.2 * 255.0) as u8]
        }).collect::<Vec<u8>>()
    }).collect()
}

fn render_thickening_field(cli: &Cli, w: usize, h: usize, scale: f64) -> Vec<u8> {
    let origin_x = cli.center_x - cli.radius;
    let origin_y = cli.center_y - cli.radius;
    let seed = cli.seed;
    // Light from the north-west, low, so belts throw shadow across strike.
    let (lx, ly, lz) = (-0.55f64, -0.55, 0.63);
    let d = scale.max(1.0);
    // The plateau is read off the plate graph, so the view builds the
    // outlines under the viewport, as the event's own prepare does.
    let outlines = Outlines::in_box(cli.center_x, cli.center_y, cli.radius, seed);
    let outlines = &outlines;
    let coasts = Coasts::in_box(cli.center_x, cli.center_y, cli.radius, seed);
    let coasts = &coasts;
    let surface = move |x: f64, y: f64| {
        world::substrate_on(x, y, coasts, seed) + world::events::thickening::thickening_on(x, y, outlines)
    };

    (0..h).into_par_iter().flat_map(|py| {
        (0..w).flat_map(move |px| {
            let wx = origin_x + px as f64 * scale;
            let wy = origin_y + py as f64 * scale;
            let z = surface(wx, wy);
            let zx = surface(wx + d, wy);
            let zy = surface(wx, wy + d);
            let base = orogen_ramp(z);
            // RISE converts a z-level to this crate's horizontal unit of height.
            let (gx, gy) = ((zx - z) * world::RISE / d, (zy - z) * world::RISE / d);
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

/// The rock at the surface by kind, shale grey, sandstone tan, limestone
/// pale, basement dark red, the sea blue, with the cuestas hillshaded: the
/// bands and rings the cover makes and the scarps that stand on them.
fn render_lithology_field(cli: &Cli, w: usize, h: usize, scale: f64) -> Vec<u8> {
    let origin_x = cli.center_x - cli.radius;
    let origin_y = cli.center_y - cli.radius;
    let seed = cli.seed;
    let (lx, ly, lz) = (-0.55f64, -0.55, 0.63);
    let d = scale.max(1.0);
    let outlines = Outlines::in_box(cli.center_x, cli.center_y, cli.radius, seed);
    let outlines = &outlines;
    let coasts = Coasts::in_box(cli.center_x, cli.center_y, cli.radius, seed);
    let coasts = &coasts;
    let at = move |x: f64, y: f64| {
        let substrate = world::substrate_on(x, y, coasts, seed);
        (rock_on(x, y, seed, coasts, outlines), substrate)
    };
    let mut counts = [0usize; 5];
    let buf: Vec<u8> = (0..h).into_par_iter().flat_map(|py| {
        (0..w).flat_map(move |px| {
            let wx = origin_x + px as f64 * scale;
            let wy = origin_y + py as f64 * scale;
            let (g, substrate) = at(wx, wy);
            if substrate <= 0.0 {
                return [40u8, 70, 140];
            }
            let base: (f64, f64, f64) = match g.rock {
                Rock::Shale => (0.55, 0.55, 0.52),
                Rock::Sandstone => (0.82, 0.68, 0.42),
                Rock::Limestone => (0.90, 0.88, 0.78),
                Rock::Basement => (0.55, 0.25, 0.22),
            };
            let z = g.stand;
            let zx = at(wx + d, wy).0.stand;
            let zy = at(wx, wy + d).0.stand;
            let (gx, gy) = ((zx - z) * world::RISE / d, (zy - z) * world::RISE / d);
            let inv = 1.0 / (gx * gx + gy * gy + 1.0).sqrt();
            let (nx, ny, nz) = (-gx * inv, -gy * inv, inv);
            let lambert = (nx * lx + ny * ly + nz * lz).clamp(0.0, 1.0);
            let shade = 0.45 + 0.7 * lambert;
            [
                ((base.0 * shade).clamp(0.0, 1.0) * 255.0) as u8,
                ((base.1 * shade).clamp(0.0, 1.0) * 255.0) as u8,
                ((base.2 * shade).clamp(0.0, 1.0) * 255.0) as u8,
            ]
        }).collect::<Vec<u8>>()
    }).collect();
    // A census of the land in view, for the eye's check of the shares.
    let step = (w / 200).max(1);
    for py in (0..h).step_by(step) {
        for px in (0..w).step_by(step) {
            let (g, substrate) = at(origin_x + px as f64 * scale, origin_y + py as f64 * scale);
            if substrate <= 0.0 { counts[4] += 1; continue }
            counts[match g.rock { Rock::Shale => 0, Rock::Sandstone => 1, Rock::Limestone => 2, Rock::Basement => 3 }] += 1;
        }
    }
    let land: usize = counts[..4].iter().sum::<usize>().max(1);
    log::info!(
        "Lithology: shale {:.0}%, sandstone {:.0}%, limestone {:.0}%, basement {:.0}% of the land in view",
        100.0 * counts[0] as f64 / land as f64,
        100.0 * counts[1] as f64 / land as f64,
        100.0 * counts[2] as f64 / land as f64,
        100.0 * counts[3] as f64 / land as f64,
    );
    buf
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
    let coasts = Coasts::in_box(cli.center_x, cli.center_y, cli.radius, seed);
    let coasts = &coasts;

    let mut buf: Vec<u8> = (0..h).into_par_iter().flat_map(|py| {
        (0..w).flat_map(move |px| {
            let wx = origin_x + px as f64 * scale;
            let wy = origin_y + py as f64 * scale;
            let e = world::substrate_on(wx, wy, coasts, seed);
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
    let put = |buf: &mut Vec<u8>, x: i64, y: i64| {
        if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 { return }
        let k = (y as usize * w + x as usize) * 3;
        buf[k] = 20; buf[k + 1] = 20; buf[k + 2] = 24;
    };
    for gy in (spacing / 2..h).step_by(spacing) {
        for gx in (spacing / 2..w).step_by(spacing) {
            let wx = origin_x + gx as f64 * scale;
            let wy = origin_y + gy as f64 * scale;
            if world::substrate_on(wx, wy, coasts, seed) < 0.0 { continue }
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
