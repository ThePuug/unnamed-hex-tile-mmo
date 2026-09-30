use bevy::prelude::*;

/// What Vitality's commitment has banked: a share of each blow the actor
/// let land rather than reacting to (`ActorAttributes::grit_bank`), held
/// until its next skill strikes with it. Every actor carries one; only the
/// server fills it, and it empties when the fight ends.
#[derive(Clone, Component, Copy, Debug, Default)]
pub struct Grit {
    pub bank: f32,
}

impl Grit {
    /// Takes the whole bank, leaving it empty
    pub fn spend(&mut self) -> f32 {
        std::mem::take(&mut self.bank)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bank_is_spent_whole_and_once() {
        let mut grit = Grit::default();
        grit.bank += 30.0;
        grit.bank += 12.0;
        assert_eq!(grit.spend(), 42.0, "every blow banked strikes together");
        assert_eq!(grit.spend(), 0.0, "and only once");
    }
}
