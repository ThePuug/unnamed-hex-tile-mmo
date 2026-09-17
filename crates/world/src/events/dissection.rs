//! DissectionEvent — the valleys rivers cut along the channels drainage
//! publishes.
//!
//! # Claims
//!
//! Rivers cut. Dissection removes ground along the channels drainage
//! publishes, and it is the only layer in the stack that removes any:
//! everything beneath it builds an envelope, and dissection is the
//! difference between that envelope and the land. A channel incises at a
//! rate set by its stream power, so over the life of a landscape a bigger
//! river has cut a deeper valley: depth grows with catchment as its square
//! root, the way a channel's width does. Between channels the ground creeps
//! downhill by diffusion, so a divide is convex, and a wall steepens below
//! the rim and eases again onto the floor. Divides keep the envelope's
//! height: a plain with valleys through it is a dissected plateau, and a
//! belt with a trunk river through it has a gorge.
//!
//! A river cuts toward its base level and never below it: the sea, or the
//! lake it ends in, so flooded ground is never cut. A lake's outlet is its
//! sill, uncut, since a cut there drains the lake the routing filled; the
//! river leaving a lake cuts from the first node downstream. A floor is the
//! envelope less a share of its height above base level, and that share
//! saturates below one, so a graded trunk keeps the fall that keeps it
//! flowing. Along a reach the floor never rises, because the envelope falls
//! downstream and the share grows with the water; a test holds it.
//!
//! A valley reaches half the node spacing to each side, so neighbouring
//! valleys meet at their divide, and where two overlap the deeper stands.
//! Depth alone sets a wall's steepness: gentle on a plain, a gorge through a
//! plateau. A channel begins at its head, where enough ground drains through
//! it for its flow to cut; above the head the ground is unchannelled
//! hillslope, which is what keeps the sheet flow an undissected envelope
//! sheds from printing a valley along every thread.
//!
//! Not dissection's: channels finer than the node spacing, floodplains,
//! meanders, sediment, terraces. Unbuilt.
//!
//! # The window
//!
//! No deform: nothing originates here. `prepare` gathers the drainage cells
//! under the cell and its ring into valleys, and a query reads the cut at
//! its own tile from the envelope beneath it. The cell is drainage's, so the
//! footprint plus one ring is the seven cells drainage routed for the same
//! tiles, and reading them costs no route.

use std::any::Any;

use crate::chains::{Segment, SegmentGrid};
use crate::lattice::{hex_distance, node_tile, NodeKey, NODE_SPACING};
use crate::{hex_to_world, world_to_hex};
use super::drainage::{DrainageCell, DrainageEvent, DrainageIndex, DrainageNode, DRAINAGE_CELL_SCALE};
use super::index::IndexRegistry;
use super::plates::Coasts;
use super::thrusting::Outlines;
use super::{CellScope, TileOutput, TileView, WorldEvent};

// ── The valley ──────────────────────────────────────────────────────────────

/// How far a valley reaches from its channel, in world units: half the node
/// spacing, so neighbouring valleys meet at their divide and never take each
/// other's walls. Structural, not tuned.
pub const VALLEY_HALF_WIDTH: f64 = NODE_SPACING as f64 / 2.0;

/// Catchment, in nodes, at which a channel begins to cut: the channel head,
/// a hillslope's length below the divide.
///
/// EMPIRICAL: read from `drainage_probe::catchment_distribution`, at which
/// about a third of land nodes carry a channel and the rest are hillslope.
pub const CHANNEL_HEAD: f64 = 4.0;

/// Catchment, in nodes, at which a channel has cut its full share.
///
/// EMPIRICAL: the 99th percentile of catchment in the same census, so a
/// trunk is full and everything else is partial.
pub const CATCHMENT_FULL: f64 = 48.0;

/// The share of its height above base level a full trunk has removed: the
/// Grand Canyon's 1.6 km cut through a 2.3 km plateau. The rest is the fall
/// that keeps the river flowing.
pub const RELIEF_SHARE_MAX: f64 = 0.7;

/// The share of the height above base level a channel of `catchment` nodes
/// has cut: nothing below the channel head, then growing as the square root
/// of the catchment past it, the way a channel's width does, to the full
/// share.
pub fn relief_share(catchment: f64) -> f64 {
    if catchment <= CHANNEL_HEAD {
        return 0.0;
    }
    let u = ((catchment - CHANNEL_HEAD) / (CATCHMENT_FULL - CHANNEL_HEAD)).min(1.0);
    RELIEF_SHARE_MAX * u.sqrt()
}

/// A valley's cross-section at a share `u` of the half-width from its
/// channel: one at the channel, nothing at the divide, level at both and
/// steepest between, the foot incision leaves and the rim diffusion rounds.
pub fn profile(u: f64) -> f64 {
    let u = u.clamp(0.0, 1.0);
    1.0 - u * u * (3.0 - 2.0 * u)
}

/// The depth a channel has cut at a node: its share of the node's height
/// above base level. Nothing on flooded ground, which lies below its base,
/// and nothing at a lake's sill, which holds the lake's surface: the river
/// leaving a lake cuts from the first node downstream.
pub fn depth_at(node: &DrainageNode) -> f64 {
    if node.sill {
        return 0.0;
    }
    (node.elevation - node.base).max(0.0) * relief_share(node.catchment)
}

/// What one channel segment cuts, at each end.
#[derive(Clone, Copy, Debug)]
struct Cut {
    depth0: f64,
    depth1: f64,
    base0: f64,
    base1: f64,
}

/// The valleys a set of tiles can lie in: every channel segment of the
/// drainage cells in reach, bucketed for the search, with what each cuts.
pub struct Valleys {
    grid: SegmentGrid,
    cuts: Vec<Cut>,
}

impl Valleys {
    /// Valleys from published drainage cells, keeping the segments with an
    /// end `keep` accepts. Nodes are looked up across every cell given,
    /// since a reach's downstream link may name a node the next cell owns; a
    /// link to no published node, the sea or the window's edge, ends the
    /// valley at the last node, where a river to the sea is at the shore.
    pub fn new(cells: &[&DrainageCell], keep: impl Fn(NodeKey) -> bool) -> Self {
        let node = |key: NodeKey| cells.iter().find_map(|c| c.nodes.get(&key));
        let mut segments = Vec::new();
        let mut cuts = Vec::new();
        for c in cells {
            for reach in &c.reaches {
                let mut prev: Option<&DrainageNode> = None;
                for key in reach.nodes.iter().copied().chain(reach.joins) {
                    let Some(n) = node(key) else { break };
                    if let Some(p) = prev {
                        if keep(p.key) || keep(n.key) {
                            segments.push(Segment::along((p.wx, p.wy), (n.wx, n.wy), true));
                            cuts.push(Cut { depth0: depth_at(p), depth1: depth_at(n), base0: p.base, base1: n.base });
                        }
                    }
                    prev = Some(n);
                }
            }
        }
        Self { grid: SegmentGrid::new(segments, VALLEY_HALF_WIDTH), cuts }
    }

    /// The valleys under a square box, routed from the plate graph directly:
    /// what a view or a probe builds once. Routes every drainage cell within
    /// the box's reach, so it costs a window's routing per cell.
    pub fn in_box(cx: f64, cy: f64, half: f64, seed: u64) -> Self {
        let lattice = DrainageIndex::lattice();
        let (q, r) = world_to_hex(cx, cy);
        let centre = lattice.cell_id(q, r);
        let rings = ((half * std::f64::consts::SQRT_2 + VALLEY_HALF_WIDTH) / lattice.radius as f64).ceil() as u32 + 1;
        let window = (3 * lattice.radius + 1) as f64;
        let event = DrainageEvent::new();
        let cells: Vec<DrainageCell> = lattice
            .cells_within_distance(centre, rings)
            .into_iter()
            .map(|cell| {
                let (cq, cr) = lattice.cell_center(cell);
                let (x, y) = hex_to_world(cq, cr);
                let coasts = Coasts::in_box(x, y, window, seed);
                let outlines = Outlines::in_box(x, y, window, seed);
                event.route(&lattice, cell, seed, &coasts, &outlines).owned_cell()
            })
            .collect();
        let refs: Vec<&DrainageCell> = cells.iter().collect();
        Self::new(&refs, |_| true)
    }

    pub fn is_empty(&self) -> bool {
        self.grid.is_empty()
    }

    /// The cut at a position whose envelope is `envelope`, in z-levels: the
    /// deepest valley in reach, each interpolated along its segment and
    /// profiled across it, never below the base level it drains to.
    pub fn cut_at(&self, wx: f64, wy: f64, envelope: f64) -> f64 {
        let mut cut = 0.0f64;
        self.grid.for_each_within(wx, wy, VALLEY_HALF_WIDTH, |i, d| {
            let (t, _) = self.grid.segments()[i].project(wx, wy);
            let c = &self.cuts[i];
            let depth = c.depth0 + t * (c.depth1 - c.depth0);
            let base = c.base0 + t * (c.base1 - c.base0);
            let v = (depth * profile(d / VALLEY_HALF_WIDTH)).min((envelope - base).max(0.0));
            cut = cut.max(v);
        });
        cut
    }
}

/// The valleys a cell's tiles can lie in: the drainage cells under the cell
/// and its ring, keeping the segments within a valley's reach of the cell's
/// ground.
pub fn valleys_of(scope: &CellScope) -> Valleys {
    let cells = scope.source_cells::<DrainageIndex>();
    let Some(idx) = scope.read::<DrainageIndex>() else {
        return Valleys::new(&[], |_| true);
    };
    let centre = scope.lattice().cell_center(scope.cell());
    let keep_within = scope.lattice().radius as i32 + NODE_SPACING + VALLEY_HALF_WIDTH.ceil() as i32;
    Valleys::new(&idx.cells_in(&cells), |key| hex_distance(node_tile(key), centre) <= keep_within)
}

// ── The event ───────────────────────────────────────────────────────────────

pub struct DissectionEvent;

impl DissectionEvent {
    pub fn new() -> Self { DissectionEvent }
}

impl Default for DissectionEvent {
    fn default() -> Self { Self::new() }
}

impl WorldEvent for DissectionEvent {
    fn name(&self) -> &str { "dissection" }
    fn scale(&self) -> u32 { DRAINAGE_CELL_SCALE }

    /// Nothing originates here.
    fn max_influence(&self) -> u32 { 0 }

    fn register_indexes(&self, _registry: &mut IndexRegistry) {}

    /// Nothing to place: the valleys are read off the drainage index.
    fn deform(&self, _scope: &CellScope) {}

    fn prepare(&self, scope: &CellScope) -> Box<dyn Any + Send + Sync> {
        Box::new(valleys_of(scope))
    }

    fn query(
        &self,
        q: i32, r: i32,
        below: &TileView,
        cell: &(dyn Any + Send + Sync),
        _seed: u64,
    ) -> Option<TileOutput> {
        let valleys = cell.downcast_ref::<Valleys>()?;
        let (wx, wy) = hex_to_world(q, r);
        let cut = valleys.cut_at(wx, wy, below.elevation);
        if cut <= 0.0 {
            return None;
        }
        Some(TileOutput { elevation_delta: -cut, ..TileOutput::default() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lattice::node_world;

    const S: u64 = 0x9E3779B97F4A7C15;

    /// The profile is level at the channel and the divide, falls between,
    /// and is never steeper than one and a half depths per half-width.
    #[test]
    fn profile_is_level_at_channel_and_divide() {
        assert_eq!(profile(0.0), 1.0);
        assert_eq!(profile(1.0), 0.0);
        let mut last = 1.0;
        for i in 1..=100 {
            let u = i as f64 / 100.0;
            let p = profile(u);
            assert!(p <= last, "profile rises at {u}");
            assert!(last - p <= 1.5 * 0.01 + 1e-9, "profile steeper than its middle at {u}");
            last = p;
        }
    }

    /// The share starts at the head, grows with catchment, and saturates.
    #[test]
    fn share_starts_at_the_head_and_saturates() {
        assert_eq!(relief_share(CHANNEL_HEAD), 0.0);
        assert_eq!(relief_share(1.0), 0.0);
        let mut last = 0.0;
        for i in 0..200 {
            let a = CHANNEL_HEAD + i as f64 * 0.5;
            let g = relief_share(a);
            assert!(g >= last && g <= RELIEF_SHARE_MAX, "share {g} at {a}");
            last = g;
        }
        assert!((relief_share(CATCHMENT_FULL) - RELIEF_SHARE_MAX).abs() < 1e-12);
        assert_eq!(relief_share(10.0 * CATCHMENT_FULL), RELIEF_SHARE_MAX);
    }

    /// On a routed cell, the floor along every reach never rises, flooded
    /// ground is never cut, and no cut at a tile exceeds the envelope's
    /// height above sea level.
    #[test]
    fn floors_never_rise_and_lakes_are_uncut() {
        let lattice = DrainageIndex::lattice();
        let cell = lattice.cell_id(-58_204, 4_907);
        let (cq, cr) = lattice.cell_center(cell);
        let (cx, cy) = hex_to_world(cq, cr);
        let window = (3 * lattice.radius + 1) as f64;
        let coasts = Coasts::in_box(cx, cy, window, S);
        let outlines = Outlines::in_box(cx, cy, window, S);
        let published = DrainageEvent::new().route(&lattice, cell, S, &coasts, &outlines).owned_cell();
        assert!(!published.reaches.is_empty());
        let mut cut_nodes = 0;
        for reach in &published.reaches {
            let mut last = f64::MAX;
            for key in &reach.nodes {
                let n = &published.nodes[key];
                let floor = n.elevation - depth_at(n);
                assert!(floor >= n.base - 1e-9, "a floor below base level at {key:?}");
                assert!(floor <= last + 1e-9, "a floor rising downstream at {key:?}");
                last = floor;
                if depth_at(n) > 0.0 { cut_nodes += 1 }
            }
        }
        assert!(cut_nodes > 0, "no node in the spawn cell carries a channel");
        for n in published.nodes.values() {
            if n.lake.is_some() {
                assert_eq!(depth_at(n), 0.0, "a flooded node cut at {:?}", n.key);
            }
        }
        let mut sills = 0;
        for lake in &published.lakes {
            let Some(outlet) = lake.outlet.and_then(|o| published.nodes.get(&o)) else { continue };
            sills += 1;
            assert!(outlet.sill, "a lake's outlet not marked as its sill at {:?}", outlet.key);
            assert_eq!(depth_at(outlet), 0.0, "a sill cut at {:?}", outlet.key);
            assert!(outlet.elevation >= lake.surface - 1e-9, "a sill below its lake's surface at {:?}", outlet.key);
            let next = outlet.down.and_then(|d| published.nodes.get(&d));
            if let Some(next) = next {
                assert!(next.elevation - depth_at(next) <= outlet.elevation + 1e-9, "a floor rising past a sill at {:?}", next.key);
            }
        }
        assert!(sills > 0, "no lake in the spawn cell drains through an outlet the cell owns");
        let valleys = Valleys::new(&[&published], |_| true);
        for n in published.nodes.values() {
            let (x, y) = node_world(n.key);
            let cut = valleys.cut_at(x + 100.0, y + 60.0, n.elevation);
            assert!(cut >= 0.0 && cut <= n.elevation.max(0.0) + 1e-9, "cut {cut} at {:?}", n.key);
        }
    }
}
