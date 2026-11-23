//! The progressions a mode is heard in, four bars at a time: the loops
//! that stay open, ending away from the tonic, and the cadences that
//! close on it.
//! Chords come in phrases, not one at a time; a walk from chord to
//! chord has no direction, and the ear hears that it has none. Over a
//! tonic drone only some chords can be held at all, and every schema
//! is filtered by that before a droned piece may draw it.

use super::{interval_class, Chord, Key, Mode, DIATONIC};

#[derive(Clone, Copy, Debug)]
pub struct Schema {
    pub name: &'static str,
    pub modes: &'static [Mode],
    /// The root degree of each bar's chord.
    pub roots: [i32; 4],
    /// Whether the phrase on it closes: the tune comes home at its end,
    /// else rests on a tone of the last chord.
    pub closed: bool,
    /// How each bar's chord moves its tones off the mode (`Chord::alter`):
    /// a borrowed chord, the major V of a minor key.
    pub alters: [[i8; 4]; 4],
}

impl Schema {
    /// The chord of bar `k`, of `size` tones.
    pub fn chord(&self, k: usize, size: u8) -> Chord {
        Chord { root: self.roots[k], size, alter: self.alters[k] }
    }
}

use Mode::{Aeolian, Dorian, Hijaz};

/// The Balkan loops and cadences, as an arranger sets a tune that is
/// traditionally only droned: the shuttle between i and ♭VII, the
/// commonest Bulgarian pattern; the lament down through ♭VI to v; the
/// plagal ones every minor mode shares; and the closes home through
/// ♭VII — i–iv–♭VII–i above all; the turn through the relative major,
/// i–♭III–♭VII, out to it or home; and the tonic held two bars before
/// it moves, to iv or to ♭VII, which is all Hijaz has over a drone
/// besides its shuttle.
/// Never ♭VI–♭VII–i, which is rock's, not the folk's.
pub const SCHEMATA: [Schema; 12] = [
    Schema { name: "subtonic shuttle", modes: &[Aeolian, Dorian], roots: [0, 6, 0, 6], closed: false, alters: [DIATONIC; 4] },
    Schema { name: "lament", modes: &[Aeolian], roots: [0, 6, 5, 4], closed: false, alters: [DIATONIC; 4] },
    Schema { name: "plagal shuttle", modes: &[Aeolian, Dorian, Hijaz], roots: [0, 3, 0, 3], closed: false, alters: [DIATONIC; 4] },
    Schema { name: "subtonic close", modes: &[Aeolian, Dorian], roots: [0, 6, 3, 0], closed: true, alters: [DIATONIC; 4] },
    Schema { name: "folk close", modes: &[Aeolian, Dorian], roots: [0, 3, 6, 0], closed: true, alters: [DIATONIC; 4] },
    Schema { name: "plagal close", modes: &[Aeolian, Dorian, Hijaz], roots: [0, 3, 3, 0], closed: true, alters: [DIATONIC; 4] },
    Schema { name: "relative rise", modes: &[Aeolian, Dorian], roots: [0, 2, 6, 2], closed: false, alters: [DIATONIC; 4] },
    Schema { name: "plagal turn", modes: &[Aeolian, Dorian], roots: [0, 3, 0, 6], closed: false, alters: [DIATONIC; 4] },
    Schema { name: "held plagal", modes: &[Aeolian, Dorian, Hijaz], roots: [0, 0, 3, 3], closed: false, alters: [DIATONIC; 4] },
    Schema { name: "relative close", modes: &[Aeolian, Dorian], roots: [0, 2, 6, 0], closed: true, alters: [DIATONIC; 4] },
    Schema { name: "subtonic held", modes: &[Aeolian, Dorian], roots: [0, 0, 6, 0], closed: true, alters: [DIATONIC; 4] },
    Schema { name: "held plagal close", modes: &[Aeolian, Dorian, Hijaz], roots: [0, 0, 3, 0], closed: true, alters: [DIATONIC; 4] },
];

/// The horo's progressions, with no drone under them to spare: the
/// minor's shuttle to its flat sixth, and its turn through ♭VII to the
/// iv; Hijaz's shuttle to its flat second, the Ottoman and Balkan
/// colour and the minor second a horo turns on, and its close down
/// through that second to the tonic; the Balkan close home through iv
/// and ♭VII — in Hijaz the minor ♭vii, its standard progression — and
/// the plagal close.
pub const HORO: [Schema; 6] = [
    Schema { name: "war shuttle", modes: &[Aeolian], roots: [0, 5, 0, 5], closed: false, alters: [DIATONIC; 4] },
    Schema { name: "war turn", modes: &[Aeolian], roots: [0, 6, 0, 3], closed: false, alters: [DIATONIC; 4] },
    Schema { name: "hijaz shuttle", modes: &[Hijaz], roots: [0, 1, 0, 1], closed: false, alters: [DIATONIC; 4] },
    Schema { name: "war close", modes: &[Aeolian, Hijaz], roots: [0, 3, 6, 0], closed: true, alters: [DIATONIC; 4] },
    Schema { name: "phrygian close", modes: &[Hijaz], roots: [0, 3, 1, 0], closed: true, alters: [DIATONIC; 4] },
    Schema { name: "plagal close", modes: &[Aeolian, Hijaz], roots: [0, 3, 3, 0], closed: true, alters: [DIATONIC; 4] },
];

/// The horo's schemata a mode may use: the open ones and the closed.
pub fn horo_schemata(mode: Mode) -> (Vec<&'static Schema>, Vec<&'static Schema>) {
    let fits: Vec<&Schema> = HORO.iter().filter(|s| s.modes.contains(&mode)).collect();
    (fits.iter().copied().filter(|s| !s.closed).collect(), fits.iter().copied().filter(|s| s.closed).collect())
}

/// The chords a mode may hold over its tonic drone: a triad with no
/// tritone, not augmented, and no tone a semitone from the tonic. That
/// drops the diminished triad wherever the mode puts it, and in Hijaz
/// every chord that carries the flat second, leaving I and iv, so
/// Hijaz's colour is the melody's.
pub fn holdable_roots(key: &Key) -> Vec<i32> {
    (0..7)
        .filter(|d| {
            let chord = Chord::triad(*d);
            let tones: Vec<u8> = chord.degrees().iter().map(|deg| key.pitch(*deg, 4)).collect();
            let no_tritone = !tones.iter().enumerate().any(|(i, a)| tones[i + 1..].iter().any(|b| interval_class(*a, *b) == 6));
            let not_augmented = !(tones[1] - tones[0] == 4 && tones[2] - tones[1] == 4);
            let clear_of_drone = tones.iter().all(|t| interval_class(*t, key.pitch(0, 4)) != 1);
            no_tritone && not_augmented && clear_of_drone
        })
        .collect()
}

/// The schemata a key may use, its mode's and every chord holdable over
/// its drone: the open ones and the closed.
pub fn schemata_for(key: &Key) -> (Vec<&'static Schema>, Vec<&'static Schema>) {
    let holdable = holdable_roots(key);
    let fits: Vec<&Schema> = SCHEMATA.iter().filter(|s| s.modes.contains(&key.mode) && s.roots.iter().all(|r| holdable.contains(r))).collect();
    (fits.iter().copied().filter(|s| !s.closed).collect(), fits.iter().copied().filter(|s| s.closed).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every schema opens on the tonic, closes as it says, and every
    /// mode a piece may be in has at least one open and one closed.
    #[test]
    fn every_mode_has_a_question_and_an_answer() {
        for s in &SCHEMATA {
            assert_eq!(s.roots[0], 0, "{}: does not open on the tonic", s.name);
            assert_eq!(s.roots[3] == 0, s.closed, "{}: its end belies its kind", s.name);
        }
        for mode in [Aeolian, Dorian, Hijaz] {
            for tonic in ["D", "E", "G", "A", "C"] {
                let (open, closed) = schemata_for(&Key::new(tonic, mode));
                assert!(!open.is_empty() && !closed.is_empty(), "{tonic} {mode:?}");
            }
        }
    }

    /// Every horo schema opens on the tonic and closes as it says,
    /// every chord in it a triad with no tritone and not augmented, and
    /// each of the horo's modes has a question and an answer.
    #[test]
    fn every_horo_has_a_question_and_an_answer() {
        for s in &HORO {
            assert_eq!(s.roots[0], 0, "{}", s.name);
            assert_eq!(s.roots[3] == 0, s.closed, "{}", s.name);
            for mode in s.modes {
                let key = Key::new("D", *mode);
                for r in s.roots {
                    let tones: Vec<u8> = Chord::triad(r).degrees().iter().map(|d| key.pitch(*d, 4)).collect();
                    assert!(!tones.iter().enumerate().any(|(i, a)| tones[i + 1..].iter().any(|b| interval_class(*a, *b) == 6)), "{}: a tritone on {r}", s.name);
                    assert!(!(tones[1] - tones[0] == 4 && tones[2] - tones[1] == 4), "{}: augmented on {r}", s.name);
                }
            }
        }
        for mode in [Aeolian, Hijaz] {
            let (open, closed) = horo_schemata(mode);
            assert!(!open.is_empty() && !closed.is_empty(), "{mode:?}");
        }
    }

    /// Hijaz holds only its I and iv over the drone.
    #[test]
    fn hijaz_keeps_its_colour_for_the_melody() {
        assert_eq!(holdable_roots(&Key::new("D", Hijaz)), vec![0, 3]);
        assert_eq!(holdable_roots(&Key::new("D", Aeolian)), vec![0, 2, 3, 4, 5, 6]);
    }
}
