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
        Option<&mut crate::common::components::targeting_state::TargetingState>)>,
    map: Res<Map>,
    buffers: Res<crate::common::resources::InputQueues>,
) {
    for &message in reader.read() {
        let Do { event: Event::Incremental { ent, component } } = message else { continue; };

        // Handle Component::Projectile separately - projectiles don't have all the components in the main query
        if let Component::Projectile(projectile) = component {
            commands.entity(ent).insert(projectile);
            continue;
        }

        let Ok((o_loc, o_offset, o_heading, o_keybits, o_behaviour, o_health, o_stamina, o_mana, o_combat_state, o_player_controlled, o_targeting_state)) = query.get_mut(ent) else {
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
                    // Local players only: preserve world-space positions for smooth visual transitions
                    // Convert all offset fields to world positions, then re-express in new tile's coordinate system
                    let state_world = map.convert(**loc0) + offset0.state;
                    let prev_world = map.convert(**loc0) + offset0.prev_step;
                    let step_world = map.convert(**loc0) + offset0.step;

                    let new_tile_center = map.convert(*loc);
                    offset0.state = state_world - new_tile_center;
                    offset0.prev_step = prev_world - new_tile_center;
                    offset0.step = step_world - new_tile_center;
                } else {
                    // Remote players and NPCs: use heading-based positioning for smooth transitions
                    // Calculate current visual position to preserve for interpolation
                    // Use actual interpolated position (not just step target) in case update arrives mid-interpolation
                    let current_interp_fraction = if offset0.interp_duration > 0.0 {
                        (offset0.interp_elapsed / offset0.interp_duration).min(1.0)
                    } else {
                        1.0
                    };
                    let current_visual_offset = offset0.prev_step.lerp(offset0.step, current_interp_fraction);
                    let prev_world = map.convert(**loc0) + current_visual_offset;

                    let new_tile_center = map.convert(*loc);

                    // Calculate heading-based target position
                    let Some(ref heading0) = o_heading else { panic!("no heading for remote player/NPC") };
                    let target_offset = if ***heading0 != default() {
                        let heading_neighbor_world = map.convert(*loc + ***heading0);
                        let direction = heading_neighbor_world - new_tile_center;
                        (direction * HERE).xz()
                    } else {
                        Vec2::ZERO
                    };

                    let target_offset_3d = Vec3::new(target_offset.x, 0.0, target_offset.y);

                    // Set up interpolation: prev_step (current visual) -> step (heading target)
                    offset0.prev_step = prev_world - new_tile_center;
                    offset0.step = target_offset_3d;
                    offset0.state = target_offset_3d;

                    // Calculate expected travel time based on distance and movement speed
                    // Used for time-based interpolation of remote players and NPCs
                    let distance = (offset0.step - offset0.prev_step).length();
                    const MOVEMENT_SPEED: f32 = 0.005; // units per millisecond
                    const MIN_INTERP_DISTANCE: f32 = 0.01; // Don't interpolate tiny movements (spawns/teleports)

                    if distance > MIN_INTERP_DISTANCE {
                        offset0.interp_duration = distance / MOVEMENT_SPEED / 1000.0; // convert ms to seconds
                        offset0.interp_elapsed = 0.0;
                    } else {
                        // Distance too small - snap instantly (no interpolation)
                        offset0.interp_duration = 0.0;
                        offset0.interp_elapsed = 0.0;
                    }
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

                // Only update offset.state for remote PLAYERS on heading change
                // NPCs get Heading+Loc updates together, so skip standalone heading adjustments
                if !is_local && is_player {
                    if let Some(mut offset0) = o_offset {
                        let Some(ref loc0) = o_loc else { panic!("no loc") };
                        let tile_center = map.convert(***loc0);

                        let target_offset = if *heading != default() {
                            let heading_neighbor_world = map.convert(***loc0 + *heading);
                            let direction = heading_neighbor_world - tile_center;
                            (direction * HERE).xz()
                        } else {
                            Vec2::ZERO
                        };

                        offset0.state = Vec3::new(target_offset.x, 0.0, target_offset.y);
                    }
                }
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
            Component::TargetingState(targeting_state) => {
                if let Some(mut targeting_state0) = o_targeting_state {
                    *targeting_state0 = targeting_state;
                } else {
                    commands.entity(ent).insert(targeting_state);
                }
            }
            Component::Projectile(_) => {
                // Handled at top of function before query (projectiles don't match main query)
                unreachable!("Component::Projectile should be handled before query");
            }
            _ => {}
        }
    }
}
