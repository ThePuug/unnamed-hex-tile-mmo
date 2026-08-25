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
use crate::{CONTINENT_CELL_SIZE, hex_to_world};
use super::index::IndexRegistry;
use super::{CellScope, TileOutput, TileView, WorldEvent};

const TILT_SEED: u64 = 0x5469_6C74_5F5F_5F5F; // "Tilt____"

/// Matched to `PLATE_CELL_SCALE` and `MOTION_CELL_SCALE`, so this layer shares
/// cell boundaries with the ones either side of it and warms with them.
///
/// Scale is pure cache granularity here — `deform` is empty and there is no
/// index — but it is not free: a layer's scale dilates every layer beneath it,
/// and choosing a coarser one would deform more plate cells for no gain.
pub const TILT_CELL_SCALE: u32 = 1800;

// ── Wavelength ──────────────────────────────────────────────────────────────

/// Wavelength of the tilt potential, in world units.
///
/// The gradient of a smooth field holds one direction for about a quarter of
/// its wavelength — from a maximum to the next zero — so a landmass sits on one
/// limb only if it fits well inside that quarter. A continent is
/// [`CONTINENT_CELL_SIZE`] across, so eight of them puts the quarter-wave at
/// two continents and a landmass occupies at most half a limb: the lean holds
/// its direction right across, with margin, rather than turning over in the
/// middle.
///
/// Corroboration rather than derivation: the orogen prototype's orientation
/// octave landed at 120,000 for the same reason on a different quantity.
pub const TILT_WAVELENGTH: f64 = 8.0 * CONTINENT_CELL_SIZE;

// ── Amplitude ───────────────────────────────────────────────────────────────

/// Vertical spacing of one z-level in world units. Matches
/// `common::camera::RISE` — a grade is a ratio of two lengths, so both axes
/// have to be in the same units.
const RISE: f64 = 0.8;

/// Regional grade a continent is built to lean at.
///
/// Real continental grades run near a tenth of a percent: Australia's Great
/// Dividing Range stands ~1,000 m above an interior ~1,000 km away. This is the
/// quantity with meaning; the amplitude below is derived from it.
const TARGET_GRADE: f64 = 0.001;

/// Amplitude of the tilt potential, in z-levels.
///
/// A smooth field climbs from zero to its amplitude over a quarter wavelength,
/// so `amplitude = grade × (λ/4) / RISE`: at 0.1% over 25,000 WU that is 25 WU
/// of rise, and 31.25 z at `RISE` 0.8.
///
/// For scale, the substrate gives a continent ~45 z of freeboard, so a tilt of
/// this size is a third of it either way — enough to drown one margin and lift
/// the other, which is the effect wanted rather than a side effect. What it
/// actually achieves is measured, because simplex noise is not a sinusoid and
/// does not reach its bounds: see `tilt_probe::grade_and_coherence`.
pub const TILT_AMPLITUDE: f64 = TARGET_GRADE * (TILT_WAVELENGTH * 0.25) / RISE;

// ── Gating ──────────────────────────────────────────────────────────────────

/// Substrate elevation at which a continent takes the full lean, in z-levels.
///
/// The substrate's land elevation p10 is 8.7 z, so saturating here gives nine
/// tenths of all land the full tilt and confines the taper to the beach band.
///
/// It **saturates** deliberately. Amplitude simply proportional to substrate
/// elevation would put the maximum in the interior and zero at every coast,
/// which is a dome — the one shape a tilt is not. Only the coastal band tapers;
/// the interior leans uniformly and the lean's direction comes from the
/// potential's gradient, not from the coastline.
const TILT_FULL_ELEVATION: f64 = 9.0;

/// How much of the lean the crust at this elevation takes. Zero at and below
/// sea level, one from [`TILT_FULL_ELEVATION`] up.
fn shore_gate(elevation: f64) -> f64 {
    let t = (elevation / TILT_FULL_ELEVATION).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

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

/// The elevation tilt adds at a position, given the substrate beneath it.
pub fn tilt_at(wx: f64, wy: f64, substrate: f64, seed: u64) -> f64 {
    let gate = shore_gate(substrate);
    if gate <= 0.0 { return 0.0 }
    potential(wx, wy, seed) * gate
}

pub struct TiltEvent;

impl TiltEvent {
    pub fn new() -> Self { TiltEvent }
}

impl Default for TiltEvent {
    fn default() -> Self { Self::new() }
}

impl WorldEvent for TiltEvent {
    fn name(&self) -> &str { "tilt" }
    fn scale(&self) -> u32 { TILT_CELL_SCALE }

    /// Nothing originates anywhere, so nothing reaches.
    fn max_influence(&self) -> u32 { 0 }

    fn register_indexes(&self, _registry: &mut IndexRegistry) {}

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

        // Curvature is published as zero rather than computed. The potential's
        // second derivative is of order amplitude / wavelength squared —
        // 3e-9 z per world unit squared — and the creep operator that consumes
        // curvature was measured to need displacements above half a z-level to
        // register at all. Stating a number that small would be stating noise.
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

    /// The gate saturates. A tilt that scaled with substrate elevation would be
    /// a dome, and this is the assertion that separates the two.
    #[test]
    fn gate_saturates_above_the_beach() {
        assert_eq!(shore_gate(-10.0), 0.0);
        assert_eq!(shore_gate(0.0), 0.0);
        assert!(shore_gate(TILT_FULL_ELEVATION * 0.5) > 0.3);
        assert_eq!(shore_gate(TILT_FULL_ELEVATION), 1.0);
        // Saturated: the interior leans uniformly however high it stands.
        assert_eq!(shore_gate(20.0), 1.0);
        assert_eq!(shore_gate(45.0), 1.0);
    }

    /// The amplitude is the target grade restated, so the derivation is
    /// asserted rather than described.
    #[test]
    fn amplitude_matches_the_target_grade() {
        let quarter = TILT_WAVELENGTH * 0.25;
        let grade = TILT_AMPLITUDE * RISE / quarter;
        assert!((grade - TARGET_GRADE).abs() < 1e-12,
            "amplitude implies a {grade} grade against a {TARGET_GRADE} target");
    }

    /// A quarter wavelength has to hold at least two continents, or a landmass
    /// straddles a maximum and leans two ways at once.
    #[test]
    fn a_continent_fits_inside_one_limb() {
        let quarter = TILT_WAVELENGTH * 0.25;
        assert!(quarter >= 2.0 * CONTINENT_CELL_SIZE,
            "quarter wavelength {quarter} holds under two continents");
    }

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
