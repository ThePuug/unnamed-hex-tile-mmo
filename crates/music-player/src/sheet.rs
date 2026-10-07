//! A score's most present voices, section by section, as the sheet draws
//! them.

use music::score::{Note, Role, Score, TICKS_PER_EIGHTH};

use crate::theme::VOICE_INKS;

/// How many voices the sheet draws at once, one to a lane.
pub const LANES: usize = VOICE_INKS.len();

/// A score as the sheet draws it: its pitched voices, and each lane's
/// stretches, a section's most present voice in the first lane, its next
/// in the second, and so on.
pub struct Sheet {
    pub voices: Vec<Voice>,
    pub lanes: [Vec<Stretch>; LANES],
    /// Where each bar line falls, seconds, as the score's tempo moves:
    /// bar `n` as a musician counts, from one, starts at `bars[n - 1]`.
    pub bars: Vec<f64>,
}

/// A voice: its name, the pitches its lane spans, and its notes as a
/// span in seconds and a pitch each.
pub struct Voice {
    pub name: &'static str,
    pub lo: u8,
    pub hi: u8,
    pub notes: Vec<(f64, f64, u8)>,
}

/// A lane's time in seconds through which one voice holds it, an index
/// into `Sheet::voices`: the sections running together that rank it the
/// same. A lane no voice holds through a section has no stretch there.
pub struct Stretch {
    pub from: f64,
    pub to: f64,
    pub voice: usize,
}

impl Sheet {
    /// Each section's lanes are its most present voices. A voice's
    /// presence is how strongly it sounds over the time it sounds: per
    /// sixteenth, the strongest of its notes sounding then — velocity
    /// squared, as the ear hears it, so a chord counts once — times its
    /// level against the others'. A note counts to four eighths at most:
    /// a drone held through the piece is a floor, not the most present
    /// voice. A part another plays the same notes of, at the same pitch
    /// or an octave off, is that part: it adds to its presence and is not
    /// drawn again. Drums have no pitch to draw.
    pub fn of(score: &Score) -> Sheet {
        let channels: Vec<u8> = score.instruments.iter().filter(|i| i.role != Role::Percussion).map(|i| i.channel).collect();
        let notes_of = |ch: u8| -> Vec<&Note> { score.notes.iter().filter(|n| n.channel == ch).collect() };
        // Each channel's voice: its own, or the voice of an earlier part
        // that plays all its notes.
        let mut voice_of: Vec<usize> = Vec::new();
        let mut voices: Vec<Voice> = Vec::new();
        let mut leaders: Vec<u8> = Vec::new();
        for &ch in &channels {
            let mine = notes_of(ch);
            let doubled = leaders.iter().position(|&lead| {
                let theirs = notes_of(lead);
                !mine.is_empty() && mine.iter().all(|n| theirs.iter().any(|t| t.start == n.start && t.pitch % 12 == n.pitch % 12))
            });
            match doubled {
                Some(v) => voice_of.push(v),
                None => {
                    voice_of.push(voices.len());
                    leaders.push(ch);
                    let notes: Vec<(f64, f64, u8)> = mine.iter().map(|n| (score.seconds(n.start), score.seconds(n.end()), n.pitch)).collect();
                    let lo = notes.iter().map(|n| n.2).min().unwrap_or(60).saturating_sub(1);
                    let hi = notes.iter().map(|n| n.2).max().unwrap_or(60).saturating_add(1).max(lo + 12);
                    voices.push(Voice { name: score.instrument(ch).name, lo, hi, notes });
                }
            }
        }

        let step = TICKS_PER_EIGHTH / 2;
        let cap = 4 * TICKS_PER_EIGHTH;
        let end = score.notes.iter().map(Note::end).max().unwrap_or(0).max(score.sections.last().map_or(0, |s| s.end));
        let steps = (end / step + 1) as usize;
        // Each voice's presence at every sixteenth.
        let mut presence = vec![vec![0.0f64; steps]; voices.len()];
        for (k, &ch) in channels.iter().enumerate() {
            let gain = 10f64.powf(score.instrument(ch).level as f64 / 10.0);
            let mut strongest = vec![0.0f64; steps];
            for n in notes_of(ch) {
                let strength = (n.vel as f64 / 127.0).powi(2);
                let (a, b) = (n.start / step, (n.start + n.len.min(cap)).div_ceil(step).max(n.start / step + 1));
                for s in &mut strongest[a as usize..(b as usize).min(steps)] {
                    *s = s.max(strength);
                }
            }
            for (sum, s) in presence[voice_of[k]].iter_mut().zip(strongest) {
                *sum += s * gain;
            }
        }

        let mut lanes: [Vec<Stretch>; LANES] = Default::default();
        let last = score.sections.len().saturating_sub(1);
        for (i, section) in score.sections.iter().enumerate() {
            // A piece rings on past its last section; the ring is the
            // last section's.
            let to = if i == last { end } else { section.end };
            let (a, b) = ((section.start / step) as usize, ((to / step) as usize).min(steps));
            let mut ranked: Vec<(usize, f64)> = presence.iter().enumerate().map(|(v, p)| (v, p[a..b].iter().sum())).filter(|(_, p)| *p > 0.0).collect();
            ranked.sort_by(|x, y| y.1.partial_cmp(&x.1).unwrap());
            let (from, to) = (score.seconds(section.start), score.seconds(to));
            for (lane, (voice, _)) in ranked.into_iter().take(LANES).enumerate() {
                match lanes[lane].last_mut() {
                    Some(held) if held.voice == voice && held.to >= from => held.to = to,
                    _ => lanes[lane].push(Stretch { from, to, voice }),
                }
            }
        }
        let bars = (0..=score.end() / score.bar()).map(|b| score.seconds(b * score.bar())).collect();
        Sheet { voices, lanes, bars }
    }

    /// The bar playing at `at` seconds, counted from one; none before the
    /// first.
    pub fn bar_at(&self, at: f64) -> Option<usize> {
        self.bars.iter().rposition(|b| *b <= at).map(|i| i + 1)
    }

    /// The voice in each lane at `at` seconds, none where the lane is
    /// empty then.
    pub fn at(&self, at: f64) -> [Option<usize>; LANES] {
        std::array::from_fn(|lane| self.lanes[lane].iter().find(|s| s.from <= at && at < s.to).map(|s| s.voice))
    }
}
