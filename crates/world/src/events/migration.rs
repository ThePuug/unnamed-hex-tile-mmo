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
//! The train is that process run: the channel migrates. Each step every
//! point's bank retreats outward from its bend by the curvature averaged
//! over a couple of widths to each side of it, less the curvature at the
//! point itself, after Howard and Knutson; the flow answers the curvature
//! upstream of a point, which walks a bend downstream, but a segment
//! pinned at its nodes cannot let bends walk, so the average is taken to
//! both sides and bends grow where they lie, and the local term damps
//! every wave the average cannot see so the channel's own spacing never
//! grows into a zigzag. A bend is cut off when its banks touch across its
//! neck, or sooner when a flood cuts a chute across its point bar, at a
//! bend sinuosity of three: the loop is left as an oxbow, the channel runs
//! straight across, and the oxbow silts to a scar in time, a plug in the
//! floodplain the bank cannot cut, so the next bend takes its turn
//! elsewhere. Bends of every age lie along a channel that has run long
//! enough, some new in a cutoff's straight, some at their neck, so the
//! train is irregular the way a river's is and its belt drifts across the
//! floor as its cutoffs leave it. How far the bank retreats is the vigour;
//! a river with none holds its flow line.
//!
//! The channel starts from a seed with a slight sinuosity: the
//! sine-generated curve at the wavelength the width sets, each bend's
//! length and size its own by a hash, so bends mature unequally, crossing
//! the flow line at both nodes toward the side each node's hash picks. The
//! nodes pin the channel, since a segment is one cell's and its neighbour
//! must draw the same node; the seed and the bank's retreat fade over a
//! shoulder at each end so the pin leaves a straight and never a kink, and
//! the two segments' trains never tangle across the node they share. A
//! tributary's seed fades toward the flow line over its last bend by the
//! share of the water it brings, so it arrives at a trunk along its line;
//! where it first crosses the trunk's train it ends, at the trunk's bank,
//! which a reader settles over its ring since a trunk's segment may be
//! another cell's.
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

/// How far one bend's length strays from the train's in the seed, as a
/// share of it.
pub const MEANDER_LOBE_VARIANCE: f64 = 0.3;

/// How far one bend's size strays from the train's in the seed, as a
/// share of it. Bends grow exponentially as they migrate, a few e-folds
/// over a run, so a bend seeded at a fifth of its neighbour is still young
/// when the neighbour reaches its neck: bends of every age lie along the
/// train, and a river is a mix of straights and loops instead of a coil.
pub const MEANDER_SIZE_VARIANCE: f64 = 0.8;

/// Points per wavelength of channel the train is drawn with, and the
/// shortest step in world units. A bend's tightest radius is its length
/// over pi times the deflection, so a step this share of the wavelength
/// misses the shortest bend's curve by under half a tile at a trunk, and
/// a step under a tile resolves nothing a tile can show.
const MEANDER_STEPS_PER_WAVE: f64 = 24.0;
const MEANDER_STEP_MIN: f64 = 1.0;

/// Hash channels: which side a node's crossing turns toward, a bend's
/// length, and a bend's size.
const MEANDER_SIDE: u64 = 0x6d65_616e;
const MEANDER_LOBE: u64 = 0x6c6f_6265;
const MEANDER_SIZE: u64 = 0x7369_7a65;

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

/// The seed a train grows from, in the flow line's frame: the sine-generated
/// curve crossing the line at both nodes, toward the side each node's hash
/// picks, a whole number of half-waves at the wavelength the channel's
/// width sets, each bend's length and size its own by a hash, at
/// [`MIGRATION_SEED`] of its full amplitude. Over its last
/// bend it fades toward the line by `entry`, the share of the end node's
/// water the channel brings, so a tributary arrives at a trunk along its
/// flow line. Returns the points as (along, across) in world units, and
/// the fitted wavelength.
fn seed_train(length: f64, width: f64, from: NodeKey, to: NodeKey, entry: f64, seed: u64) -> (Vec<(f64, f64)>, f64) {
    let side = |key: NodeKey| if hash_channel(key.0 as i64, key.1 as i64, seed, MEANDER_SIDE) & 1 == 0 { 1.0 } else { -1.0 };
    let (sign, sign1) = (side(from), side(to));
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
    // Each bend's length: its share of the path, normalised so the bends
    // fill it; a bend's size follows its length, since the curve returns
    // to the line over any bend.
    let mut bends: Vec<f64> = (0..k)
        .map(|j| {
            let u = hash_channel_f64(from.0 as i64, from.1 as i64, seed, MEANDER_LOBE.wrapping_add(j));
            1.0 + MEANDER_LOBE_VARIANCE * (2.0 * u - 1.0)
        })
        .collect();
    let total: f64 = bends.iter().sum();
    for b in &mut bends {
        *b *= path / total;
    }
    let step = (path_wave / MEANDER_STEPS_PER_WAVE).max(MEANDER_STEP_MIN);
    // Each bend is walked in its own whole number of steps, so its
    // midpoint samples sit symmetrically about its centre and the curve
    // returns to the line at its end exactly. The fade and the bend's
    // size scale the offset and never the heading, for the same reason.
    let mut pts = Vec::with_capacity((path / step) as usize + bends.len() + 1);
    let (mut x, mut y) = (0.0, 0.0);
    pts.push((x, y));
    for (j, &bend) in bends.iter().enumerate() {
        let count = (bend / step).ceil().max(1.0);
        let step = bend / count;
        let side = if j % 2 == 0 { sign } else { -sign };
        let size = {
            let u = hash_channel_f64(from.0 as i64, from.1 as i64, seed, MEANDER_SIZE.wrapping_add(j as u64));
            1.0 + MEANDER_SIZE_VARIANCE * (2.0 * u - 1.0)
        };
        for i in 0..count as usize {
            let frac = (i as f64 + 0.5) / count;
            let heading = side * MEANDER_DEFLECTION * (PI * frac).cos();
            x += step * heading.cos();
            y += step * heading.sin();
            let end = (i as f64 + 1.0) / count;
            let fade = if j + 1 == bends.len() { 1.0 - (1.0 - entry) * end * end * (3.0 - 2.0 * end) } else { 1.0 };
            pts.push((x, y * size * fade * MIGRATION_SEED));
        }
    }
    let scale = length / x;
    for p in &mut pts {
        p.0 = (p.0 * scale).min(length);
    }
    (pts, wavelength)
}

// ── Migration ───────────────────────────────────────────────────────────────

/// Points per channel width the channel is walked with as it migrates.
const MIGRATION_SPACING: f64 = 1.0;

/// How far the curvature is averaged to each side of a point, in widths,
/// for the bank's retreat there. The flow answers the curvature over a
/// few widths, the way Ikeda, Parker and Sawai found; it answers it
/// upstream, which walks a bend downstream as it grows, but a segment
/// pinned at its nodes cannot let bends walk, they pile against the
/// downstream pin, so the average is taken to both sides and bends grow
/// where they lie. Two widths: at one the average cannot damp the
/// zigzag the channel's own spacing would grow into; at three it no
/// longer grows a bend of the wavelength the width sets.
pub const MIGRATION_LAG: f64 = 2.0;

/// How much the retreat owes to the averaged curvature against the
/// curvature at the point itself, which counts against it, after Howard
/// and Knutson. The local term damps what the average smooths away: a
/// wave of two widths shrinks, one of four holds, and one of the width's
/// wavelength grows fastest, so the channel's spacing never grows into a
/// zigzag and the wavelength a river picks is the one its width sets.
const MIGRATION_AVERAGED: f64 = 3.5;

/// Bank retreat per step per unit of net curvature, in widths: the
/// erodibility and a step's time in one number, small enough that the
/// tightest bend a neck allows moves under a fifth of a width in a step.
const MIGRATION_RATE: f64 = 0.05;

/// Steps a channel migrates for at full vigour: past a trunk's first
/// cutoffs, where its sinuosity settles near one and a half and its belt
/// near a wavelength to each side, with bends of every age along it. A
/// head stream at full vigour, three times the points, costs three times
/// a trunk: ten and thirty milliseconds.
///
/// EMPIRICAL: read from `migration_probe`.
pub const MIGRATION_STEPS: usize = 400;

/// Steps an oxbow stays open water after its bend is cut off, before it
/// has silted to a scar: the last of a run's cutoffs are lakes, the rest
/// are plugs in the floodplain the bank still cannot cut.
const OXBOW_LIFE: usize = 150;

/// Steps between searches for a neck. A neck closes over many steps and
/// the search is the costliest part of one.
const NECK_EVERY: usize = 4;

/// The length at each end of a segment over which the bank's retreat
/// fades to nothing, in wavelengths: the node pins the channel, and a pin
/// needs a shoulder or the channel kinks against it. A grown loop reaches
/// a quarter wavelength back from its own bend, so at three quarters the
/// first loop past the shoulder on one side of a node stays half a
/// wavelength clear of the first on the other, and the two segments'
/// trains never tangle across the node they share.
const MIGRATION_TAPER: f64 = 0.75;

/// The distance between two parts of the channel, in widths, at which the
/// neck between them breaks: the banks touch.
const NECK: f64 = 1.0;

/// The sinuosity of one bend, its path over its chord, at which a flood
/// cuts a chute across its point bar: the bend is cut off while still
/// open, well before its neck closes, so a train is open bends and
/// straights instead of loops packed shoulder to shoulder, and its
/// sinuosity settles under two instead of near three.
const CHUTE_SINUOSITY: f64 = 3.0;

/// The longest chord a chute is looked for across, in widths: what the
/// points are bucketed at.
const CHUTE_REACH: f64 = 8.0;

/// The fewest points two parts of the channel can be apart along it and
/// still be a neck or a chute; fewer is one bend's own wall.
const NECK_LOOP: usize = 8;

/// The share of its full amplitude the seed train is drawn at: a slight
/// sinuosity at the wavelength the width sets, for the bends to grow from.
const MIGRATION_SEED: f64 = 0.1;

/// How far across the valley a channel may sweep, as a share of the
/// valley's half-width: the wall stops the bank.
const MIGRATION_ROOM: f64 = 0.8;

/// How far an oxbow's fill resists the bank, in widths, and what it
/// leaves of the retreat there. An abandoned loop fills with the fines
/// the flood drops in it, a plug the next bend cannot cut as it cuts
/// the floodplain's sand, so a bend that has cut off does not grow back
/// in its own place: the next bend downstream takes its turn, and the
/// cutoffs spread along the river instead of stacking at one neck.
const PLUG_REACH: f64 = 2.0;
const PLUG_RETREAT: f64 = 0.2;

/// A channel's train across its flow line: the live channel in world
/// space, the loops it has cut off, and both bucketed for the search; how
/// far it has strayed from the line over its migration, what the belt it
/// swept is wide to each side before the channel's own half-width; and
/// the wavelength its width set.
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
    /// meanders or no channel runs: the seed train migrated for
    /// [`MIGRATION_STEPS`] steps.
    fn new(axis: &Axis, channel: &Channel, seed: u64) -> Option<Self> {
        Self::migrated(axis, channel, seed, MIGRATION_STEPS)
    }

    /// The seed train migrated for `steps` steps, in the flow line's frame
    /// and in units of the channel's width. Each step, every point's bank
    /// retreats outward from the bend by the curvature over the lag
    /// upstream of it, by the vigour there and by the taper at the ends;
    /// the channel is resampled to its spacing; and wherever two parts of
    /// it come within a width the neck breaks, the loop between them is an
    /// oxbow, and the channel runs straight across. The channel never
    /// sweeps past the valley's wall.
    pub fn migrated(axis: &Axis, channel: &Channel, seed: u64, steps: usize) -> Option<Self> {
        let width = channel.half0 + channel.half1;
        if width <= 0.0 || (channel.vigour0 <= 0.0 && channel.vigour1 <= 0.0) {
            return None;
        }
        let length = axis.length();
        let (seeded, wavelength) = seed_train(length, width, channel.from, channel.to, channel.entry, seed);
        // Into widths.
        let length_w = length / width;
        let taper = MIGRATION_TAPER * wavelength / width;
        let shoulder = |d: f64| {
            let u = (d / taper).clamp(0.0, 1.0);
            u * u * (3.0 - 2.0 * u)
        };
        // Into widths, the seed arriving along the line at both nodes: a
        // seed crossing the node at its full deflection puts its sharpest
        // bend against the pin, and the first bend past the shoulder
        // inherits it and matures first in every segment alike.
        let mut pts: Vec<(f64, f64)> = seeded
            .iter()
            .map(|&(x, y)| (x / width, y / width * shoulder(x / width) * shoulder(length_w - x / width)))
            .collect();
        let room = MIGRATION_ROOM * VALLEY_HALF_WIDTH / width;
        let decay = (-MIGRATION_SPACING / MIGRATION_LAG).exp();
        let mut oxbows: Vec<(usize, Vec<(f64, f64)>)> = Vec::new();
        let mut plugs: HashMap<(i64, i64), Vec<(f64, f64)>> = HashMap::new();
        let plugged = |plugs: &HashMap<(i64, i64), Vec<(f64, f64)>>, p: (f64, f64)| -> bool {
            let (bx, by) = ((p.0 / PLUG_REACH).floor() as i64, (p.1 / PLUG_REACH).floor() as i64);
            (-1..=1).any(|dx| {
                (-1..=1).any(|dy| {
                    plugs.get(&(bx + dx, by + dy)).map_or(false, |v| v.iter().any(|q| (q.0 - p.0).hypot(q.1 - p.1) < PLUG_REACH))
                })
            })
        };
        let mut amplitude: f64 = 0.0;
        pts = resample(&pts, MIGRATION_SPACING);
        for step in 0..steps {
            let n = pts.len();
            if n < 3 {
                break;
            }
            // The curvature at every point, and its average to each side:
            // an exponential average walked down the channel and one
            // walked up it, halved.
            let mut curvature = vec![0.0; n];
            for i in 1..n - 1 {
                let (a, b, c) = (pts[i - 1], pts[i], pts[i + 1]);
                let (abx, aby) = (b.0 - a.0, b.1 - a.1);
                let (bcx, bcy) = (c.0 - b.0, c.1 - b.1);
                let (acx, acy) = (c.0 - a.0, c.1 - a.1);
                let cross = abx * bcy - aby * bcx;
                let denom = abx.hypot(aby) * bcx.hypot(bcy) * acx.hypot(acy);
                curvature[i] = if denom > 0.0 { (2.0 * cross / denom).clamp(-2.0, 2.0) } else { 0.0 };
            }
            let mut averaged = vec![0.0; n];
            let mut down = 0.0;
            for i in 0..n {
                down = down * decay + curvature[i] * (1.0 - decay);
                averaged[i] = 0.5 * down;
            }
            let mut up = 0.0;
            for i in (0..n).rev() {
                up = up * decay + curvature[i] * (1.0 - decay);
                averaged[i] += 0.5 * up;
            }
            let mut shift = vec![(0.0, 0.0); n];
            for i in 1..n - 1 {
                let (a, b, c) = (pts[i - 1], pts[i], pts[i + 1]);
                let (acx, acy) = (c.0 - a.0, c.1 - a.1);
                let x = b.0;
                let t = (x / length_w).clamp(0.0, 1.0);
                let vigour = channel.vigour0 + t * (channel.vigour1 - channel.vigour0);
                let plug = if plugged(&plugs, b) { PLUG_RETREAT } else { 1.0 };
                let net = MIGRATION_AVERAGED * averaged[i] - curvature[i];
                let retreat = MIGRATION_RATE * net * vigour * plug * shoulder(x) * shoulder(length_w - x);
                // The left normal of the chord through the point; a left
                // turn's outer bank is on the right, so the bank retreats
                // against the normal.
                let len = acx.hypot(acy);
                if len > 0.0 {
                    shift[i] = (retreat * acy / len, -retreat * acx / len);
                }
            }
            for i in 1..n - 1 {
                pts[i].0 += shift[i].0;
                pts[i].1 = (pts[i].1 + shift[i].1).clamp(-room, room);
                amplitude = amplitude.max(pts[i].1.abs());
            }
            pts = resample(&pts, MIGRATION_SPACING);
            if step % NECK_EVERY != 0 {
                continue;
            }
            if let Some((i, j)) = neck(&pts) {
                let mut loop_pts: Vec<(f64, f64)> = pts[i..=j].to_vec();
                loop_pts.push(pts[i]);
                for &q in &loop_pts {
                    plugs.entry(((q.0 / PLUG_REACH).floor() as i64, (q.1 / PLUG_REACH).floor() as i64)).or_default().push(q);
                }
                oxbows.push((step, loop_pts));
                pts.drain(i + 1..j);
                pts = resample(&pts, MIGRATION_SPACING);
            }
        }
        // Into world.
        let place = |frame: &[(f64, f64)]| -> Vec<(f64, f64)> {
            let mut from = 0;
            frame
                .iter()
                .map(|&(x, y)| {
                    let (point, normal) = axis.at((x * width).clamp(0.0, length), &mut from);
                    (point.0 + y * width * normal.0, point.1 + y * width * normal.1)
                })
                .collect()
        };
        let pts = place(&pts);
        let oxbows: Vec<Vec<(f64, f64)>> = oxbows
            .iter()
            .filter(|(at, _)| at + OXBOW_LIFE >= steps)
            .map(|(_, o)| place(o))
            .collect();
        let grid = Self::bucket(&pts, &oxbows);
        Some(Train { pts, oxbows, grid, amplitude: amplitude * width, wavelength })
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

/// The channel walked again at `spacing` from its first point, its last
/// point kept, each new point on the Catmull-Rom curve through the old
/// ones. Walking the chords instead cuts the corner of every bend by a
/// few hundredths of its amplitude, more than a step grows it, and cuts
/// it less where the samples sit still against the pinned node than
/// where they slide as the channel lengthens, so the head of a segment
/// would grow and its tail would waste.
fn resample(pts: &[(f64, f64)], spacing: f64) -> Vec<(f64, f64)> {
    let n = pts.len();
    if n < 2 {
        return pts.to_vec();
    }
    // Past either end the curve continues by reflection, so a straight
    // line resamples straight to its last point.
    let at = |i: isize| -> (f64, f64) {
        let last = n as isize - 1;
        if i < 0 {
            let (o, m) = (pts[0], pts[(-i).min(last) as usize]);
            (2.0 * o.0 - m.0, 2.0 * o.1 - m.1)
        } else if i > last {
            let (o, m) = (pts[last as usize], pts[(2 * last - i).max(0) as usize]);
            (2.0 * o.0 - m.0, 2.0 * o.1 - m.1)
        } else {
            pts[i as usize]
        }
    };
    let curve = |i: usize, u: f64| -> (f64, f64) {
        let (p0, p1, p2, p3) = (at(i as isize - 1), at(i as isize), at(i as isize + 1), at(i as isize + 2));
        let (u2, u3) = (u * u, u * u * u);
        let blend = |a: f64, b: f64, c: f64, d: f64| {
            0.5 * (2.0 * b + (c - a) * u + (2.0 * a - 5.0 * b + 4.0 * c - d) * u2 + (3.0 * b - a - 3.0 * c + d) * u3)
        };
        (blend(p0.0, p1.0, p2.0, p3.0), blend(p0.1, p1.1, p2.1, p3.1))
    };
    let mut out = vec![pts[0]];
    let mut carry = 0.0;
    for i in 0..n - 1 {
        let (a, b) = (pts[i], pts[i + 1]);
        let len = (b.0 - a.0).hypot(b.1 - a.1);
        if len <= 0.0 {
            continue;
        }
        let mut s = spacing - carry;
        while s <= len {
            out.push(curve(i, s / len));
            s += spacing;
        }
        carry = len - (s - spacing);
    }
    let last = pts[n - 1];
    if out.last().map_or(true, |&o| (o.0 - last.0).hypot(o.1 - last.1) > 0.5 * spacing) {
        out.push(last);
    } else if out.len() > 1 {
        *out.last_mut().unwrap() = last;
    }
    out
}

/// The cutoff most due along the channel: of every two points at least
/// [`NECK_LOOP`] apart along it, within [`NECK`] of each other or with a
/// path between them over [`CHUTE_SINUOSITY`] times their chord, the pair
/// whose chord is the smallest share of its path; found by bucketing the
/// points at [`CHUTE_REACH`]. None when the channel is clear.
fn neck(pts: &[(f64, f64)]) -> Option<(usize, usize)> {
    let mut buckets: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (i, p) in pts.iter().enumerate() {
        buckets.entry(((p.0 / CHUTE_REACH).floor() as i64, (p.1 / CHUTE_REACH).floor() as i64)).or_default().push(i);
    }
    let mut best: Option<(usize, usize, f64)> = None;
    for (i, p) in pts.iter().enumerate() {
        let (bx, by) = ((p.0 / CHUTE_REACH).floor() as i64, (p.1 / CHUTE_REACH).floor() as i64);
        for dx in -1..=1 {
            for dy in -1..=1 {
                let Some(others) = buckets.get(&(bx + dx, by + dy)) else { continue };
                for &j in others {
                    if j < i + NECK_LOOP {
                        continue;
                    }
                    let d = (pts[j].0 - p.0).hypot(pts[j].1 - p.1);
                    let path = (j - i) as f64 * MIGRATION_SPACING;
                    let share = d / path;
                    if (d < NECK || d * CHUTE_SINUOSITY < path) && best.map_or(true, |(_, _, bs)| share < bs) {
                        best = Some((i, j, share));
                    }
                }
            }
        }
    }
    best.map(|(i, j, _)| (i, j))
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

    /// A trunk's channel between two nodes on a straight flow line: at no
    /// vigour there is no train and the channel is the line; migrated, it
    /// still runs from node to node, stays inside the valley, is longer
    /// than the line by a river's sinuosity, and has swept a belt that is
    /// narrower the shorter it runs and at half vigour; and it is the same
    /// train twice.
    #[test]
    fn a_migrated_train_joins_its_nodes_and_grows_with_its_run() {
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
        let young = Train::migrated(&axis, &channel(1.0, 1.0), S, MIGRATION_STEPS / 4).unwrap();
        let half = Train::new(&axis, &channel(0.5, 0.5), S).unwrap();
        assert!(young.amplitude < full.amplitude, "a shorter run swept wider: {} for {}", young.amplitude, full.amplitude);
        assert!(half.amplitude < full.amplitude, "half vigour swept wider: {} for {}", half.amplitude, full.amplitude);
        assert!(sinuosity(&young) > 1.0 && sinuosity(&young) < s_full, "a shorter run is not straighter: {} for {s_full}", sinuosity(&young));
        let again = Train::new(&axis, &channel(1.0, 1.0), S).unwrap();
        assert_eq!(full.pts, again.pts, "the same channel migrated twice differs");
        // Every point of the channel is within the slot's reach of itself.
        for &(x, y) in &full.pts {
            assert!(full.distance(x, y) < 1e-9);
        }
    }

    /// Resampling keeps both ends and walks the line at its spacing:
    /// exactly on evenly spaced points, and near it on uneven ones, since
    /// the curve through them is walked by their chords' lengths. A bend
    /// resampled keeps its apex: the curve passes through the points.
    #[test]
    fn resampling_keeps_the_ends_the_spacing_and_the_apex() {
        let line: Vec<(f64, f64)> = (0..10).map(|i| (i as f64 * 2.5, 0.0)).collect();
        let out = resample(&line, 1.0);
        assert_eq!(out[0], line[0]);
        assert_eq!(*out.last().unwrap(), *line.last().unwrap());
        for w in out.windows(2).take(out.len() - 2) {
            let d = (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1);
            assert!((d - 1.0).abs() < 1e-9, "a step of {d}");
        }
        let uneven: Vec<(f64, f64)> = (0..10).map(|i| (i as f64 * 2.5 + if i % 2 == 0 { 0.0 } else { 0.5 }, 0.0)).collect();
        for w in resample(&uneven, 1.0).windows(2).take(20) {
            let d = (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1);
            assert!((d - 1.0).abs() < 0.25, "a step of {d}");
        }
        // A sinusoid at one point per unit, resampled at a phase between
        // its points: the apex survives to within a hundredth.
        let wave: Vec<(f64, f64)> = (0..40).map(|i| (i as f64, (i as f64 * PI / 10.0).sin())).collect();
        let out = resample(&wave, 1.0);
        let apex = out.iter().map(|p| p.1).fold(0.0, f64::max);
        assert!(apex > 0.99, "the apex fell to {apex}");
    }

    /// A straight channel has no neck; a channel looping back on itself
    /// has one where its two limbs touch, and never between neighbours.
    #[test]
    fn a_neck_is_found_where_the_limbs_touch() {
        let straight: Vec<(f64, f64)> = (0..40).map(|i| (i as f64, 0.0)).collect();
        assert_eq!(neck(&straight), None);
        // A hairpin: out along y = 0, back along y = 0.5, thirty points apart.
        let mut hairpin: Vec<(f64, f64)> = (0..20).map(|i| (i as f64, 0.0)).collect();
        hairpin.extend((0..20).map(|i| (19.0 - i as f64, 0.5)));
        let (i, j) = neck(&hairpin).expect("no neck in a hairpin");
        assert!(j >= i + NECK_LOOP);
        assert!((hairpin[i].0 - hairpin[i].1).abs() < 25.0);
        let d = (hairpin[j].0 - hairpin[i].0).hypot(hairpin[j].1 - hairpin[i].1);
        assert!(d < NECK);
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

