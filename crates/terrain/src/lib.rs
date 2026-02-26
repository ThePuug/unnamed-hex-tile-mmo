mod noise;
mod plates;
mod microplates;

pub use plates::{PlateCenter, PlateCache, macro_plate_at, warped_plate_at,
                 macro_plates_in_radius, macro_plate_neighbors,
                 regime_value_at, warp_strength_at};
pub use microplates::{MicroplateCenter, MicroplateCache,
                      micro_cell_at, macro_plate_for, plate_info_at,
                      micro_cells_for_macro, microplate_neighbors};

// ──── Constants ────

/// Base macro plate spacing in tiles.
pub const MACRO_CELL_SIZE: f64 = 1800.0;

/// Very large scale noise wavelength for jitter modulation.
pub const JITTER_NOISE_WAVELENGTH: f64 = 30000.0;

/// Minimum jitter factor (stable regions → regular plates).
pub const JITTER_MIN: f64 = 0.1;

/// Maximum jitter factor (chaotic regions → irregular plates).
pub const JITTER_MAX: f64 = 0.45;

/// Fraction of macro hex grid cells that produce no plate center.
/// Neighbors expand to fill the gap, creating naturally larger plates.
pub const CELL_SUPPRESSION_RATE: f64 = 0.15;

// ──── Macro Plate Warp Constants ────

/// Noise wavelength for per-cell boundary wobble.
/// Short enough for irregularity within a plate neighborhood,
/// long enough that adjacent micro cells don't flip randomly.
pub const WARP_NOISE_WAVELENGTH: f64 = 800.0;

/// Triple-prime warp strength wavelengths. Summed simplex octaves whose
/// LCM ≈ 5.7 trillion tiles — effectively never repeats.
pub const WARP_PRIME_A: f64 = 29989.0;  // Continental scale
pub const WARP_PRIME_B: f64 = 17393.0;  // Province scale
pub const WARP_PRIME_C: f64 = 11003.0;  // Local scale

/// Minimum warp strength — pure Voronoi, convex plates.
pub const WARP_STRENGTH_MIN: f64 = 0.0;

/// Maximum warp strength — irregular, non-convex plates.
pub const WARP_STRENGTH_MAX: f64 = 600.0;

/// World-unit step for gradient sampling of the regime field.
pub const GRAD_STEP: f64 = 100.0;

/// Gradient magnitude at which warp reaches 50% of max.
pub const CONTRAST_MIDPOINT: f64 = 0.5;

/// Sigmoid sharpness — higher values give a sharper transition
/// from calm interiors to active coastline boundaries.
pub const CONTRAST_STEEPNESS: f64 = 6.0;

/// Maximum noise stretch ratio along coastlines.
/// At peak gradient, warp noise features are MAX_ELONGATION× longer
/// along the coast than across it.
pub const MAX_ELONGATION: f64 = 4.0;

// ──── Microplate Sub-Grid Constants ────

/// Microplate hex lattice spacing in tiles (1/4 of macro).
pub const MICRO_CELL_SIZE: f64 = 450.0;

/// Fraction of micro hex grid cells that produce no microplate center.
/// Independent from macro CELL_SUPPRESSION_RATE.
pub const MICRO_SUPPRESSION_RATE: f64 = 0.20;

/// Noise wavelength for microplate jitter modulation.
pub const MICRO_JITTER_WAVELENGTH: f64 = 5000.0;

/// Minimum microplate jitter factor.
pub const MICRO_JITTER_MIN: f64 = 0.10;

/// Maximum microplate jitter factor.
pub const MICRO_JITTER_MAX: f64 = 0.40;

// ──── Coordinate Conversion ────

const SQRT_3: f64 = 1.7320508075688772;

/// Convert hex tile coordinates to world (cartesian) coordinates.
/// Hex q,r axes are 60° apart; this produces isotropic x,y.
pub fn hex_to_world(q: i32, r: i32) -> (f64, f64) {
    let qf = q as f64;
    let rf = r as f64;
    (qf + rf * 0.5, rf * SQRT_3 / 2.0)
}

// ──── Terrain ────

pub struct Terrain {
    seed: u64,
}

impl Default for Terrain {
    fn default() -> Self {
        Self::new(0x9E3779B97F4A7C15)
    }
}

impl Terrain {
    pub fn new(seed: u64) -> Self {
        Self { seed }
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Placeholder — elevation system not yet implemented.
    pub fn get_height(&self, _q: i32, _r: i32) -> i32 {
        0
    }

    /// Returns both the macro plate and micro cell at the given hex tile.
    pub fn plate_info_at(&self, q: i32, r: i32) -> (PlateCenter, MicroplateCenter) {
        let (wx, wy) = hex_to_world(q, r);
        plate_info_at(wx, wy, self.seed)
    }

    /// Returns the geometrically nearest macro seed (pure Voronoi, no warp).
    pub fn macro_plate_at(&self, q: i32, r: i32) -> PlateCenter {
        let (wx, wy) = hex_to_world(q, r);
        macro_plate_at(wx, wy, self.seed)
    }
}
