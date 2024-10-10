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
    // map: Res<Map>,
    mut query: Query<(Entity, &Heading, &Hx, &mut Offset, Option<&mut AirTime>)>,
) {
    for &message in reader.read() {
        match message {
            Do { event: Event::Input { ent, key_bits, dt } } => {
                if let Ok((_ent, &heading, &hx0, mut offset0, _air_time)) = query.get_mut(ent) {
                    // let mut offset = *offset0;

                    let px = Vec3::from(hx0);
                    let curr = px + offset0.0;
                    let curr_hx = Hx::from(curr);
                    let curr_px = Vec3::from(curr_hx).xy();

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
                    
                    let target = 
                        if key_bits.any_pressed([KB_HEADING_Q, KB_HEADING_R]) { 
                            curr_px.lerp(Vec3::from(curr_hx + heading.0).xy(), 1.25)
                        } else { 
                            px.xy().lerp(Vec3::from(hx0 + heading.0).xy(), 0.25)
                        };
                    
                    let dist = curr.xy().distance(target);
                    let ratio = 0_f32.max((dist - dt as f32 / 10.) / dist);
                    offset0.0 = (curr.xy().lerp(target, 1. - ratio) - px.xy()).extend(offset0.0.z);
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
        writer.send(Try { event: Event::Discover { hx: hx + heading.0 + Hx { q: 0, r: 0, z: -1 } } });
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
