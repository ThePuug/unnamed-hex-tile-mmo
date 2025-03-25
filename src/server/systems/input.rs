use bevy::prelude::*;

use crate::{ *,
    common::{
        components::{
            heading::*,
            hx::*,
            keybits::*,
            offset::*,
        },
        message::{*, Event},
    },
};

pub fn generate_input(
    mut writer: EventWriter<Do>,
    mut buffers: ResMut<InputQueues>,
    dt: Res<Time>,
) {
    for (&ent0, buffer) in buffers.0.iter_mut() {
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
                writer.send(Do { event: Event::Input { ent, key_bits, dt: dt0, seq: 0 } });
                break;
            }
            writer.send(Do { event: Event::Input { ent, key_bits, dt: dt, seq: 0 } });
            writer.send(Do { event: Event::Input { ent, key_bits, dt: buffer.accumulator_out+dt, seq } });
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
            if let Some(buffer) = buffers.0.get_mut(&ent) {
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
    mut query: Query<(&Hx, &Heading, &mut Offset, &mut AirTime)>,
    map: Res<Map>,
) {
    for &message in reader.read() {
        if let Do { event: Event::Input { ent, key_bits, dt, seq, .. } } = message {
            if seq != 0 { continue; }
            let (&hx, &heading, mut offset, mut air_time) = query.get_mut(ent).unwrap();
            (offset.state, air_time.state) = apply(key_bits, dt as i16, hx, heading, offset.state, air_time.state, &map);
        }
    }
}

pub fn try_gcd(
    mut reader: EventReader<Try>,
    mut writer: EventWriter<Do>,
) {
    for &message in reader.read() {
        if let Try { event: Event::Gcd { ent, typ, .. } } = message {
            debug!("try gcd {ent} {:?}", typ);
            writer.send(Do { event: Event::Gcd { ent, typ }});
        }
    }
}

pub fn do_gcd(
    mut reader: EventReader<Do>,
) {
    for &message in reader.read() {
        if let Do { event: Event::Gcd { ent, typ } } = message {
            debug!("do gcd {ent} {:?}", typ);
        }
    }
}

pub fn update_hx(
    mut writer: EventWriter<Try>,
    mut query: Query<(Entity, &Hx, &Offset), Changed<Offset>>,
) {
    for (ent, &hx0, &offset) in &mut query {
        let px = Vec3::from(hx0);
        let hx = Hx::from(px + offset.state);
        if hx0 != hx { 
            let attr = Attribute::Hx { hx }; 
            writer.send(Try { event: Event::Incremental { ent, attr } }); 
        }
    }
}
