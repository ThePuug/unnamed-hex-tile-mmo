use bevy::prelude::*;

use crate::common::components::*;

pub fn handle_input(
    time: Res<Time>,
    mut query: Query<(&mut Input, &mut Transform)>,
) {
    for (input, mut transform) in query.iter_mut() {
        let mut dy = f32::from((input.keys & KeyBit::UP as u8) != 0);
        dy -= f32::from((input.keys & KeyBit::DOWN as u8) != 0);
        let mut dx = f32::from((input.keys & KeyBit::RIGHT as u8) != 0);
        dx -= f32::from((input.keys & KeyBit::LEFT as u8) != 0);

        transform.translation.x += dx * 100.0 * time.delta_seconds();
        transform.translation.y += dy * 100.0 * time.delta_seconds();
    }
}
