//! Spawner density probe — measures how many spawner placements the current
//! SpawnerEvent settings produce over a region, and how far apart they are.

//! Not a pass/fail test — prints density + spacing stats so tuning decisions are
//! grounded in real numbers rather than guesses.

//! Run: cargo test -p world --release --test spawner_density -- --ignored --nocapture

use std::sync::Arc;

use common::PlateTag;
use world::events::Composite;
use world::events::plates::PlateEvent;
use world::events::spawner::{archetype_for_tagset, SpawnerArchetype, SpawnerEvent, SpawnerPlacementIndex};
use world::events::spines::SpineEvent;
use world::PlateCache;

const SEED: u64 = 0x9E3779B97F4A7C15;

/// AOI radius in tiles (server constant) — how far a player sees.
const AOI_RADIUS: i32 = 123;

fn composite_full() -> Composite {
    let plate_cache = Arc::new(PlateCache::new(SEED));
    let mut c = Composite::new(SEED);
    c.add_event(Box::new(PlateEvent::with_cache(plate_cache.clone())));
    c.add_event(Box::new(SpineEvent::with_cache(plate_cache, SEED)));
    c.add_event(Box::new(SpawnerEvent::new(SEED)));
    c
}

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

fn hex_distance(a: (i32, i32), b: (i32, i32)) -> i32 {
    let dq = a.0 - b.0;
    let dr = a.1 - b.1;
    dq.abs().max(dr.abs()).max((dq + dr).abs())
}

#[test]
#[ignore]
fn spawner_density_probe() {
    let c = composite_full();

    // Sample several scattered regions to get a representative picture of terrain
    // composition across the world (spawn point sits inside one spine's foothills).
    let centers = [
        (3423, 1155),   // server spawn
        (10000, -5000),
        (-8000, 8000),
        (20000, 3000),
        (5000, 20000),
        (-15000, -10000),
    ];
    for &(center_q, center_r) in &centers {
        probe_region(&c, center_q, center_r, 120);
    }
}

fn probe_region(c: &Composite, center_q: i32, center_r: i32, region_radius: i32) {
    let tiles = hexball(center_q, center_r, region_radius);
    let total_tiles = tiles.len();

    // Warm the spawner index over the whole region.
    c.ensure_indexed(&tiles);

    // Count land tiles (positive elevation) and their terrain composition.
    let mut land_tiles = 0usize;
    let mut ridge = 0usize;
    let mut highland = 0usize;
    let mut foothills = 0usize;
    let mut flat_inland = 0usize; // Inland with no spine tag → Kiter (the carpet)
    for &(q, r) in &tiles {
        let view = c.tile_at(q, r);
        if view.elevation <= 0.0 { continue; }
        land_tiles += 1;
        if view.tags.has(PlateTag::Ridge) { ridge += 1; }
        else if view.tags.has(PlateTag::Highland) { highland += 1; }
        else if view.tags.has(PlateTag::Foothills) { foothills += 1; }
        else if view.tags.has(PlateTag::Inland) { flat_inland += 1; }
    }

    // Pull every placement whose tile falls inside the region.
    let placements: Vec<(i32, i32)> = c.with_indexes(|idx| {
        let Some(spawner) = idx.get::<SpawnerPlacementIndex>() else { return Vec::new(); };
        spawner
            .cells
            .values()
            .flat_map(|v| v.iter().map(|p| (p.q, p.r)))
            .filter(|&(q, r)| hex_distance((center_q, center_r), (q, r)) <= region_radius)
            .collect()
    });

    let n = placements.len();

    // Resolve each placement's archetype from the composite tags (deform stores
    // a Kiter placeholder; query resolves the real archetype from tags below).
    let mut by_arch = [0usize; 4]; // Berserker, Juggernaut, Kiter, Defender
    for &(q, r) in &placements {
        if let Some(a) = archetype_for_tagset(&c.tile_at(q, r).tags) {
            let idx = match a {
                SpawnerArchetype::Berserker => 0,
                SpawnerArchetype::Juggernaut => 1,
                SpawnerArchetype::Kiter => 2,
                SpawnerArchetype::Defender => 3,
            };
            by_arch[idx] += 1;
        }
    }

    // Nearest-neighbor spacing distribution.
    let mut nn: Vec<i32> = placements
        .iter()
        .map(|&p| {
            placements
                .iter()
                .filter(|&&q| q != p)
                .map(|&q| hex_distance(p, q))
                .min()
                .unwrap_or(i32::MAX)
        })
        .filter(|&d| d != i32::MAX)
        .collect();
    nn.sort_unstable();

    let median_nn = if nn.is_empty() { 0 } else { nn[nn.len() / 2] };
    let min_nn = nn.first().copied().unwrap_or(0);
    let mean_nn = if nn.is_empty() {
        0.0
    } else {
        nn.iter().sum::<i32>() as f64 / nn.len() as f64
    };

    // Density expressed multiple ways.
    let per_1k_land = if land_tiles == 0 {
        0.0
    } else {
        n as f64 * 1000.0 / land_tiles as f64
    };
    // Expected count within one AOI footprint (3R^2+3R+1 tiles).
    let aoi_tiles = 3 * AOI_RADIUS * AOI_RADIUS + 3 * AOI_RADIUS + 1;
    let region_density = n as f64 / total_tiles as f64;
    let expected_in_aoi = region_density * aoi_tiles as f64;

    if land_tiles == 0 {
        println!("\n=== region ({center_q},{center_r}) r{region_radius}: all sea, skipped ===");
        return;
    }

    println!("\n=== region ({center_q},{center_r}) r{region_radius} ===");
    println!("  total tiles in region : {total_tiles}");
    println!("  land tiles (elev > 0) : {land_tiles}");
    println!("    land composition    : ridge {ridge} / highland {highland} / foothills {foothills} / flat-inland {flat_inland}");
    println!("    spine-feature land  : {} ({:.1}% of land)", ridge + highland + foothills,
        (ridge + highland + foothills) as f64 * 100.0 / land_tiles.max(1) as f64);
    println!("  spawner placements    : {n}");
    println!("    by archetype        : berserker(highland) {} / juggernaut(foothills) {} / defender(ridge) {} / kiter(flat-inland) {}",
        by_arch[0], by_arch[1], by_arch[3], by_arch[2]);
    println!("  placements / 1k land  : {per_1k_land:.2}");
    println!("  expected within AOI {AOI_RADIUS} : {expected_in_aoi:.1}");
    println!("  nearest-neighbor dist : min {min_nn}, median {median_nn}, mean {mean_nn:.1}");
    println!();
}
