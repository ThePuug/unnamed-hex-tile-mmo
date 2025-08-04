use std::cmp::min;

use bevy::prelude::*;
use qrz::{Convert, Qrz};

use crate::common::{ 
    components::{ heading::*, keybits::*, offset::*, * }, 
    message::Event,
    resources::{map::*, *},
};

const GRAVITY: f32 = 0.005;

pub fn update(
    mut query: Query<(&Loc, &Heading, &mut Offset, &mut AirTime), With<Physics>>,
    map: Res<Map>,
    buffers: Res<InputQueues>,
) {
    for (&ent, buffer) in buffers.iter() {
        let Ok((&loc, &heading, mut offset0, mut airtime0)) = query.get_mut(ent) else { continue; };
        let (mut offset, mut airtime) = (offset0.state, airtime0.state);
        for input in buffer.queue.iter().rev() {
            let Event::Input { key_bits, dt, .. } = input else { unreachable!() };
            (offset, airtime) = apply(*key_bits, *dt as i16, *loc, heading, offset, airtime, &map);
        }
        (offset0.step, airtime0.step) = (offset,airtime);
    }
}

pub fn apply(
    key_bits: KeyBits, 
    mut dt0: i16, 
    qrz0: Qrz,
    heading0: Heading,
    offset0: Vec3,
    airtime0: Option<i16>,
    map: &Map,
) -> (Vec3, Option<i16>) {
    let mut offset0 = offset0;
    let mut airtime0 = airtime0;
    let mut jumped = false;
    let heading0 = if key_bits.any_pressed([KB_HEADING_Q, KB_HEADING_R]) { Heading::from(key_bits) } else { heading0 };
    while dt0 >= 0 {
        dt0-=125; 
        let mut dt = min(125+dt0, 125);

        let px0 = map.convert(qrz0);

        let floor = map.find(qrz0 + Qrz{q:0,r:0,z:5}, -10);
        if airtime0.is_none() {
            if floor.is_none() || map.convert(map.convert(qrz0) + Vec3::Y * offset0.y).z > floor.unwrap().0.z+1 {
                airtime0 = Some(0); 
            }
            if key_bits.is_pressed(KB_JUMP) && !jumped { 
                airtime0 = Some(125); 
                jumped = true; 
            }
        }
            
        if let Some(mut airtime) = airtime0 {
            if airtime > 0 {
                // ensure we ascend to the apex
                if airtime < dt { 
                    dt0 += dt-airtime;
                    dt = airtime;
                }
                airtime -= dt;
                airtime0 = Some(airtime);
                offset0.y += dt as f32 * GRAVITY * 5.;
            } else {
                // falling
                airtime -= dt;
                airtime0 = Some(airtime);
                let dy = -dt as f32 * GRAVITY;
                if floor.is_none() || map.convert(map.convert(qrz0) + Vec3::Y * (offset0.y + dy)).z > floor.unwrap().0.z+1 { 
                    offset0.y += dy;
                } else {
                    offset0.y = map.convert(floor.unwrap().0 + Qrz { z: 1-qrz0.z, ..qrz0 }).y; 
                    airtime0 = None;
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

    (offset0, airtime0)
}
