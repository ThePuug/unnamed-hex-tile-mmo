use bevy::prelude::*;

use crate::common::{
    components::{recovery::{GlobalRecovery, SynergyUnlock}, ActorAttributes},
    message::AbilityType,
    systems::combat::damage as damage_calc,
};

/// Synergy trigger types for ability categorization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SynergyTrigger {
    GapCloser,   // Lunge
    HeavyStrike, // Overpower
    Push,        // Knockback
    Mitigate,    // Counter (ADR-014)
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
    // Heavy Strike → Mitigate: Counter unlocks 1.0s early during Overpower recovery (ADR-014)
    SynergyRule {
        trigger: SynergyTrigger::HeavyStrike,
        target: AbilityType::Counter,
        unlock_reduction: 1.0, // Counter available at 1.0s instead of 2.2s (0.2s window)
    },
];

/// Get the synergy trigger type for an ability
pub fn get_synergy_trigger(ability: AbilityType) -> Option<SynergyTrigger> {
    match ability {
        AbilityType::Lunge => Some(SynergyTrigger::GapCloser),
        AbilityType::Overpower => Some(SynergyTrigger::HeavyStrike),
        AbilityType::Counter => Some(SynergyTrigger::Mitigate),  // ADR-014: Mitigate type
        AbilityType::Deflect => Some(SynergyTrigger::Defensive),
        AbilityType::AutoAttack | AbilityType::Volley => None, // No synergies
    }
}

/// Apply synergies when an ability is used (ADR-012, SOW-021 Phase 2)
/// This should be called immediately after creating GlobalRecovery
/// Both server and client run this function locally (no network broadcast needed)
///
/// Pass the recovery struct directly since it may not be queryable yet (command buffering)
///
/// SOW-021 Phase 2: Finesse vs Cunning contest modifies synergy unlock_reduction
/// - High Finesse (attacker) → tighter burst windows (more reduction)
/// - High Cunning (defender) → longer gaps between chains (less reduction)
pub fn apply_synergies(
    entity: Entity,
    used_ability: AbilityType,
    recovery: &GlobalRecovery,
    attacker_attrs: &ActorAttributes,
    defender_attrs: &ActorAttributes,
    commands: &mut Commands,
) {
    // Get the trigger type for the used ability
    let Some(trigger_type) = get_synergy_trigger(used_ability) else {
        return; // No synergies for this ability
    };

    // Calculate Finesse vs Cunning contest modifier (SOW-021 Phase 2)
    // Self-casting: both attacker and defender are the same entity, so contest is neutral (1.0)
    let synergy_mod = damage_calc::contest_modifier(attacker_attrs.finesse(), defender_attrs.cunning());

    // Find and apply matching synergy rules
    for rule in MVP_SYNERGIES {
        if rule.trigger == trigger_type {
            // Base synergy unlock time (what everyone gets)
            let base_unlock = recovery.remaining - rule.unlock_reduction;

            // Finesse provides 0-50% improvement (contest_mod 0.0-1.0 maps to 0-50%)
            let improvement_factor = synergy_mod * 0.5;

            // Reduce the base unlock time by 0-50%
            let unlock_at = (base_unlock * (1.0 - improvement_factor)).max(0.0);

            // Insert synergy unlock component (both server and client do this locally)
            // Only insert if entity exists (may have been evicted client-side)
            let synergy = SynergyUnlock::new(rule.target, unlock_at, used_ability);
            if let Ok(mut entity_cmd) = commands.get_entity(entity) {
                entity_cmd.insert(synergy);
            }
        }
    }
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
            get_synergy_trigger(AbilityType::Counter),
            Some(SynergyTrigger::Mitigate)
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

        // Overpower → Counter (ADR-014: replaces Knockback)
        let overpower_synergy = &MVP_SYNERGIES[1];
        assert_eq!(overpower_synergy.trigger, SynergyTrigger::HeavyStrike);
        assert_eq!(overpower_synergy.target, AbilityType::Counter);
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
        // Test Overpower → Counter synergy timing (ADR-014: replaces Knockback)
        let recovery = GlobalRecovery::new(2.0, AbilityType::Overpower);
        let synergy = SynergyUnlock::new(AbilityType::Counter, 1.0, AbilityType::Overpower);

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
            "Counter should unlock at 1.0s remaining"
        );
    }

    #[test]
    fn test_synergy_finesse_cunning_neutral() {
        // Test Finesse vs Cunning contest with balanced stats (SOW-021 Phase 2)
        // Equal stats → contest_modifier = 0.0 → synergy completely negated
        use crate::common::components::ActorAttributes;

        // Create attributes with balanced Finesse/Cunning
        // grace: axis=4, spectrum=3 → 4×16 + 3×12 = 64+36 = 100
        // instinct: axis=-4, spectrum=3 → 4×16 + 3×12 = 64+36 = 100
        let attrs = ActorAttributes::new(
            4, 3, 0,   // might_grace: axis=4 → grace=100 (finesse)
            0, 0, 0,   // vitality_focus
            -4, 3, 0,  // instinct_presence: axis=-4 → instinct=100 (cunning)
        );

        assert_eq!(attrs.finesse(), 100, "Finesse should be 100");
        assert_eq!(attrs.cunning(), 100, "Cunning should be 100");

        // Contest modifier should be 0.0 (equal stats cancel)
        let contest_mod = damage_calc::contest_modifier(attrs.finesse(), attrs.cunning());
        assert_eq!(contest_mod, 0.0, "Contest modifier should be 0.0 for equal stats");

        // Example: Recovery 3s, base synergy unlock at 2s (3s - 1s reduction)
        // Equal stats: 0% improvement → unlocks at 2.0s (base only)
        let base_unlock = 2.0;
        let improvement_factor = contest_mod * 0.5;  // 0.0 × 0.5 = 0.0 (0%)
        let final_unlock = base_unlock * (1.0 - improvement_factor);
        assert!((final_unlock - 2.0).abs() < 0.01, "Equal stats: 0% improvement → 2.0s unlock");
    }

    #[test]
    fn test_synergy_high_finesse_advantage() {
        // Test high Finesse vs low Cunning (tighter burst windows, SOW-021 Phase 2)
        // Finesse 200 vs Cunning 50 → stronger burst compression
        use crate::common::components::ActorAttributes;

        // grace: axis=11, spectrum=2 → 11×16 + 2×12 = 176+24 = 200
        // instinct: axis=-2, spectrum=2, shift=-1 → 2×16 + 2×12 - (-1×12) = 32+24+12 = 68 (close to 50)
        // Actually use axis=-3, spectrum=0, shift=0 → 3×16 = 48 (close to 50)
        let attacker = ActorAttributes::new(
            11, 2, 0,  // might_grace: axis=11 → grace≈200 (finesse)
            0, 0, 0,   // vitality_focus
            0, 0, 0,   // instinct_presence
        );

        let defender = ActorAttributes::new(
            0, 0, 0,   // might_grace
            0, 0, 0,   // vitality_focus
            -3, 0, 0,  // instinct_presence: axis=-3 → instinct=48 (cunning)
        );

        let attacker_finesse = attacker.finesse();
        let defender_cunning = defender.cunning();
        assert!(attacker_finesse >= 195 && attacker_finesse <= 205, "Finesse should be ~200, got {}", attacker_finesse);
        assert!(defender_cunning >= 45 && defender_cunning <= 55, "Cunning should be ~50, got {}", defender_cunning);

        // Contest modifier should be > 0.0 (attacker advantage)
        let contest_mod = damage_calc::contest_modifier(attacker.finesse(), defender.cunning());
        assert!(contest_mod > 0.0, "High finesse should provide synergy improvement");
        assert!(contest_mod <= 1.0, "Contest modifier capped at 1.0");

        // Example: Recovery 3s, base synergy unlock at 2s (3s - 1s reduction)
        // High finesse: significant improvement → unlocks earlier than 2s
        let base_unlock = 2.0;
        let improvement_factor = contest_mod * 0.5;  // 0-50% improvement
        let final_unlock = base_unlock * (1.0 - improvement_factor);
        assert!(final_unlock < 2.0, "High finesse should compress burst window (unlock < 2.0s)");
    }

    #[test]
    fn test_synergy_high_cunning_resistance() {
        // Test low Finesse vs high Cunning (longer gaps, SOW-021 Phase 2)
        // Finesse ~50 vs Cunning ~200 → weaker burst compression
        use crate::common::components::ActorAttributes;

        // grace: axis=3, spectrum=0 → 3×16 = 48
        // instinct: axis=-11, spectrum=2 → 11×16 + 2×12 = 176+24 = 200
        let attacker = ActorAttributes::new(
            3, 0, 0,   // might_grace: axis=3 → grace=48 (finesse)
            0, 0, 0,   // vitality_focus
            0, 0, 0,   // instinct_presence
        );

        let defender = ActorAttributes::new(
            0, 0, 0,    // might_grace
            0, 0, 0,    // vitality_focus
            -11, 2, 0,  // instinct_presence: axis=-11 → instinct=200 (cunning)
        );

        let attacker_finesse = attacker.finesse();
        let defender_cunning = defender.cunning();
        assert!(attacker_finesse >= 45 && attacker_finesse <= 55, "Finesse should be ~50, got {}", attacker_finesse);
        assert!(defender_cunning >= 195 && defender_cunning <= 205, "Cunning should be ~200, got {}", defender_cunning);

        // Contest modifier should be 0.0 (defender has advantage, completely negates synergy)
        let contest_mod = damage_calc::contest_modifier(attacker.finesse(), defender.cunning());
        assert_eq!(contest_mod, 0.0, "Defender cunning advantage should completely negate synergy compression");

        // Lunge → Overpower: 0.5s × 0.0 = 0.0s (synergy completely negated)
        let base_reduction = 0.5;
        let effective_reduction = base_reduction * contest_mod;
        assert!(effective_reduction < 0.5, "High cunning should extend gap between chains");
    }

    #[test]
    fn test_synergy_maximum_finesse_advantage() {
        // Test maximum Finesse advantage (50% improvement, SOW-021 Phase 2)
        use crate::common::components::ActorAttributes;

        // Max grace: axis=30, spectrum=30, shift=0 → 30×16 + 30×12 = 480+360 = 840
        // Min cunning: 0
        let attacker = ActorAttributes::new(
            30, 30, 0,  // might_grace: max values for maximum finesse (shift=0, spectrum can't shift if axis is positive)
            0, 0, 0,    // vitality_focus
            0, 0, 0,    // instinct_presence
        );

        let defender = ActorAttributes::new(
            0, 0, 0,  // might_grace
            0, 0, 0,  // vitality_focus
            0, 0, 0,  // instinct_presence: min cunning (0)
        );

        let contest_mod = damage_calc::contest_modifier(attacker.finesse(), defender.cunning());
        assert!((contest_mod - 1.0).abs() < 0.01, "Contest modifier should cap at 1.0, got {}", contest_mod);

        // Example: Recovery 3s, base synergy unlock at 2s (3s - 1s reduction)
        // Max finesse: 50% improvement → unlocks at 1.0s (2s × 0.5)
        let base_unlock = 2.0;
        let improvement_factor = contest_mod * 0.5;  // 1.0 × 0.5 = 0.5 (50%)
        let final_unlock = base_unlock * (1.0 - improvement_factor);
        assert!((final_unlock - 1.0).abs() < 0.01, "Max finesse: 50% improvement → 1.0s unlock");
    }

    #[test]
    fn test_synergy_maximum_cunning_resistance() {
        // Test maximum Cunning resistance (0% improvement, SOW-021 Phase 2)
        use crate::common::components::ActorAttributes;

        // Min finesse: 0
        // Max cunning: axis=-30, spectrum=30, shift=0 → 30×16 + 30×12 = 480+360 = 840
        let attacker = ActorAttributes::new(
            0, 0, 0,   // might_grace: min finesse (0)
            0, 0, 0,   // vitality_focus
            0, 0, 0,   // instinct_presence
        );

        let defender = ActorAttributes::new(
            0, 0, 0,      // might_grace
            0, 0, 0,      // vitality_focus
            -30, 30, 0,   // instinct_presence: max values for maximum cunning
        );

        let contest_mod = damage_calc::contest_modifier(attacker.finesse(), defender.cunning());
        assert_eq!(contest_mod, 0.0, "Contest modifier should be 0.0 (defender advantage), got {}", contest_mod);

        // Example: Recovery 3s, base synergy unlock at 2s (3s - 1s reduction)
        // No finesse: 0% improvement → unlocks at 2.0s (base only)
        let base_unlock = 2.0;
        let improvement_factor = contest_mod * 0.5;  // 0.0 × 0.5 = 0.0 (0%)
        let final_unlock = base_unlock * (1.0 - improvement_factor);
        assert!((final_unlock - 2.0).abs() < 0.01, "No finesse: 0% improvement → 2.0s unlock");
    }

    #[test]
    fn test_synergy_self_cast_behavior() {
        // Test self-casting behavior: when entity uses ability on self,
        // contest is between own finesse vs own cunning (SOW-021 Phase 2)
        use crate::common::components::ActorAttributes;

        // Create entity with higher finesse than cunning
        // grace: axis=15 → 15×16 + 8×12 = 240+96 = 336
        // instinct: axis=-12 → 12×16 + 5×12 = 192+60 = 252
        let attrs = ActorAttributes::new(
            15, 8, 0,   // might_grace
            0, 0, 0,    // vitality_focus
            -12, 5, 0,  // instinct_presence
        );

        let finesse = attrs.finesse();
        let cunning = attrs.cunning();

        // Self-cast: contest is between own stats (finesse vs cunning)
        // High finesse entity → better at compressing synergy chains
        let contest_mod = damage_calc::contest_modifier(finesse, cunning);
        assert!(contest_mod > 0.0, "High finesse entity should have contest_mod > 0.0 in self-cast");

        // Verify the contest formula is applied correctly (sqrt-based with 300 delta for full scale)
        let delta = (finesse as f32 - cunning as f32).max(0.0);
        let expected = (delta / 300.0).min(1.0).sqrt();
        assert!((contest_mod - expected).abs() < 0.01, "Contest modifier should match formula");
    }
}
