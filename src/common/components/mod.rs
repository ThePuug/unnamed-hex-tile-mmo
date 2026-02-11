pub mod ally_target;
pub mod behaviour;
pub mod engagement;
pub mod entity_type;
pub mod gcd;
pub mod heading;
pub mod hex_assignment;
pub mod keybits;
pub mod movement_intent_state;
pub mod movement_prediction;
pub mod npc_recovery;
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

        // must be at most 1 hex away with at most 1 z-level difference
        flat_dist <= 1 && z_diff <= 1
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

// === SCALING MODE INFRASTRUCTURE (ADR-026, RFC-020) ===
// Layer 2: Three scaling modes layered on top of existing A/S/S model.
// These are pure abstractions that take derived attribute values as input.
// - Absolute: derived_value × level_multiplier (methods already on ActorAttributes)
// - Relative: attacker_derived - defender_derived (Phase 4, not yet implemented)
// - Commitment: tier_from_percentage(derived_value / total_budget) (below)

/// Discrete commitment tier based on percentage of total attribute budget.
///
/// Thresholds: T0 (<30%), T1 (≥30%), T2 (≥45%), T3 (≥60%).
/// Budget math forces hard build choices:
/// - Specialist: T3 (60%) + T1 (30%) = 90% → viable
/// - Dual identity: T2 (45%) + T2 (45%) = 90% → viable
/// - Generalist: T1 (30%) × 3 = 90% → viable
/// - T3 + T2 = 105% → impossible
///
/// See ADR-027 for design rationale.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CommitmentTier {
    /// No commitment identity — baseline only
    T0,
    /// Identity unlocked — noticeable specialization (≥30% of budget)
    T1,
    /// Identity deepened — significant commitment (≥45% of budget)
    T2,
    /// Identity defining — dominant aspect of build (≥60% of budget)
    T3,
}

impl CommitmentTier {
    /// Calculate commitment tier from a derived attribute value and total budget.
    ///
    /// This is a pure function — it does not know which attribute produced the value
    /// or how it was derived from A/S/S. It only cares about the percentage.
    pub fn calculate(derived_value: u16, total_budget: u32) -> Self {
        if total_budget == 0 {
            return Self::T0;
        }
        let pct = (derived_value as f64 / total_budget as f64) * 100.0;
        if pct >= 60.0 {
            Self::T3
        } else if pct >= 40.0 {
            Self::T2
        } else if pct >= 20.0 {
            Self::T1
        } else {
            Self::T0
        }
    }
}

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
        // Pure spectrum (axis=0) cannot shift - requires axis commitment
        if self.might_grace_axis == 0 {
            self.might_grace_shift = 0;
            return;
        }

        let max_shift = self.might_grace_spectrum.max(0);
        // Shift constrained by axis direction:
        // axis > 0 (grace/right) → shift can only go negative (toward might/left)
        // axis < 0 (might/left) → shift can only go positive (toward grace/right)
        let clamped = if self.might_grace_axis > 0 {
            shift.clamp(-max_shift, 0)
        } else {
            shift.clamp(0, max_shift)
        };
        self.might_grace_shift = clamped;
    }

    pub fn set_vitality_focus_shift(&mut self, shift: i8) {
        // Pure spectrum (axis=0) cannot shift - requires axis commitment
        if self.vitality_focus_axis == 0 {
            self.vitality_focus_shift = 0;
            return;
        }

        let max_shift = self.vitality_focus_spectrum.max(0);
        // axis > 0 (focus/right) → shift can only go negative (toward vitality/left)
        // axis < 0 (vitality/left) → shift can only go positive (toward focus/right)
        let clamped = if self.vitality_focus_axis > 0 {
            shift.clamp(-max_shift, 0)
        } else {
            shift.clamp(0, max_shift)
        };
        self.vitality_focus_shift = clamped;
    }

    pub fn set_instinct_presence_shift(&mut self, shift: i8) {
        // Pure spectrum (axis=0) cannot shift - requires axis commitment
        if self.instinct_presence_axis == 0 {
            self.instinct_presence_shift = 0;
            return;
        }

        let max_shift = self.instinct_presence_spectrum.max(0);
        // axis > 0 (presence/right) → shift can only go negative (toward instinct/left)
        // axis < 0 (instinct/left) → shift can only go positive (toward presence/right)
        let clamped = if self.instinct_presence_axis > 0 {
            shift.clamp(-max_shift, 0)
        } else {
            shift.clamp(0, max_shift)
        };
        self.instinct_presence_shift = clamped;
    }

    // === Private: Get current scaled position (for derived stats like movement speed) ===
    // These calculate the NET position (might vs grace) factoring in both axis and shift
    // Positive = grace side, Negative = might side
    // Returns scaled value based on investment

    fn might_grace_position(&self) -> i16 {
        let axis_scaled = (self.might_grace_axis as i16) * 6;
        let shift_scaled = (self.might_grace_shift as i16) * 12;
        axis_scaled + shift_scaled
    }

    // === MIGHT ↔ GRACE ===

    /// Maximum Might reach (including maximum shift)
    /// When axis on might side: axis×16 + spectrum×12 (at shift=0)
    /// When axis on grace side: spectrum×12 (max shift gives -shift×12)
    pub fn might_reach(&self) -> u16 {
        let spectrum_reach = (self.might_grace_spectrum.max(0) as u16) * 12;

        if self.might_grace_axis <= 0 {
            // Axis on might side: already at max with shift=0
            let axis_reach = (self.might_grace_axis.unsigned_abs() as u16) * 16;
            axis_reach + spectrum_reach
        } else {
            // Axis on grace side: can shift spectrum to might
            spectrum_reach
        }
    }

    /// Maximum Grace reach (including maximum shift)
    /// When axis on grace side: axis×16 + spectrum×12 (at shift=0)
    /// When axis on might side: spectrum×12 (max shift gives shift×12)
    pub fn grace_reach(&self) -> u16 {
        let spectrum_reach = (self.might_grace_spectrum.max(0) as u16) * 12;

        if self.might_grace_axis >= 0 {
            // Axis on grace side: already at max with shift=0
            let axis_reach = (self.might_grace_axis.unsigned_abs() as u16) * 16;
            axis_reach + spectrum_reach
        } else {
            // Axis on might side: can shift spectrum to grace
            spectrum_reach
        }
    }

    /// Current available Might (scaled)
    /// When axis < 0 (might): axis×10 + spectrum×6 - shift×6
    /// When axis > 0 (grace): -shift×6 only
    /// When axis = 0: spectrum×6
    pub fn might(&self) -> u16 {
        let spectrum_base = (self.might_grace_spectrum.max(0) as i16) * 12;
        let shift_scaled = (self.might_grace_shift as i16) * 12;

        if self.might_grace_axis < 0 {
            // Axis on might side: axis + spectrum - shift
            let axis_reach = (self.might_grace_axis.unsigned_abs() as i16) * 16;
            (axis_reach + spectrum_base - shift_scaled).max(0) as u16
        } else if self.might_grace_axis > 0 {
            // Axis on grace side: opposite side gets -shift only
            (-shift_scaled).max(0) as u16
        } else {
            // Balanced (axis=0): spectrum with reduced multiplier (×7)
            ((self.might_grace_spectrum.max(0) as i16) * 6).max(0) as u16
        }
    }

    /// Current available Grace (scaled)
    /// When axis > 0 (grace): axis×15 + spectrum×10 + shift×10
    /// When axis < 0 (might): shift×10 only
    /// When axis = 0: spectrum×7 (balanced, reduced multiplier)
    pub fn grace(&self) -> u16 {
        let spectrum_base = (self.might_grace_spectrum.max(0) as i16) * 12;
        let shift_scaled = (self.might_grace_shift as i16) * 12;

        if self.might_grace_axis > 0 {
            // Axis on grace side: axis + spectrum + shift
            let axis_reach = (self.might_grace_axis as i16) * 16;
            (axis_reach + spectrum_base + shift_scaled).max(0) as u16
        } else if self.might_grace_axis < 0 {
            // Axis on might side: opposite side gets shift only
            shift_scaled.max(0) as u16
        } else {
            // Balanced (axis=0): spectrum with reduced multiplier (×7)
            ((self.might_grace_spectrum.max(0) as i16) * 6).max(0) as u16
        }
    }

    // === VITALITY ↔ FOCUS ===

    /// Maximum Vitality reach (including maximum shift)
    /// When axis on vitality side: axis*10 + spectrum*7 (at shift=0)
    /// When axis on focus side: spectrum*7*2 (shift all spectrum to vitality)
    pub fn vitality_reach(&self) -> u16 {
        let spectrum_reach = (self.vitality_focus_spectrum.max(0) as u16) * 12;

        if self.vitality_focus_axis <= 0 {
            // Axis on vitality side: already at max with shift=0
            let axis_reach = (self.vitality_focus_axis.unsigned_abs() as u16) * 16;
            axis_reach + spectrum_reach
        } else {
            // Axis on focus side: can shift spectrum to vitality
            spectrum_reach
        }
    }

    /// Maximum Focus reach (including maximum shift)
    /// When axis on focus side: axis×16 + spectrum×12 (at shift=0)
    /// When axis on vitality side: spectrum×12 (max shift toward focus)
    pub fn focus_reach(&self) -> u16 {
        let spectrum_reach = (self.vitality_focus_spectrum.max(0) as u16) * 12;

        if self.vitality_focus_axis >= 0 {
            // Axis on focus side: already at max with shift=0
            let axis_reach = (self.vitality_focus_axis.unsigned_abs() as u16) * 16;
            axis_reach + spectrum_reach
        } else {
            // Axis on vitality side: can shift spectrum to focus
            spectrum_reach
        }
    }

    /// Current available Vitality (scaled)
    /// When axis < 0 (vitality): axis×10 + spectrum×6 - shift×6
    /// When axis > 0 (focus): -shift×6 only
    /// When axis = 0: spectrum×6
    pub fn vitality(&self) -> u16 {
        let spectrum_base = (self.vitality_focus_spectrum.max(0) as i16) * 12;
        let shift_scaled = (self.vitality_focus_shift as i16) * 12;

        if self.vitality_focus_axis < 0 {
            // Axis on vitality side: axis + spectrum - shift
            let axis_reach = (self.vitality_focus_axis.unsigned_abs() as i16) * 16;
            (axis_reach + spectrum_base - shift_scaled).max(0) as u16
        } else if self.vitality_focus_axis > 0 {
            // Axis on focus side: opposite side gets -shift only
            (-shift_scaled).max(0) as u16
        } else {
            // Balanced (axis=0): spectrum with reduced multiplier (×7)
            ((self.vitality_focus_spectrum.max(0) as i16) * 6).max(0) as u16
        }
    }

    /// Current available Focus (scaled)
    /// When axis > 0 (focus): axis×15 + spectrum×10 + shift×10
    /// When axis < 0 (vitality): shift×10 only
    /// When axis = 0: spectrum×7 (balanced, reduced multiplier)
    pub fn focus(&self) -> u16 {
        let spectrum_base = (self.vitality_focus_spectrum.max(0) as i16) * 12;
        let shift_scaled = (self.vitality_focus_shift as i16) * 12;

        if self.vitality_focus_axis > 0 {
            // Axis on focus side: axis + spectrum + shift
            let axis_reach = (self.vitality_focus_axis as i16) * 16;
            (axis_reach + spectrum_base + shift_scaled).max(0) as u16
        } else if self.vitality_focus_axis < 0 {
            // Axis on vitality side: opposite side gets shift only
            shift_scaled.max(0) as u16
        } else {
            // Balanced (axis=0): spectrum with reduced multiplier (×7)
            ((self.vitality_focus_spectrum.max(0) as i16) * 6).max(0) as u16
        }
    }

    // === INSTINCT ↔ PRESENCE ===

    /// Maximum Instinct reach (including maximum shift)
    /// When axis on instinct side: axis*10 + spectrum*7 (at shift=0)
    /// When axis on presence side: spectrum*7*2 (shift all spectrum to instinct)
    pub fn instinct_reach(&self) -> u16 {
        let spectrum_reach = (self.instinct_presence_spectrum.max(0) as u16) * 12;

        if self.instinct_presence_axis <= 0 {
            // Axis on instinct side: already at max with shift=0
            let axis_reach = (self.instinct_presence_axis.unsigned_abs() as u16) * 16;
            axis_reach + spectrum_reach
        } else {
            // Axis on presence side: can shift spectrum to instinct
            spectrum_reach
        }
    }

    /// Maximum Presence reach (including maximum shift)
    /// When axis on presence side: axis×16 + spectrum×12 (at shift=0)
    /// When axis on instinct side: spectrum×12 (max shift toward presence)
    pub fn presence_reach(&self) -> u16 {
        let spectrum_reach = (self.instinct_presence_spectrum.max(0) as u16) * 12;

        if self.instinct_presence_axis >= 0 {
            // Axis on presence side: already at max with shift=0
            let axis_reach = (self.instinct_presence_axis.unsigned_abs() as u16) * 16;
            axis_reach + spectrum_reach
        } else {
            // Axis on instinct side: can shift spectrum to presence
            spectrum_reach
        }
    }

    /// Current available Instinct (scaled)
    /// When axis < 0 (instinct): axis×10 + spectrum×6 - shift×6
    /// When axis > 0 (presence): -shift×6 only
    /// When axis = 0: spectrum×6
    pub fn instinct(&self) -> u16 {
        let spectrum_base = (self.instinct_presence_spectrum.max(0) as i16) * 12;
        let shift_scaled = (self.instinct_presence_shift as i16) * 12;

        if self.instinct_presence_axis < 0 {
            // Axis on instinct side: axis + spectrum - shift
            let axis_reach = (self.instinct_presence_axis.unsigned_abs() as i16) * 16;
            (axis_reach + spectrum_base - shift_scaled).max(0) as u16
        } else if self.instinct_presence_axis > 0 {
            // Axis on presence side: opposite side gets -shift only
            (-shift_scaled).max(0) as u16
        } else {
            // Balanced (axis=0): spectrum with reduced multiplier (×7)
            ((self.instinct_presence_spectrum.max(0) as i16) * 6).max(0) as u16
        }
    }

    /// Current available Presence (scaled)
    /// When axis > 0 (presence): axis×15 + spectrum×10 + shift×10
    /// When axis < 0 (instinct): shift×10 only
    /// When axis = 0: spectrum×7 (balanced, reduced multiplier)
    pub fn presence(&self) -> u16 {
        let spectrum_base = (self.instinct_presence_spectrum.max(0) as i16) * 12;
        let shift_scaled = (self.instinct_presence_shift as i16) * 12;

        if self.instinct_presence_axis > 0 {
            // Axis on presence side: axis + spectrum + shift
            let axis_reach = (self.instinct_presence_axis as i16) * 16;
            (axis_reach + spectrum_base + shift_scaled).max(0) as u16
        } else if self.instinct_presence_axis < 0 {
            // Axis on instinct side: opposite side gets shift only
            shift_scaled.max(0) as u16
        } else {
            // Balanced (axis=0): spectrum with reduced multiplier (×7)
            ((self.instinct_presence_spectrum.max(0) as i16) * 6).max(0) as u16
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
    /// Moderate scaling: balanced with HP growth to preserve level advantage without exponential runaway
    pub fn damage_level_multiplier(&self) -> f32 {
        const K: f32 = 0.15;
        const P: f32 = 1.5;
        Self::level_multiplier(self.total_level(), K, P)
    }

    /// Reaction stat level multiplier
    /// Gentle scaling: bounded by human reaction limits
    pub fn reaction_level_multiplier(&self) -> f32 {
        const K: f32 = 0.10;
        const P: f32 = 1.2;
        Self::level_multiplier(self.total_level(), K, P)
    }

    // === GAME STATS (Layer 3 - continued) ===

    /// Movement speed - currently flat until allocated to a meta-attribute
    /// TODO: Determine which meta-attribute should govern movement speed
    pub fn movement_speed(&self) -> f32 {
        0.005  // Flat speed for all entities
    }

    /// Maximum health from Constitution meta-attribute
    /// Constitution scales vitality with level multiplier for progression
    pub fn max_health(&self) -> f32 {
        self.constitution()
    }

    // === LAYER 2: SCALING MODE HELPERS ===
    //
    // The attribute system has three layers:
    //
    // **Layer 1 — Bipolar Input (Axis/Spectrum/Shift):**
    //   9 i8 fields storing raw investment counts per pair
    //
    // **Layer 2 — Derived Attribute Values:**
    //   Six pure values from A/S/S scaling: might(), grace(), vitality(),
    //   focus(), instinct(), presence()
    //
    // **Layer 3 — Three Scaling Modes:**
    //   - ABSOLUTE (progression): max_health(), movement_speed() — scales with level
    //   - RELATIVE (build matchup): contest_modifier() in damage.rs — no level scaling
    //   - COMMITMENT (build identity): queue_capacity(), cadence_interval(),
    //     evasion_chance() — discrete tiers based on % of total budget
    //
    // See docs/00-spec/attribute-system.md for full design.

    /// Total attribute budget: sum of all six derived attribute values.
    ///
    /// This is the denominator for commitment tier percentage calculations.
    /// Unlike total_level() which counts invested points (axis + spectrum),
    /// this sums the actual derived values after A/S/S scaling.
    pub fn total_budget(&self) -> u32 {
        self.might() as u32
            + self.grace() as u32
            + self.vitality() as u32
            + self.focus() as u32
            + self.instinct() as u32
            + self.presence() as u32
    }

    /// Calculate the commitment tier for a specific derived attribute value.
    ///
    /// Compares derived_value against the maximum possible for any single attribute
    /// given total investment (total_level × 10). This ensures spectrum builds aren't
    /// penalized compared to axis builds with the same point investment.
    ///
    /// Example: `attrs.commitment_tier_for(attrs.focus())` → Focus commitment tier
    pub fn commitment_tier_for(&self, derived_value: u16) -> CommitmentTier {
        let max_possible = self.total_level() as u32 * 6;
        CommitmentTier::calculate(derived_value, max_possible)
    }

    // === META-ATTRIBUTES (Layer 2) ===
    //
    // Meta-attributes are derived from Layer 1 attributes and serve as inputs to Layer 3 game stats.
    // Three types: Absolute (scaled by level), Relative (used in contests), Commitment (tier-based).
    //
    // Gear/weapons/buffs will eventually modify these meta-attributes directly.

    // --- ABSOLUTE META-ATTRIBUTES (scaled by level multiplier) ---

    /// Force: Offensive power from might (absolute meta-attribute)
    /// Fully scaled damage output including level progression. Used as base damage for abilities.
    pub fn force(&self) -> f32 {
        let might = self.might() as f32;
        let base = 10.0;
        let linear = base + (might * 0.3);
        linear * self.damage_level_multiplier()
    }

    /// Constitution: Defensive capacity from vitality
    /// Scales with level for progression. Used to calculate max health.
    pub fn constitution(&self) -> f32 {
        let vitality = self.vitality() as f32;
        let base = 100.0;
        let linear = base + (vitality * 3.8);
        linear * self.hp_level_multiplier()
    }

    // --- RELATIVE META-ATTRIBUTES (raw values for contests) ---

    /// Precision: Accuracy and critical hit potential from grace
    /// Used in contest vs Toughness (affects crit chance and mitigation)
    pub fn precision(&self) -> u16 { self.grace() }

    /// Toughness: Physical damage resistance from vitality
    /// Used in contest vs Precision (affects mitigation and crit resistance)
    pub fn toughness(&self) -> u16 { self.vitality() }

    /// Impact: Raw damage output from might
    /// Used in contest vs Composure (affects recovery pushback)
    pub fn impact(&self) -> u16 { self.might() }

    /// Composure: Mental fortitude from focus
    /// Used in contest vs Impact (reduces recovery pushback)
    pub fn composure(&self) -> u16 { self.focus() }

    /// Dominance: Tempo control from presence
    /// Used in contest vs Cunning (affects reaction window and recovery pushback)
    pub fn dominance(&self) -> u16 { self.presence() }

    /// Cunning: Reactive capability from instinct
    /// Used in contest vs Dominance (improves reaction window)
    pub fn cunning(&self) -> u16 { self.instinct() }

    // --- COMMITMENT META-ATTRIBUTES (tier-based) ---

    /// Poise: Evasion capability from grace commitment
    /// Returns commitment tier (T0-T3) based on grace as % of total budget
    pub fn poise(&self) -> CommitmentTier {
        self.commitment_tier_for(self.grace())
    }

    /// Concentration: Mental focus capacity from focus commitment
    /// Returns commitment tier (T0-T3) based on focus as % of total budget
    pub fn concentration(&self) -> CommitmentTier {
        self.commitment_tier_for(self.focus())
    }

    /// Intensity: Attack tempo from presence commitment
    /// Returns commitment tier (T0-T3) based on presence as % of total budget
    pub fn intensity(&self) -> CommitmentTier {
        self.commitment_tier_for(self.presence())
    }

    // === GAME STATS (Layer 3) ===
    // These use meta-attributes from Layer 2

    /// Reaction queue capacity from Concentration meta-attribute
    /// Higher Concentration tier → more queue slots for reactive play
    /// T0 → 1 slot, T1 → 2 slots, T2 → 3 slots, T3 → 4 slots
    pub fn queue_capacity(&self) -> usize {
        match self.concentration() {
            CommitmentTier::T0 => 1,
            CommitmentTier::T1 => 2,
            CommitmentTier::T2 => 3,
            CommitmentTier::T3 => 4,
        }
    }

    /// Auto-attack interval from Intensity meta-attribute
    /// Higher Intensity tier → faster attacks (shorter interval)
    /// T0 → 3000ms, T1 → 2500ms, T2 → 2000ms, T3 → 1500ms
    pub fn cadence_interval(&self) -> std::time::Duration {
        match self.intensity() {
            CommitmentTier::T0 => std::time::Duration::from_millis(3000),
            CommitmentTier::T1 => std::time::Duration::from_millis(2500),
            CommitmentTier::T2 => std::time::Duration::from_millis(2000),
            CommitmentTier::T3 => std::time::Duration::from_millis(1500),
        }
    }

    /// Evasion (dodge) chance from Poise meta-attribute
    /// Higher Poise tier → higher chance to completely evade incoming threats
    /// T0 → 0%, T1 → 10%, T2 → 20%, T3 → 30%
    pub fn evasion_chance(&self) -> f32 {
        match self.poise() {
            CommitmentTier::T0 => 0.0,
            CommitmentTier::T1 => 0.10,
            CommitmentTier::T2 => 0.20,
            CommitmentTier::T3 => 0.30,
        }
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

    // ===== MOVEMENT SPEED TESTS =====
    // TODO: Re-enable when movement speed is allocated to a meta-attribute

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

    // ===== COMMITMENT TIER TESTS (ADR-027, Layer 2) =====

    #[test]
    fn test_commitment_tier_thresholds() {
        // T0: below 30%
        assert_eq!(CommitmentTier::calculate(29, 100), CommitmentTier::T0);
        assert_eq!(CommitmentTier::calculate(0, 100), CommitmentTier::T0);

        // T1: exactly 30% and above
        assert_eq!(CommitmentTier::calculate(30, 100), CommitmentTier::T1);
        assert_eq!(CommitmentTier::calculate(44, 100), CommitmentTier::T1);

        // T2: exactly 45% and above
        assert_eq!(CommitmentTier::calculate(45, 100), CommitmentTier::T2);
        assert_eq!(CommitmentTier::calculate(59, 100), CommitmentTier::T2);

        // T3: above 60% (not including 60%)
        assert_eq!(CommitmentTier::calculate(60, 100), CommitmentTier::T2); // Exactly 60% is T2
        assert_eq!(CommitmentTier::calculate(61, 100), CommitmentTier::T3); // 61% is T3
        assert_eq!(CommitmentTier::calculate(100, 100), CommitmentTier::T3);
    }

    #[test]
    fn test_commitment_tier_zero_budget() {
        // Zero total budget always returns T0
        assert_eq!(CommitmentTier::calculate(0, 0), CommitmentTier::T0);
        assert_eq!(CommitmentTier::calculate(50, 0), CommitmentTier::T0);
    }

    #[test]
    fn test_commitment_tier_ordering() {
        // Tiers are ordered T0 < T1 < T2 < T3
        assert!(CommitmentTier::T0 < CommitmentTier::T1);
        assert!(CommitmentTier::T1 < CommitmentTier::T2);
        assert!(CommitmentTier::T2 < CommitmentTier::T3);
    }

    #[test]
    fn test_commitment_tier_non_round_budget() {
        // Verify with non-round total budget values
        // 30 out of 73 = 41.1% → T1
        assert_eq!(CommitmentTier::calculate(30, 73), CommitmentTier::T1);
        // 33 out of 73 = 45.2% → T2
        assert_eq!(CommitmentTier::calculate(33, 73), CommitmentTier::T2);
        // 44 out of 73 = 60.3% → T3
        assert_eq!(CommitmentTier::calculate(44, 73), CommitmentTier::T3);
    }

    // ===== TOTAL BUDGET TESTS (Layer 2) =====

    #[test]
    fn test_total_budget_default() {
        let attrs = ActorAttributes::default();
        assert_eq!(attrs.total_budget(), 0, "Default attrs should have zero budget");
    }

    #[test]
    fn test_total_budget_sums_all_derived_values() {
        // axis=-3, spectrum=2 on M/G pair: might side
        // might = |axis|*10 + spectrum*7 - shift*7 = 30 + 14 - 0 = 44
        // grace = spectrum*7 + shift*7 = 14 + 0 = 14
        // (Other pairs at default = 0)
        let attrs = ActorAttributes::new(-3, 2, 0, 0, 0, 0, 0, 0, 0);
        let expected = attrs.might() as u32 + attrs.grace() as u32
            + attrs.vitality() as u32 + attrs.focus() as u32
            + attrs.instinct() as u32 + attrs.presence() as u32;
        assert_eq!(attrs.total_budget(), expected);
        assert!(attrs.total_budget() > 0, "Should have non-zero budget with investment");
    }

    #[test]
    fn test_total_budget_differs_from_total_level() {
        // total_level counts invested points; total_budget sums derived values
        let attrs = ActorAttributes::new(-3, 2, 0, 0, 0, 0, 0, 0, 0);
        // total_level = |axis| + spectrum = 3 + 2 = 5
        assert_eq!(attrs.total_level(), 5);
        // With axis=-3 (might), spectrum=2, shift=0:
        // might = 3*10 + 2*6 - 0 = 42, grace = 0 (opposite side, no shift)
        // total_budget = 42 + 0 + 0+0+0+0 = 42
        assert_eq!(attrs.total_budget(), 42);
        assert_ne!(attrs.total_level(), attrs.total_budget() as u32);
    }

    // ===== COMMITMENT_TIER_FOR TESTS (Layer 2) =====

    #[test]
    fn test_commitment_tier_for_convenience() {
        // Specialist build: heavy investment in one attribute
        // axis=-5, spectrum=0 → might=50, grace=0, total_budget=50
        // might commitment: 50/50 = 100% → T3
        let attrs = ActorAttributes::new(-5, 0, 0, 0, 0, 0, 0, 0, 0);
        assert_eq!(attrs.commitment_tier_for(attrs.might()), CommitmentTier::T3);
        assert_eq!(attrs.commitment_tier_for(attrs.grace()), CommitmentTier::T0);
    }

    #[test]
    fn test_commitment_tier_for_balanced_build() {
        // Spread across pairs: each pair gets some investment
        // M/G: axis=0, spectrum=3 → might=21, grace=21
        // V/F: axis=0, spectrum=3 → vitality=21, focus=21
        // I/P: axis=0, spectrum=3 → instinct=21, presence=21
        // total_budget = 126, each attr = 21/126 = 16.7% → all T0
        let attrs = ActorAttributes::new(0, 3, 0, 0, 3, 0, 0, 3, 0);
        assert_eq!(attrs.commitment_tier_for(attrs.might()), CommitmentTier::T0);
        assert_eq!(attrs.commitment_tier_for(attrs.grace()), CommitmentTier::T0);
        assert_eq!(attrs.commitment_tier_for(attrs.vitality()), CommitmentTier::T0);
        assert_eq!(attrs.commitment_tier_for(attrs.focus()), CommitmentTier::T0);
    }

    #[test]
    fn test_commitment_tier_budget_constraints() {
        // Verify that T3+T1 is achievable (60%+30%=90%)
        // Need: one attr at ≥60%, another at ≥30%
        // axis=-6, spectrum=0 → might=60, grace=0
        // axis=0, spectrum=0, axis=-3, spectrum=0 → vitality=30
        // total_budget = 60 + 0 + 30 + 0 + 0 + 0 = 90
        // might: 60/90 = 66.7% → T3 ✓
        // vitality: 30/90 = 33.3% → T1 ✓
        let attrs = ActorAttributes::new(-6, 0, 0, -3, 0, 0, 0, 0, 0);
        assert_eq!(attrs.commitment_tier_for(attrs.might()), CommitmentTier::T3);
        assert_eq!(attrs.commitment_tier_for(attrs.vitality()), CommitmentTier::T1);
    }

    #[test]
    fn test_commitment_tier_dual_t2() {
        // Verify dual T2 is achievable (45%+45%=90%)
        // might=45, vitality=45, total_budget=90
        // Both at 50% → T2
        // axis=-4, spectrum=0 → might=40... not quite
        // We need derived values that work out. Let's use:
        // M/G: axis=-4, spectrum=1 → might=47, grace=7
        // V/F: axis=-4, spectrum=1 → vitality=47, focus=7
        // total_budget = 47+7+47+7+0+0 = 108
        // might: 47/108 = 43.5% → T1 (just under T2)
        // Let's try axis=-5, spectrum=0:
        // might=50, grace=0, vitality=50, focus=0 → total=100
        // might: 50/100 = 50% → T2 ✓, vitality: 50/100 = 50% → T2 ✓
        let attrs = ActorAttributes::new(-5, 0, 0, -5, 0, 0, 0, 0, 0);
        assert_eq!(attrs.commitment_tier_for(attrs.might()), CommitmentTier::T2);
        assert_eq!(attrs.commitment_tier_for(attrs.vitality()), CommitmentTier::T2);
    }

    // ===== SHIFT CONSTRAINT TESTS =====
    // Shift is constrained by axis direction: can only shift toward the side WITHOUT axis

    #[test]
    fn test_shift_constrained_when_axis_on_right_might_grace() {
        // axis=5 (grace/right side), spectrum=5
        // shift can only go negative (toward might/left): [-5, 0]
        let mut attrs = ActorAttributes::new(5, 5, 0, 0, 0, 0, 0, 0, 0);

        // Try to set positive shift (should clamp to 0)
        attrs.set_might_grace_shift(3);
        assert_eq!(attrs.might_grace_shift(), 0, "Positive shift should clamp to 0 when axis on right");

        // Set negative shift (should work)
        attrs.set_might_grace_shift(-3);
        assert_eq!(attrs.might_grace_shift(), -3, "Negative shift should work when axis on right");

        // Try to exceed max negative shift (should clamp to -spectrum)
        attrs.set_might_grace_shift(-10);
        assert_eq!(attrs.might_grace_shift(), -5, "Shift should clamp to -spectrum");
    }

    #[test]
    fn test_shift_constrained_when_axis_on_left_might_grace() {
        // axis=-5 (might/left side), spectrum=5
        // shift can only go positive (toward grace/right): [0, +5]
        let mut attrs = ActorAttributes::new(-5, 5, 0, 0, 0, 0, 0, 0, 0);

        // Try to set negative shift (should clamp to 0)
        attrs.set_might_grace_shift(-3);
        assert_eq!(attrs.might_grace_shift(), 0, "Negative shift should clamp to 0 when axis on left");

        // Set positive shift (should work)
        attrs.set_might_grace_shift(3);
        assert_eq!(attrs.might_grace_shift(), 3, "Positive shift should work when axis on left");

        // Try to exceed max positive shift (should clamp to +spectrum)
        attrs.set_might_grace_shift(10);
        assert_eq!(attrs.might_grace_shift(), 5, "Shift should clamp to +spectrum");
    }

    #[test]
    fn test_shift_constrained_vitality_focus() {
        // Test same constraints for vitality/focus pair
        let mut attrs = ActorAttributes::new(0, 0, 0, 3, 4, 0, 0, 0, 0);

        // axis=3 (focus/right), shift can only go negative
        attrs.set_vitality_focus_shift(2);
        assert_eq!(attrs.vitality_focus_shift(), 0, "Positive shift should clamp when axis on right");

        attrs.set_vitality_focus_shift(-2);
        assert_eq!(attrs.vitality_focus_shift(), -2, "Negative shift should work when axis on right");
    }

    #[test]
    fn test_shift_constrained_instinct_presence() {
        // Test same constraints for instinct/presence pair
        let mut attrs = ActorAttributes::new(0, 0, 0, 0, 0, 0, -4, 3, 0);

        // axis=-4 (instinct/left), shift can only go positive
        attrs.set_instinct_presence_shift(-2);
        assert_eq!(attrs.instinct_presence_shift(), 0, "Negative shift should clamp when axis on left");

        attrs.set_instinct_presence_shift(2);
        assert_eq!(attrs.instinct_presence_shift(), 2, "Positive shift should work when axis on left");
    }

    #[test]
    fn test_shift_zero_allowed_regardless_of_axis() {
        // Shift=0 should always be valid regardless of axis direction
        let mut attrs = ActorAttributes::new(5, 5, 0, -3, 4, 0, 0, 6, 0);

        attrs.set_might_grace_shift(0);
        assert_eq!(attrs.might_grace_shift(), 0);

        attrs.set_vitality_focus_shift(0);
        assert_eq!(attrs.vitality_focus_shift(), 0);

        attrs.set_instinct_presence_shift(0);
        assert_eq!(attrs.instinct_presence_shift(), 0);
    }

    #[test]
    #[ignore] // Manual exploration test - run with --ignored to see output
    fn explore_build_variations() {
        println!("\n=== BUILD VARIATIONS ===\n");

        // 3-point builds (early game)
        println!("--- 3-POINT BUILDS (Early Game) ---");
        println!("Max possible = 30");
        let early = [
            ("3 might axis", -3, 0, 0, 0, 0, 0, 0, 0, 0),
            ("3 spectrum", 0, 3, 0, 0, 0, 0, 0, 0, 0),
            ("2 axis + 1 spec", -2, 1, 0, 0, 0, 0, 0, 0, 0),
        ];
        for (desc, a1, a2, a3, a4, a5, a6, a7, a8, a9) in early {
            let attrs = ActorAttributes::new(a1, a2, a3, a4, a5, a6, a7, a8, a9);
            println!("{}: M={} ({:?}), G={} ({:?}), total={}",
                desc, attrs.might(), attrs.commitment_tier_for(attrs.might()),
                attrs.grace(), attrs.commitment_tier_for(attrs.grace()),
                attrs.might() + attrs.grace());
        }

        // 10-point single-pair builds
        println!("\n--- 10-POINT SINGLE-PAIR BUILDS ---");
        println!("Max possible = 100, T1≥30, T2≥45, T3>60\n");

        let single = [
            ("10 axis (specialist)", -10, 0, 0, 0, 0, 0, 0, 0, 0),
            ("10 spectrum (flexible)", 0, 10, 0, 0, 0, 0, 0, 0, 0),
            ("7 axis + 3 spec", -7, 3, 0, 0, 0, 0, 0, 0, 0),
            ("5 axis + 5 spec", -5, 5, 0, 0, 0, 0, 0, 0, 0),
            ("3 axis + 7 spec", -3, 7, 0, 0, 0, 0, 0, 0, 0),
        ];
        for (desc, a1, a2, a3, a4, a5, a6, a7, a8, a9) in single {
            let attrs = ActorAttributes::new(a1, a2, a3, a4, a5, a6, a7, a8, a9);
            println!("{}: M={} ({:?}), G={} ({:?}), total={}",
                desc, attrs.might(), attrs.commitment_tier_for(attrs.might()),
                attrs.grace(), attrs.commitment_tier_for(attrs.grace()),
                attrs.might() + attrs.grace());
        }

        // Cross-pair builds
        println!("\n--- 10-POINT CROSS-PAIR BUILDS ---");
        let cross = [
            ("M/G 3 axis + I/P 6 axis + 1 spec", 3, 0, 0, 0, 0, 0, 6, 1, 0),
            ("M/G 5 axis + V/F 5 axis", -5, 0, 0, -5, 0, 0, 0, 0, 0),
            ("M/G 3/3 + V/F 4", -3, 3, 0, -4, 0, 0, 0, 0, 0),
            ("All spectrum (3/3/4)", 0, 3, 0, 0, 3, 0, 0, 4, 0),
        ];
        for (desc, a1, a2, a3, a4, a5, a6, a7, a8, a9) in cross {
            let attrs = ActorAttributes::new(a1, a2, a3, a4, a5, a6, a7, a8, a9);
            println!("{}", desc);
            println!("  M={} ({:?}), G={} ({:?})", attrs.might(), attrs.commitment_tier_for(attrs.might()),
                attrs.grace(), attrs.commitment_tier_for(attrs.grace()));
            println!("  V={} ({:?}), F={} ({:?})", attrs.vitality(), attrs.commitment_tier_for(attrs.vitality()),
                attrs.focus(), attrs.commitment_tier_for(attrs.focus()));
            println!("  I={} ({:?}), P={} ({:?})", attrs.instinct(), attrs.commitment_tier_for(attrs.instinct()),
                attrs.presence(), attrs.commitment_tier_for(attrs.presence()));
            println!("  Total: {}\n", attrs.might() + attrs.grace() + attrs.vitality() +
                attrs.focus() + attrs.instinct() + attrs.presence());
        }

        // Strategic build examples
        println!("\n--- STRATEGIC BUILD EXAMPLES (Level 10) ---\n");

        println!("SINGLE-PAIR (10 points in one pair):");
        let examples = [
            ("Glass Cannon", -10, 0, 0, 0, 0, 0, 0, 0, 0, "Max damage, no defense"),
            ("Tank", 0, 0, 0, -10, 0, 0, 0, 0, 0, "Max health, low damage"),
            ("Balanced DPS", 0, 10, 0, 0, 0, 0, 0, 0, 0, "Dual T2 might/grace"),
        ];
        for (name, a1, a2, a3, a4, a5, a6, a7, a8, a9, note) in examples {
            let a = ActorAttributes::new(a1, a2, a3, a4, a5, a6, a7, a8, a9);
            println!("  {}: {}", name, note);
            println!("    M={} ({:?}), G={} ({:?}), V={} ({:?}), F={} ({:?}), I={} ({:?}), P={} ({:?})",
                a.might(), a.commitment_tier_for(a.might()),
                a.grace(), a.commitment_tier_for(a.grace()),
                a.vitality(), a.commitment_tier_for(a.vitality()),
                a.focus(), a.commitment_tier_for(a.focus()),
                a.instinct(), a.commitment_tier_for(a.instinct()),
                a.presence(), a.commitment_tier_for(a.presence()));
        }

        println!("\nDUAL-PAIR (10 points across two pairs):");
        let dual = [
            ("Bruiser", -5, 0, 0, -5, 0, 0, 0, 0, 0, "T2 might + T2 vitality"),
            ("Duelist", -6, 0, 0, 0, 0, 0, -4, 0, 0, "T3 might + T1 instinct (crit)"),
            ("Presence Tank", 0, 0, 0, -7, 0, 0, 3, 0, 0, "T3 vitality + T1 presence"),
            ("Flex Fighter", -3, 3, 0, -4, 0, 0, 0, 0, 0, "T3 might + T1 grace + T1 vitality"),
        ];
        for (name, a1, a2, a3, a4, a5, a6, a7, a8, a9, note) in dual {
            let a = ActorAttributes::new(a1, a2, a3, a4, a5, a6, a7, a8, a9);
            println!("  {}: {}", name, note);
            println!("    M={} ({:?}), G={} ({:?}), V={} ({:?}), F={} ({:?}), I={} ({:?}), P={} ({:?})",
                a.might(), a.commitment_tier_for(a.might()),
                a.grace(), a.commitment_tier_for(a.grace()),
                a.vitality(), a.commitment_tier_for(a.vitality()),
                a.focus(), a.commitment_tier_for(a.focus()),
                a.instinct(), a.commitment_tier_for(a.instinct()),
                a.presence(), a.commitment_tier_for(a.presence()));
        }

        println!("\nTRIPLE-PAIR (10 points across all three pairs):");
        let triple = [
            ("Jack of All Trades", -3, 0, 0, -3, 0, 0, -4, 0, 0, "T1 each, spread thin"),
            ("Spectrum Generalist", 0, 3, 0, 0, 3, 0, 0, 4, 0, "T2 on 6 stats but low values"),
            ("Hybrid Versatile", -4, 1, 0, -3, 1, 0, -1, 1, 0, "Mixed approach"),
        ];
        for (name, a1, a2, a3, a4, a5, a6, a7, a8, a9, note) in triple {
            let a = ActorAttributes::new(a1, a2, a3, a4, a5, a6, a7, a8, a9);
            println!("  {}: {}", name, note);
            println!("    M={} ({:?}), G={} ({:?}), V={} ({:?}), F={} ({:?}), I={} ({:?}), P={} ({:?})",
                a.might(), a.commitment_tier_for(a.might()),
                a.grace(), a.commitment_tier_for(a.grace()),
                a.vitality(), a.commitment_tier_for(a.vitality()),
                a.focus(), a.commitment_tier_for(a.focus()),
                a.instinct(), a.commitment_tier_for(a.instinct()),
                a.presence(), a.commitment_tier_for(a.presence()));
            let total = a.might() + a.grace() + a.vitality() + a.focus() + a.instinct() + a.presence();
            println!("    Total stats: {}\n", total);
        }

        panic!("Exploration complete - check output above");
    }

    #[test]
    #[ignore]
    fn find_build_varieties() {
        println!("\n=== 10-POINT BUILD VARIETIES ===");
        println!("Axis×16, Spectrum×12, Axis=0×6");
        println!("T1≥30, T2≥45, T3≥60\n");

        let builds = [
            // Pure axis builds
            ("4/0/0 + 4/0/0 + 2/0/0", (-4,0,0, -4,0,0, -2,0,0)),
            ("4/0/0 + 3/0/0 + 3/0/0", (-4,0,0, -3,0,0, -3,0,0)),
            ("5/0/0 + 5/0/0", (-5,0,0, -5,0,0, 0,0,0)),

            // Pure spectrum builds
            ("0/5/0 + 0/5/0", (0,5,0, 0,5,0, 0,0,0)),
            ("0/4/0 + 0/3/0 + 0/3/0", (0,4,0, 0,3,0, 0,3,0)),
            ("0/3/0 + 0/3/0 + 0/4/0", (0,3,0, 0,3,0, 0,4,0)),

            // Hybrid builds
            ("2/3/0 + 2/3/0", (-2,3,0, -2,3,0, 0,0,0)),
            ("1/4/0 + 1/4/0", (-1,4,0, -1,4,0, 0,0,0)),
            ("3/2/0 + 3/2/0", (-3,2,0, -3,2,0, 0,0,0)),
        ];

        for (desc, (a1,a2,a3, a4,a5,a6, a7,a8,a9)) in builds {
            let attrs = ActorAttributes::new(a1,a2,a3, a4,a5,a6, a7,a8,a9);
            let m = attrs.might();
            let g = attrs.grace();
            let v = attrs.vitality();
            let f = attrs.focus();
            let i = attrs.instinct();
            let p = attrs.presence();

            let tiers: Vec<_> = [m,g,v,f,i,p].iter()
                .filter(|&&val| val > 0)
                .map(|&val| attrs.commitment_tier_for(val))
                .collect();

            let tier_counts = format!(
                "{}×T3 + {}×T2 + {}×T1",
                tiers.iter().filter(|&&t| t == CommitmentTier::T3).count(),
                tiers.iter().filter(|&&t| t == CommitmentTier::T2).count(),
                tiers.iter().filter(|&&t| t == CommitmentTier::T1).count()
            );

            let total = m + g + v + f + i + p;
            println!("{} = {} ({})", desc, tier_counts, total);
        }

        panic!("Exploration complete");
    }

}