//! ThickeningEvent — crustal thickening of a plate, driven by its edges: the
//! plateau.
//!
//! # Claims
//!
//! Convergent shortening thickens the crust, and the thickened root floats up
//! as a plateau. That is isostasy, and isostasy does not verge. Thickening is
//! the plate's, driven by its edges: every edge of a plate carries a height,
//! the plateau height its convergence buys where the plate overrides on it
//! and nothing anywhere else. The crust thickens by stacking, so from a front
//! the plateau rises one sheet's share at a time, from nothing at the front
//! to the edge's height at the last sheet's backlimb, the rim: a thrust
//! wedge's taper, the surface climbing a few degrees from foreland to
//! hinterland with the ranges standing on it. Inside the plate the surface
//! is the blend of its convergent edges' heights by nearness, so a plate
//! with one strong margin holds that margin's height across its interior and
//! a plate with two margins slopes between them, and it runs down to nothing
//! over an escarpment two sheets wide at every edge the plate does not
//! override on. Two plates meet at nothing across a quiet edge and the
//! overriding side of a front climbs from nothing, so nothing steps. A plate
//! that overrides nowhere stays at the substrate. The plateau stands to a
//! range's relief as Tibet stands to the Himalaya above it.
//!
//! An oceanic plate carries no plateau: thickened oceanic crust makes no
//! surface, and an island arc is volcanism above the slab, a chain of
//! separate edifices and never a thin belt. Unbuilt. A belt scaled down to
//! oceanic crust breached the sea only where a crest crossed a continent's
//! shelf, the same oval off every coast.
//!
//! # The window
//!
//! No deform: nothing originates here. `prepare` gathers the resolved edges
//! of the graph's cells under the cell and its ring into outlines, one per
//! plate, and a query finds the plate its tile stands in and reads that
//! outline. The cell is the graph's, sized so one ring holds the whole
//! outline of any plate a tile in the cell stands in.

use std::any::Any;

use crate::hex_to_world;
use super::index::IndexRegistry;
use super::plates::GRAPH_CELL_SCALE;
use super::thrusting::{outlines_of, sheets_in, sheets_of, smoothstep, Outlines, PlateOutline, RANGE_RISE, RANGE_SPACING};
use super::{CellScope, TileOutput, TileView, WorldEvent};

// ── The plateau ─────────────────────────────────────────────────────────────

/// Height of a plateau against the relief of the ranges on its margin: Tibet
/// stands 5 km above the sea and the Himalaya 3.8 km above Tibet.
const PLATEAU_TO_RANGE: f64 = 5.0 / 3.8;

/// Elevation the plateau reaches behind a full wedge on continental crust: a
/// range's relief in Tibet's proportion. Across one sheet the plateau climbs
/// one sheet's share of this, well under a range, so a trough lies between
/// every two ranges by construction; a test holds it.
pub const PLATEAU_RISE: f64 = RANGE_RISE * PLATEAU_TO_RANGE;

/// Elevation a belt reaches at full convergence on continental crust: the
/// plateau plus a range standing on it.
pub const OROGEN_MAX_RISE: f64 = PLATEAU_RISE + RANGE_RISE;

/// Width of the escarpment a plateau ends in at an edge its plate does not
/// override on: two sheet spacings, the Drakensberg's few tens of kilometres
/// from the Karoo's rim to the coastal plain.
pub const ESCARPMENT: f64 = 2.0 * RANGE_SPACING;

// ── The plateau's profile ───────────────────────────────────────────────────

/// The plateau at a position as a share of its rise: the blend by nearness
/// of the convergent edges' heights of the plate the position stands in,
/// each climbing from nothing at its front to its height at its rim, run
/// down over the escarpment to nothing at the plate's other edges. Zero on
/// a plate that overrides nowhere, and on an oceanic plate, whose edges
/// carry no convergence.
pub fn plateau_share(outlines: &Outlines, wx: f64, wy: f64) -> f64 {
    let Some(at) = outlines.at(wx, wy) else { return 0.0 };
    plateau_share_of(at.plate, &at.distances)
}

/// [`plateau_share`] for the plate and distances `Outlines::at` found, so a
/// caller reading several layers at one position looks the plate up once.
pub fn plateau_share_of(plate: &PlateOutline, distances: &[f64]) -> f64 {
    let (mut weighted, mut weight) = (0.0, 0.0);
    for (e, d) in plate.edges.iter().zip(distances) {
        if e.converge <= 0.0 { continue }
        let sheets = sheets_of(e.converge);
        let climb = (sheets_in(*d) / sheets).min(1.0);
        // Nearness: an edge's own value wins at the edge, and the interior
        // is the blend of every margin's.
        let w = 1.0 / (d * d).max(1.0);
        weighted += w * e.converge * climb;
        weight += w;
    }
    if weight <= 0.0 { return 0.0 }
    let quiet = plate.quiet_distance(distances);
    (weighted / weight) * smoothstep(quiet / ESCARPMENT)
}

/// Elevation the thickening adds at a position, in z-levels, given the
/// outlines in reach.
pub fn thickening_on(wx: f64, wy: f64, outlines: &Outlines) -> f64 {
    PLATEAU_RISE * plateau_share(outlines, wx, wy)
}

// ── The event ───────────────────────────────────────────────────────────────

pub struct ThickeningEvent;

impl ThickeningEvent {
    pub fn new() -> Self { ThickeningEvent }
}

impl Default for ThickeningEvent {
    fn default() -> Self { Self::new() }
}

impl WorldEvent for ThickeningEvent {
    fn name(&self) -> &str { "thickening" }
    fn scale(&self) -> u32 { GRAPH_CELL_SCALE }

    /// Nothing originates here.
    fn max_influence(&self) -> u32 { 0 }

    fn register_indexes(&self, _registry: &mut IndexRegistry) {}

    /// Nothing to place: the plateau is read off the plate graph.
    fn deform(&self, _scope: &CellScope) {}

    fn prepare(&self, scope: &CellScope) -> Box<dyn Any + Send + Sync> {
        Box::new(outlines_of(scope))
    }

    fn query(
        &self,
        q: i32, r: i32,
        _below: &TileView,
        cell: &(dyn Any + Send + Sync),
        _seed: u64,
    ) -> Option<TileOutput> {
        let outlines = cell.downcast_ref::<Outlines>()?;
        let (wx, wy) = hex_to_world(q, r);
        let rise = thickening_on(wx, wy, outlines);
        if rise <= 0.0 { return None }
        Some(TileOutput { elevation_delta: rise, ..TileOutput::default() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::thrusting::WEDGE_SHEETS;

    const S: u64 = 0x9E3779B97F4A7C15;

    /// Across one sheet the plateau climbs one sheet's share of its height,
    /// and a range stands taller than that, or there is no trough between
    /// ranges on the taper.
    #[test]
    fn plateau_holds_a_trough_between_ranges() {
        let climb = PLATEAU_RISE / WEDGE_SHEETS;
        assert!(climb < RANGE_RISE, "plateau climbs {climb} z per sheet against a range of {RANGE_RISE} z");
    }

    /// The plateau share is bounded, nothing on an oceanic plate or a plate
    /// that overrides nowhere, never above the plate's strongest edge, and
    /// something on a continental plate that overrides somewhere.
    #[test]
    fn plateau_share_is_the_plates() {
        let outlines = Outlines::in_box(0.0, 0.0, 30_000.0, S);
        let (mut raised, mut quiet, mut oceanic) = (0, 0, 0);
        for i in 0..120 {
            for j in 0..120 {
                let (x, y) = (i as f64 * 500.0 - 30_000.0, j as f64 * 500.0 - 30_000.0);
                let share = plateau_share(&outlines, x, y);
                assert!((0.0..=1.0).contains(&share), "share {share} at ({x}, {y})");
                let Some(at) = outlines.at(x, y) else { continue };
                let plate = at.plate;
                let strongest = plate.edges.iter().map(|e| e.converge).fold(0.0, f64::max);
                assert!(share <= strongest + 1e-12, "share {share} past the strongest edge at ({x}, {y})");
                if !plate.continental { assert_eq!(share, 0.0); oceanic += 1 }
                else if strongest <= 0.0 { assert_eq!(share, 0.0); quiet += 1 }
                else if share > 0.0 { raised += 1 }
            }
        }
        assert!(quiet > 100 && raised > 100 && oceanic > 100, "{quiet} quiet, {raised} raised, {oceanic} oceanic samples");
    }
}
