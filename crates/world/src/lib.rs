pub(crate) mod noise;
mod plates;
mod microplates;
pub mod events;
pub mod glacial;
pub mod faces;
pub(crate) mod lithology;
pub mod slope_form;
pub mod spine;

pub use common::{ArrayVec, PlateTag, TagSet, Tagged, MAX_PLATE_TAGS};
pub use faces::{ErosionalFace, FaceIndex};
pub use glacial::{Cirque, CirqueProbe, Outflow, GLACIATION_LINE};
pub use plates::{PlateCenter, PlateCache, macro_plate_at, warped_plate_at,
                 macro_plates_in_radius, macro_plate_neighbors,
                 raw_regime_noise, warp_strength_at,
                 substrate_elevation_at, substrate_from_raw,
                 continent_at, ContinentCell,
                 RAW_GRAD_MAX};
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

/// Exponent applied to a cell.s distance from the datum before it drives
/// suppression. Fitted, not derived: it holds plate density across the
/// substrate rewrite.
///
/// Suppression used to read a steepness-40 sigmoid that was saturated on 79% of
/// macro cells, so `SUPPRESSION_RATE_MAX` applied almost everywhere and the
/// variable band covered under 2% of the world. The substrate grades, so that
/// band is now 6x wider — and feeding the grade in unexponentiated raises
/// survival by 47%, which is a world-scale rescale of plate count arriving as a
/// side effect of a terrain change.
///
/// What it trades: per-cell contrast between coastal and deep suppression
/// compresses from a 0.61 spread in survival probability to 0.28. Coastal cells
/// still survive about twice as often as deep ones, and the band they occupy is
/// six times wider, so the aggregate "smaller plates near coasts" effect holds
/// while the count does. Raising it toward 1.0 restores the old per-cell
/// contrast at the cost of the count — that is a rescale, and a decision.
pub const SUPPRESSION_DEPTH_EXPONENT: f64 = 0.11;

// ──── Crustal substrate ────

/// Raw regime value at the sea-level datum. Not a classification threshold:
/// nothing branches on which side of it a tile falls except the substrate
/// curve itself, which is continuous across it. Land is whatever the substrate
/// puts above elevation 0.
///
/// The value is the raw noise level the world's coastline has always sat at,
/// carried over verbatim so the substrate rewrite left the shoreline where it
/// was rather than moving every coast at once.
pub const SHORE_RAW: f64 = 0.2566348724376276;

/// Substrate elevation in z-levels at the abyssal plain (raw regime ≈ 0). At
/// `RISE` 0.8 this is 160 world units below sea level, matching the deepest
/// stop on the terrain shader's elevation ramp.
pub const SEA_MAX_DEPTH: f64 = 200.0;

/// Shelf profile exponent, applied to the normalised shore→abyss fraction.
/// Greater than 1 holds the near-shore band shallow so the coastline is a
/// wadeable beach instead of a drop-off.
pub const SHELF_EXPONENT: f64 = 2.0;

/// Substrate elevation in z-levels where the raw regime field saturates
/// inland — the ceiling on continental freeboard before any layer above adds
/// relief.
///
/// Ratio to [`SEA_MAX_DEPTH`] is Earth's: mean land elevation 840 m against a
/// mean ocean depth of 3,700 m. Continental crust floats high, but its
/// freeboard is a small fraction of the ocean it floats beside, so the whole
/// mountain range of the shader's elevation ramp is left to the layers above.
pub const CONTINENT_MAX_RISE: f64 = SEA_MAX_DEPTH * (840.0 / 3700.0);

/// Continental rise exponent, applied to the normalised shore→interior
/// fraction. The reciprocal of [`SHELF_EXPONENT`], and load-bearing at that
/// value: the sea branch flattens *into* the shoreline, so a land branch that
/// also flattened into it would put a plateau at elevation 0 on both sides —
/// the land mask this substrate replaced, rebuilt out of the curve. The
/// reciprocal makes land climb away from the datum as fast as the sea bed
/// levels into it, leaving no flat band on either side.
pub const CONTINENT_RISE_EXPONENT: f64 = 1.0 / SHELF_EXPONENT;

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

/// Warp strength above which a plate counts as sited in the coastal
/// transition band. A gradient property, not a land test: the field can be
/// steep on either side of sea level, and both are the transition zone.
pub const COASTAL_WARP_THRESHOLD: f64 = 40.0;

/// World-unit step for gradient sampling of the regime field.
pub const GRAD_STEP: f64 = 50.0;

/// Regime-gradient magnitude, per world unit, that saturates plate elongation
/// and warp strength.
///
/// EMPIRICAL, not a bound. Every other normalizer in this file is a provable
/// maximum; this one is a measured percentile, because the field it reads has
/// no useful maximum. The substrate rewrite moved anisotropy off a
/// steepness-40 sigmoid onto the raw field, and the sigmoid's derivative
/// spanned four orders of magnitude where the raw field's spans one — so a
/// bound-derived normalizer elongates a third of the world instead of its
/// coastlines.
///
/// Derived as the p95.5 of measured raw-gradient magnitude × (MAX_ELONGATION −
/// 1), which places that percentile at elongation 2.0. It was fitted to hold
/// two properties of the field it replaced, and those are what a retune must
/// preserve rather than this number:
///   - **4.5% of the world above elongation 2.0** — anisotropy is a coastal
///     effect, not a global one.
///   - **~91% of that within 500 WU of a shoreline** — it lands on coasts.
/// `elongation_gate_probe::normalizer_still_hits_its_targets` asserts both, and
/// pins the percentile across six seeds so retuning the regime field, the
/// continental gate, or the local fBm wavelengths fails loudly here instead of
/// silently flattening plate strike.
pub const ELONGATION_GRAD_NORM: f64 = 3.3e-3;

/// Maximum noise stretch ratio along coastlines.
/// At peak gradient, warp noise features are MAX_ELONGATION× longer
/// along the coast than across it.
///
/// A bound, not a target: [`ELONGATION_GRAD_NORM`] sits above the largest
/// gradient the field produces, so observed elongation tops out near half this.
/// Search radii derived from it (`PLATE_CHUNK_SIZE`, [`ORPHAN_CORRECTION_MARGIN`])
/// stay conservative because of that, never short.
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

pub(crate) const SQRT_3: f64 = 1.7320508075688772;

/// World-unit distance between neighbouring tiles, held by `hex_to_world` for
/// all six directions. Anything reasoning about rise per tile reads it.
pub const TILE_SPACING: f64 = 1.0;

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
    let plate_cache = PlateCache::new(seed);
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

    // Phase 2: Spine generation (candidate selection, growth, peak scattering)
    let spine_instances = if with_spines {
        let lap = Instant::now();
        let instances = generate_spines(&mut plates, &plate_cache, seed);
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

    // Phase 3: Micro pre-pass (orphan correction, macro ID resolution)
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

    fn spine_tags_at(q: i32, r: i32, plate_cache: &PlateCache, spine_cache: &mut SpineCache) -> ArrayVec<[PlateTag; 2]> {
        let (wx, wy) = hex_to_world(q, r);
        let mut tags = ArrayVec::new();
        if let Some(spine_tag) = spine_cache.tag_at(wx, wy, plate_cache) {
            tags.push(spine_tag);
        }
        tags
    }

    #[test]
    fn spine_tags_at_deterministic() {
        let plate_cache = PlateCache::new(DEFAULT_SEED);
        let mut spine_cache = SpineCache::new(DEFAULT_SEED);
        let a = spine_tags_at(100, 50, &plate_cache, &mut spine_cache);
        let b = spine_tags_at(100, 50, &plate_cache, &mut spine_cache);
        assert_eq!(a.as_slice(), b.as_slice());
    }

    /// Tags are spine tags and nothing else. A base classification here would
    /// be the land mask coming back.
    #[test]
    fn spine_region_tags_are_spine_tags() {
        let plate_cache = PlateCache::new(DEFAULT_SEED);
        let mut spine_cache = SpineCache::new(DEFAULT_SEED);
        let mut found_spine = false;
        for q in (-8000..=8000).step_by(400) {
            for r in (-8000..=8000).step_by(400) {
                for tag in spine_tags_at(q, r, &plate_cache, &mut spine_cache) {
                    assert!(
                        matches!(tag, PlateTag::Ridge | PlateTag::Highland | PlateTag::Foothills),
                        "tag should be a spine tag, got {tag:?}"
                    );
                    found_spine = true;
                }
            }
        }
        assert!(found_spine, "should find at least one spine-influenced tile in a 16k-tile grid");
    }

    // ── Composite determinism tests ─────────────────────────────────────────

    fn make_composite() -> events::Composite {
        let seed = DEFAULT_SEED;
        let plate_cache = std::sync::Arc::new(PlateCache::new(seed));
        let mut composite = events::Composite::new(seed);
        composite.add_event(Box::new(events::plates::PlateEvent::with_cache(plate_cache.clone())));
        composite.add_event(Box::new(events::spines::SpineEvent::with_cache(plate_cache, seed)));
        composite.add_event(Box::new(events::slope_form::SlopeFormEvent::new()));
        composite
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

    /// Base plate tags are always present (every tile has a classification).
    /// Every tile stands on the substrate, and the substrate alone decides
    /// whether it is land. Both crust types must occur, or the field is a
    /// constant and the sign test means nothing.
    #[test]
    fn composite_puts_every_tile_on_the_substrate() {
        let composite = make_composite();
        let mut land = 0;
        let mut sea = 0;

        for q in (-4000..=4000).step_by(250) {
            for r in (-4000..=4000).step_by(250) {
                let view = composite.tile_at(q, r);
                let (wx, wy) = hex_to_world(q, r);
                let substrate = substrate_elevation_at(wx, wy, DEFAULT_SEED);
                // Layers above only ever add, so the composite never sits below
                // the substrate it stands on.
                assert!(view.elevation >= substrate - 1e-9,
                    "tile ({q},{r}) at {} is below its substrate {substrate}", view.elevation);
                if substrate >= 0.0 { land += 1 } else { sea += 1 }
            }
        }
        assert!(land > 0 && sea > 0, "expected both crust types: {land} land, {sea} sea");
    }
}
