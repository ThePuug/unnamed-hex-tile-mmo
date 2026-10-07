//! The players: what a written score becomes when people play it. The
//! score is exact — every note on its tick, at its velocity, in tune —
//! and a band that plays it so sounds like machines. Each player here
//! has habits: a lean ahead of or behind the beat, a sense of time that
//! drifts, a hand that is never twice the same, a tuning a few cents
//! off. Over those, each player's intensity rises and falls on its own
//! slow breath; a held tone swells as the player enters and carries
//! through every tone joined to it; a sung or bowed tone grows a
//! vibrato once it settles. A lead ornaments as its player does
//! (`players::Ornaments`): graces, bends and scoops into a tone, slides,
//! a vibrato of the bend or a shake, a phrase's end let go a different
//! way each time. How far each habit
//! goes is the player's (`players`); this is how a habit plays.
//!
//! Every draw is keyed on the note — its channel, pitch and tick — so a
//! score is played the same every time it is rendered. The checks judge
//! the written score; this is what is heard.

use audio::rng::Rng;

use crate::render::volume;
use crate::players::Ornaments;
use crate::score::{Role, Score, TICKS_PER_EIGHTH};

/// A message to the synthesizer at `at` seconds. At one instant a
/// controller goes before a note-off, and a note-off before a note-on,
/// so a retriggered pitch restarts and a note starts on its settings.
#[derive(Clone, Copy, Debug)]
pub struct Played {
    pub at: f64,
    pub msg: Msg,
}

#[derive(Clone, Copy, Debug)]
pub enum Msg {
    Control { channel: u8, number: u8, value: u8 },
    /// 14-bit pitch bend, 8192 in tune, over General MIDI's two semitones.
    Bend { channel: u8, value: u16 },
    Off { channel: u8, pitch: u8 },
    /// A note struck; a `legato` one is entered with its string already
    /// ringing, slurred from the note before.
    On { channel: u8, pitch: u8, vel: u8, legato: bool },
}

impl Msg {
    pub fn order(&self) -> u8 {
        match self {
            Msg::Control { .. } | Msg::Bend { .. } => 0,
            Msg::Off { .. } => 1,
            Msg::On { .. } => 2,
        }
    }
}

/// How long a tone is before its stroke's slip starts to settle: a
/// tone this long or shorter slips by the full touch, a longer one by
/// this share of its length.
const SETTLE_S: f64 = 0.5;

/// How far a player's slow breath takes its velocity, steps either way.
const BREATH_TOUCH: f32 = 5.0;

/// How far a player's slow breath takes its level down, dB.
const BREATH_DB: f32 = 2.0;

/// How far under its full level a player enters on a held tone, dB,
/// and how long it takes to swell to full; after, it eases back by a
/// share of the dip, over `EASE_S`. A tone struck while the player's
/// last still sounds, or within `JOIN_S` of its release, is carried on
/// the same bow or breath: the swell goes on from the entry, and a
/// chord change is not a dip.
const SWELL_DB: f32 = 3.0;
const SWELL_S: f64 = 1.2;
const EASE: f32 = 0.4;
const EASE_S: f64 = 4.0;
const JOIN_S: f64 = 0.1;

/// How often the expression pedal moves, seconds: fine enough that a
/// swell is a curve, not steps.
const PEDAL_S: f64 = 0.04;

/// How far a player's tuning may stand off, as a share of the spread it
/// is drawn from.
const TUNING_BOUND: f32 = 2.0;

/// The shortest a played note is, seconds.
const SHORTEST_S: f64 = 0.03;

/// A draw near zero with unit spread, bounded at ±2.5: the sum of four
/// uniforms.
fn slip(rng: &mut Rng) -> f32 {
    let s: f32 = (0..4).map(|_| rng.f32()).sum();
    ((s - 2.0) * 3f32.sqrt()).clamp(-2.5, 2.5)
}

/// A player's slow breath: two sines, one 24 s and one 9 s long, in -1..1.
struct Breath {
    phase: [f64; 2],
}

impl Breath {
    const PERIODS_S: [f64; 2] = [24.0, 9.0];

    fn new(rng: &mut Rng) -> Self {
        Breath { phase: [rng.f32() as f64 * std::f64::consts::TAU, rng.f32() as f64 * std::f64::consts::TAU] }
    }

    fn at(&self, t: f64) -> f32 {
        let w = |k: usize| (std::f64::consts::TAU * t / Self::PERIODS_S[k] + self.phase[k]).sin();
        (0.6 * w(0) + 0.4 * w(1)) as f32
    }
}

/// A held tone's level against full, dB, `d` seconds after it was struck.
fn swell(d: f64) -> f32 {
    if d < SWELL_S {
        let x = (d / SWELL_S) as f32;
        -SWELL_DB * (1.0 - x * x * (3.0 - 2.0 * x))
    } else {
        -SWELL_DB * EASE * (1.0 - (-(d - SWELL_S) / EASE_S).exp()) as f32
    }
}

/// How much of a phrase's last tone a shake takes, its end: a player has
/// the breath for a lip trill there, not for one through a long tone.
const SHAKE_S: f64 = 0.6;

/// The pitch-bend value for `cents` on a channel bent over `range`
/// semitones either way.
fn bend_on(cents: f32, range: f32) -> u16 {
    (8192.0 + cents * 8192.0 / (100.0 * range)).round().clamp(0.0, 16383.0) as u16
}

/// The score as played, in time order.
pub fn perform(score: &Score) -> Vec<Played> {
    let salt = Rng::new(score.end() as u64 ^ (score.eighth_bpm.to_bits() as u64) << 32);
    let mut out = Vec::new();

    for inst in &score.instruments {
        let ch = inst.channel;
        let f = score.player(ch);
        let mut player = salt.fork(ch as u64 + 1);
        let lean = f.lean.0 + (f.lean.1 - f.lean.0) * player.f32();
        let breath = Breath::new(&mut player);
        let time = Breath::new(&mut player);
        let percussion = inst.role == Role::Percussion;
        let tuning = if percussion { 0.0 } else { (slip(&mut player) * f.tuning).clamp(-TUNING_BOUND * f.tuning, TUNING_BOUND * f.tuning) };
        let guitar = f.ornaments;
        let swells = |len_s: f64| f.swell.is_some_and(|shortest| len_s >= shortest);

        if let Some(g) = &guitar {
            // The bend range by registered parameter 0, then the parameter
            // closed.
            for (number, value) in [(101, 0), (100, 0), (6, g.range as u8), (38, 0), (101, 127), (100, 127)] {
                out.push(Played { at: 0.0, msg: Msg::Control { channel: ch, number, value } });
            }
        }
        let range = guitar.map_or(2.0, |g| g.range);
        if !percussion {
            out.push(Played { at: 0.0, msg: Msg::Bend { channel: ch, value: bend_on(tuning, range) } });
        }
        // How soon each note's next on the channel is struck, for a run's
        // timing to tighten to.
        let mut starts: Vec<u32> = score.notes.iter().filter(|n| n.channel == ch).map(|n| n.start).collect();
        starts.sort();
        starts.dedup();
        let interval = |start: u32| -> f64 { starts.iter().find(|s| **s > start).map_or(1.0, |s| score.seconds(*s) - score.seconds(start)) };
        // The ticks another lead guitar strikes on: a tone in harmony with
        // one is played plain, its vibrato alone, so the thirds stay thirds.
        let others: Vec<u32> = score.notes.iter().filter(|n| n.channel != ch && score.instrument(n.channel).role == Role::Melody && score.player(n.channel).ornaments.is_some()).map(|n| n.start).collect();

        // (on, off, pitch, vel, whether it swells)
        let mut notes: Vec<(f64, f64, u8, u8, bool)> = Vec::new();
        // A guitar's notes as played, with each one's tuning, cents,
        // whether it is in harmony with another's, and its written tick.
        let mut played: Vec<(f64, f64, u8, f32, bool, u32)> = Vec::new();
        for n in score.notes.iter().filter(|n| n.channel == ch) {
            let mut rng = salt.fork(((ch as u64) << 40) ^ ((n.pitch as u64) << 32) ^ n.start as u64);
            let written = score.seconds(n.start);
            let written_end = written + (score.seconds(n.end()) - written) * f.length as f64;
            let off_beat = (lean + f.drift * time.at(written)) / 1000.0;
            // In a fast run a player's hand errs by a few hundredths of
            // the time between notes, with no lean behind the beat.
            let slip_ms = f.run.map_or(f.slip, |run| f.slip.min((run * 1000.0) * interval(n.start) as f32));
            let off_beat = if f.run.is_some() { off_beat.min(slip_ms / 1000.0) } else { off_beat };
            let on = written + (off_beat + slip_ms * slip(&mut rng) / 1000.0) as f64;
            let off = written_end + (off_beat + 0.4 * f.slip * slip(&mut rng) / 1000.0) as f64;
            // A long tone settles to the player's dynamic: the hand's slip
            // is heard in a stroke, not held for bars.
            let settle = (SETTLE_S / (written_end - written)).min(1.0) as f32;
            let vel = f.force + if f.touch > 0.0 { n.vel as f32 + BREATH_TOUCH * breath.at(written) + settle * f.touch * slip(&mut rng) } else { n.vel as f32 };
            let vel = vel.round().clamp(1.0, 127.0) as u8;
            notes.push((on.max(0.0), off.max(0.0), n.pitch, vel, swells(written_end - written)));
            if guitar.is_some() {
                played.push((on.max(0.0), off.max(0.0), n.pitch, tuning + f.intonation * slip(&mut rng), others.contains(&n.start), n.start));
            } else if f.intonation > 0.0 {
                let cents = tuning + f.intonation * slip(&mut rng);
                out.push(Played { at: on.max(0.0), msg: Msg::Bend { channel: ch, value: bend_on(cents, range) } });
            }
        }
        // A pitch's release never lands after its next strike, or the
        // late release silences the new note.
        notes.sort_by(|a, b| (a.2, a.0).partial_cmp(&(b.2, b.0)).unwrap());
        for i in 0..notes.len() {
            let next = notes.get(i + 1).filter(|m| m.2 == notes[i].2).map(|m| m.0);
            let n = &mut notes[i];
            if let Some(next) = next {
                n.1 = n.1.min(next);
            }
            n.1 = n.1.max(n.0 + SHORTEST_S);
        }
        notes.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let mut fades: Vec<(f64, f64)> = Vec::new();
        // The notes slurred from the one before, by start and pitch.
        let mut slurred: Vec<(f64, u8)> = Vec::new();
        if let Some(g) = &guitar {
            played.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            // Each note's release as it was settled, by its start and pitch.
            let ends: Vec<(f64, u8, f64)> = notes.iter().map(|n| (n.0, n.2, n.1)).collect();
            let mut in_row = 0u8;
            for i in 0..played.len() {
                let (on, _, pitch, cents, plain, tick) = played[i];
                let off = ends.iter().find(|e| e.0 == on && e.1 == pitch).map_or(played[i].1, |e| e.2);
                let prev = i.checked_sub(1).map(|j| (played[j].0, played[j].1, played[j].2));
                let next = played.get(i + 1).map(|n| (n.0, n.1, n.2));
                let mut rng = salt.fork(((ch as u64) << 40) ^ ((pitch as u64) << 32) ^ (on * 1000.0) as u64 ^ 0x6775_6974);
                let whole = score.key_at(score.tick_at(on)).contains(pitch.saturating_sub(2));
                let slid = ornament(&mut out, &mut fades, g, ch, (on, off, pitch, cents), prev, next, whole && !plain, plain, &mut rng);
                let slurs = f.legato.zip(prev).is_some_and(|(l, p)| {
                    let step = pitch.abs_diff(p.2);
                    let mut rng = salt.fork(((ch as u64) << 40) ^ ((pitch as u64) << 32) ^ tick as u64 ^ 0x6c65_6761);
                    !plain && on - p.1 < l.gap_s && (1..=l.reach).contains(&step) && tick % (2 * TICKS_PER_EIGHTH) != 0 && in_row < l.run && rng.chance(l.share)
                });
                if slurs || slid {
                    slurred.push((on, pitch));
                    in_row += 1;
                } else {
                    in_row = 0;
                }
            }
        }
        // A slurred note sounds softer, and the note before it rings on
        // into it, the string never let go.
        if let Some(l) = &f.legato {
            for i in 1..notes.len() {
                if slurred.contains(&(notes[i].0, notes[i].2)) {
                    notes[i].3 = notes[i].3.saturating_sub(l.softer).max(1);
                    let on = notes[i].0;
                    notes[i - 1].1 = notes[i - 1].1.max(on);
                }
            }
        }

        for &(on, off, pitch, vel, _) in &notes {
            let legato = slurred.contains(&(on, pitch));
            out.push(Played { at: on, msg: Msg::On { channel: ch, pitch, vel, legato } });
            out.push(Played { at: off, msg: Msg::Off { channel: ch, pitch } });
        }

        // The pedal: the section's trim, the player's breath, and the
        // swell from the player's entry while its tones stay joined.
        let end_s = score.seconds(score.end());
        let steps = (end_s / PEDAL_S).ceil() as usize;
        let mut entry: Option<(f64, f64)> = None;
        let mut sounding_until = f64::NEG_INFINITY;
        let mut next = 0;
        let mut sent = None;
        for k in 0..=steps {
            let t = k as f64 * PEDAL_S;
            while next < notes.len() && notes[next].0 <= t {
                let (on, off, _, _, swells) = notes[next];
                let joined = sounding_until + JOIN_S >= on;
                entry = match entry {
                    _ if !swells => None,
                    Some((from, until)) if joined => Some((from, until.max(off))),
                    _ => Some((on, off)),
                };
                sounding_until = sounding_until.max(off);
                next += 1;
            }
            let held = entry.filter(|&(_, off)| t < off).map_or(0.0, |(on, _)| swell(t - on));
            // A slide off a phrase's end fades with its fall.
            let fade = fades.iter().find(|(a, b)| t >= *a && t < *b).map_or(0.0, |(a, b)| -24.0 * ((t - a) / (b - a)) as f32);
            let db = held + BREATH_DB * (breath.at(t) - 1.0) / 2.0 + fade;
            let tick = score.tick_at(t).min(score.end().saturating_sub(1));
            let gain = score.trim_at(tick).clamp(0.0, 1.0) * 10f32.powf(db / 20.0);
            // The synthesizer squares the pedal into gain.
            let value = (127.0 * gain.sqrt()).round() as u8;
            if sent != Some(value) {
                out.push(Played { at: t, msg: Msg::Control { channel: ch, number: 11, value } });
                sent = Some(value);
            }
        }
    }
    out.sort_by(|a, b| (a.at, a.msg.order()).partial_cmp(&(b.at, b.msg.order())).unwrap());
    out
}

/// A lead's note's ornaments as pitch bends on `ch`, around its tuning
/// `cents`: bent or slid into where it arrives, its vibrato where it is
/// held, let go its own way where it ends a phrase. `whole` is whether a
/// whole step under it is in the key, so a bend rises through the mode.
/// A fall off a phrase's end is a fade in `fades` as well. A `plain` tone
/// takes its vibrato alone. Whether it slid in from the note before, the
/// string never picked.
#[allow(clippy::too_many_arguments)]
fn ornament(out: &mut Vec<Played>, fades: &mut Vec<(f64, f64)>, g: &Ornaments, ch: u8, (on, off, pitch, cents): (f64, f64, u8, f32), prev: Option<(f64, f64, u8)>, next: Option<(f64, f64, u8)>, whole: bool, plain: bool, rng: &mut Rng) -> bool {
    let at = |t: f64, c: f32, out: &mut Vec<Played>| out.push(Played { at: t, msg: Msg::Bend { channel: ch, value: bend_on(c, g.range) } });
    let span = |r: (f64, f64), rng: &mut Rng| r.0 + (r.1 - r.0) * rng.f32() as f64;
    let len = off - on;
    let ends_phrase = next.is_none_or(|n| n.0 - off > g.phrase_gap_s);
    let arrives = len >= g.arrival_s && !plain;
    // Where a bend lands: a few cents off, sharp more often than flat.
    let landing = cents + 4.0 + 6.0 * slip(rng);
    let mut bent = false;
    let mut from_prev = false;
    let mut settled = on;
    let grace = g.grace.filter(|gr| arrives && rng.chance(gr.share));
    if let Some(gr) = grace {
        // A jump from over the tone and down onto it, held the same at any
        // tempo.
        at(on, cents + gr.cents, out);
        settled = on + gr.s.min(len / 3.0);
        at(settled, cents, out);
    } else if arrives && rng.chance(g.bend) {
        let under = if whole && rng.chance(g.whole) { 200.0 } else { 100.0 };
        let rise = span(g.rise_s, rng).min(len / 3.0);
        for k in 0..=6 {
            let x = k as f32 / 6.0;
            let curve = x * x * (3.0 - 2.0 * x);
            at(on + rise * x as f64, landing - under * (1.0 - curve), out);
        }
        bent = true;
        settled = on + rise;
        if len >= 2.0 * g.vibrato_min_s && rng.chance(g.release) {
            // Let back down to the fretted tone over the last of the note.
            let from = off - (len / 4.0).min(0.25);
            for k in 1..=4 {
                let x = k as f32 / 4.0;
                at(from + (off - from) * x as f64, landing - under * x, out);
            }
            return false;
        }
    } else if arrives && rng.chance(g.slide / (1.0 - g.bend).max(0.01)) {
        // From the note before where it is a few frets under, else two to
        // five frets under, a fret at a time.
        let frets = match prev {
            Some(p) if p.2 < pitch && pitch - p.2 <= 5 && on - p.1 < 0.1 => {
                from_prev = true;
                (pitch - p.2) as i32
            }
            _ => rng.range(2, 5),
        };
        let step = span(g.slide_step_s, rng).min(len / (2.0 * frets as f64));
        for k in 0..=frets {
            at(on + step * k as f64, cents - 100.0 * (frets - k) as f32, out);
        }
        settled = on + step * frets as f64;
    } else {
        at(on, cents, out);
    }
    let base = if bent { landing } else { cents };
    // A phrase's end: its vibrato most often; bent up a step, or slid off
    // and fading, one time in seven each.
    let end = if ends_phrase && !plain { rng.weighted(&g.ends) } else { 0 };
    let shake_until = match end {
        1 if !bent && arrives => {
            let from = off - (len / 3.0).min(0.2);
            for k in 1..=5 {
                let x = k as f32 / 5.0;
                at(from + (off - from) * x as f64, base + 200.0 * x * x * (3.0 - 2.0 * x), out);
            }
            from
        }
        2 => {
            let from = (off - g.fall_s).max(settled);
            let fall = rng.range(g.fall.0, g.fall.1);
            for k in 1..=fall {
                at(from + (off - from) * k as f64 / fall as f64, base - 100.0 * k as f32, out);
            }
            fades.push((from, off));
            from
        }
        _ => off,
    };
    // The vibrato from once the tone settles, or late in a long tone where
    // the player's comes late; a phrase's last long tone shaken to the tone
    // over it through its last moments instead, where the player shakes.
    let from = settled.max(on + span(g.vibrato_from_s, rng));
    let from = if len >= g.wide_s { from.max(on + g.terminal as f64 * len) } else { from };
    let shake = g.shake.filter(|sh| ends_phrase && end == 0 && len >= g.wide_s && !plain && rng.chance(sh.share));
    let shake_from = shake.map(|_| (off - SHAKE_S).max(from));
    let shake_until = shake_from.map_or(shake_until, |s| s.min(shake_until));
    if let (Some(sh), Some(start)) = (shake, shake_from) {
        // Quickening from half its rate to its rate, as a lip trill starts.
        let mut t = start;
        let mut up = true;
        while t < off {
            let x = ((t - start) / (off - start).max(1e-3)) as f32;
            let half = 0.5 / (sh.rate_hz * (0.5 + 0.5 * x)) as f64;
            if t + half >= off {
                break;
            }
            at(t, base + if up { sh.cents } else { 0.0 }, out);
            up = !up;
            t += half;
        }
        at(t.min(off), base, out);
    }
    // The vibrato, once the tone settles: arcs from where it is, up from a
    // plain tone — down first where the player dips — and down from a
    // bent one, quickening across the tone.
    if shake_until - from >= g.vibrato_min_s / 2.0 && len >= g.vibrato_min_s {
        let wide = len >= g.wide_s && rng.chance(0.5);
        let depth = if wide { g.wide.0 + (g.wide.1 - g.wide.0) * rng.f32() } else { g.narrow.0 + (g.narrow.1 - g.narrow.0) * rng.f32() };
        let rate = g.rate_hz.0 + (g.rate_hz.1 - g.rate_hz.0) * rng.f32();
        let way = if bent || g.dips { -1.0 } else { 1.0 };
        let mut t = from;
        while t < shake_until {
            let quicker = 1.0 + (g.accel - 1.0) * ((t - from) / (shake_until - from)) as f32;
            let cycle = 1.0 / (rate * quicker * (0.9 + 0.2 * rng.f32())) as f64;
            let d = depth * (0.8 + 0.4 * rng.f32());
            for k in 1..=6 {
                let x = k as f32 / 6.0;
                let when = t + cycle * x as f64;
                if when >= shake_until {
                    break;
                }
                at(when, base + way * d * (std::f32::consts::PI * x).sin(), out);
            }
            t += cycle;
        }
        at(shake_until.min(off), base, out);
    }
    from_prev
}

/// A message of a score as MIDI: when, in seconds, and its bytes — two
/// for a program change, three for the rest.
#[derive(Clone, Debug)]
pub struct Event {
    pub at: f64,
    pub bytes: Vec<u8>,
}

/// The score as `perform` plays it, as MIDI any synthesizer takes: each
/// player's program, reverb send, pan and volume first, then every
/// message played, in `perform`'s order.
pub fn midi(score: &Score) -> Vec<Event> {
    let mut out = Vec::new();
    for inst in &score.instruments {
        let ch = inst.channel;
        out.push(Event { at: 0.0, bytes: vec![0xC0 | ch, inst.program] });
        out.push(Event { at: 0.0, bytes: vec![0xB0 | ch, 91, inst.reverb] });
        out.push(Event { at: 0.0, bytes: vec![0xB0 | ch, 10, (64 + inst.pan as i32) as u8] });
        out.push(Event { at: 0.0, bytes: vec![0xB0 | ch, 7, volume(inst.level)] });
    }
    out.extend(perform(score).into_iter().map(|p| Event {
        at: p.at,
        bytes: match p.msg {
            Msg::On { channel, pitch, vel, .. } => vec![0x90 | channel, pitch, vel],
            Msg::Off { channel, pitch } => vec![0x80 | channel, pitch, 0],
            Msg::Control { channel, number, value } => vec![0xB0 | channel, number, value],
            Msg::Bend { channel, value } => vec![0xE0 | channel, (value & 0x7F) as u8, (value >> 7) as u8],
        },
    }));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midi_sets_every_player_up_first_and_turns_every_note_off() {
        for piece in crate::pieces::TRACKS {
            let score = (piece.build)(&crate::pieces::Params::of(piece, 0));
            let events = midi(&score);
            let setup = 4 * score.instruments.len();
            assert!(events[..setup].iter().all(|e| e.at == 0.0 && e.bytes[0] & 0xF0 != 0x90), "{}", piece.name);
            assert!(events[setup..].windows(2).all(|w| w[0].at <= w[1].at), "{}", piece.name);
            let mut sounding = std::collections::HashMap::new();
            for e in &events[setup..] {
                let key = (e.bytes[0] & 0x0F, e.bytes[1]);
                match e.bytes[0] & 0xF0 {
                    0x90 => *sounding.entry(key).or_insert(0) += 1,
                    0x80 => *sounding.entry(key).or_insert(0) -= 1,
                    _ => {}
                }
            }
            assert!(sounding.values().all(|n| *n <= 0), "{}: a note is left on", piece.name);
        }
    }

    #[test]
    fn a_swell_rises_to_full_and_eases_back_part_way() {
        assert!(swell(0.0) < swell(SWELL_S / 2.0));
        assert!(swell(SWELL_S / 2.0) < swell(SWELL_S));
        assert!(swell(SWELL_S) > swell(SWELL_S + 10.0));
        assert!(swell(1000.0) > swell(0.0));
    }
}
