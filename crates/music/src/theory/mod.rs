//! Pitch, mode, chord and meter: the arithmetic a piece is written in.
//! Pitches are MIDI numbers; degrees are steps of the mode from its
//! tonic, any integer, so a degree of 7 is the tonic an octave up and -1
//! the leading tone below. A chord may borrow from outside the mode —
//! its third raised for the major V of a minor key — and the key a bar
//! is heard in is the mode with its chord's tones in it (`Key::under`):
//! over that V the melody's seventh is raised too. The vocabularies a piece composes from —
//! the dances, the harmonic schemata, the phrase forms, the melodic
//! shapes and the rules between lines — are the modules beside this.

pub mod counterpoint;
pub mod groove;
pub mod melody;
pub mod phrase;
pub mod schema;

pub const NOTE_NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

/// Seven-note modes by their semitone steps from the tonic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Aeolian,
    Dorian,
    HarmonicMinor,
    /// Phrygian dominant: the augmented second between its second and
    /// third degrees is the Balkan and Ottoman colour.
    Hijaz,
    Ionian,
    Mixolydian,
}

impl Mode {
    pub fn steps(self) -> [u8; 7] {
        match self {
            Mode::Aeolian => [0, 2, 3, 5, 7, 8, 10],
            Mode::Dorian => [0, 2, 3, 5, 7, 9, 10],
            Mode::HarmonicMinor => [0, 2, 3, 5, 7, 8, 11],
            Mode::Hijaz => [0, 1, 4, 5, 7, 8, 10],
            Mode::Ionian => [0, 2, 4, 5, 7, 9, 11],
            Mode::Mixolydian => [0, 2, 4, 5, 7, 9, 10],
        }
    }

    /// What a musician would call the key: the minor modes all read as
    /// "minor" on a tag, the rest by name.
    pub fn label(self) -> &'static str {
        match self {
            Mode::Aeolian | Mode::HarmonicMinor => "minor",
            Mode::Dorian => "Dorian",
            Mode::Hijaz => "Hijaz",
            Mode::Ionian => "major",
            Mode::Mixolydian => "Mixolydian",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Key {
    /// Pitch class of the tonic, 0 = C.
    pub tonic: u8,
    pub mode: Mode,
    /// Semitones each degree is moved from the mode's, by the chord the
    /// key is under; none in the piece's own key.
    pub alter: [i8; 7],
}

impl Key {
    pub fn new(tonic: &str, mode: Mode) -> Self {
        let tonic = NOTE_NAMES.iter().position(|n| *n == tonic).expect("a note name") as u8;
        Key { tonic, mode, alter: [0; 7] }
    }

    /// The key a bar on `chord` is heard in: the mode, with each of the
    /// chord's tones as the chord has it. The piece's key under any
    /// chord, however often it is taken.
    pub fn under(&self, chord: Chord) -> Key {
        let mut alter = [0; 7];
        for (i, a) in chord.alter.iter().enumerate().take(chord.size as usize) {
            alter[(chord.root + 2 * i as i32).rem_euclid(7) as usize] = *a;
        }
        Key { alter, ..*self }
    }

    /// Semitones from the tonic to each degree.
    fn steps(&self) -> [i32; 7] {
        let mode = self.mode.steps();
        std::array::from_fn(|d| mode[d] as i32 + self.alter[d] as i32)
    }

    /// The pitch at `degree` of the mode, with degree 0 in `octave`
    /// (MIDI octaves: 4 holds middle C, 60).
    pub fn pitch(&self, degree: i32, octave: i32) -> u8 {
        let oct = octave + degree.div_euclid(7);
        let d = degree.rem_euclid(7) as usize;
        (12 * (oct + 1) + self.tonic as i32 + self.steps()[d]) as u8
    }

    /// The degree of `pitch` counted from the tonic of octave 4, the
    /// inverse of `pitch(d, 4)`; None off the mode.
    pub fn absolute_degree(&self, pitch: u8) -> Option<i32> {
        let d = self.degree_of(pitch)?;
        let octave = (pitch as i32 - self.tonic as i32 - self.steps()[d]).div_euclid(12) - 1;
        Some(d as i32 + 7 * (octave - 4))
    }

    /// The degree of a pitch in the mode, or None off it.
    pub fn degree_of(&self, pitch: u8) -> Option<usize> {
        let pc = (pitch as i32 - self.tonic as i32).rem_euclid(12);
        self.steps().iter().position(|s| s.rem_euclid(12) == pc)
    }

    pub fn contains(&self, pitch: u8) -> bool {
        self.degree_of(pitch).is_some()
    }

    /// The pitch nearest `pitch` on the mode, ties downward.
    pub fn snap(&self, pitch: u8) -> u8 {
        (0..=6)
            .flat_map(|d| [pitch.saturating_sub(d), pitch.saturating_add(d)])
            .find(|p| self.contains(*p))
            .unwrap()
    }

    pub fn name(&self) -> String {
        format!("{} {}", NOTE_NAMES[self.tonic as usize], self.mode.label())
    }
}

/// A chord by its root degree; its tones are stacked thirds of the
/// mode, each moved by its `alter`, so a chord may borrow from outside
/// the mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chord {
    pub root: i32,
    /// 3 for a triad, 4 for a seventh.
    pub size: u8,
    /// Semitones each tone, root first, is moved from the mode's: the
    /// major V of a minor key raises its third.
    pub alter: [i8; 4],
}

/// No tone moved: the mode's own chord.
pub const DIATONIC: [i8; 4] = [0; 4];
/// The third raised: a major chord where the mode has a minor one.
pub const MAJOR: [i8; 4] = [0, 1, 0, 0];

impl Chord {
    pub fn triad(root: i32) -> Self {
        Chord { root, size: 3, alter: DIATONIC }
    }

    /// The chord's degrees, root first.
    pub fn degrees(&self) -> Vec<i32> {
        (0..self.size as i32).map(|i| self.root + 2 * i).collect()
    }

    /// Whether `pitch` is one of the chord's tones in any octave, as the
    /// chord has them.
    pub fn holds(&self, key: &Key, pitch: u8) -> bool {
        let Some(d) = key.under(*self).degree_of(pitch) else { return false };
        self.degrees().iter().any(|cd| cd.rem_euclid(7) as usize == d)
    }

    /// The chord's pitches within `lo..=hi`, ascending.
    pub fn pitches_within(&self, key: &Key, lo: u8, hi: u8) -> Vec<u8> {
        (lo..=hi).filter(|p| self.holds(key, *p)).collect()
    }
}

/// Interval class between two pitches, 0..=6 semitones.
pub fn interval_class(a: u8, b: u8) -> u8 {
    let d = (a as i32 - b as i32).rem_euclid(12) as u8;
    d.min(12 - d)
}

/// Whether two sustained pitches clash: a semitone, its inversion, or the
/// tritone, the intervals that read as a wrong note held; and a whole
/// tone with both under G3, where it is mud.
pub fn clashes(a: u8, b: u8) -> bool {
    let ic = interval_class(a, b);
    ic == 1 || ic == 6 || (ic == 2 && a.max(b) < 55)
}

/// A bar as groups of eighths: 7/8 as 3+2+2, 4/4 as 2+2+2+2. The first
/// eighth of each group is a strong beat.
#[derive(Clone, Debug)]
pub struct Meter {
    pub groups: Vec<u8>,
}

impl Meter {
    pub fn new(groups: &[u8]) -> Self {
        Meter { groups: groups.to_vec() }
    }

    /// Eighths in a bar.
    pub fn eighths(&self) -> u32 {
        self.groups.iter().map(|g| *g as u32).sum()
    }

    /// Whether the `i`th eighth of a bar opens a group.
    pub fn strong(&self, i: u32) -> bool {
        let mut at = 0;
        for g in &self.groups {
            if at == i {
                return true;
            }
            at += *g as u32;
        }
        false
    }

    /// The eighths that open a group.
    pub fn strong_eighths(&self) -> Vec<u32> {
        (0..self.eighths()).filter(|i| self.strong(*i)).collect()
    }

    /// What a musician would call the meter: in quarters where every
    /// group is two eighths, else in eighths.
    pub fn label(&self) -> String {
        if self.groups.iter().all(|g| *g == 2) {
            format!("{}/4", self.groups.len())
        } else {
            format!("{}/8", self.eighths())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn degrees_wrap_octaves() {
        let k = Key::new("D", Mode::Aeolian);
        assert_eq!(k.pitch(0, 4), 62);
        assert_eq!(k.pitch(7, 4), 74);
        assert_eq!(k.pitch(-1, 4), 60);
        assert_eq!(k.pitch(4, 3), 57);
    }

    #[test]
    fn absolute_degree_inverts_pitch() {
        let k = Key::new("D", Mode::Aeolian);
        for d in -10..20 {
            assert_eq!(k.absolute_degree(k.pitch(d, 4)), Some(d));
        }
    }

    #[test]
    fn membership_and_snap() {
        let k = Key::new("D", Mode::Aeolian);
        assert!(k.contains(70));
        assert!(!k.contains(71));
        assert_eq!(k.snap(71), 70);
        assert_eq!(k.snap(61), 60);
    }

    /// The major V of A minor holds G sharp and not G, and the key under
    /// it raises the seventh for every line over it; taken twice, or
    /// from another chord's key, it is the same key.
    #[test]
    fn a_borrowed_chord_brings_its_tones() {
        let k = Key::new("A", Mode::Aeolian);
        let v = Chord { root: 4, size: 3, alter: MAJOR };
        assert!(v.holds(&k, 68) && v.holds(&k, 64) && v.holds(&k, 71));
        assert!(!v.holds(&k, 67));
        let under = k.under(v);
        assert_eq!(under.pitch(-1, 4), 68);
        assert!(!under.contains(67));
        assert_eq!(under.absolute_degree(68), Some(-1));
        assert_eq!(under.under(v).pitch(-1, 4), 68);
        assert_eq!(under.under(Chord::triad(0)).pitch(-1, 4), 67);
        assert!(Chord::triad(4).holds(&k, 67));
    }

    #[test]
    fn chord_tones() {
        let k = Key::new("D", Mode::Aeolian);
        let i = Chord::triad(0);
        assert!(i.holds(&k, 62) && i.holds(&k, 65) && i.holds(&k, 69));
        assert!(!i.holds(&k, 64));
        assert_eq!(i.pitches_within(&k, 60, 72), vec![62, 65, 69]);
    }

    #[test]
    fn seven_eight_groups() {
        let m = Meter::new(&[3, 2, 2]);
        assert_eq!(m.eighths(), 7);
        assert_eq!(m.strong_eighths(), vec![0, 3, 5]);
        assert_eq!(m.label(), "7/8");
        assert_eq!(Meter::new(&[2, 2, 2, 2]).label(), "4/4");
        assert_eq!(Meter::new(&[3, 3, 3, 3]).label(), "12/8");
    }
}
