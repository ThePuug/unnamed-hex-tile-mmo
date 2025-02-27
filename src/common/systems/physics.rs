use std::cmp::{max, min};

use crate::{ *,
    common::{
        message::{*, Event},
        components::{
            heading::*,
            hx::*,
            keybits::*,
            offset::*,
        },
        resources::map::*,
    },
};

pub fn apply(
    key_bits: KeyBits, 
    mut dt: i16, 
    heading: Heading,
    hx0: Hx,
    offset0: Vec3,
    air_time0: Option<i16>,
    map: &Map,
) -> (Vec3, Option<i16>) {
    let mut offset0 = offset0;
    let mut air_time0 = air_time0;

    let px = Vec3::from(hx0);

    let (floor, _) = map.find(hx0 + Hx{ z: 1, ..default() }, -5);
    if air_time0.is_none() && (
        floor.is_none() || floor.is_some() && hx0.z > floor.unwrap().z+1 || key_bits.is_pressed(KB_JUMP)
    ) { air_time0 = Some(200); }
    
    if let Some(mut air_time) = air_time0 {
        if air_time > 0 {
            let mut dt = dt as i16;
            if air_time < dt { dt = air_time; }
            offset0.z += 0_f32.lerp(air_time as f32 / 50., 1. - 2_f32.powf(-10. * dt as f32 / 1000.));
        }
        if dt > air_time { 
            dt -= air_time; 
            air_time = 0; 
        }
        air_time -= dt;

        air_time0 = Some(air_time);
        if air_time < 0 {
            let dz = dt as f32 / -100.;
            if floor.is_none() || hx0.z as f32 + offset0.z + dz > floor.unwrap().z as f32 + 1. { 
                offset0.z += dz;
            } else {
                offset0.z = floor.unwrap().z as f32 + 1. - hx0.z as f32; 
                air_time0 = None;
            }
        }
    }

    let pxy = px.xy();
    let hxy = Vec3::from(heading.0).xy();
    let here = Vec2::ZERO.lerp(hxy, 0.25);
    let there = Vec2::ZERO.lerp(hxy, 2.25);
    let next = Vec2::ZERO.lerp(hxy, 1.25);
    let target = 
        if map.get(Hx::from((pxy+next).extend(hx0.z as f32))) == Entity::PLACEHOLDER && key_bits.any_pressed([KB_HEADING_Q, KB_HEADING_R]) { there }
        else { here };

    let dist = offset0.xy().distance(target);
    let ratio = 0_f32.max((dist - 0.1*dt as f32) / dist);
    offset0 = offset0.xy().lerp(target, 1. - ratio).extend(offset0.z);

    (offset0, air_time0)
}

pub fn do_move(
    mut reader: EventReader<Do>,
    mut writer: EventWriter<Try>,
    mut query: Query<(&mut Hx, &mut Offset, &mut Heading)>,
) {
    for &message in reader.read() {
        match message {
            Do { event: Event::Move { ent, hx, heading } } => {
                if let Ok((mut hx0, mut offset0, mut heading0)) = query.get_mut(ent) {
                    offset0.state = Vec3::from(*hx0) + offset0.state - Vec3::from(hx);
                    offset0.step = Vec3::from(*hx0) + offset0.step - Vec3::from(hx);
                    if *hx0 != hx { *hx0 = hx; }
                    if *heading0 != heading { *heading0 = heading; }

                    for q in -5..=5 {
                        for r in max(-5, -q-5)..=min(5, -q+5) {
                            let hx = *hx0 + Hx { q, r, ..default() };
                            writer.send(Try { event: Event::Discover { ent, hx } }); 
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
    mut writer: EventWriter<Try>,
    mut query: Query<(Entity, &Hx, &Heading, &Offset), Changed<Offset>>,
) {
    for (ent, &hx0, &heading, &offset) in &mut query {
        let px = Vec3::from(hx0);
        let hx = Hx::from(px + offset.state);
        if hx0 != hx { 
            writer.send(Try { event: Event::Move { ent, hx, heading } }); 
        }
    }
}
