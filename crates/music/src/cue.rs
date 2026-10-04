//! A cue: a one-shot cut to picture. It is told in movements that
//! start where the picture cuts, each a run of whole half-phrases so
//! every cut lands on a cadence; the tempo is what brings the cadences
//! nearest the cuts, taken from the dance's band, since a cut the
//! music misses by a bar is a cut the picture has to move. A movement
//! that turns the harmony starts a phrase.

/// Where the picture cuts to a movement.
#[derive(Clone, Copy, Debug)]
pub struct Cut {
    /// Seconds from the head.
    pub at: f32,
    /// Whether the movement opens a phrase: its harmony turns there.
    pub phrase: bool,
    /// The fewest half-phrases the movement lasts.
    pub least: u32,
}

/// The tempo, eighths a minute within `band`, and the half-phrase each
/// cut falls on, the first at the head and the last where the tune
/// ends: the one whose cadences sit nearest the cuts, least squares
/// over every cut, the whole a number of phrases. `eighths` is a bar's.
pub fn fit(cuts: &[Cut], band: (i32, i32), eighths: u32, bars_a_half: u32) -> (f32, Vec<u32>) {
    let mut best: Option<(f32, f32, Vec<u32>)> = None;
    for tempo in band.0..=band.1 {
        let half = (bars_a_half * eighths) as f32 * 60.0 / tempo as f32;
        if let Some((err, at)) = place(cuts, half) {
            if best.as_ref().is_none_or(|(e, _, _)| err < *e) {
                best = Some((err, tempo as f32, at));
            }
        }
    }
    let (_, tempo, at) = best.expect("a tempo that places every cut");
    (tempo, at)
}

/// The half-phrase of every cut at `half` seconds a half, by dynamic
/// programme over each cut's nearest candidates, and its squared miss;
/// None where no placement keeps every movement its least.
fn place(cuts: &[Cut], half: f32) -> Option<(f32, Vec<u32>)> {
    let candidates = |c: &Cut| -> Vec<u32> {
        let near = (c.at / half).round() as i64;
        (near - 3..=near + 3).filter(|k| *k >= 0 && (!c.phrase || k % 2 == 0)).map(|k| k as u32).collect()
    };
    // For each cut, each candidate's best total miss and the path to it.
    let mut rows: Vec<Vec<(u32, f32, Vec<u32>)>> = Vec::new();
    for (i, c) in cuts.iter().enumerate() {
        let miss = |k: u32| (k as f32 * half - c.at).powi(2);
        let row: Vec<(u32, f32, Vec<u32>)> = if i == 0 {
            vec![(0, c.at.powi(2), vec![0])]
        } else {
            candidates(c)
                .into_iter()
                .filter_map(|k| {
                    rows[i - 1]
                        .iter()
                        .filter(|(p, _, _)| k >= p + cuts[i - 1].least)
                        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
                        .map(|(_, e, path)| {
                            let mut path = path.clone();
                            path.push(k);
                            (k, e + miss(k), path)
                        })
                })
                .collect()
        };
        if row.is_empty() {
            return None;
        }
        rows.push(row);
    }
    rows.pop()?.into_iter().filter(|(k, _, _)| k % 2 == 0).min_by(|a, b| a.1.partial_cmp(&b.1).unwrap()).map(|(_, e, path)| (e, path))
}

/// `n` half-phrases over `parts` parts: each at least one, the rest
/// going to the last, so a movement builds to its longest stay.
pub fn spread(n: u32, parts: usize) -> Vec<u32> {
    let parts = parts as u32;
    let (each, over) = (n / parts, n % parts);
    (0..parts).map(|p| each + (p >= parts - over) as u32).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cuts at a steady five seconds a half-phrase fall exactly on the
    /// tempo that makes one, and every cut that opens a phrase falls on
    /// one.
    #[test]
    fn cuts_fall_on_the_tempo_that_meets_them() {
        let cut = |at: f32, phrase: bool| Cut { at, phrase, least: 1 };
        let cuts = [cut(0.0, true), cut(15.0, false), cut(40.0, true), cut(60.0, true), cut(90.0, true), cut(110.0, true)];
        let (tempo, at) = fit(&cuts, (130, 170), 7, 2);
        assert_eq!(tempo, 168.0);
        assert_eq!(at, vec![0, 3, 8, 12, 18, 22]);
    }

    /// A cut that cannot fall on its nearest cadence takes the next
    /// that keeps every movement its least and every phrase whole.
    #[test]
    fn every_movement_keeps_its_least() {
        let cuts = [Cut { at: 0.0, phrase: true, least: 4 }, Cut { at: 5.0, phrase: true, least: 1 }, Cut { at: 30.0, phrase: true, least: 1 }];
        let (_, at) = fit(&cuts, (140, 160), 7, 2);
        assert!(at[1] >= 4 && at[1] % 2 == 0 && at[2] % 2 == 0, "{at:?}");
    }

    #[test]
    fn a_spread_builds_to_its_last_part() {
        assert_eq!(spread(5, 4), vec![1, 1, 1, 2]);
        assert_eq!(spread(6, 4), vec![1, 1, 2, 2]);
        assert_eq!(spread(3, 3), vec![1, 1, 1]);
        assert_eq!(spread(4, 2), vec![2, 2]);
    }
}
