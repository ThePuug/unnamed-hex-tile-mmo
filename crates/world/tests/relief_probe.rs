//! Relief probe — quantifies how much terrain variation a player actually sees.

//! Not a pass/fail test. Answers: what fraction of the world is flat, how far
//! apart are features, and what is the elevation range inside one screen?

//! Run: cargo test -p world --release --test relief_probe -- --ignored --nocapture

use std::time::Instant;

use common::PlateTag;
use world::events::Composite;
use world::events::plates::PlateEvent;
use world::events::tilt::TiltEvent;
use world::events::motion::MotionEvent;
use world::events::thickening::ThickeningEvent;
use world::events::thrusting::{Outlines, ThrustingEvent};
use world::events::drainage::{surface_at, DrainageEvent};
use world::events::dissection::{DissectionEvent, Valleys};
use world::events::plates::Coasts;
use world::{hex_to_world, substrate_elevation_at};

const SEED: u64 = 0x9E3779B97F4A7C15;
const SPAWN: (i32, i32) = (-58204, 4907);

/// Tiles per second at MOVEMENT_SPEED 0.0075 WU/ms with hex radius 1.0
/// (neighbour spacing sqrt(3) WU): 7.5 / 1.732.
const TILES_PER_SEC: f64 = 4.33;

fn composite() -> Composite {
    let mut c = Composite::new(SEED);
    c.add_event(Box::new(PlateEvent::new()));
    c.add_event(Box::new(TiltEvent::new()));
    c.add_event(Box::new(MotionEvent::new()));
    c.add_event(Box::new(ThrustingEvent::new()));
    c.add_event(Box::new(ThickeningEvent::new()));
    c.add_event(Box::new(DrainageEvent::new()));
    c.add_event(Box::new(DissectionEvent::new()));
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
    let mut tag_counts = [0u32; 3];
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
            if substrate_elevation_at(wx, wy, SEED) >= 0.0 {
                land += 1;
            }
            for (k, tag) in [
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
    println!("  land (substrate >= sea level): {:.1}%", pct(land));
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
    for (l, n) in ["Ridge", "Highland", "Foothills"]
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

    // View radius: FIXED_STREAM_RADIUS 21 chunks × 16.46 tiles of Euclidean
    // chunk spacing = 346 tiles of streamed detail. Sample rings out to it.
    const VIEW: i32 = 346;
    const RINGS: [i32; 7] = [50, 100, 150, 200, 250, 300, 346];
    const SPOKES: usize = 24;

    // Standpoints on a spiral around the spawn, on land only: what a player
    // walking out from the haven stands on.
    let standpoints: Vec<(i32, i32)> = (0..400)
        .map(|i| {
            let a = i as f64 * 2.399963; // golden-angle spiral
            let rad = 1500.0 * (i as f64).sqrt();
            (SPAWN.0 + (rad * a.cos()) as i32, SPAWN.1 + (rad * a.sin()) as i32)
        })
        .filter(|&(q, r)| c.elevation_at(q, r) >= 0)
        .take(40)
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

    // A flat run: consecutive land samples within one z-level of where the
    // run began. Sea ends a run and is not counted.
    let mut runs: Vec<i32> = Vec::new();
    let mut current = 0i32;
    let mut start: Option<i32> = None;
    let mut nonzero = 0u32;
    let mut samples = 0u32;

    let t = Instant::now();
    let mut q = -LEN / 2;
    while q < LEN / 2 {
        let z = c.elevation_at(q, SPAWN.1);
        samples += 1;
        if z >= 0 { nonzero += 1 }
        match start {
            Some(z0) if z >= 0 && (z - z0).abs() <= 1 => current += STEP,
            _ => {
                if current > 0 { runs.push(current) }
                start = if z >= 0 { Some(z) } else { None };
                current = if z >= 0 { STEP } else { 0 };
            }
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
        "  land samples: {nonzero}/{samples} ({:.1}%)",
        100.0 * nonzero as f64 / samples as f64
    );
    if runs.is_empty() {
        println!("  no flat runs recorded");
        return;
    }
    let n = runs.len();
    println!("  flat stretches on land (within one z of their start): {n}");
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
/// PlateEvent submerges everything below the regime land threshold, and a spawn
/// under the waterline puts the camera beneath the water plane.
#[test]
#[ignore]
fn spawn_point_is_above_water() {
    let c = composite();
    for &(q, r, label) in &[(SPAWN.0, SPAWN.1, "server spawn")] {
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
    let (sq, sr) = SPAWN;
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

#[test]
#[ignore]
fn adjacent_tile_steps() {
    println!("\n=== ADJACENT-TILE ELEVATION STEPS ===");
    let c = composite();
    // Hex neighbours are coordinate offsets.
    const NB: [(i32, i32); 6] = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, -1), (-1, 1)];
    const N: i32 = 340;
    const STEP: i32 = 37;
    let origin = -(N / 2) * STEP;
    let mut steps = Vec::new();
    let mut belt_steps = Vec::new();
    let (mut blocked, mut total) = (0usize, 0usize);

    for i in 0..N {
        for j in 0..N {
            let (q, r) = (origin + i * STEP, origin + j * STEP);
            let z = c.elevation_at(q, r);
            let e = c.tile_at(q, r).elevation;
            let (wx, wy) = world::hex_to_world(q, r);
            let base = world::substrate_elevation_at(wx, wy, SEED);
            let in_belt = e - base > 1.0;
            for (dq, dr) in NB {
                let zn = c.elevation_at(q + dq, r + dr);
                let d = (z - zn).abs();
                total += 1;
                if d > 1 { blocked += 1 }
                steps.push(d as f64);
                if in_belt { belt_steps.push(d as f64) }
            }
        }
    }
    let q = |v: &mut Vec<f64>, p: f64| { v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[((v.len() - 1) as f64 * p) as usize] };
    println!("  neighbour pairs: {total}");
    println!("  |dz| p50 {:.0}  p90 {:.0}  p99 {:.0}  max {:.0}",
             q(&mut steps, 0.5), q(&mut steps, 0.9), q(&mut steps, 0.99), q(&mut steps, 1.0));
    println!("  steps over 1 z (blocks movement): {blocked} ({:.2}%)",
             100.0 * blocked as f64 / total as f64);
    if !belt_steps.is_empty() {
        println!("  within belts: p50 {:.0}  p90 {:.0}  max {:.0}  ({} pairs)",
                 q(&mut belt_steps, 0.5), q(&mut belt_steps, 0.9), q(&mut belt_steps, 1.0),
                 belt_steps.len());
    }
}

/// The ceiling has to survive the stack. The field reaches `OROGEN_MAX_RISE`;
/// this checks the event delivers it too, sampled at crests
/// the field probe already found rather than on a blind grid — a belt is a
/// fifth of the land and a coarse sweep lands on its skirts.
#[test]
#[ignore]
fn belt_reaches_its_ceiling() {
    println!("
=== PEAK ELEVATION IN THE BELT WINDOW ===");
    let c = composite();
    // The belt window the seam probes scan, on a coarse grid: the tallest
    // ground, and how much of it is plateau and how much range.
    let (cx, cy, half) = (-48_000.0, 9_000.0, 24_000.0);
    let outlines = Outlines::in_box(cx, cy, half, SEED);
    let (mut best, mut at, mut plateau, mut relief) = (f64::MIN, (0.0, 0.0), 0.0, 0.0);
    let mut stacked: Vec<f64> = Vec::new();
    for i in 0..=120 {
        for j in 0..=120 {
            let (px, py) = (cx - half + i as f64 * half / 60.0, cy - half + j as f64 * half / 60.0);
            let (q, r) = world::world_to_hex(px, py);
            let e = c.tile_at(q, r).elevation;
            let (f, o) = (world::events::thickening::thickening_on(px, py, &outlines), outlines.relief(px, py));
            if f + o > 0.0 { stacked.push(e) }
            if e > best { best = e; at = (px, py); plateau = f; relief = o }
        }
    }
    stacked.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let pct = |p: f64| stacked.get(((stacked.len() - 1) as f64 * p) as usize).copied().unwrap_or(0.0);
    println!("  {} of {} samples carry plateau or range; their elevation p50 {:.0}  p90 {:.0}  max {:.0}",
        stacked.len(), 121 * 121, pct(0.5), pct(0.9), pct(1.0));
    println!("  highest ground {best:.1} z at ({:.0}, {:.0}): plateau {plateau:.1} + range {relief:.1}", at.0, at.1);
    println!("  OROGEN_MAX_RISE = {:.1}", world::events::thickening::OROGEN_MAX_RISE);
}

/// Slope census over the spawn belt: how much ground stands past the angles
/// slope form acts at, on the composed surface the player walks. Repose at
/// 34° is where talus forms; a +1 z step per tile is 38.7° and is the most a
/// player can climb; the critical angle for a bare face is 75°.
#[test]
#[ignore]
fn slope_census() {
    println!("
=== SLOPE CENSUS AROUND SPAWN ===");
    let c = composite();
    let (cq, cr) = SPAWN;
    let radius = 3000;
    let (cx, cy) = world::hex_to_world(cq, cr);
    let outlines = Outlines::in_box(cx, cy, radius as f64 * 1.2, SEED);
    let stride = 4;
    let angle = |dz: f64| (dz * 0.8).atan().to_degrees();
    let mut land = 0usize;
    let mut belt = 0usize;
    let mut land_over = [0usize; 4];
    let mut belt_over = [0usize; 4];
    let mut steps_over_one = 0usize;
    let thresholds = [24.8, 34.0, 45.0, 60.0];
    let t = Instant::now();
    let mut dq = -radius;
    while dq <= radius {
        let mut dr = -radius;
        while dr <= radius {
            let (q, r) = (cq + dq, cr + dr);
            let e = c.tile_at(q, r).elevation;
            if e > 0.0 {
                let steep = [(1, 0), (0, 1), (1, -1)]
                    .iter()
                    .map(|&(a, b)| (c.tile_at(q + a, r + b).elevation - e).abs())
                    .fold(0.0f64, f64::max);
                let deg = angle(steep);
                land += 1;
                let (px, py) = world::hex_to_world(q, r);
                let in_belt = outlines.relief(px, py) > 0.0;
                if in_belt { belt += 1; }
                for (i, th) in thresholds.iter().enumerate() {
                    if deg >= *th {
                        land_over[i] += 1;
                        if in_belt { belt_over[i] += 1; }
                    }
                }
                if steep.round() > 1.0 { steps_over_one += 1; }
            }
            dr += stride;
        }
        dq += stride;
    }
    println!("  {land} land samples ({belt} in a belt), stride {stride}, {:.1}s", t.elapsed().as_secs_f64());
    println!("  {:>10} {:>12} {:>12}", "slope ≥", "of land", "of belt");
    for (i, th) in thresholds.iter().enumerate() {
        println!("  {:>9.1}° {:>11.2}% {:>11.2}%", th,
            100.0 * land_over[i] as f64 / land.max(1) as f64,
            100.0 * belt_over[i] as f64 / belt.max(1) as f64);
    }
    println!("  tiles whose steepest neighbour steps more than +1 z (blocked on foot): {:.2}% of land",
        100.0 * steps_over_one as f64 / land.max(1) as f64);
}

/// Where a haven could stand: low flat land within sight of a range and a
/// short walk from open water, over the belt window on a coarse grid. Prints
/// the best few by how near and how tall the range is.
#[test]
#[ignore]
fn haven_candidates() {
    let c = composite();
    let (cx, cy, half, step) = (-48_000.0, 9_000.0, 24_000.0, 250.0);
    let n = (2.0 * half / step) as i32 + 1;
    let at = |i: i32, j: i32| (cx - half + i as f64 * step, cy - half + j as f64 * step);
    let mut z = vec![0.0f64; (n * n) as usize];
    for i in 0..n {
        for j in 0..n {
            let (px, py) = at(i, j);
            let (q, r) = world::world_to_hex(px, py);
            z[(i * n + j) as usize] = c.tile_at(q, r).elevation;
        }
    }
    let reach = (1_500.0 / step) as i32;
    let mut found: Vec<(f64, i32, i32, f64, f64, f64, f64)> = Vec::new();
    for i in reach..n - reach {
        for j in reach..n - reach {
            let e = z[(i * n + j) as usize];
            if !(3.0..=60.0).contains(&e) { continue }
            let (mut flat, mut peak, mut peak_d, mut water_d) = (true, 0.0f64, f64::MAX, f64::MAX);
            for di in -reach..=reach {
                for dj in -reach..=reach {
                    let v = z[((i + di) * n + j + dj) as usize];
                    let d = ((di * di + dj * dj) as f64).sqrt() * step;
                    if d <= step * 1.5 && (v - e).abs() > 10.0 { flat = false }
                    if v >= 300.0 && d < peak_d { peak_d = d; peak = v }
                    if v < 0.0 && d < water_d { water_d = d }
                }
            }
            if !flat || peak_d == f64::MAX || water_d == f64::MAX || water_d < 400.0 { continue }
            found.push((peak / peak_d, i, j, e, peak, peak_d, water_d));
        }
    }
    found.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    println!("  {} candidates; best by range height over its distance:", found.len());
    for (_, i, j, e, peak, peak_d, water_d) in found.iter().take(12) {
        let (px, py) = at(*i, *j);
        let (q, r) = world::world_to_hex(px, py);
        println!("    world ({px:>7.0}, {py:>7.0}) hex ({q}, {r}): z {e:.0}, range {peak:.0} z at {peak_d:.0} WU, water {water_d:.0} WU");
    }
}

/// Valley census over the belt window: how much land lies in a valley, how
/// deep and how steep the valleys are on plains against belts, and how many
/// views hold one. The cut is read off the valleys directly, against the
/// envelope the layers beneath dissection sum to.
#[test]
#[ignore]
fn valley_census() {
    println!("\n=== valley census (the belt window) ===\n");
    let (cx, cy, half, step) = (-48_000.0, 9_000.0, 12_000.0, 250.0);
    let t = Instant::now();
    let valleys = Valleys::in_box(cx, cy, half, SEED);
    let outlines = Outlines::in_box(cx, cy, half, SEED);
    let coasts = Coasts::in_box(cx, cy, half, SEED);
    println!("  valleys, outlines and coasts built in {:?}", t.elapsed());
    let n = (2.0 * half / step) as usize + 1;
    let at = |i: usize, j: usize| (cx - half + i as f64 * step, cy - half + j as f64 * step);
    let mut land = 0usize;
    let mut cut_land = 0usize;
    let mut depths: Vec<f64> = Vec::new();
    let mut plain_slopes: Vec<f64> = Vec::new();
    let mut belt_slopes: Vec<f64> = Vec::new();
    let mut in_valley = vec![false; n * n];
    for j in 0..n {
        for i in 0..n {
            let (x, y) = at(i, j);
            let envelope = surface_at(x, y, SEED, &coasts, &outlines);
            if envelope < 0.0 { continue }
            land += 1;
            let cut = valleys.cut_at(x, y, envelope);
            if cut <= 0.0 { continue }
            cut_land += 1;
            in_valley[j * n + i] = true;
            depths.push(cut);
            // The steepest of the two axis gradients at eight tiles, in z per tile.
            let e8x = surface_at(x + 8.0, y, SEED, &coasts, &outlines);
            let e8y = surface_at(x, y + 8.0, SEED, &coasts, &outlines);
            let cx8 = valleys.cut_at(x + 8.0, y, e8x);
            let cy8 = valleys.cut_at(x, y + 8.0, e8y);
            let slope = ((cx8 - cut).abs() / 8.0).max((cy8 - cut).abs() / 8.0);
            if outlines.relief(x, y) > 0.0 || envelope > 100.0 { belt_slopes.push(slope) } else { plain_slopes.push(slope) }
        }
    }
    let pct = |v: &mut Vec<f64>, p: f64| {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v.get(((v.len().max(1) - 1) as f64 * p) as usize).copied().unwrap_or(0.0)
    };
    println!("  {land} land samples, {cut_land} in a valley ({:.1}%)", 100.0 * cut_land as f64 / land.max(1) as f64);
    println!("  depth p50 {:.1}  p90 {:.1}  max {:.1} z", pct(&mut depths, 0.5), pct(&mut depths, 0.9), pct(&mut depths, 1.0));
    println!("  wall slope on plains p50 {:.2}  p90 {:.2}  max {:.2} z/tile ({} samples)", pct(&mut plain_slopes, 0.5), pct(&mut plain_slopes, 0.9), pct(&mut plain_slopes, 1.0), plain_slopes.len());
    println!("  wall slope in belts  p50 {:.2}  p90 {:.2}  max {:.2} z/tile ({} samples)", pct(&mut belt_slopes, 0.5), pct(&mut belt_slopes, 0.9), pct(&mut belt_slopes, 1.0), belt_slopes.len());
    // A view is 692 tiles across: three samples. Share of land views holding a valley.
    let (mut views, mut with) = (0usize, 0usize);
    for j in 0..n - 2 {
        for i in 0..n - 2 {
            let (x, y) = at(i + 1, j + 1);
            if surface_at(x, y, SEED, &coasts, &outlines) < 0.0 { continue }
            views += 1;
            if (0..3).any(|dj| (0..3).any(|di| in_valley[(j + dj) * n + i + di])) { with += 1 }
        }
    }
    println!("  land views (692 tiles) holding a valley: {with}/{views} ({:.1}%)", 100.0 * with as f64 / views.max(1) as f64);
}
