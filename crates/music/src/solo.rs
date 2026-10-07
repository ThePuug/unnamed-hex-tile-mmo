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
//! Other idioms are other players. A power ballad's soloist bends and
//! holds more and opens held; a blues player breathes — phrases of six or
//! seven beats with rests between, most entering off the beat or pushed
//! ahead of it, one in three restating an earlier one, on the minor
//! pentatonic in eighths; a wedding band's soloist runs sixteenths
//! regrouped against the dance's limp, turns about a tone, sequences cells,
//! and closes every two or four bars on a held tone.
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
    /// The register a turn spans, in the steps of its scale.
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
    /// Whether it breaks a beat into threes, fives, sevens and
    /// thirty-seconds as a shredder does; else its fast beats are sixteenths
    /// and eighths alone.
    pub tuplets: bool,
    /// Whether it strikes a tone again inside a run — a tone hammered
    /// through a beat, a scale with every tone picked twice — as a
    /// picking guitarist does and a wind or a horn does not.
    pub picks: bool,
    /// How often a turn opens on a bend, a flurry, a held tone and a
    /// lick; ends held and runs on; and is densest in its middle, evenly,
    /// at its start and at its end.
    pub opens: [f32; 4],
    pub ends: [f32; 2],
    pub density: [f32; 4],
    /// How often its main fast value is eighths, sixteenths, sextuplets,
    /// or mixed.
    pub values: [f32; 4],
    /// How it breathes; one unbroken line where it does not.
    pub phrasing: Option<Phrasing>,
    /// The degrees of the mode it stands on, where it plays fewer than
    /// seven, its every step one of them; every degree where none.
    pub scale: Option<&'static [i32]>,
}

/// How a player breathes: phrases of `len` beats — bars where `in_bars` —
/// with rests of `rest` beats between; a phrase entering an eighth ahead
/// of its beat `pickup` of the time and an eighth after it `late`, else
/// on it; restating an earlier phrase `again` of the time. A phrase not
/// the turn's last closes on a held tone.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Phrasing {
    pub len: (u32, u32),
    pub in_bars: bool,
    pub rest: (u32, u32),
    pub pickup: f32,
    pub late: f32,
    pub again: f32,
}

/// Hansen's way: four beats in five fast, runs of seven beats, thirty-one
/// semitones, leaps, cells in two turns of three, odd groupings in half.
pub const SHREDDER: Player = Player { name: "shredder", fast: 0.79, run: 7, span: 18, pedal: 0.14, turn: 0.03, leap: 0.17, cells: 0.68, odd: 0.53, peaks_mid: 0.37, tuplets: true, picks: true, opens: METAL_OPENS, ends: METAL_ENDS, density: [1.0; 4], values: [0.0, 5.0, 1.5, 2.0 + 4.0 * 0.53], phrasing: None, scale: None };

/// Weikath's way: half the beats fast, runs of two or three beats,
/// twenty-four semitones, short bent phrases round a pedal or a
/// neighbour, the peak in the middle in two turns of three.
pub const SINGER: Player = Player { name: "singer", fast: 0.52, run: 3, span: 14, pedal: 0.21, turn: 0.12, leap: 0.07, cells: 0.31, odd: 0.31, peaks_mid: 0.69, tuplets: true, picks: true, opens: METAL_OPENS, ends: METAL_ENDS, density: [1.0; 4], values: [0.0, 5.0, 1.5, 2.0 + 4.0 * 0.31], phrasing: None, scale: None };

/// How the speed-metal turns surveyed open and end.
const METAL_OPENS: [f32; 4] = [21.0, 11.0, 7.0, 6.0];
const METAL_ENDS: [f32; 2] = [30.0, 12.0];

/// A power ballad's soloist, from forty-seven turns
/// (`proofs/research/ballad-guitars-findings.md`): slower than a speed
/// metal turn, bent twice as often, a tone held about every bar; opening
/// held more than bent, ending held about half the time; its peak in the
/// middle half the time and its fastest playing most often in its last
/// third, after the peak.
pub const BALLADEER: Player = Player { name: "balladeer", fast: 0.5, run: 3, span: 14, pedal: 0.18, turn: 0.1, leap: 0.06, cells: 0.2, odd: 0.15, peaks_mid: 0.51, tuplets: true, picks: true, opens: [32.0, 19.0, 38.0, 11.0], ends: [53.0, 47.0], density: [20.0, 20.0, 20.0, 40.0], values: [0.0, 5.0, 1.5, 2.6], phrasing: None, scale: None };

/// A blues soloist, from ninety-seven blues solos of the Weimar Jazz
/// Database and the recordings' transcriptions
/// (`proofs/research/blues-findings.md`): phrases of six or seven beats
/// with a beat or two of rest between, five or six a chorus; one in five
/// pushed in ahead of its beat, almost none on it; one in three
/// restating an earlier one; steps and thirds of the minor pentatonic,
/// walked in its own steps, in eighths — a beat of sixteenths now and
/// then, a turn mostly of them seldom — long tones only where a phrase
/// ends; denser as it goes, its peak in its second half.
pub const BLUESMAN: Player = Player {
    name: "bluesman",
    fast: 0.7,
    run: 2,
    span: 8,
    pedal: 0.12,
    turn: 0.15,
    leap: 0.05,
    cells: 0.35,
    odd: 0.0,
    peaks_mid: 0.3,
    tuplets: false,
    picks: false,
    opens: [30.0, 10.0, 20.0, 40.0],
    ends: [70.0, 30.0],
    density: [20.0, 15.0, 10.0, 55.0],
    values: [9.0, 1.0, 0.0, 0.0],
    phrasing: Some(Phrasing { len: (4, 10), in_bars: false, rest: (1, 2), pickup: 0.22, late: 0.71, again: 0.34 }),
    scale: Some(crate::tune::MINOR_PENTATONIC),
};

/// A village band's soloist over a horo's vamp
/// (`proofs/research/ornaments-findings.md`): sixteenths and eighths,
/// never a shredder's tuplets, regrouped against the limp by the dance's
/// own uneven groups; turns about a tone and trills
/// on it far more than a guitarist's, cells sequenced, few leaps; phrases
/// of two or four bars, each closed on a held tone, re-entering on the
/// beat with hardly a breath; faster as it goes, as everything at a
/// wedding is.
pub const BALKAN: Player = Player {
    name: "village soloist",
    fast: 0.7,
    run: 8,
    span: 13,
    pedal: 0.1,
    turn: 0.25,
    leap: 0.08,
    cells: 0.5,
    odd: 0.0,
    peaks_mid: 0.3,
    tuplets: false,
    picks: false,
    opens: [0.0, 35.0, 40.0, 25.0],
    ends: [70.0, 30.0],
    density: [1.0, 1.0, 1.0, 2.0],
    values: [2.0, 6.0, 0.0, 0.0],
    phrasing: Some(Phrasing { len: (2, 4), in_bars: true, rest: (0, 1), pickup: 0.0, late: 0.0, again: 0.3 }),
    scale: None,
};

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
    /// Eighths, now and then a beat of sixteenths: a blues line's.
    Eighths,
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
            opening: [Opening::Bend, Opening::Flurry, Opening::Held, Opening::Lick][rng.weighted(&player.opens)],
            close: [Close::Held, Close::RunOn][rng.weighted(&player.ends)],
            density: [Density::Arch, Density::Flat, Density::Fall, Density::Rise][rng.weighted(&player.density)],
            value: [Value::Eighths, Value::Sixteenths, Value::Sextuplets, Value::Mixed][rng.weighted(&player.values)],
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
    scale: Option<&'static [i32]>,
}

/// The mode degree a walk's step `s` stands on: the step itself, or the
/// `s`th step of `scale` counted up the octaves.
fn degree_of(scale: Option<&[i32]>, s: i32) -> i32 {
    match scale {
        None => s,
        Some(sc) => {
            let n = sc.len() as i32;
            7 * s.div_euclid(n) + sc[s.rem_euclid(n) as usize]
        }
    }
}

/// The step of `scale` at or under mode degree `d`.
fn step_of(scale: Option<&[i32]>, d: i32) -> i32 {
    match scale {
        None => d,
        Some(sc) => {
            let r = d.rem_euclid(7);
            d.div_euclid(7) * sc.len() as i32 + sc.iter().rposition(|x| *x <= r).unwrap_or(0) as i32
        }
    }
}

/// Step `s` as a pitch in the key of the bar at `at`.
fn tone_at(score: &Score, scale: Option<&[i32]>, at: u32, s: i32) -> u8 {
    score.key_at(at).pitch(degree_of(scale, s), 4)
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
        let picked = if player.picks { 1.0 } else { 0.0 };
        let weights = [0.21, player.pedal, player.leap, 0.12, 0.08, player.turn, 0.06 * picked, 0.04 * picked, 0.04, 0.02];
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
fn division(value: Value, eighths: u32, odd: bool, tuplets: bool, rng: &mut Rng) -> u32 {
    let sixteenths = 2 * eighths;
    if !tuplets {
        return match value {
            Value::Eighths if rng.chance(0.2) => sixteenths,
            Value::Eighths => eighths,
            _ => sixteenths,
        };
    }
    let sextuplets = 3 * eighths;
    let quintuplets = if eighths == 2 { 5 } else { 7 };
    match value {
        Value::Eighths if rng.chance(0.2) => sixteenths,
        Value::Eighths => eighths,
        Value::Sixteenths if rng.chance(0.2) => if odd && rng.chance(0.5) { quintuplets } else { sextuplets },
        Value::Sixteenths => sixteenths,
        Value::Sextuplets if rng.chance(0.2) => sixteenths,
        Value::Sextuplets => sextuplets,
        Value::Mixed => [sixteenths, sextuplets, if odd { quintuplets } else { sixteenths }, 4 * eighths][rng.weighted(&[5.0, 3.0, 2.0, 0.5])],
    }
}

/// A turn on bars `a..z` of `score` in `lo..=hi`, by `player`, shaped as
/// `shape`: the notes as placed, in the bars' keys, before the piece
/// holds its strong beats to the chords. A player that breathes leaves
/// rests between its phrases, and may end before `z`.
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
    let scale = player.scale;
    let degree = |p: u8| step_of(scale, key.absolute_degree(key.snap(p)).unwrap());
    let floor = degree(lo + 2);
    let top = (floor + player.span - shape.top).min(degree(hi.saturating_sub(2))).max(floor + 5);
    let mut walk = Walk { now: floor + (top - floor) * 2 / 5, floor, top, last: None, scale };
    let tone = |at: u32, d: i32| tone_at(score, scale, at, d);
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
    let spans = match &player.phrasing {
        Some(p) => phrases(n, groups.len(), p, rng),
        None => vec![(0, n)],
    };
    let mut line: Vec<Placed> = Vec::new();
    let mut said: Vec<(usize, Vec<Placed>)> = Vec::new();
    for (k, &(from, to)) in spans.iter().enumerate() {
        let last = k + 1 == spans.len();
        let mut part: Vec<Placed> = Vec::new();
        // An earlier phrase restated from this one's first beat, re-keyed,
        // as far as this one runs.
        if k > 0 && player.phrasing.is_some_and(|p| rng.chance(p.again)) {
            let (at, notes) = &said[rng.below(said.len())];
            let shift = beats[from].at as i64 - beats[*at].at as i64;
            let end = beats[to - 1].at + beats[to - 1].eighths * E;
            for (s, l, p) in notes {
                let s = (*s as i64 + shift).max(0) as u32;
                if s + l <= end {
                    part.push((s, *l, tone(s, step_of(scale, score.key_at(s).standing_degree(*p)))));
                }
            }
        }
        if part.is_empty() {
            let mut i = from;
            if k == 0 {
                let open_len = match shape.opening {
                    Opening::Bend | Opening::Held => rng.range(2, 3) as usize,
                    Opening::Lick => 2,
                    Opening::Flurry => rng.range(2, 4) as usize,
                }
                .min(to - from);
                match shape.opening {
                    Opening::Bend | Opening::Held => {
                        let d = chord_tone(score, scale, beats[from].at, walk.now + 1);
                        held(&mut part, &beats[from..from + open_len], tone(beats[from].at, d));
                        walk.now = d;
                    }
                    Opening::Lick => {
                        // Two eighths stepping up into a tone held the next
                        // beat.
                        let d = chord_tone(score, scale, beats[from].at, walk.now);
                        let half = beats[from].eighths * E / 2;
                        part.push((beats[from].at, half, tone(beats[from].at, d - 2)));
                        part.push((beats[from].at + half, beats[from].eighths * E - half, tone(beats[from].at, d - 1)));
                        held(&mut part, &beats[(from + 1).min(to - 1)..from + open_len], tone(beats[(from + 1).min(to - 1)].at, d));
                        walk.now = d;
                    }
                    Opening::Flurry => {
                        fast(score, &mut part, &beats[from..from + open_len], &mut walk, player, shape.value, odd, cells, ceiling(from as f32 / n as f32), rng);
                    }
                }
                i += open_len;
            }
            // The close: the turn's at its end, held or running on; a
            // phrase's before it, its last beat held.
            let close_len = match (last, shape.close, player.phrasing.is_some()) {
                (true, Close::Held, _) => rng.range(2, 4) as usize,
                (true, Close::RunOn, _) => 0,
                (false, _, true) => 1,
                (false, _, false) => 0,
            };
            let body_end = to.saturating_sub(close_len).max(i);
            while i < body_end {
                let x = i as f32 / n as f32;
                let ceil = ceiling(x);
                if rng.chance(fast_at(x)) {
                    let len = (rng.range(1, player.run as i32) as usize).min(body_end - i);
                    fast(score, &mut part, &beats[i..i + len], &mut walk, player, shape.value, odd, cells, ceil, rng);
                    i += len;
                } else {
                    let len = (rng.range(1, 3) as usize).min(body_end - i);
                    slow(score, &mut part, &beats[i..i + len], &mut walk, ceil, rng);
                    i += len;
                }
            }
            if i < to {
                if last {
                    // Held at the peak where the turn climbs to its end, else
                    // where the walk is, a step or two of run-up into it.
                    let goal = if shape.peaks_mid { walk.now } else { walk.top };
                    let d = chord_tone(score, scale, beats[i].at, goal);
                    let at = beats[i].at;
                    let step = beats[i].eighths * E / 4;
                    for k in 0..2 {
                        part.push((at + k * step, step, tone(at, d - 2 + k as i32)));
                    }
                    let start = at + 2 * step;
                    let end = beats[to - 1].at + beats[to - 1].eighths * E;
                    part.push((start, end - start, tone(at, d)));
                } else {
                    let d = chord_tone(score, scale, beats[i].at, walk.now);
                    held(&mut part, &beats[i..to], tone(beats[i].at, d));
                    walk.now = d;
                    walk.last = None;
                }
            }
        }
        part.sort_by_key(|n| n.0);
        // Where it enters: pushed an eighth ahead into the rest before, or
        // an eighth after the beat.
        if let (Some(p), Some(first)) = (&player.phrasing, part.first().copied()) {
            let room = k > 0 && from > spans[k - 1].1;
            if room && rng.chance(p.pickup) {
                part[0] = (first.0 - E, first.1 + E, first.2);
            } else if rng.chance(p.late / (1.0 - p.pickup).max(0.01)) {
                if first.1 > E {
                    part[0] = (first.0 + E, first.1 - E, first.2);
                } else {
                    part.retain(|n| n.0 >= first.0 + E);
                }
            }
        }
        said.push((from, part.clone()));
        line.extend(part);
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
    line.sort_by_key(|n| n.0);
    // A wind or a horn holds a tone it arrives on rather than striking it
    // again: a short tone and the same tone straight after it are one.
    if !player.picks {
        let mut held: Vec<Placed> = Vec::with_capacity(line.len());
        for n in line {
            match held.last_mut() {
                Some(m) if m.2 == n.2 && m.1 <= E && m.0 + m.1 == n.0 => m.1 += n.1,
                _ => held.push(n),
            }
        }
        line = held;
    }
    line
}

/// A turn of `n` beats, `per_bar` to a bar, as `p` breathes it: the beats
/// each phrase spans, the rests between. A phrase too short to say
/// anything joins the one before.
fn phrases(n: usize, per_bar: usize, p: &Phrasing, rng: &mut Rng) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < n {
        let len = rng.range(p.len.0 as i32, p.len.1 as i32) as usize * if p.in_bars { per_bar } else { 1 };
        let to = (i + len).min(n);
        match out.last_mut() {
            Some(last) if to - i < 2 => last.1 = to,
            _ => out.push((i, to)),
        }
        i = to + rng.range(p.rest.0 as i32, p.rest.1 as i32) as usize;
    }
    out
}

/// The step of `scale` nearest step `d` that is a tone of the chord of
/// the bar at `at`.
fn chord_tone(score: &Score, scale: Option<&[i32]>, at: u32, d: i32) -> i32 {
    let key = score.key_at(at);
    let chord = score.chord_at(at);
    (d - 3..=d + 3).filter(|c| chord.holds(&key, tone_at(score, scale, at, *c))).min_by_key(|c| ((c - d).abs(), -c)).unwrap_or(d)
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
    let scale = walk.scale;
    let tone = |at: u32, d: i32| tone_at(score, scale, at, d);
    match rng.weighted(&[33.0, 34.0, 22.0, 14.0]) {
        0 | 1 => {
            let d = chord_tone(score, scale, beats[0].at, (walk.now + if rng.chance(0.5) { 1 } else { -1 }).clamp(walk.floor, ceil));
            held(line, beats, tone(beats[0].at, d));
            walk.now = d;
        }
        2 => {
            // A bent melody: a quarter, two eighths, stepping.
            for b in beats {
                let d = chord_tone(score, scale, b.at, (walk.now + rng.range(-1, 1)).clamp(walk.floor, ceil));
                let half = b.eighths * E / 2;
                line.push((b.at, half, tone(b.at, d)));
                line.push((b.at + half, b.eighths * E - half, tone(b.at, d + if rng.chance(0.5) { 1 } else { -1 })));
                walk.now = d;
            }
        }
        _ => {
            // A motif of a beat, played again.
            let d = chord_tone(score, scale, beats[0].at, walk.now);
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
    let kind = rng.weighted(&[25.0, 14.0, 12.0, if cells { 12.0 } else { 2.0 }, 8.0, 6.0, if player.picks { 4.0 } else { 0.0 }, 2.0]);
    let pedal_tone = walk.now;
    let mut seq_shape: Option<Contour> = None;
    for (k, b) in beats.iter().enumerate() {
        let n = division(value, b.eighths, odd, player.tuplets, rng) as usize;
        let unit = b.eighths * E / n as u32;
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
        let want = start.clamp(walk.floor + 2, (ceil - 2).max(walk.floor + 2));
        let mut start = chord_tone(score, walk.scale, b.at, want);
        // A beat opening on the tone the last one ended on strikes it
        // again; the chord's next tone the way the run was going instead.
        if !player.picks && line.last().is_some_and(|n| n.2 == tone_at(score, walk.scale, b.at, start)) {
            let way = if want >= walk.now { 1 } else { -1 };
            let key = score.key_at(b.at);
            let chord = score.chord_at(b.at);
            if let Some(next) = (1..=3).map(|k| start + way * k).find(|c| chord.holds(&key, tone_at(score, walk.scale, b.at, *c))) {
                start = next;
            }
        }
        let steps = contour.steps(n);
        // A run that reaches a bound turns back from it, as a hand does,
        // rather than striking the bound again.
        let bounce = |d: i32| {
            let hi = ceil + 1;
            if d < walk.floor {
                (2 * walk.floor - d).min(hi)
            } else if d > hi {
                (2 * hi - d).max(walk.floor)
            } else {
                d
            }
        };
        for (j, s) in steps.iter().enumerate() {
            line.push((b.at + j as u32 * unit, unit, tone_at(score, walk.scale, b.at, bounce(start + s))));
        }
        let end = bounce(start + steps.last().copied().unwrap_or(0));
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
            let player = [&SHREDDER, &SINGER, &BALLADEER][seed as usize % 3];
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

    /// A player that breathes leaves rests between its phrases, ends
    /// inside its bars and in its register, and the bluesman stands on the
    /// pentatonic.
    #[test]
    fn a_breathing_player_rests_between_phrases() {
        let s = score();
        for seed in 0..40 {
            let mut rng = Rng::new(seed);
            let player = if seed % 2 == 0 { &BLUESMAN } else { &BALKAN };
            let shape = Shape::draw(player, &mut rng);
            let line = turn(&s, 0, 8, (59, 88), player, &shape, &mut rng);
            assert!(!line.is_empty());
            assert!(line.iter().all(|n| n.0 + n.1 <= 8 * s.bar() && (59..=88).contains(&n.2)), "seed {seed}");
            let rests = line.windows(2).filter(|w| w[1].0 >= w[0].0 + w[0].1 + E).count();
            if player == &BLUESMAN {
                assert!(rests >= 2, "seed {seed}: {rests} rests in eight bars");
                for n in &line {
                    let d = s.key_at(n.0).standing_degree(n.2).rem_euclid(7);
                    assert!(crate::tune::MINOR_PENTATONIC.contains(&d), "seed {seed}: degree {d} off the pentatonic");
                }
            }
        }
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
