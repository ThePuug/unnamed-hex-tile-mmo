//! A story is a ladder: the layers of a bed in the order they join,
//! each a notch at a time, and who leads at every rung. A seed walks it
//! one rung a part to each of the story's turns in order, then to each
//! the piece adds, so a part boundary is one layer moving one notch —
//! but at a turn that leaps, where the band drops to its rung at once,
//! a breakdown, or comes back in at once — and the walk ends at its last
//! turn, where the piece's ending takes over. A part is whole
//! half-phrases, so every boundary is a cadence, and a walk is whole
//! harmonic cycles, so it closes on an answer. What the layers are and
//! what each plays at each notch is the piece's; this is the walk.

use crate::rng::Rng;
use crate::score::{Score, Section};
use crate::theory::phrase;

/// A bed: each of a piece's layers at its notch.
pub trait Bed: Copy + Eq {
    type Layer: Copy + 'static;

    /// The bed with `layer` one notch up; the same bed where it is
    /// already at its top.
    fn up(self, layer: Self::Layer) -> Self;

    /// What the sheet calls the step from `from` to this bed: the layer
    /// that moved, and how — in from off, up a notch, down a notch, out
    /// to off.
    fn step_from(self, from: Self) -> &'static str;
}

/// Where a walk turns: the rung it climbs or falls to, how many parts
/// it stays there beyond the one that arrives, and whether it arrives in
/// one part rather than a rung at a time.
#[derive(Clone, Copy)]
pub struct Turn {
    pub rung: (i32, i32),
    pub hold: (i32, i32),
    pub leaps: bool,
}

/// A turn the walk climbs or falls to a rung a part.
pub const fn turn(rung: (i32, i32), hold: (i32, i32)) -> Turn {
    Turn { rung, hold, leaps: false }
}

/// A turn the walk lands on in one part: the band dropping to a few
/// players, or all of it coming back in.
pub const fn leap(rung: (i32, i32), hold: (i32, i32)) -> Turn {
    Turn { rung, hold, leaps: true }
}

pub struct Story<B: Bed + 'static, L: 'static> {
    pub name: &'static str,
    /// How often the seed picks it, against the others.
    pub weight: f32,
    /// What plays at the foot of the ladder.
    pub base: B,
    /// The layers in the order they join, each a notch.
    pub ladder: &'static [B::Layer],
    /// Who leads at each rung, the foot first: one more than the ladder.
    pub leads: &'static [L],
    /// The walk's turns, in order; it opens at the foot and ends at the
    /// last.
    pub turns: &'static [Turn],
    /// Half-phrases a part lasts: a layer moves at a cadence, the
    /// question's or the answer's.
    pub halves: (i32, i32),
    /// The stretch of the groove's tempo band the story is taken at, as
    /// fractions of it: the whole band, or its slow end, or its quick.
    pub pace: (f32, f32),
}

/// A part as placed: its bars, and what plays through them.
pub struct Part<B, L> {
    pub rung: usize,
    pub bed: B,
    pub lead: L,
    pub a: u32,
    pub b: u32,
}

/// Consecutive parts under one lead, as bars.
pub struct Run<L> {
    pub lead: L,
    pub a: u32,
    pub b: u32,
}

/// A walk placed in bars.
pub struct Walk<B, L> {
    pub parts: Vec<Part<B, L>>,
}

/// The gain that gives back `lift` LU.
pub fn gain(lift: f32) -> f32 {
    10f32.powf(-lift / 20.0)
}

/// One of `stories` by weight.
pub fn draw<'a, B: Bed, L>(stories: &'a [Story<B, L>], rng: &mut Rng) -> &'a Story<B, L> {
    let weights: Vec<f32> = stories.iter().map(|s| s.weight).collect();
    &stories[rng.weighted(&weights)]
}

impl<B: Bed, L: Copy> Story<B, L> {
    /// A tempo for the story, eighths a minute, drawn from its stretch
    /// of `band`.
    pub fn tempo(&self, band: (i32, i32), rng: &mut Rng) -> f32 {
        let span = (band.1 - band.0) as f32;
        rng.range(band.0 + (span * self.pace.0).round() as i32, band.0 + (span * self.pace.1).round() as i32) as f32
    }

    /// The bed at `rung`: the base with the first `rung` layers up.
    pub fn bed(&self, rung: usize) -> B {
        self.ladder[..rung].iter().fold(self.base, |t, l| t.up(*l))
    }

    /// The rungs of one walk, the foot first: to each of the story's
    /// turns and then each of `then`, one rung a part or in one where the
    /// turn leaps, held as the turn says, ending at the last.
    pub fn walk(&self, then: &[Turn], rng: &mut Rng) -> Vec<usize> {
        let mut rungs = vec![0usize];
        for t in self.turns.iter().chain(then) {
            let target = rng.range(t.rung.0, t.rung.1) as usize;
            while *rungs.last().unwrap() != target {
                let last = *rungs.last().unwrap();
                rungs.push(if t.leaps { target } else if target > last { last + 1 } else { last - 1 });
            }
            for _ in 0..rng.range(t.hold.0, t.hold.1) {
                rungs.push(target);
            }
        }
        rungs
    }

    /// A walk placed on the score, the story's turns and then `then`,
    /// part by part: each a draw of
    /// half-phrases long, then stretched so the whole is whole cycles
    /// of `cycle` half-phrases — a half at a time to the highest part,
    /// where a longer stay is the crest, and to the last, where the piece
    /// ends, by turns; each part a section, named
    /// for the step that made it — the first for the story, a leap for
    /// the way it went — and trimmed by `trim` of its bed and lead.
    pub fn place(
        &self,
        rng: &mut Rng,
        score: &mut Score,
        cycle: u32,
        then: &[Turn],
        trim: impl Fn(B, L) -> f32,
    ) -> Walk<B, L> {
        let bar = score.bar();
        score.story = self.name;
        let walk = self.walk(then, rng);
        let mut lengths: Vec<u32> = walk
            .iter()
            .map(|_| rng.range(self.halves.0, self.halves.1) as u32)
            .collect();
        let short = (cycle - lengths.iter().sum::<u32>() % cycle) % cycle;
        let top = walk.iter().enumerate().max_by_key(|(i, r)| (**r, std::cmp::Reverse(*i))).map(|(i, _)| i).unwrap();
        let last = lengths.len() - 1;
        for k in 0..short {
            lengths[if k % 2 == 0 { top } else { last }] += 1;
        }
        let mut at = 0;
        let mut parts: Vec<Part<B, L>> = Vec::new();
        for (rung, halves) in walk.into_iter().zip(lengths) {
            let bars = halves * phrase::BARS / 2;
            let bed = self.bed(rung);
            let name = match parts.last() {
                None => self.name,
                Some(prev) if prev.rung > rung + 1 => "breakdown",
                Some(prev) if rung > prev.rung + 1 => "all in",
                Some(prev) => bed.step_from(prev.bed),
            };
            let lead = self.leads[rung];
            score.sections.push(Section {
                name,
                start: at,
                end: at + bars * bar,
                trim: trim(bed, lead),
                level: None,
                rings: false,
            });
            parts.push(Part {
                rung,
                bed,
                lead,
                a: at / bar,
                b: at / bar + bars,
            });
            at += bars * bar;
        }
        Walk { parts }
    }

    /// Why the story is not a ladder, if it is not: a lead per rung,
    /// every rung moving a layer, every turn on the ladder.
    pub fn fault(&self) -> Option<String> {
        if self.leads.len() != self.ladder.len() + 1 {
            return Some(format!("{}: a lead per rung", self.name));
        }
        for r in 0..self.ladder.len() {
            if self.bed(r) == self.bed(r + 1) {
                return Some(format!("{}: rung {} moves nothing", self.name, r + 1));
            }
        }
        for t in self.turns {
            if !(0 <= t.rung.0 && t.rung.0 <= t.rung.1 && t.rung.1 as usize <= self.ladder.len()) {
                return Some(format!("{}: a turn off the ladder", self.name));
            }
            if !(0 <= t.hold.0 && t.hold.0 <= t.hold.1) {
                return Some(format!("{}: a hold out of order", self.name));
            }
        }
        None
    }
}

impl<B: Copy, L: Copy + PartialEq> Walk<B, L> {
    pub fn at(&self, bar: u32) -> &Part<B, L> {
        self.parts
            .iter()
            .find(|p| p.a <= bar && bar < p.b)
            .unwrap_or(self.parts.last().unwrap())
    }

    pub fn bed_at(&self, bar: u32) -> B {
        self.at(bar).bed
    }

    pub fn bars(&self) -> u32 {
        self.parts.last().unwrap().b
    }

    /// The parts grouped by lead, so a tune runs on across the bed's
    /// steps.
    pub fn runs(&self) -> Vec<Run<L>> {
        let mut runs: Vec<Run<L>> = Vec::new();
        for p in &self.parts {
            match runs.last_mut() {
                Some(r) if r.lead == p.lead => r.b = p.b,
                _ => runs.push(Run {
                    lead: p.lead,
                    a: p.a,
                    b: p.b,
                }),
            }
        }
        runs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    struct Two(u8, u8);

    impl Bed for Two {
        type Layer = usize;
        fn up(self, layer: usize) -> Two {
            match layer {
                0 => Two((self.0 + 1).min(2), self.1),
                _ => Two(self.0, (self.1 + 1).min(1)),
            }
        }
        fn step_from(self, from: Two) -> &'static str {
            if self.0 != from.0 {
                "a"
            } else if self.1 != from.1 {
                "b"
            } else {
                "held"
            }
        }
    }

    const STORY: Story<Two, u8> = Story {
        name: "test",
        weight: 1.0,
        base: Two(0, 0),
        ladder: &[0, 1, 0],
        leads: &[0, 1, 1, 2],
        turns: &[turn((3, 3), (0, 1)), turn((1, 1), (0, 0))],
        halves: (1, 2),
        pace: (0.0, 1.0),
    };

    /// Every walk opens at the foot, moves one rung a part at most,
    /// reaches every turn, and ends at the last.
    #[test]
    fn every_walk_moves_a_rung_at_a_time_and_ends_at_its_last_turn() {
        assert_eq!(STORY.fault(), None);
        for seed in 0..64 {
            let walk = STORY.walk(&[], &mut Rng::new(seed));
            assert_eq!(walk[0], 0);
            assert_eq!(*walk.last().unwrap(), 1);
            for w in walk.windows(2) {
                assert!(w[0].abs_diff(w[1]) <= 1, "{walk:?}");
            }
            assert_eq!(*walk.iter().max().unwrap(), 3);
        }
    }

    /// A turn that leaps lands in one part, from wherever the walk is,
    /// and the piece's turns follow the story's.
    #[test]
    fn a_leap_lands_in_one_part() {
        const THEN: [Turn; 2] = [leap((0, 0), (0, 0)), leap((3, 3), (1, 1))];
        for seed in 0..16 {
            let walk = STORY.walk(&THEN, &mut Rng::new(seed));
            let n = walk.len();
            assert_eq!(&walk[n - 4..], &[1, 0, 3, 3], "{walk:?}");
        }
    }

    /// A story whose rung moves nothing, or whose turn leaves the
    /// ladder, is faulted.
    #[test]
    fn a_broken_ladder_is_faulted() {
        let stuck = Story {
            ladder: &[1, 1, 0],
            ..STORY
        };
        assert!(stuck.fault().unwrap().contains("moves nothing"));
        const OFF: [Turn; 1] = [turn((4, 4), (0, 0))];
        let off = Story {
            turns: &OFF,
            ..STORY
        };
        assert!(off.fault().unwrap().contains("off the ladder"));
    }
}
