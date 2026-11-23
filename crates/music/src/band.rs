//! A band: its members, each a player on an instrument, and how it likes
//! to play.
//! A band is defined, as a track is — its name, who it is, who plays
//! every part of its style on what, how each of them plays, and what the
//! band prefers are written here — and only the play, the
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
//! - **Its preferences:** how it likes to play a track — where in a
//!   groove's tempo range, which grooves and which stories it reaches for
//!   more or less often than the track would (`Prefs`).
//!
//! Adding a band: an entry in `BANDS`, a member for every part of its
//! style, drawn from the style's own practice.

use crate::players::{self, Player};

/// A style of music: the vocabulary its tracks share and its bands are
/// made for.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Style {
    /// The Bulgarian folk of the land, its uneven dances and its modes:
    /// the overworld's bed, the horo, the teaser's cue.
    Bulgarian,
}

impl Style {
    pub const ALL: [Style; 1] = [Style::Bulgarian];

    pub fn name(self) -> &'static str {
        match self {
            Style::Bulgarian => "bulgarian",
        }
    }

    /// The parts its tracks give out, each a band of it may name a member
    /// for.
    pub fn parts(self) -> &'static [Part] {
        use Part::*;
        match self {
            Style::Bulgarian => &[Lead, Second, Bass, Drums, Drone, Pad, Pluck, Pluck2, Choir, Horn, Shimmer, Figure, Trombone, Tuba, Accordion, Doubler, HeldReed, Echo],
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
    /// A held horn, a colour.
    Horn,
    /// The tune echoed under everything, soft.
    Echo,
    /// A bright struck colour: the dulcimer.
    Shimmer,
    Drone,
    /// The held harmony: strings.
    Pad,
    /// A plucked part of its own: the tambura's line.
    Pluck,
    Pluck2,
    /// Voices: the choir's held harmony.
    Choir,
    /// The figure a band runs on: the tambura's strum under a horo.
    Figure,
    Trombone,
    Tuba,
    Accordion,
    /// The riff doubled, in unison under a horo.
    Doubler,
    /// A reed holding where the dance thickens.
    HeldReed,
}

/// A band, as it is defined in `BANDS`.
#[derive(Debug, PartialEq)]
pub struct Band {
    pub name: &'static str,
    pub style: Style,
    /// Who the band is, in a line.
    pub about: &'static str,
    pub members: &'static [Member],
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

/// General MIDI programs, 0-based, and drum kits.
mod gm {
    pub const DULCIMER: u8 = 15;
    pub const ACCORDION: u8 = 21;
    pub const NYLON_GUITAR: u8 = 24;
    pub const CLEAN_GUITAR: u8 = 27;
    pub const UPRIGHT_BASS: u8 = 32;
    pub const FINGER_BASS: u8 = 33;
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
    pub const FRENCH_HORN: u8 = 60;
    pub const TENOR_SAX: u8 = 66;
    pub const ENGLISH_HORN: u8 = 69;
    pub const CLARINET: u8 = 71;
    pub const FLUTE: u8 = 73;
    pub const PAN_FLUTE: u8 = 75;
    pub const FIDDLE: u8 = 110;
    pub const SHANAI: u8 = 111;
    /// The standard drum kit.
    pub const KIT: u8 = 0;
}
use gm::*;

/// A member, in a line: the part, the program, the player of it, and the
/// member's feel, `Feel`'s fields in order.
#[allow(clippy::too_many_arguments)]
const fn m(part: Part, program: u8, player: &'static Player, ornaments: f32, vibrato: f32, rate: f32, lean: f32, loose: f32, force: f32, length: f32) -> Member {
    Member { part, program, player, rig: None, feel: Feel { ornaments, vibrato, rate, lean, loose, force, length } }
}

use crate::rigs::Rig;
use players::{DOUBLER, DRONE, DRUMMER, FOLK_CLARINET, OBOIST, PAN_PIPER, PICKER, PIPER, SECOND, SECTION, VILLAGE_CLARINET, VILLAGE_FIDDLE, VILLAGE_TRUMPET, ZURNACI};
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
        prefs: Prefs {
            tempo: (0.5, 1.0),
            leans: &[("râčenica", 2.0), ("kopanica", 1.5), ("lesnoto", 0.7), ("full swing", 1.5), ("dance", 1.5), ("fragments", 0.5), ("dance first", 2.0), ("call", 1.5), ("fade", 0.5), ("stroke", 2.0), ("decay", 0.5), ("the tapan", 1.5), ("a taksim", 0.5), ("pressing on", 3.0), ("a unison run", 3.0)],
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
        prefs: Prefs {
            tempo: (0.0, 0.6),
            leans: &[("lesnoto", 1.5), ("dajčovo", 1.3), ("kopanica", 0.6), ("râčenica", 0.8), ("drum first", 1.5), ("breather", 1.2), ("lament", 1.5), ("fragments", 1.3), ("prelude", 2.0), ("figure first", 1.5), ("dance first", 0.5), ("alone", 2.0), ("stroke", 0.5), ("a taksim", 2.5), ("one tempo", 1.5), ("a held tone", 2.0), ("a unison run", 0.3)],
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
        prefs: Prefs {
            tempo: (0.25, 0.75),
            leans: &[("arc", 1.5), ("two waves", 1.3), ("slow burn", 1.2), ("gathering", 1.5), ("fade", 2.0), ("cadence", 3.0), ("stroke", 0.5), ("straight in", 2.0), ("building", 2.0), ("a step a section", 2.0), ("the last line three times", 2.0)],
        },
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
