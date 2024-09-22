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
    mut query: Query<(&mut Hx, &mut Heading)>,
) {
    for &message in reader.read() {
        match message {
            Try { event: Event::Move { ent, hx, heading } } => {
                if let Ok((mut hx0, mut heading0)) = query.get_mut(ent) {
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
    mut writer: EventWriter<Do>,
    map: Res<TerrainedMap>,
    time: Res<Time>,
    mut query: Query<(Entity, &mut Hx, &mut Offset, &Heading)>,
) {
    for (ent, mut hx0, mut offset0, &heading) in query.iter_mut() {
        let (floor, _) = map.find(*hx0, 3);
        let px = Vec3::from(*hx0) + offset0.0;

        let mut hx = Hx::from(px);
        let mut offset = *offset0;
        let hxc = Vec3::from(hx);
        if hx != *hx0 { offset.0 = px - hxc; }
    
        // fall
        let d_fall = time.delta_seconds() * 0.5;
        if floor.is_none() || hx.z as f32 - d_fall > floor.unwrap().z as f32 + 1. { offset.0.z -= d_fall; } 
        else {
            hx.z = floor.unwrap().z + 1;
            offset.0.z = 0.;
        }

        if *hx0 != hx { 
            *hx0 = hx;
            writer.send(Do { event: Event::Move { ent, hx, heading } }); 
        }
        *offset0 = offset;
    }
}
