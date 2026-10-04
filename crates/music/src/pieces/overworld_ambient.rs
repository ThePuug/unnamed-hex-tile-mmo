//! The overworld's music: a place told in a loop, on a dance, from a
//! drone. Every choice is a draw from the seed's stream, one fork per
//! purpose, and the first fork decides what kind of piece this is —
//! which story it tells, in what mode and tonic, on which dance, with
//! which instruments, on what theme, in which phrase form, on which
//! harmonic schemata — so two seeds are two pieces of one place. The
//! seed picks which shape at every level; it never picks the next
//! note.
//!
//! A story is a ladder of the bed's layers, walked as `ladder` says;
//! the story is the texture's, and the loudness holds, the pedal
//! giving back what each layer adds. One theme runs under the whole
//! piece, phrase after phrase, an open schema and a closed by turns,
//! and one player tells it, the lead: it sings the tune, plays it as a
//! riff, holds a tone between the tellings. The weave echoes it a bar
//! behind, a second holds tones under the riff, and the bed — drum,
//! plucks, strings, breath — plays the dance under it.

use crate::ladder::{self, turn, Bed, Story, Walk};
use crate::pieces::Params;
use crate::rng::Rng;
use crate::score::{Instrument, Note, Role, Score, TICKS_PER_EIGHTH as E};
use crate::teller::{self, Hold, Teller, Telling};
use crate::theory::groove::{Groove, BALKAN};
use crate::theory::melody::Theme;
use crate::theory::phrase::{self, Form as PhraseForm, FORMS};
use crate::theory::schema::{schemata_for, Schema};
use crate::theory::{clashes, Chord, Key, Mode};
use crate::variation::{self, Role as Bar};
use crate::tune::{self, Tune};

/// General MIDI programs, 0-based.
const CELLO: u8 = 42;
const CONTRABASS: u8 = 43;
const STRINGS_1: u8 = 48;
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
const HARP: u8 = 46;
/// Drum keys on General MIDI's percussion channel: the low and high conga
/// stand in for a frame drum's dum and tek.
const DUM: u8 = 64;
const TEK: u8 = 60;

const CH_DRONE: u8 = 0;
const CH_PAD: u8 = 1;
const CH_LEAD: u8 = 2;
const CH_PLUCK: u8 = 3;
const CH_SECOND: u8 = 4;
const CH_CHOIR: u8 = 5;
const CH_HORN: u8 = 6;
const CH_DOUBLE: u8 = 7;
const CH_SHIMMER: u8 = 8;
const CH_DRUM: u8 = 9;
const CH_PLUCK_2: u8 = 10;
const CH_WEAVE: u8 = 11;
const CH_WEAVE_2: u8 = 12;
const CH_PAD_ALONE: u8 = 13;
const CH_BREATH: u8 = 14;
const CH_BREATH_AIR: u8 = 15;

/// How long the room rings: a stone hall, the bed's space as much as
/// its sound.
const ROOM_S: f32 = 3.2;

/// Who may lead: the players that can hold a line and shape it. The
/// pool's files each have their own.
const LEADS: [u8; 5] = [FLUTE, PAN_FLUTE, FIDDLE, CLARINET, ENGLISH_HORN];
/// Each lead's level, dB, so that wherever it plays it sits 1.5 dB
/// under the band: the bank's samples of them are not one loudness.
/// Measured against the band on the pool's seeds.
fn lead_level(program: u8) -> f32 {
    match program {
        FLUTE => -0.6,
        PAN_FLUTE => 3.2,
        FIDDLE => 3.1,
        CLARINET => 2.1,
        ENGLISH_HORN => 4.1,
        _ => 0.0,
    }
}

/// Who may hold tones under the lead's riff.
const SECONDS: [u8; 4] = [FIDDLE, CLARINET, ACCORDION, ENGLISH_HORN];

/// The velocity every voice strikes at before its own accent: one
/// dynamic for the whole bed, since its story is in what plays.
const VEL: i32 = 85;

/// What each layer at each notch lifts the bed's loudness by, LU, and
/// what each lead does, which the pedal takes back through the part
/// so a full bed is no louder than a thin one. Fitted to the render,
/// every section of thirty seeds against what was on in it, to well
/// under a LU; the range check holds them true.
const LIFT_PAD: [f32; 3] = [0.0, 0.3, 1.7];
const LIFT_PLUCKS: [f32; 3] = [0.0, 0.0, 0.4];
const LIFT_DRUM: [f32; 3] = [0.0, 0.3, 0.3];
const LIFT_COLOURS: f32 = 0.8;
const LIFT_WEAVE: [f32; 3] = [0.0, 0.4, 0.4];
/// By telling, in `Telling`'s order; the overworld never trades.
const LIFT_LEAD: [f32; 6] = [0.0, 1.6, 3.1, 0.0, 3.8, 3.4];

/// The tune's register; the lead and the second take it here, the weave
/// an octave under. Home is the tonic a third under its middle, since
/// a shape reaches a sixth above home and a step below, so with the
/// sequences the tune spans an octave over home and never leaves the
/// register.
const TUNE: (u8, u8) = (62, 88);
/// The sequence a phrase pair takes, in degrees, pair by pair: the
/// theme, a step up, the theme, a step up, round again.
const PAIRS: [i32; 2] = [0, 1];
/// The sequence the ladder's top takes instead, a third up: the
/// piece's own seyir, the tune rising where the bed is fullest.
const CLIMB: i32 = 2;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Pad {
    Off,
    /// The top voice alone.
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
    /// The dum alone.
    Accents,
    Full,
}

/// The theme echoed under everything, soft.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Weave {
    Off,
    /// The dulcimer, the tune a bar behind.
    One,
    /// The harp too, the skeleton a foot late, a third up.
    Two,
}

/// A layer of the bed, the thing a rung of the ladder moves.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Layer {
    Pad,
    Plucks,
    Drum,
    /// The seed's colours — choir, horn, shimmer — together.
    Colours,
    Weave,
}

/// What the bed is: each layer at its notch.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Texture {
    pad: Pad,
    plucks: Plucks,
    drum: Drum,
    colours: bool,
    weave: Weave,
}

impl Texture {
    const BARE: Texture = Texture { pad: Pad::Off, plucks: Plucks::Off, drum: Drum::Off, colours: false, weave: Weave::Off };

    /// The pedal's gain under this texture and `lead`: what they lift
    /// the loudness by, given back.
    fn trim(&self, lead: Telling) -> f32 {
        ladder::gain(LIFT_PAD[self.pad as usize] + LIFT_PLUCKS[self.plucks as usize] + LIFT_DRUM[self.drum as usize] + if self.colours { LIFT_COLOURS } else { 0.0 } + LIFT_WEAVE[self.weave as usize] + LIFT_LEAD[lead as usize])
    }
}

impl Bed for Texture {
    type Layer = Layer;

    fn up(self, layer: Layer) -> Texture {
        match layer {
            Layer::Pad => Texture { pad: if self.pad == Pad::Off { Pad::Thin } else { Pad::Full }, ..self },
            Layer::Plucks => Texture { plucks: if self.plucks == Plucks::Off { Plucks::Accents } else { Plucks::Full }, ..self },
            Layer::Drum => Texture { drum: if self.drum == Drum::Off { Drum::Accents } else { Drum::Full }, ..self },
            Layer::Colours => Texture { colours: true, ..self },
            Layer::Weave => Texture { weave: if self.weave == Weave::Off { Weave::One } else { Weave::Two }, ..self },
        }
    }

    fn step_from(self, from: Texture) -> &'static str {
        let to = self;
        let moved = |a: u8, b: u8, names: [&'static str; 4]| match (a, b) {
            (0, _) => names[0],
            (_, 0) => names[3],
            _ if b > a => names[1],
            _ => names[2],
        };
        if from.pad != to.pad {
            moved(from.pad as u8, to.pad as u8, ["pad in", "pad up", "pad down", "pad out"])
        } else if from.plucks != to.plucks {
            moved(from.plucks as u8, to.plucks as u8, ["plucks in", "plucks up", "plucks down", "plucks out"])
        } else if from.drum != to.drum {
            moved(from.drum as u8, to.drum as u8, ["drum in", "drum up", "drum down", "drum out"])
        } else if from.colours != to.colours {
            if to.colours { "colours in" } else { "colours out" }
        } else if from.weave != to.weave {
            moved(from.weave as u8, to.weave as u8, ["weave in", "weave up", "weave down", "weave out"])
        } else {
            "held"
        }
    }
}

use Layer::{Colours, Drum as DrumLayer, Pad as PadLayer, Plucks as PlucksLayer, Weave as WeaveLayer};
use Telling::{Long, Off, Phrases, Riff, RiffAndLong};

const STORIES: [Story<Texture, Telling>; 6] = [
    Story {
        name: "arc",
        weight: 3.0,
        base: Texture::BARE,
        ladder: &[PadLayer, DrumLayer, PadLayer, PlucksLayer, WeaveLayer, Colours, PlucksLayer],
        leads: &[Phrases, Phrases, Phrases, Long, Long, RiffAndLong, RiffAndLong, Riff],
        turns: &[turn((7, 7), (1, 1))],
        halves: (1, 1),
        pace: (0.0, 1.0),
    },
    Story {
        name: "two waves",
        weight: 2.0,
        base: Texture::BARE,
        ladder: &[PadLayer, DrumLayer, PlucksLayer, WeaveLayer, PadLayer, Colours],
        leads: &[Phrases, Phrases, Phrases, Long, Long, RiffAndLong, Riff],
        turns: &[turn((3, 3), (0, 0)), turn((1, 1), (0, 0)), turn((6, 6), (1, 1))],
        halves: (1, 1),
        pace: (0.0, 1.0),
    },
    Story {
        name: "slow burn",
        weight: 2.0,
        base: Texture::BARE,
        ladder: &[PadLayer, DrumLayer, PadLayer, WeaveLayer, PlucksLayer, Colours],
        leads: &[Phrases, Phrases, Phrases, Phrases, Long, Long, Phrases],
        turns: &[turn((6, 6), (2, 2))],
        halves: (1, 2),
        pace: (0.0, 1.0),
    },
    Story {
        name: "lament",
        weight: 2.0,
        base: Texture::BARE,
        ladder: &[PadLayer, PlucksLayer, PadLayer, WeaveLayer, Colours],
        leads: &[Phrases, Phrases, Phrases, Phrases, Long, Long],
        turns: &[turn((5, 5), (1, 1))],
        halves: (2, 2),
        pace: (0.0, 1.0),
    },
    Story {
        name: "dance",
        weight: 2.0,
        base: Texture { pad: Pad::Off, plucks: Plucks::Full, drum: Drum::Accents, colours: false, weave: Weave::Off },
        ladder: &[PadLayer, WeaveLayer, PadLayer, DrumLayer, Colours],
        leads: &[Off, Phrases, Phrases, RiffAndLong, RiffAndLong, Riff],
        turns: &[turn((5, 5), (1, 1)), turn((1, 1), (0, 0)), turn((4, 5), (1, 1))],
        halves: (1, 1),
        pace: (0.0, 1.0),
    },
    Story {
        name: "fragments",
        weight: 1.5,
        base: Texture { pad: Pad::Thin, plucks: Plucks::Off, drum: Drum::Off, colours: false, weave: Weave::Off },
        ladder: &[PlucksLayer, WeaveLayer, PlucksLayer, PadLayer, Colours],
        leads: &[Phrases, Riff, Phrases, Riff, Phrases, Long],
        turns: &[turn((1, 1), (0, 0)), turn((0, 0), (0, 0)), turn((1, 1), (0, 0)), turn((2, 3), (0, 0)), turn((1, 1), (0, 0)), turn((4, 5), (0, 1))],
        halves: (1, 1),
        pace: (0.0, 1.0),
    },
];

/// What a seed's piece is.
struct Design {
    drone: u8,
    pad: u8,
    /// The one player who tells the tune, the storyteller.
    lead: u8,
    /// Who holds tones under the lead's riff: never the lead's own.
    second: u8,
    pluck: u8,
    pluck_2: u8,
    shimmer: bool,
    choir: bool,
    horn: bool,
    groove: &'static Groove,
    form: PhraseForm,
    theme: Theme,
    open: &'static Schema,
    closed: &'static Schema,
}

/// The bed's velocity with an accent and a little jitter so no two
/// notes strike alike.
fn vel(accent: i32, rng: &mut Rng) -> u8 {
    (VEL + accent + rng.range(-4, 4)).clamp(1, 127) as u8
}

struct Form {
    bar: u32,
    walk: Walk<Texture, Telling>,
    tune: Tune,
    design: Design,
}

impl Form {
    fn texture_at(&self, bar: u32) -> Texture {
        self.walk.bed_at(bar)
    }
    fn chord(&self, bar: u32) -> Chord {
        self.tune.chords[bar as usize]
    }
    fn bars(&self) -> u32 {
        self.walk.bars()
    }
}

pub fn build(params: &Params) -> Score {
    let rng = Rng::new(params.seed);
    let mut skeleton = rng.fork(0);
    let story = ladder::draw(&STORIES, &mut skeleton);
    // Never the harmonic minor: its leading tone sits a semitone under
    // the tonic drone in every chord that carries it.
    let mode = [Mode::Aeolian, Mode::Dorian, Mode::Hijaz][skeleton.weighted(&[4.0, 2.5, 2.5])];
    let key = Key::new(["D", "E", "G", "A", "C"][skeleton.weighted(&[3.0, 2.0, 2.0, 2.0, 1.0])], mode);
    // The slow dances, for a wander; the quick ones are other pieces'.
    let groove = &BALKAN[skeleton.weighted(&[4.0, 0.0, 0.0, 2.0, 1.0])];
    let tempo = story.tempo(groove.tempo, &mut skeleton);
    // At least one colour, since a ladder may bring the colours in.
    let colours = skeleton.range(1, 7);
    let (open, closed) = schemata_for(&key);
    let teller = LEADS[skeleton.below(LEADS.len())];
    let design = Design {
        drone: [CELLO, CONTRABASS][skeleton.below(2)],
        pad: [STRINGS_1, STRINGS_2][skeleton.below(2)],
        lead: teller,
        second: *skeleton.pick(&SECONDS.iter().copied().filter(|p| *p != teller).collect::<Vec<u8>>()),
        pluck: [NYLON_GUITAR, PIZZICATO][skeleton.below(2)],
        pluck_2: [NYLON_GUITAR, NYLON_GUITAR, DULCIMER][skeleton.below(3)],
        choir: colours & 1 != 0,
        horn: colours & 2 != 0,
        shimmer: colours & 4 != 0,
        groove,
        form: FORMS[skeleton.below(FORMS.len())],
        theme: Theme::draw(groove, &mut skeleton),
        open: open[skeleton.below(open.len())],
        closed: closed[skeleton.below(closed.len())],
    };
    let instruments = vec![
        Instrument { name: "drone", program: design.drone, channel: CH_DRONE, role: Role::Drone, low: 24, high: 60, reverb: 40, pan: 0, level: 0.0 },
        Instrument { name: "strings", program: design.pad, channel: CH_PAD, role: Role::Sustain, low: 48, high: 79, reverb: 100, pan: -29, level: 0.0 },
        Instrument { name: "lead", program: design.lead, channel: CH_LEAD, role: Role::Melody, low: 62, high: 91, reverb: 40, pan: 0, level: lead_level(design.lead) },
        Instrument { name: "pluck", program: design.pluck, channel: CH_PLUCK, role: Role::Pluck, low: 45, high: 74, reverb: 30, pan: 34, level: 0.0 },
        Instrument { name: "second", program: design.second, channel: CH_SECOND, role: Role::Melody, low: 55, high: 88, reverb: 60, pan: 21, level: 0.0 },
        Instrument { name: "choir", program: CHOIR_AAHS, channel: CH_CHOIR, role: Role::Sustain, low: 55, high: 72, reverb: 100, pan: 23, level: 0.0 },
        Instrument { name: "horn", program: FRENCH_HORN, channel: CH_HORN, role: Role::Sustain, low: 48, high: 67, reverb: 55, pan: -39, level: 0.0 },
        Instrument { name: "strings, the riff", program: STRINGS_2, channel: CH_DOUBLE, role: Role::Doubling, low: 55, high: 88, reverb: 60, pan: -34, level: 0.0 },
        Instrument { name: "shimmer", program: DULCIMER, channel: CH_SHIMMER, role: Role::Pluck, low: 62, high: 91, reverb: 50, pan: 49, level: 0.0 },
        Instrument { name: "frame drum", program: 0, channel: CH_DRUM, role: Role::Percussion, low: 60, high: 64, reverb: 30, pan: -8, level: 0.0 },
        Instrument { name: "pluck 2", program: design.pluck_2, channel: CH_PLUCK_2, role: Role::Pluck, low: 55, high: 72, reverb: 30, pan: -44, level: 0.0 },
        Instrument { name: "weave", program: DULCIMER, channel: CH_WEAVE, role: Role::Pluck, low: 50, high: 81, reverb: 35, pan: -52, level: 0.0 },
        Instrument { name: "weave, the harp", program: HARP, channel: CH_WEAVE_2, role: Role::Pluck, low: 55, high: 88, reverb: 40, pan: 55, level: 0.0 },
        Instrument { name: "strings, alone", program: STRINGS_2, channel: CH_PAD_ALONE, role: Role::Sustain, low: 48, high: 79, reverb: 105, pan: -29, level: 0.0 },
        Instrument { name: "breath", program: VOICE_OOHS, channel: CH_BREATH, role: Role::Sustain, low: 48, high: 67, reverb: 110, pan: -42, level: 0.0 },
        Instrument { name: "breath, the air", program: HALO_PAD, channel: CH_BREATH_AIR, role: Role::Sustain, low: 48, high: 67, reverb: 120, pan: 42, level: 0.0 },
    ];
    let mut score = Score::new(key, groove.meter(), tempo, instruments, ROOM_S);
    score.loops = true;
    score.lead = Some(CH_LEAD);
    let name = |program: u8| match program {
        CELLO => "cello",
        CONTRABASS => "contrabass",
        STRINGS_1 => "strings",
        STRINGS_2 => "slow strings",
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
    let colours: Vec<&str> = [(design.choir, "choir"), (design.horn, "horn"), (design.shimmer, "shimmer")].into_iter().filter(|(on, _)| *on).map(|(_, n)| n).collect();
    score.summary = format!(
        "{} on the {}: a {:?} in a {:?}, {} to ask and {} to answer; {} drone, {} pad, {} lead, {} second, {} and {} plucks, colours {}",
        story.name,
        groove.name,
        design.theme.shape,
        design.form,
        design.open.name,
        design.closed.name,
        name(design.drone),
        name(design.pad),
        name(design.lead),
        name(design.second),
        name(design.pluck),
        name(design.pluck_2),
        colours.join(", ")
    );
    let bar = score.bar();

    // The walk, in half-phrases, and whole question-and-answer pairs
    // of them, so the loop closes on an answer.
    let walk = story.place(&mut skeleton, &mut score, 4, |texture, lead| texture.trim(lead));

    // The tune's degrees shifted: the climb at the ladder's top, else
    // the phrase pair's sequence.
    let upper = story.ladder.len().max(1);
    let shifts: Vec<i32> = (0..walk.bars()).map(|b| if walk.at(b).rung >= upper { CLIMB } else { PAIRS[(b / phrase::BARS / 2) as usize % PAIRS.len()] }).collect();
    let tune = Tune::compose(&design.theme, &score.meter, design.form, &[design.open, design.closed], 3, shifts);
    score.harmony = tune.chords.clone();
    let form = Form { bar, walk, tune, design };

    drone(&mut score, &form);
    breath(&mut score, &form, &mut rng.fork(12));
    pad(&mut score, &form, &mut rng.fork(2));
    plucks(&mut score, &form, &mut rng.fork(3));
    if form.design.shimmer {
        shimmer(&mut score, &form, &mut rng.fork(4));
    }
    if form.design.choir {
        choir(&mut score, &form, &mut rng.fork(5));
    }
    if form.design.horn {
        horn(&mut score, &form, &mut rng.fork(6));
    }
    weave(&mut score, &form, &mut rng.fork(9));
    let teller = Teller {
        lead: CH_LEAD,
        second: CH_SECOND,
        double: Some((CH_DOUBLE, -20)),
        echo: CH_WEAVE,
        register: TUNE,
        grace: teller::above,
        sung: (-10, -20),
        riff: (4, -4, -18),
        long: (72, 84, -18),
        under: (62, 76, -18),
        hold: Hold::Breathing,
        breathes: false,
        vel,
    };
    teller::tell(&mut score, &teller, &form.tune, &form.walk.runs(), &mut rng.fork(8));
    frame_drum(&mut score, &form, &mut rng.fork(10));
    linger(&mut score);
    score.finish();
    score
}

/// The tonic in two octaves under everything, one note each for the whole
/// piece: a retrigger would restart the sample. The tonic alone, not the
/// fifth: a fifth held under the minor's VI and iv rubs a semitone
/// against their flat sixth. Held to the end, where the loop continues
/// it. Its level is the bed's floor, what a rest of every other voice
/// leaves; it sits where that rest is a lull and not a hole.
fn drone(score: &mut Score, form: &Form) {
    let end = form.bars() * form.bar;
    for pitch in [score.key.pitch(0, 2), score.key.pitch(0, 3)] {
        score.add(Note { start: 0, len: end, pitch, vel: 54, channel: CH_DRONE });
    }
}

/// A breathy choir under everything, the whole piece through: the
/// root and fifth of the chord that stands, low, held a bar at a time
/// and carried across the bar where the chord keeps them, on the oohs
/// with the halo pad's air under them, far back in the room, a tone
/// that would rub the drone under G3 up the octave. It is the glue
/// between the bed's voices and the wash that hides what a bank's
/// samples lack; it never leads, and it never leaves.
fn breath(score: &mut Score, form: &Form, rng: &mut Rng) {
    let drone = [score.key.pitch(0, 2), score.key.pitch(0, 3)];
    for b in 0..form.bars() {
        let chord = form.chord(b);
        let tones = chord.pitches_within(&score.key, 50, 64);
        let root = *tones.iter().find(|p| score.key.degree_of(**p) == Some(chord.root.rem_euclid(7) as usize)).unwrap();
        let fifth = tones.iter().find(|p| score.key.degree_of(**p) == Some((chord.root + 4).rem_euclid(7) as usize) && **p > root).copied();
        for p in std::iter::once(root).chain(fifth) {
            let p = if drone.iter().any(|d| clashes(p, *d)) { p + 12 } else { p };
            score.hold(Note { start: b * form.bar, len: form.bar + E / 2, pitch: p, vel: vel(-34, rng), channel: CH_BREATH });
            score.hold(Note { start: b * form.bar, len: form.bar + E / 2, pitch: p, vel: vel(-40, rng), channel: CH_BREATH_AIR });
        }
    }
}

/// Three voices of the bar's chord, each moving to the nearest tone of
/// the next, so the chords change without a leap; the top voice alone
/// where the part is thin, on the slow strings and softer, since with
/// nothing under it a string's attack is a strike, and rocking to the
/// next tone of the chord each bar the chord stands, so it moves
/// without the harmony moving. Every note lingers half an eighth into
/// the next chord, so a change is a crossfade and not a cut; `linger`
/// carries the consonant ones further once every part is written.
fn pad(score: &mut Score, form: &Form, rng: &mut Rng) {
    // From G3: a whole tone against the drone's octave, lower, is mud.
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
                // The top of the three lowest, or of the full pad's, carries on alone.
                None => voices.as_ref().map_or_else(|| candidates[2.min(candidates.len() - 1)], |v| nearest(&candidates, *v.iter().max().unwrap())),
                // While the chord stands, to its next tone up, and back
                // down from the top: a slow rocking.
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
            score.hold(Note { start: b * form.bar, len: form.bar + E / 2, pitch: v, vel: vel(-26, rng), channel: CH_PAD_ALONE });
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
            // After the lone voice: it stays the top and two join under
            // it, so the step up is two voices in, not a leap.
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

/// How far a string's tone carries into the next chord past the change:
/// a section bows into the next chord, and a crossfade of half an
/// eighth reads as a lift of the bow.
const LINGER: u32 = 2 * E;

/// Carries every string tone that ends half an eighth into the next
/// chord `LINGER` into it, unless it would rub a held voice sounding
/// there — a semitone, a tritone, a low whole tone — as a tone stepping
/// a semitone into the next chord does against its own successor; a
/// held clash is heard however soft it is. The loop's last chord keeps
/// its half eighth: the head's chord is the next, and the loop
/// continues what reaches it.
fn linger(score: &mut Score) {
    let bar = score.bar();
    let end = score.end();
    let held = |score: &Score, i: usize, from: u32, to: u32| -> bool {
        let n = score.notes[i];
        score.notes.iter().enumerate().any(|(j, m)| {
            j != i && matches!(score.instrument(m.channel).role, Role::Drone | Role::Sustain) && m.start < to && m.end() > from && clashes(n.pitch, m.pitch)
        })
    };
    for i in 0..score.notes.len() {
        let n = score.notes[i];
        let change = n.end() - E / 2;
        if !matches!(n.channel, CH_PAD | CH_PAD_ALONE) || change % bar != 0 || change >= end {
            continue;
        }
        if !held(score, i, n.end(), change + LINGER) {
            score.notes[i].len = change + LINGER - n.start;
        }
    }
}

/// The chord's tone the bass takes over the drone: its root where that
/// is a unison, fourth or fifth from the tonic, else its fifth, else its
/// third, so a bare bass never rubs the drone by a step — a root of VII
/// or III alone against the pedal is a wrong note, its fifth is not.
fn bass_degree(chord: Chord) -> i32 {
    let consonant = |d: i32| matches!(d.rem_euclid(7), 0 | 3 | 4);
    [chord.root, chord.root + 4, chord.root + 2].into_iter().find(|d| consonant(*d)).unwrap_or(chord.root)
}

/// The plucks on the dance: the first takes the bass on every strong
/// beat, the chord's tone that sits with the drone, an octave clear of
/// the drone's own pitch; the second takes the chord's other tones on
/// the dance's strikes, an octave up. Both sit under the bed's level:
/// a pluck is a strike, and a strike at the bed's level cuts in; alone
/// on the accents, with nothing to stand over, the bass is softer
/// still. The bass's variant bar passes through the mode into its last
/// beat, and its cadence takes the chord's other tone there.
fn plucks(score: &mut Score, form: &Form, rng: &mut Rng) {
    let strong = score.meter.strong_eighths();
    let floor = score.key.pitch(0, 3) + 2;
    for b in 0..form.bars() {
        let mode = form.texture_at(b).plucks;
        if mode == Plucks::Off {
            continue;
        }
        let chord = form.chord(b);
        let degree = bass_degree(chord);
        let tones = chord.pitches_within(&score.key, floor, floor + 12);
        let bass = *tones.iter().find(|p| score.key.degree_of(**p) == Some(degree.rem_euclid(7) as usize)).unwrap();
        // The cadence's last beat takes the chord's other tone that sits
        // with the drone, root for fifth or fifth for root; the variant
        // passes through the mode into the bar's last beat.
        let other = [chord.root, chord.root + 4].iter().map(|d| d.rem_euclid(7)).find(|d| *d != degree.rem_euclid(7)).and_then(|d| tones.iter().copied().find(|p| score.key.degree_of(*p) == Some(d as usize)));
        let role = variation::role(b);
        let last = *strong.last().unwrap();
        for i in strong.iter() {
            let start = b * form.bar + i * E;
            let pitch = if role == Bar::Cadence && *i == last { other.unwrap_or(bass) } else { bass };
            score.add(Note { start, len: E - 40, pitch, vel: vel(if mode == Plucks::Accents { -6 } else { 6 }, rng), channel: CH_PLUCK });
        }
        if role == Bar::Variant && last > 0 && !score.meter.strong(last - 1) {
            let passing = score.key.pitch(score.key.absolute_degree(bass).unwrap() + 1, 4);
            score.add(Note { start: b * form.bar + (last - 1) * E, len: E - 40, pitch: passing, vel: vel(-8, rng), channel: CH_PLUCK });
        }
        if mode != Plucks::Full {
            continue;
        }
        let others: Vec<u8> = chord.pitches_within(&score.key, 57, 69).into_iter().filter(|p| score.key.degree_of(*p) != Some(degree.rem_euclid(7) as usize)).collect();
        for (j, i) in form.design.groove.chord.iter().enumerate() {
            let start = b * form.bar + i * E;
            let pitch = others[(j + b as usize) % others.len()];
            score.add(Note { start, len: E - 60, pitch, vel: vel(-6, rng), channel: CH_PLUCK_2 });
        }
    }
}

/// A shimmer above the riff where the colours play: two tones of the
/// chord struck by halves for a bar, then a bar's rest, the tones drawn
/// afresh each time.
fn shimmer(score: &mut Score, form: &Form, rng: &mut Rng) {
    let mut b = 0;
    while b < form.bars() {
        if !form.texture_at(b).colours {
            b += 1;
            continue;
        }
        let chord = form.chord(b);
        let tones = chord.pitches_within(&score.key, 76, 91);
        let a = tones[rng.below(tones.len())];
        let c = tones[rng.below(tones.len())];
        for h in 0..score.meter.eighths() * 2 {
            let start = b * form.bar + h * E / 2;
            score.add(Note { start, len: E / 2, pitch: if h % 2 == 0 { a } else { c }, vel: vel(-22, rng), channel: CH_SHIMMER });
        }
        b += 2;
    }
}

/// Root and fifth of the chord above the drone, held a bar at a time,
/// where the colours play, the root alone entering a bar early.
fn choir(score: &mut Score, form: &Form, rng: &mut Rng) {
    for b in 0..form.bars() {
        let now = form.texture_at(b).colours;
        let next = b + 1 < form.bars() && form.texture_at(b + 1).colours;
        if !now && !next {
            continue;
        }
        let chord = form.chord(b);
        let tones = chord.pitches_within(&score.key, 55, 72);
        let root = *tones.iter().find(|p| score.key.degree_of(**p) == Some(chord.root.rem_euclid(7) as usize)).unwrap();
        let fifth = tones.iter().find(|p| score.key.degree_of(**p) == Some((chord.root + 4).rem_euclid(7) as usize) && **p > root).copied();
        let mut pitches = vec![root];
        if now {
            if let Some(f) = fifth {
                pitches.push(f);
            }
        }
        for p in pitches {
            score.hold(Note { start: b * form.bar, len: form.bar + E / 8, pitch: p, vel: vel(-30, rng), channel: CH_CHOIR });
        }
    }
}

/// The chord's third where the colours play, a bar at a time, held on
/// where the next chord keeps it: the one tone the choir's root and
/// fifth leave to it, under the strings' lowest voice where the drone
/// allows — a whole tone from it under G3 is mud — so the horn is
/// heard as a voice of its own and not a doubling.
fn horn(score: &mut Score, form: &Form, rng: &mut Rng) {
    let drone = [score.key.pitch(0, 2), score.key.pitch(0, 3)];
    for b in 0..form.bars() {
        if !form.texture_at(b).colours {
            continue;
        }
        let chord = form.chord(b);
        let mut third = score.key.pitch(chord.root + 2, 4);
        while third > 58 {
            third -= 12;
        }
        if third < 48 || drone.iter().any(|d| clashes(third, *d)) {
            third += 12;
        }
        score.hold(Note { start: b * form.bar, len: form.bar + E / 8, pitch: third, vel: vel(-14, rng), channel: CH_HORN });
    }
}

/// The tune echoed under everything, soft, where the bed asks: the
/// dulcimer plays each bar the tune of the bar before, detached, an
/// octave under the tune's register — a canon at the bar, so the tune
/// is in the air before the riff takes it and behind it after; at the
/// second notch the harp takes the bar's own skeleton, its strong-beat
/// tones, a foot late and a third up, bent to the chord.
fn weave(score: &mut Score, form: &Form, rng: &mut Rng) {
    let (lo, hi) = (TUNE.0 - 12, TUNE.1 - 12);
    let bars = form.bars();
    let late = form.design.theme.feet[0][0].1;
    let eighths = score.meter.eighths();
    for b in 0..bars {
        let mode = form.texture_at(b).weave;
        if mode == Weave::Off {
            continue;
        }
        let behind = (b + bars - 1) % bars;
        let chord = form.chord(b);
        for (start, len, pitch) in form.tune.bar(score, behind, lo, hi, false) {
            let onset = start - behind * form.bar;
            let held = if len >= 3 * E { 2 * E } else { len - E / 2 };
            // A bar behind, the tune meets this bar's chord: its strong
            // beats bend to it as the tune's own would.
            let pitch = if score.meter.strong(onset / E) && !chord.holds(&score.key, pitch) { tune::nearest_chord_tone(&score.key, chord, pitch, lo, hi) } else { pitch };
            score.add(Note { start: b * form.bar + onset, len: held, pitch, vel: vel(-8, rng), channel: CH_WEAVE });
        }
        if mode != Weave::Two {
            continue;
        }
        for (start, len, pitch) in form.tune.bar(score, b, lo, hi, false) {
            let onset = (start - b * form.bar) / E;
            if !score.meter.strong(onset) || onset + late >= eighths {
                continue;
            }
            let mut third = score.key.pitch(score.key.absolute_degree(pitch).unwrap() + 2, 4).min(hi + 12);
            if !chord.holds(&score.key, third) {
                third = tune::nearest_chord_tone(&score.key, chord, third, lo, hi + 12);
            }
            let held = (len - E / 2).min((eighths - onset - late) * E - E / 4);
            score.add(Note { start: start + late * E, len: held, pitch: third, vel: vel(-14, rng), channel: CH_WEAVE_2 });
        }
    }
}

/// The frame drum on the dance where the bed asks for it: the dum on
/// the dance's low strokes, and where the bed asks for the whole hand,
/// the tek on its high; soft, a hand and not a kit. Its phrase's variant
/// bar slips a ghost tek onto an eighth the dance leaves bare, and its
/// cadence rolls the fingers through the bar's last group, growing, into
/// the next phrase's dum.
fn frame_drum(score: &mut Score, form: &Form, rng: &mut Rng) {
    let groove = form.design.groove;
    let eighths = score.meter.eighths();
    let last_group = eighths - *groove.groups.last().unwrap() as u32;
    let bare = (1..eighths).find(|e| !score.meter.strong(*e) && !groove.dum.contains(e) && !groove.tek.contains(e));
    for b in 0..form.bars() {
        let mode = form.texture_at(b).drum;
        if mode == Drum::Off {
            continue;
        }
        let start = b * form.bar;
        let role = variation::role(b);
        let roll_from = (role == Bar::Cadence && b + 1 < form.bars()).then_some(last_group);
        let before_roll = |i: &u32| roll_from.is_none_or(|r| *i < r);
        for i in groove.dum.iter().filter(|i| before_roll(i)) {
            score.add(Note { start: start + i * E, len: E - 20, pitch: DUM, vel: vel(0, rng), channel: CH_DRUM });
        }
        if mode == Drum::Full {
            for i in groove.tek.iter().filter(|i| before_roll(i)) {
                score.add(Note { start: start + i * E, len: E - 20, pitch: TEK, vel: vel(-14, rng), channel: CH_DRUM });
            }
        }
        if let Some(e) = bare.filter(|_| role == Bar::Variant) {
            score.add(Note { start: start + e * E, len: E - 20, pitch: TEK, vel: vel(-24, rng), channel: CH_DRUM });
        }
        if let Some(r) = roll_from {
            let top = if mode == Drum::Full { -8 } else { -14 };
            variation::roll(score, CH_DRUM, TEK, start + r * E, (eighths - r) * 2, |x| vel(-26 + ((top + 26) as f32 * x) as i32, rng));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every story is a ladder.
    #[test]
    fn every_story_is_a_ladder() {
        for story in &STORIES {
            assert_eq!(story.fault(), None);
        }
    }

    /// The piece is a loop of whole question-and-answer pairs, its parts
    /// whole half-phrases, its harmony the schemata's, its tune in its
    /// register; nothing struck ends past the end, only what is held,
    /// which the loop continues.
    #[test]
    fn a_piece_is_whole_phrases_on_its_schemata() {
        for seed in 0..24 {
            let score = build(&Params { seed });
            assert!(score.loops);
            let bars = score.end() / score.bar();
            assert_eq!(bars % (2 * phrase::BARS), 0, "seed {seed}: {bars} bars");
            for s in &score.sections {
                assert_eq!((s.end - s.start) % (phrase::BARS / 2 * score.bar()), 0, "seed {seed}: a part of broken half-phrases");
            }
            assert_eq!(score.harmony[0].root, 0);
            assert_eq!(score.harmony.last().unwrap().root, 0);
            for n in score.notes.iter().filter(|n| matches!(n.channel, CH_LEAD | CH_SECOND)) {
                assert!((TUNE.0..=TUNE.1).contains(&n.pitch), "seed {seed}: the tune at {} leaves its register", n.pitch);
            }
            let end = score.end();
            for n in &score.notes {
                let role = score.instrument(n.channel).role;
                assert!(n.end() <= end || matches!(role, Role::Drone | Role::Sustain), "seed {seed}: {} past the end", score.instrument(n.channel).name);
            }
        }
    }
}
