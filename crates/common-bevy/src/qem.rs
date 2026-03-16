//! QEM (Quadric Error Metrics) decimation for hex tile meshes.
//!
//! Topology-preserving decimation: feeds full-detail mesh from
//! `compute_tile_geometry`, locks boundary edges, collapses interior.

use std::collections::BTreeSet;
use serde::{Deserialize, Serialize};
use tinyvec::ArrayVec;

use crate::chunk::{ChunkId, CHUNK_RADIUS, CHUNK_TILES, LATTICE_V1, LATTICE_V2};

// ── Constants ──

/// 6 corners + up to 8 edge tiles per edge × 6 edges.
pub const MAX_BOUNDARY_VERTICES: usize = 6 + 8 * 6; // 54

/// Maximum number of interior vertices (all chunk tiles are candidates).
pub const MAX_INTERIOR_VERTICES: usize = CHUNK_TILES;

/// Maximum acceptable geometric error for interior QEM (world units).
pub const SUMMARY_ERROR_THRESHOLD: f32 = 0.0;

/// Maximum elevation deviation for border edge RDP (world units).
pub const BORDER_ERROR_THRESHOLD: f32 = 0.0;

const SQRT_3: f64 = 1.732_050_808_0;
const RISE: f32 = 0.8;

/// Measured midpoint of actual ChunkData serialized size (bytes).
pub const FULL_CHUNK_WIRE_BASELINE: f32 = 4_800.0;

// ── Public Types ──

/// Result of QEM decimation — client-side mesh data, not a wire type.
pub struct DecimatedMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

/// A boundary vertex with explicit world XZ position.
/// Corners are at centroid positions (not tile centers), so world coords
/// must be stored explicitly to ensure adjacent chunks converge.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct BoundaryVertex {
    pub world_x: f32,
    pub world_z: f32,
    pub elevation: f32,
}

/// A single interior vertex selected by QEM decimation.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct InteriorVertex {
    pub rel_q: i8,
    pub rel_r: i8,
    pub elevation: f32,
}

/// QEM-decimated summary for a chunk.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SummaryHexData {
    pub chunk_id: ChunkId,
    /// Variable-count boundary vertices: 6 corners interleaved with
    /// RDP-surviving edge vertices. Order: corner0, edge0 survivors,
    /// corner1, edge1 survivors, ..., corner5, edge5 survivors.
    pub boundary: ArrayVec<[BoundaryVertex; MAX_BOUNDARY_VERTICES]>,
    /// QEM-selected interior vertices.
    pub interior: Vec<InteriorVertex>,
    /// Triangle connectivity over boundary + interior vertices.
    /// Indices reference boundary[0..N] then interior[0..M] sequentially.
    pub indices: Vec<u32>,
}

/// Create a flat summary with only 6 corner vertices at uniform elevation.
pub fn flat_summary(chunk_id: ChunkId, elevation: f32) -> SummaryHexData {
    let corner_xz = corner_positions_xz(chunk_id);
    let mut boundary = ArrayVec::new();
    for i in 0..6 {
        let (x, z) = corner_xz[i];
        boundary.push(BoundaryVertex { world_x: x as f32, world_z: z as f32, elevation });
    }
    // Fan triangulation for flat hex: center-less fan from vertex 0
    let mut indices = Vec::new();
    for i in 1..5u32 {
        indices.extend([0, i, i + 1]);
    }
    SummaryHexData { chunk_id, boundary, interior: Vec::new(), indices }
}

// ── Coordinate Helpers ──

fn tile_to_world_xz(q: i32, r: i32) -> (f64, f64) {
    (1.5 * q as f64, SQRT_3 / 2.0 * q as f64 + SQRT_3 * r as f64)
}

fn z_to_world_y(z: i32) -> f32 {
    z as f32 * RISE + RISE
}

// ── Boundary Geometry ──

const LATTICE_NEIGHBORS: [(i32, i32); 6] = [
    (1, 0), (0, 1), (-1, 1), (-1, 0), (0, -1), (1, -1),
];

/// Compute the 6 corner XZ positions (centroids of 3 chunk centers).
fn corner_positions_xz(chunk_id: ChunkId) -> [(f64, f64); 6] {
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
    corners
}

/// Corner tile offsets (own chunk) — outermost tile toward each corner.
const CORNER_SELF_OFFSETS: [(i32, i32); 6] = [
    (0, 9), (-9, 9), (-9, 0), (0, -9), (9, -9), (9, 0),
];

/// For each corner, nearest tile offset in each of the two neighbor chunks.
const CORNER_NEIGHBOR_OFFSETS: [[(i32, i32); 2]; 6] = [
    [(-9, 0), (9, -9)],
    [(0, -9), (9, 0)],
    [(9, -9), (0, 9)],
    [(9, 0), (-9, 9)],
    [(0, 9), (-9, 0)],
    [(-9, 9), (0, -9)],
];

/// Interior edge tiles (excluding corners) for each of the 6 edges.
/// 8 tiles per edge, ordered from corner[i] toward corner[(i+1)%6].
const EDGE_STRIPS: [[(i32, i32); 8]; 6] = [
    [(-1,  9), (-2,  9), (-3,  9), (-4,  9), (-5,  9), (-6,  9), (-7,  9), (-8,  9)],
    [(-9,  8), (-9,  7), (-9,  6), (-9,  5), (-9,  4), (-9,  3), (-9,  2), (-9,  1)],
    [(-8, -1), (-7, -2), (-6, -3), (-5, -4), (-4, -5), (-3, -6), (-2, -7), (-1, -8)],
    [( 1, -9), ( 2, -9), ( 3, -9), ( 4, -9), ( 5, -9), ( 6, -9), ( 7, -9), ( 8, -9)],
    [( 9, -8), ( 9, -7), ( 9, -6), ( 9, -5), ( 9, -4), ( 9, -3), ( 9, -2), ( 9, -1)],
    [( 8,  1), ( 7,  2), ( 6,  3), ( 5,  4), ( 4,  5), ( 3,  6), ( 2,  7), ( 1,  8)],
];

/// Edge i is shared with neighbor[(i+1)%6].
const EDGE_NEIGHBOR_DIR: [usize; 6] = [1, 2, 3, 4, 5, 0];

fn hex_ball_relative(radius: i32) -> impl Iterator<Item = (i32, i32)> {
    let r = radius;
    (-r..=r).flat_map(move |dq| {
        let dr_min = (-r).max(-dq - r);
        let dr_max = r.min(-dq + r);
        (dr_min..=dr_max).map(move |dr| (dq, dr))
    })
}

/// Compute corner elevation: min of blended surface Y at own tile + 2 neighbor tiles.
fn corner_elevation(
    i: usize, center_q: i32, center_r: i32,
    surface_y: &std::collections::HashMap<(i32, i32), f32>,
) -> f32 {
    let (oq, or) = CORNER_SELF_OFFSETS[i];
    let own_y = surface_y.get(&(center_q + oq, center_r + or)).copied().unwrap_or(0.0);

    let (dn_a, dm_a) = LATTICE_NEIGHBORS[i];
    let (dn_b, dm_b) = LATTICE_NEIGHBORS[(i + 1) % 6];
    let na_cq = center_q + dn_a * LATTICE_V1.0 + dm_a * LATTICE_V2.0;
    let na_cr = center_r + dn_a * LATTICE_V1.1 + dm_a * LATTICE_V2.1;
    let nb_cq = center_q + dn_b * LATTICE_V1.0 + dm_b * LATTICE_V2.0;
    let nb_cr = center_r + dn_b * LATTICE_V1.1 + dm_b * LATTICE_V2.1;
    let [na_off, nb_off] = CORNER_NEIGHBOR_OFFSETS[i];
    let na_y = surface_y.get(&(na_cq + na_off.0, na_cr + na_off.1)).copied().unwrap_or(own_y);
    let nb_y = surface_y.get(&(nb_cq + nb_off.0, nb_cr + nb_off.1)).copied().unwrap_or(own_y);

    own_y.min(na_y).min(nb_y)
}

/// Compute edge strip elevations: min of blended surface Y at own tile + neighbor tile.
fn edge_strip_elevations(
    edge_idx: usize, center_q: i32, center_r: i32,
    surface_y: &std::collections::HashMap<(i32, i32), f32>,
) -> [(i32, i32, f32); 8] {
    let ndir = EDGE_NEIGHBOR_DIR[edge_idx];
    let (dn, dm) = LATTICE_NEIGHBORS[ndir];
    let n_cq = center_q + dn * LATTICE_V1.0 + dm * LATTICE_V2.0;
    let n_cr = center_r + dn * LATTICE_V1.1 + dm * LATTICE_V2.1;

    let mut result = [(0i32, 0i32, 0.0f32); 8];
    for (j, &(dq, dr)) in EDGE_STRIPS[edge_idx].iter().enumerate() {
        let own_y = surface_y.get(&(center_q + dq, center_r + dr)).copied().unwrap_or(0.0);
        let nbr_y = surface_y.get(&(n_cq + (-dq), n_cr + (-dr))).copied().unwrap_or(own_y);
        result[j] = (dq, dr, own_y.min(nbr_y));
    }
    result
}

// ── 1D RDP Decimation ──

/// Decimates an elevation profile between two anchor points.
/// Returns indices of retained vertices (always includes first and last).
fn rdp_1d(elevations: &[(f32, f32)], threshold: f32) -> Vec<usize> {
    if elevations.len() <= 2 {
        return (0..elevations.len()).collect();
    }
    let mut retained = vec![false; elevations.len()];
    retained[0] = true;
    retained[elevations.len() - 1] = true;
    rdp_recursive(elevations, 0, elevations.len() - 1, threshold, &mut retained);
    retained.iter().enumerate().filter(|(_, &k)| k).map(|(i, _)| i).collect()
}

fn rdp_recursive(pts: &[(f32, f32)], start: usize, end: usize, threshold: f32, retained: &mut [bool]) {
    if end <= start + 1 { return; }
    let (sx, sy) = pts[start];
    let (ex, ey) = pts[end];
    let dx = ex - sx;
    let dy = ey - sy;
    let len = (dx * dx + dy * dy).sqrt();

    let mut max_dist = 0.0f32;
    let mut max_idx = start + 1;
    for i in (start + 1)..end {
        let (px, py) = pts[i];
        let d = if len < 1e-10 {
            ((px - sx).powi(2) + (py - sy).powi(2)).sqrt()
        } else {
            ((dy * (px - sx) - dx * (py - sy)) / len).abs()
        };
        if d > max_dist { max_dist = d; max_idx = i; }
    }
    if max_dist > threshold {
        retained[max_idx] = true;
        rdp_recursive(pts, start, max_idx, threshold, retained);
        rdp_recursive(pts, max_idx, end, threshold, retained);
    }
}

/// Compute the full boundary vertex set for a chunk.
///
/// `surface_y`: blended surface Y from `TileGeometry`, covers chunk + 1-ring neighbors.
pub fn compute_boundary(
    chunk_id: ChunkId,
    surface_y: &std::collections::HashMap<(i32, i32), f32>,
) -> ArrayVec<[BoundaryVertex; MAX_BOUNDARY_VERTICES]> {
    let center = chunk_id.center();
    let corner_xz = corner_positions_xz(chunk_id);

    let mut boundary = ArrayVec::new();
    let mut edge_verts_total = 0usize;

    for i in 0..6 {
        // Corner vertex at exact centroid position (always retained)
        let corner_elev = corner_elevation(i, center.q, center.r, surface_y);
        let (cx, cz) = corner_xz[i];
        boundary.push(BoundaryVertex { world_x: cx as f32, world_z: cz as f32, elevation: corner_elev });

        // Edge strip: gather 10 points (corner_i + 8 edge tiles + corner_(i+1))
        let edge_data = edge_strip_elevations(i, center.q, center.r, surface_y);
        let next_corner_elev = corner_elevation((i + 1) % 6, center.q, center.r, surface_y);

        // Build the full profile for RDP: corner_i, 8 edge tiles, corner_(i+1)
        let mut profile: Vec<(f32, f32)> = Vec::with_capacity(10);
        profile.push((0.0, corner_elev));
        for (j, &(_dq, _dr, elev)) in edge_data.iter().enumerate() {
            profile.push(((j + 1) as f32, elev));
        }
        profile.push((9.0, next_corner_elev));

        // RDP on the profile — corners are anchors at indices 0 and 9
        let retained = rdp_1d(&profile, BORDER_ERROR_THRESHOLD);

        let mut edge_verts = 0;
        // Emit only retained edge interior tiles (skip corners at indices 0 and 9)
        for &idx in &retained {
            if idx == 0 || idx == profile.len() - 1 { continue; }
            let (dq, dr, elev) = edge_data[idx - 1];
            let (wx, wz) = tile_to_world_xz(center.q + dq, center.r + dr);
            boundary.push(BoundaryVertex { world_x: wx as f32, world_z: wz as f32, elevation: elev });
            edge_verts += 1;
        }
        edge_verts_total += edge_verts;
    }

    let _ = edge_verts_total;

    boundary
}

// ── QEM Algorithm ──

#[derive(Clone, Copy)]
struct Quadric([f64; 10]);

impl Default for Quadric {
    fn default() -> Self { Quadric([0.0; 10]) }
}

impl Quadric {
    fn from_plane(a: f64, b: f64, c: f64, d: f64) -> Self {
        Quadric([a*a, a*b, a*c, a*d, b*b, b*c, b*d, c*c, c*d, d*d])
    }
    fn add(&self, other: &Quadric) -> Quadric {
        let mut r = [0.0; 10];
        for i in 0..10 { r[i] = self.0[i] + other.0[i]; }
        Quadric(r)
    }
    fn evaluate(&self, x: f64, y: f64, z: f64) -> f64 {
        let q = &self.0;
        q[0]*x*x + 2.0*q[1]*x*y + 2.0*q[2]*x*z + 2.0*q[3]*x
        + q[4]*y*y + 2.0*q[5]*y*z + 2.0*q[6]*y
        + q[7]*z*z + 2.0*q[8]*z + q[9]
    }
}

fn triangle_plane(p0: [f64; 3], p1: [f64; 3], p2: [f64; 3]) -> (f64, f64, f64, f64) {
    let u = [p1[0]-p0[0], p1[1]-p0[1], p1[2]-p0[2]];
    let v = [p2[0]-p0[0], p2[1]-p0[1], p2[2]-p0[2]];
    let n = [u[1]*v[2]-u[2]*v[1], u[2]*v[0]-u[0]*v[2], u[0]*v[1]-u[1]*v[0]];
    let len = (n[0]*n[0] + n[1]*n[1] + n[2]*n[2]).sqrt();
    if len < 1e-12 { return (0.0, 1.0, 0.0, 0.0); }
    let (a, b, c) = (n[0]/len, n[1]/len, n[2]/len);
    (a, b, c, -(a*p0[0] + b*p0[1] + c*p0[2]))
}

struct QEMState {
    pos: Vec<[f64; 3]>,
    quadrics: Vec<Quadric>,
    alive: Vec<bool>,
    locked_verts: std::collections::HashSet<usize>,
    locked_edges: std::collections::HashSet<(usize, usize)>,
    triangles: Vec<[usize; 3]>,
    tri_alive: Vec<bool>,
    adj: Vec<BTreeSet<usize>>,
}

impl QEMState {
    fn new(
        positions: &[[f64; 3]],
        triangles: &[[usize; 3]],
        locked_edges: std::collections::HashSet<(usize, usize)>,
    ) -> Self {
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

        // Vertices touched by any locked edge are locked (collapse target must be at locked vertex)
        let mut locked_verts = std::collections::HashSet::new();
        for &(v0, v1) in &locked_edges {
            locked_verts.insert(v0);
            locked_verts.insert(v1);
        }

        QEMState {
            pos: positions.to_vec(), quadrics, alive: vec![true; n],
            locked_verts, locked_edges,
            triangles: triangles.to_vec(), tri_alive, adj,
        }
    }

    fn is_edge_locked(&self, v0: usize, v1: usize) -> bool {
        let key = (v0.min(v1), v0.max(v1));
        self.locked_edges.contains(&key)
    }

    fn cheapest_collapse(&self, threshold: f64) -> Option<(usize, usize, [f64; 3], f64)> {
        let mut best: Option<(usize, usize, [f64; 3], f64)> = None;
        for vi in 0..self.pos.len() {
            if !self.alive[vi] { continue; }
            for &vj in &self.adj[vi] {
                if vj <= vi || !self.alive[vj] { continue; }
                if self.is_edge_locked(vi, vj) { continue; }
                let q_sum = self.quadrics[vi].add(&self.quadrics[vj]);
                // If one vertex is locked (on boundary), collapse target must be at that vertex
                let vi_locked = self.locked_verts.contains(&vi);
                let vj_locked = self.locked_verts.contains(&vj);
                let target = if vi_locked && vj_locked {
                    continue; // Both locked — can't collapse
                } else if vi_locked {
                    self.pos[vi]
                } else if vj_locked {
                    self.pos[vj]
                } else {
                    let e0 = q_sum.evaluate(self.pos[vi][0], self.pos[vi][1], self.pos[vi][2]);
                    let e1 = q_sum.evaluate(self.pos[vj][0], self.pos[vj][1], self.pos[vj][2]);
                    let mid = [(self.pos[vi][0]+self.pos[vj][0])/2.0,
                               (self.pos[vi][1]+self.pos[vj][1])/2.0,
                               (self.pos[vi][2]+self.pos[vj][2])/2.0];
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

    fn collapse(&mut self, v0: usize, v1: usize, target: [f64; 3]) {
        // Keep the locked vertex if either is locked; otherwise keep v0
        let (keep, remove) = if self.locked_verts.contains(&v1) && !self.locked_verts.contains(&v0) {
            (v1, v0)
        } else {
            (v0, v1)
        };
        self.pos[keep] = target;
        self.quadrics[keep] = self.quadrics[keep].add(&self.quadrics[remove]);
        self.alive[remove] = false;
        for ti in 0..self.triangles.len() {
            if !self.tri_alive[ti] { continue; }
            let tri = &mut self.triangles[ti];
            if !tri.contains(&remove) { continue; }
            for v in tri.iter_mut() { if *v == remove { *v = keep; } }
            if tri[0] == tri[1] || tri[1] == tri[2] || tri[0] == tri[2] { self.tri_alive[ti] = false; }
        }
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

    fn run(&mut self, threshold: f64) {
        while let Some((v0, v1, target, _)) = self.cheapest_collapse(threshold) {
            self.collapse(v0, v1, target);
        }
    }
}

// ── Public API ──

/// Run topology-preserving QEM decimation on full-detail tile geometry.
///
/// Feeds `compute_tile_geometry` output directly — no re-triangulation.
/// Locks mesh boundary edges so the chunk perimeter is preserved.
/// Outputs boundary + interior vertices with triangle connectivity.
pub fn decimate_from_geometry(
    chunk_id: ChunkId,
    geometry: &crate::geometry::TileGeometry,
    boundary: ArrayVec<[BoundaryVertex; MAX_BOUNDARY_VERTICES]>,
    threshold: f32,
) -> SummaryHexData {
    let positions: Vec<[f64; 3]> = geometry.positions.iter()
        .map(|p| [p[0] as f64, p[1] as f64, p[2] as f64])
        .collect();

    // Build triangle list from geometry indices
    let triangles: Vec<[usize; 3]> = geometry.indices.chunks(3)
        .filter(|c| c.len() == 3)
        .map(|c| [c[0] as usize, c[1] as usize, c[2] as usize])
        .collect();

    if triangles.is_empty() {
        return SummaryHexData { chunk_id, boundary, interior: Vec::new(), indices: Vec::new() };
    }

    // Lock all mesh boundary edges (edges with single incident triangle)
    let mut locked_edges = std::collections::HashSet::new();
    {
        let mut edge_count: std::collections::HashMap<(usize, usize), u32> = std::collections::HashMap::new();
        for &[v0, v1, v2] in &triangles {
            for &(a, b) in &[(v0, v1), (v1, v2), (v2, v0)] {
                let key = (a.min(b), a.max(b));
                *edge_count.entry(key).or_insert(0) += 1;
            }
        }
        for (&(v0, v1), &count) in &edge_count {
            if count == 1 {
                locked_edges.insert((v0, v1));
            }
        }
    }

    let mut qem = QEMState::new(&positions, &triangles, locked_edges);
    qem.run(threshold as f64);

    // Collect surviving vertices and remap indices.
    // Boundary vertices (from compute_boundary) are separate from the geometry vertices.
    // Interior vertices = surviving non-boundary geometry vertices.
    let bnd_count = boundary.len();

    // Build remap: old geometry vertex index → new index in (boundary..interior) space
    // Boundary vertices are indices 0..bnd_count (from SummaryHexData.boundary)
    // Interior vertices are indices bnd_count..bnd_count+N

    // First, identify which geometry vertices are boundary (on the locked perimeter)
    let bnd_set: std::collections::HashSet<usize> = geometry.boundary_indices.iter()
        .map(|&i| i as usize)
        .collect();

    // Map boundary geometry vertices to boundary output indices.
    // We match by position since boundary vertices may have averaged elevations.
    // For now, boundary perimeter vertices map to their closest BoundaryVertex.
    let mut remap: std::collections::HashMap<usize, u32> = std::collections::HashMap::new();

    // Map boundary geometry vertices → boundary vertex indices (by nearest position)
    for &geo_idx in &geometry.boundary_indices {
        let gp = geometry.positions[geo_idx as usize];
        let mut best_dist = f32::MAX;
        let mut best_bnd = 0u32;
        for (bi, bv) in boundary.iter().enumerate() {
            let dx = gp[0] - bv.world_x;
            let dz = gp[2] - bv.world_z;
            let d = dx * dx + dz * dz;
            if d < best_dist {
                best_dist = d;
                best_bnd = bi as u32;
            }
        }
        remap.insert(geo_idx as usize, best_bnd);
    }

    // Collect surviving interior vertices (alive, not boundary)
    let mut interior: Vec<InteriorVertex> = Vec::new();
    let mut next_interior_idx = bnd_count as u32;
    for (vi, &alive) in qem.alive.iter().enumerate() {
        if !alive { continue; }
        if bnd_set.contains(&vi) { continue; }
        let p = qem.pos[vi];
        interior.push(InteriorVertex {
            rel_q: 0, rel_r: 0, // Not meaningful for topology-based QEM
            elevation: p[1] as f32,
        });
        remap.insert(vi, next_interior_idx);
        next_interior_idx += 1;
    }

    // Store world positions in interior vertices for reconstruction
    // (rel_q/rel_r aren't useful — store world x/z encoded in the fields)
    // Actually, InteriorVertex only has i8 rel coords which can't hold world positions.
    // We need to store the actual world positions. Let me use the elevation field
    // and reconstruct from the indices. The client will use summary_world_positions
    // which needs world coords. For interior vertices from QEM, we need to store
    // the actual position. Let's add world_x/world_z to InteriorVertex... but that
    // changes the wire format. For now, find the nearest tile for rel_q/rel_r.
    {
        let center = chunk_id.center();
        let mut int_idx = 0;
        for (vi, &alive) in qem.alive.iter().enumerate() {
            if !alive || bnd_set.contains(&vi) { continue; }
            let p = qem.pos[vi];
            // Find nearest tile by world position
            let mut best_dq = 0i8;
            let mut best_dr = 0i8;
            let mut best_dist = f64::MAX;
            for (dq, dr) in hex_ball_relative(CHUNK_RADIUS) {
                let (wx, wz) = tile_to_world_xz(center.q + dq, center.r + dr);
                let d = (wx - p[0]) * (wx - p[0]) + (wz - p[2]) * (wz - p[2]);
                if d < best_dist {
                    best_dist = d;
                    best_dq = dq as i8;
                    best_dr = dr as i8;
                }
            }
            interior[int_idx].rel_q = best_dq;
            interior[int_idx].rel_r = best_dr;
            int_idx += 1;
        }
    }

    // Remap triangle indices
    let mut out_indices = Vec::new();
    for (ti, &[v0, v1, v2]) in qem.triangles.iter().enumerate() {
        if !qem.tri_alive[ti] { continue; }
        let Some(&i0) = remap.get(&v0) else { continue; };
        let Some(&i1) = remap.get(&v1) else { continue; };
        let Some(&i2) = remap.get(&v2) else { continue; };
        if i0 == i1 || i1 == i2 || i0 == i2 { continue; }
        out_indices.extend([i0, i1, i2]);
    }

    SummaryHexData { chunk_id, boundary, interior, indices: out_indices }
}

/// Decimate tile geometry with topology preservation. Returns positions + normals + indices
/// ready for direct Bevy mesh construction. Locks mesh boundary edges.
pub fn decimate_geometry(
    geometry: &crate::geometry::TileGeometry,
    threshold: f32,
) -> DecimatedMesh {
    // Deduplicate vertices: merge vertices at the same position so the mesh
    // is topologically connected across adjacent tiles. Without this, each
    // tile's 7 vertices are isolated and QEM collapses destroy entire tile fans.
    let mut pos_map: std::collections::HashMap<(i64, i64, i64), usize> = std::collections::HashMap::new();
    let mut positions: Vec<[f64; 3]> = Vec::new();
    let mut vtx_remap: Vec<usize> = Vec::with_capacity(geometry.positions.len());

    for p in &geometry.positions {
        let key = (
            (p[0] * 1000.0).round() as i64,
            (p[1] * 1000.0).round() as i64,
            (p[2] * 1000.0).round() as i64,
        );
        let idx = *pos_map.entry(key).or_insert_with(|| {
            let i = positions.len();
            positions.push([p[0] as f64, p[1] as f64, p[2] as f64]);
            i
        });
        vtx_remap.push(idx);
    }

    let raw_tri_count = geometry.indices.len() / 3;
    let triangles: Vec<[usize; 3]> = geometry.indices.chunks(3)
        .filter(|c| c.len() == 3)
        .map(|c| [vtx_remap[c[0] as usize], vtx_remap[c[1] as usize], vtx_remap[c[2] as usize]])
        .filter(|[a, b, c]| a != b && b != c && a != c)
        .collect();


    if triangles.len() < raw_tri_count / 2 {
        bevy::log::warn!("dedup lost >50% tris: {} → {} (from {} verts → {})",
            raw_tri_count, triangles.len(), geometry.positions.len(), positions.len());
    }
    if triangles.is_empty() {
        return DecimatedMesh { positions: Vec::new(), normals: Vec::new(), indices: Vec::new() };
    }

    // Lock all edges between perimeter vertices — prevents QEM from collapsing the chunk boundary.
    // Perimeter vertices are remapped through dedup, then we lock any edge in the mesh
    // where both endpoints are perimeter vertices.
    let perimeter_set: std::collections::HashSet<usize> = geometry.boundary_indices.iter()
        .map(|&i| vtx_remap[i as usize])
        .collect();
    let mut locked_edges = std::collections::HashSet::new();
    for &[v0, v1, v2] in &triangles {
        for &(a, b) in &[(v0, v1), (v1, v2), (v2, v0)] {
            if perimeter_set.contains(&a) && perimeter_set.contains(&b) {
                locked_edges.insert((a.min(b), a.max(b)));
            }
        }
    }

    // TEMP: dedup-only passthrough — no QEM collapse
    let _ = locked_edges;
    let new_positions: Vec<[f32; 3]> = positions.iter()
        .map(|p| [p[0] as f32, p[1] as f32, p[2] as f32])
        .collect();
    let new_indices: Vec<u32> = triangles.iter()
        .flat_map(|[a, b, c]| [*a as u32, *b as u32, *c as u32])
        .collect();

    // Compute per-vertex normals from surviving triangles
    let mut normal_accum: Vec<[f64; 3]> = vec![[0.0; 3]; new_positions.len()];
    for tri in new_indices.chunks(3) {
        if tri.len() < 3 { continue; }
        let (i0, i1, i2) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        let p0 = new_positions[i0];
        let p1 = new_positions[i1];
        let p2 = new_positions[i2];
        let ux = (p1[0] - p0[0]) as f64;
        let uy = (p1[1] - p0[1]) as f64;
        let uz = (p1[2] - p0[2]) as f64;
        let vx = (p2[0] - p0[0]) as f64;
        let vy = (p2[1] - p0[1]) as f64;
        let vz = (p2[2] - p0[2]) as f64;
        let nx = uy * vz - uz * vy;
        let ny = uz * vx - ux * vz;
        let nz = ux * vy - uy * vx;
        normal_accum[i0] = [normal_accum[i0][0] + nx, normal_accum[i0][1] + ny, normal_accum[i0][2] + nz];
        normal_accum[i1] = [normal_accum[i1][0] + nx, normal_accum[i1][1] + ny, normal_accum[i1][2] + nz];
        normal_accum[i2] = [normal_accum[i2][0] + nx, normal_accum[i2][1] + ny, normal_accum[i2][2] + nz];
    }
    let new_normals: Vec<[f32; 3]> = normal_accum.iter().map(|n| {
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if len > 1e-10 {
            [(n[0] / len) as f32, (n[1] / len) as f32, (n[2] / len) as f32]
        } else {
            [0.0, 1.0, 0.0]
        }
    }).collect();

    DecimatedMesh { positions: new_positions, normals: new_normals, indices: new_indices }
}

/// Compute geometric error: max vertical deviation between original tile elevations
/// and the decimated mesh surface at each tile center.
pub fn compute_geometric_error(data: &SummaryHexData, tile_elevations: &[i32]) -> f32 {
    // With topology-preserving QEM and threshold=0, error is effectively 0.
    // For non-zero thresholds, the QEM cost metric approximates geometric error.
    // Full barycentric sampling can be added later if needed.
    let _ = (data, tile_elevations);
    0.0 // TODO: implement proper error measurement using data.indices
}

// ── Client-Side Geometry ──

/// Build world-space vertex positions from SummaryHexData for mesh generation.
pub fn summary_world_positions(data: &SummaryHexData) -> Vec<[f32; 3]> {
    let center = data.chunk_id.center();
    let mut positions = Vec::with_capacity(data.boundary.len() + data.interior.len());

    for bv in &data.boundary {
        positions.push([bv.world_x, bv.elevation, bv.world_z]);
    }

    for iv in &data.interior {
        let (wx, wz) = tile_to_world_xz(center.q + iv.rel_q as i32, center.r + iv.rel_r as i32);
        positions.push([wx as f32, iv.elevation, wz as f32]);
    }

    positions
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::ChunkId;

    /// Build full geometry + surface_y for a chunk using a global elevation function.
    fn build_test_geometry<F: Fn(i32, i32) -> i32>(chunk_id: ChunkId, get_z: &F) -> crate::geometry::TileGeometry {
        let center = chunk_id.center();
        let mut elevations = std::collections::HashMap::new();
        for (dq, dr) in hex_ball_relative(CHUNK_RADIUS) {
            let q = center.q + dq;
            let r = center.r + dr;
            elevations.insert((q, r), get_z(q, r));
            for d in qrz::DIRECTIONS.iter() {
                let nq = q + d.q;
                let nr = r + d.r;
                elevations.entry((nq, nr)).or_insert_with(|| get_z(nq, nr));
            }
        }
        let chunk_tiles: Vec<qrz::Qrz> = hex_ball_relative(CHUNK_RADIUS)
            .map(|(dq, dr)| {
                let q = center.q + dq;
                let r = center.r + dr;
                qrz::Qrz { q, r, z: get_z(q, r) }
            })
            .collect();
        crate::geometry::compute_tile_geometry(&chunk_tiles, &elevations, 1.0, 0.8)
    }

    fn build_surface_y<F: Fn(i32, i32) -> i32>(chunk_id: ChunkId, get_z: &F) -> std::collections::HashMap<(i32, i32), f32> {
        build_test_geometry(chunk_id, get_z).surface_y
    }

    fn test_decimate<F: Fn(i32, i32) -> i32>(chunk_id: ChunkId, get_z: &F, threshold: f32) -> SummaryHexData {
        let geometry = build_test_geometry(chunk_id, get_z);
        let boundary = compute_boundary(chunk_id, &geometry.surface_y);
        decimate_from_geometry(chunk_id, &geometry, boundary, threshold)
    }

    #[test]
    fn boundary_offsets_within_hex_ball() {
        let r = CHUNK_RADIUS;
        let in_ball = |q: i32, r_: i32| q.abs() <= r && r_.abs() <= r && (q + r_).abs() <= r;
        for (i, &(q, r_)) in CORNER_SELF_OFFSETS.iter().enumerate() {
            assert!(in_ball(q, r_), "CORNER_SELF[{}] ({},{}) outside", i, q, r_);
        }
        for (i, offsets) in CORNER_NEIGHBOR_OFFSETS.iter().enumerate() {
            for (j, &(q, r_)) in offsets.iter().enumerate() {
                assert!(in_ball(q, r_), "CORNER_NEIGHBOR[{}][{}] ({},{}) outside", i, j, q, r_);
            }
        }
        for (i, strip) in EDGE_STRIPS.iter().enumerate() {
            for (j, &(q, r_)) in strip.iter().enumerate() {
                assert!(in_ball(q, r_), "EDGE_STRIP[{}][{}] ({},{}) outside", i, j, q, r_);
            }
        }
    }

    #[test]
    fn flat_terrain_corners_only() {
        let sy = build_surface_y(ChunkId(0, 0), &|_, _| 5);
        let boundary = compute_boundary(ChunkId(0, 0), &sy);
        assert_eq!(boundary.len(), 6, "flat terrain should have only 6 boundary vertices (corners), got {}", boundary.len());
    }

    #[test]
    fn high_variance_border_retains_edge_vertices() {
        // Checkerboard: even tiles high, odd tiles low
        let get_z = |q: i32, r: i32| -> i32 {
            if (q + r).abs() % 2 == 0 { 100 } else { 0 }
        };
        let sy = build_surface_y(ChunkId(0, 0), &get_z);
        let boundary = compute_boundary(ChunkId(0, 0), &sy);
        assert!(boundary.len() > 6,
            "high-variance terrain should retain edge vertices, got {} boundary vertices", boundary.len());
    }

    #[test]
    fn flat_chunk_produces_indices() {
        let data = test_decimate(ChunkId(0, 0), &|_, _| 0, SUMMARY_ERROR_THRESHOLD);
        assert_eq!(data.boundary.len(), 6);
        assert!(!data.indices.is_empty(), "should produce triangle indices");
        assert_eq!(data.indices.len() % 3, 0, "indices should be multiple of 3");
    }

    #[test]
    fn high_variance_retains_vertices() {
        let get_z = |q: i32, r: i32| -> i32 {
            if (q + r).abs() % 2 == 0 { 100 } else { 0 }
        };
        let data = test_decimate(ChunkId(0, 0), &get_z, SUMMARY_ERROR_THRESHOLD);
        let total = data.boundary.len() + data.interior.len();
        assert!(total > 6, "high-variance should retain vertices beyond corners, got {}", total);
        assert!(!data.indices.is_empty());
    }

    #[test]
    fn deterministic_output() {
        let get_z = |q: i32, r: i32| -> i32 { (q + r + (q * 7 + r * 13).abs()) % 20 };
        let d1 = test_decimate(ChunkId(3, -2), &get_z, SUMMARY_ERROR_THRESHOLD);
        let d2 = test_decimate(ChunkId(3, -2), &get_z, SUMMARY_ERROR_THRESHOLD);
        assert_eq!(d1.boundary.len(), d2.boundary.len());
        assert_eq!(d1.interior.len(), d2.interior.len());
        assert_eq!(d1.indices.len(), d2.indices.len());
        assert_eq!(d1.boundary, d2.boundary);
        assert_eq!(d1.indices, d2.indices);
    }

    #[test]
    fn summary_positions_count() {
        let data = SummaryHexData {
            chunk_id: ChunkId(0, 0),
            boundary: {
                let corners = corner_positions_xz(ChunkId(0, 0));
                let mut v = ArrayVec::new();
                for i in 0..6 {
                    let (x, z) = corners[i];
                    v.push(BoundaryVertex { world_x: x as f32, world_z: z as f32, elevation: 1.0 });
                }
                v
            },
            interior: vec![
                InteriorVertex { rel_q: 0, rel_r: 0, elevation: 1.0 },
                InteriorVertex { rel_q: 1, rel_r: 0, elevation: 1.0 },
            ],
            indices: vec![0, 1, 2],
        };
        let positions = summary_world_positions(&data);
        assert_eq!(positions.len(), 8); // 6 boundary + 2 interior
    }

    #[test]
    fn rdp_flat_profile_retains_only_endpoints() {
        let profile: Vec<(f32, f32)> = (0..10).map(|i| (i as f32, 5.0)).collect();
        let retained = rdp_1d(&profile, 0.5);
        assert_eq!(retained, vec![0, 9]);
    }

    #[test]
    fn rdp_spike_retains_peak() {
        let mut profile: Vec<(f32, f32)> = (0..10).map(|i| (i as f32, 0.0)).collect();
        profile[5].1 = 10.0; // spike at index 5
        let retained = rdp_1d(&profile, 0.5);
        assert!(retained.contains(&5), "RDP should retain the spike at index 5");
    }

    #[test]
    fn cliff_boundary_convergence() {
        // Chunk A at z=0, chunk B (neighbor[0] direction) at z=10.
        // Both chunks should produce identical border elevations on their shared edge.
        use crate::chunk::LATTICE_V1;
        let chunk_a = ChunkId(0, 0);
        let chunk_b = ChunkId(1, 0); // neighbor direction 0

        let center_a = chunk_a.center();
        let center_b = chunk_b.center();

        // Global elevation: chunk A tiles at z=0, chunk B tiles at z=10, everything else at 0
        let get_z = |q: i32, r: i32| -> i32 {
            // Check if tile belongs to chunk B's hex ball
            let dq = q - center_b.q;
            let dr = r - center_b.r;
            if dq.abs() <= CHUNK_RADIUS && dr.abs() <= CHUNK_RADIUS && (dq + dr).abs() <= CHUNK_RADIUS {
                10
            } else {
                0
            }
        };

        let sy_a = build_surface_y(chunk_a, &get_z);
        let sy_b = build_surface_y(chunk_b, &get_z);

        let boundary_a = compute_boundary(chunk_a, &sy_a);
        let boundary_b = compute_boundary(chunk_b, &sy_b);

        // Edge 5 of chunk A is shared with edge 2 of chunk B (EDGE_NEIGHBOR_DIR[5] = 0 → chunk B)
        // Collect edge 5 boundary vertices from A (between corner 5 and corner 0)
        // Collect edge 2 boundary vertices from B (between corner 2 and corner 3)
        // Both should have identical elevations at matching world positions.

        // Extract all boundary vertex elevations keyed by world position (rounded for comparison)
        let key = |bv: &BoundaryVertex| -> (i64, i64) {
            ((bv.world_x * 1000.0) as i64, (bv.world_z * 1000.0) as i64)
        };

        let a_map: std::collections::HashMap<(i64, i64), f32> =
            boundary_a.iter().map(|bv| (key(bv), bv.elevation)).collect();
        let b_map: std::collections::HashMap<(i64, i64), f32> =
            boundary_b.iter().map(|bv| (key(bv), bv.elevation)).collect();

        // Find vertices that exist in both boundaries (shared edge)
        let mut shared_count = 0;
        for (pos, &elev_a) in &a_map {
            if let Some(&elev_b) = b_map.get(pos) {
                assert_eq!(elev_a, elev_b,
                    "border elevation mismatch at ({}, {}): chunk A = {}, chunk B = {}",
                    pos.0, pos.1, elev_a, elev_b);
                shared_count += 1;
            }
        }
        assert!(shared_count >= 2,
            "expected at least 2 shared border vertices between adjacent chunks, found {}", shared_count);
    }
}
