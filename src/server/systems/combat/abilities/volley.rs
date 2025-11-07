use bevy::prelude::*;
use crate::common::{
    components::{entity_type::*, Loc, reaction_queue::DamageType},
    message::{AbilityFailReason, AbilityType, Do, Try, Event as GameEvent},
    plugins::nntree::*,
};

/// Handle Volley ability (NPC ranged attack)
/// - NPC-only ability (typically used by Kite behavior)
/// - Ranged attack (optimal 5-8 hex range)
/// - No stamina cost, cooldown tracked in Kite component
/// - Attacks single hostile target
pub fn handle_volley(
    mut commands: Commands,
    mut reader: EventReader<Try>,
    entity_query: Query<(&EntityType, &Loc, Option<&crate::common::components::behaviour::PlayerControlled>)>,
    loc_query: Query<&Loc>,
    respawn_query: Query<&crate::common::components::resources::RespawnTimer>,
    nntree: Res<NNTree>,
    mut writer: EventWriter<Do>,
) {
    for event in reader.read() {
        let Try { event: GameEvent::UseAbility { ent, ability, target_loc: ability_target_loc } } = event else {
            continue;
        };

        // Filter for Volley only
        let Some(AbilityType::Volley) = (ability == &AbilityType::Volley).then_some(ability) else {
            continue;
        };

        // Check if caster is dead (has RespawnTimer)
        if respawn_query.get(*ent).is_ok() {
            // Dead NPCs can't use abilities - silently ignore
            continue;
        };

        // Validate target_loc is provided
        let Some(target_qrz) = ability_target_loc else {
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::NoTargets,
                },
            });
            continue;
        };

        // Get caster's location
        let Ok(caster_loc) = loc_query.get(*ent) else {
            continue;
        };

        // Validate target is within range (optimal 5-8 hexes for kite behavior)
        // Allow slightly wider range (1-10 hexes) for flexibility
        let target_loc = Loc::new(*target_qrz);
        let distance = caster_loc.flat_distance(&target_loc);

        if distance < 1 || distance > 10 {
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::OutOfRange,
                },
            });
            continue;
        }

        // Determine if caster is a player (for asymmetric targeting)
        let caster_is_player = entity_query
            .get(*ent)
            .ok()
            .and_then(|(_, _, pc_opt)| pc_opt)
            .is_some();

        // Find single hostile target on target hex using NNTree
        let target_entity: Option<Entity> = nntree
            .locate_within_distance(target_loc, 0) // Distance 0 = exact location match
            .filter_map(|nn| {
                let target_ent = nn.ent;

                // Skip self
                if target_ent == *ent {
                    return None;
                }

                // Skip dead players (with RespawnTimer)
                if respawn_query.get(target_ent).is_ok() {
                    return None;
                }

                // Check if this is a valid hostile target (asymmetric targeting)
                entity_query.get(target_ent).ok().and_then(|(et, _, player_controlled_opt)| {
                    // Check if it's an Actor
                    if !matches!(et, EntityType::Actor(_)) {
                        return None;
                    }

                    let target_is_player = player_controlled_opt.is_some();
                    // Asymmetric targeting: NPCs can only attack players
                    if caster_is_player != target_is_player {
                        Some(target_ent)
                    } else {
                        None
                    }
                })
            })
            .next(); // Take first valid target

        // If no valid target found, fail the ability
        let Some(target) = target_entity else {
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::NoTargets,
                },
            });
            continue;
        };

        // Send DealDamage event for this target
        // Volley: 20 damage, Physical type
        commands.trigger_targets(
            Try {
                event: GameEvent::DealDamage {
                    source: *ent,
                    target,
                    base_damage: 20.0,
                    damage_type: DamageType::Physical,
                    ability: Some(AbilityType::Volley),
                },
            },
            target,
        );
    }
}
