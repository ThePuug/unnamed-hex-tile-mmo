//! Where on the home continent the haven could stand: open ground, dry and
//! level, with a view. Each candidate on a coarse lattice over the
//! continent is read through the stack and scored by what a player
//! standing there sees: rays out in every bearing, each sample seen if it
//! rises above the horizon the nearer samples drew, and the best sector's
//! far land, the relief across it, and how far the ground falls away in
//! front. Prints the best, kept apart from each other.
//!
//! Run: `cargo test --release -p world --test haven_probe -- --ignored --nocapture`

use std::time::Instant;

use common::{Content, SITES};
use world::continents::{claim, nearest_site};
use world::events::Composite;
use world::tectonic::plate_at;
use world::{hex_to_world, WORLD_SEED};

/// The haven's tile, as `common_bevy::haven::HAVEN_LOCATION` holds it,
/// which names the continent searched.
const HAVEN: (i32, i32) = (107289, -7677);
/// How far the search reaches from it, and the lattice it walks.
const REACH: i32 = 45_000;
const STEP: i32 = 500;

/// Rays: this many bearings, sampled at distances growing by a half each
/// step from the first, out past the client's reach.
const BEARINGS: usize = 24;
const FIRST: f64 = 60.0;
const SAMPLES: usize = 14;
/// A view is the best run of this many neighbouring bearings: a third of
/// the round.
const SECTOR: usize = 8;
/// Land past this many tiles is the view; nearer is the foreground.
const FAR: f64 = 300.0;
/// The eye stands this many z-levels over the ground: the camera's boom.
const EYE: f64 = 15.0;
/// Open ground: trees on no more than this share of the tiles within
/// `OPEN_RADIUS`, and none on the tile or its ring.
const OPEN_SHARE: f64 = 0.1;
const OPEN_RADIUS: i32 = 40;

/// Hex direction for bearing `k` of `BEARINGS`, as fractional (q, r) per
/// tile of distance.
fn bearing(k: usize) -> (f64, f64) {
    let a = std::f64::consts::TAU * k as f64 / BEARINGS as f64;
    let (x, y) = (a.cos(), a.sin());
    // Flat-top axial: x = 1.5 q, y = √3 (r + q / 2), per unit tile size.
    let q = x / 1.5;
    let r = y / 3f64.sqrt() - q / 2.0;
    let len = ((q * q + r * r + q * r).abs()).sqrt();
    (q / len, r / len)
}

fn has_trees(c: &Composite, q: i32, r: i32) -> bool {
    let cover = c.tile_at(q, r).cover;
    (0..SITES.len()).any(|k| matches!(cover.growth(k), Content::Pine | Content::Deciduous))
}

struct Scored {
    q: i32,
    r: i32,
    z: f64,
    score: f64,
    far_seen: usize,
    relief: f64,
    drop: f64,
    facing: usize,
    trees: f64,
}

fn score(c: &Composite, q: i32, r: i32, home: &[world::tectonic::PlateId]) -> Option<Scored> {
    let (wx, wy) = hex_to_world(q, r);
    if !home.contains(&plate_at(wx, wy, WORLD_SEED).id) {
        return None;
    }
    let here = c.tile_at(q, r);
    if here.water.is_some() || here.elevation <= 1.0 {
        return None;
    }
    let z = here.elevation;
    // Level, dry and clear underfoot.
    let ring = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, -1), (-1, 1)];
    for (dq, dr) in ring {
        let t = c.tile_at(q + 3 * dq, r + 3 * dr);
        if (t.elevation - z).abs() > 3.0 || t.water.is_some() {
            return None;
        }
        if has_trees(c, q + dq, r + dr) {
            return None;
        }
    }
    if has_trees(c, q, r) {
        return None;
    }
    // Open: trees on few tiles about it, and no water near.
    let (mut seen, mut wooded) = (0.0, 0.0);
    for k in 0..12 {
        let (bq, br) = bearing(2 * k);
        for step in 1..=(OPEN_RADIUS / 4) {
            let d = (4 * step) as f64;
            let (tq, tr) = (q + (bq * d).round() as i32, r + (br * d).round() as i32);
            seen += 1.0;
            if has_trees(c, tq, tr) {
                wooded += 1.0;
            }
            if step == OPEN_RADIUS / 4 && c.tile_at(tq, tr).water.is_some() {
                return None;
            }
        }
    }
    let trees = wooded / seen;
    if trees > OPEN_SHARE {
        return None;
    }

    // Rays: what the eye sees in each bearing.
    let rise = common::grid::RISE as f64;
    let spacing = 3f64.sqrt();
    let eye = z + EYE;
    let mut per_bearing = Vec::with_capacity(BEARINGS);
    for k in 0..BEARINGS {
        let (bq, br) = bearing(k);
        let mut horizon = f64::NEG_INFINITY;
        let (mut far, mut low, mut high, mut near_drop, mut near_n) = (0usize, f64::INFINITY, f64::NEG_INFINITY, 0.0f64, 0.0f64);
        let mut d = FIRST;
        for _ in 0..SAMPLES {
            let (tq, tr) = (q + (bq * d).round() as i32, r + (br * d).round() as i32);
            let t = c.tile_at(tq, tr);
            let ground = t.water.unwrap_or(t.elevation).max(t.elevation);
            let angle = (ground - eye) * rise / (d * spacing);
            if d < FAR {
                near_drop += z - ground;
                near_n += 1.0;
            }
            if angle >= horizon {
                if d >= FAR {
                    far += 1;
                    low = low.min(ground);
                    high = high.max(ground);
                }
                horizon = angle;
            }
            d *= 1.5;
        }
        per_bearing.push((far, low, high, near_drop / near_n.max(1.0)));
    }
    let mut best: Option<Scored> = None;
    for start in 0..BEARINGS {
        let (mut far, mut low, mut high, mut drop) = (0usize, f64::INFINITY, f64::NEG_INFINITY, 0.0);
        for i in 0..SECTOR {
            let (f, l, h, n) = per_bearing[(start + i) % BEARINGS];
            far += f;
            low = low.min(l);
            high = high.max(h);
            drop += n;
        }
        let drop = drop / SECTOR as f64;
        let relief = if far > 0 { high - low } else { 0.0 };
        let s = far as f64 / (SECTOR * (SAMPLES - 5)) as f64 + (relief / 600.0).min(1.0) + (drop / 150.0).clamp(0.0, 1.0);
        if best.as_ref().map_or(true, |b| s > b.score) {
            best = Some(Scored { q, r, z, score: s, far_seen: far, relief, drop, facing: (start + SECTOR / 2) % BEARINGS, trees });
        }
    }
    best
}

#[test]
#[ignore]
fn where_the_haven_could_stand() {
    let c = Composite::standard(WORLD_SEED);
    let (hx, hy) = hex_to_world(HAVEN.0, HAVEN.1);
    let site = nearest_site(hx, hy, WORLD_SEED);
    let home = claim(site, WORLD_SEED).continent.clone();
    let started = Instant::now();

    let mut cells = Vec::new();
    let mut dq = -REACH;
    while dq <= REACH {
        let mut dr = -REACH;
        while dr <= REACH {
            cells.push((HAVEN.0 + dq, HAVEN.1 + dr));
            dr += STEP;
        }
        dq += STEP;
    }
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let chunk = cells.len().div_ceil(threads);
    let mut found: Vec<Scored> = std::thread::scope(|s| {
        let handles: Vec<_> = cells
            .chunks(chunk)
            .map(|part| {
                let (c, home) = (&c, &home);
                s.spawn(move || part.iter().filter_map(|&(q, r)| score(c, q, r, home)).collect::<Vec<_>>())
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().expect("a scoring thread")).collect()
    });
    println!("{} candidates of {} cells scored in {:.1}s", found.len(), cells.len(), started.elapsed().as_secs_f64());

    let old = score(&c, HAVEN.0, HAVEN.1, &home);
    match old {
        Some(s) => println!("the haven now: score {:.2}, far {}, relief {:.0}, drop {:.0}, trees {:.2}", s.score, s.far_seen, s.relief, s.drop, s.trees),
        None => println!("the haven now: not a candidate (wooded, wet or uneven)"),
    }

    found.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut kept: Vec<&Scored> = Vec::new();
    for s in &found {
        if kept.iter().all(|k| (k.q - s.q).abs().max((k.r - s.r).abs()) >= 3_000) {
            kept.push(s);
        }
        if kept.len() == 15 {
            break;
        }
    }
    for s in kept {
        println!(
            "({}, {}) z {:.0}: score {:.2}, far land seen {}, relief {:.0}, falls {:.0} in front, facing {}°, trees {:.0}%, {} tiles from the haven",
            s.q, s.r, s.z, s.score, s.far_seen, s.relief, s.drop,
            s.facing * 360 / BEARINGS, 100.0 * s.trees,
            (s.q - HAVEN.0).abs().max((s.r - HAVEN.1).abs()).max((s.q + s.r - HAVEN.0 - HAVEN.1).abs()),
        );
    }
}
