//! What the sea would be were it its plate's and not its height's: the
//! ground under a site's top sampled on a hex grid, each sample below the
//! sea told apart by the plate it stands on and by whether the sea reaches
//! it. Ocean below the sea is the sea. Continental ground below the sea is
//! a bay where samples below the sea join it to the ocean, and a basin the
//! sea cannot reach where none do: ground the ladder floods as sea today
//! (`top::route_top`'s sinks) that would be dry or a lake. Ocean above the
//! sea is land standing on an oceanic plate.
//!
//! The ground is the ladder's own before water cuts it
//! (`ladder_ground::StackGround`), and no river grades below the sea, so
//! what stands below it here does after the cut. A basin narrower than the
//! spacing is not seen.
//!
//! `SEA_SITE="i,j"` names the site, the haven's by default; `SEA_STEP` the
//! spacing in tiles, 81 by default.
//!
//! Run: `cargo test -p world --features inspect --test sea_probe -- --ignored --nocapture`

use std::collections::HashMap;
use std::time::Instant;

use world::continents;
use world::events::ladder_ground::{top_bounds, StackGround};
use world::events::thrusting::Outlines;
use world::ladder::Ground;
use world::lattice::DIRECTIONS;
use world::{hex_to_world, world_to_hex, WORLD_SEED};

const HAVEN: (i32, i32) = (107289, -7677);

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Land,
    /// Ocean below the sea.
    Sea,
    /// Ocean at or above the sea.
    OceanLand,
    /// Continental ground below the sea.
    Low,
}

#[test]
#[ignore]
fn sea_probe() {
    let started = Instant::now();
    let (hx, hy) = hex_to_world(HAVEN.0, HAVEN.1);
    let site = match std::env::var("SEA_SITE") {
        Ok(s) => {
            let (i, j) = s.split_once(',').expect("a site is i,j");
            (i.trim().parse().expect("a whole i"), j.trim().parse().expect("a whole j"))
        }
        Err(_) => continents::land_site(hx, hy, WORLD_SEED).expect("the haven stands on land"),
    };
    let step: i64 = std::env::var("SEA_STEP").ok().map_or(81, |s| s.parse().expect("a whole spacing"));
    let (w, lo, hi) = top_bounds(WORLD_SEED, site).expect("a site with land");
    let corners = [(lo.0, lo.1), (hi.0, lo.1), (lo.0, hi.1), (hi.0, hi.1)].map(|(i, j)| hex_to_world((i * w) as i32, (j * w) as i32));
    let (x0, x1) = corners.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |a, c| (a.0.min(c.0), a.1.max(c.0)));
    let (y0, y1) = corners.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |a, c| (a.0.min(c.1), a.1.max(c.1)));
    let centre = world_to_hex((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let half = (x1 - x0).max(y1 - y0) / 2.0 + w as f64;
    let ground = StackGround::new(WORLD_SEED, centre, half);
    let outlines = Outlines::in_box((x0 + x1) / 2.0, (y0 + y1) / 2.0, half, WORLD_SEED);

    let (ni, nj) = (((hi.0 - lo.0) * w / step) as usize, ((hi.1 - lo.1) * w / step) as usize);
    let tile = |a: usize, b: usize| (lo.0 * w + a as i64 * step, lo.1 * w + b as i64 * step);
    println!("site {site:?}: {ni} by {nj} samples {step} tiles apart");
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let rows: Vec<usize> = (0..ni).collect();
    let (ground, outlines) = (&ground, &outlines);
    let read: Vec<(Kind, f64)> = std::thread::scope(|s| {
        let handles: Vec<_> = rows
            .chunks(ni.div_ceil(threads).max(1))
            .map(|part| {
                s.spawn(move || {
                    part.iter()
                        .flat_map(|&a| (0..nj).map(move |b| (a, b)))
                        .map(|(a, b)| {
                            let (q, r) = tile(a, b);
                            let z = ground.height(q, r);
                            let (wx, wy) = hex_to_world(q as i32, r as i32);
                            // Past every outline is open ocean.
                            let ocean = outlines.at(wx, wy).is_none_or(|at| !at.plate.continental);
                            let kind = match (ocean, z < 0.0) {
                                (true, true) => Kind::Sea,
                                (true, false) => Kind::OceanLand,
                                (false, true) => Kind::Low,
                                (false, false) => Kind::Land,
                            };
                            (kind, z)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().expect("a reading thread")).collect()
    });
    println!("progress {:.0}s: ground read", started.elapsed().as_secs_f64());

    let at = |a: usize, b: usize| a * nj + b;
    let around = |x: usize| {
        let (a, b) = ((x / nj) as i64, (x % nj) as i64);
        DIRECTIONS.iter().filter_map(move |&(da, db)| {
            let (a, b) = (a + da as i64, b + db as i64);
            (a >= 0 && b >= 0 && (a as usize) < ni && (b as usize) < nj).then(|| at(a as usize, b as usize))
        })
    };
    // The sea reaches in from the ocean over ground below it.
    let mut reached = vec![false; read.len()];
    let mut queue: Vec<usize> = (0..read.len()).filter(|&x| read[x].0 == Kind::Sea).collect();
    while let Some(x) = queue.pop() {
        for y in around(x) {
            if read[y].0 == Kind::Low && !reached[y] {
                reached[y] = true;
                queue.push(y);
            }
        }
    }
    // The basins it cannot reach, each the samples joined to one another.
    let mut basin = vec![usize::MAX; read.len()];
    let mut basins: Vec<(usize, f64, usize)> = Vec::new();
    for x in 0..read.len() {
        if read[x].0 != Kind::Low || reached[x] || basin[x] != usize::MAX {
            continue;
        }
        let id = basins.len();
        let (mut size, mut deepest, mut lowest) = (0, f64::INFINITY, x);
        let mut queue = vec![x];
        basin[x] = id;
        while let Some(y) = queue.pop() {
            size += 1;
            if read[y].1 < deepest {
                (deepest, lowest) = (read[y].1, y);
            }
            for n in around(y) {
                if read[n].0 == Kind::Low && !reached[n] && basin[n] == usize::MAX {
                    basin[n] = id;
                    queue.push(n);
                }
            }
        }
        basins.push((size, deepest, lowest));
    }

    let mut counts: HashMap<&str, usize> = HashMap::new();
    for (x, (kind, _)) in read.iter().enumerate() {
        let name = match kind {
            Kind::Land => "continental land",
            Kind::Sea => "ocean below the sea",
            Kind::OceanLand => "ocean at or above the sea",
            Kind::Low if reached[x] => "continental below the sea, a bay the sea reaches",
            Kind::Low => "continental below the sea, a basin it cannot reach",
        };
        *counts.entry(name).or_default() += 1;
    }
    let tiles = (step * step) as f64;
    let mut names: Vec<_> = counts.into_iter().collect();
    names.sort();
    for (name, n) in names {
        println!("{n:>9} samples, {:>12.0} tiles, {:>6.2}%: {name}", n as f64 * tiles, 100.0 * n as f64 / read.len() as f64);
    }
    basins.sort_by(|a, b| b.0.cmp(&a.0));
    println!("{} basins the sea cannot reach; the widest:", basins.len());
    for (size, deepest, lowest) in basins.iter().take(12) {
        let (q, r) = tile(lowest / nj, lowest % nj);
        println!("  {size:>6} samples, {:>10.0} tiles, lowest {deepest:>7.1} at tile ({q}, {r})", *size as f64 * tiles);
    }
    println!("done in {:.0}s", started.elapsed().as_secs_f64());
}

/// The ground with every plate the ocean's: the sea by height alone, as
/// routing took it before it asked the plate.
struct ByHeight<'a>(&'a StackGround, bool);

impl Ground for ByHeight<'_> {
    fn at(&self, q: i64, r: i64) -> world::ladder::Point {
        let point = self.0.at(q, r);
        world::ladder::Point { ocean: self.1 || point.ocean, ..point }
    }
    fn height(&self, q: i64, r: i64) -> f64 {
        self.0.height(q, r)
    }
    fn ocean(&self, q: i64, r: i64) -> bool {
        self.1 || self.0.ocean(q, r)
    }
}

/// What taking the sea from the plate changes: a site's ladder built twice
/// over one ground, the sea its plate's and the sea every ground below it,
/// and every cell of one level set beside its twin. `SEA_LEVEL` is the
/// level's width, 81 by default.
///
/// Run: `cargo test -p world --features inspect --test sea_probe sea_change -- --ignored --nocapture`
#[test]
#[ignore]
fn sea_change() {
    use world::events::ladder_ground::landmass_bounds;
    use world::ladder::terrain::Terrain;
    use world::ladder::{Ladder, Params};

    let started = Instant::now();
    let (hx, hy) = hex_to_world(HAVEN.0, HAVEN.1);
    let site = continents::land_site(hx, hy, WORLD_SEED).expect("the haven stands on land");
    let level: i64 = std::env::var("SEA_LEVEL").ok().map_or(81, |s| s.parse().expect("a level's width"));
    let top_box = top_bounds(WORLD_SEED, site).expect("a site with land");
    let (w, lo, hi) = top_box;
    let corners = [(lo.0, lo.1), (hi.0, lo.1), (lo.0, hi.1), (hi.0, hi.1)].map(|(i, j)| hex_to_world((i * w) as i32, (j * w) as i32));
    let (x0, x1) = corners.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |a, c| (a.0.min(c.0), a.1.max(c.0)));
    let (y0, y1) = corners.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |a, c| (a.0.min(c.1), a.1.max(c.1)));
    let ground = StackGround::new(WORLD_SEED, world_to_hex((x0 + x1) / 2.0, (y0 + y1) / 2.0), (x1 - x0).max(y1 - y0) / 2.0 + w as f64);
    // `SEA_SAME=1` builds the plate's rule twice, to show two builds agree.
    let old_ground = ByHeight(&ground, std::env::var("SEA_SAME").is_err());
    let p = Params::default();

    let top = Ladder::top(&ground, top_box, p);
    let bounds = landmass_bounds(&top, WORLD_SEED, site);
    let plate = Terrain::from_ladder(Ladder::with_top(&ground, &bounds, p, Some(top)));
    let top = Ladder::top(&old_ground, top_box, p);
    let height = Terrain::from_ladder(Ladder::with_top(&old_ground, &bounds, p, Some(top)));
    println!("progress {:.0}s: both tops built", started.elapsed().as_secs_f64());
    // `SEA_TOP=1` sets the two tops' routing side by side, vertex by vertex.
    if std::env::var("SEA_TOP").is_ok() {
        let (a, b) = (&plate.ladder().levels[0], &height.ladder().levels[0]);
        let nn = a.cap();
        let mut n = [0usize; 6];
        let mut shown = 0;
        for v in (0..3 * nn).filter(|&v| a.exists[v % nn]) {
            let same = |x: f64, y: f64| x == y || (x.is_nan() && y.is_nan());
            let diff = [a.rec.get(v) != b.rec.get(v), !same(a.acc.get(v), b.acc.get(v)), !same(a.graded.get(v), b.graded.get(v)), !same(a.floor.get(v), b.floor.get(v)), a.trough.get(v) != b.trough.get(v), !same(a.z.get(v), b.z.get(v))];
            for (k, d) in diff.iter().enumerate() {
                n[k] += *d as usize;
            }
            if diff[2] && a.z.get(v) >= 0.0 && shown < 40 && (a.graded.get(v) - b.graded.get(v)).abs() > 20.0 || (diff[2] && a.graded.get(v).is_nan() != b.graded.get(v).is_nan() && shown < 40) {
                shown += 1;
                let (t, i, j) = a.vertex(v);
                let (q3, r3) = world::ladder::geom::pos3(t, i, j, a.w);
                println!("  tile ({}, {}) z {} sea {} rec {} / {} acc {:.0} / {:.0} graded {:.1} / {:.1} floor {} / {}", q3 / 3, r3 / 3, a.z.get(v), a.sea(v), a.rec.get(v), b.rec.get(v), a.acc.get(v), b.acc.get(v), a.graded.get(v), b.graded.get(v), a.floor.get(v), b.floor.get(v));
            }
        }
        println!("top vertices differing in rec, acc, graded, floor, trough, z: {n:?}");
        return;
    }

    // `SEA_VIEW="q,r,half"` writes the level's cells `half` tiles either
    // way of a tile under both rules to `SEA_OUT`, a line a cell: its
    // index, then each rule's height and water, for a picture.
    if let Ok(view) = std::env::var("SEA_VIEW") {
        let n: Vec<i64> = view.split(',').map(|s| s.trim().parse().expect("a whole number")).collect();
        let (ci, cj, half) = (n[0].div_euclid(level), n[1].div_euclid(level), n[2] / level);
        let at: Vec<(i64, i64)> = (-half..=half).flat_map(|a| (-half..=half).map(move |b| (ci + a, cj + b))).collect();
        let (a, b) = (plate.cells(level, &at), height.cells(level, &at));
        let mut out = String::new();
        for ((&(i, j), a), b) in at.iter().zip(a).zip(b) {
            let (Some(a), Some(b)) = (a, b) else { continue };
            let water = |w: Option<f64>| w.map_or("nan".to_string(), |w| w.to_string());
            out += &format!("{i} {j} {} {} {} {}
", a.z, water(a.water), b.z, water(b.water));
        }
        std::fs::write(std::env::var("SEA_OUT").expect("SEA_OUT names the file"), out).expect("the view written");
        println!("view written in {:.0}s", started.elapsed().as_secs_f64());
        return;
    }
    let &(_, lo, hi) = bounds.iter().find(|b| b.0 == level).expect("a level of the ladder");
    let rows: Vec<i64> = (lo.0..hi.0).collect();
    let (mut cells, mut land, mut moved, mut wetter, mut drier, mut most) = (0usize, 0usize, 0usize, 0usize, 0usize, 0.0f64);
    let mut steps = [0usize; 5];
    let mut by_top: HashMap<(i64, i64), usize> = HashMap::new();
    let batch = (20_000 / (hi.1 - lo.1).max(1)).max(1) as usize;
    for (n, part) in rows.chunks(batch).enumerate() {
        let at: Vec<(i64, i64)> = part.iter().flat_map(|&i| (lo.1..hi.1).map(move |j| (i, j))).collect();
        let (a, b) = (plate.cells(level, &at), height.cells(level, &at));
        for ((&(i, j), a), b) in at.iter().zip(a).zip(b) {
            let (Some(a), Some(b)) = (a, b) else { continue };
            cells += 1;
            if a.z < 0.0 && b.z < 0.0 && a.water.is_none() && b.water.is_none() {
                continue;
            }
            land += 1;
            let dz = (a.z - b.z).abs();
            let changed = dz > 0.0 || a.water != b.water;
            if dz > 0.0 {
                moved += 1;
                most = most.max(dz);
                steps[(dz as usize).min(5) - 1] += 1;
            }
            wetter += (a.water.is_some() && b.water.is_none()) as usize;
            drier += (a.water.is_none() && b.water.is_some()) as usize;
            if changed {
                *by_top.entry((i * level / 729, j * level / 729)).or_default() += 1;
            }
        }
        if n % 10 == 0 {
            println!("progress {:.0}s: {} of {} rows", started.elapsed().as_secs_f64(), (n + 1) * batch, rows.len());
        }
    }
    println!("level {level}: {cells} cells, {land} of them land or water");
    println!("  ground moved in {moved} ({:.3}% of those), the most {most} steps; by 1, 2, 3, 4, 5 or more: {steps:?}", 100.0 * moved as f64 / land as f64);
    println!("  water where there was none: {wetter}; none where there was water: {drier}");
    let mut places: Vec<_> = by_top.into_iter().collect();
    places.sort_by(|a, b| b.1.cmp(&a.1));
    println!("  changed cells by the 729-wide square of tiles they lie in, {} squares, the most changed:", places.len());
    for ((a, b), n) in places.iter().take(15) {
        println!("    {n:>6} cells near tile ({}, {})", a * 729, b * 729);
    }
    println!("done in {:.0}s", started.elapsed().as_secs_f64());
}

/// The shore nearest a tile: the nearest tile of the sea, sought on a
/// coarse grid and then tile by tile round the nearest found, and the land
/// tile beside it. `SEA_FROM="q,r"` names the tile, the haven by default.
///
/// Run: `cargo test -p world --features inspect --test sea_probe nearest_shore -- --ignored --nocapture`
#[test]
#[ignore]
fn nearest_shore() {
    let from: (i64, i64) = match std::env::var("SEA_FROM") {
        Ok(s) => {
            let (q, r) = s.split_once(',').expect("a tile is q,r");
            (q.trim().parse().expect("a whole q"), r.trim().parse().expect("a whole r"))
        }
        Err(_) => (HAVEN.0 as i64, HAVEN.1 as i64),
    };
    let (fx, fy) = hex_to_world(from.0 as i32, from.1 as i32);
    let ground = StackGround::new(WORLD_SEED, (from.0 as i32, from.1 as i32), 20_000.0);
    let sea = |q: i64, r: i64| ground.height(q, r) < 0.0 && ground.ocean(q, r);
    let far = |q: i64, r: i64| {
        let (x, y) = hex_to_world(q as i32, r as i32);
        (x - fx).hypot(y - fy)
    };
    let nearest = |tiles: &mut dyn Iterator<Item = (i64, i64)>| tiles.filter(|&(q, r)| sea(q, r)).min_by(|a, b| far(a.0, a.1).total_cmp(&far(b.0, b.1)));
    let (step, reach) = (27i64, 12_000i64 / 27);
    let coarse = nearest(&mut (-reach..=reach).flat_map(|a| (-reach..=reach).map(move |b| (from.0 + a * step, from.1 + b * step)))).expect("sea within reach");
    // The nearest sea tile lies no further than the coarse one, and within
    // a coarse step of some sample nearer than it: round the coarse find.
    let (cq, cr) = coarse;
    let fine = nearest(&mut (-2 * step..=2 * step).flat_map(|a| (-2 * step..=2 * step).map(move |b| (cq + a, cr + b)))).expect("the coarse find is sea");
    let land = DIRECTIONS
        .iter()
        .map(|&(dq, dr)| (fine.0 + dq as i64, fine.1 + dr as i64))
        .filter(|&(q, r)| ground.height(q, r) >= 0.0)
        .min_by(|a, b| far(a.0, a.1).total_cmp(&far(b.0, b.1)));
    println!("from tile {from:?}: the nearest sea tile found is {fine:?}, {:.0} tiles off; the land beside it {land:?}, standing {:.1}", far(fine.0, fine.1), land.map_or(f64::NAN, |(q, r)| ground.height(q, r)));
}

/// The ground's height along a line of tiles through `SEA_FROM`, a tile to
/// a column and three rows of them: how steeply it crosses the waterline.
///
/// Run: `SEA_FROM=q,r cargo test -p world --features inspect --test sea_probe shore_profile -- --ignored --nocapture`
#[test]
#[ignore]
fn shore_profile() {
    let spec = std::env::var("SEA_FROM").expect("SEA_FROM names the tile");
    let (q, r) = spec.split_once(',').expect("a tile is q,r");
    let from: (i64, i64) = (q.trim().parse().expect("a whole q"), r.trim().parse().expect("a whole r"));
    let ground = StackGround::new(WORLD_SEED, (from.0 as i32, from.1 as i32), 4_000.0);
    for (name, (dq, dr)) in [("along q", (1i64, 0i64)), ("along r", (0, 1)), ("along q - r", (1, -1))] {
        let heights: Vec<String> = (-40..=40).map(|k| format!("{:.2}", ground.height(from.0 + k * dq, from.1 + k * dr))).collect();
        println!("{name}: {}", heights.join(" "));
    }
}

/// Every coast within `SEA_REACH` world units of `SEA_FROM`, 40,000 and the
/// haven by default: where its middle lies, how long it is, how hard its
/// margin closes, its continent's plate's age, and the shelf those give it.
///
/// Run: `cargo test -p world --features inspect --test sea_probe coast_kinds -- --ignored --nocapture`
#[test]
#[ignore]
fn coast_kinds() {
    use world::events::motion::resolve;
    use world::events::thrusting::CONVERGENCE_FULL;
    use world::tectonic::{edges_of, plates_near};
    let from: (i32, i32) = match std::env::var("SEA_FROM") {
        Ok(s) => {
            let (q, r) = s.split_once(',').expect("a tile is q,r");
            (q.trim().parse().expect("a whole q"), r.trim().parse().expect("a whole r"))
        }
        Err(_) => HAVEN,
    };
    let reach: f64 = std::env::var("SEA_REACH").ok().map_or(40_000.0, |s| s.parse().expect("a reach"));
    let (cx, cy) = hex_to_world(from.0, from.1);
    let mut seen = std::collections::HashSet::new();
    let mut coasts = Vec::new();
    for plate in plates_near(cx, cy, reach, WORLD_SEED) {
        for edge in edges_of(plate.id, WORLD_SEED) {
            if edge.is_coast() && seen.insert(edge.ids()) {
                let (mx, my) = edge.mid();
                if (mx - cx).hypot(my - cy) <= reach {
                    coasts.push(edge);
                }
            }
        }
    }
    coasts.sort_by(|a, b| a.mid().0.total_cmp(&b.mid().0));
    println!("{} coasts within {reach} of tile {from:?}", coasts.len());
    for edge in &coasts {
        let land = if edge.a.continental { &edge.a } else { &edge.b };
        let closing = resolve(edge, WORLD_SEED).convergence / CONVERGENCE_FULL;
        let shelf = world::Shelf::of(closing, land.age);
        let (q, r) = world_to_hex(edge.mid().0, edge.mid().1);
        println!(
            "  near tile ({q}, {r}), {:>5.0} long: closing {closing:>5.2}, plate age {:.2} -> shoreface {:>3.0}, shelf {:>5.0} wide",
            edge.length(),
            land.age,
            shelf.shoreface,
            shelf.width
        );
    }
}

/// The ground's height out from `SEA_FROM` along each of the six ways
/// across the tiles, every `SEA_STEP` tiles (60 by default) for thirty
/// steps: where a sea floor's shelf ends and how deep it lies there.
///
/// Run: `SEA_FROM=q,r cargo test -p world --features inspect --test sea_probe floor_out -- --ignored --nocapture`
#[test]
#[ignore]
fn floor_out() {
    let spec = std::env::var("SEA_FROM").expect("SEA_FROM names the tile");
    let (q, r) = spec.split_once(',').expect("a tile is q,r");
    let from: (i64, i64) = (q.trim().parse().expect("a whole q"), r.trim().parse().expect("a whole r"));
    let step: i64 = std::env::var("SEA_STEP").ok().map_or(60, |s| s.parse().expect("a whole step"));
    let ground = StackGround::new(WORLD_SEED, (from.0 as i32, from.1 as i32), 8_000.0);
    println!("at {from:?}: {:.1}; every {step} tiles", ground.height(from.0, from.1));
    for (dq, dr) in DIRECTIONS {
        let heights: Vec<String> = (1..=30).map(|k| format!("{:.0}", ground.height(from.0 + k * step * dq as i64, from.1 + k * step * dr as i64))).collect();
        println!("  toward ({dq:>2}, {dr:>2}): {}", heights.join(" "));
    }
}

/// The ground as the game builds it, through the whole stack, out from
/// `SEA_FROM` along each of the six ways every `SEA_STEP` tiles (2 by
/// default) for sixty steps, beside the stack's ground under the ladder:
/// where the two part, the ladder moved the sea floor.
///
/// Run: `SEA_FROM=q,r cargo test -p world --features inspect --test sea_probe floor_built -- --ignored --nocapture`
#[test]
#[ignore]
fn floor_built() {
    let spec = std::env::var("SEA_FROM").expect("SEA_FROM names the tile");
    let (q, r) = spec.split_once(',').expect("a tile is q,r");
    let from: (i32, i32) = (q.trim().parse().expect("a whole q"), r.trim().parse().expect("a whole r"));
    let step: i32 = std::env::var("SEA_STEP").ok().map_or(2, |s| s.parse().expect("a whole step"));
    let world = world::events::Composite::standard(WORLD_SEED);
    let under = StackGround::new(WORLD_SEED, from, 8_000.0);
    for (dq, dr) in DIRECTIONS {
        let at = |k: i32| (from.0 + k * step * dq, from.1 + k * step * dr);
        let built: Vec<String> = (0..60).map(|k| format!("{:.0}", world.tile_at(at(k).0, at(k).1).elevation)).collect();
        let stack: Vec<String> = (0..60).map(|k| format!("{:.0}", under.height(at(k).0 as i64, at(k).1 as i64))).collect();
        println!("toward ({dq:>2}, {dr:>2}) built: {}\n               stack: {}", built.join(" "), stack.join(" "));
    }
}
