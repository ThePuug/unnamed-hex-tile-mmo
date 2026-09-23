//! OutcropEvent — the rare knots of rock that stand out of the land, and
//! the boulders each stands in its tiles' slots. `design/outcrop.md` in the
//! internal repo is the spec; the claims below are the ones the code binds.
//!
//! # Claims
//!
//! An outcrop is a place, not a cover. Sites stand on a sparse jittered
//! lattice, about three minutes' walk apart, and each is a crag about its
//! origin: a core of rock no one walks into, thinning to scattered stones
//! at its edge. Whether a site shows, and how much of it, is the ground's:
//! rock stands out where it resists, so a tile near an origin shows rock
//! by how hard its rock is, how steep its slope, and how cold, and a site
//! on soft, gentle, warm ground shows nothing. On basement a site shows on
//! the flat, a tor; on shale only where the slope is near repose.
//!
//! A site is a pure function of its lattice cell's hash, so a tile finds
//! the sites reaching it from its own lattice cell and the six around it,
//! and nothing is published. A tile reads its grade, its rock and its
//! elevation from the layers beneath, and nothing else. Drought, the rim a
//! hard bed stands as at a scarp, and sea cliffs are unbuilt.

use common::{Cover, HexLattice, Rock, TILE_SLOTS};

use crate::hex_to_world;
use crate::noise::hash_channel_f64;
use super::forest::{temperature, TREELINE, TREELINE_BAND};
use super::thrusting::{smoothstep, REPOSE_GRADE};
use super::{CellScope, TileOutput, TileView, WorldEvent};

const SITE_SEED: u64 = 0x6372_6167;
const BOULDER_SEED: u64 = 0x626f_756c;

/// The radius of a site lattice cell, in tiles: cells whose centres stand
/// about 750 tiles apart, three minutes' walk.
pub const SITE_LATTICE: u32 = 433;

/// How far a site's origin swings off its cell's centre, as a share of the
/// spacing, so sites are nowhere on a grid.
pub const SITE_JITTER: f64 = 0.3;

/// A crag's radius in tiles, and how far one site's differs from it either
/// way, as a share.
pub const CRAG_RADIUS: f64 = 5.0;
pub const CRAG_SPREAD: f64 = 0.3;

/// The strength past which every slot is rock: the crag's core, which no
/// one walks into.
pub const CORE: f64 = 0.8;

/// The strength below which a tile shows no rock: the crag's scattered
/// edge ends here.
pub const SCATTER: f64 = 0.08;

/// The erodibility of the hardest rock, which stands out on the flat.
const HARDEST: f64 = 0.3;

/// The lattice sites stand on.
pub fn site_lattice() -> HexLattice {
    HexLattice::new(SITE_LATTICE)
}

/// A site: its origin in world space and its radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Site {
    pub x: f64,
    pub y: f64,
    pub radius: f64,
}

/// The site of a lattice cell: its centre jittered, and its radius, by
/// the cell's hash.
pub fn site_of(lattice: &HexLattice, id: (i32, i32), seed: u64) -> Site {
    let (cq, cr) = lattice.cell_center(id);
    let (cx, cy) = hex_to_world(cq, cr);
    let h = |channel: u64| hash_channel_f64(id.0 as i64, id.1 as i64, seed ^ SITE_SEED, channel);
    let swing = 2.0 * SITE_JITTER * (lattice.tiles_per_cell() as f64).sqrt();
    Site {
        x: cx + (h(1) - 0.5) * swing,
        y: cy + (h(2) - 0.5) * swing,
        radius: CRAG_RADIUS * (1.0 + CRAG_SPREAD * (2.0 * h(3) - 1.0)),
    }
}

/// How far into the nearest site reaching a tile it lies, 1 at the origin
/// to 0 at the site's edge, or 0 where none reaches.
pub fn nearness(lattice: &HexLattice, q: i32, r: i32, seed: u64) -> f64 {
    let (wx, wy) = hex_to_world(q, r);
    let home = lattice.cell_id(q, r);
    std::iter::once(home)
        .chain(lattice.neighbor_cells(home))
        .map(|id| {
            let s = site_of(lattice, id, seed);
            1.0 - (s.x - wx).hypot(s.y - wy) / s.radius
        })
        .fold(0.0, f64::max)
}

/// How readily a tile's ground stands out as rock, 0 to 1: the most of how
/// hard its rock is, how near repose its slope, and how far above the
/// treeline it is.
pub fn resistance(rock: Rock, grade: f64, t: f64) -> f64 {
    let hardness = ((1.0 - rock.erodibility()) / (1.0 - HARDEST)).clamp(0.0, 1.0);
    let steepness = (grade / REPOSE_GRADE).clamp(0.0, 1.0);
    let cold = 1.0 - smoothstep((t - TREELINE) / TREELINE_BAND);
    hardness.max(steepness).max(cold)
}

/// The share of a tile's slots boulders take at a strength: every one at
/// the core, thinning to none at the scatter's end.
pub fn boulder_share(strength: f64) -> f64 {
    smoothstep((strength - SCATTER) / (CORE - SCATTER))
}

/// A tile's boulders at a share: each slot by its own hash against it.
pub fn boulders_of(q: i32, r: i32, share: f64, rock: Rock, seed: u64) -> Cover {
    let mut cover = Cover::NONE;
    for k in 0..TILE_SLOTS as usize {
        if hash_channel_f64(q as i64, r as i64, seed ^ BOULDER_SEED, k as u64) < share {
            cover = cover.with_boulder(k);
        }
    }
    if cover.is_empty() { cover } else { cover.with_rock(rock) }
}

// ── The event ───────────────────────────────────────────────────────────────

/// Cell scale for the layer's own grid. A site is found from its hash, so
/// nothing is published and the scale only sets how often `prepare` runs.
pub const OUTCROP_CELL_SCALE: u32 = 1800;

pub struct OutcropEvent {
    lattice: HexLattice,
}

impl OutcropEvent {
    pub fn new() -> Self { OutcropEvent { lattice: site_lattice() } }
}

impl Default for OutcropEvent {
    fn default() -> Self { Self::new() }
}

impl WorldEvent for OutcropEvent {
    fn name(&self) -> &str { "outcrop" }
    fn scale(&self) -> u32 { OUTCROP_CELL_SCALE }

    /// Nothing to place: a site is its lattice cell's hash.
    fn deform(&self, _scope: &CellScope) {}

    fn query(
        &self,
        q: i32, r: i32,
        below: &TileView,
        _cell: &(dyn std::any::Any + Send + Sync),
        seed: u64,
    ) -> Option<TileOutput> {
        let near = nearness(&self.lattice, q, r, seed);
        if near <= 0.0 || below.water.is_some() {
            return None;
        }
        let rock = below.rock?;
        let (wx, wy) = hex_to_world(q, r);
        let t = temperature(wx, wy, below.elevation, seed);
        let share = boulder_share(near * resistance(rock, below.grade(), t));
        let cover = boulders_of(q, r, share, rock, seed);
        if cover.is_empty() {
            return None;
        }
        Some(TileOutput { cover, ..TileOutput::default() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: u64 = 0x9E3779B97F4A7C15;
    const WARM: f64 = 20.0;

    /// Harder, steeper and colder ground each stand out more, and soft,
    /// gentle, warm ground not at all.
    #[test]
    fn hard_steep_or_cold_ground_resists() {
        assert_eq!(resistance(Rock::Shale, 0.0, WARM), 0.0);
        assert_eq!(resistance(Rock::Basement, 0.0, WARM), 1.0);
        assert!(resistance(Rock::Limestone, 0.0, WARM) > resistance(Rock::Sandstone, 0.0, WARM));
        assert_eq!(resistance(Rock::Shale, REPOSE_GRADE, WARM), 1.0);
        assert_eq!(resistance(Rock::Shale, 0.0, TREELINE), 1.0);
        let mut last = 0.0;
        for i in 0..=20 {
            let v = resistance(Rock::Sandstone, REPOSE_GRADE * i as f64 / 20.0, WARM);
            assert!(v >= last);
            last = v;
        }
    }

    /// A crag is rock through its core, thins toward its edge, and is
    /// nothing past its scatter.
    #[test]
    fn a_crag_thins_from_its_core() {
        assert_eq!(boulder_share(1.0), 1.0);
        assert_eq!(boulder_share(CORE), 1.0);
        assert_eq!(boulder_share(SCATTER), 0.0);
        let mut last = 0.0;
        for i in 0..=20 {
            let s = boulder_share(i as f64 / 20.0);
            assert!(s >= last);
            last = s;
        }
    }

    /// Sites are sparse: about one per lattice cell, each reaching only its
    /// radius, so nearly every tile lies in none and the origin lies in one.
    #[test]
    fn sites_are_rare_and_found_from_any_tile() {
        let lattice = site_lattice();
        let mut near = 0;
        let mut total = 0;
        for dq in (-2000..=2000).step_by(3) {
            for dr in (-2000..=2000).step_by(37) {
                total += 1;
                if nearness(&lattice, 104_289 + dq, -4_677 + dr, S) > 0.0 {
                    near += 1;
                }
            }
        }
        let share = near as f64 / total as f64;
        assert!(share > 0.0 && share < 0.002, "{near} of {total} tiles lie in a site");
        let id = lattice.cell_id(104_289, -4_677);
        let s = site_of(&lattice, id, S);
        let (q, r) = crate::world_to_hex(s.x, s.y);
        assert!(nearness(&lattice, q, r, S) > 0.8, "the origin's own tile is not in its site");
    }

    /// Boulders fill with their share and carry the rock they are.
    #[test]
    fn boulders_fill_with_their_share() {
        for (q, r) in [(0, 0), (104_289, -4_677), (-3_000_000, 1_000_000)] {
            assert!(boulders_of(q, r, 0.0, Rock::Sandstone, S).is_empty());
            let all = boulders_of(q, r, 1.0, Rock::Limestone, S);
            assert_eq!(all.fullness(), TILE_SLOTS);
            assert_eq!(all.rock(), Rock::Limestone);
            let mut last = 0;
            for i in 0..=20 {
                let n = boulders_of(q, r, i as f64 / 20.0, Rock::Basement, S).boulders().count();
                assert!(n >= last);
                last = n;
            }
        }
    }
}
