//! # NPC Recovery Timer

//! The random delay an NPC waits before each use of its signature ability.
//! The delay starts once the NPC can afford the ability again, never at the
//! use itself: every NPC of an archetype refills stamina at the same rate,
//! so a delay started at the use would run out while NPCs that fired
//! together still wait on stamina, and they would fire together again.
//! Auto-attacks never read it; their cadence is fixed.

use bevy::prelude::*;
use std::time::Duration;

use crate::spatial_difficulty::EnemyArchetype;

/// Per-archetype delay ranges (milliseconds)
const BERSERKER_RECOVERY_MIN_MS: u64 = 7000;
const BERSERKER_RECOVERY_MAX_MS: u64 = 11000;

const JUGGERNAUT_RECOVERY_MIN_MS: u64 = 3000;
const JUGGERNAUT_RECOVERY_MAX_MS: u64 = 5000;

const KITER_RECOVERY_MIN_MS: u64 = 500;
const KITER_RECOVERY_MAX_MS: u64 = 2500;

const DEFENDER_RECOVERY_MIN_MS: u64 = 2000;
const DEFENDER_RECOVERY_MAX_MS: u64 = 4000;

/// An NPC's delay before its next signature ability.
///
/// Unarmed until [`arm`](Self::arm) draws `random(min..=max)`; ready once
/// that has elapsed; [`spend`](Self::spend) on use disarms it again.
#[derive(Clone, Component, Copy, Debug)]
pub struct NpcRecovery {
    /// Server time the delay ends, once armed
    pub ready_at: Option<Duration>,
    /// Minimum delay for this NPC's archetype
    pub min_ms: u64,
    /// Maximum delay for this NPC's archetype
    pub max_ms: u64,
}

impl NpcRecovery {
    /// Create an unarmed delay for the given archetype.
    pub fn for_archetype(archetype: EnemyArchetype) -> Self {
        let (min_ms, max_ms) = recovery_range(archetype);
        Self {
            ready_at: None,
            min_ms,
            max_ms,
        }
    }

    /// Start the delay if it has not started. Call on every check that
    /// finds the ability affordable; one draw is made per use.
    pub fn arm(&mut self, now: Duration) {
        if self.ready_at.is_none() {
            let duration_ms = rand::Rng::random_range(&mut rand::rng(), self.min_ms..=self.max_ms);
            self.ready_at = Some(now + Duration::from_millis(duration_ms));
        }
    }

    /// Whether the delay is armed and has run out
    pub fn is_ready(&self, now: Duration) -> bool {
        self.ready_at.is_some_and(|at| now >= at)
    }

    /// The ability was used: the next delay waits until it is affordable again.
    pub fn spend(&mut self) {
        self.ready_at = None;
    }
}

/// Get the delay range for an archetype.
pub fn recovery_range(archetype: EnemyArchetype) -> (u64, u64) {
    match archetype {
        EnemyArchetype::Berserker => (BERSERKER_RECOVERY_MIN_MS, BERSERKER_RECOVERY_MAX_MS),
        EnemyArchetype::Juggernaut => (JUGGERNAUT_RECOVERY_MIN_MS, JUGGERNAUT_RECOVERY_MAX_MS),
        EnemyArchetype::Defender => (DEFENDER_RECOVERY_MIN_MS, DEFENDER_RECOVERY_MAX_MS),
        EnemyArchetype::Kiter => (KITER_RECOVERY_MIN_MS, KITER_RECOVERY_MAX_MS),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unarmed_is_not_ready() {
        let recovery = NpcRecovery::for_archetype(EnemyArchetype::Berserker);
        assert!(!recovery.is_ready(Duration::from_secs(10)));
    }

    #[test]
    fn armed_is_ready_only_after_the_delay() {
        let mut recovery = NpcRecovery::for_archetype(EnemyArchetype::Berserker);
        let now = Duration::from_secs(10);
        recovery.arm(now);
        let (min, max) = recovery_range(EnemyArchetype::Berserker);
        assert!(!recovery.is_ready(now + Duration::from_millis(min - 1)));
        assert!(recovery.is_ready(now + Duration::from_millis(max)));
    }

    #[test]
    fn arming_again_keeps_the_first_draw() {
        let mut recovery = NpcRecovery::for_archetype(EnemyArchetype::Juggernaut);
        let now = Duration::from_secs(10);
        recovery.arm(now);
        let first = recovery.ready_at;
        recovery.arm(now + Duration::from_secs(1));
        assert_eq!(recovery.ready_at, first);
    }

    #[test]
    fn spending_disarms() {
        let mut recovery = NpcRecovery::for_archetype(EnemyArchetype::Berserker);
        let now = Duration::from_secs(10);
        recovery.arm(now);
        recovery.spend();
        assert!(!recovery.is_ready(now + Duration::from_secs(60)));
    }

    #[test]
    fn delay_within_range() {
        let mut recovery = NpcRecovery::for_archetype(EnemyArchetype::Juggernaut);
        let now = Duration::from_secs(10);
        recovery.arm(now);
        let delay = recovery.ready_at.unwrap() - now;
        let (min, max) = recovery_range(EnemyArchetype::Juggernaut);
        assert!(delay.as_millis() >= min as u128 && delay.as_millis() <= max as u128);
    }

    #[test]
    fn archetype_recovery_ranges_are_ordered() {
        for archetype in [EnemyArchetype::Berserker, EnemyArchetype::Juggernaut, EnemyArchetype::Kiter, EnemyArchetype::Defender] {
            let (min, max) = recovery_range(archetype);
            assert!(min <= max, "{archetype:?} delay range runs backwards");
        }
    }

    #[test]
    fn consecutive_delays_vary() {
        // With a 1000ms range and 20 draws, all identical is vanishingly unlikely
        let mut recovery = NpcRecovery::for_archetype(EnemyArchetype::Berserker);
        let now = Duration::from_secs(100);
        let delays: Vec<_> = (0..20).map(|_| {
            recovery.arm(now);
            let delay = recovery.ready_at.unwrap() - now;
            recovery.spend();
            delay
        }).collect();
        assert!(!delays.windows(2).all(|w| w[0] == w[1]), "delays should vary between uses");
    }
}
