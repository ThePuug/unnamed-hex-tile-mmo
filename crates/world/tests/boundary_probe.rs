//! Boundary field probe — what the plate motion layer resolves boundaries to.

//! The acceptance question for the layer is structural, not numerical: do
//! convergent boundaries form coherent chains with one steep side along their
//! length, or do they scatter as disconnected segments that disagree about
//! which flank is steep? Everything measured here bears on that.

//! Two adjacencies are used and they answer different questions. *Sharing a
//! plate* is the raw graph: the Voronoi edges around one plate meet at its
//! vertices. *Continuing a belt* is that, restricted to pairs whose strike also
//! lines up — consecutive edges of one plate's hexagon sit ~60 degrees apart
//! and are not a continuation of anything. Only the second says whether a range
//! could run along the field.

//! Run: cargo test -p world --release --test boundary_probe -- --ignored --nocapture

use std::collections::HashMap;
use std::sync::Arc;


use world::PlateCache;
use world::events::Composite;
use world::events::motion::{
    BoundaryRegime, BoundarySegment, MarginClass, MotionEvent, PlateBoundaryIndex,
};
use world::events::plates::PlateEvent;

const SEED: u64 = 0x9E3779B97F4A7C15;

/// Half-width of the sampled block, in tiles. At `MACRO_CELL_SIZE` 900 this is
/// roughly 66 plates across — enough that a belt has room to run and end inside
/// the block rather than being clipped by it.
const BLOCK: i32 = 30_000;

/// Spacing of the tiles materialized to fill the index. Only needs to be finer
/// than a motion cell (radius 1800) so no cell inside the block is missed.
const STEP: i32 = 1_500;

/// Cosine of the largest strike difference two segments may have and still
/// count as continuing one another. 0.85 is 32 degrees.
const BELT_ALIGNMENT: f64 = 0.85;

fn sampled_block() -> Vec<BoundarySegment> {
    sampled_block_for(SEED)
}

fn sampled_block_for(seed: u64) -> Vec<BoundarySegment> {
    let cache = Arc::new(PlateCache::new(seed));
    let mut c = Composite::new(seed);
    c.add_event(Box::new(PlateEvent::with_cache(cache.clone())));
    c.add_event(Box::new(MotionEvent::with_cache(cache, seed)));

    let mut coords = Vec::new();
    let mut q = -BLOCK;
    while q <= BLOCK {
        let mut r = -BLOCK;
        while r <= BLOCK {
            coords.push((q, r));
            r += STEP;
        }
        q += STEP;
    }
    c.tiles_at(&coords);

    let mut segs: Vec<BoundarySegment> = c.with_indexes(|ix| {
        ix.get::<PlateBoundaryIndex>()
            .map(|idx| idx.cells.values().flat_map(|v| v.iter().cloned()).collect())
            .unwrap_or_default()
    });
    segs.sort_by_key(|s| (s.plate_a, s.plate_b));
    segs
}

// ── Shared graph helpers ────────────────────────────────────────────────────

fn strike_dot(a: &BoundarySegment, b: &BoundarySegment) -> f64 {
    (a.strike_x * b.strike_x + a.strike_y * b.strike_y).abs()
}

/// Whether a margin segment's vergence points at its own ocean.
///
/// `None` for a boundary that is not a continent–ocean margin, which is what
/// separates the two frames the vergence rules are coherent in.
fn seaward(s: &BoundarySegment) -> Option<bool> {
    let ocean = match (s.elev_a < 0.0, s.elev_b < 0.0) {
        (false, true) => (s.bx - s.mx, s.by - s.my),
        (true, false) => (s.ax - s.mx, s.ay - s.my),
        _ => return None,
    };
    Some(s.vergence_x * ocean.0 + s.vergence_y * ocean.1 > 0.0)
}

/// Index pairs sharing a plate, over a pre-filtered slice.
fn adjacent_pairs(picked: &[&BoundarySegment]) -> Vec<(usize, usize)> {
    let mut by_plate: HashMap<u64, Vec<usize>> = HashMap::new();
    for (i, s) in picked.iter().enumerate() {
        by_plate.entry(s.plate_a).or_default().push(i);
        by_plate.entry(s.plate_b).or_default().push(i);
    }
    let mut pairs = Vec::new();
    for members in by_plate.values() {
        for i in 0..members.len() {
            for j in (i + 1)..members.len() {
                pairs.push((members[i], members[j]));
            }
        }
    }
    pairs
}

/// Connected components of the sub-graph, reported by size.
fn run_sizes(picked: &[&BoundarySegment], belt_aligned_only: bool) -> Vec<usize> {
    let mut parent: Vec<usize> = (0..picked.len()).collect();
    fn find(parent: &mut Vec<usize>, mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for (i, j) in adjacent_pairs(picked) {
        if belt_aligned_only && strike_dot(picked[i], picked[j]) < BELT_ALIGNMENT {
            continue;
        }
        let (a, b) = (find(&mut parent, i), find(&mut parent, j));
        if a != b { parent[a] = b; }
    }
    let mut counts: HashMap<usize, usize> = HashMap::new();
    for i in 0..picked.len() {
        let root = find(&mut parent, i);
        *counts.entry(root).or_insert(0) += 1;
    }
    let mut sizes: Vec<usize> = counts.into_values().collect();
    sizes.sort_unstable_by(|a, b| b.cmp(a));
    sizes
}

fn report_runs(label: &str, sizes: &[usize]) {
    if sizes.is_empty() {
        println!("  {label:<12} none");
        return;
    }
    let total: usize = sizes.iter().sum();
    let singletons = sizes.iter().filter(|&&n| n == 1).count();
    let mean = total as f64 / sizes.len() as f64;
    // Share of segments in a run of 4 or more — long enough to carry a range
    // rather than a bump.
    let in_belts: usize = sizes.iter().filter(|&&n| n >= 4).sum();
    let head: Vec<usize> = sizes.iter().take(10).copied().collect();
    println!(
        "  {label:<12} {total:>5} segs in {:>4} runs — mean {mean:>5.1}, median {:>3}, \
         longest {:>4}, singleton runs {:>3.0}%, segs in runs>=4 {:>3.0}%",
        sizes.len(),
        sizes[sizes.len() / 2],
        sizes[0],
        100.0 * singletons as f64 / sizes.len() as f64,
        100.0 * in_belts as f64 / total as f64,
    );
    println!("  {:<12} longest: {head:?}", "");
}

fn percentiles(mut v: Vec<f64>) -> String {
    if v.is_empty() { return "none".into(); }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let at = |p: f64| v[((v.len() - 1) as f64 * p).round() as usize];
    format!(
        "min {:.4}, p10 {:.4}, p25 {:.4}, median {:.4}, p75 {:.4}, p90 {:.4}, max {:.4}",
        v[0], at(0.10), at(0.25), at(0.50), at(0.75), at(0.90), v[v.len() - 1]
    )
}

#[test]
#[ignore]
fn boundary_field_report() {
    let segs = sampled_block();
    let n = segs.len() as f64;
    println!("\n=== Plate boundary field over a {}x{} tile block: {} segments ===",
        BLOCK * 2, BLOCK * 2, segs.len());
    assert!(!segs.is_empty(), "no boundaries resolved");

    let count = |f: &dyn Fn(&BoundarySegment) -> bool| segs.iter().filter(|s| f(s)).count();
    let pick = |f: &dyn Fn(&BoundarySegment) -> bool| -> Vec<&BoundarySegment> {
        segs.iter().filter(|s| f(s)).collect()
    };

    // ── Regime shares ──
    let conv = count(&|s| s.regime() == BoundaryRegime::Convergent);
    let div = count(&|s| s.regime() == BoundaryRegime::Divergent);
    let trans = count(&|s| s.regime() == BoundaryRegime::Transform);
    println!("\nRegime share");
    println!("  convergent          {conv:>6}  {:>5.1}%", 100.0 * conv as f64 / n);
    println!("  divergent           {div:>6}  {:>5.1}%", 100.0 * div as f64 / n);
    println!("  transform-dominant  {trans:>6}  {:>5.1}%", 100.0 * trans as f64 / n);

    // ── Magnitude ──
    println!("\nMagnitude distribution");
    println!("  |convergence|  {}", percentiles(segs.iter().map(|s| s.convergence.abs()).collect()));
    println!("  convergent     {}", percentiles(
        segs.iter().filter(|s| s.convergence > 0.0).map(|s| s.convergence).collect()));
    println!("  transform      {}", percentiles(segs.iter().map(|s| s.transform).collect()));

    // ── Margins ──
    let active = count(&|s| s.margin == MarginClass::Active);
    let passive = count(&|s| s.margin == MarginClass::Passive);
    let margins = active + passive;
    println!("\nContinent-ocean margins: {margins} of {} segments ({:.1}%)",
        segs.len(), 100.0 * margins as f64 / n);
    if margins > 0 {
        println!("  active  {active:>6}  {:>5.1}% of margins", 100.0 * active as f64 / margins as f64);
        println!("  passive {passive:>6}  {:>5.1}% of margins", 100.0 * passive as f64 / margins as f64);
    }

    // ── Runs, both adjacencies ──
    println!("\nRuns of same-sign segments, linked by a shared plate");
    report_runs("convergent", &run_sizes(&pick(&|s| s.convergence > 0.0), false));
    report_runs("divergent", &run_sizes(&pick(&|s| s.convergence <= 0.0), false));
    println!("\nRuns of same-sign segments, linked by shared plate AND aligned strike");
    report_runs("convergent", &run_sizes(&pick(&|s| s.convergence > 0.0), true));
    report_runs("divergent", &run_sizes(&pick(&|s| s.convergence <= 0.0), true));
    println!("\nRuns of same-class margin segments, linked by a shared plate");
    report_runs("active", &run_sizes(&pick(&|s| s.margin == MarginClass::Active), false));
    report_runs("passive", &run_sizes(&pick(&|s| s.margin == MarginClass::Passive), false));

    // ── Margin class coherence ──
    // A margin that flips active/passive along its length produces a range that
    // appears and vanishes down the coast. Measured over margin segments that
    // touch, whatever class each resolved to.
    let margin_segs = pick(&|s| s.margin != MarginClass::Interior);
    let mut mpairs = 0usize;
    let mut magree = 0usize;
    for (i, j) in adjacent_pairs(&margin_segs) {
        mpairs += 1;
        if margin_segs[i].margin == margin_segs[j].margin { magree += 1; }
    }
    println!("\nMargin class agreement between touching margin segments");
    println!("  {magree} of {mpairs} pairs agree ({:.1}%)",
        100.0 * magree as f64 / mpairs.max(1) as f64);

    // ── Vergence coherence — the gate ──
    // Two populations, two frames, because the two rules are different.
    //
    // Interior boundaries pick a side by projecting the drift onto the normal,
    // so world coordinates are the frame they are coherent in and the
    // world-frame dot product is the right test.
    //
    // Margins verge at their own ocean. Two margin segments on opposite sides
    // of one plate — which is what aligned strike selects for — face different
    // water and point apart in world coordinates while both being correct. The
    // frame that carries their rule is the local ocean direction, so that is
    // what they are compared against: each segment's vergence is reduced to
    // "seaward or landward" first, and the pair agrees when both read the same.
    let convergent = pick(&|s| s.convergence > 0.0);
    let mut turn_sum = 0.0;
    let mut turn_n = 0usize;
    let (mut interior, mut margin, mut mixed) = ((0usize, 0usize), (0usize, 0usize), (0usize, 0usize));
    for (i, j) in adjacent_pairs(&convergent) {
        let (a, b) = (convergent[i], convergent[j]);
        let d = strike_dot(a, b);
        if d < BELT_ALIGNMENT { continue; }
        turn_sum += d.min(1.0).acos().to_degrees();
        turn_n += 1;

        match (seaward(a), seaward(b)) {
            // Both margins: compare in the ocean frame.
            (Some(sa), Some(sb)) => {
                margin.1 += 1;
                if sa == sb { margin.0 += 1; }
            }
            // Neither is a margin: world frame is the frame the drift rule works in.
            (None, None) => {
                interior.1 += 1;
                if a.vergence_x * b.vergence_x + a.vergence_y * b.vergence_y > 0.0 { interior.0 += 1; }
            }
            // One of each. No shared frame — this is the arc/interior seam, and
            // the world-frame answer is the honest one: the two rules genuinely
            // pick sides independently here.
            _ => {
                mixed.1 += 1;
                if a.vergence_x * b.vergence_x + a.vergence_y * b.vergence_y > 0.0 { mixed.0 += 1; }
            }
        }
    }
    let mean_turn = turn_sum / turn_n.max(1) as f64;
    let pct = |t: (usize, usize)| 100.0 * t.0 as f64 / t.1.max(1) as f64;
    println!("\nVergence agreement between belt-continuing convergent segments");
    println!("  interior pairs, world frame  {:>4} of {:>4}  {:>5.1}%",
        interior.0, interior.1, pct(interior));
    println!("    their normals turn {mean_turn:.1} deg on average, so a rule picking a side by");
    println!("    projecting any field onto the normal cannot beat {:.1}%",
        100.0 * (1.0 - mean_turn / 180.0));
    println!("  margin pairs, ocean frame    {:>4} of {:>4}  {:>5.1}%",
        margin.0, margin.1, pct(margin));
    println!("    (both seaward, or both landward — the world-frame dot product is");
    println!("     meaningless here: opposite edges of a plate face different water)");
    println!("  arc/interior seam pairs      {:>4} of {:>4}  {:>5.1}%",
        mixed.0, mixed.1, pct(mixed));
    println!("    (no shared frame; belt-level reconciliation belongs to the orogen layer)");

    // ── Coast-parallel check ──
    // The claim is that AnisoContext elongates coastal plates along the shore,
    // so margin boundaries inherit coast-parallel strike without this layer
    // producing it. Measured against the substrate gradient, which points straight
    // out to sea: a coast-parallel boundary has its normal along that gradient.
    let cache = PlateCache::new(SEED);
    let h = 64.0;
    let mut margin_align = Vec::new();
    let mut interior_align = Vec::new();
    for s in &segs {
        let gx = cache.substrate_elevation_at(s.mx + h, s.my) - cache.substrate_elevation_at(s.mx - h, s.my);
        let gy = cache.substrate_elevation_at(s.mx, s.my + h) - cache.substrate_elevation_at(s.mx, s.my - h);
        let g = gx.hypot(gy);
        if g <= 0.0 { continue; }
        let sep = s.separation();
        let along = ((gx / g) * (s.bx - s.ax) / sep + (gy / g) * (s.by - s.ay) / sep).abs();
        if s.margin == MarginClass::Interior { interior_align.push(along); }
        else { margin_align.push(along); }
    }
    println!("\nBoundary normal against the regime gradient (1.0 = coast-parallel boundary)");
    println!("  margins  {}", percentiles(margin_align));
    println!("  interior {}", percentiles(interior_align));
    println!("  a uniformly random orientation gives median 0.707");
    println!();
}

/// Six seeds, chosen once and fixed. Odd 64-bit constants with no shared
/// factors, so no two sample the same phase of the margin field.
const SEEDS: [u64; 6] = [
    0x9E3779B97F4A7C15,
    0x0123_4567_89AB_CDEF,
    0xDEAD_BEEF_CAFE_1235,
    0x5555_AAAA_3333_CCC1,
    0xF0E1_D2C3_B4A5_9687,
    0x0000_0000_0000_0001,
];

/// The active/passive split is decided outright by one long-wavelength field,
/// so a single block on a single seed says nothing about whether both classes
/// occur — it says where that seed's block happens to sit in the field's phase.
///
/// A seed whose coasts are all active, or all passive, is a defect: every coast
/// carrying a range is as wrong as none doing so.
#[test]
#[ignore]
fn margin_mix_across_seeds() {
    println!("\n=== Margin mix across {} seeds, {}x{} tile block each ===",
        SEEDS.len(), BLOCK * 2, BLOCK * 2);

    let mut degenerate = Vec::new();
    for seed in SEEDS {
        let segs = sampled_block_for(seed);
        let pick = |f: &dyn Fn(&BoundarySegment) -> bool| -> Vec<&BoundarySegment> {
            segs.iter().filter(|s| f(s)).collect()
        };
        let active = pick(&|s| s.margin == MarginClass::Active);
        let passive = pick(&|s| s.margin == MarginClass::Passive);
        let margins = active.len() + passive.len();

        println!("\nseed {seed:#018x} — {} segments, {margins} margin ({:.1}%)",
            segs.len(), 100.0 * margins as f64 / segs.len() as f64);
        if margins == 0 {
            println!("  no continent-ocean margins in this block");
            degenerate.push((seed, "no margins"));
            continue;
        }
        let share = 100.0 * active.len() as f64 / margins as f64;
        println!("  active  {:>5}  {share:>5.1}%", active.len());
        println!("  passive {:>5}  {:>5.1}%", passive.len(), 100.0 - share);
        report_runs("active", &run_sizes(&active, false));
        report_runs("passive", &run_sizes(&passive, false));

        // Both classes must occur substantially, not merely at all. Below this
        // a coast is effectively one class with a few stragglers.
        if share < 10.0 || share > 90.0 {
            degenerate.push((seed, "one class dominates"));
        }
    }

    println!();
    assert!(
        degenerate.is_empty(),
        "degenerate seeds: {degenerate:?} — a world where every coast has a range, \
         or none does, is a defect rather than variance"
    );
}
