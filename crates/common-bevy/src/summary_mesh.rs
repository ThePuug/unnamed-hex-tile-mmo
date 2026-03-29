//! Mesh region assembly for summary hexes.
//!
//! Groups summaries into mesh regions of ~271 summaries each (radius-9 in
//! summary-lattice space). Handles mesh assembly, the generic intra-region
//! skirt pass, and perimeter edge collection for cross-region exchange.

use std::collections::{HashMap, HashSet};

use bevy::math::Vec3;

use crate::{
    chunk::{self, ChunkId},
    geometry::flat_top_tile_center,
    summary::{
        SummaryLattice, SummarySurface,
        mesh_region_lattice, select_center_z, summary_lattice,
    },
};

/// Key identifying a mesh region within a single distance band.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MeshRegionKey {
    /// Mesh region lattice coordinates (in summary-lattice space).
    pub mn: i32,
    pub mm: i32,
}

/// A perimeter edge at the boundary of a mesh region.
///
/// Represents one side of a shared summary edge whose other side
/// is in an adjacent mesh region. Collected during the intra-region
/// skirt pass and stored for cross-region exchange.
#[derive(Clone, Debug)]
pub struct PerimeterEdge {
    /// Sorted canonical vertex ID pair defining this edge.
    pub vertex_ids: [(i32, i32); 2],
    /// World-space positions of the two vertices on this side.
    pub positions: [Vec3; 2],
}

/// Result of building a mesh region.
pub struct SummaryMeshResult {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub tri_count: u32,
    /// World-space origin of this mesh region (for Transform).
    pub mesh_origin: Vec3,
    /// Edges at the mesh region boundary, for cross-region skirt exchange.
    pub perimeter_edges: Vec<PerimeterEdge>,
    /// How many summaries had complete tile data and were built.
    /// A mesh region has 271 summary slots. If `summaries_built < 271`,
    /// some summaries were skipped due to missing tile data and the
    /// region should be rebuilt when more data arrives.
    pub summaries_built: u32,
}

/// Number of summary slots in a mesh region (radius-9 hex ball).
pub const MESH_REGION_SUMMARIES: u32 = 271;

/// Build a summary mesh region for the given radius.
///
/// Collects all summaries in the mesh region, computes center_z for each
/// from the tile elevation data, emits flat hex geometry, runs the
/// intra-region skirt pass, and collects perimeter edges.
///
/// `radius`: summary radius (must be > 0).
/// `region_key`: mesh region lattice coordinates.
/// `elevation_fn`: returns z for tile (q, r), or None if not loaded.
///
/// Returns None if no summaries have data.
pub fn build_summary_mesh_region(
    radius: u32,
    region_key: MeshRegionKey,
    elevation_fn: &dyn Fn(i32, i32) -> Option<i32>,
) -> Option<SummaryMeshResult> {
    debug_assert!(radius > 0, "r=0 should use tile passthrough");

    let summary_lat = summary_lattice(radius);
    let region_lat = mesh_region_lattice();

    // The mesh region center in summary-lattice space
    let region_center = region_lat.cell_center((region_key.mn, region_key.mm));

    // Compute mesh origin from the region center summary's tile center
    let (origin_cq, origin_cr) = summary_lat.cell_center(region_center);
    let (origin_wx, origin_wz) = flat_top_tile_center(origin_cq, origin_cr, 1.0);
    let mesh_origin = Vec3::new(origin_wx, 0.0, origin_wz);

    // Enumerate all summaries in this mesh region and build surfaces
    let mut surfaces: Vec<SummarySurface> = Vec::new();

    for (sn, sm) in region_lat.tiles_in_cell((region_key.mn, region_key.mm)) {
        let (cq, cr) = summary_lat.cell_center((sn, sm));

        // Collect z values for all tiles in this summary's hex ball
        let mut tile_zs: Vec<i32> = Vec::new();
        let mut all_present = true;

        for (tq, tr) in summary_lat.tiles_in_cell((sn, sm)) {
            if let Some(z) = elevation_fn(tq, tr) {
                tile_zs.push(z);
            } else {
                all_present = false;
                break;
            }
        }

        if !all_present || tile_zs.is_empty() {
            continue;
        }

        let center_z = select_center_z(&tile_zs);
        surfaces.push(SummarySurface::flat(sn, sm, radius, cq, cr, center_z));
    }

    if surfaces.is_empty() {
        return None;
    }

    // Emit geometry for all ready summaries
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut tri_count = 0u32;

    for surface in &surfaces {
        tri_count += surface.emit_geometry(&mut positions, &mut normals, &mut indices, mesh_origin);
    }

    // Intra-region skirt pass + perimeter edge collection
    let (skirt_tris, perimeter_edges) = emit_skirts(
        &surfaces,
        &mut positions,
        &mut normals,
        &mut indices,
        mesh_origin,
    );
    tri_count += skirt_tris;

    Some(SummaryMeshResult {
        positions,
        normals,
        indices,
        tri_count,
        mesh_origin,
        perimeter_edges,
        summaries_built: surfaces.len() as u32,
    })
}

/// Emit intra-region skirt quads and collect perimeter edges.
///
/// Edges with 2 sides (both summaries in this region) get skirt quads.
/// Edges with 1 side (boundary of region) become perimeter edges for
/// cross-region exchange.
fn emit_skirts(
    surfaces: &[SummarySurface],
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    indices: &mut Vec<u32>,
    mesh_origin: Vec3,
) -> (u32, Vec<PerimeterEdge>) {
    struct EdgeSide {
        v1_pos: Vec3,
        v2_pos: Vec3,
    }

    type EdgeKey = ((i32, i32), (i32, i32));
    let mut edge_map: HashMap<EdgeKey, Vec<EdgeSide>> = HashMap::new();

    for surface in surfaces {
        for i in 0..6 {
            let j = (i + 1) % 6;
            let id_a = surface.corner_ids[i];
            let id_b = surface.corner_ids[j];
            let (key_lo, key_hi, pos_lo, pos_hi) = if id_a <= id_b {
                (id_a, id_b, surface.corners[i], surface.corners[j])
            } else {
                (id_b, id_a, surface.corners[j], surface.corners[i])
            };
            edge_map
                .entry((key_lo, key_hi))
                .or_default()
                .push(EdgeSide {
                    v1_pos: pos_lo,
                    v2_pos: pos_hi,
                });
        }
    }

    let mut tris = 0u32;
    let mut perimeter_edges = Vec::new();

    for (&(id_lo, id_hi), sides) in &edge_map {
        if sides.len() == 1 {
            // Perimeter edge: only one side in this region
            perimeter_edges.push(PerimeterEdge {
                vertex_ids: [id_lo, id_hi],
                positions: [sides[0].v1_pos, sides[0].v2_pos],
            });
            continue;
        }
        if sides.len() != 2 {
            continue;
        }

        tris += emit_skirt_quad(
            &sides[0].v1_pos, &sides[0].v2_pos,
            &sides[1].v1_pos, &sides[1].v2_pos,
            positions, normals, indices, mesh_origin,
        );
    }

    (tris, perimeter_edges)
}

/// Emit a single skirt quad between two sides of a shared edge.
/// Returns 0 if both sides are at the same Y, otherwise 2 (triangles).
fn emit_skirt_quad(
    a_v1: &Vec3, a_v2: &Vec3,
    b_v1: &Vec3, b_v2: &Vec3,
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    indices: &mut Vec<u32>,
    mesh_origin: Vec3,
) -> u32 {
    let dy1 = (a_v1.y - b_v1.y).abs();
    let dy2 = (a_v2.y - b_v2.y).abs();
    if dy1 < 1e-6 && dy2 < 1e-6 {
        return 0;
    }

    let (top1, bot1) = if a_v1.y >= b_v1.y {
        (*a_v1, *b_v1)
    } else {
        (*b_v1, *a_v1)
    };
    let (top2, bot2) = if a_v2.y >= b_v2.y {
        (*a_v2, *b_v2)
    } else {
        (*b_v2, *a_v2)
    };

    let edge_dir = (top2 - top1).normalize_or_zero();
    let outward = edge_dir.cross(Vec3::NEG_Y).normalize_or_zero();
    let n: [f32; 3] = if outward.length_squared() > 0.5 {
        outward.into()
    } else {
        [0.0, 0.0, 1.0]
    };

    let base = positions.len() as u32;
    let t1 = top1 - mesh_origin;
    let t2 = top2 - mesh_origin;
    let b1 = bot1 - mesh_origin;
    let b2 = bot2 - mesh_origin;
    positions.extend([
        [t1.x, t1.y, t1.z],
        [t2.x, t2.y, t2.z],
        [b2.x, b2.y, b2.z],
        [b1.x, b1.y, b1.z],
    ]);
    normals.extend([n; 4]);

    indices.extend([base, base + 1, base + 2]);
    indices.extend([base, base + 2, base + 3]);
    2
}

/// Compute cross-region skirt geometry between two regions' perimeter edges.
///
/// Matches edges by canonical vertex ID pairs. For each match where Y
/// values differ, emits a skirt quad. Returns the raw quad geometry
/// (world-space positions and normals, not yet mesh-origin-relative).
pub fn compute_cross_region_skirts(
    my_edges: &[PerimeterEdge],
    neighbor_edges: &[PerimeterEdge],
) -> Vec<CrossRegionSkirtQuad> {
    let neighbor_map: HashMap<[(i32, i32); 2], &PerimeterEdge> =
        neighbor_edges.iter().map(|e| (e.vertex_ids, e)).collect();

    let mut quads = Vec::new();

    for my_edge in my_edges {
        if let Some(their_edge) = neighbor_map.get(&my_edge.vertex_ids) {
            let dy0 = (my_edge.positions[0].y - their_edge.positions[0].y).abs();
            let dy1 = (my_edge.positions[1].y - their_edge.positions[1].y).abs();
            if dy0 < 1e-6 && dy1 < 1e-6 {
                continue;
            }

            let (top0, bot0) = if my_edge.positions[0].y >= their_edge.positions[0].y {
                (my_edge.positions[0], their_edge.positions[0])
            } else {
                (their_edge.positions[0], my_edge.positions[0])
            };
            let (top1, bot1) = if my_edge.positions[1].y >= their_edge.positions[1].y {
                (my_edge.positions[1], their_edge.positions[1])
            } else {
                (their_edge.positions[1], my_edge.positions[1])
            };

            let edge_dir = (top1 - top0).normalize_or_zero();
            let outward = edge_dir.cross(Vec3::NEG_Y).normalize_or_zero();
            let normal = if outward.length_squared() > 0.5 {
                outward
            } else {
                Vec3::Z
            };

            quads.push(CrossRegionSkirtQuad {
                positions: [top0, top1, bot1, bot0],
                normal,
            });
        }
    }

    quads
}

/// A skirt quad computed from cross-region edge matching.
/// Positions are in world space.
pub struct CrossRegionSkirtQuad {
    /// 4 vertices: top0, top1, bot1, bot0 (world-space).
    pub positions: [Vec3; 4],
    /// Outward-facing normal.
    pub normal: Vec3,
}

/// The 6 mesh region neighbors in lattice space.
pub fn mesh_region_neighbors(key: MeshRegionKey) -> [MeshRegionKey; 6] {
    const OFFSETS: [(i32, i32); 6] = [
        (1, 0), (-1, 0), (0, 1), (0, -1), (1, -1), (-1, 1),
    ];
    OFFSETS.map(|(dn, dm)| MeshRegionKey {
        mn: key.mn + dn,
        mm: key.mm + dm,
    })
}

/// Determine which game chunks contribute tiles to a summary's hex ball.
pub fn contributing_chunks(
    summary_lat: &SummaryLattice,
    sn: i32,
    sm: i32,
) -> HashSet<ChunkId> {
    let mut chunks = HashSet::new();
    for (tq, tr) in summary_lat.tiles_in_cell((sn, sm)) {
        let qrz = qrz::Qrz { q: tq, r: tr, z: 0 };
        chunks.insert(chunk::loc_to_chunk(qrz));
    }
    chunks
}

/// Enumerate all mesh region keys that overlap a set of loaded chunks,
/// for a given summary radius.
pub fn visible_mesh_regions(
    radius: u32,
    loaded_chunks: &HashSet<ChunkId>,
) -> HashSet<MeshRegionKey> {
    let summary_lat = summary_lattice(radius);
    let region_lat = mesh_region_lattice();
    let mut regions = HashSet::new();

    for &chunk_id in loaded_chunks {
        let center = chunk_id.center();
        let summary_cell = summary_lat.cell_id(center.q, center.r);
        let region = region_lat.cell_id(summary_cell.0, summary_cell.1);
        regions.insert(MeshRegionKey {
            mn: region.0,
            mm: region.1,
        });

        for &(dn, dm) in &[(1, 0), (-1, 0), (0, 1), (0, -1), (1, -1), (-1, 1)] {
            let nb = (summary_cell.0 + dn, summary_cell.1 + dm);
            let r = region_lat.cell_id(nb.0, nb.1);
            regions.insert(MeshRegionKey { mn: r.0, mm: r.1 });
        }
    }

    regions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mesh_region_contains_271_summaries() {
        let region_lat = mesh_region_lattice();
        let count = region_lat.tiles_in_cell((0, 0)).count();
        assert_eq!(count, 271);
    }

    #[test]
    fn build_flat_region_r1() {
        let result = build_summary_mesh_region(
            1,
            MeshRegionKey { mn: 0, mm: 0 },
            &|_q, _r| Some(5),
        );
        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.tri_count, 271 * 6);
        // Flat terrain → no skirts, but perimeter edges should exist
        // (summaries at region boundary have outward-facing edges)
        assert!(!result.perimeter_edges.is_empty());
    }

    #[test]
    fn build_returns_none_when_no_data() {
        let result = build_summary_mesh_region(
            1,
            MeshRegionKey { mn: 0, mm: 0 },
            &|_q, _r| None,
        );
        assert!(result.is_none());
    }

    #[test]
    fn build_with_elevation_variation_has_skirts() {
        let summary_lat = summary_lattice(1);
        let result = build_summary_mesh_region(
            1,
            MeshRegionKey { mn: 0, mm: 0 },
            &|q, r| {
                let cell = summary_lat.cell_id(q, r);
                Some(if (cell.0 + cell.1) % 2 == 0 { 0 } else { 10 })
            },
        );
        assert!(result.is_some());
        let result = result.unwrap();
        assert!(
            result.tri_count > 271 * 6,
            "expected skirts: {} tris",
            result.tri_count
        );
    }

    #[test]
    fn perimeter_edges_have_sorted_ids() {
        let result = build_summary_mesh_region(
            1,
            MeshRegionKey { mn: 0, mm: 0 },
            &|_q, _r| Some(5),
        )
        .unwrap();
        for edge in &result.perimeter_edges {
            assert!(
                edge.vertex_ids[0] <= edge.vertex_ids[1],
                "perimeter edge IDs not sorted: {:?}",
                edge.vertex_ids
            );
        }
    }

    #[test]
    fn cross_region_skirts_between_adjacent_regions() {
        // Build two adjacent regions with different z values
        let key_a = MeshRegionKey { mn: 0, mm: 0 };
        let key_b = MeshRegionKey { mn: 1, mm: 0 };
        let result_a = build_summary_mesh_region(
            1,
            key_a,
            &|_q, _r| Some(0),
        )
        .unwrap();
        let result_b = build_summary_mesh_region(
            1,
            key_b,
            &|_q, _r| Some(10),
        )
        .unwrap();

        let quads = compute_cross_region_skirts(
            &result_a.perimeter_edges,
            &result_b.perimeter_edges,
        );
        // Adjacent regions with different z should produce cross-region skirts
        assert!(
            !quads.is_empty(),
            "expected cross-region skirts between regions at z=0 and z=10"
        );
    }

    #[test]
    fn cross_region_skirts_zero_when_same_z() {
        let key_a = MeshRegionKey { mn: 0, mm: 0 };
        let key_b = MeshRegionKey { mn: 1, mm: 0 };
        let result_a = build_summary_mesh_region(
            1,
            key_a,
            &|_q, _r| Some(5),
        )
        .unwrap();
        let result_b = build_summary_mesh_region(
            1,
            key_b,
            &|_q, _r| Some(5),
        )
        .unwrap();

        let quads = compute_cross_region_skirts(
            &result_a.perimeter_edges,
            &result_b.perimeter_edges,
        );
        assert!(
            quads.is_empty(),
            "no skirts expected between regions at same z, got {}",
            quads.len()
        );
    }

    #[test]
    fn mesh_region_neighbors_count() {
        let neighbors = mesh_region_neighbors(MeshRegionKey { mn: 0, mm: 0 });
        assert_eq!(neighbors.len(), 6);
        // All should be distinct
        let set: HashSet<_> = neighbors.iter().collect();
        assert_eq!(set.len(), 6);
    }

    #[test]
    fn contributing_chunks_r1_center_origin() {
        let lat = summary_lattice(1);
        let chunks = contributing_chunks(&lat, 0, 0);
        assert!(chunks.contains(&ChunkId(0, 0)));
    }

    #[test]
    fn contributing_chunks_r9_single_chunk() {
        let lat = summary_lattice(9);
        let chunks = contributing_chunks(&lat, 0, 0);
        assert_eq!(chunks.len(), 1, "r=9 summary should span one chunk");
        assert!(chunks.contains(&ChunkId(0, 0)));
    }
}
