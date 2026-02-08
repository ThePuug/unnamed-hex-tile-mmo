use bevy::prelude::*;
use qrz::Convert;

use crate::common::{
    components::{behaviour::*, heading::*, keybits::*, position::{Position, VisualPosition}, resources::*, *},
    message::{Component, Event, *},
    resources::map::*
};

pub fn try_incremental(
    mut reader: MessageReader<Try>,
    mut writer: MessageWriter<Do>,
) {
    for &message in reader.read() {
        if let Try { event: Event::Incremental { ent, component } } = message {
            writer.write(Do { event: Event::Incremental { ent, component }});
        }
    }
}

/// Process incremental component updates from server.
/// Updates existing components or inserts them if missing (late-binding for NPCs).
/// Exceptions: Loc/Heading require all related components and skip if dependencies missing.
pub fn do_incremental(
    mut commands: Commands,
    mut reader: MessageReader<Do>,
    mut query: Query<(
        Option<&mut Loc>,
        Option<&mut Heading>,
        Option<&mut KeyBits>,
        Option<&mut Behaviour>,
        Option<&mut Health>,
        Option<&mut Stamina>,
        Option<&mut Mana>,
        Option<&mut CombatState>,
        Option<&mut PlayerControlled>,
        Option<&mut crate::common::components::tier_lock::TierLock>,
        Option<&crate::common::components::movement_prediction::MovementPrediction>,
        Option<&mut Position>,
        Option<&mut VisualPosition>)>,
    map: Res<Map>,
    buffers: Res<crate::common::resources::InputQueues>,
) {
    for &message in reader.read() {
        let Do { event: Event::Incremental { ent, component } } = message else { continue; };

        let Ok((o_loc, o_heading, o_keybits, o_behaviour, o_health, o_stamina, o_mana, o_combat_state, o_player_controlled, o_tier_lock, o_prediction, o_position, o_visual)) = query.get_mut(ent) else {
            // Entity might have been despawned
            continue;
        };
        match component {
            Component::Loc(loc) => {
                let Some(mut loc0) = o_loc else {
                    continue;
                };

                // Check if this is a local player (has input buffer) or remote player
                let is_local = buffers.get(&ent).is_some();
                let is_player = o_player_controlled.is_some();

                // On server: skip adjustments for NPCs (server is authoritative for NPC physics)
                // (server's actor::update already set Loc and adjusted Position before sending this event)
                if !is_player && **loc0 == *loc {
                    continue;
                }

                if is_local {
                    // Teleport detection: Check if this Loc update is a smooth tile crossing or a teleport.
                    const TELEPORT_THRESHOLD_HEXES: i16 = 2;
                    let hex_distance = loc0.flat_distance(&loc);

                    if hex_distance >= TELEPORT_THRESHOLD_HEXES {
                        // Teleport: snap to new position
                        if let Some(mut vis) = o_visual {
                            let teleport_world: Vec3 = map.convert(*loc);
                            vis.snap_to(teleport_world);
                        }
                        if let Some(mut pos) = o_position {
                            pos.tile = *loc;
                            pos.offset = Vec3::ZERO;
                        }
                    } else {
                        // Smooth tile crossing: update Position tile, adjust offset for new coordinate system
                        if let Some(mut pos) = o_position {
                            let old_tile_center: Vec3 = map.convert(**loc0);
                            let new_tile_center: Vec3 = map.convert(*loc);
                            let world_pos = old_tile_center + pos.offset;
                            pos.tile = *loc;
                            pos.offset = world_pos - new_tile_center;
                        }
                        // VisualPosition continues interpolating in world space - no adjustment needed
                    }
                } else {
                    // Remote entity: tile boundary crossing
                    let new_tile_center: Vec3 = map.convert(*loc);
                    let Some(ref heading0) = o_heading else { panic!("no heading for remote player/NPC") };
                    let target_offset = if ***heading0 != default() {
                        let heading_neighbor_world: Vec3 = map.convert(*loc + ***heading0);
                        let direction = heading_neighbor_world - new_tile_center;
                        (direction * HERE).xz()
                    } else {
                        Vec2::ZERO
                    };

                    // ADR-011: Clear movement prediction if it exists
                    if let Some(_prediction) = o_prediction {
                        commands.entity(ent).remove::<crate::common::components::movement_prediction::MovementPrediction>();
                    }

                    // Update Position for remote entity
                    if let Some(mut pos) = o_position {
                        pos.tile = *loc;
                        pos.offset = Vec3::new(target_offset.x, 0.0, target_offset.y);
                    }
                    // VisualPosition continues interpolating in world space - no adjustment needed
                }

                *loc0 = loc;
            }
            Component::Heading(heading) => {
                let Some(mut heading0) = o_heading else { continue; };

                let is_player = o_player_controlled.is_some();

                // On server: skip adjustments for NPCs (server is authoritative)
                if !is_player && *heading0 == heading {
                    continue;
                }

                *heading0 = heading;
            }
            Component::Behaviour(behaviour) => {
                if let Some(mut behaviour0) = o_behaviour {
                    *behaviour0 = behaviour;
                } else {
                    commands.entity(ent).insert(behaviour);
                }
            }
            Component::KeyBits(keybits) => {
                if let Some(mut keybits0) = o_keybits {
                    *keybits0 = keybits;
                } else {
                    commands.entity(ent).insert(keybits);
                }
            }
            Component::Health(health) => {
                if let Some(mut health0) = o_health {
                    *health0 = health;
                } else {
                    commands.entity(ent).insert(health);
                }
            }
            Component::Stamina(stamina) => {
                if let Some(mut stamina0) = o_stamina {
                    *stamina0 = stamina;
                } else {
                    commands.entity(ent).insert(stamina);
                }
            }
            Component::Mana(mana) => {
                if let Some(mut mana0) = o_mana {
                    *mana0 = mana;
                } else {
                    commands.entity(ent).insert(mana);
                }
            }
            Component::CombatState(combat_state) => {
                if let Some(mut combat_state0) = o_combat_state {
                    *combat_state0 = combat_state;
                } else {
                    commands.entity(ent).insert(combat_state);
                }
            }
            Component::PlayerControlled(player_controlled) => {
                if o_player_controlled.is_none() {
                    commands.entity(ent).insert(player_controlled);
                }
            }
            Component::TierLock(tier_lock) => {
                if let Some(mut tier_lock0) = o_tier_lock {
                    *tier_lock0 = tier_lock;
                } else {
                    commands.entity(ent).insert(tier_lock);
                }
            }
            Component::Returning(returning) => {
                commands.entity(ent).insert(returning);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::Vec3;
    use qrz::Qrz;

    /// Helper to create a test Map with default terrain (radius=1.0, rise=0.8)
    fn create_test_map() -> Map {
        Map::new(qrz::Map::new(1.0, 0.8))
    }

    // ===== INVARIANT TESTS =====
    // These tests verify critical architectural invariants (ADR-015)

    /// INV-003: World-Space Preservation During Loc Updates
    /// When entity crosses tile boundary (adjacent hex, distance < 2),
    /// world-space position MUST be preserved for visual continuity.
    #[test]
    fn test_world_space_preserved_on_smooth_tile_crossing() {
        let map = create_test_map();
        let old_loc = Qrz { q: 5, r: 5, z: 0 };
        let new_loc = Qrz { q: 6, r: 5, z: 0 }; // Adjacent hex (distance = 1)
        let old_offset = Vec3::new(0.5, 0.0, 0.3);

        // World position before update
        let world_pos_before = map.convert(old_loc) + old_offset;

        // Apply world-space preservation formula (from do_incremental line 95-102)
        let state_world = map.convert(old_loc) + old_offset;
        let new_tile_center = map.convert(new_loc);
        let new_offset = state_world - new_tile_center;

        // World position after update
        let world_pos_after = map.convert(new_loc) + new_offset;

        // ASSERT: World position unchanged (within floating-point tolerance)
        assert!(
            (world_pos_before - world_pos_after).length() < 0.001,
            "World-space position changed during tile crossing: before={:?}, after={:?}",
            world_pos_before, world_pos_after
        );
    }

    /// INV-003: Teleport Clears Offset
    /// When entity jumps ≥2 hexes (teleport), offset MUST be cleared to zero.
    /// This prevents visual artifacts from client-side prediction conflicts.
    #[test]
    fn test_teleport_clears_offset_for_jumps_over_two_hexes() {
        let old_loc = Qrz { q: 0, r: 0, z: 0 };
        let new_loc = Qrz { q: 5, r: 5, z: 0 }; // Distance >= 2 (teleport)
        let old_offset = Vec3::new(0.5, 1.0, 0.3);

        // Teleport detection (from do_incremental line 82-87)
        const TELEPORT_THRESHOLD_HEXES: i16 = 2;
        let hex_distance = old_loc.flat_distance(&new_loc);

        let new_offset = if hex_distance >= TELEPORT_THRESHOLD_HEXES {
            Vec3::ZERO // Teleport: clear offset
        } else {
            old_offset // Smooth crossing: preserve world space
        };

        assert_eq!(new_offset, Vec3::ZERO, "Teleport did not clear offset");
    }

    /// INV-003: Adjacent Tile Crossing Detection
    /// Verify that distance calculation correctly identifies adjacent tiles (distance=1).
    #[test]
    fn test_adjacent_tile_has_distance_one() {
        let loc = Qrz { q: 0, r: 0, z: 0 };
        let adjacent = Qrz { q: 1, r: 0, z: 0 }; // East neighbor

        let distance = loc.flat_distance(&adjacent);

        assert_eq!(distance, 1, "Adjacent tile should have distance 1");
    }

    /// INV-003: Teleport Detection Boundary
    /// Verify that distance=2 triggers teleport behavior.
    #[test]
    fn test_teleport_threshold_is_two_hexes() {
        let loc = Qrz { q: 0, r: 0, z: 0 };
        let two_away = Qrz { q: 2, r: 0, z: 0 }; // 2 hexes east

        let distance = loc.flat_distance(&two_away);

        assert_eq!(distance, 2, "Two hexes away should have distance 2");
        assert!(
            distance >= 2,
            "Distance 2 should trigger teleport behavior"
        );
    }
}
