//! Targeting State Component
//!
//! Tracks the targeting mode for entities, allowing tier lock functionality.
//! Part of ADR-010 Phase 1: Combat Variety.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::common::systems::targeting::RangeTier;

/// Targeting state component that tracks how an entity selects targets
#[derive(Clone, Component, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TargetingState {
    pub mode: TargetingMode,
    pub last_target: Option<Entity>,
}

/// The targeting mode determines how targets are selected
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum TargetingMode {
    /// Default: select nearest target in facing direction
    Automatic,
    /// Locked to a specific tier (Close/Mid/Far) until ability use
    TierLocked(RangeTier),
    /// Manually locked to a specific entity (TAB cycling - future feature)
    ManualLocked(Entity),
}

impl Default for TargetingState {
    fn default() -> Self {
        Self {
            mode: TargetingMode::Automatic,
            last_target: None,
        }
    }
}

impl TargetingState {
    /// Create a new TargetingState in Automatic mode
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the targeting mode to tier locked
    pub fn set_tier_lock(&mut self, tier: RangeTier) {
        self.mode = TargetingMode::TierLocked(tier);
    }

    /// Reset to automatic targeting (called after ability use)
    pub fn reset_to_automatic(&mut self) {
        self.mode = TargetingMode::Automatic;
    }

    /// Check if currently tier locked
    pub fn is_tier_locked(&self) -> bool {
        matches!(self.mode, TargetingMode::TierLocked(_))
    }

    /// Get the current tier lock, if any
    pub fn get_tier_lock(&self) -> Option<RangeTier> {
        match self.mode {
            TargetingMode::TierLocked(tier) => Some(tier),
            _ => None,
        }
    }

    /// Check if in automatic mode
    pub fn is_automatic(&self) -> bool {
        matches!(self.mode, TargetingMode::Automatic)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_automatic() {
        let state = TargetingState::default();
        assert_eq!(state.mode, TargetingMode::Automatic);
        assert!(state.is_automatic());
        assert!(!state.is_tier_locked());
        assert_eq!(state.get_tier_lock(), None);
    }

    #[test]
    fn test_set_tier_lock_close() {
        let mut state = TargetingState::new();
        state.set_tier_lock(RangeTier::Close);

        assert_eq!(state.mode, TargetingMode::TierLocked(RangeTier::Close));
        assert!(state.is_tier_locked());
        assert!(!state.is_automatic());
        assert_eq!(state.get_tier_lock(), Some(RangeTier::Close));
    }

    #[test]
    fn test_set_tier_lock_mid() {
        let mut state = TargetingState::new();
        state.set_tier_lock(RangeTier::Mid);

        assert_eq!(state.mode, TargetingMode::TierLocked(RangeTier::Mid));
        assert!(state.is_tier_locked());
        assert_eq!(state.get_tier_lock(), Some(RangeTier::Mid));
    }

    #[test]
    fn test_set_tier_lock_far() {
        let mut state = TargetingState::new();
        state.set_tier_lock(RangeTier::Far);

        assert_eq!(state.mode, TargetingMode::TierLocked(RangeTier::Far));
        assert!(state.is_tier_locked());
        assert_eq!(state.get_tier_lock(), Some(RangeTier::Far));
    }

    #[test]
    fn test_reset_to_automatic() {
        let mut state = TargetingState::new();
        state.set_tier_lock(RangeTier::Close);
        assert!(state.is_tier_locked());

        state.reset_to_automatic();
        assert!(state.is_automatic());
        assert!(!state.is_tier_locked());
        assert_eq!(state.get_tier_lock(), None);
    }

    #[test]
    fn test_tier_lock_can_be_changed() {
        let mut state = TargetingState::new();

        // Lock to Close
        state.set_tier_lock(RangeTier::Close);
        assert_eq!(state.get_tier_lock(), Some(RangeTier::Close));

        // Change to Mid
        state.set_tier_lock(RangeTier::Mid);
        assert_eq!(state.get_tier_lock(), Some(RangeTier::Mid));

        // Change to Far
        state.set_tier_lock(RangeTier::Far);
        assert_eq!(state.get_tier_lock(), Some(RangeTier::Far));
    }

    #[test]
    fn test_last_target_tracking() {
        let mut state = TargetingState::new();
        assert_eq!(state.last_target, None);

        let entity = Entity::from_raw(42);
        state.last_target = Some(entity);
        assert_eq!(state.last_target, Some(entity));
    }
}
