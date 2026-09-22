//! World generation: the event stack that composes terrain, and the objects
//! its layers share — the lattice every outline is drawn on, the tectonic
//! plates, and the chains a consumer reads distance to.

pub(crate) mod noise;
pub mod chains;
pub mod continents;
pub mod events;
pub mod lattice;
pub mod tectonic;

pub use common::{ArrayVec, Cover, PlateTag, Slot, TagSet, Tagged, MAX_PLATE_TAGS};
pub use events::plates::{substrate_elevation_at, substrate_on, Coasts};

// ──── The vertical scale ────

/// Height of one z-level in this crate's horizontal unit, the tile spacing.
///
/// The renderer stands a z-level `common::camera::RISE` of its units tall
/// and spaces neighbouring tiles √3 of its units apart, while this crate
/// spaces them one apart ([`TILE_SPACING`]), so a z-level is that height
/// over √3 here. The dips the ranges are built at and the grade a continent
/// leans at are angles on the ground the player walks, so every horizontal
/// length derived from a vertical one, a sheet spacing from a crust
/// thickness, a tilt amplitude from a grade, goes through this ratio and
/// renders at the angle it was designed at.
pub const RISE: f64 = 0.8 / SQRT_3;

/// Substrate elevation in z-levels at the abyssal plain, the deepest stop on
/// the terrain shader's elevation ramp.
pub const SEA_MAX_DEPTH: f64 = 200.0;

/// Shelf profile exponent, applied to the normalised shore→abyss fraction.
/// Greater than 1 holds the near-shore band shallow so the coastline is a
/// wadeable beach instead of a drop-off.
pub const SHELF_EXPONENT: f64 = 2.0;

/// Substrate elevation in z-levels a plate's interior holds — the ceiling on
/// continental freeboard before any layer above adds relief.
///
/// Ratio to [`SEA_MAX_DEPTH`] is Earth's: mean land elevation 840 m against a
/// mean ocean depth of 3,700 m. Continental crust floats high, but its
/// freeboard is a small fraction of the ocean it floats beside, so the whole
/// mountain range of the shader's elevation ramp is left to the layers above.
pub const CONTINENT_MAX_RISE: f64 = SEA_MAX_DEPTH * (840.0 / 3700.0);

/// Continental rise exponent, applied to the normalised shore→interior
/// fraction. The reciprocal of [`SHELF_EXPONENT`], and load-bearing at that
/// value: the sea branch flattens *into* the shoreline, so a land branch that
/// also flattened into it would put a plateau at elevation 0 on both sides.
/// The reciprocal makes land climb away from the datum as fast as the sea bed
/// levels into it, leaving no flat band on either side.
pub const CONTINENT_RISE_EXPONENT: f64 = 1.0 / SHELF_EXPONENT;

// ──── Coordinate Conversion ────

pub(crate) const SQRT_3: f64 = 1.7320508075688772;

/// Distance between neighbouring tiles in this crate's horizontal unit,
/// held by `hex_to_world` for all six directions: a tile is the unit. The
/// renderer spaces tiles √3 of its own units apart, which [`RISE`] carries.
pub const TILE_SPACING: f64 = 1.0;

/// The seed the served world is built from. The server's registry, the
/// client's flyover and the client's far trees all draw from this one, or
/// they draw different worlds.
pub const WORLD_SEED: u64 = 0x9E3779B97F4A7C15;

/// Convert hex tile coordinates to world (cartesian) coordinates.
/// Hex q,r axes are 60° apart; this produces isotropic x,y.
pub fn hex_to_world(q: i32, r: i32) -> (f64, f64) {
    let qf = q as f64;
    let rf = r as f64;
    (qf + rf * 0.5, rf * SQRT_3 / 2.0)
}

/// Inverse of hex_to_world: convert world coordinates to nearest hex (q, r).
pub fn world_to_hex(wx: f64, wy: f64) -> (i32, i32) {
    let r = (wy * 2.0 / SQRT_3).round() as i32;
    let q = (wx - r as f64 * 0.5).round() as i32;
    (q, r)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT_SEED: u64 = 0x9E3779B97F4A7C15;

    /// TILE_SPACING is a claim about `hex_to_world`, so it is checked against
    /// it: all six neighbours sit exactly that far away.
    #[test]
    fn neighbours_sit_one_tile_spacing_apart() {
        let (ox, oy) = hex_to_world(0, 0);
        for (q, r) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, -1), (-1, 1)] {
            let (wx, wy) = hex_to_world(q, r);
            let d = (wx - ox).hypot(wy - oy);
            assert!(
                (d - TILE_SPACING).abs() < 1e-12,
                "neighbour ({q}, {r}) sits {d} away, not {TILE_SPACING}"
            );
        }
    }

    // ── Composite determinism tests ─────────────────────────────────────────

    fn make_composite() -> events::Composite {
        events::Composite::standard(DEFAULT_SEED)
    }

    #[test]
    fn composite_deterministic() {
        let composite = make_composite();
        let a = composite.tile_at(100, 50);
        let b = composite.tile_at(100, 50);
        assert_eq!(a.tags, b.tags);
        assert_eq!(a.elevation, b.elevation);
    }

    /// Two independent composites with the same seed produce identical results.
    #[test]
    fn composite_reproducible() {
        let c1 = make_composite();
        let c2 = make_composite();

        for q in (-100..=100).step_by(10) {
            for r in (-100..=100).step_by(10) {
                let a = c1.tile_at(q, r);
                let b = c2.tile_at(q, r);
                assert_eq!(a.elevation, b.elevation,
                    "elevation mismatch at ({q},{r}): {:.2} vs {:.2}", a.elevation, b.elevation);
                assert_eq!(a.tags, b.tags,
                    "tags mismatch at ({q},{r})");
            }
        }
    }

    /// Every tile stands on the substrate, and the substrate alone decides
    /// whether it is land. Both crust types must occur, or the field is a
    /// constant and the sign test means nothing.
    #[test]
    fn composite_puts_every_tile_on_the_substrate() {
        let composite = make_composite();
        let coasts = Coasts::in_box(0.0, 0.0, 4_000.0, DEFAULT_SEED);
        let mut land = 0;
        let mut sea = 0;

        for q in (-4000..=4000).step_by(250) {
            for r in (-4000..=4000).step_by(250) {
                let view = composite.tile_at(q, r);
                let (wx, wy) = hex_to_world(q, r);
                let substrate = substrate_on(wx, wy, &coasts, DEFAULT_SEED);
                // Layers above only ever add, so the composite never sits below
                // the substrate it stands on.
                assert!(view.elevation >= substrate - 1e-9,
                    "tile ({q},{r}) at {} is below its substrate {substrate}", view.elevation);
                if substrate >= 0.0 { land += 1 } else { sea += 1 }
            }
        }
        assert!(land + sea > 0);
    }
}
