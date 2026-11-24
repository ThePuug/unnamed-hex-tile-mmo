//! An economy interaction as the ledger keeps it: what moved by its
//! number ([`stored`]), each holder by what the store knows it as, and
//! the cause by its number.

use common::network::Identity;
use serde_json::{json, Value};

use super::{store::Entry, stored};
use crate::systems::economy::{Cause, Holder, Moved, Thing};

fn holder(holder: Holder, character: &impl Fn(Identity) -> Option<i64>) -> Option<Value> {
    Some(match holder {
        Holder::World => json!("world"),
        Holder::Character(identity) => json!({"character": character(identity)?}),
        Holder::Pile(q, r, slot) => json!({"pile": [q, r, slot]}),
        Holder::Camp(q, r) => json!({"camp": [q, r]}),
        Holder::Depot(q, r) => json!({"depot": [q, r]}),
    })
}

/// `moved` as the ledger keeps it, each player's character by the id
/// `character` gives it; none where it names a player this server holds
/// no character of.
pub fn entry(moved: &Moved, character: impl Fn(Identity) -> Option<i64>) -> Option<Entry> {
    Some(Entry {
        item: match moved.what {
            Thing::Stack(kind) => stored::kind(kind),
            Thing::Piece(item) => stored::item(item),
        },
        count: moved.count as i64,
        source: holder(moved.from, &character)?,
        dest: holder(moved.to, &character)?,
        cause: moved.cause.id() as i32,
        npc: match moved.cause {
            Cause::Loot(archetype) => Some(archetype.id() as i32),
            _ => None,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::{Material, Stack, Stackable};
    use common_bevy::archetype::EnemyArchetype;

    const WHO: Identity = Identity::Key([7; 16]);

    /// An entry names what moved and its cause by their numbers, a pile
    /// by its place and a character by its id; one naming a player this
    /// server holds none of is no entry.
    #[test]
    fn an_entry_names_everything_by_what_the_store_knows_it_as() {
        let hide = Stack { kind: Stackable::Material(Material::Hide), count: 1 };
        let loot = Moved::stack(hide, Holder::World, Holder::Pile(4, -9, 2), Cause::Loot(EnemyArchetype::Berserker));
        let kept = entry(&loot, |_| None).expect("no character is named");
        assert_eq!((kept.item, kept.count), (json!({"material": Material::Hide.id()}), 1));
        assert_eq!((kept.source, kept.dest), (json!("world"), json!({"pile": [4, -9, 2]})));
        assert_eq!((kept.cause, kept.npc), (Cause::Loot(EnemyArchetype::Berserker).id() as i32, Some(EnemyArchetype::Berserker.id() as i32)));

        let take = Moved::stack(hide, Holder::Pile(4, -9, 2), Holder::Character(WHO), Cause::Take);
        let kept = entry(&take, |_| Some(31)).expect("its character is held");
        assert_eq!((kept.dest, kept.npc), (json!({"character": 31}), None));
        assert!(entry(&take, |_| None).is_none());
    }
}
