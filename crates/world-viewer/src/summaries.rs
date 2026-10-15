//! `world-viewer summaries [q,r]`: the summaries the server sends a player
//! standing at a tile, the haven by default, made as the server makes them,
//! a region's cells made ready together and then read, the finest band
//! first; each band timed, for timing what the ladder's far levels cost.

use std::collections::BTreeMap;
use std::time::Instant;

use common::summary::SummarySource;
use common_bevy::geometry::flat_top_tile_center;
use common_bevy::summary::{compute_active_bands, mesh_region_lattice, reach_wu, LOD_LEVELS};
use common_bevy::summary_mesh::visible_lod_regions;
use world::events::Composite;

pub fn run(args: &[String]) {
    let haven = common_bevy::haven::launch();
    let at = args.first().map_or((haven.q, haven.r), |s| {
        let (q, r) = s.split_once(',').expect("a tile as q,r");
        (q.trim().parse().expect("q"), r.trim().parse().expect("r"))
    });
    let world = Composite::standard(world::WORLD_SEED);
    let (wx, wz) = flat_top_tile_center(at.0, at.1, 1.0);
    let bands = compute_active_bands(reach_wu());
    let mut regions = visible_lod_regions(&bands, wx, wz, common_bevy::chunk::FIXED_STREAM_APOTHEM_WU, 0);
    regions.extend(visible_lod_regions(&bands, wx, wz, 0.0, 0).into_iter().filter(|rk| rk.r >= LOD_LEVELS[1]));
    let mut by_band: BTreeMap<u32, Vec<(i32, i32)>> = BTreeMap::new();
    for rk in regions.into_iter().filter(|rk| rk.r >= LOD_LEVELS[1]) {
        by_band.entry(rk.r).or_default().push((rk.mn, rk.mm));
    }
    let rl = mesh_region_lattice();
    let started = Instant::now();
    println!("summaries round ({}, {}), the finest band first", at.0, at.1);
    for (r, mut keys) in by_band {
        keys.sort_unstable();
        let w = common::summary::scale(r) as i64;
        let band = Instant::now();
        let mut said = Instant::now();
        let (mut cells, mut wet) = (0usize, 0usize);
        for (n, &key) in keys.iter().enumerate() {
            let tiles: Vec<(i64, i64)> = rl.tiles_in_cell(key).map(|(q, r)| (q as i64, r as i64)).collect();
            world.warm_level(w, &tiles);
            for &(sq, sr) in &tiles {
                if let Some((_, parts)) = world.cell(r, sq as i32, sr as i32) {
                    cells += 1;
                    wet += usize::from(parts != 0);
                }
            }
            if said.elapsed().as_secs() >= 30 {
                println!("  band {w}: {}/{} regions at {:.1} s", n + 1, keys.len(), band.elapsed().as_secs_f64());
                said = Instant::now();
            }
        }
        println!(
            "band {w:>2}: {:>4} regions, {cells:>6} summaries, {wet:>5} with water, {:.1} s",
            keys.len(),
            band.elapsed().as_secs_f64()
        );
    }
    println!("all bands: {:.1} s", started.elapsed().as_secs_f64());
}
