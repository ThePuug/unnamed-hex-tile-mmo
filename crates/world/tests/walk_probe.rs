//! Walk probe — what a walker meets on the range nearest the spawn: where
//! it is, its cross-sections at a culmination and a saddle, and on a patch
//! of its frontal sheet the edges a step refuses, what a walker can reach
//! from the foot and from behind, the cover, and a hillshade.
//!
//! Not a pass/fail test. A walker steps up at most one level per tile
//! (`common_bevy::systems::movement::is_tile_blocked`), two with a jump,
//! drops any distance, and never enters a tile its solid cover fills
//! (`COVER_FULL`). Elevations are rounded as `Composite::elevation_at`
//! rounds them; the ground before the ladder cut it is `Composite::base`.
//!
//! Run one test at a time, each builds the ladder over what it reads:
//!   cargo test -p world --release --test walk_probe -- --ignored --nocapture locate_range
//!   cargo test -p world --release --test walk_probe -- --ignored --nocapture sections
//!   cargo test -p world --release --test walk_probe -- --ignored --nocapture forelimb_patch
//!   cargo test -p world --release --test walk_probe -- --ignored --nocapture convergent_coast
//! Env: CULM, SADDLE (along-strike positions overriding the found ones);
//! PATCH="s0,s1" (along-strike span of the patch); FORE, BACK (the patch's
//! reach out past the front and in from it, tiles). Images land in
//! proofs/world/bare-ranges/.

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::time::Instant;

use common::{Content, SITES};
use world::events::plates::warp;
use world::events::thrusting::{rim_of, EdgeOutline, Outlines, RANGE_END};
use world::events::Composite;
use world::tectonic::PlateId;
use world::{hex_to_world, world_to_hex, RISE};

const SEED: u64 = world::WORLD_SEED;
const SPAWN: (i32, i32) = (104289, -4677);
const NB6: [(i32, i32); 6] = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, -1), (-1, 1)];
const BIN: f64 = 50.0;

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
    0
}

fn env_f(name: &str) -> Option<f64> {
    std::env::var(name).ok().and_then(|v| v.trim().parse().ok())
}

/// Maps `f` over `items` on every core, each thread a contiguous run so it
/// reads one stretch of ground.
fn par_map<T: Sync, R: Send + Default + Clone>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 16);
    let chunk = items.len().div_ceil(n).max(1);
    let mut out = vec![R::default(); items.len()];
    std::thread::scope(|s| {
        for (src, dst) in items.chunks(chunk).zip(out.chunks_mut(chunk)) {
            let f = &f;
            std::thread::Builder::new()
                .stack_size(64 << 20)
                .spawn_scoped(s, move || {
                    for (a, b) in src.iter().zip(dst) {
                        *b = f(a)
                    }
                })
                .unwrap();
        }
    });
    out
}

// ── The range ───────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug)]
struct Bin {
    along: f64,
    crest: f64,
    wx: f64,
    wy: f64,
    d: f64,
    /// Distance to the plate's nearest other edge at the crest point.
    others: f64,
}

struct Range {
    outlines: Outlines,
    plate: PlateId,
    edge: usize,
    bins: Vec<Bin>,
    nearest: (f64, f64, f64),
}

impl Range {
    fn edge(&self) -> &EdgeOutline {
        &self.outlines.plate(self.plate).unwrap().edges[self.edge]
    }

    /// Signed distance from the front, positive into the overriding plate,
    /// and the position along strike, of a world position.
    fn frame(&self, wx: f64, wy: f64) -> (f64, f64) {
        let (x, y) = warp(wx, wy, SEED);
        let e = self.edge();
        let (d, side) = e.distance(x, y);
        (if side > 0.0 { d } else { -d }, e.along(x, y))
    }

    /// Unit vector in world space pointing away from the front, into the
    /// overriding plate, at a world position.
    fn normal(&self, wx: f64, wy: f64) -> (f64, f64) {
        let h = 4.0;
        let gx = self.frame(wx + h, wy).0 - self.frame(wx - h, wy).0;
        let gy = self.frame(wx, wy + h).0 - self.frame(wx, wy - h).0;
        let l = gx.hypot(gy);
        (gx / l, gy / l)
    }

    fn bin_at(&self, along: f64) -> Bin {
        *self.bins.iter().min_by(|a, b| (a.along - along).abs().partial_cmp(&(b.along - along).abs()).unwrap()).unwrap()
    }

    fn culmination(&self) -> Bin {
        if let Some(a) = env_f("CULM") {
            return self.bin_at(a);
        }
        *self.full_bins().max_by(|a, b| a.crest.partial_cmp(&b.crest).unwrap()).unwrap()
    }

    /// The lowest crest within 4000 along of the culmination, where the
    /// range stands clear of the plate's other edges.
    fn saddle(&self) -> Bin {
        if let Some(a) = env_f("SADDLE") {
            return self.bin_at(a);
        }
        let c = self.culmination();
        *self
            .full_bins()
            .filter(|b| (b.along - c.along).abs() <= 4000.0)
            .min_by(|a, b| a.crest.partial_cmp(&b.crest).unwrap())
            .unwrap()
    }

    fn full_bins(&self) -> impl Iterator<Item = &Bin> {
        self.bins.iter().filter(|b| b.others >= RANGE_END && b.crest > 0.0)
    }
}

fn locate() -> Range {
    let (sx, sy) = hex_to_world(SPAWN.0, SPAWN.1);
    let outlines = Outlines::in_box(sx, sy, 16_000.0, SEED);
    // The nearest substantial range on a coarse grid.
    let mut nearest = (f64::MAX, 0.0, 0.0);
    let mut i = -8000.0;
    while i <= 8000.0 {
        let mut j = -8000.0;
        while j <= 8000.0 {
            let (wx, wy) = (sx + i, sy + j);
            let d = i.hypot(j);
            if d < nearest.0 && d <= 8000.0 && outlines.relief(wx, wy) > 100.0 {
                nearest = (d, wx, wy);
            }
            j += 100.0;
        }
        i += 100.0;
    }
    assert!(nearest.0 < f64::MAX, "no range within 8000 tiles of the spawn");
    let at = outlines.at(nearest.1, nearest.2).unwrap();
    let plate = at.plate.id;
    let edge = at
        .plate
        .edges
        .iter()
        .zip(&at.distances)
        .enumerate()
        .filter(|(_, (e, _))| e.faces.is_some())
        .min_by(|a, b| a.1 .1.partial_cmp(b.1 .1).unwrap())
        .map(|(k, _)| k)
        .unwrap();
    drop(at);

    // The frontal sheet's crest along strike, on a 20-tile grid.
    let rim1 = rim_of(1.0);
    let mut bins: HashMap<i64, Bin> = HashMap::new();
    let (cx, cy) = (nearest.1, nearest.2);
    let half = 7000.0;
    let mut i = -half;
    while i <= half {
        let mut j = -half;
        while j <= half {
            let (wx, wy) = (cx + i, cy + j);
            if let Some(at) = outlines.at(wx, wy) {
                if at.plate.id == plate {
                    let d = at.distances[edge];
                    if d < rim1 {
                        let e = &at.plate.edges[edge];
                        let along = e.along(at.x, at.y);
                        let relief = outlines.relief_of(&at);
                        let others = at.distances.iter().enumerate().filter(|(k, _)| *k != edge).map(|(_, d)| *d).fold(f64::MAX, f64::min);
                        let key = (along / BIN).floor() as i64;
                        let b = bins.entry(key).or_insert(Bin { along: (key as f64 + 0.5) * BIN, crest: -1.0, wx, wy, d, others });
                        if relief > b.crest {
                            *b = Bin { along: b.along, crest: relief, wx, wy, d, others };
                        }
                    }
                }
            }
            j += 20.0;
        }
        i += 20.0;
    }
    let mut bins: Vec<Bin> = bins.into_values().collect();
    bins.sort_by(|a, b| a.along.partial_cmp(&b.along).unwrap());
    Range { outlines, plate, edge, bins, nearest }
}

#[test]
#[ignore]
fn locate_range() {
    let t = Instant::now();
    let range = locate();
    let e = range.edge();
    let (sx, sy) = hex_to_world(SPAWN.0, SPAWN.1);
    println!("\n=== nearest range to spawn {SPAWN:?} = world ({sx:.0}, {sy:.0}) ===");
    println!("  nearest point with relief > 100: world ({:.0}, {:.0}), {:.0} tiles from spawn", range.nearest.1, range.nearest.2, range.nearest.0);
    println!("  overriding plate {:?}, front against {:?}, converge {:.3}, standing {:.2}", range.plate, e.neighbour, e.converge, e.standing);
    let [x0, y0, x1, y1] = e.line;
    let strike = (y1 - y0).atan2(x1 - x0).to_degrees();
    println!("  straight edge (warped space) ({x0:.0},{y0:.0}) -> ({x1:.0},{y1:.0}), strike {strike:.1} deg from +x, length {:.0}", (x1 - x0).hypot(y1 - y0));
    let (nx, ny) = range.normal(range.nearest.1, range.nearest.2);
    println!("  at the nearest point the front faces world direction ({:.2}, {:.2}) (forelimb faces this way; plate interior is the opposite)", -nx, -ny);
    let sp = range.frame(sx, sy);
    println!("  spawn: signed distance from front {:.0} (positive inside overriding plate), along {:.0}", sp.0, sp.1);
    println!("\n  frontal-sheet crest along strike (ranges' own relief, 20-tile grid; others = distance to plate's other edges):");
    println!("  {:>8} {:>8} {:>8} {:>8} {:>6} {:>7} {:>10}", "along", "crest", "wx", "wy", "d", "others", "hex");
    for b in range.bins.iter().step_by(5) {
        let (q, r) = world_to_hex(b.wx, b.wy);
        println!("  {:8.0} {:8.1} {:8.0} {:8.0} {:6.0} {:7.0} ({q},{r})", b.along, b.crest, b.wx, b.wy, b.d, b.others.min(99999.0));
    }
    for (label, b) in [("culmination", range.culmination()), ("saddle", range.saddle())] {
        let (q, r) = world_to_hex(b.wx, b.wy);
        let (nx, ny) = range.normal(b.wx, b.wy);
        println!("  {label}: along {:.0}, crest relief {:.1}, at world ({:.0}, {:.0}) hex ({q}, {r}), {:.0} in from the front; into-plate normal ({nx:.2}, {ny:.2})", b.along, b.crest, b.wx, b.wy, b.d);
    }
    println!("  {:.1}s", t.elapsed().as_secs_f64());
}

// ── Cross-sections ──────────────────────────────────────────────────────────

#[derive(Clone)]
struct Sample {
    s: f64,
    q: i32,
    r: i32,
    d: f64,
    relief: f64,
    base: f64,
    z: i32,
    water: bool,
}

fn section_at(range: &Range, c: &Composite, base: &Composite, b: Bin, fore: f64, back: f64) -> Vec<Sample> {
    let (nx, ny) = range.normal(b.wx, b.wy);
    let (d0, _) = range.frame(b.wx, b.wy);
    let (ax, ay) = (b.wx - (d0 + fore) * nx, b.wy - (d0 + fore) * ny);
    let len = fore + back;
    let mut tiles: Vec<(i32, i32)> = Vec::new();
    let mut s = 0.0;
    while s <= len {
        let t = world_to_hex(ax + s * nx, ay + s * ny);
        if tiles.last() != Some(&t) {
            tiles.push(t)
        }
        s += 0.25;
    }
    par_map(&tiles, |&(q, r)| {
        let (cx, cy) = hex_to_world(q, r);
        let view = c.tile_at(q, r);
        let water = Composite::water_at(c, q, r).is_some();
        Some(Sample {
            s: (cx - ax) * nx + (cy - ay) * ny,
            q,
            r,
            d: range.frame(cx, cy).0,
            relief: range.outlines.relief(cx, cy),
            base: base.tile_at(q, r).elevation,
            z: view.elevation.round() as i32,
            water,
        })
    })
    .into_iter()
    .flatten()
    .collect()
}

/// Mean and max grade, levels per tile, between `from` and `to` (sample
/// indices), of a height read off each sample, and the share of adjacent
/// steps over one level.
fn grades(v: &[Sample], from: usize, to: usize, h: impl Fn(&Sample) -> f64) -> (f64, f64, f64, f64) {
    let (lo, hi) = (from.min(to), from.max(to));
    let width = (v[hi].s - v[lo].s).abs().max(1.0);
    let mean = (h(&v[hi]) - h(&v[lo])).abs() / width;
    let (mut max, mut over, mut steps) = (0.0f64, 0usize, 0usize);
    for i in lo..hi {
        let ds = (v[i + 1].s - v[i].s).abs().max(0.5);
        let g = (h(&v[i + 1]) - h(&v[i])).abs() / ds;
        let adjacent = NB6.contains(&(v[i + 1].q - v[i].q, v[i + 1].r - v[i].r));
        if adjacent {
            steps += 1;
            if (h(&v[i + 1]).round() - h(&v[i]).round()).abs() > 1.0 {
                over += 1
            }
        }
        max = max.max(g);
    }
    // Max over 10-tile windows, so one step's rounding does not decide it.
    let mut max10 = 0.0f64;
    let mut j = lo;
    for i in lo..=hi {
        while j < hi && v[j].s - v[i].s < 10.0 {
            j += 1
        }
        if v[j].s - v[i].s >= 9.5 {
            max10 = max10.max((h(&v[j]) - h(&v[i])).abs() / (v[j].s - v[i].s));
        }
    }
    let _ = max;
    (width, mean, max10, over as f64 / steps.max(1) as f64)
}

fn report_section(label: &str, v: &[Sample]) {
    println!("\n--- section across the {label} ({} tiles) ---", v.len());
    // The foot: the sample nearest the front. The crest: highest final
    // ground on the frontal sheet. The trough behind: lowest relief past it.
    let foot = (0..v.len()).min_by(|&a, &b| v[a].d.abs().partial_cmp(&v[b].d.abs()).unwrap()).unwrap();
    let rim1 = rim_of(1.0);
    let sheet: Vec<usize> = (0..v.len()).filter(|&i| v[i].d >= 0.0 && v[i].d <= rim1).collect();
    let crest = *sheet.iter().max_by_key(|&&i| v[i].z).unwrap();
    let rcrest = *sheet.iter().max_by(|&&a, &&b| v[a].relief.partial_cmp(&v[b].relief).unwrap()).unwrap();
    let trough = *sheet.iter().filter(|&&i| i > rcrest).min_by(|&&a, &&b| v[a].relief.partial_cmp(&v[b].relief).unwrap()).unwrap_or(&(v.len() - 1));
    let fore_z: Vec<i32> = v.iter().filter(|s| s.d < -50.0).map(|s| s.z).collect();
    let fore_med = { let mut f = fore_z.clone(); f.sort(); f.get(f.len() / 2).copied().unwrap_or(0) };
    println!("  foot (front) at s {:.0}: z {}  base {:.1}  relief {:.1}; foreland median z {} (min {}, max {})", v[foot].s, v[foot].z, v[foot].base, v[foot].relief, fore_med,
        fore_z.iter().min().unwrap_or(&0), fore_z.iter().max().unwrap_or(&0));
    println!("  final crest z {} at s {:.0} (d {:.0}) hex ({}, {}); relief crest {:.1} at s {:.0} (d {:.0}); base there {:.1}", v[crest].z, v[crest].s, v[crest].d, v[crest].q, v[crest].r, v[rcrest].relief, v[rcrest].s, v[rcrest].d, v[rcrest].base);
    println!("  trough behind at s {:.0} (d {:.0}): z {}  relief {:.1}", v[trough].s, v[trough].d, v[trough].z, v[trough].relief);
    println!("  crest above foot: final {}  base {:.1}  relief {:.1}", v[crest].z - v[foot].z, v[rcrest].base - v[foot].base, v[rcrest].relief - v[foot].relief);
    println!("  {:>10} {:>10} {:>8} {:>10} {:>10} {:>14}", "flank", "ground", "width", "mean lv/t", "max lv/t", "steps >1 lv");
    for (flank, a, b) in [("forelimb", foot, rcrest), ("backlimb", rcrest, trough)] {
        for (g, h) in [("final", 0usize), ("base", 1), ("relief", 2)] {
            let f = |s: &Sample| match h { 0 => s.z as f64, 1 => s.base, _ => s.relief };
            let (w, mean, max10, over) = grades(v, a, b, f);
            println!("  {flank:>10} {g:>10} {w:8.0} {mean:10.2} {max10:10.2} {:13.1}%", 100.0 * over);
        }
    }
    let fl: Vec<&Sample> = v[foot.min(rcrest)..=foot.max(rcrest)].iter().collect();
    let cut: Vec<f64> = fl.iter().map(|s| s.base - s.z as f64).collect();
    let deep = cut.iter().filter(|&&c| c >= 3.0).count();
    let wet = fl.iter().filter(|s| s.water).count();
    println!("  forelimb cut by the ladder (base - final): mean {:.1}, max {:.1}; {} of {} tiles cut 3+ levels; {} with water",
        cut.iter().sum::<f64>() / cut.len() as f64, cut.iter().cloned().fold(f64::MIN, f64::max), deep, fl.len(), wet);
    println!("  profile every ~20 tiles:");
    println!("  {:>6} {:>6} {:>8} {:>8} {:>6} {:>5} {:>14}", "s", "d", "relief", "base", "final", "water", "hex");
    let mut next = 0.0;
    for s in v {
        if s.s >= next {
            println!("  {:6.0} {:6.0} {:8.1} {:8.1} {:6} {:>5} ({},{})", s.s, s.d, s.relief, s.base, s.z, if s.water { "w" } else { "" }, s.q, s.r);
            next = s.s + 20.0;
        }
    }
}

#[test]
#[ignore]
fn sections() {
    let range = locate();
    let c = Composite::standard(SEED);
    let base = Composite::base(SEED);
    let fore = env_f("FORE").unwrap_or(300.0);
    for (label, b) in [("culmination", range.culmination()), ("saddle", range.saddle())] {
        let t = Instant::now();
        let back = rim_of(1.0) - b.d + 300.0;
        let v = section_at(&range, &c, &base, b, fore, back);
        report_section(&format!("{label} (along {:.0}, world {:.0},{:.0})", b.along, b.wx, b.wy), &v);
        println!("  {:.1}s", t.elapsed().as_secs_f64());
    }
}

// ── The patch ───────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
struct Tile {
    q: i32,
    r: i32,
    t: f64,
    along: f64,
    z: i32,
    base: i32,
    relief: f64,
    water: bool,
    growth: bool,
    boulder: bool,
    full: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Class {
    Fore,
    Forelimb,
    Back,
}

struct Patch {
    tiles: Vec<Tile>,
    index: HashMap<(i32, i32), usize>,
    s0: f64,
    bins: usize,
    /// Per along bin: the final crest's distance from the front (the
    /// nearest tile at the bin's highest final ground), the ranges' own
    /// crest's, the highest final ground on the sheet, the lowest at the
    /// foot.
    t_crest: Vec<f64>,
    t_rcrest: Vec<f64>,
    crest_z: Vec<i32>,
    foot_z: Vec<i32>,
    bbox: [f64; 4],
}

const PBIN: f64 = 10.0;

impl Patch {
    fn bin(&self, i: usize) -> usize {
        (((self.tiles[i].along - self.s0) / PBIN) as usize).min(self.bins - 1)
    }
    fn class(&self, i: usize) -> Class {
        let t = self.tiles[i].t;
        if t < 0.0 {
            Class::Fore
        } else if t <= self.t_crest[self.bin(i)] {
            Class::Forelimb
        } else {
            Class::Back
        }
    }
    fn neighbours(&self, i: usize) -> impl Iterator<Item = usize> + '_ {
        let (q, r) = (self.tiles[i].q, self.tiles[i].r);
        NB6.iter().filter_map(move |(dq, dr)| self.index.get(&(q + dq, r + dr)).copied())
    }
}

fn build_patch(range: &Range, s0: f64, s1: f64, fore: f64, back: f64) -> Patch {
    let base = Composite::base(SEED);
    let e = range.edge();
    let mut bbox = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for b in range.bins.iter().filter(|b| b.along >= s0 - BIN && b.along <= s1 + BIN) {
        bbox = [bbox[0].min(b.wx), bbox[1].min(b.wy), bbox[2].max(b.wx), bbox[3].max(b.wy)];
    }
    let pad = fore.max(back) + 600.0;
    bbox = [bbox[0] - pad, bbox[1] - pad, bbox[2] + pad, bbox[3] + pad];
    let rows: Vec<i32> = ((bbox[1] * 2.0 / 3f64.sqrt()).floor() as i32..=(bbox[3] * 2.0 / 3f64.sqrt()).ceil() as i32).collect();
    let t = Instant::now();
    let found: Vec<Vec<(i32, i32, f64, f64)>> = par_map(&rows, |&r| {
        let mut row = Vec::new();
        let q0 = (bbox[0] - r as f64 * 0.5).floor() as i32;
        let q1 = (bbox[2] - r as f64 * 0.5).ceil() as i32;
        for q in q0..=q1 {
            let (wx, wy) = hex_to_world(q, r);
            let (x, y) = warp(wx, wy, SEED);
            let along = e.along(x, y);
            if along < s0 || along >= s1 {
                continue;
            }
            if e.off_line(x, y) > fore.max(back) + world::lattice::CHAIN_SWING + 10.0 {
                continue;
            }
            let (d, side) = e.distance(x, y);
            let t = if side > 0.0 { d } else { -d };
            if t >= -fore && t <= back {
                row.push((q, r, t, along));
            }
        }
        row
    });
    let picked: Vec<(i32, i32, f64, f64)> = found.into_iter().flatten().collect();
    println!("  patch: {} tiles picked in {:.1}s; reading them through the composite...", picked.len(), t.elapsed().as_secs_f64());
    // The ladder keeps what it builds, some 16 KB a tile read, so the
    // patch is read in stretches of strike, each on a fresh composite:
    // every value is a function of its position, whatever was read before.
    let t = Instant::now();
    let chunk = env_f("CHUNK").unwrap_or(200.0);
    let mut tiles: Vec<Tile> = Vec::with_capacity(picked.len());
    let mut picked = picked;
    picked.sort_by(|a, b| ((a.3 - s0) / chunk).floor().partial_cmp(&((b.3 - s0) / chunk).floor()).unwrap().then(a.1.cmp(&b.1)).then(a.0.cmp(&b.0)));
    let mut start = 0;
    while start < picked.len() {
        let k = ((picked[start].3 - s0) / chunk).floor();
        let end = picked[start..].iter().position(|p| ((p.3 - s0) / chunk).floor() != k).map_or(picked.len(), |e| start + e);
        let c = Composite::standard(SEED);
        let part: Vec<Tile> = par_map(&picked[start..end], |&(q, r, t, along)| {
            let view = c.tile_at(q, r);
            let (cx, cy) = hex_to_world(q, r);
            let cover = view.cover;
            let z = view.elevation.round() as i32;
            Tile {
                q,
                r,
                t,
                along,
                z,
                base: 0,
                relief: range.outlines.relief(cx, cy),
                water: view.water.is_some_and(|w| w.round() as i32 > z),
                growth: (0..SITES.len()).any(|k| cover.growth(k) != Content::Empty),
                boulder: cover.boulders().next().is_some(),
                full: cover.fullness() >= 4,
            }
        });
        println!("    along {:.0}..{:.0}: {} tiles, {:.0}s so far, private {} MB at the stretch's end", s0 + k * chunk, s0 + (k + 1.0) * chunk, part.len(), t.elapsed().as_secs_f64(), private_bytes() >> 20);
        drop(c);
        tiles.extend(part);
        start = end;
    }
    println!("  read {} final tiles in {:.1}s; private {} MB", tiles.len(), t.elapsed().as_secs_f64(), private_bytes() >> 20);
    let t = Instant::now();
    let bases: Vec<i32> = par_map(&tiles, |tile| base.tile_at(tile.q, tile.r).elevation.round() as i32);
    for (tile, b) in tiles.iter_mut().zip(bases) {
        tile.base = b;
    }
    println!("  read {} base tiles in {:.1}s; private {} MB", tiles.len(), t.elapsed().as_secs_f64(), private_bytes() >> 20);
    let index: HashMap<(i32, i32), usize> = tiles.iter().enumerate().map(|(i, t)| ((t.q, t.r), i)).collect();
    let bins = ((s1 - s0) / PBIN).ceil() as usize;
    let mut t_crest = vec![f64::MAX; bins];
    let mut t_rcrest = vec![0.0; bins];
    let mut best_relief = vec![f64::MIN; bins];
    let mut crest_z = vec![i32::MIN; bins];
    let mut foot_z = vec![i32::MAX; bins];
    for tile in &tiles {
        let b = (((tile.along - s0) / PBIN) as usize).min(bins - 1);
        if tile.t >= 0.0 {
            if tile.relief > best_relief[b] {
                best_relief[b] = tile.relief;
                t_rcrest[b] = tile.t;
            }
            if tile.z > crest_z[b] || (tile.z == crest_z[b] && tile.t < t_crest[b]) {
                t_crest[b] = tile.t;
            }
            crest_z[b] = crest_z[b].max(tile.z);
        }
        if tile.t >= -20.0 && tile.t <= 5.0 {
            foot_z[b] = foot_z[b].min(tile.z);
        }
    }
    Patch { tiles, index, s0, bins, t_crest, t_rcrest, crest_z, foot_z, bbox }
}

/// Every tile a walker reaches from `seeds` inside `allowed`, stepping up
/// at most `up` levels and down at most `down`, never into a tile its
/// cover fills when `cover` holds: each reached tile's parent, the seed's
/// its own index, unreached `usize::MAX`.
fn flood(p: &Patch, seeds: &[usize], allowed: impl Fn(usize) -> bool, up: i32, down: i32, cover: bool) -> Vec<usize> {
    let mut parent = vec![usize::MAX; p.tiles.len()];
    let mut queue = VecDeque::new();
    for &s in seeds {
        if allowed(s) && !(cover && p.tiles[s].full) {
            parent[s] = s;
            queue.push_back(s);
        }
    }
    while let Some(i) = queue.pop_front() {
        let z = p.tiles[i].z;
        for j in p.neighbours(i) {
            if parent[j] != usize::MAX || !allowed(j) || (cover && p.tiles[j].full) {
                continue;
            }
            let dz = p.tiles[j].z - z;
            if dz > up || -dz > down {
                continue;
            }
            parent[j] = i;
            queue.push_back(j);
        }
    }
    parent
}

struct Reach {
    share: f64,
    forelimb_share: f64,
    /// Per bin: highest reached ground as a share of foot-to-crest.
    bin_share: Vec<f64>,
    crest_bins: Vec<bool>,
}

fn reach(p: &Patch, parent: &[usize]) -> Reach {
    let n = p.tiles.len();
    let reached = parent.iter().filter(|&&x| x != usize::MAX).count();
    let fl: Vec<usize> = (0..n).filter(|&i| p.class(i) == Class::Forelimb).collect();
    let fl_reached = fl.iter().filter(|&&i| parent[i] != usize::MAX).count();
    let mut top = vec![i32::MIN; p.bins];
    for i in 0..n {
        if parent[i] != usize::MAX && p.tiles[i].t >= 0.0 {
            let b = p.bin(i);
            top[b] = top[b].max(p.tiles[i].z);
        }
    }
    let bin_share: Vec<f64> = (0..p.bins)
        .map(|b| {
            let h = (p.crest_z[b] - p.foot_z[b]).max(1) as f64;
            if top[b] == i32::MIN { 0.0 } else { ((top[b] - p.foot_z[b]) as f64 / h).clamp(0.0, 1.0) }
        })
        .collect();
    let crest_bins = (0..p.bins).map(|b| top[b] != i32::MIN && top[b] >= p.crest_z[b] - 3).collect();
    Reach { share: reached as f64 / n as f64, forelimb_share: fl_reached as f64 / fl.len().max(1) as f64, bin_share, crest_bins }
}

fn runs(v: &[bool]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, &b) in v.iter().chain(std::iter::once(&false)).enumerate() {
        match (b, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                out.push((s, i - 1));
                start = None
            }
            _ => {}
        }
    }
    out
}

fn print_reach(p: &Patch, label: &str, r: &Reach) {
    let mut shares = r.bin_share.clone();
    shares.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mean = shares.iter().sum::<f64>() / shares.len() as f64;
    let rs = runs(&r.crest_bins);
    println!(
        "  {label:<44} reached {:5.1}% of patch, {:5.1}% of forelimb; highest reached / local foot-to-crest: mean {:.2} min {:.2} p50 {:.2} max {:.2}; crest reached in {} of {} bins, {} separate places",
        100.0 * r.share, 100.0 * r.forelimb_share, mean, shares[0], shares[shares.len() / 2], shares[shares.len() - 1],
        r.crest_bins.iter().filter(|&&b| b).count(), p.bins, rs.len()
    );
}

/// The path from a seed to tile `i`, seed first.
fn path_to(parent: &[usize], mut i: usize) -> Vec<usize> {
    let mut path = vec![i];
    while parent[i] != i {
        i = parent[i];
        path.push(i);
    }
    path.reverse();
    path
}

fn describe_path(p: &Patch, path: &[usize]) {
    let t = |i: usize| &p.tiles[i];
    let wet = path.iter().filter(|&&i| t(i).water).count();
    let carved = path.iter().filter(|&&i| t(i).base - t(i).z >= 3).count();
    let fl: Vec<usize> = path.iter().copied().filter(|&i| p.class(i) == Class::Forelimb).collect();
    let fl_carved = fl.iter().filter(|&&i| t(i).base - t(i).z >= 3).count();
    let (amin, amax) = fl.iter().fold((f64::MAX, f64::MIN), |(a, b), &i| (a.min(t(i).along), b.max(t(i).along)));
    let (tmin, tmax) = fl.iter().fold((f64::MAX, f64::MIN), |(a, b), &i| (a.min(t(i).t), b.max(t(i).t)));
    let top = *path.last().unwrap();
    let mid = path.iter().copied().find(|&i| t(i).z >= (t(path[0]).z + t(top).z) / 2).unwrap_or(top);
    let start = fl.first().copied().unwrap_or(path[0]);
    println!(
        "    path {} tiles ({} on the forelimb: across-strike t {:.0}..{:.0}, along-strike spread {:.0}); {} wet, {} carved 3+ below base ({} of the forelimb's)",
        path.len(), fl.len(), tmin, tmax, amax - amin, wet, carved, fl_carved
    );
    for (label, i) in [("foot of forelimb", start), ("half height", mid), ("top", top)] {
        let x = t(i);
        println!("      {label:<16} hex ({}, {}) z {}  base {}  t {:.0}  along {:.0}{}", x.q, x.r, x.z, x.base, x.t, x.along, if x.water { "  water" } else { "" });
    }
    // How the path climbs: steps by the kind of ground they stand on.
    let mut ups = (0, 0, 0);
    for w in path.windows(2) {
        if t(w[1]).z > t(w[0]).z {
            if t(w[1]).water || t(w[0]).water {
                ups.0 += 1
            } else if t(w[1]).base - t(w[1]).z >= 3 {
                ups.1 += 1
            } else {
                ups.2 += 1
            }
        }
    }
    println!("      climbing steps: {} in water, {} on carved ground, {} on uncut ground", ups.0, ups.1, ups.2);
}

fn census(p: &Patch) {
    let n = p.tiles.len();
    let mut pairs: HashMap<Class, (usize, usize, usize, f64)> = HashMap::new();
    for i in 0..n {
        let (q, r, z) = (p.tiles[i].q, p.tiles[i].r, p.tiles[i].z);
        for (dq, dr) in [(1, 0), (0, 1), (1, -1)] {
            if let Some(&j) = p.index.get(&(q + dq, r + dr)) {
                let d = (p.tiles[j].z - z).abs();
                let e = pairs.entry(p.class(i)).or_default();
                e.0 += 1;
                if d > 1 { e.1 += 1 }
                if d > 2 { e.2 += 1 }
                e.3 += d as f64;
            }
        }
    }
    println!("\n  {:<10} {:>8} {:>10} {:>10} {:>10} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9}", "class", "tiles", "pairs", ">1 lv", ">2 lv", "mean|dz|", "water", "carved3+", "tree/brsh", "boulder", "full");
    let mut all = (0usize, 0usize, 0usize, 0f64);
    for class in [Class::Fore, Class::Forelimb, Class::Back] {
        let ids: Vec<usize> = (0..n).filter(|&i| p.class(i) == class).collect();
        let pc = |f: &dyn Fn(&Tile) -> bool| 100.0 * ids.iter().filter(|&&i| f(&p.tiles[i])).count() as f64 / ids.len().max(1) as f64;
        let e = pairs.get(&class).copied().unwrap_or_default();
        all = (all.0 + e.0, all.1 + e.1, all.2 + e.2, all.3 + e.3);
        println!(
            "  {:<10} {:>8} {:>10} {:>9.1}% {:>9.1}% {:>9.2} {:>8.1}% {:>8.1}% {:>8.1}% {:>8.1}% {:>8.1}%",
            format!("{class:?}"), ids.len(), e.0, 100.0 * e.1 as f64 / e.0.max(1) as f64, 100.0 * e.2 as f64 / e.0.max(1) as f64, e.3 / e.0.max(1) as f64,
            pc(&|t| t.water), pc(&|t| t.base - t.z >= 3), pc(&|t| t.growth), pc(&|t| t.boulder), pc(&|t| t.full)
        );
    }
    println!("  {:<10} {:>8} {:>10} {:>9.1}% {:>9.1}% {:>9.2}", "patch", n, all.0, 100.0 * all.1 as f64 / all.0 as f64, 100.0 * all.2 as f64 / all.0 as f64, all.3 / all.0 as f64);
    // The forelimb in bands across strike: where the steep steps sit.
    println!("\n  forelimb by share of the way from front to crest:");
    println!("  {:>8} {:>8} {:>9} {:>9} {:>9} {:>9} {:>9}", "band", "tiles", ">1 lv", "mean|dz|", "water", "carved3+", "tree/brsh");
    for k in 0..5 {
        let (lo, hi) = (k as f64 / 5.0, (k + 1) as f64 / 5.0);
        let ids: Vec<usize> = (0..n)
            .filter(|&i| p.class(i) == Class::Forelimb)
            .filter(|&i| {
                let f = p.tiles[i].t / p.t_crest[p.bin(i)].max(1.0);
                f >= lo && f < hi
            })
            .collect();
        let (mut pr, mut over, mut sum) = (0usize, 0usize, 0f64);
        for &i in &ids {
            for j in p.neighbours(i) {
                let d = (p.tiles[j].z - p.tiles[i].z).abs();
                pr += 1;
                if d > 1 { over += 1 }
                sum += d as f64;
            }
        }
        let pc = |f: &dyn Fn(&Tile) -> bool| 100.0 * ids.iter().filter(|&&i| f(&p.tiles[i])).count() as f64 / ids.len().max(1) as f64;
        println!("  {:>3.1}-{:<3.1} {:>8} {:>8.1}% {:>9.2} {:>8.1}% {:>8.1}% {:>8.1}%", lo, hi, ids.len(), 100.0 * over as f64 / pr.max(1) as f64, sum / pr.max(1) as f64, pc(&|t| t.water), pc(&|t| t.base - t.z >= 3), pc(&|t| t.growth));
    }
}

#[test]
#[ignore]
fn forelimb_patch() {
    let t0 = Instant::now();
    let range = locate();
    let (cul, sad) = (range.culmination(), range.saddle());
    let (s0, s1) = match std::env::var("PATCH").ok() {
        Some(v) => {
            let v: Vec<f64> = v.split(',').map(|s| s.trim().parse().unwrap()).collect();
            (v[0], v[1])
        }
        None => {
            let (a, b) = (cul.along.min(sad.along) - 100.0, cul.along.max(sad.along) + 100.0);
            let mid = 0.5 * (a + b);
            let half = (0.5 * (b - a)).min(1000.0);
            (mid - half, mid + half)
        }
    };
    let fore = env_f("FORE").unwrap_or(150.0);
    let back = env_f("BACK").unwrap_or(700.0);
    println!("\n=== forelimb patch: along {s0:.0}..{s1:.0} ({:.0} of strike), from {fore:.0} out past the front to {back:.0} in ===", s1 - s0);
    println!("  culmination along {:.0} (relief {:.0}), saddle along {:.0} (relief {:.0})", cul.along, cul.crest, sad.along, sad.crest);
    let p = build_patch(&range, s0, s1, fore, back);
    let n = p.tiles.len();

    // Along strike: the local crest and foot.
    println!("\n  along strike (every 100): the ranges' own crest's t, the final crest's t and z, the foot's z");
    for b in (0..p.bins).step_by(10) {
        println!("    along {:6.0}: relief crest at t {:4.0}; final crest z {:4} at t {:4.0}; foot z {:4}", s0 + b as f64 * PBIN, p.t_rcrest[b], p.crest_z[b], p.t_crest[b], p.foot_z[b]);
    }
    census(&p);
    if let Some(&i) = p.index.get(&SPAWN) {
        let x = &p.tiles[i];
        println!("
  spawn {SPAWN:?} lies in the patch: z {}  base {}  t {:.0}  along {:.0}  class {:?}  water {}  growth {}", x.z, x.base, x.t, x.along, p.class(i), x.water, x.growth);
    }

    // Reachability.
    println!("\n  reachability (seeds: every tile within 30 of the patch's outer edge on that side):");
    let front_seeds: Vec<usize> = (0..n).filter(|&i| p.tiles[i].t <= -fore + 30.0).collect();
    let back_seeds: Vec<usize> = (0..n).filter(|&i| p.tiles[i].t >= back - 30.0).collect();
    let all = |_: usize| true;
    let front_only = |i: usize| p.tiles[i].t <= p.t_crest[p.bin(i)] + 3.0;
    let a = flood(&p, &front_seeds, all, 1, i32::MAX, true);
    let af = flood(&p, &front_seeds, front_only, 1, i32::MAX, true);
    let b = flood(&p, &front_seeds, all, 1, 1, true);
    let cj = flood(&p, &front_seeds, all, 2, i32::MAX, true);
    let cjf = flood(&p, &front_seeds, front_only, 2, i32::MAX, true);
    let d = flood(&p, &front_seeds, all, 1, i32::MAX, false);
    let bk = flood(&p, &back_seeds, all, 1, i32::MAX, true);
    let bkj = flood(&p, &back_seeds, all, 2, i32::MAX, true);
    let ra = reach(&p, &a);
    let raf = reach(&p, &af);
    print_reach(&p, "front, up<=1 down any, cover blocks", &ra);
    print_reach(&p, "front, same, forelimb only (no backlimb)", &raf);
    print_reach(&p, "front, |dz|<=1 both ways", &reach(&p, &b));
    print_reach(&p, "front, up<=2 (jump), down any", &reach(&p, &cj));
    print_reach(&p, "front, up<=2 (jump), forelimb only", &reach(&p, &cjf));
    print_reach(&p, "front, up<=1 down any, cover ignored", &reach(&p, &d));
    print_reach(&p, "back, up<=1 down any, cover blocks", &reach(&p, &bk));
    print_reach(&p, "back, up<=2 (jump), down any", &reach(&p, &bkj));
    println!("  front seeds {}, back seeds {}", front_seeds.len(), back_seeds.len());

    println!("\n  per 100 along: highest share reached from the front (up<=1, forelimb only / up<=2 forelimb only / from the back up<=1):");
    let rcj = reach(&p, &cjf);
    let rbk = reach(&p, &bk);
    for b0 in (0..p.bins).step_by(10) {
        let m = |v: &[f64]| v[b0..(b0 + 10).min(p.bins)].iter().cloned().fold(0.0, f64::max);
        println!("    along {:6.0}: {:.2} / {:.2} / {:.2}", s0 + b0 as f64 * PBIN, m(&raf.bin_share), m(&rcj.bin_share), m(&rbk.bin_share));
    }

    // Ways up: each run of crest bins reached from the front alone.
    let mut ways: Vec<Vec<usize>> = Vec::new();
    for (label, parent, r) in [("up<=1", &af, &raf), ("up<=2 (jump)", &cjf, &rcj)] {
        let rs = runs(&r.crest_bins);
        println!("\n  ways up the forelimb from the front, {label}: {} places reach the crest", rs.len());
        for (k, &(b0, b1)) in rs.iter().enumerate().take(8) {
            let top = (0..n)
                .filter(|&i| parent[i] != usize::MAX && (b0..=b1).contains(&p.bin(i)))
                .max_by_key(|&i| p.tiles[i].z)
                .unwrap();
            println!("  [{k}] crest bins along {:.0}..{:.0}:", s0 + b0 as f64 * PBIN, s0 + (b1 + 1) as f64 * PBIN);
            let path = path_to(parent, top);
            describe_path(&p, &path);
            if label == "up<=1" && ways.len() < 3 {
                ways.push(path);
            }
        }
        if rs.is_empty() {
            // The best partial climbs: the highest share of the local crest.
            let mut best: Vec<(f64, usize)> = (0..n)
                .filter(|&i| parent[i] != usize::MAX && p.class(i) == Class::Forelimb)
                .map(|i| {
                    let b = p.bin(i);
                    ((p.tiles[i].z - p.foot_z[b]) as f64 / (p.crest_z[b] - p.foot_z[b]).max(1) as f64, i)
                })
                .collect();
            best.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
            let mut shown: Vec<f64> = Vec::new();
            for &(share, i) in &best {
                if shown.iter().any(|&a| (a - p.tiles[i].along).abs() < 200.0) {
                    continue;
                }
                shown.push(p.tiles[i].along);
                println!("  best partial climb: {:.2} of foot-to-crest", share);
                let path = path_to(parent, i);
                describe_path(&p, &path);
                if label == "up<=1" && ways.len() < 3 {
                    ways.push(path);
                }
                if shown.len() >= 3 {
                    break;
                }
            }
        }
    }

    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../proofs/world/bare-ranges");
    std::fs::create_dir_all(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    render(&p, &a, &af, &ways, &dir);
    println!("\n  images in {}", dir.display());
    println!("  {:.1}s total", t0.elapsed().as_secs_f64());
}

// ── Pictures ────────────────────────────────────────────────────────────────

fn render(p: &Patch, reach_all: &[usize], reach_front: &[usize], ways: &[Vec<usize>], dir: &Path) {
    // Crop the box to the patch's own tiles.
    let mut bb = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for t in &p.tiles {
        let (x, y) = hex_to_world(t.q, t.r);
        bb = [bb[0].min(x), bb[1].min(y), bb[2].max(x), bb[3].max(y)];
    }
    let _ = p.bbox;
    let (w, h) = ((bb[2] - bb[0]).ceil() as usize + 1, (bb[3] - bb[1]).ceil() as usize + 1);
    let n = p.tiles.len();
    // Shade from the six neighbours' heights, 1 level = RISE tiles high,
    // light from the north-west (+y is north, up the image) at 45°.
    let offs: Vec<(f64, f64)> = NB6.iter().map(|&(dq, dr)| hex_to_world(dq, dr)).collect();
    let (lx, ly, lz) = { let v: (f64, f64, f64) = (-1.0, 1.0, 2f64.sqrt()); let l = (v.0 * v.0 + v.1 * v.1 + v.2 * v.2).sqrt(); (v.0 / l, v.1 / l, v.2 / l) };
    let shade: Vec<f64> = (0..n)
        .map(|i| {
            let (q, r, z) = (p.tiles[i].q, p.tiles[i].r, p.tiles[i].z);
            let (mut gx, mut gy) = (0.0, 0.0);
            for (k, (dq, dr)) in NB6.iter().enumerate() {
                let zn = p.index.get(&(q + dq, r + dr)).map_or(z, |&j| p.tiles[j].z);
                gx += (zn - z) as f64 * offs[k].0 / 3.0;
                gy += (zn - z) as f64 * offs[k].1 / 3.0;
            }
            let (nx, ny, nz) = (-gx * RISE, -gy * RISE, 1.0);
            let l = (nx * nx + ny * ny + nz * nz).sqrt();
            ((nx * lx + ny * ly + nz * lz) / l).max(0.0)
        })
        .collect();
    let blocked: Vec<bool> = (0..n).map(|i| p.neighbours(i).any(|j| p.tiles[j].z - p.tiles[i].z > 1)).collect();
    let mut on_way = vec![false; n];
    for path in ways {
        for &i in path {
            on_way[i] = true;
        }
    }
    let crest: Vec<bool> = (0..n).map(|i| p.tiles[i].t >= 0.0 && (p.tiles[i].t - p.t_crest[p.bin(i)]).abs() < 1.5).collect();
    let mut hill = vec![30u8; w * h * 3];
    let mut walk = vec![30u8; w * h * 3];
    for py in 0..h {
        for px in 0..w {
            let (x, y) = (bb[0] + px as f64, bb[3] - py as f64);
            let (q, r) = world_to_hex(x, y);
            let Some(&i) = p.index.get(&(q, r)) else { continue };
            let s = 40.0 + 200.0 * shade[i];
            let o = (py * w + px) * 3;
            let t = &p.tiles[i];
            let mut c = [s, s, s];
            if t.water {
                c = [0.3 * s, 0.5 * s, 255.0f64.min(s + 60.0)];
            } else if blocked[i] {
                c = [255.0f64.min(s + 70.0), 0.55 * s, 0.5 * s];
            }
            if t.t.abs() < 1.0 {
                c = [255.0, 255.0, 0.0];
            }
            for k in 0..3 {
                hill[o + k] = c[k].clamp(0.0, 255.0) as u8;
            }
            let mut c = [0.8 * s, 0.8 * s, 0.8 * s];
            if reach_front[i] != usize::MAX {
                c = [0.25 * s, 0.6 * s + 70.0, 0.25 * s];
            } else if reach_all[i] != usize::MAX {
                c = [0.3 * s + 40.0, 0.45 * s + 40.0, 0.9 * s];
            }
            if t.water {
                c = [0.2 * s, 0.4 * s, 255.0f64.min(s + 60.0)];
            }
            if p.tiles[i].t >= 0.0 && (p.tiles[i].t - p.t_rcrest[p.bin(i)]).abs() < 1.5 {
                c = [255.0, 120.0, 0.0];
            }
            if crest[i] {
                c = [255.0, 230.0, 0.0];
            }
            if t.t.abs() < 1.0 {
                c = [255.0, 255.0, 255.0];
            }
            if on_way[i] {
                c = [255.0, 0.0, 255.0];
            }
            for k in 0..3 {
                walk[o + k] = c[k].clamp(0.0, 255.0) as u8;
            }
        }
    }
    write_png(&dir.join("forelimb-hillshade.png"), w, h, &hill);
    write_png(&dir.join("forelimb-reach.png"), w, h, &walk);
    let (sw, sh, small) = halve(w, h, &hill);
    write_png(&dir.join("forelimb-hillshade-half.png"), sw, sh, &small);
    let (sw, sh, small) = halve(w, h, &walk);
    write_png(&dir.join("forelimb-reach-half.png"), sw, sh, &small);
    println!("  images {w}x{h} px, one tile per px, +y (north) up; bbox world x {:.0}..{:.0}, y {:.0}..{:.0}", bb[0], bb[2], bb[1], bb[3]);
}

fn halve(w: usize, h: usize, rgb: &[u8]) -> (usize, usize, Vec<u8>) {
    let (sw, sh) = (w / 2, h / 2);
    let mut out = vec![0u8; sw * sh * 3];
    for y in 0..sh {
        for x in 0..sw {
            for k in 0..3 {
                let s: u32 = [(0, 0), (1, 0), (0, 1), (1, 1)].iter().map(|(a, b)| rgb[((2 * y + b) * w + 2 * x + a) * 3 + k] as u32).sum();
                out[(y * sw + x) * 3 + k] = (s / 4) as u8;
            }
        }
    }
    (sw, sh, out)
}

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (n, e) in table.iter_mut().enumerate() {
        let mut c = n as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *e = c;
    }
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c = table[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

/// An RGB PNG, its deflate stream stored uncompressed.
fn write_png(path: &Path, w: usize, h: usize, rgb: &[u8]) {
    let mut raw = Vec::with_capacity((w * 3 + 1) * h);
    for y in 0..h {
        raw.push(0);
        raw.extend_from_slice(&rgb[y * w * 3..(y + 1) * w * 3]);
    }
    let mut z = vec![0x78, 0x01];
    let mut i = 0;
    loop {
        let n = (raw.len() - i).min(65535);
        let last = i + n >= raw.len();
        z.push(last as u8);
        z.extend_from_slice(&(n as u16).to_le_bytes());
        z.extend_from_slice(&(!(n as u16)).to_le_bytes());
        z.extend_from_slice(&raw[i..i + n]);
        i += n;
        if last {
            break;
        }
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &raw {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut chunk = |kind: &[u8], data: &[u8]| {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend_from_slice(&body);
        out.extend_from_slice(&crc32(&body).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &z);
    chunk(b"IEND", &[]);
    std::fs::write(path, out).unwrap();
}

// ── The coast ───────────────────────────────────────────────────────────────

/// A front within 15000 tiles of the spawn whose foreland is sea before the
/// ladder: the nearest few, then a section of the final ground across the
/// nearest.
#[test]
#[ignore]
fn convergent_coast() {
    let t = Instant::now();
    let (sx, sy) = hex_to_world(SPAWN.0, SPAWN.1);
    let outlines = Outlines::in_box(sx, sy, 16_000.0, SEED);
    let base = Composite::base(SEED);
    let mut fronts: Vec<(f64, f64, f64, PlateId, usize)> = Vec::new();
    let step = 40.0;
    let mut i = -15_000.0;
    while i <= 15_000.0 {
        let mut j = -15_000.0;
        while j <= 15_000.0 {
            let (wx, wy) = (sx + i, sy + j);
            if let Some(at) = outlines.at(wx, wy) {
                for (k, (e, d)) in at.plate.edges.iter().zip(&at.distances).enumerate() {
                    if e.faces.is_some() && *d < step * 0.6 && outlines.relief(wx, wy) >= 0.0 {
                        fronts.push((i.hypot(j), wx, wy, at.plate.id, k));
                    }
                }
            }
            j += step;
        }
        i += step;
    }
    println!("\n=== convergent coasts within 15000 of spawn: {} front points on a {step}-tile grid ({:.1}s) ===", fronts.len(), t.elapsed().as_secs_f64());
    fronts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let mut coasts: Vec<(f64, f64, f64, PlateId, usize, (f64, f64))> = Vec::new();
    for &(dist, wx, wy, plate, k) in &fronts {
        if coasts.iter().any(|c| (c.1 - wx).hypot(c.2 - wy) < 1500.0) || coasts.len() >= 12 {
            continue;
        }
        let e = &outlines.plate(plate).unwrap().edges[k];
        let dd = |x: f64, y: f64| {
            let (u, v) = warp(x, y, SEED);
            let (d, side) = e.distance(u, v);
            if side > 0.0 { d } else { -d }
        };
        let (gx, gy) = (dd(wx + 4.0, wy) - dd(wx - 4.0, wy), dd(wx, wy + 4.0) - dd(wx, wy - 4.0));
        let l = gx.hypot(gy);
        let (nx, ny) = (gx / l, gy / l);
        let sea = [60.0, 150.0].iter().all(|&o| {
            let (q, r) = world_to_hex(wx - o * nx, wy - o * ny);
            base.tile_at(q, r).elevation < 0.0
        });
        if sea {
            coasts.push((dist, wx, wy, plate, k, (nx, ny)));
        }
    }
    println!("  fronts whose foreland is sea 60 and 150 tiles out (before the ladder), nearest first:");
    for c in &coasts {
        let (q, r) = world_to_hex(c.1, c.2);
        let e = &outlines.plate(c.3).unwrap().edges[c.4];
        println!("    {:.0} tiles from spawn: world ({:.0}, {:.0}) hex ({q}, {r}), plate {:?} against {:?}, converge {:.2}, into-plate normal ({:.2}, {:.2})", c.0, c.1, c.2, c.3, e.neighbour, e.converge, c.5 .0, c.5 .1);
    }
    let skip = env_f("COAST").unwrap_or(0.0) as usize;
    let strong = coasts.iter().filter(|c| outlines.plate(c.3).unwrap().edges[c.4].converge >= 0.25).nth(skip);
    let Some(&(_, wx, wy, plate, k, (nx, ny))) = strong else {
        println!("  none found in the area searched");
        return;
    };
    let c = Composite::standard(SEED);
    let e = &outlines.plate(plate).unwrap().edges[k];
    println!("\n  section of the final ground across the nearest, from 200 out to 500 in:");
    println!("  {:>6} {:>8} {:>8} {:>6} {:>8} {:>14}", "d", "relief", "base", "final", "water", "hex");
    let mut prev: Option<(i32, i32)> = None;
    let mut s = -200.0;
    let mut steps = (0, 0);
    let mut last_z: Option<i32> = None;
    let mut next_print = -200.0;
    while s <= 500.0 {
        let (x, y) = (wx + s * nx, wy + s * ny);
        let (q, r) = world_to_hex(x, y);
        if prev != Some((q, r)) {
            prev = Some((q, r));
            let (cx, cy) = hex_to_world(q, r);
            let view = c.tile_at(q, r);
            let z = view.elevation.round() as i32;
            if let Some(lz) = last_z {
                steps.0 += 1;
                if z - lz > 1 { steps.1 += 1 }
            }
            last_z = Some(z);
            let (u, v) = warp(cx, cy, SEED);
            let (d, side) = e.distance(u, v);
            let d = if side > 0.0 { d } else { -d };
            if s >= next_print {
                next_print = s + if s.abs() < 40.0 { 2.0 } else { 10.0 };
                let water = c.water_at(q, r).map(|w| w.to_string()).unwrap_or_default();
                println!("  {:6.0} {:8.1} {:8.1} {:6} {:>8} ({q},{r})", d, outlines.relief(cx, cy), base.tile_at(q, r).elevation, z, water);
            }
        }
        s += 0.5;
    }
    println!("  steps up more than one level along the line: {} of {}", steps.1, steps.0);
    println!("  {:.1}s", t.elapsed().as_secs_f64());
}
