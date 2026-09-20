//! Drainage probe — the channel network, read back from the routing and the
//! index. Shape tests run always; the measurements are `#[ignore]`.
//!
//! Run: cargo test -p world --release --test drainage_probe -- --ignored --nocapture

use std::time::Instant;

use common::HexLattice;
use world::events::drainage::{
    node_tile, DrainageEvent, CHANNEL_HEAD, DrainageIndex, Kind, Terminus, DRAINAGE_CELL_SCALE, NODE_SPACING,
};
use world::events::motion::MotionEvent;
use world::events::lithology::LithologyEvent;
use world::events::thickening::ThickeningEvent;
use world::events::plates::Coasts;
use world::events::thrusting::{Outlines, ThrustingEvent};
use world::events::plates::PlateEvent;
use world::events::tilt::{potential, TiltEvent};
use world::events::Composite;
use world::hex_to_world;

const SEED: u64 = 0x9E3779B97F4A7C15;
/// Spawn: on land, under a belt.
const SPAWN: (i32, i32) = (-58204, 4907);

fn composite() -> Composite {
    let mut c = Composite::new(SEED);
    c.add_event(Box::new(PlateEvent::new()));
    c.add_event(Box::new(TiltEvent::new()));
    c.add_event(Box::new(MotionEvent::new()));
    c.add_event(Box::new(ThrustingEvent::new()));
    c.add_event(Box::new(ThickeningEvent::new()));
    c.add_event(Box::new(LithologyEvent::new()));
    c.add_event(Box::new(DrainageEvent::new()));
    c
}

/// The nearest cell to the spawn whose routing cuts a sill it owns: what
/// the sill's claims are read on. Searched, since a change beneath
/// drainage can drain the lakes of any one cell.
fn cell_with_a_cut_sill() -> (i32, i32) {
    let lat = lattice();
    let spawn = spawn_cell();
    let mut cells = lat.cells_within_distance(spawn, 2);
    let centre = lat.cell_center(spawn);
    cells.sort_by_key(|&c| {
        let (q, r) = lat.cell_center(c);
        ((q - centre.0).abs() + (r - centre.1).abs() + (q + r - centre.0 - centre.1).abs(), c)
    });
    for cell in cells {
        let routing = DrainageEvent::new().route(&lat, cell, SEED, &coasts_for(cell), &outlines_for(cell));
        let owned = routing.owned_cell();
        if owned.lakes.iter().any(|l| l.outlet.and_then(|o| owned.nodes.get(&o)).map_or(false, |s| s.cut > 0.0)) {
            return cell;
        }
    }
    panic!("no cell within two of the spawn cuts a sill it owns");
}

fn lattice() -> HexLattice {
    HexLattice::new(DRAINAGE_CELL_SCALE)
}

fn spawn_cell() -> (i32, i32) {
    lattice().cell_id(SPAWN.0, SPAWN.1)
}

/// The plate outlines a routing of `cell` can see: built over its window,
/// what the framework hands its deform.
fn outlines_for(cell: (i32, i32)) -> Outlines {
    let lat = lattice();
    let (cq, cr) = lat.cell_center(cell);
    let (cx, cy) = hex_to_world(cq, cr);
    Outlines::in_box(cx, cy, (3 * lat.radius + 1) as f64, SEED)
}

/// The coasts a routing of `cell` can see, likewise.
fn coasts_for(cell: (i32, i32)) -> Coasts {
    let lat = lattice();
    let (cq, cr) = lat.cell_center(cell);
    let (cx, cy) = hex_to_world(cq, cr);
    Coasts::in_box(cx, cy, (3 * lat.radius + 1) as f64, SEED)
}

/// The routing reads the ground the game stands on: a node's elevation is the
/// composed tile's elevation, exactly, because both are the same functions
/// summed in the same order.
#[test]
fn nodes_sit_on_the_composed_surface() {
    let c = composite();
    let routing = DrainageEvent::new().route(&lattice(), spawn_cell(), SEED, &coasts_for(spawn_cell()), &outlines_for(spawn_cell()));
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
    let routing = DrainageEvent::new().route(&lattice(), spawn_cell(), SEED, &coasts_for(spawn_cell()), &outlines_for(spawn_cell()));
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
    let routing = DrainageEvent::new().route(&lattice(), spawn_cell(), SEED, &coasts_for(spawn_cell()), &outlines_for(spawn_cell()));
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
    let ra = ev.route(&lat, a, SEED, &coasts_for(a), &outlines_for(a));
    let rb = ev.route(&lat, b, SEED, &coasts_for(b), &outlines_for(b));

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
    let routing = DrainageEvent::new().route(&lattice(), spawn_cell(), SEED, &coasts_for(spawn_cell()), &outlines_for(spawn_cell()));
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
    let routing = DrainageEvent::new().route(&lattice(), spawn_cell(), SEED, &coasts_for(spawn_cell()), &outlines_for(spawn_cell()));
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

/// The lakes: how deep each stands over its floor, and how many sit inside
/// a belt. A trough between two ranges impounds until something breaches
/// the sill, and the depth is how much breaching it takes.
#[test]
#[ignore]
fn lakes_in_the_troughs() {
    let routing = DrainageEvent::new().route(&lattice(), spawn_cell(), SEED, &coasts_for(spawn_cell()), &outlines_for(spawn_cell()));
    let cell = routing.owned_cell();
    let mut depths: Vec<f64> = Vec::new();
    let mut in_belt = 0usize;
    let outlines = outlines_for(spawn_cell());
    for lake in &cell.lakes {
        let floor = lake
            .nodes
            .iter()
            .map(|k| cell.nodes[k].elevation)
            .fold(f64::MAX, f64::min);
        depths.push(lake.surface - floor);
        let (q, r) = node_tile(lake.nodes[0]);
        let (wx, wy) = hex_to_world(q, r);
        if outlines.relief(wx, wy) > 0.0 {
            in_belt += 1;
        }
    }
    depths.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p = |f: f64| depths.get(((depths.len().max(1) - 1) as f64 * f) as usize).copied().unwrap_or(0.0);
    println!(
        "\n=== lakes in cell {:?} ===\n{} lakes, {} inside a belt; depth over floor p50 {:.1} z, p90 {:.1} z, max {:.1} z; \
         nodes per lake p50 {}",
        spawn_cell(),
        depths.len(),
        in_belt,
        p(0.5),
        p(0.9),
        depths.last().copied().unwrap_or(0.0),
        {
            let mut sizes: Vec<usize> = cell.lakes.iter().map(|l| l.nodes.len()).collect();
            sizes.sort_unstable();
            sizes.get(sizes.len() / 2).copied().unwrap_or(0)
        }
    );
}

/// What a cell costs, and what the first tile in a fresh composite costs
/// because of it.
#[test]
#[ignore]
fn cost() {
    let lat = lattice();
    let ev = DrainageEvent::new();
    let outlines = outlines_for(spawn_cell());
    let coasts = coasts_for(spawn_cell());
    let t = Instant::now();
    let routing = ev.route(&lat, spawn_cell(), SEED, &coasts, &outlines);
    let cold = t.elapsed();
    let t = Instant::now();
    let _ = ev.route(&lat, spawn_cell(), SEED, &coasts, &outlines);
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

/// Base level is the first lake down a node's larger share, a cut node's
/// floor, or the sea:
/// what dissection cuts toward, published because only the routing knows
/// the path.
#[test]
fn base_level_is_the_first_lake_downstream_or_the_sea() {
    let routing = DrainageEvent::new().route(&lattice(), spawn_cell(), SEED, &coasts_for(spawn_cell()), &outlines_for(spawn_cell()));
    let n = routing.keys.len();
    let (mut lakes, mut seas) = (0, 0);
    for k in 0..n {
        if !matches!(routing.kind[k], Kind::Land | Kind::Lake) { continue }
        let mut cur = k;
        let expected = loop {
            if cur != k && routing.cut[cur] > 0.0 {
                break routing.elevation[cur] - routing.cut[cur];
            }
            if let Some(id) = routing.lake_of[cur] {
                let surface = routing.surface[cur];
                let _ = id;
                break surface;
            }
            match routing.down[cur] {
                Some(d) if matches!(routing.kind[d], Kind::Land | Kind::Lake) => cur = d,
                _ => break 0.0,
            }
        };
        if expected > 0.0 { lakes += 1 } else { seas += 1 }
        assert!((routing.base[k] - expected).abs() < 1e-9, "base {} against {expected} at {:?}", routing.base[k], routing.keys[k]);
        assert!(routing.elevation[k] >= routing.base[k] - 1e-9 || routing.lake_of[k].is_some(), "ground below its base at {:?}", routing.keys[k]);
    }
    println!("{lakes} nodes drain to a lake, {seas} to the sea or the window's edge");
    assert!(seas > 0);
}

/// A spilling lake's sill is cut, never below the base level beneath it,
/// and its surface is the cut sill. From the sill the breach runs down the
/// outflow without rising and ends on ground no higher than it. A pit that
/// drains less than a channel head keeps its lake whole. Age is a share.
#[test]
fn a_cut_sill_lowers_its_lake_and_breaches_the_rim() {
    let cell = cell_with_a_cut_sill();
    let routing = DrainageEvent::new().route(&lattice(), cell, SEED, &coasts_for(cell), &outlines_for(cell));
    for k in 0..routing.keys.len() {
        assert!((0.0..=1.0).contains(&routing.age[k]), "age {} at {:?}", routing.age[k], routing.keys[k]);
        assert!(routing.cut[k] >= 0.0, "a negative cut at {:?}", routing.keys[k]);
    }
    let cell = routing.owned_cell();
    let (mut cut, mut kept, mut breach_nodes) = (0, 0, 0);
    for lake in &cell.lakes {
        let Some(sill) = lake.outlet.and_then(|o| cell.nodes.get(&o)) else { continue };
        let floor = lake.nodes.iter().map(|k| cell.nodes[k].elevation).fold(f64::MAX, f64::min);
        println!("lake of {} nodes: surface {:.1}, floor {:.1}, sill {:.1} cut {:.2}, age {:.2}, catchment {:.1}, base {:.1}", lake.nodes.len(), lake.surface, floor, sill.elevation, sill.cut, sill.age, sill.catchment, sill.base);
        assert!((sill.elevation - sill.cut - lake.surface).abs() < 1e-9, "a lake's surface off its cut sill at {:?}", sill.key);
        assert!(lake.surface >= sill.base - 1e-9, "a lake cut below the base level beneath its sill at {:?}", sill.key);
        if sill.catchment <= CHANNEL_HEAD {
            assert_eq!(sill.cut, 0.0, "a sill cut by water below the channel head at {:?}", sill.key);
            kept += 1;
        }
        if sill.cut == 0.0 {
            continue;
        }
        cut += 1;
        let mut last = lake.surface;
        let mut next = sill.down.and_then(|d| cell.nodes.get(&d));
        while let Some(n) = next {
            if n.lake.is_some() {
                break;
            }
            let ground = n.elevation - n.cut;
            assert!(ground <= last + 1e-9, "the breach rises at {:?}", n.key);
            if n.cut == 0.0 {
                break;
            }
            breach_nodes += 1;
            last = ground;
            next = n.down.and_then(|d| cell.nodes.get(&d));
        }
    }
    let (breached, deepest, at) = routing.cut.iter().enumerate().fold((0, 0.0f64, (0.0, 0.0)), |(n, d, at), (k, &c)| {
        let (q, r) = node_tile(routing.keys[k]);
        if c > d { (n + 1, c, hex_to_world(q, r)) } else { (n + (c > 0.0) as usize, d, at) }
    });
    println!("{cut} owned sills cut, {breach_nodes} nodes of breach behind them; {kept} lakes kept for draining less than a head; {breached} nodes cut in the window, deepest {deepest:.1} z at world {at:?}");
    assert!(cut > 0, "no sill in the cell is cut");
}

/// Where the ground is cut below the envelope and by how much, over the
/// cells around a few places, each cut told apart: a sill, the breach
/// down its outflow, or a rim node lowered to a lake's spill. The deepest
/// rim cuts are the notches a walker meets.
#[test]
#[ignore]
fn deepest_cuts() {
    let lat = lattice();
    let places = [("spawn", SPAWN), ("belt", world::world_to_hex(-46_300.0, 3_200.0)), ("bowl", world::world_to_hex(-62_475.0, 10_912.0))];
    let mut found: Vec<(f64, &str, (i32, i32), (f64, f64), &'static str, f64, f64)> = Vec::new();
    for (label, at) in places {
        let center = lat.cell_id(at.0, at.1);
        for cell in lat.cells_within_distance(center, 1) {
            let routing = DrainageEvent::new().route(&lat, cell, SEED, &coasts_for(cell), &outlines_for(cell));
            let owned = routing.owned_cell();
            let sills: std::collections::HashSet<_> = owned.lakes.iter().filter_map(|l| l.outlet).collect();
            let mut breach = std::collections::HashSet::new();
            for &s in &sills {
                let mut next = owned.nodes.get(&s).and_then(|n| n.down);
                while let Some(k) = next {
                    let Some(n) = owned.nodes.get(&k) else { break };
                    if n.cut <= 0.0 || !breach.insert(k) { break }
                    next = n.down;
                }
            }
            for (key, n) in &owned.nodes {
                if n.cut <= 0.0 { continue }
                let kind = if sills.contains(key) { "sill" } else if breach.contains(key) { "breach" } else { "rim" };
                let (q, r) = node_tile(*key);
                found.push((n.cut, label, (q, r), hex_to_world(q, r), kind, n.elevation, n.elevation - n.cut));
            }
        }
    }
    found.sort_by(|a, b| b.0.total_cmp(&a.0));
    found.dedup_by(|a, b| a.2 == b.2);
    println!("{:>7} {:>6} {:>16} {:>18} {:>7} {:>8} {:>8}", "cut", "place", "qr", "world", "kind", "envelope", "floor");
    for (cut, label, (q, r), (wx, wy), kind, env, floor) in found.iter().take(25) {
        println!("{cut:7.1} {label:>6} {:>16} {:>18} {kind:>7} {env:8.1} {floor:8.1}", format!("({q},{r})"), format!("({wx:.0},{wy:.0})"));
    }
    let rims = found.iter().filter(|f| f.4 == "rim").count();
    println!("{} cut nodes, {rims} of them rim", found.len());
}

/// A lake's shore along a line, at the tile level: the envelope, the
/// composed ground, the water, and whether the nearest fine point is in
/// the lake's extent. Run with `SHORE="x0,y0,x1,y1,step"`.
#[test]
#[ignore]
fn shore_section() {
    use world::events::drainage::nearest_fine;
    let spec = std::env::var("SHORE").unwrap();
    let v: Vec<f64> = spec.split(',').map(|s| s.trim().parse().unwrap()).collect();
    let (x0, y0, x1, y1, step) = (v[0], v[1], v[2], v[3], v[4]);
    let lat = lattice();
    let (mq, mr) = world::world_to_hex((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let cell = lat.cell_id(mq, mr);
    let (coasts, outlines) = (coasts_for(cell), outlines_for(cell));
    let routing = DrainageEvent::new().route(&lat, cell, SEED, &coasts, &outlines);
    let owned = routing.owned_cell();
    let c = Composite::standard(SEED);
    let len = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
    let n = (len / step).ceil() as usize;
    println!("{:>6} {:>8} {:>8} {:>8} {:>8} {:>6}  {:>12} lake", "s", "wx", "wy", "envelope", "ground", "water", "fine");
    for i in 0..=n {
        let t = i as f64 / n as f64;
        let (wx, wy) = (x0 + (x1 - x0) * t, y0 + (y1 - y0) * t);
        let (q, r) = world::world_to_hex(wx, wy);
        let envelope = world::events::drainage::surface_at(wx, wy, SEED, &coasts, &outlines);
        let ground = c.tile_at(q, r).elevation;
        let water = c.water_at(q, r).map(|w| w.to_string()).unwrap_or("-".into());
        let fine = nearest_fine(wx, wy);
        let held: Vec<String> = owned.lakes.iter().filter(|l| l.extent.contains(&fine)).map(|l| format!("{:.1}", l.surface)).collect();
        println!("{:6.0} {wx:8.0} {wy:8.0} {envelope:8.1} {ground:8.1} {water:>6}  {:>12} {}", t * len, format!("{fine:?}"), if held.is_empty() { "-".to_string() } else { held.join(" ") });
    }
}

/// Dry tiles under a lake's surface within a node spacing of its flooded
/// nodes: the holes in the water. Run with `HOLES="cx,cy,radius,stride"`.
#[test]
#[ignore]
fn holes() {
    use world::events::drainage::nearest_fine;
    let spec = std::env::var("HOLES").unwrap();
    let v: Vec<f64> = spec.split(',').map(|s| s.trim().parse().unwrap()).collect();
    let (cx, cy, radius, stride) = (v[0], v[1], v[2], v[3]);
    let lat = lattice();
    let (mq, mr) = world::world_to_hex(cx, cy);
    let cell = lat.cell_id(mq, mr);
    let (coasts, outlines) = (coasts_for(cell), outlines_for(cell));
    let routing = DrainageEvent::new().route(&lat, cell, SEED, &coasts, &outlines);
    let owned = routing.owned_cell();
    let c = Composite::standard(SEED);
    let (mut wet, mut holes) = (0, 0);
    let mut shown = 0;
    let mut y = cy - radius;
    while y <= cy + radius {
        let mut x = cx - radius;
        while x <= cx + radius {
            let (q, r) = world::world_to_hex(x, y);
            let view = c.tile_at(q, r);
            let water = c.water_at(q, r);
            if water.is_some() { wet += 1 }
            if water.is_none() {
                let envelope = world::events::drainage::surface_at(x, y, SEED, &coasts, &outlines);
                for lake in &owned.lakes {
                    let near = lake.nodes.iter().any(|k| { let (nq, nr) = node_tile(*k); let (nx, ny) = hex_to_world(nq, nr); (nx - x).hypot(ny - y) <= NODE_SPACING as f64 });
                    if near && lake.surface > view.elevation + 0.5 {
                        holes += 1;
                        let fine = nearest_fine(x, y);
                        if shown < 12 {
                            shown += 1;
                            let (fx, fy) = world::events::drainage::fine_world(fine);
                            let fine_env = world::events::drainage::surface_at(fx, fy, SEED, &coasts, &outlines);
                            println!("hole at qr ({q},{r}) world ({x:.0},{y:.0}): ground {:.1}, envelope {envelope:.1}, lake {:.1}, fine {fine:?} in extent {} at {:.0} from it, envelope there {fine_env:.1}", view.elevation, lake.surface, lake.extent.contains(&fine), (fx - x).hypot(fy - y));
                        }
                        break;
                    }
                }
            }
            x += stride;
        }
        y += stride;
    }
    println!("{wet} wet samples, {holes} holes");
}

/// One tile's water as every cell around it publishes the lake: which
/// cells' copies of the lake hold the tile's nearest fine point in their
/// extent. Run with `TILE="q,r"`.
#[test]
#[ignore]
fn tile_shores() {
    use world::events::drainage::nearest_fine;
    let spec = std::env::var("TILE").unwrap();
    let v: Vec<i32> = spec.split(',').map(|s| s.trim().parse().unwrap()).collect();
    let (q, r) = (v[0], v[1]);
    let (wx, wy) = hex_to_world(q, r);
    let fine = nearest_fine(wx, wy);
    let lat = lattice();
    let home = lat.cell_id(q, r);
    for cell in lat.cells_within_distance(home, 1) {
        let routing = DrainageEvent::new().route(&lat, cell, SEED, &coasts_for(cell), &outlines_for(cell));
        let owned = routing.owned_cell();
        for lake in &owned.lakes {
            let near = lake.nodes.iter().filter(|k| { let (nq, nr) = node_tile(**k); let (nx, ny) = hex_to_world(nq, nr); (nx - wx).hypot(ny - wy) <= NODE_SPACING as f64 }).count();
            let nearest = lake.nodes.iter().map(|k| { let (nq, nr) = node_tile(*k); let (nx, ny) = hex_to_world(nq, nr); (nx - wx).hypot(ny - wy) }).fold(f64::MAX, f64::min);
            if nearest > 6.0 * NODE_SPACING as f64 { continue }
            println!("cell {cell:?}: nearest node {nearest:.0}; lake surface {:.1}, {} owned nodes, {near} within a spacing, extent {} points, holds the tile's fine point: {}", lake.surface, lake.nodes.len(), lake.extent.len(), lake.extent.contains(&fine));
        }
    }
}
