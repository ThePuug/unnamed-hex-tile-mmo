//! Substrate probe — characterises the crustal substrate that replaced the
//! land mask, so its curve is designed against measured data rather than
//! guessed, and so a later retune can see what it is preserving.

//! Run: cargo test -p world --release --test substrate_probe -- --ignored --nocapture

use world::{raw_regime_noise, REGIONAL_MOD_MAX};

const SEED: u64 = 0x9E3779B97F4A7C15;

/// The raw regime value at the sea-level datum, before it became a constant.
/// Kept here so the probe can run against both sides of the rewrite.
const SHORE_RAW: f64 = 0.2566348724376276;

fn pct(v: &[f64], p: f64) -> f64 {
    let i = ((v.len() - 1) as f64 * p).round() as usize;
    v[i]
}

/// How far above the shoreline does the raw field actually reach? The positive
/// branch needs a divisor, and the theoretical bound (REGIONAL_MOD_MAX, since
/// local and gate are both at most 1) is only usable if the field gets near it.
#[test]
#[ignore]
fn raw_headroom_above_shore() {
    const N: i32 = 400;
    const STEP: f64 = 125.0;
    let origin = -(N as f64) * STEP * 0.5;

    println!("\n=== raw field headroom above the shoreline ===\n");
    println!("SHORE_RAW          = {SHORE_RAW:.6}");
    println!("REGIONAL_MOD_MAX   = {REGIONAL_MOD_MAX} (theoretical raw max)");

    let mut land: Vec<f64> = Vec::new();
    let mut all: Vec<f64> = Vec::new();
    for i in 0..N {
        for j in 0..N {
            let wx = origin + i as f64 * STEP;
            let wy = origin + j as f64 * STEP;
            let v = raw_regime_noise(wx, wy, SEED);
            all.push(v);
            if v >= SHORE_RAW { land.push(v) }
        }
    }
    all.sort_by(|a, b| a.partial_cmp(b).unwrap());
    land.sort_by(|a, b| a.partial_cmp(b).unwrap());

    println!("\nland fraction: {:.2}%", 100.0 * land.len() as f64 / all.len() as f64);
    println!("all  raw: p50 {:.4}  p90 {:.4}  p99 {:.4}  max {:.4}",
             pct(&all, 0.5), pct(&all, 0.9), pct(&all, 0.99), all[all.len() - 1]);
    println!("land raw: p50 {:.4}  p90 {:.4}  p99 {:.4}  max {:.4}",
             pct(&land, 0.5), pct(&land, 0.9), pct(&land, 0.99), land[land.len() - 1]);

    println!("\n-- headroom above shore, as a fraction of (max - shore) --");
    for (label, divisor) in [
        ("REGIONAL_MOD_MAX", REGIONAL_MOD_MAX - SHORE_RAW),
        ("observed max", land[land.len() - 1] - SHORE_RAW),
        ("land p99", pct(&land, 0.99) - SHORE_RAW),
    ] {
        let fracs: Vec<f64> = land.iter().map(|v| ((v - SHORE_RAW) / divisor).min(1.0)).collect();
        let mean: f64 = fracs.iter().sum::<f64>() / fracs.len() as f64;
        let mut s = fracs.clone();
        s.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!("  {label:<18} divisor {divisor:.4}  mean frac {mean:.3}  \
                  p50 {:.3}  p90 {:.3}  max {:.3}",
                 pct(&s, 0.5), pct(&s, 0.9), s[s.len() - 1]);
    }
}

/// Candidate positive-branch curves, scored against the terrain shader's
/// elevation ramp — the game's own statement of what a height reads as.
/// Ramp stops: 0 shore water, 10 sand, 30 green, 150 green, 400 dry olive,
/// 700 brown, 2000 rock, 4000 snow.
#[test]
#[ignore]
fn positive_branch_candidates() {
    const N: i32 = 400;
    const STEP: f64 = 125.0;
    let origin = -(N as f64) * STEP * 0.5;

    // SEA_MAX_DEPTH x Earth's mean-land / mean-ocean-depth ratio (840 m / 3700 m).
    let max_rise = 200.0 * (840.0 / 3700.0);
    println!("\n=== positive branch candidates ===\n");
    println!("CONTINENT_MAX_RISE = {max_rise:.2} z");

    let mut land: Vec<f64> = Vec::new();
    let mut n_all = 0usize;
    for i in 0..N {
        for j in 0..N {
            let wx = origin + i as f64 * STEP;
            let wy = origin + j as f64 * STEP;
            let v = raw_regime_noise(wx, wy, SEED);
            n_all += 1;
            if v >= SHORE_RAW { land.push(v) }
        }
    }
    println!("land fraction {:.2}%  ({} of {n_all} samples)\n",
             100.0 * land.len() as f64 / n_all as f64, land.len());

    let headroom_bound = REGIONAL_MOD_MAX - SHORE_RAW;
    let shore_relative = SHORE_RAW;

    println!("{:<34} {:>6} {:>6} {:>6} {:>6}   {:>6} {:>6} {:>6} {:>6}",
             "curve", "p10", "p50", "p90", "max", "water", "sand", "green", "dry+");
    for (label, divisor, exp) in [
        ("bound, linear",        headroom_bound, 1.0),
        ("bound, ^0.75",         headroom_bound, 0.75),
        ("bound, ^0.5",          headroom_bound, 0.5),
        ("bound, ^2 (shelf)",    headroom_bound, 2.0),
        ("shore-relative, linear", shore_relative, 1.0),
        ("shore-relative, ^2",   shore_relative, 2.0),
    ] {
        let mut e: Vec<f64> = land.iter()
            .map(|v| max_rise * ((v - SHORE_RAW) / divisor).clamp(0.0, 1.0).powf(exp))
            .collect();
        e.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let n = e.len() as f64;
        let band = |lo: f64, hi: f64| 100.0 * e.iter().filter(|&&x| x >= lo && x < hi).count() as f64 / n;
        println!("{label:<34} {:>6.1} {:>6.1} {:>6.1} {:>6.1}   {:>5.1}% {:>5.1}% {:>5.1}% {:>5.1}%",
                 pct(&e, 0.10), pct(&e, 0.50), pct(&e, 0.90), e[e.len() - 1],
                 band(0.0, 10.0), band(10.0, 30.0), band(30.0, 150.0), band(150.0, 1e9));
    }
    println!("\nwater = below the sand stop; sand = 10-30; green = 30-150; dry+ = 150+");
}

/// World census — the numbers a substrate change is allowed to move a little
/// and not a lot. Plate density is world scale: it must not drift when the
/// field the suppression reads changes shape.
#[test]
#[ignore]
fn world_census() {
    use world::{PlateCache, generate_spines};

    const SEEDS: [u64; 6] = [
        0x9E3779B97F4A7C15,
        0x0123_4567_89AB_CDEF,
        0xDEAD_BEEF_CAFE_1235,
        0x5555_AAAA_3333_CCC1,
        0xF0E1_D2C3_B4A5_9687,
        0x0000_0000_0000_0001,
    ];
    const RADIUS: f64 = 30_000.0;

    println!("\n=== world census ===\n");
    println!("{:<20} {:>8} {:>10} {:>8} {:>10}",
             "seed", "plates", "land %", "spines", "peaks");
    let mut totals = (0usize, 0usize, 0usize);
    for seed in SEEDS {
        let cache = PlateCache::new(seed);
        let mut plates = cache.plates_in_radius(0.0, 0.0, RADIUS);
        let n_plates = plates.len();

        // Land fraction over a coarse grid in the same disc.
        const N: i32 = 220;
        const STEP: f64 = 250.0;
        let origin = -(N as f64) * STEP * 0.5;
        let mut land = 0usize;
        let mut total = 0usize;
        for i in 0..N {
            for j in 0..N {
                let wx = origin + i as f64 * STEP;
                let wy = origin + j as f64 * STEP;
                total += 1;
                if raw_regime_noise(wx, wy, seed) >= SHORE_RAW { land += 1 }
            }
        }

        let spines = generate_spines(&mut plates, &cache, seed);
        let peaks: usize = spines.iter().map(|s| s.peaks.len()).sum();

        println!("{seed:#018x} {n_plates:>8} {:>9.2}% {:>8} {peaks:>10}",
                 100.0 * land as f64 / total as f64, spines.len());
        totals.0 += n_plates;
        totals.1 += spines.len();
        totals.2 += peaks;
    }
    println!("{:<20} {:>8} {:>10} {:>8} {:>10}",
             "TOTAL", totals.0, "", totals.1, totals.2);
}

/// Spine epicentre positions, so a rekey can be checked for drift rather than
/// only for count.
#[test]
#[ignore]
fn spine_positions() {
    use world::{PlateCache, generate_spines};
    const SEED2: u64 = 0x9E3779B97F4A7C15;
    let cache = PlateCache::new(SEED2);
    let mut plates = cache.plates_in_radius(0.0, 0.0, 30_000.0);
    let spines = generate_spines(&mut plates, &cache, SEED2);
    println!("\n=== spine epicentres ({}) ===\n", spines.len());
    let mut centers: Vec<(i64, i64, usize)> = spines.iter()
        .map(|s| (s.bounding_center.0.round() as i64,
                  s.bounding_center.1.round() as i64,
                  s.peaks.len()))
        .collect();
    centers.sort();
    for (x, y, n) in &centers {
        println!("  ({x:>8}, {y:>8})  {n} peaks");
    }
}

/// How many tiles does the shore-to-deep transition span, on each side?
/// This is what decides whether a coastline is a beach or a cliff, and it is
/// the one measurement the depth curve was originally designed against — kept
/// so the land branch added beside it can be read on the same scale.
#[test]
#[ignore]
fn coastal_profile_width() {
    use world::{hex_to_world, substrate_elevation_at, SEA_MAX_DEPTH, CONTINENT_MAX_RISE};

    println!("\n=== coast-to-interior and coast-to-abyss width, in tiles ===\n");
    let targets = [0.25f64, 0.50, 0.90];

    let mut sea: [Vec<i32>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    let mut land: [Vec<i32>; 3] = [Vec::new(), Vec::new(), Vec::new()];

    for line in 0..6 {
        let r = -9000 + line * 3000;
        let mut prev_land: Option<bool> = None;
        let mut q = -20000;
        while q < 20000 {
            let (wx, wy) = hex_to_world(q, r);
            let is_land = substrate_elevation_at(wx, wy, SEED) >= 0.0;
            // March away from every crossing, on whichever side it opens onto.
            if let Some(prev) = prev_land {
                if prev != is_land {
                    let (widths, sign, scale) = if is_land {
                        (&mut land, 1.0, CONTINENT_MAX_RISE)
                    } else {
                        (&mut sea, -1.0, SEA_MAX_DEPTH)
                    };
                    let mut hit = [false; 3];
                    for d in 0..6000 {
                        let (sx, sy) = hex_to_world(q + d, r);
                        let e = substrate_elevation_at(sx, sy, SEED) * sign;
                        if e < 0.0 { break } // crossed back over the datum
                        let frac = (e / scale).clamp(0.0, 1.0);
                        for (k, &t) in targets.iter().enumerate() {
                            if !hit[k] && frac >= t {
                                hit[k] = true;
                                widths[k].push(d);
                            }
                        }
                        if hit.iter().all(|&h| h) { break }
                    }
                }
            }
            prev_land = Some(is_land);
            q += 1;
        }
    }

    for (label, widths) in [("seaward", &sea), ("landward", &land)] {
        println!("  {label}:");
        for (k, &t) in targets.iter().enumerate() {
            let mut v = widths[k].clone();
            v.sort_unstable();
            if v.is_empty() {
                println!("    frac {t:.2}: never reached");
                continue;
            }
            let n = v.len();
            println!(
                "    tiles from shore to frac {t:.2}:  median {:>5}  p25 {:>5}  p75 {:>5}  (n={n})",
                v[n / 2], v[n / 4], v[n * 3 / 4],
            );
        }
    }
    println!("\n  (player view radius is ~400 tiles; they cover 4.33 tiles/sec)");
}
