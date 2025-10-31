use bevy::prelude::*;
use std::time::Duration;
use serde::{Deserialize, Serialize};

use crate::common::systems::combat::gcd::GcdType;

/// Global Cooldown component for tracking ability cooldowns
/// Shared between players and NPCs for uniform cooldown validation
#[derive(Component, Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Gcd {
    pub gcd_type: Option<GcdType>,  // None = no GCD active
    pub expires_at: Duration,       // Time::elapsed() when GCD ends
}

impl Default for Gcd {
    fn default() -> Self {
        Self {
            gcd_type: None,
            expires_at: Duration::ZERO,
        }
    }
}

impl Gcd {
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if GCD is currently active (now < expires_at)
    pub fn is_active(&self, now: Duration) -> bool {
        now < self.expires_at
    }

    /// Activate GCD with given type and duration
    pub fn activate(&mut self, gcd_type: GcdType, duration: Duration, now: Duration) {
        self.gcd_type = Some(gcd_type);
        self.expires_at = now + duration;
    }

    /// Clear GCD (set to inactive)
    pub fn clear(&mut self) {
        self.gcd_type = None;
        self.expires_at = Duration::ZERO;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gcd_new_is_inactive() {
        let gcd = Gcd::new();
        assert_eq!(gcd.gcd_type, None);
        assert_eq!(gcd.expires_at, Duration::ZERO);
        assert!(!gcd.is_active(Duration::ZERO));
    }

    #[test]
    fn test_gcd_is_active_when_not_expired() {
        let mut gcd = Gcd::new();
        let now = Duration::from_millis(1000);
        let duration = Duration::from_millis(500);

        gcd.activate(GcdType::Attack, duration, now);

        // At now + 250ms (halfway through), should be active
        let check_time = Duration::from_millis(1250);
        assert!(gcd.is_active(check_time));
    }

    #[test]
    fn test_gcd_is_inactive_after_expiry() {
        let mut gcd = Gcd::new();
        let now = Duration::from_millis(1000);
        let duration = Duration::from_millis(500);

        gcd.activate(GcdType::Attack, duration, now);

        // At now + 600ms (past expiry), should be inactive
        let check_time = Duration::from_millis(1600);
        assert!(!gcd.is_active(check_time));
    }

    #[test]
    fn test_gcd_is_inactive_exactly_at_expiry() {
        let mut gcd = Gcd::new();
        let now = Duration::from_millis(1000);
        let duration = Duration::from_millis(500);

        gcd.activate(GcdType::Attack, duration, now);

        // Exactly at expiry time (now + duration), should be inactive
        // Because is_active uses `<`, not `<=`
        let check_time = Duration::from_millis(1500);
        assert!(!gcd.is_active(check_time));
    }

    #[test]
    fn test_gcd_activate_sets_correct_expiry() {
        let mut gcd = Gcd::new();
        let now = Duration::from_millis(2000);
        let duration = Duration::from_millis(1500);

        gcd.activate(GcdType::Attack, duration, now);

        assert_eq!(gcd.gcd_type, Some(GcdType::Attack));
        assert_eq!(gcd.expires_at, Duration::from_millis(3500));
    }

    #[test]
    fn test_gcd_clear_resets_state() {
        let mut gcd = Gcd::new();
        let now = Duration::from_millis(1000);

        gcd.activate(GcdType::Attack, Duration::from_millis(500), now);
        assert!(gcd.is_active(Duration::from_millis(1200)));

        gcd.clear();

        assert_eq!(gcd.gcd_type, None);
        assert_eq!(gcd.expires_at, Duration::ZERO);
        assert!(!gcd.is_active(Duration::from_millis(1200)));
    }

    #[test]
    fn test_gcd_can_be_reactivated_after_clear() {
        let mut gcd = Gcd::new();

        // First activation
        gcd.activate(GcdType::Attack, Duration::from_millis(500), Duration::from_millis(1000));
        assert!(gcd.is_active(Duration::from_millis(1200)));

        // Clear
        gcd.clear();
        assert!(!gcd.is_active(Duration::from_millis(1200)));

        // Second activation (should work)
        gcd.activate(GcdType::Attack, Duration::from_millis(300), Duration::from_millis(2000));
        assert!(gcd.is_active(Duration::from_millis(2100)));
        assert_eq!(gcd.expires_at, Duration::from_millis(2300));
    }

    #[test]
    fn test_gcd_overwrite_active_gcd() {
        let mut gcd = Gcd::new();

        // First activation
        gcd.activate(GcdType::Attack, Duration::from_millis(1000), Duration::from_millis(1000));
        assert_eq!(gcd.expires_at, Duration::from_millis(2000));

        // Immediately activate again (overwrite)
        gcd.activate(GcdType::Attack, Duration::from_millis(500), Duration::from_millis(1100));
        assert_eq!(gcd.expires_at, Duration::from_millis(1600));
    }
}
