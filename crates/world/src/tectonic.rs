//! Tectonic plates — the primary object of the world: Voronoi cells around
//! seeds a few of which make a continent, each continental or oceanic, with
//! every edge between two of them drawn on the lattice.
//!
//! # Claims
//!
//! A plate is a Voronoi cell around a seed on a jittered hex lattice. It is
//! continental when a continent site claims it (`crate::continents`), so
//! continental plates come as the compact clusters the sites grow, and a
//! continent is one site's claim. It has an age by a slow field sampled at
//! its seed: how far erosion has carried its landscape, which drainage,
//! lithology and migration read and nothing here uses.
//! An edge between two plates is the straight line between two Voronoi
//! vertices, drawn on the lattice as runs with sparse corners. A coast is an
//! edge with one continental side and one oceanic.
//!
//! # Exactness
//!
//! An edge is computed from either of its plates and has to come out
//! identical from both, or two cells publish two versions of one edge. Its
//! vertices are circumcentres of the three seeds that meet there, computed
//! from the seeds in id order, and its chain is drawn from its lesser vertex.
//! The polygon clipping only decides which plates meet where; no number is
//! read off its intersections.

use crate::lattice::{lattice_path, nearest_node, NodeKey};
use crate::noise::{hash_f64, simplex_2d};

/// Seed spacing in world units: a plate is a province, and a continent a
/// cluster of several.
pub const PLATE_SPACING: f64 = 12_500.0;

/// How far a seed is displaced from its lattice point, as a share of the
/// spacing, in each axis. What stops every plate being the same hexagon.
pub const PLATE_JITTER: f64 = 0.35;

/// Wavelength of the field that gives a plate its age: a few plates, so a
/// continent holds plates of different ages, a gorge-cut one beside one
/// still filling its basins, and neighbours often share one.
pub const AGE_WAVELENGTH: f64 = 3.0 * PLATE_SPACING;

/// The level of that field at which a plate is fully aged; at its negative
/// the plate is new, and between them age runs linearly.
///
/// EMPIRICAL: a tenth of plates new and a tenth fully aged, the quartiles
/// near 0.2 and 0.8, measured by `plate_probe`.
pub const AGE_SPREAD: f64 = 0.5;

/// What a new plate's erosion has done as a share of an aged plate's, down
/// and sideways alike: the narrow cut of a young orogen, its plateau
/// surface largely intact, its cuestas barely etched.
///
/// Tuning, not yet judged in the viewer.
pub const YOUNG_SHARE: f64 = 0.5;

/// The aged share: what a plate of `age` has done of an aged plate's work.
pub fn aged(age: f64) -> f64 {
    YOUNG_SHARE + (1.0 - YOUNG_SHARE) * age.clamp(0.0, 1.0)
}

/// The farthest any ground of a plate lies from its seed, in world units.
///
/// EMPIRICAL, measured by `plate_reach_is_bounded` over thousands of plates
/// at this jitter: the largest seed-to-vertex distance found, with margin.
/// Bounds how far a plate's edges can lie from its seed, which is what every
/// gather of plates around a position is sized by.
pub const PLATE_REACH: f64 = 12_000.0;

pub(crate) const ROW: f64 = 0.866_025_403_784_438_6;
const JITTER_SEED_X: u64 = 0x506C_6174_655F_5F58; // "Plate__X"
const JITTER_SEED_Y: u64 = 0x506C_6174_655F_5F59; // "Plate__Y"
const AGE_SEED: u64 = 0x506C_6174_6541_6765; // "PlateAge"

/// A plate's identity: its seed's lattice cell.
pub type PlateId = (i32, i32);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plate {
    pub id: PlateId,
    pub wx: f64,
    pub wy: f64,
    pub continental: bool,
    /// How far erosion has carried the plate's landscape, from 0, an orogen
    /// still filling its basins, to 1, one drained through gorges.
    pub age: f64,
}

/// The cell of a hex lattice of `spacing`, odd rows shifted half a cell,
/// whose undisplaced centre is nearest a position.
pub(crate) fn cell_for(wx: f64, wy: f64, spacing: f64) -> (i32, i32) {
    let cr = (wy / (spacing * ROW)).round() as i32;
    let odd_shift = if cr & 1 != 0 { spacing * 0.5 } else { 0.0 };
    let cq = ((wx - odd_shift) / spacing).round() as i32;
    (cq, cr)
}

/// A cell's point on a hex lattice of `spacing`, displaced by up to
/// `jitter` of the spacing in each axis by a hash of the cell.
pub(crate) fn lattice_point(id: (i32, i32), spacing: f64, jitter: f64, seed: u64) -> (f64, f64) {
    let (cq, cr) = id;
    let odd_shift = if cr & 1 != 0 { spacing * 0.5 } else { 0.0 };
    let cx = cq as f64 * spacing + odd_shift;
    let cy = cr as f64 * spacing * ROW;
    let jx = (hash_f64(cq as i64, cr as i64, seed ^ JITTER_SEED_X) * 2.0 - 1.0) * jitter * spacing;
    let jy = (hash_f64(cq as i64, cr as i64, seed ^ JITTER_SEED_Y) * 2.0 - 1.0) * jitter * spacing;
    (cx + jx, cy + jy)
}

/// The lattice cell whose undisplaced centre is nearest a position.
pub fn plate_cell_for(wx: f64, wy: f64) -> PlateId {
    cell_for(wx, wy, PLATE_SPACING)
}

/// The seed of a plate: its lattice point, jittered.
pub fn seed_point(id: PlateId, seed: u64) -> (f64, f64) {
    lattice_point(id, PLATE_SPACING, PLATE_JITTER, seed)
}

pub fn plate(id: PlateId, seed: u64) -> Plate {
    let (wx, wy) = seed_point(id, seed);
    let aged = simplex_2d(wx / AGE_WAVELENGTH, wy / AGE_WAVELENGTH, seed ^ AGE_SEED);
    let age = ((aged + AGE_SPREAD) / (2.0 * AGE_SPREAD)).clamp(0.0, 1.0);
    Plate { id, wx, wy, continental: crate::continents::is_continental(id, seed), age }
}

/// Lattice cells within `rings` of a cell, in odd-r offset coordinates.
pub(crate) fn cells_around(id: (i32, i32), rings: i32) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    for dr in -rings..=rings {
        let r = id.1 + dr;
        // Rows shift by half a cell every other row, so the q range of a ring
        // widens by one on the side the row is shifted toward.
        let shift = (r & 1) - (id.1 & 1);
        for dq in -rings..=rings {
            let q = id.0 + dq;
            let (dx, dy) = ((dq as f64) + 0.5 * shift as f64, dr as f64 * ROW);
            if dx.hypot(dy) <= rings as f64 + 0.01 {
                out.push((q, r));
            }
        }
    }
    out
}

/// The plate a position lies in: the nearest seed. The home cell's seed
/// lies within half a spacing and the jitter of the position, and a
/// cell's seed in the second ring's row lies at least a row's height
/// twice over less the jitter away, farther than that, so the contest is
/// over the home cell and two rings, on the seeds alone; the winner is
/// built once.
pub fn plate_at(wx: f64, wy: f64, seed: u64) -> Plate {
    let home = plate_cell_for(wx, wy);
    let mut best: Option<(f64, PlateId)> = None;
    for id in cells_around(home, 2) {
        let (sx, sy) = seed_point(id, seed);
        let d = (sx - wx).hypot(sy - wy);
        if best.map_or(true, |(bd, _)| d < bd) {
            best = Some((d, id));
        }
    }
    plate(best.expect("a lattice cell has neighbours").1, seed)
}

/// Every plate whose ground can reach within `radius` of a position: seeds
/// within `radius` plus [`PLATE_REACH`].
pub fn plates_near(wx: f64, wy: f64, radius: f64, seed: u64) -> Vec<Plate> {
    let home = plate_cell_for(wx, wy);
    let reach = radius + PLATE_REACH + PLATE_JITTER * PLATE_SPACING;
    let rings = (reach / (PLATE_SPACING * ROW)).ceil() as i32 + 1;
    let mut out = Vec::new();
    for id in cells_around(home, rings) {
        let p = plate(id, seed);
        if (p.wx - wx).hypot(p.wy - wy) <= reach {
            out.push(p);
        }
    }
    out.sort_by_key(|p| p.id);
    out
}

/// One edge of the plate graph: the straight Voronoi edge between two
/// plates, and its chain on the lattice.
#[derive(Clone, Debug)]
pub struct Edge {
    /// The two plates, `a.id < b.id`.
    pub a: Plate,
    pub b: Plate,
    /// The Voronoi vertices the edge runs between, the lesser by `(x, y)`
    /// first.
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    /// The edge on the lattice, from the node nearest `(x0, y0)` to the node
    /// nearest `(x1, y1)`.
    pub chain: Vec<NodeKey>,
}

impl Edge {
    /// Midpoint of the straight edge: what decides which cell owns it.
    pub fn mid(&self) -> (f64, f64) {
        (0.5 * (self.x0 + self.x1), 0.5 * (self.y0 + self.y1))
    }

    pub fn length(&self) -> f64 {
        (self.x1 - self.x0).hypot(self.y1 - self.y0)
    }

    /// Whether the edge is a coast: one side continental, the other oceanic.
    pub fn is_coast(&self) -> bool {
        self.a.continental != self.b.continental
    }

    pub fn ids(&self) -> (PlateId, PlateId) {
        (self.a.id, self.b.id)
    }
}

/// Circumcentre of three seeds, computed from them in id order so every
/// plate that meets there computes the same point bit for bit.
fn circumcentre(mut ids: [PlateId; 3], seed: u64) -> (f64, f64) {
    ids.sort();
    let [(ax, ay), (bx, by), (cx, cy)] = ids.map(|id| seed_point(id, seed));
    let d = 2.0 * (ax * (by - cy) + bx * (cy - ay) + cx * (ay - by));
    let (a2, b2, c2) = (ax * ax + ay * ay, bx * bx + by * by, cx * cx + cy * cy);
    let ux = (a2 * (by - cy) + b2 * (cy - ay) + c2 * (ay - by)) / d;
    let uy = (a2 * (cx - bx) + b2 * (ax - cx) + c2 * (bx - ax)) / d;
    (ux, uy)
}

/// One side of a plate's polygon: the neighbour across it, its vertices,
/// the lesser by `(x, y)` first, and its chain on the lattice.
struct Side {
    other: PlateId,
    p0: (f64, f64),
    p1: (f64, f64),
    chain: Vec<NodeKey>,
}

/// The sides of one plate: its Voronoi polygon, clipped by the bisector of
/// every seed within three rings, read for which neighbour makes each side.
/// Geometry alone, from the seeds: what a claim grows over, so it never
/// asks what a plate is.
fn sides(id: PlateId, seed: u64) -> Vec<Side> {
    let (sx, sy) = seed_point(id, seed);
    // A square far past any vertex; every side of it is clipped away.
    let big = 3.0 * PLATE_SPACING;
    let mut pts: Vec<(f64, f64)> = vec![(sx - big, sy - big), (sx + big, sy - big), (sx + big, sy + big), (sx - big, sy + big)];
    // The neighbour whose bisector makes the side ending at each vertex.
    let mut side: Vec<Option<PlateId>> = vec![None; 4];

    for nid in cells_around(id, 3) {
        if nid == id { continue }
        let (nx, ny) = seed_point(nid, seed);
        // Inside the half-plane nearer to me than to the neighbour.
        let (hx, hy) = (nx - sx, ny - sy);
        let h0 = 0.5 * (nx * nx + ny * ny - sx * sx - sy * sy);
        let inside = |p: (f64, f64)| hx * p.0 + hy * p.1 - h0 <= 0.0;
        let cross = |p: (f64, f64), q: (f64, f64)| {
            let fp = hx * p.0 + hy * p.1 - h0;
            let fq = hx * q.0 + hy * q.1 - h0;
            let t = fp / (fp - fq);
            (p.0 + t * (q.0 - p.0), p.1 + t * (q.1 - p.1))
        };
        let n = pts.len();
        let mut new_pts = Vec::with_capacity(n + 2);
        let mut new_side = Vec::with_capacity(n + 2);
        for i in 0..n {
            let (p, q) = (pts[i], pts[(i + 1) % n]);
            let label = side[(i + 1) % n];
            match (inside(p), inside(q)) {
                (true, true) => { new_pts.push(q); new_side.push(label); }
                (true, false) => { new_pts.push(cross(p, q)); new_side.push(label); }
                (false, true) => {
                    new_pts.push(cross(p, q)); new_side.push(Some(nid));
                    new_pts.push(q); new_side.push(label);
                }
                (false, false) => {}
            }
        }
        pts = new_pts;
        side = new_side;
        if pts.is_empty() { return Vec::new() }
    }

    let n = pts.len();
    let mut sides = Vec::with_capacity(n);
    for k in 0..n {
        let (Some(prev), Some(this), Some(next)) = (side[(k + n - 1) % n], side[k], side[(k + 1) % n]) else {
            debug_assert!(false, "a plate polygon side survived from the initial square");
            continue;
        };
        // The side ending at vertex k is against `this`; its ends are the
        // vertices shared with the sides before and after.
        let v0 = circumcentre([id, prev, this], seed);
        let v1 = circumcentre([id, this, next], seed);
        if (v1.0 - v0.0).hypot(v1.1 - v0.1) < 1e-6 { continue }
        let (p0, p1) = if (v0.0, v0.1) <= (v1.0, v1.1) { (v0, v1) } else { (v1, v0) };
        // A side shorter than the lattice resolves to a single node: the two
        // plates touch at a point and share no boundary.
        let chain = lattice_path(nearest_node(p0.0, p0.1), nearest_node(p1.0, p1.1));
        if chain.len() < 2 { continue }
        sides.push(Side { other: this, p0, p1, chain });
    }
    sides
}

/// The plates across a plate's sides: the ones it shares an edge with, so
/// a coast or a strait lies between it and each. Two plates that touch at
/// a point are not neighbours.
pub fn neighbours_of(id: PlateId, seed: u64) -> Vec<PlateId> {
    sides(id, seed).into_iter().map(|s| s.other).collect()
}

/// The edges of one plate, one per side, with both plates built.
pub fn edges_of(id: PlateId, seed: u64) -> Vec<Edge> {
    let me = plate(id, seed);
    sides(id, seed)
        .into_iter()
        .map(|s| {
            let other = plate(s.other, seed);
            let (a, b) = if me.id < other.id { (me, other) } else { (other, me) };
            Edge { a, b, x0: s.p0.0, y0: s.p0.1, x1: s.p1.0, y1: s.p1.1, chain: s.chain }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: u64 = 0x9E3779B97F4A7C15;

    /// An edge computed from either plate is the same edge: same vertices,
    /// same chain.
    #[test]
    fn edges_agree_from_both_plates() {
        let mut checked = 0;
        for cq in -6..=6 {
            for cr in -6..=6 {
                for e in edges_of((cq, cr), S) {
                    let other = if e.a.id == (cq, cr) { e.b.id } else { e.a.id };
                    let twin = edges_of(other, S)
                        .into_iter()
                        .find(|t| t.ids() == e.ids())
                        .expect("the neighbour sees the edge too");
                    assert_eq!((e.x0, e.y0, e.x1, e.y1), (twin.x0, twin.y0, twin.x1, twin.y1));
                    assert_eq!(e.chain, twin.chain);
                    checked += 1;
                }
            }
        }
        assert!(checked > 500, "{checked} edges checked");
    }

    /// Every vertex of a plate lies within the stated reach of its seed, and
    /// the position of every vertex is nearer to its three plates than to any
    /// other, which is what makes it a Voronoi vertex.
    #[test]
    fn plate_reach_is_bounded() {
        let mut worst: f64 = 0.0;
        for cq in -12..=12 {
            for cr in -12..=12 {
                let p = plate((cq, cr), S);
                for e in edges_of((cq, cr), S) {
                    for (vx, vy) in [(e.x0, e.y0), (e.x1, e.y1)] {
                        worst = worst.max((vx - p.wx).hypot(vy - p.wy));
                        let at = plate_at(vx, vy, S);
                        let d_at = (at.wx - vx).hypot(at.wy - vy);
                        let d_me = (p.wx - vx).hypot(p.wy - vy);
                        assert!(d_at <= d_me + 1e-6, "a vertex nearer to a fourth plate at ({vx:.0}, {vy:.0})");
                    }
                }
            }
        }
        assert!(worst <= PLATE_REACH, "a vertex {worst:.0} from its seed, past PLATE_REACH");
        assert!(worst > 0.5 * PLATE_REACH, "PLATE_REACH is slack: the farthest vertex is {worst:.0}");
    }

    /// A plate's edges close into a ring on the lattice: each chain end is
    /// shared by exactly two of its edges' chains.
    #[test]
    fn edges_close() {
        for cq in -3..=3 {
            for cr in -3..=3 {
                let edges = edges_of((cq, cr), S);
                assert!(edges.len() >= 4, "a plate with {} edges", edges.len());
                let mut ends: Vec<NodeKey> = edges
                    .iter()
                    .flat_map(|e| [e.chain[0], *e.chain.last().unwrap()])
                    .collect();
                ends.sort();
                for pair in ends.chunks(2) {
                    assert_eq!(pair[0], pair[1], "a vertex on only one edge of plate ({cq}, {cr})");
                }
            }
        }
    }

    /// The nearest seed is found, against a wide scan.
    #[test]
    fn plate_at_finds_the_nearest_seed() {
        for i in 0..40 {
            for j in 0..40 {
                let (x, y) = (i as f64 * 3_333.0 - 60_000.0, j as f64 * 2_777.0 - 50_000.0);
                let p = plate_at(x, y, S);
                let d = (p.wx - x).hypot(p.wy - y);
                for other in plates_near(x, y, 0.0, S) {
                    assert!((other.wx - x).hypot(other.wy - y) >= d - 1e-9);
                }
            }
        }
    }
}
