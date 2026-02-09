pub mod abilities;

use bevy::prelude::*;
use crate::common::{
    components::{entity_type::*, reaction_queue::*, resources::*, gcd::Gcd, LastAutoAttack, *},
    message::{AbilityFailReason, AbilityType, Do, Try, Event as GameEvent},
    systems::{
        combat::{damage as damage_calc, queue as queue_utils},
    },
};

/// System to process DealDamage events (Phase 1: Outgoing damage calculation)
/// Rolls for crit, calculates outgoing damage, inserts into reaction queue
pub fn process_deal_damage(
    trigger: On<Try>,
    mut commands: Commands,
    mut target_query: Query<(&mut ReactionQueue, &ActorAttributes, &Health)>,
    mut combat_query: Query<&mut CombatState>,
    all_attrs: Query<&ActorAttributes>,
    time: Res<Time>,
    runtime: Res<crate::server::resources::RunTime>,
    mut writer: MessageWriter<Do>,
) {
    let event = &trigger.event().event;

    if let GameEvent::DealDamage { source, target, base_damage, damage_type, ability } = event {
        // Get attacker attributes for scaling
        let Ok(source_attrs) = all_attrs.get(*source) else {
            return;
        };

        // Roll for critical hit
        let (_was_crit, crit_mult) = damage_calc::roll_critical(source_attrs);

        // Calculate outgoing damage (Phase 1)
        let outgoing = damage_calc::calculate_outgoing_damage(*base_damage, source_attrs, *damage_type);
        let outgoing_with_crit = outgoing * crit_mult;

        // Get target's queue, attributes, and health
        let Ok((mut queue, attrs, health)) = target_query.get_mut(*target) else {
            return;
        };

        // Don't queue threats on dead targets
        if health.state <= 0.0 {
            return;
        }

        // Insert threat into queue
        // Use game world time (server uptime + offset) for consistent time base
        let now_ms = time.elapsed().as_millis() + runtime.elapsed_offset;
        let now = std::time::Duration::from_millis(now_ms.min(u64::MAX as u128) as u64);
        let timer_duration = queue_utils::calculate_timer_duration(attrs);

        let threat = QueuedThreat {
            source: *source,
            damage: outgoing_with_crit,
            damage_type: *damage_type,
            inserted_at: now,
            timer_duration,
            ability: *ability,
        };

        // Try to insert threat into queue
        let overflow = queue_utils::insert_threat(&mut queue, threat, now);

        // Enter combat for both attacker and target AFTER threat is successfully inserted
        // Handle case where source == target (self-damage)
        if source == target {
            if let Ok(mut combat_state) = combat_query.get_mut(*source) {
                crate::common::systems::combat::state::enter_combat(*source, &mut combat_state, &time, &mut writer);
            }
        } else {
            // Enter combat for attacker (put threat in queue)
            if let Ok(mut attacker_combat) = combat_query.get_mut(*source) {
                crate::common::systems::combat::state::enter_combat(*source, &mut attacker_combat, &time, &mut writer);
            }
            // Enter combat for target (received threat in queue)
            if let Ok(mut target_combat) = combat_query.get_mut(*target) {
                crate::common::systems::combat::state::enter_combat(*target, &mut target_combat, &time, &mut writer);
            }
        }

        // Send InsertThreat event to clients
        writer.write(Do {
            event: GameEvent::InsertThreat {
                ent: *target,
                threat,
            },
        });

        // If queue overflowed, immediately resolve the overflow threat
        if let Some(overflow_threat) = overflow {
            // Emit ResolveThreat event for the overflow
            commands.trigger(
                Try {
                    event: GameEvent::ResolveThreat {
                        ent: *target,
                        threat: overflow_threat,
                    },
                },
            );
        }
    }
}

/// System to resolve threats (Phase 2: Apply passive modifiers and apply to health)
/// Processes ResolveThreat events emitted by expiry system or overflow
pub fn resolve_threat(
    trigger: On<Try>,
    _commands: Commands,
    mut query: Query<(&mut Health, &ActorAttributes)>,
    mut writer: MessageWriter<Do>,
) {
    let event = &trigger.event().event;

    if let GameEvent::ResolveThreat { ent, threat } = event {
        if let Ok((mut health, attrs)) = query.get_mut(*ent) {
            // Apply passive modifiers (Phase 2)
            let final_damage = damage_calc::apply_passive_modifiers(
                threat.damage,
                attrs,
                threat.damage_type,
            );

            // Apply damage to health
            health.state = (health.state - final_damage).max(0.0);
            health.step = health.state; // Snap step to state for immediate feedback

            // Broadcast damage event to clients
            writer.write(Do {
                event: GameEvent::ApplyDamage {
                    ent: *ent,
                    damage: final_damage,
                    source: threat.source,
                },
            });

            // Send authoritative health value to sync clients
            writer.write(Do {
                event: GameEvent::Incremental {
                    ent: *ent,
                    component: crate::common::message::Component::Health(*health),
                },
            });

            // Death check moved to dedicated check_death system (decoupled from combat)
        }
    }
}

/// System to validate ability prerequisites (GCD, death status)
/// Runs before individual ability systems
/// Emits AbilityFailed for invalid attempts
pub fn validate_ability_prerequisites(
    mut reader: MessageReader<Try>,
    caster_respawn_query: Query<&RespawnTimer>,
    gcd_query: Query<&Gcd>,
    time: Res<Time>,
    mut writer: MessageWriter<Do>,
) {
    for event in reader.read() {
        if let GameEvent::UseAbility { ent, ability: _, target_loc: _ } = event.event {
            // Ignore abilities from dead players (those with RespawnTimer)
            if caster_respawn_query.get(ent).is_ok() {
                writer.write(Do {
                    event: GameEvent::AbilityFailed {
                        ent,
                        reason: AbilityFailReason::NoTargets, // Dead players can't use abilities
                    },
                });
                continue;
            }

            // Check GCD - if active, reject ability
            if let Ok(gcd) = gcd_query.get(ent) {
                if gcd.is_active(time.elapsed()) {
                    writer.write(Do {
                        event: GameEvent::AbilityFailed {
                            ent,
                            reason: AbilityFailReason::OnCooldown,
                        },
                    });
                    continue;
                }
            }
        }
    }
}

// Individual ability handlers are in combat/abilities/ module:
// - abilities::auto_attack::handle_auto_attack
// - abilities::overpower::handle_overpower
// - abilities::lunge::handle_lunge
// - abilities::knockback::handle_knockback
// - abilities::counter::handle_counter (ADR-014)
// - abilities::deflect::handle_deflect
// - abilities::volley::handle_volley
// GCD and tier lock are now reset directly by ability systems to prevent race conditions

/// CRITICAL: until we add a new system ... we need this one to bypass a random magic 
/// number of systems causing scheduling issues
pub fn do_nothing(){}

/// System to automatically trigger auto-attacks when adjacent to hostiles (ADR-009)
/// Runs periodically to check if actors have adjacent hostiles and can auto-attack
/// Auto-attack cooldown: 1.5s (1500ms)
pub fn process_passive_auto_attack(
    mut query: Query<
        (Entity, &Loc, &mut LastAutoAttack, Option<&Gcd>, &crate::common::components::target::Target),
        Without<crate::common::components::behaviour::PlayerControlled>
    >,
    entity_query: Query<(&EntityType, &Loc, Option<&RespawnTimer>)>,
    time: Res<Time>,
    runtime: Res<crate::server::resources::RunTime>,
    mut writer: MessageWriter<Try>,
) {
    // Use game world time (server uptime + offset) for consistent time base
    let now_ms = time.elapsed().as_millis() + runtime.elapsed_offset;
    let now = std::time::Duration::from_millis(now_ms.min(u64::MAX as u128) as u64);

    const AUTO_ATTACK_COOLDOWN_MS: u64 = 1500; // 1.5 seconds

    // Only iterate over NPCs (entities Without PlayerControlled)
    for (ent, loc, mut last_auto_attack, gcd_opt, target) in query.iter_mut() {
        // Check if on GCD
        if let Some(gcd) = gcd_opt {
            if gcd.is_active(time.elapsed()) {
                continue; // Skip if on GCD
            }
        }

        // Check cooldown (1.5s between auto-attacks)
        let time_since_last_attack = now.saturating_sub(last_auto_attack.last_attack_time);
        if time_since_last_attack.as_millis() < AUTO_ATTACK_COOLDOWN_MS as u128 {
            continue; // Still on cooldown
        }

        // ADR-009: Check if NPC's target (from unified Target component) is adjacent
        // Unwrap Target Option<Entity>
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

        // Check if target is adjacent (distance == 1)
        let distance = loc.flat_distance(target_loc);
        if distance == 1 {
            // Target is adjacent - trigger auto-attack
            writer.write(Try {
                event: GameEvent::UseAbility {
                    ent,
                    ability: AbilityType::AutoAttack,
                    target_loc: Some(**target_loc),
                },
            });

            // Update last attack time
            last_auto_attack.last_attack_time = now;
        }
    }
}

// Tests removed - need proper integration testing setup with Bevy's event system
// The core logic is tested through the common/systems/reaction_queue tests
