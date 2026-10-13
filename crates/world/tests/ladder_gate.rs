//! The ladder over the real ground around the haven: every level requested
//! over a region, then identity and seams checked, and each level's arrays
//! written for the measures and renders. Thrust's relief is averaged over
//! each cell's descendants `LADDER_DEPTH` levels down (default 2), the rest
//! of the stack read as fields. `LADDER_EXACT=1` checks identity by position
//! and seams between requests made apart; `LADDER_WATER=0` runs it dry.
//!
//! Run: `LADDER_OUT=<dir> cargo test --release -p world --test ladder_gate -- --ignored --nocapture ladder_gate`
//!
//! `ladder_averaging` measures what averaging thrust over a cell costs and
//! how far each depth lies from the cell's whole average.

use std::io::Write;
use std::time::Instant;

use world::events::{lithology, motion, plates, thickening, thrusting, tilt, Composite};
use world::ladder::ground::Layered;
use world::ladder::{Ground, Ladder, Level, Params};
use world::WORLD_SEED;

/// The haven's tile, as `common_bevy::haven::HAVEN_LOCATION` holds it.
const HAVEN: (i32, i32) = (107289, -7677);

/// Each level's width and how far its cells reach from the haven, in tiles.
const LEVELS: [(i64, i64); 7] = [(729, 142_884), (243, 57_591), (81, 19_197), (27, 6_399), (9, 2_133), (3, 832), (1, 481)];

/// Each level's request: a rhombus of cells around the haven, this many
/// tiles from it either way.
fn region(w: i64) -> i64 {
    match w {
        1 | 3 => 364,
        9 => 1080,
        27 => 3240,
        81 => 9720,
        _ => 29160,
    }
}

fn stack(seed: u64) -> Composite {
    let mut c = Composite::new(seed);
    c.add_event(Box::new(plates::PlateEvent::new()));
    c.add_event(Box::new(tilt::TiltEvent::new()));
    c.add_event(Box::new(motion::MotionEvent::new()));
    c.add_event(Box::new(thrusting::ThrustingEvent::new()));
    c.add_event(Box::new(thickening::ThickeningEvent::new()));
    c.add_event(Box::new(lithology::LithologyEvent::new()));
    c
}

/// Each level's cells: by default as [`LEVELS`] reaches; `LADDER_SMALL`
/// gives every level below the top the same reach in tiles, for a test.
fn extents() -> Vec<(i64, i64, i64)> {
    let small: Option<i64> = std::env::var("LADDER_SMALL").ok().and_then(|s| s.parse().ok());
    LEVELS.iter().map(|&(w, reach)| {
        let reach = match small { Some(s) if w < 729 => s, _ => reach };
        let half = reach / w + 2;
        (w, -half, half + 1)
    }).collect()
}

fn region_cells(w: i64) -> Vec<(i64, i64)> {
    let h = region(w) / w;
    (-h..=h).flat_map(|i| (-h..=h).map(move |j| (i, j))).collect()
}

/// A small deterministic generator for the cells the exact checks pick.
struct Draw(u64);

impl Draw {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % (hi - lo) as u64) as i64
    }
}

fn same(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
}

/// Whether all six of a cell's corners are drawn: its own two and the four
/// its neighbours own, where those exist.
fn drawn(lv: &Level, i: i64, j: i64) -> bool {
    [(i, j), (i - 1, j + 1), (i - 1, j), (i, j - 1)].iter().all(|&(a, b)| lv.idx(a, b).is_none_or(|c| !lv.exists[c] || lv.corners[c]))
}

/// What two ladders hold differently at a cell of level k: its height,
/// corners, and every vertex's routing, each named with its vertex.
fn differs(a: &Level, b: &Level, i: i64, j: i64) -> Vec<String> {
    let (Some(ca), Some(cb)) = (a.idx(i, j), b.idx(i, j)) else { return vec!["held".into()] };
    let (na, nb) = (a.n * a.n, b.n * b.n);
    let mut bad = Vec::new();
    if a.built[ca] && b.built[cb] && !same(a.z[ca], b.z[cb]) {
        bad.push(format!("z {} / {}", a.z[ca], b.z[cb]));
    }
    if drawn(a, i, j) && drawn(b, i, j) && !(0..6).all(|s| same(a.cc[ca][s], b.cc[cb][s])) {
        bad.push(format!("cc {:?} / {:?}", a.cc[ca], b.cc[cb]));
    }
    for t in 0..3 {
        let (va, vb) = (t * na + ca, t * nb + cb);
        if a.corners[ca] && b.corners[cb] && !same(a.elev[va], b.elev[vb]) {
            bad.push(format!("elev{t} {} / {}", a.elev[va], b.elev[vb]));
        }
        if !same(a.gelev[va], b.gelev[vb]) {
            bad.push(format!("gelev{t} {} / {}", a.gelev[va], b.gelev[vb]));
        }
        if a.trough[va] != b.trough[vb] {
            bad.push(format!("trough{t} {} / {}", a.trough[va], b.trough[vb]));
        }
        if !same(a.acc[va], b.acc[vb]) {
            bad.push(format!("acc{t} {} / {}", a.acc[va], b.acc[vb]));
        }
        if !same(a.floor[va], b.floor[vb]) {
            bad.push(format!("floor{t} {} / {}", a.floor[va], b.floor[vb]));
        }
        // A receiver is a vertex index in each ladder's own arrays.
        let r = |lv: &Level, v: i64| (v >= 0).then(|| lv.vertex(v as usize));
        if r(a, a.rec[va]) != r(b, b.rec[vb]) {
            bad.push(format!("rec{t} {:?} / {:?}", r(a, a.rec[va]), r(b, b.rec[vb])));
        }
    }
    bad
}

fn alike(a: &Level, b: &Level, i: i64, j: i64) -> bool {
    differs(a, b, i, j).is_empty()
}

fn exact<G: Ground>(ground: &G, whole: &Ladder<G>, p: Params, out: &mut String) {
    let ext = extents();
    let mut draw = Draw(5);
    for k in 1..ext.len() {
        let w = ext[k].0;
        let h = region(w) / w;
        let (mut identical, mut seams, mut cells) = (true, 0u64, 0u64);
        let started = Instant::now();
        for _ in 0..3 {
            let c = (draw.range(-h / 2, h / 2 + 1), draw.range(-h / 2, h / 2 + 1));
            let mut one = Ladder::new(ground, &ext, p);
            one.request(k, &[c]);
            let mut block: Vec<(i64, i64)> = (-4..=4).flat_map(|a| (-4..=4).map(move |b| (c.0 + a, c.1 + b))).collect();
            for x in (1..block.len()).rev() {
                let y = draw.range(0, x as i64 + 1) as usize;
                block.swap(x, y);
            }
            let mut apart = Ladder::new(ground, &ext, p);
            for &cell in &block {
                apart.request(k, &[cell]);
            }
            let wl = &whole.levels[k];
            identical &= alike(&one.levels[k], wl, c.0, c.1) && alike(&apart.levels[k], wl, c.0, c.1);
            // Every cell either routed agrees with the whole run.
            for m in [&one, &apart] {
                let lv = &m.levels[k];
                for cc in 0..lv.n * lv.n {
                    if lv.routed[cc] {
                        let (i, j) = lv.ij(cc);
                        cells += 1;
                        let bad = differs(lv, wl, i, j);
                        if !bad.is_empty() {
                            seams += 1;
                            out.push_str(&format!("   seam w={w} cell ({i}, {j}): {}\n", bad.join("; ")));
                        }
                    }
                }
            }
        }
        let line = format!("exact w={w} identical={identical} seam_disagreements={seams} of {cells} cells ({:.1}s)\n", started.elapsed().as_secs_f64());
        print!("{line}");
        out.push_str(&line);
    }
}

fn dump_f64(path: &str, v: impl Iterator<Item = f64>) {
    let mut f = std::io::BufWriter::new(std::fs::File::create(path).expect("a dump"));
    for x in v {
        f.write_all(&x.to_le_bytes()).expect("written");
    }
}

fn dump_i64(path: &str, v: impl Iterator<Item = i64>) {
    let mut f = std::io::BufWriter::new(std::fs::File::create(path).expect("a dump"));
    for x in v {
        f.write_all(&x.to_le_bytes()).expect("written");
    }
}

fn dump_bool(path: &str, v: &[bool]) {
    std::fs::write(path, v.iter().map(|&b| b as u8).collect::<Vec<u8>>()).expect("a dump");
}

fn dump(dir: &str, lv: &Level) {
    let p = |name: &str| format!("{dir}/{}_{name}.bin", lv.w);
    for (name, v) in [
        ("z", &lv.z),
        ("ground", &lv.ground),
        ("wpred", &lv.wpred),
        ("cut", &lv.cut),
        ("wshift", &lv.wshift),
        ("wfull", &lv.wfull),
        ("wpar", &lv.wpar),
        ("wdist", &lv.wdist),
        ("age", &lv.age),
        ("uelev", &lv.uelev),
        ("base", &lv.base),
        ("line_base", &lv.line_base),
        ("p", &lv.p),
        ("erod", &lv.erod),
        ("elev", &lv.elev),
        ("psurf", &lv.psurf),
        ("gelev", &lv.gelev),
        ("acc", &lv.acc),
        ("floor", &lv.floor),
        ("line_acc", &lv.line_acc),
        ("line_fl", &lv.line_fl),
        ("line_dev", &lv.line_dev),
        ("line_pin", &lv.line_pin),
    ] {
        dump_f64(&p(name), v.iter().copied());
    }
    for (l, layer) in lv.layers.iter().enumerate() {
        dump_f64(&p(&format!("layer{l}")), layer.iter().copied());
    }
    dump_f64(&p("cc"), lv.cc.iter().flat_map(|c| c.iter().copied()));
    for (name, v) in [("rec", &lv.rec), ("line_rec", &lv.line_rec), ("line_corr", &lv.line_corr), ("born", &lv.born), ("line_born", &lv.line_born), ("wborn", &lv.wborn), ("wgov", &lv.wgov)] {
        dump_i64(&p(name), v.iter().copied());
    }
    for (name, v) in [
        ("exists", &lv.exists),
        ("trough", &lv.trough),
        ("line_on", &lv.line_on),
        ("line_wall", &lv.line_wall),
        ("line_hollow", &lv.line_hollow),
        ("routed", &lv.routed),
        ("built", &lv.built),
        ("lined", &lv.lined),
        ("wfade", &lv.wfade),
    ] {
        dump_bool(&p(name), v);
    }
}

fn run<G: Ground>(ground: &G, p: Params, dir: &str, log: &mut String) {
    let ext = extents();
    let started = Instant::now();
    let mut ladder = Ladder::new(ground, &ext, p);
    log.push_str(&format!("top {:.1}s\n", started.elapsed().as_secs_f64()));
    let only: Option<i64> = std::env::var("LADDER_SMALL_REGION").ok().and_then(|s| s.parse().ok());
    for k in (1..ext.len()).rev() {
        let t = Instant::now();
        let cells = match only {
            Some(r) if k + 1 == ext.len() => (-r..=r).flat_map(|i| (-r..=r).map(move |j| (i, j))).collect(),
            Some(_) => continue,
            None => region_cells(ext[k].0),
        };
        ladder.request(k, &cells);
        let line = format!("built w={} {:.1}s\n", ext[k].0, t.elapsed().as_secs_f64());
        print!("{line}");
        log.push_str(&line);
    }
    log.push_str(&format!("build_seconds {:.1}\n", started.elapsed().as_secs_f64()));
    for (k, lv) in ladder.levels.iter().enumerate() {
        log.push_str(&format!(
            "stats w={} lines={:?} route={:?} cost={:?}\n",
            lv.w, ladder.line_stats[k], ladder.route_stats[k], lv.cost
        ));
    }
    if std::env::var("LADDER_EXACT").is_ok_and(|v| v == "1") {
        exact(ground, &ladder, p, log);
    }
    for lv in &ladder.levels {
        dump(dir, lv);
    }
    let mut meta = String::from("w lo n layers\n");
    for lv in &ladder.levels {
        meta.push_str(&format!("{} {} {} {}\n", lv.w, lv.lo, lv.n, lv.layers.len()));
    }
    std::fs::write(format!("{dir}/levels.txt"), meta).expect("the levels");
}

/// How far either way of the haven the outlines are built over, in tiles.
fn outline_half() -> f64 {
    (LEVELS[0].1 + 3 * LEVELS[0].0) as f64
}

#[test]
#[ignore]
fn ladder_gate() {
    let dir = std::env::var("LADDER_OUT").expect("LADDER_OUT names the directory to write to");
    let depth: u32 = std::env::var("LADDER_DEPTH").ok().and_then(|d| d.parse().ok()).unwrap_or(2);
    std::fs::create_dir_all(&dir).expect("the output directory");
    let c = stack(WORLD_SEED);
    let water = std::env::var("LADDER_WATER").map_or(true, |v| v != "0");
    // Where drainage's cut starts and where it is full, in tiles.
    let (cut_from, cut_full) = match std::env::var("LADDER_CUT").as_deref() {
        Ok("B") => (13_122.0, 1_102_500.0),
        Ok("C") => (1_102_500.0, 13_230_000.0),
        _ => (1_500.0, 40_000.0),
    };
    let mut log = format!("ground layered, thrust averaged {depth} levels down, water {water}
");
    let g = Layered::new(&c, HAVEN, outline_half(), WORLD_SEED, depth);
    // How far water reads, in cells: the deform cell's ring unless a test
    // widens it.
    let ring = std::env::var("LADDER_RING").ok().and_then(|r| r.parse().ok()).unwrap_or(world::ladder::RING);
    log.push_str(&format!("cut from {cut_from} full at {cut_full}, ring {ring}
"));
    run(&g, Params { water, cut_from, cut_full, ring, ..Params::default() }, &dir, &mut log);
    print!("{log}");
    std::fs::write(format!("{dir}/log.txt"), log).expect("the log");
}

/// For cells drawn across each level's region: thrust's relief averaged at
/// each depth, against the whole cell's average (its every tile, or seven
/// levels down at the top), and what each depth costs a cell.
#[test]
#[ignore]
fn ladder_averaging() {
    let c = stack(WORLD_SEED);
    let g = Layered::new(&c, HAVEN, outline_half(), WORLD_SEED, 0);
    let mut draw = Draw(11);
    let mut out = String::from("w depth samples err_p50 err_p99 err_max us_per_cell (levels; whole = every tile)
");
    for &(w, reach) in &LEVELS {
        let full = w.ilog(3);
        let h = (region(w).min(reach)) / w;
        // Cells on the ranges, where the average differs from a point.
        let mut cells = Vec::new();
        while cells.len() < 24 {
            let cell = (draw.range(-h, h + 1), draw.range(-h, h + 1));
            if g.thrust_mean(w, cell.0, cell.1, 1) > 1.0 {
                cells.push(cell);
            }
            if cells.is_empty() && draw.next() % 1000 == 0 {
                break;
            }
        }
        let whole: Vec<f64> = cells.iter().map(|&(i, j)| g.thrust_mean(w, i, j, full)).collect();
        for depth in 0..full {
            let started = Instant::now();
            let mut errs: Vec<f64> = cells.iter().zip(&whole).map(|(&(i, j), &f)| (g.thrust_mean(w, i, j, depth) - f).abs()).collect();
            let us = started.elapsed().as_secs_f64() * 1e6 / cells.len() as f64;
            errs.sort_by(f64::total_cmp);
            let n = errs.len();
            let line = format!("{w} {depth} {} {:.2} {:.2} {:.2} {:.0}
", 9u64.pow(depth), errs[n / 2], errs[(n * 99 / 100).min(n - 1)], errs[n - 1], us);
            print!("{line}");
            out.push_str(&line);
        }
    }
    if let Ok(dir) = std::env::var("LADDER_OUT") {
        std::fs::write(format!("{dir}/averaging.txt"), out).expect("the averaging table");
    }
}

/// What one cell costs at each level: a fresh ladder over the same ground,
/// its top built, then a single cell at the haven requested, timed, with
/// the deforms each pass ran. The first request at a level reads the ground
/// cold; the second, on another fresh ladder, finds it read.
#[test]
#[ignore]
fn ladder_single_cell() {
    let c = stack(WORLD_SEED);
    let g = Layered::new(&c, HAVEN, outline_half(), WORLD_SEED, 2);
    let ext = extents();
    let mut out = String::from("w cold_s warm_s deforms(ground water route built) per level, warm\n");
    for k in 1..ext.len() {
        let mut times = Vec::new();
        let mut costs = String::new();
        for _ in 0..2 {
            let mut ladder = Ladder::new(&g, &ext, Params::default());
            let t = Instant::now();
            ladder.request(k, &[(0, 0)]);
            times.push(t.elapsed().as_secs_f64());
            costs = ladder.levels[1..].iter().map(|lv| format!("{}:{:?}", lv.w, lv.cost)).collect::<Vec<_>>().join(" ");
        }
        let line = format!("{} {:.2} {:.2} {}\n", ext[k].0, times[0], times[1], costs);
        print!("{line}");
        out.push_str(&line);
    }
    if let Ok(dir) = std::env::var("LADDER_OUT") {
        std::fs::write(format!("{dir}/single_cell.txt"), out).expect("the single-cell table");
    }
}

/// Which of a cell's values differ between a ladder that requested one cell
/// and one that requested the whole region, field by field.
#[test]
#[ignore]
fn ladder_seam_debug() {
    let c = stack(WORLD_SEED);
    let g = Layered::new(&c, HAVEN, outline_half(), WORLD_SEED, 2);
    let ext = extents();
    let k = 1;
    let mut whole = Ladder::new(&g, &ext, Params::default());
    whole.request(k, &region_cells(ext[k].0));
    let mut one = Ladder::new(&g, &ext, Params::default());
    one.request(k, &[(3, -5)]);
    let (a, b) = (&one.levels[k], &whole.levels[k]);
    let mut counts = std::collections::BTreeMap::new();
    let mut shown = 0;
    for c in 0..a.n * a.n {
        if !a.routed[c] {
            continue;
        }
        let (i, j) = a.ij(c);
        let cb = b.idx(i, j).unwrap();
        let nn = a.n * a.n;
        let mut bad = Vec::new();
        if a.built[c] && b.built[cb] && !same(a.z[c], b.z[cb]) { bad.push("z"); }
        if a.supplied[c] && b.supplied[cb] && !same(a.ground[c], b.ground[cb]) { bad.push("ground"); }
        if drawn(a, i, j) && drawn(b, i, j) && !(0..6).all(|s| same(a.cc[c][s], b.cc[cb][s])) { bad.push("cc"); }
        for t in 0..3 {
            let (va, vb) = (t * nn + c, t * nn + cb);
            if !same(a.gelev[va], b.gelev[vb]) { bad.push("gelev"); }
            if !same(a.psurf[va], b.psurf[vb]) { bad.push("psurf"); }
            if a.trough[va] != b.trough[vb] { bad.push("trough"); }
            if !same(a.acc[va], b.acc[vb]) { bad.push("acc"); }
            if !same(a.floor[va], b.floor[vb]) { bad.push("floor"); }
            if a.line_on[va] != b.line_on[vb] || !same(a.line_fl[va], b.line_fl[vb]) { bad.push("line"); }
            let r = |lv: &Level, v: i64| (v >= 0).then(|| lv.vertex(v as usize));
            if r(a, a.rec[va]) != r(b, b.rec[vb]) { bad.push("rec"); }
        }
        bad.sort();
        bad.dedup();
        for f in &bad {
            *counts.entry(*f).or_insert(0) += 1;
        }
        if !bad.is_empty() && shown < 12 {
            shown += 1;
            println!("cell {i},{j}: {:?} built {}/{} corners {}/{} lined {}/{} z {}/{} gelev0 {}/{} psurf0 {}/{}", bad, a.built[c], b.built[cb], a.corners[c], b.corners[cb],
                a.lined[c], b.lined[cb], a.z[c], b.z[cb], a.gelev[c], b.gelev[cb], a.psurf[c], b.psurf[cb]);
        }
    }
    println!("{counts:?}");
}

/// CPU time this process has spent, user and kernel, in seconds.
#[cfg(windows)]
fn cpu_seconds() -> f64 {
    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        lo: u32,
        hi: u32,
    }
    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn GetProcessTimes(h: isize, created: *mut FileTime, exited: *mut FileTime, kernel: *mut FileTime, user: *mut FileTime) -> i32;
    }
    let (mut c, mut e, mut k, mut u) = (FileTime::default(), FileTime::default(), FileTime::default(), FileTime::default());
    // SAFETY: the pseudo-handle of the current process and four live out-parameters.
    unsafe { GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u) };
    let s = |t: &FileTime| ((t.hi as u64) << 32 | t.lo as u64) as f64 * 1e-7;
    s(&k) + s(&u)
}

#[cfg(not(windows))]
fn cpu_seconds() -> f64 {
    f64::NAN
}

/// What an area of tiles costs: `LADDER_AREA` tiles across (81 by default),
/// centred on the haven, requested at tiles on a fresh ladder, cold (the
/// ground first read in this process) and then warm on another fresh ladder:
/// wall and CPU time, the threads, and each level's deforms and cells built.
#[test]
#[ignore]
fn ladder_area_speed() {
    let across: i64 = std::env::var("LADDER_AREA").ok().and_then(|a| a.parse().ok()).unwrap_or(81);
    let c = stack(WORLD_SEED);
    let g = Layered::new(&c, HAVEN, outline_half(), WORLD_SEED, 2);
    let ext = extents();
    let k = ext.len() - 1;
    let h = across / 2;
    let cells: Vec<(i64, i64)> = (-h..=h).flat_map(|i| (-h..=h).map(move |j| (i, j))).collect();
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let mut out = format!("area {across} x {across} tiles ({} tiles), threads {threads}\n", cells.len());
    for run in ["cold", "warm"] {
        let (t0, c0) = (Instant::now(), cpu_seconds());
        let mut ladder = Ladder::new(&g, &ext, Params::default());
        let (top_wall, top_cpu) = (t0.elapsed().as_secs_f64(), cpu_seconds() - c0);
        let (t1, c1) = (Instant::now(), cpu_seconds());
        ladder.request(k, &cells);
        let (wall, cpu) = (t1.elapsed().as_secs_f64(), cpu_seconds() - c1);
        out.push_str(&format!("{run}: top {top_wall:.1} s wall, {top_cpu:.1} s CPU; the area {wall:.1} s wall, {cpu:.1} s CPU\n"));
        for lv in &ladder.levels[1..] {
            let built = lv.built.iter().filter(|&&b| b).count();
            out.push_str(&format!("   w={}: deforms ground {} water {} route {}; cells built {built}\n", lv.w, lv.cost[0], lv.cost[1], lv.cost[2]));
        }
    }
    print!("{out}");
    if let Ok(dir) = std::env::var("LADDER_OUT") {
        std::fs::write(format!("{dir}/area_{across}.txt"), out).expect("the area table");
    }
}
