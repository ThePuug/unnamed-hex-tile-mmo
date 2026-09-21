//! MigrationEvent — where each channel runs between the nodes drainage
//! published, and what its lateral work has left.
//!
//! Drainage publishes a river as nodes a spacing apart; dissection cuts a
//! valley along it. Between those two lies the channel's planform, which
//! neither owns: the flow line through the nodes, the train the river has
//! meandered across it, and the belt it has swept doing so. This layer
//! publishes that, one entry per segment between two nodes, and moves no
//! ground: dissection reads it and cuts.
//!
//! # The flow line
//!
//! Drainage's nodes are samples of a flow line, and the chord between two
//! of them was never the river: the channel between two nodes is the curve
//! that leaves each along its own downslope, the true outflow drainage
//! publishes at every node, and arrives at the next along that node's. So a
//! river bends at the wavelength of the ground that turns it, tributaries
//! arrive at their confluence pointing downstream along the trunk's outflow,
//! and a reach turning at a node turns as a curve and never as a corner. A
//! downslope more than square to its chord is read as square: the flow
//! turns harder at that node than a curve between samples can carry, and a
//! hook toward the node would be drawn from nothing. The line is a cubic
//! between the nodes with those tangents at the chord's length, drawn as a
//! polyline, and it stays within [`AXIS_SWING`] of the chord. On a uniformly
//! tilted plain the downslope holds and the line is straight, which is
//! right: nothing there bends it.
//!
//! # The train
//!
//! A river far above its base level cuts down: its bed erodes faster than
//! its banks, and the channel holds the flow line through a gorge. A river
//! at grade, its floor falling only as fast as its load needs, cuts
//! sideways instead: the flow in any slight bend scours the outer bank and
//! builds the inner, the bend grows and walks downstream, and the river
//! sweeps a belt several bends wide, planing it level as it goes. So where
//! the floor's grade toward the next node is under the grade a river cuts
//! down at, the channel meanders across the flow line; how far, by the
//! plate's age, the same dial the depth turns on: the segment's vigour.
//!
//! The train is the state that process leaves, drawn and never run: a
//! river's planform is its bends, each at an age of its own, and ages are
//! what a hash gives. A run of the process, Howard and Knutson's bank
//! retreat stepped a few hundred times with necks and chutes cut, costs a
//! cell five times its routing and draws the same shapes. So the channel
//! is a whole number of bends between its nodes at the wavelength the
//! width sets, each bend's length its own by a hash, and each bend has an
//! age: the vigour where it lies, times the cycles from seed to cutoff a
//! full run holds, spread by the bend's hash. The fraction of the age is
//! the bend's maturity and sets its crossing angle, from nothing to the
//! angle a flood cuts a chute across the point bar at; the shape is
//! Kinoshita's curve, leaning upstream and fattening as it matures. Past
//! a whole cycle the bend has cut off: the channel across its length is a
//! young bend at the fraction's maturity, and while the fraction is small
//! the loop it left lies beside it as an oxbow, silted after. The belt
//! drifts off the line by the vigour in slow hashed waves. A river of
//! little vigour carries only young bends and fades into its flow line as
//! its vigour does; a river with none holds its flow line.
//!
//! The channel crosses the flow line at both nodes toward the side each
//! node's hash picks. The nodes pin the channel, since a segment is one
//! cell's and its neighbour must draw the same node; the bends fade over
//! a shoulder at each end so the pin leaves a straight and never a kink,
//! and the two segments' trains never tangle across the node they share.
//! A tributary's train fades toward the flow line over its last bend by
//! the share of the water it brings, so it arrives at a trunk along its
//! line; where it first crosses the trunk's train it ends, at the trunk's
//! bank, which a reader settles over its ring since a trunk's segment may
//! be another cell's.
//!
//! # The cell
//!
//! A segment is owned by the cell its start node's tile lies in, and the
//! cell reads the drainage index over its footprint and ring, which holds
//! the end node a spacing away. The reach one ring must cover is the
//! segment's: the far node, the flow line's swing beyond the chord, and the
//! valley dissection reads beside it, which sets the cell scale. Dissection
//! shares the scale, so its footprint plus one ring is the seven cells whose
//! channels can reach its tiles.

use std::any::Any;
use std::collections::HashMap;
use std::f64::consts::PI;
use std::sync::OnceLock;

use common::HexLattice;

use crate::chains::{Segment, SegmentGrid};
use super::drainage::{aged, growth, DrainageCell, DrainageIndex, DrainageNode};
use super::index::{CellId, CellIndex, EventIndex, IndexRegistry};
use super::{CellScope, TileOutput, TileView, WorldEvent, RING_CLEARANCE};
use crate::lattice::{node_site, site_at, NodeKey, NODE_SPACING, NODE_SWING};
use crate::noise::{hash_channel, hash_channel_f64};
use crate::RISE;

// ── The channel ─────────────────────────────────────────────────────────────

/// Half-width of the channel at the channel head, in tiles. A straight line
/// can pass 1/√3 of a tile from every tile centre, the hex lattice's covering
/// radius, so a narrower strip leaves gaps in a stream.
pub const CHANNEL_HALF_WIDTH_MIN: f64 = 0.6;

/// Half-width of a full trunk's channel, in tiles.
pub const CHANNEL_HALF_WIDTH_MAX: f64 = 3.5;

/// The channel's half-width at `catchment` nodes on rock of `erodibility`:
/// nothing below the channel head, then from the head's width to a trunk's
/// as the catchment grows, the way a channel's width grows with discharge.
pub fn channel_half_width(catchment: f64, erodibility: f64) -> f64 {
    growth(catchment, erodibility).map_or(0.0, |g| CHANNEL_HALF_WIDTH_MIN + (CHANNEL_HALF_WIDTH_MAX - CHANNEL_HALF_WIDTH_MIN) * g)
}

// ── The flow line ───────────────────────────────────────────────────────────

/// How far from its channel a valley reaches, in world units: half the node
/// spacing, so neighbouring valleys meet at their divide and never take each
/// other's walls. Structural, not tuned. Dissection profiles the valley over
/// it; it lives here because the channel's reach, and so the cell scale,
/// covers it.
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
pub const AXIS_STEPS: usize = 24;

/// The farthest a segment's ground lies from its start node: the far node
/// a spacing and both sites' swing away, the flow line's swing beyond the
/// chord, and the valley dissection reads beside it. What one ring of
/// cells has to cover.
pub const CHANNEL_REACH: f64 = NODE_SPACING as f64 + 2.0 * NODE_SWING + AXIS_SWING + VALLEY_HALF_WIDTH;

/// Cell scale, derived: one ring covers a segment's reach from its start
/// node.
pub const MIGRATION_CELL_SCALE: u32 = (CHANNEL_REACH / RING_CLEARANCE) as u32 + 1;

/// The flow line from node `p` to node `n`: a cubic leaving `p` along its
/// downslope and arriving at `n` along `n`'s, each tangent the chord's
/// length and turned no further than square to the chord, as
/// [`AXIS_STEPS`] steps of points.
pub fn flow_line(p: &DrainageNode, n: &DrainageNode) -> Vec<(f64, f64)> {
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

/// A flow line as a polyline: the length along it at each point, and a
/// unit normal at each, the mean of the sub-segments' meeting there, so an
/// offset from the line turns with it and never steps at a joint.
pub struct Axis {
    pub pts: Vec<(f64, f64)>,
    pub cum: Vec<f64>,
    normals: Vec<(f64, f64)>,
}

impl Axis {
    pub fn new(pts: Vec<(f64, f64)>) -> Self {
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

    pub fn length(&self) -> f64 {
        *self.cum.last().unwrap_or(&0.0)
    }

    /// The point at length `x` along the line and the normal there, found
    /// from sub-segment `from`, which follows `x` either way.
    pub fn at(&self, x: f64, from: &mut usize) -> ((f64, f64), (f64, f64)) {
        while *from + 2 < self.cum.len() && self.cum[*from + 1] < x {
            *from += 1;
        }
        while *from > 0 && self.cum[*from] > x {
            *from -= 1;
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

// ── The train ───────────────────────────────────────────────────────────────

/// A meander's wavelength along the flow line as a multiple of the
/// channel's width: Leopold and Wolman's 10.9, holding from brooks to the
/// Mississippi.
pub const MEANDER_WAVELENGTH: f64 = 10.9;

/// The angle a bend crosses the flow line at when a flood cuts a chute
/// across its point bar, in radians: a bend sinuosity of three, the
/// reciprocal of the Bessel J0 of it. Under a right angle at the crossing
/// itself, so the channel never runs back up the valley there and its
/// position along the flow line stays one-to-one.
pub const MEANDER_DEFLECTION_CUTOFF: f64 = 1.8;

/// The third harmonic's share of a bend's heading at full deflection, as
/// Kinoshita's curve carries it: the skew that leans a mature bend
/// upstream, its apex short of the middle of its path and its downstream
/// limb the longer, and the flatness that fattens its lobe, both growing
/// as the cube of the deflection. Parker and Andrews' values.
pub const MEANDER_SKEW: f64 = 1.0 / 32.0;
pub const MEANDER_FLATNESS: f64 = 1.0 / 192.0;

/// The floor's grade, as rise over run, at and under which a river is at
/// grade and sweeps its whole belt: a thousandth, the slope of the
/// meandering rivers of the plains.
pub const MEANDER_GRADE_FULL: f64 = 0.001;

/// The grade at and over which a river cuts down and holds the flow line:
/// a hundredth, where mountain streams begin.
pub const MEANDER_GRADE_NONE: f64 = 0.01;

/// How far one bend's length strays from the train's, as a share of it.
pub const MEANDER_LOBE_VARIANCE: f64 = 0.3;

/// How many times a bend at full vigour has grown from seed to cutoff over
/// the river's run, and how far one bend's count strays from another's:
/// a bend's age is the vigour times the cycles, spread by its hash from
/// this much under to this much over, so bends of every age lie along a
/// river, some new in a cutoff's straight and some at their neck, and a
/// river of little vigour has only young ones.
pub const MEANDER_CYCLES: f64 = 1.3;
pub const MEANDER_AGE_SPREAD: f64 = 0.4;

/// The share of a cycle an oxbow stays open after its cutoff before it
/// silts to a scar in the floodplain.
pub const OXBOW_SHARE: f64 = 0.4;

/// How far the belt drifts off the flow line, in wavelengths, at full
/// vigour, and the waves it drifts in: each a wavelength in wavelengths
/// and a share of the drift. Cutoffs leave the channel displaced, so a
/// river's belt wanders over its floor at a few bends' scale.
pub const MEANDER_DRIFT: f64 = 0.35;
const DRIFT_WAVES: [(f64, f64); 2] = [(2.6, 0.6), (4.1, 0.4)];

/// Points per wavelength of path the train is drawn with, and the
/// shortest step in world units. A bend's tightest radius is its path
/// over pi times the deflection, so a step this share of the path's
/// wavelength misses the tightest bend's curve by under half a tile at a
/// trunk, and a step under a tile resolves nothing a tile can show.
const MEANDER_STEPS_PER_WAVE: f64 = 24.0;
const MEANDER_STEP_MIN: f64 = 1.0;

/// How far from each node the train fades to the flow line, in
/// wavelengths: a shoulder so the pin leaves a straight and never a kink,
/// and the two segments' trains never tangle across the node they share.
const MIGRATION_TAPER: f64 = 0.75;

/// How far across the valley a channel may sweep, as a share of the
/// valley's half-width: the wall stops the bank.
const MIGRATION_ROOM: f64 = 0.8;

/// Hash channels: which side a node's crossing turns toward, a bend's
/// length, a bend's age, and the drift's phases.
const MEANDER_SIDE: u64 = 0x6d65_616e;
const MEANDER_LOBE: u64 = 0x6c6f_6265;
const MEANDER_AGE: u64 = 0x6167_6500;
const MEANDER_PHASE: u64 = 0x7068_6173;

/// A bend's heading at a share `u` of its path, for a crossing angle
/// `theta`: Kinoshita's curve over one half-wave, the sine-generated
/// heading with the third harmonic that leans the bend upstream and
/// fattens its lobe as it matures.
fn bend_heading(theta: f64, u: f64) -> f64 {
    let cube = theta * theta * theta;
    theta * (PI * u).cos() + cube * (MEANDER_SKEW * (3.0 * PI * u).sin() + MEANDER_FLATNESS * (3.0 * PI * u).cos())
}

/// One bend of unit path at crossing angle `theta`, walked in `count`
/// steps from its start: the points as (along, across), and how far it
/// advances along the flow line, which the last point's along is. The
/// same quadrature that fits a bend to its extent draws it, so a bend
/// ends where its extent says exactly. A leaning bend's heading does not
/// integrate back to the line over one half-wave, only over two, so the
/// bend is sheared to end on the line: one bend is one object, and a
/// train of bends of different sizes could not cancel each other's ends.
fn bend(theta: f64, count: usize) -> (Vec<(f64, f64)>, f64) {
    let (mut x, mut y) = (0.0, 0.0);
    let mut pts = Vec::with_capacity(count);
    for i in 0..count {
        let heading = bend_heading(theta, (i as f64 + 0.5) / count as f64);
        x += heading.cos() / count as f64;
        y += heading.sin() / count as f64;
        pts.push((x, y));
    }
    let shear = y / x;
    for p in &mut pts {
        p.1 -= shear * p.0;
    }
    (pts, x)
}

/// A bend's crossing angle at a maturity, 0 to 1: nothing for a bend
/// just started, a cutoff's at one, so a river's train fades into its
/// flow line as its vigour does and a fresh chute is straight.
fn deflection(maturity: f64) -> f64 {
    MEANDER_DEFLECTION_CUTOFF * maturity.clamp(0.0, 1.0)
}

/// A bend at cutoff over one wavelength of extent: its advance per unit
/// path, and how far it reaches to the side at its apex per unit path.
fn cutoff_shape() -> (f64, f64) {
    static SHAPE: OnceLock<(f64, f64)> = OnceLock::new();
    *SHAPE.get_or_init(|| {
        let (pts, advance) = bend(MEANDER_DEFLECTION_CUTOFF, 4096);
        let peak = pts.iter().map(|p| p.1.abs()).fold(0.0, f64::max);
        (advance, peak)
    })
}

/// How far a train whose wavelength along the flow line is `wavelength`
/// can reach to either side of the line, in world units: its longest bend
/// at cutoff, and the belt's drift. What the belt a river at grade has
/// swept is wide to each side, before its channel. A channel of width `w`
/// has a wavelength of [`MEANDER_WAVELENGTH`] times `w`, fitted between
/// its nodes.
pub fn meander_amplitude(wavelength: f64) -> f64 {
    let (advance, peak) = cutoff_shape();
    (1.0 + MEANDER_LOBE_VARIANCE) * 0.5 * wavelength * peak / advance + MEANDER_DRIFT * wavelength
}

/// How far the river at a node has turned to its banks, 0 to 1: nothing
/// above the channel head or on flooded ground; else the plate's aged
/// share, by the bank's erodibility, by how far the floor's grade over
/// `run` to `floor_down`, the next floor downstream, lies under the grade
/// a river cuts down at, whole at the grade of the plains and nothing at a
/// mountain stream's. A river in shale sweeps its belt; the same river
/// across basement holds its line.
pub fn vigour(node: &DrainageNode, floor_down: f64, run: f64) -> f64 {
    if node.flooded || growth(node.catchment, node.erodibility).is_none() {
        return 0.0;
    }
    let grade = (node.floor - floor_down).max(0.0) * RISE / run;
    let g = ((grade - MEANDER_GRADE_FULL) / (MEANDER_GRADE_NONE - MEANDER_GRADE_FULL)).clamp(0.0, 1.0);
    aged(node.age) * node.erodibility * (1.0 - g * g * (3.0 - 2.0 * g))
}

/// A channel's train across its flow line: the live channel in world
/// space, the loops it has cut off, and both bucketed for the search; how
/// far it has strayed from the line, what the belt it swept is wide to
/// each side before the channel's own half-width; and the wavelength its
/// width set.
#[derive(Clone, Debug)]
pub struct Train {
    pub pts: Vec<(f64, f64)>,
    pub oxbows: Vec<Vec<(f64, f64)>>,
    grid: SegmentGrid,
    pub amplitude: f64,
    pub wavelength: f64,
}

impl Train {
    /// The train of `channel` across `axis`, or none where neither end
    /// meanders or no channel runs.
    pub fn new(axis: &Axis, channel: &Channel, seed: u64) -> Option<Self> {
        let width = channel.half0 + channel.half1;
        if width <= 0.0 || (channel.vigour0 <= 0.0 && channel.vigour1 <= 0.0) {
            return None;
        }
        let length = axis.length();
        let (frame, oxbows, wavelength) = drawn_train(length, width, channel, seed);
        let room = MIGRATION_ROOM * VALLEY_HALF_WIDTH;
        let mut amplitude: f64 = 0.0;
        let place = |line: &[(f64, f64)], amplitude: &mut f64| -> Vec<(f64, f64)> {
            let mut from = 0;
            line.iter()
                .map(|&(x, y)| {
                    let y = y.clamp(-room, room);
                    *amplitude = amplitude.max(y.abs());
                    let (point, normal) = axis.at(x.clamp(0.0, length), &mut from);
                    (point.0 + y * normal.0, point.1 + y * normal.1)
                })
                .collect()
        };
        let pts = place(&frame, &mut amplitude);
        let oxbows: Vec<Vec<(f64, f64)>> = oxbows.iter().map(|o| place(o, &mut amplitude)).collect();
        let grid = Self::bucket(&pts, &oxbows);
        Some(Train { pts, oxbows, grid, amplitude, wavelength })
    }

    /// The live channel and the oxbows as segments in buckets of a few
    /// channel widths, so the slot's half-width is found in one ring.
    fn bucket(pts: &[(f64, f64)], oxbows: &[Vec<(f64, f64)>]) -> SegmentGrid {
        let segments: Vec<Segment> = std::iter::once(pts)
            .chain(oxbows.iter().map(|o| o.as_slice()))
            .flat_map(|line| line.windows(2).map(|w| Segment::along(w[0], w[1], true)))
            .collect();
        SegmentGrid::new(segments, 4.0 * CHANNEL_HALF_WIDTH_MAX)
    }

    /// End this train where it first crosses the `trunk` points, walking
    /// downstream from its start: a tributary is captured wherever the
    /// trunk's train has swept across it, joins the trunk at the trunk's
    /// bank there, and runs no further; the oxbows its lost reach left
    /// within two wavelengths of the capture go with it, since the trunk's
    /// belt swept that ground. Untouched when the two never cross before
    /// the node, where both trains meet in any case.
    pub fn end_at(&mut self, trunk: &[(f64, f64)]) {
        for i in 0..self.pts.len().saturating_sub(1) {
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
            if let Some((_, point)) = hit {
                self.pts.truncate(i + 1);
                self.pts.push(point);
                let reach = 2.0 * self.wavelength;
                self.oxbows.retain(|o| {
                    let n = o.len() as f64;
                    let (cx, cy) = o.iter().fold((0.0, 0.0), |(x, y), p| (x + p.0 / n, y + p.1 / n));
                    (cx - point.0).hypot(cy - point.1) > reach
                });
                self.grid = Self::bucket(&self.pts, &self.oxbows);
                return;
            }
        }
    }

    /// The last `reach` of the channel, walked back from its end: what a
    /// tributary's train is tested against.
    pub fn tail(&self, reach: f64) -> &[(f64, f64)] {
        let mut first = self.pts.len().saturating_sub(1);
        let mut walked = 0.0;
        while first > 0 && walked < reach {
            walked += (self.pts[first].0 - self.pts[first - 1].0).hypot(self.pts[first].1 - self.pts[first - 1].1);
            first -= 1;
        }
        &self.pts[first..]
    }

    /// The first `reach` of the channel from its start.
    pub fn head(&self, reach: f64) -> &[(f64, f64)] {
        let mut last = 0;
        let mut walked = 0.0;
        while last + 1 < self.pts.len() && walked < reach {
            walked += (self.pts[last + 1].0 - self.pts[last].0).hypot(self.pts[last + 1].1 - self.pts[last].1);
            last += 1;
        }
        &self.pts[..=last]
    }

    /// The distance from a position to the channel or an oxbow, when one
    /// lies within a trunk's channel and a margin; else more than any slot
    /// is wide.
    pub fn distance(&self, wx: f64, wy: f64) -> f64 {
        self.grid.nearest(wx, wy, CHANNEL_HALF_WIDTH_MAX + 1.0).map_or(f64::MAX, |n| n.distance)
    }
}

/// The train in the flow line's frame, as (along, across) in world units,
/// with the oxbows its cutoffs left and the fitted wavelength. The channel
/// crosses the line at both nodes toward the side each node's hash picks,
/// in a whole number of bends at the wavelength the channel's width sets,
/// each bend's extent along the line its own by a hash. Each bend has an
/// age: the vigour where it lies, times the cycles a full run holds,
/// spread by its hash. The fraction of the age is the bend's maturity,
/// its crossing angle between a seed's and a cutoff's; a whole cycle
/// elapsed means it has cut off, the channel across its extent is a young
/// bend at that maturity, and while the phase is fresh the loop it left
/// lies beside it as an oxbow. The belt drifts off the line by the vigour
/// in two hashed waves. At each node the train fades to the line over a
/// shoulder, and over its last bend by `entry`, the share of the end
/// node's water the channel brings, so a tributary arrives at a trunk
/// along its flow line.
fn drawn_train(length: f64, width: f64, channel: &Channel, seed: u64) -> (Vec<(f64, f64)>, Vec<Vec<(f64, f64)>>, f64) {
    let (from, to) = (channel.from, channel.to);
    let hash = |id: u64| hash_channel_f64(from.0 as i64, from.1 as i64, seed, id);
    let side = |k: NodeKey| if hash_channel(k.0 as i64, k.1 as i64, seed, MEANDER_SIDE) & 1 == 0 { 1.0 } else { -1.0 };
    let (sign, sign1) = (side(from), side(to));
    // Bends between the nodes: the count nearest the wavelength whose
    // parity turns the crossing at the end the way its hash says.
    let wavelength = MEANDER_WAVELENGTH * width;
    let target = 2.0 * length / wavelength;
    let even = sign == sign1;
    let mut k = target.round().max(1.0) as u64;
    if (k % 2 == 0) != even {
        k = if k > 1 && (target - (k - 1) as f64).abs() <= ((k + 1) as f64 - target).abs() { k - 1 } else { k + 1 };
    }
    let wavelength = 2.0 * length / k as f64;
    // Each bend's extent along the line: its share of the length.
    let mut extents: Vec<f64> = (0..k).map(|j| 1.0 + MEANDER_LOBE_VARIANCE * (2.0 * hash(MEANDER_LOBE.wrapping_add(j)) - 1.0)).collect();
    let total: f64 = extents.iter().sum();
    for e in &mut extents {
        *e *= length / total;
    }
    let taper = MIGRATION_TAPER * wavelength;
    let shoulder = |x: f64| {
        let u = (x / taper).clamp(0.0, 1.0);
        u * u * (3.0 - 2.0 * u)
    };
    let shoulders = |x: f64| shoulder(x) * shoulder(length - x);
    let envelope = |x: f64| {
        let t = (x / length).clamp(0.0, 1.0);
        (channel.vigour0 + t * (channel.vigour1 - channel.vigour0)) * shoulders(x)
    };
    let phases = [hash(MEANDER_PHASE), hash(MEANDER_PHASE ^ 1)];
    let drift = |x: f64| {
        let d: f64 = DRIFT_WAVES.iter().zip(phases).map(|(&(wave, share), phase)| share * (2.0 * PI * (x / (wave * wavelength) + phase)).sin()).sum();
        MEANDER_DRIFT * wavelength * envelope(x) * d
    };
    let step = (wavelength / MEANDER_STEPS_PER_WAVE).max(MEANDER_STEP_MIN);
    let last = extents.len() - 1;
    let mut pts = vec![(0.0, drift(0.0))];
    let mut oxbows = Vec::new();
    let mut x0 = 0.0;
    for (j, &extent) in extents.iter().enumerate() {
        let bend_sign = if j % 2 == 0 { sign } else { -sign };
        let age = envelope(x0 + 0.5 * extent) * MEANDER_CYCLES * (1.0 + MEANDER_AGE_SPREAD * (2.0 * hash(MEANDER_AGE.wrapping_add(j as u64)) - 1.0));
        let maturity = age.fract();
        // A bend fitted to its extent: its path is the extent over the
        // unit bend's advance, drawn in steps of the path, the offset
        // faded at the shoulders and, on the last bend, by the entry.
        let draw = |theta: f64| -> Vec<(f64, f64)> {
            let (_, advance) = bend(theta, 64);
            let count = ((extent / advance / step).ceil().max(1.0)) as usize;
            let (unit, advance) = bend(theta, count);
            let path = extent / advance;
            unit.iter()
                .enumerate()
                .map(|(i, &(ux, uy))| {
                    let x = x0 + ux * path;
                    let end = (i as f64 + 1.0) / count as f64;
                    let entry = if j == last { 1.0 - (1.0 - channel.entry) * end * end * (3.0 - 2.0 * end) } else { 1.0 };
                    (x, (bend_sign * uy * path * shoulders(x) + drift(x)) * entry)
                })
                .collect()
        };
        if age >= 1.0 && maturity < OXBOW_SHARE {
            let mut oxbow = vec![*pts.last().unwrap()];
            oxbow.extend(draw(MEANDER_DEFLECTION_CUTOFF));
            oxbow.push(oxbow[0]);
            oxbows.push(oxbow);
        }
        pts.extend(draw(deflection(maturity)));
        x0 += extent;
    }
    (pts, oxbows, wavelength)
}

// ── The index ───────────────────────────────────────────────────────────────

/// One channel segment, from a node to the node its water goes to next.
#[derive(Clone, Debug)]
pub struct Channel {
    pub from: NodeKey,
    pub to: NodeKey,
    /// The flow line from `from` to `to`, [`AXIS_STEPS`] + 1 points.
    pub axis: Vec<(f64, f64)>,
    /// The train across the flow line, or none where the river holds it.
    pub train: Option<Train>,
    /// The channel's half-width at each end, in tiles.
    pub half0: f64,
    pub half1: f64,
    /// How far the river has worked its banks at each end, 0 to 1.
    pub vigour0: f64,
    pub vigour1: f64,
    /// The share of the end node's water the start node brings: near one
    /// along a channel, small for a tributary entering a trunk.
    pub entry: f64,
}

#[derive(Clone, Debug, Default)]
pub struct MigrationCell {
    pub channels: Vec<Channel>,
}

#[derive(Default)]
pub struct ChannelIndex {
    pub cells: HashMap<CellId, MigrationCell>,
}

impl ChannelIndex {
    pub fn lattice() -> HexLattice {
        HexLattice::new(MIGRATION_CELL_SCALE)
    }

    /// The published cells among `cell_ids`: what a reader gathers over its
    /// footprint and ring.
    pub fn cells_in(&self, cell_ids: &[CellId]) -> Vec<&MigrationCell> {
        cell_ids.iter().filter_map(|id| self.cells.get(id)).collect()
    }
}

impl CellIndex for ChannelIndex {
    type Cell = MigrationCell;

    fn set(&mut self, cell: CellId, entry: Self::Cell) {
        self.cells.insert(cell, entry);
    }
}

impl EventIndex for ChannelIndex {
    fn source_scale(&self) -> u32 { MIGRATION_CELL_SCALE }

    /// Each channel at its start node's tile.
    fn tiles(&self, cell_ids: &[CellId]) -> Vec<(i32, i32)> {
        cell_ids
            .iter()
            .filter_map(|id| self.cells.get(id))
            .flat_map(|c| c.channels.iter().map(|ch| node_site(ch.from)))
            .collect()
    }

    /// Downstream: the tile of the node a channel starting here ends at.
    fn neighbors(&self, q: i32, r: i32) -> Vec<(i32, i32)> {
        let Some(key) = site_at(q, r) else { return Vec::new() };
        self.cells
            .get(&Self::lattice().cell_id(q, r))
            .and_then(|c| c.channels.iter().find(|ch| ch.from == key))
            .map(|ch| vec![node_site(ch.to)])
            .unwrap_or_default()
    }

    fn remove_cell(&mut self, cell_id: CellId) {
        self.cells.remove(&cell_id);
    }
}

/// The channels of the reaches in `cells` whose start node `owns` accepts:
/// each pair of consecutive nodes along a reach, the last with the node it
/// joins, looked up across every cell given since a reach's downstream
/// link may name a node the next cell owns; a link to no published node,
/// the sea or the window's edge, ends the channel at the last node.
pub fn channels(cells: &[&DrainageCell], owns: impl Fn(&DrainageNode) -> bool, seed: u64) -> Vec<Channel> {
    let node = |key: NodeKey| cells.iter().find_map(|c| c.nodes.get(&key));
    // The next floor downstream is the node's own base level where its
    // downstream node is unpublished, the sea or the window's edge, and
    // never under that base: a river entering closed ground grades to the
    // fill, not to the floor beneath it.
    let vigour_of = |n: &DrainageNode| {
        let (floor_down, run) = match n.down.and_then(node) {
            Some(d) => (d.floor, (d.wx - n.wx).hypot(d.wy - n.wy)),
            None => (n.base, NODE_SPACING as f64),
        };
        vigour(n, floor_down.max(n.base), run)
    };
    let mut out = Vec::new();
    for c in cells {
        for reach in &c.reaches {
            let mut prev: Option<&DrainageNode> = None;
            for key in reach.nodes.iter().copied().chain(reach.joins) {
                let Some(n) = node(key) else { break };
                if let Some(p) = prev.filter(|p| owns(p)) {
                    let axis = Axis::new(flow_line(p, n));
                    let mut channel = Channel {
                        from: p.key,
                        to: n.key,
                        axis: Vec::new(),
                        train: None,
                        half0: channel_half_width(p.catchment, p.erodibility),
                        half1: channel_half_width(n.catchment, n.erodibility),
                        vigour0: vigour_of(p),
                        vigour1: vigour_of(n),
                        entry: if n.catchment > 0.0 { (p.catchment / n.catchment).clamp(0.0, 1.0) } else { 1.0 },
                    };
                    channel.train = Train::new(&axis, &channel, seed);
                    channel.axis = axis.pts;
                    out.push(channel);
                }
                prev = Some(n);
            }
        }
    }
    out
}

// ── The event ───────────────────────────────────────────────────────────────

pub struct MigrationEvent;

impl MigrationEvent {
    pub fn new() -> Self { MigrationEvent }
}

impl Default for MigrationEvent {
    fn default() -> Self { Self::new() }
}

impl WorldEvent for MigrationEvent {
    fn name(&self) -> &str { "migration" }
    fn scale(&self) -> u32 { MIGRATION_CELL_SCALE }

    /// A channel reaches its far node, swings beyond the chord, and has a
    /// valley read beside it.
    fn max_influence(&self) -> u32 { CHANNEL_REACH.ceil() as u32 }

    fn register_indexes(&self, registry: &mut IndexRegistry) {
        registry.pre_register::<ChannelIndex>();
    }

    /// The channels starting at the nodes whose tiles lie in this cell,
    /// from the drainage cells under the footprint and ring, which hold
    /// every end node a spacing away.
    fn deform(&self, scope: &CellScope) {
        let cells = scope.source_cells::<DrainageIndex>();
        let lattice = scope.lattice();
        let cell = scope.cell();
        let channels = scope
            .read::<DrainageIndex>()
            .map(|idx| channels(&idx.cells_in(&cells), |p| lattice.cell_id(p.q, p.r) == cell, scope.seed()))
            .unwrap_or_default();
        scope.publish::<ChannelIndex>(MigrationCell { channels });
    }

    /// Nothing per tile: dissection cuts along what this publishes.
    fn query(
        &self,
        _q: i32,
        _r: i32,
        _below: &TileView,
        _cell: &(dyn Any + Send + Sync),
        _seed: u64,
    ) -> Option<TileOutput> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::drainage::{CATCHMENT_FULL, CHANNEL_HEAD, YOUNG_SHARE};
    use std::collections::HashSet;

    const S: u64 = 0x9E3779B97F4A7C15;

    fn node(wx: f64, wy: f64, key: NodeKey, direction: (f64, f64), catchment: f64, age: f64, flooded: bool) -> DrainageNode {
        DrainageNode {
            key, q: 0, r: 0, wx, wy, elevation: 10.0, surface: 10.0, flooded, direction, catchment, base: 0.0,
            down: None, sill: false, age, erodibility: 1.0, cut: 0.0, floor: 5.0,
        }
    }

    /// The channel is nothing at the head, a tile just past it, grows with
    /// catchment, and saturates at a trunk's.
    #[test]
    fn channel_width_starts_at_the_head_and_saturates() {
        assert_eq!(channel_half_width(CHANNEL_HEAD, 1.0), 0.0);
        assert!((channel_half_width(CHANNEL_HEAD + 1e-9, 1.0) - CHANNEL_HALF_WIDTH_MIN).abs() < 1e-3);
        let mut last = 0.0;
        for i in 0..200 {
            let w = channel_half_width(CHANNEL_HEAD + i as f64 * 0.5, 1.0);
            assert!(w >= last, "channel narrows");
            last = w;
        }
        assert_eq!(channel_half_width(10.0 * CATCHMENT_FULL, 1.0), CHANNEL_HALF_WIDTH_MAX);
        // On hard rock the head lies further out: no channel yet where
        // shale has one.
        assert_eq!(channel_half_width(CHANNEL_HEAD + 1.0, 0.3), 0.0);
        assert!(channel_half_width(2.0 * CATCHMENT_FULL, 0.3) > 0.0);
    }

    /// Over every pair of downslopes, square, backward and missing ones
    /// included, the flow line runs from its first node to its second,
    /// never upstream of the first or past the second along the chord,
    /// stays within the swing of the chord, and leaves the first node along
    /// its downslope wherever that is within a right angle of the chord.
    #[test]
    fn the_flow_line_joins_its_nodes_within_the_swing() {
        let l = NODE_SPACING as f64;
        let bearings: Vec<(f64, f64)> = (0..24)
            .map(|i| {
                let a = i as f64 * PI / 12.0;
                (a.cos(), a.sin())
            })
            .chain(std::iter::once((0.0, 0.0)))
            .collect();
        for &da in &bearings {
            for &db in &bearings {
                let (p, n) = (node(0.0, 0.0, (0, 0), da, 10.0, 1.0, false), node(l, 0.0, (1, 0), db, 10.0, 1.0, false));
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

    /// A trunk's channel between two nodes on a straight flow line: at no
    /// vigour there is no train and the channel is the line; drawn, it
    /// still runs from node to node, stays inside the valley, is longer
    /// than the line by a river's sinuosity, has cut off bends at full
    /// vigour and none at a little, and has swept a belt that is narrower
    /// at half vigour; and it is the same train twice.
    #[test]
    fn a_train_joins_its_nodes_and_grows_with_its_vigour() {
        let l = NODE_SPACING as f64;
        let (p, n) = (
            node(0.0, 0.0, (0, 0), (1.0, 0.0), CATCHMENT_FULL, 1.0, false),
            node(l, 0.0, (1, 0), (1.0, 0.0), CATCHMENT_FULL, 1.0, false),
        );
        let axis = Axis::new(flow_line(&p, &n));
        assert!((axis.length() - l).abs() < 1e-9);
        let channel = |v0: f64, v1: f64| Channel {
            from: p.key, to: n.key, axis: Vec::new(), train: None,
            half0: CHANNEL_HALF_WIDTH_MAX, half1: CHANNEL_HALF_WIDTH_MAX, vigour0: v0, vigour1: v1, entry: 1.0,
        };
        assert!(Train::new(&axis, &channel(0.0, 0.0), S).is_none());
        let sinuosity = |t: &Train| t.pts.windows(2).map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1)).sum::<f64>() / l;
        let full = Train::new(&axis, &channel(1.0, 1.0), S).unwrap();
        let first = full.pts[0];
        let last = full.pts[full.pts.len() - 1];
        assert!(first.0.abs() < 1e-6 && first.1.abs() < 1e-6, "the train starts at {first:?}");
        assert!((last.0 - l).abs() < 1e-6 && last.1.abs() < 1e-6, "the train ends at {last:?}");
        for line in std::iter::once(&full.pts).chain(&full.oxbows) {
            for &(x, y) in line {
                assert!(x > -VALLEY_HALF_WIDTH && x < l + VALLEY_HALF_WIDTH && y.abs() <= VALLEY_HALF_WIDTH, "the channel left the valley at ({x}, {y})");
            }
        }
        assert!(full.amplitude > 0.0 && full.amplitude <= VALLEY_HALF_WIDTH);
        let s_full = sinuosity(&full);
        assert!(s_full > 1.3, "a trunk at full vigour is only {s_full} sinuous");
        assert!(!full.oxbows.is_empty(), "a trunk at full vigour cut off no bend");
        assert!(full.amplitude <= meander_amplitude(full.wavelength) + 1e-9, "the belt {} is wider than the stated amplitude {}", full.amplitude, meander_amplitude(full.wavelength));
        let little = Train::new(&axis, &channel(0.2, 0.2), S).unwrap();
        let half = Train::new(&axis, &channel(0.5, 0.5), S).unwrap();
        assert!(little.oxbows.is_empty(), "a river of little vigour cut off a bend");
        assert!(little.amplitude < half.amplitude && half.amplitude < full.amplitude, "the belt does not widen with vigour: {} {} {}", little.amplitude, half.amplitude, full.amplitude);
        assert!(sinuosity(&little) > 1.0 && sinuosity(&little) < s_full, "little vigour is not straighter: {} for {s_full}", sinuosity(&little));
        let again = Train::new(&axis, &channel(1.0, 1.0), S).unwrap();
        assert_eq!(full.pts, again.pts, "the same channel drawn twice differs");
        // Every point of the channel is within the slot's reach of itself.
        for &(x, y) in &full.pts {
            assert!(full.distance(x, y) < 1e-9);
        }
    }

    /// A bend returns to the line at its end, advances less and reaches
    /// further the more it matures, and a mature bend leans upstream: its
    /// apex lies short of the middle of its path, where a young bend's
    /// lies at it. The cutoff bend has a sinuosity of three.
    #[test]
    fn a_bend_matures_from_a_wiggle_to_a_leaning_loop() {
        let apex = |pts: &[(f64, f64)]| {
            let (i, _) = pts.iter().enumerate().max_by(|a, b| a.1 .1.abs().total_cmp(&b.1 .1.abs())).unwrap();
            (i as f64 + 1.0) / pts.len() as f64
        };
        let (mut last_advance, mut last_peak) = (2.0, -1.0);
        for i in 0..=10 {
            let (pts, advance) = bend(deflection(i as f64 / 10.0), 400);
            let end = pts.last().unwrap();
            assert!(end.1.abs() < 1e-3, "a bend ends {} off the line at maturity {i}", end.1);
            let peak = pts.iter().map(|p| p.1.abs()).fold(0.0, f64::max);
            assert!(advance < last_advance && peak > last_peak, "a bend at maturity {i} advances {advance} for {last_advance} and reaches {peak} for {last_peak}");
            last_advance = advance;
            last_peak = peak;
        }
        let (young, _) = bend(deflection(0.1), 400);
        let (mature, advance) = bend(deflection(1.0), 400);
        assert!((apex(&young) - 0.5).abs() < 0.02, "a young bend's apex at {}", apex(&young));
        assert!(apex(&mature) < 0.49, "a mature bend's apex at {}, not upstream", apex(&mature));
        assert!((1.0 / advance - 3.0).abs() < 0.3, "the cutoff bend's sinuosity is {}", 1.0 / advance);
    }

    /// Vigour is nothing above the channel head and on flooded ground,
    /// whole for an aged plate's trunk at the grade of the plains, nothing
    /// at a mountain stream's, falls with the grade between, and grows
    /// with age.
    #[test]
    fn vigour_falls_with_grade_and_grows_with_age() {
        let run = NODE_SPACING as f64;
        let down = |n: &DrainageNode, grade: f64| n.floor - grade * run / RISE;
        let at = |catchment: f64, age: f64, flooded: bool| node(0.0, 0.0, (0, 0), (1.0, 0.0), catchment, age, flooded);
        let trunk = at(CATCHMENT_FULL, 1.0, false);
        assert_eq!(vigour(&at(CHANNEL_HEAD, 1.0, false), 0.0, run), 0.0);
        assert_eq!(vigour(&at(CATCHMENT_FULL, 1.0, true), 0.0, run), 0.0);
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
        let young = at(CATCHMENT_FULL, 0.0, false);
        let v = vigour(&young, down(&young, 0.0), run);
        assert!(v > 0.0 && v < 1.0 && (v - YOUNG_SHARE).abs() < 1e-12, "a young plate's vigour {v}");
    }

    /// A routed cell's channels: one from every node with a downstream
    /// node but a basin's pit, each owned by the cell its start node lies
    /// in, its flow line running from the start node to the end node.
    #[test]
    fn a_cell_publishes_the_channels_starting_in_it() {
        use super::super::drainage::{DrainageEvent, DrainageIndex};
        use super::super::plates::Coasts;
        use super::super::thrusting::Outlines;
        use crate::hex_to_world;
        let lattice = DrainageIndex::lattice();
        let cell = lattice.cell_id(-58_204, 4_907);
        let (cq, cr) = lattice.cell_center(cell);
        let (cx, cy) = hex_to_world(cq, cr);
        let window = (3 * lattice.radius + 1) as f64;
        let coasts = Coasts::in_box(cx, cy, window, S);
        let outlines = Outlines::in_box(cx, cy, window, S);
        let published = DrainageEvent::new().route(&lattice, cell, S, &coasts, &outlines).owned_cell();
        let fine = ChannelIndex::lattice();
        let own = fine.cell_id(-58_204, 4_907);
        let all = channels(&[&published], |_| true, S);
        let owned = channels(&[&published], |p| fine.cell_id(p.q, p.r) == own, S);
        assert!(!all.is_empty() && !owned.is_empty() && owned.len() < all.len());
        let from: HashSet<NodeKey> = all.iter().map(|c| c.from).collect();
        // Every node with its next node in the cell starts a channel, but a
        // basin's pit: its water leaves over the sill, and the river ends.
        for n in published.nodes.values().filter(|n| n.down.map_or(false, |d| published.nodes.contains_key(&d))) {
            let pit = n.flooded && !published.nodes[&n.down.unwrap()].flooded;
            assert_eq!(from.contains(&n.key), !pit, "channel from {:?}, pit {pit}", n.key);
        }
        for ch in &all {
            let (p, n) = (&published.nodes[&ch.from], &published.nodes[&ch.to]);
            assert_eq!(ch.axis.len(), AXIS_STEPS + 1);
            assert_eq!(ch.axis[0], (p.wx, p.wy));
            assert!((ch.axis[AXIS_STEPS].0 - n.wx).abs() < 1e-6 && (ch.axis[AXIS_STEPS].1 - n.wy).abs() < 1e-6);
            if let Some(train) = &ch.train {
                assert!((train.pts[0].0 - p.wx).abs() < 1e-6 && (train.pts[0].1 - p.wy).abs() < 1e-6);
            }
        }
        for ch in &owned {
            assert_eq!(fine.cell_id(published.nodes[&ch.from].q, published.nodes[&ch.from].r), own);
        }
    }
}

