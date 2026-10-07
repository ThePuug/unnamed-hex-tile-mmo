//! The mix bus: what the summed band goes through before it is a file,
//! as a mastering engineer's chain does. A low shelf gives the bass and
//! the kick the weight the bank's samples leave thin; a high shelf gives
//! back the air its samples lack, every piece's top octave sitting some
//! twenty decibels under its middle; and a glue compressor pulls the
//! loudest passages down a few decibels, so the band sits together and
//! not as players in separate rooms. Its threshold is the render's own:
//! set from where the render's loudest passages sit, so a quiet piece is
//! compressed as much as a loud one. Pure arithmetic over a buffer: the
//! same samples give the same samples.

use crate::render::SAMPLE_RATE;

/// The low shelf: its corner and its lift, dB.
const LOW_SHELF_HZ: f32 = 110.0;
const LOW_SHELF_DB: f32 = 2.5;
/// The high shelf: its corner and its lift, dB.
const HIGH_SHELF_HZ: f32 = 7000.0;
const HIGH_SHELF_DB: f32 = 3.0;

/// The glue: its ratio, its knee's width, dB, how fast it takes hold and
/// lets go, and how far under the render's loud passages it begins: the
/// 95th percentile of its level, so those passages give some three dB.
const RATIO: f32 = 2.0;
const KNEE_DB: f32 = 6.0;
const ATTACK_MS: f32 = 25.0;
const RELEASE_MS: f32 = 250.0;
const DETECT_MS: f32 = 10.0;
const UNDER_LOUD_DB: f32 = 6.0;

/// The limiter's ceiling on the true peak, dBTP, under the -1 a file may
/// peak at by what the gain moving between samples bends the waveform
/// between them; how far ahead it looks, and how fast it takes hold and
/// lets go.
const CEILING_DB: f32 = -1.2;
const LOOKAHEAD_MS: f32 = 5.0;
const LIMIT_ATTACK_MS: f32 = 1.0;
const LIMIT_RELEASE_MS: f32 = 80.0;

/// The last stage, after the loudness is set: every sample's true peak
/// held under the ceiling, the gain eased down over the few milliseconds before a peak
/// that would pass it and back up after, so a rare stroke over it is
/// turned down rather than clipped.
pub fn limit(audio: &mut [[f32; 2]]) {
    let fs = SAMPLE_RATE as f32;
    let ceiling = 10f32.powf(CEILING_DB / 20.0);
    let need: Vec<f32> = ::audio::measure::true_peaks(audio).into_iter().map(|p| (ceiling / p.max(1e-9)).min(1.0)).collect();
    let look = (LOOKAHEAD_MS * 0.001 * fs) as usize;
    // The least gain any sample from here to `look` ahead needs: a
    // sliding minimum, run back from the end.
    let mut ahead = vec![1.0f32; need.len()];
    let mut window: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
    for i in (0..need.len()).rev() {
        while window.back().is_some_and(|k| need[*k] >= need[i]) {
            window.pop_back();
        }
        window.push_back(i);
        while window.front().is_some_and(|k| *k > i + look) {
            window.pop_front();
        }
        ahead[i] = need[window[0]];
    }
    let pole = |ms: f32| (-1.0 / (ms * 0.001 * fs)).exp();
    let (attack, release) = (pole(LIMIT_ATTACK_MS), pole(LIMIT_RELEASE_MS));
    let mut gain = 1.0f32;
    for (s, (target, floor)) in audio.iter_mut().zip(ahead.into_iter().zip(need)) {
        let pole = if target < gain { attack } else { release };
        gain = (pole * gain + (1.0 - pole) * target).min(floor);
        s[0] *= gain;
        s[1] *= gain;
    }
}

/// `audio` through the bus, in place.
pub fn master(audio: &mut [[f32; 2]]) {
    shelve(audio, Shelf::Low, LOW_SHELF_HZ, LOW_SHELF_DB);
    shelve(audio, Shelf::High, HIGH_SHELF_HZ, HIGH_SHELF_DB);
    glue(audio);
}

#[derive(Clone, Copy)]
enum Shelf {
    Low,
    High,
}

/// An RBJ shelving biquad, `db` at and past `hz`, run over both sides.
fn shelve(audio: &mut [[f32; 2]], shelf: Shelf, hz: f32, db: f32) {
    let a = 10f32.powf(db / 40.0);
    let w = std::f32::consts::TAU * hz / SAMPLE_RATE as f32;
    let (cos, sin) = (w.cos(), w.sin());
    let alpha = sin / 2.0 * std::f32::consts::SQRT_2;
    let k = 2.0 * a.sqrt() * alpha;
    let (b, c) = match shelf {
        Shelf::Low => (
            [a * ((a + 1.0) - (a - 1.0) * cos + k), 2.0 * a * ((a - 1.0) - (a + 1.0) * cos), a * ((a + 1.0) - (a - 1.0) * cos - k)],
            [(a + 1.0) + (a - 1.0) * cos + k, -2.0 * ((a - 1.0) + (a + 1.0) * cos), (a + 1.0) + (a - 1.0) * cos - k],
        ),
        Shelf::High => (
            [a * ((a + 1.0) + (a - 1.0) * cos + k), -2.0 * a * ((a - 1.0) + (a + 1.0) * cos), a * ((a + 1.0) + (a - 1.0) * cos - k)],
            [(a + 1.0) - (a - 1.0) * cos + k, 2.0 * ((a - 1.0) - (a + 1.0) * cos), (a + 1.0) - (a - 1.0) * cos - k],
        ),
    };
    let (b0, b1, b2, a1, a2) = (b[0] / c[0], b[1] / c[0], b[2] / c[0], c[1] / c[0], c[2] / c[0]);
    for ch in 0..2 {
        let (mut x1, mut x2, mut y1, mut y2) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        for s in audio.iter_mut() {
            let x = s[ch];
            let y = b0 * x + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
            (x2, x1, y2, y1) = (x1, x, y1, y);
            s[ch] = y;
        }
    }
}

/// The glue compressor: the level of both sides together, its gain
/// reduction over a soft knee, eased in and out, applied to both alike.
fn glue(audio: &mut [[f32; 2]]) {
    let fs = SAMPLE_RATE as f32;
    let pole = |ms: f32| (-1.0 / (ms * 0.001 * fs)).exp();
    let (detect, attack, release) = (pole(DETECT_MS), pole(ATTACK_MS), pole(RELEASE_MS));
    let mut power = 0.0f32;
    let level: Vec<f32> = audio
        .iter()
        .map(|s| {
            power = detect * power + (1.0 - detect) * (s[0] * s[0] + s[1] * s[1]) * 0.5;
            10.0 * (power + 1e-12).log10()
        })
        .collect();
    let mut loud: Vec<f32> = level.iter().step_by(64).copied().filter(|l| *l > -90.0).collect();
    if loud.is_empty() {
        return;
    }
    loud.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let threshold = loud[loud.len() * 95 / 100] - UNDER_LOUD_DB;
    let slope = 1.0 - 1.0 / RATIO;
    let mut reduction = 0.0f32;
    for (s, l) in audio.iter_mut().zip(level) {
        let over = l - threshold;
        let want = if over <= -KNEE_DB / 2.0 {
            0.0
        } else if over >= KNEE_DB / 2.0 {
            over * slope
        } else {
            slope * (over + KNEE_DB / 2.0).powi(2) / (2.0 * KNEE_DB)
        };
        let pole = if want > reduction { attack } else { release };
        reduction = pole * reduction + (1.0 - pole) * want;
        let gain = 10f32.powf(-reduction / 20.0);
        s[0] *= gain;
        s[1] *= gain;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(hz: f32, seconds: f32, amp: f32) -> Vec<[f32; 2]> {
        (0..(seconds * SAMPLE_RATE as f32) as usize).map(|i| {
            let v = amp * (std::f32::consts::TAU * hz * i as f32 / SAMPLE_RATE as f32).sin();
            [v, v]
        }).collect()
    }

    fn rms(x: &[[f32; 2]]) -> f32 {
        (x.iter().map(|s| s[0] * s[0]).sum::<f32>() / x.len() as f32).sqrt()
    }

    /// The shelves lift what is past their corners and leave the middle.
    #[test]
    fn the_shelves_lift_their_ends() {
        for (hz, lifted) in [(40.0, true), (1000.0, false), (14000.0, true)] {
            let mut x = tone(hz, 1.0, 0.1);
            let before = rms(&x[24000..]);
            shelve(&mut x, Shelf::Low, LOW_SHELF_HZ, LOW_SHELF_DB);
            shelve(&mut x, Shelf::High, HIGH_SHELF_HZ, HIGH_SHELF_DB);
            let db = 20.0 * (rms(&x[24000..]) / before).log10();
            assert_eq!(db > 1.5, lifted, "{hz} Hz moved {db:.2} dB");
        }
    }

    /// The limiter holds every sample under its ceiling and leaves what is
    /// under it.
    #[test]
    fn the_limiter_holds_the_ceiling() {
        let mut x = tone(220.0, 1.0, 0.3);
        x.extend(tone(220.0, 0.2, 1.2));
        x.extend(tone(220.0, 1.0, 0.3));
        let quiet = rms(&x[..24000]);
        limit(&mut x);
        let ceiling = 10f32.powf(CEILING_DB / 20.0);
        assert!(x.iter().all(|s| s[0].abs() <= ceiling + 1e-6));
        assert!((rms(&x[..24000]) / quiet - 1.0).abs() < 0.001);
    }

    /// The glue pulls a loud passage down and leaves a quiet one.
    #[test]
    fn the_glue_pulls_down_the_loud_alone() {
        let mut x = tone(220.0, 4.0, 0.02);
        x.extend(tone(220.0, 1.0, 0.5));
        let quiet = rms(&x[48000..96000]);
        let loud = rms(&x[200000..240000]);
        glue(&mut x);
        assert!((rms(&x[48000..96000]) / quiet - 1.0).abs() < 0.02);
        assert!(rms(&x[200000..240000]) < loud * 0.8);
    }
}
