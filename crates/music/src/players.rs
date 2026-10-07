//! Who plays each part. The score says what is played and the instrument
//! what sounds; a player is the hands between them: where it sits against
//! the beat, how its time wanders, how unevenly it strikes, how far out
//! of tune it pitches, whether it swells into a held tone, and how it
//! shapes a tone — its vibrato, and for a lead how it bends, scoops,
//! slides, graces and lets a phrase go — a guitarist's way, a
//! saxophonist's, a zurna player's. Every shape is a bend of the
//! channel's pitch, the player's own: a bank's mod-wheel vibrato runs at
//! the bank's rate, which on the harmonica and the tenor is a bleat, and
//! on a bank recorded with vibrato is a second one beating against it.
//! `perform` plays a score by its players' habits.
//!
//! A part is played by the player its role and instrument call for
//! (`of`) unless its score casts another (`Score::cast`), so a player is
//! tuned here once for every part it plays, and swapping one, a drummer
//! for another, changes nothing written. The harness offers every habit
//! and no part takes them all: the lead characterises — scoops, falls,
//! graces, shakes, its own vibrato — and a second voice under it plays
//! plain, as a section player does. Which player a style's lead is, the
//! style says, casting it (`Score::cast`): a village clarinet's pralls
//! and not a blues horn's, a saxophonist's falls and not a zurna's.
//! Unless a style casts it, a part is played by its role and its
//! instrument alone (`of`), which know no style.

use crate::score::Role;
use crate::voices;

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
    /// A lead's bends, scoops, slides, graces and phrase ends, where it
    /// plays so.
    pub ornaments: Option<Ornaments>,
    /// Which of its notes it slurs from the one before rather than
    /// picks, where it does.
    pub legato: Option<Legato>,
    /// How much harder than written it strikes every note, velocity
    /// steps; under nothing, softer.
    pub force: f32,
    /// The share of each written length it holds a note for: under one,
    /// detached; over, carried into the next.
    pub length: f32,
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

/// How a lead ornaments, every gesture a bend of its channel's pitch, so
/// a note is shaped and never struck again. An arrival — a tone of
/// `arrival_s` or more — may be graced from over it, bent or scooped
/// into from a whole step under where the mode has one, `whole` of the
/// time, else a half step, or slid into a semitone at a time; a bend
/// lands a few cents off, sharp more often than flat, and a held peak is
/// now and then let back down. A held tone shakes with a vibrato made of
/// the bend, rising from a plain tone, or dipping first where the player
/// dips, and falling from a bent one, every cycle a little different,
/// quickening across the tone by `accel`, wide on a long peak — or, now
/// and then, is shaken to the tone over it instead. A phrase ends on its
/// vibrato, bent up a step, or let fall and fade, as often as `ends`
/// says. A guitarist's way is `LEAD_GUITARIST`'s; the winds' and the
/// horns' are their own presets, from the studies of each
/// (`proofs/research/ornaments-findings.md`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ornaments {
    /// The bend range its channel is set to, semitones: wide enough for
    /// a slide.
    pub range: f32,
    pub arrival_s: f64,
    /// How often an arrival is bent into, and slid into, and how often a
    /// bend starts a whole step under where the mode has one.
    pub bend: f32,
    pub slide: f32,
    pub whole: f32,
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
    /// A slide off a phrase's end: how many semitones it falls, and over
    /// how long.
    pub fall: (i32, i32),
    pub fall_s: f64,
    /// The share of a tone held `wide_s` or longer its vibrato waits
    /// through, where the player's vibrato comes late in a long tone.
    pub terminal: f32,
    /// How often a phrase ends on its vibrato, bent up a step, and let
    /// fall.
    pub ends: [f32; 3],
    /// How much faster its vibrato is at a tone's end than at its start,
    /// and whether it dips under the tone first.
    pub accel: f32,
    pub dips: bool,
    /// A grace into an arrival from over it, a bend jump of `cents` held
    /// `s` seconds whatever the tempo, `share` of arrivals; none where it
    /// plays none.
    pub grace: Option<Grace>,
    /// A shake to the tone `cents` over a phrase's last tone, held
    /// `wide_s` or longer, `share` of the time: through its last moments
    /// only (`perform`'s `SHAKE_S`), quickening from half `rate_hz` to it,
    /// its vibrato before — what a player has the breath for.
    pub shake: Option<Shake>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grace {
    pub share: f32,
    pub cents: f32,
    pub s: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shake {
    pub share: f32,
    pub cents: f32,
    pub rate_hz: f32,
}

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
    ornaments: None,
    legato: None,
    force: 0.0,
    length: 1.0,
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
    ornaments: None,
    legato: None,
    force: 0.0,
    length: 1.0,
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
    ornaments: None,
    legato: None,
    force: 0.0,
    length: 1.0,
};

/// A lead guitarist, from players' transcriptions and the literature on
/// guitar technique (`proofs/research/lead-findings.md`,
/// `guitar-lead-findings.md`): a singer's time and touch, tight in a run,
/// its vibrato its own bend; about two notes in five ornamented, a bend
/// rising in sixty milliseconds to an eighth, from a whole step three
/// times in four; a phrase ending on its vibrato most often, bent up a
/// step or slid off and fading one time in seven each.
pub const LEAD_GUITARIST: Player = Player {
    name: "lead guitarist",
    run: Some(0.045),
    ornaments: Some(Ornaments {
        range: 12.0,
        arrival_s: 0.3,
        bend: 0.45,
        slide: 0.2,
        whole: 0.73,
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
        terminal: 0.0,
        ends: [5.0, 1.0, 1.0],
        accel: 1.0,
        dips: false,
        grace: None,
        shake: None,
    }),
    legato: Some(Legato { reach: 4, gap_s: 0.06, run: 2, share: 0.75, softer: 16 }),
    force: 0.0,
    length: 1.0,
    ..SINGER
};

/// A wind's or a horn's ornaments before its own: nothing bent or slid,
/// a vibrato of five and a half to six times a second quickening a sixth
/// by the tone's end, every phrase ending on it.
const PLAIN_WIND: Ornaments = Ornaments {
    range: 12.0,
    arrival_s: 0.25,
    bend: 0.0,
    slide: 0.0,
    whole: 0.0,
    release: 0.0,
    rise_s: (0.06, 0.12),
    slide_step_s: (0.03, 0.05),
    vibrato_from_s: (0.25, 0.3),
    vibrato_min_s: 0.35,
    rate_hz: (5.5, 6.0),
    narrow: (12.0, 18.0),
    wide: (18.0, 25.0),
    wide_s: 1.0,
    phrase_gap_s: 0.15,
    fall: (2, 5),
    fall_s: 0.18,
    terminal: 0.0,
    ends: [1.0, 0.0, 0.0],
    accel: 1.15,
    dips: false,
    grace: None,
    shake: None,
};

/// A blues saxophonist: scooping into a quarter of its accented tones
/// from a half or a whole step, falling off a sixth of its phrases two to
/// five semitones, a doit seldom; its vibrato dipping under the tone
/// first, from a quarter second — through a long tone's last two fifths
/// only — five and a half to six times a second; a downbeat late, its
/// bank's own wheel left alone.
pub const SAXOPHONIST: Player = Player {
    name: "saxophonist",
    lean: (10.0, 35.0),
    ornaments: Some(Ornaments { bend: 0.25, whole: 0.5, rise_s: (0.06, 0.14), narrow: (12.0, 20.0), wide: (18.0, 25.0), terminal: 0.6, ends: [0.79, 0.05, 0.16], dips: true, ..PLAIN_WIND }),
    ..SINGER
};

/// A blues harmonica player: dipping into a third of its tones from a
/// semitone under, bent down only, cutting a quarter of its phrases a
/// semitone flat; its vibrato only on a held, bent tone, and now and
/// then a shake to the next hole on a long peak; its bank's own wheel,
/// at the wrong rate, left alone.
pub const HARP_PLAYER: Player = Player {
    name: "harmonica player",
    lean: (10.0, 35.0),
    ornaments: Some(Ornaments {
        bend: 0.35,
        whole: 0.15,
        rise_s: (0.04, 0.09),
        rate_hz: (5.5, 6.5),
        narrow: (12.0, 20.0),
        vibrato_from_s: (0.2, 0.25),
        vibrato_min_s: 0.4,
        fall: (1, 1),
        fall_s: 0.18,
        ends: [0.75, 0.0, 0.25],
        accel: 1.0,
        shake: Some(Shake { share: 0.1, cents: 300.0, rate_hz: 7.0 }),
        ..PLAIN_WIND
    }),
    ..SINGER
};

/// A Harmon-muted trumpet: little vibrato and that late in a long tone, a
/// half-valve scoop now and then, a phrase's end glided down a little or
/// fallen off, a shake on a long peak.
pub const MUTED_TRUMPETER: Player = Player {
    name: "muted trumpeter",
    lean: (10.0, 30.0),
    ornaments: Some(Ornaments {
        bend: 0.1,
        whole: 0.3,
        rise_s: (0.08, 0.15),
        vibrato_from_s: (0.5, 0.7),
        vibrato_min_s: 0.6,
        rate_hz: (5.5, 6.5),
        narrow: (8.0, 12.0),
        wide: (10.0, 14.0),
        terminal: 0.5,
        fall: (2, 5),
        fall_s: 0.22,
        ends: [0.85, 0.0, 0.15],
        shake: Some(Shake { share: 0.08, cents: 250.0, rate_hz: 6.0 }),
        ..PLAIN_WIND
    }),
    ..SINGER
};

/// A zurna player: every tone of a third of a second or more shaking at
/// five and a half times a second, quickening by a third, wide; scooping
/// into half its phrase openings; graced from a step over now and then;
/// no falls, its phrases joined.
pub const ZURNACI: Player = Player {
    name: "zurna player",
    lean: (0.0, 5.0),
    slip: 8.0,
    ornaments: Some(Ornaments {
        bend: 0.15,
        whole: 0.3,
        rise_s: (0.06, 0.1),
        vibrato_from_s: (0.12, 0.15),
        vibrato_min_s: 0.3,
        rate_hz: (5.3, 5.7),
        narrow: (20.0, 30.0),
        wide: (28.0, 35.0),
        accel: 1.35,
        ends: [1.0, 0.0, 0.0],
        grace: Some(Grace { share: 0.15, cents: 200.0, s: 0.04 }),
        ..PLAIN_WIND
    }),
    ..SINGER
};

/// A village clarinettist, the older style's: a prall from the
/// semitone over on one arrival in ten — the wedding style's grace
/// between nearly every sixteenth is "a shapeless melodic mass" in it —
/// a scoop now and then, a phrase in ten ending on a short fall, no
/// trill; its vibrato quickening a third, wide on a held peak
/// (`ornaments-findings.md`).
pub const VILLAGE_CLARINET: Player = Player {
    name: "village clarinettist",
    lean: (0.0, 5.0),
    slip: 8.0,
    ornaments: Some(Ornaments {
        bend: 0.1,
        whole: 0.2,
        rise_s: (0.04, 0.1),
        vibrato_from_s: (0.18, 0.2),
        rate_hz: (5.0, 5.4),
        narrow: (15.0, 25.0),
        wide: (30.0, 40.0),
        accel: 1.35,
        fall: (1, 2),
        fall_s: 0.1,
        ends: [0.9, 0.0, 0.1],
        grace: Some(Grace { share: 0.1, cents: 100.0, s: 0.03 }),
        ..PLAIN_WIND
    }),
    ..SINGER
};

/// A village trumpeter: graced more slowly than a reed and seldom, a
/// rip up into a high held tone now and then, a phrase in ten fallen
/// off, no shake.
pub const VILLAGE_TRUMPET: Player = Player {
    name: "village trumpeter",
    lean: (0.0, 5.0),
    ornaments: Some(Ornaments {
        bend: 0.1,
        whole: 0.5,
        rise_s: (0.06, 0.12),
        vibrato_from_s: (0.22, 0.25),
        vibrato_min_s: 0.4,
        rate_hz: (5.5, 6.5),
        narrow: (15.0, 25.0),
        accel: 1.2,
        ends: [0.9, 0.0, 0.1],
        grace: Some(Grace { share: 0.08, cents: 200.0, s: 0.05 }),
        ..PLAIN_WIND
    }),
    ..SINGER
};

/// A pan flute player: a slow, wide vibrato from a third of a second,
/// scooping into a third of its held tones, a third of its phrases
/// drooping off.
pub const PAN_PIPER: Player = Player {
    name: "pan flute player",
    ornaments: Some(Ornaments {
        bend: 0.3,
        whole: 0.3,
        rise_s: (0.1, 0.25),
        vibrato_from_s: (0.3, 0.35),
        vibrato_min_s: 0.5,
        rate_hz: (5.0, 6.0),
        narrow: (25.0, 35.0),
        wide: (30.0, 35.0),
        accel: 1.1,
        fall: (1, 2),
        fall_s: 0.3,
        ends: [0.7, 0.0, 0.3],
        grace: Some(Grace { share: 0.08, cents: 200.0, s: 0.06 }),
        ..PLAIN_WIND
    }),
    ..SINGER
};

/// An English horn player: a narrow vibrato, slow to come, a tone seldom
/// graced or scooped.
pub const OBOIST: Player = Player {
    name: "English horn player",
    ornaments: Some(Ornaments {
        bend: 0.1,
        whole: 0.0,
        rise_s: (0.04, 0.06),
        vibrato_from_s: (0.25, 0.3),
        vibrato_min_s: 0.45,
        rate_hz: (5.0, 5.5),
        narrow: (10.0, 18.0),
        wide: (14.0, 18.0),
        accel: 1.07,
        grace: Some(Grace { share: 0.05, cents: 200.0, s: 0.08 }),
        ..PLAIN_WIND
    }),
    ..SINGER
};

/// A folk flute or fiddle player, whose bank's tones carry a vibrato
/// recorded in them, so it adds none: an arrival cut from a step or two
/// over one time in eight, and seldom scooped.
pub const PIPER: Player = Player {
    name: "piper",
    ornaments: Some(Ornaments {
        bend: 0.08,
        whole: 0.0,
        rise_s: (0.06, 0.12),
        vibrato_min_s: f64::INFINITY,
        ends: [1.0, 0.0, 0.0],
        accel: 1.0,
        grace: Some(Grace { share: 0.12, cents: 200.0, s: 0.045 }),
        ..PLAIN_WIND
    }),
    ..SINGER
};

/// A village fiddler, whose bank's tones carry their own vibrato: a
/// prall from the semitone over on one arrival in ten, a phrase in ten
/// ending on a short glissando down.
pub const VILLAGE_FIDDLE: Player = Player {
    name: "village fiddler",
    lean: (0.0, 8.0),
    ornaments: Some(Ornaments {
        vibrato_min_s: f64::INFINITY,
        fall: (1, 2),
        fall_s: 0.1,
        ends: [0.9, 0.0, 0.1],
        accel: 1.0,
        grace: Some(Grace { share: 0.1, cents: 100.0, s: 0.03 }),
        ..PLAIN_WIND
    }),
    ..SINGER
};

/// A melody on a bank that carries its own vibrato, or on one that does
/// not sing: a singer's time and touch, its tones as they sound.
pub const PLAIN: Player = Player { name: "plain", ..SINGER };

/// A lead no style has cast: a singer's time and touch, a vibrato of its
/// own from a quarter second, no gesture.
pub const PLAIN_LEAD: Player = Player { name: "lead", ornaments: Some(PLAIN_WIND), ..SINGER };

/// A wind's or a horn's second voice, holding tones under the lead or
/// taking a phrase it hands over: a plain tone, a narrow vibrato through
/// a long one's last half, none of the lead's gestures — a section
/// player's way, so the lead is the one heard shaping its line.
pub const SECOND: Player = Player {
    name: "second",
    ornaments: Some(Ornaments { narrow: (6.0, 10.0), wide: (8.0, 12.0), vibrato_from_s: (0.5, 0.7), vibrato_min_s: 0.8, terminal: 0.5, accel: 1.0, ..PLAIN_WIND }),
    ..SINGER
};

/// A clarinettist in a slow folk tune: a straight tone, a grace now and
/// then.
pub const FOLK_CLARINET: Player = Player {
    name: "clarinettist",
    ornaments: Some(Ornaments { vibrato_min_s: f64::INFINITY, grace: Some(Grace { share: 0.05, cents: 200.0, s: 0.05 }), ..PLAIN_WIND }),
    ..SINGER
};

/// A line doubling another's in unison: its vibrato narrow and steady,
/// none of the lead's gestures, as the twin guitar keeps its line plain.
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
    ornaments: Some(Ornaments { narrow: (8.0, 12.0), wide: (8.0, 12.0), vibrato_from_s: (0.25, 0.25), vibrato_min_s: 0.4, accel: 1.0, ..PLAIN_WIND }),
    legato: None,
    force: 0.0,
    length: 1.0,
};

/// A line doubling another's on a bank that carries its own vibrato, or
/// on one that does not sing.
pub const PLAIN_DOUBLER: Player = Player { name: "doubler", ornaments: None, ..DOUBLER };

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
    ornaments: None,
    legato: None,
    force: 0.0,
    length: 1.0,
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
    ornaments: None,
    legato: None,
    force: 0.0,
    length: 1.0,
};

/// The player a melody on `program` is played by where its score casts
/// none: the overdriven and the distorted guitar by a lead guitarist,
/// whoever's line it is, since a guitar melody is a soloist's; a bank
/// that carries its own vibrato, or a tone that is not shaped, plainly; a
/// wind or a horn as the lead where it is the `lead`, and as a second
/// voice where it is not. A style casts its own lead (`Score::cast`).
fn melody(program: u8, lead: bool) -> &'static Player {
    match program {
        29 | 30 => &LEAD_GUITARIST,
        _ if voices::vibrato_recorded(program) || !voices::sings(program) => &PLAIN,
        _ if lead => &PLAIN_LEAD,
        _ => &SECOND,
    }
}

/// The player a part of `role` on `program` is played by where its score
/// casts none; `lead` where the part is the score's lead.
pub fn of(role: Role, program: u8, lead: bool) -> &'static Player {
    match role {
        Role::Percussion => &DRUMMER,
        Role::Pluck => &PICKER,
        Role::Melody => melody(program, lead),
        Role::Doubling if voices::sings(program) && !voices::vibrato_recorded(program) => &DOUBLER,
        Role::Doubling => &PLAIN_DOUBLER,
        Role::Sustain => &SECTION,
        Role::Drone => &DRONE,
    }
}
