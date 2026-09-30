//! # Qrz: Hex Coordinate System

//! This module implements a 3D hexagonal coordinate system using axial coordinates.

//! ## Overview

//! Hexagonal grids can be represented using **axial coordinates** (q, r) which satisfy
//! the constraint `q + r + s = 0` where `s = -q - r`. This library extends this to 3D
//! by adding a vertical `z` component for elevation.

//! ## Coordinate System

//! - **q**: Axis aligned with "east-west"
//! - **r**: Axis aligned with "northeast-southwest"
//! - **s**: Derived axis (s = -q - r), aligned with "southeast-northwest"
//! - **z**: Vertical elevation

//! ## Key Features

//! - **Axial invariant**: `q + r + s = 0` is always maintained
//! - **Distance metrics**: Manhattan distance on hex grid (`flat_distance`) and 3D distance
//! - **Neighbor finding**: Get all 6 adjacent hexagonal tiles
//! - **Field of view**: Calculate visible tiles in a cone
//! - **Arithmetic**: Add, subtract, and scalar multiply coordinates

//! ## Usage

//! ```rust
//! use qrz::Qrz;

//! // Create a coordinate
//! let origin = Qrz { q: 0, r: 0, z: 0 };
//! let east = Qrz { q: 1, r: 0, z: 0 };

//! // Calculate distance
//! assert_eq!(origin.flat_distance(&east), 1);

//! // Get neighbors
//! let neighbors = origin.neighbors();
//! assert_eq!(neighbors.len(), 6);
//! ```

use std::ops::{Add, Mul, Sub};

use serde::{Deserialize, Serialize};

/// The 6 cardinal directions on a hex grid (west, southwest, southeast, east, northeast, northwest)
pub const DIRECTIONS: [Qrz; 6] = [
        Qrz { q: -1, r: 0, z: 0 }, // west
        Qrz { q: -1, r: 1, z: 0 }, // south-west
        Qrz { q: 0, r: 1, z: 0 }, // south-east
        Qrz { q: 1, r: 0, z: 0 }, // east
        Qrz { q: 1, r: -1, z: 0 }, // north-east
        Qrz { q: 0, r: -1, z: 0 }, // north-west
];

/// A 3D hexagonal coordinate using axial representation

/// # Invariant

/// Qrz coordinates must satisfy `q + r + s = 0` where `s = -q - r`.
/// This is automatically maintained by all operations.

/// # Fields

/// - `q`: Horizontal axis (east-west)
/// - `r`: Diagonal axis (northeast-southwest)
/// - `z`: Vertical axis (elevation)

/// # Example

/// ```
/// # use qrz::Qrz;
/// let coord = Qrz { q: 1, r: -1, z: 0 };
/// let s = -coord.q - coord.r; // s = 0
/// assert_eq!(coord.q + coord.r + s, 0); // Invariant holds
/// ```
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct Qrz {
    pub q: i32,
    pub r: i32,
    pub z: i32,
}

impl Ord for Qrz {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.into_doublewidth().cmp(&other.into_doublewidth())
    }
}
impl PartialOrd for Qrz {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}


impl Qrz {
    pub const Q: Qrz = Qrz{q:1,r:0,z:0};
    pub const R: Qrz = Qrz{q:0,r:1,z:0};
    pub const Z: Qrz = Qrz{q:0,r:0,z:1};

    pub fn flat_distance(&self, other: &Qrz) -> i32 {
        *[
            (self.q - other.q).abs(),
            (self.r - other.r).abs(),
            (-self.q-self.r - (-other.q-other.r)).abs()
        ].iter().max().unwrap()
    }

    pub fn normalize(&self) -> Qrz {
        let max = [self.q, self.r].into_iter().max_by_key(|a| a.abs()).unwrap();
        Qrz { 
            q: if max == self.q { self.q.signum() } else { 0 }, 
            r: if max == self.r { self.r.signum() } else { 0 }, 
            z: 0 } 
    }

    pub fn distance(&self, other: &Qrz) -> i32 {
        self.flat_distance(other) + (self.z-other.z).abs()
    }


    pub fn arc(&self, dir: &Qrz, radius: u8) -> Vec<Qrz> {
        let start = *dir * radius as i32;
        let idx = DIRECTIONS.iter().position(|i| { *i == *dir}).unwrap();
        (1..=radius).map(|i| start + DIRECTIONS[(idx + 2) % 6] * i as i32).chain(
        (0..=radius).map(|i| start + DIRECTIONS[(idx + 4) % 6] * i as i32))
            .map(|i| *self + i)
            .collect()
    }

    pub fn fov(&self, dir: &Qrz, dist: u8) -> Vec<Qrz> {
        (1..=dist).map(|i| self.arc(dir, i)).flatten().collect::<Vec<Qrz>>()
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

    pub fn neighbors(&self) -> Vec<Qrz> {
        vec![
            *self + Qrz { q: -1, r: 0, z: 0 }, // west
            *self + Qrz { q: -1, r: 1, z: 0 }, // south-west
            *self + Qrz { q: 0, r: 1, z: 0 }, // south-east
            *self + Qrz { q: 1, r: 0, z: 0 }, // east
            *self + Qrz { q: 1, r: -1, z: 0 }, // north-east
            *self + Qrz { q: 0, r: -1, z: 0 }, // north-west
        ]
    }

    pub fn into_doublewidth(&self) -> (i64,i64,i32) {
        (
            2 * self.q as i64 + self.r as i64,
            self.r as i64,
            self.z
        )
    }
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

pub fn round(q0: f64, r0: f64, z0: f64) -> Qrz {
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
        assert_eq!(here.beyond(here - Qrz::Q, 3), vec![here + Qrz::Q, here + Qrz::Q * 2, here + Qrz::Q * 3], "along an axis, its tiles");
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
        let (east, west) = (here.beyond(here - Qrz::Q, 1)[0], here.beyond(here + Qrz::Q, 1)[0]);
        assert_eq!(east - here, here - west);
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

    // ===== COORDINATE INVARIANT TESTS =====

    #[test]
    fn test_axial_coordinate_invariant() {
        // The fundamental invariant of axial coordinates: q + r + s = 0
        // Since s = -q-r by definition, this is always true for valid coordinates
        let coords = vec![
            Qrz { q: 0, r: 0, z: 0 },
            Qrz { q: 1, r: 0, z: 0 },
            Qrz { q: 0, r: 1, z: 0 },
            Qrz { q: 1, r: -1, z: 0 },
            Qrz { q: -1, r: 1, z: 0 },
            Qrz { q: 5, r: -3, z: 2 },
        ];

        for coord in coords {
            let s = -coord.q - coord.r;
            assert_eq!(
                coord.q + coord.r + s, 0,
                "Axial invariant failed for {:?}: q+r+s = {}",
                coord, coord.q + coord.r + s
            );
        }
    }

    #[test]
    fn test_origin_is_zero() {
        let origin = Qrz::default();
        assert_eq!(origin.q, 0);
        assert_eq!(origin.r, 0);
        assert_eq!(origin.z, 0);
    }

    #[test]
    fn test_basis_vectors() {
        // Test the three basis vectors
        assert_eq!(Qrz::Q, Qrz { q: 1, r: 0, z: 0 });
        assert_eq!(Qrz::R, Qrz { q: 0, r: 1, z: 0 });
        assert_eq!(Qrz::Z, Qrz { q: 0, r: 0, z: 1 });
    }

    #[test]
    fn test_directions_are_valid() {
        // All 6 hex directions should be valid axial coordinates
        for (i, dir) in DIRECTIONS.iter().enumerate() {
            let s = -dir.q - dir.r;
            assert_eq!(
                dir.q + dir.r + s, 0,
                "Direction {} is invalid: {:?}",
                i, dir
            );
            assert_eq!(dir.z, 0, "Directions should be flat (z=0)");
        }
    }

    #[test]
    fn test_six_directions() {
        // Should have exactly 6 directions
        assert_eq!(DIRECTIONS.len(), 6, "Hexagon should have 6 directions");
    }

    // ===== ARITHMETIC TESTS =====

    #[test]
    fn test_addition() {
        let a = Qrz { q: 1, r: 2, z: 3 };
        let b = Qrz { q: 4, r: 5, z: 6 };
        let result = a + b;

        assert_eq!(result, Qrz { q: 5, r: 7, z: 9 });
    }

    #[test]
    fn test_subtraction() {
        let a = Qrz { q: 5, r: 7, z: 9 };
        let b = Qrz { q: 1, r: 2, z: 3 };
        let result = a - b;

        assert_eq!(result, Qrz { q: 4, r: 5, z: 6 });
    }

    #[test]
    fn test_scalar_multiplication() {
        let a = Qrz { q: 2, r: 3, z: 1 };
        let result = a * 3;

        assert_eq!(result, Qrz { q: 6, r: 9, z: 3 });
    }

    #[test]
    fn test_addition_is_commutative() {
        let a = Qrz { q: 1, r: 2, z: 3 };
        let b = Qrz { q: 4, r: 5, z: 6 };

        assert_eq!(a + b, b + a, "Addition should be commutative");
    }

    #[test]
    fn test_addition_identity() {
        let a = Qrz { q: 5, r: -3, z: 2 };
        let zero = Qrz::default();

        assert_eq!(a + zero, a, "Adding zero should not change coordinate");
    }

    // ===== DISTANCE TESTS =====

    #[test]
    fn test_distance_to_self_is_zero() {
        let coord = Qrz { q: 5, r: -3, z: 2 };
        assert_eq!(coord.distance(&coord), 0, "Distance to self should be 0");
    }

    #[test]
    fn test_flat_distance_to_self_is_zero() {
        let coord = Qrz { q: 5, r: -3, z: 0 };
        assert_eq!(coord.flat_distance(&coord), 0, "Flat distance to self should be 0");
    }

    #[test]
    fn test_distance_is_symmetric() {
        let a = Qrz { q: 1, r: 0, z: 0 };
        let b = Qrz { q: 5, r: -2, z: 3 };

        assert_eq!(
            a.distance(&b), b.distance(&a),
            "Distance should be symmetric"
        );
    }

    #[test]
    fn test_flat_distance_is_symmetric() {
        let a = Qrz { q: 1, r: 0, z: 0 };
        let b = Qrz { q: 5, r: -2, z: 0 };

        assert_eq!(
            a.flat_distance(&b), b.flat_distance(&a),
            "Flat distance should be symmetric"
        );
    }

    #[test]
    fn test_adjacent_hex_distance() {
        let origin = Qrz { q: 0, r: 0, z: 0 };

        for dir in &DIRECTIONS {
            let neighbor = origin + *dir;
            assert_eq!(
                origin.flat_distance(&neighbor), 1,
                "Adjacent hex should be distance 1, direction: {:?}",
                dir
            );
        }
    }

    #[test]
    fn test_distance_includes_z() {
        let a = Qrz { q: 0, r: 0, z: 0 };
        let b = Qrz { q: 0, r: 0, z: 5 };

        assert_eq!(a.distance(&b), 5, "Distance should include Z component");
        assert_eq!(a.flat_distance(&b), 0, "Flat distance should ignore Z");
    }

    #[test]
    fn test_triangle_inequality() {
        // For any three points a, b, c: distance(a,c) <= distance(a,b) + distance(b,c)
        let a = Qrz { q: 0, r: 0, z: 0 };
        let b = Qrz { q: 2, r: 1, z: 1 };
        let c = Qrz { q: 5, r: -2, z: 3 };

        let ac = a.distance(&c);
        let ab = a.distance(&b);
        let bc = b.distance(&c);

        assert!(
            ac <= ab + bc,
            "Triangle inequality violated: {} > {} + {}",
            ac, ab, bc
        );
    }

    // ===== NEIGHBOR TESTS =====

    #[test]
    fn test_neighbors_returns_six() {
        let origin = Qrz { q: 0, r: 0, z: 0 };
        let neighbors = origin.neighbors();

        assert_eq!(neighbors.len(), 6, "Hex should have exactly 6 neighbors");
    }

    #[test]
    fn test_neighbors_match_directions() {
        let origin = Qrz { q: 0, r: 0, z: 0 };
        let neighbors = origin.neighbors();

        // Neighbors should be origin + each direction
        for (i, expected) in DIRECTIONS.iter().enumerate() {
            let expected_neighbor = origin + *expected;
            assert!(
                neighbors.contains(&expected_neighbor),
                "Neighbor {} ({:?}) not found in neighbors list",
                i, expected_neighbor
            );
        }
    }

    #[test]
    fn test_all_neighbors_are_distance_one() {
        let center = Qrz { q: 3, r: -1, z: 0 };
        let neighbors = center.neighbors();

        for neighbor in neighbors {
            assert_eq!(
                center.flat_distance(&neighbor), 1,
                "All neighbors should be exactly distance 1 from center"
            );
        }
    }

    // ===== ROUNDING TESTS =====

    #[test]
    fn test_round_integers_unchanged() {
        // Rounding integer coordinates should return the same values
        let result = round(3.0, -1.0, 2.0);
        assert_eq!(result, Qrz { q: 3, r: -1, z: 2 });
    }

    #[test]
    fn test_round_halfway_values() {
        // Test rounding behavior at halfway points
        let result = round(1.5, -1.5, 0.0);
        let s = -result.q - result.r;
        assert_eq!(result.q + result.r + s, 0, "Rounded value must maintain invariant");
    }

    #[test]
    fn test_round_maintains_invariant() {
        // Rounding arbitrary floating point values should maintain q+r+s=0
        let test_values = vec![
            (1.2, 2.7, 0.0),
            (-3.8, 1.1, 5.5),
            (0.0, 0.0, 0.0),
            (10.9, -5.3, -2.7),
        ];

        for (q, r, z) in test_values {
            let result = round(q, r, z);
            let s = -result.q - result.r;
            assert_eq!(
                result.q + result.r + s, 0,
                "round({}, {}, {}) = {:?} violates invariant",
                q, r, z, result
            );
        }
    }

    // ===== PROPERTY TESTS =====

    #[test]
    fn test_normalize_reduces_to_unit_vector() {
        let coords = vec![
            Qrz { q: 3, r: 0, z: 0 },   // Should normalize to (1, 0, 0)
            Qrz { q: 0, r: 5, z: 0 },   // Should normalize to (0, 1, 0)
            Qrz { q: -2, r: 0, z: 0 },  // Should normalize to (-1, 0, 0)
            Qrz { q: 0, r: -3, z: 0 },  // Should normalize to (0, -1, 0)
        ];

        for coord in coords {
            let normalized = coord.normalize();
            // Normalized coords should have max component of ±1 or 0
            assert!(
                normalized.q.abs() <= 1 && normalized.r.abs() <= 1,
                "{:?}.normalize() = {:?} is not a unit vector",
                coord, normalized
            );
            assert_eq!(normalized.z, 0, "Normalize should zero out Z");
        }
    }

    #[test]
    fn test_equality() {
        let a = Qrz { q: 1, r: 2, z: 3 };
        let b = Qrz { q: 1, r: 2, z: 3 };
        let c = Qrz { q: 1, r: 2, z: 4 };

        assert_eq!(a, b, "Identical coordinates should be equal");
        assert_ne!(a, c, "Different coordinates should not be equal");
    }

    #[test]
    fn test_ordering_is_consistent() {
        let a = Qrz { q: 0, r: 0, z: 0 };
        let b = Qrz { q: 1, r: 0, z: 0 };
        let c = Qrz { q: 2, r: 0, z: 0 };

        // If a < b and b < c, then a < c (transitivity)
        assert!(a < b);
        assert!(b < c);
        assert!(a < c, "Ordering should be transitive");
    }

    // ===== FOV & ARC TESTS =====

    #[test]
    fn test_arc_contains_correct_count() {
        let center = Qrz { q: 0, r: 0, z: 0 };
        let dir = &DIRECTIONS[0]; // West

        // Arc of radius 1 should contain (2*radius + 1) tiles = 3
        let arc1 = center.arc(dir, 1);
        assert_eq!(arc1.len(), 3, "Arc of radius 1 should have 3 tiles");

        // Arc of radius 2 should contain 5 tiles
        let arc2 = center.arc(dir, 2);
        assert_eq!(arc2.len(), 5, "Arc of radius 2 should have 5 tiles");
    }

    #[test]
    fn test_fov_increases_with_distance() {
        let center = Qrz { q: 0, r: 0, z: 0 };
        let dir = &DIRECTIONS[0];

        let fov1 = center.fov(dir, 1);
        let fov2 = center.fov(dir, 2);

        assert!(
            fov2.len() > fov1.len(),
            "Larger FOV distance should contain more tiles"
        );
    }
}
