//! Damage arithmetic: the level gap and the spread.

use crate::tuning::Tuning;

/// What a blow from a striker of `striker_level` on a target of
/// `target_level` is multiplied by: `Tuning::level_gap` for each level the
/// striker stands above, divided by it for each below, 1 between equals.
/// It reads only the gap, so ten levels weigh the same at any height. Every
/// attack takes it once, as it resolves.
pub fn level_factor(tuning: &Tuning, striker_level: u32, target_level: u32) -> f32 {
    tuning.level_gap.powi(striker_level as i32 - target_level as i32)
}

/// An attack's damage within its range: `spread` of `damage` either side of
/// it, where `draw`, from -1 to 1, falls. Every attack's damage is a range,
/// so two near-even actors trade wins instead of one winning by a sliver
/// every time.
pub fn spread(damage: f32, spread: f32, draw: f32) -> f32 {
    damage * (1.0 + spread * draw.clamp(-1.0, 1.0))
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
    fn a_level_gap_weighs_the_same_at_any_height() {
        let tuning = Tuning::DEFAULT;
        assert_eq!(level_factor(&tuning, 10, 10), 1.0, "equals, as struck");
        assert!(level_factor(&tuning, 10, 6) > 1.0 && level_factor(&tuning, 6, 10) < 1.0, "harder down, lighter up");
        assert_eq!(level_factor(&tuning, 14, 10), level_factor(&tuning, 34, 30), "only the gap counts");
    }
}
