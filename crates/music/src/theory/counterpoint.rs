//! What a line owes its listener, and what two lines owe each other.
//! A leap is answered: after a skip of a fourth or more the line turns
//! back, steps on, or stays, and never leaps on the same way, since the
//! ear expects the gap to be filled and hears two leaps one way as a
//! line falling off its shape. Two tunes are two: they never move in
//! parallel unisons, octaves, fifths or fourths from one strong beat
//! to the next, which would fuse them into one thickened line.

use super::interval_class;

/// A skip of this many degrees or more is a leap.
pub const LEAP: i32 = 3;

/// Whether two pitches sound a perfect interval: a unison or octave,
/// a fifth or its inversion the fourth.
pub fn perfect(a: u8, b: u8) -> bool {
    matches!(interval_class(a, b), 0 | 5)
}

/// The index of the first tone that leaps on the same way as the leap
/// before it, or None where every leap is answered. The last tone of a
/// line has nothing to answer.
pub fn unrecovered_leap(degrees: &[i32]) -> Option<usize> {
    for i in 1..degrees.len().saturating_sub(1) {
        let leap = degrees[i] - degrees[i - 1];
        if leap.abs() < LEAP {
            continue;
        }
        let next = degrees[i + 1] - degrees[i];
        if next.abs() >= LEAP && next.signum() == leap.signum() {
            return Some(i + 1);
        }
    }
    None
}

/// The tick at which two lines, each given as `(tick, pitch)` on the
/// strong beats they sound on, move in parallel perfects from the beat
/// before, or None. Only beats both lines sound on count, and only
/// where both move.
pub fn parallel_perfects(a: &[(u32, u8)], b: &[(u32, u8)]) -> Option<u32> {
    let mut shared: Vec<(u32, u8, u8)> = a.iter().filter_map(|(t, pa)| b.iter().find(|(tb, _)| tb == t).map(|(_, pb)| (*t, *pa, *pb))).collect();
    shared.sort_by_key(|s| s.0);
    for w in shared.windows(2) {
        let (t0, a0, b0) = w[0];
        let (t1, a1, b1) = w[1];
        let _ = t0;
        if a0 != a1 && b0 != b1 && perfect(a0, b0) && perfect(a1, b1) && (a1 as i32 - a0 as i32).signum() == (b1 as i32 - b0 as i32).signum() {
            return Some(t1);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_leap_never_leaps_on() {
        assert_eq!(unrecovered_leap(&[0, 4, 3, 2]), None);
        assert_eq!(unrecovered_leap(&[0, 4, 4, 2]), None);
        assert_eq!(unrecovered_leap(&[0, 4, 5]), None);
        assert_eq!(unrecovered_leap(&[0, 4, 0]), None);
        assert_eq!(unrecovered_leap(&[0, 4, 7]), Some(2));
        assert_eq!(unrecovered_leap(&[4, 0, -3]), Some(2));
        assert_eq!(unrecovered_leap(&[0, 4]), None);
    }

    #[test]
    fn two_lines_may_not_march_in_octaves() {
        let a = [(0, 62), (240, 64), (480, 65)];
        let octaves = [(0, 74), (240, 76), (480, 77)];
        assert_eq!(parallel_perfects(&a, &octaves), Some(240));
        let thirds = [(0, 65), (240, 67), (480, 69)];
        assert_eq!(parallel_perfects(&a, &thirds), None);
        let held = [(0, 74), (240, 74), (480, 77)];
        assert_eq!(parallel_perfects(&a, &held), None);
    }
}
