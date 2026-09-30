//! When an NPC uses its archetype's signature.

use bevy::prelude::*;
use common_bevy::{
    components::entity_type::{actor::ActorIdentity, EntityType},
    message::AbilityType,
    systems::combat::synergies::reacts_through,
};

use super::{in_arc, Abilities};

impl Abilities<'_, '_> {
    /// Uses each NPC's signature where it would: a strike (Lunge, Rattle,
    /// Volley, Flank) when its target stands within the ability's reach and
    /// the NPC's arc; a Counter when threats stand in its queue; a Disengage
    /// from the blow at the front of its queue, an auto-attack's as
    /// overflow.
    ///
    /// Every use waits out the NPC's `NpcRecovery` delay, armed once the
    /// ability is affordable and out of lockout, or a reaction its
    /// Preparation lets through the lockout, so NPCs that fire together
    /// drift apart. The delay is spent as it asks, whether or not the gate
    /// then lets the ability through.
    pub(super) fn signatures(&mut self) {
        let now = self.time.elapsed();
        let mut asks: Vec<(Entity, AbilityType, Option<Entity>)> = Vec::new();
        for (ent, entity_type, mut delay) in &mut self.npcs {
            let EntityType::Actor(actor) = entity_type else { continue };
            let ActorIdentity::Npc(archetype) = actor.identity else { continue };
            let ability = archetype.profile().ability;
            let Ok((&loc, attrs, _, heading, _, range, _)) = self.actors.get(ent) else { continue };

            // Out of lockout, or a reaction its Preparation lets through it, and affordable
            let recovery = self.lockouts.get(ent).ok();
            let locked = recovery.is_some_and(|recovery| recovery.is_active());
            let affordable = self.stamina.get(ent).is_ok_and(|stamina| stamina.state >= common_bevy::tuning::tuning().cost(ability));
            if (locked && !reacts_through(ability, recovery, Some(attrs))) || !affordable {
                continue;
            }
            delay.arm(now);
            if !delay.is_ready(now) {
                continue;
            }

            let queue = self.queues.get(ent).ok();
            let ask = match ability {
                AbilityType::Disengage => queue.and_then(|queue| queue.threats.front().map(|blow| Some(blow.source))),
                AbilityType::Counter => queue.filter(|queue| !queue.threats.is_empty()).map(|_| None),
                _ => self.targets.get(ent).ok().and_then(|(_, target)| target.entity).filter(|&target| {
                    let reach = ability.reach(range.copied().unwrap_or_default().0);
                    self.actors.get(target).is_ok_and(|(target_loc, ..)| {
                        reach.is_some_and(|reach| reach.contains(&loc.distance(target_loc)))
                            && in_arc(heading, Some(attrs), &loc, target_loc)
                    })
                }).map(Some),
            };
            if let Some(asked) = ask {
                delay.spend();
                asks.push((ent, ability, asked));
            }
        }
        for (ent, ability, asked) in asks {
            self.ask(ent, ability, asked);
        }
    }
}
