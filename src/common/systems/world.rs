use bevy::prelude::*;
use qrz::Convert;

use crate::common::{
    components::{behaviour::*, heading::*, keybits::*, offset::*, resources::*, *},
    message::{Component, Event, *},
    resources::map::*
};

pub fn try_incremental(
    mut reader: EventReader<Try>,
    mut writer: EventWriter<Do>,
) {
    for &message in reader.read() {
        if let Try { event: Event::Incremental { ent, component } } = message {
            writer.write(Do { event: Event::Incremental { ent, component }});
        }
    }
}

/// Process incremental component updates from server.
/// Updates existing components or inserts them if missing (late-binding for NPCs).
/// Exceptions: Loc/Offset/Heading require all related components and skip if dependencies missing.
pub fn do_incremental(
    mut commands: Commands,
    mut reader: EventReader<Do>,
    mut query: Query<(
        Option<&mut Loc>,
        Option<&mut Offset>,
        Option<&mut Heading>,
        Option<&mut KeyBits>,
        Option<&mut Behaviour>,
        Option<&mut Health>,
        Option<&mut Stamina>,
        Option<&mut Mana>,
        Option<&mut CombatState>,
        Option<&mut PlayerControlled>,
        Option<&mut crate::common::components::tier_lock::TierLock>,
        Option<&crate::common::components::movement_prediction::MovementPrediction>)>,
    map: Res<Map>,
    buffers: Res<crate::common::resources::InputQueues>,
) {
    for &message in reader.read() {
        let Do { event: Event::Incremental { ent, component } } = message else { continue; };

        let Ok((o_loc, o_offset, o_heading, o_keybits, o_behaviour, o_health, o_stamina, o_mana, o_combat_state, o_player_controlled, o_tier_lock, o_prediction)) = query.get_mut(ent) else {
            // Entity might have been despawned
            continue;
        };
        match component {
            Component::Loc(loc) => {
                let Some(mut loc0) = o_loc else {
                    continue;
                };
                let Some(mut offset0) = o_offset else {
                    continue;
                };

                // Check if this is a local player (has input buffer) or remote player
                let is_local = buffers.get(&ent).is_some();
                let is_player = o_player_controlled.is_some();

                // On server: skip offset adjustments for NPCs (server is authoritative for NPC physics)
                // On client: process all Loc updates (both players and NPCs come from server)
                // We detect "server authoritative NPC" by: not a player, and Loc is already up-to-date
                // (server's actor::update already set Loc and adjusted offset before sending this event)
                if !is_player && **loc0 == *loc {
                    // NPC Loc update on server - offset was already adjusted by server/actor::update
                    continue;
                }

                if is_local {
                    // Teleport detection: Check if this Loc update is a smooth tile crossing or a teleport.
                    //
                    // Local player movement is client-predicted and crosses adjacent tiles (distance=1).
                    // Server-initiated teleports (Lunge, dev console) jump multiple hexes (distance>=2).
                    //
                    // We detect teleports to avoid preserving world-space offsets, which would cause
                    // the character to appear stuck at the old position (client prediction conflict).
                    const TELEPORT_THRESHOLD_HEXES: i16 = 2; // Jumps of 2+ hexes are non-adjacent (teleports)

                    let hex_distance = loc0.flat_distance(&loc);

                    if hex_distance >= TELEPORT_THRESHOLD_HEXES {
                        // Teleport: Clear offset for instant visual snap (no interpolation)
                        // Server has moved us multiple hexes non-adjacently
                        offset0.state = Vec3::ZERO;
                        offset0.step = Vec3::ZERO;
                        offset0.prev_step = Vec3::ZERO;
                        offset0.interp_elapsed = 0.0;
                        offset0.interp_duration = 0.0;
                    } else {
                        // Smooth tile crossing: Preserve world-space position for visual continuity
                        // Convert offset from old tile's coordinate system to new tile's
                        let state_world = map.convert(**loc0) + offset0.state;
                        let prev_world = map.convert(**loc0) + offset0.prev_step;
                        let step_world = map.convert(**loc0) + offset0.step;

                        let new_tile_center = map.convert(*loc);
                        offset0.state = state_world - new_tile_center;
                        offset0.prev_step = prev_world - new_tile_center;
                        offset0.step = step_world - new_tile_center;
                    }
                } else {
                    // Calculate current visual position from interpolation
                    // (offset.state is authoritative combat position, NOT visual position)
                    let current_interp_fraction = if offset0.interp_duration > 0.0 {
                        (offset0.interp_elapsed / offset0.interp_duration).min(1.0)
                    } else {
                        1.0  // Completed or no interpolation
                    };
                    let current_visual_offset = offset0.prev_step.lerp(offset0.step, current_interp_fraction);

                    // Calculate server's actual authoritative position (heading-adjusted on new tile)
                    let new_tile_center = map.convert(*loc);
                    let Some(ref heading0) = o_heading else { panic!("no heading for remote player/NPC") };
                    let target_offset = if ***heading0 != default() {
                        let heading_neighbor_world = map.convert(*loc + ***heading0);
                        let direction = heading_neighbor_world - new_tile_center;
                        (direction * HERE).xz()
                    } else {
                        Vec2::ZERO
                    };

                    // ADR-011: Validate movement prediction if it exists
                    if let Some(_prediction) = o_prediction {
                        // Clear prediction component
                        commands.entity(ent).remove::<crate::common::components::movement_prediction::MovementPrediction>();
                    }

                    // ADR-011: Preserve ongoing MovementIntent interpolation, just convert coordinate system
                    // When Loc arrives, the entity is crossing tile boundaries mid-interpolation
                    // We need to convert all offsets from old tile's coordinates to new tile's coordinates
                    let prev_world = map.convert(**loc0) + offset0.prev_step;
                    let step_world = map.convert(**loc0) + offset0.step;

                    // Convert to new tile's coordinate system (preserve interpolation!)
                    offset0.prev_step = prev_world - new_tile_center;
                    offset0.step = step_world - new_tile_center;

                    // Update authoritative combat position (already calculated above for gap measurement)
                    offset0.state = Vec3::new(target_offset.x, 0.0, target_offset.y);

                    // NOTE: Do NOT reset interp_duration/interp_elapsed - let MovementIntent interpolation continue!
                }

                *loc0 = loc;
            }
            Component::Heading(heading) => {
                let Some(mut heading0) = o_heading else { continue; };

                // Update offset.state for remote players when heading changes
                let is_local = buffers.get(&ent).is_some();
                let is_player = o_player_controlled.is_some();

                // On server: skip offset adjustments for NPCs (server is authoritative for NPC physics)
                // Check before assignment to detect if this is a server NPC update
                if !is_player && *heading0 == heading {
                    // NPC Heading update on server - heading already set, don't reset offset.state
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
                // PlayerControlled is a marker - if already present, no update needed
            }
            Component::TierLock(tier_lock) => {
                if let Some(mut tier_lock0) = o_tier_lock {
                    *tier_lock0 = tier_lock;
                } else {
                    commands.entity(ent).insert(tier_lock);
                }
            }
            Component::Returning(returning) => {
                // Always insert Returning (it's a marker component for leash regen prediction)
                commands.entity(ent).insert(returning);
            }
            _ => {}
        }
    }
}
