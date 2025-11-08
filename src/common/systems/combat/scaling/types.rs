//! Configuration types for three-pole attribute scaling system
//!
//! Defines the data structures used to configure how ability components scale
//! across three fundamental dimensions:
//! - MAGNITUDE: Absolute power (damage, HP)
//! - COMMITMENT: Specialization efficiency (cooldowns, attack speed)
//! - RATIO: Contested matchups (counter reflection, mitigation penetration)

/// Scalars for magnitude-based calculations
///
/// Magnitude scaling produces absolute outputs from absolute stat inputs.
/// Used for: damage, HP pools, resource capacity
#[derive(Debug, Clone, Copy)]
pub struct MagnitudeScalars {
    /// Per-level scaling factor
    pub level: f32,
    /// Per-stat-point scaling factor
    pub stat: f32,
    /// Per-reach-point scaling factor
    pub reach: f32,
}

/// Curve configuration for commitment-based scaling
///
/// Commitment scaling produces efficiency modifiers based on investment ratio.
/// Used for: cooldown reduction, attack speed, movement speed
#[derive(Debug, Clone, Copy)]
pub struct CommitmentCurve {
    /// Starting value (at 0% investment)
    pub base: f32,
    /// Scaling factor
    pub scale: f32,
    /// Cap on investment ratio (typically 2.0 for spectrum)
    pub max_ratio: f32,
    /// Shape of curve (linear, sqrt, square)
    pub function: CurveFunction,
    /// How to apply the result (additive, multiplicative, reduction)
    pub mode: CurveMode,
}

/// Shape function for commitment curves
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurveFunction {
    /// 1:1 relationship
    Linear,
    /// Diminishing returns (sqrt)
    Sqrt,
    /// Accelerating returns (square) - rare
    Square,
}

/// Application mode for commitment scaling results
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurveMode {
    /// base + (ratio * scale)
    Additive,
    /// base * (1 + ratio * scale)
    Multiplicative,
    /// base * (1 - ratio * scale) - for cooldowns/recovery
    Reduction,
}

/// Configuration for ratio-based contested calculations
///
/// Ratio scaling determines effectiveness through actor comparison.
/// Used for: counter reflection, mitigation penetration, CC duration
#[derive(Debug, Clone, Copy)]
pub struct RatioConfig {
    /// Base effectiveness (at 1:1 ratio)
    pub base: f32,
    /// Multiplier applied to base
    pub base_multiplier: f32,
    /// How much ratio affects result
    pub ratio_scale: f32,
    /// Cap on investment ratio
    pub max_ratio: f32,
    /// Minimum resistance value (prevent div/0)
    pub min_resistance: f32,
    /// Whether level difference affects ratio
    pub level_matters: bool,
}
