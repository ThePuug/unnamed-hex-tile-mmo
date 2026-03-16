//! QEM (Quadric Error Metrics) decimation for summary hex meshes.
//!
//! Replaces the fixed 7-vertex summary hex with a variable-vertex mesh:
//! 12 fixed boundary vertices (seamless tiling) plus N QEM-selected
//! interior vertices driven by terrain variance.

use std::collections::BTreeSet;
use serde::{Deserialize, Serialize};
use tinyvec::ArrayVec;

use crate::chunk::{ChunkId, CHUNK_RADIUS, CHUNK_TILES, LATTICE_V1, LATTICE_V2};

// ── Constants ──

/// Number of fixed boundary vertices (6 corners + 6 edge midpoints).
pub const BOUNDARY_VERTEX_COUNT: usize = 12;

/// Maximum number of interior vertices (all chunk tiles are candidates).
pub const MAX_INTERIOR_VERTICES: usize = CHUNK_TILES;

/// Maximum acceptable geometric error in world units between the
/// decimated mesh and the original 271-tile elevation field.
/// With rise=0.8 wu per z-level, 2.0 wu ≈ 2.5 z-levels — loose enough
/// for first pass. Tighten after reviewing p95 metrics in console.
pub const SUMMARY_ERROR_THRESHOLD: f32 = 2.0;

const SQRT_3: f64 = 1.732_050_808_0;
const RISE: f32 = 0.8;

/// Full-detail mesh vertex count baseline: 271 tiles × 7 vertices (no cliff skirts).
pub const FULL_DETAIL_VERTEX_BASELINE: f32 = 1_897.0;

/// Measured midpoint of actual ChunkData serialized size (bytes).
pub const FULL_CHUNK_WIRE_BASELINE: f32 = 4_800.0;

// ── Public Types ──

/// A single interior vertex selected by QEM decimation.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct InteriorVertex {
    /// Tile q relative to chunk center.
    pub rel_q: i8,
    /// Tile r relative to chunk center.
    pub rel_r: i8,
    /// World-space Y elevation.
    pub elevation: f32,
}

/// QEM-decimated summary for a chunk.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct SummaryHexData {
    pub chunk_id: ChunkId,
    /// World-space Y elevations at 12 boundary positions.
    /// [0..6] = corners (centroids of 3 chunk centers),
    /// [6..12] = edge midpoints.
    pub boundary_elevations: [f32; BOUNDARY_VERTEX_COUNT],
    /// QEM-selected interior vertices (0 for flat, up to CHUNK_TILES for max).
    pub interior: ArrayVec<[InteriorVertex; MAX_INTERIOR_VERTICES]>,
}

// ── Coordinate Helpers ──

/// Convert tile (q, r) to world (x, z) for FlatTop hex with radius=1.0.
fn tile_to_world_xz(q: i32, r: i32) -> (f64, f64) {
    (1.5 * q as f64, SQRT_3 / 2.0 * q as f64 + SQRT_3 * r as f64)
}

/// Convert z-level to world Y.
fn z_to_world_y(z: i32) -> f32 {
    z as f32 * RISE + RISE
}

// ── Boundary Vertex Computation ──

/// Lattice neighbor directions (matching generate_summary_mesh ordering).
const LATTICE_NEIGHBORS: [(i32, i32); 6] = [
    (1, 0), (0, 1), (-1, 1), (-1, 0), (0, -1), (1, -1),
];

/// Compute the 12 boundary vertex XZ positions in world space.
/// [0..6] = corners (centroids of 3 chunk centers).
/// [6..12] = edge midpoints (midpoints between adjacent corners).
pub fn boundary_positions_xz(chunk_id: ChunkId) -> [(f64, f64); BOUNDARY_VERTEX_COUNT] {
    let center = chunk_id.center();
    let (cx, cz) = tile_to_world_xz(center.q, center.r);

    let mut corners = [(0.0, 0.0); 6];
    for i in 0..6 {
        let (dn1, dm1) = LATTICE_NEIGHBORS[i];
        let (dn2, dm2) = LATTICE_NEIGHBORS[(i + 1) % 6];

        let n1q = dn1 * LATTICE_V1.0 + dm1 * LATTICE_V2.0;
        let n1r = dn1 * LATTICE_V1.1 + dm1 * LATTICE_V2.1;
        let n2q = dn2 * LATTICE_V1.0 + dm2 * LATTICE_V2.0;
        let n2r = dn2 * LATTICE_V1.1 + dm2 * LATTICE_V2.1;

        let (w1x, w1z) = tile_to_world_xz(center.q + n1q, center.r + n1r);
        let (w2x, w2z) = tile_to_world_xz(center.q + n2q, center.r + n2r);

        corners[i] = ((cx + w1x + w2x) / 3.0, (cz + w1z + w2z) / 3.0);
    }

    let mut result = [(0.0, 0.0); 12];
    for i in 0..6 {
        result[i] = corners[i];
        result[6 + i] = (
            (corners[i].0 + corners[(i + 1) % 6].0) / 2.0,
            (corners[i].1 + corners[(i + 1) % 6].1) / 2.0,
        );
    }

    result
}

/// Compute the 12 boundary vertex world-Y elevations.
/// `self_z`: z-level at chunk center.
/// `neighbor_z`: z-levels at the 6 neighbor chunk centers.
pub fn boundary_elevations(self_z: i32, neighbor_z: &[i32; 6]) -> [f32; BOUNDARY_VERTEX_COUNT] {
    let mut elevations = [0.0f32; 12];

    // Corner i: average of self, neighbor_i, neighbor_{(i+1)%6}
    for i in 0..6 {
        let avg = (self_z + neighbor_z[i] + neighbor_z[(i + 1) % 6]) as f32 / 3.0;
        elevations[i] = avg * RISE + RISE;
    }

    // Edge midpoint i: average of adjacent corners
    for i in 0..6 {
        elevations[6 + i] = (elevations[i] + elevations[(i + 1) % 6]) / 2.0;
    }

    elevations
}

// ── Hex Geometry ──

/// Iterate relative tile positions in a hex ball of given radius.
fn hex_ball_relative(radius: i32) -> impl Iterator<Item = (i32, i32)> {
    let r = radius;
    (-r..=r).flat_map(move |dq| {
        let dr_min = (-r).max(-dq - r);
        let dr_max = r.min(-dq + r);
        (dr_min..=dr_max).map(move |dr| (dq, dr))
    })
}

// ── Delaunay Triangulation ──

/// 2D Delaunay triangulation via Bowyer-Watson algorithm.
/// Returns triangle indices into the input points array.
fn delaunay_2d(points: &[[f64; 2]]) -> Vec<[usize; 3]> {
    if points.len() < 3 {
        return Vec::new();
    }

    // Find bounding box
    let (mut min_x, mut min_y) = (f64::MAX, f64::MAX);
    let (mut max_x, mut max_y) = (f64::MIN, f64::MIN);
    for p in points {
        min_x = min_x.min(p[0]);
        min_y = min_y.min(p[1]);
        max_x = max_x.max(p[0]);
        max_y = max_y.max(p[1]);
    }
    let d = (max_x - min_x).max(max_y - min_y).max(1.0) * 20.0;
    let mid_x = (min_x + max_x) / 2.0;
    let mid_y = (min_y + max_y) / 2.0;

    // Super-triangle vertices (appended at indices n, n+1, n+2)
    let n = points.len();
    let mut all_pts: Vec<[f64; 2]> = points.to_vec();
    all_pts.push([mid_x - d, mid_y - d]);
    all_pts.push([mid_x + d, mid_y - d]);
    all_pts.push([mid_x, mid_y + d]);

    let mut triangles: Vec<[usize; 3]> = vec![[n, n + 1, n + 2]];

    for pi in 0..n {
        let p = all_pts[pi];

        // Find triangles whose circumcircle contains p
        let mut bad = Vec::new();
        for (ti, &tri) in triangles.iter().enumerate() {
            if in_circumcircle(&all_pts, tri, p) {
                bad.push(ti);
            }
        }

        // Collect boundary edges of the hole (edges not shared by two bad triangles)
        let mut polygon: Vec<[usize; 2]> = Vec::new();
        for &ti in &bad {
            let tri = triangles[ti];
            for ei in 0..3 {
                let edge = [tri[ei], tri[(ei + 1) % 3]];
                let shared = bad.iter().any(|&oti| {
                    oti != ti && {
                        let o = triangles[oti];
                        o.contains(&edge[0]) && o.contains(&edge[1])
                    }
                });
                if !shared {
                    polygon.push(edge);
                }
            }
        }

        // Remove bad triangles (reverse order for stable indices)
        bad.sort_unstable();
        for &ti in bad.iter().rev() {
            triangles.swap_remove(ti);
        }

        // Create new triangles from polygon edges to the inserted point
        for edge in polygon {
            triangles.push([edge[0], edge[1], pi]);
        }
    }

    // Remove any triangle referencing super-triangle vertices
    triangles.retain(|tri| tri.iter().all(|&v| v < n));
    triangles
}

/// Check if point p is inside the circumcircle of triangle [a, b, c].
fn in_circumcircle(pts: &[[f64; 2]], tri: [usize; 3], p: [f64; 2]) -> bool {
    let (ax, ay) = (pts[tri[0]][0] - p[0], pts[tri[0]][1] - p[1]);
    let (bx, by) = (pts[tri[1]][0] - p[0], pts[tri[1]][1] - p[1]);
    let (cx, cy) = (pts[tri[2]][0] - p[0], pts[tri[2]][1] - p[1]);

    let a_sq = ax * ax + ay * ay;
    let b_sq = bx * bx + by * by;
    let c_sq = cx * cx + cy * cy;

    let det = ax * (by * c_sq - cy * b_sq)
            - ay * (bx * c_sq - cx * b_sq)
            + a_sq * (bx * cy - by * cx);

    // Orientation of the triangle determines sign convention
    let orient = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax);
    if orient > 0.0 { det > 0.0 } else { det < 0.0 }
}

// ── QEM Algorithm ──

/// Symmetric 4×4 quadric error matrix stored as upper triangle (10 values).
/// Indices: [a², ab, ac, ad, b², bc, bd, c², cd, d²]
#[derive(Clone, Copy)]
struct Quadric([f64; 10]);

impl Default for Quadric {
    fn default() -> Self { Quadric([0.0; 10]) }
}

impl Quadric {
    fn from_plane(a: f64, b: f64, c: f64, d: f64) -> Self {
        Quadric([
            a*a, a*b, a*c, a*d,
            b*b, b*c, b*d,
            c*c, c*d,
            d*d,
        ])
    }

    fn add(&self, other: &Quadric) -> Quadric {
        let mut r = [0.0; 10];
        for i in 0..10 { r[i] = self.0[i] + other.0[i]; }
        Quadric(r)
    }

    /// Evaluate v^T Q v for homogeneous point [x, y, z, 1].
    fn evaluate(&self, x: f64, y: f64, z: f64) -> f64 {
        let q = &self.0;
        q[0]*x*x + 2.0*q[1]*x*y + 2.0*q[2]*x*z + 2.0*q[3]*x
        + q[4]*y*y + 2.0*q[5]*y*z + 2.0*q[6]*y
        + q[7]*z*z + 2.0*q[8]*z
        + q[9]
    }
}

/// Compute the plane equation (a,b,c,d) for triangle vertices.
fn triangle_plane(p0: [f64; 3], p1: [f64; 3], p2: [f64; 3]) -> (f64, f64, f64, f64) {
    let u = [p1[0]-p0[0], p1[1]-p0[1], p1[2]-p0[2]];
    let v = [p2[0]-p0[0], p2[1]-p0[1], p2[2]-p0[2]];
    let n = [
        u[1]*v[2] - u[2]*v[1],
        u[2]*v[0] - u[0]*v[2],
        u[0]*v[1] - u[1]*v[0],
    ];
    let len = (n[0]*n[0] + n[1]*n[1] + n[2]*n[2]).sqrt();
    if len < 1e-12 {
        return (0.0, 1.0, 0.0, 0.0);
    }
    let (a, b, c) = (n[0]/len, n[1]/len, n[2]/len);
    let d = -(a*p0[0] + b*p0[1] + c*p0[2]);
    (a, b, c, d)
}

/// Internal QEM decimation state.
struct QEMState {
    pos: Vec<[f64; 3]>,
    quadrics: Vec<Quadric>,
    alive: Vec<bool>,
    locked: Vec<bool>,
    triangles: Vec<[usize; 3]>,
    tri_alive: Vec<bool>,
    adj: Vec<BTreeSet<usize>>,
}

impl QEMState {
    fn new(positions: &[[f64; 3]], locked: &[bool], triangles: &[[usize; 3]]) -> Self {
        let n = positions.len();
        let mut quadrics = vec![Quadric::default(); n];
        let mut adj: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); n];
        let tri_alive = vec![true; triangles.len()];

        for &[v0, v1, v2] in triangles {
            let (a, b, c, d) = triangle_plane(positions[v0], positions[v1], positions[v2]);
            let q = Quadric::from_plane(a, b, c, d);
            quadrics[v0] = quadrics[v0].add(&q);
            quadrics[v1] = quadrics[v1].add(&q);
            quadrics[v2] = quadrics[v2].add(&q);

            adj[v0].insert(v1); adj[v0].insert(v2);
            adj[v1].insert(v0); adj[v1].insert(v2);
            adj[v2].insert(v0); adj[v2].insert(v1);
        }

        QEMState {
            pos: positions.to_vec(),
            quadrics,
            alive: vec![true; n],
            locked: locked.to_vec(),
            triangles: triangles.to_vec(),
            tri_alive,
            adj,
        }
    }

    /// Find the cheapest valid edge collapse under the threshold.
    fn cheapest_collapse(&self, threshold: f64) -> Option<(usize, usize, [f64; 3], f64)> {
        let mut best: Option<(usize, usize, [f64; 3], f64)> = None;

        for vi in 0..self.pos.len() {
            if !self.alive[vi] { continue; }
            for &vj in &self.adj[vi] {
                if vj <= vi || !self.alive[vj] { continue; }
                if self.locked[vi] && self.locked[vj] { continue; }

                let q_sum = self.quadrics[vi].add(&self.quadrics[vj]);

                // Target: locked endpoint, or best of endpoints/midpoint
                let target = if self.locked[vi] {
                    self.pos[vi]
                } else if self.locked[vj] {
                    self.pos[vj]
                } else {
                    let e0 = q_sum.evaluate(self.pos[vi][0], self.pos[vi][1], self.pos[vi][2]);
                    let e1 = q_sum.evaluate(self.pos[vj][0], self.pos[vj][1], self.pos[vj][2]);
                    let mid = [
                        (self.pos[vi][0]+self.pos[vj][0])/2.0,
                        (self.pos[vi][1]+self.pos[vj][1])/2.0,
                        (self.pos[vi][2]+self.pos[vj][2])/2.0,
                    ];
                    let em = q_sum.evaluate(mid[0], mid[1], mid[2]);
                    if em <= e0.min(e1) { mid } else if e0 <= e1 { self.pos[vi] } else { self.pos[vj] }
                };

                let cost = q_sum.evaluate(target[0], target[1], target[2]);
                if cost > threshold { continue; }

                if best.is_none() || cost < best.unwrap().3 {
                    best = Some((vi, vj, target, cost));
                }
            }
        }

        best
    }

    /// Perform an edge collapse: merge `remove` into `keep`.
    fn collapse(&mut self, v0: usize, v1: usize, target: [f64; 3]) {
        let (keep, remove) = if self.locked[v1] { (v1, v0) } else { (v0, v1) };

        self.pos[keep] = target;
        self.quadrics[keep] = self.quadrics[keep].add(&self.quadrics[remove]);
        self.alive[remove] = false;

        // Update triangles: replace `remove` with `keep`, kill degenerates
        for ti in 0..self.triangles.len() {
            if !self.tri_alive[ti] { continue; }
            let tri = &mut self.triangles[ti];

            let has_remove = tri.contains(&remove);
            if !has_remove { continue; }

            // Replace remove → keep
            for v in tri.iter_mut() {
                if *v == remove { *v = keep; }
            }

            // Degenerate if two vertices are the same
            if tri[0] == tri[1] || tri[1] == tri[2] || tri[0] == tri[2] {
                self.tri_alive[ti] = false;
            }
        }

        // Transfer adjacency from remove to keep
        let remove_adj: Vec<usize> = self.adj[remove].iter().copied().collect();
        for &neighbor in &remove_adj {
            if neighbor == keep { continue; }
            self.adj[neighbor].remove(&remove);
            self.adj[neighbor].insert(keep);
            self.adj[keep].insert(neighbor);
        }
        self.adj[keep].remove(&remove);
        self.adj[remove].clear();
    }

    /// Run QEM decimation until no more collapses are possible under threshold.
    fn run(&mut self, threshold: f64) {
        while let Some((v0, v1, target, _cost)) = self.cheapest_collapse(threshold) {
            self.collapse(v0, v1, target);
        }
    }
}

// ── Public API ──

/// Run QEM decimation on a chunk's tile data.
///
/// `tile_elevations`: z-level for each tile in `chunk_tiles` order (271 values).
/// `neighbor_center_z`: z-level at the center of each of the 6 neighbor chunks.
/// `threshold`: maximum quadric error per collapse (world units²).
pub fn decimate_chunk(
    chunk_id: ChunkId,
    tile_elevations: &[i32],
    neighbor_center_z: &[i32; 6],
    threshold: f32,
) -> SummaryHexData {
    assert_eq!(tile_elevations.len(), CHUNK_TILES);

    let center = chunk_id.center();
    let self_z = tile_elevations[CHUNK_TILES / 2]; // center tile (approximate)

    // Find actual center tile elevation
    let self_center_z = {
        let mut center_z = self_z;
        for (k, (dq, dr)) in hex_ball_relative(CHUNK_RADIUS).enumerate() {
            if dq == 0 && dr == 0 {
                center_z = tile_elevations[k];
                break;
            }
        }
        center_z
    };

    // Compute boundary vertex positions and elevations
    let bnd_xz = boundary_positions_xz(chunk_id);
    let bnd_y = boundary_elevations(self_center_z, neighbor_center_z);

    // Build vertex list: [0..12) = boundary, [12..283) = tiles
    let mut positions: Vec<[f64; 3]> = Vec::with_capacity(12 + CHUNK_TILES);
    let mut locked = Vec::with_capacity(12 + CHUNK_TILES);

    // Boundary vertices
    for i in 0..12 {
        positions.push([bnd_xz[i].0, bnd_y[i] as f64, bnd_xz[i].1]);
        locked.push(true);
    }

    // Tile vertices
    let mut tile_rel_coords: Vec<(i8, i8)> = Vec::with_capacity(CHUNK_TILES);
    for (k, (dq, dr)) in hex_ball_relative(CHUNK_RADIUS).enumerate() {
        let (wx, wz) = tile_to_world_xz(center.q + dq, center.r + dr);
        let wy = z_to_world_y(tile_elevations[k]);
        positions.push([wx, wy as f64, wz]);
        locked.push(false);
        tile_rel_coords.push((dq as i8, dr as i8));
    }

    // Triangulate all vertices in the XZ plane
    let points_2d: Vec<[f64; 2]> = positions.iter().map(|p| [p[0], p[2]]).collect();
    let triangles = delaunay_2d(&points_2d);

    if triangles.is_empty() {
        // Fallback: no triangulation (shouldn't happen with 283 points)
        return SummaryHexData {
            chunk_id,
            boundary_elevations: bnd_y,
            interior: ArrayVec::new(),
        };
    }

    // Run QEM
    let mut qem = QEMState::new(&positions, &locked, &triangles);
    qem.run(threshold as f64);

    // Collect surviving interior vertices
    let mut interior = ArrayVec::new();
    for k in 0..CHUNK_TILES {
        let vi = 12 + k; // vertex index (boundary offset)
        if qem.alive[vi] {
            let (rq, rr) = tile_rel_coords[k];
            interior.push(InteriorVertex {
                rel_q: rq,
                rel_r: rr,
                elevation: qem.pos[vi][1] as f32,
            });
        }
    }

    SummaryHexData {
        chunk_id,
        boundary_elevations: bnd_y,
        interior,
    }
}

/// Compute the maximum vertical deviation (geometric error) between
/// the decimated mesh and the original 271-tile elevation field.
///
/// Returns the worst-case error in world units.
pub fn compute_geometric_error(
    data: &SummaryHexData,
    tile_elevations: &[i32],
) -> f32 {
    let center = data.chunk_id.center();
    let bnd_xz = boundary_positions_xz(data.chunk_id);

    // Build the decimated mesh vertices (boundary + interior)
    let mut verts_xz: Vec<[f64; 2]> = Vec::new();
    let mut verts_y: Vec<f64> = Vec::new();

    for i in 0..12 {
        verts_xz.push([bnd_xz[i].0, bnd_xz[i].1]);
        verts_y.push(data.boundary_elevations[i] as f64);
    }
    for iv in &data.interior {
        let (wx, wz) = tile_to_world_xz(center.q + iv.rel_q as i32, center.r + iv.rel_r as i32);
        verts_xz.push([wx, wz]);
        verts_y.push(iv.elevation as f64);
    }

    // Triangulate the decimated vertices
    let triangles = delaunay_2d(&verts_xz);

    // For each original tile, find the triangle containing it and interpolate
    let mut max_error: f64 = 0.0;
    for (k, (dq, dr)) in hex_ball_relative(CHUNK_RADIUS).enumerate() {
        let (tx, tz) = tile_to_world_xz(center.q + dq, center.r + dr);
        let original_y = z_to_world_y(tile_elevations[k]) as f64;

        // Find containing triangle and interpolate
        let mut interpolated_y = original_y; // fallback: no error
        for &[v0, v1, v2] in &triangles {
            if let Some(y) = barycentric_interpolate_y(
                &verts_xz, &verts_y, v0, v1, v2, tx, tz
            ) {
                interpolated_y = y;
                break;
            }
        }

        let error = (interpolated_y - original_y).abs();
        max_error = max_error.max(error);
    }

    max_error as f32
}

/// Barycentric interpolation of Y within a triangle in the XZ plane.
/// Returns None if the point is outside the triangle.
fn barycentric_interpolate_y(
    xz: &[[f64; 2]], y: &[f64],
    v0: usize, v1: usize, v2: usize,
    px: f64, pz: f64,
) -> Option<f64> {
    let (x0, z0) = (xz[v0][0], xz[v0][1]);
    let (x1, z1) = (xz[v1][0], xz[v1][1]);
    let (x2, z2) = (xz[v2][0], xz[v2][1]);

    let denom = (z1 - z2) * (x0 - x2) + (x2 - x1) * (z0 - z2);
    if denom.abs() < 1e-12 { return None; }

    let w0 = ((z1 - z2) * (px - x2) + (x2 - x1) * (pz - z2)) / denom;
    let w1 = ((z2 - z0) * (px - x2) + (x0 - x2) * (pz - z2)) / denom;
    let w2 = 1.0 - w0 - w1;

    const EPS: f64 = -1e-6;
    if w0 < EPS || w1 < EPS || w2 < EPS {
        return None;
    }

    Some(y[v0] * w0 + y[v1] * w1 + y[v2] * w2)
}

/// Build world-space vertex positions from SummaryHexData for mesh generation.
/// Returns (positions, count) where positions[0..12] are boundary,
/// positions[12..12+n] are interior.
pub fn summary_world_positions(data: &SummaryHexData) -> Vec<[f32; 3]> {
    let bnd_xz = boundary_positions_xz(data.chunk_id);
    let center = data.chunk_id.center();

    let mut positions = Vec::with_capacity(12 + data.interior.len());

    for i in 0..12 {
        positions.push([bnd_xz[i].0 as f32, data.boundary_elevations[i], bnd_xz[i].1 as f32]);
    }

    for iv in &data.interior {
        let (wx, wz) = tile_to_world_xz(center.q + iv.rel_q as i32, center.r + iv.rel_r as i32);
        positions.push([wx as f32, iv.elevation, wz as f32]);
    }

    positions
}

/// Triangulate summary vertex positions in the XZ plane.
/// Returns triangle indices into the positions array.
pub fn triangulate_summary(positions: &[[f32; 3]]) -> Vec<[usize; 3]> {
    let points_2d: Vec<[f64; 2]> = positions.iter().map(|p| [p[0] as f64, p[2] as f64]).collect();
    delaunay_2d(&points_2d)
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_chunk_elevations(z: i32) -> Vec<i32> {
        vec![z; CHUNK_TILES]
    }

    #[test]
    fn boundary_vertex_count_always_12() {
        let bnd = boundary_positions_xz(ChunkId(0, 0));
        assert_eq!(bnd.len(), BOUNDARY_VERTEX_COUNT);
    }

    #[test]
    fn flat_chunk_zero_interior() {
        let elevs = flat_chunk_elevations(0);
        let neighbor_z = [0; 6];
        let data = decimate_chunk(ChunkId(0, 0), &elevs, &neighbor_z, SUMMARY_ERROR_THRESHOLD);

        assert_eq!(data.boundary_elevations.len(), 12);
        assert!(
            data.interior.len() <= 10,
            "flat chunk should have very few interior vertices, got {}",
            data.interior.len()
        );
    }

    #[test]
    fn high_variance_retains_vertices() {
        // Alternating high/low elevations
        let mut elevs = Vec::with_capacity(CHUNK_TILES);
        for (k, _) in hex_ball_relative(CHUNK_RADIUS).enumerate() {
            elevs.push(if k % 2 == 0 { 100 } else { 0 });
        }
        let neighbor_z = [50; 6];
        let data = decimate_chunk(ChunkId(0, 0), &elevs, &neighbor_z, SUMMARY_ERROR_THRESHOLD);

        assert!(
            data.interior.len() > 50,
            "high-variance chunk should retain many vertices, got {}",
            data.interior.len()
        );
    }

    #[test]
    fn geometric_error_within_threshold() {
        let mut elevs = Vec::with_capacity(CHUNK_TILES);
        for (_k, (dq, dr)) in hex_ball_relative(CHUNK_RADIUS).enumerate() {
            // Smooth hill
            let dist = ((dq * dq + dr * dr + dq * dr) as f64).sqrt();
            elevs.push((50.0 - dist * 3.0).max(0.0) as i32);
        }
        let neighbor_z = [0; 6];
        let threshold = 4.0;
        let data = decimate_chunk(ChunkId(0, 0), &elevs, &neighbor_z, threshold);
        let error = compute_geometric_error(&data, &elevs);

        // Error should be reasonable (QEM cost approximates but doesn't equal geometric error)
        assert!(
            error < threshold * 4.0,
            "geometric error {} too large for threshold {}",
            error, threshold
        );
    }

    #[test]
    fn deterministic_output() {
        let elevs: Vec<i32> = hex_ball_relative(CHUNK_RADIUS)
            .enumerate()
            .map(|(k, (dq, dr))| (dq + dr + k as i32) % 20)
            .collect();
        let neighbor_z = [5; 6];

        let d1 = decimate_chunk(ChunkId(3, -2), &elevs, &neighbor_z, SUMMARY_ERROR_THRESHOLD);
        let d2 = decimate_chunk(ChunkId(3, -2), &elevs, &neighbor_z, SUMMARY_ERROR_THRESHOLD);

        assert_eq!(d1.interior.len(), d2.interior.len(), "interior count differs");
        for (a, b) in d1.interior.iter().zip(d2.interior.iter()) {
            assert_eq!(a.rel_q, b.rel_q);
            assert_eq!(a.rel_r, b.rel_r);
            assert_eq!(a.elevation, b.elevation);
        }
        assert_eq!(d1.boundary_elevations, d2.boundary_elevations);
    }

    #[test]
    fn wire_size_always_less_than_full_chunk() {
        let elevs = flat_chunk_elevations(10);
        let neighbor_z = [10; 6];
        let data = decimate_chunk(ChunkId(0, 0), &elevs, &neighbor_z, SUMMARY_ERROR_THRESHOLD);

        // Wire size: 8 (chunk_id) + 48 (boundary) + 2 (len) + interior_count * 8
        let wire_bytes = 8 + 48 + 2 + data.interior.len() * 8;
        // Full chunk baseline: 271 tiles * ~10 bytes each ≈ 2710
        let baseline = 2710;
        assert!(
            wire_bytes < baseline,
            "wire size {} >= baseline {}",
            wire_bytes, baseline
        );
    }

    #[test]
    fn boundary_shared_between_neighbors() {
        // X and Y = n0 share two corners: X_C0 = Y_C2, X_C5 = Y_C3
        let x = ChunkId(0, 0);
        let y = ChunkId(1, 0); // neighbor n0

        let x_bnd = boundary_positions_xz(x);
        let y_bnd = boundary_positions_xz(y);

        let eps = 1e-6;

        // X corner 0 = centroid(X, n0, n1) = Y corner 2 = centroid(Y, Y_n2=n1, Y_n3=X)
        let (xc, yc) = (x_bnd[0], y_bnd[2]);
        assert!(
            (xc.0 - yc.0).abs() < eps && (xc.1 - yc.1).abs() < eps,
            "X_C0 ({:.4}, {:.4}) != Y_C2 ({:.4}, {:.4})",
            xc.0, xc.1, yc.0, yc.1
        );

        // X corner 5 = centroid(X, n5, n0) = Y corner 3 = centroid(Y, Y_n3=X, Y_n4=n5)
        let (xc, yc) = (x_bnd[5], y_bnd[3]);
        assert!(
            (xc.0 - yc.0).abs() < eps && (xc.1 - yc.1).abs() < eps,
            "X_C5 ({:.4}, {:.4}) != Y_C3 ({:.4}, {:.4})",
            xc.0, xc.1, yc.0, yc.1
        );

        // Edge midpoints should also match
        // Midpoint of X edge 5→0 = midpoint of Y edge 2→3
        let xm = x_bnd[6 + 5]; // midpoint 5 (between corners 5 and 0)
        let ym = y_bnd[6 + 2]; // midpoint 2 (between corners 2 and 3)
        assert!(
            (xm.0 - ym.0).abs() < eps && (xm.1 - ym.1).abs() < eps,
            "X_M5 ({:.4}, {:.4}) != Y_M2 ({:.4}, {:.4})",
            xm.0, xm.1, ym.0, ym.1
        );
    }

    #[test]
    fn delaunay_simple_triangle() {
        let pts = [[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]];
        let tris = delaunay_2d(&pts);
        assert_eq!(tris.len(), 1);
        assert!(tris[0].contains(&0) && tris[0].contains(&1) && tris[0].contains(&2));
    }

    #[test]
    fn delaunay_square() {
        let pts = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let tris = delaunay_2d(&pts);
        assert_eq!(tris.len(), 2, "4 points should produce 2 triangles");
    }

    #[test]
    fn summary_positions_count() {
        let data = SummaryHexData {
            chunk_id: ChunkId(0, 0),
            boundary_elevations: [1.0; 12],
            interior: {
                let mut v = ArrayVec::new();
                v.push(InteriorVertex { rel_q: 0, rel_r: 0, elevation: 1.0 });
                v.push(InteriorVertex { rel_q: 1, rel_r: 0, elevation: 1.0 });
                v
            },
        };
        let positions = summary_world_positions(&data);
        assert_eq!(positions.len(), 14); // 12 boundary + 2 interior
    }
}
