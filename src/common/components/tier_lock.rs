//! Tier Lock Component
//!
//! Tracks tier lock state for players, allowing them to lock targeting to a specific range tier.
//! Part of ADR-010 Phase 1: Combat Variety.
//!
//! This component is REPLICATED - it represents server-authoritative targeting constraints.
//! UI state (last_target) belongs in the Target component, not here.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::common::systems::targeting::RangeTier;

/// Tier lock component that tracks targeting tier constraints
/// Only players have this component - NPCs use automatic targeting
#[derive(Clone, Component, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TierLock {
    /// The tier the player has locked to, if any
    pub tier: Option<RangeTier>,
}

impl Default for TierLock {
    fn default() -> Self {
        Self { tier: None }
    }
}

impl TierLock {
    /// Create a new TierLock in unlocked state
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the tier lock
    pub fn set(&mut self, tier: RangeTier) {
        self.tier = Some(tier);
    }

    /// Clear the tier lock (reset to automatic targeting)
    pub fn clear(&mut self) {
        self.tier = None;
    }

    /// Check if currently tier locked
    pub fn is_locked(&self) -> bool {
        self.tier.is_some()
    }

    /// Check if unlocked (automatic targeting)
    pub fn is_unlocked(&self) -> bool {
        self.tier.is_none()
    }

    /// Get the current tier lock, if any
    pub fn get(&self) -> Option<RangeTier> {
        self.tier
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_unlocked() {
        let lock = TierLock::default();
        assert_eq!(lock.tier, None);
        assert!(!lock.is_locked());
        assert!(lock.is_unlocked());
        assert_eq!(lock.get(), None);
    }

    #[test]
    fn test_new_is_unlocked() {
        let lock = TierLock::new();
        assert_eq!(lock.tier, None);
        assert!(!lock.is_locked());
        assert!(lock.is_unlocked());
    }

    #[test]
    fn test_set_tier_lock_close() {
        let mut lock = TierLock::new();
        lock.set(RangeTier::Close);

        assert_eq!(lock.tier, Some(RangeTier::Close));
        assert!(lock.is_locked());
        assert!(!lock.is_unlocked());
        assert_eq!(lock.get(), Some(RangeTier::Close));
    }

    #[test]
    fn test_set_tier_lock_mid() {
        let mut lock = TierLock::new();
        lock.set(RangeTier::Mid);

        assert_eq!(lock.tier, Some(RangeTier::Mid));
        assert!(lock.is_locked());
        assert_eq!(lock.get(), Some(RangeTier::Mid));
    }

    #[test]
    fn test_set_tier_lock_far() {
        let mut lock = TierLock::new();
        lock.set(RangeTier::Far);

        assert_eq!(lock.tier, Some(RangeTier::Far));
        assert!(lock.is_locked());
        assert_eq!(lock.get(), Some(RangeTier::Far));
    }

    #[test]
    fn test_clear_tier_lock() {
        let mut lock = TierLock::new();
        lock.set(RangeTier::Close);
        assert!(lock.is_locked());

        lock.clear();
        assert!(lock.is_unlocked());
        assert!(!lock.is_locked());
        assert_eq!(lock.get(), None);
    }

    #[test]
    fn test_tier_lock_can_be_changed() {
        let mut lock = TierLock::new();

        // Lock to Close
        lock.set(RangeTier::Close);
        assert_eq!(lock.get(), Some(RangeTier::Close));

        // Change to Mid
        lock.set(RangeTier::Mid);
        assert_eq!(lock.get(), Some(RangeTier::Mid));

        // Change to Far
        lock.set(RangeTier::Far);
        assert_eq!(lock.get(), Some(RangeTier::Far));
    }
}
