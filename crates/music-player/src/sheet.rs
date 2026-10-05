//! A score's most present voices, as the sheet draws them.

use music::score::{Role, Score, TICKS_PER_EIGHTH};

use crate::theme::VOICE_INKS;

/// A score's three most present voices as a piano roll: each note's
/// span in seconds, its pitch and whose it is.
pub struct Sheet {
    pub voices: Vec<&'static str>,
    pub notes: Vec<(f64, f64, u8, usize)>,
    pub lo: u8,
    pub hi: u8,
    pub bar_s: f64,
}

impl Sheet {
    /// A voice's presence is its notes' strength — velocity squared, as
    /// the ear hears it — over their length, each counted to four
    /// eighths at most: a drone held through the piece is a floor, not
    /// the most present voice, and a part that plays is. Drums have no
    /// pitch to draw.
    pub fn of(score: &Score) -> Sheet {
        let cap = 4 * TICKS_PER_EIGHTH;
        let mut presence: Vec<(u8, f64)> = score
            .instruments
            .iter()
            .filter(|i| i.role != Role::Percussion)
            .map(|i| {
                let weight = score.notes.iter().filter(|n| n.channel == i.channel).map(|n| (n.vel as f64 / 127.0).powi(2) * n.len.min(cap) as f64).sum::<f64>();
                (i.channel, weight)
            })
            .filter(|(_, w)| *w > 0.0)
            .collect();
        presence.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        presence.truncate(VOICE_INKS.len());
        let channels: Vec<u8> = presence.iter().map(|(c, _)| *c).collect();
        let notes: Vec<(f64, f64, u8, usize)> = score
            .notes
            .iter()
            .filter_map(|n| channels.iter().position(|c| *c == n.channel).map(|v| (score.seconds(n.start), score.seconds(n.end()), n.pitch, v)))
            .collect();
        let lo = notes.iter().map(|n| n.2).min().unwrap_or(48).saturating_sub(2);
        let hi = notes.iter().map(|n| n.2).max().unwrap_or(72).saturating_add(2).max(lo + 12);
        Sheet { voices: channels.iter().map(|c| score.instrument(*c).name).collect(), notes, lo, hi, bar_s: score.seconds(score.bar()) }
    }
}

