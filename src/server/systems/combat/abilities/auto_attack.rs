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
            commands.trigger_targets(
                Try {
                    event: GameEvent::DealDamage {
                        source: *ent,
                        target: target_ent,
                        base_damage,
                        damage_type: DamageType::Physical,
                    },
                },
                target_ent,
            );
        }

        // AutoAttack does NOT trigger GCD (passive ability) - no event emitted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::event::Events;
    use qrz::Qrz;
    use crate::common::components::{
        entity_type::actor::*,
        behaviour::PlayerControlled,
    };

    /// Test that auto-attack works on adjacent targets with vertical offset
    /// With sloping terrain, a target 1 hex away horizontally but 1 z-level different
    /// should still be considered adjacent for melee attacks
    #[test]
    fn test_auto_attack_adjacent_with_vertical_offset() {
        let mut app = App::new();
        app.add_event::<Try>();
        app.add_event::<Do>();
        app.insert_resource(NNTree::new_for_test());

        // Spawn caster at z=0
        let caster_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let caster = app.world_mut().spawn((
            EntityType::Actor(ActorImpl {
                origin: Origin::Evolved,
                approach: Approach::Direct,
                resilience: Resilience::Vital,
                identity: ActorIdentity::Player,
            }),
            caster_loc,
            PlayerControlled,
        )).id();

        // Spawn target 1 hex away horizontally, but 1 z-level higher (z=1)
        let target_loc = Loc::new(Qrz { q: 1, r: 0, z: 1 });
        let target = app.world_mut().spawn((
            EntityType::Actor(ActorImpl {
                origin: Origin::Evolved,
                approach: Approach::Direct,
                resilience: Resilience::Vital,
                identity: ActorIdentity::Npc(NpcType::WildDog),
            }),
            target_loc,
        )).id();

        // Add both to NNTree
        app.world_mut().resource_mut::<NNTree>().insert(NearestNeighbor::new(caster, caster_loc));
        app.world_mut().resource_mut::<NNTree>().insert(NearestNeighbor::new(target, target_loc));

        // Send auto-attack command targeting the adjacent hex
        app.world_mut().resource_mut::<Events<Try>>().send(Try {
            event: GameEvent::UseAbility {
                ent: caster,
                ability: AbilityType::AutoAttack,
                target_loc: Some(*target_loc),
            },
        });

        // Run the handler system
        app.add_systems(Update, handle_auto_attack);
        app.update();

        // Should trigger DealDamage, not AbilityFailed
        let do_events = app.world().resource::<Events<Do>>();
        let events: Vec<_> = do_events.iter_current_update_events().collect();

        // Should NOT have any AbilityFailed events
        let failed = events.iter().any(|e| matches!(
            e.event,
            GameEvent::AbilityFailed { reason: AbilityFailReason::OutOfRange, .. }
        ));

        assert!(
            !failed,
            "Auto-attack should work on adjacent hex with 1 z-level difference (sloping terrain)"
        );
    }

    /// Test that auto-attack rejects targets that are too high (2+ z-levels)
    #[test]
    fn test_auto_attack_rejects_too_high_targets() {
        let mut app = App::new();
        app.add_event::<Try>();
        app.add_event::<Do>();
        app.insert_resource(NNTree::new_for_test());

        // Spawn caster at z=0
        let caster_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let caster = app.world_mut().spawn((
            EntityType::Actor(ActorImpl {
                origin: Origin::Evolved,
                approach: Approach::Direct,
                resilience: Resilience::Vital,
                identity: ActorIdentity::Player,
            }),
            caster_loc,
            PlayerControlled,
        )).id();

        // Spawn target 1 hex away but 2 z-levels higher (too steep)
        let target_loc = Loc::new(Qrz { q: 1, r: 0, z: 2 });
        let target = app.world_mut().spawn((
            EntityType::Actor(ActorImpl {
                origin: Origin::Evolved,
                approach: Approach::Direct,
                resilience: Resilience::Vital,
                identity: ActorIdentity::Npc(NpcType::WildDog),
            }),
            target_loc,
        )).id();

        // Add both to NNTree
        app.world_mut().resource_mut::<NNTree>().insert(NearestNeighbor::new(caster, caster_loc));
        app.world_mut().resource_mut::<NNTree>().insert(NearestNeighbor::new(target, target_loc));

        // Send auto-attack command
        app.world_mut().resource_mut::<Events<Try>>().send(Try {
            event: GameEvent::UseAbility {
                ent: caster,
                ability: AbilityType::AutoAttack,
                target_loc: Some(*target_loc),
            },
        });

        // Run the handler system
        app.add_systems(Update, handle_auto_attack);
        app.update();

        // Should trigger AbilityFailed with OutOfRange
        let do_events = app.world().resource::<Events<Do>>();
        let events: Vec<_> = do_events.iter_current_update_events().collect();

        let failed = events.iter().any(|e| matches!(
            e.event,
            GameEvent::AbilityFailed { reason: AbilityFailReason::OutOfRange, .. }
        ));

        assert!(
            failed,
            "Auto-attack should fail when target is 2 z-levels higher (too steep)"
        );
    }

    /// Test that auto-attack correctly rejects targets that are too far horizontally
    #[test]
    fn test_auto_attack_rejects_distant_targets() {
        let mut app = App::new();
        app.add_event::<Try>();
        app.add_event::<Do>();
        app.insert_resource(NNTree::new_for_test());

        // Spawn caster at z=0
        let caster_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let caster = app.world_mut().spawn((
            EntityType::Actor(ActorImpl {
                origin: Origin::Evolved,
                approach: Approach::Direct,
                resilience: Resilience::Vital,
                identity: ActorIdentity::Player,
            }),
            caster_loc,
            PlayerControlled,
        )).id();

        // Spawn target 2 hexes away horizontally (too far for melee)
        let target_loc = Loc::new(Qrz { q: 2, r: 0, z: 0 });
        let target = app.world_mut().spawn((
            EntityType::Actor(ActorImpl {
                origin: Origin::Evolved,
                approach: Approach::Direct,
                resilience: Resilience::Vital,
                identity: ActorIdentity::Npc(NpcType::WildDog),
            }),
            target_loc,
        )).id();

        // Add both to NNTree
        app.world_mut().resource_mut::<NNTree>().insert(NearestNeighbor::new(caster, caster_loc));
        app.world_mut().resource_mut::<NNTree>().insert(NearestNeighbor::new(target, target_loc));

        // Send auto-attack command
        app.world_mut().resource_mut::<Events<Try>>().send(Try {
            event: GameEvent::UseAbility {
                ent: caster,
                ability: AbilityType::AutoAttack,
                target_loc: Some(*target_loc),
            },
        });

        // Run the handler system
        app.add_systems(Update, handle_auto_attack);
        app.update();

        // Should trigger AbilityFailed with OutOfRange
        let do_events = app.world().resource::<Events<Do>>();
        let events: Vec<_> = do_events.iter_current_update_events().collect();

        let failed = events.iter().any(|e| matches!(
            e.event,
            GameEvent::AbilityFailed { reason: AbilityFailReason::OutOfRange, .. }
        ));

        assert!(
            failed,
            "Auto-attack should fail when target is 2 hexes away (too far for melee)"
        );
    }
}
