//! Items, stacks and skills as the store keeps them: each by the number
//! given it once, never by a name or a place in a table, which the next
//! build may change. What a build has no number for is not read as
//! anything: it comes back apart, as it was kept, for its holder to keep
//! on, so nothing kept is lost to a build that cannot play it.

use common::{Material, Stack, Stackable};
use common_bevy::{
    components::{
        equipment::{Item, Piece},
        loadout::Loadout,
    },
    message::AbilityType,
};
use serde_json::{json, Value};

pub fn item(item: Item) -> Value {
    json!({"piece": item.piece.id(), "seed": item.seed})
}

/// A stackable kind, with no count.
pub fn kind(kind: Stackable) -> Value {
    match kind {
        Stackable::Material(material) => json!({"material": material.id()}),
    }
}

pub fn stack(stack: Stack) -> Value {
    let mut kept = kind(stack.kind);
    kept["count"] = json!(stack.count);
    kept
}

/// A bar, a skill's number to a slot.
pub fn loadout(loadout: &Loadout) -> Value {
    loadout.0.iter().map(|ability| ability.id()).collect()
}

fn number(value: &Value, field: &str) -> Option<u32> {
    value.get(field)?.as_u64()?.try_into().ok()
}

fn read_item(kept: &Value) -> Option<Item> {
    Some(Item { piece: Piece::kept_as(number(kept, "piece")?)?, seed: number(kept, "seed")?.try_into().ok()? })
}

fn read_stack(kept: &Value) -> Option<Stack> {
    Some(Stack { kind: Stackable::Material(Material::kept_as(number(kept, "material")?)?), count: number(kept, "count")? })
}

/// `kept` as what this build reads of it, in order, and the rest as kept.
fn sorted<T>(kept: Vec<Value>, read: impl Fn(&Value) -> Option<T>) -> (Vec<T>, Vec<Value>) {
    let mut known = Vec::new();
    let mut unknown = Vec::new();
    for value in kept {
        match read(&value) {
            Some(read) => known.push(read),
            None => unknown.push(value),
        }
    }
    (known, unknown)
}

/// The items `kept` holds that this build has, and the rest as kept.
pub fn items(kept: Vec<Value>) -> (Vec<Item>, Vec<Value>) {
    sorted(kept, read_item)
}

/// The stacks `kept` holds that this build has, and the rest as kept.
pub fn stacks(kept: Vec<Value>) -> (Vec<Stack>, Vec<Value>) {
    sorted(kept, read_stack)
}

/// The bar `kept` holds. A slot whose skill the game no longer has, or
/// holds twice, takes the first skill the bar lacks, a new character's
/// first; anything that is no bar is a new character's.
pub fn bar(kept: &Value) -> Loadout {
    let Some(kept) = kept.as_array().filter(|kept| kept.len() == Loadout::DEFAULT.0.len()) else { return Loadout::DEFAULT };
    let mut held: Vec<Option<AbilityType>> = Vec::new();
    for slot in kept {
        let ability = slot.as_u64().and_then(|id| AbilityType::kept_as(id.try_into().ok()?));
        held.push(ability.filter(|ability| *ability != AbilityType::AutoAttack && !held.contains(&Some(*ability))));
    }
    let mut spare = Loadout::DEFAULT.0.into_iter().chain(Loadout::pool()).filter(|ability| !held.contains(&Some(*ability))).collect::<Vec<_>>().into_iter();
    let mut bar = Loadout::DEFAULT;
    for (slot, ability) in held.into_iter().enumerate() {
        bar.0[slot] = ability.or_else(|| spare.next()).expect("the pool fills a bar");
    }
    bar
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two things under one number would each be kept as the other.
    #[test]
    fn every_number_is_one_things() {
        fn distinct(mut ids: Vec<u32>) -> bool {
            let count = ids.len();
            ids.sort();
            ids.dedup();
            ids.len() == count
        }
        assert!(distinct(Piece::all().map(Piece::id).collect()));
        assert!(distinct(Material::ALL.map(Material::id).to_vec()));
        assert!(distinct(AbilityType::ALL.map(AbilityType::id).to_vec()));
    }

    #[test]
    fn what_is_kept_reads_back_and_what_is_unknown_stays_as_kept() {
        let vest = Item { piece: Piece::named("ranger-vest"), seed: 5 };
        let wood = Stack { kind: Stackable::Material(Material::Hardwood), count: 7 };
        let gone = json!({"piece": 4_000_000, "seed": 1});
        let odd = json!({"coin": 3, "count": 2});
        assert_eq!(items(vec![gone.clone(), item(vest)]), (vec![vest], vec![gone]));
        assert_eq!(stacks(vec![stack(wood), odd.clone()]), (vec![wood], vec![odd]));
    }

    #[test]
    fn a_bar_keeps_every_skill_the_game_still_has() {
        let mut was = Loadout::DEFAULT.placed(AbilityType::Kick, 2);
        assert_eq!(bar(&loadout(&was)), was);
        let mut kept = loadout(&was);
        kept[5] = json!(4_000_000);
        was.0[5] = bar(&kept).0[5];
        assert_eq!(bar(&kept), was, "only the slot whose skill is gone changes");
        assert!(bar(&kept).fits());
        assert_eq!(bar(&json!("no bar")), Loadout::DEFAULT);
    }
}
