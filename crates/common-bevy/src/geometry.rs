//! Pure tile geometry computation — no Bevy dependency.
//!
//! Extracted from `Map::generate_chunk_mesh()`. Produces identical geometry
//! for both client mesh rendering and server QEM decimation input.

use std::collections::HashMap;
use bevy::math::Vec3;

// Re-export slope blending from common (single source of truth for both
// mesh generation and decimation).
pub use common::hex_slope::{slope_adjustments, DIRECTION_TO_VERTICES};

/// Skirt vertex mapping: for each direction, (curr_v1, curr_v2, neighbor_v1, neighbor_v2).
/// curr_v1 ↔ neighbor_v1, curr_v2 ↔ neighbor_v2 are at the same XZ position.
pub const SKIRT_VERTEX_MAP: [(usize, usize, usize, usize); 6] = [
    (4, 5, 2, 1), // Dir 0
    (3, 4, 1, 0), // Dir 1
    (2, 3, 0, 5), // Dir 2
    (1, 2, 5, 4), // Dir 3
    (0, 1, 4, 3), // Dir 4
    (5, 0, 3, 2), // Dir 5
];

/// Vertex XZ offsets for flat-top hex, relative to tile center.
/// Returns [(x, z_world); 7] — 6 outer vertices + center.
/// Y is not included; it comes from elevation + slope adjustment.
pub fn flat_top_vertex_offsets(radius: f32) -> [(f32, f32); 7] {
    let w = (radius as f64 * (3.0_f64).sqrt() / 2.0) as f32;
    let h = radius / 2.0;
    [
        (h, -w),        // 0 NE
        (radius, 0.0),  // 1 E
        (h, w),         // 2 SE
        (-h, w),        // 3 SW
        (-radius, 0.0), // 4 W
        (-h, -w),       // 5 NW
        (0.0, 0.0),     // 6 Center
    ]
}

/// Compute tile center world position (x, z_world) for flat-top hex.
pub fn flat_top_tile_center(q: i32, r: i32, radius: f32) -> (f32, f32) {
    let x = (1.5 * q as f64 * radius as f64) as f32;
    let z = ((q as f64 * (3.0_f64).sqrt() / 2.0 + r as f64 * (3.0_f64).sqrt()) * radius as f64)
        as f32;
    (x, z)
}

/// Compute per-vertex normal for a hex tile from its actual geometry.
///
/// verts: 7 vertices [v0..v5 outer, v6 center].
/// vertex_idx: 0-6.
pub fn hex_vertex_normal(verts: &[Vec3], vertex_idx: usize) -> Vec3 {
    let center = verts[6];
    if vertex_idx == 6 {
        let mut sum = Vec3::ZERO;
        for i in 0..6 {
            sum += (verts[(i + 1) % 6] - center).cross(verts[i] - center);
        }
        if sum.length_squared() > 1e-10 {
            sum.normalize()
        } else {
            Vec3::Y
        }
    } else {
        let j = vertex_idx;
        let n1 = (verts[(j + 1) % 6] - center).cross(verts[j] - center);
        let n2 = (verts[j] - center).cross(verts[(j + 5) % 6] - center);
        let sum = n1 + n2;
        if sum.length_squared() > 1e-10 {
            sum.normalize()
        } else {
            Vec3::Y
        }
    }
}

/// Raw geometry for a set of hex tiles. No Bevy types.
pub struct TileGeometry {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    /// Blended surface Y at each tile center: (q, r) → world Y after slope blending.
    /// Covers all tiles passed to `compute_tile_geometry` (chunk + 1-ring neighbors).
    pub surface_y: HashMap<(i32, i32), f32>,
    /// Ordered vertex indices forming the mesh perimeter (boundary edges = single-face edges).
    /// Used by QEM to lock boundary edges. Empty if mesh has no boundary.
    pub boundary_indices: Vec<u32>,
}

/// Compute full-detail hex mesh geometry from tile data.
///
/// `chunk_tiles`: tile positions (q, r, z) within the chunk.
/// `elevations`: (q, r) → z lookup covering chunk + 1-ring neighbors.
/// `radius`, `rise`: hex geometry constants.
///
/// Produces center + 6 outer vertices per tile (7 × N), plus cliff skirt
/// vertices for downward elevation changes. Same geometry the client renders.
pub fn compute_tile_geometry(
    chunk_tiles: &[qrz::Qrz],
    elevations: &HashMap<(i32, i32), i32>,
    radius: f32,
    rise: f32,
    chunk_origin: Vec3,
) -> TileGeometry {
    // Empty map for vertex computation — only needs geometry params
    let map: qrz::Map<()> = qrz::Map::new(radius, rise, qrz::HexOrientation::FlatTop);

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut surface_y: HashMap<(i32, i32), f32> = HashMap::new();
    // Track perimeter vertices for boundary locking in QEM.
    // Ordered list of vertex indices forming the chunk's outer boundary.
    let mut perimeter_vertices: Vec<u32> = Vec::new();
    let chunk_tile_set: std::collections::HashSet<(i32, i32)> = chunk_tiles.iter()
        .map(|t| (t.q, t.r))
        .collect();

    let skirt_vertex_map = SKIRT_VERTEX_MAP;

    // Pre-compute surface_y for all tiles in the elevation lookup (chunk + 1-ring neighbors).
    // Both chunks sharing an edge will have the shared tiles in their lookup and produce
    // identical blended Y values — this is the convergence guarantee for border elevations.
    for (&(q, r), &z) in elevations {
        let tile = qrz::Qrz { q, r, z };
        let raw = map.vertices(tile);
        // Center vertex Y = raw[6].y (no slope adjustment on center)
        surface_y.insert((q, r), raw[6].y);
    }

    for &tile_qrz in chunk_tiles {
        let raw_verts = map.vertices(tile_qrz);

        // Slope-adjusted vertices
        let slope_verts = vertices_with_slopes(&map, tile_qrz, elevations);

        // Use raw XZ for edge alignment, slope-adjusted Y for height
        let tile_verts: Vec<Vec3> = raw_verts.iter().enumerate().map(|(i, &raw_pos)| {
            if i < 6 {
                Vec3::new(raw_pos.x, slope_verts[i].y, raw_pos.z)
            } else {
                raw_pos
            }
        }).collect();

        let base_idx = positions.len() as u32;

        // Center vertex (index 6 in tile_verts → first emitted vertex)
        // Rebase to chunk-local coordinates for f32 precision at any world distance.
        positions.push((tile_verts[6] - chunk_origin).into());
        normals.push(hex_vertex_normal(&tile_verts, 6).into());

        // Outer vertices (0-5)
        for i in 0..6 {
            positions.push((tile_verts[i] - chunk_origin).into());
            normals.push(hex_vertex_normal(&tile_verts, i).into());
        }

        // 6 triangles for hex top surface (CCW winding)
        for i in 0..6u32 {
            let v1 = base_idx + 1 + i;
            let v2 = base_idx + 1 + ((i + 1) % 6);
            indices.extend([base_idx, v2, v1]);
        }

        // Vertical skirt geometry for cliff edges
        for (dir_idx, direction) in qrz::DIRECTIONS.iter().enumerate() {
            let neighbor_qrz = tile_qrz + *direction;
            let neighbor_z = match elevations.get(&(neighbor_qrz.q, neighbor_qrz.r)) {
                Some(&z) => z,
                None => continue,
            };

            let elevation_diff = neighbor_z - tile_qrz.z;
            if elevation_diff >= 0 {
                continue;
            }

            let actual_neighbor = qrz::Qrz { q: neighbor_qrz.q, r: neighbor_qrz.r, z: neighbor_z };
            let neighbor_raw = map.vertices(actual_neighbor);
            let neighbor_slope = vertices_with_slopes(&map, actual_neighbor, elevations);
            let neighbor_verts: Vec<Vec3> = neighbor_raw.iter().enumerate().map(|(i, &raw_pos)| {
                if i < 6 {
                    Vec3::new(raw_pos.x, neighbor_slope[i].y, raw_pos.z)
                } else {
                    raw_pos
                }
            }).collect();

            let (cv1, cv2, nv1, nv2) = skirt_vertex_map[dir_idx];

            let curr_v1 = tile_verts[cv1];
            let curr_v2 = tile_verts[cv2];
            let neighbor_v1 = neighbor_verts[nv1];
            let neighbor_v2 = neighbor_verts[nv2];

            let edge_dir = (curr_v2 - curr_v1).normalize();
            let outward_normal = edge_dir.cross(Vec3::new(0., -1., 0.)).normalize();

            let skirt_base = positions.len() as u32;
            let cv1 = curr_v1 - chunk_origin;
            let cv2 = curr_v2 - chunk_origin;
            let nv2 = neighbor_v2 - chunk_origin;
            let nv1 = neighbor_v1 - chunk_origin;
            positions.extend([[cv1.x, cv1.y, cv1.z],
                              [cv2.x, cv2.y, cv2.z],
                              [nv2.x, nv2.y, nv2.z],
                              [nv1.x, nv1.y, nv1.z]]);
            let n: [f32; 3] = outward_normal.into();
            normals.extend([n; 4]);

            indices.extend([skirt_base, skirt_base + 1, skirt_base + 2]);
            indices.extend([skirt_base, skirt_base + 2, skirt_base + 3]);
        }

        // Track perimeter vertices: outward-facing edges of boundary tiles.
        for (dir_idx, direction) in qrz::DIRECTIONS.iter().enumerate() {
            let neighbor_qrz = tile_qrz + *direction;
            if chunk_tile_set.contains(&(neighbor_qrz.q, neighbor_qrz.r)) {
                continue; // Interior edge — neighbor is in the chunk
            }

            // This edge faces outward. Check if a cliff skirt was created.
            let neighbor_z = elevations.get(&(neighbor_qrz.q, neighbor_qrz.r)).copied();
            let has_skirt = neighbor_z.map_or(false, |nz| nz < tile_qrz.z);

            let (cv1_idx, cv2_idx, _nv1_idx, _nv2_idx) = skirt_vertex_map[dir_idx];

            if has_skirt {
                // Cliff: lock the bottom skirt vertices (where the skirt meets neighbor surface).
                // The skirt was pushed as 4 vertices: [curr_v1, curr_v2, neighbor_v2, neighbor_v1]
                // We need the neighbor_v1 and neighbor_v2 indices (skirt_base + 2, skirt_base + 3).
                // But we don't have skirt_base here — it was in the skirt loop above.
                // Instead, lock the surface top vertices (they'll merge with skirt top via dedup).
                // The skirt bottom is at a different position so it's a separate vertex after dedup.
                // For now, lock the surface outer vertices — this prevents the seam from moving.
                perimeter_vertices.push(base_idx + 1 + cv1_idx as u32);
                perimeter_vertices.push(base_idx + 1 + cv2_idx as u32);
            } else {
                // No cliff: lock the surface outer vertices at this edge.
                perimeter_vertices.push(base_idx + 1 + cv1_idx as u32);
                perimeter_vertices.push(base_idx + 1 + cv2_idx as u32);
            }
        }
    }

    // Deduplicate perimeter vertices (each vertex appears from 1-2 adjacent outward edges)
    perimeter_vertices.sort();
    perimeter_vertices.dedup();

    TileGeometry { positions, normals, indices, surface_y, boundary_indices: perimeter_vertices }
}

/// Compute slope-adjusted vertices for a hex tile.
fn vertices_with_slopes(
    map: &qrz::Map<()>,
    qrz: qrz::Qrz,
    elevations: &HashMap<(i32, i32), i32>,
) -> Vec<Vec3> {
    let mut verts = map.vertices(qrz);
    let adjustments = slope_adjustments(qrz.z, map.rise(), |dir_idx| {
        let n = qrz + qrz::DIRECTIONS[dir_idx];
        elevations.get(&(n.q, n.r)).copied()
    });
    for i in 0..6 {
        verts[i].y += adjustments[i];
    }
    verts
}
