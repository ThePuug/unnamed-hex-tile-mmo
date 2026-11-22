//! When an NPC uses a skill: its skills channel ([`skills`]) weighs what
//! it perceives ([`perception`]), and it asks the gate for the best
//! decision that beats waiting, as a player's key press would.
//!
//! [`perception`]: crate::behaviour::perception

use bevy::prelude::*;
use common_bevy::{
    components::{entity_type::{actor::ActorIdentity, EntityType}, Loc},
    message::{AbilityType, SkillKind},
    moment::Moment,
    systems::{combat::damage, targeting},
};

use super::{strike, Abilities};
use crate::leap::{away, toward};
use common_bevy::archetype::EnemyArchetype;
use crate::behaviour::{
    perception::Skill,
    skills::{self, Foe, View},
};

impl Abilities<'_, '_> {
    /// Asks, for each NPC, the skill its skills channel chooses, if any:
    /// mid-activation the gate leaves it only its stances.
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
            let now = Moment::ZERO + self.time.elapsed();
            let foe = match self.minds.get_mut(ent) {
                Ok((_, mut sight)) => sight.look(&self.dice, ent, &skill, now, target.zip(foe)),
                Err(_) => foe,
            };
            let mind = self.mind_set.mind(Some(archetype));
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
            // A block and a stance are its own; every other skill is used
            // on its target
            let at = match decision.ability {
                ability if ability.blocks() || ability.kind() == SkillKind::Stance => None,
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
        let tuning = *self.tuning;
        let target = target?;
        let (&loc, attrs, heading, ..) = self.actors.get(ent).ok()?;
        let (&target_loc, target_attrs, target_heading, ..) = self.actors.get(target).ok()?;
        let health = self.health.get(target).ok()?;
        let level = damage::level_factor(&tuning, target_attrs.total_level(), attrs.total_level());
        let activation = self.activation_of(target);
        // What the strike it is activating at this one would land
        let strike = activation
            .filter(|activation| activation.target == Some(ent) && activation.ability.kind() == SkillKind::Strike)
            .map_or(0.0, |activation| {
                let lost = 1.0 - health.state / health.max.max(f32::EPSILON);
                target_attrs.base_potency(&tuning) * strike::share(&tuning, target_attrs, activation.ability, self.activation_of(ent).is_some(), lost) * level
            });
        // The swing clock runs on the server's own time, the game clock
        // an offset from it
        let elapsed = Moment::ZERO + self.time.elapsed();
        let swing = self.swings.get(target).ok().and_then(|swing| swing.due)
            .map(|due| self.game_now() + elapsed.until(due) - due.until(elapsed));
        Some(Foe {
            at: target_loc,
            heading: target_heading.copied(),
            distance: loc.distance(&target_loc),
            health: health.state,
            in_arc: targeting::faces(heading, &loc, &target_loc),
            flanked: targeting::flanked(target_heading, &target_loc, &loc),
            activation,
            strike,
            swing,
            auto: target_attrs.auto_damage(&tuning) * level,
            status: self.statuses.get(target).ok().copied().unwrap_or_default(),
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
    /// once are activating a skill at `target`: a member holds its slot
    /// from its skill's use until it lands.
    fn capacity_taken(&self, ent: Entity, target: Option<Entity>) -> bool {
        let Some(engagement) = self.leashed.get(ent).ok().map(|(_, member)| member.0) else { return false };
        let Some(capacity) = self.engagements.get(engagement).ok().map(|engagement| engagement.attack_capacity) else { return false };
        let Some(target) = target else { return false };
        let holders = self.skills.iter()
            .filter(|(other, activating, _)| *other != ent && activating.0.is_some_and(|activation| activation.target == Some(target)))
            .filter(|(other, ..)| self.leashed.get(*other).is_ok_and(|(_, member)| member.0 == engagement))
            .count();
        holders >= capacity as usize
    }

    /// What `ent`, perceiving with `skill`, knows as it weighs `ability`
    /// against `foe`, its target as it perceives it; None for the dead. Its
    /// own state it knows at once.
    fn view(&self, ent: Entity, ability: AbilityType, skill: &Skill, foe: Option<Foe>, target: Option<Entity>) -> Option<View> {
        let tuning = *self.tuning;
        let (&loc, &attrs, _, _, range, _) = self.actors.get(ent).ok()?;
        if !self.lives(ent) {
            return None;
        }
        let now = self.game_now();
        let health = self.health.get(ent).ok()?;
        let reach = range.copied().unwrap_or_default().0;
        let (activating, recharges) = self.skills.get(ent).map_or_else(|_| (false, default()), |(_, activating, recharges)| (activating.0.is_some(), recharges.clone()));
        let lands_in = foe.and_then(|foe| foe.activation)
            .map(|activation| now.until(skill.judged(&self.dice, ent, &activation)).as_secs_f32());
        Some(View {
            tuning,
            ability,
            attrs,
            health: health.state,
            health_max: health.max,
            endurance: self.endurance.get(ent).map_or(0.0, |endurance| endurance.state),
            endurance_max: self.endurance.get(ent).map_or(0.0, |endurance| endurance.max),
            now,
            activating,
            recharges,
            status: self.statuses.get(ent).ok().copied().unwrap_or_default(),
            reach,
            leap: attrs.leap_tiles(&tuning) as i32,
            leap_room: self.leap_room(ent, loc, reach, target),
            capacity_taken: self.capacity_taken(ent, target),
            lands_in,
            foe,
            runes: self.runes.iter().filter(|(_, _, stone)| stone.caster == ent).map(|(_, _, stone)| stone.rune.kind()).collect(),
        })
    }
}
