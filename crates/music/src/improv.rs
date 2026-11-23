//! A tune improvised about one idea. A play has a motif: a bar of its
//! dance's rhythm with a turn of its own, short enough to know after a
//! few hearings, stated plainly at the start until it is known, and
//! after that seldom said twice alike: a tone passed through, a
//! neighbour taken, a group's long and short changed about, or the
//! motif said broad, a tone a beat — its beat tones and its opening
//! never touched, since those are what it is known by. From there the
//! lead plays its distance from the motif, a half-phrase at a
//! time (`Stance`): it states it, says it higher, answers it in its own
//! rhythm, leaves it for something else, starts it and breaks off, or
//! heads back to it. Nothing says beforehand which or when; the longer
//! the lead has been away the harder the motif pulls, so a play is
//! leavings and returns of its own making, and a listener who knows the
//! motif feels a return coming and hears its next tone before it sounds.
//!
//! Every tone that is not the motif's is drawn, weighted by what the
//! idiom pulls toward — a step before a leap, a leap answered, the chord
//! on a strong beat, the fall to the final, the tone the half-phrase
//! rests on — on the dance's feet, so it sits on the meter. The harmony
//! is the lead's too: each phrase takes a row of the mode's, a question
//! and an answer by turns, a row heard before over a new one, so a play
//! settles on a few rows it keeps coming home to.
//!
//! A play has a feel (`Feel`): its figure, the bar's rhythm the motif is
//! said in and the band strikes under it, and a bar or two more the lead
//! goes to when it leaves.
//!
//! What holds by construction, and a caller may rely on: every bar that
//! sounds is feet that tile it, but where the lead breaks off — the bar
//! held from its last tone, and the bar after silent; the play opens on
//! the motif twice and ends on it, come home; and no leap is followed by
//! a leap the same way, across half-phrases too.

use crate::rng::Rng;
use crate::theory::counterpoint::LEAP;
use crate::theory::groove::Groove;
use crate::theory::melody::{self, Tone};
use crate::theory::schema::Schema;
use crate::theory::Chord;
use crate::tune::{open_tone, Tune};

/// The degrees from home a line moves in: the tune's home has this much
/// room in its register (`tune::home_tonic`).
const LOW: i32 = -2;
const HIGH: i32 = 7;

/// A play's feel: a few bars' feet of its dance. The first is its figure,
/// the motif's rhythm, which the band strikes.
#[derive(Clone, Debug)]
pub struct Feel {
    cells: Vec<Vec<(u32, u32)>>,
}

impl Feel {
    /// A feel on `groove`: a figure that moves off the beat at least
    /// twice, since a figure of held beats is no figure, and one or two
    /// more bars' feet, each unlike the rest where a few draws find one.
    pub fn draw(groove: &Groove, rng: &mut Rng) -> Feel {
        let meter = groove.meter();
        let weak = |feet: &[(u32, u32)]| feet.iter().filter(|(onset, _)| !meter.strong(*onset)).count();
        let mut figure = groove.feet(rng);
        for _ in 0..64 {
            if weak(&figure) >= 2 {
                break;
            }
            figure = groove.feet(rng);
        }
        let mut cells = vec![figure];
        for _ in 0..rng.range(1, 2) {
            let mut cell = groove.feet(rng);
            for _ in 0..8 {
                if !cells.contains(&cell) {
                    break;
                }
                cell = groove.feet(rng);
            }
            cells.push(cell);
        }
        Feel { cells }
    }

    /// The eighths off the beat the figure moves on: where the band
    /// strikes under the lead.
    pub fn strikes(&self, groove: &Groove) -> Vec<u32> {
        let meter = groove.meter();
        self.cells[0].iter().map(|(onset, _)| *onset).filter(|onset| !meter.strong(*onset)).collect()
    }

    /// The figure's feet.
    fn figure(&self) -> &[(u32, u32)] {
        &self.cells[0]
    }

    /// A bar's feet away from the figure: another of the feel's bars, or
    /// now and then feet of the moment.
    fn other(&self, groove: &Groove, rng: &mut Rng) -> Vec<(u32, u32)> {
        let weights: Vec<f32> = (1..=self.cells.len()).map(|i| if i < self.cells.len() { 2.0 } else { 1.0 }).collect();
        match 1 + rng.weighted(&weights) {
            i if i < self.cells.len() => self.cells[i].clone(),
            _ => groove.feet(rng),
        }
    }
}

/// `feet` with the bar's last group one held tone: how a half-phrase
/// comes to rest.
fn resting(groove: &Groove, mut feet: Vec<(u32, u32)>) -> Vec<(u32, u32)> {
    let last = *groove.meter().strong_eighths().last().unwrap();
    feet.retain(|(onset, _)| *onset < last);
    feet.push((last, groove.eighths() - last));
    feet
}

/// Where the lead stands to its motif through a half-phrase.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stance {
    /// The motif, as it is, and a bar that rests.
    States,
    /// The motif a step or a third higher.
    Lifts,
    /// The motif's rhythm with other tones.
    Answers,
    /// Something else, in another rhythm and another place.
    Leaves,
    /// The motif begun and broken off, its last tone held, and a bar of
    /// silence.
    Breaks,
    /// A run toward the motif, ending a step from where it begins.
    Returns,
}

impl Stance {
    const ALL: [Stance; 6] = [Stance::States, Stance::Lifts, Stance::Answers, Stance::Leaves, Stance::Breaks, Stance::Returns];

    pub fn name(self) -> &'static str {
        match self {
            Stance::States => "the tune",
            Stance::Lifts => "the tune, higher",
            Stance::Answers => "an answer",
            Stance::Leaves => "away",
            Stance::Breaks => "broken off",
            Stance::Returns => "coming back",
        }
    }

    /// Whether the motif sounds whole.
    pub fn arrives(self) -> bool {
        matches!(self, Stance::States | Stance::Lifts)
    }

    /// How readily the lead goes from this stance to each, in `ALL`'s
    /// order, before the motif's pull: from the motif to an answer, from
    /// an answer away, from away back by a run, and from a run or a break
    /// to the motif.
    fn onward(self) -> [f32; 6] {
        match self {
            Stance::States => [1.6, 1.2, 2.5, 1.0, 0.4, 0.0],
            Stance::Lifts => [2.0, 0.3, 1.5, 1.5, 0.8, 0.0],
            Stance::Answers => [1.5, 0.6, 0.5, 2.0, 1.0, 1.0],
            Stance::Leaves => [0.2, 0.0, 0.5, 1.2, 1.2, 2.0],
            Stance::Breaks => [3.0, 1.0, 0.0, 0.3, 0.2, 2.0],
            Stance::Returns => [5.0, 2.5, 0.0, 0.0, 1.0, 0.3],
        }
    }
}

/// A play: its tune and harmony, and where the lead stood to its motif
/// through each half-phrase, which is what the band follows.
pub struct Play {
    pub tune: Tune,
    pub stances: Vec<Stance>,
}

/// The stances of a play of `halves` half-phrases: the motif twice to
/// open, and to end; between, each drawn from the one before it and the
/// motif's pull, which grows with every half-phrase away and falls where
/// the motif has just been said twice running. The motif is said higher
/// or broken off only once it has been stated three times: a listener
/// has to know it to hear either.
fn journey(halves: usize, rng: &mut Rng) -> Vec<Stance> {
    let mut stances: Vec<Stance> = Vec::new();
    let (mut stated, mut away) = (0usize, 0usize);
    for h in 0..halves {
        let stance = if h < 2 || h + 1 == halves {
            Stance::States
        } else {
            let prev = stances[h - 1];
            let pull = 1.0 + 0.6 * away as f32;
            let said_twice = stances[h - 2..].iter().all(|s| *s == Stance::States);
            let known = stated >= 3;
            let weights: Vec<f32> = Stance::ALL
                .iter()
                .zip(prev.onward())
                .map(|(to, w)| {
                    w * match to {
                        Stance::States => pull * if !known { 3.0 } else if said_twice { 0.3 } else { 1.0 },
                        Stance::Lifts if known => pull,
                        Stance::Breaks if known => 1.0,
                        Stance::Lifts | Stance::Breaks => 0.0,
                        Stance::Returns => pull,
                        Stance::Answers => 1.0 / (1.0 + 0.3 * away as f32),
                        // Never away just before the end: the play comes home.
                        Stance::Leaves if h + 2 == halves => 0.0,
                        Stance::Leaves => 1.0 / pull,
                    }
                })
                .collect();
            Stance::ALL[rng.weighted(&weights)]
        };
        if stance.arrives() {
            stated += 1;
            away = 0;
        } else {
            away += 1;
        }
        stances.push(stance);
    }
    stances
}

/// Where a line stands: its last tone and the move that reached it.
#[derive(Clone, Copy)]
struct Line {
    last: Option<i32>,
    moved: i32,
}

impl Line {
    fn step(&mut self, degree: i32) {
        self.moved = self.last.map_or(0, |p| degree - p);
        self.last = Some(degree);
    }

    fn through(&mut self, bars: &[Vec<Tone>]) {
        for t in bars.iter().flatten() {
            self.step(t.degree);
        }
    }

    /// Whether moving to `degree` leaps on the same way as the leap
    /// before it.
    fn leaps_on(&self, degree: i32) -> bool {
        let m = self.last.map_or(0, |p| degree - p);
        self.moved.abs() >= LEAP && m.abs() >= LEAP && m.signum() == self.moved.signum()
    }

    /// Whether `bars` follow from here in the register with every leap
    /// answered.
    fn takes(&self, bars: &[Vec<Tone>]) -> bool {
        let mut line = *self;
        bars.iter().flatten().all(|t| {
            let ok = !line.leaps_on(t.degree) && (LOW..=HIGH).contains(&t.degree);
            line.step(t.degree);
            ok
        })
    }
}

/// Whether `degree` is a tone of `chord`.
fn on_chord(chord: Chord, degree: i32) -> bool {
    chord.degrees().iter().any(|d| d.rem_euclid(7) == degree.rem_euclid(7))
}

/// The line's next tone, drawn: a step likeliest, then a third, a leap
/// seldom and never on from a leap the same way, which a step back
/// answers; a chord tone on a `strong` beat; home and its fifth a little
/// over the rest; the weight of the line pulling toward its centre, and
/// down more readily than up; and toward `goal`, the tone that many
/// tones ahead, so the tone before it is a step or a third from it and
/// never the goal itself.
fn next(line: Line, strong: bool, long: bool, chord: Chord, centre: f32, goal: Option<(i32, usize)>, rng: &mut Rng) -> i32 {
    let weights: Vec<f32> = (LOW..=HIGH)
        .map(|d| {
            let mut w = 1.0f32;
            if let Some(p) = line.last {
                let m = d - p;
                w *= match m.abs() {
                    0 if long => 0.15,
                    0 => 0.6,
                    1 => 4.0,
                    2 => 1.6,
                    3 => 0.5,
                    4 => 0.35,
                    _ => 0.0,
                };
                if line.leaps_on(d) {
                    return 0.0;
                }
                if line.moved.abs() >= LEAP && m.signum() == -line.moved.signum() && m.abs() <= 2 {
                    w *= 2.5;
                }
                if m < 0 && p as f32 > centre {
                    w *= 1.4;
                } else if m > 0 && (p as f32) < centre {
                    w *= 1.2;
                }
            }
            if strong {
                w *= if on_chord(chord, d) { 3.0 } else { 0.25 };
            }
            w *= match d.rem_euclid(7) {
                0 => 1.3,
                4 => 1.2,
                _ => 1.0,
            };
            w *= (-(d as f32 - centre).powi(2) / 14.0).exp();
            if let Some((to, ahead)) = goal {
                let far = (d - to).abs();
                w *= match (ahead, far) {
                    (1, 0) => 0.1,
                    (1, 1) => 3.0,
                    (1, 2) => 1.0,
                    (1, _) => 0.0,
                    (n, far) if far > 2 * n as i32 => 0.0,
                    (n, far) => (-(far as f32) / (n as f32 + 0.5)).exp(),
                };
            }
            w
        })
        .collect();
    if weights.iter().sum::<f32>() <= 0.0 {
        // Nothing the pulls allow: the nearest tone toward the goal that
        // does not leap on.
        let to = goal.map_or(line.last.unwrap_or(0), |g| g.0);
        return (LOW..=HIGH).filter(|d| !line.leaps_on(*d)).min_by_key(|d| ((d - to).abs() + line.last.map_or(0, |p| (d - p).abs()), *d)).unwrap_or(to);
    }
    LOW + rng.weighted(&weights) as i32
}

/// Bars on `rhythm`, one bar's feet each, drawn tone by tone from where
/// `line` stands, which they move on: `chords` a bar each, and the last
/// tone of the last bar `goal` where there is one, the tones before it
/// drawn toward it.
fn drawn(groove: &Groove, rhythm: &[Vec<(u32, u32)>], chords: &[Chord], centre: f32, goal: Option<i32>, line: &mut Line, rng: &mut Rng) -> Vec<Vec<Tone>> {
    let meter = groove.meter();
    let total: usize = rhythm.iter().map(|feet| feet.len()).sum();
    let mut n = 0;
    rhythm
        .iter()
        .zip(chords)
        .map(|(feet, chord)| {
            feet.iter()
                .map(|(onset, len)| {
                    let ahead = total - 1 - n;
                    n += 1;
                    let degree = match goal {
                        Some(g) if ahead == 0 => g,
                        _ => next(*line, meter.strong(*onset), *len >= 3, *chord, centre, goal.map(|g| (g, ahead)), rng),
                    };
                    line.step(degree);
                    Tone { onset: *onset, len: *len, degree }
                })
                .collect()
        })
        .collect()
}

/// A motif on `figure`: a bar opening on a tone of the tonic's chord and
/// moving on from it by step, always; and of a few hundred draws the one
/// a listener would most readily tell from another — turning inside a
/// span of a third to a sixth, no tone said for more than a third of it,
/// ending off home within a fourth of it, so the bar asks to go on, with
/// a leap and a turn or two, which are what the ear keeps.
fn motif(groove: &Groove, figure: &[(u32, u32)], rng: &mut Rng) -> Vec<Tone> {
    let tonic = Chord::triad(0);
    let mut best: Option<(i32, Vec<Tone>)> = None;
    for _ in 0..200 {
        let first = [0, 2, 4][rng.weighted(&[3.0, 2.0, 2.0])];
        let mut line = Line { last: Some(first), moved: 0 };
        let mut bar = vec![Tone { onset: figure[0].0, len: figure[0].1, degree: first }];
        bar.extend(drawn(groove, &[figure[1..].to_vec()], &[tonic], first as f32 + 1.0, None, &mut line, rng).remove(0));
        // By step from its first tone, so the motif joins whatever came
        // before it without leaping on.
        if (bar[1].degree - first).abs() >= LEAP {
            bar[1].degree = first + (bar[1].degree - first).signum();
        }
        let degrees: Vec<i32> = bar.iter().map(|t| t.degree).collect();
        let moves: Vec<i32> = degrees.windows(2).map(|w| w[1] - w[0]).filter(|m| *m != 0).collect();
        let turns = moves.windows(2).filter(|w| w[0].signum() != w[1].signum()).count() as i32;
        let span = degrees.iter().max().unwrap() - degrees.iter().min().unwrap();
        let most = degrees.iter().map(|d| degrees.iter().filter(|o| *o == d).count()).max().unwrap();
        let last = *degrees.last().unwrap();
        let unanswered = crate::theory::counterpoint::unrecovered_leap(&degrees).is_some();
        let fits = !unanswered && (2..=5).contains(&span) && most * 3 <= degrees.len().max(6) && last != 0 && last.abs() <= 3 && turns >= 1;
        let leaps = moves.iter().filter(|m| m.abs() >= LEAP).count() as i32;
        let told = if unanswered { -100 } else if fits { 10 } else { 0 } + turns.min(2) + leaps.min(1);
        if best.as_ref().is_none_or(|(b, _)| told > *b) {
            best = Some((told, bar));
        }
        if told >= 12 {
            break;
        }
    }
    best.unwrap().1
}

/// The motif said a little differently, as a player never says a thing
/// twice alike: a held tone giving its last eighth to a step toward the
/// next tone; a tone off the beat on its neighbour; a group's long and
/// short the other way about; or broad, its beat tones alone, each held
/// through its group. Its first tone, its beat tones and its last stand
/// in every one, and it leaves its first tone by step as the motif does.
/// The motif as it is where the change would leave a leap unanswered.
fn varied(groove: &Groove, motif: &[Tone], rng: &mut Rng) -> Vec<Tone> {
    let meter = groove.meter();
    let strong = meter.strong_eighths();
    let ends = |g: usize| strong.get(g + 1).copied().unwrap_or(groove.eighths());
    let mut said = motif.to_vec();
    let last = said.len() - 1;
    match rng.weighted(&[2.0, 2.0, 1.5, 1.5]) {
        0 => {
            // A step toward the next tone on a held tone's last eighth.
            let held: Vec<usize> = (0..last).filter(|i| said[*i].len >= 2 && said[*i + 1].degree != said[*i].degree).collect();
            if let Some(i) = (!held.is_empty()).then(|| held[rng.below(held.len())]) {
                let toward = (said[i + 1].degree - said[i].degree).signum();
                said[i].len -= 1;
                let passing = Tone { onset: said[i].onset + said[i].len, len: 1, degree: said[i].degree + toward };
                said.insert(i + 1, passing);
            }
        }
        1 => {
            // A tone off the beat on the neighbour that keeps it a step
            // from the tones either side.
            let weak: Vec<usize> = (1..last).filter(|i| !meter.strong(said[*i].onset)).collect();
            if let Some(i) = (!weak.is_empty()).then(|| weak[rng.below(weak.len())]) {
                let fits = |d: i32| (d - said[i - 1].degree).abs() <= 2 && (d - said[i + 1].degree).abs() <= 2;
                if let Some(d) = [said[i].degree + 1, said[i].degree - 1].into_iter().find(|d| fits(*d)) {
                    said[i].degree = d;
                }
            }
        }
        2 => {
            // A group of two tones, its long and its short changed about.
            let pairs: Vec<usize> = (0..strong.len())
                .filter_map(|g| {
                    let inside: Vec<usize> = (0..said.len()).filter(|i| said[*i].onset >= strong[g] && said[*i].onset < ends(g)).collect();
                    (inside.len() == 2 && said[inside[0]].len != said[inside[1]].len).then_some(inside[0])
                })
                .collect();
            if let Some(i) = (!pairs.is_empty()).then(|| pairs[rng.below(pairs.len())]) {
                let (a, b) = (said[i].len, said[i + 1].len);
                said[i].len = b;
                said[i + 1].onset = said[i].onset + b;
                said[i + 1].len = a;
            }
        }
        _ => {
            said = strong.iter().enumerate().filter_map(|(g, s)| motif.iter().find(|t| t.onset == *s).map(|t| Tone { onset: *s, len: ends(g) - s, degree: t.degree })).collect();
        }
    }
    let degrees: Vec<i32> = said.iter().map(|t| t.degree).collect();
    let sound = degrees.len() >= 2 && (degrees[1] - degrees[0]).abs() < LEAP && degrees.iter().all(|d| (LOW..=HIGH).contains(d)) && crate::theory::counterpoint::unrecovered_leap(&degrees).is_none();
    if sound {
        said
    } else {
        motif.to_vec()
    }
}

/// A tune of `phrases` four-bar phrases improvised about a motif in
/// `feel`, the harmony it takes with it, and the lead's stance through
/// each half-phrase. Each phrase's row is an `open` one and a `closed`
/// one by turns: one heard before over a new one, more so as the play
/// goes on, till it has been taken twice running and wears; an answer's
/// leaning to one that opens as its question did. Chords are of `size`
/// tones. Panics where either kind has no row.
pub fn improvise(groove: &Groove, feel: &Feel, open: &[&Schema], closed: &[&Schema], size: u8, phrases: usize, rng: &mut Rng) -> Play {
    let meter = groove.meter();
    let figure = feel.figure().to_vec();
    let motif = motif(groove, &figure, rng);
    let stances = journey(2 * phrases, rng);
    // The most the motif goes up where it is said higher: a third where
    // it has the room, else a step.
    let lift = [2, 1].into_iter().find(|by| motif.iter().all(|t| t.degree + by < HIGH)).unwrap_or(0);
    // Where the line's weight sits climbs to a crest past the middle and
    // comes home, the crest's place and height the play's own.
    let (crest_at, crest) = (0.45 + 0.25 * rng.f32(), 2.0 + 2.0 * rng.f32());
    // The bar the motif's first statement rested on, and its first
    // answer: what the lead comes back to after either.
    let mut rest: Option<Vec<Tone>> = None;
    let mut answer: Option<Vec<Vec<Tone>>> = None;
    let mut line = Line { last: None, moved: 0 };
    let mut taken: [Vec<usize>; 2] = [Vec::new(), Vec::new()];
    let mut chords = Vec::new();
    let mut bars: Vec<Vec<Tone>> = Vec::new();
    for p in 0..phrases {
        let kind = p % 2;
        let pool = if kind == 0 { open } else { closed };
        let x = if phrases > 1 { p as f32 / (phrases - 1) as f32 } else { 0.0 };
        let rise = if x < crest_at { x / crest_at } else { (1.0 - x) / (1.0 - crest_at) };
        let centre = 1.0 + crest * rise;
        let asked = (kind == 1).then(|| open[*taken[0].last().unwrap()]);
        let weights: Vec<f32> = pool
            .iter()
            .enumerate()
            .map(|(i, row)| {
                let times = taken[kind].iter().filter(|t| **t == i).count();
                let known = if times > 0 { 1.0 + 2.0 * times as f32 } else { 1.2 - x };
                let worn = if taken[kind].len() >= 2 && taken[kind].iter().rev().take(2).all(|t| *t == i) { 0.3 } else { 1.0 };
                let echoes = if asked.is_some_and(|o| o.roots[..2] == row.roots[..2]) { 3.0 } else { 1.0 };
                known * worn * echoes
            })
            .collect();
        let row = rng.weighted(&weights);
        taken[kind].push(row);
        let schema = pool[row];
        chords.extend((0..4).map(|k| schema.chord(k, size)));
        for half in 0..2 {
            let h = 2 * p + half;
            let stance = stances[h];
            let after = stances.get(h + 1).copied();
            let under = [schema.chord(2 * half, size), schema.chord(2 * half + 1, size)];
            // The tone the half-phrase rests on: a tone of its chord near
            // home, and home where an answer closes.
            let goal = match half {
                0 => open_tone(under[1]),
                _ if schema.closed => 0,
                _ => open_tone(under[1]),
            };
            // Plain twice to open and to end; after that as often varied
            // as not, the motif said higher always plain.
            let said = match stance {
                Stance::States if h >= 2 && h + 1 < stances.len() && rng.chance(0.55) => varied(groove, &motif, rng),
                _ => melody::sequence(&motif, if stance == Stance::Lifts { lift } else { 0 }),
            };
            let mut pair: Vec<Vec<Tone>> = match stance {
                Stance::States | Stance::Lifts => {
                    let mut on = line;
                    on.through(std::slice::from_ref(&said));
                    // The bar it rested on the first time, brought to this
                    // half-phrase's rest, where it follows; else one drawn.
                    let kept = rest.as_ref().filter(|_| stance == Stance::States && rng.chance(0.65)).map(|r| melody::ending(r, goal)).filter(|r| on.takes(std::slice::from_ref(r)));
                    let second = kept.unwrap_or_else(|| drawn(groove, &[resting(groove, feel.other(groove, rng))], &under[1..], centre, Some(goal), &mut on, rng).remove(0));
                    if stance == Stance::States && rest.is_none() {
                        rest = Some(second.clone());
                    }
                    vec![said, second]
                }
                Stance::Answers => {
                    let kept = answer.as_ref().filter(|_| rng.chance(0.6)).map(|a| vec![a[0].clone(), melody::ending(&a[1], goal)]).filter(|a| line.takes(a));
                    kept.unwrap_or_else(|| {
                        let mut on = line;
                        let mut pair = drawn(groove, std::slice::from_ref(&figure), &under[..1], centre, None, &mut on, rng);
                        pair.extend(drawn(groove, &[resting(groove, feel.other(groove, rng))], &under[1..], centre, Some(goal), &mut on, rng));
                        if answer.is_none() {
                            answer = Some(pair.clone());
                        }
                        pair
                    })
                }
                Stance::Leaves => {
                    // Another rhythm, and another place: over the line's
                    // weight where it sits low, under it where it sits high.
                    let elsewhere = if centre < 2.5 { centre + 2.5 } else { centre - 2.0 };
                    let rhythm = [feel.other(groove, rng), resting(groove, feel.other(groove, rng))];
                    drawn(groove, &rhythm, &under, elsewhere, Some(goal), &mut line.clone(), rng)
                }
                Stance::Breaks => {
                    // Through its second group, or its first, where that
                    // leaves enough unsaid to be missed and stops on a
                    // tone it stepped to, so what follows never leaps on
                    // from it; else its first tone alone.
                    let strong = meter.strong_eighths();
                    let cut = |e: u32| motif.iter().copied().filter(|t| t.onset < e).collect::<Vec<Tone>>();
                    let stops = |begun: &[Tone]| motif.len() - begun.len() >= 2 && begun.windows(2).last().is_none_or(|w| (w[1].degree - w[0].degree).abs() < LEAP);
                    let mut begun = [2, 1].into_iter().map(|g| cut(strong.get(g).copied().unwrap_or(groove.eighths()))).find(|b| stops(b)).unwrap_or_else(|| motif[..1].to_vec());
                    let held = begun.last_mut().unwrap();
                    held.len = groove.eighths() - held.onset;
                    vec![begun, Vec::new()]
                }
                Stance::Returns => {
                    // A run in the figure, ending a step under where the
                    // motif will begin, or over it where there is no room
                    // under; where the motif does not follow, on its rest.
                    let begins = motif[0].degree + if after == Some(Stance::Lifts) { lift } else { 0 };
                    let to = if begins > LOW { begins - 1 } else { begins + 1 };
                    let goal = if after.is_some_and(|a| a.arrives()) { to } else { goal };
                    drawn(groove, &[figure.clone(), feel.other(groove, rng)], &under, centre, Some(goal), &mut line.clone(), rng)
                }
            };
            line.through(&pair);
            // A half-phrase's last tone gives up an eighth or two to a
            // breath, since a line that never rests is an instrument and
            // not a singer; a run into the motif runs on.
            if stance != Stance::Returns {
                if let Some(t) = pair[1].last_mut() {
                    t.len -= (t.len - 1).min(2);
                }
            }
            bars.extend(pair);
        }
    }
    let shifts = vec![0; bars.len()];
    Play { tune: Tune { chords, bars, shifts }, stances }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theory::counterpoint::unrecovered_leap;
    use crate::theory::groove::BALKAN;
    use crate::theory::schema::SCHEMATA;

    const PHRASES: usize = 12;

    fn rows() -> (Vec<&'static Schema>, Vec<&'static Schema>) {
        (SCHEMATA.iter().filter(|s| !s.closed).collect(), SCHEMATA.iter().filter(|s| s.closed).collect())
    }

    fn play(groove: &Groove, seed: u64) -> Play {
        let (open, closed) = rows();
        let mut rng = Rng::new(seed);
        let feel = Feel::draw(groove, &mut rng);
        improvise(groove, &feel, &open, &closed, 3, PHRASES, &mut rng)
    }

    fn said(bar: &[Tone]) -> Vec<(u32, i32)> {
        bar.iter().map(|t| (t.onset, t.degree)).collect()
    }

    /// The tones of `bar` on the beats of `groove`.
    fn beats(groove: &Groove, bar: &[Tone]) -> Vec<(u32, i32)> {
        let meter = groove.meter();
        said(bar).into_iter().filter(|(onset, _)| meter.strong(*onset)).collect()
    }

    /// Every play on every dance tiles the bars that sound but for a
    /// breath, is silent only after the lead breaks off, answers every
    /// leap, keeps its register and ends at home.
    #[test]
    fn a_line_holds_to_what_it_owes() {
        for groove in &BALKAN {
            for seed in 0..40 {
                let p = play(groove, seed);
                assert_eq!(p.tune.bars.len(), 4 * PHRASES);
                assert_eq!(p.stances.len(), 2 * PHRASES);
                for (b, bar) in p.tune.bars.iter().enumerate() {
                    let stance = p.stances[b / 2];
                    if bar.is_empty() {
                        assert!(stance == Stance::Breaks && b % 2 == 1, "{} seed {seed} bar {b}: silent in {stance:?}", groove.name);
                        continue;
                    }
                    let covered: u32 = bar.iter().map(|t| t.len).sum();
                    assert!(covered <= groove.eighths() && covered + 2 >= groove.eighths(), "{} seed {seed} bar {b}: a hole", groove.name);
                    assert_eq!(bar[0].onset, 0, "{} seed {seed} bar {b}", groove.name);
                }
                let degrees: Vec<i32> = p.tune.bars.iter().flatten().map(|t| t.degree).collect();
                assert_eq!(unrecovered_leap(&degrees), None, "{} seed {seed}: {degrees:?}", groove.name);
                assert!(degrees.iter().all(|d| (LOW..=HIGH).contains(d)), "{} seed {seed}", groove.name);
                assert_eq!(*degrees.last().unwrap(), 0, "{} seed {seed} does not come home", groove.name);
            }
        }
    }

    /// A play is about its motif: it opens on it twice and ends on it,
    /// as it is; says it whole in a third of its half-phrases or more,
    /// its beat tones the same every time and most plays saying it more
    /// than one way; never stays away from it long; and no two plays of
    /// many have the same one.
    #[test]
    fn a_play_is_about_its_motif() {
        let groove = &BALKAN[0];
        let mut motifs = Vec::new();
        let mut ways = 0;
        for seed in 0..60 {
            let p = play(groove, seed);
            let motif = said(&p.tune.bars[0]);
            assert_eq!(&p.stances[..2], &[Stance::States, Stance::States], "seed {seed}");
            assert_eq!(*p.stances.last().unwrap(), Stance::States, "seed {seed}");
            let mut told: Vec<Vec<(u32, i32)>> = Vec::new();
            for (h, stance) in p.stances.iter().enumerate() {
                if *stance == Stance::States {
                    let bar = &p.tune.bars[2 * h];
                    assert_eq!(beats(groove, bar), beats(groove, &p.tune.bars[0]), "seed {seed}: half {h} states another");
                    if h < 2 || h + 1 == p.stances.len() {
                        assert_eq!(said(bar), motif, "seed {seed}: half {h} is not the motif as it is");
                    }
                    told.push(said(bar));
                }
            }
            told.sort();
            told.dedup();
            if told.len() > 1 {
                ways += 1;
            }
            let whole = p.stances.iter().filter(|s| s.arrives()).count();
            assert!(whole * 3 >= p.stances.len(), "seed {seed}: the motif in {whole} of {} halves", p.stances.len());
            let longest = p.stances.split(|s| s.arrives()).map(|away| away.len()).max().unwrap();
            assert!(longest <= 6, "seed {seed}: {longest} half-phrases away");
            motifs.push(motif);
        }
        assert!(ways >= 40, "{ways} of 60 plays say the motif more than one way");
        assert_eq!(play(groove, 5).tune.bars, play(groove, 5).tune.bars);
        let all = motifs.len();
        motifs.sort();
        motifs.dedup();
        assert!(motifs.len() * 10 >= all * 9, "{} motifs in {all} plays", motifs.len());
    }

    /// A play has a story: most leave the motif, and where a run leads to
    /// the motif it ends a step from where the motif begins.
    #[test]
    fn a_play_leaves_and_returns() {
        let groove = &BALKAN[0];
        let (mut left, mut led) = (0, 0);
        for seed in 0..60 {
            let p = play(groove, seed);
            if p.stances.contains(&Stance::Leaves) || p.stances.contains(&Stance::Answers) {
                left += 1;
            }
            for h in 0..p.stances.len() - 1 {
                if p.stances[h] == Stance::Returns && p.stances[h + 1].arrives() {
                    led += 1;
                    let from = p.tune.bars[2 * h + 1].last().unwrap().degree;
                    let to = p.tune.bars[2 * h + 2][0].degree;
                    assert_eq!((to - from).abs(), 1, "seed {seed} half {h}: a run ending at {from} into {to}");
                }
            }
        }
        assert!(left >= 55, "{left} of 60 plays leave the motif");
        assert!(led >= 30, "{led} runs into the motif in 60 plays");
    }

    /// A play's harmony is its own and comes home: plays take different
    /// rows, most take more than one question, most come back to one, and
    /// every phrase is a question and then its answer.
    #[test]
    fn the_harmony_goes_with_the_lead() {
        let groove = &BALKAN[0];
        let (open, closed) = rows();
        let roots = |t: &Tune, p: usize| t.chords[p * 4..p * 4 + 4].iter().map(|c| c.root).collect::<Vec<i32>>();
        let (mut plays, mut moving) = (Vec::new(), 0);
        for seed in 0..40 {
            let t = play(groove, seed).tune;
            for p in 0..PHRASES {
                let row = roots(&t, p);
                let pool = if p % 2 == 0 { &open } else { &closed };
                assert!(pool.iter().any(|s| s.roots.to_vec() == row), "seed {seed} phrase {p}: {row:?}");
            }
            let mut questions: Vec<Vec<i32>> = (0..PHRASES / 2).map(|q| roots(&t, q * 2)).collect();
            questions.sort();
            questions.dedup();
            if questions.len() > 1 {
                moving += 1;
            }
            plays.push((0..PHRASES).map(|p| roots(&t, p)).collect::<Vec<_>>());
        }
        assert!(moving >= 20, "{moving} of 40 plays take more than one question");
        let homes = plays.iter().filter(|rows: &&Vec<Vec<i32>>| (0..PHRASES / 2).map(|q| (0..PHRASES / 2).filter(|o| rows[o * 2] == rows[q * 2]).count()).max().unwrap() >= 2).count();
        assert!(homes >= 30, "{homes} of 40 plays come back to a question");
        plays.sort();
        plays.dedup();
        assert!(plays.len() >= 30, "{} harmonies in 40 plays", plays.len());
    }

    /// A feel's figure moves off the beat, and its strikes are the
    /// figure's, never on a strong beat.
    #[test]
    fn a_figure_moves_off_the_beat() {
        for groove in &BALKAN {
            let meter = groove.meter();
            for seed in 0..40 {
                let feel = Feel::draw(groove, &mut Rng::new(seed));
                let strikes = feel.strikes(groove);
                assert!(strikes.len() >= 2, "{} seed {seed}: {strikes:?}", groove.name);
                assert!(strikes.iter().all(|e| !meter.strong(*e) && *e < groove.eighths()));
            }
        }
    }
}
