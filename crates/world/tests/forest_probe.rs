//! Forest probe — what the forest layer puts on the ground, read back
//! through the stack, and what it costs. Shape tests run always; the
//! measurements are `#[ignore]`.
//!
//! Run: cargo test -p world --release --test forest_probe -- --ignored --nocapture

use std::time::Instant;

use common::Slot;
use world::events::dissection::DissectionEvent;
use world::events::drainage::DrainageEvent;
use world::events::forest::{self, ForestEvent};
use world::events::lithology::LithologyEvent;
use world::events::migration::MigrationEvent;
use world::events::motion::MotionEvent;
use world::events::plates::PlateEvent;
use world::events::thickening::ThickeningEvent;
use world::events::thrusting::ThrustingEvent;
use world::events::tilt::TiltEvent;
use world::events::Composite;
use world::hex_to_world;

const SEED: u64 = 0x9E3779B97F4A7C15;
const SPAWN: (i32, i32) = (104289, -4677);

/// The stack to dissection, without the forest: what the layer is
/// measured against.
fn without_forest() -> Composite {
    let mut c = Composite::new(SEED);
    c.add_event(Box::new(PlateEvent::new()));
    c.add_event(Box::new(TiltEvent::new()));
    c.add_event(Box::new(MotionEvent::new()));
    c.add_event(Box::new(ThrustingEvent::new()));
    c.add_event(Box::new(ThickeningEvent::new()));
    c.add_event(Box::new(LithologyEvent::new()));
    c.add_event(Box::new(DrainageEvent::new()));
    c.add_event(Box::new(MigrationEvent::new()));
    c.add_event(Box::new(DissectionEvent::new()));
    c
}

fn with_forest() -> Composite {
    let mut c = without_forest();
    c.add_event(Box::new(ForestEvent::new()));
    c
}

/// A window of tiles about a centre, `step` apart.
fn window(centre: (i32, i32), half: i32, step: i32) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    let mut q = -half;
    while q <= half {
        let mut r = -half;
        while r <= half {
            out.push((centre.0 + q, centre.1 + r));
            r += step;
        }
        q += step;
    }
    out
}

/// Nothing stands in water, above the treeline, or where the climate
/// gives nothing: every covered tile is dry land warm enough for trees.
#[test]
fn cover_stands_only_where_it_may() {
    let c = with_forest();
    for (q, r) in window(SPAWN, 600, 40) {
        let v = c.tile_at(q, r);
        if v.cover.is_empty() {
            continue;
        }
        assert!(v.water.is_none(), "cover in water at ({q}, {r})");
        let (wx, wy) = hex_to_world(q, r);
        assert!(forest::temperature(wx, wy, v.elevation, SEED) > forest::TREELINE, "cover above the treeline at ({q}, {r})");
        assert!(v.cover.fullness() <= 3);
    }
}

/// A census of a sparse window: what share of the land carries cover, of
/// what kinds and how full, how much land is above the treeline, and
/// where the temperature and moisture sit.
#[test]
#[ignore]
fn forest_census() {
    println!("\n=== forest census about the spawn ===\n");
    let c = with_forest();
    let tiles = window(SPAWN, 12_000, 250);
    let t = Instant::now();
    let (mut land, mut covered, mut cold, mut full) = (0u32, 0u32, 0u32, 0u64);
    let mut kinds = [0u32; 4];
    let mut hist = [0u32; 4];
    let (mut t_min, mut t_max, mut t_sum) = (f64::MAX, f64::MIN, 0.0);
    for &(q, r) in &tiles {
        let v = c.tile_at(q, r);
        if v.elevation <= 0.0 || v.water.is_some() {
            continue;
        }
        land += 1;
        let (wx, wy) = hex_to_world(q, r);
        let temp = forest::temperature(wx, wy, v.elevation, SEED);
        t_min = t_min.min(temp);
        t_max = t_max.max(temp);
        t_sum += temp;
        if temp <= forest::TREELINE {
            cold += 1;
        }
        let f = v.cover.fullness();
        hist[f as usize] += 1;
        if f > 0 {
            covered += 1;
            full += f as u64;
            for (_, s) in v.cover.filled() {
                kinds[s as usize] += 1;
            }
        }
    }
    let secs = t.elapsed().as_secs_f64();
    println!("{} tiles in {secs:.2}s ({:.1} µs per tile, sparse)", tiles.len(), secs * 1e6 / tiles.len() as f64);
    println!("land {land}: {:.1}% covered, {:.1}% above the treeline", 100.0 * covered as f64 / land.max(1) as f64, 100.0 * cold as f64 / land.max(1) as f64);
    println!("temperature on land: {t_min:.1} to {t_max:.1}, mean {:.1} (treeline {})", t_sum / land.max(1) as f64, forest::TREELINE);
    println!("mean fullness where covered: {:.2}", full as f64 / covered.max(1) as f64);
    println!("fullness histogram 0..3: {hist:?}");
    println!(
        "slots: scrub {}, pine {}, deciduous {}",
        kinds[Slot::Scrub as usize], kinds[Slot::Pine as usize], kinds[Slot::Deciduous as usize]
    );
}

/// What the layer costs: a contiguous chunk and a sparse sample, each
/// read through the stack with and without the forest.
#[test]
#[ignore]
fn forest_cost() {
    println!("\n=== forest cost ===\n");
    for (name, make) in [("without", without_forest as fn() -> Composite), ("with", with_forest as fn() -> Composite)] {
        let c = make();
        let t = Instant::now();
        c.tile_at(SPAWN.0, SPAWN.1);
        let first = t.elapsed().as_secs_f64() * 1e3;
        let dense = window(SPAWN, 60, 1);
        let t = Instant::now();
        for &(q, r) in &dense {
            c.tile_at(q, r);
        }
        let dense_us = t.elapsed().as_secs_f64() * 1e6 / dense.len() as f64;
        let sparse = window((SPAWN.0 + 9_000, SPAWN.1 - 9_000), 6_000, 300);
        let t = Instant::now();
        for &(q, r) in &sparse {
            c.tile_at(q, r);
        }
        let sparse_us = t.elapsed().as_secs_f64() * 1e6 / sparse.len() as f64;
        println!("{name:>8}: first tile {first:.0} ms, dense {dense_us:.1} µs/tile ({}), sparse {sparse_us:.1} µs/tile ({})", dense.len(), sparse.len());
    }
}

/// The heights the treeline is set against.
#[test]
#[ignore]
fn heights() {
    use world::events::thickening::PLATEAU_RISE;
    use world::events::thrusting::RANGE_RISE;
    println!("range rise {RANGE_RISE:.0} z, plateau rise {PLATEAU_RISE:.0} z, freeboard {:.0} z", world::CONTINENT_MAX_RISE);
    let c = with_forest();
    let mut zs: Vec<f64> = window(SPAWN, 12_000, 250).into_iter().map(|(q, r)| c.tile_at(q, r).elevation).filter(|z| *z > 0.0).collect();
    zs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let at = |p: f64| zs[((zs.len() - 1) as f64 * p) as usize];
    println!("land elevation: median {:.0}, 75% {:.0}, 90% {:.0}, 99% {:.0}, max {:.0}", at(0.5), at(0.75), at(0.9), at(0.99), at(1.0));
}

/// Vantage points for a look in the client: the spawn's own cover, and the
/// best ridge near it, a high tile whose ground falls far to wooded tiles
/// within a short walk.
#[test]
#[ignore]
fn vantage_points() {
    let c = with_forest();
    let v = c.tile_at(SPAWN.0, SPAWN.1);
    println!("spawn {:?}: z {:.0}, fullness {}, water {:?}", SPAWN, v.elevation, v.cover.fullness(), v.water);
    let tiles = window(SPAWN, 1500, 25);
    let views: std::collections::HashMap<(i32, i32), (f64, u8)> = tiles.iter().map(|&(q, r)| {
        let v = c.tile_at(q, r);
        ((q, r), (v.elevation, if v.water.is_some() { 0 } else { v.cover.fullness() }))
    }).collect();
    let mut best: Vec<(f64, (i32, i32), (i32, i32), f64, u8)> = Vec::new();
    for (&(q, r), &(z, _)) in &views {
        for (&(oq, or), &(oz, f)) in &views {
            let d = ((oq - q).abs() + (or - r).abs() + (oq - q + or - r).abs()) / 2;
            if f < 2 || d < 30 || d > 120 {
                continue;
            }
            let drop = z - oz;
            if drop > 12.0 {
                best.push((drop / (d as f64).sqrt(), (q, r), (oq, or), drop, f));
            }
        }
    }
    best.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    for (score, ridge, wood, drop, f) in best.iter().take(5) {
        println!("ridge {:?} z {:.0} over wood {:?} (fullness {f}): drop {drop:.0} z, score {score:.1}", ridge, views[ridge].0, wood);
    }
    let near: Vec<((i32, i32), u8)> = views.iter().filter(|(_, (_, f))| *f >= 3).map(|(k, (_, f))| (*k, *f)).take(5).collect();
    println!("wooded tiles near the spawn: {near:?}");
}
