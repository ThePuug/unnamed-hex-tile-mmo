use bevy::prelude::*;

use crate::{
    common::message::{ Event, * },
    server::{resources::InputQueues, *},
};

pub fn tick(
    mut reader: EventReader<Tick>,
    mut writer: EventWriter<Do>,
    mut buffers: ResMut<InputQueues>,
    dt: Res<Time>,
) {
    for &message in reader.read() {
        let Tick { ent: ent0, behaviour: Behaviour::Controlled } = message else { continue; };
        let Some(buffer) = buffers.get_mut(&ent0) else { warn!("no queue for {ent0}"); continue; };
        let mut dt0 = (dt.delta_secs() * 1000.) as u16;
        
        let Event::Input { dt, ..} = buffer.queue.back_mut().expect("no input for {ent}!") else { unreachable!() };
        *dt += dt0.saturating_sub(buffer.accumulator_in);
        buffer.accumulator_in = 0;

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