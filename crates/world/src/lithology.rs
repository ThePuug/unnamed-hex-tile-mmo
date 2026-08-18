//! The material every erosional layer cuts.

//! Glacial and water carving both divide their depth and their wall angle by
//! the resistance of the rock at the carve site. The value is uniform — a
//! lithology layer replacing the body of [`resistance_at`] with real values is
//! the whole retrofit, because no carve site holds a depth or an angle that is
//! not already expressed against it.

/// Resistance of uniform material. 1.0 so it is the identity at every site
/// that divides by it.
const UNIFORM_RESISTANCE: f64 = 1.0;

/// Erosional resistance of the material at a world position. Divides carve
/// depth — resistant rock yields a shallower cut — and divides wall exponents,
/// where a smaller exponent is a steeper wall that resistant rock can hold.
pub(crate) fn resistance_at(_wx: f64, _wy: f64) -> f64 {
    UNIFORM_RESISTANCE
}
