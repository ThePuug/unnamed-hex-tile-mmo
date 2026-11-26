//! The economy as the game announces it: every item that appears, is
//! consumed or changes holder is one [`Moved`], written the frame it
//! happens. An item enters play from the world and leaves play to it, so
//! what the world has given less what it has taken back is everything in
//! play. Whatever keeps the world reads these; nothing here knows how.

use bevy::prelude::*;
use common::network::Identity;
use common_bevy::{archetype::EnemyArchetype, components::equipment::{Item, Lot}};

/// What holds an item: a player's character, a pile in one slot of a
/// tile, the camp round the hearth on a tile, whose settler builds with
/// it and whose wagon carries it, the depot of the hub known by a tile,
/// where wagons and players leave it in its storehouse, or the world,
/// which holds whatever is not in play.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Holder {
    World,
    Character(Identity),
    Pile(i32, i32, usize),
    Camp(i32, i32),
    Depot(i32, i32),
}

/// What moves: so many of a stackable kind, or a piece in a seed. Neither
/// has an identity of its own, and both are counted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Thing {
    Stack(common::Stackable),
    Piece(Item),
}

/// Why an item moved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cause {
    /// A tree, a stump or a boulder gave its yield.
    Gather,
    /// A player took from a pile.
    Take,
    /// A player dropped from its bag.
    Drop,
    /// A pile its gatherer let go of joined the tile's pile of its kind.
    LetGo,
    /// A construct was built of it.
    Build,
    /// A construct came down and left what it was built of.
    TakeDown,
    /// An NPC of this archetype died and left it.
    Loot(EnemyArchetype),
    /// A player's first character entered wearing it.
    Outfit,
    /// A player handed it to a settler to build with.
    Give,
    /// A player drew a map: the fibre it is drawn on, and the map.
    Draw,
    /// A wagon carried it to a hub, or left by the map it used up.
    Haul,
    /// A player left it in a storehouse.
    Store,
    /// A player took its own out of a storehouse.
    Withdraw,
}

impl Cause {
    /// The number a store keeps the cause by. It is given once: a cause
    /// renamed keeps it, a new one takes the next never used, and a
    /// removed one's is never given again.
    pub fn id(self) -> u32 {
        match self {
            Cause::Gather => 1,
            Cause::Take => 2,
            Cause::Drop => 3,
            Cause::LetGo => 4,
            Cause::Build => 5,
            Cause::TakeDown => 6,
            Cause::Loot(_) => 7,
            Cause::Outfit => 8,
            Cause::Give => 9,
            Cause::Draw => 10,
            Cause::Haul => 11,
            Cause::Store => 12,
            Cause::Withdraw => 13,
        }
    }
}

/// `count` of `what` went from one holder to another.
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct Moved {
    pub what: Thing,
    pub count: u32,
    pub from: Holder,
    pub to: Holder,
    pub cause: Cause,
}

impl Moved {
    pub fn stack(stack: common::Stack, from: Holder, to: Holder, cause: Cause) -> Self {
        Self { what: Thing::Stack(stack.kind), count: stack.count, from, to, cause }
    }

    /// A stack's count of its kind, or the one piece.
    pub fn lot(lot: Lot, from: Holder, to: Holder, cause: Cause) -> Self {
        match lot {
            Lot::Stack(stack) => Self::stack(stack, from, to, cause),
            Lot::Piece(item) => Self { what: Thing::Piece(item), count: 1, from, to, cause },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two causes under one number would each be kept as the other.
    #[test]
    fn every_cause_has_its_own_number() {
        let causes = [Cause::Gather, Cause::Take, Cause::Drop, Cause::LetGo, Cause::Build, Cause::TakeDown, Cause::Loot(EnemyArchetype::Berserker), Cause::Outfit, Cause::Give, Cause::Draw, Cause::Haul];
        let mut ids: Vec<u32> = causes.iter().map(|cause| cause.id()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), causes.len());
    }
}
