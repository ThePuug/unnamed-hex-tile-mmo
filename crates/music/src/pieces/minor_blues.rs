//! A minor blues as a small band plays it. Every choice
//! is a draw from the seed's stream, one fork per purpose, and the
//! first forks decide what kind of song this is — which story it
//! tells, in what mode and tonic, on which feel — the slow twelve-eight,
//! the rumba, the walking four or the six-eight — with which players, on
//! what theme, in which phrase form, on which rows of the twelve-bar and
//! which turn into its next chorus, how it opens, the one thing it does
//! once past its middle, how the band answers its singer and how it
//! ends, each as often as the recordings surveyed do it
//! (`proofs/research/blues-findings.md`) — so two seeds are two songs by
//! one band. The seed picks which shape at every level; it never picks
//! the next note.
//!
//! A story is a ladder of the bed's layers, walked as `ladder` says,
//! the band's loudness rising a little as it fills, declared and met by
//! the render. The bed is a band: the bass in two or walking, the
//! piano's comp and the guitar's, the kit on the brushes, the organ as
//! the wash, and the seed's colours. A blues holds its form: every
//! chorus is the song's twelve-bar, its rows the same every time round,
//! as a band's are. The head is the first chorus and the last, and every
//! chorus between is the lead's solo, a blues player's turn
//! (`solo::BLUESMAN`). One theme runs the head, and one
//! player tells it, the lead, in the blues' AAB: a row's line, the same
//! line again over the IV, and an answer; the lead calls through a
//! row's first two bars and lands, and leaves its last two to the band,
//! where the weave answers with the call — in every hole, in about
//! half, or never, holding the chord instead, as the song's band
//! answers. No player of the band plays
//! one bar over and over: the comp draws its rhythm a bar at a time,
//! never the same three bars running; the bass walks or skips; the kit
//! comps on the snare and fills where a blues fills.
//!
//! A setting leans the song's draws and bounds its walk (`lean`): a town
//! hears the songs as the recordings play them, wandering hears the band
//! late at night, and a fight hears it drive.

use crate::ladder::{self, turn, Bed, Bounds, Run, Story, Walk};
use crate::band::{Answers, Harmony, Last, Part, Turnaround};
use crate::pieces::{Params, Setting};
use crate::rng::Rng;
use crate::score::{Instrument, Note, Role, Score, Section, TICKS_PER_EIGHTH as E};
use crate::solo;
use crate::teller::{self, Hold, Teller, Telling};
use crate::voices;
use crate::theory::groove::{Groove, BLUES};
use crate::theory::melody::{Theme, Tone, BLUES_SHAPES};
use crate::theory::phrase::{Form as PhraseForm, FORMS};
use crate::theory::schema::{Schema, TWELVE_BAR};
use crate::theory::{interval_class, Chord, Key, Mode, DIATONIC, MAJOR};
use crate::theory::phrase;
use crate::tune::{self, Placed, Tune};
use crate::variation::{self, Role as Bar};

/// General MIDI programs, 0-based.
const PIANO: u8 = 0;
const E_PIANO: u8 = 4;
const VIBES: u8 = 11;
const ORGAN: u8 = 16;
const HARMONICA: u8 = 22;
const ALTO_SAX: u8 = 65;
const JAZZ_GUITAR: u8 = 26;
const UPRIGHT_BASS: u8 = 32;
const MUTED_TRUMPET: u8 = 59;
const TENOR_SAX: u8 = 66;
/// The brush kit, the General MIDI drum program a bank keeps brushes
/// at, and the keys the kit plays.
const BRUSH_KIT: u8 = 40;
const KICK: u8 = 36;
const SNARE: u8 = 38;
const HAT_PEDAL: u8 = 44;
const RIDE: u8 = 51;

const CH_BASS: u8 = 0;
const CH_PIANO: u8 = 1;
const CH_LEAD: u8 = 2;
const CH_GUITAR: u8 = 3;
const CH_SECOND: u8 = 4;
const CH_ORGAN: u8 = 5;
const CH_HORN: u8 = 6;
const CH_WEAVE: u8 = 7;
const CH_SHIMMER: u8 = 8;
const CH_KIT: u8 = 9;
const CH_WEAVE_2: u8 = 10;

/// How long the room rings: a club at night, close enough to hear the
/// brushes.
const ROOM_S: f32 = 1.6;

/// Who may lead: the band's singers, the ones that hold a line and
/// shape it. Each seed draws one.
const LEADS: [u8; 4] = [HARMONICA, MUTED_TRUMPET, TENOR_SAX, ALTO_SAX];
/// Who may hold tones under the lead's riff: a horn that is neither
/// the lead nor the colours' horn.
const SECONDS: [u8; 3] = [TENOR_SAX, ALTO_SAX, MUTED_TRUMPET];

/// The lead's level, dB, whichever it is: forward of any one player of
/// the band, a little under the band as a whole. The render evens what
/// the bank gives each player, so one level holds for every lead.
const LEAD: f32 = 3.0;

/// The velocity every voice strikes at before its own accent: one
/// dynamic for the whole band, since its story is in what plays.
const VEL: i32 = 85;

/// The kit's level, dB: its ride's, the stroke it plays most, well
/// under the band, since the brushes keep the time and never lead it.
const LEVEL_KIT: f32 = -13.0;

/// Where the band's foot sits under its top rung, LU: the band grows a
/// little as the band fills, by a level the render meets on whatever
/// bank plays it, and no more — the blues is a bed the game plays over.
const LEVEL_FOOT: f32 = -2.5;

/// The tune's register; the lead and the second take it here, the weave
/// an octave under. Home is the tonic a third under its middle, with
/// room over it for a shape's sixth, a sequence's step and the climb's
/// third.
const TUNE: (u8, u8) = (55, 81);
/// The tune a third up on the ladder's top rung: the shout chorus.
const CLIMB: i32 = 2;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Bass {
    Off,
    /// Root and fifth on one and three.
    Two,
    /// A chord tone every beat and a pickup to the next bar's root.
    Walking,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Comp {
    Off,
    /// The piano's third and seventh on the feel's strikes.
    Shells,
    /// The piano's fifth on top, and the guitar on every beat.
    Full,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kit {
    Off,
    /// The ride and the hat: time.
    Time,
    /// The kick and the brush on the snare too.
    Full,
}

/// The organ, the wash under the band.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Organ {
    Off,
    /// One tone, rocking through the chord.
    Thin,
    /// Root, fifth and seventh.
    Full,
}

/// The theme echoed under everything, soft.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Weave {
    Off,
    /// The electric piano, the tune a bar behind, an octave under.
    One,
    /// The vibes too, the skeleton a foot late, a third up.
    Two,
}

/// A layer of the band, the thing a rung of the ladder moves.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Layer {
    Bass,
    Comp,
    Kit,
    Organ,
    /// The seed's colours — a horn, a shimmer — together.
    Colours,
    Weave,
}

/// What the band is: each layer at its notch.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Texture {
    bass: Bass,
    comp: Comp,
    kit: Kit,
    organ: Organ,
    colours: bool,
    weave: Weave,
}

impl Texture {
    const BARE: Texture = Texture { bass: Bass::Off, comp: Comp::Off, kit: Kit::Off, organ: Organ::Off, colours: false, weave: Weave::Off };
}

impl Bed for Texture {
    type Layer = Layer;

    fn up(self, layer: Layer) -> Texture {
        match layer {
            Layer::Bass => Texture { bass: if self.bass == Bass::Off { Bass::Two } else { Bass::Walking }, ..self },
            Layer::Comp => Texture { comp: if self.comp == Comp::Off { Comp::Shells } else { Comp::Full }, ..self },
            Layer::Kit => Texture { kit: if self.kit == Kit::Off { Kit::Time } else { Kit::Full }, ..self },
            Layer::Organ => Texture { organ: if self.organ == Organ::Off { Organ::Thin } else { Organ::Full }, ..self },
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
        if from.bass != to.bass {
            moved(from.bass as u8, to.bass as u8, ["bass in", "bass up", "bass down", "bass out"])
        } else if from.comp != to.comp {
            moved(from.comp as u8, to.comp as u8, ["comp in", "comp up", "comp down", "comp out"])
        } else if from.kit != to.kit {
            moved(from.kit as u8, to.kit as u8, ["kit in", "kit up", "kit down", "kit out"])
        } else if from.organ != to.organ {
            moved(from.organ as u8, to.organ as u8, ["organ in", "organ up", "organ down", "organ out"])
        } else if from.colours != to.colours {
            if to.colours { "colours in" } else { "colours out" }
        } else if from.weave != to.weave {
            moved(from.weave as u8, to.weave as u8, ["weave in", "weave up", "weave down", "weave out"])
        } else {
            "held"
        }
    }
}

use Layer::{Bass as BassLayer, Colours, Comp as CompLayer, Kit as KitLayer, Organ as OrganLayer, Weave as WeaveLayer};
use Telling::{Phrases, Riff, RiffAndLong, Trading};

const STORIES: [Story<Texture, Telling>; 5] = [
    Story {
        name: "stroll",
        weight: 3.0,
        base: Texture { bass: Bass::Two, kit: Kit::Time, ..Texture::BARE },
        ladder: &[CompLayer, BassLayer, OrganLayer, KitLayer, WeaveLayer, Colours, CompLayer],
        leads: &[Phrases, Phrases, Trading, Trading, Phrases, RiffAndLong, RiffAndLong, Riff],
        turns: &[turn((7, 7), (1, 1)), turn((0, 0), (0, 0))],
        halves: (1, 1),
        pace: (0.2, 0.8),
    },
    Story {
        name: "late night",
        weight: 2.0,
        base: Texture { organ: Organ::Thin, ..Texture::BARE },
        ladder: &[BassLayer, CompLayer, WeaveLayer, KitLayer, OrganLayer, BassLayer, Colours],
        leads: &[Phrases, Phrases, Trading, Phrases, Trading, RiffAndLong, Phrases, Riff],
        turns: &[turn((4, 5), (0, 0)), turn((7, 7), (0, 1)), turn((0, 0), (0, 0))],
        halves: (1, 1),
        pace: (0.0, 0.6),
    },
    Story {
        name: "rush hour",
        weight: 2.0,
        base: Texture { bass: Bass::Walking, comp: Comp::Shells, kit: Kit::Time, ..Texture::BARE },
        ladder: &[KitLayer, OrganLayer, WeaveLayer, CompLayer, Colours, WeaveLayer],
        leads: &[Riff, Trading, Trading, RiffAndLong, RiffAndLong, Riff, Trading],
        turns: &[turn((3, 3), (0, 0)), turn((1, 1), (0, 0)), turn((6, 6), (1, 1)), turn((0, 0), (0, 0))],
        halves: (1, 1),
        pace: (0.6, 1.0),
    },
    Story {
        name: "corner",
        weight: 2.0,
        base: Texture { comp: Comp::Shells, ..Texture::BARE },
        ladder: &[BassLayer, KitLayer, WeaveLayer, OrganLayer, BassLayer, Colours],
        leads: &[Trading, Trading, Phrases, Trading, Riff, Trading, Phrases],
        turns: &[turn((2, 2), (0, 0)), turn((0, 0), (0, 0)), turn((3, 4), (0, 0)), turn((1, 1), (0, 0)), turn((6, 6), (0, 1)), turn((0, 0), (0, 0))],
        halves: (1, 1),
        pace: (0.2, 1.0),
    },
    Story {
        name: "after hours",
        weight: 1.5,
        base: Texture { organ: Organ::Thin, bass: Bass::Two, ..Texture::BARE },
        ladder: &[CompLayer, WeaveLayer, OrganLayer, KitLayer, BassLayer, Colours],
        leads: &[Phrases, Phrases, Trading, Phrases, RiffAndLong, Trading, Phrases],
        turns: &[turn((6, 6), (1, 2)), turn((0, 0), (0, 0))],
        halves: (1, 2),
        pace: (0.0, 0.3),
    },
];

/// The band's feel, as often as the recordings surveyed take each:
/// `BLUES`'s grooves, in this order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Feel {
    Slow,
    Rumba,
    Swing,
    SixEight,
}

const FEELS: [Feel; 4] = [Feel::Slow, Feel::Rumba, Feel::Swing, Feel::SixEight];
const FEEL_WEIGHTS: [f32; 4] = [35.0, 35.0, 20.0, 10.0];

/// How the song opens: a vamp on the tonic the band layers into; the
/// lead soloing a whole chorus before the head; four bars from the V; the
/// lead alone in free time over a held tonic; or the head at once.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Intro {
    Vamp,
    SoloChorus,
    Turnaround,
    Rubato,
    Straight,
}

/// The one thing the song does once, about two thirds through, or
/// nothing: a chorus in stop-time, the band striking each bar's first
/// beat for eight bars under the lead; a chorus the band drops to the
/// bass and the kit for; or a vamp on the tonic at a lifted tempo before
/// the end. The bridge and the feel switch the recordings also take are
/// unbuilt.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Event {
    None,
    StopTime,
    Drop,
    CodaVamp,
}

/// How the song ends: its last chord held and rung; a vamp on the
/// tonic falling away; the band stopping on the last chorus's eleventh
/// bar for the lead's break, then a stab; or the last two bars three
/// times.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Ending {
    Held,
    VampOut,
    Break,
    Tag,
}

/// How a setting leans the song (`proofs/research/settings-findings.md`,
/// §7): each draw's weights multiplied and the tempo bounded, and
/// nothing of the song withheld — a setting picks among whole songs the
/// band plays, which keep their form, their kit and their solos, since a
/// song with a part held back is a thin song and not a quiet one.
/// Wandering, the band plays late: the slow feels and the quiet stories,
/// the slow half of the feel's tempo, no stop-time and no lift, opening
/// on a vamp or in free time and ending held or falling away. In a fight
/// it drives: the walking four and the rumba, the stroll and the rush
/// hour, the quick third of the tempo, stop-time twice as often and no
/// dropping out, straight in or from the V, and ending on a stab or a
/// tag, never slowing into it. In a town, and as it is, the songs as the
/// recordings play them.
struct Lean {
    feels: [f32; 4],
    stories: [f32; 5],
    intros: [f32; 5],
    events: [f32; 4],
    endings: [f32; 4],
    slows: bool,
    bounds: Bounds,
}

const AS_RECORDED: Lean = Lean { feels: [1.0; 4], stories: [1.0; 5], intros: [1.0; 5], events: [1.0; 4], endings: [1.0; 4], slows: true, bounds: ladder::UNBOUNDED };

fn lean(setting: Setting) -> Lean {
    match setting {
        Setting::Ambient => Lean {
            feels: [1.5, 0.6, 0.0, 2.0],
            stories: [0.5, 2.0, 0.0, 0.5, 2.0],
            intros: [1.0, 0.0, 0.0, 1.0, 0.0],
            events: [1.0, 0.0, 1.0, 0.0],
            endings: [1.0, 1.0, 0.0, 0.0],
            slows: true,
            bounds: Bounds { tempo: (0.0, 0.5) },
        },
        Setting::Combat => Lean {
            feels: [0.3, 1.5, 2.5, 0.5],
            stories: [1.0, 0.0, 1.0, 0.0, 0.0],
            intros: [0.0, 0.0, 1.0, 0.0, 1.0],
            events: [1.0, 2.0, 0.0, 1.0],
            endings: [0.3, 0.0, 3.0, 2.0],
            slows: false,
            bounds: Bounds { tempo: (2.0 / 3.0, 1.0) },
        },
        Setting::City | Setting::None => AS_RECORDED,
    }
}

/// `weights` leaned by `by`.
fn leaned<const N: usize>(weights: [f32; N], by: [f32; N]) -> [f32; N] {
    std::array::from_fn(|i| weights[i] * by[i])
}

/// What the song draws, in the order their weights are given: the
/// setting leans each, and the band after it, by name.
const INTROS: [Intro; 5] = [Intro::Vamp, Intro::SoloChorus, Intro::Turnaround, Intro::Rubato, Intro::Straight];
const EVENTS: [Event; 4] = [Event::None, Event::StopTime, Event::Drop, Event::CodaVamp];
const ENDINGS: [Ending; 4] = [Ending::Held, Ending::VampOut, Ending::Break, Ending::Tag];

impl Intro {
    /// Its name, as a band's preferences name it.
    fn name(self) -> &'static str {
        match self {
            Intro::Vamp => "vamp",
            Intro::SoloChorus => "solo chorus",
            Intro::Turnaround => "turnaround",
            Intro::Rubato => "rubato",
            Intro::Straight => "straight",
        }
    }
}

impl Event {
    fn name(self) -> &'static str {
        match self {
            Event::None => "no event",
            Event::StopTime => "stop-time",
            Event::Drop => "drop",
            Event::CodaVamp => "coda vamp",
        }
    }
}

impl Ending {
    fn name(self) -> &'static str {
        match self {
            Ending::Held => "held",
            Ending::VampOut => "vamp out",
            Ending::Break => "break",
            Ending::Tag => "tag",
        }
    }
}

/// What a seed's piece is.
struct Design {
    piano: u8,
    /// The one player who tells the tune, the storyteller.
    lead: u8,
    /// Who holds tones under the lead's riff.
    second: u8,
    horn: u8,
    shimmer: bool,
    horns: bool,
    groove: &'static Groove,
    form: PhraseForm,
    theme: Theme,
    rows: [&'static Schema; 3],
    feel: Feel,
    /// The tune's register as the lead's instrument has it: `TUNE` fitted
    /// to where the teller sounds.
    register: (u8, u8),
    intro: Intro,
    event: Event,
    answers: Answers,
    turnaround: Turnaround,
    ending: Ending,
    last: Last,
    /// Whether the band slows into its last chord, and whether the lead
    /// runs into its last tone.
    ritard: bool,
    run_over: bool,
}

/// The bars of a chorus.
const CHORUS: u32 = TWELVE_BAR.len() as u32 * phrase::BARS;

/// The band's velocity with an accent and a little jitter so no two
/// notes strike alike.
fn vel(accent: i32, rng: &mut Rng) -> u8 {
    (VEL + accent + rng.range(-4, 4)).clamp(1, 127) as u8
}

struct Form {
    bar: u32,
    walk: Walk<Texture, Telling>,
    tune: Tune,
    design: Design,
    /// Whether each chorus is the lead's solo.
    solos: Vec<bool>,
    /// The chorus the song's event falls in, where it falls in one.
    event_at: Option<u32>,
    /// Whether the weave answers the lead with its call in each bar.
    answers: Vec<bool>,
}

impl Form {
    /// The band at `bar`: the walk's, but the bass and the kit alone
    /// through the chorus the band drops for.
    fn texture_at(&self, bar: u32) -> Texture {
        let t = self.walk.bed_at(bar);
        if self.design.event == Event::Drop && self.event_at == Some(bar / CHORUS) {
            Texture { comp: Comp::Off, organ: Organ::Off, colours: false, weave: Weave::Off, ..t }
        } else {
            t
        }
    }
    fn solo(&self, bar: u32) -> bool {
        self.solos.get((bar / CHORUS) as usize).copied().unwrap_or(false)
    }
    /// Whether the band plays stop-time at `bar`: the first eight bars of
    /// the stop-time chorus.
    fn stop(&self, bar: u32) -> bool {
        self.design.event == Event::StopTime && self.event_at == Some(bar / CHORUS) && bar % CHORUS < 2 * phrase::BARS
    }
    /// The walk's runs, the lead silent through its solos, which it plays
    /// itself.
    fn runs(&self) -> Vec<Run<Telling>> {
        let mut runs: Vec<Run<Telling>> = Vec::new();
        for b in 0..self.bars() {
            let lead = if self.solo(b) { Telling::Off } else { self.walk.at(b).lead };
            match runs.last_mut() {
                Some(r) if r.lead == lead => r.b = b + 1,
                _ => runs.push(Run { lead, a: b, b: b + 1 }),
            }
        }
        runs
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
    // The band: its members, and its habits.
    let band = params.band;
    let habits_of_band = band.blues();
    let lean = lean(params.setting);
    let stories: Vec<f32> = STORIES.iter().zip(lean.stories).map(|(s, by)| s.weight * by * band.lean(s.name)).collect();
    let story = &STORIES[skeleton.weighted(&stories)];
    // A minor blues: Dorian's major IV brightens it, Aeolian's does not.
    let mode = [Mode::Dorian, Mode::Aeolian][skeleton.weighted(&[3.0, 2.0])];
    let key = Key::new(["C", "G", "D", "F", "E"][skeleton.weighted(&[3.0, 2.0, 2.0, 2.0, 1.0])], mode);
    let feel = skeleton.weighted(&leaned(leaned(FEEL_WEIGHTS, lean.feels), std::array::from_fn(|i| band.lean(BLUES[i].name))));
    let groove = &BLUES[feel];
    let tempo = story.tempo(groove.tempo, &lean.bounds.within(band.prefs.tempo), &mut skeleton);
    // At least one colour, since a ladder may bring the colours in.
    let colours = skeleton.range(1, 3);
    // The band's first lead the blues has a voice for, its horn, and its
    // first second that is neither.
    let teller = band.program(Part::Lead, &LEADS, LEADS[0]);
    let other_horn = band.program(Part::Horn, &[], if teller == MUTED_TRUMPET { TENOR_SAX } else { MUTED_TRUMPET });
    let second = band.programs(Part::Second).into_iter().chain(SECONDS).find(|p| *p != teller && *p != other_horn && SECONDS.contains(p)).unwrap_or(SECONDS[0]);
    // The song's habits, on a stream of their own.
    let mut habits = rng.fork(14);
    let close = ENDINGS[habits.weighted(&leaned(leaned([70.0, 15.0, 10.0, 5.0], lean.endings), ENDINGS.map(|e| band.lean(e.name()))))];
    let mut design = Design {
        piano: band.program(Part::Keys, &[], PIANO),
        lead: teller,
        second,
        horn: other_horn,
        horns: colours & 1 != 0,
        shimmer: colours & 2 != 0,
        groove,
        form: FORMS[skeleton.below(FORMS.len())],
        theme: Theme::draw(groove, &BLUES_SHAPES, &mut skeleton),
        rows: [row(0, mode, &habits_of_band.harmony, &mut skeleton), row(1, mode, &habits_of_band.harmony, &mut skeleton), row(2, mode, &habits_of_band.harmony, &mut skeleton)],
        feel: FEELS[feel],
        register: voices::fit(TUNE, teller),
        intro: INTROS[habits.weighted(&leaned(leaned([35.0, 25.0, 15.0, 15.0, 10.0], lean.intros), INTROS.map(|i| band.lean(i.name()))))],
        event: EVENTS[habits.weighted(&leaned(leaned([40.0, 15.0, 10.0, 10.0], lean.events), EVENTS.map(|e| band.lean(e.name()))))],
        answers: habits_of_band.answers,
        turnaround: drawn(habits_of_band.harmony.turnarounds, &mut habits),
        ending: close,
        last: drawn(habits_of_band.harmony.lasts, &mut habits),
        // A held ending slows into its chord about half the time, the
        // others less; the lead runs into its last tone two times in five.
        ritard: habits.chance(if close == Ending::Held { 0.45 } else { 0.3 }) && lean.slows,
        run_over: habits_of_band.runs_over,
    };
    // Every part written where its player's instrument has notes.
    let bass_program = band.program(Part::Bass, &[], UPRIGHT_BASS);
    let bass_bounds = voices::within(bass_program, (28, 60));
    let lead_bounds = voices::within(design.lead, (design.register.0.saturating_sub(2), design.register.1 + 3));
    let second_bounds = voices::within(design.second, (50, 84));
    let horn_bounds = voices::within(design.horn, (50, 72));
    let instruments = vec![
        Instrument { name: "bass", program: bass_program, channel: CH_BASS, role: Role::Pluck, low: bass_bounds.0, high: bass_bounds.1, reverb: 25, pan: 0, level: 0.0 },
        Instrument { name: "piano", program: design.piano, channel: CH_PIANO, role: Role::Pluck, low: 48, high: 79, reverb: 45, pan: -21, level: 0.0 },
        Instrument { name: "lead", program: design.lead, channel: CH_LEAD, role: Role::Melody, low: lead_bounds.0, high: lead_bounds.1, reverb: 45, pan: 0, level: LEAD },
        Instrument { name: "guitar", program: band.program(Part::Comp, &[], JAZZ_GUITAR), channel: CH_GUITAR, role: Role::Pluck, low: 40, high: 72, reverb: 35, pan: 36, level: 0.0 },
        Instrument { name: "second", program: design.second, channel: CH_SECOND, role: Role::Melody, low: second_bounds.0, high: second_bounds.1, reverb: 55, pan: 18, level: 0.0 },
        Instrument { name: "organ", program: band.program(Part::Organ, &[], ORGAN), channel: CH_ORGAN, role: Role::Sustain, low: 52, high: 69, reverb: 80, pan: -34, level: 0.0 },
        Instrument { name: "horn", program: design.horn, channel: CH_HORN, role: Role::Sustain, low: horn_bounds.0, high: horn_bounds.1, reverb: 75, pan: 29, level: 0.0 },
        Instrument { name: "weave", program: band.program(Part::Echo, &[], E_PIANO), channel: CH_WEAVE, role: Role::Pluck, low: 40, high: 84, reverb: 70, pan: -47, level: 0.0 },
        Instrument { name: "shimmer", program: band.program(Part::Shimmer, &[], VIBES), channel: CH_SHIMMER, role: Role::Pluck, low: 72, high: 91, reverb: 75, pan: 47, level: 0.0 },
        Instrument { name: "kit", program: band.program(Part::Drums, &[], BRUSH_KIT), channel: CH_KIT, role: Role::Percussion, low: KICK, high: RIDE, reverb: 35, pan: 0, level: LEVEL_KIT },
        Instrument { name: "weave, the vibes", program: band.program(Part::Shimmer, &[], VIBES), channel: CH_WEAVE_2, role: Role::Pluck, low: 53, high: 89, reverb: 70, pan: 44, level: 0.0 },
    ];
    let mut score = Score::new(key, groove.meter(), tempo, instruments, ROOM_S);
    score.lead = Some(CH_LEAD);
    score.played_by(
        band,
        &[
            (CH_BASS, Part::Bass),
            (CH_PIANO, Part::Keys),
            (CH_LEAD, Part::Lead),
            (CH_GUITAR, Part::Comp),
            (CH_SECOND, Part::Second),
            (CH_ORGAN, Part::Organ),
            (CH_HORN, Part::Horn),
            (CH_WEAVE, Part::Echo),
            (CH_SHIMMER, Part::Shimmer),
            (CH_KIT, Part::Drums),
            (CH_WEAVE_2, Part::Shimmer),
        ],
    );
    let bar = score.bar();

    // The walk, in half-phrases, and whole choruses of them, so it
    // closes on the turn home.
    let walk = story.place(&mut skeleton, &mut score, 2 * TWELVE_BAR.len() as u32, &[], |_, _| 1.0);
    let upper = story.ladder.len().max(1);
    for (section, part) in score.sections.iter_mut().zip(&walk.parts) {
        section.level = Some(LEVEL_FOOT * (1.0 - part.rung as f32 / upper as f32));
    }

    // The head first and last, the lead's solos between, and before the
    // head where the song opens on one.
    let choruses = walk.bars() / CHORUS;
    if design.intro == Intro::SoloChorus && choruses < 3 {
        design.intro = Intro::Vamp;
    }
    let head = if design.intro == Intro::SoloChorus { 1 } else { 0 };
    let soloed: Vec<bool> = (0..choruses).map(|c| (c == 0 && head == 1) || (c > head && c + 1 < choruses)).collect();
    // The event in a chorus between the heads two thirds or so through:
    // one starting from half to four fifths of the way, else the nearest.
    let middle: Vec<u32> = (head + 1..choruses.saturating_sub(1)).collect();
    let within: Vec<u32> = middle.iter().copied().filter(|c| (0.5..=0.8).contains(&((c * CHORUS) as f32 / walk.bars() as f32))).collect();
    let event_at = match design.event {
        Event::StopTime | Event::Drop if !within.is_empty() => Some(within[habits.below(within.len())]),
        Event::StopTime | Event::Drop => middle.iter().copied().min_by_key(|c| (((c * CHORUS) as f32 / walk.bars() as f32 - 0.65).abs() * 1000.0) as i32),
        _ => None,
    };
    if design.event != Event::CodaVamp && event_at.is_none() {
        design.event = Event::None;
    }
    let colours: Vec<&str> = [(design.horns, "horn"), (design.shimmer, "shimmer")].into_iter().filter(|(on, _)| *on).map(|(_, n)| n).collect();
    let name = |program: u8| match program {
        PIANO => "piano",
        E_PIANO => "electric piano",
        VIBES => "vibes",
        HARMONICA => "harmonica",
        ALTO_SAX => "alto sax",
        JAZZ_GUITAR => "jazz guitar",
        MUTED_TRUMPET => "muted trumpet",
        TENOR_SAX => "tenor sax",
        56 => "trumpet",
        _ => "?",
    };
    score.summary = format!(
        "{} on the {}: a {:?} in a {:?}, {} / {} / {}, turning on the {:?}; {} comp, {} lead, {} second, {} horn, colours {}; opens {:?}, {:?}, answers {:?}, ends {:?} on the {:?}",
        story.name,
        groove.name,
        design.theme.shape,
        design.form,
        design.rows[0].name,
        design.rows[1].name,
        design.rows[2].name,
        design.turnaround,
        name(design.piano),
        name(design.lead),
        name(design.second),
        if design.horns { name(design.horn) } else { "no" },
        colours.join(", "),
        design.intro,
        design.event,
        design.answers,
        design.ending,
        design.last,
    )
    .to_lowercase();

    // The rows of every chorus the song's own, the same each time round;
    // the last bar turning into the next chorus as the song turns, but the
    // last chorus's, which the ending follows.
    let rows: Vec<&Schema> = (0..choruses).flat_map(|_| design.rows).collect();
    // The tune a third up on the ladder's top, else as written: a blues
    // keeps its tune and moves the changes under it.
    let shifts: Vec<i32> = (0..walk.bars()).map(|b| if walk.at(b).rung >= upper { CLIMB } else { 0 }).collect();
    let mut tune = Tune::compose(&[&design.theme], &score.meter, design.form, &rows, 4, shifts);
    tune.scale = Some(tune::MINOR_PENTATONIC);
    if let Some(turn) = turnaround(mode, design.turnaround) {
        for c in 0..choruses.saturating_sub(1) {
            tune.chords[((c + 1) * CHORUS - 1) as usize] = turn;
        }
    }
    // Where the weave answers the lead's call: every hole it plays in, or
    // half of them, or none, where it holds the chord instead.
    let dropped = |b: u32| design.event == Event::Drop && event_at == Some(b / CHORUS);
    let weaves = |b: u32| hole(b) && walk.bed_at(b).weave != Weave::Off && !dropped(b);
    let mut answering = rng.fork(16);
    let answers: Vec<bool> = (0..walk.bars())
        .map(|b| {
            weaves(b)
                && match design.answers {
                    Answers::Obbligato => true,
                    Answers::Sparse => answering.chance(0.5),
                    Answers::Pads => false,
                }
        })
        .collect();
    let pads = design.answers == Answers::Pads;
    aab(&mut tune, score.meter.eighths(), |b| answers[b as usize] || (pads && weaves(b)));
    score.harmony = tune.chords.clone();
    // The stop-time bars and the dropped chorus are sections of their own,
    // declared under the part they fall in: a level is met over bars that
    // play alike, and a few bars of hits inside a full part are not.
    if let Some(c) = event_at {
        let span = match design.event {
            Event::StopTime => Some((c * CHORUS, c * CHORUS + 2 * phrase::BARS, "stop-time")),
            Event::Drop => Some((c * CHORUS, (c + 1) * CHORUS, "bass and kit")),
            _ => None,
        };
        if let Some((a, z, name)) = span {
            set_apart(&mut score, a * bar, z * bar, name, EVENT_UNDER);
        }
    }
    let form = Form { bar, walk, tune, design, solos: soloed, event_at, answers };

    bass(&mut score, &form, &mut rng.fork(1));
    piano(&mut score, &form, &mut rng.fork(2));
    guitar(&mut score, &form, &mut rng.fork(3));
    organ(&mut score, &form, &mut rng.fork(4));
    if form.design.horns {
        horn(&mut score, &form, &mut rng.fork(5));
    }
    if form.design.shimmer {
        shimmer(&mut score, &form, &mut rng.fork(6));
    }
    weave(&mut score, &form, &mut rng.fork(9));
    stops(&mut score, &form, &mut rng.fork(17));
    let teller = Teller {
        lead: CH_LEAD,
        second: CH_SECOND,
        double: None,
        echo: CH_WEAVE,
        register: form.design.register,
        sung: -12,
        riff: (-8, -14),
        // The lead holds at its tune's strength: a bank's horns fall
        // away under a soft stroke by more than its harmonica does, and
        // a lead's level holds only for what it plays at one strength.
        long: (60, 72, -12),
        under: (55, 67, -18),
        hold: Hold::Ringing,
        breathes: false,
        vel,
        fills: 0.0,
        soars: 0.0,
        // A blues phrase starts a beat ahead in one of five.
        pushes: 0.2,
    };
    teller::tell(&mut score, &teller, &form.tune, &form.runs(), &mut rng.fork(8));
    solos(&mut score, &form, &mut rng.fork(15));
    kit(&mut score, &form, &mut rng.fork(10));
    score.mark_phrases(0, form.bars());
    ending(&mut score, &form, &mut rng.fork(12));
    match form.design.intro {
        Intro::Turnaround => intro(&mut score, &mut rng.fork(13)),
        Intro::Vamp => {
            // Two bars or four, so the vamp is whole half-phrases.
            let bars = [2, 4][rng.fork(18).below(2)];
            score.delay(&vec![tonic(); bars as usize]);
            score.sections.insert(0, Section { name: "vamp", start: 0, end: bars * bar, trim: 1.0, level: Some(LEVEL_FOOT), rings: false });
            // The bass first, the kit a bar in, the comp from the vamp's
            // last two bars.
            vamp(&mut score, &form, 0, bars, |k| (k >= 1, k + 2 >= bars, false), &mut rng.fork(13));
        }
        Intro::Rubato => rubato(&mut score, &form, &mut rng.fork(13)),
        Intro::SoloChorus | Intro::Straight => {}
    }
    score.finish();
    (score, form)
}

/// How far under the part it falls in the stop-time's bars and the
/// dropped chorus are declared, LU.
const EVENT_UNDER: f32 = 2.0;

/// Ticks `a..z` made a section of their own named `name`, declared
/// `under` the level of the section it begins in; the sections it cuts
/// keep their own on either side.
fn set_apart(score: &mut Score, a: u32, z: u32, name: &'static str, under: f32) {
    let Some(level) = score.sections.iter().find(|s| s.start <= a && a < s.end).and_then(|s| s.level) else { return };
    let mut out: Vec<Section> = Vec::new();
    for s in &score.sections {
        if s.end <= a || s.start >= z {
            out.push(s.clone());
            continue;
        }
        if s.start < a {
            out.push(Section { end: a, ..s.clone() });
        }
        if s.end > z {
            out.push(Section { start: z, ..s.clone() });
        }
    }
    out.push(Section { name, start: a, end: z, trim: 1.0, level: Some(level - under), rings: false });
    out.sort_by_key(|s| s.start);
    score.sections = out;
}

/// The tonic's seventh, the chord a vamp stands on.
fn tonic() -> Chord {
    Chord { root: 0, size: 4, alter: DIATONIC }
}

/// The chord a chorus's last bar turns on in `mode`; none where it holds
/// the tonic. The borrowed ones lower what the mode has: Aeolian's VI
/// and ii stand where the ♭VI and iiø do, Dorian's a semitone over.
fn turnaround(mode: Mode, turn: Turnaround) -> Option<Chord> {
    let dorian = mode == Mode::Dorian;
    let seventh = |root: i32, alter: [i8; 4]| Some(Chord { root, size: 4, alter });
    match turn {
        Turnaround::Tonic => None,
        Turnaround::Dominant => seventh(4, MAJOR),
        Turnaround::FlatSix => seventh(5, if dorian { [-1, 0, 0, -1] } else { [0, 0, 0, -1] }),
        Turnaround::HalfDim => seventh(1, if dorian { [0, 0, -1, 0] } else { DIATONIC }),
        Turnaround::FlatTwo => seventh(1, if dorian { [-1, 0, -1, -1] } else { [-1, 0, 0, -1] }),
    }
}

/// The song's last chord in `mode`: the tonic's seventh or its ninth, the
/// major seventh of ♭VI or ♭II, or the major tonic. The lead's home tonic
/// is a tone of every one of them.
fn last_chord(mode: Mode, last: Last) -> Chord {
    let dorian = mode == Mode::Dorian;
    match last {
        Last::Seventh => tonic(),
        Last::Ninth => Chord { root: 0, size: 5, alter: DIATONIC },
        Last::FlatSix => Chord { root: 5, size: 4, alter: if dorian { [-1, 0, 0, 0] } else { DIATONIC } },
        Last::Picardy => Chord { root: 0, size: 3, alter: MAJOR },
        Last::FlatTwo => Chord { root: 1, size: 4, alter: if dorian { [-1, 0, -1, 0] } else { [-1, 0, 0, 0] } },
    }
}

/// The lead's solos: a blues player's turn over every run of solo
/// choruses, the second answering the first.
fn solos(score: &mut Score, form: &Form, rng: &mut Rng) {
    let player = &solo::BLUESMAN;
    let mut answered: Option<solo::Shape> = None;
    let n = form.solos.len() as u32;
    let mut c = 0;
    while c < n {
        if !form.solos[c as usize] {
            c += 1;
            continue;
        }
        let from = c;
        while c < n && form.solos[c as usize] {
            c += 1;
        }
        let shape = match answered {
            Some(to) => solo::Shape::answer(&to, player, rng),
            None => solo::Shape::draw(player, rng),
        };
        let line = solo::turn(score, from * CHORUS, c * CHORUS, form.design.register, player, &shape, rng);
        play(score, form, line, from * CHORUS, c * CHORUS, rng);
        answered = Some(shape);
    }
}

/// A blues player's line on bars `a..z`, played by the lead: every tone
/// in its bar's key and every strong beat bent onto the chord, the line
/// repaired as the tune is — and the line a listener follows through a
/// run, its tones longer than a sixteenth, too — growing as it goes.
fn play(score: &mut Score, form: &Form, mut line: Vec<Placed>, a: u32, z: u32, rng: &mut Rng) {
    let (lo, hi) = form.design.register;
    let bar = form.bar;
    line.sort_by_key(|n| n.0);
    for i in 0..line.len() {
        let key = score.key_at(line[i].0);
        if !key.contains(line[i].2) {
            line[i].2 = key.snap(line[i].2).clamp(lo, hi);
        }
        let chord = score.chord_at(line[i].0);
        if score.strong(line[i].0) && !chord.holds(&key, line[i].2) {
            let prev = i.checked_sub(1).map(|j| line[j].2);
            let next = line.get(i + 1).map(|m| m.2);
            line[i].2 = tune::bent_apart(&key, chord, line[i].2, prev, next, lo, hi);
        }
    }
    // The turn's quick tones repaired as a run of its own; then the line a
    // listener follows through it — its tones longer than a sixteenth —
    // joined to the lead's heard tones either side of it within a bar, the
    // last two before, which stand: the leap into the join is read with
    // the turn's own first, and the turn bends to them. A written tone is
    // heard from a half-eighth; the turn's own sixteenths are written a
    // hair under one, graces.
    form.tune.repair(score, &mut line, lo, hi);
    let heard_lead = |n: &&Note| n.channel == CH_LEAD && n.len >= E / 2;
    let mut before: Vec<Placed> = score.notes.iter().filter(heard_lead).filter(|n| n.start < a * bar && n.end() + bar > a * bar).map(|n| (n.start, n.len, n.pitch)).collect();
    before.sort_by_key(|n| n.0);
    let before: Vec<Placed> = before.iter().rev().take(2).rev().copied().collect();
    let after = score.notes.iter().filter(heard_lead).filter(|n| n.start >= z * bar && n.start < (z + 1) * bar).min_by_key(|n| n.start).map(|n| (n.start, n.len, n.pitch));
    let heard: Vec<usize> = (0..line.len()).filter(|i| line[*i].1 > E / 2).collect();
    let mut skeleton: Vec<Placed> = before.iter().copied().chain(heard.iter().map(|i| line[*i])).chain(after).collect();
    form.tune.repair_from(score, &mut skeleton, lo, hi, before.len());
    for (k, i) in heard.iter().enumerate() {
        line[*i].2 = skeleton[before.len() + k].2;
    }
    // Where the repair moved the tone after, it stands, and the turn's last
    // heard tone steps to it instead.
    if let Some(next) = after.filter(|n| skeleton.last() != Some(n)) {
        if let Some(last) = line.iter_mut().rev().find(|n| n.1 > E / 2) {
            let key = score.key_at(last.0);
            let chord = score.chord_at(last.0);
            let to = key.standing_degree(next.2);
            let fits = |p: u8| key.contains(p) && (!score.strong(last.0) || chord.holds(&key, p)) && (lo..=hi).contains(&p);
            if let Some(p) = [to - 1, to + 1, to - 2, to + 2, to].iter().map(|d| key.pitch(*d, 4)).find(|p| fits(*p)) {
                last.2 = p;
            }
        }
    }
    let span = ((z - a) * form.bar).max(1);
    for (start, len, pitch) in line {
        let grow = (8 * start.saturating_sub(a * form.bar) / span) as i32;
        let accent = -14 + grow + if len >= 2 * E { 4 } else if score.strong(start) { 0 } else { -6 };
        let len = if len <= E / 2 { len.saturating_sub(10).max(E / 8) } else { len - E / 8 };
        score.add(Note { start, len, pitch, vel: vel(accent, rng), channel: CH_LEAD });
    }
}

/// The band's hits through the stop-time chorus's first eight bars: the
/// bass's root, the piano's shell and the guitar's chord on the bar's
/// first beat with the kick; the variant strikes again at the bar's
/// middle, the cadence pushes the bar's last eighth; the snare picks the
/// band back up into the ninth bar.
fn stops(score: &mut Score, form: &Form, rng: &mut Rng) {
    let key = score.key;
    let strong = score.meter.strong_eighths();
    let eighths = score.meter.eighths();
    for b in (0..form.bars()).filter(|b| form.stop(*b)) {
        let chord = form.chord(b);
        let start = b * form.bar;
        let strokes: Vec<u32> = match variation::role(b) {
            Bar::Variant => vec![0, strong[strong.len() / 2]],
            Bar::Cadence => vec![0, eighths - 1],
            _ => vec![0],
        };
        let root = nearest(&at_degree(&key, chord, chord.root, 33, 57), 43);
        let shell: Vec<u8> = [chord.root + 2, chord.root + 6].iter().filter_map(|d| at_degree(&key, chord, *d, 52, 67).first().copied()).collect();
        for (k, at) in strokes.iter().enumerate() {
            let accent = if k == 0 { -8 } else { -14 };
            let t = start + at * E;
            score.add(Note { start: t, len: E, pitch: root, vel: vel(accent - 6, rng), channel: CH_BASS });
            for p in &shell {
                score.add(Note { start: t, len: E - 40, pitch: *p, vel: vel(accent - 6, rng), channel: CH_PIANO });
            }
            score.add(Note { start: t, len: E - 20, pitch: KICK, vel: vel(accent - 10, rng), channel: CH_KIT });
            score.add(Note { start: t, len: E - 20, pitch: SNARE, vel: vel(accent - 4, rng), channel: CH_KIT });
        }
        if b % CHORUS == 2 * phrase::BARS - 1 {
            for k in 0..3 {
                score.add(Note { start: start + (eighths - 3 + k) * E, len: E / 2, pitch: SNARE, vel: vel(-20 + 6 * k as i32, rng), channel: CH_KIT });
            }
        }
    }
}

/// Bars `from..from + n` vamped on the tonic: the bass in two and walking
/// by turns, a pickup into the next bar; and as `joins` says of the
/// vamp's `k`th bar, the brushes' time, the piano's comp on its drawn
/// rhythm, the organ's root and fifth.
fn vamp(score: &mut Score, form: &Form, from: u32, n: u32, joins: impl Fn(u32) -> (bool, bool, bool), rng: &mut Rng) {
    let key = score.key;
    let chord = tonic();
    let bar = form.bar;
    let strong = score.meter.strong_eighths();
    let eighths = score.meter.eighths();
    let groups = score.meter.groups.clone();
    let groove = form.design.groove;
    let tones = chord.pitches_within(&key, 33, 57);
    let root = nearest(&at_degree(&key, chord, 0, 33, 57), 43);
    let rhythm = cells(&score.meter, n, rng);
    for k in 0..n {
        let start = (from + k) * bar;
        let (kit, comp, organ) = joins(k);
        let mut p = root;
        if k % 2 == 0 {
            let mid = strong.len() / 2;
            let fifth = if root >= 33 + 5 { root - 5 } else { root + 7 };
            let half: u32 = groups[..mid].iter().map(|g| *g as u32).sum();
            score.add(Note { start, len: half * E - 40, pitch: root, vel: vel(-18, rng), channel: CH_BASS });
            score.add(Note { start: start + strong[mid] * E, len: (eighths - half - 1) * E - 40, pitch: fifth, vel: vel(-24, rng), channel: CH_BASS });
            p = fifth;
        } else {
            let at = tones.iter().position(|t| *t == root).unwrap_or(0);
            for (g, s) in strong.iter().enumerate() {
                p = tones[(at + g).min(tones.len() - 1)];
                let len = groups[g] as u32 * E - if g + 1 == strong.len() { E } else { 0 } - 40;
                score.add(Note { start: start + s * E, len, pitch: p, vel: vel(if g == 0 { -16 } else { -24 }, rng), channel: CH_BASS });
            }
        }
        let degree = key.absolute_degree(root).unwrap();
        let pickup = key.pitch(degree + if p > root { 1 } else { -1 }, 4);
        score.add(Note { start: start + (eighths - 1) * E, len: E - 40, pitch: pickup, vel: vel(-30, rng), channel: CH_BASS });
        if kit {
            // The variant bar leaves its last ride stroke for the hat, as the
            // band's time does.
            let ride: Vec<u32> = strong.iter().chain(groove.chord).copied().collect();
            let dropped = (variation::role(k) == Bar::Variant).then(|| ride.iter().copied().max()).flatten();
            for e in &ride {
                let pitch = if Some(*e) == dropped { HAT_PEDAL } else { RIDE };
                score.add(Note { start: start + e * E, len: E - 20, pitch, vel: vel(if strong.contains(e) { 6 } else { -8 }, rng), channel: CH_KIT });
            }
            for e in groove.tek {
                score.add(Note { start: start + e * E, len: E - 20, pitch: HAT_PEDAL, vel: vel(0, rng), channel: CH_KIT });
            }
            if k + 1 == n {
                for j in 0..3 {
                    score.add(Note { start: start + (eighths - 3 + j) * E, len: E / 2, pitch: SNARE, vel: vel(-20 + 6 * j as i32, rng), channel: CH_KIT });
                }
            }
        }
        if comp {
            let shell: Vec<u8> = [2, 6].iter().filter_map(|d| at_degree(&key, chord, *d, 52, 67).first().copied()).collect();
            let cell = rhythm[k as usize];
            for i in cell.strikes {
                let len = if cell.held { bar - E / 4 } else { E - 60 };
                for p in &shell {
                    score.add(Note { start: start + i * E, len, pitch: *p, vel: vel(-20, rng), channel: CH_PIANO });
                }
            }
        }
        if organ {
            for d in [0, 4] {
                if let Some(p) = at_degree(&key, chord, d, 52, 69).first() {
                    score.hold(Note { start, len: bar + E / 2, pitch: *p, vel: vel(-26, rng), channel: CH_ORGAN });
                }
            }
        }
    }
}

/// The bars of the rubato opening.
const RUBATO_BARS: u32 = 2;

/// The rubato opening: the lead alone in free time over the organ's held
/// tonic, a blues player's two bars taken at about half the band's pace,
/// wavering beat to beat, before the band comes in at its own.
fn rubato(score: &mut Score, form: &Form, rng: &mut Rng) {
    score.delay(&[tonic(); RUBATO_BARS as usize]);
    let bar = form.bar;
    let key = score.key;
    score.sections.insert(0, Section { name: "rubato", start: 0, end: RUBATO_BARS * bar, trim: 1.0, level: Some(LEVEL_FOOT), rings: false });
    for d in [0, 4] {
        if let Some(p) = at_degree(&key, tonic(), d, 52, 69).first() {
            score.add(Note { start: 0, len: RUBATO_BARS * bar + E / 8, pitch: *p, vel: vel(-26, rng), channel: CH_ORGAN });
        }
    }
    let player = &solo::BLUESMAN;
    let shape = solo::Shape::draw(player, rng);
    let line = solo::turn(score, 0, RUBATO_BARS, form.design.register, player, &shape, rng);
    play(score, form, line, 0, RUBATO_BARS, rng);
    let base = score.eighth_bpm;
    let beats: Vec<u32> = (0..RUBATO_BARS * bar).filter(|t| score.strong(*t)).collect();
    for t in beats {
        score.tempo.push((t, base * rng.range(45, 60) as f32 / 100.0));
    }
    if !score.tempo.iter().any(|(t, _)| *t == RUBATO_BARS * bar) {
        score.tempo.push((RUBATO_BARS * bar, base));
    }
    score.tempo.sort_by_key(|(t, _)| *t);
}

/// The intro's chords, four bars from the V: the twelve-bar's last row,
/// the V a dominant seventh — its third raised, the leading tone the head
/// is pulled home by — then the iv, the i, and the V again into the head.
fn intro_chords() -> [Chord; 4] {
    let seventh = |root: i32, alter: [i8; 4]| Chord { root, size: 4, alter };
    [seventh(4, MAJOR), seventh(3, DIATONIC), seventh(0, DIATONIC), seventh(4, MAJOR)]
}

/// Four bars from the V before the head, as a blues band opens: the bass
/// walking up through each chord a beat a tone, the piano's shell on the
/// beat and held to the push, the organ holding the chord under them, the
/// brushes' ride on the beats and the hat's pedal on the backbeat, a
/// snare pickup into the head; the lead waits for its head.
fn intro(score: &mut Score, rng: &mut Rng) {
    let chords = intro_chords();
    score.delay(&chords);
    let bar = score.bar();
    let strong = score.meter.strong_eighths();
    let eighths = score.meter.eighths();
    let groups = score.meter.groups.clone();
    score.sections.insert(0, Section { name: "intro", start: 0, end: chords.len() as u32 * bar, trim: 1.0, level: Some(LEVEL_FOOT), rings: false });
    for (b, chord) in chords.iter().enumerate() {
        let b = b as u32;
        let key = score.key_at(b * bar);
        let start = b * bar;
        let tones = chord.pitches_within(&key, 33, 57);
        let root = nearest(&at_degree(&key, *chord, chord.root, 33, 57), 43);
        let from = tones.iter().position(|t| *t == root).unwrap_or(0);
        for (g, at) in strong.iter().enumerate() {
            let pitch = tones[(from + g).min(tones.len() - 1)];
            score.add(Note { start: start + at * E, len: groups[g] as u32 * E - 40, pitch, vel: vel(if g == 0 { -16 } else { -24 }, rng), channel: CH_BASS });
        }
        let shell: Vec<u8> = [chord.root + 2, chord.root + 6].iter().flat_map(|d| at_degree(&key, *chord, *d, 52, 67)).take(2).collect();
        let push = strong[1] + 1;
        for (at, to) in [(0, push), (push, eighths)] {
            for p in &shell {
                score.add(Note { start: start + at * E, len: (to - at) * E - 40, pitch: *p, vel: vel(-20, rng), channel: CH_PIANO });
            }
        }
        for d in [chord.root, chord.root + 4] {
            if let Some(p) = at_degree(&key, *chord, d, 52, 69).first() {
                score.add(Note { start, len: bar - E / 4, pitch: *p, vel: vel(-26, rng), channel: CH_ORGAN });
            }
        }
        for (g, at) in strong.iter().enumerate() {
            score.add(Note { start: start + at * E, len: E, pitch: RIDE, vel: vel(-14, rng), channel: CH_KIT });
            if g % 2 == 1 {
                score.add(Note { start: start + at * E, len: E, pitch: HAT_PEDAL, vel: vel(-20, rng), channel: CH_KIT });
            }
        }
        if b + 1 == chords.len() as u32 {
            for k in 0..3 {
                score.add(Note { start: start + (eighths - 3 + k) * E, len: E / 2, pitch: SNARE, vel: vel(-20 + 6 * k as i32, rng), channel: CH_KIT });
            }
        }
    }
}

/// How far the band slows into its last chord where it slows, the bars
/// it slows over, the bars the chord rings, and the ending's level: under
/// the foot by what a ring falling away puts the hit over its mean.
const SLOWEST: f32 = 0.85;
const SLOWING_BARS: u32 = 2;
const RING_BARS: u32 = 3;
const LEVEL_END: f32 = LEVEL_FOOT - 4.0;

/// The bars of a vamp out, or of the coda's vamp, and how much faster
/// the coda's vamp is taken.
const VAMP_BARS: u32 = 4;
const CODA_LIFT: f32 = 0.06;

/// The ending, after the last chorus's turn home, as the song's `Ending`
/// has it — out of the coda's vamp, a tempo lifted, where the song's
/// event is one. Held: the band, slowing into it where the song slows,
/// lands together on its last chord — the bass's low root, the piano's
/// chord spread up from its third, the organ's root and fifth, the lead
/// on its home tonic, run into from a step or two above where it runs,
/// the brushes rolling on the ride, or on the snare where they keep
/// time there, and swelling — and the room rings. The
/// vamp out falls away bar by bar into that chord, soft. The break stops
/// the band on the last chorus's eleventh bar for two bars of the lead
/// alone, and the band stabs the chord. The tag plays the last two bars
/// three times before the chord.
fn ending(score: &mut Score, form: &Form, rng: &mut Rng) {
    let bar = form.bar;
    let design = &form.design;
    let mut at = form.bars();
    score.mark_coda(at);
    let vamps = (design.event == Event::CodaVamp) as u32 + (design.ending == Ending::VampOut) as u32;
    if vamps > 0 {
        // The coda's vamp and the vamp out are one vamp where the song has
        // both: lifted and falling away.
        score.harmony.extend((0..VAMP_BARS).map(|_| tonic()));
        if design.event == Event::CodaVamp {
            let base = score.bpm_at(at * bar);
            score.tempo.push((at * bar, base * (1.0 + CODA_LIFT)));
        }
        // Falling away a half-phrase at a time where it vamps out.
        let half = phrase::BARS / 2;
        for k in (0..VAMP_BARS).step_by(half as usize) {
            let fall = if design.ending == Ending::VampOut { 1.5 * (k / half + 1) as f32 } else { 0.0 };
            score.sections.push(Section { name: "vamp", start: (at + k) * bar, end: (at + k + half) * bar, trim: 1.0, level: Some(LEVEL_FOOT - fall), rings: false });
        }
        vamp(score, form, at, VAMP_BARS, |_| (true, true, true), rng);
        let player = &solo::BLUESMAN;
        let shape = solo::Shape::draw(player, rng);
        let line = solo::turn(score, at, at + VAMP_BARS, form.design.register, player, &shape, rng);
        play(score, form, line, at, at + VAMP_BARS, rng);
        at += VAMP_BARS;
    }
    if design.ending == Ending::Tag {
        // The brushes pick up into every repeat of the line, as a drummer
        // leads a tag round: the last two bars played three times as they
        // were is a loop, and where the two are alike the kit plays one bar
        // six times running.
        let eighths = score.meter.eighths();
        for k in 0..3 {
            score.add(Note { start: (at - 1) * bar + (eighths - 3 + k) * E, len: E / 2, pitch: SNARE, vel: vel(-20 + 6 * k as i32, rng), channel: CH_KIT });
        }
        for _ in 0..2 {
            let start = score.again(at - 2, at);
            score.sections.push(Section { name: "tag", start: start * bar, end: (start + 2) * bar, trim: 1.0, level: Some(LEVEL_FOOT), rings: false });
        }
        at = score.harmony.len() as u32;
    }
    let stab = design.ending == Ending::Break && vamps == 0;
    if stab {
        // The band stops on the eleventh bar's first beat, and the lead
        // breaks alone.
        let (from, to) = ((at - 2) * bar, at * bar);
        let band = |ch: u8| ch != CH_LEAD && ch != CH_SECOND;
        score.notes.retain(|n| !(band(n.channel) && n.start > from && n.start < to) && !(n.channel == CH_LEAD && n.start >= from && n.start < to));
        for n in score.notes.iter_mut().filter(|n| band(n.channel) && n.start == from) {
            n.len = n.len.min(2 * E);
        }
        let player = &solo::BLUESMAN;
        let shape = solo::Shape::draw(player, rng);
        let line = solo::turn(score, at - 2, at, form.design.register, player, &shape, rng);
        play(score, form, line, at - 2, at, rng);
    }
    let hit = at * bar;
    if design.ritard && !stab && design.ending != Ending::VampOut {
        score.ritardando((at - SLOWING_BARS) * bar, hit, SLOWEST);
    }
    let chord = last_chord(score.key.mode, design.last);
    let level = if design.ending == Ending::VampOut { LEVEL_END - 2.0 } else { LEVEL_END };
    score.sections.push(Section { name: "end", start: hit, end: hit + RING_BARS * bar, trim: 1.0, level: Some(level), rings: true });
    score.harmony.extend((0..RING_BARS).map(|_| chord));
    let key = score.key;
    let len = if stab { 2 * E } else { RING_BARS * bar - E };
    let low = nearest(&at_degree(&key, chord, chord.root, 28, 40), 33);
    score.add(Note { start: hit, len, pitch: low, vel: vel(-10, rng), channel: CH_BASS });
    for p in chord.pitches_within(&key, 55, 74).into_iter().skip(1).take(4) {
        score.add(Note { start: hit, len, pitch: p, vel: vel(-16, rng), channel: CH_PIANO });
    }
    if !stab {
        for d in [chord.root, chord.root + 4] {
            if let Some(p) = at_degree(&key, chord, d, 52, 69).first() {
                score.add(Note { start: hit, len, pitch: *p, vel: vel(-24, rng), channel: CH_ORGAN });
            }
        }
    }
    // The lead's home tonic, a tone of every last chord; run into, where it
    // runs, from the chord's tone over it down the pentatonic in the
    // chord's first beat.
    let home = tune::home_tonic(&key, form.design.register.0, form.design.register.1);
    let under = key.under(chord);
    let first = *score.meter.strong_eighths().get(1).unwrap_or(&score.meter.eighths());
    let mut from = hit;
    if design.run_over {
        let above = chord.pitches_within(&key, home + 3, home + 9).into_iter().next();
        if let Some(top) = above {
            let steps: Vec<u8> = (home + 1..=top).rev().filter(|p| under.contains(*p) && tune::MINOR_PENTATONIC.contains(&under.standing_degree(*p).rem_euclid(7))).collect();
            let unit = first * E / (steps.len() as u32).max(1);
            for (k, p) in steps.iter().enumerate() {
                score.add(Note { start: hit + k as u32 * unit, len: unit - 10, pitch: *p, vel: vel(-14, rng), channel: CH_LEAD });
            }
            from = hit + first * E;
        }
    }
    score.add(Note { start: from, len: 2 * bar - (from - hit), pitch: home, vel: vel(-10, rng), channel: CH_LEAD });
    score.add(Note { start: hit, len: E, pitch: KICK, vel: vel(-16, rng), channel: CH_KIT });
    if !stab {
        let eighths = score.meter.eighths();
        for k in 0..eighths * 2 {
            let x = k as f32 / (eighths * 2) as f32;
            score.add(Note { start: hit + k * E / 2, len: E / 2, pitch: RIDE, vel: vel(-34 + (24.0 * x) as i32, rng), channel: CH_KIT });
        }
    }
}

/// One of row `r`'s schemata the mode can take, as often as the band's
/// `harmony` takes each; a row the band names no weight for it never
/// plays.
fn row(r: usize, mode: Mode, harmony: &Harmony, rng: &mut Rng) -> &'static Schema {
    let fits: Vec<&'static Schema> = TWELVE_BAR[r].iter().filter(|s| s.modes.contains(&mode)).collect();
    let weights: Vec<f32> = fits.iter().map(|s| harmony.rows.iter().find(|(n, _)| *n == s.name).map_or(0.0, |(_, w)| *w)).collect();
    assert!(weights.iter().any(|w| *w > 0.0), "no row of {r} in {mode:?} for the band's harmony");
    fits[rng.weighted(&weights)]
}

/// One of the band's choices, as often as it takes each.
fn drawn<T: Copy>(choices: &[(T, f32)], rng: &mut Rng) -> T {
    let weights: Vec<f32> = choices.iter().map(|(_, w)| *w).collect();
    choices[rng.weighted(&weights)].0
}

/// The tune as a blues sings it, every chorus AAB: the second row's line
/// is the first's again — bent to the IV where its strong beats meet it
/// — and the third answers; in every row the lead calls through the
/// first two bars, holds the row's last tone through the third, and
/// leaves the fourth to the band, the hole the weave answers in. Where
/// `answered` says no weave plays there, the lead answers itself in the
/// fourth with the row's first bar, as a player with no band to answer
/// does, and the hole is never the bass and the organ alone.
fn aab(tune: &mut Tune, eighths: u32, answered: impl Fn(u32) -> bool) {
    let rows = TWELVE_BAR.len();
    let bars = phrase::BARS as usize;
    for chorus in 0..tune.bars.len() / (rows * bars) {
        let head = chorus * rows * bars;
        for k in 0..bars {
            tune.bars[head + bars + k] = tune.bars[head + k].clone();
        }
        for r in 0..rows {
            let row = head + r * bars;
            if let Some(end) = tune.bars[row + bars - 1].last().copied() {
                tune.bars[row + bars - 2] = vec![Tone { onset: 0, len: eighths, degree: end.degree }];
            }
            let last = row + bars - 1;
            tune.bars[last] = if answered(last as u32) { Vec::new() } else { tune.bars[row].clone() };
        }
    }
}

/// Whether bar `b` is a row's response hole, the two bars the lead
/// leaves to the band.
fn hole(b: u32) -> bool {
    b % phrase::BARS >= phrase::BARS - 2
}

/// A comp's rhythm for a bar: the eighths it strikes on, and whether it
/// holds its chord to the bar's end. A comping player draws one a bar —
/// the Charleston, its reverse, the backbeats, the shuffle's strikes, a
/// push on the bar's last eighth, a chord held, a bar laid out — and
/// never the same three bars running, since one rhythm over and over is
/// the riff a blues varies and not one it plays.
struct Cell {
    strikes: &'static [u32],
    held: bool,
}

/// The cells of the four, of the twelve-eight and of the six-eight, with
/// how often each is drawn.
const CELLS_FOUR: [(Cell, f32); 6] = [
    (Cell { strikes: &[0, 3], held: false }, 3.0),
    (Cell { strikes: &[1, 4], held: false }, 2.0),
    (Cell { strikes: &[2, 6], held: false }, 2.0),
    (Cell { strikes: &[3, 7], held: false }, 1.5),
    (Cell { strikes: &[0], held: true }, 1.0),
    (Cell { strikes: &[], held: false }, 0.5),
];
const CELLS_SHUFFLE: [(Cell, f32); 6] = [
    (Cell { strikes: &[0, 5], held: false }, 3.0),
    (Cell { strikes: &[2, 6], held: false }, 2.0),
    (Cell { strikes: &[3, 9], held: false }, 2.0),
    (Cell { strikes: &[2, 5, 8, 11], held: false }, 2.0),
    (Cell { strikes: &[0], held: true }, 1.0),
    (Cell { strikes: &[], held: false }, 0.5),
];

const CELLS_SIX: [(Cell, f32); 6] = [
    (Cell { strikes: &[0, 3], held: false }, 3.0),
    (Cell { strikes: &[2, 5], held: false }, 2.0),
    (Cell { strikes: &[1, 4], held: false }, 2.0),
    (Cell { strikes: &[0, 5], held: false }, 1.5),
    (Cell { strikes: &[0], held: true }, 1.0),
    (Cell { strikes: &[], held: false }, 0.5),
];

/// A cell for each bar of the piece, drawn, never the same three bars
/// running.
fn cells(meter: &crate::theory::Meter, bars: u32, rng: &mut Rng) -> Vec<&'static Cell> {
    let pool: &'static [(Cell, f32)] = match meter.eighths() {
        12 => &CELLS_SHUFFLE,
        6 => &CELLS_SIX,
        _ => &CELLS_FOUR,
    };
    let weights: Vec<f32> = pool.iter().map(|(_, w)| *w).collect();
    let mut out: Vec<usize> = Vec::new();
    for _ in 0..bars {
        let mut k = rng.weighted(&weights);
        while out.len() >= 2 && out[out.len() - 1] == k && out[out.len() - 2] == k {
            k = rng.weighted(&weights);
        }
        out.push(k);
    }
    out.into_iter().map(|k| &pool[k].0).collect()
}

/// The chord's pitches in `lo..=hi` at `degree` of the mode, as the chord
/// has it: a borrowed chord's tone stands for the degree it moves.
fn at_degree(key: &Key, chord: Chord, degree: i32, lo: u8, hi: u8) -> Vec<u8> {
    let under = key.under(chord);
    chord.pitches_within(key, lo, hi).into_iter().filter(|p| under.degree_of(*p) == Some(degree.rem_euclid(7) as usize)).collect()
}

/// The chord's fifth nearest a fourth under `root` in `lo..=hi`, as the
/// chord has it: a half-diminished chord's is flat.
fn fifth_of(key: &Key, chord: Chord, root: u8, lo: u8, hi: u8) -> u8 {
    let fifths = at_degree(key, chord, chord.root + 4, lo, hi);
    if fifths.is_empty() {
        root
    } else {
        nearest(&fifths, if root >= lo + 5 { root - 5 } else { root + 7 })
    }
}

fn nearest(candidates: &[u8], to: u8) -> u8 {
    *candidates.iter().min_by_key(|c| ((**c as i32 - to as i32).abs(), **c)).unwrap()
}

/// Each of `voices` led to the nearest of `candidates` not yet taken,
/// so a chord moves by the least motion and no two voices meet.
fn led(candidates: &[u8], voices: &[u8]) -> Vec<u8> {
    let mut pool = candidates.to_vec();
    voices
        .iter()
        .map(|v| {
            let p = nearest(&pool, *v);
            pool.retain(|c| *c != p);
            p
        })
        .collect()
}

/// The bass: in two, the root on one and the fifth on three, held to
/// the next; walking, a chord tone on every beat — the root first,
/// then up or down through the chord, the way turning bar by bar, and
/// again on a phrase's cadence; its variant bar skips into its third
/// beat from a step of the mode off it.
/// Either way, on the last eighth a pickup one step of the mode toward
/// the next bar's root, from the side the line comes from, so a change
/// is walked into and not jumped at, and the ending's tonic has a
/// pickup into it like every other bar. On the rumba the walking notch is
/// the rumba's own figure instead: the root, the fifth on the and of two
/// and the octave on three, three, three and two. It rests through the
/// stop-time bars, which strike their own. Well under the band's level: the
/// upright's low end weighs on the loudness far past what the ear
/// gives it, and the pedal would cut the band around it.
fn bass(score: &mut Score, form: &Form, rng: &mut Rng) {
    // From A1 to A3: where an upright walks, its root in two on C3 and
    // its fifth under it, clear of the boom below.
    let (lo, hi) = voices::within(score.instrument(CH_BASS).program, (33, 57));
    let strong = score.meter.strong_eighths();
    let eighths = score.meter.eighths();
    let groups = score.meter.groups.clone();
    let mut last: Option<u8> = None;
    let mut rising = true;
    for b in 0..form.bars() {
        let mode = form.texture_at(b).bass;
        if mode == Bass::Off || form.stop(b) {
            last = None;
            continue;
        }
        // The bar's key, a borrowed chord's tones its own.
        let key = score.key_at(b * form.bar);
        let chord = form.chord(b);
        let roots = at_degree(&key, chord, chord.root, lo, hi);
        let root = nearest(&roots, last.unwrap_or(46));
        let beat = |g: usize| b * form.bar + strong[g] * E;
        let mut p = root;
        match mode {
            Bass::Walking if form.design.feel == Feel::Rumba => {
                let fifth = fifth_of(&key, chord, root, lo, hi);
                let octave = if root + 12 <= hi { root + 12 } else { root };
                // The variant takes the third where the fifth stood, the
                // cadence falls to the fifth where the octave stood.
                let third = at_degree(&key, chord, chord.root + 2, lo, hi);
                let second = if variation::role(b) == Bar::Variant && !third.is_empty() { nearest(&third, fifth) } else { fifth };
                let last_stroke = if variation::role(b) == Bar::Cadence { fifth } else { octave };
                for (at, len, pitch) in [(0, 3, root), (3, 3, second), (6, 1, last_stroke)] {
                    score.add(Note { start: b * form.bar + at * E, len: len * E - 40, pitch, vel: vel(if at == 0 { -16 } else { -24 }, rng), channel: CH_BASS });
                }
                p = last_stroke;
            }
            Bass::Two => {
                let fifth = fifth_of(&key, chord, root, lo, hi);
                let mid = strong.len() / 2;
                let half = groups[..mid].iter().map(|g| *g as u32).sum::<u32>() * E;
                // Where the chord stands a second bar, the fifth takes the
                // one and the root the three; on the variant bar a skip
                // note leads into the three.
                let held = b > 0 && form.chord(b - 1) == chord && form.texture_at(b - 1).bass != Bass::Off;
                let (one, three) = if held { (fifth, root) } else { (root, fifth) };
                let third = at_degree(&key, chord, chord.root + 2, lo, hi);
                let three = if variation::role(b) == Bar::Cadence && !third.is_empty() { nearest(&third, three) } else { three };
                let skip = variation::role(b) == Bar::Variant;
                let first = if skip { half - E - 40 } else { half - 40 };
                score.add(Note { start: beat(0), len: first, pitch: one, vel: vel(-18, rng), channel: CH_BASS });
                if skip {
                    let passing = key.pitch(key.standing_degree(three) + if three > one { -1 } else { 1 }, 4);
                    score.add(Note { start: beat(mid) - E, len: E - 40, pitch: passing.clamp(lo, hi), vel: vel(-28, rng), channel: CH_BASS });
                }
                let rest = form.bar - half;
                score.add(Note { start: beat(mid), len: rest - E - 40, pitch: three, vel: vel(-24, rng), channel: CH_BASS });
                p = three;
            }
            Bass::Walking => {
                let tones = chord.pitches_within(&key, lo, hi);
                // The cadence turns the line the other way; the variant
                // skips into its third beat a step of the mode off it.
                let role = variation::role(b);
                if role == Bar::Cadence {
                    rising = !rising;
                }
                let mut line = Vec::with_capacity(strong.len());
                for g in 0..strong.len() {
                    if g > 0 {
                        let up = tones.iter().copied().find(|t| *t > p);
                        let down = tones.iter().copied().rev().find(|t| *t < p);
                        p = match if rising { up } else { down } {
                            Some(next) => next,
                            None => {
                                rising = !rising;
                                (if rising { up } else { down }).unwrap_or(p)
                            }
                        };
                    }
                    line.push(p);
                }
                // Into the bar's middle beat, the second of a bar of two.
                let into = if line.len() > 2 { 2 } else { 1 };
                let skip = (role == Bar::Variant && line.len() > 1).then(|| {
                    let to = line[into];
                    key.pitch(key.standing_degree(to) + if line[into - 1] > to { 1 } else { -1 }, 4).clamp(lo, hi)
                });
                for (g, pitch) in line.iter().enumerate() {
                    let last_beat = g + 1 == strong.len();
                    let short = last_beat || (g + 1 == into && skip.is_some());
                    let len = groups[g] as u32 * E - if short { E } else { 0 } - 40;
                    score.add(Note { start: beat(g), len, pitch: *pitch, vel: vel(if g == 0 { -16 } else { -24 }, rng), channel: CH_BASS });
                }
                if let Some(pitch) = skip {
                    score.add(Note { start: beat(into) - E, len: E - 40, pitch, vel: vel(-30, rng), channel: CH_BASS });
                }
                rising = !rising;
            }
            Bass::Off => unreachable!(),
        }
        // The last bar walks into the ending's chord, the tonic.
        let next = if b + 1 < form.bars() { form.chord(b + 1) } else { Chord { root: 0, size: 4, alter: DIATONIC } };
        let next_root = nearest(&at_degree(&key, next, next.root, lo, hi), if mode == Bass::Two { root } else { p });
        let degree = key.standing_degree(next_root);
        let pickup = key.pitch(degree + if p > next_root { 1 } else { -1 }, 4);
        score.add(Note { start: b * form.bar + (eighths - 1) * E, len: E - 40, pitch: pickup, vel: vel(-30, rng), channel: CH_BASS });
        last = Some(if mode == Bass::Two { root } else { p });
    }
}

/// The piano's comp: the chord's third and seventh, each voice to the
/// nearest tone of the next chord so a change moves by step, at the full
/// notch the fifth on top, on the bar's drawn rhythm (`cells`); into a
/// row's first bar, now and then, the next chord pushed on the last
/// eighth. Short, and under the band's level: a strike at the band's
/// level cuts in.
fn piano(score: &mut Score, form: &Form, rng: &mut Rng) {
    let key = score.key;
    // Under the tune's register, so the comp is a floor and not a rival.
    let (lo, hi) = (50, 66);
    let rhythm = cells(&score.meter, form.bars(), rng);
    let eighths = score.meter.eighths();
    let mut prev: Option<Vec<u8>> = None;
    let voicing_of = |chord: Chord, prev: &Option<Vec<u8>>, full: bool| -> Vec<u8> {
        let voice = |degree: i32, near: u8| nearest(&at_degree(&key, chord, degree, lo, hi), near);
        let (third, seventh) = match prev {
            Some(v) => (voice(chord.root + 2, v[0]), voice(chord.root + 6, v[1])),
            None => {
                let third = voice(chord.root + 2, 56);
                (third, voice(chord.root + 6, third + 4))
            }
        };
        let mut voicing = vec![third, seventh];
        if full {
            voicing.push(voice(chord.root + 4, third.max(seventh) + 4));
        }
        voicing
    };
    for b in 0..form.bars() {
        let mode = form.texture_at(b).comp;
        if mode == Comp::Off || form.stop(b) {
            prev = None;
            continue;
        }
        let voicing = voicing_of(form.chord(b), &prev, mode == Comp::Full);
        let cell = rhythm[b as usize];
        for i in cell.strikes {
            let len = if cell.held { form.bar - E / 4 } else { E - 60 };
            for p in &voicing {
                score.add(Note { start: b * form.bar + i * E, len, pitch: *p, vel: vel(-20, rng), channel: CH_PIANO });
            }
        }
        let into_row = b + 1 < form.bars() && (b + 1) % phrase::BARS == 0;
        if into_row && !cell.strikes.contains(&(eighths - 1)) && !cell.held && rng.chance(0.5) {
            let next = voicing_of(form.chord(b + 1), &Some(voicing.clone()), mode == Comp::Full);
            // A borrowed chord's tones pushed into the bar before are off
            // that bar's key.
            let here = score.key_at(b * form.bar);
            if !next.iter().all(|p| here.contains(*p)) {
                prev = Some(voicing);
                continue;
            }
            for p in &next {
                score.add(Note { start: b * form.bar + (eighths - 1) * E, len: E - 60, pitch: *p, vel: vel(-18, rng), channel: CH_PIANO });
            }
        }
        prev = Some(voicing);
    }
}

/// The guitar where the comp is full: the chord in a close voicing under
/// the piano's, short, four to the bar — but on the phrase's variant bar
/// it leaves the third beat and pushes the bar's last eighth, and on its
/// cadence it strikes only the backbeats, so the pulse under the comp
/// breathes with the phrase.
fn guitar(score: &mut Score, form: &Form, rng: &mut Rng) {
    let key = score.key;
    let (lo, hi) = (48, 64);
    let strong = score.meter.strong_eighths();
    let eighths = score.meter.eighths();
    let mut prev: Option<Vec<u8>> = None;
    for b in 0..form.bars() {
        if form.texture_at(b).comp != Comp::Full || form.stop(b) {
            prev = None;
            continue;
        }
        let chord = form.chord(b);
        let voicing: Vec<u8> = match &prev {
            Some(v) => led(&chord.pitches_within(&key, lo, hi), v),
            None => [chord.root, chord.root + 4, chord.root + 6].iter().zip([52, 59, 62]).map(|(d, near)| nearest(&at_degree(&key, chord, *d, lo, hi), near)).collect(),
        };
        let strikes: Vec<u32> = match variation::role(b) {
            Bar::Variant => strong.iter().copied().enumerate().filter(|(g, _)| *g != 2).map(|(_, e)| e).chain(std::iter::once(eighths - 1)).collect(),
            Bar::Cadence => strong.iter().copied().enumerate().filter(|(g, _)| g % 2 == 1).map(|(_, e)| e).collect(),
            _ => strong.clone(),
        };
        for e in strikes {
            for p in &voicing {
                score.add(Note { start: b * form.bar + e * E, len: E / 2, pitch: *p, vel: vel(-24, rng), channel: CH_GUITAR });
            }
        }
        prev = Some(voicing);
    }
}

/// The organ, the wash: root, fifth and seventh of the bar's chord
/// held a bar at a time and carried across the bar where the chord
/// keeps them, each voice to the nearest tone of the next chord; thin,
/// one tone alone, rocking to the next of the three each bar the chord
/// stands. Never the third: the third against the seventh of the
/// dominant is the tritone, which the piano strikes and no one holds —
/// but under a major seventh, root, third and fifth, since that seventh
/// is a semitone from the root and no one holds that either.
/// Every note lingers half an eighth into the next chord, so a change
/// is a crossfade and not a cut.
fn organ(score: &mut Score, form: &Form, rng: &mut Rng) {
    let key = score.key;
    // Under the tune's peaks; from G#3, so its root and seventh, a
    // whole tone apart, are never both under G3, where that is mud.
    let (lo, hi) = voices::within(score.instrument(CH_ORGAN).program, (52, 64));
    let mut voices: Option<Vec<u8>> = None;
    let mut alone: Option<u8> = None;
    let mut rising = true;
    for b in 0..form.bars() {
        let mode = form.texture_at(b).organ;
        if mode == Organ::Off || form.stop(b) {
            voices = None;
            alone = None;
            rising = true;
            continue;
        }
        let chord = form.chord(b);
        let under = key.under(chord);
        let major_seventh = (under.pitch(chord.root + 6, 4) as i32 - under.pitch(chord.root, 4) as i32).rem_euclid(12) == 11;
        // A half-diminished chord's root and fifth are a tritone, which no
        // one holds either: its root, third and seventh instead.
        let diminished = interval_class(under.pitch(chord.root, 4), under.pitch(chord.root + 4, 4)) == 6;
        let held: [i32; 3] = if major_seventh { [chord.root, chord.root + 2, chord.root + 4] } else if diminished { [chord.root, chord.root + 2, chord.root + 6] } else { [chord.root, chord.root + 4, chord.root + 6] };
        let candidates: Vec<u8> = held.iter().flat_map(|d| at_degree(&key, chord, *d, lo, hi)).collect::<std::collections::BTreeSet<u8>>().into_iter().collect();
        if mode == Organ::Thin {
            let stands = b > 0 && form.chord(b - 1) == chord;
            let v = match alone {
                None => voices.as_ref().map_or_else(|| nearest(&at_degree(&key, chord, chord.root + 4, lo, hi), 60), |v| nearest(&candidates, v[1])),
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
            score.hold(Note { start: b * form.bar, len: form.bar + E / 2, pitch: v, vel: vel(-26, rng), channel: CH_ORGAN });
            alone = Some(v);
            voices = None;
            continue;
        }
        let next: Vec<u8> = match (&voices, alone) {
            (Some(prev), _) => led(&candidates, prev),
            _ => {
                let root = nearest(&at_degree(&key, chord, chord.root, lo, hi), alone.unwrap_or(57).saturating_sub(5));
                held[1..].iter().zip([root + 7, root + 10]).fold(vec![root], |mut v, (d, near)| {
                    v.push(nearest(&at_degree(&key, chord, *d, lo, hi), near));
                    v
                })
            }
        };
        for p in &next {
            score.hold(Note { start: b * form.bar, len: form.bar + E / 2, pitch: *p, vel: vel(-22, rng), channel: CH_ORGAN });
        }
        voices = Some(next);
        alone = None;
    }
}

/// The horn where the colours play: the chord's third held a bar at a
/// time, the one tone the organ leaves — but the fifth where the third
/// would hold a tritone against the chord's seventh, since a held
/// tritone reads as a wrong note and the dominant's is the piano's to
/// strike; carried across the bar where the next chord keeps it.
fn horn(score: &mut Score, form: &Form, rng: &mut Rng) {
    let key = score.key;
    let (lo, hi) = voices::within(score.instrument(CH_HORN).program, (52, 67));
    let mut last: Option<u8> = None;
    for b in 0..form.bars() {
        if !form.texture_at(b).colours || form.stop(b) {
            last = None;
            continue;
        }
        let chord = form.chord(b);
        let third = nearest(&at_degree(&key, chord, chord.root + 2, lo, hi), last.unwrap_or(60));
        let seventh = key.under(chord).pitch(chord.root + 6, 4);
        let pitch = if interval_class(third, seventh) == 6 { nearest(&at_degree(&key, chord, chord.root + 4, lo, hi), last.unwrap_or(60)) } else { third };
        score.hold(Note { start: b * form.bar, len: form.bar + E / 8, pitch, vel: vel(-16, rng), channel: CH_HORN });
        last = Some(pitch);
    }
}

/// A shimmer above the band where the colours play: two tones of the
/// chord on the vibes, struck by halves for a bar, then a bar's rest,
/// the tones drawn afresh each time.
fn shimmer(score: &mut Score, form: &Form, rng: &mut Rng) {
    let mut b = 0;
    while b < form.bars() {
        if !form.texture_at(b).colours || form.stop(b) {
            b += 1;
            continue;
        }
        let chord = form.chord(b);
        let tones = chord.pitches_within(&score.key, 74, 89);
        let a = tones[rng.below(tones.len())];
        let c = tones[rng.below(tones.len())];
        for h in 0..score.meter.eighths() * 2 {
            let start = b * form.bar + h * E / 2;
            score.add(Note { start, len: E / 2, pitch: if h % 2 == 0 { a } else { c }, vel: vel(-26, rng), channel: CH_SHIMMER });
        }
        b += 2;
    }
}

/// The weave answers the lead in a row's hole, as strong as the lead's
/// riff, where the band asks for it and the song's band answers, and
/// never while the lead calls: a soft answer is a hole in the loudness.
/// The electric piano plays the call — the row's first two bars — again
/// in the hole, an octave under the tune's register, its strong beats
/// bent to the hole's chords; at the second notch the vibes take the
/// call's skeleton, its strong-beat tones, a foot late and a third up,
/// bent to the chord. A band that never answers holds the hole's chord
/// instead, the electric piano's third and fifth and the vibes' root over
/// them. It leaves the lead's solos alone.
fn weave(score: &mut Score, form: &Form, rng: &mut Rng) {
    let (lo, hi) = (form.design.register.0 - 12, form.design.register.1 - 12);
    let late = form.design.theme.feet[0][0].1;
    let eighths = score.meter.eighths();
    for b in (0..form.bars()).filter(|b| hole(*b)) {
        let mode = form.texture_at(b).weave;
        if mode == Weave::Off || form.solo(b) || form.stop(b) {
            continue;
        }
        if form.design.answers == Answers::Pads {
            let chord = form.chord(b);
            let key = score.key;
            for d in [chord.root + 2, chord.root + 4] {
                if let Some(p) = at_degree(&key, chord, d, 52, 67).first() {
                    score.add(Note { start: b * form.bar, len: form.bar - E / 4, pitch: *p, vel: vel(-22, rng), channel: CH_WEAVE });
                }
            }
            if mode == Weave::Two {
                if let Some(p) = at_degree(&key, chord, chord.root, 67, 79).first() {
                    score.add(Note { start: b * form.bar, len: form.bar - E / 4, pitch: *p, vel: vel(-26, rng), channel: CH_WEAVE_2 });
                }
            }
            continue;
        }
        if !form.answers[b as usize] {
            continue;
        }
        let call = b - 2;
        let chord = form.chord(b);
        for (start, len, pitch) in form.tune.bar(score, call, lo, hi, false) {
            let onset = start - call * form.bar;
            let held = if len >= 3 * E { 2 * E } else { len - E / 2 };
            let key = score.key_at(b * form.bar);
            let pitch = if !key.contains(pitch) || (score.meter.strong(onset / E) && !chord.holds(&key, pitch)) { tune::nearest_chord_tone(&key, chord, pitch, lo, hi) } else { pitch };
            score.add(Note { start: b * form.bar + onset, len: held, pitch, vel: vel(-8, rng), channel: CH_WEAVE });
        }
        if mode != Weave::Two {
            continue;
        }
        for (start, len, pitch) in form.tune.bar(score, call, form.design.register.0, form.design.register.1, false) {
            let onset = (start - call * form.bar) / E;
            if !score.meter.strong(onset) || onset + late >= eighths {
                continue;
            }
            let mut third = score.key.pitch(score.key.standing_degree(pitch) + 2, 4).min(89);
            if !chord.holds(&score.key, third) {
                third = tune::nearest_chord_tone(&score.key, chord, third, form.design.register.0, 89);
            }
            let held = (len - E / 2).min((eighths - onset - late) * E - E / 4);
            score.add(Note { start: b * form.bar + (onset + late) * E, len: held, pitch: third, vel: vel(-18, rng), channel: CH_WEAVE_2 });
        }
    }
}

/// The toms a brush fill falls down through, high to low.
const TOMS: [u8; 4] = [50, 47, 45, 41];
const CRASH: u8 = 49;

/// The kit on the brushes where the band asks for it: time is the ride on
/// every beat and on the feel's strikes — the swung third of the beat,
/// or the push — and the hat's pedal on the backbeat; the whole kit adds
/// the kick on the feel's low strokes and the brush on the snare on its
/// high, and comps on the snare, soft, an eighth or two drawn afresh
/// every bar. A blues fills where it turns: through the last beat of a
/// chorus's last bar most often, into its ninth bar less, into its fifth
/// least, and at time a phrase's variant bar leaves the ride's last
/// strike for the hat; every other chorus the whole kit opens on the
/// crash. Every drum strikes about the band's velocity, the kit's
/// level set by its ride, the stroke it plays most, and the crash
/// marks a chorus a little over it. A stroke's length changes nothing.
fn kit(score: &mut Score, form: &Form, rng: &mut Rng) {
    let strong = score.meter.strong_eighths();
    let eighths = score.meter.eighths();
    let groove = form.design.groove;
    let chorus = TWELVE_BAR.len() as u32 * phrase::BARS;
    let last_beat = *strong.last().unwrap();
    for b in 0..form.bars() {
        let mode = form.texture_at(b).kit;
        if mode == Kit::Off || form.stop(b) {
            continue;
        }
        let in_chorus = b % chorus;
        let fills = b + 1 < form.bars()
            && rng.chance(match in_chorus {
                11 => 0.7,
                7 => 0.3,
                3 => 0.2,
                _ => 0.0,
            });
        let until = if fills { last_beat } else { eighths };
        let role = variation::role(b);
        let mut stroke = |e: u32, pitch: u8, accent: i32, rng: &mut Rng| {
            score.add(Note { start: b * form.bar + e * E, len: E - 20, pitch, vel: vel(accent, rng), channel: CH_KIT });
        };
        let ride: Vec<u32> = strong.iter().chain(groove.chord).copied().filter(|e| *e < until).collect();
        let dropped = (role == Bar::Variant).then(|| ride.iter().copied().max()).flatten();
        for e in &ride {
            if Some(*e) == dropped {
                stroke(*e, HAT_PEDAL, 0, rng);
            } else {
                stroke(*e, RIDE, if strong.contains(e) { 6 } else { -8 }, rng);
            }
        }
        for e in groove.tek.iter().filter(|e| **e < until) {
            stroke(*e, HAT_PEDAL, 0, rng);
        }
        if mode == Kit::Full {
            for e in groove.dum.iter().filter(|e| **e < until) {
                stroke(*e, KICK, -30, rng);
            }
            for e in groove.tek.iter().filter(|e| **e < until) {
                stroke(*e, SNARE, -8, rng);
            }
            let open: Vec<u32> = (0..until).filter(|e| !strong.contains(e) && !groove.tek.contains(e)).collect();
            for _ in 0..rng.range(1, 2) {
                if !open.is_empty() {
                    let e = open[rng.below(open.len())];
                    stroke(e, SNARE, -20, rng);
                }
            }
            if in_chorus == 0 && (b / chorus) % 2 == 1 {
                stroke(0, CRASH, 10, rng);
            }
        }
        if fills {
            let n = (eighths - last_beat) * 2;
            for k in 0..n {
                let pitch = if mode == Kit::Full { TOMS[(k as usize * TOMS.len() / n as usize).min(TOMS.len() - 1)] } else { SNARE };
                score.add(Note { start: b * form.bar + last_beat * E + k * E / 2, len: E / 2 - 10, pitch, vel: vel(-18 + (16 * k / n) as i32, rng), channel: CH_KIT });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theory::phrase;

    /// Every story is a ladder.
    #[test]
    fn every_story_is_a_ladder() {
        for story in &STORIES {
            assert_eq!(story.fault(), None);
        }
    }

    /// The bars the song opens with before its first chorus.
    fn intro_bars(score: &Score) -> u32 {
        score.sections.iter().filter(|s| matches!(s.name, "intro" | "vamp" | "rubato") && s.start == 0).map(|s| s.end / score.bar()).max().unwrap_or(0)
    }

    /// The piece is whole choruses between its intro and its ending, every
    /// chorus on the song's own rows, its parts whole half-phrases, its
    /// harmony the twelve-bar's, its tune in its register; nothing struck
    /// ends past the end, only what is held.
    #[test]
    fn a_piece_is_whole_choruses() {
        for seed in 0..32 {
            let (score, form) = compose(&Params::of(crate::pieces::find("minor-blues").unwrap(), seed));
            let bars = form.bars();
            assert_eq!(bars % CHORUS, 0, "seed {seed}: {bars} bars");
            for s in score.sections.iter().filter(|s| !s.rings) {
                assert_eq!((s.end - s.start) % (phrase::BARS / 2 * score.bar()), 0, "seed {seed}: {} of broken half-phrases", s.name);
            }
            let intro = intro_bars(&score) as usize;
            let body = &score.harmony[intro..intro + bars as usize];
            for (i, chord) in body.iter().enumerate() {
                assert_eq!(chord.size, 4);
                assert!(matches!(chord.root, 0 | 1 | 3 | 4 | 5), "seed {seed}: bar {i} on {}", chord.root);
                if i % CHORUS as usize != CHORUS as usize - 1 {
                    assert_eq!(*chord, body[i % CHORUS as usize], "seed {seed}: bar {i} off the song's rows");
                }
            }
            assert_eq!(body[0].root, 0);
            assert!(matches!(body[8].root, 1 | 4 | 5));
            if form.design.intro == Intro::Turnaround {
                assert_eq!(score.harmony[intro - 1].root, 4, "seed {seed}: the intro does not end on the V");
            }
            let (lo, hi) = form.design.register;
            for n in score.notes.iter().filter(|n| matches!(n.channel, CH_LEAD | CH_SECOND) && n.len >= E / 2) {
                assert!((lo..=hi).contains(&n.pitch), "seed {seed}: the tune at {} leaves its register {lo}..{hi}", n.pitch);
            }
            let end = score.end();
            for n in &score.notes {
                let role = score.instrument(n.channel).role;
                assert!(n.end() <= end || matches!(role, Role::Drone | Role::Sustain), "seed {seed}: {} past the end", score.instrument(n.channel).name);
            }
        }
    }

    /// Wandering, the band plays its slow feels and quiet stories, opens
    /// on a vamp or in free time and never stops time, and its walk is the
    /// story's whole; in a fight it never drops out, never opens in free
    /// time or on a vamp, never slows into its last chord, and takes the
    /// quick third of its feel's tempo.
    #[test]
    fn a_setting_leans_the_song() {
        let track = crate::pieces::find("minor-blues").unwrap();
        for seed in 0..32 {
            let (score, form) = compose(&Params { setting: Setting::Ambient, ..Params::of(track, seed) });
            assert_ne!(form.design.feel, Feel::Swing, "seed {seed}: the walking four while wandering");
            assert!(matches!(score.story, "late night" | "after hours" | "stroll" | "corner"), "seed {seed}: {}", score.story);
            assert!(matches!(form.design.intro, Intro::Vamp | Intro::Rubato), "seed {seed}: {:?}", form.design.intro);
            assert_ne!(form.design.event, Event::StopTime, "seed {seed}");
            let rungs = STORIES.iter().find(|s| s.name == score.story).unwrap().ladder.len();
            assert!(form.walk.parts.iter().any(|p| p.rung == rungs), "seed {seed}: the story never reaches its top");
            let (score, form) = compose(&Params { setting: Setting::Combat, ..Params::of(track, seed) });
            assert_ne!(form.design.event, Event::Drop, "seed {seed}");
            assert!(!matches!(form.design.intro, Intro::Rubato | Intro::Vamp | Intro::SoloChorus), "seed {seed}: {:?}", form.design.intro);
            assert!(!form.design.ritard, "seed {seed}: slows");
            let (lo, hi) = form.design.groove.tempo;
            assert!(score.eighth_bpm >= lo as f32 + (hi - lo) as f32 * 2.0 / 3.0 - 1.0, "seed {seed}: {} under the quick third", score.eighth_bpm);
        }
    }

    /// The seeds take every feel, intro and ending, the bands every way of
    /// answering, and the lead solos between its heads.
    #[test]
    fn the_songs_are_their_own() {
        let bands: Vec<&'static crate::band::Band> = crate::band::of_style(crate::band::Style::Blues).collect();
        let track = crate::pieces::find("minor-blues").unwrap();
        let designs: Vec<(Score, Form)> = (0..48).map(|seed| compose(&Params { band: bands[seed as usize % bands.len()], ..Params::of(track, seed) })).collect();
        let took = |f: &dyn Fn(&Form) -> bool| designs.iter().any(|(_, form)| f(form));
        for feel in FEELS {
            assert!(took(&|f| f.design.feel == feel), "{feel:?} never taken");
        }
        for intro in [Intro::Vamp, Intro::SoloChorus, Intro::Turnaround, Intro::Rubato, Intro::Straight] {
            assert!(took(&|f| f.design.intro == intro), "{intro:?} never taken");
        }
        for ending in [Ending::Held, Ending::VampOut, Ending::Break] {
            assert!(took(&|f| f.design.ending == ending), "{ending:?} never taken");
        }
        for answers in [Answers::Obbligato, Answers::Sparse, Answers::Pads] {
            assert!(took(&|f| f.design.answers == answers), "{answers:?} never taken");
        }
        for (score, form) in &designs {
            let choruses = form.solos.len();
            if choruses >= 3 {
                assert!(form.solos.iter().any(|s| *s), "{}: no solo in {choruses} choruses", score.summary);
                let intro = intro_bars(score);
                assert!(score.notes.iter().any(|n| n.channel == CH_LEAD && n.start / score.bar() >= intro && form.solo(n.start / score.bar() - intro) && n.start < (intro + form.bars()) * score.bar()));
            }
            assert!(!form.solos[choruses - 1], "the last chorus is a solo");
        }
    }
}

