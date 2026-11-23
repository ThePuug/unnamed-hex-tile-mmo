//! The overworld's music: a place told on a dance, from a drone. Every
//! choice is a draw from the seed's stream, one fork per purpose, and the
//! first forks decide what kind of piece this is — in what mode and
//! tonic, on which dance, with which instruments, the order its band
//! gathers in; whether the dance is there from the start, steps in, is a
//! plucked figure with no drum or no pulse at all; whether its tempo
//! holds or presses on; how it opens and how it ends — each as often as
//! the exploration music and the dance records surveyed do it
//! (`proofs/research/overworld-findings.md`), so two seeds are two pieces
//! of one place.
//!
//! The lead plays first. The piece is about its motif, and the lead
//! improvises its distance from it a half-phrase at a time (`improv`),
//! taking each phrase's schema as it goes. The band is written after and
//! follows where the lead stands (`gathered`): how many play, the roll
//! and the step up behind a run back, the stroke the motif lands on, the
//! silence after the lead breaks off, and the level, which the pedal lets
//! move with the band (`SWELL`) where it gives back the rest of what each
//! layer adds. Nothing of the band is placed by the bar count: a fill
//! that comes every phrase says nothing, and one that comes because the
//! lead is on its way back says so. What the band plays is the lead's
//! too, bar by bar: the plucks and the drum's fingers strike where the
//! lead moves off the beat, the second pluck a third under its tones,
//! and where the lead holds they take the figure's eighths it leaves
//! open; the weave says the motif back in the bar the lead rests after
//! it, a second holds tones under the motif said higher, and the bed
//! — drum, plucks, strings, breath — plays the dance under it.

use crate::ladder::{self, Run, Walk};
use crate::band::Part;
use crate::improv::{self, Feel, Stance};
use crate::pieces::Params;
use crate::rng::Rng;
use crate::score::{Instrument, Note, Role, Score, Section, TICKS_PER_EIGHTH as E};
use crate::teller::{self, Hold, Teller, Telling};
use crate::theory::groove::{Groove, BALKAN};
use crate::theory::phrase;
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

/// Who may lead: the players that can hold a line and shape it. Each
/// seed draws one.
const LEADS: [u8; 5] = [FLUTE, PAN_FLUTE, FIDDLE, CLARINET, ENGLISH_HORN];
/// The lead's level, dB, whichever it is: forward of any one voice of
/// the bed, a little under the bed as a whole. The render evens what
/// the bank gives each player, so one level holds for every lead.
const LEAD: f32 = 3.0;

/// Who may hold tones under the lead's riff.
const SECONDS: [u8; 4] = [FIDDLE, CLARINET, ACCORDION, ENGLISH_HORN];

/// The velocity every voice strikes at before its own accent: one
/// dynamic for the whole bed, since its level is the pedal's to move.
const VEL: i32 = 85;

/// The frame drum's level, dB: its most struck stroke's, under the
/// band by what holds it where the bed's balance has it.
const DRUM_LEVEL: f32 = -3.0;

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
/// a line moves from two degrees under home to an octave over it and
/// never leaves the register.
const TUNE: (u8, u8) = (62, 88);
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
    /// The bass on the dance's strong beats and the chord where the lead
    /// moves, both soft.
    Accents,
    /// Both at full weight.
    Full,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Drum {
    Off,
    /// The dum, and the fingers where the lead moves.
    Accents,
    /// The dance's teks too.
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

/// A layer of the bed, the thing that joins as the band gathers.
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

impl Texture {
    /// The bed with `layer` one notch up; the same bed where it is
    /// already at its top.
    fn up(self, layer: Layer) -> Texture {
        match layer {
            Layer::Pad => Texture { pad: if self.pad == Pad::Off { Pad::Thin } else { Pad::Full }, ..self },
            Layer::Plucks => Texture { plucks: if self.plucks == Plucks::Off { Plucks::Accents } else { Plucks::Full }, ..self },
            Layer::Drum => Texture { drum: if self.drum == Drum::Off { Drum::Accents } else { Drum::Full }, ..self },
            Layer::Colours => Texture { colours: true, ..self },
            Layer::Weave => Texture { weave: if self.weave == Weave::Off { Weave::One } else { Weave::Two }, ..self },
        }
    }
}

use Layer::{Colours, Drum as DrumLayer, Pad as PadLayer, Plucks as PlucksLayer, Weave as WeaveLayer};
/// How a band gathers: what plays where it is barest, and the layers in
/// the order they join as it comes in behind the lead, each a notch.
struct Gathering {
    name: &'static str,
    /// How often the seed picks it, against the others.
    weight: f32,
    base: Texture,
    ladder: &'static [Layer],
}

impl Gathering {
    /// The bed with the band `gathered` this far, 0 its barest and 1 all
    /// of it.
    fn bed(&self, gathered: f32) -> Texture {
        let rung = (gathered * self.ladder.len() as f32).round() as usize;
        self.ladder[..rung.min(self.ladder.len())].iter().fold(self.base, |t, l| t.up(*l))
    }
}

const GATHERINGS: [Gathering; 6] = [
    Gathering { name: "arc", weight: 3.0, base: Texture::BARE, ladder: &[PadLayer, DrumLayer, PadLayer, PlucksLayer, WeaveLayer, Colours, PlucksLayer] },
    Gathering { name: "two waves", weight: 2.0, base: Texture::BARE, ladder: &[PadLayer, DrumLayer, PlucksLayer, WeaveLayer, PadLayer, Colours] },
    Gathering { name: "slow burn", weight: 2.0, base: Texture::BARE, ladder: &[PadLayer, DrumLayer, PadLayer, WeaveLayer, PlucksLayer, Colours] },
    Gathering { name: "lament", weight: 2.0, base: Texture::BARE, ladder: &[PadLayer, PlucksLayer, PadLayer, WeaveLayer, Colours] },
    Gathering {
        name: "dance",
        weight: 2.0,
        base: Texture { pad: Pad::Off, plucks: Plucks::Full, drum: Drum::Accents, colours: false, weave: Weave::Off },
        ladder: &[PadLayer, WeaveLayer, PadLayer, DrumLayer, Colours],
    },
    Gathering {
        name: "fragments",
        weight: 1.5,
        base: Texture { pad: Pad::Thin, plucks: Plucks::Off, drum: Drum::Off, colours: false, weave: Weave::Off },
        ladder: &[PlucksLayer, WeaveLayer, PlucksLayer, PadLayer, Colours],
    },
];

/// How the lead tells a half-phrase it stands to its motif so: sung, but
/// the motif higher, which it plays detached over the second's held
/// tones with the strings on it, and its run back, detached.
fn telling(stance: Stance) -> Telling {
    match stance {
        Stance::Lifts => Telling::RiffAndLong,
        Stance::Returns => Telling::Riff,
        _ => Telling::Phrases,
    }
}

/// How far the band is gathered through each bar of a play of `stances`,
/// 0 its barest and 1 all of it. It follows the lead: a little more with
/// each statement the play opens on; all the more where the lead says the
/// motif higher; thinner where it answers and thinnest where it leaves;
/// coming in bar by bar behind a run back; out, but for the drone and the
/// breath, in the silence after the lead breaks off; landing with the
/// motif where a run or a break led to it, a little fuller each time; and
/// settling under the last statement.
fn gathered(stances: &[Stance]) -> Vec<f32> {
    let mut out = Vec::new();
    let mut e = 0.2f32;
    let mut landings = 0;
    for (h, stance) in stances.iter().enumerate() {
        let prev = h.checked_sub(1).map(|p| stances[p]);
        let (first, second) = match stance {
            Stance::States => {
                e = match prev {
                    Some(Stance::Returns | Stance::Breaks) => {
                        landings += 1;
                        (0.5 + 0.1 * landings as f32).min(0.9)
                    }
                    // The last statement settles; the first few gather.
                    Some(Stance::States) if h + 1 == stances.len() => (e - 0.15).max(0.3),
                    None | Some(Stance::States) => (e + 0.1).min(0.7),
                    Some(Stance::Lifts) => (e - 0.15).max(0.4),
                    _ => e.max(0.45),
                };
                (e, e)
            }
            Stance::Lifts => {
                e = (e + 0.15).min(1.0);
                (e, e)
            }
            Stance::Answers => {
                e = (e - 0.15).max(0.3);
                (e, e)
            }
            Stance::Leaves => {
                e = (e - 0.3).max(0.2);
                (e, e)
            }
            Stance::Breaks => ((e - 0.2).max(0.15), 0.0),
            Stance::Returns => {
                let first = (e + 0.1).min(1.0);
                e = (e + 0.25).min(1.0);
                (first, e)
            }
        };
        out.extend([first, second]);
    }
    out
}

/// How far the level moves with the band, LU, from its barest to all of
/// it: what the pedal lets through of what the layers add, so a return
/// is heard to land and a leaving to fall away.
const SWELL: f32 = 5.0;

/// Whether the piece has a pulse, and what: the dance on the plucks and
/// the drum from its start; the dance with its drum stepping in a quarter
/// to two fifths through; the plucks' figure with no drum; or no pulse,
/// the plucks touching an accent every other bar under the pad and the
/// lead. Field music is mostly the last two, dance records the first.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Pulse {
    Dance,
    StepsIn,
    Figure,
    Free,
}

impl Pulse {
    fn drums(self) -> bool {
        matches!(self, Pulse::Dance | Pulse::StepsIn)
    }
}

/// How the piece opens: the bed fading in from the drone; the lead's
/// prelude alone over the drone; the dance first; the plucks' figure
/// alone; or the lead's short call.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Intro {
    Fade,
    Prelude,
    Dance,
    Figure,
    Call,
}

impl Intro {
    fn name(self) -> &'static str {
        match self {
            Intro::Fade => "fade",
            Intro::Prelude => "prelude",
            Intro::Dance => "dance first",
            Intro::Figure => "figure first",
            Intro::Call => "call",
        }
    }
}

/// How the piece ends: the bed thinning to the drone, which rings on; the
/// lead alone over the drone, slowing, holding home; the pad and the
/// choir holding the last chord; the drum's and the plucks' stroke; or
/// the last bar played whole and cut.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Ending {
    Decay,
    Alone,
    Cadence,
    Stroke,
    Stop,
}

impl Ending {
    fn name(self) -> &'static str {
        match self {
            Ending::Decay => "decay",
            Ending::Alone => "alone",
            Ending::Cadence => "cadence",
            Ending::Stroke => "stroke",
            Ending::Stop => "stop",
        }
    }
}

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
    /// The play's rhythms, the lead's and the band's.
    feel: Feel,
    /// The rows the mode has that the drone holds: the questions and the
    /// answers the lead takes its harmony from.
    open: Vec<&'static Schema>,
    closed: Vec<&'static Schema>,
    pulse: Pulse,
    /// Whether the tempo presses on from a little past halfway, and to
    /// how much faster.
    ramp: Option<f32>,
    intro: Intro,
    ending: Ending,
    /// Whether the band slows into its last bars.
    slows: bool,
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
    /// Where the lead stands to its motif, a half-phrase each.
    stances: Vec<Stance>,
    /// The bar the drum steps in at where it steps in.
    steps_in: u32,
}

impl Form {
    /// The bed at `bar`: the band's as it follows the lead, as the pulse
    /// has it, emptied where the lead has broken off or the ending
    /// empties it.
    fn texture_at(&self, bar: u32) -> Texture {
        if self.spaces(bar) {
            return Texture::BARE;
        }
        let mut t = self.walk.bed_at(bar);
        match self.design.pulse {
            Pulse::Dance => {
                if t.plucks == Plucks::Off {
                    t.plucks = Plucks::Accents;
                }
                if t.drum == Drum::Off {
                    t.drum = Drum::Accents;
                }
            }
            Pulse::StepsIn if bar < self.steps_in => t.drum = Drum::Off,
            Pulse::StepsIn if t.drum == Drum::Off => t.drum = Drum::Accents,
            Pulse::StepsIn => {}
            Pulse::Figure => t.drum = Drum::Off,
            Pulse::Free => {
                t.drum = Drum::Off;
                if t.plucks == Plucks::Full {
                    t.plucks = Plucks::Accents;
                }
            }
        }
        let at = self.bars();
        let alone = self.design.ending == Ending::Alone && bar + phrase::BARS / 2 >= at;
        let decayed = self.design.ending == Ending::Decay && bar + phrase::BARS / 2 >= at;
        if alone || decayed {
            return Texture::BARE;
        }
        if self.design.ending == Ending::Decay && bar + phrase::BARS >= at {
            t = Texture { plucks: Plucks::Off, drum: Drum::Off, colours: false, weave: Weave::Off, ..t };
        }
        t
    }
    fn stance(&self, bar: u32) -> Option<Stance> {
        self.stances.get(bar as usize / 2).copied()
    }
    /// Whether `bar` is the silence after the lead breaks off, where the
    /// band opens the space the motif is missed in.
    fn spaces(&self, bar: u32) -> bool {
        bar % 2 == 1 && self.stance(bar) == Some(Stance::Breaks)
    }
    /// Whether `bar` is the last of a run into the motif: where the band
    /// leans in, the drum rolling and the bass stepping up to it.
    fn gathers(&self, bar: u32) -> bool {
        bar % 2 == 1 && self.stance(bar) == Some(Stance::Returns) && self.stance(bar + 1).is_some_and(|s| s.arrives())
    }
    /// Whether the motif lands on `bar`, come back to by a run or after a
    /// break: where the band comes down on it together.
    fn lands(&self, bar: u32) -> bool {
        bar % 2 == 0 && bar > 0 && self.stance(bar).is_some_and(|s| s.arrives()) && matches!(self.stance(bar - 1), Some(Stance::Returns | Stance::Breaks))
    }
    /// The eighths off the beat the band strikes in `bar`: the lead's own
    /// where it moves off the beat twice or more, so the band is in the
    /// rhythm the lead is playing, whatever that is; else the figure's
    /// that the lead leaves open, the band answering where it holds.
    fn strikes(&self, bar: u32) -> Vec<u32> {
        let meter = self.design.groove.meter();
        let lead: Vec<u32> = self.tune.bars[bar as usize].iter().map(|t| t.onset).filter(|onset| !meter.strong(*onset)).collect();
        if lead.len() >= 2 {
            return lead;
        }
        self.design.feel.strikes(self.design.groove).into_iter().filter(|e| !lead.contains(e)).collect()
    }
    /// Whether `bar` is where the lead rests after its motif or an
    /// answer, the bar the weave echoes it in.
    fn echoes(&self, bar: u32) -> bool {
        bar % 2 == 1 && matches!(self.stance(bar), Some(Stance::States | Stance::Lifts | Stance::Answers))
    }
    fn runs(&self) -> Vec<Run<Telling>> {
        self.walk.runs()
    }
    fn chord(&self, bar: u32) -> Chord {
        self.tune.chords[bar as usize]
    }
    fn bars(&self) -> u32 {
        self.walk.bars()
    }
}

pub fn build(params: &Params) -> Score {
    compose(params).0
}

/// The score, and the form it was written on.
fn compose(params: &Params) -> (Score, Form) {
    let rng = Rng::new(params.seed);
    let mut skeleton = rng.fork(0);
    // The band: the place's players.
    let band = params.band;
    let gatherings: Vec<f32> = GATHERINGS.iter().map(|g| g.weight * band.lean(g.name)).collect();
    let gathering = &GATHERINGS[skeleton.weighted(&gatherings)];
    // Never the harmonic minor: its leading tone sits a semitone under
    // the tonic drone in every chord that carries it.
    let mode = [Mode::Aeolian, Mode::Dorian, Mode::Hijaz][skeleton.weighted(&[4.0, 2.5, 2.5])];
    let key = Key::new(["D", "E", "G", "A", "C"][skeleton.weighted(&[3.0, 2.0, 2.0, 2.0, 1.0])], mode);
    // The slow dances, for a wander; the quick ones are other pieces'.
    let groove = &BALKAN[skeleton.weighted(&[4.0, 0.0, 0.0, 2.0, 1.0f32].iter().enumerate().map(|(i, w)| w * band.lean(BALKAN[i].name)).collect::<Vec<f32>>())];
    let tempo = {
        let (lo, hi) = (groove.tempo.0 as f32, groove.tempo.1 as f32);
        let (a, b) = ladder::UNBOUNDED.within(band.prefs.tempo).tempo;
        skeleton.range((lo + (hi - lo) * a).round() as i32, (lo + (hi - lo) * b).round() as i32) as f32
    };
    // At least one colour, since a gathering may bring the colours in.
    let colours = skeleton.range(1, 7);
    let (open, closed) = schemata_for(&key);
    // The band's first lead the overworld has a voice for, and its first
    // second besides.
    let teller = band.program(Part::Lead, &LEADS, LEADS[0]);
    let drone_voice = band.program(Part::Drone, &[], CONTRABASS);
    let pad_voice = band.program(Part::Pad, &[], STRINGS_1);
    let second = band.programs(Part::Second).into_iter().chain(SECONDS).find(|p| *p != teller && SECONDS.contains(p)).unwrap_or(SECONDS[0]);
    let pluck = band.program(Part::Pluck, &[], NYLON_GUITAR);
    let pluck_2 = band.program(Part::Pluck2, &[], NYLON_GUITAR);
    let feel = Feel::draw(groove, &mut skeleton);
    // The piece's identity, on a stream of its own; how it opens and how
    // it ends as the band likes to, its house ways.
    let mut habits = rng.fork(14);
    let mut pulse = [Pulse::Dance, Pulse::StepsIn, Pulse::Figure, Pulse::Free][habits.weighted(&[30.0, 30.0, 20.0, 20.0])];
    let intros = [Intro::Fade, Intro::Prelude, Intro::Dance, Intro::Figure, Intro::Call];
    let opening = intros[habits.weighted(&intros.iter().zip([35.0, 20.0, 20.0, 15.0, 10.0]).map(|(i, w)| w * band.lean(i.name())).collect::<Vec<f32>>())];
    // The figure first is the plucks' and wants them playing.
    if opening == Intro::Figure && pulse == Pulse::Free {
        pulse = Pulse::Figure;
    }
    // The tempo presses on in a dance's records.
    let ramp = (pulse.drums() && habits.chance(0.2)).then(|| habits.range(110, 120) as f32 / 100.0);
    let endings = [Ending::Decay, Ending::Alone, Ending::Cadence, Ending::Stroke, Ending::Stop];
    let close = endings[habits.weighted(&endings.iter().zip([40.0, 20.0, 15.0, 20.0, 5.0]).map(|(e, w)| w * band.lean(e.name())).collect::<Vec<f32>>())];
    // The stroke is the drum's.
    let close = if close == Ending::Stroke && !pulse.drums() { Ending::Decay } else { close };
    let slows = close == Ending::Alone || (close != Ending::Stop && habits.chance(0.15));
    let design = Design {
        drone: drone_voice,
        pad: pad_voice,
        lead: teller,
        second,
        pluck,
        pluck_2,
        choir: colours & 1 != 0,
        horn: colours & 2 != 0,
        shimmer: colours & 4 != 0,
        groove,
        feel,
        open,
        closed,
        pulse,
        ramp,
        intro: opening,
        ending: close,
        slows,
    };
    let instruments = vec![
        Instrument { name: "drone", program: design.drone, channel: CH_DRONE, role: Role::Drone, low: 24, high: 60, reverb: 40, pan: 0, level: 0.0 },
        Instrument { name: "strings", program: design.pad, channel: CH_PAD, role: Role::Sustain, low: 48, high: 79, reverb: 100, pan: -29, level: 0.0 },
        Instrument { name: "lead", program: design.lead, channel: CH_LEAD, role: Role::Melody, low: 62, high: 91, reverb: 40, pan: 0, level: LEAD },
        Instrument { name: "pluck", program: design.pluck, channel: CH_PLUCK, role: Role::Pluck, low: 45, high: 74, reverb: 30, pan: 34, level: 0.0 },
        Instrument { name: "second", program: design.second, channel: CH_SECOND, role: Role::Melody, low: 55, high: 88, reverb: 60, pan: 21, level: 0.0 },
        Instrument { name: "choir", program: band.program(Part::Choir, &[], CHOIR_AAHS), channel: CH_CHOIR, role: Role::Sustain, low: 55, high: 72, reverb: 100, pan: 23, level: 0.0 },
        Instrument { name: "horn", program: band.program(Part::Horn, &[], FRENCH_HORN), channel: CH_HORN, role: Role::Sustain, low: 48, high: 67, reverb: 55, pan: -39, level: 0.0 },
        Instrument { name: "strings, the riff", program: STRINGS_2, channel: CH_DOUBLE, role: Role::Doubling, low: 55, high: 88, reverb: 60, pan: -34, level: 0.0 },
        Instrument { name: "shimmer", program: band.program(Part::Shimmer, &[], DULCIMER), channel: CH_SHIMMER, role: Role::Pluck, low: 62, high: 91, reverb: 50, pan: 49, level: 0.0 },
        Instrument { name: "frame drum", program: band.program(Part::Drums, &[], 0), channel: CH_DRUM, role: Role::Percussion, low: 60, high: 64, reverb: 30, pan: -8, level: DRUM_LEVEL },
        Instrument { name: "pluck 2", program: design.pluck_2, channel: CH_PLUCK_2, role: Role::Pluck, low: 55, high: 72, reverb: 30, pan: -44, level: 0.0 },
        Instrument { name: "weave", program: DULCIMER, channel: CH_WEAVE, role: Role::Pluck, low: 50, high: 81, reverb: 35, pan: -52, level: 0.0 },
        Instrument { name: "weave, the harp", program: HARP, channel: CH_WEAVE_2, role: Role::Pluck, low: 55, high: 88, reverb: 40, pan: 55, level: 0.0 },
        Instrument { name: "strings, alone", program: STRINGS_2, channel: CH_PAD_ALONE, role: Role::Sustain, low: 48, high: 79, reverb: 105, pan: -29, level: 0.0 },
        Instrument { name: "breath", program: VOICE_OOHS, channel: CH_BREATH, role: Role::Sustain, low: 48, high: 67, reverb: 110, pan: -42, level: 0.0 },
        Instrument { name: "breath, the air", program: HALO_PAD, channel: CH_BREATH_AIR, role: Role::Sustain, low: 48, high: 67, reverb: 120, pan: 42, level: 0.0 },
    ];
    let mut score = Score::new(key, groove.meter(), tempo, instruments, ROOM_S);
    score.lead = Some(CH_LEAD);
    score.played_by(
        band,
        &[
            (CH_DRONE, Part::Drone),
            (CH_PAD, Part::Pad),
            (CH_LEAD, Part::Lead),
            (CH_PLUCK, Part::Pluck),
            (CH_SECOND, Part::Second),
            (CH_CHOIR, Part::Choir),
            (CH_HORN, Part::Horn),
            (CH_SHIMMER, Part::Shimmer),
            (CH_DRUM, Part::Drums),
            (CH_PLUCK_2, Part::Pluck2),
        ],
    );
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
    let bar = score.bar();

    // The lead plays first: five or six questions and their answers
    // about its motif. The band follows where it stands, a half-phrase a
    // part: its layers by how far it is gathered, the lead told as its
    // stance asks, and the level let to move with the band.
    let pairs = skeleton.range(5, 6) as usize;
    let improv::Play { tune, stances } = improv::improvise(design.groove, &design.feel, &design.open, &design.closed, 3, 2 * pairs, &mut rng.fork(16));
    let bars = tune.bars.len() as u32;
    let gathered = gathered(&stances);
    score.story = gathering.name;
    let mut parts = Vec::new();
    for b in 0..bars {
        let lead = telling(stances[b as usize / 2]);
        let bed = gathering.bed(gathered[b as usize]);
        if b % 2 == 0 {
            let trim = bed.trim(lead) * ladder::gain(-SWELL * (gathered[b as usize] - 0.5));
            score.sections.push(Section { name: stances[b as usize / 2].name(), start: b * bar, end: (b + 2) * bar, trim, level: None, rings: false });
        }
        parts.push(ladder::Part { rung: 0, bed, lead, a: b, b: b + 1 });
    }
    let walk = Walk { parts };
    let steps_in = ((habits.range(25, 40) as f32 / 100.0 * bars as f32) as u32 / phrase::BARS) * phrase::BARS;
    let said = stances.iter().filter(|s| s.arrives()).count();
    score.summary = format!(
        "{} on the {}: {} drone, {} pad, {} lead, {} second, {} and {} plucks, colours {}; {:?}{}, the tune said {} times in {} half-phrases, opens {:?}, ends {:?}",
        gathering.name,
        groove.name,
        name(design.drone),
        name(design.pad),
        name(design.lead),
        name(design.second),
        name(design.pluck),
        name(design.pluck_2),
        colours.join(", "),
        design.pulse,
        if design.ramp.is_some() { ", pressing on" } else { "" },
        said,
        stances.len(),
        design.intro,
        design.ending,
    )
    .to_lowercase();
    score.harmony = tune.chords.clone();
    let form = Form { bar, walk, tune, design, stances, steps_in };

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
    teller::tell(&mut score, &teller, &form.tune, &form.runs(), &mut rng.fork(8));
    frame_drum(&mut score, &form, &mut rng.fork(10));
    linger(&mut score);
    score.mark_phrases(0, form.bars());
    pace(&mut score, &form);
    ending(&mut score, &form, &mut rng.fork(12));
    intro(&mut score, &form, &mut rng.fork(13));
    score.finish();
    (score, form)
}

/// The piece's tempo: pressing on a phrase at a time from a little past
/// halfway to its last phrase, where it presses on, and leaning with the
/// lead, every player together. No one part moves it alone: a phrase
/// taken off the beat and back was heard as a band losing its way.
fn pace(score: &mut Score, form: &Form) {
    let bar = form.bar;
    let bars = form.bars();
    let base = score.eighth_bpm;
    let from = (bars * 55 / 100).next_multiple_of(phrase::BARS);
    let pressed = |b: u32| match form.design.ramp {
        Some(to) if b >= from => 1.0 + (to - 1.0) * ((b - from) / phrase::BARS + 1) as f32 / ((bars - from) / phrase::BARS).max(1) as f32,
        _ => 1.0,
    };
    let mut last = 1.0f32;
    for b in 0..bars {
        let leans = if form.gathers(b) { LEANS_IN } else if form.spaces(b) { HOLDS_BACK } else { 1.0 };
        let now = pressed(b) * leans;
        if (now - last).abs() > 1e-4 {
            score.tempo.push((b * bar, base * now));
            last = now;
        }
    }
    score.tempo.sort_by_key(|(t, _)| *t);
}

/// How the whole band's tempo leans with the lead, together: a little
/// quicker through the last bar of its run into the motif, and held back
/// through the silence after it breaks off, the motif landing in tempo
/// after either.
const LEANS_IN: f32 = 1.04;
const HOLDS_BACK: f32 = 0.94;

/// The bars of the opening call over the drone, of the prelude, of the
/// dance or the figure first, and the bars the drone rings on alone after
/// the last stroke.
const INTRO_BARS: u32 = 2;
const PRELUDE_BARS: u32 = 4;
const RING_BARS: u32 = 3;

/// The opening, as the piece's `Intro` has it, the drone under all of it
/// from its first bar. The fade: the breath a bar in, the pad's top voice
/// the bar after, two bars or four, the lead coming in with the tune. The
/// prelude: the lead alone over the drone, four bars at the dance's
/// tempo, a tone high in the mode held and turned about, then the call
/// down from the fifth to the tonic — at the tempo, since a prelude taken
/// in free time and a bed that enters on the beat sound like players
/// warming up and a band starting. The dance first, two bars or four of the plucks and
/// the drum where the pulse has one, and the figure first, two bars of
/// the plucks alone. The call: two bars of the lead down from the fifth
/// to the tonic, the descent a Balkan player's prelude ends on.
fn intro(score: &mut Score, form: &Form, rng: &mut Rng) {
    let bar = form.bar;
    let n = match form.design.intro {
        Intro::Fade | Intro::Dance => [2, 4][rng.below(2)],
        Intro::Prelude => PRELUDE_BARS,
        Intro::Figure | Intro::Call => INTRO_BARS,
    };
    score.delay(&vec![Chord::triad(0); n as usize]);
    let opening = n * bar;
    let trim = score.sections.first().map_or(1.0, |s| s.trim);
    let name = match form.design.intro {
        Intro::Fade => "fade",
        Intro::Prelude => "prelude",
        Intro::Dance => "dance",
        Intro::Figure => "figure",
        Intro::Call => "call",
    };
    score.sections.insert(0, Section { name, start: 0, end: opening, trim, level: None, rings: false });
    for note in score.notes.iter_mut().filter(|note| note.channel == CH_DRONE && note.start == opening) {
        note.start = 0;
        note.len += opening;
    }
    let key = score.key;
    let tonic = Chord::triad(0);
    match form.design.intro {
        Intro::Fade => {
            for k in 1..n {
                for p in tonic.pitches_within(&key, 50, 64).into_iter().filter(|p| matches!(key.degree_of(*p), Some(0 | 4))) {
                    let p = if [key.pitch(0, 2), key.pitch(0, 3)].iter().any(|d| clashes(p, *d)) { p + 12 } else { p };
                    score.hold(Note { start: k * bar, len: bar + E / 2, pitch: p, vel: vel(-34, rng), channel: CH_BREATH });
                    score.hold(Note { start: k * bar, len: bar + E / 2, pitch: p, vel: vel(-40, rng), channel: CH_BREATH_AIR });
                }
                if k >= 2 || n == 2 {
                    let top = *tonic.pitches_within(&key, 55, 72).iter().nth(2).unwrap();
                    score.hold(Note { start: k * bar, len: bar + E / 2, pitch: top, vel: vel(-28, rng), channel: CH_PAD_ALONE });
                }
            }
        }
        Intro::Prelude | Intro::Call => {
            let home = key.absolute_degree(tune::home_tonic(&key, TUNE.0, TUNE.1)).unwrap();
            let e = score.meter.eighths();
            let strong = score.meter.strong_eighths();
            let second = strong[1];
            let call_at = (n - INTRO_BARS) * e;
            if form.design.intro == Intro::Prelude {
                // The fifth held a bar, then turned about — the sixth and
                // the fourth off the beat, the fifth on it — a bar more.
                score.add(Note { start: 0, len: bar - E / 8, pitch: key.pitch(home + 4, 4), vel: vel(-8, rng), channel: CH_LEAD });
                for i in 0..e {
                    let d = if score.meter.strong(i) { 4 } else if i % 2 == 1 { 5 } else { 3 };
                    score.add(Note { start: bar + i * E, len: E - E / 8, pitch: key.pitch(home + d, 4), vel: vel(-12, rng), channel: CH_LEAD });
                }
            }
            let call = [(0, e - 1, 4), (e - 1, 1, 3), (e, second - 1, 2), (e + second - 1, 1, 1), (e + second, INTRO_BARS * e - e - second - 1, 0)];
            for (at, len, degree) in call {
                score.add(Note { start: (call_at + at) * E, len: len * E - E / 8, pitch: key.pitch(home + degree, 4), vel: vel(if degree == 4 { -6 } else { -12 }, rng), channel: CH_LEAD });
            }
        }
        Intro::Dance | Intro::Figure => {
            for k in 0..n {
                pluck_bar(score, form, k, tonic, Plucks::Full, false, &form.design.feel.strikes(form.design.groove), &[], rng);
                if form.design.intro == Intro::Dance && form.design.pulse.drums() {
                    drum_bar(score, form, k, Drum::Accents, false, false, &form.design.feel.strikes(form.design.groove), rng);
                }
            }
        }
    }
}

/// The ending, after the walk's last part, as the piece's `Ending` has it,
/// the room taking what rings. The decay: the bed has thinned through the
/// last phrase to the drone and the breath, and the drone rings on three
/// to six bars. The lead alone: it holds its home tonic over the drone.
/// The cadence: the pad's three voices and the choir's root and fifth hold
/// the tonic over the drone, the lead silent. The stroke: on the last
/// downbeat the frame drum's one low stroke and the plucks' tonic, the
/// lead holding its home tonic. The stop: nothing after the last bar, the
/// drone cut with it. Where it slows, the last two bars slow by a tenth
/// to a fifth.
fn ending(score: &mut Score, form: &Form, rng: &mut Rng) {
    let bar = form.bar;
    let hit = form.bars() * bar;
    score.mark_coda(form.bars());
    if form.design.slows {
        let slowest = rng.range(80, 90) as f32 / 100.0;
        score.ritardando(hit - 2 * bar, hit, slowest);
    }
    let ring = if form.design.ending == Ending::Decay { rng.range(3, 6) as u32 } else { RING_BARS };
    let trim = score.sections.last().map_or(1.0, |s| s.trim);
    score.sections.push(Section { name: "end", start: hit, end: hit + ring * bar, trim, level: None, rings: true });
    score.harmony.extend((0..ring).map(|_| Chord::triad(0)));
    if form.design.ending != Ending::Stop {
        for n in score.notes.iter_mut().filter(|n| n.channel == CH_DRONE && n.end() == hit) {
            n.len += ring * bar;
        }
    }
    let key = score.key;
    let home = tune::home_tonic(&key, TUNE.0, TUNE.1);
    match form.design.ending {
        Ending::Stroke => {
            score.add(Note { start: hit, len: 2 * E, pitch: DUM, vel: vel(0, rng), channel: CH_DRUM });
            score.add(Note { start: hit, len: 2 * E, pitch: key.pitch(0, 3), vel: vel(-6, rng), channel: CH_PLUCK });
            score.add(Note { start: hit, len: 2 * bar - E, pitch: home, vel: vel(-10, rng), channel: CH_LEAD });
        }
        Ending::Alone => {
            score.add(Note { start: hit, len: 2 * bar - E, pitch: home, vel: vel(-12, rng), channel: CH_LEAD });
        }
        Ending::Cadence => {
            let tonic = Chord::triad(0);
            for p in tonic.pitches_within(&key, 55, 72).into_iter().take(3) {
                score.add(Note { start: hit, len: ring * bar - E, pitch: p, vel: vel(-24, rng), channel: CH_PAD });
            }
            for p in tonic.pitches_within(&key, 55, 72).into_iter().filter(|p| matches!(key.degree_of(*p), Some(0 | 4))).take(2) {
                score.add(Note { start: hit, len: ring * bar - E, pitch: p, vel: vel(-30, rng), channel: CH_CHOIR });
            }
        }
        Ending::Decay | Ending::Stop => {}
    }
}

/// The tonic in two octaves under everything, one note each for the whole
/// piece: a retrigger would restart the sample. The tonic alone, not the
/// fifth: a fifth held under the minor's VI and iv rubs a semitone
/// against their flat sixth. Held to the end, where the ending rings
/// it on alone. Its level is the bed's floor, what a rest of every other voice
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
/// held clash is heard however soft it is. The walk's last chord keeps
/// its half eighth: the ending strikes its own chord there.
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
/// the eighths off the beat the lead moves on, with it, or in the bars it
/// holds, on the figure's, an octave up. Both sit under the bed's level:
/// a pluck is a strike, and a strike at the bed's level cuts in; alone
/// on the accents, with nothing to stand over, the bass is softer
/// still. The bass's variant bar passes through the mode into its last
/// beat, and its cadence takes the chord's other tone there.
fn plucks(score: &mut Score, form: &Form, rng: &mut Rng) {
    for b in 0..form.bars() {
        let mode = form.texture_at(b).plucks;
        // With no pulse, an accent every other bar.
        if mode == Plucks::Off || (form.design.pulse == Pulse::Free && b % 2 == 1) {
            continue;
        }
        let line = form.tune.bar(score, b, TUNE.0, TUNE.1, false);
        pluck_bar(score, form, b, form.chord(b), mode, form.gathers(b), &form.strikes(b), &line, rng);
    }
}

/// The plucks' bar `b` on `chord` at `mode`; where the band `gathers`
/// behind the lead's run, the bass steps up into the bar's last beat. The
/// second pluck takes the chord on `strikes`, and under a tone of the
/// lead's `line` struck with it, the chord's tone nearest a third under
/// that tone, so it moves as the lead moves.
#[allow(clippy::too_many_arguments)]
fn pluck_bar(score: &mut Score, form: &Form, b: u32, chord: Chord, mode: Plucks, gathers: bool, strikes: &[u32], line: &[tune::Placed], rng: &mut Rng) {
    let strong = score.meter.strong_eighths();
    let floor = score.key.pitch(0, 3) + 2;
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
    if (role == Bar::Variant || gathers) && last > 0 && !score.meter.strong(last - 1) {
        let passing = score.key.pitch(score.key.absolute_degree(bass).unwrap() + 1, 4);
        score.add(Note { start: b * form.bar + (last - 1) * E, len: E - 40, pitch: passing, vel: vel(-8, rng), channel: CH_PLUCK });
    }
    let soft = if mode == Plucks::Full { -6 } else { -12 };
    let others: Vec<u8> = chord.pitches_within(&score.key, 57, 69).into_iter().filter(|p| score.key.degree_of(*p) != Some(degree.rem_euclid(7) as usize)).collect();
    for (j, i) in strikes.iter().enumerate() {
        let start = b * form.bar + i * E;
        let under = |lead: u8| {
            let off = |p: &u8| (lead as i32 - 3 - *p as i32).rem_euclid(12);
            *others.iter().min_by_key(|p| off(p).min(12 - off(p))).unwrap()
        };
        let pitch = line.iter().find(|n| n.0 == start).map_or(others[(j + b as usize) % others.len()], |n| under(n.2));
        score.add(Note { start, len: E - 60, pitch, vel: vel(soft, rng), channel: CH_PLUCK_2 });
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

/// The tune answered, soft, where the bed asks: in the bar the lead rests
/// after its motif or an answer, the dulcimer plays what the lead just
/// said, detached, an octave under the tune's register — the band saying
/// it back while the lead holds, and silent while the lead speaks; at the
/// second notch the harp takes the bar's own skeleton, its strong-beat
/// tones, a foot late and a third up, bent to the chord.
fn weave(score: &mut Score, form: &Form, rng: &mut Rng) {
    let (lo, hi) = (TUNE.0 - 12, TUNE.1 - 12);
    let bars = form.bars();
    let late = form.tune.bars[0][0].len;
    let eighths = score.meter.eighths();
    for b in 0..bars {
        let mode = form.texture_at(b).weave;
        if mode == Weave::Off {
            continue;
        }
        let behind = (b + bars - 1) % bars;
        let chord = form.chord(b);
        let said = if form.echoes(b) { form.tune.bar(score, behind, lo, hi, false) } else { Vec::new() };
        for (start, len, pitch) in said {
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
/// last a soft stroke into the next. It rolls only behind the lead's run
/// into the motif, the fingers through the
/// bar's last group, growing, and comes down harder on the bar the motif
/// lands on: a roll every phrase says nothing.
fn frame_drum(score: &mut Score, form: &Form, rng: &mut Rng) {
    for b in 0..form.bars() {
        let mode = form.texture_at(b).drum;
        if mode == Drum::Off {
            continue;
        }
        drum_bar(score, form, b, mode, form.gathers(b), form.lands(b), &form.strikes(b), rng);
    }
}

/// The frame drum's bar `b` at `mode`, rolling into the next where the
/// band `gathers`, its first stroke harder where the motif `lands`, its
/// fingers on `strikes`.
#[allow(clippy::too_many_arguments)]
fn drum_bar(score: &mut Score, form: &Form, b: u32, mode: Drum, gathers: bool, lands: bool, strikes: &[u32], rng: &mut Rng) {
    let groove = form.design.groove;
    let eighths = score.meter.eighths();
    let last_group = eighths - *groove.groups.last().unwrap() as u32;
    let bare = (1..eighths).find(|e| !score.meter.strong(*e) && !groove.dum.contains(e) && !groove.tek.contains(e));
    let start = b * form.bar;
    let role = variation::role(b);
    let roll_from = gathers.then_some(last_group);
    let before_roll = |i: &u32| roll_from.is_none_or(|r| *i < r);
    for i in groove.dum.iter().filter(|i| before_roll(i)) {
        score.add(Note { start: start + i * E, len: E - 20, pitch: DUM, vel: vel(if lands && *i == 0 { 10 } else { 0 }, rng), channel: CH_DRUM });
    }
    if mode == Drum::Full {
        for i in groove.tek.iter().filter(|i| before_roll(i)) {
            score.add(Note { start: start + i * E, len: E - 20, pitch: TEK, vel: vel(-14, rng), channel: CH_DRUM });
        }
    }
    let ghost = bare.filter(|_| role == Bar::Variant);
    if let Some(e) = ghost {
        score.add(Note { start: start + e * E, len: E - 20, pitch: TEK, vel: vel(-24, rng), channel: CH_DRUM });
    }
    // The fingers where the lead moves off the beat, under the hand's
    // strokes; the phrase's variant bar leaves one of them out, a
    // different one each phrase.
    let left_out = (role == Bar::Variant && !strikes.is_empty()).then(|| strikes[(b / phrase::BARS) as usize % strikes.len()]);
    let teks = |e: &u32| mode == Drum::Full && groove.tek.contains(e);
    for e in strikes.iter().filter(|e| before_roll(e) && !groove.dum.contains(e) && !teks(e) && Some(**e) != ghost && Some(**e) != left_out) {
        score.add(Note { start: start + e * E, len: E - 20, pitch: TEK, vel: vel(-26, rng), channel: CH_DRUM });
    }
    // A phrase's last bar takes one soft stroke in its last group, on an
    // eighth nothing else of the drum's is on, or between its last two
    // where every eighth is taken: a hand that varies its bar by the
    // phrase, and too little to be heard as a signal.
    if role == Bar::Cadence && roll_from.is_none() {
        let figure = strikes;
        let taken = |e: &u32| groove.dum.contains(e) || teks(e) || figure.contains(e);
        let at = (last_group..eighths).rev().find(|e| !taken(e)).map_or((eighths - 1) * E + E / 2, |e| e * E);
        score.add(Note { start: start + at, len: E / 2 - 10, pitch: TEK, vel: vel(-22, rng), channel: CH_DRUM });
    }
    if let Some(r) = roll_from {
        let top = if mode == Drum::Full { -8 } else { -14 };
        variation::roll(score, CH_DRUM, TEK, start + r * E, (eighths - r) * 2, |x| vel(-26 + ((top + 26) as f32 * x) as i32, rng));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every layer a band gathers moves the bed, and the band follows
    /// the lead: out in the silence after it breaks off, never thinner
    /// through a run back, and at its fullest where the motif sounds.
    #[test]
    fn the_band_follows_the_lead() {
        for g in &GATHERINGS {
            for r in 0..g.ladder.len() {
                let n = g.ladder.len() as f32;
                assert_ne!(g.bed(r as f32 / n), g.bed((r + 1) as f32 / n), "{}: rung {} moves nothing", g.name, r + 1);
            }
        }
        use Stance::*;
        let stances = [States, States, Answers, Leaves, Returns, States, Breaks, States, Lifts, States];
        let e = gathered(&stances);
        assert_eq!(e.len(), 2 * stances.len());
        assert_eq!(e[13], 0.0);
        assert!(e[8] <= e[9] && e[9] <= e[10], "{e:?}");
        assert!(e[6] < e[4] && e[10] > e[2] && e[14] > e[10], "{e:?}");
        let top = e.iter().cloned().fold(0.0, f32::max);
        assert_eq!(e[16], top, "{e:?}");
    }

    /// The bars the piece opens with before its walk.
    fn intro_bars(score: &Score) -> u32 {
        score.sections.iter().filter(|s| s.start == 0 && matches!(s.name, "fade" | "prelude" | "dance" | "figure" | "call")).map(|s| s.end / score.bar()).max().unwrap_or(0)
    }

    /// The piece is whole question-and-answer pairs, its parts
    /// whole half-phrases between its opening and its ending, its harmony
    /// the schemata's, its tune in its register; nothing struck ends past
    /// the end, only what is held.
    #[test]
    fn a_piece_is_whole_phrases_on_its_schemata() {
        for seed in 0..24 {
            let (score, form) = compose(&Params::of(crate::pieces::find("overworld-ambient").unwrap(), seed));
            assert_eq!(form.bars() % (2 * phrase::BARS), 0, "seed {seed}: {} bars", form.bars());
            let intro = intro_bars(&score);
            assert!(intro > 0, "seed {seed}: no opening");
            for s in score.sections.iter().filter(|s| !s.rings && s.start >= intro * score.bar()) {
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

    /// The seeds take every pulse, opening and ending, and a free piece
    /// has no drum.
    #[test]
    fn the_pieces_are_their_own() {
        let forms: Vec<(Score, Form)> = (0..48).map(|seed| compose(&Params::of(crate::pieces::find("overworld-ambient").unwrap(), seed))).collect();
        let took = |f: &dyn Fn(&Design) -> bool| forms.iter().any(|(_, form)| f(&form.design));
        for pulse in [Pulse::Dance, Pulse::StepsIn, Pulse::Figure, Pulse::Free] {
            assert!(took(&|d| d.pulse == pulse), "{pulse:?} never taken");
        }
        for intro in [Intro::Fade, Intro::Prelude, Intro::Dance, Intro::Figure, Intro::Call] {
            assert!(took(&|d| d.intro == intro), "{intro:?} never taken");
        }
        for ending in [Ending::Decay, Ending::Alone, Ending::Cadence, Ending::Stroke] {
            assert!(took(&|d| d.ending == ending), "{ending:?} never taken");
        }
        for (score, form) in &forms {
            if !form.design.pulse.drums() {
                assert!(!score.notes.iter().any(|n| n.channel == CH_DRUM), "{}: a drum without a pulse", score.summary);
            }
        }
    }
}
