//! A band: its members, each a player on an instrument, and its habits.
//! A band is defined, as a track is — its name, who it is, who plays
//! every part of its style on what, how each of them plays, and what the
//! band does whatever it plays are written here — and only the play, the
//! track's improvisation, is drawn. A band is made for a style and
//! checked against that style's tracks; a band playing outside its style
//! is undefined — it plays, and may sound wrong, as a band out of its
//! idiom does. What a band holds:
//!
//! - **Its members:** one for every part its style's tracks give out
//!   (`Part`), each an instrument and the player on it — the bassist's
//!   upright or electric, the drummer's brushes or sticks, the tambura or
//!   the guitar. A part the band names no member for is played on the
//!   track's own instrument by the player its role calls for: every part
//!   of a track sounds whoever plays it, since a story's ladder moves its
//!   layers and a layer no one plays is a rung that moves nothing. A part
//!   may have several members, ranked, where a track takes the first its
//!   palette allows, as a lead does: a wedding orchestra's clarinet leads
//!   the overworld and the horo alike.
//! - **How each member plays:** a player of its part (`players`) tilted
//!   by the member's own feel — more scoops or fewer, a wider or a
//!   narrower vibrato, ahead of the beat or behind it, tight or loose —
//!   within the ranges the studies give
//!   (`proofs/research/ornaments-findings.md`), so no two members of any
//!   band play alike (`Feel`).
//! - **Its habits:** what the band does whatever it plays — how often its
//!   drummer fills and how, how its horns answer the singer, whether its
//!   lead runs into a last tone.
//! - **Its preferences:** how it likes to play a track — where in a
//!   groove's tempo range, which grooves and which stories it reaches for
//!   more or less often than the track would (`Prefs`).
//!
//! Adding a band: an entry in `BANDS`, a member for every part of its
//! style, drawn from the style's own practice.

use crate::players::{self, Player};
use crate::rock::{self, Fill};

/// A style of music: the vocabulary its tracks share and its bands are
/// made for.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Style {
    /// The Bulgarian folk of the land, its uneven dances and its modes:
    /// the overworld's bed, the horo, the teaser's cue.
    Bulgarian,
    Blues,
    Metal,
    /// Indie folk-pop as Of Monsters and Men played it on their debut: the
    /// major, a strummed acoustic, two voices trading the tune, the hook
    /// on a horn or the accordion over a stomp.
    IndieFolk,
}

impl Style {
    pub const ALL: [Style; 4] = [Style::Bulgarian, Style::Blues, Style::Metal, Style::IndieFolk];

    pub fn name(self) -> &'static str {
        match self {
            Style::Bulgarian => "bulgarian",
            Style::Blues => "blues",
            Style::Metal => "metal",
            Style::IndieFolk => "indie folk",
        }
    }

    /// The parts its tracks give out, each a band of it may name a member
    /// for.
    pub fn parts(self) -> &'static [Part] {
        use Part::*;
        match self {
            Style::Bulgarian => &[Lead, Second, Bass, Drums, Drone, Pad, Pluck, Pluck2, Choir, Horn, Shimmer, Figure, Trombone, Tuba, Accordion, Doubler, HeldReed, Echo],
            Style::Blues => &[Lead, Second, Bass, Drums, Comp, Keys, Organ, Horn, Echo, Shimmer],
            Style::Metal => &[Lead, Second, Bass, Drums, RhythmLeft, RhythmRight, Clean, Pad, Choir],
            Style::IndieFolk => &[Lead, Second, Bass, Drums, Figure, Pluck, Keys, Choir, Doubler],
        }
    }

}

/// A part of the music, what a track gives one member to play.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Part {
    /// The one who tells the tune.
    Lead,
    /// The voice under the lead, holding tones and taking a phrase.
    Second,
    Bass,
    Drums,
    /// The blues guitar's comp.
    Comp,
    /// The piano: the blues piano's comp, the folk piano's chords on the
    /// quarters.
    Keys,
    Organ,
    /// A held horn, a colour.
    Horn,
    /// The tune echoed under everything, soft.
    Echo,
    /// A bright struck colour: the vibes, the dulcimer.
    Shimmer,
    Drone,
    /// The held harmony: strings.
    Pad,
    /// A plucked part of its own: the tambura's line, the banjo's rolls.
    Pluck,
    Pluck2,
    /// Voices: the choir's held harmony, a folk song's gang.
    Choir,
    /// The figure a band runs on: the tambura's strum under a horo, the
    /// acoustic guitar's strum under a folk song.
    Figure,
    Trombone,
    Tuba,
    Accordion,
    /// The riff doubled: in unison under a horo, an octave over a folk
    /// song's hook.
    Doubler,
    /// A reed holding where the dance thickens.
    HeldReed,
    RhythmLeft,
    RhythmRight,
    /// The ballad's clean arpeggio.
    Clean,
}

/// A band, as it is defined in `BANDS`.
#[derive(Debug, PartialEq)]
pub struct Band {
    pub name: &'static str,
    pub style: Style,
    /// Who the band is, in a line.
    pub about: &'static str,
    pub members: &'static [Member],
    pub habits: Habits,
    pub prefs: Prefs,
}

/// How a band likes to play a track: the stretch of a groove's tempo
/// range it takes, of the stretch the setting allows (`ladder::Bounds`),
/// and how much more or less often it reaches for a choice its tracks
/// draw, by the choice's name in the style's vocabulary — a groove, a
/// story, how a track opens, the one thing it does, who solos, how it
/// ends — every other as the track draws it. The openings and endings are
/// the band's signatures, what a listener learns it by across its songs:
/// a band whose every song opens and ends as any other's does is a
/// roster, not a band.
#[derive(Debug, PartialEq)]
pub struct Prefs {
    pub tempo: (f32, f32),
    pub leans: &'static [(&'static str, f32)],
}

/// One member: the part they play, on what, through what, and how.
#[derive(Debug, PartialEq)]
pub struct Member {
    pub part: Part,
    /// The General MIDI program, a drum kit's for the drums.
    pub program: u8,
    /// The player of the part this member is.
    pub player: &'static Player,
    /// The gear they bring, where their instrument is recorded at the
    /// jack; none, the rig the program calls for (`rigs::of`).
    pub rig: Option<&'static Rig>,
    pub feel: Feel,
}

/// How a member plays against the player of their part: their ornaments'
/// shares scaled (two thirds to four thirds), their vibrato's width
/// scaled (four fifths to five quarters), where in the player's range
/// their vibrato's rate sits (0 the slowest, 1 the quickest), how far
/// they lean behind the beat, ms (under nothing, ahead), how loose they
/// are — their time's wander, their strokes' spread, their touch and
/// their intonation scaled together (three fifths, tight, to three
/// halves, loose), how much harder they strike than written, velocity
/// steps (under nothing, softer), and how much of a note's written length
/// they hold, scaled (four fifths, detached, to six fifths, carried on).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Feel {
    pub ornaments: f32,
    pub vibrato: f32,
    pub rate: f32,
    pub lean: f32,
    pub loose: f32,
    pub force: f32,
    pub length: f32,
}

/// What a band does whatever it plays, in its style's terms.
#[derive(Debug, PartialEq)]
pub enum Habits {
    Bulgarian,
    Blues(Blues),
    Metal(rock::Habits),
    IndieFolk,
}

/// A blues band's habits: how it answers its singer, whether its lead
/// runs into a last tone, and the harmony it puts under the twelve bars.
#[derive(Debug, PartialEq)]
pub struct Blues {
    pub answers: Answers,
    pub runs_over: bool,
    pub harmony: Harmony,
    /// How the band opens a song, what it does once in one, and how it
    /// ends one, each as often as it does
    /// (`proofs/research/blues-bands-findings.md` §8): a bandstand's
    /// conventions are a band's, not a song's.
    pub openings: &'static [(Intro, f32)],
    pub events: &'static [(Event, f32)],
    pub endings: &'static [(Ending, f32)],
    /// How the bass moves where the song has it moving
    /// (`proofs/research/blues-bands-findings.md` §3), each as often as
    /// the band takes it; the rumba's three and three and two is the
    /// groove's, not the band's.
    pub moving: &'static [(Moving, f32)],
}

/// A bass's motion under a moving bar: a walk, a chord tone on every
/// beat and a chromatic step into the next bar's root; the shuffle's box,
/// the root, its fifth and its seventh on the beats, the up-town box
/// leaping to the octave; or the riff, one figure in the minor pentatonic
/// over every chord's root, the guitar in unison an octave up.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Moving {
    Walking,
    Boxes,
    Riff,
}

/// The harmony a blues band puts under the frame
/// (`proofs/research/blues-bands-findings.md` §2): how often it takes
/// each twelve-bar row the schemata hold, by name and weight — the
/// quick change, the jazz blues' dominant into four, the turn home
/// through ♭VI7 and V7, the soul blues' thrill turn — what it turns a
/// chorus on in its last bar, and what it ends on. A row a band names
/// no weight for it never plays.
#[derive(Debug, PartialEq)]
pub struct Harmony {
    pub rows: &'static [(&'static str, f32)],
    pub turnarounds: &'static [(Turnaround, f32)],
    pub lasts: &'static [(Last, f32)],
}

/// The chord a chorus turns into the next on, its last bar: the tonic
/// held, the V7, ♭VI7, iiø or ♭II7.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Turnaround {
    Tonic,
    Dominant,
    FlatSix,
    HalfDim,
    FlatTwo,
}

/// The last chord: the tonic seventh, its ninth, ♭VI, the major tonic,
/// or ♭II.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Last {
    Seventh,
    Ninth,
    FlatSix,
    Picardy,
    FlatTwo,
}

/// How the song opens: a vamp on the tonic the band layers into; the
/// lead soloing a whole chorus before the head; four bars from the V; the
/// lead alone in free time over a held tonic; or the head at once.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Intro {
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
pub enum Event {
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
pub enum Ending {
    Held,
    VampOut,
    Break,
    Tag,
}

/// The Chicago harp combo's harmony: the plain form, the quick change
/// a third of the time, the turn home through V and IV or through the
/// minor turn, a turnaround bar as often as the tonic held, the seventh
/// or its ninth to end.
const PLAIN_BLUES: Harmony = Harmony {
    rows: &[("tonic row", 7.0), ("quick change", 3.0), ("subdominant row", 1.0), ("turn home", 40.0), ("turn home, the dominant held", 20.0), ("minor turn", 40.0), ("minor turn, the sixth lowered", 40.0)],
    turnarounds: &[(Turnaround::Tonic, 50.0), (Turnaround::Dominant, 50.0)],
    lasts: &[(Last::Seventh, 70.0), (Last::Ninth, 30.0)],
};

/// The late-night quartet's: the quick change and the dominant into
/// four, the minor turn, the deceptive and the two-five, a chorus turned
/// on the iiø, ♭II7 or ♭VI7, the ninth or a borrowed major seventh to
/// end.
const JAZZ_BLUES: Harmony = Harmony {
    rows: &[("quick change", 5.0), ("quick change, the dominant into four", 5.0), ("tonic row, the dominant into four", 2.0), ("subdominant row", 1.0), ("minor turn", 50.0), ("minor turn, the sixth lowered", 50.0), ("deceptive turn", 25.0), ("deceptive turn, the sixth lowered", 25.0), ("two-five turn", 25.0), ("two-five turn, the fifth lowered", 25.0)],
    turnarounds: &[(Turnaround::HalfDim, 35.0), (Turnaround::FlatTwo, 25.0), (Turnaround::FlatSix, 20.0), (Turnaround::Dominant, 20.0)],
    lasts: &[(Last::Ninth, 50.0), (Last::FlatSix, 20.0), (Last::FlatTwo, 20.0), (Last::Seventh, 10.0)],
};

/// The soul horn band's: the tonic row mostly, the thrill turn — ♭VI a
/// major seventh into the V — over the turn home, the tonic held or the
/// V to turn, the seventh, the major tonic or the ninth to end.
const SOUL_BLUES: Harmony = Harmony {
    rows: &[("tonic row", 6.0), ("quick change", 4.0), ("subdominant row", 1.0), ("the thrill turn", 50.0), ("the thrill turn, the sixth lowered", 50.0), ("turn home", 30.0), ("minor turn", 20.0), ("minor turn, the sixth lowered", 20.0)],
    turnarounds: &[(Turnaround::Tonic, 60.0), (Turnaround::Dominant, 30.0), (Turnaround::FlatSix, 10.0)],
    lasts: &[(Last::Seventh, 60.0), (Last::Picardy, 20.0), (Last::Ninth, 20.0)],
};

/// How a blues band answers its singer: in every hole the lead leaves,
/// in about half of them, or never, holding the chord.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Answers {
    Obbligato,
    Sparse,
    Pads,
}

/// General MIDI programs, 0-based, and drum kits.
mod gm {
    pub const PIANO: u8 = 0;
    pub const E_PIANO: u8 = 4;
    pub const VIBES: u8 = 11;
    pub const DULCIMER: u8 = 15;
    pub const ORGAN: u8 = 16;
    pub const ACCORDION: u8 = 21;
    pub const HARMONICA: u8 = 22;
    pub const NYLON_GUITAR: u8 = 24;
    pub const STEEL_GUITAR: u8 = 25;
    pub const JAZZ_GUITAR: u8 = 26;
    pub const CLEAN_GUITAR: u8 = 27;
    pub const OVERDRIVEN: u8 = 29;
    pub const DISTORTION: u8 = 30;
    pub const UPRIGHT_BASS: u8 = 32;
    pub const FINGER_BASS: u8 = 33;
    pub const PICKED_BASS: u8 = 34;
    pub const VIOLIN: u8 = 40;
    pub const CELLO: u8 = 42;
    pub const CONTRABASS: u8 = 43;
    pub const PIZZICATO: u8 = 45;
    pub const HARP: u8 = 46;
    pub const STRINGS_1: u8 = 48;
    pub const STRINGS_2: u8 = 49;
    pub const CHOIR_AAHS: u8 = 52;
    pub const TRUMPET: u8 = 56;
    pub const TROMBONE: u8 = 57;
    pub const TUBA: u8 = 58;
    pub const MUTED_TRUMPET: u8 = 59;
    pub const FRENCH_HORN: u8 = 60;
    pub const ALTO_SAX: u8 = 65;
    pub const TENOR_SAX: u8 = 66;
    pub const ENGLISH_HORN: u8 = 69;
    pub const CLARINET: u8 = 71;
    pub const FLUTE: u8 = 73;
    pub const PAN_FLUTE: u8 = 75;
    pub const WHISTLE: u8 = 78;
    pub const BANJO: u8 = 105;
    pub const FIDDLE: u8 = 110;
    pub const SHANAI: u8 = 111;
    /// Drum kits: the standard, the rock kit on sticks, the brushes.
    pub const KIT: u8 = 0;
    pub const ROCK_KIT: u8 = 16;
    pub const BRUSH_KIT: u8 = 40;
}
use gm::*;

/// A member, in a line: the part, the program, the player of it, and the
/// member's feel, `Feel`'s fields in order.
#[allow(clippy::too_many_arguments)]
const fn m(part: Part, program: u8, player: &'static Player, ornaments: f32, vibrato: f32, rate: f32, lean: f32, loose: f32, force: f32, length: f32) -> Member {
    Member { part, program, player, rig: None, feel: Feel { ornaments, vibrato, rate, lean, loose, force, length } }
}

use crate::rigs::Rig;
use players::{DOUBLER, DRONE, DRUMMER, FOLK_CLARINET, HARP_PLAYER, LEAD_GUITARIST, MUTED_TRUMPETER, OBOIST, PAN_PIPER, PICKER, PIPER, PLAIN, SAXOPHONIST, SECOND, SECTION, VILLAGE_CLARINET, VILLAGE_FIDDLE, VILLAGE_TRUMPET, ZURNACI};
use Part::*;

/// Every band, by style, the first of each style the one a play takes
/// where none is named.
pub const BANDS: &[Band] = &[
    Band {
        name: "Orkestar Slavei",
        style: Style::Bulgarian,
        about: "a Thracian wedding orchestra: the clarinet out front, the trumpet and the fiddle after it, ornaments thick and the beat pushed; electric bass and guitar, sax, accordion and brass on the kit",
        members: &[
            m(Lead, CLARINET, &VILLAGE_CLARINET, 1.3, 1.0, 0.75, -4.0, 0.8, 3.0, 1.0),
            m(Lead, TRUMPET, &VILLAGE_TRUMPET, 1.2, 1.1, 0.6, -3.0, 0.9, 3.0, 1.0),
            m(Lead, FIDDLE, &VILLAGE_FIDDLE, 1.25, 1.05, 0.7, -2.0, 0.9, 3.0, 1.0),
            m(Lead, FLUTE, &PIPER, 1.1, 1.0, 0.6, -2.0, 0.9, 3.0, 1.0),
            m(Second, ACCORDION, &SECOND, 1.0, 0.9, 0.5, -2.0, 0.8, 3.0, 1.0),
            m(Second, FIDDLE, &SECOND, 1.0, 1.0, 0.5, -1.0, 0.9, 3.0, 1.0),
            m(Second, FRENCH_HORN, &SECOND, 1.0, 1.0, 0.5, 0.0, 1.0, 3.0, 1.0),
            m(Bass, FINGER_BASS, &PICKER, 1.0, 1.0, 0.5, -5.0, 0.7, 6.0, 0.85),
            m(Drums, KIT, &DRUMMER, 1.0, 1.0, 0.5, -4.0, 0.7, 6.0, 1.0),
            m(Drone, CONTRABASS, &DRONE, 1.0, 1.0, 0.5, 0.0, 1.0, 3.0, 1.0),
            m(Pad, STRINGS_1, &SECTION, 1.0, 1.0, 0.5, 2.0, 0.9, 3.0, 1.0),
            m(Pluck, CLEAN_GUITAR, &PICKER, 1.0, 1.0, 0.5, -3.0, 0.8, 6.0, 0.85),
            m(Pluck2, NYLON_GUITAR, &PICKER, 1.0, 1.0, 0.5, -2.0, 0.9, 6.0, 0.85),
            m(Horn, FRENCH_HORN, &SECTION, 1.0, 1.0, 0.5, 2.0, 0.9, 3.0, 1.0),
            m(Figure, CLEAN_GUITAR, &PICKER, 1.0, 1.0, 0.5, -4.0, 0.7, 6.0, 0.85),
            m(Trombone, TROMBONE, &PICKER, 1.0, 1.0, 0.5, -2.0, 0.8, 6.0, 0.85),
            m(Tuba, TUBA, &PICKER, 1.0, 1.0, 0.5, -1.0, 0.9, 6.0, 0.85),
            m(Accordion, ACCORDION, &SECTION, 1.0, 1.0, 0.5, -2.0, 0.8, 3.0, 1.0),
            m(Doubler, TENOR_SAX, &DOUBLER, 1.0, 1.1, 0.6, -3.0, 0.8, 6.0, 0.85),
            m(HeldReed, CLARINET, &SECTION, 1.0, 1.0, 0.5, 0.0, 0.9, 3.0, 1.0),
            m(Echo, NYLON_GUITAR, &PICKER, 1.0, 1.0, 0.5, -2.0, 0.9, 6.0, 0.85),
        ],
        habits: Habits::Bulgarian,
        prefs: Prefs {
            tempo: (0.5, 1.0),
            leans: &[("râčenica", 2.0), ("kopanica", 1.5), ("lesnoto", 0.7), ("full swing", 1.5), ("dance", 1.5), ("fragments", 0.5), ("dance first", 2.0), ("call", 1.5), ("fade", 0.5), ("fifth", 1.5), ("stroke", 2.0), ("decay", 0.5), ("the tapan", 1.5), ("a taksim", 0.5), ("pressing on", 3.0), ("a unison run", 3.0)],
        },
    },
    Band {
        name: "Gorno Pole Village Band",
        style: Style::Bulgarian,
        about: "Rhodope village musicians: a shepherd's flute and the gadulka, a zurna at a feast, the tambura's strum and the tapan, no brass; plain-spoken, loose, a little behind the beat",
        members: &[
            // Ranked so the overworld takes the shepherd's pipe and the horo
            // the zurna, a track taking the first lead its palette allows.
            m(Lead, PAN_FLUTE, &PAN_PIPER, 0.9, 0.85, 0.4, 3.0, 1.3, -3.0, 1.0),
            m(Lead, SHANAI, &ZURNACI, 1.0, 0.9, 0.5, 2.0, 1.2, -3.0, 1.0),
            m(Lead, FIDDLE, &VILLAGE_FIDDLE, 0.95, 0.9, 0.45, 3.0, 1.3, -3.0, 1.0),
            m(Lead, FLUTE, &PIPER, 0.9, 0.85, 0.4, 3.0, 1.3, -3.0, 1.0),
            m(Second, FIDDLE, &SECOND, 1.0, 0.9, 0.4, 4.0, 1.3, -3.0, 1.0),
            m(Second, ACCORDION, &SECOND, 1.0, 0.9, 0.4, 4.0, 1.2, -3.0, 1.0),
            m(Bass, UPRIGHT_BASS, &PICKER, 1.0, 1.0, 0.5, 4.0, 1.4, -6.0, 1.05),
            m(Drums, KIT, &DRUMMER, 1.0, 1.0, 0.5, 2.0, 1.4, -6.0, 1.0),
            m(Drone, CELLO, &DRONE, 1.0, 1.0, 0.5, 0.0, 1.0, -3.0, 1.0),
            m(Pad, STRINGS_2, &SECTION, 1.0, 1.0, 0.5, 4.0, 1.2, -3.0, 1.0),
            m(Pluck, PIZZICATO, &PICKER, 1.0, 1.0, 0.5, 3.0, 1.3, -6.0, 1.05),
            m(Pluck2, DULCIMER, &PICKER, 1.0, 1.0, 0.5, 3.0, 1.3, -6.0, 1.05),
            m(Choir, CHOIR_AAHS, &SECTION, 1.0, 1.0, 0.5, 4.0, 1.2, -3.0, 1.0),
            m(Shimmer, DULCIMER, &PICKER, 1.0, 1.0, 0.5, 3.0, 1.3, -6.0, 1.05),
            m(Figure, NYLON_GUITAR, &PICKER, 1.0, 1.0, 0.5, 3.0, 1.4, -6.0, 1.05),
            m(Accordion, ACCORDION, &SECTION, 1.0, 1.0, 0.5, 4.0, 1.2, -3.0, 1.0),
            m(Doubler, FLUTE, &DOUBLER, 1.0, 0.9, 0.4, 3.0, 1.3, -6.0, 1.05),
            m(Echo, NYLON_GUITAR, &PICKER, 1.0, 1.0, 0.5, 3.0, 1.3, -6.0, 1.05),
        ],
        habits: Habits::Bulgarian,
        prefs: Prefs {
            tempo: (0.0, 0.6),
            leans: &[("lesnoto", 1.5), ("dajčovo", 1.3), ("kopanica", 0.6), ("râčenica", 0.8), ("drum first", 1.5), ("breather", 1.2), ("lament", 1.5), ("fragments", 1.3), ("prelude", 2.0), ("figure first", 1.5), ("dance first", 0.5), ("song", 2.0), ("alone", 2.0), ("stroke", 0.5), ("a taksim", 2.5), ("one tempo", 1.5), ("a held tone", 2.0), ("a unison run", 0.3)],
        },
    },
    Band {
        name: "Ensemble Zora",
        style: Style::Bulgarian,
        about: "a state folk ensemble: folk tunes arranged for flute, English horn, harp and strings under a women's choir, the brass orchestral, ornamented as a tune is sung, tight and on the beat",
        members: &[
            // Ranked so the overworld takes the flute and the horo the fiddle,
            // the gadulka's stand-in, where a wedding orchestra's horo is the
            // clarinet's.
            m(Lead, FLUTE, &PIPER, 0.75, 1.15, 0.5, 0.0, 0.65, 0.0, 1.0),
            m(Lead, ENGLISH_HORN, &OBOIST, 0.75, 1.15, 0.5, 0.0, 0.65, 0.0, 1.0),
            m(Lead, FIDDLE, &PIPER, 0.75, 1.2, 0.5, 0.0, 0.65, 0.0, 1.0),
            m(Lead, CLARINET, &FOLK_CLARINET, 0.75, 1.1, 0.5, 0.0, 0.65, 0.0, 1.0),
            m(Second, ENGLISH_HORN, &SECOND, 1.0, 1.15, 0.5, 1.0, 0.65, 0.0, 1.0),
            m(Second, FIDDLE, &SECOND, 1.0, 1.15, 0.5, 1.0, 0.65, 0.0, 1.0),
            m(Second, FRENCH_HORN, &SECOND, 1.0, 1.1, 0.5, 1.0, 0.65, 0.0, 1.0),
            m(Bass, UPRIGHT_BASS, &PICKER, 1.0, 1.0, 0.5, 0.0, 0.6, 0.0, 1.0),
            m(Drums, KIT, &DRUMMER, 1.0, 1.0, 0.5, 0.0, 0.6, 0.0, 1.0),
            m(Drone, CONTRABASS, &DRONE, 1.0, 1.0, 0.5, 0.0, 1.0, 0.0, 1.0),
            m(Pad, STRINGS_2, &SECTION, 1.0, 1.0, 0.5, 1.0, 0.65, 0.0, 1.0),
            m(Pluck, HARP, &PICKER, 1.0, 1.0, 0.5, 0.0, 0.6, 0.0, 1.0),
            m(Pluck2, DULCIMER, &PICKER, 1.0, 1.0, 0.5, 0.0, 0.6, 0.0, 1.0),
            m(Choir, CHOIR_AAHS, &SECTION, 1.0, 1.0, 0.5, 1.0, 0.65, 0.0, 1.0),
            m(Horn, FRENCH_HORN, &SECTION, 1.0, 1.0, 0.5, 1.0, 0.65, 0.0, 1.0),
            m(Shimmer, DULCIMER, &PICKER, 1.0, 1.0, 0.5, 0.0, 0.6, 0.0, 1.0),
            m(Figure, HARP, &PICKER, 1.0, 1.0, 0.5, 0.0, 0.6, 0.0, 1.0),
            m(Trombone, TROMBONE, &PICKER, 1.0, 1.0, 0.5, 0.0, 0.65, 0.0, 1.0),
            m(Tuba, TUBA, &PICKER, 1.0, 1.0, 0.5, 0.0, 0.65, 0.0, 1.0),
            m(Doubler, CLARINET, &DOUBLER, 1.0, 1.1, 0.5, 0.0, 0.65, 0.0, 1.0),
            m(HeldReed, ENGLISH_HORN, &SECTION, 1.0, 1.0, 0.5, 1.0, 0.65, 0.0, 1.0),
            m(Echo, HARP, &PICKER, 1.0, 1.0, 0.5, 0.0, 0.6, 0.0, 1.0),
        ],
        habits: Habits::Bulgarian,
        prefs: Prefs {
            tempo: (0.25, 0.75),
            leans: &[("arc", 1.5), ("two waves", 1.3), ("slow burn", 1.2), ("gathering", 1.5), ("fade", 2.0), ("lifted", 2.0), ("handover", 1.5), ("cadence", 3.0), ("stroke", 0.5), ("straight in", 2.0), ("building", 2.0), ("a step a section", 2.0), ("the last line three times", 2.0)],
        },
    },
    Band {
        name: "Ruby Hollis & the Late Shift",
        style: Style::Blues,
        about: "a Chicago harp combo: the harmonica out front, the tenor behind it, electric guitar and bass, sticks on the kit, the guitar answering every line; no organ, no horns",
        members: &[
            m(Lead, HARMONICA, &HARP_PLAYER, 1.25, 1.1, 0.6, -2.0, 1.1, 2.0, 1.0),
            m(Lead, TENOR_SAX, &SAXOPHONIST, 1.2, 1.1, 0.6, -2.0, 1.1, 2.0, 1.0),
            m(Second, TENOR_SAX, &SECOND, 1.0, 1.1, 0.6, -1.0, 1.1, 2.0, 1.0),
            m(Second, ALTO_SAX, &SECOND, 1.0, 1.1, 0.6, -1.0, 1.1, 2.0, 1.0),
            m(Bass, FINGER_BASS, &PICKER, 1.0, 1.0, 0.5, -3.0, 1.0, 5.0, 0.95),
            m(Drums, ROCK_KIT, &DRUMMER, 1.0, 1.0, 0.5, -3.0, 1.0, 5.0, 1.0),
            m(Comp, CLEAN_GUITAR, &PICKER, 1.0, 1.0, 0.5, -2.0, 1.1, 5.0, 0.95),
            m(Keys, PIANO, &PICKER, 1.0, 1.0, 0.5, -1.0, 1.1, 5.0, 0.95),
            m(Echo, CLEAN_GUITAR, &PICKER, 1.0, 1.0, 0.5, -1.0, 1.1, 5.0, 0.95),
        ],
        habits: Habits::Blues(Blues { answers: Answers::Obbligato, runs_over: true, harmony: PLAIN_BLUES, openings: &[(Intro::Turnaround, 40.0), (Intro::Vamp, 30.0), (Intro::Straight, 20.0), (Intro::SoloChorus, 10.0)], events: &[(Event::None, 40.0), (Event::StopTime, 30.0), (Event::Drop, 15.0), (Event::CodaVamp, 15.0)], endings: &[(Ending::Break, 40.0), (Ending::Tag, 35.0), (Ending::Held, 25.0)], moving: &[(Moving::Boxes, 55.0), (Moving::Walking, 45.0)] }),
        prefs: Prefs {
            tempo: (0.4, 1.0),
            leans: &[("slow twelve-eight", 1.3), ("walking four", 1.3), ("rumba", 0.7), ("six-eight", 0.6), ("rush hour", 1.3), ("corner", 1.3)],
        },
    },
    Band {
        name: "The Back Room Quartet",
        style: Style::Blues,
        about: "late-night jazz blues: a muted trumpet and the alto, upright bass and jazz guitar, the vibes over the comp, brushes; the band holding the chord under the singer, laid back",
        members: &[
            m(Lead, MUTED_TRUMPET, &MUTED_TRUMPETER, 0.8, 0.9, 0.35, 5.0, 0.9, -4.0, 1.0),
            m(Lead, ALTO_SAX, &SAXOPHONIST, 0.8, 0.9, 0.35, 5.0, 0.9, -4.0, 1.0),
            m(Second, ALTO_SAX, &SECOND, 1.0, 0.9, 0.35, 6.0, 0.9, -4.0, 1.0),
            m(Second, TENOR_SAX, &SECOND, 1.0, 0.9, 0.35, 6.0, 0.9, -4.0, 1.0),
            m(Bass, UPRIGHT_BASS, &PICKER, 1.0, 1.0, 0.5, 2.0, 0.8, -8.0, 1.05),
            m(Drums, BRUSH_KIT, &DRUMMER, 1.0, 1.0, 0.5, 3.0, 0.8, -8.0, 1.0),
            m(Comp, JAZZ_GUITAR, &PICKER, 1.0, 1.0, 0.5, 5.0, 0.9, -8.0, 1.05),
            m(Keys, PIANO, &PICKER, 1.0, 1.0, 0.5, 5.0, 0.9, -8.0, 1.05),
            m(Echo, VIBES, &PICKER, 1.0, 1.0, 0.5, 5.0, 0.9, -8.0, 1.05),
            m(Shimmer, VIBES, &PICKER, 1.0, 1.0, 0.5, 5.0, 0.9, -8.0, 1.05),
        ],
        habits: Habits::Blues(Blues { answers: Answers::Pads, runs_over: false, harmony: JAZZ_BLUES, openings: &[(Intro::Rubato, 40.0), (Intro::SoloChorus, 25.0), (Intro::Vamp, 20.0), (Intro::Turnaround, 15.0)], events: &[(Event::None, 50.0), (Event::Drop, 25.0), (Event::CodaVamp, 15.0), (Event::StopTime, 10.0)], endings: &[(Ending::Held, 50.0), (Ending::VampOut, 35.0), (Ending::Tag, 15.0)], moving: &[(Moving::Walking, 100.0)] }),
        prefs: Prefs {
            tempo: (0.2, 0.7),
            leans: &[("walking four", 2.0), ("rumba", 0.6), ("six-eight", 0.8), ("late night", 1.5), ("after hours", 1.5)],
        },
    },
    Band {
        name: "Otis Mabry & the Night Owls",
        style: Style::Blues,
        about: "a soul-blues horn band: the tenor sings, the alto and the trumpet punch, electric bass and guitar, the electric piano comps and the organ washes, sticks on the kit",
        members: &[
            m(Lead, TENOR_SAX, &SAXOPHONIST, 1.0, 1.2, 0.55, 0.0, 1.0, 1.0, 1.0),
            m(Lead, ALTO_SAX, &SAXOPHONIST, 1.0, 1.2, 0.55, 0.0, 1.0, 1.0, 1.0),
            m(Second, ALTO_SAX, &SECOND, 1.0, 1.2, 0.55, 1.0, 1.0, 1.0, 1.0),
            m(Second, MUTED_TRUMPET, &SECOND, 1.0, 1.2, 0.55, 1.0, 1.0, 1.0, 1.0),
            m(Bass, FINGER_BASS, &PICKER, 1.0, 1.0, 0.5, -1.0, 0.8, 2.0, 0.9),
            m(Drums, ROCK_KIT, &DRUMMER, 1.0, 1.0, 0.5, -1.0, 0.8, 2.0, 1.0),
            m(Comp, CLEAN_GUITAR, &PICKER, 1.0, 1.0, 0.5, 0.0, 0.9, 2.0, 0.9),
            m(Keys, E_PIANO, &PICKER, 1.0, 1.0, 0.5, 0.0, 0.9, 2.0, 0.9),
            m(Organ, ORGAN, &SECTION, 1.0, 1.0, 0.5, 1.0, 0.9, 1.0, 1.0),
            m(Horn, TRUMPET, &SECTION, 1.0, 1.0, 0.5, 0.0, 0.9, 1.0, 1.0),
            m(Echo, E_PIANO, &PICKER, 1.0, 1.0, 0.5, 0.0, 0.9, 2.0, 0.9),
            m(Shimmer, VIBES, &PICKER, 1.0, 1.0, 0.5, 0.0, 0.9, 2.0, 0.9),
        ],
        habits: Habits::Blues(Blues { answers: Answers::Sparse, runs_over: true, harmony: SOUL_BLUES, openings: &[(Intro::Straight, 40.0), (Intro::Turnaround, 30.0), (Intro::Vamp, 30.0)], events: &[(Event::None, 35.0), (Event::StopTime, 25.0), (Event::CodaVamp, 25.0), (Event::Drop, 15.0)], endings: &[(Ending::Tag, 40.0), (Ending::VampOut, 35.0), (Ending::Break, 25.0)], moving: &[(Moving::Riff, 60.0), (Moving::Walking, 40.0)] }),
        prefs: Prefs {
            tempo: (0.3, 0.8),
            leans: &[("rumba", 1.8), ("six-eight", 1.5), ("slow twelve-eight", 1.2), ("walking four", 0.6), ("stroll", 1.3)],
        },
    },
    Band {
        name: "Ashen Crown",
        style: Style::Metal,
        about: "twin-guitar power metal: melodic leads in harmony, a picked bass in sixteenths, a drummer who rolls down the toms every eight bars, strings and a choir for the ballads",
        members: &[
            m(Lead, DISTORTION, &LEAD_GUITARIST, 1.0, 1.0, 0.5, 0.0, 0.8, 2.0, 1.0),
            m(Second, OVERDRIVEN, &LEAD_GUITARIST, 0.9, 1.0, 0.5, 0.0, 0.8, 2.0, 1.0),
            m(Bass, PICKED_BASS, &PICKER, 1.0, 1.0, 0.5, -2.0, 0.7, 4.0, 0.95),
            m(Drums, ROCK_KIT, &DRUMMER, 1.0, 1.0, 0.5, -2.0, 0.6, 4.0, 1.0),
            m(RhythmLeft, DISTORTION, &PICKER, 1.0, 1.0, 0.5, -1.0, 0.7, 4.0, 0.95),
            m(RhythmRight, OVERDRIVEN, &PICKER, 1.0, 1.0, 0.5, 0.0, 0.7, 4.0, 0.95),
            m(Clean, CLEAN_GUITAR, &PICKER, 1.0, 1.0, 0.5, 0.0, 0.8, 4.0, 0.95),
            m(Pad, STRINGS_2, &SECTION, 1.0, 1.0, 0.5, 2.0, 0.8, 2.0, 1.0),
            m(Choir, CHOIR_AAHS, &SECTION, 1.0, 1.0, 0.5, 2.0, 0.8, 2.0, 1.0),
        ],
        habits: Habits::Metal(rock::Habits { shredder_leads: false, fill: Fill::Descent, fill_every: 8, bass_sixteenths: true, violin: 0.0, fills: 0.15, pushes: 0.2 }),
        prefs: Prefs {
            tempo: (0.5, 1.0),
            leans: &[("doubled", 1.5), ("gallop", 1.5), ("backbeat", 0.7), ("anthem", 1.5), ("power ballad", 1.3), ("the twins first", 2.0), ("the ritual", 2.0), ("guitar solo", 1.5), ("a big ending", 2.0), ("a tag", 1.5)],
        },
    },
    Band {
        name: "Iron Requiem",
        style: Style::Metal,
        about: "shred metal: two distorted guitars, the lead fast and often, a finger bass, a drummer who fills every four bars and licks in the singer's gaps, a piano for the clean parts",
        members: &[
            m(Lead, DISTORTION, &LEAD_GUITARIST, 1.3, 1.2, 0.75, -2.0, 0.7, 4.0, 1.0),
            m(Second, DISTORTION, &LEAD_GUITARIST, 1.2, 1.15, 0.7, -2.0, 0.7, 4.0, 1.0),
            m(Bass, FINGER_BASS, &PICKER, 1.0, 1.0, 0.5, -3.0, 0.6, 8.0, 0.85),
            m(Drums, ROCK_KIT, &DRUMMER, 1.0, 1.0, 0.5, -4.0, 0.6, 8.0, 1.0),
            m(RhythmLeft, DISTORTION, &PICKER, 1.0, 1.0, 0.5, -3.0, 0.6, 8.0, 0.85),
            m(RhythmRight, DISTORTION, &PICKER, 1.0, 1.0, 0.5, -2.0, 0.6, 8.0, 0.85),
            m(Clean, PIANO, &PICKER, 1.0, 1.0, 0.5, -1.0, 0.7, 8.0, 0.85),
            m(Pad, STRINGS_1, &SECTION, 1.0, 1.0, 0.5, 0.0, 0.7, 4.0, 1.0),
        ],
        habits: Habits::Metal(rock::Habits { shredder_leads: true, fill: Fill::Roll, fill_every: 4, bass_sixteenths: true, violin: 0.0, fills: 0.45, pushes: 0.25 }),
        prefs: Prefs {
            tempo: (0.6, 1.0),
            leans: &[("doubled", 2.0), ("single", 1.3), ("half-speed", 0.5), ("power ballad", 1.2), ("requiem", 0.7), ("drums first", 2.0), ("one guitar first", 1.5), ("one stab", 2.0), ("a false ending", 2.0), ("drum solo", 2.0), ("the band leaving", 0.5)],
        },
    },
    Band {
        name: "Pale Choir",
        style: Style::Metal,
        about: "gothic metal with a violinist, who leads most of its ballads; overdriven guitars, a finger bass in eighths, a sparing drummer behind the beat, piano, strings and a choir",
        members: &[
            m(Lead, OVERDRIVEN, &LEAD_GUITARIST, 0.85, 1.15, 0.3, 3.0, 1.0, -2.0, 1.0),
            m(Lead, VIOLIN, &PLAIN, 1.0, 1.15, 0.3, 3.0, 1.0, -2.0, 1.0),
            m(Second, OVERDRIVEN, &LEAD_GUITARIST, 0.8, 1.1, 0.3, 3.0, 1.0, -2.0, 1.0),
            m(Bass, FINGER_BASS, &PICKER, 1.0, 1.0, 0.5, 4.0, 1.0, -4.0, 1.08),
            m(Drums, ROCK_KIT, &DRUMMER, 1.0, 1.0, 0.5, 4.0, 1.0, -4.0, 1.0),
            m(RhythmLeft, OVERDRIVEN, &PICKER, 1.0, 1.0, 0.5, 3.0, 1.0, -4.0, 1.08),
            m(RhythmRight, OVERDRIVEN, &PICKER, 1.0, 1.0, 0.5, 4.0, 1.0, -4.0, 1.08),
            m(Clean, PIANO, &PICKER, 1.0, 1.0, 0.5, 4.0, 1.0, -4.0, 1.08),
            m(Pad, STRINGS_2, &SECTION, 1.0, 1.0, 0.5, 5.0, 1.0, -2.0, 1.0),
            m(Choir, CHOIR_AAHS, &SECTION, 1.0, 1.0, 0.5, 5.0, 1.0, -2.0, 1.0),
        ],
        habits: Habits::Metal(rock::Habits { shredder_leads: false, fill: Fill::SnareThenToms, fill_every: 16, bass_sixteenths: false, violin: 0.6, fills: 0.0, pushes: 0.12 }),
        prefs: Prefs {
            tempo: (0.0, 0.6),
            leans: &[("backbeat", 1.5), ("half-speed", 1.5), ("doubled", 0.6), ("requiem", 1.8), ("slow burn", 1.5), ("twelve-eight", 1.4), ("held chords first", 2.0), ("a chord left to ring", 3.0), ("the intro again", 2.0), ("the band leaving", 1.5), ("bass solo", 0.5)],
        },
    },
    Band {
        name: "Of Giants and Lanterns",
        style: Style::IndieFolk,
        about: "Icelandic stomp-and-holler in Of Monsters and Men's manner: two trumpets in unison on the hook, an accordion answering, two steel-string acoustics strummed hard, a piano on the quarters, the kit's kick and floor tom on every beat and the whole band shouting the chorus",
        members: &[
            m(Lead, TRUMPET, &PLAIN, 1.0, 0.9, 0.4, 0.0, 0.9, 0.0, 1.0),
            m(Lead, ACCORDION, &PLAIN, 1.0, 1.0, 0.5, 0.0, 0.9, 0.0, 1.05),
            m(Second, TRUMPET, &SECOND, 1.0, 0.9, 0.4, 2.0, 0.9, 0.0, 1.0),
            m(Second, ACCORDION, &SECOND, 1.0, 1.0, 0.5, 1.0, 0.9, 0.0, 1.0),
            m(Bass, FINGER_BASS, &PICKER, 1.0, 1.0, 0.5, -1.0, 0.9, 2.0, 1.0),
            m(Drums, KIT, &DRUMMER, 1.0, 1.0, 0.5, -2.0, 0.9, 2.0, 1.0),
            m(Figure, STEEL_GUITAR, &PICKER, 1.0, 1.0, 0.5, -1.0, 1.0, 2.0, 1.1),
            m(Keys, PIANO, &PICKER, 1.0, 1.0, 0.5, 0.0, 0.9, 0.0, 1.0),
            m(Choir, CHOIR_AAHS, &SECTION, 1.0, 1.0, 0.5, 2.0, 1.0, 0.0, 1.0),
        ],
        habits: Habits::IndieFolk,
        prefs: Prefs { tempo: (0.0, 0.6), leans: &[("hook and drop", 1.8), ("drive", 1.3), ("from the top", 0.7), ("the hook first", 2.0), ("the gang out", 2.0), ("thinned out", 0.5)] },
    },
    Band {
        name: "The Wheelwrights",
        style: Style::IndieFolk,
        about: "a barn-floor stomp-and-holler band in the manner of Mumford & Sons and the Lumineers: two guitarists strumming hard over the kick drum at one's foot, a banjo rolling sixteenths through the choruses, a piano on the quarters, the hook on the accordion with a whistle over it, a cello bowing long tones under it, the upright bass and the gang on the chorus",
        members: &[
            m(Lead, ACCORDION, &PLAIN, 1.0, 1.0, 0.5, 2.0, 1.1, 0.0, 1.05),
            m(Second, CELLO, &SECOND, 1.0, 0.85, 0.3, 3.0, 1.1, -2.0, 1.0),
            m(Bass, UPRIGHT_BASS, &PICKER, 1.0, 1.0, 0.5, 3.0, 1.2, 0.0, 1.05),
            m(Drums, KIT, &DRUMMER, 1.0, 1.0, 0.5, 1.0, 1.2, 4.0, 1.0),
            m(Figure, STEEL_GUITAR, &PICKER, 1.0, 1.0, 0.5, 1.0, 1.2, 3.0, 1.0),
            m(Pluck, BANJO, &PICKER, 1.0, 1.0, 0.5, 0.0, 1.1, 0.0, 1.0),
            m(Keys, PIANO, &PICKER, 1.0, 1.0, 0.5, 2.0, 1.1, 0.0, 1.0),
            m(Choir, CHOIR_AAHS, &SECTION, 1.0, 1.0, 0.5, 3.0, 1.2, 2.0, 1.0),
            m(Doubler, WHISTLE, &DOUBLER, 1.0, 1.0, 0.5, 2.0, 1.1, 0.0, 1.0),
        ],
        habits: Habits::IndieFolk,
        prefs: Prefs { tempo: (0.2, 1.0), leans: &[("from the top", 1.8), ("long climb", 1.4), ("four on the floor", 1.3), ("hook and drop", 0.5), ("the strum first", 1.5), ("thinned out", 1.5), ("one stomp", 1.5), ("the gang out", 0.5)] },
    },
];

/// The band named `name`.
pub fn find(name: &str) -> Option<&'static Band> {
    BANDS.iter().find(|b| b.name == name)
}

/// The bands of `style`, the first the one a play takes where none is
/// named.
pub fn of_style(style: Style) -> impl Iterator<Item = &'static Band> {
    BANDS.iter().filter(move |b| b.style == style)
}

impl Band {
    /// The first band of `style`.
    pub fn first(style: Style) -> &'static Band {
        of_style(style).next().expect("every style has a band")
    }

    /// The members playing `part`, ranked.
    pub fn members(&self, part: Part) -> impl Iterator<Item = &'static Member> + use<'_> {
        let members: &'static [Member] = self.members;
        members.iter().filter(move |m| m.part == part)
    }

    /// The program `part` is played on: the first of its members' that
    /// `palette` allows, or any where `palette` is empty; else `fallback`,
    /// the track's own.
    pub fn program(&self, part: Part, palette: &[u8], fallback: u8) -> u8 {
        self.members(part).map(|m| m.program).find(|p| palette.is_empty() || palette.contains(p)).unwrap_or(fallback)
    }

    /// The programs `part`'s members play, ranked.
    pub fn programs(&self, part: Part) -> Vec<u8> {
        self.members(part).map(|m| m.program).collect()
    }

    /// The member playing `part` on `program`, else the first playing
    /// `part`, as they play: their player tilted by their feel.
    pub fn player(&self, part: Part, program: u8) -> Option<Player> {
        let member = self.members(part).find(|m| m.program == program).or_else(|| self.members(part).next())?;
        Some(tilt(*member.player, member.feel))
    }

    /// The gear the member playing `part` on `program` brings, where
    /// they bring any.
    pub fn rig(&self, part: Part, program: u8) -> Option<&'static Rig> {
        self.members(part).find(|m| m.program == program).and_then(|m| m.rig)
    }

    /// How much more or less often the band reaches for the choice named
    /// `name` than the track would: 1 where it has no preference.
    pub fn lean(&self, name: &str) -> f32 {
        self.prefs.leans.iter().find(|(n, _)| *n == name).map_or(1.0, |(_, w)| *w)
    }

    /// The band's blues habits; a band of another style plays out of its
    /// idiom on the first blues band's.
    pub fn blues(&self) -> &Blues {
        match &self.habits {
            Habits::Blues(h) => h,
            _ => Band::first(Style::Blues).blues(),
        }
    }

    /// The band's metal habits, as `blues` is.
    pub fn metal(&self) -> &rock::Habits {
        match &self.habits {
            Habits::Metal(h) => h,
            _ => Band::first(Style::Metal).metal(),
        }
    }
}

/// `p` played with `feel`: its ornaments' shares scaled together, its
/// vibrato's width scaled and its rate placed in the player's own range,
/// its lean shifted, its looseness, its force and its length.
fn tilt(p: Player, feel: Feel) -> Player {
    let lean = (p.lean.0 + feel.lean, p.lean.1 + feel.lean);
    let loose = Player { lean, drift: p.drift * feel.loose, slip: p.slip * feel.loose, touch: p.touch * feel.loose, intonation: p.intonation * feel.loose, force: p.force + feel.force, length: p.length * feel.length, ..p };
    let Some(mut o) = p.ornaments else {
        return loose;
    };
    let share = feel.ornaments;
    o.bend = (o.bend * share).min(0.9);
    o.slide = (o.slide * share).min(0.9 - o.bend);
    o.grace = o.grace.map(|g| players::Grace { share: (g.share * share).min(0.6), ..g });
    o.shake = o.shake.map(|s| players::Shake { share: (s.share * share).min(0.4), ..s });
    o.narrow = (o.narrow.0 * feel.vibrato, o.narrow.1 * feel.vibrato);
    o.wide = (o.wide.0 * feel.vibrato, o.wide.1 * feel.vibrato);
    let mid = o.rate_hz.0 + (o.rate_hz.1 - o.rate_hz.0) * feel.rate;
    o.rate_hz = (mid - 0.2, mid + 0.2);
    Player { ornaments: Some(o), ..loose }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every style has bands; every band names a lead and no part out of
    /// its style; every name is its band's alone; every feel keeps to the
    /// studies' ranges.
    #[test]
    fn every_band_is_defined_in_its_style() {
        for style in Style::ALL {
            assert!(of_style(style).count() >= 2, "{} has too few bands", style.name());
        }
        for (i, band) in BANDS.iter().enumerate() {
            assert!(band.members(Part::Lead).next().is_some(), "{} brings no lead", band.name);
            for m in band.members {
                assert!(band.style.parts().contains(&m.part), "{}: {:?} is no part of {}", band.name, m.part, band.style.name());
                let f = m.feel;
                assert!((0.67..=1.34).contains(&f.ornaments) && (0.8..=1.25).contains(&f.vibrato) && (0.0..=1.0).contains(&f.rate) && f.lean.abs() <= 8.0 && (0.6..=1.5).contains(&f.loose) && f.force.abs() <= 12.0 && (0.8..=1.2).contains(&f.length), "{}: {:?}'s feel out of range", band.name, m.part);
            }
            assert!(BANDS[i + 1..].iter().all(|b| b.name != band.name), "{} twice", band.name);
            assert!(0.0 <= band.prefs.tempo.0 && band.prefs.tempo.0 < band.prefs.tempo.1 && band.prefs.tempo.1 <= 1.0, "{}: a tempo out of the range", band.name);
            assert!(band.prefs.leans.iter().all(|(_, w)| *w > 0.0), "{}: a lean that forbids", band.name);
            let fits = matches!((&band.habits, band.style), (Habits::Bulgarian, Style::Bulgarian) | (Habits::Blues(_), Style::Blues) | (Habits::Metal(_), Style::Metal) | (Habits::IndieFolk, Style::IndieFolk));
            assert!(fits, "{}: habits out of its style", band.name);
        }
    }

    /// The bands of a style play each part their own way: no two bassists
    /// or drummers of a style alike.
    #[test]
    fn no_two_members_play_alike() {
        for style in Style::ALL {
            let bands: Vec<&Band> = of_style(style).collect();
            for part in [Part::Bass, Part::Drums, Part::Lead] {
                let players: Vec<(u8, Player)> = bands.iter().map(|b| {
                    let program = b.programs(part)[0];
                    (program, b.player(part, program).unwrap())
                }).collect();
                for i in 0..players.len() {
                    for j in i + 1..players.len() {
                        assert_ne!(players[i], players[j], "{} and {} play the {part:?} alike", bands[i].name, bands[j].name);
                    }
                }
            }
        }
    }
}
