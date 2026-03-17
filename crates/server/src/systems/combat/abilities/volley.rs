use bevy::prelude::*;
use common_bevy::{
    components::{entity_type::*, Loc, reaction_queue::DamageType},
    message::{AbilityFailReason, AbilityType, Do, Try, Event as GameEvent},
};

/// Handle Volley ability (NPC ranged attack)
/// - NPC-only ability (typically used by Kite behavior)
/// - Ranged attack (optimal 5-8 hex range)
/// - Base damage from Force meta-attribute (scales with might + level)
/// - No stamina cost, cooldown tracked in Kite component
/// - Attacks single hostile target
pub fn handle_volley(
    mut commands: Commands,
    mut reader: MessageReader<Try>,
    entity_query: Query<(&EntityType, &Loc, Option<&common_bevy::components::behaviour::PlayerControlled>)>,
    loc_query: Query<&Loc>,
    attrs_query: Query<&common_bevy::components::ActorAttributes>,
    respawn_query: Query<&common_bevy::components::resources::RespawnTimer>,
    mut writer: MessageWriter<Do>,
) {
    for event in reader.read() {
        let Try { event: GameEvent::UseAbility { ent, ability, target: ability_target } } = event else {
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

        // Validate target entity is provided
        let Some(target_ent) = *ability_target else {
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

        // Skip dead targets
        if respawn_query.get(target_ent).is_ok() {
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::NoTargets,
                },
            });
            continue;
        }

        // Validate target is within range (optimal 5-8 hexes for kite behavior)
        // Allow slightly wider range (1-10 hexes) for flexibility
        let Ok(target_loc) = loc_query.get(target_ent) else {
            continue;
        };
        let distance = caster_loc.flat_distance(target_loc);

        if distance < 1 || distance > 10 {
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::OutOfRange,
                },
            });
            continue;
        }

        // Validate target is a hostile actor (asymmetric targeting)
        let caster_is_player = entity_query
            .get(*ent)
            .ok()
            .and_then(|(_, _, pc_opt)| pc_opt)
            .is_some();

        let target_valid = entity_query
            .get(target_ent)
            .ok()
            .map(|(et, _, pc_opt)| {
                matches!(et, EntityType::Actor(_)) && pc_opt.is_some() != caster_is_player
            })
            .unwrap_or(false);

        if !target_valid {
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::NoTargets,
                },
            });
            continue;
        };

        // Send DealDamage event for this target
        // Base damage from Force meta-attribute (same as Lunge)
        let attrs = attrs_query.get(*ent).expect("Volley caster must have ActorAttributes");
        let base_damage = attrs.force();

        commands.trigger(
            Try {
                event: GameEvent::DealDamage {
                    source: *ent,
                    target: target_ent,
                    base_damage,
                    damage_type: DamageType::Physical,
                    ability: Some(AbilityType::Volley),
                },
            },
        );
    }
}
