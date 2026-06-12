//! Summary hex rendering for distant terrain.
//!
//! A summary is a single flat-top hex representing a group of tiles.
//! At radius r, each summary covers tiles within hex distance r of its
//! center on an axis-aligned scaled lattice: center(sq,sr) = (sq*s, sr*s)
//! where s = 2r+1. The rendered flat-top hex at outer_radius = s tiles
//! perfectly on this lattice with zero gaps or overlaps.

use bevy::math::Vec3;

use crate::geometry::flat_top_tile_center;

// ── Constants ──

/// Nested LoD levels: summary scales triple per level.
///
/// scale = 2r+1 ∈ {1, 3, 9, 27, 81, 243}. Tripling makes the levels nest:
/// every coarse summary center is also a fine summary center, and
/// `sample_center_z`'s 7 sample points at d = scale/3 land exactly on the
/// child level's summary centers (INV-010). Arbitrary integer radii do not
/// nest — adjacent-band lattices would share no structure at all.
pub const LOD_LEVELS: [u32; 6] = [0, 1, 4, 13, 40, 121];

/// Camera distance (WU) per tile of summary scale — the band quality knob.
/// Band threshold: `threshold_horiz(r) = (2r+1)·2·K − CAMERA_DISTANCE`.
///
/// Anchored so the scale-3 band's outer edge lands exactly on
/// FIXED_STREAM_RADIUS_WU (598.5): 3·2K − 120 = 598.5 → K = 119.75.
/// The ownership boundary (client Map vs server summaries) then coincides
/// with a band boundary — no band ever has mixed provenance.
pub const BAND_QUALITY_K: f32 = 119.75;

/// Hex outer radius (vertex-to-vertex half-diameter) in world units.
const HEX_OUTER_RADIUS: f32 = 1.0;

/// Render-only depth bias per LoD level (WU). Adjacent levels overlap near
/// band boundaries (footprint-overlap enumeration); on flat terrain their
/// plates would be exactly coplanar — nested sampling produces equal
/// center_z — and z-fight. Coarser levels sink slightly so the finer plate
/// always wins. A tenth of one z-step per level: invisible, but well
/// outside depth-buffer noise.
pub const LEVEL_DEPTH_BIAS_WU: f32 = 0.08;

/// Depth bias for a summary radius: rank of the level in LOD_LEVELS.
/// Radii not in the table (forced debug radii) use their would-be rank.
pub fn level_depth_bias(r: u32) -> f32 {
    let rank = LOD_LEVELS
        .iter()
        .position(|&l| l == r)
        .unwrap_or_else(|| LOD_LEVELS.partition_point(|&l| l < r));
    rank as f32 * LEVEL_DEPTH_BIAS_WU
}

use common::camera::RISE as Z_SCALE;

/// Vertex offset tables for canonical doubled-integer coordinates.
/// Same formula as the tile grid canonical IDs, applied to summary-lattice coords.
pub const VX2: [i32; 6] = [1, 2, 1, -1, -2, -1];
pub const VZ2: [i32; 6] = [-1, 0, 1, 1, 0, -1];

/// Mesh region radius in summary-lattice space (271 summaries per region).
pub const MESH_REGION_RADIUS: u32 = 9;

// ── Radius formula ──

/// Smallest LoD level whose band covers the given camera distance.
///
/// r=0: single tile. r=1: 7 tiles. r=4: 61 tiles. r=13: 1,099 tiles.
pub fn summary_radius(camera_distance_wu: f32) -> u32 {
    for &r in &LOD_LEVELS {
        if camera_distance_wu <= (2 * r + 1) as f32 * 2.0 * BAND_QUALITY_K {
            return r;
        }
    }
    *LOD_LEVELS.last().expect("LOD_LEVELS is non-empty")
}

// ── Distance Bands ──

/// A distance band: all mesh regions at a specific summary radius.
#[derive(Clone, Debug)]
pub struct Band {
    /// Summary radius for this band.
    pub r: u32,
    /// Inner edge in horizontal world units from player.
    pub inner_wu: f32,
    /// Outer edge in horizontal world units from player.
    pub outer_wu: f32,
}

/// Compute active distance bands from player to `max_distance_wu` (horizontal).
///
/// One band per nested LoD level (`LOD_LEVELS`): band r covers
/// `[threshold(prev level), threshold(r)]` where
/// `threshold_horiz(r) = (2r+1)·2·BAND_QUALITY_K − CAMERA_DISTANCE`
/// (at horizontal distance D from the player, the worst-case
/// camera-to-ground distance is D + CAMERA_DISTANCE). If the horizon
/// extends past the coarsest level's threshold, the final band stretches
/// to cover it.
pub fn compute_active_bands(max_distance_wu: f32) -> Vec<Band> {
    use common::camera::CAMERA_DISTANCE;

    if max_distance_wu <= 0.0 {
        return vec![Band { r: 0, inner_wu: 0.0, outer_wu: 0.0 }];
    }

    let mut bands = Vec::new();
    let mut prev = 0.0_f32;

    for &r in &LOD_LEVELS {
        let scale = (2 * r + 1) as f32;
        let threshold_horiz = (scale * 2.0 * BAND_QUALITY_K - CAMERA_DISTANCE).max(0.0);
        let outer = threshold_horiz.min(max_distance_wu);
        if outer <= prev && r > 0 {
            prev = threshold_horiz;
            continue;
        }
        bands.push(Band {
            r,
            inner_wu: prev,
            outer_wu: outer,
        });
        prev = threshold_horiz;
        if threshold_horiz >= max_distance_wu {
            break;
        }
    }

    // Horizon beyond the coarsest level: stretch the final band to cover it.
    if let Some(last) = bands.last_mut() {
        if last.outer_wu < max_distance_wu && prev < max_distance_wu {
            last.outer_wu = max_distance_wu;
        }
    }

    bands
}

/// Enumerate summary-lattice cells within a world-space annulus.
///
/// Returns `(sq, sr)` lattice coordinates for summaries whose world-space
/// centers fall within `[inner_wu, outer_wu]` of the given point.
/// Used by the server to determine which summaries to send per band.
pub fn visible_summary_cells_in_band(
    r: u32,
    center_wx: f32,
    center_wz: f32,
    inner_wu: f32,
    outer_wu: f32,
) -> Vec<(i32, i32)> {
    let lat = SummaryLattice::new(r);
    let scale = lat.scale as f64;

    // Convert world position to summary-lattice coordinates
    let cam_q = center_wx as f64 / 1.5;
    let cam_r = (center_wz as f64 - cam_q * 3.0_f64.sqrt() / 2.0) / 3.0_f64.sqrt();
    let cam_sq = (cam_q / scale).round() as i32;
    let cam_sr = (cam_r / scale).round() as i32;

    // Search radius in summary-lattice units
    let summary_flat = (2 * r + 1) as f32 * HEX_OUTER_RADIUS * (3.0_f32).sqrt();
    let search = ((outer_wu / summary_flat) as i32 + 2).min(200);

    let mut cells = Vec::new();
    for dq in -search..=search {
        let dr_min = (-search).max(-dq - search);
        let dr_max = search.min(-dq + search);
        for dr in dr_min..=dr_max {
            let sq = cam_sq + dq;
            let sr = cam_sr + dr;
            let (cq, cr) = lat.cell_center((sq, sr));
            let (wx, wz) = flat_top_tile_center(cq, cr, 1.0);
            let dx = wx - center_wx;
            let dz = wz - center_wz;
            let dist = (dx * dx + dz * dz).sqrt();
            if dist >= inner_wu && dist <= outer_wu {
                cells.push((sq, sr));
            }
        }
    }
    cells
}

/// Approximate world-space width of one mesh region at radius r.
/// Used for overlap extension at band boundaries.
pub fn mesh_region_extent_wu(r: u32) -> f32 {
    let scale = (2 * r + 1) as f32;
    // A mesh region spans ~2*MESH_REGION_RADIUS summaries across.
    // Each summary is scale tiles wide. Flat-to-flat width ≈ scale * sqrt(3).
    let summary_flat_to_flat = scale * HEX_OUTER_RADIUS * (3.0_f32).sqrt();
    (2 * MESH_REGION_RADIUS) as f32 * summary_flat_to_flat
}

// ── Summary Lattice ──

/// Axis-aligned summary lattice. Centers are placed at integer multiples
/// of `scale = 2r+1` along both axial axes. The rendered flat-top hex at
/// `outer_radius = scale * HEX_OUTER_RADIUS` tiles perfectly on this grid.
///
/// Each summary collects tiles within hex distance `r` of its center for
/// z-selection. The rendered hex covers slightly more area at the corners
/// (the hex-ball has 3r²+3r+1 tiles; the rendered hex footprint spans
/// (2r+1)² axial cells). The visual overlap at corners is covered by
/// neighboring summary geometry at the same or different z, bridged by skirts.
#[derive(Clone)]
pub struct SummaryLattice {
    pub radius: u32,
    pub scale: i32,
}

impl SummaryLattice {
    pub fn new(radius: u32) -> Self {
        Self {
            radius,
            scale: (2 * radius + 1) as i32,
        }
    }

    /// Center tile of a summary at lattice coordinates (sq, sr).
    pub fn cell_center(&self, id: (i32, i32)) -> (i32, i32) {
        (id.0 * self.scale, id.1 * self.scale)
    }

    /// Summary-lattice cell ID for a tile at (q, r).
    /// Finds the nearest center by hex distance, tie-breaking by lower (sq, sr).
    pub fn cell_id(&self, q: i32, r: i32) -> (i32, i32) {
        let s = self.scale as f64;
        let sqf = q as f64 / s;
        let srf = r as f64 / s;
        let sq0 = sqf.floor() as i32;
        let sr0 = srf.floor() as i32;

        let mut best = (sq0, sr0);
        let mut best_dist = i32::MAX;

        for dsq in 0..=1 {
            for dsr in 0..=1 {
                let sq = sq0 + dsq;
                let sr = sr0 + dsr;
                let cq = sq * self.scale;
                let cr = sr * self.scale;
                let dq = q - cq;
                let dr = r - cr;
                let dist = dq.abs().max(dr.abs()).max((dq + dr).abs());
                if dist < best_dist || (dist == best_dist && (sq, sr) < best) {
                    best_dist = dist;
                    best = (sq, sr);
                }
            }
        }
        best
    }

    /// Tiles in the hex ball of radius `r` around the cell center.
    /// Returns 3r²+3r+1 tiles used for z-selection and readiness checks.
    pub fn tiles_in_cell(&self, id: (i32, i32)) -> impl Iterator<Item = (i32, i32)> {
        let (cq, cr) = self.cell_center(id);
        let r = self.radius as i32;
        (-r..=r).flat_map(move |dq| {
            let dr_min = (-r).max(-dq - r);
            let dr_max = r.min(-dq + r);
            (dr_min..=dr_max).map(move |dr| (cq + dq, cr + dr))
        })
    }

    /// Number of tiles in the hex ball: 3r²+3r+1.
    pub fn tiles_per_cell(&self) -> u32 {
        3 * self.radius * self.radius + 3 * self.radius + 1
    }
}

/// Create the summary lattice for a given radius.
pub fn summary_lattice(radius: u32) -> SummaryLattice {
    SummaryLattice::new(radius)
}

/// Create the mesh region lattice (groups summaries into mesh regions).
///
/// Uses HexLattice::new(9) over summary-lattice coordinates.
/// Summary-lattice coords form a regular hex grid, so HexLattice
/// tiles them correctly into groups of 271 summaries.
pub fn mesh_region_lattice() -> common::HexLattice {
    common::HexLattice::new(MESH_REGION_RADIUS)
}

/// Tile count in a summary of given radius: 3r² + 3r + 1.
pub fn summary_tile_count(radius: u32) -> u32 {
    3 * radius * radius + 3 * radius + 1
}

// ── Canonical vertex IDs ──

/// Canonical vertex ID for a summary at lattice coordinates (sq, sr).
///
/// Produces doubled-integer coordinates that uniquely identify each
/// corner vertex. Adjacent summaries sharing an edge produce matching
/// IDs at their shared corners.
///
/// Scoped per distance band — not globally unique across bands.
pub fn canonical_vertex_id(sq: i32, sr: i32, vertex_index: usize) -> (i32, i32) {
    (
        3 * sq + VX2[vertex_index],
        sq + 2 * sr + VZ2[vertex_index],
    )
}

// ── Center z sampling ──

/// Sample 7 elevations (center + 6 hex-axis points) and select center_z.
///
/// The single center_z rule for every producer (Map, server EventRegistry,
/// flyover AdminComposite). For nested LoD levels (scale divisible by 3),
/// the sample distance d = scale/3 puts the 6 axis samples exactly on the
/// child level's summary centers (INV-010) — coarse summaries are anchored
/// to the same tiles that anchor their children, so refinement preserves
/// the silhouette. At r=1 the 7 samples are the entire hexball (exact).
pub fn sample_center_z(r: u32, sq: i32, sr: i32, mut elevation_at: impl FnMut(i32, i32) -> i32) -> i32 {
    sample_center_z_opt(r, sq, sr, |q, rr| Some(elevation_at(q, rr)))
        .expect("infallible elevation source")
}

/// Fallible variant of `sample_center_z` for tile sources with holes
/// (the client Map while chunks stream in). Returns None unless all
/// 7 samples are available.
pub fn sample_center_z_opt(
    r: u32,
    sq: i32,
    sr: i32,
    mut elevation_at: impl FnMut(i32, i32) -> Option<i32>,
) -> Option<i32> {
    let (cq, cr) = summary_lattice(r).cell_center((sq, sr));
    let d = (2 * r as i32 + 1) / 3;
    let offsets: [(i32, i32); 7] = [(0,0),(d,0),(-d,0),(0,d),(0,-d),(d,-d),(-d,d)];
    let mut zs = [0i32; 7];
    for (i, (dq, dr)) in offsets.into_iter().enumerate() {
        zs[i] = elevation_at(cq + dq, cr + dr)?;
    }
    Some(select_center_z(&zs))
}

// ── Center z selection ──

/// Select center_z using extremal deviation from the mean.
///
/// 1. Compute mean z of all tiles
/// 2. Select the tile with greatest |tile_z - mean|
/// 3. Tie-break: prefer higher z (peaks over valleys)
///
/// Returns the z value of the most extreme tile. If empty, returns 0.
pub fn select_center_z(tile_zs: &[i32]) -> i32 {
    if tile_zs.is_empty() {
        return 0;
    }
    if tile_zs.len() == 1 {
        return tile_zs[0];
    }

    let sum: f64 = tile_zs.iter().map(|&z| z as f64).sum();
    let mean = sum / tile_zs.len() as f64;

    let mut best_z = tile_zs[0];
    let mut best_dev = (tile_zs[0] as f64 - mean).abs();

    for &z in &tile_zs[1..] {
        let dev = (z as f64 - mean).abs();
        if dev > best_dev || (dev == best_dev && z > best_z) {
            best_z = z;
            best_dev = dev;
        }
    }

    best_z
}

// ── Summary Surface ──

/// Computed surface data for a single summary hex, ready for geometry emission.
pub struct SummarySurface {
    /// Center vertex position (world-space).
    pub center: Vec3,
    /// 6 corner vertex positions (world-space), flat-top ordering:
    /// NE(0), E(1), SE(2), SW(3), W(4), NW(5).
    pub corners: [Vec3; 6],
    /// Canonical vertex IDs for the 6 corners.
    pub corner_ids: [(i32, i32); 6],
    /// Summary-lattice coordinates.
    pub sq: i32,
    pub sr: i32,
}

impl SummarySurface {
    /// Compute the surface for a flat summary hex at radius r > 0.
    ///
    /// All 7 vertices (center + 6 corners) are at center_z elevation (flat).
    /// The outer radius is (2r+1) * HEX_OUTER_RADIUS, matching the lattice
    /// spacing so adjacent summaries tile with zero gaps or overlaps.
    pub fn flat(
        sq: i32,
        sr: i32,
        radius: u32,
        center_q: i32,
        center_r: i32,
        center_z: i32,
    ) -> Self {
        let (center_wx, center_wz) = flat_top_tile_center(center_q, center_r, HEX_OUTER_RADIUS);
        // Uniform per-level depth bias: keeps overlapping plates from
        // adjacent levels out of each other's depth range (finer on top).
        // Uniform within a level, so intra-level skirt matching is unaffected.
        let y = center_z as f32 * Z_SCALE + Z_SCALE - level_depth_bias(radius);
        let outer_radius = (2 * radius + 1) as f32 * HEX_OUTER_RADIUS;

        // Flat-top hex corner offsets at summary outer radius
        let w = (outer_radius as f64 * (3.0_f64).sqrt() / 2.0) as f32;
        let h = outer_radius / 2.0;
        let corner_offsets: [(f32, f32); 6] = [
            (h, -w),              // 0: NE
            (outer_radius, 0.0),  // 1: E
            (h, w),               // 2: SE
            (-h, w),              // 3: SW
            (-outer_radius, 0.0), // 4: W
            (-h, -w),             // 5: NW
        ];

        let center = Vec3::new(center_wx, y, center_wz);
        let corners = corner_offsets.map(|(dx, dz)| Vec3::new(center_wx + dx, y, center_wz + dz));
        let corner_ids = std::array::from_fn(|i| canonical_vertex_id(sq, sr, i));

        Self {
            center,
            corners,
            corner_ids,
            sq,
            sr,
        }
    }

    /// Emit flat hex geometry into mesh buffers.
    ///
    /// Positions are relative to `mesh_origin` for f32 precision.
    /// Returns the number of triangles emitted (always 6).
    pub fn emit_geometry(
        &self,
        positions: &mut Vec<[f32; 3]>,
        normals: &mut Vec<[f32; 3]>,
        indices: &mut Vec<u32>,
        mesh_origin: Vec3,
    ) -> u32 {
        let base_idx = positions.len() as u32;

        // Center vertex
        let c = self.center - mesh_origin;
        positions.push([c.x, c.y, c.z]);
        normals.push([0.0, 1.0, 0.0]);

        // 6 corner vertices
        for corner in &self.corners {
            let v = *corner - mesh_origin;
            positions.push([v.x, v.y, v.z]);
            normals.push([0.0, 1.0, 0.0]);
        }

        // 6 triangles: CCW fan from center to adjacent corner pairs
        // Matches existing winding: (center, v_next, v_curr)
        for i in 0..6u32 {
            let v1 = base_idx + 1 + i;
            let v2 = base_idx + 1 + ((i + 1) % 6);
            indices.extend([base_idx, v2, v1]);
        }

        6
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── SummaryLattice tests ──

    #[test]
    fn lattice_center_at_origin() {
        let lat = SummaryLattice::new(3);
        assert_eq!(lat.cell_center((0, 0)), (0, 0));
    }

    #[test]
    fn lattice_center_scaled() {
        let lat = SummaryLattice::new(3); // scale = 7
        assert_eq!(lat.cell_center((1, 0)), (7, 0));
        assert_eq!(lat.cell_center((0, 1)), (0, 7));
        assert_eq!(lat.cell_center((2, -1)), (14, -7));
    }

    #[test]
    fn lattice_cell_id_at_center() {
        let lat = SummaryLattice::new(3);
        assert_eq!(lat.cell_id(0, 0), (0, 0));
        assert_eq!(lat.cell_id(7, 0), (1, 0));
        assert_eq!(lat.cell_id(0, 7), (0, 1));
    }

    #[test]
    fn lattice_cell_id_nearest_center() {
        let lat = SummaryLattice::new(1); // scale = 3
        // Tile (1, 0): hex dist to (0,0) = 1, to (3,0) = 2 → belongs to (0,0)
        assert_eq!(lat.cell_id(1, 0), (0, 0));
        // Tile (2, 0): hex dist to (0,0) = 2, to (3,0) = 1 → belongs to (1,0)
        assert_eq!(lat.cell_id(2, 0), (1, 0));
    }

    #[test]
    fn lattice_tiles_per_cell() {
        assert_eq!(SummaryLattice::new(0).tiles_per_cell(), 1);
        assert_eq!(SummaryLattice::new(1).tiles_per_cell(), 7);
        assert_eq!(SummaryLattice::new(3).tiles_per_cell(), 37);
        assert_eq!(SummaryLattice::new(9).tiles_per_cell(), 271);
    }

    #[test]
    fn lattice_tile_count_matches_iteration() {
        for r in [1, 2, 3, 5, 9] {
            let lat = SummaryLattice::new(r);
            let count = lat.tiles_in_cell((0, 0)).count();
            assert_eq!(
                count,
                lat.tiles_per_cell() as usize,
                "r={r}: iterated {count}, expected {}",
                lat.tiles_per_cell()
            );
        }
    }

    #[test]
    fn adjacent_centers_world_distance_matches_hex_diameter() {
        // For axis-aligned scaling, adjacent centers along the q-axis are
        // `scale` tiles apart in q. In world space this is scale * 1.5 in x
        // and scale * sqrt(3)/2 in z.
        // The flat-top hex vertex-to-vertex diameter is 2 * outer_radius.
        // The flat-to-flat width (perpendicular to the flat edge) is
        // outer_radius * sqrt(3).
        // For perfect tiling, the inter-center distance along any hex
        // direction must equal the flat-to-flat width.
        for r in [1, 3, 5, 9] {
            let scale = (2 * r + 1) as f64;
            // Center (0,0) → world (0, 0)
            // Center (1,0) → tile (scale, 0) → world (scale*1.5, scale*sqrt3/2)
            let (wx, wz) = (scale * 1.5, scale * 3.0_f64.sqrt() / 2.0);
            let dist = (wx * wx + wz * wz).sqrt();

            // The flat-to-flat width of a flat-top hex with outer_radius = scale
            let flat_to_flat = scale * 3.0_f64.sqrt();

            // dist should equal flat_to_flat for perfect tiling along this direction
            assert!(
                (dist - flat_to_flat).abs() < 0.01,
                "r={r}: center distance {dist:.3} != flat-to-flat {flat_to_flat:.3}"
            );
        }
    }

    // ── center_z tests ──

    #[test]
    fn select_center_z_single_tile() {
        assert_eq!(select_center_z(&[42]), 42);
    }

    #[test]
    fn select_center_z_empty() {
        assert_eq!(select_center_z(&[]), 0);
    }

    #[test]
    fn select_center_z_uniform() {
        assert_eq!(select_center_z(&[5, 5, 5, 5]), 5);
    }

    #[test]
    fn select_center_z_peak() {
        assert_eq!(select_center_z(&[1, 2, 3, 2, 10]), 10);
    }

    #[test]
    fn select_center_z_valley() {
        assert_eq!(select_center_z(&[10, 10, 10, 10, 0]), 0);
    }

    #[test]
    fn select_center_z_tie_prefers_higher() {
        assert_eq!(select_center_z(&[5, 0, 10, 5]), 10);
    }

    #[test]
    fn select_center_z_symmetric_tie() {
        assert_eq!(select_center_z(&[0, 10]), 10);
    }

    // ── Nested level tests ──

    /// INV-010: each LoD level's 7 sample points land exactly on the child
    /// level's summary centers — the sampling rule is hierarchical.
    #[test]
    fn lod_levels_sample_points_are_child_centers() {
        for w in LOD_LEVELS.windows(2) {
            let (child_r, parent_r) = (w[0], w[1]);
            let child_scale = (2 * child_r + 1) as i32;
            let parent_scale = (2 * parent_r + 1) as i32;
            assert_eq!(parent_scale, 3 * child_scale, "levels must triple");

            let d = (2 * parent_r as i32 + 1) / 3;
            assert_eq!(d, child_scale, "sample distance must equal child scale");

            // Every sample point of a parent summary is a child lattice center.
            let parent_lat = SummaryLattice::new(parent_r);
            for &(sq, sr) in &[(0, 0), (1, 0), (-2, 3), (5, -4)] {
                let (cq, cr) = parent_lat.cell_center((sq, sr));
                let offsets = [(0,0),(d,0),(-d,0),(0,d),(0,-d),(d,-d),(-d,d)];
                for (dq, dr) in offsets {
                    let (q, r) = (cq + dq, cr + dr);
                    assert_eq!(q % child_scale, 0, "sample q={q} not a child center");
                    assert_eq!(r % child_scale, 0, "sample r={r} not a child center");
                }
            }
        }
    }

    /// The scale-3 band's outer edge is anchored to the chunk-stream radius:
    /// ownership boundary == band boundary.
    #[test]
    fn scale3_band_anchored_to_stream_radius() {
        let bands = compute_active_bands(10_000.0);
        let scale3 = bands.iter().find(|b| b.r == 1).expect("scale-3 band exists");
        assert!(
            (scale3.outer_wu - crate::chunk::FIXED_STREAM_RADIUS_WU).abs() < 0.01,
            "scale-3 outer {} != stream radius {}",
            scale3.outer_wu,
            crate::chunk::FIXED_STREAM_RADIUS_WU
        );
    }

    /// Bands tile [0, horizon] contiguously with one band per active level.
    #[test]
    fn bands_are_contiguous_nested_levels() {
        let bands = compute_active_bands(25_000.0);
        let mut prev_outer = 0.0_f32;
        for (i, band) in bands.iter().enumerate() {
            assert!(LOD_LEVELS.contains(&band.r), "band r={} not a LoD level", band.r);
            assert!((band.inner_wu - prev_outer).abs() < 0.01, "gap before band {i}");
            assert!(band.outer_wu > band.inner_wu, "empty band {i}");
            prev_outer = band.outer_wu;
        }
        assert!((prev_outer - 25_000.0).abs() < 0.01, "bands must reach the horizon");
    }

    // ── summary_radius tests ──

    #[test]
    fn summary_radius_zero_at_close() {
        assert_eq!(summary_radius(1.0), 0);
    }

    #[test]
    fn summary_radius_increases_with_distance() {
        let r1 = summary_radius(100.0);
        let r2 = summary_radius(200.0);
        assert!(r2 >= r1, "should increase: {r1} vs {r2}");
    }

    #[test]
    fn summary_radius_monotonic() {
        let mut prev = summary_radius(0.0);
        for d in (10..=500).step_by(10) {
            let r = summary_radius(d as f32);
            assert!(r >= prev, "decreased at d={d}: {prev} -> {r}");
            prev = r;
        }
    }

    #[test]
    fn summary_tile_count_values() {
        assert_eq!(summary_tile_count(0), 1);
        assert_eq!(summary_tile_count(1), 7);
        assert_eq!(summary_tile_count(2), 19);
        assert_eq!(summary_tile_count(3), 37);
        assert_eq!(summary_tile_count(9), 271);
    }

    // ── canonical_vertex_id tests ──

    #[test]
    fn canonical_vertex_id_east_west_match() {
        let a1 = canonical_vertex_id(0, 0, 1);
        let b4 = canonical_vertex_id(1, 0, 4);
        let a2 = canonical_vertex_id(0, 0, 2);
        let b5 = canonical_vertex_id(1, 0, 5);
        assert!(
            (a1 == b4 && a2 == b5) || (a1 == b5 && a2 == b4),
            "E/W edge mismatch: ({a1:?},{a2:?}) vs ({b4:?},{b5:?})"
        );
    }

    #[test]
    fn canonical_vertex_id_ne_sw_match() {
        let a0 = canonical_vertex_id(0, 0, 0);
        let a1 = canonical_vertex_id(0, 0, 1);
        let b3 = canonical_vertex_id(1, -1, 3);
        let b4 = canonical_vertex_id(1, -1, 4);
        assert!(
            (a0 == b3 && a1 == b4) || (a0 == b4 && a1 == b3),
            "NE/SW edge mismatch: ({a0:?},{a1:?}) vs ({b3:?},{b4:?})"
        );
    }

    #[test]
    fn canonical_vertex_id_se_nw_match() {
        let a2 = canonical_vertex_id(0, 0, 2);
        let a3 = canonical_vertex_id(0, 0, 3);
        let b5 = canonical_vertex_id(0, 1, 5);
        let b0 = canonical_vertex_id(0, 1, 0);
        assert!(
            (a2 == b5 && a3 == b0) || (a2 == b0 && a3 == b5),
            "SE/NW edge mismatch: ({a2:?},{a3:?}) vs ({b5:?},{b0:?})"
        );
    }

    // ── SummarySurface tests ──

    #[test]
    fn flat_surface_all_same_y() {
        let surface = SummarySurface::flat(0, 0, 3, 0, 0, 10);
        let expected_y = 10.0 * Z_SCALE + Z_SCALE - level_depth_bias(3);
        assert!((surface.center.y - expected_y).abs() < 1e-6);
        for corner in &surface.corners {
            assert!(
                (corner.y - expected_y).abs() < 1e-6,
                "corner Y {} != center Y {}",
                corner.y,
                expected_y
            );
        }
    }

    #[test]
    fn flat_surface_outer_radius() {
        let radius = 3u32;
        let surface = SummarySurface::flat(0, 0, radius, 0, 0, 0);
        let expected_outer = (2 * radius + 1) as f32 * HEX_OUTER_RADIUS;
        let dx = surface.corners[1].x - surface.center.x;
        assert!(
            (dx - expected_outer).abs() < 1e-4,
            "E corner dx {dx} != expected {expected_outer}"
        );
    }

    #[test]
    fn emit_geometry_counts() {
        let surface = SummarySurface::flat(0, 0, 1, 0, 0, 5);
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut indices = Vec::new();
        let tris = surface.emit_geometry(&mut positions, &mut normals, &mut indices, Vec3::ZERO);

        assert_eq!(tris, 6);
        assert_eq!(positions.len(), 7);
        assert_eq!(normals.len(), 7);
        assert_eq!(indices.len(), 18);
    }

    #[test]
    fn emit_geometry_normals_up() {
        let surface = SummarySurface::flat(0, 0, 2, 0, 0, 0);
        let mut normals = Vec::new();
        surface.emit_geometry(&mut Vec::new(), &mut normals, &mut Vec::new(), Vec3::ZERO);
        for n in &normals {
            assert!((n[0]).abs() < 1e-6 && (n[1] - 1.0).abs() < 1e-6 && (n[2]).abs() < 1e-6);
        }
    }

    #[test]
    fn adjacent_summaries_share_corner_world_positions() {
        // On a flat-top hex, the +q neighbor shares edge via
        // SKIRT_VERTEX_MAP dir 3: curr (1,2) ↔ neighbor (5,4).
        // So (0,0).E(1) matches (1,0).NW(5) and (0,0).SE(2) matches (1,0).W(4).
        let r = 3u32;
        let lat = SummaryLattice::new(r);
        let (cq0, cr0) = lat.cell_center((0, 0));
        let (cq1, cr1) = lat.cell_center((1, 0));
        let s0 = SummarySurface::flat(0, 0, r, cq0, cr0, 5);
        let s1 = SummarySurface::flat(1, 0, r, cq1, cr1, 5);

        let eps = 1e-3;
        // s0 E(1) should equal s1 NW(5) in XZ
        assert!(
            (s0.corners[1].x - s1.corners[5].x).abs() < eps
                && (s0.corners[1].z - s1.corners[5].z).abs() < eps,
            "s0.E != s1.NW: ({:.3},{:.3}) vs ({:.3},{:.3})",
            s0.corners[1].x, s0.corners[1].z,
            s1.corners[5].x, s1.corners[5].z,
        );
        // s0 SE(2) should equal s1 W(4) in XZ
        assert!(
            (s0.corners[2].x - s1.corners[4].x).abs() < eps
                && (s0.corners[2].z - s1.corners[4].z).abs() < eps,
            "s0.SE != s1.W: ({:.3},{:.3}) vs ({:.3},{:.3})",
            s0.corners[2].x, s0.corners[2].z,
            s1.corners[4].x, s1.corners[4].z,
        );
        // Canonical vertex IDs must also match at those shared corners
        assert_eq!(s0.corner_ids[1], s1.corner_ids[5], "E/NW vertex ID mismatch");
        assert_eq!(s0.corner_ids[2], s1.corner_ids[4], "SE/W vertex ID mismatch");
    }
}
