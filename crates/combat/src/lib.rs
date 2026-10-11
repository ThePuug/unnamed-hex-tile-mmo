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
    moment::Moment,
    systems::{
        combat::{damage as damage_calc, queue as queue_utils},
    },
};
use common_bevy::tuning::Tuning;

/// The game's clock: what the server's `Time` has run, in milliseconds,
/// from `elapsed_offset` (0 live; a test starts it where it likes). A
/// threat's times, a press and a client's `Init` and `Pong` are all
/// stamped on it, each a [`Moment`]. The calendar is not: `wall_at_zero`
/// is the wall-clock moment the clock read 0 at, sent to clients once and
/// read by the calendar and the sky alone. A moment of the wall clock is
/// billions of milliseconds; as a float it keeps minutes, so it never
/// reaches timing.
#[derive(Default, Resource)]
pub struct RunTime {
    pub elapsed_offset: u128,
    pub wall_at_zero: u128,
}

impl RunTime {
    /// The game's clock at `time`
    pub fn now(&self, time: &Time) -> Moment {
        Moment::from_millis(time.elapsed().as_millis() as u64 + self.elapsed_offset as u64)
    }
}

/// Takes a `DealDamage` and queues the threat it makes on its target.
/// An attack made at a target with Patience overcommits its source
/// (`Status::overcommit`). Lands a strike from past its target's forward
/// faces, a flank, harder by its striker's Grace (`ActorAttributes::flank`),
/// and at Grace's capstone breaks its target's stride as it lands
/// (`QueuedThreat::stride`); weighs its blow and DoT by the level gap
/// (`damage::level_factor`), rolls the attack's damage within its range
/// (`Tuning::damage_spread`, one roll for its blow and its DoT), rolls its
/// blow's crit, a patient striker's skill likelier and harder on an
/// overcommitted foe and certain once the foe carries Patience's capstone
/// stacks, which that crit spends; and inserts it into the reaction queue,
/// its window starting as the strike is made: now, or its `delay` after
pub fn process_deal_damage(
    trigger: On<Try>,
    tuning: Res<Tuning>,
    mut commands: Commands,
    mut statuses: Query<&mut common_bevy::components::status::Status>,
    mut target_query: Query<(&mut ReactionQueue, &ActorAttributes, &Health, Option<&Endurance>, Option<&mut common_bevy::components::recovery::GlobalRecovery>)>,
    mut combat_query: Query<&mut CombatState>,
    all_attrs: Query<&ActorAttributes>,
    places: Query<(&Loc, Option<&common_bevy::components::heading::Heading>)>,
    surges: Query<&abilities::surge::Surged>,
    time: Res<Time>,
    runtime: Res<crate::RunTime>,
    dice: Res<dice::Dice>,
    mut rolls: Query<&mut dice::Rolls>,
    mut writer: MessageWriter<Do>,
) {
    let event = &trigger.event().event;

    if let GameEvent::DealDamage { source, target, base_damage, ability, dot, delay, stun, drag } = event {
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

        let flanked = places.get(*source).ok().zip(places.get(*target).ok())
            .is_some_and(|((from, _), (at, heading))| common_bevy::systems::targeting::takes_flank(heading, statuses.get(*target).ok(), at, from));
        // An attack gives a lurker away; a skill struck out of hiding is an
        // ambush, a certain crit
        let lurking = statuses.get(*source).is_ok_and(|status| status.is_lurking());
        if lurking {
            landing::update(*source, &mut statuses, &mut commands, &mut writer, |status| status.lurk = None);
        }
        // A Surge spends what its user's moves built on its next skill strike
        let surged = match statuses.get(*source).map(|status| status.surging()) {
            Ok(share) if share > 0.0 && ability.is_some_and(|ability| ability.kind() == common_bevy::message::SkillKind::Strike && ability != common_bevy::message::AbilityType::AutoAttack) => {
                let tiles = surges.get(*source).map_or(0, |surged| surged.tiles);
                landing::update(*source, &mut statuses, &mut commands, &mut writer, |status| status.surge = None);
                1.0 + share * tiles as f32
            }
            _ => 1.0,
        };
        let base_damage = &(base_damage * surged);
        let base_damage = if flanked { base_damage * (1.0 + source_attrs.flank(&tuning)) } else { *base_damage };
        let stride = if flanked { source_attrs.flank_stride(&tuning).unwrap_or(0.0) } else { 0.0 };
        let gap = damage_calc::level_factor(&tuning, source_attrs.total_level(), attrs.total_level());
        let base_damage = base_damage * gap;
        let draw = dice.draw(&mut rolls, ("spread", *source)).signed();
        let outgoing = damage_calc::spread(base_damage, tuning.damage_spread, draw);
        // A skill crits an overcommitted foe likelier and harder by its
        // striker's Patience, and for certain at its capstone's stacks
        let skill = *ability != Some(common_bevy::message::AbilityType::AutoAttack);
        let stacks = if skill { statuses.get(*target).map_or(0, |status| status.overcommits()) } else { 0 };
        let opening = stacks > 0 && source_attrs.patience_opening(&tuning).is_some_and(|at| stacks >= at);
        let ambush = lurking && skill;
        let patient = if opening || ambush { 1.0 } else { source_attrs.patience_crit(&tuning) * stacks as f32 };
        let power = if stacks > 0 { source_attrs.patience_power(&tuning) } else { 0.0 } + tuning.crit_severity(*ability);
        let roll = dice.draw(&mut rolls, ("crit", *source)).share();
        let critted = damage_calc::crits(&tuning, source_attrs, attrs, patient, roll);
        let outgoing = damage_calc::crit(&tuning, outgoing, source_attrs, attrs, patient, power, roll);
        if opening {
            landing::update(*target, &mut statuses, &mut commands, &mut writer, |status| status.spend_overcommits());
        }
        let dot = damage_calc::spread(*dot * gap, tuning.damage_spread, draw);

        let now = runtime.now(&time) + *delay;

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

        // INV-003: the threat's timer is the helper's to set
        let threat = queue_utils::create_threat(
            &tuning,
            *source,
            attrs,
            source_attrs,
            outgoing,
            *ability,
            now,
            dot,
            Endurance::fatigue_of(&tuning, endurance),
        ).breaking(stride).critical(critted).carrying(*stun, *drag);

        queue_utils::insert_threat(&mut queue, threat);

        // Both enter combat once the threat stands in the queue
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

        writer.write(Do {
            event: GameEvent::InsertThreat {
                ent: *target,
                threat,
            },
        });
    }
}

/// Lands a `ResolveThreat` on its target: its damage on the target's
/// health, and what it carries, a stride broken and a stun, on its status;
/// and a share of its damage on its source's health where a Bloodlust holds
/// it (`Status::lifesteal`).
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
        let Ok(mut health) = query.get_mut(*ent) else { return };
        let final_damage = threat.damage + threat.dot_left();
        land_damage(*ent, threat.source, final_damage, threat.is_wound(), &mut health, &mut writer);

        // A flank strike at Grace's capstone breaks its target's stride
        // for a swing as it lands
        if threat.stride > 0.0 {
            landing::update(*ent, &mut statuses, &mut commands, &mut writer, |status| status.break_stride(threat.stride, tuning.base_interval));
        }
        if threat.stun > 0.0 {
            landing::update(*ent, &mut statuses, &mut commands, &mut writer, |status| status.hold(threat.stun));
        }

        let lifesteal = statuses.get(threat.source).map_or(0.0, |status| status.lifesteal());
        if lifesteal > 0.0 && final_damage > 0.0 {
            let Ok(mut health) = query.get_mut(threat.source) else { return };
            if health.state > 0.0 {
                health.state = (health.state + final_damage * lifesteal).min(health.max);
                writer.write(Do { event: GameEvent::Incremental { ent: threat.source, component: common_bevy::message::Component::Health(*health) } });
            }
        }
    }
}

/// Moves the target of a `ResolveThreat` that carries a drag as it lands: a
/// walk over the ground up to its tiles, toward the threat's source and
/// stopping beside it, or away along the line from it, inside the
/// target's leash (`leap::Leash::of`), slid there as a Leap is. A braced
/// target stands.
pub fn resolve_drag(
    trigger: On<Try>,
    mut commands: Commands,
    mut writer: MessageWriter<Do>,
    map: Res<common_bevy::resources::map::Map>,
    actors: Query<(&Loc, &Health)>,
    statuses: Query<&common_bevy::components::status::Status>,
    leashed: Query<(&behaviour::chase::Chase, &engagement::EngagementMember)>,
    dens: Query<&Loc, With<engagement::Engagement>>,
) {
    let Try { event: GameEvent::ResolveThreat { ent, threat } } = trigger.event() else { return };
    if threat.drag == 0 || statuses.get(*ent).is_ok_and(|status| status.is_braced()) {
        return;
    }
    let (Ok((&at, health)), Ok((&to, _))) = (actors.get(*ent), actors.get(threat.source)) else { return };
    if health.state <= 0.0 {
        return;
    }
    let leash = leap::Leash::of(*ent, &leashed, &dens);
    let tiles = threat.drag.unsigned_abs() as usize;
    let landing = if threat.drag > 0 { leap::toward(&map, *at, *to, tiles, leash) } else { leap::away(&map, *at, *to, tiles, leash) };
    if let Some(landing) = landing {
        leap::slide(*ent, landing, leap::LEAP_MS, None, &mut commands, &mut writer);
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
/// swing waits.
pub fn track_engagement(
    mut query: Query<(Entity, &CombatState, &Loc, Option<&common_bevy::components::behaviour::Side>, &mut common_bevy::components::Swing)>,
    others: Query<(&common_bevy::components::behaviour::Side, &Health)>,
    nntree: Res<common_bevy::plugins::nntree::NNTree>,
    time: Res<Time>,
) {
    for (ent, state, &loc, side, mut swing) in &mut query {
        let hostile_near = side.is_some_and(|&side| {
            crate::behaviour::hostiles_near(&nntree, ent, loc, crate::behaviour::ACQUISITION_RANGE, side, |other| {
                others.get(other).ok().map(|(other_side, health)| (*other_side, health.state))
            }).next().is_some()
        });
        let engaged = state.in_combat || hostile_near;
        match (engaged, swing.due) {
            (false, Some(_)) => swing.due = None,
            (true, None) => swing.due = Some(Moment::ZERO + time.elapsed()),
            _ => {}
        }
    }
}

/// Physique's Intimidation at work, for an actor committed to it: every
/// living hostile within its zone, its reach and at the capstone further
/// (`ActorAttributes::intimidation_zone`), is slowed to its core's pace,
/// taxed by its facet's toll and at the capstone pinned, each lingering
/// `Tuning::intimidation_aura_secs` past the zone; of two zones the deeper
/// slow and the dearer toll hold. Each is renewed only once half spent, so
/// the clients are told of it a couple of times a second, not every tick.
pub fn intimidate(
    tuning: Res<Tuning>,
    mut commands: Commands,
    mut writer: MessageWriter<Do>,
    actors: Query<(Entity, &Loc, &ActorAttributes, &common_bevy::components::behaviour::Side, Option<&AttackRange>)>,
    others: Query<(&common_bevy::components::behaviour::Side, &Health)>,
    mut statuses: Query<&mut common_bevy::components::status::Status>,
    nntree: Res<common_bevy::plugins::nntree::NNTree>,
) {
    let linger = tuning.intimidation_aura_secs;
    let fresh = |timed: Option<common_bevy::components::status::Timed>| timed.is_some_and(|timed| timed.remaining > linger / 2.0);
    for (ent, &loc, attrs, &side, range) in &actors {
        let Some(pace) = attrs.intimidation_pace(&tuning) else { continue };
        let toll = attrs.intimidation_toll(&tuning);
        let zone = attrs.intimidation_zone(&tuning);
        let reach = (range.copied().unwrap_or_default().0 + zone.unwrap_or(0)).max(0) as u32;
        let foes: Vec<Entity> = crate::behaviour::hostiles_near(&nntree, ent, loc, reach, side, |other| {
            others.get(other).ok().map(|(other_side, health)| (*other_side, health.state))
        }).collect();
        for other in foes {
            let held = statuses.get(other).ok().is_some_and(|status| {
                // A braced foe takes no slow or pin to keep fresh
                if status.is_braced() {
                    return toll <= 0.0 || status.toll.is_some_and(|held| held.pace >= 1.0 + toll) && fresh(status.toll);
                }
                let slowed = status.slow.is_some_and(|slow| slow.pace <= pace) && fresh(status.slow);
                let taxed = toll <= 0.0 || (status.toll.is_some_and(|held| held.pace >= 1.0 + toll) && fresh(status.toll));
                let pinned = zone.is_none() || fresh(status.pinned);
                slowed && taxed && pinned
            });
            if !held {
                landing::update(other, &mut statuses, &mut commands, &mut writer, |status| {
                    status.slow(pace, linger);
                    if toll > 0.0 {
                        status.tax(1.0 + toll, linger);
                    }
                    if zone.is_some() {
                        status.pin(linger);
                    }
                });
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
    use common_bevy::components::Swing;
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
        let calm = CombatState { in_combat: false, last_action: Moment::ZERO };
        let waiting = app.world_mut().spawn((calm, at(0), Side::WILD, Health::full(100.0), Swing::default())).id();
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
    fn a_fight_starts_the_swing_clock_and_its_end_clears_it() {
        let secs = Duration::from_secs;
        let at = |secs: u64| Moment::from_millis(secs * 1_000);
        let mut app = App::new();
        app.add_plugins(common_bevy::plugins::nntree::NNTreePlugin);
        app.add_message::<Do>();
        let world = app.world_mut();
        let mut time = Time::<()>::default();
        time.advance_by(secs(10));
        world.insert_resource(time);
        let here = Loc::new(qrz::Qrz { q: 0, r: 0, z: 0 });
        let fighter = world.spawn((CombatState { in_combat: true, last_action: Moment::ZERO }, here, Swing::default())).id();

        engaging(world);
        let swing = *world.get::<Swing>(fighter).unwrap();
        assert_eq!(swing.waited(at(10)), Some(Duration::ZERO), "due as the fight finds it");
        assert_eq!(swing.waited(at(15)), Some(secs(5)), "and waiting from then");
        assert_eq!(Swing { due: Some(at(20)) }.waited(at(15)), None, "one still to come due waits for nothing");

        world.get_mut::<CombatState>(fighter).unwrap().in_combat = false;
        engaging(world);
        assert_eq!(world.get::<Swing>(fighter).unwrap().waited(at(60)), Some(Duration::ZERO), "disengaged, it is due and has waited no time");
    }

    #[test]
    fn intimidation_slows_its_zone_then_taxes_it_then_pins_it_further_out() {
        use common_bevy::{components::{behaviour::Side, status::Status, AttackRange, Loc}, plugins::nntree::{NNTreePlugin, NearestNeighbor}};
        let zone_of = |steps: i8, foe_q: i32| {
            let mut app = App::new();
            app.add_plugins(NNTreePlugin);
            app.add_message::<Do>();
            app.init_resource::<Tuning>();
            let at = |q: i32| Loc::new(qrz::Qrz { q, r: 0, z: 0 });
            let attrs = ActorAttributes::new(0, 0, 0, -steps, 0, 0, 0, 0, 0);
            let imposing = app.world_mut().spawn((at(0), attrs, Side::PLAYERS, AttackRange::default(), Health::full(100.0))).id();
            let foe = app.world_mut().spawn((at(foe_q), Side::WILD, Health::full(100.0))).id();
            for (ent, q) in [(imposing, 0), (foe, foe_q)] {
                app.world_mut().entity_mut(ent).insert(NearestNeighbor::new(ent, at(q)));
            }
            app.update();
            app.world_mut().run_system_once(intimidate).unwrap();
            app.world().get::<Status>(foe).copied().unwrap_or_default()
        };
        let reach = AttackRange::default().0;
        assert!(!zone_of(0, 1).is_slowed(), "no Intimidation, no zone");
        let core = zone_of(3, 1);
        assert!(core.is_slowed() && core.price() == 1.0 && !core.is_pinned(), "the core slows");
        let facet = zone_of(9, 1);
        assert!(facet.price() > 1.0 && !facet.is_pinned(), "the facet taxes");
        assert!(zone_of(18, 1).is_pinned(), "the capstone pins");
        assert!(!zone_of(18, reach + 1).is_slowed(), "its zone is its reach at T6");
        assert!(zone_of(24, reach + 2).is_pinned(), "and reaches further by its depth");
        assert!(zone_of(3, 1).slow.unwrap().pace > zone_of(6, 1).slow.unwrap().pace, "the core's depth slows deeper");
    }
}
