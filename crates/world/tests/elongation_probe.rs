//! Elongation probe — guards `ELONGATION_GRAD_NORM`, the one normalizer in the
//! regime field that is measured rather than proved.
//!
//! Anisotropy is what makes plate boundaries run parallel to a coast. It used
//! to key off a steepness-40 sigmoid, whose derivative spanned four orders of
//! magnitude and so concentrated all elongation onto the shoreline by itself.
//! The substrate grades instead, and a graded field's gradient spans one order
//! of magnitude — so the constant, not the field, is now what keeps elongation
//! coastal. It was fitted to two properties, and
//! `normalizer_still_hits_its_targets` is where those properties are stated.
//! The `#[ignore]`d tests below are the measurements it was fitted from.

//! Run: cargo test -p world --release --test elongation_probe -- --ignored --nocapture

use world::{raw_regime_noise, substrate_elevation_at, GRAD_STEP, MAX_ELONGATION,
            ELONGATION_GRAD_NORM, RAW_GRAD_MAX};

const SEED: u64 = 0x9E3779B97F4A7C15;

/// The seeds the margin work fixed once. Reused here so a field retune has to
/// clear the same six worlds everywhere it is measured.
const SEEDS: [u64; 6] = [
    0x9E3779B97F4A7C15,
    0x0123_4567_89AB_CDEF,
    0xDEAD_BEEF_CAFE_1235,
    0x5555_AAAA_3333_CCC1,
    0xF0E1_D2C3_B4A5_9687,
    0x0000_0000_0000_0001,
];

/// Percentile of raw-gradient magnitude that `ELONGATION_GRAD_NORM` places at
/// elongation 2.0, and therefore the share of the world above it.
const PINNED_PERCENTILE: f64 = 0.955;

/// Same 4-point stencil `RegimeGradient::at` uses, over an arbitrary field.
fn grad_mag(f: impl Fn(f64, f64) -> f64, wx: f64, wy: f64) -> f64 {
    let gx = f(wx + GRAD_STEP, wy) - f(wx - GRAD_STEP, wy);
    let gy = f(wx, wy + GRAD_STEP) - f(wx, wy - GRAD_STEP);
    (gx * gx + gy * gy).sqrt() / (2.0 * GRAD_STEP)
}

fn raw_grad_for(seed: u64, wx: f64, wy: f64) -> f64 {
    grad_mag(|x, y| raw_regime_noise(x, y, seed), wx, wy)
}

fn raw_grad(wx: f64, wy: f64) -> f64 {
    raw_grad_for(SEED, wx, wy)
}

fn pct(v: &[f64], p: f64) -> f64 {
    let i = ((v.len() - 1) as f64 * p).round() as usize;
    v[i]
}

fn summary(label: &str, v: &mut Vec<f64>) {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!(
        "  {label:<24} p50 {:.3e}  p90 {:.3e}  p99 {:.3e}  max {:.3e}",
        pct(v, 0.50), pct(v, 0.90), pct(v, 0.99), v[v.len() - 1],
    );
}

fn elong(normalized: f64) -> f64 {
    1.0 + (MAX_ELONGATION - 1.0) * normalized.clamp(0.0, 1.0)
}

/// A sorted block of raw-gradient magnitudes.
fn gradient_block(seed: u64, n: i32, step: f64) -> Vec<f64> {
    let origin = -(n as f64) * step * 0.5;
    let mut g = Vec::with_capacity((n * n) as usize);
    for i in 0..n {
        for j in 0..n {
            g.push(raw_grad_for(seed, origin + i as f64 * step, origin + j as f64 * step));
        }
    }
    g.sort_by(|a, b| a.partial_cmp(b).unwrap());
    g
}

// ── The pin ─────────────────────────────────────────────────────────────────

/// `ELONGATION_GRAD_NORM` is empirical, so it can only be trusted as far as the
/// distribution it came from still holds. Retuning the regime field, the
/// continental gate, or the local fBm wavelengths moves that distribution and
/// silently flattens plate strike; this is where that fails loudly instead.
///
/// Two properties, both fitted, both named in the constant's doc comment: how
/// much of the world is elongated, and whether it is the coastal part.
#[test]
fn normalizer_still_hits_its_targets() {
    const N: i32 = 200;
    const STEP: f64 = 200.0;

    // 1. The percentile the constant was derived from, on every seed.
    let expected = ELONGATION_GRAD_NORM / (MAX_ELONGATION - 1.0);
    for seed in SEEDS {
        let g = gradient_block(seed, N, STEP);
        let p = pct(&g, PINNED_PERCENTILE);
        let ratio = p / expected;
        assert!(
            (0.80..=1.25).contains(&ratio),
            "seed {seed:#018x}: raw-gradient p{:.1} is {p:.4e}, {ratio:.2}x the {expected:.4e} \
             ELONGATION_GRAD_NORM was derived from. The regime field changed shape — re-derive \
             the constant from the new distribution and re-check the targets below.",
            100.0 * PINNED_PERCENTILE,
        );
    }

    // 2. Elongation is still a coastal effect, on the seed the targets were fitted on.
    let origin = -(N as f64) * STEP * 0.5;
    let mut elongated = 0usize;
    let mut near_shore = 0usize;
    let mut total = 0usize;
    for i in 0..N {
        for j in 0..N {
            let wx = origin + i as f64 * STEP;
            let wy = origin + j as f64 * STEP;
            total += 1;
            if elong(raw_grad(wx, wy) / ELONGATION_GRAD_NORM) <= 2.0 { continue }
            elongated += 1;
            // Near shore: some point within 500 WU sits on the other side of
            // the datum, so a coastline runs between here and there.
            let here = substrate_elevation_at(wx, wy, SEED) >= 0.0;
            let crosses = [(500.0, 0.0), (-500.0, 0.0), (0.0, 500.0), (0.0, -500.0),
                           (354.0, 354.0), (-354.0, 354.0), (354.0, -354.0), (-354.0, -354.0)]
                .iter()
                .any(|&(dx, dy)| (substrate_elevation_at(wx + dx, wy + dy, SEED) >= 0.0) != here);
            if crosses { near_shore += 1 }
        }
    }

    let share = elongated as f64 / total as f64;
    assert!(
        (0.02..=0.09).contains(&share),
        "{:.2}% of the world is elongated past 2.0; it was fitted to 4.5%. \
         Anisotropy has stopped being a coastal effect.",
        100.0 * share,
    );

    let locality = near_shore as f64 / elongated as f64;
    assert!(
        locality >= 0.80,
        "only {:.1}% of elongated positions are within 500 WU of a shoreline; it was fitted \
         to ~91%. Elongation has drifted off the coasts it exists for.",
        100.0 * locality,
    );
}

// ── The measurements it was fitted from ─────────────────────────────────────

#[test]
#[ignore]
fn elongation_distribution() {
    // 50,000 WU square — four continent cells across, so the block spans
    // several coastlines, interiors and open ocean.
    const N: i32 = 400;
    const STEP: f64 = 125.0;
    let origin = -(N as f64) * STEP * 0.5;

    println!("\n=== elongation distribution ===\n");
    println!("ELONGATION_GRAD_NORM = {ELONGATION_GRAD_NORM:.6e}  (empirical)");
    println!("RAW_GRAD_MAX         = {RAW_GRAD_MAX:.6e}  (product-rule, under-tight by 1.30x)");
    println!("block: {}x{} samples, {STEP} WU step, {:.0} WU span",
             N, N, N as f64 * STEP);

    let mut raws: Vec<f64> = Vec::with_capacity((N * N) as usize);
    for i in 0..N {
        for j in 0..N {
            raws.push(raw_grad(origin + i as f64 * STEP, origin + j as f64 * STEP));
        }
    }

    println!("\n-- gradient magnitude per WU --");
    summary("raw field", &mut raws.clone());

    println!("\n-- elongation --");
    let e: Vec<f64> = raws.iter().map(|g| elong(g / ELONGATION_GRAD_NORM)).collect();
    summary("in use", &mut e.clone());
    let total = e.len() as f64;
    let mean: f64 = e.iter().sum::<f64>() / total;
    println!("  mean {mean:.3}  >2.0 {:.2}%  >4.0 {:.2}%",
             100.0 * e.iter().filter(|&&x| x > 2.0).count() as f64 / total,
             100.0 * e.iter().filter(|&&x| x > 4.0).count() as f64 / total);

    println!("\n-- warp strength (same normalizer; COASTAL_WARP_THRESHOLD = 40) --");
    let w: Vec<f64> = raws.iter()
        .map(|g| (g / ELONGATION_GRAD_NORM).clamp(0.0, 1.0) * world::WARP_STRENGTH_MAX)
        .collect();
    summary("in use", &mut w.clone());
    println!("  over COASTAL_WARP_THRESHOLD: {:.2}%   <- share of plates read as in-transition",
             100.0 * w.iter().filter(|&&x| x > world::COASTAL_WARP_THRESHOLD).count() as f64 / total);

    println!("\n-- against the product-rule bound, for comparison --");
    let eb: Vec<f64> = raws.iter().map(|g| elong(g / RAW_GRAD_MAX)).collect();
    let meanb: f64 = eb.iter().sum::<f64>() / total;
    println!("  mean {meanb:.3}  >2.0 {:.2}%   <- a third of the world, not its coasts",
             100.0 * eb.iter().filter(|&&x| x > 2.0).count() as f64 / total);
}

/// Where elongation lands relative to the shoreline. The gradient *direction*
/// is not in question — the substrate is monotone in the raw field, so the
/// across/along axes point the same way whichever of the two a stencil reads
/// (measured: 0.07° median disagreement). Only the magnitude, and hence where
/// anisotropy applies, depends on the normalizer.
#[test]
#[ignore]
fn elongation_locality() {
    const N: usize = 400;
    const STEP: f64 = 125.0;
    let origin = -(N as f64) * STEP * 0.5;

    println!("\n=== where does elongation land? ===\n");

    let mut land = vec![false; N * N];
    let mut rg = vec![0.0f64; N * N];
    for i in 0..N {
        for j in 0..N {
            let wx = origin + i as f64 * STEP;
            let wy = origin + j as f64 * STEP;
            land[i * N + j] = substrate_elevation_at(wx, wy, SEED) >= 0.0;
            rg[i * N + j] = raw_grad(wx, wy);
        }
    }

    // Chamfer distance (in cells) to the nearest land/water boundary cell.
    const FAR: f64 = 1e9;
    let mut dist = vec![FAR; N * N];
    for i in 0..N {
        for j in 0..N {
            let me = land[i * N + j];
            let edge = (i > 0 && land[(i - 1) * N + j] != me)
                || (i + 1 < N && land[(i + 1) * N + j] != me)
                || (j > 0 && land[i * N + j - 1] != me)
                || (j + 1 < N && land[i * N + j + 1] != me);
            if edge { dist[i * N + j] = 0.0 }
        }
    }
    for _ in 0..2 {
        for i in 0..N {
            for j in 0..N {
                let mut d = dist[i * N + j];
                if i > 0 { d = d.min(dist[(i - 1) * N + j] + 1.0) }
                if j > 0 { d = d.min(dist[i * N + j - 1] + 1.0) }
                if i > 0 && j > 0 { d = d.min(dist[(i - 1) * N + j - 1] + 1.41421356) }
                dist[i * N + j] = d;
            }
        }
        for i in (0..N).rev() {
            for j in (0..N).rev() {
                let mut d = dist[i * N + j];
                if i + 1 < N { d = d.min(dist[(i + 1) * N + j] + 1.0) }
                if j + 1 < N { d = d.min(dist[i * N + j + 1] + 1.0) }
                if i + 1 < N && j + 1 < N { d = d.min(dist[(i + 1) * N + j + 1] + 1.41421356) }
                dist[i * N + j] = d;
            }
        }
    }

    let bands: [(f64, f64, &str); 6] = [
        (0.0, 1.5, "0-190 WU (shore)"),
        (1.5, 4.0, "190-500 WU"),
        (4.0, 8.0, "500-1000 WU"),
        (8.0, 16.0, "1-2 kWU"),
        (16.0, 32.0, "2-4 kWU"),
        (32.0, FAR, "4+ kWU"),
    ];
    println!("{:<18} {:>7}   {:>26}", "distance to shore", "share", "elongation");
    for (lo, hi, label) in bands {
        let idx: Vec<usize> = (0..N * N).filter(|&k| dist[k] >= lo && dist[k] < hi).collect();
        if idx.is_empty() { continue }
        let n = idx.len() as f64;
        let e: Vec<f64> = idx.iter().map(|&k| elong(rg[k] / ELONGATION_GRAD_NORM)).collect();
        let mean: f64 = e.iter().sum::<f64>() / n;
        let o2 = 100.0 * e.iter().filter(|&&x| x > 2.0).count() as f64 / n;
        println!("{label:<18} {:>6.1}%           mean {mean:>5.2}  >2 {o2:>5.1}%",
                 100.0 * n / (N * N) as f64);
    }

    let hot: Vec<usize> = (0..N * N)
        .filter(|&k| elong(rg[k] / ELONGATION_GRAD_NORM) > 2.0)
        .collect();
    let n = hot.len() as f64;
    let mut d: Vec<f64> = hot.iter().map(|&k| dist[k] * STEP).collect();
    d.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!("\nof all positions elongated >2.0: {:.2}% of the world;  \
              within 500 WU of shore {:.1}%;  median dist {:.0} WU",
             100.0 * n / (N * N) as f64,
             100.0 * hot.iter().filter(|&&k| dist[k] < 4.0).count() as f64 / n,
             pct(&d, 0.5));
}

/// The distribution the empirical constant rests on, across seeds. What
/// `normalizer_still_hits_its_targets` asserts, printed rather than checked.
#[test]
#[ignore]
fn raw_gradient_stability_across_seeds() {
    const N: i32 = 250;
    const STEP: f64 = 160.0;

    println!("\n=== raw gradient distribution across seeds ===\n");
    println!("{:<20} {:>10} {:>10} {:>10} {:>10} {:>10}",
             "seed", "p50", "p95.5", "p99", "p99.9", "max");
    for seed in SEEDS {
        let g = gradient_block(seed, N, STEP);
        println!("{seed:#018x} {:>10.3e} {:>10.3e} {:>10.3e} {:>10.3e} {:>10.3e}",
                 pct(&g, 0.50), pct(&g, PINNED_PERCENTILE), pct(&g, 0.99),
                 pct(&g, 0.999), g[g.len() - 1]);
    }
    println!("\nELONGATION_GRAD_NORM / (MAX_ELONGATION - 1) = {:.4e}  <- the pinned p95.5",
             ELONGATION_GRAD_NORM / (MAX_ELONGATION - 1.0));
}
