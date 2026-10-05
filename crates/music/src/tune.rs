//! A tune put in a register on a harmony. The harmony is a cycle of
//! schemata, four bars a row; the tune is one theme in its phrase form
//! on every row, ending on the row's own tones. A voice takes it bar
//! by bar in a register from its home: every tone shifted as the piece
//! says, each strong beat bent to the chord that stands, and the
//! voice leading repaired where the line would leap on after a leap.

use crate::score::{Score, TICKS_PER_EIGHTH as E};
use crate::theory::counterpoint;
use crate::theory::melody::{self, Theme, Tone};
use crate::theory::phrase::{self, Form};
use crate::theory::schema::Schema;
use crate::theory::{Chord, Key, Meter};

pub struct Tune {
    /// The chord of each bar.
    pub chords: Vec<Chord>,
    /// The tune, bar by bar, in degrees from home.
    pub bars: Vec<Vec<Tone>>,
    /// What each bar's degrees are shifted by, the piece's sequence.
    pub shifts: Vec<i32>,
    /// The degrees of the mode the line stands on, where a style sings in
    /// fewer than seven — the blues' pentatonic: the tune's degrees count
    /// this scale's steps, so a step of the theme is a step of the scale
    /// and never a tone struck again. Every degree where `None`.
    pub scale: Option<&'static [i32]>,
}

/// The minor pentatonic as degrees of a minor mode: the tonic, the
/// flat third, the fourth, the fifth and the flat seventh.
pub const MINOR_PENTATONIC: &[i32] = &[0, 2, 3, 4, 6];

/// A note as placed: `(tick, len, pitch)`.
pub type Placed = (u32, u32, u8);

impl Tune {
    /// One bar per shift: the cycle's rows in turn, each row's chords
    /// of `size` tones, and the theme in `form` on each row, its open
    /// ending on the tone of the row's last chord nearest home, its
    /// closed ending home.
    pub fn compose(
        theme: &Theme,
        meter: &Meter,
        form: Form,
        rows: &[&Schema],
        size: u8,
        shifts: Vec<i32>,
    ) -> Tune {
        let mut chords = Vec::new();
        let mut bars = Vec::new();
        for p in 0..shifts.len() / phrase::BARS as usize {
            let schema = rows[p % rows.len()];
            chords.extend((0..4).map(|k| schema.chord(k, size)));
            let mid = open_tone(schema.chord(1, size));
            let end = if schema.closed { 0 } else { open_tone(schema.chord(3, size)) };
            bars.extend(melody::phrase(theme, meter, form, mid, end));
        }
        Tune {
            chords,
            bars,
            shifts,
            scale: None,
        }
    }

    pub fn len(&self) -> u32 {
        self.bars.len() as u32
    }

    /// Bar `b` as pitches in `lo..=hi` from the tune's home there, each
    /// strong beat's tone bent to the chord where it is not on it,
    /// toward the tone before; `sung` as a singer takes it, else every
    /// foot.
    pub fn bar(&self, score: &Score, b: u32, lo: u8, hi: u8, sung: bool) -> Vec<Placed> {
        let key = &score.key_at(b * score.bar());
        let home_degree = score.key.absolute_degree(home_tonic(&score.key, lo, hi)).unwrap();
        let shift = self.shifts[b as usize];
        let chord = self.chords[b as usize];
        let bar = if sung {
            melody::sung(&self.bars[b as usize], &score.meter)
        } else {
            self.bars[b as usize].clone()
        };
        // The tone before the bar, as the shape has it, for the first
        // bend to lead from; none before the first bar.
        let mut prev = b.checked_sub(1).map(|last| last as usize).and_then(|last| {
            self.bars[last]
                .last()
                .map(|t| score.key_at(last as u32 * score.bar()).pitch(home_degree + self.on_scale(t.degree) + self.shifts[last], 4))
        });
        bar.iter()
            .map(|t| {
                let mut pitch = key.pitch(home_degree + self.on_scale(t.degree) + shift, 4);
                if score.meter.strong(t.onset) && !chord.holds(key, pitch) {
                    pitch = bent_to_chord(key, chord, pitch, prev, lo, hi);
                }
                prev = Some(pitch);
                (b * score.bar() + t.onset * E, t.len * E, pitch)
            })
            .collect()
    }

    /// Bars `a..b` as one line, its voice leading repaired: where a leap
    /// is followed by a leap the same way, the landing tone moves to
    /// the nearest tone that makes the first move a step — a chord tone
    /// on a strong beat, any tone of the mode between — or, where none
    /// is, the tone after it moves to the nearest that steps or turns
    /// back, since a line that leaps on falls off its shape.
    pub fn run(&self, score: &Score, a: u32, b: u32, lo: u8, hi: u8, sung: bool) -> Vec<Placed> {
        let mut line: Vec<Placed> = (a..b)
            .flat_map(|bar| self.bar(score, bar, lo, hi, sung))
            .collect();
        self.repair(score, &mut line, lo, hi);
        line
    }

    /// `line`'s voice leading repaired in place, as `run` repairs a run:
    /// for a line pieced from several, where the joins are leaps too.
    pub fn repair(&self, score: &Score, line: &mut [Placed], lo: u8, hi: u8) {
        // Every tone is a degree of the key its bar is heard in, so a
        // borrowed tone counts as the degree it stands for.
        let degree = |tick: u32, p: u8| score.key_at(tick).absolute_degree(p).unwrap();
        for i in 1..line.len().saturating_sub(1) {
            let key = &score.key_at(line[i].0);
            let (d0, d1, d2) = (
                degree(line[i - 1].0, line[i - 1].2),
                degree(line[i].0, line[i].2),
                degree(line[i + 1].0, line[i + 1].2),
            );
            let (leap, next) = (d1 - d0, d2 - d1);
            if leap.abs() < counterpoint::LEAP
                || next.abs() < counterpoint::LEAP
                || leap.signum() != next.signum()
            {
                continue;
            }
            let strong = score.strong(line[i].0);
            let chord = self.chords[(line[i].0 / score.bar()) as usize];
            let target = key.pitch(d0 + 2 * leap.signum(), 4);
            let fixed = (target.saturating_sub(4)..=target.saturating_add(4))
                .filter(|p| {
                    *p != line[i].2
                        && *p >= lo
                        && *p <= hi
                        && self.sings(key, *p)
                        && (!strong || chord.holds(key, *p))
                })
                .filter(|p| (degree(line[i].0, *p) - d0).abs() < counterpoint::LEAP)
                .min_by_key(|p| (*p as i32 - target as i32).abs());
            if let Some(p) = fixed {
                line[i].2 = p;
                continue;
            }
            // Where the landing cannot move — a strong beat with no chord
            // tone a step from where the leap began — the tone after it
            // steps or turns back instead of leaping on.
            let (start, pitch) = (line[i + 1].0, line[i + 1].2);
            let key = &score.key_at(start);
            let strong = score.strong(start);
            let chord = self.chords[(start / score.bar()) as usize];
            let turned = (pitch.saturating_sub(5)..=pitch.saturating_add(5))
                .filter(|p| *p >= lo && *p <= hi && self.sings(key, *p) && (!strong || chord.holds(key, *p)))
                .filter(|p| {
                    let next = degree(start, *p) - d1;
                    next.abs() < counterpoint::LEAP || next.signum() != leap.signum()
                })
                .min_by_key(|p| ((*p as i32 - pitch as i32).abs(), *p));
            if let Some(p) = turned {
                line[i + 1].2 = p;
            }
        }
    }
}

impl Tune {
    /// The mode's degree from home of the tune's `step` from home: the
    /// step itself, or where there is a scale, that many of its steps.
    fn on_scale(&self, step: i32) -> i32 {
        match self.scale {
            Some(scale) => {
                let n = scale.len() as i32;
                7 * step.div_euclid(n) + scale[step.rem_euclid(n) as usize]
            }
            None => step,
        }
    }

    /// Whether the line may stand on `pitch`: in the mode, and on the
    /// scale where there is one.
    fn sings(&self, key: &Key, pitch: u8) -> bool {
        key.absolute_degree(pitch).is_some_and(|d| self.scale.is_none_or(|s| s.contains(&d.rem_euclid(7))))
    }
}

/// The chord's tone nearest home that is not home, as a degree within
/// a fourth of it: where an open phrase rests.
pub fn open_tone(chord: Chord) -> i32 {
    chord
        .degrees()
        .iter()
        .map(|d| {
            let d = d.rem_euclid(7);
            if d > 3 {
                d - 7
            } else {
                d
            }
        })
        .filter(|d| *d != 0)
        .min_by_key(|d| (d.abs(), *d > 0))
        .unwrap()
}

/// A tone bent to the chord: of the chord's tones within two degrees
/// of it, the one nearest the tone before, so a bend leads on from the
/// line and never widens a leap; the nearest chord tone where none is
/// that close.
pub fn bent_to_chord(key: &Key, chord: Chord, pitch: u8, prev: Option<u8>, lo: u8, hi: u8) -> u8 {
    let near: Vec<u8> = (pitch.saturating_sub(4)..=pitch.saturating_add(4))
        .filter(|p| *p >= lo && *p <= hi && chord.holds(key, *p))
        .collect();
    match (near.is_empty(), prev) {
        (false, Some(p)) => *near
            .iter()
            .min_by_key(|c| {
                (
                    (**c as i32 - p as i32).abs(),
                    (**c as i32 - pitch as i32).abs(),
                )
            })
            .unwrap(),
        (false, None) => *near
            .iter()
            .min_by_key(|c| (**c as i32 - pitch as i32).abs())
            .unwrap(),
        (true, _) => nearest_chord_tone(key, chord, pitch, lo, hi),
    }
}

/// The chord tone nearest `pitch` within `lo..=hi`, ties downward.
pub fn nearest_chord_tone(key: &Key, chord: Chord, pitch: u8, lo: u8, hi: u8) -> u8 {
    (0..=12u8)
        .flat_map(|d| [pitch.saturating_sub(d), pitch.saturating_add(d)])
        .find(|p| *p >= lo && *p <= hi && chord.holds(key, *p))
        .unwrap()
}

/// A tune's home in `lo..=hi`: the tonic with room for the tune's whole
/// span — two degrees under it, an octave over — nearest a third under
/// the register's middle, ties downward; the nearest tonic where none
/// has the room.
pub fn home_tonic(key: &Key, lo: u8, hi: u8) -> u8 {
    let seat = (lo + hi) / 2 - 5;
    let nearest = |lo: u8, hi: u8| {
        (lo..=hi)
            .filter(|p| p % 12 == key.tonic)
            .min_by_key(|p| (*p as i32 - seat as i32).abs() * 2 + (*p > seat) as i32)
    };
    nearest(lo + 4, hi - 12)
        .or_else(|| nearest(lo, hi))
        .unwrap()
}
