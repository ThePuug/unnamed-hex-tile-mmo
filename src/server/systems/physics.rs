use crate::{ *,
    common::{
        message::{*, Event},
        components::hx::*,
    },
    server::resources::map::*,
};

pub fn try_move(
    mut reader: EventReader<Try>,
    mut writer: EventWriter<Do>,
    mut query: Query<(&mut Hx, &mut Heading, Option<&mut AirTime>)>,
) {
    for &message in reader.read() {
        match message {
            Try { event: Event::Move { ent, mut hx, heading } } => {
                if let Ok((mut hx0, mut heading0, air_time)) = query.get_mut(ent) {
                    hx.z = match air_time {
                        Some(..) => hx0.z,
                        None => hx.z,
                    };
                    if *hx0 != hx || heading0.0 != heading.0 {
                        writer.send(Do { event: Event::Move { ent, hx, heading } });
                    }
                    if *hx0 != hx { *hx0 = hx; }
                    if heading0.0 != heading.0 { *heading0 = heading; }
                }
            }
            _ => {}
        }
    }
}

pub fn update_positions(
    mut commands: Commands,
    mut writer: EventWriter<Do>,
    map: Res<TerrainedMap>,
    time: Res<Time>,
    mut query: Query<(Entity, &mut Hx, &mut Offset, &Heading, Option<&mut AirTime>)>,
) {
    for (ent, mut hx0, mut offset0, &heading, air_time) in query.iter_mut() {
        let (floor, _) = map.find(*hx0, 3);
        let mut offset = *offset0;
    
        if let Some(mut air_time) = air_time { 
            if air_time.0 > 0. {air_time.0 -= time.delta_seconds(); }
            else { commands.entity(ent).remove::<AirTime>(); }
        } else {
            let d_fall = time.delta_seconds() * 10.;
            let z = Vec3::from(*hx0).z;
            if floor.is_none() 
                || z + offset.0.z - d_fall > floor.unwrap().z as f32 + 1. { 
                offset.0.z -= d_fall; 
            } else {
                offset.0.z = floor.unwrap().z as f32 - z + 1.;
            }
        }

        let px = Vec3::from(*hx0) + offset.0;
        let hx = Hx::from(px);
        if *hx0 != hx { 
            offset.0 = px - Vec3::from(hx);
            *hx0 = hx;
            writer.send(Do { event: Event::Move { ent, hx, heading } }); 
        }
        *offset0 = offset;
    }
}
