pub mod abilities;
pub mod landing;
pub mod leap;

use bevy::prelude::*;
use common_bevy::{
    components::{entity_type::*, reaction_queue::*, resources::*, LastAutoAttack, *},
    message::{AbilityType, Do, Try, Event as GameEvent},
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
    mut target_query: Query<(&mut ReactionQueue, &ActorAttributes, &Health, Option<&mut common_bevy::components::recovery::GlobalRecovery>)>,
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
        let Ok((mut queue, attrs, health, recovery_opt)) = target_query.get_mut(*target) else {
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

        // Recovery pushback: Impact vs Composure with gap factor
        if let Some(mut recovery) = recovery_opt {
            let pushback_pct = damage_calc::calculate_recovery_pushback(
                source_attrs.impact(),
                attrs.composure(),
                damage_calc::level_edge(source_attrs.total_level(), attrs.total_level()),
            );
            recovery.apply_pushback(pushback_pct);
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
    time: Res<Time>,
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
            // What would pass the defender's Grit waits for the seconds after;
            // a wound's DoT is never held back
            let blow = mitigated + (threat.damage - mitigated) * pierce;
            let cap = attrs.grit_cap() * health.max;
            let blow = grit.map_or(blow, |mut grit| grit.take(time.elapsed(), blow, cap, threat.source));
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

/// Forgets the swing of every actor out of combat, so its next fight's
/// first swing banks nothing.
pub fn forget_swings(mut query: Query<(&CombatState, &mut common_bevy::components::Swing)>) {
    for (state, mut swing) in &mut query {
        if !state.in_combat && swing.at.is_some() {
            swing.at = None;
        }
    }
}

/// A blow's spill lands on another hostile near its striker, outside the
/// queue: its share was already weighed against that hostile's Toughness.
pub fn resolve_spill(
    trigger: On<Try>,
    mut query: Query<(&mut Health, &ActorAttributes, Option<&mut common_bevy::components::grit::Grit>)>,
    time: Res<Time>,
    mut writer: MessageWriter<Do>,
) {
    let Try { event: GameEvent::Spill { ent, source, damage } } = trigger.event() else { return };
    let Ok((mut health, attrs, grit)) = query.get_mut(*ent) else { return };
    if health.state <= 0.0 {
        return;
    }
    let cap = attrs.grit_cap() * health.max;
    let damage = grit.map_or(*damage, |mut grit| grit.take(time.elapsed(), *damage, cap, *source));
    land_damage(*ent, *source, damage, false, &mut health, &mut writer);
}

/// Lands damage Grit held back as its windows clear: the whole of it in
/// time, only no faster than its cap allows.
pub fn release_grit(
    mut query: Query<(Entity, &mut common_bevy::components::grit::Grit, &mut Health, &ActorAttributes)>,
    time: Res<Time>,
    mut writer: MessageWriter<Do>,
) {
    for (ent, mut grit, mut health, attrs) in &mut query {
        if grit.deferred <= 0.0 || health.state <= 0.0 {
            continue;
        }
        let damage = grit.release(time.elapsed(), attrs.grit_cap() * health.max);
        if damage > 0.0 {
            let source = grit.source.unwrap_or(ent);
            land_damage(ent, source, damage, false, &mut health, &mut writer);
        }
    }
}

/// Takes `damage` from `health` and tells every client: the one place
/// damage lands, whatever dealt it. `dot` marks a wound's, shown apart.
fn land_damage(ent: Entity, source: Entity, damage: f32, dot: bool, health: &mut Health, writer: &mut MessageWriter<Do>) {
    health.state = (health.state - damage).max(0.0);
    health.step = health.state;
    writer.write(Do { event: GameEvent::ApplyDamage { ent, damage, source, dot } });
    writer.write(Do { event: GameEvent::Incremental { ent, component: common_bevy::message::Component::Health(*health) } });
}


/// System to automatically trigger auto-attacks when a hostile is in range.
/// The cadence is fixed by `ActorAttributes::cadence_interval`, stretched
/// by a daze: no random spread, and an ability's lockout does not pause it. Every actor's
/// swings are timed here, a player's as an NPC's, so none is timed by a client. An NPC swings
/// wherever it stands; its assigned hex decides only where it walks.
pub fn process_passive_auto_attack(
    mut query: Query<
        (Entity, &Loc, &mut LastAutoAttack, &common_bevy::components::target::Target,
         &ActorAttributes,
         Option<&common_bevy::components::AttackRange>,
         Option<&common_bevy::components::heading::Heading>,
         Option<&common_bevy::components::status::Status>),
    >,
    entity_query: Query<(&EntityType, &Loc, Option<&RespawnTimer>)>,
    time: Res<Time>,
    runtime: Res<crate::resources::RunTime>,
    mut writer: MessageWriter<Try>,
) {
    // Use game world time (server uptime + offset) for consistent time base
    let now_ms = time.elapsed().as_millis() + runtime.elapsed_offset;
    let now = std::time::Duration::from_millis(now_ms.min(u64::MAX as u128) as u64);

    for (ent, loc, mut last_auto_attack, target, attrs, attack_range_opt, heading, status) in query.iter_mut() {
        if common_bevy::components::status::Status::holds(status) {
            continue;
        }
        // Check cooldown: the fixed interval, stretched by a daze
        let cooldown = common_bevy::components::status::Status::cadence(attrs.cadence_interval(), status);
        let time_since_last_attack = now.saturating_sub(last_auto_attack.last_attack_time);
        if time_since_last_attack < cooldown {
            continue; // Still on cooldown
        }

        // The hostile it targets: an NPC's from its chase, a player's from its facing
        let Some(target_ent) = target.entity else {
            continue; // No target set
        };

        // Get target's location
        let Ok((_, target_loc, respawn_timer_opt)) = entity_query.get(target_ent) else {
            continue; // Target entity doesn't exist or missing components
        };

        // Skip dead targets
        if respawn_timer_opt.is_some() {
            continue;
        }

        // Check if target is within auto-attack range (manhattan: flat hex distance + z difference)
        let distance = loc.distance(target_loc);
        let max_range = attack_range_opt.copied().unwrap_or_default().0;
        if distance <= max_range && common_bevy::systems::targeting::faces(heading, attrs.arc(), loc, target_loc) {
            // Target is in range and within its arc - trigger auto-attack
            writer.write(Try {
                event: GameEvent::UseAbility {
                    ent,
                    ability: AbilityType::AutoAttack,
                    target: Some(target_ent),
                },
            });

            // Update last attack time
            last_auto_attack.last_attack_time = now;
        }
    }
}

// Tests removed - need proper integration testing setup with Bevy's event system
// The core logic is tested through the common/systems/reaction_queue tests
