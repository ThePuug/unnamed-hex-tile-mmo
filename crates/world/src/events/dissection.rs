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
//! # The flow line
//!
//! Drainage's nodes are samples of a flow line, and the chord between two
//! of them was never the river: dissection draws the channel between two
//! nodes as the curve that leaves each along its own downslope, the true
//! outflow drainage publishes at every node, and arrives at the next along
//! that node's. So a river bends at the wavelength of the ground that
//! turns it, tributaries arrive at their confluence pointing downstream
//! along the trunk's outflow, and a reach turning at a node turns as a
//! curve and never as a corner. A downslope more than square to its chord
//! is read as square: the flow turns harder at that node than a curve
//! between samples can carry, and a hook toward the node would be drawn
//! from nothing. The line is a cubic between the nodes with those tangents
//! at the chord's length, drawn as a polyline, and it stays within
//! [`AXIS_SWING`] of the chord. On a uniformly tilted plain the downslope
//! holds and the line is straight, which is right: nothing there bends it.
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
//! # The meander
//!
//! A river far above its base level cuts down: its bed erodes faster than
//! its banks, and the channel holds the flow line through a gorge. A river
//! at grade, its floor falling only as fast as its load needs, cuts
//! sideways instead: the flow in any slight bend scours the outer bank and
//! builds the inner, the bend grows and walks downstream, and the river
//! sweeps a belt several bends wide, planing it level as it goes. So where
//! the floor's grade toward the next node is under the grade a river cuts
//! down at, the channel meanders across the flow line and the floor is
//! level across the belt it has swept; how far, by the plate's age, the
//! same dial the depth turns on. The train is the sine-generated curve, the
//! heading swinging as a sine of the distance along the channel, with the
//! wavelength a channel's width sets and each bend's length its own by a
//! hash, its size following: the shape is the curve's and the hash sets a
//! quantity. The channel crosses the flow line at every node, so a train
//! fits between two nodes as a whole number of half-waves, and it crosses
//! toward the side a hash of the node picks, so the train continues
//! through a node whichever cell drew each side. A tributary's train
//! fades into the trunk over its last bend by the share of the water it
//! brings, and ends at the trunk's bank where it first meets the trunk's
//! channel, so the two never knot at the node. The channel is drawn as a
//! polyline laid across the flow
//! line, kept by its position along the line, so a tile reads its distance
//! to the channel from the samples near its own position and never scans
//! the train.
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
//! No deform: nothing originates here. `prepare` gathers the drainage cells
//! under the cell and its ring into valleys, and a query reads the cut at
//! its own tile from the envelope beneath it. The cell is drainage's, so the
//! footprint plus one ring is the seven cells drainage routed for the same
//! tiles, and reading them costs no route.

use std::any::Any;
use std::collections::HashMap;
use std::f64::consts::PI;
use std::sync::OnceLock;

use crate::chains::{Segment, SegmentGrid};
use crate::lattice::{hex_distance, nearest_node, node_tile, NodeKey, DIRECTIONS, NODE_SPACING};
use crate::noise::{hash_channel, hash_channel_f64};
use crate::{hex_to_world, world_to_hex, RISE};
use super::drainage::{
    fine_world, growth, nearest_fine, surface_at as envelope_at, DrainageCell, DrainageEvent, DrainageIndex,
    DrainageNode, DRAINAGE_CELL_SCALE,
};
pub use super::drainage::{CATCHMENT_FULL, CHANNEL_HEAD};
use super::index::IndexRegistry;
use super::plates::{Coasts, PlateEdgeIndex};
use super::thrusting::{outlines_of, Outlines};
use super::{CellScope, TileOutput, TileView, WorldEvent};

// ── The valley ──────────────────────────────────────────────────────────────

/// How far a valley reaches from its channel, in world units: half the node
/// spacing, so neighbouring valleys meet at their divide and never take each
/// other's walls. Structural, not tuned.
pub const VALLEY_HALF_WIDTH: f64 = NODE_SPACING as f64 / 2.0;

/// The farthest the flow line between two nodes strays from their chord,
/// in world units: a quarter of the spacing. A cubic with tangents of the
/// chord's length within a right angle of it lies within `t(1 - t)` of a
/// chord from the chord, a quarter at most; a test holds it over every
/// tangent pair.
pub const AXIS_SWING: f64 = NODE_SPACING as f64 / 4.0;

/// Sub-segments the flow line between two nodes is drawn with. The line's
/// curvature peaks at the ends of a reverse bend with square tangents, a
/// radius of a sixth of the chord, where a sub-segment this short misses
/// the curve by a tile; along an ordinary bend by a fraction of one.
const AXIS_STEPS: usize = 24;

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
    growth(catchment).map_or(0.0, |g| RELIEF_SHARE_MAX * g * aged(age))
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

// ── The meander ─────────────────────────────────────────────────────────────

/// A meander's wavelength along the flow line as a multiple of the
/// channel's width: Leopold and Wolman's 10.9, holding from brooks to the
/// Mississippi.
pub const MEANDER_WAVELENGTH: f64 = 10.9;

/// The angle the channel crosses the flow line at, in radians: the
/// amplitude of the sine-generated curve's heading. Sinuosity is the
/// reciprocal of the Bessel J0 of it, here about 1.9, a well-developed
/// train with rounded lobes. Under a right angle, so the channel never
/// runs back up the valley and its position along the flow line is
/// one-to-one.
pub const MEANDER_DEFLECTION: f64 = 1.45;

/// The floor's grade, as rise over run, at and under which a river is at
/// grade and sweeps its whole belt: a thousandth, the slope of the
/// meandering rivers of the plains.
pub const MEANDER_GRADE_FULL: f64 = 0.001;

/// The grade at and over which a river cuts down and holds the flow line:
/// a hundredth, where mountain streams begin.
pub const MEANDER_GRADE_NONE: f64 = 0.01;

/// How far one bend's length strays from the train's, as a share of it;
/// its size follows its length.
pub const MEANDER_LOBE_VARIANCE: f64 = 0.3;

/// Points per wavelength of channel the train is drawn with, and the
/// shortest step in world units. A bend's tightest radius is its length
/// over pi times the deflection, so a step this share of the wavelength
/// misses the shortest bend's curve by under half a tile at a trunk, and
/// a step under a tile resolves nothing a tile can show.
const MEANDER_STEPS_PER_WAVE: f64 = 24.0;
const MEANDER_STEP_MIN: f64 = 1.0;

/// Hash channels: which side a node's crossing turns toward, and a bend's
/// length.
const MEANDER_SIDE: u64 = 0x6d65_616e;
const MEANDER_LOBE: u64 = 0x6c6f_6265;

/// The sine-generated curve over one wavelength of unit path: how far it
/// advances along the flow line, the Bessel J0 of the deflection, and how
/// far it reaches to the side at a lobe's apex.
fn meander_shape() -> (f64, f64) {
    static SHAPE: OnceLock<(f64, f64)> = OnceLock::new();
    *SHAPE.get_or_init(|| {
        let n = 4096;
        let (mut x, mut y, mut peak) = (0.0, 0.0, 0.0f64);
        for i in 0..n {
            let s = (i as f64 + 0.5) / n as f64;
            let heading = MEANDER_DEFLECTION * (2.0 * PI * s).cos();
            x += heading.cos() / n as f64;
            y += heading.sin() / n as f64;
            peak = peak.max(y.abs());
        }
        (x, peak)
    })
}

/// How far a full train whose wavelength along the flow line is
/// `wavelength` reaches to either side of the line, in world units, its
/// largest lobe included: what the belt a river at grade has swept is wide
/// to each side, before its channel. A channel of width `w` has a
/// wavelength of [`MEANDER_WAVELENGTH`] times `w`, fitted between its nodes.
pub fn meander_amplitude(wavelength: f64) -> f64 {
    let (advance, peak) = meander_shape();
    (1.0 + MEANDER_LOBE_VARIANCE) * peak * wavelength / advance
}

/// The aged share: what a plate of `age` has done of an aged plate's
/// cutting, sideways as much as down.
fn aged(age: f64) -> f64 {
    YOUNG_SHARE + (1.0 - YOUNG_SHARE) * age.clamp(0.0, 1.0)
}

/// How far the river at a node has turned to its banks, 0 to 1: nothing
/// above the channel head or on flooded ground; else the plate's aged
/// share by how far the floor's grade over `run` to `floor_down`, the next
/// floor downstream, lies under the grade a river cuts down at, whole at
/// the grade of the plains and nothing at a mountain stream's.
pub fn vigour(node: &DrainageNode, floor_down: f64, run: f64) -> f64 {
    if node.lake.is_some() || growth(node.catchment).is_none() {
        return 0.0;
    }
    let floor = node.elevation - depth_at(node);
    let grade = (floor - floor_down).max(0.0) * RISE / run;
    let g = ((grade - MEANDER_GRADE_FULL) / (MEANDER_GRADE_NONE - MEANDER_GRADE_FULL)).clamp(0.0, 1.0);
    aged(node.age) * (1.0 - g * g * (3.0 - 2.0 * g))
}

// ── The shore ───────────────────────────────────────────────────────────────

/// How often the envelope is read along the line from a shore tile to the
/// lake's extent, in tiles. A rim crest is a crease at repose, so a crest
/// the samples straddle stands within half a step of one and a crest under
/// half a step's climb, a level or two, is the most the line can miss.
const SHORE_STEP: f64 = 4.0;

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
    /// The flow line's length, what `t` runs along.
    length: f64,
    vigour0: f64,
    vigour1: f64,
    /// The share of the end node's water the start node brings: near one
    /// along a channel, small for a tributary entering a trunk.
    entry: f64,
    /// The channel's train, or none where the river holds the flow line.
    meander: Option<Meander>,
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

/// A flow line as a polyline: the length along it at each point, and a
/// unit normal at each, the mean of the sub-segments' meeting there, so an
/// offset from the line turns with it and never steps at a joint.
struct Axis {
    pts: Vec<(f64, f64)>,
    cum: Vec<f64>,
    normals: Vec<(f64, f64)>,
}

impl Axis {
    fn new(pts: Vec<(f64, f64)>) -> Self {
        let mut cum = Vec::with_capacity(pts.len());
        let mut sides = Vec::with_capacity(pts.len());
        let mut total = 0.0;
        cum.push(0.0);
        for pair in pts.windows(2) {
            let (dx, dy) = (pair[1].0 - pair[0].0, pair[1].1 - pair[0].1);
            let len = dx.hypot(dy);
            total += len;
            cum.push(total);
            sides.push(if len > 0.0 { (-dy / len, dx / len) } else { (0.0, 0.0) });
        }
        let unit = |(x, y): (f64, f64)| {
            let len = x.hypot(y);
            if len > 0.0 { (x / len, y / len) } else { (0.0, 0.0) }
        };
        let normals = (0..pts.len())
            .map(|i| match (i.checked_sub(1).and_then(|j| sides.get(j)), sides.get(i)) {
                (Some(a), Some(b)) => unit((a.0 + b.0, a.1 + b.1)),
                (Some(a), None) | (None, Some(a)) => *a,
                (None, None) => (0.0, 0.0),
            })
            .collect();
        Axis { pts, cum, normals }
    }

    fn length(&self) -> f64 {
        *self.cum.last().unwrap_or(&0.0)
    }

    /// The point at length `x` along the line and the normal there, found
    /// from sub-segment `from` onward, which advances with `x`.
    fn at(&self, x: f64, from: &mut usize) -> ((f64, f64), (f64, f64)) {
        while *from + 2 < self.cum.len() && self.cum[*from + 1] < x {
            *from += 1;
        }
        let j = *from;
        let len = self.cum[j + 1] - self.cum[j];
        let u = if len > 0.0 { ((x - self.cum[j]) / len).clamp(0.0, 1.0) } else { 0.0 };
        let lerp = |a: (f64, f64), b: (f64, f64)| (a.0 + u * (b.0 - a.0), a.1 + u * (b.1 - a.1));
        let (nx, ny) = lerp(self.normals[j], self.normals[j + 1]);
        let nl = nx.hypot(ny);
        let normal = if nl > 0.0 { (nx / nl, ny / nl) } else { self.normals[j] };
        (lerp(self.pts[j], self.pts[j + 1]), normal)
    }
}

/// A channel's train laid across its flow line: its points in world space
/// and each one's position along the line, ascending, so the points near
/// a position along the line are found by that position; and how far it
/// reaches to each side at full vigour, its largest lobe included, before
/// the channel's own half-width.
#[derive(Clone, Debug)]
struct Meander {
    along: Vec<f64>,
    pts: Vec<(f64, f64)>,
    step: f64,
    amplitude: f64,
    /// The wavelength along the flow line, as fitted between the nodes.
    wavelength: f64,
}

impl Meander {
    /// The train from node `p` to node `n` across `axis`, at the vigour
    /// each end has, or none where neither end meanders or no channel runs:
    /// the sine-generated curve crossing the line at both nodes, toward the
    /// side each node's hash picks, a whole number of half-waves at the
    /// wavelength the channel's width sets, each bend's length its own by
    /// a hash and its size following, the whole scaled by the vigour along
    /// the line. Over its last bend the train fades toward the line by the
    /// share of `n`'s water `p` brings, so a tributary arrives at a trunk
    /// along its flow line and a channel continuing through a node keeps
    /// its train.
    fn new(axis: &Axis, p: &DrainageNode, n: &DrainageNode, cut: &Cut, seed: u64) -> Option<Self> {
        let width = cut.half0 + cut.half1;
        if width <= 0.0 || (cut.vigour0 <= 0.0 && cut.vigour1 <= 0.0) {
            return None;
        }
        let length = axis.length();
        let side = |key: NodeKey| if hash_channel(key.0 as i64, key.1 as i64, seed, MEANDER_SIDE) & 1 == 0 { 1.0 } else { -1.0 };
        let (sign, sign1) = (side(p.key), side(n.key));
        // Half-waves between the nodes: the count nearest the wavelength
        // whose parity turns the crossing at `n` the way `n`'s hash says.
        let wavelength = MEANDER_WAVELENGTH * width;
        let target = 2.0 * length / wavelength;
        let even = sign == sign1;
        let mut k = target.round().max(1.0) as u64;
        if (k % 2 == 0) != even {
            k = if k > 1 && (target - (k - 1) as f64).abs() <= ((k + 1) as f64 - target).abs() { k - 1 } else { k + 1 };
        }
        let (advance, _) = meander_shape();
        let wavelength = 2.0 * length / k as f64;
        let path_wave = wavelength / advance;
        let path = length / advance;
        let amplitude = meander_amplitude(wavelength);
        // Each bend's length: its share of the path, normalised so the
        // bends fill it; a bend's size follows its length, since the curve
        // returns to the line over any bend.
        let mut bends: Vec<f64> = (0..k)
            .map(|j| 1.0 + MEANDER_LOBE_VARIANCE * (2.0 * hash_channel_f64(p.key.0 as i64, p.key.1 as i64, seed, MEANDER_LOBE.wrapping_add(j)) - 1.0))
            .collect();
        let total: f64 = bends.iter().sum();
        for b in &mut bends {
            *b *= path / total;
        }
        let entry = cut.entry;
        let step = (path_wave / MEANDER_STEPS_PER_WAVE).max(MEANDER_STEP_MIN);
        // Each bend is walked in its own whole number of steps, so its
        // midpoint samples sit symmetrically about its centre and the
        // curve returns to the line at its end exactly. The fade scales the
        // offset and never the heading, for the same reason.
        let mut samples = Vec::with_capacity((path / step) as usize + bends.len() + 1);
        let (mut x, mut y) = (0.0, 0.0);
        let mut longest: f64 = 0.0;
        samples.push((x, y, 1.0));
        for (j, &bend) in bends.iter().enumerate() {
            let count = (bend / step).ceil().max(1.0);
            let step = bend / count;
            longest = longest.max(step);
            let side = if j % 2 == 0 { sign } else { -sign };
            for i in 0..count as usize {
                let frac = (i as f64 + 0.5) / count;
                let heading = side * MEANDER_DEFLECTION * (PI * frac).cos();
                x += step * heading.cos();
                y += step * heading.sin();
                let end = (i as f64 + 1.0) / count;
                let fade = if j + 1 == bends.len() { 1.0 - (1.0 - entry) * end * end * (3.0 - 2.0 * end) } else { 1.0 };
                samples.push((x, y, fade));
            }
        }
        let step = longest;
        let scale = length / x;
        let mut from = 0;
        let mut along = Vec::with_capacity(samples.len());
        let mut pts = Vec::with_capacity(samples.len());
        for &(x, y, fade) in &samples {
            let x = (x * scale).min(length);
            let t = x / length;
            let vigour = cut.vigour0 + t * (cut.vigour1 - cut.vigour0);
            let offset = y * fade * vigour;
            let (point, normal) = axis.at(x, &mut from);
            along.push(x);
            pts.push((point.0 + offset * normal.0, point.1 + offset * normal.1));
        }
        Some(Meander { along, pts, step, amplitude, wavelength })
    }

    /// End this train where it first crosses the `trunk` points, walking
    /// downstream over its last `reach` of line: a tributary joins the
    /// trunk at the trunk's bank and runs no further. Untouched when the
    /// two never cross before the node, where both trains meet in any
    /// case.
    fn end_at(&mut self, trunk: &[(f64, f64)], reach: f64) {
        let Some(&length) = self.along.last() else { return };
        let first = self.along.partition_point(|&a| a < length - reach).saturating_sub(1);
        for i in first..self.pts.len() - 1 {
            let (a, b) = (self.pts[i], self.pts[i + 1]);
            let mut hit: Option<(f64, (f64, f64))> = None;
            for pair in trunk.windows(2) {
                let (c, d) = (pair[0], pair[1]);
                let (rx, ry) = (b.0 - a.0, b.1 - a.1);
                let (sx, sy) = (d.0 - c.0, d.1 - c.1);
                let den = rx * sy - ry * sx;
                if den.abs() < 1e-12 {
                    continue;
                }
                let (qx, qy) = (c.0 - a.0, c.1 - a.1);
                let t = (qx * sy - qy * sx) / den;
                let u = (qx * ry - qy * rx) / den;
                if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) && hit.map_or(true, |(bt, _)| t < bt) {
                    hit = Some((t, (a.0 + t * rx, a.1 + t * ry)));
                }
            }
            if let Some((t, point)) = hit {
                let along = self.along[i] + t * (self.along[i + 1] - self.along[i]);
                self.pts.truncate(i + 1);
                self.along.truncate(i + 1);
                self.pts.push(point);
                self.along.push(along);
                return;
            }
        }
    }

    /// The distance from a position to the channel, read from the points
    /// within reach of the position's length `x` along the flow line: the
    /// channel's half-width, the step, and the stretch a bend of the line
    /// puts between lengths along it and distances beside it.
    fn distance(&self, x: f64, wx: f64, wy: f64) -> f64 {
        let reach = 4.0 * CHANNEL_HALF_WIDTH_MAX + 2.0 * self.step;
        let lo = self.along.partition_point(|&a| a < x - reach).saturating_sub(1);
        let hi = self.along.partition_point(|&a| a <= x + reach).min(self.pts.len() - 1);
        let mut best = f64::MAX;
        for i in lo..hi {
            let (a, b) = (self.pts[i], self.pts[i + 1]);
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let len2 = dx * dx + dy * dy;
            let u = if len2 > 0.0 { (((wx - a.0) * dx + (wy - a.1) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
            best = best.min((wx - a.0 - u * dx).hypot(wy - a.1 - u * dy));
        }
        best
    }
}

/// The flow line from node `p` to node `n`: a cubic leaving `p` along its
/// downslope and arriving at `n` along `n`'s, each tangent the chord's
/// length and turned no further than square to the chord, as
/// [`AXIS_STEPS`] steps of points.
fn flow_line(p: &DrainageNode, n: &DrainageNode) -> Vec<(f64, f64)> {
    let (cx, cy) = (n.wx - p.wx, n.wy - p.wy);
    let length = cx.hypot(cy);
    let tangent = |(dx, dy): (f64, f64)| -> (f64, f64) {
        let along = dx * cx + dy * cy;
        let across = cx * dy - cy * dx;
        if dx.hypot(dy) < 0.5 || (along <= 0.0 && across == 0.0) {
            (cx, cy)
        } else if along < 0.0 {
            let sign = across.signum();
            (-cy * sign, cx * sign)
        } else {
            (dx * length, dy * length)
        }
    };
    let (m0x, m0y) = tangent(p.direction);
    let (m1x, m1y) = tangent(n.direction);
    (0..=AXIS_STEPS)
        .map(|i| {
            let t = i as f64 / AXIS_STEPS as f64;
            let (t2, t3) = (t * t, t * t * t);
            let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
            let h10 = t3 - 2.0 * t2 + t;
            let h01 = -2.0 * t3 + 3.0 * t2;
            let h11 = t3 - t2;
            (
                h00 * p.wx + h10 * m0x + h01 * n.wx + h11 * m1x,
                h00 * p.wy + h10 * m0y + h01 * n.wy + h11 * m1y,
            )
        })
        .collect()
}

/// What a cut reads at one point along its segment.
#[derive(Clone, Copy, Debug)]
struct At {
    depth: f64,
    base: f64,
    half: f64,
    chan: f64,
    /// How far the belt the river has swept reaches to each side of the
    /// flow line: the train's reach by the vigour, then the channel.
    belt: f64,
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
        let reach = self.meander.as_ref().map_or(0.0, |m| m.amplitude);
        At { depth, base: lerp(self.base0, self.base1), half, chan: lerp(self.chan0, self.chan1), belt: vigour * reach + half }
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

/// The valleys a set of tiles can lie in: every channel segment of the
/// drainage cells in reach, bucketed for the search, with what each cuts;
/// every flooded node in reach with its lake, and every point of the fine
/// lattice a lake's water stands over.
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
    /// Valleys from published drainage cells, keeping the segments with an
    /// end `keep` accepts, over the envelope the cells were routed on.
    /// Nodes are looked up across every cell given, since a reach's
    /// downstream link may name a node the next cell owns; a link to no
    /// published node, the sea or the window's edge, ends the valley at the
    /// last node, where a river to the sea is at the shore.
    pub fn new(cells: &[&DrainageCell], keep: impl Fn(NodeKey) -> bool, envelope: Envelope) -> Self {
        let node = |key: NodeKey| cells.iter().find_map(|c| c.nodes.get(&key));
        // The next floor downstream is the node's own base level where its
        // downstream node is unpublished, the sea or the window's edge, and
        // never under that base: a river entering a lake grades to the
        // lake's surface, not to the lakebed beneath it.
        let vigour_of = |n: &DrainageNode| {
            let (floor_down, run) = match n.down.and_then(node) {
                Some(d) => (d.elevation - depth_at(d), (d.wx - n.wx).hypot(d.wy - n.wy)),
                None => (n.base, NODE_SPACING as f64),
            };
            vigour(n, floor_down.max(n.base), run)
        };
        let mut segments = Vec::new();
        let mut subs = Vec::new();
        let mut cuts: Vec<Cut> = Vec::new();
        let mut ends: Vec<(NodeKey, NodeKey)> = Vec::new();
        for c in cells {
            for reach in &c.reaches {
                let mut prev: Option<&DrainageNode> = None;
                for key in reach.nodes.iter().copied().chain(reach.joins) {
                    let Some(n) = node(key) else { break };
                    if let Some(p) = prev {
                        if keep(p.key) || keep(n.key) {
                            let axis = Axis::new(flow_line(p, n));
                            for (j, pair) in axis.pts.windows(2).enumerate() {
                                segments.push(Segment::along(pair[0], pair[1], true));
                                subs.push(Sub { cut: cuts.len(), start: axis.cum[j], length: axis.cum[j + 1] - axis.cum[j] });
                            }
                            let mut cut = Cut {
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
                                length: axis.length(),
                                vigour0: vigour_of(p),
                                vigour1: vigour_of(n),
                                entry: if n.catchment > 0.0 { (p.catchment / n.catchment).clamp(0.0, 1.0) } else { 1.0 },
                                meander: None,
                            };
                            cut.meander = Meander::new(&axis, p, n, &cut, envelope.seed);
                            ends.push((p.key, n.key));
                            cuts.push(cut);
                        }
                    }
                    prev = Some(n);
                }
            }
        }
        // A tributary, a segment bringing under half its end node's water,
        // ends its train where it first crosses the trunk's: the train
        // leaving the node, and the one arriving with the most water. Each
        // is searched over two wavelengths from the node.
        let mut leaving: HashMap<NodeKey, usize> = HashMap::new();
        let mut arriving: HashMap<NodeKey, usize> = HashMap::new();
        for (i, &(from, to)) in ends.iter().enumerate() {
            leaving.insert(from, i);
            if cuts[i].entry > 0.5 {
                arriving.insert(to, i);
            }
        }
        for i in 0..cuts.len() {
            if cuts[i].entry > 0.5 || cuts[i].meander.is_none() {
                continue;
            }
            let to = ends[i].1;
            let reach = |m: &Meander| 2.0 * m.wavelength;
            let trunks: Vec<Vec<(f64, f64)>> = [leaving.get(&to), arriving.get(&to)]
                .into_iter()
                .enumerate()
                .filter_map(|(which, j)| {
                    let m = cuts[*j?].meander.as_ref()?;
                    let span = reach(m);
                    Some(if which == 0 {
                        let last = m.along.partition_point(|&a| a <= span).min(m.pts.len() - 1);
                        m.pts[..=last].to_vec()
                    } else {
                        let length = *m.along.last().unwrap_or(&0.0);
                        let first = m.along.partition_point(|&a| a < length - span).saturating_sub(1);
                        m.pts[first..].to_vec()
                    })
                })
                .collect();
            let tributary = cuts[i].meander.as_mut().unwrap();
            let span = reach(tributary);
            for trunk in &trunks {
                tributary.end_at(trunk, span);
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

    /// The valleys under a square box, routed from the plate graph directly:
    /// what a view or a probe builds once. Routes every drainage cell within
    /// the box's reach, so it costs a window's routing per cell.
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
        let reach = half + NODE_SPACING as f64;
        let envelope = Envelope::new(seed, Coasts::in_box(cx, cy, reach, seed), Outlines::in_box(cx, cy, reach, seed));
        Self::new(&refs, |_| true, envelope)
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
            let valley = (at.depth * profile(u)).min((envelope - at.base).max(0.0));
            best.valley = best.valley.max(valley);
            if d <= at.belt.max(at.half) {
                let beside = cut.meander.as_ref().map_or(d, |m| m.distance(x, wx, wy));
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

/// The valleys a cell's tiles can lie in: the drainage cells under the cell
/// and its ring, keeping the segments within a valley's reach of the cell's
/// ground.
pub fn valleys_of(scope: &CellScope) -> Valleys {
    let edge_cells = scope.source_cells::<PlateEdgeIndex>();
    let coasts = Coasts::new(&scope.read::<PlateEdgeIndex>().map(|idx| idx.edges_in(&edge_cells)).unwrap_or_default());
    let envelope = Envelope::new(scope.seed(), coasts, outlines_of(scope));
    let cells = scope.source_cells::<DrainageIndex>();
    let Some(idx) = scope.read::<DrainageIndex>() else {
        return Valleys::new(&[], |_| true, envelope);
    };
    let centre = scope.lattice().cell_center(scope.cell());
    let keep_within = scope.lattice().radius as i32 + NODE_SPACING + (VALLEY_HALF_WIDTH + AXIS_SWING).ceil() as i32;
    Valleys::new(&idx.cells_in(&cells), |key| hex_distance(node_tile(key), centre) <= keep_within, envelope)
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

    /// Over every pair of downslopes, square, backward and missing ones
    /// included, the flow line runs from its first node to its second,
    /// never upstream of the first or past the second along the chord,
    /// stays within the swing of the chord, and leaves the first node along
    /// its downslope wherever that is within a right angle of the chord.
    #[test]
    fn the_flow_line_joins_its_nodes_within_the_swing() {
        let node = |wx: f64, wy: f64, direction: (f64, f64)| DrainageNode {
            key: (0, 0), q: 0, r: 0, wx, wy, elevation: 10.0, surface: 10.0,
            direction, catchment: 10.0, base: 0.0, down: None, lake: None, sill: false, age: 1.0, cut: 0.0,
        };
        let l = NODE_SPACING as f64;
        let bearings: Vec<(f64, f64)> = (0..24)
            .map(|i| {
                let a = i as f64 * std::f64::consts::PI / 12.0;
                (a.cos(), a.sin())
            })
            .chain(std::iter::once((0.0, 0.0)))
            .collect();
        for &da in &bearings {
            for &db in &bearings {
                let (p, n) = (node(0.0, 0.0, da), node(l, 0.0, db));
                let line = flow_line(&p, &n);
                assert_eq!(line.len(), AXIS_STEPS + 1);
                assert_eq!(line[0], (0.0, 0.0));
                assert!((line[AXIS_STEPS].0 - l).abs() < 1e-9 && line[AXIS_STEPS].1.abs() < 1e-9);
                for &(x, y) in &line {
                    assert!(x >= -1e-9 && x <= l + 1e-9, "the line runs past a node at {x} for {da:?} {db:?}");
                    assert!(y.abs() <= AXIS_SWING + 1e-9, "the line swings {y} for {da:?} {db:?}");
                }
                if da.0 > 1e-9 {
                    let (dx, dy) = (line[1].0 - line[0].0, line[1].1 - line[0].1);
                    let cos = (dx * da.0 + dy * da.1) / dx.hypot(dy);
                    assert!(cos > (10.0f64).to_radians().cos(), "the line leaves off its downslope for {da:?} {db:?}");
                }
            }
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
        let valleys = Valleys::new(&[&published], |_| true, Envelope::new(S, coasts, outlines));
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
        let valleys = Valleys::new(&[&published], |_| true, Envelope::new(S, coasts, outlines));
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

    /// A train between two nodes on a straight flow line crosses the line
    /// at both, never runs back along it, keeps within its amplitude by
    /// the vigour, and at full vigour is longer than the line by about its
    /// sinuosity; at no vigour there is no train, and the channel is the
    /// line.
    #[test]
    fn the_train_crosses_at_the_nodes_within_its_amplitude() {
        let node = |wx: f64, key: NodeKey| DrainageNode {
            key, q: 0, r: 0, wx, wy: 0.0, elevation: 10.0, surface: 10.0, direction: (1.0, 0.0),
            catchment: CATCHMENT_FULL, base: 0.0, down: None, lake: None, sill: false, age: 1.0, cut: 0.0,
        };
        let l = NODE_SPACING as f64;
        let (p, n) = (node(0.0, (0, 0)), node(l, (1, 0)));
        let axis = Axis::new(flow_line(&p, &n));
        assert!((axis.length() - l).abs() < 1e-9);
        let cut = |v0: f64, v1: f64| Cut {
            env0: 10.0, env1: 10.0, floor0: 5.0, floor1: 5.0, graded: false, pool: None, base0: 0.0, base1: 0.0,
            half0: CHANNEL_HALF_WIDTH_MAX, half1: CHANNEL_HALF_WIDTH_MAX, chan0: 3.0, chan1: 3.0, length: l,
            vigour0: v0, vigour1: v1, entry: 1.0, meander: None,
        };
        assert!(Meander::new(&axis, &p, &n, &cut(0.0, 0.0), S).is_none());
        let full = Meander::new(&axis, &p, &n, &cut(1.0, 1.0), S).unwrap();
        let first = full.pts[0];
        let last = full.pts[full.pts.len() - 1];
        assert!(first.0.abs() < 1e-6 && first.1.abs() < 1e-6, "the train starts at {first:?}");
        assert!((last.0 - l).abs() < 1e-6 && last.1.abs() < 1e-6, "the train ends at {last:?}");
        let mut path = 0.0;
        for pair in full.pts.windows(2) {
            assert!(pair[1].0 > pair[0].0, "the train runs back along the line at {:?}", pair[0]);
            path += (pair[1].0 - pair[0].0).hypot(pair[1].1 - pair[0].1);
        }
        for w in full.along.windows(2) {
            assert!(w[1] > w[0]);
        }
        let reach = full.pts.iter().map(|p| p.1.abs()).fold(0.0, f64::max);
        assert!(reach <= full.amplitude + 1e-9 && reach > 0.5 * full.amplitude, "reach {reach} of {}", full.amplitude);
        // Lobes alternate sides: the train returns across the line at
        // every crossing, many times over a spacing.
        let crossings = full.pts.windows(2).filter(|w| (w[0].1 > 0.0) != (w[1].1 > 0.0)).count();
        assert!(crossings >= 4, "the train crosses the line {crossings} times");
        assert!(full.pts.iter().any(|p| p.1 > 0.3 * full.amplitude) && full.pts.iter().any(|p| p.1 < -0.3 * full.amplitude));
        let (advance, _) = meander_shape();
        let sinuosity = path / l;
        assert!(sinuosity > 1.2 && sinuosity < 1.0 / advance * 1.2, "sinuosity {sinuosity} for {}", 1.0 / advance);
        let half = Meander::new(&axis, &p, &n, &cut(0.5, 0.5), S).unwrap();
        let reach = half.pts.iter().map(|p| p.1.abs()).fold(0.0, f64::max);
        assert!(reach <= 0.5 * half.amplitude + 1e-9, "half vigour reaches {reach} of {}", half.amplitude);
        for (a, b) in full.pts.iter().zip(&half.pts) {
            assert!((a.1 * 0.5 - b.1).abs() < 1e-9, "the half train is not the full one halved");
        }
    }

    /// Vigour is nothing above the channel head and on flooded ground,
    /// whole for an aged plate's trunk at the grade of the plains, nothing
    /// at a mountain stream's, falls with the grade between, and grows
    /// with age.
    #[test]
    fn vigour_falls_with_grade_and_grows_with_age() {
        let node = |catchment: f64, age: f64, lake: Option<usize>| DrainageNode {
            key: (0, 0), q: 0, r: 0, wx: 0.0, wy: 0.0, elevation: 10.0, surface: 10.0, direction: (1.0, 0.0),
            catchment, base: 0.0, down: None, lake, sill: false, age, cut: 0.0,
        };
        let run = NODE_SPACING as f64;
        let down = |n: &DrainageNode, grade: f64| n.elevation - depth_at(n) - grade * run / RISE;
        let trunk = node(CATCHMENT_FULL, 1.0, None);
        assert_eq!(vigour(&node(CHANNEL_HEAD, 1.0, None), 0.0, run), 0.0);
        assert_eq!(vigour(&node(CATCHMENT_FULL, 1.0, Some(0)), 0.0, run), 0.0);
        assert!((vigour(&trunk, down(&trunk, MEANDER_GRADE_FULL), run) - 1.0).abs() < 1e-12);
        assert!((vigour(&trunk, down(&trunk, 0.0), run) - 1.0).abs() < 1e-12);
        assert_eq!(vigour(&trunk, down(&trunk, MEANDER_GRADE_NONE), run), 0.0);
        let mut last = 1.0;
        for i in 1..=20 {
            let grade = MEANDER_GRADE_FULL + (MEANDER_GRADE_NONE - MEANDER_GRADE_FULL) * i as f64 / 20.0;
            let v = vigour(&trunk, down(&trunk, grade), run);
            assert!(v <= last, "vigour rises with grade at {grade}");
            last = v;
        }
        let young = node(CATCHMENT_FULL, 0.0, None);
        let v = vigour(&young, down(&young, 0.0), run);
        assert!(v > 0.0 && v < 1.0 && (v - YOUNG_SHARE).abs() < 1e-12, "a young plate's vigour {v}");
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
        let valleys = Valleys::new(&[&published], |_| true, Envelope::new(S, coasts, outlines));
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
