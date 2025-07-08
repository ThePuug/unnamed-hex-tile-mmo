use std::cmp::min;

use bevy::prelude::*;
use qrz::{Convert, Qrz};

use crate::common::{ 
    components::{ *,
        heading::*,
        keybits::*,
        offset::*,
    }, 
    message::{ Attribute, Event, * }, 
    resources::map::*
};

const GRAVITY: f32 = 0.005;

pub fn apply(
    key_bits: KeyBits, 
    mut dt0: i16, 
    qrz0: Qrz,
    heading0: Heading,
    offset0: Vec3,
    air_time0: Option<i16>,
    map: &Map,
) -> (Vec3, Option<i16>) {
    let mut offset0 = offset0;
    let mut air_time0 = air_time0;
    let mut jumped = false;
    let heading0 = if key_bits.any_pressed([KB_HEADING_Q, KB_HEADING_R]) { Heading::from(key_bits) } else { heading0 };
    while dt0 >= 0 {
        dt0-=125; 
        let mut dt = min(125+dt0, 125);

        let px0 = map.convert(qrz0);

        let floor = map.find(qrz0 + Qrz{q:0,r:0,z:1}, -5);
        if air_time0.is_none() {
            if floor.is_none() || map.convert(map.convert(qrz0) + Vec3::Y * offset0.y).z > floor.unwrap().0.z+1 {
                air_time0 = Some(0); 
            }
            if key_bits.is_pressed(KB_JUMP) && !jumped { 
                air_time0 = Some(125); 
                jumped = true; 
            }
        }
        
        if let Some(mut air_time) = air_time0 {
            if air_time > 0 {
                // ensure we ascend to the apex
                if air_time < dt { 
                    dt0 += dt-air_time;
                    dt = air_time;
                }
                air_time -= dt;
                air_time0 = Some(air_time);
                offset0.y += dt as f32 * GRAVITY * 5.;
            } else {
                // falling
                air_time -= dt;
                air_time0 = Some(air_time);
                let dy = -dt as f32 * GRAVITY;
                if floor.is_none() || map.convert(map.convert(qrz0) + Vec3::Y * (offset0.y + dy)).z > floor.unwrap().0.z+1 { 
                    offset0.y += dy;
                } else {
                    offset0.y = map.convert(floor.unwrap().0 + Qrz { z: 1-qrz0.z, ..qrz0 }).y; 
                    air_time0 = None;
                }
            }
        }

        let hpx = map.convert(*heading0);
        let npx = map.convert(map.convert(px0 + offset0));
        let here = hpx * HERE;
        let there = hpx * THERE;
        let tpx = if map.get(map.convert(npx+there)).is_none() && key_bits.any_pressed([KB_HEADING_Q, KB_HEADING_R]) 
                { npx + there - px0 }
            else { here };

        let dpx = offset0.distance(tpx);
        let ratio = 0_f32.max((dpx - 0.005*dt as f32) / dpx);
        let lxz = offset0.xz().lerp(tpx.xz(), 1. - ratio);
        offset0 = Vec3::new(lxz.x, offset0.y, lxz.y);
    }

    (offset0, air_time0)
}

pub fn do_incremental(
    mut reader: EventReader<Do>,
    mut writer: EventWriter<Try>,
    mut query: Query<(&mut Loc, &mut Offset, &mut Heading)>,
    map: Res<Map>,
) {
    for &message in reader.read() {
        if let Do { event: Event::Incremental { ent, attr } } = message {
            if let Ok((mut loc0, mut offset0, mut heading0)) = query.get_mut(ent) {
                match attr {
                    Attribute::Qrz { qrz } => {
                        offset0.state = map.convert(**loc0) + offset0.state - map.convert(qrz);
                        offset0.step = map.convert(**loc0) + offset0.step - map.convert(qrz);

                        writer.send(Try { event: Event::Discover { ent, qrz } });
                        *loc0 = Loc::new(qrz);

                        if **heading0 != Qrz::default() {
                            for qrz in loc0.fov(&heading0, 10) {
                                writer.send(Try { event: Event::Discover { ent, qrz } });
                            }
                        }
                    }
                    Attribute::Heading { heading } => {
                        *heading0 = heading;
                        if **heading0 != Qrz::default() {
                            for qrz in loc0.fov(&heading0, 10) {
                                writer.send(Try { event: Event::Discover { ent, qrz } });
                            }
                        }
                    }
                    _ => unreachable!(),
                }
            }
        }
    }
}

pub fn update_heading(
    mut writer: EventWriter<Try>,
    mut query: Query<(Entity, &KeyBits, &mut Heading), Changed<KeyBits>>,
) {
    for (ent, &key_bits, mut heading0) in &mut query {
        let heading = if key_bits.any_pressed([KB_HEADING_Q, KB_HEADING_R]) { Heading::from(key_bits) } else { *heading0 };
        if *heading0 != heading { 
            *heading0 = heading;
            let attr = Attribute::Heading { heading };
            writer.send(Try { event: Event::Incremental { ent, attr } });
        }
    }
}
