use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::common::components::{reaction_queue::DamageType, Loc};

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
    /// Target's Loc at cast time (for movement and hit detection)
    pub target_loc: Loc,
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
        target_loc: Loc,
        speed: f32,
        damage_type: DamageType,
    ) -> Self {
        Self {
            source,
            damage,
            target_loc,
            speed,
            damage_type,
        }
    }

    /// Calculate target world position (tile center + chest height offset)
    pub fn target_world_pos(&self, map: &crate::common::resources::map::Map) -> Vec3 {
        use qrz::Convert;
        map.convert(*self.target_loc) + Vec3::new(0.0, 0.5, 0.0)
    }

    /// Calculate the distance remaining to target
    pub fn distance_to_target(&self, current_pos: Vec3, map: &crate::common::resources::map::Map) -> f32 {
        current_pos.distance(self.target_world_pos(map))
    }

    /// Check if the projectile has reached its target (within threshold)
    pub fn has_reached_target(&self, current_pos: Vec3, threshold: f32, map: &crate::common::resources::map::Map) -> bool {
        self.distance_to_target(current_pos, map) < threshold
    }

    /// Calculate the direction vector toward the target
    pub fn direction_to_target(&self, current_pos: Vec3, map: &crate::common::resources::map::Map) -> Vec3 {
        (self.target_world_pos(map) - current_pos).normalize_or_zero()
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
    use qrz::Qrz;

    fn create_test_map() -> crate::common::resources::map::Map {
        use crate::common::components::entity_type::*;
        crate::common::resources::map::Map::new(qrz::Map::new(1.0, 0.8))
    }

    #[test]
    fn test_projectile_creation() {
        // Test that we can create a projectile with all required fields
        let source = Entity::from_raw(1);
        let target_loc = Loc::from_qrz(5, 0, 0);
        let damage = 20.0;
        let speed = 4.0;
        let damage_type = DamageType::Physical;

        let projectile = Projectile::new(source, damage, target_loc, speed, damage_type);

        assert_eq!(projectile.source, source);
        assert_eq!(projectile.damage, damage);
        assert_eq!(projectile.target_loc, target_loc);
        assert_eq!(projectile.speed, speed);
        assert_eq!(projectile.damage_type, damage_type);
    }

    #[test]
    fn test_calculate_move_distance() {
        let source = Entity::from_raw(1);
        let target_loc = Loc::from_qrz(5, 0, 0);
        let projectile = Projectile::new(source, 20.0, target_loc, 4.0, DamageType::Physical);

        // At 4 hexes/second, in 0.5 seconds should move 2.0 world units
        let move_distance = projectile.calculate_move_distance(0.5);
        assert!((move_distance - 2.0).abs() < 0.001, "Should move 2.0 units in 0.5 seconds, got {}", move_distance);

        // At 4 hexes/second, in 1.0 seconds should move 4.0 world units
        let move_distance = projectile.calculate_move_distance(1.0);
        assert!((move_distance - 4.0).abs() < 0.001, "Should move 4.0 units in 1.0 seconds, got {}", move_distance);

        // At 8 hexes/second, in 0.125 seconds (one FixedUpdate tick) should move 1.0 world units
        let projectile_fast = Projectile::new(source, 20.0, target_loc, 8.0, DamageType::Physical);
        let move_distance = projectile_fast.calculate_move_distance(0.125);
        assert!((move_distance - 1.0).abs() < 0.001, "Should move 1.0 units in 0.125 seconds at 8 hexes/sec, got {}", move_distance);
    }
}
