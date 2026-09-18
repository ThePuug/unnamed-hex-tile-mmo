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
//! lake it ends in, so flooded ground is never cut. A lake's sill is cut by
//! exactly what drainage published, the sill's height less the lake's
//! surface, since the sill holds the surface; along the breach past it the
//! floor is never above the cut sill, so the outlet is a gorge through the
//! rim until the valley's own floor is lower. The river across a lake, the
//! flood's path from its deepest node to its sill, is a reach, and a breach
//! cuts to the straight floor between its nodes, so the lake's floor, the
//! throat to its sill and the gorge beyond are cut through any ridge the
//! lattice did not sample; every other valley interpolates its depth and
//! such a ridge stands. A floor is the envelope less a share of its height
//! above base level, and that share saturates below one, so a graded trunk
//! keeps the fall that keeps it flowing. The share grows with the plate's
//! age too, from a young orogen's narrow cut to an old one's: age is how
//! much of the difference between envelope and land has happened. Along a
//! reach the floor never rises, because the envelope falls downstream and
//! the share grows with the water; a test holds it.
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
//! ground lies below it, a lake's surface over the lake's extent, the ground
//! drainage found its water reaches on the fine lattice, or, for a lake
//! never read there, within one node spacing of its flooded nodes and not
//! past its sill, wherever the ground lies below that surface; the lake's
//! surface too over the throat to its sill, where the throat's cut brought
//! the ground under it; and in a channel the valley floor the channel is
//! cut into. The highest stands. A tile with no surface over it is dry
//! ground.
//!
//! # The window
//!
//! No deform: nothing originates here. `prepare` gathers the drainage cells
//! under the cell and its ring into valleys, and a query reads the cut at
//! its own tile from the envelope beneath it. The cell is drainage's, so the
//! footprint plus one ring is the seven cells drainage routed for the same
//! tiles, and reading them costs no route.

use std::any::Any;
use std::collections::{HashMap, HashSet};

use crate::chains::{Segment, SegmentGrid};
use crate::lattice::{hex_distance, nearest_node, node_tile, NodeKey, DIRECTIONS, NODE_SPACING};
use crate::{hex_to_world, world_to_hex};
use super::drainage::{growth, nearest_fine, DrainageCell, DrainageEvent, DrainageIndex, DrainageNode, DRAINAGE_CELL_SCALE};
pub use super::drainage::{CATCHMENT_FULL, CHANNEL_HEAD};
use super::index::IndexRegistry;
use super::plates::Coasts;
use super::thrusting::Outlines;
use super::{CellScope, TileOutput, TileView, WorldEvent};

// ── The valley ──────────────────────────────────────────────────────────────

/// How far a valley reaches from its channel, in world units: half the node
/// spacing, so neighbouring valleys meet at their divide and never take each
/// other's walls. Structural, not tuned.
pub const VALLEY_HALF_WIDTH: f64 = NODE_SPACING as f64 / 2.0;

/// The share of its height above base level a full trunk on a fully aged
/// plate has removed: the Grand Canyon's 1.6 km cut through a 2.3 km
/// plateau. The rest is the fall that keeps the river flowing.
pub const RELIEF_SHARE_MAX: f64 = 0.7;

/// What a new plate's rivers have cut as a share of an aged plate's: the
/// narrow cut of a young orogen, its plateau surface largely intact.
///
/// Tuning, not yet judged in the viewer.
pub const YOUNG_SHARE: f64 = 0.5;

/// The share of the height above base level a channel of `catchment` nodes
/// on a plate of `age` has cut: nothing below the channel head, then growing
/// as the square root of the catchment past it, the way a channel's width
/// does, to the full share; and with age, from a young plate's share of it
/// to the whole.
pub fn relief_share(catchment: f64, age: f64) -> f64 {
    let aged = YOUNG_SHARE + (1.0 - YOUNG_SHARE) * age.clamp(0.0, 1.0);
    growth(catchment).map_or(0.0, |g| RELIEF_SHARE_MAX * g * aged)
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
/// the water, except at a hump the river across the lake has cut, which
/// keeps the river's slot so the lake is one water through it; otherwise
/// from the head's depth to a trunk's as the catchment grows. Cut below
/// base level too: a channel reaching the sea is under it.
pub fn channel_depth(node: &DrainageNode) -> f64 {
    if node.lake.is_some() && node.cut <= 0.0 {
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
/// above base level. Nothing on flooded ground, which lies below its base.
/// Where drainage published a cut, at a lake's sill and along the breach
/// the river leaving it has made, exactly that cut: the floor is the ground
/// the routing ran over, the sill holding the lake's surface and the breach
/// grading down from it, so the floor never rises along it however the
/// envelope humps beneath.
pub fn depth_at(node: &DrainageNode) -> f64 {
    if node.sill || node.cut > 0.0 {
        return node.cut;
    }
    if node.lake.is_some() {
        return 0.0;
    }
    (node.elevation - node.base).max(0.0) * relief_share(node.catchment, node.age)
}

/// What one channel segment cuts, at each end: the envelope and the valley
/// floor, the base level, and the channel slot's half-width and depth.
#[derive(Clone, Copy, Debug)]
struct Cut {
    env0: f64,
    env1: f64,
    floor0: f64,
    floor1: f64,
    /// The floor runs straight between the ends and the envelope is cut
    /// down to it: a breach, at a sill or along the river leaving one, and
    /// the river across a lake cut through a ridge the lattice did not
    /// sample. Elsewhere, a river entering a lake included, the depth is
    /// interpolated and an unsampled ridge stands.
    graded: bool,
    /// The lake surface the segment holds when it is the river across a lake
    /// or the throat to its sill: the water in it is the lake's, to the lip.
    pool: Option<f64>,
    base0: f64,
    base1: f64,
    half0: f64,
    half1: f64,
    chan0: f64,
    chan1: f64,
}

impl Cut {
    /// The valley's depth below `envelope` at `t` along the segment, and
    /// the base level, channel half-width and channel depth there.
    fn at(&self, t: f64, envelope: f64) -> (f64, f64, f64, f64) {
        let lerp = |a: f64, b: f64| a + t * (b - a);
        let depth = if self.graded {
            (envelope - lerp(self.floor0, self.floor1)).max(0.0)
        } else {
            lerp(self.env0 - self.floor0, self.env1 - self.floor1)
        };
        (depth, lerp(self.base0, self.base1), lerp(self.half0, self.half1), lerp(self.chan0, self.chan1))
    }
}

/// The two cuts at a position: the valley's, and the channel slot's below
/// the valley floor. The channel is where the water stands: a river's
/// surface is the floor, the ground plus the channel cut. In the throat
/// between a lake and its sill the water is the lake's, standing at the
/// sill's floor over the whole slot: the pool.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Cuts {
    pub valley: f64,
    pub channel: f64,
    pub pool: Option<f64>,
}

/// A lake as the flood reads it: its surface; its extent on the fine
/// lattice when drainage read it there, the points its water stands over,
/// which the flood holds to; else where its water ends, its sill, since
/// ground beyond the sill along the line out from the flooded node holds
/// the river's water and not the lake's.
#[derive(Clone, Debug)]
struct Shore {
    surface: f64,
    extent: Option<HashSet<NodeKey>>,
    ends: Option<(f64, f64)>,
}

/// The valleys a set of tiles can lie in: every channel segment of the
/// drainage cells in reach, bucketed for the search, with what each cuts;
/// and every flooded node in reach with its lake.
pub struct Valleys {
    grid: SegmentGrid,
    cuts: Vec<Cut>,
    flooded: HashMap<NodeKey, usize>,
    shores: Vec<Shore>,
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
                                env0: p.elevation,
                                env1: n.elevation,
                                floor0: p.elevation - depth_at(p),
                                floor1: n.elevation - depth_at(n),
                                graded: p.sill || n.sill || p.cut > 0.0 || n.cut > 0.0 || (p.lake.is_some() && n.lake.is_some()),
                                pool: (p.lake.is_some() && (n.lake.is_some() || n.sill)).then(|| p.surface),
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
        let mut flooded = HashMap::new();
        let mut shores = Vec::new();
        for lake in cells.iter().flat_map(|c| c.lakes.iter()) {
            let ends = lake.outlet.and_then(node).map(|s| (s.wx, s.wy));
            let extent = (!lake.extent.is_empty()).then(|| lake.extent.iter().copied().collect());
            flooded.extend(lake.nodes.iter().map(|&k| (k, shores.len())));
            shores.push(Shore { surface: lake.surface, extent, ends });
        }
        Self { grid: SegmentGrid::new(segments, VALLEY_HALF_WIDTH), cuts, flooded, shores }
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
    /// breach cuts to the straight floor between its nodes, through any
    /// ridge between them; any other valley interpolates its depth. A
    /// valley never cuts below the base level it drains to; its channel, a
    /// slot of the segment's half-width and depth, cuts on below it. A
    /// throat in reach pools the lake's water to its sill's floor.
    pub fn cuts_at(&self, wx: f64, wy: f64, envelope: f64) -> Cuts {
        let mut best = Cuts::default();
        let mut pool: Option<f64> = None;
        self.grid.for_each_within(wx, wy, VALLEY_HALF_WIDTH, |i, d| {
            let (t, _) = self.grid.segments()[i].project(wx, wy);
            let (depth, base, half, chan) = self.cuts[i].at(t, envelope);
            let valley = (depth * profile(d / VALLEY_HALF_WIDTH)).min((envelope - base).max(0.0));
            let channel = if d <= half { chan } else { 0.0 };
            if valley + channel > best.valley + best.channel {
                best = Cuts { valley, channel, pool: None };
            }
            // The pool holds over the throat itself, not past the lip, and
            // only where the throat's cut brought the ground under it: low
            // ground beside it is the lake's if its extent says so.
            if let Some(p) = self.cuts[i].pool.filter(|&p| (0.0..1.0).contains(&t) && envelope >= p) {
                pool = Some(pool.map_or(p, |q| q.max(p)));
            }
        });
        best.pool = pool;
        best
    }

    /// The whole cut at a position: valley and channel together.
    pub fn cut_at(&self, wx: f64, wy: f64, envelope: f64) -> f64 {
        let c = self.cuts_at(wx, wy, envelope);
        c.valley + c.channel
    }

    /// The surface water stands at over a position whose ground, after the
    /// cuts, is `ground`: the highest of the sea, a lake over the position,
    /// the lake's pool in the throat to its sill, and the floor of the
    /// channel the position lies in. None where none stands above the
    /// ground. A lake stands over a position within one node spacing of a
    /// flooded node that lies in the lake's extent, read on the fine
    /// lattice, or, for a lake never read there, that is not past its sill
    /// on the line out from the flooded node: the lake's plane would
    /// otherwise hang over the gorge the river leaves by.
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
        if let Some(pool) = cuts.pool {
            stand(pool);
        }
        // Every node within one spacing of a position is the nearest node
        // or one of its six neighbours: the nearest lies within the
        // lattice's covering radius, a spacing over √3, and the second ring
        // starts at √3 spacings.
        let n = nearest_node(wx, wy);
        let reach = NODE_SPACING as f64;
        for key in std::iter::once(n).chain(DIRECTIONS.iter().map(|(di, dj)| (n.0 + di, n.1 + dj))) {
            let Some(&shore) = self.flooded.get(&key) else { continue };
            let (x, y) = crate::lattice::node_world(key);
            let from_flooded = (x - wx).hypot(y - wy);
            if from_flooded > reach {
                continue;
            }
            let Shore { surface: s, extent, ends } = &self.shores[shore];
            let held = match extent {
                Some(extent) => extent.contains(&nearest_fine(wx, wy)),
                None => ends.map_or(true, |(px, py)| (wx - px) * (px - x) + (wy - py) * (py - y) <= 0.0),
            };
            if held {
                stand(*s);
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
            direction: (1.0, 0.0), catchment, base: 0.0, down: None, lake, sill: false, age: 1.0, cut: 0.0,
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
        assert_eq!(relief_share(CHANNEL_HEAD, 1.0), 0.0);
        assert_eq!(relief_share(1.0, 1.0), 0.0);
        let mut last = 0.0;
        for i in 0..200 {
            let a = CHANNEL_HEAD + i as f64 * 0.5;
            let g = relief_share(a, 1.0);
            assert!(g >= last && g <= RELIEF_SHARE_MAX, "share {g} at {a}");
            last = g;
        }
        assert!((relief_share(CATCHMENT_FULL, 1.0) - RELIEF_SHARE_MAX).abs() < 1e-12);
        assert_eq!(relief_share(10.0 * CATCHMENT_FULL, 1.0), RELIEF_SHARE_MAX);
    }

    /// The share grows with the plate's age, from a young plate's share of
    /// the full cut to the whole of it, at every catchment past the head.
    #[test]
    fn share_grows_with_age() {
        for c in [CHANNEL_HEAD + 1.0, CATCHMENT_FULL / 2.0, CATCHMENT_FULL] {
            let (young, old) = (relief_share(c, 0.0), relief_share(c, 1.0));
            assert!(young > 0.0 && young < old, "share {young} young, {old} old at {c}");
            assert!((young - YOUNG_SHARE * old).abs() < 1e-12);
            let mut last = young;
            for i in 1..=10 {
                let s = relief_share(c, i as f64 / 10.0);
                assert!(s >= last, "share falls with age at {c}");
                last = s;
            }
        }
    }

    /// On a routed cell, the floor along every reach never rises past the
    /// lake it leaves, flooded
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
                if n.lake.is_some() {
                    continue;
                }
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
                assert_eq!(depth_at(n), n.cut, "a flooded node cut past its hump at {:?}", n.key);
                if n.cut > 0.0 {
                    let surface = published.lakes[n.lake.unwrap()].surface;
                    assert!(n.elevation - n.cut <= surface + 1e-9, "a flooded node cut above its surface at {:?}", n.key);
                }
            }
        }
        let (mut sills, mut cut_sills) = (0, 0);
        for lake in &published.lakes {
            let Some(outlet) = lake.outlet.and_then(|o| published.nodes.get(&o)) else { continue };
            sills += 1;
            assert!(outlet.sill, "a lake's outlet not marked as its sill at {:?}", outlet.key);
            assert_eq!(depth_at(outlet), outlet.cut, "a sill cut past what drainage published at {:?}", outlet.key);
            let floor = outlet.elevation - depth_at(outlet);
            assert!((floor - lake.surface).abs() < 1e-9, "a sill's floor {floor} off its lake's surface {} at {:?}", lake.surface, outlet.key);
            if outlet.cut > 0.0 { cut_sills += 1 }
            let mut last = floor;
            let mut next = outlet.down.and_then(|d| published.nodes.get(&d));
            while let Some(n) = next {
                if n.lake.is_some() { break }
                let f = n.elevation - depth_at(n);
                assert!(f <= last + 1e-9, "a floor rising past a sill at {:?}", n.key);
                if n.cut == 0.0 { break }
                last = f;
                next = n.down.and_then(|d| published.nodes.get(&d));
            }
        }
        assert!(sills > 0, "no lake in the spawn cell drains through an outlet the cell owns");
        assert!(cut_sills > 0, "no sill in the spawn cell is cut");
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
            assert!(n.lake.is_some() || at.valley >= depth_at(n) - 1e-9, "a shallower valley than the node's own at {:?}", n.key);
            let (dx, dy) = (-n.direction.1, n.direction.0);
            let off = channel_half_width(n.catchment) + 1.0;
            let beside = valleys.cuts_at(x + dx * off, y + dy * off, n.elevation);
            // Beside the node the slot is the segment's, interpolated a hair
            // short of the node, so the slack is interpolation's.
            assert!(beside.channel == 0.0 || beside.channel < at.channel + 1e-6, "a channel beside the channel at {:?}", n.key);
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
                // A node flooded by a hair reads dry, as the spec allows.
                if published.lakes[lake].surface - n.elevation > 1e-6 {
                    assert_eq!(water, Some(published.lakes[lake].surface), "a flooded node not under its lake at {:?}", n.key);
                }
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
