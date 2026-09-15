//! MotionEvent — plate motion, resolved onto the edges of the plate graph.
//!
//! # Claims
//!
//! Each plate moves as one rigid body, and every edge between two plates
//! resolves to what the two motions do there: convergent, divergent or
//! transform, how hard, and for a convergent edge which plate goes under.
//! Thrusting reads the result and builds on the convergent edges; nothing
//! else in the stack places a belt. The layer contributes no elevation.
//!
//! Velocity is a two-channel simplex field sampled at each plate's seed: a
//! hash per plate gives an edge field that is pure noise, every edge
//! disagreeing with the next about sign and vergence alike. It is read at
//! two wavelengths. Convergence is a difference between two plates, so its
//! octave sits a few plates out or there is nothing to differ about.
//! Vergence asks which plate is advancing in absolute terms, which has to
//! hold along a whole belt, so its octave is many plates long.
//!
//! Both vergence rules read a side off something attached to the ground,
//! because relative motion turns over between edges while the drift and the
//! ocean do not. On a coast the ocean plate is the dense one, so it subducts
//! and the wedge stands on the continent, verging seaward. Between two
//! continents or two ocean plates the plate advancing faster in absolute
//! terms goes under, so the wedge verges against the drift and stands on the
//! other. Whether a coast is active or passive is the edge's own
//! convergence: an edge is one object a continent-width long, so there is no
//! run of small pieces to keep in agreement.
//!
//! # The window
//!
//! `deform` reads the edges the plate layer published for this cell,
//! resolves each against its two plates' velocities, and publishes the
//! result under the same cell: an edge's owner is the cell of its midpoint
//! in both indexes. Nothing per tile.

use std::collections::HashMap;

use crate::noise::simplex_2d;
use crate::tectonic::{Edge, PLATE_SPACING};
use crate::world_to_hex;
use super::index::{CellId, CellIndex, EventIndex, IndexRegistry};
use super::plates::{PlateEdgeIndex, GRAPH_CELL_SCALE};
use super::{CellScope, TileOutput, TileView, WorldEvent};

/// The plate graph's cell: the layer reads edges by the cell that owns them
/// and publishes under the same one.
pub const MOTION_CELL_SCALE: u32 = GRAPH_CELL_SCALE;

/// Wavelength of the differential octave of the motion field, in world
/// units: nine plates. Neighbours sample a tenth of a wavelength apart and
/// share a broad sense of direction, while the gradient across that span
/// is still large enough that convergent and divergent edges both occur.
pub const STRAIN_WAVELENGTH: f64 = 9.0 * PLATE_SPACING;

/// Wavelength of the drift octave, in world units: five differential
/// wavelengths. Convergence is a difference between two plates and wants a
/// wavelength near the plate spacing. Vergence asks which plate advances in
/// absolute terms, and that has to hold along a whole belt; read from a
/// field that turns over every few plates it gives a belt whose steep side
/// alternates, which is the one failure this layer exists to prevent.
const DRIFT_WAVELENGTH: f64 = 5.0 * STRAIN_WAVELENGTH;

/// Below this the along-strike component reads as absent, so an edge with no
/// motion at all does not classify as transform on rounding noise.
const TRANSFORM_EPSILON: f64 = 1e-9;

pub(crate) const STRAIN_SEED_X: u64 = 0x4D6F_7469_6F6E_5F58; // "Motion_X"
pub(crate) const STRAIN_SEED_Y: u64 = 0x4D6F_7469_6F6E_5F59; // "Motion_Y"
const DRIFT_SEED_X: u64 = 0x4472_6966_745F_5F58; // "Drift__X"
const DRIFT_SEED_Y: u64 = 0x4472_6966_745F_5F59; // "Drift__Y"

// ── Published types ─────────────────────────────────────────────────────────

/// What a continent–ocean edge is doing. A consumer reads this rather than
/// inferring it from a convergence threshold of its own choosing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarginClass {
    /// Not a coast: both sides continental, or both oceanic.
    Interior,
    /// A coast the motion closes. The ocean plate subducts, the wedge stands
    /// on the continent and verges seaward.
    Active,
    /// A coast the motion does not close: a coastal plain, no range.
    Passive,
}

/// How an edge resolves, once convergence and transform are known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundaryRegime {
    Convergent,
    Divergent,
    /// Along-strike motion exceeds normal motion either way.
    Transform,
}

/// One edge of the plate graph, resolved against the motion of both plates.
#[derive(Debug, Clone)]
pub struct BoundarySegment {
    /// The edge, with its two plates and its chain.
    pub edge: Edge,
    /// Signed normal component of `v_a − v_b`, positive when the plates
    /// close.
    pub convergence: f64,
    /// Magnitude of the along-strike component of `v_a − v_b`.
    pub transform: f64,
    /// Unit vector toward the plate that goes under: the flank a wedge on
    /// this edge leans onto, the steep short side. Zero when the edge is not
    /// convergent.
    pub vergence_x: f64,
    pub vergence_y: f64,
    pub margin: MarginClass,
}

impl BoundarySegment {
    pub fn regime(&self) -> BoundaryRegime {
        if self.transform > self.convergence.abs() {
            BoundaryRegime::Transform
        } else if self.convergence > 0.0 {
            BoundaryRegime::Convergent
        } else {
            BoundaryRegime::Divergent
        }
    }

    pub fn mid(&self) -> (f64, f64) {
        self.edge.mid()
    }

    /// The plate the wedge stands on, for a convergent edge: the one the
    /// vergence points away from.
    pub fn overriding(&self) -> &crate::tectonic::Plate {
        let (mx, my) = self.mid();
        let toward_a = (self.edge.a.wx - mx) * self.vergence_x + (self.edge.a.wy - my) * self.vergence_y;
        if toward_a > 0.0 { &self.edge.b } else { &self.edge.a }
    }
}

/// Resolved edges, by the cell that owns the edge.
#[derive(Default)]
pub struct PlateBoundaryIndex {
    pub cells: HashMap<CellId, Vec<BoundarySegment>>,
}

impl PlateBoundaryIndex {
    /// Segments owned by any of `cell_ids`.
    pub fn segments_in(&self, cell_ids: &[CellId]) -> Vec<&BoundarySegment> {
        cell_ids.iter()
            .filter_map(|id| self.cells.get(id))
            .flat_map(|v| v.iter())
            .collect()
    }
}

impl CellIndex for PlateBoundaryIndex {
    type Cell = Vec<BoundarySegment>;

    fn set(&mut self, cell: CellId, entry: Self::Cell) {
        self.cells.insert(cell, entry);
    }
}

impl EventIndex for PlateBoundaryIndex {
    fn source_scale(&self) -> u32 { MOTION_CELL_SCALE }

    fn tiles(&self, cell_ids: &[CellId]) -> Vec<(i32, i32)> {
        cell_ids.iter()
            .filter_map(|id| self.cells.get(id))
            .flat_map(|segs| segs.iter().map(|s| { let (x, y) = s.mid(); world_to_hex(x, y) }))
            .collect()
    }

    fn neighbors(&self, _q: i32, _r: i32) -> Vec<(i32, i32)> { Vec::new() }

    fn remove_cell(&mut self, cell_id: CellId) {
        self.cells.remove(&cell_id);
    }
}

// ── Motion field ────────────────────────────────────────────────────────────

/// The drift the crust at a position is carried on: the long octave of the
/// motion field, read on its own, because vergence reads it and nothing can
/// reconstruct it from a summed velocity.
pub fn plate_drift(wx: f64, wy: f64, seed: u64) -> (f64, f64) {
    let x = wx / DRIFT_WAVELENGTH;
    let y = wy / DRIFT_WAVELENGTH;
    (
        simplex_2d(x, y, seed ^ DRIFT_SEED_X),
        simplex_2d(x, y, seed ^ DRIFT_SEED_Y),
    )
}

/// Velocity of the plate whose seed is at a position: the drift it rides on
/// plus the differential motion it has against its neighbours.
pub fn plate_velocity(wx: f64, wy: f64, seed: u64) -> (f64, f64) {
    let x = wx / STRAIN_WAVELENGTH;
    let y = wy / STRAIN_WAVELENGTH;
    let (dx, dy) = plate_drift(wx, wy, seed);
    (
        dx + simplex_2d(x, y, seed ^ STRAIN_SEED_X),
        dy + simplex_2d(x, y, seed ^ STRAIN_SEED_Y),
    )
}

/// Resolve one edge against the motion of its two plates.
pub fn resolve(edge: &Edge, seed: u64) -> BoundarySegment {
    let (a, b) = (&edge.a, &edge.b);
    let (dx, dy) = (b.wx - a.wx, b.wy - a.wy);
    let sep = dx.hypot(dy);
    // Normal points a → b; strike is its left perpendicular.
    let (nx, ny) = (dx / sep, dy / sep);
    let (sx, sy) = (-ny, nx);

    let (vax, vay) = plate_velocity(a.wx, a.wy, seed);
    let (vbx, vby) = plate_velocity(b.wx, b.wy, seed);
    let (rel_x, rel_y) = (vax - vbx, vay - vby);

    let convergence = rel_x * nx + rel_y * ny;
    let along = (rel_x * sx + rel_y * sy).abs();
    let transform = if along < TRANSFORM_EPSILON { 0.0 } else { along };

    // Which way the ocean lies across a coast, along the normal.
    let ocean_side = match (a.continental, b.continental) {
        (true, false) => Some(1.0),
        (false, true) => Some(-1.0),
        _ => None,
    };
    let margin = match ocean_side {
        None => MarginClass::Interior,
        Some(_) if convergence > 0.0 => MarginClass::Active,
        Some(_) => MarginClass::Passive,
    };

    let (vergence_x, vergence_y) = if convergence <= 0.0 {
        (0.0, 0.0)
    } else if let Some(side) = ocean_side {
        (nx * side, ny * side)
    } else {
        // The plate advancing faster in absolute terms goes under: carried
        // toward b, it is a that drives into the collision.
        let (mx, my) = edge.mid();
        let (ddx, ddy) = plate_drift(mx, my, seed);
        let sign = if ddx * nx + ddy * ny >= 0.0 { -1.0 } else { 1.0 };
        (nx * sign, ny * sign)
    };

    BoundarySegment { edge: edge.clone(), convergence, transform, vergence_x, vergence_y, margin }
}

// ── MotionEvent ─────────────────────────────────────────────────────────────

pub struct MotionEvent;

impl MotionEvent {
    pub fn new() -> Self { MotionEvent }
}

impl Default for MotionEvent {
    fn default() -> Self { Self::new() }
}

impl WorldEvent for MotionEvent {
    fn name(&self) -> &str { "motion" }
    fn scale(&self) -> u32 { MOTION_CELL_SCALE }

    /// A resolved edge reaches as far as the edge does; the plate layer's
    /// cell already holds that.
    fn max_influence(&self) -> u32 { 0 }

    fn register_indexes(&self, registry: &mut IndexRegistry) {
        registry.pre_register::<PlateBoundaryIndex>();
    }

    fn deform(&self, scope: &CellScope) {
        let cell = scope.cell();
        let edges = scope
            .read::<PlateEdgeIndex>()
            .map(|idx| idx.edges_in(&[cell]))
            .unwrap_or_default();
        let segments: Vec<BoundarySegment> = edges.iter().map(|e| resolve(e, scope.seed())).collect();
        scope.publish::<PlateBoundaryIndex>(segments);
    }

    /// Nothing. The layer states what edges do; it puts nothing on the
    /// ground.
    fn query(
        &self,
        _q: i32, _r: i32,
        _below: &TileView,
        _cell: &(dyn std::any::Any + Send + Sync),
        _seed: u64,
    ) -> Option<TileOutput> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tectonic::edges_of;

    const S: u64 = 0x9E3779B97F4A7C15;

    /// A convergent edge's vergence points across the edge toward one of its
    /// plates, at a coast toward the ocean; a non-convergent edge has none.
    #[test]
    fn vergence_points_at_the_plate_going_under() {
        let mut convergent = 0;
        for cq in -8..=8 {
            for cr in -8..=8 {
                for e in edges_of((cq, cr), S) {
                    let s = resolve(&e, S);
                    let v = s.vergence_x.hypot(s.vergence_y);
                    if s.convergence <= 0.0 {
                        assert_eq!(v, 0.0);
                        continue;
                    }
                    convergent += 1;
                    assert!((v - 1.0).abs() < 1e-9);
                    let under = if std::ptr::eq(s.overriding(), &s.edge.a) { &s.edge.b } else { &s.edge.a };
                    if s.margin == MarginClass::Active {
                        assert!(!under.continental, "at a coast the ocean plate goes under");
                        assert!(s.overriding().continental);
                    }
                }
            }
        }
        assert!(convergent > 50, "{convergent} convergent edges");
    }
}
