use bevy::prelude::*;
use crate::tuning::Tuning;

/// Vitality's commitment, a bank that fills as the actor takes blows: each
/// it lets land rather than reacting to fills it by the actor's tier
/// (`ActorAttributes::grit_fill`), up to `Tuning::grit_bank`. Only a full
/// bank is spent: the next skill that strikes lands `Tuning::grit_share`
/// harder and binds its target, and the bank empties. Every actor carries
/// one; only the server fills it, and it empties when the fight ends.
#[derive(Clone, Component, Copy, Debug, Default)]
pub struct Grit {
    pub filled: u8,
}

impl Grit {
    /// What the bank holds when full
    pub fn size(tuning: &Tuning) -> u8 {
        tuning.grit_bank.round().clamp(1.0, u8::MAX as f32) as u8
    }

    /// Fills the bank by `fill`, no further than full
    pub fn take(&mut self, tuning: &Tuning, fill: u8) {
        self.filled = self.filled.saturating_add(fill).min(Self::size(tuning));
    }

    pub fn is_full(&self, tuning: &Tuning) -> bool {
        self.filled >= Self::size(tuning)
    }

    /// Whether the bank was full, emptying it if so: a skill that strikes
    /// spends it only full
    pub fn release(&mut self, tuning: &Tuning) -> bool {
        let full = self.is_full(tuning);
        if full {
            self.filled = 0;
        }
        full
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bank_fills_by_the_tier_and_releases_only_full() {
        let tuning = Tuning::DEFAULT;
        let size = Grit::size(&tuning);
        let mut grit = Grit::default();
        grit.take(&tuning, 3);
        assert!(!grit.release(&tuning), "short of full it releases nothing");
        assert_eq!(grit.filled, 3, "and keeps what it holds");
        for _ in 0..size {
            grit.take(&tuning, 3);
        }
        assert_eq!(grit.filled, size, "it fills no further than full");
        assert!(grit.release(&tuning), "full, it releases");
        assert!(!grit.release(&tuning) && grit.filled == 0, "once, and empties");
    }
}
