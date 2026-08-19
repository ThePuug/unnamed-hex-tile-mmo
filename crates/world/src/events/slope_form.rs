//! SlopeFormEvent — the finishing stage of the erosional stack.

//! Reads the composed surface every layer below has built and returns the
//! correction that gives its slopes the profile a hillslope settles into:
//! convex at the crest where creep dominates, concave at the base, bounded in
//! angle, with talus at the foot of anything steeper than that bound. The
//! sub-primitives live in [`crate::slope_form`]; this is the layer that feeds
//! them the surface.

//! **Horizontal, in a cascade that is vertical.** Every other event resolves a
//! tile from its own position. A diffusion kernel reads a neighbourhood, so
//! this query calls `below()` across a bounded hex ball. That is only affordable
//! because the framework deforms the cell a `below()` call lands in before
//! querying it, so a tap on the far side of a cell boundary caches like any
//! other tap instead of recomputing the whole cascade on every access.

//! **Order-independent.** No index, no survey, no deform. The output at a tile
//! is a pure function of the surface below over a bounded ball, so it does not
//! matter which tiles were visited first or in what order.

use std::sync::Arc;



use common::HexLattice;


use crate::slope_form::{critical_slope, repose_slope, MASS_WASTING_REACH, SLOPE_FORM_REACH};
use super::index::{CellId, IndexRegistry};

use super::faces::{BasinIndex, ErosionalFaceIndex};
use super::spines::SPINE_CELL_SCALE;
use super::{Survey, TileOutput, TileView, WorldEvent};

/// Cell scale in tiles.
///
/// Every layer here is scoped so a tile resolves from its own cell plus one
/// ring, and this one holds to that too: the kernel reaches
/// [`SLOPE_FORM_REACH`] tiles, well inside a cell of this size, so no tap can
/// leave the envelope the framework has already deformed around the tile.
///
/// Slope form does not strictly need it — it reads no index of its own, and
/// `below()` deforms the cell each tap lands in. Sizing it to the convention
/// anyway is what keeps the envelope derivable from cell scales rather than
/// from predicate reach, and leaves the layer correctly scoped if it ever
/// gains an index. Matched to the chunk the server streams, which clears the
/// floor comfortably and lets a chunk's tiles share one cell.
const SLOPE_FORM_CELL_SCALE: u32 = 9;

const _: () = assert!(SLOPE_FORM_REACH <= SLOPE_FORM_CELL_SCALE as f64);

/// What a tile in a cell reads: the published geometry standing over it,
/// taken once for the cell.
struct SlopeFormCell {
    faces: Vec<Arc<crate::faces::FaceIndex>>,
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


/// Multiply-shift hasher for packed tile keys. The kernel takes tens of lookups
/// per tile and a general-purpose hash is most of their cost — a packed key is
/// already well spread by one multiply against a 64-bit odd constant, with the
/// fold putting entropy in the low bits the table indexes on.
#[derive(Default)]
pub struct TileHasher(u64);

impl std::hash::Hasher for TileHasher {
    fn finish(&self) -> u64 {
        self.0 ^ (self.0 >> 32)
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ b as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        }
    }
    fn write_i64(&mut self, v: i64) {
        self.0 = (v as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }
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

    fn deform(
        &self,
        _cell_id: CellId,
        _matched: &[(i32, i32)],
        _indexes: &IndexRegistry,
        _seed: u64,
    ) {
        // Nothing to build. The stage is a pure function of the surface below.
    }

    /// The faces and basins standing over this cell, taken once for it.
    fn prepare(
        &self,
        cell_id: CellId,
        indexes: &IndexRegistry,
        _seed: u64,
    ) -> Box<dyn std::any::Any + Send + Sync> {
        // A slope-form cell is far smaller than a spine cell, so every tile in
        // it shares one spine cell — and the ring, since a spine reaches past
        // its own cell edge.
        //
        // A cell id means nothing outside the lattice that issued it, so the
        // hop between them goes through tile coordinates: this cell's centre,
        // then the spine cell that contains it.
        let (cq, cr) = self.lattice.cell_center(cell_id);
        let spine_cell = self.spine_lattice.cell_id(cq, cr);
        let cells = self.spine_lattice.cells_within_distance(spine_cell, 1);
        Box::new(SlopeFormCell {
            faces: indexes.get::<ErosionalFaceIndex>().map(|ix| {
                cells.iter().filter_map(|id| ix.cells.get(id).cloned()).collect()
            }).unwrap_or_default(),
            basins: indexes.get::<BasinIndex>().map(|ix| {
                cells.iter().filter_map(|id| ix.cells.get(id).cloned()).collect()
            }).unwrap_or_default(),
        })
    }

    fn query(
        &self,
        q: i32, r: i32,
        below: &TileView,
        cell: &(dyn std::any::Any + Send + Sync),
        _seed: u64,
    ) -> Option<TileOutput> {
        let cell = cell.downcast_ref::<SlopeFormCell>()?;
        let (wx, wy) = (below.wx, below.wy);
        let base = below.elevation;
        let _ = (q, r);

        // Failure and deposition both read the published faces: the only ground
        // within reach that can sit far below a tile is ground a carve took
        // away, and every carve published its floor.
        //
        // Creep is absent. It is an average over a neighbourhood, which is the
        // one thing this contract does not hand out, and its replacement is the
        // composition itself — a crest is sharp because the surface is a hard
        // max over cones, and softening that seam is what rounds it. Until then
        // crests keep the curvature the tectonic layer gave them.
        let repose = repose_slope();
        let critical = critical_slope(wx, wy);
        let cap = repose * MASS_WASTING_REACH;

        let mut limited = base;
        let mut apron = 0.0f64;
        for faces in &cell.faces {
            if let Some(l) = faces.limit_at(wx, wy, critical) {
                limited = limited.min(l);
            }
            let a = faces.apron_at(wx, wy, repose, cap);
            if a > apron { apron = a; }
        }

        let delta = (limited - base) + apron;
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
