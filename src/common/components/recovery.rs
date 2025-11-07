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
        AbilityType::Lunge => 1.0,      // Gap closer: 1s lockout
        AbilityType::Overpower => 2.0,  // Heavy strike: 2s lockout
        AbilityType::Knockback => 0.5,  // Push: 0.5s lockout
        AbilityType::Deflect => 1.0,    // Defensive: 1s lockout
        AbilityType::AutoAttack => 0.0, // AutoAttack uses its own timer, not GlobalRecovery
        AbilityType::Volley => 3.0,     // NPC ranged: 3s lockout
    }
}

/// Calculate auto-attack period based on Presence attribute
/// Uses hyperbolic diminishing returns curve approaching 1.0s soft cap
///
/// - Presence = 0: 2.5s (slow, base NPCs)
/// - Presence = 50: 1.75s (mid-game)
/// - Presence = 100: 1.5s (high presence build)
/// - Presence → ∞: 1.0s (soft cap, extreme stacking)
pub fn calculate_auto_attack_period(presence: i8) -> f32 {
    const MIN_PERIOD: f32 = 1.0;  // Soft cap (high Presence)
    const BASE_PERIOD: f32 = 2.5; // Period at 0 Presence
    const SCALE: f32 = 50.0;      // Steeper curve (reaches 2.0s at 50 Presence)

    // Clamp to 0 minimum - negative Presence doesn't slow attacks further
    let presence_clamped = presence.max(0) as f32;

    // Hyperbolic diminishing returns: period = 1.0 + 1.5 / (1 + presence/50)
    MIN_PERIOD + (BASE_PERIOD - MIN_PERIOD) / (1.0 + presence_clamped / SCALE)
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
        // MVP values from ADR-012
        assert_eq!(get_ability_recovery_duration(AbilityType::Lunge), 1.0);
        assert_eq!(get_ability_recovery_duration(AbilityType::Overpower), 2.0);
        assert_eq!(get_ability_recovery_duration(AbilityType::Knockback), 0.5);
        assert_eq!(get_ability_recovery_duration(AbilityType::Deflect), 1.0);
        assert_eq!(get_ability_recovery_duration(AbilityType::AutoAttack), 0.0); // Uses own timer
        assert_eq!(get_ability_recovery_duration(AbilityType::Volley), 3.0);
    }

    // ===== Auto-Attack Period Tests =====

    #[test]
    fn test_auto_attack_period_base() {
        // 0 Presence = 2.5s (base period for NPCs with no Presence)
        let period = calculate_auto_attack_period(0);
        assert!((period - 2.5).abs() < 0.001, "Expected 2.5s at 0 Presence, got {}", period);
    }

    #[test]
    fn test_auto_attack_period_negative_presence() {
        // Negative Presence is clamped to 0 (doesn't slow further)
        let period = calculate_auto_attack_period(-50);
        assert!((period - 2.5).abs() < 0.001, "Negative Presence should clamp to base period");
    }

    #[test]
    fn test_auto_attack_period_mid_game() {
        // 50 Presence = 1.75s (significant early gain)
        let period = calculate_auto_attack_period(50);
        assert!((period - 1.75).abs() < 0.01, "Expected 1.75s at 50 Presence, got {}", period);
    }

    #[test]
    fn test_auto_attack_period_high_presence() {
        // 100 Presence = 1.5s (high presence build)
        let period = calculate_auto_attack_period(100);
        assert!((period - 1.5).abs() < 0.01, "Expected 1.5s at 100 Presence, got {}", period);
    }

    #[test]
    fn test_auto_attack_period_extreme_presence() {
        // 127 Presence (i8 max) = ~1.27s (approaching soft cap with diminishing returns)
        let period = calculate_auto_attack_period(127);
        assert!(period < 1.5, "Expected period < 1.5s at max Presence");
        assert!(period > 1.0, "Should never reach 1.0s soft cap");
    }

    #[test]
    fn test_auto_attack_period_approaches_soft_cap() {
        // Very high Presence approaches but never reaches 1.0s
        // Using i8::MAX (127) as test value: 1.0 + 1.5/(1+127/50) ≈ 1.42s
        let period_max = calculate_auto_attack_period(i8::MAX);

        assert!(period_max > 1.0, "Should never reach 1.0s soft cap even at i8::MAX");
        assert!(period_max < 1.5, "Should be approaching soft cap at max Presence");
        assert!((period_max - 1.42).abs() < 0.05, "Expected ~1.42s at i8::MAX, got {}", period_max);
    }

    #[test]
    fn test_auto_attack_period_diminishing_returns() {
        // Test that returns diminish as Presence increases (using values within i8 range)
        let gain_0_to_50 = calculate_auto_attack_period(0) - calculate_auto_attack_period(50);
        let gain_50_to_100 = calculate_auto_attack_period(50) - calculate_auto_attack_period(100);
        let gain_100_to_127 = calculate_auto_attack_period(100) - calculate_auto_attack_period(127);

        assert!(gain_0_to_50 > gain_50_to_100, "First 50 Presence should give more benefit than second 50");
        assert!(gain_50_to_100 > gain_100_to_127, "Benefit should continue diminishing");
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
