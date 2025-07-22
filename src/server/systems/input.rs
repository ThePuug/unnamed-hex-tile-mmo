use bevy::prelude::*;
use qrz::Convert;

use crate::{ 
    common::{
        components::{ heading::*, keybits::*, offset::*, * }, 
        message::{Event, *}, 
        plugins::nntree::NearestNeighbor, 
        systems::gcd::GcdType
    }, *
};

pub fn generate_input(
    mut writer: EventWriter<Do>,
    mut buffers: ResMut<InputQueues>,
    dt: Res<Time>,
) {
    for (&ent0, buffer) in buffers.iter_mut() {
        let mut dt0 = (dt.delta_secs() * 1000.) as u16;
        
        match buffer.queue.back_mut() {
            Some(Event::Input { dt, .. }) => {
                *dt += dt0.saturating_sub(buffer.accumulator_in);
                buffer.accumulator_in = 0;
            }
            _ => unreachable!(),
        }

        while let Some(Event::Input { ent, key_bits, dt, seq }) = buffer.queue.pop_front() {
            if ent0 != ent {
                error!("received input for {ent} from queue for {ent0}");
                continue;
            }

            if buffer.queue.is_empty() || dt > dt0 {
                dt0 = dt.clamp(0,dt0);
                buffer.accumulator_out += dt0;
                buffer.queue.push_front(Event::Input { ent, key_bits, dt: dt-dt0, seq });
                writer.write(Do { event: Event::Input { ent, key_bits, dt: dt0, seq: 0 } });
                break;
            }
            writer.write(Do { event: Event::Input { ent, key_bits, dt, seq: 0 } });
            writer.write(Do { event: Event::Input { ent, key_bits, dt: buffer.accumulator_out+dt, seq } });
            dt0 -= dt;
            buffer.accumulator_out = 0;
        }
    }
}

pub fn try_input(
    mut reader: EventReader<Try>,
    mut query: Query<&mut KeyBits>,
    mut buffers: ResMut<InputQueues>,
    dt0: Res<Time<Fixed>>,
) {
    for &message in reader.read() {
        if let Try { event: Event::Input { ent, key_bits, seq, .. } } = message {
            if let Some(buffer) = buffers.get_mut(&ent) {
                // add overstep difference to the previous accumulating input
                let dt0 = dt0.overstep().as_millis() as u16 - buffer.accumulator_in;
                buffer.accumulator_in += dt0;
                match buffer.queue.back_mut() {
                    Some(Event::Input { dt, .. }) => {
                        *dt += dt0;
                    }
                    _ => unreachable!(),
                }

                *query.get_mut(ent).unwrap() = key_bits;
                buffer.queue.push_back(Event::Input { ent, key_bits, dt: 0, seq });
            } else {
                warn!("no queue for {ent}");
            }
        }
    }
}

pub fn do_input(
    mut reader: EventReader<Do>,
    mut query: Query<(&Loc, &Heading, &mut Offset, &mut AirTime)>,
    map: Res<Map>,
) {
    for &message in reader.read() {
        if let Do { event: Event::Input { ent, key_bits, dt, seq, .. } } = message {
            if seq != 0 { continue; }
            let (&loc, &heading, mut offset, mut air_time) = query.get_mut(ent).unwrap();
            (offset.state, air_time.state) = physics::apply(key_bits, dt as i16, *loc, heading, offset.state, air_time.state, &map);
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
                            commands.spawn((
                                typ,
                                Loc::new(*loc + *heading),
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
    for (ent, &qrz0, &offset) in &mut query {
        let px = map.convert(*qrz0);
        let qrz = map.convert(px + offset.state);
        if *qrz0 != qrz { 
            let attr = Attribute::Qrz { qrz }; 
            writer.write(Try { event: Event::Incremental { ent, attr } }); 
        }
    }
}
