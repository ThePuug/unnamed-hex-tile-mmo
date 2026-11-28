//! What the far view costs in memory: each summary band about the haven
//! built as the server builds it, a block of summaries to a thread, the
//! process's private bytes after the band's ground is warmed and again
//! after its summaries are read.
//!
//! Run: cargo test -p world --release --test far_view_memory_probe -- --ignored --nocapture

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use common::summary::{center_tile, part_offsets, part_width, scale, summarize, LOD_LEVELS};
use world::events::Composite;
use world::hex_to_world;

const HAVEN: (i32, i32) = (107_289, -7_677);
const LIMIT: u64 = 24 << 30;
/// Summaries along a side of a block, one thread's request.
const BLOCK: i32 = 8;
/// `common_bevy::summary::threshold_horiz`, in tiles: a band's outer edge.
fn edge(r: u32) -> f64 {
    let k = (21.0 * 28.5 + 40.0) / 6.0;
    (scale(r) as f64 * 2.0 * k - 40.0) / 3f64.sqrt()
}

#[cfg(windows)]
fn private_bytes() -> u64 {
    #[repr(C)]
    #[derive(Default)]
    struct Counters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set: usize,
        working_set: usize,
        quota_peak_paged: usize,
        quota_paged: usize,
        quota_peak_non_paged: usize,
        quota_non_paged: usize,
        pagefile: usize,
        peak_pagefile: usize,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn K32GetProcessMemoryInfo(process: isize, counters: *mut Counters, cb: u32) -> i32;
    }
    let mut c = Counters { cb: std::mem::size_of::<Counters>() as u32, ..Default::default() };
    unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) };
    c.pagefile as u64
}

#[cfg(not(windows))]
fn private_bytes() -> u64 {
    let s = std::fs::read_to_string("/proc/self/statm").unwrap();
    s.split_whitespace().nth(1).unwrap().parse::<u64>().unwrap() * 4096
}

/// The summaries of radius `r` whose centres lie past `inner` and within
/// `outer` tiles of the haven, in blocks.
fn blocks(r: u32, inner: f64, outer: f64) -> Vec<Vec<(i32, i32)>> {
    let s = scale(r);
    let (hx, hy) = hex_to_world(HAVEN.0, HAVEN.1);
    let (cq, cr) = (HAVEN.0.div_euclid(s), HAVEN.1.div_euclid(s));
    let span = (outer / s as f64 * 1.2) as i32 + 2;
    let mut by: std::collections::BTreeMap<(i32, i32), Vec<(i32, i32)>> = Default::default();
    for sq in cq - span..=cq + span {
        for sr in cr - span..=cr + span {
            let (q, t) = center_tile(r, sq, sr);
            let (x, y) = hex_to_world(q, t);
            let d = (x - hx).hypot(y - hy);
            if d > inner && d <= outer {
                by.entry((sq.div_euclid(BLOCK), sr.div_euclid(BLOCK))).or_default().push((sq, sr));
            }
        }
    }
    by.into_values().collect()
}

fn each(blocks: &[Vec<(i32, i32)>], f: impl Fn(&[(i32, i32)]) + Sync) {
    let next = AtomicUsize::new(0);
    // THREADS=<n> overrides the server's half of the cores.
    let threads = std::env::var("THREADS").ok().map_or_else(|| std::thread::available_parallelism().map_or(4, |n| n.get() / 2).max(1), |n| n.parse().unwrap());
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| loop {
                let k = next.fetch_add(1, Ordering::Relaxed);
                let Some(block) = blocks.get(k) else { break };
                f(block);
            });
        }
    });
}

fn report(c: &Composite, what: &str, began: Instant, last: &mut u64) {
    let now = private_bytes();
    let m = c.drain_metrics();
    let cells: Vec<String> = m.layers.iter().filter(|l| l.indexed > 100).map(|l| format!("{}={}", l.name, l.indexed)).collect();
    println!("{what:<34} {:>6} MB ({:>+6} MB) {:>6.1}s  cells {}", now >> 20, (now as i64 - *last as i64) >> 20, began.elapsed().as_secs_f64(), cells.join(" "));
    *last = now;
    assert!(now < LIMIT, "past the limit");
}

#[test]
#[ignore]
fn memory_by_band() {
    let c = Composite::standard(world::WORLD_SEED);
    let began = Instant::now();
    let mut last = private_bytes();
    c.tile_at(HAVEN.0, HAVEN.1);
    report(&c, "the haven's tile", began, &mut last);
    for k in 2..LOD_LEVELS.len() {
        let r = LOD_LEVELS[k];
        let (inner, outer) = (edge(LOD_LEVELS[k - 1]), edge(r));
        // The last band in rings, to tell a cost by area from one by step
        let rings = if k == LOD_LEVELS.len() - 1 { 4 } else { 1 };
        for ring in 0..rings {
            let a = inner + (outer - inner) * ring as f64 / rings as f64;
            let b = inner + (outer - inner) * (ring + 1) as f64 / rings as f64;
            let blocks = blocks(r, a, b);
            let count: usize = blocks.iter().map(|b| b.len()).sum();
            let began = Instant::now();
            each(&blocks, |block| {
                let cells: Vec<(i64, i64)> = block.iter().map(|&(sq, sr)| (sq as i64, sr as i64)).collect();
                c.warm_level(scale(r) as i64, &cells);
                let part = part_width(r);
                let parts: Vec<(i64, i64)> = block
                    .iter()
                    .flat_map(|&(sq, sr)| {
                        let (cq, cr) = center_tile(r, sq, sr);
                        part_offsets(r).map(|(dq, dr)| ((cq + dq) as i64 / part, (cr + dr) as i64 / part))
                    })
                    .collect();
                c.warm_level(part, &parts);
            });
            report(&c, &format!("r={r} {a:.0}..{b:.0}: {count} warmed"), began, &mut last);
            let began = Instant::now();
            each(&blocks, |block| {
                for &(sq, sr) in block {
                    summarize(r, sq, sr, &c).expect("the stack has every tile");
                }
            });
            report(&c, &format!("r={r} {a:.0}..{b:.0}: summarized"), began, &mut last);
        }
    }
}

/// A few blocks of the last band, warmed and then summarized on one
/// thread: what each layer's own code cost, and the memory each step took.
#[test]
#[ignore]
fn cost_by_layer() {
    let c = Composite::standard(world::WORLD_SEED);
    let began = Instant::now();
    let mut last = private_bytes();
    c.tile_at(HAVEN.0, HAVEN.1);
    report(&c, "the haven's tile", began, &mut last);
    let r = *LOD_LEVELS.last().unwrap();
    // RING=<inner>,<outer>,<step> reads every step-th block of a ring.
    let ring = std::env::var("RING").ok().map(|s| s.split(',').map(|v| v.parse::<f64>().unwrap()).collect::<Vec<_>>());
    let blocks: Vec<Vec<(i32, i32)>> = match ring {
        Some(v) => blocks(r, v[0], v[1]).into_iter().step_by(v[2] as usize).collect(),
        None => blocks(r, 21_000.0, 23_000.0).into_iter().step_by(40).take(6).collect(),
    };
    let quiet = std::env::var_os("RING").is_some();
    for block in &blocks {
        let began = Instant::now();
        let cells: Vec<(i64, i64)> = block.iter().map(|&(sq, sr)| (sq as i64, sr as i64)).collect();
        c.warm_level(scale(r) as i64, &cells);
        let part = part_width(r);
        let parts: Vec<(i64, i64)> = block
            .iter()
            .flat_map(|&(sq, sr)| {
                let (cq, cr) = center_tile(r, sq, sr);
                part_offsets(r).map(|(dq, dr)| ((cq + dq) as i64 / part, (cr + dr) as i64 / part))
            })
            .collect();
        c.warm_level(part, &parts);
        let warmed = private_bytes();
        let at = began.elapsed();
        for &(sq, sr) in block {
            summarize(r, sq, sr, &c).expect("the stack has every tile");
        }
        let now = private_bytes();
        println!(
            "block at {:?}: {} summaries, warmed {:+} MB in {:.1}s, summarized {:+} MB in {:.1}s",
            block[0],
            block.len(),
            (warmed as i64 - last as i64) >> 20,
            at.as_secs_f64(),
            (now as i64 - warmed as i64) >> 20,
            (began.elapsed() - at).as_secs_f64()
        );
        let dull = quiet && (began.elapsed().as_secs_f64() < 3.0 && (now as i64 - last as i64) >> 20 < 150);
        last = now;
        for l in c.drain_metrics().layers {
            if dull {
                continue;
            }
            let s = |s: world::events::Spent| format!("{:>6}x {:>8.2}s", s.count, s.time.as_secs_f64());
            if l.deform.time.as_millis() + l.prepare.time.as_millis() + l.query.time.as_millis() > 20 {
                println!("    {:<11} cells {:>6}  deform {}  prepare {}  query {}", l.name, l.indexed, s(l.deform), s(l.prepare), s(l.query));
            }
        }
    }
}

/// One costly patch of the last band: its parts warmed, then the six cells
/// round each part warmed in one request, then its summaries read.
#[test]
#[ignore]
fn costly_patch() {
    let c = Composite::standard(world::WORLD_SEED);
    c.tile_at(HAVEN.0, HAVEN.1);
    let r = *LOD_LEVELS.last().unwrap();
    let ring = std::env::var_os("RING_FIRST").is_some();
    let block: Vec<(i32, i32)> = (504..512).flat_map(|sq| (-32..-16).map(move |sr| (sq, sr))).collect();
    let part = part_width(r);
    let parts: Vec<(i64, i64)> = block
        .iter()
        .flat_map(|&(sq, sr)| {
            let (cq, cr) = center_tile(r, sq, sr);
            part_offsets(r).map(|(dq, dr)| ((cq + dq) as i64 / part, (cr + dr) as i64 / part))
        })
        .collect();
    let mut last = private_bytes();
    let mut step = |what: &str, f: &dyn Fn()| {
        let began = Instant::now();
        f();
        let now = private_bytes();
        println!("{what:<28} {:>+6} MB {:>7.2}s", (now as i64 - last as i64) >> 20, began.elapsed().as_secs_f64());
        last = now;
    };
    step("cells warmed", &|| c.warm_level(scale(r) as i64, &block.iter().map(|&(q, t)| (q as i64, t as i64)).collect::<Vec<_>>()));
    step("parts warmed", &|| c.warm_level(part, &parts));
    let wet = c.level(part, &parts).iter().filter(|p| p.is_some_and(|p| p.2.is_some())).count();
    let none = c.level(part, &parts).iter().filter(|p| p.is_none()).count();
    println!("{} parts: {wet} under water, {none} no ladder's", parts.len());
    let own: Vec<(i64, i64)> = block.iter().map(|&(q, t)| (q as i64, t as i64)).collect();
    let orphan: Vec<(i32, i32)> = block.iter().zip(c.level(scale(r) as i64, &own)).filter(|(_, v)| v.is_none()).map(|(b, _)| *b).collect();
    let with_land = orphan.iter().filter(|&&(sq, sr)| {
        let (cq, cr) = center_tile(r, sq, sr);
        let at: Vec<(i64, i64)> = part_offsets(r).iter().map(|&(dq, dr)| ((cq + dq) as i64 / part, (cr + dr) as i64 / part)).collect();
        c.level(part, &at).iter().any(|p| p.is_some())
    }).count();
    println!("{} summaries: {} whose own cell no ladder holds, {with_land} of those with a part a ladder holds", block.len(), orphan.len());
    if ring {
        let around: Vec<(i64, i64)> = parts.iter().flat_map(|&(i, j)| [(1, 0), (1, -1), (0, -1), (-1, 0), (-1, 1), (0, 1)].map(|(di, dj)| (i + di, j + dj))).collect();
        step("rings warmed in one request", &|| c.warm_level(part, &around));
    }
    let ladders = c.ladders().expect("the standard stack has ladders");
    let mut sites: Vec<(i32, i32)> = parts.iter().filter_map(|&(i, j)| ladders.site_of((i * part) as i32, (j * part) as i32)).collect();
    sites.sort_unstable();
    sites.dedup();
    let spent = |site| ladders.with(site, |l| (l.terrain().ladder().timing(), l.terrain().ladder().levels.iter().map(|lv| (lv.w, lv.cost())).collect::<Vec<_>>()));
    let before: Vec<_> = sites.iter().map(|&s| spent(s)).collect();
    step("summarized", &|| {
        for &(sq, sr) in &block {
            summarize(r, sq, sr, &c).expect("the stack has every tile");
        }
    });
    for (&site, (t0, c0)) in sites.iter().zip(before) {
        let (t1, c1) = spent(site);
        let held = parts.iter().filter(|&&(i, j)| ladders.site_of((i * part) as i32, (j * part) as i32) == Some(site)).count();
        println!("site {site:?}: {held} parts");
        for (k, ((a, b), ((w, ca), (_, cb)))) in t0.iter().zip(&t1).zip(c0.iter().zip(&c1)).enumerate() {
            let dt: Vec<String> = world::ladder::STAGES.iter().zip(a.iter().zip(b)).filter(|(_, (a, b))| *b - *a > 0.01).map(|(n, (a, b))| format!("{n} {:.2}s", b - a)).collect();
            let dc: Vec<u64> = ca.iter().zip(cb).map(|(a, b)| b - a).collect();
            if !dt.is_empty() || dc.iter().any(|&n| n > 0) {
                println!("  level {k} (w={w}): {} | deforms and cells {dc:?}", dt.join(", "));
            }
        }
    }
}
