//! What an actor wears and what a player owns. The design is
//! `design/equipment.md` in the internal repo.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// A garment the asset build makes, cut for every body that can wear it.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum Piece {
    LeatherHood,
    LeatherVest,
    LeatherGloves,
    LeatherGirdle,
    LeatherPants,
    LeatherBoots,
}

impl Piece {
    /// Every piece, in slot order.
    pub const ALL: [Piece; 6] = [
        Piece::LeatherHood,
        Piece::LeatherVest,
        Piece::LeatherGloves,
        Piece::LeatherGirdle,
        Piece::LeatherPants,
        Piece::LeatherBoots,
    ];

    /// The asset's name: the stem of `models/<name>-<actor>.glb`, and the
    /// key another piece's `hides.over` names, so the two must agree.
    pub fn name(self) -> &'static str {
        match self {
            Piece::LeatherHood => "leather-hood",
            Piece::LeatherVest => "leather-vest",
            Piece::LeatherGloves => "leather-gloves",
            Piece::LeatherGirdle => "leather-girdle",
            Piece::LeatherPants => "leather-pants",
            Piece::LeatherBoots => "leather-boots",
        }
    }

    /// The slot a piece is worn in. The game assigns it: pants and boots
    /// both fit the legs.
    pub fn slot(self) -> Slot {
        match self {
            Piece::LeatherHood => Slot::Head,
            Piece::LeatherVest => Slot::Torso,
            Piece::LeatherGloves => Slot::Hands,
            Piece::LeatherGirdle => Slot::Waist,
            Piece::LeatherPants => Slot::Legs,
            Piece::LeatherBoots => Slot::Feet,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Piece::LeatherHood => "Leather Hood",
            Piece::LeatherVest => "Leather Vest",
            Piece::LeatherGloves => "Leather Gloves",
            Piece::LeatherGirdle => "Leather Girdle",
            Piece::LeatherPants => "Leather Pants",
            Piece::LeatherBoots => "Leather Boots",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum Slot {
    Head,
    Torso,
    Hands,
    Waist,
    Legs,
    Feet,
}

impl Slot {
    pub const ALL: [Slot; 6] = [Slot::Head, Slot::Torso, Slot::Hands, Slot::Waist, Slot::Legs, Slot::Feet];

    pub fn name(self) -> &'static str {
        match self {
            Slot::Head => "Head",
            Slot::Torso => "Torso",
            Slot::Hands => "Hands",
            Slot::Waist => "Waist",
            Slot::Legs => "Legs",
            Slot::Feet => "Feet",
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// The styles every piece is made in: the set's seeds, one outfit each,
/// which are the piece's glTF scenes in order.
pub const STYLES: u8 = 3;

/// A piece in a style.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct Item {
    pub piece: Piece,
    pub style: u8,
}

/// What an actor wears, one item to a slot. Server authority, sent to every
/// client that sees the actor.
#[derive(Clone, Component, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Equipment {
    worn: [Option<Item>; 6],
}

impl Equipment {
    pub fn worn(&self, slot: Slot) -> Option<Item> {
        self.worn[slot.index()]
    }

    pub fn is_worn(&self, item: Item) -> bool {
        self.worn(item.piece.slot()) == Some(item)
    }

    /// Wears `item` and returns what its slot held.
    pub fn wear(&mut self, item: Item) -> Option<Item> {
        self.worn[item.piece.slot().index()].replace(item)
    }

    /// Takes `item` off; false when it was not worn.
    pub fn take_off(&mut self, item: Item) -> bool {
        let worn = self.is_worn(item);
        if worn {
            self.worn[item.piece.slot().index()] = None;
        }
        worn
    }

    pub fn items(&self) -> impl Iterator<Item = Item> + '_ {
        self.worn.iter().flatten().copied()
    }
}

/// Everything a player owns, worn or not, in the order it arrived. Server
/// authority, sent to its owner only.
#[derive(Clone, Component, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Inventory {
    pub items: Vec<Item>,
}

impl Inventory {
    /// Every piece in every style, slot by slot: the bag a player starts with.
    pub fn every_piece() -> Self {
        let items = Piece::ALL
            .iter()
            .flat_map(|&piece| (0..STYLES).map(move |style| Item { piece, style }))
            .collect();
        Self { items }
    }

    pub fn contains(&self, item: Item) -> bool {
        self.items.contains(&item)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(piece: Piece, style: u8) -> Item {
        Item { piece, style }
    }

    #[test]
    fn wearing_displaces_what_held_the_slot() {
        let mut worn = Equipment::default();
        assert_eq!(worn.wear(item(Piece::LeatherPants, 0)), None);
        assert_eq!(worn.wear(item(Piece::LeatherPants, 1)), Some(item(Piece::LeatherPants, 0)));
        assert!(worn.is_worn(item(Piece::LeatherPants, 1)));
        assert!(!worn.is_worn(item(Piece::LeatherPants, 0)));
        assert_eq!(worn.items().count(), 1);
    }

    #[test]
    fn taking_off_only_what_is_worn() {
        let mut worn = Equipment::default();
        assert!(!worn.take_off(item(Piece::LeatherVest, 2)));
        worn.wear(item(Piece::LeatherVest, 2));
        assert!(!worn.take_off(item(Piece::LeatherVest, 0)));
        assert!(worn.take_off(item(Piece::LeatherVest, 2)));
        assert_eq!(worn.worn(Slot::Torso), None);
    }

    #[test]
    fn pants_and_boots_take_different_slots() {
        let mut worn = Equipment::default();
        worn.wear(item(Piece::LeatherPants, 0));
        worn.wear(item(Piece::LeatherBoots, 0));
        assert_eq!(worn.items().count(), 2);
        assert_ne!(Piece::LeatherPants.slot(), Piece::LeatherBoots.slot());
    }

    #[test]
    fn every_piece_fills_the_bag_slot_by_slot() {
        let bag = Inventory::every_piece();
        assert_eq!(bag.items.len(), Piece::ALL.len() * STYLES as usize);
        let slots: Vec<Slot> = bag.items.iter().map(|i| i.piece.slot()).collect();
        let mut sorted = slots.clone();
        sorted.sort_by_key(|s| s.index());
        assert_eq!(slots, sorted);
        for piece in Piece::ALL {
            for style in 0..STYLES {
                assert!(bag.contains(item(piece, style)));
            }
        }
    }

    #[test]
    fn piece_names_are_distinct_asset_stems() {
        let names: Vec<&str> = Piece::ALL.iter().map(|p| p.name()).collect();
        for name in &names {
            assert!(name.chars().all(|c| c.is_ascii_lowercase() || c == '-'), "{name}");
        }
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), names.len());
    }
}
