//! Regime field probe — characterises the land/water field so a bathymetry
//! curve can be designed against real data rather than guessed.
//!
//! Run: cargo test -p world --release --test regime_probe -- --ignored --nocapture

use std::sync::Arc;

use common::PlateTag;
use world::events::Composite;
use world::events::plates::PlateEvent;
use world::events::spines::SpineEvent;
use world::{hex_to_world, raw_regime_noise, regime_value_at, PlateCache, REGIME_LAND_THRESHOLD};

const SEED: u64 = 0x9E3779B97F4A7C15;

/// Distribution of raw regime values, and how the sigmoid distributes them.
#[test]
#[ignore]
fn regime_distribution() {
    println!("\n=== regime field distribution ===\n");
    println!("REGIME_LAND_THRESHOLD = {REGIME_LAND_THRESHOLD}");

    const N: i32 = 200;
    const STEP: i32 = 200; // 40,000 tile span
    let origin = -(N / 2) * STEP;

    let mut water: Vec<f64> = Vec::new();
    let mut land: Vec<f64> = Vec::new();

    for i in 0..N {
        for j in 0..N {
            let (wx, wy) = hex_to_world(origin + i * STEP, origin + j * STEP);
            let v = regime_value_at(wx, wy, SEED);
            if v < REGIME_LAND_THRESHOLD { water.push(v) } else { land.push(v) }
        }
    }

    let total = water.len() + land.len();
    println!("\nsamples: {total}   water {:.1}%   land {:.1}%",
        100.0 * water.len() as f64 / total as f64,
        100.0 * land.len() as f64 / total as f64);

    for (label, mut v) in [("water", water), ("land", land)] {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        if v.is_empty() { continue }
        let n = v.len();
        println!(
            "  {label:>5}: min {:.4}  p10 {:.4}  p25 {:.4}  median {:.4}  p75 {:.4}  p90 {:.4}  max {:.4}",
            v[0], v[n / 10], v[n / 4], v[n / 2], v[n * 3 / 4], v[n * 9 / 10], v[n - 1],
        );
    }

    // How much of the water band is usably graded vs saturated at the floor?
    let mut buckets = [0u32; 6];
    let mut count = 0u32;
    for i in 0..N {
        for j in 0..N {
            let (wx, wy) = hex_to_world(origin + i * STEP, origin + j * STEP);
            let v = regime_value_at(wx, wy, SEED);
            if v >= REGIME_LAND_THRESHOLD { continue }
            count += 1;
            let frac = v / REGIME_LAND_THRESHOLD; // 0 = deepest, 1 = shoreline
            let b = ((frac * 6.0) as usize).min(5);
            buckets[b] += 1;
        }
    }
    println!("\n  water samples by depth fraction (0 = far from land, 1 = shoreline):");
    for (k, n) in buckets.iter().enumerate() {
        println!("    {:.2}-{:.2}: {:5.1}%  ({n})",
            k as f64 / 6.0, (k + 1) as f64 / 6.0,
            100.0 * *n as f64 / count.max(1) as f64);
    }
}

/// The pre-sigmoid field as a bathymetry driver. The sigmoid saturates the
/// water band; the raw field should still be graded.
#[test]
#[ignore]
fn raw_regime_as_depth() {
    println!("\n=== pre-sigmoid regime as a depth field ===\n");
    println!("REGIME_SIGMOID_MIDPOINT = 0.30 (raw value at the shoreline)");

    const N: i32 = 200;
    const STEP: i32 = 200;
    let origin = -(N / 2) * STEP;

    let mut water_raw: Vec<f64> = Vec::new();
    for i in 0..N {
        for j in 0..N {
            let (wx, wy) = hex_to_world(origin + i * STEP, origin + j * STEP);
            if regime_value_at(wx, wy, SEED) < REGIME_LAND_THRESHOLD {
                water_raw.push(raw_regime_noise(wx, wy, SEED));
            }
        }
    }

    water_raw.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = water_raw.len();
    println!(
        "  water raw regime: min {:.4}  p10 {:.4}  p25 {:.4}  median {:.4}  p75 {:.4}  p90 {:.4}  max {:.4}",
        water_raw[0], water_raw[n / 10], water_raw[n / 4], water_raw[n / 2],
        water_raw[n * 3 / 4], water_raw[n * 9 / 10], water_raw[n - 1],
    );

    // Shore raw value ~= the sigmoid midpoint. Normalise depth against it.
    const SHORE_RAW: f64 = 0.2426; // raw value where sigmoid(x) == REGIME_LAND_THRESHOLD
    let mut buckets = [0u32; 8];
    for v in &water_raw {
        let frac = (1.0 - v / SHORE_RAW).clamp(0.0, 1.0); // 0 = shore, 1 = deepest
        let b = ((frac * 8.0) as usize).min(7);
        buckets[b] += 1;
    }
    println!("\n  water by normalised depth (0 = shore, 1 = abyss):");
    for (k, c) in buckets.iter().enumerate() {
        println!("    {:.2}-{:.2}: {:5.1}%  ({c})",
            k as f64 / 8.0, (k + 1) as f64 / 8.0,
            100.0 * *c as f64 / n as f64);
    }
}

/// How many tiles does the shore-to-deep transition span? This decides whether
/// a depth curve produces a beach or a cliff.
#[test]
#[ignore]
fn shelf_width() {
    println!("\n=== continental shelf width in tiles ===\n");
    const SHORE_RAW: f64 = 0.2426;

    // Walk several long transects, find every land->water crossing, then march
    // seaward measuring how far to reach each depth fraction.
    let mut widths: [Vec<i32>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    let targets = [0.25f64, 0.50, 0.90];

    for line in 0..6 {
        let r = -9000 + line * 3000;
        let mut prev_land = None;
        let mut q = -20000;
        while q < 20000 {
            let (wx, wy) = hex_to_world(q, r);
            let is_land = regime_value_at(wx, wy, SEED) >= REGIME_LAND_THRESHOLD;
            if prev_land == Some(true) && !is_land {
                // Shore crossing at q, heading seaward (+q).
                let mut hit = [false; 3];
                for d in 0..6000 {
                    let (sx, sy) = hex_to_world(q + d, r);
                    if regime_value_at(sx, sy, SEED) >= REGIME_LAND_THRESHOLD {
                        break; // hit land again, not open ocean
                    }
                    let frac = (1.0 - raw_regime_noise(sx, sy, SEED) / SHORE_RAW).clamp(0.0, 1.0);
                    for (k, &t) in targets.iter().enumerate() {
                        if !hit[k] && frac >= t {
                            hit[k] = true;
                            widths[k].push(d);
                        }
                    }
                    if hit.iter().all(|&h| h) { break }
                }
            }
            prev_land = Some(is_land);
            q += 1;
        }
    }

    for (k, &t) in targets.iter().enumerate() {
        let v = &mut widths[k];
        v.sort_unstable();
        if v.is_empty() {
            println!("  depth frac {t:.2}: never reached");
            continue;
        }
        let n = v.len();
        println!(
            "  tiles from shore to depth frac {t:.2}:  median {:>5}  p25 {:>5}  p75 {:>5}  (n={n})",
            v[n / 2], v[n / 4], v[n * 3 / 4],
        );
    }
    println!("\n  (player view radius is ~400 tiles; they cover 4.33 tiles/sec)");
}

/// Does the coarse plate tag agree with the fine per-tile regime value?
/// Tags come from the plate centroid; the real coastline is the regime contour.
#[test]
#[ignore]
fn tag_vs_regime_agreement() {
    println!("\n=== plate tag vs per-tile regime ===\n");

    let plate_cache = Arc::new(PlateCache::new(SEED));
    let mut c = Composite::new(SEED);
    c.add_event(Box::new(PlateEvent::with_cache(plate_cache.clone())));
    c.add_event(Box::new(SpineEvent::with_cache(plate_cache, SEED)));

    const N: i32 = 120;
    const STEP: i32 = 60; // 7,200 tile span, tile-scale detail
    let origin = -(N / 2) * STEP;

    let mut agree = 0u32;
    let mut land_tagged_sea = 0u32;
    let mut sea_tagged_land = 0u32;
    let mut coast_over_water = 0u32;
    let mut coast_total = 0u32;
    let mut total = 0u32;

    for i in 0..N {
        for j in 0..N {
            let (q, r) = (origin + i * STEP, origin + j * STEP);
            let (wx, wy) = hex_to_world(q, r);
            let regime_is_land = regime_value_at(wx, wy, SEED) >= REGIME_LAND_THRESHOLD;
            let tags = c.tags_at(q, r);
            total += 1;

            if tags.has(PlateTag::Coast) {
                coast_total += 1;
                if !regime_is_land { coast_over_water += 1 }
                continue;
            }
            let tag_is_land = tags.has(PlateTag::Inland);
            if tag_is_land == regime_is_land {
                agree += 1;
            } else if regime_is_land {
                land_tagged_sea += 1;
            } else {
                sea_tagged_land += 1;
            }
        }
    }

    let pct = |n: u32| 100.0 * n as f64 / total as f64;
    println!("  samples: {total} over {}x{} tiles", N * STEP, N * STEP);
    println!("  Sea/Inland tag agrees with regime:  {:.1}%  ({agree})", pct(agree));
    println!("  regime says land, tagged Sea:       {:.1}%  ({land_tagged_sea})", pct(land_tagged_sea));
    println!("  regime says water, tagged Inland:   {:.1}%  ({sea_tagged_land})", pct(sea_tagged_land));
    println!("  tagged Coast:                       {:.1}%  ({coast_total})", pct(coast_total));
    println!("    of which regime says water:       {:.1}% of coast",
        100.0 * coast_over_water as f64 / coast_total.max(1) as f64);
}
