use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::common::message::AbilityType;

/// Universal ability lockout timer (single component per player)
/// When ANY ability is used, ALL abilities are locked for the recovery duration.
/// Synergies can allow specific abilities to unlock early (see SynergyUnlock).
#[derive(Component, Clone, Copy, Debug, Serialize, Deserialize)]
pub struct GlobalRecovery {
    pub remaining: f32,              // Seconds until ALL abilities unlock
    pub duration: f32,               // Total duration of current lockout
    pub triggered_by: AbilityType,   // Which ability triggered this lockout
}

impl GlobalRecovery {
    pub fn new(duration: f32, triggered_by: AbilityType) -> Self {
        Self {
            remaining: duration,
            duration,
            triggered_by,
        }
    }

    /// Check if lockout is still active (remaining > 0)
    pub fn is_active(&self) -> bool {
        self.remaining > 0.0
    }

    /// Tick the recovery timer (subtract delta time)
    pub fn tick(&mut self, delta: f32) {
        self.remaining = (self.remaining - delta).max(0.0);
    }
}

/// Marks an ability as synergy-available (glowing, can use early during lockout)
/// Multiple SynergyUnlock components can exist per player (one per synergized ability)
#[derive(Component, Clone, Copy, Debug, Serialize, Deserialize)]
pub struct SynergyUnlock {
    pub ability: AbilityType,         // Which ability can unlock early
    pub unlock_at: f32,               // Lockout time when this ability becomes available
    pub triggered_by: AbilityType,    // Which ability triggered this synergy
}

impl SynergyUnlock {
    pub fn new(ability: AbilityType, unlock_at: f32, triggered_by: AbilityType) -> Self {
        Self {
            ability,
            unlock_at,
            triggered_by,
        }
    }

    /// Check if this ability can be used now (lockout remaining <= unlock_at)
    pub fn is_unlocked(&self, lockout_remaining: f32) -> bool {
        lockout_remaining <= self.unlock_at
    }
}

/// Get the recovery duration for an ability (universal lockout time)
/// These are MVP values from ADR-012
pub fn get_ability_recovery_duration(ability: AbilityType) -> f32 {
    match ability {
        AbilityType::Lunge => 2.0,      // Gap closer: 2s lockout
        AbilityType::Overpower => 3.0,  // Heavy strike: 3s lockout
        AbilityType::Knockback => 1.0,  // Push: 1s lockout
        AbilityType::Deflect => 1.0,    // Defensive: 1s lockout
        AbilityType::AutoAttack => 0.0, // AutoAttack uses its own timer, not GlobalRecovery
        AbilityType::Volley => 4.0,     // NPC ranged: 4s lockout
        AbilityType::Counter => 1.5,    // ADR-014: Counter-attack: 1.5s lockout
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== GlobalRecovery Tests =====

    #[test]
    fn test_global_recovery_new() {
        let recovery = GlobalRecovery::new(1.5, AbilityType::Lunge);
        assert_eq!(recovery.remaining, 1.5);
        assert_eq!(recovery.duration, 1.5);
        assert_eq!(recovery.triggered_by, AbilityType::Lunge);
        assert!(recovery.is_active());
    }

    #[test]
    fn test_global_recovery_is_active() {
        let recovery = GlobalRecovery::new(1.0, AbilityType::Overpower);
        assert!(recovery.is_active(), "Should be active with remaining > 0");

        let recovery = GlobalRecovery {
            remaining: 0.0,
            duration: 1.0,
            triggered_by: AbilityType::Overpower,
        };
        assert!(!recovery.is_active(), "Should be inactive when remaining == 0");
    }

    #[test]
    fn test_global_recovery_tick() {
        let mut recovery = GlobalRecovery::new(1.0, AbilityType::Knockback);

        // Tick by 0.3s
        recovery.tick(0.3);
        assert!((recovery.remaining - 0.7).abs() < 0.001, "Should be 0.7s remaining");
        assert!(recovery.is_active());

        // Tick by another 0.5s
        recovery.tick(0.5);
        assert!((recovery.remaining - 0.2).abs() < 0.001, "Should be 0.2s remaining");
        assert!(recovery.is_active());

        // Tick by 0.5s (overshoots, should clamp to 0)
        recovery.tick(0.5);
        assert_eq!(recovery.remaining, 0.0, "Should be clamped to 0");
        assert!(!recovery.is_active());
    }

    #[test]
    fn test_global_recovery_tick_does_not_go_negative() {
        let mut recovery = GlobalRecovery::new(0.5, AbilityType::Deflect);

        // Tick by more than remaining (should clamp to 0, not go negative)
        recovery.tick(1.0);
        assert_eq!(recovery.remaining, 0.0, "Should be clamped to 0, not negative");
        assert!(!recovery.is_active());
    }

    #[test]
    fn test_global_recovery_preserves_triggered_by() {
        let mut recovery = GlobalRecovery::new(2.0, AbilityType::Overpower);

        recovery.tick(1.5);
        assert_eq!(recovery.triggered_by, AbilityType::Overpower, "Should preserve triggered_by during ticking");

        recovery.tick(0.5);
        assert_eq!(recovery.triggered_by, AbilityType::Overpower, "Should preserve triggered_by even after expiring");
    }

    // ===== SynergyUnlock Tests =====

    #[test]
    fn test_synergy_unlock_new() {
        let synergy = SynergyUnlock::new(
            AbilityType::Overpower,
            0.5,
            AbilityType::Lunge,
        );

        assert_eq!(synergy.ability, AbilityType::Overpower);
        assert_eq!(synergy.unlock_at, 0.5);
        assert_eq!(synergy.triggered_by, AbilityType::Lunge);
    }

    #[test]
    fn test_synergy_unlock_is_unlocked() {
        let synergy = SynergyUnlock::new(
            AbilityType::Overpower,
            0.5,  // Unlocks when lockout remaining <= 0.5s
            AbilityType::Lunge,
        );

        // Lockout just started (1.0s remaining) - not unlocked yet
        assert!(!synergy.is_unlocked(1.0), "Should not be unlocked at 1.0s remaining");

        // Lockout at 0.7s remaining - not unlocked yet
        assert!(!synergy.is_unlocked(0.7), "Should not be unlocked at 0.7s remaining");

        // Lockout at 0.5s remaining - exactly at unlock time
        assert!(synergy.is_unlocked(0.5), "Should be unlocked at exactly 0.5s remaining");

        // Lockout at 0.3s remaining - unlocked
        assert!(synergy.is_unlocked(0.3), "Should be unlocked at 0.3s remaining");

        // Lockout at 0.0s remaining - unlocked
        assert!(synergy.is_unlocked(0.0), "Should be unlocked at 0.0s remaining");
    }

    #[test]
    fn test_synergy_unlock_immediate() {
        // Synergy that unlocks immediately (unlock_at = lockout duration)
        let synergy = SynergyUnlock::new(
            AbilityType::Knockback,
            2.0,  // Unlocks when lockout remaining <= 2.0s (which is immediately for a 2s lockout)
            AbilityType::Overpower,
        );

        // Lockout just started (2.0s remaining) - should be unlocked immediately
        assert!(synergy.is_unlocked(2.0), "Should be unlocked immediately at 2.0s remaining");
    }

    #[test]
    fn test_synergy_unlock_never_unlocks_early() {
        // Synergy with unlock_at = 0 means it only unlocks when lockout expires
        let synergy = SynergyUnlock::new(
            AbilityType::Deflect,
            0.0,  // Only unlocks at 0s remaining
            AbilityType::Knockback,
        );

        assert!(!synergy.is_unlocked(0.5), "Should not be unlocked at 0.5s remaining");
        assert!(!synergy.is_unlocked(0.1), "Should not be unlocked at 0.1s remaining");
        assert!(synergy.is_unlocked(0.0), "Should only be unlocked at 0.0s remaining");
    }

    // ===== Ability Recovery Duration Tests =====

    #[test]
    fn test_ability_recovery_durations() {
        // Updated values for slower combat pace
        assert_eq!(get_ability_recovery_duration(AbilityType::Lunge), 2.0);
        assert_eq!(get_ability_recovery_duration(AbilityType::Overpower), 3.0);
        assert_eq!(get_ability_recovery_duration(AbilityType::Knockback), 1.0);
        assert_eq!(get_ability_recovery_duration(AbilityType::Deflect), 1.0);
        assert_eq!(get_ability_recovery_duration(AbilityType::AutoAttack), 0.0); // Uses own timer
        assert_eq!(get_ability_recovery_duration(AbilityType::Volley), 4.0);
        assert_eq!(get_ability_recovery_duration(AbilityType::Counter), 1.5);
    }

    // ===== Integration Tests =====

    #[test]
    fn test_lunge_to_overpower_synergy_flow() {
        // Simulate Lunge → Overpower synergy flow
        // Lunge has 1.0s lockout, Overpower unlocks 0.5s early

        // 1. Use Lunge → 1s lockout starts
        let mut recovery = GlobalRecovery::new(1.0, AbilityType::Lunge);

        // 2. Synergy detected immediately → Overpower can unlock at 0.5s
        let synergy = SynergyUnlock::new(AbilityType::Overpower, 0.5, AbilityType::Lunge);

        // 3. At t=0.0s: Overpower not unlocked yet
        assert!(!synergy.is_unlocked(recovery.remaining));

        // 4. Tick to t=0.4s (0.6s remaining)
        recovery.tick(0.4);
        assert!(!synergy.is_unlocked(recovery.remaining), "Overpower should not be unlocked at 0.6s remaining");

        // 5. Tick to t=0.5s (0.5s remaining) - synergy unlocks!
        recovery.tick(0.1);
        assert!(synergy.is_unlocked(recovery.remaining), "Overpower should be unlocked at 0.5s remaining");

        // 6. Tick to t=1.0s (0.0s remaining) - full recovery
        recovery.tick(0.5);
        assert!(!recovery.is_active(), "Lockout should be expired");
        assert!(synergy.is_unlocked(recovery.remaining), "Overpower should still be unlocked");
    }

    #[test]
    fn test_overpower_to_knockback_synergy_flow() {
        // Simulate Overpower → Knockback synergy flow
        // Overpower has 2.0s lockout, Knockback unlocks 1.0s early

        // 1. Use Overpower → 2s lockout starts
        let mut recovery = GlobalRecovery::new(2.0, AbilityType::Overpower);

        // 2. Synergy detected → Knockback can unlock at 1.0s
        let synergy = SynergyUnlock::new(AbilityType::Knockback, 1.0, AbilityType::Overpower);

        // 3. At t=0.0s: Knockback not unlocked yet
        assert!(!synergy.is_unlocked(recovery.remaining));

        // 4. Tick to t=0.5s (1.5s remaining)
        recovery.tick(0.5);
        assert!(!synergy.is_unlocked(recovery.remaining), "Knockback should not be unlocked at 1.5s remaining");

        // 5. Tick to t=1.0s (1.0s remaining) - synergy unlocks!
        recovery.tick(0.5);
        assert!(synergy.is_unlocked(recovery.remaining), "Knockback should be unlocked at 1.0s remaining");

        // 6. Tick to t=2.0s (0.0s remaining) - full recovery
        recovery.tick(1.0);
        assert!(!recovery.is_active(), "Lockout should be expired");
        assert!(synergy.is_unlocked(recovery.remaining), "Knockback should still be unlocked");
    }

    #[test]
    fn test_multiple_synergies_can_coexist() {
        // In theory, multiple abilities could synergize at once
        let recovery = GlobalRecovery::new(2.0, AbilityType::Overpower);

        let synergy1 = SynergyUnlock::new(AbilityType::Knockback, 1.0, AbilityType::Overpower);
        let synergy2 = SynergyUnlock::new(AbilityType::Deflect, 0.5, AbilityType::Overpower);

        // At 2.0s remaining: nothing unlocked
        assert!(!synergy1.is_unlocked(recovery.remaining));
        assert!(!synergy2.is_unlocked(recovery.remaining));

        // At 1.0s remaining: synergy1 unlocks
        assert!(synergy1.is_unlocked(1.0));
        assert!(!synergy2.is_unlocked(1.0));

        // At 0.5s remaining: both unlock
        assert!(synergy1.is_unlocked(0.5));
        assert!(synergy2.is_unlocked(0.5));
    }
}
