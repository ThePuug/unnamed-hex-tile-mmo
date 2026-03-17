//! QEM (Quadric Error Metrics) mesh decimation.
//!
//! Deduplicates vertices, then runs Garland-Heckbert edge collapse with
//! priority queue, per-collapse vertex locking, and winding correction.

use std::collections::{BTreeSet, BinaryHeap, HashSet, HashMap};
use std::cmp::Reverse;

// ── Public Types ──

/// Result of QEM decimation — positions, normals, indices ready for rendering.
pub struct DecimatedMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
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

/// Newtype for f64 that implements Ord (for BinaryHeap).
/// NaN is treated as greater than everything (pushed to back).
#[derive(Clone, Copy, PartialEq)]
struct OrdF64(f64);

impl Eq for OrdF64 {}

impl PartialOrd for OrdF64 {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> { Some(self.cmp(other)) }
}

impl Ord for OrdF64 {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.partial_cmp(&other.0).unwrap_or(std::cmp::Ordering::Greater)
    }
}

struct QEMState {
    pos: Vec<[f64; 3]>,
    quadrics: Vec<Quadric>,
    alive: Vec<bool>,
    locked_verts: Vec<bool>,
    locked_edges: HashSet<(usize, usize)>,
    triangles: Vec<[usize; 3]>,
    tri_alive: Vec<bool>,
    adj: Vec<BTreeSet<usize>>,
}

impl QEMState {
    fn new(
        positions: &[[f64; 3]],
        triangles: &[[usize; 3]],
        locked_edges: HashSet<(usize, usize)>,
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

        let mut locked_verts = vec![false; n];
        for &(v0, v1) in &locked_edges {
            locked_verts[v0] = true;
            locked_verts[v1] = true;
        }

        QEMState {
            pos: positions.to_vec(), quadrics, alive: vec![true; n],
            locked_verts, locked_edges,
            triangles: triangles.to_vec(), tri_alive, adj,
        }
    }

    fn is_edge_locked(&self, v0: usize, v1: usize) -> bool {
        self.locked_edges.contains(&(v0.min(v1), v0.max(v1)))
    }

    /// Evaluate collapse cost and target position for edge (vi, vj).
    /// Returns None if the edge can't be collapsed (locked, both locked, etc).
    fn evaluate_edge(&self, vi: usize, vj: usize, threshold: f64) -> Option<(f64, [f64; 3])> {
        if !self.alive[vi] || !self.alive[vj] { return None; }
        if self.locked_verts[vi] && self.locked_verts[vj] { return None; }
        if self.is_edge_locked(vi, vj) { return None; }

        let q_sum = self.quadrics[vi].add(&self.quadrics[vj]);
        let target = if self.locked_verts[vi] {
            self.pos[vi]
        } else if self.locked_verts[vj] {
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
        if cost > threshold { return None; }

        Some((cost, target))
    }

    fn collapse(&mut self, v0: usize, v1: usize, target: [f64; 3]) {
        let (keep, remove) = if self.locked_verts[v1] && !self.locked_verts[v0] {
            (v1, v0)
        } else {
            (v0, v1)
        };
        self.pos[keep] = target;
        self.quadrics[keep] = self.quadrics[keep].add(&self.quadrics[remove]);
        self.alive[remove] = false;
        // Lock keep vertex — prevents cascading collapses that create non-manifold edges
        self.locked_verts[keep] = true;

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
        // Build priority queue: all valid edges ranked by cost (min-heap)
        let mut heap: BinaryHeap<Reverse<(OrdF64, usize, usize)>> = BinaryHeap::new();
        let mut targets: HashMap<(usize, usize), [f64; 3]> = HashMap::new();

        for vi in 0..self.pos.len() {
            for &vj in &self.adj[vi] {
                if vj <= vi { continue; }
                if let Some((cost, target)) = self.evaluate_edge(vi, vj, threshold) {
                    let key = (vi.min(vj), vi.max(vj));
                    heap.push(Reverse((OrdF64(cost), key.0, key.1)));
                    targets.insert(key, target);
                }
            }
        }

        // Process collapses in cost order
        while let Some(Reverse((cost, v0, v1))) = heap.pop() {
            if cost.0 > threshold { break; }
            // Skip stale entries: vertex dead or locked since this entry was queued
            if !self.alive[v0] || !self.alive[v1] { continue; }
            if self.locked_verts[v0] || self.locked_verts[v1] { continue; }

            let key = (v0.min(v1), v0.max(v1));
            let Some(&target) = targets.get(&key) else { continue; };

            self.collapse(v0, v1, target);
        }

        // Kill duplicate triangles that may have formed from vertex merging
        let mut seen = HashSet::new();
        for ti in 0..self.triangles.len() {
            if !self.tri_alive[ti] { continue; }
            let tri = self.triangles[ti];
            let mut key = [tri[0], tri[1], tri[2]];
            key.sort();
            if !seen.insert(key) {
                self.tri_alive[ti] = false;
            }
        }
    }
}

// ── Public API ──

/// Decimate tile geometry with vertex dedup, QEM collapse, boundary locking,
/// and winding correction.
pub fn decimate_geometry(
    geometry: &crate::geometry::TileGeometry,
    threshold: f32,
) -> DecimatedMesh {
    // Deduplicate vertices: merge vertices at the same 3D position
    let mut pos_map: HashMap<(i64, i64, i64), usize> = HashMap::new();
    let mut positions: Vec<[f64; 3]> = Vec::new();
    let mut vtx_remap: Vec<usize> = Vec::with_capacity(geometry.positions.len());

    for p in &geometry.positions {
        // Quantize to 0.01 precision — wide enough to merge shared hex vertices
        // whose f32 positions differ by float rounding (~1e-4 at world coords ~200)
        let key = (
            (p[0] * 100.0).round() as i64,
            (p[1] * 100.0).round() as i64,
            (p[2] * 100.0).round() as i64,
        );
        let idx = *pos_map.entry(key).or_insert_with(|| {
            let i = positions.len();
            positions.push([p[0] as f64, p[1] as f64, p[2] as f64]);
            i
        });
        vtx_remap.push(idx);
    }

    let triangles: Vec<[usize; 3]> = geometry.indices.chunks(3)
        .filter(|c| c.len() == 3)
        .map(|c| [vtx_remap[c[0] as usize], vtx_remap[c[1] as usize], vtx_remap[c[2] as usize]])
        .filter(|[a, b, c]| a != b && b != c && a != c)
        .collect();

    if triangles.is_empty() {
        return DecimatedMesh { positions: Vec::new(), normals: Vec::new(), indices: Vec::new() };
    }

    // Lock edges between perimeter vertices
    let perimeter_set: HashSet<usize> = geometry.boundary_indices.iter()
        .map(|&i| vtx_remap[i as usize])
        .collect();
    let mut locked_edges = HashSet::new();
    for &[v0, v1, v2] in &triangles {
        for &(a, b) in &[(v0, v1), (v1, v2), (v2, v0)] {
            if perimeter_set.contains(&a) && perimeter_set.contains(&b) {
                locked_edges.insert((a.min(b), a.max(b)));
            }
        }
    }

    let mut qem = QEMState::new(&positions, &triangles, locked_edges);
    qem.run(threshold as f64);

    // Remap surviving vertices
    let mut remap: Vec<Option<u32>> = vec![None; qem.pos.len()];
    let mut new_positions: Vec<[f32; 3]> = Vec::new();
    for (vi, &alive) in qem.alive.iter().enumerate() {
        if alive {
            remap[vi] = Some(new_positions.len() as u32);
            let p = qem.pos[vi];
            new_positions.push([p[0] as f32, p[1] as f32, p[2] as f32]);
        }
    }

    // Remap indices with winding fix
    let mut new_indices: Vec<u32> = Vec::new();
    for (ti, &[v0, v1, v2]) in qem.triangles.iter().enumerate() {
        if !qem.tri_alive[ti] { continue; }
        let (r0, r1, r2) = (remap[v0], remap[v1], remap[v2]);
        if r0.is_none() || r1.is_none() || r2.is_none() { continue; }
        let (i0, i1, i2) = (r0.unwrap(), r1.unwrap(), r2.unwrap());
        if i0 == i1 || i1 == i2 || i0 == i2 { continue; }

        new_indices.extend([i0, i1, i2]);
    }

    // Compute per-vertex normals
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
