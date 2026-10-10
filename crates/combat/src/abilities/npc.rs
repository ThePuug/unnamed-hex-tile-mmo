//! When an NPC uses a skill: its skills channel ([`skills`]) weighs what
//! it perceives ([`perception`]), and it asks the gate for the best
//! decision that beats waiting, as a player's key press would.
//!
//! [`perception`]: crate::behaviour::perception

use bevy::prelude::*;
use common_bevy::{
    components::{entity_type::{actor::ActorIdentity, EntityType}, Loc},
    message::AbilityType,
    systems::targeting,
};

use super::{in_arc, Abilities};
use crate::leap::{away, toward};
use common_bevy::archetype::EnemyArchetype;
use crate::behaviour::{
    approach_of,
    perception::Skill,
    skills::{self, Foe, Threats, View},
};

impl Abilities<'_, '_> {
    /// Asks, for each NPC, the skill its skills channel chooses, if any.
    pub(super) fn skills(&mut self) {
        let npcs: Vec<(Entity, EnemyArchetype, Vec<AbilityType>)> = self.npcs.iter()
            .filter_map(|(ent, entity_type, bar)| match entity_type {
                EntityType::Actor(actor) => match actor.identity {
                    ActorIdentity::Npc(archetype) => Some((ent, archetype, bar.0.clone())),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        let mut asks: Vec<(Entity, AbilityType, Option<Entity>)> = Vec::new();
        for (ent, archetype, bar) in npcs {
            let Some(&ability) = bar.first() else { continue };
            let skill = self.minds.get(ent).map_or(Skill::SHARP, |(skill, _)| *skill);
            let target = self.targets.get(ent).ok().and_then(|(_, target)| target.entity);
            let foe = self.foe_of(ent, target);
            let now = self.time.elapsed();
            let reach = target.and_then(|target| self.reach_seen(ent, target));
            let foe = match self.minds.get_mut(ent) {
                Ok((_, mut sight)) => {
                    if let Some(reach) = reach {
                        sight.saw_strike(reach);
                    }
                    sight.look(&self.dice, ent, &skill, now, target.zip(foe))
                }
                Err(_) => foe,
            };
            // How long ago the foe's last skill was counts as much as its
            // Approach, on its target frame, makes it count
            let mind = self.mind_set.mind(Some(archetype));
            let approach = approach_of(target.and_then(|target| self.kinds.get(target).ok()));
            let foe = foe.map(|foe| Foe { since_skill: foe.since_skill.map(|since| since / mind.just_acted(approach)), ..foe });
            let Some(mut view) = self.view(ent, ability, &skill, foe, target) else { continue };
            let Ok(mut rolls) = self.rolls.get_mut(ent) else { continue };
            let dice = *self.dice;
            let stray = |decision: &skills::Decision| skill.error * dice.draw(&mut rolls, ("stray", ent, decision.ability, decision.reason)).signed();
            let chosen = skills::choose(&mut view, &bar, &mind, stray);
            if let (Some(decision), Some(decisions)) = (&chosen, self.decisions.as_mut()) {
                let mut ranked = skills::weigh(&mut view, &bar, &mind);
                ranked.sort_by(|a, b| b.score.total_cmp(&a.score));
                let line = ranked.iter().take(3).map(|weighed| {
                    let responses: Vec<String> = weighed.responses.iter().map(|(name, response)| format!("{name} {response:.2}")).collect();
                    format!("{:?} {} {:.2} [{}]", weighed.ability, weighed.reason, weighed.score, responses.join(", "))
                }).collect::<Vec<_>>().join(" | ");
                decisions.0.push(format!("{ent} uses {:?} to {} at {:.2}: {line}", decision.ability, decision.reason, decision.score));
            }
            let Some(decision) = chosen else { continue };
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

    /// How far off `target` stood as a threat of its, new in `ent`'s queue
    /// this frame, came in: what `ent` sees of how far it strikes from
    fn reach_seen(&self, ent: Entity, target: Entity) -> Option<i32> {
        let queue = self.queues.get(ent).ok()?;
        let game_now = self.game_now();
        let fresh = queue.threats.iter().any(|threat| threat.source == target && game_now.saturating_sub(threat.inserted_at) < self.time.delta() * 2);
        let (&loc, ..) = self.actors.get(ent).ok()?;
        let (&target_loc, ..) = self.actors.get(target).ok()?;
        fresh.then(|| loc.distance(&target_loc))
    }

    /// How `target` stands as `ent` would see it now
    fn foe_of(&self, ent: Entity, target: Option<Entity>) -> Option<Foe> {
        let tuning = *self.tuning;
        let (&loc, attrs, _, heading, ..) = self.actors.get(ent).ok()?;
        let (&target_loc, target_attrs, target_health, target_heading, ..) = self.actors.get(target?).ok()?;
        Some(Foe {
            at: target_loc,
            heading: target_heading.copied(),
            distance: loc.distance(&target_loc),
            health: target_health.state,
            in_arc: in_arc(&tuning, heading, Some(attrs), &loc, &target_loc),
            flanked: targeting::flanked(target_heading, &target_loc, &loc),
            patient: target_attrs.patience_crit(&tuning) * (1.0 + target_attrs.patience_power(&tuning)),
            since_skill: self.last_skills.get(target?).ok()
                .map(|last| self.time.elapsed().saturating_sub(last.0).as_secs_f32()),
            status: self.statuses.get(target?).ok().copied().unwrap_or_default(),
        })
    }

    /// Share of `ent`'s leash left where a Leap from `loc` lands, clear of
    /// `target` within `reach` or onto it beyond, as the Leap's own rule
    /// has it: 1 with no leash or no target, 0 with nowhere to land
    fn leap_room(&self, ent: Entity, loc: Loc, reach: i32, target: Option<Entity>) -> f32 {
        let tuning = *self.tuning;
        let Some(leash) = self.leash(ent) else { return 1.0 };
        let Some((&target_loc, ..)) = target.and_then(|target| self.actors.get(target).ok()) else { return 1.0 };
        let distance = self.actors.get(ent).map_or(tuning.leap_distance, |(_, attrs, ..)| attrs.leap_tiles(&tuning));
        let landing = if loc.distance(&target_loc) <= reach {
            away(&self.map, *loc, *target_loc, distance, Some(leash))
        } else {
            toward(&self.map, *loc, *target_loc, distance, Some(leash))
        };
        landing.map_or(0.0, |landing| (leash.reach - landing.flat_distance(&leash.den)) as f32 / leash.reach.max(1) as f32)
    }

    /// Whether as many others of `ent`'s engagement as it lets attack at
    /// once have an ability standing in `target`'s queue: a member holds
    /// its slot from its ability's use until the last threat it queued
    /// resolves.
    fn capacity_taken(&self, ent: Entity, target: Option<Entity>) -> bool {
        let Some(engagement) = self.leashed.get(ent).ok().map(|(_, member)| member.0) else { return false };
        let Some(capacity) = self.engagements.get(engagement).ok().map(|engagement| engagement.attack_capacity) else { return false };
        let Some(queue) = target.and_then(|target| self.queues.get(target).ok()) else { return false };
        let holders: std::collections::HashSet<Entity> = queue.threats.iter()
            .filter(|threat| !threat.is_pressure() && threat.source != ent)
            .filter(|threat| self.leashed.get(threat.source).is_ok_and(|(_, member)| member.0 == engagement))
            .map(|threat| threat.source)
            .collect();
        holders.len() >= capacity as usize
    }

    /// What `ent`, perceiving with `skill`, knows as it weighs `ability`
    /// against `foe`, its target as it perceives it; None for the dead. Its
    /// own state it knows at once; a threat only once its delay has run.
    fn view(&self, ent: Entity, ability: AbilityType, skill: &Skill, foe: Option<Foe>, target: Option<Entity>) -> Option<View> {
        let tuning = *self.tuning;
        let (&loc, &attrs, health, _, _, range, dead) = self.actors.get(ent).ok()?;
        if dead {
            return None;
        }
        let game_now = self.game_now();
        let queue: Vec<_> = self.queues.get(ent)
            .map(|queue| queue.threats.iter()
                .filter(|threat| skill.sees(&self.dice, ent, threat, game_now))
                .map(|threat| (skill.judged(&self.dice, ent, threat), *threat))
                .collect())
            .unwrap_or_default();
        let reach = range.copied().unwrap_or_default().0;
        Some(View {
            tuning,
            ability,
            attrs,
            health: health.state,
            endurance: self.endurance.get(ent).map_or(0.0, |endurance| endurance.state),
            endurance_max: self.endurance.get(ent).map_or(0.0, |endurance| endurance.max),
            recovery: self.recoveries.get(ent).ok().copied(),
            status: self.statuses.get(ent).ok().copied().unwrap_or_default(),
            reach,
            leap: attrs.leap_tiles(&tuning) as i32,
            leap_room: self.leap_room(ent, loc, reach, target),
            capacity_taken: self.capacity_taken(ent, target),
            queue: Threats::reading(&queue, attrs.span(&tuning), attrs.awareness_snap(&tuning), game_now),
            foe,
        })
    }
}
