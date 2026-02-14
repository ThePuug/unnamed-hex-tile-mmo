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
    mut target_query: Query<(&mut ReactionQueue, &ActorAttributes, &Health, Option<&mut crate::common::components::recovery::GlobalRecovery>)>,
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

        // Get target's queue, attributes, health, and recovery
        let Ok((mut queue, attrs, health, recovery_opt)) = target_query.get_mut(*target) else {
            return;
        };

        // Don't queue threats on dead targets
        if health.state <= 0.0 {
            return;
        }

        // Evasion check (Grace commitment tier)
        let dodge = attrs.evasion_chance();
        if dodge > 0.0 && rand::Rng::random_range(&mut rand::rng(), 0.0..1.0) < dodge {
            return; // Threat evaded — no queue insertion, no combat entry
        }

        // Calculate outgoing damage (Phase 1)
        let outgoing = damage_calc::calculate_outgoing_damage(*base_damage, source_attrs, *damage_type);

        // Use game world time (server uptime + offset) for consistent time base
        let now_ms = time.elapsed().as_millis() + runtime.elapsed_offset;
        let now = std::time::Duration::from_millis(now_ms.min(u64::MAX as u128) as u64);

        // Recovery pushback: Impact vs Composure with gap factor
        if let Some(mut recovery) = recovery_opt {
            let pushback_pct = damage_calc::calculate_recovery_pushback(
                source_attrs.impact(),
                attrs.composure(),
                source_attrs.total_level(),
                attrs.total_level(),
            );
            recovery.apply_pushback(pushback_pct);
        }

        // Create threat using canonical helper (INV-003: ensures consistent timers)
        let threat = queue_utils::create_threat(
            *source,       // Source entity
            attrs,         // Target attributes
            source_attrs,  // Source attributes
            outgoing,      // Damage
            *damage_type,  // Damage type
            *ability,      // Ability
            now,           // Current time
        );

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

        // ADR-030: Queue is unbounded, no overflow handling needed
    }
}

/// System to resolve threats (Phase 2: Apply passive modifiers and apply to health)
/// Processes ResolveThreat events emitted by expiry system or overflow
pub fn resolve_threat(
    trigger: On<Try>,
    _commands: Commands,
    mut query: Query<(&mut Health, &ActorAttributes)>,
    loc_query: Query<&Loc>,
    all_attrs: Query<&ActorAttributes>,
    mut writer: MessageWriter<Do>,
) {
    let event = &trigger.event().event;

    if let GameEvent::ResolveThreat { ent, threat } = event {
        if let Ok((mut health, attrs)) = query.get_mut(*ent) {
            // Scan for strongest Dominance aura affecting this entity
            let (max_dominance, dominant_level) = damage_calc::find_max_dominance_in_range(*ent, &loc_query, &all_attrs);

            // Apply passive mitigation (unified for all damage types)
            let final_damage = damage_calc::apply_passive_modifiers(
                threat.damage,
                attrs,
                max_dominance,
                dominant_level,
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
/// Auto-attack cooldown: tier-based (750ms-2000ms based on Presence commitment)
pub fn process_passive_auto_attack(
    mut query: Query<
        (Entity, &Loc, &mut LastAutoAttack, Option<&Gcd>, &crate::common::components::target::Target,
         Option<&mut crate::common::components::npc_recovery::NpcRecovery>,
         Option<&crate::common::components::hex_assignment::AssignedHex>,
         Option<&crate::common::components::recovery::GlobalRecovery>,
         &ActorAttributes),
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

    // Only iterate over NPCs (entities Without PlayerControlled)
    for (ent, loc, mut last_auto_attack, gcd_opt, target, npc_recovery_opt, assigned_hex_opt, global_recovery_opt, attrs) in query.iter_mut() {
        // Check if on GCD
        if let Some(gcd) = gcd_opt {
            if gcd.is_active(time.elapsed()) {
                continue; // Skip if on GCD
            }
        }

        // Check if in ability recovery lockout (from using Lunge, Overpower, etc.)
        if let Some(recovery) = global_recovery_opt {
            if recovery.is_active() {
                continue; // Skip if still locked out from ability usage
            }
        }

        // SOW-018: Check NPC recovery timer (per-archetype cooldown between attacks)
        if let Some(ref recovery) = npc_recovery_opt {
            if recovery.is_recovering(now) {
                continue; // Still in recovery phase
            }
        }

        // SOW-018: Check NPC is on assigned hex (if it has one)
        if let Some(assigned) = assigned_hex_opt {
            if loc.flat_distance(&crate::common::components::Loc::new(assigned.0)) != 0 {
                continue; // Not on assigned hex yet
            }
        }

        // Check cooldown (tier-based cadence from Presence commitment)
        let cooldown = attrs.cadence_interval();
        let time_since_last_attack = now.saturating_sub(last_auto_attack.last_attack_time);
        if time_since_last_attack < cooldown {
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

        // Check if target is within range (same hex or adjacent: distance <= 1)
        let distance = loc.flat_distance(target_loc);
        if distance <= 1 {
            // Target is in range - trigger auto-attack
            writer.write(Try {
                event: GameEvent::UseAbility {
                    ent,
                    ability: AbilityType::AutoAttack,
                    target_loc: Some(**target_loc),
                },
            });

            // Update last attack time
            last_auto_attack.last_attack_time = now;

            // SOW-018: Start NPC recovery timer after attacking
            if let Some(mut recovery) = npc_recovery_opt {
                recovery.start_recovery(now);
            }
        }
    }
}

// Tests removed - need proper integration testing setup with Bevy's event system
// The core logic is tested through the common/systems/reaction_queue tests
