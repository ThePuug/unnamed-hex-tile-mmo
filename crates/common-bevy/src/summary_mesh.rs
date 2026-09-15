//! Mesh region assembly on the terrain surface — one builder for every LoD
//! level.

//! A region is a radius-9 hex ball of lattice cells (271) at the level's
//! scale, one mesh each. At r = 0 the lattice is the tile grid and a region
//! is a chunk; above it the cells are summaries. Every cell is a fan from its
//! centre to six corners shared with its neighbours (`crate::surface`), so
//! adjacent cells — and adjacent regions, which compute the same corners
//! from the same heights — meet without seams. The only extra geometry is a
//! curtain under each edge that faces a cell this mesh did not build: it
//! closes the horizon and unbuilt ground, and once the neighbour exists it
//! hangs under a closed surface where nothing can see it.

use std::collections::{HashMap, HashSet};

use bevy::math::{Vec2, Vec3};

use crate::{
    chunk::{self, ChunkId},
    geometry::flat_top_tile_center,
    summary::{
        Band, canonical_vertex_id, level_depth_bias, mesh_region_lattice, summary_lattice,
    },
    surface::{
        CORNER_NEIGHBOURS, centre_normal, corner_normal, corner_offsets, corner_z, height_y,
    },
};

/// Key identifying a mesh region within a specific distance band.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MeshRegionKey {
    /// Summary radius for this band.
    pub r: u32,
    /// Mesh region lattice coordinates (in summary-lattice space).
    pub mn: i32,
    pub mm: i32,
}

/// Result of building a mesh region.
pub struct SummaryMeshResult {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub tri_count: u32,
    /// World-space origin of this mesh region (for Transform).
    pub mesh_origin: Vec3,
}

/// Cells in a mesh region (radius-9 hex ball).
pub const MESH_REGION_CELLS: u32 = 271;

/// Downward curtain depth (WU) under edges facing unbuilt cells. Deep enough
/// to cover the relief between a built region and the ground beside it.
pub const CURTAIN_DEPTH_WU: f32 = 24.0;

/// Build a mesh region at `radius` from a height lookup over that level's
/// lattice: `height(sq, sr)` is the cell's z, or None while its data is
/// absent. At r = 0 the lattice coordinates are tile coordinates.

/// Built only once every cell of the region and of the ring around it has a
/// height — the ring is what the perimeter corners are made of — and then
/// final: heights are durable, so nothing a built region depends on ever
/// changes. Returns None until then. Producers cover one region ring more
/// than consumers build (`visible_lod_regions`), so a needed region's ring
/// always arrives.
pub fn build_summary_mesh_region(
    radius: u32,
    region_key: MeshRegionKey,
    height: &dyn Fn(i32, i32) -> Option<i32>,
) -> Option<SummaryMeshResult> {
    let lattice = summary_lattice(radius);
    let region_lat = mesh_region_lattice();
    let region_id = (region_key.mn, region_key.mm);

    let region_center = region_lat.cell_center(region_id);
    let (origin_cq, origin_cr) = lattice.cell_center(region_center);
    let (origin_wx, origin_wz) = flat_top_tile_center(origin_cq, origin_cr, 1.0);
    let mesh_origin = Vec3::new(origin_wx, 0.0, origin_wz);

    // Heights for the region and its ring — the ring only feeds corners.
    let region_cells: Vec<(i32, i32)> = region_lat.tiles_in_cell(region_id).collect();
    let region_set: HashSet<(i32, i32)> = region_cells.iter().copied().collect();
    let mut heights: HashMap<(i32, i32), i32> = HashMap::new();
    for &(sq, sr) in &region_cells {
        for (dq, dr) in [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1), (1, -1), (-1, 1)] {
            let cell = (sq + dq, sr + dr);
            if heights.contains_key(&cell) {
                continue;
            }
            heights.insert(cell, height(cell.0, cell.1)?);
        }
    }

    let bias = level_depth_bias(radius);
    let outer_radius = lattice.scale as f32;
    let offsets = corner_offsets(outer_radius);
    let centre_of = |cell: (i32, i32)| -> Vec2 {
        let (cq, cr) = lattice.cell_center(cell);
        let (wx, wz) = flat_top_tile_center(cq, cr, 1.0);
        Vec2::new(wx, wz)
    };
    let centre_3d = |cell: (i32, i32)| -> Option<Vec3> {
        let z = *heights.get(&cell)?;
        let c = centre_of(cell);
        Some(Vec3::new(c.x, height_y(z as f32) - bias, c.y))
    };

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut corner_index: HashMap<(i32, i32), u32> = HashMap::new();

    let push = |positions: &mut Vec<[f32; 3]>, normals: &mut Vec<[f32; 3]>, p: Vec3, n: Vec3| -> u32 {
        let v = p - mesh_origin;
        positions.push([v.x, v.y, v.z]);
        normals.push([n.x, n.y, n.z]);
        (positions.len() - 1) as u32
    };

    for &cell in &region_cells {
        let z = heights[&cell];
        let centre = centre_of(cell);
        let centre_y = height_y(z as f32) - bias;

        let mut corner_pos = [Vec3::ZERO; 6];
        let mut corner_idx = [0u32; 6];
        for i in 0..6 {
            let [a, b] = CORNER_NEIGHBOURS[i];
            let na = (cell.0 + a.0, cell.1 + a.1);
            let nb = (cell.0 + b.0, cell.1 + b.1);
            let cz = corner_z([Some(z), heights.get(&na).copied(), heights.get(&nb).copied()])
                .expect("owner cell is present");
            let p = Vec3::new(centre.x + offsets[i].x, height_y(cz) - bias, centre.y + offsets[i].y);
            corner_pos[i] = p;
            let id = canonical_vertex_id(cell.0, cell.1, i);
            corner_idx[i] = *corner_index.entry(id).or_insert_with(|| {
                let n = corner_normal([centre_3d(cell), centre_3d(na), centre_3d(nb)]);
                push(&mut positions, &mut normals, p, n)
            });
        }

        let centre_pos = Vec3::new(centre.x, centre_y, centre.y);
        let ci = push(&mut positions, &mut normals, centre_pos, centre_normal(centre_pos, corner_pos));
        for i in 0..6 {
            let j = (i + 1) % 6;
            indices.extend([ci, corner_idx[j], corner_idx[i]]);
        }

        // Curtains under edges facing the ring: cells this mesh does not build.
        // (i, i+1) faces the first neighbour listed for corner i.
        for i in 0..6 {
            let d = CORNER_NEIGHBOURS[i][0];
            let facing = (cell.0 + d.0, cell.1 + d.1);
            if region_set.contains(&facing) {
                continue;
            }
            let j = (i + 1) % 6;
            let (top0, top1) = (corner_pos[i], corner_pos[j]);
            let (bot0, bot1) = (top0 - Vec3::Y * CURTAIN_DEPTH_WU, top1 - Vec3::Y * CURTAIN_DEPTH_WU);
            let outward = (top1 - top0).normalize_or_zero().cross(Vec3::NEG_Y).normalize_or_zero();
            let n = if outward.length_squared() > 0.5 { outward } else { Vec3::Z };
            let base = positions.len() as u32;
            for p in [top0, top1, bot1, bot0] {
                push(&mut positions, &mut normals, p, n);
            }
            indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }

    Some(SummaryMeshResult {
        tri_count: indices.len() as u32 / 3,
        positions,
        normals,
        indices,
        mesh_origin,
    })
}

/// Enumerate mesh regions within a distance band that overlap loaded chunks.

/// `camera_wx/wz`: camera world position (XZ plane).
/// `inner_wu/outer_wu`: extended band range (natural + overlap).
/// Only includes regions whose world-space centers fall within range
/// AND overlap at least one loaded chunk.
pub fn visible_mesh_regions_in_band(
    r: u32,
    camera_wx: f32,
    camera_wz: f32,
    inner_wu: f32,
    outer_wu: f32,
    loaded_chunks: &HashSet<ChunkId>,
) -> HashSet<MeshRegionKey> {
    let summary_lat = summary_lattice(r);
    let region_lat = mesh_region_lattice();

    // Convert camera world position to tile coordinates using flat-top inverse,
    // then to summary-lattice coordinates, then to mesh-region lattice.
    // Flat-top: x = 1.5*q, z = sqrt(3)/2*q + sqrt(3)*r
    // Inverse: q = x/1.5, r = (z - sqrt(3)/2*q) / sqrt(3)
    let cam_q = camera_wx as f64 / 1.5;
    let cam_r = (camera_wz as f64 - cam_q * 3.0_f64.sqrt() / 2.0) / 3.0_f64.sqrt();
    let cam_sq = (cam_q / summary_lat.scale as f64).round() as i32;
    let cam_sr = (cam_r / summary_lat.scale as f64).round() as i32;
    let cam_region = region_lat.cell_id(cam_sq, cam_sr);

    // Search radius in mesh-region lattice units. Conservative estimate.
    let region_extent = crate::summary::mesh_region_extent_wu(r).max(1.0);
    let search_radius = ((outer_wu / region_extent) as i32 + 2).min(60);

    let mut regions = HashSet::new();
    let sr = search_radius;
    for dn in -sr..=sr {
        let dm_min = (-sr).max(-dn - sr);
        let dm_max = sr.min(-dn + sr);
        for dm in dm_min..=dm_max {
            let mn = cam_region.0 + dn;
            let mm = cam_region.1 + dm;

            // Check world-space distance of this region's center from camera
            let region_center = region_lat.cell_center((mn, mm));
            let (scq, scr) = summary_lat.cell_center(region_center);
            let (rwx, rwz) = flat_top_tile_center(scq, scr, 1.0);
            let dx = rwx - camera_wx;
            let dz = rwz - camera_wz;
            let dist = (dx * dx + dz * dz).sqrt();

            if dist < inner_wu || dist > outer_wu {
                continue;
            }

            // Check overlap with loaded chunks: at least one summary's tiles
            // must be in loaded chunks. Quick check: region center's tile.
            let qrz = qrz::Qrz { q: scq, r: scr, z: 0 };
            let chunk_id = chunk::loc_to_chunk(qrz);
            if !loaded_chunks.contains(&chunk_id) {
                // Try neighbor summary centers
                let mut any_loaded = false;
                for &(dsn, dsm) in &[(1,0),(-1,0),(0,1),(0,-1),(1,-1),(-1,1)] {
                    let nb = (region_center.0 + dsn, region_center.1 + dsm);
                    let (nq, nr) = summary_lat.cell_center(nb);
                    let nqrz = qrz::Qrz { q: nq, r: nr, z: 0 };
                    if loaded_chunks.contains(&chunk::loc_to_chunk(nqrz)) {
                        any_loaded = true;
                        break;
                    }
                }
                if !any_loaded { continue; }
            }

            regions.insert(MeshRegionKey { r, mn, mm });
        }
    }

    regions
}

/// Like `visible_mesh_regions_in_band` but without the loaded-chunk gate.
/// Used for remote summary bands where data comes from server, not local tiles.
pub fn visible_mesh_regions_in_band_ungated(
    r: u32,
    camera_wx: f32,
    camera_wz: f32,
    inner_wu: f32,
    outer_wu: f32,
) -> HashSet<MeshRegionKey> {
    let summary_lat = summary_lattice(r);
    let region_lat = mesh_region_lattice();

    let cam_q = camera_wx as f64 / 1.5;
    let cam_r = (camera_wz as f64 - cam_q * 3.0_f64.sqrt() / 2.0) / 3.0_f64.sqrt();
    let cam_sq = (cam_q / summary_lat.scale as f64).round() as i32;
    let cam_sr = (cam_r / summary_lat.scale as f64).round() as i32;
    let cam_region = region_lat.cell_id(cam_sq, cam_sr);

    let region_extent = crate::summary::mesh_region_extent_wu(r).max(1.0);
    let search_radius = ((outer_wu / region_extent) as i32 + 2).min(60);

    let mut regions = HashSet::new();
    let sr = search_radius;
    for dn in -sr..=sr {
        let dm_min = (-sr).max(-dn - sr);
        let dm_max = sr.min(-dn + sr);
        for dm in dm_min..=dm_max {
            let mn = cam_region.0 + dn;
            let mm = cam_region.1 + dm;

            let region_center = region_lat.cell_center((mn, mm));
            let (scq, scr) = summary_lat.cell_center(region_center);
            let (rwx, rwz) = flat_top_tile_center(scq, scr, 1.0);
            let dx = rwx - camera_wx;
            let dz = rwz - camera_wz;
            let dist = (dx * dx + dz * dz).sqrt();

            if dist < inner_wu || dist > outer_wu {
                continue;
            }

            regions.insert(MeshRegionKey { r, mn, mm });
        }
    }

    regions
}

/// Collect visible mesh regions across all bands for the producers (server,
/// flyover): everything beyond the local-data boundary, plus regions whose
/// footprint straddles it.

/// `local_boundary_wu` is the extent the consumer's Map can serve:
/// FIXED_STREAM_RADIUS_WU in gameplay, the flyover's detail-chunk radius in
/// flyover (much smaller — using the gameplay constant there left an
/// un-rendered ring between the flyover's chunks and the first produced band).

/// Straddling matters: a region centered just inside the boundary has
/// summaries beyond it whose tiles the Map can never resolve — the producer
/// covers those. Values agree with Map-computed ones because every producer
/// uses the same 7-sample rule over the same elevation field.

/// The consumer builds a region only once its ring has heights too, so the
/// producer covers one region ring more than the consumer needs at both
/// ends of every band — without it, the horizon shell and the regions just
/// inside the boundary would wait forever.
pub fn visible_lod_regions(
    bands: &[Band],
    cam_wx: f32,
    cam_wz: f32,
    local_boundary_wu: f32,
) -> HashSet<MeshRegionKey> {
    let mut out = HashSet::new();
    for band in bands {
        let half_extent = 0.5 * crate::summary::mesh_region_extent_wu(band.r);
        let ring = crate::summary::mesh_region_spacing_wu(band.r);
        // Footprint-overlap enumeration over the level's window (matches
        // the consumer): every region whose footprint touches the window
        // is produced, so the strip the cut keeps past the band edge has
        // data. Center-only membership left regions centered just outside
        // an edge to neither band — un-rendered crescents at every level
        // boundary.
        let (win_inner, win_outer) = band.window();
        let outer = win_outer + half_extent + ring;
        // Skip bands whose regions and rings cannot reach past the local
        // boundary — those are fully consumer-owned (Map-computed).
        if outer <= local_boundary_wu { continue; }
        let inner = (win_inner - half_extent).max(local_boundary_wu) - ring;
        out.extend(visible_mesh_regions_in_band_ungated(
            band.r, cam_wx, cam_wz, inner.max(0.0), outer,
        ));
    }
    out
}

/// Enumerate all mesh region keys that overlap a set of loaded chunks,
/// for a given summary radius. Used by forced-radius mode (no distance filter).
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
            r: radius,
            mn: region.0,
            mm: region.1,
        });

        for &(dn, dm) in &[(1, 0), (-1, 0), (0, 1), (0, -1), (1, -1), (-1, 1)] {
            let nb = (summary_cell.0 + dn, summary_cell.1 + dm);
            let rid = region_lat.cell_id(nb.0, nb.1);
            regions.insert(MeshRegionKey { r: radius, mn: rid.0, mm: rid.1 });
        }
    }

    regions
}


#[cfg(test)]
mod tests {
    use super::*;

    const REGION: MeshRegionKey = MeshRegionKey { r: 1, mn: 0, mm: 0 };

    /// Perimeter edges of a radius-R hex ball: 6(2R+1).
    const PERIMETER_EDGES: u32 = 6 * (2 * 9 + 1);

    /// Corner vertices of a radius-R hex ball: 6(R+1)².
    const CORNER_VERTICES: usize = 6 * 10 * 10;

    fn fan_vertices(result: &SummaryMeshResult) -> Vec<(Vec3, Vec3)> {
        result
            .positions
            .iter()
            .zip(&result.normals)
            .map(|(p, n)| (Vec3::from_array(*p) + result.mesh_origin, Vec3::from_array(*n)))
            .filter(|(_, n)| n.y.abs() > 1e-3)
            .collect()
    }

    #[test]
    fn mesh_region_contains_271_summaries() {
        let region_lat = mesh_region_lattice();
        assert_eq!(region_lat.tiles_in_cell((0, 0)).count(), MESH_REGION_CELLS as usize);
    }

    #[test]
    fn build_returns_none_when_no_data() {
        assert!(build_summary_mesh_region(1, REGION, &|_, _| None).is_none());
    }

    #[test]
    fn flat_region_is_fans_plus_perimeter_curtains() {
        let result = build_summary_mesh_region(1, REGION, &|_, _| Some(5)).unwrap();
        assert_eq!(result.tri_count, MESH_REGION_CELLS * 6 + PERIMETER_EDGES * 2);
        let fans = fan_vertices(&result);
        let y = fans[0].0.y;
        assert!(fans.iter().all(|(p, _)| (p.y - y).abs() < 1e-5), "flat ground is not flat");
        assert!(fans.iter().all(|(_, n)| (n.y - 1.0).abs() < 1e-5), "flat ground normals are not up");
    }

    #[test]
    fn shared_corners_are_single_vertices() {
        let result = build_summary_mesh_region(1, REGION, &|_, _| Some(5)).unwrap();
        assert_eq!(fan_vertices(&result).len(), MESH_REGION_CELLS as usize + CORNER_VERTICES);
    }

    #[test]
    fn build_waits_for_every_cell_of_region_and_ring() {
        let region_lat = mesh_region_lattice();
        let hole = |q: i32, r: i32| ((q, r) != (0, 0)).then_some(5);
        assert!(build_summary_mesh_region(1, REGION, &hole).is_none(), "built with a cell missing");
        let no_ring = |q: i32, r: i32| (region_lat.cell_id(q, r) == (0, 0)).then_some(5);
        assert!(build_summary_mesh_region(1, REGION, &no_ring).is_none(), "built with the ring missing");
    }

    #[test]
    fn adjacent_regions_agree_on_shared_vertices() {
        // A varying field: every vertex the two regions both place must be
        // at the same height, or the seam between them opens.
        let field = |q: i32, r: i32| Some(((q * 7 + r * 3).rem_euclid(11)) - 5);
        let a = build_summary_mesh_region(1, REGION, &field).unwrap();
        let b = build_summary_mesh_region(1, MeshRegionKey { r: 1, mn: 1, mm: 0 }, &field).unwrap();
        let av = fan_vertices(&a);
        let mut shared = 0;
        for (pb, _) in fan_vertices(&b) {
            for (pa, _) in &av {
                if (pa.x - pb.x).abs() < 1e-3 && (pa.z - pb.z).abs() < 1e-3 {
                    shared += 1;
                    assert!((pa.y - pb.y).abs() < 1e-3, "seam opens at {pa:?} vs {pb:?}");
                }
            }
        }
        assert!(shared > 0, "adjacent regions share no vertices");
    }

    #[test]
    fn steep_neighbours_make_steep_triangles_not_gaps() {
        // A cliff: half the field 20 steps higher. Every triangle still
        // shares its vertices (no vertex duplicated among the fans), and the
        // fan normals at the cliff lean over.
        let field = |q: i32, _r: i32| Some(if q > 0 { 20 } else { 0 });
        let result = build_summary_mesh_region(1, REGION, &field).unwrap();
        assert_eq!(fan_vertices(&result).len(), MESH_REGION_CELLS as usize + CORNER_VERTICES);
        let leaning = fan_vertices(&result).iter().filter(|(_, n)| n.y < 0.7).count();
        assert!(leaning > 0, "a 20-step cliff produced no steep normals");
    }
}
