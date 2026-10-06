//! Who plays each part. The score says what is played and the instrument
//! what sounds; a player is the hands between them: where it sits against
//! the beat, how its time wanders, how unevenly it strikes, how far out
//! of tune it pitches, whether it swells into a held tone, how it shakes
//! one, and, for a lead guitarist, how it bends, slides and lets a phrase
//! go. `perform` plays a score by its players' habits.
//!
//! A part is played by the player its role and instrument call for
//! (`of`) unless its score casts another (`Score::cast`), so a player is
//! tuned here once for every part it plays, and swapping one, a drummer
//! for another, changes nothing written.

use crate::score::Role;

/// A player's habits, in milliseconds, velocity steps and cents.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Player {
    pub name: &'static str,
    /// The range its habitual place against the beat is drawn from, ms.
    pub lean: (f32, f32),
    /// How far its time wanders over its slow breath, ms.
    pub drift: f32,
    /// The spread of one stroke's error about both, ms.
    pub slip: f32,
    /// In a run, the most a stroke errs and leans, as a share of the time
    /// to its next stroke; none where it errs alike at any speed.
    pub run: Option<f32>,
    /// The spread of one stroke's velocity, steps; none where it strikes
    /// every note as written.
    pub touch: f32,
    /// The spread its tuning is drawn from, cents; nothing for a drum.
    pub tuning: f32,
    /// The spread each tone is pitched off its tuning besides, cents;
    /// none where its tones are pitched alike.
    pub intonation: f32,
    /// The shortest tone it swells into, seconds; it swells into none
    /// where there is none.
    pub swell: Option<f64>,
    /// Its vibrato on an instrument that sings; none where it plays
    /// without one, or makes one of its own (`ornaments`).
    pub vibrato: Option<Vibrato>,
    /// A lead guitarist's bends, slides and phrase ends, where it plays
    /// so.
    pub ornaments: Option<Guitar>,
    /// Which of its notes it slurs from the one before rather than
    /// picks, where it does.
    pub legato: Option<Legato>,
}

/// How a guitarist slurs a run: a note joined to the one before it, a few
/// frets from it and off the beat, is hammered on or pulled off — `share`
/// of the time, never more than `run` in a row, so the hand picks again
/// at each string's first note — and sounds `softer` than a picked one.
/// A note slid into from the one before is slurred as well.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Legato {
    /// The widest slur, semitones.
    pub reach: u8,
    /// How soon after the note before ends a note is joined to it, s.
    pub gap_s: f64,
    pub run: u8,
    pub share: f32,
    /// Velocity steps.
    pub softer: u8,
}

/// A vibrato by the mod wheel: the depth a player's is drawn from (127 is
/// ±50 cents on this synthesizer), when it starts after the attack, how
/// long it takes to reach depth, and the shortest tone that gets it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vibrato {
    pub depth: (f32, f32),
    pub from_s: f64,
    pub ramp_s: f64,
    pub min_s: f64,
}

/// How a lead guitarist ornaments, from players' transcriptions and the
/// literature on guitar technique (`proofs/research/lead-findings.md`,
/// `guitar-lead-findings.md`): about two notes in five ornamented. An
/// arrival — a tone of `arrival_s` or more — is bent into from a whole
/// step under three times in four where the mode has one, else a half
/// step, or slid into from two to five frets under, a fret at a time; a
/// bend rises in sixty milliseconds to an eighth and lands a few cents
/// off, sharp more often than flat, and a held peak is now and then let
/// back down. A held tone shakes with a vibrato made of the bend, rising
/// from a fretted tone and falling from a bent one, five to seven times a
/// second, every cycle a little different, wide on a long peak. A phrase
/// ends on its vibrato most often, bent up a step or slid off and fading
/// one time in seven each.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Guitar {
    /// The bend range its channel is set to, semitones: wide enough for
    /// a slide.
    pub range: f32,
    pub arrival_s: f64,
    /// How often an arrival is bent into, and slid into.
    pub bend: f32,
    pub slide: f32,
    /// How often a held bend is let back down.
    pub release: f32,
    pub rise_s: (f64, f64),
    pub slide_step_s: (f64, f64),
    pub vibrato_from_s: (f64, f64),
    pub vibrato_min_s: f64,
    pub rate_hz: (f32, f32),
    /// The vibrato's depth, cents: a narrow one, and the wide one a held
    /// tone of `wide_s` or longer takes half the time.
    pub narrow: (f32, f32),
    pub wide: (f32, f32),
    pub wide_s: f64,
    /// A gap this long after a tone ends its phrase.
    pub phrase_gap_s: f64,
    /// A slide off a phrase's end: how many frets it falls, and over how
    /// long.
    pub fall: (i32, i32),
    pub fall_s: f64,
}

/// The vibrato every singing player uses.
const WHEEL: Vibrato = Vibrato { depth: (22.0, 36.0), from_s: 0.18, ramp_s: 0.42, min_s: 0.4 };

/// A pitched player's tuning spread, cents, and a melody's intonation.
const TUNING: f32 = 2.5;
const INTONATION: f32 = 3.0;

/// The drum keeps the time the others lean against.
pub const DRUMMER: Player = Player {
    name: "drummer",
    lean: (-4.0, 4.0),
    drift: 4.0,
    slip: 5.0,
    run: None,
    touch: 8.0,
    tuning: 0.0,
    intonation: 0.0,
    swell: None,
    vibrato: None,
    ornaments: None,
    legato: None,
};

/// A part struck on the groove: a bass, a comp, a riff.
pub const PICKER: Player = Player {
    name: "picker",
    lean: (-5.0, 10.0),
    drift: 6.0,
    slip: 8.0,
    run: None,
    touch: 7.0,
    tuning: TUNING,
    intonation: 0.0,
    swell: None,
    vibrato: Some(WHEEL),
    ornaments: None,
    legato: None,
};

/// A tune's player, a little behind the beat, swelling into its long
/// tones.
pub const SINGER: Player = Player {
    name: "singer",
    lean: (0.0, 18.0),
    drift: 10.0,
    slip: 14.0,
    run: None,
    touch: 6.0,
    tuning: TUNING,
    intonation: INTONATION,
    swell: Some(1.0),
    vibrato: Some(WHEEL),
    ornaments: None,
    legato: None,
};

/// A lead guitarist: a singer's time and touch, tight in a run, its
/// vibrato its own bend.
pub const LEAD_GUITARIST: Player = Player {
    name: "lead guitarist",
    run: Some(0.045),
    vibrato: None,
    ornaments: Some(Guitar {
        range: 12.0,
        arrival_s: 0.3,
        bend: 0.45,
        slide: 0.2,
        release: 0.27,
        rise_s: (0.06, 0.16),
        slide_step_s: (0.02, 0.045),
        vibrato_from_s: (0.25, 0.5),
        vibrato_min_s: 0.4,
        rate_hz: (5.0, 7.0),
        narrow: (30.0, 60.0),
        wide: (100.0, 180.0),
        wide_s: 1.2,
        phrase_gap_s: 0.15,
        fall: (5, 12),
        fall_s: 0.18,
    }),
    legato: Some(Legato { reach: 4, gap_s: 0.06, run: 2, share: 0.75, softer: 16 }),
    ..SINGER
};

/// A line doubling another's.
pub const DOUBLER: Player = Player {
    name: "doubler",
    lean: (0.0, 20.0),
    drift: 10.0,
    slip: 14.0,
    run: None,
    touch: 5.0,
    tuning: TUNING,
    intonation: 0.0,
    swell: Some(1.0),
    vibrato: Some(WHEEL),
    ornaments: None,
    legato: None,
};

/// A section holding the harmony: its entries spread, and a slow attack
/// hides it.
pub const SECTION: Player = Player {
    name: "section",
    lean: (0.0, 25.0),
    drift: 12.0,
    slip: 22.0,
    run: None,
    touch: 4.0,
    tuning: TUNING,
    intonation: 0.0,
    swell: Some(0.0),
    vibrato: None,
    ornaments: None,
    legato: None,
};

/// The drone is struck once and held; it has nothing to place.
pub const DRONE: Player = Player {
    name: "drone",
    lean: (0.0, 0.0),
    drift: 0.0,
    slip: 0.0,
    run: None,
    touch: 0.0,
    tuning: TUNING,
    intonation: 0.0,
    swell: Some(0.0),
    vibrato: None,
    ornaments: None,
    legato: None,
};

/// Whether a General MIDI program bends as a lead guitarist plays it:
/// the overdriven and the distorted guitar.
fn bends(program: u8) -> bool {
    matches!(program, 29 | 30)
}

/// The player a part of `role` on `program` is played by where its score
/// casts none.
pub fn of(role: Role, program: u8) -> &'static Player {
    match role {
        Role::Percussion => &DRUMMER,
        Role::Pluck => &PICKER,
        Role::Melody if bends(program) => &LEAD_GUITARIST,
        Role::Melody => &SINGER,
        Role::Doubling => &DOUBLER,
        Role::Sustain => &SECTION,
        Role::Drone => &DRONE,
    }
}
