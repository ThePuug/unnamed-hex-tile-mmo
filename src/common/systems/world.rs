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
) {
    for &message in reader.read() {
        let Do { event: Event::Incremental { ent, component } } = message else { continue; };
        let (o_loc, o_offset, o_heading, o_keybits, o_behaviour) = query.get_mut(ent).unwrap();
        match component {
            Component::Loc(loc) => {
                let Some(mut loc0) = o_loc else { continue; };
                let Some(mut offset0) = o_offset else { continue; };

                offset0.state = map.convert(**loc0) + offset0.state - map.convert(*loc);
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
