//! Damage and contest arithmetic: the level gap, the two contest curves,
//! the spread, the crit and the pushback.

use crate::components::ActorAttributes;
use crate::tuning::Tuning;

/// What a blow from a striker of `striker_level` on a target of
/// `target_level` is multiplied by: `Tuning::level_gap` for each level the
/// striker stands above, divided by it for each below, 1 between equals.
/// It reads only the gap, so ten levels weigh the same at any height. Every
/// blow takes it once, where it is queued (`process_deal_damage`), or where
/// it is returned (Counter).
pub fn level_factor(tuning: &Tuning, striker_level: u32, target_level: u32) -> f32 {
    tuning.level_gap.powi(striker_level as i32 - target_level as i32)
}

/// The contest points the level gap gives an actor of `level` against one
/// of `opposing_level`: `Tuning::contest_per_level` a level, positive for
/// the higher, negative for the lower, nothing between equals. It weighs in
/// every relative contest, so an actor wins each a little against those
/// below it whatever it has invested.
pub fn level_edge(tuning: &Tuning, level: u32, opposing_level: u32) -> f32 {
    (level as f32 - opposing_level as f32) * tuning.contest_per_level
}

/// The nullifying contest: nothing at or below parity.

/// Returns 0 up to 1, never reaching it, as a share does:
/// - Equal/losing → 0 (effect nullified)
/// - `Tuning::contest_scale` advantage → 0.5, half the effect's ceiling
/// - Past it, `lead / (lead + contest_scale)`, so no lead at any level
///   wins the whole of an effect

/// Used by: pushback, combo unlock, recovery speed, crit chance, auto-attack pace.
/// `edge` is the level gap's contest points on the advantage side ([`level_edge`]).
pub fn contest_factor(tuning: &Tuning, advantage_stat: u16, counter_stat: u16, edge: f32) -> f32 {
    let delta = advantage_stat as f32 - counter_stat as f32 + edge;
    if delta <= 0.0 {
        return 0.0;
    }

    delta / (delta + tuning.contest_scale)
}

/// The reaction window's contest, which keeps a baseline.

/// Returns 1.0 up to 1.0 + `Tuning::window_bonus`, never reaching it:
/// - Equal/losing → 1.0 (baseline window preserved)
/// - Past it, the window bonus times [`contest_factor`]'s curve

/// Used ONLY by reaction window to ensure playable baseline.
/// `edge` is the level gap's contest points on the defender's side ([`level_edge`]).
pub fn reaction_contest_factor(tuning: &Tuning, reflex: u16, flow: u16, edge: f32) -> f32 {
    let delta = reflex as f32 - flow as f32 + edge;
    if delta <= 0.0 {
        return 1.0;
    }

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

/// `Tuning::crit_chance` × contest_factor(the
/// attacker's Focus, the defender's Fitness), with the level gap's edge
/// on the attacker's side: none at or below parity, and never the whole
/// of the ceiling.
pub fn crit_chance(tuning: &Tuning, attacker: &ActorAttributes, defender: &ActorAttributes) -> f32 {
    let edge = level_edge(tuning, attacker.total_level(), defender.total_level());
    tuning.crit_chance * contest_factor(tuning, attacker.focus(), defender.fitness(), edge)
}

/// A blow's damage after its crit roll: `Tuning::crit_power` times
/// `damage`, `power` more again, where `draw`, from 0 to 1, falls under the
/// chance `attacker` crits on `defender` ([`crit_chance`]) and `extra`
/// beside it, else as it was; an `extra` of 1 makes it certain. The chance
/// decides whether; `power` is a patient striker's, on an overcommitted foe.
/// Rolled as the blow enters the queue, so a crit stands there at its full
/// weight for the defender to see.
pub fn crit(tuning: &Tuning, damage: f32, attacker: &ActorAttributes, defender: &ActorAttributes, extra: f32, power: f32, draw: f32) -> f32 {
    if draw < crit_chance(tuning, attacker, defender) + extra {
        damage * tuning.crit_power * (1.0 + power)
    } else {
        damage
    }
}

/// Calculate recovery pushback percentage: Impact's, alone.

/// `Tuning::pushback_share` × contest_factor(Impact,
/// Efficiency), with the level gap's `edge` on the attacker's side. No ceiling: the recovery itself
/// never stretches past twice its length.
pub fn calculate_recovery_pushback(
    tuning: &Tuning,
    attacker_impact: u16,
    defender_efficiency: u16,
    edge: f32,
) -> f32 {
    tuning.pushback_share * contest_factor(tuning, attacker_impact, defender_efficiency, edge)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_lead_wins_a_whole_effect() {
        let tuning = Tuning::DEFAULT;
        let mut last = 0.0;
        for lead in [1, 50, 500, 5_000, 60_000] {
            let contest = contest_factor(&tuning, lead, 0, level_edge(&tuning, 100, 1));
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
    fn focus_over_fitness_decides_whether_a_blow_crits_and_never_how_hard() {
        let tuning = Tuning::DEFAULT;
        let focused = ActorAttributes::new(0, 0, 0, 0, 0, 0, 10, 0, 0);
        let keen = ActorAttributes::new(0, 0, 0, 0, 0, 0, 5, 0, 0);
        let tough = ActorAttributes::new(0, 0, 0, -10, 0, 0, 0, 0, 0);
        let plain = ActorAttributes::default();
        assert!(crit(&tuning, 100.0, &focused, &plain, 0.0, 0.0, 0.0) > 100.0, "a draw under the chance crits");
        assert_eq!(crit(&tuning, 100.0, &focused, &plain, 0.0, 0.0, 0.999), 100.0, "a draw over it does not");
        assert_eq!(crit(&tuning, 100.0, &plain, &plain, 0.0, 0.0, 0.0), 100.0, "without a Focus lead nothing crits");
        assert_eq!(crit_chance(&tuning, &focused, &tough), 0.0, "Fitness that matches it nullifies it");
        assert!(crit_chance(&tuning, &focused, &plain) > crit_chance(&tuning, &keen, &plain), "a wider lead crits more often");
        assert_eq!(crit(&tuning, 100.0, &focused, &plain, 0.0, 0.0, 0.0), crit(&tuning, 100.0, &keen, &plain, 0.0, 0.0, 0.0), "and no harder");
        assert!(crit(&tuning, 100.0, &plain, &plain, 0.5, 0.0, 0.25) > 100.0, "what Patience adds beside the contest crits too");
        assert!(crit(&tuning, 100.0, &plain, &plain, 1.0, 0.0, 0.999) > 100.0, "a whole extra chance is certain");
        assert!(crit(&tuning, 100.0, &focused, &plain, 0.0, 0.2, 0.0) > crit(&tuning, 100.0, &focused, &plain, 0.0, 0.0, 0.0), "power lands it harder");
    }

    #[test]
    fn a_level_edge_favours_the_higher_level_in_a_contest() {
        let tuning = Tuning::DEFAULT;
        assert_eq!(level_edge(&tuning, 10, 10), 0.0, "equals, no edge");
        assert!(level_edge(&tuning, 10, 6) > 0.0 && level_edge(&tuning, 6, 10) < 0.0, "the higher level's edge, the lower's deficit");
        assert_eq!(contest_factor(&tuning, 100, 100, 0.0), 0.0, "equal stats nullify");
        assert!(contest_factor(&tuning, 100, 100, level_edge(&tuning, 10, 6)) > 0.0, "a level edge wins an even contest");
        assert!(contest_factor(&tuning, 150, 100, level_edge(&tuning, 6, 10)) < contest_factor(&tuning, 150, 100, 0.0), "outleveled, an advantage shrinks");
        assert_eq!(reaction_contest_factor(&tuning, 0, 0, level_edge(&tuning, 6, 10)), 1.0, "an outleveled window keeps its base");
        assert!(reaction_contest_factor(&tuning, 0, 0, level_edge(&tuning, 10, 6)) > 1.0, "a higher-level defender's window grows");
    }
}
