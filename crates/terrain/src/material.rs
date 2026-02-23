// ──── Simplex Noise ────

const F2: f64 = 0.36602540378443864; // (sqrt(3) - 1) / 2
const G2: f64 = 0.21132486540518713; // (3 - sqrt(3)) / 6

/// 12 unit gradient vectors at 30° intervals — isotropic, no directional bias.
const GRAD2: [[f64; 2]; 12] = [
    [ 1.0,                 0.0],                //   0°
    [ 0.8660254037844387,  0.5],                //  30°
    [ 0.5,                 0.8660254037844387],  //  60°
    [ 0.0,                 1.0],                //  90°
    [-0.5,                 0.8660254037844387],  // 120°
    [-0.8660254037844387,  0.5],                // 150°
    [-1.0,                 0.0],                // 180°
    [-0.8660254037844387, -0.5],                // 210°
    [-0.5,                -0.8660254037844387],  // 240°
    [ 0.0,                -1.0],                // 270°
    [ 0.5,                -0.8660254037844387],  // 300°
    [ 0.8660254037844387, -0.5],                // 330°
];

/// 2D simplex noise. Returns a value in approximately [-1, 1].
pub(crate) fn simplex_2d(x: f64, y: f64, seed: u64) -> f64 {
    let s = (x + y) * F2;
    let i = (x + s).floor() as i64;
    let j = (y + s).floor() as i64;

    let t = (i + j) as f64 * G2;
    let x0 = x - (i as f64 - t);
    let y0 = y - (j as f64 - t);

    let (i1, j1) = if x0 > y0 { (1i64, 0i64) } else { (0i64, 1i64) };

    let x1 = x0 - i1 as f64 + G2;
    let y1 = y0 - j1 as f64 + G2;
    let x2 = x0 - 1.0 + 2.0 * G2;
    let y2 = y0 - 1.0 + 2.0 * G2;

    let mut n = 0.0;

    let t0 = 0.5 - x0 * x0 - y0 * y0;
    if t0 > 0.0 {
        let t0 = t0 * t0;
        let gi = grad_index(i, j, seed);
        n += t0 * t0 * (GRAD2[gi][0] * x0 + GRAD2[gi][1] * y0);
    }

    let t1 = 0.5 - x1 * x1 - y1 * y1;
    if t1 > 0.0 {
        let t1 = t1 * t1;
        let gi = grad_index(i + i1, j + j1, seed);
        n += t1 * t1 * (GRAD2[gi][0] * x1 + GRAD2[gi][1] * y1);
    }

    let t2 = 0.5 - x2 * x2 - y2 * y2;
    if t2 > 0.0 {
        let t2 = t2 * t2;
        let gi = grad_index(i + 1, j + 1, seed);
        n += t2 * t2 * (GRAD2[gi][0] * x2 + GRAD2[gi][1] * y2);
    }

    70.0 * n
}

pub(crate) fn grad_index(ix: i64, iy: i64, seed: u64) -> usize {
    let h = seed
        .wrapping_mul(0x517cc1b727220a95)
        .wrapping_add(ix as u64)
        .wrapping_mul(0xff51afd7ed558ccd)
        .wrapping_add(iy as u64);
    let h = h ^ (h >> 33);
    (h as usize) % 12
}

// ──── Material Density ────

/// Discordant noise waves for material distribution.
/// Wavelengths at near-golden-ratio spacing (~1.618x) are incommensurate —
/// their sum never repeats, producing unique structure everywhere without
/// requiring any single wavelength to be enormous.
/// (wavelength in tiles, relative amplitude)
pub(crate) const MATERIAL_WAVES: [(f64, f64); 3] = [
    (12_547.0, 1.0),  // ~2x cell size — regional variation
    (20_297.0, 0.7),  // ~3.4x cell size — provincial character
    (32_833.0, 0.5),  // ~5.5x cell size — broad structure
];

/// How much material density varies from the midpoint (0.0–0.5).
/// 0.0 = perfectly uniform material, no cell size variation.
/// 0.5 = maximum contrast between dense and light regions.
pub(crate) const MATERIAL_AMPLITUDE: f64 = 0.8;

/// Power curve exponent for province separation.
/// 1.0 = no change. <1.0 = more contrast. 0.7 = moderate.
const MATERIAL_CONTRAST: f64 = 0.7;

/// Material density at cartesian coordinates.
/// All callers must convert hex → cartesian before calling.
pub(crate) fn material_density_cart(cx: f64, cy: f64, seed: u64) -> f64 {
    let base_seed = seed ^ 0xDEAD_BEEF_CAFE_BABE;

    let mut total = 0.0;
    let mut max_value = 0.0;

    for (i, &(wavelength, amplitude)) in MATERIAL_WAVES.iter().enumerate() {
        let wave_seed = base_seed.wrapping_add((i as u64).wrapping_mul(0x9E3779B97F4A7C15));
        total += simplex_2d(cx / wavelength, cy / wavelength, wave_seed) * amplitude;
        max_value += amplitude;
    }

    let normalized = (total / max_value + 1.0) / 2.0;
    let centered = normalized - 0.5;
    let contrasted = centered.signum() * centered.abs().powf(MATERIAL_CONTRAST);
    (0.5 + contrasted * MATERIAL_AMPLITUDE).clamp(0.0, 1.0)
}

// ──── Tests ────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simplex_noise_range() {
        let mut min_val = f64::MAX;
        let mut max_val = f64::MIN;
        let seed = 42;
        for x in (-50000..50000).step_by(37) {
            for y in (-50000..50000).step_by(41) {
                let v = simplex_2d(x as f64 / 10000.0, y as f64 / 10000.0, seed);
                if v < min_val { min_val = v; }
                if v > max_val { max_val = v; }
            }
        }
        eprintln!("simplex_2d range: [{:.4}, {:.4}]", min_val, max_val);
        assert!(min_val >= -1.0 && max_val <= 1.0,
            "simplex_2d exceeds [-1, 1]: [{:.4}, {:.4}]", min_val, max_val);
    }
}
