//! GroundEvent — the ground beneath drainage, at every node of its lattice.
//!
//! # Claims
//!
//! Drainage routes over the ground the layers beneath it leave: the field
//! layers — substrate, tilt, thrusting, thickening, lithology — and the
//! scarps carved into them, summed as the functions they are by
//! [`ground_at`]. Evaluating that sum at a position reads the whole outline
//! of the plate the position stands in, and a scarp's carve there reads the
//! ground at its site in turn, so a node's ground needs the plate graph
//! further out than drainage's own cell and ring reach. This layer
//! evaluates it where that is in reach: each cell evaluates the nodes whose
//! sites lie in it and publishes them, each with the ground there, its
//! plate's age and the erodibility of its rock. Drainage reads them and
//! evaluates no ground of its own.
//!
//! A node on a range stands in the trough beside it. Its lattice site is
//! where it falls, and on a belt that is a crest or a face as often as a
//! trough, so a river drawn between nodes there ran across faces and over
//! crests; the water a range sheds gathers in the trough between it and
//! the next, and runs along it. So a node whose site lies on a sheet slides
//! down its fall line to the nearer trough the sheet stands in, the one
//! toward the front or the one behind, wherever the next sheet's face meets
//! this one's, or the front. A node with no trough on either side, on a
//! last sheet's back slope that runs out onto the plateau, keeps its site.
//!
//! A node also says what stands between it and each of its six
//! neighbours: the pass, the highest ground the link crosses where it
//! crosses a range's crest, read at each crossing. Two nodes in troughs
//! either side of a range are neighbours across it, and water routed by
//! their heights alone would cross the range wherever the far trough lies
//! lower; with the pass it fills to the range's height before it can, and
//! leaves a trough over its lowest saddle. A link that crosses no crest has
//! no pass.
//!
//! A layer added beneath drainage either stays a field or publishes what
//! drainage needs, and is added to [`ground_at`], or drainage routes over
//! ground that is not there. `drainage_probe` checks the sum against the
//! composite.
//!
//! # The window
//!
//! The cell reaches as far as a node's ground reads: a plate's whole
//! outline past the site of any scarp whose footprint holds the node. No
//! query: the layer puts nothing on the ground.

use std::any::Any;
use std::collections::HashMap;

use crate::lattice::{node_site, site_world, NodeKey, DIRECTIONS, NODE_SPACING, NODE_SWING};
use crate::{hex_to_world, world_to_hex};
use super::drainage::{ground_at, Ground};
use super::index::{CellId, CellIndex, IndexRegistry, Located};
use super::plates::{coasts_of, Coasts};
use super::scarp::{self, Scarps};
use super::thrusting::{outlines_of, EdgeOutline, Outlines, Standing, RANGE_SPACING, SCREE_REACH, WEDGE_REACH};
use super::{CellScope, TileOutput, TileView, WorldEvent, RING_CLEARANCE};

/// The ground beneath drainage at one node: its key, where the node
/// stands, the ground there, and the pass on its link to each neighbour,
/// in the lattice's order of directions, or minus infinity where the link
/// crosses no crest.
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub key: NodeKey,
    pub wx: f64,
    pub wy: f64,
    pub ground: Ground,
    pub passes: [f64; 6],
}

/// A sample lies where its node stands, and as far as its lattice site,
/// which decides the cell owning it.
impl Located for Sample {
    fn bounds(&self) -> [f64; 4] {
        let (sx, sy) = site_world(self.key);
        [self.wx.min(sx), self.wy.min(sy), self.wx.max(sx), self.wy.max(sy)]
    }
}

/// Where node `key` stands: its lattice site, or on a range, the nearer
/// trough the sheet at its site stands in, reached along the fall line, the
/// way the distance to the sheet's front climbs in the world, the outline's
/// distances measured through the warp, at the centre of the tile there.
/// A trough is never further than a
/// sheet spacing across the ground; where the warp would put it further, the
/// node keeps its site.
pub fn stands_at(key: NodeKey, outlines: &Outlines) -> (f64, f64) {
    let (x, y) = site_world(key);
    let Some(at) = outlines.at(x, y) else { return (x, y) };
    let Some(t) = outlines.troughs_of(&at) else { return (x, y) };
    let nearer = match (t.front, t.back) {
        (Some(f), Some(b)) => if t.at - f <= b - t.at { f } else { b },
        (Some(f), None) => f,
        (None, Some(b)) => b,
        (None, None) => return (x, y),
    };
    let plate = at.plate.id;
    let distance = |x: f64, y: f64| outlines.at(x, y).filter(|a| a.plate.id == plate).map(|a| a.distances[t.edge]);
    let gradient = (|| {
        let gx = (distance(x + 1.0, y)? - distance(x - 1.0, y)?) / 2.0;
        let gy = (distance(x, y + 1.0)? - distance(x, y - 1.0)?) / 2.0;
        Some((gx, gy))
    })();
    let Some((gx, gy)) = gradient else { return (x, y) };
    let g2 = gx * gx + gy * gy;
    if g2 <= 1e-12 {
        return (x, y);
    }
    let step = (nearer - t.at) / g2;
    if (step * step * g2).sqrt() > RANGE_SPACING {
        return (x, y);
    }
    // A node stands at a tile's centre, as its site does.
    let (q, r) = world_to_hex(x + gx * step, y + gy * step);
    hex_to_world(q, r)
}

/// The sample at node `key`: where it stands, the ground there, and the
/// pass to each neighbour.
pub fn sample_of(key: NodeKey, seed: u64, coasts: &Coasts, outlines: &Outlines, scarps: &Scarps) -> Sample {
    sample_with(key, |k| stands_at(k, outlines), |a, b| pass_between(a, b, seed, coasts, outlines, scarps), seed, coasts, outlines, scarps)
}

/// [`sample_of`] with where a node stands, and the pass between two
/// nodes standing where they do in key order, from `stands` and `pass`:
/// what a cell remembers across its nodes, since each stands beside six.
fn sample_with(
    key: NodeKey,
    mut stands: impl FnMut(NodeKey) -> (f64, f64),
    mut pass: impl FnMut((f64, f64), (f64, f64)) -> f64,
    seed: u64,
    coasts: &Coasts,
    outlines: &Outlines,
    scarps: &Scarps,
) -> Sample {
    let (wx, wy) = stands(key);
    let passes = DIRECTIONS.map(|(di, dj)| {
        let other = (key.0 + di, key.1 + dj);
        let (a, b) = if key <= other { (key, other) } else { (other, key) };
        pass(stands(a), stands(b))
    });
    Sample { key, wx, wy, ground: ground_at(wx, wy, seed, coasts, outlines, scarps), passes }
}

/// The highest ground the link from `a` to `b` crosses where it crosses a
/// sheet's crest, or minus infinity where it crosses none: each crest a
/// wedge of either end's plate stands, where the distance to its front
/// passes the crest's along the link, read straight between the ends.
fn pass_between(a: (f64, f64), b: (f64, f64), seed: u64, coasts: &Coasts, outlines: &Outlines, scarps: &Scarps) -> f64 {
    let (Some(sa), Some(sb)) = (outlines.at(a.0, a.1), outlines.at(b.0, b.1)) else { return f64::NEG_INFINITY };
    let signed = |e: &EdgeOutline, x: f64, y: f64| {
        let (d, side) = e.distance(x, y);
        if side >= 0.0 { d } else { -d }
    };
    // A wedge's crests lie within its reach of its front, and an end
    // standing in the wedge's plate already knows its distance to it, so
    // only a link with an end that near walks a front's chain; the link
    // counted twice over, for the warp's stretch.
    let crests_within = WEDGE_REACH + SCREE_REACH;
    let length = (b.0 - a.0).hypot(b.1 - a.1);
    let mut pass = f64::NEG_INFINITY;
    let plates = [&sa, &sb];
    for (n, at) in plates.iter().enumerate() {
        if n == 1 && at.plate.id == plates[0].plate.id {
            break;
        }
        for (i, e) in at.plate.edges.iter().enumerate() {
            let Some(faces) = &e.faces else { continue };
            if at.distances[i] > crests_within + 2.0 * length {
                continue;
            }
            let distance = |end: &Standing| {
                if end.plate.id == at.plate.id { end.distances[i] } else { signed(e, end.x, end.y) }
            };
            let (da, db) = (distance(&sa), distance(&sb));
            if da.min(db) > WEDGE_REACH + SCREE_REACH || da.max(db) < -SCREE_REACH {
                continue;
            }
            for c in faces.crests() {
                if (c - da) * (c - db) >= 0.0 {
                    continue;
                }
                let t = (c - da) / (db - da);
                let (x, y) = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
                pass = pass.max(ground_at(x, y, seed, coasts, outlines, scarps).surface);
            }
        }
    }
    pass
}

/// The samples each cell evaluated: those of the nodes whose sites lie in
/// it.
#[derive(Default)]
pub struct GroundIndex {
    cells: HashMap<CellId, Vec<Sample>>,
}

impl CellIndex for GroundIndex {
    type Cell = Vec<Sample>;
    type Feature = Sample;

    fn set(&mut self, cell: CellId, entry: Self::Cell) {
        self.cells.insert(cell, entry);
    }

    fn get(&self, cell: CellId) -> Option<&Self::Cell> {
        self.cells.get(&cell)
    }

    fn features(entry: &Self::Cell) -> impl Iterator<Item = &Sample> {
        entry.iter()
    }
}

/// How far a node's ground reads from its site: from where it and its
/// neighbours stand, each at most a sheet spacing off their sites, along
/// its links, a spacing and their sites' swing long, as far as a scarp's
/// carve there reads, which holds the outline of the plate the ground
/// stands in.
pub fn ground_reach() -> f64 {
    RANGE_SPACING + NODE_SPACING as f64 + 2.0 * NODE_SWING + scarp::read_reach()
}

#[derive(Default)]
pub struct GroundEvent;

impl GroundEvent {
    pub fn new() -> Self {
        GroundEvent
    }
}

impl WorldEvent for GroundEvent {
    fn name(&self) -> &str { "ground" }

    /// One ring holds everything a node of the cell reads.
    fn scale(&self) -> u32 {
        (ground_reach() / RING_CLEARANCE).ceil() as u32 + 1
    }

    fn max_influence(&self) -> u32 {
        ground_reach().ceil() as u32
    }

    fn register_indexes(&self, registry: &mut IndexRegistry) {
        registry.pre_register::<GroundIndex>();
    }

    /// Where every node whose site lies in the cell stands, and the ground
    /// there, in key order.
    fn deform(&self, scope: &CellScope) {
        let (lattice, cell, seed) = (scope.lattice(), scope.cell(), scope.seed());
        let coasts = coasts_of(scope);
        let outlines = outlines_of(scope);
        let scarps = Scarps::new();
        let (cq, cr) = lattice.cell_center(cell);
        let reach = lattice.radius as i32 + NODE_SPACING;
        let s = NODE_SPACING;
        let (i0, i1) = ((cq - reach).div_euclid(s), (cq + reach).div_euclid(s) + 1);
        let (j0, j1) = ((cr - reach).div_euclid(s), (cr + reach).div_euclid(s) + 1);
        // Each node stands beside six and each link joins two of the
        // cell's nodes or one of them and a neighbour's, so where a node
        // stands and the pass on a link are found once for the cell.
        let mut stands: HashMap<NodeKey, (f64, f64)> = HashMap::new();
        let mut passes: HashMap<((i64, i64), (i64, i64)), f64> = HashMap::new();
        let mut samples = Vec::new();
        for i in i0..=i1 {
            for j in j0..=j1 {
                let (q, r) = node_site((i, j));
                if lattice.cell_id(q, r) != cell {
                    continue;
                }
                let mut stand = |k: NodeKey| *stands.entry(k).or_insert_with(|| stands_at(k, &outlines));
                let mut pass = |a: (f64, f64), b: (f64, f64)| {
                    let key = ((a.0.to_bits() as i64, a.1.to_bits() as i64), (b.0.to_bits() as i64, b.1.to_bits() as i64));
                    *passes.entry(key).or_insert_with(|| pass_between(a, b, seed, &coasts, &outlines, &scarps))
                };
                samples.push(sample_with((i, j), &mut stand, &mut pass, seed, &coasts, &outlines, &scarps));
            }
        }
        scope.publish::<GroundIndex>(samples);
    }

    /// Nothing per tile: drainage reads the samples.
    fn query(&self, _q: i32, _r: i32, _below: &TileView, _cell: &(dyn Any + Send + Sync), _seed: u64) -> Option<TileOutput> {
        None
    }
}
