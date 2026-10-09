//! # Combat
//!
//! The fight as the live server runs it, with no networking: damage and
//! threats, the reaction queue, every ability through one gate, targeting,
//! how NPCs behave and choose, and the engagements they fight in. The
//! server installs it with its network round it; the balance arena
//! (`crates/arena`) installs it bare, so the arena fights by the rules
//! players meet.

pub mod abilities;
pub mod actor;
pub mod behaviour;
pub mod dice;
pub mod engagement;
pub mod landing;
pub mod leap;
mod plugin;
pub mod reaction_queue;
pub mod targeting;

pub use plugin::{BehaviourPlugin, CombatPlugin};

use bevy::prelude::*;
use common_bevy::{
    components::{reaction_queue::*, resources::*, *},
    message::{Do, Try, Event as GameEvent},
    systems::{
        combat::{damage as damage_calc, queue as queue_utils},
    },
};
use common_bevy::tuning::Tuning;

#[derive(Default, Resource)]
pub struct RunTime {
    pub elapsed_offset: u128,
}

/// System to process DealDamage events (Phase 1: Outgoing damage calculation)
/// An attack made at a target with Patience overcommits its source
/// (`Status::overcommit`). Lands a strike from past its target's forward
/// faces, a flank, harder by its striker's Grace (`ActorAttributes::flank`),
/// weighs its blow and DoT by the level gap (`damage::level_factor`), rolls
/// the attack's
/// damage within its range (`Tuning::damage_spread`, one roll for its blow
/// and its DoT), rolls its blow's crit, and inserts
/// it into the reaction queue, its window starting as the strike is made:
/// now, or its `delay` after
pub fn process_deal_damage(
    trigger: On<Try>,
    tuning: Res<Tuning>,
    mut commands: Commands,
    mut statuses: Query<&mut common_bevy::components::status::Status>,
    mut target_query: Query<(&mut ReactionQueue, &ActorAttributes, &Health, Option<&Endurance>, Option<&mut common_bevy::components::recovery::GlobalRecovery>)>,
    mut combat_query: Query<&mut CombatState>,
    all_attrs: Query<&ActorAttributes>,
    places: Query<(&Loc, Option<&common_bevy::components::heading::Heading>)>,
    time: Res<Time>,
    runtime: Res<crate::RunTime>,
    dice: Res<dice::Dice>,
    mut rolls: Query<&mut dice::Rolls>,
    mut writer: MessageWriter<Do>,
) {
    let event = &trigger.event().event;

    if let GameEvent::DealDamage { source, target, base_damage, ability, dot, bind, delay } = event {
        // Get attacker attributes for scaling
        let (Ok(source_attrs), Ok(mut rolls)) = (all_attrs.get(*source), rolls.get_mut(*source)) else {
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

        if attrs.patience().index() > 0 && source != target {
            landing::update(*source, &mut statuses, &mut commands, &mut writer, |status| status.overcommit(tuning.overcommit_secs));
        }

        let flanked = places.get(*source).ok().zip(places.get(*target).ok()).is_some_and(|((&from, _), (&at, heading))| {
            heading.is_some_and(|&heading| !common_bevy::systems::targeting::is_in_facing_cone(heading, at, from))
        });
        let base_damage = if flanked { base_damage * (1.0 + source_attrs.flank(&tuning)) } else { *base_damage };
        let gap = damage_calc::level_factor(&tuning, source_attrs.total_level(), attrs.total_level());
        let base_damage = base_damage * gap;
        let draw = dice.draw(&mut rolls, ("spread", *source)).signed();
        let outgoing = damage_calc::spread(base_damage, tuning.damage_spread, draw);
        // A skill crits an overcommitted foe likelier by its striker's Patience
        let patient = if *ability == Some(common_bevy::message::AbilityType::AutoAttack) { 0.0 } else {
            source_attrs.patience_crit(&tuning) * statuses.get(*target).map_or(0, |status| status.overcommits()) as f32
        };
        let outgoing = damage_calc::crit(&tuning, outgoing, source_attrs, attrs, patient, dice.draw(&mut rolls, ("crit", *source)).share());
        let dot = damage_calc::spread(*dot * gap, tuning.damage_spread, draw);

        // Use game world time (server uptime + offset) for consistent time base
        let now_ms = time.elapsed().as_millis() + runtime.elapsed_offset;
        let now = std::time::Duration::from_millis(now_ms.min(u64::MAX as u128) as u64) + *delay;

        // A blow pushes its target's recovery back, by the attacker's Impact
        // over the target's Efficiency with the level gap weighing in
        if let Some(mut recovery) = recovery_opt {
            let pushback_pct = damage_calc::calculate_recovery_pushback(
                &tuning,
                source_attrs.impact(),
                attrs.efficiency(),
                damage_calc::level_edge(&tuning, source_attrs.total_level(), attrs.total_level()),
            );
            recovery.apply_pushback(pushback_pct);
            writer.write(Do { event: GameEvent::Incremental { ent: *target, component: common_bevy::message::Component::Recovery(*recovery) } });
        }

        // Create threat using canonical helper (INV-003: ensures consistent timers)
        let threat = queue_utils::create_threat(
            &tuning,
            *source,       // Source entity
            attrs,         // Target attributes
            source_attrs,  // Source attributes
            outgoing,      // Damage
            *ability,      // Ability
            now,           // When the strike is made
            dot,           // DoT per tick, a wound's
            Endurance::fatigue_of(&tuning, endurance),
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
    tuning: Res<Tuning>,
    mut commands: Commands,
    mut query: Query<&mut Health>,
    mut statuses: Query<&mut common_bevy::components::status::Status>,
    mut writer: MessageWriter<Do>,
) {
    let event = &trigger.event().event;

    if let GameEvent::ResolveThreat { ent, threat } = event {
        if let Ok(mut health) = query.get_mut(*ent) {
            let final_damage = threat.damage + threat.dot_left();

            land_damage(*ent, threat.source, final_damage, threat.is_wound(), &mut health, &mut writer);

            // A blow Intimidation's bank struck back binds its target as it
            // lands: it slows, or roots a target already slowed
            if threat.bind > 0.0 {
                landing::update(*ent, &mut statuses, &mut commands, &mut writer, |status| {
                    if status.is_slowed() {
                        status.root(tuning.intimidation_root_secs);
                    } else {
                        status.slow(1.0 - threat.bind, tuning.intimidation_slow_secs);
                    }
                });
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
/// swing waits. The fight's end empties Intimidation's bank.
#[allow(clippy::too_many_arguments)]
pub fn track_engagement(
    mut query: Query<(Entity, &CombatState, &Loc, Option<&common_bevy::components::behaviour::Side>, &mut common_bevy::components::Swing, &mut common_bevy::components::intimidation::Intimidation)>,
    others: Query<(&common_bevy::components::behaviour::Side, &Health)>,
    nntree: Res<common_bevy::plugins::nntree::NNTree>,
    time: Res<Time>,
) {
    for (ent, state, &loc, side, mut swing, mut intimidation) in &mut query {
        let hostile_near = side.is_some_and(|&side| {
            crate::behaviour::spotted(&nntree, loc, crate::behaviour::ACQUISITION_RANGE)
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
            intimidation.filled = 0.0;
        }
    }
}

/// Physique's Intimidation at work, for an actor committed to it: each
/// living hostile within its reach that is not targeting it is slowed out of
/// `Tuning::intimidation_slow` of its speed, lingering
/// `Tuning::intimidation_aura_secs` past it, and while it is in a fight its
/// bank fills each second by its tier, twice that while any hostile is
/// ignoring it so. A slow is renewed only once half spent, so the clients
/// are told of it a couple of times a second, not every tick.
#[allow(clippy::too_many_arguments)]
pub fn intimidate(
    tuning: Res<Tuning>,
    time: Res<Time>,
    mut commands: Commands,
    mut writer: MessageWriter<Do>,
    mut actors: Query<(Entity, &Loc, &ActorAttributes, &CombatState, &common_bevy::components::behaviour::Side, Option<&AttackRange>, &mut common_bevy::components::intimidation::Intimidation)>,
    others: Query<(&common_bevy::components::behaviour::Side, &Health, Option<&common_bevy::components::target::Target>)>,
    mut statuses: Query<&mut common_bevy::components::status::Status>,
    nntree: Res<common_bevy::plugins::nntree::NNTree>,
) {
    let dt = time.delta_secs();
    let pace = 1.0 - tuning.intimidation_slow;
    for (ent, &loc, attrs, state, &side, range, mut intimidation) in &mut actors {
        let fill = attrs.intimidation_fill();
        if fill <= 0.0 {
            continue;
        }
        let reach = range.copied().unwrap_or_default().0.max(0) as u32;
        let ignoring: Vec<Entity> = crate::behaviour::spotted(&nntree, loc, reach)
            .filter(|&other| other != ent)
            .filter(|&other| others.get(other).is_ok_and(|(other_side, health, target)| {
                side.is_hostile_to(*other_side) && health.state > 0.0 && target.and_then(|target| target.entity) != Some(ent)
            }))
            .collect();
        if state.in_combat {
            intimidation.take(&tuning, fill * if ignoring.is_empty() { 1.0 } else { 2.0 } * dt);
        }
        for other in ignoring {
            let held = statuses.get(other).ok().and_then(|status| status.slow)
                .is_some_and(|slow| slow.pace <= pace && slow.remaining > tuning.intimidation_aura_secs / 2.0);
            if !held {
                landing::update(other, &mut statuses, &mut commands, &mut writer, |status| status.slow(pace, tuning.intimidation_aura_secs));
            }
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
    use common_bevy::components::{intimidation::Intimidation, Swing};
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
        let waiting = app.world_mut().spawn((calm, at(0), Side::WILD, Health::full(100.0), Swing::default(), Intimidation::default())).id();
        app.world_mut().entity_mut(waiting).insert(NearestNeighbor::new(waiting, at(0)));
        app.update();
        engaging(app.world_mut());
        assert_eq!(app.world().get::<Swing>(waiting).unwrap().due, None, "with no hostile near it banks nothing");

        let far = crate::behaviour::ACQUISITION_RANGE as i32 - 1;
        let hostile = app.world_mut().spawn((at(far), Side::PLAYERS, Health::full(100.0))).id();
        app.world_mut().entity_mut(hostile).insert(NearestNeighbor::new(hostile, at(far)));
        app.update();
        engaging(app.world_mut());
        assert!(app.world().get::<Swing>(waiting).unwrap().due.is_some(), "a hostile within range starts its clock, out of combat");
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
        let fighter = world.spawn((CombatState { in_combat: true, last_action: Duration::ZERO }, here, Swing::default(), Intimidation { filled: 4.0 })).id();

        engaging(world);
        assert_eq!(world.get::<Intimidation>(fighter).unwrap().filled, 4.0, "in the fight, Intimidation keeps what it banked");
        let swing = *world.get::<Swing>(fighter).unwrap();
        assert_eq!(swing.waited(secs(10)), Some(Duration::ZERO), "due as the fight finds it");
        assert_eq!(swing.waited(secs(15)), Some(secs(5)), "and waiting from then");
        assert_eq!(Swing { due: Some(secs(20)) }.waited(secs(15)), None, "one still to come due waits for nothing");

        world.get_mut::<CombatState>(fighter).unwrap().in_combat = false;
        engaging(world);
        assert_eq!(world.get::<Intimidation>(fighter).unwrap().filled, 0.0, "the fight over, it is gone");
        assert_eq!(world.get::<Swing>(fighter).unwrap().waited(secs(60)), Some(Duration::ZERO), "disengaged, it is due and has waited no time");
    }

    #[test]
    fn intimidation_slows_whoever_looks_elsewhere_and_fills_faster_ignored() {
        use common_bevy::{components::{behaviour::Side, status::Status, target::Target, AttackRange, Loc}, plugins::nntree::{NNTreePlugin, NearestNeighbor}};
        let mut app = App::new();
        app.add_plugins(NNTreePlugin);
        app.add_message::<Do>();
        app.init_resource::<Tuning>();
        app.init_resource::<Time>();
        let at = |q: i32| Loc::new(qrz::Qrz { q, r: 0, z: 0 });
        let fighting = CombatState { in_combat: true, last_action: Duration::ZERO };
        let attrs = ActorAttributes::new(0, 0, 0, -10, 0, 0, 0, 0, 0);
        let imposing = app.world_mut().spawn((fighting, at(0), attrs, Side::PLAYERS, AttackRange::default(), Intimidation::default(), Health::full(100.0))).id();
        let watcher = app.world_mut().spawn((at(1), Side::WILD, Health::full(100.0), Target { entity: Some(imposing), last_target: None })).id();
        for (ent, q) in [(imposing, 0), (watcher, 1)] {
            app.world_mut().entity_mut(ent).insert(NearestNeighbor::new(ent, at(q)));
        }
        app.update();
        let filled = |app: &App| app.world().get::<Intimidation>(imposing).unwrap().filled;
        let slowed = |app: &App, ent| app.world().get::<Status>(ent).is_some_and(Status::is_slowed);

        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(1));
        app.world_mut().run_system_once(intimidate).unwrap();
        let watched = filled(&app);
        assert!(watched > 0.0, "fought one on one, it fills");
        assert!(!slowed(&app, watcher), "and whoever watches it goes free");

        let ignorer = app.world_mut().spawn((at(-1), Side::WILD, Health::full(100.0), Target::default())).id();
        app.world_mut().entity_mut(ignorer).insert(NearestNeighbor::new(ignorer, at(-1)));
        app.update();
        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(1));
        app.world_mut().run_system_once(intimidate).unwrap();
        assert!(filled(&app) - watched > watched, "ignored, it fills faster");
        assert!(slowed(&app, ignorer), "and whoever looks elsewhere is slowed");
        assert!(!slowed(&app, watcher));
    }
}
