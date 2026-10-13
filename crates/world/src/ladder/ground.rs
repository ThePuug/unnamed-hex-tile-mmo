//! The world's stack read as the ladder's ground, for the probes (INV-009).
//!
//! Cell (i, j) of a level `w` tiles wide is centred on the tile
//! `origin + (i, j)·w`, so the origin is a centre at every level. A cell's
//! tiles are its descendants: the nine children of each cell the ladder
//! nests, down to the tiles, so every tile lies in one cell of each level
//! and a cell holds `w²` of them. Its rock and its plate are those at its
//! centre.

use dashmap::DashMap;

use super::geom::descendants;
use super::Ground;
use crate::events::thrusting::Outlines;
use crate::events::Composite;
use crate::hex_to_world;

/// A cell's centre and the midpoints of its six edges, as tile offsets.
fn samples(w: i64) -> Vec<(i64, i64)> {
    if w == 1 {
        return vec![(0, 0)];
    }
    let h = w / 2;
    vec![(0, 0), (h, 0), (-h, 0), (0, h), (0, -h), (h, -h), (-h, h)]
}

/// What a cell read once: each layer's value, the rock's erodibility and
/// the plate's age.
type Read = (Vec<f64>, f64, f64);

/// The stack as two layers: thrusting's relief averaged over the cell, and
/// everything else as the stack composes it, the mean over the cell's centre
/// and the midpoints of its edges.
pub struct Layered<'c> {
    stack: &'c Composite,
    outlines: Outlines,
    origin: (i64, i64),
    /// How many levels down thrust's relief is averaged over: at most
    /// `9^depth` reads a cell, the tiles themselves where the cell has
    /// fewer.
    depth: u32,
    memo: DashMap<(i64, i64, i64), Read>,
}

impl<'c> Layered<'c> {
    /// The stack's ground around `origin`, its outlines built over a box
    /// `half` tiles across either way.
    pub fn new(stack: &'c Composite, origin: (i32, i32), half: f64, seed: u64, depth: u32) -> Self {
        let (cx, cy) = hex_to_world(origin.0, origin.1);
        Layered { stack, outlines: Outlines::in_box(cx, cy, half, seed), origin: (origin.0 as i64, origin.1 as i64), depth, memo: DashMap::new() }
    }

    /// Thrust's relief at a tile offset from the origin.
    fn relief(&self, q: i64, r: i64) -> f64 {
        let (wx, wy) = hex_to_world((self.origin.0 + q) as i32, (self.origin.1 + r) as i32);
        self.outlines.at(wx, wy).map_or(0.0, |at| self.outlines.relief_of(&at))
    }

    /// Thrust's relief averaged over cell (i, j) of the level `w` wide, read
    /// at its descendants `depth` levels down, or at its tiles where those
    /// are fewer.
    pub fn thrust_mean(&self, w: i64, i: i64, j: i64, depth: u32) -> f64 {
        let depth = depth.min(w.ilog(3));
        let offs = descendants(w, depth);
        offs.iter().map(|&(dq, dr)| self.relief(i * w + dq, j * w + dr)).sum::<f64>() / offs.len() as f64
    }

    fn cell(&self, w: i64, i: i64, j: i64) -> Read {
        if let Some(hit) = self.memo.get(&(w, i, j)) {
            return hit.clone();
        }
        let (q, r) = (self.origin.0 + i * w, self.origin.1 + j * w);
        let offs = samples(w);
        let (mut fields, mut erod) = (0.0, 1.0);
        for &(dq, dr) in &offs {
            let (tq, tr) = ((q + dq) as i32, (r + dr) as i32);
            let (view, outs) = self.stack.outputs_at(tq, tr);
            let own = outs.iter().find(|(name, _)| *name == "thrusting").and_then(|(_, o)| o.as_ref()).map_or(0.0, |o| o.elevation_delta);
            fields += view.elevation - own;
            if (dq, dr) == (0, 0) {
                erod = view.rock.map_or(1.0, |rock| rock.erodibility());
            }
        }
        let (wx, wy) = hex_to_world(q as i32, r as i32);
        let age = self.outlines.at(wx, wy).map_or(0.0, |at| at.plate.age);
        let read = (vec![fields / offs.len() as f64, self.thrust_mean(w, i, j, self.depth)], erod, age);
        self.memo.insert((w, i, j), read.clone());
        read
    }
}

impl Ground for Layered<'_> {
    fn layers(&self) -> usize {
        2
    }

    fn read(&self, w: i64, i: i64, j: i64, out: &mut [f64]) {
        out.copy_from_slice(&self.cell(w, i, j).0);
    }

    fn erodibility(&self, w: i64, i: i64, j: i64) -> f64 {
        self.cell(w, i, j).1
    }

    fn age(&self, w: i64, i: i64, j: i64) -> f64 {
        self.cell(w, i, j).2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::geom::NB;

    /// A cell's descendants are its tiles once the depth reaches them: `w²`
    /// distinct tiles.
    #[test]
    fn descendants_are_the_cells_tiles() {
        for w in [3i64, 9, 27] {
            let mut d = descendants(w, w.ilog(3));
            assert_eq!(d.len() as i64, w * w);
            d.sort_unstable();
            d.dedup();
            assert_eq!(d.len() as i64, w * w);
        }
    }

    /// Neighbouring cells' tiles never overlap: the descendants partition
    /// the plane.
    #[test]
    fn neighbouring_cells_share_no_tile() {
        let w = 27i64;
        let own: std::collections::HashSet<(i64, i64)> = descendants(w, 3).into_iter().collect();
        for (di, dj) in NB {
            for (q, r) in descendants(w, 3) {
                assert!(!own.contains(&(q + di * w, r + dj * w)), "a tile in two cells");
            }
        }
    }
}
