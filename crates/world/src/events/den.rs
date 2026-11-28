//! DenEvent — where creatures make their dens: on each cell of a jittered
//! lattice a colony's candidate sites, close set, and of those the ones
//! whose own ground takes a den, with the habitat that ground gives it.
//!
//! # Claims
//!
//! A colony's candidates are a pure function of its lattice cell's hash:
//! the cell's centre jittered, a site there and six round it. A cell finds
//! once the candidates of its own lattice cell and the six around it, as
//! far as a site strays, a tile whether it is one of them, and nothing is
//! published. A candidate reads its habitat from its own tile, as the
//! layers beneath compose it, and from nothing else: under water; under
//! trees in most of its growth sites; under brush in most of them; on
//! ground near repose; on rock harder than limestone; else open land.
//! Where several hold, the first of those does. A site under water keeps
//! its den: the server finds the dry ground nearest it to stand it on.
//!
//! The same tile says whether the candidate is a site at all: each
//! habitat keeps a share of its candidates ([`share`]), open land few, by
//! the tile's own hash against how rich its colony is, which is the
//! cell's. So the land says how many nests a colony has and whether it has
//! any: thick where the ground homes something, thin and scattered over
//! bare plain, and no two colonies alike. No tile knows how many of its
//! colony's candidates the ground took, which only their own tiles say.
//!
//! The tile carries its den's habitat, the colony it is of, by its lattice
//! cell, and which of that colony's candidates it is. What stands at a
//! site, and exactly where, is the server's to decide on the ground it has
//! materialized round the site.

use common::{den::{Habitat, Site}, Canopy, HexLattice, SITES};

use crate::lattice::jittered_centre;
use crate::noise::hash_channel_f64;
use crate::world_to_hex;
use super::index::CellId;
use super::thrusting::REPOSE_GRADE;
use super::{CellScope, TileOutput, TileView, WorldEvent};

const DEN_SEED: u64 = 0x6465_6e73;

/// The radius of the lattice colonies stand on: centres about 2r + 1,
/// some 173 tiles, apart, forty seconds of walking.
pub const DEN_LATTICE: u32 = 86;

/// How far a colony's centre strays from its cell's, as a share of the
/// spacing.
pub const DEN_JITTER: f64 = 0.4;

/// How many candidate sites a colony has: one at its centre and six
/// round it.
pub const CANDIDATES: usize = 7;

/// How far from its colony's centre a candidate round it stands, nearest
/// and furthest, in tiles: far enough that two neighbours stand a den and
/// its cleared ground apart, and near enough that the colony is one place.
pub const CLUMP_REACH: (f64, f64) = (26.5, 29.5);

/// How far a candidate strays from its even place round the colony, in
/// radians.
const CLUMP_SWAY: f64 = 0.04;

/// How rich the richest colony is: its candidates are kept this many
/// times as readily as their habitat's share, and the poorest never.
pub const RICHEST: f64 = 2.0;

/// The share of its candidates a habitat keeps in a colony of middling
/// richness: ground that homes something keeps half, a range a little
/// less for how little of it is fit to stand on, and open land few, so
/// the bare plain holds scattered packs and no more.
pub fn share(habitat: Habitat) -> f64 {
    match habitat {
        Habitat::River | Habitat::Woods | Habitat::Scrub | Habitat::Rock => 0.5,
        Habitat::Range => 0.4,
        Habitat::Open => 0.12,
    }
}

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

/// The tiles a lattice cell puts its colony's candidate sites on: one on
/// the cell's centre, jittered, and the rest set evenly round it at a
/// turn, each at its own reach and a little off its place, all by the
/// cell's hash.
pub fn sites_of(lattice: &HexLattice, id: CellId, seed: u64) -> impl Iterator<Item = (i32, i32)> {
    let seed = seed ^ DEN_SEED;
    let (x, y) = jittered_centre(lattice, id, DEN_JITTER, seed);
    // The centre's jitter takes the hash's first two channels.
    let h = move |channel: u64| hash_channel_f64(id.0 as i64, id.1 as i64, seed, channel);
    let turn = h(4) * std::f64::consts::TAU;
    let round = (CANDIDATES - 1) as f64;
    std::iter::once(world_to_hex(x, y)).chain((0..CANDIDATES - 1).map(move |k| {
        let channel = 5 + 2 * k as u64;
        let angle = turn + std::f64::consts::TAU * k as f64 / round + (h(channel) - 0.5) * 2.0 * CLUMP_SWAY;
        let reach = CLUMP_REACH.0 + h(channel + 1) * (CLUMP_REACH.1 - CLUMP_REACH.0);
        world_to_hex(x + reach * angle.cos(), y + reach * angle.sin())
    }))
}

/// How rich colony `id` is, from none to [`RICHEST`], by its cell's hash.
pub fn richness(id: CellId, seed: u64) -> f64 {
    RICHEST * hash_channel_f64(id.0 as i64, id.1 as i64, seed ^ DEN_SEED, 3)
}

/// Whether the candidate on tile `(q, r)`, on ground of `habitat` in a
/// colony of `richness`, is a den site: by the tile's own hash.
pub fn takes(q: i32, r: i32, habitat: Habitat, richness: f64, seed: u64) -> bool {
    hash_channel_f64(q as i64, r as i64, seed ^ DEN_SEED, 0) < share(habitat) * richness
}

/// A candidate's place: its tile, its colony, which of that colony's
/// candidates it is, and how rich that colony is.
pub type Place = ((i32, i32), CellId, u8, f64);

/// The candidates a cell's tiles may be, in the order of their tiles:
/// its own colony's and those of the six around it, which is as far as a
/// site strays.
pub fn sites_near(lattice: &HexLattice, cell: CellId, seed: u64) -> Vec<Place> {
    let mut near: Vec<Place> = std::iter::once(cell)
        .chain(lattice.neighbor_cells(cell))
        .flat_map(|id| {
            let rich = richness(id, seed);
            sites_of(lattice, id, seed).enumerate().map(move |(nth, tile)| (tile, id, nth as u8, rich))
        })
        .collect();
    near.sort_by_key(|&(tile, ..)| tile);
    near
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

    /// Nothing to place: a candidate is its lattice cell's hash.
    fn deform(&self, _scope: &CellScope) {}

    /// The candidates the cell's tiles may be, found once for all of them
    fn prepare(&self, scope: &CellScope) -> Box<dyn std::any::Any + Send + Sync> {
        Box::new(sites_near(&self.lattice, scope.cell(), scope.seed()))
    }

    fn query(&self, q: i32, r: i32, below: &TileView, cell: &(dyn std::any::Any + Send + Sync), seed: u64) -> Option<TileOutput> {
        let near = cell.downcast_ref::<Vec<Place>>()?;
        let (_, colony, nth, rich) = near[near.binary_search_by_key(&(q, r), |&(tile, ..)| tile).ok()?];
        let habitat = habitat(below);
        takes(q, r, habitat, rich, seed).then(|| TileOutput { den: Some(Site { habitat, colony, nth }), ..TileOutput::default() })
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
    fn every_cell_has_its_candidates_and_the_cell_one_lies_in_finds_it() {
        let lattice = den_lattice();
        for id in [(0, 0), (3, -2), (-5, 7), (40, 11), (-9, -30)] {
            let sites: Vec<_> = sites_of(&lattice, id, 7).collect();
            assert_eq!(sites.len(), CANDIDATES);
            assert_eq!(sites_of(&lattice, id, 7).collect::<Vec<_>>(), sites, "a colony is its cell's hash");
            for (nth, &tile) in sites.iter().enumerate() {
                let near = sites_near(&lattice, lattice.cell_id(tile.0, tile.1), 7);
                let place = near.iter().find(|place| place.0 == tile).map(|&(_, colony, nth, _)| (colony, nth as usize));
                assert_eq!(place, Some((id, nth)), "a candidate strays no further than the ring, and knows its colony");
            }
        }
    }

    /// The candidates of a colony stand apart by more than a den and the
    /// ground it clears, and near enough to be one place.
    #[test]
    fn a_colonys_candidates_stand_apart_and_together() {
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

    /// Ground that homes something keeps more of its candidates than open
    /// land, a richer colony more than a poorer, and the poorest none.
    #[test]
    fn the_ground_and_the_colony_say_how_many_candidates_are_sites() {
        let kept = |habitat: Habitat, richness: f64| (0..4000).filter(|&q| takes(q, -q / 3, habitat, richness, 7)).count();
        assert!(kept(Habitat::Woods, 1.0) > kept(Habitat::Open, 1.0) && kept(Habitat::Open, 1.0) > 0);
        assert!(kept(Habitat::Woods, RICHEST) > kept(Habitat::Woods, 1.0));
        assert_eq!(kept(Habitat::Woods, 0.0), 0);
        let rich: Vec<f64> = (0..200).map(|q| richness((q, 3 - q), 7)).collect();
        assert!(rich.iter().all(|&r| (0.0..RICHEST).contains(&r)) && rich.iter().any(|&r| r < 0.5) && rich.iter().any(|&r| r > 1.5));
    }
}
