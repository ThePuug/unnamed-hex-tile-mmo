//! GroundEvent — the ground beneath drainage, at every node of its lattice.
//!
//! # Claims
//!
//! Drainage routes over the ground the layers beneath it leave: the field
//! layers — substrate, tilt, thrusting, thickening, lithology — and the
//! scarps carved into them, summed as the functions they are by
//! [`ground_at`]. Evaluating that sum at a position reads the whole outline
//! of the plate the position stands in, and a scarp's carve there reads the
//! ground at its site in turn, so a node's ground needs the plate graph
//! further out than drainage's own cell and ring reach. This layer
//! evaluates it where that is in reach: each cell evaluates the nodes whose
//! sites lie in it and publishes them, each with the ground there, its
//! plate's age and the erodibility of its rock. Drainage reads them and
//! evaluates no ground of its own.
//!
//! A layer added beneath drainage either stays a field or publishes what
//! drainage needs, and is added to [`ground_at`], or drainage routes over
//! ground that is not there. `drainage_probe` checks the sum against the
//! composite.
//!
//! # The window
//!
//! The cell reaches as far as a node's ground reads: a plate's whole
//! outline past the site of any scarp whose footprint holds the node. No
//! query: the layer puts nothing on the ground.

use std::any::Any;
use std::collections::HashMap;

use crate::hex_to_world;
use crate::lattice::{node_site, NodeKey, NODE_SPACING};
use super::drainage::{ground_at, Ground};
use super::index::{CellId, CellIndex, IndexRegistry, Located};
use super::plates::coasts_of;
use super::scarp::{self, Scarps};
use super::thrusting::outlines_of;
use super::{CellScope, TileOutput, TileView, WorldEvent, RING_CLEARANCE};

/// The ground beneath drainage at one node: its key, where its site lies,
/// and the ground there.
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub key: NodeKey,
    pub wx: f64,
    pub wy: f64,
    pub ground: Ground,
}

/// A sample lies at its node's site.
impl Located for Sample {
    fn bounds(&self) -> [f64; 4] {
        [self.wx, self.wy, self.wx, self.wy]
    }
}

/// The samples each cell evaluated: those of the nodes whose sites lie in
/// it.
#[derive(Default)]
pub struct GroundIndex {
    cells: HashMap<CellId, Vec<Sample>>,
}

impl CellIndex for GroundIndex {
    type Cell = Vec<Sample>;
    type Feature = Sample;

    fn set(&mut self, cell: CellId, entry: Self::Cell) {
        self.cells.insert(cell, entry);
    }

    fn get(&self, cell: CellId) -> Option<&Self::Cell> {
        self.cells.get(&cell)
    }

    fn features(entry: &Self::Cell) -> impl Iterator<Item = &Sample> {
        entry.iter()
    }
}

/// How far a node's ground reads from its site: as far as a scarp's carve
/// there reads, which holds the outline of the plate the node stands in.
pub fn ground_reach() -> f64 {
    scarp::read_reach()
}

#[derive(Default)]
pub struct GroundEvent;

impl GroundEvent {
    pub fn new() -> Self {
        GroundEvent
    }
}

impl WorldEvent for GroundEvent {
    fn name(&self) -> &str { "ground" }

    /// One ring holds everything a node of the cell reads.
    fn scale(&self) -> u32 {
        (ground_reach() / RING_CLEARANCE).ceil() as u32 + 1
    }

    fn max_influence(&self) -> u32 {
        ground_reach().ceil() as u32
    }

    fn register_indexes(&self, registry: &mut IndexRegistry) {
        registry.pre_register::<GroundIndex>();
    }

    /// The ground at every node whose site lies in the cell, in key order.
    fn deform(&self, scope: &CellScope) {
        let (lattice, cell, seed) = (scope.lattice(), scope.cell(), scope.seed());
        let coasts = coasts_of(scope);
        let outlines = outlines_of(scope);
        let scarps = Scarps::new();
        let (cq, cr) = lattice.cell_center(cell);
        let reach = lattice.radius as i32 + NODE_SPACING;
        let s = NODE_SPACING;
        let (i0, i1) = ((cq - reach).div_euclid(s), (cq + reach).div_euclid(s) + 1);
        let (j0, j1) = ((cr - reach).div_euclid(s), (cr + reach).div_euclid(s) + 1);
        let mut samples = Vec::new();
        for i in i0..=i1 {
            for j in j0..=j1 {
                let (q, r) = node_site((i, j));
                if lattice.cell_id(q, r) != cell {
                    continue;
                }
                let (wx, wy) = hex_to_world(q, r);
                samples.push(Sample { key: (i, j), wx, wy, ground: ground_at(wx, wy, seed, &coasts, &outlines, &scarps) });
            }
        }
        scope.publish::<GroundIndex>(samples);
    }

    /// Nothing per tile: drainage reads the samples.
    fn query(&self, _q: i32, _r: i32, _below: &TileView, _cell: &(dyn Any + Send + Sync), _seed: u64) -> Option<TileOutput> {
        None
    }
}
