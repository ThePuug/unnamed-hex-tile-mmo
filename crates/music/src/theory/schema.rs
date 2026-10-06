//! The progressions a mode is heard in, four bars at a time: the loops
//! that stay open, ending away from the tonic, and the cadences that
//! close on it; and the twelve-bar, three rows that are one chorus.
//! Chords come in phrases, not one at a time; a walk from chord to
//! chord has no direction, and the ear hears that it has none. Over a
//! tonic drone only some chords can be held at all, and every schema
//! is filtered by that before a droned piece may draw it.

use super::{interval_class, Chord, Key, Mode, DIATONIC, MAJOR};

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
/// ♭VII — i–iv–♭VII–i above all. Never ♭VI–♭VII–i, which is rock's and
/// the ballad's, not the folk's.
pub const SCHEMATA: [Schema; 6] = [
    Schema { name: "subtonic shuttle", modes: &[Aeolian, Dorian], roots: [0, 6, 0, 6], closed: false, alters: [DIATONIC; 4] },
    Schema { name: "lament", modes: &[Aeolian], roots: [0, 6, 5, 4], closed: false, alters: [DIATONIC; 4] },
    Schema { name: "plagal shuttle", modes: &[Aeolian, Dorian, Hijaz], roots: [0, 3, 0, 3], closed: false, alters: [DIATONIC; 4] },
    Schema { name: "subtonic close", modes: &[Aeolian, Dorian], roots: [0, 6, 3, 0], closed: true, alters: [DIATONIC; 4] },
    Schema { name: "folk close", modes: &[Aeolian, Dorian], roots: [0, 3, 6, 0], closed: true, alters: [DIATONIC; 4] },
    Schema { name: "plagal close", modes: &[Aeolian, Dorian, Hijaz], roots: [0, 3, 3, 0], closed: true, alters: [DIATONIC; 4] },
];

/// The twelve-bar blues as three rows of one chorus, each a choice: the
/// tonic's row, plain or with the quick change to IV; the
/// subdominant's, IV falling back to I; and the turn home, V through
/// IV to I, the V held a bar longer or not, or in the Aeolian the
/// flat sixth's major seventh falling to the v — the minor turn of a
/// night blues. A minor blues, on sevenths:
/// in Dorian the tonic and v are minor sevenths and IV is a dominant
/// seventh, in Aeolian all three are minor, and either is diatonic to
/// the last tone, so the blue notes are the mode's own.
pub const TWELVE_BAR: [&[Schema]; 3] = [
    &[
        Schema { name: "tonic row", modes: &[Dorian, Aeolian], roots: [0, 0, 0, 0], closed: false, alters: [DIATONIC; 4] },
        Schema { name: "quick change", modes: &[Dorian, Aeolian], roots: [0, 3, 0, 0], closed: false, alters: [DIATONIC; 4] },
    ],
    &[Schema { name: "subdominant row", modes: &[Dorian, Aeolian], roots: [3, 3, 0, 0], closed: false, alters: [DIATONIC; 4] }],
    &[
        Schema { name: "turn home", modes: &[Dorian, Aeolian], roots: [4, 3, 0, 0], closed: true, alters: [DIATONIC; 4] },
        Schema { name: "turn home, the dominant held", modes: &[Dorian, Aeolian], roots: [4, 4, 0, 0], closed: true, alters: [DIATONIC; 4] },
        Schema { name: "minor turn", modes: &[Aeolian], roots: [5, 4, 0, 0], closed: true, alters: [DIATONIC; 4] },
    ],
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

/// The ballad's progressions, in the minor with no drone, by the part
/// of the song they serve. A verse opens on the tonic and stays on it
/// longest, swinging slowly to one or two chords and back — i to VI, i
/// down through VII to VI, i through III and VII to the minor v, the
/// tonic held as the bass falls to VI and the major V — and comes home
/// through III and VII, holds the tonic till VII, or falls through VI
/// to the major V. A chorus moves a chord a bar and opens off the tonic
/// as often as on it, on the sixth; it walks down i, VII, VI to the
/// major V, the Andalusian cadence, and comes home by the sixth and
/// seventh, the cadence hard rock and metal end on, or through iv and
/// the major V. The major V borrows its third from the harmonic minor,
/// the leading tone a metal ballad turns on and a folk tune never
/// has. The climax takes a progression of its own, heard nowhere before
/// it: the tonic shuttling with the seventh and sixth. Every chord a
/// major or minor triad: the minor's second is diminished and never
/// stands.
pub struct Song {
    pub verse: &'static [Schema],
    pub chorus: &'static [Schema],
    pub climax: &'static [Schema],
}

pub const BALLAD: Song = Song {
    verse: &[
        Schema { name: "tonic and sixth", modes: &[Aeolian], roots: [0, 0, 5, 5], closed: false, alters: [DIATONIC; 4] },
        Schema { name: "subtonic descent", modes: &[Aeolian], roots: [0, 0, 6, 5], closed: false, alters: [DIATONIC; 4] },
        Schema { name: "relative turn", modes: &[Aeolian], roots: [0, 2, 6, 4], closed: false, alters: [DIATONIC; 4] },
        Schema { name: "falling to the dominant", modes: &[Aeolian], roots: [0, 0, 5, 4], closed: false, alters: [DIATONIC, DIATONIC, DIATONIC, MAJOR] },
        Schema { name: "relative home", modes: &[Aeolian], roots: [0, 2, 6, 0], closed: true, alters: [DIATONIC; 4] },
        Schema { name: "tonic held home", modes: &[Aeolian], roots: [0, 0, 6, 0], closed: true, alters: [DIATONIC; 4] },
        Schema { name: "home through the dominant", modes: &[Aeolian], roots: [0, 5, 4, 0], closed: true, alters: [DIATONIC, DIATONIC, MAJOR, DIATONIC] },
    ],
    chorus: &[
        Schema { name: "lift", modes: &[Aeolian], roots: [5, 6, 0, 6], closed: false, alters: [DIATONIC; 4] },
        Schema { name: "climb", modes: &[Aeolian], roots: [0, 5, 2, 6], closed: false, alters: [DIATONIC; 4] },
        Schema { name: "andalusian", modes: &[Aeolian], roots: [0, 6, 5, 4], closed: false, alters: [DIATONIC, DIATONIC, DIATONIC, MAJOR] },
        Schema { name: "lift home", modes: &[Aeolian], roots: [5, 6, 0, 0], closed: true, alters: [DIATONIC; 4] },
        Schema { name: "subdominant home", modes: &[Aeolian], roots: [3, 5, 6, 0], closed: true, alters: [DIATONIC; 4] },
        Schema { name: "dominant home", modes: &[Aeolian], roots: [0, 3, 4, 0], closed: true, alters: [DIATONIC, DIATONIC, MAJOR, DIATONIC] },
    ],
    climax: &[
        Schema { name: "outro shuttle", modes: &[Aeolian], roots: [0, 6, 5, 6], closed: false, alters: [DIATONIC; 4] },
        Schema { name: "outro close", modes: &[Aeolian], roots: [0, 5, 6, 0], closed: true, alters: [DIATONIC; 4] },
    ],
};

/// Speed metal's progressions as Helloween wrote them in 1985–88, in
/// the minor, by the part of the song they serve; power chords, so a
/// chord's third is the lead's to choose. A verse is Aeolian: the tonic
/// held and swinging to the seventh, rocking with the relative major,
/// or turning through the sixth and seventh to the major V. A
/// pre-chorus lifts: the relative major's V and I rocked, the sixth and
/// seventh climbed two bars each, or the sixth, third and seventh to the
/// major V. A chorus is the minor's — the tonic held into the sixth and
/// seventh, Halloween's turn through the seventh and third, Starlight's
/// way home through iv and the sixth, How Many Tears' sixth and seventh
/// to the major V and home — or bright, on the relative major: its I, V,
/// vi and iii, the Pachelbel turn of Guardians and Eagle Fly Free, home
/// through its IV, I and V to the minor tonic. Under the solos the other
/// guitar holds a pedal on one root or two. The major V is a cadence's
/// colour, never the riff's mode.
pub struct Speed {
    pub verse: &'static [Schema],
    pub pre: &'static [Schema],
    pub chorus: &'static [Schema],
    pub bright: &'static [Schema],
    pub solo: &'static [Schema],
}

pub const SPEED: Speed = Speed {
    verse: &[
        Schema { name: "subtonic pedal", modes: &[Aeolian], roots: [0, 0, 6, 6], closed: false, alters: [DIATONIC; 4] },
        Schema { name: "relative swing", modes: &[Aeolian], roots: [0, 2, 0, 5], closed: false, alters: [DIATONIC; 4] },
        Schema { name: "relative rock", modes: &[Aeolian], roots: [0, 6, 2, 6], closed: false, alters: [DIATONIC; 4] },
        Schema { name: "turn to the dominant", modes: &[Aeolian], roots: [0, 5, 6, 4], closed: false, alters: [DIATONIC, DIATONIC, DIATONIC, MAJOR] },
        Schema { name: "relative home", modes: &[Aeolian], roots: [0, 6, 2, 0], closed: true, alters: [DIATONIC; 4] },
        Schema { name: "subtonic home", modes: &[Aeolian], roots: [0, 6, 6, 0], closed: true, alters: [DIATONIC; 4] },
    ],
    pre: &[
        Schema { name: "relative shuttle", modes: &[Aeolian], roots: [6, 2, 6, 2], closed: false, alters: [DIATONIC; 4] },
        Schema { name: "the climb", modes: &[Aeolian], roots: [5, 5, 6, 6], closed: false, alters: [DIATONIC; 4] },
        Schema { name: "climb to the dominant", modes: &[Aeolian], roots: [5, 2, 6, 4], closed: false, alters: [DIATONIC, DIATONIC, DIATONIC, MAJOR] },
    ],
    chorus: &[
        Schema { name: "the drive", modes: &[Aeolian], roots: [0, 0, 5, 6], closed: false, alters: [DIATONIC; 4] },
        Schema { name: "halloween turn", modes: &[Aeolian], roots: [6, 2, 0, 4], closed: false, alters: [DIATONIC; 4] },
        Schema { name: "starlight home", modes: &[Aeolian], roots: [0, 3, 5, 0], closed: true, alters: [DIATONIC; 4] },
        Schema { name: "tears home", modes: &[Aeolian], roots: [5, 6, 4, 0], closed: true, alters: [DIATONIC, DIATONIC, MAJOR, DIATONIC] },
    ],
    bright: &[
        Schema { name: "pachelbel", modes: &[Aeolian], roots: [2, 6, 0, 4], closed: false, alters: [DIATONIC; 4] },
        Schema { name: "relative lift", modes: &[Aeolian], roots: [5, 2, 6, 2], closed: false, alters: [DIATONIC; 4] },
        Schema { name: "relative close", modes: &[Aeolian], roots: [5, 2, 6, 0], closed: true, alters: [DIATONIC; 4] },
    ],
    solo: &[
        Schema { name: "solo pedal", modes: &[Aeolian], roots: [0, 5, 0, 6], closed: false, alters: [DIATONIC; 4] },
        Schema { name: "solo close", modes: &[Aeolian], roots: [5, 5, 6, 0], closed: true, alters: [DIATONIC; 4] },
    ],
};

/// A part of the song's schemata: the open ones and the closed.
pub fn split(schemata: &'static [Schema]) -> (Vec<&'static Schema>, Vec<&'static Schema>) {
    (schemata.iter().filter(|s| !s.closed).collect(), schemata.iter().filter(|s| s.closed).collect())
}

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

    /// The twelve-bar's rows are the blues' three chords and the minor
    /// turn's flat sixth, every row falling back to the tonic, the first
    /// two open and the last closed, and every row offering both minor
    /// modes a choice; the flat sixth only where it is a major chord.
    #[test]
    fn the_twelve_bar_is_three_chords_and_a_turn_home() {
        for (r, row) in TWELVE_BAR.iter().enumerate() {
            for s in row.iter() {
                assert!(s.roots.iter().all(|d| matches!(d, 0 | 3 | 4) || (*d == 5 && !s.modes.contains(&Dorian))), "{}: a chord off the blues", s.name);
                assert_eq!(s.roots[3], 0, "{}: does not fall back to the tonic", s.name);
                assert_eq!(s.closed, r == 2, "{}: its end belies its row", s.name);
            }
            for mode in [Dorian, Aeolian] {
                assert!(row.iter().any(|s| s.modes.contains(&mode)), "row {r} offers {mode:?} nothing");
            }
        }
        assert!(TWELVE_BAR[1].iter().all(|s| s.roots[0] == 3));
        assert!(TWELVE_BAR[2].iter().all(|s| matches!(s.roots[0], 4 | 5)));
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

    /// Every ballad schema closes as it says, every chord a major or
    /// minor triad; every verse opens on the tonic, some chorus opens off
    /// it, and every part of the song has a question and an answer.
    #[test]
    fn every_part_of_the_ballad_asks_and_answers() {
        let key = Key::new("E", Aeolian);
        for part in [BALLAD.verse, BALLAD.chorus, BALLAD.climax] {
            for s in part {
                assert_eq!(s.roots[3] == 0, s.closed, "{}", s.name);
                for k in 0..4 {
                    let chord = s.chord(k, 3);
                    let under = key.under(chord);
                    let tones: Vec<u8> = chord.degrees().iter().map(|d| under.pitch(*d, 4)).collect();
                    assert!(matches!((tones[1] - tones[0], tones[2] - tones[1]), (3, 4) | (4, 3)), "{}: {} is no major or minor triad", s.name, chord.root);
                }
            }
            let (open, closed) = split(part);
            assert!(!open.is_empty() && !closed.is_empty());
        }
        assert!(BALLAD.verse.iter().all(|s| s.roots[0] == 0));
        assert!(BALLAD.chorus.iter().any(|s| s.roots[0] != 0));
        // The major V, in a verse and in a chorus.
        for part in [BALLAD.verse, BALLAD.chorus] {
            assert!(part.iter().any(|s| (0..4).any(|k| s.roots[k] == 4 && s.alters[k] == MAJOR)));
        }
    }

    /// Every speed metal schema closes as it says and every chord stands a
    /// power chord, its fifth perfect in the bar's key; a pre-chorus never
    /// closes, it lifts into the chorus; every other part asks and answers.
    #[test]
    fn every_part_of_speed_metal_asks_and_answers() {
        let key = Key::new("E", Aeolian);
        for part in [SPEED.verse, SPEED.pre, SPEED.chorus, SPEED.bright, SPEED.solo] {
            for s in part {
                assert_eq!(s.roots[3] == 0, s.closed, "{}", s.name);
                for k in 0..4 {
                    let under = key.under(s.chord(k, 3));
                    assert_eq!(under.pitch(s.roots[k] + 4, 4) - under.pitch(s.roots[k], 4), 7, "{}: no power chord on {}", s.name, s.roots[k]);
                }
            }
        }
        assert!(SPEED.pre.iter().all(|s| !s.closed));
        for part in [SPEED.verse, SPEED.chorus, SPEED.bright, SPEED.solo] {
            let (open, closed) = split(part);
            assert!(!open.is_empty() && !closed.is_empty());
        }
    }

    /// A progression in a mode belongs to one style: the folk's — the
    /// overworld's and the horo's, one world — the blues' or the
    /// ballad's, so no two styles are heard turning the same way; a
    /// progression is its chords as they sound, so the Andalusian's major
    /// V is not the lament's minor v.
    #[test]
    fn no_progression_crosses_styles() {
        let folk: Vec<&Schema> = SCHEMATA.iter().chain(HORO.iter()).collect();
        let blues: Vec<&Schema> = TWELVE_BAR.iter().flat_map(|row| row.iter()).collect();
        let ballad: Vec<&Schema> = [BALLAD.verse, BALLAD.chorus, BALLAD.climax].into_iter().flatten().collect();
        let speed: Vec<&Schema> = [SPEED.verse, SPEED.pre, SPEED.chorus, SPEED.bright, SPEED.solo].into_iter().flatten().collect();
        let styles = [("folk", folk), ("blues", blues), ("ballad", ballad), ("speed", speed)];
        for (i, (a, ours)) in styles.iter().enumerate() {
            for (b, theirs) in &styles[i + 1..] {
                for s in ours {
                    for t in theirs.iter().filter(|t| t.roots == s.roots && t.alters == s.alters) {
                        assert!(!s.modes.iter().any(|m| t.modes.contains(m)), "{a}'s {} is {b}'s {}", s.name, t.name);
                    }
                }
            }
        }
    }

    /// Hijaz holds only its I and iv over the drone.
    #[test]
    fn hijaz_keeps_its_colour_for_the_melody() {
        assert_eq!(holdable_roots(&Key::new("D", Hijaz)), vec![0, 3]);
        assert_eq!(holdable_roots(&Key::new("D", Aeolian)), vec![0, 2, 3, 4, 5, 6]);
    }
}
