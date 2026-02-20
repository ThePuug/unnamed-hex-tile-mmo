use bevy::prelude::*;
use std::time::Duration;
use common::{
    components::{entity_type::*, resources::*, stagger::Stagger, Loc, reaction_queue::{DamageType, ReactionQueue, QueuedThreat}, recovery::{GlobalRecovery, get_ability_recovery_duration}},
    message::{AbilityFailReason, AbilityType, ClearType, Do, Try, Event as GameEvent},
    resources::map::Map,
    systems::combat::synergies::apply_synergies,
};
use crate::{resources::RunTime, systems::stagger::Knockback};

use qrz::Qrz;

/// Calculate knockback destination using greedy terrain-following pathfinding.
/// Projects a far target in the knockback direction, then uses `greedy_path`
/// to find the actual terrain-following path. Returns (destination, tiles_pushed).
fn calculate_knockback_destination(
    source_loc: Qrz,
    direction: Qrz,
    distance: i16,
    map: &Map,
) -> (Qrz, i16) {
    // Project far target well beyond knockback range
    let far_target = Qrz {
        q: source_loc.q + direction.q * 20,
        r: source_loc.r + direction.r * 20,
        z: 0,
    };

    let path = map.greedy_path(source_loc, far_target, distance as usize);
    if path.is_empty() {
        (source_loc, 0)
    } else {
        (*path.last().unwrap(), path.len() as i16)
    }
}

/// Handle Kick ability — REACTIVE KICK
/// - 40 stamina cost
/// - Clears all visible window threats
/// - Deals 75% Technique damage to adjacent threat sources
/// - Knockback adjacent sources 4 tiles directly away
/// - Self-synergy: Kick → Kick
pub fn handle_kick(
    mut commands: Commands,
    mut reader: MessageReader<Try>,
    entity_query: Query<(&EntityType, &Loc)>,
    mut queue_query: Query<(&Loc, &mut ReactionQueue)>,
    mut stamina_query: Query<&mut Stamina>,
    attrs_query: Query<&common::components::ActorAttributes>,
    recovery_query: Query<&GlobalRecovery>,
    synergy_query: Query<&common::components::recovery::SynergyUnlock>,
    respawn_query: Query<&RespawnTimer>,
    time: Res<Time>,
    runtime: Res<RunTime>,
    map: Res<Map>,
    mut writer: MessageWriter<Do>,
) {
    for event in reader.read() {
        let Try { event: GameEvent::UseAbility { ent, ability, target: _ } } = event else {
            continue;
        };

        if *ability != AbilityType::Kick {
            continue;
        }

        // Check if caster is dead
        if respawn_query.get(*ent).is_ok() {
            continue;
        }

        // Check recovery lockout (unless synergy-unlocked for Kick)
        if let Ok(recovery) = recovery_query.get(*ent) {
            if recovery.is_active() {
                let is_synergy_unlocked = synergy_query
                    .get(*ent)
                    .ok()
                    .map(|synergy| {
                        synergy.ability == AbilityType::Kick
                            && synergy.is_unlocked(recovery.remaining)
                    })
                    .unwrap_or(false);

                if !is_synergy_unlocked {
                    writer.write(Do {
                        event: GameEvent::AbilityFailed {
                            ent: *ent,
                            reason: AbilityFailReason::OnCooldown,
                        },
                    });
                    continue;
                }
            }
        }

        // Get caster's attributes and location
        let Ok(caster_attrs) = attrs_query.get(*ent) else {
            continue;
        };

        let caster_loc = {
            let Ok((loc, _)) = queue_query.get(*ent) else {
                writer.write(Do {
                    event: GameEvent::AbilityFailed {
                        ent: *ent,
                        reason: AbilityFailReason::NoTargets,
                    },
                });
                continue;
            };
            *loc
        };

        // Get visible window threats (collect to drop borrow)
        let visible_threats: Vec<QueuedThreat> = {
            let Ok((_, queue)) = queue_query.get(*ent) else {
                continue;
            };
            queue.threats.iter()
                .take(queue.window_size)
                .copied()
                .collect()
        };

        if visible_threats.is_empty() {
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::NoTargets,
                },
            });
            continue;
        }

        // Check stamina (40 cost)
        let kick_stamina_cost = 40.0;
        let Ok(mut stamina) = stamina_query.get_mut(*ent) else {
            continue;
        };

        if stamina.state < kick_stamina_cost {
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::InsufficientStamina,
                },
            });
            continue;
        }

        // Consume stamina
        stamina.state -= kick_stamina_cost;
        stamina.step = stamina.state;

        writer.write(Do {
            event: GameEvent::Incremental {
                ent: *ent,
                component: common::message::Component::Stamina(*stamina),
            },
        });

        let now_ms = time.elapsed().as_millis() + runtime.elapsed_offset;
        let now = Duration::from_millis(now_ms.min(u64::MAX as u128) as u64);

        use common::systems::combat::queue::create_threat;

        // Process each visible threat: deal damage and knockback adjacent sources
        for threat in &visible_threats {
            // Only affect sources that are alive and adjacent
            if respawn_query.get(threat.source).is_ok() {
                continue;
            }
            let Ok((_, source_loc)) = entity_query.get(threat.source) else {
                continue;
            };
            if caster_loc.flat_distance(source_loc) != 1 {
                continue;
            }

            // Deal 75% Technique damage via threat insertion
            let kick_damage = caster_attrs.technique() * 0.75;

            if let Ok((_, mut target_queue)) = queue_query.get_mut(threat.source) {
                if let Ok(target_attrs) = attrs_query.get(threat.source) {
                    let kick_threat = create_threat(
                        *ent,
                        target_attrs,
                        caster_attrs,
                        kick_damage,
                        DamageType::Physical,
                        Some(AbilityType::Kick),
                        now,
                    );

                    target_queue.threats.push_back(kick_threat);

                    writer.write(Do {
                        event: GameEvent::InsertThreat {
                            ent: threat.source,
                            threat: kick_threat,
                        },
                    });
                }
            }

            // Knockback: push direction is source - caster (away from kicker)
            let direction = Qrz {
                q: source_loc.q - caster_loc.q,
                r: source_loc.r - caster_loc.r,
                z: 0,
            };

            // Calculate full knockback destination via greedy terrain-following path
            let (kb_destination, tiles_pushed) = calculate_knockback_destination(**source_loc, direction, 4, &map);

            if tiles_pushed > 0 {
                // Send MovementIntent so client starts visual slide immediately.
                // +Z: entities stand ON terrain (matches MovementIntent convention).
                let knockback_duration_ms = tiles_pushed as u16 * 125;
                writer.write(Do {
                    event: GameEvent::MovementIntent {
                        ent: threat.source,
                        destination: kb_destination + Qrz::Z,
                        duration_ms: knockback_duration_ms,
                    },
                });

                // Tile-by-tile knockback: process_knockback moves 1 tile per server tick (125ms).
                // Stagger freezes AI movement; Knockback handles the physical push.
                commands.entity(threat.source).insert((
                    Knockback { destination: kb_destination, remaining_tiles: tiles_pushed },
                    Stagger::new(0.5),
                ));
            } else {
                // Can't push (immediately blocked), just stagger
                commands.entity(threat.source).insert(Stagger::new(0.5));
            }
        }

        // Drain visible threats from caster's queue
        if let Ok((_, mut caster_queue)) = queue_query.get_mut(*ent) {
            let count = visible_threats.len();
            caster_queue.threats.drain(..count);

            writer.write(Do {
                event: GameEvent::ClearQueue {
                    ent: *ent,
                    clear_type: ClearType::First(count),
                },
            });
        }

        // Broadcast ability success
        writer.write(Do {
            event: GameEvent::UseAbility {
                ent: *ent,
                ability: AbilityType::Kick,
                target: None,
            },
        });

        // Trigger recovery lockout
        let recovery_duration = get_ability_recovery_duration(AbilityType::Kick);
        let recovery = GlobalRecovery::new(recovery_duration, AbilityType::Kick);
        commands.entity(*ent).insert(recovery);

        // Apply synergies (self-synergy: Kick → Kick)
        let Ok(attrs) = attrs_query.get(*ent) else {
            continue;
        };
        apply_synergies(*ent, AbilityType::Kick, &recovery, attrs, attrs, &mut commands);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::components::entity_type::EntityType;

    fn make_test_map() -> Map {
        let mut qrz_map = qrz::Map::<EntityType>::new(1.0, 0.8);
        // Flat terrain: 10x10 grid at z=0
        for q in -5..=5 {
            for r in -5..=5 {
                qrz_map.insert(Qrz { q, r, z: 0 }, EntityType::Decorator(default()));
            }
        }
        Map::new(qrz_map)
    }

    #[test]
    fn knockback_flat_terrain_pushes_full_distance() {
        let map = make_test_map();
        let source = Qrz { q: 0, r: 0, z: 0 };
        let direction = Qrz { q: 1, r: 0, z: 0 }; // East

        let (dest, tiles) = calculate_knockback_destination(source, direction, 4, &map);
        assert_eq!(dest.q, 4);
        assert_eq!(dest.r, 0);
        assert_eq!(tiles, 4);
    }

    #[test]
    fn knockback_stops_at_map_edge() {
        let map = make_test_map();
        // Start near edge, push east
        let source = Qrz { q: 4, r: 0, z: 0 };
        let direction = Qrz { q: 1, r: 0, z: 0 };

        let (dest, _) = calculate_knockback_destination(source, direction, 4, &map);
        // Should stop at q=5 (last tile with floor)
        assert_eq!(dest.q, 5);
        assert_eq!(dest.r, 0);
    }

    #[test]
    fn knockback_stops_at_cliff() {
        let mut qrz_map = qrz::Map::<EntityType>::new(1.0, 0.8);
        // Flat tiles z=0 from q=0..2, then cliff at q=3 (z=5)
        for q in 0..=2 {
            qrz_map.insert(Qrz { q, r: 0, z: 0 }, EntityType::Decorator(default()));
        }
        qrz_map.insert(Qrz { q: 3, r: 0, z: 5 }, EntityType::Decorator(default()));
        qrz_map.insert(Qrz { q: 4, r: 0, z: 5 }, EntityType::Decorator(default()));
        let map = Map::new(qrz_map);

        let source = Qrz { q: 0, r: 0, z: 0 };
        let direction = Qrz { q: 1, r: 0, z: 0 };

        let (dest, tiles) = calculate_knockback_destination(source, direction, 4, &map);
        // Should stop at q=2 (before cliff at q=3)
        assert_eq!(dest.q, 2);
        assert_eq!(dest.r, 0);
        assert_eq!(dest.z, 0);
        assert_eq!(tiles, 2);
    }

    #[test]
    fn knockback_zero_distance_returns_source() {
        let map = make_test_map();
        let source = Qrz { q: 0, r: 0, z: 0 };
        let direction = Qrz { q: 1, r: 0, z: 0 };

        let (dest, tiles) = calculate_knockback_destination(source, direction, 0, &map);
        assert_eq!(dest, source);
        assert_eq!(tiles, 0);
    }

    #[test]
    fn knockback_all_six_directions() {
        let map = make_test_map();
        let source = Qrz { q: 0, r: 0, z: 0 };

        for dir in qrz::DIRECTIONS.iter() {
            let (dest, tiles) = calculate_knockback_destination(source, *dir, 2, &map);
            let flat = Qrz { q: 0, r: 0, z: 0 };
            assert_eq!(dest.flat_distance(&flat), 2,
                "Direction ({},{}) should push 2 tiles", dir.q, dir.r);
            assert_eq!(tiles, 2);
        }
    }

    #[test]
    fn knockback_allows_gentle_slopes() {
        let mut qrz_map = qrz::Map::<EntityType>::new(1.0, 0.8);
        // Gradual slope: z increases by 1 each tile (passable)
        for q in 0..=4 {
            qrz_map.insert(Qrz { q, r: 0, z: q }, EntityType::Decorator(default()));
        }
        let map = Map::new(qrz_map);

        let source = Qrz { q: 0, r: 0, z: 0 };
        let direction = Qrz { q: 1, r: 0, z: 0 };

        let (dest, _) = calculate_knockback_destination(source, direction, 4, &map);
        assert_eq!(dest.q, 4);
        assert_eq!(dest.z, 4);
    }
}
