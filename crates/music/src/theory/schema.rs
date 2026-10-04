//! The progressions a mode is heard in, four bars at a time: the loops
//! that stay open, ending away from the tonic, and the cadences that
//! close on it; and the twelve-bar, three rows that are one chorus.
//! Chords come in phrases, not one at a time; a walk from chord to
//! chord has no direction, and the ear hears that it has none. Over a
//! tonic drone only some chords can be held at all, and every schema
//! is filtered by that before a droned piece may draw it.

use super::{interval_class, Chord, Key, Mode};

#[derive(Clone, Copy, Debug)]
pub struct Schema {
    pub name: &'static str,
    pub modes: &'static [Mode],
    /// The root degree of each bar's chord.
    pub roots: [i32; 4],
    /// Whether the phrase on it closes: the tune comes home at its end,
    /// else rests on a tone of the last chord.
    pub closed: bool,
}

use Mode::{Aeolian, Dorian, Hijaz};

/// The modal loops and cadences: the Aeolian's shuttles between i and
/// ♭VII and its walk down through ♭VI, the Dorian's to its major IV, the
/// plagal ones every minor mode shares, and the cadences that bring
/// each home.
pub const SCHEMATA: [Schema; 8] = [
    Schema { name: "aeolian shuttle", modes: &[Aeolian], roots: [0, 6, 5, 6], closed: false },
    Schema { name: "subtonic shuttle", modes: &[Aeolian, Dorian], roots: [0, 6, 0, 6], closed: false },
    Schema { name: "lament", modes: &[Aeolian], roots: [0, 6, 5, 4], closed: false },
    Schema { name: "plagal shuttle", modes: &[Aeolian, Dorian, Hijaz], roots: [0, 3, 0, 3], closed: false },
    Schema { name: "aeolian cadence", modes: &[Aeolian], roots: [0, 5, 6, 0], closed: true },
    Schema { name: "subtonic close", modes: &[Aeolian, Dorian], roots: [0, 6, 3, 0], closed: true },
    Schema { name: "dorian close", modes: &[Dorian], roots: [0, 3, 6, 0], closed: true },
    Schema { name: "plagal close", modes: &[Aeolian, Dorian, Hijaz], roots: [0, 3, 3, 0], closed: true },
];

/// The twelve-bar blues as three rows of one chorus, each a choice: the
/// tonic's row, plain or with the quick change to IV; the
/// subdominant's, IV falling back to I; and the turn home, V through
/// IV to I, the V held a bar longer or not. A minor blues, on sevenths:
/// in Dorian the tonic and v are minor sevenths and IV is a dominant
/// seventh, in Aeolian all three are minor, and either is diatonic to
/// the last tone, so the blue notes are the mode's own.
pub const TWELVE_BAR: [&[Schema]; 3] = [
    &[
        Schema { name: "tonic row", modes: &[Dorian, Aeolian], roots: [0, 0, 0, 0], closed: false },
        Schema { name: "quick change", modes: &[Dorian, Aeolian], roots: [0, 3, 0, 0], closed: false },
    ],
    &[Schema { name: "subdominant row", modes: &[Dorian, Aeolian], roots: [3, 3, 0, 0], closed: false }],
    &[
        Schema { name: "turn home", modes: &[Dorian, Aeolian], roots: [4, 3, 0, 0], closed: true },
        Schema { name: "turn home, the dominant held", modes: &[Dorian, Aeolian], roots: [4, 4, 0, 0], closed: true },
    ],
];

/// The fight's progressions, with no drone under them to spare: the
/// minor's shuttle to its flat sixth and its walk home through the
/// sixth and seventh; Hijaz's shuttle to its flat second, the Ottoman
/// and Balkan colour and the minor second a fight turns on, and its
/// close down through that second to the tonic; and the plagal close
/// both share.
pub const FIGHT: [Schema; 6] = [
    Schema { name: "war shuttle", modes: &[Aeolian], roots: [0, 5, 0, 5], closed: false },
    Schema { name: "descent", modes: &[Aeolian], roots: [0, 6, 5, 6], closed: false },
    Schema { name: "hijaz shuttle", modes: &[Hijaz], roots: [0, 1, 0, 1], closed: false },
    Schema { name: "war close", modes: &[Aeolian], roots: [0, 5, 6, 0], closed: true },
    Schema { name: "phrygian close", modes: &[Hijaz], roots: [0, 3, 1, 0], closed: true },
    Schema { name: "plagal close", modes: &[Aeolian, Hijaz], roots: [0, 3, 3, 0], closed: true },
];

/// The fight's schemata a mode may use: the open ones and the closed.
pub fn fight_schemata(mode: Mode) -> (Vec<&'static Schema>, Vec<&'static Schema>) {
    let fits: Vec<&Schema> = FIGHT.iter().filter(|s| s.modes.contains(&mode)).collect();
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

    /// The twelve-bar's rows are the blues' three chords, every row
    /// falling back to the tonic, the first two open and the last
    /// closed, in both minor modes.
    #[test]
    fn the_twelve_bar_is_three_chords_and_a_turn_home() {
        for (r, row) in TWELVE_BAR.iter().enumerate() {
            for s in row.iter() {
                assert!(s.roots.iter().all(|d| matches!(d, 0 | 3 | 4)), "{}: a chord off the blues", s.name);
                assert_eq!(s.roots[3], 0, "{}: does not fall back to the tonic", s.name);
                assert_eq!(s.closed, r == 2, "{}: its end belies its row", s.name);
                assert!(s.modes.contains(&Dorian) && s.modes.contains(&Aeolian));
            }
        }
        assert!(TWELVE_BAR[1].iter().all(|s| s.roots[0] == 3));
        assert!(TWELVE_BAR[2].iter().all(|s| s.roots[0] == 4));
    }

    /// Every fight schema opens on the tonic and closes as it says,
    /// every chord in it a triad with no tritone and not augmented, and
    /// each of the fight's modes has a question and an answer.
    #[test]
    fn every_fight_has_a_question_and_an_answer() {
        for s in &FIGHT {
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
            let (open, closed) = fight_schemata(mode);
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
