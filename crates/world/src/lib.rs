//! World generation: the event stack that composes terrain, and the objects
//! its layers share — the lattice every outline is drawn on, the tectonic
//! plates, and the chains a consumer reads distance to.

pub(crate) mod noise;
pub mod chains;
pub mod continents;
pub mod events;
pub mod hillslope;
pub mod water;
pub mod lattice;
pub mod tectonic;

pub use common::{ArrayVec, Cover, Content};
pub use events::plates::{substrate_elevation_at, substrate_on, Coasts};

// ──── The vertical scale ────

/// Height of one z-level in this crate's horizontal unit, the tile spacing:
/// tan 34°, so one level per tile climbs at the angle of repose.
///
/// The renderer stands a z-level `common::grid::RISE` of its units tall
/// and spaces neighbouring tiles √3 of its units apart, while this crate
/// spaces them one apart ([`TILE_SPACING`]), so a z-level is that height
/// over √3 here. The dips the ranges are built at and the grade a continent
/// leans at are angles on the ground the player walks, so every horizontal
/// length derived from a vertical one, a sheet spacing from a crust
/// thickness, a tilt amplitude from a grade, goes through this ratio and
/// renders at the angle it was designed at.
pub const RISE: f64 = 0.674_508_516_842_426_5;

/// Substrate elevation in z-levels at the abyssal plain, the deepest stop on
/// the terrain shader's elevation ramp.
pub const SEA_MAX_DEPTH: f64 = 200.0;

/// The sea floor off a coast, shore to abyss: a shoreface falling from the
/// shoreline and easing onto the shelf, the shelf running out at a gentle
/// grade to its edge, and the continental slope dropping from there to the
/// abyssal plain. Depths in z-levels, lengths in world units.
///
/// The shoreface is what makes a shore: the floor leaves the datum at a
/// slope a tile can show, [`SHOREFACE_DEPTH`] over the coast's own reach,
/// so where the sea meets the land is one line at every distance it is
/// drawn from. A floor that flattened into the shoreline put half a level
/// of depth hundreds of tiles out, and every level of detail drew its own
/// coast. It is wadeable for the first third of its reach and over a
/// walker's head by the end of it.
pub const SHOREFACE_DEPTH: f64 = 3.0;
/// How deep the shelf's edge lies: Earth's shelf break at about 130 m, in
/// the ratio [`SEA_MAX_DEPTH`] stands to the mean ocean's 3,700 m.
pub const SHELF_EDGE_DEPTH: f64 = 10.0;
/// How far the continental slope runs from the shelf's edge to the
/// abyssal plain.
pub const SLOPE_WIDTH: f64 = 250.0;

/// A coast's own sea floor, by its margin (Inman & Nordstrom 1971): how
/// far its shoreface runs before it eases onto the shelf, and how far out
/// the shelf's edge lies, in world units. Where the ocean's plate goes
/// under the continent the shelf is narrow and the shore steep; on a quiet
/// margin the shelf is wide, and wider the longer the continent's plate
/// has been wearing down onto it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shelf {
    pub shoreface: f64,
    pub width: f64,
}

impl Shelf {
    /// The steepest shore and the narrowest shelf, a margin closing as
    /// hard as any; the gentlest shore; and the shelf of a quiet margin
    /// whose plate is as young as a plate is, and as old.
    const SHOREFACE_CLOSING: f64 = 18.0;
    const SHOREFACE_QUIET: f64 = 55.0;
    const WIDTH_CLOSING: f64 = 60.0;
    const WIDTH_YOUNG: f64 = 200.0;
    const WIDTH_OLD: f64 = 500.0;
    /// The widest any shelf lies.
    pub const WIDEST: f64 = Self::WIDTH_OLD;

    /// The shelf of a margin closing `closing` of the way to as hard as
    /// any, none on a quiet one, off a plate of `age`, both 0 to 1.
    pub fn of(closing: f64, age: f64) -> Self {
        let (closing, age) = (closing.clamp(0.0, 1.0), age.clamp(0.0, 1.0));
        let quiet = Self::WIDTH_YOUNG + (Self::WIDTH_OLD - Self::WIDTH_YOUNG) * age;
        Shelf {
            shoreface: Self::SHOREFACE_QUIET + (Self::SHOREFACE_CLOSING - Self::SHOREFACE_QUIET) * closing,
            width: quiet + (Self::WIDTH_CLOSING - quiet) * closing,
        }
    }
}

/// How deep the sea floor lies `d` world units off a shore whose shelf is
/// `shelf`, in z-levels: the shoreface, the shelf and the continental
/// slope, each a term that has run its course before the next matters.
/// Never shallower further out.
pub fn sea_floor(d: f64, shelf: Shelf) -> f64 {
    let shoreface = SHOREFACE_DEPTH * (1.0 - (-d / shelf.shoreface).exp());
    let over_shelf = (SHELF_EDGE_DEPTH - SHOREFACE_DEPTH) * (d / shelf.width).clamp(0.0, 1.0);
    let slope = (SEA_MAX_DEPTH - SHELF_EDGE_DEPTH) * smoothstep((d - shelf.width) / SLOPE_WIDTH);
    shoreface + over_shelf + slope
}

/// Substrate elevation in z-levels a plate's interior holds — the ceiling on
/// continental freeboard before any layer above adds relief.
///
/// Ratio to [`SEA_MAX_DEPTH`] is Earth's: mean land elevation 840 m against a
/// mean ocean depth of 3,700 m. Continental crust floats high, but its
/// freeboard is a small fraction of the ocean it floats beside, so the whole
/// mountain range of the shader's elevation ramp is left to the layers above.
pub const CONTINENT_MAX_RISE: f64 = SEA_MAX_DEPTH * (840.0 / 3700.0);

/// Continental rise exponent, applied to the normalised shore→interior
/// fraction. Under one, so the land climbs steeply away from the datum and
/// flattens toward the interior: a backshore a few levels high within a
/// few tiles of the shore, never a plain lying at sea level.
pub const CONTINENT_RISE_EXPONENT: f64 = 0.5;

// ──── Coordinate Conversion ────

pub(crate) const SQRT_3: f64 = 1.7320508075688772;

/// Distance between neighbouring tiles in this crate's horizontal unit,
/// held by `hex_to_world` for all six directions: a tile is the unit. The
/// renderer spaces tiles √3 of its own units apart, which [`RISE`] carries.
pub const TILE_SPACING: f64 = 1.0;

/// The seed the served world is built from. The server's registry and
/// every probe of the served world draw from this one, or they draw
/// different worlds.
pub const WORLD_SEED: u64 = 0x9E3779B97F4A7C15;

pub use common::{hex_to_world, world_to_hex};

/// The smooth step from 0 at `u ≤ 0` to 1 at `u ≥ 1`, level at both: how
/// every layer eases one thing into another over a reach. One function, so
/// a gate reads the same wherever it is taken.
pub fn smoothstep(u: f64) -> f64 {
    let u = u.clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT_SEED: u64 = 0x9E3779B97F4A7C15;

    /// A z-level here is the renderer's over its tile spacing, and a level per
    /// tile is the angle of repose.
    #[test]
    fn a_level_per_tile_is_repose() {
        let renderer = common::grid::RISE as f64 / (common::grid::HEX_RADIUS as f64 * SQRT_3);
        assert!((renderer - RISE).abs() < 1e-6, "the renderer's z-level is {renderer} tiles, this crate's {RISE}");
        assert!((RISE.atan().to_degrees() - 34.0).abs() < 1e-9);
    }

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

    /// Two independent composites with the same seed produce identical
    /// tiles, whatever order they are asked in: the height, the water, the
    /// cover, the rock and the den.
    #[test]
    fn composite_reproducible() {
        let c1 = make_composite();
        let c2 = make_composite();
        let tiles: Vec<(i32, i32)> = (-100..=100).step_by(10).flat_map(|q| (-100..=100).step_by(10).map(move |r| (q, r))).collect();
        for &(q, r) in tiles.iter().rev() {
            c2.tile_at(q, r);
        }
        for &(q, r) in &tiles {
            let a = c1.tile_at(q, r);
            let b = c2.tile_at(q, r);
            assert_eq!(a.elevation, b.elevation,
                "elevation mismatch at ({q},{r}): {:.2} vs {:.2}", a.elevation, b.elevation);
            assert_eq!(a.water, b.water, "water mismatch at ({q},{r})");
            assert_eq!(a.cover, b.cover, "cover mismatch at ({q},{r})");
            assert_eq!(a.rock, b.rock, "rock mismatch at ({q},{r})");
            assert_eq!(a.den, b.den, "den mismatch at ({q},{r})");
        }
    }
}
