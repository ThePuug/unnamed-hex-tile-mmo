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

const GRAVITY: f32 = 0.005;

pub fn apply(
    key_bits: KeyBits, 
    mut dt0: i16, 
    hx0: Hx,
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

        let px0 = Vec3::from(hx0);

        let (floor, _) = map.find(hx0 + Hx{q:0,r:0,z:1}, -5);
        if air_time0.is_none() {
            if floor.is_none() || Hx::from(Vec3::from(hx0) + Vec3::Y * offset0.y).z > floor.unwrap().z+1 { 
                air_time0 = Some(0); 
            }
            if key_bits.is_pressed(KB_JUMP) && !jumped { 
                trace!("jump");
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
    
                // trace!("ascend for {dt}ms");
                offset0.y += dt as f32 * GRAVITY * 5.;
            } else {
                // falling
                air_time -= dt;
                air_time0 = Some(air_time);

                // debug!("falling for {dt}ms");
                let dy = -dt as f32 * GRAVITY;
                if floor.is_none() || Hx::from(Vec3::from(hx0) + Vec3::Y * (offset0.y + dy)).z > floor.unwrap().z+1 { 
                    offset0.y += dy;
                } else {
                    debug!("landed after {:?}", air_time0);
                    offset0.y = Vec3::from(floor.unwrap() + Hx { z: 1-hx0.z, ..hx0 }).y; 
                    air_time0 = None;
                }
            }
        }

        let hpx = Vec3::from(heading0.0);
        let npx = Vec3::from(Hx::from(px0 + offset0));
        let here = hpx * HERE;
        let there = Vec3::ZERO.lerp(hpx, 1.25);
        let tpx = if map.get(Hx::from(npx+there)) == Entity::PLACEHOLDER && key_bits.any_pressed([KB_HEADING_Q, KB_HEADING_R]) 
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
        if heading0.0 != heading.0 { 
            *heading0 = heading;
            let attr = Attribute::Heading { heading };
            writer.send(Try { event: Event::Incremental { ent, attr } });
        }
    }
}
