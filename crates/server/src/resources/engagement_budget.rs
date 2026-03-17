//! # Engagement Budget System (ADR-014)
//!
//! Tracks active engagements per zone to prevent overwhelming encounter density.
//! Max 8 engagements per 240-tile zone.

use bevy::prelude::*;
use std::collections::HashMap;

use common_bevy::components::engagement::ZoneId;

/// Maximum engagements allowed per zone
pub const MAX_ENGAGEMENTS_PER_ZONE: usize = 8;

/// Resource tracking active engagement counts per zone
/// Prevents overwhelming encounter density through budget system
#[derive(Resource, Default, Debug)]
pub struct EngagementBudget {
    /// Map of zone_id → count of active engagements
    active_per_zone: HashMap<ZoneId, usize>,
}

impl EngagementBudget {
    /// Check if zone can accept another engagement
    pub fn can_spawn_in_zone(&self, zone_id: ZoneId) -> bool {
        self.active_per_zone.get(&zone_id).unwrap_or(&0) < &MAX_ENGAGEMENTS_PER_ZONE
    }

    /// Register a new engagement in zone (increment counter)
    pub fn register_engagement(&mut self, zone_id: ZoneId) {
        *self.active_per_zone.entry(zone_id).or_insert(0) += 1;
    }

    /// Unregister an engagement from zone (decrement counter, remove if zero)
    pub fn unregister_engagement(&mut self, zone_id: ZoneId) {
        if let Some(count) = self.active_per_zone.get_mut(&zone_id) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.active_per_zone.remove(&zone_id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_budget_empty() {
        let budget = EngagementBudget::default();
        let zone = ZoneId(0, 0);

        assert!(budget.can_spawn_in_zone(zone));
    }

    #[test]
    fn test_budget_register() {
        let mut budget = EngagementBudget::default();
        let zone = ZoneId(0, 0);

        budget.register_engagement(zone);
        assert!(budget.can_spawn_in_zone(zone));

        budget.register_engagement(zone);
        assert!(budget.can_spawn_in_zone(zone));
    }

    #[test]
    fn test_budget_unregister() {
        let mut budget = EngagementBudget::default();
        let zone = ZoneId(0, 0);

        budget.register_engagement(zone);
        budget.register_engagement(zone);

        budget.unregister_engagement(zone);
        budget.unregister_engagement(zone);

        budget.unregister_engagement(zone);
    }

    #[test]
    fn test_budget_max_limit() {
        let mut budget = EngagementBudget::default();
        let zone = ZoneId(0, 0);

        for _ in 0..MAX_ENGAGEMENTS_PER_ZONE {
            assert!(budget.can_spawn_in_zone(zone));
            budget.register_engagement(zone);
        }

        assert!(!budget.can_spawn_in_zone(zone));

        budget.unregister_engagement(zone);
        assert!(budget.can_spawn_in_zone(zone));
    }

    #[test]
    fn test_budget_multiple_zones() {
        let mut budget = EngagementBudget::default();
        let zone1 = ZoneId(0, 0);
        let zone2 = ZoneId(1, 0);
        let zone3 = ZoneId(0, 1);

        budget.register_engagement(zone1);
        budget.register_engagement(zone1);
        budget.register_engagement(zone2);
        budget.register_engagement(zone3);

        budget.unregister_engagement(zone1);
    }
}
