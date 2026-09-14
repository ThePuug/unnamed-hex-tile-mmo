//! Periodic noise. Every function takes unit coordinates `u, v` and a cell
//! count, and repeats at `u = 1`, so a texture built from it tiles by
//! construction. Fractal sums double the cell count per octave, which keeps
//! every octave periodic on the same tile.

use std::f32::consts::{SQRT_2, TAU};

fn hash(ix: i64, iy: i64, seed: u64) -> u64 {
    // Each coordinate is mixed through a full splitmix64 finaliser before the
    // next is folded in; a lighter mix leaves diagonal correlation between
    // lattice points, which shows as streaking.
    fn mix(mut h: u64) -> u64 {
        h = (h ^ (h >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        h = (h ^ (h >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        h ^ (h >> 31)
    }
    let h = mix(seed ^ 0x9E37_79B9_7F4A_7C15);
    let h = mix(h ^ (ix as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    mix(h ^ (iy as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F))
}

/// Hash of a lattice point to [0, 1).
pub fn hash01(ix: i64, iy: i64, seed: u64) -> f32 {
    (hash(ix, iy, seed) >> 40) as f32 / (1u64 << 24) as f32
}

/// A cell id to [0, 1).
pub fn id01(id: u64) -> f32 {
    (id >> 40) as f32 / (1u64 << 24) as f32
}

fn wrap(i: i64, period: i64) -> i64 {
    i.rem_euclid(period)
}

fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// Gradient noise over `cells` lattice cells across the tile, in about
/// [-1, 1].
pub fn perlin(u: f32, v: f32, cells: u32, seed: u64) -> f32 {
    let period = cells as i64;
    let x = u * cells as f32;
    let y = v * cells as f32;
    let x0 = x.floor();
    let y0 = y.floor();
    let fx = x - x0;
    let fy = y - y0;
    let ix = x0 as i64;
    let iy = y0 as i64;
    let grad = |dx: i64, dy: i64, px: f32, py: f32| {
        let a = hash01(wrap(ix + dx, period), wrap(iy + dy, period), seed) * TAU;
        a.cos() * px + a.sin() * py
    };
    let n00 = grad(0, 0, fx, fy);
    let n10 = grad(1, 0, fx - 1.0, fy);
    let n01 = grad(0, 1, fx, fy - 1.0);
    let n11 = grad(1, 1, fx - 1.0, fy - 1.0);
    let s = fade(fx);
    let t = fade(fy);
    let nx0 = n00 + s * (n10 - n00);
    let nx1 = n01 + s * (n11 - n01);
    // Unit gradients bound 2D Perlin at ±sqrt(2)/2; scale to ±1.
    (nx0 + t * (nx1 - nx0)) * SQRT_2
}

/// Fractal sum of `octaves` gradient layers starting at `cells`, each
/// octave twice as fine and `gain` as strong. Normalised to about [-1, 1].
pub fn fbm(u: f32, v: f32, cells: u32, octaves: u32, gain: f32, seed: u64) -> f32 {
    let mut sum = 0.0;
    let mut norm = 0.0;
    let mut amp = 1.0;
    for o in 0..octaves {
        // Each octave is shifted so the lattice rows of one never line up
        // with the next; a shared lattice shows as faint bands.
        let (du, dv) = (hash01(o as i64, 0, seed), hash01(o as i64, 1, seed));
        sum += amp * perlin(u + du, v + dv, cells << o, seed.wrapping_add(o as u64 * 0x9E37_79B9));
        norm += amp;
        amp *= gain;
    }
    sum / norm
}

pub struct Cell {
    /// Distance to the nearest feature point, in cell units.
    pub f1: f32,
    /// Distance to the second nearest.
    pub f2: f32,
    /// Names the nearest cell; the same cell has the same id across the seam.
    pub id: u64,
    /// Names the second nearest cell, so `id ^ id2` names the edge between
    /// them from either side.
    pub id2: u64,
    /// Offset from the nearest feature point, in cell units.
    pub dx: f32,
    pub dy: f32,
}

/// Cellular noise over `cells` cells across the tile, one feature point per
/// cell.
pub fn worley(u: f32, v: f32, cells: u32, seed: u64) -> Cell {
    worley_xy(u, v, cells, cells, seed)
}

/// Cellular noise over `cells_u` by `cells_v` cells across the tile, so
/// cells can run wider than tall.
pub fn worley_xy(u: f32, v: f32, cells_u: u32, cells_v: u32, seed: u64) -> Cell {
    let (period_x, period_y) = (cells_u as i64, cells_v as i64);
    let x = u * cells_u as f32;
    let y = v * cells_v as f32;
    let cx = x.floor() as i64;
    let cy = y.floor() as i64;
    let mut cell = Cell { f1: f32::MAX, f2: f32::MAX, id: 0, id2: 0, dx: 0.0, dy: 0.0 };
    for oy in -1..=1 {
        for ox in -1..=1 {
            let (ix, iy) = (cx + ox, cy + oy);
            let (wx, wy) = (wrap(ix, period_x), wrap(iy, period_y));
            let px = ix as f32 + hash01(wx, wy, seed);
            let py = iy as f32 + hash01(wx, wy, seed ^ 0x5BD1_E995);
            let d = ((x - px).powi(2) + (y - py).powi(2)).sqrt();
            if d < cell.f1 {
                cell.f2 = cell.f1;
                cell.id2 = cell.id;
                cell.f1 = d;
                cell.id = hash(wx, wy, seed);
                cell.dx = x - px;
                cell.dy = y - py;
            } else if d < cell.f2 {
                cell.f2 = d;
                cell.id2 = hash(wx, wy, seed);
            }
        }
    }
    cell
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perlin_repeats_at_the_tile_edge() {
        for i in 0..50 {
            let (u, v) = (i as f32 * 0.0137, i as f32 * 0.0291);
            let a = perlin(u, v, 5, 3);
            assert!((perlin(u + 1.0, v, 5, 3) - a).abs() < 1e-4);
            assert!((perlin(u, v + 1.0, 5, 3) - a).abs() < 1e-4);
        }
    }

    #[test]
    fn worley_repeats_at_the_tile_edge() {
        for i in 0..50 {
            let (u, v) = (i as f32 * 0.0137, i as f32 * 0.0291);
            let a = worley(u, v, 7, 9);
            let b = worley(u + 1.0, v, 7, 9);
            assert!((a.f1 - b.f1).abs() < 1e-4);
            assert_eq!(a.id, b.id);
        }
    }
}
