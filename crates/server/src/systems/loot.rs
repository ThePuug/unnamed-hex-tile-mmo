//! Loot: what an NPC leaves as it dies. An NPC holds nothing; its loot
//! enters play from the world as a pile where it stood, anyone's to take.

use bevy::prelude::*;
use common::{Material, Stack, Stackable};
use common_bevy::{
    archetype::EnemyArchetype,
    components::{
        entity_type::{actor::ActorIdentity, EntityType},
        position::Position,
        Loc,
    },
    message::{Do, Event},
};

use crate::systems::{
    economy::{Cause, Holder},
    gathering::Looters,
};

/// What one of `archetype` may leave as it dies, and the chance it does.
fn left_by(archetype: EnemyArchetype) -> Option<(Stack, f32)> {
    match archetype {
        EnemyArchetype::Berserker => Some((Stack { kind: Stackable::Material(Material::Hide), count: 1 }, 0.5)),
        _ => None,
    }
}

/// What a death of `archetype` leaves on `roll`, a share in `0..1`.
fn rolled(archetype: EnemyArchetype, roll: f32) -> Option<Stack> {
    left_by(archetype).filter(|&(_, chance)| roll < chance).map(|(stack, _)| stack)
}

/// Lays what each NPC that died leaves, where it stood, as a drop is laid
/// ([`Looters::lay`]). Where nowhere there takes it, it leaves nothing.
/// The deaths are read before anything is laid: laying tells the clients,
/// on the bus the deaths came by.
pub fn leave_loot(mut heard: ParamSet<(MessageReader<Do>, Looters)>, dice: Res<combat::dice::Dice>, actors: Query<(&EntityType, &Loc, &Position)>) {
    let died: Vec<Entity> = heard
        .p0()
        .read()
        .filter_map(|message| match message {
            Do { event: Event::Despawn { ent, died: true } } => Some(*ent),
            _ => None,
        })
        .collect();
    let mut looters = heard.p1();
    for ent in died {
        let Ok((EntityType::Actor(actor), loc, position)) = actors.get(ent) else { continue };
        let ActorIdentity::Npc(archetype) = actor.identity else { continue };
        let Some(stack) = rolled(archetype, dice.roll(("loot", ent)).share()) else { continue };
        let Some((q, r, slot)) = looters.lay(stack, (loc.q, loc.r), (position.tile.q, position.tile.r), position.offset.xz(), None) else {
            info!("loot: a {} died at ({}, {}), where nothing it leaves can lie", archetype.profile().name, loc.q, loc.r);
            continue;
        };
        looters.moved(stack, Holder::World, Holder::Pile(q, r, slot), Cause::Loot(archetype));
        info!("loot: a {} left {} {:?} in slot {slot} of ({q}, {r})", archetype.profile().name, stack.count, stack.kind);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A dog leaves a hide on some of its deaths and nothing on the rest;
    /// the lower the roll, the likelier it leaves one.
    #[test]
    fn a_dog_leaves_a_hide_by_chance() {
        let hide = Stackable::Material(Material::Hide);
        assert_eq!(rolled(EnemyArchetype::Berserker, 0.0).map(|stack| stack.kind), Some(hide));
        assert_eq!(rolled(EnemyArchetype::Berserker, 0.999_999), None);
        let leaves = |roll: f32| rolled(EnemyArchetype::Berserker, roll).is_some();
        let rolls: Vec<f32> = (0..100).map(|i| i as f32 / 100.0).collect();
        assert!(rolls.windows(2).all(|pair| leaves(pair[0]) >= leaves(pair[1])), "a lower roll never leaves less");
    }
}
