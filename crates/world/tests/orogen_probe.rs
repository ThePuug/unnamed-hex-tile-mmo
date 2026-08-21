//! Gate 2 — do orogen crest lines run through, and does the steep side hold?
//!
//! Crest geometry only: no cross-section, no elevation. The question is whether
//! belts emerge from the boundary graph without anything assembling them.
//!
//! Adjacency here is a *shared Voronoi vertex*: segments (P,Q) and (P,R) meet at
//! a vertex exactly when Q and R are themselves neighbours, so the two faces
//! bound a common third plate. Sharing a plate is not enough — the two opposite
//! edges of one plate share it and never touch.
//!
//! Run: cargo test -p world --release --test orogen_probe -- --ignored --nocapture

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use world::PlateCache;
use world::events::Composite;
use world::events::motion::{BoundarySegment, MarginClass, MotionEvent, PlateBoundaryIndex};
use world::events::orogen::{OrogenEvent, OrogenSwathIndex, Swath};
use world::events::plates::PlateEvent;

const SEED: u64 = 0x9E3779B97F4A7C15;
const BLOCK: i32 = 30_000;
const STEP: i32 = 1_500;

/// Cosine of the largest strike difference two crests may have and still read
/// as one range rather than as a crossing. 0.85 is 32 degrees.
const BELT_ALIGNMENT: f64 = 0.85;

fn block(extent: i32, step: i32) -> Vec<(i32, i32)> {
    let mut v = Vec::new();
    let mut q = -extent;
    while q <= extent {
        let mut r = -extent;
        while r <= extent {
            v.push((q, r));
            r += step;
        }
        q += step;
    }
    v
}

fn sampled(coords: &[(i32, i32)]) -> (Vec<BoundarySegment>, Vec<Swath>) {
    let cache = Arc::new(PlateCache::new(SEED));
    let mut c = Composite::new(SEED);
    c.add_event(Box::new(PlateEvent::with_cache(cache.clone())));
    c.add_event(Box::new(MotionEvent::with_cache(cache, SEED)));
    c.add_event(Box::new(OrogenEvent::new()));
    c.tiles_at(coords);

    let segs = c.with_indexes(|ix| {
        ix.get::<PlateBoundaryIndex>()
            .map(|idx| idx.cells.values().flat_map(|v| v.iter().cloned()).collect())
            .unwrap_or_default()
    });
    let mut swaths: Vec<Swath> = c.with_indexes(|ix| {
        ix.get::<OrogenSwathIndex>()
            .map(|idx| {
                idx.cells.values().flat_map(|v| v.iter().map(|s| (**s).clone())).collect()
            })
            .unwrap_or_default()
    });
    swaths.sort_by(|a, b| (a.plate_a, a.plate_b).cmp(&(b.plate_a, b.plate_b)));
    (segs, swaths)
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

/// Shortest distance between two line segments.
fn segment_distance(
    a0: (f64, f64), a1: (f64, f64), b0: (f64, f64), b1: (f64, f64),
) -> f64 {
    fn cross(o: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
        (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
    }
    // Proper crossing: the endpoints of each straddle the other's line.
    let d1 = cross(a0, a1, b0);
    let d2 = cross(a0, a1, b1);
    let d3 = cross(b0, b1, a0);
    let d4 = cross(b0, b1, a1);
    if ((d1 > 0.0) != (d2 > 0.0)) && ((d3 > 0.0) != (d4 > 0.0)) { return 0.0; }

    fn point_to_segment(p: (f64, f64), s0: (f64, f64), s1: (f64, f64)) -> f64 {
        let (vx, vy) = (s1.0 - s0.0, s1.1 - s0.1);
        let len2 = vx * vx + vy * vy;
        let t = if len2 <= 0.0 { 0.0 } else {
            (((p.0 - s0.0) * vx + (p.1 - s0.1) * vy) / len2).clamp(0.0, 1.0)
        };
        (p.0 - (s0.0 + t * vx)).hypot(p.1 - (s0.1 + t * vy))
    }
    point_to_segment(a0, b0, b1)
        .min(point_to_segment(a1, b0, b1))
        .min(point_to_segment(b0, a0, a1))
        .min(point_to_segment(b1, a0, a1))
}

/// Connected components of an edge list, as (size, spatial extent).
fn components(n: usize, edges: &[(usize, usize)], points: &[(f64, f64)]) -> Vec<(usize, f64)> {
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut Vec<usize>, mut i: usize) -> usize {
        while parent[i] != i { parent[i] = parent[parent[i]]; i = parent[i]; }
        i
    }
    for &(i, j) in edges {
        let (a, b) = (find(&mut parent, i), find(&mut parent, j));
        if a != b { parent[a] = b; }
    }
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n { let r = find(&mut parent, i); groups.entry(r).or_default().push(i); }
    let mut out: Vec<(usize, f64)> = groups.values().map(|members| {
        let mut extent = 0.0f64;
        for i in 0..members.len() {
            for j in (i + 1)..members.len() {
                let (a, b) = (points[members[i]], points[members[j]]);
                extent = extent.max((a.0 - b.0).hypot(a.1 - b.1));
            }
        }
        (members.len(), extent)
    }).collect();
    out.sort_unstable_by(|a, b| b.0.cmp(&a.0));
    out
}

fn report_components(label: &str, comps: &[(usize, f64)]) {
    if comps.is_empty() { println!("  {label:<22} none"); return; }
    let total: usize = comps.iter().map(|c| c.0).sum();
    let singles = comps.iter().filter(|c| c.0 == 1).count();
    let in_belts: usize = comps.iter().filter(|c| c.0 >= 4).map(|c| c.0).sum();
    let head: Vec<String> = comps.iter().take(8)
        .map(|(n, e)| format!("{n}@{:.0}", e)).collect();
    println!(
        "  {label:<22} {total:>5} crests in {:>4} runs — median {:>2}, longest {:>3} \
         (extent {:.0} WU), singletons {:>3.0}%, in runs>=4 {:>3.0}%",
        comps.len(), comps[comps.len() / 2].0, comps[0].0, comps[0].1,
        100.0 * singles as f64 / comps.len() as f64,
        100.0 * in_belts as f64 / total as f64,
    );
    println!("  {:<22} longest: {}", "", head.join(", "));
}

#[test]
#[ignore]
fn crest_continuity() {
    let coords = block(BLOCK, STEP);
    let (segs, swaths) = sampled(&coords);
    println!("\n=== Orogen crests over a {}x{} tile block ===", BLOCK * 2, BLOCK * 2);
    println!("  {} boundary segments, {} carried a swath ({:.1}%)",
        segs.len(), swaths.len(), 100.0 * swaths.len() as f64 / segs.len() as f64);
    assert!(!swaths.is_empty(), "no swaths placed");

    let active = swaths.iter().filter(|s| s.margin == MarginClass::Active).count();
    println!("  {active} on active margins, {} interior", swaths.len() - active);
    println!("  drive              {}", percentiles(swaths.iter().map(|s| s.drive).collect()));
    println!("  half-length        {}", percentiles(swaths.iter().map(|s| s.half_length).collect()));

    // Voronoi adjacency, from every segment — including the ones that carried no
    // swath, since they still say which faces touch.
    let mut nbrs: HashMap<u64, HashSet<u64>> = HashMap::new();
    for s in &segs {
        nbrs.entry(s.plate_a).or_default().insert(s.plate_b);
        nbrs.entry(s.plate_b).or_default().insert(s.plate_a);
    }
    let mut by_plate: HashMap<u64, Vec<usize>> = HashMap::new();
    for (i, s) in swaths.iter().enumerate() {
        by_plate.entry(s.plate_a).or_default().push(i);
        by_plate.entry(s.plate_b).or_default().push(i);
    }
    let other = |s: &Swath, p: u64| if s.plate_a == p { s.plate_b } else { s.plate_a };

    // Pairs of placed swaths whose faces meet at a Voronoi vertex.
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    for (&p, members) in &by_plate {
        for i in 0..members.len() {
            for j in (i + 1)..members.len() {
                let (x, y) = (members[i], members[j]);
                let (qa, qb) = (other(&swaths[x], p), other(&swaths[y], p));
                if nbrs.get(&qa).map_or(false, |s| s.contains(&qb)) {
                    pairs.push((x, y));
                }
            }
        }
    }
    pairs.sort_unstable();
    pairs.dedup();
    println!("\nPlaced-swath pairs meeting at a shared vertex: {}", pairs.len());

    // ── Position continuity ──
    let mut gap = Vec::new();
    let mut turn = Vec::new();
    let mut broken = 0usize;
    for &(i, j) in &pairs {
        let (a0, a1) = swaths[i].crest_ends();
        let (b0, b1) = swaths[j].crest_ends();
        let d = segment_distance(a0, a1, b0, b1);
        gap.push(d);
        if d > 0.0 { broken += 1; }
        let dot = (swaths[i].strike_x * swaths[j].strike_x
            + swaths[i].strike_y * swaths[j].strike_y).abs().min(1.0);
        turn.push(dot.acos().to_degrees());
    }
    println!("\nPOSITION — distance between crest lines that share a vertex");
    println!("  crest gap WU       {}", percentiles(gap.clone()));
    println!("  pairs not touching {broken} of {} ({:.1}%)",
        pairs.len(), 100.0 * broken as f64 / pairs.len().max(1) as f64);
    println!("\nALIGNMENT — how far the strike turns across that vertex");
    println!("  turn deg           {}", percentiles(turn.clone()));
    let aligned = turn.iter().filter(|&&t| t.to_radians().cos() >= BELT_ALIGNMENT).count();
    println!("  within 32 deg      {aligned} of {} ({:.1}%)",
        pairs.len(), 100.0 * aligned as f64 / pairs.len().max(1) as f64);

    // ── Vergence continuity ──
    // Two frames. An active margin verges seaward by construction, so two of
    // them agree without anything checking; interior pairs pick a side by
    // projecting the drift onto their own normal, and the world frame is where
    // that rule is coherent.
    let mut interior = (0usize, 0usize);
    let mut margin = (0usize, 0usize);
    let mut mixed = (0usize, 0usize);
    let mut flip_drive = Vec::new();
    let mut hold_drive = Vec::new();
    let mut agree_edges: Vec<(usize, usize)> = Vec::new();
    for &(i, j) in &pairs {
        let (a, b) = (&swaths[i], &swaths[j]);
        let dot = a.vergence_x * b.vergence_x + a.vergence_y * b.vergence_y;
        let agrees = match (a.margin, b.margin) {
            (MarginClass::Active, MarginClass::Active) => { margin.1 += 1; margin.0 += 1; true }
            (MarginClass::Interior, MarginClass::Interior) => {
                interior.1 += 1;
                if dot > 0.0 { interior.0 += 1; true } else { false }
            }
            _ => {
                mixed.1 += 1;
                if dot > 0.0 { mixed.0 += 1; true } else { false }
            }
        };
        let weaker = a.drive.min(b.drive);
        if agrees { hold_drive.push(weaker); } else { flip_drive.push(weaker); }
        if agrees { agree_edges.push((i, j)); }
    }
    let pct = |t: (usize, usize)| 100.0 * t.0 as f64 / t.1.max(1) as f64;
    println!("\nVERGENCE — does the steep side hold across a vertex");
    println!("  interior pairs     {:>4} of {:>4}  {:>5.1}%", interior.0, interior.1, pct(interior));
    println!("  margin pairs       {:>4} of {:>4}  {:>5.1}%  (seaward by construction)",
        margin.0, margin.1, pct(margin));
    println!("  arc/interior seam  {:>4} of {:>4}  {:>5.1}%", mixed.0, mixed.1, pct(mixed));
    let flips = flip_drive.len();
    println!("  flips overall      {flips} of {} ({:.1}%)",
        pairs.len(), 100.0 * flips as f64 / pairs.len().max(1) as f64);
    println!("\n  Where flips land. A flip on a strong pair switches a visible range's");
    println!("  steep side mid-length; one on a weak pair is on ground already tapering out.");
    println!("  drive at flips     {}", percentiles(flip_drive.clone()));
    println!("  drive where held   {}", percentiles(hold_drive));
    for band in [(0.0, 0.25), (0.25, 0.5), (0.5, 1.01)] {
        let f = flip_drive.iter().filter(|&&d| d >= band.0 && d < band.1).count();
        let tot = pairs.iter().filter(|&&(i, j)| {
            let d = swaths[i].drive.min(swaths[j].drive);
            d >= band.0 && d < band.1
        }).count();
        println!("    drive {:.2}-{:.2}     {f:>4} flips of {tot:>4} pairs  {:>5.1}%",
            band.0, band.1.min(1.0), 100.0 * f as f64 / tot.max(1) as f64);
    }

    // ── Belt runs ──
    let pts: Vec<(f64, f64)> = swaths.iter().map(|s| (s.cx, s.cy)).collect();
    let n = swaths.len();
    let aligned_edges: Vec<(usize, usize)> = pairs.iter().copied()
        .filter(|&(i, j)| {
            (swaths[i].strike_x * swaths[j].strike_x
                + swaths[i].strike_y * swaths[j].strike_y).abs() >= BELT_ALIGNMENT
        })
        .collect();
    let both_edges: Vec<(usize, usize)> = agree_edges.iter().copied()
        .filter(|&(i, j)| {
            (swaths[i].strike_x * swaths[j].strike_x
                + swaths[i].strike_y * swaths[j].strike_y).abs() >= BELT_ALIGNMENT
        })
        .collect();
    println!("\nBELT RUNS — connected crests, by what is required to link two");
    report_components("shared vertex", &components(n, &pairs, &pts));
    report_components("+ vergence agrees", &components(n, &agree_edges, &pts));
    report_components("+ strike aligned", &components(n, &aligned_edges, &pts));
    report_components("+ both", &components(n, &both_edges, &pts));
    println!();
}

/// Two composites, same ground, tiles touched in opposite orders. Any parameter
/// deriving from the observing cell rather than from the segment shows up as a
/// swath that moved.
#[test]
#[ignore]
fn cell_boundary_agreement() {
    let coords = block(9_000, 900);
    let (_, forward) = sampled(&coords);
    let reversed_coords: Vec<(i32, i32)> = coords.iter().rev().copied().collect();
    let (_, reversed) = sampled(&reversed_coords);

    println!("\n=== Cell-boundary agreement: {} vs {} swaths ===",
        forward.len(), reversed.len());
    assert_eq!(forward.len(), reversed.len(), "swath count changed with visit order");

    let mut moved = 0usize;
    let mut worst = 0.0f64;
    for (a, b) in forward.iter().zip(reversed.iter()) {
        assert_eq!((a.plate_a, a.plate_b), (b.plate_a, b.plate_b));
        let d = (a.cx - b.cx).hypot(a.cy - b.cy);
        if d > 0.0 { moved += 1; }
        worst = worst.max(d);
        assert_eq!(a.drive.to_bits(), b.drive.to_bits(), "drive differed");
        assert_eq!(a.vergence_x.to_bits(), b.vergence_x.to_bits(), "vergence differed");
        assert_eq!(a.half_length.to_bits(), b.half_length.to_bits(), "half-length differed");
    }
    println!("  crests that moved: {moved}, worst displacement {worst} WU");

    // And each segment written exactly once, whichever cells its swath reaches.
    let mut seen = HashSet::new();
    for s in &forward {
        assert!(seen.insert((s.plate_a, s.plate_b)),
            "segment ({}, {}) placed twice", s.plate_a, s.plate_b);
    }
    println!("  {} swaths, all from distinct segments", forward.len());
    println!();
}
