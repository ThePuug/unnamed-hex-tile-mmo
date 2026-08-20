//! Slope form probe — the angle distribution and profile curvature of the
//! composed surface.

//! Real hillslopes are convex at the crest, concave at the base, and bounded in
//! angle, so their slope histogram piles up against the failure threshold
//! instead of spreading past it. This measures all three.

//! Run: cargo test -p world --release --test slope_probe -- --ignored --nocapture

use std::sync::Arc;
use std::time::Instant;

use world::events::Composite;
use world::events::plates::PlateEvent;
use world::events::sea::SeaEvent;
use world::events::slope_form::SlopeFormEvent;

use world::events::spines::{SpineEvent, SpineInstanceIndex};
use world::PlateCache;
use common::HexLattice;

const SEED: u64 = 0x9E3779B97F4A7C15;

/// Render geometry as the server builds the Map: hex radius 1.0, so neighbours
/// sit sqrt(3) world units apart, and RISE 0.8 world units per z-level. One
/// z-level per tile is therefore 24.8 degrees on screen.
const RENDER_SLOPE_PER_Z_PER_TILE: f64 = 0.8 / 1.7320508075688772;

/// A glaciated summit — the region every measurement here is taken over.
/// Sampling flat inland ground would report nothing about slope form, and the
/// span has to clear the bowls, which sit a bowl radius out from the summit.
const SAMPLE: (i32, i32) = (6667, -43037);
const SAMPLE_SPAN: i32 = 700;

fn composite_below_slope_form() -> Composite {
    let plate_cache = Arc::new(PlateCache::new(SEED));
    let mut c = Composite::new(SEED);
    c.add_event(Box::new(PlateEvent::with_cache(plate_cache.clone())));
    c.add_event(Box::new(SeaEvent::new()));
    c.add_event(Box::new(SpineEvent::with_cache(plate_cache, SEED)));
    c
}

fn composite() -> Composite {
    let plate_cache = Arc::new(PlateCache::new(SEED));
    let mut c = Composite::new(SEED);
    c.add_event(Box::new(PlateEvent::with_cache(plate_cache.clone())));
    c.add_event(Box::new(SeaEvent::new()));
    c.add_event(Box::new(SpineEvent::with_cache(plate_cache, SEED)));
    c.add_event(Box::new(SlopeFormEvent::new()));
    c
}

const NEIGHBOURS: [(i32, i32); 6] = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, -1), (-1, 1)];

/// Unit direction of each neighbour offset in world space.
fn neighbour_dirs() -> [(f64, f64); 6] {
    let (ox, oy) = world::hex_to_world(0, 0);
    let mut d = [(0.0, 0.0); 6];
    for (i, &(q, r)) in NEIGHBOURS.iter().enumerate() {
        let (wx, wy) = world::hex_to_world(q, r);
        d[i] = (wx - ox, wy - oy);
    }
    d
}

/// Planar gradient magnitude in z per world unit, least-squares fitted over the
/// six neighbours. Six evenly spaced unit directions give sum of u uT = 3I.
fn gradient(z: &dyn Fn(i32, i32) -> f64, q: i32, r: i32, dirs: &[(f64, f64); 6]) -> f64 {
    let z0 = z(q, r);
    let (mut gx, mut gy) = (0.0, 0.0);
    for (i, &(dq, dr)) in NEIGHBOURS.iter().enumerate() {
        let dz = z(q + dq, r + dr) - z0;
        gx += dz * dirs[i].0;
        gy += dz * dirs[i].1;
    }
    (gx / 3.0).hypot(gy / 3.0)
}

fn degrees(slope_z_per_wu: f64) -> f64 {
    (slope_z_per_wu * RENDER_SLOPE_PER_Z_PER_TILE).atan().to_degrees()
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() { return 0.0; }
    sorted[((sorted.len() - 1) as f64 * p) as usize]
}

/// Slope-angle histogram over relief. A threshold-limited landscape piles up
/// against its critical angle; an unlimited one spreads past it.
#[test]
#[ignore]
fn slope_histogram() {
    println!("\n=== slope angle histogram ===\n");
    let c = composite();
    let z = |q: i32, r: i32| c.tile_at(q, r).elevation;
    let dirs = neighbour_dirs();

    let mut angles: Vec<f64> = Vec::new();
    let mut total = 0u32;
    let t = Instant::now();
    for i in 0..SAMPLE_SPAN {
        for j in 0..SAMPLE_SPAN {
            let (q, r) = (SAMPLE.0 - SAMPLE_SPAN / 2 + i, SAMPLE.1 - SAMPLE_SPAN / 2 + j);
            if z(q, r) <= 0.0 { continue; }
            total += 1;
            angles.push(degrees(gradient(&z, q, r, &dirs)));
        }
    }
    angles.sort_by(|a, b| a.partial_cmp(b).unwrap());

    println!("land samples: {total} in {:?}", t.elapsed());
    if angles.is_empty() { return; }
    println!(
        "  p05 {:.1}deg  p25 {:.1}  med {:.1}  p75 {:.1}  p95 {:.1}  p99 {:.1}  max {:.1}",
        percentile(&angles, 0.05), percentile(&angles, 0.25),
        percentile(&angles, 0.50), percentile(&angles, 0.75),
        percentile(&angles, 0.95), percentile(&angles, 0.99),
        angles[angles.len() - 1],
    );

    let bins = [0.0, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 35.0, 40.0, 45.0, 55.0, 65.0, 75.0, 90.1];
    println!("\n  angle          share");
    for w in bins.windows(2) {
        let n = angles.iter().filter(|&&a| a >= w[0] && a < w[1]).count();
        let pct = 100.0 * n as f64 / angles.len() as f64;
        println!("  {:>4.0}-{:<4.0}  {:>6.2}%  {}", w[0], w[1], pct, "#".repeat((pct * 1.2) as usize));
    }
}

/// Curvature sign along a downhill transect from the highest tile in the
/// sample block. Creep makes crests convex and slope bases concave.
#[test]
#[ignore]
fn crest_curvature() {
    println!("\n=== crest / base curvature ===\n");
    let c = composite();
    let z = |q: i32, r: i32| c.tile_at(q, r).elevation;

    let mut best = (0i32, 0i32, f64::MIN);
    for i in 0..SAMPLE_SPAN {
        for j in 0..SAMPLE_SPAN {
            let (q, r) = (SAMPLE.0 - SAMPLE_SPAN / 2 + i, SAMPLE.1 - SAMPLE_SPAN / 2 + j);
            let e = z(q, r);
            if e > best.2 { best = (q, r, e); }
        }
    }
    println!("summit at ({}, {}) elev {:.1}", best.0, best.1, best.2);

    let (mut q, mut r) = (best.0, best.1);
    let mut profile: Vec<f64> = vec![best.2];
    for _ in 0..200 {
        let z0 = z(q, r);
        let mut step = None;
        let mut lowest = z0;
        for &(dq, dr) in &NEIGHBOURS {
            let e = z(q + dq, r + dr);
            if e < lowest { lowest = e; step = Some((dq, dr)); }
        }
        let Some((dq, dr)) = step else { break };
        q += dq;
        r += dr;
        profile.push(z(q, r));
    }

    let n = profile.len();
    println!("transect length {n} tiles");
    if n < 12 {
        println!("  transect too short");
        return;
    }
    // Second difference along the transect: negative is convex, positive concave.
    let curv = |i: usize| profile[i - 1] - 2.0 * profile[i] + profile[i + 1];
    let seg = n / 3;
    let mean = |lo: usize, hi: usize| {
        let (lo, hi) = (lo.max(1), hi.min(n - 1));
        (lo..hi).map(curv).sum::<f64>() / (hi - lo).max(1) as f64
    };
    println!("  crest third  mean curvature {:+.4}  (convex wants < 0)", mean(0, seg));
    println!("  middle third mean curvature {:+.4}", mean(seg, 2 * seg));
    println!("  base third   mean curvature {:+.4}  (concave wants > 0)", mean(2 * seg, n));
}

/// What the stage does to real ground, read as the difference between the
/// composite with it and the same composite without — which is what ships,
/// rather than what a sub-primitive would do in isolation.
#[test]
#[ignore]
fn slope_form_census() {
    println!("\n=== slope form census ===\n");
    let below = composite_below_slope_form();
    let full = composite();

    const N: i32 = SAMPLE_SPAN;
    let mut creep: Vec<f64> = Vec::new();
    let mut cut: Vec<f64> = Vec::new();
    let mut fill: Vec<f64> = Vec::new();
    let mut land = 0u32;

    for i in 0..N {
        for j in 0..N {
            let (q, r) = (SAMPLE.0 - N / 2 + i, SAMPLE.1 - N / 2 + j);
            let bv = below.tile_at(q, r);
            if bv.elevation <= 0.0 { continue; }
            land += 1;
            // Creep is the one sub-primitive an outside reader can reproduce:
            // it is a closed-form function of the curvature the layers below
            // state. Subtracting it leaves failure and deposition, which
            // otherwise vanish under a delta that moves every tile.
            let c = world::slope_form::creep_delta(bv.curvature, bv.wx, bv.wy);
            let d = full.tile_at(q, r).elevation - bv.elevation - c;
            if c.abs() > 1e-9 { creep.push(c); }
            if d < -1e-9 { cut.push(d); }
            if d > 1e-9 { fill.push(d); }
        }
    }
    if land == 0 { println!("  no relief in the sample block"); return; }

    for v in [&mut creep, &mut cut, &mut fill] {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    }
    println!("land samples with relief: {land}");
    for (name, v) in [("creep", &creep), ("failure", &cut), ("talus", &fill)] {
        if v.is_empty() {
            println!("  {name:<8} never fires");
            continue;
        }
        println!(
            "  {name:<8} {:.1}% of tiles   p10 {:+.2} / med {:+.2} / p90 {:+.2} / max |{:.2}| z",
            100.0 * v.len() as f64 / land as f64,
            percentile(v, 0.1), percentile(v, 0.5), percentile(v, 0.9),
            v.iter().fold(0.0f64, |a, &b| a.max(b.abs())),
        );
    }
}

/// The gate the cirque diagnostics can no longer reach. They walk a spine
/// instance directly, which now answers the water layer's surface — slope form
/// sits above it in the composite. A basin that survives the layers below and
/// is then opened by a smoothed rim would show up here and nowhere else.
#[test]
#[ignore]
fn basins_survive_slope_form() {
    println!("\n=== basin closure ===\n");
    // The same reading either side of the stage. Sampling the rim at tile
    // resolution loses some of what a continuous walk sees, so the run without
    // the stage is the control: only the difference between the two is
    // attributable to slope form.
    basin_closure_of("below slope form", &composite_below_slope_form());
    basin_closure_of("finished surface", &composite());
}

fn basin_closure_of(label: &str, c: &Composite) {
    println!("-- {label} --");
    const N: i32 = 7;
    const STEP: i32 = 4_000;
    let origin = -(N / 2) * STEP;
    for i in 0..N {
        for j in 0..N {
            c.tile_at(origin + i * STEP, origin + j * STEP);
        }
    }

    let mut checked = 0u32;
    let mut breached = 0u32;
    let mut cascaded = 0u32;
    let mut held: Vec<f64> = Vec::new();
    let mut worst = f64::MAX;

    /// A bowl reduced to what the closure test needs, so the index guard is
    /// released before the composite is queried again.
    struct Bowl {
        cx: f64,
        cy: f64,
        floor: f64,
        radius: f64,
        /// Rim positions, sampled far finer than the rim was fitted at.
        rim: Vec<(f64, f64)>,
    }

    let bowls: Vec<Bowl> = c.with_indexes(|ix| {
        let Some(idx) = ix.get::<SpineInstanceIndex>() else { return Vec::new() };
        idx.cells.values()
            .flat_map(|v| v.iter())
            .flat_map(|inst| inst.cirques.iter())
            .map(|q| Bowl {
                cx: q.cx,
                cy: q.cy,
                floor: q.floor,
                radius: q.radius,
                rim: (0..1440)
                    .map(|s| {
                        let theta = std::f64::consts::TAU * s as f64 / 1440.0;
                        // Just inside the footprint: the impounding level holds
                        // over the open disc, so its boundary is the one place
                        // the clamp does not apply.
                        let rr = q.radius_at(theta) * 0.995;
                        (q.cx + theta.cos() * rr, q.cy + theta.sin() * rr)
                    })
                    .collect(),
            })
            .collect()
    });

    // Read the rim through the composite, so the reading includes the stage.
    // Tiles, not continuous positions: above the spine layer elevation is a
    // per-tile quantity and the composite only answers at tile centres.
    for b in &bowls {
        let (cx, cy, floor, radius) = (&b.cx, &b.cy, &b.floor, &b.radius);
        let mut lowest = f64::MAX;
        for &(wx, wy) in &b.rim {
            let (q, r) = world::world_to_hex(wx, wy);
            let e = c.tile_at(q, r).elevation;
            if e < lowest { lowest = e; }
        }
        checked += 1;
        let depth = lowest - floor;
        worst = worst.min(depth);
        if depth <= 0.0 {
            // A deeper bowl overlapping this rim is a cascade, not a defect:
            // the pair merges and drains into the lower floor.
            let by_neighbour = bowls.iter().any(|o| {
                o.floor < *floor && (o.cx - cx).hypot(o.cy - cy) < o.radius + radius
            });
            if by_neighbour { cascaded += 1; } else { breached += 1; }
        } else {
            held.push(depth);
        }
    }

    held.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let pct = |n: u32| 100.0 * n as f64 / checked.max(1) as f64;
    println!("bowls checked      {checked}");
    println!("  holding          {} ({:.1}%)", held.len(), 100.0 * held.len() as f64 / checked.max(1) as f64);
    println!("  draining into a deeper neighbour {cascaded} ({:.1}%)", pct(cascaded));
    println!("  breached otherwise {breached} ({:.1}%)", pct(breached));
    println!(
        "water depth held   p10 {:.1} / med {:.1} / p90 {:.1}  (deepest breach {:.1})",
        percentile(&held, 0.1), percentile(&held, 0.5), percentile(&held, 0.9), worst
    );
}

/// What LoD summary building costs, either side of the stage.
///
/// The server computes summaries a mesh region at a time — 271 summaries, each
/// reading 7 tiles spaced `(2r+1)/3` apart. Those samples are sparse but not
/// isolated, so the cost sits between a contiguous chunk and a lone probe, and
/// only the real pattern says where.
#[test]
#[ignore]
fn summary_region_cost() {
    println!("\n=== summary region cost (271 summaries, 7 samples each) ===\n");
    // Mesh regions group summaries with HexLattice::new(9) over summary coords.
    let region = HexLattice::new(9);

    for band in [1u32, 4, 13, 40] {
        let summaries = HexLattice::new(band);
        let d = (2 * band as i32 + 1) / 3;
        let offsets = [(0, 0), (d, 0), (-d, 0), (0, d), (0, -d), (d, -d), (-d, d)];

        let mut samples: Vec<(i32, i32)> = Vec::new();
        for (sq, sr) in region.tiles_in_cell((0, 0)) {
            let (cq, cr) = summaries.cell_center((sq, sr));
            for (dq, dr) in offsets {
                samples.push((cq + dq, cr + dr));
            }
        }

        let mut line = format!("  band r={band:<3} d={d:<3} {} samples", samples.len());
        for (label, c) in [
            ("below", composite_below_slope_form()),
            ("full stack", composite()),
        ] {
            // Warm the deform cascade so the timing is per-sample work.
            c.elevation_at(samples[0].0, samples[0].1);
            let t = Instant::now();
            for &(q, r) in &samples {
                std::hint::black_box(c.elevation_at(q, r));
            }
            let cold = t.elapsed();
            // Second pass over the same samples: whether the cost is a one-time
            // charge per region or one the server pays again on every rebuild.
            let t = Instant::now();
            for &(q, r) in &samples {
                std::hint::black_box(c.elevation_at(q, r));
            }
            let warm = t.elapsed();
            let n = samples.len() as u32;
            line += &format!("   {label} {:?}/{:?}", cold / n, warm / n);
        }
        println!("{line}");
    }
}

/// Cost of resolving elevation, against which the slope form pass is measured.
/// The per-instance number is the one a kernel of area A multiplies.
#[test]
#[ignore]
fn elevation_cost() {
    println!("\n=== elevation cost ===\n");
    let c = composite();
    c.tile_at(SAMPLE.0, SAMPLE.1);

    const N: i32 = 100;
    let t = Instant::now();
    for i in 0..N {
        for j in 0..N {
            c.tile_at(SAMPLE.0 + i, SAMPLE.1 + j);
        }
    }
    let cold = t.elapsed();
    let t = Instant::now();
    for i in 0..N {
        for j in 0..N {
            c.tile_at(SAMPLE.0 + i, SAMPLE.1 + j);
        }
    }
    let warm = t.elapsed();
    let n = (N * N) as u32;
    println!(
        "  {n} tiles: first pass {cold:?} ({:?}/tile), cached {warm:?} ({:?}/tile)",
        cold / n,
        warm / n
    );

    let mut evals = 0u32;
    let t = Instant::now();
    c.with_indexes(|ix| {
        let Some(idx) = ix.get::<SpineInstanceIndex>() else { return };
        for inst in idx.cells.values().flat_map(|v| v.iter()) {
            for i in 0..N {
                for j in 0..N {
                    let (wx, wy) = world::hex_to_world(SAMPLE.0 + i, SAMPLE.1 + j);
                    std::hint::black_box(inst.elevation_at(wx, wy));
                    evals += 1;
                }
            }
        }
    });
    let dt = t.elapsed();
    if evals > 0 {
        println!("  {evals} SpineInstance::elevation_at: {dt:?} ({:?}/eval)", dt / evals);
    }
}

/// Where the threshold limiter cuts a rim the composite does not, and which
/// published face told it to. Bisecting by hand costs a rebuild per guess;
/// this prints the binding face and what the ground under it actually is.
#[test]
#[ignore]
fn limiter_overcut_diagnostic() {
    use common::HexLattice;
    use world::events::faces::ErosionalFaceIndex;
    use world::events::spines::SPINE_CELL_SCALE;

    println!("\n=== limiter overcut ===\n");
    let below = composite_below_slope_form();
    let full = composite();
    const N: i32 = 7;
    const STEP: i32 = 4_000;
    let origin = -(N / 2) * STEP;
    for i in 0..N {
        for j in 0..N {
            below.tile_at(origin + i * STEP, origin + j * STEP);
            full.tile_at(origin + i * STEP, origin + j * STEP);
        }
    }

    // Worst rim tile: where the finished surface sits furthest under the one
    // the layers below left.
    let bowls: Vec<(f64, f64, f64, f64)> = full.with_indexes(|ix| {
        let Some(idx) = ix.get::<SpineInstanceIndex>() else { return Vec::new() };
        idx.cells.values().flat_map(|v| v.iter())
            .flat_map(|inst| inst.cirques.iter())
            .map(|c| (c.cx, c.cy, c.radius, c.floor))
            .collect()
    });

    let mut worst = (0.0f64, (0i32, 0i32), 0.0, 0.0);
    for (cx, cy, radius, _floor) in &bowls {
        for s in 0..360 {
            let theta = std::f64::consts::TAU * s as f64 / 360.0;
            let (wx, wy) = (cx + theta.cos() * radius * 0.9, cy + theta.sin() * radius * 0.9);
            let (q, r) = world::world_to_hex(wx, wy);
            let b = below.tile_at(q, r).elevation;
            let f = full.tile_at(q, r).elevation;
            if b - f > worst.0 { worst = (b - f, (q, r), b, f); }
        }
    }

    let (cut, (q, r), b, f) = worst;
    if cut <= 0.0 { println!("  the stage never cuts a rim"); return; }
    let (wx, wy) = world::hex_to_world(q, r);
    println!("worst cut {cut:.1} z at ({q}, {r}): below {b:.1} -> finished {f:.1}");

    let lattice = HexLattice::new(SPINE_CELL_SCALE);
    let cells = lattice.cells_within_distance(lattice.cell_id(q, r), 1);
    full.with_indexes(|ix| {
        let Some(idx) = ix.get::<ErosionalFaceIndex>() else { return };
        for id in &cells {
            let Some(faces) = idx.cells.get(id) else { continue };
            let mut lowest: Option<(f64, f64, f64)> = None;
            faces.for_each_near(wx, wy, |face, d| {
                if lowest.map_or(true, |(f, _, _)| face.floor < f) {
                    lowest = Some((face.floor, face.height, d));
                }
            });
            if let Some((floor, height, d)) = lowest {
                println!("  lowest face floor {floor:.1} h {height:.1} at d {d:.2}");
            }
        }
    });
}

/// Where the ground past the critical angle sits relative to the geometry the
/// limiter can act on.
///
/// The limiter only ever caps ground against a published face. Anything above
/// critical is therefore one of two things: ground a face could reach and did
/// not, or ground no layer publishes a face for at all. Those want opposite
/// fixes — more reach against the first, a different treatment entirely
/// against the second — and only the distance to the nearest face separates
/// them.
#[test]
#[ignore]
fn critical_ground_reach() {
    use std::collections::HashSet;
    use world::events::faces::ErosionalFaceIndex;
    use world::events::spines::SPINE_CELL_SCALE;
    use world::slope_form::{critical_slope, MASS_WASTING_REACH};

    println!("\n=== ground past critical, by distance to a face ===\n");
    let c = composite();
    let z = |q: i32, r: i32| c.tile_at(q, r).elevation;
    let dirs = neighbour_dirs();

    let mut steep: Vec<(f64, f64, f64)> = Vec::new();
    let mut land = 0u32;
    for i in 0..SAMPLE_SPAN {
        for j in 0..SAMPLE_SPAN {
            let (q, r) = (SAMPLE.0 - SAMPLE_SPAN / 2 + i, SAMPLE.1 - SAMPLE_SPAN / 2 + j);
            if z(q, r) <= 0.0 { continue; }
            land += 1;
            let (wx, wy) = world::hex_to_world(q, r);
            let g = gradient(&z, q, r, &dirs);
            if g > critical_slope(wx, wy) {
                steep.push((wx, wy, degrees(g)));
            }
        }
    }
    println!("land samples {land}, past critical {} ({:.2}%)",
        steep.len(), 100.0 * steep.len() as f64 / land.max(1) as f64);
    if steep.is_empty() { return; }

    // Every face published over the sampled block. The block is warm by now,
    // so its spine cell and ring hold whatever reaches it.
    let (sx, sy) = world::hex_to_world(SAMPLE.0, SAMPLE.1);
    let lattice = HexLattice::new(SPINE_CELL_SCALE);
    let cells = lattice.cells_within_distance(lattice.cell_id(SAMPLE.0, SAMPLE.1), 1);
    let mut feet: Vec<(f64, f64)> = Vec::new();
    c.with_indexes(|ix| {
        let Some(idx) = ix.get::<ErosionalFaceIndex>() else { return };
        let mut seen: HashSet<(u64, u64)> = HashSet::new();
        for id in &cells {
            let Some(faces) = idx.cells.get(id) else { continue };
            faces.for_each_in(sx, sy, SAMPLE_SPAN as f64, |f| {
                if seen.insert((f.wx.to_bits(), f.wy.to_bits())) {
                    feet.push((f.wx, f.wy));
                }
            });
        }
    });
    println!("faces published over the block: {}", feet.len());

    // Indexed at the widest bucket, so one 7-cell query covers every distance
    // the buckets distinguish.
    let far = 4.0 * MASS_WASTING_REACH;
    let mut grid: common::HexSpatialGrid<(f64, f64)> = common::HexSpatialGrid::new(far);
    for &(fx, fy) in &feet {
        grid.insert(fx, fy, (fx, fy));
    }

    let bounds = [MASS_WASTING_REACH, 2.0 * MASS_WASTING_REACH, far];
    let mut buckets = [0u32; 4];
    let mut worst_in_reach = 0.0f64;
    for &(wx, wy, angle) in &steep {
        let mut nearest = f64::MAX;
        for &(fx, fy) in grid.query(wx, wy) {
            let d = (wx - fx).hypot(wy - fy);
            if d < nearest { nearest = d; }
        }
        match bounds.iter().position(|&b| nearest <= b) {
            Some(i) => buckets[i] += 1,
            None => buckets[3] += 1,
        }
        if nearest <= MASS_WASTING_REACH && angle > worst_in_reach {
            worst_in_reach = angle;
        }
    }

    let labels = [
        format!("within reach (<={:.0} wu)", bounds[0]),
        format!("{:.0}-{:.0} wu", bounds[0], bounds[1]),
        format!("{:.0}-{:.0} wu", bounds[1], bounds[2]),
        format!("beyond {:.0} wu or no face", bounds[2]),
    ];
    println!("\n  distance to nearest face foot      share of steep ground");
    for (label, n) in labels.iter().zip(buckets) {
        let pct = 100.0 * n as f64 / steep.len() as f64;
        println!("  {label:<32}  {pct:>6.2}%  {}", "#".repeat((pct * 0.4) as usize));
    }
    if buckets[0] > 0 {
        println!("\nsteepest tile the limiter could reach: {worst_in_reach:.1}deg");
    }
}

/// The analytic creep against the kernel it replaces.
///
/// Both are the same physics — diffusion over one interval — reached two ways:
/// the kernel averages a gathered ball, this stage reads the curvature the
/// layers below state in closed form. The kernel is the reference because it
/// is the thing that cannot ship: it costs a neighbourhood per tile.
#[test]
#[ignore]
fn creep_reference() {
    use world::slope_form::Neighbourhood;

    println!("
=== creep: analytic against kernel ===
");
    let below = composite_below_slope_form();
    let shipped = composite();
    let raw = |q: i32, r: i32| below.tile_at(q, r).elevation;
    let kernel = |q: i32, r: i32| {
        let (wx, wy) = world::hex_to_world(q, r);
        Neighbourhood::gather(q, r, wx, wy, &raw).creep()
    };
    let full = |q: i32, r: i32| shipped.tile_at(q, r).elevation;

    // The transect runs downhill from the summit of the surface below, so all
    // three profiles are read along the same ground.
    let mut best = (0i32, 0i32, f64::MIN);
    for i in 0..SAMPLE_SPAN {
        for j in 0..SAMPLE_SPAN {
            let (q, r) = (SAMPLE.0 - SAMPLE_SPAN / 2 + i, SAMPLE.1 - SAMPLE_SPAN / 2 + j);
            let e = raw(q, r);
            if e > best.2 { best = (q, r, e); }
        }
    }
    println!("summit at ({}, {}) elev {:.1}", best.0, best.1, best.2);

    let mut path = vec![(best.0, best.1)];
    let (mut q, mut r) = (best.0, best.1);
    for _ in 0..200 {
        let mut step = None;
        let mut lowest = raw(q, r);
        for &(dq, dr) in &NEIGHBOURS {
            let e = raw(q + dq, r + dr);
            if e < lowest { lowest = e; step = Some((dq, dr)); }
        }
        let Some((dq, dr)) = step else { break };
        q += dq;
        r += dr;
        path.push((q, r));
    }
    println!("transect {} tiles", path.len());
    if path.len() < 12 { return; }

    println!("
  distance   below      + kernel    shipped");
    for (i, &(q, r)) in path.iter().enumerate() {
        if !matches!(i, 0 | 1 | 2 | 4 | 8 | 16 | 32 | 64 | 128) { continue; }
        println!("    d={i:<4} {:9.2}  {:+8.3}  {:+8.3}",
            raw(q, r), kernel(q, r) - raw(q, r), full(q, r) - raw(q, r));
    }

    println!("
  profile          crest     middle    base");
    for (label, z) in [
        ("below", &raw as &dyn Fn(i32, i32) -> f64),
        ("kernel", &kernel),
        ("shipped", &full),
    ] {
        let profile: Vec<f64> = path.iter().map(|&(q, r)| z(q, r)).collect();
        let n = profile.len();
        let curv = |i: usize| profile[i - 1] - 2.0 * profile[i] + profile[i + 1];
        let seg = n / 3;
        let mean = |lo: usize, hi: usize| {
            let (lo, hi) = (lo.max(1), hi.min(n - 1));
            (lo..hi).map(curv).sum::<f64>() / (hi - lo).max(1) as f64
        };
        println!("  {label:<14} {:+.4}   {:+.4}   {:+.4}",
            mean(0, seg), mean(seg, 2 * seg), mean(2 * seg, n));
    }
}
