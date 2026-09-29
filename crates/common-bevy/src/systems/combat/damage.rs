//! Damage calculation functions for combat system

//! Two contest patterns:
//! - Pattern 1 (Nullifying): base × gap × contest_factor → nullifies at equal investment
//! - Pattern 2 (Baseline+Bonus): base × gap × (1.0 + k × contest_factor) → preserves baseline



use bevy::prelude::Entity;
use crate::components::reaction_queue::DamageType;
use crate::components::ActorAttributes;

/// The contest points the level gap gives an actor of `level` against one
/// of `opposing_level`: `Tuning::contest_per_level` a level, positive for
/// the higher, negative for the lower, nothing between equals. It weighs in
/// every relative contest, so an actor wins each a little against those
/// below it whatever it has invested.
pub fn level_edge(level: u32, opposing_level: u32) -> f32 {
    (level as f32 - opposing_level as f32) * crate::tuning::tuning().contest_per_level
}

/// Contest factor (Pattern 1: Nullifying).

/// Returns 0 and up, with no ceiling:
/// - Equal/losing → 0 (effect nullified)
/// - `Tuning::contest_scale` advantage → 1.0, the effect's base share
/// - Past it, growing as the square root: four times it → 2.0

/// Used by: mitigation, pushback, healing reduction, synergy, recovery speed.
/// `edge` is the level gap's contest points on the advantage side ([`level_edge`]).
pub fn contest_factor(advantage_stat: u16, counter_stat: u16, edge: f32) -> f32 {
    let delta = advantage_stat as f32 - counter_stat as f32 + edge;
    if delta <= 0.0 {
        return 0.0;
    }

    (delta / crate::tuning::tuning().contest_scale).sqrt()
}

/// Reaction window contest (Pattern 2: Baseline+Bonus).

/// Returns 1.0 and up:
/// - Equal/losing → 1.0 (baseline window preserved)
/// - The base advantage → 1.0 + `Tuning::window_bonus`

/// Used ONLY by reaction window to ensure playable baseline.
/// `edge` is the level gap's contest points on the defender's side ([`level_edge`]).
pub fn reaction_contest_factor(reflex: u16, flow: u16, edge: f32) -> f32 {
    let delta = reflex as f32 - flow as f32 + edge;
    if delta <= 0.0 {
        return 1.0;
    }

    let tuning = crate::tuning::tuning();
    1.0 + (delta / tuning.contest_scale).sqrt() * tuning.window_bonus
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

/// Calculate recovery pushback percentage: Impact's, alone.

/// Pattern 1 (Nullifying): `Tuning::pushback_share` × contest_factor(Impact,
/// Composure), with the level gap's `edge` on the attacker's side. No ceiling: the lockout itself
/// never stretches past twice its length.

/// Applied to effective_recovery_base (after composure, before synergy).
pub fn calculate_recovery_pushback(
    attacker_impact: u16,
    defender_composure: u16,
    edge: f32,
) -> f32 {
    crate::tuning::tuning().pushback_share * contest_factor(attacker_impact, defender_composure, edge)
}

/// Scan for the strongest Presence aura within range of target, among
/// `actors`, each read with its own position.
pub fn find_max_presence_in_range(
    target: Entity,
    actors: &bevy::prelude::Query<(&crate::components::Loc, &ActorAttributes)>,
) -> u16 {
    const RADIUS: i32 = 5;

    let Ok((target_loc, _)) = actors.get(target) else {
        return 0;
    };

    actors.iter()
        .filter(|(loc, _)| target_loc.flat_distance(loc) as i32 <= RADIUS)
        .map(|(_, attrs)| attrs.presence())
        .max()
        .unwrap_or(0)
}

/// Apply passive mitigation to damage (unified for all damage types).

/// Pattern 1 (Nullifying): `Tuning::mitigation_share` × contest_factor(Toughness,
/// Presence), with no ceiling: past 100% the blow does nothing.
pub fn apply_passive_modifiers(
    outgoing_damage: f32,
    attrs: &ActorAttributes,
    max_dominance_in_range: u16,
    edge: f32,
) -> f32 {
    let mitigation = crate::tuning::tuning().mitigation_share * contest_factor(attrs.toughness(), max_dominance_in_range, edge);
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
    fn a_level_edge_favours_the_higher_level_in_a_contest() {
        assert_eq!(level_edge(10, 10), 0.0, "equals, no edge");
        assert!(level_edge(10, 6) > 0.0 && level_edge(6, 10) < 0.0, "the higher level's edge, the lower's deficit");
        assert_eq!(contest_factor(100, 100, 0.0), 0.0, "equal stats nullify");
        assert!(contest_factor(100, 100, level_edge(10, 6)) > 0.0, "a level edge wins an even contest");
        assert!(contest_factor(150, 100, level_edge(6, 10)) < contest_factor(150, 100, 0.0), "outleveled, an advantage shrinks");
        assert_eq!(reaction_contest_factor(0, 0, level_edge(6, 10)), 1.0, "an outleveled window keeps its base");
        assert!(reaction_contest_factor(0, 0, level_edge(10, 6)) > 1.0, "a higher-level defender's window grows");
    }
}
