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
/// it into the reaction queue, its window starting as the strike is made:
/// now, or its `delay` after
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

    if let GameEvent::DealDamage { source, target, base_damage, ability, dot, bind, delay } = event {
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
        let outgoing = damage_calc::crit(outgoing, source_attrs, attrs, rand::Rng::random_range(&mut rand::rng(), 0.0..1.0));
        let dot = damage_calc::spread(*dot, tuning.damage_spread, draw);

        // Use game world time (server uptime + offset) for consistent time base
        let now_ms = time.elapsed().as_millis() + runtime.elapsed_offset;
        let now = std::time::Duration::from_millis(now_ms.min(u64::MAX as u128) as u64) + *delay;

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
            now,           // When the strike is made
            dot,           // DoT per tick, a wound's
            Endurance::fatigue_of(endurance),
        ).binding(*bind);

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
    mut statuses: Query<&mut common_bevy::components::status::Status>,
    mut writer: MessageWriter<Do>,
) {
    let tuning = common_bevy::tuning::tuning();
    let event = &trigger.event().event;

    if let GameEvent::ResolveThreat { ent, threat } = event {
        if let Ok((mut health, attrs, grit)) = query.get_mut(*ent) {
            // The defender let this blow land: its Grit's bank fills by its
            // tier; a wound's DoT fills nothing
            let blow = threat.damage;
            if let Some(mut grit) = grit {
                grit.take(attrs.grit_fill());
            }
            let final_damage = blow + threat.dot_left();

            land_damage(*ent, threat.source, final_damage, threat.is_wound(), &mut health, &mut writer);

            // A blow Grit's bank struck back binds its target as it lands
            if threat.bind > 0.0 {
                let seconds = tuning.grit_bind_secs;
                landing::update(*ent, &mut statuses, &mut commands, &mut writer, |status| status.slow(1.0 - threat.bind, seconds));
            }

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

/// Keeps an actor's swing clock to its engagement. The clock runs while it
/// is engaged: in combat, or with a living hostile within the range a fight
/// is taken up at (`behaviour::ACQUISITION_RANGE`), first contact or not.
/// As it is engaged its clock starts, a swing due at once; disengaged, no
/// swing waits. A swing that has come due and gone unstruck leaves it
/// waiting (`Status::waiting`), and Patience refills its stamina faster,
/// until it swings or uses a skill. Grit banks only blows, so only in the
/// fight, and the fight's end empties it.
#[allow(clippy::too_many_arguments)]
pub fn track_engagement(
    mut commands: Commands,
    mut writer: MessageWriter<Do>,
    mut query: Query<(Entity, &CombatState, &Loc, Option<&common_bevy::components::behaviour::Side>, &mut common_bevy::components::Swing, &mut common_bevy::components::grit::Grit, Option<&crate::systems::combat::abilities::LastSkill>)>,
    others: Query<(&common_bevy::components::behaviour::Side, &Health)>,
    mut statuses: Query<&mut common_bevy::components::status::Status>,
    nntree: Res<common_bevy::plugins::nntree::NNTree>,
    time: Res<Time>,
) {
    let now = time.elapsed();
    for (ent, state, &loc, side, mut swing, mut grit, last_skill) in &mut query {
        let hostile_near = side.is_some_and(|&side| {
            crate::systems::behaviour::spotted(&nntree, loc, crate::systems::behaviour::ACQUISITION_RANGE)
                .filter(|&other| other != ent)
                .any(|other| others.get(other).is_ok_and(|(other_side, health)| side.is_hostile_to(*other_side) && health.state > 0.0))
        });
        let engaged = state.in_combat || hostile_near;
        match (engaged, swing.due) {
            (false, Some(_)) => swing.due = None,
            (true, None) => swing.due = Some(time.elapsed()),
            _ => {}
        }
        if !state.in_combat {
            grit.filled = 0;
        }
        let waiting = swing.due.is_some_and(|due| due < now && last_skill.is_none_or(|last| last.0 < due));
        if statuses.get(ent).map_or(false, |status| status.waiting) != waiting {
            landing::update(ent, &mut statuses, &mut commands, &mut writer, |status| status.waiting = waiting);
        }
    }
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

    fn engaging(world: &mut World) {
        world.run_system_once(track_engagement).unwrap();
    }

    #[test]
    fn a_hostile_in_range_starts_the_swing_clock_before_first_contact() {
        use common_bevy::{components::{behaviour::Side, Loc}, plugins::nntree::{NNTreePlugin, NearestNeighbor}};
        let mut app = App::new();
        app.add_plugins(NNTreePlugin);
        app.add_message::<Do>();
        app.init_resource::<Time>();
        let at = |q: i32| Loc::new(qrz::Qrz { q, r: 0, z: 0 });
        let calm = CombatState { in_combat: false, last_action: Duration::ZERO };
        let waiting = app.world_mut().spawn((calm, at(0), Side::WILD, Health::full(100.0), Swing::default(), Grit::default())).id();
        app.world_mut().entity_mut(waiting).insert(NearestNeighbor::new(waiting, at(0)));
        app.update();
        engaging(app.world_mut());
        assert_eq!(app.world().get::<Swing>(waiting).unwrap().due, None, "with no hostile near it banks nothing");

        let far = crate::systems::behaviour::ACQUISITION_RANGE as i32 - 1;
        let hostile = app.world_mut().spawn((at(far), Side::PLAYERS, Health::full(100.0))).id();
        app.world_mut().entity_mut(hostile).insert(NearestNeighbor::new(hostile, at(far)));
        app.update();
        engaging(app.world_mut());
        assert!(app.world().get::<Swing>(waiting).unwrap().due.is_some(), "a hostile within range starts its clock, out of combat");

        let waits = |app: &App| app.world().get::<common_bevy::components::status::Status>(waiting).is_some_and(|status| status.waiting);
        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(1));
        engaging(app.world_mut());
        assert!(waits(&app), "its swing come due and unstruck, it waits");
        let now = app.world().resource::<Time>().elapsed();
        app.world_mut().entity_mut(waiting).insert(crate::systems::combat::abilities::LastSkill(now));
        engaging(app.world_mut());
        assert!(!waits(&app), "a skill used ends it");
    }

    #[test]
    fn a_fight_starts_the_swing_clock_and_its_end_empties_both_banks() {
        let secs = Duration::from_secs;
        let mut app = App::new();
        app.add_plugins(common_bevy::plugins::nntree::NNTreePlugin);
        app.add_message::<Do>();
        let world = app.world_mut();
        let mut time = Time::<()>::default();
        time.advance_by(secs(10));
        world.insert_resource(time);
        let here = Loc::new(qrz::Qrz { q: 0, r: 0, z: 0 });
        let fighter = world.spawn((CombatState { in_combat: true, last_action: Duration::ZERO }, here, Swing::default(), Grit { filled: 4 })).id();

        engaging(world);
        assert_eq!(world.get::<Grit>(fighter).unwrap().filled, 4, "in the fight, Grit keeps what it banked");
        let swing = *world.get::<Swing>(fighter).unwrap();
        assert_eq!(swing.waited(secs(10)), Some(Duration::ZERO), "due as the fight finds it");
        assert_eq!(swing.waited(secs(15)), Some(secs(5)), "and waiting from then");
        assert_eq!(Swing { due: Some(secs(20)) }.waited(secs(15)), None, "one still to come due waits for nothing");

        world.get_mut::<CombatState>(fighter).unwrap().in_combat = false;
        engaging(world);
        assert_eq!(world.get::<Grit>(fighter).unwrap().filled, 0, "the fight over, it is gone");
        assert_eq!(world.get::<Swing>(fighter).unwrap().waited(secs(60)), Some(Duration::ZERO), "disengaged, it is due and has waited no time");
    }
}
