//! A guitar solo as the players of 1980s metal built one, counted from
//! forty-five Helloween turns and eighteen more — Maiden, Accept, Gamma
//! Ray, Malmsteen, Racer X, Cacophony, Metallica — decoded from their
//! transcriptions (`proofs/research/solos-findings.md`).
//!
//! A turn is gestures: held bends and held tones with vibrato between
//! fast runs, licks, trills, sequences, pedal-point licks and repeated
//! cells, about a quarter of its beats held and three or four phrases to
//! eight bars, split by held tones rather than rests. It opens on a bend
//! about half the time, a flurry a quarter, a held tone or a short lick
//! the rest; it ends held two times in three, else runs on into what
//! follows. Its register climbs to a peak in its last third about half
//! the time and arches the rest; how dense it is runs its own course —
//! fastest in the middle, at the start, evenly or, least often, at the
//! end — drawn apart from the register's. A fast beat's contour is drawn
//! by how often each comes — a zigzag, a pedal, a leap, a two-note
//! trill, a scale falling twice as often as rising — and runs on from
//! the beat before by a step about half the time, a jump a third,
//! a sequence or a repeat the rest, its rhythm changing about one beat in
//! five. Two players are two people: one runs long lines with leaps,
//! repeated cells and odd groupings, Hansen's way; the other plays short
//! bent phrases round a pedal or a neighbour and peaks in the middle,
//! Weikath's. A turn that answers another flips at least two of how it
//! opens, its main note value, its density's course and its top.
//!
//! A turn is pitch and time in the score's key; the piece holds every
//! strong beat to the chord and repairs the line, as it does a tune.

use crate::rng::Rng;
use crate::score::{Score, TICKS_PER_EIGHTH as E};
use crate::tune::Placed;

/// A soloist's habits, from the counts of one player's turns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Player {
    pub name: &'static str,
    /// The share of beats that are fast: runs, licks, trills, cells.
    pub fast: f32,
    /// The most beats one fast gesture runs.
    pub run: u32,
    /// The register a turn spans, in degrees.
    pub span: i32,
    /// The share of fast beats on a pedal, on a turn about a neighbour,
    /// and that leap inside the beat.
    pub pedal: f32,
    pub turn: f32,
    pub leap: f32,
    /// How likely a turn is to repeat a cell machine-gun fashion, and to
    /// run five or seven to the beat.
    pub cells: f32,
    pub odd: f32,
    /// How likely the turn's highest tone falls in its middle third.
    pub peaks_mid: f32,
}

/// Hansen's way: four beats in five fast, runs of seven beats, thirty-one
/// semitones, leaps, cells in two turns of three, odd groupings in half.
pub const SHREDDER: Player = Player { name: "shredder", fast: 0.79, run: 7, span: 18, pedal: 0.14, turn: 0.03, leap: 0.17, cells: 0.68, odd: 0.53, peaks_mid: 0.37 };

/// Weikath's way: half the beats fast, runs of two or three beats,
/// twenty-four semitones, short bent phrases round a pedal or a
/// neighbour, the peak in the middle in two turns of three.
pub const SINGER: Player = Player { name: "singer", fast: 0.52, run: 3, span: 14, pedal: 0.21, turn: 0.12, leap: 0.07, cells: 0.31, odd: 0.31, peaks_mid: 0.69 };

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Opening {
    /// A bend held into, the turn's first tone.
    Bend,
    /// Fast from the first beat.
    Flurry,
    /// A tone held, unbent.
    Held,
    /// A short lick, then a held tone.
    Lick,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Close {
    /// The last tone held to the turn's end.
    Held,
    /// Fast to the last beat, running on into what follows.
    RunOn,
}

/// Where a turn is densest.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Density {
    Arch,
    Flat,
    Fall,
    Rise,
}

/// A turn's main fast value.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Value {
    Sixteenths,
    Sextuplets,
    /// Sixteenths, sextuplets and odd groupings by turns.
    Mixed,
}

/// What a turn is, before its notes: how it opens and closes, the course
/// of its density, its main value, whether it peaks in its middle, and
/// how far under the player's span its top stops, in degrees.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Shape {
    pub opening: Opening,
    pub close: Close,
    pub density: Density,
    pub value: Value,
    pub peaks_mid: bool,
    pub top: i32,
}

impl Shape {
    pub fn draw(player: &Player, rng: &mut Rng) -> Shape {
        Shape {
            opening: [Opening::Bend, Opening::Flurry, Opening::Held, Opening::Lick][rng.weighted(&[21.0, 11.0, 7.0, 6.0])],
            close: [Close::Held, Close::RunOn][rng.weighted(&[30.0, 12.0])],
            density: [Density::Arch, Density::Flat, Density::Fall, Density::Rise][rng.below(4)],
            value: [Value::Sixteenths, Value::Sextuplets, Value::Mixed][rng.weighted(&[5.0, 1.5, 2.0 + 4.0 * player.odd])],
            peaks_mid: rng.chance(player.peaks_mid),
            top: rng.range(0, 3),
        }
    }

    /// A turn answering `to`: drawn again until at least two of its
    /// opening, its main value, its density's course and its top differ,
    /// as the second turn of a block contrasts rather than escalates.
    pub fn answer(to: &Shape, player: &Player, rng: &mut Rng) -> Shape {
        loop {
            let s = Shape::draw(player, rng);
            let flips = [s.opening != to.opening, s.value != to.value, s.density != to.density, s.top != to.top].iter().filter(|f| **f).count();
            if flips >= 2 {
                return s;
            }
        }
    }
}

/// What the solo block holds, part by part.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Part {
    /// A turn by the block's `n`th player.
    Turn(usize),
    /// Both players together: the piece's twin break.
    Together,
}

/// The block's parts in `bars` bars, as the surveyed blocks run: two
/// turns of eight bars with the twin break after, between or before
/// them; or the turns trading eight and then halving to four, as
/// Halloween's do; each part whole bars of two.
pub fn layout(bars: u32, rng: &mut Rng) -> Vec<(u32, Part)> {
    let third = (bars / 3 / 2 * 2).max(2);
    let rest = bars - 2 * third;
    match rng.weighted(&[3.0, 3.0, 2.0, 2.0]) {
        0 => vec![(third, Part::Turn(0)), (third, Part::Turn(1)), (rest, Part::Together)],
        1 => vec![(third, Part::Turn(0)), (rest, Part::Together), (third, Part::Turn(1))],
        2 => vec![(rest, Part::Together), (third, Part::Turn(0)), (third, Part::Turn(1))],
        _ => {
            let long = (bars / 3 / 2 * 2).max(2);
            let short = ((bars - 2 * long) / 2 / 2 * 2).max(2);
            let last = bars - 2 * long - short;
            vec![(long, Part::Turn(0)), (long, Part::Turn(1)), (short, Part::Turn(0)), (last, Part::Turn(1))]
        }
    }
}

/// A beat of the turn: where it starts and how many eighths it spans.
#[derive(Clone, Copy)]
struct Beat {
    at: u32,
    eighths: u32,
}

/// The turn's walk through its register: where it is, its floor, its
/// top, and what the beat before played, for the next to run on from.
struct Walk {
    now: i32,
    floor: i32,
    top: i32,
    last: Option<(Contour, i32)>,
}

/// A fast beat's contour, by how often each comes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Contour {
    Zigzag,
    Pedal,
    Leap,
    Trill,
    ScaleDown,
    Turn,
    Repeat,
    Pairs,
    ScaleUp,
    Arpeggio,
}

impl Contour {
    fn draw(player: &Player, rng: &mut Rng) -> Contour {
        use Contour::*;
        let all = [Zigzag, Pedal, Leap, Trill, ScaleDown, Turn, Repeat, Pairs, ScaleUp, Arpeggio];
        let weights = [0.21, player.pedal, player.leap, 0.12, 0.08, player.turn, 0.06, 0.04, 0.04, 0.02];
        all[rng.weighted(&weights)]
    }

    /// The contour's `n` steps, degrees from the beat's first tone.
    fn steps(self, n: usize) -> Vec<i32> {
        use Contour::*;
        (0..n as i32)
            .map(|k| match self {
                Zigzag => [0, 2, 1, 3][k as usize % 4] + 2 * (k / 4),
                Pedal => if k % 2 == 0 { -k / 2 } else { 2 },
                Leap => if k == 0 { 0 } else { 4 - (k - 1) },
                Trill => k % 2,
                ScaleDown => -k,
                Turn => [0, 1, 0, -1][k as usize % 4],
                Repeat => 0,
                Pairs => -(k / 2),
                ScaleUp => k,
                Arpeggio => 2 * k,
            })
            .collect()
    }
}

/// How many notes a fast beat of `eighths` holds in `value`, switching to
/// another about one beat in five where the value is mixed.
fn division(value: Value, eighths: u32, odd: bool, rng: &mut Rng) -> u32 {
    let sixteenths = 2 * eighths;
    let sextuplets = 3 * eighths;
    let quintuplets = if eighths == 2 { 5 } else { 7 };
    match value {
        Value::Sixteenths if rng.chance(0.2) => if odd && rng.chance(0.5) { quintuplets } else { sextuplets },
        Value::Sixteenths => sixteenths,
        Value::Sextuplets if rng.chance(0.2) => sixteenths,
        Value::Sextuplets => sextuplets,
        Value::Mixed => [sixteenths, sextuplets, if odd { quintuplets } else { sixteenths }, 4 * eighths][rng.weighted(&[5.0, 3.0, 2.0, 0.5])],
    }
}

/// A turn on bars `a..z` of `score` in `lo..=hi`, by `player`, shaped as
/// `shape`: the notes as placed, in the bars' keys, before the piece
/// holds its strong beats to the chords. `entry` is how far from the bar
/// line it comes in: none, a pickup in the bar before, or late.
pub fn turn(score: &Score, a: u32, z: u32, (lo, hi): (u8, u8), player: &Player, shape: &Shape, rng: &mut Rng) -> Vec<Placed> {
    let bar = score.bar();
    let groups: Vec<u32> = score.meter.groups.iter().map(|g| *g as u32).collect();
    let mut beats: Vec<Beat> = Vec::new();
    for b in a..z {
        let mut at = b * bar;
        for g in &groups {
            beats.push(Beat { at, eighths: *g });
            at += g * E;
        }
    }
    let n = beats.len();
    let key = score.key;
    let degree = |p: u8| key.absolute_degree(key.snap(p)).unwrap();
    let floor = degree(lo + 2);
    let top = (floor + player.span - shape.top).min(degree(hi.saturating_sub(2))).max(floor + 5);
    let mut walk = Walk { now: floor + (top - floor) * 2 / 5, floor, top, last: None };
    let tone = |at: u32, d: i32| score.key_at(at).pitch(d, 4);
    // The ceiling over the walk at `x` of the turn: rising to the top in
    // its last third, or arching to it in its middle.
    let ceiling = |x: f32| -> i32 {
        let reach = if shape.peaks_mid { 1.0 - (2.0 * x - 1.0).abs() } else { x.min(0.85) / 0.85 };
        floor + ((top - floor) as f32 * (0.5 + 0.5 * reach)).round() as i32
    };
    // How fast the turn is at `x`: the player's share along the density's
    // course.
    let fast_at = |x: f32| -> f32 {
        let f = player.fast;
        (match shape.density {
            Density::Arch => f * (0.55 + 0.9 * (1.0 - (2.0 * x - 1.0).abs())),
            Density::Rise => f * (0.55 + 0.9 * x),
            Density::Fall => f * (1.45 - 0.9 * x),
            Density::Flat => f,
        })
        .clamp(0.1, 0.95)
    };
    let odd = rng.chance(player.odd);
    let cells = rng.chance(player.cells);
    let mut line: Vec<Placed> = Vec::new();
    let mut i = 0usize;
    // The opening.
    let open_len = match shape.opening {
        Opening::Bend | Opening::Held => rng.range(2, 3) as usize,
        Opening::Lick => 2,
        Opening::Flurry => rng.range(2, 4) as usize,
    }
    .min(n);
    match shape.opening {
        Opening::Bend | Opening::Held => {
            let d = chord_tone(score, beats[0].at, walk.now + 1);
            held(&mut line, &beats[..open_len], tone(beats[0].at, d));
            walk.now = d;
        }
        Opening::Lick => {
            // Two eighths stepping up into a tone held the next beat.
            let d = chord_tone(score, beats[0].at, walk.now);
            let half = beats[0].eighths * E / 2;
            line.push((beats[0].at, half, tone(beats[0].at, d - 2)));
            line.push((beats[0].at + half, beats[0].eighths * E - half, tone(beats[0].at, d - 1)));
            held(&mut line, &beats[1..open_len], tone(beats[1.min(n - 1)].at, d));
            walk.now = d;
        }
        Opening::Flurry => {
            fast(score, &mut line, &beats[..open_len], &mut walk, player, shape.value, odd, cells, ceiling(0.0), rng);
        }
    }
    i += open_len;
    // The close: its beats at the end, held or running on.
    let close_len = if shape.close == Close::Held { rng.range(2, 4) as usize } else { 0 };
    let body_end = n.saturating_sub(close_len).max(i);
    while i < body_end {
        let x = i as f32 / n as f32;
        let ceil = ceiling(x);
        if rng.chance(fast_at(x)) {
            let len = (rng.range(1, player.run as i32) as usize).min(body_end - i);
            fast(score, &mut line, &beats[i..i + len], &mut walk, player, shape.value, odd, cells, ceil, rng);
            i += len;
        } else {
            let len = (rng.range(1, 3) as usize).min(body_end - i);
            slow(score, &mut line, &beats[i..i + len], &mut walk, ceil, rng);
            i += len;
        }
    }
    if close_len > 0 && i < n {
        // Held at the peak where the turn climbs to its end, else where
        // the walk is.
        let goal = if shape.peaks_mid { walk.now } else { walk.top };
        let d = chord_tone(score, beats[i].at, goal);
        // A step or two of run-up into it.
        let at = beats[i].at;
        let step = beats[i].eighths * E / 4;
        for k in 0..2 {
            line.push((at + k * step, step, tone(at, d - 2 + k as i32)));
        }
        let start = at + 2 * step;
        let end = beats[n - 1].at + beats[n - 1].eighths * E;
        line.push((start, end - start, tone(at, d)));
    }
    // Into the register by octaves, where a chord tone sought near a
    // bound fell past it.
    for n in line.iter_mut() {
        while n.2 < lo {
            n.2 += 12;
        }
        while n.2 > hi {
            n.2 -= 12;
        }
    }
    line
}

/// The chord tone of the bar at `at` nearest degree `d`.
fn chord_tone(score: &Score, at: u32, d: i32) -> i32 {
    let key = score.key_at(at);
    let chord = score.chord_at(at);
    (d - 3..=d + 3).filter(|c| chord.holds(&key, key.pitch(*c, 4))).min_by_key(|c| ((c - d).abs(), -c)).unwrap_or(d)
}

/// One tone held across `beats`.
fn held(line: &mut Vec<Placed>, beats: &[Beat], pitch: u8) {
    if let (Some(first), Some(last)) = (beats.first(), beats.last()) {
        line.push((first.at, last.at + last.eighths * E - first.at, pitch));
    }
}

/// A slow gesture over `beats`: a held tone bent into or shaken, a bent
/// melody in eighths and quarters, or a short motif played twice.
fn slow(score: &Score, line: &mut Vec<Placed>, beats: &[Beat], walk: &mut Walk, ceil: i32, rng: &mut Rng) {
    let tone = |at: u32, d: i32| score.key_at(at).pitch(d, 4);
    match rng.weighted(&[33.0, 34.0, 22.0, 14.0]) {
        0 | 1 => {
            let d = chord_tone(score, beats[0].at, (walk.now + if rng.chance(0.5) { 1 } else { -1 }).clamp(walk.floor, ceil));
            held(line, beats, tone(beats[0].at, d));
            walk.now = d;
        }
        2 => {
            // A bent melody: a quarter, two eighths, stepping.
            for b in beats {
                let d = chord_tone(score, b.at, (walk.now + rng.range(-1, 1)).clamp(walk.floor, ceil));
                let half = b.eighths * E / 2;
                line.push((b.at, half, tone(b.at, d)));
                line.push((b.at + half, b.eighths * E - half, tone(b.at, d + if rng.chance(0.5) { 1 } else { -1 })));
                walk.now = d;
            }
        }
        _ => {
            // A motif of a beat, played again.
            let d = chord_tone(score, beats[0].at, walk.now);
            let cell = [0, -1, 1][rng.below(3)];
            for b in beats {
                let third = b.eighths * E / 3;
                line.push((b.at, third, tone(b.at, d)));
                line.push((b.at + third, third, tone(b.at, d + cell)));
                line.push((b.at + 2 * third, b.eighths * E - 2 * third, tone(b.at, d)));
            }
            walk.now = d;
        }
    }
    walk.last = None;
}

/// A fast gesture over `beats`: a mixed lick of contours drawn a beat at
/// a time; a sequence of one shape stepping down; a pedal-point lick; a
/// cell repeated machine-gun fashion; a two-note trill; a scale run
/// falling an octave; a scale with every tone picked twice; or, seldom,
/// an arpeggio swept up and down the chord.
#[allow(clippy::too_many_arguments)]
fn fast(score: &Score, line: &mut Vec<Placed>, beats: &[Beat], walk: &mut Walk, player: &Player, value: Value, odd: bool, cells: bool, ceil: i32, rng: &mut Rng) {
    let kind = rng.weighted(&[25.0, 14.0, 12.0, if cells { 12.0 } else { 2.0 }, 8.0, 6.0, 4.0, 2.0]);
    let pedal_tone = walk.now;
    let mut seq_shape: Option<Contour> = None;
    for (k, b) in beats.iter().enumerate() {
        let n = division(value, b.eighths, odd, rng) as usize;
        let unit = b.eighths * E / n as u32;
        let key = score.key_at(b.at);
        // Where the beat starts, run on from the beat before.
        let link = rng.weighted(&[51.0, 36.0, 7.0, 6.0]);
        let (contour, start) = match (kind, walk.last) {
            (0, Some((c, s))) if link == 3 => (c, s),
            (0, Some((c, s))) if link == 2 => (c, s - 1),
            (0, _) => {
                let jump = if link == 1 { [3, -3, 4, -4][rng.below(4)] } else if rng.chance(0.5) { 1 } else { -1 };
                (Contour::draw(player, rng), walk.now + jump)
            }
            (1, _) => {
                let c = *seq_shape.get_or_insert([Contour::Zigzag, Contour::ScaleDown, Contour::Turn][rng.below(3)]);
                (c, walk.now - if k == 0 { 0 } else { 1 })
            }
            (2, _) => (Contour::Pedal, pedal_tone + 2 - k as i32),
            (3, Some((c, s))) => (c, s),
            (3, None) => (Contour::Zigzag, walk.now),
            (4, _) => (Contour::Trill, walk.now),
            (5, _) => (Contour::ScaleDown, if k == 0 { walk.now + 3 } else { walk.now }),
            (6, _) => (Contour::Pairs, walk.now),
            _ => (Contour::Arpeggio, walk.now - 2),
        };
        // Turn about at the bounds.
        let start = chord_tone(score, b.at, start.clamp(walk.floor + 2, (ceil - 2).max(walk.floor + 2)));
        let steps = contour.steps(n);
        for (j, s) in steps.iter().enumerate() {
            let d = (start + s).clamp(walk.floor, ceil + 1);
            line.push((b.at + j as u32 * unit, unit, key.pitch(d, 4)));
        }
        let end = start + steps.last().copied().unwrap_or(0);
        walk.now = end.clamp(walk.floor, ceil);
        walk.last = Some((contour, start));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::score::{Instrument, Role};
    use crate::theory::{Chord, Key, Meter, Mode};

    fn score() -> Score {
        let key = Key::new("E", Mode::Aeolian);
        let inst = vec![Instrument { name: "solo", program: 30, channel: 0, role: Role::Melody, low: 59, high: 88, reverb: 40, pan: 0, level: 0.0 }];
        let mut s = Score::new(key, Meter::new(&[2, 2, 2, 2]), 300.0, inst, 1.6);
        s.harmony = [0, 5, 0, 6, 0, 5, 6, 0].iter().map(|r| Chord::triad(*r)).collect();
        s
    }

    /// A turn fills its bars, stays in its register, and two turns of one
    /// player drawn apart are not one line.
    #[test]
    fn a_turn_fills_its_bars_in_its_register() {
        let s = score();
        for seed in 0..40 {
            let mut rng = Rng::new(seed);
            let player = if seed % 2 == 0 { &SHREDDER } else { &SINGER };
            let shape = Shape::draw(player, &mut rng);
            let line = turn(&s, 0, 8, (59, 88), player, &shape, &mut rng);
            assert!(!line.is_empty());
            let end = line.iter().map(|n| n.0 + n.1).max().unwrap();
            assert!(end <= 8 * s.bar() && end + 4 * E >= 8 * s.bar(), "seed {seed}: a turn ending at {end}");
            assert!(line.iter().all(|n| (59..=88).contains(&n.2)), "seed {seed}: out of register");
            assert!(line.windows(2).all(|w| w[0].0 <= w[1].0), "seed {seed}: out of order");
        }
    }

    /// An answering turn flips at least two of what the turn it answers
    /// is, and the shredder plays more fast notes than the singer.
    #[test]
    fn an_answer_contrasts_and_the_players_differ() {
        let s = score();
        let (mut shred, mut sing) = (0usize, 0usize);
        for seed in 0..40 {
            let mut rng = Rng::new(seed);
            let a = Shape::draw(&SHREDDER, &mut rng);
            let b = Shape::answer(&a, &SINGER, &mut rng);
            let flips = [a.opening != b.opening, a.value != b.value, a.density != b.density, a.top != b.top].iter().filter(|f| **f).count();
            assert!(flips >= 2);
            shred += turn(&s, 0, 8, (59, 88), &SHREDDER, &a, &mut rng).len();
            sing += turn(&s, 0, 8, (59, 88), &SINGER, &b, &mut rng).len();
        }
        assert!(shred > sing, "{shred} against {sing}");
    }

    /// Every layout fills its bars, whole bars of two, with a turn for
    /// each player.
    #[test]
    fn every_layout_fills_the_block() {
        for seed in 0..40 {
            let parts = layout(24, &mut Rng::new(seed));
            assert_eq!(parts.iter().map(|(n, _)| n).sum::<u32>(), 24);
            assert!(parts.iter().all(|(n, _)| *n >= 2 && n % 2 == 0));
            assert!(parts.contains(&(parts[0].0, Part::Turn(0))) || parts.iter().any(|p| p.1 == Part::Turn(0)));
            assert!(parts.iter().any(|p| p.1 == Part::Turn(1)));
        }
    }
}
