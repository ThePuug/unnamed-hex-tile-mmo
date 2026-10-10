//! How hard an incoming threat would hit, as a colour: shared by the
//! highway, the resolved stack and the combat log.

use bevy::prelude::*;

use common_bevy::components::reaction_queue::*;
use common_bevy::components::resources::Health;

/// Map severity (estimated_damage / max_health) to an RGB color.
///
/// - 0–10%: Muted yellow-green → yellow
/// - 10–30%: Yellow → orange
/// - 30%+: Orange → intense red
pub fn severity_rgb(severity: f32) -> (f32, f32, f32) {
    let s = severity.clamp(0.0, 1.0);
    if s < 0.1 {
        // Muted yellow-green → yellow
        let t = s / 0.1;
        (
            0.6 + 0.4 * t, // 0.6 → 1.0
            0.8 - 0.1 * t, // 0.8 → 0.7
            0.2 * (1.0 - t), // 0.2 → 0.0
        )
    } else if s < 0.3 {
        // Yellow → orange
        let t = (s - 0.1) / 0.2;
        (
            1.0,
            0.7 - 0.35 * t, // 0.7 → 0.35
            0.0,
        )
    } else {
        // Orange → intense red
        let t = ((s - 0.3) / 0.3).min(1.0);
        (
            1.0,
            0.35 * (1.0 - t), // 0.35 → 0.0
            0.0,
        )
    }
}

/// What `threat` would deal the player as it lands, a wound's DoT
/// included.
pub fn estimate(threat: &QueuedThreat) -> f32 {
    threat.damage + threat.dot_left()
}

/// The share of the player's health `threat` would take as it lands, for
/// its colour.
pub fn severity(threat: &QueuedThreat, health: &Health) -> f32 {
    if health.max > 0.0 {
        (estimate(threat) / health.max).clamp(0.0, 1.0)
    } else {
        1.0
    }
}

/// The colour of damage over time, apart from every blow's
pub const DOT_COLOR: Color = Color::srgb(0.65, 0.3, 0.95);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_severity_rgb_clamped_above_one() {
        let (r, g, b) = severity_rgb(2.0);
        // Should clamp to max severity (same as 1.0)
        let (r1, g1, b1) = severity_rgb(1.0);
        assert!((r - r1).abs() < 0.01);
        assert!((g - g1).abs() < 0.01);
        assert!((b - b1).abs() < 0.01);
    }

    #[test]
    fn test_severity_rgb_monotonic_red() {
        // Red channel should be monotonically non-decreasing
        let severities = [0.0, 0.05, 0.1, 0.2, 0.3, 0.5, 0.8, 1.0];
        let mut prev_r = 0.0;
        for s in severities {
            let (r, _, _) = severity_rgb(s);
            assert!(r >= prev_r - 0.01, "Red not monotonic at severity={s}: {r} < {prev_r}");
            prev_r = r;
        }
    }

    #[test]
    fn test_severity_rgb_green_decreasing() {
        // Green should generally decrease (gets more red over time)
        let (_, g_low, _) = severity_rgb(0.05);
        let (_, g_high, _) = severity_rgb(0.5);
        assert!(g_low > g_high, "Green should decrease: {g_low} vs {g_high}");
    }
}
