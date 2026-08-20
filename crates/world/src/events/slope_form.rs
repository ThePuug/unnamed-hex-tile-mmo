//! SlopeFormEvent — the finishing stage of the erosional stack.

//! Reads the composed surface every layer below has built and returns the
//! correction that gives its slopes the profile a hillslope settles into:
//! convex at the crest where creep dominates, concave at the base, bounded in
//! angle, with talus at the foot of anything steeper than that bound. The
//! sub-primitives live in [`crate::slope_form`]; this is the layer that feeds
//! them the surface.

//! **A tile resolves from its own position.** The layers below publish the
//! geometry they cut — every face steep enough to fail, every bowl deep enough
//! to hold water — so this stage never reads a neighbouring tile. It reads
//! what stands over the one it is given.

//! **Order-independent.** It builds no index and runs no survey; `prepare`
//! reads what the layers below published over a neighbourhood the framework
//! guarantees is deformed, so the answer at a tile does not depend on which
//! tiles were visited first.

use std::collections::HashSet;
use std::sync::Arc;



use common::HexLattice;


use crate::slope_form::{
    creep_delta, critical_slope, repose_slope, MASS_WASTING_REACH, SLOPE_FORM_REACH,
};

use super::faces::{BasinIndex, ErosionalFaceIndex};
use super::spines::{SpineInstanceIndex, SPINE_CELL_SCALE};
use super::{CellScope, Survey, TileOutput, TileView, WorldEvent};

/// Cell scale in tiles.
///
/// Sets what one `prepare` covers, and with it where the cost of settling
/// lands. Too small and a lone summary sample pays for a cell it uses one tile
/// of; too large and the settle sweeps ground no tile of the cell can reach.
/// Matched to the chunk the server streams, so a chunk's tiles share one cell
/// and the whole settle is charged once across all 271 of them.
///
/// It also clears [`SLOPE_FORM_REACH`] comfortably, which keeps the layer
/// correctly scoped against the convention every other event here follows.
const SLOPE_FORM_CELL_SCALE: u32 = 9;

const _: () = assert!(SLOPE_FORM_REACH <= SLOPE_FORM_CELL_SCALE as f64);

/// World-unit radius of the ground a cell settles geometry for: its own
/// footprint plus everything close enough to act on a tile inside it.
///
/// A hex ball of N tiles has circumradius N — its six corners sit exactly N
/// tile spacings out — so the cell scale is the footprint radius directly.
const CELL_FOOTPRINT_RADIUS: f64 =
    (SLOPE_FORM_CELL_SCALE as f64) * crate::TILE_SPACING + MASS_WASTING_REACH;

/// What a tile in a cell reads: the published geometry standing over it,
/// taken once for the cell.
struct SlopeFormCell {
    /// Settled against every spine that reaches each foot.
    faces: crate::faces::FaceIndex,
    basins: Vec<Arc<common::HexSpatialGrid<crate::Cirque>>>,
}

pub struct SlopeFormEvent {
    /// This layer's own cell lattice, for turning a cell id back into the tile
    /// coordinates every other lattice is addressed by.
    lattice: HexLattice,
    /// The lattice the published geometry is keyed by. A face has to be looked
    /// up in the same cells the layer that published it resolves against, or a
    /// bowl answers against a different mountain than the elevation under it.
    spine_lattice: HexLattice,
}


impl SlopeFormEvent {
    pub fn new() -> Self {
        Self {
            lattice: HexLattice::new(SLOPE_FORM_CELL_SCALE),
            spine_lattice: HexLattice::new(SPINE_CELL_SCALE),
        }
    }

}

impl Default for SlopeFormEvent {
    fn default() -> Self { Self::new() }
}

impl WorldEvent for SlopeFormEvent {
    fn name(&self) -> &str { "slope-form" }
    fn scale(&self) -> u32 { SLOPE_FORM_CELL_SCALE }
    fn survey(&self) -> Survey { Survey::none() }

    /// Reads no index of its own — the neighbourhood it gathers is the surface
    /// below, which `below()` resolves and the framework deforms for it.
    fn query_reach(&self) -> u32 { 0 }

    fn deform(&self, _scope: &CellScope, _matched: &[(i32, i32)]) {
        // Nothing to build. The stage is a pure function of the surface below.
    }

    /// The geometry standing over this cell, settled once for it.
    ///
    /// A producer publishes a face with the floor its own carve leaves, because
    /// `deform` sees the layers below over its footprint and never its own ring
    /// — it cannot know that a neighbouring spine buries the channel it just
    /// cut. Compositing that is a fold over the ring, which is precisely what
    /// this phase is allowed to read: `query_reach` guarantees the ring is
    /// deformed before any tile here resolves.
    ///
    /// So the reader settles. Every face that can act on a tile of this cell is
    /// re-floored against every spine reaching its foot, once, and the result
    /// is what the per-tile query reads.
    fn prepare(&self, scope: &CellScope) -> Box<dyn std::any::Any + Send + Sync> {
        // A cell id means nothing outside the lattice that issued it, so the
        // hop between them goes through tile coordinates.
        let (cq, cr) = self.lattice.cell_center(scope.cell());
        let spine_cell = self.spine_lattice.cell_id(cq, cr);
        let cells = self.spine_lattice.cells_within_distance(spine_cell, 1);
        let raw: Vec<Arc<crate::faces::FaceIndex>> = scope
            .read::<ErosionalFaceIndex>()
            .map(|ix| cells.iter().filter_map(|id| ix.cells.get(id).cloned()).collect())
            .unwrap_or_default();
        let basins = scope
            .read::<BasinIndex>()
            .map(|ix| cells.iter().filter_map(|id| ix.cells.get(id).cloned()).collect())
            .unwrap_or_default();

        // Gather before settling. Most ground carries no carve at all, and the
        // instances exist here only to re-floor one — reading them first would
        // charge every empty cell for a fold it never performs.
        let (cx, cy) = crate::hex_to_world(cq, cr);
        let mut gathered: Vec<crate::ErosionalFace> = Vec::new();
        let mut seen: HashSet<(u64, u64)> = HashSet::new();
        for producer in &raw {
            producer.for_each_in(cx, cy, CELL_FOOTPRINT_RADIUS, |face| {
                if seen.insert((face.wx.to_bits(), face.wy.to_bits())) {
                    gathered.push(*face);
                }
            });
        }

        let reach = MASS_WASTING_REACH;
        let mut faces = crate::faces::FaceIndex::new(reach);
        if !gathered.is_empty() {
            let instances = scope
                .read::<SpineInstanceIndex>()
                .map(|ix| ix.instances_in(&cells))
                .unwrap_or_default();
            let min_height = crate::ELEVATION_PER_Z / crate::TILE_SPACING * reach;
            for face in gathered {
                let top = face.floor + face.height;
                let floor = instances
                    .iter()
                    .fold(0.0f64, |acc, i| acc.max(i.sample_at(face.wx, face.wy).elevation));
                faces.insert(
                    crate::ErosionalFace { floor, height: top - floor, ..face },
                    min_height,
                );
            }
        }

        Box::new(SlopeFormCell { faces, basins })
    }

    fn query(
        &self,
        _q: i32, _r: i32,
        below: &TileView,
        cell: &(dyn std::any::Any + Send + Sync),
        _seed: u64,
    ) -> Option<TileOutput> {
        let cell = cell.downcast_ref::<SlopeFormCell>()?;
        let (wx, wy) = (below.wx, below.wy);
        let base = below.elevation;

        // Failure and deposition both read the published faces: the only ground
        // within reach that can sit far below a tile is ground a carve took
        // away, and every carve published its floor. Creep reads the curvature
        // the layers below state at this tile. None of the three reads a
        // neighbour.
        let repose = repose_slope();
        let critical = critical_slope(wx, wy);
        let cap = repose * MASS_WASTING_REACH;

        let limited = cell.faces.limit_at(wx, wy, critical).map_or(base, |l| base.min(l));
        let apron = cell.faces.apron_at(wx, wy, repose, cap);
        let creep = creep_delta(below.curvature, wx, wy);
        let delta = (limited - base) + apron + creep;
        if delta == 0.0 { return None; }

        // Slope form may not cut below the level that impounds a basin — the
        // same rule the water layer obeys. Without it a smoothed rim drops
        // under its own spill altitude and drains the tarn behind it. Held
        // under the surface below as well, so the clamp can only stop a cut,
        // never raise ground the layers below deliberately took away.
        let floor = cell
            .basins
            .iter()
            .filter_map(|g| {
                g.query(wx, wy)
                    .filter_map(|c| c.base_level(wx, wy))
                    .fold(None, |acc: Option<f64>, l| Some(acc.map_or(l, |a: f64| a.min(l))))
            })
            .fold(None, |acc: Option<f64>, l| Some(acc.map_or(l, |a: f64| a.max(l))))
            .map_or(f64::MIN, |f| f.min(base));

        let mut out = TileOutput::default();
        out.elevation_delta = (base + delta).max(floor) - base;
        Some(out)
    }
}
