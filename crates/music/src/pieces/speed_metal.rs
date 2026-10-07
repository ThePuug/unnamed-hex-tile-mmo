//! Speed metal as Helloween played it on Walls of Jericho and the two
//! Keepers, 1985–88. The rules below are counted from their songs'
//! transcriptions and the metal surveyed beside them, in
//! `proofs/research/`: `helloween-findings.md` (keys, progressions,
//! form, drums, twin leads), `variety-findings.md` (what sets one song
//! of an album off from the next, and how songs end),
//! `lead-findings.md` (the line a singer carries), `guitars-findings.md`
//! (two rhythm guitarists) and `solos-findings.md` (the solos).
//!
//! **A song is its own.** Key and tempo barely separate the songs of one
//! album; what does is drawn first: the groove's family — the skank with
//! the kick doubled or single, the gallop, the backbeat, the half-speed
//! feel — the intro, the chorus's mood, the form's lengths, how far and
//! how the two guitars split, which soloist goes first, how it ends, and
//! one signature event no two songs share — the guitars dropping out
//! over bass and drums before the last chorus, a dead bar, the second
//! verse in another feel, a half-time chorus, the drums alone into the
//! solos, the last chorus up a step.
//!
//! **The song** is verse, most often a pre-chorus, and chorus twice, a
//! solo block, and the way back to a last chorus, doubled three times in
//! four; every section whole phrases. A verse is Aeolian; a pre-chorus
//! lifts; a chorus sits in the minor or on the relative major, as
//! Weikath's do. Each part of the song lifts the kit its own way: the
//! pre-chorus doubles the kick, the chorus moves to the ride, the solos
//! do both.
//!
//! **The lead sings.** One theme the song through: in the verse every
//! foot of it, legato, five tones a bar and half of them off the beat; in
//! the pre-chorus held tones climbing to the chorus's first; in the
//! chorus a third higher, held bars and moving bars by turns, the twin a
//! diatonic third over it, plain. It restates a half-phrase an octave up
//! now and then, pushes a tone ahead of the beat when it sings a phrase
//! again, fills a few of its gaps from the second verse on, and climbs at
//! the end of the last chorus to the song's highest tone, held over the
//! ending. Bends, slides and vibrato are its player's (`perform`).
//!
//! **Two rhythm guitarists.** Most bars they play one part, two takes on
//! two amps, one hard each side; how much they split is the song's, from
//! never to most bars, and the pre-chorus splits most — the riff
//! harmonised, one holding chords while the other chugs, a pedal over the
//! riff, another voicing or an octave up — and under the solos they play
//! one part again. Their riff speaks in phrases: the second bar answers,
//! the third is the first again, the fourth turns round and the eighth
//! harder; a later verse answers its own way.
//!
//! **The solos** are two players, a shredder and a singer (`solo`), in
//! one of the surveyed layouts, each turn answering the last by
//! contrast; the twin break is the theme in thirds.
//!
//! **It ends** as Helloween end: the ritual — the chord held under a
//! cymbal's wash and the toms, a separate last hit — a figure of the
//! band's hits and a stop, the riff into one stab, a false ending, the
//! lead alone before the stab, or, seldom, a chord left to ring; on the
//! chorus's home chord, so a bright chorus ends on the relative major.
//!
//! **The production** is a record's: the rhythm guitars hard left and
//! right, the bass, kick and snare in the middle, the kit's drums across
//! the stage, the lead left of the middle and the twin right.
//!
//! Every choice is a draw from the seed's stream, one fork per purpose;
//! the seed picks which shape at every level, never the next note.

use crate::ladder::{self, leap, turn, Bed, Run, Story, Turn, Walk};
use crate::band::{Part, Style};
use crate::pieces::Params;
use crate::rng::Rng;
use crate::rock::{self, Band, Figure, Ritual, Fill};
use crate::score::{Instrument, Note, Role, Score, Section as ScoreSection, TICKS_PER_EIGHTH as E};
use crate::solo::{self, Part as SoloPart, Shape as SoloShape, SHREDDER, SINGER};
use crate::teller::{self, Hold, Teller, Telling};
use crate::theory::groove::SPEED;
use crate::theory::melody::{run_between, Theme, SPEED_SHAPES};
use crate::theory::phrase::{self, Form as PhraseForm, FORMS};
use crate::theory::schema::{split, Schema, SPEED as SONG};
use crate::theory::{Chord, Key, Mode};
use crate::tune::{self, Placed, Tune};
use crate::variation::{self, Role as Bar};

/// A sixteenth, in ticks.
const S: u32 = E / 2;

/// General MIDI programs, 0-based.
const OVERDRIVEN: u8 = 29;
const DISTORTION: u8 = 30;
const BASS: u8 = 33;
const STRINGS: u8 = 48;
/// The power kit, the bank's rock kit, and its keys.
const ROCK_KIT: u8 = 16;
const KICK: u8 = 36;
const SNARE: u8 = 38;
const HAT: u8 = 42;
const OPEN_HAT: u8 = 46;
const CRASH: u8 = 49;
const RIDE: u8 = 51;
const RIDE_BELL: u8 = 53;
const CRASH_2: u8 = 57;
/// The toms, high to low, the floor's two last.
const TOMS: [u8; 6] = [50, 48, 47, 45, 43, 41];

const CH_BASS: u8 = 0;
const CH_LEFT: u8 = 1;
const CH_RIGHT: u8 = 2;
const CH_LEAD: u8 = 3;
const CH_TWIN: u8 = 4;
const CH_STRINGS: u8 = 5;
const CH_KIT: u8 = 9;

/// How long the room rings: a studio's live room, tight under the
/// ballad's hall.
const ROOM_S: f32 = 1.6;

/// The lead's level, dB, forward of the band and of the guitars' wall: a
/// guitar lead among the double-tracked guitars, one of them its own
/// program, is lost at less.
const LEAD: f32 = 5.0;
/// The twin's, a decibel under the lead it harmonises.
const TWIN: f32 = LEAD - 1.0;
/// The band's level, dB.
const BAND: f32 = -2.0;
/// The kit's, its most struck drum under the band's.
const KIT: f32 = BAND - 1.0;
/// The rhythm guitars', each of the pair, over the band: a record's wall,
/// its widest and loudest part. A palm-muted chug is a short stroke with
/// no ring through the amp, some seven dB under the held tone a level is
/// measured on; under it the mix is the middle's — the lead, the bass,
/// the kick and snare — and narrow.
const GUITARS: f32 = BAND + 7.0;
/// The strings', a wash under the chorus.
const STRINGS_LEVEL: f32 = BAND - 3.0;

/// The velocity every voice strikes at before its own accent.
const VEL: i32 = 88;

/// Where each part of the song sits, LU against the solo, its loudest. A
/// speed metal record is loud from its first bar to its last; the chorus
/// and the solo stand a little over the verse, the bare openings under it.
const LEVEL_VERSE: f32 = -2.5;
const LEVEL_PRE: f32 = -2.0;
const LEVEL_CHORUS: f32 = -1.0;
const LEVEL_SOLO: f32 = 0.0;
const LEVEL_BARE: f32 = -6.0;
/// The ending's level over its hit and ring: a ring falling away puts
/// the hit some four LU over its mean.
const LEVEL_END: f32 = LEVEL_CHORUS - 4.0;

/// The tune's register; the lead takes it here, and the twin a third
/// over it, under a 24-fret neck's top.
const TUNE: (u8, u8) = (57, 81);
/// The solos' register, up to the top E.
const SOLO: (u8, u8) = (64, 88);
/// The sequence a verse's phrase pair takes, in degrees.
const PAIRS: [i32; 2] = [0, 1];
/// The chorus a third up: on the minor's third, the relative major's
/// tonic, where Helloween's minor choruses hold.
const CLIMB: i32 = 2;

/// The rhythm guitars' register: the power chord's root from the low E.
const ROOT: (u8, u8, u8) = (40, 52, 45);

/// The bars the room rings after an ending's last stroke.
const RING_BARS: u32 = 2;

/// The ladder's rungs: the verse at its foot, the pre-chorus, the chorus,
/// the solos.
const VERSE: i32 = 0;
const PRE: i32 = 1;
const CHORUS: i32 = 2;
const SOLOS: i32 = 3;

/// What the band is at a rung.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Texture {
    /// The kit lifted for the chorus: the pre-chorus.
    lift: bool,
    /// The guitars opened up: the chorus.
    open: bool,
    solo: bool,
}

impl Texture {
    fn section(&self) -> Section {
        if self.solo {
            Section::Solo
        } else if self.open {
            Section::Chorus
        } else if self.lift {
            Section::Pre
        } else {
            Section::Verse
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Layer {
    Lift,
    Open,
    Solo,
}

impl Bed for Texture {
    type Layer = Layer;

    fn up(self, layer: Layer) -> Texture {
        match layer {
            Layer::Lift => Texture { lift: true, ..self },
            Layer::Open => Texture { open: true, ..self },
            Layer::Solo => Texture { solo: true, ..self },
        }
    }

    fn step_from(self, _from: Texture) -> &'static str {
        match self.section() {
            Section::Solo => "solos",
            Section::Chorus => "chorus",
            Section::Pre => "pre-chorus",
            _ => "verse",
        }
    }
}

/// What part of the song a bar is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Section {
    Verse,
    Pre,
    Chorus,
    /// The intro's twin melody come back, on the chorus's chords.
    Interlude,
    Solo,
    /// The guitars out, the bass and the drums under the lead.
    Break,
}

/// How the lead tells a part: as the storyteller does, and whether its
/// twin harmonises it a third over.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Lead {
    telling: Telling,
    twin: bool,
}

/// The lead resting, through the solos, which are their own.
const RESTS: Lead = Lead { telling: Telling::Off, twin: false };
/// A verse's line: every foot of the tune, legato.
const PATTER: Lead = Lead { telling: Telling::Patter, twin: false };
/// A pre-chorus's held tones, climbing into the chorus (`climb`).
const HOLDS: Lead = Lead { telling: Telling::Long, twin: false };
/// A chorus, and the interlude's twin melody — the hook as the chorus
/// sings it: the tune sung, held bars and moving bars by turns
/// (`broaden`), the twin a third over it.
const ANTHEM: Lead = Lead { telling: Telling::Phrases, twin: true };

/// The song from its first verse, on one ladder; the form's lengths are
/// the seed's (`walk_of`), the intro the opening laid before it, the
/// ending what follows its last part. A part is a phrase.
const STORY: Story<Texture, Lead> = Story {
    name: "the song",
    weight: 1.0,
    base: Texture { lift: false, open: false, solo: false },
    ladder: &[Layer::Lift, Layer::Open, Layer::Solo],
    leads: &[PATTER, HOLDS, ANTHEM, RESTS],
    turns: &[],
    halves: (2, 2),
    pace: (0.0, 1.0),
};

/// The groove's family, as the songs surveyed spread across them: the
/// skank with the kick doubled in sixteenths or on the beats, the gallop
/// carried by the kick, the rock backbeat, the half-speed feel with the
/// snare on three.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Family {
    Doubled,
    Single,
    Gallop,
    Backbeat,
    HalfSpeed,
}

impl Family {
    /// The family's name, as a band's preferences name it.
    fn name(self) -> &'static str {
        match self {
            Family::Doubled => "doubled",
            Family::Single => "single",
            Family::Gallop => "gallop",
            Family::Backbeat => "backbeat",
            Family::HalfSpeed => "half-speed",
        }
    }
}

/// The song's one signature event, as half the songs surveyed have one
/// and no two of an album share it; most fall before the last chorus.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Event {
    /// The guitars out over the bass and the drums before the last chorus.
    Break,
    /// A bar of silence before the last chorus.
    DeadBar,
    /// The second verse in another riff and feel.
    FeelSwitch,
    /// The chorus in half-time, the snare on three.
    HalfTime,
    /// The drums alone for the solo block's first two bars.
    DrumBars,
    /// The last chorus a whole step up.
    Lift,
}

impl Event {
    fn name(self) -> &'static str {
        match self {
            Event::Break => "the guitars out before the last chorus",
            Event::DeadBar => "a dead bar before the last chorus",
            Event::FeelSwitch => "the second verse in another feel",
            Event::HalfTime => "a half-time chorus",
            Event::DrumBars => "the drums alone into the solos",
            Event::Lift => "the last chorus a step up",
        }
    }
}

/// What follows the solo block: the pre-chorus into the last chorus,
/// nine of twelve; a verse first; or the last chorus at once.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SongForm {
    PreChorus,
    Verse,
    Chorus,
}

/// How the song opens, as the songs surveyed do: the twins alone with
/// the hook; the band's hits; the drums alone; held chords under the hook;
/// the riff with the band; or one guitar alone on the riff.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Intro {
    Twins,
    Hits,
    Drums,
    Held,
    Band,
    Alone,
}

/// How the song ends.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Ending {
    /// The chord held under the wash and a separate last hit.
    Ritual(Ritual, bool),
    /// A figure of the band's hits, then one stroke.
    Hits(Figure),
    /// The riff into one stab, short or let ring, sliding off or not.
    Stab(bool, bool),
    /// A bar of silence, then the band back for the last hit.
    False,
    /// The lead alone a bar, then the stab.
    Cadenza,
    /// The chord struck and left to ring, no last hit.
    Ring,
}

impl Ending {
    fn name(&self) -> &'static str {
        match self {
            Ending::Ritual(..) => "the ritual",
            Ending::Hits(_) => "the band's hits",
            Ending::Stab(..) => "one stab",
            Ending::False => "a false ending",
            Ending::Cadenza => "the lead alone, then the stab",
            Ending::Ring => "a chord left to ring",
        }
    }
}

/// The verse's riff: palm-muted sixteenths on the root; the gallop; a
/// stab and two of the pedal in threes; a chord and its chug, each
/// half-bar; eighths, under a backbeat.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Riff {
    Tremolo,
    Gallop,
    Stabs,
    Chug,
    Eighths,
}

/// The chorus's guitars, opened up: sixteenths let ring, every eighth
/// struck, or the chord struck three, three and two sixteenths apart.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Strum {
    Sixteenths,
    Eighths,
    Ringing,
}

/// How the two rhythm guitars split, where they do, as the surveyed songs
/// mostly keep to one way: the riff harmonised, one guitar on the chord's
/// third and fifth; one holding the chord while the other chugs; a pedal
/// over the riff; another voicing, the fifth and the octave; the riff an
/// octave up.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Device {
    Harmony,
    Sustain,
    Pedal,
    Voicing,
    Octave,
}

/// The tones a guitar plays a riff on: the chord's root and fifth, or the
/// split's.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Voice {
    Root,
    Upper,
    Inverted,
    Octave,
}

/// The degrees each stab of the pedal-and-stab riff stands on against
/// the bar's root; the first and the last on the beat, so on the root.
const STABS: [[i32; 5]; 3] = [[0, -1, 0, 2, 0], [0, 2, 3, 2, 0], [0, -2, -1, 0, 0]];

/// What a seed's song is: its identity first, then its parts.
struct Design {
    family: Family,
    event: Event,
    lead: u8,
    twin: u8,
    /// The two soloists, the lead's and the twin's, the first to play.
    soloists: [&'static solo::Player; 2],
    riff: Riff,
    /// The second verse's riff: the first's but where the feel switches.
    riff2: Riff,
    strum: Strum,
    /// How the guitars split and in what share of a verse or chorus's
    /// phrases; none where they never do.
    split: Option<Device>,
    share: f32,
    double_chorus: bool,
    bass_sixteenths: bool,
    fill_every: u32,
    fill: Fill,
    strings: bool,
    stabs: [i32; 5],
    form: PhraseForm,
    theme: Theme,
    verse: [&'static Schema; 2],
    pre: [&'static Schema; 2],
    chorus: [&'static Schema; 2],
    solo: [&'static Schema; 2],
    bright: bool,
    /// The form's lengths, in phrases: the verses, the pre-chorus where
    /// there is one, the chorus, and whether the last chorus is doubled.
    verses: [u32; 2],
    pre_len: Option<u32>,
    chorus_len: u32,
    doubled: bool,
    intro: Intro,
    interlude: bool,
    song: SongForm,
    ending: Ending,
    layout: Vec<(u32, SoloPart)>,
}

impl Design {
    fn lift(&self) -> i8 {
        if self.event == Event::Lift { 2 } else { 0 }
    }
    /// The chord the song comes home to: the chorus's, so a bright chorus
    /// ends on the relative major.
    fn home(&self) -> Chord {
        Chord::triad(if self.bright { 2 } else { 0 })
    }
}

/// The walk's turns, from the form's lengths: the verse, the pre-chorus
/// where there is one, the chorus; again; the solo block as laid out;
/// the way back the song form takes, through the break where the event is
/// one; and the last chorus.
fn walk_of(d: &Design) -> Vec<Turn> {
    let mut t = vec![turn((VERSE, VERSE), (d.verses[0] as i32 - 1, d.verses[0] as i32 - 1))];
    let into_chorus = |t: &mut Vec<Turn>, from_pre: bool, len: u32| {
        let hold = (len as i32 - 1, len as i32 - 1);
        if from_pre { t.push(turn((CHORUS, CHORUS), hold)) } else { t.push(leap((CHORUS, CHORUS), hold)) }
    };
    for k in 0..2 {
        if k == 1 {
            t.push(leap((VERSE, VERSE), (d.verses[1] as i32 - 1, d.verses[1] as i32 - 1)));
        }
        if let Some(p) = d.pre_len {
            t.push(turn((PRE, PRE), (p as i32 - 1, p as i32 - 1)));
        }
        into_chorus(&mut t, d.pre_len.is_some(), d.chorus_len);
    }
    let solos = (d.layout.iter().map(|(n, _)| n).sum::<u32>() / phrase::BARS) as i32;
    t.push(turn((SOLOS, SOLOS), (solos - 1, solos - 1)));
    if d.event == Event::Break {
        t.push(leap((VERSE, VERSE), (0, 0)));
    }
    let last = if d.doubled { 2 * d.chorus_len } else { d.chorus_len };
    match (d.song, d.pre_len) {
        (SongForm::PreChorus, Some(p)) => {
            if d.event == Event::Break { t.push(turn((PRE, PRE), (p as i32 - 1, p as i32 - 1))) } else { t.push(leap((PRE, PRE), (p as i32 - 1, p as i32 - 1))) }
            into_chorus(&mut t, true, last);
        }
        (SongForm::Verse, _) | (SongForm::PreChorus, None) => {
            if d.event != Event::Break {
                t.push(leap((VERSE, VERSE), (0, 0)));
            }
            into_chorus(&mut t, false, last);
        }
        (SongForm::Chorus, _) => {
            if d.event == Event::Break { into_chorus(&mut t, false, last) } else { t.push(turn((CHORUS, CHORUS), (last as i32 - 1, last as i32 - 1))) }
        }
    }
    t
}

struct Form {
    bar: u32,
    walk: Walk<Texture, Lead>,
    tune: Tune,
    design: Design,
    leads: Vec<Lead>,
    sections: Vec<Section>,
    /// Each part's riff style, its answers drawn afresh for every section,
    /// so a later verse answers its own way.
    styles: Vec<u8>,
    /// Whether the guitars split in each bar.
    splits: Vec<bool>,
}

impl Form {
    fn part_at(&self, bar: u32) -> usize {
        self.walk.parts.iter().position(|p| p.a <= bar && bar < p.b).unwrap_or(self.walk.parts.len() - 1)
    }
    fn section_at(&self, bar: u32) -> Section {
        self.sections[self.part_at(bar)]
    }
    fn chord(&self, bar: u32) -> Chord {
        self.tune.chords[bar as usize]
    }
    fn bars(&self) -> u32 {
        self.walk.bars()
    }
    fn last(&self, bar: u32) -> bool {
        bar + 1 == self.bars()
    }
    fn part_starts(&self, bar: u32) -> bool {
        self.walk.parts.iter().any(|p| p.a == bar)
    }
    fn turns_at(&self, bar: u32) -> bool {
        bar > 0 && bar < self.bars() && self.section_at(bar) != self.section_at(bar - 1)
    }
    /// The solo block's bars.
    fn solos(&self) -> Option<(u32, u32)> {
        let a = (0..self.walk.parts.len()).find(|i| self.sections[*i] == Section::Solo)?;
        let z = (a..self.walk.parts.len()).take_while(|i| self.sections[*i] == Section::Solo).last()?;
        Some((self.walk.parts[a].a, self.walk.parts[z].b))
    }
    /// Whether `bar` is one of the drums' bars before the solos.
    fn drum_bars(&self, bar: u32) -> bool {
        self.design.event == Event::DrumBars && self.solos().is_some_and(|(a, _)| bar >= a && bar < a + 2)
    }
    /// The bar of silence where the event is a dead bar: the last before
    /// the last chorus.
    fn dead_bar(&self) -> Option<u32> {
        if self.design.event != Event::DeadBar {
            return None;
        }
        self.last_chorus().map(|a| a - 1)
    }
    /// The first bar of the last chorus.
    fn last_chorus(&self) -> Option<u32> {
        let last = (0..self.walk.parts.len()).rev().find(|i| self.sections[*i] == Section::Chorus)?;
        let first = (0..=last).rev().take_while(|i| self.sections[*i] == Section::Chorus).last()?;
        Some(self.walk.parts[first].a)
    }
    /// The first bar of the first chorus's first whole phrase: the hook.
    fn hook(&self) -> u32 {
        let p = self.walk.parts.iter().enumerate().find(|(i, _)| self.sections[*i] == Section::Chorus).map(|(_, p)| p.a).unwrap_or(0);
        p.next_multiple_of(phrase::BARS)
    }
    /// The first verse's last bar after which the second's riff plays:
    /// where the first chorus ends.
    fn second_verse(&self) -> u32 {
        (0..self.walk.parts.len()).find(|i| self.sections[*i] == Section::Chorus).and_then(|c| (c..self.walk.parts.len()).find(|i| self.sections[*i] != Section::Chorus)).map_or(u32::MAX, |i| self.walk.parts[i].a)
    }
    fn runs(&self) -> Vec<Run<Lead>> {
        let mut runs: Vec<Run<Lead>> = Vec::new();
        for (i, p) in self.walk.parts.iter().enumerate() {
            let lead = self.leads[i];
            match runs.last_mut() {
                Some(r) if r.lead == lead => r.b = p.b,
                _ => runs.push(Run { lead, a: p.a, b: p.b }),
            }
        }
        runs
    }
}

pub fn build(params: &Params) -> Score {
    compose(params).0
}

fn vel(accent: i32, rng: &mut Rng) -> u8 {
    (VEL + accent + rng.range(-4, 4)).clamp(1, 127) as u8
}

fn compose(params: &Params) -> (Score, Form) {
    let rng = Rng::new(params.seed);
    let mut skeleton = rng.fork(0);
    // The band: its members, and its habits.
    let band = params.band;
    let lineup = band.metal();
    // The song's identity, before anything else.
    let families = [Family::Doubled, Family::Single, Family::Gallop, Family::Backbeat, Family::HalfSpeed];
    let weights: Vec<f32> = families.iter().zip([30.0, 20.0, 15.0, 25.0, 10.0]).map(|(f, w)| w * band.lean(f.name())).collect();
    let family = families[skeleton.weighted(&weights)];
    let event = [Event::Break, Event::DeadBar, Event::FeelSwitch, Event::HalfTime, Event::DrumBars, Event::Lift][skeleton.weighted(&[20.0, 15.0, 15.0, 10.0, 10.0, 10.0])];
    let key = Key::new(["E", "A", "D", "F#", "B", "G"][skeleton.weighted(&[4.0, 2.0, 2.0, 2.0, 2.0, 1.0])], Mode::Aeolian);
    let groove = &SPEED[if family == Family::Backbeat { 1 } else { 0 }];
    let tempo = STORY.tempo(groove.tempo, &ladder::UNBOUNDED.within(band.prefs.tempo), &mut skeleton);
    let pair = |schemata: &'static [Schema], rng: &mut Rng| {
        let (open, closed) = split(schemata);
        [open[rng.below(open.len())], closed[rng.below(closed.len())]]
    };
    let riff_of = |family: Family, rng: &mut Rng| -> Riff {
        let riffs = [Riff::Tremolo, Riff::Gallop, Riff::Stabs, Riff::Chug, Riff::Eighths];
        let weights: [f32; 5] = match family {
            Family::Doubled => [4.0, 1.0, 2.0, 1.0, 0.0],
            Family::Single => [3.0, 1.0, 2.0, 2.0, 0.0],
            Family::Gallop => [0.0, 1.0, 0.0, 0.0, 0.0],
            Family::Backbeat => [1.0, 0.5, 0.0, 2.0, 3.0],
            Family::HalfSpeed => [0.0, 0.0, 1.0, 2.0, 2.0],
        };
        riffs[rng.weighted(&weights)]
    };
    let riff = riff_of(family, &mut skeleton);
    let riff2 = if event == Event::FeelSwitch {
        let mut r = riff_of(if family == Family::Gallop { Family::Single } else { family }, &mut skeleton);
        if r == riff {
            r = if riff == Riff::Tremolo { Riff::Chug } else { Riff::Tremolo };
        }
        r
    } else {
        riff
    };
    let bright = skeleton.chance(0.35);
    let lead = band.program(Part::Lead, &[OVERDRIVEN, DISTORTION], DISTORTION);
    let shredder_leads = lineup.shredder_leads;
    let split_device = Some([Device::Harmony, Device::Sustain, Device::Pedal, Device::Voicing, Device::Octave][skeleton.weighted(&[12.0, 6.0, 6.0, 6.0, 2.0])]);
    let share = [0.0, 0.1, 0.25, 0.4, 0.6, 0.85][skeleton.weighted(&[2.0, 2.0, 3.0, 2.0, 1.0, 1.0])];
    let pre_len = if skeleton.chance(0.8) { Some([1, 2][skeleton.weighted(&[1.0, 3.0])]) } else { None };
    let verse_len = [2, 4][skeleton.weighted(&[2.0, 3.0])];
    let verse2_len = if skeleton.chance(0.3) { (verse_len - 1).max(2) } else { verse_len };
    let chorus_len = [2, 3, 4][skeleton.weighted(&[4.0, 2.0, 1.0])];
    let doubled = skeleton.chance(0.75);
    // The solo block: four, six or eight phrases.
    let solo_bars = [16, 24, 32][skeleton.weighted(&[2.0, 4.0, 1.0])];
    let song = [SongForm::PreChorus, SongForm::Verse, SongForm::Chorus][skeleton.weighted(&[6.0, 2.0, 2.0])];
    let close = match skeleton.weighted(&[35.0, 30.0, 15.0, 5.0, 5.0, 10.0]) {
        0 => Ending::Ritual(
            Ritual { hold: [1, 2, 3, 4][skeleton.weighted(&[2.0, 3.0, 2.0, 1.0])], early: skeleton.chance(0.25), kick_runs: skeleton.chance(0.5), ring: 0 },
            skeleton.chance(0.4),
        ),
        1 => Ending::Hits([Figure::Dotted, Figure::DottedQuarters, Figure::Quarters, Figure::Pair][skeleton.below(4)]),
        2 => Ending::Stab(skeleton.chance(0.5), skeleton.chance(0.2)),
        3 => Ending::False,
        4 => Ending::Cadenza,
        _ => Ending::Ring,
    };
    let pres = SONG.pre;
    let first_pre = skeleton.below(pres.len());
    let design = Design {
        family,
        event,
        lead,
        twin: band.program(Part::Second, &[OVERDRIVEN, DISTORTION], if lead == DISTORTION { OVERDRIVEN } else { DISTORTION }),
        soloists: if shredder_leads { [&SHREDDER, &SINGER] } else { [&SINGER, &SHREDDER] },
        riff,
        riff2,
        strum: [Strum::Sixteenths, Strum::Eighths, Strum::Ringing][skeleton.weighted(&[4.0, 3.0, 3.0])],
        split: split_device,
        share,
        double_chorus: skeleton.chance(0.25),
        bass_sixteenths: lineup.bass_sixteenths,
        fill_every: lineup.fill_every,
        fill: lineup.fill,
        strings: skeleton.chance(if bright { 0.5 } else { 0.2 }),
        stabs: STABS[skeleton.below(STABS.len())],
        form: FORMS[skeleton.below(FORMS.len())],
        theme: Theme::draw(groove, &SPEED_SHAPES, &mut skeleton),
        verse: pair(SONG.verse, &mut skeleton),
        pre: [&pres[first_pre], &pres[(first_pre + 1) % pres.len()]],
        chorus: if bright { pair(SONG.bright, &mut skeleton) } else { pair(SONG.chorus, &mut skeleton) },
        solo: pair(SONG.solo, &mut skeleton),
        bright,
        verses: [verse_len, verse2_len],
        pre_len,
        chorus_len,
        doubled,
        intro: [Intro::Twins, Intro::Hits, Intro::Drums, Intro::Held, Intro::Band, Intro::Alone][skeleton.weighted(&[3.0, 2.0, 1.0, 2.0, 3.0, 2.0])],
        interlude: skeleton.chance(0.6),
        song,
        ending: close,
        layout: solo::layout(solo_bars, &mut skeleton),
    };
    let instruments = vec![
        Instrument { name: "bass", program: band.program(Part::Bass, &[], BASS), channel: CH_BASS, role: Role::Pluck, low: 28, high: 52, reverb: 10, pan: 0, level: BAND },
        Instrument { name: "guitar, left", program: band.program(Part::RhythmLeft, &[], DISTORTION), channel: CH_LEFT, role: Role::Pluck, low: 38, high: 79, reverb: 18, pan: -63, level: GUITARS },
        Instrument { name: "guitar, right", program: band.program(Part::RhythmRight, &[], OVERDRIVEN), channel: CH_RIGHT, role: Role::Pluck, low: 38, high: 79, reverb: 18, pan: 63, level: GUITARS },
        Instrument { name: "lead", program: design.lead, channel: CH_LEAD, role: Role::Melody, low: TUNE.0 - 2, high: SOLO.1, reverb: 40, pan: -14, level: LEAD },
        Instrument { name: "twin", program: design.twin, channel: CH_TWIN, role: Role::Melody, low: TUNE.0 - 2, high: SOLO.1, reverb: 40, pan: 24, level: TWIN },
        Instrument { name: "strings", program: band.program(Part::Pad, &[], STRINGS), channel: CH_STRINGS, role: Role::Sustain, low: 52, high: 79, reverb: 70, pan: 0, level: STRINGS_LEVEL },
        Instrument { name: "kit", program: band.program(Part::Drums, &[], ROCK_KIT), channel: CH_KIT, role: Role::Percussion, low: KICK, high: CRASH_2, reverb: 28, pan: 0, level: KIT },
    ];
    let mut score = Score::new(key, groove.meter(), tempo, instruments, ROOM_S);
    score.lead = Some(CH_LEAD);
    score.played_by(
        band,
        Style::Metal,
        &[(CH_BASS, Part::Bass), (CH_LEFT, Part::RhythmLeft), (CH_RIGHT, Part::RhythmRight), (CH_LEAD, Part::Lead), (CH_TWIN, Part::Second), (CH_STRINGS, Part::Pad), (CH_KIT, Part::Drums)],
    );
    let bar = score.bar();

    let walk = STORY.place(&mut skeleton, &mut score, 2, &walk_of(&design), &ladder::UNBOUNDED, |_, _| 1.0);
    let mut sections: Vec<Section> = walk.parts.iter().map(|p| p.bed.section()).collect();
    let mut leads: Vec<Lead> = walk.parts.iter().map(|p| p.lead).collect();
    // The interlude takes the first part after the first chorus; the
    // break, the verse after the solos.
    if design.interlude {
        if let Some(c) = sections.iter().position(|s| *s == Section::Chorus) {
            if let Some(i) = (c + 1..sections.len()).find(|i| sections[*i] != Section::Chorus).filter(|i| sections[*i] == Section::Verse) {
                sections[i] = Section::Interlude;
                leads[i] = ANTHEM;
            }
        }
    }
    if design.event == Event::Break {
        if let Some(s) = sections.iter().rposition(|s| *s == Section::Solo) {
            if sections.get(s + 1) == Some(&Section::Verse) {
                sections[s + 1] = Section::Break;
            }
        }
    }
    // Where the event changes a part, the sheet names it there.
    let first_chorus = sections.iter().position(|s| *s == Section::Chorus);
    let second_verse_at = first_chorus.and_then(|c| (c..sections.len()).find(|i| sections[*i] != Section::Chorus)).map_or(usize::MAX, |i| i);
    let last_chorus_at = (0..sections.len()).rev().find(|i| sections[*i] == Section::Chorus).map(|last| (0..=last).rev().take_while(|i| sections[*i] == Section::Chorus).last().unwrap()).unwrap_or(usize::MAX);
    let first_solo = sections.iter().position(|s| *s == Section::Solo).unwrap_or(usize::MAX);
    for (i, (s, (section, part))) in score.sections.iter_mut().zip(sections.iter().zip(&walk.parts)).enumerate() {
        let _ = part;
        s.level = Some(match section {
            Section::Solo => LEVEL_SOLO,
            Section::Chorus | Section::Interlude => LEVEL_CHORUS,
            Section::Pre => LEVEL_PRE,
            Section::Verse => LEVEL_VERSE,
            Section::Break => LEVEL_VERSE - 1.5,
        });
        s.name = match (section, design.event) {
            (Section::Verse, Event::FeelSwitch) if i >= second_verse_at => "verse, another feel",
            (Section::Verse, _) => "verse",
            (Section::Pre, _) => "pre-chorus",
            (Section::Chorus, Event::HalfTime) => "half-time chorus",
            (Section::Chorus, Event::Lift) if i >= last_chorus_at => "chorus, a step up",
            (Section::Chorus, _) => "chorus",
            (Section::Interlude, _) => "interlude",
            (Section::Solo, Event::DrumBars) if i == first_solo => "drums into solos",
            (Section::Solo, _) => "solos",
            (Section::Break, _) => "break",
        };
    }

    // Each phrase on its section's question or answer, by turns counted
    // back from the section's last phrase, which answers.
    let phrases = walk.bars() / phrase::BARS;
    let section_of = |b: u32| sections[walk.parts.iter().position(|p| p.a <= b && b < p.b).unwrap_or(walk.parts.len() - 1)];
    let rows: Vec<&Schema> = (0..phrases)
        .map(|p| {
            let here = section_of(p * phrase::BARS);
            let left = (p + 1..phrases).take_while(|q| section_of(q * phrase::BARS) == here).count();
            let pair = match here {
                Section::Verse | Section::Break => design.verse,
                Section::Pre => design.pre,
                Section::Chorus | Section::Interlude => design.chorus,
                Section::Solo => design.solo,
            };
            pair[1 - left % 2]
        })
        .collect();
    let shifts: Vec<i32> = (0..walk.bars())
        .map(|b| match section_of(b) {
            Section::Chorus | Section::Interlude => CLIMB,
            _ => PAIRS[(b / phrase::BARS / 2) as usize % PAIRS.len()],
        })
        .collect();
    let tune = Tune::compose(&[&design.theme], &score.meter, design.form, &rows, 3, shifts);
    score.harmony = tune.chords.clone();

    // The riff's answers, afresh for every section; where the guitars
    // split, a phrase at a time.
    let mut parts_rng = rng.fork(11);
    let mut styles: Vec<u8> = Vec::new();
    for i in 0..walk.parts.len() {
        let fresh = i == 0 || sections[i] != sections[i - 1];
        styles.push(if fresh { parts_rng.below(3) as u8 } else { *styles.last().unwrap() });
    }
    // Where the guitars split, a phrase at a time: the pre-chorus most,
    // the verse and chorus by the song's share; and every song somewhere,
    // its second verse where it has no pre-chorus.
    let second_verse = sections.iter().position(|s| *s == Section::Chorus).and_then(|c| (c..sections.len()).find(|i| sections[*i] == Section::Verse)).map(|i| (walk.parts[i].a, walk.parts[i].b));
    let splits: Vec<bool> = (0..phrases)
        .flat_map(|p| {
            let b = p * phrase::BARS;
            let chance = match (design.split, section_of(b)) {
                (None, _) => 0.0,
                (_, Section::Pre) => 0.85,
                (_, Section::Verse) if design.pre_len.is_none() && second_verse.is_some_and(|(a, z)| b >= a && b < z) => 1.0,
                (_, Section::Verse | Section::Chorus) => design.share,
                (_, Section::Solo) => 0.2,
                _ => 0.0,
            };
            let on = parts_rng.chance(chance);
            [on; phrase::BARS as usize]
        })
        .collect();
    let name = |p: u8| if p == DISTORTION { "distortion" } else { "overdriven" };
    score.summary = format!(
        "the {:?} family at {:.0}, {}; a {:?} in a {:?}; verse {} and {}, {}, {} chorus {} and {}; {:?} riff{}, {:?} chorus, the guitars {}; {} lead and {} twin, the {} first, solos {:?}; {}{}, ending on {}",
        design.family,
        tempo / 2.0,
        design.event.name(),
        design.theme.shape,
        design.form,
        design.verse[0].name,
        design.verse[1].name,
        if design.pre_len.is_some() { design.pre[0].name } else { "no pre-chorus" },
        if design.bright { "bright" } else { "minor" },
        design.chorus[0].name,
        design.chorus[1].name,
        design.riff,
        if design.riff2 != design.riff { format!(" then {:?}", design.riff2) } else { String::new() },
        design.strum,
        match design.split {
            Some(d) if design.share > 0.0 => format!("splitting by {d:?} in {:.0}% of phrases", 100.0 * design.share),
            Some(d) => format!("splitting by {d:?} in the pre-chorus"),
            None => "as one".to_string(),
        },
        name(design.lead),
        name(design.twin),
        design.soloists[0].name,
        design.layout.iter().map(|(_, p)| *p).collect::<Vec<_>>(),
        match design.intro {
            Intro::Twins => "opening on the twins",
            Intro::Hits => "opening on the band's hits",
            Intro::Drums => "opening on the drums",
            Intro::Held => "opening on held chords",
            Intro::Band => "opening on the riff",
            Intro::Alone => "opening on one guitar",
        },
        if sections.contains(&Section::Interlude) { ", the twin melody again after the first chorus" } else { "" },
        design.ending.name(),
    );
    let form = Form { bar, walk, tune, design, leads, sections, styles, splits };

    guitars(&mut score, &form, &mut rng.fork(1));
    bass(&mut score, &form, &mut rng.fork(2));
    kit(&mut score, &form, &mut rng.fork(3));
    strings(&mut score, &form, &mut rng.fork(4));
    let teller = Teller {
        lead: CH_LEAD,
        second: CH_TWIN,
        double: None,
        echo: CH_TWIN,
        register: TUNE,
        sung: -2,
        riff: (4, -6),
        long: (62, 76, 0),
        under: (55, 69, -10),
        hold: Hold::Ringing,
        breathes: true,
        vel,
        fills: 0.22,
        // A half-phrase an octave up, but not where the last chorus is
        // lifted a step, which would carry it past the neck.
        soars: if form.design.lift() == 0 { 0.6 } else { 0.0 },
        pushes: 0.3,
    };
    // The pre-chorus's held tones are its own climb.
    let runs: Vec<Run<Telling>> = form.runs().into_iter().map(|r| Run { lead: if r.lead == HOLDS { Telling::Off } else { r.lead.telling }, a: r.a, b: r.b }).collect();
    teller::tell(&mut score, &teller, &form.tune, &runs, &mut rng.fork(5));
    climb(&mut score, &form, &mut rng.fork(10));
    under_cap(&mut score, &form);
    let sung: Vec<(u32, u32)> = form.runs().iter().filter(|r| r.lead.telling == Telling::Phrases).map(|r| (r.a, r.b)).collect();
    broaden(&mut score, &form, &sung);
    climax(&mut score, &form, &mut rng.fork(12));
    solos(&mut score, &form, &mut rng.fork(6));
    repair(&mut score, &form, CH_LEAD);
    score.mark_phrases(0, form.bars());
    let mut twinned: Vec<(u32, u32)> = form.runs().iter().filter(|r| r.lead.twin).map(|r| (r.a, r.b)).collect();
    twinned.extend(twin_breaks(&form));
    harmonise(&mut score, &form, &twinned, &mut rng.fork(7));
    if let Some(b) = form.dead_bar() {
        silence(&mut score, b);
    }
    ending(&mut score, &form, &mut rng.fork(8));
    intro(&mut score, &form, &mut rng.fork(9));
    repair(&mut score, &form, CH_LEAD);
    repair(&mut score, &form, CH_TWIN);
    if form.design.lift() != 0 {
        if let Some(at) = form.last_chorus() {
            let intro = score.sections.iter().take_while(|s| s.name.starts_with("intro")).map(|s| s.end).max().unwrap_or(0);
            score.modulate(intro + at.next_multiple_of(phrase::BARS) * form.bar, form.design.lift());
        }
    }
    score.finish();
    (score, form)
}

/// A bar of the whole band's silence at `bar`: what struck in it is gone
/// and what was held into it stops at its bar line; its section a part of
/// its own, set to no level, since silence has none.
fn silence(score: &mut Score, bar: u32) {
    let (a, z) = (bar * score.bar(), (bar + 1) * score.bar());
    score.notes.retain(|n| !(n.start >= a && n.start < z));
    for n in score.notes.iter_mut().filter(|n| n.start < a && n.end() > a) {
        n.len = a - n.start;
    }
    if let Some(i) = score.sections.iter().position(|s| s.start <= a && a < s.end) {
        let mut tail = score.sections[i].clone();
        score.sections[i].end = a;
        let mut dead = tail.clone();
        dead.name = "dead bar";
        dead.start = a;
        dead.end = z;
        dead.level = None;
        tail.start = z;
        let mut at = i + 1;
        if score.sections[i].end == score.sections[i].start {
            score.sections.remove(i);
            at = i;
        }
        score.sections.insert(at, dead);
        if tail.end > tail.start {
            score.sections.insert(at + 1, tail);
        }
    }
}

/// The power chord on `chord`'s root in the guitars' register: root,
/// fifth, octave.
fn power(key: &Key, chord: Chord) -> [u8; 3] {
    let root = at_degree(key, chord, chord.root, ROOT.0, ROOT.1, ROOT.2);
    let fifth = at_degree(key, chord, chord.root + 4, root + 1, root + 8, root + 7);
    [root, fifth, root + 12]
}

/// A guitar's tones on `chord` as its `voice` has them: the power chord,
/// or the split's — the chord's third and fifth, the fifth and the
/// octave, the power chord an octave up.
fn voiced(key: &Key, chord: Chord, voice: Voice) -> [u8; 3] {
    let [root, fifth, top] = power(key, chord);
    match voice {
        Voice::Root => [root, fifth, top],
        Voice::Upper => {
            let third = at_degree(key, chord, chord.root + 2, root + 1, root + 6, root + 3);
            [third, fifth, third + 12]
        }
        Voice::Inverted => [fifth, top, fifth + 12],
        Voice::Octave => [top, fifth + 12, top],
    }
}

/// The power chord on the key's `degree`, its root near `to`; a degree
/// whose fifth is not perfect stands on the degree under it.
fn power_on(key: &Key, degree: i32, to: u8) -> [u8; 3] {
    let degree = if key.pitch(degree + 4, 4) - key.pitch(degree, 4) == 7 { degree } else { degree - 1 };
    let root = nearest_degree(key, degree, to).clamp(ROOT.0, ROOT.1 + 2);
    let root = if key.contains(root) { root } else { key.snap(root) };
    let d = key.absolute_degree(root).unwrap();
    [root, key.pitch(d + 4, 4), root + 12]
}

/// The chord's tone at `degree` nearest `to` within `lo..=hi`.
fn at_degree(key: &Key, chord: Chord, degree: i32, lo: u8, hi: u8, to: u8) -> u8 {
    *chord.pitches_within(key, lo, hi).iter().filter(|p| key.under(chord).degree_of(**p) == Some(degree.rem_euclid(7) as usize)).min_by_key(|p| ((**p as i32 - to as i32).abs(), **p)).unwrap()
}

/// The key's tone at `degree` nearest `to`, in any octave.
fn nearest_degree(key: &Key, degree: i32, to: u8) -> u8 {
    (0..10).map(|o| key.pitch(degree.rem_euclid(7), o)).min_by_key(|p| (*p as i32 - to as i32).abs()).unwrap()
}

/// What a bar of the riff is to its phrases, and what follows it.
#[derive(Clone, Copy)]
struct Place {
    role: Bar,
    /// Whether the bar closes a pair of phrases, turning round hardest.
    pair: bool,
    /// The section's way of answering, of three.
    style: u8,
    /// The next bar's power chord, where the riff runs on into one.
    next: Option<[u8; 3]>,
}

/// One bar of the rhythm guitar on `channel` from `start`: the figure
/// on `chord`'s tones as `voice` has them, palm-muted; its phrase's second
/// bar answering in the section's style, its fourth turning into the next
/// bar's root, its eighth turning harder — the band's hits through its
/// last half, or a run of four into the next root.
#[allow(clippy::too_many_arguments)]
fn riff_bar(score: &mut Score, channel: u8, start: u32, chord: Chord, figure: Riff, place: Place, voice: Voice, design: &Design, rng: &mut Rng) {
    let key = score.key_at(start);
    let [root, fifth, top] = voiced(&key, chord, voice);
    let muted = |score: &mut Score, at: u32, accent: i32, rng: &mut Rng| {
        for p in [root, fifth] {
            score.add(Note { start: start + at, len: S * 2 / 5, pitch: p, vel: vel(accent, rng), channel });
        }
    };
    let open = |score: &mut Score, at: u32, len: u32, tones: [u8; 3], accent: i32, rng: &mut Rng| {
        for p in tones {
            score.add(Note { start: start + at, len, pitch: p, vel: vel(accent, rng), channel });
        }
    };
    let turning = place.role == Bar::Cadence && place.next.is_some();
    let hard = turning && place.pair;
    let hits = hard && place.style != 1;
    let run = if hard && !hits { 3 } else if turning { 2 } else { 0 };
    let until = if hits { 8 } else { 16 - run };
    let variant = place.role == Bar::Variant;
    let chord_tones = [root, fifth, top];
    match figure {
        Riff::Tremolo => {
            for k in 0..until {
                let stab = variant && match place.style {
                    0 => k >= 14,
                    1 => k == 8 || k == 9,
                    _ => k == 6 || k == 7,
                };
                if stab {
                    if k % 2 == 0 {
                        open(score, k * S, 2 * S - 10, chord_tones, 2, rng);
                    }
                    continue;
                }
                muted(score, k * S, if k % 4 == 0 { -4 } else { -16 }, rng);
            }
        }
        Riff::Gallop => {
            for beat in 0..4 {
                let at = beat * 4;
                let reverse = variant && match place.style {
                    0 => beat == 3,
                    1 => beat % 2 == 1,
                    _ => false,
                };
                let held = variant && place.style == 2 && beat == 3;
                if held && at < until {
                    open(score, at * S, 4 * S - 10, chord_tones, 2, rng);
                    continue;
                }
                let slots: &[(u32, u32)] = if reverse { &[(0, 1), (1, 1), (2, 2)] } else { &[(0, 2), (2, 1), (3, 1)] };
                for (k, (s, n)) in slots.iter().enumerate() {
                    if at + s >= until {
                        break;
                    }
                    let accent = if k == 0 && !reverse { -2 } else { -14 };
                    if reverse && *n == 2 {
                        open(score, (at + s) * S, n * S - 10, chord_tones, 0, rng);
                    } else {
                        muted(score, (at + s) * S, accent, rng);
                    }
                }
            }
        }
        Riff::Stabs => {
            let groups = [0u32, 3, 6, 9, 12];
            let base = design.stabs;
            let stabs = if !variant {
                base
            } else if place.style == 0 {
                [base[0], base[3], base[2], base[1], base[4]]
            } else {
                [base[0], -base[1], -base[2], -base[3], base[4]]
            };
            for (g, at) in groups.iter().enumerate() {
                let end = groups.get(g + 1).copied().unwrap_or(16).min(until);
                if *at >= until {
                    break;
                }
                let degree = key.under(chord).standing_degree(root) + stabs[g];
                let tones = if stabs[g] == 0 { chord_tones } else { power_on(&key, degree, root) };
                open(score, at * S, S - 10, tones, -2, rng);
                for k in at + 1..end {
                    muted(score, k * S, -16, rng);
                }
            }
        }
        Riff::Chug => {
            for half in 0..2 {
                let at = half * 8;
                let stabs: &[u32] = match (variant, place.style) {
                    (false, _) => &[0],
                    (true, 0) => if half == 1 { &[3] } else { &[0] },
                    (true, 1) => if half == 0 { &[0, 6] } else { &[0] },
                    _ => &[0, 4],
                };
                let mut k = at;
                while k < (at + 8).min(until) {
                    if stabs.contains(&(k - at)) {
                        open(score, k * S, 2 * S - 10, chord_tones, 0, rng);
                        k += 2;
                        continue;
                    }
                    muted(score, k * S, -14, rng);
                    k += 1;
                }
            }
        }
        Riff::Eighths => {
            // The chord rung on the bar line and on the and of two; the
            // answer moves the second.
            let accents = if !variant { [0, 6] } else { [[0, 10], [0, 14], [0, 4]][place.style as usize % 3] };
            for k in (0..until).step_by(2) {
                if accents.contains(&k) {
                    open(score, k * S, if k == 0 { 3 * S } else { 2 * S - 10 }, chord_tones, 0, rng);
                } else {
                    muted(score, k * S, -10, rng);
                }
            }
        }
    }
    if hits {
        for (at, n) in [(8u32, 3u32), (11, 3), (14, 2)] {
            open(score, at * S, n * S - 15, chord_tones, if at == 8 { 4 } else { 0 }, rng);
        }
    } else if run > 0 {
        let next = place.next.unwrap();
        let to = key.standing_degree(key.snap(next[0]));
        let from = key.standing_degree(root);
        for k in 0..run {
            let d = if to >= from { to - (run as i32 - k as i32) } else { to + (run as i32 - k as i32) };
            let p = key.pitch(d, 4).clamp(ROOT.0 - 2, ROOT.1 + 12);
            score.add(Note { start: start + (16 - run + k) * S, len: S - 10, pitch: p, vel: vel(-6 + 2 * k as i32, rng), channel });
        }
    }
}

/// One bar of the guitars opened up: the chorus's strum on `chord`'s
/// tones as `voice` has them, its variant pushing into the next bar, its
/// cadence driving the last two beats in eighths, and the eighth bar the
/// band's hits through its last half.
fn open_bar(score: &mut Score, channel: u8, start: u32, chord: Chord, strum: Strum, place: Place, voice: Voice, rng: &mut Rng) {
    let key = score.key_at(start);
    let tones = voiced(&key, chord, voice);
    let hit = |score: &mut Score, at: u32, len: u32, accent: i32, rng: &mut Rng| {
        for p in tones {
            score.add(Note { start: start + at, len, pitch: p, vel: vel(accent, rng), channel });
        }
    };
    let turning = place.role == Bar::Cadence && place.next.is_some();
    let hits = turning && place.pair && place.style != 2;
    let pushed = place.role == Bar::Variant && strum != Strum::Ringing;
    let until = if turning { 8 } else if pushed { 12 } else { 16 };
    if pushed {
        hit(score, 12 * S, 4 * S - 12, 4, rng);
    }
    match strum {
        Strum::Sixteenths => {
            for k in 0..until {
                hit(score, k * S, S - 12, if k % 4 == 0 { 0 } else { -12 }, rng);
            }
        }
        Strum::Eighths => {
            for k in (0..until).step_by(2) {
                hit(score, k * S, 2 * S - 20, if k % 4 == 0 { 2 } else { -8 }, rng);
            }
        }
        Strum::Ringing => {
            let hits: &[(u32, u32)] = if place.role == Bar::Variant { &[(0, 3), (3, 3), (6, 4), (10, 6)] } else { &[(0, 3), (3, 3), (6, 10)] };
            for (at, n) in hits {
                if *at < until {
                    hit(score, at * S, (n * S).min((until - at) * S) - 12, if *at == 0 { 4 } else { -2 }, rng);
                }
            }
        }
    }
    if hits {
        for (at, n) in [(8u32, 3u32), (11, 3), (14, 2)] {
            hit(score, at * S, n * S - 15, if at == 8 { 4 } else { 0 }, rng);
        }
    } else if turning {
        for k in (8..16).step_by(2) {
            hit(score, k * S, 2 * S - 20, -10 + 2 * (k - 8) as i32, rng);
        }
    }
}

/// The right guitar's own part where the pair splits by holding or by a
/// pedal: the chord rung on the bar's first and third beats while the
/// left chugs, or eighths on a high tone of the chord over the left's
/// riff.
fn held_bar(score: &mut Score, start: u32, chord: Chord, device: Device, role: Bar, rng: &mut Rng) {
    let key = score.key_at(start);
    let tones = power(&key, chord);
    match device {
        Device::Sustain => {
            // Struck on one and three; the variant pushes the second to the
            // and of two, the cadence adds a push on the and of four.
            let strokes: &[(u32, u32)] = match role {
                Bar::Variant => &[(0, 6), (6, 10)],
                Bar::Cadence => &[(0, 8), (8, 6), (14, 2)],
                _ => &[(0, 8), (8, 8)],
            };
            for (at, n) in strokes {
                for p in tones {
                    score.add(Note { start: start + at * S, len: n * S - 15, pitch: p, vel: vel(if *at == 0 { 2 } else { -4 }, rng), channel: CH_RIGHT });
                }
            }
        }
        _ => {
            // A pedal on the fifth an octave up; the variant's last eighth
            // a step over it, the cadence's last two stepping to the root.
            let high = tones[1] + 12;
            let d = key.standing_degree(high);
            for k in (0..16).step_by(2) {
                let p = match (role, k) {
                    (Bar::Variant, 14) => key.pitch(d + 1, 4),
                    (Bar::Cadence, 12) => key.pitch(d - 1, 4),
                    (Bar::Cadence, 14) => key.pitch(d - 2, 4),
                    _ => high,
                };
                let p = if k % 4 == 0 && !chord.holds(&key, p) { high } else { p };
                score.add(Note { start: start + k * S, len: 2 * S - 25, pitch: p, vel: vel(if k % 4 == 0 { -2 } else { -10 }, rng), channel: CH_RIGHT });
            }
        }
    }
}

/// The rhythm guitars. Both play one part, two takes on two amps, one
/// each side, but where the song splits them: then the left keeps the
/// part and the right harmonises it, holds the chord, rides a pedal over
/// it, takes another voicing or plays it an octave up. A verse plays the
/// song's riff, the second verse its own where the feel switches; a
/// pre-chorus palm-muted sixteenths on its climbing roots; a chorus the
/// song's strum opened up; under the solos the palm-muted pedal. They
/// rest through the break and the drums' bars; the interlude drops the
/// right guitar now and then, as bridges do.
fn guitars(score: &mut Score, form: &Form, rng: &mut Rng) {
    let second = form.second_verse();
    for b in 0..form.bars() {
        let section = form.section_at(b);
        if section == Section::Break || form.drum_bars(b) {
            continue;
        }
        let start = b * form.bar;
        let chord = form.chord(b);
        let next = (!form.last(b)).then(|| power(&score.key_at((b + 1) * form.bar), form.chord(b + 1)));
        let place = Place { role: variation::role(b), pair: variation::closes_pair(b), style: form.styles[form.part_at(b)], next };
        let riff = if b >= second { form.design.riff2 } else { form.design.riff };
        let split = form.splits[b as usize].then_some(form.design.split).flatten();
        for channel in [CH_LEFT, CH_RIGHT] {
            if channel == CH_RIGHT && section == Section::Interlude && variation::role(b) != Bar::First && rng.chance(0.3) {
                continue;
            }
            let voice = match (channel, split) {
                (CH_RIGHT, Some(Device::Harmony)) => Voice::Upper,
                (CH_RIGHT, Some(Device::Voicing)) => Voice::Inverted,
                (CH_RIGHT, Some(Device::Octave)) => Voice::Octave,
                (CH_RIGHT, Some(d @ (Device::Sustain | Device::Pedal))) => {
                    held_bar(score, start, chord, d, variation::role(b), rng);
                    continue;
                }
                _ => Voice::Root,
            };
            match section {
                Section::Chorus => open_bar(score, channel, start, chord, form.design.strum, place, voice, rng),
                Section::Pre | Section::Solo => riff_bar(score, channel, start, chord, Riff::Tremolo, place, voice, &form.design, rng),
                _ => riff_bar(score, channel, start, chord, riff, place, voice, &form.design, rng),
            }
        }
    }
}

/// The bass, Grosskopf's way: the guitar's root in unison an octave
/// under, on its own rhythm — sixteenths under sixteenth riffs or
/// eighths, the gallop under the gallop, eighths under the chorus —
/// never an octave jump. Through the break it carries the song with the
/// drums; it rests for the drums' bars.
fn bass(score: &mut Score, form: &Form, rng: &mut Rng) {
    let second = form.second_verse();
    for b in 0..form.bars() {
        if form.drum_bars(b) {
            continue;
        }
        let riff = if b >= second { form.design.riff2 } else { form.design.riff };
        let rhythm: Vec<(u32, u32)> = match (form.section_at(b), riff) {
            (Section::Verse | Section::Interlude | Section::Break, Riff::Gallop) => (0..4).flat_map(|beat| [(beat * 4, 2), (beat * 4 + 2, 1), (beat * 4 + 3, 1)]).collect(),
            (Section::Chorus, _) | (Section::Verse | Section::Interlude, Riff::Eighths) => EIGHTHS.to_vec(),
            _ if form.design.bass_sixteenths => (0..16).map(|k| (k, 1)).collect(),
            _ => EIGHTHS.to_vec(),
        };
        let next = (!form.last(b)).then(|| form.chord(b + 1));
        bass_bar(score, b * form.bar, form.chord(b), next, variation::role(b), &rhythm, rng);
    }
}

/// A bar of eighths, in sixteenths: where each falls and how long.
const EIGHTHS: [(u32, u32); 8] = [(0, 2), (2, 2), (4, 2), (6, 2), (8, 2), (10, 2), (12, 2), (14, 2)];

/// One bar of the bass from `start` on `chord`'s root in `rhythm`, its
/// variant's last eighth on the chord's fifth over the root, its
/// cadence's a step of the mode from the `next` chord's root.
fn bass_bar(score: &mut Score, start: u32, chord: Chord, next: Option<Chord>, role: Bar, rhythm: &[(u32, u32)], rng: &mut Rng) {
    let bar = score.bar();
    let key = score.key_at(start);
    let [root, fifth, _] = power(&key, chord).map(|p| p - 12);
    let turn = match (role, next) {
        (Bar::Cadence, Some(next)) => Some(approach(&key, root, power(&score.key_at(start + bar), next)[0] - 12)),
        (Bar::Variant, _) => Some(fifth),
        _ => None,
    };
    let until = if turn.is_some() { 14 } else { 16 };
    for (at, n) in rhythm.iter().filter(|(at, _)| *at < until) {
        let n = (*n).min(until - at);
        score.add(Note { start: start + at * S, len: n * S * 3 / 4, pitch: root, vel: vel(if at % 4 == 0 { -2 } else { -12 }, rng), channel: CH_BASS });
    }
    if let Some(p) = turn {
        score.add(Note { start: start + 14 * S, len: 2 * S * 3 / 4, pitch: p, vel: vel(-6, rng), channel: CH_BASS });
    }
}

/// The tone a bass on `from` takes into `to`, in the bass's register: a
/// step of the mode from `to` on the side it comes from, but the far side
/// where the two are a step apart, since the near side is `from` itself.
fn approach(key: &Key, from: u8, to: u8) -> u8 {
    let (f, t) = (key.standing_degree(from), key.standing_degree(to));
    let step = match t - f {
        1 | -1 => t + (t - f),
        d if d > 0 => t - 1,
        _ => t + 1,
    };
    let mut p = key.pitch(step, 4);
    while p < 28 {
        p += 12;
    }
    while p > 52 {
        p -= 12;
    }
    p
}

/// The cymbal a part keeps its time on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Lane {
    Hat,
    Ride,
    /// The crash on the beats and the ride between, the way Schwichtenberg
    /// rode the crash itself.
    Crash,
}

/// The kit. The verse plays the song's family — the skank with the kick
/// doubled in sixteenths or on the beats, the gallop's kick, the
/// backbeat, the half-speed feel's snare on three — and each part of the
/// song lifts it its own way, so arriving at a chorus is heard: the
/// pre-chorus doubles the kick, or rides the crash where the verse's is
/// doubled already; the chorus moves to the ride, the snare on three
/// where the chorus is half-time; the solos ride over the doubled kick.
/// Every phrase's variant opens the hat into the next bar or strikes the
/// crash on its last off-beat; the kit fills a beat or two every
/// `fill_every` bars and into every new part; a crash and the kick strike
/// every part's first beat and the beat after a fill. The drums' bars
/// before the solos are a tom groove alone.
fn kit(score: &mut Score, form: &Form, rng: &mut Rng) {
    let family = form.design.family;
    for b in 0..form.bars() {
        let start = b * form.bar;
        let section = form.section_at(b);
        if form.drum_bars(b) {
            for k in 0..16u32 {
                let pitch = if k % 4 == 2 { SNARE } else if k % 2 == 0 { TOMS[(k as usize / 2 + b as usize) % 4 + 2] } else { KICK };
                score.add(Note { start: start + k * S, len: S - 10, pitch, vel: vel(if k % 4 == 0 { 6 } else { -6 }, rng), channel: CH_KIT });
            }
            score.add(Note { start, len: 2 * S, pitch: CRASH, vel: vel(28, rng), channel: CH_KIT });
            continue;
        }
        let verse_doubled = family == Family::Doubled;
        let (lane, doubled) = match section {
            Section::Verse | Section::Interlude | Section::Break => (Lane::Hat, verse_doubled),
            Section::Pre => (if verse_doubled { Lane::Crash } else { Lane::Hat }, true),
            Section::Chorus => (Lane::Ride, form.design.double_chorus),
            Section::Solo => (Lane::Ride, true),
        };
        let half_time = (family == Family::HalfSpeed && matches!(section, Section::Verse | Section::Interlude | Section::Break)) || (form.design.event == Event::HalfTime && section == Section::Chorus);
        let backbeat = family == Family::Backbeat && section != Section::Solo;
        let gallop = family == Family::Gallop && matches!(section, Section::Verse | Section::Interlude | Section::Break);
        let turning = !form.last(b) && form.turns_at(b + 1);
        let role = variation::role(b);
        let fill_from = if turning {
            Some(if rng.chance(0.5) { 8 } else { 4 })
        } else if !form.last(b) && role == Bar::Cadence && (b + 1) % form.design.fill_every == 0 {
            Some(if rng.chance(0.6) { 12 } else { 8 })
        } else {
            None
        };
        let until = fill_from.unwrap_or(16);
        let opened = role == Bar::Variant && rng.chance(0.5);
        for k in 0..until {
            let at = start + k * S;
            let eighth = k % 2 == 0;
            let beat = k % 4 == 0;
            let kick = if gallop {
                [0, 2, 3].contains(&(k % 4))
            } else if doubled {
                true
            } else if !eighth {
                false
            } else if half_time {
                [0u32, 3, 5].contains(&(k / 2))
            } else if backbeat {
                [0u32, 3, 4].contains(&(k / 2))
            } else {
                beat
            };
            if kick {
                score.add(Note { start: at, len: S, pitch: KICK, vel: vel(if eighth { -6 } else { -14 }, rng), channel: CH_KIT });
            }
            if !eighth {
                continue;
            }
            let i = k / 2;
            let (cymbal, accent) = if role == Bar::Variant && i == 7 {
                if opened { (OPEN_HAT, 18) } else { (CRASH, 28) }
            } else {
                match lane {
                    Lane::Crash => (if beat { CRASH } else { RIDE }, if beat { 24 } else { 18 }),
                    Lane::Ride => (if beat { RIDE_BELL } else { RIDE }, if beat { 22 } else { 18 }),
                    Lane::Hat => (HAT, if beat { 22 } else { 12 }),
                }
            };
            score.add(Note { start: at, len: S, pitch: cymbal, vel: vel(accent, rng), channel: CH_KIT });
            let snare = if half_time { i == 4 } else if backbeat { [2u32, 6].contains(&i) } else { i % 2 == 1 };
            if snare {
                score.add(Note { start: at, len: S, pitch: SNARE, vel: vel(8, rng), channel: CH_KIT });
            }
        }
        let after_fill = b > 0 && (form.turns_at(b) || (role == Bar::First && b % form.design.fill_every == 0));
        if form.part_starts(b) || after_fill || (section == Section::Chorus && b % phrase::BARS == 0) {
            score.add(Note { start, len: 2 * S, pitch: CRASH, vel: vel(32, rng), channel: CH_KIT });
        }
        if let Some(from) = fill_from {
            let shape = if turning && from == 8 && rng.chance(0.4) { Fill::Roll } else { form.design.fill };
            rock::fill(score, CH_KIT, shape, start + from * S, 16 - from, (-4, 30), Some(4), vel, rng);
        }
    }
}

/// The strings, where the seed brings them, under the choruses: three
/// voices of the chord, each led to the nearest tone of the next.
fn strings(score: &mut Score, form: &Form, rng: &mut Rng) {
    if !form.design.strings {
        return;
    }
    let mut voices: Option<Vec<u8>> = None;
    for b in 0..form.bars() {
        if form.section_at(b) != Section::Chorus {
            voices = None;
            continue;
        }
        let key = score.key_at(b * form.bar);
        let chord = form.chord(b);
        let mut candidates = chord.pitches_within(&key, 55, 76);
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
            score.hold(Note { start: b * form.bar, len: form.bar + E / 8, pitch: *p, vel: vel(-16, rng), channel: CH_STRINGS });
        }
        voices = Some(next);
    }
}

/// The solo block's parts as placed: each part's bars and who plays it,
/// a turn's channel or none for the twin break; the drums' bars, where
/// the event is theirs, come off the first part's front.
fn block(form: &Form) -> Vec<(u32, u32, Option<(u8, usize)>)> {
    let Some((a, z)) = form.solos() else { return Vec::new() };
    let mut out = Vec::new();
    let mut at = a;
    for (k, (len, part)) in form.design.layout.iter().enumerate() {
        let from = if k == 0 && form.design.event == Event::DrumBars { at + 2 } else { at };
        let to = (at + len).min(z);
        let who = match part {
            SoloPart::Turn(p) => Some((if *p == 0 { CH_LEAD } else { CH_TWIN }, *p)),
            SoloPart::Together => None,
        };
        if from < to {
            out.push((from, to, who));
        }
        at += len;
    }
    out
}

/// The twin breaks: the bars of the solo block the two guitars play the
/// theme in thirds.
fn twin_breaks(form: &Form) -> Vec<(u32, u32)> {
    block(form).into_iter().filter(|(_, _, p)| p.is_none()).map(|(a, z, _)| (a, z)).collect()
}

/// The solo block: each turn a player's (`solo::turn`), every turn after
/// the first answering the one before it by contrast; the twin break the
/// theme as a riff on the lead, the twin's harmony following with every
/// other twinned run's.
fn solos(score: &mut Score, form: &Form, rng: &mut Rng) {
    let mut last: Option<SoloShape> = None;
    for (from, to, who) in block(form) {
        let Some((channel, p)) = who else {
            let top = cap(score, &form.design);
            let mut line: Vec<Placed> = (from..to).flat_map(|b| form.tune.bar(score, b, TUNE.0, top, false)).collect();
            form.tune.repair(score, &mut line, TUNE.0, top);
            for (start, len, pitch) in line {
                let held = if len >= 3 * E { 2 * E } else { len - E / 2 };
                score.add(Note { start, len: held.max(E / 4), pitch, vel: vel(4, rng), channel: CH_LEAD });
            }
            continue;
        };
        let player = form.design.soloists[p];
        let shape = match &last {
            Some(prev) => SoloShape::answer(prev, player, rng),
            None => SoloShape::draw(player, rng),
        };
        let top = cap(score, &form.design);
        let line = solo::turn(score, from, to, (SOLO.0, top.max(SOLO.0 + 12)), player, &shape, rng);
        sound(score, form, line, (from, to), channel, rng);
        last = Some(shape);
    }
}

/// A solo turn's `line` played on `channel`: in the register and the
/// bar's key, every strong beat's tone bent onto the chord and the line
/// repaired so no leap leaps on, struck harder as it goes.
fn sound(score: &mut Score, form: &Form, mut line: Vec<Placed>, (a, z): (u32, u32), channel: u8, rng: &mut Rng) {
    let (lo, hi) = (SOLO.0, cap(score, &form.design).max(SOLO.0 + 12));
    line.sort_by_key(|n| n.0);
    for n in line.iter_mut() {
        let key = score.key_at(n.0);
        n.2 = key.snap(n.2);
        while n.2 < lo {
            n.2 += 12;
        }
        while n.2 > hi {
            n.2 -= 12;
        }
    }
    let mut prev: Option<u8> = None;
    for n in line.iter_mut() {
        let chord = score.chord_at(n.0);
        let key = score.key_at(n.0);
        if score.strong(n.0) && !chord.holds(&key, n.2) {
            n.2 = tune::bent_to_chord(&key, chord, n.2, prev, lo, hi);
        }
        prev = Some(n.2);
    }
    form.tune.repair(score, &mut line, lo, hi);
    let span = (z - a) * form.bar;
    for (start, len, pitch) in &line {
        let grow = (12 * start.saturating_sub(a * form.bar) / span) as i32 - 2;
        let accent = grow + if *len >= 2 * E { 8 } else if score.strong(*start) { 2 } else { -6 };
        let len = if *len <= S { (*len).saturating_sub(8).max(E / 8) } else { len - E / 8 };
        score.add(Note { start: *start, len, pitch: *pitch, vel: vel(accent, rng), channel });
    }
}

/// The pre-chorus's held tones, three a bar: a chord tone held most of
/// the bar and two eighths stepping on toward the next, over the verse's
/// floor and so clear of the rhythm guitars, climbing a step every two
/// bars to a step under the tone the twin opens the chorus on — its top in
/// its last bar, as the pre-choruses surveyed peak.
fn climb(score: &mut Score, form: &Form, rng: &mut Rng) {
    let (lo, hi) = TUNE;
    let floor = tune::home_tonic(&score.key, lo, hi);
    for run in form.runs().iter().filter(|r| r.lead == HOLDS) {
        let next = run.b.min(form.bars() - 1);
        let goal = form.tune.bar(score, next, lo, hi, true).first().map(|t| t.2).unwrap_or(floor);
        let top = score.key_at(next * form.bar).standing_degree(goal) + 1;
        let pairs = (run.b - run.a).div_ceil(2) as i32;
        let tones: Vec<u8> = (run.a..run.b)
            .enumerate()
            .map(|(k, b)| {
                let key = score.key_at(b * form.bar);
                let want = key.pitch(top - (pairs - 1 - k as i32 / 2), 4).max(floor);
                form.chord(b).pitches_within(&key, floor, hi).into_iter().min_by_key(|p| ((*p as i32 - want as i32).abs(), *p)).unwrap()
            })
            .collect();
        for (k, b) in (run.a..run.b).enumerate() {
            let start = b * form.bar;
            let key = score.key_at(start);
            let chord = form.chord(b);
            let pitch = tones[k];
            let to = tones.get(k + 1).copied().unwrap_or(goal);
            // Held through the bar's first half, a chord tone a step on
            // toward the next on three, and the and of four stepping into it.
            score.add(Note { start, len: 4 * E - E / 8, pitch, vel: vel(if k == 0 { 2 } else { -2 }, rng), channel: CH_LEAD });
            let toward = if to > pitch { pitch + 2 } else if to < pitch { pitch.saturating_sub(2) } else { pitch };
            let third = chord.pitches_within(&key, floor, hi).into_iter().min_by_key(|p| ((*p as i32 - toward as i32).abs(), *p)).unwrap_or(pitch);
            score.add(Note { start: start + 4 * E, len: 3 * E - E / 8, pitch: third, vel: vel(-6, rng), channel: CH_LEAD });
            let (from_d, to_d) = (key.standing_degree(third), key.standing_degree(to));
            let pick = key.pitch(if to_d > from_d { to_d - 1 } else if to_d < from_d { to_d + 1 } else { from_d + 1 }, 4);
            if key.contains(pick) && pick >= lo && pick <= hi {
                score.add(Note { start: start + 7 * E, len: E - E / 8, pitch: pick, vel: vel(-8, rng), channel: CH_LEAD });
            }
        }
    }
}

/// The lead's sung line through `spans`, as Helloween's choruses are sung
/// against their verses: the first and third bar of every phrase held, a
/// tone a half-bar held through the tones it stands for, the second and
/// fourth moving as sung, so held bars and moving bars take turns.
fn broaden(score: &mut Score, form: &Form, spans: &[(u32, u32)]) {
    let half = form.bar / 2;
    for (a, z) in spans {
        for b in (*a..*z).filter(|b| b % 2 == 0) {
            let (from, to) = (b * form.bar, (b + 1) * form.bar);
            let mut line: Vec<Note> = score.notes.iter().filter(|n| n.channel == CH_LEAD && n.start >= from && n.start < to && n.len >= E / 2).copied().collect();
            line.sort_by_key(|n| n.start);
            score.notes.retain(|n| !(n.channel == CH_LEAD && n.start >= from && n.start < to));
            let mut kept: Vec<Note> = Vec::new();
            for n in line {
                match kept.last_mut() {
                    Some(k) if k.start / half == n.start / half => k.len = n.end().max(k.end()) - k.start,
                    _ => kept.push(n),
                }
            }
            for n in kept {
                score.add(n);
            }
        }
    }
}

/// The last chorus's climb, where the ending holds the lead over it: its
/// last two bars run up in eighths from where the line stood to a step
/// under the tone the lead holds over the ending, the song's highest —
/// the strong beats on the chord.
fn climax(score: &mut Score, form: &Form, rng: &mut Rng) {
    if !holds_lead(&form.design.ending) {
        return;
    }
    let first = form.bars() - 2;
    let start = first * form.bar;
    let end = form.bars() * form.bar;
    let peak = peak(score, &form.design);
    let before = score.notes.iter().filter(|n| n.channel == CH_LEAD && n.start < start && n.len >= E / 2).max_by_key(|n| n.start).map_or(peak - 12, |n| n.pitch);
    score.notes.retain(|n| !(n.channel == CH_LEAD && n.start >= start && n.start < end));
    for n in score.notes.iter_mut().filter(|n| n.channel == CH_LEAD && n.start < start && n.end() > start) {
        n.len = start - n.start;
    }
    let key = score.key_at(start);
    let (from, to) = (key.standing_degree(key.snap(before)), key.standing_degree(peak) - 1);
    let line: Vec<i32> = std::iter::once(from).chain(run_between(from, to, 16)).collect();
    for (k, d) in line.into_iter().enumerate() {
        let at = start + k as u32 * E;
        let key = score.key_at(at);
        let chord = score.chord_at(at);
        let p = key.pitch(d, 4);
        let p = if score.strong(at) && !chord.holds(&key, p) { tune::nearest_chord_tone(&key, chord, p, TUNE.0, peak - 1) } else { p };
        score.add(Note { start: at, len: E - E / 8, pitch: p, vel: vel(-6 + k as i32 / 2, rng), channel: CH_LEAD });
    }
}

/// Whether the ending holds the lead's climb over its last chord: every
/// ending but the cadenza, whose lead is its own.
fn holds_lead(ending: &Ending) -> bool {
    !matches!(ending, Ending::Cadenza)
}

/// The tone the lead holds over the ending, the song's highest: the home
/// chord's highest tone the neck reaches, under what a lifted last chorus
/// raises it.
fn peak(score: &Score, design: &Design) -> u8 {
    *design.home().pitches_within(&score.key, TUNE.1, SOLO.1 - design.lift() as u8).iter().max().expect("a tone of the home chord over the tune")
}

/// The most anything before the climb may reach — the twin, the solos,
/// a half-phrase restated an octave up — so the peak is heard as new.
fn cap(score: &Score, design: &Design) -> u8 {
    peak(score, design) - 3
}

/// The lead's half-phrases restated an octave up that rise past the cap,
/// set back down an octave.
fn under_cap(score: &mut Score, form: &Form) {
    let cap = cap(score, &form.design);
    let half = phrase::BARS / 2 * form.bar;
    let solos = form.solos().map_or((0, 0), |(a, z)| (a * form.bar, z * form.bar));
    let end = form.bars() * form.bar;
    let mut windows: Vec<u32> = score.notes.iter().filter(|n| n.channel == CH_LEAD && n.pitch > cap && n.start < end && !(n.start >= solos.0 && n.start < solos.1)).map(|n| n.start / half).collect();
    windows.sort();
    windows.dedup();
    for w in windows {
        let inside = |n: &Note| n.channel == CH_LEAD && n.start / half == w;
        if score.notes.iter().filter(|n| inside(n)).all(|n| n.pitch >= TUNE.0 - 2 + 12) {
            for n in score.notes.iter_mut().filter(|n| inside(n)) {
                n.pitch -= 12;
            }
        }
    }
}

/// `channel`'s whole line repaired as one, its graces aside.
fn repair(score: &mut Score, form: &Form, channel: u8) {
    let (lo, hi) = (TUNE.0 - 2, SOLO.1);
    let mut at: Vec<usize> = (0..score.notes.len()).filter(|i| score.notes[*i].channel == channel && score.notes[*i].len >= E / 2).collect();
    at.sort_by_key(|i| score.notes[*i].start);
    let mut line: Vec<Placed> = at.iter().map(|i| (score.notes[*i].start, score.notes[*i].len, score.notes[*i].pitch)).collect();
    form.tune.repair(score, &mut line, lo, hi);
    for (i, n) in at.into_iter().zip(line) {
        score.notes[i].pitch = n.2;
    }
}

/// The twin a diatonic third over the lead through `spans`: on a strong
/// beat the chord's tone a third or a sixth over the lead's, never a
/// perfect interval apart; between, the third over in the bar's key; a
/// third under where over leaves the neck.
fn harmonise(score: &mut Score, form: &Form, spans: &[(u32, u32)], rng: &mut Rng) {
    // The doubled last chorus's second pass takes the twin under the lead,
    // a new harmony for the last time through.
    let under_from = form.last_chorus().filter(|_| form.design.doubled).map(|a| (a + form.design.chorus_len * phrase::BARS) * form.bar).unwrap_or(u32::MAX);
    // Under the neck's top by what a lifted last chorus raises it.
    let (lo, hi) = (TUNE.0 - 2, cap(score, &form.design));
    let bar = form.bar;
    let mut line: Vec<(Placed, u8)> = Vec::new();
    for (a, z) in spans {
        let lead: Vec<Note> = score.notes.iter().filter(|n| n.channel == CH_LEAD && n.start >= a * bar && n.start < z * bar && n.len >= E / 2).copied().collect();
        for n in lead {
            let key = score.key_at(n.start);
            let chord = score.chord_at(n.start);
            let d = key.standing_degree(n.pitch);
            let over = |step: i32| key.pitch(d + step, 4);
            let steps: [i32; 4] = if n.start >= under_from { [-2, -5, 2, 5] } else { [2, 5, -2, -5] };
            let pitch = if score.strong(n.start) {
                // A chord tone a third or a sixth from the lead, over it
                // where the neck allows; under the peak, any chord tone off a
                // perfect interval nearest a third over.
                steps
                    .iter()
                    .map(|s| over(*s))
                    .find(|p| chord.holds(&key, *p) && *p <= hi && *p >= lo && !crate::theory::counterpoint::perfect(*p, n.pitch))
                    .or_else(|| chord.pitches_within(&key, lo, hi).into_iter().filter(|p| *p != n.pitch && !crate::theory::counterpoint::perfect(*p, n.pitch)).min_by_key(|p| (*p as i32 - n.pitch as i32 - 4).abs()))
                    .unwrap_or(n.pitch)
            } else if n.start >= under_from || over(2) > hi {
                over(-2)
            } else {
                over(2)
            };
            line.push(((n.start, n.len, pitch), n.vel));
        }
    }
    line.sort_by_key(|(n, _)| n.0);
    let mut placed: Vec<Placed> = line.iter().map(|(n, _)| *n).collect();
    form.tune.repair(score, &mut placed, lo, hi);
    for ((start, len, pitch), (_, v)) in placed.into_iter().zip(line) {
        score.add(Note { start, len, pitch, vel: (v as i32 - 4 + rng.range(-3, 3)).clamp(1, 127) as u8, channel: CH_TWIN });
    }
}

/// How the song opens, laid before the walk on the hook's chords: the
/// twins alone with the chorus's hook, then the band under it; the band's
/// hits, then the band and the twins; a bar of the drive and a fill
/// alone, then the band and the twins; the hook over held chords; the
/// band and the twins from the first bar, the hook twice; or one guitar
/// alone on the riff, then the band and the twins.
fn intro(score: &mut Score, form: &Form, rng: &mut Rng) {
    let bar = form.bar;
    let hook = form.hook();
    let chords: Vec<Chord> = (hook..hook + phrase::BARS).map(|b| form.chord(b)).collect();
    let lead: Vec<Note> = score.notes.iter().filter(|n| matches!(n.channel, CH_LEAD | CH_TWIN) && n.start >= hook * bar && n.start < (hook + phrase::BARS) * bar).copied().collect();
    // (bars, what plays): 0 the twins alone, 1 the hits, 2 the drums, 3 held
    // chords under the hook, 4 the band and the twins, 5 one guitar.
    let segments: Vec<(u32, u8)> = match form.design.intro {
        Intro::Twins => vec![(4, 0), (4, 4)],
        Intro::Hits => vec![(4, 1), (4, 4)],
        Intro::Drums => vec![(2, 2), (4, 4)],
        Intro::Held => vec![(4, 3)],
        Intro::Band => vec![(4, 4), (4, 4)],
        Intro::Alone => vec![(4, 5), (4, 4)],
    };
    let total: u32 = segments.iter().map(|(n, _)| n).sum();
    let opening: Vec<Chord> = segments.iter().flat_map(|(n, _)| (0..*n).map(|k| chords[(k % phrase::BARS) as usize])).collect();
    score.delay(&opening);
    let band = |score: &Score, chord: Chord| -> [u8; 3] { power(&score.key_at(0), chord) };
    let _ = band;
    let mut at = 0u32;
    let mut sections = Vec::new();
    for (n, what) in &segments {
        let (name, level) = match what {
            0 => ("intro, the twins", LEVEL_BARE),
            1 => ("intro, the hits", LEVEL_VERSE),
            2 => ("intro, the drums", LEVEL_BARE + 1.0),
            3 => ("intro, held chords", LEVEL_VERSE - 1.0),
            5 => ("intro, one guitar", LEVEL_BARE),
            _ => ("intro, the band", LEVEL_CHORUS),
        };
        sections.push(ScoreSection { name, start: at * bar, end: (at + n) * bar, trim: 1.0, level: Some(level), rings: false });
        for k in 0..*n {
            let b = at + k;
            let start = b * bar;
            let chord = opening[b as usize];
            let next_chord = opening.get(b as usize + 1).copied().unwrap_or(form.chord(0));
            let into_song = b + 1 == total;
            let role = variation::role(k);
            let next = Some(power(&score.key_at(start + bar), next_chord));
            let place = Place { role, pair: false, style: 0, next };
            if matches!(what, 0 | 3 | 4) {
                for m in &lead {
                    let offset = m.start - hook * bar;
                    if offset / bar == k % phrase::BARS {
                        score.add(Note { start: start + offset % bar, len: m.len.min(bar * (n - k) - offset % bar), ..*m });
                    }
                }
            }
            match what {
                1 => {
                    let tones = power(&score.key_at(start), chord);
                    let band = Band { guitars: &[CH_LEFT, CH_RIGHT], bass: CH_BASS, kit: CH_KIT, chord: tones, root: tones[0] - 12, vel };
                    rock::figure(score, &band, start, Figure::Dotted, rng);
                }
                2 => {
                    if k + 1 < *n {
                        for i in 0..8u32 {
                            score.add(Note { start: start + i * E, len: S, pitch: if i % 2 == 0 { RIDE_BELL } else { RIDE }, vel: vel(20, rng), channel: CH_KIT });
                            if i % 2 == 1 {
                                score.add(Note { start: start + i * E, len: S, pitch: SNARE, vel: vel(6, rng), channel: CH_KIT });
                            }
                        }
                        for i in 0..16u32 {
                            score.add(Note { start: start + i * S, len: S, pitch: KICK, vel: vel(if i % 4 == 0 { -4 } else { -14 }, rng), channel: CH_KIT });
                        }
                        score.add(Note { start, len: 2 * S, pitch: CRASH, vel: vel(30, rng), channel: CH_KIT });
                    } else {
                        rock::fill(score, CH_KIT, form.design.fill, start, 16, (-10, 30), Some(4), vel, rng);
                    }
                }
                3 => {
                    let tones = power(&score.key_at(start), chord);
                    for channel in [CH_LEFT, CH_RIGHT] {
                        for p in tones {
                            score.add(Note { start, len: bar - E / 4, pitch: p, vel: vel(2, rng), channel });
                        }
                    }
                    score.add(Note { start, len: bar - E / 4, pitch: tones[0] - 12, vel: vel(0, rng), channel: CH_BASS });
                    score.add(Note { start, len: 2 * S, pitch: CRASH, vel: vel(30, rng), channel: CH_KIT });
                    score.add(Note { start, len: S, pitch: KICK, vel: vel(-2, rng), channel: CH_KIT });
                    if into_song {
                        rock::fill(score, CH_KIT, form.design.fill, start + 8 * S, 8, (-6, 30), Some(4), vel, rng);
                    }
                }
                5 => {
                    riff_bar(score, CH_LEFT, start, chord, form.design.riff, place, Voice::Root, &form.design, rng);
                    if k + 1 == *n {
                        rock::fill(score, CH_KIT, form.design.fill, start + 12 * S, 4, (-8, 30), Some(4), vel, rng);
                    }
                }
                _ => {
                    for channel in [CH_LEFT, CH_RIGHT] {
                        riff_bar(score, channel, start, chord, form.design.riff, place, Voice::Root, &form.design, rng);
                    }
                    bass_bar(score, start, chord, Some(next_chord), role, &EIGHTHS, rng);
                    for i in 0..8u32 {
                        score.add(Note { start: start + i * E, len: S, pitch: HAT, vel: vel(if i % 2 == 0 { 22 } else { 12 }, rng), channel: CH_KIT });
                        score.add(Note { start: start + i * E, len: S, pitch: if i % 2 == 0 { KICK } else { SNARE }, vel: vel(if i % 2 == 0 { -6 } else { 8 }, rng), channel: CH_KIT });
                    }
                    if k == 0 {
                        score.add(Note { start, len: 2 * S, pitch: CRASH, vel: vel(32, rng), channel: CH_KIT });
                    }
                    if role == Bar::Cadence {
                        rock::fill(score, CH_KIT, form.design.fill, start + 12 * S, 4, (-4, 30), Some(4), vel, rng);
                    }
                }
            }
        }
        at += n;
    }
    for s in sections.into_iter().rev() {
        score.sections.insert(0, s);
    }
}

/// The song's ending, after the walk's last part, on the chorus's home
/// chord. The ritual slows into it where it has it, holds the chord under
/// the wash and closes on its separate last hit (`rock::ritual`), the
/// lead holding the song's highest tone over it where it does. The hits
/// strike the band's figure through a bar, then one stroke. The stab is
/// one stroke on the bar line, short or rung, the guitars sliding off it
/// now and then. The false ending stops dead for a bar and comes back for
/// a short ritual. The cadenza is the lead alone through a bar, falling to
/// the stab. The ring strikes the chord and lets it ring under the wash,
/// no last hit. Every ending rings out with the room.
fn ending(score: &mut Score, form: &Form, rng: &mut Rng) {
    let bar = form.bar;
    let at = form.bars();
    let home = form.design.home();
    score.mark_coda(at);
    let key = score.key_at((at - 1) * bar).under(home);
    let tones = power(&key, home);
    let band = Band { guitars: &[CH_LEFT, CH_RIGHT], bass: CH_BASS, kit: CH_KIT, chord: tones, root: tones[0] - 12, vel };
    let section = |score: &mut Score, name: &'static str, from: u32, bars: u32, level: Option<f32>, rings: bool| {
        score.sections.push(ScoreSection { name, start: from * bar, end: (from + bars) * bar, trim: 1.0, level, rings });
        score.harmony.extend((0..bars).map(|_| home));
    };
    // The lead holds the song's highest tone from the ending's first beat
    // through its last stroke and on with its ring.
    let lead_over = |score: &mut Score, from: u32, to: u32, rng: &mut Rng| {
        let peak = peak(score, &form.design);
        score.add(Note { start: from, len: to - from - E / 4, pitch: peak, vel: vel(6, rng), channel: CH_LEAD });
    };
    match form.design.ending {
        Ending::Ritual(r, slows) => {
            if slows {
                let slowest = 0.4 + 0.3 * rng.f32();
                score.ritardando((at - 2) * bar, at * bar, slowest);
            }
            section(score, "end", at, r.hold + RING_BARS, Some(LEVEL_END), true);
            let ring = (at + r.hold + RING_BARS) * bar;
            let cut = rock::ritual(score, &band, at * bar, Ritual { ring: 0, ..r }, rng);
            // The last hit rings out with the room.
            for channel in [CH_LEFT, CH_RIGHT] {
                for p in tones {
                    score.add(Note { start: cut + E, len: ring - cut - 2 * E, pitch: p, vel: vel(-30, rng), channel });
                }
            }
            lead_over(score, at * bar, ring, rng);
        }
        Ending::Hits(figure) => {
            section(score, "hits", at, 1, Some(LEVEL_SOLO), false);
            rock::figure(score, &band, at * bar, figure, rng);
            section(score, "end", at + 1, RING_BARS, None, true);
            rock::stab(score, &band, (at + 1) * bar, 2 * E, false, rng);
            lead_over(score, at * bar, (at + 1 + RING_BARS) * bar, rng);
        }
        Ending::Stab(long, slides) => {
            section(score, "end", at, RING_BARS, None, true);
            let len = if long { RING_BARS * bar - E } else { E };
            rock::stab(score, &band, at * bar, len, slides && !long, rng);
            lead_over(score, at * bar, (at + RING_BARS) * bar, rng);
        }
        Ending::False => {
            section(score, "false end", at, 1, None, false);
            section(score, "end", at + 1, 1 + RING_BARS, Some(LEVEL_END), true);
            rock::ritual(score, &band, (at + 1) * bar, Ritual { hold: 1, early: false, kick_runs: false, ring: RING_BARS * bar - E }, rng);
            lead_over(score, (at + 1) * bar, (at + 2 + RING_BARS) * bar, rng);
        }
        Ending::Cadenza => {
            section(score, "cadenza", at, 1, None, false);
            // A run down from the peak through the bar in sextuplets, to the
            // home chord's root.
            let peak = peak(score, &form.design);
            let from = key.standing_degree(peak);
            let n = 24u32;
            for (k, d) in std::iter::once(from).chain(run_between(from, from - 7, n)).enumerate() {
                        // Turning about under the peak, never over it.
                let p = key.pitch(d.min(from), 4);
                let p = if k % 6 == 0 && !home.holds(&key, p) { tune::nearest_chord_tone(&key, home, p, TUNE.0 - 2, SOLO.1) } else { p };
                score.add(Note { start: at * bar + k as u32 * (bar / n), len: bar / n - 5, pitch: p, vel: vel(-2 - (k as i32) / 4, rng), channel: CH_LEAD });
            }
            section(score, "end", at + 1, RING_BARS, None, true);
            rock::stab(score, &band, (at + 1) * bar, 3 * E, false, rng);
        }
        Ending::Ring => {
            section(score, "end", at, 1 + RING_BARS, Some(LEVEL_END), true);
            band.stroke(score, at * bar, (1 + RING_BARS) * bar - E, 4, rng);
            // The wash swells through the first bar and dies away through
            // the ring, a chord let go rather than cut.
            let n = 16 * (1 + RING_BARS) - 8;
            for k in 4..n {
                let x = (k - 4) as f32 / (n - 4) as f32;
                let swell = 1.0 - (2.0 * x - 0.6).abs().min(1.0);
                score.add(Note { start: at * bar + k * S, len: S - 10, pitch: CRASH_2, vel: vel(-34 + (30.0 * swell) as i32, rng), channel: CH_KIT });
            }
            lead_over(score, at * bar, (at + 1 + RING_BARS) * bar - E, rng);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::score::Mark;

    /// The story is a ladder whose top is the solos, with a chorus under
    /// it, a pre-chorus under that and a verse at its foot.
    #[test]
    fn the_song_is_a_ladder_to_the_solos() {
        assert_eq!(STORY.fault(), None);
        assert_eq!(STORY.bed(SOLOS as usize).section(), Section::Solo);
        assert_eq!(STORY.bed(CHORUS as usize).section(), Section::Chorus);
        assert_eq!(STORY.bed(PRE as usize).section(), Section::Pre);
        assert_eq!(STORY.base.section(), Section::Verse);
    }

    /// The song opens with its intro, is whole phrases, has its solo block
    /// and a last chorus after it — twice the first where it is doubled —
    /// and ends ringing; its coda is marked and nothing struck ends past
    /// the end.
    #[test]
    fn a_song_is_whole_phrases_with_its_solos_and_an_ending() {
        for seed in 0..32 {
            let (score, form) = compose(&Params::of(crate::pieces::find("speed-metal").unwrap(), seed));
            assert_eq!(form.bars() % phrase::BARS, 0, "seed {seed}");
            let choruses: Vec<u32> = form
                .walk
                .parts
                .iter()
                .zip(&form.sections)
                .fold(Vec::<(u32, bool)>::new(), |mut runs, (p, s)| {
                    let chorus = *s == Section::Chorus;
                    match runs.last_mut() {
                        Some((n, c)) if *c == chorus => *n += p.b - p.a,
                        _ => runs.push((p.b - p.a, chorus)),
                    }
                    runs
                })
                .into_iter()
                .filter(|(_, c)| *c)
                .map(|(n, _)| n)
                .collect();
            let last = *choruses.last().unwrap();
            assert_eq!(last, if form.design.doubled { 2 * choruses[0] } else { choruses[0] }, "seed {seed}: choruses {choruses:?}");
            assert!(score.sections[0].name.starts_with("intro"), "seed {seed}");
            assert!(score.sections.last().unwrap().rings, "seed {seed}");
            assert!(form.solos().is_some(), "seed {seed}");
            assert!(form.sections.iter().rposition(|s| *s == Section::Chorus).unwrap() > form.sections.iter().position(|s| *s == Section::Solo).unwrap(), "seed {seed}: no chorus after the solos");
            assert_eq!(score.marked(Mark::Coda).count(), 1);
            let end = score.end();
            for n in &score.notes {
                let role = score.instrument(n.channel).role;
                assert!(n.end() <= end || matches!(role, Role::Sustain), "seed {seed}: {} past the end", score.instrument(n.channel).name);
            }
        }
    }

    /// Every song splits its guitars somewhere, and where it does the
    /// right goes its own way.
    #[test]
    fn every_song_splits_its_guitars_somewhere() {
        for seed in 0..32 {
            let (score, form) = compose(&Params::of(crate::pieces::find("speed-metal").unwrap(), seed));
            assert!(form.splits.iter().any(|s| *s), "seed {seed}: never splits");
            let side = |ch: u8| -> Vec<(u32, u8)> { score.notes.iter().filter(|n| n.channel == ch).map(|n| (n.start, n.pitch)).collect() };
            let (l, r) = (side(CH_LEFT), side(CH_RIGHT));
            assert!(r.iter().any(|n| !l.contains(n)), "seed {seed}: the right never leaves the left");
        }
    }

    /// Every family, event, intro and ending comes up, and the songs differ
    /// in more than their notes.
    #[test]
    fn the_songs_are_their_own() {
        let (mut families, mut events, mut intros, mut endings) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for seed in 0..160 {
            let (_, form) = compose(&Params::of(crate::pieces::find("speed-metal").unwrap(), seed));
            families.push(form.design.family);
            events.push(form.design.event);
            intros.push(form.design.intro);
            endings.push(form.design.ending.name());
        }
        for f in [Family::Doubled, Family::Single, Family::Gallop, Family::Backbeat, Family::HalfSpeed] {
            assert!(families.contains(&f), "{f:?}");
        }
        for e in [Event::Break, Event::DeadBar, Event::FeelSwitch, Event::HalfTime, Event::DrumBars, Event::Lift] {
            assert!(events.contains(&e), "{e:?}");
        }
        for i in [Intro::Twins, Intro::Hits, Intro::Drums, Intro::Held, Intro::Band, Intro::Alone] {
            assert!(intros.contains(&i), "{i:?}");
        }
        endings.sort();
        endings.dedup();
        assert_eq!(endings.len(), 6, "{endings:?}");
    }
}
