use bevy::prelude::*;

/// What Vitality's commitment has banked: a share (`Tuning::grit_share`)
/// of each blow the actor let land rather than reacting to, the first of
/// them up to as many as its Grit holds (`ActorAttributes::grit_holds`),
/// held until its next skill strikes with it. Full, it banks no more, so
/// it pays to spend it. Every actor carries one; only the server fills
/// it, and it empties when the fight ends.
#[derive(Clone, Component, Copy, Debug, Default)]
pub struct Grit {
    pub bank: f32,
    /// Blows in the bank
    pub held: u8,
}

impl Grit {
    /// Banks `share` of a blow while it holds fewer than `holds`
    pub fn take(&mut self, share: f32, holds: u8) {
        if self.held < holds {
            self.bank += share;
            self.held += 1;
        }
    }

    /// Takes the whole bank and the count of blows in it, leaving it empty
    pub fn spend(&mut self) -> (f32, u8) {
        (std::mem::take(&mut self.bank), std::mem::take(&mut self.held))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bank_is_spent_whole_and_once() {
        let mut grit = Grit::default();
        for share in [30.0, 12.0, 8.0] {
            grit.take(share, 2);
        }
        assert_eq!(grit.spend(), (42.0, 2), "the first it holds strike together; full, it banked no more");
        assert_eq!(grit.spend(), (0.0, 0), "and only once");
        grit.take(5.0, 2);
        assert_eq!(grit.spend(), (5.0, 1), "spent, it banks again");
    }
}
