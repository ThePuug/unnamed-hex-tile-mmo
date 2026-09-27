//! Damage calculation functions for combat system

//! Two contest patterns:
//! - Pattern 1 (Nullifying): base × gap × contest_factor → nullifies at equal investment
//! - Pattern 2 (Baseline+Bonus): base × gap × (1.0 + k × contest_factor) → preserves baseline



use bevy::prelude::Entity;
use crate::components::reaction_queue::DamageType;
use crate::components::ActorAttributes;

/// Level gap scaling factor.

/// Gaussian decay: e^(-gap² × ln(3) / 100)
/// - Equal levels → 1.0
/// - 10 level gap → 0.333
/// - 20 level gap → ~0.012 (near zero)

/// The beneficiary's effect is reduced when the opponent outlevels them.
/// When beneficiary is equal or higher level, returns 1.0.
pub fn gap_factor(beneficiary_level: u32, opponent_level: u32) -> f32 {
    let gap = opponent_level.saturating_sub(beneficiary_level) as f32;
    const K: f32 = 1.0986123 / 100.0; // ln(3) / 100
    (-K * gap * gap).exp()
}

/// Contest factor (Pattern 1: Nullifying).

/// Returns 0 to 1.0:
/// - Equal/losing → 0 (effect nullified)
/// - Max advantage (300 delta) → 1.0 (full effect)
/// - Half advantage (150 delta) → 0.707 (~71% benefit)

/// Used by: mitigation, pushback, healing reduction, synergy, recovery speed.
pub fn contest_factor(advantage_stat: u16, counter_stat: u16) -> f32 {
    let delta = advantage_stat as i32 - counter_stat as i32;
    if delta <= 0 {
        return 0.0;
    }

    let normalized = (delta as f32 / 300.0).min(1.0);
    normalized.sqrt()
}

/// Reaction window contest (Pattern 2: Baseline+Bonus).

/// Returns 1.0 to 1.5:
/// - Equal/losing → 1.0 (baseline window preserved)
/// - Max advantage → 1.5 (50% improvement)

/// Used ONLY by reaction window to ensure playable baseline.
pub fn reaction_contest_factor(cunning: u16, finesse: u16) -> f32 {
    let delta = cunning as i32 - finesse as i32;
    if delta <= 0 {
        return 1.0;
    }

    let normalized = (delta as f32 / 300.0).min(1.0);
    1.0 + normalized.sqrt() * 0.5
}

/// Calculate outgoing damage (Phase 1 pass-through).
pub fn calculate_outgoing_damage(
    base_damage: f32,
    _attrs: &ActorAttributes,
    _damage_type: DamageType,
) -> f32 {
    base_damage
}

/// An attack's damage within its range: `spread` of `damage` either side of
/// it, where `draw`, from -1 to 1, falls. A roll, not a crit: every attack's
/// damage is a range, so two near-even actors trade wins instead of one
/// winning by a sliver every time.
pub fn spread(damage: f32, spread: f32, draw: f32) -> f32 {
    damage * (1.0 + spread * draw.clamp(-1.0, 1.0))
}

/// Calculate recovery pushback percentage.

/// Pattern 1 (Nullifying): base × gap × contest_factor
/// Base: 50%, Cap: 50%

/// Applied to effective_recovery_base (after composure, before synergy).
pub fn calculate_recovery_pushback(
    attacker_impact: u16,
    defender_composure: u16,
    attacker_level: u32,
    defender_level: u32,
) -> f32 {
    const BASE_PUSHBACK: f32 = 0.50;
    const MAX_PUSHBACK: f32 = 0.50;

    let base = BASE_PUSHBACK;
    let gap = gap_factor(attacker_level, defender_level);
    let contest = contest_factor(attacker_impact, defender_composure);

    (base * gap * contest).min(MAX_PUSHBACK)
}

/// Scan for the strongest Dominance aura within range of target, among
/// `actors`, each read with its own position.
/// Returns (max_dominance, level_of_dominant_entity).
pub fn find_max_dominance_in_range(
    target: Entity,
    actors: &bevy::prelude::Query<(&crate::components::Loc, &ActorAttributes)>,
) -> (u16, u32) {
    const RADIUS: i32 = 5;

    let Ok((target_loc, _)) = actors.get(target) else {
        return (0, 0);
    };

    let mut max_dominance = 0u16;
    let mut dominant_level = 0u32;

    for (loc, attrs) in actors.iter() {
        let distance = target_loc.flat_distance(loc) as i32;
        if distance <= RADIUS {
            let dominance = attrs.dominance();
            if dominance > max_dominance {
                max_dominance = dominance;
                dominant_level = attrs.total_level();
            }
        }
    }

    (max_dominance, dominant_level)
}

/// Apply passive mitigation to damage (unified for all damage types).

/// Pattern 1 (Nullifying): base × gap × contest_factor
/// Base: 75%, Cap: 75%
pub fn apply_passive_modifiers(
    outgoing_damage: f32,
    attrs: &ActorAttributes,
    max_dominance_in_range: u16,
    attacker_level: u32,
) -> f32 {
    const BASE_MITIGATION: f32 = 0.75;
    const MAX_MITIGATION: f32 = 0.75;

    let toughness = attrs.toughness();
    let base = BASE_MITIGATION;
    let gap = gap_factor(attrs.total_level(), attacker_level);
    let contest = contest_factor(toughness, max_dominance_in_range);

    let mitigation = (base * gap * contest).min(MAX_MITIGATION);
    (outgoing_damage * (1.0 - mitigation)).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spread_is_a_range_about_the_damage() {
        assert_eq!(spread(100.0, 0.2, 0.0), 100.0, "the middle of the range is the damage");
        assert!((spread(100.0, 0.2, -1.0) - 80.0).abs() < 1e-3, "the low end");
        assert!((spread(100.0, 0.2, 1.0) - 120.0).abs() < 1e-3, "the high end");
        assert!(spread(100.0, 0.2, 0.5) > spread(100.0, 0.2, -0.5), "a higher draw hits harder");
        assert_eq!(spread(100.0, 0.0, 1.0), 100.0, "no spread, no range");
    }

    #[test]
    fn test_gap_factor_equal_levels_is_one() {
        assert!((gap_factor(10, 10) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_gap_factor_10_level_gap() {
        let f = gap_factor(10, 20);
        assert!((f - 0.333).abs() < 0.01, "10 level gap should be ~0.333, got {f}");
    }

    #[test]
    fn test_gap_factor_20_level_gap_near_zero() {
        assert!(gap_factor(10, 30) < 0.05);
    }

    #[test]
    fn test_gap_factor_beneficiary_higher_stays_one() {
        assert!((gap_factor(20, 10) - 1.0).abs() < 0.001);
    }
}
