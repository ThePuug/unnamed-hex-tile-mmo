//! A horo: a Bulgarian circle dance as a wedding band plays it, taken
//! at a run. Its tune is several sections, its kolena, each a theme on
//! its own question and answer played twice running, the round of them
//! coming again, a turn for the lead over a vamp after the first round
//! where the seed gives one; how it opens, how its tempo moves and how
//! it ends are drawn as often as the dances surveyed do them, most
//! stopping on the cadence of a section the listener knows. What makes
//! a horo drive is its rhythm before its tune: the
//! bass and the tambura's strum on every eighth of the dance's limp, the
//! tapan's deep head on the long group and its thin stick on the
//! others, the brass stabbing the strokes and pushing off the beat, the
//! accordion holding where it thickens and the saxophone doubling the
//! lead's riff; the dance is told in layers joining, and a breakdown is
//! the breath the next figure lands out of. The harmony moves, to the
//! minor's flat sixth or Hijaz's flat second, with no drone under it,
//! and the tune is told by one lead — the zurna, the fiddle, the
//! clarinet or the trumpet — mostly as a riff. Every choice is a draw
//! from the seed's stream,
//! one fork per purpose; the seed picks which shape at every level,
//! never the next note.
//!
//! A setting leans the dance's draws and bounds its tempo (`lean`): in a
//! fight the band takes the quick dances at the quick end and holds one
//! tempo; as it is, the dance is as the records play it.

use crate::ladder::{self, turn, Bed, Bounds, Run, Story, Walk};
use crate::band::Part;
use crate::pieces::{Params, Setting};
use crate::rng::Rng;
use crate::score::{Instrument, Note, Role, Score, Section, TICKS_PER_EIGHTH as E};
use crate::solo;
use crate::teller::{self, Hold, Teller, Telling};
use crate::theory::groove::{Groove, HORO};
use crate::theory::melody::{run_between, Theme, FOLK_SHAPES};
use crate::theory::phrase::{self, Form as PhraseForm, FORMS};
use crate::theory::schema::{horo_schemata, Schema};
use crate::theory::{Chord, Key, Mode};
use crate::variation::{self, Role as Bar};
use crate::tune::{self, Placed, Tune};

/// General MIDI programs, 0-based.
const ACCORDION: u8 = 21;
const NYLON_GUITAR: u8 = 24;
const STEEL_GUITAR: u8 = 25;
const FINGER_BASS: u8 = 33;
const TRUMPET: u8 = 56;
const TROMBONE: u8 = 57;
const TUBA: u8 = 58;
const FRENCH_HORN: u8 = 60;
const CLARINET: u8 = 71;
const FIDDLE: u8 = 110;
const SHANAI: u8 = 111;
const TENOR_SAX: u8 = 66;
/// The standard kit, and the keys the tapan's strokes are played on:
/// its deep head on the bass drum, its beater on the kick, its thin
/// stick on the side stick.
const KIT: u8 = 0;
const BASS_DRUM: u8 = 35;
const KICK: u8 = 36;
const SIDE_STICK: u8 = 37;
const FLOOR_TOM: u8 = 41;
const CRASH: u8 = 49;
/// The toms a fill falls down through, high to low.
const FILL: [u8; 4] = [50, 47, 45, 43];

const CH_BASS: u8 = 0;
const CH_FIGURE: u8 = 1;
const CH_LEAD: u8 = 2;
const CH_TROMBONE: u8 = 3;
const CH_SECOND: u8 = 4;
const CH_TUBA: u8 = 5;
const CH_CHOIR: u8 = 6;
const CH_DOUBLE: u8 = 7;
const CH_KIT: u8 = 9;
const CH_WEAVE: u8 = 10;
const CH_HORN: u8 = 11;
const CH_PAD: u8 = 12;

/// How long the room rings: a village square, shorter than the
/// overworld's hall, so a stroke lands and is gone.
const ROOM_S: f32 = 1.6;

/// Who may lead: the players that cut through a band, the zurna first
/// among them. Each seed draws one.
const LEADS: [u8; 4] = [SHANAI, FIDDLE, CLARINET, TRUMPET];
/// Who may hold tones under the lead and take a phrase it hands over.
const SECONDS: [u8; 5] = [FRENCH_HORN, ACCORDION, FIDDLE, CLARINET, TRUMPET];

/// The lead's level, dB, whichever it is: forward of any one player of
/// the band, a little under the band as a whole. The render evens what
/// the bank gives each player, so one level holds for every lead.
const LEAD: f32 = 3.0;

/// Every other player's level, dB: the band is dense, rhythm first, and
/// sits under the lead together so its own balance holds; the lead
/// sits some three dB under it where it plays.
const BAND: f32 = -3.0;

/// The velocity every voice strikes at before its own accent: a dance
/// at a run strikes harder than the overworld's bed.
const VEL: i32 = 90;

/// What each layer at each notch lifts the loudness by, LU, and what
/// each telling does, which the pedal takes back through the part, so
/// a full dance is no louder than a sparse one: the story is told in
/// what plays. Fitted to the render, every section of forty seeds
/// against what was on in it; the range check holds them true.
const LIFT_OSTINATO: [f32; 3] = [0.0, 1.2, 1.35];
/// The drum is under every part of every story, so only its step up
/// is measured; its strokes are the floor the rest is measured on.
const LIFT_DRUM: [f32; 3] = [0.0, 0.0, 0.3];
const LIFT_BRASS: [f32; 3] = [0.0, 0.4, 0.6];
const LIFT_PAD: [f32; 3] = [0.0, 0.4, 1.8];
const LIFT_WEAVE: [f32; 2] = [0.0, 0.1];
/// By telling, in `Telling`'s order. A part with the lead silent sits a
/// LU and a half under the dance: raised the whole way, the band alone
/// is lifted by all the lead gave, and its strokes are the piece's
/// peaks.
const LIFT_LEAD: [f32; 6] = [1.5, 6.8, 6.5, 6.1, 5.65, 6.95];
/// The drum alone is let sit a LU and a half further under the dance:
/// held as loud as all of it, its strokes are the piece's peaks.
const LIFT_ALONE: f32 = 1.5;

/// The tune's register; the lead takes it here, the echo an octave
/// under. Its middle puts every key's home between E4 and D5, so no key
/// throws the tune an octave up into a shriek, and it is wide enough
/// over home for the climb's third on a phrase pair's step.
const TUNE: (u8, u8) = (60, 89);
/// The tune a third up on the ladder's top rung.
const CLIMB: i32 = 2;

/// The bass and the tambura on every eighth, the dance's engine.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Ostinato {
    Off,
    /// The bass on the root, every eighth, the group's first struck.
    Low,
    /// The tambura's strum too, a figure through the chord each group.
    Full,
}

/// The davul.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Drum {
    Off,
    /// The low stroke with the deep head under it, and the thin stick on the
    /// other groups' first beats: every group struck.
    Strokes,
    /// The stick's pickups into the groups too, and the cymbal at every
    /// phrase.
    Full,
}

/// The low brass.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Brass {
    Off,
    /// Trombone and tuba on the drum's strokes.
    Stabs,
    /// And on the off-beats between, pushing.
    Full,
}

/// Held chords where the dance thickens.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Pad {
    Off,
    /// The accordion on root and fifth.
    Thin,
    /// The clarinet held on the chord and the horn on its third.
    Full,
}

/// The tune echoed under the dance, on the guitar.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Weave {
    Off,
    /// A bar behind, an octave under.
    On,
}

/// A layer of the bed, the thing a rung of the ladder moves.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Layer {
    Ostinato,
    Drum,
    Brass,
    Pad,
    Weave,
}

/// What the bed is: each layer at its notch.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Texture {
    ostinato: Ostinato,
    drum: Drum,
    brass: Brass,
    pad: Pad,
    weave: Weave,
}

impl Texture {
    const BARE: Texture = Texture { ostinato: Ostinato::Off, drum: Drum::Off, brass: Brass::Off, pad: Pad::Off, weave: Weave::Off };

    /// The pedal's gain under this texture and `lead`: what they lift
    /// the loudness by, given back.
    fn trim(&self, lead: Telling) -> f32 {
        let alone = if self.ostinato == Ostinato::Off && self.brass == Brass::Off && self.pad == Pad::Off { LIFT_ALONE } else { 0.0 };
        ladder::gain(LIFT_OSTINATO[self.ostinato as usize] + LIFT_DRUM[self.drum as usize] + LIFT_BRASS[self.brass as usize] + LIFT_PAD[self.pad as usize] + LIFT_WEAVE[self.weave as usize] + LIFT_LEAD[lead as usize] + alone)
    }
}

impl Bed for Texture {
    type Layer = Layer;

    fn up(self, layer: Layer) -> Texture {
        match layer {
            Layer::Ostinato => Texture { ostinato: if self.ostinato == Ostinato::Off { Ostinato::Low } else { Ostinato::Full }, ..self },
            Layer::Drum => Texture { drum: if self.drum == Drum::Off { Drum::Strokes } else { Drum::Full }, ..self },
            Layer::Brass => Texture { brass: if self.brass == Brass::Off { Brass::Stabs } else { Brass::Full }, ..self },
            Layer::Pad => Texture { pad: if self.pad == Pad::Off { Pad::Thin } else { Pad::Full }, ..self },
            Layer::Weave => Texture { weave: Weave::On, ..self },
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
        if from.ostinato != to.ostinato {
            moved(from.ostinato as u8, to.ostinato as u8, ["ostinato in", "ostinato up", "ostinato down", "ostinato out"])
        } else if from.drum != to.drum {
            moved(from.drum as u8, to.drum as u8, ["drum in", "drum up", "drum down", "drum out"])
        } else if from.brass != to.brass {
            moved(from.brass as u8, to.brass as u8, ["brass in", "brass up", "brass down", "brass out"])
        } else if from.pad != to.pad {
            moved(from.pad as u8, to.pad as u8, ["pad in", "pad up", "pad down", "pad out"])
        } else if from.weave != to.weave {
            if to.weave == Weave::On { "echo in" } else { "echo out" }
        } else {
            "held"
        }
    }
}

use Layer::{Brass as BrassLayer, Drum as DrumLayer, Ostinato as OstinatoLayer, Pad as PadLayer, Weave as WeaveLayer};
use Telling::{Long, Off, Phrases, Riff, RiffAndLong, Trading};

/// Every story's parts are two or three half-phrases: the dances' bars
/// are short at a run, and a part must stand long enough to be one. A
/// horo repeats its figures, so a story climbs, eases and climbs again,
/// and ends at its height, where the dance ends — how far it eases, and
/// whether it eases at all, the seed's, as how long it stays at each turn
/// is, so two seeds of one story keep their own shape. At
/// its crest the lead sings the tune, or holds long tones over all of
/// it, the wail a riff never gives.
const STORIES: [Story<Texture, Telling>; 4] = [
    // The dance joined on the drum and the bass, built to all of it.
    Story {
        name: "gathering",
        weight: 3.0,
        base: Texture { ostinato: Ostinato::Low, drum: Drum::Strokes, ..Texture::BARE },
        ladder: &[DrumLayer, BrassLayer, OstinatoLayer, PadLayer, BrassLayer, PadLayer, WeaveLayer],
        leads: &[Riff, Trading, Riff, Phrases, Trading, RiffAndLong, RiffAndLong, Phrases],
        turns: &[turn((7, 7), (0, 2)), turn((3, 7), (0, 1)), turn((7, 7), (1, 2))],
        halves: (2, 3),
        pace: (0.2, 0.8),
    },
    // In at full stride and never letting up.
    Story {
        name: "full swing",
        weight: 2.0,
        base: Texture { ostinato: Ostinato::Full, drum: Drum::Full, brass: Brass::Stabs, ..Texture::BARE },
        ladder: &[PadLayer, BrassLayer, WeaveLayer, PadLayer],
        leads: &[Trading, Riff, Trading, RiffAndLong, Phrases],
        turns: &[turn((4, 4), (1, 3)), turn((1, 4), (0, 1)), turn((4, 4), (1, 3))],
        halves: (2, 3),
        pace: (0.6, 1.0),
    },
    // The drum alone, then the dance snaps in around it, the lead a
    // part after the engine.
    Story {
        name: "drum first",
        weight: 2.0,
        base: Texture { drum: Drum::Strokes, ..Texture::BARE },
        ladder: &[OstinatoLayer, DrumLayer, OstinatoLayer, BrassLayer, BrassLayer, PadLayer, WeaveLayer],
        leads: &[Off, Off, Phrases, Trading, Riff, RiffAndLong, Trading, Phrases],
        turns: &[turn((3, 3), (0, 0)), turn((2, 2), (0, 0)), turn((7, 7), (0, 2)), turn((3, 7), (0, 0)), turn((7, 7), (1, 2))],
        halves: (2, 3),
        pace: (0.3, 0.9),
    },
    // Up, a breakdown to the engine alone — the bass and the drum, the
    // lead silent — the breath the next figure lands out of, and up
    // again.
    Story {
        name: "breather",
        weight: 2.0,
        base: Texture { ostinato: Ostinato::Low, drum: Drum::Full, ..Texture::BARE },
        ladder: &[OstinatoLayer, BrassLayer, PadLayer, BrassLayer, PadLayer, WeaveLayer],
        leads: &[Off, Trading, Riff, RiffAndLong, Trading, RiffAndLong, Long],
        turns: &[turn((6, 6), (0, 2)), turn((0, 1), (0, 1)), turn((6, 6), (1, 2))],
        halves: (2, 3),
        pace: (0.3, 1.0),
    },
];

/// One of the dance's sections, its kolena: a theme on its own question
/// and its own answer, two phrases, played twice running as a horo
/// plays each of its figures.
struct Kolyano {
    theme: Theme,
    open: &'static Schema,
    closed: &'static Schema,
}

/// What a phrase pair of the dance plays: a kolyano, or the lead's own
/// turn over the vamp.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Pair {
    Kolyano(usize),
    Solo,
}

/// How the dance opens, as the dances surveyed open: most straight in,
/// the band together from its first stroke; some after a solo in free
/// time over a held chord, the taksim; the tapan alone for a bar is the
/// tradition's least written down, kept the least drawn.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Opening {
    Straight,
    Taksim,
    Tapan,
}

impl Opening {
    fn name(self) -> &'static str {
        match self {
            Opening::Straight => "straight in",
            Opening::Taksim => "a taksim",
            Opening::Tapan => "the tapan",
        }
    }

    /// The bars it lays before the dance.
    #[cfg(test)]
    fn bars(self) -> u32 {
        match self {
            Opening::Straight => 0,
            Opening::Taksim => TAKSIM_BARS,
            Opening::Tapan => 1,
        }
    }
}

/// How the tempo moves, a phrase pair at a time and never smoothly, so
/// the meter holds: most often not at all, as Bulgarian dance records hold
/// theirs; else — the shapes a horo's tempo is written down taking:
/// pressing on through the last quarter, as a band pushes its dancers; a
/// step faster at every new section; building from the first pair to the
/// last; or faster to past the middle and back, giving the dancers a rest.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Pace {
    Steady,
    Press,
    Steps,
    Build,
    Arch,
}

impl Pace {
    fn name(self) -> &'static str {
        match self {
            Pace::Steady => "one tempo",
            Pace::Press => "pressing on",
            Pace::Steps => "a step a section",
            Pace::Build => "building",
            Pace::Arch => "up and back",
        }
    }
}

/// How the dance ends, as the dances surveyed end: most stop on the
/// cadence of a section the listener knows — the first come back, or the
/// last played twice — the band striking the tonic once on the downbeat
/// after it; some play the last four bars three times before that stroke;
/// some hold the last tone; the unison run into a hit is a concert's
/// showpiece, seldom a dance's.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Ending {
    Stop,
    Tag,
    Held,
    Run,
}

impl Ending {
    fn name(self) -> &'static str {
        match self {
            Ending::Stop => "a stop",
            Ending::Tag => "the last line three times",
            Ending::Held => "a held tone",
            Ending::Run => "a unison run",
        }
    }
}

/// How a setting leans the dance (`proofs/research/settings-findings.md`,
/// §7): each draw's weights multiplied and the tempo bounded, the walk
/// the story's whole — a walk held at a floor is a dance that never
/// breathes. In a fight: the râčenica and the kopanica most, the
/// five-eight never, full swing twice as often, the quick third of the
/// dance's tempo held from the first bar to the last, straight in or off
/// the tapan — no taksim, no free time before a fight — and out on a
/// stop, a tag or a run, never a held tone. As it is, the dance as the
/// records play it.
struct Lean {
    grooves: [f32; 4],
    stories: [f32; 4],
    openings: [f32; 3],
    paces: [f32; 5],
    endings: [f32; 4],
    bounds: Bounds,
}

const AS_RECORDED: Lean = Lean { grooves: [1.0; 4], stories: [1.0; 4], openings: [1.0; 3], paces: [1.0; 5], endings: [1.0; 4], bounds: ladder::UNBOUNDED };

fn lean(setting: Setting) -> Lean {
    match setting {
        Setting::Combat => Lean {
            grooves: [1.0, 0.0, 1.0, 0.5],
            stories: [1.0, 2.0, 1.0, 1.0],
            openings: [1.0, 0.0, 1.0],
            paces: [1.0, 0.0, 0.0, 0.0, 0.0],
            endings: [1.0, 1.0, 0.0, 1.0],
            bounds: Bounds { tempo: (2.0 / 3.0, 1.0) },
        },
        Setting::Ambient | Setting::City | Setting::None => AS_RECORDED,
    }
}

/// `weights` leaned by `by`.
fn leaned<const N: usize>(weights: [f32; N], by: [f32; N]) -> [f32; N] {
    std::array::from_fn(|i| weights[i] * by[i])
}

/// What the dance draws, in the order their weights are given: the
/// setting leans each, and the band after it, by name.
const OPENINGS: [Opening; 3] = [Opening::Straight, Opening::Taksim, Opening::Tapan];
const PACES: [Pace; 5] = [Pace::Steady, Pace::Press, Pace::Steps, Pace::Build, Pace::Arch];
const ENDINGS: [Ending; 4] = [Ending::Stop, Ending::Tag, Ending::Held, Ending::Run];

/// What a seed's piece is.
struct Design {
    /// The one player who tells the tune.
    lead: u8,
    groove: &'static Groove,
    form: PhraseForm,
    /// The dance's kolena, in the order they first come.
    kolena: Vec<Kolyano>,
    /// Whether the lead takes its turn over the vamp after the first
    /// round of the kolena, and the vamp it takes it over: the mode's
    /// shuttle between two chords.
    solo: bool,
    vamp: Option<&'static Schema>,
    /// Whether the dance comes back to its first kolyano to end on.
    returns: bool,
    opening: Opening,
    pace: Pace,
    ending: Ending,
}

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
    /// What each phrase pair of the walk plays.
    order: Vec<Pair>,
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
    /// Whether the lead takes its turn at `bar`.
    fn solo(&self, bar: u32) -> bool {
        self.order.get((bar / PAIR_BARS) as usize) == Some(&Pair::Solo)
    }
    /// The walk's runs, the lead silent through its turn, which it plays
    /// itself.
    fn runs(&self) -> Vec<Run<Telling>> {
        let mut runs: Vec<Run<Telling>> = Vec::new();
        for b in 0..self.bars() {
            let lead = if self.solo(b) { Off } else { self.walk.at(b).lead };
            match runs.last_mut() {
                Some(r) if r.lead == lead => r.b = b + 1,
                _ => runs.push(Run { lead, a: b, b: b + 1 }),
            }
        }
        runs
    }
}

/// The bars of a phrase pair, a question and its answer.
const PAIR_BARS: u32 = 2 * phrase::BARS;

/// The bars of the taksim.
const TAKSIM_BARS: u32 = 2;

/// What each phrase pair of a dance `pairs` long plays: each of `n`
/// kolena twice running, as many as fit twice; then the lead's turn over
/// the vamp where `solo` and there is room for it and a kolyano after; then
/// the round again from the second kolyano, as a band comes back round its
/// figures; the last pair the first kolyano where the dance `returns` to
/// it.
fn order(pairs: usize, n: usize, solo: bool, returns: bool) -> Vec<Pair> {
    let n = n.min((pairs / 2).max(1));
    let mut seq: Vec<Pair> = (0..n).flat_map(|k| [Pair::Kolyano(k); 2]).collect();
    let turn = if !solo { 0 } else if pairs >= seq.len() + 4 { 2 } else if pairs >= seq.len() + 2 { 1 } else { 0 };
    seq.extend(std::iter::repeat_n(Pair::Solo, turn));
    let mut k = 1 % n;
    while seq.len() < pairs {
        seq.extend([Pair::Kolyano(k); 2]);
        k = (k + 1) % n;
    }
    seq.truncate(pairs);
    if returns {
        *seq.last_mut().unwrap() = Pair::Kolyano(0);
    }
    seq
}

pub fn build(params: &Params) -> Score {
    compose(params).0
}

/// The score, and the form it was written on.
fn compose(params: &Params) -> (Score, Form) {
    let rng = Rng::new(params.seed);
    let mut skeleton = rng.fork(0);
    // The band: its members.
    let band = params.band;
    let lean = lean(params.setting);
    let stories: Vec<f32> = STORIES.iter().zip(lean.stories).map(|(s, by)| s.weight * by * band.lean(s.name)).collect();
    let story = &STORIES[skeleton.weighted(&stories)];
    let mode = [Mode::Aeolian, Mode::Hijaz][skeleton.weighted(&[3.0, 3.0])];
    let key = Key::new(["D", "E", "G", "A", "C"][skeleton.weighted(&[3.0, 3.0, 2.0, 2.0, 1.0])], mode);
    // The five-eight least: at a run its bar is under a second.
    let groove = &HORO[skeleton.weighted(&leaned(leaned([3.0, 1.0, 2.0, 2.0], lean.grooves), std::array::from_fn(|i| band.lean(HORO[i].name))))];
    let tempo = story.tempo(groove.tempo, &lean.bounds.within(band.prefs.tempo), &mut skeleton);
    let (open, closed) = horo_schemata(mode);
    // The band's first lead the horo has a voice for.
    let lead = band.program(Part::Lead, &LEADS, LEADS[0]);
    let form = FORMS[skeleton.below(FORMS.len())];
    // Each kolyano its own theme and, as far as the mode has them, its own
    // question and answer, so two running never share their chords.
    let (first_open, first_closed) = (skeleton.below(open.len()), skeleton.below(closed.len()));
    let kolena: Vec<Kolyano> = (0..[3, 4][skeleton.weighted(&[3.0, 2.0])])
        .map(|k| Kolyano { theme: Theme::draw(groove, &FOLK_SHAPES, &mut skeleton), open: open[(first_open + k) % open.len()], closed: closed[(first_closed + k) % closed.len()] })
        .collect();
    let vamp = open.iter().copied().find(|s| s.roots[0] == s.roots[2] && s.roots[1] == s.roots[3] && s.roots[0] != s.roots[1]);
    let design = Design {
        lead,
        groove,
        form,
        kolena,
        solo: vamp.is_some() && skeleton.chance(0.6),
        vamp,
        returns: skeleton.chance(0.6),
        opening: OPENINGS[skeleton.weighted(&leaned(leaned([3.0, 2.0, 1.0], lean.openings), OPENINGS.map(|o| band.lean(o.name()))))],
        // Bulgarian dance records hold one tempo, fifteen in seventeen; the
        // rest press on, most from about halfway.
        pace: PACES[skeleton.weighted(&leaned(leaned([15.0, 1.0, 1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0], lean.paces), PACES.map(|p| band.lean(p.name()))))],
        ending: ENDINGS[skeleton.weighted(&leaned(leaned([11.0, 3.0, 3.0, 2.0], lean.endings), ENDINGS.map(|e| band.lean(e.name()))))],
    };
    let instruments = vec![
        Instrument { name: "bass", program: band.program(Part::Bass, &[], FINGER_BASS), channel: CH_BASS, role: Role::Pluck, low: 36, high: 60, reverb: 20, pan: 0, level: BAND },
        Instrument { name: "tambura", program: band.program(Part::Figure, &[], STEEL_GUITAR), channel: CH_FIGURE, role: Role::Pluck, low: 45, high: 64, reverb: 35, pan: -26, level: BAND },
        Instrument { name: "lead", program: design.lead, channel: CH_LEAD, role: Role::Melody, low: 58, high: 92, reverb: 30, pan: 0, level: LEAD },
        Instrument { name: "trombone", program: band.program(Part::Trombone, &[], TROMBONE), channel: CH_TROMBONE, role: Role::Pluck, low: 45, high: 70, reverb: 30, pan: 22, level: BAND },
        Instrument { name: "horn, the second", program: band.programs(Part::Second).into_iter().find(|p| *p != lead && SECONDS.contains(p)).unwrap_or(FRENCH_HORN), channel: CH_SECOND, role: Role::Melody, low: 53, high: 76, reverb: 45, pan: -30, level: BAND },
        Instrument { name: "tuba", program: band.program(Part::Tuba, &[], TUBA), channel: CH_TUBA, role: Role::Pluck, low: 28, high: 45, reverb: 20, pan: 8, level: BAND },
        Instrument { name: "accordion", program: band.program(Part::Accordion, &[], ACCORDION), channel: CH_CHOIR, role: Role::Sustain, low: 52, high: 71, reverb: 70, pan: 30, level: BAND },
        Instrument { name: "sax, the riff", program: band.program(Part::Doubler, &[], TENOR_SAX), channel: CH_DOUBLE, role: Role::Doubling, low: 58, high: 92, reverb: 40, pan: -36, level: BAND },
        Instrument { name: "tapan", program: band.program(Part::Drums, &[], KIT), channel: CH_KIT, role: Role::Percussion, low: BASS_DRUM, high: 50, reverb: 25, pan: 0, level: BAND },
        Instrument { name: "echo", program: band.program(Part::Echo, &[], NYLON_GUITAR), channel: CH_WEAVE, role: Role::Pluck, low: 46, high: 80, reverb: 35, pan: 40, level: BAND },
        Instrument { name: "horn", program: band.program(Part::Horn, &[], FRENCH_HORN), channel: CH_HORN, role: Role::Sustain, low: 53, high: 65, reverb: 50, pan: -42, level: BAND },
        Instrument { name: "clarinet, held", program: band.program(Part::HeldReed, &[], CLARINET), channel: CH_PAD, role: Role::Sustain, low: 57, high: 74, reverb: 60, pan: -18, level: BAND },
    ];
    let mut score = Score::new(key, groove.meter(), tempo, instruments, ROOM_S);
    score.lead = Some(CH_LEAD);
    score.played_by(
        band,
        &[
            (CH_BASS, Part::Bass),
            (CH_FIGURE, Part::Figure),
            (CH_LEAD, Part::Lead),
            (CH_TROMBONE, Part::Trombone),
            (CH_SECOND, Part::Second),
            (CH_TUBA, Part::Tuba),
            (CH_CHOIR, Part::Accordion),
            (CH_DOUBLE, Part::Doubler),
            (CH_KIT, Part::Drums),
            (CH_WEAVE, Part::Echo),
            (CH_HORN, Part::Horn),
            (CH_PAD, Part::HeldReed),
        ],
    );
    let name = |program: u8| match program {
        SHANAI => "zurna",
        FIDDLE => "fiddle",
        CLARINET => "clarinet",
        TRUMPET => "trumpet",
        _ => "?",
    };
    let bar = score.bar();

    // The walk, in half-phrases, and whole question-and-answer pairs
    // of them, so it closes on an answer.
    let walk = story.place(&mut skeleton, &mut score, 4, &[], |texture, lead| texture.trim(lead));
    let order = order((walk.bars() / PAIR_BARS) as usize, design.kolena.len(), design.solo, design.returns);
    let kolena = order.iter().filter_map(|p| if let Pair::Kolyano(k) = p { Some(*k) } else { None }).max().unwrap_or(0) + 1;
    let soloed = order.contains(&Pair::Solo);
    score.summary = format!(
        "{} on the {}: {} kolena in a {:?}, {}; {} lead; opens {}, {}, ends on {}",
        story.name,
        groove.name,
        kolena,
        design.form,
        if soloed { "the lead's turn over the vamp after the first round" } else { "no solo" },
        name(design.lead),
        design.opening.name(),
        design.pace.name(),
        design.ending.name(),
    );

    // A phrase pair's themes and rows: the kolyano's, or the vamp's under
    // the lead's turn. The tune comes back the same each time a kolyano
    // does, but a third up on the ladder's top rung.
    let mut themes: Vec<&Theme> = Vec::new();
    let mut rows: Vec<&Schema> = Vec::new();
    for pair in &order {
        match pair {
            Pair::Kolyano(k) => {
                let kolyano = &design.kolena[*k];
                themes.extend([&kolyano.theme; 2]);
                rows.extend([kolyano.open, kolyano.closed]);
            }
            Pair::Solo => {
                themes.extend([&design.kolena[0].theme; 2]);
                rows.extend([design.vamp.unwrap(); 2]);
            }
        }
    }
    let upper = story.ladder.len().max(1);
    let shifts: Vec<i32> = (0..walk.bars()).map(|b| if walk.at(b).rung >= upper { CLIMB } else { 0 }).collect();
    let tune = Tune::compose(&themes, &score.meter, design.form, &rows, 3, shifts);
    score.harmony = tune.chords.clone();
    let form = Form { bar, walk, tune, design, order };

    ostinato(&mut score, &form, &mut rng.fork(1));
    drum(&mut score, &form, &mut rng.fork(2));
    brass(&mut score, &form, &mut rng.fork(3));
    pad(&mut score, &form, &mut rng.fork(4));
    weave(&mut score, &form, &mut rng.fork(5));
    let teller = Teller {
        lead: CH_LEAD,
        second: CH_SECOND,
        double: Some((CH_DOUBLE, -18)),
        echo: CH_WEAVE,
        register: TUNE,
        sung: -6,
        riff: (10, 0),
        long: (72, 84, -6),
        under: (55, 67, -14),
        hold: Hold::Breathing,
        breathes: true,
        vel,
        fills: 0.0,
        soars: 0.0,
        pushes: 0.0,
    };
    teller::tell(&mut score, &teller, &form.tune, &form.runs(), &mut rng.fork(6));
    improvise(&mut score, &form, &mut rng.fork(9));
    score.mark_phrases(0, form.bars());
    pace(&mut score, &form);
    ending(&mut score, &form, &mut rng.fork(7));
    match form.design.opening {
        Opening::Straight => {}
        Opening::Tapan => tapan(&mut score, form.design.groove, &mut rng.fork(8)),
        Opening::Taksim => taksim(&mut score, &form, &mut rng.fork(8)),
    }
    score.finish();
    (score, form)
}

/// How much faster the dance is at each step than at the one before, and
/// the most it presses on in all: a band pushes its dancers a phrase at
/// a time, never smoothly, so the meter holds.
const PRESS: f32 = 0.05;
const PRESS_MOST: f32 = 0.2;

/// The bars the last stroke rings after it lands.
const RING_BARS: u32 = 2;

/// The dance's tempo as its `Pace` moves it, a step at a phrase's or a
/// pair's first bar: pressing on through the last quarter a phrase at a
/// time; a step at every pair that plays something new; a step every
/// pair from the first to the most at the last; or a step every pair to
/// the most two thirds through and back down a step a pair to where it
/// began.
fn pace(score: &mut Score, form: &Form) {
    let bars = form.bars();
    let base = score.eighth_bpm;
    let pairs = form.order.len();
    let steps: Vec<(u32, f32)> = match form.design.pace {
        Pace::Steady => Vec::new(),
        Pace::Press => (0..bars).step_by(phrase::BARS as usize).filter(|b| *b >= bars * 3 / 4).enumerate().map(|(k, b)| (b, PRESS * (k + 1) as f32)).collect(),
        Pace::Steps => {
            let changes = (1..pairs).filter(|p| form.order[*p] != form.order[p - 1]);
            changes.enumerate().map(|(k, p)| (p as u32 * PAIR_BARS, PRESS * (k + 1) as f32)).collect()
        }
        Pace::Build => (1..pairs).map(|p| (p as u32 * PAIR_BARS, PRESS_MOST * p as f32 / (pairs - 1) as f32)).collect(),
        Pace::Arch => {
            let peak = (pairs * 2 / 3).max(1);
            (1..pairs).map(|p| (p as u32 * PAIR_BARS, if p <= peak { PRESS * p as f32 } else { PRESS * peak as f32 - PRESS * (p - peak) as f32 })).collect()
        }
    };
    for (b, faster) in steps {
        score.tempo.push((b * form.bar, base * (1.0 + faster.clamp(0.0, PRESS_MOST))));
    }
}

/// The opening: the tapan alone for a bar, the dance's strokes — its
/// beater and deep head on the long group, its stick on the others —
/// so the dancers have the step before the band comes in.
fn tapan(score: &mut Score, groove: &Groove, rng: &mut Rng) {
    score.delay(&[Chord::triad(0)]);
    let bar = score.bar();
    let trim = score.sections.first().map_or(1.0, |s| s.trim);
    score.sections.insert(0, Section { name: "tapan", start: 0, end: bar, trim, level: None, rings: false });
    for i in groove.dum {
        let accent = if *i == 0 { 4 } else { -4 };
        score.add(Note { start: i * E, len: E - 20, pitch: KICK, vel: vel(accent, rng), channel: CH_KIT });
        score.add(Note { start: i * E, len: E - 20, pitch: BASS_DRUM, vel: vel(accent - 10, rng), channel: CH_KIT });
    }
    for i in groove.tek {
        score.add(Note { start: i * E, len: E / 2, pitch: SIDE_STICK, vel: vel(-6, rng), channel: CH_KIT });
    }
}

/// The taksim: the lead alone in free time over the accordion's held
/// tonic, two of the dance's bars taken at under half its pace and
/// wavering beat to beat — a tone high in the mode held, falling by
/// runs to rest on home — before the dance comes in at its own tempo.
fn taksim(score: &mut Score, form: &Form, rng: &mut Rng) {
    let tonic = Chord::triad(0);
    score.delay(&[tonic; TAKSIM_BARS as usize]);
    let bar = score.bar();
    let key = score.key;
    let trim = score.sections.first().map_or(1.0, |s| s.trim);
    score.sections.insert(0, Section { name: "taksim", start: 0, end: TAKSIM_BARS * bar, trim, level: None, rings: false });
    let root = at_degree(&key, tonic, 0, 52, 64, 57);
    let fifth = at_degree(&key, tonic, 4, root + 1, 71, root + 7);
    for p in [root, fifth] {
        score.add(Note { start: 0, len: TAKSIM_BARS * bar + E / 8, pitch: p, vel: vel(-22, rng), channel: CH_CHOIR });
    }
    let home = key.absolute_degree(tune::home_tonic(&key, TUNE.0, TUNE.1)).unwrap();
    let mut line = phrased(score, (0, TAKSIM_BARS), |x| home + (5.0 * (1.0 - x)).round() as i32, 0.3, 0.6, rng);
    form.tune.repair(score, &mut line, TUNE.0, TUNE.1);
    sound(score, &line, rng);
    let base = score.eighth_bpm;
    let beats: Vec<u32> = (0..TAKSIM_BARS * bar).filter(|t| score.strong(*t)).collect();
    for t in beats {
        score.tempo.push((t, base * rng.range(34, 48) as f32 / 100.0));
    }
    if !score.tempo.iter().any(|(t, _)| *t == TAKSIM_BARS * bar) {
        score.tempo.push((TAKSIM_BARS * bar, base));
    }
    score.tempo.sort_by_key(|(t, _)| *t);
}

/// The lead's turn over the vamp, as a wedding band's soloist takes it
/// (`solo::BALKAN`): sixteenths regrouped against the limp, turns and
/// trills about a tone, cells sequenced, phrases of two or four bars each
/// closed on a held tone, faster as it goes; every strong beat's tone on
/// the bar's chord and the line repaired as the tune is, and the line a
/// listener follows through a run, its tones longer than a sixteenth,
/// too. A second turn answers the first.
fn improvise(score: &mut Score, form: &Form, rng: &mut Rng) {
    let player = &solo::BALKAN;
    let mut answered: Option<solo::Shape> = None;
    let mut b = 0;
    while b < form.bars() {
        if !form.solo(b) {
            b += 1;
            continue;
        }
        let a = b;
        while b < form.bars() && form.solo(b) {
            b += 1;
        }
        let shape = match answered {
            Some(to) => solo::Shape::answer(&to, player, rng),
            None => solo::Shape::draw(player, rng),
        };
        let mut line = solo::turn(score, a, b, TUNE, player, &shape, rng);
        for i in 0..line.len() {
            let n = line[i];
            let key = score.key_at(n.0);
            let chord = score.chord_at(n.0);
            if score.strong(n.0) && !chord.holds(&key, n.2) {
                let prev = i.checked_sub(1).map(|j| line[j].2);
                let next = line.get(i + 1).map(|m| m.2);
                line[i].2 = tune::bent_apart(&key, chord, n.2, prev, next, TUNE.0, TUNE.1);
            }
        }
        form.tune.repair(score, &mut line, TUNE.0, TUNE.1);
        let heard: Vec<usize> = (0..line.len()).filter(|i| line[*i].1 > E / 2).collect();
        let mut skeleton: Vec<Placed> = heard.iter().map(|i| line[*i]).collect();
        form.tune.repair(score, &mut skeleton, TUNE.0, TUNE.1);
        for (k, i) in heard.iter().enumerate() {
            line[*i].2 = skeleton[k].2;
        }
        sound(score, &line, rng);
        answered = Some(shape);
    }
}

/// A free line over bars `a..z`, a group at a time: each group opening on
/// the tone of its bar's chord nearest the `contour` there, from 0 to 1
/// across the span, and running on to the next by steps — in sixteenths
/// with chance `busy` through the middle of the span, else in eighths —
/// or held through the group with chance `held`, and always through a
/// phrase's last group; the last group holds to the end.
fn phrased(score: &Score, (a, z): (u32, u32), contour: impl Fn(f32) -> i32, busy: f32, held: f32, rng: &mut Rng) -> Vec<Placed> {
    let bar = score.bar();
    let strong = score.meter.strong_eighths();
    let eighths = score.meter.eighths();
    let spans: Vec<(u32, u32)> = strong.iter().enumerate().map(|(g, s)| (*s, strong.get(g + 1).copied().unwrap_or(eighths) - s)).collect();
    let groups: Vec<(u32, u32)> = (a..z).flat_map(|b| spans.iter().map(move |(s, len)| (b * bar + s * E, *len))).collect();
    let n = groups.len();
    let firsts: Vec<i32> = groups
        .iter()
        .enumerate()
        .map(|(i, (t, _))| {
            let key = score.key_at(*t);
            let chord = score.chord_at(*t);
            let want = if i + 1 == n { key.absolute_degree(tune::home_tonic(&key, TUNE.0, TUNE.1)).unwrap() } else { contour(i as f32 / (n - 1).max(1) as f32) };
            (want - 3..=want + 3).filter(|d| chord.holds(&key, key.pitch(*d, 4)) && (TUNE.0..=TUNE.1).contains(&key.pitch(*d, 4))).min_by_key(|d| ((d - want).abs(), *d)).unwrap_or(want)
        })
        .collect();
    let mut line = Vec::new();
    for (i, (t, len)) in groups.iter().enumerate() {
        let key = score.key_at(*t);
        let x = i as f32 / n as f32;
        let breath = variation::role(t / bar) == Bar::Cadence && *t % bar + len * E == bar;
        if i + 1 == n || breath || rng.chance(held) {
            let to = if i + 1 == n { z * bar } else { t + len * E };
            line.push((*t, to - t, key.pitch(firsts[i], 4)));
            continue;
        }
        let count = if (0.3..0.85).contains(&x) && rng.chance(busy) { len * 2 } else { *len };
        let unit = len * E / count;
        let tones = std::iter::once(firsts[i]).chain(run_between(firsts[i], firsts[i + 1], count));
        for (k, d) in tones.enumerate() {
            line.push((t + k as u32 * unit, unit, key.pitch(d, 4)));
        }
    }
    line
}

/// A free line played by the lead, every strong beat's tone a little
/// harder and the long tones the hardest.
fn sound(score: &mut Score, line: &[Placed], rng: &mut Rng) {
    for (start, len, pitch) in line {
        let accent = if *len >= 2 * E { 6 } else if score.strong(*start) { 2 } else { -8 };
        let len = if *len <= E / 2 { len.saturating_sub(10) } else { len - E / 8 };
        score.add(Note { start: *start, len, pitch: *pitch, vel: vel(accent, rng), channel: CH_LEAD });
    }
}

/// The dance's ending, as its `Ending` has it. The stop: the band strikes
/// the tonic on the downbeat after the last cadence, once, short, the
/// tapan's beater and deep head with it, and the room rings. The tag
/// plays the last four bars twice more first. The held tone strikes it
/// and holds it a bar under a fermata. The run: a bar on the dominant's
/// chord — the v of the minor, the minor vii of Hijaz, the home a Balkan
/// close comes from — where the band strikes it on the bar, holds, and
/// over its last group runs down together through the mode to the
/// tonic's neighbour, the tapan's snare rolling up into it; then one hit
/// on the tonic by everyone and the crash. The tonic's third is the
/// mode's own, major in Hijaz.
fn ending(score: &mut Score, form: &Form, rng: &mut Rng) {
    let bar = form.bar;
    let at = form.bars();
    let key = score.key;
    let eighths = score.meter.eighths();
    let groups = form.design.groove.groups;
    let tonic = Chord::triad(0);
    let trim = score.sections.last().map_or(1.0, |s| s.trim);
    score.mark_coda(at);
    let ring = |score: &mut Score, from: u32| {
        score.sections.push(Section { name: "end", start: from * bar, end: (from + RING_BARS) * bar, trim, level: None, rings: true });
        score.harmony.extend((0..RING_BARS).map(|_| tonic));
    };
    match form.design.ending {
        Ending::Stop => {
            ring(score, at);
            strike(score, at * bar, groups[0] as u32 * E - E / 8, false, rng);
        }
        Ending::Tag => {
            for _ in 0..2 {
                let start = score.again(at - phrase::BARS, at);
                score.sections.push(Section { name: "tag", start: start * bar, end: (start + phrase::BARS) * bar, trim, level: None, rings: false });
            }
            let end = score.harmony.len() as u32;
            ring(score, end);
            strike(score, end * bar, groups[0] as u32 * E - E / 8, false, rng);
        }
        Ending::Held => {
            let slower = score.bpm_at(at * bar - 1) / 2.0;
            score.tempo.push((at * bar, slower));
            ring(score, at);
            strike(score, at * bar, bar - E / 4, false, rng);
        }
        Ending::Run => {
            let hit_at = at + 1;
            let group = *groups.last().unwrap() as u32;
            let last_group = eighths - group;
            let toward = Chord::triad(if key.mode == Mode::Hijaz { 6 } else { 4 });
            score.sections.push(Section { name: "run", start: at * bar, end: hit_at * bar, trim, level: None, rings: false });
            score.harmony.push(toward);
            ring(score, hit_at);
            // The run starts on the dominant chord's tone nearest twice the
            // group's eighths over the tonic, so it fills the group in even
            // strokes, and steps down to the neighbour.
            let top = (2 * group as i32..).find(|d| toward.degrees().iter().any(|c| c.rem_euclid(7) == d.rem_euclid(7))).unwrap();
            let start = at * bar;
            let unit = group * E / top as u32;
            let runs = [(CH_LEAD, tune::home_tonic(&key, TUNE.0, TUNE.1)), (CH_DOUBLE, tune::home_tonic(&key, TUNE.0, TUNE.1)), (CH_TROMBONE, nearest_tonic(&key, 45, 56)), (CH_BASS, nearest_tonic(&key, 36, 47))];
            for (channel, home) in runs {
                let home = key.absolute_degree(home).unwrap();
                score.add(Note { start, len: last_group * E - E / 8, pitch: key.pitch(home + top, 4), vel: vel(0, rng), channel });
                for (k, d) in (1..=top).rev().enumerate() {
                    score.add(Note { start: start + last_group * E + k as u32 * unit, len: unit, pitch: key.pitch(home + d, 4), vel: vel(-4 + 2 * k as i32, rng), channel });
                }
            }
            let low = nearest_tonic(&key, 28, 45);
            let under = at_degree(&key, toward, toward.root, 28, 45, low);
            score.add(Note { start, len: last_group * E - E / 8, pitch: under, vel: vel(0, rng), channel: CH_TUBA });
            for p in toward.pitches_within(&key, 52, 71).into_iter().take(3) {
                score.add(Note { start, len: last_group * E - E / 8, pitch: p, vel: vel(-6, rng), channel: CH_CHOIR });
                score.add(Note { start, len: E, pitch: p, vel: vel(-4, rng), channel: CH_FIGURE });
            }
            score.add(Note { start, len: E - 20, pitch: KICK, vel: vel(4, rng), channel: CH_KIT });
            score.add(Note { start, len: E - 20, pitch: BASS_DRUM, vel: vel(-6, rng), channel: CH_KIT });
            variation::roll(score, CH_KIT, 38, start + last_group * E, group * 2, |x| vel(-20 + (24.0 * x) as i32, rng));
            strike(score, hit_at * bar, bar - E / 4, true, rng);
        }
    }
}

/// The band's last stroke on the tonic at `at`, every voice's tone held
/// `held`: the lead and its double on home, the tuba low, the bass, the
/// trombones and horns on the chord, the accordion and the tambura's
/// strum; the tapan's beater and deep head under it, and the crash where
/// `crash`.
fn strike(score: &mut Score, at: u32, held: u32, crash: bool, rng: &mut Rng) {
    let key = score.key;
    let tonic = Chord::triad(0);
    let home = tune::home_tonic(&key, TUNE.0, TUNE.1);
    for channel in [CH_LEAD, CH_DOUBLE] {
        score.add(Note { start: at, len: held, pitch: home, vel: vel(4, rng), channel });
    }
    score.add(Note { start: at, len: held, pitch: nearest_tonic(&key, 28, 45), vel: vel(4, rng), channel: CH_TUBA });
    score.add(Note { start: at, len: (2 * E).min(held), pitch: nearest_tonic(&key, 36, 55), vel: vel(4, rng), channel: CH_BASS });
    for p in tonic.pitches_within(&key, 45, 65).into_iter().take(3) {
        score.add(Note { start: at, len: held, pitch: p, vel: vel(0, rng), channel: CH_TROMBONE });
    }
    for p in tonic.pitches_within(&key, 53, 65).into_iter().take(2) {
        score.add(Note { start: at, len: held, pitch: p, vel: vel(-4, rng), channel: CH_HORN });
    }
    for p in tonic.pitches_within(&key, 52, 71).into_iter().take(3) {
        score.add(Note { start: at, len: held, pitch: p, vel: vel(-4, rng), channel: CH_CHOIR });
        score.add(Note { start: at, len: (2 * E).min(held), pitch: p, vel: vel(0, rng), channel: CH_FIGURE });
    }
    let drums: &[(u8, i32)] = if crash { &[(KICK, 8), (BASS_DRUM, 0), (CRASH, 0)] } else { &[(KICK, 8), (BASS_DRUM, 0)] };
    for (pitch, accent) in drums {
        score.add(Note { start: at, len: 2 * E, pitch: *pitch, vel: vel(*accent, rng), channel: CH_KIT });
    }
}

/// The tonic nearest the middle of `lo..=hi`, an octave wide or more.
fn nearest_tonic(key: &Key, lo: u8, hi: u8) -> u8 {
    let mid = (lo + hi) / 2;
    (lo..=hi).filter(|p| p % 12 == key.tonic).min_by_key(|p| (*p as i32 - mid as i32).abs()).unwrap()
}

/// The chord's tone at `degree` nearest `to` within `lo..=hi`.
fn at_degree(key: &Key, chord: Chord, degree: i32, lo: u8, hi: u8, to: u8) -> u8 {
    *chord.pitches_within(key, lo, hi).iter().filter(|p| key.degree_of(**p) == Some(degree.rem_euclid(7) as usize)).min_by_key(|p| ((**p as i32 - to as i32).abs(), **p)).unwrap()
}

/// The engine: the bass on the chord's root every eighth, the
/// group's first struck hard and the rest pressed, short enough that
/// each is a stroke; at full, the tambura an octave over them through
/// the chord each group — root, fifth, third — a sixteenth shorter, so
/// the two sound as one bow, and under the lead's register, where a
/// figure on its pitches masks it. A phrase's variant bar kicks the low
/// strings up to the fifth on the eighth into the dance's long group;
/// its cadence climbs both through the chord across the last group into
/// the next phrase.
fn ostinato(score: &mut Score, form: &Form, rng: &mut Rng) {
    let key = score.key;
    let groups = form.design.groove.groups.to_vec();
    let eighths = score.meter.eighths();
    let last_group = eighths - *groups.last().unwrap() as u32;
    let kick = form.design.groove.dum.get(1).map(|d| d - 1).filter(|e| !score.meter.strong(*e));
    for b in 0..form.bars() {
        let mode = form.texture_at(b).ostinato;
        if mode == Ostinato::Off {
            continue;
        }
        let chord = form.chord(b);
        let root = at_degree(&key, chord, chord.root, 38, 50, 43);
        let role = variation::role(b);
        // The cadence's last group climbs the chord into the next phrase;
        // the variant kicks up to the fifth into the dance's long group.
        let climb = chord.pitches_within(&key, root, 55);
        let fifth = chord.pitches_within(&key, 36, 55).into_iter().filter(|p| key.degree_of(*p) == Some((chord.root + 4).rem_euclid(7) as usize)).min_by_key(|p| (*p as i32 - root as i32 - 7).abs());
        let sweep = chord.pitches_within(&key, 45, 64);
        let mut at = 0;
        for g in &groups {
            for i in 0..*g as u32 {
                let e = at + i;
                let start = b * form.bar + e * E;
                let accent = if i == 0 { 6 } else { -8 };
                let pitch = match role {
                    Bar::Cadence if e >= last_group => climb[((e - last_group) as usize).min(climb.len() - 1)],
                    Bar::Variant if Some(e) == kick => fifth.unwrap_or(root),
                    _ => root,
                };
                score.add(Note { start, len: E * 3 / 4, pitch, vel: vel(accent, rng), channel: CH_BASS });
                if mode == Ostinato::Full {
                    let pitch = if role == Bar::Cadence && e >= last_group {
                        sweep[((e - last_group) as usize * 2).min(sweep.len() - 1)]
                    } else {
                        let degree = [chord.root, chord.root + 4, chord.root + 2][(i as usize) % 3];
                        at_degree(&key, chord, degree, 45, 60, 52)
                    };
                    score.add(Note { start, len: E / 2, pitch, vel: vel(accent - 6, rng), channel: CH_FIGURE });
                }
            }
            at += *g as u32;
        }
    }
}

/// The davul: its low stroke on the bar's first beat and on the dance's
/// long group, the kick and the floor tom together with the deep head under
/// them, the bar's the harder, and its thin stick on the other groups'
/// first beats, so every group is struck and the limp is heard; at
/// full, the stick's pickups into the groups, softer, the cymbal on
/// every phrase's first beat, and a fill down the toms through the last
/// group of every phrase, rising in strength, into the next. A phrase's
/// variant bar adds the stick's pickup into the long group and the
/// deep head under the stick's first stroke; its cadence rolls the deep head
/// into the next phrase.
fn drum(score: &mut Score, form: &Form, rng: &mut Rng) {
    for b in 0..form.bars() {
        let mode = form.texture_at(b).drum;
        if mode == Drum::Off {
            continue;
        }
        let groove = form.design.groove;
        for i in groove.dum {
            let start = b * form.bar + i * E;
            let accent = if *i == 0 { 2 } else { -3 };
            score.add(Note { start, len: E - 20, pitch: KICK, vel: vel(accent, rng), channel: CH_KIT });
            // Under the kick: three drums struck at once are the dance's
            // peak, and the kick is the one the ear counts.
            score.add(Note { start, len: E - 20, pitch: FLOOR_TOM, vel: vel(accent - 14, rng), channel: CH_KIT });
            score.add(Note { start, len: E - 20, pitch: BASS_DRUM, vel: vel(accent - 10, rng), channel: CH_KIT });
        }
        let role = variation::role(b);
        let last_eighth = groove.eighths() - 1;
        for i in groove.tek {
            let group = score.meter.strong(*i);
            if group || mode == Drum::Full {
                score.add(Note { start: b * form.bar + i * E, len: E / 2, pitch: SIDE_STICK, vel: vel(if group { -4 } else { -14 }, rng), channel: CH_KIT });
            }
        }
        // The variant's stick picks up into the long group; the deep head
        // answers the stick's first stroke there, and on the cadence rolls
        // two sixteenths into the next phrase.
        // The eighths neither hand plays, where a pickup can go.
        let free: Vec<u32> = (1..groove.eighths()).filter(|e| !groove.dum.contains(e) && !groove.tek.contains(e)).collect();
        if role == Bar::Variant {
            let into_long = groove.dum.get(1).map(|d| d - 1).filter(|e| free.contains(e));
            if let Some(e) = into_long.or(free.first().copied()) {
                score.add(Note { start: b * form.bar + e * E, len: E / 2, pitch: SIDE_STICK, vel: vel(-12, rng), channel: CH_KIT });
            }
            if let Some(i) = groove.tek.first() {
                score.add(Note { start: b * form.bar + i * E, len: E - 20, pitch: BASS_DRUM, vel: vel(-16, rng), channel: CH_KIT });
            }
        }
        if role == Bar::Cadence && b + 1 < form.bars() && !groove.dum.contains(&last_eighth) {
            variation::roll(score, CH_KIT, BASS_DRUM, b * form.bar + last_eighth * E, 2, |x| vel(-14 + (8.0 * x) as i32, rng));
            if mode != Drum::Full {
                score.add(Note { start: b * form.bar + last_eighth * E, len: E / 2, pitch: FLOOR_TOM, vel: vel(-10, rng), channel: CH_KIT });
            }
        }
        if mode != Drum::Full {
            continue;
        }
        if b % phrase::BARS == 0 {
            score.add(Note { start: b * form.bar, len: 2 * E, pitch: CRASH, vel: vel(-10, rng), channel: CH_KIT });
        }
        if role == Bar::Cadence || form.walk.parts.iter().any(|p| p.b == b + 1) {
            let groups = groove.groups;
            let last = *groups.last().unwrap() as u32;
            let from = groove.eighths() - last;
            for k in 0..last {
                let pitch = FILL[(k as usize * FILL.len() / last as usize).min(FILL.len() - 1)];
                score.add(Note { start: b * form.bar + (from + k) * E, len: E - 20, pitch, vel: vel(-10 + 5 * k as i32, rng), channel: CH_KIT });
            }
        }
    }
}

/// The low brass: trombone on the chord's root and fifth and tuba on
/// its root, short, on the dance's strokes, the bar's first the hardest;
/// at full, the trombones' fifth on the accompaniment's off-beats too,
/// softer, the push between the blows. A phrase's variant bar anticipates
/// its second stroke by an eighth; its cadence rips up the chord through
/// the last group, growing, into the next phrase.
fn brass(score: &mut Score, form: &Form, rng: &mut Rng) {
    let key = score.key;
    for b in 0..form.bars() {
        let mode = form.texture_at(b).brass;
        if mode == Brass::Off {
            continue;
        }
        let chord = form.chord(b);
        let root = at_degree(&key, chord, chord.root, 45, 57, 50);
        let fifth = at_degree(&key, chord, chord.root + 4, root + 1, 65, root + 7);
        let low = at_degree(&key, chord, chord.root, 28, 40, 34);
        let groove = form.design.groove;
        let role = variation::role(b);
        let last_group = groove.eighths() - *groove.groups.last().unwrap() as u32;
        let rips = role == Bar::Cadence && b + 1 < form.bars();
        for (k, i) in groove.dum.iter().enumerate() {
            if rips && *i >= last_group {
                continue;
            }
            // The variant anticipates the second stroke by an eighth.
            let i = if role == Bar::Variant && k == 1 { i - 1 } else { *i };
            let start = b * form.bar + i * E;
            let accent = if i == 0 { 8 } else { -2 };
            for p in [root, fifth] {
                score.add(Note { start, len: E * 2 / 3, pitch: p, vel: vel(accent, rng), channel: CH_TROMBONE });
            }
            score.add(Note { start, len: E * 2 / 3, pitch: low, vel: vel(accent - 4, rng), channel: CH_TUBA });
        }
        if rips {
            // The cadence rips up the chord through its last group.
            let up = chord.pitches_within(&key, root, 65);
            for (k, e) in (last_group..groove.eighths()).enumerate() {
                let pitch = up[(k * 2).min(up.len() - 1)];
                score.add(Note { start: b * form.bar + e * E, len: E / 2, pitch, vel: vel(-6 + 5 * k as i32, rng), channel: CH_TROMBONE });
            }
            score.add(Note { start: b * form.bar + last_group * E, len: E * 2 / 3, pitch: low, vel: vel(-4, rng), channel: CH_TUBA });
        }
        if mode != Brass::Full {
            continue;
        }
        for i in groove.chord.iter().filter(|i| !rips || **i < last_group) {
            score.add(Note { start: b * form.bar + i * E, len: E / 2, pitch: fifth, vel: vel(-12, rng), channel: CH_TROMBONE });
        }
    }
}

/// Held chords where the dance thickens: the accordion on root and fifth a
/// bar at a time, carried across the bar where the next chord keeps
/// them; at full, the clarinet held on three voices of the chord,
/// each led to the nearest tone of the next, and the horn on the third.
fn pad(score: &mut Score, form: &Form, rng: &mut Rng) {
    let key = score.key;
    let mut voices: Option<Vec<u8>> = None;
    for b in 0..form.bars() {
        let mode = form.texture_at(b).pad;
        if mode == Pad::Off {
            voices = None;
            continue;
        }
        let chord = form.chord(b);
        let root = at_degree(&key, chord, chord.root, 52, 64, 57);
        let fifth = at_degree(&key, chord, chord.root + 4, root + 1, 71, root + 7);
        for p in [root, fifth] {
            score.hold(Note { start: b * form.bar, len: form.bar + E / 8, pitch: p, vel: vel(-22, rng), channel: CH_CHOIR });
        }
        if mode != Pad::Full {
            voices = None;
            continue;
        }
        let third = at_degree(&key, chord, chord.root + 2, 53, 65, 59);
        score.hold(Note { start: b * form.bar, len: form.bar + E / 8, pitch: third, vel: vel(-18, rng), channel: CH_HORN });
        let mut candidates = chord.pitches_within(&key, 57, 72);
        let next: Vec<u8> = match &voices {
            Some(prev) => prev
                .iter()
                .map(|v| {
                    let (i, p) = candidates.iter().enumerate().min_by_key(|(_, c)| (**c as i32 - *v as i32).abs()).map(|(i, c)| (i, *c)).unwrap();
                    candidates.remove(i);
                    p
                })
                .collect(),
            None => candidates.iter().copied().take(3).collect(),
        };
        for p in &next {
            score.hold(Note { start: b * form.bar, len: form.bar + E / 8, pitch: *p, vel: vel(-24, rng), channel: CH_PAD });
        }
        voices = Some(next);
    }
}

/// The tune echoed under the dance on the guitar: each bar the
/// tune of the bar before, detached, an octave under the tune's
/// register, its strong beats bent to this bar's chord; silent through
/// the lead's turn, where nobody plays the tune.
fn weave(score: &mut Score, form: &Form, rng: &mut Rng) {
    let (lo, hi) = (TUNE.0 - 12, TUNE.1 - 12);
    let bars = form.bars();
    for b in 0..bars {
        let behind = (b + bars - 1) % bars;
        if form.texture_at(b).weave == Weave::Off || form.solo(b) || form.solo(behind) {
            continue;
        }
        let chord = form.chord(b);
        for (start, len, pitch) in form.tune.bar(score, behind, lo, hi, false) {
            let onset = start - behind * form.bar;
            let held = if len >= 3 * E { 2 * E } else { len - E / 2 };
            let pitch = if score.meter.strong(onset / E) && !chord.holds(&score.key, pitch) { tune::nearest_chord_tone(&score.key, chord, pitch, lo, hi) } else { pitch };
            score.add(Note { start: b * form.bar + onset, len: held, pitch, vel: vel(-10, rng), channel: CH_WEAVE });
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

    /// A play of the horo as it is, unbounded.
    fn as_it_is(seed: u64) -> Params {
        Params { setting: Setting::None, ..Params::of(crate::pieces::find("balkan-horo").unwrap(), seed) }
    }

    /// In a fight the dance holds one tempo from the quick third of its
    /// band, opens with no taksim and ends on no held tone, and its walk
    /// is the story's whole.
    #[test]
    fn a_fight_holds_the_dance_at_full_stride() {
        for seed in 0..48 {
            let (score, form) = compose(&Params { setting: Setting::Combat, ..as_it_is(seed) });
            assert_eq!(form.design.pace, Pace::Steady, "seed {seed}");
            assert_ne!(form.design.opening, Opening::Taksim, "seed {seed}");
            assert_ne!(form.design.ending, Ending::Held, "seed {seed}");
            let (lo, hi) = form.design.groove.tempo;
            assert!(score.eighth_bpm >= lo as f32 + (hi - lo) as f32 * 2.0 / 3.0 - 1.0, "seed {seed}: {} under the quick third", score.eighth_bpm);
            let rungs = STORIES.iter().find(|s| s.name == score.story).unwrap().ladder.len();
            assert!(form.walk.parts.iter().any(|p| p.rung == rungs), "seed {seed}: the story never reaches its top");
        }
    }

    /// The dance is whole question-and-answer pairs between its opening
    /// and its ending, its parts whole half-phrases, its harmony opening
    /// and closing on the tonic and its tune in its register; it never
    /// falls under its own tempo while it is danced; nothing struck ends
    /// past the end, only what is held.
    #[test]
    fn a_horo_is_whole_phrases_between_its_opening_and_its_ending() {
        for seed in 0..24 {
            let (score, form) = compose(&as_it_is(seed));
            let bars = form.bars();
            assert_eq!(bars % PAIR_BARS, 0, "seed {seed}: {bars} bars");
            let (from, to) = (form.design.opening.bars() * score.bar(), (form.design.opening.bars() + bars) * score.bar());
            assert!(score.tempo.iter().filter(|(t, _)| *t >= from && *t < to).all(|(_, bpm)| *bpm >= score.eighth_bpm), "seed {seed}: the dance slows");
            for s in score.sections.iter().filter(|s| s.start >= from && s.end <= to) {
                assert_eq!((s.end - s.start) % (phrase::BARS / 2 * score.bar()), 0, "seed {seed}: a part of broken half-phrases");
            }
            assert_eq!(score.harmony[0].root, 0);
            assert_eq!(score.harmony.last().unwrap().root, 0);
            for n in score.notes.iter().filter(|n| n.channel == CH_LEAD && n.len >= E / 2) {
                assert!((TUNE.0..=TUNE.1).contains(&n.pitch), "seed {seed}: the tune at {} leaves its register", n.pitch);
            }
            let end = score.end();
            for n in &score.notes {
                let role = score.instrument(n.channel).role;
                assert!(n.end() <= end || matches!(role, Role::Drone | Role::Sustain), "seed {seed}: {} past the end", score.instrument(n.channel).name);
            }
        }
    }

    /// Every kolyano comes twice running in the first round — but the
    /// last pair of a dance that returns, which is its first — and the
    /// lead's turn only after the round.
    #[test]
    fn the_kolena_come_twice_and_the_solo_after_them() {
        for (pairs, n, solo, returns) in [(6, 3, false, true), (10, 3, true, false), (12, 4, true, true), (4, 4, true, false), (16, 3, true, true)] {
            let seq = order(pairs, n, solo, returns);
            assert_eq!(seq.len(), pairs);
            let first = seq.iter().position(|p| *p == Pair::Solo).unwrap_or(pairs);
            let kolena = n.min(pairs / 2);
            let round = &seq[..pairs - returns as usize];
            for k in 0..kolena.min(first / 2) {
                assert!(round[2 * k..].iter().take(2).all(|p| *p == Pair::Kolyano(k)), "{seq:?}");
            }
            assert!(first >= 2 * kolena, "{seq:?}: the solo before the first round ends");
            assert!(seq.last() != Some(&Pair::Solo), "{seq:?}: ends on the solo");
            if returns {
                assert_eq!(seq.last(), Some(&Pair::Kolyano(0)), "{seq:?}");
            }
        }
    }

    /// Every opening, pace and ending comes up, and so does the lead's
    /// turn, the dance's harmony a vamp under it.
    #[test]
    fn the_dances_vary_in_how_they_open_move_and_end() {
        let (mut openings, mut paces, mut endings, mut solos) = (Vec::new(), Vec::new(), Vec::new(), 0);
        for seed in 0..120 {
            let (score, form) = compose(&as_it_is(seed));
            openings.push(form.design.opening);
            paces.push(form.design.pace);
            endings.push(form.design.ending);
            if let Some(b) = (0..form.bars()).find(|b| form.solo(*b)) {
                solos += 1;
                let vamp = form.design.vamp.unwrap();
                let at = (b + form.design.opening.bars()) as usize;
                assert_eq!(score.harmony[at].root, vamp.roots[0], "seed {seed}");
            }
        }
        for o in [Opening::Straight, Opening::Taksim, Opening::Tapan] {
            assert!(openings.contains(&o), "{o:?} never drawn");
        }
        assert!(paces.contains(&Pace::Steady), "never steady");
        assert!(paces.iter().any(|p| *p != Pace::Steady), "never pressing on");
        for e in [Ending::Stop, Ending::Tag, Ending::Held, Ending::Run] {
            assert!(endings.contains(&e), "{e:?} never drawn");
        }
        assert!(solos > 0, "no dance has a solo");
    }
}
