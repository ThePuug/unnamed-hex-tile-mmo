//! Seam probe — straight discontinuities in the composed elevation, and
//! whether the composite or the summed fields carry them. A jump the
//! composite has and the sum of fields lacks is the framework's: a cell
//! reading a neighbourhood the next cell does not.
//!
//! Run: cargo test -p world --release --test seam_probe -- --ignored --nocapture


use world::events::dissection::Valleys;
use world::events::drainage::surface_at;
use world::events::plates::Coasts;
use world::events::thrusting::Outlines;
use world::events::Composite;
use world::{hex_to_world, world_to_hex};

const SEED: u64 = 0x9E3779B97F4A7C15;

fn composite() -> Composite {
    Composite::standard(SEED)
}

/// Sweep the spawn belt's window on a coarse grid: the largest jumps in
/// the composite between neighbouring samples, and at each, the jump the
/// summed fields show and how far the two disagree.
#[test]
#[ignore]
fn straight_seams_in_the_belt_window() {
    let c = composite();
    let (cx, cy, half, step) = (0.0, 0.0, 30_000.0, 250.0);
    let outlines = Outlines::in_box(cx, cy, half, SEED);
    let coasts = Coasts::in_box(cx, cy, half, SEED);
    let valleys = Valleys::in_box(cx, cy, half, SEED);
    let n = (2.0 * half / step) as usize;
    let mut comp = vec![0.0f64; n * n];
    let mut field = vec![0.0f64; n * n];
    for j in 0..n {
        for i in 0..n {
            let (x, y) = (cx - half + i as f64 * step, cy - half + j as f64 * step);
            let (q, r) = world_to_hex(x, y);
            comp[j * n + i] = c.tile_at(q, r).elevation;
            let (wx, wy) = hex_to_world(q, r);
            let envelope = surface_at(wx, wy, SEED, &coasts, &outlines);
            field[j * n + i] = envelope - valleys.cut_at(wx, wy, envelope);
        }
    }
    let mut jumps = Vec::new();
    let mut mismatched = 0usize;
    for j in 0..n {
        for i in 0..n {
            let k = j * n + i;
            if (comp[k] - field[k]).abs() > 1.0 { mismatched += 1 }
            if i + 1 < n {
                let dc = (comp[k + 1] - comp[k]).abs();
                let df = (field[k + 1] - field[k]).abs();
                jumps.push((dc, df, comp[k] - field[k], comp[k + 1] - field[k + 1], cx - half + i as f64 * step, cy - half + j as f64 * step));
            }
        }
    }
    jumps.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    println!("\n=== SEAMS, spawn belt window, {n}x{n} samples {step} WU apart ===");
    println!("samples where composite and summed fields disagree by over 1 z: {mismatched} of {}", n * n);
    println!("largest composite jumps (x → x+{step}): composite jump, field jump, composite−field either side, position");
    for (dc, df, m0, m1, x, y) in jumps.iter().take(12) {
        println!("  comp {dc:>7.1}  field {df:>7.1}  diff {m0:>7.1} | {m1:>7.1}  at ({x:>8.0},{y:>8.0})");
    }
    println!("decomposition either side of the three largest: substrate, tilt, thickening_on, relief, cut");
    for (_, _, _, _, x, y) in jumps.iter().take(3) {
        for xx in [*x, x + step] {
            let (q, r) = world_to_hex(xx, *y);
            let (wx, wy) = hex_to_world(q, r);
            let substrate = world::substrate_on(wx, wy, &coasts, SEED);
            let tilt = world::events::tilt::tilt_at(wx, wy, substrate, SEED);
            let base = substrate + tilt;
            let relief = outlines.relief(wx, wy).max(0.0);
            let thick = world::events::thickening::thickening_on(wx, wy, &outlines).max(0.0);
            let plateau = base + thick;
            let cut = valleys.cut_at(wx, wy, plateau + relief);
            println!("  ({wx:>8.0},{wy:>8.0}) substrate {substrate:>7.1}  tilt {tilt:>7.1}  thickening {thick:>7.1}  relief {relief:>7.1}  cut {cut:>7.1}  sum {:>7.1}", plateau + relief - cut);
        }
    }
}

/// The same window at the viewer's own resolution: every horizontal pair of
/// pixels whose composed elevation jumps past what a forelimb can rise in
/// one step, and where they lie.
#[test]
#[ignore]
fn fine_seams_in_the_belt_window() {
    let c = composite();
    let (cx, cy, half, step) = (-48_000.0, 9_000.0, 12_000.0, 8.0);
    let outlines = Outlines::in_box(cx, cy, half, SEED);
    let coasts = Coasts::in_box(cx, cy, half, SEED);
    let valleys = Valleys::in_box(cx, cy, half, SEED);
    let n = (2.0 * half / step) as usize;
    let mut jumps: Vec<(f64, f64, f64)> = Vec::new();
    let mut row = vec![0.0f64; n];
    for j in (0..n).step_by(4) {
        let y = cy - half + j as f64 * step;
        for i in 0..n {
            let (q, r) = world_to_hex(cx - half + i as f64 * step, y);
            row[i] = c.tile_at(q, r).elevation;
        }
        for i in 0..n - 1 {
            let d = (row[i + 1] - row[i]).abs();
            if d > 60.0 { jumps.push((d, cx - half + i as f64 * step, y)) }
        }
    }
    jumps.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    println!("\n=== FINE SEAMS, spawn belt window, every 4th row at {step} WU ===\njumps over 60 z per {step} WU: {}", jumps.len());
    for (d, x, y) in jumps.iter().take(20) {
        println!("  {d:>7.1} z at ({x:>8.0},{y:>8.0})");
    }
    println!("decomposition either side of the three largest: substrate, tilt, thickening_on, relief");
    for (_, x, y) in jumps.iter().take(3) {
        for xx in [*x, x + step] {
            let (q, r) = world_to_hex(xx, *y);
            let (wx, wy) = hex_to_world(q, r);
            let substrate = world::substrate_on(wx, wy, &coasts, SEED);
            let tilt = world::events::tilt::tilt_at(wx, wy, substrate, SEED);
            let base = substrate + tilt;
            let relief = outlines.relief(wx, wy).max(0.0);
            let thick = world::events::thickening::thickening_on(wx, wy, &outlines).max(0.0);
            let plateau = base + thick;
            let cut = valleys.cut_at(wx, wy, plateau + relief);
            let room = outlines.at(wx, wy).map_or("none".to_string(), |(p, _)| format!("{:?}", p.id));
            println!("  ({wx:>8.0},{wy:>8.0}) substrate {substrate:>7.1}  tilt {tilt:>7.1}  thickening {thick:>7.1}  relief {relief:>7.1}  cut {cut:>7.1}  plate {room}  sum {:.1}  composite {:.1}", plateau + relief - cut, c.tile_at(q, r).elevation);

        }
    }
}

