use crate::{ *,
    common::{
        message::{*, Event},
        components::{
            keybits::*,
            hx::*,
        },
        resources::map::*,
    },
};

pub fn update_positions(
    mut commands: Commands,
    mut writer: EventWriter<Try>,
    map: Res<Map>,
    time: Res<Time>,
    mut query: Query<(Entity, &Heading, &mut Hx, &mut Offset, Option<&mut AirTime>, Option<&KeyBits>)>,
) {
    for (ent, &heading, mut hx0, mut offset0, air_time, key_bits) in query.iter_mut() {
        let px = Vec3::from(*hx0);
        let curr = px + offset0.0;

        let (floor, _) = map.find(*hx0, 10);
        let mut offset = *offset0;
    
        if let Some(mut air_time) = air_time { 
            if air_time.0 > 0. { air_time.0 -= time.delta_seconds(); }
            else {
                let d_fall = time.delta_seconds() * 10.;
                if floor.is_none() 
                    || px.z + offset.0.z - d_fall > floor.unwrap().z as f32 + 1. { 
                    offset.0.z -= d_fall; 
                } else {
                    offset.0.z = floor.unwrap().z as f32 - px.z + 1.;
                    commands.entity(ent).remove::<AirTime>();
                }
            }
        }

        let target = px.lerp(Vec3::from(*hx0 + heading.0),
            if key_bits.is_some() && (*(key_bits.unwrap()) & (KB_HEADING_Q | KB_HEADING_R)) { 1.25 }
            else { 0.25 }); 
        
        let dist = curr.distance(target);
        let ratio = 0_f32.max((dist - 100_f32 * time.delta_seconds()) / dist);
        offset0.0 = curr.lerp(target, 1. - ratio) - px;

        let hx = Hx::from(px);
        if *hx0 != hx { 
            trace!("Moving {:?} from {:?} to {:?}", ent, *hx0, hx);
            offset.0 = px - Vec3::from(hx);
            *hx0 = hx;
            writer.send(Try { event: Event::Move { ent, hx, heading } }); 
        }
        *offset0 = offset;
    }
}

pub fn do_move(
    mut reader: EventReader<Do>,
    mut query: Query<(&mut Hx, &mut Offset, &mut Heading)>,
) {
    for &message in reader.read() {
        match message {
            Do { event: Event::Move { ent, hx, heading } } => {
                if let Ok((mut hx0, mut offset0, mut heading0)) = query.get_mut(ent) {
                    *offset0 = Offset(Vec3::from(*hx0) + offset0.0 - Vec3::from(hx));
                    *hx0 = hx; 
                    *heading0 = heading;
                }
            }
            _ => {}
        }
    }
}
