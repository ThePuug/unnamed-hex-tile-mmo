//! The players: what a written score becomes when people play it. The
//! score is exact — every note on its tick, at its velocity, in tune —
//! and a band that plays it so sounds like machines. Each player here
//! has habits: a lean ahead of or behind the beat, a sense of time that
//! drifts, a hand that is never twice the same, a tuning a few cents
//! off. Over those, each player's intensity rises and falls on its own
//! slow breath; a held tone swells as the player enters and carries
//! through every tone joined to it; a sung or bowed tone grows a
//! vibrato once it settles.
//!
//! Every draw is keyed on the note's place in the period — its channel,
//! pitch and tick modulo `period` — and every slow curve has a whole
//! number of cycles in it, so a loop unrolled for rendering plays each
//! pass the same and its seam holds. The checks judge the written score;
//! this is what is heard.

use audio::rng::Rng;

use crate::score::{Role, Score};

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
    On { channel: u8, pitch: u8, vel: u8 },
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

/// How a role plays, milliseconds and velocity steps. `lean` is the
/// range a player's habitual place against the beat is drawn from;
/// `drift` how far the player's time wanders over the slow breath;
/// `slip` the spread of one note's error about both.
struct Feel {
    lean: (f32, f32),
    drift: f32,
    slip: f32,
    touch: f32,
}

fn feel(role: Role) -> Feel {
    match role {
        // The drum keeps the time the others lean against.
        Role::Percussion => Feel { lean: (-4.0, 4.0), drift: 4.0, slip: 5.0, touch: 8.0 },
        Role::Pluck => Feel { lean: (-5.0, 10.0), drift: 6.0, slip: 8.0, touch: 7.0 },
        Role::Melody => Feel { lean: (0.0, 18.0), drift: 10.0, slip: 14.0, touch: 6.0 },
        Role::Doubling => Feel { lean: (0.0, 20.0), drift: 10.0, slip: 14.0, touch: 5.0 },
        // A section's entries spread; a slow attack hides it.
        Role::Sustain => Feel { lean: (0.0, 25.0), drift: 12.0, slip: 22.0, touch: 4.0 },
        // The drone is struck once and held; it has nothing to place.
        Role::Drone => Feel { lean: (0.0, 0.0), drift: 0.0, slip: 0.0, touch: 0.0 },
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

/// A melody's tone swells only if it is written this long.
const SWELL_MELODY_S: f64 = 1.0;

/// How often the expression pedal moves, seconds: fine enough that a
/// swell is a curve, not steps.
const PEDAL_S: f64 = 0.04;

/// Vibrato: the mod wheel's depth range a player's is drawn from (127
/// is ±50 cents on this synthesizer), when it starts after the attack,
/// how long it takes to reach depth, and the shortest tone that gets it.
const VIBRATO: (f32, f32) = (22.0, 36.0);
const VIBRATO_FROM_S: f64 = 0.18;
const VIBRATO_RAMP_S: f64 = 0.42;
const VIBRATO_MIN_S: f64 = 0.4;
const VIBRATO_STEPS: usize = 6;

/// A player's tuning, cents: the spread it is drawn from and its bound;
/// a melody's every note is pitched with a spread of its own besides.
const TUNING: f32 = 2.5;
const TUNING_MAX: f32 = 5.0;
const INTONATION: f32 = 3.0;

/// The shortest a played note is, seconds.
const SHORTEST_S: f64 = 0.03;

/// Whether a General MIDI program sustains a tone a player can shape
/// with vibrato: bowed strings, voices, brass, reeds, pipes, harmonica,
/// fiddle.
fn sings(program: u8) -> bool {
    matches!(program, 22 | 29 | 30 | 40..=44 | 48..=49 | 52..=54 | 56..=79 | 110)
}

/// Whether a General MIDI program's player bends up into a long tone,
/// as a lead guitarist does: the overdriven and the distorted guitar.
fn bends_in(program: u8) -> bool {
    matches!(program, 29 | 30)
}

/// A bend into a tone: the shortest tone a player bends into, how often
/// it does, and how long the rise takes. It starts a whole step under
/// where that is the mode's whole tone, else a semitone, so the bend
/// rises through the mode.
const BEND_IN_S: f64 = 0.35;
const BEND_IN_CHANCE: f32 = 0.4;
const BEND_IN_RISE_S: f64 = 0.1;
const BEND_IN_STEPS: usize = 6;

/// A draw near zero with unit spread, bounded at ±2.5: the sum of four
/// uniforms.
fn slip(rng: &mut Rng) -> f32 {
    let s: f32 = (0..4).map(|_| rng.f32()).sum();
    ((s - 2.0) * 3f32.sqrt()).clamp(-2.5, 2.5)
}

/// A player's slow breath: two sines with a whole number of cycles in
/// the period, one about 24 s and one about 9 s long, in -1..1.
struct Breath {
    cycles: [f64; 2],
    phase: [f64; 2],
    period_s: f64,
}

impl Breath {
    fn new(rng: &mut Rng, period_s: f64) -> Self {
        let cycles = [(period_s / 24.0).round().max(1.0), (period_s / 9.0).round().max(2.0)];
        let phase = [rng.f32() as f64 * std::f64::consts::TAU, rng.f32() as f64 * std::f64::consts::TAU];
        Breath { cycles, phase, period_s }
    }

    fn at(&self, t: f64) -> f32 {
        let w = |k: usize| (std::f64::consts::TAU * self.cycles[k] * t / self.period_s + self.phase[k]).sin();
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

fn bend(cents: f32) -> u16 {
    (8192.0 + cents * 8192.0 / 200.0).round().clamp(0.0, 16383.0) as u16
}

/// The score as played, in time order. `period` is the tick at which
/// the performance repeats: the score's end, or, for a loop unrolled to
/// render, one pass's.
pub fn perform(score: &Score, period: u32) -> Vec<Played> {
    let period_s = score.seconds(period);
    let tick_s = score.seconds(1);
    let salt = Rng::new(period as u64 ^ (score.eighth_bpm.to_bits() as u64) << 32);
    let mut out = Vec::new();

    for inst in &score.instruments {
        let ch = inst.channel;
        let f = feel(inst.role);
        let mut player = salt.fork(ch as u64 + 1);
        let lean = f.lean.0 + (f.lean.1 - f.lean.0) * player.f32();
        let breath = Breath::new(&mut player, period_s);
        let time = Breath::new(&mut player, period_s);
        let depth = VIBRATO.0 + (VIBRATO.1 - VIBRATO.0) * player.f32();
        let percussion = inst.role == Role::Percussion;
        let tuning = if percussion { 0.0 } else { (slip(&mut player) * TUNING).clamp(-TUNING_MAX, TUNING_MAX) };
        let vibrato = inst.role != Role::Sustain && inst.role != Role::Drone && sings(inst.program);
        let swells = |len_s: f64| match inst.role {
            Role::Sustain | Role::Drone => true,
            Role::Melody | Role::Doubling => len_s >= SWELL_MELODY_S,
            _ => false,
        };

        if !percussion {
            out.push(Played { at: 0.0, msg: Msg::Bend { channel: ch, value: bend(tuning) } });
        }

        // (on, off, pitch, vel, whether it swells)
        let mut notes: Vec<(f64, f64, u8, u8, bool)> = Vec::new();
        for n in score.notes.iter().filter(|n| n.channel == ch) {
            let mut rng = salt.fork(((ch as u64) << 40) ^ ((n.pitch as u64) << 32) ^ (n.start % period) as u64);
            let written = score.seconds(n.start);
            let written_end = score.seconds(n.end());
            let off_beat = (lean + f.drift * time.at(written)) / 1000.0;
            let on = written + (off_beat + f.slip * slip(&mut rng) / 1000.0) as f64;
            let off = written_end + (off_beat + 0.4 * f.slip * slip(&mut rng) / 1000.0) as f64;
            // A long tone settles to the player's dynamic: the hand's slip
            // is heard in a stroke, not held for bars.
            let settle = (SETTLE_S / (written_end - written)).min(1.0) as f32;
            let vel = if f.touch > 0.0 { n.vel as f32 + BREATH_TOUCH * breath.at(written) + settle * f.touch * slip(&mut rng) } else { n.vel as f32 };
            let vel = vel.round().clamp(1.0, 127.0) as u8;
            notes.push((on.max(0.0), off.max(0.0), n.pitch, vel, swells(written_end - written)));
            if inst.role == Role::Melody {
                let cents = tuning + INTONATION * slip(&mut rng);
                if bends_in(inst.program) && written_end - written >= BEND_IN_S && rng.chance(BEND_IN_CHANCE) {
                    let under = if score.key.contains(n.pitch.saturating_sub(2)) { 200.0 } else { 100.0 };
                    for k in 0..=BEND_IN_STEPS {
                        let x = k as f32 / BEND_IN_STEPS as f32;
                        let rise = x * x * (3.0 - 2.0 * x);
                        out.push(Played { at: on.max(0.0) + BEND_IN_RISE_S * x as f64, msg: Msg::Bend { channel: ch, value: bend(cents - under * (1.0 - rise)) } });
                    }
                } else {
                    out.push(Played { at: on.max(0.0), msg: Msg::Bend { channel: ch, value: bend(cents) } });
                }
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

        for &(on, off, pitch, vel, _) in &notes {
            out.push(Played { at: on, msg: Msg::On { channel: ch, pitch, vel } });
            out.push(Played { at: off, msg: Msg::Off { channel: ch, pitch } });
            if vibrato && off - on >= VIBRATO_MIN_S {
                out.push(Played { at: on, msg: Msg::Control { channel: ch, number: 1, value: 0 } });
                for k in 1..=VIBRATO_STEPS {
                    let x = k as f64 / VIBRATO_STEPS as f64;
                    let at = on + VIBRATO_FROM_S + VIBRATO_RAMP_S * x;
                    if at < off {
                        out.push(Played { at, msg: Msg::Control { channel: ch, number: 1, value: (depth * x as f32).round() as u8 } });
                    }
                }
            }
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
            let db = held + BREATH_DB * (breath.at(t) - 1.0) / 2.0;
            let tick = ((t / tick_s) as u32).min(score.end().saturating_sub(1));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_swell_rises_to_full_and_eases_back_part_way() {
        assert!(swell(0.0) < swell(SWELL_S / 2.0));
        assert!(swell(SWELL_S / 2.0) < swell(SWELL_S));
        assert!(swell(SWELL_S) > swell(SWELL_S + 10.0));
        assert!(swell(1000.0) > swell(0.0));
    }

    #[test]
    fn a_breath_repeats_with_its_period() {
        let b = Breath::new(&mut Rng::new(3), 111.3);
        for t in [0.0, 7.5, 40.2] {
            assert!((b.at(t) - b.at(t + 111.3)).abs() < 1e-4);
        }
    }
}
