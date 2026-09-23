//! Summary hex rendering for distant terrain.

//! A summary is a single flat-top hex representing a group of tiles.
//! At radius r, each summary covers tiles within hex distance r of its
//! center on an axis-aligned scaled lattice: center(sq,sr) = (sq*s, sr*s)
//! where s = 2r+1. The rendered flat-top hex at outer_radius = s tiles
//! perfectly on this lattice with zero gaps or overlaps.


use crate::geometry::flat_top_tile_center;

/// The sampling rule is shared with every producer that has no Bevy, the
/// world viewer among them.
pub use common::summary::{select_center_water, select_center_z, summarize, SummaryCell, SummarySource, TileSample, LOD_LEVELS};

// ── Constants ──

/// Camera distance (WU) per tile of summary scale — the band quality knob.
/// Band threshold: `threshold_horiz(r) = (2r+1)·2·K − CAMERA_DISTANCE`.

/// Anchored so the scale-3 band's outer edge lands exactly on
/// FIXED_STREAM_RADIUS_WU: 3·2K − CAMERA_DISTANCE = FIXED_STREAM_RADIUS_WU.
/// The ownership boundary (client Map vs server summaries) then coincides
/// with a band boundary — no band ever has mixed provenance.
pub const BAND_QUALITY_K: f32 = (crate::chunk::FIXED_STREAM_RADIUS_WU + common::camera::CAMERA_DISTANCE) / 6.0;

/// Hex outer radius (vertex-to-vertex half-diameter) in world units.
const HEX_OUTER_RADIUS: f32 = 1.0;

/// Width of the transition at a level's outer edge, in summaries of the
/// coarser level: the strip inside the band edge over which the level's
/// surface morphs onto the coarser one, meeting it at the edge.
pub const TRANSITION_SUMMARIES: f32 = 4.0;

/// Render-only depth bias per LoD level (WU). At a band edge the finer
/// surface has morphed onto the coarser one, so the two are exactly
/// coplanar along the seam and would z-fight. Coarser levels sink slightly
/// so the finer plate always wins. A tenth of one z-step per level:
/// invisible, but well outside depth-buffer noise.
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


/// Vertex offset tables for canonical doubled-integer coordinates.
/// Same formula as the tile grid canonical IDs, applied to summary-lattice coords.
pub const VX2: [i32; 6] = [1, 2, 1, -1, -2, -1];
pub const VZ2: [i32; 6] = [-1, 0, 1, 1, 0, -1];

/// Mesh region radius in summary-lattice space (271 summaries per region).
pub const MESH_REGION_RADIUS: u32 = 9;

// ── Radius formula ──

/// Smallest LoD level whose band covers the given camera distance.

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

/// Flat-to-flat width of one summary at level `r`, in world units.
pub fn summary_width_wu(r: u32) -> f32 {
    summary_outer_radius_wu(r) * 3.0_f32.sqrt()
}

/// Centre-to-corner radius of one summary at level `r`, in world units:
/// the longest edge of the fan triangles its mesh is made of, so every
/// vertex of a triangle over a point lies within it of that point.
pub fn summary_outer_radius_wu(r: u32) -> f32 {
    (2 * r + 1) as f32 * HEX_OUTER_RADIUS
}

/// The next coarser level of the ladder, if `r` is not the coarsest.
pub fn coarser_level(r: u32) -> Option<u32> {
    LOD_LEVELS.iter().copied().find(|&l| l > r)
}

/// The next finer level of the ladder, if `r` is not the finest.
pub fn finer_level(r: u32) -> Option<u32> {
    LOD_LEVELS.iter().copied().filter(|&l| l < r).last()
}

/// Width of the transition strip inside level `r`'s outer edge, in world
/// units: zero at the coarsest level, whose outer edge is the horizon.
pub fn transition_wu(r: u32) -> f32 {
    coarser_level(r).map_or(0.0, |c| TRANSITION_SUMMARIES * summary_width_wu(c))
}

/// Ground distance from the player at which level `r`'s band ends. At
/// horizontal distance D the worst-case camera-to-ground distance is
/// D + CAMERA_DISTANCE, so the camera-distance threshold
/// `(2r+1)·2·BAND_QUALITY_K` is brought back to the ground by that much.
pub fn threshold_horiz(r: u32) -> f32 {
    let scale = (2 * r + 1) as f32;
    (scale * 2.0 * BAND_QUALITY_K - common::camera::CAMERA_DISTANCE).max(0.0)
}

/// The reach: the ground distance the world is drawn to, where the ladder
/// ends, about one plate. Producers on both sides cover to it and the haze
/// completes inside it. A fixed distance, not a camera angle, so what the
/// camera can see is bounded by the ladder alone and nothing nearer hides
/// behind an early haze.
pub fn reach_wu() -> f32 {
    threshold_horiz(*LOD_LEVELS.last().expect("the ladder has a level"))
}

/// Level `r`'s band on the ladder, ignoring the horizon: from the finer
/// level's threshold to its own.
pub fn ladder_band(r: u32) -> Band {
    Band {
        r,
        inner_wu: finer_level(r).map_or(0.0, threshold_horiz),
        outer_wu: threshold_horiz(r),
    }
}

/// Level `r`'s band among the active bands, and whether it is the
/// outermost — the horizon lies at its outer edge and nothing beyond it
/// competes. A level with no active band (stale regions awaiting eviction,
/// or a ladder truncated by a shrunken horizon) gets its ladder band.
pub fn level_band(r: u32, bands: &[Band]) -> (Band, bool) {
    match bands.iter().position(|b| b.r == r) {
        Some(i) => (bands[i].clone(), i + 1 == bands.len()),
        None => (ladder_band(r), false),
    }
}

/// Compute active distance bands from player to `max_distance_wu` (horizontal).

/// One band per nested LoD level (`LOD_LEVELS`): band r covers
/// `[threshold_horiz(prev level), threshold_horiz(r)]`. If the horizon
/// extends past the coarsest level's threshold, the final band stretches
/// to cover it.
pub fn compute_active_bands(max_distance_wu: f32) -> Vec<Band> {
    if max_distance_wu <= 0.0 {
        return vec![Band { r: 0, inner_wu: 0.0, outer_wu: 0.0 }];
    }

    let mut bands = Vec::new();
    let mut prev = 0.0_f32;

    for &r in &LOD_LEVELS {
        let threshold_horiz = threshold_horiz(r);
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

/// World-space distance between the centers of adjacent mesh regions at
/// radius r: the region lattice's basis vector, √(3R²+3R+1) summaries long.
pub fn mesh_region_spacing_wu(r: u32) -> f32 {
    let cells = 3 * MESH_REGION_RADIUS * MESH_REGION_RADIUS + 3 * MESH_REGION_RADIUS + 1;
    (cells as f32).sqrt() * summary_width_wu(r)
}

// ── Summary Lattice ──

/// Axis-aligned summary lattice. Centers are placed at integer multiples
/// of `scale = 2r+1` along both axial axes. The rendered flat-top hex at
/// `outer_radius = scale * HEX_OUTER_RADIUS` tiles perfectly on this grid.

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

    /// The cell whose rendered hex is under world `xz`. The hexes tile the
    /// plane, so there is exactly one; the lattice is the tile grid scaled
    /// by `scale`, so the tile grid's own conversion at that radius finds it.
    pub fn cell_at(&self, xz: bevy::math::Vec2) -> (i32, i32) {
        use qrz::Convert;
        let scaled: qrz::Map<()> = qrz::Map::new(self.scale as f32, 1.0, qrz::HexOrientation::FlatTop);
        let cell = scaled.convert(bevy::math::Vec3::new(xz.x, 0.0, xz.y));
        (cell.q, cell.r)
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

    /// The tiles the cell covers: every tile whose nearest centre is this
    /// one, by [`SummaryLattice::cell_id`]. More than the hex ball, which
    /// leaves the tiles between balls to no cell: the cells' footprints
    /// tile the plane, `scale²` tiles each on average.
    pub fn tiles_covered(&self, id: (i32, i32)) -> impl Iterator<Item = (i32, i32)> + '_ {
        let (cq, cr) = self.cell_center(id);
        let reach = self.scale;
        (-reach..=reach).flat_map(move |dq| {
            let dr_min = (-reach).max(-dq - reach);
            let dr_max = reach.min(-dq + reach);
            (dr_min..=dr_max).map(move |dr| (cq + dq, cr + dr))
        })
        .filter(move |&(q, r)| self.cell_id(q, r) == id)
    }
}

/// Create the summary lattice for a given radius.
pub fn summary_lattice(radius: u32) -> SummaryLattice {
    SummaryLattice::new(radius)
}

/// Create the mesh region lattice (groups summaries into mesh regions).

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

/// Produces doubled-integer coordinates that uniquely identify each
/// corner vertex. Adjacent summaries sharing an edge produce matching
/// IDs at their shared corners.

/// Scoped per distance band — not globally unique across bands.
pub fn canonical_vertex_id(sq: i32, sr: i32, vertex_index: usize) -> (i32, i32) {
    (
        3 * sq + VX2[vertex_index],
        sq + 2 * sr + VZ2[vertex_index],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The cells' footprints tile the plane: every tile of a window is
    /// covered by exactly one cell, the one its nearest centre names.
    #[test]
    fn covered_tiles_partition_the_plane() {
        for radius in [1u32, 4] {
            let lattice = summary_lattice(radius);
            let mut seen = std::collections::HashMap::new();
            let span = 3 * lattice.scale;
            for sq in -3..=3 {
                for sr in -3..=3 {
                    for tile in lattice.tiles_covered((sq, sr)) {
                        assert!(seen.insert(tile, (sq, sr)).is_none(), "tile {tile:?} covered twice at r={radius}");
                    }
                }
            }
            for q in -span..=span {
                for r in -span..=span {
                    let id = lattice.cell_id(q, r);
                    if id.0.abs() <= 3 && id.1.abs() <= 3 {
                        assert_eq!(seen.get(&(q, r)), Some(&id), "tile ({q}, {r}) uncovered at r={radius}");
                    }
                }
            }
        }
    }

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

    // ── transition tests ──

    /// The strip lies inside its band, wider than one summary of the
    /// coarser level, so the morph has room to leave the relief before the
    /// edge; the coarsest level has no edge to leave across.
    #[test]
    fn transition_strip_lies_inside_the_band() {
        let bands = compute_active_bands(25_000.0);
        for pair in bands.windows(2) {
            let (fine, coarse) = (&pair[0], &pair[1]);
            let strip = transition_wu(fine.r);
            assert!(strip > summary_width_wu(coarse.r));
            assert!(strip < fine.outer_wu - fine.inner_wu, "r={} strip wider than its band", fine.r);
        }
        assert_eq!(transition_wu(*LOD_LEVELS.last().unwrap()), 0.0);
    }

    #[test]
    fn cell_at_agrees_with_cell_centres_and_tiles_the_plane() {
        for r in [0u32, 1, 4, 13] {
            let lat = SummaryLattice::new(r);
            for &cell in &[(0, 0), (3, -1), (-2, 5), (7, 7)] {
                let (cq, cr) = lat.cell_center(cell);
                let (wx, wz) = flat_top_tile_center(cq, cr, 1.0);
                assert_eq!(lat.cell_at(bevy::math::Vec2::new(wx, wz)), cell);
                // Just inside every corner is still this cell.
                let radius = lat.scale as f32;
                for o in crate::surface::corner_offsets(radius) {
                    let p = bevy::math::Vec2::new(wx, wz) + o * 0.999;
                    assert_eq!(lat.cell_at(p), cell, "r={r} corner {o:?}");
                }
            }
        }
    }

    #[test]
    fn bands_start_at_the_player() {
        assert_eq!(compute_active_bands(25_000.0)[0].inner_wu, 0.0);
        assert_eq!(ladder_band(0).inner_wu, 0.0);
    }

    #[test]
    fn level_band_marks_only_the_outermost_band() {
        let bands = compute_active_bands(25_000.0);
        assert!(bands.len() >= 3, "test needs several bands");
        for (i, band) in bands.iter().enumerate() {
            let (b, outermost) = level_band(band.r, &bands);
            assert_eq!(b.outer_wu, band.outer_wu);
            assert_eq!(outermost, i + 1 == bands.len());
        }
    }

    #[test]
    fn level_band_confines_an_inactive_level_to_its_ladder_band() {
        let bands = compute_active_bands(1_000.0);
        let beyond = *LOD_LEVELS.last().unwrap();
        assert!(bands.iter().all(|b| b.r != beyond));
        let (b, outermost) = level_band(beyond, &bands);
        assert_eq!((b.inner_wu, b.outer_wu), (ladder_band(beyond).inner_wu, ladder_band(beyond).outer_wu));
        assert!(!outermost);
    }

    #[test]
    fn ladder_bands_are_the_active_bands_before_the_horizon() {
        let bands = compute_active_bands(25_000.0);
        for band in bands.iter().take(bands.len() - 1) {
            let ladder = ladder_band(band.r);
            assert_eq!(ladder.inner_wu, band.inner_wu);
            assert_eq!(ladder.outer_wu, band.outer_wu);
        }
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

    // ── canonical corner id tests ──

    #[test]
    fn shared_corners_have_one_canonical_id() {
        // Every corner of cell (0,0) is a corner of the two cells listed
        // for it in CORNER_NEIGHBOURS; all three must derive the same id.
        use crate::surface::{CORNER_NEIGHBOURS, corner_offsets};
        let offsets = corner_offsets(1.0);
        for (i, [a, b]) in CORNER_NEIGHBOURS.iter().enumerate() {
            let id = canonical_vertex_id(0, 0, i);
            for n in [a, b] {
                // Find which corner index of the neighbour lands on the same
                // point, then compare ids.
                let (nx, nz) = flat_top_tile_center(n.0, n.1, 1.0);
                let p = (offsets[i].x, offsets[i].y);
                let k = (0..6)
                    .find(|&k| {
                        ((nx + offsets[k].x) - p.0).abs() < 1e-4 && ((nz + offsets[k].y) - p.1).abs() < 1e-4
                    })
                    .expect("shared corner");
                assert_eq!(canonical_vertex_id(n.0, n.1, k), id, "corner {i} vs {n:?}[{k}]");
            }
        }
    }
}
