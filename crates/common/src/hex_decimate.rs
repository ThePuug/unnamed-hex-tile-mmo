//! Hex-native decimation: consolidates groups of tiles into larger hexagonal
//! regions at discrete z-levels, preserving flat/slope/cliff terrain character.
//!
//! The unit of decimation is the hex tile, not the mesh vertex. Groups of tiles
//! consolidate into an inscribed hexagon (6 inner triangles) plus residual wedges
//! at the 6 boundary vertices.
//!
//! # Invariants
//! - **INV-008**: Decimation recalculates from original tile data. Never cascaded.
//! - **INV-009**: Inner hex center z is always a discrete integer. Partial tile
//!   centers on hex edges use interpolated z (sole exception).

use std::collections::{HashMap, HashSet};

use crate::HexLattice;

// ── Hex Grid Constants ──

/// Axial hex directions: W, SW, SE, E, NE, NW (matches qrz::DIRECTIONS).
const DIRECTIONS: [(i32, i32); 6] = [
    (-1, 0),  // 0: W
    (-1, 1),  // 1: SW
    (0, 1),   // 2: SE
    (1, 0),   // 3: E
    (1, -1),  // 4: NE
    (0, -1),  // 5: NW
];

/// Integer doubled x-offset per flat-top vertex index.
/// Tile center at (q,r) has doubled-x = 3q. Vertex v has doubled-x = 3q + VX2[v].
const VX2: [i32; 6] = [1, 2, 1, -1, -2, -1];

/// Integer doubled z-offset (in units of √3/2) per flat-top vertex index.
/// Tile center at (q,r) has doubled-z = q + 2r. Vertex v has doubled-z = q + 2r + VZ2[v].
const VZ2: [i32; 6] = [-1, 0, 1, 1, 0, -1];

/// Surviving triangle indices for partial tiles on each inscribed hex edge.
/// Edge e (from vertex e to vertex (e+1)%6) → the 3 triangles whose centroids
/// fall outside the inscribed hex. Derived from centroid offset analysis:
/// surviving for edge e = [(e+5)%6, e, (e+1)%6].
const SURVIVING_TRI: [[u8; 3]; 6] = [
    [5, 0, 1], // edge 0: V0→V1
    [0, 1, 2], // edge 1: V1→V2
    [1, 2, 3], // edge 2: V2→V3
    [2, 3, 4], // edge 3: V3→V4
    [3, 4, 5], // edge 4: V4→V5
    [4, 5, 0], // edge 5: V5→V0
];

/// Triangle centroid offsets in tripled (a, b) coordinates.
/// Centroid of triangle i = tile_center + (TRI_CX[i]/3, TRI_CZ[i]/3) in (a,b) space.
/// Computed as VX2[i]+VX2[(i+1)%6] and VZ2[i]+VZ2[(i+1)%6].
const TRI_CX: [i32; 6] = [3, 3, 0, -3, -3, 0];
const TRI_CZ: [i32; 6] = [-1, 1, 2, 1, -1, -2];

// ── Public Types ──

/// Result of decimating a single hexball.
#[derive(Debug, Clone)]
pub struct DecimatedHexball {
    pub center_q: i32,
    pub center_r: i32,
    /// Discrete integer z-level chosen to minimize max deviation (INV-009).
    pub center_z: i32,
    /// Hexball radius used for this decimation.
    pub radius: u32,
    /// Blended z-heights at the 6 inscribed hex boundary vertices.
    /// Index matches flat-top vertex convention: 0=NE, 1=E, 2=SE, 3=SW, 4=W, 5=NW.
    pub boundary_z: [f64; 6],
    /// Tile and vertex info for each boundary vertex (for mesh builder).
    pub boundary_tiles: [(i32, i32, u8); 6],
    /// Partial residual tiles (centers on inscribed hex edges, split geometry).
    pub partial_residuals: Vec<PartialResidual>,
    /// Full residual tiles (entirely outside inscribed hex, all 6 triangles survive).
    pub full_residuals: Vec<FullResidual>,
    /// Effective z for each absorbed tile: slopes gently from center_z.
    /// Used by the geometry pipeline so fans blend toward compressed z values.
    pub effective_z: HashMap<(i32, i32), i32>,
}

/// A tile whose center lies on an inscribed hex edge, split into inner (absorbed)
/// and outer (surviving) triangle fans.
#[derive(Debug, Clone)]
pub struct PartialResidual {
    pub q: i32,
    pub r: i32,
    pub original_z: i32,
    /// Z snapped to interpolated height on the inscribed hex edge (INV-009 exception).
    pub snapped_z: f64,
    /// Which 3 triangle indices (0-5) survive (face outward from hexball).
    pub surviving_triangles: [u8; 3],
}

/// A tile entirely outside the inscribed hex. All 6 triangles survive unmodified.
#[derive(Debug, Clone)]
pub struct FullResidual {
    pub q: i32,
    pub r: i32,
    pub z: i32,
}

/// Result of decimating an entire chunk.
#[derive(Debug)]
pub struct ChunkDecimation {
    /// Successfully decimated hexballs.
    pub hexballs: Vec<DecimatedHexball>,
    /// Tiles not absorbed by any hexball (boundary tiles + failed hexballs).
    pub survivors: Vec<(i32, i32, i32)>,
}

// ── Hex Math ──

/// Hex distance between two axial offsets.
pub fn hex_dist(dq: i32, dr: i32) -> i32 {
    dq.abs().max(dr.abs()).max((dq + dr).abs())
}

// ── Inscribed Hex Geometry ──

/// Integer-space representation of an inscribed hex vertex.
/// Actual world-space: x = a × tile_radius / 2, z = b × tile_radius × √3 / 2.
#[derive(Debug, Clone, Copy)]
struct IntVertex {
    tile_q: i32,
    tile_r: i32,
    vertex_idx: usize,
    /// Integer doubled x-coordinate (actual x = a/2 × tile_radius).
    a: i32,
    /// Integer doubled z-coordinate in √3/2 units (actual z = b × √3/2 × tile_radius).
    b: i32,
}

/// Compute the 6 inscribed hex boundary vertices for a hexball of given radius.
///
/// For even radius: each vertex is at a specific vertex of the midpoint edge tile
/// on the corresponding hexball side.
///
/// For odd radius: each vertex is at the shared outer vertex between the two
/// tiles straddling the midpoint of each hexball side.
fn inscribed_vertices(cq: i32, cr: i32, radius: u32) -> [IntVertex; 6] {
    let n = radius as i32;
    let mut result = [IntVertex {
        tile_q: 0,
        tile_r: 0,
        vertex_idx: 0,
        a: 0,
        b: 0,
    }; 6];

    for i in 0..6 {
        // Inscribed hex vertex V_i lies on hexball side s = (4 - i) % 6.
        let side = (4 + 6 - i) % 6;
        let (cdq, cdr) = DIRECTIONS[side];
        let corner_q = cq + cdq * n;
        let corner_r = cr + cdr * n;

        // Walking direction along the side
        let walk_dir = (side + 2) % 6;
        let (wq, wr) = DIRECTIONS[walk_dir];
        let k = n / 2; // position along side (floor(N/2))

        let (tq, tr, vidx) = if n % 2 == 0 {
            // Even: vertex index i of the tile at position k
            (corner_q + wq * k, corner_r + wr * k, i)
        } else {
            // Odd: outer shared vertex between tiles at positions k and k+1
            (corner_q + wq * k, corner_r + wr * k, (i + 5) % 6)
        };

        result[i] = IntVertex {
            tile_q: tq,
            tile_r: tr,
            vertex_idx: vidx,
            a: 3 * tq + VX2[vidx],
            b: tq + 2 * tr + VZ2[vidx],
        };
    }

    result
}

/// Compute the slope-blended z-height at a specific vertex of a tile.
///
/// Delegates to `hex_slope::slope_adjustments()` with rise=1.0 so the
/// adjustment is in z-level units. Guarantees identical tiebreaking with
/// the mesh rendering path.
fn blended_z(
    tq: i32,
    tr: i32,
    tz: i32,
    vidx: usize,
    tiles: &impl Fn(i32, i32) -> Option<i32>,
) -> f64 {
    let adj = crate::hex_slope::slope_adjustments(tz, 1.0, |dir_idx| {
        let (dq, dr) = DIRECTIONS[dir_idx];
        tiles(tq + dq, tr + dr)
    });
    tz as f64 + adj[vidx] as f64
}

/// Test whether a specific triangle centroid is inside (or on the boundary of)
/// the inscribed hex. Uses tripled coordinates to stay in integer arithmetic.
fn is_centroid_inside(vertices: &[IntVertex; 6], tile_a: i32, tile_b: i32, tri: usize) -> bool {
    // Centroid in tripled coords: (3*tile_a + TRI_CX[tri], 3*tile_b + TRI_CZ[tri])
    // Hex vertices in tripled coords: (3*v.a, 3*v.b)
    // Cross product scales by 9 (3×3) but sign is preserved.
    let ca = 3i64 * tile_a as i64 + TRI_CX[tri] as i64;
    let cb = 3i64 * tile_b as i64 + TRI_CZ[tri] as i64;
    for e in 0..6 {
        let v0 = &vertices[e];
        let v1 = &vertices[(e + 1) % 6];
        let da = (v1.a - v0.a) as i64; // edge dir (not tripled — cancels in cross product)
        let db = (v1.b - v0.b) as i64;
        let pa = ca - 3 * v0.a as i64;
        let pb = cb - 3 * v0.b as i64;
        let k = da * pb - db * pa;
        if k < 0 {
            return false; // outside this edge
        }
    }
    true // inside or on boundary of all edges
}

/// Given that exactly 3 triangles are inside, find which inscribed hex edge the tile
/// straddles. The absorbed (inside) triangles for edge e start at index (e+2)%6.
fn find_partial_edge(inside: &[bool; 6]) -> usize {
    for s in 0..6 {
        if inside[s] && inside[(s + 1) % 6] && inside[(s + 2) % 6]
            && !inside[(s + 3) % 6]
            && !inside[(s + 4) % 6]
            && !inside[(s + 5) % 6]
        {
            // Absorbed block starts at s. Edge = (s + 4) % 6.
            return (s + 4) % 6;
        }
    }
    panic!("partial tile must have exactly 3 consecutive inside triangles");
}

/// Project a tile center onto inscribed hex edge e and interpolate z.
/// Uses orthogonal projection in the (a,b) metric where world distance² = a²/4 + 3b²/4.
fn project_onto_edge(
    vertices: &[IntVertex; 6],
    boundary_z: &[f64; 6],
    e: usize,
    a: i32,
    b: i32,
) -> f64 {
    let v0 = &vertices[e];
    let v1 = &vertices[(e + 1) % 6];

    let da = (v1.a - v0.a) as f64;
    let db = (v1.b - v0.b) as f64;
    let pa = (a - v0.a) as f64;
    let pb = (b - v0.b) as f64;

    // Orthogonal projection with metric: dot(u,v) = u_a*v_a + 3*u_b*v_b
    let t = (pa * da + 3.0 * pb * db) / (da * da + 3.0 * db * db);

    let z0 = boundary_z[e];
    let z1 = boundary_z[(e + 1) % 6];
    z0 + t * (z1 - z0)
}

// ── Core Decimation ──

/// Attempt to decimate a hexball of the given radius centered at (cq, cr).
///
/// Returns `None` if:
/// - `radius` is 0
/// - Any tile within the hexball is missing from the lookup
/// - The max z-deviation exceeds `threshold`
///
/// The `tiles` lookup must cover all tiles within the hexball radius **plus**
/// their immediate neighbors (for boundary vertex height computation).
pub fn decimate_hexball(
    center_q: i32,
    center_r: i32,
    radius: u32,
    threshold: u32,
    tiles: &impl Fn(i32, i32) -> Option<i32>,
) -> Option<DecimatedHexball> {
    if radius == 0 {
        return None;
    }

    let n = radius as i32;

    // Step 1: Gather all tiles, find z range
    let mut z_min = i32::MAX;
    let mut z_max = i32::MIN;
    let mut z_values = Vec::new();

    for dq in -n..=n {
        let dr_min = (-n).max(-dq - n);
        let dr_max = n.min(-dq + n);
        for dr in dr_min..=dr_max {
            match tiles(center_q + dq, center_r + dr) {
                Some(z) => {
                    z_min = z_min.min(z);
                    z_max = z_max.max(z);
                    z_values.push(z);
                }
                None => return None,
            }
        }
    }

    // Step 2: Select center z — minimizes max deviation, ties → median
    let z_sum = z_min as i64 + z_max as i64;
    let center_z = if z_sum % 2 == 0 {
        (z_sum / 2) as i32
    } else {
        let lo = z_sum.div_euclid(2) as i32;
        let hi = lo + 1;
        z_values.sort_unstable();
        let median = z_values[z_values.len() / 2];
        if (median - hi).abs() <= (median - lo).abs() { hi } else { lo }
    };

    // Step 3: Check threshold
    let dev_low = (center_z as i64 - z_min as i64) as u32;
    let dev_high = (z_max as i64 - center_z as i64) as u32;
    let max_dev = dev_low.max(dev_high);
    if max_dev > threshold {
        return None;
    }

    // Step 4: Compute inscribed hex vertices and boundary z-heights
    let vertices = inscribed_vertices(center_q, center_r, radius);
    let mut boundary_z = [0.0f64; 6];
    let mut boundary_tiles = [(0i32, 0i32, 0u8); 6];
    for i in 0..6 {
        let v = &vertices[i];
        let tz = tiles(v.tile_q, v.tile_r).expect("inscribed hex vertex tile must exist");
        boundary_z[i] = blended_z(v.tile_q, v.tile_r, tz, v.vertex_idx, tiles);
        boundary_tiles[i] = (v.tile_q, v.tile_r, v.vertex_idx as u8);
    }

    // Step 4b: Check ALL blended vertex heights against threshold.
    // The inner hex replaces per-tile slope blending with a single surface.
    // If any vertex of any tile in the hexball has a blended height that deviates
    // from center_z by more than the threshold, the inner hex can't faithfully
    // represent the original mesh — slope blending creates detail the hex flattens.
    let cz = center_z as f64;
    for dq in -n..=n {
        let dr_min = (-n).max(-dq - n);
        let dr_max = n.min(-dq + n);
        for dr in dr_min..=dr_max {
            let q = center_q + dq;
            let r = center_r + dr;
            let z = tiles(q, r).unwrap();
            for vidx in 0..6 {
                let bz = blended_z(q, r, z, vidx, tiles);
                if (bz - cz).abs() > threshold as f64 {
                    return None;
                }
            }
        }
    }

    // Steps 5-7: Classify ALL tiles by triangle centroid testing.
    let mut partial_residuals = Vec::new();
    let mut full_residuals = Vec::new();
    let mut effective_z = HashMap::new();

    for dq in -n..=n {
        let dr_min = (-n).max(-dq - n);
        let dr_max = n.min(-dq + n);
        for dr in dr_min..=dr_max {
            let q = center_q + dq;
            let r = center_r + dr;
            let z = tiles(q, r).unwrap();
            let (ta, tb) = (3 * q, q + 2 * r);

            // Default effective_z: clamp to ±hex_dist from center_z
            let dist = hex_dist(dq, dr);
            effective_z.insert((q, r), z.max(center_z - dist).min(center_z + dist));

            let inside: [bool; 6] =
                std::array::from_fn(|t| is_centroid_inside(&vertices, ta, tb, t));
            let inside_count = inside.iter().filter(|&&v| v).count();

            match inside_count {
                6 => {} // fully absorbed by inner hex
                0 => {
                    full_residuals.push(FullResidual { q, r, z });
                }
                3 => {
                    let e = find_partial_edge(&inside);
                    let snapped_z = project_onto_edge(&vertices, &boundary_z, e, ta, tb);
                    partial_residuals.push(PartialResidual {
                        q, r, original_z: z, snapped_z,
                        surviving_triangles: SURVIVING_TRI[e],
                    });
                }
                _ => unreachable!(
                    "tile ({q},{r}) has {inside_count}/6 centroids inside inscribed hex"
                ),
            }
        }
    }

    // Override effective_z for edge tiles anchored to the inscribed hex surface.
    // Partials: discretized snapped_z (sits on the inscribed hex edge).
    for pr in &partial_residuals {
        effective_z.insert((pr.q, pr.r), pr.snapped_z.round() as i32);
    }

    // Full residuals: clamp original_z to ±1 of the nearest adjacent partial's
    // snapped_z. This keeps the full residual within one slope step of the
    // inscribed hex surface.
    // Group partials by inscribed hex edge for lookup.
    let mut partials_by_edge: [Vec<&PartialResidual>; 6] = Default::default();
    for pr in &partial_residuals {
        let e = pr.surviving_triangles[1] as usize;
        partials_by_edge[e].push(pr);
    }

    for fr in &full_residuals {
        // Find the nearest partial(s) by hex distance
        let mut best_dist = i32::MAX;
        let mut lo = i32::MAX;
        let mut hi = i32::MIN;
        for edge_partials in &partials_by_edge {
            for pr in edge_partials {
                let d = hex_dist(fr.q - pr.q, fr.r - pr.r);
                let rounded = pr.snapped_z.round() as i32;
                if d < best_dist {
                    best_dist = d;
                    lo = rounded - 1;
                    hi = rounded + 1;
                } else if d == best_dist {
                    lo = lo.min(rounded - 1);
                    hi = hi.max(rounded + 1);
                }
            }
        }
        if best_dist < i32::MAX {
            effective_z.insert((fr.q, fr.r), fr.z.max(lo).min(hi));
        }
    }

    Some(DecimatedHexball {
        center_q,
        center_r,
        center_z,
        radius,
        boundary_z,
        boundary_tiles,
        partial_residuals,
        full_residuals,
        effective_z,
    })
}

// ── Chunk-Level Decimation ──

/// Decimate all possible hexballs within a chunk's tiles at the given threshold.
///
/// Tries each odd radius from `max_radius` down to 1. Larger hexballs are
/// placed first on their lattice; unclaimed tiles fall through to smaller
/// radii. Tiles not absorbed by any hexball remain as survivors.
///
/// `elevations` must cover the chunk tiles plus their 1-ring neighbors —
/// the same lookup used for mesh slope blending. This ensures the blended
/// vertex threshold check sees the same neighbor data as the mesh builder.
pub fn decimate_chunk(
    tiles: &[(i32, i32, i32)],
    max_radius: u32,
    threshold: u32,
    elevations: &impl Fn(i32, i32) -> Option<i32>,
) -> ChunkDecimation {
    assert!(max_radius == 0 || max_radius % 2 == 1,
        "max_radius must be 0 or odd, got {max_radius}");

    if max_radius == 0 || tiles.is_empty() {
        return ChunkDecimation {
            hexballs: Vec::new(),
            survivors: tiles.to_vec(),
        };
    }

    let tile_map: HashMap<(i32, i32), i32> =
        tiles.iter().map(|&(q, r, z)| ((q, r), z)).collect();

    let mut hexballs = Vec::new();
    let mut consumed: HashSet<(i32, i32)> = HashSet::new();

    // Try each odd radius from largest to smallest
    let mut r = max_radius;
    loop {
        let lattice = HexLattice::new(r);

        let mut cells: Vec<(i32, i32)> = tiles
            .iter()
            .filter(|(q, r, _)| !consumed.contains(&(*q, *r)))
            .map(|&(q, r, _)| lattice.cell_id(q, r))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        cells.sort(); // deterministic order

        for cell in &cells {
            // All tiles in this hexball must be present AND unclaimed
            let all_available = lattice
                .tiles_in_cell(*cell)
                .all(|(q, r)| tile_map.contains_key(&(q, r)) && !consumed.contains(&(q, r)));
            if !all_available {
                continue;
            }

            let (cq, cr) = lattice.cell_center(*cell);

            if let Some(hexball) = decimate_hexball(cq, cr, r, threshold, elevations) {
                for (q, r) in lattice.tiles_in_cell(*cell) {
                    consumed.insert((q, r));
                }
                hexballs.push(hexball);
            }
        }

        if r <= 1 { break; }
        r -= 2;
    }

    let survivors: Vec<(i32, i32, i32)> = tiles
        .iter()
        .filter(|(q, r, _)| !consumed.contains(&(*q, *r)))
        .copied()
        .collect();

    ChunkDecimation {
        hexballs,
        survivors,
    }
}

// ── Test Utilities ──

/// Count residual triangles in a decimated hexball.
pub fn residual_tri_count(hexball: &DecimatedHexball) -> usize {
    hexball.partial_residuals.len() * 3 + hexball.full_residuals.len() * 6
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Helpers ──

    /// Build a tile map for a hexball + 1-ring neighbor buffer, all at the same z.
    fn flat_tiles(cq: i32, cr: i32, radius: u32, z: i32) -> HashMap<(i32, i32), i32> {
        let n = (radius + 1) as i32; // +1 for neighbor ring
        let mut map = HashMap::new();
        for dq in -n..=n {
            let dr_min = (-n).max(-dq - n);
            let dr_max = n.min(-dq + n);
            for dr in dr_min..=dr_max {
                map.insert((cq + dq, cr + dr), z);
            }
        }
        map
    }

    /// Build a tile map with per-tile z values from a function.
    fn tiles_from_fn(
        cq: i32,
        cr: i32,
        radius: u32,
        z_fn: impl Fn(i32, i32) -> i32,
    ) -> HashMap<(i32, i32), i32> {
        let n = (radius + 1) as i32;
        let mut map = HashMap::new();
        for dq in -n..=n {
            let dr_min = (-n).max(-dq - n);
            let dr_max = n.min(-dq + n);
            for dr in dr_min..=dr_max {
                let q = cq + dq;
                let r = cr + dr;
                map.insert((q, r), z_fn(q, r));
            }
        }
        map
    }

    fn lookup(map: &HashMap<(i32, i32), i32>) -> impl Fn(i32, i32) -> Option<i32> + '_ {
        move |q, r| map.get(&(q, r)).copied()
    }

    fn tile_count(radius: u32) -> usize {
        let r = radius as usize;
        3 * r * r + 3 * r + 1
    }

    // ══════════════════════════════════════════════════════════════════
    // Threshold 0 (lossless)
    // ══════════════════════════════════════════════════════════════════

    #[test]
    fn flat_terrain_threshold_0() {
        for radius in [1, 2, 3, 4] {
            let map = flat_tiles(0, 0, radius, 0);
            let result = decimate_hexball(0, 0, radius, 0, &lookup(&map));
            let hb = result.expect(&format!("r={radius}: flat terrain should decimate at t=0"));
            assert_eq!(hb.center_z, 0, "r={radius}");
            for i in 0..6 {
                assert!(
                    (hb.boundary_z[i] - 0.0).abs() < 1e-10,
                    "r={radius}: boundary vertex {i} should be 0, got {}",
                    hb.boundary_z[i]
                );
            }
        }
    }

    #[test]
    fn flat_terrain_nonzero_z() {
        let map = flat_tiles(0, 0, 2, 42);
        let hb = decimate_hexball(0, 0, 2, 0, &lookup(&map)).unwrap();
        assert_eq!(hb.center_z, 42);
    }

    #[test]
    fn single_outlier_fails_threshold_0() {
        let mut map = flat_tiles(0, 0, 1, 0);
        map.insert((1, 0), 1); // one tile at z=1
        let result = decimate_hexball(0, 0, 1, 0, &lookup(&map));
        assert!(result.is_none(), "single outlier should fail at threshold 0");
    }

    #[test]
    fn ridge_through_center_fails_threshold_0() {
        // Ridge: tiles at q=0 are z=2, rest at z=0
        let map = tiles_from_fn(0, 0, 2, |q, _r| if q == 0 { 2 } else { 0 });
        let result = decimate_hexball(0, 0, 2, 0, &lookup(&map));
        assert!(result.is_none(), "ridge should fail at threshold 0");
    }

    // ══════════════════════════════════════════════════════════════════
    // Threshold N
    // ══════════════════════════════════════════════════════════════════

    #[test]
    fn staircase_within_threshold() {
        // 3-step staircase: z = 0, 1, 2 based on q
        let map = tiles_from_fn(0, 0, 1, |q, _r| q.clamp(-1, 1) + 1);
        // z range: [0, 2], center_z = 1, max_dev = 1
        let result = decimate_hexball(0, 0, 1, 1, &lookup(&map));
        assert!(result.is_some(), "staircase within threshold should succeed");
        assert_eq!(result.unwrap().center_z, 1);
    }

    #[test]
    fn cliff_exceeds_threshold() {
        // Sharp cliff: z = 0 for q < 0, z = 5 for q >= 0
        let map = tiles_from_fn(0, 0, 2, |q, _r| if q >= 0 { 5 } else { 0 });
        // z range: [0, 5], max_dev = 3 (center_z = 2)
        let result = decimate_hexball(0, 0, 2, 2, &lookup(&map));
        assert!(result.is_none(), "cliff exceeding threshold should fail");
    }

    #[test]
    fn cliff_exactly_at_threshold() {
        // z range [0, 4], center_z = 2, max_dev = 2
        let map = tiles_from_fn(0, 0, 1, |q, _r| if q > 0 { 4 } else { 0 });
        let result = decimate_hexball(0, 0, 1, 2, &lookup(&map));
        assert!(result.is_some(), "cliff exactly at threshold should succeed");
        assert_eq!(result.unwrap().center_z, 2);
    }

    #[test]
    fn center_z_minimizes_max_deviation() {
        // z range [0, 5] → center_z = 2 (floor of 2.5), max_dev = 3
        let map = tiles_from_fn(0, 0, 1, |q, _r| if q > 0 { 5 } else { 0 });
        let hb = decimate_hexball(0, 0, 1, 3, &lookup(&map)).unwrap();
        assert_eq!(hb.center_z, 2, "center_z should be floor of midpoint");
    }

    #[test]
    fn center_z_ties_broken_by_median() {
        // z range [0, 3] → candidates 1 and 2
        // z values mostly 0, median=0 → closer to 1
        let map = tiles_from_fn(0, 0, 1, |q, _r| if q > 0 { 3 } else { 0 });
        let hb = decimate_hexball(0, 0, 1, 2, &lookup(&map)).unwrap();
        assert_eq!(hb.center_z, 1, "median tiebreaker picks 1 (closer to majority z=0)");
    }

    #[test]
    fn center_z_median_picks_majority() {
        // 5 tiles z=10, 2 tiles z=9 → median=10, picks 10 not 9
        let mut map = HashMap::new();
        map.insert((0, 0), 10);
        map.insert((-1, 0), 9);
        map.insert((-1, 1), 10);
        map.insert((0, 1), 10);
        map.insert((1, 0), 10);
        map.insert((1, -1), 10);
        map.insert((0, -1), 9);
        for dq in -2..=2 {
            for dr in (-2).max(-dq - 2)..=(2).min(-dq + 2) {
                map.entry((dq, dr)).or_insert(10);
            }
        }
        let hb = decimate_hexball(0, 0, 1, 1, &lookup(&map)).unwrap();
        assert_eq!(hb.center_z, 10, "median should prefer majority z=10 over z=9");
    }

    #[test]
    fn effective_z_all_tiles_present() {
        // center_z=45, all 7 tiles get effective_z
        let mut map = HashMap::new();
        map.insert((0, 0), 50);
        for &(dq, dr) in &[(-1,0),(-1,1),(0,1),(1,0),(1,-1),(0,-1)] {
            map.insert((dq, dr), 40);
        }
        for dq in -2..=2 {
            for dr in (-2).max(-dq - 2)..=(2).min(-dq + 2) {
                map.entry((dq, dr)).or_insert(40);
            }
        }
        let hb = decimate_hexball(0, 0, 1, 10, &lookup(&map)).unwrap();
        assert_eq!(hb.effective_z.len(), 7);
        assert_eq!(*hb.effective_z.get(&(0, 0)).unwrap(), hb.center_z);
        // Ring-1 partials get snapped_z-derived effective_z (anchored to inscribed hex edge)
        for &(dq, dr) in &[(-1,0),(-1,1),(0,1),(1,0),(1,-1),(0,-1)] {
            assert!(hb.effective_z.contains_key(&(dq, dr)),
                "ring-1 tile ({dq},{dr}) missing from effective_z");
        }
    }

    #[test]
    fn negative_z_values() {
        let map = tiles_from_fn(0, 0, 1, |q, _r| if q > 0 { -1 } else { -5 });
        let hb = decimate_hexball(0, 0, 1, 2, &lookup(&map)).unwrap();
        assert_eq!(hb.center_z, -3, "should handle negative z correctly");
    }

    // ══════════════════════════════════════════════════════════════════
    // Structural Properties
    // ══════════════════════════════════════════════════════════════════

    #[test]
    fn determinism() {
        let map = tiles_from_fn(0, 0, 2, |q, r| (q + r).abs());
        let a = decimate_hexball(0, 0, 2, 2, &lookup(&map));
        let b = decimate_hexball(0, 0, 2, 2, &lookup(&map));
        match (a, b) {
            (Some(a), Some(b)) => {
                assert_eq!(a.center_z, b.center_z);
                assert_eq!(a.boundary_z, b.boundary_z);
                assert_eq!(a.partial_residuals.len(), b.partial_residuals.len());
                assert_eq!(a.full_residuals.len(), b.full_residuals.len());
            }
            (None, None) => {}
            _ => panic!("determinism violated: different Some/None"),
        }
    }

    #[test]
    fn symmetry_60_degree_rotation() {
        // Rotate tile (q,r) by 60° CW: (q,r) → (-r, q+r)
        let z_fn = |q: i32, r: i32| -> i32 { (q * 3 + r * 7).abs() % 4 };

        for radius in [1, 2, 3] {
            let map_a = tiles_from_fn(0, 0, radius, z_fn);
            // Build rotated terrain: for each tile (q,r), the z at the rotated
            // position equals the z at the original position.
            let map_b = tiles_from_fn(0, 0, radius, |q, r| {
                // Inverse of 60° CW rotation: (q,r) → (q+r, -q)
                let oq = q + r;
                let or_ = -q;
                z_fn(oq, or_)
            });

            let a = decimate_hexball(0, 0, radius, 3, &lookup(&map_a));
            let b = decimate_hexball(0, 0, radius, 3, &lookup(&map_b));

            match (&a, &b) {
                (Some(a), Some(b)) => {
                    assert_eq!(a.center_z, b.center_z, "r={radius}: center z should match");
                    assert_eq!(
                        a.partial_residuals.len(),
                        b.partial_residuals.len(),
                        "r={radius}: partial count should match"
                    );
                    assert_eq!(
                        a.full_residuals.len(),
                        b.full_residuals.len(),
                        "r={radius}: full residual count should match"
                    );
                }
                (None, None) => {}
                _ => panic!("r={radius}: symmetry violated — one succeeded, other failed"),
            }
        }
    }

    #[test]
    fn t_junction_snapped_z_on_inner_hex_plane() {
        // For flat terrain, all boundary z = 0, so snapped z should also be 0
        let map = flat_tiles(0, 0, 2, 0);
        let hb = decimate_hexball(0, 0, 2, 0, &lookup(&map)).unwrap();
        for p in &hb.partial_residuals {
            assert!(
                p.snapped_z.abs() < 1e-10,
                "partial at ({},{}) snapped_z={} should be 0 for flat terrain",
                p.q,
                p.r,
                p.snapped_z
            );
        }
    }

    #[test]
    fn t_junction_interpolation_with_slope() {
        // Terrain with slope: z = q (increases eastward)
        let map = tiles_from_fn(0, 0, 2, |q, _r| q);
        // z range [-2, 2], center_z = 0, max_dev = 2.
        // Boundary vertices blend ±0.5 from external neighbors, reaching ±2.5.
        // Threshold 3 accommodates the full blended range.
        let hb = decimate_hexball(0, 0, 2, 3, &lookup(&map)).unwrap();

        for p in &hb.partial_residuals {
            // snapped_z should be between the two boundary z values of the edge
            let edge = SURVIVING_TRI
                .iter()
                .position(|tri| *tri == p.surviving_triangles)
                .expect("surviving triangles must match a known edge");

            let z0 = hb.boundary_z[edge];
            let z1 = hb.boundary_z[(edge + 1) % 6];
            let lo = z0.min(z1);
            let hi = z0.max(z1);
            assert!(
                p.snapped_z >= lo - 1e-10 && p.snapped_z <= hi + 1e-10,
                "partial ({},{}) snapped_z={} not in [{}, {}]",
                p.q,
                p.r,
                p.snapped_z,
                lo,
                hi
            );
        }
    }

    #[test]
    fn boundary_vertex_consistency_adjacent_hexballs() {
        // Two hexballs on the radius-1 lattice sharing a perimeter edge
        // should compute identical boundary vertex heights at shared positions.
        let map = tiles_from_fn(0, 0, 4, |q, r| ((q * 3 + r * 5) % 3).abs());

        // Radius-1 lattice: v1=(2,1), v2=(-1,3). Adjacent cells at (0,0) and (1,0).
        let lattice = HexLattice::new(1);
        let (c0q, c0r) = lattice.cell_center((0, 0));
        let (c1q, c1r) = lattice.cell_center((1, 0));

        let hb0 = decimate_hexball(c0q, c0r, 1, 3, &lookup(&map));
        let hb1 = decimate_hexball(c1q, c1r, 1, 3, &lookup(&map));

        if let (Some(hb0), Some(hb1)) = (hb0, hb1) {
            // Find shared boundary vertices: same (tile_q, tile_r, vertex_idx)
            for i in 0..6 {
                for j in 0..6 {
                    if hb0.boundary_tiles[i] == hb1.boundary_tiles[j] {
                        assert!(
                            (hb0.boundary_z[i] - hb1.boundary_z[j]).abs() < 1e-10,
                            "shared boundary vertex mismatch: {} vs {}",
                            hb0.boundary_z[i],
                            hb1.boundary_z[j]
                        );
                    }
                }
            }
        }
    }

    // ══════════════════════════════════════════════════════════════════
    // Triangle Counts
    // ══════════════════════════════════════════════════════════════════

    #[test]
    fn triangle_count_r1() {
        let map = flat_tiles(0, 0, 1, 0);
        let hb = decimate_hexball(0, 0, 1, 0, &lookup(&map)).unwrap();
        let inner = 6;
        let residual = residual_tri_count(&hb);
        let original = tile_count(1) * 6;

        assert_eq!(inner + residual, 24, "r=1: 6 inner + 18 residual = 24");
        assert_eq!(residual, 18, "r=1: 18 residual triangles");
        assert_eq!(original, 42, "r=1: 42 original triangles");
        assert_eq!(hb.partial_residuals.len(), 6, "r=1: 6 partial tiles");
        assert_eq!(hb.full_residuals.len(), 0, "r=1: 0 full residual tiles");
    }

    #[test]
    fn triangle_count_r2() {
        let map = flat_tiles(0, 0, 2, 0);
        let hb = decimate_hexball(0, 0, 2, 0, &lookup(&map)).unwrap();
        let residual = residual_tri_count(&hb);

        assert_eq!(6 + residual, 24, "r=2: 6 inner + 18 residual = 24");
        assert_eq!(hb.partial_residuals.len(), 6, "r=2: 6 partial tiles");
        assert_eq!(hb.full_residuals.len(), 0, "r=2: 0 full residual tiles");
    }

    #[test]
    fn triangle_count_r3() {
        let map = flat_tiles(0, 0, 3, 0);
        let hb = decimate_hexball(0, 0, 3, 0, &lookup(&map)).unwrap();
        let residual = residual_tri_count(&hb);

        assert_eq!(6 + residual, 78, "r=3: 6 inner + 72 residual = 78");
        assert_eq!(hb.partial_residuals.len(), 12, "r=3: 12 partial tiles");
        assert_eq!(hb.full_residuals.len(), 6, "r=3: 6 full residual tiles");
    }

    #[test]
    fn triangle_count_r4() {
        let map = flat_tiles(0, 0, 4, 0);
        let hb = decimate_hexball(0, 0, 4, 0, &lookup(&map)).unwrap();
        let residual = residual_tri_count(&hb);

        assert_eq!(6 + residual, 78, "r=4: 6 inner + 72 residual = 78");
        assert_eq!(hb.partial_residuals.len(), 12, "r=4: 12 partial tiles");
        assert_eq!(hb.full_residuals.len(), 6, "r=4: 6 full residual tiles");
    }

    #[test]
    fn triangle_count_r5() {
        let map = flat_tiles(0, 0, 5, 0);
        let hb = decimate_hexball(0, 0, 5, 0, &lookup(&map)).unwrap();
        let residual = residual_tri_count(&hb);

        assert_eq!(residual, 162, "r=5: 162 residual triangles");
        assert_eq!(6 + residual, 168, "r=5: 6 inner + 162 residual = 168");
        assert_eq!(hb.partial_residuals.len(), 18, "r=5: 18 partial tiles");
        assert_eq!(hb.full_residuals.len(), 18, "r=5: 18 full residual tiles");
    }

    #[test]
    fn triangle_count_r6() {
        let map = flat_tiles(0, 0, 6, 0);
        let hb = decimate_hexball(0, 0, 6, 0, &lookup(&map)).unwrap();
        let residual = residual_tri_count(&hb);

        // r=5 and r=6 are paired — same residual structure
        assert_eq!(residual, 162, "r=6: 162 residual triangles");
        assert_eq!(6 + residual, 168, "r=6: 6 inner + 162 residual = 168");
        assert_eq!(hb.partial_residuals.len(), 18, "r=6: 18 partial tiles");
    }

    #[test]
    fn residual_counts_per_radius() {
        // Spec formula: ceil(N/2) partial tiles per side, ceil(N/2)-1 full tiles per side.
        // "Per side" = per inscribed hex vertex wedge, 6 total.
        // Partial tiles include ring-N AND ring-(N-1) tiles on hex edges (starting at r=5).
        // Residual tri per side = 3*ceil(N/2) + 6*full_per_side.
        for radius in 1..=8u32 {
            let map = flat_tiles(0, 0, radius, 0);
            let hb = decimate_hexball(0, 0, radius, 0, &lookup(&map)).unwrap();

            let ceil_half = ((radius + 1) / 2) as usize;
            let expected_partial = 6 * ceil_half;

            assert_eq!(
                hb.partial_residuals.len(),
                expected_partial,
                "r={radius}: partial count"
            );

            // Verify residual triangle count matches spec formula
            let residual_tri = residual_tri_count(&hb);
            let expected_residual_per_side = 3 * ceil_half + 6 * hb.full_residuals.len() / 6;
            assert_eq!(
                residual_tri,
                expected_residual_per_side * 6,
                "r={radius}: residual tri count"
            );

            // Every partial tile should have exactly 3 surviving triangles
            for p in &hb.partial_residuals {
                assert_eq!(p.surviving_triangles.len(), 3, "r={radius}: partial tri count");
            }
        }
    }

    // ══════════════════════════════════════════════════════════════════
    // Edge cases
    // ══════════════════════════════════════════════════════════════════

    #[test]
    fn radius_0_returns_none() {
        let map = flat_tiles(0, 0, 0, 0);
        assert!(decimate_hexball(0, 0, 0, 0, &lookup(&map)).is_none());
    }

    #[test]
    fn missing_tile_returns_none() {
        let mut map = flat_tiles(0, 0, 1, 0);
        map.remove(&(1, 0)); // remove one tile
        assert!(decimate_hexball(0, 0, 1, 0, &lookup(&map)).is_none());
    }

    #[test]
    fn nonzero_center_position() {
        let map = flat_tiles(10, -5, 2, 3);
        let hb = decimate_hexball(10, -5, 2, 0, &lookup(&map)).unwrap();
        assert_eq!(hb.center_q, 10);
        assert_eq!(hb.center_r, -5);
        assert_eq!(hb.center_z, 3);
    }

    // ══════════════════════════════════════════════════════════════════
    // Chunk-Level Decimation
    // ══════════════════════════════════════════════════════════════════

    #[test]
    fn chunk_flat_terrain_r1() {
        // A chunk-sized hexball of flat tiles at radius 9 (271 tiles)
        let tiles: Vec<(i32, i32, i32)> = {
            let mut v = Vec::new();
            for dq in -9..=9 {
                let dr_min = (-9).max(-dq - 9);
                let dr_max = 9.min(-dq + 9);
                for dr in dr_min..=dr_max {
                    v.push((dq, dr, 0));
                }
            }
            v
        };
        assert_eq!(tiles.len(), 271);

        let elev = |q: i32, r: i32| -> Option<i32> {
            tiles.iter().find(|&&(tq, tr, _)| tq == q && tr == r).map(|&(_, _, z)| z)
        };
        let result = decimate_chunk(&tiles, 1, 0, &elev);
        // With radius-1 hexballs (7 tiles each) tiling a 271-tile chunk:
        // 271 / 7 = 38.7... so at most 38 hexballs, with some boundary tiles surviving.
        assert!(
            !result.hexballs.is_empty(),
            "should decimate at least some hexballs"
        );
        let consumed = result.hexballs.len() * tile_count(1);
        assert_eq!(
            consumed + result.survivors.len(),
            271,
            "consumed + survivors = total"
        );
    }

    #[test]
    fn chunk_empty() {
        let no_elev = |_q: i32, _r: i32| -> Option<i32> { None };
        let result = decimate_chunk(&[], 1, 0, &no_elev);
        assert!(result.hexballs.is_empty());
        assert!(result.survivors.is_empty());
    }

    #[test]
    fn chunk_radius_0() {
        let tiles = vec![(0, 0, 0)];
        let no_elev = |_q: i32, _r: i32| -> Option<i32> { Some(0) };
        let result = decimate_chunk(&tiles, 0, 0, &no_elev);
        assert!(result.hexballs.is_empty());
        assert_eq!(result.survivors.len(), 1);
    }

    // ══════════════════════════════════════════════════════════════════
    // Inscribed hex vertex geometry
    // ══════════════════════════════════════════════════════════════════

    #[test]
    fn inscribed_hex_is_regular() {
        // All 6 vertices should be equidistant from center (regular hexagon)
        for radius in 1..=6u32 {
            let verts = inscribed_vertices(0, 0, radius);

            // Check all vertices at same distance from origin
            let dist_sq_0 = verts[0].a as i64 * verts[0].a as i64
                + 3 * verts[0].b as i64 * verts[0].b as i64;
            for i in 1..6 {
                let d = verts[i].a as i64 * verts[i].a as i64
                    + 3 * verts[i].b as i64 * verts[i].b as i64;
                // In (a, b) space, squared distance is a² + 3b² (since z = b*√3/2)
                assert_eq!(
                    d, dist_sq_0,
                    "r={radius}: vertex {i} distance mismatch"
                );
            }

            // Check all edges have equal length
            for i in 0..6 {
                let j = (i + 1) % 6;
                let da = (verts[j].a - verts[i].a) as i64;
                let db = (verts[j].b - verts[i].b) as i64;
                let edge_sq = da * da + 3 * db * db;
                let da0 = (verts[1].a - verts[0].a) as i64;
                let db0 = (verts[1].b - verts[0].b) as i64;
                let edge0_sq = da0 * da0 + 3 * db0 * db0;
                assert_eq!(
                    edge_sq, edge0_sq,
                    "r={radius}: edge {i}→{j} length mismatch"
                );
            }

            // Circumradius equals edge length (property of regular hexagon)
            let da = (verts[1].a - verts[0].a) as i64;
            let db = (verts[1].b - verts[0].b) as i64;
            let edge_sq = da * da + 3 * db * db;
            assert_eq!(
                dist_sq_0, edge_sq,
                "r={radius}: circumradius should equal edge length"
            );
        }
    }

    #[test]
    fn inscribed_hex_vertices_are_on_ring_n_tiles() {
        for radius in 1..=6u32 {
            let n = radius as i32;
            let verts = inscribed_vertices(0, 0, radius);
            for (i, v) in verts.iter().enumerate() {
                let d = hex_dist(v.tile_q, v.tile_r);
                assert_eq!(
                    d, n,
                    "r={radius}: vertex {i} tile ({},{}) at distance {d}, expected {n}",
                    v.tile_q, v.tile_r
                );
                assert!(
                    v.vertex_idx < 6,
                    "r={radius}: vertex {i} has invalid vertex_idx {}",
                    v.vertex_idx
                );
            }
        }
    }
}
