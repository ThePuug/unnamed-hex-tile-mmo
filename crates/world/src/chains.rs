//! Chains — outlines as segments with a side, and the bucket grid that
//! answers "the nearest segment within reach" without a scan.
//!
//! Every outline in the world is a chain of lattice nodes: a coast, a plate
//! edge, a belt front. A consumer asks two things of one: how far a position
//! is from it, and which side of it the position lies on. Distance to a set
//! of segments is continuous, and the layers that read chains read that
//! distance and nothing that picks among segments, which is what keeps their
//! output free of seams.
//!
//! **A chain's segments all face the same way along it.** The side is
//! decided once per chain, from the straight line the chain draws, and every
//! segment takes the left or the right of its own direction accordingly. A
//! segment facing a point of its own picks a side by projection, and for the
//! short steps a lattice chain takes across its line that projection is
//! noise: half the steps face backward, and every position nearest a
//! backward step's end reads the wrong side, a wedge of land in the sea.
//!
//! **A corner's side is the corner's.** A position whose nearest point on a
//! chain is a node reads its side off that node's normal, the average of the
//! normals of the segments meeting there. Read off either segment alone, a
//! position square off the node reads a side of zero, and which segment the
//! search found first decides.
//!
//! **An open end has no side, only a facing.** Past the last node of a chain
//! the side is a half-plane through the end, and a consumer that gates on it
//! cuts a cliff along that line. So a position nearest an open end is handed
//! how squarely it faces the end, from one directly behind to nothing at
//! right angles, and a consumer that fades by it wraps the end the way
//! distance does.

use std::collections::HashMap;

use crate::lattice::NodeKey;

/// One segment of a chain, with the unit normal that points to the side the
/// chain encloses: into the belt for a front, onto the land for a coast, and
/// the normals at its two ends, the corners' once joined.
#[derive(Clone, Copy, Debug)]
pub struct Segment {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub nx: f64,
    pub ny: f64,
    pub n0x: f64,
    pub n0y: f64,
    pub n1x: f64,
    pub n1y: f64,
    /// Whether each end is the open end of its chain: a node no other segment
    /// meets it at.
    pub open0: bool,
    pub open1: bool,
}

/// What a position reads off the segment nearest it.
#[derive(Clone, Copy, Debug)]
pub struct Nearest {
    /// Distance to the segment.
    pub distance: f64,
    /// Which side the position lies on: positive on the side the normal
    /// points to.
    pub side: f64,
    /// How squarely the position faces the chain, in [−1, 1]: ±1 anywhere
    /// but past an open end, and past one the cosine of the angle from
    /// squarely behind the end.
    pub facing: f64,
    /// Index of the segment in its grid.
    pub index: usize,
}

impl Segment {
    /// A segment from `p` to `q` whose normal is the left perpendicular of
    /// its direction, or the right when `left` is false. Both end normals
    /// start as the segment's own, and both ends closed.
    pub fn along(p: (f64, f64), q: (f64, f64), left: bool) -> Self {
        let (tx, ty) = (q.0 - p.0, q.1 - p.1);
        let (mut nx, mut ny) = (-ty, tx);
        let len = nx.hypot(ny);
        if len > 0.0 { nx /= len; ny /= len } else { nx = 1.0; ny = 0.0 }
        if !left { nx = -nx; ny = -ny }
        Segment { x0: p.0, y0: p.1, x1: q.0, y1: q.1, nx, ny, n0x: nx, n0y: ny, n1x: nx, n1y: ny, open0: false, open1: false }
    }

    /// Whether a point lies to the left of the line from `p` to `q`: what a
    /// chain drawn from `p` to `q` asks once, of the point it encloses.
    pub fn is_left(p: (f64, f64), q: (f64, f64), point: (f64, f64)) -> bool {
        (q.0 - p.0) * (point.1 - p.1) - (q.1 - p.1) * (point.0 - p.0) > 0.0
    }

    pub fn mid(&self) -> (f64, f64) {
        (0.5 * (self.x0 + self.x1), 0.5 * (self.y0 + self.y1))
    }

    /// Unit tangent: the strike of the chain here.
    pub fn tangent(&self) -> (f64, f64) {
        let (dx, dy) = (self.x1 - self.x0, self.y1 - self.y0);
        let len = dx.hypot(dy);
        if len > 0.0 { (dx / len, dy / len) } else { (1.0, 0.0) }
    }

    /// Distance from a position to the segment, its side, and its facing.
    /// Past either end the side is read off that end's normal.
    pub fn nearest(&self, x: f64, y: f64) -> (f64, f64, f64) {
        let (dx, dy) = (self.x1 - self.x0, self.y1 - self.y0);
        let len2 = dx * dx + dy * dy;
        let u = if len2 > 0.0 { ((x - self.x0) * dx + (y - self.y0) * dy) / len2 } else { 0.0 };
        let t = u.clamp(0.0, 1.0);
        let (ex, ey) = (x - (self.x0 + t * dx), y - (self.y0 + t * dy));
        let d = ex.hypot(ey);
        let (nx, ny, open) = if u <= 0.0 {
            (self.n0x, self.n0y, self.open0)
        } else if u >= 1.0 {
            (self.n1x, self.n1y, self.open1)
        } else {
            (self.nx, self.ny, false)
        };
        let side = ex * nx + ey * ny;
        let facing = if open && d > 0.0 { (side / d).clamp(-1.0, 1.0) } else if side >= 0.0 { 1.0 } else { -1.0 };
        (d, side, facing)
    }

    /// Distance from a position to the segment, and which side it lies on.
    pub fn distance(&self, x: f64, y: f64) -> (f64, f64) {
        let (d, side, _) = self.nearest(x, y);
        (d, side)
    }
}

/// Give every segment its corners' normals and mark its open ends: at each
/// node, the normalised sum of the normals of the segments meeting there,
/// and a node with one segment is that segment's open end. A node where the
/// normals cancel keeps each segment's own.
pub fn join_at_nodes(segments: &mut [Segment], nodes: &[(NodeKey, NodeKey)]) {
    let mut sum: HashMap<NodeKey, (f64, f64, usize)> = HashMap::new();
    for (s, (a, b)) in segments.iter().zip(nodes) {
        for node in [a, b] {
            let e = sum.entry(*node).or_insert((0.0, 0.0, 0));
            e.0 += s.nx;
            e.1 += s.ny;
            e.2 += 1;
        }
    }
    let corner = |node: &NodeKey, own: (f64, f64)| {
        let (x, y, count) = sum[node];
        let l = x.hypot(y);
        let n = if l > 1e-9 { (x / l, y / l) } else { own };
        (n, count == 1)
    };
    for (s, (a, b)) in segments.iter_mut().zip(nodes) {
        let own = (s.nx, s.ny);
        let ((n0x, n0y), open0) = corner(a, own);
        let ((n1x, n1y), open1) = corner(b, own);
        s.n0x = n0x; s.n0y = n0y; s.open0 = open0;
        s.n1x = n1x; s.n1y = n1y; s.open1 = open1;
    }
}

/// Segments bucketed on a square grid for a nearest search.
///
/// Each segment sits in every bucket its span touches. The nearest segment
/// to a position is found by widening rings of buckets around the position's
/// own and stopping once a ring can hold nothing closer than the best so far:
/// a ring `k` out holds nothing nearer than `k − 1` buckets, whatever the
/// position within its own. A search bounded by a reach of a few buckets
/// visits a few dozen buckets, nearly all empty.
pub struct SegmentGrid {
    segments: Vec<Segment>,
    buckets: Vec<Vec<u32>>,
    bucket: f64,
    i0: i64,
    j0: i64,
    nx: usize,
    ny: usize,
}

impl SegmentGrid {
    /// Bucket the segments on a grid `bucket` wide.
    pub fn new(segments: Vec<Segment>, bucket: f64) -> Self {
        let b = bucket;
        let span = |s: &Segment| {
            (
                (s.x0.min(s.x1) / b).floor() as i64,
                (s.y0.min(s.y1) / b).floor() as i64,
                (s.x0.max(s.x1) / b).floor() as i64,
                (s.y0.max(s.y1) / b).floor() as i64,
            )
        };
        let (mut i0, mut j0, mut i1, mut j1) = (i64::MAX, i64::MAX, i64::MIN, i64::MIN);
        for s in &segments {
            let (a, c, d, e) = span(s);
            i0 = i0.min(a);
            j0 = j0.min(c);
            i1 = i1.max(d);
            j1 = j1.max(e);
        }
        if segments.is_empty() {
            return Self { segments, buckets: Vec::new(), bucket: b, i0: 0, j0: 0, nx: 0, ny: 0 };
        }
        let (nx, ny) = ((i1 - i0 + 1) as usize, (j1 - j0 + 1) as usize);
        let mut buckets = vec![Vec::new(); nx * ny];
        for (k, s) in segments.iter().enumerate() {
            let (a, c, d, e) = span(s);
            for j in c..=e {
                for i in a..=d {
                    buckets[(j - j0) as usize * nx + (i - i0) as usize].push(k as u32);
                }
            }
        }
        Self { segments, buckets, bucket: b, i0, j0, nx, ny }
    }

    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    fn cell(&self, i: i64, j: i64) -> &[u32] {
        let (i, j) = (i - self.i0, j - self.j0);
        if i < 0 || j < 0 || i as usize >= self.nx || j as usize >= self.ny {
            return &[];
        }
        &self.buckets[j as usize * self.nx + i as usize]
    }

    /// The nearest segment within `limit` of every group present, where
    /// `group` names a segment's group and `groups` is how many there are:
    /// what a consumer reads when the outlines in reach belong to different
    /// objects and choosing among them would cut a seam. The search widens
    /// until every group has been found and no nearer one can appear, or
    /// until the limit.
    pub fn nearest_by_group(
        &self,
        x: f64,
        y: f64,
        limit: f64,
        groups: usize,
        group: impl Fn(usize) -> usize,
        mut visit: impl FnMut(usize, Nearest),
    ) {
        if self.segments.is_empty() || groups == 0 {
            return;
        }
        let b = self.bucket;
        let (ci, cj) = ((x / b).floor() as i64, (y / b).floor() as i64);
        let mut best: Vec<Option<Nearest>> = vec![None; groups];
        let mut found = 0usize;
        let mut farthest_best = 0.0f64;
        for k in 0i64.. {
            let floor = (k - 1) as f64 * b;
            if floor >= limit || (found == groups && floor >= farthest_best) {
                break;
            }
            for j in cj - k..=cj + k {
                let step = if k > 0 && j != cj - k && j != cj + k { 2 * k } else { 1 };
                let mut i = ci - k;
                while i <= ci + k {
                    for &idx in self.cell(i, j) {
                        let (distance, side, facing) = self.segments[idx as usize].nearest(x, y);
                        if distance >= limit { continue }
                        let g = group(idx as usize);
                        let slot = &mut best[g];
                        if slot.map_or(true, |n| distance < n.distance) {
                            if slot.is_none() { found += 1 }
                            *slot = Some(Nearest { distance, side, facing, index: idx as usize });
                        }
                    }
                    i += step;
                }
            }
            farthest_best = best.iter().flatten().map(|n| n.distance).fold(0.0, f64::max);
        }
        for (g, n) in best.into_iter().enumerate() {
            if let Some(n) = n { visit(g, n) }
        }
    }

    /// The nearest segment within `limit`, and what the position reads off
    /// it.
    pub fn nearest(&self, x: f64, y: f64, limit: f64) -> Option<Nearest> {
        if self.segments.is_empty() {
            return None;
        }
        let b = self.bucket;
        let (ci, cj) = ((x / b).floor() as i64, (y / b).floor() as i64);
        let mut best: Option<Nearest> = None;
        let mut best_d = limit;
        for k in 0i64.. {
            if (k - 1) as f64 * b >= best_d {
                break;
            }
            for j in cj - k..=cj + k {
                let step = if k > 0 && j != cj - k && j != cj + k { 2 * k } else { 1 };
                let mut i = ci - k;
                while i <= ci + k {
                    for &idx in self.cell(i, j) {
                        let (distance, side, facing) = self.segments[idx as usize].nearest(x, y);
                        if distance < best_d {
                            best_d = distance;
                            best = Some(Nearest { distance, side, facing, index: idx as usize });
                        }
                    }
                    i += step;
                }
            }
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bucket search finds what a scan of every segment finds, at any
    /// bucket size, from positions inside and outside the reach.
    #[test]
    fn nearest_matches_a_full_scan() {
        let mut segs = Vec::new();
        for i in 0..60 {
            let t = i as f64 * 0.37;
            let p = (4_000.0 * t.cos() + 100.0 * i as f64, 3_000.0 * t.sin());
            let q = (p.0 + 300.0 * (t * 3.0).cos(), p.1 + 300.0 * (t * 3.0).sin());
            segs.push(Segment::along(p, q, true));
        }
        for bucket in [200.0, 1_400.0, 5_000.0] {
            let grid = SegmentGrid::new(segs.clone(), bucket);
            for i in 0..30 {
                for j in 0..30 {
                    let (x, y) = (i as f64 * 400.0 - 6_000.0, j as f64 * 300.0 - 4_500.0);
                    let scan = segs
                        .iter()
                        .map(|s| s.distance(x, y).0)
                        .filter(|d| *d < 2_500.0)
                        .min_by(|a, b| a.partial_cmp(b).unwrap());
                    let got = grid.nearest(x, y, 2_500.0).map(|n| n.distance);
                    assert_eq!(got, scan, "bucket {bucket} at ({x}, {y})");
                }
            }
        }
        assert!(SegmentGrid::new(Vec::new(), 100.0).nearest(0.0, 0.0, 1e9).is_none());
    }

    /// A staircase chain drawn with one side and joined at its corners reads
    /// that side everywhere near it, steps and corners included, and the
    /// side is the one the enclosed point lies on. Past its open ends the
    /// facing turns smoothly from one behind the end to nothing square off
    /// it and below.
    #[test]
    fn a_staircase_faces_one_way() {
        let (p, q) = ((0.0, 0.0), (1000.0, 300.0));
        let enclosed = (500.0, 800.0);
        let left = Segment::is_left(p, q, enclosed);
        assert!(left);
        let nodes = [(0, 0), (3, 0), (3, 1), (6, 1), (6, 2), (9, 2), (9, 3), (10, 3)];
        let at = |n: (i32, i32)| (n.0 as f64 * 100.0, n.1 as f64 * 100.0);
        let mut segs: Vec<Segment> = nodes.windows(2).map(|w| Segment::along(at(w[0]), at(w[1]), left)).collect();
        let pairs: Vec<(NodeKey, NodeKey)> = nodes.windows(2).map(|w| (w[0], w[1])).collect();
        join_at_nodes(&mut segs, &pairs);
        assert!(segs[0].open0 && !segs[0].open1 && segs[6].open1);
        let grid = SegmentGrid::new(segs, 200.0);
        for x in (100..=900).step_by(50) {
            let x = x as f64;
            for dy in [50.0, 200.0, 600.0] {
                let above = grid.nearest(x, 300.0 + dy, 2_000.0).unwrap();
                let below = grid.nearest(x, -dy, 2_000.0).unwrap();
                assert!(above.side > 0.0 && above.facing == 1.0, "the enclosed side reads outside at ({x}, {})", 300.0 + dy);
                assert!(below.side < 0.0 && below.facing == -1.0, "the open side reads inside at ({x}, {})", -dy);
            }
        }
        // Past the open start at the origin: straight behind faces fully,
        // square off faces nothing, ahead faces away.
        let behind = grid.nearest(0.0, 300.0, 2_000.0).unwrap();
        assert!((behind.facing - 1.0).abs() < 1e-9);
        let off = grid.nearest(-300.0, 0.0, 2_000.0).unwrap();
        assert!(off.facing.abs() < 1e-9);
        let ahead = grid.nearest(0.0, -300.0, 2_000.0).unwrap();
        assert!((ahead.facing + 1.0).abs() < 1e-9);
        let between = grid.nearest(-300.0, 300.0, 2_000.0).unwrap();
        assert!(between.facing > 0.0 && between.facing < 1.0);
    }
}
