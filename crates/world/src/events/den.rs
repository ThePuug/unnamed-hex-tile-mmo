//! DenEvent — where creatures make their dens: one site on each cell of a
//! jittered lattice, about forty seconds' walk apart, and the habitat its
//! ground gives it.
//!
//! # Claims
//!
//! A site is its lattice cell's centre jittered by the cell's hash, and the
//! cell its origin falls in publishes it. Its habitat is read once, at the
//! origin, from what the layers below published, never from a tile: within
//! reach of a river's channel; inside a forest stand, woods where trees
//! fill most of it and scrub where brush does; on a range's raised ground;
//! on rock harder than limestone; else open land. Where several hold, the
//! first of those does. A site off the land publishes nothing.
//!
//! The layer places nothing in a tile. What stands at a site, and exactly
//! where, is the server's to decide once the ground there is materialized.

use std::collections::HashMap;

use common::{den::Habitat, HexLattice};

use crate::noise::hash_channel_f64;
use crate::{hex_to_world, world_to_hex};
use super::forest::{axes_of, coasts_of, Reach, StandIndex};
use super::index::{CellId, CellIndex, EventIndex, IndexRegistry};
use super::lithology::rock_on;
use super::plates::GRAPH_CELL_SCALE;
use super::thrusting::{OutlineIndex, Outlines, RANGE_RISE};
use super::{CellScope, TileOutput, TileView, WorldEvent};

const DEN_SEED: u64 = 0x6465_6e73;

/// The radius of the lattice sites stand on: centres about 2r + 1, some
/// 173 tiles, apart, forty seconds of walking.
pub const DEN_LATTICE: u32 = 86;

/// How far a site strays from its cell's centre, as a share of the spacing.
pub const DEN_JITTER: f64 = 0.3;

/// Tiles beyond a channel's flow line a site still counts as beside it.
pub const RIVER_REACH: f64 = 12.0;

/// The stand density under which a site is not inside a stand.
pub const STAND_DENSITY: f64 = 0.15;

/// The share of a stand's filling that is trees from which it is woods.
pub const WOODS_TREES: f64 = 0.5;

/// The share of a range's full rise from which ground counts as a range.
pub const RANGE_SHARE: f64 = 0.25;

/// The erodibility under which rock counts as hard: harder than limestone.
pub const HARD_ROCK: f64 = 0.45;

/// A den site: its origin's tile and its habitat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Den {
    pub q: i32,
    pub r: i32,
    pub habitat: Habitat,
}

#[derive(Clone, Debug, Default)]
pub struct DenCell {
    pub dens: Vec<Den>,
}

#[derive(Default)]
pub struct DenIndex {
    pub cells: HashMap<CellId, DenCell>,
}

impl CellIndex for DenIndex {
    type Cell = DenCell;

    fn set(&mut self, cell: CellId, entry: Self::Cell) {
        self.cells.insert(cell, entry);
    }

    fn get(&self, cell: CellId) -> Option<&Self::Cell> {
        self.cells.get(&cell)
    }
}

impl EventIndex for DenIndex {
    fn source_scale(&self) -> u32 { DEN_LATTICE }

    /// Each den at its origin's tile.
    fn tiles(&self, cell_ids: &[CellId]) -> Vec<(i32, i32)> {
        cell_ids.iter().filter_map(|id| self.cells.get(id)).flat_map(|c| c.dens.iter().map(|d| (d.q, d.r))).collect()
    }

    fn neighbors(&self, _q: i32, _r: i32) -> Vec<(i32, i32)> { Vec::new() }

    fn remove_cell(&mut self, cell_id: CellId) {
        self.cells.remove(&cell_id);
    }
}

/// The lattice den sites stand on.
pub fn den_lattice() -> HexLattice {
    HexLattice::new(DEN_LATTICE)
}

/// The origin a lattice cell puts its den site at: the cell's centre,
/// jittered by the cell's hash.
pub fn origin_of(lattice: &HexLattice, id: CellId, seed: u64) -> (f64, f64) {
    let (cq, cr) = lattice.cell_center(id);
    let (cx, cy) = hex_to_world(cq, cr);
    let swing = 2.0 * DEN_JITTER * (lattice.tiles_per_cell() as f64).sqrt();
    let h = |channel: u64| hash_channel_f64(id.0 as i64, id.1 as i64, seed ^ DEN_SEED, channel);
    (cx + (h(1) - 0.5) * swing, cy + (h(2) - 0.5) * swing)
}

/// A site's habitat from what holds at its origin, the first that does:
/// `river` within reach of a channel, the stand there as its densest and
/// the share of its filling that is trees, the share of a range's rise the
/// ground stands at, and whether its rock is hard.
pub fn habitat(river: bool, (density, trees): (f64, f64), range: f64, hard: bool) -> Habitat {
    if river {
        Habitat::River
    } else if density >= STAND_DENSITY && trees >= WOODS_TREES {
        Habitat::Woods
    } else if density >= STAND_DENSITY {
        Habitat::Scrub
    } else if range >= RANGE_SHARE {
        Habitat::Range
    } else if hard {
        Habitat::Rock
    } else {
        Habitat::Open
    }
}

/// The dens whose origins fall in a cell, each with its habitat read at
/// its origin.
fn dens_of(scope: &CellScope) -> Vec<Den> {
    let seed = scope.seed();
    let lattice = scope.lattice();
    let cell = scope.cell();

    // Every index is deformed under the footprint before any guard is held
    let axes = axes_of(scope);
    scope.source_cells::<OutlineIndex>();
    scope.source_cells::<StandIndex>();
    let coasts = coasts_of(scope);
    let outlines = scope.read::<OutlineIndex>();
    let stands = Reach::new(scope.read::<StandIndex>().iter().flat_map(|idx| idx.entries().flat_map(|c| c.stands.clone()).collect::<Vec<_>>()));
    let graph = HexLattice::new(GRAPH_CELL_SCALE);

    let mut out = Vec::new();
    for id in std::iter::once(cell).chain(lattice.neighbor_cells(cell)) {
        let (ox, oy) = origin_of(lattice, id, seed);
        let (oq, or) = world_to_hex(ox, oy);
        if lattice.cell_id(oq, or) != cell || !coasts.shore(ox, oy).1 {
            continue;
        }
        let outline = outlines.as_ref()
            .and_then(|idx| idx.entry(graph.cell_id(oq, or)).cloned())
            .unwrap_or_else(|| std::sync::Arc::new(Outlines::new(&[], seed)));
        let river = axes.nearest(ox, oy, RIVER_REACH).is_some();
        let range = outline.relief(ox, oy) / RANGE_RISE;
        let hard = rock_on(ox, oy, seed, &coasts, &outline).erodibility <= HARD_ROCK;
        out.push(Den { q: oq, r: or, habitat: habitat(river, stands.at(ox, oy), range, hard) });
    }
    out
}

pub struct DenEvent;

impl DenEvent {
    pub fn new() -> Self { DenEvent }
}

impl Default for DenEvent {
    fn default() -> Self { Self::new() }
}

impl WorldEvent for DenEvent {
    fn name(&self) -> &str { "den" }
    fn scale(&self) -> u32 { DEN_LATTICE }

    /// A den is a place, not a tile's content: the layer stands out of the
    /// tile cascade, and its sites are found when the server reads them
    fn shapes_tiles(&self) -> bool { false }

    fn register_indexes(&self, registry: &mut IndexRegistry) {
        registry.pre_register::<DenIndex>();
    }

    fn deform(&self, scope: &CellScope) {
        scope.publish::<DenIndex>(DenCell { dens: dens_of(scope) });
    }

    /// Never asked: the layer shapes no tile
    fn query(&self, _q: i32, _r: i32, _below: &TileView, _cell: &(dyn std::any::Any + Send + Sync), _seed: u64) -> Option<TileOutput> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_most_specific_habitat_holds() {
        assert_eq!(habitat(true, (0.9, 0.9), 1.0, true), Habitat::River, "a river beats every other");
        assert_eq!(habitat(false, (0.9, 0.9), 1.0, true), Habitat::Woods);
        assert_eq!(habitat(false, (0.9, 0.1), 1.0, true), Habitat::Scrub, "a stand mostly brush is scrub");
        assert_eq!(habitat(false, (0.0, 0.0), 1.0, true), Habitat::Range);
        assert_eq!(habitat(false, (0.0, 0.0), 0.0, true), Habitat::Rock);
        assert_eq!(habitat(false, (0.0, 0.0), 0.0, false), Habitat::Open);
    }

    #[test]
    fn each_cell_puts_its_site_near_its_centre() {
        let lattice = den_lattice();
        let spacing = (lattice.tiles_per_cell() as f64).sqrt();
        for id in [(0, 0), (3, -2), (-5, 7)] {
            let (cq, cr) = lattice.cell_center(id);
            let (cx, cy) = hex_to_world(cq, cr);
            let (ox, oy) = origin_of(&lattice, id, 7);
            assert!((ox - cx).hypot(oy - cy) <= DEN_JITTER * spacing * std::f64::consts::SQRT_2);
            assert_eq!(origin_of(&lattice, id, 7), (ox, oy), "a site is its cell's hash");
        }
    }
}
