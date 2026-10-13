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
use world::ladder::{Ground, Ladder, Level, Params, LAST_WATER, STAGES};
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

/// What the running probe is doing.
static STATUS: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

/// Says what the probe is doing now, and starts the heartbeat that prints
/// it with the time every 30 seconds, so a run never goes quiet.
fn status(now: &str) {
    static BEAT: std::sync::Once = std::sync::Once::new();
    *STATUS.lock().unwrap() = now.to_string();
    println!("progress: {now}");
    BEAT.call_once(|| {
        let started = Instant::now();
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_secs(30));
            println!("progress {:.0}s: {}", started.elapsed().as_secs_f64(), STATUS.lock().unwrap());
        });
    });
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
        status(&format!("exact checks at {w}"));
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
                    // A level finer than water keeps no routing: its built cells.
                    if lv.routed[cc] || (lv.w < LAST_WATER && lv.built[cc]) {
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
        ("wfull", &lv.wfull),
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

/// The tiles' channels: curves drawn and those whose floor rose; how far
/// from its governing bank each tile a channel cut lies; and every river
/// whose walls reached the reach still below the ground, with the tiles
/// where they did and the largest step left there.
fn channels<G: Ground>(ladder: &Ladder<G>) -> String {
    let mut reach: Vec<f64> = ladder.channel_reach.iter().map(|r| r[0]).collect();
    reach.sort_by(f64::total_cmp);
    let pct = |q: f64| if reach.is_empty() { f64::NAN } else { reach[((reach.len() - 1) as f64 * q).round() as usize] };
    // The reach counting only tiles where the channel is a whole step or
    // more below the drawn surface.
    let mut deep: Vec<f64> = ladder.channel_reach.iter().filter(|r| r[1] >= 1.0).map(|r| r[0]).collect();
    deep.sort_by(f64::total_cmp);
    let dpct = |q: f64| if deep.is_empty() { f64::NAN } else { deep[((deep.len() - 1) as f64 * q).round() as usize] };
    let under_half = ladder.channel_reach.iter().filter(|r| r[1] < 0.5).count();
    let mut out = format!(
        "channels: {} curves, {} with a rising floor; {} tiles reached, {} lowered; reach p50 {:.1} p99 {:.1} max {:.1} tiles of {:.1}; largest step at the zone's edge {:.1}; {} rivers at the reach\n",
        ladder.channel_stats[0],
        ladder.channel_stats[1],
        reach.len(),
        ladder.channel_stats[2],
        pct(0.5),
        pct(0.99),
        pct(1.0),
        world::ladder::channel::channel_reach(),
        ladder.channel_edge,
        ladder.channel_capped.len()
    );
    let mut capped: Vec<_> = ladder.channel_capped.iter().collect();
    capped.sort_by(|a, b| b.1 .1.total_cmp(&a.1 .1));
    out.push_str(&format!(
        "   reached tiles under half a step below the drawn surface: {under_half}; a step or more below: {} tiles, reach p50 {:.1} p99 {:.1} max {:.1}\n",
        deep.len(),
        dpct(0.5),
        dpct(0.99),
        dpct(1.0)
    ));
    for (river, (tiles, step)) in capped {
        out.push_str(&format!("   at the reach: river {river:?}, {tiles} tiles, step left up to {step:.1}\n"));
    }
    out
}

/// Water at `LAST_WATER` against the valley from every river the level
/// routed, however far: over a sample of its cells in the middle half of
/// its region, the height's difference from the dry ground met by that
/// valley, rounded, and the cut's before rounding.
fn truth<G: Ground>(ladder: &Ladder<G>) -> String {
    let Some(k) = ladder.levels.iter().position(|lv| lv.w == LAST_WATER) else { return String::new() };
    let lv = &ladder.levels[k];
    let h = region(lv.w) / lv.w / 2;
    // Every built cell in the middle half, or a sample of them where they
    // are more.
    let mut cells: Vec<(i64, i64)> = (-h..=h).flat_map(|i| (-h..=h).map(move |j| (i, j))).filter(|&(i, j)| lv.idx(i, j).is_some_and(|x| lv.built[x])).collect();
    let mut draw = Draw(23);
    for x in (1..cells.len()).rev() {
        let y = draw.range(0, x as i64 + 1) as usize;
        cells.swap(x, y);
    }
    cells.truncate(2000);
    if cells.is_empty() {
        return String::new();
    }
    let whole = ladder.unlimited_cut(k, &cells);
    let (mut steps, mut cuts) = (Vec::new(), Vec::new());
    let mut far = Vec::new();
    for (&(i, j), &t) in cells.iter().zip(&whole) {
        let c = lv.idx(i, j).unwrap();
        let dry = lv.ground[c] - lv.wpred[c];
        let off = (lv.z[c] - (dry + t + 0.5).floor()).abs();
        if off > 3.0 {
            far.push(format!(
                "   ({i}, {j}): {off} steps; cut {:.1}, every river's {t:.1}, handed down {:.1}; governing river {:.2} cells off, fading {}\n",
                lv.cut[c], lv.wpred[c], lv.wdist[c], lv.wfade[c]
            ));
        }
        steps.push(off);
        cuts.push((lv.cut[c] - t).abs());
    }
    let pct = |v: &mut Vec<f64>, q: f64| {
        v.sort_by(f64::total_cmp);
        v[((v.len() - 1) as f64 * q).round() as usize]
    };
    format!(
        "truth w={}: {} cells against min(dry ground, valley from every river): height p99 {} max {} steps; cut p99 {:.2} max {:.2}\n",
        lv.w,
        cells.len(),
        pct(&mut steps, 0.99),
        pct(&mut steps, 1.0),
        pct(&mut cuts, 0.99),
        pct(&mut cuts, 1.0)
    ) + &format!("   cells over 3 steps: {}\n", far.len())
        + &far.concat()
}

fn run<G: Ground>(ground: &G, p: Params, dir: &str, log: &mut String) {
    let ext = extents();
    let started = Instant::now();
    status("building the top");
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
        status(&format!("requesting level {}", ext[k].0));
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
    log.push_str(&timing(&ladder.levels, &ladder.timing));
    log.push_str(&channels(&ladder));
    status("checking 27 against every river");
    log.push_str(&truth(&ladder));
    if std::env::var("LADDER_EXACT").is_ok_and(|v| v == "1") {
        exact(ground, &ladder, p, log);
    }
    status("writing the dumps");
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
        Ok("A") => (1_500.0, 40_000.0),
        _ => (13_122.0, 1_102_500.0),
    };
    let mut log = format!("ground layered, thrust averaged {depth} levels down, water {water}\n");
    let g = Layered::new(&c, HAVEN, outline_half(), WORLD_SEED, depth);
    // How far water reads, in cells: the deform cell's ring unless a test
    // widens it.
    let ring = std::env::var("LADDER_RING").ok().and_then(|r| r.parse().ok()).unwrap_or(world::ladder::RING);
    log.push_str(&format!("cut from {cut_from} full at {cut_full}, ring {ring}\n"));
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
    let mut out = String::from("w depth samples err_p50 err_p99 err_max us_per_cell (levels; whole = every tile)\n");
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
            let line = format!("{w} {depth} {} {:.2} {:.2} {:.2} {:.0}\n", 9u64.pow(depth), errs[n / 2], errs[(n * 99 / 100).min(n - 1)], errs[n - 1], us);
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

/// This process's working set now and at its peak, and its private memory
/// at its peak, in bytes.
#[cfg(windows)]
fn memory() -> (usize, usize, usize) {
    #[repr(C)]
    #[derive(Default)]
    struct Counters {
        cb: u32,
        faults: u32,
        peak_working: usize,
        working: usize,
        quota: [usize; 4],
        private: usize,
        peak_private: usize,
    }
    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn K32GetProcessMemoryInfo(h: isize, counters: *mut Counters, cb: u32) -> i32;
    }
    let mut m = Counters { cb: std::mem::size_of::<Counters>() as u32, ..Default::default() };
    // SAFETY: the pseudo-handle of the current process and a live, sized out-parameter.
    unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut m, m.cb) };
    (m.working, m.peak_working, m.peak_private)
}

#[cfg(not(windows))]
fn memory() -> (usize, usize, usize) {
    (0, 0, 0)
}

/// Each level's wall time by stage, in seconds, nested work at other
/// levels not counted.
fn timing(levels: &[Level], times: &[[f64; 8]]) -> String {
    let mut out = format!("   {:>5} {}\n", "w", STAGES.iter().map(|s| format!("{s:>8}")).collect::<String>());
    for (lv, t) in levels.iter().zip(times) {
        out.push_str(&format!("   {:>5} {} total {:.2}\n", lv.w, t.iter().map(|x| format!("{x:>8.2}")).collect::<String>(), t.iter().sum::<f64>()));
    }
    out
}

fn timed(f: &mut dyn FnMut()) -> (f64, f64) {
    let (t, c) = (Instant::now(), cpu_seconds());
    f();
    (t.elapsed().as_secs_f64(), cpu_seconds() - c)
}

/// What an area of tiles costs: `LADDER_AREA` tiles across (243 by
/// default), centred on the haven, requested at tiles four ways, each with
/// its wall and CPU time and each level's time by stage:
///
/// - cold: a fresh ladder, the ground first read in this process;
/// - fresh: another fresh ladder, the ground's reads kept, so every level
///   is computed again but nothing is read;
/// - again: the same request on the cold run's ladder, which keeps it;
/// - baked: a ladder whose levels to `LAST_WATER` were built beforehand
///   over the cells the area needs, the levels below computed, the ground's
///   reads kept: the budget. Its tiles must equal the cold run's.
#[test]
#[ignore]
fn ladder_area_speed() {
    let across: i64 = std::env::var("LADDER_AREA").ok().and_then(|a| a.parse().ok()).unwrap_or(243);
    let c = stack(WORLD_SEED);
    let g = Layered::new(&c, HAVEN, outline_half(), WORLD_SEED, 2);
    let ext = extents();
    let k = ext.len() - 1;
    let h = across / 2;
    let cells: Vec<(i64, i64)> = (-h..=h).flat_map(|i| (-h..=h).map(move |j| (i, j))).collect();
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let mut out = format!("area {across} x {across} tiles ({} tiles), threads {threads}\n", cells.len());
    let mut cold = None;
    for run in ["cold", "fresh"] {
        status(&format!("the area, {run}"));
        let mut ladder = None;
        let (top_wall, top_cpu) = timed(&mut || ladder = Some(Ladder::new(&g, &ext, Params::default())));
        let mut ladder = ladder.unwrap();
        let (wall, cpu) = timed(&mut || ladder.request(k, &cells));
        out.push_str(&format!("{run}: top {top_wall:.2} s wall, {top_cpu:.1} s CPU; the area {wall:.2} s wall, {cpu:.1} s CPU\n"));
        for lv in &ladder.levels[1..] {
            let built = lv.built.iter().filter(|&&b| b).count();
            out.push_str(&format!("   w={}: deforms ground {} water {} route {}; cells built {built}\n", lv.w, lv.cost[0], lv.cost[1], lv.cost[2]));
        }
        out.push_str(&timing(&ladder.levels, &ladder.timing));
        out.push_str(&channels(&ladder));
        print!("{out}");
        if cold.is_none() {
            cold = Some(ladder);
        }
    }
    let mut cold = cold.unwrap();
    status("the area again");
    let before: f64 = cold.timing.iter().flatten().sum();
    let (wall, cpu) = timed(&mut || cold.request(k, &cells));
    let more = cold.timing.iter().flatten().sum::<f64>() - before;
    out.push_str(&format!("again: the area {wall:.4} s wall, {cpu:.3} s CPU; stage time added {more:.4} s\n"));
    // The levels to LAST_WATER over every cell the cold run routed or built
    // there.
    let s = ext.iter().position(|e| e.0 == LAST_WATER).unwrap();
    status("the area from a bake");
    let mut baked = Ladder::new(&g, &ext, Params::default());
    let src = &cold.levels[s];
    let bake_cells: Vec<(i64, i64)> = (0..src.n * src.n).filter(|&c| src.built[c] || src.routed[c]).map(|c| src.ij(c)).collect();
    let (bwall, _) = timed(&mut || baked.request(s, &bake_cells));
    let start = baked.timing.clone();
    let (wall, cpu) = timed(&mut || baked.request(k, &cells));
    out.push_str(&format!("baked: the levels to {LAST_WATER} over {} of its cells in {bwall:.1} s; then the area {wall:.3} s wall, {cpu:.2} s CPU\n", bake_cells.len()));
    let fine: Vec<[f64; 8]> = baked.timing.iter().zip(&start).map(|(f, b)| std::array::from_fn(|x| f[x] - b[x])).collect();
    out.push_str(&timing(&baked.levels, &fine));
    for lv in &baked.levels[s + 1..] {
        let built = lv.built.iter().filter(|&&b| b).count();
        out.push_str(&format!("   w={}: cells built {built}\n", lv.w));
    }
    let (a, b) = (&cold.levels[k], &baked.levels[k]);
    let differ = cells.iter().filter(|&&(i, j)| !same(a.z[a.idx(i, j).unwrap()], b.z[b.idx(i, j).unwrap()])).count();
    out.push_str(&format!("   tiles differing from the cold run: {differ}\n"));
    // The same bake on a ground read afresh: the levels below read cold.
    status("the area from a bake, its ground read cold");
    let fresh = Layered::new(&c, HAVEN, outline_half(), WORLD_SEED, 2);
    let mut baked = Ladder::new(&fresh, &ext, Params::default());
    baked.request(s, &bake_cells);
    let start = baked.timing.clone();
    let (wall, cpu) = timed(&mut || baked.request(k, &cells));
    out.push_str(&format!("baked, ground read cold below {LAST_WATER}: the area {wall:.3} s wall, {cpu:.2} s CPU\n"));
    let fine: Vec<[f64; 8]> = baked.timing.iter().zip(&start).map(|(f, b)| std::array::from_fn(|x| f[x] - b[x])).collect();
    out.push_str(&timing(&baked.levels, &fine));
    let b = &baked.levels[k];
    let differ = cells.iter().filter(|&&(i, j)| !same(a.z[a.idx(i, j).unwrap()], b.z[b.idx(i, j).unwrap()])).count();
    out.push_str(&format!("   tiles differing from the cold run: {differ}\n"));
    // The game's tiles over the same area, each composed by `tile_at`: on
    // one thread on a fresh standard stack, then on every thread on another
    // fresh one, then on every thread again on that one.
    let tiles: Vec<(i32, i32)> = cells.iter().map(|&(i, j)| (HAVEN.0 + i as i32, HAVEN.1 + j as i32)).collect();
    status("the game's tiles");
    let game = Composite::standard(WORLD_SEED);
    let (wall, cpu) = timed(&mut || {
        let _: f64 = tiles.iter().map(|&(q, r)| game.tile_at(q, r).elevation).sum();
    });
    out.push_str(&format!("game: Composite::standard fresh, tile_at over the area on one thread: {wall:.3} s wall, {cpu:.2} s CPU\n"));
    let game = Composite::standard(WORLD_SEED);
    let chunk = tiles.len().div_ceil(threads);
    for run in ["fresh", "again"] {
        let (wall, cpu) = timed(&mut || {
            std::thread::scope(|sc| {
                for part in tiles.chunks(chunk) {
                    let game = &game;
                    sc.spawn(move || part.iter().map(|&(q, r)| game.tile_at(q, r).elevation).sum::<f64>());
                }
            })
        });
        out.push_str(&format!("game: Composite::standard {run}, tile_at over the area on {threads} threads: {wall:.3} s wall, {cpu:.2} s CPU\n"));
    }
    print!("{out}");
    if let Ok(dir) = std::env::var("LADDER_OUT") {
        std::fs::write(format!("{dir}/area_{across}.txt"), out).expect("the area table");
    }
}

/// What a level holds in the ladder's working arrays, in bytes per cell:
/// each cell's values, and each of its three vertices'.
const WORKING: usize = {
    let cell = 2 * 8 // layers
        + 9 * 8 // erod, age, p, ground, wpred, cut, wfull, wdist, z
        + 2 * 8 // wborn, wgov
        + 6 * 8 + 6 * 8 // a3, b3; cc
        + 3 * 16 // blocks
        + 10; // wfade and the nine progress flags
    let vertex = 12 * 8 // elev, psurf, gelev, uelev, base, floor, acc, line_acc, line_fl, line_base, line_dev, line_pin
        + 5 * 8 // born, rec, line_rec, line_born, line_corr
        + 2 * 8 // line_anchor
        + 4; // trough, line_on, line_wall, line_hollow
    cell + 3 * vertex
};

/// One field a baked level keeps: its name, values per cell, and whether
/// it is a height or a floor, which fits two bytes.
type Kept = (&'static str, usize, bool);

/// What a level finer than water reads of a baked `LAST_WATER` level, per
/// cell: the prediction's heights, corners and handed-down values, the rock,
/// and each vertex's river the channels are drawn from.
const KEEP_LAST: [Kept; 11] = [
    ("z", 1, true),
    ("a3 (meet values at corner A)", 3, true),
    ("b3 (meet values at corner B)", 3, true),
    ("cc (six corner values)", 6, true),
    ("layers", 2, false),
    ("cut", 1, false),
    ("erod", 1, false),
    ("river: kind (line, born, none) x3", 3, false),
    ("river: receiver x3", 3, false),
    ("river: floor x3", 3, true),
    ("river: catchment x3", 3, false),
];

/// What a level that routes and cuts water reads of a baked level above it:
/// the prediction's, the ground and cut its water term reads, and every
/// vertex's trough and line.
const KEEP_ABOVE: [Kept; 22] = [
    ("z", 1, true),
    ("a3", 3, true),
    ("b3", 3, true),
    ("cc", 6, true),
    ("layers", 2, false),
    ("cut", 1, false),
    ("ground", 1, true),
    ("wpred", 1, false),
    ("erod", 1, false),
    ("age", 1, false),
    ("trough, line_on x3", 3, false),
    ("rec x3", 3, false),
    ("acc x3", 3, false),
    ("floor x3", 3, true),
    ("base x3", 3, true),
    ("born x3", 3, false),
    ("elev x3", 3, true),
    ("line_rec x3", 3, false),
    ("line_acc x3", 3, false),
    ("line_fl x3", 3, true),
    ("line_base x3", 3, true),
    ("line_born x3", 3, false),
];

fn kept_table(name: &str, fields: &[Kept]) -> (String, usize, usize) {
    let mut out = format!("   kept per cell, {name}:\n");
    let (mut wide, mut narrow) = (0, 0);
    for &(field, n, height) in fields {
        let q = if height { 2 } else { 8 };
        out.push_str(&format!("      {field:<36} {n:>2} values: {:>4} B at 8 B, {:>4} B quantized\n", 8 * n, q * n));
        wide += 8 * n;
        narrow += q * n;
    }
    out.push_str(&format!("      {:<36}           {wide:>4} B at 8 B, {narrow:>4} B quantized\n", "total"));
    (out, wide, narrow)
}

/// What baking the levels to `LADDER_BAKE` (81 by default) costs over the
/// haven's landmass: the top's land joined to the haven's cell and its box,
/// then every cell of the finest level under that land requested in one
/// ladder, timed by stage; what the finest level keeps, by field; and what a
/// bake made block by block, one top cell at a time, holds at its peak,
/// measured on `LADDER_BAKE_SAMPLES` blocks (3 by default) each requested on
/// a fresh ladder: every cell any level read, in working arrays, and the
/// top kept whole.
#[test]
#[ignore]
fn ladder_bake() {
    let c = stack(WORLD_SEED);
    let g = Layered::new(&c, HAVEN, outline_half(), WORLD_SEED, 2);
    let top = LEVELS[0];
    let half = top.1 / top.0 + 2;
    status("the landmass");
    let alone = Ladder::new(&g, &[(top.0, -half, half + 1)], Params::default());
    let lv = &alone.levels[0];
    let mut land = vec![false; lv.n * lv.n];
    let mut todo = vec![lv.idx(0, 0).unwrap()];
    while let Some(c) = todo.pop() {
        if land[c] || lv.z[c] < 0.0 {
            continue;
        }
        land[c] = true;
        let (i, j) = lv.ij(c);
        for (di, dj) in world::ladder::geom::NB {
            if let Some(d) = lv.idx(i + di, j + dj) {
                todo.push(d);
            }
        }
    }
    let at: Vec<(i64, i64)> = (0..land.len()).filter(|&c| land[c]).map(|c| lv.ij(c)).collect();
    let (ilo, ihi) = (at.iter().map(|a| a.0).min().unwrap(), at.iter().map(|a| a.0).max().unwrap());
    let (jlo, jhi) = (at.iter().map(|a| a.1).min().unwrap(), at.iter().map(|a| a.1).max().unwrap());
    let touches = at.iter().any(|&(i, j)| i.abs() >= half - 1 || j.abs() >= half - 1);
    let mut out = format!(
        "landmass: {} cells of 729, {:.0} tiles, box i {ilo}..{ihi} j {jlo}..{jhi} (cells of 729); reaches the top's edge: {touches}\n",
        at.len(),
        at.len() as f64 * 729.0 * 729.0
    );
    print!("{out}");
    if std::env::var("LADDER_BAKE_SIZE_ONLY").is_ok() {
        return;
    }
    let finest: i64 = std::env::var("LADDER_BAKE").ok().and_then(|f| f.parse().ok()).unwrap_or(81);
    let samples: usize = std::env::var("LADDER_BAKE_SAMPLES").ok().and_then(|f| f.parse().ok()).unwrap_or(3);
    // Each level's cells cover the box and the reach of the levels below.
    let ext: Vec<(i64, i64, i64)> = [729i64, 243, 81, 27]
        .iter()
        .filter(|&&w| w >= finest)
        .map(|&w| {
            let m = 729 / w;
            let pad = if w == 729 { 2 } else { 40 };
            (w, ilo.min(jlo) * m - pad, (ihi.max(jhi) + 1) * m + pad)
        })
        .collect();
    let k = ext.len() - 1;
    let under = |fl: &Level, top_cells: &dyn Fn(i64, i64) -> bool| -> Vec<(i64, i64)> {
        (0..fl.n * fl.n)
            .map(|c| fl.ij(c))
            .filter(|&(i, j)| {
                let (a, b) = world::ladder::geom::ancestor(i, j, k as u32);
                top_cells(a, b)
            })
            .collect()
    };
    let top_cells = alone.levels[0].n * alone.levels[0].n;

    // Block by block: one top cell's area at a time on a fresh ladder.
    out.push_str(&format!("block by block, one top cell ({} cells of {finest}) a block, {samples} blocks measured:\n", (729 / finest).pow(2)));
    let picks: Vec<(i64, i64)> = (0..samples).map(|s| at[(s + 1) * at.len() / (samples + 1)]).collect();
    let mut peak = 0usize;
    for &(a, b) in &picks {
        status(&format!("block ({a}, {b})"));
        let mut ladder = Ladder::new(&g, &ext, Params::default());
        let cells = under(&ladder.levels[k], &|x, y| (x, y) == (a, b));
        let (t, _) = timed(&mut || ladder.request(k, &cells));
        let mut held = 0usize;
        let mut row = String::new();
        for lv in &ladder.levels[1..] {
            let read = (0..lv.n * lv.n).filter(|&c| !lv.layers[0][c].is_nan() || lv.supplied[c]).count();
            let built = lv.built.iter().filter(|&&x| x).count();
            held += read;
            row.push_str(&format!(" w={} read {read} built {built};", lv.w));
        }
        let bytes = (held + top_cells) * WORKING;
        peak = peak.max(bytes);
        out.push_str(&format!(
            "   block ({a}, {b}): {:.1} s; cells requested {}, built at {finest} {} ({:.1}x);{row} peak {:.0} MB (the top {:.0} MB of it)\n",
            t,
            cells.len(),
            ladder.levels[k].built.iter().filter(|&&x| x).count(),
            ladder.levels[k].built.iter().filter(|&&x| x).count() as f64 / cells.len() as f64,
            bytes as f64 / 1e6,
            (top_cells * WORKING) as f64 / 1e6
        ));
        print!("{out}");
    }
    out.push_str(&format!("   working arrays {WORKING} B a cell; peak over the blocks {:.0} MB\n", peak as f64 / 1e6));

    // The whole landmass in one ladder.
    status(&format!("the bake to {finest}"));
    let before = memory();
    let (t0, c0) = (Instant::now(), cpu_seconds());
    let mut ladder = Ladder::new(&g, &ext, Params::default());
    let top_s = t0.elapsed().as_secs_f64();
    let cells = under(&ladder.levels[k], &|x, y| alone.levels[0].idx(x, y).is_some_and(|d| land[d]));
    ladder.request(k, &cells);
    let (wall, cpu) = (t0.elapsed().as_secs_f64(), cpu_seconds() - c0);
    out.push_str(&format!("bake 729 -> {finest} in one ladder: {} cells requested at {finest}; top {top_s:.1} s; {wall:.1} s wall, {cpu:.0} s CPU\n", cells.len()));
    let after = memory();
    out.push_str(&format!(
        "   memory: working set before the bake {:.2} GB, after {:.2} GB; process peak working set {:.2} GB, peak private {:.2} GB\n",
        before.0 as f64 / 1e9,
        after.0 as f64 / 1e9,
        after.1 as f64 / 1e9,
        after.2 as f64 / 1e9
    ));
    out.push_str(&timing(&ladder.levels, &ladder.timing));
    let (table, wide, narrow) =
        if finest == LAST_WATER { kept_table("the last water level, read by the dry levels and the channels", &KEEP_LAST) } else { kept_table("a level read by water's levels below it", &KEEP_ABOVE) };
    out.push_str(&table);
    let mut all = (0usize, 0usize);
    for lv in &ladder.levels {
        let built = lv.built.iter().filter(|&&b| b).count();
        all.0 += built * wide;
        all.1 += built * narrow;
        out.push_str(&format!(
            "   w={}: {built} cells built; at the table's rate {:.1} MB at 8 B, {:.1} MB quantized; working arrays held {:.0} MB\n",
            lv.w,
            (built * wide) as f64 / 1e6,
            (built * narrow) as f64 / 1e6,
            (lv.n * lv.n * WORKING) as f64 / 1e6
        ));
    }
    out.push_str(&format!("   every level at the table's rate: {:.1} MB at 8 B, {:.1} MB quantized\n", all.0 as f64 / 1e6, all.1 as f64 / 1e6));
    print!("{out}");
    if let Ok(dir) = std::env::var("LADDER_OUT") {
        std::fs::write(format!("{dir}/bake_{finest}.txt"), out).expect("the bake table");
    }
}
