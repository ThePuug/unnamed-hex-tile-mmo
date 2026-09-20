//! DissectionEvent — the valleys rivers cut along the channels migration
//! publishes.
//!
//! # Claims
//!
//! Rivers cut. Dissection removes ground along the channels migration
//! publishes over drainage's reaches, and it is the only layer in the stack
//! that removes any: everything beneath it builds an envelope, and
//! dissection is the difference between that envelope and the land. A
//! channel incises at a rate set by its stream power, so over the life of a
//! landscape a bigger river has cut a deeper valley: depth grows with
//! catchment as its square root, the way a channel's width does. Between
//! channels the ground creeps downhill by diffusion, so a divide is convex,
//! and a wall steepens below the rim and eases again onto the floor.
//! Divides keep the envelope's height: a plain with valleys through it is a
//! dissected plateau, and a belt with a trunk river through it has a gorge.
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
//! such a ridge stands. The floor at a node is what drainage published for
//! it: the envelope less a share of its height above base level, the share
//! saturating below one so a graded trunk keeps the fall that keeps it
//! flowing, and growing with the plate's age from a young orogen's narrow
//! cut to an old one's. Along a reach the floor never rises, because the
//! envelope falls downstream and the share grows with the water; a test
//! holds it.
//!
//! A valley runs along its channel's flow line and reaches half the node
//! spacing to each side, so neighbouring valleys meet at their divide, and
//! where two overlap the deeper stands. Depth alone sets a wall's
//! steepness: gentle on a plain, a gorge through a plateau. A channel
//! begins at its head, where enough ground drains through it for its flow
//! to cut; above the head the ground is unchannelled hillslope, which is
//! what keeps the sheet flow an undissected envelope sheds from printing a
//! valley along every thread.
//!
//! # The channel and its belt
//!
//! In the floor runs the channel, a slot as wide and as deep as its
//! catchment says: a tile wide and a z-level deep at the channel head,
//! several of each at a trunk, growing as the square root of the catchment
//! the way a channel's width and depth grow with discharge. It runs along
//! the train migration published where the river meanders, and along the
//! flow line where it does not. Water stands in it to the floor, so the
//! slot is what keeps a river below its banks. The slot is cut below base
//! level too, where the valley stops: a channel reaching the sea runs
//! under it, and one entering a lake shallows to nothing at the flooded
//! node. A river ends at its reach's last land node, so a channel stops
//! short of the shore by up to one spacing. Unbuilt.
//!
//! Where the river meanders it has swept a belt and planed it: the floor
//! is level across the belt the train reaches over, by the channel's
//! vigour, and the valley's wall climbs from the belt's edge. A tributary's
//! train ends at the trunk's bank where it first crosses the trunk's, which
//! is settled here and not in migration, since the trunk's segment may be
//! another cell's and `prepare` is the phase that sees the ring.
//!
//! Not dissection's: channels finer than the node spacing, sediment,
//! terraces, the bank a bend cuts and the bar it builds. Unbuilt.
//!
//! # Water
//!
//! Dissection also publishes the surface water stands at over each tile,
//! since it alone knows the floor it left: the sea at zero wherever the
//! ground lies below it; a lake's surface over the lake's extent, the fine
//! points drainage found its water reaches, and over the shore between the
//! extent and the ground at the surface, where the envelope stays under
//! the surface from the tile to the extent, so the water meets the land at
//! the surface's own contour and never crosses a rim; for a lake never
//! read on the fine lattice, within one node spacing of its flooded nodes
//! and not past its sill; the lake's surface too over the throat to its
//! sill, where the throat's cut brought the ground under it; and in a
//! channel the valley floor the channel is cut into. The highest stands,
//! and only over ground below it. A tile with no surface over it is dry
//! ground.
//!
//! # The window
//!
//! No deform: nothing originates here. `prepare` gathers the channels
//! migration published under the cell and its ring, with the drainage
//! nodes they run between, into valleys; a query reads the cut at its own
//! tile from the envelope beneath it. The cell is migration's, so the
//! footprint plus one ring is the seven cells whose channels can reach the
//! cell's tiles.

use std::any::Any;
use std::collections::HashMap;

use crate::chains::{Segment, SegmentGrid};
use crate::lattice::{hex_distance, nearest_node, node_tile, NodeKey, DIRECTIONS, NODE_SPACING};
use crate::{hex_to_world, world_to_hex};
use super::drainage::{
    fine_world, growth, nearest_fine, surface_at as envelope_at, DrainageCell, DrainageEvent, DrainageIndex,
    DrainageNode,
};
pub use super::drainage::{CATCHMENT_FULL, CHANNEL_HEAD};
use super::index::IndexRegistry;
use super::migration::{channels, Channel, ChannelIndex, Train, AXIS_SWING, CHANNEL_REACH, MIGRATION_CELL_SCALE};
pub use super::migration::VALLEY_HALF_WIDTH;
use super::plates::{Coasts, PlateEdgeIndex};
use super::thrusting::{outlines_of, Outlines};
use super::{CellScope, TileOutput, TileView, WorldEvent};

// ── The channel ─────────────────────────────────────────────────────────────

/// Depth of the channel at the channel head, in z-levels: the one step a
/// walker wades.
pub const CHANNEL_DEPTH_MIN: f64 = 1.0;

/// Depth of a full trunk's channel, in z-levels.
pub const CHANNEL_DEPTH_MAX: f64 = 3.0;

// ── The shore ───────────────────────────────────────────────────────────────

/// How often the envelope is read along the line from a shore tile to the
/// lake's extent, in tiles. A rim crest is a crease at repose, so a crest
/// the samples straddle stands within half a step of one and a crest under
/// half a step's climb, a level or two, is the most the line can miss.
const SHORE_STEP: f64 = 4.0;

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
    growth(node.catchment, node.erodibility).map_or(0.0, |g| CHANNEL_DEPTH_MIN + (CHANNEL_DEPTH_MAX - CHANNEL_DEPTH_MIN) * g)
}

/// A valley's cross-section at a share `u` of the half-width from its
/// channel through rock of `erodibility`: one at the channel, nothing at
/// the divide, level at both and steepest between, the foot incision
/// leaves and the rim diffusion rounds. In weak rock the wall is the
/// smooth step; in hard rock creep does little against incision, so the
/// drop crowds against the channel and the valley is a gorge under a wide
/// rim: the step raised to the hardness.
pub fn profile(u: f64, erodibility: f64) -> f64 {
    let u = u.clamp(0.0, 1.0);
    (1.0 - u * u * (3.0 - 2.0 * u)).powf(1.0 / erodibility.max(0.05))
}

/// What dissection removes at a node: the envelope less the floor drainage
/// published for it.
pub fn depth_at(node: &DrainageNode) -> f64 {
    (node.elevation - node.floor).max(0.0)
}

/// What one channel segment cuts, at each end: the envelope and the valley
/// floor, the base level, the channel slot's half-width and depth, and
/// how far the river meanders; and the train it meanders on.
#[derive(Clone, Debug)]
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
    erode0: f64,
    erode1: f64,
    /// The flow line's length, what `t` runs along.
    length: f64,
    vigour0: f64,
    vigour1: f64,
    /// The channel's train, or none where the river holds the flow line.
    train: Option<Train>,
}

/// One sub-segment of a flow line: which cut it belongs to and where along
/// the line it starts, so a position projected onto it reads its `t` along
/// the whole line.
#[derive(Clone, Copy, Debug)]
struct Sub {
    cut: usize,
    start: f64,
    length: f64,
}

/// What a cut reads at one point along its segment.
#[derive(Clone, Copy, Debug)]
struct At {
    depth: f64,
    base: f64,
    half: f64,
    chan: f64,
    /// How far the belt the river has swept reaches to each side of the
    /// flow line: the farthest its train strayed, then the channel.
    belt: f64,
    erodibility: f64,
}

impl Cut {
    /// The valley's depth below `envelope` at `t` along the segment, with
    /// the base level, channel half-width, channel depth and belt there.
    /// Where the river meanders, the floor is planed level across its belt
    /// by as much as it does: the depth takes up the envelope's rise over
    /// the line's own, interpolated between the nodes, by the vigour.
    fn at(&self, t: f64, envelope: f64) -> At {
        let lerp = |a: f64, b: f64| a + t * (b - a);
        let vigour = lerp(self.vigour0, self.vigour1);
        let half = lerp(self.half0, self.half1);
        let depth = if self.graded {
            (envelope - lerp(self.floor0, self.floor1)).max(0.0)
        } else {
            lerp(self.env0 - self.floor0, self.env1 - self.floor1) + vigour * (envelope - lerp(self.env0, self.env1))
        };
        let reach = self.train.as_ref().map_or(0.0, |m| m.amplitude);
        At {
            depth,
            base: lerp(self.base0, self.base1),
            half,
            chan: lerp(self.chan0, self.chan1),
            belt: reach + half,
            erodibility: lerp(self.erode0, self.erode1),
        }
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

/// A lake as the flood reads it: its surface; whether drainage read it on
/// the fine lattice, when its water stands over the extent it found there
/// and the shore between; else where its water ends, its sill, since
/// ground beyond the sill along the line out from the flooded node holds
/// the river's water and not the lake's.
#[derive(Clone, Debug)]
struct Shore {
    surface: f64,
    read: bool,
    ends: Option<(f64, f64)>,
}

/// The envelope as drainage reads it, at any position: what a shore is
/// read against between a tile and a lake's extent.
pub struct Envelope {
    seed: u64,
    coasts: Coasts,
    outlines: Outlines,
}

impl Envelope {
    pub fn new(seed: u64, coasts: Coasts, outlines: Outlines) -> Self {
        Envelope { seed, coasts, outlines }
    }

    pub fn at(&self, x: f64, y: f64) -> f64 {
        envelope_at(x, y, self.seed, &self.coasts, &self.outlines)
    }
}

/// The valleys a set of tiles can lie in: every channel in reach along its
/// flow line, bucketed for the search, with what each cuts; every flooded
/// node in reach with its lake, and every point of the fine lattice a
/// lake's water stands over.
pub struct Valleys {
    grid: SegmentGrid,
    subs: Vec<Sub>,
    cuts: Vec<Cut>,
    flooded: HashMap<NodeKey, usize>,
    extent_of: HashMap<NodeKey, usize>,
    shores: Vec<Shore>,
    envelope: Envelope,
}

impl Valleys {
    /// Valleys from published channels and the drainage cells their nodes
    /// are in, keeping the channels with an end `keep` accepts, over the
    /// envelope the cells were routed on. A channel whose node is in no
    /// cell given is left out.
    pub fn new(cells: &[&DrainageCell], channels: &[&Channel], keep: impl Fn(NodeKey) -> bool, envelope: Envelope) -> Self {
        let node = |key: NodeKey| cells.iter().find_map(|c| c.nodes.get(&key));
        let mut segments = Vec::new();
        let mut subs = Vec::new();
        let mut cuts: Vec<Cut> = Vec::new();
        let mut ends: Vec<(NodeKey, NodeKey, f64)> = Vec::new();
        for ch in channels {
            if !(keep(ch.from) || keep(ch.to)) {
                continue;
            }
            let (Some(p), Some(n)) = (node(ch.from), node(ch.to)) else { continue };
            let mut start = 0.0;
            for pair in ch.axis.windows(2) {
                let length = (pair[1].0 - pair[0].0).hypot(pair[1].1 - pair[0].1);
                segments.push(Segment::along(pair[0], pair[1], true));
                subs.push(Sub { cut: cuts.len(), start, length });
                start += length;
            }
            ends.push((ch.from, ch.to, ch.entry));
            cuts.push(Cut {
                env0: p.elevation,
                env1: n.elevation,
                floor0: p.floor,
                floor1: n.floor,
                graded: p.sill || n.sill || p.cut > 0.0 || n.cut > 0.0 || (p.lake.is_some() && n.lake.is_some()),
                pool: (p.lake.is_some() && (n.lake.is_some() || n.sill)).then(|| p.surface),
                base0: p.base,
                base1: n.base,
                half0: ch.half0,
                half1: ch.half1,
                chan0: channel_depth(p),
                chan1: channel_depth(n),
                erode0: p.erodibility,
                erode1: n.erodibility,
                length: start,
                vigour0: ch.vigour0,
                vigour1: ch.vigour1,
                train: ch.train.clone(),
            });
        }
        // A tributary, a channel bringing under half its end node's water,
        // ends its train where it first crosses the trunk's: the train
        // leaving the node, and the one arriving with the most water, each
        // over three of the trunk's wavelengths from the node, which holds
        // every loop of the trunk's that can lie across the tributary.
        let mut leaving: HashMap<NodeKey, usize> = HashMap::new();
        let mut arriving: HashMap<NodeKey, usize> = HashMap::new();
        for (i, &(from, to, entry)) in ends.iter().enumerate() {
            leaving.insert(from, i);
            if entry > 0.5 {
                arriving.insert(to, i);
            }
        }
        for i in 0..cuts.len() {
            if ends[i].2 > 0.5 || cuts[i].train.is_none() {
                continue;
            }
            let to = ends[i].1;
            let reach = |m: &Train| 3.0 * m.wavelength;
            let trunks: Vec<Vec<(f64, f64)>> = [leaving.get(&to), arriving.get(&to)]
                .into_iter()
                .enumerate()
                .filter_map(|(which, j)| {
                    let m = cuts[*j?].train.as_ref()?;
                    Some(if which == 0 { m.head(reach(m)).to_vec() } else { m.tail(reach(m)).to_vec() })
                })
                .collect();
            let tributary = cuts[i].train.as_mut().unwrap();
            for trunk in &trunks {
                tributary.end_at(trunk);
            }
        }
        let mut flooded = HashMap::new();
        let mut extent_of = HashMap::new();
        let mut shores = Vec::new();
        for lake in cells.iter().flat_map(|c| c.lakes.iter()) {
            let ends = lake.outlet.and_then(node).map(|s| (s.wx, s.wy));
            flooded.extend(lake.nodes.iter().map(|&k| (k, shores.len())));
            extent_of.extend(lake.extent.iter().map(|&k| (k, shores.len())));
            shores.push(Shore { surface: lake.surface, read: !lake.extent.is_empty(), ends });
        }
        Self { grid: SegmentGrid::new(segments, VALLEY_HALF_WIDTH), subs, cuts, flooded, extent_of, shores, envelope }
    }

    /// The valleys under a square box, routed from the plate graph directly
    /// and their channels drawn from the routing: what a view or a probe
    /// builds once. Routes every drainage cell within the box's reach, so
    /// it costs a window's routing per cell.
    pub fn in_box(cx: f64, cy: f64, half: f64, seed: u64) -> Self {
        let lattice = DrainageIndex::lattice();
        let (q, r) = world_to_hex(cx, cy);
        let centre = lattice.cell_id(q, r);
        let rings = ((half * std::f64::consts::SQRT_2 + VALLEY_HALF_WIDTH + AXIS_SWING) / lattice.radius as f64).ceil() as u32 + 1;
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
        let within = half * std::f64::consts::SQRT_2 + CHANNEL_REACH;
        let drawn = channels(&refs, |p| (p.wx - cx).hypot(p.wy - cy) <= within, seed);
        let channel_refs: Vec<&Channel> = drawn.iter().collect();
        let reach = half + NODE_SPACING as f64;
        let envelope = Envelope::new(seed, Coasts::in_box(cx, cy, reach, seed), Outlines::in_box(cx, cy, reach, seed));
        Self::new(&refs, &channel_refs, |_| true, envelope)
    }

    pub fn is_empty(&self) -> bool {
        self.grid.is_empty()
    }

    /// The cuts at a position whose envelope is `envelope`, in z-levels: of
    /// every valley in reach, each interpolated along its flow line and
    /// profiled across it from the edge of its belt, the deepest; and of
    /// every channel the position lies in, the deepest slot, whichever
    /// valley's wall it crosses, so a tributary keeps its channel down the
    /// wall of a trunk's deeper valley. A breach cuts to the straight floor
    /// between its nodes, through any ridge between them; any other valley
    /// interpolates its depth. A valley never cuts below the base level it
    /// drains to; its channel, a slot of the segment's half-width and depth
    /// along its train, cuts on below it. A throat in reach pools the
    /// lake's water to its sill's floor.
    ///
    /// Every sub-segment in reach reads on its own and the deepest stands,
    /// which keeps the cut continuous: the nearest point on a bent line
    /// jumps along it across the bend's inside, and a read off the nearest
    /// alone would step there. A far sub-segment's read of the train can
    /// only miss the channel, never find one that is not there.
    pub fn cuts_at(&self, wx: f64, wy: f64, envelope: f64) -> Cuts {
        let mut best = Cuts::default();
        let mut pool: Option<f64> = None;
        self.grid.for_each_within(wx, wy, VALLEY_HALF_WIDTH, |i, d| {
            let sub = self.subs[i];
            let (along, _) = self.grid.segments()[i].project(wx, wy);
            let cut = &self.cuts[sub.cut];
            let x = sub.start + along * sub.length;
            let t = x / cut.length;
            let at = cut.at(t, envelope);
            let u = if at.belt < VALLEY_HALF_WIDTH { ((d - at.belt) / (VALLEY_HALF_WIDTH - at.belt)).max(0.0) } else { 0.0 };
            let valley = (at.depth * profile(u, at.erodibility)).min((envelope - at.base).max(0.0));
            best.valley = best.valley.max(valley);
            if d <= at.belt.max(at.half) {
                let beside = cut.train.as_ref().map_or(d, |m| m.distance(wx, wy));
                if beside <= at.half {
                    best.channel = best.channel.max(at.chan);
                }
            }
            // The pool holds over the throat itself, not past the lip, and
            // only where the throat's cut brought the ground under it: low
            // ground beside it is the lake's if its extent says so.
            if let Some(p) = cut.pool.filter(|&p| (0.0..1.0).contains(&t) && envelope >= p) {
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

    /// The surface water stands at over a position whose envelope is
    /// `envelope` and whose ground, after the cuts, is `ground`: the
    /// highest of the sea, a lake over the position, the lake's pool in the
    /// throat to its sill, and the floor of the channel the position lies
    /// in. None where none stands above the ground.
    ///
    /// A lake read on the fine lattice stands over its extent, the fine
    /// points its water reaches, and over the shore between: a position
    /// under the surface whose nearest fine point is not in the extent but
    /// neighbours one, when the envelope stays under the surface along the
    /// line from the position to that point, so the water never crosses a
    /// rim to stand over the far side. A lake never read there stands
    /// within one node spacing of a flooded node and not past its sill on
    /// the line out from the flooded node: the lake's plane would otherwise
    /// hang over the gorge the river leaves by.
    pub fn surface_at(&self, wx: f64, wy: f64, envelope: f64, ground: f64, cuts: Cuts) -> Option<f64> {
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
        let p = nearest_fine(wx, wy);
        if let Some(&shore) = self.extent_of.get(&p) {
            stand(self.shores[shore].surface);
        } else {
            for (di, dj) in DIRECTIONS {
                let q = (p.0 + di, p.1 + dj);
                let Some(&shore) = self.extent_of.get(&q) else { continue };
                let s = self.shores[shore].surface;
                if envelope < s && self.under(wx, wy, fine_world(q), s) {
                    stand(s);
                }
            }
        }
        // Every node within one spacing of a position is the nearest node
        // or one of its six neighbours: the nearest lies within the
        // lattice's covering radius, a spacing over √3, and the second ring
        // starts at √3 spacings.
        let n = nearest_node(wx, wy);
        let reach = NODE_SPACING as f64;
        for key in std::iter::once(n).chain(DIRECTIONS.iter().map(|(di, dj)| (n.0 + di, n.1 + dj))) {
            let Some(&shore) = self.flooded.get(&key) else { continue };
            let Shore { surface: s, read, ends } = &self.shores[shore];
            if *read {
                continue;
            }
            let (x, y) = crate::lattice::node_world(key);
            if (x - wx).hypot(y - wy) > reach {
                continue;
            }
            if ends.map_or(true, |(px, py)| (wx - px) * (px - x) + (wy - py) * (py - y) <= 0.0) {
                stand(*s);
            }
        }
        surface
    }

    /// Whether the envelope stays under `surface` along the line from a
    /// position to a point, read every [`SHORE_STEP`] between them.
    fn under(&self, wx: f64, wy: f64, to: (f64, f64), surface: f64) -> bool {
        let (dx, dy) = (to.0 - wx, to.1 - wy);
        let steps = (dx.hypot(dy) / SHORE_STEP).ceil().max(1.0);
        (1..steps as usize).all(|i| {
            let t = i as f64 / steps;
            self.envelope.at(wx + dx * t, wy + dy * t) < surface
        })
    }
}

/// The valleys a cell's tiles can lie in: the channels migration published
/// under the cell and its ring, with the drainage cells their nodes are
/// in, keeping the channels within a valley's reach of the cell's ground.
pub fn valleys_of(scope: &CellScope) -> Valleys {
    let edge_cells = scope.source_cells::<PlateEdgeIndex>();
    let coasts = Coasts::new(&scope.read::<PlateEdgeIndex>().map(|idx| idx.edges_in(&edge_cells)).unwrap_or_default());
    let envelope = Envelope::new(scope.seed(), coasts, outlines_of(scope));
    let drainage_cells = scope.source_cells::<DrainageIndex>();
    let channel_cells = scope.source_cells::<ChannelIndex>();
    let (Some(drainage), Some(channels)) = (scope.read::<DrainageIndex>(), scope.read::<ChannelIndex>()) else {
        return Valleys::new(&[], &[], |_| true, envelope);
    };
    let centre = scope.lattice().cell_center(scope.cell());
    let keep_within = scope.lattice().radius as i32 + NODE_SPACING + (VALLEY_HALF_WIDTH + AXIS_SWING).ceil() as i32;
    let cells = drainage.cells_in(&drainage_cells);
    let drawn: Vec<&Channel> = channels.cells_in(&channel_cells).iter().flat_map(|c| c.channels.iter()).collect();
    Valleys::new(&cells, &drawn, |key| hex_distance(node_tile(key), centre) <= keep_within, envelope)
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
    fn scale(&self) -> u32 { MIGRATION_CELL_SCALE }

    /// Nothing originates here.
    fn max_influence(&self) -> u32 { 0 }

    fn register_indexes(&self, _registry: &mut IndexRegistry) {}

    /// Nothing to place: the valleys are read off the channel index.
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
        let water = valleys.surface_at(wx, wy, below.elevation, below.elevation - cut, cuts);
        if cut <= 0.0 && water.is_none() {
            return None;
        }
        Some(TileOutput { elevation_delta: -cut, water, ..TileOutput::default() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::migration::{meander_amplitude, CHANNEL_HALF_WIDTH_MAX, MEANDER_WAVELENGTH};
    use crate::lattice::node_world;

    const S: u64 = 0x9E3779B97F4A7C15;

    /// The nearest cell to the spawn that owns a channelled node and a
    /// lake with its outlet, routed, with its channels drawn and its
    /// valleys built over the envelope they were routed on. Searched
    /// rather than named, since a change beneath drainage can drain the
    /// lakes of any one cell.
    fn spawn_valleys() -> (DrainageCell, Valleys) {
        let lattice = DrainageIndex::lattice();
        let spawn = lattice.cell_id(-58_204, 4_907);
        let mut cells = lattice.cells_within_distance(spawn, 2);
        cells.sort_by_key(|&c| (hex_distance(lattice.cell_center(c), lattice.cell_center(spawn)), c));
        for cell in cells {
            let (cq, cr) = lattice.cell_center(cell);
            let (cx, cy) = hex_to_world(cq, cr);
            let window = (3 * lattice.radius + 1) as f64;
            let coasts = Coasts::in_box(cx, cy, window, S);
            let outlines = Outlines::in_box(cx, cy, window, S);
            let published = DrainageEvent::new().route(&lattice, cell, S, &coasts, &outlines).owned_cell();
            let outlet = published.lakes.iter().any(|l| l.outlet.map_or(false, |o| published.nodes.contains_key(&o)));
            let channelled = published.nodes.values().any(|n| channel_depth(n) > 0.0);
            if !(outlet && channelled) {
                continue;
            }
            let drawn = channels(&[&published], |_| true, S);
            let refs: Vec<&Channel> = drawn.iter().collect();
            let valleys = Valleys::new(&[&published], &refs, |_| true, Envelope::new(S, coasts, outlines));
            return (published, valleys);
        }
        panic!("no cell within two of the spawn owns a lake with its outlet and a channel");
    }

    /// The profile is level at the channel and the divide, falls between,
    /// and in weak rock is never steeper than one and a half depths per
    /// half-width; in hard rock it drops nearer the channel and is lower
    /// everywhere between.
    #[test]
    fn profile_is_level_at_channel_and_divide() {
        assert_eq!(profile(0.0, 1.0), 1.0);
        assert_eq!(profile(1.0, 1.0), 0.0);
        let mut last = 1.0;
        for i in 1..=100 {
            let u = i as f64 / 100.0;
            let p = profile(u, 1.0);
            assert!(p <= last, "profile rises at {u}");
            assert!(last - p <= 1.5 * 0.01 + 1e-9, "profile steeper than its middle at {u}");
            last = p;
            let hard = profile(u, 0.3);
            assert!(hard <= p, "a gorge stands higher than a vale's wall at {u}");
        }
        assert!(profile(0.25, 0.3) < 0.6 && profile(0.25, 1.0) > 0.8);
    }

    /// The channel's depth is nothing at the head, a z-level just past it,
    /// grows with catchment, and saturates at a trunk's.
    #[test]
    fn channel_depth_starts_at_the_head_and_saturates() {
        let node = |catchment: f64, lake: Option<usize>| DrainageNode {
            key: (0, 0), q: 0, r: 0, wx: 0.0, wy: 0.0, elevation: 10.0, surface: 10.0,
            direction: (1.0, 0.0), catchment, base: 0.0, down: None, lake, sill: false, age: 1.0, erodibility: 1.0, cut: 0.0, floor: 5.0,
        };
        assert_eq!(channel_depth(&node(CHANNEL_HEAD, None)), 0.0);
        assert!((channel_depth(&node(CHANNEL_HEAD + 1e-9, None)) - CHANNEL_DEPTH_MIN).abs() < 1e-3);
        let mut last = 0.0;
        for i in 0..200 {
            let d = channel_depth(&node(CHANNEL_HEAD + i as f64 * 0.5, None));
            assert!(d >= last, "channel shallows");
            last = d;
        }
        assert_eq!(channel_depth(&node(10.0 * CATCHMENT_FULL, None)), CHANNEL_DEPTH_MAX);
        assert_eq!(channel_depth(&node(10.0 * CATCHMENT_FULL, Some(0))), 0.0, "a channel cut into a lake");
    }

    /// On a routed cell, the floor along every reach never rises past the
    /// lake it leaves, flooded ground is never cut, no valley cut at a tile
    /// exceeds the envelope's height above sea level, and no slot is
    /// deeper than a trunk's.
    #[test]
    fn floors_never_rise_and_lakes_are_uncut() {
        let (published, valleys) = spawn_valleys();
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
        for n in published.nodes.values() {
            let (x, y) = node_world(n.key);
            let cuts = valleys.cuts_at(x + 100.0, y + 60.0, n.elevation);
            assert!(cuts.valley >= 0.0 && cuts.valley <= n.elevation.max(0.0) + 1e-9, "valley {} at {:?}", cuts.valley, n.key);
            assert!(cuts.channel >= 0.0 && cuts.channel <= CHANNEL_DEPTH_MAX, "slot {} at {:?}", cuts.channel, n.key);
        }
    }

    /// At a channelled node the slot is cut at least to the node's channel
    /// depth beneath the valley floor, deeper only where a bigger river's
    /// train has wandered over the node, and past the widest belt any
    /// train sweeps it is not; the surface a river stands at, the ground
    /// plus the channel cut, is the valley floor either way.
    #[test]
    fn the_channel_is_a_slot_in_the_valley_floor() {
        let (published, valleys) = spawn_valleys();
        let mut channelled = 0;
        for n in published.nodes.values() {
            if channel_depth(n) <= 0.0 || n.sill {
                continue;
            }
            channelled += 1;
            let (x, y) = node_world(n.key);
            let at = valleys.cuts_at(x, y, n.elevation);
            assert!(at.channel >= channel_depth(n) - 1e-9, "channel {} for {} at {:?}", at.channel, channel_depth(n), n.key);
            assert!(n.lake.is_some() || at.valley >= depth_at(n) - 1e-9, "a shallower valley than the node's own at {:?}", n.key);
            // Past the widest belt any train sweeps, square off the flow
            // line at the node, no slot of this channel is cut; one there
            // is another channel's, interpolated short of its own node.
            let (dx, dy) = (-n.direction.1, n.direction.0);
            let off = 1.5 * meander_amplitude(MEANDER_WAVELENGTH * 2.0 * CHANNEL_HALF_WIDTH_MAX) + CHANNEL_HALF_WIDTH_MAX + 1.0;
            let beside = valleys.cuts_at(x + dx * off, y + dy * off, n.elevation);
            assert!(beside.channel == 0.0 || beside.channel < at.channel + 1e-6, "a channel beside the channel at {:?}", n.key);
        }
        assert!(channelled > 0, "no channelled node in the spawn cell");
    }

    /// Water stands at the valley floor in a channel, at the lake's surface
    /// over a flooded node, at zero over ground below the sea, and nowhere
    /// over dry ground above it.
    #[test]
    fn water_stands_at_the_floor_the_surface_and_the_sea() {
        let (published, valleys) = spawn_valleys();
        let (mut rivers, mut lakes, mut dry) = (0, 0, 0);
        for n in published.nodes.values() {
            let (x, y) = node_world(n.key);
            let cuts = valleys.cuts_at(x, y, n.elevation);
            let ground = n.elevation - cuts.valley - cuts.channel;
            let water = valleys.surface_at(x, y, n.elevation, ground, cuts);
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
