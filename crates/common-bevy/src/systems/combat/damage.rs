//! Damage calculation functions for combat system

//! Two contest patterns:
//! - Pattern 1 (Nullifying): base × gap × contest_factor → nullifies at equal investment
//! - Pattern 2 (Baseline+Bonus): base × gap × (1.0 + k × contest_factor) → preserves baseline



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

/// A blow's damage after its crit roll: `damage` times `attacker`'s crit
/// multiplier where `draw`, from 0 to 1, falls under its crit chance, else
/// as it was. Rolled as the blow enters the queue, so a crit stands there
/// at its full weight for the defender to see.
pub fn crit(damage: f32, attacker: &ActorAttributes, draw: f32) -> f32 {
    if draw < attacker.crit_chance() {
        damage * attacker.crit_multiplier()
    } else {
        damage
    }
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

/// The share of a landed blow that spills onto another hostile within the
/// striker's reach.

/// Pattern 1 (Nullifying): `Tuning::spill_share` × contest_factor(the
/// striker's Presence, that hostile's Toughness), with the level gap's
/// `edge` on the striker's side.
pub fn spill_share(presence: u16, toughness: u16, edge: f32) -> f32 {
    crate::tuning::tuning().spill_share * contest_factor(presence, toughness, edge)
}

/// Apply passive mitigation to damage (unified for all damage types).

/// Pattern 1 (Nullifying): `Tuning::mitigation_share` × contest_factor(the
/// defender's Toughness, the attacker's Presence), with the level gap's
/// `edge` on the defender's side and no ceiling: past 100% the blow does nothing.
pub fn apply_passive_modifiers(
    outgoing_damage: f32,
    attrs: &ActorAttributes,
    attacker_presence: u16,
    edge: f32,
) -> f32 {
    let mitigation = crate::tuning::tuning().mitigation_share * contest_factor(attrs.toughness(), attacker_presence, edge);
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
    fn a_crit_lands_only_under_the_chance() {
        let instinct = ActorAttributes::new(0, 0, 0, 0, 0, 0, -10, 0, 0);
        let plain = ActorAttributes::default();
        assert!(crit(100.0, &instinct, 0.0) > 100.0, "a draw under the chance crits");
        assert_eq!(crit(100.0, &instinct, 0.999), 100.0, "a draw over it does not");
        assert_eq!(crit(100.0, &plain, 0.0), 100.0, "without Intuition nothing crits");
    }

    #[test]
    fn toughness_mitigates_and_the_attackers_presence_meets_it() {
        let vital = ActorAttributes::new(0, 0, 0, -10, 0, 0, 0, 0, 0);
        let plain = ActorAttributes::default();
        let taken = |attrs: &ActorAttributes, presence: u16| apply_passive_modifiers(100.0, attrs, presence, 0.0);
        assert_eq!(taken(&plain, 0), 100.0, "no Vitality, no Toughness");
        assert!(taken(&vital, 0) < 100.0, "Vitality's Toughness mitigates");
        assert!(taken(&vital, 50) > taken(&vital, 0), "Presence meets it");
        assert_eq!(taken(&vital, vital.toughness()), 100.0, "matched, it nullifies");
    }

    #[test]
    fn presence_spills_what_toughness_does_not_hold() {
        assert_eq!(spill_share(0, 0, 0.0), 0.0, "no Presence, no spill");
        assert!(spill_share(100, 0, 0.0) > 0.0, "Presence spills onto the unarmoured");
        assert!(spill_share(100, 50, 0.0) < spill_share(100, 0, 0.0), "Toughness holds some back");
        assert_eq!(spill_share(100, 100, 0.0), 0.0, "matched, it nullifies");
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
