use bevy::prelude::*;
use crate::tuning::Tuning;

/// Physique's commitment, a bank that fills while the actor is engaged:
/// each second by its tier (`ActorAttributes::intimidation_fill`), twice
/// that while any hostile within its reach is not targeting it, up to
/// `Tuning::intimidation_bank`. Only a full bank is spent: the next skill
/// that strikes lands `Tuning::intimidation_share` harder and binds its
/// target, and the bank empties. Every actor carries one; only the server
/// fills it, and it empties when the fight ends.
#[derive(Clone, Component, Copy, Debug, Default)]
pub struct Intimidation {
    pub filled: f32,
}

impl Intimidation {
    /// What the bank holds when full
    pub fn size(tuning: &Tuning) -> f32 {
        tuning.intimidation_bank.max(f32::EPSILON)
    }

    /// Fills the bank by `fill`, no further than full
    pub fn take(&mut self, tuning: &Tuning, fill: f32) {
        self.filled = (self.filled + fill).min(Self::size(tuning));
    }

    pub fn is_full(&self, tuning: &Tuning) -> bool {
        self.filled >= Self::size(tuning)
    }

    /// Whether the bank was full, emptying it if so: a skill that strikes
    /// spends it only full
    pub fn release(&mut self, tuning: &Tuning) -> bool {
        let full = self.is_full(tuning);
        if full {
            self.filled = 0.0;
        }
        full
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bank_fills_and_releases_only_full() {
        let tuning = Tuning::DEFAULT;
        let size = Intimidation::size(&tuning);
        let mut intimidation = Intimidation::default();
        intimidation.take(&tuning, 3.0);
        assert!(!intimidation.release(&tuning), "short of full it releases nothing");
        assert_eq!(intimidation.filled, 3.0, "and keeps what it holds");
        intimidation.take(&tuning, size);
        assert_eq!(intimidation.filled, size, "it fills no further than full");
        assert!(intimidation.release(&tuning), "full, it releases");
        assert!(!intimidation.release(&tuning) && intimidation.filled == 0.0, "once, and empties");
    }
}
