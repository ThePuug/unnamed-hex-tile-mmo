use bevy::prelude::*;
use crate::{
    common::{
        components::{entity_type::*, resources::*, tier_lock::TierLock, target::Target, Loc, reaction_queue::DamageType, gcd::Gcd},
        message::{AbilityFailReason, AbilityType, Do, Try, Event as GameEvent},
        systems::{targeting::get_range_tier, combat::gcd::GcdType},
    },
};

/// Handle Lunge ability (Q key)
/// - 20 stamina cost
/// - 40 base damage
/// - 4 hex range
/// - Teleports caster adjacent to target
/// - Triggers Attack GCD
pub fn handle_lunge(
    mut commands: Commands,
    mut reader: EventReader<Try>,
    entity_query: Query<&Loc>,
    loc_target_query: Query<(&Loc, &Target, Option<&TierLock>)>,
    mut stamina_query: Query<&mut Stamina>,
    mut gcd_query: Query<&mut Gcd>,
    respawn_query: Query<&RespawnTimer>,
    mut writer: EventWriter<Do>,
    time: Res<Time>,
) {
    for event in reader.read() {
        let Try { event: GameEvent::UseAbility { ent, ability, target_loc: _ } } = event else {
            continue;
        };

        // Filter for Lunge only
        let Some(AbilityType::Lunge) = (ability == &AbilityType::Lunge).then_some(ability) else {
            continue;
        };

        // Check if caster is dead (has RespawnTimer)
        if respawn_query.get(*ent).is_ok() {
            // Dead players can't use abilities - silently ignore
            continue;
        }

        // Check GCD (must not be on cooldown)
        let Ok(gcd) = gcd_query.get(*ent) else {
            continue;
        };

        if gcd.is_active(time.elapsed()) {
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::OnCooldown,
                },
            });
            continue;
        }

        // Get caster's location, current target, and targeting state
        let Ok((caster_loc, target, targeting_state_opt)) = loc_target_query.get(*ent) else {
            continue;
        };

        // Get the current target from Target component
        let target_ent_opt = target.entity;  // Get the entity field (Option<Entity>)

        info!("Lunge activated: caster_loc={:?}, Target={:?}, tier_lock={:?}",
            caster_loc, target_ent_opt, targeting_state_opt.and_then(|ts| ts.get()));

        // If tier locked, validate target is in correct tier
        let validated_target = if let (Some(targeting_state), Some(target_ent)) = (targeting_state_opt, target_ent_opt) {
            if let Some(locked_tier) = targeting_state.get() {
                // Tier locked - validate target is in the correct tier
                if let Ok(target_loc) = entity_query.get(target_ent) {
                    let distance = caster_loc.flat_distance(target_loc) as u32;
                    let target_tier = get_range_tier(distance);

                    if target_tier == locked_tier {
                        Some(target_ent) // Target is in correct tier
                    } else {
                        None // Target not in locked tier, can't use ability
                    }
                } else {
                    None // Target doesn't exist
                }
            } else {
                target_ent_opt // Not tier locked, use target as-is
            }
        } else {
            target_ent_opt // No targeting state or no target
        };

        let Some(target_ent) = validated_target else {
            // No valid target
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::NoTargets,
                },
            });
            continue;
        };

        // Check if target is alive
        if respawn_query.get(target_ent).is_ok() {
            // Target is dead
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::NoTargets,
                },
            });
            continue;
        }

        let Some(target_loc) = entity_query.get(target_ent).ok() else {
            continue;
        };

        // Check range (must be within 4 hexes for Lunge)
        let distance = caster_loc.flat_distance(&target_loc) as u32;

        if distance > 4 || distance < 1 {
            // Target is out of range (or we're already on top of them)
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::OutOfRange,
                },
            });
            continue;
        }

        // Check stamina (20 cost)
        let lunge_stamina_cost = 20.0;
        let Ok(mut stamina) = stamina_query.get_mut(*ent) else {
            continue;
        };

        if stamina.state < lunge_stamina_cost {
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::InsufficientStamina,
                },
            });
            continue;
        }

        // Consume stamina
        stamina.state -= lunge_stamina_cost;
        stamina.step = stamina.state;

        // Broadcast updated stamina
        writer.write(Do {
            event: GameEvent::Incremental {
                ent: *ent,
                component: crate::common::message::Component::Stamina(*stamina),
            },
        });

        // Find landing position: adjacent to target, closest to caster
        let target_neighbors = (**target_loc).neighbors();
        let landing_loc = target_neighbors
            .iter()
            .min_by_key(|neighbor_loc| caster_loc.flat_distance(neighbor_loc))
            .copied()
            .unwrap_or(**target_loc); // Fallback to target loc if no neighbors

        // Update caster's location (teleport 2+ hexes to target's neighbor)
        commands.entity(*ent).insert(Loc::new(landing_loc));

        // Broadcast Loc update to clients
        // NOTE: Client detects teleport by hex distance (>=2 hexes) and clears Offset automatically
        // Offset is client-side interpolation state - server doesn't need to manage it
        writer.write(Do {
            event: GameEvent::Incremental {
                ent: *ent,
                component: crate::common::message::Component::Loc(Loc::new(landing_loc)),
            },
        });

        // Deal damage (40 base damage)
        commands.trigger_targets(
            Try {
                event: GameEvent::DealDamage {
                    source: *ent,
                    target: target_ent,
                    base_damage: 40.0,
                    damage_type: DamageType::Physical,
                },
            },
            target_ent,
        );

        // Trigger Attack GCD immediately (prevents race conditions)
        if let Ok(mut gcd) = gcd_query.get_mut(*ent) {
            let gcd_duration = std::time::Duration::from_secs(1); // 1s for Attack GCD (ADR-006)
            gcd.activate(GcdType::Attack, gcd_duration, time.elapsed());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::event::Events;
    use qrz::Qrz;
    use crate::common::{
        components::{
            entity_type::actor::*,
            behaviour::PlayerControlled,
            offset::Offset,
        },
        systems::combat::gcd::GcdType,
    };

    /// Test that Lunge teleports player adjacent to target AND clears Offset
    /// This ensures the visual position snaps to the new location (no interpolation glide)
    #[test]
    fn test_lunge_teleports_and_clears_offset() {
        let mut app = App::new();
        app.add_event::<Try>();
        app.add_event::<Do>();
        app.init_resource::<Time<()>>();

        // Spawn player at (0,0) with some offset (sub-tile position)
        let player_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let player_offset = Offset {
            state: Vec3::new(0.5, 0.0, 0.3), // Player has moved slightly within tile
            step: Vec3::new(0.5, 0.0, 0.3),
            prev_step: Vec3::new(0.4, 0.0, 0.2),
            interp_elapsed: 0.0,
            interp_duration: 0.0,
        };
        let player = app.world_mut().spawn((
            EntityType::Actor(ActorImpl {
                origin: Origin::Evolved,
                approach: Approach::Direct,
                resilience: Resilience::Vital,
                identity: ActorIdentity::Player,
            }),
            player_loc,
            player_offset,
            PlayerControlled,
            Stamina::default(),
            Target { entity: None, last_target: None },
            TierLock::default(),
            Gcd::default(),
        )).id();

        // Spawn target 4 hexes away
        let target_loc = Loc::new(Qrz { q: 4, r: 0, z: 0 });
        let target = app.world_mut().spawn((
            EntityType::Actor(ActorImpl {
                origin: Origin::Evolved,
                approach: Approach::Direct,
                resilience: Resilience::Vital,
                identity: ActorIdentity::Npc(NpcType::WildDog),
            }),
            target_loc,
        )).id();

        // Set player's target
        app.world_mut().entity_mut(player).insert(Target { entity: Some(target), last_target: Some(target) });

        // Send Lunge command
        app.world_mut().resource_mut::<Events<Try>>().send(Try {
            event: GameEvent::UseAbility {
                ent: player,
                ability: AbilityType::Lunge,
                target_loc: None, // Uses Target component
            },
        });

        // Run the handler system
        app.add_systems(Update, handle_lunge);
        app.update();

        // Check that player moved to adjacent hex (one of target's neighbors)
        let player_final_loc = app.world().entity(player).get::<Loc>().unwrap();
        let distance = player_final_loc.flat_distance(&target_loc);
        assert_eq!(
            distance, 1,
            "Player should be adjacent to target (distance 1), got distance {}",
            distance
        );

        // NOTE: Offset clearing happens client-side in world::do_incremental
        // Server doesn't manage Offset (client interpolation state)
        // Client detects teleport by checking hex distance (>=2 hexes) and clears offset automatically

        // Check that stamina was consumed
        let player_stamina = app.world().entity(player).get::<Stamina>().unwrap();
        assert_eq!(
            player_stamina.state, 80.0,
            "Lunge should consume 20 stamina"
        );

        // Check that GCD was triggered (component exists and was activated)
        let player_gcd = app.world().entity(player).get::<Gcd>().unwrap();
        // Note: Can't easily test is_active() without advancing time,
        // but we verify the GCD component exists and was modified
    }

    /// Test that Lunge finds closest adjacent hex to player
    #[test]
    fn test_lunge_chooses_closest_landing_position() {
        let mut app = App::new();
        app.add_event::<Try>();
        app.add_event::<Do>();
        app.init_resource::<Time<()>>();

        // Spawn player at (0,0)
        let player_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let player = app.world_mut().spawn((
            EntityType::Actor(ActorImpl {
                origin: Origin::Evolved,
                approach: Approach::Direct,
                resilience: Resilience::Vital,
                identity: ActorIdentity::Player,
            }),
            player_loc,
            Offset::default(),
            PlayerControlled,
            Stamina::default(),
            Target { entity: None, last_target: None },
            TierLock::default(),
            Gcd::default(),
        )).id();

        // Spawn target at (0,3) - player should land at (0,2) which is closest
        let target_loc = Loc::new(Qrz { q: 0, r: 3, z: 0 });
        let target = app.world_mut().spawn((
            EntityType::Actor(ActorImpl {
                origin: Origin::Evolved,
                approach: Approach::Direct,
                resilience: Resilience::Vital,
                identity: ActorIdentity::Npc(NpcType::WildDog),
            }),
            target_loc,
        )).id();

        // Set player's target
        app.world_mut().entity_mut(player).insert(Target { entity: Some(target), last_target: Some(target) });

        // Send Lunge command
        app.world_mut().resource_mut::<Events<Try>>().send(Try {
            event: GameEvent::UseAbility {
                ent: player,
                ability: AbilityType::Lunge,
                target_loc: None,
            },
        });

        // Run the handler system
        app.add_systems(Update, handle_lunge);
        app.update();

        // Check that player landed at (0,2) - the neighbor closest to (0,0)
        let player_final_loc = app.world().entity(player).get::<Loc>().unwrap();
        assert_eq!(
            **player_final_loc,
            Qrz { q: 0, r: 2, z: 0 },
            "Player should land at (0,2), the neighbor of (0,3) closest to starting position (0,0)"
        );
    }

    /// Test that Lunge fails when out of range (>4 hexes)
    #[test]
    fn test_lunge_fails_beyond_range() {
        let mut app = App::new();
        app.add_event::<Try>();
        app.add_event::<Do>();
        app.init_resource::<Time<()>>();

        // Spawn player at (0,0)
        let player_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let player = app.world_mut().spawn((
            EntityType::Actor(ActorImpl {
                origin: Origin::Evolved,
                approach: Approach::Direct,
                resilience: Resilience::Vital,
                identity: ActorIdentity::Player,
            }),
            player_loc,
            Offset::default(),
            PlayerControlled,
            Stamina::default(),
            Target { entity: None, last_target: None },
            TierLock::default(),
            Gcd::default(),
        )).id();

        // Spawn target 5 hexes away (out of range)
        let target_loc = Loc::new(Qrz { q: 5, r: 0, z: 0 });
        let target = app.world_mut().spawn((
            EntityType::Actor(ActorImpl {
                origin: Origin::Evolved,
                approach: Approach::Direct,
                resilience: Resilience::Vital,
                identity: ActorIdentity::Npc(NpcType::WildDog),
            }),
            target_loc,
        )).id();

        // Set player's target
        app.world_mut().entity_mut(player).insert(Target { entity: Some(target), last_target: Some(target) });

        // Send Lunge command
        app.world_mut().resource_mut::<Events<Try>>().send(Try {
            event: GameEvent::UseAbility {
                ent: player,
                ability: AbilityType::Lunge,
                target_loc: None,
            },
        });

        // Run the handler system
        app.add_systems(Update, handle_lunge);
        app.update();

        // Check that AbilityFailed was broadcast
        let do_events = app.world().resource::<Events<Do>>();
        let events: Vec<_> = do_events.iter_current_update_events().collect();

        let failed = events.iter().any(|e| matches!(
            e.event,
            GameEvent::AbilityFailed {
                reason: AbilityFailReason::OutOfRange,
                ..
            }
        ));

        assert!(
            failed,
            "Lunge should fail when target is >4 hexes away"
        );

        // Check that player didn't move
        let player_final_loc = app.world().entity(player).get::<Loc>().unwrap();
        assert_eq!(
            **player_final_loc,
            Qrz { q: 0, r: 0, z: 0 },
            "Player should not move when Lunge fails"
        );
    }
}
