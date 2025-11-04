use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::common::components::reaction_queue::DamageType;

/// Projectile component for entity-based projectiles (ADR-010 Phase 3)
///
/// Projectiles are entities that travel through space from a source to a target position.
/// They move at a constant speed and deal damage to entities at their position when they arrive.
///
/// **Mechanics:**
/// - Travel Speed: 4 hexes/second (configurable via speed field)
/// - Targeting: Snapshot of target location at cast time (stored in target_pos)
/// - Hit Detection: Damages entities at projectile position when it arrives
/// - Dodgeable: Entities can move off the targeted position during travel time
#[derive(Clone, Component, Copy, Debug, Deserialize, Serialize)]
pub struct Projectile {
    /// Entity that fired the projectile
    pub source: Entity,
    /// Base damage to deal on hit
    pub damage: f32,
    /// Snapshot of target location at cast time (world position)
    pub target_pos: Vec3,
    /// Travel speed in hexes per second
    pub speed: f32,
    /// Type of damage (Physical or Magic)
    pub damage_type: DamageType,
}

impl Projectile {
    /// Create a new projectile
    pub fn new(
        source: Entity,
        damage: f32,
        target_pos: Vec3,
        speed: f32,
        damage_type: DamageType,
    ) -> Self {
        Self {
            source,
            damage,
            target_pos,
            speed,
            damage_type,
        }
    }

    /// Calculate the distance remaining to target
    pub fn distance_to_target(&self, current_pos: Vec3) -> f32 {
        current_pos.distance(self.target_pos)
    }

    /// Check if the projectile has reached its target (within threshold)
    pub fn has_reached_target(&self, current_pos: Vec3, threshold: f32) -> bool {
        self.distance_to_target(current_pos) < threshold
    }

    /// Calculate the direction vector toward the target
    pub fn direction_to_target(&self, current_pos: Vec3) -> Vec3 {
        (self.target_pos - current_pos).normalize_or_zero()
    }

    /// Calculate how far the projectile should move in a given time delta
    ///
    /// # Arguments
    /// * `delta_seconds` - Time elapsed since last update in seconds
    ///
    /// # Returns
    /// Distance to move in world units
    pub fn calculate_move_distance(&self, delta_seconds: f32) -> f32 {
        // Convert speed (hexes/sec) to world units/sec
        // 1 hex ≈ 1.0 world units (from qrz library constants)
        const HEX_SIZE: f32 = 1.0;
        self.speed * HEX_SIZE * delta_seconds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_projectile_creation() {
        // Test that we can create a projectile with all required fields
        let source = Entity::from_raw(1);
        let target_pos = Vec3::new(10.0, 0.0, 5.0);
        let damage = 20.0;
        let speed = 4.0;
        let damage_type = DamageType::Physical;

        let projectile = Projectile::new(source, damage, target_pos, speed, damage_type);

        assert_eq!(projectile.source, source);
        assert_eq!(projectile.damage, damage);
        assert_eq!(projectile.target_pos, target_pos);
        assert_eq!(projectile.speed, speed);
        assert_eq!(projectile.damage_type, damage_type);
    }

    #[test]
    fn test_distance_to_target() {
        let source = Entity::from_raw(1);
        let target_pos = Vec3::new(10.0, 0.0, 0.0);
        let projectile = Projectile::new(source, 20.0, target_pos, 4.0, DamageType::Physical);

        let current_pos = Vec3::new(0.0, 0.0, 0.0);
        let distance = projectile.distance_to_target(current_pos);

        assert!((distance - 10.0).abs() < 0.001, "Distance should be 10.0, got {}", distance);
    }

    #[test]
    fn test_has_reached_target_within_threshold() {
        let source = Entity::from_raw(1);
        let target_pos = Vec3::new(10.0, 0.0, 0.0);
        let projectile = Projectile::new(source, 20.0, target_pos, 4.0, DamageType::Physical);

        // Position is 0.4 units away from target, threshold is 0.5
        let current_pos = Vec3::new(9.6, 0.0, 0.0);
        assert!(
            projectile.has_reached_target(current_pos, 0.5),
            "Projectile should be considered at target when within threshold"
        );
    }

    #[test]
    fn test_has_not_reached_target_outside_threshold() {
        let source = Entity::from_raw(1);
        let target_pos = Vec3::new(10.0, 0.0, 0.0);
        let projectile = Projectile::new(source, 20.0, target_pos, 4.0, DamageType::Physical);

        // Position is 1.0 units away from target, threshold is 0.5
        let current_pos = Vec3::new(9.0, 0.0, 0.0);
        assert!(
            !projectile.has_reached_target(current_pos, 0.5),
            "Projectile should not be considered at target when outside threshold"
        );
    }

    #[test]
    fn test_direction_to_target() {
        let source = Entity::from_raw(1);
        let target_pos = Vec3::new(10.0, 0.0, 0.0);
        let projectile = Projectile::new(source, 20.0, target_pos, 4.0, DamageType::Physical);

        let current_pos = Vec3::new(0.0, 0.0, 0.0);
        let direction = projectile.direction_to_target(current_pos);

        // Direction should be normalized unit vector along X axis
        assert!((direction.x - 1.0).abs() < 0.001, "Direction X should be 1.0, got {}", direction.x);
        assert!(direction.y.abs() < 0.001, "Direction Y should be 0.0, got {}", direction.y);
        assert!(direction.z.abs() < 0.001, "Direction Z should be 0.0, got {}", direction.z);
    }

    #[test]
    fn test_direction_to_target_diagonal() {
        let source = Entity::from_raw(1);
        let target_pos = Vec3::new(3.0, 0.0, 4.0);
        let projectile = Projectile::new(source, 20.0, target_pos, 4.0, DamageType::Physical);

        let current_pos = Vec3::new(0.0, 0.0, 0.0);
        let direction = projectile.direction_to_target(current_pos);

        // Direction should be normalized (magnitude = 1.0)
        let magnitude = (direction.x * direction.x + direction.y * direction.y + direction.z * direction.z).sqrt();
        assert!((magnitude - 1.0).abs() < 0.001, "Direction should be normalized, magnitude is {}", magnitude);

        // Direction should point toward target (3, 0, 4)
        // Normalized: (3/5, 0, 4/5) = (0.6, 0, 0.8)
        assert!((direction.x - 0.6).abs() < 0.001, "Direction X should be 0.6, got {}", direction.x);
        assert!(direction.y.abs() < 0.001, "Direction Y should be 0.0, got {}", direction.y);
        assert!((direction.z - 0.8).abs() < 0.001, "Direction Z should be 0.8, got {}", direction.z);
    }

    #[test]
    fn test_calculate_move_distance() {
        let source = Entity::from_raw(1);
        let target_pos = Vec3::new(10.0, 0.0, 0.0);
        let projectile = Projectile::new(source, 20.0, target_pos, 4.0, DamageType::Physical);

        // At 4 hexes/second, in 0.5 seconds should move 2.0 world units
        let move_distance = projectile.calculate_move_distance(0.5);
        assert!((move_distance - 2.0).abs() < 0.001, "Should move 2.0 units in 0.5 seconds, got {}", move_distance);

        // At 4 hexes/second, in 1.0 seconds should move 4.0 world units
        let move_distance = projectile.calculate_move_distance(1.0);
        assert!((move_distance - 4.0).abs() < 0.001, "Should move 4.0 units in 1.0 seconds, got {}", move_distance);

        // At 4 hexes/second, in 0.125 seconds (one FixedUpdate tick) should move 0.5 world units
        let move_distance = projectile.calculate_move_distance(0.125);
        assert!((move_distance - 0.5).abs() < 0.001, "Should move 0.5 units in 0.125 seconds, got {}", move_distance);
    }

    #[test]
    fn test_direction_at_target_returns_zero() {
        // When projectile is already at target, direction should be zero
        let source = Entity::from_raw(1);
        let target_pos = Vec3::new(10.0, 0.0, 0.0);
        let projectile = Projectile::new(source, 20.0, target_pos, 4.0, DamageType::Physical);

        let direction = projectile.direction_to_target(target_pos);

        // normalize_or_zero() should return Vec3::ZERO when at target
        assert!(direction.length() < 0.001, "Direction should be zero when at target, got length {}", direction.length());
    }
}
