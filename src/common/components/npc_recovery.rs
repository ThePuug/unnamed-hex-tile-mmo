//! # NPC Recovery Timer (SOW-018 Phase 1)
//!
//! Per-NPC recovery timer that gates attack initiation, creating natural
//! gaps between threats from each NPC. Independent of player GlobalRecovery.

use bevy::prelude::*;
use std::time::Duration;

use crate::common::spatial_difficulty::EnemyArchetype;

/// Per-archetype recovery duration ranges (milliseconds)
const BERSERKER_RECOVERY_MIN_MS: u64 = 1000;
const BERSERKER_RECOVERY_MAX_MS: u64 = 2000;

const JUGGERNAUT_RECOVERY_MIN_MS: u64 = 3000;
const JUGGERNAUT_RECOVERY_MAX_MS: u64 = 5000;

const DEFENDER_RECOVERY_MIN_MS: u64 = 4000;
const DEFENDER_RECOVERY_MAX_MS: u64 = 6000;

/// NPC-side recovery timer that prevents attacks during cooldown.
///
/// After each attack, `recovery_until` is set to `now + random(min..=max)`.
/// The NPC cannot initiate new attacks while `now < recovery_until`.
#[derive(Clone, Component, Copy, Debug)]
pub struct NpcRecovery {
    /// Server time at which recovery ends (NPC can attack again)
    pub recovery_until: Duration,
    /// Minimum recovery duration for this NPC's archetype
    pub min_ms: u64,
    /// Maximum recovery duration for this NPC's archetype
    pub max_ms: u64,
}

impl NpcRecovery {
    /// Create a new NpcRecovery for the given archetype.
    /// Starts with no recovery (ready to attack immediately).
    pub fn for_archetype(archetype: EnemyArchetype) -> Self {
        let (min_ms, max_ms) = recovery_range(archetype);
        Self {
            recovery_until: Duration::ZERO,
            min_ms,
            max_ms,
        }
    }

    /// Check if the NPC is currently recovering (cannot attack)
    pub fn is_recovering(&self, now: Duration) -> bool {
        now < self.recovery_until
    }

    /// Set recovery timer after an attack. Randomizes duration within archetype range.
    pub fn start_recovery(&mut self, now: Duration) {
        let duration_ms = rand::Rng::random_range(&mut rand::rng(), self.min_ms..=self.max_ms);
        self.recovery_until = now + Duration::from_millis(duration_ms);
    }
}

/// Get the recovery duration range for an archetype.
/// Kiter returns (0, 0) — no explicit recovery (implicit from flee phase).
pub fn recovery_range(archetype: EnemyArchetype) -> (u64, u64) {
    match archetype {
        EnemyArchetype::Berserker => (BERSERKER_RECOVERY_MIN_MS, BERSERKER_RECOVERY_MAX_MS),
        EnemyArchetype::Juggernaut => (JUGGERNAUT_RECOVERY_MIN_MS, JUGGERNAUT_RECOVERY_MAX_MS),
        EnemyArchetype::Defender => (DEFENDER_RECOVERY_MIN_MS, DEFENDER_RECOVERY_MAX_MS),
        EnemyArchetype::Kiter => (0, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_starts_ready() {
        let recovery = NpcRecovery::for_archetype(EnemyArchetype::Berserker);
        let now = Duration::from_secs(10);
        assert!(!recovery.is_recovering(now), "Should be ready to attack initially");
    }

    #[test]
    fn recovery_gates_attacks_after_start() {
        let mut recovery = NpcRecovery::for_archetype(EnemyArchetype::Berserker);
        let now = Duration::from_secs(10);
        recovery.start_recovery(now);

        // Still recovering 500ms later (min is 1000ms)
        let half_sec_later = now + Duration::from_millis(500);
        assert!(recovery.is_recovering(half_sec_later), "Should be recovering within min duration");
    }

    #[test]
    fn recovery_clears_after_max_duration() {
        let mut recovery = NpcRecovery::for_archetype(EnemyArchetype::Berserker);
        let now = Duration::from_secs(10);
        recovery.start_recovery(now);

        // After max duration (2000ms), should be ready
        let after_max = now + Duration::from_millis(2001);
        assert!(!recovery.is_recovering(after_max), "Should be ready after max recovery duration");
    }

    #[test]
    fn archetype_recovery_ranges() {
        assert_eq!(recovery_range(EnemyArchetype::Berserker), (1000, 2000));
        assert_eq!(recovery_range(EnemyArchetype::Juggernaut), (3000, 5000));
        assert_eq!(recovery_range(EnemyArchetype::Defender), (4000, 6000));
        assert_eq!(recovery_range(EnemyArchetype::Kiter), (0, 0));
    }

    #[test]
    fn recovery_duration_within_range() {
        let mut recovery = NpcRecovery::for_archetype(EnemyArchetype::Juggernaut);
        let now = Duration::from_secs(10);
        recovery.start_recovery(now);

        let duration = recovery.recovery_until - now;
        assert!(duration.as_millis() >= 3000, "Juggernaut recovery should be >= 3000ms");
        assert!(duration.as_millis() <= 5000, "Juggernaut recovery should be <= 5000ms");
    }

    #[test]
    fn consecutive_recoveries_can_vary() {
        // Statistical test: with enough samples, not all should be identical
        let mut recovery = NpcRecovery::for_archetype(EnemyArchetype::Berserker);
        let mut durations = Vec::new();

        for i in 0..20 {
            let now = Duration::from_secs(100 + i * 5);
            recovery.start_recovery(now);
            durations.push(recovery.recovery_until - now);
        }

        // With 1000ms range and 20 samples, extremely unlikely all are identical
        let all_same = durations.windows(2).all(|w| w[0] == w[1]);
        assert!(!all_same, "Recovery durations should vary between attacks");
    }
}
