use bevy::prelude::*;

use crate::common::{
    components::recovery::{GlobalRecovery, SynergyUnlock},
    message::AbilityType,
};

/// Synergy trigger types for ability categorization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SynergyTrigger {
    GapCloser,   // Lunge
    HeavyStrike, // Overpower
    Push,        // Knockback
    Defensive,   // Deflect
}

/// Synergy rule definition (what ability unlocks what)
#[derive(Debug, Clone)]
pub struct SynergyRule {
    pub trigger: SynergyTrigger,
    pub target: AbilityType,
    pub unlock_reduction: f32, // How much earlier to unlock (in seconds)
}

/// MVP Synergy Rules (hardcoded for Phase 2, data-driven later in Phase 4)
pub const MVP_SYNERGIES: &[SynergyRule] = &[
    // Gap Closer → Heavy Strike: Overpower unlocks 0.5s early during Lunge recovery
    SynergyRule {
        trigger: SynergyTrigger::GapCloser,
        target: AbilityType::Overpower,
        unlock_reduction: 0.5, // Overpower available at 0.5s instead of 1.0s
    },
    // Heavy Strike → Push: Knockback unlocks 1.0s early during Overpower recovery
    SynergyRule {
        trigger: SynergyTrigger::HeavyStrike,
        target: AbilityType::Knockback,
        unlock_reduction: 1.0, // Knockback available at 1.0s instead of 2.0s
    },
];

/// Get the synergy trigger type for an ability
pub fn get_synergy_trigger(ability: AbilityType) -> Option<SynergyTrigger> {
    match ability {
        AbilityType::Lunge => Some(SynergyTrigger::GapCloser),
        AbilityType::Overpower => Some(SynergyTrigger::HeavyStrike),
        AbilityType::Knockback => Some(SynergyTrigger::Push),
        AbilityType::Deflect => Some(SynergyTrigger::Defensive),
        AbilityType::AutoAttack | AbilityType::Volley => None, // No synergies
    }
}

/// Apply synergies when an ability is used (ADR-012)
/// This should be called immediately after creating GlobalRecovery
/// Both server and client run this function locally (no network broadcast needed)
///
/// Pass the recovery struct directly since it may not be queryable yet (command buffering)
pub fn apply_synergies(
    entity: Entity,
    used_ability: AbilityType,
    recovery: &GlobalRecovery,
    commands: &mut Commands,
) {
    // Get the trigger type for the used ability
    let Some(trigger_type) = get_synergy_trigger(used_ability) else {
        return; // No synergies for this ability
    };

    // Find and apply matching synergy rules
    for rule in MVP_SYNERGIES {
        if rule.trigger == trigger_type {
            // Calculate unlock time: recovery.remaining - unlock_reduction
            let unlock_at = (recovery.remaining - rule.unlock_reduction).max(0.0);

            // Insert synergy unlock component (both server and client do this locally)
            let synergy = SynergyUnlock::new(rule.target, unlock_at, used_ability);
            commands.entity(entity).insert(synergy);
        }
    }
}

/// Check if an ability can be used (considering recovery and synergies)
pub fn can_use_ability(
    ability: AbilityType,
    entity: Entity,
    recovery_query: &Query<&GlobalRecovery>,
    synergy_query: &Query<&SynergyUnlock>,
) -> bool {
    // Check if universal lockout is active
    if let Ok(recovery) = recovery_query.get(entity) {
        // Check if synergy unlocks this ability early
        if let Ok(synergy) = synergy_query.get(entity) {
            if synergy.ability == ability {
                // Synergy active: check if unlock time reached
                return synergy.is_unlocked(recovery.remaining);
            }
        }

        // No synergy: locked until full recovery
        return false;
    }

    // No lockout: ability available
    true
}

/// System to clean up expired synergies when recovery expires
pub fn synergy_cleanup_system(
    mut commands: Commands,
    recovery_query: Query<Entity, With<GlobalRecovery>>,
    synergy_query: Query<(Entity, &SynergyUnlock)>,
) {
    // Collect entities with synergies but no recovery
    let entities_with_recovery: std::collections::HashSet<Entity> =
        recovery_query.iter().collect();

    for (entity, _synergy) in synergy_query.iter() {
        if !entities_with_recovery.contains(&entity) {
            // Recovery expired, remove synergy
            commands.entity(entity).remove::<SynergyUnlock>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_synergy_trigger() {
        assert_eq!(
            get_synergy_trigger(AbilityType::Lunge),
            Some(SynergyTrigger::GapCloser)
        );
        assert_eq!(
            get_synergy_trigger(AbilityType::Overpower),
            Some(SynergyTrigger::HeavyStrike)
        );
        assert_eq!(
            get_synergy_trigger(AbilityType::Knockback),
            Some(SynergyTrigger::Push)
        );
        assert_eq!(
            get_synergy_trigger(AbilityType::Deflect),
            Some(SynergyTrigger::Defensive)
        );
        assert_eq!(get_synergy_trigger(AbilityType::AutoAttack), None);
        assert_eq!(get_synergy_trigger(AbilityType::Volley), None);
    }

    #[test]
    fn test_mvp_synergies_rules() {
        assert_eq!(MVP_SYNERGIES.len(), 2, "MVP should have 2 synergy rules");

        // Lunge → Overpower
        let lunge_synergy = &MVP_SYNERGIES[0];
        assert_eq!(lunge_synergy.trigger, SynergyTrigger::GapCloser);
        assert_eq!(lunge_synergy.target, AbilityType::Overpower);
        assert_eq!(lunge_synergy.unlock_reduction, 0.5);

        // Overpower → Knockback
        let overpower_synergy = &MVP_SYNERGIES[1];
        assert_eq!(overpower_synergy.trigger, SynergyTrigger::HeavyStrike);
        assert_eq!(overpower_synergy.target, AbilityType::Knockback);
        assert_eq!(overpower_synergy.unlock_reduction, 1.0);
    }

    // Note: Following DEVELOPER role guidance to write durable unit tests.
    // The can_use_ability function is designed to be called from systems with ECS queries,
    // so we test the logic components (GlobalRecovery, SynergyUnlock) directly instead.

    #[test]
    fn test_ability_locked_by_recovery() {
        // Test that recovery locks abilities
        let recovery = GlobalRecovery::new(1.0, AbilityType::Lunge);
        assert!(recovery.is_active(), "Recovery should be active");
    }

    #[test]
    fn test_synergy_unlock_logic() {
        // Test synergy unlock logic directly
        let synergy = SynergyUnlock::new(AbilityType::Overpower, 0.5, AbilityType::Lunge);

        // At 1.0s remaining (not unlocked yet)
        assert!(
            !synergy.is_unlocked(1.0),
            "Should not be unlocked at 1.0s remaining"
        );

        // At 0.5s remaining (unlocked)
        assert!(
            synergy.is_unlocked(0.5),
            "Should be unlocked at 0.5s remaining"
        );

        // At 0.3s remaining (unlocked)
        assert!(
            synergy.is_unlocked(0.3),
            "Should be unlocked at 0.3s remaining"
        );
    }

    #[test]
    fn test_lunge_synergy_timing() {
        // Test Lunge → Overpower synergy timing
        let recovery = GlobalRecovery::new(1.0, AbilityType::Lunge);
        let synergy = SynergyUnlock::new(AbilityType::Overpower, 0.5, AbilityType::Lunge);

        // At start (1.0s remaining): locked
        assert!(recovery.is_active());
        assert!(!synergy.is_unlocked(recovery.remaining));

        // After 0.5s (0.5s remaining): synergy unlocks
        let mut recovery_mid = recovery.clone();
        recovery_mid.tick(0.5);
        assert!(recovery_mid.is_active());
        assert!(
            synergy.is_unlocked(recovery_mid.remaining),
            "Overpower should unlock at 0.5s remaining"
        );
    }

    #[test]
    fn test_overpower_synergy_timing() {
        // Test Overpower → Knockback synergy timing
        let recovery = GlobalRecovery::new(2.0, AbilityType::Overpower);
        let synergy = SynergyUnlock::new(AbilityType::Knockback, 1.0, AbilityType::Overpower);

        // At start (2.0s remaining): locked
        assert!(recovery.is_active());
        assert!(!synergy.is_unlocked(recovery.remaining));

        // After 0.5s (1.5s remaining): still locked
        let mut recovery_early = recovery.clone();
        recovery_early.tick(0.5);
        assert!(!synergy.is_unlocked(recovery_early.remaining));

        // After 1.0s (1.0s remaining): synergy unlocks
        let mut recovery_mid = recovery.clone();
        recovery_mid.tick(1.0);
        assert!(
            synergy.is_unlocked(recovery_mid.remaining),
            "Knockback should unlock at 1.0s remaining"
        );
    }
}
