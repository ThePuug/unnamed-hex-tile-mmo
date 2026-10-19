//! The ladder over the real ground around the haven, its lattice anchored
//! at the world's origin: every level requested over a region, identity and
//! seams checked, and every level written for the measures. The ground
//! is the base layers read by `StackGround`, each cell at its nine
//! children's centres.
//! `LADDER_EXACT=1` checks identity by position and seams between requests
//! made apart; `LADDER_WATER=0` runs it dry.
//!
//! Run: `LADDER_OUT=<dir> cargo test --release -p world --test ladder_gate -- --ignored --nocapture --exact ladder_gate`
//!
//! Every probe prints what it is doing at least every 30 seconds.

use std::io::Write;
use std::time::Instant;

use world::events::ladder_ground::{StackGround, AREA_PAD};
use world::events::Composite;
use common::corner::meet;
use world::ladder::geom::cube_round;
use world::ladder::terrain::{Cell, Terrain};
use world::ladder::{Bounds, Ground, Ladder, Level, Params, STAGES};
use world::WORLD_SEED;

/// The haven's tile, as `common_bevy::haven::HAVEN_LOCATION` holds it.
const HAVEN: (i32, i32) = (107289, -7677);

/// Each level to 27 wide and how far its cells reach from the haven, in
/// tiles.
const COARSE: [(i64, i64); 4] = [(729, 142_884), (243, 57_591), (81, 19_197), (27, 6_399)];

/// The levels finer than 27, which reach [`AREA_PAD`] of their cells past
/// their region.
const FINE: [i64; 3] = [9, 3, 1];

/// Each level's request: a rhombus of cells around the haven's, this many
/// tiles either way.
fn region(w: i64) -> i64 {
    match w {
        1 | 3 => 364,
        9 => 1080,
        27 => 3240,
        81 => 9720,
        _ => 29160,
    }
}

/// The cell of the level `w` wide whose centre lies nearest the haven.
fn haven_cell(w: i64) -> (i64, i64) {
    cube_round(HAVEN.0 as f64 / w as f64, HAVEN.1 as f64 / w as f64)
}

/// The levels to 27 as far as [`COARSE`] reaches either way of the haven's
/// cell, the finer ones [`AREA_PAD`] of their cells past their region;
/// `LADDER_SMALL` gives every level below the top the same reach in tiles,
/// for a test.
fn bounds() -> Vec<Bounds> {
    let small: Option<i64> = std::env::var("LADDER_SMALL").ok().and_then(|s| s.parse().ok());
    COARSE
        .iter()
        .copied()
        .chain(FINE.iter().map(|&w| (w, region(w) + AREA_PAD * w)))
        .map(|(w, reach)| {
            let reach = match small {
                Some(s) if w < 729 => s,
                _ => reach,
            };
            let h = reach / w + 2;
            let (ci, cj) = haven_cell(w);
            (w, (ci - h, cj - h), (ci + h + 1, cj + h + 1))
        })
        .collect()
}

/// A level's request: its cells within [`region`] of the haven either way.
fn region_cells(w: i64) -> Vec<(i64, i64)> {
    let h = region(w) / w;
    let (ci, cj) = haven_cell(w);
    (-h..=h).flat_map(|a| (-h..=h).map(move |b| (ci + a, cj + b))).collect()
}

/// How far either way of the haven the outlines are built over, in tiles.
fn outline_half() -> f64 {
    (COARSE[0].1 + 3 * COARSE[0].0) as f64
}

fn ground() -> StackGround {
    StackGround::new(WORLD_SEED, HAVEN, outline_half())
}

fn index_of(ladder: &Ladder<impl Ground>, w: i64) -> usize {
    ladder.levels.iter().position(|l| l.w == w).expect("a level of the ladder")
}

/// Whether two fine cells are the same, bit for bit.
fn same_cell(a: &Option<Cell>, b: &Option<Cell>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => {
            same(a.z, b.z)
                && same(a.zd, b.zd)
                && same(a.cut, b.cut)
                && a.water.map(f64::to_bits) == b.water.map(f64::to_bits)
                && a.layers.iter().zip(b.layers.iter()).all(|(x, y)| same(*x, *y))
        }
        (None, None) => true,
        _ => false,
    }
}

fn exact<G: Ground>(ground: &G, whole: &Terrain<G>, p: Params, out: &mut String) {
    let bounds = bounds();
    let mut draw = Draw(5);
    let held = whole.ladder();
    for (k, &(w, _, _)) in bounds.iter().enumerate().skip(1) {
        let h = region(w) / w;
        let (ci, cj) = haven_cell(w);
        let (mut identical, mut seams, mut cells) = (true, 0u64, 0u64);
        let started = Instant::now();
        status(&format!("exact checks at {w}"));
        for _ in 0..3 {
            let c = (ci + draw.range(-h / 2, h / 2 + 1), cj + draw.range(-h / 2, h / 2 + 1));
            let one = Terrain::new_box(ground, &bounds, p);
            one.ladder().request(k, &[c]);
            let mut block: Vec<(i64, i64)> = (-4..=4).flat_map(|a| (-4..=4).map(move |b| (c.0 + a, c.1 + b))).collect();
            for x in (1..block.len()).rev() {
                let y = draw.range(0, x as i64 + 1) as usize;
                block.swap(x, y);
            }
            let apart = Terrain::new_box(ground, &bounds, p);
            for &cell in &block {
                apart.ladder().request(k, &[cell]);
            }
            let wl = &held.levels[k];
            let (one, apart) = (one.ladder(), apart.ladder());
            identical &= alike(&one.levels[k], wl, c.0, c.1) && alike(&apart.levels[k], wl, c.0, c.1);
            for m in [&one, &apart] {
                let lv = &m.levels[k];
                for cc in 0..lv.cap() {
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

fn dump_bool(path: &str, v: impl Iterator<Item = bool>) {
    std::fs::write(path, v.map(|b| b as u8).collect::<Vec<u8>>()).expect("a dump");
}

/// A level's cells in rows along i, each along j, as the measures read them.
fn rows(lv: &Level) -> Vec<usize> {
    let mut out = Vec::with_capacity(lv.n * lv.n_j);
    for a in 0..lv.n as i64 {
        for b in 0..lv.n_j as i64 {
            out.push(lv.idx(lv.lo + a, lv.lo_j + b).expect("a cell the level holds"));
        }
    }
    out
}

fn dump(dir: &str, lv: &Level) {
    let p = |name: &str| format!("{dir}/{}_{name}.bin", lv.w);
    let order = rows(lv);
    let cap = lv.cap();
    let cells = |f: &dyn Fn(usize) -> f64| order.iter().map(|&c| f(c)).collect::<Vec<f64>>();
    let verts = |f: &dyn Fn(usize) -> f64| (0..3).flat_map(|t| order.iter().map(move |&c| t * cap + c)).map(f).collect::<Vec<f64>>();
    for (name, v) in [
        ("z", cells(&|c| lv.z[c])),
        ("zd", cells(&|c| lv.zd[c])),
        ("ground", cells(&|c| lv.ground[c])),
        ("cut", cells(&|c| lv.cut[c])),
        ("wgfloor", cells(&|c| lv.wgfloor[c])),
        ("wdist", cells(&|c| lv.wdist[c])),
        ("water", cells(&|c| lv.water[c])),
        ("wdepth", cells(&|c| lv.wdepth[c])),
        ("age", cells(&|c| lv.age[c])),
        ("p", cells(&|c| lv.p[c])),
        ("erod", cells(&|c| lv.erod[c])),
        ("base", verts(&|v| lv.base[v])),
        ("elev", verts(&|v| lv.elev[v])),
        ("psurf", verts(&|v| lv.psurf[v])),
        ("gelev", verts(&|v| lv.gelev[v])),
        ("acc", verts(&|v| lv.acc[v])),
        ("floor", verts(&|v| lv.floor[v])),
    ] {
        dump_f64(&p(name), v.into_iter());
    }
    for (l, layer) in lv.layers.iter().enumerate() {
        dump_f64(&p(&format!("layer{l}")), order.iter().map(|&c| layer[c]));
    }
    dump_f64(&p("cc"), order.iter().flat_map(|&c| lv.cc[c]));
    let vi = |f: &dyn Fn(usize) -> i64| (0..3).flat_map(|t| order.iter().map(move |&c| t * cap + c)).map(f).collect::<Vec<i64>>();
    // A vertex index as the measures number it: rows along i, each along j.
    let rowed = |v: i64| -> i64 {
        if v < 0 {
            return v;
        }
        let (t, i, j) = lv.vertex(v as usize);
        (t as i64 * lv.n as i64 + (i - lv.lo)) * lv.n_j as i64 + (j - lv.lo_j)
    };
    for (name, v) in [
        ("rec", vi(&|v| rowed(lv.rec[v]))),
        ("born", vi(&|v| lv.born[v])),
    ] {
        dump_i64(&p(name), v.into_iter());
    }
    dump_i64(&p("wborn"), order.iter().map(|&c| lv.wborn[c]));
    dump_i64(&p("wsrc"), order.iter().map(|&c| lv.wsrc[c]));
    dump_i64(&p("wwet"), order.iter().map(|&c| rowed(lv.wwet[c])));
    // A governing river read from a coarser level is a vertex of that level.
    dump_i64(&p("wgov"), order.iter().map(|&c| if lv.wsrc[c] == lv.w { rowed(lv.wgov[c]) } else { -2 }));
    let vb = |f: &dyn Fn(usize) -> bool| (0..3).flat_map(|t| order.iter().map(move |&c| t * cap + c)).map(f).collect::<Vec<bool>>();
    dump_bool(&p("trough"), vb(&|v| lv.trough[v]).into_iter());
    for (name, f) in [
        ("exists", &|c: usize| lv.exists[c] as u8 != 0),
        ("routed", &|c: usize| lv.routed[c]),
        ("built", &|c: usize| lv.built[c]),
    ] as [(&str, &dyn Fn(usize) -> bool); 3]
    {
        dump_bool(&p(name), order.iter().map(|&c| f(c)));
    }
}

/// Water at the level `w` wide against the valley from every river the
/// level routed, however far: over every built cell in the middle half of
/// its region, or a sample of 2000 where they are more, the height's
/// difference from the dry ground met by that valley, rounded, and the
/// cut's before rounding.
fn truth<G: Ground>(ladder: &Ladder<G>, w: i64) -> String {
    let k = index_of(ladder, w);
    let lv = &ladder.levels[k];
    let h = region(lv.w) / lv.w / 2;
    let (ci, cj) = haven_cell(lv.w);
    let mut cells: Vec<(i64, i64)> = (-h..=h)
        .flat_map(|a| (-h..=h).map(move |b| (ci + a, cj + b)))
        .filter(|&(i, j)| lv.idx(i, j).is_some_and(|x| lv.built[x]))
        .collect();
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
    let (mut steps, mut cuts, mut far) = (Vec::new(), Vec::new(), Vec::new());
    for (&(i, j), &t) in cells.iter().zip(&whole) {
        let c = lv.idx(i, j).unwrap();
        let dry = lv.ground[c];
        let off = (lv.z[c] - (dry + t + 0.5).floor()).abs();
        if off > 3.0 {
            far.push(format!(
                "   ({i}, {j}): {off} steps; cut {:.1}, every river's {t:.1}; governing river read at {}, {:.2} cells off\n",
                lv.cut[c], lv.wsrc[c], lv.wdist[c]
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
        "truth w={}: {} cells against min(dry ground, valley from every river): height p99 {} max {} steps; cut p99 {:.2} max {:.2}\n   cells over 3 steps: {}\n",
        lv.w,
        cells.len(),
        pct(&mut steps, 0.99),
        pct(&mut steps, 1.0),
        pct(&mut cuts, 0.99),
        pct(&mut cuts, 1.0),
        far.len()
    ) + &far.concat()
}

/// Water floating over its ground at each level: over the built cells of
/// its region, those whose water stands more than its bed's depth above the
/// cell's own height, the largest excess, and the ten nearest the haven.
fn floating<G: Ground>(ladder: &Ladder<G>) -> String {
    let mut out = String::from("floating water: w: wet cells; floating (water more than its bed's depth above the cell); largest excess in levels\n");
    for lv in &ladder.levels[1..] {
        let h = region(lv.w) / lv.w;
        let (ci, cj) = haven_cell(lv.w);
        let (mut wet, mut float, mut worst) = (0u64, Vec::new(), 0.0f64);
        for a in -h..=h {
            for b in -h..=h {
                let Some(c) = lv.idx(ci + a, cj + b).filter(|&c| lv.built[c] && !lv.water[c].is_nan()) else { continue };
                wet += 1;
                let excess = lv.water[c] - lv.z[c] - lv.wdepth[c];
                if excess > 0.0 {
                    worst = worst.max(excess);
                    float.push((a.abs().max(b.abs()).max((a + b).abs()), ci + a, cj + b, excess));
                }
            }
        }
        out.push_str(&format!("   {}: {wet}; {}; {worst}\n", lv.w, float.len()));
        float.sort_by_key(|f| f.0);
        for &(_, i, j, e) in float.iter().take(10) {
            out.push_str(&format!("      cell ({i}, {j}), tile ({}, {}): {e} over\n", i * lv.w, j * lv.w));
        }
    }
    out
}

/// Faces in each level's drawn surface over its region: of the edges
/// between two built cells, those where the corner rule leaves the two apart
/// at either end, and of the corners, those it splits.
fn faces<G: Ground>(ladder: &Ladder<G>) -> String {
    let mut out = String::from("faces in the drawn surface: w: edges with a face of edges; corners split of corners
");
    for lv in &ladder.levels[1..] {
        let h = region(lv.w) / lv.w;
        let (ci, cj) = haven_cell(lv.w);
        let z = |i: i64, j: i64| lv.idx(i, j).filter(|&c| lv.exists[c] && lv.built[c]).map_or(f64::NAN, |c| lv.z[c]);
        let met = |cells: [(i64, i64); 3]| meet(cells.map(|(i, j)| z(i, j)), lv.w as f64);
        // A corner's cells, and whether two of them part there.
        let apart = |cells: [(i64, i64); 3], x: usize, y: usize| {
            let m = met(cells);
            !m[x].is_nan() && !m[y].is_nan() && m[x] != m[y]
        };
        let (mut edges, mut faced, mut corners, mut split) = (0u64, 0u64, 0u64, 0u64);
        for a in -h..=h {
            for b in -h..=h {
                let (i, j) = (ci + a, cj + b);
                if z(i, j).is_nan() {
                    continue;
                }
                let ca = [(i, j), (i + 1, j), (i, j + 1)];
                let cb = [(i, j), (i + 1, j - 1), (i + 1, j)];
                for cells in [ca, cb] {
                    if cells.iter().all(|&(x, y)| !z(x, y).is_nan()) {
                        corners += 1;
                        let m = met(cells);
                        split += (m[0] != m[1] || m[1] != m[2]) as u64;
                    }
                }
                // Each edge forward of the cell, by the corners at its ends.
                let ends = [
                    ((i + 1, j), (ca, 0, 1), (cb, 0, 2)),
                    ((i, j + 1), (ca, 0, 2), ([(i - 1, j + 1), (i, j), (i, j + 1)], 1, 2)),
                    ((i + 1, j - 1), (cb, 0, 1), ([(i, j - 1), (i + 1, j - 1), (i, j)], 2, 1)),
                ];
                for ((x, y), (c0, x0, y0), (c1, x1, y1)) in ends {
                    if z(x, y).is_nan() {
                        continue;
                    }
                    edges += 1;
                    faced += (apart(c0, x0, y0) || apart(c1, x1, y1)) as u64;
                }
            }
        }
        out.push_str(&format!("   {}: {faced} of {edges}; {split} of {corners}
", lv.w));
    }
    out
}

fn run<G: Ground>(ground: &G, p: Params, dir: &str, log: &mut String) {
    let started = Instant::now();
    status("building the top");
    let terrain = Terrain::new_box(ground, &bounds(), p);
    log.push_str(&format!("top {:.1}s\n", started.elapsed().as_secs_f64()));
    let only: Option<i64> = std::env::var("LADDER_SMALL_REGION").ok().and_then(|s| s.parse().ok());
    let widths: Vec<i64> = FINE.iter().rev().copied().chain(COARSE[1..].iter().rev().map(|&(w, _)| w)).collect();
    for w in widths {
        let t = Instant::now();
        let cells = match only {
            Some(r) if w == 1 => (-r..=r).flat_map(|i| (-r..=r).map(move |j| (HAVEN.0 as i64 + i, HAVEN.1 as i64 + j))).collect(),
            Some(_) => continue,
            None => region_cells(w),
        };
        status(&format!("requesting level {w}"));
        terrain.cells(w, &cells);
        let line = format!("built w={w} {:.1}s\n", t.elapsed().as_secs_f64());
        print!("{line}");
        log.push_str(&line);
    }
    log.push_str(&format!("build_seconds {:.1}\n", started.elapsed().as_secs_f64()));
    {
        let ladder = terrain.ladder();
        for (k, lv) in ladder.levels.iter().enumerate() {
            log.push_str(&format!("stats w={} route={:?} cost={:?}\n", lv.w, ladder.route_stats[k], lv.cost));
        }
        log.push_str(&timing(&ladder.levels, &ladder.timing));
        for w in [243, 81, 27, 9, 3, 1] {
            status(&format!("checking {w} against every river"));
            log.push_str(&truth(&ladder, w));
        }
        log.push_str(&floating(&ladder));
        log.push_str(&faces(&ladder));
    }
    if std::env::var("LADDER_EXACT").is_ok_and(|v| v == "1") {
        exact(ground, &terrain, p, log);
    }
    status("writing the dumps");
    let ladder = terrain.ladder();
    for lv in &ladder.levels {
        dump(dir, lv);
    }
    let mut meta = String::from("w lo n layers lo_j n_j\n");
    for lv in &ladder.levels {
        meta.push_str(&format!("{} {} {} {} {} {}\n", lv.w, lv.lo, lv.n, lv.layers.len(), lv.lo_j, lv.n_j));
    }
    std::fs::write(format!("{dir}/levels.txt"), meta).expect("the levels");
}

#[test]
#[ignore]
fn ladder_gate() {
    let dir = std::env::var("LADDER_OUT").expect("LADDER_OUT names the directory to write to");
    std::fs::create_dir_all(&dir).expect("the output directory");
    let water = std::env::var("LADDER_WATER").map_or(true, |v| v != "0");
    let (cut_from, cut_full) = match std::env::var("LADDER_CUT").as_deref() {
        Ok("C") => (1_102_500.0, 13_230_000.0),
        Ok("A") => (1_500.0, 40_000.0),
        _ => (Params::default().cut_from, Params::default().cut_full),
    };
    let mut log = format!("ground stack, water {water}\n");
    let g = ground();
    let ring = std::env::var("LADDER_RING").ok().and_then(|r| r.parse().ok()).unwrap_or(world::ladder::RING);
    log.push_str(&format!("cut from {cut_from} full at {cut_full}, ring {ring}\n"));
    run(&g, Params { water, cut_from, cut_full, ring, ..Params::default() }, &dir, &mut log);
    print!("{log}");
    std::fs::write(format!("{dir}/log.txt"), log).expect("the log");
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
    let (na, nb) = (a.cap(), b.cap());
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
fn timing(levels: &[Level], times: &[[f64; 6]]) -> String {
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


/// The ladder's tiles beside the game's over 512 by 512 tiles in axial q
/// and r centred on the haven, rows by r then q, heights in levels as
/// little-endian i16: `ladder_z.bin` from a fresh ladder at the defaults;
/// `ladder_dry_z.bin`, the same with water off; `game_z.bin`, the same
/// tiles through `Composite::standard`, the game's path, which must
/// equal `ladder_z`; `stack_z.bin` from `Composite::standard`; and
/// `ladder_bed.bin`, a byte, 1 where water stands over a tile;
/// `ladder_water.bin`, its water's surface as i16, -32768 where none; and
/// `meta.json`.
#[test]
#[ignore]
fn ladder_export() {
    let dir = std::env::var("LADDER_OUT").expect("LADDER_OUT names the directory to write to");
    std::fs::create_dir_all(&dir).expect("the output directory");
    const SIDE: i64 = 512;
    let half = SIDE / 2;
    let g = ground();
    let bounds = bounds();
    let (hq, hr) = (HAVEN.0 as i64, HAVEN.1 as i64);
    let cells: Vec<(i64, i64)> = (-half..half).flat_map(|r| (-half..half).map(move |q| (hq + q, hr + r))).collect();
    let i16s = |v: &mut dyn Iterator<Item = f64>| -> Vec<u8> {
        v.flat_map(|z| {
            assert!(z.abs() < 32_767.0, "a height {z} past i16");
            (z as i16).to_le_bytes()
        })
        .collect()
    };
    status("the ladder's tiles");
    let started = Instant::now();
    let terrain = Terrain::new_box(&g, &bounds, Params::default());
    let tiles: Vec<Cell> = terrain.cells(1, &cells).into_iter().map(|c| c.expect("a tile the ladder holds")).collect();
    let ladder_s = started.elapsed().as_secs_f64();
    std::fs::write(format!("{dir}/ladder_z.bin"), i16s(&mut tiles.iter().map(|c| c.z))).expect("ladder_z");
    std::fs::write(format!("{dir}/ladder_bed.bin"), tiles.iter().map(|c| c.water.is_some() as u8).collect::<Vec<u8>>()).expect("ladder_bed");
    std::fs::write(
        format!("{dir}/ladder_water.bin"),
        tiles.iter().flat_map(|c| c.water.map_or(i16::MIN, |w| w as i16).to_le_bytes()).collect::<Vec<u8>>(),
    )
    .expect("ladder_water");
    status("the ladder's tiles with water off");
    let started = Instant::now();
    let dry = Terrain::new_box(&g, &bounds, Params { water: false, ..Params::default() });
    let dry_tiles: Vec<Cell> = dry.cells(1, &cells).into_iter().map(|c| c.expect("a tile the ladder holds")).collect();
    let dry_s = started.elapsed().as_secs_f64();
    std::fs::write(format!("{dir}/ladder_dry_z.bin"), i16s(&mut dry_tiles.iter().map(|c| c.z))).expect("ladder_dry_z");
    let higher = tiles.iter().zip(&dry_tiles).filter(|(w, d)| w.z > d.z).count();
    status("the game's tiles, laddered");
    let started = Instant::now();
    let game = Composite::standard(WORLD_SEED);
    let coords: Vec<(i32, i32)> = cells.iter().map(|&(q, r)| (q as i32, r as i32)).collect();
    game.warm(&coords);
    let game_z: Vec<f64> = coords.iter().map(|&(q, r)| game.elevation_at(q, r) as f64).collect();
    let game_s = started.elapsed().as_secs_f64();
    std::fs::write(format!("{dir}/game_z.bin"), i16s(&mut game_z.iter().copied())).expect("game_z");
    let off = tiles.iter().zip(&game_z).map(|(c, &z)| (c.z - z).abs()).fold(0.0, f64::max);
    status("the stack's tiles");
    let started = Instant::now();
    let stack = Composite::standard(WORLD_SEED);
    let stack_z: Vec<u8> = i16s(&mut coords.iter().map(|&(q, r)| stack.elevation_at(q, r) as f64));
    std::fs::write(format!("{dir}/stack_z.bin"), stack_z).expect("stack_z");
    let stack_s = started.elapsed().as_secs_f64();
    let meta = format!(
        "{{\n  \"haven\": {{\"q\": {}, \"r\": {}}},\n  \"origin\": {{\"q\": {}, \"r\": {}}},\n  \"size\": {{\"q\": {SIDE}, \"r\": {SIDE}}},\n  \"order\": \"rows by r, then q\",\n  \"rise\": {},\n  \"rise_unit\": \"a level's height in tile spacings, centre to centre\",\n  \"orientation\": \"flat-top\",\n  \"ladder_anchor\": {{\"q\": 0, \"r\": 0}}\n}}\n",
        HAVEN.0,
        HAVEN.1,
        hq - half,
        hr - half,
        world::RISE
    );
    std::fs::write(format!("{dir}/meta.json"), meta).expect("meta");
    let beds = tiles.iter().filter(|c| c.water.is_some()).count();
    let out = format!(
        "export: {SIDE} by {SIDE} tiles; ladder {ladder_s:.1} s, dry {dry_s:.1} s, game (laddered) {game_s:.1} s, stack {stack_s:.1} s; {beds} tiles under water; tiles higher wet than dry: {higher}; game against ladder, largest difference: {off}\n"
    );
    print!("{out}");
    std::fs::write(format!("{dir}/export.txt"), out).expect("the export log");
}

/// Where water raises ground: the ladder built wet and dry over the tiles'
/// region, and at each level, over the cells both built, how many stand
/// higher wet.
#[test]
#[ignore]
fn ladder_wet_dry() {
    let g = ground();
    let bounds = bounds();
    let h = region(1);
    let cells: Vec<(i64, i64)> = (-h..=h).flat_map(|i| (-h..=h).map(move |j| (HAVEN.0 as i64 + i, HAVEN.1 as i64 + j))).collect();
    status("the wet ladder");
    let wet = Terrain::new_box(&g, &bounds, Params::default());
    wet.cells(1, &cells);
    status("the dry ladder");
    let dry = Terrain::new_box(&g, &bounds, Params { water: false, ..Params::default() });
    dry.cells(1, &cells);
    let mut out = String::from("w: cells both built; higher wet\n");
    {
        let (a, b) = (wet.ladder(), dry.ladder());
        for k in 1..a.levels.len() {
            let (la, lb) = (&a.levels[k], &b.levels[k]);
            let (mut both, mut up) = (0, 0);
            for c in 0..la.cap() {
                if la.built[c] && lb.built[c] {
                    both += 1;
                    up += (la.z[c] > lb.z[c]) as u64;
                }
            }
            out.push_str(&format!("{}: {both}; {up}\n", la.w));
        }
    }
    print!("{out}");
    if let Ok(dir) = std::env::var("LADDER_OUT") {
        std::fs::write(format!("{dir}/wet_dry.txt"), out).expect("the wet-dry table");
    }
}

/// Where each band meets the next finer one: over the cells of each level
/// around the haven, the coarser cell's height against the finer cell
/// sharing its centre, in the coarser level's walkable span (its width in
/// steps), which the cross-level standard holds under one.
#[test]
#[ignore]
fn ladder_bands() {
    let g = ground();
    let terrain = Terrain::new_box(&g, &bounds(), Params::default());
    let mut out = String::from("coarse fine: pairs; |difference| / coarse span p50 p99 max (steps max)\n");
    let z_of = |w: i64, cells: &[(i64, i64)]| -> Vec<Option<f64>> { terrain.cells(w, cells).into_iter().map(|c| c.map(|c| c.z)).collect() };
    for (coarse, fine) in [(3i64, 1i64), (9, 3), (27, 9), (81, 27)] {
        status(&format!("bands {coarse} against {fine}"));
        let h = (region(1).max(coarse * 8)) / coarse;
        let (ci, cj) = haven_cell(coarse);
        let cells: Vec<(i64, i64)> = (-h..=h).flat_map(|a| (-h..=h).map(move |b| (ci + a, cj + b))).collect();
        let finer: Vec<(i64, i64)> = cells.iter().map(|&(i, j)| (3 * i, 3 * j)).collect();
        let (a, b) = (z_of(coarse, &cells), z_of(fine, &finer));
        let mut d: Vec<f64> = a.iter().zip(&b).filter_map(|(x, y)| Some((x.as_ref()? - y.as_ref()?).abs())).collect();
        d.sort_by(f64::total_cmp);
        let q = |f: f64| d[((d.len() - 1) as f64 * f) as usize] / coarse as f64;
        out.push_str(&format!("{coarse} {fine}: {}; {:.3} {:.3} {:.3} ({})\n", d.len(), q(0.5), q(0.99), q(1.0), d[d.len() - 1]));
    }
    print!("{out}");
    if let Ok(dir) = std::env::var("LADDER_OUT") {
        std::fs::write(format!("{dir}/bands.txt"), out).expect("the band table");
    }
}
