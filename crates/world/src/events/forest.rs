//! ForestEvent — where trees stand: the climate two fields say at each
//! stand's origin, the stands that originate in each cell, and the seven
//! slots each tile fills. `design/forest.md` in the internal repo is the
//! spec; the claims below are the ones the code binds.
//!
//! # Claims
//!
//! Cold gates, water sets. Temperature is a slow field across the plates
//! less the lapse rate up the ground, and trees stop at a growing-season
//! isotherm, the same on every continent, so the treeline is a band on the
//! ranges at the height the lapse rate reaches it. Moisture is what the
//! wind carries: the sea's share, falling inland up the coast's own ramp,
//! less what every belt between a position and the wind has wrung out. A
//! belt shadows the ground downwind of its rim by its height, fading as
//! the air takes up water again; its windward flank has the rain the
//! climb wrings out and no shadow. Every belt is a convergent edge's
//! wedge, read off the plate outlines thrusting published, so the shadow
//! is the belt's and never a field's.
//!
//! A stand is a feature: it originates at a point of a jittered lattice,
//! the cell that point falls in publishes it, and it reaches its radius.
//! The climate is read once at the origin, with what the rock there keeps
//! of it and the ground's own water, the valley floor and a flooded basin,
//! and sets the stand's density and
//! how many of its slots are trees rather than scrub. A stand stands with
//! the probability its density gives, so between the wet core and the dry
//! ground the woods are islands with edges: forest and open ground as
//! alternative states, decided at the origin and never per tile.
//!
//! A tile reads the stands over it from the ring, the treeline from its
//! own elevation, and its valley from what dissection published, and
//! fills each of its seven slots by a hash against the density it lands
//! on. Nothing is read from a neighbour.

use std::any::Any;
use std::collections::HashMap;
use std::f64::consts::PI;
use std::sync::Arc;

use common::{Cover, HexLattice, HexSpatialGrid, Slot, SLOTS};

use crate::chains::{Segment, SegmentGrid};
use crate::lattice::{nearest_node, PATH_SWING};
use crate::noise::{hash_channel_f64, hash_f64, simplex_2d};
use crate::tectonic::{Edge, PLATE_SPACING};
use crate::{hex_to_world, world_to_hex};
use super::drainage::DrainageIndex;
use super::index::{CellId, CellIndex, EventIndex, IndexRegistry};
use super::lithology::{rock_on, Rock};
use super::migration::{ChannelIndex, VALLEY_HALF_WIDTH};
use super::plates::{Coasts, PlateEdgeIndex, COAST_REACH, GRAPH_CELL_SCALE, WARP_SWING};
use super::thrusting::{rim_of, sheets_of, smoothstep, EdgeOutline, OutlineIndex, Outlines, RANGE_RISE, RANGE_SPACING, WEDGE_SHEETS};
use super::{footprint_plus_ring, CellScope, TileOutput, TileView, WorldEvent, RING_CLEARANCE};

const WIND_SEED: u64 = 0x7769_6e64;
const CLIMATE_SEED: u64 = 0x636c_696d;
const STAND_SEED: u64 = 0x7374_616e;
const SLOT_FILL: u64 = 0x66;
const SLOT_KIND: u64 = 0x6b;
const SLOT_MIX: u64 = 0x6d;

// ── The sky ─────────────────────────────────────────────────────────────────

/// The prevailing wind: one bearing for the whole world, a quantity of the
/// seed, as a unit vector in world space pointing the way it blows.
pub fn wind(seed: u64) -> (f64, f64) {
    let a = 2.0 * PI * hash_f64(0, 0, seed ^ WIND_SEED);
    (a.cos(), a.sin())
}

/// The share of the sea's moisture that reaches the interior: what is
/// left where the sea's reach ends and holds from there.
pub const INTERIOR_MOISTURE: f64 = 0.42;

/// How far inland the sea's moisture falls to the interior's share: a
/// quarter of a plate, short of the substrate's own ramp, so a continent
/// two plates wide has an interior.
pub const MOISTURE_REACH: f64 = 0.25 * PLATE_SPACING;

/// What a full wedge wrings out of the air crossing it: the share of what
/// arrives that its lee does without.
pub const SHADOW_DEPTH: f64 = 0.6;

/// How far upwind a belt is looked for: the reach of the shadow it casts.
pub const SHADOW_REACH: f64 = PLATE_SPACING;

/// The sea's moisture at a position: everything at the shore, falling
/// over [`MOISTURE_REACH`] to the interior's share.
pub fn sea_moisture(wx: f64, wy: f64, coasts: &Coasts) -> f64 {
    let (frac, land) = coasts.shore(wx, wy);
    if !land {
        return 1.0;
    }
    1.0 - (1.0 - INTERIOR_MOISTURE) * smoothstep(frac * COAST_REACH / MOISTURE_REACH)
}

/// Where a ray from `(x, y)` along `(ux, uy)`, `reach` long, first
/// crosses the edge's chain, as the distance along the ray: its bounding
/// box first, then its segments. None where it does not.
fn crossing(edge: &EdgeOutline, x: f64, y: f64, ux: f64, uy: f64, reach: f64) -> Option<f64> {
    let [x0, y0, x1, y1] = edge.bounds;
    let slab = |p: f64, d: f64, lo: f64, hi: f64| -> Option<(f64, f64)> {
        if d.abs() < 1e-12 {
            (p >= lo && p <= hi).then_some((0.0, reach))
        } else {
            let (a, b) = ((lo - p) / d, (hi - p) / d);
            Some((a.min(b), a.max(b)))
        }
    };
    let (tx0, tx1) = slab(x, ux, x0, x1)?;
    let (ty0, ty1) = slab(y, uy, y0, y1)?;
    if tx0.max(ty0).max(0.0) > tx1.min(ty1).min(reach) {
        return None;
    }
    edge.segments
        .iter()
        .filter_map(|s| ray_hits(s, x, y, ux, uy, reach))
        .min_by(|a, b| a.partial_cmp(b).unwrap())
}

/// Where a ray crosses one segment within `reach` of its start, as the
/// distance along the ray.
fn ray_hits(s: &Segment, x: f64, y: f64, ux: f64, uy: f64, reach: f64) -> Option<f64> {
    let (ex, ey) = (s.x1 - s.x0, s.y1 - s.y0);
    let den = ux * ey - uy * ex;
    if den.abs() < 1e-12 {
        return None;
    }
    let (px, py) = (s.x0 - x, s.y0 - y);
    let t = (px * ey - py * ex) / den;
    let u = (px * uy - py * ux) / den;
    (t >= 0.0 && t <= reach && (0.0..=1.0).contains(&u)).then_some(t)
}

/// How much of a belt's shadow is left `t` downwind of it: the whole for
/// half the reach, then fading to nothing at the reach, since the air
/// takes up moisture again as it goes.
fn fade(t: f64) -> f64 {
    1.0 - smoothstep((t - 0.5 * SHADOW_REACH) / (0.5 * SHADOW_REACH))
}

/// The share of the air's moisture the belts between a position and the
/// wind's source have wrung out, 0 to 1. The plate the position stands in
/// reads its own wedges by where it stands in them: the windward flank has
/// none up to the last sheet below the rim, and past the rim, or under air
/// that came over it, the wedge's whole; every other plate's wedge shadows
/// it where its front lies across the way upwind. A shadow eases in over
/// one sheet's spacing either side of the rim, so nothing steps there.
pub fn shadow(wx: f64, wy: f64, wind: (f64, f64), outlines: &Outlines) -> f64 {
    let Some(at) = outlines.at(wx, wy) else { return 0.0 };
    let (ux, uy) = (-wind.0, -wind.1);
    let mut passes = 1.0;
    for (e, d) in at.plate.edges.iter().zip(&at.distances) {
        if e.converge <= 0.0 {
            continue;
        }
        let sheets = sheets_of(e.converge);
        let rim = rim_of(sheets);
        let share = if let Some(t) = crossing(e, at.x, at.y, ux, uy, SHADOW_REACH) {
            smoothstep((d - (rim - RANGE_SPACING)) / RANGE_SPACING) * fade(t)
        } else if *d < rim && e.distance(at.x + ux * rim, at.y + uy * rim).0 >= rim {
            smoothstep((rim - d) / RANGE_SPACING)
        } else {
            0.0
        };
        passes *= 1.0 - SHADOW_DEPTH * share * sheets / WEDGE_SHEETS;
    }
    for p in outlines.plates() {
        if p.id == at.plate.id {
            continue;
        }
        for e in &p.edges {
            if e.converge <= 0.0 {
                continue;
            }
            if let Some(t) = crossing(e, at.x, at.y, ux, uy, SHADOW_REACH) {
                passes *= 1.0 - SHADOW_DEPTH * fade(t) * sheets_of(e.converge) / WEDGE_SHEETS;
            }
        }
    }
    1.0 - passes
}

/// What the sky gives a position: the sea's moisture less the belts'
/// shadow.
pub fn sky_moisture(wx: f64, wy: f64, wind: (f64, f64), coasts: &Coasts, outlines: &Outlines) -> f64 {
    sea_moisture(wx, wy, coasts) * (1.0 - shadow(wx, wy, wind, outlines))
}

// ── Temperature ─────────────────────────────────────────────────────────────

/// The growing-season isotherm trees stop at, in degrees: Körner's, the
/// same on every continent.
pub const TREELINE: f64 = 6.4;

/// The height the mean region's temperature reaches the treeline at, in
/// z-levels: a range and a half's rise, so a full plateau stands just
/// under the line in cold forest and a range's crest above it. The stack's
/// heights are their own scale, so the lapse rate is set against them and
/// never against a kilometre.
pub const TREELINE_RISE: f64 = 1.5 * RANGE_RISE;

/// The lapse rate in degrees per z-level, from [`TREELINE_RISE`].
pub const LAPSE: f64 = (SEA_LEVEL_MEAN - TREELINE) / TREELINE_RISE;

/// How many degrees above the treeline the forest thins to scrub.
pub const TREELINE_BAND: f64 = 2.0;

/// Below this the trees are pine; above [`DECIDUOUS_ABOVE`] deciduous;
/// between, both, by the share of the way.
pub const PINE_BELOW: f64 = 11.0;
pub const DECIDUOUS_ABOVE: f64 = 15.0;

/// Sea-level temperature across the world: a mean, a spread, and the
/// wavelength of the slow field carrying it, several plates.
pub const SEA_LEVEL_MEAN: f64 = 18.0;
pub const SEA_LEVEL_SPREAD: f64 = 10.0;
pub const CLIMATE_WAVELENGTH: f64 = 4.0 * PLATE_SPACING;

/// The regional temperature at sea level: the world has no latitude, so
/// this slow field is the only climate a region has.
pub fn sea_level_temperature(wx: f64, wy: f64, seed: u64) -> f64 {
    SEA_LEVEL_MEAN + SEA_LEVEL_SPREAD * simplex_2d(wx / CLIMATE_WAVELENGTH, wy / CLIMATE_WAVELENGTH, seed ^ CLIMATE_SEED)
}

/// The temperature at a position `elevation` z-levels up: the regional
/// field less the lapse rate over the ground above the sea.
pub fn temperature(wx: f64, wy: f64, elevation: f64, seed: u64) -> f64 {
    sea_level_temperature(wx, wy, seed) - LAPSE * elevation.max(0.0)
}

// ── The stands ──────────────────────────────────────────────────────────────

/// Below this moisture nothing stands; from here scrub, and from
/// [`MOISTURE_TREES`] trees among it, both whole at [`MOISTURE_CLOSED`].
pub const MOISTURE_SCRUB: f64 = 0.12;
pub const MOISTURE_TREES: f64 = 0.25;
pub const MOISTURE_CLOSED: f64 = 0.55;

/// The density a closed stand fills its slots at: well short of every
/// slot, so a closed forest averages two of its three and refuses about
/// one tile in five, a maze that is crossed.
pub const DENSITY_MAX: f64 = 0.6;

/// The share of a stand's radius its density holds full before tapering
/// to nothing at the edge.
pub const TAPER_FROM: f64 = 0.6;

/// How far up a valley's wall, as a share, the gallery along its channel
/// reaches from the belt's edge.
pub const GALLERY_SHARE: f64 = 0.08;

/// The stand lattice's cell radius, in tiles: its centres are the origins,
/// about a spacing and three quarters of this apart.
pub const STAND_LATTICE: u32 = 70;

/// A stand's radius, in tiles, and the share either side of it a stand's
/// own radius lies within.
pub const STAND_RADIUS: f64 = 110.0;
pub const STAND_SPREAD: f64 = 0.5;

/// How far a stand is drawn out along one axis and in along the other, at
/// most: no stand is round, and no two are alike.
pub const STAND_STRETCH: f64 = 0.4;

/// How far an origin is jittered off its lattice point, as a share of the
/// lattice's centre spacing, per axis: half, so the lattice never shows.
pub const STAND_JITTER: f64 = 0.5;

/// The furthest a stand reaches from its origin.
pub const STAND_REACH: f64 = STAND_RADIUS * (1.0 + STAND_SPREAD) * (1.0 + STAND_STRETCH);

/// The furthest from its origin a stand reads the coasts: the shore
/// ramp, with the swing a chain and the warp take off a coast's line.
pub const COAST_READ: f64 = COAST_REACH + WARP_SWING + PATH_SWING;

/// The furthest anything a tile of the layer depends on lies from it: a
/// stand reaching it, or a coast its stand's origin reads.
pub const FOREST_REACH: f64 = if COAST_READ > STAND_REACH { COAST_READ } else { STAND_REACH };

/// The cell scale: one ring holds everything within [`FOREST_REACH`].
pub const FOREST_CELL_SCALE: u32 = (FOREST_REACH / RING_CLEARANCE) as u32 + 1;

/// A stand of trees: where it originates, how far it reaches and which way
/// it is drawn out, how densely it fills its slots, and how many of them
/// are trees rather than scrub.
#[derive(Clone, Debug)]
pub struct Stand {
    pub wx: f64,
    pub wy: f64,
    pub radius: f64,
    /// The share the stand is drawn out along `along` and in across it.
    pub stretch: f64,
    pub along: (f64, f64),
    pub density: f64,
    pub trees: f64,
}

impl Stand {
    /// How far a position lies from the origin as a share of the stand's
    /// reach that way: one at its edge.
    pub fn share_at(&self, wx: f64, wy: f64) -> f64 {
        let (dx, dy) = (wx - self.wx, wy - self.wy);
        let a = dx * self.along.0 + dy * self.along.1;
        let b = -dx * self.along.1 + dy * self.along.0;
        let (ra, rb) = (self.radius * (1.0 + self.stretch), self.radius * (1.0 - self.stretch));
        ((a / ra).powi(2) + (b / rb).powi(2)).sqrt()
    }

    /// The furthest the stand reaches from its origin.
    pub fn reach(&self) -> f64 {
        self.radius * (1.0 + self.stretch)
    }
}

#[derive(Clone, Debug, Default)]
pub struct ForestCell {
    pub stands: Vec<Stand>,
}

#[derive(Default)]
pub struct StandIndex {
    pub cells: HashMap<CellId, ForestCell>,
}

impl CellIndex for StandIndex {
    type Cell = ForestCell;

    fn set(&mut self, cell: CellId, entry: Self::Cell) {
        self.cells.insert(cell, entry);
    }

    fn get(&self, cell: CellId) -> Option<&Self::Cell> {
        self.cells.get(&cell)
    }
}

impl EventIndex for StandIndex {
    fn source_scale(&self) -> u32 { FOREST_CELL_SCALE }

    /// Each stand at its origin's tile.
    fn tiles(&self, cell_ids: &[CellId]) -> Vec<(i32, i32)> {
        cell_ids
            .iter()
            .filter_map(|id| self.cells.get(id))
            .flat_map(|c| c.stands.iter().map(|s| world_to_hex(s.wx, s.wy)))
            .collect()
    }

    fn neighbors(&self, _q: i32, _r: i32) -> Vec<(i32, i32)> { Vec::new() }

    fn remove_cell(&mut self, cell_id: CellId) {
        self.cells.remove(&cell_id);
    }
}

/// The lattice the origins are drawn on.
pub fn stand_lattice() -> HexLattice {
    HexLattice::new(STAND_LATTICE)
}

/// The origin a stand lattice cell puts its stand at: the cell's centre,
/// jittered by the cell's hash.
pub fn origin_of(lattice: &HexLattice, id: CellId, seed: u64) -> (f64, f64) {
    let (cq, cr) = lattice.cell_center(id);
    let (cx, cy) = hex_to_world(cq, cr);
    let spacing = (lattice.tiles_per_cell() as f64).sqrt();
    let swing = 2.0 * STAND_JITTER * spacing;
    let jx = (hash_channel_f64(id.0 as i64, id.1 as i64, seed ^ STAND_SEED, 1) - 0.5) * swing;
    let jy = (hash_channel_f64(id.0 as i64, id.1 as i64, seed ^ STAND_SEED, 2) - 0.5) * swing;
    (cx + jx, cy + jy)
}

/// The stand lattice cells whose origins can fall in a cell of `lattice`:
/// those touching its footprint and their ring, which holds the jitter.
fn candidate_origins(lattice: &HexLattice, cell: CellId) -> Vec<CellId> {
    footprint_plus_ring(lattice, cell, &stand_lattice())
}

/// A stand's density from the moisture at its origin, short of full, or
/// nothing below the scrub line.
pub fn density_of(moisture: f64) -> f64 {
    DENSITY_MAX * smoothstep((moisture - MOISTURE_SCRUB) / (MOISTURE_CLOSED - MOISTURE_SCRUB))
}

/// The share of a stand's slots that are trees rather than scrub, from
/// the moisture at its origin.
pub fn trees_of(moisture: f64) -> f64 {
    smoothstep((moisture - MOISTURE_TREES) / (MOISTURE_CLOSED - MOISTURE_TREES))
}

/// The moisture a stand's origin reads: the share of the sky's the rock
/// keeps, raised toward the closed forest's by how low on the valley's
/// wall it lies, and to it on a basin's flooded floor, whatever the rock.
pub fn ground_moisture(sky: f64, retention: f64, wall: f64, flooded: bool) -> f64 {
    let held = sky * retention;
    if flooded {
        return held.max(MOISTURE_CLOSED);
    }
    let foot = (1.0 - wall.clamp(0.0, 1.0)).powi(3);
    held + (MOISTURE_CLOSED - held).max(0.0) * foot
}

/// A stand's density at a share `u` of its radius from the origin: full
/// to [`TAPER_FROM`], then easing to nothing at the edge.
pub fn taper(u: f64) -> f64 {
    if u < TAPER_FROM { 1.0 } else { smoothstep((1.0 - u) / (1.0 - TAPER_FROM)) }
}

/// The gallery along a channel at a share `wall` of the way up the
/// valley's wall: closed across the belt, gone a short way up.
pub fn gallery(wall: f64) -> f64 {
    DENSITY_MAX * smoothstep((GALLERY_SHARE - wall) / GALLERY_SHARE)
}

/// The coasts a cell's origins can read, from the edges plates published
/// under the cell and its ring: the coast edges whose straight line lies
/// within the shore ramp's reach of the cell, with the swing a chain and
/// the warp take off it. The rest would be built and never read.
fn coasts_of(scope: &CellScope) -> Coasts {
    let (cq, cr) = scope.lattice().cell_center(scope.cell());
    let (cx, cy) = hex_to_world(cq, cr);
    let within = scope.lattice().radius as f64 + COAST_READ;
    let edges = scope.read::<PlateEdgeIndex>();
    let near = |e: &&Edge| e.is_coast() && Segment::along((e.x0, e.y0), (e.x1, e.y1), true).distance(cx, cy).0 <= within;
    Coasts::new(edges.iter().flat_map(|idx| idx.entries().flatten()).filter(near), scope.seed())
}

/// The channel axes in reach of a cell's origins, from the channels
/// migration published under the cell and its ring.
fn axes_of(scope: &CellScope) -> SegmentGrid {
    let (cq, cr) = scope.lattice().cell_center(scope.cell());
    let (cx, cy) = hex_to_world(cq, cr);
    let within = scope.lattice().radius as f64 + 2.0 * VALLEY_HALF_WIDTH;
    let mut segments = Vec::new();
    if let Some(idx) = scope.read::<ChannelIndex>() {
        for c in idx.entries() {
            for ch in &c.channels {
                let near = ch.axis.iter().any(|&(x, y)| (x - cx).hypot(y - cy) <= within);
                if !near {
                    continue;
                }
                for w in ch.axis.windows(2) {
                    segments.push(Segment::along(w[0], w[1], true));
                }
            }
        }
    }
    SegmentGrid::new(segments, VALLEY_HALF_WIDTH)
}

/// The stands originating in a cell, each with its climate read at its
/// origin.
fn stands_of(scope: &CellScope) -> Vec<Stand> {
    let seed = scope.seed();
    let lattice = scope.lattice();
    let cell = scope.cell();
    let stands = stand_lattice();
    let wind = wind(seed);

    // Every index is deformed under the footprint before any guard is
    // held: a read guard held across a lower layer's deform waits on the
    // write that deform needs, and nothing moves.
    let axes = axes_of(scope);
    scope.source_cells::<OutlineIndex>();
    scope.source_cells::<DrainageIndex>();
    let coasts = coasts_of(scope);
    let outlines = scope.read::<OutlineIndex>();
    let drainage = scope.read::<DrainageIndex>();
    let graph = HexLattice::new(GRAPH_CELL_SCALE);
    let mut by_graph_cell: HashMap<CellId, Arc<Outlines>> = HashMap::new();

    let mut out = Vec::new();
    for id in candidate_origins(lattice, cell) {
        let (ox, oy) = origin_of(&stands, id, seed);
        let (oq, or) = world_to_hex(ox, oy);
        if lattice.cell_id(oq, or) != cell {
            continue;
        }
        let graph_cell = graph.cell_id(oq, or);
        let outline = by_graph_cell
            .entry(graph_cell)
            .or_insert_with(|| outlines.as_ref().and_then(|idx| idx.entry(graph_cell).cloned()).unwrap_or_else(|| Arc::new(Outlines::new(&[], seed))))
            .clone();
        let sky = sky_moisture(ox, oy, wind, &coasts, &outline);
        let wall = axes.nearest(ox, oy, VALLEY_HALF_WIDTH).map_or(1.0, |n| n.distance / VALLEY_HALF_WIDTH);
        let flooded = drainage
            .as_ref()
            .and_then(|idx| idx.node(nearest_node(ox, oy)))
            .is_some_and(|n| n.flooded);
        let retention = rock_on(ox, oy, seed, &coasts, &outline).rock.retention();
        let moisture = ground_moisture(sky, retention, wall, flooded);
        let density = density_of(moisture);
        if density <= 0.0 {
            continue;
        }
        // Forest and open ground are alternative states: a stand stands
        // with the probability its density gives, so the mosaic has edges.
        if hash_channel_f64(id.0 as i64, id.1 as i64, seed ^ STAND_SEED, 3) >= density / DENSITY_MAX {
            continue;
        }
        let spread = 2.0 * hash_channel_f64(id.0 as i64, id.1 as i64, seed ^ STAND_SEED, 4) - 1.0;
        let turn = 2.0 * PI * hash_channel_f64(id.0 as i64, id.1 as i64, seed ^ STAND_SEED, 5);
        out.push(Stand {
            wx: ox,
            wy: oy,
            radius: STAND_RADIUS * (1.0 + STAND_SPREAD * spread),
            stretch: STAND_STRETCH * hash_channel_f64(id.0 as i64, id.1 as i64, seed ^ STAND_SEED, 6),
            along: (turn.cos(), turn.sin()),
            density,
            trees: trees_of(moisture),
        });
    }
    out
}

/// What a cell's tiles share: every stand reaching the cell, from the
/// cell and its ring, bucketed by where it reaches.
pub struct Reach {
    grid: HexSpatialGrid<Stand>,
}

impl Reach {
    pub fn new(stands: impl Iterator<Item = Stand>) -> Self {
        let mut grid = HexSpatialGrid::new(STAND_REACH);
        for s in stands {
            grid.insert_radius(s.wx, s.wy, s.reach(), s);
        }
        Reach { grid }
    }

    /// The stands over a position as one: the densest of them, so thin
    /// woods overlapping stay thin, and the share of the filling that is
    /// trees, weighted by each stand's density there.
    pub fn at(&self, wx: f64, wy: f64) -> (f64, f64) {
        let mut densest: f64 = 0.0;
        let mut trees = 0.0;
        let mut weight = 0.0;
        if let Some(stands) = self.grid.cell_contents(self.grid.cell_at(wx, wy)) {
            for s in stands {
                let u = s.share_at(wx, wy);
                if u >= 1.0 {
                    continue;
                }
                let d = s.density * taper(u);
                densest = densest.max(d);
                trees += s.trees * d;
                weight += d;
            }
        }
        if weight <= 0.0 {
            return (0.0, 0.0);
        }
        (densest, trees / weight)
    }
}

/// The tree a slot holds at a temperature: pine where cold, deciduous
/// where warm, either between by the share of the way and the slot's hash.
pub fn tree_at(temperature: f64, mix: f64) -> Slot {
    let warm = (temperature - PINE_BELOW) / (DECIDUOUS_ABOVE - PINE_BELOW);
    if mix < warm { Slot::Deciduous } else { Slot::Pine }
}

/// The three draws slot `k` of tile `(q, r)` is filled by, each in
/// [0, 1): against the density, whether it holds anything; against the
/// tree share, scrub or a tree; and which tree.
fn slot_draws(q: i32, r: i32, k: usize, seed: u64) -> (f64, f64, f64) {
    (
        hash_channel_f64(q as i64, r as i64, seed, SLOT_FILL + k as u64),
        hash_channel_f64(q as i64, r as i64, seed, SLOT_KIND + k as u64),
        hash_channel_f64(q as i64, r as i64, seed, SLOT_MIX + k as u64),
    )
}

/// A tile's cover from the density and tree share it lands on at
/// `temperature`: each slot filled by its own draw against the density,
/// scrub or a tree by another against the share.
pub fn cover_of(q: i32, r: i32, density: f64, trees: f64, temperature: f64, seed: u64) -> Cover {
    let mut cover = Cover::NONE;
    for k in 0..SLOTS.len() {
        let (fill, kind, mix) = slot_draws(q, r, k, seed);
        if density <= fill {
            continue;
        }
        let slot = if kind >= trees { Slot::Scrub } else { tree_at(temperature, mix) };
        cover = cover.with(k, slot);
    }
    cover
}

// ── The event ───────────────────────────────────────────────────────────────

pub struct ForestEvent;

impl ForestEvent {
    pub fn new() -> Self { ForestEvent }
}

impl Default for ForestEvent {
    fn default() -> Self { Self::new() }
}

impl WorldEvent for ForestEvent {
    fn name(&self) -> &str { "forest" }
    fn scale(&self) -> u32 { FOREST_CELL_SCALE }

    /// A stand reaches its radius from an origin in the cell, and reads
    /// the coasts out to [`COAST_READ`].
    fn max_influence(&self) -> u32 { FOREST_REACH.ceil() as u32 }

    fn register_indexes(&self, registry: &mut IndexRegistry) {
        registry.pre_register::<StandIndex>();
    }

    /// The stands whose origins fall in this cell, each read at its origin.
    fn deform(&self, scope: &CellScope) {
        scope.publish::<StandIndex>(ForestCell { stands: stands_of(scope) });
    }

    /// Every stand reaching the cell, from the cell and its ring.
    fn prepare(&self, scope: &CellScope) -> Box<dyn Any + Send + Sync> {
        let stands: Vec<Stand> = scope
            .read::<StandIndex>()
            .map(|idx| idx.entries().flat_map(|c| c.stands.iter().cloned()).collect())
            .unwrap_or_default();
        Box::new(Reach::new(stands.into_iter()))
    }

    fn query(
        &self,
        q: i32, r: i32,
        below: &TileView,
        cell: &(dyn Any + Send + Sync),
        seed: u64,
    ) -> Option<TileOutput> {
        if below.water.is_some() {
            return None;
        }
        let reach = cell.downcast_ref::<Reach>()?;
        let (wx, wy) = hex_to_world(q, r);
        let t = temperature(wx, wy, below.elevation, seed);
        if t <= TREELINE {
            return None;
        }
        let (mut density, mut trees) = reach.at(wx, wy);
        if let Some(wall) = below.valley {
            let g = gallery(wall);
            if g > density {
                density = g;
                trees = 1.0;
            }
        }
        let band = smoothstep((t - TREELINE) / TREELINE_BAND);
        let cover = cover_of(q, r, density * band, trees * band, t, seed);
        if cover.is_empty() {
            return None;
        }
        Some(TileOutput { cover, ..TileOutput::default() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::Composite;
    use super::super::plates::unwarp;
    use crate::tectonic::PLATE_REACH;

    const S: u64 = 0x9E3779B97F4A7C15;
    const SPAWN: (i32, i32) = (104_289, -4_677);

    /// Density and the tree share rise with moisture, from nothing below
    /// the scrub line to full at the closed line, and a stand never fills
    /// every slot.
    #[test]
    fn density_and_trees_rise_with_moisture() {
        assert_eq!(density_of(MOISTURE_SCRUB), 0.0);
        assert_eq!(trees_of(MOISTURE_TREES), 0.0);
        let (mut d, mut t) = (0.0, 0.0);
        for i in 0..=100 {
            let m = i as f64 / 100.0;
            let (dd, tt) = (density_of(m), trees_of(m));
            assert!(dd >= d && tt >= t, "not monotone at {m}");
            assert!(dd <= DENSITY_MAX && tt <= 1.0);
            (d, t) = (dd, tt);
        }
        assert!((density_of(1.0) - DENSITY_MAX).abs() < 1e-12);
        assert_eq!(trees_of(1.0), 1.0);
    }

    /// Stands overlapping are as dense as the densest of them: two thin
    /// woods never make a closed one.
    #[test]
    fn overlap_takes_the_densest() {
        let stand = |wx: f64, density: f64| Stand { wx, wy: 0.0, radius: 100.0, stretch: 0.0, along: (1.0, 0.0), density, trees: 1.0 };
        let one = Reach::new([stand(0.0, 0.2)].into_iter()).at(0.0, 0.0).0;
        let two = Reach::new([stand(0.0, 0.2), stand(1.0, 0.2)].into_iter()).at(0.0, 0.0).0;
        let mixed = Reach::new([stand(0.0, 0.2), stand(1.0, 0.5)].into_iter()).at(0.0, 0.0).0;
        assert!((two - one).abs() < 0.01, "two thin woods {two} denser than one {one}");
        assert!(mixed > two && mixed <= 0.5);
    }

    /// The rock sets how much of the sky's water the ground keeps, so
    /// shale's ground is the wettest and limestone's the driest, while a
    /// valley's floor and a flooded basin are wet whatever the rock.
    #[test]
    fn the_rock_keeps_the_rain() {
        let rocks = [Rock::Limestone, Rock::Basement, Rock::Sandstone, Rock::Shale];
        for sky in [0.2, 0.42, 0.7] {
            let wet: Vec<f64> = rocks.iter().map(|r| ground_moisture(sky, r.retention(), 1.0, false)).collect();
            assert!(wet.windows(2).all(|w| w[0] <= w[1]), "not ordered by the rock at sky {sky}: {wet:?}");
            for r in rocks {
                assert!(ground_moisture(sky, r.retention(), 0.0, false) >= MOISTURE_CLOSED.min(sky));
                assert!(ground_moisture(sky, r.retention(), 1.0, true) >= MOISTURE_CLOSED);
            }
        }
    }

    /// The temperature falls with elevation at the lapse rate, so the
    /// treeline is a height, and the sea-level field stays in its spread.
    #[test]
    fn temperature_falls_at_the_lapse_rate() {
        let (wx, wy) = hex_to_world(SPAWN.0, SPAWN.1);
        let t0 = temperature(wx, wy, 0.0, S);
        assert!((t0 - SEA_LEVEL_MEAN).abs() <= SEA_LEVEL_SPREAD);
        assert!((temperature(wx, wy, 100.0, S) - (t0 - 100.0 * LAPSE)).abs() < 1e-9);
        assert_eq!(temperature(wx, wy, -50.0, S), t0, "the sea is at sea level");
    }

    /// Fullness never passes seven, is nothing at no density and every slot
    /// at a density past every hash, and never falls as density rises.
    #[test]
    fn slots_fill_with_density() {
        for (q, r) in [(0, 0), (SPAWN.0, SPAWN.1), (1_000_000, -2_000_000)] {
            assert!(cover_of(q, r, 0.0, 1.0, 20.0, S).is_empty());
            assert_eq!(cover_of(q, r, 1.0, 1.0, 20.0, S).fullness(), SLOTS.len() as u8);
            let mut last = 0;
            for i in 0..=20 {
                let f = cover_of(q, r, i as f64 / 20.0, 1.0, 20.0, S).fullness();
                assert!(f >= last, "fullness fell at {i}");
                last = f;
            }
        }
    }

    /// No slot holds a tree where the share is nothing, none holds scrub
    /// where it is everything, and the kind follows the temperature.
    #[test]
    fn kinds_follow_the_share_and_the_temperature() {
        let all = |f: &dyn Fn(Slot) -> bool, c: Cover| c.filled().all(|(_, s)| f(s));
        assert!(all(&|s| s == Slot::Scrub, cover_of(3, 4, 1.0, 0.0, 20.0, S)));
        assert!(all(&|s| s == Slot::Pine, cover_of(3, 4, 1.0, 1.0, PINE_BELOW - 1.0, S)));
        assert!(all(&|s| s == Slot::Deciduous, cover_of(3, 4, 1.0, 1.0, DECIDUOUS_ABOVE + 1.0, S)));
    }

    /// A stand's origin has one owner: over a cell and its neighbours, no
    /// lattice point's origin falls in two cells, every point near the
    /// centre falls in one, and every origin falling in the cell, out to
    /// its corners, is among its candidates.
    #[test]
    fn every_origin_has_one_owner() {
        let lattice = HexLattice::new(FOREST_CELL_SCALE);
        let home = lattice.cell_id(SPAWN.0, SPAWN.1);
        let mut owners: HashMap<CellId, Vec<CellId>> = HashMap::new();
        for cell in lattice.cells_within_distance(home, 1) {
            for id in candidate_origins(&lattice, cell) {
                let (ox, oy) = origin_of(&stand_lattice(), id, S);
                let (oq, or) = world_to_hex(ox, oy);
                if lattice.cell_id(oq, or) == cell {
                    owners.entry(id).or_default().push(cell);
                }
            }
        }
        assert!(owners.values().all(|v| v.len() == 1), "an origin with two owners");
        let stands = stand_lattice();
        let centre = lattice.cell_center(home);
        for id in stands.cells_within_distance(stands.cell_id(centre.0, centre.1), 2) {
            assert!(owners.contains_key(&id), "origin {id:?} near the centre has no owner");
        }
        let candidates: std::collections::HashSet<CellId> = candidate_origins(&lattice, home).into_iter().collect();
        let spacing = (stands.tiles_per_cell() as f64).sqrt();
        let rings = (2.0 * lattice.radius as f64 / spacing) as u32 + 2;
        for id in stands.cells_within_distance(stands.cell_id(centre.0, centre.1), rings) {
            let (ox, oy) = origin_of(&stands, id, S);
            let (oq, or) = world_to_hex(ox, oy);
            if lattice.cell_id(oq, or) == home {
                assert!(candidates.contains(&id), "origin {id:?} falls in the cell but is no candidate");
            }
        }
    }

    /// Behind a convergent edge, downwind of it, the shadow is deeper than
    /// on the foreland upwind of it: read off a real front near the spawn,
    /// with the wind set across it.
    #[test]
    fn a_belt_shadows_its_lee() {
        let (sx, sy) = hex_to_world(SPAWN.0, SPAWN.1);
        let outlines = Outlines::in_box(sx, sy, PLATE_REACH, S);
        let front = outlines
            .plates()
            .flat_map(|p| p.edges.iter().filter(|e| e.converge > 0.5).map(move |e| (p, e)))
            .min_by(|a, b| {
                let da = a.1.distance(sx, sy).0;
                let db = b.1.distance(sx, sy).0;
                da.partial_cmp(&db).unwrap()
            });
        let Some((plate, edge)) = front else {
            eprintln!("no strong front within a plate of the spawn; nothing to judge");
            return;
        };
        let mid = edge.segments[edge.segments.len() / 2];
        let (mx, my) = mid.mid();
        // The edge's normal points into the plate: the wind blows from the
        // foreland over the front into it. The chain is read through the
        // warp, so the positions are unwarped to land where they are put.
        let wind = (mid.nx, mid.ny);
        let rim = rim_of(sheets_of(edge.converge));
        let lee = unwarp(mx + mid.nx * (rim + 1500.0), my + mid.ny * (rim + 1500.0), S);
        let fore = unwarp(mx - mid.nx * 1500.0, my - mid.ny * 1500.0, S);
        let s_lee = shadow(lee.0, lee.1, wind, &outlines);
        let s_fore = shadow(fore.0, fore.1, wind, &outlines);
        assert!(s_lee > s_fore, "lee {s_lee} not deeper than foreland {s_fore} behind plate {:?}", plate.id);
        assert!(s_lee > 0.0 && s_lee <= 1.0);
        // Turned round, the foreland is the lee.
        let back = (-wind.0, -wind.1);
        assert!(shadow(fore.0, fore.1, back, &outlines) > shadow(lee.0, lee.1, back, &outlines));
    }

    /// The stack reads one cover for a tile however it is asked: the same
    /// from two composites of one seed, and after its neighbours.
    #[test]
    fn cover_is_deterministic() {
        let a = Composite::standard(S);
        let b = Composite::standard(S);
        let tiles: Vec<(i32, i32)> = (0..6).flat_map(|i| (0..6).map(move |j| (SPAWN.0 + i * 37, SPAWN.1 + j * 41))).collect();
        let first: Vec<Cover> = tiles.iter().map(|&(q, r)| a.cover_at(q, r)).collect();
        let second: Vec<Cover> = tiles.iter().rev().map(|&(q, r)| b.cover_at(q, r)).collect();
        for (i, &(q, r)) in tiles.iter().enumerate() {
            assert_eq!(first[i], second[tiles.len() - 1 - i], "cover at ({q}, {r}) differs");
            assert_eq!(a.cover_at(q, r), first[i]);
        }
    }
}
