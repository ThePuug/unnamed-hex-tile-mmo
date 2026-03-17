//! Pure tile geometry computation — no Bevy dependency.
//!
//! Extracted from `Map::generate_chunk_mesh()`. Produces identical geometry
//! for both client mesh rendering and server QEM decimation input.

use std::collections::HashMap;
use bevy::math::Vec3;

const SQRT_3_F64: f64 = 1.7320508075688772;

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

    let direction_to_vertices = [
        (4, 5), // Dir 0: West edge → SW(4), NW(5)
        (3, 4), // Dir 1: SW edge → S(3), SW(4)
        (2, 3), // Dir 2: SE edge → SE(2), S(3)
        (1, 2), // Dir 3: East edge → NE(1), SE(2)
        (0, 1), // Dir 4: NE edge → N(0), NE(1)
        (5, 0), // Dir 5: NW edge → NW(5), N(0)
    ];

    let skirt_vertex_map = [
        (4, 5, 2, 1), // Dir 0
        (3, 4, 1, 0), // Dir 1
        (2, 3, 0, 5), // Dir 2
        (1, 2, 5, 4), // Dir 3
        (0, 1, 4, 3), // Dir 4
        (5, 0, 3, 2), // Dir 5
    ];

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
        positions.push(tile_verts[6].into());
        normals.push(hex_vertex_normal(&tile_verts, 6).into());

        // Outer vertices (0-5)
        for i in 0..6 {
            positions.push(tile_verts[i].into());
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
            positions.extend([[curr_v1.x, curr_v1.y, curr_v1.z],
                              [curr_v2.x, curr_v2.y, curr_v2.z],
                              [neighbor_v2.x, neighbor_v2.y, neighbor_v2.z],
                              [neighbor_v1.x, neighbor_v1.y, neighbor_v1.z]]);
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

/// Find edges with exactly one incident triangle, then traverse them in order
/// to produce an ordered boundary vertex loop.
fn extract_ordered_boundary(indices: &[u32]) -> Vec<u32> {
    use std::collections::HashMap;

    // Count how many triangles share each edge
    let mut edge_count: HashMap<(u32, u32), u32> = HashMap::new();
    for tri in indices.chunks(3) {
        if tri.len() < 3 { continue; }
        for i in 0..3 {
            let v0 = tri[i];
            let v1 = tri[(i + 1) % 3];
            let key = (v0.min(v1), v0.max(v1));
            *edge_count.entry(key).or_insert(0) += 1;
        }
    }

    // Boundary edges: count == 1
    let mut bnd_adj: HashMap<u32, Vec<u32>> = HashMap::new();
    for (&(v0, v1), &count) in &edge_count {
        if count == 1 {
            bnd_adj.entry(v0).or_default().push(v1);
            bnd_adj.entry(v1).or_default().push(v0);
        }
    }

    if bnd_adj.is_empty() {
        return Vec::new();
    }

    // Find ALL boundary loops, return the largest (chunk perimeter, not skirt edges)
    let mut global_visited = std::collections::HashSet::new();
    let mut largest_loop: Vec<u32> = Vec::new();

    let mut all_verts: Vec<u32> = bnd_adj.keys().copied().collect();
    all_verts.sort();

    for &start in &all_verts {
        if global_visited.contains(&start) { continue; }

        let mut loop_verts = vec![start];
        global_visited.insert(start);
        let mut curr = start;

        loop {
            let neighbors = match bnd_adj.get(&curr) {
                Some(n) => n,
                None => break,
            };
            let next = neighbors.iter()
                .filter(|&&n| !global_visited.contains(&n))
                .min();
            match next {
                Some(&n) => {
                    loop_verts.push(n);
                    global_visited.insert(n);
                    curr = n;
                }
                None => break,
            }
        }

        if loop_verts.len() > largest_loop.len() {
            largest_loop = loop_verts;
        }
    }

    largest_loop
}

/// Compute slope-adjusted vertices for a hex tile.
/// Mechanical copy of `Map::vertices_with_slopes_inner`.
fn vertices_with_slopes(
    map: &qrz::Map<()>,
    qrz: qrz::Qrz,
    elevations: &HashMap<(i32, i32), i32>,
) -> Vec<Vec3> {
    let mut verts = map.vertices(qrz);
    let rise = map.rise();
    let mut vertex_adjustments: [Vec<f32>; 6] = Default::default();

    let direction_to_vertices = [
        (4, 5), (3, 4), (2, 3), (1, 2), (0, 1), (5, 0),
    ];

    for (dir_idx, direction) in qrz::DIRECTIONS.iter().enumerate() {
        let neighbor_qrz = qrz + *direction;
        if let Some(&neighbor_z) = elevations.get(&(neighbor_qrz.q, neighbor_qrz.r)) {
            let elevation_diff = neighbor_z - qrz.z;
            let adjustment = if elevation_diff > 0 {
                rise * 0.5
            } else if elevation_diff < 0 {
                rise * -0.5
            } else {
                0.0
            };
            if adjustment != 0.0 {
                let (v1, v2) = direction_to_vertices[dir_idx];
                vertex_adjustments[v1].push(adjustment);
                vertex_adjustments[v2].push(adjustment);
            }
        }
    }

    for (i, adjustments) in vertex_adjustments.iter().enumerate() {
        if let Some(&max_adj) = adjustments.iter()
            .max_by(|a, b| a.abs().partial_cmp(&b.abs()).unwrap())
        {
            verts[i].y += max_adj;
        }
    }

    verts
}

/// Compute per-vertex normal for a hex tile from its actual geometry.
/// Mechanical copy of `Map::hex_vertex_normal`.
fn hex_vertex_normal(verts: &[Vec3], vertex_idx: usize) -> Vec3 {
    let center = verts[6];
    if vertex_idx == 6 {
        let mut sum = Vec3::ZERO;
        for i in 0..6 {
            sum += (verts[(i + 1) % 6] - center).cross(verts[i] - center);
        }
        if sum.length_squared() > 1e-10 { sum.normalize() } else { Vec3::Y }
    } else {
        let j = vertex_idx;
        let n1 = (verts[(j + 1) % 6] - center).cross(verts[j] - center);
        let n2 = (verts[j] - center).cross(verts[(j + 5) % 6] - center);
        let sum = n1 + n2;
        if sum.length_squared() > 1e-10 { sum.normalize() } else { Vec3::Y }
    }
}
