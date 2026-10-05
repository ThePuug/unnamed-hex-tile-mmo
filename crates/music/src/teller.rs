//! The storyteller: one player tells a piece's tune from end to end —
//! sings it, plays it as a riff, calls with a phrase's first half and
//! answers itself with the riff in its second, holds a chord tone
//! between the tellings — and a second player holds tones under its
//! riff. Nothing else carries the tune but an echo. A piece says how
//! the lead tells each run of its walk and gives its idiom: the
//! ornament into a note, the accents, the registers the tones are held
//! in.

use crate::ladder::Run;
use crate::rng::Rng;
use crate::score::{Note, Score, TICKS_PER_EIGHTH as E};
use crate::theory::{counterpoint, phrase, Key};
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
}

/// A piece's storyteller: who plays where, and in what idiom. Accents
/// are added to the piece's velocity by `vel`.
pub struct Teller {
    /// The lead's channel, and the second's.
    pub lead: u8,
    pub second: u8,
    /// A channel that doubles the lead's riff in unison, and its accent
    /// under the riff's.
    pub double: Option<(u8, i32)>,
    /// A channel echoing the tune, whose bar-line tones the held tones
    /// keep off a perfect interval as they do the lead's.
    pub echo: u8,
    /// The tune's register.
    pub register: (u8, u8),
    /// The piece's ornament: the grace into `pitch`, None where it
    /// would leave the register's top.
    pub grace: fn(&Key, u8, u8) -> Option<u8>,
    /// A sung note's accent, and its grace's.
    pub sung: (i32, i32),
    /// A riff note's accent on a strong beat and off one, and its
    /// grace's.
    pub riff: (i32, i32, i32),
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

/// The ornament a Balkan line takes into a note: the degree above it;
/// None above the register.
pub fn above(key: &Key, pitch: u8, hi: u8) -> Option<u8> {
    let grace = key.pitch(key.absolute_degree(pitch).unwrap() + 1, 4);
    (grace <= hi).then_some(grace)
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
/// note held to the next, a hair short of it, a grace into the next now
/// and then, stealing its length from this one where they touch; where
/// it plays the riff, the tune detached to its feet — a foot held to the
/// next is not heard as a foot, though a tone filling its bar rings —
/// doubled where the piece doubles it, a
/// grace into some strong beats, and under that riff the second holds a
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
                // Where the lead sings next, its last held tone leads in:
                // the tone nearest the one the tune will open on.
                let goal = runs.get(r + 1).filter(|next| matches!(next.lead, Telling::Phrases | Telling::Trading)).map(|next| tune.bar(score, next.a, lo, hi, true)[0].2);
                last = held(score, teller, tune, run, runs, teller.long, teller.lead, last, goal, rng);
            }
            Telling::Phrases | Telling::Trading | Telling::Riff | Telling::RiffAndLong => {
                let notes = &told[r];
                for j in 0..notes.len() {
                    let ((start, len, pitch), is_sung) = notes[j];
                    if is_sung {
                        let mut len = len - E / 8;
                        // A grace on the tone the line holds is that tone,
                        // carried into the next, never struck again.
                        let grace = notes.get(j + 1).filter(|(n, _)| n.2 != pitch).and_then(|(n, _)| (teller.grace)(&grace_key(score, n.0, n.2), n.2, hi).map(|g| (n.0, g))).filter(|_| rng.chance(0.35)).filter(|(_, g)| *g != pitch);
                        if let Some((next_start, _)) = grace {
                            len = len.min(next_start - E / 4 - start);
                        }
                        score.add(Note { start, len, pitch, vel: vel(teller.sung.0, rng), channel: teller.lead });
                        if let Some((next_start, grace)) = grace {
                            score.add(Note { start: next_start - E / 4, len: E / 4, pitch: grace, vel: vel(teller.sung.1, rng), channel: teller.lead });
                        }
                    } else {
                        let strong = score.strong(start);
                        let accent = if strong { teller.riff.0 } else { teller.riff.1 };
                        // A tone that fills its bar is a hold, not a foot,
                        // and rings in the riff as in the song.
                        let held = if len >= score.bar() { len - E / 2 } else if len >= 3 * E { 2 * E } else { len - E / 2 };
                        if strong && j > 0 && rng.chance(0.4) {
                            if let Some(grace) = (teller.grace)(&grace_key(score, start, pitch), pitch, hi).filter(|g| *g != notes[j - 1].0.2) {
                                score.add(Note { start: start - E / 4, len: E / 4, pitch: grace, vel: vel(teller.riff.2, rng), channel: teller.lead });
                            }
                        }
                        score.add(Note { start, len: held, pitch, vel: vel(accent, rng), channel: teller.lead });
                        if let Some((channel, under)) = teller.double {
                            score.add(Note { start, len: held + E / 8, pitch, vel: vel(accent + under, rng), channel });
                        }
                    }
                }
                last = notes.last().map(|(n, _)| n.2);
                if run.lead == Telling::RiffAndLong {
                    second = held(score, teller, tune, run, runs, teller.under, teller.second, second, None, rng);
                }
            }
        }
    }
}

/// The key a grace into `pitch` at `tick` is drawn in: the bar's it
/// sounds in, a sixteenth before, so a grace from under a borrowed
/// chord is that chord's — or `pitch`'s own where the grace's bar
/// leaves `pitch` out.
fn grace_key(score: &Score, tick: u32, pitch: u8) -> Key {
    let under = score.key_at(tick.saturating_sub(E / 4));
    if under.contains(pitch) { under } else { score.key_at(tick) }
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
    let tells = |t: Telling| matches!(t, Telling::Phrases | Telling::Trading | Telling::Riff | Telling::RiffAndLong);
    let mut told: Vec<Vec<(Placed, bool)>> = runs
        .iter()
        .map(|run| {
            if !tells(run.lead) {
                return Vec::new();
            }
            let sung = |b: u32| run.lead == Telling::Phrases || (run.lead == Telling::Trading && calls(run, b));
            let mut notes: Vec<(Placed, bool)> = Vec::new();
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
/// where the run opens or closes the loop, so the tones fall every
/// other bar straight through the seam and neither side of it is a bar
/// thinner than the other — held as the piece's `hold` says, within
/// `range.0..=range.1` at accent `range.2`: a third or a sixth against
/// the tone the lead and the echo each play on the bar line, never a
/// perfect interval, so the lines stay apart — the lead's first where
/// no tone is apart from both; in the run's last two bars the nearest
/// `goal`, where the tune opens next, of all the chord's tones, else
/// within a third of the last
/// where one is and the nearest it, the first from home. Returns the
/// last tone held.
#[allow(clippy::too_many_arguments)]
fn held(score: &mut Score, teller: &Teller, tune: &Tune, run: &Run<Telling>, runs: &[Run<Telling>], range: (u8, u8, i32), channel: u8, mut last: Option<u8>, goal: Option<u8>, rng: &mut Rng) -> Option<u8> {
    let (lo, hi) = teller.register;
    let bar = score.bar();
    let bars = runs.last().map_or(0, |r| r.b);
    let mut b = if run.a == 0 || run.b == bars { run.a } else { run.a + 1 };
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
