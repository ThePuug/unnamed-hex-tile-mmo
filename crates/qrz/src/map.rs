//! The grid's geometry: the affine map between a tile's `Qrz` and the
//! world, and what it gives — a tile's vertices, the faces between
//! neighbours, where a ray leaves a tile. The grid is flat-top: a flat
//! edge faces north, vertices point east and west. Tiles are kept
//! elsewhere (`common_bevy::resources::map::Map`); this `Map` holds only
//! the grid's `radius` and `rise`.
//!
//! ```rust
//! use qrz::{Map, Qrz, Convert};
//! use glam::Vec3;
//!
//! let map = Map::new(1.0, 0.8);
//! let coord = Qrz { q: 1, r: 2, z: 3 };
//! let world_pos: Vec3 = map.convert(coord);
//! let recovered: Qrz = map.convert(world_pos);
//! assert_eq!(coord, recovered);
//! ```

use std::f64::consts::SQRT_3;

use glam::{Vec2, Vec3, Vec3Swizzles};

use crate::qrz::{ self, Qrz };

/// Qrz → (x, z) at unit radius: `x = 3/2·q`, `z = √3/2·q + √3·r`.
const FORWARD: [f64; 4] = [3./2., 0., SQRT_3/2., SQRT_3];
/// (x, z) → (q, r) at unit radius, the inverse of `FORWARD`.
const INVERSE: [f64; 4] = [2./3., 0., -1./3., SQRT_3/3.];

/// Trait for bidirectional coordinate conversion
pub trait Convert<T,U> {
    /// Convert from type T to type U
    fn convert(&self, it: T) -> U;
}

/// The grid's dimensions: `radius`, a tile's centre to its vertex in
/// world units, and `rise`, the world height of one z-level.
#[derive(Clone, Debug, Default)]
pub struct Map {
    radius: f32,
    rise: f32,
}

impl Map {
    pub fn new(radius: f32, rise: f32) -> Self {
        Self { radius, rise }
    }

    pub fn radius(&self) -> f32 { self.radius }
    pub fn rise(&self) -> f32 { self.rise }

    /// The tile's seven vertices, `rise` above its centre: six corners
    /// clockwise from the north-east, `[NE, E, SE, SW, W, NW]`, then the
    /// centre. Corners `i` and `i + 1` bound the edge facing
    /// `DIRECTIONS[(4 - i) mod 6]`.
    pub fn vertices(&self, qrz: Qrz) -> Vec<Vec3> {
        let center = self.convert(qrz);
        let w = (self.radius as f64 * SQRT_3 / 2.) as f32;
        let h = self.radius / 2.;
        vec![
            center + Vec3 { x: h,  y: self.rise, z: -w },            // NE
            center + Vec3 { x: self.radius, y: self.rise, z: 0. },   // E
            center + Vec3 { x: h,  y: self.rise, z: w },             // SE
            center + Vec3 { x: -h, y: self.rise, z: w },             // SW
            center + Vec3 { x: -self.radius, y: self.rise, z: 0. },  // W
            center + Vec3 { x: -h, y: self.rise, z: -w },            // NW
            center + Vec3 { x: 0., y: self.rise, z: 0. },            // Center
        ]
    }

    /// The face between `here` and its neighbour `next`, in the ground plane
    /// (x, z): the unit normal from `here` into `next`, and the midpoint. A
    /// face is the perpendicular bisector of the two centres.
    pub fn face(&self, here: Qrz, next: Qrz) -> (Vec2, Vec2) {
        let a: Vec3 = self.convert(here);
        let b: Vec3 = self.convert(next);
        let (a, b) = (a.xz(), b.xz());
        ((b - a).normalize(), (a + b) * 0.5)
    }

    /// Where a ray from `from`, a ground-plane point in `here`, along `dir`
    /// leaves the tile: the multiple of `dir` to the first face it meets,
    /// world units for a unit `dir`, and the neighbour across that face. A
    /// point a hair past a face, as rounding leaves it, exits through it at
    /// zero. A ray meeting no face, `dir` zero, returns infinity and `here`.
    pub fn exit(&self, from: Vec2, dir: Vec2, here: Qrz) -> (f32, Qrz) {
        let mut nearest = (f32::INFINITY, here);
        for offset in qrz::DIRECTIONS {
            let next = here + offset;
            let (normal, mid) = self.face(here, next);
            let toward = dir.dot(normal);
            if toward <= 0. {
                continue;
            }
            let distance = (mid - from).dot(normal).max(0.) / toward;
            if distance < nearest.0 {
                nearest = (distance, next);
            }
        }
        nearest
    }
}

impl Convert<Vec3,Qrz> for Map {
    fn convert(&self, other: Vec3) -> Qrz {
        let q = (INVERSE[0] * other.x as f64 + INVERSE[1] * other.z as f64) / self.radius as f64;
        let r = (INVERSE[2] * other.x as f64 + INVERSE[3] * other.z as f64) / self.radius as f64;
        let z = other.y as f64 / self.rise as f64;
        qrz::round(q, r, z)
    }
}

impl Convert<Qrz,Vec3> for Map {
    fn convert(&self, other: Qrz) -> Vec3 {
        let x = (FORWARD[0] * other.q as f64 + FORWARD[1] * other.r as f64) * self.radius as f64;
        let z = (FORWARD[2] * other.q as f64 + FORWARD[3] * other.r as f64) * self.radius as f64;
        let y = other.z as f64 * self.rise as f64;
        Vec3 { x: x as f32, y: y as f32, z: z as f32 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conversion_roundtrip() {
        let coords = [
            Qrz { q: 0, r: 0, z: 0 },
            Qrz { q: 1, r: 0, z: 0 },
            Qrz { q: 0, r: 1, z: 0 },
            Qrz { q: -1, r: 1, z: 0 },
            Qrz { q: 5, r: -3, z: 2 },
            Qrz { q: -10, r: 7, z: -5 },
        ];
        let map = Map::new(1.0, 0.8);
        for &original in &coords {
            let world_pos: Vec3 = map.convert(original);
            let recovered: Qrz = map.convert(world_pos);
            assert_eq!(original, recovered, "roundtrip failed for {original:?}");
        }
    }

    #[test]
    fn test_z_coordinate_affects_y() {
        let map = Map::new(1.0, 0.8);
        let pos_flat: Vec3 = map.convert(Qrz { q: 0, r: 0, z: 0 });
        let pos_elevated: Vec3 = map.convert(Qrz { q: 0, r: 0, z: 5 });
        assert!(pos_elevated.y > pos_flat.y, "higher z should give higher y");
        assert!((pos_elevated.y - pos_flat.y - 5.0 * 0.8).abs() < 0.001, "y difference should equal z * rise");
    }

    #[test]
    fn test_vertices_form_hexagon() {
        let map = Map::new(1.0, 0.8);
        let verts = map.vertices(Qrz { q: 0, r: 0, z: 0 });
        let center = verts[6];
        for i in 0..6 {
            let dist = (verts[i] - center).length();
            assert!((dist - map.radius()).abs() < 0.01, "vertex {i} should be at radius distance from center, got {dist}");
        }
    }

    #[test]
    fn test_flat_top_q1r0_goes_right() {
        let map = Map::new(1.0, 0.8);
        let pos: Vec3 = map.convert(Qrz { q: 1, r: 0, z: 0 });
        assert!(pos.x > 0.0, "q=1 should have positive x");
    }

    /// Corners `i` and `i + 1` bound the edge facing `DIRECTIONS[(4 - i)
    /// mod 6]`: the edge's midpoint lies on the line from the centre to
    /// that neighbour's. `common_bevy::surface::CORNER_NEIGHBOURS` is
    /// written to this order.
    #[test]
    fn an_edge_faces_its_direction() {
        let map = Map::new(1.0, 0.8);
        let here = Qrz { q: 2, r: -3, z: 1 };
        let centre: Vec3 = map.convert(here);
        let corners = map.vertices(here);
        for i in 0..6 {
            let out = ((corners[i] + corners[(i + 1) % 6]) * 0.5 - centre).xz();
            let next: Vec3 = map.convert(here + qrz::DIRECTIONS[(4 + 6 - i) % 6]);
            let toward = (next - centre).xz();
            assert!(out.normalize().dot(toward.normalize()) > 1. - 1e-5, "edge {i}");
        }
    }

    /// A face's normal points from the tile's centre to its neighbour's, and
    /// the face lies halfway between them.
    #[test]
    fn a_face_bisects_the_centres() {
        let map = Map::new(2.0, 0.8);
        let here = Qrz { q: 3, r: -2, z: 0 };
        for offset in qrz::DIRECTIONS {
            let next = here + offset;
            let (normal, mid) = map.face(here, next);
            let a: Vec3 = map.convert(here);
            let b: Vec3 = map.convert(next);
            assert!((normal.length() - 1.).abs() < 1e-6);
            assert!(normal.dot((b.xz() - a.xz()).normalize()) > 1. - 1e-6);
            assert!((mid.distance(a.xz()) - mid.distance(b.xz())).abs() < 1e-6);
        }
    }

    /// From the centre, a ray straight at a neighbour leaves through that
    /// face at the inradius; a ray at a corner leaves at the circumradius,
    /// farther; a ray from a hair past a face leaves through it at once.
    #[test]
    fn a_ray_leaves_through_the_face_it_meets_first() {
        let map = Map::new(1.0, 0.8);
        let here = Qrz { q: 0, r: 0, z: 0 };
        let centre: Vec3 = map.convert(here);
        let inradius = (SQRT_3 / 2.) as f32;

        for offset in qrz::DIRECTIONS {
            let (normal, _) = map.face(here, here + offset);
            let (distance, next) = map.exit(centre.xz(), normal, here);
            assert_eq!(next, here + offset);
            assert!((distance - inradius).abs() < 1e-5, "{distance}");
        }

        let corner = map.vertices(here)[0].xz() - centre.xz();
        let (distance, _) = map.exit(centre.xz(), corner.normalize(), here);
        assert!((distance - 1.).abs() < 1e-5, "{distance}");
        assert!(distance > inradius);

        let east = here + qrz::DIRECTIONS[3];
        let (normal, mid) = map.face(here, east);
        let (distance, next) = map.exit(mid + normal * 1e-4, normal, here);
        assert_eq!((distance, next), (0., east));

        assert_eq!(map.exit(centre.xz(), Vec2::ZERO, here), (f32::INFINITY, here));
    }
}
