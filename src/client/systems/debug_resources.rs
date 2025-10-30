use bevy::prelude::*;

use crate::common::components::{
    resources::*,
    behaviour::Behaviour,
    reaction_queue::{ReactionQueue, QueuedThreat, DamageType},
    ActorAttributes,
};
use crate::common::systems::combat::queue::{insert_threat, calculate_timer_duration};

/// Debug system to test reaction queue and resource bars
/// Press keys to test different behaviors:
/// - Digit1 (top row): Queue a 20 damage threat (tests reaction queue system)
/// - Digit2 (top row): Drain 30 stamina directly
/// - Digit3 (top row): Drain 25 mana directly
pub fn debug_drain_resources(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut query: Query<(Entity, &mut Health, &mut Stamina, &mut Mana, &mut ReactionQueue, &ActorAttributes), With<Behaviour>>,
) {
    for (entity, mut health, mut stamina, mut mana, mut queue, attrs) in &mut query {
        // Queue a threat with Digit1 key (top row) - tests new reaction queue behavior
        if keyboard.just_pressed(KeyCode::Digit1) {
            let now = time.elapsed();
            let timer_duration = calculate_timer_duration(attrs);

            let threat = QueuedThreat {
                source: entity, // Self-inflicted for testing
                damage: 20.0,
                damage_type: DamageType::Physical,
                inserted_at: now,
                timer_duration,
            };

            // Insert threat into queue
            let overflow = insert_threat(&mut queue, threat, now);

            if let Some(overflow_threat) = overflow {
                // Queue was full, apply overflow damage immediately
                health.step = (health.step - overflow_threat.damage).max(0.0);
                health.state = health.step;
            }
        }

        // Drain stamina with Digit2 key (top row) - direct drain for testing
        if keyboard.just_pressed(KeyCode::Digit2) {
            stamina.step = (stamina.step - 30.0).max(0.0);
            stamina.state = stamina.step;
        }

        // Drain mana with Digit3 key (top row) - direct drain for testing
        if keyboard.just_pressed(KeyCode::Digit3) {
            mana.step = (mana.step - 25.0).max(0.0);
            mana.state = mana.step;
        }

        // Only process first player (local player)
        break;
    }
}

/// Debug system to manually process expired threats on the client side for UAT testing
/// In production, the server handles threat expiry and sends ApplyDamage events
/// This system allows us to test the queue behavior without server integration
pub fn debug_process_expired_threats(
    time: Res<Time>,
    mut query: Query<(&mut Health, &mut ReactionQueue), With<Behaviour>>,
) {
    let now = time.elapsed();

    for (mut health, mut queue) in &mut query {
        // Check for expired threats
        let mut expired_indices = Vec::new();
        for (idx, threat) in queue.threats.iter().enumerate() {
            if now >= threat.inserted_at + threat.timer_duration {
                expired_indices.push(idx);
            }
        }

        // Apply damage from expired threats and remove them
        for idx in expired_indices.iter().rev() {
            if let Some(threat) = queue.threats.remove(*idx) {
                health.step = (health.step - threat.damage).max(0.0);
                health.state = health.step;
            }
        }

        // Only process first player (local player)
        break;
    }
}
