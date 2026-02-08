//! Movement Prediction and Visual Interpolation Systems (ADR-019)
//!
//! This module manages Position and VisualPosition components for local player
//! prediction and visual interpolation.
//!
//! # System Execution Order
//!
//! ```text
//! FixedUpdate:
//!   controlled::apply     → writes confirmed state to Position.offset
//!
//! FixedPostUpdate:
//!   predict_local_player  → replays InputQueue from Position.offset → VisualPosition
//!
//! Update (every frame):
//!   apply_movement_intent → VisualPosition.interpolate_toward(dest) (remote entities)
//!   advance_interpolation → VisualPosition.advance(delta_secs) (ALL entities)
//!   actor::update         → Transform = VisualPosition.current() (ALL entities)
//! ```

use bevy::prelude::*;
use qrz::Convert;

use crate::common::{
    components::{
        heading::*,
        keybits::*,
        position::{Position, VisualPosition},
        AirTime, ActorAttributes, Loc,
    },
    message::Event,
    plugins::nntree::*,
    resources::{map::Map, InputQueues},
    systems::physics,
};

/// Predict local player position by replaying the InputQueue from confirmed state.
///
/// Runs in FixedPostUpdate after controlled::apply has written confirmed state
/// to Position.offset and do_input has popped confirmed inputs.
///
/// Replays the entire queue from Position.offset (confirmed) to produce a
/// predicted world position, then writes that to VisualPosition for smooth rendering.
/// Position.offset is NEVER overwritten — it stays authoritative/confirmed.
pub fn predict_local_player(
    fixed_time: Res<Time<Fixed>>,
    mut query: Query<(&Loc, &Position, &mut Heading, &mut AirTime, &mut VisualPosition, Option<&ActorAttributes>)>,
    map: Res<Map>,
    nntree: Res<NNTree>,
    buffers: Res<InputQueues>,
) {
    let tick_duration = fixed_time.timestep().as_secs_f32();

    for (ent, buffer) in buffers.iter() {
        // Queue invariant: all queues must have at least 1 input
        assert!(!buffer.queue.is_empty(), "Queue invariant violation: entity {ent} has empty queue");

        let Ok((&loc, position, mut heading, mut airtime, mut visual, attrs)) = query.get_mut(ent) else { continue; };
        let movement_speed = attrs.map(|a| a.movement_speed()).unwrap_or(0.005);

        // Start from confirmed state
        let (mut offset, mut air) = (position.offset, airtime.state);
        let mut new_heading = *heading;

        // Replay entire queue from confirmed state to produce predicted state
        for input in buffer.queue.iter().rev() {
            let Event::Input { key_bits, dt, .. } = input else { unreachable!() };
            let input_heading = Heading::from(*key_bits);
            if *input_heading != default() {
                new_heading = input_heading;
            }
            let dest = Loc::new(*input_heading + *loc);
            if key_bits.is_pressed(KB_JUMP) && air.is_none() { air = Some(125); }
            (offset, air) = physics::apply(dest, *dt as i16, loc, offset, air, movement_speed, new_heading, &map, &nntree);
        }

        // Update heading to last non-default heading from prediction
        *heading = new_heading;

        // Store predicted airtime for animation (step field used by animator)
        airtime.step = air;

        // Write predicted world position to VisualPosition (NOT Position)
        // Use 2x tick duration so interpolation spans ~2 frames, providing real smoothing.
        // At 1x, interpolation completes in a single frame (tick≈15.6ms < frame≈16.7ms),
        // causing visible jitter on direction changes as prediction targets shift abruptly.
        let predicted_world = map.convert(*loc) + offset;
        visual.interpolate_toward(predicted_world, tick_duration * 2.0);
    }
}

/// Advance VisualPosition interpolation each frame.
///
/// Runs in Update. Moves progress forward by frame delta time.
/// This produces smooth movement at any frame rate.
pub fn advance_interpolation(
    time: Res<Time>,
    mut query: Query<&mut VisualPosition>,
) {
    let delta = time.delta_secs();

    for mut visual in &mut query {
        visual.advance(delta);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::components::position::Position;
    use qrz::Qrz;

    #[test]
    fn test_visual_position_interpolation_flow() {
        // Simulates the FixedUpdate → Update interpolation flow
        let map = Map::new(qrz::Map::new(1.0, 0.8));

        // Initial state: entity at tile (0,0,0) with zero offset
        let position = Position::at_tile(Qrz { q: 0, r: 0, z: 0 });
        let mut visual = VisualPosition::at(position.to_world(&map));

        // Physics tick: entity moves east
        let new_position = Position::new(Qrz { q: 0, r: 0, z: 0 }, Vec3::new(0.3, 0.0, 0.0));
        let target = new_position.to_world(&map);
        let tick_duration = 0.125; // 125ms fixed timestep

        visual.interpolate_toward(target, tick_duration);

        // Frame at 50% through tick
        visual.advance(tick_duration * 0.5);
        let mid_pos = visual.current();

        // Should be halfway between old and new position
        let expected_mid = visual.from.lerp(target, 0.5);
        assert!(
            (mid_pos - expected_mid).length() < 0.001,
            "Mid-frame position should be halfway. Got {:?}, expected {:?}",
            mid_pos, expected_mid
        );

        // Frame at 100% through tick
        visual.advance(tick_duration * 0.5);
        let end_pos = visual.current();

        // Should be at target
        assert!(
            (end_pos - target).length() < 0.001,
            "End-frame position should be at target. Got {:?}, expected {:?}",
            end_pos, target
        );
    }

    #[test]
    fn test_direction_change_no_jitter() {
        // Simulates direction change: moving east then switching to west
        let map = Map::new(qrz::Map::new(1.0, 0.8));

        let initial_pos = Position::at_tile(Qrz { q: 0, r: 0, z: 0 });
        let mut visual = VisualPosition::at(initial_pos.to_world(&map));

        // Tick 1: Move east
        let east_pos = Position::new(Qrz { q: 0, r: 0, z: 0 }, Vec3::new(0.5, 0.0, 0.0));
        visual.interpolate_toward(east_pos.to_world(&map), 0.125);
        visual.advance(0.0625); // 50% through tick

        let pos_before_change = visual.current();

        // Tick 2: Direction change! Now moving west
        let west_pos = Position::new(Qrz { q: 0, r: 0, z: 0 }, Vec3::new(-0.5, 0.0, 0.0));
        visual.interpolate_toward(west_pos.to_world(&map), 0.125);

        let pos_after_change = visual.current();

        // Key assertion: no visual jump on direction change
        let jump_distance = (pos_after_change - pos_before_change).length();
        assert!(
            jump_distance < 0.001,
            "Direction change should not cause visual jump. Jump distance: {}",
            jump_distance
        );
    }

    #[test]
    fn test_teleport_correction() {
        // When server sends a large correction, VisualPosition snaps
        let map = Map::new(qrz::Map::new(1.0, 0.8));

        let initial_pos = Position::at_tile(Qrz { q: 0, r: 0, z: 0 });
        let mut visual = VisualPosition::at(initial_pos.to_world(&map));

        // Simulate teleport: server says we're at (10, 10, 0)
        let teleport_pos = Position::at_tile(Qrz { q: 10, r: 10, z: 0 });
        let teleport_world = teleport_pos.to_world(&map);
        visual.snap_to(teleport_world);

        assert!(visual.is_complete());
        assert_eq!(visual.current(), teleport_world);
    }

    #[test]
    fn test_smooth_correction() {
        // When server sends a small correction, VisualPosition interpolates
        let map = Map::new(qrz::Map::new(1.0, 0.8));

        // Client predicts we're at (1, 0, 0)
        let predicted = Position::new(Qrz { q: 1, r: 0, z: 0 }, Vec3::ZERO);
        let mut visual = VisualPosition::at(predicted.to_world(&map));

        // Server says we're actually at adjacent tile - smooth correction
        let corrected = Position::new(Qrz { q: 1, r: 0, z: 0 }, Vec3::new(0.1, 0.0, 0.0));
        let corrected_world = corrected.to_world(&map);
        visual.interpolate_toward(corrected_world, 0.125);

        // Should start from current position, not jump
        let pos_at_start = visual.current();
        let predicted_world = predicted.to_world(&map);
        assert!(
            (pos_at_start - predicted_world).length() < 0.001,
            "Correction should start from current visual position"
        );
    }
}
