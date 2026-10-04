//! The city's music: a place told in a loop, on a blues. Every choice
//! is a draw from the seed's stream, one fork per purpose, and the
//! first fork decides what kind of piece this is — which story it
//! tells, in what mode and tonic, on which feel, with which players,
//! on what theme, in which phrase form, on which rows of the twelve-bar
//! — so two seeds are two nights in one city. The seed picks which
//! shape at every level; it never picks the next note.
//!
//! A story is a ladder of the bed's layers, walked as `ladder` says,
//! the band's loudness rising a little as it fills, declared and met by
//! the render. The bed is a band: the bass in two or walking, the
//! piano's comp and the guitar's, the kit on the brushes, the organ as
//! the wash, and the seed's colours. A blues holds its form and varies
//! what fills its slots: every chorus is the twelve-bar, and its rows —
//! the quick change or not, the turn home through the V or the flat
//! sixth — are drawn afresh each chorus but the first and the last, so
//! the head comes back as it was. One theme runs the piece, and one
//! player tells it, the lead, in the blues' AAB: a row's line, the same
//! line again over the IV, and an answer; the lead calls through a
//! row's first two bars and lands, and leaves its last two to the band,
//! where the weave answers with the call. No player of the band plays
//! one bar over and over: the comp draws its rhythm a bar at a time,
//! never the same three bars running; the bass walks or skips; the kit
//! comps on the snare and fills where a blues fills.

use crate::ladder::{self, turn, Bed, Story, Walk};
use crate::pieces::Params;
use crate::rng::Rng;
use crate::score::{Instrument, Note, Role, Score, TICKS_PER_EIGHTH as E};
use crate::teller::{self, Hold, Teller, Telling};
use crate::theory::groove::{Groove, BLUES};
use crate::theory::melody::Theme;
use crate::theory::phrase::{Form as PhraseForm, FORMS};
use crate::theory::schema::{Schema, TWELVE_BAR};
use crate::theory::{interval_class, Chord, Key, Mode};
use crate::theory::melody::Tone;
use crate::theory::phrase;
use crate::tune::{self, Tune};
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
/// shape it. The pool's files each have their own.
const LEADS: [u8; 4] = [HARMONICA, MUTED_TRUMPET, TENOR_SAX, ALTO_SAX];
/// Who may hold tones under the lead's riff: a horn that is neither
/// the lead nor the colours' horn.
const SECONDS: [u8; 3] = [TENOR_SAX, ALTO_SAX, MUTED_TRUMPET];

/// Each lead's level, dB, so that wherever it plays it sits 1.5 dB
/// under the band: the bank's samples of them are not one
/// loudness. Measured against the band on the pool's seeds.
fn lead_level(program: u8) -> f32 {
    match program {
        HARMONICA => 0.4,
        MUTED_TRUMPET => 5.3,
        TENOR_SAX => 5.2,
        ALTO_SAX => 1.2,
        _ => 0.0,
    }
}

/// The velocity every voice strikes at before its own accent: one
/// dynamic for the whole band, since its story is in what plays.
const VEL: i32 = 85;

/// Where the band's foot sits under its top rung, LU: the night grows a
/// little as the band fills, by a level the render meets on whatever
/// bank plays it, and no more — the city is a bed the game plays over.
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
use Telling::{Long, Phrases, Riff, RiffAndLong, Trading};

const STORIES: [Story<Texture, Telling>; 5] = [
    Story {
        name: "stroll",
        weight: 3.0,
        base: Texture { bass: Bass::Two, kit: Kit::Time, ..Texture::BARE },
        ladder: &[CompLayer, BassLayer, OrganLayer, KitLayer, WeaveLayer, Colours, CompLayer],
        leads: &[Phrases, Phrases, Trading, Trading, Long, RiffAndLong, RiffAndLong, Riff],
        turns: &[turn((7, 7), (1, 1))],
        halves: (1, 1),
        pace: (0.2, 0.8),
    },
    Story {
        name: "late night",
        weight: 2.0,
        base: Texture { organ: Organ::Thin, ..Texture::BARE },
        ladder: &[BassLayer, CompLayer, WeaveLayer, KitLayer, OrganLayer, BassLayer, Colours],
        leads: &[Long, Phrases, Phrases, Phrases, Trading, Trading, Long, Riff],
        turns: &[turn((4, 5), (0, 0)), turn((7, 7), (0, 1))],
        halves: (1, 1),
        pace: (0.0, 0.6),
    },
    Story {
        name: "rush hour",
        weight: 2.0,
        base: Texture { bass: Bass::Walking, comp: Comp::Shells, kit: Kit::Time, ..Texture::BARE },
        ladder: &[KitLayer, OrganLayer, WeaveLayer, CompLayer, Colours, WeaveLayer],
        leads: &[Riff, Trading, Trading, RiffAndLong, RiffAndLong, Riff, Trading],
        turns: &[turn((3, 3), (0, 0)), turn((1, 1), (0, 0)), turn((6, 6), (1, 1))],
        halves: (1, 1),
        pace: (0.6, 1.0),
    },
    Story {
        name: "corner",
        weight: 2.0,
        base: Texture { comp: Comp::Shells, ..Texture::BARE },
        ladder: &[BassLayer, KitLayer, WeaveLayer, OrganLayer, BassLayer, Colours],
        leads: &[Trading, Trading, Phrases, Trading, Riff, Trading, Long],
        turns: &[turn((2, 2), (0, 0)), turn((0, 0), (0, 0)), turn((3, 4), (0, 0)), turn((1, 1), (0, 0)), turn((6, 6), (0, 1))],
        halves: (1, 1),
        pace: (0.2, 1.0),
    },
    Story {
        name: "after hours",
        weight: 1.5,
        base: Texture { organ: Organ::Thin, bass: Bass::Two, ..Texture::BARE },
        ladder: &[CompLayer, WeaveLayer, OrganLayer, KitLayer, BassLayer, Colours],
        leads: &[Long, Long, Phrases, Phrases, Trading, Long, Phrases],
        turns: &[turn((6, 6), (1, 2))],
        halves: (1, 2),
        pace: (0.0, 0.3),
    },
];

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
    // A minor blues: Dorian's major IV brightens it, Aeolian's does not.
    let mode = [Mode::Dorian, Mode::Aeolian][skeleton.weighted(&[3.0, 2.0])];
    let key = Key::new(["C", "G", "D", "F", "E"][skeleton.weighted(&[3.0, 2.0, 2.0, 2.0, 1.0])], mode);
    let groove = &BLUES[skeleton.weighted(&[3.0, 2.0])];
    let tempo = story.tempo(groove.tempo, &mut skeleton);
    // At least one colour, since a ladder may bring the colours in.
    let colours = skeleton.range(1, 3);
    let teller = LEADS[skeleton.below(LEADS.len())];
    // The horn that is not the lead.
    let other_horn = if teller == MUTED_TRUMPET { TENOR_SAX } else { MUTED_TRUMPET };
    let design = Design {
        piano: [E_PIANO, PIANO][skeleton.below(2)],
        lead: teller,
        second: *skeleton.pick(&SECONDS.iter().copied().filter(|p| *p != teller && *p != other_horn).collect::<Vec<u8>>()),
        horn: other_horn,
        horns: colours & 1 != 0,
        shimmer: colours & 2 != 0,
        groove,
        form: FORMS[skeleton.below(FORMS.len())],
        theme: Theme::draw(groove, &mut skeleton),
        rows: [row(0, mode, &mut skeleton), row(1, mode, &mut skeleton), row(2, mode, &mut skeleton)],
    };
    let instruments = vec![
        Instrument { name: "bass", program: UPRIGHT_BASS, channel: CH_BASS, role: Role::Pluck, low: 28, high: 60, reverb: 25, pan: 0, level: 0.0 },
        Instrument { name: "piano", program: design.piano, channel: CH_PIANO, role: Role::Pluck, low: 48, high: 79, reverb: 45, pan: -21, level: 0.0 },
        Instrument { name: "lead", program: design.lead, channel: CH_LEAD, role: Role::Melody, low: 53, high: 84, reverb: 45, pan: 0, level: lead_level(design.lead) },
        Instrument { name: "guitar", program: JAZZ_GUITAR, channel: CH_GUITAR, role: Role::Pluck, low: 40, high: 72, reverb: 35, pan: 36, level: 0.0 },
        Instrument { name: "second", program: design.second, channel: CH_SECOND, role: Role::Melody, low: 50, high: 84, reverb: 55, pan: 18, level: 0.0 },
        Instrument { name: "organ", program: ORGAN, channel: CH_ORGAN, role: Role::Sustain, low: 52, high: 69, reverb: 80, pan: -34, level: 0.0 },
        Instrument { name: "horn", program: design.horn, channel: CH_HORN, role: Role::Sustain, low: 50, high: 72, reverb: 75, pan: 29, level: 0.0 },
        Instrument { name: "weave", program: E_PIANO, channel: CH_WEAVE, role: Role::Pluck, low: 40, high: 84, reverb: 70, pan: -47, level: 0.0 },
        Instrument { name: "shimmer", program: VIBES, channel: CH_SHIMMER, role: Role::Pluck, low: 72, high: 91, reverb: 75, pan: 47, level: 0.0 },
        Instrument { name: "kit", program: BRUSH_KIT, channel: CH_KIT, role: Role::Percussion, low: KICK, high: RIDE, reverb: 35, pan: 0, level: 0.0 },
        Instrument { name: "weave, the vibes", program: VIBES, channel: CH_WEAVE_2, role: Role::Pluck, low: 53, high: 89, reverb: 70, pan: 44, level: 0.0 },
    ];
    let mut score = Score::new(key, groove.meter(), tempo, instruments, ROOM_S);
    score.loops = true;
    score.lead = Some(CH_LEAD);
    let name = |program: u8| match program {
        PIANO => "piano",
        E_PIANO => "electric piano",
        VIBES => "vibes",
        HARMONICA => "harmonica",
        ALTO_SAX => "alto sax",
        JAZZ_GUITAR => "jazz guitar",
        MUTED_TRUMPET => "muted trumpet",
        TENOR_SAX => "tenor sax",
        _ => "?",
    };
    let colours: Vec<&str> = [(design.horns, "horn"), (design.shimmer, "shimmer")].into_iter().filter(|(on, _)| *on).map(|(_, n)| n).collect();
    score.summary = format!(
        "{} on the {}: a {:?} in a {:?}, {} / {} / {}; {} comp, {} lead, {} second, {} horn, colours {}",
        story.name,
        groove.name,
        design.theme.shape,
        design.form,
        design.rows[0].name,
        design.rows[1].name,
        design.rows[2].name,
        name(design.piano),
        name(design.lead),
        name(design.second),
        name(design.horn),
        colours.join(", ")
    );
    let bar = score.bar();

    // The walk, in half-phrases, and whole choruses of them, so the
    // loop closes on the turn home.
    let walk = story.place(&mut skeleton, &mut score, 2 * TWELVE_BAR.len() as u32, |_, _| 1.0);
    let upper = story.ladder.len().max(1);
    for (section, part) in score.sections.iter_mut().zip(&walk.parts) {
        section.level = Some(LEVEL_FOOT * (1.0 - part.rung as f32 / upper as f32));
    }

    // The rows of every chorus: the piece's own at the head and at the
    // end, so the return is heard, and drawn afresh between.
    let choruses = walk.bars() / (TWELVE_BAR.len() as u32 * phrase::BARS);
    let mut changes = rng.fork(11);
    let rows: Vec<&Schema> = (0..choruses)
        .flat_map(|c| {
            let own = c == 0 || c + 1 == choruses;
            (0..TWELVE_BAR.len()).map(|r| if own { design.rows[r] } else { row(r, mode, &mut changes) }).collect::<Vec<_>>()
        })
        .collect();
    // The tune a third up on the ladder's top, else as written: a blues
    // keeps its tune and moves the changes under it.
    let shifts: Vec<i32> = (0..walk.bars()).map(|b| if walk.at(b).rung >= upper { CLIMB } else { 0 }).collect();
    let mut tune = Tune::compose(&design.theme, &score.meter, design.form, &rows, 4, shifts);
    aab(&mut tune, score.meter.eighths());
    score.harmony = tune.chords.clone();
    let form = Form { bar, walk, tune, design };

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
    let teller = Teller {
        lead: CH_LEAD,
        second: CH_SECOND,
        double: None,
        echo: CH_WEAVE,
        register: TUNE,
        grace: scoop,
        sung: (-12, -24),
        riff: (-8, -14, -14),
        // The lead holds at its tune's strength: a bank's horns fall
        // away under a soft stroke by more than its harmonica does, and
        // a lead's level holds only for what it plays at one strength.
        long: (60, 72, -12),
        under: (55, 67, -18),
        hold: Hold::Ringing,
        breathes: false,
        vel,
    };
    teller::tell(&mut score, &teller, &form.tune, &form.walk.runs(), &mut rng.fork(8));
    kit(&mut score, &form, &mut rng.fork(10));
    score.finish();
    score
}

/// One of row `r`'s schemata the mode can take.
fn row(r: usize, mode: Mode, rng: &mut Rng) -> &'static Schema {
    let fits: Vec<&'static Schema> = TWELVE_BAR[r].iter().filter(|s| s.modes.contains(&mode)).collect();
    fits[rng.below(fits.len())]
}

/// The tune as a blues sings it, every chorus AAB: the second row's line
/// is the first's again — bent to the IV where its strong beats meet it
/// — and the third answers; in every row the lead calls through the
/// first two bars, holds the row's last tone through the third, and
/// leaves the fourth to the band, the hole the weave answers in.
fn aab(tune: &mut Tune, eighths: u32) {
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
            tune.bars[row + bars - 1].clear();
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

/// The cells of the four, and of the shuffle's twelve-eight, with how
/// often each is drawn.
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

/// A cell for each bar of the piece, drawn, never the same three bars
/// running.
fn cells(meter: &crate::theory::Meter, bars: u32, rng: &mut Rng) -> Vec<&'static Cell> {
    let pool: &'static [(Cell, f32)] = if meter.groups[0] == 3 { &CELLS_SHUFFLE } else { &CELLS_FOUR };
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

/// The chord's pitches in `lo..=hi` at `degree` of the mode.
fn at_degree(key: &Key, chord: Chord, degree: i32, lo: u8, hi: u8) -> Vec<u8> {
    chord.pitches_within(&key, lo, hi).into_iter().filter(|p| key.degree_of(*p) == Some(degree.rem_euclid(7) as usize)).collect()
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
/// is walked into and not jumped at, and the loop's head has a pickup
/// into it like every other bar. Well under the band's level: the
/// upright's low end weighs on the loudness far past what the ear
/// gives it, and the pedal would cut the band around it.
fn bass(score: &mut Score, form: &Form, rng: &mut Rng) {
    let key = score.key;
    // From A1 to A3: where an upright walks, its root in two on C3 and
    // its fifth under it, clear of the boom below.
    let (lo, hi) = (33, 57);
    let strong = score.meter.strong_eighths();
    let eighths = score.meter.eighths();
    let groups = score.meter.groups.clone();
    let mut last: Option<u8> = None;
    let mut rising = true;
    for b in 0..form.bars() {
        let mode = form.texture_at(b).bass;
        if mode == Bass::Off {
            last = None;
            continue;
        }
        let chord = form.chord(b);
        let roots = at_degree(&key, chord, chord.root, lo, hi);
        let root = nearest(&roots, last.unwrap_or(46));
        let beat = |g: usize| b * form.bar + strong[g] * E;
        let mut p = root;
        match mode {
            Bass::Two => {
                let fifth = if root >= lo + 5 { root - 5 } else { root + 7 };
                let half = (groups[0] + groups[1]) as u32 * E;
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
                    let passing = key.pitch(key.absolute_degree(three).unwrap() + if three > one { -1 } else { 1 }, 4);
                    score.add(Note { start: beat(2) - E, len: E - 40, pitch: passing.clamp(lo, hi), vel: vel(-28, rng), channel: CH_BASS });
                }
                score.add(Note { start: beat(2), len: half - E - 40, pitch: three, vel: vel(-24, rng), channel: CH_BASS });
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
                let skip = (role == Bar::Variant && line.len() > 2).then(|| {
                    let to = line[2];
                    key.pitch(key.absolute_degree(to).unwrap() + if line[1] > to { 1 } else { -1 }, 4).clamp(lo, hi)
                });
                for (g, pitch) in line.iter().enumerate() {
                    let last_beat = g + 1 == strong.len();
                    let short = last_beat || (g == 1 && skip.is_some());
                    let len = groups[g] as u32 * E - if short { E } else { 0 } - 40;
                    score.add(Note { start: beat(g), len, pitch: *pitch, vel: vel(if g == 0 { -16 } else { -24 }, rng), channel: CH_BASS });
                }
                if let Some(pitch) = skip {
                    score.add(Note { start: beat(2) - E, len: E - 40, pitch, vel: vel(-30, rng), channel: CH_BASS });
                }
                rising = !rising;
            }
            Bass::Off => unreachable!(),
        }
        let next = form.chord((b + 1) % form.bars());
        let next_root = nearest(&at_degree(&key, next, next.root, lo, hi), if mode == Bass::Two { root } else { p });
        let degree = key.absolute_degree(next_root).unwrap();
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
        if mode == Comp::Off {
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
        if form.texture_at(b).comp != Comp::Full {
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
    let (lo, hi) = (52, 64);
    let mut voices: Option<Vec<u8>> = None;
    let mut alone: Option<u8> = None;
    let mut rising = true;
    for b in 0..form.bars() {
        let mode = form.texture_at(b).organ;
        if mode == Organ::Off {
            voices = None;
            alone = None;
            rising = true;
            continue;
        }
        let chord = form.chord(b);
        let major_seventh = (key.pitch(chord.root + 6, 4) as i32 - key.pitch(chord.root, 4) as i32).rem_euclid(12) == 11;
        let held: [i32; 3] = if major_seventh { [chord.root, chord.root + 2, chord.root + 4] } else { [chord.root, chord.root + 4, chord.root + 6] };
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
    let (lo, hi) = (52, 67);
    let mut last: Option<u8> = None;
    for b in 0..form.bars() {
        if !form.texture_at(b).colours {
            last = None;
            continue;
        }
        let chord = form.chord(b);
        let third = nearest(&at_degree(&key, chord, chord.root + 2, lo, hi), last.unwrap_or(60));
        let seventh = key.pitch(chord.root + 6, 4);
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
        if !form.texture_at(b).colours {
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

/// The grace a blues takes into a strong beat: from a semitone under
/// the key's fifth — the blue note, bent up — and from the step under
/// any other tone, a scoop; None above the register.
fn scoop(key: &Key, pitch: u8, hi: u8) -> Option<u8> {
    let grace = if (pitch as i32 - key.tonic as i32).rem_euclid(12) == 7 { pitch - 1 } else { key.pitch(key.absolute_degree(pitch).unwrap() - 1, 4) };
    (grace <= hi).then_some(grace)
}

/// The weave answers the lead in a row's hole, soft, where the band asks
/// for it, and never while the lead calls: the electric piano plays the
/// call — the row's first two bars — again in the hole, an octave under
/// the tune's register, its strong beats bent to the hole's chords; at the
/// second notch the vibes take the call's skeleton, its strong-beat
/// tones, a foot late and a third up, bent to the chord.
fn weave(score: &mut Score, form: &Form, rng: &mut Rng) {
    let (lo, hi) = (TUNE.0 - 12, TUNE.1 - 12);
    let late = form.design.theme.feet[0][0].1;
    let eighths = score.meter.eighths();
    for b in (0..form.bars()).filter(|b| hole(*b)) {
        let mode = form.texture_at(b).weave;
        if mode == Weave::Off {
            continue;
        }
        let call = b - 2;
        let chord = form.chord(b);
        for (start, len, pitch) in form.tune.bar(score, call, lo, hi, false) {
            let onset = start - call * form.bar;
            let held = if len >= 3 * E { 2 * E } else { len - E / 2 };
            let pitch = if score.meter.strong(onset / E) && !chord.holds(&score.key, pitch) { tune::nearest_chord_tone(&score.key, chord, pitch, lo, hi) } else { pitch };
            score.add(Note { start: b * form.bar + onset, len: held, pitch, vel: vel(-14, rng), channel: CH_WEAVE });
        }
        if mode != Weave::Two {
            continue;
        }
        for (start, len, pitch) in form.tune.bar(score, call, TUNE.0, TUNE.1, false) {
            let onset = (start - call * form.bar) / E;
            if !score.meter.strong(onset) || onset + late >= eighths {
                continue;
            }
            let mut third = score.key.pitch(score.key.absolute_degree(pitch).unwrap() + 2, 4).min(89);
            if !chord.holds(&score.key, third) {
                third = tune::nearest_chord_tone(&score.key, chord, third, TUNE.0, 89);
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
/// crash. The cymbals strike at the top of the velocity and the kick
/// well under the band's, since the bank keeps its cymbals some twenty
/// decibels under its kick, and a stroke's length changes nothing.
fn kit(score: &mut Score, form: &Form, rng: &mut Rng) {
    let strong = score.meter.strong_eighths();
    let eighths = score.meter.eighths();
    let groove = form.design.groove;
    let chorus = TWELVE_BAR.len() as u32 * phrase::BARS;
    let last_beat = *strong.last().unwrap();
    for b in 0..form.bars() {
        let mode = form.texture_at(b).kit;
        if mode == Kit::Off {
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
                stroke(*e, HAT_PEDAL, 30, rng);
            } else {
                stroke(*e, RIDE, if strong.contains(e) { 42 } else { 24 }, rng);
            }
        }
        for e in groove.tek.iter().filter(|e| **e < until) {
            stroke(*e, HAT_PEDAL, 38, rng);
        }
        if mode == Kit::Full {
            for e in groove.dum.iter().filter(|e| **e < until) {
                stroke(*e, KICK, -14, rng);
            }
            for e in groove.tek.iter().filter(|e| **e < until) {
                stroke(*e, SNARE, 24, rng);
            }
            let open: Vec<u32> = (0..until).filter(|e| !strong.contains(e) && !groove.tek.contains(e)).collect();
            for _ in 0..rng.range(1, 2) {
                if !open.is_empty() {
                    let e = open[rng.below(open.len())];
                    stroke(e, SNARE, 4, rng);
                }
            }
            if in_chorus == 0 && (b / chorus) % 2 == 1 {
                stroke(0, CRASH, 44, rng);
            }
        }
        if fills {
            let n = (eighths - last_beat) * 2;
            for k in 0..n {
                let pitch = if mode == Kit::Full { TOMS[(k as usize * TOMS.len() / n as usize).min(TOMS.len() - 1)] } else { SNARE };
                score.add(Note { start: b * form.bar + last_beat * E + k * E / 2, len: E / 2 - 10, pitch, vel: vel(4 + (16 * k / n) as i32, rng), channel: CH_KIT });
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

    /// The piece is a loop of whole choruses, its parts whole
    /// half-phrases, its harmony the twelve-bar's, its tune in its
    /// register; nothing struck ends past the end, only what is held,
    /// which the loop continues.
    #[test]
    fn a_piece_is_whole_choruses() {
        for seed in 0..24 {
            let score = build(&Params { seed });
            assert!(score.loops);
            let bars = score.end() / score.bar();
            assert_eq!(bars % (3 * phrase::BARS), 0, "seed {seed}: {bars} bars");
            for s in &score.sections {
                assert_eq!((s.end - s.start) % (phrase::BARS / 2 * score.bar()), 0, "seed {seed}: a part of broken half-phrases");
            }
            for (i, chord) in score.harmony.iter().enumerate() {
                assert_eq!(chord.size, 4);
                assert!(matches!(chord.root, 0 | 3 | 4) || (chord.root == 5 && score.key.mode == Mode::Aeolian), "seed {seed}: bar {i} on {}", chord.root);
            }
            assert_eq!(score.harmony[0].root, 0);
            assert!(matches!(score.harmony[8].root, 4 | 5));
            for n in score.notes.iter().filter(|n| matches!(n.channel, CH_LEAD | CH_SECOND) && n.len >= E / 2) {
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
