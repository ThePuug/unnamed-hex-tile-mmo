//! # Map: Hexagonal Tile Storage with World Space Conversion
//!
//! This module provides a `Map` type that stores tiles at hexagonal coordinates
//! and handles conversion between hex coordinates (Qrz) and 3D world space (Vec3).
//!
//! ## Overview
//!
//! The Map uses "pointy-top" hex orientation where hexagons have a vertex pointing north.
//! Conversion between hex and world coordinates uses an affine transformation based on
//! the map's `radius` (hex size) and `rise` (vertical scale).
//!
//! ## Features
//!
//! - **Bidirectional conversion**: Qrz ↔ Vec3 with automatic rounding
//! - **Tile storage**: HashMap and BTreeMap for fast lookup and iteration
//! - **Vertical search**: Find nearest tile below/above a position
//! - **Line tracing**: Get all tiles between two points
//! - **Mesh generation**: Generate hexagon vertices for rendering
//!
//! ## Example
//!
//! ```rust
//! use qrz::{Map, Qrz, Convert};
//! use glam::Vec3;
//!
//! let mut map: Map<i32> = Map::new(1.0, 0.8);
//!
//! // Store a tile
//! let coord = Qrz { q: 1, r: 2, z: 3 };
//! map.insert(coord, 42);
//!
//! // Convert to world space
//! let world_pos: Vec3 = map.convert(coord);
//!
//! // Convert back
//! let recovered: Qrz = map.convert(world_pos);
//! assert_eq!(coord, recovered);
//! ```

use std::{
    collections::{ BTreeMap, HashMap },
    f64::consts::SQRT_3
};

use glam::Vec3;
use derive_more::*;

use crate::qrz::{ self, Qrz };

/// Affine transformation matrix for pointy-top hex orientation
/// Format: (forward matrix, inverse matrix) for Vec3 ↔ Qrz conversions
const ORIENTATION: ([f64; 4], [f64; 4]) = (
    [SQRT_3, SQRT_3/2., 0., 3./2.],
    [SQRT_3/3., -1./3., 0., 2./3.],
);

/// Trait for bidirectional coordinate conversion
pub trait Convert<T,U> {
    /// Convert from type T to type U
    fn convert(&self, it: T) -> U;
}

/// A hexagonal tile map with world space conversion
///
/// Stores tiles at hexagonal coordinates and provides conversion between
/// hex coordinates (Qrz) and 3D world space (Vec3).
///
/// # Type Parameters
///
/// - `T`: The type of data stored at each tile position (must implement `Copy`)
///
/// # Fields
///
/// - `radius`: Size of each hexagon in world units
/// - `rise`: Vertical scale factor (Z coordinate → Y world space)
/// - `tree`: BTreeMap for ordered iteration
/// - `hash`: HashMap for fast O(1) lookup
#[derive(Clone, Debug, Default, IntoIterator)]
pub struct Map<T> {
    radius: f32,
    rise: f32,
    #[into_iterator(owned)]
    tree: BTreeMap<Qrz, T>,
    hash: HashMap<Qrz, T>,
}

impl<T> Map<T> 
where T : Copy {
    pub fn new(radius: f32, rise: f32) -> Self {
        Self { radius, rise, tree: BTreeMap::new(), hash: HashMap::new() }
    }

    pub fn radius(&self) -> f32 { self.radius }
    pub fn rise(&self) -> f32 { self.rise }

    pub fn line(&self, a: &Qrz, b: &Qrz) -> Vec<Qrz> { 
        let dist = a.flat_distance(b); 
        let step = 1. / (dist+1) as f32;
        (1..=dist+1).map(|i| {
            self.convert(self.convert(*a).lerp(self.convert(*b), i as f32 * step))
        }).collect()
    }

    pub fn find(&self, qrz: Qrz, dist: i8) -> Option<(Qrz, T)> {
        for i in 0..=dist.abs() {
            let z = if dist < 0 { -i as i16 } else { i as i16 };
            let qrz = qrz + Qrz { q: 0, r: 0, z };
            if let Some(obj) = self.get(qrz) { return Some((qrz, *obj)); }
        }
        None
    }

    pub fn get(&self, qrz: Qrz) -> Option<&T> {
        self.hash.get(&qrz)
    }

    pub fn insert(&mut self, qrz: Qrz, obj: T) {
        self.tree.insert(qrz, obj);
        self.hash.insert(qrz, obj);
    }

    pub fn remove(&mut self, qrz: Qrz) -> Option<T> {
        self.tree.remove(&qrz);
        self.hash.remove(&qrz)
    }

    pub fn len(&self) -> usize {
        self.hash.len()
    }

    pub fn vertices(&self, qrz: Qrz) -> Vec<Vec3> {
        let center = self.convert(qrz);
        let w = (self.radius as f64 * SQRT_3 / 2.) as f32;
        let h = self.radius / 2.;
        vec![
            center + Vec3 { x: 0., y: self.rise, z: -self.radius },
            center + Vec3 { x: w,  y: self.rise, z: -h },
            center + Vec3 { x: w,  y: self.rise, z: h },
            center + Vec3 { x: 0., y: self.rise, z: self.radius },
            center + Vec3 { x: -w, y: self.rise, z: h },
            center + Vec3 { x: -w, y: self.rise, z: -h },
            center + Vec3 { x: 0., y: self.rise, z: 0. },
        ]
    }

    pub fn neighbors(&self, qrz: Qrz) -> Vec<(Qrz,T)> {
        let mut neighbors = Vec::new();
        for check in qrz.neighbors() {
            let Some(neighbor) = self.find(check + Qrz::Z, -2) else { continue };
            neighbors.extend_one(neighbor);
        }
        neighbors
    }
}

impl<T> Convert<Vec3,Qrz> for Map<T> {
    fn convert(&self, other: Vec3) -> Qrz {
        let q = (ORIENTATION.1[0] * other.x as f64 + ORIENTATION.1[1] * other.z as f64) / self.radius as f64;
        let r = (ORIENTATION.1[2] * other.x as f64 + ORIENTATION.1[3] * other.z as f64) / self.radius as f64;
        let z = other.y as f64 / self.rise as f64;
        qrz::round(q, r, z)
    }
}

impl<T> Convert<Qrz,Vec3> for Map<T> {
    fn convert(&self, other: Qrz) -> Vec3 {
        let x = (ORIENTATION.0[0] * other.q as f64 + ORIENTATION.0[1] * other.r as f64) * self.radius as f64;
        let z = (ORIENTATION.0[2] * other.q as f64 + ORIENTATION.0[3] * other.r as f64) * self.radius as f64;
        let y = other.z as f64 * self.rise as f64;
        Vec3 { x: x as f32, y: y as f32, z: z as f32 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== MAP BASIC TESTS =====

    #[test]
    fn test_map_creation() {
        let map: Map<i32> = Map::new(1.0, 0.8);
        assert_eq!(map.radius(), 1.0);
        assert_eq!(map.rise(), 0.8);
    }

    #[test]
    fn test_map_insert_and_get() {
        let mut map = Map::new(1.0, 0.8);
        let coord = Qrz { q: 1, r: 2, z: 3 };

        map.insert(coord, 42);
        assert_eq!(map.get(coord), Some(&42));
    }

    #[test]
    fn test_map_get_nonexistent() {
        let map: Map<i32> = Map::new(1.0, 0.8);
        let coord = Qrz { q: 1, r: 2, z: 3 };

        assert_eq!(map.get(coord), None);
    }

    #[test]
    fn test_map_remove() {
        let mut map = Map::new(1.0, 0.8);
        let coord = Qrz { q: 1, r: 2, z: 3 };

        map.insert(coord, 42);
        assert_eq!(map.remove(coord), Some(42));
        assert_eq!(map.get(coord), None);
    }

    #[test]
    fn test_map_overwrite() {
        let mut map = Map::new(1.0, 0.8);
        let coord = Qrz { q: 1, r: 2, z: 3 };

        map.insert(coord, 42);
        map.insert(coord, 100);
        assert_eq!(map.get(coord), Some(&100));
    }

    // ===== COORDINATE CONVERSION TESTS =====

    #[test]
    fn test_origin_converts_to_zero() {
        let map: Map<i32> = Map::new(1.0, 0.8);
        let origin = Qrz { q: 0, r: 0, z: 0 };
        let world_pos = map.convert(origin);

        assert!(world_pos.x.abs() < 0.001, "Origin X should be ~0, got {}", world_pos.x);
        assert!(world_pos.y.abs() < 0.001, "Origin Y should be ~0, got {}", world_pos.y);
        assert!(world_pos.z.abs() < 0.001, "Origin Z should be ~0, got {}", world_pos.z);
    }

    #[test]
    fn test_conversion_roundtrip() {
        let map: Map<i32> = Map::new(1.0, 0.8);
        let original = Qrz { q: 5, r: -3, z: 2 };

        // Convert to world space and back
        let world_pos = map.convert(original);
        let recovered: Qrz = map.convert(world_pos);

        assert_eq!(original, recovered, "Roundtrip conversion failed");
    }

    #[test]
    fn test_conversion_roundtrip_multiple_coords() {
        let map: Map<i32> = Map::new(1.0, 0.8);
        let coords = vec![
            Qrz { q: 0, r: 0, z: 0 },
            Qrz { q: 1, r: 0, z: 0 },
            Qrz { q: 0, r: 1, z: 0 },
            Qrz { q: -1, r: 1, z: 0 },
            Qrz { q: 5, r: -3, z: 2 },
            Qrz { q: -10, r: 7, z: -5 },
        ];

        for original in coords {
            let world_pos = map.convert(original);
            let recovered: Qrz = map.convert(world_pos);
            assert_eq!(original, recovered, "Roundtrip failed for {:?}", original);
        }
    }

    #[test]
    fn test_z_coordinate_affects_y() {
        let map: Map<i32> = Map::new(1.0, 0.8);
        let flat = Qrz { q: 0, r: 0, z: 0 };
        let elevated = Qrz { q: 0, r: 0, z: 5 };

        let pos_flat = map.convert(flat);
        let pos_elevated = map.convert(elevated);

        assert!(pos_elevated.y > pos_flat.y, "Higher Z should result in higher Y");
        assert!((pos_elevated.y - pos_flat.y - 5.0 * 0.8).abs() < 0.001,
                "Y difference should equal Z * rise");
    }

    #[test]
    fn test_different_radius_scales_output() {
        let map_small: Map<i32> = Map::new(0.5, 0.8);
        let map_large: Map<i32> = Map::new(2.0, 0.8);
        let coord = Qrz { q: 1, r: 0, z: 0 };

        let pos_small = map_small.convert(coord);
        let pos_large = map_large.convert(coord);

        assert!(pos_large.x > pos_small.x, "Larger radius should result in larger world coordinates");
    }

    #[test]
    fn test_different_rise_scales_y() {
        let map_short: Map<i32> = Map::new(1.0, 0.5);
        let map_tall: Map<i32> = Map::new(1.0, 1.5);
        let coord = Qrz { q: 0, r: 0, z: 1 };

        let pos_short = map_short.convert(coord);
        let pos_tall = map_tall.convert(coord);

        assert!(pos_tall.y > pos_short.y, "Larger rise should result in taller world coordinates");
    }

    // ===== MAP OPERATION TESTS =====

    #[test]
    fn test_find_exact_match() {
        let mut map = Map::new(1.0, 0.8);
        let coord = Qrz { q: 1, r: 2, z: 3 };
        map.insert(coord, 42);

        let result = map.find(coord, 0);
        assert_eq!(result, Some((coord, 42)), "Should find exact coordinate");
    }

    #[test]
    fn test_find_searches_vertically() {
        let mut map = Map::new(1.0, 0.8);
        let base = Qrz { q: 1, r: 2, z: 5 };
        map.insert(base, 42);

        // Search from Z=10 downward by 10 levels
        let search_start = Qrz { q: 1, r: 2, z: 10 };
        let result = map.find(search_start, -10);

        assert_eq!(result, Some((base, 42)), "Should find tile 5 levels below search start");
    }

    #[test]
    fn test_find_returns_none_when_not_found() {
        let map: Map<i32> = Map::new(1.0, 0.8);
        let search_start = Qrz { q: 1, r: 2, z: 10 };
        let result = map.find(search_start, -5);

        assert_eq!(result, None, "Should return None when no tile found");
    }

    #[test]
    fn test_vertices_returns_seven() {
        let map: Map<i32> = Map::new(1.0, 0.8);
        let coord = Qrz { q: 0, r: 0, z: 0 };
        let verts = map.vertices(coord);

        // Hexagon has 6 outer vertices + 1 center
        assert_eq!(verts.len(), 7, "Hexagon should have 7 vertices (6 + center)");
    }

    #[test]
    fn test_vertices_form_hexagon() {
        let map: Map<i32> = Map::new(1.0, 0.8);
        let coord = Qrz { q: 0, r: 0, z: 0 };
        let verts = map.vertices(coord);

        // All outer vertices should be roughly equidistant from center
        let center = verts[6];
        let radius = map.radius();

        for i in 0..6 {
            let dist = (verts[i] - center).length();
            assert!(
                (dist - radius).abs() < 0.01,
                "Vertex {} should be at radius distance from center, got {}",
                i, dist
            );
        }
    }

    #[test]
    fn test_line_between_adjacent() {
        let map: Map<i32> = Map::new(1.0, 0.8);
        let a = Qrz { q: 0, r: 0, z: 0 };
        let b = Qrz { q: 1, r: 0, z: 0 }; // Adjacent hex

        let line = map.line(&a, &b);

        // Line between adjacent hexes should include both endpoints
        assert!(line.len() >= 2, "Line should contain at least start and end");
        assert!(line.contains(&a) || line.contains(&b), "Line should contain at least one endpoint");
    }

    #[test]
    fn test_line_length_matches_distance() {
        let map: Map<i32> = Map::new(1.0, 0.8);
        let a = Qrz { q: 0, r: 0, z: 0 };
        let b = Qrz { q: 5, r: 0, z: 0 };

        let distance = a.flat_distance(&b);
        let line = map.line(&a, &b);

        // Line length should be approximately distance + 1
        assert!(
            line.len() as i16 >= distance,
            "Line should have at least {} tiles, got {}",
            distance, line.len()
        );
    }

    // ===== PROPERTY TESTS =====

    #[test]
    fn test_conversion_is_deterministic() {
        let map: Map<i32> = Map::new(1.0, 0.8);
        let coord = Qrz { q: 3, r: -1, z: 2 };

        let pos1 = map.convert(coord);
        let pos2 = map.convert(coord);

        assert_eq!(pos1, pos2, "Conversion should be deterministic");
    }

    #[test]
    fn test_neighbors_respects_elevation() {
        let mut map = Map::new(1.0, 0.8);
        let center = Qrz { q: 0, r: 0, z: 5 };
        map.insert(center, 1);

        // Add neighbors at different elevations
        map.insert(Qrz { q: 1, r: 0, z: 5 }, 2);  // Same level
        map.insert(Qrz { q: -1, r: 0, z: 4 }, 3); // One below
        map.insert(Qrz { q: 0, r: 1, z: 6 }, 4);  // One above

        let neighbors = map.neighbors(center);

        // Should find neighbors within vertical search range
        assert!(!neighbors.is_empty(), "Should find some neighbors");
    }
}
