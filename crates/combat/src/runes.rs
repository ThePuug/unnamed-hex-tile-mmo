//! Runes: a while of something on a place. A rune is cut on the tile at its
//! own distance along its caster's heading and lays down ahead of time what
//! its line's skills spend, for them to cash there. One rune stands to a
//! tile, and an actor keeps one of each kind; it has 1 health, so a blow
//! that lands on it strikes it down.
//!
//! Unbuilt: nothing casts a rune. Each is named here with its line, its kind
//! and what it does.

use common_bevy::components::Attribute;

/// What a rune works on
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuneKind {
    /// Moves or turns a foe entering its reach, by threats of its own,
    /// measured from the rune wherever the foe stands as they land
    Control,
    /// Changes the blows struck from its reach
    Damage,
    /// Changes what lands on allies in its reach
    Support,
}

/// The rune each attribute line holds
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Rune {
    /// Might's: blows struck from its reach land harder, so a Frenzy bursts
    /// into it and a Bloodlust returns more. Its line raises how much harder
    Empower,
    /// Agility's: turns a foe entering its reach to face the rune, its back
    /// to a striker beyond, for a Backstab and Grace's flank. Its line
    /// raises its reach
    Distract,
    /// Physique's: drags a foe entering its reach straight toward the rune,
    /// into an Overpower's reach and Intimidation's ground. Its line raises
    /// how far
    Drag,
    /// Discipline's: pushes a foe entering its reach straight away from the
    /// rune, the distance a Leap dives across and a Surge counts. Its line
    /// raises how far
    Push,
    /// Instinct's: every crit struck from its reach by its caster's side,
    /// auto-attacks included, queues a wound beside its blow, each its own
    /// and ticking each second; a Punish lands harder for each wound
    /// waiting in its target's queue. Its line raises each wound's damage
    SharpenedBlades,
    /// Resolve's: the next skill a foe lands on an ally in its reach lands
    /// lighter, and spends it, so a Defy may let threats wait. Its line
    /// raises how much lighter
    Ward,
}

impl Rune {
    pub const ALL: [Rune; 6] = [Rune::Empower, Rune::Distract, Rune::Drag, Rune::Push, Rune::SharpenedBlades, Rune::Ward];

    /// The attribute line it belongs to
    pub fn line(self) -> Attribute {
        match self {
            Rune::Empower => Attribute::Might,
            Rune::Distract => Attribute::Agility,
            Rune::Drag => Attribute::Physique,
            Rune::Push => Attribute::Discipline,
            Rune::SharpenedBlades => Attribute::Instinct,
            Rune::Ward => Attribute::Resolve,
        }
    }

    pub fn kind(self) -> RuneKind {
        match self {
            Rune::Distract | Rune::Drag | Rune::Push => RuneKind::Control,
            Rune::Empower | Rune::SharpenedBlades => RuneKind::Damage,
            Rune::Ward => RuneKind::Support,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_line_holds_one_rune() {
        for attribute in [Attribute::Might, Attribute::Agility, Attribute::Physique, Attribute::Discipline, Attribute::Instinct, Attribute::Resolve] {
            assert_eq!(Rune::ALL.iter().filter(|rune| rune.line() == attribute).count(), 1, "{attribute:?}");
        }
    }
}
