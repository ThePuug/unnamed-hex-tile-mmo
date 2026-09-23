//! A census of the rock that shows: how much land carries boulders, how
//! much of it is closed by them, and on what rock, about the haven and on
//! the home continent's belt.
//!
//! Run: `cargo test --release -p world --test outcrop_probe -- --ignored --nocapture`

use common::Rock;
use world::events::Composite;

const SEED: u64 = 0x9E3779B97F4A7C15;

#[test]
#[ignore]
fn census() {
    let c = Composite::standard(SEED);
    for (name, (q0, r0)) in [("haven", (104_289, -4_677)), ("belt", (99_155, -2_309))] {
        let (mut land, mut rocky, mut closed, mut trees_closed, mut bedrock) = (0, 0, 0, 0, 0);
        let mut hist = [0usize; 8];
        let mut by_rock = [(0usize, 0usize); 4];
        for dq in (-400..=400).step_by(5) {
            for dr in (-400..=400).step_by(5) {
                let v = c.tile_at(q0 + dq, r0 + dr);
                if v.water.is_some() {
                    continue;
                }
                land += 1;
                let n = v.cover.boulders().count();
                hist[n] += 1;
                let i = match v.rock { Some(Rock::Shale) => 0, Some(Rock::Sandstone) => 1, Some(Rock::Limestone) => 2, _ => 3 };
                by_rock[i].0 += 1;
                if n > 0 {
                    rocky += 1;
                    by_rock[i].1 += 1;
                }
                if n == 7 {
                    bedrock += 1;
                }
                if v.cover.fullness() >= 4 {
                    closed += 1;
                    if n < 4 {
                        trees_closed += 1;
                    }
                }
            }
        }
        let pct = |k: usize| 100.0 * k as f64 / land.max(1) as f64;
        println!("{name}: {land} land tiles; {:.1}% carry boulders, {:.1}% bedrock, {:.1}% closed ({:.1}% by trees or trees among rock)", pct(rocky), pct(bedrock), pct(closed), pct(trees_closed));
        println!("  boulders per tile 0..7: {hist:?}");
        for (i, rock) in ["shale", "sandstone", "limestone", "basement"].iter().enumerate() {
            let (n, k) = by_rock[i];
            println!("  {rock}: {n} tiles, {:.1}% with boulders", 100.0 * k as f64 / n.max(1) as f64);
        }
    }
}
