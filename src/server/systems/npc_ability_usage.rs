/// # NPC Ability Usage System (ADR-014 Phase 3B)
///
/// NPCs with Chase or Kite behaviors will use their signature abilities
/// based on archetype when appropriate conditions are met.

use bevy::prelude::*;
use crate::{
    common::{
        components::{
            entity_type::{EntityType, actor::ActorIdentity},
            resources::*, Loc, target::Target,
            recovery::GlobalRecovery,
        },
        message::{Event, Try, AbilityType},
        spatial_difficulty::EnemyArchetype,
    },
    server::systems::behaviour::{chase::Chase, kite::Kite},
};

/// System to trigger NPC signature abilities (ADR-014 Phase 3B)
/// Runs periodically to check if NPCs should use their archetype abilities
///
/// Ability usage rules:
/// - Berserker (Lunge): Use when target is 2-4 hexes away (gap closer)
/// - Juggernaut (Overpower): Use when adjacent to target (heavy strike)
/// - Kiter (Volley): Use when at optimal distance 5-8 hexes (ranged attack)
/// - Defender (Counter): Reactive - triggers when threats appear in reaction queue
///
/// Update frequency: 0.5s (fast enough for Defenders to respond to incoming threats)
pub fn npc_ability_usage(
    // Query NPCs with Chase or Kite behavior
    mut npc_query: Query<
        (Entity, &EntityType, &Loc, &Target, &Stamina, Option<&GlobalRecovery>, Option<&crate::common::components::reaction_queue::ReactionQueue>),
        Or<(With<Chase>, With<Kite>)>
    >,
    target_query: Query<&Loc, With<crate::common::components::behaviour::PlayerControlled>>,
    _time: Res<Time>,
    mut writer: MessageWriter<Try>,
) {
    for (npc_entity, entity_type, npc_loc, target, stamina, recovery_opt, queue_opt) in npc_query.iter_mut() {
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
                use crate::common::components::entity_type::actor::NpcType;
                match npc_type {
                    NpcType::WildDog => EnemyArchetype::Berserker,
                    NpcType::Juggernaut => EnemyArchetype::Juggernaut,
                    NpcType::ForestSprite => EnemyArchetype::Kiter,
                    NpcType::Defender => EnemyArchetype::Defender,
                }
            }
            _ => continue, // Not an NPC
        };

        // Get signature ability for this archetype
        let ability = archetype.ability();

        // Handle Defender Counter specially - reactive ability triggered by threats in queue
        if ability == AbilityType::Counter {
            // Defender uses Counter when threats are in reaction queue
            if let Some(queue) = queue_opt {
                if !queue.threats.is_empty() {
                    // Has threats to counter - check stamina
                    let counter_stamina_cost = 30.0;
                    if stamina.state >= counter_stamina_cost {
                        writer.write(Try {
                            event: Event::UseAbility {
                                ent: npc_entity,
                                ability: AbilityType::Counter,
                                target_loc: None,
                            },
                        });
                    }
                }
            }
            continue; // Skip rest of logic for Defenders
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
            EnemyArchetype::Berserker => {
                // Lunge: Gap closer when target is 2-4 hexes away
                // Don't use if too close (melee range) or too far (out of range)
                let lunge_stamina_cost = 20.0;
                distance >= 2 && distance <= 4 && stamina.state >= lunge_stamina_cost
            }
            EnemyArchetype::Juggernaut => {
                // Overpower: Heavy strike when adjacent (1 hex)
                let overpower_stamina_cost = 40.0;
                distance == 1 && stamina.state >= overpower_stamina_cost
            }
            EnemyArchetype::Kiter => {
                // Volley: Ranged attack at optimal distance (5-8 hexes)
                // Kiter already uses Volley via its own system, but we provide fallback logic
                let volley_stamina_cost = 25.0;
                distance >= 5 && distance <= 8 && stamina.state >= volley_stamina_cost
            }
            EnemyArchetype::Defender => {
                // Counter is reactive only - never proactively triggered
                false
            }
        };

        if should_use_ability {
            // Trigger ability (no target_loc needed - targeting handles it server-side)
            writer.write(Try {
                event: Event::UseAbility {
                    ent: npc_entity,
                    ability,
                    target_loc: None,
                },
            });
        }
    }
}
