//! Damage calculation functions for combat system

//! Two contest patterns:
//! - Pattern 1 (Nullifying): base × gap × contest_factor → nullifies at equal investment
//! - Pattern 2 (Baseline+Bonus): base × gap × (1.0 + k × contest_factor) → preserves baseline



use bevy::prelude::Entity;
use crate::components::reaction_queue::DamageType;
use crate::components::ActorAttributes;

/// Level gap scaling factor.

/// Contest points each level of gap is worth, on the higher-level side of
/// every relative contest.
pub const CONTEST_PER_LEVEL: f32 = 20.0;

/// How a level gap weighs in the relative contests: a flat number of contest
/// points per level, on the higher-level side of every one, so an actor wins
/// each contest a little against those below it whatever it has invested.
/// The game plays on [`CONTEST_PER_LEVEL`]; the balance arena tries others.
#[derive(bevy::prelude::Resource, Clone, Copy, Debug)]
pub struct LevelContest {
    pub per_level: f32,
}

impl Default for LevelContest {
    fn default() -> Self {
        Self { per_level: CONTEST_PER_LEVEL }
    }
}

impl LevelContest {
    /// The contest points the level gap gives an actor of `level` against
    /// one of `opposing_level`: positive for the higher, negative for the
    /// lower, nothing between equals.
    pub fn edge(&self, level: u32, opposing_level: u32) -> f32 {
        (level as f32 - opposing_level as f32) * self.per_level
    }
}

/// Contest factor (Pattern 1: Nullifying).

/// Returns 0 to 1.0:
/// - Equal/losing → 0 (effect nullified)
/// - Max advantage (300 delta) → 1.0 (full effect)
/// - Half advantage (150 delta) → 0.707 (~71% benefit)

/// Used by: mitigation, pushback, healing reduction, synergy, recovery speed.
/// `edge` is the level gap's contest points on the advantage side ([`LevelContest::edge`]).
pub fn contest_factor(advantage_stat: u16, counter_stat: u16, edge: f32) -> f32 {
    let delta = advantage_stat as f32 - counter_stat as f32 + edge;
    if delta <= 0.0 {
        return 0.0;
    }

    let normalized = (delta / 300.0).min(1.0);
    normalized.sqrt()
}

/// Reaction window contest (Pattern 2: Baseline+Bonus).

/// Returns 1.0 to 1.5:
/// - Equal/losing → 1.0 (baseline window preserved)
/// - Max advantage → 1.5 (50% improvement)

/// Used ONLY by reaction window to ensure playable baseline.
/// `edge` is the level gap's contest points on the defender's side ([`LevelContest::edge`]).
pub fn reaction_contest_factor(cunning: u16, finesse: u16, edge: f32) -> f32 {
    let delta = cunning as f32 - finesse as f32 + edge;
    if delta <= 0.0 {
        return 1.0;
    }

    let normalized = (delta / 300.0).min(1.0);
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

/// Calculate recovery pushback percentage: Impact's, alone.

/// Pattern 1 (Nullifying): 50% × contest_factor(Impact, Composure), capped at
/// 50%, with the level gap's `edge` on the attacker's side.

/// Applied to effective_recovery_base (after composure, before synergy).
pub fn calculate_recovery_pushback(
    attacker_impact: u16,
    defender_composure: u16,
    edge: f32,
) -> f32 {
    const BASE_PUSHBACK: f32 = 0.50;
    const MAX_PUSHBACK: f32 = 0.50;

    let contest = contest_factor(attacker_impact, defender_composure, edge);

    (BASE_PUSHBACK * contest).min(MAX_PUSHBACK)
}

/// Scan for the strongest Dominance aura within range of target, among
/// `actors`, each read with its own position.
pub fn find_max_dominance_in_range(
    target: Entity,
    actors: &bevy::prelude::Query<(&crate::components::Loc, &ActorAttributes)>,
) -> u16 {
    const RADIUS: i32 = 5;

    let Ok((target_loc, _)) = actors.get(target) else {
        return 0;
    };

    actors.iter()
        .filter(|(loc, _)| target_loc.flat_distance(loc) as i32 <= RADIUS)
        .map(|(_, attrs)| attrs.dominance())
        .max()
        .unwrap_or(0)
}

/// Apply passive mitigation to damage (unified for all damage types).

/// Pattern 1 (Nullifying): base × gap × contest_factor
/// Base: 75%, Cap: 75%
pub fn apply_passive_modifiers(
    outgoing_damage: f32,
    attrs: &ActorAttributes,
    max_dominance_in_range: u16,
    edge: f32,
) -> f32 {
    const BASE_MITIGATION: f32 = 0.75;
    const MAX_MITIGATION: f32 = 0.75;

    let toughness = attrs.toughness();
    let contest = contest_factor(toughness, max_dominance_in_range, edge);

    let mitigation = (BASE_MITIGATION * contest).min(MAX_MITIGATION);
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
        let contest = LevelContest { per_level: 20.0 };
        assert_eq!(contest.edge(10, 10), 0.0, "equals, no edge");
        assert!(contest.edge(10, 6) > 0.0 && contest.edge(6, 10) < 0.0, "the higher level's edge, the lower's deficit");
        assert_eq!(contest_factor(100, 100, 0.0), 0.0, "equal stats nullify");
        assert!(contest_factor(100, 100, contest.edge(10, 6)) > 0.0, "a level edge wins an even contest");
        assert!(contest_factor(150, 100, contest.edge(6, 10)) < contest_factor(150, 100, 0.0), "outleveled, an advantage shrinks");
        assert_eq!(reaction_contest_factor(0, 0, contest.edge(6, 10)), 1.0, "an outleveled window keeps its base");
        assert!(reaction_contest_factor(0, 0, contest.edge(10, 6)) > 1.0, "a higher-level defender's window grows");
    }
}
