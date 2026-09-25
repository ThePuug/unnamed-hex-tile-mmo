/// # NPC Ability Usage System

/// NPCs with Chase or Kite behaviors will use their signature abilities
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
use crate::systems::behaviour::{chase::Chase, kite::Kite};

/// System to trigger NPC signature abilities
/// Runs periodically to check if NPCs should use their archetype abilities

/// Ability usage rules:
/// - Berserker (Lunge): Use when target is 2-4 hexes away (gap closer)
/// - Juggernaut (Overpower): Use when adjacent to target (heavy strike)
/// - Kiter: No signature ability — relies on kite behavior + ranged auto-attack
/// - Defender (Counter): Reactive - triggers when threats appear in reaction queue
///
/// Every use waits out the NPC's `NpcRecovery` delay, armed once the ability
/// is affordable and out of lockout, so NPCs that fire together drift apart.

/// Update frequency: 0.5s (fast enough for Defenders to respond to incoming threats)
pub fn npc_ability_usage(
    // Query NPCs with Chase or Kite behavior
    mut npc_query: Query<
        (Entity, &EntityType, &Loc, &Target, &Stamina, Option<&GlobalRecovery>, Option<&common_bevy::components::reaction_queue::ReactionQueue>, &mut NpcRecovery),
        Or<(With<Chase>, With<Kite>)>
    >,
    target_query: Query<&Loc, With<common_bevy::components::behaviour::PlayerControlled>>,
    time: Res<Time>,
    mut writer: MessageWriter<Try>,
) {
    let now = time.elapsed();
    for (npc_entity, entity_type, npc_loc, target, stamina, recovery_opt, queue_opt, mut delay) in npc_query.iter_mut() {
        // Skip if in recovery (ability lockout)
        if let Some(recovery) = recovery_opt {
            if recovery.is_active() {
                continue;
            }
        }

        // Get archetype from NPC type
        let EntityType::Actor(actor_impl) = entity_type else {
            continue;
        };

        let archetype = match actor_impl.identity {
            ActorIdentity::Npc(npc_type) => {
                use common_bevy::components::entity_type::actor::NpcType;
                match npc_type {
                    NpcType::WildDog => EnemyArchetype::Berserker,
                    NpcType::Juggernaut => EnemyArchetype::Juggernaut,
                    NpcType::ForestSprite => EnemyArchetype::Kiter,
                    NpcType::Defender => EnemyArchetype::Defender,
                }
            }
            _ => continue, // Not an NPC
        };

        // Get signature ability for this archetype (None = auto-attack only, skip)
        let Some(ability) = archetype.ability() else {
            continue;
        };

        let stamina_cost = match ability {
            AbilityType::Lunge => 20.0,
            AbilityType::Overpower => 40.0,
            AbilityType::Counter => 30.0,
            _ => continue,
        };
        if stamina.state < stamina_cost {
            continue;
        }
        delay.arm(now);
        if !delay.is_ready(now) {
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

        // Calculate distance to target
        let distance = npc_loc.flat_distance(target_loc);

        // Decide whether to use ability based on archetype and distance
        let should_use_ability = match archetype {
            // Lunge: gap closer, not in melee range and not out of range
            EnemyArchetype::Berserker => (2..=4).contains(&distance),
            // Overpower: heavy strike when adjacent
            EnemyArchetype::Juggernaut => distance == 1,
            // Kiter has no signature ability; Defender's Counter is handled above
            EnemyArchetype::Kiter | EnemyArchetype::Defender => false,
        };

        if should_use_ability {
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
