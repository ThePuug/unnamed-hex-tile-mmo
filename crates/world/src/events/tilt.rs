//! TiltEvent — the regional slope a continent leans on.
//!
//! A landmass with no regional gradient sheds no trunk rivers: water needs
//! somewhere to go, over a distance longer than any single feature. Real
//! continents lean — Australia's divide sheds west across the interior, Africa
//! tilts toward its basins — and that lean is what the coarse drainage layer
//! will route against. It has to exist before dissection can route anything.
//!
//! It sits **below** orogen for the same reason: a belt landing on a sloping
//! base takes its absolute height and its drainage direction from where it
//! lands, so the base has to be there first.
//!
//! # A field, not a feature
//!
//! Empty `deform`, no index, `max_influence` of zero, and a `query` that reads
//! the substrate beneath it and returns one number. Nothing originates anywhere
//! and nothing has extent.
//!
//! # Why a potential rather than a direction
//!
//! The obvious mechanism — lean the land along the plate drift — does not
//! exist. Drift is two independent noise channels and is not curl-free, so no
//! scalar field has it as a gradient; integrating it is unbounded and depends
//! on the path taken. A vector field cannot become an elevation.
//!
//! So the field here is a scalar potential and the lean is its gradient. That
//! makes the slope emergent and bounded by construction rather than integrated
//! into existence.

use std::any::Any;

use crate::noise::simplex_2d;
use crate::{hex_to_world, RISE};
use crate::tectonic::PLATE_SPACING;
use super::plates::shore_gate;
use super::{CellScope, TileOutput, TileView, WorldEvent};

const TILT_SEED: u64 = 0x5469_6C74_5F5F_5F5F; // "Tilt____"

/// Cell scale: tile-cache partitioning only. The layer reads no index and
/// publishes none, so the scale decides how many tiles share one cached
/// cell and nothing else.
pub const TILT_CELL_SCALE: u32 = 1800;

// ── Wavelength ──────────────────────────────────────────────────────────────

/// Wavelength of the tilt potential, in world units.
///
/// The gradient of a smooth field holds one direction for about a quarter of
/// its wavelength — from a maximum to the next zero — so a landmass sits on one
/// limb only if it fits well inside that quarter. A continent is
/// [`PLATE_SPACING`] across, so eight of them puts the quarter-wave at
/// two continents and a landmass occupies at most half a limb: the lean holds
/// its direction right across, with margin, rather than turning over in the
/// middle.
///
/// Corroboration rather than derivation: the orogen prototype's orientation
/// octave landed at 120,000 for the same reason on a different quantity.
pub const TILT_WAVELENGTH: f64 = 8.0 * PLATE_SPACING;

// ── Amplitude ───────────────────────────────────────────────────────────────

/// Regional grade a continent is built to lean at.
///
/// Real continental grades run near a tenth of a percent: Australia's Great
/// Dividing Range stands ~1,000 m above an interior ~1,000 km away. This is the
/// quantity with meaning; the amplitude below is derived from it.
const TARGET_GRADE: f64 = 0.001;

/// Amplitude of the tilt potential, in z-levels.
///
/// A smooth field climbs from zero to its amplitude over a quarter wavelength,
/// so `amplitude = grade × (λ/4) / RISE`, with the quarter wavelength in
/// tiles and [`RISE`] turning the rise into z-levels.
///
/// For scale, the substrate gives a continent ~45 z of freeboard, so a tilt of
/// this size is a third of it either way — enough to drown one margin and lift
/// the other, which is the effect wanted rather than a side effect. What it
/// actually achieves is measured, because simplex noise is not a sinusoid and
/// does not reach its bounds: see `tilt_probe::grade_and_coherence`.
pub const TILT_AMPLITUDE: f64 = TARGET_GRADE * (TILT_WAVELENGTH * 0.25) / RISE;

// ── The field ───────────────────────────────────────────────────────────────

/// The tilt potential at a position, before gating, in z-levels. Exposed for
/// probes and for the viewer's gradient arrows.
pub fn potential(wx: f64, wy: f64, seed: u64) -> f64 {
    TILT_AMPLITUDE * simplex_2d(
        wx / TILT_WAVELENGTH,
        wy / TILT_WAVELENGTH,
        seed ^ TILT_SEED,
    )
}

/// The elevation tilt adds at a position, given the substrate beneath it:
/// the potential, gated by how far above the beach the substrate stands, so
/// the interior leans uniformly and only the coastal band tapers.
pub fn tilt_at(wx: f64, wy: f64, substrate: f64, seed: u64) -> f64 {
    let gate = shore_gate(substrate);
    if gate <= 0.0 { return 0.0 }
    potential(wx, wy, seed) * gate
}

#[derive(Default)]
pub struct TiltEvent;

impl TiltEvent {
    pub fn new() -> Self { TiltEvent }
}

impl WorldEvent for TiltEvent {
    fn name(&self) -> &str { "tilt" }
    fn scale(&self) -> u32 { TILT_CELL_SCALE }

    /// Nothing originates anywhere, so nothing reaches.
    fn max_influence(&self) -> u32 { 0 }

    /// Nothing to place. A tilt has no features, no origins and no extent — it
    /// is a function of position, and the only thing it reads is the substrate
    /// directly beneath the tile being asked about.
    fn deform(&self, _scope: &CellScope) {}

    fn query(
        &self,
        q: i32, r: i32,
        below: &TileView,
        _cell: &(dyn Any + Send + Sync),
        seed: u64,
    ) -> Option<TileOutput> {
        // `below.elevation` is the substrate: this layer sits directly on it.
        let gate = shore_gate(below.elevation);
        if gate <= 0.0 { return None }
        let (wx, wy) = hex_to_world(q, r);
        Some(TileOutput {
            elevation_delta: potential(wx, wy, seed) * gate,
            ..TileOutput::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: u64 = 0x9E3779B97F4A7C15;

    /// Below sea level the layer contributes nothing at all — not a small
    /// number, nothing, so the ocean floor is untouched.
    #[test]
    fn deep_ocean_is_untouched() {
        for i in 0..50 {
            let wx = i as f64 * 2_000.0;
            assert_eq!(tilt_at(wx, 0.0, -50.0, SEED), 0.0);
            assert_eq!(tilt_at(wx, 0.0, -200.0, SEED), 0.0);
        }
    }
}
