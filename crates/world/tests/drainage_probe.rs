//! Drainage probe — the channel network, read back from the routing and the
//! index. Shape tests run always; the measurements are `#[ignore]`.
//!
//! Run: cargo test -p world --release --test drainage_probe -- --ignored --nocapture

use std::sync::Arc;
use std::time::Instant;

use common::HexLattice;
use world::events::drainage::{
    node_tile, DrainageEvent, DrainageIndex, Kind, Terminus, DRAINAGE_CELL_SCALE, NODE_SPACING,
};
use world::events::motion::MotionEvent;
use world::events::orogen::OrogenEvent;
use world::events::plates::PlateEvent;
use world::events::tilt::{potential, TiltEvent};
use world::events::Composite;
use world::{hex_to_world, PlateCache};

const SEED: u64 = 0x9E3779B97F4A7C15;
/// Spawn: on land, under a belt.
const SPAWN: (i32, i32) = (116953, 21431);

fn composite() -> Composite {
    let plate_cache = Arc::new(PlateCache::new(SEED));
    let mut c = Composite::new(SEED);
    c.add_event(Box::new(PlateEvent::with_cache(plate_cache.clone())));
    c.add_event(Box::new(TiltEvent::new()));
    c.add_event(Box::new(MotionEvent::with_cache(plate_cache, SEED)));
    c.add_event(Box::new(OrogenEvent::new()));
    c.add_event(Box::new(DrainageEvent::new()));
    c
}

fn lattice() -> HexLattice {
    HexLattice::new(DRAINAGE_CELL_SCALE)
}

fn spawn_cell() -> (i32, i32) {
    lattice().cell_id(SPAWN.0, SPAWN.1)
}

/// The routing reads the ground the game stands on: a node's elevation is the
/// composed tile's elevation, exactly, because both are the same functions
/// summed in the same order.
#[test]
fn nodes_sit_on_the_composed_surface() {
    let c = composite();
    let routing = DrainageEvent::new().route(&lattice(), spawn_cell(), SEED);
    let mut worst = 0.0f64;
    let mut checked = 0;
    for (k, &key) in routing.keys.iter().enumerate().step_by(211) {
        let (q, r) = node_tile(key);
        worst = worst.max((c.tile_at(q, r).elevation - routing.elevation[k]).abs());
        checked += 1;
    }
    println!("{checked} nodes; worst |composite − surface_at| = {worst:.3e}");
    assert!(worst < 1e-9, "the surface drainage routes on is not the composite's: {worst}");
}

/// Every land node reaches the sea or the window's edge, and never loops.
#[test]
fn every_land_node_drains_to_a_sink() {
    let routing = DrainageEvent::new().route(&lattice(), spawn_cell(), SEED);
    let n = routing.keys.len();
    let (mut land, mut to_sea, mut to_edge) = (0, 0, 0);
    for k in 0..n {
        if !matches!(routing.kind[k], Kind::Land | Kind::Lake) {
            continue;
        }
        land += 1;
        let (mut cur, mut steps) = (k, 0);
        while let Some(d) = routing.down[cur] {
            cur = d;
            steps += 1;
            assert!(steps <= n, "cycle downstream of node {:?}", routing.keys[k]);
        }
        match routing.kind[cur] {
            Kind::Sea => to_sea += 1,
            Kind::Edge => to_edge += 1,
            other => panic!("node {:?} ends on a {other:?} node", routing.keys[k]),
        }
    }
    println!("{n} nodes, {land} land: {to_sea} reach the sea, {to_edge} leave the window");
}

/// Catchment counts every node exactly once at its sink, and along a reach
/// it never falls below the larger share handed down: a node's water splits
/// two ways, and a reach follows the bigger one.
#[test]
fn catchment_is_conserved_and_grows_downstream() {
    let routing = DrainageEvent::new().route(&lattice(), spawn_cell(), SEED);
    let n = routing.keys.len();
    let at_sinks: f64 = (0..n)
        .filter(|&k| routing.down[k].is_none())
        .map(|k| routing.catchment[k])
        .sum();
    assert!(
        (at_sinks - n as f64).abs() < 1e-6 * n as f64,
        "catchment lost or double-counted on the way down: {at_sinks} of {n}"
    );

    let cell = routing.owned_cell();
    let mut reaches = 0;
    for reach in &cell.reaches {
        let mut last = 0.0f64;
        for key in &reach.nodes {
            let c = cell.nodes[key].catchment;
            assert!(c >= 0.5 * last - 1e-9, "catchment fell from {last} to {c} along a reach");
            last = c;
        }
        reaches += 1;
    }
    println!("{reaches} reaches in the cell; catchment conserved to {at_sinks:.6} of {n}");
}

/// A cell publishes its own nodes and nothing else, and everything a reach
/// or a lake names is a node it published.
#[test]
fn the_index_holds_the_owned_share_only() {
    let c = composite();
    c.tile_at(SPAWN.0, SPAWN.1);
    let lat = lattice();
    let cell = spawn_cell();
    c.with_indexes(|ix| {
        let idx = ix.get::<DrainageIndex>().expect("drainage index registered");
        let entry = idx.cells.get(&cell).expect("spawn cell deformed");
        for (key, node) in &entry.nodes {
            assert_eq!(lat.cell_id(node.q, node.r), cell, "node {key:?} is not this cell's");
            assert_eq!(idx.node(*key).map(|n| n.key), Some(*key), "node lookup by key");
        }
        for reach in &entry.reaches {
            for key in &reach.nodes {
                assert!(entry.nodes.contains_key(key), "reach names unpublished node {key:?}");
            }
        }
        for lake in &entry.lakes {
            for key in &lake.nodes {
                let node = &entry.nodes[key];
                assert!((node.surface - lake.surface).abs() < 1e-9, "lake node at another level");
            }
        }
        println!(
            "cell {cell:?}: {} nodes, {} reaches, {} lakes",
            entry.nodes.len(),
            entry.reaches.len(),
            entry.lakes.len()
        );
    });
}

/// Two windows containing the same node compute the same outflow wherever
/// their filled surfaces agree. The share that disagrees is the price of
/// windowing, and it is measured here rather than assumed away.
#[test]
#[ignore]
fn neighbouring_windows_agree_on_shared_nodes() {
    let lat = lattice();
    let a = spawn_cell();
    let b = lat.neighbor_cells(a)[0];
    let ev = DrainageEvent::new();
    let ra = ev.route(&lat, a, SEED);
    let rb = ev.route(&lat, b, SEED);

    let (mut shared, mut agree, mut same_catchment) = (0, 0, 0);
    for (kb, &key) in rb.keys.iter().enumerate() {
        if !rb.owned[kb] || !matches!(rb.kind[kb], Kind::Land | Kind::Lake) {
            continue;
        }
        let Some(ka) = ra.index_of(key) else { continue };
        shared += 1;
        let down_a = ra.down[ka].map(|d| ra.keys[d]);
        let down_b = rb.down[kb].map(|d| rb.keys[d]);
        if down_a == down_b {
            agree += 1;
        }
        if (ra.catchment[ka] - rb.catchment[kb]).abs() < 1e-9 {
            same_catchment += 1;
        }
    }
    println!(
        "\n=== windows {a:?} and {b:?} ===\n{shared} of {b:?}'s land nodes lie in both windows: \
         outflow agrees on {agree} ({:.1}%), catchment on {same_catchment} ({:.1}%)",
        100.0 * agree as f64 / shared.max(1) as f64,
        100.0 * same_catchment as f64 / shared.max(1) as f64
    );
    assert!(agree * 10 >= shared * 9, "windows disagree on more than a tenth of shared outflows");
}

/// The spec's own check: a continent drains from its belts down its tilt.
/// For every channel with a catchment, the share whose outflow runs with the
/// tilt's downslope.
#[test]
#[ignore]
fn continents_drain_down_their_tilt() {
    let routing = DrainageEvent::new().route(&lattice(), spawn_cell(), SEED);
    let h = NODE_SPACING as f64;
    let (mut channels, mut with_tilt, mut nodes_to_sea, mut land) = (0, 0, 0.0f64, 0u64);
    let mut ends = [0usize; 4];
    for k in 0..routing.keys.len() {
        if matches!(routing.kind[k], Kind::Land | Kind::Lake) {
            land += 1;
        }
        if routing.kind[k] != Kind::Land || routing.catchment[k] < 10.0 {
            continue;
        }
        channels += 1;
        let (q, r) = node_tile(routing.keys[k]);
        let (wx, wy) = hex_to_world(q, r);
        let (dx, dy) = routing.direction[k];
        let gx = potential(wx + h, wy, SEED) - potential(wx - h, wy, SEED);
        let gy = potential(wx, wy + h, SEED) - potential(wx, wy - h, SEED);
        if dx * -gx + dy * -gy > 0.0 {
            with_tilt += 1;
        }
    }
    for k in 0..routing.keys.len() {
        if routing.down[k].is_none() && routing.kind[k] == Kind::Sea {
            nodes_to_sea += routing.catchment[k] - 1.0;
        }
    }
    let cell = routing.owned_cell();
    for reach in &cell.reaches {
        ends[match reach.end {
            Terminus::Continues => 0,
            Terminus::Sea => 1,
            Terminus::Lake => 2,
            Terminus::Edge => 3,
        }] += 1;
    }
    println!(
        "\n=== drainage against tilt, cell {:?} ===\n\
         {channels} channel nodes (catchment ≥ 10): {with_tilt} run with the tilt ({:.1}%)\n\
         {nodes_to_sea:.0} of {land} land nodes drain to the sea within the window\n\
         reach ends: {} continue, {} sea, {} lake, {} edge; {} lakes",
        spawn_cell(),
        100.0 * with_tilt as f64 / channels.max(1) as f64,
        ends[0],
        ends[1],
        ends[2],
        ends[3],
        cell.lakes.len()
    );
    assert!(with_tilt * 2 > channels, "channels run against the tilt more often than with it");
}

/// How concentrated the water is: catchment percentiles over the window's
/// land nodes. Sheet flow reads as a flat distribution with a low maximum;
/// a channel network reads as a long tail.
#[test]
#[ignore]
fn catchment_distribution() {
    let routing = DrainageEvent::new().route(&lattice(), spawn_cell(), SEED);
    let mut c: Vec<f64> = (0..routing.keys.len())
        .filter(|&k| routing.kind[k] == Kind::Land)
        .map(|k| routing.catchment[k])
        .collect();
    c.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p = |f: f64| c[((c.len() - 1) as f64 * f) as usize];
    let over = |t: f64| c.iter().filter(|&&x| x >= t).count();
    println!(
        "\n=== catchment over {} land nodes ===\np50 {:.1}  p90 {:.1}  p99 {:.1}  max {:.1}\n\
         nodes with catchment ≥ 4: {}, ≥ 16: {}, ≥ 64: {}",
        c.len(),
        p(0.5),
        p(0.9),
        p(0.99),
        c.last().unwrap(),
        over(4.0),
        over(16.0),
        over(64.0)
    );
}

/// What a cell costs, and what the first tile in a fresh composite costs
/// because of it.
#[test]
#[ignore]
fn cost() {
    let lat = lattice();
    let ev = DrainageEvent::new();
    let t = Instant::now();
    let routing = ev.route(&lat, spawn_cell(), SEED);
    let cold = t.elapsed();
    let t = Instant::now();
    let _ = ev.route(&lat, spawn_cell(), SEED);
    let warm = t.elapsed();
    println!(
        "\n=== cost ===\nwindow: {} nodes, {} owned; route cold {:.1} ms, with elevations memoised {:.1} ms",
        routing.keys.len(),
        routing.owned.iter().filter(|&&o| o).count(),
        cold.as_secs_f64() * 1e3,
        warm.as_secs_f64() * 1e3
    );

    let c = composite();
    let t = Instant::now();
    c.tile_at(SPAWN.0, SPAWN.1);
    let first = t.elapsed();
    let t = Instant::now();
    c.tile_at(SPAWN.0 + 4000, SPAWN.1 - 3000);
    let second = t.elapsed();
    println!(
        "first tile in a fresh composite {:.1} ms; a tile 5,000 away in the same cell {:.2} ms",
        first.as_secs_f64() * 1e3,
        second.as_secs_f64() * 1e3
    );
}
