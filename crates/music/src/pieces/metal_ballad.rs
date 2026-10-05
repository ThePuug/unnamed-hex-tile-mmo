//! A metal ballad: a clean guitar's arpeggio that the band builds on a
//! layer at a time — the bass, the kit, the distorted guitars palm-muted
//! and then ringing open, the strings — to a solo at its crest, and back
//! down to the arpeggio it opened on. What the band plays makes the part
//! of the song: with the guitars open it is a chorus, under the solo the
//! climax, else a verse; each has its own harmony, the verse holding the
//! tonic, the chorus opening off it and coming home by the sixth and
//! seventh or through the major V, borrowed from the harmonic minor, the
//! climax a progression heard nowhere else, and the tune sits a third
//! higher in a chorus. Every part reads its bar's key, so over the major
//! V the arpeggio, the bass's approach and the lead take the raised
//! seventh. No part plays one bar over and over:
//! every four-bar phrase is a bar, its variant and a cadence, and the
//! kit fills where a phrase or a part turns. The guitars are
//! double-tracked, one each side; one lead, a guitar or a violin, sings
//! the theme where a voice would. The solo is the seed's: a guitar that
//! states the theme, answers itself, climbs in sequences to a peak it
//! has not touched before and lands on the theme with a second guitar a
//! third over it; or the kit over the band's stop-time hits, its motif
//! growing and turning until its signature fill brings the band back.
//! Every choice is a draw from the seed's stream, one fork per purpose;
//! the seed picks which shape at every level, never the next note.

use crate::ladder::{self, turn, Bed, Story, Walk};
use crate::pieces::Params;
use crate::rng::Rng;
use crate::score::{Instrument, Note, Role, Score, Section as ScoreSection, TICKS_PER_EIGHTH as E};
use crate::teller::{self, Hold, Teller, Telling};
use crate::theory::groove::{Groove, BALLAD};
use crate::theory::melody::{run_between, Shape, Theme};
use crate::theory::phrase::{self, Form as PhraseForm, FORMS};
use crate::theory::schema::{split, Schema, BALLAD as SONG};
use crate::theory::{Chord, Key, Mode};
use crate::tune::{self, Placed, Tune};
use crate::variation::{self, Role as Bar};

/// General MIDI programs, 0-based.
const PIANO: u8 = 0;
const CLEAN_GUITAR: u8 = 27;
const OVERDRIVEN: u8 = 29;
const DISTORTION: u8 = 30;
/// The band's bass, fingered: the picked bass's recordings start over
/// the low E. The bass's solo is picked, which cuts where the fingered
/// thins, and stays over its lowest recording, the C sharp at 37.
const BASS: u8 = 33;
const SOLO_BASS: u8 = 34;
const SOLO_BASS_LOW: u8 = 37;
const VIOLIN: u8 = 40;
const SLOW_STRINGS: u8 = 49;
const CHOIR_AAHS: u8 = 52;
/// The power kit, the bank's rock kit, and its keys.
const ROCK_KIT: u8 = 16;
const KICK: u8 = 36;
const SIDE_STICK: u8 = 37;
const SNARE: u8 = 38;
const HAT: u8 = 42;
const OPEN_HAT: u8 = 46;
const CRASH: u8 = 49;
const RIDE: u8 = 51;
const CRASH_2: u8 = 57;
const PEDAL_HAT: u8 = 44;
const CHINA: u8 = 52;
const RIDE_BELL: u8 = 53;
const SPLASH: u8 = 55;
/// The toms, high to low, the floor's two last.
const TOMS: [u8; 6] = [50, 48, 47, 45, 43, 41];

const CH_BASS: u8 = 0;
const CH_CLEAN: u8 = 1;
const CH_LEAD: u8 = 2;
const CH_LEFT: u8 = 3;
const CH_RIGHT: u8 = 4;
const CH_SECOND: u8 = 5;
const CH_PAD: u8 = 6;
const CH_CHOIR: u8 = 7;
const CH_SOLO: u8 = 8;
const CH_KIT: u8 = 9;
const CH_HARMONY: u8 = 10;

/// How long the room rings: a hall, the ballad's size, under the
/// overworld's stone.
const ROOM_S: f32 = 2.2;

/// Who may sing the theme: the lead guitar, overdriven or distorted, or
/// a violin. The pool's files each have their own.
const LEADS: [u8; 3] = [OVERDRIVEN, DISTORTION, VIOLIN];

/// The lead's level, dB, forward of the band whichever it is: a guitar
/// lead among the double-tracked guitars, one of them its own program,
/// is lost at any less.
const LEAD: f32 = 3.0;

/// The band's level, dB: under the lead together, its own balance kept.
const BAND: f32 = -2.0;

/// The kit's level, dB: its most struck drum's, under the band's by
/// what holds the kit where the band's balance has it.
const KIT: f32 = BAND - 1.5;

/// The rhythm guitars' level, dB: two of them chugging every beat
/// outweigh a kit striking its strokes, so the pair sits with the kit.
const GUITARS: f32 = BAND - 6.0;

/// The soloist's level, dB: a solo is the song's peak and stands out of
/// the band.
const SOLO_LEVEL: f32 = 4.0;

/// The arpeggio's level, dB, over the band's: alone it is the intro and
/// the outro, and under the guitars' chug a picked chord is lost.
const ARPEGGIO: f32 = 4.0;

/// The velocity every voice strikes at before its own accent.
const VEL: i32 = 88;

/// The guitars' stab under the kit's solo, against `VEL`: two distorted
/// guitars struck at the band's stroke outweigh the whole kit, which is
/// the song's lead there.
const STAB: i32 = -34;

/// The shapes a ballad's tune sings in: the sigh down to home and the
/// wave a verse sings low, the arch and the leap a chorus sings a third
/// over it — never the circling or the climb, a folk tune's and a
/// pre-chorus's.
const BALLAD_SHAPES: [Shape; 4] = [Shape::Descent, Shape::Wave, Shape::Arch, Shape::LeapBack];

/// Where each part of the song sits, LU against the solo, its loudest:
/// the arpeggio alone it opens and closes on, a verse with every layer
/// in, a chorus; a verse with fewer sits between the foot and the full
/// verse by the layers it has, so the band grows as it joins. The arc
/// is declared and the render meets it, so it holds whatever bank plays
/// the band — a balance fitted to one bank's samples is wrong on the
/// next — and a ballad is not afraid of its loud parts.
const LEVEL_FOOT: f32 = -6.0;
const LEVEL_VERSE: f32 = -3.5;
const LEVEL_CHORUS: f32 = -1.5;
const LEVEL_SOLO: f32 = 0.0;
/// How far under its last part a solo's first sits, LU: the solo builds
/// part by part to its loudest, where a level met part by part would
/// pull each back down and a kit's rising strokes play as a sawtooth.
const SOLO_RISE: f32 = 3.0;

/// The tune's register; the lead takes it here.
const TUNE: (u8, u8) = (60, 88);
/// The guitar's solo's register, over the tune's, up to a 24-fret
/// guitar's top E; the bass's, the two octaves over the band's bass
/// from E2, where a bass still sounds like one.
const SOLO: (u8, u8) = (62, 88);
const BASS_SOLO: (u8, u8) = (40, 64);
/// The sequence a verse's phrase pair takes, in degrees: the theme, a
/// step up.
const PAIRS: [i32; 2] = [0, 1];
/// The tune a third up in a chorus: a chorus sits higher than its verse.
const CLIMB: i32 = 2;

/// A clean guitar's or a piano's picked chord.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Clean {
    Off,
    /// A tone on every beat: the root, then up through the chord.
    Thin,
    /// A tone on every eighth, in the seed's picking pattern.
    Full,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Bass {
    Off,
    /// The root on the kick's strokes, held to the next.
    Held,
    /// The root on every eighth, its octave now and then.
    Driving,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kit {
    Off,
    /// A verse's time: the closed hat on every eighth, the kick, the
    /// side stick on the backbeat, ghost strokes on the snare.
    Time,
    /// A chorus's: the ride, the snare's backbeat or the half-time's,
    /// the crash at every part.
    Full,
}

/// The distorted guitars, double-tracked, one each side.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Rhythm {
    Off,
    /// A verse's: root and fifth palm-muted on the eighths, the next
    /// chord struck open on the bar's last eighth and rung over the line.
    Muted,
    /// A chorus's: power chords struck on the kick and let ring.
    Open,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Pad {
    Off,
    /// The slow strings on root and fifth.
    Thin,
    /// The strings on the chord, voice-led, and the choir on root and
    /// fifth.
    Full,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Solo {
    Off,
    On,
}

/// Who takes the solo, the seed's.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Soloist {
    Guitar,
    Drums,
    Bass,
}

impl Soloist {
    fn name(self) -> &'static str {
        match self {
            Soloist::Guitar => "guitar solo",
            Soloist::Drums => "drum solo",
            Soloist::Bass => "bass solo",
        }
    }
}

/// What part of the song a bar is, by what plays.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Section {
    Verse,
    Chorus,
    Climax,
}

/// A layer of the band, the thing a rung of the ladder moves.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Layer {
    Clean,
    Bass,
    Kit,
    Rhythm,
    Pad,
    Solo,
}

/// What the band is: each layer at its notch.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Texture {
    clean: Clean,
    bass: Bass,
    kit: Kit,
    rhythm: Rhythm,
    pad: Pad,
    solo: Solo,
}

impl Texture {
    const BARE: Texture = Texture { clean: Clean::Off, bass: Bass::Off, kit: Kit::Off, rhythm: Rhythm::Off, pad: Pad::Off, solo: Solo::Off };

    /// The part of the song this band makes: the solo's is the climax,
    /// the guitars ringing open a chorus, anything less a verse.
    fn section(&self) -> Section {
        if self.solo == Solo::On {
            Section::Climax
        } else if self.rhythm == Rhythm::Open {
            Section::Chorus
        } else {
            Section::Verse
        }
    }
}

impl Bed for Texture {
    type Layer = Layer;

    fn up(self, layer: Layer) -> Texture {
        match layer {
            Layer::Clean => Texture { clean: if self.clean == Clean::Off { Clean::Thin } else { Clean::Full }, ..self },
            Layer::Bass => Texture { bass: if self.bass == Bass::Off { Bass::Held } else { Bass::Driving }, ..self },
            Layer::Kit => Texture { kit: if self.kit == Kit::Off { Kit::Time } else { Kit::Full }, ..self },
            Layer::Rhythm => Texture { rhythm: if self.rhythm == Rhythm::Off { Rhythm::Muted } else { Rhythm::Open }, ..self },
            Layer::Pad => Texture { pad: if self.pad == Pad::Off { Pad::Thin } else { Pad::Full }, ..self },
            Layer::Solo => Texture { solo: Solo::On, ..self },
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
        if from.clean != to.clean {
            moved(from.clean as u8, to.clean as u8, ["arpeggio in", "arpeggio up", "arpeggio down", "arpeggio out"])
        } else if from.bass != to.bass {
            moved(from.bass as u8, to.bass as u8, ["bass in", "bass up", "bass down", "bass out"])
        } else if from.kit != to.kit {
            moved(from.kit as u8, to.kit as u8, ["kit in", "kit up", "kit down", "kit out"])
        } else if from.rhythm != to.rhythm {
            moved(from.rhythm as u8, to.rhythm as u8, ["guitars in", "chorus", "verse", "guitars out"])
        } else if from.pad != to.pad {
            moved(from.pad as u8, to.pad as u8, ["strings in", "strings up", "strings down", "strings out"])
        } else if from.solo != to.solo {
            if to.solo == Solo::On { "solo" } else { "solo out" }
        } else {
            "held"
        }
    }
}

use Layer::{Bass as BassLayer, Clean as CleanLayer, Kit as KitLayer, Pad as PadLayer, Rhythm as RhythmLayer, Solo as SoloLayer};
use Telling::{Long, Off, Phrases, Riff};

/// Every story is told in two-bar parts, a layer moving at every
/// half-phrase's cadence, so the song is dense rather than long. It
/// opens on its foot two parts, four bars, the lead stating the theme
/// over the arpeggio; climbs to the solo, held four parts at the crest,
/// eight bars, a solo's arc without its outstaying the song; and comes
/// down a rung to the chorus, held two parts, the final chorus the
/// ending lands out of. The
/// guitars open its choruses; the lead states the theme over the
/// arpeggio it opens on, plays it as a riff over the muted guitars,
/// holds long tones in the part before a chorus — the breath a
/// pre-chorus takes — is silent under the solo and sings the chorus
/// after it.
const STORIES: [Story<Texture, Telling>; 4] = [
    // The arpeggio, the band joining under the tune to a first chorus,
    // back to the verse, and up again to the solo.
    Story {
        name: "power ballad",
        weight: 3.0,
        base: Texture { clean: Clean::Full, ..Texture::BARE },
        ladder: &[BassLayer, KitLayer, RhythmLayer, PadLayer, RhythmLayer, KitLayer, SoloLayer],
        leads: &[Phrases, Phrases, Phrases, Riff, Long, Phrases, Phrases, Off],
        turns: &[turn((0, 0), (1, 1)), turn((6, 6), (0, 0)), turn((4, 4), (0, 0)), turn((7, 7), (3, 3)), turn((6, 6), (1, 1))],
        halves: (1, 1),
        pace: (0.2, 0.8),
    },
    // One long climb from the arpeggio and the strings to the solo, and
    // down.
    Story {
        name: "slow burn",
        weight: 2.0,
        base: Texture { clean: Clean::Full, ..Texture::BARE },
        ladder: &[PadLayer, BassLayer, KitLayer, RhythmLayer, BassLayer, RhythmLayer, KitLayer, SoloLayer],
        leads: &[Phrases, Phrases, Phrases, Phrases, Riff, Long, Phrases, Phrases, Off],
        turns: &[turn((0, 0), (1, 1)), turn((8, 8), (3, 3)), turn((7, 7), (1, 1))],
        halves: (1, 1),
        pace: (0.0, 0.5),
    },
    // The band in from the first bar under the tune, a chorus, a dip back
    // to the muted verse, and the strings and choir building it to an
    // anthem.
    Story {
        name: "anthem",
        weight: 2.0,
        base: Texture { clean: Clean::Full, bass: Bass::Held, kit: Kit::Time, ..Texture::BARE },
        ladder: &[RhythmLayer, PadLayer, RhythmLayer, BassLayer, PadLayer, KitLayer, SoloLayer],
        leads: &[Phrases, Riff, Long, Phrases, Phrases, Phrases, Phrases, Off],
        turns: &[turn((0, 0), (1, 1)), turn((5, 5), (0, 0)), turn((2, 2), (0, 0)), turn((7, 7), (3, 3)), turn((6, 6), (1, 1))],
        halves: (1, 1),
        pace: (0.4, 1.0),
    },
    // Opened on the strings alone under the lead singing the theme, the
    // arpeggio and the band gathering under it.
    Story {
        name: "requiem",
        weight: 1.0,
        base: Texture { pad: Pad::Thin, ..Texture::BARE },
        ladder: &[CleanLayer, BassLayer, KitLayer, RhythmLayer, RhythmLayer, PadLayer, KitLayer, SoloLayer],
        leads: &[Phrases, Phrases, Phrases, Phrases, Riff, Phrases, Phrases, Phrases, Off],
        turns: &[turn((0, 0), (1, 1)), turn((8, 8), (3, 3)), turn((7, 7), (1, 1))],
        halves: (1, 1),
        pace: (0.0, 0.6),
    },
];

/// The arpeggio's picking patterns, in chord tones counted up from the
/// root: 0 the root, 1 its third, 2 its fifth, 3 the octave, 4 the
/// tenth, 5 the twelfth. Its highest step is where a drone takes over.
const PICKING: [&[usize]; 3] = [&[0, 2, 3, 4, 3, 2], &[0, 2, 3, 2, 4, 2], &[0, 3, 2, 4, 3, 5]];

/// The seed's fill, the one it opens every chorus with and closes the
/// kit's solo on: the toms down; the snare and then the toms; each drum
/// twice down the kit; or two toms and the kick by turns, the triplet a
/// twelve-eight rolls in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Fill {
    Descent,
    SnareThenToms,
    Pairs,
    HandHandKick,
}

/// A drum solo's motif in a beat of two eighths and of three: sixteenths
/// struck within the beat, and the tom each falls on.
const MOTIFS_TWO: [&[(u32, usize)]; 3] = [&[(0, 0), (3, 2)], &[(0, 0), (1, 1), (3, 3)], &[(0, 1), (2, 1), (3, 5)]];
const MOTIFS_THREE: [&[(u32, usize)]; 3] = [&[(0, 0), (3, 2), (5, 5)], &[(0, 1), (2, 2), (4, 3)], &[(0, 0), (1, 0), (3, 4)]];

/// What a seed's piece is.
struct Design {
    /// The one player who tells the tune, and the one who holds under
    /// its riff.
    lead: u8,
    second: u8,
    /// The arpeggio's instrument and pattern, and the degree of the key
    /// its top rings on whatever the chord, where it drones.
    clean: u8,
    picking: &'static [usize],
    drone: Option<i32>,
    soloist: Soloist,
    /// Whether the guitar's climax is a held tone, else a run to its peak.
    held_climax: bool,
    /// Whether the chorus is in half-time, the snare on the bar's middle.
    half_time: bool,
    fill: Fill,
    motif: &'static [(u32, usize)],
    groove: &'static Groove,
    form: PhraseForm,
    theme: Theme,
    /// Each part of the song's question and answer.
    verse: [&'static Schema; 2],
    chorus: [&'static Schema; 2],
    climax: [&'static Schema; 2],
    ending: Ending,
}

/// How the song ends, out of its final chorus.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Ending {
    /// The band slows into one hit on the tonic, held under a cymbal's
    /// swell and cut with a last crash, the room ringing after.
    Big,
    /// The band falls away to the clean arpeggio alone, slowing over the
    /// tonic, its last chord left to ring.
    Arpeggio,
}

/// The ending's level against the solo, over its hit and its ring: a
/// ring falling away through the section puts the hit some four LU over
/// its mean, so the hit lands at the chorus's height.
const LEVEL_END: f32 = LEVEL_CHORUS - 4.0;

/// The bars the ending's hit is held before the cut, the bars the room
/// rings after it, and the arpeggio coda's bars before its last chord.
const HELD_BARS: u32 = 1;
const RING_BARS: u32 = 2;
const CODA_BARS: u32 = 2;

/// How far the song slows into its last chord: the tempo there, against
/// the song's.
const SLOWEST: f32 = 0.8;

/// The band's velocity with an accent and a little jitter so no two
/// notes strike alike.
fn vel(accent: i32, rng: &mut Rng) -> u8 {
    (VEL + accent + rng.range(-4, 4)).clamp(1, 127) as u8
}

/// The ornament a guitar's line takes into a note: a hammer from the
/// degree under it.
fn hammer(key: &Key, pitch: u8, _hi: u8) -> Option<u8> {
    Some(key.pitch(key.absolute_degree(pitch).unwrap() - 1, 4))
}

fn variant(bar: u32) -> bool {
    variation::role(bar) == Bar::Variant
}
fn cadence(bar: u32) -> bool {
    variation::role(bar) == Bar::Cadence
}

/// The stages of a melodic solo over its bars: the theme stated, the
/// theme again woven with sixteenths, a call and answer, the build, one
/// bar of climax, the release and the landing — the bars each opens on.
struct Stages {
    woven: u32,
    call: u32,
    build: u32,
    climax: u32,
    landing: u32,
}

fn stages(a: u32, z: u32) -> Stages {
    let n = z - a;
    let at = |f: f32| a + ((n as f32 * f).round() as u32).min(n - 1);
    let woven = at(0.12).max(a + 1);
    let call = at(0.3).max(woven + 1);
    let build = at(0.45).max(call + 1);
    let climax = at(0.62).max(build + 1).min(z - 3);
    let landing = at(0.85).max(climax + 2).min(z - 1);
    Stages { woven, call, build, climax, landing }
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
    /// Whether `bar` is the walk's last, after which the ending strikes
    /// its own chord, so nothing struck there leans into the next bar.
    fn last(&self, bar: u32) -> bool {
        bar + 1 == self.bars()
    }
    /// Whether the bass solos at `bar`: the band under it keeps out of its
    /// register.
    fn bass_solo(&self, bar: u32) -> bool {
        self.texture_at(bar).solo == Solo::On && self.design.soloist == Soloist::Bass
    }
    /// Whether the band plays stop-time at `bar`: under the kit's solo it
    /// strikes the bar's first beat and leaves the rest to the drums.
    fn stop_time(&self, bar: u32) -> bool {
        self.texture_at(bar).solo == Solo::On && self.design.soloist == Soloist::Drums
    }
    /// Whether a part begins at `bar`.
    fn part_starts(&self, bar: u32) -> bool {
        self.walk.parts.iter().any(|p| p.a == bar)
    }
    /// Whether the part of the song changes at `bar`, from the bar before.
    fn turns_at(&self, bar: u32) -> bool {
        bar > 0 && bar < self.bars() && self.texture_at(bar).section() != self.texture_at(bar - 1).section()
    }
    /// Where the band drops out at `bar`, the eighth it falls silent from:
    /// the last beat before a melodic solo's climax, so the peak lands out
    /// of silence.
    fn drop_from(&self, bar: u32, last_beat: u32) -> Option<u32> {
        (self.design.soloist != Soloist::Drums && self.solos().iter().any(|(a, z)| stages(*a, *z).climax == bar + 1)).then_some(last_beat)
    }
    /// The bars the solo plays, each a run of the walk's.
    fn solos(&self) -> Vec<(u32, u32)> {
        let mut out: Vec<(u32, u32)> = Vec::new();
        for p in self.walk.parts.iter().filter(|p| p.bed.solo == Solo::On) {
            match out.last_mut() {
                Some(last) if last.1 == p.a => last.1 = p.b,
                _ => out.push((p.a, p.b)),
            }
        }
        out
    }
}

pub fn build(params: &Params) -> Score {
    compose(params).0
}

/// The score, and the form it was written on.
fn compose(params: &Params) -> (Score, Form) {
    let rng = Rng::new(params.seed);
    let mut skeleton = rng.fork(0);
    let story = ladder::draw(&STORIES, &mut skeleton);
    let key = Key::new(["E", "A", "D", "B", "G", "C"][skeleton.weighted(&[3.0, 3.0, 2.0, 2.0, 1.0, 1.0])], Mode::Aeolian);
    let groove = &BALLAD[skeleton.weighted(&[3.0, 2.0])];
    let tempo = story.tempo(groove.tempo, &mut skeleton);
    let lead = LEADS[skeleton.below(LEADS.len())];
    let pair = |schemata: &'static [Schema], rng: &mut Rng| {
        let (open, closed) = split(schemata);
        [open[rng.below(open.len())], closed[rng.below(closed.len())]]
    };
    let design = Design {
        lead,
        second: if lead == OVERDRIVEN { DISTORTION } else { OVERDRIVEN },
        clean: [CLEAN_GUITAR, PIANO][skeleton.weighted(&[3.0, 1.0])],
        picking: PICKING[skeleton.below(PICKING.len())],
        drone: [None, Some(0), Some(4)][skeleton.weighted(&[2.0, 2.0, 1.0])],
        soloist: [Soloist::Guitar, Soloist::Drums, Soloist::Bass][skeleton.weighted(&[3.0, 2.0, 2.0])],
        held_climax: skeleton.chance(0.6),
        half_time: skeleton.chance(0.35),
        fill: [Fill::Descent, Fill::SnareThenToms, Fill::Pairs, Fill::HandHandKick][skeleton.weighted(&[2.0, 2.0, 1.0, if groove.groups[0] == 3 { 2.0 } else { 0.5 }])],
        motif: if groove.groups[0] == 3 { MOTIFS_THREE[skeleton.below(MOTIFS_THREE.len())] } else { MOTIFS_TWO[skeleton.below(MOTIFS_TWO.len())] },
        groove,
        form: FORMS[skeleton.below(FORMS.len())],
        theme: Theme::draw(groove, &BALLAD_SHAPES, &mut skeleton),
        verse: pair(SONG.verse, &mut skeleton),
        chorus: pair(SONG.chorus, &mut skeleton),
        climax: pair(SONG.climax, &mut skeleton),
        ending: [Ending::Big, Ending::Arpeggio][skeleton.weighted(&[3.0, 2.0])],
    };
    let instruments = vec![
        Instrument { name: "bass", program: BASS, channel: CH_BASS, role: Role::Pluck, low: 28, high: 52, reverb: 15, pan: 0, level: BAND },
        Instrument { name: "arpeggio", program: design.clean, channel: CH_CLEAN, role: Role::Pluck, low: 40, high: 79, reverb: 55, pan: 18, level: ARPEGGIO },
        Instrument { name: "lead", program: design.lead, channel: CH_LEAD, role: Role::Melody, low: 55, high: 91, reverb: 45, pan: 0, level: LEAD },
        Instrument { name: "guitar, left", program: DISTORTION, channel: CH_LEFT, role: Role::Pluck, low: 38, high: 76, reverb: 25, pan: -58, level: GUITARS },
        Instrument { name: "guitar, right", program: OVERDRIVEN, channel: CH_RIGHT, role: Role::Pluck, low: 38, high: 76, reverb: 25, pan: 58, level: GUITARS },
        Instrument { name: "guitar, the second", program: design.second, channel: CH_SECOND, role: Role::Melody, low: 50, high: 72, reverb: 45, pan: -30, level: BAND },
        Instrument { name: "strings", program: SLOW_STRINGS, channel: CH_PAD, role: Role::Sustain, low: 52, high: 79, reverb: 75, pan: -20, level: BAND },
        Instrument { name: "choir", program: CHOIR_AAHS, channel: CH_CHOIR, role: Role::Sustain, low: 52, high: 72, reverb: 85, pan: 26, level: BAND },
        match design.soloist {
            Soloist::Bass => Instrument { name: "solo bass", program: SOLO_BASS, channel: CH_SOLO, role: Role::Melody, low: SOLO_BASS_LOW, high: BASS_SOLO.1, reverb: 35, pan: 0, level: SOLO_LEVEL },
            _ => Instrument { name: "solo guitar", program: DISTORTION, channel: CH_SOLO, role: Role::Melody, low: SOLO.0 - 4, high: SOLO.1, reverb: 40, pan: 12, level: SOLO_LEVEL },
        },
        Instrument { name: "kit", program: ROCK_KIT, channel: CH_KIT, role: Role::Percussion, low: KICK, high: CRASH_2, reverb: 30, pan: 0, level: KIT },
        Instrument { name: "solo, the harmony", program: OVERDRIVEN, channel: CH_HARMONY, role: Role::Melody, low: BASS_SOLO.0, high: SOLO.1 + 4, reverb: 40, pan: -24, level: BAND },
    ];
    let mut score = Score::new(key, groove.meter(), tempo, instruments, ROOM_S);
    score.lead = Some(CH_LEAD);
    let name = |program: u8| match program {
        PIANO => "piano",
        CLEAN_GUITAR => "clean guitar",
        OVERDRIVEN => "overdriven guitar",
        DISTORTION => "distortion guitar",
        VIOLIN => "violin",
        _ => "?",
    };
    score.summary = format!(
        "{} on the {}: a {:?} in a {:?}; verse {} and {}, chorus {} and {}, climax {} and {}; {} lead, {} arpeggio, {}",
        story.name,
        groove.name,
        design.theme.shape,
        design.form,
        design.verse[0].name,
        design.verse[1].name,
        design.chorus[0].name,
        design.chorus[1].name,
        design.climax[0].name,
        design.climax[1].name,
        name(design.lead),
        name(design.clean),
        design.soloist.name(),
    );
    score.facets = vec![groove.name, design.soloist.name()];
    let bar = score.bar();

    let walk = story.place(&mut skeleton, &mut score, 4, &[], |_, _| 1.0);
    // A verse grows by every layer that joins it, from the foot to the
    // full band's verse, and falls by every one that leaves.
    let top_verse = walk.parts.iter().filter(|p| p.bed.section() == Section::Verse).map(|p| p.rung).max().unwrap_or(0).max(1);
    let solo_parts = walk.parts.iter().filter(|p| p.bed.section() == Section::Climax).count();
    let mut solo_part = 0;
    for (section, part) in score.sections.iter_mut().zip(&walk.parts) {
        section.level = Some(match part.bed.section() {
            Section::Climax => {
                solo_part += 1;
                LEVEL_SOLO - SOLO_RISE * (solo_parts - solo_part) as f32 / (solo_parts - 1).max(1) as f32
            }
            Section::Chorus => LEVEL_CHORUS,
            Section::Verse => LEVEL_FOOT + (LEVEL_VERSE - LEVEL_FOOT) * part.rung as f32 / top_verse as f32,
        });
    }

    // Each phrase on its part's question or answer, by turns; the tune a
    // third up in a chorus, and a verse's pairs stepping up by turns.
    let phrases = walk.bars() / phrase::BARS;
    let rows: Vec<&Schema> = (0..phrases)
        .map(|p| {
            let pair = match walk.bed_at(p * phrase::BARS).section() {
                Section::Verse => design.verse,
                Section::Chorus => design.chorus,
                Section::Climax => design.climax,
            };
            pair[(p % 2) as usize]
        })
        .collect();
    let shifts: Vec<i32> = (0..walk.bars()).map(|b| if walk.bed_at(b).section() == Section::Chorus { CLIMB } else { PAIRS[(b / phrase::BARS / 2) as usize % PAIRS.len()] }).collect();
    let tune = Tune::compose(&[&design.theme], &score.meter, design.form, &rows, 3, shifts);
    score.harmony = tune.chords.clone();
    let form = Form { bar, walk, tune, design };

    arpeggio(&mut score, &form, &mut rng.fork(1));
    bass(&mut score, &form, &mut rng.fork(2));
    guitars(&mut score, &form, &mut rng.fork(3));
    pad(&mut score, &form, &mut rng.fork(4));
    kit(&mut score, &form, &mut rng.fork(5));
    match form.design.soloist {
        Soloist::Guitar => guitar_solo(&mut score, &form, &mut rng.fork(6)),
        Soloist::Bass => bass_solo(&mut score, &form, &mut rng.fork(6)),
        Soloist::Drums => drum_solo(&mut score, &form, &mut rng.fork(6)),
    }
    let teller = Teller {
        lead: CH_LEAD,
        second: CH_SECOND,
        double: None,
        echo: CH_CLEAN,
        register: TUNE,
        grace: hammer,
        sung: (-4, -16),
        riff: (8, -2, -14),
        long: (64, 79, 0),
        under: (52, 67, -12),
        hold: Hold::Ringing,
        breathes: true,
        vel,
    };
    teller::tell(&mut score, &teller, &form.tune, &form.walk.runs(), &mut rng.fork(7));
    score.mark_phrases(0, form.bars());
    ending(&mut score, &form, &mut rng.fork(8));
    score.finish();
    (score, form)
}

/// The song's ending, after its final chorus. The big ending slows over
/// the chorus's last two bars into one hit on the tonic: the guitars'
/// power chord and the bass's low root held, the strings and choir on the
/// chord, the lead on its home tonic, the kit's crash and kick on it and
/// a roll on the cymbal swelling to the cut, a last crash and kick, where
/// everything stops and the room rings. The arpeggio coda leaves the
/// clean arpeggio alone over the tonic for two bars, slowing, and its last
/// chord struck low to high and left to ring.
fn ending(score: &mut Score, form: &Form, rng: &mut Rng) {
    let key = score.key;
    let bar = form.bar;
    let at = form.bars();
    let tonic = Chord::triad(0);
    let strong = score.meter.strong_eighths();
    let eighths = score.meter.eighths();
    score.mark_coda(at);
    let section = |score: &mut Score, name: &'static str, from: u32, bars: u32, level: f32, rings: bool| {
        score.sections.push(ScoreSection { name, start: from * bar, end: (from + bars) * bar, trim: 1.0, level: Some(level), rings });
        score.harmony.extend((0..bars).map(|_| tonic));
    };
    match form.design.ending {
        Ending::Big => {
            score.ritardando((at - 2) * bar, at * bar, SLOWEST);
            // One section from the hit, ringing: the pedal falls slowly at
            // first, so the hit, the swell and the cut stand, and the room
            // rings on after. A ring alone is too quiet to set a level by.
            let ring_at = at + HELD_BARS;
            section(score, "end", at, HELD_BARS + RING_BARS, LEVEL_END, true);
            let (hit, cut) = (at * bar, ring_at * bar);
            let root = at_degree(&key, tonic, 0, 40, 52, 45);
            for channel in [CH_LEFT, CH_RIGHT] {
                for p in [root, root + 7, root + 12] {
                    score.add(Note { start: hit, len: cut - hit - E / 8, pitch: p, vel: vel(0, rng), channel });
                    score.add(Note { start: cut, len: E, pitch: p, vel: vel(-6, rng), channel });
                }
            }
            let low = at_degree(&key, tonic, 0, 28, 40, 33);
            score.add(Note { start: hit, len: cut - hit - E / 8, pitch: low, vel: vel(4, rng), channel: CH_BASS });
            score.add(Note { start: cut, len: E, pitch: low, vel: vel(4, rng), channel: CH_BASS });
            for p in tonic.pitches_within(&key, 55, 72).into_iter().take(3) {
                score.add(Note { start: hit, len: cut - hit, pitch: p, vel: vel(-6, rng), channel: CH_PAD });
            }
            for degree in [0, 4] {
                let p = at_degree(&key, tonic, degree, 52, 72, 62);
                score.add(Note { start: hit, len: cut - hit, pitch: p, vel: vel(-8, rng), channel: CH_CHOIR });
            }
            let home = tune::home_tonic(&key, TUNE.0, TUNE.1);
            score.add(Note { start: hit, len: cut - hit - E / 8, pitch: home, vel: vel(4, rng), channel: CH_LEAD });
            // Struck under the band's full stroke: on one instant every
            // player's hit is the song's peak.
            for (t, pitch, accent) in [(hit, CRASH, 2), (hit, KICK, 0), (cut, CRASH, -4), (cut, KICK, -6)] {
                score.add(Note { start: t, len: 2 * E, pitch, vel: vel(accent, rng), channel: CH_KIT });
            }
            // The swell: sixteenths on the ride's edge from the bar's
            // middle, rising to the cut.
            let from = strong[strong.len() / 2];
            let slots = (eighths - from) * 2;
            for k in 0..slots {
                let x = k as f32 / slots as f32;
                score.add(Note { start: hit + from * E + k * E / 2, len: E / 2, pitch: CRASH_2, vel: vel(-34 + (28.0 * x) as i32, rng), channel: CH_KIT });
            }
        }
        Ending::Arpeggio => {
            let last = at + CODA_BARS;
            score.ritardando(at * bar, last * bar, SLOWEST);
            section(score, "coda", at, CODA_BARS, LEVEL_FOOT, false);
            section(score, "ring", last, RING_BARS, LEVEL_FOOT - 4.0, true);
            let tones = tonic.pitches_within(&key, 40, 76);
            let pattern = form.design.picking;
            for b in at..last {
                for i in 0..eighths {
                    let pitch = tones[pattern[i as usize % pattern.len()].min(tones.len() - 1)];
                    let start = b * bar + i * E;
                    score.add(Note { start, len: last * bar - start, pitch, vel: vel(if strong.contains(&i) { -8 } else { -16 }, rng), channel: CH_CLEAN });
                }
            }
            for (k, p) in tones.iter().take(6).enumerate() {
                let start = last * bar + k as u32 * E / 4;
                score.add(Note { start, len: RING_BARS * bar - k as u32 * E / 4, pitch: *p, vel: vel(-6, rng), channel: CH_CLEAN });
            }
        }
    }
}

/// The chord's tone at `degree` nearest `to` within `lo..=hi`.
fn at_degree(key: &Key, chord: Chord, degree: i32, lo: u8, hi: u8, to: u8) -> u8 {
    *chord.pitches_within(key, lo, hi).iter().filter(|p| key.degree_of(**p) == Some(degree.rem_euclid(7) as usize)).min_by_key(|p| ((**p as i32 - to as i32).abs(), **p)).unwrap()
}

/// The key's tone at `degree` nearest `to`, in any octave.
fn nearest_degree(key: &Key, degree: i32, to: u8) -> u8 {
    (0..10).map(|o| key.pitch(degree.rem_euclid(7), o)).min_by_key(|p| (*p as i32 - to as i32).abs()).unwrap()
}

/// The arpeggio: on the bar's first beat the chord's root in the low
/// register, then up through its tones — at thin one a beat, at full one
/// an eighth in the seed's picking pattern — each let ring to the chord's
/// change, as a picked chord rings under the fingers. Where the seed
/// drones, the pattern's top is one tone of the key whatever the chord,
/// the moving voices clashing and resolving under it; a chord now and
/// then takes its ninth for its tenth, the add9 a clean chord rings. Its
/// phrase's variant bar turns the pattern over now and then, and its
/// cadence either strikes the chord at the bar's middle and holds it,
/// steps its top to a neighbour, or hammers into the next bar's root. It
/// rests in a chorus, where the open guitars carry the chord, and under
/// the solo, which plays in the room it leaves.
fn arpeggio(score: &mut Score, form: &Form, rng: &mut Rng) {
    let strong = score.meter.strong_eighths();
    let eighths = score.meter.eighths();
    let half = strong[strong.len() / 2];
    let top = *form.design.picking.iter().max().unwrap();
    for b in 0..form.bars() {
        let key = score.key_at(b * form.bar);
        let mode = form.texture_at(b).clean;
        if mode == Clean::Off || form.texture_at(b).section() != Section::Verse {
            continue;
        }
        let chord = form.chord(b);
        let root = at_degree(&key, chord, chord.root, 40, 52, 45);
        let tones = chord.pitches_within(&key, root, root + 26);
        let start = b * form.bar;
        let end = start + form.bar + if form.last(b) { 0 } else { E / 8 };
        let drone = form.design.drone.map(|d| nearest_degree(&key, d, tones[top.min(tones.len() - 1)]));
        let ninth = rng.chance(0.3).then(|| key.pitch(key.absolute_degree(root).unwrap() + 8, 4));
        let turned = variant(b) && rng.chance(0.25);
        let close = if cadence(b) && mode == Clean::Full { Some(rng.below(3)) } else { None };
        let onsets: Vec<u32> = match mode {
            Clean::Thin => strong.clone(),
            _ => (0..eighths).collect(),
        };
        for (k, i) in onsets.iter().enumerate() {
            let at = start + i * E;
            let weak = !strong.contains(i);
            if close == Some(0) && *i >= half {
                // The cadence's chord, struck at the bar's middle and held.
                if *i == half {
                    for t in [0, 2, 3, 4] {
                        score.add(Note { start: at, len: end - at, pitch: tones[t.min(tones.len() - 1)], vel: vel(-6, rng), channel: CH_CLEAN });
                    }
                }
                continue;
            }
            let pattern = form.design.picking;
            let step = match mode {
                Clean::Thin => [0, 2, 3, 4][k % 4],
                _ if turned => pattern[(pattern.len() - k % pattern.len()) % pattern.len()],
                _ => pattern[k % pattern.len()],
            };
            let mut pitch = tones[step.min(tones.len() - 1)];
            if weak && step == top {
                pitch = drone.unwrap_or(pitch);
            }
            if weak && step == 4 {
                pitch = ninth.unwrap_or(pitch);
            }
            if close == Some(1) && weak && *i + 2 >= eighths {
                pitch = key.pitch(key.absolute_degree(pitch).unwrap() + if *i + 1 == eighths { 1 } else { -1 }, 4);
            }
            let accent = if *i == 0 { -2 } else if !weak { -8 } else if Some(pitch) == drone { -18 } else { -14 };
            score.add(Note { start: at, len: end - at, pitch, vel: vel(accent, rng), channel: CH_CLEAN });
        }
        if close == Some(2) && !form.last(b) {
            let next = form.chord(b + 1);
            let to = at_degree(&key, next, next.root, 40, 52, root);
            let from = key.pitch(key.absolute_degree(to).unwrap() - 1, 4).max(40);
            score.add(Note { start: start + form.bar - E / 4, len: E / 4, pitch: from, vel: vel(-12, rng), channel: CH_CLEAN });
        }
    }
}

/// The bass on the chord's root, locked to the kick: on its strokes and
/// held to the next, or on every eighth, the octave over it on the
/// variant bar's push into the backbeat now and then. Into a new chord it
/// takes the last eighth on a tone of the key a step from the next root —
/// the passing tone between where they are a third apart — on a third of
/// the changes and on every cadence's; under the kit's solo it holds the
/// root through every bar, the pedal the drums play over, struck again
/// at the variant bar's middle and stepping into the next root on the
/// cadence.
fn bass(score: &mut Score, form: &Form, rng: &mut Rng) {
    let strong = score.meter.strong_eighths();
    let eighths = score.meter.eighths();
    let groove = form.design.groove;
    for b in 0..form.bars() {
        let key = score.key_at(b * form.bar);
        let mode = form.texture_at(b).bass;
        if mode == Bass::Off || form.bass_solo(b) {
            continue;
        }
        let chord = form.chord(b);
        let root = at_degree(&key, chord, chord.root, 28, 40, 36);
        let start = b * form.bar;
        if form.stop_time(b) {
            // The pedal strikes again at the bar's middle on the variant,
            // and steps into the next bar's root on the cadence.
            let middle = strong[strong.len() / 2] * E;
            let into = (variation::role(b) == Bar::Cadence && !form.last(b)).then(|| {
                let next = form.chord(b + 1);
                let to = at_degree(&key, next, next.root, 28, 40, root);
                key.pitch(key.absolute_degree(to).unwrap() + if to > root { -1 } else { 1 }, 4).clamp(28, 52)
            });
            let held = match variation::role(b) {
                Bar::Variant => middle,
                _ if into.is_some() => form.bar - E,
                _ => form.bar,
            };
            score.add(Note { start, len: held - E / 8, pitch: root, vel: vel(0, rng), channel: CH_BASS });
            if variation::role(b) == Bar::Variant {
                score.add(Note { start: start + middle, len: form.bar - middle - E / 8, pitch: root, vel: vel(-4, rng), channel: CH_BASS });
            }
            if let Some(from) = into {
                score.add(Note { start: start + form.bar - E, len: E * 3 / 4, pitch: from, vel: vel(-6, rng), channel: CH_BASS });
            }
            continue;
        }
        let next = (!form.last(b) && !form.stop_time(b + 1)).then(|| form.chord(b + 1)).filter(|n| n.root != chord.root || cadence(b));
        let approach = next.filter(|_| cadence(b) || rng.chance(0.35)).map(|n| {
            let to = at_degree(&key, n, n.root, 28, 40, root);
            let (rd, td) = (key.absolute_degree(root).unwrap(), key.absolute_degree(to).unwrap());
            let step = if (td - rd).abs() == 2 { (td + rd) / 2 } else if td > rd { td - 1 } else { td + 1 };
            key.pitch(step, 4).clamp(28, 52)
        });
        let last_beat = eighths - *groove.groups.last().unwrap() as u32;
        let dropped = form.drop_from(b, last_beat);
        let approach = approach.filter(|_| dropped.is_none());
        let until = dropped.unwrap_or(if approach.is_some() { eighths - 1 } else { eighths });
        match mode {
            Bass::Held => {
                // On the variant bar the fifth walks into the kick's second
                // stroke.
                let passing = groove.dum.get(1).map(|d| d - 1).filter(|_| variant(b));
                for (k, i) in groove.dum.iter().enumerate() {
                    let to = groove.dum.get(k + 1).copied().unwrap_or(eighths).min(until);
                    let to = passing.filter(|p| *p > *i && *p < to).unwrap_or(to);
                    if *i < to {
                        score.add(Note { start: start + i * E, len: (to - i) * E - E / 8, pitch: root, vel: vel(-2, rng), channel: CH_BASS });
                    }
                }
                if let Some(p) = passing.filter(|p| *p < until) {
                    let fifth = at_degree(&key, chord, chord.root + 4, root + 1, 52, root + 7);
                    score.add(Note { start: start + p * E, len: E * 3 / 4, pitch: fifth, vel: vel(-6, rng), channel: CH_BASS });
                }
            }
            _ => {
                let push = groove.tek.last().map(|t| t - 1).filter(|_| variant(b) && root + 12 <= 52);
                for i in 0..until {
                    let pitch = if Some(i) == push { root + 12 } else { root };
                    let accent = if strong.contains(&i) { 0 } else { -10 };
                    score.add(Note { start: start + i * E, len: E * 3 / 4, pitch, vel: vel(accent, rng), channel: CH_BASS });
                }
            }
        }
        if let Some(p) = approach {
            score.add(Note { start: start + (eighths - 1) * E, len: E * 3 / 4, pitch: p, vel: vel(-6, rng), channel: CH_BASS });
        }
    }
}

/// The distorted guitars, double-tracked. In a verse, the root and fifth
/// palm-muted on the eighths and the next chord struck open on the
/// bar's last eighth, rung over the line — short, short, short, long —
/// and where the kit is at full, the gallop on each beat; in half the
/// verses the right side leaves the eighths to the left and plays the
/// accents alone, the bar's chord rung from its first beat and the push;
/// in a chorus, power chords — root, fifth and octave — struck on the
/// kick where it falls on a beat and let ring, the right side on the chord's inversion — fifth,
/// octave and the fifth over it, under the lead — in the parts its
/// verse played accents in; under the kit's solo, the band's stab on
/// every bar, a two-eighth hit that leaves the bar to the kit, again at
/// the middle of a phrase's variant bar and pushed into the next on its
/// cadence; under the bass's solo they rest, their
/// register the bass's.
fn guitars(score: &mut Score, form: &Form, rng: &mut Rng) {
    let key = score.key;
    let dum = form.design.groove.dum;
    let eighths = score.meter.eighths();
    let strong = score.meter.strong_eighths();
    let power = |chord: Chord| {
        let root = at_degree(&key, chord, chord.root, 40, 52, 45);
        let fifth = at_degree(&key, chord, chord.root + 4, root + 1, root + 8, root + 7);
        [root, fifth, root + 12]
    };
    let apart: Vec<bool> = form.walk.parts.iter().map(|_| rng.chance(0.5)).collect();
    let mut pushed = false;
    for b in 0..form.bars() {
        let texture = form.texture_at(b);
        let mode = texture.rhythm;
        let was_pushed = std::mem::take(&mut pushed);
        if mode == Rhythm::Off {
            continue;
        }
        let chord = form.chord(b);
        let chord_tones = power(chord);
        let start = b * form.bar;
        if form.stop_time(b) {
            // A stab is a hit, two eighths, and the bar is the kit's; the
            // variant stabs again at the bar's middle, the cadence pushes
            // into the next bar on its last eighth.
            let middle = strong[strong.len() / 2];
            let mut stabs = vec![(0, 2 * E, STAB)];
            match variation::role(b) {
                Bar::Variant => stabs.push((middle, E, STAB - 8)),
                Bar::Cadence if !form.last(b) => stabs.push((eighths - 1, E - E / 8, STAB - 4)),
                _ => {}
            }
            for (at, len, accent) in stabs {
                for channel in [CH_LEFT, CH_RIGHT] {
                    for p in chord_tones {
                        score.add(Note { start: start + at * E, len, pitch: p, vel: vel(accent, rng), channel });
                    }
                }
            }
            continue;
        }
        if form.bass_solo(b) {
            continue;
        }
        let part = form.walk.parts.iter().position(|p| p.a <= b && b < p.b).unwrap_or(0);
        let dropped = form.drop_from(b, eighths - *form.design.groove.groups.last().unwrap() as u32).unwrap_or(eighths);
        match mode {
            Rhythm::Muted => {
                let push = !form.last(b) && form.texture_at(b + 1).rhythm != Rhythm::Off && !form.stop_time(b + 1) && form.drop_from(b, eighths).is_none();
                let gallop = texture.kit == Kit::Full;
                for channel in [CH_LEFT, CH_RIGHT] {
                    let accents = channel == CH_RIGHT && apart[part];
                    for i in 0..dropped {
                        let at = start + i * E;
                        if i == 0 && was_pushed {
                            continue;
                        }
                        if accents && i == 0 {
                            for p in chord_tones {
                                score.add(Note { start: at, len: dum.get(1).copied().unwrap_or(eighths) * E - E / 8, pitch: p, vel: vel(0, rng), channel });
                            }
                            continue;
                        }
                        if accents && !(push && i + 1 == eighths) {
                            continue;
                        }
                        if push && i + 1 == eighths {
                            for p in power(form.chord(b + 1)) {
                                score.add(Note { start: at, len: 2 * E - E / 8, pitch: p, vel: vel(4, rng), channel });
                            }
                            continue;
                        }
                        let accent = if i == 0 { -4 } else { -14 };
                        let strokes: &[(u32, u32)] = if gallop && strong.contains(&i) { &[(0, E), (E, E / 2), (3 * E / 2, E / 2)] } else { &[(0, E)] };
                        if gallop && !strong.contains(&i) && strong.contains(&(i - 1)) {
                            continue;
                        }
                        for (at_off, span) in strokes {
                            for p in &chord_tones[..2] {
                                score.add(Note { start: at + at_off, len: span * 2 / 5, pitch: *p, vel: vel(accent, rng), channel });
                            }
                        }
                    }
                }
                pushed = push;
            }
            _ => {
                let [_, fifth, top] = chord_tones;
                let inverted = [fifth, top, fifth + 12];
                let last_beat = eighths - *form.design.groove.groups.last().unwrap() as u32;
                // A ringing chord is struck again only on a kick that falls
                // on a beat: a stroke off the beat inside a held chord is a
                // stumble, in four as in three.
                let struck: Vec<u32> = dum.iter().copied().filter(|i| *i == 0 || strong.contains(i)).collect();
                let pickup = struck.get(1).map(|d| d - 1).filter(|_| variant(b));
                let driving = (cadence(b) && dropped == eighths).then_some(last_beat);
                for channel in [CH_LEFT, CH_RIGHT] {
                    let voicing = if channel == CH_RIGHT && apart[part] { inverted } else { chord_tones };
                    for (k, i) in struck.iter().enumerate().filter(|(_, i)| **i < dropped) {
                        let to = struck.get(k + 1).copied().unwrap_or(eighths).min(dropped);
                        let to = [pickup, driving].into_iter().flatten().filter(|p| *p > *i && *p < to).min().unwrap_or(to);
                        for p in voicing {
                            score.add(Note { start: start + i * E, len: (to - i) * E - E / 8, pitch: p, vel: vel(if *i == 0 { 4 } else { -2 }, rng), channel });
                        }
                    }
                    if let Some(p) = pickup {
                        for t in voicing {
                            score.add(Note { start: start + p * E, len: E * 2 / 3, pitch: t, vel: vel(0, rng), channel });
                        }
                    }
                    if let Some(from) = driving {
                        for i in from..eighths {
                            for t in &voicing[..2] {
                                score.add(Note { start: start + i * E, len: E * 2 / 5, pitch: *t, vel: vel(-10 + 4 * (i - from) as i32, rng), channel });
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The strings on root and fifth a bar at a time, carried across the
/// bar where the next chord keeps them; at full, on three voices of the
/// chord, each led to the nearest tone of the next, and the choir on
/// root and fifth; thin under a solo of the bass's or the kit's, whatever
/// their notch, so the soloist is the song's lead there.
fn pad(score: &mut Score, form: &Form, rng: &mut Rng) {
    let mut voices: Option<Vec<u8>> = None;
    for b in 0..form.bars() {
        let key = score.key_at(b * form.bar);
        // A bass or a kit has no register over the band's to sing from:
        // the whole chord held covers it.
        let mode = if form.bass_solo(b) || form.stop_time(b) {
            Pad::Thin
        } else {
            form.texture_at(b).pad
        };
        if mode == Pad::Off {
            voices = None;
            continue;
        }
        let chord = form.chord(b);
        let len = form.bar + E / 8;
        let root = at_degree(&key, chord, chord.root, 52, 64, 57);
        let fifth = at_degree(&key, chord, chord.root + 4, root + 1, 71, root + 7);
        if mode == Pad::Thin {
            for p in [root, fifth] {
                score.hold(Note { start: b * form.bar, len, pitch: p, vel: vel(-18, rng), channel: CH_PAD });
            }
            voices = None;
            continue;
        }
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
            score.hold(Note { start: b * form.bar, len, pitch: *p, vel: vel(-18, rng), channel: CH_PAD });
        }
        voices = Some(next);
        for p in [root, fifth] {
            score.hold(Note { start: b * form.bar, len, pitch: p, vel: vel(-24, rng), channel: CH_CHOIR });
        }
    }
}

/// The kit. A verse's time: the closed hat on every eighth, the beats
/// over the eighths between, the kick on the groove's strokes, the side
/// stick on its backbeat and two or three ghost strokes on the snare's
/// sixteenths; a chorus's: the ride, the snare on the backbeat or, in
/// half-time, on the bar's middle alone, the open hat into it, the crash
/// on every part's first beat and every chorus phrase's. Every phrase's
/// variant bar opens the hat into the next bar or moves the kick's second
/// stroke an eighth late; its cadence fills its last beat or two, and a
/// bar that turns the song into its next part fills its last half or the
/// whole bar after its first beat, in the seed's fill. Under the guitar's
/// solo the kick drives every eighth; under the bass's it keeps a verse's
/// time, light under a lead in its own register. Cymbals strike near the
/// top of the velocity and the kick under the band's, since the bank
/// keeps its cymbals far under its kick. The kit's own solo is
/// `drum_solo`'s.
fn kit(score: &mut Score, form: &Form, rng: &mut Rng) {
    let groove = form.design.groove;
    let strong = score.meter.strong_eighths();
    let eighths = score.meter.eighths();
    let half = strong[strong.len() / 2];
    for b in 0..form.bars() {
        let texture = form.texture_at(b);
        if texture.kit == Kit::Off || form.stop_time(b) {
            continue;
        }
        let start = b * form.bar;
        let full = texture.kit == Kit::Full && !form.bass_solo(b);
        let driving = full && texture.solo == Solo::On && form.design.soloist == Soloist::Guitar;
        let last_beat = eighths - *groove.groups.last().unwrap() as u32;
        let fill_from = if let Some(from) = form.drop_from(b, last_beat) {
            Some(from)
        } else if !form.last(b) && form.turns_at(b + 1) {
            Some(if rng.chance(0.6) { strong[1] } else { half })
        } else if cadence(b) {
            Some(if rng.chance(0.5) { last_beat } else { strong[strong.len().saturating_sub(2)] })
        } else {
            None
        };
        let until = fill_from.unwrap_or(eighths);
        let snare: Vec<u32> = if full && form.design.half_time { vec![half] } else { groove.tek.to_vec() };
        let opened = variant(b) && rng.chance(0.5);
        let late_kick = variant(b) && !opened;
        let kicks: Vec<u32> = groove.dum.iter().enumerate().map(|(k, d)| if late_kick && k > 0 && d + 1 < eighths && !snare.contains(&(d + 1)) { d + 1 } else { *d }).collect();
        for i in 0..until {
            let on = strong.contains(&i);
            let at = start + i * E;
            let (cymbal, accent) = if opened && i + 1 == eighths {
                (OPEN_HAT, 16)
            } else if full {
                (RIDE, if on { 30 } else { 18 })
            } else {
                (HAT, if on { 24 } else { 12 })
            };
            score.add(Note { start: at, len: E - 20, pitch: cymbal, vel: vel(accent, rng), channel: CH_KIT });
            if kicks.contains(&i) || driving {
                score.add(Note { start: at, len: E - 20, pitch: KICK, vel: vel(if kicks.contains(&i) { -6 } else { -16 }, rng), channel: CH_KIT });
            }
            if snare.contains(&i) {
                let (pitch, accent) = if full { (SNARE, 8) } else { (SIDE_STICK, 0) };
                score.add(Note { start: at, len: E - 20, pitch, vel: vel(accent, rng), channel: CH_KIT });
            }
            if full && snare.contains(&(i + 1)) && !on && !opened {
                score.add(Note { start: at, len: E - 20, pitch: OPEN_HAT, vel: vel(10, rng), channel: CH_KIT });
            }
        }
        if !full {
            // Ghost strokes on the snare's sixteenths between the eighths,
            // clear of the backbeat.
            let ghosts = rng.range(2, 3) as usize;
            let mut slots: Vec<u32> = (0..until).filter(|i| !snare.contains(i) && !snare.contains(&(i + 1))).collect();
            for _ in 0..ghosts.min(slots.len()) {
                let i = slots.remove(rng.below(slots.len()));
                score.add(Note { start: start + i * E + E / 2, len: E / 2 - 10, pitch: SNARE, vel: vel(-46, rng), channel: CH_KIT });
            }
        }
        let opens_part = form.part_starts(b) || (texture.section() == Section::Chorus && b % phrase::BARS == 0);
        if full && opens_part {
            score.add(Note { start, len: 2 * E, pitch: CRASH, vel: vel(34, rng), channel: CH_KIT });
            score.add(Note { start, len: E - 20, pitch: KICK, vel: vel(-4, rng), channel: CH_KIT });
        }
        if let Some(from) = fill_from {
            fill(score, form.design.fill, start + from * E, (eighths - from) * 2, -2, rng);
        }
    }
}

/// The seed's fill in `n` sixteenths from `start`, growing in strength
/// from `accent` to the top of the kit's into the crash the next part
/// opens on: a roll is the loudest the band plays.
fn fill(score: &mut Score, shape: Fill, start: u32, n: u32, accent: i32, rng: &mut Rng) {
    let down = |k: u32, of: u32| TOMS[(k as usize * TOMS.len() / of.max(1) as usize).min(TOMS.len() - 1)];
    for k in 0..n {
        let pitch = match shape {
            Fill::Descent => down(k, n),
            Fill::SnareThenToms if k < n / 2 => SNARE,
            Fill::SnareThenToms => down(k - n / 2, n - n / 2),
            Fill::Pairs => down(k / 2, n.div_ceil(2)),
            Fill::HandHandKick if k % 3 == 2 => KICK,
            Fill::HandHandKick => down(k / 3, n.div_ceil(3)),
        };
        score.add(Note { start: start + k * E / 2, len: E / 2 - 10, pitch, vel: vel(accent + (36 * k / n.max(1)) as i32, rng), channel: CH_KIT });
    }
}

/// One stroke of the kit's solo, `slot` sixteenths into its beat.
fn stroke(score: &mut Score, beat: u32, slot: u32, pitch: u8, accent: i32, rng: &mut Rng) {
    score.add(Note { start: beat + slot * E / 2, len: E / 2 - 10, pitch, vel: vel(accent, rng), channel: CH_KIT });
}

/// The kit's solo over the band's stop-time, built on the seed's motif,
/// loud from its first bar and louder to its last, the whole kit in it:
/// every bar opens on the crash and the kick under the band's stab.
/// Through its first stretch the motif sounds on the toms over the ride's
/// bell, the kick on every beat and the snare on the backbeat. Through
/// its second each beat is a figure drawn from the kit's vocabulary —
/// the six toms down, the snare's roll from a flam, a linear run of kick,
/// snare and toms, the china over the kick and the toms after it, tom
/// pairs over the kick — each a different figure from the beat before.
/// Through its third the kicks double in sixteenths under the motif
/// displaced an eighth, the china or a crash on every beat; then triplet
/// sixteenths roll round the six toms, the kick every third, the crash
/// on every beat; and its last bar is the seed's fill, the one that opens
/// the choruses, into the band.
fn drum_solo(score: &mut Score, form: &Form, rng: &mut Rng) {
    let strong = score.meter.strong_eighths();
    let eighths = score.meter.eighths();
    let groups = form.design.groove.groups.to_vec();
    let backbeat = form.design.groove.tek;
    let motif = form.design.motif;
    for (a, z) in form.solos() {
        let n = z - a;
        let mut last_figure = usize::MAX;
        for b in a..z {
            let start = b * form.bar;
            let x = (b - a) as f32 / n as f32;
            let base = 16 + (12.0 * x) as i32;
            // The band's stab: every player strikes on one instant, so each
            // strikes under its full stroke, or the instant is the piece's peak.
            score.add(Note { start, len: 2 * E, pitch: CRASH, vel: vel(28, rng), channel: CH_KIT });
            score.add(Note { start, len: E - 20, pitch: KICK, vel: vel(-6, rng), channel: CH_KIT });
            if b + 1 == z {
                fill(score, form.design.fill, start + strong[1] * E, (eighths - strong[1]) * 2, base.max(-6), rng);
                continue;
            }
            let mut at = 0u32;
            for (g, len) in groups.iter().enumerate() {
                let beat = start + at * E;
                let slots = *len as u32 * 2;
                let first = if g == 0 { 2 } else { 0 };
                if x < 0.3 && cadence(b) && g + 1 == groups.len() {
                    fill(score, form.design.fill, beat, slots, base - 4, rng);
                } else if x < 0.3 {
                    for s in first..slots {
                        if s % 2 == 0 {
                            stroke(score, beat, s, RIDE_BELL, base + 14, rng);
                        }
                    }
                    if g > 0 {
                        stroke(score, beat, 0, KICK, base - 4, rng);
                    }
                    if backbeat.contains(&at) {
                        stroke(score, beat, 0, SNARE, base + 8, rng);
                    }
                    for (s, tom) in motif.iter().filter(|(s, _)| *s >= first) {
                        stroke(score, beat, *s, TOMS[*tom], base + 2, rng);
                    }
                } else if x < 0.55 {
                    let mut figure = rng.below(5);
                    if figure == last_figure {
                        figure = (figure + 1) % 5;
                    }
                    last_figure = figure;
                    for s in first..slots {
                        let rise = (10 * s / slots) as i32;
                        match figure {
                            0 => stroke(score, beat, s, TOMS[(s as usize * TOMS.len() / slots as usize).min(TOMS.len() - 1)], base + rise, rng),
                            1 => {
                                if s == first {
                                    score.add(Note { start: beat + s * E / 2 - E / 8, len: E / 8, pitch: SNARE, vel: vel(base - 20, rng), channel: CH_KIT });
                                }
                                stroke(score, beat, s, SNARE, base - 8 + 2 * rise, rng);
                            }
                            2 => stroke(score, beat, s, [KICK, SNARE, TOMS[1], TOMS[4]][s as usize % 4], base + rise, rng),
                            3 => {
                                if s == first {
                                    stroke(score, beat, s, CHINA, base + 20, rng);
                                    stroke(score, beat, s, KICK, base, rng);
                                } else {
                                    stroke(score, beat, s, TOMS[2 + (s as usize % 4)], base + rise, rng);
                                }
                            }
                            _ => {
                                stroke(score, beat, s, TOMS[(s as usize / 2 + g) % TOMS.len()], base + rise, rng);
                                if s % 2 == 0 {
                                    stroke(score, beat, s, KICK, base - 8, rng);
                                }
                            }
                        }
                    }
                } else if x < 0.8 {
                    for s in first..slots {
                        stroke(score, beat, s, KICK, base - 6, rng);
                    }
                    if g > 0 {
                        stroke(score, beat, 0, if g % 2 == 1 { CHINA } else { CRASH_2 }, base + 18, rng);
                    }
                    for (s, tom) in motif {
                        stroke(score, beat, (s + 2) % slots, TOMS[*tom], base + 8, rng);
                    }
                    if backbeat.contains(&at) {
                        stroke(score, beat, 0, SNARE, base + 12, rng);
                    }
                    if g + 1 == groups.len() {
                        stroke(score, beat, slots - 1, SPLASH, base + 10, rng);
                    }
                } else {
                    let triplets = *len as u32 * 3;
                    for t in 0..triplets {
                        if g == 0 && t < 3 {
                            continue;
                        }
                        let pitch = if t % 3 == 2 { KICK } else { TOMS[(t as usize / 3 + 2 * g) % TOMS.len()] };
                        score.add(Note { start: beat + t * E / 3, len: E / 3 - 8, pitch, vel: vel(base + 4 + (10 * t / triplets) as i32, rng), channel: CH_KIT });
                    }
                    if g > 0 {
                        stroke(score, beat, 0, CRASH, base + 20, rng);
                    }
                }
                if x >= 0.3 && g % 2 == 1 {
                    stroke(score, beat, 0, PEDAL_HAT, base, rng);
                }
                at += *len as u32;
            }
        }
    }
}

/// The solo's register as degrees: its lowest and highest tones on the
/// mode.
fn degree_span(key: &Key, (lo, hi): (u8, u8)) -> (i32, i32) {
    let degree = |p: u8| key.absolute_degree(p).unwrap();
    (degree((lo..=hi).find(|p| key.contains(*p)).unwrap()), degree((lo..=hi).rev().find(|p| key.contains(*p)).unwrap()))
}

/// The solo's peak: the highest root or fifth of the climax's chord in
/// the register.
fn peak_of(key: &Key, chord: Chord, (lo_d, hi_d): (i32, i32)) -> i32 {
    (lo_d..=hi_d)
        .rev()
        .find(|d| {
            let c = key.degree_of(key.pitch(*d, 4)).unwrap() as i32;
            [chord.root, chord.root + 4].iter().any(|r| r.rem_euclid(7) == c)
        })
        .unwrap_or(hi_d)
}

/// `notes` woven: every note of an eighth or longer run on in
/// sixteenths to the note after it — a step at a time, turning about
/// where the way is short — so the theme is heard in its own tones on
/// the beats and a guitarist's run between them.
fn woven(key: &Key, notes: &[Placed], then: Option<u8>) -> Vec<Placed> {
    let degree = |p: u8| key.absolute_degree(p).unwrap();
    let mut out = Vec::new();
    for (i, (s, l, p)) in notes.iter().enumerate() {
        let next = notes.get(i + 1).map(|n| n.2).or(then).unwrap_or(*p);
        if *l < E {
            out.push((*s, *l, *p));
            continue;
        }
        let slots = l / (E / 2);
        out.push((*s, E / 2, *p));
        for (k, d) in run_between(degree(*p), degree(next), slots).into_iter().enumerate() {
            out.push((s + (k as u32 + 1) * E / 2, E / 2, key.pitch(d, 4)));
        }
    }
    out
}

/// A beat's figure in the build, steps from its first tone, in
/// sixteenths: four up, by thirds, an enclosure, a pedal point; in a beat
/// of three eighths, two threes.
const FIGURES_TWO: [[i32; 4]; 4] = [[0, 1, 2, 3], [0, 2, 1, 3], [0, -1, 1, 2], [0, -1, 0, 2]];
const FIGURES_THREE: [[i32; 6]; 4] = [[0, 1, 2, 1, 2, 3], [0, 2, 1, 3, 2, 4], [0, -1, 1, 0, 2, 3], [0, -1, 0, 2, 0, 3]];

/// The guitar's solo, its arc in stages (`stages`), denser as it goes and
/// louder to its peak. It states the theme as the lead sang it, entering
/// off the beat now and then and resting through the last beat of every
/// other bar; plays it again woven, every foot run on in sixteenths to
/// the next; calls and answers a third higher, the call resting through
/// its last beat and the answer woven; then builds in sequences, each
/// beat a figure drawn from the vocabulary a step higher than the last
/// and starting on the chord, its last bar in sextuplets. The band drops
/// out for the beat before the climax, and the guitar breaks alone in
/// it, a run up to the tone the climax opens on: the peak — the highest
/// root or fifth of the climax's chord, touched nowhere before it, two
/// degrees under it the ceiling — lands with the band's return, slid
/// into and held most of the bar while the player bends into it and
/// shakes it, or reached by a sextuplet run at the bar's last beat. The
/// release cascades down in sextuplets, a beat a step lower, and the
/// landing sings the theme again where it began with a second guitar a
/// third over it, the harmony held back until now.
fn guitar_solo(score: &mut Score, form: &Form, rng: &mut Rng) {
    let key = score.key;
    let (lo, hi) = SOLO;
    let strong = score.meter.strong_eighths();
    let eighths = score.meter.eighths();
    let degree = |p: u8| key.standing_degree(p);
    let span = degree_span(&key, SOLO);
    let beat_of = |g: usize| strong.get(g + 1).copied().unwrap_or(eighths) - strong[g];
    for (a, z) in form.solos() {
        let st = stages(a, z);
        let peak = peak_of(&key, form.chord(st.climax), span);
        let ceiling = peak - 2;
        let capped = |p: u8| if degree(p) > ceiling { key.pitch(degree(p) - 7, 4) } else { p };
        let last_group = strong[strong.len() - 1];
        let mut line: Vec<Placed> = Vec::new();
        for b in a..z {
            let key = score.key_at(b * form.bar);
            let bar_start = b * form.bar;
            let chord = form.chord(b);
            let theme = |sung: bool| form.tune.bar(score, b, lo, hi, sung).into_iter().map(|(s, l, p)| (s, l, capped(p))).collect::<Vec<Placed>>();
            if b < st.woven || b >= st.landing {
                let mut notes = theme(true);
                if b < st.woven {
                    if (b - a) % 2 == 1 {
                        notes.retain(|n| n.0 < bar_start + last_group * E);
                    }
                    if rng.chance(0.4) {
                        if let Some(first) = notes.first_mut().filter(|n| n.1 > E) {
                            first.0 += E;
                            first.1 -= E;
                        }
                    }
                }
                line.extend(notes);
            } else if b < st.call {
                let then = form.tune.bar(score, b + 1, lo, hi, false).first().map(|n| capped(n.2));
                line.extend(woven(&key, &theme(false), then));
            } else if b < st.build {
                let up: Vec<Placed> = theme(false)
                    .into_iter()
                    .map(|(s, l, p)| {
                        let up = key.pitch(degree(p) + 2, 4);
                        let up = if score.strong(s) && !chord.holds(&key, up) { tune::nearest_chord_tone(&key, chord, up, lo, hi) } else { up };
                        (s, l, capped(up))
                    })
                    .collect();
                if (b - st.call) % 2 == 0 {
                    line.extend(up.into_iter().filter(|n| n.0 < bar_start + last_group * E));
                } else {
                    line.extend(woven(&key, &up, None));
                }
            } else if b < st.climax {
                let mut from = line.last().map_or(degree(tune::home_tonic(&key, lo, hi)), |n| degree(n.2));
                let bursting = b + 1 == st.climax;
                for (g, s) in strong.iter().enumerate() {
                    if bursting && g + 1 == strong.len() {
                        // The band's silent beat is the guitar's break: a
                        // run alone up to the tone the climax opens on.
                        let into = if form.design.held_climax { peak - 1 } else { (peak - 7).max(span.0) };
                        let slide = if form.design.held_climax { E / 4 } else { 0 };
                        let slots = beat_of(g) * 3;
                        let unit = (beat_of(g) * E - slide) / slots;
                        for (k, d) in run_between(from, into, slots + 1).into_iter().enumerate() {
                            line.push((bar_start + s * E + k as u32 * unit, unit, key.pitch(d.min(peak - 1), 4)));
                        }
                        break;
                    }
                    let want = (from + 1).min(ceiling - 4);
                    let first = (want - 2..=want + 2).filter(|d| chord.holds(&key, key.pitch(*d, 4)) && *d <= ceiling - 4).min_by_key(|d| ((d - want).abs(), -d)).unwrap_or(want);
                    let at = bar_start + s * E;
                    if bursting && beat_of(g) == 2 {
                        for k in 0..6 {
                            line.push((at + k * E / 3, E / 3, key.pitch((first + k as i32).min(ceiling), 4)));
                        }
                    } else if beat_of(g) == 3 {
                        for (k, step) in FIGURES_THREE[rng.below(4)].iter().enumerate() {
                            line.push((at + k as u32 * E / 2, E / 2, key.pitch((first + step).min(ceiling), 4)));
                        }
                    } else {
                        for (k, step) in FIGURES_TWO[rng.below(4)].iter().enumerate() {
                            line.push((at + k as u32 * E / 2, E / 2, key.pitch((first + step).min(ceiling), 4)));
                        }
                    }
                    from = first;
                }
            } else if b == st.climax {
                if form.design.held_climax {
                    // Slid into from the degree under it, held, and shaken
                    // in its last beat.
                    let held_to = bar_start + last_group * E;
                    line.push((bar_start - E / 4, E / 4, key.pitch(peak - 1, 4)));
                    line.push((bar_start, held_to - bar_start, key.pitch(peak, 4)));
                    for k in 0..beat_of(strong.len() - 1) * 4 {
                        let d = if k % 2 == 0 { peak } else { peak - 1 };
                        line.push((held_to + k * E / 4, E / 4, key.pitch(d, 4)));
                    }
                } else {
                    let from = peak - 7;
                    let run_from = strong[strong.len().saturating_sub(2)];
                    line.push((bar_start, run_from * E, key.pitch(from.max(span.0), 4)));
                    let slots = (last_group - run_from) * 3;
                    for (k, d) in run_between(from, peak, slots).into_iter().enumerate() {
                        line.push((bar_start + run_from * E + k as u32 * E / 3, E / 3, key.pitch(d.min(peak), 4)));
                    }
                    line.push((bar_start + last_group * E, (eighths - last_group) * E, key.pitch(peak, 4)));
                }
            } else {
                // The release: sextuplets cascading down, a beat a step lower.
                let mut from = line.last().map_or(peak, |n| degree(n.2)).min(ceiling);
                for (g, s) in strong.iter().enumerate() {
                    let want = (from - 1).max(span.0 + 6);
                    let first = (want - 2..=want + 2).filter(|d| chord.holds(&key, key.pitch(*d, 4)) && *d <= ceiling && *d >= span.0 + 6).min_by_key(|d| ((d - want).abs(), *d)).unwrap_or(want);
                    let unit = beat_of(g) * E / 6;
                    for (k, step) in [0, -1, -2, -1, -2, -3].iter().enumerate() {
                        line.push((bar_start + s * E + k as u32 * unit, unit, key.pitch(first + step, 4)));
                    }
                    from = first;
                }
            }
        }
        sound(score, form, line, SOLO, (a, z, st.landing), 0, rng);
    }
}

/// The bass's solo, a bassist's: the band's one bass, so the band's bass
/// part rests under it, and it plays in the two octaves from E2. It
/// states the theme there; grooves a riff — the chord's root,
/// fifth, octave and fifth on the beats, the mode's steps walking between
/// — a variant of it answering every other bar, its last beat run on in
/// sixteenths; gallops up through the chord, an eighth and two sixteenths
/// on each tone, a beat a step higher; breaks alone in the beat the band
/// drops out for before its climax, a run up to the peak's door, and
/// slides into the peak, held, and runs down
/// from it in its last beat; walks back down in sixteenths through the
/// release; and lands on the theme with a guitar a third over it.
fn bass_solo(score: &mut Score, form: &Form, rng: &mut Rng) {
    let key = score.key;
    let (lo, hi) = BASS_SOLO;
    let strong = score.meter.strong_eighths();
    let eighths = score.meter.eighths();
    let degree = |p: u8| key.standing_degree(p);
    let span = degree_span(&key, BASS_SOLO);
    let beat_of = |g: usize| strong.get(g + 1).copied().unwrap_or(eighths) - strong[g];
    let middle = key.absolute_degree(key.snap(52)).unwrap();
    for (a, z) in form.solos() {
        let st = stages(a, z);
        let peak = peak_of(&key, form.chord(st.climax), span);
        let ceiling = peak - 2;
        let last_group = strong[strong.len() - 1];
        let mut line: Vec<Placed> = Vec::new();
        for b in a..z {
            let key = score.key_at(b * form.bar);
            let bar_start = b * form.bar;
            let chord = form.chord(b);
            let dropped = b + 1 == st.climax;
            // The chord's root nearest the middle of the solo's register.
            let base = (middle - 4..=middle + 3).find(|d| key.degree_of(key.pitch(*d, 4)) == Some(chord.root.rem_euclid(7) as usize)).unwrap_or(middle);
            if b < st.woven || b >= st.landing {
                line.extend(form.tune.bar(score, b, lo, hi, true));
            } else if b < st.build {
                // The riff: chord tones on the beats, the mode walking
                // between; its variant answering every other bar.
                let answer = (b - st.woven) % 2 == 1;
                let skeleton: [i32; 4] = if answer { [0, 2, 4, 7] } else { [0, 4, 7, 4] };
                let targets: Vec<i32> = (0..=strong.len()).map(|g| base + skeleton[g % 4]).collect();
                for (g, s) in strong.iter().enumerate() {
                    let at = bar_start + s * E;
                    let fills = answer && g + 1 == strong.len();
                    let unit = if fills { E / 2 } else { E };
                    let count = beat_of(g) * E / unit;
                    line.push((at, unit, key.pitch(targets[g].min(ceiling), 4)));
                    for (k, d) in run_between(targets[g], targets[g + 1], count).into_iter().enumerate() {
                        line.push((at + (k as u32 + 1) * unit, unit, key.pitch(d.min(ceiling), 4)));
                    }
                }
            } else if b < st.climax {
                // The gallop, a beat a step higher.
                let mut from = line.last().map_or(base, |n| degree(n.2));
                for (g, s) in strong.iter().enumerate() {
                    if dropped && g + 1 == strong.len() {
                        // The band's silent beat is the bass's break: a run
                        // alone up to the tone it slides into the peak from.
                        let slots = beat_of(g) * 2;
                        let unit = (beat_of(g) * E - E / 4) / slots;
                        for (k, d) in run_between(from, peak - 1, slots + 1).into_iter().enumerate() {
                            line.push((bar_start + s * E + k as u32 * unit, unit, key.pitch(d.min(peak - 1), 4)));
                        }
                        break;
                    }
                    let want = (from + 1).min(ceiling);
                    let tone = (want - 2..=want + 2).filter(|d| chord.holds(&key, key.pitch(*d, 4)) && *d <= ceiling).min_by_key(|d| ((d - want).abs(), -d)).unwrap_or(want);
                    let at = bar_start + s * E;
                    let pitch = key.pitch(tone, 4);
                    line.push((at, E, pitch));
                    line.push((at + E, E / 2, pitch));
                    line.push((at + 3 * E / 2, E / 2, pitch));
                    if beat_of(g) == 3 {
                        line.push((at + 2 * E, E, pitch));
                    }
                    from = tone;
                }
            } else if b == st.climax {
                let held_to = bar_start + last_group * E;
                line.push((bar_start - E / 4, E / 4, key.pitch(peak - 1, 4)));
                line.push((bar_start, held_to - bar_start, key.pitch(peak, 4)));
                let slots = beat_of(strong.len() - 1) * 2;
                for (k, d) in run_between(peak, peak - 4, slots).into_iter().enumerate() {
                    line.push((held_to + (k as u32 + 1) * E / 2, E / 2, key.pitch(d, 4)));
                }
            } else {
                // Walking back down in sixteenths, a beat a step lower.
                let mut from = line.last().map_or(peak, |n| degree(n.2)).min(ceiling);
                for (g, s) in strong.iter().enumerate() {
                    let want = (from - 1).max(span.0 + 2);
                    let first = (want - 2..=want + 2).filter(|d| chord.holds(&key, key.pitch(*d, 4)) && *d <= ceiling && *d >= span.0 + 2).min_by_key(|d| ((d - want).abs(), *d)).unwrap_or(want);
                    let count = beat_of(g) * 2;
                    let at = bar_start + s * E;
                    line.push((at, E / 2, key.pitch(first, 4)));
                    for (k, d) in run_between(first, first - 2, count).into_iter().enumerate() {
                        line.push((at + (k as u32 + 1) * E / 2, E / 2, key.pitch(d.max(span.0), 4)));
                    }
                    from = first;
                }
            }
        }
        sound(score, form, line, BASS_SOLO, (a, z, st.landing), 12, rng);
    }
}

/// A melodic solo's `line` played: held to the solo's end at its last
/// tone, every strong beat's tone bent onto the chord and the line
/// repaired as the lead's is, so no leap leaps on; struck `lift` over the
/// band's velocity and growing through the solo; and over its landing a
/// guitar a third above it, a third under where the third over is off the
/// chord.
fn sound(score: &mut Score, form: &Form, mut line: Vec<Placed>, (lo, hi): (u8, u8), (a, z, landing): (u32, u32, u32), lift: i32, rng: &mut Rng) {
    let (key, harmony, bar) = (score.key, score.harmony.clone(), score.bar());
    let key_at = move |tick: u32| key.under(harmony[((tick / bar) as usize).min(harmony.len() - 1)]);
    let degree = |tick: u32, p: u8| key_at(tick).absolute_degree(p).unwrap();
    line.sort_by_key(|n| n.0);
    if let Some(last) = line.last_mut() {
        last.1 = (z * form.bar - E / 2).saturating_sub(last.0).max(last.1);
    }
    for n in line.iter_mut() {
        n.2 = n.2.clamp(lo, hi);
        if !key_at(n.0).contains(n.2) {
            n.2 = key_at(n.0).snap(n.2).clamp(lo, hi);
        }
    }
    on_the_chord(score, form, &mut line, lo, hi);
    form.tune.repair(score, &mut line, lo, hi);
    let span = (z - a) * form.bar;
    for (start, len, pitch) in &line {
        let grow = (14 * start.saturating_sub(a * form.bar) / span) as i32 - 4;
        let accent = lift + grow + if *len >= 2 * E { 10 } else if score.strong(*start) { 4 } else { -6 };
        let len = if *len <= E / 2 { (*len).saturating_sub(10).max(E / 8) } else { len - E / 8 };
        score.add(Note { start: *start, len, pitch: *pitch, vel: vel(accent, rng), channel: CH_SOLO });
    }
    let mut over: Vec<Placed> = line
        .iter()
        .filter(|n| n.0 >= landing * form.bar)
        .map(|(s, l, p)| {
            let chord = form.chord(s / form.bar);
            let key = key_at(*s);
            let third = key.pitch(degree(*s, *p) + 2, 4);
            let pitch = if !score.strong(*s) || chord.holds(&key, third) { third } else { key.pitch(degree(*s, *p) - 2, 4) };
            (*s, *l, pitch)
        })
        .collect();
    on_the_chord(score, form, &mut over, lo, hi + 4);
    form.tune.repair(score, &mut over, lo, hi + 4);
    for (start, len, pitch) in over {
        let len = if len <= E / 2 { len.saturating_sub(10).max(E / 8) } else { len - E / 8 };
        score.add(Note { start, len, pitch, vel: vel(-6, rng), channel: CH_HARMONY });
    }
}

/// Every tone of `line` on a strong beat bent onto its bar's chord,
/// toward the tone before it.
fn on_the_chord(score: &Score, form: &Form, line: &mut [Placed], lo: u8, hi: u8) {
    let mut prev: Option<u8> = None;
    for n in line.iter_mut() {
        let chord = form.chord(n.0 / form.bar);
        if score.strong(n.0) && !chord.holds(&score.key, n.2) {
            n.2 = tune::bent_to_chord(&score.key, chord, n.2, prev, lo, hi);
        }
        prev = Some(n.2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every story is a ladder, and every one reaches the solo.
    #[test]
    fn every_story_is_a_ladder_to_the_solo() {
        for story in &STORIES {
            assert_eq!(story.fault(), None);
            assert!(matches!(story.ladder.last(), Some(Layer::Solo)), "{}: the solo is not its crest", story.name);
            assert_eq!(*story.leads.last().unwrap(), Off, "{}: the lead sings over the solo", story.name);
            assert_eq!(story.base.section(), Section::Verse, "{}: does not open on a verse", story.name);
            assert!((0..story.ladder.len()).any(|r| story.bed(r).section() == Section::Chorus), "{}: has no chorus", story.name);
        }
    }

    /// The song is whole question-and-answer pairs, its parts whole
    /// half-phrases, then its ending, marked where it begins; its harmony
    /// opens and closes on the tonic, its tune and its solo in their
    /// registers; nothing struck ends past the end, only what is held.
    #[test]
    fn a_ballad_is_whole_phrases_and_an_ending() {
        for seed in 0..24 {
            let (score, form) = compose(&Params { seed });
            let bars = form.bars();
            assert_eq!(bars % (2 * phrase::BARS), 0, "seed {seed}: {bars} bars");
            for s in score.sections.iter().filter(|s| s.end <= bars * score.bar()) {
                assert_eq!((s.end - s.start) % (phrase::BARS / 2 * score.bar()), 0, "seed {seed}: a part of broken half-phrases");
            }
            assert_eq!(score.marked(crate::score::Mark::Coda).collect::<Vec<_>>(), vec![bars * score.bar()]);
            assert!(score.sections.last().unwrap().rings, "seed {seed}: the ending does not ring");
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

    /// No part plays one bar the whole song through: the kit's bars and
    /// the arpeggio's differ within a phrase somewhere in every seed.
    #[test]
    fn no_part_repeats_one_bar() {
        for seed in 0..12 {
            let (score, form) = compose(&Params { seed });
            for ch in [CH_KIT, CH_CLEAN] {
                let bar_of = |b: u32| -> Vec<(u32, u8)> { score.notes.iter().filter(|n| n.channel == ch && n.start / form.bar == b).map(|n| (n.start % form.bar, n.pitch)).collect() };
                let varied = (0..form.bars() - 1).any(|b| {
                    let (x, y) = (bar_of(b), bar_of(b + 1));
                    !x.is_empty() && !y.is_empty() && form.chord(b) == form.chord(b + 1) && x != y
                }) || (0..form.bars()).any(|b| cadence(b) && !bar_of(b).is_empty());
                assert!(varied, "seed {seed}: channel {ch} never varies");
            }
        }
    }
}
