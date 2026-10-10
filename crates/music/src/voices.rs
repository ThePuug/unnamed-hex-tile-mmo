//! Which bank plays each instrument. A score names its players as
//! General MIDI programs, and the default bank — GeneralUser GS — plays
//! every one; but banks specialise, and where another plays a program
//! better, this table says which file and which of its presets. A piece
//! never names a bank: the choice is made once here for every piece that
//! uses the instrument, so a better guitar is every guitar.
//!
//! The banks are sampled instruments, every one free for commercial use:
//! the synthesizer ignores a bank's modulators, so a bank whose tone is
//! programmed rather than recorded — as GeneralUser's guitars are —
//! comes out flat, and one with each velocity recorded keeps its bite.
//! `banks/install.py` beside musicgen fetches and names them.
//!
//! A bank this table names sits in the default bank's directory; a
//! machine without it plays the program on the default bank, so a build
//! there sounds as the table was before the bank was added.

/// A program played by another bank: its file, the bank number and
/// preset the program is under there, the keys it sounds, and for a drum
/// kit the key each General MIDI drum is struck on there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Voice {
    /// The General MIDI program it plays, 0-based; for a drum kit, the
    /// kit's program on the percussion channel.
    pub program: u8,
    pub percussion: bool,
    /// The bank's file name, beside the default bank's.
    pub file: &'static str,
    pub bank: u8,
    pub preset: u8,
    /// `(General MIDI key, the bank's key)`, for a kit whose drums are not
    /// on General MIDI's keys; a key not named plays as written.
    pub keys: &'static [(u8, u8)],
    /// The lowest and highest keys the file sounds. A melodic note outside
    /// them is struck the fewest octaves in, so no note falls silent and
    /// none leaves its chord; a part written past them is heard folded.
    pub range: (u8, u8),
    /// The recordings the file holds of each note, and how one is
    /// chosen.
    pub takes: Takes,
    /// Whether it was recorded at the jack, before any amp: it sounds
    /// through its part's rig (`rigs`).
    pub direct: bool,
    /// The preset playing each note entered past its attack, the string
    /// already ringing — a hammer-on, a pull-off, a slide's arrival —
    /// where the file has one.
    pub legato: Option<u8>,
}

/// How many recordings of each note a file holds, and how the
/// synthesizer is asked for one: a struck note takes a different one from
/// the last, as a hand never strikes the same note twice alike.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Takes {
    One,
    /// The n-th on the key `TAKE_KEYS` × n over the first, as a kit lays
    /// them out.
    Keys(u8),
    /// The n-th where the velocity is n modulo `count`, within each of
    /// the file's dynamic `layers`, the velocities each one over the first
    /// starts at: FSBS's guitars, recorded four times at each of two
    /// dynamics.
    Velocities { count: u8, layers: &'static [u8] },
}

impl Takes {
    pub fn count(self) -> u8 {
        match self {
            Takes::One => 1,
            Takes::Keys(n) | Takes::Velocities { count: n, .. } => n,
        }
    }
}

/// How far apart a kit lays the recordings of one drum, keys.
pub const TAKE_KEYS: u8 = 24;

/// The FSBS guitars' takes: four of each note, soft under 93 below G4.
const FSBS: Takes = Takes::Velocities { count: 4, layers: &[93] };

const fn melodic(program: u8, file: &'static str, range: (u8, u8)) -> Voice {
    Voice { program, percussion: false, file, bank: 0, preset: 0, keys: &[], range, takes: Takes::One, direct: false, legato: None }
}

/// MuldjordKit's drums on General MIDI's keys: its two kicks, its snare
/// and the snare struck at rest for a side stick, its hats, rides with
/// their bells, crashes and china, and its four toms for General MIDI's
/// six.
const MULDJORD: &[(u8, u8)] = &[
    (35, 49),
    (36, 48),
    (37, 65),
    (38, 50),
    (40, 51),
    (42, 52),
    (44, 52),
    (46, 53),
    (49, 58),
    (51, 54),
    (52, 60),
    (53, 55),
    (55, 59),
    (57, 59),
    (59, 56),
    (50, 61),
    (48, 62),
    (47, 62),
    (45, 63),
    (43, 64),
    (41, 64),
];

/// Swirly Drums' drums on General MIDI's keys: its very high tom for the
/// high tom, which it lacks; every other drum it holds is on its own key.
const SWIRLY: &[(u8, u8)] = &[(50, 48)];

/// Every program another bank plays: FreePats' Fender guitars sampled
/// clean and jazz, and recorded at the jack for the overdriven and the
/// distorted guitar, which sound through their parts' rigs, entered past
/// the pick for legato (`banks/fsbs.py`); its fingered bass, tenor sax,
/// upright piano and drawbar organ; Lars Muldjord's rock kit (CC BY 4.0:
/// the music must credit him); Karoryfer's Swirly Drums for the brush
/// kit, its drums struck with brushes and its cymbals as well, four
/// recordings of every stroke (`banks/swirly.py`), and only the drums a
/// piece plays: the kick, the snare's centre, the hat's foot, the crash,
/// the ride and the toms; a harmonica built from VCSL's Hohner
/// Special 20s (`banks/harmonica.py`); and Karoryfer's Pastabass, picked,
/// built from its SFZ mapping (`banks/sfz.py`). The
/// fingered bass is a round floor from the low D to the A over the low
/// E, a line over it folded down; the picked starts at the C♯ over the
/// low E and reaches high, so it suits a part that sits high, a solo.
/// The brass — the trumpet open and under the harmon mute, the tenor
/// trombone, the tuba, the horn, each sustained — are VSCO 2 Community
/// Edition's, built by `banks/install.py` from its SFZ mappings: the
/// default bank's muted trumpet, a blues band's lead, was heard as bad.
pub const VOICES: &[Voice] = &[
    melodic(0, "upright-piano-kw.sf2", (21, 108)),
    melodic(16, "drawbar-organ.sf2", (33, 98)),
    Voice { program: 22, percussion: false, file: "harmonica.sf2", bank: 0, preset: 22, keys: &[], range: (0, 127), takes: Takes::One, direct: false, legato: None },
    Voice { takes: FSBS, ..melodic(26, "fsbs-jazz.sf2", (35, 86)) },
    Voice { takes: FSBS, ..melodic(27, "fsbs-clean.sf2", (35, 86)) },
    Voice { takes: FSBS, direct: true, legato: Some(1), ..melodic(29, "guitar-di.sf2", (35, 86)) },
    Voice { takes: FSBS, direct: true, legato: Some(1), ..melodic(30, "guitar-di.sf2", (35, 86)) },
    melodic(33, "yr-finger-bass.sf2", (26, 45)),
    Voice { program: 34, percussion: false, file: "pasta-bass.sf2", bank: 0, preset: 34, keys: &[], range: (37, 85), takes: Takes::One, direct: false, legato: None },
    melodic(56, "vsco-trumpet.sf2", (53, 84)),
    melodic(57, "vsco-trombone.sf2", (34, 65)),
    melodic(58, "vsco-tuba.sf2", (29, 62)),
    melodic(59, "vsco-muted-trumpet.sf2", (58, 81)),
    melodic(60, "vsco-horn.sf2", (33, 77)),
    melodic(66, "tenor-sax.sf2", (43, 89)),
    Voice { program: 16, percussion: true, file: "muldjord-kit.sf2", bank: 0, preset: 0, keys: MULDJORD, range: (0, 127), takes: Takes::One, direct: false, legato: None },
    Voice { program: 40, percussion: true, file: "swirly-kit.sf2", bank: 0, preset: 0, keys: SWIRLY, range: (0, 127), takes: Takes::Keys(4), direct: false, legato: None },
];

/// Whether `program` sustains a tone a player shapes — bends, scoops,
/// vibrato: bowed strings, voices, brass, reeds, pipes, the harmonica,
/// the overdriven and the distorted guitar; a struck or plucked tone is
/// not shaped once it sounds.
pub fn sings(program: u8) -> bool {
    matches!(program, 22 | 29 | 30 | 40..=44 | 48..=49 | 52..=54 | 56..=79 | 110 | 111)
}

/// Whether the bank's samples of `program` carry a vibrato recorded in
/// them — the flute's, the violin's and the fiddle's, the strings' and
/// the choir's on GeneralUser (`proofs/research/ornaments-findings.md`)
/// — so a player adds none: a second one beats against it.
pub fn vibrato_recorded(program: u8) -> bool {
    matches!(program, 40 | 48 | 49 | 52 | 73 | 110)
}

/// The voice for `program`, where another bank plays it.
pub fn voice(program: u8, percussion: bool) -> Option<&'static Voice> {
    VOICES.iter().find(|v| v.program == program && v.percussion == percussion)
}

/// The key `voice` strikes for General MIDI's `key`: a kit's own key for
/// the drum, a melodic note folded by octaves into the voice's range, and
/// on it the `take`-th recording where the voice lays its takes on keys.
pub fn key(voice: &Voice, key: u8, take: u8) -> u8 {
    let lift = match voice.takes {
        Takes::Keys(n) => take % n * TAKE_KEYS,
        _ => 0,
    };
    if voice.percussion {
        let drum = voice.keys.iter().find(|(gm, _)| *gm == key).map_or(key, |(_, k)| *k);
        return drum + lift;
    }
    let (lo, hi) = voice.range;
    let mut k = key;
    while k < lo && k + 12 <= hi {
        k += 12;
    }
    while k > hi && k >= lo + 12 {
        k -= 12;
    }
    k + lift
}

/// The velocity that strikes `voice`'s `take`-th recording at about
/// `vel`: the nearest of its dynamic layer's velocities that names the
/// take, where the voice chooses its takes by velocity; `vel` itself
/// otherwise. A nudge of a step or two is a fraction of a decibel.
pub fn velocity(voice: &Voice, vel: u8, take: u8) -> u8 {
    let Takes::Velocities { count, layers } = voice.takes else {
        return vel;
    };
    let lo = layers.iter().copied().filter(|l| *l <= vel).max().unwrap_or(1);
    let hi = layers.iter().copied().filter(|l| *l > vel).min().map_or(127, |l| l - 1);
    (lo..=hi).filter(|v| v % count == take % count).min_by_key(|v| ((*v as i32 - vel as i32).abs(), *v)).unwrap_or(vel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_past_a_voice_is_struck_an_octave_in() {
        let bass = voice(34, false).unwrap();
        let (lo, hi) = bass.range;
        for pitch in 0..=127u8 {
            let k = key(bass, pitch, 0);
            assert!((lo..=hi).contains(&k), "{pitch} struck at {k}");
            assert_eq!(k % 12, pitch % 12, "{pitch} left its pitch class");
            if (lo..=hi).contains(&pitch) {
                assert_eq!(k, pitch);
            } else {
                assert!(k < lo + 12 || k + 12 > hi, "{pitch} folded past the nearest octave in");
            }
        }
    }

    #[test]
    fn a_take_by_velocity_stays_in_its_layer_and_near() {
        let guitar = voice(30, false).unwrap();
        for vel in 1..=127u8 {
            let struck: Vec<u8> = (0..4).map(|n| velocity(guitar, vel, n)).collect();
            let mut takes: Vec<u8> = struck.iter().map(|v| v % 4).collect();
            takes.sort();
            takes.dedup();
            assert_eq!(takes.len(), 4, "{vel}: {struck:?}");
            assert!(struck.iter().all(|v| (*v >= 93) == (vel >= 93) && (*v as i32 - vel as i32).abs() <= 3 && *v >= 1), "{vel}: {struck:?}");
        }
        let bass = voice(34, false).unwrap();
        assert_eq!(velocity(bass, 90, 3), 90);
    }

    #[test]
    fn a_kit_strikes_its_own_keys() {
        let kit = voice(16, true).unwrap();
        assert_eq!(key(kit, 36, 0), 48);
        assert_eq!(key(kit, 90, 0), 90);
    }

    #[test]
    fn a_drum_s_recordings_lie_on_keys_of_their_own() {
        let kit = voice(40, true).unwrap();
        let mut struck: Vec<u8> = [36, 38, 41, 44, 45, 47, 49, 50, 51].iter().flat_map(|k| (0..kit.takes.count()).map(|n| key(kit, *k, n))).collect();
        let all = struck.len();
        struck.sort();
        struck.dedup();
        assert_eq!(struck.len(), all);
        assert!(struck.iter().all(|k| *k <= 127));
        assert_eq!(key(kit, 51, kit.takes.count()), key(kit, 51, 0));
    }
}
