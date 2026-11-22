//! DenEvent — where creatures make their dens: a clump of sites on each
//! cell of a jittered lattice, the clumps about forty seconds' walk apart
//! with empty country between, and the habitat each site's ground gives
//! it.
//!
//! # Claims
//!
//! A clump is a pure function of its lattice cell's hash: the cell's
//! centre jittered, and a few sites set round it. A cell finds once the
//! sites of its own lattice cell and the six around it, as far as a site
//! strays, a tile whether it is one of them, and nothing is published. A
//! site reads its
//! habitat from its own tile, as the layers beneath compose it, and from
//! nothing else: under water; under trees in most of its growth sites; under brush in most of them; on
//! ground near repose; on rock harder than limestone; else open land. Where
//! several hold, the first of those does. A site under water keeps its den:
//! the server finds the dry ground nearest it to stand it on.
//!
//! The tile carries its den's habitat and nothing more. What stands at a
//! site, and exactly where, is the server's to decide on the ground it has
//! materialized round the site.

use common::{den::Habitat, Canopy, HexLattice, SITES};

use crate::lattice::jittered_centre;
use crate::noise::hash_channel_f64;
use crate::world_to_hex;
use super::index::CellId;
use super::thrusting::REPOSE_GRADE;
use super::{CellScope, TileOutput, TileView, WorldEvent};

const DEN_SEED: u64 = 0x6465_6e73;

/// The radius of the lattice clumps stand on: centres about 2r + 1, some
/// 173 tiles, apart, forty seconds of walking.
pub const DEN_LATTICE: u32 = 86;

/// How far a clump's centre strays from its cell's, as a share of the
/// spacing.
pub const DEN_JITTER: f64 = 0.3;

/// The fewest and the most sites to a clump.
pub const CLUMP: (usize, usize) = (2, 4);

/// How far from its clump's centre a site stands, nearest and furthest, in
/// tiles: far enough that the most sites, set evenly round, stand a den and
/// its cleared ground apart, and near enough that a fight at one is seen
/// from the next.
pub const CLUMP_REACH: (f64, f64) = (22.0, 28.0);

/// How far a site strays from its even place round the clump, in radians.
const CLUMP_SWAY: f64 = 0.1;

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

/// The tiles a lattice cell puts its clump's den sites on: set evenly
/// round the cell's centre, jittered, at a turn, each at its own reach and
/// a little off its place, all by the cell's hash.
pub fn sites_of(lattice: &HexLattice, id: CellId, seed: u64) -> impl Iterator<Item = (i32, i32)> {
    let seed = seed ^ DEN_SEED;
    let (x, y) = jittered_centre(lattice, id, DEN_JITTER, seed);
    // The centre's jitter takes the hash's first two channels.
    let h = move |channel: u64| hash_channel_f64(id.0 as i64, id.1 as i64, seed, channel);
    let count = CLUMP.0 + (h(3) * (CLUMP.1 - CLUMP.0 + 1) as f64) as usize;
    let turn = h(4) * std::f64::consts::TAU;
    (0..count).map(move |k| {
        let channel = 5 + 2 * k as u64;
        let angle = turn + std::f64::consts::TAU * k as f64 / count as f64 + (h(channel) - 0.5) * 2.0 * CLUMP_SWAY;
        let reach = CLUMP_REACH.0 + h(channel + 1) * (CLUMP_REACH.1 - CLUMP_REACH.0);
        world_to_hex(x + reach * angle.cos(), y + reach * angle.sin())
    })
}

/// The sites a cell's tiles may be: its own clump's and those of the six
/// around it, which is as far as a site strays.
pub fn sites_near(lattice: &HexLattice, cell: CellId, seed: u64) -> Vec<(i32, i32)> {
    std::iter::once(cell).chain(lattice.neighbor_cells(cell)).flat_map(|id| sites_of(lattice, id, seed)).collect()
}

/// A site's habitat from its own tile, the first that holds.
pub fn habitat(tile: &TileView) -> Habitat {
    let [pine, deciduous, brush] = Canopy::tally(tile.cover);
    if tile.water.is_some() {
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

    /// The site lattice's own: the seven clumps `prepare` finds for a cell
    /// hold every site a tile in it may be only when the cell is the
    /// tile's own site lattice cell.
    fn scale(&self) -> u32 { self.lattice.radius }

    /// Nothing to place: a site is its lattice cell's hash.
    fn deform(&self, _scope: &CellScope) {}

    /// The sites the cell's tiles may be, found once for all of them
    fn prepare(&self, scope: &CellScope) -> Box<dyn std::any::Any + Send + Sync> {
        Box::new(sites_near(&self.lattice, scope.cell(), scope.seed()))
    }

    fn query(&self, q: i32, r: i32, below: &TileView, cell: &(dyn std::any::Any + Send + Sync), _seed: u64) -> Option<TileOutput> {
        let sites = cell.downcast_ref::<Vec<(i32, i32)>>()?;
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
        let mut tile = TileView::on(1, 0, 0);
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
        tile.water = Some(1.0);
        assert_eq!(habitat(&tile), Habitat::River, "under water, it dens on the dry ground nearest");
    }

    #[test]
    fn every_cell_has_a_clump_and_the_cell_a_site_lies_in_finds_it() {
        let lattice = den_lattice();
        for id in [(0, 0), (3, -2), (-5, 7), (40, 11), (-9, -30)] {
            let sites: Vec<_> = sites_of(&lattice, id, 7).collect();
            assert!((CLUMP.0..=CLUMP.1).contains(&sites.len()));
            assert_eq!(sites_of(&lattice, id, 7).collect::<Vec<_>>(), sites, "a clump is its cell's hash");
            for &(q, r) in &sites {
                assert!(sites_near(&lattice, lattice.cell_id(q, r), 7).contains(&(q, r)), "a site strays no further than the ring");
            }
        }
    }

    /// The sites of a clump stand apart by more than a den and the ground
    /// it clears, and near enough to be one place.
    #[test]
    fn a_clumps_sites_stand_apart_and_together() {
        let lattice = den_lattice();
        let flat = |a: (i32, i32), b: (i32, i32)| {
            let (dq, dr) = (a.0 - b.0, a.1 - b.1);
            dq.abs().max(dr.abs()).max((dq + dr).abs())
        };
        for q in -12..12 {
            for r in -12..12 {
                let sites: Vec<_> = sites_of(&lattice, (q, r), 7).collect();
                for (i, &a) in sites.iter().enumerate() {
                    for &b in &sites[i + 1..] {
                        let apart = flat(a, b);
                        // Tiles counted along the grid run up to 2/√3 of the
                        // straight line.
                        assert!(apart >= 24 && apart as f64 <= 2.0 * CLUMP_REACH.1 * 2.0 / 3f64.sqrt() + 2.0, "{a:?} {b:?}: {apart}");
                    }
                }
            }
        }
    }
}
