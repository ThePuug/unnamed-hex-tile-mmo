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
pub fn roll_critical(attrs: &ActorAttributes) -> (bool, f32) {
    let instinct = attrs.instinct() as f32;
    let base_crit_chance = 0.05; // 5%
    let crit_chance = base_crit_chance + (instinct / 1000.0);

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
) -> f32 {
    let mitigation = match damage_type {
        DamageType::Physical => {
            let vitality = attrs.vitality() as f32;
            (vitality / 330.0).min(0.75) // Cap at 75% reduction
        }
        DamageType::Magic => {
            let focus = attrs.focus() as f32;
            (focus / 330.0).min(0.75) // Cap at 75% reduction
        }
    };

    let final_damage = outgoing_damage * (1.0 - mitigation);
    final_damage.max(0.0) // Clamp to 0 (no healing from negative damage)
}

