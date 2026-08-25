//! TiltEvent — regional slope, its grade, its coherence, and what it does to
//! the coastline.
//!
//! Run: cargo test -p world --release --test tilt_probe -- --ignored --nocapture

use world::events::tilt::{TILT_AMPLITUDE, TILT_WAVELENGTH, potential, tilt_at};
use world::substrate_elevation_at;

const SEED: u64 = 0x9E3779B97F4A7C15;
/// Several tilt wavelengths across, so the block holds a population of limbs.
const SPAN: f64 = 400_000.0;
const RISE: f64 = 0.8;

fn pct(v: &[f64], p: f64) -> f64 {
    if v.is_empty() { return f64::NAN }
    v[((v.len() - 1) as f64 * p).round() as usize]
}

fn sorted(v: &[f64]) -> Vec<f64> {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    s
}

fn stats(label: &str, v: Vec<f64>) {
    if v.is_empty() { println!("  {label:<30} none"); return }
    let s = sorted(&v);
    println!("  {label:<30} n {:>7}  p10 {:>8.3}  p50 {:>8.3}  p90 {:>8.3}  max {:>8.3}",
             s.len(), pct(&s, 0.10), pct(&s, 0.50), pct(&s, 0.90), s[s.len() - 1]);
}

/// Tilt contribution at a position, from the substrate beneath it.
fn tilt(wx: f64, wy: f64) -> f64 {
    tilt_at(wx, wy, substrate_elevation_at(wx, wy, SEED), SEED)
}

// ── 1. Contribution, grade, coherence ───────────────────────────────────────

#[test]
#[ignore]
fn grade_and_coherence() {
    println!("\n=== TILT FIELD ===");
    println!("wavelength {TILT_WAVELENGTH:.0} WU, amplitude {TILT_AMPLITUDE:.2} z");
    println!("target grade 0.100%; amplitude derived as grade x (lambda/4) / RISE");

    const N: i32 = 700;
    let mut contrib = Vec::new();
    let mut on_land = Vec::new();
    for i in 0..N {
        for j in 0..N {
            let wx = -SPAN * 0.5 + SPAN * i as f64 / N as f64;
            let wy = -SPAN * 0.5 + SPAN * j as f64 / N as f64;
            let t = tilt(wx, wy);
            contrib.push(t);
            if substrate_elevation_at(wx, wy, SEED) >= 0.0 { on_land.push(t) }
        }
    }
    println!();
    stats("contribution, all, z", contrib.clone());
    stats("contribution, on land, z", on_land.clone());
    let a = sorted(&contrib.iter().map(|v| v.abs()).collect::<Vec<_>>());
    println!("  {:<30} p50 {:.3}  p90 {:.3}  max {:.3}", "magnitude, all, z",
             pct(&a, 0.5), pct(&a, 0.9), a[a.len() - 1]);

    // Grade actually achieved: the potential's own gradient, ungated, over the
    // interior where the gate is saturated.
    let h = 500.0;
    let mut grades = Vec::new();
    for i in 0..N {
        for j in 0..N {
            let wx = -SPAN * 0.5 + SPAN * i as f64 / N as f64;
            let wy = -SPAN * 0.5 + SPAN * j as f64 / N as f64;
            if substrate_elevation_at(wx, wy, SEED) < 9.0 { continue } // gate saturated
            let gx = (potential(wx + h, wy, SEED) - potential(wx - h, wy, SEED)) / (2.0 * h);
            let gy = (potential(wx, wy + h, SEED) - potential(wx, wy - h, SEED)) / (2.0 * h);
            grades.push(gx.hypot(gy) * RISE * 100.0); // percent
        }
    }
    println!();
    stats("regional grade on land, %", grades.clone());
    println!("  target 0.100% — this is the potential's gradient where the gate is full");

    // Rise a continent actually sees: the grade across CONTINENT_CELL_SIZE.
    let g = sorted(&grades);
    println!("  across a 12,500 WU continent that is {:.1} WU of rise at p50, {:.1} at p90",
             pct(&g, 0.5) / 100.0 * 12_500.0, pct(&g, 0.9) / 100.0 * 12_500.0);
}

// ── 2. Directional coherence within a landmass ──────────────────────────────

/// Does a whole landmass lean one way, or does the field turn inside it? This
/// is the acceptance criterion for the wavelength.
#[test]
#[ignore]
fn directional_coherence() {
    println!("\n=== DIRECTIONAL COHERENCE ===");
    const N: usize = 900;
    let step = SPAN / N as f64;
    let at = |i: usize, j: usize| (-SPAN * 0.5 + i as f64 * step, -SPAN * 0.5 + j as f64 * step);

    let mut land = vec![false; N * N];
    for i in 0..N { for j in 0..N {
        let (wx, wy) = at(i, j);
        land[i * N + j] = substrate_elevation_at(wx, wy, SEED) >= 0.0;
    }}

    // Landmasses as connected components of the land mask.
    let mut comp = vec![usize::MAX; N * N];
    let mut comps: Vec<Vec<usize>> = Vec::new();
    for start in 0..N * N {
        if !land[start] || comp[start] != usize::MAX { continue }
        let id = comps.len();
        let mut cells = Vec::new();
        let mut stack = vec![start];
        comp[start] = id;
        while let Some(k) = stack.pop() {
            cells.push(k);
            let (i, j) = (k / N, k % N);
            for (di, dj) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                let (ni, nj) = (i as i64 + di, j as i64 + dj);
                if ni < 0 || nj < 0 || ni >= N as i64 || nj >= N as i64 { continue }
                let nk = ni as usize * N + nj as usize;
                if land[nk] && comp[nk] == usize::MAX { comp[nk] = id; stack.push(nk) }
            }
        }
        comps.push(cells);
    }
    println!("  landmasses found: {}", comps.len());

    let h = 500.0;
    let dir_at = |wx: f64, wy: f64| {
        let gx = (potential(wx + h, wy, SEED) - potential(wx - h, wy, SEED)) / (2.0 * h);
        let gy = (potential(wx, wy + h, SEED) - potential(wx, wy - h, SEED)) / (2.0 * h);
        let m = gx.hypot(gy);
        if m > 1e-15 { Some((gx / m, gy / m)) } else { None }
    };

    let mut spreads = Vec::new();
    let mut worst: (f64, usize, f64) = (0.0, 0, 0.0);
    let mut sized = 0usize;
    for cells in &comps {
        // Only landmasses big enough to be called one: a continent cell is
        // 12,500 WU, so require at least a tenth of that across.
        let area = cells.len() as f64 * step * step;
        if area < (1_250.0f64).powi(2) { continue }
        sized += 1;
        // Mean direction as a doubled-angle average, so it is an axis average
        // and not confused by sign.
        let (mut sx, mut sy, mut n) = (0.0, 0.0, 0.0);
        let mut dirs = Vec::new();
        for &k in cells {
            let (wx, wy) = at(k / N, k % N);
            let Some(d) = dir_at(wx, wy) else { continue };
            sx += d.0; sy += d.1; n += 1.0;
            dirs.push(d);
        }
        if n < 4.0 { continue }
        let m = (sx / n).hypot(sy / n);
        if m < 1e-12 { continue }
        let mean = (sx / n / m, sy / n / m);
        let mut devs: Vec<f64> = dirs.iter()
            .map(|d| (d.0 * mean.0 + d.1 * mean.1).clamp(-1.0, 1.0).acos().to_degrees())
            .collect();
        devs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let p90 = pct(&devs, 0.90);
        spreads.push(p90);
        if p90 > worst.0 { worst = (p90, cells.len(), area.sqrt()) }
    }
    println!("  landmasses at least 1,250 WU across: {sized}");
    stats("p90 deviation from the mean lean, deg", spreads.clone());
    let s = sorted(&spreads);
    println!("  landmasses whose lean holds within 20 deg (p90): {:.1}%",
             100.0 * s.iter().filter(|&&d| d <= 20.0).count() as f64 / s.len().max(1) as f64);
    println!("  within 32 deg: {:.1}%",
             100.0 * s.iter().filter(|&&d| d <= 32.0).count() as f64 / s.len().max(1) as f64);
    println!("  worst: {:.1} deg on a landmass {:.0} WU across", worst.0, worst.2);
}

// ── 3. What it does to the coastline ────────────────────────────────────────

#[test]
#[ignore]
fn coastline_movement() {
    println!("\n=== COASTLINE ===");
    const N: usize = 1100;
    let step = SPAN / N as f64;
    let at = |i: usize, j: usize| (-SPAN * 0.5 + i as f64 * step, -SPAN * 0.5 + j as f64 * step);

    let mut before = vec![false; N * N];
    let mut after = vec![false; N * N];
    let (mut lb, mut la) = (0usize, 0usize);
    for i in 0..N { for j in 0..N {
        let (wx, wy) = at(i, j);
        let s = substrate_elevation_at(wx, wy, SEED);
        let b = s >= 0.0;
        let a = s + tilt_at(wx, wy, s, SEED) >= 0.0;
        before[i * N + j] = b;
        after[i * N + j] = a;
        if b { lb += 1 }
        if a { la += 1 }
    }}
    let total = (N * N) as f64;
    println!("  land fraction before {:.3}%   after {:.3}%   ({:+.3} points)",
             100.0 * lb as f64 / total, 100.0 * la as f64 / total,
             100.0 * (la as f64 - lb as f64) / total);
    let flipped = (0..N * N).filter(|&k| before[k] != after[k]).count();
    println!("  cells that changed side: {flipped} ({:.3}% of the block, {:.2}% of land)",
             100.0 * flipped as f64 / total, 100.0 * flipped as f64 / lb.max(1) as f64);

    // Distance from each before-coastline cell to the nearest after-coastline.
    let coast = |m: &Vec<bool>| -> Vec<bool> {
        let mut c = vec![false; N * N];
        for i in 0..N { for j in 0..N {
            let me = m[i * N + j];
            let edge = (i > 0 && m[(i - 1) * N + j] != me)
                || (i + 1 < N && m[(i + 1) * N + j] != me)
                || (j > 0 && m[i * N + j - 1] != me)
                || (j + 1 < N && m[i * N + j + 1] != me);
            if edge { c[i * N + j] = true }
        }}
        c
    };
    let cb = coast(&before);
    let ca = coast(&after);

    const FAR: f64 = 1e9;
    let mut dist = vec![FAR; N * N];
    for k in 0..N * N { if ca[k] { dist[k] = 0.0 } }
    for _ in 0..2 {
        for i in 0..N { for j in 0..N {
            let mut d = dist[i * N + j];
            if i > 0 { d = d.min(dist[(i - 1) * N + j] + step) }
            if j > 0 { d = d.min(dist[i * N + j - 1] + step) }
            if i > 0 && j > 0 { d = d.min(dist[(i - 1) * N + j - 1] + step * 1.41421356) }
            dist[i * N + j] = d;
        }}
        for i in (0..N).rev() { for j in (0..N).rev() {
            let mut d = dist[i * N + j];
            if i + 1 < N { d = d.min(dist[(i + 1) * N + j] + step) }
            if j + 1 < N { d = d.min(dist[i * N + j + 1] + step) }
            if i + 1 < N && j + 1 < N { d = d.min(dist[(i + 1) * N + j + 1] + step * 1.41421356) }
            dist[i * N + j] = d;
        }}
    }
    let moved: Vec<f64> = (0..N * N).filter(|&k| cb[k]).map(|k| dist[k]).collect();
    stats("coastline displacement, WU", moved.clone());
    let m = sorted(&moved);
    println!("  p99 {:.0} WU   ({:.1}% of a continent)", pct(&m, 0.99),
             100.0 * pct(&m, 0.99) / 12_500.0);

    // Did any landmass flip wholesale?
    println!("\n  a continent is 12,500 WU across; displacement beyond a few percent of");
    println!("  that on the p99 would mean whole margins drowning rather than shifting");
}

// ── 4. Coherence by landmass size, and landmass survival ────────────────────

/// The deviation-from-mean measure degenerates where a landmass straddles a
/// saddle: the vector mean of the gradients is near zero and every deviation is
/// large against a direction that means nothing. The order parameter has no
/// such degeneracy — it is the length of that mean, 1 for perfect alignment and
/// 0 for none — so coherence is reported with it, cut by landmass size.
#[test]
#[ignore]
fn coherence_by_size() {
    println!("\n=== COHERENCE BY LANDMASS SIZE ===");
    const N: usize = 900;
    let step = SPAN / N as f64;
    let at = |i: usize, j: usize| (-SPAN * 0.5 + i as f64 * step, -SPAN * 0.5 + j as f64 * step);

    let mut land = vec![false; N * N];
    for i in 0..N { for j in 0..N {
        let (wx, wy) = at(i, j);
        land[i * N + j] = substrate_elevation_at(wx, wy, SEED) >= 0.0;
    }}
    let comps = components(&land, N);

    let h = 500.0;
    let mut rows: Vec<(f64, f64, usize)> = Vec::new(); // extent, order parameter, cells
    for cells in &comps {
        if cells.len() < 8 { continue }
        let (mut sx, mut sy, mut n) = (0.0, 0.0, 0.0);
        let (mut minx, mut maxx, mut miny, mut maxy) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
        for &k in cells {
            let (wx, wy) = at(k / N, k % N);
            minx = minx.min(wx); maxx = maxx.max(wx);
            miny = miny.min(wy); maxy = maxy.max(wy);
            let gx = (potential(wx + h, wy, SEED) - potential(wx - h, wy, SEED)) / (2.0 * h);
            let gy = (potential(wx, wy + h, SEED) - potential(wx, wy - h, SEED)) / (2.0 * h);
            let m = gx.hypot(gy);
            if m < 1e-15 { continue }
            sx += gx / m; sy += gy / m; n += 1.0;
        }
        if n < 4.0 { continue }
        let order = (sx / n).hypot(sy / n);
        rows.push(((maxx - minx).max(maxy - miny), order, cells.len()));
    }

    println!("  landmasses measured: {}", rows.len());
    println!("  order parameter: 1.00 = every gradient parallel, 0.00 = no shared direction");
    println!("\n  {:<22} {:>7} {:>9} {:>9} {:>9}", "extent across", "count", "p10", "p50", "worst");
    for (lo, hi, label) in [
        (0.0, 3_000.0, "under 3,000 WU"),
        (3_000.0, 6_250.0, "3,000-6,250"),
        (6_250.0, 12_500.0, "6,250-12,500 (half a continent)"),
        (12_500.0, 25_000.0, "12,500-25,000 (1-2 continents)"),
        (25_000.0, f64::MAX, "over 25,000 (a quarter wave)"),
    ] {
        let mut v: Vec<f64> = rows.iter()
            .filter(|(e, _, _)| *e >= lo && *e < hi)
            .map(|(_, o, _)| *o)
            .collect();
        if v.is_empty() { continue }
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!("  {label:<30} {:>5} {:>9.3} {:>9.3} {:>9.3}",
                 v.len(), pct(&v, 0.10), pct(&v, 0.50), v[0]);
    }
}

/// Did tilt drown or raise a whole landmass, or only move margins? A land
/// fraction can shift by a couple of points either by trimming every coast or
/// by sinking a continent, and those are very different outcomes.
#[test]
#[ignore]
fn landmass_survival() {
    println!("\n=== LANDMASS SURVIVAL ===");
    const N: usize = 900;
    let step = SPAN / N as f64;
    let at = |i: usize, j: usize| (-SPAN * 0.5 + i as f64 * step, -SPAN * 0.5 + j as f64 * step);

    let (mut before, mut after) = (vec![false; N * N], vec![false; N * N]);
    for i in 0..N { for j in 0..N {
        let (wx, wy) = at(i, j);
        let s = substrate_elevation_at(wx, wy, SEED);
        before[i * N + j] = s >= 0.0;
        after[i * N + j] = s + tilt_at(wx, wy, s, SEED) >= 0.0;
    }}

    let cb = components(&before, N);
    let ca = components(&after, N);
    let big = |c: &Vec<Vec<usize>>, min_cells: usize| c.iter().filter(|v| v.len() >= min_cells).count();
    // A continent cell is 12,500 WU; at this step that is ~28x28 cells.
    let cont = ((12_500.0 / step) * (12_500.0 / step) * 0.25) as usize;
    println!("  landmasses: {} before, {} after", cb.len(), ca.len());
    println!("  at least a quarter of a continent in area ({cont} cells): {} before, {} after",
             big(&cb, cont), big(&ca, cont));
    let area = |c: &Vec<Vec<usize>>| c.iter().map(|v| v.len()).sum::<usize>() as f64;
    println!("  total land cells: {:.0} before, {:.0} after ({:+.2}%)",
             area(&cb), area(&ca), 100.0 * (area(&ca) / area(&cb) - 1.0));

    // Largest landmasses, before and after.
    let top = |c: &Vec<Vec<usize>>| {
        let mut s: Vec<usize> = c.iter().map(|v| v.len()).collect();
        s.sort_unstable_by(|a, b| b.cmp(a));
        s.into_iter().take(5).collect::<Vec<_>>()
    };
    println!("  five largest, cells: before {:?}", top(&cb));
    println!("                after  {:?}", top(&ca));
}

fn components(mask: &[bool], n: usize) -> Vec<Vec<usize>> {
    let mut comp = vec![usize::MAX; n * n];
    let mut out: Vec<Vec<usize>> = Vec::new();
    for start in 0..n * n {
        if !mask[start] || comp[start] != usize::MAX { continue }
        let id = out.len();
        let mut cells = Vec::new();
        let mut stack = vec![start];
        comp[start] = id;
        while let Some(k) = stack.pop() {
            cells.push(k);
            let (i, j) = (k / n, k % n);
            for (di, dj) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                let (ni, nj) = (i as i64 + di, j as i64 + dj);
                if ni < 0 || nj < 0 || ni >= n as i64 || nj >= n as i64 { continue }
                let nk = ni as usize * n + nj as usize;
                if mask[nk] && comp[nk] == usize::MAX { comp[nk] = id; stack.push(nk) }
            }
        }
        out.push(cells);
    }
    out
}
