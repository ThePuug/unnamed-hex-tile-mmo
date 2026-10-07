//! The storyteller: one player tells a piece's tune from end to end —
//! sings it, plays it as a riff, calls with a phrase's first half and
//! answers itself with the riff in its second, holds a chord tone
//! between the tellings — and a second player holds tones under its
//! riff. Nothing else carries the tune but an echo. A piece says how
//! the lead tells each run of its walk and gives its idiom: the
//! accents, the registers the tones are held in. The ornaments into a
//! note are its player's (`players::Ornaments`), never written here: a
//! grace written as a note and a grace played as a bend into it are one
//! grace struck twice.

use crate::ladder::Run;
use crate::rng::Rng;
use crate::score::{Note, Score, TICKS_PER_EIGHTH as E};
use crate::theory::{counterpoint, phrase};
use crate::tune::{self, Placed, Tune};

/// How the lead tells a run.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Telling {
    Off,
    /// One chord tone held every other bar.
    Long,
    /// The tune, sung.
    Phrases,
    /// Each phrase's first half sung, the call, and its second played
    /// as the riff, the answer, which is what a blues is.
    Trading,
    /// The tune detached.
    Riff,
    /// The riff over the second's held tones.
    RiffAndLong,
    /// The tune's every foot, sung legato and never thinned: a verse's
    /// patter, a repeated tone struck again rather than tied.
    Patter,
}

/// A piece's storyteller: who plays where, and in what idiom. Accents
/// are added to the piece's velocity by `vel`.
pub struct Teller {
    /// The lead's channel, and the second's.
    pub lead: u8,
    pub second: u8,
    /// A channel that doubles the lead's riff in unison, an octave down
    /// where unison leaves its range, and its accent under the riff's.
    pub double: Option<(u8, i32)>,
    /// A channel echoing the tune, whose bar-line tones the held tones
    /// keep off a perfect interval as they do the lead's.
    pub echo: u8,
    /// The tune's register.
    pub register: (u8, u8),
    /// A sung note's accent.
    pub sung: i32,
    /// A riff note's accent on a strong beat and off one.
    pub riff: (i32, i32),
    /// Where the lead holds between the tellings, and its accent.
    pub long: (u8, u8, i32),
    /// Where the second holds under the riff, and its accent.
    pub under: (u8, u8, i32),
    /// How long a held tone rings.
    pub hold: Hold,
    /// Whether the riff breathes at every half-phrase's end: the last
    /// bar's first tone, then a rest to the bar line. A riff played for
    /// a hundred bars without one is a shout, not a line.
    pub breathes: bool,
    /// The piece's velocity: its one dynamic, `accent` and a jitter.
    pub vel: fn(i32, &mut Rng) -> u8,
    /// How often a gap of two beats or more after a sung phrase holds a
    /// lick, from the second run the lead tells on: a singer's band leaves
    /// most gaps to the riff.
    pub fills: f32,
    /// How likely each run the lead sings is to restate one half-phrase an
    /// octave up, where its instrument reaches.
    pub soars: f32,
    /// How often a sung tone on a beat is pushed an eighth ahead of it, so
    /// a phrase sung again keeps its tune and changes its delivery.
    pub pushes: f32,
}

/// How long a held tone rings.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hold {
    /// Part of its bar, four eighths to one short of the bar, drawn: a
    /// tone that breathes before the bar is out.
    Breathing,
    /// Over its two bars to an eighth before the next.
    Ringing,
}

/// Whether the lead calls at `bar` where it trades through `run`: the
/// run's half-phrases by turns, the first the call; a run of one
/// half-phrase trades by the bar, so every call has its answer.
pub fn calls(run: &Run<Telling>, bar: u32) -> bool {
    let unit = if run.b - run.a <= phrase::BARS / 2 { 1 } else { phrase::BARS / 2 };
    ((bar - run.a) / unit) % 2 == 0
}

/// The lead, run by run over `runs`: where it sings, the tune as a
/// singer takes it — the skeleton held, a passing tone a group — each
/// note held to the next, a hair short of it; where
/// it plays the riff, the tune detached to its feet — a foot held to the
/// next is not heard as a foot, though a tone filling its bar rings —
/// doubled where the piece doubles it, and under that riff the second holds a
/// chord tone every other bar; where it holds, one chord tone every
/// other bar, so the storyteller is heard between the tellings. Where
/// it last was carries run to run, so a held tone leads on from the
/// tune and into it.
pub fn tell(score: &mut Score, teller: &Teller, tune: &Tune, runs: &[Run<Telling>], rng: &mut Rng) {
    let (lo, hi) = teller.register;
    let vel = teller.vel;
    let told = told(score, tune, runs, lo, hi, teller.breathes);
    let mut last: Option<u8> = None;
    let mut second: Option<u8> = None;
    for (r, run) in runs.iter().enumerate() {
        match run.lead {
            Telling::Off => {}
            Telling::Long => {
                // Where the lead tells the tune next, its last held tone
                // leads in: the tone nearest the one the tune will open on,
                // sung or riffed as it will be.
                let goal = runs.get(r + 1).filter(|next| next.lead != Telling::Off && next.lead != Telling::Long).and_then(|next| {
                    let sung = matches!(next.lead, Telling::Phrases | Telling::Trading);
                    tune.bar(score, next.a, lo, hi, sung).first().map(|n| n.2)
                });
                last = held(score, teller, tune, run, teller.long, teller.lead, last, goal, rng);
            }
            Telling::Phrases | Telling::Patter | Telling::Trading | Telling::Riff | Telling::RiffAndLong => {
                let notes = delivered(score, teller, &told[r], r > 0 && runs[..r].iter().any(|p| p.lead == run.lead), rng);
                let notes = &notes;
                for &((start, len, pitch), is_sung) in notes {
                    if is_sung {
                        score.add(Note { start, len: len - E / 8, pitch, vel: vel(teller.sung, rng), channel: teller.lead });
                    } else {
                        let strong = score.strong(start);
                        let accent = if strong { teller.riff.0 } else { teller.riff.1 };
                        // A tone that fills its bar is a hold, not a foot,
                        // and rings in the riff as in the song.
                        let held = if len >= score.bar() { len - E / 2 } else if len >= 3 * E { 2 * E } else { len - E / 2 };
                        score.add(Note { start, len: held, pitch, vel: vel(accent, rng), channel: teller.lead });
                        if let Some((channel, under)) = teller.double {
                            // An octave down where unison leaves its range.
                            let high = score.instrument(channel).high;
                            let pitch = if pitch > high { pitch - 12 } else { pitch };
                            score.add(Note { start, len: held + E / 8, pitch, vel: vel(accent + under, rng), channel });
                        }
                    }
                }
                last = notes.last().map(|(n, _)| n.2);
                if r > 0 && runs[..r].iter().any(|p| p.lead == run.lead) && matches!(run.lead, Telling::Phrases | Telling::Patter) {
                    fills(score, teller, notes, rng);
                }
                if run.lead == Telling::RiffAndLong {
                    second = held(score, teller, tune, run, teller.under, teller.second, second, None, rng);
                }
            }
        }
    }
}

/// A run's notes as delivered this time: where it is told again, some
/// tones on a beat pushed an eighth ahead, the tone before giving up the
/// time; and, now and then, one half-phrase restated an octave up where
/// the lead reaches.
fn delivered(score: &Score, teller: &Teller, notes: &[(Placed, bool)], again: bool, rng: &mut Rng) -> Vec<(Placed, bool)> {
    let mut out = notes.to_vec();
    if again && teller.pushes > 0.0 {
        for j in 1..out.len() {
            let ((start, len, pitch), sung) = out[j];
            let room = start - out[j - 1].0 .0;
            if sung && score.strong(start) && room > E && out[j - 1].0 .1 > E && score.key_at(start - E).contains(pitch) && rng.chance(teller.pushes) {
                out[j] = ((start - E, len + E, pitch), sung);
                out[j - 1].0 .1 -= E;
            }
        }
    }
    let high = score.instrument(teller.lead).high;
    if teller.soars > 0.0 && rng.chance(teller.soars) && !out.is_empty() {
        let bar = score.bar();
        let half = phrase::BARS / 2;
        let first = out[0].0 .0 / bar / half;
        let last = out[out.len() - 1].0 .0 / bar / half;
        let pick = first + rng.below((last - first + 1) as usize) as u32;
        let inside = |n: &(Placed, bool)| n.0 .0 / bar / half == pick;
        if out.iter().filter(|n| inside(n)).all(|n| n.0 .2 + 12 <= high) {
            for n in out.iter_mut().filter(|n| inside(n)) {
                n.0 .2 += 12;
            }
        }
    }
    out
}

/// Licks in the gaps the lead leaves: where two beats or more pass
/// between one sung tone's end and the next's start, now and then a run
/// of sixteenths fills the last of the gap, from a step off the tone the
/// phrase ended on to a step off the tone the next opens on.
fn fills(score: &mut Score, teller: &Teller, notes: &[(Placed, bool)], rng: &mut Rng) {
    let (lo, hi) = teller.register;
    let beat = score.bar() / score.meter.groups.len() as u32;
    for w in notes.windows(2) {
        let ((s0, l0, p0), _) = w[0];
        let ((s1, _, p1), _) = w[1];
        let gap = s1.saturating_sub(s0 + l0);
        if gap < 2 * beat || !rng.chance(teller.fills) {
            continue;
        }
        let beats = (gap / beat).min(rng.range(1, 2) as u32);
        let count = beats * beat / (E / 2);
        let from = s1 - count * (E / 2);
        let key = score.key_at(from);
        let (Some(a), Some(b)) = (key.absolute_degree(key.snap(p0)), key.absolute_degree(key.snap(p1))) else { continue };
        let line = crate::theory::melody::run_between(a + 1, b + if a < b { -1 } else { 1 }, count + 1);
        for (k, d) in line.into_iter().enumerate() {
            let at = from + k as u32 * (E / 2);
            let p = key.pitch(d, 4).clamp(lo, hi);
            // A tone on a strong beat stands on the chord.
            let chord = score.chord_at(at);
            let p = if score.strong(at) && !chord.holds(&key, p) { tune::nearest_chord_tone(&key, chord, p, lo, hi) } else { p };
            if key.contains(p) {
                score.add(Note { start: at, len: E / 2 - 10, pitch: p, vel: (teller.vel)(teller.riff.1, rng), channel: teller.lead });
            }
        }
    }
}

/// The tune each run tells, its notes and whether each is sung: the
/// sung reduction where the lead sings or calls, a tone tied to the one
/// before where it repeats its pitch, the whole line where it riffs or
/// answers; empty where it holds or rests; a breathing riff
/// keeps only its first tone of each half-phrase's last bar. Runs that
/// follow one another with the tune running on are one line, and every
/// line is repaired as it is played, so neither a join nor a breath
/// leaves a leap on after a leap.
fn told(score: &Score, tune: &Tune, runs: &[Run<Telling>], lo: u8, hi: u8, breathes: bool) -> Vec<Vec<(Placed, bool)>> {
    let bar = score.bar();
    let tells = |t: Telling| matches!(t, Telling::Phrases | Telling::Patter | Telling::Trading | Telling::Riff | Telling::RiffAndLong);
    let mut told: Vec<Vec<(Placed, bool)>> = runs
        .iter()
        .map(|run| {
            if !tells(run.lead) {
                return Vec::new();
            }
            let sung = |b: u32| run.lead == Telling::Phrases || (run.lead == Telling::Trading && calls(run, b));
            let mut notes: Vec<(Placed, bool)> = Vec::new();
            if run.lead == Telling::Patter {
                notes.extend(tune.run(score, run.a, run.b, lo, hi, false).into_iter().map(|n| (n, true)));
                return notes;
            }
            notes.extend(tune.run(score, run.a, run.b, lo, hi, true).into_iter().filter(|n| sung(n.0 / bar)).map(|n| (n, true)));
            notes.extend(tune.run(score, run.a, run.b, lo, hi, false).into_iter().filter(|n| !sung(n.0 / bar)).map(|n| (n, false)));
            notes.sort_by_key(|(n, _)| n.0);
            // A sung tone on the pitch of the one it follows straight on is
            // held, not struck again: a singer ties it. Tied before the
            // line is repaired, so the repair hears the line as sung.
            let mut tied: Vec<(Placed, bool)> = Vec::with_capacity(notes.len());
            for (n, sung) in notes {
                match tied.last_mut() {
                    Some((m, true)) if sung && m.2 == n.2 && m.0 + m.1 >= n.0 => m.1 = n.0 + n.1 - m.0,
                    _ => tied.push((n, sung)),
                }
            }
            let mut notes = tied;
            if breathes {
                let half = phrase::BARS / 2;
                let mut kept_in: Option<u32> = None;
                notes.retain(|(n, sung)| {
                    let b = n.0 / bar;
                    if *sung || (b + 1) % half != 0 {
                        return true;
                    }
                    let first = kept_in != Some(b);
                    kept_in = Some(b);
                    first
                });
            }
            notes
        })
        .collect();
    let mut r = 0;
    while r < runs.len() {
        let mut s = r + 1;
        while s < runs.len() && tells(runs[r].lead) && tells(runs[s].lead) && runs[s].a == runs[s - 1].b {
            s += 1;
        }
        if tells(runs[r].lead) {
            let mut line: Vec<Placed> = told[r..s].iter().flatten().map(|(n, _)| *n).collect();
            tune.repair(score, &mut line, lo, hi);
            let mut k = 0;
            for notes in &mut told[r..s] {
                for (n, _) in notes.iter_mut() {
                    *n = line[k];
                    k += 1;
                }
            }
        }
        r = s;
    }
    told
}

/// One chord tone every other bar of `run` on `channel`, from the bar
/// line — a bar in, a breath after the tune, but from the first bar
/// where the run opens the piece — held as the piece's `hold` says, within
/// `range.0..=range.1` at accent `range.2`: a third or a sixth against
/// the tone the lead and the echo each play on the bar line, never a
/// perfect interval, so the lines stay apart — the lead's first where
/// no tone is apart from both; in the run's last two bars the nearest
/// `goal`, where the tune opens next, of all the chord's tones, else
/// within a third of the last
/// where one is and the nearest it, the first from home. Returns the
/// last tone held.
#[allow(clippy::too_many_arguments)]
fn held(score: &mut Score, teller: &Teller, tune: &Tune, run: &Run<Telling>, range: (u8, u8, i32), channel: u8, mut last: Option<u8>, goal: Option<u8>, rng: &mut Rng) -> Option<u8> {
    let (lo, hi) = teller.register;
    let bar = score.bar();
    let mut b = if run.a == 0 { run.a } else { run.a + 1 };
    while b < run.b {
        // The second holds under the riff, never where the tune rests.
        if channel == teller.second && tune.bars[b as usize].is_empty() {
            b += 2;
            continue;
        }
        let chord = tune.chords[b as usize];
        let all = chord.pitches_within(&score.key, range.0, range.1);
        let mut tones = all.clone();
        let on_bar = |ch: u8| -> Vec<u8> { score.notes.iter().filter(|n| n.channel == ch && ch != channel && n.start == b * bar && n.len >= E / 2).map(|n| n.pitch).collect() };
        let (lead, echo) = (on_bar(teller.lead), on_bar(teller.echo));
        let apart = |from: &[u8]| -> Vec<u8> { tones.iter().copied().filter(|p| from.iter().all(|q| !counterpoint::perfect(*q, *p))).collect() };
        // Off both where a tone is, else off the lead, whose line the
        // parallels are judged against.
        let both: Vec<u8> = lead.iter().chain(&echo).copied().collect();
        if let Some(t) = [apart(&both), apart(&lead)].into_iter().find(|t| !t.is_empty()) {
            tones = t;
        }
        let leading_in = b + 2 >= run.b && goal.is_some();
        let toward = if leading_in { goal } else { last };
        let close: Vec<u8> = last.map_or_else(Vec::new, |l| tones.iter().copied().filter(|p| p.abs_diff(l) <= 4).collect());
        let to = toward.unwrap_or_else(|| tune::home_tonic(&score.key, lo, hi));
        // Leading in, the goal before closeness and before keeping off
        // the echo: a tone left a leap from where the tune opens is a
        // leap the tune's first step cannot answer.
        let pool = if leading_in { &all } else if close.is_empty() { &tones } else { &close };
        let pitch = *pool.iter().min_by_key(|c| ((**c as i32 - to as i32).abs(), **c)).unwrap();
        last = Some(pitch);
        let len = match teller.hold {
            Hold::Breathing => E * rng.range(4, score.meter.eighths() as i32 - 1) as u32,
            Hold::Ringing => (2 * bar).min((run.b - b) * bar) - E,
        };
        score.add(Note { start: b * bar, len, pitch, vel: (teller.vel)(range.2, rng), channel });
        b += 2;
    }
    last
}
