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
    heading: &Heading,
    hx0: &Hx,
    offset0: Vec3,
    air_time0: Option<i16>,
    map: &Map,
) -> (Vec3, Option<i16>) {
    let mut offset0 = offset0;
    let mut air_time0 = air_time0;

    // trace!("was offset({}), air_time({:?})", offset0, air_time0);

    if air_time0.is_none() && key_bits.all_pressed([KB_JUMP]) { air_time0 = Some(500); }

    let px = Vec3::from(*hx0);
    let curr = px + offset0;
    let curr_hx = Hx::from(curr);
    let curr_px = Vec3::from(curr_hx);

    let (floor, _) = map.find(curr_hx + Hx{ z: 1, ..default() }, -5);
    
    if air_time0.is_some() {
        let air_time = &mut *air_time0.as_mut().unwrap();
        if *air_time > 0 {
            let mut dt = dt as i16;
            if *air_time < dt { dt = *air_time; }
            offset0.z = offset0.z.lerp(2.4, 1.-(*air_time as f32 / 1000.).powf(dt as f32 / 1000.));
        }
        if dt > *air_time { dt -= *air_time; *air_time = 0; }
        *air_time -= dt; 
        if *air_time < 0 {
            let dz = dt as f32 / -100.;
            if floor.is_none() || curr_hx.z as f32 + offset0.z + dz > floor.unwrap().z as f32 + 1. { 
                offset0.z += dz;
            } else {
                offset0.z = floor.unwrap().z as f32 + 1. - curr_hx.z as f32;
                air_time0 = None;
            }
        }
    }
    
    let far = curr_px.xy().lerp(Vec3::from(curr_hx + heading.0).xy(), 1.25);
    let near = px.xy().lerp(Vec3::from(*hx0 + heading.0).xy(), 0.25);
    let next = map.get(Hx::from(far.extend(hx0.z as f32)));
    let target = 
        if next == Entity::PLACEHOLDER && key_bits.any_pressed([KB_HEADING_Q, KB_HEADING_R]) { far }
        else { near };

    let dist = curr.xy().distance(target);
    let ratio = 0_f32.max((dist - dt as f32 / 10.) / dist);
    offset0 = (curr.xy().lerp(target, 1. - ratio) - px.xy()).extend(offset0.z);

    // trace!("now offset({}), air_time({:?}) after dt({})", offset0, air_time0, dt);
    
    (offset0, air_time0)
}

pub fn do_input(
    mut reader: EventReader<Do>,
    mut query: Query<(&Heading, &Hx, &mut Offset, &mut AirTime)>,
    map: Res<Map>,
) {
    for &message in reader.read() {
        match message {
            Do { event: Event::Input { ent, key_bits, dt, seq } } => {
                trace!("do seq({}) dt({}) kb({}) ent({})", seq, dt, key_bits.key_bits, ent);
                let (&heading, &hx, mut offset, mut air_time) = query.get_mut(ent).unwrap();
                (offset.state, air_time.state) = apply(key_bits, dt as i16, &heading, &hx, offset.state, air_time.state, &map);
                offset.step = offset.state;
                air_time.step = air_time.state;
            }, 
            _ => {}
        }
    }
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
                    if *hx0 != hx { 
                        *hx0 = hx; 
                        *heading0 = heading;
                    }
                    else if *heading0 != heading { *heading0 = heading; }

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
    mut query: Query<(Entity, &Hx, &Heading, &Offset, &mut AirTime), Changed<Offset>>,
) {
    for (ent, &hx0, &heading, &offset, mut air_time) in &mut query {
        let px = Vec3::from(hx0);
        let hx = Hx::from(px + offset.state);
        if hx0 != hx { 
            air_time.state = Some(0);
            writer.send(Try { event: Event::Move { ent, hx, heading } }); 
        }
    }
}
