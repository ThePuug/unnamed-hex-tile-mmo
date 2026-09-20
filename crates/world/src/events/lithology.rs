//! LithologyEvent — the rock at the surface, and what stands because of it.
//!
//! # Claims
//!
//! A continent is basement under a cover. The basement is the plate's
//! crystalline crust; the cover is the sequence of sedimentary formations
//! laid on it, sandstone, shale and limestone by turns as seas came and
//! went, a few kilometres thick where it is thickest. The cover is not
//! everywhere the same thickness: it sags into round intracratonic basins
//! four or five hundred kilometres across, the Michigan, the Williston,
//! the Paris, with three to five kilometres of section at the centre
//! against a kilometre on the platform around, and it is gone altogether
//! over the shields. An erosion surface planed nearly flat across gently
//! dipping beds cuts each formation in a band, so the outcrops of a basin
//! come in concentric rings, youngest at the centre, and the outcrops of
//! a tilted platform in belts along its strike. A range lifts the
//! sequence and erosion strips its crests, so a young belt carries its
//! cover to the summits and an old belt is basement-cored with the cover
//! wrapped around its flanks.
//!
//! Rocks erode at different rates. Shale and mudstone go fastest; sandstone
//! and limestone hold; basement holds hardest. So a formation that holds
//! stands above its neighbours as a cuesta: a scarp fifty to two hundred
//! metres high facing the older rocks updip, and a long dip slope down the
//! bed's own top surface to the next vale. Every river crossing the
//! platform reads that: it runs along the vales, crosses the scarps at
//! gaps, and cuts as fast as the rock it is in allows.
//!
//! # A field
//!
//! No index. The plate a position stands in carries its column, hashed:
//! the formations in order from the top with their thicknesses, resistant
//! and weak by turns. A basin potential says how much cover remains at the
//! position, and the relief above the platform says how much the plate's
//! age has stripped from it, so the depth into the column at the surface,
//! and with it the rock, is a function of position. The cuesta is a
//! function of that depth alone: how far the weak rock around the position
//! has been lowered against the resistant, held up where a harder bed lies
//! just beneath, which is what makes the dip slope, and stepped at every
//! contact, which is what makes the scarp. What stands is added to the
//! envelope, on land, and the rock's erodibility is what drainage,
//! migration and dissection read.
//!
//! Not lithology's: the sheets' own stratigraphy in a fold belt, the
//! ridge-and-valley a folded cover makes; volcanics; karst; the coastal
//! plain's unconsolidated wedge. Unbuilt.

use std::any::Any;

use crate::noise::{hash_channel_f64, simplex_2d};
use crate::tectonic::{aged, PlateId, PLATE_SPACING};
use crate::{hex_to_world, substrate_on};
use super::index::IndexRegistry;
use super::plates::{Coasts, PlateEdgeIndex, GRAPH_CELL_SCALE};
use super::thrusting::{outlines_of, Outlines};
use super::{CellScope, TileOutput, TileView, WorldEvent};

const COVER_SEED: u64 = 0x436f_7665_725f_5f5f; // "Cover___"
const COLUMN_THICKNESS: u64 = 0x7468_6b;
const COLUMN_PHASE: u64 = 0x7068_61;

// ── The rock ────────────────────────────────────────────────────────────────

/// A kind of rock, by what it does under water: how fast a river cuts it,
/// how it stands against its neighbours.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rock {
    Shale,
    Sandstone,
    Limestone,
    Basement,
}

impl Rock {
    /// How fast a river cuts this rock, as a share of the rate it cuts
    /// shale: shale the fastest, the sandstones and limestones of a cover
    /// about half as fast, crystalline basement a third. Everything a
    /// river does is scaled by this: where its channel begins, how deep it
    /// has cut, how far it has swept its banks.
    pub fn erodibility(self) -> f64 {
        match self {
            Rock::Shale => 1.0,
            Rock::Sandstone => 0.6,
            Rock::Limestone => 0.5,
            Rock::Basement => 0.3,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Rock::Shale => "shale",
            Rock::Sandstone => "sandstone",
            Rock::Limestone => "limestone",
            Rock::Basement => "basement",
        }
    }
}

// ── The column ──────────────────────────────────────────────────────────────

/// Formations in a plate's cover, from the top down.
pub const FORMATIONS: usize = 8;

/// A formation's thickness, in z-levels: two hundred to eight hundred
/// metres, the thickness at which a formation makes a landform of its own.
pub const FORMATION_MIN: f64 = 10.0;
pub const FORMATION_MAX: f64 = 40.0;

/// The order formations come in: a sea advancing lays sandstone, then
/// shale, then limestone as it deepens, and shale again as it goes, so
/// resistant and weak beds alternate down the column.
const CYCLE: [Rock; 4] = [Rock::Sandstone, Rock::Shale, Rock::Limestone, Rock::Shale];

/// A plate's cover: its formations from the top down, each a rock and a
/// thickness, and the whole's thickness. Basement lies beneath.
#[derive(Clone, Debug)]
pub struct Column {
    pub beds: Vec<(Rock, f64)>,
    pub total: f64,
}

impl Column {
    /// The column of `plate`: the cycle entered at a hashed phase, each
    /// formation's thickness hashed.
    pub fn of(plate: PlateId, seed: u64) -> Self {
        let (a, b) = (plate.0 as i64, plate.1 as i64);
        let phase = (hash_channel_f64(a, b, seed, COLUMN_PHASE) * CYCLE.len() as f64) as usize;
        let beds: Vec<(Rock, f64)> = (0..FORMATIONS)
            .map(|k| {
                let u = hash_channel_f64(a, b, seed, COLUMN_THICKNESS.wrapping_add(k as u64));
                (CYCLE[(phase + k) % CYCLE.len()], FORMATION_MIN + (FORMATION_MAX - FORMATION_MIN) * u)
            })
            .collect();
        let total = beds.iter().map(|b| b.1).sum();
        Column { beds, total }
    }

    /// The bed at `depth` into the column from its top: its index, its
    /// rock, and how far into it the depth lies; past the column, basement
    /// at the index one past the last bed.
    pub fn bed_at(&self, depth: f64) -> (usize, Rock, f64) {
        let mut top = 0.0;
        for (k, &(rock, thick)) in self.beds.iter().enumerate() {
            if depth < top + thick {
                return (k, rock, depth - top);
            }
            top += thick;
        }
        (self.beds.len(), Rock::Basement, depth - top)
    }

    fn rock_of(&self, k: usize) -> Rock {
        self.beds.get(k).map_or(Rock::Basement, |b| b.0)
    }

    /// How far the ground at `depth` into the column has been lowered
    /// against the most resistant rock, in z-levels: the weak rock's full
    /// lowering, unless a harder bed lies within it beneath, which holds
    /// the floor up on its own top, the dip slope. Stepped at every contact
    /// over [`CONTACT_RAMP`] of column, the scarp.
    fn lowering(&self, depth: f64) -> f64 {
        let (k, _, into) = self.bed_at(depth);
        let own = |k: usize| CUESTA_RELIEF * self.rock_of(k).erodibility();
        // Lowering within bed k at `d` into it: its own, held up by the
        // first harder bed beneath at that bed's own lowering plus the
        // rock still to cut down to it.
        let held = |k: usize, d: f64| -> f64 {
            let mut lowering = own(k);
            let mut below = self.beds.get(k).map_or(f64::INFINITY, |b| b.1 - d);
            for j in k + 1..=self.beds.len() {
                if self.rock_of(j).erodibility() < self.rock_of(k).erodibility() {
                    lowering = lowering.min(below + own(j));
                    break;
                }
                below += self.beds.get(j).map_or(f64::INFINITY, |b| b.1);
            }
            lowering
        };
        let here = held(k, into);
        let thick = self.beds.get(k).map_or(f64::INFINITY, |b| b.1);
        // Across the ramp centred on the nearer contact, the lowering runs
        // from the bed above's at its base to the bed below's at its top;
        // a bed thinner than the ramp reads as its own middle.
        let half = 0.5 * CONTACT_RAMP;
        let (above, beneath, u) = if k > 0 && into < half {
            (held(k - 1, self.beds[k - 1].1), held(k, 0.0), (into + half) / CONTACT_RAMP)
        } else if k < self.beds.len() && thick - into < half {
            (held(k, thick), held(k + 1, 0.0), (half - (thick - into)) / CONTACT_RAMP)
        } else {
            return here;
        };
        let u = u.clamp(0.0, 1.0);
        above + (beneath - above) * u * u * (3.0 - 2.0 * u)
    }
}

// ── The cover ───────────────────────────────────────────────────────────────

/// Wavelength of the basin potential, in world units: a plate's width, so
/// a plate holds a basin or an arch or two, the four or five hundred
/// kilometres of an intracratonic basin on a continent of a few plates.
pub const BASIN_WAVELENGTH: f64 = PLATE_SPACING;

/// Cover on the platform, in z-levels, and how far the basins and arches
/// swing it: a kilometre of section on the platform, three and a half at a
/// basin's centre, and none over an arch's crest, the shield.
pub const COVER_MEAN: f64 = 60.0;
pub const COVER_SWING: f64 = 120.0;

/// The cover remaining at a position, in z-levels; below zero is shield.
pub fn cover(wx: f64, wy: f64, seed: u64) -> f64 {
    COVER_MEAN + COVER_SWING * simplex_2d(wx / BASIN_WAVELENGTH, wy / BASIN_WAVELENGTH, seed ^ COVER_SEED)
}

// ── The cuesta ──────────────────────────────────────────────────────────────

/// How far the weakest rock is lowered against the most resistant, in
/// z-levels: the hundred and sixty metres of a cratonic cuesta's scarp,
/// between the Flint Hills' sixty and the Niagara's hundred and more.
pub const CUESTA_RELIEF: f64 = 8.0;

/// The column depth over which the lowering steps at a contact, in
/// z-levels: at the platform's dip that is a scarp a few tens of tiles
/// across, steeper where the beds dip harder.
const CONTACT_RAMP: f64 = 1.0;

/// Substrate elevation at which the cuestas stand fully, in z-levels: only
/// land stands, and the beach band tapers, as the tilt's does.
const STAND_FULL_ELEVATION: f64 = 9.0;

fn shore_gate(substrate: f64) -> f64 {
    let t = (substrate / STAND_FULL_ELEVATION).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The rock at a position, and what it does to the ground there.
#[derive(Clone, Copy, Debug)]
pub struct Ground {
    pub rock: Rock,
    /// The rock's, as [`Rock::erodibility`].
    pub erodibility: f64,
    /// How far the ground stands above the vales around it, in z-levels:
    /// what is added to the envelope.
    pub stand: f64,
    /// Depth into the column at the surface, in z-levels.
    pub exposed: f64,
}

/// The rock at a position on `plate`, given the substrate beneath, the
/// relief the ranges stand above it, and the plate's age: the column
/// entered at the depth the cover leaves exposed, deeper by what the age
/// has stripped from the relief; lowered by the cuesta rule, so the rock
/// read is the rock at the lowered floor, the dip slope showing the bed
/// beneath. The relief is the ranges' and not the plateau's: a plateau is
/// uplift, and keeps its cover flat-lying to be cut in canyons, while a
/// range's crest is what erosion has bitten into.
pub fn rock_at(wx: f64, wy: f64, seed: u64, plate: PlateId, age: f64, substrate: f64, relief: f64) -> Ground {
    let column = Column::of(plate, seed);
    let exposed = (column.total - cover(wx, wy, seed) + age * relief.max(0.0)).max(0.0);
    let lowering = column.lowering(exposed) * aged(age);
    let stand = (CUESTA_RELIEF * aged(age) - lowering).max(0.0) * shore_gate(substrate);
    let (_, rock, _) = column.bed_at(exposed + lowering);
    Ground { rock, erodibility: rock.erodibility(), stand, exposed }
}

/// The ground off any plate: the sea floor, weak and standing nowhere.
pub fn off_plate() -> Ground {
    Ground { rock: Rock::Shale, erodibility: 1.0, stand: 0.0, exposed: 0.0 }
}

/// The rock at a position read through the outlines and coasts a view or
/// a probe built: what the event's own query reads through its cell.
pub fn rock_on(wx: f64, wy: f64, seed: u64, coasts: &Coasts, outlines: &Outlines) -> Ground {
    let substrate = substrate_on(wx, wy, coasts, seed);
    let Some(at) = outlines.at(wx, wy) else { return off_plate() };
    let relief = outlines.relief_of(&at).max(0.0);
    rock_at(wx, wy, seed, at.plate.id, at.plate.age, substrate, relief)
}

// ── The event ───────────────────────────────────────────────────────────────

pub struct LithologyEvent;

impl LithologyEvent {
    pub fn new() -> Self { LithologyEvent }
}

impl Default for LithologyEvent {
    fn default() -> Self { Self::new() }
}

/// What a cell's tiles read: the coasts and outlines in reach.
struct Reach {
    coasts: Coasts,
    outlines: Outlines,
}

impl WorldEvent for LithologyEvent {
    fn name(&self) -> &str { "lithology" }
    fn scale(&self) -> u32 { GRAPH_CELL_SCALE }

    /// Nothing originates here.
    fn max_influence(&self) -> u32 { 0 }

    fn register_indexes(&self, _registry: &mut IndexRegistry) {}

    /// Nothing to place: the rock is read off the plate graph.
    fn deform(&self, _scope: &CellScope) {}

    fn prepare(&self, scope: &CellScope) -> Box<dyn Any + Send + Sync> {
        let edge_cells = scope.source_cells::<PlateEdgeIndex>();
        let coasts = Coasts::new(&scope.read::<PlateEdgeIndex>().map(|idx| idx.edges_in(&edge_cells)).unwrap_or_default(), scope.seed());
        Box::new(Reach { coasts, outlines: outlines_of(scope) })
    }

    /// What stands at the tile: the cuesta over the envelope beneath.
    fn query(
        &self,
        q: i32, r: i32,
        _below: &TileView,
        cell: &(dyn Any + Send + Sync),
        seed: u64,
    ) -> Option<TileOutput> {
        let reach = cell.downcast_ref::<Reach>()?;
        let (wx, wy) = hex_to_world(q, r);
        let ground = rock_on(wx, wy, seed, &reach.coasts, &reach.outlines);
        if ground.stand <= 0.0 { return None }
        Some(TileOutput { elevation_delta: ground.stand, ..TileOutput::default() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: u64 = 0x9E3779B97F4A7C15;

    /// A column has its formations, resistant and weak by turns, each
    /// within the thickness a formation comes in; two plates differ; and
    /// the depth lookup walks it from the top to basement.
    #[test]
    fn a_column_alternates_and_differs_by_plate() {
        let c = Column::of((3, 4), S);
        assert_eq!(c.beds.len(), FORMATIONS);
        for pair in c.beds.windows(2) {
            assert_ne!(pair[0].0.erodibility(), pair[1].0.erodibility(), "two beds alike in a row");
        }
        for &(_, t) in &c.beds {
            assert!(t >= FORMATION_MIN && t <= FORMATION_MAX);
        }
        assert!((c.total - c.beds.iter().map(|b| b.1).sum::<f64>()).abs() < 1e-12);
        let d = Column::of((5, 4), S);
        assert!(c.beds.iter().zip(&d.beds).any(|(a, b)| (a.1 - b.1).abs() > 1e-9));
        assert_eq!(c.bed_at(0.0).0, 0);
        assert_eq!(c.bed_at(c.total + 1.0).1, Rock::Basement);
        let (k, rock, into) = c.bed_at(c.beds[0].1 + 0.5);
        assert_eq!((k, rock), (1, c.beds[1].0));
        assert!((into - 0.5).abs() < 1e-12);
    }

    /// The lowering is a weak bed's full lowering deep in a weak bed, a
    /// resistant bed's own on it, held up on a harder bed's top just
    /// beneath a weak one, and continuous down the column.
    #[test]
    fn weak_rock_is_lowered_and_held_up_by_hard_rock_beneath() {
        let c = Column::of((3, 4), S);
        let mut last = c.lowering(0.0);
        let mut steps = 0;
        let mut d = 0.0;
        while d < c.total + 20.0 {
            let l = c.lowering(d);
            assert!(l >= 0.0 && l <= CUESTA_RELIEF + 1e-9, "lowering {l} at {d}");
            assert!((l - last).abs() < 0.6, "a step of {} in the lowering at {d}", (l - last).abs());
            last = l;
            d += 0.1;
            steps += 1;
        }
        assert!(steps > 100);
        // Deep in a weak bed over a hard one the floor is held up: within
        // the cuesta's relief of the hard bed's top the lowering is less
        // than the weak bed's own, and right at the top it is the hard's.
        for (k, &(rock, thick)) in c.beds.iter().enumerate() {
            if rock != Rock::Shale || k + 1 >= c.beds.len() {
                continue;
            }
            let top: f64 = c.beds[..k].iter().map(|b| b.1).sum();
            let deep = c.lowering(top + CONTACT_RAMP + 0.5);
            let on_hard = c.lowering(top + thick - CONTACT_RAMP - 0.1);
            assert!(on_hard < deep, "the floor on the hard bed's top is not held up: {on_hard} for {deep}");
            assert!(on_hard <= CUESTA_RELIEF * c.beds[k + 1].0.erodibility() + CONTACT_RAMP + 0.1 + 1e-9);
        }
    }

    /// The rock read is the column's at the depth the cover leaves, older
    /// where the cover is thinner and stripped from relief by age; what
    /// stands is nothing at sea and never more than the cuesta's relief.
    #[test]
    fn rock_follows_the_cover_and_the_stripping() {
        let plate = (0, 0);
        let col = Column::of(plate, S);
        let x = 12_345.0;
        let thin = rock_at(x, 777.0, S, plate, 1.0, 20.0, 0.0);
        assert!(thin.exposed >= col.total - cover(x, 777.0, S) - 1e-9);
        let stripped = rock_at(x, 777.0, S, plate, 1.0, 20.0, 400.0);
        assert_eq!(stripped.rock, Rock::Basement, "an aged range's crest keeps its cover");
        let young = rock_at(x, 777.0, S, plate, 0.0, 20.0, 400.0);
        assert!(young.exposed < stripped.exposed, "a young range is stripped as far as an old one");
        let sea = rock_at(x, 777.0, S, plate, 1.0, -5.0, 0.0);
        assert_eq!(sea.stand, 0.0);
        for i in 0..200 {
            let g = rock_at(x + i as f64 * 97.0, 777.0 - i as f64 * 53.0, S, plate, 0.7, 30.0, 0.0);
            assert!(g.stand >= 0.0 && g.stand <= CUESTA_RELIEF + 1e-9);
            assert!((g.erodibility - g.rock.erodibility()).abs() < 1e-12);
        }
    }
}

