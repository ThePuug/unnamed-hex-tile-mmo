//! Performance probe for the Composite terrain pipeline.

//! Not a pass/fail test — prints timing data for the production event stack
//! under realistic access patterns (chunk materialization, sparse LoD sampling,
//! dense flyover regions).

//! Run: cargo test -p world --release --test perf_probe -- --ignored --nocapture

use std::time::Instant;

use world::events::Composite;
use world::events::plates::PlateEvent;
use world::events::tilt::TiltEvent;


const SEED: u64 = 0x9E3779B97F4A7C15;

fn composite_full() -> Composite {
    Composite::standard(SEED)
}

/// All tiles within `radius` hex distance of (cq, cr).
fn hexball(cq: i32, cr: i32, radius: i32) -> Vec<(i32, i32)> {
    let mut v = Vec::new();
    for dq in -radius..=radius {
        let lo = (-radius).max(-dq - radius);
        let hi = radius.min(-dq + radius);
        for dr in lo..=hi {
            v.push((cq + dq, cr + dr));
        }
    }
    v
}

fn report_metrics(c: &Composite, label: &str) {
    let m = c.drain_metrics();
    println!(
        "    [{label}] tile-cache: {} hits / {} misses",
        m.tile_hits, m.tile_misses
    );
    for l in &m.layers {
        println!(
            "      layer {:8} cells={} cell-hits={} cell-misses={}",
            l.name, l.indexed, l.cell_hits, l.cell_misses
        );
    }
}

/// Breakdown of the one-time deform cascade: plate-only composite first touch
/// (1 plate cell) vs full-stack first touch, where the spine layer's cell scale
/// dilates the 1800-scale layers below it into the thousands.
#[test]
#[ignore]
fn perf_probe_cascade_breakdown() {
    // 1 plate-cell deform alone
    let mut c = Composite::new(SEED);
    c.add_event(Box::new(PlateEvent::new()));
    c.add_event(Box::new(TiltEvent::new()));
    let t = Instant::now();
    c.tile_at(3000, 2000);
    println!("plate-only first_touch (1 plate cell deform): {:?}", t.elapsed());

    // Full stack on a fresh composite: 169 plate cells + spine deform
    let c = composite_full();
    let t = Instant::now();
    c.tile_at(3000, 2000);
    println!("full-stack first_touch: {:?}", t.elapsed());

    // Second spine cell, far away: plate chunks partially warm
    let t = Instant::now();
    c.tile_at(3000, 40000);
    println!("second spine cell first_touch: {:?}", t.elapsed());
}

#[test]
#[ignore]
fn perf_probe() {
    println!("\n=== terrain perf probe ===\n");

    // ── 1. First touch: single tile_at on a fresh composite ────────────────
    for &(q, r) in &[(0, 0), (3000, 2000), (-8000, 5000)] {
        let c = composite_full();
        let t = Instant::now();
        let view = c.tile_at(q, r);
        let dt = t.elapsed();
        let tags: Vec<_> = view.tags.iter().collect();
        println!(
            "first_touch ({q},{r}): {dt:?}  tags={tags:?} elev={:.1}",
            view.elevation
        );
    }
    println!();

    // ── 2/3. Cold + warm chunk (271 tiles, chunk-sized hexball) ────────────
    for &(cq, cr) in &[(0, 0), (3000, 2000)] {
        let c = composite_full();
        let tiles = hexball(cq, cr, 9);
        let t = Instant::now();
        for &(q, r) in &tiles {
            c.tile_at(q, r);
        }
        let cold = t.elapsed();
        let t = Instant::now();
        for &(q, r) in &tiles {
            c.tile_at(q, r);
        }
        let warm = t.elapsed();
        println!(
            "chunk_271 @ ({cq},{cr}): cold {cold:?} ({:?}/tile), warm {warm:?} ({:?}/tile)",
            cold / 271,
            warm / 271
        );
        report_metrics(&c, "chunk");
    }
    println!();

    // ── 4. Sparse sampling, 200-tile spacing (LoD mid-band pattern) ────────
    // 100 samples in a 10x10 grid, 200 tiles apart: every layer's cell is 1800
    // tiles, so each cell serves several samples and the cost is query, not
    // deform.
    {
        let c = composite_full();
        let t = Instant::now();
        for i in 0..100 {
            let q = (i % 10) * 200;
            let r = (i / 10) * 200;
            c.elevation_at(q, r);
        }
        let dt = t.elapsed();
        println!(
            "sparse_100 @ spacing 200: {dt:?} ({:?}/sample)",
            dt / 100
        );
        report_metrics(&c, "sparse200");
    }
    println!();

    // ── 5. Sparse sampling, 2000-tile spacing (far-band / flyover pattern) ─
    // Every sample in a distinct cell of every layer.
    {
        let c = composite_full();
        let t = Instant::now();
        for i in 0..50 {
            let q = -20000 + (i % 10) * 2000;
            let r = -10000 + (i / 10) * 2000;
            c.elevation_at(q, r);
            if i % 10 == 9 {
                println!("  sparse_wide progress {}/50 elapsed {:?}", i + 1, t.elapsed());
            }
        }
        let dt = t.elapsed();
        println!(
            "sparse_50 @ spacing 2000 (full stack): {dt:?} ({:?}/sample)",
            dt / 50
        );
        report_metrics(&c, "sparse2000");
    }
    println!();

    // ── 6. Dense region: 10,000 contiguous tiles (flyover region build) ────
    {
        let c = composite_full();
        let t = Instant::now();
        for q in 0..100 {
            for r in 0..100 {
                c.tile_at(3000 + q, 2000 + r);
            }
        }
        let dt = t.elapsed();
        let per_sec = 10_000.0 / dt.as_secs_f64();
        println!(
            "dense_10k @ (3000,2000): {dt:?} ({:?}/tile, {per_sec:.0} tiles/sec)",
            dt / 10_000
        );
        report_metrics(&c, "dense");
    }
}

/// What each layer costs, by building the stack one layer at a time and
/// materialising the same fresh patches through each: the first tile is
/// the cascade, the patch is the steady state, and a second patch a
/// flyover's stride away is what moving on costs. The increments between
/// stacks are each layer's own.
#[test]
#[ignore]
fn layer_costs() {
    use world::events::drainage::DrainageEvent;
    use world::events::dissection::DissectionEvent;
    use world::events::lithology::LithologyEvent;
    use world::events::migration::MigrationEvent;
    use world::events::motion::MotionEvent;
    use world::events::thickening::ThickeningEvent;
    use world::events::thrusting::ThrustingEvent;
    let build = |n: usize| -> Composite {
        let mut c = Composite::new(SEED);
        let events: Vec<Box<dyn world::events::WorldEvent>> = vec![
            Box::new(PlateEvent::new()),
            Box::new(TiltEvent::new()),
            Box::new(MotionEvent::new()),
            Box::new(ThrustingEvent::new()),
            Box::new(ThickeningEvent::new()),
            Box::new(LithologyEvent::new()),
            Box::new(DrainageEvent::new()),
            Box::new(MigrationEvent::new()),
            Box::new(DissectionEvent::new()),
        ];
        for e in events.into_iter().take(n) {
            c.add_event(e);
        }
        c
    };
    let names = ["plates", "tilt", "motion", "thrusting", "thickening", "lithology", "drainage", "migration", "dissection"];
    let (sq, sr) = (104_289, -4_677);
    let patch = hexball(sq, sr, 100);
    let next = hexball(sq + 400, sr - 200, 100);
    // A summary's samples: sparse, a stride apart, over a wide reach.
    let sparse: Vec<(i32, i32)> = (0..8).flat_map(|i| (0..8).map(move |j| (sq - 8_000 + i * 2_000, sr - 8_000 + j * 2_000))).collect();
    println!("{:>10} {:>12} {:>14} {:>14} {:>16}", "stack", "first ms", "patch µs/tile", "next µs/tile", "sparse ms/sample");
    for n in 1..=9 {
        let c = build(n);
        let t = Instant::now();
        c.tile_at(sq, sr);
        let first = t.elapsed();
        let t = Instant::now();
        for &(q, r) in &patch {
            c.tile_at(q, r);
        }
        let per = t.elapsed().as_secs_f64() * 1e6 / patch.len() as f64;
        let t = Instant::now();
        for &(q, r) in &next {
            c.tile_at(q, r);
        }
        let per_next = t.elapsed().as_secs_f64() * 1e6 / next.len() as f64;
        let c = build(n);
        let t = Instant::now();
        for &(q, r) in &sparse {
            c.tile_at(q, r);
        }
        let per_sample = t.elapsed().as_secs_f64() * 1e3 / sparse.len() as f64;
        println!("{:>10} {:>12.0} {:>14.1} {:>14.1} {:>16.2}", names[n - 1], first.as_secs_f64() * 1e3, per, per_next, per_sample);
    }
}

/// What one ground sample is made of: the envelope's functions timed
/// apart over a patch of the spawn belt, since a routing is thirty
/// thousand of them.
#[test]
#[ignore]
fn ground_sample_costs() {
    use world::events::drainage::ground_at;
    use world::events::lithology::rock_at;
    use world::events::plates::{substrate_on, Coasts};
    use world::events::thickening::plateau_share_of;
    use world::events::thrusting::Outlines;
    use world::events::tilt::tilt_at;
    use world::hex_to_world;
    let (cx, cy) = hex_to_world(104_289, -4_677);
    let coasts = Coasts::in_box(cx, cy, 6_000.0, SEED);
    let outlines = Outlines::in_box(cx, cy, 6_000.0, SEED);
    let pts: Vec<(f64, f64)> = (0..100).flat_map(|i| (0..100).map(move |j| (cx - 5_000.0 + i as f64 * 100.0, cy - 5_000.0 + j as f64 * 100.0))).collect();
    let n = pts.len() as f64;
    let time = |label: &str, f: &dyn Fn(f64, f64) -> f64| {
        let t = Instant::now();
        let mut acc = 0.0;
        for &(x, y) in &pts {
            acc += f(x, y);
        }
        println!("{label:>16}: {:6.2} µs  (sum {acc:.0})", t.elapsed().as_secs_f64() * 1e6 / n);
    };
    time("substrate", &|x, y| substrate_on(x, y, &coasts, SEED));
    time("tilt", &|x, y| tilt_at(x, y, 10.0, SEED));
    time("outlines.at", &|x, y| outlines.at(x, y).map_or(0.0, |s| s.distances[0]));
    time("relief_of", &|x, y| outlines.at(x, y).map_or(0.0, |s| outlines.relief_of(&s)));
    time("plateau", &|x, y| outlines.at(x, y).map_or(0.0, |s| plateau_share_of(s.plate, &s.distances)));
    time("rock_at", &|x, y| outlines.at(x, y).map_or(0.0, |s| rock_at(x, y, SEED, s.plate.id, s.plate.age, 10.0, 50.0).stand));
    time("ground_at", &|x, y| ground_at(x, y, SEED, &coasts, &outlines).surface);
}

/// What `Outlines::at` is made of over the same patch: the warp, the seed
/// contest, the home plate's distances, and how often the home plate is
/// not the one and its neighbours are tried.
#[test]
#[ignore]
fn outline_lookup_costs() {
    use world::events::plates::warp;
    use world::events::thrusting::Outlines;
    use world::tectonic::plate_at;
    use world::hex_to_world;
    let (cx, cy) = hex_to_world(104_289, -4_677);
    let outlines = Outlines::in_box(cx, cy, 6_000.0, SEED);
    let pts: Vec<(f64, f64)> = (0..100).flat_map(|i| (0..100).map(move |j| (cx - 5_000.0 + i as f64 * 100.0, cy - 5_000.0 + j as f64 * 100.0))).collect();
    let n = pts.len() as f64;
    let t = Instant::now();
    let mut acc = 0.0;
    for &(x, y) in &pts { acc += warp(x, y, SEED).0; }
    println!("{:>16}: {:6.2} µs ({acc:.0})", "warp", t.elapsed().as_secs_f64() * 1e6 / n);
    let t = Instant::now();
    let mut acc = 0i64;
    for &(x, y) in &pts { acc += plate_at(x, y, SEED).id.0 as i64; }
    println!("{:>16}: {:6.2} µs ({acc})", "plate_at", t.elapsed().as_secs_f64() * 1e6 / n);
    let t = Instant::now();
    let mut acc = 0.0;
    let mut swapped = 0;
    for &(x, y) in &pts {
        let (wx, wy) = warp(x, y, SEED);
        let home = plate_at(wx, wy, SEED);
        let Some(p) = outlines.plate(home.id) else { continue };
        let (d, inside) = p.distances(wx, wy);
        acc += d[0];
        if !inside { swapped += 1 }
    }
    println!("{:>16}: {:6.2} µs ({acc:.0}); {swapped} of {} outside the seed's plate", "home distances", t.elapsed().as_secs_f64() * 1e6 / n, pts.len());
    let segments: usize = outlines.plates().map(|p| p.edges.iter().map(|e| e.segments.len()).sum::<usize>()).sum();
    let plates = outlines.plates().count();
    println!("{:>16}: {plates} plates, {segments} segments, {:.0} per plate", "outlines", segments as f64 / plates as f64);
}

/// What the server pays per far summary: the 7-point sample of scale-243
/// summaries in a sweep of fresh ground 20–50 km out, as the outermost
/// band asks for it at login. Marginal cost per sample, and the cells it
/// deforms on the way.
#[test]
#[ignore]
fn far_band_sample_costs() {
    let c = composite_full();
    let scale = 243_i32;
    let d = scale / 3;
    let offsets = [(0, 0), (d, 0), (-d, 0), (0, d), (0, -d), (d, -d), (-d, d)];
    // Summary centres on the scale-243 lattice along a line out from the origin.
    let mut samples = 0usize;
    let t = Instant::now();
    for k in 60..120 {
        for j in -3..=3 {
            let (sq, sr) = (k, j);
            let (cq, cr) = (sq * scale, sr * scale);
            let mut z = i32::MIN;
            for (oq, or) in offsets {
                z = z.max(c.elevation_at(cq + oq, cr + or));
                samples += 1;
            }
            let _ = z;
        }
    }
    let secs = t.elapsed().as_secs_f64();
    println!("far band: {samples} samples in {secs:.2} s = {:.0} µs/sample", secs * 1e6 / samples as f64);
    report_metrics(&c, "far band");
}
