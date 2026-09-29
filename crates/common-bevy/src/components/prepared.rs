use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::message::AbilityType;

/// The reactions an actor has prepared, their stamina and lockout already
/// paid: Discipline's commitment holds up to its tier of them
/// (`ActorAttributes::preparation`) until the actor leaves combat, and each
/// fires free, even mid-lockout. The server sends the whole of it whenever
/// it changes.
#[derive(Clone, Component, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Prepared {
    held: [Option<AbilityType>; 3],
}

impl Prepared {
    /// Whether `ability` answers a threat, so can be prepared
    pub fn is_reaction(ability: AbilityType) -> bool {
        matches!(ability, AbilityType::Counter | AbilityType::Deflect | AbilityType::Disengage)
    }

    /// Whether one of `ability` is held
    pub fn holds(&self, ability: AbilityType) -> bool {
        self.held.contains(&Some(ability))
    }

    /// How many are held, of any reaction
    pub fn count(&self) -> usize {
        self.held.iter().flatten().count()
    }

    /// Holds one more of `ability`, where fewer than `room` are held
    pub fn hold(&mut self, ability: AbilityType, room: usize) -> bool {
        if self.count() >= room {
            return false;
        }
        let Some(slot) = self.held.iter_mut().find(|slot| slot.is_none()) else { return false };
        *slot = Some(ability);
        true
    }

    /// Spends one held `ability`, if there is one
    pub fn take(&mut self, ability: AbilityType) -> bool {
        let Some(slot) = self.held.iter_mut().find(|slot| **slot == Some(ability)) else { return false };
        *slot = None;
        true
    }

    /// Lets every held reaction go
    pub fn clear(&mut self) {
        self.held = [None; 3];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_holds_up_to_its_room_and_spends_what_it_holds() {
        let mut prepared = Prepared::default();
        assert!(!prepared.hold(AbilityType::Counter, 0), "no room without Preparation");
        assert!(prepared.hold(AbilityType::Counter, 2));
        assert!(prepared.hold(AbilityType::Deflect, 2));
        assert!(!prepared.hold(AbilityType::Counter, 2), "full");
        assert!(prepared.holds(AbilityType::Deflect));
        assert!(prepared.take(AbilityType::Counter));
        assert!(!prepared.take(AbilityType::Counter), "spent");
        assert_eq!(prepared.count(), 1);
    }
}
