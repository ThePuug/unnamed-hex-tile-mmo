//! Unified Position and Visual Interpolation Components
//!
//! This module implements the Unified Interpolation Model (ADR-019) which separates
//! authoritative position from visual interpolation.
//!
//! # Architecture
//!
//! ```text
//! Authoritative Layer:          Visual Layer:
//! ┌─────────────────────┐       ┌─────────────────────┐
//! │ Position            │       │ VisualPosition      │
//! │  - tile: Qrz        │──────▶│  - from: Vec3       │
//! │  - offset: Vec3     │       │  - to: Vec3         │
//! └─────────────────────┘       │  - progress: f32    │
//!                               └─────────────────────┘
//! ```
//!
//! # Key Insight
//!
//! When Position changes, VisualPosition starts interpolating from its current
//! visual location toward the new Position. This means direction changes don't
//! cause jitter - the visual smoothly continues from wherever it currently is.

use bevy::prelude::*;
use qrz::Qrz;
use serde::{Deserialize, Serialize};

/// Authoritative position in the game world.
///
/// Combines discrete tile location (Qrz hex coordinates) with continuous
/// sub-tile offset (Vec3). This is "where physics says the entity is."
///
/// # Fields
///
/// - `tile`: The hex tile the entity occupies (discrete)
/// - `offset`: Sub-tile offset from tile center (continuous, typically -0.5 to 0.5)
///
/// # Usage
///
/// - **Local player**: Updated by physics prediction, confirmed by server
/// - **Remote entities**: Updated by server messages
/// - **Both**: VisualPosition interpolates toward this
#[derive(Clone, Component, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Position {
    /// The hex tile this entity occupies
    pub tile: Qrz,
    /// Sub-tile offset from tile center (world units)
    pub offset: Vec3,
}

impl Position {
    /// Create a new position at the given tile with zero offset
    pub fn at_tile(tile: Qrz) -> Self {
        Self { tile, offset: Vec3::ZERO }
    }

    /// Create a new position with explicit tile and offset
    pub fn new(tile: Qrz, offset: Vec3) -> Self {
        Self { tile, offset }
    }

    /// Convert to world-space position using the map
    pub fn to_world(&self, map: &crate::common::resources::map::Map) -> Vec3 {
        use qrz::Convert;
        map.convert(self.tile) + self.offset
    }

    /// Create a Position from a world-space Vec3 using the map
    pub fn from_world(world_pos: Vec3, map: &crate::common::resources::map::Map) -> Self {
        use qrz::Convert;
        let tile = map.convert(world_pos);
        let tile_center = map.convert(tile);
        let offset = world_pos - tile_center;
        Self { tile, offset }
    }
}

/// Visual interpolation state for smooth rendering.
///
/// This component handles all visual smoothing, completely separate from
/// authoritative position. When Position changes, the rendering system
/// updates VisualPosition to interpolate from current visual location
/// toward the new authoritative position.
///
/// # Why This Fixes Jitter
///
/// Old system: physics updates `step`, then lerp from `prev_step` causes oscillation
/// New system: interpolation always starts from current visual position, never jumps
///
/// # Fields
///
/// - `from`: World-space position where interpolation started
/// - `to`: World-space position we're interpolating toward
/// - `progress`: 0.0 = at `from`, 1.0 = at `to`
/// - `duration`: Time in seconds for this interpolation
#[derive(Clone, Component, Copy, Debug)]
pub struct VisualPosition {
    /// World-space position where interpolation started
    pub from: Vec3,
    /// World-space position we're interpolating toward
    pub to: Vec3,
    /// Interpolation progress (0.0 to 1.0)
    pub progress: f32,
    /// Total duration for this interpolation in seconds
    pub duration: f32,
}

impl Default for VisualPosition {
    fn default() -> Self {
        Self {
            from: Vec3::ZERO,
            to: Vec3::ZERO,
            progress: 1.0, // Start complete (at destination)
            duration: 0.0,
        }
    }
}

impl VisualPosition {
    /// Create a new VisualPosition at a specific world position (no interpolation)
    pub fn at(position: Vec3) -> Self {
        Self {
            from: position,
            to: position,
            progress: 1.0,
            duration: 0.0,
        }
    }

    /// Start a new interpolation from current visual position toward target
    ///
    /// This is the key method that prevents jitter: we always start from
    /// wherever we currently appear, not from a physics-calculated position.
    pub fn interpolate_toward(&mut self, target: Vec3, duration: f32) {
        self.from = self.current();
        self.to = target;
        self.progress = 0.0;
        self.duration = duration.max(0.001); // Avoid division by zero
    }

    /// Get the current visual position (lerp between from and to)
    pub fn current(&self) -> Vec3 {
        self.from.lerp(self.to, self.progress.clamp(0.0, 1.0))
    }

    /// Advance the interpolation by delta time (in seconds)
    ///
    /// Returns true if interpolation is complete (progress >= 1.0)
    pub fn advance(&mut self, delta_seconds: f32) -> bool {
        if self.duration > 0.0 {
            self.progress += delta_seconds / self.duration;
        } else {
            self.progress = 1.0;
        }
        self.progress >= 1.0
    }

    /// Check if interpolation is complete
    pub fn is_complete(&self) -> bool {
        self.progress >= 1.0
    }

    /// Snap to a position immediately (no interpolation)
    pub fn snap_to(&mut self, position: Vec3) {
        self.from = position;
        self.to = position;
        self.progress = 1.0;
        self.duration = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== Position Tests =====

    #[test]
    fn test_position_at_tile() {
        let pos = Position::at_tile(Qrz { q: 5, r: 3, z: 0 });
        assert_eq!(pos.tile, Qrz { q: 5, r: 3, z: 0 });
        assert_eq!(pos.offset, Vec3::ZERO);
    }

    #[test]
    fn test_position_new() {
        let offset = Vec3::new(0.3, 0.5, -0.2);
        let pos = Position::new(Qrz { q: 1, r: 2, z: 3 }, offset);
        assert_eq!(pos.tile, Qrz { q: 1, r: 2, z: 3 });
        assert_eq!(pos.offset, offset);
    }

    #[test]
    fn test_position_default() {
        let pos = Position::default();
        assert_eq!(pos.tile, Qrz::default());
        assert_eq!(pos.offset, Vec3::ZERO);
    }

    // ===== VisualPosition Tests =====

    #[test]
    fn test_visual_position_default_is_complete() {
        let vis = VisualPosition::default();
        assert!(vis.is_complete());
        assert_eq!(vis.current(), Vec3::ZERO);
    }

    #[test]
    fn test_visual_position_at() {
        let pos = Vec3::new(1.0, 2.0, 3.0);
        let vis = VisualPosition::at(pos);
        assert!(vis.is_complete());
        assert_eq!(vis.current(), pos);
    }

    #[test]
    fn test_visual_position_interpolation_starts_from_current() {
        let mut vis = VisualPosition::at(Vec3::new(0.0, 0.0, 0.0));

        // Start interpolating toward (10, 0, 0) over 1 second
        vis.interpolate_toward(Vec3::new(10.0, 0.0, 0.0), 1.0);

        assert!(!vis.is_complete());
        assert_eq!(vis.progress, 0.0);
        assert_eq!(vis.from, Vec3::ZERO);
        assert_eq!(vis.to, Vec3::new(10.0, 0.0, 0.0));
    }

    #[test]
    fn test_visual_position_advance() {
        let mut vis = VisualPosition::at(Vec3::ZERO);
        vis.interpolate_toward(Vec3::new(10.0, 0.0, 0.0), 1.0);

        // Advance by 0.5 seconds (50%)
        vis.advance(0.5);
        assert!(!vis.is_complete());

        let current = vis.current();
        assert!((current.x - 5.0).abs() < 0.01, "Expected x=5.0, got {}", current.x);

        // Advance another 0.5 seconds (100%)
        let complete = vis.advance(0.5);
        assert!(complete);
        assert!(vis.is_complete());
    }

    #[test]
    fn test_visual_position_current_lerps_correctly() {
        let mut vis = VisualPosition {
            from: Vec3::new(0.0, 0.0, 0.0),
            to: Vec3::new(10.0, 20.0, 30.0),
            progress: 0.25,
            duration: 1.0,
        };

        let current = vis.current();
        assert!((current.x - 2.5).abs() < 0.01);
        assert!((current.y - 5.0).abs() < 0.01);
        assert!((current.z - 7.5).abs() < 0.01);
    }

    #[test]
    fn test_visual_position_direction_change_no_jump() {
        // This is the key test: direction change should not cause visual jump
        let mut vis = VisualPosition::at(Vec3::ZERO);

        // Start moving right
        vis.interpolate_toward(Vec3::new(10.0, 0.0, 0.0), 1.0);
        vis.advance(0.5); // Now at (5, 0, 0)

        let pos_before_direction_change = vis.current();
        assert!((pos_before_direction_change.x - 5.0).abs() < 0.01);

        // Change direction: now moving up instead
        vis.interpolate_toward(Vec3::new(5.0, 10.0, 0.0), 1.0);

        // Key assertion: from should be our current visual position, not some physics position
        assert!((vis.from.x - 5.0).abs() < 0.01, "from.x should be ~5.0 (current visual), got {}", vis.from.x);
        assert!((vis.from.y - 0.0).abs() < 0.01, "from.y should be ~0.0 (current visual), got {}", vis.from.y);

        // Immediately after direction change, visual position should not have jumped
        let pos_after_direction_change = vis.current();
        assert!((pos_after_direction_change.x - pos_before_direction_change.x).abs() < 0.01,
            "Visual position should not jump on direction change");
    }

    #[test]
    fn test_visual_position_snap_to() {
        let mut vis = VisualPosition::at(Vec3::ZERO);
        vis.interpolate_toward(Vec3::new(100.0, 0.0, 0.0), 10.0);
        vis.advance(0.1); // Partway through

        // Snap should immediately move to new position
        vis.snap_to(Vec3::new(50.0, 50.0, 50.0));

        assert!(vis.is_complete());
        assert_eq!(vis.current(), Vec3::new(50.0, 50.0, 50.0));
    }

    #[test]
    fn test_visual_position_zero_duration_completes_immediately() {
        let mut vis = VisualPosition::at(Vec3::ZERO);
        vis.interpolate_toward(Vec3::new(10.0, 0.0, 0.0), 0.0);

        // With zero duration, should complete immediately on any advance
        let complete = vis.advance(0.001);
        assert!(complete);
    }

    // ===== Invariant Tests =====

    #[test]
    fn test_progress_clamped_in_current() {
        // Even if progress exceeds 1.0, current() should clamp
        let vis = VisualPosition {
            from: Vec3::ZERO,
            to: Vec3::new(10.0, 0.0, 0.0),
            progress: 2.0, // Over 100%
            duration: 1.0,
        };

        let current = vis.current();
        assert_eq!(current, Vec3::new(10.0, 0.0, 0.0), "Progress > 1.0 should clamp to target");
    }

    #[test]
    fn test_negative_progress_clamped() {
        let vis = VisualPosition {
            from: Vec3::ZERO,
            to: Vec3::new(10.0, 0.0, 0.0),
            progress: -0.5,
            duration: 1.0,
        };

        let current = vis.current();
        assert_eq!(current, Vec3::ZERO, "Negative progress should clamp to from");
    }
}
