//! SeaEvent — Event #1: ocean bathymetry below sea level.

//! Everything the regime field puts below the land threshold gets a depth, so
//! water is terrain rather than an untextured extension of the z=0 plane.

//! Depth is driven by the **pre-sigmoid** regime field. `regime_value_at`
//! applies a steepness-40 sigmoid that deliberately flattens deep water, and it
//! does its job too well to reuse here: 93.7% of water tiles land within 17% of
//! the sigmoid floor, and the median water tile reads 0.0000. `raw_regime_noise`
//! still grades smoothly — measured continental shelf is ~230 tiles from shore
//! to a quarter of the raw range, which is a wide beach at player scale.

//! No indexes and no survey: depth is a pure per-tile function of
//! (position, seed), so `deform` is a no-op and the layer contributes only
//! through `query`.

use common::PlateTag;

use crate::hex_to_world;
use crate::plates::{inverse_sigmoid, raw_regime_noise};
use crate::{REGIME_LAND_THRESHOLD, REGIME_SIGMOID_MIDPOINT, REGIME_SIGMOID_STEEPNESS};
use super::index::{CellId, IndexRegistry};
use super::{CellScope, Survey, TileOutput, TileView, WorldEvent};

/// Cell scale in tiles. Bathymetry has no cross-tile structure, so this sets
/// only tile-cache granularity — matched to `PLATE_CELL_SCALE` so the two
/// layers share cell boundaries and warm together.
const SEA_CELL_SCALE: u32 = 1800;

/// Depth in z-levels at the abyssal plain (raw regime ≈ 0). At `RISE` 0.8 this
/// is 160 world units below sea level, matching the deepest stop on the
/// terrain shader's elevation ramp.
pub const SEA_MAX_DEPTH: f64 = 200.0;

/// Shelf profile exponent, applied to the normalised shore→abyss fraction.
/// Greater than 1 holds the near-shore band shallow so the coastline is a
/// wadeable beach instead of a drop-off.
const SHELF_EXPONENT: f64 = 2.0;

/// Ocean bathymetry. Submerges every tile whose regime value falls below the
/// land threshold and corrects the plate-centroid tag to per-tile truth.
pub struct SeaEvent {
    /// Raw (pre-sigmoid) regime value at the shoreline — the `x` where
    /// `sigmoid(x) == REGIME_LAND_THRESHOLD`. Derived from the sigmoid
    /// constants rather than tuned, so retuning the regime field moves the
    /// shoreline here automatically.
    shore_raw: f64,
}

impl SeaEvent {
    pub fn new() -> Self {
        Self {
            shore_raw: inverse_sigmoid(
                REGIME_LAND_THRESHOLD,
                REGIME_SIGMOID_MIDPOINT,
                REGIME_SIGMOID_STEEPNESS,
            ),
        }
    }

    /// Raw regime value at the shoreline. Exposed for probes and tests.
    pub fn shore_raw(&self) -> f64 { self.shore_raw }

    /// Depth in z-levels below sea level at a world position. Zero on land.
    pub fn depth_at(&self, wx: f64, wy: f64, seed: u64) -> f64 {
        let raw = raw_regime_noise(wx, wy, seed);
        if raw >= self.shore_raw {
            return 0.0;
        }
        // 0 at the shoreline, approaching 1 in open ocean.
        let frac = (1.0 - raw / self.shore_raw).clamp(0.0, 1.0);
        SEA_MAX_DEPTH * frac.powf(SHELF_EXPONENT)
    }
}

impl Default for SeaEvent {
    fn default() -> Self { Self::new() }
}

impl WorldEvent for SeaEvent {
    fn name(&self) -> &str { "sea" }
    fn scale(&self) -> u32 { SEA_CELL_SCALE }
    fn survey(&self) -> Survey { Survey::none() }

    fn deform(&self, _scope: &CellScope, _matched: &[(i32, i32)]) {
        // Pure per-tile function — no structural work to do.
    }

    fn query(
        &self,
        q: i32, r: i32,
        _below: &TileView,
        _cell: &(dyn std::any::Any + Send + Sync),
        seed: u64,
    ) -> Option<TileOutput> {
        let (wx, wy) = hex_to_world(q, r);
        let depth = self.depth_at(wx, wy, seed);
        if depth <= 0.0 {
            return None;
        }

        let mut out = TileOutput::default();
        out.elevation_delta = -depth;
        // Per-tile truth beats the plate-centroid tag underneath: PlateEvent
        // classifies a whole 900-tile plate from its centroid, which disagrees
        // with the tile's own regime value ~4% of the time. Tag the water the
        // player is actually standing in, and drop Inland — a submerged tile is
        // not interior land, whatever its plate says.
        out.tags_added.add(PlateTag::Sea);
        out.tags_removed.add(PlateTag::Inland);
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::regime_value_at;

    const SEED: u64 = 0x9E3779B97F4A7C15;

    #[test]
    fn shore_raw_matches_land_threshold() {
        let e = SeaEvent::new();
        let back = crate::plates::sigmoid(
            e.shore_raw(),
            REGIME_SIGMOID_MIDPOINT,
            REGIME_SIGMOID_STEEPNESS,
        );
        assert!(
            (back - REGIME_LAND_THRESHOLD).abs() < 1e-9,
            "inverse_sigmoid round-trip failed: {back} != {REGIME_LAND_THRESHOLD}"
        );
    }

    #[test]
    fn land_is_never_submerged() {
        let e = SeaEvent::new();
        let mut checked = 0;
        for i in 0..200 {
            for j in 0..200 {
                let (wx, wy) = hex_to_world(i * 150 - 15000, j * 150 - 15000);
                if regime_value_at(wx, wy, SEED) >= REGIME_LAND_THRESHOLD {
                    assert_eq!(
                        e.depth_at(wx, wy, SEED), 0.0,
                        "land tile at ({wx}, {wy}) was given depth"
                    );
                    checked += 1;
                }
            }
        }
        assert!(checked > 1000, "expected a meaningful land sample, got {checked}");
    }

    #[test]
    fn depth_is_bounded_and_deepens_offshore() {
        let e = SeaEvent::new();
        let mut deepest: f64 = 0.0;
        for i in 0..200 {
            for j in 0..200 {
                let (wx, wy) = hex_to_world(i * 150 - 15000, j * 150 - 15000);
                let d = e.depth_at(wx, wy, SEED);
                assert!(
                    (0.0..=SEA_MAX_DEPTH).contains(&d),
                    "depth {d} out of range at ({wx}, {wy})"
                );
                deepest = deepest.max(d);
            }
        }
        assert!(
            deepest > SEA_MAX_DEPTH * 0.9,
            "open ocean never approached max depth: {deepest}"
        );
    }
}
