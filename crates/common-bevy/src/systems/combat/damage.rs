//! Damage calculation functions for combat system

//! Two contest patterns:
//! - Pattern 1 (Nullifying): base × gap × contest_factor → nullifies at equal investment
//! - Pattern 2 (Baseline+Bonus): base × gap × (1.0 + k × contest_factor) → preserves baseline



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

/// Returns 0 up to 1, never reaching it, as a share does:
/// - Equal/losing → 0 (effect nullified)
/// - `Tuning::contest_scale` advantage → 0.5, half the effect's ceiling
/// - Past it, `lead / (lead + contest_scale)`, so no lead at any level
///   wins the whole of an effect

/// Used by: mitigation, pushback, healing reduction, combo unlock, recovery speed.
/// `edge` is the level gap's contest points on the advantage side ([`level_edge`]).
pub fn contest_factor(advantage_stat: u16, counter_stat: u16, edge: f32) -> f32 {
    let delta = advantage_stat as f32 - counter_stat as f32 + edge;
    if delta <= 0.0 {
        return 0.0;
    }

    delta / (delta + crate::tuning::tuning().contest_scale)
}

/// Reaction window contest (Pattern 2: Baseline+Bonus).

/// Returns 1.0 up to 1.0 + `Tuning::window_bonus`, never reaching it:
/// - Equal/losing → 1.0 (baseline window preserved)
/// - Past it, the window bonus times [`contest_factor`]'s curve

/// Used ONLY by reaction window to ensure playable baseline.
/// `edge` is the level gap's contest points on the defender's side ([`level_edge`]).
pub fn reaction_contest_factor(reflex: u16, flow: u16, edge: f32) -> f32 {
    let delta = reflex as f32 - flow as f32 + edge;
    if delta <= 0.0 {
        return 1.0;
    }

    let tuning = crate::tuning::tuning();
    1.0 + delta / (delta + tuning.contest_scale) * tuning.window_bonus
}

/// An attack's damage within its range: `spread` of `damage` either side of
/// it, where `draw`, from -1 to 1, falls. A roll, not a crit: every attack's
/// damage is a range, so two near-even actors trade wins instead of one
/// winning by a sliver every time.
pub fn spread(damage: f32, spread: f32, draw: f32) -> f32 {
    damage * (1.0 + spread * draw.clamp(-1.0, 1.0))
}

/// The chance a blow `attacker` strikes on `defender` crits.

/// Pattern 1 (Nullifying): `Tuning::crit_chance` × contest_factor(the
/// attacker's Focus, the defender's Toughness), with the level gap's edge
/// on the attacker's side: none at or below parity, and never the whole
/// of the ceiling.
pub fn crit_chance(attacker: &ActorAttributes, defender: &ActorAttributes) -> f32 {
    let edge = level_edge(attacker.total_level(), defender.total_level());
    crate::tuning::tuning().crit_chance * contest_factor(attacker.focus(), defender.toughness(), edge)
}

/// A blow's damage after its crit roll: `Tuning::crit_power` times
/// `damage` where `draw`, from 0 to 1, falls under the chance `attacker`
/// crits on `defender` ([`crit_chance`]), else as it was. The contest
/// decides whether, never how hard. Rolled as the blow enters the queue,
/// so a crit stands there at its full weight for the defender to see.
pub fn crit(damage: f32, attacker: &ActorAttributes, defender: &ActorAttributes, draw: f32) -> f32 {
    if draw < crit_chance(attacker, defender) {
        damage * crate::tuning::tuning().crit_power
    } else {
        damage
    }
}

/// Calculate recovery pushback percentage: Impact's, alone.

/// Pattern 1 (Nullifying): `Tuning::pushback_share` × contest_factor(Impact,
/// Composure), with the level gap's `edge` on the attacker's side. No ceiling: the recovery itself
/// never stretches past twice its length.

/// Applied to effective_recovery_base (after composure, before the combo).
pub fn calculate_recovery_pushback(
    attacker_impact: u16,
    defender_composure: u16,
    edge: f32,
) -> f32 {
    crate::tuning::tuning().pushback_share * contest_factor(attacker_impact, defender_composure, edge)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_lead_wins_a_whole_effect() {
        let mut last = 0.0;
        for lead in [1, 50, 500, 5_000, 60_000] {
            let contest = contest_factor(lead, 0, level_edge(100, 1));
            assert!(contest > last, "a wider lead wins more");
            assert!(contest < 1.0, "a lead of {lead} reached the ceiling");
            last = contest;
        }
    }

    #[test]
    fn a_spread_is_a_range_about_the_damage() {
        assert_eq!(spread(100.0, 0.2, 0.0), 100.0, "the middle of the range is the damage");
        assert!((spread(100.0, 0.2, -1.0) - 80.0).abs() < 1e-3, "the low end");
        assert!((spread(100.0, 0.2, 1.0) - 120.0).abs() < 1e-3, "the high end");
        assert!(spread(100.0, 0.2, 0.5) > spread(100.0, 0.2, -0.5), "a higher draw hits harder");
        assert_eq!(spread(100.0, 0.0, 1.0), 100.0, "no spread, no range");
    }

    #[test]
    fn focus_over_toughness_decides_whether_a_blow_crits_and_never_how_hard() {
        let focused = ActorAttributes::new(0, 0, 0, 0, 0, 0, 10, 0, 0);
        let keen = ActorAttributes::new(0, 0, 0, 0, 0, 0, 5, 0, 0);
        let tough = ActorAttributes::new(0, 0, 0, -10, 0, 0, 0, 0, 0);
        let plain = ActorAttributes::default();
        assert!(crit(100.0, &focused, &plain, 0.0) > 100.0, "a draw under the chance crits");
        assert_eq!(crit(100.0, &focused, &plain, 0.999), 100.0, "a draw over it does not");
        assert_eq!(crit(100.0, &plain, &plain, 0.0), 100.0, "without a Focus lead nothing crits");
        assert_eq!(crit_chance(&focused, &tough), 0.0, "Toughness that matches it nullifies it");
        assert!(crit_chance(&focused, &plain) > crit_chance(&keen, &plain), "a wider lead crits more often");
        assert_eq!(crit(100.0, &focused, &plain, 0.0), crit(100.0, &keen, &plain, 0.0), "and no harder");
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
