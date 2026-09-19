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
//! The train drawn here is a stand-in for that process: the sine-generated
//! curve, the heading swinging as a sine of the distance along the channel,
//! with the wavelength a channel's width sets and each bend's length its
//! own by a hash, its size following. It is the equilibrium shape of one
//! bend stamped along the line, so it is regular where a river is not. The
//! migration that makes a train irregular, bends growing, walking
//! downstream and cutting off at the neck, is unbuilt; it replaces this
//! train and nothing that reads it.
//!
//! The channel crosses the flow line at every node, so a train fits between
//! two nodes as a whole number of half-waves, and it crosses toward the
//! side a hash of the node picks, so the train continues through a node
//! whichever cell drew each side. A tributary's train fades toward the flow
//! line over its last bend by the share of the water it brings, so it
//! arrives along its flow line; where it first crosses the trunk's train it
//! ends, at the trunk's bank, which a reader settles over its ring since a
//! trunk's segment may be another cell's.
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

use super::drainage::{aged, growth, DrainageCell, DrainageIndex, DrainageNode};
use super::index::{CellId, CellIndex, EventIndex, IndexRegistry};
use super::{CellScope, TileOutput, TileView, WorldEvent, RING_CLEARANCE};
use crate::lattice::{node_tile, NodeKey, NODE_SPACING};
use crate::noise::{hash_channel, hash_channel_f64};
use crate::RISE;

// ── The channel ─────────────────────────────────────────────────────────────

/// Half-width of the channel at the channel head, in tiles. A straight line
/// can pass 1/√3 of a tile from every tile centre, the hex lattice's covering
/// radius, so a narrower strip leaves gaps in a stream.
pub const CHANNEL_HALF_WIDTH_MIN: f64 = 0.6;

/// Half-width of a full trunk's channel, in tiles.
pub const CHANNEL_HALF_WIDTH_MAX: f64 = 3.5;

/// The channel's half-width at `catchment` nodes: nothing below the channel
/// head, then from the head's width to a trunk's as the catchment grows, the
/// way a channel's width grows with discharge.
pub fn channel_half_width(catchment: f64) -> f64 {
    growth(catchment).map_or(0.0, |g| CHANNEL_HALF_WIDTH_MIN + (CHANNEL_HALF_WIDTH_MAX - CHANNEL_HALF_WIDTH_MIN) * g)
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
/// a spacing away, the flow line's swing beyond the chord, and the valley
/// dissection reads beside it. What one ring of cells has to cover.
pub const CHANNEL_REACH: f64 = NODE_SPACING as f64 + AXIS_SWING + VALLEY_HALF_WIDTH;

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
    /// from sub-segment `from` onward, which advances with `x`.
    pub fn at(&self, x: f64, from: &mut usize) -> ((f64, f64), (f64, f64)) {
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

// ── The train ───────────────────────────────────────────────────────────────

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

/// How far the river at a node has turned to its banks, 0 to 1: nothing
/// above the channel head or on flooded ground; else the plate's aged
/// share by how far the floor's grade over `run` to `floor_down`, the next
/// floor downstream, lies under the grade a river cuts down at, whole at
/// the grade of the plains and nothing at a mountain stream's.
pub fn vigour(node: &DrainageNode, floor_down: f64, run: f64) -> f64 {
    if node.lake.is_some() || growth(node.catchment).is_none() {
        return 0.0;
    }
    let grade = (node.floor - floor_down).max(0.0) * RISE / run;
    let g = ((grade - MEANDER_GRADE_FULL) / (MEANDER_GRADE_NONE - MEANDER_GRADE_FULL)).clamp(0.0, 1.0);
    aged(node.age) * (1.0 - g * g * (3.0 - 2.0 * g))
}

/// A channel's train laid across its flow line: its points in world space
/// and each one's position along the line, ascending, so the points near
/// a position along the line are found by that position; and how far it
/// reaches to each side at full vigour, its largest lobe included, before
/// the channel's own half-width.
#[derive(Clone, Debug)]
pub struct Train {
    pub along: Vec<f64>,
    pub pts: Vec<(f64, f64)>,
    pub step: f64,
    pub amplitude: f64,
    /// The wavelength along the flow line, as fitted between the nodes.
    pub wavelength: f64,
}

impl Train {
    /// The train of `channel` across `axis`, or none where neither end
    /// meanders or no channel runs: the sine-generated curve crossing the
    /// line at both nodes, toward the side each node's hash picks, a whole
    /// number of half-waves at the wavelength the channel's width sets,
    /// each bend's length its own by a hash and its size following, the
    /// whole scaled by the vigour along the line. Over its last bend the
    /// train fades toward the line by the share of the end node's water
    /// the channel brings, so a tributary arrives at a trunk along its flow
    /// line and a channel continuing through a node keeps its train.
    fn new(axis: &Axis, channel: &Channel, seed: u64) -> Option<Self> {
        let width = channel.half0 + channel.half1;
        if width <= 0.0 || (channel.vigour0 <= 0.0 && channel.vigour1 <= 0.0) {
            return None;
        }
        let length = axis.length();
        let side = |key: NodeKey| if hash_channel(key.0 as i64, key.1 as i64, seed, MEANDER_SIDE) & 1 == 0 { 1.0 } else { -1.0 };
        let (sign, sign1) = (side(channel.from), side(channel.to));
        // Half-waves between the nodes: the count nearest the wavelength
        // whose parity turns the crossing at the end the way its hash says.
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
            .map(|j| {
                let u = hash_channel_f64(channel.from.0 as i64, channel.from.1 as i64, seed, MEANDER_LOBE.wrapping_add(j));
                1.0 + MEANDER_LOBE_VARIANCE * (2.0 * u - 1.0)
            })
            .collect();
        let total: f64 = bends.iter().sum();
        for b in &mut bends {
            *b *= path / total;
        }
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
                let fade = if j + 1 == bends.len() { 1.0 - (1.0 - channel.entry) * end * end * (3.0 - 2.0 * end) } else { 1.0 };
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
            let vigour = channel.vigour0 + t * (channel.vigour1 - channel.vigour0);
            let offset = y * fade * vigour;
            let (point, normal) = axis.at(x, &mut from);
            along.push(x);
            pts.push((point.0 + offset * normal.0, point.1 + offset * normal.1));
        }
        Some(Train { along, pts, step, amplitude, wavelength })
    }

    /// End this train where it first crosses the `trunk` points, walking
    /// downstream over its last `reach` of line: a tributary joins the
    /// trunk at the trunk's bank and runs no further. Untouched when the
    /// two never cross before the node, where both trains meet in any
    /// case.
    pub fn end_at(&mut self, trunk: &[(f64, f64)], reach: f64) {
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
    pub fn distance(&self, x: f64, wx: f64, wy: f64) -> f64 {
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
            .flat_map(|c| c.channels.iter().map(|ch| node_tile(ch.from)))
            .collect()
    }

    /// Downstream: the tile of the node a channel starting here ends at.
    fn neighbors(&self, q: i32, r: i32) -> Vec<(i32, i32)> {
        if q % NODE_SPACING != 0 || r % NODE_SPACING != 0 {
            return Vec::new();
        }
        let key = (q / NODE_SPACING, r / NODE_SPACING);
        self.cells
            .get(&Self::lattice().cell_id(q, r))
            .and_then(|c| c.channels.iter().find(|ch| ch.from == key))
            .map(|ch| vec![node_tile(ch.to)])
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
    // never under that base: a river entering a lake grades to the lake's
    // surface, not to the lakebed beneath it.
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
                        half0: channel_half_width(p.catchment),
                        half1: channel_half_width(n.catchment),
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

    const S: u64 = 0x9E3779B97F4A7C15;

    fn node(wx: f64, wy: f64, key: NodeKey, direction: (f64, f64), catchment: f64, age: f64, lake: Option<usize>) -> DrainageNode {
        DrainageNode {
            key, q: 0, r: 0, wx, wy, elevation: 10.0, surface: 10.0, direction, catchment, base: 0.0,
            down: None, lake, sill: false, age, cut: 0.0, floor: 5.0,
        }
    }

    /// The channel is nothing at the head, a tile just past it, grows with
    /// catchment, and saturates at a trunk's.
    #[test]
    fn channel_width_starts_at_the_head_and_saturates() {
        assert_eq!(channel_half_width(CHANNEL_HEAD), 0.0);
        assert!((channel_half_width(CHANNEL_HEAD + 1e-9) - CHANNEL_HALF_WIDTH_MIN).abs() < 1e-3);
        let mut last = 0.0;
        for i in 0..200 {
            let w = channel_half_width(CHANNEL_HEAD + i as f64 * 0.5);
            assert!(w >= last, "channel narrows");
            last = w;
        }
        assert_eq!(channel_half_width(10.0 * CATCHMENT_FULL), CHANNEL_HALF_WIDTH_MAX);
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
                let (p, n) = (node(0.0, 0.0, (0, 0), da, 10.0, 1.0, None), node(l, 0.0, (1, 0), db, 10.0, 1.0, None));
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

    /// A train between two nodes on a straight flow line crosses the line
    /// at both, never runs back along it, alternates sides, keeps within
    /// its amplitude by the vigour, and at full vigour is longer than the
    /// line by about its sinuosity; at no vigour there is no train, and the
    /// channel is the line.
    #[test]
    fn the_train_crosses_at_the_nodes_within_its_amplitude() {
        let l = NODE_SPACING as f64;
        let (p, n) = (
            node(0.0, 0.0, (0, 0), (1.0, 0.0), CATCHMENT_FULL, 1.0, None),
            node(l, 0.0, (1, 0), (1.0, 0.0), CATCHMENT_FULL, 1.0, None),
        );
        let axis = Axis::new(flow_line(&p, &n));
        assert!((axis.length() - l).abs() < 1e-9);
        let channel = |v0: f64, v1: f64| Channel {
            from: p.key, to: n.key, axis: Vec::new(), train: None,
            half0: CHANNEL_HALF_WIDTH_MAX, half1: CHANNEL_HALF_WIDTH_MAX, vigour0: v0, vigour1: v1, entry: 1.0,
        };
        assert!(Train::new(&axis, &channel(0.0, 0.0), S).is_none());
        let full = Train::new(&axis, &channel(1.0, 1.0), S).unwrap();
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
        let crossings = full.pts.windows(2).filter(|w| (w[0].1 > 0.0) != (w[1].1 > 0.0)).count();
        assert!(crossings >= 4, "the train crosses the line {crossings} times");
        assert!(full.pts.iter().any(|p| p.1 > 0.3 * full.amplitude) && full.pts.iter().any(|p| p.1 < -0.3 * full.amplitude));
        let (advance, _) = meander_shape();
        let sinuosity = path / l;
        assert!(sinuosity > 1.2 && sinuosity < 1.0 / advance * 1.2, "sinuosity {sinuosity} for {}", 1.0 / advance);
        let half = Train::new(&axis, &channel(0.5, 0.5), S).unwrap();
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
        let run = NODE_SPACING as f64;
        let down = |n: &DrainageNode, grade: f64| n.floor - grade * run / RISE;
        let at = |catchment: f64, age: f64, lake: Option<usize>| node(0.0, 0.0, (0, 0), (1.0, 0.0), catchment, age, lake);
        let trunk = at(CATCHMENT_FULL, 1.0, None);
        assert_eq!(vigour(&at(CHANNEL_HEAD, 1.0, None), 0.0, run), 0.0);
        assert_eq!(vigour(&at(CATCHMENT_FULL, 1.0, Some(0)), 0.0, run), 0.0);
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
        let young = at(CATCHMENT_FULL, 0.0, None);
        let v = vigour(&young, down(&young, 0.0), run);
        assert!(v > 0.0 && v < 1.0 && (v - YOUNG_SHARE).abs() < 1e-12, "a young plate's vigour {v}");
    }

    /// A routed cell's channels: one per node with a downstream node, each
    /// owned by the cell its start node lies in, its flow line running from
    /// the start node to the end node.
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
        let with_down = published.nodes.values().filter(|n| n.down.map_or(false, |d| published.nodes.contains_key(&d))).count();
        assert!(all.len() >= with_down, "{} channels for {with_down} nodes with a published downstream node", all.len());
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
