//! ThrustingEvent — thrust sheets stacked from a convergent plate edge: the
//! ranges.
//!
//! The shortening an edge takes up is accommodated by thrust sheets stacking
//! in sequence from the deformation front, and their structural relief is
//! the ranges. The front is a real object — the mapped line at the edge of
//! every fold-thrust belt — and here it is a convergent edge of the plate
//! graph, read from the motion layer's resolution with the edge's
//! convergence. A tile's place in the family is its distance to the front,
//! which moves one for one with the ground.
//!
//! # Claims
//!
//! Sheets stack across strike one spacing apart, the spacing set by the
//! thickness of the shortened layer and tightened by a sheet's internal
//! shortening, which accumulates with its age in the wedge. A wedge is as
//! many sheets deep as its edge has shortened: a sheet takes up about one
//! spacing of displacement before the next breaks in front of it, so the
//! count is the edge's convergence over a constant, four at full convergence
//! the way the Himalaya stack four between the foreland and Tibet, and the
//! last sheet is partial, since a sheet grows as displacement accumulates on
//! it. Ranges scale with convergence and never switch on: the weakest edge
//! carries one low frontal ridge, which is what foothills are. Each sheet is
//! an asymmetric wedge whose steep forelimb faces the front; the frontal
//! sheet's forelimb rises from the front itself and the last sheet's
//! backlimb rests on the plateau. A belt is one-sided: the wedge stands on
//! the overriding plate and verges toward the plate going under, whose side
//! carries nothing. A sheet is continuous for its length: a range stands
//! full along its edge, ends against the plate's other edges over one sheet
//! spacing, straight and on the lattice, and its height varies along strike
//! with the ground under it and the front's shape, never with a field finer
//! than a sheet.
//!
//! The retro-wedge at the plateau's far side is not expressed. Unbuilt.
//!
//! # What a tile reads
//!
//! **The plate it stands in, its distance to each of that plate's edges, and
//! no choice among them.** A plate's outline is its edges' chains, and a
//! tile stands in the plate whose outline encloses it. Each convergent edge
//! of that plate gives the tile a wedge by the tile's distance to that edge,
//! and the strongest stands. Distance to each edge is continuous and a choice
//! among edges is not: a tile that took the nearest edge alone would switch
//! wedges along the bisector between two edges of different convergence, a
//! cliff the height of a range. Across an edge the two plates' wedges both
//! reach nothing, so nothing steps.
//!
//! **Spacing varies across strike only.** A spacing read from a field that
//! varies along strike moves the n-th sheet n times as far as the field's
//! gradient: the innermost sheets swing faster than the ground.
//!
//! **Height varies along strike with nothing finer than a sheet.** Scaled by
//! a strain octave, ranges broke into lenses a few thousand units long; a
//! sheet runs its whole length.
//!
//! # The window
//!
//! The layer publishes nothing: the ranges are read off the plate graph at
//! every tile. `prepare` gathers the resolved edges of the graph's cells
//! under the cell and its ring into outlines, one per plate, and a query
//! finds the plate its tile stands in and reads that outline. The cell is the
//! graph's, which is sized so one ring holds the whole outline of any plate a
//! tile in the cell stands in.

use std::any::Any;
use std::collections::{HashMap, HashSet};

use crate::chains::{join_at_nodes, Segment};
use crate::lattice::{nearest_node, node_world, NodeKey, PATH_SWING};
use crate::tectonic::{edges_of, plate_at, plates_near, PlateId, PLATE_REACH};
use crate::{hex_to_world, RISE, SEA_MAX_DEPTH};
use super::index::IndexRegistry;
use super::motion::{resolve, BoundaryRegime, BoundarySegment, PlateBoundaryIndex};
use super::plates::GRAPH_CELL_SCALE;
use super::{CellScope, TileOutput, TileView, WorldEvent};

// ── Sheet geometry ──────────────────────────────────────────────────────────

/// Continental crust thickness in z-levels, on the vertical scale that maps
/// Earth's 4 km abyssal plain to [`SEA_MAX_DEPTH`]: 35 km of crust.
const CRUST_THICKNESS: f64 = SEA_MAX_DEPTH * 35.0 / 4.0;

/// Spacing of sheets across a belt, in world units.
///
/// Thrust spacing scales with the thickness of the layer being shortened, and
/// for basement-cored ranges that layer is the whole crust. So the spacing is
/// the crust's thickness, turned into a horizontal length by the same
/// z-to-world ratio the dips use.
pub const RANGE_SPACING: f64 = CRUST_THICKNESS * RISE;

/// Half-width of a range: half a spacing. A range is one thrust sheet wide,
/// so neighbouring ranges meet at the trough between them with no floor in
/// between. The flanks are asymmetric, and the steep and graded half-widths
/// sum to the spacing.
pub const RANGE_HALF_WIDTH: f64 = RANGE_SPACING * 0.5;

/// Width of a sheet's steep flank as a share of its graded flank.
///
/// A fold-and-thrust wedge is asymmetric because it is built by thrusts that
/// all verge one way: the forelimb dips 40–60° and the backlimb 10–25°, so the
/// steep flank is roughly a third the width of the graded one at the same
/// height.
const STEEP_FLANK_SHARE: f64 = 0.35;

pub const RANGE_STEEP_HALF_WIDTH: f64 =
    2.0 * RANGE_HALF_WIDTH * STEEP_FLANK_SHARE / (1.0 + STEEP_FLANK_SHARE);
pub const RANGE_GRADED_HALF_WIDTH: f64 = 2.0 * RANGE_HALF_WIDTH / (1.0 + STEEP_FLANK_SHARE);

/// Exponent of the cross-strike amplitude taper of one sheet. A squared taper
/// puts the shoulders where a wedge's are: relief falls away slowly near the
/// crest and steeply at the toe. It has no derivation of its own.
const TAPER_EXPONENT: f64 = 2.0;

/// Dip of a range's forelimb, in degrees: the midpoint of the 40–60° a thrust
/// wedge's forelimb dips at, the same geology [`STEEP_FLANK_SHARE`] is read
/// from. The backlimb then falls out at its own 10–25°, which a test holds.
///
/// Nothing here is held walkable. Steep ground is what dissection and slope
/// form exist for, and what a player can climb is theirs to decide.
#[cfg(test)]
const FORELIMB_DIP_DEGREES: f64 = 50.0;

/// tan of the forelimb dip, 50°. A const cannot call `tan`; a test holds
/// the two equal.
const FORELIMB_DIP_TAN: f64 = 1.191_753_592_594_210;

/// Elevation a range reaches at full strength on continental crust: the
/// forelimb dip restated as height over the steep half-width.
///
/// **Height and half-width are coupled, and the coupling is the durable thing
/// here.** A slope is a ratio of two lengths: `dip = atan(rise × RISE /
/// half_width)`. Widening ranges raises this with them, or a range flattens
/// into a ramp.
pub const RANGE_RISE: f64 = RANGE_STEEP_HALF_WIDTH * FORELIMB_DIP_TAN / RISE;

/// Total shortening a fold-thrust belt has taken up, as a fraction of its
/// original width: the Zagros 15–25%, the Appalachians near half. A sheet
/// that has been in the wedge for the belt's whole life has shortened by this
/// much, and sits that much closer to its neighbour.
const BELT_SHORTENING: f64 = 0.30;

/// How far into the wedge a sheet has been there for the belt's whole life:
/// two spacings, so the frontal sheet is fresh and the third is fully
/// shortened.
const SHORTENING_REACH: f64 = 2.0 * RANGE_SPACING;

/// Sheets a wedge stacks at full convergence. A crust-scale fold-thrust belt
/// is a handful of major sheets deep before the hinterland is plateau: the
/// Himalaya stack four between the foreland and Tibet.
pub const WEDGE_SHEETS: f64 = 4.0;

/// Convergence at which an edge stacks the full wedge, in the motion
/// field's units.
///
/// EMPIRICAL: the 90th percentile of convergence over convergent edges,
/// measured by `plate_probe`, so a tenth of convergent edges carry the full
/// four sheets and the rest fewer.
pub const CONVERGENCE_FULL: f64 = 0.43;

/// The farthest the last sheet's backlimb can reach from the front, in world
/// units: every sheet at the untightened spacing. Tightening only brings the
/// rim nearer, so this bounds the wedge and a tile beyond it stands on
/// plateau, never on a sheet.
pub const WEDGE_REACH: f64 = WEDGE_SHEETS * RANGE_SPACING;

/// How far from a plate's other edges a wedge's ranges reach full height:
/// one sheet spacing, so a range ends against the edge it is cut off by.
pub const RANGE_END: f64 = RANGE_SPACING;

/// The farthest a plate's outline lies from a position standing in the
/// plate, in world units: the plate's reach from its seed twice over, plus
/// the swing a chain takes off its straight edge. What every reader of
/// outlines has to be able to see from a tile.
pub const OUTLINE_REACH: f64 = 2.0 * PLATE_REACH + PATH_SWING;

// ── Outlines ────────────────────────────────────────────────────────────────

/// One edge of a plate's outline, as that plate reads it: its chain facing
/// into the plate, and the convergence share it carries where the plate is
/// continental and overrides on it, else none.
pub struct EdgeOutline {
    pub neighbour: PlateId,
    pub segments: Vec<Segment>,
    pub converge: f64,
}

impl EdgeOutline {
    /// Distance from a position to the edge, and the side of its nearest
    /// segment, positive inside the plate.
    pub fn distance(&self, x: f64, y: f64) -> (f64, f64) {
        let mut best = (f64::MAX, 0.0);
        for s in &self.segments {
            let (d, side) = s.distance(x, y);
            if d < best.0 { best = (d, side) }
        }
        best
    }
}

/// A plate as its outline: every edge's chain facing inward, with what each
/// edge carries.
pub struct PlateOutline {
    pub id: PlateId,
    pub continental: bool,
    pub edges: Vec<EdgeOutline>,
}

impl PlateOutline {
    /// Distance from a position to each edge, and whether the position lies
    /// inside the outline: on the inner side of the nearest segment.
    pub fn distances(&self, x: f64, y: f64) -> (Vec<f64>, bool) {
        let mut nearest = (f64::MAX, 0.0);
        let mut out = Vec::with_capacity(self.edges.len());
        for e in &self.edges {
            let (d, side) = e.distance(x, y);
            if d < nearest.0 { nearest = (d, side) }
            out.push(d);
        }
        (out, nearest.1 > 0.0)
    }

    /// Distance to the nearest edge the plate does not override on.
    pub fn quiet_distance(&self, distances: &[f64]) -> f64 {
        self.edges
            .iter()
            .zip(distances)
            .filter(|(e, _)| e.converge <= 0.0)
            .map(|(_, d)| *d)
            .fold(f64::MAX, f64::min)
    }
}

/// The outlines of every plate a set of tiles can stand in, built from the
/// resolved edges the motion layer published.
pub struct Outlines {
    plates: HashMap<PlateId, PlateOutline>,
    seed: u64,
}

impl Outlines {
    /// Outlines from resolved edges: each edge joins both its plates'
    /// outlines, facing each, carrying its convergence for the plate that
    /// overrides on it. Corners join across each whole outline, so a position
    /// nearest a plate corner reads the corner's side.
    pub fn new(resolved: &[&BoundarySegment], seed: u64) -> Self {
        let mut plates: HashMap<PlateId, PlateOutline> = HashMap::new();
        for s in resolved {
            let e = &s.edge;
            let over = if s.regime() == BoundaryRegime::Convergent { Some(s.overriding().id) } else { None };
            let converge = (s.convergence / CONVERGENCE_FULL).clamp(0.0, 1.0);
            for (plate, other) in [(&e.a, &e.b), (&e.b, &e.a)] {
                let left = Segment::is_left((e.x0, e.y0), (e.x1, e.y1), (plate.wx, plate.wy));
                let segments: Vec<Segment> = e
                    .chain
                    .windows(2)
                    .map(|w| Segment::along(node_world(w[0]), node_world(w[1]), left))
                    .collect();
                let outline = plates.entry(plate.id).or_insert_with(|| PlateOutline {
                    id: plate.id,
                    continental: plate.continental,
                    edges: Vec::new(),
                });
                // An oceanic plate carries no wedge: its crust has nothing to
                // shorten into sheets, and an island arc is volcanism. Unbuilt.
                let carries = over == Some(plate.id) && plate.continental;
                outline.edges.push(EdgeOutline {
                    neighbour: other.id,
                    segments,
                    converge: if carries { converge } else { 0.0 },
                });
            }
        }
        for outline in plates.values_mut() {
            let mut all: Vec<Segment> = outline.edges.iter().flat_map(|e| e.segments.iter().copied()).collect();
            let nodes: Vec<(NodeKey, NodeKey)> = all
                .iter()
                .map(|s| (nearest_node(s.x0, s.y0), nearest_node(s.x1, s.y1)))
                .collect();
            join_at_nodes(&mut all, &nodes);
            let mut k = 0;
            for e in &mut outline.edges {
                for s in &mut e.segments {
                    *s = all[k];
                    k += 1;
                }
            }
        }
        Self { plates, seed }
    }

    /// The outlines of every plate within reach of a square box, built from
    /// the plate graph directly: what a view or a probe builds once.
    pub fn in_box(cx: f64, cy: f64, half: f64, seed: u64) -> Self {
        let radius = half * std::f64::consts::SQRT_2 + OUTLINE_REACH;
        let mut seen: HashSet<(PlateId, PlateId)> = HashSet::new();
        let mut resolved = Vec::new();
        for p in plates_near(cx, cy, radius, seed) {
            for e in edges_of(p.id, seed) {
                if seen.insert(e.ids()) {
                    resolved.push(resolve(&e, seed));
                }
            }
        }
        let refs: Vec<&BoundarySegment> = resolved.iter().collect();
        Self::new(&refs, seed)
    }

    pub fn plate(&self, id: PlateId) -> Option<&PlateOutline> {
        self.plates.get(&id)
    }

    pub fn plates(&self) -> impl Iterator<Item = &PlateOutline> {
        self.plates.values()
    }

    /// The plate a position stands in, by its outline, with the position's
    /// distance to each of its edges. The nearest seed's plate first, then
    /// its neighbours, since a chain swings off its straight edge and a
    /// position near an edge can stand across it.
    pub fn at(&self, x: f64, y: f64) -> Option<(&PlateOutline, Vec<f64>)> {
        let home = plate_at(x, y, self.seed).id;
        let first = self.plates.get(&home)?;
        let (d, inside) = first.distances(x, y);
        if inside { return Some((first, d)) }
        for e in &first.edges {
            if let Some(p) = self.plates.get(&e.neighbour) {
                let (d, inside) = p.distances(x, y);
                if inside { return Some((p, d)) }
            }
        }
        None
    }

    /// Elevation the ranges add at a position, in z-levels: the strongest
    /// wedge of the convergent edges of the plate the position stands in,
    /// each ending against every other edge of the plate. A wedge ends
    /// against a convergent edge as much as a quiet one: past the corner that
    /// edge's own wedge climbs from nothing, and the plate across it stands
    /// at nothing. Nothing on an oceanic plate, whose edges carry no
    /// convergence.
    pub fn relief(&self, x: f64, y: f64) -> f64 {
        let Some((plate, distances)) = self.at(x, y) else { return 0.0 };
        let mut best = 0.0f64;
        for (i, (e, d)) in plate.edges.iter().zip(&distances).enumerate() {
            if e.converge <= 0.0 { continue }
            let family = range_family(RANGE_STEEP_HALF_WIDTH - RANGE_SPACING * sheets_in(*d), sheets_of(e.converge));
            if family <= 0.0 { continue }
            let others = distances.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, d)| *d).fold(f64::MAX, f64::min);
            best = best.max(family * smoothstep(others / RANGE_END));
        }
        RANGE_RISE * best
    }
}

pub fn smoothstep(u: f64) -> f64 {
    let u = u.clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

/// Sheets an edge stacks for a convergence share: the count is the
/// shortening over a constant, partial at the end.
pub fn sheets_of(converge: f64) -> f64 {
    converge.clamp(0.0, 1.0) * WEDGE_SHEETS
}

/// Sheets in from the front at a distance: one over the local spacing,
/// accumulated. Spacing is [`RANGE_SPACING`] at the front and tightens by
/// [`BELT_SHORTENING`] over [`SHORTENING_REACH`], the internal shortening a
/// sheet accumulates with its age in the wedge; deeper than that a sheet
/// has been in the wedge for the belt's whole life. Accumulating keeps
/// neighbouring sheets a local spacing apart; a coordinate scaled by the
/// local spacing moves the n-th sheet n times as far as the spacing changed.
pub fn sheets_in(room: f64) -> f64 {
    let aged = |d: f64| {
        -(1.0 - BELT_SHORTENING * d / SHORTENING_REACH).ln() * SHORTENING_REACH
            / (RANGE_SPACING * BELT_SHORTENING)
    };
    if room <= SHORTENING_REACH {
        aged(room)
    } else {
        aged(SHORTENING_REACH) + (room - SHORTENING_REACH) / (RANGE_SPACING * (1.0 - BELT_SHORTENING))
    }
}

/// Distance from the front at which a wedge of `sheets` ends: the inverse
/// of [`sheets_in`].
pub fn rim_of(sheets: f64) -> f64 {
    let at_reach = sheets_in(SHORTENING_REACH);
    if sheets <= at_reach {
        SHORTENING_REACH / BELT_SHORTENING
            * (1.0 - (-sheets * RANGE_SPACING * BELT_SHORTENING / SHORTENING_REACH).exp())
    } else {
        SHORTENING_REACH + (sheets - at_reach) * RANGE_SPACING * (1.0 - BELT_SHORTENING)
    }
}

/// One sheet's cross-section at a signed distance from its crest, positive
/// toward the front: the steep flank faces the front, and the taper reaches
/// zero at the trough it shares with the next sheet.
fn wedge(d: f64) -> f64 {
    let half = if d >= 0.0 { RANGE_STEEP_HALF_WIDTH } else { RANGE_GRADED_HALF_WIDTH };
    let u = d.abs() / half;
    if u >= 1.0 { 0.0 } else { (1.0 - u * u).powf(TAPER_EXPONENT) }
}

/// The wedge across a belt, in [0, 1] of a range's rise, at a coordinate
/// that runs from the frontal sheet's crest toward the front: `sheets`
/// sheets one spacing apart, each meeting the next at a trough, every steep
/// flank facing the front, the last sheet standing at its partial share and
/// nothing past it. A caller with a tighter local spacing hands a coordinate
/// stretched by the ratio.
fn range_family(across: f64, sheets: f64) -> f64 {
    let k = (-across / RANGE_SPACING).round();
    let mut h = 0.0;
    for j in [k - 1.0, k, k + 1.0] {
        if j < 0.0 { continue }
        let weight = (sheets - j).clamp(0.0, 1.0);
        if weight <= 0.0 { continue }
        h += weight * wedge(across + j * RANGE_SPACING);
    }
    h.min(1.0)
}

// ── The event ───────────────────────────────────────────────────────────────

pub struct ThrustingEvent;

impl ThrustingEvent {
    pub fn new() -> Self { ThrustingEvent }
}

impl Default for ThrustingEvent {
    fn default() -> Self { Self::new() }
}

/// The outlines of every plate a cell's tiles can stand in: the resolved
/// edges of the graph's cells under the cell and its ring.
pub fn outlines_of(scope: &CellScope) -> Outlines {
    let cells = scope.source_cells::<PlateBoundaryIndex>();
    scope
        .read::<PlateBoundaryIndex>()
        .map(|idx| Outlines::new(&idx.segments_in(&cells), scope.seed()))
        .unwrap_or_else(|| Outlines::new(&[], scope.seed()))
}

impl WorldEvent for ThrustingEvent {
    fn name(&self) -> &str { "thrusting" }
    fn scale(&self) -> u32 { GRAPH_CELL_SCALE }

    /// Nothing originates here.
    fn max_influence(&self) -> u32 { 0 }

    fn register_indexes(&self, _registry: &mut IndexRegistry) {}

    /// Nothing to place: the ranges are read off the plate graph.
    fn deform(&self, _scope: &CellScope) {}

    fn prepare(&self, scope: &CellScope) -> Box<dyn Any + Send + Sync> {
        Box::new(outlines_of(scope))
    }

    fn query(
        &self,
        q: i32, r: i32,
        _below: &TileView,
        cell: &(dyn Any + Send + Sync),
        _seed: u64,
    ) -> Option<TileOutput> {
        let outlines = cell.downcast_ref::<Outlines>()?;
        let (wx, wy) = hex_to_world(q, r);
        let rise = outlines.relief(wx, wy);
        if rise <= 0.0 { return None }
        Some(TileOutput { elevation_delta: rise, ..TileOutput::default() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: u64 = 0x9E3779B97F4A7C15;

    /// A range stands at a thrust wedge's structural dips: the forelimb at the
    /// dip it is built from, and the backlimb inside the 10–25° the same
    /// geology states.
    #[test]
    fn range_stands_at_its_structural_dips() {
        assert!((FORELIMB_DIP_TAN - FORELIMB_DIP_DEGREES.to_radians().tan()).abs() < 1e-12);
        let forelimb = (RANGE_RISE * RISE / RANGE_STEEP_HALF_WIDTH).atan().to_degrees();
        let backlimb = (RANGE_RISE * RISE / RANGE_GRADED_HALF_WIDTH).atan().to_degrees();
        assert!((forelimb - FORELIMB_DIP_DEGREES).abs() < 1e-9, "forelimb dips {forelimb}");
        assert!((10.0..=25.0).contains(&backlimb), "backlimb dips {backlimb}, outside 10–25°");
    }

    /// Neighbouring sheets meet at the trough: the steep and graded
    /// half-widths sum to one spacing, the family is a share, a crest sits at
    /// the origin, the frontal sheet's forelimb reaches the ground one steep
    /// half-width toward the front, and the last sheet stands at its partial
    /// share with nothing past it.
    #[test]
    fn sheets_meet_at_the_trough() {
        assert!((RANGE_STEEP_HALF_WIDTH + RANGE_GRADED_HALF_WIDTH - RANGE_SPACING).abs() < 1e-9);
        for across in [0.0, 350.0, 700.0, 1_400.0, 2_100.0, 3_000.0, -3_000.0] {
            let h = range_family(across, WEDGE_SHEETS);
            assert!((0.0..=1.0).contains(&h), "family height {h} at {across}");
        }
        assert!((range_family(0.0, WEDGE_SHEETS) - 1.0).abs() < 1e-9);
        assert!(range_family(RANGE_STEEP_HALF_WIDTH, WEDGE_SHEETS).abs() < 1e-9, "the front is one steep half-width out");
        assert!(range_family(-RANGE_GRADED_HALF_WIDTH, WEDGE_SHEETS).abs() < 1e-9, "the trough is one graded half-width in");
        assert!((range_family(-(WEDGE_SHEETS - 1.0) * RANGE_SPACING, WEDGE_SHEETS) - 1.0).abs() < 1e-9, "the last crest stands full");
        assert!(range_family(-WEDGE_SHEETS * RANGE_SPACING, WEDGE_SHEETS).abs() < 1e-9, "the last backlimb rests on the plateau");
        assert!((range_family(-2.0 * RANGE_SPACING, 2.5) - 0.5).abs() < 1e-9, "a half sheet stands at half height");
        assert!(range_family(-3.0 * RANGE_SPACING, 2.5).abs() < 1e-9, "nothing stands past a partial wedge");
    }

    /// Sheets count up from the front at the thickness-set spacing, tighten
    /// with age to the belt's shortening, hold that spacing past the
    /// shortening reach, and the rim is where the count says.
    #[test]
    fn sheets_tighten_with_age() {
        assert!(sheets_in(0.0).abs() < 1e-12);
        let local = |d: f64| 1.0 / (sheets_in(d + 0.5) - sheets_in(d - 0.5));
        assert!((local(0.5) - RANGE_SPACING).abs() < 1.0, "front spacing {}", local(0.5));
        let aged = RANGE_SPACING * (1.0 - BELT_SHORTENING);
        assert!((local(SHORTENING_REACH + 100.0) - aged).abs() < 1.0);
        let mut last = local(0.5);
        for i in 1..=40 {
            let s = local(i as f64 * SHORTENING_REACH / 20.0);
            assert!(s <= last + 1e-6, "spacing widened to {s} from {last}");
            last = s;
        }
        for sheets in [0.5, 1.0, 2.5, WEDGE_SHEETS] {
            let rim = rim_of(sheets);
            assert!((sheets_in(rim) - sheets).abs() < 1e-9, "rim of {sheets} sheets at {rim} holds {}", sheets_in(rim));
        }
        assert!(rim_of(WEDGE_SHEETS) <= WEDGE_REACH, "the full wedge outreaches WEDGE_REACH");
    }

    /// Every chain node lies within the stated swing of the straight edge it
    /// draws, and the swing is not slack.
    #[test]
    fn chains_stay_near_their_edges() {
        let mut worst: f64 = 0.0;
        for cq in -12..=12 {
            for cr in -12..=12 {
                for e in edges_of((cq, cr), S) {
                    let line = Segment::along((e.x0, e.y0), (e.x1, e.y1), true);
                    for n in &e.chain {
                        let (x, y) = node_world(*n);
                        worst = worst.max(line.distance(x, y).0);
                    }
                }
            }
        }
        assert!(worst <= PATH_SWING, "a chain node {worst:.0} off its edge, past PATH_SWING");
        assert!(worst > 0.5 * PATH_SWING, "PATH_SWING is slack for the edges: the farthest node is {worst:.0}");
    }

    /// Every position stands in exactly the plate whose outline encloses it,
    /// found from the nearest seed's plate or one of its neighbours.
    #[test]
    fn every_position_stands_in_one_plate() {
        let outlines = Outlines::in_box(0.0, 0.0, 20_000.0, S);
        let mut swapped = 0;
        for i in 0..80 {
            for j in 0..80 {
                let (x, y) = (i as f64 * 500.0 - 20_000.0, j as f64 * 500.0 - 20_000.0);
                let (plate, _) = outlines.at(x, y).expect("a position with no plate");
                if plate.id != plate_at(x, y, S).id { swapped += 1 }
                let inside: Vec<PlateId> = outlines.plates().filter(|p| p.distances(x, y).1).map(|p| p.id).collect();
                assert_eq!(inside.len(), 1, "position ({x}, {y}) inside {inside:?}");
                assert_eq!(inside[0], plate.id);
            }
        }
        assert!(swapped > 0, "no position stood across a chain from its seed's plate");
    }
}
