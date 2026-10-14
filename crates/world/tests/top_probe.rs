//! Each site's top: its size and how long its ladder takes to build, and
//! the band along the edge of the land it owns, every tile of it, read for
//! land. A tile is a site's land where its position as the plates are drawn
//! lies within `LAND_REACH` of a plate the site claimed
//! (`continents::land_site`), and the site's top holds every such tile by
//! construction; land past every site's reach is the one way a tile could
//! have no top. Two neighbouring tiles are drawn at most `WARP_STEP` apart,
//! so land running out of a site's reach stands on a tile of the band whose
//! drawn position lies within `WARP_STEP` inside the reach: a band of no
//! land is land that never leaves.
//!
//! With `TOP_MARGIN=1` it also reads every tile the top holds past the
//! site's land reach, its sea margin, for land: an islet standing wholly
//! past the reach, which the band cannot see.
//!
//! `TOP_SITES="i,j;i,j"` names the sites, the haven's by default.
//!
//! Run: `cargo test --release -p world --features inspect --test top_probe -- --ignored --nocapture`

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;

use world::continents::{self, LAND_REACH};
use world::events::ladder_ground::{top_bounds, Laddered};
use world::events::plates::{warp, WARP_STEP};
use world::events::Composite;
use world::ladder::geom::cube_round;
use world::ladder::Params;
use world::{hex_to_world, WORLD_SEED};

const HAVEN: (i32, i32) = (107289, -7677);

/// Tiles a side of the blocks the band is sought in.
const BLOCK: i32 = 27;

#[test]
#[ignore]
fn top_probe() {
    let started = Instant::now();
    let progress = |now: &str| println!("progress {:.0}s: {now}", started.elapsed().as_secs_f64());
    let (hx, hy) = hex_to_world(HAVEN.0, HAVEN.1);
    let mut sites = vec![continents::land_site(hx, hy, WORLD_SEED).expect("the haven stands on land")];
    if let Ok(more) = std::env::var("TOP_SITES") {
        sites = more
            .split(';')
            .filter(|s| !s.is_empty())
            .map(|s| {
                let (i, j) = s.split_once(',').expect("a site is i,j");
                (i.trim().parse().expect("a whole i"), j.trim().parse().expect("a whole j"))
            })
            .collect();
    }
    let mut seams = 0;
    for id in sites {
        let Some((w, lo, hi)) = top_bounds(WORLD_SEED, id) else {
            println!("site {id:?}: claims no land");
            continue;
        };
        progress(&format!("site {id:?}: building its ladder"));
        let t = Instant::now();
        let laddered = Laddered::with(WORLD_SEED, id, Params::default());
        let seconds = t.elapsed().as_secs_f64();
        let (land_cells, peak) = {
            let ladder = laddered.terrain().ladder();
            let top = &ladder.levels[0];
            let land = (0..top.cap()).filter(|&c| top.exists[c] && top.z[c] >= 0.0).count();
            let peak = (0..top.cap()).filter(|&c| top.exists[c]).map(|c| top.z[c]).fold(f64::NEG_INFINITY, f64::max);
            (land, peak)
        };
        drop(laddered);
        println!(
            "site {id:?}: top {w} wide, {} by {} = {} cells, {land_cells} on land, peak {peak}; ladder built in {seconds:.1} s",
            hi.0 - lo.0,
            hi.1 - lo.1,
            (hi.0 - lo.0) * (hi.1 - lo.1)
        );

        progress(&format!("site {id:?}: reading the edge band"));
        let land = continents::land(id, WORLD_SEED);
        let held = |q: i32, r: i32| {
            let (i, j) = cube_round(q as f64 / w as f64, r as f64 / w as f64);
            i >= lo.0 && i < hi.0 && j >= lo.1 && j < hi.1
        };
        // A block's drawn positions lie within this of its centre's.
        let spread = 2.0 * BLOCK as f64 * WARP_STEP;
        let (q0, q1) = ((lo.0 * w - w) as i32, (hi.0 * w + w) as i32);
        let (r0, r1) = ((lo.1 * w - w) as i32, (hi.1 * w + w) as i32);
        let rows: Vec<i32> = (r0..r1).step_by(BLOCK as usize).collect();
        let base = Composite::base(WORLD_SEED);
        let (band, wet, out, done) = (AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicUsize::new(0));
        let examples = std::sync::Mutex::new(Vec::new());
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
        std::thread::scope(|s| {
            for part in rows.chunks(rows.len().div_ceil(threads)) {
                let (land, base, band, wet, out, done, examples, held) = (&land, &base, &band, &wet, &out, &done, &examples, &held);
                let count = rows.len();
                s.spawn(move || {
                    for &br in part {
                        for bq in (q0..q1).step_by(BLOCK as usize) {
                            let (cx, cy) = hex_to_world(bq + BLOCK / 2, br + BLOCK / 2);
                            let (dx, dy) = warp(cx, cy, WORLD_SEED);
                            let gap = land.gap(dx, dy);
                            if gap > LAND_REACH + spread || gap < LAND_REACH - WARP_STEP - spread {
                                continue;
                            }
                            for r in br..br + BLOCK {
                                for q in bq..bq + BLOCK {
                                    let (wx, wy) = hex_to_world(q, r);
                                    let (x, y) = warp(wx, wy, WORLD_SEED);
                                    let g = land.gap(x, y);
                                    if g <= LAND_REACH - WARP_STEP || g > LAND_REACH {
                                        continue;
                                    }
                                    band.fetch_add(1, Ordering::Relaxed);
                                    if !held(q, r) {
                                        out.fetch_add(1, Ordering::Relaxed);
                                    }
                                    if base.elevation_at(q, r) >= 0 {
                                        wet.fetch_add(1, Ordering::Relaxed);
                                        examples.lock().unwrap().push((q, r));
                                    }
                                }
                            }
                        }
                        let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                        if n % 200 == 0 {
                            println!("progress: site {id:?}, {n} of {count} block rows");
                        }
                    }
                });
            }
        });
        let (band, wet, out) = (band.into_inner(), wet.into_inner(), out.into_inner());
        let mut examples = examples.into_inner().unwrap();
        examples.sort();
        let first = if examples.is_empty() { String::new() } else { format!("; first {:?}", &examples[..examples.len().min(8)]) };
        println!(
            "site {id:?}: edge band, every tile drawn within {WARP_STEP:.1} inside the land reach {LAND_REACH:.0}: {band} tiles; on land {wet}; outside the top {out}{first}"
        );
        seams += wet + out;

        if std::env::var("TOP_MARGIN").is_ok_and(|v| v == "1") {
            progress(&format!("site {id:?}: reading the sea margin"));
            let (sea, islets, done) = (AtomicU64::new(0), AtomicU64::new(0), AtomicUsize::new(0));
            let found = std::sync::Mutex::new(Vec::new());
            std::thread::scope(|s| {
                for part in rows.chunks(rows.len().div_ceil(threads)) {
                    let (land, base, sea, islets, done, found, held) = (&land, &base, &sea, &islets, &done, &found, &held);
                    let count = rows.len();
                    s.spawn(move || {
                        for &br in part {
                            for bq in (q0..q1).step_by(BLOCK as usize) {
                                let (cx, cy) = hex_to_world(bq + BLOCK / 2, br + BLOCK / 2);
                                let (dx, dy) = warp(cx, cy, WORLD_SEED);
                                if land.gap(dx, dy) < LAND_REACH - spread {
                                    continue;
                                }
                                for r in br..br + BLOCK {
                                    for q in bq..bq + BLOCK {
                                        if !held(q, r) {
                                            continue;
                                        }
                                        let (wx, wy) = hex_to_world(q, r);
                                        let (x, y) = warp(wx, wy, WORLD_SEED);
                                        if land.gap(x, y) <= LAND_REACH {
                                            continue;
                                        }
                                        sea.fetch_add(1, Ordering::Relaxed);
                                        if base.elevation_at(q, r) >= 0 {
                                            islets.fetch_add(1, Ordering::Relaxed);
                                            found.lock().unwrap().push((q, r));
                                        }
                                    }
                                }
                            }
                            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                            if n % 100 == 0 {
                                println!("progress: site {id:?} margin, {n} of {count} block rows");
                            }
                        }
                    });
                }
            });
            let (sea, islets) = (sea.into_inner(), islets.into_inner());
            let mut found = found.into_inner().unwrap();
            found.sort();
            let first = if found.is_empty() { String::new() } else { format!("; first {:?}", &found[..found.len().min(8)]) };
            println!("site {id:?}: sea margin, every tile the top holds past the land reach: {sea} tiles; on land {islets}{first}");
            seams += islets;
        }
    }
    println!("done in {:.1} s", started.elapsed().as_secs_f64());
    assert_eq!(seams, 0, "land at the edge of a site's reach, or a band tile outside its top");
}
