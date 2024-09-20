use bevy::prelude::*;

use crate::common::{
    components::{ *,
        message::Event,
    }, 
    hx::*,
};

pub fn handle_input(
    mut commands: Commands,
    mut reader: EventReader<Event>,
    query: Query<(&mut Pos, &mut Heading)>,
) {
    for &event in reader.read() {
        match event {
            Event::Input { ent, key_bits, dt } => {
                let (pos0, heading0) = query.get(ent).unwrap();
                let mut heading = *heading0;
                if key_bits & (KEYBIT_UP | KEYBIT_DOWN | KEYBIT_LEFT | KEYBIT_RIGHT) != default() {
                    if key_bits & KEYBIT_UP != default() {
                        if key_bits & KEYBIT_LEFT != default() || key_bits & KEYBIT_RIGHT == default()
                            &&(heading.0 == (Hx {q:-1, r: 0, z: -1})
                            || heading.0 == (Hx {q:-1, r: 1, z: -1})
                            || heading.0 == (Hx {q: 1, r:-1, z: -1})) { heading = Heading { 0:Hx {q:-1, r: 1, z: -1} }; }
                        else  { heading = Heading { 0:Hx {q: 0, r: 1, z: -1} }; }
                    } else if key_bits & KEYBIT_DOWN != default() {
                        if key_bits & KEYBIT_RIGHT != default() || key_bits & KEYBIT_LEFT == default()
                            &&(heading.0 == (Hx {q: 1, r: 0, z: -1})
                            || heading.0 == (Hx {q: 1, r:-1, z: -1})
                            || heading.0 == (Hx {q:-1, r: 1, z: -1})) { heading = Heading { 0:Hx {q: 1, r: -1, z: -1} }; }
                        else { heading = Heading { 0:Hx {q: 0, r:-1, z: -1} }; }
                    } 
                    else if key_bits & KEYBIT_RIGHT != default() { heading = Heading { 0:Hx {q: 1, r: 0, z: -1} }; }
                    else if key_bits & KEYBIT_LEFT != default() { heading = Heading { 0:Hx {q:-1, r: 0, z: -1} }; }
        
                    let target = pos0.hx + heading.0;
                    let px = Vec3::from(pos0.hx);
                    let delta = Vec3::from(target).xy() - (px + pos0.offset).xy();
                    let offset = pos0.offset + (delta.normalize_or_zero() * 100. * (dt as f32 / 1000.)).extend(0.);
                    let px_new = px + offset;

                    let mut entity = commands.entity(ent);
                    entity.insert(heading);
                    entity.insert(key_bits);
                    let hx = Hx::from(px_new);
                    if hx != pos0.hx {
                        entity.insert(Pos { 
                            hx, 
                            offset: px_new - Vec3::from(hx) 
                        });
                    } else { entity.insert(Pos { hx, offset }); }
                }
            }
            _ => {}
        }
    }
}
