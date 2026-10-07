//! What can be measured of the rendered audio. Loudness follows BS.1770
//! at 48 kHz: K-weighting, 400 ms blocks every 100 ms, the two gates;
//! the loudness range follows EBU Tech 3342, on 3 s windows.

use crate::SAMPLE_RATE;

/// BS.1770-4 K-weighting at 48 kHz: the head shelf, then the high-pass.
const K_SHELF: ([f64; 3], [f64; 3]) = ([1.53512485958697, -2.69169618940638, 1.19839281085285], [1.0, -1.69065929318241, 0.73248077421585]);
const K_HIGHPASS: ([f64; 3], [f64; 3]) = ([1.0, -2.0, 1.0], [1.0, -1.99004745483398, 0.99007225036621]);

const BLOCK_S: f64 = 0.4;
const SHORT_TERM_S: f64 = 3.0;
const HOP_S: f64 = 0.1;

fn biquad(x: &[f64], (b, a): ([f64; 3], [f64; 3])) -> Vec<f64> {
    let mut y = vec![0.0; x.len()];
    let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
    for i in 0..x.len() {
        let v = b[0] * x[i] + b[1] * x1 + b[2] * x2 - a[1] * y1 - a[2] * y2;
        x2 = x1;
        x1 = x[i];
        y2 = y1;
        y1 = v;
        y[i] = v;
    }
    y
}

/// Loudness, LUFS, over every window of `window_s` seconds, one every
/// hop, and each window's start in seconds; one window over the whole
/// when the audio is shorter than the window.
fn windowed(audio: &[[f32; 2]], window_s: f64) -> (Vec<f32>, Vec<f32>) {
    let rate = SAMPLE_RATE as f64;
    let mut power = vec![0.0f64; audio.len()];
    for ch in 0..2 {
        let x: Vec<f64> = audio.iter().map(|s| s[ch] as f64).collect();
        let k = biquad(&biquad(&x, K_SHELF), K_HIGHPASS);
        for (p, v) in power.iter_mut().zip(k) {
            *p += v * v;
        }
    }
    let mut prefix = vec![0.0f64; audio.len() + 1];
    for (i, p) in power.iter().enumerate() {
        prefix[i + 1] = prefix[i] + p;
    }
    let lufs_of = |a: usize, b: usize| (-0.691 + 10.0 * ((prefix[b] - prefix[a]) / (b - a).max(1) as f64).max(1e-12).log10()) as f32;
    let (n, hop) = ((window_s * rate) as usize, (HOP_S * rate) as usize);
    let mut lufs = Vec::new();
    let mut starts = Vec::new();
    let mut s = 0;
    while s + n <= audio.len() {
        lufs.push(lufs_of(s, s + n));
        starts.push((s as f64 / rate) as f32);
        s += hop;
    }
    if lufs.is_empty() {
        lufs.push(lufs_of(0, audio.len()));
        starts.push(0.0);
    }
    (lufs, starts)
}

/// Momentary loudness, LUFS, per block, and each block's start in seconds.
pub fn block_loudness(audio: &[[f32; 2]]) -> (Vec<f32>, Vec<f32>) {
    windowed(audio, BLOCK_S)
}

/// Short-term loudness, LUFS, over three-second windows, and each
/// window's start in seconds: what the loudness range is measured on.
pub fn short_term_loudness(audio: &[[f32; 2]]) -> (Vec<f32>, Vec<f32>) {
    windowed(audio, SHORT_TERM_S)
}

/// Loudness range, LU: how far the short-term loudness ranges between
/// its 10th and 95th percentiles, gated at -70 LUFS and 20 LU under the
/// mean, so a lull and a swell count and a moment's silence does not.
pub fn loudness_range(audio: &[[f32; 2]]) -> f32 {
    let (short, _) = windowed(audio, SHORT_TERM_S);
    let above: Vec<f32> = short.into_iter().filter(|l| *l > -70.0).collect();
    if above.is_empty() {
        return 0.0;
    }
    let gate = mean_power(&above) - 20.0;
    let mut kept: Vec<f32> = above.into_iter().filter(|l| *l > gate).collect();
    if kept.len() < 2 {
        return 0.0;
    }
    kept.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let at = |q: f32| kept[((kept.len() - 1) as f32 * q).round() as usize];
    at(0.95) - at(0.10)
}

fn mean_power(lufs: &[f32]) -> f32 {
    10.0 * (lufs.iter().map(|l| 10f32.powf(l / 10.0)).sum::<f32>() / lufs.len() as f32).log10()
}

/// The loudest moment, LUFS: the highest momentary loudness over the
/// audio padded with silence to at least one block, so a sound shorter
/// than a block is measured in the window it is heard in and not over
/// its own length — the level a one-shot plays at in a mix.
pub fn loudest_moment(audio: &[[f32; 2]]) -> f32 {
    let block = (BLOCK_S * SAMPLE_RATE as f64) as usize;
    if audio.len() >= block {
        return block_loudness(audio).0.into_iter().fold(f32::NEG_INFINITY, f32::max);
    }
    let mut padded = vec![[0.0f32; 2]; block];
    padded[..audio.len()].copy_from_slice(audio);
    block_loudness(&padded).0.into_iter().fold(f32::NEG_INFINITY, f32::max)
}

/// Gated integrated loudness, LUFS: the absolute gate at -70, then the
/// relative gate 10 LU under the ungated mean.
pub fn integrated(audio: &[[f32; 2]]) -> f32 {
    let (lufs, _) = block_loudness(audio);
    let above: Vec<f32> = lufs.iter().copied().filter(|l| *l > -70.0).collect();
    if above.is_empty() {
        return f32::NEG_INFINITY;
    }
    let gate = mean_power(&above) - 10.0;
    let kept: Vec<f32> = above.into_iter().filter(|l| *l > gate).collect();
    if kept.is_empty() {
        f32::NEG_INFINITY
    } else {
        mean_power(&kept)
    }
}

/// Peak after 4x oversampling with a windowed-sinc interpolator, dBTP.
pub fn true_peak(audio: &[[f32; 2]]) -> f32 {
    20.0 * true_peaks(audio).into_iter().fold(0.0f32, f32::max).max(1e-9).log10()
}

/// Each sample's true peak, linear: the loudest of either channel at it
/// and at the three points 4x oversampling puts between it and the next,
/// by a windowed-sinc interpolator. A sample within the interpolator's
/// reach of either end is its own peak.
pub fn true_peaks(audio: &[[f32; 2]]) -> Vec<f32> {
    const TAPS: usize = 12;
    let mut peaks: Vec<f32> = audio.iter().map(|s| s[0].abs().max(s[1].abs())).collect();
    for phase in 1..4 {
        let frac = phase as f32 / 4.0;
        let taps: Vec<f32> = (0..2 * TAPS)
            .map(|k| {
                let t = k as f32 - TAPS as f32 + 1.0 - frac;
                let sinc = (std::f32::consts::PI * t).sin() / (std::f32::consts::PI * t);
                let w = 0.5 + 0.5 * (std::f32::consts::PI * t / TAPS as f32).cos();
                sinc * w
            })
            .collect();
        for ch in 0..2 {
            for i in TAPS..audio.len().saturating_sub(TAPS) {
                let v: f32 = taps.iter().enumerate().map(|(k, t)| t * audio[i + k - TAPS + 1][ch]).sum();
                peaks[i] = peaks[i].max(v.abs());
            }
        }
    }
    peaks
}

/// Seconds under -60 dBFS at the head and at the tail.
pub fn silence(audio: &[[f32; 2]]) -> (f32, f32) {
    let floor = 10f32.powf(-60.0 / 20.0);
    let loud = |s: &[f32; 2]| s[0].abs().max(s[1].abs()) > floor;
    let first = audio.iter().position(loud);
    let last = audio.iter().rposition(loud);
    match (first, last) {
        (Some(f), Some(l)) => (f as f32 / SAMPLE_RATE as f32, (audio.len() - 1 - l) as f32 / SAMPLE_RATE as f32),
        _ => (audio.len() as f32 / SAMPLE_RATE as f32, audio.len() as f32 / SAMPLE_RATE as f32),
    }
}

/// Momentary loudness over the first and the last second, LUFS.
pub fn ends(audio: &[[f32; 2]]) -> (f32, f32) {
    let n = SAMPLE_RATE as usize;
    let (head, _) = block_loudness(&audio[..n.min(audio.len())]);
    let (tail, _) = block_loudness(&audio[audio.len().saturating_sub(n)..]);
    (mean_power(&head), mean_power(&tail))
}

/// The loudest three seconds of momentary loudness, LUFS, and where that
/// stretch is centred as a fraction of the clip.
pub fn peak(audio: &[[f32; 2]]) -> (f32, f32) {
    let (lufs, starts) = block_loudness(audio);
    let n = (3.0 / HOP_S) as usize;
    let n = n.min(lufs.len()).max(1);
    let mut best = (f32::NEG_INFINITY, 0.0);
    for i in 0..=lufs.len() - n {
        let m = mean_power(&lufs[i..i + n]);
        if m > best.0 {
            best = (m, (starts[i] + 1.5) / (audio.len() as f32 / SAMPLE_RATE as f32));
        }
    }
    best
}

/// The mean momentary loudness, LUFS, of each span `(from, to)` in
/// seconds: of the blocks that start within it, or -inf where none does.
pub fn spans(audio: &[[f32; 2]], spans: &[(f32, f32)]) -> Vec<f32> {
    let (lufs, starts) = block_loudness(audio);
    within(&lufs, &starts, spans)
}

fn within(lufs: &[f32], starts: &[f32], spans: &[(f32, f32)]) -> Vec<f32> {
    spans
        .iter()
        .map(|(a, b)| {
            // Gated as BS.1770 gates: a ring's last blocks under -70 LUFS
            // are silence, there in a render and cut from the file.
            let blocks: Vec<f32> = lufs.iter().zip(starts).filter(|(l, s)| **s >= *a && **s < *b && **l > -70.0).map(|(l, _)| *l).collect();
            if blocks.is_empty() {
                f32::NEG_INFINITY
            } else {
                mean_power(&blocks)
            }
        })
        .collect()
}

pub struct Report {
    pub duration_s: f32,
    pub lufs: f32,
    pub peak_dbtp: f32,
    pub silence_head_s: f32,
    pub silence_tail_s: f32,
    pub lufs_head: f32,
    pub lufs_tail: f32,
    pub lufs_peak: f32,
    pub peak_at: f32,
    /// Loudness range, LU.
    pub lra: f32,
    /// Mean momentary loudness of each section, in order.
    pub sections: Vec<(String, f32)>,
}

pub fn report(audio: &[[f32; 2]], sections: &[(String, f32, f32)]) -> Report {
    let (lufs, starts) = block_loudness(audio);
    let (silence_head_s, silence_tail_s) = silence(audio);
    let (lufs_head, lufs_tail) = ends(audio);
    let (lufs_peak, peak_at) = peak(audio);
    let spans: Vec<(f32, f32)> = sections.iter().map(|(_, a, b)| (*a, *b)).collect();
    let sections = sections.iter().map(|(name, _, _)| name.clone()).zip(within(&lufs, &starts, &spans)).collect();
    Report {
        duration_s: audio.len() as f32 / SAMPLE_RATE as f32,
        lufs: integrated(audio),
        peak_dbtp: true_peak(audio),
        silence_head_s,
        silence_tail_s,
        lufs_head,
        lufs_tail,
        lufs_peak,
        peak_at,
        lra: loudness_range(audio),
        sections,
    }
}

impl Report {
    pub fn json(&self) -> String {
        let sections: Vec<String> = self.sections.iter().map(|(n, l)| format!("    {{ \"name\": \"{n}\", \"lufs\": {l:.2} }}")).collect();
        format!(
            "{{\n  \"duration_s\": {:.3},\n  \"lufs\": {:.2},\n  \"peak_dbtp\": {:.2},\n  \"silence_head_s\": {:.3},\n  \"silence_tail_s\": {:.3},\n  \"lufs_head\": {:.2},\n  \"lufs_tail\": {:.2},\n  \"lufs_peak\": {:.2},\n  \"peak_at\": {:.2},\n  \"lra\": {:.2},\n  \"sections\": [\n{}\n  ]\n}}\n",
            self.duration_s, self.lufs, self.peak_dbtp, self.silence_head_s, self.silence_tail_s, self.lufs_head, self.lufs_tail, self.lufs_peak, self.peak_at, self.lra,
            sections.join(",\n")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// BS.1770's own reference: a full-scale 997 Hz sine in one channel
    /// reads -3.01 LUFS.
    #[test]
    fn reference_sine_reads_minus_three() {
        let n = SAMPLE_RATE as usize * 4;
        let audio: Vec<[f32; 2]> = (0..n).map(|i| [(2.0 * std::f32::consts::PI * 997.0 * i as f32 / SAMPLE_RATE as f32).sin(), 0.0]).collect();
        assert!((integrated(&audio) + 3.01).abs() < 0.1);
        assert!((true_peak(&audio) - 0.0).abs() < 0.2);
    }

    /// A tone held at one level has no range; the same tone at two
    /// levels, half the time each, ranges by their difference, since the
    /// 10th percentile sits in the soft half and the 95th in the loud.
    #[test]
    fn loudness_range_is_the_swing_between_levels() {
        let n = SAMPLE_RATE as usize * 20;
        let tone = |i: usize| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / SAMPLE_RATE as f32).sin();
        let flat: Vec<[f32; 2]> = (0..n).map(|i| [0.3 * tone(i), 0.3 * tone(i)]).collect();
        assert!(loudness_range(&flat) < 0.1);
        let gain = |i: usize| if i < n / 2 { 0.1 } else { 0.3 };
        let stepped: Vec<[f32; 2]> = (0..n).map(|i| [gain(i) * tone(i), gain(i) * tone(i)]).collect();
        let lra = loudness_range(&stepped);
        assert!((lra - 20.0 * 3f32.log10()).abs() < 1.0, "{lra}");
    }
}
