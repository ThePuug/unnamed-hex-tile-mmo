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
//! pit of the closed ground it runs down into. A basin's
//! sill is cut by exactly what drainage published, the sill's height less
//! the basin's fill, since the sill holds the fill; along the breach past
//! it the floor is never above the cut sill, so the outlet is a gorge
//! through the rim until the valley's own floor is lower. A breach cuts to
//! the straight floor between its nodes, so the gorge is cut through any
//! ridge the lattice did not sample; every other valley interpolates its
//! depth and such a ridge stands. The floor at a node is what drainage
//! published for it: the envelope less a share of its height above base
//! level, the share saturating below one so a graded trunk keeps the fall
//! that keeps it flowing, and growing with the plate's age from a young
//! orogen's narrow cut to an old one's. Along a reach the floor never
//! rises, because the envelope falls downstream and the share grows with
//! the water; a test holds it.
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
//! under it, and one entering closed ground runs on down to the basin's
//! pit and ends there.
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
//! ground lies below it, and in a channel the valley floor the channel is
//! cut into. The higher stands, and only over ground below it. A tile with
//! no surface over it is dry ground. A lake over closed ground is unbuilt.
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
use crate::lattice::{hex_distance, node_site, NodeKey, NODE_SPACING, NODE_SWING};
use crate::{hex_to_world, world_to_hex};
use super::drainage::{growth, DrainageCell, DrainageEvent, DrainageIndex, DrainageNode};
pub use super::drainage::{CATCHMENT_FULL, CHANNEL_HEAD};
use super::index::IndexRegistry;
use super::migration::{channels, Channel, ChannelIndex, Train, AXIS_SWING, CHANNEL_REACH, MIGRATION_CELL_SCALE};
pub use super::migration::VALLEY_HALF_WIDTH;
use super::plates::Coasts;
use super::thrusting::Outlines;
use super::{gradient_of, CellScope, TileOutput, TileView, WorldEvent};

// ── The channel ─────────────────────────────────────────────────────────────

/// Depth of the channel at the channel head, in z-levels: the one step a
/// walker wades.
pub const CHANNEL_DEPTH_MIN: f64 = 1.0;

/// Depth of a full trunk's channel, in z-levels.
pub const CHANNEL_DEPTH_MAX: f64 = 3.0;

/// The depth of the channel slot below the valley floor at a node: nothing
/// below the channel head; otherwise from the head's depth to a trunk's as
/// the catchment grows. Cut below base level too: a channel reaching the
/// sea is under it.
pub fn channel_depth(node: &DrainageNode) -> f64 {
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
    /// down to it: a breach, at a sill or along the river leaving one.
    /// Elsewhere the depth is interpolated and an unsampled ridge stands.
    graded: bool,
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
/// surface is the floor, the ground plus the channel cut. With them, where
/// on the wall the position lies, published for the layers above.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cuts {
    pub valley: f64,
    pub channel: f64,
    /// How far up the nearest valley's wall the position lies: nothing
    /// across the belt the river has swept, one at the divide, and one
    /// where no valley reaches.
    pub wall: f64,
}

impl Default for Cuts {
    fn default() -> Self {
        Cuts { valley: 0.0, channel: 0.0, wall: 1.0 }
    }
}

/// The valleys a set of tiles can lie in: every channel in reach along its
/// flow line, bucketed for the search, with what each cuts.
pub struct Valleys {
    grid: SegmentGrid,
    subs: Vec<Sub>,
    cuts: Vec<Cut>,
}

impl Valleys {
    /// Valleys from published channels and the drainage cells their nodes
    /// are in, keeping the channels with an end `keep` accepts. A channel
    /// whose node is in no cell given is left out.
    pub fn new(cells: &[&DrainageCell], channels: &[&Channel], keep: impl Fn(NodeKey) -> bool) -> Self {
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
                graded: p.sill || n.sill || p.cut > 0.0 || n.cut > 0.0,
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
        Self { grid: SegmentGrid::new(segments, VALLEY_HALF_WIDTH), subs, cuts }
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
        Self::new(&refs, &channel_refs, |_| true)
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
    /// along its train, cuts on below it.
    ///
    /// Every sub-segment in reach reads on its own and the deepest stands,
    /// which keeps the cut continuous: the nearest point on a bent line
    /// jumps along it across the bend's inside, and a read off the nearest
    /// alone would step there. A far sub-segment's read of the train can
    /// only miss the channel, never find one that is not there.
    pub fn cuts_at(&self, wx: f64, wy: f64, envelope: f64) -> Cuts {
        let mut best = Cuts::default();
        self.grid.for_each_within(wx, wy, VALLEY_HALF_WIDTH, |i, d| {
            let sub = self.subs[i];
            let (along, _) = self.grid.segments()[i].project(wx, wy);
            let cut = &self.cuts[sub.cut];
            let x = sub.start + along * sub.length;
            let t = x / cut.length;
            let at = cut.at(t, envelope);
            let u = if at.belt < VALLEY_HALF_WIDTH { ((d - at.belt) / (VALLEY_HALF_WIDTH - at.belt)).max(0.0) } else { 0.0 };
            // Past the divide the profile is nothing, and the power it is
            // raised to costs more than the rest of the visit.
            if u < 1.0 && at.depth > 0.0 {
                let valley = (at.depth * profile(u, at.erodibility)).min((envelope - at.base).max(0.0));
                best.valley = best.valley.max(valley);
                best.wall = best.wall.min(u);
            }
            if d <= at.belt.max(at.half) {
                let beside = cut.train.as_ref().map_or(d, |m| m.distance(wx, wy));
                if beside <= at.half {
                    best.channel = best.channel.max(at.chan);
                }
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
    /// cuts, is `ground`: the higher of the sea and the floor of the
    /// channel the position lies in. None where neither stands above the
    /// ground.
    pub fn surface_at(&self, ground: f64, cuts: Cuts) -> Option<f64> {
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
        surface
    }
}

/// The valleys a cell's tiles can lie in: the channels migration published
/// under the cell and its ring, with the drainage cells their nodes are
/// in, keeping the channels within a valley's reach of the cell's ground.
pub fn valleys_of(scope: &CellScope) -> Valleys {
    let (Some(drainage), Some(channels)) = (scope.read::<DrainageIndex>(), scope.read::<ChannelIndex>()) else {
        return Valleys::new(&[], &[], |_| true);
    };
    let centre = scope.lattice().cell_center(scope.cell());
    let keep_within = scope.lattice().radius as i32 + NODE_SPACING + (2.0 * NODE_SWING + VALLEY_HALF_WIDTH + AXIS_SWING).ceil() as i32;
    let cells: Vec<&DrainageCell> = drainage.entries().collect();
    let drawn: Vec<&Channel> = channels.entries().flat_map(|c| c.channels.iter()).collect();
    Valleys::new(&cells, &drawn, |key| hex_distance(node_site(key), centre) <= keep_within)
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
        let water = valleys.surface_at(below.elevation - cut, cuts);
        let valley = (cuts.wall < 1.0).then_some(cuts.wall);
        if cut <= 0.0 && water.is_none() && valley.is_none() {
            return None;
        }
        // The wall's slope, never the channel's: a bank is a step, and a
        // step read over half a tile is a cliff that is not there. The
        // envelope leans with the ground beneath, which the cut follows.
        let (gx, gy) = below.gradient;
        let (dx, dy) = gradient_of(wx, wy, cuts.valley, |x, y| {
            valleys.cuts_at(x, y, below.elevation + gx * (x - wx) + gy * (y - wy)).valley
        });
        Some(TileOutput { elevation_delta: -cut, gradient: (-dx, -dy), water, valley, ..TileOutput::default() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::migration::{meander_amplitude, CHANNEL_HALF_WIDTH_MAX, MEANDER_WAVELENGTH};


    const S: u64 = 0x9E3779B97F4A7C15;

    /// The nearest cell to the spawn that owns a channelled node and a
    /// flooded one, routed, with its channels drawn and its valleys built.
    /// Searched rather than named, since a change beneath drainage can
    /// drain the closed ground of any one cell.
    fn spawn_valleys() -> (DrainageCell, Valleys) {
        let lattice = DrainageIndex::lattice();
        let spawn = lattice.cell_id(104_289, -4_677);
        let mut cells = lattice.cells_within_distance(spawn, 2);
        cells.sort_by_key(|&c| (hex_distance(lattice.cell_center(c), lattice.cell_center(spawn)), c));
        for cell in cells {
            let (cq, cr) = lattice.cell_center(cell);
            let (cx, cy) = hex_to_world(cq, cr);
            let window = (3 * lattice.radius + 1) as f64;
            let coasts = Coasts::in_box(cx, cy, window, S);
            let outlines = Outlines::in_box(cx, cy, window, S);
            let published = DrainageEvent::new().route(&lattice, cell, S, &coasts, &outlines).owned_cell();
            let flooded = published.nodes.values().any(|n| n.flooded);
            let channelled = published.nodes.values().any(|n| channel_depth(n) > 0.0);
            if !(flooded && channelled) {
                continue;
            }
            let drawn = channels(&[&published], |_| true, S);
            let refs: Vec<&Channel> = drawn.iter().collect();
            let valleys = Valleys::new(&[&published], &refs, |_| true);
            return (published, valleys);
        }
        panic!("no cell within two of the spawn owns closed ground and a channel");
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
        let node = |catchment: f64, flooded: bool| DrainageNode {
            key: (0, 0), q: 0, r: 0, wx: 0.0, wy: 0.0, elevation: 10.0, surface: 10.0, flooded,
            direction: (1.0, 0.0), catchment, base: 0.0, down: None, sill: false, age: 1.0, erodibility: 1.0, cut: 0.0, floor: 5.0,
        };
        assert_eq!(channel_depth(&node(CHANNEL_HEAD, false)), 0.0);
        assert!((channel_depth(&node(CHANNEL_HEAD + 1e-9, false)) - CHANNEL_DEPTH_MIN).abs() < 1e-3);
        let mut last = 0.0;
        for i in 0..200 {
            let d = channel_depth(&node(CHANNEL_HEAD + i as f64 * 0.5, false));
            assert!(d >= last, "channel shallows");
            last = d;
        }
        assert_eq!(channel_depth(&node(10.0 * CATCHMENT_FULL, false)), CHANNEL_DEPTH_MAX);
        assert_eq!(channel_depth(&node(10.0 * CATCHMENT_FULL, true)), CHANNEL_DEPTH_MAX, "closed ground carries its river's channel");
    }

    /// On a routed cell, the floor along every reach never rises, a hump
    /// on a drained floor's path is cut exactly what the routing cut, a
    /// sill is cut exactly what drainage published and the breach below it
    /// never rises, no valley cut at a tile exceeds the envelope's height
    /// above sea level, and no slot is deeper than a trunk's.
    #[test]
    fn floors_never_rise_and_sills_are_cut_as_published() {
        let (published, valleys) = spawn_valleys();
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
        for n in published.nodes.values().filter(|n| n.flooded && n.cut > 0.0) {
            assert_eq!(depth_at(n), n.cut, "a hump on a drained floor cut past what the routing cut at {:?}", n.key);
        }
        let (mut sills, mut cut_sills) = (0, 0);
        for outlet in published.nodes.values().filter(|n| n.sill) {
            sills += 1;
            assert_eq!(depth_at(outlet), outlet.cut, "a sill cut past what drainage published at {:?}", outlet.key);
            if outlet.cut > 0.0 { cut_sills += 1 }
            let mut last = outlet.elevation - outlet.cut;
            let mut next = outlet.down.and_then(|d| published.nodes.get(&d));
            while let Some(n) = next {
                if n.flooded { break }
                let f = n.elevation - depth_at(n);
                assert!(f <= last + 1e-9, "a floor rising past a sill at {:?}", n.key);
                if n.cut == 0.0 { break }
                last = f;
                next = n.down.and_then(|d| published.nodes.get(&d));
            }
        }
        assert!(sills > 0, "no basin in the spawn cell drains through a sill the cell owns");
        assert!(cut_sills > 0, "no sill in the spawn cell is cut");
        for n in published.nodes.values() {
            let (x, y) = (n.wx, n.wy);
            let cuts = valleys.cuts_at(x + 100.0, y + 60.0, n.elevation);
            assert!(cuts.valley >= 0.0 && cuts.valley <= n.elevation.max(0.0) + 1e-9, "valley {} at {:?}", cuts.valley, n.key);
            assert!(cuts.channel >= 0.0 && cuts.channel <= CHANNEL_DEPTH_MAX, "slot {} at {:?}", cuts.channel, n.key);
        }
    }

    /// At a channelled node the slot is cut at least to the node's channel
    /// depth beneath the valley floor, deeper only where a bigger river's
    /// train has wandered over the node, and past the widest belt any
    /// train sweeps it is not; the surface a river stands at, the ground
    /// plus the channel cut, is the valley floor either way. A node whose
    /// next node the cell does not publish is the neighbour's to draw.
    #[test]
    fn the_channel_is_a_slot_in_the_valley_floor() {
        let (published, valleys) = spawn_valleys();
        let mut channelled = 0;
        for n in published.nodes.values() {
            if channel_depth(n) <= 0.0 || n.sill || n.down.map_or(true, |d| !published.nodes.contains_key(&d)) {
                continue;
            }
            channelled += 1;
            let (x, y) = (n.wx, n.wy);
            let at = valleys.cuts_at(x, y, n.elevation);
            assert!(at.channel >= channel_depth(n) - 1e-9, "channel {} for {} at {:?}", at.channel, channel_depth(n), n.key);
            assert!(n.flooded || at.valley >= depth_at(n) - 1e-9, "a shallower valley than the node's own at {:?}", n.key);
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

    /// Water stands at the valley floor in a channel, at zero over ground
    /// below the sea, and nowhere over dry ground above it, closed ground
    /// included.
    #[test]
    fn water_stands_at_the_floor_and_the_sea() {
        let (published, valleys) = spawn_valleys();
        let (mut rivers, mut basins, mut dry) = (0, 0, 0);
        for n in published.nodes.values() {
            let (x, y) = (n.wx, n.wy);
            let cuts = valleys.cuts_at(x, y, n.elevation);
            let ground = n.elevation - cuts.valley - cuts.channel;
            let water = valleys.surface_at(ground, cuts);
            if cuts.channel > 0.0 {
                rivers += 1;
                let floor = n.elevation - cuts.valley;
                assert!(water.map_or(false, |w| w >= floor - 1e-9), "a channel not under water at {:?}", n.key);
                if water == Some(floor) {
                    assert!((water.unwrap() - ground - cuts.channel).abs() < 1e-9);
                }
            } else if ground >= 0.0 {
                if n.flooded { basins += 1 } else { dry += 1 }
                assert_eq!(water, None, "water over dry ground at {:?}", n.key);
            } else {
                assert_eq!(water, Some(0.0), "sea floor not under the sea at {:?}", n.key);
            }
        }
        assert!(rivers > 0 && basins > 0 && dry > 0, "rivers {rivers}, basins {basins}, dry {dry}");
    }
}
