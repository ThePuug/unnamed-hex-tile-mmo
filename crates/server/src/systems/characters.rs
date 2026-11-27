//! The characters the server knows: each player's, in the world or as it
//! left it. A character that leaves is kept here and enters again as it
//! left. How they are kept past the server is not this module's to know:
//! it asks after a player it does not know, and whatever keeps them
//! recalls it.

use std::collections::HashMap;

use bevy::prelude::*;
use common::network::Identity;
use common_bevy::components::{
    equipment::{Equipment, Inventory},
    heading::Heading,
    loadout::Loadout,
    ActorAttributes,
};
use qrz::Qrz;

/// Whose character an actor in the world is.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Character(pub Identity);

/// A character out of the world, as it left: what it owns and wears, its
/// build and its bar, where it stood, where it stands at the hearth it
/// returns to, if it has one, and at the home it lives in, if it has one.
/// Its health is not kept, and it enters whole.
#[derive(Clone, Debug)]
pub struct Kept {
    pub equipment: Equipment,
    pub inventory: Inventory,
    pub attrs: ActorAttributes,
    pub loadout: Loadout,
    pub at: Qrz,
    pub heading: Heading,
    pub home: Option<Qrz>,
    pub lives: Option<Qrz>,
}

impl Kept {
    /// A player's first character, standing at `at`: nothing worn, an
    /// empty bag and the starting build.
    pub fn fresh(at: Qrz) -> Self {
        Self {
            equipment: Equipment::default(),
            inventory: Inventory::default(),
            attrs: ActorAttributes::new(-3, 4, 0, 1, 0, 0, -3, 4, 0),
            loadout: Loadout::default(),
            at,
            heading: Heading::default(),
            home: None,
            lives: None,
        }
    }
}

/// Every player's character out of the world.
#[derive(Resource, Default)]
pub struct Characters {
    /// Each player known: its character as it left, or none where it has
    /// yet to make one.
    known: HashMap<Identity, Option<Kept>>,
    wanted: Vec<Identity>,
    left: Vec<Identity>,
}

impl Characters {
    /// Whether `identity`'s character may enter: whether it has been
    /// [`Self::recall`]ed. Where it has not, it is asked after, once.
    pub fn knows(&mut self, identity: Identity) -> bool {
        if self.known.contains_key(&identity) {
            return true;
        }
        if !self.wanted.contains(&identity) {
            self.wanted.push(identity);
        }
        false
    }

    /// `identity`'s character as it left the world, or none where it has
    /// made none.
    pub fn as_left(&self, identity: Identity) -> Option<&Kept> {
        self.known.get(&identity)?.as_ref()
    }

    /// `identity`'s character left the world as `kept`.
    pub fn leave(&mut self, identity: Identity, kept: Kept) {
        self.known.insert(identity, Some(kept));
        self.left.push(identity);
    }

    /// What was kept of `identity` past the server: its character, or none
    /// where it has made none.
    pub fn recall(&mut self, identity: Identity, kept: Option<Kept>) {
        self.wanted.retain(|&wanted| wanted != identity);
        self.known.entry(identity).or_insert(kept);
    }

    /// `identity` is no longer this server's to know: it is asked after
    /// again before its character next enters.
    pub fn forget(&mut self, identity: Identity) {
        self.known.remove(&identity);
    }

    /// The players asked after since the last call.
    pub fn take_wanted(&mut self) -> Vec<Identity> {
        std::mem::take(&mut self.wanted)
    }

    /// The players whose characters left the world since the last call.
    pub fn take_left(&mut self) -> Vec<Identity> {
        std::mem::take(&mut self.left)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHO: Identity = Identity::Key([7; 16]);

    #[test]
    fn a_character_enters_as_it_left() {
        let mut characters = Characters::default();
        characters.recall(WHO, None);
        assert!(characters.knows(WHO) && characters.as_left(WHO).is_none());
        let mut kept = Kept::fresh(Qrz { q: 3, r: -2, z: 5 });
        kept.inventory.add(common::Stack { kind: common::Stackable::Material(common::Material::Softwood), count: 4 });
        characters.leave(WHO, kept.clone());
        let back = characters.as_left(WHO).expect("it left");
        assert_eq!((back.at, &back.inventory), (kept.at, &kept.inventory));
        assert_eq!(characters.take_left(), vec![WHO]);
    }

    #[test]
    fn a_character_waits_to_be_recalled() {
        let mut characters = Characters::default();
        assert!(!characters.knows(WHO));
        assert!(!characters.knows(WHO));
        assert_eq!(characters.take_wanted(), vec![WHO], "asked after once");
        characters.recall(WHO, None);
        assert!(characters.knows(WHO));
        assert!(characters.as_left(WHO).is_none(), "a new player makes its first");
        characters.forget(WHO);
        assert!(!characters.knows(WHO));
    }
}
