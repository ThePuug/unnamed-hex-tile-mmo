//! The terrain surface: one height function shared by the mesh builder and
//! physics, so what an entity stands on is what is drawn.

//! A hex lattice at any scale carries one height per cell. Each cell renders
//! as a fan of six triangles from its centre to its six corners, and a corner
//! is one vertex shared by the three cells meeting there, at the mean of
//! the heights their ground runs through (`common::grid::Ground`): a cell's
//! centre stands at its own, so ground lying in a plane is drawn as it. Adjacent cells share corners and edges, so the surface is
//! watertight by construction; a large height difference between neighbours
//! is a steep triangle, not a gap.
//!
//! A cell's corner is its own value by the corner rule (`common::corner`):
//! cells whose levels are within a walkable span of each other meet, and
//! cells out of reach each keep their own height there, so a face between
//! them is a cliff, in the mesh and in physics alike.

use bevy::math::{Vec2, Vec3, Vec3Swizzles};

use common::grid::{Ground, HEX_RADIUS, RISE};

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

/// The lattice height, for [`height_y`], that water standing at the
/// z-level `w` is drawn at: half a step under its level, over the ground
/// it floods, and the sea, at zero, on its plane at the shore's ground.
pub fn water_z(w: i32) -> f32 {
    (w as f32 - 0.5).max(0.0)
}

/// A cell's own height at a corner from the cells meeting there, in z
/// units, the cell itself first, on a lattice `w` tiles wide: the corner
/// rule's value at that width, through the heights the cells' ground runs
/// through. Without the cell itself, the mean of those present
/// ([`corner_z`]); None when none is present.
pub fn own_corner_z(cells: [Option<Ground>; 3], w: i32) -> Option<f32> {
    let Some(own) = cells[0] else {
        // Without the tile itself, the others meet among themselves.
        return corner_z(cells);
    };
    // Measured from the cell's own level, so the sum keeps the fine steps
    // however high the ground stands.
    let level = cells.map(|c| c.map_or(f64::NAN, |g| (g.z - own.z) as f64));
    let through = cells.map(|c| c.map_or(f64::NAN, |g| g.above(own.z) as f64));
    Some(own.z as f32 + common::corner::meet_through(level, through, w as f64)[0] as f32)
}

/// Height of a corner from the cells meeting there, in z units: the mean of
/// those present. A cell can be absent at the edge of the loaded tiles;
/// physics and the tile outlines then take the mean of the rest, while the
/// mesh builds only with every cell present. None when none is present.
pub fn corner_z(cells: [Option<Ground>; 3]) -> Option<f32> {
    let base = cells.iter().flatten().next()?.z;
    let (sum, n) = cells.iter().flatten().fold((0.0, 0u32), |(s, n), g| (s + g.above(base), n + 1));
    Some(base as f32 + sum / n as f32)
}

/// The six corner heights of a tile, in z units, from a ground lookup over
/// the tiles ([`own_corner_z`]). A corner with no present tile (the owner
/// is absent too) is None.
pub fn cell_corner_zs<G: Into<Ground>>(
    cell: (i32, i32),
    mut ground: impl FnMut(i32, i32) -> Option<G>,
) -> [Option<f32>; 6] {
    let mut ground = |q: i32, r: i32| ground(q, r).map(Into::into);
    let own = ground(cell.0, cell.1);
    std::array::from_fn(|i| {
        let [a, b] = CORNER_NEIGHBOURS[i];
        own_corner_z([own, ground(cell.0 + a.0, cell.1 + a.1), ground(cell.0 + b.0, cell.1 + b.1)], 1)
    })
}

/// The fan triangle under `p` and its weights, for a cell centred at
/// `centre` with the given outer radius: the corners `(i, j)` it spans and
/// the weights `(centre, i, j)` that place `p` in it. Any per-vertex
/// quantity — height, normal — interpolates across the fan by these
/// weights, as the rasterizer interpolates the drawn triangle. `p` is
/// assumed inside the cell; a point exactly on an edge weighs the same from
/// either side because the edge is shared.
pub fn fan_weights(p: Vec2, centre: Vec2, outer_radius: f32) -> ((usize, usize), Vec3) {
    let d = p - centre;
    if d.length_squared() < 1e-12 {
        return ((0, 1), Vec3::X);
    }
    // Corners sit at -60°, 0°, 60°, … from the +x axis (E is corner 1), so
    // the sector from corner i to corner i+1 starts at (i-1)·60°.
    let angle = d.y.atan2(d.x).to_degrees();
    let i = ((angle + 60.0) / 60.0).floor().rem_euclid(6.0) as usize;
    let j = (i + 1) % 6;
    let offsets = corner_offsets(outer_radius);
    let (a, b) = (offsets[i], offsets[j]);
    // d = u·a + v·b.
    let det = a.x * b.y - a.y * b.x;
    let u = (d.x * b.y - d.y * b.x) / det;
    let v = (a.x * d.y - a.y * d.x) / det;
    ((i, j), Vec3::new(1.0 - u - v, u, v))
}

/// Height (world Y) of the fan surface under `p`, for a cell centred at
/// `centre` with the given outer radius, centre height and corner heights.
pub fn fan_height(p: Vec2, centre: Vec2, outer_radius: f32, centre_y: f32, corner_y: [f32; 6]) -> f32 {
    let ((i, j), w) = fan_weights(p, centre, outer_radius);
    w.x * centre_y + w.y * corner_y[i] + w.z * corner_y[j]
}

/// Height (world Y) of the tile surface at world `xz`, standing on the
/// tile at `(q, r)` whose ground is `floor`. Neighbours' ground comes from
/// `ground(q, r)`; an absent neighbour leaves its corners to the cells that
/// are present, exactly as the mesh does.
pub fn surface_y<G: Into<Ground>>(
    xz: Vec2,
    (q, r): (i32, i32),
    floor: Ground,
    floor_centre: Vec3,
    ground: impl FnMut(i32, i32) -> Option<G>,
) -> f32 {
    let corner_zs = cell_corner_zs((q, r), ground);
    let centre = floor.above(0);
    let corner_y = corner_zs.map(|z| height_y(z.unwrap_or(centre)));
    fan_height(xz, floor_centre.xz(), HEX_RADIUS, height_y(centre), corner_y)
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
        let at = |z: i32| Some(Ground::level(z));
        assert_eq!(corner_z([at(0), at(0), at(3)]), Some(1.0));
        assert_eq!(corner_z([at(2), None, at(4)]), Some(3.0));
        assert_eq!(corner_z([None, None, None]), None);
    }

    #[test]
    fn corner_neighbours_are_mutually_adjacent() {
        // The two neighbours sharing a corner must also neighbour each other,
        // or the corner is not a point three cells meet at.
        for [a, b] in CORNER_NEIGHBOURS {
            assert_eq!(qrz::hex_distance(a, b), 1, "{a:?} and {b:?} are not adjacent");
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
    fn fan_weights_are_barycentric_and_pick_the_sector_under_the_point() {
        let offsets = corner_offsets(1.0);
        for (i, o) in offsets.iter().enumerate() {
            let j = (i + 1) % 6;
            // A point a third of the way to each of corner i, corner j and
            // the centre lies in sector (i, j) with equal weights.
            let p = (*o + offsets[j]) / 3.0;
            let ((si, sj), w) = fan_weights(p, Vec2::ZERO, 1.0);
            assert_eq!((si, sj), (i, j));
            assert!((w.x + w.y + w.z - 1.0).abs() < 1e-5);
            assert!((w - Vec3::splat(1.0 / 3.0)).length() < 1e-5, "sector {i}: {w:?}");
        }
        assert_eq!(fan_weights(Vec2::ZERO, Vec2::ZERO, 1.0).1, Vec3::X);
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
        let from_a = surface_y(mid.xz(), (0, 0), Ground::level(0), centre(0, 0), height);
        let from_b = surface_y(mid.xz(), (1, 0), Ground::level(1), centre(1, 0), height);
        assert!((from_a - from_b).abs() < 1e-4, "{from_a} vs {from_b}");
        // And it lies strictly between the two tile heights.
        assert!(from_a > height_y(0.0) && from_a < height_y(1.0));
    }

    #[test]
    fn surface_rises_monotonically_toward_a_higher_neighbour() {
        let height = |q: i32, r: i32| Some(if (q, r) == (1, 0) { 3 } else { 0 });
        let (cx, cz) = crate::geometry::flat_top_tile_center(0, 0, 1.0);
        let (nx, nz) = crate::geometry::flat_top_tile_center(1, 0, 1.0);
        let dir = Vec2::new(nx - cx, nz - cz).normalize();
        let mut last = f32::MIN;
        for k in 0..=8 {
            let p = Vec2::new(cx, cz) + dir * (0.1 * k as f32);
            let y = surface_y(p, (0, 0), Ground::level(0), Vec3::new(cx, 0.0, cz), height);
            assert!(y >= last - 1e-6, "dipped at step {k}: {y} < {last}");
            last = y;
        }
    }

    #[test]
    fn absent_neighbours_do_not_move_the_centre() {
        let height = |q: i32, r: i32| ((q, r) == (0, 0)).then_some(4);
        let y = surface_y(Vec2::ZERO, (0, 0), Ground::level(4), Vec3::ZERO, height);
        assert_eq!(y, height_y(4.0));
        assert_eq!(cell_corner_zs((0, 0), flat_at(4)), [Some(4.0); 6]);
    }

    /// Ground lying in a plane is drawn as that plane, across tiles and
    /// inside them, though its levels step: the surface under any point is
    /// the plane's height there.
    #[test]
    fn ground_in_a_plane_is_drawn_as_the_plane() {
        let centre = |q: i32, r: i32| {
            let (x, z) = crate::geometry::flat_top_tile_center(q, r, 1.0);
            Vec3::new(x, 0.0, z)
        };
        // Under one level a tile either way, so every tile meets its neighbours.
        let plane = |p: Vec3| 40.0 + 0.21 * p.x as f64 - 0.13 * p.z as f64;
        let ground = |q: i32, r: i32| Some(Ground::through(plane(centre(q, r))));
        for (q, r) in [(0, 0), (3, -1), (-2, 5)] {
            for off in [Vec2::ZERO, Vec2::new(0.4, 0.2), Vec2::new(-0.7, 0.1), Vec2::new(0.1, -0.8)] {
                let at = centre(q, r) + Vec3::new(off.x, 0.0, off.y);
                let y = surface_y(at.xz(), (q, r), ground(q, r).unwrap(), centre(q, r), ground);
                let want = height_y(plane(at) as f32);
                assert!((y - want).abs() < 2.0 * RISE / Ground::FINE as f32, "({q}, {r}) + {off}: {y} for {want}");
            }
        }
    }

    /// A wall stands between tiles two levels apart whatever their ground
    /// runs through, and none between tiles one apart.
    #[test]
    fn a_wall_stands_where_the_levels_are_out_of_reach() {
        let wall = |low: Ground, high: Ground| {
            let ground = move |q: i32, _: i32| Some(if q >= 1 { high } else { low });
            // The corner south of the edge between them: the low tile's SE, the high one's W.
            let (a, b) = (cell_corner_zs((0, 0), ground)[2].unwrap(), cell_corner_zs((1, 0), ground)[4].unwrap());
            (a - b).abs()
        };
        assert!(wall(Ground { z: 10, fine: 127 }, Ground { z: 12, fine: -128 }) > 0.9, "a wall, though the heights are barely a level apart");
        assert!(wall(Ground { z: 10, fine: -128 }, Ground { z: 11, fine: 127 }) < 1e-5, "no wall, though the heights are nearly two apart");
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
