use bevy::prelude::*;
use qrz::Convert;

use crate::{ 
    common::{
        components::{ behaviour::*, entity_type::*, heading::*, offset::*, * }, 
        message::{Component, Event, *}, 
        plugins::nntree::*, 
        systems::gcd::*
    }, *
};

pub fn try_input(
    mut reader: EventReader<Try>,
    mut writer: EventWriter<Do>,
) {
    for &message in reader.read() {
        let Try { event } = message;
        let Event::Input { .. } = event else { continue };
        writer.write(Do { event });
    }
}

pub fn send_input(
    lobby: Res<Lobby>,
    mut conn: ResMut<RenetServer>,
    mut buffers: ResMut<InputQueues>,
) {
    for (ent, buffer) in buffers.iter_mut() {
        while buffer.queue.len() > 1 {
            let event = buffer.queue.pop_back().unwrap();
            let message = bincode::serde::encode_to_vec(
                Do { event }, 
                bincode::config::legacy()).unwrap();
            conn.send_message(*lobby.get_by_right(&ent).unwrap(), DefaultChannel::ReliableOrdered, message);
        }
    }
}

pub fn try_gcd(
    mut commands: Commands,
    mut reader: EventReader<Try>,
    mut writer: EventWriter<Do>,
    query: Query<(&Loc, &Heading)>,
) {
    for &message in reader.read() {
        if let Try { event: Event::Gcd { ent, typ, .. } } = message {
            match typ {
                GcdType::Spawn(typ) => {
                    let (&loc, &heading) = query.get(ent).expect(&format!("missing loc/heading for entity {ent}"));
                    let ent = match typ {
                        EntityType::Actor(_) => {
                            let qrz = *loc + *heading;
                            commands.spawn((
                                typ,
                                Loc::new(qrz),
                                Behaviour::Wander(Wander { qrz }),
                                NearestNeighbor::default(),
                            )).id()
                        },
                        EntityType::Decorator(_) => {
                            Entity::PLACEHOLDER
                        }
                    };
                    writer.write(Do { event: Event::Spawn { ent, typ, qrz: *loc + *heading }});
                }
                _ => unreachable!()
            }
        }
    }
}

pub fn update_qrz(
    mut writer: EventWriter<Try>,
    mut query: Query<(Entity, &Loc, &Offset), Changed<Offset>>,
    map: Res<Map>,
) {
    for (ent, &loc0, &offset) in &mut query {
        let px = map.convert(*loc0);
        let qrz = map.convert(px + offset.state);
        if *loc0 != qrz { 
            let component = Component::Loc(Loc::new(qrz));
            writer.write(Try { event: Event::Incremental { ent, component } }); 
        }
    }
}
