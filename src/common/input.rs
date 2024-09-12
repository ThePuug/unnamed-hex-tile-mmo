use bevy::prelude::*;

use super::components::keybits::KeyBits;

pub const KEYBIT_UP: u8 = 1 << 0;
pub const KEYBIT_DOWN: u8 = 1 << 1; 
pub const KEYBIT_LEFT: u8 = 1 << 2; 
pub const KEYBIT_RIGHT: u8 = 1 << 3; 
// pub const KEYBIT_JUMP = 1 << 4,

pub fn handle_input(
    time: Res<Time>,
    mut query: Query<(&KeyBits, &mut Transform)>,
) {
    for (&keys, mut transform) in query.iter_mut() {
        let mut direction = Vec2::ZERO;
        if keys & KEYBIT_UP != default() { direction.y += 1.0; }
        if keys & KEYBIT_DOWN != default() { direction.y -= 1.0; }
        if keys & KEYBIT_LEFT != default() { direction.x -= 1.0; }
        if keys & KEYBIT_RIGHT != default() { direction.x += 1.0; }

        transform.translation += (direction * 100. * time.delta_seconds()).extend(0.);
    }
}
