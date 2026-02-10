//! Damage calculation functions for combat system
//!
//! This module provides the core damage calculation formulas for the combat system,
//! implementing a two-phase damage calculation model:
//!
//! **Phase 1 (at attack time):** Calculate outgoing damage with attacker's attributes
//! **Phase 2 (at resolution time):** Apply passive modifiers with defender's attributes
//!
//! See ADR-005 for architectural details.

use crate::common::components::reaction_queue::DamageType;
use crate::common::components::ActorAttributes;
use rand::Rng;

/// Compute a relative stat contest modifier between attacker and defender.
///
/// Uses raw attribute values only (no level multiplier). Clamped linear:
/// `modifier = clamp(1.0 + delta / K, 0.5, 1.5)` where `delta = attacker - defender`.
///
/// - Equal stats → 1.0 (neutral)
/// - Attacker +100 over defender → 1.5 (capped)
/// - Defender +100 over attacker → 0.5 (capped)
pub fn contest_modifier(attacker_stat: u16, defender_stat: u16) -> f32 {
    let delta = attacker_stat as f32 - defender_stat as f32;
    const K: f32 = 200.0;
    const MIN: f32 = 0.5;
    const MAX: f32 = 1.5;
    (1.0 + delta / K).clamp(MIN, MAX)
}

/// Calculate outgoing damage based on attacker's attributes
///
/// Formula (from combat-system.md spec):
/// - Physical: damage = base * (1 + might/33)
/// - Magic: damage = base * (1 + focus/33)
///
/// Scaling: 20 might = 1.6x, 50 might = 2.5x, 100 might = 4.0x
///
/// # Arguments
/// * `base_damage` - Base damage from ability
/// * `attrs` - Attacker's attributes
/// * `damage_type` - Physical or Magic
///
/// # Returns
/// Scaled damage before mitigation
///
/// # Example
/// ```
/// let attrs = ActorAttributes::default();
/// let damage = calculate_outgoing_damage(20.0, &attrs, DamageType::Physical);
/// ```
pub fn calculate_outgoing_damage(
    base_damage: f32,
    attrs: &ActorAttributes,
    damage_type: DamageType,
) -> f32 {
    let scaling_attribute = match damage_type {
        DamageType::Physical => attrs.might() as f32,
        DamageType::Magic => attrs.focus() as f32,
    };

    // Linear scaling from attributes, then super-linear level multiplier (ADR-020)
    let linear = base_damage * (1.0 + scaling_attribute / 165.0);
    linear * attrs.damage_level_multiplier()
}

/// Roll for critical hit and calculate multiplier
///
/// Formula (scaled for u16 values):
/// - Crit chance: base (5%) + (instinct / 1000)
///   - At instinct=0: 5% crit chance
///   - At instinct=500 (level 50): 55% crit chance
/// - Crit multiplier: 1.5 + (instinct / 1000)
///   - At instinct=0: 1.5x damage
///   - At instinct=500 (level 50): 2.0x damage
///
/// # Arguments
/// * `attrs` - Attacker's attributes
///
/// # Returns
/// Tuple of (was_crit: bool, multiplier: f32)
/// - If crit: (true, 1.5-2.0)
/// - If not crit: (false, 1.0)
///
/// # Example
/// ```
/// let attrs = ActorAttributes::default();
/// let (was_crit, multiplier) = roll_critical(&attrs);
/// ```
pub fn roll_critical(attrs: &ActorAttributes, precision_mod: f32) -> (bool, f32) {
    let instinct = attrs.instinct() as f32;
    let base_crit_chance = 0.05; // 5%
    let crit_chance = (base_crit_chance + (instinct / 1000.0)) * precision_mod;

    let mut rng = rand::rng();
    let was_crit = rng.random::<f32>() < crit_chance;

    let crit_multiplier = if was_crit {
        1.5 + (instinct / 1000.0)
    } else {
        1.0
    };

    (was_crit, crit_multiplier)
}

/// Apply passive defensive modifiers to damage
///
/// Formula (scaled for u16 values):
/// - Physical: mitigation = vitality / 330 (capped at 75%)
/// - Magic: mitigation = focus / 330 (capped at 75%)
/// - Final damage = outgoing * (1 - mitigation)
///
/// Scaling: vitality=100 → 30% mitigation, vitality=250 → 75% (cap)
///
/// # Arguments
/// * `outgoing_damage` - Damage after attacker scaling
/// * `attrs` - Defender's attributes
/// * `damage_type` - Physical or Magic
///
/// # Returns
/// Final damage after mitigation (clamped to 0 minimum)
///
/// # Example
/// ```
/// let attrs = ActorAttributes::default();
/// let final_damage = apply_passive_modifiers(50.0, &attrs, DamageType::Physical);
/// ```
pub fn apply_passive_modifiers(
    outgoing_damage: f32,
    attrs: &ActorAttributes,
    damage_type: DamageType,
    precision_mod: f32,
) -> f32 {
    let base_mitigation = match damage_type {
        DamageType::Physical => {
            let vitality = attrs.vitality() as f32;
            vitality / 330.0
        }
        DamageType::Magic => {
            let focus = attrs.focus() as f32;
            focus / 330.0
        }
    };

    // Precision contest inversely scales mitigation:
    // attacker advantage (mod=1.5) → mitigation * 0.67
    // defender advantage (mod=0.5) → mitigation * 2.0
    let mitigation = (base_mitigation * (1.0 / precision_mod)).min(0.75);

    let final_damage = outgoing_damage * (1.0 - mitigation);
    final_damage.max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== CONTEST MODIFIER TESTS =====

    #[test]
    fn test_contest_modifier_neutral() {
        assert_eq!(contest_modifier(100, 100), 1.0);
        assert_eq!(contest_modifier(0, 0), 1.0);
        assert_eq!(contest_modifier(50, 50), 1.0);
    }

    #[test]
    fn test_contest_modifier_attacker_advantage() {
        let m = contest_modifier(150, 100);
        assert!(m > 1.0, "Attacker advantage should produce >1.0, got {m}");
        assert!((m - 1.25).abs() < 0.001, "150 vs 100 → delta 50/200 = 1.25, got {m}");
    }

    #[test]
    fn test_contest_modifier_defender_advantage() {
        let m = contest_modifier(100, 150);
        assert!(m < 1.0, "Defender advantage should produce <1.0, got {m}");
        assert!((m - 0.75).abs() < 0.001, "100 vs 150 → delta -50/200 = 0.75, got {m}");
    }

    #[test]
    fn test_contest_modifier_clamped() {
        // +200 delta would give 2.0 unclamped, should cap at 1.5
        assert_eq!(contest_modifier(300, 100), 1.5);
        // -200 delta would give 0.0 unclamped, should floor at 0.5
        assert_eq!(contest_modifier(100, 300), 0.5);
        // Extreme values
        assert_eq!(contest_modifier(u16::MAX, 0), 1.5);
        assert_eq!(contest_modifier(0, u16::MAX), 0.5);
    }

    #[test]
    fn test_contest_modifier_symmetric() {
        // f(a,b) + f(b,a) = 2.0 (they are complements around 1.0)
        let pairs = [(100, 150), (0, 50), (200, 100), (80, 120)];
        for (a, b) in pairs {
            let forward = contest_modifier(a, b);
            let reverse = contest_modifier(b, a);
            assert!(
                (forward + reverse - 2.0).abs() < 0.001,
                "f({a},{b})={forward} + f({b},{a})={reverse} should = 2.0"
            );
        }
    }

    #[test]
    fn test_contest_modifier_monotonic() {
        // Larger attacker stat → larger modifier
        let m1 = contest_modifier(100, 100); // delta 0
        let m2 = contest_modifier(120, 100); // delta 20
        let m3 = contest_modifier(150, 100); // delta 50
        let m4 = contest_modifier(200, 100); // delta 100 (capped)
        assert!(m1 < m2, "m1={m1} should be < m2={m2}");
        assert!(m2 < m3, "m2={m2} should be < m3={m3}");
        assert!(m3 <= m4, "m3={m3} should be <= m4={m4}");
    }

    // ===== PRECISION MOD EFFECT TESTS =====

    #[test]
    fn test_crit_chance_with_precision_mod() {
        let attrs = ActorAttributes::default();
        // With precision_mod > 1, crit chance should increase
        let base_instinct = attrs.instinct() as f32;
        let base_chance = 0.05 + base_instinct / 1000.0;
        let boosted = base_chance * 1.5;
        assert!(boosted > base_chance);
    }

    #[test]
    fn test_mitigation_with_precision_mod() {
        // Build attrs with some vitality for mitigation
        let attrs = ActorAttributes::new(0, 0, 0, -5, 0, 0, 0, 0, 0);
        let damage = 100.0;

        let neutral = apply_passive_modifiers(damage, &attrs, DamageType::Physical, 1.0);
        let boosted = apply_passive_modifiers(damage, &attrs, DamageType::Physical, 1.5);
        let reduced = apply_passive_modifiers(damage, &attrs, DamageType::Physical, 0.5);

        // Attacker advantage → less mitigation → more damage through
        assert!(boosted > neutral, "precision_mod 1.5 should let more damage through: {boosted} > {neutral}");
        // Defender advantage → more mitigation → less damage through
        assert!(reduced < neutral, "precision_mod 0.5 should reduce damage through: {reduced} < {neutral}");
    }
}

