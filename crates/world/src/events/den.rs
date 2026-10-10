//! DenEvent — where creatures make their dens: one site on each cell of a
//! jittered lattice, about forty seconds' walk apart, and the habitat its
//! ground gives it.
//!
//! # Claims
//!
//! A site is a pure function of its lattice cell's hash: the cell's centre
//! jittered. A cell finds once the sites of its own lattice cell and the
//! six around it, as far as a site strays, a tile whether it is one of
//! them, and nothing is published. A site reads its
//! habitat from its own tile, as the layers beneath compose it, and from
//! nothing else: under water or on a valley's floor, where rivers run;
//! under trees in most of its growth sites; under brush in most of them; on
//! ground near repose; on rock harder than limestone; else open land. Where
//! several hold, the first of those does. A site under water keeps its den:
//! the server finds the dry ground nearest it to stand it on.
//!
//! The tile carries its den's habitat and nothing more. What stands at a
//! site, and exactly where, is the server's to decide on the ground it has
//! materialized round the site.

use common::{den::Habitat, Canopy, HexLattice, SITES};

use crate::lattice::jittered_centre;
use crate::world_to_hex;
use super::index::CellId;
use super::thrusting::REPOSE_GRADE;
use super::{CellScope, TileOutput, TileView, WorldEvent};

const DEN_SEED: u64 = 0x6465_6e73;

/// The radius of the lattice sites stand on: centres about 2r + 1, some
/// 173 tiles, apart, forty seconds of walking.
pub const DEN_LATTICE: u32 = 86;

/// How far a site strays from its cell's centre, as a share of the spacing.
pub const DEN_JITTER: f64 = 0.3;

/// How far up a valley's wall still counts as its floor, where rivers run.
pub const RIVER_FLOOR: f64 = 0.1;

/// The growth sites of a tile's three that trees or brush must hold for a
/// den there to be in woods or scrub.
pub const GROWN: u32 = 2;

/// The share of repose from which ground counts as a range's high ground.
pub const RANGE_GRADE: f64 = 0.5;

/// The erodibility under which rock counts as hard: harder than limestone.
pub const HARD_ROCK: f64 = 0.45;

/// The lattice den sites stand on.
pub fn den_lattice() -> HexLattice {
    HexLattice::new(DEN_LATTICE)
}

/// The tile a lattice cell puts its den site on: the cell's centre,
/// jittered by the cell's hash.
pub fn site_of(lattice: &HexLattice, id: CellId, seed: u64) -> (i32, i32) {
    let (x, y) = jittered_centre(lattice, id, DEN_JITTER, seed ^ DEN_SEED);
    world_to_hex(x, y)
}

/// The sites a cell's tiles may be: its own and the six around it, which is
/// as far as a site strays.
pub fn sites_near(lattice: &HexLattice, cell: CellId, seed: u64) -> [(i32, i32); 7] {
    let mut sites = [site_of(lattice, cell, seed); 7];
    for (slot, id) in sites.iter_mut().skip(1).zip(lattice.neighbor_cells(cell)) {
        *slot = site_of(lattice, id, seed);
    }
    sites
}

/// A site's habitat from its own tile, the first that holds.
pub fn habitat(tile: &TileView) -> Habitat {
    let [pine, deciduous, brush] = Canopy::tally(tile.cover);
    if tile.water.is_some() || tile.valley.is_some_and(|wall| wall <= RIVER_FLOOR) {
        Habitat::River
    } else if pine + deciduous >= GROWN {
        Habitat::Woods
    } else if brush >= GROWN {
        Habitat::Scrub
    } else if tile.grade() >= RANGE_GRADE * REPOSE_GRADE {
        Habitat::Range
    } else if tile.rock.is_some_and(|rock| rock.erodibility() <= HARD_ROCK) {
        Habitat::Rock
    } else {
        Habitat::Open
    }
}

pub struct DenEvent {
    lattice: HexLattice,
}

impl DenEvent {
    pub fn new() -> Self { DenEvent { lattice: den_lattice() } }
}

impl Default for DenEvent {
    fn default() -> Self { Self::new() }
}

impl WorldEvent for DenEvent {
    fn name(&self) -> &str { "den" }

    /// The site lattice's own: the seven sites `prepare` finds for a cell
    /// are the seven every tile in it may be only when the cell is the
    /// tile's own site lattice cell.
    fn scale(&self) -> u32 { self.lattice.radius }

    /// Nothing to place: a site is its lattice cell's hash.
    fn deform(&self, _scope: &CellScope) {}

    /// The sites the cell's tiles may be, found once for all of them
    fn prepare(&self, scope: &CellScope) -> Box<dyn std::any::Any + Send + Sync> {
        Box::new(sites_near(&self.lattice, scope.cell(), scope.seed()))
    }

    fn query(&self, q: i32, r: i32, below: &TileView, cell: &(dyn std::any::Any + Send + Sync), _seed: u64) -> Option<TileOutput> {
        let sites = cell.downcast_ref::<[(i32, i32); 7]>()?;
        if !sites.contains(&(q, r)) {
            return None;
        }
        Some(TileOutput { den: Some(habitat(below)), ..TileOutput::default() })
    }
}

const _: () = assert!(SITES.len() == 3, "GROWN counts of a tile's three growth sites");

#[cfg(test)]
mod tests {
    use super::*;
    use common::{Content, Cover, Rock};

    fn ground() -> TileView {
        let mut tile = TileView::at(0, 0);
        tile.rock = Some(Rock::Shale);
        tile
    }

    fn grown(content: Content, sites: usize) -> Cover {
        (0..sites).fold(Cover::NONE, |cover, k| cover.with(k, content))
    }

    #[test]
    fn a_site_takes_the_first_habitat_its_tile_holds() {
        let mut tile = ground();
        assert_eq!(habitat(&tile), Habitat::Open);
        tile.rock = Some(Rock::Basement);
        assert_eq!(habitat(&tile), Habitat::Rock, "hard rock");
        tile.gradient = (REPOSE_GRADE, 0.0);
        assert_eq!(habitat(&tile), Habitat::Range, "steep ground beats hard rock");
        tile.cover = grown(Content::Brush, 2);
        assert_eq!(habitat(&tile), Habitat::Scrub);
        tile.cover = grown(Content::Pine, 2);
        assert_eq!(habitat(&tile), Habitat::Woods);
        tile.valley = Some(0.0);
        assert_eq!(habitat(&tile), Habitat::River, "a valley floor beats every other");
        tile.valley = None;
        tile.water = Some(1.0);
        assert_eq!(habitat(&tile), Habitat::River, "under water, it dens on the dry ground nearest");
    }

    #[test]
    fn every_cell_has_one_site_and_the_cell_its_tile_lies_in_finds_it() {
        let lattice = den_lattice();
        for id in [(0, 0), (3, -2), (-5, 7)] {
            let (q, r) = site_of(&lattice, id, 7);
            assert!(sites_near(&lattice, lattice.cell_id(q, r), 7).contains(&(q, r)), "a site strays no further than the ring");
            assert_eq!(site_of(&lattice, id, 7), (q, r), "a site is its cell's hash");
        }
    }
}
