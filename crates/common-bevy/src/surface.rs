//! The terrain surface: one height function shared by the mesh builder and
//! physics, so what an entity stands on is what is drawn.

//! A hex lattice at any scale carries one height per cell. Each cell renders
//! as a fan of six triangles from its centre to its six corners, and a corner
//! is one vertex shared by the three cells meeting there, at the mean of
//! their heights. Adjacent cells share corners and edges, so the surface is
//! watertight by construction; a large height difference between neighbours
//! is a steep triangle, not a gap.

use bevy::math::{Vec2, Vec3, Vec3Swizzles};

use common::camera::{HEX_RADIUS, RISE};

/// Lattice offsets of the two cells sharing each corner with the cell that
/// owns it, in flat-top corner order NE(0), E(1), SE(2), SW(3), W(4), NW(5).
/// Corner i lies between the edges facing these two neighbours.
pub const CORNER_NEIGHBOURS: [[(i32, i32); 2]; 6] = [
    [(1, -1), (0, -1)], // NE: between the NE and NW neighbours
    [(1, 0), (1, -1)],  // E:  E and NE
    [(0, 1), (1, 0)],   // SE: SE and E
    [(-1, 1), (0, 1)],  // SW: SW and SE
    [(-1, 0), (-1, 1)], // W:  W and SW
    [(0, -1), (-1, 0)], // NW: NW and W
];

/// Corner offsets from a cell centre for a flat-top hex of the given outer
/// radius, in corner order.
pub fn corner_offsets(outer_radius: f32) -> [Vec2; 6] {
    let w = (outer_radius as f64 * 3.0_f64.sqrt() / 2.0) as f32;
    let h = outer_radius / 2.0;
    [
        Vec2::new(h, -w),
        Vec2::new(outer_radius, 0.0),
        Vec2::new(h, w),
        Vec2::new(-h, w),
        Vec2::new(-outer_radius, 0.0),
        Vec2::new(-h, -w),
    ]
}

/// World Y of a surface at a lattice height: one step above the cell's z,
/// so a tile at z = 0 is walked on at `RISE`.
pub fn height_y(z: f32) -> f32 {
    (z + 1.0) * RISE
}

/// Height of a corner from the cells meeting there, in z units: the mean of
/// those present. A cell can be absent at the edge of the loaded tiles;
/// physics and the tile outlines then take the mean of the rest, while the
/// mesh builds only with every cell present. None when none is present.
pub fn corner_z(cells: [Option<i32>; 3]) -> Option<f32> {
    let (sum, n) = cells
        .iter()
        .flatten()
        .fold((0i64, 0u32), |(s, n), &z| (s + z as i64, n + 1));
    (n > 0).then(|| sum as f32 / n as f32)
}

/// The six corner heights of a cell, in z units, from a height lookup over
/// its lattice. A corner with no present cell (the owner is absent too) is
/// None.
pub fn cell_corner_zs(
    cell: (i32, i32),
    mut height: impl FnMut(i32, i32) -> Option<i32>,
) -> [Option<f32>; 6] {
    let own = height(cell.0, cell.1);
    std::array::from_fn(|i| {
        let [a, b] = CORNER_NEIGHBOURS[i];
        corner_z([own, height(cell.0 + a.0, cell.1 + a.1), height(cell.0 + b.0, cell.1 + b.1)])
    })
}

/// Height (world Y) of the fan surface under `p`, for a cell centred at
/// `centre` with the given outer radius, centre height and corner heights.
/// `p` is assumed inside the cell; a point exactly on an edge yields the same
/// height from either side because the edge is shared.
pub fn fan_height(p: Vec2, centre: Vec2, outer_radius: f32, centre_y: f32, corner_y: [f32; 6]) -> f32 {
    let d = p - centre;
    if d.length_squared() < 1e-12 {
        return centre_y;
    }
    // Corners sit at -60°, 0°, 60°, … from the +x axis (E is corner 1), so
    // the sector from corner i to corner i+1 starts at (i-1)·60°.
    let angle = d.y.atan2(d.x).to_degrees();
    let i = ((angle + 60.0) / 60.0).floor().rem_euclid(6.0) as usize;
    let j = (i + 1) % 6;
    let offsets = corner_offsets(outer_radius);
    let (a, b) = (offsets[i], offsets[j]);
    // d = u·a + v·b; height is the same combination of the corner rises.
    let det = a.x * b.y - a.y * b.x;
    let u = (d.x * b.y - d.y * b.x) / det;
    let v = (a.x * d.y - a.y * d.x) / det;
    centre_y + u * (corner_y[i] - centre_y) + v * (corner_y[j] - centre_y)
}

/// Height (world Y) of the tile surface at world `xz`, standing on `floor`:
/// the tile the point is over, with its z. Neighbour heights come from
/// `height(q, r)`; an absent neighbour leaves its corners to the cells that
/// are present, exactly as the mesh does.
pub fn surface_y(
    xz: Vec2,
    floor: qrz::Qrz,
    floor_centre: Vec3,
    height: impl FnMut(i32, i32) -> Option<i32>,
) -> f32 {
    let corner_zs = cell_corner_zs((floor.q, floor.r), height);
    let corner_y = corner_zs.map(|z| height_y(z.unwrap_or(floor.z as f32)));
    fan_height(xz, floor_centre.xz(), HEX_RADIUS, height_y(floor.z as f32), corner_y)
}

/// Normal of the surface at a corner: the plane through the three cell
/// centres around it (the corner is their centroid, and lies on that plane).
/// Falls back to up when a cell is absent.
pub fn corner_normal(centres: [Option<Vec3>; 3]) -> Vec3 {
    let [Some(a), Some(b), Some(c)] = centres else { return Vec3::Y };
    let n = (b - a).cross(c - a);
    if n.length_squared() < 1e-12 {
        return Vec3::Y;
    }
    let n = n.normalize();
    if n.y < 0.0 { -n } else { n }
}

/// Normal of the surface at a cell centre: the area-weighted mean of its six
/// fan triangles.
pub fn centre_normal(centre: Vec3, corners: [Vec3; 6]) -> Vec3 {
    let mut sum = Vec3::ZERO;
    for i in 0..6 {
        let j = (i + 1) % 6;
        sum += (corners[j] - centre).cross(corners[i] - centre);
    }
    if sum.length_squared() < 1e-12 {
        return Vec3::Y;
    }
    let n = sum.normalize();
    if n.y < 0.0 { -n } else { n }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_at(z: i32) -> impl FnMut(i32, i32) -> Option<i32> {
        move |_, _| Some(z)
    }

    #[test]
    fn corner_is_mean_of_present_cells() {
        assert_eq!(corner_z([Some(0), Some(0), Some(3)]), Some(1.0));
        assert_eq!(corner_z([Some(2), None, Some(4)]), Some(3.0));
        assert_eq!(corner_z([None, None, None]), None);
    }

    #[test]
    fn corner_neighbours_are_mutually_adjacent() {
        // The two neighbours sharing a corner must also neighbour each other,
        // or the corner is not a point three cells meet at.
        for [a, b] in CORNER_NEIGHBOURS {
            let d = (a.0 - b.0, a.1 - b.1);
            let dist = d.0.abs().max(d.1.abs()).max((d.0 + d.1).abs());
            assert_eq!(dist, 1, "{a:?} and {b:?} are not adjacent");
        }
    }

    #[test]
    fn corner_positions_match_between_sharing_cells() {
        // Each corner of cell (0,0) coincides with a corner of both cells
        // that share it, at the same world position.
        let centre = |q: i32, r: i32| {
            let (x, z) = crate::geometry::flat_top_tile_center(q, r, 1.0);
            Vec2::new(x, z)
        };
        let offsets = corner_offsets(1.0);
        for (i, [a, b]) in CORNER_NEIGHBOURS.iter().enumerate() {
            let p = centre(0, 0) + offsets[i];
            for n in [a, b] {
                let c = centre(n.0, n.1);
                let hit = offsets.iter().any(|o| (c + *o - p).length() < 1e-4);
                assert!(hit, "corner {i} of (0,0) is not a corner of {n:?}");
            }
        }
    }

    #[test]
    fn flat_ground_is_flat_everywhere() {
        let corners = [height_y(5.0); 6];
        for (x, z) in [(0.0, 0.0), (0.5, 0.3), (-0.9, 0.0), (0.2, -0.8)] {
            let y = fan_height(Vec2::new(x, z), Vec2::ZERO, 1.0, height_y(5.0), corners);
            assert!((y - height_y(5.0)).abs() < 1e-5);
        }
    }

    #[test]
    fn height_at_centre_is_the_cell_height() {
        let corners = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0];
        let y = fan_height(Vec2::ZERO, Vec2::ZERO, 1.0, 7.0, corners);
        assert_eq!(y, 7.0);
    }

    #[test]
    fn height_at_a_corner_is_that_corner() {
        let corners = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0];
        for (i, o) in corner_offsets(1.0).iter().enumerate() {
            let y = fan_height(*o * 0.9999, Vec2::ZERO, 1.0, 7.0, corners);
            assert!((y - corners[i]).abs() < 1e-2, "corner {i}: {y} vs {}", corners[i]);
        }
    }

    #[test]
    fn edge_midpoint_agrees_from_both_sides() {
        // A step: tile (1,0) one higher than everything else. Both tiles
        // sharing the edge between (0,0) and (1,0) must place its midpoint
        // at the same height, from their own fans.
        let height = |q: i32, r: i32| Some(if (q, r) == (1, 0) { 1 } else { 0 });
        let centre = |q: i32, r: i32| {
            let (x, z) = crate::geometry::flat_top_tile_center(q, r, 1.0);
            Vec3::new(x, 0.0, z)
        };
        let mid = (centre(0, 0) + centre(1, 0)) * 0.5;
        let from_a = surface_y(mid.xz(), qrz::Qrz { q: 0, r: 0, z: 0 }, centre(0, 0), height);
        let from_b = surface_y(mid.xz(), qrz::Qrz { q: 1, r: 0, z: 1 }, centre(1, 0), height);
        assert!((from_a - from_b).abs() < 1e-4, "{from_a} vs {from_b}");
        // And it lies strictly between the two tile heights.
        assert!(from_a > height_y(0.0) && from_a < height_y(1.0));
    }

    #[test]
    fn surface_rises_monotonically_toward_a_higher_neighbour() {
        let height = |q: i32, r: i32| Some(if (q, r) == (1, 0) { 3 } else { 0 });
        let floor = qrz::Qrz { q: 0, r: 0, z: 0 };
        let (cx, cz) = crate::geometry::flat_top_tile_center(0, 0, 1.0);
        let (nx, nz) = crate::geometry::flat_top_tile_center(1, 0, 1.0);
        let dir = Vec2::new(nx - cx, nz - cz).normalize();
        let mut last = f32::MIN;
        for k in 0..=8 {
            let p = Vec2::new(cx, cz) + dir * (0.1 * k as f32);
            let y = surface_y(p, floor, Vec3::new(cx, 0.0, cz), height);
            assert!(y >= last - 1e-6, "dipped at step {k}: {y} < {last}");
            last = y;
        }
    }

    #[test]
    fn absent_neighbours_do_not_move_the_centre() {
        let height = |q: i32, r: i32| ((q, r) == (0, 0)).then_some(4);
        let floor = qrz::Qrz { q: 0, r: 0, z: 4 };
        let y = surface_y(Vec2::ZERO, floor, Vec3::ZERO, height);
        assert_eq!(y, height_y(4.0));
        assert_eq!(cell_corner_zs((0, 0), flat_at(4)), [Some(4.0); 6]);
    }

    #[test]
    fn normals_point_up_on_flat_ground() {
        let centre = Vec3::new(0.0, 1.0, 0.0);
        let corners = corner_offsets(1.0).map(|o| Vec3::new(o.x, 1.0, o.y));
        assert!((centre_normal(centre, corners) - Vec3::Y).length() < 1e-6);
        let c = [Some(Vec3::ZERO), Some(Vec3::X), Some(Vec3::Z)];
        assert!((corner_normal(c) - Vec3::Y).length() < 1e-6);
    }
}
