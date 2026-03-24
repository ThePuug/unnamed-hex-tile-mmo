//! Hexball geometry — one function for any hex radius.
//!
//! Two-phase architecture:
//! 1. `compute_hexball_surface()` — computes all final vertex positions with
//!    overrides applied (BV Y, snapped_z). Single source of truth.
//! 2. `emit_hexball_mesh()` — pushes surface data into positions/normals/indices
//!    arrays. No computation, no overrides, just emit.
//!
//! The terrain report reads from the same `HexballSurface` struct, so reported
//! values always match rendered values.

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

// ── Surface data (phase 1 output) ────────────────────────────────────────────

/// Complete surface geometry for a single tile (r=0 or full residual).
pub struct TileSurface {
    pub verts: [Vec3; 7],
    pub q: i32,
    pub r: i32,
    pub z: i32,
}

/// A partial fan's final vertex positions.
pub struct PartialFanSurface {
    pub center: Vec3,
    /// 4 outer vertices [ov0, ov1, ov2, ov3] — inner vertex Y overrides applied.
    pub outer: [Vec3; 4],
    pub surviving_triangles: [u8; 3],
    pub q: i32,
    pub r: i32,
    pub z: i32,
}

/// A skirt quad connecting a high edge to a low edge.
pub struct SkirtQuad {
    pub top: [Vec3; 2],
    pub bottom: [Vec3; 2],
    pub from_q: i32,
    pub from_r: i32,
    pub to_q: i32,
    pub to_r: i32,
}

/// All computed surface data for a hexball, before mesh emission.
/// Every Vec3 is the final rendered position — no further overrides.
pub struct HexballSurface {
    pub hex_center: Option<Vec3>,
    pub hex_boundary: Option<[Vec3; 6]>,
    pub partial_fans: Vec<PartialFanSurface>,
    pub full_tiles: Vec<TileSurface>,
    pub skirts: Vec<SkirtQuad>,
}

/// Raw mesh geometry output.
pub struct HexballGeometry {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

// ── Public types for chunk decimation ────────────────────────────────────────

pub struct HexballDecimation {
    pub center_q: i32,
    pub center_r: i32,
    pub center_z: i32,
    pub radius: u32,
}

pub struct ChunkDecimation {
    pub hexballs: Vec<HexballDecimation>,
    pub survivors: Vec<(i32, i32, i32)>,
}

// ── Public API ───────────────────────────────────────────────────────────────

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
pub fn compute_hexball_geometry(
    center_q: i32, center_r: i32, center_z: i32, radius: u32,
    hex_radius: f32, rise: f32, chunk_origin: Vec3,
    tile_z: &impl Fn(i32, i32) -> Option<i32>,
) -> HexballGeometry {
    let surface = compute_hexball_surface(center_q, center_r, center_z, radius, hex_radius, rise, tile_z);
    emit_hexball_mesh(&surface, chunk_origin)
}

/// Phase 1: Compute all final vertex positions for a hexball.
///
/// Every Vec3 in the returned struct is the final rendered position.
/// BV Y overrides, snapped_z fan centers — all applied here.
/// The terrain report can read this struct directly.
pub fn compute_hexball_surface(
    center_q: i32, center_r: i32, center_z: i32, radius: u32,
    hex_radius: f32, rise: f32,
    tile_z: &impl Fn(i32, i32) -> Option<i32>,
) -> HexballSurface {
    assert!(radius == 0 || radius % 2 == 1, "hexball radius must be 0 or odd, got {radius}");

    if radius == 0 {
        let verts = tile_vertices(center_q, center_r, center_z, hex_radius, rise, tile_z);
        let skirts = compute_tile_skirts(center_q, center_r, center_z, &verts, hex_radius, rise, tile_z);
        return HexballSurface {
            hex_center: None,
            hex_boundary: None,
            partial_fans: Vec::new(),
            full_tiles: vec![TileSurface { verts, q: center_q, r: center_r, z: center_z }],
            skirts,
        };
    }

    let hb = common::hex_decimate::decimate_hexball(
        center_q, center_r, radius, u32::MAX, tile_z,
    ).expect("all tiles within hexball must exist for geometry generation");

    // Inscribed hex center
    let center_y = center_z as f32 * rise + rise;
    let (ccx, ccz) = flat_top_tile_center(center_q, center_r, hex_radius);
    let hex_center = Vec3::new(ccx, center_y, ccz);

    // Boundary vertices
    let hex_boundary: [Vec3; 6] = std::array::from_fn(|i| {
        let (tq, tr, vi) = hb.boundary_tiles[i];
        let tz = tile_z(tq, tr).unwrap_or(center_z);
        tile_vertex_pos(tq, tr, tz, vi as usize, hex_radius, rise, tile_z)
    });

    // Partial fans with BV Y overrides
    let partial_fans: Vec<PartialFanSurface> = hb.partial_residuals.iter().map(|pr| {
        let tv = tile_vertices(pr.q, pr.r, pr.original_z, hex_radius, rise, tile_z);
        let st = pr.surviving_triangles;
        let edge = st[1] as usize;
        let edge_next = (edge + 1) % 6;
        let ov_indices = [st[0] as usize, st[1] as usize, st[2] as usize, (st[2] as usize + 1) % 6];

        let mut outer = [tv[ov_indices[0]], tv[ov_indices[1]], tv[ov_indices[2]], tv[ov_indices[3]]];
        outer[0].y = hb.boundary_z[edge] as f32 * rise + rise;
        outer[3].y = hb.boundary_z[edge_next] as f32 * rise + rise;

        let fan_center_y = pr.snapped_z as f32 * rise + rise;
        let center = Vec3::new(tv[6].x, fan_center_y, tv[6].z);

        PartialFanSurface {
            center, outer, surviving_triangles: st,
            q: pr.q, r: pr.r, z: pr.original_z,
        }
    }).collect();

    // Full residual tiles
    let full_tiles: Vec<TileSurface> = hb.full_residuals.iter().map(|fr| {
        let verts = tile_vertices(fr.q, fr.r, fr.z, hex_radius, rise, tile_z);
        TileSurface { verts, q: fr.q, r: fr.r, z: fr.z }
    }).collect();

    // Skirts — computed from final surface positions
    let mut skirts = Vec::new();

    // Fan skirts: 3 outward edges per fan, using final ov_pos values
    for fan in &partial_fans {
        let st = &fan.surviving_triangles;
        for (k, &tri_idx) in st.iter().enumerate() {
            let vi_a = tri_idx as usize;
            let dir_idx = (4 + 6 - vi_a) % 6;
            let dir = qrz::DIRECTIONS[dir_idx];
            let nq = fan.q + dir.q;
            let nr = fan.r + dir.r;
            let nz = match tile_z(nq, nr) { Some(z) => z, None => continue };
            if nz >= fan.z { continue; }

            let n_verts = tile_vertices(nq, nr, nz, hex_radius, rise, tile_z);
            let (_, _, nv1_idx, nv2_idx) = SKIRT_VERTEX_MAP[dir_idx];
            skirts.push(SkirtQuad {
                top: [fan.outer[k], fan.outer[k + 1]],
                bottom: [n_verts[nv1_idx], n_verts[nv2_idx]],
                from_q: fan.q, from_r: fan.r, to_q: nq, to_r: nr,
            });
        }
    }

    // Full residual skirts
    for tile in &full_tiles {
        skirts.extend(compute_tile_skirts(tile.q, tile.r, tile.z, &tile.verts, hex_radius, rise, tile_z));
    }

    HexballSurface {
        hex_center: Some(hex_center),
        hex_boundary: Some(hex_boundary),
        partial_fans,
        full_tiles,
        skirts,
    }
}

// ── Phase 2: Emit mesh from surface data ─────────────────────────────────────

fn emit_hexball_mesh(surface: &HexballSurface, chunk_origin: Vec3) -> HexballGeometry {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut indices = Vec::new();

    // Inscribed hex
    if let (Some(center), Some(bv)) = (&surface.hex_center, &surface.hex_boundary) {
        let inner_hex: [Vec3; 7] = {
            let mut v = [Vec3::ZERO; 7];
            for i in 0..6 { v[i] = bv[i]; }
            v[6] = *center;
            v
        };
        positions.push((*center - chunk_origin).into());
        normals.push(hex_vertex_normal(&inner_hex, 6).into());
        for i in 0..6 {
            positions.push((bv[i] - chunk_origin).into());
            normals.push(hex_vertex_normal(&inner_hex, i).into());
        }
        for i in 0..6u32 {
            indices.extend([0, 1 + ((i + 1) % 6), 1 + i]);
        }
    }

    // Partial fans
    for fan in &surface.partial_fans {
        let fan_base = positions.len() as u32;
        positions.push((fan.center - chunk_origin).into());
        for p in &fan.outer { positions.push((*p - chunk_origin).into()); }

        indices.extend([fan_base, fan_base + 2, fan_base + 1]);
        indices.extend([fan_base, fan_base + 3, fan_base + 2]);
        indices.extend([fan_base, fan_base + 4, fan_base + 3]);

        let fn0 = triangle_normal(fan.center, fan.outer[1], fan.outer[0]);
        let fn1 = triangle_normal(fan.center, fan.outer[2], fan.outer[1]);
        let fn2 = triangle_normal(fan.center, fan.outer[3], fan.outer[2]);
        normals.push(avg_normal(&[fn0, fn1, fn2]).into());
        normals.push(fn0.into());
        normals.push(avg_normal(&[fn0, fn1]).into());
        normals.push(avg_normal(&[fn1, fn2]).into());
        normals.push(fn2.into());
    }

    // Full residual tiles
    for tile in &surface.full_tiles {
        let base = positions.len() as u32;
        positions.push((tile.verts[6] - chunk_origin).into());
        normals.push(hex_vertex_normal(&tile.verts, 6).into());
        for i in 0..6 {
            positions.push((tile.verts[i] - chunk_origin).into());
            normals.push(hex_vertex_normal(&tile.verts, i).into());
        }
        for i in 0..6u32 {
            indices.extend([base, base + 1 + ((i + 1) % 6), base + 1 + i]);
        }
    }

    // Skirts
    for skirt in &surface.skirts {
        let edge_dir = (skirt.top[1] - skirt.top[0]).normalize();
        let outward = edge_dir.cross(Vec3::new(0.0, -1.0, 0.0)).normalize();
        let base = positions.len() as u32;
        let v0: [f32; 3] = (skirt.top[0] - chunk_origin).into();
        let v1: [f32; 3] = (skirt.top[1] - chunk_origin).into();
        let v2: [f32; 3] = (skirt.bottom[1] - chunk_origin).into();
        let v3: [f32; 3] = (skirt.bottom[0] - chunk_origin).into();
        positions.extend([v0, v1, v2, v3]);
        let n: [f32; 3] = outward.into();
        normals.extend([n; 4]);
        indices.extend([base, base + 1, base + 2]);
        indices.extend([base, base + 2, base + 3]);
    }

    HexballGeometry { positions, normals, indices }
}

// ── Vertex helpers ───────────────────────────────────────────────────────────

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
    Vec3::new(cx + ox, z as f32 * rise + rise + adj[vertex_idx], cz + oz)
}

// ── Shared helpers ───────────────────────────────────────────────────────────

fn compute_tile_skirts(
    q: i32, r: i32, z: i32, verts: &[Vec3; 7],
    hex_radius: f32, rise: f32,
    tile_z: &impl Fn(i32, i32) -> Option<i32>,
) -> Vec<SkirtQuad> {
    let mut skirts = Vec::new();
    for (dir_idx, direction) in qrz::DIRECTIONS.iter().enumerate() {
        let nq = q + direction.q;
        let nr = r + direction.r;
        let nz = match tile_z(nq, nr) { Some(z) => z, None => continue };
        if nz >= z { continue; }
        let n_verts = tile_vertices(nq, nr, nz, hex_radius, rise, tile_z);
        let (cv1, cv2, nv1, nv2) = SKIRT_VERTEX_MAP[dir_idx];
        skirts.push(SkirtQuad {
            top: [verts[cv1], verts[cv2]],
            bottom: [n_verts[nv1], n_verts[nv2]],
            from_q: q, from_r: r, to_q: nq, to_r: nr,
        });
    }
    skirts
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

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_tile_z(elevations: &HashMap<(i32, i32), i32>) -> impl Fn(i32, i32) -> Option<i32> + '_ {
        move |q, r| elevations.get(&(q, r)).copied()
    }

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

    #[test]
    fn r1_perimeter_stitching() {
        let elevations = flat_tiles(0, 0, 4, 0);
        let tile_z = make_tile_z(&elevations);
        let a = compute_hexball_geometry(0, 0, 0, 1, 1.0, 0.8, Vec3::ZERO, &tile_z);
        let b = compute_hexball_geometry(2, 1, 0, 1, 1.0, 0.8, Vec3::ZERO, &tile_z);
        assert_shared_vertices(&a, &b, 2);
    }

    #[test]
    fn r3_perimeter_stitching() {
        let elevations = flat_tiles(0, 0, 10, 0);
        let tile_z = make_tile_z(&elevations);
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

    #[test]
    #[should_panic(expected = "hexball radius must be 0 or odd")]
    fn even_radius_panics() {
        let elevations = HashMap::new();
        let tile_z = make_tile_z(&elevations);
        compute_hexball_geometry(0, 0, 0, 2, 1.0, 0.8, Vec3::ZERO, &tile_z);
    }

    #[test]
    fn surface_reports_overridden_y() {
        let mut elevations = HashMap::new();
        for dq in -3..=3 {
            for dr in (-3).max(-dq - 3)..=(3).min(-dq + 3) {
                elevations.insert((dq, dr), dq + 3);
            }
        }
        let tile_z = make_tile_z(&elevations);
        let surface = compute_hexball_surface(0, 0, 3, 1, 1.0, 0.8, &tile_z);
        let bv = surface.hex_boundary.unwrap();
        for fan in &surface.partial_fans {
            let edge = fan.surviving_triangles[1] as usize;
            let edge_next = (edge + 1) % 6;
            assert!((fan.outer[0].y - bv[edge].y).abs() < 1e-5,
                "fan ov[0] y={:.4} != bv[{edge}] y={:.4}", fan.outer[0].y, bv[edge].y);
            assert!((fan.outer[3].y - bv[edge_next].y).abs() < 1e-5,
                "fan ov[3] y={:.4} != bv[{edge_next}] y={:.4}", fan.outer[3].y, bv[edge_next].y);
        }
    }
}
