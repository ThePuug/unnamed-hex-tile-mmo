use bevy::prelude::*;

use crate::common::components::{resources::*, behaviour::Behaviour};

/// Debug system to test resource bars
/// Press keys to drain resources and watch them regenerate:
/// - Digit1 (top row): Drain 20 health
/// - Digit2 (top row): Drain 30 stamina
/// - Digit3 (top row): Drain 25 mana
pub fn debug_drain_resources(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut query: Query<(&mut Health, &mut Stamina, &mut Mana), With<Behaviour>>,
) {
    for (mut health, mut stamina, mut mana) in &mut query {
        // Drain health with Digit1 key (top row)
        if keyboard.just_pressed(KeyCode::Digit1) {
            health.step = (health.step - 20.0).max(0.0);
            health.state = health.step;
        }

        // Drain stamina with Digit2 key (top row)
        if keyboard.just_pressed(KeyCode::Digit2) {
            stamina.step = (stamina.step - 30.0).max(0.0);
            stamina.state = stamina.step;
        }

        // Drain mana with Digit3 key (top row)
        if keyboard.just_pressed(KeyCode::Digit3) {
            mana.step = (mana.step - 25.0).max(0.0);
            mana.state = mana.step;
        }

        // Only process first player (local player)
        break;
    }
}
