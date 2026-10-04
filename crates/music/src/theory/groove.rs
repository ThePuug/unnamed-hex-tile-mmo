//! The dances and feels a piece is set on. A meter is never just a
//! count: a groove fixes its grouping of quick and slow beats, the
//! tempo a body takes it at, what the drum strikes where, where the
//! accompaniment lands, and how readily a tune fills a group with
//! steps rather than holding its foot. Eighths are the unit; a group's
//! first eighth is a strong beat.

use super::Meter;
use crate::rng::Rng;

#[derive(Clone, Copy, Debug)]
pub struct Groove {
    pub name: &'static str,
    pub groups: &'static [u8],
    /// Eighths a minute, the band the dance is taken at.
    pub tempo: (i32, i32),
    /// The eighths the drum's low stroke falls on.
    pub dum: &'static [u32],
    /// The eighths its high stroke falls on.
    pub tek: &'static [u32],
    /// The eighths the accompaniment strikes the chord on; its bass is
    /// every group's first.
    pub chord: &'static [u32],
    /// How often a tune fills a group with steps rather than holding its
    /// foot, 0 to 1: a lesnoto leans on the long beat, a râčenica runs.
    pub busy: f32,
}

/// The Balkan dances on quick and slow beats, the slow beat three
/// eighths, the quick two.
pub const BALKAN: [Groove; 5] = [
    Groove { name: "lesnoto", groups: &[3, 2, 2], tempo: (130, 170), dum: &[0], tek: &[3, 5], chord: &[1, 2, 4, 6], busy: 0.35 },
    Groove { name: "râčenica", groups: &[2, 2, 3], tempo: (170, 210), dum: &[0, 4], tek: &[2, 6], chord: &[1, 3, 5, 6], busy: 0.6 },
    Groove { name: "pajduško", groups: &[2, 3], tempo: (160, 200), dum: &[2], tek: &[0], chord: &[1, 3, 4], busy: 0.5 },
    Groove { name: "dajčovo", groups: &[2, 2, 2, 3], tempo: (150, 190), dum: &[0, 6], tek: &[2, 4, 8], chord: &[1, 3, 5, 7, 8], busy: 0.5 },
    Groove { name: "kopanica", groups: &[2, 2, 3, 2, 2], tempo: (160, 200), dum: &[0, 4], tek: &[2, 7, 9], chord: &[1, 3, 5, 6, 8, 10], busy: 0.55 },
];

/// The Balkan dances taken at a run, as a fight is: the quick ones'
/// groupings at a hundred and fifty to a hundred and eighty beats, the
/// davul's low stroke on the bar's first beat and on its long group —
/// the limp the ear counts the dance by — its thin stick on every other
/// group's first beat and on the pickup into a group, and the tune
/// running more than it holds. A stroke on every eighth is a stream
/// that hides the grouping.
pub const FIGHT: [Groove; 4] = [
    Groove { name: "râčenica", groups: &[2, 2, 3], tempo: (300, 360), dum: &[0, 4], tek: &[2, 6], chord: &[1, 3, 5, 6], busy: 0.7 },
    Groove { name: "pajduško", groups: &[2, 3], tempo: (280, 340), dum: &[0, 2], tek: &[1, 4], chord: &[1, 3, 4], busy: 0.6 },
    Groove { name: "kopanica", groups: &[2, 2, 3, 2, 2], tempo: (300, 360), dum: &[0, 4], tek: &[2, 6, 7, 9], chord: &[1, 3, 5, 6, 8, 10], busy: 0.65 },
    Groove { name: "karşılama", groups: &[2, 2, 2, 3], tempo: (280, 340), dum: &[0, 6], tek: &[2, 4, 8], chord: &[1, 3, 5, 7, 8], busy: 0.6 },
];

/// The blues feels: the shuffle, four beats of three eighths with the
/// kick on one and three, the backbeat on two and four and the
/// accompaniment on the swung third of every beat; and the walking
/// four, straight eighths, the accompaniment pushing on the and of one
/// and of three, its tune leaning on the beat, since a group of two
/// eighths filled is a run of quavers and not a sung line. The shuffle
/// is an after-hours blues, its beat 68 to 78, over the ballad's
/// twelve-eight, so the two slow feels in three are never one pulse.
pub const BLUES: [Groove; 2] = [
    Groove { name: "shuffle", groups: &[3, 3, 3, 3], tempo: (204, 234), dum: &[0, 6], tek: &[3, 9], chord: &[2, 5, 8, 11], busy: 0.4 },
    Groove { name: "walking four", groups: &[2, 2, 2, 2], tempo: (170, 220), dum: &[0, 4], tek: &[2, 6], chord: &[1, 5], busy: 0.3 },
];

/// The ballad's feels, slow enough that every beat is a weight: the
/// four of a power ballad, its kick on one and the and of three and its
/// snare on two and four, the accompaniment on the off-beats; and the
/// twelve-eight, four slow beats of three, its kick on one and the last
/// eighth of two, its snare on two and four, the accompaniment on every
/// eighth between. Its tune leans on the beat and sings rather than
/// runs. The twelve-eight's beat is 50 to 60, under the blues'
/// shuffle.
pub const BALLAD: [Groove; 2] = [
    Groove { name: "four", groups: &[2, 2, 2, 2], tempo: (120, 150), dum: &[0, 5], tek: &[2, 6], chord: &[1, 3, 5, 7], busy: 0.3 },
    Groove { name: "twelve-eight", groups: &[3, 3, 3, 3], tempo: (150, 180), dum: &[0, 5], tek: &[3, 9], chord: &[1, 2, 4, 5, 7, 8, 10, 11], busy: 0.25 },
];

/// Every groove there is.
pub fn all() -> impl Iterator<Item = &'static Groove> {
    BALKAN.iter().chain(BLUES.iter()).chain(FIGHT.iter()).chain(BALLAD.iter())
}

impl Groove {
    pub fn meter(&self) -> Meter {
        Meter::new(self.groups)
    }

    /// Eighths in a bar.
    pub fn eighths(&self) -> u32 {
        self.groups.iter().map(|g| *g as u32).sum()
    }

    /// A bar's feet for a tune: each group as one foot, or a foot and a
    /// step, or a step and a foot, or steps, drawn on the dance's
    /// busyness, so a rhythm sits on the meter by construction.
    /// `(onset, length)` in eighths.
    pub fn feet(&self, rng: &mut Rng) -> Vec<(u32, u32)> {
        let mut out = Vec::new();
        let mut at = 0;
        for g in self.groups {
            let n = *g as u32;
            match rng.weighted(&[1.0 - self.busy, 0.8, self.busy, self.busy * 0.5]) {
                0 => out.push((at, n)),
                1 => {
                    out.push((at, n - 1));
                    out.push((at + n - 1, 1));
                }
                2 => {
                    out.push((at, 1));
                    out.push((at + 1, n - 1));
                }
                _ => {
                    for i in 0..n {
                        out.push((at + i, 1));
                    }
                }
            }
            at += n;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every stroke and strike sits inside the bar, the bass has every
    /// strong beat to itself, and the drum's two hands never land on
    /// one eighth.
    #[test]
    fn every_dance_is_whole() {
        for g in all() {
            let n = g.eighths();
            let strong = g.meter().strong_eighths();
            for e in g.dum.iter().chain(g.tek).chain(g.chord) {
                assert!(*e < n, "{}: {e} past the bar", g.name);
            }
            assert!(g.chord.iter().all(|e| !strong.contains(e)), "{}: a chord on a strong beat", g.name);
            assert!(g.dum.iter().all(|e| !g.tek.contains(e)), "{}: both hands on one eighth", g.name);
            assert!(g.tempo.0 < g.tempo.1);
        }
    }

    /// A bar's feet cover the bar exactly, group by group.
    #[test]
    fn feet_tile_the_bar() {
        for g in all() {
            for seed in 0..32 {
                let feet = g.feet(&mut Rng::new(seed));
                let mut at = 0;
                for (onset, len) in &feet {
                    assert_eq!(*onset, at, "{}: a gap", g.name);
                    at += len;
                }
                assert_eq!(at, g.eighths());
            }
        }
    }
}
