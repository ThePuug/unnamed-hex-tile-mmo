/// # NPC Ability Usage System

/// NPCs will use their signature abilities
/// based on archetype when appropriate conditions are met.

use bevy::prelude::*;
use common_bevy::{
    components::{
        entity_type::{EntityType, actor::ActorIdentity},
        npc_recovery::NpcRecovery,
        resources::*, Loc, target::Target,
        recovery::GlobalRecovery,
    },
    message::{Event, Try, AbilityType},
    spatial_difficulty::EnemyArchetype,
};
use crate::systems::behaviour::chase::Chase;

/// System to trigger NPC signature abilities
/// Runs periodically to check if NPCs should use their archetype abilities

/// Ability usage rules:
/// - A strike (Lunge, Rattle, Volley, Flank): when its target stands within
///   the ability's own reach (`AbilityType::reach`) and its arc
/// - Defender (Counter): reactive - when threats stand in its reaction queue
/// - Skirmisher (Disengage): reactive - dodges the blow at the front of its queue, an auto-attack's as overflow
///
/// Every use waits out the NPC's `NpcRecovery` delay, armed once the ability
/// is affordable and out of lockout, or a reaction its Preparation lets
/// through the lockout, so NPCs that fire together drift apart.

/// Update frequency: 0.5s (fast enough for Defenders to respond to incoming threats)
pub fn npc_ability_usage(
    mut npc_query: Query<
        (Entity, &EntityType, &Loc, &Target, &Stamina, Option<&GlobalRecovery>, Option<&common_bevy::components::reaction_queue::ReactionQueue>, &mut NpcRecovery, Option<&common_bevy::components::heading::Heading>, &common_bevy::components::ActorAttributes, Option<&common_bevy::components::AttackRange>),
        With<Chase>
    >,
    target_query: Query<&Loc, With<common_bevy::components::behaviour::Side>>,
    time: Res<Time>,
    mut writer: MessageWriter<Try>,
) {
    let now = time.elapsed();
    for (npc_entity, entity_type, npc_loc, target, stamina, recovery_opt, queue_opt, mut delay, heading, attrs, own_reach) in npc_query.iter_mut() {
        // Get archetype from NPC type
        let EntityType::Actor(actor_impl) = entity_type else {
            continue;
        };

        let ActorIdentity::Npc(npc_type) = actor_impl.identity else {
            continue; // Not an NPC
        };
        let archetype = EnemyArchetype::of_npc(npc_type);

        // Get signature ability for this archetype (None = auto-attack only, skip)
        let Some(ability) = archetype.ability() else {
            continue;
        };

        // Out of lockout, or a reaction its Preparation lets through it, and affordable
        let locked = recovery_opt.is_some_and(|recovery| recovery.is_active());
        if (locked && !common_bevy::systems::combat::synergies::reacts_through(ability, recovery_opt, Some(attrs)))
            || stamina.state < common_bevy::tuning::tuning().cost(ability)
        {
            continue;
        }
        delay.arm(now);
        if !delay.is_ready(now) {
            continue;
        }

        // Skirmisher Disengages from the blow at the front of its queue: an ability's
        // while one is queued, an auto-attack as overflow
        if ability == AbilityType::Disengage {
            if let Some(blow) = queue_opt.and_then(|queue| queue.threats.front().copied()) {
                writer.write(Try {
                    event: Event::UseAbility { ent: npc_entity, ability: AbilityType::Disengage, target: Some(blow.source) },
                });
                delay.spend();
            }
            continue;
        }

        // Defender uses Counter when threats are in its reaction queue
        if ability == AbilityType::Counter {
            if queue_opt.is_some_and(|queue| !queue.threats.is_empty()) {
                writer.write(Try {
                    event: Event::UseAbility {
                        ent: npc_entity,
                        ability: AbilityType::Counter,
                        target: None,
                    },
                });
                delay.spend();
            }
            continue;
        }

        // Check if we have a valid target
        let Some(target_entity) = target.entity else {
            continue;
        };

        let Ok(target_loc) = target_query.get(target_entity) else {
            continue;
        };

        // Within the ability's own reach of its target
        let distance = npc_loc.flat_distance(target_loc);
        let should_use_ability = ability.reach(own_reach.copied().unwrap_or_default().0)
            .is_some_and(|reach| reach.contains(&distance));

        if should_use_ability && common_bevy::systems::targeting::faces(heading, attrs.arc(), npc_loc, target_loc) {
            // Send target entity from NPC's Target component
            writer.write(Try {
                event: Event::UseAbility {
                    ent: npc_entity,
                    ability,
                    target: target.entity,
                },
            });
            delay.spend();
        }
    }
}
