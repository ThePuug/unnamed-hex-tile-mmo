//! Cirque probe — how much glaciated terrain a world actually gets.

//! Not a pass/fail test. Answers: how many peaks clear the glaciation line, how
//! many bowls they host, how big and how deep those bowls are, and how much of
//! the high country a player would find bitten rather than smooth.

//! Run: cargo test -p world --release --test cirque_probe -- --ignored --nocapture

use std::sync::Arc;

use world::events::Composite;
use world::events::plates::PlateEvent;
use world::events::sea::SeaEvent;
use world::events::slope_form::SlopeFormEvent;
use world::events::spines::{SpineEvent, SpineInstanceIndex};
use world::{CirqueProbe, GLACIATION_LINE, PlateCache};

const SEED: u64 = 0x9E3779B97F4A7C15;

fn composite() -> Composite {
    let plate_cache = Arc::new(PlateCache::new(SEED));
    let mut c = Composite::new(SEED);
    c.add_event(Box::new(PlateEvent::with_cache(plate_cache.clone())));
    c.add_event(Box::new(SeaEvent::new()));
    c.add_event(Box::new(SpineEvent::with_cache(plate_cache, SEED)));
    c.add_event(Box::new(SlopeFormEvent::new()));
    c
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() { return 0.0; }
    let i = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[i]
}

#[test]
#[ignore]
fn cirque_census() {
    println!("\n=== cirque census ===\n");
    println!("glaciation line: {GLACIATION_LINE:.0}\n");

    let c = composite();

    // Touch a spread of tiles so the spine layer deforms its cells and the
    // instance index fills in.
    const N: i32 = 9;
    const STEP: i32 = 4_000;
    let origin = -(N / 2) * STEP;
    for i in 0..N {
        for j in 0..N {
            c.tile_at(origin + i * STEP, origin + j * STEP);
        }
    }

    let mut instances = 0usize;
    let mut peaks = 0usize;
    let mut glaciated_peaks = 0usize;
    let mut cirques = 0usize;
    let mut radii: Vec<f64> = Vec::new();
    let mut depths: Vec<f64> = Vec::new();
    let mut floors: Vec<f64> = Vec::new();
    let mut hosts: Vec<usize> = Vec::new();

    c.with_indexes(|indexes| {
        let Some(idx) = indexes.get::<SpineInstanceIndex>() else { return };
        for inst in idx.cells.values().flat_map(|v| v.iter()) {
            instances += 1;
            peaks += inst.peaks.len();
            glaciated_peaks += inst.peaks.iter().filter(|p| p.height >= GLACIATION_LINE).count();
            cirques += inst.cirques.len();
            hosts.push(inst.cirques.len());
            for q in &inst.cirques {
                radii.push(q.radius);
                depths.push(q.outlet_elev - q.floor);
                floors.push(q.floor);
            }
        }
    });

    radii.sort_by(|a, b| a.partial_cmp(b).unwrap());
    depths.sort_by(|a, b| a.partial_cmp(b).unwrap());
    floors.sort_by(|a, b| a.partial_cmp(b).unwrap());

    println!("instances          {instances}");
    println!("peaks              {peaks}");
    println!(
        "  above the line   {glaciated_peaks} ({:.0}%)",
        100.0 * glaciated_peaks as f64 / peaks.max(1) as f64
    );
    println!("cirques            {cirques}");
    println!(
        "  per glaciated pk {:.2}",
        cirques as f64 / glaciated_peaks.max(1) as f64
    );
    println!(
        "  per instance     min {} / med {} / max {}",
        hosts.iter().min().copied().unwrap_or(0),
        {
            let mut h = hosts.clone();
            h.sort_unstable();
            h.get(h.len() / 2).copied().unwrap_or(0)
        },
        hosts.iter().max().copied().unwrap_or(0),
    );

    if cirques == 0 {
        println!("\nNO CIRQUES — the altitude gate is rejecting every candidate.");
        return;
    }

    println!(
        "\nradius (tiles)     p10 {:.0} / med {:.0} / p90 {:.0}",
        percentile(&radii, 0.1), percentile(&radii, 0.5), percentile(&radii, 0.9)
    );
    println!(
        "basin depth (z)    p10 {:.0} / med {:.0} / p90 {:.0}",
        percentile(&depths, 0.1), percentile(&depths, 0.5), percentile(&depths, 0.9)
    );
    println!(
        "floor altitude (z) p10 {:.0} / med {:.0} / p90 {:.0}",
        percentile(&floors, 0.1), percentile(&floors, 0.5), percentile(&floors, 0.9)
    );
}

/// Whether basins actually hold on generated terrain. A bowl is fitted to a
/// sampled rim, so the question is whether the rim between those samples — with
/// ridge noise on it — ever dips to the floor and drains the basin.
#[test]
#[ignore]
fn basin_closure() {
    println!("\n=== basin closure ===\n");

    let c = composite();
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
    let mut unexplained: Vec<f64> = Vec::new();
    let mut holding: Vec<f64> = Vec::new();
    let mut worst = f64::MAX;

    c.with_indexes(|indexes| {
        let Some(idx) = indexes.get::<SpineInstanceIndex>() else { return };
        for inst in idx.cells.values().flat_map(|v| v.iter()) {
            for q in &inst.cirques {
                // Sample the rim far finer than it was fitted at, so anything
                // the fit missed between samples shows up here. Just inside it:
                // the level that impounds a basin holds over the footprint, and
                // the footprint is open, so its boundary circle is the one place
                // the clamp does not apply.
                let mut lowest = f64::MAX;
                let mut low_at = (0.0, 0.0);
                for s in 0..1440 {
                    let theta = std::f64::consts::TAU * s as f64 / 1440.0;
                    let r = q.radius_at(theta) * 0.995;
                    let p = (q.cx + theta.cos() * r, q.cy + theta.sin() * r);
                    let e = inst.elevation_at(p.0, p.1);
                    if e < lowest { lowest = e; low_at = p; }
                }
                checked += 1;
                let held = lowest - q.floor;
                worst = worst.min(held);
                if held <= 0.0 {
                    breached += 1;
                    // A deeper bowl overlapping this rim is a cascade, not a
                    // defect: the pair merges and drains into the lower floor.
                    // Footprint membership is the test, not the neighbour's own
                    // impounding level — the submerged inner slope of a bowl
                    // sits well below the rim that impounds it.
                    let by_neighbour = inst.cirques.iter().any(|o| {
                        !std::ptr::eq(o, q)
                            && o.floor < q.floor
                            && o.base_level(low_at.0, low_at.1).is_some()
                    });
                    if by_neighbour { cascaded += 1; } else { unexplained.push(held); }
                } else {
                    holding.push(held);
                }
            }
        }
    });

    holding.sort_by(|a, b| a.partial_cmp(b).unwrap());
    unexplained.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!("bowls checked      {checked}");
    println!(
        "  holding          {} ({:.1}%)",
        holding.len(), 100.0 * holding.len() as f64 / checked.max(1) as f64
    );
    println!(
        "  draining into a deeper neighbour {cascaded} ({:.1}%)",
        100.0 * cascaded as f64 / checked.max(1) as f64
    );
    println!(
        "  breached otherwise {} ({:.1}%)  worst {:.1}",
        unexplained.len(),
        100.0 * unexplained.len() as f64 / checked.max(1) as f64,
        unexplained.first().copied().unwrap_or(0.0)
    );
    println!(
        "water depth held   p10 {:.1} / med {:.1} / p90 {:.1}  (deepest breach {:.1})",
        percentile(&holding, 0.1), percentile(&holding, 0.5),
        percentile(&holding, 0.9), worst
    );
}

/// Whether the two walls of a bowl behave as designed: a headwall a player
/// cannot climb, and a lip they cross to get in. Movement blocks on a rise of
/// more than one z-level between neighbouring tiles, and neighbours sit one
/// tile spacing apart, so the passable ceiling is that ratio.
#[test]
#[ignore]
fn wall_gradients() {
    println!("\n=== wall gradients (z per world unit) ===\n");
    const PASSABLE_SLOPE: f64 = world::ELEVATION_PER_Z / world::TILE_SPACING;
    println!("passable up to {PASSABLE_SLOPE:.3}\n");

    let c = composite();
    const N: i32 = 5;
    const STEP: i32 = 4_000;
    let origin = -(N / 2) * STEP;
    for i in 0..N {
        for j in 0..N {
            c.tile_at(origin + i * STEP, origin + j * STEP);
        }
    }

    // Walk outward from each floor edge along the headwall bearing and the
    // outlet bearing, keeping the steepest rise met on the way. The ambient
    // walk continues past the footprint onto untouched flank — the control that
    // says whether a wall is an obstacle the bowl added or just the mountain.
    let mut headwall: Vec<f64> = Vec::new();
    let mut lip: Vec<f64> = Vec::new();
    let mut ambient: Vec<f64> = Vec::new();

    let steepest_along = |inst: &world::SpineInstance,
                          from: (f64, f64), bearing: f64, span: f64| -> f64 {
        let (dx, dy) = (bearing.cos(), bearing.sin());
        let mut steepest: f64 = 0.0;
        let mut prev = inst.elevation_at(from.0, from.1);
        for s in 1..=(span / 2.0) as i32 {
            let d = s as f64 * 2.0;
            let e = inst.elevation_at(from.0 + dx * d, from.1 + dy * d);
            steepest = steepest.max((e - prev) / 2.0);
            prev = e;
        }
        steepest
    };

    c.with_indexes(|indexes| {
        let Some(idx) = indexes.get::<SpineInstanceIndex>() else { return };
        for inst in idx.cells.values().flat_map(|v| v.iter()) {
            for q in &inst.cirques {
                let back = q.outlet_bearing + std::f64::consts::PI;
                headwall.push(steepest_along(inst, (q.cx, q.cy), back, q.radius));
                lip.push(steepest_along(inst, (q.cx, q.cy), q.outlet_bearing, q.radius));
                // Same length of walk, starting one footprint clear of the bowl.
                let edge = (
                    q.cx + back.cos() * q.radius * 1.5,
                    q.cy + back.sin() * q.radius * 1.5,
                );
                ambient.push(steepest_along(inst, edge, back, q.radius));
            }
        }
    });

    for v in [&mut headwall, &mut lip, &mut ambient] {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    }

    for (name, v) in [("headwall", &headwall), ("lip", &lip), ("ambient", &ambient)] {
        let blocked = v.iter().filter(|&&s| s > PASSABLE_SLOPE).count();
        println!(
            "{name:<9} p10 {:.2} / med {:.2} / p90 {:.2}   blocked {blocked}/{} ({:.0}%)",
            percentile(v, 0.1), percentile(v, 0.5), percentile(v, 0.9),
            v.len(), 100.0 * blocked as f64 / v.len().max(1) as f64
        );
    }
}

/// What a player walking the high country meets: how much of the ground above
/// the glaciation line is bowl rather than smooth cone, and how it splits
/// between the parts of a bowl.
#[test]
#[ignore]
fn glaciated_ground_coverage() {
    println!("\n=== glaciated ground coverage ===\n");

    let c = composite();
    const N: i32 = 9;
    const STEP: i32 = 4_000;
    let origin = -(N / 2) * STEP;
    for i in 0..N {
        for j in 0..N {
            c.tile_at(origin + i * STEP, origin + j * STEP);
        }
    }

    let mut high_ground = 0u32;
    let mut counts = [0u32; 4]; // floor, headwall, lip, outlet

    c.with_indexes(|indexes| {
        let Some(idx) = indexes.get::<SpineInstanceIndex>() else { return };
        for inst in idx.cells.values().flat_map(|v| v.iter()) {
            if inst.cirques.is_empty() { continue; }
            // Sample the instance's own extent on a grid.
            const S: i32 = 200;
            let (bx, by) = inst.bounding_center;
            let r = inst.bounding_radius;
            let step = 2.0 * r / S as f64;
            for i in 0..S {
                for j in 0..S {
                    let wx = bx - r + i as f64 * step;
                    let wy = by - r + j as f64 * step;
                    if inst.elevation_at(wx, wy) < GLACIATION_LINE { continue; }
                    high_ground += 1;
                    match inst.cirque_probe(wx, wy) {
                        Some(CirqueProbe::Floor) => counts[0] += 1,
                        Some(CirqueProbe::Headwall) => counts[1] += 1,
                        Some(CirqueProbe::Lip) => counts[2] += 1,
                        Some(CirqueProbe::Outlet) => counts[3] += 1,
                        None => {}
                    }
                }
            }
        }
    });

    let bitten: u32 = counts.iter().sum();
    println!("samples above the line  {high_ground}");
    if high_ground == 0 {
        println!("no ground above the glaciation line in range");
        return;
    }
    println!(
        "inside a bowl           {bitten} ({:.1}%)",
        100.0 * bitten as f64 / high_ground as f64
    );
    for (name, n) in ["floor", "headwall", "lip", "outlet"].iter().zip(counts.iter()) {
        println!(
            "  {name:<20} {n:>7} ({:.1}% of high ground)",
            100.0 * *n as f64 / high_ground as f64
        );
    }
}

/// Whether a bowl reads as draining somewhere. Two questions: is the outlet a
/// notch in the rim or just the low end of a broad ramp, and does the channel
/// leaving it start at the rim or somewhere out on the open flank.
#[test]
#[ignore]
fn drainage_path() {
    println!("\n=== drainage path ===\n");

    let c = composite();
    const N: i32 = 5;
    const STEP: i32 = 4_000;
    let origin = -(N / 2) * STEP;
    for i in 0..N {
        for j in 0..N {
            c.tile_at(origin + i * STEP, origin + j * STEP);
        }
    }

    // Rim relief: how far the rim stands above the spill point at increasing
    // angles off the outlet bearing. A notch shows as growth over a few tiles
    // of arc; a broad swale shows as nothing until well off the bearing.
    let mut notch: Vec<Vec<f64>> = vec![Vec::new(); 5];
    const OFFSETS: [f64; 5] = [15.0, 30.0, 45.0, 90.0, 180.0];

    // Channel depth along the outflow line: centre elevation against the
    // shoulders either side of it.
    let mut channel: Vec<Vec<f64>> = vec![Vec::new(); 6];
    const DOWN: [f64; 6] = [0.0, 25.0, 50.0, 75.0, 150.0, 300.0];

    c.with_indexes(|indexes| {
        let Some(idx) = indexes.get::<SpineInstanceIndex>() else { return };
        for inst in idx.cells.values().flat_map(|v| v.iter()) {
            for q in &inst.cirques {
                let sill = inst.elevation_at(q.outlet_wx, q.outlet_wy);
                for (k, off) in OFFSETS.iter().enumerate() {
                    let t = q.outlet_bearing + off.to_radians();
                    let r = q.radius_at(t) * 0.8;
                    let e = inst.elevation_at(q.cx + t.cos() * r, q.cy + t.sin() * r);
                    notch[k].push(e - sill);
                }

                let (ux, uy) = (q.outlet_bearing.cos(), q.outlet_bearing.sin());
                let (px, py) = (-uy, ux);
                for (k, d) in DOWN.iter().enumerate() {
                    let (mx, my) = (q.outlet_wx + ux * d, q.outlet_wy + uy * d);
                    let mid = inst.elevation_at(mx, my);
                    let mut shoulder = f64::MIN;
                    for s in [40.0, 60.0, 80.0] {
                        shoulder = shoulder
                            .max(inst.elevation_at(mx + px * s, my + py * s))
                            .max(inst.elevation_at(mx - px * s, my - py * s));
                    }
                    channel[k].push(shoulder - mid);
                }
            }
        }
    });

    println!("rim above the spill point, by angle off the outlet bearing (z)");
    for (k, off) in OFFSETS.iter().enumerate() {
        let mut v = notch[k].clone();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "  {off:>5.0}deg   p10 {:>7.1} / med {:>7.1} / p90 {:>7.1}",
            percentile(&v, 0.1), percentile(&v, 0.5), percentile(&v, 0.9)
        );
    }

    println!("\nchannel depth below its shoulders, by distance past the spill point (z)");
    for (k, d) in DOWN.iter().enumerate() {
        let mut v = channel[k].clone();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "  {d:>5.0}wu    p10 {:>7.1} / med {:>7.1} / p90 {:>7.1}",
            percentile(&v, 0.1), percentile(&v, 0.5), percentile(&v, 0.9)
        );
    }
}

/// Distribution of the ground below each outlet — the quantity a bowl's outflow
/// class is read from.
#[test]
#[ignore]
fn outflow_gradients() {
    println!("\n=== outflow gradients ===\n");

    let c = composite();
    const N: i32 = 5;
    const STEP: i32 = 4_000;
    let origin = -(N / 2) * STEP;
    for i in 0..N {
        for j in 0..N {
            c.tile_at(origin + i * STEP, origin + j * STEP);
        }
    }

    let mut grads: Vec<f64> = Vec::new();
    let mut drop_over_depth: Vec<f64> = Vec::new();

    c.with_indexes(|indexes| {
        let Some(idx) = indexes.get::<SpineInstanceIndex>() else { return };
        for inst in idx.cells.values().flat_map(|v| v.iter()) {
            for q in &inst.cirques {
                let rim_r = (q.outlet_wx - q.cx).hypot(q.outlet_wy - q.cy);
                let run = (q.radius_at(q.outlet_bearing) - rim_r) + q.radius * 0.5;
                let (ux, uy) = (q.outlet_bearing.cos(), q.outlet_bearing.sin());
                let far = inst.elevation_at(q.outlet_wx + ux * run, q.outlet_wy + uy * run);
                let drop = q.outlet_elev - far;
                grads.push(drop / run);
                drop_over_depth.push(drop / (q.outlet_elev - q.floor).max(1e-9));
            }
        }
    });

    grads.sort_by(|a, b| a.partial_cmp(b).unwrap());
    drop_over_depth.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!("bowls {}", grads.len());
    for p in [0.05, 0.1, 0.25, 0.5, 0.75, 0.9, 0.95] {
        println!(
            "  p{:>3.0}  slope {:>7.3} z/wu   drop/depth {:>7.2}",
            p * 100.0, percentile(&grads, p), percentile(&drop_over_depth, p)
        );
    }
    let below_1 = grads.iter().filter(|g| **g < 1.0).count();
    let dod_below_1 = drop_over_depth.iter().filter(|d| **d < 1.0).count();
    println!(
        "\n  slope < 1.0 z/wu (walkable down): {:.1}%",
        100.0 * below_1 as f64 / grads.len().max(1) as f64
    );
    println!(
        "  drop < basin depth:               {:.1}%",
        100.0 * dod_below_1 as f64 / drop_over_depth.len().max(1) as f64
    );
}

/// How much of a mountain a bowl actually takes. Answers whether the glacial
/// layer reshapes a summit or only dimples a flank, and whether neighbouring
/// bowls are anywhere near each other.
#[test]
#[ignore]
fn mountain_shaping() {
    println!("\n=== mountain shaping ===\n");

    let c = composite();
    const N: i32 = 5;
    const STEP: i32 = 4_000;
    let origin = -(N / 2) * STEP;
    for i in 0..N {
        for j in 0..N {
            c.tile_at(origin + i * STEP, origin + j * STEP);
        }
    }

    let mut falloffs: Vec<f64> = Vec::new();
    let mut heights: Vec<f64> = Vec::new();
    let mut spacing_frac: Vec<f64> = Vec::new();
    let mut above_line = 0usize;
    let mut total_peaks = 0usize;

    let mut outer: Vec<f64> = Vec::new();
    let mut inner: Vec<f64> = Vec::new();
    let mut rad_frac: Vec<f64> = Vec::new();
    let mut area_frac: Vec<f64> = Vec::new();
    let mut rejected_by_floor = 0usize;

    c.with_indexes(|indexes| {
        let Some(idx) = indexes.get::<SpineInstanceIndex>() else { return };
        for inst in idx.cells.values().flat_map(|v| v.iter()) {
            for (i, p) in inst.peaks.iter().enumerate() {
                total_peaks += 1;
                falloffs.push(p.falloff_radius);
                heights.push(p.height);
                if p.height >= GLACIATION_LINE { above_line += 1; }
                let mut nearest = f64::MAX;
                for (j, o) in inst.peaks.iter().enumerate() {
                    if i == j { continue; }
                    nearest = nearest.min((o.wx - p.wx).hypot(o.wy - p.wy));
                }
                if nearest < f64::MAX { spacing_frac.push(nearest / p.falloff_radius); }
            }

            for q in &inst.cirques {
                // Host is the nearest peak: siting places the bowl on its flank.
                let mut host: Option<&world::spine::Peak> = None;
                let mut best = f64::MAX;
                for p in &inst.peaks {
                    let d = (q.cx - p.wx).hypot(q.cy - p.wy);
                    if d < best { best = d; host = Some(p); }
                }
                let Some(h) = host else { continue };
                outer.push((best + q.radius) / h.falloff_radius);
                inner.push((best - q.radius).max(0.0) / h.falloff_radius);
                rad_frac.push(q.radius / h.falloff_radius);
                area_frac.push((q.radius / h.falloff_radius).powi(2));
            }

            // Bowls the floor gate threw away, counted by re-running the gate's
            // inputs is not possible from outside; report the shortfall against
            // the count the peaks could host instead.
            let could = inst.peaks.iter().filter(|p| p.height >= GLACIATION_LINE).count();
            if inst.cirques.len() < could { rejected_by_floor += could - inst.cirques.len(); }
        }
    });

    for v in [&mut falloffs, &mut heights, &mut spacing_frac, &mut outer, &mut inner,
              &mut rad_frac, &mut area_frac] {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    }

    println!("peaks              {total_peaks}");
    println!(
        "  above the line   {above_line} ({:.1}%)",
        100.0 * above_line as f64 / total_peaks.max(1) as f64
    );
    println!(
        "peak height        p10 {:.0} / med {:.0} / p90 {:.0}",
        percentile(&heights, 0.1), percentile(&heights, 0.5), percentile(&heights, 0.9)
    );
    println!(
        "falloff radius     p10 {:.0} / med {:.0} / p90 {:.0}",
        percentile(&falloffs, 0.1), percentile(&falloffs, 0.5), percentile(&falloffs, 0.9)
    );
    println!(
        "nearest peak, as a fraction of falloff radius\n                   p10 {:.2} / med {:.2} / p90 {:.2}",
        percentile(&spacing_frac, 0.1), percentile(&spacing_frac, 0.5), percentile(&spacing_frac, 0.9)
    );
    println!(
        "  cones overlap    {:.1}% of peaks sit inside a neighbour's falloff",
        100.0 * spacing_frac.iter().filter(|f| **f < 1.0).count() as f64
            / spacing_frac.len().max(1) as f64
    );

    println!("\nbowl against its host cone (1.0 = the whole cone)");
    println!(
        "  radius           p10 {:.2} / med {:.2} / p90 {:.2}",
        percentile(&rad_frac, 0.1), percentile(&rad_frac, 0.5), percentile(&rad_frac, 0.9)
    );
    println!(
        "  plan area        p10 {:.3} / med {:.3} / p90 {:.3}",
        percentile(&area_frac, 0.1), percentile(&area_frac, 0.5), percentile(&area_frac, 0.9)
    );
    println!(
        "  outer edge       p10 {:.2} / med {:.2} / p90 {:.2}",
        percentile(&outer, 0.1), percentile(&outer, 0.5), percentile(&outer, 0.9)
    );
    println!(
        "  inner edge       p10 {:.2} / med {:.2} / p90 {:.2}   (0.00 = eats the summit)",
        percentile(&inner, 0.1), percentile(&inner, 0.5), percentile(&inner, 0.9)
    );
    println!(
        "  bowls reaching the summit: {:.1}%   reaching the cone edge: {:.1}%",
        100.0 * inner.iter().filter(|v| **v <= 0.01).count() as f64 / inner.len().max(1) as f64,
        100.0 * outer.iter().filter(|v| **v >= 0.99).count() as f64 / outer.len().max(1) as f64
    );
}
