use bevy::prelude::*;
use qrz::Convert;

use crate::common::{
    components::{behaviour::*, heading::*, keybits::*, offset::*, *}, 
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
    mut reader: EventReader<Do>,
    mut writer: EventWriter<Try>,
    mut query: Query<(
        Option<&mut Loc>,
        Option<&mut Offset>,
        Option<&mut Heading>,
        Option<&mut KeyBits>,
        Option<&mut Behaviour>)>,
    map: Res<Map>,
    buffers: Res<crate::common::resources::InputQueues>,
) {
    for &message in reader.read() {
        let Do { event: Event::Incremental { ent, component } } = message else { continue; };
        let (o_loc, o_offset, o_heading, o_keybits, o_behaviour) = query.get_mut(ent).unwrap();
        match component {
            Component::Loc(loc) => {
                let Some(mut loc0) = o_loc else { continue; };
                let Some(mut offset0) = o_offset else { continue; };

                let is_local = buffers.get(&ent).is_some();

                if is_local {
                    // Local player: crossing tile boundaries, preserve world-space positions
                    // for smooth interpolation across the boundary

                    // Convert prev_step and step to world positions
                    let prev_world = map.convert(**loc0) + offset0.prev_step;
                    let step_world = map.convert(**loc0) + offset0.step;

                    // Calculate the adjustment for the confirmed state
                    let adjustment = map.convert(**loc0) + offset0.state - map.convert(*loc);

                    // Express prev_step and step in the new tile's coordinate system
                    offset0.prev_step = prev_world - map.convert(*loc);
                    offset0.step = step_world - map.convert(*loc);
                    offset0.state = adjustment;
                } else {
                    // Remote player: received Loc update from server
                    // Set step to the offset from old position to new Loc, then interpolate toward zero

                    // Calculate where the remote player currently is in world space
                    let current_world_pos = map.convert(**loc0) + offset0.step;

                    // Set step to be the offset from the new Loc to that world position
                    offset0.prev_step = offset0.step;
                    offset0.step = current_world_pos - map.convert(*loc);
                    offset0.state = Vec3::ZERO; // Remote players aim for center of their Loc
                }

                *loc0 = loc;

                writer.write(Try { event: Event::Discover { ent, qrz: *loc } });
                let Some(heading0) = o_heading else { panic!("no heading") };
                if **heading0 != default() {
                    for qrz in loc0.fov(&heading0, 10) {
                        writer.write(Try { event: Event::Discover { ent, qrz } });
                    }
                }
            }
            Component::Heading(heading) => {
                let Some(mut heading0) = o_heading else { continue; };

                *heading0 = heading;

                let Some(loc0) = o_loc else { panic!("no loc") };
                if **heading0 != default() {
                    for qrz in loc0.fov(&heading0, 10) {
                        writer.write(Try { event: Event::Discover { ent, qrz } });
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
            _ => {}
        }
    }
}
