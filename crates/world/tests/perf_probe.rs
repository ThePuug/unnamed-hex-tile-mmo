//! Performance probe for the Composite terrain pipeline.
//!
//! Not a pass/fail test — prints timing data for the production 3-event stack
//! under realistic access patterns (chunk materialization, sparse LoD sampling,
//! dense flyover regions).
//!
//! Run: cargo test -p world --release --test perf_probe -- --ignored --nocapture

use std::sync::Arc;
use std::time::Instant;

use world::events::Composite;
use world::events::plates::PlateEvent;
use world::events::spawner::SpawnerEvent;
use world::events::spines::SpineEvent;
use world::PlateCache;

const SEED: u64 = 0x9E3779B97F4A7C15;

fn composite_full() -> Composite {
    let plate_cache = Arc::new(PlateCache::new(SEED));
    let mut c = Composite::new(SEED);
    c.add_event(Box::new(PlateEvent::with_cache(plate_cache.clone())));
    c.add_event(Box::new(SpineEvent::with_cache(plate_cache, SEED)));
    c.add_event(Box::new(SpawnerEvent::new(SEED)));
    c
}

fn composite_no_spawner() -> Composite {
    let plate_cache = Arc::new(PlateCache::new(SEED));
    let mut c = Composite::new(SEED);
    c.add_event(Box::new(PlateEvent::with_cache(plate_cache.clone())));
    c.add_event(Box::new(SpineEvent::with_cache(plate_cache, SEED)));
    c
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
/// (1 plate cell) vs full-stack first touch (169 plate cells + grow_spine).
#[test]
#[ignore]
fn perf_probe_cascade_breakdown() {
    // 1 plate-cell deform alone
    let plate_cache = Arc::new(PlateCache::new(SEED));
    let mut c = Composite::new(SEED);
    c.add_event(Box::new(PlateEvent::with_cache(plate_cache.clone())));
    let t = Instant::now();
    c.tile_at(3000, 2000);
    println!("plate-only first_touch (1 plate cell deform): {:?}", t.elapsed());

    // Full stack on a fresh composite: 169 plate cells + spine deform
    let c = composite_full();
    let t = Instant::now();
    c.tile_at(3000, 2000);
    println!("full-stack first_touch (169 plate cells + grow_spine): {:?}", t.elapsed());

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
    // 100 samples in a 10x10 grid, 200 tiles apart: every sample is in a
    // distinct spawner cell; spine/plate cells are shared by several samples.
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
            "sparse_100 @ spacing 200 (full stack): {dt:?} ({:?}/sample)",
            dt / 100
        );
        report_metrics(&c, "sparse200");
    }

    // Same pattern, no spawner event — isolates spawner-cell survey cost.
    {
        let c = composite_no_spawner();
        let t = Instant::now();
        for i in 0..100 {
            let q = (i % 10) * 200;
            let r = (i / 10) * 200;
            c.elevation_at(q, r);
        }
        let dt = t.elapsed();
        println!(
            "sparse_100 @ spacing 200 (no spawner):  {dt:?} ({:?}/sample)",
            dt / 100
        );
        report_metrics(&c, "sparse200-nospawn");
    }
    println!();

    // ── 5. Sparse sampling, 2000-tile spacing (far-band / flyover pattern) ─
    // Every sample in a distinct spine cell AND distinct plate region.
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
