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

use dashmap::DashMap;

use common::HexLattice;

use crate::hex_to_world;
use crate::slope_form::{Neighbourhood, repose_slope, MASS_WASTING_REACH, SLOPE_FORM_REACH};
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

pub struct SlopeFormEvent {
    /// Spine lattice, hoisted out of the per-tile query. The impounding level
    /// lives on the spine instances, and they have to be looked up in the same
    /// cells the spine layer's own query uses or a bowl resolves against a
    /// different mountain than its elevation came from.
    spine_lattice: HexLattice,
    /// The surface below at a tile centre.
    ///
    /// A kernel takes O(radius²) taps per tile and adjacent tiles' kernels
    /// overlap almost entirely, so every tap is shared work. `below()` resolves
    /// a tile through every layer under this one and rebuilds its tag set on
    /// the way; this holds the one number the kernel wants.
    ///
    /// Event-private and invisible to `Composite`, which caches whole tiles
    /// above it. Entries are a pure function of position and the layers below,
    /// which are immutable once deformed, so this changes how fast a tile
    /// resolves and never what it resolves to.
    below_memo: TileMap,
    /// Publishing-layer cells whose geometry can act on a tile, once per cell.
    reach: DashMap<CellId, Arc<Vec<CellId>>>,
}

type TileMap = DashMap<i64, f64, std::hash::BuildHasherDefault<TileHasher>>;

/// Pack a tile into one key. Two i32s side by side hash in a single multiply.
fn tile_key(q: i32, r: i32) -> i64 {
    ((q as i64) << 32) | (r as u32 as i64)
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
            spine_lattice: HexLattice::new(SPINE_CELL_SCALE),
            below_memo: TileMap::default(),
            reach: DashMap::new(),
        }
    }

    /// The surface below at a tile centre, through the memo.
    fn below_at(&self, q: i32, r: i32, below: &dyn Fn(i32, i32) -> TileView) -> f64 {
        if let Some(v) = self.below_memo.get(&tile_key(q, r)) { return *v; }
        let v = below(q, r).elevation;
        self.below_memo.insert(tile_key(q, r), v);
        v
    }

    /// The cells whose published geometry can act on a tile — its own cell in
    /// the publishing layer's lattice plus the ring, the same envelope that
    /// layer's own query resolves against, so a face is never read against a
    /// different mountain than the elevation under it came from.
    ///
    /// Resolved once per cell. Every tile in a cell shares the answer, and
    /// rebuilding it per tile costs a ring walk and an allocation on the hot
    /// path — which is the whole of what made the first version of this layer
    /// expensive.
    fn cells_for(&self, q: i32, r: i32) -> Arc<Vec<CellId>> {
        let cell = self.spine_lattice.cell_id(q, r);
        if let Some(v) = self.reach.get(&cell) { return v.clone(); }
        let cells = Arc::new(self.spine_lattice.cells_within_distance(cell, 1));
        self.reach.insert(cell, cells.clone());
        cells
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

    fn query(
        &self,
        q: i32, r: i32,
        _cell_id: CellId,
        indexes: &IndexRegistry,
        below: &dyn Fn(i32, i32) -> TileView,
        _seed: u64,
    ) -> Option<TileOutput> {
        let (wx, wy) = hex_to_world(q, r);
        let cells = self.cells_for(q, r);

        let hood = Neighbourhood::gather(q, r, wx, wy, &|tq, tr| self.below_at(tq, tr, below));
        let base = hood.centre();

        // Deposition reads the faces the layers below published. Creep and
        // failure still read the neighbourhood: creep is an average over one,
        // and the published faces do not yet reproduce what the limiter cuts —
        // driving it from them opens 42 basins in 1281, so it stays on the
        // surface until the faces answer as the composite does.
        let delta = (hood.creep() - base)
            + (hood.failure() - base)
            + indexes.get::<ErosionalFaceIndex>().map_or(0.0, |ix| {
                let repose = repose_slope();
                ix.apron_in(&cells, wx, wy, repose, repose * MASS_WASTING_REACH)
            });
        if delta == 0.0 { return None; }
        // Slope form may not cut below the level that impounds a basin — the
        // same rule the water layer obeys. Without it a smoothed rim drops
        // under its own spill altitude and drains the tarn behind it. Held
        // under the surface below as well, so the clamp can only stop a cut,
        // never raise ground the layers below deliberately took away.
        let floor = indexes
            .get::<BasinIndex>()
            .and_then(|ix| ix.impound_in(&cells, wx, wy))
            .map_or(f64::MIN, |f| f.min(base));

        let mut out = TileOutput::default();
        out.elevation_delta = (base + delta).max(floor) - base;
        Some(out)
    }
}
