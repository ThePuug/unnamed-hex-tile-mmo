//! The sky: the prevailing wind and the moisture it carries to a position,
//! the sea's share falling inland up the coast's own ramp, less what every
//! belt between the position and the wind has wrung out, and more where
//! the ground lifts the air. A belt is a convergent edge's wedge, read off
//! the plate outlines thrusting published, so the shadow is the belt's and
//! never a field's. It stands on the plates and the thrust alone, below
//! everything that reads it: the water that the rain feeds and the forest
//! that drinks it. As a layer it puts what the sky gives on every tile
//! ([`TileView::moisture`]), and publishes it sampled over the ground
//! ([`Moisture`]) for a structural pass above, which has no tile to read.

use std::f64::consts::PI;

use crate::chains::Segment;
use crate::noise::hash_f64;
use crate::smoothstep;
use crate::tectonic::PLATE_SPACING;
use std::any::Any;

use crate::hex_to_world;
use super::plates::{coasts_of, Coasts, COAST_REACH};
use super::thrusting::{outlines_of, rim_of, sheets_of, EdgeOutline, Outlines, Standing, OUTLINE_REACH, RANGE_RISE, RANGE_SPACING, WEDGE_SHEETS};
use super::field::{blocks_of, Sampled};
use super::index::IndexRegistry;
use super::{CellScope, TileOutput, TileView, WorldEvent, RING_CLEARANCE};

const WIND_SEED: u64 = 0x7769_6e64;

/// The prevailing wind: one bearing for the whole world, a quantity of the
/// seed, as a unit vector in world space pointing the way it blows.
pub fn wind(seed: u64) -> (f64, f64) {
    let a = 2.0 * PI * hash_f64(0, 0, seed ^ WIND_SEED);
    (a.cos(), a.sin())
}

/// The share of the sea's moisture that reaches the interior: what is
/// left where the sea's reach ends and holds from there.
pub const INTERIOR_MOISTURE: f64 = 0.42;

/// How far inland the sea's moisture falls to the interior's share: a
/// quarter of a plate, short of the substrate's own ramp, so a continent
/// two plates wide has an interior.
pub const MOISTURE_REACH: f64 = 0.25 * PLATE_SPACING;

/// What a full wedge wrings out of the air crossing it: the share of what
/// arrives that its lee does without.
pub const SHADOW_DEPTH: f64 = 0.6;

/// How far upwind a belt is looked for: the reach of the shadow it casts.
pub const SHADOW_REACH: f64 = PLATE_SPACING;

/// The sea's moisture at a position: everything at the shore, falling
/// over [`MOISTURE_REACH`] to the interior's share.
pub fn sea_moisture(wx: f64, wy: f64, coasts: &Coasts) -> f64 {
    let (frac, land) = coasts.shore(wx, wy);
    if !land {
        return 1.0;
    }
    1.0 - (1.0 - INTERIOR_MOISTURE) * smoothstep(frac * COAST_REACH / MOISTURE_REACH)
}

/// Where a ray from `(x, y)` along `(ux, uy)`, `reach` long, first
/// crosses the edge's chain, as the distance along the ray: its bounding
/// box first, then its segments. None where it does not.
fn crossing(edge: &EdgeOutline, x: f64, y: f64, ux: f64, uy: f64, reach: f64) -> Option<f64> {
    let [x0, y0, x1, y1] = edge.bounds;
    let slab = |p: f64, d: f64, lo: f64, hi: f64| -> Option<(f64, f64)> {
        if d.abs() < 1e-12 {
            (p >= lo && p <= hi).then_some((0.0, reach))
        } else {
            let (a, b) = ((lo - p) / d, (hi - p) / d);
            Some((a.min(b), a.max(b)))
        }
    };
    let (tx0, tx1) = slab(x, ux, x0, x1)?;
    let (ty0, ty1) = slab(y, uy, y0, y1)?;
    if tx0.max(ty0).max(0.0) > tx1.min(ty1).min(reach) {
        return None;
    }
    edge.segments
        .iter()
        .filter_map(|s| ray_hits(s, x, y, ux, uy, reach))
        .min_by(|a, b| a.partial_cmp(b).unwrap())
}

/// Where a ray crosses one segment within `reach` of its start, as the
/// distance along the ray.
fn ray_hits(s: &Segment, x: f64, y: f64, ux: f64, uy: f64, reach: f64) -> Option<f64> {
    let (ex, ey) = (s.x1 - s.x0, s.y1 - s.y0);
    let den = ux * ey - uy * ex;
    if den.abs() < 1e-12 {
        return None;
    }
    let (px, py) = (s.x0 - x, s.y0 - y);
    let t = (px * ey - py * ex) / den;
    let u = (px * uy - py * ux) / den;
    (t >= 0.0 && t <= reach && (0.0..=1.0).contains(&u)).then_some(t)
}

/// How much of a belt's shadow is left `t` downwind of it: the whole for
/// half the reach, then fading to nothing at the reach, since the air
/// takes up moisture again as it goes.
fn fade(t: f64) -> f64 {
    1.0 - smoothstep((t - 0.5 * SHADOW_REACH) / (0.5 * SHADOW_REACH))
}

/// The share of the air's moisture the belts between a position and the
/// wind's source have wrung out, 0 to 1. The plate the position stands in
/// reads its own wedges by where it stands in them: the windward flank has
/// none up to the last sheet below the rim, and past the rim, or under air
/// that came over it, the wedge's whole; every other plate's wedge shadows
/// it where its front lies across the way upwind. A shadow eases in over
/// one sheet's spacing either side of the rim, so nothing steps there, and
/// is as deep as the share of the wedge that stands.
pub fn shadow(wx: f64, wy: f64, wind: (f64, f64), outlines: &Outlines) -> f64 {
    shadow_standing(outlines.at(wx, wy).as_ref(), wind, outlines)
}

/// [`shadow`] where the plate the position stands in was found already,
/// or found to be none.
fn shadow_standing(at: Option<&Standing>, wind: (f64, f64), outlines: &Outlines) -> f64 {
    let Some(at) = at else { return 0.0 };
    let (ux, uy) = (-wind.0, -wind.1);
    let mut passes = 1.0;
    for (e, d) in at.plate.edges.iter().zip(&at.distances) {
        if e.converge <= 0.0 {
            continue;
        }
        let sheets = sheets_of(e.converge);
        let rim = rim_of(sheets);
        let share = if let Some(t) = crossing(e, at.x, at.y, ux, uy, SHADOW_REACH) {
            smoothstep((d - (rim - RANGE_SPACING)) / RANGE_SPACING) * fade(t)
        } else if *d < rim && e.distance(at.x + ux * rim, at.y + uy * rim).0 >= rim {
            smoothstep((rim - d) / RANGE_SPACING)
        } else {
            0.0
        };
        passes *= 1.0 - SHADOW_DEPTH * share * e.standing * sheets / WEDGE_SHEETS;
    }
    for e in outlines.fronts_within(at.plate.id, at.x, at.y, SHADOW_REACH) {
        if let Some(t) = crossing(e, at.x, at.y, ux, uy, SHADOW_REACH) {
            passes *= 1.0 - SHADOW_DEPTH * fade(t) * e.standing * sheets_of(e.converge) / WEDGE_SHEETS;
        }
    }
    1.0 - passes
}

/// How much more than the lowland before it a range's full height wrings
/// out of the air climbing it: the windward flanks of mid-latitude ranges
/// take two to four times their lowland's rain.
pub const LIFT: f64 = 2.0;

/// What the sky gives a position: the sea's moisture less the belts'
/// shadow, and more as the ground lifts the air, by [`LIFT`] at a full
/// range's rise; one is the shore's lowland, and a range's flanks take
/// more.
pub fn sky_moisture(wx: f64, wy: f64, wind: (f64, f64), coasts: &Coasts, outlines: &Outlines) -> f64 {
    sky_standing(wx, wy, wind, coasts, outlines, outlines.at(wx, wy).as_ref())
}

/// [`sky_moisture`] where the plate the position stands in was found
/// already, or found to be none.
fn sky_standing(wx: f64, wy: f64, wind: (f64, f64), coasts: &Coasts, outlines: &Outlines, at: Option<&Standing>) -> f64 {
    let lift = at.map_or(1.0, |at| 1.0 + LIFT * outlines.relief_of(at) / RANGE_RISE);
    sea_moisture(wx, wy, coasts) * (1.0 - shadow_standing(at, wind, outlines)) * lift
}

// ── The layer ────────────────────────────────────────────────────

/// How far the sky reads from a tile: the whole outline of the plate it
/// stands in, and the belts upwind of it as far as a shadow reaches.
pub const SKY_REACH: f64 = OUTLINE_REACH + SHADOW_REACH;

/// The cell scale: one ring holds everything within [`SKY_REACH`].
pub const SKY_CELL_SCALE: u32 = ((SKY_REACH as u32 + 1) as f64 / RING_CLEARANCE) as u32 + 1;

/// What the sky gives, as its samples are published
/// ([`Sampled`]`<Moisture>`).
pub struct Moisture;

/// How far apart the sky's samples lie, in tiles: closer than a belt's
/// rim or a coast's ramp turns, so a read between two is the sky there.
pub const SKY_SPACING: i32 = 128;

/// The sky as a layer of the stack: what it gives each tile, for the
/// layers above to read off the tile.
pub struct SkyEvent;

impl SkyEvent {
    pub fn new() -> Self {
        SkyEvent
    }
}

impl Default for SkyEvent {
    fn default() -> Self {
        Self::new()
    }
}

/// What a cell's tiles read the sky through: the coasts and the outlines
/// in its reach, and the wind.
struct Sky {
    coasts: Coasts,
    outlines: Outlines,
    wind: (f64, f64),
}

impl WorldEvent for SkyEvent {
    fn name(&self) -> &str { "sky" }
    fn scale(&self) -> u32 { SKY_CELL_SCALE }
    fn max_influence(&self) -> u32 { SKY_REACH.ceil() as u32 }

    fn register_indexes(&self, registry: &mut IndexRegistry) {
        registry.pre_register::<Sampled<Moisture>>();
    }

    /// The sky's samples over the cell, filled as they are read.
    fn deform(&self, scope: &CellScope) {
        let sky = Sky { coasts: coasts_of(scope), outlines: outlines_of(scope), wind: wind(scope.seed()) };
        let source = std::sync::Arc::new(move |wx: f64, wy: f64| sky_moisture(wx, wy, sky.wind, &sky.coasts, &sky.outlines));
        scope.publish::<Sampled<Moisture>>(blocks_of(scope, SKY_SPACING, source));
    }

    fn prepare(&self, scope: &CellScope) -> Box<dyn Any + Send + Sync> {
        Box::new(Sky { coasts: coasts_of(scope), outlines: outlines_of(scope), wind: wind(scope.seed()) })
    }

    fn query(&self, q: i32, r: i32, below: &TileView, cell: &(dyn Any + Send + Sync), _seed: u64) -> Option<TileOutput> {
        let sky = cell.downcast_ref::<Sky>()?;
        let (wx, wy) = hex_to_world(q, r);
        // The plate the tile stands in was found by the layer beneath
        // that put it there: read back, never found again.
        let stood = below.stand.as_ref().map(|stand| sky.outlines.stood(wx, wy, stand).expect("the sky's cell holds the plates its tiles stand in"));
        Some(TileOutput { moisture: Some(sky_standing(wx, wy, sky.wind, &sky.coasts, &sky.outlines, stood.as_ref())), ..TileOutput::default() })
    }
}
