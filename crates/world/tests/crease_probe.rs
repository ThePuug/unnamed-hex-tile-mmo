//! Crease probe — where the ground about the haven breaks from a plane.
//! Along each hex axis a tile's bend is its neighbours' mean height less
//! its own; smooth ground bends by a few hundredths of a level, a seam by
//! much more. Prints how the bends are spread, how often a tile's true
//! height lies outside the half level about its own level, and a map of
//! the tiles that bend hard.
//!
//! Run: `cargo test -p world --test crease_probe -- --ignored --nocapture`

use world::events::Composite;
use world::WORLD_SEED;

const HAVEN: (i32, i32) = (107289, -7677);
const REACH: i32 = 60;
const AXES: [(i32, i32); 3] = [(1, 0), (0, 1), (1, -1)];

#[test]
#[ignore]
fn creases_about_the_haven() {
    let c = Composite::standard(WORLD_SEED);
    let n = (2 * REACH + 3) as usize;
    let at = |q: i32, r: i32| ((r + REACH + 1) as usize) * n + (q + REACH + 1) as usize;
    let mut height = vec![0.0f64; n * n];
    let mut level = vec![0i32; n * n];
    let mut outside = 0usize;
    let mut no_fine = 0usize;
    for r in -REACH - 1..=REACH + 1 {
        for q in -REACH - 1..=REACH + 1 {
            let view = c.tile_at(HAVEN.0 + q, HAVEN.1 + r);
            let z = view.elevation.round() as i32;
            match view.fine {
                Some(f) if (f - z as f64).abs() > 0.5 + 1e-9 => outside += 1,
                None => no_fine += 1,
                _ => {}
            }
            height[at(q, r)] = c.ground_at(HAVEN.0 + q, HAVEN.1 + r).height();
            level[at(q, r)] = z;
        }
    }
    println!("\n=== CREASES, {} tiles about the haven ===", n * n);
    println!("true height outside its level's half step: {outside}; tiles with no true height: {no_fine}");

    let mut bends = Vec::new();
    let mut worst = vec![0.0f64; n * n];
    for r in -REACH..=REACH {
        for q in -REACH..=REACH {
            for (a, (dq, dr)) in AXES.iter().enumerate() {
                let (lo, hi) = (at(q - dq, r - dr), at(q + dq, r + dr));
                // A wall on either side is no crease.
                if (level[lo] - level[at(q, r)]).abs() > 1 || (level[hi] - level[at(q, r)]).abs() > 1 {
                    continue;
                }
                let bend = (height[lo] + height[hi]) / 2.0 - height[at(q, r)];
                bends.push((bend.abs(), q, r, a));
                worst[at(q, r)] = worst[at(q, r)].max(bend.abs());
            }
        }
    }
    bends.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let pick = |f: f64| bends[((bends.len() - 1) as f64 * f) as usize].0;
    println!("bend in levels: median {:.3}, 90% {:.3}, 99% {:.3}, 99.9% {:.3}, most {:.3}", pick(0.5), pick(0.9), pick(0.99), pick(0.999), pick(1.0));
    for limit in [0.1, 0.2, 0.3, 0.5] {
        println!("  bends over {limit}: {}", bends.iter().filter(|b| b.0 > limit).count());
    }
    println!("the hardest bends: bend, tile offset from the haven, axis, world q r and each modulo 3, 9, 27");
    for &(bend, q, r, a) in bends.iter().rev().take(16) {
        let (wq, wr) = (HAVEN.0 + q, HAVEN.1 + r);
        println!(
            "  {bend:.3} at ({q:>4},{r:>4}) axis {a}  world ({wq},{wr})  mod 3 ({},{})  mod 9 ({},{})  mod 27 ({},{})",
            wq.rem_euclid(3), wr.rem_euclid(3), wq.rem_euclid(9), wr.rem_euclid(9), wq.rem_euclid(27), wr.rem_euclid(27)
        );
    }
    println!("profiles along the axis through a spread of bends, heights from the middle tile, two tiles either side:");
    let count = bends.len();
    for k in [count - 1, count - 2, count - 6, count - 40, count - 200, count - 600, count - 1200, count - 2500, count - 4000] {
        let (bend, q, r, a) = bends[k];
        if q.abs() > REACH - 2 || r.abs() > REACH - 2 { continue }
        let (dq, dr) = AXES[a];
        let row: Vec<String> = (-2..=2).map(|t| format!("{:>6.2}", height[at(q + dq * t, r + dr * t)] - height[at(q, r)])).collect();
        println!("  bend {bend:.3} at ({q:>4},{r:>4}) axis {a}: {}", row.join(" "));
    }
    println!("map, q across and r down: '#' bends over 0.3, '+' over 0.15, '.' less");
    for r in -REACH..=REACH {
        let row: String = (-REACH..=REACH)
            .map(|q| match worst[at(q, r)] {
                b if b > 0.3 => '#',
                b if b > 0.15 => '+',
                _ => '.',
            })
            .collect();
        println!("{row}");
    }
}
