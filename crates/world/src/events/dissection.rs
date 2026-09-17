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
//! In the floor runs the channel, a slot as wide and as deep as its
//! catchment says: a tile wide and a z-level deep at the channel head,
//! several of each at a trunk, growing as the square root of the catchment
//! the way a channel's width and depth grow with discharge. Water stands in
//! it to the floor, so the slot is what keeps a river below its banks. The
//! slot is cut below base level too, where the valley stops: a channel
//! reaching the sea runs under it, and one entering a lake shallows to
//! nothing at the flooded node. A river ends at its reach's last land node,
//! so a channel stops short of the shore by up to one spacing. Unbuilt.
//!
//! Not dissection's: channels finer than the node spacing, floodplains,
//! meanders, sediment, terraces. Unbuilt.
//!
//! # Water
//!
//! Dissection also publishes the surface water stands at over each tile,
//! since it alone knows the floor it left: the sea at zero wherever the
//! ground lies below it, a lake's surface within one node spacing of the
//! lake's flooded nodes wherever the ground lies below that, and in a
//! channel the valley floor the channel is cut into. The highest stands. A
//! tile with no surface over it is dry ground.
//!
//! # The window
//!
//! No deform: nothing originates here. `prepare` gathers the drainage cells
//! under the cell and its ring into valleys, and a query reads the cut at
//! its own tile from the envelope beneath it. The cell is drainage's, so the
//! footprint plus one ring is the seven cells drainage routed for the same
//! tiles, and reading them costs no route.

use std::any::Any;
use std::collections::HashMap;

use crate::chains::{Segment, SegmentGrid};
use crate::lattice::{hex_distance, nearest_node, node_tile, NodeKey, DIRECTIONS, NODE_SPACING};
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

/// How far past the channel head a catchment of `catchment` nodes has grown,
/// as the square root of its share of the way to a full trunk: the way a
/// channel's width and depth grow with discharge. None below the head.
fn growth(catchment: f64) -> Option<f64> {
    if catchment <= CHANNEL_HEAD {
        return None;
    }
    Some(((catchment - CHANNEL_HEAD) / (CATCHMENT_FULL - CHANNEL_HEAD)).min(1.0).sqrt())
}

/// The share of the height above base level a channel of `catchment` nodes
/// has cut: nothing below the channel head, then growing as the square root
/// of the catchment past it, the way a channel's width does, to the full
/// share.
pub fn relief_share(catchment: f64) -> f64 {
    growth(catchment).map_or(0.0, |g| RELIEF_SHARE_MAX * g)
}

// ── The channel ─────────────────────────────────────────────────────────────

/// Half-width of the channel at the channel head, in tiles. A straight line
/// can pass 1/√3 of a tile from every tile centre, the hex lattice's covering
/// radius, so a narrower strip leaves gaps in a stream.
pub const CHANNEL_HALF_WIDTH_MIN: f64 = 0.6;

/// Half-width of a full trunk's channel, in tiles.
pub const CHANNEL_HALF_WIDTH_MAX: f64 = 3.5;

/// Depth of the channel at the channel head, in z-levels: the one step a
/// walker wades.
pub const CHANNEL_DEPTH_MIN: f64 = 1.0;

/// Depth of a full trunk's channel, in z-levels.
pub const CHANNEL_DEPTH_MAX: f64 = 3.0;

/// The channel's half-width at `catchment` nodes: nothing below the channel
/// head, then from the head's width to a trunk's as the catchment grows.
pub fn channel_half_width(catchment: f64) -> f64 {
    growth(catchment).map_or(0.0, |g| CHANNEL_HALF_WIDTH_MIN + (CHANNEL_HALF_WIDTH_MAX - CHANNEL_HALF_WIDTH_MIN) * g)
}

/// The depth of the channel slot below the valley floor at a node: nothing
/// below the channel head and nothing on flooded ground, where the lake is
/// the water; otherwise from the head's depth to a trunk's as the catchment
/// grows. Cut below base level too: a channel reaching the sea is under it.
pub fn channel_depth(node: &DrainageNode) -> f64 {
    if node.lake.is_some() {
        return 0.0;
    }
    growth(node.catchment).map_or(0.0, |g| CHANNEL_DEPTH_MIN + (CHANNEL_DEPTH_MAX - CHANNEL_DEPTH_MIN) * g)
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

/// What one channel segment cuts, at each end: the valley's depth and base
/// level, and the channel slot's half-width and depth.
#[derive(Clone, Copy, Debug)]
struct Cut {
    depth0: f64,
    depth1: f64,
    base0: f64,
    base1: f64,
    half0: f64,
    half1: f64,
    chan0: f64,
    chan1: f64,
}

impl Cut {
    fn at(&self, t: f64) -> (f64, f64, f64, f64) {
        let lerp = |a: f64, b: f64| a + t * (b - a);
        (lerp(self.depth0, self.depth1), lerp(self.base0, self.base1), lerp(self.half0, self.half1), lerp(self.chan0, self.chan1))
    }
}

/// The two cuts at a position: the valley's, and the channel slot's below
/// the valley floor. The channel is where the water stands: a river's
/// surface is the floor, the ground plus the channel cut.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Cuts {
    pub valley: f64,
    pub channel: f64,
}

/// The valleys a set of tiles can lie in: every channel segment of the
/// drainage cells in reach, bucketed for the search, with what each cuts;
/// and every flooded node in reach with its lake's surface.
pub struct Valleys {
    grid: SegmentGrid,
    cuts: Vec<Cut>,
    flooded: HashMap<NodeKey, f64>,
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
                            cuts.push(Cut {
                                depth0: depth_at(p),
                                depth1: depth_at(n),
                                base0: p.base,
                                base1: n.base,
                                half0: channel_half_width(p.catchment),
                                half1: channel_half_width(n.catchment),
                                chan0: channel_depth(p),
                                chan1: channel_depth(n),
                            });
                        }
                    }
                    prev = Some(n);
                }
            }
        }
        let flooded = cells
            .iter()
            .flat_map(|c| c.lakes.iter())
            .flat_map(|l| l.nodes.iter().map(move |&k| (k, l.surface)))
            .collect();
        Self { grid: SegmentGrid::new(segments, VALLEY_HALF_WIDTH), cuts, flooded }
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

    /// The cuts at a position whose envelope is `envelope`, in z-levels: of
    /// every valley in reach, each interpolated along its segment and
    /// profiled across it, the one that with its channel cuts deepest. A
    /// valley never cuts below the base level it drains to; its channel, a
    /// slot of the segment's half-width and depth, cuts on below it.
    pub fn cuts_at(&self, wx: f64, wy: f64, envelope: f64) -> Cuts {
        let mut best = Cuts::default();
        self.grid.for_each_within(wx, wy, VALLEY_HALF_WIDTH, |i, d| {
            let (t, _) = self.grid.segments()[i].project(wx, wy);
            let (depth, base, half, chan) = self.cuts[i].at(t);
            let valley = (depth * profile(d / VALLEY_HALF_WIDTH)).min((envelope - base).max(0.0));
            let channel = if d <= half { chan } else { 0.0 };
            if valley + channel > best.valley + best.channel {
                best = Cuts { valley, channel };
            }
        });
        best
    }

    /// The whole cut at a position: valley and channel together.
    pub fn cut_at(&self, wx: f64, wy: f64, envelope: f64) -> f64 {
        let c = self.cuts_at(wx, wy, envelope);
        c.valley + c.channel
    }

    /// The surface water stands at over a position whose ground, after the
    /// cuts, is `ground`: the highest of the sea, a lake within one node
    /// spacing of a flooded node, and the floor of the channel the position
    /// lies in. None where none stands above the ground.
    pub fn surface_at(&self, wx: f64, wy: f64, ground: f64, cuts: Cuts) -> Option<f64> {
        let mut surface: Option<f64> = None;
        let mut stand = |s: f64| {
            if s > ground {
                surface = Some(surface.map_or(s, |v| v.max(s)));
            }
        };
        stand(0.0);
        if cuts.channel > 0.0 {
            stand(ground + cuts.channel);
        }
        // Every node within one spacing of a position is the nearest node
        // or one of its six neighbours: the nearest lies within the
        // lattice's covering radius, a spacing over √3, and the second ring
        // starts at √3 spacings.
        let n = nearest_node(wx, wy);
        let reach = NODE_SPACING as f64;
        for key in std::iter::once(n).chain(DIRECTIONS.iter().map(|(di, dj)| (n.0 + di, n.1 + dj))) {
            let Some(&s) = self.flooded.get(&key) else { continue };
            let (x, y) = crate::lattice::node_world(key);
            if (x - wx).hypot(y - wy) <= reach {
                stand(s);
            }
        }
        surface
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
        let cuts = valleys.cuts_at(wx, wy, below.elevation);
        let cut = cuts.valley + cuts.channel;
        let water = valleys.surface_at(wx, wy, below.elevation - cut, cuts);
        if cut <= 0.0 && water.is_none() {
            return None;
        }
        Some(TileOutput { elevation_delta: -cut, water, ..TileOutput::default() })
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

    /// The channel is nothing at the head, a tile and a z-level just past
    /// it, grows with catchment, and saturates at a trunk's.
    #[test]
    fn channel_starts_at_the_head_and_saturates() {
        let node = |catchment: f64, lake: Option<usize>| DrainageNode {
            key: (0, 0), q: 0, r: 0, wx: 0.0, wy: 0.0, elevation: 10.0, surface: 10.0,
            direction: (1.0, 0.0), catchment, base: 0.0, down: None, lake, sill: false,
        };
        assert_eq!(channel_half_width(CHANNEL_HEAD), 0.0);
        assert_eq!(channel_depth(&node(CHANNEL_HEAD, None)), 0.0);
        let just_past = CHANNEL_HEAD + 1e-9;
        assert!((channel_half_width(just_past) - CHANNEL_HALF_WIDTH_MIN).abs() < 1e-3);
        assert!((channel_depth(&node(just_past, None)) - CHANNEL_DEPTH_MIN).abs() < 1e-3);
        let (mut last_w, mut last_d) = (0.0, 0.0);
        for i in 0..200 {
            let c = CHANNEL_HEAD + i as f64 * 0.5;
            let (w, d) = (channel_half_width(c), channel_depth(&node(c, None)));
            assert!(w >= last_w && d >= last_d, "channel shrinks at {c}");
            (last_w, last_d) = (w, d);
        }
        assert_eq!(channel_half_width(10.0 * CATCHMENT_FULL), CHANNEL_HALF_WIDTH_MAX);
        assert_eq!(channel_depth(&node(10.0 * CATCHMENT_FULL, None)), CHANNEL_DEPTH_MAX);
        assert_eq!(channel_depth(&node(10.0 * CATCHMENT_FULL, Some(0))), 0.0, "a channel cut into a lake");
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

    /// At a channelled node the slot is cut to the node's channel depth
    /// beneath the valley floor, and past the channel's half-width it is
    /// not; the surface a river stands at, the ground plus the channel cut,
    /// is the valley floor either way.
    #[test]
    fn the_channel_is_a_slot_in_the_valley_floor() {
        let lattice = DrainageIndex::lattice();
        let cell = lattice.cell_id(-58_204, 4_907);
        let (cq, cr) = lattice.cell_center(cell);
        let (cx, cy) = hex_to_world(cq, cr);
        let window = (3 * lattice.radius + 1) as f64;
        let coasts = Coasts::in_box(cx, cy, window, S);
        let outlines = Outlines::in_box(cx, cy, window, S);
        let published = DrainageEvent::new().route(&lattice, cell, S, &coasts, &outlines).owned_cell();
        let valleys = Valleys::new(&[&published], |_| true);
        let mut channelled = 0;
        for n in published.nodes.values() {
            if channel_depth(n) <= 0.0 || n.sill {
                continue;
            }
            channelled += 1;
            let (x, y) = node_world(n.key);
            let at = valleys.cuts_at(x, y, n.elevation);
            assert!((at.channel - channel_depth(n)).abs() < 1e-9, "channel {} for {} at {:?}", at.channel, channel_depth(n), n.key);
            assert!(at.valley >= depth_at(n) - 1e-9, "a shallower valley than the node's own at {:?}", n.key);
            let (dx, dy) = (-n.direction.1, n.direction.0);
            let off = channel_half_width(n.catchment) + 1.0;
            let beside = valleys.cuts_at(x + dx * off, y + dy * off, n.elevation);
            assert!(beside.channel == 0.0 || beside.channel < at.channel + 1e-9, "a channel beside the channel at {:?}", n.key);
        }
        assert!(channelled > 0, "no channelled node in the spawn cell");
    }

    /// Water stands at the valley floor in a channel, at the lake's surface
    /// over a flooded node, at zero over ground below the sea, and nowhere
    /// over dry ground above it.
    #[test]
    fn water_stands_at_the_floor_the_surface_and_the_sea() {
        let lattice = DrainageIndex::lattice();
        let cell = lattice.cell_id(-58_204, 4_907);
        let (cq, cr) = lattice.cell_center(cell);
        let (cx, cy) = hex_to_world(cq, cr);
        let window = (3 * lattice.radius + 1) as f64;
        let coasts = Coasts::in_box(cx, cy, window, S);
        let outlines = Outlines::in_box(cx, cy, window, S);
        let published = DrainageEvent::new().route(&lattice, cell, S, &coasts, &outlines).owned_cell();
        let valleys = Valleys::new(&[&published], |_| true);
        let (mut rivers, mut lakes, mut dry) = (0, 0, 0);
        for n in published.nodes.values() {
            let (x, y) = node_world(n.key);
            let cuts = valleys.cuts_at(x, y, n.elevation);
            let ground = n.elevation - cuts.valley - cuts.channel;
            let water = valleys.surface_at(x, y, ground, cuts);
            if let Some(lake) = n.lake {
                lakes += 1;
                assert_eq!(water, Some(published.lakes[lake].surface), "a flooded node not under its lake at {:?}", n.key);
            } else if cuts.channel > 0.0 {
                rivers += 1;
                let floor = n.elevation - cuts.valley;
                assert!(water.map_or(false, |w| w >= floor - 1e-9), "a channel not under water at {:?}", n.key);
                if water == Some(floor) {
                    assert!((water.unwrap() - ground - cuts.channel).abs() < 1e-9);
                }
            } else if ground >= 0.0 {
                let near_lake = water.map_or(false, |w| w > 0.0);
                if !near_lake {
                    dry += 1;
                    assert_eq!(water, None, "water over dry ground at {:?}", n.key);
                }
            } else {
                assert_eq!(water, Some(0.0), "sea floor not under the sea at {:?}", n.key);
            }
        }
        assert!(rivers > 0 && lakes > 0 && dry > 0, "rivers {rivers}, lakes {lakes}, dry {dry}");
    }
}
