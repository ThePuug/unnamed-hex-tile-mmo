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
}

const fn melodic(program: u8, file: &'static str, range: (u8, u8)) -> Voice {
    Voice { program, percussion: false, file, bank: 0, preset: 0, keys: &[], range }
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

/// Every program another bank plays: FreePats' Fender guitars sampled
/// clean, jazz and through two amps; its tenor sax, upright piano and
/// drawbar organ; Lars Muldjord's rock kit (CC BY 4.0: the music must
/// credit him); a harmonica built from VCSL's Hohner Special 20s
/// (`banks/harmonica.py`); and Karoryfer's basses built from their SFZ
/// mappings (`banks/sfz.py`). The two distorted guitars are different
/// amps, so a double-tracked pair is two guitars. The fingered bass
/// reaches the bass's low E and under; the picked starts at the C♯ over
/// it, so it suits a part that sits high.
pub const VOICES: &[Voice] = &[
    melodic(0, "upright-piano-kw.sf2", (21, 108)),
    melodic(16, "drawbar-organ.sf2", (33, 98)),
    Voice { program: 22, percussion: false, file: "harmonica.sf2", bank: 0, preset: 22, keys: &[], range: (0, 127) },
    melodic(26, "fsbs-jazz.sf2", (35, 86)),
    melodic(27, "fsbs-clean.sf2", (35, 86)),
    melodic(29, "fsbs-dist2.sf2", (35, 86)),
    melodic(30, "fsbs-dist1.sf2", (35, 86)),
    Voice { program: 33, percussion: false, file: "fashion-bass.sf2", bank: 0, preset: 33, keys: &[], range: (18, 69) },
    Voice { program: 34, percussion: false, file: "pasta-bass.sf2", bank: 0, preset: 34, keys: &[], range: (37, 85) },
    melodic(66, "tenor-sax.sf2", (43, 89)),
    Voice { program: 16, percussion: true, file: "muldjord-kit.sf2", bank: 0, preset: 0, keys: MULDJORD, range: (0, 127) },
];

/// The voice for `program`, where another bank plays it.
pub fn voice(program: u8, percussion: bool) -> Option<&'static Voice> {
    VOICES.iter().find(|v| v.program == program && v.percussion == percussion)
}

/// The key `voice` strikes for General MIDI's `key`: a kit's own key for
/// the drum, a melodic note folded by octaves into the voice's range.
pub fn key(voice: &Voice, key: u8) -> u8 {
    if voice.percussion {
        return voice.keys.iter().find(|(gm, _)| *gm == key).map_or(key, |(_, k)| *k);
    }
    let (lo, hi) = voice.range;
    let mut k = key;
    while k < lo && k + 12 <= hi {
        k += 12;
    }
    while k > hi && k >= lo + 12 {
        k -= 12;
    }
    k
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_past_a_voice_is_struck_an_octave_in() {
        let bass = voice(34, false).unwrap();
        let (lo, hi) = bass.range;
        for pitch in 0..=127u8 {
            let k = key(bass, pitch);
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
    fn a_kit_strikes_its_own_keys() {
        let kit = voice(16, true).unwrap();
        assert_eq!(key(kit, 36), 48);
        assert_eq!(key(kit, 90), 90);
    }
}
