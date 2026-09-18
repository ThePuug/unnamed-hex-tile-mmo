//! Plate probe — what the plate graph and the motion on it come to: how
//! much of the world is continental, how big continents are, what the edges
//! resolve to, and how hard convergent edges close. The convergence
//! distribution is what `CONVERGENCE_FULL` is read from.
//!
//! Run: cargo test -p world --release --test plate_probe -- --ignored --nocapture

use std::collections::{HashMap, HashSet};

use world::events::motion::{resolve, BoundaryRegime, MarginClass};
use world::events::thrusting::CONVERGENCE_FULL;
use world::tectonic::{edges_of, plate, PlateId, CONTINENT_GATE, PLATE_SPACING};

const SEEDS: [u64; 3] = [0x9E3779B97F4A7C15, 0x0123456789abcdef, 0xdeadbeefcafe1235];

/// Plates in a square of `n × n` lattice cells around the origin.
fn plate_ids(n: i32) -> Vec<PlateId> {
    let mut ids = Vec::new();
    for cq in -n..=n {
        for cr in -n..=n {
            ids.push((cq, cr));
        }
    }
    ids
}

fn percentile(v: &mut Vec<f64>, f: f64) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[((v.len() - 1) as f64 * f) as usize]
}

/// Land share, continent sizes, edge lengths, regime shares by length, and
/// the convergence distribution, per seed.
#[test]
#[ignore]
fn plate_graph_report() {
    for seed in SEEDS {
        let ids = plate_ids(24);
        let plates: HashMap<PlateId, _> = ids.iter().map(|&id| (id, plate(id, seed))).collect();
        let continental = plates.values().filter(|p| p.continental).count();

        // Continents: connected components of continental plates over the
        // edges, counted only where both plates lie in the window.
        let mut edges = Vec::new();
        let mut seen: HashSet<(PlateId, PlateId)> = HashSet::new();
        for &id in &ids {
            for e in edges_of(id, seed) {
                if plates.contains_key(&e.a.id) && plates.contains_key(&e.b.id) && seen.insert(e.ids()) {
                    edges.push(e);
                }
            }
        }
        let mut parent: HashMap<PlateId, PlateId> = ids.iter().map(|&i| (i, i)).collect();
        fn find(parent: &mut HashMap<PlateId, PlateId>, x: PlateId) -> PlateId {
            let p = parent[&x];
            if p == x { return x }
            let root = find(parent, p);
            parent.insert(x, root);
            root
        }
        for e in &edges {
            if e.a.continental && e.b.continental {
                let (ra, rb) = (find(&mut parent, e.a.id), find(&mut parent, e.b.id));
                if ra != rb { parent.insert(ra, rb); }
            }
        }
        let mut sizes: HashMap<PlateId, usize> = HashMap::new();
        for &id in &ids {
            if plates[&id].continental {
                let r = find(&mut parent, id);
                *sizes.entry(r).or_default() += 1;
            }
        }
        let mut size_hist: Vec<usize> = sizes.values().copied().collect();
        size_hist.sort();
        let mut by_size: HashMap<usize, usize> = HashMap::new();
        for s in &size_hist { *by_size.entry(*s).or_default() += 1 }
        let mut by_size: Vec<(usize, usize)> = by_size.into_iter().collect();
        by_size.sort();

        // Edges: lengths, regimes by length, convergence.
        let mut lengths: Vec<f64> = edges.iter().map(|e| e.length()).collect();
        let (mut conv_len, mut div_len, mut tr_len) = (0.0, 0.0, 0.0);
        let (mut coast_len, mut active_len) = (0.0, 0.0);
        let mut convergence: Vec<f64> = Vec::new();
        let mut full = 0usize;
        for e in &edges {
            let s = resolve(e, seed);
            let l = e.length();
            match s.regime() {
                BoundaryRegime::Convergent => { conv_len += l; convergence.push(s.convergence); if s.convergence >= CONVERGENCE_FULL { full += 1 } }
                BoundaryRegime::Divergent => div_len += l,
                BoundaryRegime::Transform => tr_len += l,
            }
            if e.is_coast() {
                coast_len += l;
                if s.margin == MarginClass::Active { active_len += l }
            }
        }
        let total = conv_len + div_len + tr_len;

        println!("\n=== seed {seed:#x}: {} plates, spacing {PLATE_SPACING}, continental gate {CONTINENT_GATE} ===", ids.len());
        println!("continental plates {} ({:.1}%); continents by plate count: {}",
            continental, 100.0 * continental as f64 / ids.len() as f64,
            by_size.iter().map(|(s, n)| format!("{s}×{n}")).collect::<Vec<_>>().join(" "));
        let mut ages: Vec<f64> = plates.values().map(|p| p.age).collect();
        let (new, aged) = (ages.iter().filter(|&&a| a <= 0.0).count(), ages.iter().filter(|&&a| a >= 1.0).count());
        println!("age: new {:.1}%, fully aged {:.1}%, p25 {:.2} p50 {:.2} p75 {:.2}",
            100.0 * new as f64 / ids.len() as f64, 100.0 * aged as f64 / ids.len() as f64,
            percentile(&mut ages, 0.25), percentile(&mut ages, 0.5), percentile(&mut ages, 0.75));
        println!("edges {}: length p50 {:.0} p90 {:.0} max {:.0}", edges.len(),
            percentile(&mut lengths, 0.5), percentile(&mut lengths, 0.9), percentile(&mut lengths, 1.0));
        println!("regime by length: convergent {:.1}%  divergent {:.1}%  transform {:.1}%",
            100.0 * conv_len / total, 100.0 * div_len / total, 100.0 * tr_len / total);
        println!("coasts {:.1}% of edge length, active {:.1}% of coast length",
            100.0 * coast_len / total, if coast_len > 0.0 { 100.0 * active_len / coast_len } else { 0.0 });
        if !convergence.is_empty() {
            let n = convergence.len();
            println!("convergence over {n} convergent edges: p10 {:.3} p50 {:.3} p90 {:.3} p99 {:.3} max {:.3}; {} at or past CONVERGENCE_FULL {CONVERGENCE_FULL} ({:.1}%)",
                percentile(&mut convergence, 0.1), percentile(&mut convergence, 0.5), percentile(&mut convergence, 0.9),
                percentile(&mut convergence, 0.99), percentile(&mut convergence, 1.0), full, 100.0 * full as f64 / n as f64);
        }
    }
}
