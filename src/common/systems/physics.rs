use crate::{ *,
    common::{
        message::{*, Event},
        components::{
            keybits::*,
            hx::*,
        },
        // resources::map::*,
    },
};

pub fn do_input(
    // mut commands: Commands,
    mut reader: EventReader<Do>,
    map: Res<Map>,
    mut query: Query<(Entity, &Heading, &Hx, &mut Offset, Option<&mut AirTime>)>,
) {
    for &message in reader.read() {
        match message {
            Do { event: Event::Input { ent, key_bits, dt } } => {
                if let Ok((_ent, &heading, &hx0, mut offset0, _air_time)) = query.get_mut(ent) {
                    let mut offset = offset0.0;

                    let px = Vec3::from(hx0);
                    let curr = px + offset;
                    let curr_hx = Hx::from(curr);
                    let curr_px = Vec3::from(curr_hx);

                    // let (hx_floor, _) = map.find(curr_hx + Hx{ z: 1, ..default() }, -10);
                    // 
                    // if let Some(mut air_time) = air_time { 
                    //     if air_time.0 > 0 { air_time.0 -= dt as i16; }
                    //     else {
                    //         let z_fall = dt as f32 / 100.;
                    //         if hx_floor.is_none() 
                    //             || curr_hx.z as f32 + offset.0.z - z_fall > hx_floor.unwrap().z as f32 + 1. { 
                    //             offset.0.z -= z_fall;
                    //         } else {
                    //             offset.0.z = hx_floor.unwrap().z as f32 + 1. - curr_hx.z as f32;
                    //             commands.entity(ent).remove::<AirTime>();
                    //         }
                    //     }
                    // }
                    
                    let far = curr_px.lerp(Vec3::from(curr_hx + heading.0), 1.25);
                    let near = px.lerp(Vec3::from(hx0 + heading.0), 0.25);
                    let next = map.get(Hx::from(far));
                    let target = 
                        if next == Entity::PLACEHOLDER && key_bits.any_pressed([KB_HEADING_Q, KB_HEADING_R]) { far }
                        else { near };

                    let dist = curr.distance(target);
                    let ratio = 0_f32.max((dist - dt as f32 / 10.) / dist);
                    offset = curr.lerp(target, 1. - ratio) - px;                    

                    offset0.0 = offset;
                }
            }
            _ => {}
        }
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
                    if *hx0 != hx { 
                        *hx0 = hx; 
                        *heading0 = heading;
                    }
                    else if *heading0 != heading { *heading0 = heading; }
                }
            }
            _ => {}
        }
    }
}

pub fn update_headings(
    mut writer: EventWriter<Try>,
    mut query: Query<(Entity, &Hx, &Heading), Changed<Heading>>,
) {
    for (ent, &hx, &heading) in &mut query {
        writer.send(Try { event: Event::Move { ent, hx, heading } });
    }
}

pub fn update_offsets(
    mut writer: EventWriter<Try>,
    mut query: Query<(Entity, &Hx, &Heading, &Offset), Changed<Offset>>,
) {
    for (ent, &hx0, &heading, &offset) in &mut query {
        let px = Vec3::from(hx0);
        let hx = Hx::from(px + offset.0);
        if hx0 != hx { writer.send(Try { event: Event::Move { ent, hx, heading } }); }
    }
}
