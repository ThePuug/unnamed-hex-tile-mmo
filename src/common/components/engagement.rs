//! # Engagement System Components (ADR-014)
//!
//! Dynamic enemy encounters that spawn when players explore new chunks.
//! Replaces static spawners with exploration-driven content discovery.

use bevy::prelude::*;
use qrz::Qrz;
use serde::{Deserialize, Serialize};

use crate::common::spatial_difficulty::EnemyArchetype;

/// Zone ID for budget tracking (240-tile zones)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ZoneId(pub i32, pub i32);

impl ZoneId {
    /// Zone radius in tiles
    pub const ZONE_RADIUS: i32 = 240;

    /// Calculate zone ID from position (240-tile zones)
    pub fn from_position(pos: Qrz) -> Self {
        ZoneId(
            pos.q as i32 / Self::ZONE_RADIUS,
            pos.r as i32 / Self::ZONE_RADIUS,
        )
    }
}

/// Engagement parent entity - manages a group of NPCs spawned together
///
/// Lifecycle:
/// 1. Spawn engagement entity when chunk received (if validation passes)
/// 2. Spawn 1-3 NPC entities as children
/// 3. Monitor NPCs (track deaths, player proximity)
/// 4. Cleanup when all NPCs killed or abandoned (no players within 100 tiles for 60s)
#[derive(Component, Debug, Clone)]
pub struct Engagement {
    /// Location where engagement spawned
    pub spawn_location: Qrz,
    /// Enemy level (0-10) based on distance from haven
    pub level: u8,
    /// Enemy archetype (determines abilities and attributes)
    pub archetype: EnemyArchetype,
    /// Number of NPCs in this engagement (1-3)
    pub npc_count: u8,
    /// Child NPC entities (tracked for cleanup)
    pub spawned_npcs: Vec<Entity>,
    /// Zone ID for budget tracking
    pub zone_id: ZoneId,
}

impl Engagement {
    /// Create new engagement
    pub fn new(
        spawn_location: Qrz,
        level: u8,
        archetype: EnemyArchetype,
        npc_count: u8,
    ) -> Self {
        let zone_id = ZoneId::from_position(spawn_location);
        Self {
            spawn_location,
            level,
            archetype,
            npc_count,
            spawned_npcs: Vec::new(),
            zone_id,
        }
    }

    /// Add NPC entity to tracking list
    pub fn add_npc(&mut self, entity: Entity) {
        self.spawned_npcs.push(entity);
    }
}

/// Marker component for NPCs that belong to an engagement
/// Back-reference to parent engagement entity
#[derive(Component, Debug, Clone, Copy)]
pub struct EngagementMember(pub Entity);

/// Last time players were near this engagement (for abandonment tracking)
#[derive(Component, Debug, Clone, Copy)]
pub struct LastPlayerProximity {
    /// Game time when a player was last within proximity range
    pub last_seen: std::time::Duration,
}

impl LastPlayerProximity {
    pub fn new(current_time: std::time::Duration) -> Self {
        Self {
            last_seen: current_time,
        }
    }

    /// Update last seen time
    pub fn update(&mut self, current_time: std::time::Duration) {
        self.last_seen = current_time;
    }

    /// Check if abandoned (no players for given duration)
    pub fn is_abandoned(&self, current_time: std::time::Duration, abandonment_duration: std::time::Duration) -> bool {
        current_time.saturating_sub(self.last_seen) >= abandonment_duration
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zone_id_from_position() {
        // Origin should be zone (0, 0)
        let pos = Qrz { q: 0, r: 0, z: 0 };
        assert_eq!(ZoneId::from_position(pos), ZoneId(0, 0));

        // 150 tiles away should still be zone (0, 0)
        let pos = Qrz { q: 150, r: 0, z: 0 };
        assert_eq!(ZoneId::from_position(pos), ZoneId(0, 0));

        // 240 tiles away should be zone (1, 0)
        let pos = Qrz { q: 240, r: 0, z: 0 };
        assert_eq!(ZoneId::from_position(pos), ZoneId(1, 0));

        // Negative coordinates
        let pos = Qrz { q: -240, r: 0, z: 0 };
        assert_eq!(ZoneId::from_position(pos), ZoneId(-1, 0));

        // Both axes
        let pos = Qrz { q: 480, r: -720, z: 0 };
        assert_eq!(ZoneId::from_position(pos), ZoneId(2, -3));
    }

    #[test]
    fn test_engagement_creation() {
        use crate::common::spatial_difficulty::EnemyArchetype;

        let spawn = Qrz { q: 30, r: 0, z: 0 };
        let engagement = Engagement::new(
            spawn,
            3, // level 3
            EnemyArchetype::Berserker,
            2, // 2 NPCs
        );

        assert_eq!(engagement.spawn_location, spawn);
        assert_eq!(engagement.level, 3);
        assert_eq!(engagement.archetype, EnemyArchetype::Berserker);
        assert_eq!(engagement.npc_count, 2);
        assert_eq!(engagement.spawned_npcs.len(), 0); // Empty initially
        assert_eq!(engagement.zone_id, ZoneId(0, 0)); // 30 tiles is within zone (0, 0)
    }

    #[test]
    fn test_last_player_proximity_abandonment() {
        use std::time::Duration;

        let start_time = Duration::from_secs(100);
        let mut proximity = LastPlayerProximity::new(start_time);

        // Not abandoned immediately
        assert!(!proximity.is_abandoned(start_time, Duration::from_secs(60)));

        // Not abandoned after 30 seconds
        let later = start_time + Duration::from_secs(30);
        assert!(!proximity.is_abandoned(later, Duration::from_secs(60)));

        // Abandoned after 60 seconds
        let much_later = start_time + Duration::from_secs(60);
        assert!(proximity.is_abandoned(much_later, Duration::from_secs(60)));

        // Update proximity - no longer abandoned
        proximity.update(much_later);
        assert!(!proximity.is_abandoned(much_later, Duration::from_secs(60)));
    }
}
