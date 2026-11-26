//! A character as the store keeps it: JSON holding each item, stack and
//! skill by its number ([`stored`]).

use common_bevy::components::{
    equipment::{Equipment, Inventory},
    heading::Heading,
    loadout::Loadout,
    ActorAttributes,
};
use qrz::Qrz;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::stored;
use crate::systems::characters::Kept;

#[derive(Serialize, Deserialize)]
struct State {
    owned: Vec<Value>,
    worn: Vec<Value>,
    stock: Vec<Value>,
    attrs: ActorAttributes,
    loadout: Value,
    at: Qrz,
    heading: Heading,
    /// Where it stands at the home it lives in; none where it lives in
    /// none. A character kept before it could live apart from where it
    /// returns has its hearth here, and no `returns`.
    #[serde(default)]
    home: Option<Qrz>,
    /// Where it stands at the hearth it returns to; none where that is
    /// the haven.
    #[serde(default)]
    returns: Option<Qrz>,
}

/// What a character holds that this build cannot play, as the store kept
/// it. The character keeps it, out of the world, and is written with it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Dormant {
    owned: Vec<Value>,
    stock: Vec<Value>,
}

/// A character standing at `at` facing `heading`, returning to `home`,
/// living at `lives` and holding `dormant` besides, as the store keeps it.
#[allow(clippy::too_many_arguments)]
pub fn state(equipment: &Equipment, inventory: &Inventory, attrs: &ActorAttributes, loadout: &Loadout, at: Qrz, heading: Heading, home: Option<Qrz>, lives: Option<Qrz>, dormant: &Dormant) -> String {
    let state = State {
        owned: inventory.items.iter().copied().map(stored::item).chain(dormant.owned.iter().cloned()).collect(),
        worn: equipment.items().map(stored::item).collect(),
        stock: inventory.stock.iter().copied().map(stored::stack).chain(dormant.stock.iter().cloned()).collect(),
        attrs: *attrs,
        loadout: stored::loadout(loadout),
        at,
        heading,
        home: lives,
        returns: home,
    };
    serde_json::to_string(&state).expect("a character serializes")
}

/// `kept`, holding `dormant` besides, as the store keeps it.
pub fn kept(kept: &Kept, dormant: &Dormant) -> String {
    state(&kept.equipment, &kept.inventory, &kept.attrs, &kept.loadout, kept.at, kept.heading, kept.home, kept.lives, dormant)
}

/// The character `state` keeps, and what of it this build cannot play. It
/// wears only what it owns.
pub fn recalled(state: Value) -> Result<(Kept, Dormant), serde_json::Error> {
    let state: State = serde_json::from_value(state)?;
    let (items, owned) = stored::items(state.owned);
    let (stock, unstocked) = stored::stacks(state.stock);
    let mut equipment = Equipment::default();
    for item in stored::items(state.worn).0.into_iter().filter(|item| items.contains(item)) {
        equipment.wear(item);
    }
    let kept = Kept {
        equipment,
        inventory: Inventory { items, stock },
        attrs: state.attrs,
        loadout: stored::bar(&state.loadout),
        at: state.at,
        heading: state.heading,
        home: state.returns.or(state.home),
        lives: state.home,
    };
    Ok((kept, Dormant { owned, stock: unstocked }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::Stack;

    #[test]
    fn a_character_is_recalled_as_it_was_kept() {
        let mut was = Kept::fresh(Qrz { q: 9, r: -4, z: 2 });
        was.inventory.add(Stack { kind: common::Stackable::Material(common::Material::Softwood), count: 5 });
        let vest = was.equipment.worn(common_bevy::components::equipment::Slot::Torso).unwrap();
        was.equipment.take_off(vest);
        let text = kept(&was, &Dormant::default());
        let (back, dormant) = recalled(serde_json::from_str(&text).unwrap()).unwrap();
        assert_eq!(dormant, Dormant::default());
        assert_eq!(kept(&back, &dormant), text);
        assert_eq!((back.equipment, &back.inventory, back.at), (was.equipment, &was.inventory, was.at));
    }

    /// What this build has no number for stays out of the world and in
    /// the store, and the rest of the character comes back.
    #[test]
    fn what_this_build_cannot_play_is_kept_and_written_back() {
        let was = Kept::fresh(Qrz::default());
        let mut state: Value = serde_json::from_str(&kept(&was, &Dormant::default())).unwrap();
        let gone = serde_json::json!({"piece": 4_000_000, "seed": 0});
        let coin = serde_json::json!({"coin": 1, "count": 30});
        state["owned"].as_array_mut().unwrap().push(gone.clone());
        state["worn"].as_array_mut().unwrap().push(gone.clone());
        state["stock"].as_array_mut().unwrap().push(coin.clone());
        let (back, dormant) = recalled(state).unwrap();
        assert_eq!(back.inventory.items, was.inventory.items);
        assert_eq!(back.equipment, was.equipment);
        let again: Value = serde_json::from_str(&kept(&back, &dormant)).unwrap();
        assert_eq!(again["owned"].as_array().unwrap().last(), Some(&gone));
        assert_eq!(again["stock"].as_array().unwrap().last(), Some(&coin));
    }
}
