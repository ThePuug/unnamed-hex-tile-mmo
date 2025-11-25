use bevy::prelude::*;
use crate::common::{
    components::{entity_type::*, Loc, reaction_queue::DamageType},
    message::{AbilityFailReason, AbilityType, Do, Try, Event as GameEvent},
    plugins::nntree::*,
};

/// Handle AutoAttack ability
/// - Passive ability (no stamina cost, no GCD)
/// - Attacks ALL hostile entities on target hex
/// - Requires target_loc to be adjacent (distance == 1)
pub fn handle_auto_attack(
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

        // Filter for AutoAttack only
        let Some(AbilityType::AutoAttack) = (ability == &AbilityType::AutoAttack).then_some(ability) else {
            continue;
        };

        // Check if caster is dead (has RespawnTimer)
        if respawn_query.get(*ent).is_ok() {
            // Dead players can't use abilities - silently ignore
            continue;
        }

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

        // Validate target hex is adjacent (considering slopes)
        // Adjacent means 1 hex away horizontally with at most 1 z-level difference
        let target_loc = Loc::new(*target_qrz);

        if !caster_loc.is_adjacent(&target_loc) {
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

        // Find ALL hostile entities on target hex using NNTree
        let target_entities: Vec<Entity> = nntree
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
                    // Asymmetric targeting: can only attack entities on opposite "team"
                    if caster_is_player != target_is_player {
                        Some(target_ent)
                    } else {
                        None  // Same team - no friendly fire
                    }
                })
            })
            .collect();

        // If no valid targets on the hex, fail
        if target_entities.is_empty() {
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::NoTargets,
                },
            });
            continue;
        }

        // Deal damage to ALL hostile entities on the target hex
        let base_damage = 20.0;
        for target_ent in target_entities {
            commands.trigger(
                Try {
                    event: GameEvent::DealDamage {
                        source: *ent,
                        target: target_ent,
                        base_damage,
                        damage_type: DamageType::Physical,
                        ability: Some(AbilityType::AutoAttack),
                    },
                },
            );
        }

        // AutoAttack does NOT trigger GCD (passive ability) - no event emitted
    }
}
