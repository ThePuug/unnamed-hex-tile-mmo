//! A character as the store keeps it: JSON that names each thing as the
//! game names it to a player, never by its place in a table the next build
//! may reorder.

use bevy::prelude::*;
use common::Stack;
use common_bevy::components::{
    equipment::{Equipment, Inventory, Item, Piece},
    heading::Heading,
    loadout::Loadout,
    ActorAttributes,
};
use qrz::Qrz;
use serde::{Deserialize, Serialize};

use crate::systems::characters::Kept;

#[derive(Serialize, Deserialize)]
struct State {
    owned: Vec<Named>,
    worn: Vec<Named>,
    stock: Vec<Stack>,
    attrs: ActorAttributes,
    /// Read apart from the rest: a bar naming a skill the game no longer
    /// has is the default bar, and the character is otherwise as it was.
    loadout: serde_json::Value,
    at: Qrz,
    heading: Heading,
}

/// An item by its piece's name.
#[derive(Serialize, Deserialize)]
struct Named {
    piece: String,
    seed: u16,
}

impl Named {
    fn of(item: Item) -> Self {
        Self { piece: item.piece.name().to_owned(), seed: item.seed }
    }

    /// The item, or none where the assets no longer declare its piece.
    fn item(&self) -> Option<Item> {
        let piece = Piece::all().find(|piece| piece.name() == self.piece);
        if piece.is_none() {
            error!("store: a character owns a `{}`, which the assets no longer declare; it is dropped", self.piece);
        }
        Some(Item { piece: piece?, seed: self.seed })
    }
}

/// A character standing at `at` facing `heading`, as the store keeps it.
pub fn state(equipment: &Equipment, inventory: &Inventory, attrs: &ActorAttributes, loadout: &Loadout, at: Qrz, heading: Heading) -> String {
    let state = State {
        owned: inventory.items.iter().copied().map(Named::of).collect(),
        worn: equipment.items().map(Named::of).collect(),
        stock: inventory.stock.clone(),
        attrs: *attrs,
        loadout: serde_json::to_value(loadout).expect("a loadout serializes"),
        at,
        heading,
    };
    serde_json::to_string(&state).expect("a character serializes")
}

/// `kept` as the store keeps it.
pub fn kept(kept: &Kept) -> String {
    state(&kept.equipment, &kept.inventory, &kept.attrs, &kept.loadout, kept.at, kept.heading)
}

/// The character `state` keeps. It wears only what it owns.
pub fn recalled(state: serde_json::Value) -> Result<Kept, serde_json::Error> {
    let state: State = serde_json::from_value(state)?;
    let items: Vec<Item> = state.owned.iter().filter_map(Named::item).collect();
    let mut equipment = Equipment::default();
    for item in state.worn.iter().filter_map(Named::item).filter(|item| items.contains(item)) {
        equipment.wear(item);
    }
    Ok(Kept {
        equipment,
        inventory: Inventory { items, stock: state.stock },
        attrs: state.attrs,
        loadout: serde_json::from_value(state.loadout).unwrap_or_default(),
        at: state.at,
        heading: state.heading,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_character_is_recalled_as_it_was_kept() {
        let mut was = Kept::fresh(Qrz { q: 9, r: -4, z: 2 });
        was.inventory.add(Stack { kind: common::Stackable::Material(common::Material::Softwood), count: 5 });
        let vest = was.equipment.worn(common_bevy::components::equipment::Slot::Torso).unwrap();
        was.equipment.take_off(vest);
        let text = kept(&was);
        let back = recalled(serde_json::from_str(&text).unwrap()).unwrap();
        assert_eq!(kept(&back), text);
        assert_eq!((back.equipment, &back.inventory, back.at), (was.equipment, &was.inventory, was.at));
    }

    /// What the next build no longer has is dropped, and the rest of the
    /// character comes back.
    #[test]
    fn a_piece_or_a_skill_since_removed_costs_only_itself() {
        let was = Kept::fresh(Qrz::default());
        let mut state: serde_json::Value = serde_json::from_str(&kept(&was)).unwrap();
        state["owned"].as_array_mut().unwrap().push(serde_json::json!({"piece": "no-such-piece", "seed": 0}));
        state["worn"].as_array_mut().unwrap().push(serde_json::json!({"piece": "no-such-piece", "seed": 0}));
        state["loadout"] = serde_json::json!(["NoSuchSkill"]);
        let back = recalled(state).unwrap();
        assert_eq!(back.inventory.items, was.inventory.items);
        assert_eq!(back.equipment, was.equipment);
        assert_eq!(back.loadout, Loadout::default());
    }
}
