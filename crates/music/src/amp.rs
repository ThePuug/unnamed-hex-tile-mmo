//! A rig played over a whole stem (`rigs`): the instrument's sound at the
//! jack in, the cabinet's microphone and the delay's two sides out. The
//! amp's clipping runs at `OVER` times the sample rate, between a
//! windowed-sinc interpolator and decimator, so the harmonics it makes
//! over the band fold back as none; the cabinet is its impulse response
//! convolved by blocks through a radix-2 FFT. Pure arithmetic over a
//! buffer, as the render is.

use std::f32::consts::PI;

use crate::rigs::{Amp, Cabinet, Compressor, Delay, Rig, BASS_HZ, PRESENCE_HZ, TREBLE_HZ};
use audio::SAMPLE_RATE;

/// How many times the sample rate the amp clips at.
const OVER: usize = 4;
/// Taps of the interpolator and decimator, a multiple of `OVER`.
const TAPS: usize = 128;
/// Where they cut, Hz: the band kept, under the base rate's Nyquist.
const CUT_HZ: f32 = 20_000.0;
/// The low-pass every triode stage carries, Hz: its plate's capacitance
/// taming the fizz the clipping makes.
const STAGE_TOP_HZ: f32 = 9_000.0;

/// `rig` played over `jack`, mono at `SAMPLE_RATE`, with the score's
/// eighth note `eighth_s` long: the stem as `(mid, side)`, the delay's
/// repeats either side of the middle.
pub fn play(rig: &Rig, jack: &[f32], eighth_s: f64) -> Vec<(f32, f32)> {
    let mut x = jack.to_vec();
    if let Some(c) = &rig.compressor {
        x = hold(c, &x);
    }
    if let Some(amp) = &rig.amp {
        x = head(amp, &x);
    }
    if let Some(cab) = rig.cabinet {
        x = convolve(&x, &impulse(cab));
    }
    match &rig.delay {
        Some(d) => delay(d, &x, eighth_s),
        None => x.into_iter().map(|m| (m, 0.0)).collect(),
    }
}

/// `x` held by `c`: a peak follower with the compressor's attack and
/// release, and every rise of its level over the threshold given back
/// divided by the ratio.
fn hold(c: &Compressor, x: &[f32]) -> Vec<f32> {
    let threshold_db = c.threshold_db;
    let attack = (-1.0 / (c.attack_s * SAMPLE_RATE as f32)).exp();
    let release = (-1.0 / (c.release_s * SAMPLE_RATE as f32)).exp();
    let slope = 1.0 - 1.0 / c.ratio;
    let mut env = 0.0f32;
    x.iter()
        .map(|&s| {
            let v = s.abs();
            let coeff = if v > env { attack } else { release };
            env = coeff * env + (1.0 - coeff) * v;
            let db = 20.0 * env.max(1e-9).log10();
            if db > threshold_db { s * 10f32.powf((threshold_db - db) * slope / 20.0) } else { s }
        })
        .collect()
}

/// The cabinet's impulse response, 48 kHz.
fn impulse(cab: Cabinet) -> Vec<f32> {
    let bytes: &[u8] = match cab {
        Cabinet::V30 => include_bytes!("cabinets/v30-sm57.f32"),
        Cabinet::Celestion => include_bytes!("cabinets/celestion-sm57.f32"),
    };
    bytes.as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)).collect()
}

/// The head over `x`: up to the clipping rate, the pedal, the stages,
/// the tone stack and the power stage, and down again.
fn head(amp: &Amp, x: &[f32]) -> Vec<f32> {
    let rate = (SAMPLE_RATE as usize * OVER) as f32;
    let mut y = up(x);
    if let Some(b) = &amp.boost {
        let mut high = OnePole::high(b.hz, rate);
        let mut tone = OnePole::low(b.tone_hz, rate);
        let g = db(b.gain_db);
        for s in y.iter_mut() {
            *s = tone.run(*s + clip(g * high.run(*s)));
        }
    }
    let drive = db(amp.drive_db);
    for s in y.iter_mut() {
        *s *= drive;
    }
    for stage in amp.stages {
        let mut coupling = OnePole::high(stage.low_cut_hz, rate);
        let mut plate = OnePole::low(STAGE_TOP_HZ, rate);
        let (g, b) = (db(stage.gain_db), stage.bias);
        let rest = clip(g * b);
        for s in y.iter_mut() {
            *s = plate.run(clip(g * (coupling.run(*s) + b)) - rest);
        }
    }
    let mut stack = [
        Biquad::shelf(BASS_HZ, amp.bass_db, rate, false),
        Biquad::bell(amp.mid_hz, amp.mid_db, 0.7, rate),
        Biquad::shelf(TREBLE_HZ, amp.treble_db, rate, true),
    ];
    let mut presence = Biquad::shelf(PRESENCE_HZ, amp.presence_db, rate, true);
    for s in y.iter_mut() {
        let toned = stack.iter_mut().fold(*s, |v, f| f.run(v));
        *s = presence.run(clip(1.5 * toned));
    }
    down(&y, x.len())
}

fn db(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// A hyperbolic tangent near enough for a clipping stage, its rational
/// Padé form, which meets ±1 with a level slope at ±3 and holds there:
/// the clipping runs on millions of samples a stem.
fn clip(x: f32) -> f32 {
    let x = x.clamp(-3.0, 3.0);
    x * (27.0 + x * x) / (27.0 + 9.0 * x * x)
}

/// The interpolating and decimating filter: a Blackman-windowed sinc at
/// the clipping rate, unity at DC.
fn kernel() -> Vec<f32> {
    let fc = CUT_HZ / (SAMPLE_RATE as f32 * OVER as f32);
    let mid = (TAPS - 1) as f32 / 2.0;
    let h: Vec<f32> = (0..TAPS)
        .map(|n| {
            let t = n as f32 - mid;
            let sinc = if t == 0.0 { 2.0 * fc } else { (2.0 * PI * fc * t).sin() / (PI * t) };
            let w = 0.42 - 0.5 * (2.0 * PI * n as f32 / (TAPS - 1) as f32).cos() + 0.08 * (4.0 * PI * n as f32 / (TAPS - 1) as f32).cos();
            sinc * w
        })
        .collect();
    let sum: f32 = h.iter().sum();
    h.into_iter().map(|v| v / sum).collect()
}

/// `x` at `OVER` times its rate, delayed by the kernel's half.
fn up(x: &[f32]) -> Vec<f32> {
    let h = kernel();
    let k = TAPS / OVER;
    // Each phase's taps, newest sample first, and the input behind `k - 1`
    // zeros, so every window is whole.
    let phases: Vec<Vec<f32>> = (0..OVER).map(|p| (0..k).rev().map(|j| h[j * OVER + p] * OVER as f32).collect()).collect();
    let padded: Vec<f32> = std::iter::repeat_n(0.0, k - 1).chain(x.iter().copied()).chain(std::iter::repeat_n(0.0, k)).collect();
    let mut y = Vec::with_capacity((x.len() + k) * OVER);
    for window in padded.windows(k) {
        for taps in &phases {
            y.push(window.iter().zip(taps).map(|(v, t)| v * t).sum());
        }
    }
    y
}

/// `y` back at the base rate, `len` samples, its delay through `up` and
/// here taken off.
fn down(y: &[f32], len: usize) -> Vec<f32> {
    let h: Vec<f32> = kernel().into_iter().rev().collect();
    let mut padded = y.to_vec();
    padded.resize(len * OVER + TAPS, 0.0);
    (0..len).map(|n| padded[n * OVER..n * OVER + TAPS].iter().zip(&h).map(|(v, t)| v * t).sum()).collect()
}

/// A first-order filter, low- or high-pass, by the matched pole.
struct OnePole {
    a: f32,
    low: f32,
    high: bool,
}

impl OnePole {
    fn low(hz: f32, rate: f32) -> Self {
        OnePole { a: (-2.0 * PI * hz / rate).exp(), low: 0.0, high: false }
    }

    fn high(hz: f32, rate: f32) -> Self {
        OnePole { high: true, ..OnePole::low(hz, rate) }
    }

    fn run(&mut self, x: f32) -> f32 {
        self.low = x + self.a * (self.low - x);
        if self.high {
            x - self.low
        } else {
            self.low
        }
    }
}

/// A second-order section, the cookbook's shelves and bell.
struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    z: [f32; 2],
}

impl Biquad {
    fn new(b: [f32; 3], a: [f32; 3]) -> Self {
        Biquad { b: [b[0] / a[0], b[1] / a[0], b[2] / a[0]], a: [a[1] / a[0], a[2] / a[0]], z: [0.0; 2] }
    }

    fn shelf(hz: f32, gain_db: f32, rate: f32, high: bool) -> Self {
        let a = 10f32.powf(gain_db / 40.0);
        let w = 2.0 * PI * hz / rate;
        let (cos, alpha) = (w.cos(), w.sin() / 2.0 * 2f32.sqrt());
        let s = 2.0 * a.sqrt() * alpha;
        let k = if high { -1.0 } else { 1.0 };
        Biquad::new(
            [a * ((a + 1.0) - k * (a - 1.0) * cos + s), 2.0 * k * a * ((a - 1.0) - k * (a + 1.0) * cos), a * ((a + 1.0) - k * (a - 1.0) * cos - s)],
            [(a + 1.0) + k * (a - 1.0) * cos + s, -2.0 * k * ((a - 1.0) + k * (a + 1.0) * cos), (a + 1.0) + k * (a - 1.0) * cos - s],
        )
    }

    fn bell(hz: f32, gain_db: f32, q: f32, rate: f32) -> Self {
        let a = 10f32.powf(gain_db / 40.0);
        let w = 2.0 * PI * hz / rate;
        let alpha = w.sin() / (2.0 * q);
        Biquad::new([1.0 + alpha * a, -2.0 * w.cos(), 1.0 - alpha * a], [1.0 + alpha / a, -2.0 * w.cos(), 1.0 - alpha / a])
    }

    fn run(&mut self, x: f32) -> f32 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

/// `x` convolved with `ir`, as long as `x`: overlap-add by blocks of the
/// FFT's size less the response's.
fn convolve(x: &[f32], ir: &[f32]) -> Vec<f32> {
    let n = (2 * ir.len()).next_power_of_two();
    let block = n - ir.len() + 1;
    let fft = Fft::new(n);
    let mut h: Vec<(f32, f32)> = (0..n).map(|i| (ir.get(i).copied().unwrap_or(0.0), 0.0)).collect();
    fft.run(&mut h, false);
    let mut out = vec![0.0f32; x.len() + n];
    let mut buf = vec![(0.0f32, 0.0f32); n];
    for (b, chunk) in x.chunks(block).enumerate() {
        for (i, v) in buf.iter_mut().enumerate() {
            *v = (chunk.get(i).copied().unwrap_or(0.0), 0.0);
        }
        fft.run(&mut buf, false);
        for (v, k) in buf.iter_mut().zip(&h) {
            *v = (v.0 * k.0 - v.1 * k.1, v.0 * k.1 + v.1 * k.0);
        }
        fft.run(&mut buf, true);
        for (i, v) in buf.iter().enumerate() {
            out[b * block + i] += v.0 / n as f32;
        }
    }
    out.truncate(x.len());
    out
}

/// A radix-2 complex FFT of one size, its twiddles computed once.
struct Fft {
    n: usize,
    twiddles: Vec<(f32, f32)>,
}

impl Fft {
    fn new(n: usize) -> Self {
        assert!(n.is_power_of_two());
        let twiddles = (0..n / 2).map(|k| {
            let a = -2.0 * std::f64::consts::PI * k as f64 / n as f64;
            (a.cos() as f32, a.sin() as f32)
        });
        Fft { n, twiddles: twiddles.collect() }
    }

    /// In place; `inverse` unscaled.
    fn run(&self, x: &mut [(f32, f32)], inverse: bool) {
        let n = self.n;
        let bits = n.trailing_zeros();
        for i in 0..n {
            let j = i.reverse_bits() >> (usize::BITS - bits);
            if j > i {
                x.swap(i, j);
            }
        }
        let mut len = 2;
        while len <= n {
            let step = n / len;
            for start in (0..n).step_by(len) {
                for k in 0..len / 2 {
                    let (c, s) = self.twiddles[k * step];
                    let s = if inverse { -s } else { s };
                    let (a, b) = (x[start + k], x[start + k + len / 2]);
                    let t = (b.0 * c - b.1 * s, b.0 * s + b.1 * c);
                    x[start + k] = (a.0 + t.0, a.1 + t.1);
                    x[start + k + len / 2] = (a.0 - t.0, a.1 - t.1);
                }
            }
            len *= 2;
        }
    }
}

/// `x` with `d`'s repeats: each side's line fed back into itself through
/// a low-pass, under the dry by `mix`, as `(mid, side)`.
fn delay(d: &Delay, x: &[f32], eighth_s: f64) -> Vec<(f32, f32)> {
    let rate = SAMPLE_RATE as f64;
    let lengths = [(d.left_eighths as f64 * eighth_s * rate).round().max(1.0) as usize, (d.right_eighths as f64 * eighth_s * rate).round().max(1.0) as usize];
    let mut lines: Vec<Vec<f32>> = lengths.iter().map(|l| vec![0.0; *l]).collect();
    let mut tones = [OnePole::low(d.tone_hz, rate as f32), OnePole::low(d.tone_hz, rate as f32)];
    x.iter()
        .enumerate()
        .map(|(i, dry)| {
            let mut wet = [0.0f32; 2];
            for side in 0..2 {
                let at = i % lengths[side];
                let echo = lines[side][at];
                lines[side][at] = tones[side].run(dry + d.feedback * echo);
                wet[side] = echo;
            }
            let (l, r) = (dry + d.mix * wet[0], dry + d.mix * wet[1]);
            ((l + r) / 2.0, (l - r) / 2.0)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(hz: f32, seconds: f32, amp: f32) -> Vec<f32> {
        (0..(seconds * SAMPLE_RATE as f32) as usize).map(|i| amp * (2.0 * PI * hz * i as f32 / SAMPLE_RATE as f32).sin()).collect()
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    #[test]
    fn up_and_down_again_is_the_signal() {
        let x = tone(440.0, 0.2, 0.5);
        let y = down(&up(&x), x.len());
        let err: Vec<f32> = x.iter().zip(&y).skip(TAPS).map(|(a, b)| a - b).collect();
        assert!(rms(&err) < 0.01 * rms(&x), "{}", rms(&err));
    }

    #[test]
    fn the_fft_convolves_as_the_sum_does() {
        let x: Vec<f32> = (0..3000).map(|i| ((i * 7919) % 101) as f32 / 50.0 - 1.0).collect();
        let ir: Vec<f32> = (0..300).map(|i| (-(i as f32) / 40.0).exp() * if i % 3 == 0 { 1.0 } else { -0.5 }).collect();
        let fast = convolve(&x, &ir);
        for n in [0, 1, 299, 300, 1777, 2999] {
            let slow: f32 = (0..=n.min(ir.len() - 1)).map(|k| ir[k] * x[n - k]).sum();
            assert!((fast[n] - slow).abs() < 1e-3, "{n}: {} {slow}", fast[n]);
        }
    }

    #[test]
    fn the_amp_compresses_a_quiet_note_and_a_loud_one_toward_each_other() {
        let rig = crate::rigs::LEAD;
        let amp = rig.amp.unwrap();
        let soft = head(&amp, &tone(330.0, 0.3, 0.05));
        let loud = head(&amp, &tone(330.0, 0.3, 0.5));
        let gap = 20.0 * (rms(&loud[2000..]) / rms(&soft[2000..])).log10();
        assert!(gap < 10.0, "{gap} dB apart for 20 dB in");
    }

    #[test]
    fn a_delay_repeats_on_its_side_a_beat_late() {
        let d = Delay { left_eighths: 2.0, right_eighths: 3.0, feedback: 0.0, mix: 1.0, tone_hz: 20_000.0 };
        let mut x = vec![0.0f32; 48_000];
        x[0] = 1.0;
        let y = delay(&d, &x, 0.25);
        let left = |i: usize| y[i].0 + y[i].1;
        let right = |i: usize| y[i].0 - y[i].1;
        assert!(left(24_000) > 0.5 && right(24_000).abs() < 1e-6);
        assert!(right(36_000) > 0.5 && left(36_000).abs() < 1e-6);
    }

    /// Peak over RMS, the mean over one-second windows.
    fn crest(x: &[f32]) -> f32 {
        let wins: Vec<f32> = x.chunks(SAMPLE_RATE as usize).map(|w| w.iter().fold(0.0f32, |m, s| m.max(s.abs())) / rms(w)).collect();
        wins.iter().sum::<f32>() / wins.len() as f32
    }

    #[test]
    fn a_held_pluck_keeps_a_floor() {
        // Three plucked low A's a second apart, each a sharp strike
        // falling twelve decibels a second.
        let rate = SAMPLE_RATE as f32;
        let x: Vec<f32> = (0..3 * SAMPLE_RATE as usize)
            .map(|i| {
                let t = (i % SAMPLE_RATE as usize) as f32 / rate;
                let strike = if t < 0.005 { 2.0 } else { 1.0 };
                0.3 * strike * 10f32.powf(-12.0 * t / 20.0) * (2.0 * PI * 55.0 * t).sin()
            })
            .collect();
        let held = hold(&crate::rigs::BASS.compressor.unwrap(), &x);
        assert!(crest(&held) < crest(&x), "held {} dry {}", crest(&held), crest(&x));
        // The tail stands nearer the strike than it did.
        let late = |y: &[f32]| rms(&y[40_000..47_000]) / rms(&y[500..7_500]);
        assert!(late(&held) > late(&x), "held tail {} dry {}", late(&held), late(&x));
    }
}
