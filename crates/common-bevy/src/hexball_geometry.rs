//! Hexball geometry — one function for any hex radius.
//!
//! A hexball is a tile at a larger scale. r=0 produces a single tile (identical
//! to `compute_tile_geometry`). r≥1 uses `hex_decimate::decimate_hexball()` for
//! tile classification, then generates inscribed hex + partial fans + full tiles.

use bevy::{
    asset::RenderAssetUsages,
    math::Vec3,
    prelude::Mesh,
    render::render_resource::PrimitiveTopology,
};
use bevy_mesh::Indices;

use crate::geometry::{
    flat_top_tile_center, flat_top_vertex_offsets, hex_vertex_normal, slope_adjustments,
    SKIRT_VERTEX_MAP,
};

/// Raw geometry output from a single hexball.
pub struct HexballGeometry {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

/// A single decimated hexball within a chunk.
pub struct HexballDecimation {
    pub center_q: i32,
    pub center_r: i32,
    pub center_z: i32,
    pub radius: u32,
}

/// Decimation plan for an entire chunk.
pub struct ChunkDecimation {
    pub hexballs: Vec<HexballDecimation>,
    pub survivors: Vec<(i32, i32, i32)>,
}

/// Build a combined Bevy `Mesh` from a chunk decimation plan.
pub fn build_chunk_mesh(
    decimation: &ChunkDecimation,
    hex_radius: f32,
    rise: f32,
    chunk_origin: Vec3,
    tile_z: &impl Fn(i32, i32) -> Option<i32>,
) -> Mesh {
    let mut combined = HexballGeometry {
        positions: Vec::new(),
        normals: Vec::new(),
        indices: Vec::new(),
    };

    for hb in &decimation.hexballs {
        let geom = compute_hexball_geometry(
            hb.center_q, hb.center_r, hb.center_z, hb.radius,
            hex_radius, rise, chunk_origin, tile_z,
        );
        merge_geometry(&mut combined, &geom);
    }

    for &(q, r, z) in &decimation.survivors {
        let geom = compute_hexball_geometry(q, r, z, 0, hex_radius, rise, chunk_origin, tile_z);
        merge_geometry(&mut combined, &geom);
    }

    let vert_count = combined.positions.len();
    let verts: Vec<Vec3> = combined.positions.iter().map(|p| Vec3::from_array(*p)).collect();
    let norms: Vec<Vec3> = combined.normals.iter().map(|n| Vec3::from_array(*n)).collect();

    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, verts)
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_UV_0,
        (0..vert_count).map(|_| [0.0f32, 0.0]).collect::<Vec<[f32; 2]>>(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, norms)
    .with_inserted_indices(Indices::U32(combined.indices))
}

/// Produce mesh geometry for a hexball of the given radius.
///
/// r=0: Single hex tile, identical to `compute_tile_geometry()`.
/// r≥1 (odd only): Inscribed hex + partial fans + full residuals via `hex_decimate`.
pub fn compute_hexball_geometry(
    center_q: i32,
    center_r: i32,
    center_z: i32,
    radius: u32,
    hex_radius: f32,
    rise: f32,
    chunk_origin: Vec3,
    tile_z: &impl Fn(i32, i32) -> Option<i32>,
) -> HexballGeometry {
    assert!(
        radius == 0 || radius % 2 == 1,
        "hexball radius must be 0 or odd, got {radius}"
    );

    if radius == 0 {
        return build_r0(center_q, center_r, center_z, hex_radius, rise, chunk_origin, tile_z);
    }

    build_rn(center_q, center_r, center_z, radius, hex_radius, rise, chunk_origin, tile_z)
}

// ── Vertex helpers ───────────────────────────────────────────────────────────

/// All 7 vertex positions for a tile (6 outer + center), slope-adjusted.
fn tile_vertices(
    q: i32, r: i32, z: i32,
    hex_radius: f32, rise: f32,
    tile_z: &impl Fn(i32, i32) -> Option<i32>,
) -> [Vec3; 7] {
    let (cx, cz) = flat_top_tile_center(q, r, hex_radius);
    let offsets = flat_top_vertex_offsets(hex_radius);
    let adj = slope_adjustments(z, rise, |dir_idx| {
        let dir = qrz::DIRECTIONS[dir_idx];
        tile_z(q + dir.q, r + dir.r)
    });
    let base_y = z as f32 * rise + rise;
    let mut verts = [Vec3::ZERO; 7];
    for i in 0..6 {
        verts[i] = Vec3::new(cx + offsets[i].0, base_y + adj[i], cz + offsets[i].1);
    }
    verts[6] = Vec3::new(cx, base_y, cz);
    verts
}

/// World position of a single tile vertex, slope-adjusted.
fn tile_vertex_pos(
    q: i32, r: i32, z: i32, vertex_idx: usize,
    hex_radius: f32, rise: f32,
    tile_z: &impl Fn(i32, i32) -> Option<i32>,
) -> Vec3 {
    let (cx, cz) = flat_top_tile_center(q, r, hex_radius);
    let offsets = flat_top_vertex_offsets(hex_radius);
    let (ox, oz) = offsets[vertex_idx];
    let adj = slope_adjustments(z, rise, |dir_idx| {
        let dir = qrz::DIRECTIONS[dir_idx];
        tile_z(q + dir.q, r + dir.r)
    });
    let y = z as f32 * rise + rise + adj[vertex_idx];
    Vec3::new(cx + ox, y, cz + oz)
}

// ── r=0: Single tile ─────────────────────────────────────────────────────────

fn build_r0(
    q: i32, r: i32, z: i32,
    hex_radius: f32, rise: f32, chunk_origin: Vec3,
    tile_z: &impl Fn(i32, i32) -> Option<i32>,
) -> HexballGeometry {
    let verts = tile_vertices(q, r, z, hex_radius, rise, tile_z);
    let mut positions = Vec::with_capacity(31);
    let mut normals = Vec::with_capacity(31);
    let mut indices = Vec::with_capacity(54);

    // Center vertex first
    positions.push((verts[6] - chunk_origin).into());
    normals.push(hex_vertex_normal(&verts, 6).into());

    for i in 0..6 {
        positions.push((verts[i] - chunk_origin).into());
        normals.push(hex_vertex_normal(&verts, i).into());
    }

    // 6 triangles (CCW winding)
    for i in 0..6u32 {
        indices.extend([0, 1 + ((i + 1) % 6), 1 + i]);
    }

    emit_tile_skirts(q, r, z, &verts, hex_radius, rise, chunk_origin, tile_z,
        &mut positions, &mut normals, &mut indices);

    HexballGeometry { positions, normals, indices }
}

// ── r≥1: Unified path via hex_decimate ───────────────────────────────────────

fn build_rn(
    center_q: i32, center_r: i32, center_z: i32, radius: u32,
    hex_radius: f32, rise: f32, chunk_origin: Vec3,
    tile_z: &impl Fn(i32, i32) -> Option<i32>,
) -> HexballGeometry {
    let hb = common::hex_decimate::decimate_hexball(
        center_q, center_r, radius, u32::MAX, tile_z,
    ).expect("all tiles within hexball must exist for geometry generation");

    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut indices = Vec::new();

    // ── Inscribed hex: 6 triangles ──
    // Center vertex
    let center_y = center_z as f32 * rise + rise;
    let (ccx, ccz) = flat_top_tile_center(center_q, center_r, hex_radius);
    let center_world = Vec3::new(ccx, center_y, ccz);

    // 6 boundary vertices from DecimatedHexball.boundary_tiles
    // These are in CCW order (NE→E→SE→SW→W→NW) matching flat-top vertex convention.
    let bv_world: [Vec3; 6] = std::array::from_fn(|i| {
        let (tq, tr, vi) = hb.boundary_tiles[i];
        let tz = tile_z(tq, tr).unwrap_or(center_z);
        tile_vertex_pos(tq, tr, tz, vi as usize, hex_radius, rise, tile_z)
    });

    // Build 7-vertex array for normal computation [BV0..BV5, center]
    let inner_hex_verts: [Vec3; 7] = {
        let mut v = [Vec3::ZERO; 7];
        for i in 0..6 { v[i] = bv_world[i]; }
        v[6] = center_world;
        v
    };

    // Emit center (index 0)
    positions.push((center_world - chunk_origin).into());
    normals.push(hex_vertex_normal(&inner_hex_verts, 6).into());

    // Emit boundary vertices (indices 1-6)
    for i in 0..6 {
        positions.push((bv_world[i] - chunk_origin).into());
        normals.push(hex_vertex_normal(&inner_hex_verts, i).into());
    }

    // 6 triangles — CCW winding: [center, BV[(i+1)%6], BV[i]]
    for i in 0..6u32 {
        indices.extend([0, 1 + ((i + 1) % 6), 1 + i]);
    }

    // ── Partial fans: 3 triangles each ──
    for pr in &hb.partial_residuals {
        let tv = tile_vertices(pr.q, pr.r, pr.original_z, hex_radius, rise, tile_z);

        // Fan center: tile center XZ, Y from snapped_z
        let fan_center_y = pr.snapped_z as f32 * rise + rise;
        let fan_center = Vec3::new(tv[6].x, fan_center_y, tv[6].z);

        // Surviving triangles are 3 consecutive indices: [(e+5)%6, e, (e+1)%6]
        // where e is the inscribed hex edge this tile straddles.
        // The 4 unique outer vertices are: st[0], st[1], st[2], (st[2]+1)%6.
        let st = &pr.surviving_triangles;
        let edge = st[1] as usize;
        let edge_next = (edge + 1) % 6;
        let ov_indices = [st[0] as usize, st[1] as usize, st[2] as usize, (st[2] as usize + 1) % 6];

        // The two inner vertices (ov[0] and ov[3]) sit on the inscribed hex
        // boundary. Their heights must match the BV positions, not the tile's
        // own slope blending (which uses a different tile z and can diverge).
        let mut ov_pos = [tv[ov_indices[0]], tv[ov_indices[1]], tv[ov_indices[2]], tv[ov_indices[3]]];
        ov_pos[0] = bv_world[edge];      // ov[0] ↔ BV[edge]
        ov_pos[3] = bv_world[edge_next]; // ov[3] ↔ BV[edge_next]

        let fan_base = positions.len() as u32;

        // Emit: center(0), ov[0](1), ov[1](2), ov[2](3), ov[3](4)
        positions.push((fan_center - chunk_origin).into());
        for p in &ov_pos {
            positions.push((*p - chunk_origin).into());
        }

        // 3 triangles — CCW winding: center, v[(t+1)%6], v[t]
        indices.extend([fan_base, fan_base + 2, fan_base + 1]);
        indices.extend([fan_base, fan_base + 3, fan_base + 2]);
        indices.extend([fan_base, fan_base + 4, fan_base + 3]);

        // Per-vertex normals from adjacent face normals (using overridden positions)
        let fn0 = triangle_normal(fan_center, ov_pos[1], ov_pos[0]);
        let fn1 = triangle_normal(fan_center, ov_pos[2], ov_pos[1]);
        let fn2 = triangle_normal(fan_center, ov_pos[3], ov_pos[2]);

        normals.push(avg_normal(&[fn0, fn1, fn2]).into()); // center
        normals.push(fn0.into());                           // ov[0]
        normals.push(avg_normal(&[fn0, fn1]).into());       // ov[1]
        normals.push(avg_normal(&[fn1, fn2]).into());       // ov[2]
        normals.push(fn2.into());                           // ov[3]

        // Fan skirts — only the 3 outward edges
        for (k, &tri_idx) in st.iter().enumerate() {
            let vi_a = tri_idx as usize;
            let dir_idx = (4 + 6 - vi_a) % 6;
            let dir = qrz::DIRECTIONS[dir_idx];
            let nq = pr.q + dir.q;
            let nr = pr.r + dir.r;

            let nz = match tile_z(nq, nr) {
                Some(z) => z,
                None => continue,
            };
            if nz >= pr.original_z { continue; }

            let n_verts = tile_vertices(nq, nr, nz, hex_radius, rise, tile_z);
            let (_, _, nv1_idx, nv2_idx) = SKIRT_VERTEX_MAP[dir_idx];

            let va = ov_pos[k];
            let vb = ov_pos[k + 1];
            emit_skirt_quad(va, vb, n_verts[nv1_idx], n_verts[nv2_idx],
                chunk_origin, &mut positions, &mut normals, &mut indices);
        }
    }

    // ── Full residuals: 6 triangles each ──
    for fr in &hb.full_residuals {
        let tile = build_r0(fr.q, fr.r, fr.z, hex_radius, rise, chunk_origin, tile_z);
        merge_geometry_into(&mut positions, &mut normals, &mut indices, &tile);
    }

    HexballGeometry { positions, normals, indices }
}

// ── Shared helpers ───────────────────────────────────────────────────────────

fn emit_tile_skirts(
    q: i32, r: i32, z: i32, verts: &[Vec3; 7],
    hex_radius: f32, rise: f32, chunk_origin: Vec3,
    tile_z: &impl Fn(i32, i32) -> Option<i32>,
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    indices: &mut Vec<u32>,
) {
    for (dir_idx, direction) in qrz::DIRECTIONS.iter().enumerate() {
        let nq = q + direction.q;
        let nr = r + direction.r;
        let neighbor_z = match tile_z(nq, nr) {
            Some(nz) => nz,
            None => continue,
        };
        if neighbor_z >= z { continue; }

        let n_verts = tile_vertices(nq, nr, neighbor_z, hex_radius, rise, tile_z);
        let (cv1, cv2, nv1, nv2) = SKIRT_VERTEX_MAP[dir_idx];
        emit_skirt_quad(verts[cv1], verts[cv2], n_verts[nv1], n_verts[nv2],
            chunk_origin, positions, normals, indices);
    }
}

fn emit_skirt_quad(
    curr_v1: Vec3, curr_v2: Vec3, neighbor_v1: Vec3, neighbor_v2: Vec3,
    chunk_origin: Vec3,
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    indices: &mut Vec<u32>,
) {
    let edge_dir = (curr_v2 - curr_v1).normalize();
    let outward_normal = edge_dir.cross(Vec3::new(0.0, -1.0, 0.0)).normalize();

    let skirt_base = positions.len() as u32;
    let cv1: [f32; 3] = (curr_v1 - chunk_origin).into();
    let cv2: [f32; 3] = (curr_v2 - chunk_origin).into();
    let nv2: [f32; 3] = (neighbor_v2 - chunk_origin).into();
    let nv1: [f32; 3] = (neighbor_v1 - chunk_origin).into();
    positions.extend([cv1, cv2, nv2, nv1]);
    let n: [f32; 3] = outward_normal.into();
    normals.extend([n; 4]);

    indices.extend([skirt_base, skirt_base + 1, skirt_base + 2]);
    indices.extend([skirt_base, skirt_base + 2, skirt_base + 3]);
}

fn triangle_normal(a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let n = (b - a).cross(c - a);
    if n.length_squared() > 1e-10 { n.normalize() } else { Vec3::Y }
}

fn avg_normal(normals: &[Vec3]) -> Vec3 {
    let sum: Vec3 = normals.iter().copied().sum();
    if sum.length_squared() > 1e-10 { sum.normalize() } else { Vec3::Y }
}

fn merge_geometry(target: &mut HexballGeometry, source: &HexballGeometry) {
    let offset = target.positions.len() as u32;
    target.positions.extend_from_slice(&source.positions);
    target.normals.extend_from_slice(&source.normals);
    target.indices.extend(source.indices.iter().map(|i| i + offset));
}

fn merge_geometry_into(
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    indices: &mut Vec<u32>,
    source: &HexballGeometry,
) {
    let offset = positions.len() as u32;
    positions.extend_from_slice(&source.positions);
    normals.extend_from_slice(&source.normals);
    indices.extend(source.indices.iter().map(|i| i + offset));
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_tile_z(elevations: &HashMap<(i32, i32), i32>) -> impl Fn(i32, i32) -> Option<i32> + '_ {
        move |q, r| elevations.get(&(q, r)).copied()
    }

    /// Fill a hex area of given radius + 1-ring buffer, all at the same z.
    fn flat_tiles(cq: i32, cr: i32, radius: u32, z: i32) -> HashMap<(i32, i32), i32> {
        let n = (radius + 1) as i32;
        let mut map = HashMap::new();
        for dq in -n..=n {
            for dr in (-n).max(-dq - n)..=n.min(-dq + n) {
                map.insert((cq + dq, cr + dr), z);
            }
        }
        map
    }

    // r=0 parity
    #[test]
    fn r0_matches_compute_tile_geometry() {
        let mut elevations = HashMap::new();
        elevations.insert((0, 0), 3);
        elevations.insert((-1, 0), 3);
        elevations.insert((-1, 1), 4);
        elevations.insert((0, 1), 2);
        elevations.insert((1, 0), 3);
        elevations.insert((1, -1), 3);
        elevations.insert((0, -1), 5);

        let tiles = vec![qrz::Qrz { q: 0, r: 0, z: 3 }];
        let tile_geom = crate::geometry::compute_tile_geometry(
            &tiles, &elevations, 1.0, 0.8, Vec3::ZERO,
        );

        let tile_z = make_tile_z(&elevations);
        let hexball = compute_hexball_geometry(0, 0, 3, 0, 1.0, 0.8, Vec3::ZERO, &tile_z);

        assert_eq!(tile_geom.positions.len(), hexball.positions.len(), "vertex count");
        for (i, (a, b)) in tile_geom.positions.iter().zip(hexball.positions.iter()).enumerate() {
            assert!((a[0]-b[0]).abs() < 1e-5 && (a[1]-b[1]).abs() < 1e-5 && (a[2]-b[2]).abs() < 1e-5,
                "pos mismatch at {i}: {a:?} vs {b:?}");
        }
        assert_eq!(tile_geom.indices, hexball.indices, "indices differ");
    }

    // Triangle counts
    #[test]
    fn r0_triangle_count() {
        let elevations = flat_tiles(0, 0, 0, 0);
        let tile_z = make_tile_z(&elevations);
        let hb = compute_hexball_geometry(0, 0, 0, 0, 1.0, 0.8, Vec3::ZERO, &tile_z);
        assert_eq!(hb.indices.len() / 3, 6);
    }

    #[test]
    fn r1_triangle_count() {
        let elevations = flat_tiles(0, 0, 1, 0);
        let tile_z = make_tile_z(&elevations);
        let hb = compute_hexball_geometry(0, 0, 0, 1, 1.0, 0.8, Vec3::ZERO, &tile_z);
        assert_eq!(hb.indices.len() / 3, 24, "r=1 should be 24 tri");
    }

    #[test]
    fn r3_triangle_count() {
        let elevations = flat_tiles(0, 0, 3, 0);
        let tile_z = make_tile_z(&elevations);
        let hb = compute_hexball_geometry(0, 0, 0, 3, 1.0, 0.8, Vec3::ZERO, &tile_z);
        assert_eq!(hb.indices.len() / 3, 78, "r=3 should be 78 tri");
    }

    #[test]
    fn r5_triangle_count() {
        let elevations = flat_tiles(0, 0, 5, 0);
        let tile_z = make_tile_z(&elevations);
        let hb = compute_hexball_geometry(0, 0, 0, 5, 1.0, 0.8, Vec3::ZERO, &tile_z);
        assert_eq!(hb.indices.len() / 3, 168, "r=5 should be 168 tri");
    }

    // Flat terrain: all vertices at y=rise
    #[test]
    fn r1_flat_terrain_all_same_y() {
        let elevations = flat_tiles(0, 0, 1, 0);
        let tile_z = make_tile_z(&elevations);
        let hb = compute_hexball_geometry(0, 0, 0, 1, 1.0, 0.8, Vec3::ZERO, &tile_z);
        for (i, pos) in hb.positions.iter().enumerate() {
            assert!((pos[1] - 0.8).abs() < 1e-5, "vertex {i} y={} expected 0.8", pos[1]);
        }
    }

    #[test]
    fn r3_flat_terrain_all_same_y() {
        let elevations = flat_tiles(0, 0, 3, 0);
        let tile_z = make_tile_z(&elevations);
        let hb = compute_hexball_geometry(0, 0, 0, 3, 1.0, 0.8, Vec3::ZERO, &tile_z);
        for (i, pos) in hb.positions.iter().enumerate() {
            assert!((pos[1] - 0.8).abs() < 1e-5, "vertex {i} y={} expected 0.8", pos[1]);
        }
    }

    // CCW winding
    #[test]
    fn r1_ccw_winding() {
        let elevations = flat_tiles(0, 0, 1, 0);
        let tile_z = make_tile_z(&elevations);
        let hb = compute_hexball_geometry(0, 0, 0, 1, 1.0, 0.8, Vec3::ZERO, &tile_z);
        for tri in hb.indices.chunks(3) {
            let a = Vec3::from(hb.positions[tri[0] as usize]);
            let b = Vec3::from(hb.positions[tri[1] as usize]);
            let c = Vec3::from(hb.positions[tri[2] as usize]);
            let n = (b - a).cross(c - a);
            assert!(n.y >= -1e-6, "downward normal: {n:?}");
        }
    }

    #[test]
    fn r3_ccw_winding() {
        let elevations = flat_tiles(0, 0, 3, 0);
        let tile_z = make_tile_z(&elevations);
        let hb = compute_hexball_geometry(0, 0, 0, 3, 1.0, 0.8, Vec3::ZERO, &tile_z);
        for tri in hb.indices.chunks(3) {
            let a = Vec3::from(hb.positions[tri[0] as usize]);
            let b = Vec3::from(hb.positions[tri[1] as usize]);
            let c = Vec3::from(hb.positions[tri[2] as usize]);
            let n = (b - a).cross(c - a);
            assert!(n.y >= -1e-6, "downward normal: {n:?}");
        }
    }

    // Determinism
    #[test]
    fn deterministic() {
        let elevations = flat_tiles(0, 0, 3, 0);
        let tile_z = make_tile_z(&elevations);
        let a = compute_hexball_geometry(0, 0, 0, 3, 1.0, 0.8, Vec3::ZERO, &tile_z);
        let b = compute_hexball_geometry(0, 0, 0, 3, 1.0, 0.8, Vec3::ZERO, &tile_z);
        assert_eq!(a.positions, b.positions);
        assert_eq!(a.normals, b.normals);
        assert_eq!(a.indices, b.indices);
    }

    // Perimeter stitching
    #[test]
    fn r1_perimeter_stitching() {
        let elevations = flat_tiles(0, 0, 4, 0);
        let tile_z = make_tile_z(&elevations);
        // HexLattice(1) cell centers: (0,0) and (2,1)
        let a = compute_hexball_geometry(0, 0, 0, 1, 1.0, 0.8, Vec3::ZERO, &tile_z);
        let b = compute_hexball_geometry(2, 1, 0, 1, 1.0, 0.8, Vec3::ZERO, &tile_z);
        assert_shared_vertices(&a, &b, 2);
    }

    #[test]
    fn r3_perimeter_stitching() {
        let elevations = flat_tiles(0, 0, 10, 0);
        let tile_z = make_tile_z(&elevations);
        // HexLattice(3) v1=(4,3): adjacent centers (0,0) and (4,3)
        let a = compute_hexball_geometry(0, 0, 0, 3, 1.0, 0.8, Vec3::ZERO, &tile_z);
        let b = compute_hexball_geometry(4, 3, 0, 3, 1.0, 0.8, Vec3::ZERO, &tile_z);
        assert_shared_vertices(&a, &b, 2);
    }

    fn assert_shared_vertices(a: &HexballGeometry, b: &HexballGeometry, min_shared: usize) {
        let eps = 1e-5;
        let mut shared = 0;
        for pa in &a.positions {
            for pb in &b.positions {
                if (pa[0]-pb[0]).abs() < eps && (pa[1]-pb[1]).abs() < eps && (pa[2]-pb[2]).abs() < eps {
                    shared += 1;
                    break;
                }
            }
        }
        assert!(shared >= min_shared,
            "expected at least {min_shared} shared vertices, found {shared}");
    }

    // Even radius panics
    #[test]
    #[should_panic(expected = "hexball radius must be 0 or odd")]
    fn even_radius_panics() {
        let elevations = HashMap::new();
        let tile_z = make_tile_z(&elevations);
        compute_hexball_geometry(0, 0, 0, 2, 1.0, 0.8, Vec3::ZERO, &tile_z);
    }
}
