//! When an NPC uses a skill: its skills channel ([`skills`]) weighs what
//! it perceives, and it asks the gate for the best decision that beats
//! waiting, as a player's key press would.

use bevy::prelude::*;
use common_bevy::{
    components::entity_type::{actor::ActorIdentity, EntityType},
    message::AbilityType,
    systems::targeting,
};

use super::{in_arc, Abilities};
use crate::systems::behaviour::skills::{self, Foe, Threats, View};

impl Abilities<'_, '_> {
    /// Asks, for each NPC, the skill its skills channel chooses, if any.
    pub(super) fn skills(&mut self) {
        let mut asks: Vec<(Entity, AbilityType, Option<Entity>)> = Vec::new();
        for (ent, entity_type) in &self.npcs {
            let EntityType::Actor(actor) = entity_type else { continue };
            let ActorIdentity::Npc(archetype) = actor.identity else { continue };
            let bar = [archetype.profile().ability];
            let Some(mut view) = self.view(ent, bar[0]) else { continue };
            let Some(decision) = skills::choose(&mut view, &bar) else { continue };
            let target = self.targets.get(ent).ok().and_then(|(_, target)| target.entity);
            let at = match decision.ability {
                ability if ability.is_reaction() => None,
                AbilityType::PerfectStride => None,
                _ => target,
            };
            debug!("npc {ent} {:?} to {}: {:.2} {:?}", decision.ability, decision.reason, decision.score, decision.responses);
            asks.push((ent, decision.ability, at));
        }
        for (ent, ability, at) in asks {
            self.ask(ent, ability, at);
        }
    }

    /// What `ent` perceives as it weighs `ability`; None for the dead.
    fn view(&self, ent: Entity, ability: AbilityType) -> Option<View> {
        let (&loc, &attrs, health, heading, _, range, dead) = self.actors.get(ent).ok()?;
        if dead {
            return None;
        }
        let tuning = common_bevy::tuning::tuning();
        let now = self.time.elapsed();
        let queue: Vec<_> = self.queues.get(ent).map(|queue| queue.threats.iter().copied().collect()).unwrap_or_default();
        let foe = self.targets.get(ent).ok()
            .and_then(|(_, target)| target.entity)
            .and_then(|target| Some((target, self.actors.get(target).ok()?)))
            .map(|(target, (&target_loc, ..))| Foe {
                distance: loc.distance(&target_loc),
                in_arc: in_arc(heading, Some(&attrs), &loc, &target_loc),
                across: targeting::across(heading, &loc, &target_loc),
                recovering: self.recoveries.get(target).ok()
                    .filter(|recovery| recovery.is_active() && recovery.duration > 0.0)
                    .map_or(0.0, |recovery| recovery.remaining / recovery.duration),
            });
        let waited = self.swings.get(ent).ok().and_then(|swing| swing.waited(now));
        Some(View {
            ability,
            attrs,
            health: health.state,
            stamina: self.stamina.get(ent).map_or(0.0, |stamina| stamina.state),
            endurance: self.endurance.get(ent).map_or(0.0, |endurance| endurance.state),
            recovery: self.recoveries.get(ent).ok().copied(),
            striding: self.strides(ent),
            grit_held: self.grits.get(ent).map_or(0, |grit| grit.held),
            banked: waited.map_or(0, |waited| attrs.banked(waited, attrs.cadence_interval())),
            reach: range.copied().unwrap_or_default().0,
            leap: tuning.leap_distance as i32,
            queue: Threats::reading(&queue, attrs.span(), self.game_now()),
            foe,
        })
    }
}
