//! Stomp and holler, the indie folk of Mumford & Sons, the Lumineers,
//! Edward Sharpe and Of Monsters and Men, built from twenty of their
//! transcriptions and the audio databases
//! (`proofs/research/stomp-holler-findings.md`): the major, one loop of
//! four chords from I, IV, V and vi a song, often opening on vi; a chant
//! on the major pentatonic; an acoustic guitar strummed through every bar;
//! a kick on every beat under a tambourine on every off-beat, never a
//! backbeat; the bass on the roots; a banjo rolling sixteenths and a
//! piano on the quarters where the song is a chorus; and the holler —
//! the hook in unison trumpets or on the accordion, the gang singing it
//! with them, clapping every beat and shouting the downbeats the tune
//! leaves empty, the band hitting under the shout. A verse is the strum
//! and a voice; a chorus is all of it, reached by layers or at once, the
//! contrast in the texture more than the level; before the last chorus
//! the band thins to the strum and builds back. Unbuilt: the six-eight of
//! Babel and Hopeless Wanderer.
//!
//! It is mixed as the records are: two acoustics, one each side, the
//! second capoed high; and every chordal part in a register of its own —
//! the guitars the octave under G4, the piano's hand and the gang's chant
//! over it, the gang on the hook in two octaves. Measured against the
//! records' previews, one guitar and every part piled round middle C left
//! the chorus narrow and thick under 400 Hz and hollow over 1 kHz, where a
//! voice's presence is: a lead standing in for one is a trumpet or an
//! accordion, a whistle only doubling it.
//!
//! The first draws are the song: its story, its loop, its key and pulse,
//! how the two voices share the tune, how it opens and how it ends, each
//! as often as the songs surveyed do it. A story is a ladder of the
//! band's layers, walked a rung a part, the breakdown and the band coming
//! back all in turns that leap. The verse and the chorus are two tunes,
//! as the songs' are (`stomp-holler-findings.md`, §7): the lead sings
//! the verse's, every tone struck as a chant's syllables are, and plays
//! the chorus's as the hook, a third over the verse; the hook frames the
//! song and is never in a verse. A line runs a half-phrase, a phrase or
//! a question and its answer before it lands, as long as the songs' lines
//! run, so no cell is cut short every two bars.

use crate::band::Part;
use crate::ladder::{self, leap, turn, Bed, Run, Story, Walk};
use crate::pieces::Params;
use crate::rng::Rng;
use crate::score::{Instrument, Note, Role, Score, Section, TICKS_PER_EIGHTH as E};
use crate::teller::{self, Hold, Teller, Telling};
use crate::theory::groove::{Groove, INDIE};
use crate::theory::melody::{Theme, INDIE_SHAPES};
use crate::theory::phrase::{self, Form as PhraseForm, FORMS};
use crate::theory::schema::{Schema, INDIE as LOOPS};
use crate::theory::{Chord, Key, Mode};
use crate::tune::{self, Placed, Tune};
use crate::variation::{self, Role as Bar};

/// General MIDI programs, 0-based.
const PIANO: u8 = 0;
const ACCORDION: u8 = 21;
const STEEL_GUITAR: u8 = 25;
const UPRIGHT_BASS: u8 = 32;
const CELLO: u8 = 42;
const CHOIR: u8 = 52;
const TRUMPET: u8 = 56;
const WHISTLE: u8 = 78;
const BANJO: u8 = 105;
/// Drum keys: the kick, the hand clap, the low floor tom, the tambourine.
const KICK: u8 = 36;
const CLAP: u8 = 39;
const FLOOR_TOM: u8 = 41;
const TAMBOURINE: u8 = 54;

const CH_GUITAR: u8 = 0;
const CH_GUITAR_HIGH: u8 = 10;
const CH_LEAD: u8 = 1;
const CH_SECOND: u8 = 2;
const CH_BASS: u8 = 3;
const CH_BANJO: u8 = 4;
const CH_KEYS: u8 = 5;
const CH_GANG: u8 = 6;
const CH_HOOK_LOW: u8 = 7;
const CH_HEY: u8 = 8;
const CH_DRUM: u8 = 9;
const CH_OVER: u8 = 11;

/// A wooden room the band plays live in, few of the records cut to a
/// click.
const ROOM_S: f32 = 1.8;

/// Who may carry the hook: a trumpet, as two carry Little Talks in
/// unison; a whistle, as Home's opens; an accordion.
const LEADS: [u8; 3] = [TRUMPET, WHISTLE, ACCORDION];
/// Who may answer it, the second voice.
const SECONDS: [u8; 4] = [TRUMPET, ACCORDION, CELLO, PIANO];

/// The tune's register; home sits a third under its middle, high enough
/// that a phrase four of the pentatonic's wide steps under it stays in
/// the lead's range.
const TUNE: (u8, u8) = (62, 86);
/// How far the chorus's line sits over the verse's on the whole,
/// degrees: a third, as the songs' choruses sit two to six semitones
/// over their verses.
const CHORUS_SHIFT: i32 = 2;
/// The second voice's floor: it answers an octave under the lead where
/// its phrase fits over this, as the band's other singer sings under.
const SECOND_LOW: u8 = 48;
const SECOND_HIGH: u8 = 84;
/// The whistle's reach over the hook.
const WHISTLE_HIGH: u8 = 98;
/// The gang's register: the singers on the hook and an octave under it,
/// the voices of a room singing along, men and women an octave apart as
/// Of Monsters and Men's two singers are.
const GANG: (u8, u8) = (48, 81);
/// Where the gang chants the chord, a chorus's and the last's: from the
/// guitars' top up, so they keep the octave under it to themselves.
const CHANT: (u8, u8) = (60, 79);
const CHANT_CREST: (u8, u8) = (64, 81);
/// The piano's right hand, from the guitars' top string up: two
/// instruments comping the chord in one octave thicken it to a smear.
const KEYS_HAND: (u8, u8) = (64, 79);
/// The second guitar's chord, the top five of it here, as a guitar
/// capoed high voices it over the open one.
const HIGH_STRINGS: (u8, u8) = (57, 76);
/// Where the banjo's three fretted strings roll, and the short fifth
/// string's drone over them: the key's tonic, as a banjo is tuned to it.
const BANJO_STRINGS: (u8, u8) = (55, 74);
const DRONE: (u8, u8) = (64, 75);

/// The velocity every voice strikes at before its own accent.
const VEL: i32 = 84;

/// Each section's level against the loudest, LU: the strum and a voice
/// at the foot, the fullest rung at the top — the whispered verse and the
/// bellowed chorus, as much in the level as the track's range leaves.
const LEVEL_FOOT: f32 = -6.0;

/// Ticks between two strings of a strum, about seven milliseconds at the
/// band's tempo: a hand's sweep across six strings takes thirty to sixty.
const STRUM: u32 = 6;
/// A sixteenth.
const S: u32 = E / 2;

/// How the guitar strums, the notches of the song's drive
/// (`stomp-holler-findings.md`, §3): once or twice a bar left to ring;
/// down on every quarter; down on the beats and up between; the driving
/// sixteenths Little Lion Man keeps the song through.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Strum {
    Ring,
    Quarters,
    Eighths,
    Drive,
}

/// The kick: off, on the bar's first beat, or on every beat.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kick {
    Off,
    One,
    Four,
}

/// The bass on the roots: off, a whole note a bar, or every quarter.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Bass {
    Off,
    Long,
    Quarters,
}

/// A layer of the band, the thing a rung of the ladder moves.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Layer {
    Strum,
    Kick,
    /// The tambourine on the off-beats.
    Chick,
    Bass,
    /// The chorus's colour: the banjo rolling, the piano on the quarters.
    Colour,
    /// The holler: the gang chanting and singing the hook, clapping every
    /// beat, shouting.
    Gang,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Texture {
    strum: Strum,
    kick: Kick,
    chick: bool,
    bass: Bass,
    colour: bool,
    gang: bool,
}

impl Texture {
    /// The guitar alone, strummed so.
    const fn strummed(strum: Strum) -> Texture {
        Texture { strum, kick: Kick::Off, chick: false, bass: Bass::Off, colour: false, gang: false }
    }
}

impl Bed for Texture {
    type Layer = Layer;

    fn up(self, layer: Layer) -> Texture {
        match layer {
            Layer::Strum => Texture {
                strum: match self.strum {
                    Strum::Ring => Strum::Quarters,
                    Strum::Quarters => Strum::Eighths,
                    _ => Strum::Drive,
                },
                ..self
            },
            Layer::Kick => Texture { kick: if self.kick == Kick::Off { Kick::One } else { Kick::Four }, ..self },
            Layer::Chick => Texture { chick: true, ..self },
            Layer::Bass => Texture { bass: if self.bass == Bass::Off { Bass::Long } else { Bass::Quarters }, ..self },
            Layer::Colour => Texture { colour: true, ..self },
            Layer::Gang => Texture { gang: true, ..self },
        }
    }

    fn step_from(self, from: Texture) -> &'static str {
        let to = self;
        let toggled = |on: bool, a: &'static str, b: &'static str| if on { a } else { b };
        if from.strum != to.strum {
            toggled(to.strum > from.strum, "strum up", "strum down")
        } else if from.kick != to.kick {
            match (from.kick, to.kick) {
                (Kick::Off, _) => "kick in",
                (_, Kick::Off) => "kick out",
                (Kick::One, Kick::Four) => "four on the floor",
                _ => "kick down",
            }
        } else if from.bass != to.bass {
            match (from.bass, to.bass) {
                (Bass::Off, _) => "bass in",
                (_, Bass::Off) => "bass out",
                (Bass::Long, Bass::Quarters) => "bass up",
                _ => "bass down",
            }
        } else if from.chick != to.chick {
            toggled(to.chick, "tambourine in", "tambourine out")
        } else if from.colour != to.colour {
            toggled(to.colour, "banjo and piano in", "banjo and piano out")
        } else if from.gang != to.gang {
            toggled(to.gang, "gang in", "gang out")
        } else {
            "held"
        }
    }
}

use Layer as L;
use Telling::{Patter, Riff};

/// The songs' ladders (`stomp-holler-findings.md`, §2, §6). Four on the
/// floor: the strum and a voice, the kick on one, then on every beat,
/// the gang last — Ho Hey, Ophelia, Stubborn Love — the second verse back
/// to the kick on one, the break before the last chorus the kick on one
/// alone. From the top: the sixteenth strum driving from the first bar,
/// the verse without a kick, the chorus all in at once with the banjo —
/// Little Lion Man, I Will Wait — down to the strum alone before the last
/// chorus and all in again. The long climb: the strum quickening a notch
/// at a time, quarters, eighths, sixteenths, the kick arriving a third of
/// the way in — The Cave, Home. Hook and drop: the verse a strum or two a
/// bar, the chorus all in, a drumless bridge — Little Talks, Mountain
/// Sound, Dirty Paws. Every drop before the last chorus comes back all in
/// at once, the bar before it stomping every eighth. The rungs from the
/// kick on every beat up are the chorus, where the lead plays the hook.
const STORIES: [Story<Texture, Telling>; 4] = [
    Story {
        name: "four on the floor",
        weight: 3.0,
        base: Texture::strummed(Strum::Eighths),
        ladder: &[L::Kick, L::Bass, L::Kick, L::Chick, L::Colour, L::Gang],
        leads: &[Patter, Patter, Patter, Riff, Riff, Riff, Riff],
        turns: &[turn((2, 2), (0, 0)), leap((5, 6), (1, 1)), leap((1, 2), (1, 1)), leap((6, 6), (1, 1)), leap((1, 1), (0, 0)), leap((6, 6), (2, 3))],
        halves: (2, 2),
        pace: (0.0, 1.0),
    },
    Story {
        name: "from the top",
        weight: 3.0,
        base: Texture::strummed(Strum::Drive),
        ladder: &[L::Bass, L::Kick, L::Kick, L::Colour, L::Bass, L::Chick, L::Gang],
        leads: &[Patter, Patter, Patter, Riff, Riff, Riff, Riff, Riff],
        turns: &[turn((1, 1), (0, 1)), leap((7, 7), (0, 0)), leap((1, 1), (0, 0)), leap((7, 7), (0, 0)), leap((0, 0), (0, 0)), leap((7, 7), (1, 1))],
        halves: (4, 4),
        pace: (0.0, 1.0),
    },
    Story {
        name: "long climb",
        weight: 2.0,
        base: Texture::strummed(Strum::Ring),
        ladder: &[L::Strum, L::Bass, L::Strum, L::Kick, L::Kick, L::Strum, L::Chick, L::Colour, L::Bass, L::Gang],
        leads: &[Patter, Patter, Patter, Patter, Patter, Riff, Riff, Riff, Riff, Riff, Riff],
        turns: &[turn((10, 10), (0, 0)), leap((2, 3), (1, 1)), leap((10, 10), (2, 3))],
        halves: (2, 2),
        pace: (0.0, 1.0),
    },
    Story {
        name: "hook and drop",
        weight: 2.0,
        base: Texture::strummed(Strum::Ring),
        ladder: &[L::Bass, L::Strum, L::Kick, L::Strum, L::Kick, L::Chick, L::Strum, L::Colour, L::Gang],
        leads: &[Patter, Patter, Patter, Patter, Patter, Riff, Riff, Riff, Riff, Riff],
        turns: &[turn((2, 2), (0, 1)), leap((9, 9), (1, 1)), leap((3, 4), (1, 1)), leap((9, 9), (1, 1)), leap((0, 1), (1, 2)), leap((9, 9), (2, 3))],
        halves: (2, 2),
        pace: (0.0, 1.0),
    },
];

/// How the second plays the hook: in unison, two of one instrument as
/// Little Talks' trumpets; an octave under; or, a bowed second, holding a
/// chord tone under the lead every other bar wherever the lead plays, as
/// a cello holds under the band — a bow's tone comes up over a stroke,
/// and a note as short as the hook's is gone before it has.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Under {
    Unison,
    Octave,
    Held,
}

/// How the two voices share the tune where it is sung: trading two bars
/// each, trading phrases, or the lead alone, as it is over a bowed
/// second's held tones; in the hook they join whichever it is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Duet {
    Halves,
    Phrases,
    Alone,
}

/// How the song opens: the hook over the strum, as Little Talks and Home
/// open, or the strum alone, as most do.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Intro {
    Hook,
    Strum,
}

impl Intro {
    /// Its name, as a band's preferences name it.
    fn name(self) -> &'static str {
        match self {
            Intro::Hook => "the hook first",
            Intro::Strum => "the strum first",
        }
    }
}

/// How it ends (`stomp-holler-findings.md`, §6): thinned to the strum and
/// the tambourine and a last stroke, as Ho Hey and Babel end; the gang
/// singing the hook out over its claps with the band gone, as Little Lion
/// Man and Little Talks end, and a last stroke; or the whole band on one
/// stomp of the tonic, ringing. None fades.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Ending {
    Thinned,
    Gang,
    Stomp,
}

impl Ending {
    fn name(self) -> &'static str {
        match self {
            Ending::Thinned => "thinned out",
            Ending::Gang => "the gang out",
            Ending::Stomp => "one stomp",
        }
    }
}

struct Design {
    lead: u8,
    second: u8,
    groove: &'static Groove,
    form: PhraseForm,
    /// The verse's theme, and the chorus's, a tune of its own.
    theme: Theme,
    chorus: Theme,
    open: &'static Schema,
    closed: &'static Schema,
    /// Whether the ringing strum strikes twice a bar, else once.
    twice: bool,
    /// How the second plays under the hook.
    under: Under,
    duet: Duet,
    intro: Intro,
    ending: Ending,
    slows: bool,
}

struct Form {
    bar: u32,
    walk: Walk<Texture, Telling>,
    tune: Tune,
    /// The chorus's tune on the song's first row, the hook an opening
    /// plays before the walk.
    hook: Tune,
    design: Design,
}

impl Form {
    fn bars(&self) -> u32 {
        self.walk.bars()
    }
}

fn vel(accent: i32, rng: &mut Rng) -> u8 {
    (VEL + accent + rng.range(-4, 4)).clamp(1, 127) as u8
}

pub fn build(params: &Params) -> Score {
    let rng = Rng::new(params.seed);
    let mut skeleton = rng.fork(0);
    let band = params.band;
    let stories: Vec<f32> = STORIES.iter().map(|s| s.weight * band.lean(s.name)).collect();
    let story = &STORIES[skeleton.weighted(&stories)];
    // The surveyed songs' keys by how often they are in them.
    let key = Key::new(["C", "E", "F", "G", "D", "D#", "C#", "G#"][skeleton.weighted(&[3.0, 3.0, 2.0, 2.0, 2.0, 2.0, 2.0, 1.0])], Mode::Ionian);
    // Four on the floor is the half-time stompers' story; the rest drive.
    let half_time = if story.name == "four on the floor" { 0.6 } else { 0.15 };
    let grooves: Vec<f32> = INDIE.iter().map(|g| band.lean(g.name) * if g.name == "stomp" { half_time } else { 1.0 - half_time }).collect();
    let groove = &INDIE[skeleton.weighted(&grooves)];
    let tempo = story.tempo(groove.tempo, &ladder::UNBOUNDED.within(band.prefs.tempo), &mut skeleton);
    let lead = band.program(Part::Lead, &LEADS, LEADS[0]);
    let second = band.programs(Part::Second).into_iter().chain(SECONDS).find(|p| SECONDS.contains(p)).unwrap_or(TRUMPET);
    let form = FORMS[skeleton.below(FORMS.len())];
    let theme = Theme::draw(groove, &INDIE_SHAPES, &mut skeleton);
    // The chorus on another shape than the verse's, where the songs' own
    // choruses are tunes apart from their verses.
    let others: Vec<_> = INDIE_SHAPES.iter().copied().filter(|s| *s != theme.shape).collect();
    let chorus = Theme::draw(groove, &others, &mut rng.fork(15));
    let [open, closed] = &LOOPS[skeleton.below(LOOPS.len())];

    let mut habits = rng.fork(14);
    let twice = habits.chance(0.5);
    let voices = [Duet::Halves, Duet::Phrases, Duet::Alone][habits.weighted(&[40.0, 35.0, 25.0])];
    // How it opens and ends, as the songs do and as the band likes to.
    let intros = [Intro::Hook, Intro::Strum];
    let opening = intros[habits.weighted(&intros.iter().zip([40.0, 60.0]).map(|(i, w)| w * band.lean(i.name())).collect::<Vec<f32>>())];
    let endings = [Ending::Thinned, Ending::Gang, Ending::Stomp];
    let close = endings[habits.weighted(&endings.iter().zip([35.0, 35.0, 30.0]).map(|(e, w)| w * band.lean(e.name())).collect::<Vec<f32>>())];
    let slows = close != Ending::Stomp && habits.chance(0.3);
    let under = if second == CELLO {
        Under::Held
    } else if second == lead {
        Under::Unison
    } else {
        Under::Octave
    };
    let voices = if under == Under::Held { Duet::Alone } else { voices };
    let design = Design { lead, second, groove, form, theme, chorus, open, closed, twice, under, duet: voices, intro: opening, ending: close, slows };

    let instruments = vec![
        Instrument { name: "guitar", program: band.program(Part::Figure, &[], STEEL_GUITAR), channel: CH_GUITAR, role: Role::Pluck, low: 40, high: 72, reverb: 35, pan: -48, level: -2.0 },
        Instrument { name: "guitar, high", program: band.program(Part::Figure, &[], STEEL_GUITAR), channel: CH_GUITAR_HIGH, role: Role::Pluck, low: HIGH_STRINGS.0, high: HIGH_STRINGS.1, reverb: 35, pan: 48, level: -2.0 },
        Instrument { name: "lead", program: lead, channel: CH_LEAD, role: Role::Melody, low: 55, high: 96, reverb: 40, pan: 6, level: 2.0 },
        Instrument { name: "second", program: second, channel: CH_SECOND, role: Role::Melody, low: SECOND_LOW, high: SECOND_HIGH, reverb: 40, pan: -10, level: 0.0 },
        Instrument { name: "bass", program: band.program(Part::Bass, &[], UPRIGHT_BASS), channel: CH_BASS, role: Role::Pluck, low: 28, high: 52, reverb: 20, pan: 0, level: -2.0 },
        Instrument { name: "banjo", program: band.program(Part::Pluck, &[], BANJO), channel: CH_BANJO, role: Role::Pluck, low: BANJO_STRINGS.0, high: DRONE.1, reverb: 30, pan: 30, level: -4.0 },
        Instrument { name: "piano", program: band.program(Part::Keys, &[], PIANO), channel: CH_KEYS, role: Role::Pluck, low: 33, high: KEYS_HAND.1, reverb: 35, pan: 14, level: -5.0 },
        Instrument { name: "gang", program: band.program(Part::Choir, &[], CHOIR), channel: CH_GANG, role: Role::Doubling, low: GANG.0, high: GANG.1, reverb: 55, pan: 0, level: -3.0 },
        Instrument { name: "second, the hook", program: second, channel: CH_HOOK_LOW, role: Role::Doubling, low: SECOND_LOW, high: SECOND_HIGH, reverb: 40, pan: -10, level: -3.0 },
        Instrument { name: "gang, the chant", program: band.program(Part::Choir, &[], CHOIR), channel: CH_HEY, role: Role::Pluck, low: GANG.0, high: GANG.1, reverb: 55, pan: 0, level: -2.0 },
        Instrument { name: "whistle, over the hook", program: band.program(Part::Doubler, &[], WHISTLE), channel: CH_OVER, role: Role::Doubling, low: 67, high: WHISTLE_HIGH, reverb: 45, pan: 20, level: -7.0 },
        Instrument { name: "kit", program: band.program(Part::Drums, &[], 0), channel: CH_DRUM, role: Role::Percussion, low: KICK, high: TAMBOURINE, reverb: 30, pan: 0, level: -3.0 },
    ];
    let mut score = Score::new(key, groove.meter(), tempo, instruments, ROOM_S);
    score.lead = Some(CH_LEAD);
    score.played_by(
        band,
        &[
            (CH_GUITAR, Part::Figure),
            (CH_GUITAR_HIGH, Part::Figure),
            (CH_OVER, Part::Doubler),
            (CH_LEAD, Part::Lead),
            (CH_SECOND, Part::Second),
            (CH_HOOK_LOW, Part::Second),
            (CH_BASS, Part::Bass),
            (CH_BANJO, Part::Pluck),
            (CH_KEYS, Part::Keys),
            (CH_GANG, Part::Choir),
            (CH_HEY, Part::Choir),
            (CH_DRUM, Part::Drums),
        ],
    );
    let bar = score.bar();

    // The walk in whole phrases, every part a whole number of them, so
    // the band changes only where the loop starts over.
    let walk = story.place(&mut skeleton, &mut score, 2, &[], |_, _| 1.0);
    let upper = story.ladder.len().max(1);
    for (section, part) in score.sections.iter_mut().zip(&walk.parts) {
        section.level = Some(LEVEL_FOOT * (1.0 - part.rung as f32 / upper as f32));
    }
    let bars = walk.bars();
    // The loop asks and answers by turns, counted back from the end, so
    // the song closes on its answer.
    let phrases = bars / phrase::BARS;
    let rows: Vec<&Schema> = (0..phrases).map(|p| if (phrases - 1 - p) % 2 == 1 { design.open } else { design.closed }).collect();
    // The verse sings its theme, the chorus its own, the chorus's line on
    // the whole a third over the verse's whatever the two shapes are.
    let chorus_at = |b: u32| walk.at(b).lead == Riff;
    let themes: Vec<&Theme> = (0..phrases).map(|p| if chorus_at(p * phrase::BARS) { &design.chorus } else { &design.theme }).collect();
    let mut tune = Tune::compose(&themes, &score.meter, design.form, &rows, 3, vec![0; bars as usize]);
    tune.scale = Some(tune::MAJOR_PENTATONIC);
    let mean = |chorus: bool| -> f32 {
        let tones = (0..bars).filter(|b| chorus_at(*b) == chorus).flat_map(|b| tune.bars[b as usize].iter());
        let (sum, len) = tones.fold((0, 0), |(s, n), t| (s + tune.on_scale(t.degree) * t.len as i32, n + t.len as i32));
        sum as f32 / len.max(1) as f32
    };
    let lift = (mean(false) - mean(true)).round() as i32 + CHORUS_SHIFT;
    tune.shifts = (0..bars).map(|b| if chorus_at(b) { lift } else { 0 }).collect();
    score.harmony = tune.chords.clone();
    score.summary = format!(
        "{} on the {}: a {:?} verse and a {:?} chorus in a {:?}, {} to ask and {} to answer; {} lead, {} second, {:?} under the hook; the strum ringing {} a bar; duet {:?}; opens {:?}, ends {:?}",
        story.name,
        groove.name,
        design.theme.shape,
        design.chorus.shape,
        design.form,
        design.open.name,
        design.closed.name,
        name(design.lead),
        name(design.second),
        design.under,
        if design.twice { "twice" } else { "once" },
        design.duet,
        design.intro,
        design.ending,
    )
    .to_lowercase();
    let mut opening = Tune::compose(&[&design.chorus], &score.meter, design.form, &rows[..1], 3, vec![lift; phrase::BARS as usize]);
    opening.scale = Some(tune::MAJOR_PENTATONIC);
    let form = Form { bar, walk, tune, hook: opening, design };

    // A bar before the band comes back all in builds into it.
    let builds: Vec<u32> = form.walk.parts.windows(2).filter(|w| w[1].rung > w[0].rung + 1).map(|w| w[0].b - 1).collect();
    for b in 0..bars {
        let t = form.walk.bed_at(b);
        let chord = form.tune.chords[b as usize];
        let next = form.tune.chords.get(b as usize + 1).copied().unwrap_or(Chord::triad(0));
        let prev = b.checked_sub(1).map(|p| form.tune.chords[p as usize]);
        let (role, turns) = (variation::role(b), variation::closes_pair(b));
        strum_bar(&mut score, t.strum, form.design.twice, b, chord, prev, next, role, turns, &mut rng.fork(1000 + b as u64));
        if t.bass != Bass::Off {
            bass_bar(&mut score, t.bass, b, chord, prev, next, role, &mut rng.fork(2000 + b as u64));
        }
        if t.colour {
            banjo_bar(&mut score, b, chord, next, role, turns, &mut rng.fork(5000 + b as u64));
            keys_bar(&mut score, b, chord, next, role, turns, &mut rng.fork(6000 + b as u64));
        }
        drum_bar(&mut score, form.design.groove, t, b, role, turns, builds.contains(&b), &mut rng.fork(3000 + b as u64));
    }
    let runs = form.walk.runs();
    let teller = Teller {
        lead: CH_LEAD,
        second: CH_SECOND,
        double: None,
        echo: CH_GANG,
        register: TUNE,
        sung: -4,
        riff: (4, -4),
        long: (62, 76, -10),
        // A bowed second's held tones, a voice under the lead's and never
        // a pad behind it.
        under: (55, 69, -6),
        hold: Hold::Ringing,
        // The phrase breathes at each half-phrase's last tone already; the
        // hook breathing a whole bar every two bars is a two-bar cell cut
        // short over and over.
        breathes: false,
        vel,
        fills: 0.0,
        soars: 0.0,
        // A sung line enters ahead of the beat: phrases start as pick-ups.
        pushes: 0.3,
    };
    teller::tell(&mut score, &teller, &form.tune, &runs, &mut rng.fork(8));
    land(&mut score, &form, &runs, &mut rng.fork(16));
    duet(&mut score, &form, &runs);
    for run in runs.iter().filter(|r| r.lead == Telling::Riff) {
        hook(&mut score, run.a * bar..run.b * bar, form.design.under, |tick| form.walk.bed_at(tick / bar).gang);
    }
    if form.design.under == Under::Held {
        let (mut last, mut bow) = (None, rng.fork(17));
        for run in runs.iter().filter(|r| matches!(r.lead, Telling::Patter | Telling::Riff)) {
            last = teller::held(&mut score, &teller, &form.tune, run, teller.under, CH_SECOND, last, None, &mut bow);
        }
    }
    gang(&mut score, &form, &mut rng.fork(9));
    score.mark_phrases(0, form.bars());
    ending(&mut score, &form, &mut rng.fork(12));
    intro(&mut score, &form, &teller, &mut rng.fork(13));
    if form.design.under == Under::Held {
        // A held tone left a leap from the last where no tone near it
        // keeps off the lead's perfect intervals.
        repair(&mut score, &form, CH_SECOND, (teller.under.0, teller.under.1));
    }
    score.finish();
    score
}

fn name(program: u8) -> &'static str {
    match program {
        PIANO => "piano",
        ACCORDION => "accordion",
        TRUMPET => "trumpet",
        WHISTLE => "whistle",
        CELLO => "cello",
        _ => "?",
    }
}

/// The degree the bass stands on under `chord`: its root, but the V's
/// third where it falls from vi or rises into I, so the bass steps vi,
/// V6, I — the first inversion I Will Wait and Dirty Paws turn on.
fn bass_degree(chord: Chord, prev: Option<Chord>, next: Chord) -> i32 {
    if chord.root == 4 && (prev.is_some_and(|p| p.root == 5) || next.root == 0) {
        6
    } else {
        chord.root
    }
}

/// The guitars' strings on `chord` over `bass`, the open guitar's and the
/// high one's. The open: the bass in the low octave, then the chord's
/// tones from a fourth over it up to G4. The high: the chord's top five
/// within `HIGH_STRINGS`.
fn strings(key: &Key, chord: Chord, bass: i32) -> [Vec<u8>; 2] {
    let low = (40..=51).find(|p| key.absolute_degree(*p).is_some_and(|d| d.rem_euclid(7) == bass.rem_euclid(7))).unwrap();
    let open = std::iter::once(low).chain(chord.pitches_within(key, low + 5, 67).into_iter().take(5)).collect();
    let high = chord.pitches_within(key, HIGH_STRINGS.0, HIGH_STRINGS.1);
    [open, high[high.len().saturating_sub(5)..].to_vec()]
}

/// The two guitars, the open one and the high one: two players
/// strumming the one part, a record's pair one each side.
const GUITARS: [u8; 2] = [CH_GUITAR, CH_GUITAR_HIGH];

/// A strum of both guitars across their `strings` from `at` ringing
/// `len`: down from the lowest string, the beat on it, a string every
/// few milliseconds; or up across the top three, softer.
fn strum(score: &mut Score, strings: &[Vec<u8>; 2], at: u32, len: u32, down: bool, accent: i32, rng: &mut Rng) {
    for (channel, strings) in GUITARS.into_iter().zip(strings) {
        strum_one(score, channel, strings, at, len, down, accent, rng);
    }
}

#[allow(clippy::too_many_arguments)]
fn strum_one(score: &mut Score, channel: u8, strings: &[u8], at: u32, len: u32, down: bool, accent: i32, rng: &mut Rng) {
    let order: Vec<u8> = if down { strings.to_vec() } else { strings.iter().rev().take(3).copied().collect() };
    let accent = if down { accent } else { accent - 12 };
    for (k, pitch) in order.into_iter().enumerate() {
        let k = k as u32;
        score.add(Note { start: at + k * STRUM, len: len.saturating_sub(k * STRUM).max(S / 2), pitch, vel: vel(accent - k as i32, rng), channel });
    }
}

/// The strokes of a bar at `strum`, as sixteenths, down or up, and their
/// accents. Ringing: once or twice, an up-stroke into the next bar in the
/// phrase's variant, the push — one, the and of two, four — in its
/// cadence. Quarters: down on each, the variant's last beat down-up.
/// Eighths: down, down-up, up, down-up, the variant's third beat down
/// alone. Drive: Little Lion Man's sixteenths, `X.X..XX.X.X..XX.`, the
/// variant I Will Wait's ten, the cadence straightening to eighths.
fn strokes(strum: Strum, twice: bool, role: Bar) -> Vec<(u32, bool, i32)> {
    let mut strokes: Vec<(u32, bool, i32)> = match strum {
        Strum::Ring => {
            let downs: &[u32] = match (role, twice) {
                (Bar::Cadence, _) => &[0, 6, 12],
                (_, true) => &[0, 8],
                (_, false) => &[0],
            };
            let mut v: Vec<(u32, bool, i32)> = downs.iter().map(|d| (*d, true, if *d == 0 { 0 } else { -6 })).collect();
            if role == Bar::Variant {
                v.push((14, false, -4));
            }
            v
        }
        Strum::Quarters => {
            let mut v: Vec<(u32, bool, i32)> = [0, 4, 8, 12].iter().map(|k| (*k, true, if *k == 0 { 0 } else { -6 })).collect();
            if matches!(role, Bar::Variant | Bar::Cadence) {
                v.push((14, false, -6));
            }
            v
        }
        Strum::Eighths => {
            let mut v = vec![(0, true, 0), (4, true, -6), (6, false, -6), (10, false, -6), (12, true, -6), (14, false, -6)];
            if role == Bar::Variant {
                v.retain(|(k, _, _)| *k != 12);
                v.push((8, true, -4));
            }
            v
        }
        Strum::Drive => {
            let grid = match role {
                Bar::Variant => "X.X..XXXX.X..XXX",
                Bar::Cadence => "X.X..XX.X.X.X.X.",
                _ => "X.X..XX.X.X..XX.",
            };
            grid.char_indices()
                .filter(|(_, c)| *c == 'X')
                .map(|(k, _)| {
                    let k = k as u32;
                    let accent = match k {
                        0 => 0,
                        8 => -3,
                        6 | 14 => -4,
                        _ => -9,
                    };
                    (k, k % 2 == 0, accent)
                })
                .collect()
        }
    };
    strokes.sort_by_key(|s| s.0);
    strokes
}

/// One bar of the guitar at `strum`. The bar that closes a question and
/// its answer strikes the next chord on its last stroke past the and of
/// four.
#[allow(clippy::too_many_arguments)]
fn strum_bar(score: &mut Score, strum_at: Strum, twice: bool, b: u32, chord: Chord, prev: Option<Chord>, next: Chord, role: Bar, turns: bool, rng: &mut Rng) {
    let key = score.key;
    let at = b * score.bar();
    let s = strings(&key, chord, bass_degree(chord, prev, next));
    let ahead = strings(&key, next, bass_degree(next, Some(chord), next));
    let all = strokes(strum_at, twice, role);
    for (i, (k, down, accent)) in all.iter().enumerate() {
        let end = all.get(i + 1).map_or(16, |s| s.0);
        let chord_strings = if turns && role == Bar::Cadence && *k >= 14 { &ahead } else { &s };
        strum(score, chord_strings, at + k * S, (end - k) * S, *down, *accent, rng);
    }
}

/// The tone a step from `into` on the side nearer `from`, within
/// `reach`: a bass's walk into the next chord.
fn approach(key: &Key, from: u8, into: u8, reach: (u8, u8)) -> u8 {
    let mut steps: Vec<u8> = Vec::new();
    for dir in [-1i32, 1] {
        let mut p = into as i32 + dir;
        while !key.contains(p as u8) {
            p += dir;
        }
        steps.push(p as u8);
    }
    steps.into_iter().filter(|p| (reach.0..=reach.1).contains(p)).min_by_key(|p| (*p as i32 - from as i32).abs()).unwrap_or(into)
}

/// The bass's tone of `degree`: in the octave nearest the key's home in
/// the bass, so the roots of a loop stay within a sixth of one another
/// rather than jumping a ninth where an octave's window wraps.
fn bass_tone(key: &Key, degree: i32) -> u8 {
    let home = (31..=42).find(|p| key.degree_of(*p) == Some(0)).unwrap();
    (28..=47).filter(|p| key.absolute_degree(*p).is_some_and(|d| d.rem_euclid(7) == degree.rem_euclid(7))).min_by_key(|p| (*p as i32 - home as i32).abs()).unwrap()
}

/// One bar of the bass on the roots (`stomp-holler-findings.md`, §4): a
/// whole note where the song is a verse, every quarter where it is a
/// chorus; the variant's second half the fifth, under the root where the
/// bass reaches it, the
/// cadence walking a step into the next chord's bass on its last eighth,
/// off the beat, where a passing tone belongs.
#[allow(clippy::too_many_arguments)]
fn bass_bar(score: &mut Score, bass: Bass, b: u32, chord: Chord, prev: Option<Chord>, next: Chord, role: Bar, rng: &mut Rng) {
    let key = score.key;
    let at = b * score.bar();
    let root = bass_tone(&key, bass_degree(chord, prev, next));
    let fifth = bass_tone(&key, chord.root + 4);
    let fifth = if fifth > root && fifth >= 40 { fifth - 12 } else { fifth };
    let into = approach(&key, root, bass_tone(&key, bass_degree(next, Some(chord), next)), (28, 52));
    let notes: Vec<(u32, u32, u8)> = match (bass, role) {
        (Bass::Long, Bar::Variant) => vec![(0, 4, root), (4, 4, fifth)],
        (Bass::Long, Bar::Cadence) => vec![(0, 7, root), (7, 1, into)],
        (Bass::Long, _) => vec![(0, 8, root)],
        (_, Bar::Variant) => vec![(0, 2, root), (2, 2, root), (4, 2, fifth), (6, 2, root)],
        (_, Bar::Cadence) => vec![(0, 2, root), (2, 2, root), (4, 2, root), (6, 1, root), (7, 1, into)],
        _ => vec![(0, 2, root), (2, 2, root), (4, 2, root), (6, 2, root)],
    };
    for (on, len, pitch) in notes {
        score.add(Note { start: at + on * E, len: len * E - E / 4, pitch, vel: vel(-8, rng), channel: CH_BASS });
    }
}

/// One bar of the banjo rolling sixteenths over the chord, its three
/// fretted strings and the drone over them (`stomp-holler-findings.md`,
/// §3): the forward roll, three and three and two; the variant rolling
/// forward and back; the cadence rolling its first half and pinching the
/// chord at its third beat, the bar closing a question and its answer
/// picking the next chord's low string last. A fretted string on every
/// beat, the drone between.
fn banjo_bar(score: &mut Score, b: u32, chord: Chord, next: Chord, role: Bar, turns: bool, rng: &mut Rng) {
    const FORWARD: [usize; 8] = [0, 1, 3, 0, 1, 3, 0, 3];
    const BACK: [usize; 8] = [0, 1, 2, 3, 2, 1, 0, 1];
    let key = score.key;
    let at = b * score.bar();
    let tones = chord.pitches_within(&key, BANJO_STRINGS.0, BANJO_STRINGS.1);
    let drone = (DRONE.0..=DRONE.1).find(|p| key.degree_of(*p) == Some(0)).unwrap();
    let strings = [tones[0], tones[1], tones[2], drone];
    let picks: Vec<(u32, usize)> = match role {
        Bar::Variant => (0..16).map(|k| (k, BACK[k as usize % 8])).collect(),
        Bar::Cadence => (0..8).map(|k| (k, FORWARD[k as usize])).collect(),
        _ => (0..16).map(|k| (k, FORWARD[k as usize % 8])).collect(),
    };
    for (k, i) in picks {
        let accent = if k % 4 == 0 { -6 } else if k % 2 == 1 { -16 } else { -12 };
        score.add(Note { start: at + k * S, len: S + S / 2, pitch: strings[i], vel: vel(accent, rng), channel: CH_BANJO });
    }
    if role == Bar::Cadence {
        for p in [strings[0], strings[2]] {
            score.add(Note { start: at + 8 * S, len: 6 * S, pitch: p, vel: vel(-8, rng), channel: CH_BANJO });
        }
        let last = if turns { next.pitches_within(&key, BANJO_STRINGS.0, BANJO_STRINGS.1)[0] } else { strings[1] };
        score.add(Note { start: at + 14 * S, len: 2 * S, pitch: last, vel: vel(-12, rng), channel: CH_BANJO });
    }
}

/// One bar of the piano on the quarters, as Ophelia's and Ho Hey's: the
/// chord in the right hand on every beat, over the guitars, the root in
/// the left on one and three; the variant's last beat two eighths; the bar closing a question
/// and its answer striking the next chord on its last eighth.
fn keys_bar(score: &mut Score, b: u32, chord: Chord, next: Chord, role: Bar, turns: bool, rng: &mut Rng) {
    let key = score.key;
    let at = b * score.bar();
    let hand = |c: Chord| -> Vec<u8> { c.pitches_within(&key, KEYS_HAND.0, KEYS_HAND.1).into_iter().take(3).collect() };
    let (right, ahead) = (hand(chord), hand(next));
    let root = (36..=47).find(|p| key.degree_of(*p) == Some(chord.root.rem_euclid(7) as usize)).unwrap();
    let mut beats: Vec<(u32, u32, &[u8])> = vec![(0, 2, &right), (2, 2, &right), (4, 2, &right), (6, 2, &right)];
    if role == Bar::Variant || (role == Bar::Cadence && turns) {
        beats.pop();
        beats.push((6, 1, &right));
        beats.push((7, 1, if role == Bar::Cadence { &ahead } else { &right }));
    }
    for (on, len, chord_tones) in beats {
        let accent = if on % 4 == 0 { -8 } else { -14 };
        for p in chord_tones {
            score.add(Note { start: at + on * E, len: len * E - E / 4, pitch: *p, vel: vel(accent, rng), channel: CH_KEYS });
        }
    }
    for on in [0, 4] {
        score.add(Note { start: at + on * E, len: 4 * E - E / 4, pitch: root, vel: vel(-10, rng), channel: CH_KEYS });
    }
}

/// One bar of the kit (`stomp-holler-findings.md`, §2): the kick on one,
/// or on every beat with the floor tom under it; the tambourine on every
/// off-beat, a sixteenth shaken into the next beat in the variant; the
/// gang's claps on every beat. The variant kicks a pick-up into the next
/// bar; the cadence strikes the floor tom on the and of four, or, where
/// it closes a question and its answer, runs it through its last half. A
/// bar building into the band all in stomps every eighth, rising.
#[allow(clippy::too_many_arguments)]
fn drum_bar(score: &mut Score, groove: &Groove, t: Texture, b: u32, role: Bar, turns: bool, builds: bool, rng: &mut Rng) {
    let at = b * score.bar();
    let hit = |score: &mut Score, e: u32, key: u8, accent: i32, rng: &mut Rng| score.add(Note { start: at + e, len: E, pitch: key, vel: vel(accent, rng), channel: CH_DRUM });
    if t.chick {
        for e in groove.tek {
            hit(score, e * E, TAMBOURINE, -14, rng);
        }
        if role == Bar::Variant {
            hit(score, 7 * E + S, TAMBOURINE, -20, rng);
        }
    }
    if t.gang {
        for e in groove.dum {
            hit(score, e * E, CLAP, -6, rng);
        }
    }
    if builds {
        for e in 0..8u32 {
            let accent = -20 + 3 * e as i32;
            hit(score, e * E, KICK, accent, rng);
            hit(score, e * E, FLOOR_TOM, accent - 2, rng);
        }
        return;
    }
    if t.kick == Kick::Off {
        return;
    }
    let fill = t.kick == Kick::Four && role == Bar::Cadence && turns;
    let beats: &[u32] = if t.kick == Kick::One { &[0] } else { groove.dum };
    for e in beats.iter().filter(|e| !fill || **e < 4) {
        hit(score, e * E, KICK, if *e == 0 { -2 } else { -6 }, rng);
        hit(score, e * E, FLOOR_TOM, -16, rng);
    }
    match role {
        // The variant's pick-up into the next downbeat, the cadence's
        // floor tom on the and of four: the galloping kick of Mountain
        // Sound, the push of Little Talks' chorus.
        Bar::Variant => hit(score, 7 * E, KICK, -10, rng),
        Bar::Cadence if fill => {
            for e in 4..8u32 {
                hit(score, e * E, FLOOR_TOM, -14 + 3 * (e as i32 - 4), rng);
            }
            hit(score, 4 * E, KICK, -6, rng);
            hit(score, 7 * E, KICK, -6, rng);
        }
        Bar::Cadence => hit(score, 7 * E, FLOOR_TOM, -10, rng),
        _ => {}
    }
}

/// The gang's chant where it is in, as Little Lion Man's three voices sing
/// half its bars and I Will Wait's "aahs" half of its: the chord's three
/// tones in open spacing, every other tone of it from the guitars' top
/// up, in one shared rhythm, every beat and two eighths into the next
/// bar, the variant's third beat two eighths, the cadence holding its
/// second half. In every two bars, on the first beat the tune leaves
/// silent, it shouts — the "Ho!" and the "Hey!" Ho Hey answers each line
/// with, short and hard — and the band hits with it: the kick, the clap
/// and the guitars' stroke struck harder (`stomp-holler-findings.md`,
/// §5). Where the band comes back all in, the gang shouts its downbeat
/// whether or not it sings on; in the last chorus it chants higher.
fn gang(score: &mut Score, form: &Form, rng: &mut Rng) {
    let bar = form.bar;
    let key = score.key;
    let silent = |score: &Score, t: u32| !score.notes.iter().any(|n| n.channel == CH_LEAD && n.start <= t && t < n.start + n.len);
    let mut shouts: Vec<u32> = Vec::new();
    for (i, part) in form.walk.parts.iter().enumerate() {
        if i > 0 && part.rung > form.walk.parts[i - 1].rung + 1 {
            shouts.push(part.a * bar);
        }
        if part.bed.gang {
            for pair in (part.a..part.b).step_by(2) {
                let beats = (0..8u32).map(|q| pair * bar + q * 2 * E).filter(|t| *t < part.b * bar);
                if let Some(t) = beats.into_iter().find(|t| silent(score, *t)) {
                    shouts.push(t);
                }
            }
        }
    }
    // The last chorus, from where the band last comes back all in, the
    // gang chants higher: the crest the song has saved for its end.
    let crest = form.walk.parts.windows(2).filter(|w| w[1].rung > w[0].rung + 1).last().map_or(u32::MAX, |w| w[1].a);
    let voices = |b: u32| -> Vec<u8> { chant(&key, form.tune.chords[b as usize], if b >= crest { CHANT_CREST } else { CHANT }) };
    for b in (0..form.bars()).filter(|b| form.walk.bed_at(*b).gang) {
        let at = b * bar;
        let rhythm: &[(u32, u32)] = match variation::role(b) {
            Bar::Variant => &[(0, 4), (4, 4), (8, 2), (10, 2), (12, 4)],
            Bar::Cadence => &[(0, 4), (4, 4), (8, 8)],
            _ => &[(0, 4), (4, 4), (8, 4), (12, 2), (14, 2)],
        };
        for (on, len) in rhythm {
            let t = at + on * S;
            let shout = shouts.contains(&t);
            let len = if shout { E } else { len * S - S / 2 };
            for p in voices(b) {
                score.add(Note { start: t, len, pitch: p, vel: vel(if shout { 8 } else { -10 }, rng), channel: CH_HEY });
            }
        }
    }
    for t in shouts {
        if !form.walk.bed_at(t / bar).gang {
            for p in voices(t / bar) {
                score.add(Note { start: t, len: E, pitch: p, vel: vel(8, rng), channel: CH_HEY });
            }
        }
        hits(score, t, rng);
    }
}

/// The band's hit under a shout at `t`: the kick and the clap where they
/// are not struck already, the guitars' stroke there struck harder.
fn hits(score: &mut Score, t: u32, rng: &mut Rng) {
    for (pitch, accent) in [(KICK, 2), (CLAP, 0)] {
        if !score.notes.iter().any(|n| n.channel == CH_DRUM && n.pitch == pitch && n.start == t) {
            score.add(Note { start: t, len: E, pitch, vel: vel(accent, rng), channel: CH_DRUM });
        }
    }
    let stroke = t..t + 6 * STRUM;
    for n in score.notes.iter_mut().filter(|n| GUITARS.contains(&n.channel) && stroke.contains(&n.start)) {
        n.vel = (n.vel + 10).min(127);
    }
}

/// The chord's three tones within `range` in open spacing: every other
/// tone of it from the range's floor.
fn chant(key: &Key, chord: Chord, range: (u8, u8)) -> Vec<u8> {
    chord.pitches_within(key, range.0, range.1).into_iter().step_by(2).take(3).collect()
}

/// Where a line runs to before it lands: a bar or two, the half-phrase;
/// three or four, the phrase; six to eight, the question run on into its
/// answer.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Line {
    Half,
    Phrase,
    Pair,
}

/// The lines the songs sing between their rests, weighted by the time
/// each length takes (`stomp-holler-findings.md`, §7: ten lines' bare
/// bars counted): a fifth of a song in lines of a bar or two, a quarter
/// in lines of three to five, over half in lines of six or more.
const LINES: [(Line, f32); 3] = [(Line::Half, 20.0), (Line::Phrase, 25.0), (Line::Pair, 55.0)];

/// The lead's lines landing: each question and its answer drawn a line
/// length (`LINES`), and each line's last bar landing on the tone its
/// phrase is going to, on the bar line, held half the bar and then a
/// rest — where the gang shouts and the next line takes its breath. A
/// line lands where its telling ends, too, whatever its length.
fn land(score: &mut Score, form: &Form, runs: &[Run<Telling>], rng: &mut Rng) {
    let bar = form.bar;
    let phrases = form.bars() / phrase::BARS;
    let (lo, hi) = (score.instrument(CH_LEAD).low, score.instrument(CH_LEAD).high);
    let weights: Vec<f32> = LINES.iter().map(|(_, w)| *w).collect();
    let mut ends: Vec<u32> = Vec::new();
    // The answers counted back from the end, as the loop's rows are.
    for p in (0..phrases).filter(|p| (phrases - 1 - p) % 2 == 0) {
        let question = p.checked_sub(1);
        let line = LINES[rng.weighted(&weights)].0;
        let mut pair: Vec<u32> = question.into_iter().chain([p]).collect();
        if line == Line::Pair {
            pair.retain(|q| *q == p);
        }
        for q in pair {
            let last = (q + 1) * phrase::BARS - 1;
            if line == Line::Half {
                ends.push(last - phrase::BARS / 2);
            }
            ends.push(last);
        }
    }
    ends.extend(runs.iter().filter(|r| matches!(r.lead, Telling::Patter | Telling::Riff)).map(|r| r.b - 1));
    ends.sort_unstable();
    ends.dedup();
    for b in ends {
        let (from, to) = (b * bar, (b + 1) * bar);
        let told: Vec<Note> = score.notes.iter().filter(|n| n.channel == CH_LEAD && (from..to).contains(&n.start)).copied().collect();
        let Some(goal) = told.last() else { continue };
        let chord = form.tune.chords[b as usize];
        let pitch = if chord.holds(&score.key, goal.pitch) { goal.pitch } else { tune::nearest_chord_tone(&score.key, chord, goal.pitch, lo, hi) };
        let vel = told[0].vel;
        score.notes.retain(|n| !(n.channel == CH_LEAD && (from..to).contains(&n.start)));
        for n in score.notes.iter_mut().filter(|n| n.channel == CH_LEAD && n.start < from && n.start + n.len > from) {
            n.len = from - n.start - E / 8;
        }
        score.add(Note { start: from, len: bar / 2 - E / 4, pitch, vel, channel: CH_LEAD });
    }
    // So a landing neither leaps on after the leap into it nor is left by
    // one.
    repair(score, form, CH_LEAD, TUNE);
}

/// `channel`'s every line, a rest of over a bar breaking it, repaired
/// within `range` as the teller repairs one: no leap on after a leap.
fn repair(score: &mut Score, form: &Form, channel: u8, range: (u8, u8)) {
    let bar = form.bar;
    let mut notes: Vec<usize> = (0..score.notes.len()).filter(|i| score.notes[*i].channel == channel).collect();
    notes.sort_by_key(|i| score.notes[*i].start);
    let mut lines: Vec<Vec<usize>> = Vec::new();
    for i in notes {
        match lines.last_mut() {
            Some(line) if score.notes[i].start <= score.notes[*line.last().unwrap()].end() + bar => line.push(i),
            _ => lines.push(vec![i]),
        }
    }
    for line in lines {
        let mut placed: Vec<Placed> = line.iter().map(|i| (score.notes[*i].start, score.notes[*i].len, score.notes[*i].pitch)).collect();
        form.tune.repair(score, &mut placed, range.0, range.1);
        for (i, p) in line.into_iter().zip(placed) {
            score.notes[i].pitch = p.2;
        }
    }
}

/// The second voice's turns where the tune is sung, played by the second
/// an octave under where its turn fits over its floor, else where the
/// lead had them — the two voices in conversation. Trading phrases, the
/// lead opens the run and they alternate. Trading two bars, the second
/// takes the question's half cadence and the answer's opening, so the
/// lead sings the question's opening and the answer's close and neither
/// voice repeats its half of the last phrase.
fn duet(score: &mut Score, form: &Form, runs: &[Run<Telling>]) {
    let bar = form.bar;
    let half = phrase::BARS / 2;
    let seconds = |run: &Run<Telling>| -> Vec<u32> {
        match form.design.duet {
            Duet::Halves => (run.a..run.b).step_by(half as usize).filter(|b| (b / phrase::BARS) % 2 != (b % phrase::BARS) / half).collect(),
            Duet::Phrases => (run.a + phrase::BARS..run.b).step_by(2 * phrase::BARS as usize).collect(),
            Duet::Alone => Vec::new(),
        }
    };
    let unit = if form.design.duet == Duet::Halves { half } else { phrase::BARS };
    for run in runs.iter().filter(|r| r.lead == Telling::Patter) {
        for a in seconds(run) {
            let z = (a + unit).min(run.b);
            let span = a * bar..z * bar;
            let turn: Vec<usize> = (0..score.notes.len()).filter(|i| score.notes[*i].channel == CH_LEAD && span.contains(&score.notes[*i].start)).collect();
            let fits = turn.iter().all(|i| score.notes[*i].pitch >= SECOND_LOW + 12);
            for i in turn {
                let n = &mut score.notes[i];
                n.channel = CH_SECOND;
                if fits {
                    n.pitch -= 12;
                }
            }
        }
    }
}

/// The hook as the band plays it: every note the lead plays in `span`
/// joined by the second voice as `under` says, in unison where it
/// reaches, else an octave under where that keeps over its floor; a
/// bowed second holds under it instead (`Under::Held`). Where `gang`
/// holds at the note, the room sings along: the gang on it and an octave
/// under, the second in unison, the octave under the gang's, and the
/// whistle an octave over where the band brings one.
fn hook(score: &mut Score, span: std::ops::Range<u32>, under: Under, gang: impl Fn(u32) -> bool) {
    let lead: Vec<Note> = score.notes.iter().filter(|n| n.channel == CH_LEAD && span.contains(&n.start)).copied().collect();
    for n in lead {
        let sung = gang(n.start);
        if under != Under::Held {
            let octave = (under == Under::Octave && !sung) || n.pitch > SECOND_HIGH;
            let low = if octave && n.pitch >= SECOND_LOW + 12 { n.pitch - 12 } else { n.pitch };
            score.add(Note { pitch: low, vel: n.vel.saturating_sub(4), channel: CH_HOOK_LOW, ..n });
        }
        if !sung {
            continue;
        }
        for pitch in [n.pitch, n.pitch.saturating_sub(12)].into_iter().filter(|p| (GANG.0..=GANG.1).contains(p)) {
            score.add(Note { pitch, vel: n.vel.saturating_sub(2), channel: CH_GANG, ..n });
        }
        if n.pitch + 12 <= WHISTLE_HIGH {
            score.add(Note { pitch: n.pitch + 12, vel: n.vel.saturating_sub(6), channel: CH_OVER, ..n });
        }
    }
}

/// The bars the opening hook takes: the chorus's first phrase.
const INTRO_BARS: u32 = phrase::BARS;
/// The bars an ending plays out before its last stroke, and the bars the
/// last stroke rings.
const OUTRO_BARS: u32 = 4;
const RING_BARS: u32 = 3;

/// The opening, before the walk. The hook: the tune's first phrase over
/// the strum, the second voice on it, as Little Talks and Home open. The
/// strum: two bars or four of the guitar alone, as the walk's foot plays
/// it.
fn intro(score: &mut Score, form: &Form, teller: &Teller, rng: &mut Rng) {
    let bar = form.bar;
    let n = match form.design.intro {
        Intro::Hook => INTRO_BARS,
        Intro::Strum => [2, 4][rng.below(2)],
    };
    let chords: Vec<Chord> = form.tune.chords[..n as usize].to_vec();
    // The hook's notes, written where the walk's first bars were, before
    // the walk moves on past them.
    let riff: Vec<(u32, u32, u8)> = if form.design.intro == Intro::Hook { form.hook.run(score, 0, n, TUNE.0, TUNE.1, false) } else { Vec::new() };
    score.delay(&chords);
    let (name, level, strum_at) = match form.design.intro {
        Intro::Hook => ("hook", LEVEL_FOOT / 2.0, form.walk.bed_at(0).strum.max(Strum::Eighths)),
        Intro::Strum => ("strum", LEVEL_FOOT, form.walk.bed_at(0).strum),
    };
    score.sections.insert(0, Section { name, start: 0, end: n * bar, trim: 1.0, level: Some(level), rings: false });
    for b in 0..n {
        let chord = chords[b as usize];
        let next = form.tune.chords.get(b as usize + 1).copied().unwrap_or(chord);
        let prev = b.checked_sub(1).map(|p| chords[p as usize]);
        strum_bar(score, strum_at, form.design.twice, b, chord, prev, next, variation::role(b), variation::closes_pair(b), rng);
    }
    for (i, (start, len, pitch)) in riff.iter().enumerate() {
        // Detached to its feet, a tone filling its bar held, the last
        // breathing before the walk comes in.
        let held = if *len >= bar { len - E } else if *len >= 3 * E { 2 * E } else { len - E / 2 };
        let accent = if score.strong(*start) { 4 } else { -4 };
        let held = if i + 1 == riff.len() { held.min(n * bar - start - E) } else { held };
        score.add(Note { start: *start, len: held, pitch: *pitch, vel: vel(accent, rng), channel: CH_LEAD });
    }
    hook(score, 0..n * bar, form.design.under, |_| false);
    if form.design.intro == Intro::Hook && form.design.under == Under::Held {
        let run = Run { a: 0, b: n, lead: Telling::Riff };
        teller::held(score, teller, &form.hook, &run, teller.under, CH_SECOND, None, None, rng);
    }
}

/// The ending, after the walk's last part (see `Ending`), then the last
/// stroke of the tonic ringing; slowing into it where the song slows.
fn ending(score: &mut Score, form: &Form, rng: &mut Rng) {
    let bar = form.bar;
    let key = score.key;
    let walked = form.bars();
    let mut hit = walked * bar;
    match form.design.ending {
        Ending::Thinned => {
            let chords: Vec<Chord> = (0..4).map(|k| form.design.closed.chord(k, 3)).collect();
            score.sections.push(Section { name: "thinned", start: hit, end: hit + OUTRO_BARS * bar, trim: 1.0, level: Some(LEVEL_FOOT), rings: false });
            score.harmony.extend(chords.iter().copied());
            for k in 0..OUTRO_BARS {
                let chord = chords[k as usize];
                let next = chords.get(k as usize + 1).copied().unwrap_or(Chord::triad(0));
                let prev = k.checked_sub(1).map(|p| chords[p as usize]);
                strum_bar(score, Strum::Ring, true, walked + k, chord, prev, next, variation::role(k), false, rng);
                for e in form.design.groove.tek {
                    score.add(Note { start: (walked + k) * bar + e * E, len: E, pitch: TAMBOURINE, vel: vel(-16, rng), channel: CH_DRUM });
                }
            }
            hit += OUTRO_BARS * bar;
        }
        Ending::Gang => {
            // The walk's last four bars of the hook again, sung by the
            // lead and the gang over their claps, the band gone.
            let from = hit - OUTRO_BARS * bar;
            for n in score.notes.iter_mut().filter(|n| n.start < hit && n.start + n.len > hit) {
                n.len = hit - n.start;
            }
            let line: Vec<Note> = score.notes.iter().filter(|n| n.channel == CH_LEAD && (from..hit).contains(&n.start)).copied().collect();
            score.sections.push(Section { name: "gang", start: hit, end: hit + OUTRO_BARS * bar, trim: 1.0, level: Some(LEVEL_FOOT / 2.0), rings: false });
            score.harmony.extend(form.tune.chords[(walked - OUTRO_BARS) as usize..walked as usize].iter().copied());
            let end = hit + OUTRO_BARS * bar;
            for n in line {
                let start = n.start + OUTRO_BARS * bar;
                score.add(Note { start, len: n.len.min(end - E - start), ..n });
            }
            hook(score, hit..end, form.design.under, |_| true);
            for q in 0..OUTRO_BARS * 4 {
                score.add(Note { start: hit + q * 2 * E, len: E, pitch: CLAP, vel: vel(-4, rng), channel: CH_DRUM });
            }
            hit = end;
        }
        Ending::Stomp => {}
    }
    score.mark_coda(hit / bar);
    if form.design.slows {
        let slowest = rng.range(78, 90) as f32 / 100.0;
        score.ritardando(hit - 2 * bar, hit, slowest);
    }
    let last = Chord::triad(0);
    // The last stroke at the level it is struck at, never raised to meet
    // a section's: a lone chord lifted to one jumps over the song.
    score.sections.push(Section { name: "end", start: hit, end: hit + RING_BARS * bar, trim: 1.0, level: None, rings: true });
    score.harmony.extend((0..RING_BARS).map(|_| last));
    let ring = RING_BARS * bar - E;
    strum(score, &strings(&key, last, last.root), hit, ring, true, 4, rng);
    if form.design.ending != Ending::Stomp {
        return;
    }
    // The whole band on one stomp of the tonic: the bass's root, the
    // piano's chord, the kick, the floor tom and the clap, the gang's
    // shout, the lead holding the chord's tone nearest home.
    let t = form.walk.bed_at(walked - 1);
    if t.bass != Bass::Off {
        let root = bass_tone(&key, 0);
        score.add(Note { start: hit, len: ring, pitch: root, vel: vel(-4, rng), channel: CH_BASS });
    }
    if t.colour {
        for p in last.pitches_within(&key, KEYS_HAND.0, KEYS_HAND.1).into_iter().take(3) {
            score.add(Note { start: hit, len: ring, pitch: p, vel: vel(-6, rng), channel: CH_KEYS });
        }
    }
    for (pitch, accent) in [(KICK, 4), (FLOOR_TOM, -2), (CLAP, 0)] {
        score.add(Note { start: hit, len: E, pitch, vel: vel(accent, rng), channel: CH_DRUM });
    }
    for p in chant(&key, last, CHANT) {
        score.add(Note { start: hit, len: E, pitch: p, vel: vel(6, rng), channel: CH_HEY });
    }
    let home = tune::home_tonic(&key, TUNE.0, TUNE.1);
    let tone = tune::nearest_chord_tone(&key, last, home, TUNE.0, TUNE.1);
    score.add(Note { start: hit, len: 2 * bar - E, pitch: tone, vel: vel(-4, rng), channel: CH_LEAD });
}
