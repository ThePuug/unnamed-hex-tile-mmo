//! A 3D hexagonal coordinate: axial `(q, r)` on the grid and `z`, the
//! elevation. The grid's third axis is derived, `s = -q - r`, so a
//! coordinate is never off the grid.
//!
//! The grid's metric is the hex distance, the largest of the three cube
//! deltas ([`hex_distance`], `flat_distance`), with the elevation gap
//! added for `distance`. The six neighbour offsets are [`DIRECTIONS`];
//! rings, walks round a tile and lines straight on are built from them.
//!
//! ```rust
//! use qrz::Qrz;
//!
//! let origin = Qrz { q: 0, r: 0, z: 0 };
//! let east = Qrz { q: 1, r: 0, z: 0 };
//! assert_eq!(origin.flat_distance(&east), 1);
//! assert_eq!(origin.neighbors().len(), 6);
//! ```

use std::ops::{Add, Mul, Sub};

use serde::{Deserialize, Serialize};

/// The six neighbour offsets, in order round the tile — each beside the
/// next, the last beside the first — on the flat-top grid NW, SW, S, SE,
/// NE, N, north being −z. `ring` and `circling` turn the same way.
pub const DIRECTIONS: [Qrz; 6] = [
    Qrz { q: -1, r: 0, z: 0 },
    Qrz { q: -1, r: 1, z: 0 },
    Qrz { q: 0, r: 1, z: 0 },
    Qrz { q: 1, r: 0, z: 0 },
    Qrz { q: 1, r: -1, z: 0 },
    Qrz { q: 0, r: -1, z: 0 },
];

/// A tile: axial `(q, r)` on the grid, `z` its elevation.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct Qrz {
    pub q: i32,
    pub r: i32,
    pub z: i32,
}

impl Qrz {
    pub const Z: Qrz = Qrz { q: 0, r: 0, z: 1 };

    /// Hex distance on the grid, elevation ignored.
    pub fn flat_distance(&self, other: &Qrz) -> i32 {
        hex_distance((self.q, self.r), (other.q, other.r))
    }

    /// Hex distance on the grid plus the elevation gap.
    pub fn distance(&self, other: &Qrz) -> i32 {
        self.flat_distance(other) + (self.z - other.z).abs()
    }

    /// The `6 × radius` tiles at flat distance `radius`, at this tile's z,
    /// in order round the ring so neighbouring entries are neighbouring
    /// tiles, the last beside the first. Entry `radius × i` is the corner
    /// out along `DIRECTIONS[i]`, so at radius 1 the order is `DIRECTIONS`'.
    /// Radius 0 is this tile alone.
    pub fn ring(&self, radius: u32) -> Vec<Qrz> {
        if radius == 0 {
            return vec![*self];
        }
        let mut at = *self + DIRECTIONS[0] * radius as i32;
        let mut ring = Vec::with_capacity(6 * radius as usize);
        for side in 0..6 {
            for _ in 0..radius {
                ring.push(at);
                at = at + DIRECTIONS[(side + 2) % 6];
            }
        }
        ring
    }

    /// The tiles a walk round this tile passes from `from` to `to`: along
    /// its ring at `to`'s flat distance, the shorter way round, from the
    /// ring tile nearest `from` — `from` itself when it stands on the ring —
    /// through `to`, the start left out. Neighbouring entries are
    /// neighbouring tiles, all at this tile's z. Empty when the walk starts
    /// on `to`, or `to` is this tile.
    pub fn circling(&self, from: Qrz, to: Qrz) -> Vec<Qrz> {
        let radius = self.flat_distance(&to) as u32;
        if radius == 0 {
            return Vec::new();
        }
        let ring = self.ring(radius);
        let flat = |tile: &Qrz| Qrz { z: self.z, ..*tile };
        let (from, to) = (flat(&from), flat(&to));
        let n = ring.len();
        let start = (0..n).min_by_key(|&i| ring[i].flat_distance(&from)).expect("a ring has tiles");
        let end = ring.iter().position(|tile| *tile == to).expect("`to` stands on its own ring");
        let ahead = (end + n - start) % n;
        let (steps, step) = if ahead <= n - ahead { (ahead, 1) } else { (n - ahead, n - 1) };
        (1..=steps).map(|k| ring[(start + k * step) % n]).collect()
    }

    /// The tiles straight on from this one along the line from `from`
    /// through it: `steps` of them at this tile's z, each a neighbour of the
    /// one before and a step further from `from`. Empty where `from` is in
    /// this tile's own column, which no line leads on from.
    pub fn beyond(&self, from: Qrz, steps: usize) -> Vec<Qrz> {
        let span = self.flat_distance(&from);
        if span == 0 {
            return Vec::new();
        }
        let (dq, dr) = ((self.q - from.q) as f64 / span as f64, (self.r - from.r) as f64 / span as f64);
        (1..=steps).map(|k| {
            let on = round(dq * k as f64, dr * k as f64, 0.0);
            Qrz { q: self.q + on.q, r: self.r + on.r, z: self.z }
        }).collect()
    }

    /// The six neighbours at this tile's z, in `DIRECTIONS`' order.
    pub fn neighbors(&self) -> Vec<Qrz> {
        DIRECTIONS.map(|d| *self + d).to_vec()
    }
}

/// Hex distance between two axial coordinates: the largest of the three
/// cube deltas, `max(|dq|, |dr|, |dq + dr|)`, the fewest steps between
/// the tiles.
pub fn hex_distance(a: (i32, i32), b: (i32, i32)) -> i32 {
    let (dq, dr) = (a.0 - b.0, a.1 - b.1);
    dq.abs().max(dr.abs()).max((dq + dr).abs())
}

impl Mul<i32> for Qrz {
    type Output = Qrz;
    fn mul(self, rhs: i32) -> Self::Output {
        Qrz { q: self.q * rhs, r: self.r * rhs, z: self.z * rhs }
    }
}

impl Add<Qrz> for Qrz {
    type Output = Qrz;
    fn add(self, rhs: Qrz) -> Self::Output {
        Qrz { q: self.q + rhs.q, r: self.r + rhs.r, z: self.z + rhs.z }
    }
}

impl Sub<Qrz> for Qrz {
    type Output = Qrz;
    fn sub(self, rhs: Qrz) -> Self::Output {
        Qrz { q: self.q - rhs.q, r: self.r - rhs.r, z: self.z - rhs.z }
    }
}

/// The tile nearest a fractional coordinate: each axis rounded, and the
/// one that moved furthest re-derived from the other two, so the result
/// stays on the grid.
pub(crate) fn round(q0: f64, r0: f64, z0: f64) -> Qrz {
    let s0 = -q0-r0;
    let mut q = q0.round();
    let mut r = r0.round();
    let s = s0.round();

    let q_diff = (q - q0).abs();
    let r_diff = (r - r0).abs();
    let s_diff = (s - s0).abs();

    if q_diff > r_diff && q_diff > s_diff { q = -r-s; }
    else if r_diff > s_diff { r = -q-s; }

    Qrz { q: q as i32, r: r as i32, z: z0.round() as i32 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beyond_runs_straight_on_a_neighbour_at_a_time() {
        let here = Qrz { q: 2, r: -1, z: 3 };
        let east = DIRECTIONS[3];
        assert_eq!(here.beyond(here - east, 3), vec![here + east, here + east * 2, here + east * 3], "along an axis, its tiles");
        assert!(here.beyond(Qrz { z: 9, ..here }, 3).is_empty(), "no line leads on from its own column");
        for from in here.ring(1).into_iter().chain(here.ring(2)).chain(here.ring(5)) {
            let mut last = here;
            for tile in here.beyond(from, 6) {
                assert_eq!(tile.z, here.z);
                assert_eq!(tile.flat_distance(&last), 1, "from {from:?}: {tile:?} is no neighbour of {last:?}");
                assert_eq!(tile.flat_distance(&from), last.flat_distance(&from) + 1, "from {from:?}: each step is one further off");
                last = tile;
            }
        }
        // Opposite ways from opposite sides: it follows the line, whatever the compass
        let (ahead, behind) = (here.beyond(here - east, 1)[0], here.beyond(here + east, 1)[0]);
        assert_eq!(ahead - here, here - behind);
    }

    #[test]
    fn ring_runs_round_every_tile_at_its_radius() {
        let centre = Qrz { q: 3, r: -2, z: 4 };
        assert_eq!(centre.ring(0), vec![centre]);
        for radius in 1..=4u32 {
            let ring = centre.ring(radius);
            assert_eq!(ring.len(), 6 * radius as usize);
            let unique: std::collections::HashSet<_> = ring.iter().collect();
            assert_eq!(unique.len(), ring.len(), "radius {radius} repeats a tile");
            for (i, tile) in ring.iter().enumerate() {
                assert_eq!(tile.flat_distance(&centre), radius as i32);
                assert_eq!(tile.z, centre.z);
                assert_eq!(tile.flat_distance(&ring[(i + 1) % ring.len()]), 1, "radius {radius} breaks after entry {i}");
            }
            for (i, direction) in DIRECTIONS.iter().enumerate() {
                assert_eq!(ring[radius as usize * i], centre + *direction * radius as i32);
            }
        }
    }

    #[test]
    fn circling_walks_the_shorter_way_round_to_the_far_side() {
        let centre = Qrz { q: 3, r: -2, z: 4 };
        for radius in 1..=3i32 {
            for (i, direction) in DIRECTIONS.iter().enumerate() {
                let front = centre + *direction * radius;
                let back = centre + DIRECTIONS[(i + 3) % 6] * radius;
                let walk = centre.circling(front, back);
                assert_eq!(walk.len(), 3 * radius as usize, "half the ring at radius {radius}");
                assert_eq!(*walk.last().unwrap(), back);
                let mut at = front;
                for tile in &walk {
                    assert_eq!(tile.flat_distance(&centre), radius);
                    assert_eq!(tile.flat_distance(&at), 1, "a step to a neighbour");
                    at = *tile;
                }
            }
        }
        // From nearer in, the walk starts at the ring tile out beyond it.
        let walk = centre.circling(centre + DIRECTIONS[0], centre + DIRECTIONS[3] * 2);
        assert_eq!(walk.first().unwrap().flat_distance(&(centre + DIRECTIONS[0] * 2)), 1);
        // A quarter of the way round goes the short way.
        let walk = centre.circling(centre + DIRECTIONS[0] * 2, centre + DIRECTIONS[1] * 2);
        assert_eq!(walk, vec![centre + DIRECTIONS[0] * 2 + DIRECTIONS[2], centre + DIRECTIONS[1] * 2]);
        assert!(centre.circling(centre + DIRECTIONS[0] * 2, centre + DIRECTIONS[0] * 2).is_empty());
    }

    /// Scaling a tile scales its elevation with it.
    #[test]
    fn test_scalar_multiplication() {
        assert_eq!(Qrz { q: 2, r: 3, z: 1 } * 3, Qrz { q: 6, r: 9, z: 3 });
    }

    /// Every direction is flat and one step off.
    #[test]
    fn test_adjacent_hex_distance() {
        let origin = Qrz { q: 0, r: 0, z: 0 };
        for dir in &DIRECTIONS {
            assert_eq!(dir.z, 0, "directions are flat: {dir:?}");
            assert_eq!(origin.flat_distance(&(origin + *dir)), 1, "a direction is one step: {dir:?}");
        }
    }

    #[test]
    fn test_distance_includes_z() {
        let a = Qrz { q: 0, r: 0, z: 0 };
        let b = Qrz { q: 0, r: 0, z: 5 };
        assert_eq!(a.distance(&b), 5, "distance includes z");
        assert_eq!(a.flat_distance(&b), 0, "flat distance ignores z");
    }

    /// A metric: zero to itself, the same either way, and never longer
    /// than a detour.
    #[test]
    fn test_triangle_inequality() {
        let a = Qrz { q: 0, r: 0, z: 0 };
        let b = Qrz { q: 2, r: 1, z: 1 };
        let c = Qrz { q: 5, r: -2, z: 3 };
        assert_eq!((c.distance(&c), c.flat_distance(&c)), (0, 0));
        assert_eq!(a.distance(&c), c.distance(&a));
        assert_eq!(a.flat_distance(&c), c.flat_distance(&a));
        assert!(a.distance(&c) <= a.distance(&b) + b.distance(&c));
    }

    #[test]
    fn test_round_integers_unchanged() {
        assert_eq!(round(3.0, -1.0, 2.0), Qrz { q: 3, r: -1, z: 2 });
    }
}
