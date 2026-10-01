//! When an NPC uses a skill: its skills channel ([`skills`]) weighs what
//! it perceives ([`perception`]), and it asks the gate for the best
//! decision that beats waiting, as a player's key press would.
//!
//! [`perception`]: crate::systems::behaviour::perception

use bevy::prelude::*;
use rand::Rng;
use common_bevy::{
    components::entity_type::{actor::ActorIdentity, EntityType},
    message::AbilityType,
    systems::targeting,
};

use super::{in_arc, Abilities};
use crate::systems::behaviour::{
    perception::Skill,
    skills::{self, Foe, Threats, View},
};

impl Abilities<'_, '_> {
    /// Asks, for each NPC, the skill its skills channel chooses, if any.
    pub(super) fn skills(&mut self) {
        let npcs: Vec<(Entity, AbilityType)> = self.npcs.iter()
            .filter_map(|(ent, entity_type)| match entity_type {
                EntityType::Actor(actor) => match actor.identity {
                    ActorIdentity::Npc(archetype) => Some((ent, archetype.profile().ability)),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        let mut asks: Vec<(Entity, AbilityType, Option<Entity>)> = Vec::new();
        let mut rng = rand::rng();
        for (ent, ability) in npcs {
            let bar = [ability];
            let skill = self.minds.get(ent).map_or(Skill::SHARP, |(skill, _)| *skill);
            let target = self.targets.get(ent).ok().and_then(|(_, target)| target.entity);
            let foe = self.foe_of(ent, target);
            let now = self.time.elapsed();
            let foe = match self.minds.get_mut(ent) {
                Ok((_, mut sight)) => sight.look(ent, &skill, now, target.zip(foe)),
                Err(_) => foe,
            };
            let Some(mut view) = self.view(ent, ability, &skill, foe) else { continue };
            let stray = || skill.error * rng.random_range(-1.0..=1.0);
            let Some(decision) = skills::choose(&mut view, &bar, stray) else { continue };
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

    /// How `target` stands as `ent` would see it now
    fn foe_of(&self, ent: Entity, target: Option<Entity>) -> Option<Foe> {
        let (&loc, attrs, _, heading, ..) = self.actors.get(ent).ok()?;
        let (&target_loc, ..) = self.actors.get(target?).ok()?;
        Some(Foe {
            distance: loc.distance(&target_loc),
            in_arc: in_arc(heading, Some(attrs), &loc, &target_loc),
            across: targeting::across(heading, &loc, &target_loc),
            recovering: self.recoveries.get(target?).ok()
                .filter(|recovery| recovery.is_active() && recovery.duration > 0.0)
                .map_or(0.0, |recovery| recovery.remaining / recovery.duration),
        })
    }

    /// What `ent`, perceiving with `skill`, knows as it weighs `ability`
    /// against `foe`, its target as it perceives it; None for the dead. Its
    /// own state it knows at once; a threat only once its delay has run.
    fn view(&self, ent: Entity, ability: AbilityType, skill: &Skill, foe: Option<Foe>) -> Option<View> {
        let (_, &attrs, health, _, _, range, dead) = self.actors.get(ent).ok()?;
        if dead {
            return None;
        }
        let tuning = common_bevy::tuning::tuning();
        let (now, game_now) = (self.time.elapsed(), self.game_now());
        let queue: Vec<_> = self.queues.get(ent)
            .map(|queue| queue.threats.iter().filter(|threat| skill.sees(ent, threat, game_now)).copied().collect())
            .unwrap_or_default();
        let swing = self.swings.get(ent).ok();
        let waited = swing.and_then(|swing| swing.waited(now));
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
            banking: swing.is_some_and(|swing| swing.due.is_some()),
            reach: range.copied().unwrap_or_default().0,
            leap: tuning.leap_distance as i32,
            queue: Threats::reading(&queue, attrs.span(), game_now),
            foe,
        })
    }
}
