//! LandformEvent — the few dramatic places a landscape is read by, each
//! carved at its own full size where the ground beneath makes it
//! reasonable: the scarp.
//!
//! # Claims
//!
//! A landscape is read by its few dramatic places, never by a rule applied
//! to every slope. The layers beneath say what kind of ground a place is,
//! and a landform is a place on that ground, sited as dens and outcrops are
//! and carved with its own profile, so geomorphology that takes a change of
//! structure or climate to produce on Earth stands within a walk of the
//! next. The field layers keep the envelope and the context; the drama is
//! the landforms'.
//!
//! Sites stand on a sparse jittered lattice, about ten minutes' walk apart,
//! each a pure function of its lattice cell's hash. What stands at a site
//! is decided from the ground at the site alone, through the layers' own
//! functions at that position, and every tile in the site's footprint reads
//! that one decision, so a landform is one thing across its extent. A site
//! on ground that fits no idiom shows nothing, which keeps landforms few. A
//! landform carves within its footprint and is nothing at its rim, and it
//! is carved before drainage routes, so water follows what it cuts.
//!
//! The scarp is Wallace's profile: a flat top, a free face of bare rock, a
//! debris slope at repose beneath it, a wash slope back to the toe. A site
//! stands one where a hard bed lies at the surface on ground gentler than
//! repose with a weak bed beneath it in the column: the face is the hard
//! bed's updip edge, where the weak bed under it has been cut back. The
//! scarp line is the contact itself, the iso-line of the depth lithology
//! exposes at the hard bed's base, so the face follows the contact's every
//! bend; a tile reads its place across the profile from its own exposed
//! depth and the contact's over the depth's gradient at the site. Downdip of
//! the contact the ground is the envelope, the flat the player walks out to
//! the edge of. Updip the free face falls the hard bed's thickness at
//! [`CLIFF_GRADE`], the debris slope the weak bed's at repose, and the wash
//! slope climbs back to the envelope at [`WASH_GRADE`]: the vale. Over the
//! last quarter of the footprint's radius, and toward the plate's edges, the
//! drop tapers to nothing, so the face ends by diminishing.
//!
//! Not landforms' yet: the cut ravine, the gully that carries a flank's
//! water, the plain, the bowl, and the scarp at a plateau's edge. Unbuilt.
//!
//! # The window
//!
//! The cell is the graph's, as lithology's is, so its tiles read the
//! outlines thrusting published for it. A tile finds the sites of its own
//! site cell and the six around it, seven hashes and distance checks, and
//! reads the decision of any whose footprint holds it; the site lattice's
//! ring holds every footprint and stray reaching a tile. The cell keeps
//! each site's decision, made from the ground at the site the first time a
//! tile of the cell reads it, so no decision is made per tile and every
//! tile of a footprint reads the same one. Drainage keeps its own decisions,
//! made by the same function from the same fields, so the surface it routes
//! on is the composite's.

use std::any::Any;
use std::sync::Arc;

use dashmap::DashMap;

use common::HexLattice;

use crate::lattice::jittered_centre;
use crate::tectonic::PlateId;
use crate::{hex_to_world, smoothstep, world_to_hex, RISE};
use super::drainage::envelope_at;
use super::index::CellId;
use super::lithology::{rock_on, Column, FORMATION_MAX};
use super::outcrop::{SITE_JITTER, SITE_LATTICE};
use super::plates::{coasts_of, Coasts, GRAPH_CELL_SCALE};
use super::thrusting::{outlines_for, Outlines, OUTLINE_REACH, REPOSE_GRADE};
use super::{gradient_of, CellScope, TileOutput, TileView, WorldEvent};

const LANDFORM_SEED: u64 = 0x4c61_6e64_666f_726d; // "Landform"

/// The radius of a landform site lattice cell, in tiles: the outcrops'
/// lattice, whose sites stand three minutes' walk apart, widened to ten,
/// some 2,500 tiles or four kilometres.
pub const LANDFORM_LATTICE: u32 = SITE_LATTICE * 10 / 3;

/// tan of a free face's angle, seventy degrees. A const cannot call `tan`;
/// a test holds the two equal.
const CLIFF_TAN: f64 = 2.747_477_419_454_622;

/// A free face's fall per tile, in z-levels: seventy degrees, 4.8 metres
/// down for every 1.7 metres across, bare rock standing as steep as a
/// bedded cliff does.
pub const CLIFF_GRADE: f64 = CLIFF_TAN / RISE;

/// tan of a wash slope's angle, five degrees.
const WASH_TAN: f64 = 0.087_488_663_525_924;

/// A wash slope's climb per tile, in z-levels: five degrees, fifteen
/// centimetres a tile, the few degrees a vale floor rises at from a
/// scarp's toe.
pub const WASH_GRADE: f64 = WASH_TAN / RISE;

/// The share of repose a scarp's site may slope at, at most: a tenth
/// gentler than repose, so the scree a range's forelimb lies at stands
/// none.
const SCARP_GROUND: f64 = 0.9;

/// The share of a footprint's radius over which its drop tapers to
/// nothing: its last quarter.
const TAPER: f64 = 0.25;

/// How far a site's footprint reaches from it, in tiles: room inside the
/// untapered three quarters for the longest scarp the beds make, the
/// thickest hard bed's face, the thickest weak bed's debris slope and the
/// wash back up from both, some 900 tiles, a kilometre and a half.
pub const FOOTPRINT: f64 = (FORMATION_MAX / CLIFF_GRADE + FORMATION_MAX / REPOSE_GRADE + 2.0 * FORMATION_MAX / WASH_GRADE) / (1.0 - TAPER);

/// The landform site lattice.
pub fn landform_lattice() -> HexLattice {
    HexLattice::new(LANDFORM_LATTICE)
}

/// Where the site of a lattice cell stands: its centre, jittered by the
/// cell's hash as an outcrop's is.
pub fn site_of(lattice: &HexLattice, id: CellId, seed: u64) -> (f64, f64) {
    jittered_centre(lattice, id, SITE_JITTER, seed ^ LANDFORM_SEED)
}

// ── The scarp ───────────────────────────────────────────────────────────────

/// A scarp as its site decided it: where the site is, the plate it stands
/// on, the depth into the column of the contact its face stands at, how
/// fast the exposed depth deepens updip there, in z-levels of column per
/// tile, and the face's and the debris slope's heights, the hard bed's and
/// the weak bed's thicknesses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scarp {
    pub x: f64,
    pub y: f64,
    pub plate: PlateId,
    pub contact: f64,
    pub slope: f64,
    pub face: f64,
    pub debris: f64,
}

/// How far below the envelope Wallace's profile stands at `s` tiles updip
/// of the contact, in z-levels: nothing downdip; the free face falling
/// `face` at [`CLIFF_GRADE`], the debris slope falling `debris` at repose,
/// and the wash slope climbing back to nothing at [`WASH_GRADE`].
/// Continuous, and never steeper than its grades.
pub fn profile(s: f64, face: f64, debris: f64) -> f64 {
    let toe = face / CLIFF_GRADE + debris / REPOSE_GRADE;
    if s <= 0.0 {
        0.0
    } else if s <= face / CLIFF_GRADE {
        CLIFF_GRADE * s
    } else if s <= toe {
        face + REPOSE_GRADE * (s - face / CLIFF_GRADE)
    } else {
        (face + debris - WASH_GRADE * (s - toe)).max(0.0)
    }
}

/// How far a scarp of a `face` and a `debris` slope reaches updip of its
/// contact, in tiles: its face, its debris slope and its wash.
#[cfg(test)]
fn extent(face: f64, debris: f64) -> f64 {
    face / CLIFF_GRADE + debris / REPOSE_GRADE + (face + debris) / WASH_GRADE
}

/// What stands at the site at `(x, y)`, from the ground there through the
/// layers' own functions: a scarp where a hard bed lies at the surface with a
/// weaker one beneath it in the column, on ground gentler than repose by
/// [`SCARP_GROUND`] and above the scarp's own height from the sea, and where
/// the contact lies near enough updip that the face and its debris slope
/// stand inside the untapered footprint. The exposed depth is read again at
/// the face's foot, along the site's gradient, so the column there is cut
/// through the whole hard bed and the foot stands on land: a depth that turns
/// before the face is down makes no scarp. Nothing elsewhere.
pub fn decide(x: f64, y: f64, seed: u64, coasts: &Coasts, outlines: &Outlines) -> Option<Scarp> {
    let at = outlines.at(x, y)?;
    let ground = rock_on(x, y, seed, coasts, outlines);
    let column = Column::of(at.plate.id, seed);
    let (k, rock, _) = column.bed_at(ground.exposed);
    let &(beneath, debris) = column.beds.get(k + 1)?;
    if beneath.erodibility() <= rock.erodibility() {
        return None;
    }
    let face = column.beds[k].1;
    let contact: f64 = column.beds[..=k].iter().map(|b| b.1).sum();
    let gradient = gradient_of(x, y, ground.exposed, |x, y| rock_on(x, y, seed, coasts, outlines).exposed);
    let slope = gradient.0.hypot(gradient.1);
    if slope <= 1e-6 {
        return None;
    }
    let envelope = envelope_at(x, y, seed, coasts, outlines).surface;
    let grade = gradient_of(x, y, envelope, |x, y| envelope_at(x, y, seed, coasts, outlines).surface);
    if grade.0.hypot(grade.1) >= SCARP_GROUND * REPOSE_GRADE || envelope <= face + debris {
        return None;
    }
    let behind = (contact - ground.exposed) / slope;
    let toe = face / CLIFF_GRADE + debris / REPOSE_GRADE;
    if behind + toe > (1.0 - TAPER) * FOOTPRINT {
        return None;
    }
    let foot = behind + face / CLIFF_GRADE;
    let (fx, fy) = (x + gradient.0 / slope * foot, y + gradient.1 / slope * foot);
    let deep = rock_on(fx, fy, seed, coasts, outlines).exposed;
    if deep - contact < face / CLIFF_GRADE * slope || envelope_at(fx, fy, seed, coasts, outlines).surface <= face + debris {
        return None;
    }
    Some(Scarp { x, y, plate: at.plate.id, contact, slope, face, debris })
}

impl Scarp {
    /// How deep the scarp carves below the envelope at a position `r` from
    /// its site, `edge` from its plate's nearest other edge, where the
    /// column is exposed `exposed` deep: the profile at the position's
    /// place across it, tapered to nothing over the footprint's last
    /// quarter and as near the plate's edges.
    fn carve(&self, exposed: f64, r: f64, edge: f64) -> f64 {
        let s = (exposed - self.contact) / self.slope;
        let rim = 1.0 - smoothstep((r / FOOTPRINT - (1.0 - TAPER)) / TAPER);
        let edge = smoothstep(edge / (TAPER * FOOTPRINT));
        profile(s, self.face, self.debris) * rim * edge
    }
}

// ── The landforms ───────────────────────────────────────────────────────────

/// The landforms a set of tiles reads: the site lattice, and the decision
/// of each site any of them has read, kept so a site is decided once.
pub struct Landforms {
    lattice: HexLattice,
    decided: DashMap<CellId, Option<Scarp>>,
}

impl Default for Landforms {
    fn default() -> Self {
        Self::new()
    }
}

impl Landforms {
    pub fn new() -> Self {
        Landforms { lattice: landform_lattice(), decided: DashMap::new() }
    }

    /// The decision of the site of lattice cell `id`, standing at `(x, y)`.
    fn decision(&self, id: CellId, x: f64, y: f64, seed: u64, coasts: &Coasts, outlines: &Outlines) -> Option<Scarp> {
        if let Some(d) = self.decided.get(&id) {
            return *d;
        }
        let d = decide(x, y, seed, coasts, outlines);
        self.decided.insert(id, d);
        d
    }

    /// How deep the landforms carve below an envelope `envelope` high at a
    /// position, in z-levels: the deepest of the scarps whose footprints
    /// hold it, of the sites of its own site cell and the six around it.
    /// A cut never takes land below the sea, so it reaches no deeper than
    /// the envelope stands above it.
    pub fn carve_at(&self, wx: f64, wy: f64, seed: u64, coasts: &Coasts, outlines: &Outlines, envelope: f64) -> f64 {
        let (q, r) = world_to_hex(wx, wy);
        let own = self.lattice.cell_id(q, r);
        let mut deepest = 0.0f64;
        let mut here: Option<Option<(PlateId, f64, f64)>> = None;
        for id in std::iter::once(own).chain(self.lattice.neighbor_cells(own)) {
            let (x, y) = site_of(&self.lattice, id, seed);
            let reach = (x - wx).hypot(y - wy);
            if reach >= FOOTPRINT {
                continue;
            }
            let Some(scarp) = self.decision(id, x, y, seed, coasts, outlines) else { continue };
            let standing = *here.get_or_insert_with(|| {
                let at = outlines.at(wx, wy)?;
                let edge = at.distances.iter().copied().fold(f64::MAX, f64::min);
                Some((at.plate.id, rock_on(wx, wy, seed, coasts, outlines).exposed, edge))
            });
            let Some((plate, exposed, edge)) = standing else { continue };
            if plate != scarp.plate {
                continue;
            }
            deepest = deepest.max(scarp.carve(exposed, reach, edge));
        }
        deepest.min(envelope.max(0.0))
    }
}

// ── The event ───────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct LandformEvent;

impl LandformEvent {
    pub fn new() -> Self {
        LandformEvent
    }
}

/// What a cell's tiles read: the coasts and outlines in reach, and the
/// decisions of the sites they have read.
struct Reach {
    coasts: Coasts,
    outlines: Arc<Outlines>,
    landforms: Landforms,
}

impl WorldEvent for LandformEvent {
    fn name(&self) -> &str { "landform" }
    fn scale(&self) -> u32 { GRAPH_CELL_SCALE }

    /// A tile reads the whole outline of the plate it stands in, which
    /// reaches further than any footprint and a site's stray.
    fn max_influence(&self) -> u32 {
        let stray = SITE_JITTER * (landform_lattice().tiles_per_cell() as f64).sqrt();
        OUTLINE_REACH.max(FOOTPRINT + stray) as u32
    }

    /// Nothing to place: a site is its lattice cell's hash.
    fn deform(&self, _scope: &CellScope) {}

    fn prepare(&self, scope: &CellScope) -> Box<dyn Any + Send + Sync> {
        Box::new(Reach { coasts: coasts_of(scope), outlines: outlines_for(scope), landforms: Landforms::new() })
    }

    /// What a landform carves at the tile below the envelope, and how
    /// steeply.
    fn query(&self, q: i32, r: i32, below: &TileView, cell: &(dyn Any + Send + Sync), seed: u64) -> Option<TileOutput> {
        let reach = cell.downcast_ref::<Reach>()?;
        let (wx, wy) = hex_to_world(q, r);
        let carve = |x: f64, y: f64| reach.landforms.carve_at(x, y, seed, &reach.coasts, &reach.outlines, below.elevation);
        let cut = carve(wx, wy);
        if cut <= 0.0 {
            return None;
        }
        let gradient = gradient_of(wx, wy, -cut, |x, y| -carve(x, y));
        Some(TileOutput { elevation_delta: -cut, gradient, ..TileOutput::default() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: u64 = 0x9E3779B97F4A7C15;

    /// The cliff is seventy degrees and the wash five, as the constants
    /// are derived.
    #[test]
    fn the_grades_are_their_angles() {
        assert!((CLIFF_TAN - 70f64.to_radians().tan()).abs() < 1e-12);
        assert!((WASH_TAN - 5f64.to_radians().tan()).abs() < 1e-12);
        assert!(WASH_GRADE < REPOSE_GRADE && REPOSE_GRADE < CLIFF_GRADE);
    }

    /// Wallace's profile: nothing downdip, monotone down the face and the
    /// debris slope, back to nothing up the wash, continuous, and never
    /// steeper than the cliff on the face or repose on the debris slope.
    #[test]
    fn the_profile_is_wallaces() {
        for (face, debris) in [(20.0, 20.0), (60.0, 25.0), (30.0, 60.0)] {
            assert_eq!(profile(-5.0, face, debris), 0.0);
            let toe = face / CLIFF_GRADE + debris / REPOSE_GRADE;
            let step = 0.01;
            let mut s = 0.0;
            while s < extent(face, debris) + 10.0 {
                let (a, b) = (profile(s, face, debris), profile(s + step, face, debris));
                let grade = (b - a) / step;
                if s + step <= face / CLIFF_GRADE {
                    assert!(grade > 0.0 && grade <= CLIFF_GRADE + 1e-9, "the face at {s}");
                } else if s >= face / CLIFF_GRADE && s + step <= toe {
                    assert!(grade > 0.0 && grade <= REPOSE_GRADE + 1e-9, "the debris slope at {s}");
                } else if s >= toe {
                    assert!(grade <= 1e-12 && grade >= -WASH_GRADE - 1e-9, "the wash at {s}");
                }
                assert!((b - a).abs() <= CLIFF_GRADE * step + 1e-9, "a step at {s}");
                s += step;
            }
            assert!((profile(toe, face, debris) - (face + debris)).abs() < 1e-9);
            assert_eq!(profile(extent(face, debris) + 1.0, face, debris), 0.0);
        }
    }

    /// A site is a pure function of its cell, and the ring of site cells
    /// about any tile holds every site whose footprint reaches it.
    #[test]
    fn a_site_is_its_cell_and_its_ring_holds_every_reach() {
        let lattice = landform_lattice();
        assert_eq!(site_of(&lattice, (3, -2), 7), site_of(&lattice, (3, -2), 7));
        assert_ne!(site_of(&lattice, (3, -2), 7), site_of(&lattice, (3, -2), 8));
        for i in -30..30 {
            for j in -30..30 {
                let (q, r) = (i * 397, j * 389);
                let (wx, wy) = hex_to_world(q, r);
                let own = lattice.cell_id(q, r);
                let ring: Vec<CellId> = std::iter::once(own).chain(lattice.neighbor_cells(own)).collect();
                for id in lattice.cells_within_distance(own, 3) {
                    let (x, y) = site_of(&lattice, id, S);
                    if (x - wx).hypot(y - wy) < FOOTPRINT {
                        assert!(ring.contains(&id), "a site past the ring reaches ({q}, {r})");
                    }
                }
            }
        }
    }

    /// A scarp's carve is its profile at the tile's place across it, the
    /// same from any reader; nothing past the footprint's rim and nothing
    /// off its plate; and continuous to the rim.
    #[test]
    fn a_scarp_carves_its_footprint_alone() {
        let scarp = Scarp { x: 0.0, y: 0.0, plate: (0, 0), contact: 100.0, slope: 0.2, face: 30.0, debris: 30.0 };
        let far = TAPER * FOOTPRINT;
        assert_eq!(scarp.carve(150.0, FOOTPRINT, far), 0.0, "the rim is the envelope");
        assert_eq!(scarp.carve(150.0, 10.0, 0.0), 0.0, "the plate's edge is the envelope");
        assert_eq!(scarp.carve(90.0, 10.0, far), 0.0, "downdip is the envelope");
        assert!((scarp.carve(110.0, 10.0, far) - profile(50.0, 30.0, 30.0)).abs() < 1e-12);
        let mut last = scarp.carve(110.0, 0.0, far);
        let mut r = 0.0;
        while r < FOOTPRINT {
            let c = scarp.carve(110.0, r, far);
            assert!(c <= last + 1e-12 && last - c <= 1.5 * 0.5 * profile(50.0, 30.0, 30.0) / (TAPER * FOOTPRINT) + 1e-9, "the drop steps at {r} from the site");
            last = c;
            r += 0.5;
        }
    }

    /// The haven's belt, where its scarps stand.
    const BELT: (f64, f64) = (98_000.0, -2_000.0);

    /// The sites of the haven's belt.
    fn belt_sites() -> Vec<(f64, f64)> {
        let lattice = landform_lattice();
        let (q, r) = world_to_hex(BELT.0, BELT.1);
        lattice
            .cells_within_distance(lattice.cell_id(q, r), 3)
            .into_iter()
            .map(|id| site_of(&lattice, id, S))
            .filter(|(x, y)| (x - BELT.0).abs() < 7_000.0 && (y - BELT.1).abs() < 7_000.0)
            .collect()
    }

    /// A site's decision is the ground's at the site: the same from the
    /// outlines and coasts of any reach holding it, and some sites of the
    /// belt stand scarps.
    #[test]
    fn a_site_is_decided_alike_from_any_reach() {
        let (near_o, near_c) = (Outlines::in_box(BELT.0, BELT.1, 9_000.0, S), Coasts::in_box(BELT.0, BELT.1, 9_000.0, S));
        let mut scarps = 0;
        for (x, y) in belt_sites() {
            let here = decide(x, y, S, &near_c, &near_o);
            let (o, c) = (Outlines::in_box(x + 3_000.0, y - 2_000.0, 6_000.0, S), Coasts::in_box(x + 3_000.0, y - 2_000.0, 6_000.0, S));
            assert_eq!(here, decide(x, y, S, &c, &o), "the site at ({x:.0}, {y:.0}) decides two ways");
            if here.is_some() { scarps += 1 }
        }
        assert!(scarps > 0, "no scarp on the belt");
    }

    /// Ground near repose, as a forelimb's scree lies, and ground on shale
    /// stand no scarp, wherever a site falls on them.
    #[test]
    fn a_forelimb_or_shale_stands_no_scarp() {
        let (o, c) = (Outlines::in_box(BELT.0, BELT.1, 9_000.0, S), Coasts::in_box(BELT.0, BELT.1, 9_000.0, S));
        let (mut steep, mut shale) = (0, 0);
        for i in -40..40 {
            for j in -40..40 {
                let (x, y) = (BELT.0 + i as f64 * 97.0, BELT.1 + j as f64 * 89.0);
                let envelope = envelope_at(x, y, S, &c, &o).surface;
                let grade = gradient_of(x, y, envelope, |x, y| envelope_at(x, y, S, &c, &o).surface);
                let Some(at) = o.at(x, y) else { continue };
                let bed = Column::of(at.plate.id, S).bed_at(rock_on(x, y, S, &c, &o).exposed).1;
                if grade.0.hypot(grade.1) >= SCARP_GROUND * REPOSE_GRADE {
                    steep += 1;
                    assert_eq!(decide(x, y, S, &c, &o), None, "a scarp on a flank at ({x:.0}, {y:.0})");
                } else if bed == common::Rock::Shale {
                    shale += 1;
                    assert_eq!(decide(x, y, S, &c, &o), None, "a scarp on shale at ({x:.0}, {y:.0})");
                }
            }
        }
        assert!(steep > 0 && shale > 0, "{steep} steep and {shale} shale positions");
    }

    /// Every tile of a scarp's footprint carves what any reader of the
    /// site decides: the stack through the landforms stands each tile where
    /// the surface drainage routes on stands, which a fresh reader of the
    /// landforms computes.
    #[test]
    fn a_footprint_reads_one_decision() {
        use super::super::{lithology, motion, plates, thickening, thrusting, tilt, Composite};
        let mut composite = Composite::new(S);
        composite.add_event(Box::new(plates::PlateEvent::new()));
        composite.add_event(Box::new(tilt::TiltEvent::new()));
        composite.add_event(Box::new(motion::MotionEvent::new()));
        composite.add_event(Box::new(thrusting::ThrustingEvent::new()));
        composite.add_event(Box::new(thickening::ThickeningEvent::new()));
        composite.add_event(Box::new(lithology::LithologyEvent::new()));
        composite.add_event(Box::new(LandformEvent::new()));
        let (o, c) = (Outlines::in_box(BELT.0, BELT.1, 9_000.0, S), Coasts::in_box(BELT.0, BELT.1, 9_000.0, S));
        let scarp = belt_sites().into_iter().find_map(|(x, y)| decide(x, y, S, &c, &o)).expect("a scarp on the belt");
        let mut carved = 0;
        for k in 0..64 {
            let angle = k as f64 * 0.37;
            let reach = FOOTPRINT * (k as f64 / 64.0);
            let (q, r) = world_to_hex(scarp.x + reach * angle.cos(), scarp.y + reach * angle.sin());
            let (wx, wy) = hex_to_world(q, r);
            let fresh = Landforms::new();
            let ground = super::super::drainage::ground_at(wx, wy, S, &c, &o, &fresh).surface;
            let envelope = envelope_at(wx, wy, S, &c, &o).surface;
            assert!((composite.tile_at(q, r).elevation - ground).abs() < 1e-9, "the tile at ({q}, {r}) reads another decision");
            if envelope - ground > 1.0 { carved += 1 }
        }
        assert!(carved > 0, "the footprint carved nothing");
    }
}
