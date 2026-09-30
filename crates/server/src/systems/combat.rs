pub mod abilities;
pub mod landing;
pub mod leap;

use bevy::prelude::*;
use common_bevy::{
    components::{reaction_queue::*, resources::*, *},
    message::{Do, Try, Event as GameEvent},
    systems::{
        combat::{damage as damage_calc, queue as queue_utils},
    },
};

/// System to process DealDamage events (Phase 1: Outgoing damage calculation)
/// Rolls the attack's damage within its range (`Tuning::damage_spread`,
/// one roll for its blow and its DoT), rolls its blow's crit, and inserts
/// it into the reaction queue
pub fn process_deal_damage(
    trigger: On<Try>,
    _commands: Commands,
    mut target_query: Query<(&mut ReactionQueue, &ActorAttributes, &Health, Option<&Endurance>, Option<&mut common_bevy::components::recovery::GlobalRecovery>)>,
    mut combat_query: Query<&mut CombatState>,
    all_attrs: Query<&ActorAttributes>,
    time: Res<Time>,
    runtime: Res<crate::resources::RunTime>,
    mut writer: MessageWriter<Do>,
) {
    let tuning = common_bevy::tuning::tuning();
    let event = &trigger.event().event;

    if let GameEvent::DealDamage { source, target, base_damage, ability, dot } = event {
        // Get attacker attributes for scaling
        let Ok(source_attrs) = all_attrs.get(*source) else {
            return;
        };

        // Get target's queue, attributes, health, and recovery
        let Ok((mut queue, attrs, health, endurance, recovery_opt)) = target_query.get_mut(*target) else {
            return;
        };

        // Don't queue threats on dead targets
        if health.state <= 0.0 {
            return;
        }

        let draw = rand::Rng::random_range(&mut rand::rng(), -1.0..=1.0);
        let outgoing = damage_calc::spread(*base_damage, tuning.damage_spread, draw);
        let outgoing = damage_calc::crit(outgoing, source_attrs, rand::Rng::random_range(&mut rand::rng(), 0.0..1.0));
        let dot = damage_calc::spread(*dot, tuning.damage_spread, draw);

        // Use game world time (server uptime + offset) for consistent time base
        let now_ms = time.elapsed().as_millis() + runtime.elapsed_offset;
        let now = std::time::Duration::from_millis(now_ms.min(u64::MAX as u128) as u64);

        // A blow pushes its target's recovery back, by the attacker's Impact
        // over the target's Composure with the level gap weighing in
        if let Some(mut recovery) = recovery_opt {
            let pushback_pct = damage_calc::calculate_recovery_pushback(
                source_attrs.impact(),
                attrs.composure(),
                damage_calc::level_edge(source_attrs.total_level(), attrs.total_level()),
            );
            recovery.apply_pushback(pushback_pct);
            writer.write(Do { event: GameEvent::Incremental { ent: *target, component: common_bevy::message::Component::Recovery(*recovery) } });
        }

        // Create threat using canonical helper (INV-003: ensures consistent timers)
        let threat = queue_utils::create_threat(
            *source,       // Source entity
            attrs,         // Target attributes
            source_attrs,  // Source attributes
            outgoing,      // Damage
            *ability,      // Ability
            now,           // Current time
            dot,           // DoT per tick, a wound's
            Endurance::fatigue_of(endurance),
        );

        // Try to insert threat into queue
        let _overflow = queue_utils::insert_threat(&mut queue, threat, now);

        // Enter combat for both attacker and target AFTER threat is successfully inserted
        // Handle case where source == target (self-damage)
        if source == target {
            if let Ok(mut combat_state) = combat_query.get_mut(*source) {
                common_bevy::systems::combat::state::enter_combat(*source, &mut combat_state, &time, &mut writer);
            }
        } else {
            // Enter combat for attacker (put threat in queue)
            if let Ok(mut attacker_combat) = combat_query.get_mut(*source) {
                common_bevy::systems::combat::state::enter_combat(*source, &mut attacker_combat, &time, &mut writer);
            }
            // Enter combat for target (received threat in queue)
            if let Ok(mut target_combat) = combat_query.get_mut(*target) {
                common_bevy::systems::combat::state::enter_combat(*target, &mut target_combat, &time, &mut writer);
            }
        }

        // Send InsertThreat event to clients
        writer.write(Do {
            event: GameEvent::InsertThreat {
                ent: *target,
                threat,
            },
        });

        // Queue is unbounded, no overflow handling needed
    }
}

/// System to resolve threats (Phase 2: Apply passive modifiers and apply to health)
/// Processes ResolveThreat events: a threat whose time ran out, one dismissed, a Counter's reflection
pub fn resolve_threat(
    trigger: On<Try>,
    mut commands: Commands,
    mut query: Query<(&mut Health, &ActorAttributes, Option<&mut common_bevy::components::grit::Grit>)>,
    actors: Query<&ActorAttributes>,
    mut statuses: Query<&mut common_bevy::components::status::Status>,
    recoveries: Query<&common_bevy::components::recovery::GlobalRecovery>,
    locs: Query<&Loc>,
    mut bursts: Query<&mut crate::systems::combat::landing::VolleyBurst>,
    map: Res<common_bevy::resources::map::Map>,
    reach: landing::SpillReach,
    mut writer: MessageWriter<Do>,
) {
    let tuning = common_bevy::tuning::tuning();
    let event = &trigger.event().event;

    if let GameEvent::ResolveThreat { ent, threat } = event {
        if let Ok((mut health, attrs, grit)) = query.get_mut(*ent) {
            // The defender's Toughness meets the attacker's Presence, the level
            // edge the defender's against the attacker
            let (attacker_level, attacker_presence) = actors.get(threat.source)
                .map_or((attrs.total_level(), 0), |source| (source.total_level(), source.presence()));

            // Apply passive mitigation (unified for all damage types), less what the
            // ability pierces
            let mitigated = damage_calc::apply_passive_modifiers(threat.damage, attrs, attacker_presence, damage_calc::level_edge(attrs.total_level(), attacker_level));
            let pierce = threat.ability.map_or(0.0, |ability| tuning.pierce(ability));
            // The defender let this blow land: its Grit banks a share of it
            // for its next skill; a wound's DoT banks nothing
            let blow = mitigated + (threat.damage - mitigated) * pierce;
            if let Some(mut grit) = grit {
                grit.bank += blow * attrs.grit_bank();
            }
            let final_damage = blow + threat.dot_left();

            land_damage(*ent, threat.source, final_damage, threat.is_wound(), &mut health, &mut writer);

            landing::land(threat.ability, *ent, threat.source, threat.inserted_at, actors.get(threat.source).ok(), &tuning, &mut statuses, &recoveries, &locs, &mut bursts, &map, &mut commands, &mut writer);
            reach.spill(threat.source, *ent, threat.damage, &mut commands);

            // Death check moved to dedicated check_death system (decoupled from combat)
        }
    }
}

/// A wound's DoT tick lands on its target, outside the queue and past its
/// defences.
pub fn resolve_dot_tick(
    trigger: On<Try>,
    mut query: Query<&mut Health>,
    mut writer: MessageWriter<Do>,
) {
    let Try { event: GameEvent::DotTick { ent, source, damage, .. } } = trigger.event() else { return };
    let Ok(mut health) = query.get_mut(*ent) else { return };
    if health.state <= 0.0 {
        return;
    }
    land_damage(*ent, *source, *damage, true, &mut health, &mut writer);
}

/// Keeps what an actor banks to its fight. Out of combat a swing is due
/// and nothing is banked, neither Patience's swings nor Grit's blows; as
/// combat finds an actor not yet swinging its clock starts, that swing
/// due at once. So each banks only in the fight, and the fight's end
/// empties both.
pub fn bank_in_combat(mut query: Query<(&CombatState, &mut common_bevy::components::Swing, &mut common_bevy::components::grit::Grit)>, time: Res<Time>) {
    for (state, mut swing, mut grit) in &mut query {
        match (state.in_combat, swing.due) {
            (false, Some(_)) => swing.due = None,
            (true, None) => swing.due = Some(time.elapsed()),
            _ => {}
        }
        if !state.in_combat && grit.bank > 0.0 {
            grit.bank = 0.0;
        }
    }
}

/// A blow's spill lands on another hostile near its striker, outside the
/// queue: its share was already weighed against that hostile's Toughness.
pub fn resolve_spill(
    trigger: On<Try>,
    mut query: Query<&mut Health>,
    mut writer: MessageWriter<Do>,
) {
    let Try { event: GameEvent::Spill { ent, source, damage } } = trigger.event() else { return };
    let Ok(mut health) = query.get_mut(*ent) else { return };
    if health.state <= 0.0 {
        return;
    }
    land_damage(*ent, *source, *damage, false, &mut health, &mut writer);
}

/// Takes `damage` from `health` and tells every client: the one place
/// damage lands, whatever dealt it. `dot` marks a wound's, shown apart.
fn land_damage(ent: Entity, source: Entity, damage: f32, dot: bool, health: &mut Health, writer: &mut MessageWriter<Do>) {
    health.state = (health.state - damage).max(0.0);
    writer.write(Do { event: GameEvent::ApplyDamage { ent, damage, source, dot } });
    writer.write(Do { event: GameEvent::Incremental { ent, component: common_bevy::message::Component::Health(*health) } });
}


#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use common_bevy::components::{grit::Grit, Swing};
    use std::time::Duration;

    #[test]
    fn a_fight_starts_the_swing_clock_and_its_end_empties_both_banks() {
        let secs = Duration::from_secs;
        let mut world = World::new();
        let mut time = Time::<()>::default();
        time.advance_by(secs(10));
        world.insert_resource(time);
        let fighter = world.spawn((CombatState { in_combat: true, last_action: Duration::ZERO }, Swing::default(), Grit { bank: 50.0 })).id();

        world.run_system_once(bank_in_combat).unwrap();
        assert_eq!(world.get::<Grit>(fighter).unwrap().bank, 50.0, "in the fight, Grit keeps what it banked");
        let swing = *world.get::<Swing>(fighter).unwrap();
        assert_eq!(swing.waited(secs(10)), Some(Duration::ZERO), "due as the fight finds it");
        assert_eq!(swing.waited(secs(15)), Some(secs(5)), "and waiting from then");
        assert_eq!(Swing { due: Some(secs(20)) }.waited(secs(15)), None, "one still to come due waits for nothing");

        world.get_mut::<CombatState>(fighter).unwrap().in_combat = false;
        world.run_system_once(bank_in_combat).unwrap();
        assert_eq!(world.get::<Grit>(fighter).unwrap().bank, 0.0, "the fight over, it is gone");
        assert_eq!(world.get::<Swing>(fighter).unwrap().waited(secs(60)), Some(Duration::ZERO), "out of combat it is due and has banked nothing");
    }
}
