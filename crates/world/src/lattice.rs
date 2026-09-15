//! The world lattice — the hex node lattice every outline is drawn on.
//!
//! Coasts, plate edges, belt fronts and channels are chains of nodes on one
//! lattice, coarser than the tiles and aligned with them, so every bend in
//! the world is 60° or 120° and a river mouth, a front's end and a coast's
//! corner can share a vertex. Elevation is not on the lattice: heights are
//! continuous functions of distance to the chains.
//!
//! A straight line between two nodes is drawn as runs along the lattice's
//! directions. An outline holds a run for [`RUN_MIN`] nodes before it turns
//! wherever the line is long enough to allow it, so its corners are sparse;
//! only the last run of a short line may fall short. The path between two
//! nodes is a pure function of the two, and the same from either end, which
//! is what lets two cells draw one edge and agree on every node of it.

use crate::{hex_to_world, world_to_hex};

/// A node's lattice coordinates. Its tile is `(i × NODE_SPACING, j × NODE_SPACING)`.
pub type NodeKey = (i32, i32);

/// Node spacing in tiles: the narrowest valley drainage reads as a valley,
/// and the finest an outline turns. A sheet spacing holds one and a half of
/// these, so the trough between two ranges is resolved and their flanks are
/// not.
pub const NODE_SPACING: i32 = 525;

/// Nodes a coast or a front runs straight before it may turn: five nodes,
/// about two sheet spacings, so a wedge's ranges get a straight run between
/// corners.
pub const RUN_MIN: i32 = 5;

/// The farthest a path node stands from the straight line between its ends,
/// in world units: a minimum run of steps standing square to the line. The
/// minor direction's steps are dealt into runs between the major runs, and
/// the line always crosses within a run minimum's worth of them. A test
/// holds it over every displacement up to a hundred nodes, twice the longest
/// plate edge.
pub const PATH_SWING: f64 = RUN_MIN as f64 * NODE_SPACING as f64 * SIN_60;

/// sin 60°, the height of a unit step square to a lattice direction.
const SIN_60: f64 = 0.866_025_403_784_438_6;

/// The six neighbours in angular order, 60° apart from `+x` round to `300°`,
/// so consecutive entries bound one facet.
pub const DIRECTIONS: [(i32, i32); 6] = [(1, 0), (0, 1), (-1, 1), (-1, 0), (0, -1), (1, -1)];

pub fn node_tile(key: NodeKey) -> (i32, i32) {
    (key.0 * NODE_SPACING, key.1 * NODE_SPACING)
}

/// A node's position in world units.
pub fn node_world(key: NodeKey) -> (f64, f64) {
    let (q, r) = node_tile(key);
    hex_to_world(q, r)
}

/// The node nearest a position.
pub fn nearest_node(wx: f64, wy: f64) -> NodeKey {
    let (q, r) = world_to_hex(wx, wy);
    let s = NODE_SPACING as f64;
    hex_round(q as f64 / s, r as f64 / s)
}

pub fn hex_round(fq: f64, fr: f64) -> (i32, i32) {
    let fs = -fq - fr;
    let (mut q, mut r, s) = (fq.round(), fr.round(), fs.round());
    let (dq, dr, ds) = ((q - fq).abs(), (r - fr).abs(), (s - fs).abs());
    if dq > dr && dq > ds {
        q = -r - s;
    } else if dr > ds {
        r = -q - s;
    }
    (q as i32, r as i32)
}

pub fn hex_distance(a: NodeKey, b: NodeKey) -> i32 {
    let (dq, dr) = (a.0 - b.0, a.1 - b.1);
    dq.abs().max(dr.abs()).max((dq + dr).abs())
}

/// The two lattice directions that bracket a displacement, with how many
/// steps of each compose it: every axial displacement is a non-negative
/// combination of two adjacent directions, and exactly one such pair unless
/// the displacement lies on a direction, where either neighbouring pair
/// serves and the first in angular order is taken.
fn decompose(dq: i32, dr: i32) -> ((usize, i32), (usize, i32)) {
    for i in 0..6 {
        let (a, b) = (DIRECTIONS[i], DIRECTIONS[(i + 1) % 6]);
        // Solve dq = m·a.0 + n·b.0, dr = m·a.1 + n·b.1 by Cramer's rule; the
        // determinant of two adjacent directions is ±1.
        let det = a.0 * b.1 - a.1 * b.0;
        let m = (dq * b.1 - dr * b.0) * det;
        let n = (a.0 * dr - a.1 * dq) * det;
        if m >= 0 && n >= 0 {
            return ((i, m), ((i + 1) % 6, n));
        }
    }
    unreachable!("an axial displacement always lies in one sextant")
}

/// The chain of nodes from one node to another, drawn as runs along the two
/// directions that bracket the line: the direction with more steps opens and
/// closes the path and the other alternates with it, each run at least
/// [`RUN_MIN`] nodes wherever the counts allow. The chain includes both ends
/// and is the same chain reversed from the other end.
pub fn lattice_path(from: NodeKey, to: NodeKey) -> Vec<NodeKey> {
    if to < from {
        let mut path = lattice_path(to, from);
        path.reverse();
        return path;
    }
    let ((di, m), (dj, n)) = decompose(to.0 - from.0, to.1 - from.1);
    let (major, minor, a, b) = if m >= n { (di, dj, m, n) } else { (dj, di, n, m) };
    // k minor runs sit between k+1 major runs. As many alternations as keep
    // every run at RUN_MIN, at least one.
    let mut k = (b / RUN_MIN).max(1);
    while k > 1 && a / (k + 1) < RUN_MIN {
        k -= 1;
    }
    let mut runs: Vec<(usize, i32)> = Vec::with_capacity((2 * k + 1) as usize);
    for i in 0..=k {
        runs.push((major, share(a, k + 1, i)));
        if i < k {
            runs.push((minor, share(b, k, i)));
        }
    }
    let mut path = vec![from];
    let mut at = from;
    for (dir, count) in runs {
        let d = DIRECTIONS[dir];
        for _ in 0..count {
            at = (at.0 + d.0, at.1 + d.1);
            path.push(at);
        }
    }
    debug_assert_eq!(*path.last().unwrap(), to);
    path
}

/// The `i`-th of `parts` shares of `total`: as even as integers allow, the
/// remainder on the first runs. The path is always drawn from the lesser
/// node, so which runs take the remainder never depends on the caller.
fn share(total: i32, parts: i32, i: i32) -> i32 {
    total / parts + if i < total % parts { 1 } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every displacement decomposes into two adjacent directions with
    /// non-negative counts that recompose it.
    #[test]
    fn displacements_decompose() {
        for dq in -30..=30 {
            for dr in -30..=30 {
                let ((i, m), (j, n)) = decompose(dq, dr);
                let (a, b) = (DIRECTIONS[i], DIRECTIONS[j]);
                assert_eq!((m * a.0 + n * b.0, m * a.1 + n * b.1), (dq, dr), "({dq}, {dr})");
                assert!(m >= 0 && n >= 0);
                assert_eq!((i + 1) % 6, j);
            }
        }
    }

    /// A path lands on its target, steps one node at a time, and is the
    /// same path reversed from the other end.
    #[test]
    fn paths_connect_and_reverse() {
        let cases = [((0, 0), (17, 3)), ((2, -5), (-9, 20)), ((0, 0), (0, 0)), ((4, 4), (5, 4)), ((0, 0), (40, -20))];
        for (p, q) in cases {
            let path = lattice_path(p, q);
            assert_eq!(path[0], p);
            assert_eq!(*path.last().unwrap(), q);
            for w in path.windows(2) {
                assert_eq!(hex_distance(w[0], w[1]), 1, "a step of more than one node");
            }
            assert_eq!(path.len() as i32 - 1, hex_distance(p, q), "a path longer than the line");
            let mut back = lattice_path(q, p);
            back.reverse();
            assert_eq!(path, back);
        }
    }

    /// Runs hold for RUN_MIN nodes wherever the line is long enough: the
    /// count of direction changes never exceeds what RUN_MIN allows.
    #[test]
    fn runs_are_long() {
        let path = lattice_path((0, 0), (40, 13));
        let mut runs: Vec<i32> = Vec::new();
        let mut last: Option<(i32, i32)> = None;
        for w in path.windows(2) {
            let d = (w[1].0 - w[0].0, w[1].1 - w[0].1);
            if last == Some(d) { *runs.last_mut().unwrap() += 1 } else { runs.push(1) }
            last = Some(d);
        }
        assert!(runs.len() >= 3, "a long oblique line has to alternate: {runs:?}");
        assert!(runs.iter().all(|&r| r >= RUN_MIN), "a run shorter than RUN_MIN: {runs:?}");
    }

    /// No path node stands farther from its straight line than PATH_SWING,
    /// and the bound is not slack.
    #[test]
    fn paths_stay_near_their_lines() {
        let mut worst = 0.0f64;
        for dq in -100..=100 {
            for dr in -100..=100 {
                let to = (dq, dr);
                if to == (0, 0) || hex_distance((0, 0), to) > 100 { continue }
                let (x1, y1) = node_world(to);
                let len = x1.hypot(y1);
                for n in lattice_path((0, 0), to) {
                    let (x, y) = node_world(n);
                    worst = worst.max((x * y1 - y * x1).abs() / len);
                }
            }
        }
        assert!(worst <= PATH_SWING, "a path node {worst:.0} off its line, past PATH_SWING {PATH_SWING:.0}");
        assert!(worst > 0.75 * PATH_SWING, "PATH_SWING is slack: the farthest node is {worst:.0}");
    }
}
