use std::cmp::min;

use bevy::prelude::*;
use qrz::{Convert, Qrz};

use crate::common::{ 
    components::{ 
        entity_type::{decorator::*, *}, 
        heading::*, 
        keybits::*, 
        offset::*, 
        * 
    }, 
    message::Event, 
    plugins::nntree::*, 
    resources::{map::*, *}
};

const GRAVITY: f32 = 0.005;

pub fn update(
    mut query: Query<(&Loc, &mut Offset, &mut AirTime), With<Physics>>,
    map: Res<Map>,
    buffers: Res<InputQueues>,
    nntree: Res<NNTree>,
) {
    for (&ent, buffer) in buffers.iter() {
        let Ok((&loc, mut offset0, mut airtime0)) = query.get_mut(ent) else { continue; };
        let (mut offset, mut airtime) = (offset0.state, airtime0.state);
        for input in buffer.queue.iter().rev() {
            let Event::Input { key_bits, dt, .. } = input else { unreachable!() };
            let dest = Loc::new(*Heading::from(*key_bits) + *loc);
            if key_bits.is_pressed(KB_JUMP) && airtime.is_none() { airtime = Some(125); }
            (offset, airtime) = apply(dest, *dt as i16, loc, offset, airtime, &map, &nntree);
        }
        (offset0.step, airtime0.step) = (offset,airtime);
    }
}

pub fn apply(
    dest: Loc,
    mut dt0: i16, 
    loc0: Loc,
    offset0: Vec3,
    airtime0: Option<i16>,
    map: &Map,
    nntree: &NNTree,
) -> (Vec3, Option<i16>) {
    let mut offset0 = offset0;
    let mut airtime0 = airtime0;

    while dt0 >= 0 {
        // step physics forward 125ms at a time
        dt0-=125; 
        let mut dt = min(125+dt0, 125);

        let px0 = map.convert(*loc0);                                       // current px of loc
        let step_hx = map.convert(px0 + offset0);                           // current offset from loc
        let floor = map.find(step_hx + Qrz::Z*30, -60);
        
        if airtime0.is_none() {
            if floor.is_none() || map.convert(map.convert(*loc0) + Vec3::Y * offset0.y).z > floor.unwrap().0.z+1 {
                airtime0 = Some(0); 
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
                offset0.y += dt as f32 * GRAVITY * 5.; // jump 5 times faster than you fall
            } else {
                // falling
                airtime -= dt;
                airtime0 = Some(airtime);
                let dy = -dt as f32 * GRAVITY;
                if floor.is_none() || map.convert(map.convert(*loc0) + Vec3::Y * (offset0.y + dy)).z > floor.unwrap().0.z+1 { 
                    offset0.y += dy;
                } else {
                    offset0.y = map.convert(floor.unwrap().0 + Qrz { z: 1-loc0.z, ..*loc0 }).y; 
                    airtime0 = None;
                }
            }
        }

        let rel_px = map.convert(*dest)-px0;                                // destination px relative to current px
        let rel_hx = map.convert(rel_px);                                   // destination tile relative to loc
        let heading = Heading::from(KeyBits::from(Heading::new(rel_hx)));   // direction towards destination tile
        let next_hx = step_hx + *heading;                                   // next tile towards destination

        // check whether next tile is solid, or if there's a walkable floor nearby
        // First check the exact tile
        let exact_is_solid = match map.get(next_hx) {
            Some(EntityType::Decorator(Decorator{is_solid, .. })) => *is_solid,
            _ => nntree.locate_all_at_point(&Loc::new(next_hx)).count() >= 7
        };
        
        // If exact tile is solid, check for a valid floor within 1 tile rise (up or down)
        // This allows smooth walking on slopes
        let is_blocked = if exact_is_solid {
            // Search for a valid floor tile within stepping distance
            let has_nearby_floor = map.find(next_hx + Qrz::Z*30, -60)
                .map(|(floor_qrz, _)| {
                    // Allow movement if floor is within 1 tile rise of current position
                    (floor_qrz.z - loc0.z).abs() <= 1
                })
                .unwrap_or(false);
            
            !has_nearby_floor  // Block only if no nearby floor exists
        } else {
            false  // Not solid, allow movement
        };

        // set target px HERE when blocked, otherwise THERE
        let target_px = if is_blocked { rel_px * HERE } else { rel_px * THERE };

        let delta_px = offset0.distance(target_px);
        let ratio = 0_f32.max((delta_px - 0.005*dt as f32) / delta_px);
        let lerp_xz = offset0.xz().lerp(target_px.xz(), 1. - ratio);
        offset0 = Vec3::new(lerp_xz.x, offset0.y, lerp_xz.y);
        
        // When on ground, smoothly adjust Y to match terrain height
        if airtime0.is_none() {
            // Recalculate floor based on new horizontal position
            let current_hx = map.convert(px0 + offset0);
            let current_floor = map.find(current_hx + Qrz::Z*30, -60);
            
            if let Some((floor_qrz, _)) = current_floor {
                let target_y = map.convert(floor_qrz + Qrz { z: 1-loc0.z, ..*loc0 }).y;
                // Smoothly lerp Y position toward terrain height
                let y_lerp_speed = 0.3; // Adjust this for faster/slower slope climbing
                offset0.y = offset0.y * (1.0 - y_lerp_speed) + target_y * y_lerp_speed;
            }
        }
    }

    (offset0, airtime0)
}
