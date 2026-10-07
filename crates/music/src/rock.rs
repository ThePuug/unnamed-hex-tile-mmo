//! How a rock band ends a song, the gestures the metal pieces share,
//! counted from seventy songs' transcriptions — Helloween, Maiden,
//! Priest, Accept, Gamma Ray, Blind Guardian
//! (`proofs/research/variety-findings.md`). The ritual holds the last
//! chord under a cymbal's wash and the toms rolling down, the kick
//! running on under it, and closes on a separate last hit, now and then
//! struck on the and of four; a figure of hits strikes the band together
//! and stops; a stab is one stroke, short or let ring, its guitars
//! sliding off it now and then. A piece draws which, and how, and adds
//! what its own players do over them.
//!
//! A metal band is one band whatever it plays: its lineup — its lead
//! guitar, which of its two soloists takes the first turn, how its
//! drummer fills and how often, how its bassist runs — is drawn from the
//! band's seed alone, so its ballads and its speed metal are one band's
//! (`Lineup`). What each form does with those players is the form's.

use crate::rng::Rng;
use crate::score::{Note, Score, TICKS_PER_EIGHTH as E};

const KICK: u8 = 36;
const SNARE: u8 = 38;
const CRASH: u8 = 49;
const CHINA: u8 = 52;
const CRASH_2: u8 = 57;
const TOMS: [u8; 6] = [50, 48, 47, 45, 43, 41];

/// A sixteenth, in ticks.
const S: u32 = E / 2;

/// A drummer's fill: the toms down; the snare and then the toms; each tom
/// twice down the kit; two toms and the kick by turns, the triplet a
/// twelve-eight rolls in; or the snare's roll alone.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fill {
    Descent,
    SnareThenToms,
    Pairs,
    HandHandKick,
    Roll,
}

/// A metal band's habits, the same through every song of theirs, a
/// ballad or speed metal: whether its shredder takes the first solo turn
/// or its singer; the drummer's fill and how many bars apart they fill
/// where the form leaves it to them; whether the bassist runs sixteenths
/// under a riff that does; the share of its ballads its violinist leads;
/// how much of the singer's gaps its guitarist licks in, and how often a
/// phrase is pushed ahead of the beat.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Habits {
    pub shredder_leads: bool,
    pub fill: Fill,
    pub fill_every: u32,
    pub bass_sixteenths: bool,
    pub violin: f32,
    pub fills: f32,
    pub pushes: f32,
}

/// `shape` in `n` sixteenths from `start` on the `kit` channel, growing
/// from `accent` by `grow` into the crash the next part opens on, the
/// kick under it every `kick` sixteenths where the style keeps the kick
/// going through a fill.
#[allow(clippy::too_many_arguments)]
pub fn fill(score: &mut Score, kit: u8, shape: Fill, start: u32, n: u32, (accent, grow): (i32, i32), kick: Option<u32>, vel: fn(i32, &mut Rng) -> u8, rng: &mut Rng) {
    let down = |k: u32, of: u32| TOMS[(k as usize * TOMS.len() / of.max(1) as usize).min(TOMS.len() - 1)];
    for k in 0..n {
        let pitch = match shape {
            Fill::Descent => down(k, n),
            Fill::SnareThenToms if k < n / 2 => SNARE,
            Fill::SnareThenToms => down(k - n / 2, n - n / 2),
            Fill::Pairs => down(k / 2, n.div_ceil(2)),
            Fill::HandHandKick if k % 3 == 2 => KICK,
            Fill::HandHandKick => down(k / 3, n.div_ceil(3)),
            Fill::Roll => SNARE,
        };
        score.add(Note { start: start + k * S, len: S - 10, pitch, vel: vel(accent + grow * k as i32 / n.max(1) as i32, rng), channel: kit });
        if kick.is_some_and(|every| k % every == 0) && pitch != KICK {
            score.add(Note { start: start + k * S, len: S - 10, pitch: KICK, vel: vel(accent - 6, rng), channel: kit });
        }
    }
}

/// The players an ending strikes with: the guitars' channels, the bass's
/// and the kit's; the guitars' chord and the bass's tone; and the
/// piece's velocity.
pub struct Band<'a> {
    pub guitars: &'a [u8],
    pub bass: u8,
    pub kit: u8,
    pub chord: [u8; 3],
    pub root: u8,
    pub vel: fn(i32, &mut Rng) -> u8,
}

impl Band<'_> {
    /// Every player of the band on one stroke at `at`, held `len`, the
    /// crash and the kick under it.
    pub fn stroke(&self, score: &mut Score, at: u32, len: u32, accent: i32, rng: &mut Rng) {
        for channel in self.guitars {
            for p in self.chord {
                score.add(Note { start: at, len, pitch: p, vel: (self.vel)(accent, rng), channel: *channel });
            }
        }
        score.add(Note { start: at, len, pitch: self.root, vel: (self.vel)(accent - 2, rng), channel: self.bass });
        for (pitch, a) in [(CRASH, 28), (CRASH_2, 24), (KICK, -2)] {
            score.add(Note { start: at, len: (2 * S).max(len.min(4 * E)), pitch, vel: (self.vel)(accent + a, rng), channel: self.kit });
        }
    }
}

/// A figure of the band's hits through one bar.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Figure {
    /// Three, three and two sixteenths apart, twice: the dotted figure.
    Dotted,
    /// Dotted quarters: on one, the and of two, and four.
    DottedQuarters,
    /// Three quarters, then a rest.
    Quarters,
    /// One, then a pair of eighths into the bar line.
    Pair,
}

impl Figure {
    /// Each hit's sixteenth in the bar and its length, sixteenths.
    fn hits(self) -> &'static [(u32, u32)] {
        match self {
            Figure::Dotted => &[(0, 3), (3, 3), (6, 2), (8, 3), (11, 3), (14, 2)],
            Figure::DottedQuarters => &[(0, 3), (6, 3), (12, 3)],
            Figure::Quarters => &[(0, 2), (4, 2), (8, 2)],
            Figure::Pair => &[(0, 4), (12, 2), (14, 2)],
        }
    }
}

/// The band's `figure` through the bar from `at`, every player on every
/// hit, the crash on the bar's first and the china or the snare on the
/// rest.
pub fn figure(score: &mut Score, band: &Band, at: u32, figure: Figure, rng: &mut Rng) {
    for (k, (h, n)) in figure.hits().iter().enumerate() {
        let start = at + h * S;
        for channel in band.guitars {
            for p in band.chord {
                score.add(Note { start, len: n * S - 15, pitch: p, vel: (band.vel)(if k == 0 { 4 } else { -2 }, rng), channel: *channel });
            }
        }
        score.add(Note { start, len: n * S - 15, pitch: band.root, vel: (band.vel)(0, rng), channel: band.bass });
        score.add(Note { start, len: S, pitch: KICK, vel: (band.vel)(-4, rng), channel: band.kit });
        let (pitch, accent) = if k == 0 { (CRASH, 30) } else if rng.chance(0.5) { (CHINA, 22) } else { (SNARE, 8) };
        score.add(Note { start, len: 2 * S, pitch, vel: (band.vel)(accent, rng), channel: band.kit });
    }
}

/// How the ritual runs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Ritual {
    /// Bars the chord is held under the wash.
    pub hold: u32,
    /// Whether the last hit falls on the and of four before the bar line.
    pub early: bool,
    /// Whether the kick keeps running in sixteenths under the hold.
    pub kick_runs: bool,
    /// How long the last hit sounds: an eighth, or rung to `ring`.
    pub ring: u32,
}

/// The ritual from `at`: the chord struck and held `hold` bars, the
/// cymbal's edge washing in sixteenths from the first bar's second beat
/// and the toms rolling down under the last bar, rising to the cut; the
/// kick running on under it where the ritual has it; then the separate
/// last hit, on the bar line after the hold or on the and of four before
/// it. Returns the tick of the last hit.
pub fn ritual(score: &mut Score, band: &Band, at: u32, ritual: Ritual, rng: &mut Rng) -> u32 {
    let bar = score.bar();
    let held = ritual.hold * bar;
    let cut = if ritual.early { at + held - E } else { at + held };
    band.stroke(score, at, cut - at - E / 8, 4, rng);
    let slots = (cut - at) / S;
    for k in 4..slots {
        let x = (k - 4) as f32 / (slots - 4).max(1) as f32;
        score.add(Note { start: at + k * S, len: S - 10, pitch: CRASH_2, vel: (band.vel)(-30 + (26.0 * x) as i32, rng), channel: band.kit });
        if ritual.kick_runs {
            score.add(Note { start: at + k * S, len: S - 10, pitch: KICK, vel: (band.vel)(-18 + (10.0 * x) as i32, rng), channel: band.kit });
        }
    }
    // The toms down through the hold's last bar.
    let roll = (at + held.saturating_sub(bar)).max(at + 4 * S);
    let n = (cut - roll) / S;
    for k in 0..n {
        let tom = TOMS[(k as usize * TOMS.len() / n.max(1) as usize).min(TOMS.len() - 1)];
        score.add(Note { start: roll + k * S, len: S - 10, pitch: tom, vel: (band.vel)(-14 + (22 * k / n.max(1)) as i32, rng), channel: band.kit });
    }
    band.stroke(score, cut, ritual.ring.max(E), 6, rng);
    cut
}

/// One stroke of the band at `at`, sounding `len`; where it `slides`,
/// the guitars run off it down the key in thirty-seconds as it is let go.
pub fn stab(score: &mut Score, band: &Band, at: u32, len: u32, slides: bool, rng: &mut Rng) {
    band.stroke(score, at, len, 6, rng);
    if slides {
        let key = score.key_at(at);
        let top = band.chord[2];
        let Some(d) = key.absolute_degree(key.snap(top)) else { return };
        for channel in band.guitars {
            for k in 1..=4u32 {
                let p = key.pitch(d - k as i32, 4);
                score.add(Note { start: at + len.min(E) + (k - 1) * (S / 2), len: S / 2 - 5, pitch: p, vel: (band.vel)(-10 - 4 * k as i32, rng), channel: *channel });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::score::{Instrument, Role};
    use crate::theory::{Chord, Key, Meter, Mode};

    fn vel(accent: i32, _: &mut Rng) -> u8 {
        (88 + accent).clamp(1, 127) as u8
    }

    fn score() -> Score {
        let instruments = vec![
            Instrument { name: "guitar", program: 30, channel: 1, role: Role::Pluck, low: 38, high: 76, reverb: 20, pan: 0, level: 0.0 },
            Instrument { name: "bass", program: 33, channel: 0, role: Role::Pluck, low: 28, high: 52, reverb: 10, pan: 0, level: 0.0 },
            Instrument { name: "kit", program: 16, channel: 9, role: Role::Percussion, low: 35, high: 59, reverb: 20, pan: 0, level: 0.0 },
        ];
        let mut s = Score::new(Key::new("E", Mode::Aeolian), Meter::new(&[2, 2, 2, 2]), 300.0, instruments, 1.6);
        s.harmony = vec![Chord::triad(0); 8];
        s
    }

    /// The ritual's last hit is separate from the held chord, on the bar
    /// line or the and of four before it, and nothing it strikes outlasts
    /// what it rings to.
    #[test]
    fn the_ritual_ends_on_a_separate_hit() {
        for early in [false, true] {
            let mut s = score();
            let band = Band { guitars: &[1], bass: 0, kit: 9, chord: [40, 47, 52], root: 28, vel };
            let bar = s.bar();
            let cut = ritual(&mut s, &band, 0, Ritual { hold: 2, early, kick_runs: true, ring: 2 * bar }, &mut Rng::new(1));
            assert_eq!(cut, if early { 2 * bar - E } else { 2 * bar });
            let held: Vec<&Note> = s.notes.iter().filter(|n| n.channel == 1 && n.start == 0).collect();
            assert!(held.iter().all(|n| n.end() < cut), "the hold runs into the last hit");
            assert!(s.notes.iter().any(|n| n.channel == 1 && n.start == cut));
        }
    }

    /// Every figure strikes the band on its first sixteenth and stays in
    /// its bar.
    #[test]
    fn every_figure_stays_in_its_bar() {
        for f in [Figure::Dotted, Figure::DottedQuarters, Figure::Quarters, Figure::Pair] {
            let mut s = score();
            let band = Band { guitars: &[1], bass: 0, kit: 9, chord: [40, 47, 52], root: 28, vel };
            figure(&mut s, &band, 0, f, &mut Rng::new(2));
            let bar = s.bar();
            assert!(s.notes.iter().all(|n| n.start < bar && n.end() <= bar), "{f:?}");
            assert!(s.notes.iter().any(|n| n.channel == 1 && n.start == 0));
        }
    }
}
