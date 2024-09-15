use bevy::prelude::*;

use super::{
    components::keybits::*, 
    hxpx::*,
};

#[derive(Component, Default)]
pub struct Heading(pub Hx);

pub const KEYBIT_UP: u8 = 1 << 0;
pub const KEYBIT_DOWN: u8 = 1 << 1; 
pub const KEYBIT_LEFT: u8 = 1 << 2; 
pub const KEYBIT_RIGHT: u8 = 1 << 3; 
// pub const KEYBIT_JUMP = 1 << 4,

pub fn handle_input(
    time: Res<Time>,
    mut query: Query<(&KeyBits, &mut Transform, &mut Heading)>,
) {
    for (&keys, mut transform, mut heading) in query.iter_mut() {
        if keys & (KEYBIT_UP | KEYBIT_DOWN | KEYBIT_LEFT | KEYBIT_RIGHT) != default() {
            if keys & KEYBIT_UP != default() {
                if keys & KEYBIT_LEFT != default() || keys & KEYBIT_RIGHT == default()
                    &&(heading.0 == (Hx {q:-1,r: 0,z: 0})
                    || heading.0 == (Hx {q:-1,r: 1,z: 0})
                    || heading.0 == (Hx {q: 1,r:-1,z: 0})) { *heading = Heading { 0:Hx {q:-1,r: 1,z: 0} }; }
                else  { *heading = Heading { 0:Hx {q: 0,r: 1,z: 0} }; }
            } else if keys & KEYBIT_DOWN != default() {
                if keys & KEYBIT_RIGHT != default() || keys & KEYBIT_LEFT == default()
                    &&(heading.0 == (Hx {q: 1,r: 0,z: 0})
                    || heading.0 == (Hx {q: 1,r:-1,z: 0})
                    || heading.0 == (Hx {q:-1,r: 1,z: 0})) { *heading = Heading { 0:Hx {q: 1,r: -1,z: 0} }; }
                else { *heading = Heading { 0:Hx {q: 0,r:-1,z: 0} }; }
            } 
            else if keys & KEYBIT_RIGHT != default() { *heading = Heading { 0:Hx {q: 1,r: 0,z: 0} }; }
            else if keys & KEYBIT_LEFT != default() { *heading = Heading { 0:Hx {q:-1,r: 0,z: 0} }; }

            let loc = Hx::from(transform.translation);
            let target = loc + heading.0;
            let delta = Vec3::from(target).xy() - transform.translation.xy();
            trace!("loc: {:?}, target: {:?}, delta: {:?}", loc, target, delta);
            transform.translation += (delta.normalize_or_zero() * 100. * time.delta_seconds()).extend(0.);            
        }
    }
}
