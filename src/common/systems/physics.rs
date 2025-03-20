use std::cmp::{max, min};

use bevy::prelude::*;

use crate::common::{ 
    components::{
        heading::*,
        hx::*,
        keybits::*,
        offset::*,
    }, 
    message::{ *, Attribute, Event }, 
    resources::map::*
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

    let px0 = Vec3::from(hx0);

    let (floor, _) = map.find(hx0 + Hx{q:0,r:0,z:1}, -5);
    if air_time0.is_none() {
        if floor.is_none() || floor.is_some() && hx0.z > floor.unwrap().z+1 { air_time0 = Some(0); }
        if key_bits.is_pressed(KB_JUMP) { air_time0 = Some(50); }
    }
    
    if let Some(mut air_time) = air_time0 {
        if air_time > 0 {
            let mut dt = dt;
            if air_time < dt { dt = air_time; }
            offset0.y += dt as f32;
        }
        if dt > air_time { 
            dt -= air_time; 
            air_time = 0; 
        }
        air_time -= dt;

        air_time0 = Some(air_time);
        if air_time < 0 {
            let dy = -dt as f32 / 10.;
            if floor.is_none() || Hx::from(Vec3::from(hx0) + Vec3::Y * (offset0.y + dy)).z > floor.unwrap().z+1 { 
                offset0.y += dy;
            } else {
                offset0.y = Vec3::from(floor.unwrap() + Hx{q:0,r:0,z:1-hx0.z}).y; 
                air_time0 = None;
            }
        }
    }

    let hpx = Vec3::from(heading.0);
    let here = Vec3::ZERO.lerp(hpx, 0.25);
    let there = Vec3::ZERO.lerp(hpx, 2.25);
    let next = Vec3::ZERO.lerp(hpx, 1.25);
    let target = 
        if map.get(Hx::from(px0+next)) == Entity::PLACEHOLDER 
            && key_bits.any_pressed([KB_HEADING_Q, KB_HEADING_R]) { there }
        else { here };

    let dpx = offset0.distance(target);
    let ratio = 0_f32.max((dpx - 0.1*dt as f32) / dpx);
    let lxz = offset0.xz().lerp(target.xz(), 1. - ratio);
    offset0 = Vec3::new(lxz.x, offset0.y, lxz.y);

    (offset0, air_time0)
}

pub fn do_incremental(
    mut reader: EventReader<Do>,
    mut writer: EventWriter<Try>,
    mut query: Query<(&mut Hx, &mut Offset, &mut Heading)>,
) {
    for &message in reader.read() {
        if let Do { event: Event::Incremental { ent, attr } } = message {
            if let Ok((mut hx0, mut offset0, mut heading0)) = query.get_mut(ent) {
                match attr {
                    Attribute::Hx { hx } => {
                        offset0.state = Vec3::from(*hx0) + offset0.state - Vec3::from(hx);
                        offset0.step = Vec3::from(*hx0) + offset0.step - Vec3::from(hx);

                        *hx0 = hx;
                        for q in -5..=5 {
                            for r in max(-5, -q-5)..=min(5, -q+5) {
                                let hx = *hx0 + Hx { q, r, ..default() };
                                writer.send(Try { event: Event::Discover { ent, hx } }); 
                            }
                        }
                    }
                    Attribute::Heading { heading } => {
                        *heading0 = heading;
                    }
                    Attribute::Offset { offset } => {
                        *offset0 = offset;
                    }
                }
            }
        }
    }
}

pub fn update_headings(
    mut writer: EventWriter<Try>,
    mut query: Query<(Entity, &KeyBits, &mut Heading), Changed<KeyBits>>,
) {
    for (ent, &key_bits, mut heading0) in &mut query {
        let heading = Heading(if key_bits.all_pressed([KB_HEADING_Q, KB_HEADING_R, KB_HEADING_NEG]) { Hx { q: 1, r: -1, z: 0 } }
            else if key_bits.all_pressed([KB_HEADING_Q, KB_HEADING_R]) { Hx { q: -1, r: 1, z: 0 } }
            else if key_bits.all_pressed([KB_HEADING_Q, KB_HEADING_NEG]) { Hx { q: -1, r: 0, z: 0 } }
            else if key_bits.all_pressed([KB_HEADING_R, KB_HEADING_NEG]) { Hx { q: 0, r: -1, z: 0 } }
            else if key_bits.all_pressed([KB_HEADING_Q]) { Hx { q: 1, r: 0, z: 0 } }
            else if key_bits.all_pressed([KB_HEADING_R]) { Hx { q: 0, r: 1, z: 0 } }
            else { heading0.0 });
        if heading0.0 != heading.0 { 
            *heading0 = heading;
            let attr = Attribute::Heading { heading };
            writer.send(Try { event: Event::Incremental { ent, attr } });
        }
    }
}


pub fn update_offsets(
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
