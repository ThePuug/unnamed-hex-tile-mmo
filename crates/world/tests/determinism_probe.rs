//! Determinism probe — requires the same seed to produce the same world.
//! Checks that a tile's elevation does not depend on what was touched first.

//! Run: cargo test -p world --release --test determinism_probe -- --ignored --nocapture

use std::sync::Arc;

use world::events::Composite;
use world::events::plates::PlateEvent;
use world::events::slope_form::SlopeFormEvent;
use world::events::spines::SpineEvent;
use world::PlateCache;

const SEED: u64 = 0x9E3779B97F4A7C15;

fn composite() -> Composite {
    let plate_cache = Arc::new(PlateCache::new(SEED));
    let mut c = Composite::new(SEED);
    c.add_event(Box::new(PlateEvent::with_cache(plate_cache.clone())));
    c.add_event(Box::new(SpineEvent::with_cache(plate_cache, SEED)));
    c.add_event(Box::new(SlopeFormEvent::new()));
    c
}

/// SpineEvent::query reads its own cell plus a 1-ring of neighbours from
/// SpineInstanceIndex, but tile_at only guarantees the tile's *own* cell is
/// deformed. If a neighbour cell is cold, its instances are missing and the
/// tile reads lower. Sweep a wide grid two ways to see whether that bites:
/// one composite for everything (neighbours warm from earlier tiles) vs a
/// fresh composite per tile (neighbours always cold).
#[test]
#[ignore]
fn spine_cell_boundaries_agree() {
    println!("\n=== spine-cell boundary agreement ===\n");

    // SPINE cell scale is 15,225 tiles. Step across several cells so plenty of
    // probes land near a boundary.
    const STEP: i32 = 1200;
    const N: i32 = 26;
    let origin = -(N / 2) * STEP;

    let shared = composite();
    let mut mismatches = 0u32;
    let mut checked = 0u32;
    let mut worst = 0i32;

    for i in 0..N {
        for j in 0..N {
            let (q, r) = (origin + i * STEP, origin + j * STEP);
            let shared_z = shared.elevation_at(q, r);
            let fresh_z = composite().elevation_at(q, r);
            checked += 1;
            if shared_z != fresh_z {
                mismatches += 1;
                worst = worst.max((shared_z - fresh_z).abs());
                if mismatches <= 8 {
                    println!("  MISMATCH ({q:>7},{r:>7}): shared z={shared_z:<6} fresh z={fresh_z}");
                }
            }
        }
        println!("  row {}/{N} — {mismatches} mismatches so far", i + 1);
    }

    println!("\n  {checked} probes across ~{} tiles", N * STEP);
    println!("  mismatches: {mismatches} ({:.2}%)", 100.0 * mismatches as f64 / checked as f64);
    println!("  largest disagreement: {worst} z");

    assert_eq!(
        mismatches, 0,
        "terrain depends on access order — client and server will disagree \
         for the same seed (largest gap {worst} z)"
    );
}

/// Same tile, different access history. Any disagreement means client and
/// server can render different terrain for the same seed.
#[test]
#[ignore]
fn elevation_independent_of_access_order() {
    println!("\n=== elevation vs access order ===\n");

    // Probe tiles spread across a spine and its surroundings.
    let probes: Vec<(i32, i32)> = (0..60)
        .map(|i| {
            let a = i as f64 * 2.399963;
            let rad = 400.0 * (i as f64).sqrt();
            (1437 + (rad * a.cos()) as i32, 8362 + (rad * a.sin()) as i32)
        })
        .collect();

    // Baseline: fresh composite per probe, nothing else touched.
    let baseline: Vec<i32> = probes
        .iter()
        .map(|&(q, r)| composite().elevation_at(q, r))
        .collect();

    // Warmed: one composite that has already walked a wide area first.
    let warm = composite();
    for i in 0..40 {
        for j in 0..40 {
            warm.elevation_at(-20000 + i * 1000, -20000 + j * 1000);
        }
    }
    let warmed: Vec<i32> = probes.iter().map(|&(q, r)| warm.elevation_at(q, r)).collect();

    // Reverse order on a third composite.
    let rev = composite();
    let mut reversed = vec![0; probes.len()];
    for (idx, &(q, r)) in probes.iter().enumerate().rev() {
        reversed[idx] = rev.elevation_at(q, r);
    }

    let mut warm_diffs = 0;
    let mut rev_diffs = 0;
    for (i, &(q, r)) in probes.iter().enumerate() {
        if baseline[i] != warmed[i] {
            warm_diffs += 1;
            println!(
                "  MISMATCH (warm)    ({q:>6},{r:>6}): fresh z={:<6} warmed z={}",
                baseline[i], warmed[i]
            );
        }
        if baseline[i] != reversed[i] {
            rev_diffs += 1;
            println!(
                "  MISMATCH (reverse) ({q:>6},{r:>6}): fresh z={:<6} reverse z={}",
                baseline[i], reversed[i]
            );
        }
    }

    println!("\n  {} probes", probes.len());
    println!("  fresh vs warmed composite:   {warm_diffs} mismatches");
    println!("  fresh vs reverse-order walk: {rev_diffs} mismatches");
    if warm_diffs == 0 && rev_diffs == 0 {
        println!("  -> elevation is access-order independent");
    }
}
