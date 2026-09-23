//! How well the grade the stack reports agrees with the slope between a
//! tile's neighbours, on a range front, a plateau and the valleys cut in
//! them.
//!
//! Run: `cargo test --release -p world --test grade_probe -- --ignored --nocapture`

use world::events::Composite;

const SEED: u64 = 0x9E3779B97F4A7C15;

/// The three axes of a flat-top hex, as neighbour offsets, and the world
/// step each spans.
const AXES: [(i32, i32); 3] = [(1, 0), (0, 1), (-1, 1)];

#[test]
#[ignore]
fn grade_against_neighbours() {
    let c = Composite::standard(SEED);
    for (name, (q0, r0)) in [("belt", (99_155, -2_309)), ("spawn", (104_289, -4_677))] {
        let (mut n, mut steep, mut sum_err, mut worst) = (0, 0, 0.0, 0.0f64);
        let mut buckets = [(0usize, 0.0f64, 0.0f64); 6];
        for dq in (-300..=300).step_by(7) {
            for dr in (-300..=300).step_by(7) {
                let (q, r) = (q0 + dq, r0 + dr);
                let v = c.tile_at(q, r);
                if v.water.is_some() {
                    continue;
                }
                // The steepest of the three axes' central differences,
                // against the reported gradient's reading along it.
                let mut fd: f64 = 0.0;
                let mut along: f64 = 0.0;
                for (aq, ar) in AXES {
                    let (x0, y0) = world::hex_to_world(0, 0);
                    let (x1, y1) = world::hex_to_world(aq, ar);
                    let (ux, uy) = (x1 - x0, y1 - y0);
                    let d = (c.tile_at(q + aq, r + ar).elevation - c.tile_at(q - aq, r - ar).elevation) / 2.0;
                    if d.abs() > fd.abs() {
                        fd = d;
                        along = v.gradient.0 * ux + v.gradient.1 * uy;
                    }
                }
                n += 1;
                let b = (fd.abs() / 0.5).min(5.0) as usize;
                buckets[b].0 += 1;
                buckets[b].1 += v.grade();
                buckets[b].2 += (along - fd).abs();
                if fd.abs() > 1.0 {
                    steep += 1;
                    sum_err += (along - fd).abs() / fd.abs();
                    worst = worst.max((along - fd).abs() / fd.abs());
                }
            }
        }
        println!("{name}: {n} land tiles, {steep} steeper than a step; relative error there mean {:.2}, worst {:.2}", sum_err / steep.max(1) as f64, worst);
        for (i, (k, g, e)) in buckets.iter().enumerate() {
            println!("  neighbour grade {:.1}..{:.1}: {k} tiles, mean reported grade {:.2}, mean error along {:.2}", i as f64 * 0.5, i as f64 * 0.5 + 0.5, g / (*k).max(1) as f64, e / (*k).max(1) as f64);
        }
    }
}
