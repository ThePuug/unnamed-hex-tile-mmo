//! Scale derivation and boundary-graph geometry for the orogen layer.
//!
//! Run: cargo test -p world --release --test orogen_scale_probe -- --ignored --nocapture

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use common::HexLattice;
use world::{MACRO_CELL_SIZE, PlateCache};
use world::events::{CellScope, Composite, TileOutput, TileView, WorldEvent};
use world::events::motion::{BoundarySegment, MotionEvent, PlateBoundaryIndex};
use world::events::plates::PlateEvent;
use world::events::slope_form::SlopeFormEvent;
use world::hex_to_world;

const SEED: u64 = 0x9E3779B97F4A7C15;
const BLOCK: i32 = 30_000;
const STEP: i32 = 1_500;

/// What `plate_neighbors` searches, and therefore the hard bound on separation.
const SEARCH_BOUND: f64 = MACRO_CELL_SIZE * 4.0;

fn sampled_block() -> Vec<BoundarySegment> {
    let cache = Arc::new(PlateCache::new(SEED));
    let mut c = Composite::new(SEED);
    c.add_event(Box::new(PlateEvent::with_cache(cache.clone())));
    c.add_event(Box::new(MotionEvent::with_cache(cache, SEED)));

    let mut coords = Vec::new();
    let mut q = -BLOCK;
    while q <= BLOCK {
        let mut r = -BLOCK;
        while r <= BLOCK {
            coords.push((q, r));
            r += STEP;
        }
        q += STEP;
    }
    c.tiles_at(&coords);

    c.with_indexes(|ix| {
        ix.get::<PlateBoundaryIndex>()
            .map(|idx| idx.cells.values().flat_map(|v| v.iter().cloned()).collect())
            .unwrap_or_default()
    })
}

fn percentiles(mut v: Vec<f64>) -> String {
    if v.is_empty() { return "none".into(); }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let at = |p: f64| v[((v.len() - 1) as f64 * p).round() as usize];
    format!(
        "n {:>5}  min {:>7.1}  p10 {:>7.1}  p50 {:>7.1}  p90 {:>7.1}  p99 {:>7.1}  max {:>7.1}",
        v.len(), v[0], at(0.10), at(0.50), at(0.90), at(0.99), v[v.len() - 1]
    )
}

// -- 1. Is the neighbour search truncating the graph? ------------------------

#[test]
#[ignore]
fn separation_tail() {
    let segs = sampled_block();
    let n = segs.len() as f64;
    println!("\n=== Separation against the {SEARCH_BOUND} neighbour-search bound ===");
    for frac in [0.90, 0.95, 0.98, 1.00] {
        let t = SEARCH_BOUND * frac;
        let c = segs.iter().filter(|s| s.separation() >= t).count();
        println!("  sep >= {:>6.0} ({:>3.0}% of bound)  {c:>5}  {:>5.2}%",
            t, frac * 100.0, 100.0 * c as f64 / n);
    }

    // Rebuild the adjacency with a wider search and see what the production
    // radius missed. Same midpoint test, so anything extra is a real Voronoi
    // neighbour the search cut off rather than a candidate the test rejected.
    let cache = PlateCache::new(SEED);
    let mut sites: HashMap<u64, (f64, f64)> = HashMap::new();
    for s in &segs {
        sites.insert(s.plate_a, (s.ax, s.ay));
        sites.insert(s.plate_b, (s.bx, s.by));
    }

    let wide = |wx: f64, wy: f64, radius: f64| -> Vec<(u64, f64, f64)> {
        let owner = cache.plate_at(wx, wy);
        cache.plates_in_radius(owner.wx, owner.wy, radius).into_iter()
            .filter(|c| c.id != owner.id)
            .filter(|c| {
                let at = cache.plate_at((owner.wx + c.wx) * 0.5, (owner.wy + c.wy) * 0.5);
                at.id == owner.id || at.id == c.id
            })
            .map(|c| (c.id, c.wx, c.wy))
            .collect()
    };

    let mut extra: Vec<f64> = Vec::new();
    let mut checked = 0usize;
    for (id, &(px, py)) in sites.iter().take(1500) {
        let _ = id;
        checked += 1;
        let narrow: HashSet<u64> = cache.plate_neighbors(px, py).iter().map(|p| p.id).collect();
        for (nid, nx, ny) in wide(px, py, SEARCH_BOUND * 2.0) {
            if !narrow.contains(&nid) {
                extra.push((nx - px).hypot(ny - py));
            }
        }
    }
    println!("\n  adjacency recomputed at 2x the search radius over {checked} plates");
    println!("  neighbours the production search missed: {}", extra.len());
    if !extra.is_empty() {
        println!("  their separation  {}", percentiles(extra));
    }
    println!();
}

// -- 2. What actually meets at a shared vertex -------------------------------

/// Half-extent of the real Voronoi face between two plates, measured along
/// strike from the midpoint.
///
/// Walking straight along strike leaves the boundary as it curves, so each
/// step re-finds the P|Q crossing along the normal. The face has ended when no
/// crossing remains inside the window.
fn face_half_extent(cache: &PlateCache, s: &BoundarySegment, dir: f64) -> f64 {
    let sep = s.separation();
    let (nx, ny) = ((s.bx - s.ax) / sep, (s.by - s.ay) / sep);
    let step = 40.0;
    let window = sep * 0.75;
    let samples = 24;
    let mut t = 0.0;
    loop {
        t += step;
        if t > sep * 4.0 { return t; }
        let (bx, by) = (s.mx + dir * t * s.strike_x, s.my + dir * t * s.strike_y);
        let mut prev: Option<u64> = None;
        let mut crossed = false;
        for i in 0..=samples {
            let u = -window + 2.0 * window * i as f64 / samples as f64;
            let id = cache.plate_at(bx + u * nx, by + u * ny).id;
            if let Some(p) = prev {
                if (p == s.plate_a && id == s.plate_b) || (p == s.plate_b && id == s.plate_a) {
                    crossed = true;
                    break;
                }
            }
            prev = Some(id);
        }
        if !crossed { return t - step; }
    }
}

#[test]
#[ignore]
fn vertex_adjacency() {
    let segs = sampled_block();
    let cache = PlateCache::new(SEED);
    println!("\n=== Boundary-graph adjacency: what meets what ===");

    // Voronoi neighbour sets, so a shared vertex can be told from a shared plate.
    let mut nbrs: HashMap<u64, HashSet<u64>> = HashMap::new();
    for s in &segs {
        nbrs.entry(s.plate_a).or_default().insert(s.plate_b);
        nbrs.entry(s.plate_b).or_default().insert(s.plate_a);
    }

    let mut by_plate: HashMap<u64, Vec<usize>> = HashMap::new();
    for (i, s) in segs.iter().enumerate() {
        by_plate.entry(s.plate_a).or_default().push(i);
        by_plate.entry(s.plate_b).or_default().push(i);
    }

    let other = |s: &BoundarySegment, p: u64| if s.plate_a == p { s.plate_b } else { s.plate_a };
    let turn = |a: &BoundarySegment, b: &BoundarySegment| {
        (a.strike_x * b.strike_x + a.strike_y * b.strike_y)
            .abs().min(1.0).acos().to_degrees()
    };

    let mut vertex_turn = Vec::new();
    let mut vertex_gap = Vec::new();
    let mut parallel_turn = Vec::new();
    let mut parallel_gap = Vec::new();
    for (&p, members) in &by_plate {
        for i in 0..members.len() {
            for j in (i + 1)..members.len() {
                let (a, b) = (&segs[members[i]], &segs[members[j]]);
                let (qa, qb) = (other(a, p), other(b, p));
                let gap = (b.mx - a.mx).hypot(b.my - a.my);
                if nbrs.get(&qa).map_or(false, |s| s.contains(&qb)) {
                    // The two faces bound a common third plate, so they meet at
                    // a Voronoi vertex.
                    vertex_turn.push(turn(a, b));
                    vertex_gap.push(gap);
                } else {
                    parallel_turn.push(turn(a, b));
                    parallel_gap.push(gap);
                }
            }
        }
    }

    println!("\nPairs sharing a plate, split by whether their faces meet at a vertex");
    println!("  meet at a vertex   {}", vertex_turn.len());
    println!("    strike turn deg  {}", percentiles(vertex_turn));
    println!("    midpoint gap     {}", percentiles(vertex_gap));
    println!("  do not             {}", parallel_turn.len());
    println!("    strike turn deg  {}", percentiles(parallel_turn));
    println!("    midpoint gap     {}", percentiles(parallel_gap));

    // The face a segment actually owns, against the separation Gate 1 sized the
    // swath from.
    let sample: Vec<&BoundarySegment> = segs.iter().step_by(segs.len() / 300 + 1).collect();
    let mut extent = Vec::new();
    let mut ratio = Vec::new();
    for s in &sample {
        let e = 0.5 * (face_half_extent(&cache, s, 1.0) + face_half_extent(&cache, s, -1.0));
        extent.push(e);
        ratio.push(e / s.separation());
    }
    println!("\nReal Voronoi face half-extent along strike ({} segments sampled)", sample.len());
    println!("  half-extent        {}", percentiles(extent));
    println!("  / separation       {}", percentiles(ratio));
    println!();
}

// -- 3. Cascade cost ---------------------------------------------------------

struct StubLayer(u32);

impl WorldEvent for StubLayer {
    fn name(&self) -> &str { "orogen-stub" }
    fn scale(&self) -> u32 { self.0 }
    fn deform(&self, _scope: &CellScope) {}
    fn query(
        &self, _q: i32, _r: i32, _b: &TileView,
        _c: &(dyn std::any::Any + Send + Sync), _s: u64,
    ) -> Option<TileOutput> { None }
}

fn one_ring_clearance(lattice: &HexLattice) -> f64 {
    fn support(r: f64, theta: f64) -> f64 {
        let sixty = std::f64::consts::PI / 3.0;
        let delta = (theta.rem_euclid(sixty) - sixty * 0.5).abs();
        r * delta.cos()
    }
    let centre = lattice.cell_center((0, 0));
    let (cx, cy) = hex_to_world(centre.0, centre.1);
    let ring1 = lattice.cells_within_distance((0, 0), 1);
    let r = lattice.radius as f64;
    let mut gap = f64::MAX;
    for id in lattice.cells_within_distance((0, 0), 2) {
        if ring1.contains(&id) { continue; }
        let c = lattice.cell_center(id);
        let (dx, dy) = hex_to_world(c.0, c.1);
        let (vx, vy) = (dx - cx, dy - cy);
        gap = gap.min(vx.hypot(vy) - 2.0 * support(r, vy.atan2(vx)));
    }
    gap
}

fn hexball(cq: i32, cr: i32, radius: i32) -> Vec<(i32, i32)> {
    let mut v = Vec::new();
    for dq in -radius..=radius {
        let lo = (-radius).max(-dq - radius);
        let hi = radius.min(-dq + radius);
        for dr in lo..=hi { v.push((cq + dq, cr + dr)); }
    }
    v
}

#[test]
#[ignore]
fn cascade_at_scale() {
    let influence = 4459.0f64;
    println!("\n=== Deform cascade, max_influence {influence} ===");
    println!("{:>8}  {:>10}  {:>9}  {:>8}  {:>8}  {:>8}  {:>10}",
        "scale", "clearance", "headroom", "plates", "motion", "orogen", "slope-form");
    for scale in [15225u32, 4000, 3600, 3516] {
        let clearance = one_ring_clearance(&HexLattice::new(scale));
        for (label, coords) in [
            ("tile", vec![(3000, 2000)]),
            ("chunk_271", hexball(3000, 2000, 9)),
        ] {
            let plate_cache = Arc::new(PlateCache::new(SEED));
            let mut c = Composite::new(SEED);
            c.add_event(Box::new(PlateEvent::with_cache(plate_cache.clone())));
            c.add_event(Box::new(MotionEvent::with_cache(plate_cache, SEED)));
            c.add_event(Box::new(StubLayer(scale)));
            c.add_event(Box::new(SlopeFormEvent::new()));
            c.tiles_at(&coords);
            let m = c.drain_metrics();
            let cells: Vec<usize> = m.layers.iter().map(|l| l.indexed).collect();
            println!("{scale:>8}  {clearance:>10.1}  {:>9.1}  {:>8}  {:>8}  {:>8}  {:>10}   cold {label}",
                clearance - influence, cells[0], cells[1], cells[2], cells[3]);
        }
    }
    println!();
}
