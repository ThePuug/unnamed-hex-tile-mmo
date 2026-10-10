//! The teaser: the overworld told once, from dawn to a title, as a cue
//! cut to picture. Its vocabulary is the overworld's — the lesnoto, a
//! tonic drone, the modal schemata, one storyteller, a breathy choir in
//! a stone hall — and what makes it a cue is its form: movements that
//! start where the picture cuts (`cue`), a density that fills, turns,
//! thins to night, drives and swells, a loudness arc each movement
//! declares and the render meets, and a close that lands on the tonic
//! and rings out.
//!
//! Every choice is a draw from the seed's stream, one fork per purpose;
//! the seed picks the key, the storyteller, the theme and the players,
//! and the cuts pick the tempo. It never picks the next note.

use crate::cue::{self, Cut};
use crate::ladder::{Part, Walk};
use crate::band;
use crate::pieces::Params;
use crate::rng::Rng;
use crate::score::{Instrument, Note, Role, Score, Section, TICKS_PER_EIGHTH as E};
use crate::teller::{self, Hold, Teller, Telling};
use crate::theory::groove::{Groove, BALKAN};
use crate::theory::melody::{Theme, FOLK_SHAPES};
use crate::theory::phrase::{self, Form as PhraseForm};
use crate::theory::schema::{schemata_for, Schema};
use crate::theory::{clashes, counterpoint, Chord, Key, Mode, DIATONIC};
use crate::variation::{self, Role as Bar};
use crate::tune::{self, Tune};

/// General MIDI programs, 0-based.
const CELLO: u8 = 42;
const CONTRABASS: u8 = 43;
const STRINGS_2: u8 = 49;
const FLUTE: u8 = 73;
const PAN_FLUTE: u8 = 75;
const ENGLISH_HORN: u8 = 69;
const FIDDLE: u8 = 110;
const CLARINET: u8 = 71;
const ACCORDION: u8 = 21;
const NYLON_GUITAR: u8 = 24;
const PIZZICATO: u8 = 45;
const DULCIMER: u8 = 15;
const CHOIR_AAHS: u8 = 52;
const VOICE_OOHS: u8 = 53;
const HALO_PAD: u8 = 94;
const FRENCH_HORN: u8 = 60;
const TIMPANI: u8 = 47;
const TAIKO: u8 = 116;
/// Drum keys on General MIDI's percussion channel: the low conga and
/// the high bongo stand in for a frame drum's dum and tek; the kick and
/// the crash sound once, under the title.
const DUM: u8 = 64;
const TEK: u8 = 60;
const KICK: u8 = 36;
const CRASH: u8 = 49;

const CH_DRONE: u8 = 0;
const CH_PAD: u8 = 1;
const CH_LEAD: u8 = 2;
const CH_PLUCK: u8 = 3;
const CH_SECOND: u8 = 4;
const CH_CHOIR: u8 = 5;
const CH_HORN: u8 = 6;
const CH_DOUBLE: u8 = 7;
const CH_TAIKO: u8 = 8;
const CH_DRUM: u8 = 9;
const CH_PLUCK_2: u8 = 10;
const CH_WEAVE: u8 = 11;
const CH_TIMPANI: u8 = 12;
const CH_HORNS: u8 = 13;
const CH_BREATH: u8 = 14;
const CH_BREATH_AIR: u8 = 15;

/// The overworld's stone hall.
const ROOM_S: f32 = 3.2;

/// Who may tell the tune: the overworld's storytellers, each at the
/// overworld's one level forward of the band, since the render evens
/// what the bank gives each.
const LEADS: [u8; 5] = [FLUTE, PAN_FLUTE, FIDDLE, CLARINET, ENGLISH_HORN];
const LEAD: f32 = 3.0;

/// Who may hold tones under the lead's riff.
const SECONDS: [u8; 4] = [FIDDLE, CLARINET, ACCORDION, ENGLISH_HORN];

/// The horns' level, dB: the bank's horn is the loudest of its players
/// at one velocity, and where two of them hold at the swell they alone
/// set the cue's peak.
const HORN_LEVEL: f32 = -4.0;

/// The frame drum's level, dB: its most struck stroke's, under the
/// band by what holds it where the bed's balance has it.
const DRUM_LEVEL: f32 = -8.0;

/// The velocity every voice strikes at before its own accent and its
/// movement's dynamic.
const VEL: i32 = 85;

/// The tune's register, the overworld's: the lead and the second take
/// it here, the echo an octave under, the horns an octave under at the
/// swell.
const TUNE: (u8, u8) = (62, 88);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Pad {
    Off,
    /// The top voice alone, rocking through the chord.
    Thin,
    Full,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Plucks {
    Off,
    /// The bass alone, on the dance's strong beats.
    Accents,
    /// The chord too, on the dance's strikes.
    Full,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Drum {
    Off,
    /// The dum alone: the pulse.
    Accents,
    Full,
}

/// What plays: each layer at its notch.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Texture {
    /// The breathy choir on the chord's root and fifth, far back.
    breath: bool,
    pad: Pad,
    plucks: Plucks,
    drum: Drum,
    /// The taiko under the dum: the stroke the work is struck on.
    taiko: bool,
    /// The dulcimer echoing the tune a bar behind.
    weave: bool,
    /// The choir on root and fifth and the horn on the third.
    colours: bool,
    /// The dulcimer high and soft over the night.
    moon: bool,
    /// The horns on the tune an octave under the lead.
    crest: bool,
    /// The timpani on the dum and the taiko on every group: the swell's
    /// peak.
    timpani: bool,
}

const BARE: Texture = Texture { breath: false, pad: Pad::Off, plucks: Plucks::Off, drum: Drum::Off, taiko: false, weave: false, colours: false, moon: false, crest: false, timpani: false };
const AIR: Texture = Texture { breath: true, ..BARE };
const VISTA: Texture = Texture { breath: true, pad: Pad::Full, weave: true, plucks: Plucks::Accents, ..BARE };
const NIGHT: Texture = Texture { pad: Pad::Thin, moon: true, drum: Drum::Accents, ..AIR };
const WORK: Texture = Texture { breath: true, pad: Pad::Full, plucks: Plucks::Full, drum: Drum::Full, taiko: true, ..BARE };
const ALL: Texture = Texture { colours: true, crest: true, timpani: true, ..WORK };

use Telling::{Long, Off, Phrases, Riff, RiffAndLong};

/// A stretch of the cue the picture cuts to: where it is cut, the level
/// it sits at against the swell, the dynamic its notes take from its
/// head to its end, the tune's shift in degrees for each phrase that
/// opens in it, the last held, and its parts, which share its
/// half-phrases, the last the longest.
struct Movement {
    name: &'static str,
    cut: Cut,
    level: f32,
    dynamic: (i32, i32),
    shifts: &'static [i32],
    parts: &'static [(Texture, Telling)],
}

/// The cue: dawn out of the drone, the storyteller alone; the bed
/// joining a layer a part under the tune;
/// the harmony turning dark as a soft pulse comes in, the tune sunk a
/// step, thinning to the lone string and the moon; the work on the
/// dance, the tune as a riff, building; the swell, all of it,
/// the tune sung a third up, growing to its end; and the title, the
/// tonic struck by everyone and let ring.
const MOVEMENTS: [Movement; 5] = [
    Movement { name: "dawn", cut: Cut { at: 0.0, phrase: true, least: 3 }, level: -17.0, dynamic: (-12, -8), shifts: &[0], parts: &[(BARE, Off), (AIR, Off), (AIR, Phrases)] },
    Movement {
        name: "vistas",
        cut: Cut { at: 15.0, phrase: false, least: 4 },
        level: -10.0,
        dynamic: (-6, -3),
        // At home: the register's top is the swell's to reach first.
        shifts: &[0],
        // The storyteller breathes while the echo carries the tune, and
        // comes back over the whole bed: a tune said without a rest for a
        // hundred seconds is a figure, not a story.
        parts: &[(Texture { pad: Pad::Thin, ..AIR }, Phrases), (Texture { pad: Pad::Full, ..AIR }, Phrases), (Texture { plucks: Plucks::Off, ..VISTA }, Long), (VISTA, Phrases)],
    },
    Movement {
        name: "day into night",
        cut: Cut { at: 40.0, phrase: true, least: 4 },
        level: -12.0,
        dynamic: (-5, -11),
        shifts: &[-1],
        // The tune on the lament as the pulse comes in, the bed thinning
        // under its answer, and the night a whole phrase: one string, the
        // heartbeat, the moon, the storyteller holding.
        parts: &[(Texture { drum: Drum::Accents, ..VISTA }, Phrases), (Texture { drum: Drum::Accents, weave: false, plucks: Plucks::Off, ..VISTA }, Phrases), (NIGHT, Long), (NIGHT, Long)],
    },
    Movement {
        name: "work",
        cut: Cut { at: 60.0, phrase: true, least: 4 },
        // Under the swell by enough that the swell arrives: the work
        // builds to it and the drop before it, not to its height.
        level: -6.0,
        // A crescendo through the work: the band strikes harder as it
        // builds, where a level held would leave the build to the
        // layers alone.
        dynamic: (-6, 4),
        // At home: the tune's height above the vistas' is the swell's to
        // reach first.
        shifts: &[0],
        // The groove and the riff on the cut — the drum, the bass, the
        // taiko's stroke, the tune detached — and the band filling after.
        parts: &[
            (Texture { pad: Pad::Thin, plucks: Plucks::Accents, ..WORK }, Riff),
            (Texture { pad: Pad::Thin, ..WORK }, Riff),
            (WORK, RiffAndLong),
            (Texture { weave: true, colours: true, ..WORK }, RiffAndLong),
        ],
    },
    Movement { name: "swell", cut: Cut { at: 95.0, phrase: true, least: 2 }, level: 0.0, dynamic: (4, 10), shifts: &[2], parts: &[(Texture { timpani: false, ..ALL }, Phrases), (ALL, Phrases)] },
];

/// Where the title falls, the tune's end, and how long it rings in the
/// score before the room takes it.
const TITLE_AT: f32 = 115.0;
const TITLE_BARS: u32 = 2;
const TITLE_DYNAMIC: i32 = 4;
/// The title's level against the swell, over the hit and its ring: a
/// ring falling away through the section puts the hit some four LU over
/// its mean, so the hit lands at the swell's height out of the drop
/// before it and no higher, since the swell is the crest and the title
/// its answer.
const TITLE_LEVEL: f32 = -4.0;

/// The dawn's row: the tonic held, the drone's.
const TONIC: Schema = Schema { name: "tonic", modes: &[Mode::Aeolian], roots: [0, 0, 0, 0], closed: false, alters: [DIATONIC; 4] };
/// The night's turn, down the lament to the minor fifth, and its close
/// through the minor fourth.
const LAMENT: Schema = Schema { name: "lament", modes: &[Mode::Aeolian], roots: [0, 6, 5, 4], closed: false, alters: [DIATONIC; 4] };
const PLAGAL: Schema = Schema { name: "plagal close", modes: &[Mode::Aeolian], roots: [0, 3, 3, 0], closed: true, alters: [DIATONIC; 4] };
/// The swell's last row, up through the minor fourth to the flat
/// seventh and held there, open, so the title's tonic is its answer:
/// the folk's way home from ♭VII, never the ballad's through ♭VI.
const RISE: Schema = Schema { name: "rise", modes: &[Mode::Aeolian], roots: [0, 3, 6, 6], closed: false, alters: [DIATONIC; 4] };

struct Design {
    drone: u8,
    lead: u8,
    second: u8,
    pluck: u8,
    pluck_2: u8,
    groove: &'static Groove,
    form: PhraseForm,
    theme: Theme,
    open: &'static Schema,
    closed: &'static Schema,
}

fn vel(accent: i32, rng: &mut Rng) -> u8 {
    (VEL + accent + rng.range(-4, 4)).clamp(1, 127) as u8
}

struct Form {
    bar: u32,
    walk: Walk<Texture, Telling>,
    tune: Tune,
    design: Design,
    /// The first bar of each movement, and the title's.
    starts: Vec<u32>,
}

impl Form {
    fn texture_at(&self, bar: u32) -> Texture {
        self.walk.bed_at(bar)
    }
    fn chord(&self, bar: u32) -> Chord {
        self.tune.chords[bar as usize]
    }
    /// Bars of the tune, the title after them.
    fn bars(&self) -> u32 {
        self.walk.bars()
    }
    fn movement(&self, bar: u32) -> usize {
        self.starts.iter().rposition(|s| *s <= bar).unwrap()
    }
}

pub fn build(params: &Params) -> Score {
    let rng = Rng::new(params.seed);
    let mut skeleton = rng.fork(0);
    // The Aeolian: the rise to the title is its flat seventh.
    let key = Key::new(["D", "E", "G", "A", "C"][skeleton.weighted(&[3.0, 2.0, 2.0, 2.0, 1.0])], Mode::Aeolian);
    // The lesnoto, the overworld's own dance.
    let groove = &BALKAN[0];
    let (open, closed) = schemata_for(&key);
    // The band, as the overworld's: its drone, its lead and second, its
    // plucks and its drum; the cue's orchestra is the cue's own.
    let band = params.band;
    let lead = band.program(band::Part::Lead, &LEADS, LEADS[0]);
    let design = Design {
        drone: band.program(band::Part::Drone, &[], CONTRABASS),
        lead,
        second: band.programs(band::Part::Second).into_iter().chain(SECONDS).find(|p| *p != lead && SECONDS.contains(p)).unwrap_or(SECONDS[0]),
        pluck: band.program(band::Part::Pluck, &[], NYLON_GUITAR),
        pluck_2: band.program(band::Part::Pluck2, &[], NYLON_GUITAR),
        groove,
        // A sentence, never a period: a cue moves on, and a period says
        // its two bars twice.
        form: PhraseForm::Sentence,
        theme: Theme::draw(groove, &FOLK_SHAPES, &mut skeleton),
        open: open[skeleton.below(open.len())],
        closed: closed[skeleton.below(closed.len())],
    };

    // The cuts set the tempo and the half-phrase each movement opens on.
    let mut cuts: Vec<Cut> = MOVEMENTS.iter().map(|m| m.cut).collect();
    cuts.push(Cut { at: TITLE_AT, phrase: true, least: 0 });
    let (tempo, halves) = cue::fit(&cuts, groove.tempo, groove.eighths(), phrase::BARS / 2);

    let instruments = vec![
        Instrument { name: "drone", program: design.drone, channel: CH_DRONE, role: Role::Drone, low: 24, high: 60, reverb: 40, pan: 0, level: 0.0 },
        Instrument { name: "strings", program: STRINGS_2, channel: CH_PAD, role: Role::Sustain, low: 48, high: 79, reverb: 105, pan: -29, level: 0.0 },
        Instrument { name: "lead", program: design.lead, channel: CH_LEAD, role: Role::Melody, low: 62, high: 91, reverb: 40, pan: 0, level: LEAD },
        Instrument { name: "pluck", program: design.pluck, channel: CH_PLUCK, role: Role::Pluck, low: 45, high: 74, reverb: 30, pan: 34, level: 0.0 },
        Instrument { name: "second", program: design.second, channel: CH_SECOND, role: Role::Melody, low: 55, high: 88, reverb: 60, pan: 21, level: 0.0 },
        Instrument { name: "choir", program: CHOIR_AAHS, channel: CH_CHOIR, role: Role::Sustain, low: 55, high: 72, reverb: 100, pan: 23, level: 0.0 },
        Instrument { name: "horn", program: FRENCH_HORN, channel: CH_HORN, role: Role::Sustain, low: 48, high: 67, reverb: 55, pan: -39, level: HORN_LEVEL },
        Instrument { name: "strings, the riff", program: STRINGS_2, channel: CH_DOUBLE, role: Role::Doubling, low: 55, high: 88, reverb: 60, pan: -34, level: 0.0 },
        Instrument { name: "taiko", program: TAIKO, channel: CH_TAIKO, role: Role::Percussion, low: 36, high: 36, reverb: 45, pan: -12, level: 0.0 },
        Instrument { name: "frame drum", program: band.program(band::Part::Drums, &[], 0), channel: CH_DRUM, role: Role::Percussion, low: KICK, high: DUM, reverb: 30, pan: -8, level: DRUM_LEVEL },
        Instrument { name: "pluck 2", program: design.pluck_2, channel: CH_PLUCK_2, role: Role::Pluck, low: 55, high: 72, reverb: 30, pan: -44, level: 0.0 },
        Instrument { name: "weave", program: DULCIMER, channel: CH_WEAVE, role: Role::Pluck, low: 50, high: 91, reverb: 50, pan: -52, level: 0.0 },
        Instrument { name: "timpani", program: TIMPANI, channel: CH_TIMPANI, role: Role::Pluck, low: 40, high: 55, reverb: 60, pan: 14, level: 0.0 },
        Instrument { name: "horns, the tune", program: FRENCH_HORN, channel: CH_HORNS, role: Role::Doubling, low: 48, high: 79, reverb: 70, pan: 30, level: HORN_LEVEL },
        Instrument { name: "breath", program: VOICE_OOHS, channel: CH_BREATH, role: Role::Sustain, low: 48, high: 67, reverb: 110, pan: -42, level: 0.0 },
        Instrument { name: "breath, the air", program: HALO_PAD, channel: CH_BREATH_AIR, role: Role::Sustain, low: 48, high: 67, reverb: 120, pan: 42, level: 0.0 },
    ];
    let mut score = Score::new(key, groove.meter(), tempo, instruments, ROOM_S);
    score.lead = Some(CH_LEAD);
    score.played_by(
        band,
        &[(CH_DRONE, band::Part::Drone), (CH_LEAD, band::Part::Lead), (CH_PLUCK, band::Part::Pluck), (CH_SECOND, band::Part::Second), (CH_DRUM, band::Part::Drums), (CH_PLUCK_2, band::Part::Pluck2)],
    );
    let bar = score.bar();
    let bars_a_half = phrase::BARS / 2;

    // Each movement a section at its level; its parts share its halves.
    let mut parts = Vec::new();
    let mut starts = Vec::new();
    for (m, movement) in MOVEMENTS.iter().enumerate() {
        let (a, b) = (halves[m] * bars_a_half, halves[m + 1] * bars_a_half);
        starts.push(a);
        score.sections.push(Section { name: movement.name, start: a * bar, end: b * bar, trim: 1.0, level: Some(movement.level), rings: false });
        let mut at = a;
        for (&(texture, lead), n) in movement.parts.iter().zip(cue::spread(halves[m + 1] - halves[m], movement.parts.len())) {
            parts.push(Part { rung: 0, bed: texture, lead, a: at, b: at + n * bars_a_half });
            at += n * bars_a_half;
        }
    }
    let walk = Walk { parts };
    let tune_bars = walk.bars();
    starts.push(tune_bars);
    score.sections.push(Section { name: "title", start: tune_bars * bar, end: (tune_bars + TITLE_BARS) * bar, trim: 1.0, level: Some(TITLE_LEVEL), rings: true });

    // A row a phrase: the dawn's tonic, the night's lament and its close,
    // the swell's rise last, and between them question and answer by turns.
    let phrases = (tune_bars / phrase::BARS) as usize;
    let movement_of = |bar: u32| starts.iter().rposition(|s| *s <= bar).unwrap();
    let mut asked = false;
    let rows: Vec<&Schema> = (0..phrases)
        .map(|p| {
            let at = p as u32 * phrase::BARS;
            let m = movement_of(at);
            if p == 0 {
                &TONIC
            } else if p + 1 == phrases {
                &RISE
            } else if MOVEMENTS[m].name == "day into night" {
                if ((at - starts[m]) / phrase::BARS).is_multiple_of(2) { &LAMENT } else { &PLAGAL }
            } else {
                asked = !asked;
                if asked { design.open } else { design.closed }
            }
        })
        .collect();
    // Each phrase's shift, from the movement it opens in, its k-th phrase
    // there taking its k-th shift, the last held.
    let mut shifts: Vec<i32> = Vec::new();
    let mut opened = [0usize; MOVEMENTS.len()];
    for p in 0..phrases as u32 {
        let m = movement_of(p * phrase::BARS);
        let list = MOVEMENTS[m].shifts;
        shifts.extend([list[opened[m].min(list.len() - 1)]; phrase::BARS as usize]);
        opened[m] += 1;
    }
    let tune = Tune::compose(&[&design.theme], &score.meter, design.form, &rows, 3, shifts);
    score.harmony = tune.chords.clone();
    score.harmony.extend(std::iter::repeat_n(Chord::triad(0), TITLE_BARS as usize));

    let name = |program: u8| match program {
        CELLO => "cello",
        CONTRABASS => "contrabass",
        FLUTE => "flute",
        PAN_FLUTE => "pan flute",
        ENGLISH_HORN => "English horn",
        FIDDLE => "fiddle",
        CLARINET => "clarinet",
        ACCORDION => "accordion",
        NYLON_GUITAR => "nylon guitar",
        PIZZICATO => "pizzicato",
        DULCIMER => "dulcimer",
        _ => "?",
    };
    score.summary = format!(
        "cue on the {} at {tempo}: a {:?} in a {:?}, {} to ask and {} to answer; {} drone, {} lead, {} second, {} and {} plucks",
        groove.name,
        design.theme.shape,
        design.form,
        design.open.name,
        design.closed.name,
        name(design.drone),
        name(design.lead),
        name(design.second),
        name(design.pluck),
        name(design.pluck_2),
    );

    let form = Form { bar, walk, tune, design, starts };
    drone(&mut score, &form);
    breath(&mut score, &form, &mut rng.fork(12));
    pad(&mut score, &form, &mut rng.fork(2));
    plucks(&mut score, &form, &mut rng.fork(3));
    colours(&mut score, &form, &mut rng.fork(5));
    moon(&mut score, &form, &mut rng.fork(4));
    weave(&mut score, &form, &mut rng.fork(9));
    let teller = Teller {
        lead: CH_LEAD,
        second: CH_SECOND,
        double: Some((CH_DOUBLE, -20)),
        echo: CH_WEAVE,
        register: TUNE,
        sung: -10,
        riff: (4, -4),
        long: (72, 84, -18),
        under: (62, 76, -18),
        hold: Hold::Breathing,
        breathes: false,
        vel,
        fills: 0.0,
        soars: 0.0,
        pushes: 0.0,
    };
    teller::tell(&mut score, &teller, &form.tune, &form.walk.runs(), &mut rng.fork(8));
    horns(&mut score, &form);
    drums(&mut score, &form, &mut rng.fork(10));
    linger(&mut score);
    // The drops before the arrivals: before the swell the band stops after
    // the bar's first group but for the drum's fill; before the title
    // everything stops at the bar line but the timpani's roll and the
    // lead, holding its tone into the hit: a whole bar of space for the
    // hit to land out of.
    let first_group = score.meter.groups[0] as u32 * E;
    hush(&mut score, form.starts[4] - 1, first_group, &[CH_DRONE, CH_BREATH, CH_BREATH_AIR, CH_DRUM]);
    fill(&mut score, &form, first_group, &mut rng.fork(14));
    hush(&mut score, form.bars() - 1, 0, &[CH_DRONE, CH_BREATH, CH_BREATH_AIR, CH_LEAD, CH_TIMPANI]);
    suspend(&mut score, form.bars() - 1);
    // The title after the drop is shaped, so its lead lands from the tone
    // the drop holds.
    title(&mut score, &form, &mut rng.fork(13));
    dynamics(&mut score, &form);
    score.finish();
    score
}

/// Every note takes its movement's dynamic where it falls, from the
/// movement's head to its end: the players strike harder as the cue
/// grows, which a bank sounds as brighter and not only louder; the
/// level each movement declares is then met by the pedal.
fn dynamics(score: &mut Score, form: &Form) {
    let bar = form.bar;
    for n in &mut score.notes {
        let m = form.movement(n.start / bar);
        let dynamic = MOVEMENTS.get(m).map_or(TITLE_DYNAMIC, |mv| {
            let (a, b) = (form.starts[m] * bar, form.starts[m + 1] * bar);
            let x = (n.start - a) as f32 / (b - a) as f32;
            (mv.dynamic.0 as f32 + (mv.dynamic.1 - mv.dynamic.0) as f32 * x).round() as i32
        });
        n.vel = (n.vel as i32 + dynamic).clamp(1, 127) as u8;
    }
}

/// The tonic in two octaves under everything, struck once at the head
/// and held to the end, the title's last tone.
fn drone(score: &mut Score, form: &Form) {
    let end = (form.bars() + TITLE_BARS) * form.bar;
    for pitch in [score.key.pitch(0, 2), score.key.pitch(0, 3)] {
        score.add(Note { start: 0, len: end, pitch, vel: 54, channel: CH_DRONE });
    }
}

/// The breathy choir, the root and fifth of the chord that stands, low,
/// a bar at a time and carried where the chord keeps them, on the oohs
/// with the halo pad's air under them; from the dawn's first light to
/// the title's end.
fn breath(score: &mut Score, form: &Form, rng: &mut Rng) {
    let drone = [score.key.pitch(0, 2), score.key.pitch(0, 3)];
    let end = (form.bars() + TITLE_BARS) * form.bar;
    for b in 0..form.bars() + TITLE_BARS {
        if b < form.bars() && !form.texture_at(b).breath {
            continue;
        }
        let chord = score.harmony[b as usize];
        let (root, fifth) = root_and_fifth(&score.key, chord, 50, 64);
        for p in std::iter::once(root).chain(fifth) {
            let p = if drone.iter().any(|d| clashes(p, *d)) { p + 12 } else { p };
            let len = (form.bar + E / 2).min(end - b * form.bar);
            score.hold(Note { start: b * form.bar, len, pitch: p, vel: vel(-34, rng), channel: CH_BREATH });
            score.hold(Note { start: b * form.bar, len, pitch: p, vel: vel(-40, rng), channel: CH_BREATH_AIR });
        }
    }
}

/// The chord's root within `lo..=hi`, and its fifth above it where that
/// fits.
fn root_and_fifth(key: &Key, chord: Chord, lo: u8, hi: u8) -> (u8, Option<u8>) {
    let tones = chord.pitches_within(key, lo, hi);
    let root = *tones.iter().find(|p| key.degree_of(**p) == Some(chord.root.rem_euclid(7) as usize)).unwrap();
    let fifth = tones.iter().find(|p| key.degree_of(**p) == Some((chord.root + 4).rem_euclid(7) as usize) && **p > root).copied();
    (root, fifth)
}

/// The strings on slow bows: three voices of the chord, each to the
/// nearest tone of the next, or, thin, the top voice alone and softer,
/// rocking through the chord while it stands. Every note lingers half
/// an eighth into the next chord; `linger` carries the consonant ones
/// further.
fn pad(score: &mut Score, form: &Form, rng: &mut Rng) {
    let (lo, hi) = (55, 72);
    let nearest = |candidates: &[u8], to: u8| *candidates.iter().min_by_key(|c| (**c as i32 - to as i32).abs()).unwrap();
    let mut voices: Option<Vec<u8>> = None;
    let mut alone: Option<u8> = None;
    let mut rising = true;
    for b in 0..form.bars() {
        let mode = form.texture_at(b).pad;
        if mode == Pad::Off {
            voices = None;
            alone = None;
            rising = true;
            continue;
        }
        let chord = form.chord(b);
        let mut candidates = chord.pitches_within(&score.key, lo, hi);
        if mode == Pad::Thin {
            let stands = b > 0 && form.chord(b - 1) == chord;
            let v = match alone {
                None => voices.as_ref().map_or_else(|| candidates[2.min(candidates.len() - 1)], |v| nearest(&candidates, *v.iter().max().unwrap())),
                Some(a) if stands => {
                    let up = |a: u8| candidates.iter().copied().find(|c| *c > a);
                    let down = |a: u8| candidates.iter().copied().rev().find(|c| *c < a);
                    match if rising { up(a) } else { down(a) } {
                        Some(step) => step,
                        None => {
                            rising = !rising;
                            (if rising { up(a) } else { down(a) }).unwrap_or(a)
                        }
                    }
                }
                Some(a) => nearest(&candidates, a),
            };
            score.hold(Note { start: b * form.bar, len: form.bar + E / 2, pitch: v, vel: vel(-26, rng), channel: CH_PAD });
            alone = Some(v);
            voices = None;
            continue;
        }
        let next: Vec<u8> = match (&voices, alone) {
            (Some(prev), _) => prev
                .iter()
                .map(|v| {
                    let (i, p) = candidates.iter().enumerate().min_by_key(|(_, c)| (**c as i32 - *v as i32).abs()).map(|(i, c)| (i, *c)).unwrap();
                    candidates.remove(i);
                    p
                })
                .collect(),
            (None, Some(a)) => {
                let top = nearest(&candidates, a);
                let mut under: Vec<u8> = candidates.iter().copied().filter(|c| *c < top).rev().take(2).collect();
                if under.len() < 2 {
                    under.extend(candidates.iter().copied().filter(|c| *c > top).take(2 - under.len()));
                }
                under.push(top);
                under
            }
            (None, None) => candidates.iter().copied().take(3).collect(),
        };
        for p in &next {
            score.hold(Note { start: b * form.bar, len: form.bar + E / 2, pitch: *p, vel: vel(-18, rng), channel: CH_PAD });
        }
        voices = Some(next);
        alone = None;
    }
}

/// How far a string's tone carries into the next chord past the change.
const LINGER: u32 = 2 * E;

/// Carries every string tone that ends half an eighth into the next
/// chord `LINGER` into it, unless it would rub a held voice sounding
/// there.
fn linger(score: &mut Score) {
    let bar = score.bar();
    let end = score.end();
    let held = |score: &Score, i: usize, from: u32, to: u32| -> bool {
        let n = score.notes[i];
        score.notes.iter().enumerate().any(|(j, m)| j != i && matches!(score.instrument(m.channel).role, Role::Drone | Role::Sustain) && m.start < to && m.end() > from && clashes(n.pitch, m.pitch))
    };
    for i in 0..score.notes.len() {
        let n = score.notes[i];
        let change = n.end() - E / 2;
        if n.channel != CH_PAD || !change.is_multiple_of(bar) || change + LINGER >= end {
            continue;
        }
        if !held(score, i, n.end(), change + LINGER) {
            score.notes[i].len = change + LINGER - n.start;
        }
    }
}

/// The chord's tone the bass takes over the drone: its root where that
/// is a unison, fourth or fifth from the tonic, else its fifth, else its
/// third.
fn bass_degree(chord: Chord) -> i32 {
    let consonant = |d: i32| matches!(d.rem_euclid(7), 0 | 3 | 4);
    [chord.root, chord.root + 4, chord.root + 2].into_iter().find(|d| consonant(*d)).unwrap_or(chord.root)
}

/// The bass pitch of `chord`, an octave clear of the drone.
fn bass(key: &Key, chord: Chord) -> u8 {
    let floor = key.pitch(0, 3) + 2;
    let degree = bass_degree(chord);
    *chord.pitches_within(key, floor, floor + 12).iter().find(|p| key.degree_of(**p) == Some(degree.rem_euclid(7) as usize)).unwrap()
}

/// The plucks on the dance: the bass on every strong beat, and the
/// chord's other tones on the dance's strikes an octave up; under the
/// bed's level, a strike at it cuts in, and the bass on the bar line
/// under the band's, where it lands with the drums and is the peak.
fn plucks(score: &mut Score, form: &Form, rng: &mut Rng) {
    let strong = score.meter.strong_eighths();
    for b in 0..form.bars() {
        let mode = form.texture_at(b).plucks;
        if mode == Plucks::Off {
            continue;
        }
        let chord = form.chord(b);
        let low = bass(&score.key, chord);
        // Where the timpani strikes the root the bass leaves it the beat:
        // four low strokes on one beat are the cue's peak.
        let timpani = form.texture_at(b).timpani;
        let role = variation::role(b);
        let last = *strong.last().unwrap();
        // The cadence's last beat steps to the chord's fifth, and the
        // variant passes through the mode into the bar's last beat.
        let fifth = chord.pitches_within(&score.key, low.saturating_sub(12), low + 12).into_iter().filter(|p| score.key.degree_of(*p) == Some((chord.root + 4).rem_euclid(7) as usize)).min_by_key(|p| (*p as i32 - low as i32).abs());
        for i in strong.iter().filter(|i| !(timpani && form.design.groove.dum.contains(i))) {
            let pitch = if role == Bar::Cadence && *i == last { fifth.unwrap_or(low) } else { low };
            score.add(Note { start: b * form.bar + i * E, len: E - 40, pitch, vel: vel(if mode == Plucks::Accents { -6 } else { -2 }, rng), channel: CH_PLUCK });
        }
        if role == Bar::Variant && last > 0 && !score.meter.strong(last - 1) {
            let passing = score.key.pitch(score.key.absolute_degree(low).unwrap() + 1, 4);
            score.add(Note { start: b * form.bar + (last - 1) * E, len: E - 40, pitch: passing, vel: vel(-10, rng), channel: CH_PLUCK });
        }
        if mode != Plucks::Full {
            continue;
        }
        let others: Vec<u8> = chord.pitches_within(&score.key, 57, 69).into_iter().filter(|p| score.key.degree_of(*p) != Some(bass_degree(chord).rem_euclid(7) as usize)).collect();
        for (j, i) in form.design.groove.chord.iter().enumerate() {
            let pitch = others[(j + b as usize) % others.len()];
            score.add(Note { start: b * form.bar + i * E, len: E - 60, pitch, vel: vel(-6, rng), channel: CH_PLUCK_2 });
        }
    }
}

/// Where the colours play: the choir on the chord's root and fifth and
/// the horn on its third, under the strings, a bar at a time, held on
/// where the next chord keeps them.
fn colours(score: &mut Score, form: &Form, rng: &mut Rng) {
    let drone = [score.key.pitch(0, 2), score.key.pitch(0, 3)];
    for b in 0..form.bars() {
        if !form.texture_at(b).colours {
            continue;
        }
        let chord = form.chord(b);
        let (root, fifth) = root_and_fifth(&score.key, chord, 55, 72);
        for p in std::iter::once(root).chain(fifth) {
            score.hold(Note { start: b * form.bar, len: form.bar + E / 8, pitch: p, vel: vel(-26, rng), channel: CH_CHOIR });
        }
        score.hold(Note { start: b * form.bar, len: form.bar + E / 8, pitch: third_under(&score.key, chord, &drone), vel: vel(-14, rng), channel: CH_HORN });
    }
}

/// The chord's third under the strings' lowest voice, an octave up where
/// it would rub the drone.
fn third_under(key: &Key, chord: Chord, drone: &[u8]) -> u8 {
    let mut third = key.pitch(chord.root + 2, 4);
    while third > 58 {
        third -= 12;
    }
    if third < 48 || drone.iter().any(|d| clashes(third, *d)) {
        third += 12;
    }
    third
}

/// The moon over the night: two tones of the chord high on the
/// dulcimer, struck by turns an eighth apart for a bar, then a bar's
/// rest, soft: a bright stroke high up is loud to the ear for its
/// strength, and the night is the cue's hush.
fn moon(score: &mut Score, form: &Form, rng: &mut Rng) {
    let mut b = 0;
    while b < form.bars() {
        if !form.texture_at(b).moon {
            b += 1;
            continue;
        }
        let tones = form.chord(b).pitches_within(&score.key, 76, 91);
        let (a, c) = (tones[rng.below(tones.len())], tones[rng.below(tones.len())]);
        for h in 0..score.meter.eighths() {
            score.add(Note { start: b * form.bar + h * E, len: E / 2, pitch: if h % 2 == 0 { a } else { c }, vel: vel(-32, rng), channel: CH_WEAVE });
        }
        b += 2;
    }
}

/// The tune echoed a bar behind on the dulcimer, detached, an octave
/// under the tune's register, its strong beats bent to the chord that
/// stands.
fn weave(score: &mut Score, form: &Form, rng: &mut Rng) {
    let (lo, hi) = (TUNE.0 - 12, TUNE.1 - 12);
    for b in 1..form.bars() {
        if !form.texture_at(b).weave {
            continue;
        }
        let behind = b - 1;
        let chord = form.chord(b);
        for (start, len, pitch) in form.tune.bar(score, behind, lo, hi, false) {
            let onset = start - behind * form.bar;
            let held = if len >= 3 * E { 2 * E } else { len - E / 2 };
            let pitch = if score.meter.strong(onset / E) && !chord.holds(&score.key, pitch) { tune::nearest_chord_tone(&score.key, chord, pitch, lo, hi) } else { pitch };
            score.add(Note { start: b * form.bar + onset, len: held, pitch, vel: vel(-8, rng), channel: CH_WEAVE });
        }
    }
}

/// At the crest, the horns take the tune's skeleton an octave under the
/// lead — its tone on every strong beat, held to the next — so the
/// climax is the tune broadened under the lead's line, one tune and not
/// two.
fn horns(score: &mut Score, form: &Form) {
    let bar = form.bar;
    let beats: Vec<Note> = score.notes.iter().filter(|n| n.channel == CH_LEAD && n.len >= E / 2 && score.strong(n.start) && form.texture_at(n.start / bar).crest).copied().collect();
    for (i, n) in beats.iter().enumerate() {
        let next = beats.get(i + 1).map_or(n.end(), |m| m.start.min(n.start + bar));
        score.add(Note { start: n.start, len: next.max(n.end()) - n.start - E / 8, pitch: n.pitch - 12, vel: n.vel.saturating_sub(8), channel: CH_HORNS });
    }
}

/// The timpani's tone of `chord`: its root, else its fifth, in the drum's
/// range.
fn timpani_tone(key: &Key, chord: Chord) -> u8 {
    let tones = chord.pitches_within(key, 40, 55);
    [chord.root, chord.root + 4].iter().find_map(|d| tones.iter().copied().find(|p| key.degree_of(*p) == Some(d.rem_euclid(7) as usize))).unwrap()
}

/// The frame drum on the dance: the dum as the night's pulse, and the
/// whole hand at the work; the taiko under the dum where the work is
/// struck; at the swell's peak the timpani on the chord's root with the
/// dum and the taiko on every group, and through the tune's last bar a
/// roll that grows into the title.
fn drums(score: &mut Score, form: &Form, rng: &mut Rng) {
    let groove = form.design.groove;
    for b in 0..form.bars() {
        let t = form.texture_at(b);
        let role = variation::role(b);
        let last_group = score.meter.eighths() - *groove.groups.last().unwrap() as u32;
        let rolls = role == Bar::Cadence && t.drum == Drum::Full && b + 1 < form.bars() && !t.timpani;
        if rolls {
            let n = (score.meter.eighths() - last_group) * 2;
            variation::roll(score, CH_DRUM, TEK, b * form.bar + last_group * E, n, |x| vel(-24 + (16.0 * x) as i32, rng));
        }
        if t.drum != Drum::Off {
            for i in groove.dum {
                let accent = match (t.drum, t.timpani) {
                    (Drum::Accents, _) => -12,
                    (_, true) => -8,
                    _ => 0,
                };
                score.add(Note { start: b * form.bar + i * E, len: E - 20, pitch: DUM, vel: vel(accent, rng), channel: CH_DRUM });
            }
        }
        if t.drum == Drum::Full {
            for i in groove.tek.iter().filter(|i| !rolls || **i < last_group) {
                score.add(Note { start: b * form.bar + i * E, len: E - 20, pitch: TEK, vel: vel(-14, rng), channel: CH_DRUM });
            }
        }
        if t.drum != Drum::Off && role == Bar::Variant {
            if let Some(e) = (1..score.meter.eighths()).find(|e| !score.meter.strong(*e) && !groove.dum.contains(e) && !groove.tek.contains(e)) {
                score.add(Note { start: b * form.bar + e * E, len: E - 20, pitch: TEK, vel: vel(-24, rng), channel: CH_DRUM });
            }
        }
        // Under the timpani the taiko and the dum strike softer:
        // three low drums struck at once are the cue's peak and not its
        // loudness.
        if t.taiko {
            for i in groove.dum {
                score.add(Note { start: b * form.bar + i * E, len: E - 20, pitch: 36, vel: vel(if t.timpani { -12 } else { -4 }, rng), channel: CH_TAIKO });
            }
            // The taiko's variant strikes the tek's first stroke softly, and
            // its cadence picks up into the next phrase on the bar's last
            // eighth.
            if !t.timpani && role == Bar::Variant {
                if let Some(i) = groove.tek.first() {
                    score.add(Note { start: b * form.bar + i * E, len: E - 20, pitch: 36, vel: vel(-18, rng), channel: CH_TAIKO });
                }
            }
            if !t.timpani && role == Bar::Cadence && b + 1 < form.bars() {
                let e = score.meter.eighths() - 1;
                score.add(Note { start: b * form.bar + e * E, len: E - 20, pitch: 36, vel: vel(-10, rng), channel: CH_TAIKO });
            }
            if t.timpani {
                for i in groove.tek {
                    score.add(Note { start: b * form.bar + i * E, len: E - 20, pitch: 36, vel: vel(-16, rng), channel: CH_TAIKO });
                }
            }
        }
        if t.timpani && b + 1 < form.bars() {
            let tone = timpani_tone(&score.key, form.chord(b));
            for i in groove.dum {
                score.add(Note { start: b * form.bar + i * E, len: E, pitch: tone, vel: vel(-14, rng), channel: CH_TIMPANI });
            }
        }
    }
    let last = form.bars() - 1;
    let tone = timpani_tone(&score.key, form.chord(last));
    let strokes = score.meter.eighths() * 4;
    for k in 0..strokes {
        let rise = -40 + (28 * k / strokes) as i32;
        score.add(Note { start: last * form.bar + k * E / 4, len: E / 4, pitch: tone, vel: vel(rise, rng), channel: CH_TIMPANI });
    }
}

/// The band stopped after `from` into bar `b` but for `keep`: a held
/// tone cut there, a struck one after it dropped. A drop before an
/// arrival is what makes it land.
fn hush(score: &mut Score, b: u32, from: u32, keep: &[u8]) {
    let (from, to) = (b * score.bar() + from, (b + 1) * score.bar());
    score.notes.retain(|n| keep.contains(&n.channel) || n.start < from || n.start >= to);
    for n in score.notes.iter_mut().filter(|n| !keep.contains(&n.channel) && n.start < from && n.end() > from) {
        n.len = from - n.start;
    }
}

/// The lead through bar `b` holds the tone it strikes on the bar line to
/// the bar's end, softer, and plays nothing after it: the suspension a
/// hit resolves.
fn suspend(score: &mut Score, b: u32) {
    let (from, to) = (b * score.bar(), (b + 1) * score.bar());
    score.notes.retain(|n| n.channel != CH_LEAD || n.start <= from || n.start >= to);
    if let Some(n) = score.notes.iter_mut().find(|n| n.channel == CH_LEAD && n.start == from) {
        n.len = to - from - E / 8;
        n.vel = n.vel.saturating_sub(12);
    }
}

/// The frame drum's fill through the drop before the swell: its high
/// stroke on every eighth after the bar's first group, growing, and
/// the dum on the last.
fn fill(score: &mut Score, form: &Form, from: u32, rng: &mut Rng) {
    let b = form.starts[4] - 1;
    let eighths = score.meter.eighths() - from / E;
    for k in 0..eighths {
        let last = k + 1 == eighths;
        score.add(Note { start: b * form.bar + from + k * E, len: E - 20, pitch: if last { DUM } else { TEK }, vel: vel(-18 + (20 * k / eighths) as i32, rng), channel: CH_DRUM });
    }
}

/// The title: the tonic struck by everyone on its first beat and let
/// ring — the lead on the tonic its line turns back to, after a leap the
/// octave the other way, else the nearest, the horns under home, the strings, the choir and
/// the horn on the chord, the plucks, the drums and the timpani once —
/// the struck tones ringing out on their own and the held ones to the
/// end under a pedal that falls away as the section rings, where the
/// room takes them.
fn title(score: &mut Score, form: &Form, rng: &mut Rng) {
    let key = score.key;
    let bar = form.bar;
    let at = form.bars() * bar;
    let end = at + TITLE_BARS * bar;
    let chord = Chord::triad(0);
    let home = tune::home_tonic(&key, TUNE.0, TUNE.1);
    let mut line: Vec<&Note> = score.notes.iter().filter(|n| n.channel == CH_LEAD && n.start < at && n.len >= E / 2).collect();
    line.sort_by_key(|n| n.start);
    // Any tonic the lead can play: the theme keeps to its register, the
    // title's held tone only to the instrument's.
    let lead = score.instrument(CH_LEAD);
    let tonics: Vec<u8> = (lead.low..=lead.high).filter(|p| p % 12 == key.tonic).collect();
    let landing = match line.as_slice() {
        [.., a, b] => {
            let leap = key.standing_degree(b.pitch) - key.standing_degree(a.pitch);
            let back = |t: &&u8| leap.abs() < counterpoint::LEAP || (**t as i32 - b.pitch as i32).signum() != leap.signum();
            tonics.iter().filter(back).min_by_key(|t| (**t as i32 - b.pitch as i32).abs()).copied().unwrap_or(home)
        }
        _ => home,
    };
    score.add(Note { start: at, len: end - at - E, pitch: landing, vel: vel(0, rng), channel: CH_LEAD });
    score.add(Note { start: at, len: end - at - E, pitch: home - 12, vel: vel(-4, rng), channel: CH_HORNS });
    for p in chord.pitches_within(&key, 55, 72).into_iter().take(3) {
        score.hold(Note { start: at, len: end - at - E / 2, pitch: p, vel: vel(-10, rng), channel: CH_PAD });
    }
    let drone = [key.pitch(0, 2), key.pitch(0, 3)];
    let (root, fifth) = root_and_fifth(&key, chord, 55, 72);
    for p in std::iter::once(root).chain(fifth) {
        score.hold(Note { start: at, len: end - at, pitch: p, vel: vel(-18, rng), channel: CH_CHOIR });
    }
    score.hold(Note { start: at, len: bar, pitch: third_under(&key, chord, &drone), vel: vel(-10, rng), channel: CH_HORN });
    // Every struck voice under its stroke's strength in the swell: struck
    // together on one instant they sum to the cue's peak.
    score.add(Note { start: at, len: bar, pitch: bass(&key, chord), vel: vel(-4, rng), channel: CH_PLUCK });
    for p in chord.pitches_within(&key, 57, 69) {
        score.add(Note { start: at, len: bar, pitch: p, vel: vel(-16, rng), channel: CH_PLUCK_2 });
    }
    score.add(Note { start: at, len: bar, pitch: home, vel: vel(-6, rng), channel: CH_WEAVE });
    score.add(Note { start: at, len: bar, pitch: timpani_tone(&key, chord), vel: vel(0, rng), channel: CH_TIMPANI });
    score.add(Note { start: at, len: bar, pitch: 36, vel: vel(-4, rng), channel: CH_TAIKO });
    for (pitch, accent) in [(DUM, -10), (KICK, -10), (CRASH, -8)] {
        score.add(Note { start: at, len: bar, pitch, vel: vel(accent, rng), channel: CH_DRUM });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The cue's own rows are the folk's: none turns as the ballad does.
    #[test]
    fn the_cue_turns_as_the_folk_does() {
        let ballad = [crate::theory::schema::BALLAD.verse, crate::theory::schema::BALLAD.chorus, crate::theory::schema::BALLAD.climax];
        for row in [&TONIC, &LAMENT, &PLAGAL, &RISE] {
            assert!(ballad.iter().copied().flatten().all(|s| s.roots != row.roots || s.alters != row.alters), "the cue's {} is the ballad's", row.name);
        }
    }

    /// The cue is its movements in order at their cuts, whole
    /// half-phrases each, the tune whole phrases ending on the rise, the
    /// title on the tonic; nothing sounds past the end.
    #[test]
    fn a_cue_is_its_movements_at_their_cuts() {
        for seed in 0..12 {
            let score = build(&Params::of(crate::pieces::find("teaser").unwrap(), seed));
            let names: Vec<&str> = score.sections.iter().map(|s| s.name).collect();
            assert_eq!(names, ["dawn", "vistas", "day into night", "work", "swell", "title"]);
            let half = phrase::BARS / 2 * score.bar();
            for (s, m) in score.sections.iter().zip(&MOVEMENTS) {
                assert_eq!((s.end - s.start) % half, 0, "seed {seed}: {} of broken half-phrases", s.name);
                assert!((score.seconds(s.start) as f32 - m.cut.at).abs() < 6.0, "seed {seed}: {} at {:.1} s", s.name, score.seconds(s.start));
            }
            let title = score.sections.last().unwrap();
            assert_eq!(title.start % (phrase::BARS * score.bar()), 0);
            assert_eq!(score.chord_at(title.start).root, 0);
            assert_eq!(score.chord_at(title.start - 1).root, 6, "seed {seed}: the rise ends on the flat seventh");
            for n in &score.notes {
                assert!(n.end() <= score.end(), "seed {seed}: {} past the end", score.instrument(n.channel).name);
            }
            for n in score.notes.iter().filter(|n| n.channel == CH_LEAD) {
                assert!(n.pitch >= TUNE.0 && n.pitch <= TUNE.1 + 3, "seed {seed}: the tune at {}", n.pitch);
            }
        }
    }
}
