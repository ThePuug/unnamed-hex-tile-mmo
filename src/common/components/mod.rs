pub mod ally_target;
pub mod behaviour;
pub mod engagement;
pub mod entity_type;
pub mod gcd;
pub mod heading;
pub mod keybits;
pub mod movement_intent_state;
pub mod movement_prediction;
pub mod position;
pub mod reaction_queue;
pub mod recovery;
pub mod resources;
pub mod returning;
pub mod target;
pub mod tier_lock;

use bevy::prelude::*;
use qrz::Qrz;
use serde::{Deserialize, Serialize};

#[derive(Clone, Component, Copy, Debug, Default, Deref, DerefMut, Deserialize, Eq, PartialEq, Serialize)]
pub struct Loc(Qrz);

impl Loc {
    pub fn from_qrz(q: i16, r: i16, z: i16) -> Self {
        Loc(Qrz { q, r, z })
    }

    pub fn new(qrz: Qrz) -> Self {
        Loc(qrz)
    }

    /// Check if two locations are adjacent for melee combat with sloping terrain
    ///
    /// Two locations are considered adjacent if:
    /// - They are on the same tile (flat_distance == 0) - for multiple entities on same hex
    /// - OR they are 1 hex apart horizontally (flat_distance == 1) AND the vertical
    ///   difference is at most 1 tile (|z_diff| <= 1)
    ///
    /// This allows melee attacks up/down slopes but prevents attacks
    /// against targets that are too high/low (e.g., 2+ tiles above/below)
    pub fn is_adjacent(&self, other: &Loc) -> bool {
        let flat_dist = self.flat_distance(other);
        let z_diff = (self.z - other.z).abs();

        // Same tile is always adjacent (multiple entities on same hex)
        if flat_dist == 0 {
            return true;
        }

        // Otherwise, must be 1 hex away with at most 1 z-level difference
        flat_dist == 1 && z_diff <= 1
    }

}

#[cfg(test)]
mod loc_tests {
    use super::*;

    #[test]
    fn test_is_adjacent_same_level() {
        let loc1 = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let loc2 = Loc::new(Qrz { q: 1, r: 0, z: 0 });
        assert!(loc1.is_adjacent(&loc2), "Same level, 1 hex apart should be adjacent");
    }

    #[test]
    fn test_is_adjacent_one_level_up() {
        let loc1 = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let loc2 = Loc::new(Qrz { q: 1, r: 0, z: 1 });
        assert!(loc1.is_adjacent(&loc2), "1 level up, 1 hex apart should be adjacent (slope)");
    }

    #[test]
    fn test_is_adjacent_one_level_down() {
        let loc1 = Loc::new(Qrz { q: 0, r: 0, z: 1 });
        let loc2 = Loc::new(Qrz { q: 1, r: 0, z: 0 });
        assert!(loc1.is_adjacent(&loc2), "1 level down, 1 hex apart should be adjacent (slope)");
    }

    #[test]
    fn test_not_adjacent_two_levels_up() {
        let loc1 = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let loc2 = Loc::new(Qrz { q: 1, r: 0, z: 2 });
        assert!(!loc1.is_adjacent(&loc2), "2 levels up should not be adjacent (too steep)");
    }

    #[test]
    fn test_not_adjacent_two_hexes_away() {
        let loc1 = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let loc2 = Loc::new(Qrz { q: 2, r: 0, z: 0 });
        assert!(!loc1.is_adjacent(&loc2), "2 hexes apart should not be adjacent");
    }

    #[test]
    fn test_adjacent_same_tile() {
        // Same tile is adjacent (for multiple entities on same hex)
        let loc1 = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let loc2 = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        assert!(loc1.is_adjacent(&loc2), "Same tile should be adjacent (multiple entities on same hex)");
    }

    #[test]
    fn test_adjacent_same_tile_different_z() {
        // Same horizontal position but different z should be adjacent
        let loc1 = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let loc2 = Loc::new(Qrz { q: 0, r: 0, z: 1 });
        assert!(loc1.is_adjacent(&loc2), "Same tile, different z should be adjacent");
    }

}

#[derive(Clone, Component, Copy, Debug, Default)]
pub struct AirTime {
    pub state: Option<i16>,
    pub step: Option<i16>,
}

#[derive(Clone, Component, Copy, Default)]
pub struct Actor;

/// Attributes for actor entities that affect gameplay mechanics
///
/// Fields store RAW INVESTMENT COUNTS (levels invested):
/// - Axis: negative = left side, positive = right side (max ±127 levels)
/// - Spectrum: flexibility range (max 255 levels)
/// - Shift: tactical position within spectrum range (±spectrum)
///
/// Access scaled values via methods:
/// - Axis: 1 level → 10 reach
/// - Spectrum: 1 level → 7 reach (each direction)
#[derive(Clone, Component, Copy, Debug, Deserialize, Serialize)]
pub struct ActorAttributes {
    // MIGHT ↔ GRACE (Physical Expression)
    // Negative axis = Might specialist, Positive axis = Grace specialist
    might_grace_axis: i8,
    might_grace_spectrum: i8,  // Raw investment count (max 127 levels)
    might_grace_shift: i8,     // Player's chosen shift from axis (within ±spectrum)

    // VITALITY ↔ FOCUS (Endurance Type)
    // Negative axis = Vitality specialist, Positive axis = Focus specialist
    vitality_focus_axis: i8,
    vitality_focus_spectrum: i8,
    vitality_focus_shift: i8,

    // INSTINCT ↔ PRESENCE (Engagement Style)
    // Negative axis = Instinct specialist, Positive axis = Presence specialist
    instinct_presence_axis: i8,
    instinct_presence_spectrum: i8,
    instinct_presence_shift: i8,
}

impl ActorAttributes {
    /// Create new ActorAttributes with raw investment counts
    ///
    /// # Arguments
    /// * axis: negative = left attribute, positive = right attribute (-127 to 127)
    /// * spectrum: flexibility investment (0 to 127)
    /// * shift: tactical adjustment within spectrum range (-spectrum to +spectrum)
    pub fn new(
        might_grace_axis: i8,
        might_grace_spectrum: i8,
        might_grace_shift: i8,
        vitality_focus_axis: i8,
        vitality_focus_spectrum: i8,
        vitality_focus_shift: i8,
        instinct_presence_axis: i8,
        instinct_presence_spectrum: i8,
        instinct_presence_shift: i8,
    ) -> Self {
        Self {
            might_grace_axis,
            might_grace_spectrum: might_grace_spectrum.max(0),  // Clamp spectrum to non-negative
            might_grace_shift,
            vitality_focus_axis,
            vitality_focus_spectrum: vitality_focus_spectrum.max(0),
            vitality_focus_shift,
            instinct_presence_axis,
            instinct_presence_spectrum: instinct_presence_spectrum.max(0),
            instinct_presence_shift,
        }
    }

    // === Raw field accessors ===

    pub fn might_grace_axis(&self) -> i8 { self.might_grace_axis }
    pub fn might_grace_spectrum(&self) -> i8 { self.might_grace_spectrum }
    pub fn might_grace_shift(&self) -> i8 { self.might_grace_shift }

    pub fn vitality_focus_axis(&self) -> i8 { self.vitality_focus_axis }
    pub fn vitality_focus_spectrum(&self) -> i8 { self.vitality_focus_spectrum }
    pub fn vitality_focus_shift(&self) -> i8 { self.vitality_focus_shift }

    pub fn instinct_presence_axis(&self) -> i8 { self.instinct_presence_axis }
    pub fn instinct_presence_spectrum(&self) -> i8 { self.instinct_presence_spectrum }
    pub fn instinct_presence_shift(&self) -> i8 { self.instinct_presence_shift }

    // === Mutators for shift values (tactical adjustments) ===

    pub fn set_might_grace_shift(&mut self, shift: i8) {
        let max_shift = self.might_grace_spectrum.max(0);
        self.might_grace_shift = shift.clamp(-max_shift, max_shift);
    }

    pub fn set_vitality_focus_shift(&mut self, shift: i8) {
        let max_shift = self.vitality_focus_spectrum.max(0);
        self.vitality_focus_shift = shift.clamp(-max_shift, max_shift);
    }

    pub fn set_instinct_presence_shift(&mut self, shift: i8) {
        let max_shift = self.instinct_presence_spectrum.max(0);
        self.instinct_presence_shift = shift.clamp(-max_shift, max_shift);
    }

    // === Private: Get current scaled position (for derived stats like movement speed) ===
    // These calculate the NET position (might vs grace) factoring in both axis and shift
    // Positive = grace side, Negative = might side
    // Returns scaled value based on investment

    fn might_grace_position(&self) -> i16 {
        let axis_scaled = (self.might_grace_axis as i16) * 10;
        let shift_scaled = (self.might_grace_shift as i16) * 7;
        axis_scaled + shift_scaled
    }

    // === MIGHT ↔ GRACE ===

    /// Maximum Might reach
    /// Formula: axis investment * 10 (on might side) + spectrum investment * 7
    pub fn might_reach(&self) -> u16 {
        let spectrum_reach = (self.might_grace_spectrum.max(0) as u16) * 7;

        if self.might_grace_axis <= 0 {
            let axis_reach = (self.might_grace_axis.unsigned_abs() as u16) * 10;
            axis_reach + spectrum_reach
        } else {
            spectrum_reach
        }
    }

    /// Maximum Grace reach
    /// Formula: axis investment * 10 (on grace side) + spectrum investment * 7
    pub fn grace_reach(&self) -> u16 {
        let spectrum_reach = (self.might_grace_spectrum.max(0) as u16) * 7;

        if self.might_grace_axis >= 0 {
            let axis_reach = (self.might_grace_axis.unsigned_abs() as u16) * 10;
            axis_reach + spectrum_reach
        } else {
            spectrum_reach
        }
    }

    /// Current available Might (scaled)
    /// Shift moves spectrum reach between might and grace
    /// Formula: axis×10 + spectrum×7 - shift×7
    pub fn might(&self) -> u16 {
        let spectrum_reach = (self.might_grace_spectrum.max(0) as i16) * 7;
        let shift_scaled = (self.might_grace_shift as i16) * 7;

        if self.might_grace_axis <= 0 {
            // On might side: axis reach + spectrum - shift adjustment
            let axis_reach = (self.might_grace_axis.unsigned_abs() as i16) * 10;
            (axis_reach + spectrum_reach - shift_scaled).max(0) as u16
        } else {
            // On grace side: spectrum - shift adjustment only
            (spectrum_reach - shift_scaled).max(0) as u16
        }
    }

    /// Current available Grace (scaled)
    /// Shift moves spectrum reach between might and grace
    pub fn grace(&self) -> u16 {
        let spectrum_reach = (self.might_grace_spectrum.max(0) as i16) * 7;
        let shift_scaled = (self.might_grace_shift as i16) * 7;

        if self.might_grace_axis >= 0 {
            let axis_reach = (self.might_grace_axis.unsigned_abs() as i16) * 10;
            (axis_reach + spectrum_reach + shift_scaled).max(0) as u16
        } else {
            (spectrum_reach + shift_scaled).max(0) as u16
        }
    }

    // === VITALITY ↔ FOCUS ===

    /// Maximum Vitality reach
    /// Formula: axis investment * 10 (on vitality side) + spectrum investment * 7
    pub fn vitality_reach(&self) -> u16 {
        let spectrum_reach = (self.vitality_focus_spectrum.max(0) as u16) * 7;

        if self.vitality_focus_axis <= 0 {
            // On vitality side or balanced: |axis| * 10 + spectrum * 7
            let axis_reach = (self.vitality_focus_axis.unsigned_abs() as u16) * 10;
            axis_reach + spectrum_reach
        } else {
            // On focus side: spectrum * 7 only
            spectrum_reach
        }
    }

    /// Maximum Focus reach
    /// Formula: axis investment * 10 (on focus side) + spectrum investment * 7
    pub fn focus_reach(&self) -> u16 {
        let spectrum_reach = (self.vitality_focus_spectrum.max(0) as u16) * 7;

        if self.vitality_focus_axis >= 0 {
            // On focus side or balanced: |axis| * 10 + spectrum * 7
            let axis_reach = (self.vitality_focus_axis.unsigned_abs() as u16) * 10;
            axis_reach + spectrum_reach
        } else {
            // On vitality side: spectrum * 7 only
            spectrum_reach
        }
    }

    /// Current available Vitality (scaled)
    /// Shift moves spectrum reach between vitality and focus
    /// Formula: axis×10 + spectrum×7 - shift×7
    pub fn vitality(&self) -> u16 {
        let spectrum_reach = (self.vitality_focus_spectrum.max(0) as i16) * 7;
        let shift_scaled = (self.vitality_focus_shift as i16) * 7;

        if self.vitality_focus_axis <= 0 {
            // On vitality side: axis reach + spectrum - shift adjustment
            let axis_reach = (self.vitality_focus_axis.unsigned_abs() as i16) * 10;
            (axis_reach + spectrum_reach - shift_scaled).max(0) as u16
        } else {
            // On focus side: spectrum - shift adjustment only
            (spectrum_reach - shift_scaled).max(0) as u16
        }
    }

    /// Current available Focus (scaled)
    /// Shift moves spectrum reach between vitality and focus
    /// Formula: spectrum×7 + shift×7 (on vitality side) or axis×10 + spectrum×7 + shift×7 (on focus side)
    pub fn focus(&self) -> u16 {
        let spectrum_reach = (self.vitality_focus_spectrum.max(0) as i16) * 7;
        let shift_scaled = (self.vitality_focus_shift as i16) * 7;

        if self.vitality_focus_axis >= 0 {
            // On focus side: axis reach + spectrum + shift adjustment
            let axis_reach = (self.vitality_focus_axis.unsigned_abs() as i16) * 10;
            (axis_reach + spectrum_reach + shift_scaled).max(0) as u16
        } else {
            // On vitality side: spectrum + shift adjustment only
            (spectrum_reach + shift_scaled).max(0) as u16
        }
    }

    // === INSTINCT ↔ PRESENCE ===

    /// Maximum Instinct reach
    /// Formula: axis investment * 10 (on instinct side) + spectrum investment * 7
    pub fn instinct_reach(&self) -> u16 {
        let spectrum_reach = (self.instinct_presence_spectrum.max(0) as u16) * 7;

        if self.instinct_presence_axis <= 0 {
            let axis_reach = (self.instinct_presence_axis.unsigned_abs() as u16) * 10;
            axis_reach + spectrum_reach
        } else {
            spectrum_reach
        }
    }

    /// Maximum Presence reach
    /// Formula: axis investment * 10 (on presence side) + spectrum investment * 7
    pub fn presence_reach(&self) -> u16 {
        let spectrum_reach = (self.instinct_presence_spectrum.max(0) as u16) * 7;

        if self.instinct_presence_axis >= 0 {
            let axis_reach = (self.instinct_presence_axis.unsigned_abs() as u16) * 10;
            axis_reach + spectrum_reach
        } else {
            spectrum_reach
        }
    }

    /// Current available Instinct (scaled)
    /// Shift moves spectrum reach between instinct and presence
    /// Formula: axis×10 + spectrum×7 - shift×7
    pub fn instinct(&self) -> u16 {
        let spectrum_reach = (self.instinct_presence_spectrum.max(0) as i16) * 7;
        let shift_scaled = (self.instinct_presence_shift as i16) * 7;

        if self.instinct_presence_axis <= 0 {
            // On instinct side: axis reach + spectrum - shift adjustment
            let axis_reach = (self.instinct_presence_axis.unsigned_abs() as i16) * 10;
            (axis_reach + spectrum_reach - shift_scaled).max(0) as u16
        } else {
            // On presence side: spectrum - shift adjustment only
            (spectrum_reach - shift_scaled).max(0) as u16
        }
    }

    /// Current available Presence (scaled)
    /// Shift moves spectrum reach between instinct and presence
    /// Formula: spectrum×7 + shift×7 (on instinct side) or axis×10 + spectrum×7 + shift×7 (on presence side)
    pub fn presence(&self) -> u16 {
        let spectrum_reach = (self.instinct_presence_spectrum.max(0) as i16) * 7;
        let shift_scaled = (self.instinct_presence_shift as i16) * 7;

        if self.instinct_presence_axis >= 0 {
            // On presence side: axis reach + spectrum + shift adjustment
            let axis_reach = (self.instinct_presence_axis.unsigned_abs() as i16) * 10;
            (axis_reach + spectrum_reach + shift_scaled).max(0) as u16
        } else {
            // On instinct side: spectrum + shift adjustment only
            (spectrum_reach + shift_scaled).max(0) as u16
        }
    }

    /// Calculate total level from invested attribute points
    /// Each level grants 1 point to invest in any axis or spectrum
    /// Fields store raw investment counts, so sum directly
    pub fn total_level(&self) -> u32 {
        let mg_points = self.might_grace_axis.unsigned_abs() as u32 + self.might_grace_spectrum.max(0) as u32;
        let vf_points = self.vitality_focus_axis.unsigned_abs() as u32 + self.vitality_focus_spectrum.max(0) as u32;
        let ip_points = self.instinct_presence_axis.unsigned_abs() as u32 + self.instinct_presence_spectrum.max(0) as u32;
        mg_points + vf_points + ip_points
    }

    // === LEVEL MULTIPLIER (ADR-020) ===

    /// Pure level multiplier for super-linear stat scaling
    /// Formula: (1 + level * k)^p
    /// Level 0 always returns 1.0 (backward compatible)
    pub fn level_multiplier(level: u32, k: f32, p: f32) -> f32 {
        (1.0 + level as f32 * k).powf(p)
    }

    /// HP/survivability level multiplier
    /// Moderate scaling: preserves danger from equal-level foes
    pub fn hp_level_multiplier(&self) -> f32 {
        const K: f32 = 0.10;
        const P: f32 = 1.5;
        Self::level_multiplier(self.total_level(), K, P)
    }

    /// Damage/offense level multiplier
    /// Aggressive scaling: rewards offensive power at high levels
    pub fn damage_level_multiplier(&self) -> f32 {
        const K: f32 = 0.15;
        const P: f32 = 2.0;
        Self::level_multiplier(self.total_level(), K, P)
    }

    /// Reaction stat level multiplier
    /// Gentle scaling: bounded by human reaction limits
    pub fn reaction_level_multiplier(&self) -> f32 {
        const K: f32 = 0.10;
        const P: f32 = 1.2;
        Self::level_multiplier(self.total_level(), K, P)
    }

    // === DERIVED ATTRIBUTES ===

    /// Movement speed derived from grace position (axis + shift)
    /// Higher grace = higher movement speed
    /// Scaled formula: max(75, 100 + (position / 10))
    ///
    /// Position = -500 (level 50 Might specialist): speed = 75% (clamped, 0.00375)
    /// Position = 0 (parity): speed = 100% (baseline, 0.005)
    /// Position = 250: speed = 125% (+25%, 0.00625)
    /// Position = 500 (level 50 Grace specialist): speed = 150% (+50%, 0.0075)
    pub fn movement_speed(&self) -> f32 {
        const BASE_SPEED: f32 = 0.005;  // World units per millisecond (MOVEMENT_SPEED from physics.rs)

        let position = self.might_grace_position() as f32;  // Scaled: -500 to +500 for level 50
        let speed_percent = (100.0 + (position / 10.0)).max(75.0);  // 75 to 150
        BASE_SPEED * (speed_percent / 100.0)
    }

    /// Maximum health derived from vitality_reach, scaled by level multiplier (ADR-020)
    /// Linear formula: 100 + (vitality_reach * 3.8)
    /// Then multiplied by hp_level_multiplier for super-linear scaling
    pub fn max_health(&self) -> f32 {
        let base = 100.0;
        let vitality_reach = self.vitality_reach() as f32;
        let linear = base + (vitality_reach * 3.8);
        linear * self.hp_level_multiplier()
    }
}

impl Default for ActorAttributes {
    fn default() -> Self {
        Self {
            might_grace_axis: 0,
            might_grace_spectrum: 0,
            might_grace_shift: 0,
            vitality_focus_axis: 0,
            vitality_focus_spectrum: 0,
            vitality_focus_shift: 0,
            instinct_presence_axis: 0,
            instinct_presence_spectrum: 0,
            instinct_presence_shift: 0,
        }
    }
}

#[derive(Clone, Component, Copy, Default)]
pub struct Physics;

#[derive(Debug, Default, Component)]
pub struct Sun();

#[derive(Debug, Default, Component)]
pub struct Moon();

/// Tracks the last time an auto-attack was performed (ADR-009)
/// Used to enforce 1.5s cooldown between passive auto-attacks
#[derive(Clone, Component, Copy, Debug)]
pub struct LastAutoAttack {
    /// Game time when last auto-attack was performed (server time + offset)
    pub last_attack_time: std::time::Duration,
}

impl Default for LastAutoAttack {
    fn default() -> Self {
        Self {
            last_attack_time: std::time::Duration::ZERO,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_balanced_attributes() {
        // axis=-50, spectrum=25: -50A/25S (on might side)
        let base = ActorAttributes {
            might_grace_axis: -50,
            might_grace_spectrum: 25,
            ..Default::default()
        };

        let attrs = ActorAttributes { might_grace_shift: -25, ..base };
        assert_eq!(attrs.might(), 87);

        let attrs = ActorAttributes { might_grace_shift: -10, ..base };
        assert_eq!(attrs.might(), 80);

        let attrs = ActorAttributes { might_grace_shift: 0, ..base };
        assert_eq!(attrs.might(), 75);

        let attrs = ActorAttributes { might_grace_shift: 10, ..base };
        assert_eq!(attrs.might(), 70);

        let attrs = ActorAttributes { might_grace_shift: 25, ..base };
        assert_eq!(attrs.might(), 63);
    }

    #[test]
    fn test_perfectly_balanced_attributes() {
        // Perfectly balanced: axis=0, spectrum=20: 0A/20S
        let base = ActorAttributes {
            might_grace_axis: 0,
            might_grace_spectrum: 20,
            ..Default::default()
        };

        // Shift fully toward might
        let attrs = ActorAttributes { might_grace_shift: -20, ..base };
        assert_eq!(attrs.might(), 30);

        // Shift partially toward might
        let attrs = ActorAttributes { might_grace_shift: -10, ..base };
        assert_eq!(attrs.might(), 25);

        // No shift (perfectly centered)
        let attrs = ActorAttributes { might_grace_shift: 0, ..base };
        assert_eq!(attrs.might(), 20);

        // Shift partially toward grace
        let attrs = ActorAttributes { might_grace_shift: 10, ..base };
        assert_eq!(attrs.might(), 15);

        // Shift fully toward grace
        let attrs = ActorAttributes { might_grace_shift: 20, ..base };
        assert_eq!(attrs.might(), 10);
    }

    #[test]
    fn test_narrow_spectrum_attributes() {
        // axis=-80, spectrum=10: -80A/10S (on might side)
        let base = ActorAttributes {
            might_grace_axis: -80,
            might_grace_spectrum: 10,
            ..Default::default()
        };

        let attrs = ActorAttributes { might_grace_shift: -10, ..base };
        assert_eq!(attrs.might(), 95);

        let attrs = ActorAttributes { might_grace_shift: -5, ..base };
        assert_eq!(attrs.might(), 92);

        let attrs = ActorAttributes { might_grace_shift: 0, ..base };
        assert_eq!(attrs.might(), 90);

        let attrs = ActorAttributes { might_grace_shift: 5, ..base };
        assert_eq!(attrs.might(), 88);

        let attrs = ActorAttributes { might_grace_shift: 10, ..base };
        assert_eq!(attrs.might(), 85);
    }

    // ===== MOVEMENT SPEED TESTS (ADR-010 Phase 2) =====

    #[test]
    fn test_movement_speed_formula_baseline() {
        // Grace = 0 (parity) should give baseline speed (100%)
        // Formula: max(75, 100 + (grace / 2))
        // Grace = 0: max(75, 100 + 0) = 100
        // Speed multiplier: 100 / 100 = 1.0
        // Final speed: 0.005 * 1.0 = 0.005
        let attrs = ActorAttributes {
            might_grace_axis: 0,
            might_grace_spectrum: 0,
            might_grace_shift: 0,
            ..Default::default()
        };

        let speed = attrs.movement_speed();
        assert!(
            (speed - 0.005).abs() < 0.0001,
            "Grace 0 should give baseline speed 0.005, got {}",
            speed
        );
    }

    #[test]
    fn test_movement_speed_formula_grace_100() {
        // Grace = 100 (Grace specialist) should give +50% speed
        // Formula: max(75, 100 + (100 / 2)) = max(75, 150) = 150
        // Speed multiplier: 150 / 100 = 1.5
        // Final speed: 0.005 * 1.5 = 0.0075
        let attrs = ActorAttributes {
            might_grace_axis: 100,
            might_grace_spectrum: 0,
            might_grace_shift: 0,
            ..Default::default()
        };

        let speed = attrs.movement_speed();
        assert!(
            (speed - 0.0075).abs() < 0.0001,
            "Grace 100 should give 150% speed (0.0075), got {}",
            speed
        );
    }

    #[test]
    fn test_movement_speed_formula_grace_neg100() {
        // Grace = -100 (Might specialist) should be clamped at 75% speed
        // Formula: max(75, 100 + (-100 / 2)) = max(75, 50) = 75
        // Speed multiplier: 75 / 100 = 0.75
        // Final speed: 0.005 * 0.75 = 0.00375
        let attrs = ActorAttributes {
            might_grace_axis: -100,
            might_grace_spectrum: 0,
            might_grace_shift: 0,
            ..Default::default()
        };

        let speed = attrs.movement_speed();
        assert!(
            (speed - 0.00375).abs() < 0.0001,
            "Grace -100 should be clamped at 75% speed (0.00375), got {}",
            speed
        );
    }

    #[test]
    fn test_movement_speed_formula_grace_50() {
        // Grace = 50 should give +25% speed
        // Formula: max(75, 100 + (50 / 2)) = max(75, 125) = 125
        // Speed multiplier: 125 / 100 = 1.25
        // Final speed: 0.005 * 1.25 = 0.00625
        let attrs = ActorAttributes {
            might_grace_axis: 50,
            might_grace_spectrum: 0,
            might_grace_shift: 0,
            ..Default::default()
        };

        let speed = attrs.movement_speed();
        assert!(
            (speed - 0.00625).abs() < 0.0001,
            "Grace 50 should give 125% speed (0.00625), got {}",
            speed
        );
    }

    #[test]
    fn test_movement_speed_with_axis_and_shift() {
        // Test that shift affects movement speed via might_grace()
        // axis = 80, shift = 20 -> might_grace() = 100 (clamped)
        let attrs = ActorAttributes {
            might_grace_axis: 80,
            might_grace_spectrum: 30,
            might_grace_shift: 20,
            ..Default::default()
        };

        // might_grace() = clamp(80 + 20, -100, 100) = 100
        let speed = attrs.movement_speed();
        assert!(
            (speed - 0.0075).abs() < 0.0001,
            "Grace 100 (via axis+shift) should give 150% speed (0.0075), got {}",
            speed
        );
    }

    // ===== LEVEL MULTIPLIER TESTS (ADR-020) =====
    // Property tests only — no specific formula values, survives balance tuning

    #[test]
    fn test_level_multiplier_identity_at_zero() {
        // Level 0 must always return 1.0 regardless of k/p
        assert_eq!(ActorAttributes::level_multiplier(0, 0.10, 1.5), 1.0);
        assert_eq!(ActorAttributes::level_multiplier(0, 0.15, 2.0), 1.0);
        assert_eq!(ActorAttributes::level_multiplier(0, 0.10, 1.2), 1.0);
        assert_eq!(ActorAttributes::level_multiplier(0, 0.99, 5.0), 1.0);
    }

    #[test]
    fn test_level_multiplier_monotonically_increasing() {
        // Higher level must always produce higher multiplier (same k/p)
        for level in 0..20u32 {
            let lower = ActorAttributes::level_multiplier(level, 0.10, 1.5);
            let higher = ActorAttributes::level_multiplier(level + 1, 0.10, 1.5);
            assert!(
                higher > lower,
                "Multiplier must increase with level: level {} ({}) >= level {} ({})",
                level + 1, higher, level, lower
            );
        }
    }

    #[test]
    fn test_level_multiplier_super_linear_growth() {
        // The gap between consecutive levels should increase (super-linear, not linear)
        // gap(level N→N+1) < gap(level N+1→N+2) for p > 1
        let gap_low = ActorAttributes::level_multiplier(2, 0.10, 1.5)
            - ActorAttributes::level_multiplier(1, 0.10, 1.5);
        let gap_high = ActorAttributes::level_multiplier(9, 0.10, 1.5)
            - ActorAttributes::level_multiplier(8, 0.10, 1.5);
        assert!(
            gap_high > gap_low,
            "Growth rate should accelerate: gap at high levels ({}) > gap at low levels ({})",
            gap_high, gap_low
        );
    }

    #[test]
    fn test_damage_multiplier_exceeds_hp_multiplier() {
        // Damage scales more aggressively than HP at all positive levels
        for level in 1..=20u32 {
            let attrs = ActorAttributes::new(
                -(level as i8).min(127), 0, 0,  // some might investment
                0, 0, 0,
                0, 0, 0,
            );
            assert!(
                attrs.damage_level_multiplier() >= attrs.hp_level_multiplier(),
                "Damage multiplier should >= HP multiplier at level {}",
                level
            );
        }
    }

    #[test]
    fn test_hp_multiplier_exceeds_reaction_multiplier() {
        // HP scales more than reaction stats at all positive levels
        for level in 1..=20u32 {
            let attrs = ActorAttributes::new(
                -(level as i8).min(127), 0, 0,
                0, 0, 0,
                0, 0, 0,
            );
            assert!(
                attrs.hp_level_multiplier() >= attrs.reaction_level_multiplier(),
                "HP multiplier should >= reaction multiplier at level {}",
                level
            );
        }
    }

    #[test]
    fn test_max_health_increases_with_level() {
        let level_0 = ActorAttributes::default();
        let level_5 = ActorAttributes::new(-3, -2, 0, 0, 0, 0, 0, 0, 0); // 5 points invested
        let level_10 = ActorAttributes::new(-5, -3, 0, -1, -1, 0, 0, 0, 0); // 10 points invested

        assert!(
            level_5.max_health() > level_0.max_health(),
            "Level 5 should have more HP than level 0"
        );
        assert!(
            level_10.max_health() > level_5.max_health(),
            "Level 10 should have more HP than level 5"
        );
    }

    #[test]
    fn test_default_attrs_max_health_is_base() {
        // Level 0, no investment: max_health = base HP * multiplier(0) = base * 1.0
        let attrs = ActorAttributes::default();
        assert_eq!(attrs.total_level(), 0);
        assert_eq!(attrs.max_health(), 100.0, "Level 0 with no vitality should have base 100 HP");
    }

}