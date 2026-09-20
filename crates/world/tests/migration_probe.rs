//! Migration probe — what a channel's train looks like by its vigour, so
//! the sinuosity, the belt and the cutoffs are read off a river and not
//! guessed, whether a pinned segment's bends lie evenly along it, and what
//! the trains cost against the routing that made them.
//!
//! Run: cargo test -p world --release --test migration_probe -- --ignored --nocapture

use std::time::Instant;

use world::events::drainage::{DrainageNode, CATCHMENT_FULL, CHANNEL_HEAD};
use world::events::migration::{channel_half_width, flow_line, meander_amplitude, Axis, Channel, Train, VALLEY_HALF_WIDTH};
use world::lattice::NODE_SPACING;

const SEED: u64 = 0x9E3779B97F4A7C15;

fn node(wx: f64, key: (i32, i32), catchment: f64) -> DrainageNode {
    DrainageNode {
        key, q: 0, r: 0, wx, wy: 0.0, elevation: 10.0, surface: 10.0, flooded: false, direction: (1.0, 0.0), catchment, base: 0.0,
        down: None, age: 1.0, erodibility: 1.0, floor: 5.0,
    }
}

/// A straight segment's channel at a vigour, with its flow line.
fn straight(key: (i32, i32), catchment: f64, vigour: f64) -> (Axis, Channel) {
    let l = NODE_SPACING as f64;
    let (p, n) = (node(0.0, key, catchment), node(l, (key.0 + 1, key.1), catchment));
    let axis = Axis::new(flow_line(&p, &n));
    let half = channel_half_width(catchment, 1.0);
    let channel = Channel {
        from: p.key, to: n.key, axis: Vec::new(), train: None, half0: half, half1: half, vigour0: vigour, vigour1: vigour, entry: 1.0,
    };
    (axis, channel)
}

/// Sinuosity, oxbows, amplitude in wavelengths and the path in points of
/// a straight segment's train at a given catchment, over a range of
/// vigours.
fn report(name: &str, catchment: f64) {
    let l = NODE_SPACING as f64;
    let (_, channel) = straight((0, 0), catchment, 1.0);
    println!("{name}: width {:.1} tiles, wavelength {:.0}, stated amplitude {:.0}", 2.0 * channel.half0, 10.9 * 2.0 * channel.half0, meander_amplitude(10.9 * 2.0 * channel.half0));
    println!("{:>6} {:>10} {:>7} {:>12} {:>7} {:>9}", "vigour", "sinuosity", "oxbows", "amplitude/λ", "points", "µs");
    for vigour in [0.05, 0.1, 0.2, 0.35, 0.5, 0.65, 0.8, 1.0] {
        let (axis, channel) = straight((0, 0), catchment, vigour);
        let t0 = Instant::now();
        let train = Train::new(&axis, &channel, SEED).unwrap();
        let us = t0.elapsed().as_secs_f64() * 1e6;
        let path: f64 = train.pts.windows(2).map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1)).sum();
        println!(
            "{vigour:>6.2} {:>10.3} {:>7} {:>12.2} {:>7} {:>9.0}{}",
            path / l,
            train.oxbows.len(),
            train.amplitude / train.wavelength,
            train.pts.len(),
            us,
            if train.amplitude > VALLEY_HALF_WIDTH * 0.79 { "  against the valley wall" } else { "" }
        );
    }
}

#[test]
#[ignore]
fn trains_by_vigour() {
    report("trunk", CATCHMENT_FULL);
    report("mid river", (CHANNEL_HEAD + CATCHMENT_FULL) / 2.0);
    report("head stream", CHANNEL_HEAD + 1.0);
}

/// Where along a trunk's segment its oxbows lie, in wavelengths from the
/// start node, over several segments: a train whose cutoffs pile against
/// one end shows it here.
#[test]
#[ignore]
fn oxbows_along_the_segment() {
    let l = NODE_SPACING as f64;
    for (seed, key) in [(SEED, (0, 0)), (SEED ^ 0x1234, (3, 5)), (SEED, (7, 2)), (SEED ^ 0xabcdef, (11, 13))] {
        let (axis, channel) = straight(key, CATCHMENT_FULL, 1.0);
        let train = Train::new(&axis, &channel, seed).unwrap();
        let at: Vec<String> = train
            .oxbows
            .iter()
            .map(|o| format!("{:.1}", o.iter().map(|p| p.0).sum::<f64>() / o.len() as f64 / train.wavelength))
            .collect();
        println!("seed {seed:#x} key {key:?}: oxbows at {} of {:.1} wavelengths", at.join(" "), l / train.wavelength);
    }
}

/// The train's reach from the line, binned by wavelength along the
/// segment, over the vigour: where the bends grow. Even bins are the
/// pinned segment's bends lying evenly; a head that outgrows its tail is
/// a bias in the drawing.
#[test]
#[ignore]
fn growth_along_the_segment() {
    for vigour in [0.2, 0.5, 0.8, 1.0] {
        let (axis, channel) = straight((0, 0), CATCHMENT_FULL, vigour);
        let train = Train::new(&axis, &channel, SEED).unwrap();
        let bins = (NODE_SPACING as f64 / train.wavelength).ceil() as usize;
        let mut reach = vec![0.0f64; bins];
        for &(x, y) in &train.pts {
            let b = ((x / train.wavelength) as usize).min(bins - 1);
            reach[b] = reach[b].max(y.abs());
        }
        let s: Vec<String> = reach.iter().map(|r| format!("{:5.1}", r)).collect();
        println!("vigour {vigour:>4.2}: reach by wavelength {}", s.join(" "));
    }
}

/// What a drainage cell's channels cost against the routing that made
/// them.
#[test]
#[ignore]
fn channels_against_routing() {
    use world::events::drainage::{DrainageEvent, DrainageIndex};
    use world::events::migration::channels;
    use world::events::plates::Coasts;
    use world::events::thrusting::Outlines;
    use world::hex_to_world;
    let lattice = DrainageIndex::lattice();
    let cell = lattice.cell_id(-58_204, 4_907);
    let (cq, cr) = lattice.cell_center(cell);
    let (cx, cy) = hex_to_world(cq, cr);
    let window = (3 * lattice.radius + 1) as f64;
    let t0 = Instant::now();
    let coasts = Coasts::in_box(cx, cy, window, SEED);
    let outlines = Outlines::in_box(cx, cy, window, SEED);
    let built = t0.elapsed();
    let t1 = Instant::now();
    let published = DrainageEvent::new().route(&lattice, cell, SEED, &coasts, &outlines).owned_cell();
    let routed = t1.elapsed();
    let t2 = Instant::now();
    let all = channels(&[&published], |_| true, SEED);
    let trained = t2.elapsed();
    let trains = all.iter().filter(|c| c.train.is_some()).count();
    println!(
        "coasts+outlines {:.0} ms; routing {:.0} ms for {} nodes; {} channels, {} with trains, {:.1} ms ({:.0} µs per train)",
        built.as_secs_f64() * 1e3,
        routed.as_secs_f64() * 1e3,
        published.nodes.len(),
        all.len(),
        trains,
        trained.as_secs_f64() * 1e3,
        trained.as_secs_f64() * 1e6 / trains.max(1) as f64,
    );
}

/// How the vigour is spread over a cell's channels: how many rivers are
/// at grade enough to show a mature train.
#[test]
#[ignore]
fn vigour_over_a_cell() {
    use world::events::drainage::{DrainageEvent, DrainageIndex};
    use world::events::migration::channels;
    use world::events::plates::Coasts;
    use world::events::thrusting::Outlines;
    use world::hex_to_world;
    let lattice = DrainageIndex::lattice();
    for (name, q, r) in [("spawn", -58_204, 4_907), ("platform", -53_000, 7_000), ("east", -45_000, 3_000)] {
        let cell = lattice.cell_id(q, r);
        let (cq, cr) = lattice.cell_center(cell);
        let (cx, cy) = hex_to_world(cq, cr);
        let window = (3 * lattice.radius + 1) as f64;
        let coasts = Coasts::in_box(cx, cy, window, SEED);
        let outlines = Outlines::in_box(cx, cy, window, SEED);
        let published = DrainageEvent::new().route(&lattice, cell, SEED, &coasts, &outlines).owned_cell();
        let all = channels(&[&published], |_| true, SEED);
        let mut bins = [0usize; 6];
        for c in &all {
            let v = c.vigour0.max(c.vigour1);
            let b = if v <= 0.0 { 0 } else { 1 + ((v * 5.0).floor() as usize).min(4) };
            bins[b] += 1;
        }
        println!("{name}: {} channels; vigour none {}, <0.2 {}, <0.4 {}, <0.6 {}, <0.8 {}, to 1 {}", all.len(), bins[0], bins[1], bins[2], bins[3], bins[4], bins[5]);
    }
}
