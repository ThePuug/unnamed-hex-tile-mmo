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
        Option<&mut CombatState>)>,
    map: Res<Map>,
    buffers: Res<crate::common::resources::InputQueues>,
) {
    for &message in reader.read() {
        let Do { event: Event::Incremental { ent, component } } = message else { continue; };
        let Ok((o_loc, o_offset, o_heading, o_keybits, o_behaviour, o_health, o_stamina, o_mana, o_combat_state)) = query.get_mut(ent) else {
            // Entity might have been despawned - skip this update
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

                if is_local {
                    // Local player: preserve world-space positions for smooth visual transitions
                    // Convert all offset fields to world positions, then re-express in new tile's coordinate system
                    let state_world = map.convert(**loc0) + offset0.state;
                    let prev_world = map.convert(**loc0) + offset0.prev_step;
                    let step_world = map.convert(**loc0) + offset0.step;

                    let new_tile_center = map.convert(*loc);
                    offset0.state = state_world - new_tile_center;
                    offset0.prev_step = prev_world - new_tile_center;
                    offset0.step = step_world - new_tile_center;
                } else {
                    // Remote player: preserve only visual positions for smooth transitions
                    // State should be the heading-based position for remote players
                    let prev_world = map.convert(**loc0) + offset0.prev_step;
                    let step_world = map.convert(**loc0) + offset0.step;

                    let new_tile_center = map.convert(*loc);

                    // Calculate heading-based target position
                    let Some(ref heading0) = o_heading else { panic!("no heading for remote player") };
                    let target_offset = if ***heading0 != default() {
                        let heading_neighbor_world = map.convert(*loc + ***heading0);
                        let direction = heading_neighbor_world - new_tile_center;
                        (direction * HERE).xz()
                    } else {
                        Vec2::ZERO
                    };

                    offset0.state = Vec3::new(target_offset.x, 0.0, target_offset.y);
                    offset0.prev_step = prev_world - new_tile_center;
                    offset0.step = step_world - new_tile_center;
                }

                *loc0 = loc;
            }
            Component::Heading(heading) => {
                let Some(mut heading0) = o_heading else { continue; };

                *heading0 = heading;

                // Update offset.state for remote players when heading changes
                let is_local = buffers.get(&ent).is_some();
                if !is_local {
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
                let Some(mut behaviour0) = o_behaviour else { continue };
                *behaviour0 = behaviour;
            }
            Component::KeyBits(keybits) => {
                let Some(mut keybits0) = o_keybits else { continue; };
                *keybits0 = keybits;
            }
            Component::Health(health) => {
                if let Some(mut health0) = o_health {
                    *health0 = health;
                } else {
                    // Entity doesn't have Health yet (e.g., NPCs on spawn) - insert it
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
            _ => {}
        }
    }
}
