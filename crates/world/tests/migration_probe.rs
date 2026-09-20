//! Migration probe — how a channel's train develops with its run, so the
//! run length at full vigour is read off a river and not guessed, and
//! whether a pinned segment migrates evenly along its length.
//!
//! Run: cargo test -p world --release --test migration_probe -- --ignored --nocapture

use world::events::drainage::{DrainageNode, CATCHMENT_FULL, CHANNEL_HEAD};
use world::events::migration::{
    channel_half_width, flow_line, Axis, Channel, Train, MIGRATION_STEPS, VALLEY_HALF_WIDTH,
};
use world::lattice::NODE_SPACING;

const SEED: u64 = 0x9E3779B97F4A7C15;

fn node(wx: f64, key: (i32, i32), catchment: f64) -> DrainageNode {
    DrainageNode {
        key, q: 0, r: 0, wx, wy: 0.0, elevation: 10.0, surface: 10.0, direction: (1.0, 0.0), catchment, base: 0.0,
        down: None, lake: None, sill: false, age: 1.0, erodibility: 1.0, cut: 0.0, floor: 5.0,
    }
}

/// A straight segment's channel at full vigour, with its flow line.
fn straight(key: (i32, i32), catchment: f64) -> (Axis, Channel) {
    let l = NODE_SPACING as f64;
    let (p, n) = (node(0.0, key, catchment), node(l, (key.0 + 1, key.1), catchment));
    let axis = Axis::new(flow_line(&p, &n));
    let half = channel_half_width(catchment, 1.0);
    let channel = Channel {
        from: p.key, to: n.key, axis: Vec::new(), train: None, half0: half, half1: half, vigour0: 1.0, vigour1: 1.0, entry: 1.0,
    };
    (axis, channel)
}

/// Sinuosity, oxbows, amplitude in wavelengths and the path in points of
/// a straight segment's train at a given catchment, over a range of runs.
fn report(name: &str, catchment: f64) {
    let l = NODE_SPACING as f64;
    let (axis, channel) = straight((0, 0), catchment);
    println!("{name}: width {:.1} tiles, wavelength {:.0}", 2.0 * channel.half0, 10.9 * 2.0 * channel.half0);
    println!("{:>6} {:>10} {:>7} {:>12} {:>7} {:>9}", "steps", "sinuosity", "oxbows", "amplitude/λ", "points", "ms");
    for steps in [0, 25, 50, 100, 150, 200, 300, 400, 600, 800, 1200] {
        let t0 = std::time::Instant::now();
        let train = Train::migrated(&axis, &channel, SEED, steps).unwrap();
        let ms = t0.elapsed().as_secs_f64() * 1e3;
        let path: f64 = train.pts.windows(2).map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1)).sum();
        println!(
            "{steps:>6} {:>10.3} {:>7} {:>12.2} {:>7} {:>9.1}{}",
            path / l,
            train.oxbows.len(),
            train.amplitude / train.wavelength,
            train.pts.len(),
            ms,
            if steps == MIGRATION_STEPS { "  <- MIGRATION_STEPS" } else { "" }
        );
        if train.amplitude > VALLEY_HALF_WIDTH * 0.79 {
            println!("       against the valley wall");
        }
    }
}

#[test]
#[ignore]
fn trains_by_run() {
    report("trunk", CATCHMENT_FULL);
    report("mid river", (CHANNEL_HEAD + CATCHMENT_FULL) / 2.0);
    report("head stream", CHANNEL_HEAD + 1.0);
}

/// Where along a trunk's segment its oxbows lie, in wavelengths from the
/// start node, over several seeds: a pinned segment whose bends pile
/// against one end, or mature at one end first, shows it here.
#[test]
#[ignore]
fn oxbows_along_the_segment() {
    let l = NODE_SPACING as f64;
    for (seed, key) in [(SEED, (0, 0)), (SEED ^ 0x1234, (3, 5)), (SEED, (7, 2)), (SEED ^ 0xabcdef, (11, 13))] {
        let (axis, channel) = straight(key, CATCHMENT_FULL);
        for steps in [MIGRATION_STEPS, 2 * MIGRATION_STEPS] {
            let train = Train::migrated(&axis, &channel, seed, steps).unwrap();
            let at: Vec<String> = train
                .oxbows
                .iter()
                .map(|o| format!("{:.1}", o.iter().map(|p| p.0).sum::<f64>() / o.len() as f64 / train.wavelength))
                .collect();
            println!("seed {seed:#x} key {key:?} steps {steps}: oxbows at {} of {:.1} wavelengths", at.join(" "), l / train.wavelength);
        }
    }
}

/// The train's reach from the line, binned by wavelength along the
/// segment, over the run: where the bends grow. Even bins are the pinned
/// segment migrating as one river; a head that outgrows its tail is a
/// bias in the model.
#[test]
#[ignore]
fn growth_along_the_segment() {
    let l = NODE_SPACING as f64;
    let (axis, channel) = straight((0, 0), CATCHMENT_FULL);
    for steps in [0, 50, 100, 150, 200, 250] {
        let train = Train::migrated(&axis, &channel, SEED, steps).unwrap();
        let bins = (l / train.wavelength).ceil() as usize;
        let mut reach = vec![0.0f64; bins];
        for &(x, y) in &train.pts {
            let b = ((x / train.wavelength) as usize).min(bins - 1);
            reach[b] = reach[b].max(y.abs());
        }
        let s: Vec<String> = reach.iter().map(|r| format!("{:5.1}", r)).collect();
        println!("steps {steps:>4}: reach by wavelength {}", s.join(" "));
    }
}
