//! The room the music plays in: a sixteen-line feedback delay network
//! behind a pre-delay and a diffuser. The synthesizer's own reverb is a
//! fixed small room that rings out in about a second, too dry for a
//! bed; this one rings for as long as the score asks. Pure arithmetic
//! over a buffer, so the same send gives the same room.

use crate::render::SAMPLE_RATE;

/// Delay lines, in samples: primes on a geometric run from 29 ms to
/// 83 ms, so no two lines' echoes fall together and the tail is dense.
const LINES: [usize; 16] = [1409, 1511, 1613, 1733, 1861, 1987, 2131, 2287, 2459, 2633, 2819, 3023, 3251, 3491, 3733, 4001];

/// Series allpasses per side, in samples: they smear an onset into a
/// wash before it enters the lines, so a pluck does not flutter.
const DIFFUSERS: [[usize; 4]; 2] = [[229, 173, 457, 331], [241, 181, 467, 347]];
const DIFFUSION: f32 = 0.625;

/// The gap before the room answers: long enough that the dry note
/// stays clear, short enough that the two are heard as one sound.
const PRE_DELAY_S: f32 = 0.03;

/// The highs die out in this share of the lows' time, as in a hall of
/// stone and cloth.
const HIGH_DECAY: f32 = 0.45;

/// The send is band-limited before the room: under the low corner is
/// rumble that muddies the drone, over the high corner hiss.
const LOW_CORNER_HZ: f32 = 150.0;
const HIGH_CORNER_HZ: f32 = 6000.0;

/// The room's answer to `send`, as long as it: the tail past the end of
/// `send` is dropped, so the caller leaves room for it. `seconds` is the
/// low tones' time to fall 60 dB.
pub fn ring(send: &[[f32; 2]], seconds: f32) -> Vec<[f32; 2]> {
    let fs = SAMPLE_RATE as f32;
    let one_pole = |hz: f32| (-std::f32::consts::TAU * hz / fs).exp();
    let (hp, lp) = (one_pole(LOW_CORNER_HZ), one_pole(HIGH_CORNER_HZ));

    let mut lines: Vec<Delay> = LINES.iter().map(|&n| Delay::new(n)).collect();
    // Each line loses what its length takes of the fall: the lows by
    // `gain`, and a one-pole lowpass in the loop takes the highs down to
    // their shorter time.
    let absorb: Vec<(f32, f32)> = LINES
        .iter()
        .map(|&n| {
            let per_second = |t: f32| 10f32.powf(-3.0 * n as f32 / (fs * t));
            let gain = per_second(seconds);
            let ratio = per_second(seconds * HIGH_DECAY) / gain;
            (gain, (1.0 - ratio) / (1.0 + ratio))
        })
        .collect();
    let mut damp = [0.0f32; 16];
    let mut diffusers: Vec<Vec<Delay>> = DIFFUSERS.iter().map(|side| side.iter().map(|&n| Delay::new(n)).collect()).collect();
    let mut pre = [Delay::new((PRE_DELAY_S * fs) as usize), Delay::new((PRE_DELAY_S * fs) as usize)];
    let mut band = [[0.0f32; 2]; 2];

    let mut out = Vec::with_capacity(send.len());
    for s in send {
        let mut input = [0.0f32; 2];
        for ch in 0..2 {
            // x → highpass (input minus its lowpass) → lowpass.
            let [low, high] = &mut band[ch];
            *low = (1.0 - hp) * s[ch] + hp * *low;
            *high = (1.0 - lp) * (s[ch] - *low) + lp * *high;
            let mut x = pre[ch].shift(*high);
            for d in &mut diffusers[ch] {
                let delayed = d.peek();
                let v = x + DIFFUSION * delayed;
                d.shift(v);
                x = delayed - DIFFUSION * v;
            }
            input[ch] = x;
        }

        let mut y = [0.0f32; 16];
        for i in 0..16 {
            let (gain, b) = absorb[i];
            damp[i] = (1.0 - b) * lines[i].peek() + b * damp[i];
            y[i] = gain * damp[i];
        }
        let (mut l, mut r) = (0.0, 0.0);
        for k in 0..8 {
            let sign = if k % 2 == 0 { 1.0 } else { -1.0 };
            l += sign * y[2 * k];
            r += sign * y[2 * k + 1];
        }
        out.push([l, r]);

        hadamard(&mut y);
        for i in 0..16 {
            lines[i].shift(y[i] * 0.25 + input[i % 2]);
        }
    }
    out
}

/// The unnormalised 16-point Walsh–Hadamard transform, in place; a
/// quarter of it is orthogonal, so the loop loses only what `absorb`
/// takes.
fn hadamard(v: &mut [f32; 16]) {
    let mut h = 1;
    while h < 16 {
        for i in (0..16).step_by(2 * h) {
            for j in i..i + h {
                let (a, b) = (v[j], v[j + h]);
                v[j] = a + b;
                v[j + h] = a - b;
            }
        }
        h *= 2;
    }
}

struct Delay {
    buf: Vec<f32>,
    at: usize,
}

impl Delay {
    fn new(n: usize) -> Self {
        Delay { buf: vec![0.0; n.max(1)], at: 0 }
    }

    /// The sample written `n` samples ago.
    fn peek(&self) -> f32 {
        self.buf[self.at]
    }

    /// Writes `x` and returns the sample it displaces.
    fn shift(&mut self, x: f32) -> f32 {
        let old = std::mem::replace(&mut self.buf[self.at], x);
        self.at = (self.at + 1) % self.buf.len();
        old
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sound in the middle comes back from the room on both sides apart:
    /// the tail's two sides are nearly uncorrelated, or the room is one
    /// speaker's worth wide.
    #[test]
    fn the_room_rings_wide() {
        let n = SAMPLE_RATE as usize * 3;
        let mut send = vec![[0.0f32; 2]; n];
        let mut seed = 1u32;
        for s in send.iter_mut().take(SAMPLE_RATE as usize / 10) {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let v = (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5;
            *s = [v, v];
        }
        let out = ring(&send, 2.0);
        let tail = &out[SAMPLE_RATE as usize / 2..];
        let (lr, ll, rr) = tail.iter().fold((0.0f64, 0.0f64, 0.0f64), |(a, b, c), s| (a + (s[0] * s[1]) as f64, b + (s[0] * s[0]) as f64, c + (s[1] * s[1]) as f64));
        let r = lr / (ll * rr).sqrt();
        eprintln!("tail correlation {r:.3}");
        assert!(r.abs() < 0.3, "the tail's sides correlate at {r:.2}");
    }
}
