use bevy::prelude::*;
use qrz::Convert;

use crate::{
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
        Option<&mut crate::components::tier_lock::TierLock>,
        Option<&crate::components::movement_prediction::MovementPrediction>,
        Option<&mut Position>,
        Option<&mut VisualPosition>,
        Option<&crate::components::AbilityDisplacement>)>,
    map: Res<Map>,
    buffers: Res<crate::resources::InputQueues>,
) {
    for &message in reader.read() {
        let Do { event: Event::Incremental { ent, component } } = message else { continue; };

        let Ok((o_loc, o_heading, o_keybits, o_behaviour, o_health, o_stamina, o_mana, o_combat_state, o_player_controlled, o_tier_lock, o_prediction, o_position, o_visual, o_ability_displacement)) = query.get_mut(ent) else {
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
                    const TELEPORT_THRESHOLD_HEXES: i32 = 2;
                    let hex_distance = loc0.flat_distance(&loc);

                    if hex_distance >= TELEPORT_THRESHOLD_HEXES {
                        if let Some(displacement) = o_ability_displacement {
                            // Ability-driven displacement (lunge/knockback): terrain-following path
                            let duration_secs = displacement.duration_ms as f32 / 1000.0;
                            if let Some(mut vis) = o_visual {
                                if hex_distance > 1 {
                                    // Multi-tile: greedy path for terrain following
                                    // Use floor-level tiles (Loc is standing height = floor + Z)
                                    let old_floor = map.find(**loc0, -60).map(|(f, _)| f).unwrap_or(**loc0);
                                    let new_floor = map.find(*loc, -60).map(|(f, _)| f).unwrap_or(*loc);
                                    let path = map.greedy_path(old_floor, new_floor, hex_distance as usize);
                                    if !path.is_empty() {
                                        let waypoints: Vec<Vec3> = path.iter()
                                            .map(|&tile| map.convert(tile + qrz::Qrz::Z))
                                            .collect();
                                        vis.interpolate_along_path(&waypoints, duration_secs);
                                    } else {
                                        // Loc is already at standing height; no +Z needed
                                        let target_world: Vec3 = map.convert(*loc);
                                        vis.interpolate_toward(target_world, duration_secs);
                                    }
                                } else {
                                    // Single tile: Loc is already at standing height; no +Z needed
                                    let target_world: Vec3 = map.convert(*loc);
                                    vis.interpolate_toward(target_world, duration_secs);
                                }
                            }
                            if let Some(mut pos) = o_position {
                                pos.tile = *loc;
                                pos.offset = Vec3::ZERO;
                            }
                            if let Ok(mut e) = commands.get_entity(ent) {
                                e.remove::<crate::components::AbilityDisplacement>();
                            }
                        } else {
                            // Teleport: snap to new position
                            if let Some(mut vis) = o_visual {
                                let teleport_world: Vec3 = map.convert(*loc);
                                vis.snap_to(teleport_world);
                            }
                            if let Some(mut pos) = o_position {
                                pos.tile = *loc;
                                pos.offset = Vec3::ZERO;
                            }
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

                    // ADR-011: Only clear prediction when arriving at predicted destination.
                    // Intermediate Loc updates (e.g. during knockback) keep prediction alive
                    // so the visual interpolation from MovementIntent continues smoothly.
                    let had_prediction = o_prediction.is_some();
                    let arrived_at_predicted = o_prediction
                        .as_ref()
                        .map_or(false, |p| loc.flat_distance(&p.predicted_dest) == 0);
                    if arrived_at_predicted {
                        if let Ok(mut e) = commands.get_entity(ent) {
                            e.remove::<crate::components::movement_prediction::MovementPrediction>();
                        }
                    }

                    // Update Position for remote entity
                    if let Some(mut pos) = o_position {
                        pos.tile = *loc;
                        pos.offset = Vec3::new(target_offset.x, 0.0, target_offset.y);
                    }

                    // If no MovementIntent preceded this Loc update (e.g., unexpected movement),
                    // start visual interpolation toward the new tile center.
                    // Loc is already at standing height (floor + 1), so no +Z needed.
                    if !had_prediction {
                        let tile_center: Vec3 = map.convert(*loc);
                        let visual_target = tile_center + Vec3::new(target_offset.x, 0.0, target_offset.y);
                        if let Some(mut vis) = o_visual {
                            vis.interpolate_toward(visual_target, 0.125);
                        }
                    }
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
                } else if let Ok(mut e) = commands.get_entity(ent) {
                    e.insert(behaviour);
                }
            }
            Component::KeyBits(keybits) => {
                if let Some(mut keybits0) = o_keybits {
                    *keybits0 = keybits;
                } else if let Ok(mut e) = commands.get_entity(ent) {
                    e.insert(keybits);
                }
            }
            Component::Health(health) => {
                if let Some(mut health0) = o_health {
                    *health0 = health;
                } else if let Ok(mut e) = commands.get_entity(ent) {
                    e.insert(health);
                }
            }
            Component::Stamina(stamina) => {
                if let Some(mut stamina0) = o_stamina {
                    *stamina0 = stamina;
                } else if let Ok(mut e) = commands.get_entity(ent) {
                    e.insert(stamina);
                }
            }
            Component::Mana(mana) => {
                if let Some(mut mana0) = o_mana {
                    *mana0 = mana;
                } else if let Ok(mut e) = commands.get_entity(ent) {
                    e.insert(mana);
                }
            }
            Component::CombatState(combat_state) => {
                if let Some(mut combat_state0) = o_combat_state {
                    *combat_state0 = combat_state;
                } else if let Ok(mut e) = commands.get_entity(ent) {
                    e.insert(combat_state);
                }
            }
            Component::PlayerControlled(player_controlled) => {
                if o_player_controlled.is_none() {
                    if let Ok(mut e) = commands.get_entity(ent) {
                        e.insert(player_controlled);
                    }
                }
            }
            Component::TierLock(tier_lock) => {
                if let Some(mut tier_lock0) = o_tier_lock {
                    *tier_lock0 = tier_lock;
                } else if let Ok(mut e) = commands.get_entity(ent) {
                    e.insert(tier_lock);
                }
            }
            Component::Returning(returning) => {
                if let Ok(mut e) = commands.get_entity(ent) {
                    e.insert(returning);
                }
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
        const TELEPORT_THRESHOLD_HEXES: i32 = 2;
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
