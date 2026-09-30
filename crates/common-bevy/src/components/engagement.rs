//! The engagement, a group of NPCs spawned together, and what tracks it.

use bevy::prelude::*;
use qrz::Qrz;

use crate::spatial_difficulty::EnemyArchetype;

/// A group of NPCs spawned together. It is cleaned up once every one is
/// dead, or no client has watched it for a while (`engagement_cleanup`).
#[derive(Component, Debug, Clone)]
pub struct Engagement {
    /// Location where engagement spawned
    pub spawn_location: Qrz,
    /// Level of its NPCs
    pub level: u8,
    /// Enemy archetype (determines abilities and attributes)
    pub archetype: EnemyArchetype,
    /// Number of NPCs in this engagement
    pub npc_count: u8,
    /// Child NPC entities (tracked for cleanup)
    pub spawned_npcs: Vec<Entity>,
}

impl Engagement {
    /// Create new engagement
    pub fn new(
        spawn_location: Qrz,
        level: u8,
        archetype: EnemyArchetype,
        npc_count: u8,
    ) -> Self {
        Self {
            spawn_location,
            level,
            archetype,
            npc_count,
            spawned_npcs: Vec::new(),
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
    fn test_engagement_creation() {
        use crate::spatial_difficulty::EnemyArchetype;

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
