use bevy::prelude::*;
use common_bevy::{
    components::{
        entity_type::*, Loc, reaction_queue::DamageType,
    },
    message::{AbilityFailReason, AbilityType, Do, Try, Event as GameEvent},
};

/// Handle AutoAttack ability — entity-based, single-target

/// - Passive ability (no stamina cost, no GCD)
/// - Reads target entity from the UseAbility event (player's intended target)
/// - Server-side range check using current positions (not stale client data)
/// - Deals damage to exactly one entity
pub fn handle_auto_attack(
    mut commands: Commands,
    mut reader: MessageReader<Try>,
    loc_query: Query<&Loc>,
    entity_type_query: Query<(&EntityType, Option<&common_bevy::components::behaviour::Side>)>,
    attrs_query: Query<&common_bevy::components::ActorAttributes>,
    range_query: Query<&common_bevy::components::AttackRange>,
    respawn_query: Query<&common_bevy::components::resources::RespawnTimer>,
    mut writer: MessageWriter<Do>,
) {
    for event in reader.read() {
        let Try { event: GameEvent::UseAbility { ent, ability, target: event_target } } = event else {
            continue;
        };

        if *ability != AbilityType::AutoAttack {
            continue;
        }

        // Dead casters can't attack
        if respawn_query.get(*ent).is_ok() {
            continue;
        }

        // Read target from the event (player's intended target)
        let Some(target_ent) = *event_target else {
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::NoTargets,
                },
            });
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

        // Validate target is an actor on a hostile side
        let caster_side = entity_type_query.get(*ent).ok().and_then(|(_, side)| side.copied());
        let target_valid = entity_type_query
            .get(target_ent)
            .ok()
            .is_some_and(|(et, side)| {
                matches!(et, EntityType::Actor(_))
                    && matches!((caster_side, side), (Some(a), Some(b)) if a.is_hostile_to(*b))
            });

        if !target_valid {
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::NoTargets,
                },
            });
            continue;
        }

        // Server-side range check using current positions
        let Ok(caster_loc) = loc_query.get(*ent) else {
            continue;
        };
        let Ok(target_loc) = loc_query.get(target_ent) else {
            continue;
        };

        let max_range = range_query.get(*ent).map_or(1, |r| r.0);
        if caster_loc.distance(target_loc) > max_range {
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::OutOfRange,
                },
            });
            continue;
        }

        // Deal damage — Gravitas at 25%: auto-attacks are pressure, bought with Presence
        let attrs = attrs_query.get(*ent).expect("Auto-attack caster must have ActorAttributes");
        let base_damage = attrs.gravitas() * 0.25;

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
}
