use std::cmp::{max, min};

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

pub fn do_input(
    mut commands: Commands,
    mut reader: EventReader<Do>,
    map: Res<Map>,
    mut query: Query<(Entity, &Heading, &Hx, &mut Offset, Option<&mut AirTime>)>,
) {
    for &message in reader.read() {
        match message {
            Do { event: Event::Input { ent, key_bits, dt, .. } } => {
                if let Ok((_ent, &heading, &hx0, mut offset0, air_time)) = query.get_mut(ent) {
                    let mut offset = offset0.0;

                    let px = Vec3::from(hx0);
                    let curr = px + offset;
                    let curr_hx = Hx::from(curr);
                    let curr_px = Vec3::from(curr_hx);

                    let (floor, _) = map.find(curr_hx + Hx{ z: 1, ..default() }, -5);
                    
                    if let Some(mut air_time) = air_time { 
                        let mut dt = dt as i16;
                        if air_time.0 >= dt { 
                            // let transform = transform0.translation.lerp(target,1.-0.01f32.powf(time.delta_seconds()));
                            offset.z = offset.z.lerp(2.4, 1.-(air_time.0 as f32 / 1000.).powf(dt as f32 / 1000.));
                            trace!("Air time: {}, dt: {}, offset.z: {}", air_time.0, dt, offset.z);
                            air_time.0 -= dt; 
                        } else {
                            if air_time.0 > 0 {
                                dt -= air_time.0;
                                air_time.0 = 0;
                            }
                            let dz = dt as f32 / -100.;
                            if floor.is_none() || curr_hx.z as f32 + offset.z + dz > floor.unwrap().z as f32 + 1. { 
                                offset.z += dz;
                            } else {
                                offset.z = floor.unwrap().z as f32 + 1. - curr_hx.z as f32;
                                commands.entity(ent).remove::<AirTime>();
                            }
                        }
                    }
                    
                    let far = curr_px.xy().lerp(Vec3::from(curr_hx + heading.0).xy(), 1.25);
                    let near = px.xy().lerp(Vec3::from(hx0 + heading.0).xy(), 0.25);
                    let next = map.get(Hx::from(far.extend(hx0.z as f32)));
                    let target = 
                        if next == Entity::PLACEHOLDER && key_bits.any_pressed([KB_HEADING_Q, KB_HEADING_R]) { far }
                        else { near };

                    let dist = curr.xy().distance(target);
                    let ratio = 0_f32.max((dist - dt as f32 / 10.) / dist);
                    offset = (curr.xy().lerp(target, 1. - ratio) - px.xy()).extend(offset.z);

                    offset0.0 = offset;
                }
            }
            _ => {}
        }
    }
}

pub fn do_move(
    mut reader: EventReader<Do>,
    mut writer: EventWriter<Try>,
    map: Res<Map>,
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

                    for q in -5..=5 {
                        for r in max(-5, -q-5)..=min(5, -q+5) {
                            let hx = *hx0 + Hx { q, r, ..default() };
                            if map.find(hx, -5).0.is_none() { writer.send(Try { event: Event::Discover { ent, hx } }); }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

pub fn update_headings(
    mut writer: EventWriter<Try>,
    mut query: Query<(Entity, &Hx, &KeyBits, &mut Heading), Changed<KeyBits>>,
) {
    for (ent, &hx, &key_bits, mut heading0) in &mut query {
        let heading = Heading(if key_bits.all_pressed([KB_HEADING_Q, KB_HEADING_R, KB_HEADING_NEG]) { Hx { q: 1, r: -1, z: 0 } }
            else if key_bits.all_pressed([KB_HEADING_Q, KB_HEADING_R]) { Hx { q: -1, r: 1, z: 0 } }
            else if key_bits.all_pressed([KB_HEADING_Q, KB_HEADING_NEG]) { Hx { q: -1, r: 0, z: 0 } }
            else if key_bits.all_pressed([KB_HEADING_R, KB_HEADING_NEG]) { Hx { q: 0, r: -1, z: 0 } }
            else if key_bits.all_pressed([KB_HEADING_Q]) { Hx { q: 1, r: 0, z: 0 } }
            else if key_bits.all_pressed([KB_HEADING_R]) { Hx { q: 0, r: 1, z: 0 } }
            else { heading0.0 });
        if heading0.0 != heading.0 { 
            *heading0 = heading;
            writer.send(Try { event: Event::Move { ent, hx, heading } });
        }
    }
}

pub fn update_offsets(
    mut commands: Commands,
    mut writer: EventWriter<Try>,
    mut query: Query<(Entity, &Hx, &Heading, &Offset), Changed<Offset>>,
) {
    for (ent, &hx0, &heading, &offset) in &mut query {
        let px = Vec3::from(hx0);
        let hx = Hx::from(px + offset.0);
        if hx0 != hx { 
            commands.entity(ent).insert(AirTime(0));
            writer.send(Try { event: Event::Move { ent, hx, heading } }); 
        }
    }
}
