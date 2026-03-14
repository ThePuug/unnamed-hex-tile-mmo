mod noise;
mod plates;
mod microplates;
pub mod spine;

pub use common::{ArrayVec, PlateTag, Tagged, MAX_PLATE_TAGS};
pub use plates::{PlateCenter, PlateCache, macro_plate_at, warped_plate_at,
                 macro_plates_in_radius, macro_plate_neighbors,
                 regime_value_at, warp_strength_at};
pub use microplates::{MicroCellGeometry, MicroplateCenter, MicroplateCache, PlateCentroid,
                      micro_cell_at, macro_plate_for, plate_info_at,
                      micro_cells_for_macro};
pub use spine::{generate_spines, cross_section_profile, cross_section_tag,
                micro_elevation_offset, RIDGE_PEAK_ELEVATION,
                Peak, SpineInstance, SpineCache, RavineStats, RavineProbe,
                evaluate_elevation, discretize_elevation, ELEVATION_PER_Z};

// ──── Constants ────

/// Base macro plate spacing in tiles.
pub const MACRO_CELL_SIZE: f64 = 900.0;

/// Very large scale noise wavelength for jitter modulation.
pub const JITTER_NOISE_WAVELENGTH: f64 = 15000.0;

/// Minimum jitter factor (stable regions → regular plates).
pub const JITTER_MIN: f64 = 0.1;

/// Maximum jitter factor (chaotic regions → irregular plates).
pub const JITTER_MAX: f64 = 0.45;

/// Minimum macro cell suppression rate (at coastlines — many small plates).
pub const SUPPRESSION_RATE_MIN: f64 = 0.05;

/// Maximum macro cell suppression rate (deep inland/water — few large plates).
pub const SUPPRESSION_RATE_MAX: f64 = 0.70;

/// Deep ocean suppression multiplier relative to deep land.
/// 1.0 = symmetric. 1.5 = deep ocean suppresses 50% more than deep land,
/// reaching max suppression at ~67% of the way from coast to regime=0.
/// Produces larger, sparser ocean plates while keeping land plates moderate.
pub const OCEAN_SUPPRESSION_BOOST: f64 = 1.5;

/// Regime classification threshold. Values below → water, above → land.
/// With multiplicative world-gate composition, output values are compressed
/// toward 0 relative to the old additive formula. 0.3 restores a balanced
/// land/water ratio: the gate pushes deep-ocean regions to near-zero, and
/// the threshold sits in the transition zone of the product distribution.
pub const REGIME_LAND_THRESHOLD: f64 = 0.15;

// ──── Macro Plate Warp Constants ────

/// Noise wavelength for per-cell boundary wobble.
/// Short enough for irregularity within a plate neighborhood,
/// long enough that adjacent micro cells don't flip randomly.
pub const WARP_NOISE_WAVELENGTH: f64 = 400.0;

/// Triple-prime local fBm wavelengths for the regime noise (B/C/D octaves).
/// Log-scale ratios: B/C ≈ 2.3×, C/D ≈ 2.2× — even spectral separation.
/// LCM ≈ 1.4 billion tiles — effectively never repeats within any playable region.
pub const WARP_PRIME_B: f64 = 12506.5;  // Continental scale — large coastal variation
pub const WARP_PRIME_C: f64 =  5501.5;  // Regional scale
pub const WARP_PRIME_D: f64 =  2499.5;  // Peninsula scale

/// Minimum warp strength — pure Voronoi, convex plates.
pub const WARP_STRENGTH_MIN: f64 = 0.0;

/// Maximum warp strength — irregular, non-convex plates.
pub const WARP_STRENGTH_MAX: f64 = 300.0;

/// Warp strength above this threshold classifies a plate as coastal.
/// High gradient magnitude signals a coastline transition zone regardless
/// of whether the plate's own regime is land or water.
pub const COASTAL_WARP_THRESHOLD: f64 = 40.0;

/// World-unit step for gradient sampling of the regime field.
pub const GRAD_STEP: f64 = 50.0;

/// Sigmoid midpoint on the regime noise field.
/// Values below this tend toward 0 (water), above toward 1 (land).
/// With multiplicative gating (local × world_gate), the product distribution
/// peaks around 0.25 at symmetric points. 0.30 centers the transition where
/// the product distribution actually concentrates.
pub const REGIME_SIGMOID_MIDPOINT: f64 = 0.30;

/// Sigmoid steepness on the regime noise field.
/// Controls how sharp the water/land transition is.
/// Must be high enough to push the bell-shaped noise distribution
/// into bimodal plateaus. The raw noise (sum of 3 simplex octaves)
/// has std ≈ 0.19 around 0.5, so the transition half-width ln(9)/k
/// must be smaller than ~0.1 to produce clear land/water separation.
pub const REGIME_SIGMOID_STEEPNESS: f64 = 40.0;

/// Maximum noise stretch ratio along coastlines.
/// At peak gradient, warp noise features are MAX_ELONGATION× longer
/// along the coast than across it.
pub const MAX_ELONGATION: f64 = 8.0;

/// Sigmoid midpoint for world-gate sharpening (applied to the cellular gate before local × gate).
/// 0.5 keeps the transition centered relative to the [0, 1] cellular gate range.
/// Higher values → smaller continents; lower values → larger continents.
/// Needs re-tuning after cell size and domain warp changes — use `--layers regime` to calibrate.
pub const WORLD_GATE_SIGMOID_MIDPOINT: f64 = 0.35;

/// Sigmoid steepness for world-gate sharpening.
/// The cellular gate already produces a linear falloff from 1 (continent center) to 0
/// (ocean midpoint). This sigmoid sharpens the continent edges. Lower values give
/// more gradual coastal falloff; higher values give harder edges and more circular continents.
/// Needs re-tuning after cell size and domain warp changes — use `--layers regime` to calibrate.
pub const WORLD_GATE_SIGMOID_STEEPNESS: f64 = 12.0;

/// Spacing between continental seed points (world units).
/// One cell ≈ one world. 12.5k diameter gives recognizable features (peninsulas, bays)
/// within each world while maintaining clear ocean gaps between worlds.
pub const CONTINENT_CELL_SIZE: f64 = 12500.0;

/// Maximum jitter of continental seed point from hex cell center, as fraction of cell size.
/// 0.0 = regular grid; 0.45 = nearly random clustering. 0.35 gives organic variation.
pub const CONTINENT_JITTER: f64 = 0.35;

/// Domain warp amplitude for cellular world gate (world units).
/// Displaces the query point before Voronoi lookup, creating irregular coastlines.
/// ~24% of cell size produces peninsula and bay features within a world.
pub const CONTINENT_WARP_AMPLITUDE: f64 = 2000.0;

/// Domain warp noise wavelength for cellular world gate (world units).
/// ~4-5k at world scale produces 2-3 major coastal lobes per world.
pub const CONTINENT_WARP_WAVELENGTH: f64 = 4000.0;

/// Regional character simplex wavelength (world units).
/// Spans many worlds — where it peaks, worlds expand into large continents;
/// where it troughs, worlds shrink to small islands.
pub const REGIONAL_CHARACTER_WAVELENGTH: f64 = 87500.0;

/// Minimum regional modulation factor.
/// Min > 0 ensures every world has at least some land.
/// Low values (0.1) let trough regions shrink to tiny islands.
pub const REGIONAL_MOD_MIN: f64 = 0.1;

/// Maximum regional modulation factor.
/// >1.0 lets peak-region worlds overfill their cellular gate area,
/// producing broader continents with fewer ocean gaps.
pub const REGIONAL_MOD_MAX: f64 = 1.15;

// ──── Microplate Sub-Grid Constants ────

/// Microplate hex lattice spacing in tiles (1/4 of macro).
pub const MICRO_CELL_SIZE: f64 = 225.0;

/// Margin to populate beyond the region of interest before running fix_orphans.
///
/// A micro cell is assigned to the macro plate whose seed wins the warped
/// Voronoi contest. The worst-case distance from a micro cell to its winning
/// seed is `MACRO_CELL_SIZE × MAX_ELONGATION + WARP_STRENGTH_MAX`. Populating
/// this margin guarantees every plate seed that owns a cell inside the region
/// is visible, so fix_orphans can always see the full main body.
pub const ORPHAN_CORRECTION_MARGIN: f64 = MACRO_CELL_SIZE * MAX_ELONGATION + WARP_STRENGTH_MAX;
// = 900 × 8.0 + 300 = 7 500 world units

/// Micro cell suppression rate — uniform across all terrain types.
/// Shape variation comes from jitter, not density modulation.
pub const MICRO_SUPPRESSION_RATE: f64 = 0.0;

// ──── Microplate Jitter Constants ────

/// Noise wavelength for microplate jitter modulation.
pub const MICRO_JITTER_WAVELENGTH: f64 = 2500.0;

/// Minimum microplate jitter factor.
pub const MICRO_JITTER_MIN: f64 = 0.10;

/// Maximum microplate jitter factor.
pub const MICRO_JITTER_MAX: f64 = 0.0;

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
    caches: std::sync::Mutex<TerrainCaches>,
}

struct TerrainCaches {
    plate_cache: PlateCache,
    spine_cache: SpineCache,
}

impl Default for Terrain {
    fn default() -> Self {
        Self::new(0x9E3779B97F4A7C15)
    }
}

impl Terrain {
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            caches: std::sync::Mutex::new(TerrainCaches {
                plate_cache: PlateCache::new(seed),
                spine_cache: SpineCache::new(seed),
            }),
        }
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Evaluate terrain height at a hex tile position.
    /// Lazily generates and caches spine data as needed.
    pub fn get_height(&self, q: i32, r: i32) -> i32 {
        let (wx, wy) = hex_to_world(q, r);
        let mut caches = self.caches.lock().unwrap();
        let TerrainCaches { ref mut spine_cache, ref mut plate_cache } = *caches;
        let spine_elev = spine_cache.elevation_at(wx, wy, plate_cache);
        discretize_elevation(spine_elev)
    }

    /// Return the continuous f64 elevation before discretization.
    pub fn get_raw_elevation(&self, q: i32, r: i32) -> f64 {
        let (wx, wy) = hex_to_world(q, r);
        let mut caches = self.caches.lock().unwrap();
        let TerrainCaches { ref mut spine_cache, ref mut plate_cache } = *caches;
        spine_cache.elevation_at(wx, wy, plate_cache)
    }

    /// UNCACHED — creates throwaway caches per call.
    /// For hot paths, use `MicroplateCache::plate_info_at` directly.
    pub fn plate_info_at(&self, q: i32, r: i32) -> (PlateCenter, MicroplateCenter) {
        let (wx, wy) = hex_to_world(q, r);
        plate_info_at(wx, wy, self.seed)
    }

    /// UNCACHED — creates throwaway cache per call.
    /// For hot paths, use `PlateCache::plate_at` directly.
    pub fn macro_plate_at(&self, q: i32, r: i32) -> PlateCenter {
        let (wx, wy) = hex_to_world(q, r);
        macro_plate_at(wx, wy, self.seed)
    }
}

// ──── Metrics ────

/// A single timing measurement from a terrain generation phase.
pub struct TerrainMetric {
    pub label: String,
    pub count: u64,
    pub unit: &'static str,
    pub duration: std::time::Duration,
}

/// Results of a full terrain region generation pipeline.
pub struct RegionResult {
    pub plates: Vec<PlateCenter>,
    pub spine_instances: Vec<SpineInstance>,
    pub macro_ids: std::collections::HashMap<u64, u64>,
    pub centroids: Vec<PlateCentroid>,
    pub geometry: MicroCellGeometry,
    pub metrics: Vec<TerrainMetric>,
}

/// Run the plate→classify→spine→prepass pipeline for a viewport region.
/// Each phase records its own timing metric.
pub fn generate_region(
    seed: u64,
    center_x: f64,
    center_y: f64,
    radius: f64,
    with_spines: bool,
) -> RegionResult {
    use std::time::Instant;
    let mut metrics = Vec::new();

    // Phase 1: Plate generation (seed scatter + warped Voronoi assignment)
    let lap = Instant::now();
    let mut plate_cache = PlateCache::new(seed);
    let mut plates = plate_cache.plates_in_radius(
        center_x, center_y,
        radius * std::f64::consts::SQRT_2 + MACRO_CELL_SIZE * 2.0,
    );
    metrics.push(TerrainMetric {
        label: "Plates".into(),
        count: plates.len() as u64,
        unit: "plates",
        duration: lap.elapsed(),
    });

    // Phase 2: Classify tags (Sea/Coast/Inland)
    let lap = Instant::now();
    plate_cache.classify_tags(&mut plates);
    metrics.push(TerrainMetric {
        label: "Classify".into(),
        count: plates.len() as u64,
        unit: "plates",
        duration: lap.elapsed(),
    });

    // Phase 3: Spine generation (candidate selection, growth, peak scattering)
    let spine_instances = if with_spines {
        let lap = Instant::now();
        let instances = generate_spines(&mut plates, &mut plate_cache, seed);
        let total_peaks: u64 = instances.iter()
            .map(|i| i.peaks.len() as u64)
            .sum();

        // Aggregate ravine stats across all instances
        let mut total_streams = 0u64;
        let mut total_merged = 0u64;
        let mut total_hanging = 0u64;
        let mut total_paths = 0u64;
        let mut global_min_w = f64::MAX;
        let mut global_max_w = f64::MIN;
        let mut global_min_d = f64::MAX;
        let mut global_max_d = f64::MIN;
        for inst in &instances {
            let rs = inst.ravine_network.stats();
            total_streams += rs.stream_count as u64;
            total_merged += rs.merged_count as u64;
            total_hanging += rs.hanging_count as u64;
            total_paths += rs.path_count as u64;
            if rs.stream_count > 0 {
                global_min_w = global_min_w.min(rs.width_range.0);
                global_max_w = global_max_w.max(rs.width_range.1);
                global_min_d = global_min_d.min(rs.depth_range.0);
                global_max_d = global_max_d.max(rs.depth_range.1);
            }
        }
        if global_min_w == f64::MAX { global_min_w = 0.0; global_max_w = 0.0; }
        if global_min_d == f64::MAX { global_min_d = 0.0; global_max_d = 0.0; }

        metrics.push(TerrainMetric {
            label: format!(
                "Spines ({} instances): {} peaks, {} streams ({} merged, {} hanging), \
                 width {:.0}-{:.0}, depth {:.0}-{:.0}, {} paths",
                instances.len(), total_peaks, total_streams,
                total_merged, total_hanging,
                global_min_w, global_max_w,
                global_min_d, global_max_d,
                total_paths,
            ),
            count: total_peaks,
            unit: "peaks",
            duration: lap.elapsed(),
        });
        instances
    } else {
        Vec::new()
    };

    // Phase 4: Micro pre-pass (orphan correction, macro ID resolution)
    let lap = Instant::now();
    let mut pre_cache = MicroplateCache::new(seed);
    pre_cache.populate_region(center_x, center_y, radius, radius);
    let macro_ids = pre_cache.all_macro_ids();
    let centroids: Vec<PlateCentroid> = pre_cache.centroids().cloned().collect();
    let geometry = pre_cache.take_geometry();
    metrics.push(TerrainMetric {
        label: "Pre-pass".into(),
        count: macro_ids.len() as u64,
        unit: "micro cells",
        duration: lap.elapsed(),
    });

    RegionResult {
        plates,
        spine_instances,
        macro_ids,
        centroids,
        geometry,
        metrics,
    }
}
