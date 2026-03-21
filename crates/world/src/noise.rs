// ──── Deterministic Hashing ────

/// Deterministic hash of two integer coordinates + seed.
/// FNV-1a variant with 3-round mixing. Proven well-distributed
/// across the old terrain pipeline (commit 6a3bcbc).
pub(crate) fn hash_u64(a: i64, b: i64, seed: u64) -> u64 {
    let mut h = seed ^ 0x517cc1b727220a95;
    h = h.wrapping_mul(0x517cc1b727220a95).wrapping_add(a as u64);
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51afd7ed558ccd).wrapping_add(b as u64);
    h ^= h >> 33;
    h = h.wrapping_mul(0xc4ceb9fe1a85ec53);
    h ^= h >> 33;
    h
}

/// Hash to f64 in [0, 1).
pub(crate) fn hash_f64(a: i64, b: i64, seed: u64) -> f64 {
    let h = hash_u64(a, b, seed);
    (h >> 11) as f64 / (1u64 << 53) as f64
}

/// Hash with a channel parameter for generating multiple independent
/// values from the same coordinates.
#[allow(dead_code)]
pub(crate) fn hash_channel(a: i64, b: i64, seed: u64, channel: u64) -> u64 {
    hash_u64(a, b, seed ^ channel.wrapping_mul(0x9E3779B97F4A7C15))
}

/// Hash with channel to f64 in [0, 1).
#[allow(dead_code)]
pub(crate) fn hash_channel_f64(a: i64, b: i64, seed: u64, channel: u64) -> f64 {
    let h = hash_channel(a, b, seed, channel);
    (h >> 11) as f64 / (1u64 << 53) as f64
}

// ──── Simplex Noise ────

const F2: f64 = 0.36602540378443864; // (sqrt(3) - 1) / 2
const G2: f64 = 0.21132486540518713; // (3 - sqrt(3)) / 6

/// 12 unit gradient vectors at 30° intervals — isotropic, no directional bias.
const GRAD2: [[f64; 2]; 12] = [
    [ 1.0,                 0.0],
    [ 0.8660254037844387,  0.5],
    [ 0.5,                 0.8660254037844387],
    [ 0.0,                 1.0],
    [-0.5,                 0.8660254037844387],
    [-0.8660254037844387,  0.5],
    [-1.0,                 0.0],
    [-0.8660254037844387, -0.5],
    [-0.5,                -0.8660254037844387],
    [ 0.0,                -1.0],
    [ 0.5,                -0.8660254037844387],
    [ 0.8660254037844387, -0.5],
];

fn grad_index(ix: i64, iy: i64, seed: u64) -> usize {
    let h = seed
        .wrapping_mul(0x517cc1b727220a95)
        .wrapping_add(ix as u64)
        .wrapping_mul(0xff51afd7ed558ccd)
        .wrapping_add(iy as u64);
    let h = h ^ (h >> 33);
    (h as usize) % 12
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_deterministic() {
        for a in -100..100 {
            for b in -100..100 {
                assert_eq!(hash_u64(a, b, 42), hash_u64(a, b, 42));
            }
        }
    }

    #[test]
    fn hash_f64_in_unit_range() {
        for a in -500..500 {
            for b in -500..500 {
                let v = hash_f64(a, b, 42);
                assert!(v >= 0.0 && v < 1.0, "hash_f64({a}, {b}) = {v} out of [0, 1)");
            }
        }
    }

    #[test]
    fn different_seeds_produce_different_hashes() {
        let mut differ = 0;
        for a in 0..100 {
            for b in 0..100 {
                if hash_u64(a, b, 0) != hash_u64(a, b, 99999) {
                    differ += 1;
                }
            }
        }
        assert!(differ > 9900, "Different seeds should produce mostly different hashes");
    }

    #[test]
    fn simplex_noise_range() {
        let mut min_val = f64::MAX;
        let mut max_val = f64::MIN;
        for x in (-50000..50000).step_by(37) {
            for y in (-50000..50000).step_by(41) {
                let v = simplex_2d(x as f64 / 10000.0, y as f64 / 10000.0, 42);
                if v < min_val { min_val = v; }
                if v > max_val { max_val = v; }
            }
        }
        assert!(min_val >= -1.0 && max_val <= 1.0,
            "simplex_2d exceeds [-1, 1]: [{min_val:.4}, {max_val:.4}]");
    }

    #[test]
    fn channels_are_independent() {
        let mut differ = 0;
        for a in 0..50 {
            for b in 0..50 {
                if hash_channel(a, b, 42, 0) != hash_channel(a, b, 42, 1) {
                    differ += 1;
                }
            }
        }
        assert!(differ > 2400, "Different channels should produce mostly different values");
    }
}
