//! Continents — sites on a lattice coarser than the plates, each claiming a
//! compact cluster of plates as its continent and, by hash, a smaller one
//! across a strait as its subcontinent. What makes a plate continental.
//!
//! # Claims
//!
//! A site is a jittered point on a hex lattice several plates wide. It
//! claims its continent by growing from the plate it lies in over plate
//! adjacency, taking at each step the adjacent plate nearest the site by a
//! lobed measure, until it holds the size it hashed. The lobes push a plate
//! or two out, a peninsula, hold one back, a gulf, and bound how long the
//! continent can be. A site claims only plates nearer to it than to any
//! other site, and never one adjacent to another site's plate: those border
//! plates are sea on both sides of every site boundary, so two continents
//! have at least two plates of water between them, farther than the client
//! draws. Nor does it claim one within [`CLAIM_GAP`] of a plate another
//! site may claim, which keeps two continents' land apart in the ground as
//! well as in the graph: two plates of water can narrow to one short side
//! where four seeds lie nearly on a circle, and a side too short for the
//! lattice is no side of the graph at all. A site with too little room for
//! a continent claims nothing.
//!
//! A subcontinent is one to four plates one plate of water from its
//! continent: grown the same way from a plate two steps out from the
//! continent, the candidate nearest a hashed bearing, never adjacent to it.
//! It claims under the same border rule, so it keeps a plate of water from
//! every other landmass too. A one-plate strait is a subcontinent's strait
//! and nothing else's.
//!
//! # Land
//!
//! A site's land stands within [`LAND_REACH`] of the plates it claimed, as
//! the plates are drawn (`plates::warp`), and [`CLAIM_GAP`] keeps any two
//! claims farther apart than twice that, so a position is the land of one
//! site at most ([`land_site`]), and none over open sea. The reach is
//! [`CLAIM_GAP`]'s own argument, not a proof: `top_probe` reads every tile
//! at its edge for land.
//!
//! # Exactness
//!
//! A claim is a function of the seed and the site alone, memoised; a plate
//! asks the site nearest its seed whether it is claimed. Nothing here reads
//! a plate's continental flag, or the claim would recurse through it.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};

use dashmap::DashMap;

use crate::events::plates::{warp, WARP_STEP};
use crate::events::thrusting::SCREE_REACH;
use crate::lattice::CHAIN_SWING;
use crate::noise::hash_channel_f64;
use crate::tectonic::{cell_for, cells_around, lattice_point, neighbours_of, plate_cell_for, polygon, seed_point, PlateId, PLATE_SPACING};

/// Site spacing in world units: the dial for the ocean between continents
/// and for the land share, which is about a continent's plates over the
/// plates in a site's cell.
///
/// EMPIRICAL: set for a subcontinent to find room in most sites that hash
/// one, measured by `plate_probe`.
pub const SITE_SPACING: f64 = 8.0 * PLATE_SPACING;

/// How far a site is displaced from its lattice point, as a share of the
/// spacing, in each axis. Enough to break the lattice's rows, short of
/// squeezing a site's cell below what a continent needs.
pub const SITE_JITTER: f64 = 0.15;

/// A continent's size in plates, inclusive at both ends.
pub const CONTINENT_SIZE: (usize, usize) = (6, 10);

/// A subcontinent's size in plates, inclusive at both ends.
pub const SUBCONTINENT_SIZE: (usize, usize) = (1, 4);

/// The share of sites that hash a subcontinent.
pub const SUBCONTINENT_SHARE: f64 = 0.5;

/// How far apart two sites' claimed plates stand at least, between their
/// polygons, in the coordinates the plates are drawn in. Land stands past a
/// continental plate only within two chain swings of its polygon: a coast
/// decides which side is land within a swing of itself, an outline stands a
/// position on its plate only within a swing of the plate's sides, a toe
/// rests within its reach of a front, and every chain lies within a swing
/// of its plate's side. Twice that, and the farthest the warp carries two adjacent tiles
/// apart, and no land tile of one continent lies beside one of another.
pub const CLAIM_GAP: f64 = 4.0 * CHAIN_SWING + WARP_STEP;

const _: () = assert!(SCREE_REACH <= CHAIN_SWING, "a toe can rest farther past its front than the claim gap allows");

/// How far past its claimed plates a site's land may stand, as the plates
/// are drawn: the two chain swings [`CLAIM_GAP`] allows each side.
pub const LAND_REACH: f64 = 2.0 * CHAIN_SWING;

const _: () = assert!(2.0 * LAND_REACH < CLAIM_GAP, "two sites' land could meet");

/// The most each lobe of the claim's measure stretches or shrinks the
/// radius, as a share of it. Two lobes, second and third harmonic, so the
/// outline gets a cape or two and a gulf; the sum bounds the aspect.
pub const LOBE_AMPLITUDE: f64 = 0.25;

pub type SiteId = (i32, i32);

const SITE_SEED: u64 = 0x436F_6E74_696E_656E; // "Continen"
const CH_SIZE: u64 = 1;
const CH_LOBE_A2: u64 = 2;
const CH_LOBE_P2: u64 = 3;
const CH_LOBE_A3: u64 = 4;
const CH_LOBE_P3: u64 = 5;
const CH_SUB_GATE: u64 = 6;
const CH_SUB_SIZE: u64 = 7;
const CH_SUB_BEARING: u64 = 8;

/// A continent site: where it stands, what it hashed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Site {
    pub id: SiteId,
    pub wx: f64,
    pub wy: f64,
    /// Plates the continent holds when the site has room for them.
    pub size: usize,
    /// Second and third harmonic of the claim's measure: amplitude, phase.
    pub lobes: [(f64, f64); 2],
    /// A subcontinent's plates and the bearing it is sought on, when the
    /// site hashed one.
    pub subcontinent: Option<(usize, f64)>,
}

impl Site {
    /// How far a position stands from the site, in the site's own shape:
    /// the distance over the lobed radius, so the claim's boundary is a
    /// contour of it.
    pub fn measure(&self, wx: f64, wy: f64) -> f64 {
        let (dx, dy) = (wx - self.wx, wy - self.wy);
        let theta = dy.atan2(dx);
        let radius = 1.0 + self.lobes[0].0 * (2.0 * theta - self.lobes[0].1).cos() + self.lobes[1].0 * (3.0 * theta - self.lobes[1].1).cos();
        dx.hypot(dy) / radius
    }
}

/// What a site claimed: its continent and its subcontinent, each empty
/// when the site had no room for it.
#[derive(Clone, Debug, Default)]
pub struct Claim {
    pub continent: Vec<PlateId>,
    pub subcontinent: Vec<PlateId>,
}

/// A site's claimed plates, continent and subcontinent, as the ground
/// stands on them: each one's polygon, and the box of every corner, as the
/// plates are drawn. No polygons where the site claimed nothing.
pub struct Land {
    pub polygons: Vec<Vec<(f64, f64)>>,
    pub lo: (f64, f64),
    pub hi: (f64, f64),
}

impl Land {
    /// Whether a position, as the plates are drawn, lies within
    /// [`LAND_REACH`] of a claimed plate.
    pub fn reaches(&self, x: f64, y: f64) -> bool {
        let r = LAND_REACH;
        x >= self.lo.0 - r && x <= self.hi.0 + r && y >= self.lo.1 - r && y <= self.hi.1 + r && self.gap(x, y) <= r
    }

    /// How far a position, as the plates are drawn, lies from the nearest
    /// claimed plate: none on one.
    pub fn gap(&self, x: f64, y: f64) -> f64 {
        self.polygons.iter().map(|p| point_gap((x, y), p)).fold(f64::INFINITY, f64::min)
    }
}

/// A site's land, computed once per site and seed.
pub fn land(id: SiteId, seed: u64) -> Arc<Land> {
    static LANDS: OnceLock<DashMap<(u64, SiteId), Arc<Land>>> = OnceLock::new();
    let lands = LANDS.get_or_init(DashMap::new);
    if let Some(l) = lands.get(&(seed, id)) {
        return l.value().clone();
    }
    let c = claim(id, seed);
    let polygons: Vec<Vec<(f64, f64)>> =
        c.continent.iter().chain(&c.subcontinent).map(|&p| polygon(p, seed).into_iter().map(|corner| corner.0).collect()).collect();
    let corners = || polygons.iter().flatten();
    let lo = corners().fold((f64::INFINITY, f64::INFINITY), |a, &(x, y)| (a.0.min(x), a.1.min(y)));
    let hi = corners().fold((f64::NEG_INFINITY, f64::NEG_INFINITY), |a, &(x, y)| (a.0.max(x), a.1.max(y)));
    let l = Arc::new(Land { polygons, lo, hi });
    lands.entry((seed, id)).or_insert(l).value().clone()
}

/// The site whose land a world position may be: the one whose claimed
/// plates lie within [`LAND_REACH`] of where the plates are drawn there.
/// None over open sea.
pub fn land_site(wx: f64, wy: f64, seed: u64) -> Option<SiteId> {
    let (x, y) = warp(wx, wy, seed);
    cells_around(site_cell_for(x, y), 1).into_iter().find(|&id| land(id, seed).reaches(x, y))
}

/// The site lattice cell whose undisplaced centre is nearest a position.
pub fn site_cell_for(wx: f64, wy: f64) -> SiteId {
    cell_for(wx, wy, SITE_SPACING)
}

pub fn site(id: SiteId, seed: u64) -> Site {
    let (wx, wy) = lattice_point(id, SITE_SPACING, SITE_JITTER, seed ^ SITE_SEED);
    let (cq, cr) = (id.0 as i64, id.1 as i64);
    let h = |ch: u64| hash_channel_f64(cq, cr, seed ^ SITE_SEED, ch);
    let span = |range: (usize, usize), u: f64| range.0 + ((range.1 - range.0 + 1) as f64 * u) as usize;
    let size = span(CONTINENT_SIZE, h(CH_SIZE)).min(CONTINENT_SIZE.1);
    let lobes = [
        (LOBE_AMPLITUDE * h(CH_LOBE_A2), std::f64::consts::TAU * h(CH_LOBE_P2)),
        (LOBE_AMPLITUDE * h(CH_LOBE_A3), std::f64::consts::TAU * h(CH_LOBE_P3)),
    ];
    let subcontinent = (h(CH_SUB_GATE) < SUBCONTINENT_SHARE)
        .then(|| (span(SUBCONTINENT_SIZE, h(CH_SUB_SIZE)).min(SUBCONTINENT_SIZE.1), std::f64::consts::TAU * h(CH_SUB_BEARING)));
    Site { id, wx, wy, size, lobes, subcontinent }
}

/// The site nearest a position. The home cell's site lies within the
/// cell's circumradius and the jitter; a second-ring site lies at least
/// two rows less the jitter away, farther, so the contest is over the
/// home cell and one ring.
pub fn nearest_site(wx: f64, wy: f64, seed: u64) -> SiteId {
    let home = site_cell_for(wx, wy);
    let mut best: Option<(f64, SiteId)> = None;
    for id in cells_around(home, 1) {
        let (sx, sy) = lattice_point(id, SITE_SPACING, SITE_JITTER, seed ^ SITE_SEED);
        let d = (sx - wx).hypot(sy - wy);
        if best.map_or(true, |(bd, _)| d < bd) {
            best = Some((d, id));
        }
    }
    best.expect("a lattice cell has neighbours").1
}

/// Whether a plate is continental: claimed by the site nearest its seed,
/// as continent or subcontinent.
pub fn is_continental(id: PlateId, seed: u64) -> bool {
    let (px, py) = seed_point(id, seed);
    let c = claim(nearest_site(px, py, seed), seed);
    c.continent.contains(&id) || c.subcontinent.contains(&id)
}

/// A site's claim, computed once per site and seed.
pub fn claim(id: SiteId, seed: u64) -> Arc<Claim> {
    static CLAIMS: OnceLock<DashMap<(u64, SiteId), Arc<Claim>>> = OnceLock::new();
    let claims = CLAIMS.get_or_init(DashMap::new);
    if let Some(c) = claims.get(&(seed, id)) {
        return c.value().clone();
    }
    let c = Arc::new(Growth::new(site(id, seed), seed).claim());
    claims.entry((seed, id)).or_insert(c).value().clone()
}

/// The plate graph around one site as its claim grows over it, the
/// neighbours and polygon of each plate read once, and whether the site
/// may claim it decided once.
struct Growth {
    site: Site,
    seed: u64,
    neighbours: HashMap<PlateId, Vec<PlateId>>,
    polygons: HashMap<PlateId, Vec<((f64, f64), PlateId)>>,
    decided: HashMap<PlateId, bool>,
}

impl Growth {
    fn new(site: Site, seed: u64) -> Self {
        Self { site, seed, neighbours: HashMap::new(), polygons: HashMap::new(), decided: HashMap::new() }
    }

    fn neighbours(&mut self, id: PlateId) -> &[PlateId] {
        let seed = self.seed;
        self.neighbours.entry(id).or_insert_with(|| neighbours_of(id, seed))
    }

    fn polygon(&mut self, id: PlateId) -> &[((f64, f64), PlateId)] {
        let seed = self.seed;
        self.polygons.entry(id).or_insert_with(|| polygon(id, seed))
    }

    fn site_of(&self, id: PlateId) -> SiteId {
        let (px, py) = seed_point(id, self.seed);
        nearest_site(px, py, self.seed)
    }

    fn owned(&self, id: PlateId) -> bool {
        self.site_of(id) == self.site.id
    }

    /// Whether the site nearest a plate's seed may claim it by the plate
    /// graph: no neighbour nearer to another site. What one site asks of
    /// another's plates without growing the other's claim.
    fn eligible(&mut self, id: PlateId) -> bool {
        let site = self.site_of(id);
        let neighbours = self.neighbours(id).to_vec();
        neighbours.iter().all(|&n| self.site_of(n) == site)
    }

    /// Whether the site may claim a plate: nearer to this site than any
    /// other, no neighbour nearer to another, and no plate another site may
    /// claim by the graph within [`CLAIM_GAP`]. The border plates the second
    /// excludes keep two claims apart in the graph; the third keeps them a
    /// gap apart in the ground, since every plate another site claims is
    /// one it may claim by the graph.
    fn claimable(&mut self, id: PlateId) -> bool {
        if let Some(&known) = self.decided.get(&id) {
            return known;
        }
        let may = self.owned(id) && self.eligible(id) && self.clear(id);
        self.decided.insert(id, may);
        may
    }

    /// Whether no plate another site may claim by the graph lies within
    /// [`CLAIM_GAP`] of a plate, between their polygons. The plates within
    /// the gap are found outward from it, over the plates round each, since
    /// the shortest line to any of them crosses only plates within the gap.
    fn clear(&mut self, id: PlateId) -> bool {
        let own: Vec<(f64, f64)> = self.polygon(id).iter().map(|c| c.0).collect();
        let mut seen = HashSet::from([id]);
        let mut queue = vec![id];
        while let Some(p) = queue.pop() {
            let round: Vec<PlateId> = self.polygon(p).iter().map(|c| c.1).collect();
            for q in round {
                if !seen.insert(q) {
                    continue;
                }
                let corners: Vec<(f64, f64)> = self.polygon(q).iter().map(|c| c.0).collect();
                if polygon_gap(&own, &corners) > CLAIM_GAP {
                    continue;
                }
                if !self.owned(q) && self.eligible(q) {
                    return false;
                }
                queue.push(q);
            }
        }
        true
    }

    fn measure(&self, id: PlateId) -> f64 {
        let (px, py) = seed_point(id, self.seed);
        self.site.measure(px, py)
    }

    /// Grow a connected claim of `size` plates from `core`, taking at each
    /// step the adjacent claimable plate least by `rank`, never one in
    /// `barred`; short when the room runs out.
    fn grow(&mut self, core: PlateId, size: usize, barred: &HashSet<PlateId>, rank: impl Fn(&Self, PlateId) -> f64) -> Vec<PlateId> {
        let mut claimed = vec![core];
        let mut held: HashSet<PlateId> = HashSet::from([core]);
        let mut rejected: HashSet<PlateId> = HashSet::new();
        while claimed.len() < size {
            let mut best: Option<(f64, PlateId)> = None;
            let frontier: Vec<PlateId> = claimed.iter().flat_map(|&c| self.neighbours(c).to_vec()).collect();
            for n in frontier {
                if held.contains(&n) || rejected.contains(&n) {
                    continue;
                }
                if barred.contains(&n) || !self.claimable(n) {
                    rejected.insert(n);
                    continue;
                }
                let key = (rank(self, n), n);
                if best.map_or(true, |b| key < b) {
                    best = Some(key);
                }
            }
            let Some((_, next)) = best else { break };
            claimed.push(next);
            held.insert(next);
        }
        claimed
    }

    /// The plates a step past a set: adjacent to one of them, not in it.
    fn ring(&mut self, of: &[PlateId], excluding: &HashSet<PlateId>) -> Vec<PlateId> {
        let mut out: Vec<PlateId> = of.iter().flat_map(|&p| self.neighbours(p).to_vec()).filter(|n| !excluding.contains(n)).collect();
        out.sort();
        out.dedup();
        out
    }

    fn claim(mut self) -> Claim {
        let site = self.site;
        // The core is the claimable plate nearest the site; none within two
        // rings means the site's cell is squeezed past use.
        let home = plate_cell_for(site.wx, site.wy);
        let mut core: Option<(f64, PlateId)> = None;
        for id in cells_around(home, 2) {
            if self.claimable(id) {
                let key = (self.measure(id), id);
                if core.map_or(true, |c| key < c) {
                    core = Some(key);
                }
            }
        }
        let core = core.map(|(_, id)| id);
        let Some(core) = core else { return Claim::default() };
        let continent = self.grow(core, site.size, &HashSet::new(), |g, id| g.measure(id));
        if continent.len() < CONTINENT_SIZE.0 {
            return Claim::default();
        }
        let Some((size, bearing)) = site.subcontinent else { return Claim { continent, subcontinent: vec![] } };

        // One plate of water between: the subcontinent starts two steps out
        // from the continent and never grows onto the ring between.
        let held: HashSet<PlateId> = continent.iter().copied().collect();
        let water = self.ring(&continent, &held);
        let barred: HashSet<PlateId> = held.iter().chain(water.iter()).copied().collect();
        let outer = self.ring(&water, &barred);
        let mut start: Option<(f64, PlateId)> = None;
        for id in outer {
            if self.claimable(id) {
                let (px, py) = seed_point(id, self.seed);
                let off = ((py - site.wy).atan2(px - site.wx) - bearing).rem_euclid(std::f64::consts::TAU);
                let key = (off.min(std::f64::consts::TAU - off), id);
                if start.map_or(true, |s| key < s) {
                    start = Some(key);
                }
            }
        }
        let start = start.map(|(_, id)| id);
        let Some(start) = start else { return Claim { continent, subcontinent: vec![] } };
        let (ax, ay) = seed_point(start, self.seed);
        let subcontinent = self.grow(start, size, &barred, move |g, id| {
            let (px, py) = seed_point(id, g.seed);
            (px - ax).hypot(py - ay)
        });
        Claim { continent, subcontinent }
    }
}

/// The distance from a point to a segment.
fn to_side(p: (f64, f64), s0: (f64, f64), s1: (f64, f64)) -> f64 {
    let (dx, dy) = (s1.0 - s0.0, s1.1 - s0.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 { (((p.0 - s0.0) * dx + (p.1 - s0.1) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    (p.0 - s0.0 - t * dx).hypot(p.1 - s0.1 - t * dy)
}

/// How far a point lies from a convex polygon: none inside it, else the
/// least distance to a side.
fn point_gap(p: (f64, f64), poly: &[(f64, f64)]) -> f64 {
    let n = poly.len();
    let side = |k: usize| {
        let (a, b) = (poly[k], poly[(k + 1) % n]);
        (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0)
    };
    if (0..n).all(|k| side(k) >= 0.0) || (0..n).all(|k| side(k) <= 0.0) {
        return 0.0;
    }
    (0..n).map(|k| to_side(p, poly[k], poly[(k + 1) % n])).fold(f64::MAX, f64::min)
}

/// The least distance between two convex polygons that do not overlap:
/// the least from a corner of either to a side of the other.
fn polygon_gap(a: &[(f64, f64)], b: &[(f64, f64)]) -> f64 {
    let one_way = |a: &[(f64, f64)], b: &[(f64, f64)]| {
        let sides = || (0..b.len()).map(|i| (b[i], b[(i + 1) % b.len()]));
        a.iter().flat_map(|&p| sides().map(move |(s0, s1)| to_side(p, s0, s1))).fold(f64::MAX, f64::min)
    };
    one_way(a, b).min(one_way(b, a))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    const SEEDS: [u64; 2] = [0x9E3779B97F4A7C15, 0x0123456789abcdef];

    fn sites(n: i32) -> Vec<SiteId> {
        cells_around((0, 0), n)
    }

    /// Steps over plate adjacency from any plate of `from` to the nearest
    /// plate of `to`, searched out to `limit`.
    fn graph_distance(from: &[PlateId], to: &[PlateId], seed: u64, limit: usize) -> usize {
        let goal: HashSet<PlateId> = to.iter().copied().collect();
        let mut seen: HashSet<PlateId> = from.iter().copied().collect();
        let mut queue: VecDeque<(PlateId, usize)> = from.iter().map(|&p| (p, 0)).collect();
        while let Some((p, d)) = queue.pop_front() {
            if goal.contains(&p) {
                return d;
            }
            if d == limit {
                continue;
            }
            for n in neighbours_of(p, seed) {
                if seen.insert(n) {
                    queue.push_back((n, d + 1));
                }
            }
        }
        limit + 1
    }

    /// Whether every plate of a set is reached from its first over the
    /// set's own adjacency.
    fn connected(part: &[PlateId], seed: u64) -> bool {
        let set: HashSet<PlateId> = part.iter().copied().collect();
        let mut seen: HashSet<PlateId> = HashSet::from([part[0]]);
        let mut queue: VecDeque<PlateId> = VecDeque::from([part[0]]);
        while let Some(p) = queue.pop_front() {
            for n in neighbours_of(p, seed) {
                if set.contains(&n) && seen.insert(n) {
                    queue.push_back(n);
                }
            }
        }
        seen.len() == set.len()
    }

    /// A claim is connected over plate adjacency, and a continent is the
    /// size its site hashed or nothing.
    #[test]
    fn claims_are_connected_and_sized() {
        for seed in SEEDS {
            for id in sites(3) {
                let s = site(id, seed);
                let c = claim(id, seed);
                assert!(c.continent.is_empty() || c.continent.len() == s.size, "site {id:?}: {} of {} plates", c.continent.len(), s.size);
                for part in [&c.continent, &c.subcontinent] {
                    assert!(part.is_empty() || connected(part, seed), "a claim in two pieces at {id:?}");
                }
                if !c.subcontinent.is_empty() {
                    assert!(c.subcontinent.len() <= s.subcontinent.expect("a subcontinent only where hashed").0);
                }
            }
        }
    }

    /// A subcontinent stands one plate of water from its continent; two
    /// continents stand two or more apart; a subcontinent and any other
    /// landmass one or more; and two sites' plates stand the claim gap
    /// apart.
    #[test]
    fn water_between_landmasses() {
        for seed in SEEDS {
            let claims: Vec<(SiteId, Arc<Claim>)> = sites(3).into_iter().map(|id| (id, claim(id, seed))).collect();
            let corners = |p: PlateId| -> Vec<(f64, f64)> { polygon(p, seed).into_iter().map(|c| c.0).collect() };
            for (id, c) in &claims {
                if !c.subcontinent.is_empty() {
                    assert_eq!(graph_distance(&c.subcontinent, &c.continent, seed, 3), 2, "site {id:?}");
                }
                for (other, o) in &claims {
                    if other == id {
                        continue;
                    }
                    for &p in c.continent.iter().chain(&c.subcontinent) {
                        for &q in o.continent.iter().chain(&o.subcontinent) {
                            let gap = polygon_gap(&corners(p), &corners(q));
                            assert!(gap > CLAIM_GAP, "plates {p:?} of {id:?} and {q:?} of {other:?} stand {gap:.0} apart");
                        }
                    }
                    if !c.continent.is_empty() && !o.continent.is_empty() {
                        assert!(graph_distance(&c.continent, &o.continent, seed, 3) >= 3, "continents {id:?} and {other:?} within two plates");
                    }
                    for land in [&o.continent, &o.subcontinent] {
                        if !c.subcontinent.is_empty() && !land.is_empty() {
                            assert!(graph_distance(&c.subcontinent, land, seed, 2) >= 2, "subcontinent of {id:?} touches {other:?}");
                        }
                    }
                }
            }
        }
    }

    /// Every claimed plate is nearest the site that claimed it, which is
    /// what lets a plate ask one site.
    #[test]
    fn a_claim_holds_only_its_own_plates() {
        for seed in SEEDS {
            for id in sites(3) {
                let c = claim(id, seed);
                for p in c.continent.iter().chain(c.subcontinent.iter()) {
                    let (px, py) = seed_point(*p, seed);
                    assert_eq!(nearest_site(px, py, seed), id);
                    assert!(is_continental(*p, seed));
                }
            }
        }
    }

    /// The nearest site is found, against a wide scan.
    #[test]
    fn nearest_site_is_nearest() {
        let seed = SEEDS[0];
        for i in 0..30 {
            for j in 0..30 {
                let (x, y) = (i as f64 * 9_333.0 - 140_000.0, j as f64 * 7_777.0 - 110_000.0);
                let best = nearest_site(x, y, seed);
                let (bx, by) = lattice_point(best, SITE_SPACING, SITE_JITTER, seed ^ SITE_SEED);
                let d = (bx - x).hypot(by - y);
                for id in cells_around(site_cell_for(x, y), 3) {
                    let (sx, sy) = lattice_point(id, SITE_SPACING, SITE_JITTER, seed ^ SITE_SEED);
                    assert!((sx - x).hypot(sy - y) >= d - 1e-9);
                }
            }
        }
    }
}
