//! Relief probe — quantifies how much terrain variation a player actually sees.

//! Not a pass/fail test. Answers: what fraction of the world is flat, how far
//! apart are features, and what is the elevation range inside one screen?

//! Run: cargo test -p world --release --test relief_probe -- --ignored --nocapture

use std::sync::Arc;
use std::time::Instant;

use common::PlateTag;
use world::events::Composite;
use world::events::plates::PlateEvent;
use world::events::sea::SeaEvent;
use world::events::slope_form::SlopeFormEvent;
use world::events::spines::SpineEvent;
use world::{hex_to_world, regime_value_at, PlateCache, REGIME_LAND_THRESHOLD};

const SEED: u64 = 0x9E3779B97F4A7C15;

/// Tiles per second at MOVEMENT_SPEED 0.0075 WU/ms with hex radius 1.0
/// (neighbour spacing sqrt(3) WU): 7.5 / 1.732.
const TILES_PER_SEC: f64 = 4.33;

fn composite() -> Composite {
    let plate_cache = Arc::new(PlateCache::new(SEED));
    let mut c = Composite::new(SEED);
    c.add_event(Box::new(PlateEvent::with_cache(plate_cache.clone())));
    c.add_event(Box::new(SeaEvent::new()));
    c.add_event(Box::new(SpineEvent::with_cache(plate_cache, SEED)));
    c.add_event(Box::new(SlopeFormEvent::new()));
    c
}

fn minutes(tiles: f64) -> f64 {
    tiles / TILES_PER_SEC / 60.0
}

/// Coarse world census: elevation + tag distribution over a continental span.
#[test]
#[ignore]
fn relief_census() {
    println!("\n=== world census ===\n");
    let c = composite();

    // 81x81 samples, 500 tiles apart -> 40,000 x 40,000 tile span
    // (~2.6 spine exclusion distances across; ~154 min of running edge to edge)
    const N: i32 = 81;
    const STEP: i32 = 500;
    let origin = -(N / 2) * STEP;

    let mut z_hist: Vec<u32> = vec![0; 13];
    let mut zero = 0u32;
    let mut land = 0u32;
    let mut tag_counts = [0u32; 6];
    let mut max_z = 0i32;
    let mut min_z = 0i32;
    let total = (N * N) as u32;

    let t = Instant::now();
    for i in 0..N {
        for j in 0..N {
            let q = origin + i * STEP;
            let r = origin + j * STEP;
            let view = c.tile_at(q, r);
            let z = view.elevation.round() as i32;
            if z == 0 {
                zero += 1;
            }
            max_z = max_z.max(z);
            min_z = min_z.min(z);
            let bucket = match z {
                i32::MIN..=-150 => 0,
                -149..=-100 => 1,
                -99..=-50 => 2,
                -49..=-10 => 3,
                -9..=-1 => 4,
                0 => 5,
                1..=9 => 6,
                10..=49 => 7,
                50..=199 => 8,
                200..=499 => 9,
                500..=999 => 10,
                1000..=1999 => 11,
                _ => 12,
            };
            z_hist[bucket] += 1;

            let (wx, wy) = hex_to_world(q, r);
            if regime_value_at(wx, wy, SEED) >= REGIME_LAND_THRESHOLD {
                land += 1;
            }
            for (k, tag) in [
                PlateTag::Sea,
                PlateTag::Coast,
                PlateTag::Inland,
                PlateTag::Ridge,
                PlateTag::Highland,
                PlateTag::Foothills,
            ]
            .iter()
            .enumerate()
            {
                if view.tags.has(*tag) {
                    tag_counts[k] += 1;
                }
            }
        }
        if i % 20 == 0 {
            println!("  row {i}/{N} elapsed {:?}", t.elapsed());
        }
    }

    let pct = |n: u32| 100.0 * n as f64 / total as f64;
    println!("\nsamples: {total} over {}x{} tiles in {:?}", N * STEP, N * STEP, t.elapsed());
    println!("  land (regime >= threshold): {:.1}%", pct(land));
    println!("  elevation == 0 (dead flat): {:.1}%", pct(zero));
    println!("  elevation range: z={min_z} .. z={max_z}");
    let labels = [
        "<-150", "-150..-100", "-99..-50", "-49..-10", "-9..-1",
        "z=0", "1-9", "10-49", "50-199", "200-499", "500-999", "1k-2k", "2k+",
    ];
    println!("\n  elevation histogram:");
    for (l, n) in labels.iter().zip(&z_hist) {
        println!("    {l:>8}: {:5.1}%  ({n})", pct(*n));
    }
    println!("\n  tag coverage:");
    for (l, n) in ["Sea", "Coast", "Inland", "Ridge", "Highland", "Foothills"]
        .iter()
        .zip(&tag_counts)
    {
        println!("    {l:>10}: {:5.1}%  ({n})", pct(*n));
    }
}

/// What a player sees from where they stand: elevation spread across one
/// view radius, sampled at many standpoints.
#[test]
#[ignore]
fn local_relief() {
    println!("\n=== local relief (what one screen contains) ===\n");
    let c = composite();

    // View radius: FIXED_STREAM_RADIUS 21 chunks * 19 tiles = 399 tiles.
    // Sample a coarse ring set out to that radius from each standpoint.
    const VIEW: i32 = 400;
    const RINGS: [i32; 4] = [100, 200, 300, 400];
    const SPOKES: usize = 12;

    // Standpoints spread across the same continental span as the census,
    // deliberately including spine interiors and open plains.
    let standpoints: Vec<(i32, i32)> = (0..40)
        .map(|i| {
            let a = i as f64 * 2.399963; // golden-angle spiral
            let rad = 1500.0 * (i as f64).sqrt();
            ((rad * a.cos()) as i32, (rad * a.sin()) as i32)
        })
        .collect();

    let mut flat_screens = 0u32;
    let mut reliefs: Vec<i32> = Vec::new();

    let t = Instant::now();
    for &(cq, cr) in &standpoints {
        let mut lo = i32::MAX;
        let mut hi = i32::MIN;
        let center = c.elevation_at(cq, cr);
        lo = lo.min(center);
        hi = hi.max(center);
        for ring in RINGS {
            for s in 0..SPOKES {
                let a = s as f64 * std::f64::consts::TAU / SPOKES as f64;
                let q = cq + (ring as f64 * a.cos()) as i32;
                let r = cr + (ring as f64 * a.sin()) as i32;
                let z = c.elevation_at(q, r);
                lo = lo.min(z);
                hi = hi.max(z);
            }
        }
        let relief = hi - lo;
        if relief == 0 {
            flat_screens += 1;
        }
        reliefs.push(relief);
        println!("  standpoint ({cq:>7},{cr:>7}): z {lo:>5}..{hi:<5} relief {relief}");
    }

    reliefs.sort_unstable();
    let n = reliefs.len();
    println!("\n  {n} standpoints, view radius {VIEW} tiles, {:?}", t.elapsed());
    println!(
        "  perfectly flat (relief 0): {}/{n}  ({:.0}%)",
        flat_screens,
        100.0 * flat_screens as f64 / n as f64
    );
    println!(
        "  relief median {}  p75 {}  p90 {}  max {}",
        reliefs[n / 2],
        reliefs[n * 3 / 4],
        reliefs[n * 9 / 10],
        reliefs[n - 1]
    );
}

/// Shoreline profile: is the beach wadeable, and does the seabed ever exceed
/// the +1 z per tile climb limit on the way back out of the water?
#[test]
#[ignore]
fn shoreline_profile() {
    println!("\n=== shoreline profile ===\n");
    let c = composite();

    let mut steps_over_limit = 0u32;
    let mut uphill_steps = 0u32;
    let mut max_step = 0i32;
    let mut profiles = 0u32;

    let t = Instant::now();
    for line in 0..6 {
        let r = -9000 + line * 3000;
        let mut prev_z: Option<i32> = None;
        let mut q = -12000;
        while q < 12000 {
            let z = c.elevation_at(q, r);
            if let Some(p) = prev_z {
                // Only count steps climbing out of water — steps on dry land are
                // spine terrain, measured separately by `climbability`.
                let step = z - p;
                if step > 0 && p < 0 {
                    uphill_steps += 1;
                    max_step = max_step.max(step);
                    if step > 1 { steps_over_limit += 1 }
                }
                // Print the profile at the first few shore crossings.
                if p < 0 && z >= 0 && profiles < 4 {
                    profiles += 1;
                    let depths: Vec<i32> = [-400, -300, -200, -100, -50, -25, 0, 25, 50]
                        .iter()
                        .map(|d| c.elevation_at(q + d, r))
                        .collect();
                    println!("  shore at ({q},{r}) — z at -400,-300,-200,-100,-50,-25,0,+25,+50 tiles:");
                    println!("    {depths:?}");
                }
            }
            prev_z = Some(z);
            q += 1;
        }
    }

    println!("\n  6 transects x 24,000 tiles, {:?}", t.elapsed());
    println!("  uphill steps: {uphill_steps}");
    println!(
        "  exceeding the +1 z climb limit: {steps_over_limit} ({:.2}% of uphill)",
        100.0 * steps_over_limit as f64 / uphill_steps.max(1) as f64
    );
    println!("  largest single-tile rise: +{max_step} z");
}

/// Traversability: a player can step up at most +1 z per tile (movement.rs
/// `is_tile_blocked`, "cliff transition"). Walk transects through a spine and
/// count how many steps exceed that.
#[test]
#[ignore]
fn climbability() {
    println!("\n=== climbability of elevated terrain ===\n");
    let c = composite();

    // Transects through the spine found near (1437, 8362) by local_relief.
    let center = (1437, 8362);
    let mut total_steps = 0u32;
    let mut uphill = 0u32;
    let mut blocked = 0u32;
    let mut max_step = 0i32;
    let mut on_slope = 0u32;

    let t = Instant::now();
    for spoke in 0..8 {
        let a = spoke as f64 * std::f64::consts::TAU / 8.0;
        let (dq, dr) = (a.cos(), a.sin());
        let mut prev: Option<i32> = None;
        // Walk inward from 3000 tiles out to the centre, one tile per step.
        for d in (0..3000).rev() {
            let q = center.0 + (d as f64 * dq) as i32;
            let r = center.1 + (d as f64 * dr) as i32;
            let z = c.elevation_at(q, r);
            if let Some(p) = prev {
                let step = z - p;
                total_steps += 1;
                if p > 0 || z > 0 {
                    on_slope += 1;
                }
                if step > 0 {
                    uphill += 1;
                    max_step = max_step.max(step);
                    if step > 1 {
                        blocked += 1;
                    }
                }
            }
            prev = Some(z);
        }
    }

    println!("  8 transects x 3000 tiles, {:?}", t.elapsed());
    println!("  steps sampled: {total_steps} ({on_slope} on non-zero terrain)");
    println!("  uphill steps: {uphill}");
    println!(
        "  uphill steps exceeding +1 z (impassable on foot): {blocked}/{uphill} ({:.1}% of uphill)",
        100.0 * blocked as f64 / uphill.max(1) as f64
    );
    println!("  largest single-tile rise: +{max_step} z");
}

/// Walk a straight line and record how long the player goes without any
/// elevation change — the "how far to the next interesting thing" number.
#[test]
#[ignore]
fn feature_spacing() {
    println!("\n=== feature spacing along a walk ===\n");
    let c = composite();

    const LEN: i32 = 60_000; // tiles walked
    const STEP: i32 = 25; // sample every 25 tiles

    let mut runs: Vec<i32> = Vec::new(); // lengths of dead-flat stretches, in tiles
    let mut current = 0i32;
    let mut nonzero = 0u32;
    let mut samples = 0u32;

    let t = Instant::now();
    let mut q = -LEN / 2;
    while q < LEN / 2 {
        let z = c.elevation_at(q, 0);
        samples += 1;
        if z == 0 {
            current += STEP;
        } else {
            nonzero += 1;
            if current > 0 {
                runs.push(current);
            }
            current = 0;
        }
        q += STEP;
    }
    if current > 0 {
        runs.push(current);
    }

    runs.sort_unstable();
    println!(
        "  walked {LEN} tiles ({:.0} min), {samples} samples, {:?}",
        minutes(LEN as f64),
        t.elapsed()
    );
    println!(
        "  samples with any elevation: {nonzero}/{samples} ({:.1}%)",
        100.0 * nonzero as f64 / samples as f64
    );
    if runs.is_empty() {
        println!("  no flat runs recorded");
        return;
    }
    let n = runs.len();
    println!("  dead-flat stretches: {n}");
    println!(
        "    median {} tiles ({:.1} min), p90 {} tiles ({:.1} min), max {} tiles ({:.1} min)",
        runs[n / 2],
        minutes(runs[n / 2] as f64),
        runs[n * 9 / 10],
        minutes(runs[n * 9 / 10] as f64),
        runs[n - 1],
        minutes(runs[n - 1] as f64),
    );
}

/// The server's hardcoded spawn point (server/src/main.rs) must be dry land.
/// SeaEvent submerges everything below the regime land threshold, and a spawn
/// under the waterline puts the camera beneath the water plane.
#[test]
#[ignore]
fn spawn_point_is_above_water() {
    let c = composite();
    for &(q, r, label) in &[(3423, 1155, "server spawn")] {
        let view = c.tile_at(q, r);
        let tags: Vec<_> = view.tags.iter().collect();
        println!("  {label} ({q},{r}): z={} tags={tags:?}", c.elevation_at(q, r));
        assert!(
            view.elevation >= 0.0,
            "{label} ({q},{r}) is underwater at elevation {:.1} — the camera \
             would start beneath the water plane",
            view.elevation
        );
    }
}

/// How far is the nearest water from the spawn point? Determines whether the
/// ocean is even in frame when the client starts.
#[test]
#[ignore]
fn distance_to_water_from_spawn() {
    let c = composite();
    let (sq, sr) = (3423, 1155);
    let mut nearest = i32::MAX;
    let mut dir = (0, 0);
    for spoke in 0..24 {
        let a = spoke as f64 * std::f64::consts::TAU / 24.0;
        for d in (1..1200).step_by(3) {
            let q = sq + (d as f64 * a.cos()) as i32;
            let r = sr + (d as f64 * a.sin()) as i32;
            if c.elevation_at(q, r) < 0 {
                if d < nearest { nearest = d; dir = (q - sq, r - sr); }
                break;
            }
        }
    }
    if nearest == i32::MAX {
        println!("  no water within 1200 tiles of spawn ({sq},{sr})");
    } else {
        println!(
            "  nearest water: {nearest} tiles from spawn, offset {:?} ({:.0} WU, horizon fade starts ~757 WU)",
            dir, nearest as f64 * 1.732
        );
    }
}
