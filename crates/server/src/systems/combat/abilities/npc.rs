//! When an NPC uses its archetype's skill.

use bevy::prelude::*;
use common_bevy::{
    components::entity_type::{actor::ActorIdentity, EntityType},
    message::AbilityType,
    systems::combat::combos::{may_use, reacts_through},
};

use super::{in_arc, Abilities};

impl Abilities<'_, '_> {
    /// Uses each NPC's skill where it would:
    /// - a strike (Frenzy, Feint) when its target stands within its reach
    ///   and arc;
    /// - a Parry or a Counter when threats stand in its queue;
    /// - a Leap clear of a target in its reach once it has traded, a blow
    ///   of its own landed since its last leap (`leap::Traded`), and threats
    ///   stand in its queue; and a Leap onto a target out of its reach;
    /// - a Perfect Stride when its target stands within its reach and it
    ///   is in none.
    ///
    /// Every use out of recovery waits out the NPC's `NpcRecovery` delay,
    /// armed once the skill is affordable, so NPCs that fire together drift
    /// apart. What it takes inside a recovery, the combo it was offered or
    /// a reaction its Preparation lets through, follows without the wait.
    /// The delay is spent as it asks, whether or not the gate then lets the
    /// skill through.
    pub(super) fn skills(&mut self) {
        let now = self.time.elapsed();
        let mut asks: Vec<(Entity, AbilityType, Option<Entity>)> = Vec::new();
        for (ent, entity_type, mut delay, traded) in &mut self.npcs {
            let EntityType::Actor(actor) = entity_type else { continue };
            let ActorIdentity::Npc(archetype) = actor.identity else { continue };
            let ability = archetype.profile().ability;
            let Ok((&loc, attrs, _, heading, _, range, _)) = self.actors.get(ent) else { continue };
            let own = range.copied().unwrap_or_default().0;

            let recovery = self.recoveries.get(ent).ok();
            let open = may_use(ability, recovery) || reacts_through(ability, recovery, Some(attrs));
            let affordable = self.stamina.get(ent).is_ok_and(|stamina| stamina.state >= common_bevy::tuning::tuning().cost(ability));
            if !open || !affordable {
                continue;
            }
            if !recovery.is_some_and(|recovery| recovery.is_active()) {
                delay.arm(now);
                if !delay.is_ready(now) {
                    continue;
                }
            }

            let threatened = self.queues.get(ent).is_ok_and(|queue| !queue.is_empty());
            let target = self.targets.get(ent).ok().and_then(|(_, target)| target.entity);
            let target_loc = target.and_then(|target| self.actors.get(target).ok()).map(|(&target_loc, ..)| target_loc);
            let in_reach = target_loc.is_some_and(|target_loc| loc.distance(&target_loc) <= own);
            let ask = match ability {
                AbilityType::Parry | AbilityType::Counter => threatened.then_some(None),
                AbilityType::Leap => target.filter(|_| !in_reach || (traded && threatened)).map(Some),
                AbilityType::PerfectStride => (in_reach && !self.striding.get(ent).is_ok_and(|stride| stride.until > now)).then_some(None),
                _ => target.zip(target_loc).filter(|(_, target_loc)| {
                    ability.reach(own).is_some_and(|reach| reach.contains(&loc.distance(target_loc)))
                        && in_arc(heading, Some(attrs), &loc, target_loc)
                }).map(|(target, _)| Some(target)),
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
