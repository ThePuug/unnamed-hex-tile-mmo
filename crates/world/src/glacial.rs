//! Glacial erosion — cirques.

//! Sits between the tectonic layers (peaks, ridgelines) and the water layer
//! (ravines) in the erosional stack. Ice needs relief before it can rest on
//! anything, and it leaves floors for water to drain: each layer consumes the
//! relief below it and sets the base level the layer above erodes toward.

//! A cirque is a bowl bitten into a peak flank below the crest. Four features
//! come out of the one primitive:
//! - **floor** — flat ground at altitude, the surface players stand on
//! - **headwall** — a steep rise from floor to surrounding terrain across the
//!   back and sides, too steep to climb
//! - **lip** — a low bar across the open side, impounding the basin
//! - **outlet** — the low point of the rim, where the basin overflows and the
//!   stream that drains it begins
//!
//! Carving is subtractive and min-composited against the pristine surface, so
//! overlapping bowls resolve identically under any evaluation order. Overlap is
//! not suppressed: two bowls eating toward each other from opposite flanks
//! leave a knife ridge where neither cuts deep, and a bowl whose floor cuts
//! through a neighbour's rim drains it, the way stacked cirques do.

use std::f64::consts::{FRAC_PI_3, PI, TAU};

use crate::TILE_SPACING;
use crate::lithology::resistance_at;
use crate::noise::hash_f64;
use crate::spine::{ELEVATION_PER_Z, Peak, RIDGE_PEAK_ELEVATION};

// ── Siting constants ────────────────────────────────────────────────────────

/// Altitude at which permanent ice becomes possible, as a fraction of the
/// tallest attainable spine summit. Expressed as a fraction so retuning the
/// elevation scale carries the line with it. A quarter of the maximum leaves
/// the tallest peaks glaciated well down their flanks while excluding modest
/// ones outright, which is what makes altitude a source of variance rather
/// than a switch that is always on or always off.
const GLACIATION_LINE_FRAC: f64 = 0.25;

/// Altitude above sea level at which ice survives the melt season. Cirques form
/// above it; below it terrain stays water-carved.
pub const GLACIATION_LINE: f64 = RIDGE_PEAK_ELEVATION * GLACIATION_LINE_FRAC;

/// Bearing of the shaded flank — one value for the whole world, since a world
/// has one sun. Snow survives the melt season facing it, so cirques cluster
/// there and every peak ends up with a bitten side and a smooth side.
const SHADE_BEARING: f64 = std::f64::consts::FRAC_PI_2;

/// Candidate flanks per peak, evenly spaced around it — one per 30°.
const FLANK_CANDIDATES: usize = 12;

/// Exponent on a flank's alignment with [`SHADE_BEARING`]. Alignment runs 1 on
/// the shaded flank to 0 on the sunlit one, so raising it concentrates bowls
/// into shade; 3 leaves the flanks either side of the shade bearing viable and
/// makes the sunlit flank effectively unreachable.
const ASPECT_EXPONENT: f64 = 3.0;

/// Bowls per suitable peak.
const CIRQUES_PER_PEAK_MIN: usize = 1;
const CIRQUES_PER_PEAK_MAX: usize = 3;

/// Bowl radius as a fraction of the host peak's falloff radius. The bowl is
/// placed one radius out from the summit, so a footprint reaches `2 * radius`
/// down the flank: the maximum stays under half the falloff radius, which is
/// what keeps cirques from adding any reach to a spine's influence extent and
/// leaves a tile's elevation resolving from its chunk plus the 1-ring.
///
/// Peaks on a spine sit closer together than their falloff radii, so a bowl at
/// the top of this range spans the ground between two summits — the rim carries
/// both, as a real cirque headwall carries the summits along the ridge it cuts.
const RADIUS_MIN_FRAC: f64 = 0.20;
const RADIUS_MAX_FRAC: f64 = 0.35;

/// Depth of the floor below the outlet, as a fraction of the bowl radius —
/// the overdeepening a tarn fills. Divided by resistance at the carve site.
const BASIN_DEPTH_MIN_FRAC: f64 = 0.06;
const BASIN_DEPTH_MAX_FRAC: f64 = 0.14;

// ── Shape constants ─────────────────────────────────────────────────────────

/// Fraction of the bowl radius that is flat floor.
const FLOOR_FRAC: f64 = 0.45;

/// Fraction of the bowl radius at which the rim crests. Past it the carve
/// rejoins the surrounding surface, which is what bounds the footprint at
/// exactly `radius` with no step at the boundary.
const CREST_FRAC: f64 = 0.80;

/// Half-angle of the mouth. The rim swings from lip to headwall across it, so
/// the bowl opens over a 120° arc and is walled for the remaining 240°.
const MOUTH_HALF_ANGLE: f64 = FRAC_PI_3;

/// Radial exponent of the headwall before resistance. Below 1 the wall takes
/// most of its rise in the first strides out of the floor, which is what makes
/// it impassable — the climb per tile there is far more than the one z-level a
/// step can take.
const HEADWALL_EXPONENT: f64 = 0.35;

/// Radial exponent of the lip. At 1 the bar climbs linearly out of the floor
/// over the basin depth alone, which makes the mouth the gentlest line into a
/// bowl by a wide margin — gentler than the flank around it, where the headwall
/// is far steeper than either.
const LIP_EXPONENT: f64 = 1.0;

/// Amplitude of the two rim harmonics, as fractions of the bounding radius.
/// Three and five lobes are coprime, so the outline does not close on itself
/// within a turn, and both are low enough in order to read as bays cut into
/// the rim rather than as roughness along it.
const LOBE_AMP_3: f64 = 0.09;
const LOBE_AMP_5: f64 = 0.05;

/// Combined harmonic swing. The rim runs between `radius * (1 - 2 * LOBE_SPAN)`
/// and exactly `radius`, so `radius` stays the outer bound every reach
/// guarantee is stated against.
const LOBE_SPAN: f64 = LOBE_AMP_3 + LOBE_AMP_5;

/// How far past the rim the ground below an outlet is read, as a fraction of
/// the bowl radius, to decide what leaves it.
const OUTFLOW_PROBE_FRAC: f64 = 0.5;

/// Rise a step can take per world unit of ground covered. Neighbouring tiles
/// sit [`TILE_SPACING`] apart and a step climbs at most one z-level, so ground
/// steeper than this cannot be walked either way.
const WALKABLE_SLOPE: f64 = ELEVATION_PER_Z / TILE_SPACING;

/// Arc spacing between rim samples in world units, and a floor on the count for
/// small bowls. The outlet is the lowest sample, so this bounds how far the
/// true low point of the rim can sit from the one chosen — and with it, how far
/// the ridge noise can carry the rim between samples below the altitude the
/// floor was set from. Held well under the shallowest basin depth, which is
/// what keeps that gap from breaching a bowl.
const RIM_SAMPLE_SPACING: f64 = 10.0;
const RIM_SAMPLES_MIN: usize = 24;

// ── Noise seeds ─────────────────────────────────────────────────────────────

const SEED_CIRQUE_COUNT: u64 = 0xAAAA_BBBB_0040;
const SEED_CIRQUE_FLANK: u64 = 0xAAAA_BBBB_0041;
const SEED_CIRQUE_JITTER: u64 = 0xAAAA_BBBB_0042;
const SEED_CIRQUE_SIZE: u64 = 0xAAAA_BBBB_0043;
const SEED_CIRQUE_DEPTH: u64 = 0xAAAA_BBBB_0044;
const SEED_CIRQUE_LOBE:  u64 = 0xAAAA_BBBB_0045;

// ── Cirque ──────────────────────────────────────────────────────────────────

/// What leaves a bowl at its outlet. Read off the ground below the rim when the
/// bowl is sited, so it follows the mountain the bowl sits on rather than a
/// separate roll of the dice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outflow {
    /// Nothing leaves. The ground below the rim falls by less than the basin is
    /// deep, so the bowl impounds more relief than it can shed and no channel
    /// establishes below it: a closed tarn.
    Impounded,
    /// The ground below the rim falls faster than a step can walk down, so the
    /// water leaves the sill over a wall and the channel begins below it.
    Fall,
    /// Graded descent. The outflow cuts a valley away from the sill.
    Ravine,
}

/// Which part of a bowl a point falls in. Diagnostic classification by
/// footprint geometry — it describes what the bowl imposes there, not what
/// survived min-compositing with the surrounding terrain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CirqueProbe {
    /// Flat ground at floor altitude.
    Floor,
    /// The steep back-and-sides wall.
    Headwall,
    /// The low bar across the open side.
    Lip,
    /// The spill point: the low point of the lip.
    Outlet,
}

/// A glacial bowl on a peak flank.
#[derive(Clone)]
pub struct Cirque {
    /// Bowl centre.
    pub cx: f64,
    pub cy: f64,
    /// Footprint radius. The carve is the identity at and beyond it.
    pub radius: f64,
    /// Flat floor altitude — the surface players stand on, and the local base
    /// level for anything draining into the bowl.
    pub floor: f64,
    /// Altitude of the rim's low point: what the basin fills to before it
    /// overflows.
    pub outlet_elev: f64,
    /// Bearing from the centre to the outlet.
    pub outlet_bearing: f64,
    /// Spill point, on the rim crest along `outlet_bearing`.
    pub outlet_wx: f64,
    pub outlet_wy: f64,
    /// Radial exponent of this bowl's headwall after resistance.
    pub headwall_exponent: f64,
    /// What the basin sheds at its outlet.
    pub outflow: Outflow,
    /// Phases of the two rim harmonics that make the footprint irregular.
    lobe_phase_3: f64,
    lobe_phase_5: f64,
}

/// Cubic smoothstep on [0, 1], saturating outside it.
fn smoothstep(x: f64) -> f64 {
    let t = x.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Absolute angle between two bearings, in [0, π].
fn angle_between(a: f64, b: f64) -> f64 {
    let mut d = (a - b) % TAU;
    if d > PI { d -= TAU; }
    if d < -PI { d += TAU; }
    d.abs()
}

impl Cirque {
    /// Rim distance from the centre along `theta`. Two low harmonics pull the
    /// outline into bays and headlands; the swing is subtracted before it is
    /// added back, so this never exceeds [`Cirque::radius`] and every bound
    /// stated against that radius still holds.
    pub fn radius_at(&self, theta: f64) -> f64 {
        self.radius
            * (1.0 - LOBE_SPAN
                + LOBE_AMP_3 * (theta * 3.0 + self.lobe_phase_3).cos()
                + LOBE_AMP_5 * (theta * 5.0 + self.lobe_phase_5).cos())
    }

    /// Surface this bowl imposes at (wx, wy), given the surface it bites into.
    /// Never above `base`, so `min` across every overlapping bowl is
    /// order-independent, and exactly `base` at and beyond the footprint edge.
    pub fn carve(&self, wx: f64, wy: f64, base: f64) -> f64 {
        let dx = wx - self.cx;
        let dy = wy - self.cy;
        let dist_sq = dx * dx + dy * dy;
        if dist_sq >= self.radius * self.radius { return base; }
        // Ground already below the floor has nothing left for ice to excavate,
        // and cutting toward the floor from below would raise it.
        if base <= self.floor { return base; }

        let theta = dy.atan2(dx);
        let rim = self.radius_at(theta);
        if dist_sq >= rim * rim { return base; }

        let u = dist_sq.sqrt() / rim;

        // Rim altitude and wall exponent both swing from lip to headwall across
        // the mouth: 0 on the outlet bearing, 1 once clear of the mouth arc.
        let mouth = smoothstep(angle_between(theta, self.outlet_bearing) / MOUTH_HALF_ANGLE);
        let crest = (self.outlet_elev + (base - self.outlet_elev) * mouth).min(base);

        if u <= CREST_FRAC {
            let t = ((u - FLOOR_FRAC) / (CREST_FRAC - FLOOR_FRAC)).clamp(0.0, 1.0);
            let exponent = LIP_EXPONENT + (self.headwall_exponent - LIP_EXPONENT) * mouth;
            self.floor + (crest - self.floor) * t.powf(exponent)
        } else {
            let t = (u - CREST_FRAC) / (1.0 - CREST_FRAC);
            crest + (base - crest) * t
        }
    }

    /// Whether (wx, wy) stands on the flat floor — the region whose local base
    /// level is [`Cirque::floor`].
    pub fn floor_contains(&self, wx: f64, wy: f64) -> bool {
        let dx = wx - self.cx;
        let dy = wy - self.cy;
        let dist_sq = dx * dx + dy * dy;
        if dist_sq >= self.radius * self.radius { return false; }
        let floor_radius = self.radius_at(dy.atan2(dx)) * FLOOR_FRAC;
        dist_sq <= floor_radius * floor_radius
    }

    /// The level anything cutting here can descend to, or `None` outside the
    /// footprint: the floor inside the basin, the spill altitude across the rim
    /// that impounds it. The rim clause is what keeps the basin closed — a
    /// stream free to incise the rim below its outlet drains the tarn, and
    /// lowering an outlet is a process the water layer does not model.
    pub fn base_level(&self, wx: f64, wy: f64) -> Option<f64> {
        let dx = wx - self.cx;
        let dy = wy - self.cy;
        let dist_sq = dx * dx + dy * dy;
        if dist_sq >= self.radius * self.radius { return None; }
        let rim = self.radius_at(dy.atan2(dx));
        if dist_sq >= rim * rim { return None; }
        let floor_radius = rim * FLOOR_FRAC;
        Some(if dist_sq <= floor_radius * floor_radius { self.floor } else { self.outlet_elev })
    }

    /// Classify (wx, wy) within the footprint, or `None` outside it.
    pub fn probe(&self, wx: f64, wy: f64) -> Option<CirqueProbe> {
        let dx = wx - self.cx;
        let dy = wy - self.cy;
        let dist_sq = dx * dx + dy * dy;
        if dist_sq >= self.radius * self.radius { return None; }
        let theta = dy.atan2(dx);
        let rim = self.radius_at(theta);
        if dist_sq >= rim * rim { return None; }

        let dist = dist_sq.sqrt();
        if dist <= rim * FLOOR_FRAC { return Some(CirqueProbe::Floor); }

        // The outlet marker covers a rim-sample's worth of arc, so it reads at
        // the resolution the outlet was chosen at.
        let odx = wx - self.outlet_wx;
        let ody = wy - self.outlet_wy;
        if odx * odx + ody * ody <= RIM_SAMPLE_SPACING * RIM_SAMPLE_SPACING {
            return Some(CirqueProbe::Outlet);
        }

        if angle_between(theta, self.outlet_bearing) >= MOUTH_HALF_ANGLE {
            Some(CirqueProbe::Headwall)
        } else {
            Some(CirqueProbe::Lip)
        }
    }
}

// ── Composite carve ─────────────────────────────────────────────────────────

/// Min-composite every bowl's target surface against `base`. Order-independent:
/// each bowl is evaluated against the pristine surface, never against another
/// bowl's result.
pub fn carve_all(cirques: &[Cirque], wx: f64, wy: f64, base: f64) -> f64 {
    let mut elevation = base;
    for c in cirques {
        let carved = c.carve(wx, wy, base);
        if carved < elevation { elevation = carved; }
    }
    elevation
}

/// The glacial surface, and the floor water may cut to on it. Both answers come
/// out of the same footprint tests, so the elevation path pays for one pass.
///
/// The floor is [`base_level_at`] held under the glacial surface: no channel may
/// incise a rim below the level that impounds the basin behind it, and none may
/// raise ground the ice already took. Applied here rather than only at each
/// stream step, because a channel is carved across its whole width — a stream
/// running past a bowl reaches into it without ever stepping inside.
pub fn carve_and_limit(cirques: &[Cirque], wx: f64, wy: f64, base: f64) -> (f64, f64) {
    let mut elevation = base;
    let mut impound = f64::MAX;
    for c in cirques {
        let carved = c.carve(wx, wy, base);
        if carved < elevation { elevation = carved; }
        if let Some(l) = c.base_level(wx, wy) {
            if l < impound { impound = l; }
        }
    }
    // No bowl here — the sea is the floor, as it is everywhere else.
    if impound == f64::MAX { impound = 0.0; }
    (elevation, impound.min(elevation))
}

/// The local base level at (wx, wy): the level the bowls standing here drain
/// to, or sea level where none do. Overlapping bowls resolve to the lowest,
/// which is the ground the composite actually shows — a bowl whose floor cuts
/// through a neighbour's rim drains it, the way stacked cirques really do.
pub fn base_level_at(cirques: &[Cirque], wx: f64, wy: f64) -> f64 {
    impound_at(cirques, wx, wy).unwrap_or(0.0)
}

/// The impounding level at (wx, wy), or `None` where no bowl claims the point.
///
/// Distinct from [`base_level_at`], which answers sea level there. A caller
/// composing the clamp across several spines has to tell "no bowl here" from
/// "the sea", or one instance without a bowl cancels every other instance's
/// clamp and every basin in the overlap opens.
pub fn impound_at(cirques: &[Cirque], wx: f64, wy: f64) -> Option<f64> {
    let mut level = f64::MAX;
    for c in cirques {
        if let Some(l) = c.base_level(wx, wy) {
            if l < level { level = l; }
        }
    }
    (level != f64::MAX).then_some(level)
}

/// The level of standing water at (wx, wy): a tarn on a bowl floor, or the sea.
/// Where a stream runs into this it has arrived and stops.
///
/// Distinct from [`base_level_at`], which also impounds across a rim. A rim is
/// something a stream crosses on its way out of a basin, not somewhere it ends;
/// conflating the two leaves every outlet stream dead at its own sill.
pub fn pool_level_at(cirques: &[Cirque], wx: f64, wy: f64) -> f64 {
    let mut level = 0.0f64;
    for c in cirques {
        if c.floor_contains(wx, wy) && c.floor > level { level = c.floor; }
    }
    level
}

/// Publish the faces these bowls leave standing.
///
/// A headwall's foot is the floor edge, and it stands as far above the ground
/// as the ground it was cut into. Both are read off the same surface the rim
/// was fitted to, so the face a consumer sees is the face the query path
/// produces. Sampled by bearing, at the spacing the rim itself was fitted at,
/// because a rim varies with bearing and one face per bowl would place the
/// whole wall at its centre.
pub fn publish_faces(
    cirques: &[Cirque],
    surface: &dyn Fn(f64, f64) -> f64,
    out: &mut crate::faces::FaceIndex,
    min_height: f64,
) {
    for c in cirques {
        let samples = RIM_SAMPLES_MIN.max((TAU * c.radius / RIM_SAMPLE_SPACING) as usize);
        for s in 0..samples {
            let theta = TAU * s as f64 / samples as f64;
            let rim = c.radius_at(theta);
            let (dx, dy) = (theta.cos(), theta.sin());
            // The foot sits at the floor's edge; the wall rises from there to
            // the surrounding ground the bowl was bitten out of.
            let foot = rim * FLOOR_FRAC;
            let (fx, fy) = (c.cx + dx * foot, c.cy + dy * foot);
            let floor = surface(fx, fy);
            let above = surface(c.cx + dx * rim, c.cy + dy * rim);
            out.insert(
                crate::faces::ErosionalFace { wx: fx, wy: fy, floor, height: above - floor },
                min_height,
            );
        }
    }
}

/// Whichever bowl claims (wx, wy), preferring the floor over walls so an
/// overlap reads as the ground a player would stand on.
pub fn probe_all(cirques: &[Cirque], wx: f64, wy: f64) -> Option<CirqueProbe> {
    let mut best: Option<CirqueProbe> = None;
    for c in cirques {
        match c.probe(wx, wy) {
            Some(CirqueProbe::Outlet) => return Some(CirqueProbe::Outlet),
            Some(CirqueProbe::Floor) => best = Some(CirqueProbe::Floor),
            Some(other) if best.is_none() => best = Some(other),
            _ => {}
        }
    }
    best
}

// ── Siting ──────────────────────────────────────────────────────────────────

/// Flank bearings to bite, weighted by aspect and drawn without replacement.
/// Weighting is multiplicative in the terms that make a flank suitable, so a
/// material term multiplies in alongside aspect when lithology lands.
fn select_flanks(count: usize, peak_idx: usize, spine_id: u64, seed: u64) -> Vec<f64> {
    let mut weights: Vec<f64> = (0..FLANK_CANDIDATES)
        .map(|k| {
            let bearing = TAU * k as f64 / FLANK_CANDIDATES as f64;
            // 1 facing the shade bearing, 0 facing away.
            let alignment = (1.0 + (bearing - SHADE_BEARING).cos()) * 0.5;
            alignment.powf(ASPECT_EXPONENT)
        })
        .collect();

    let sector = TAU / FLANK_CANDIDATES as f64;
    let mut bearings = Vec::with_capacity(count);
    for i in 0..count {
        let total: f64 = weights.iter().sum();
        if total <= 0.0 { break; }

        let mut draw = hash_f64(peak_idx as i64, i as i64, seed ^ SEED_CIRQUE_FLANK ^ spine_id) * total;
        let mut k = 0;
        while k + 1 < weights.len() && draw >= weights[k] {
            draw -= weights[k];
            k += 1;
        }
        weights[k] = 0.0;

        let jitter = hash_f64(peak_idx as i64, i as i64, seed ^ SEED_CIRQUE_JITTER ^ spine_id) * 2.0 - 1.0;
        bearings.push(TAU * k as f64 / FLANK_CANDIDATES as f64 + jitter * sector * 0.5);
    }
    bearings
}

/// Bite bowls into the flanks of every peak whose summit clears the glaciation
/// line. `surface` supplies the elevation this layer erodes — the tectonic
/// surface with its noise applied, so the rim the bowl is fitted to is the rim
/// the query path produces.
pub fn site_cirques(
    peaks: &[Peak],
    spine_id: u64,
    seed: u64,
    surface: &dyn Fn(f64, f64) -> f64,
) -> Vec<Cirque> {
    let mut cirques = Vec::new();

    for (pi, peak) in peaks.iter().enumerate() {
        // Ice has to survive on the summit before it can bite a flank below it.
        if peak.height < GLACIATION_LINE { continue; }

        let count_draw = hash_f64(pi as i64, 0, seed ^ SEED_CIRQUE_COUNT ^ spine_id);
        let span = (CIRQUES_PER_PEAK_MAX - CIRQUES_PER_PEAK_MIN + 1) as f64;
        let count = CIRQUES_PER_PEAK_MIN + (count_draw * span) as usize;

        for (ci, bearing) in select_flanks(count, pi, spine_id, seed).into_iter().enumerate() {
            let size_draw = hash_f64(pi as i64, ci as i64, seed ^ SEED_CIRQUE_SIZE ^ spine_id);
            let radius = peak.falloff_radius
                * (RADIUS_MIN_FRAC + size_draw * (RADIUS_MAX_FRAC - RADIUS_MIN_FRAC));
            // One radius out, so the rim passes through the summit and the bowl
            // bites the flank immediately below it. The carve rejoins the base
            // surface at the rim, so the summit itself survives as a horn while
            // the ground under it is scooped away — and bowls on several
            // bearings, all backed onto the same summit, leave arêtes between
            // them rather than a smooth cone with dimples in its side.
            let offset = radius;
            let cx = peak.wx + bearing.cos() * offset;
            let cy = peak.wy + bearing.sin() * offset;

            let lobe_phase_3 =
                hash_f64(pi as i64, ci as i64, seed ^ SEED_CIRQUE_LOBE ^ spine_id) * TAU;
            let lobe_phase_5 =
                hash_f64(ci as i64, pi as i64, seed ^ SEED_CIRQUE_LOBE ^ spine_id) * TAU;
            let rim_at = |theta: f64| {
                radius
                    * (1.0 - LOBE_SPAN
                        + LOBE_AMP_3 * (theta * 3.0 + lobe_phase_3).cos()
                        + LOBE_AMP_5 * (theta * 5.0 + lobe_phase_5).cos())
            };

            // The lowest point of the rim is where the basin overflows, so the
            // whole bowl is fitted to it: floor beneath it, mouth opening
            // toward it, stream leaving from it.
            let samples = RIM_SAMPLES_MIN.max((TAU * radius / RIM_SAMPLE_SPACING) as usize);
            let mut outlet_bearing = 0.0;
            let mut outlet_elev = f64::MAX;
            for s in 0..samples {
                let theta = TAU * s as f64 / samples as f64;
                let r = rim_at(theta);
                let e = surface(cx + theta.cos() * r, cy + theta.sin() * r);
                if e < outlet_elev {
                    outlet_elev = e;
                    outlet_bearing = theta;
                }
            }

            let resistance = resistance_at(cx, cy);
            let depth_draw = hash_f64(pi as i64, ci as i64, seed ^ SEED_CIRQUE_DEPTH ^ spine_id);
            let basin_depth = radius
                * (BASIN_DEPTH_MIN_FRAC + depth_draw * (BASIN_DEPTH_MAX_FRAC - BASIN_DEPTH_MIN_FRAC))
                / resistance;
            let floor = outlet_elev - basin_depth;

            // The glaciation line gates accumulation, which happens on the
            // summit; excavation happens wherever the ice then flows, well
            // below it. What a floor must clear is the sea: a tarn's surface
            // stands `basin_depth` above its floor, so a floor any lower than
            // that impounds nothing distinguishable from open water.
            if floor < basin_depth { continue; }

            let outlet_rim = rim_at(outlet_bearing);
            let outlet_wx = cx + outlet_bearing.cos() * outlet_rim * CREST_FRAC;
            let outlet_wy = cy + outlet_bearing.sin() * outlet_rim * CREST_FRAC;

            // What the basin sheds is a property of the ground below it, so it
            // is read from that ground: the run reaches from the sill past the
            // footprint onto flank the bowl does not touch.
            let run = outlet_rim * (1.0 - CREST_FRAC) + radius * OUTFLOW_PROBE_FRAC;
            let drop = outlet_elev
                - surface(
                    outlet_wx + outlet_bearing.cos() * run,
                    outlet_wy + outlet_bearing.sin() * run,
                );
            let outflow = if drop < basin_depth {
                Outflow::Impounded
            } else if drop / run >= WALKABLE_SLOPE {
                Outflow::Fall
            } else {
                Outflow::Ravine
            };

            cirques.push(Cirque {
                cx, cy, radius, floor, outlet_elev, outlet_bearing,
                outlet_wx, outlet_wy,
                headwall_exponent: HEADWALL_EXPONENT / resistance,
                outflow, lobe_phase_3, lobe_phase_5,
            });
        }
    }

    cirques
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: u64 = 0x9E3779B97F4A7C15;

    /// A cone tall enough to glaciate, on flat ground.
    fn test_peaks() -> Vec<Peak> {
        vec![Peak { wx: 0.0, wy: 0.0, height: RIDGE_PEAK_ELEVATION, falloff_radius: 2000.0 }]
    }

    /// Elevation of a lone cone with the spine's power-curve falloff.
    fn cone_surface(peaks: &[Peak]) -> impl Fn(f64, f64) -> f64 + '_ {
        move |wx, wy| {
            peaks.iter().fold(0.0f64, |acc, p| {
                let d = ((wx - p.wx).powi(2) + (wy - p.wy).powi(2)).sqrt();
                if d >= p.falloff_radius { return acc; }
                acc.max(crate::spine::cross_section_profile(d / p.falloff_radius) * p.height)
            })
        }
    }

    fn one_cirque() -> Cirque {
        let peaks = test_peaks();
        let cirques = site_cirques(&peaks, 42, SEED, &cone_surface(&peaks));
        assert!(!cirques.is_empty(), "a full-height cone should host cirques");
        cirques.into_iter().next().unwrap()
    }

    /// Sample the footprint on a polar grid, skipping the exact centre. Rings
    /// follow the rim, which varies with bearing.
    fn footprint_samples(c: &Cirque, rings: u32, spokes: u32) -> Vec<(f64, f64)> {
        let mut out = Vec::new();
        for j in 0..spokes {
            let theta = TAU * j as f64 / spokes as f64;
            let rim = c.radius_at(theta);
            for i in 1..=rings {
                let d = rim * i as f64 / rings as f64;
                out.push((c.cx + theta.cos() * d, c.cy + theta.sin() * d));
            }
        }
        out
    }

    #[test]
    fn carve_never_raises_terrain() {
        let c = one_cirque();
        let surface = c.outlet_elev + c.radius;
        for (wx, wy) in footprint_samples(&c, 12, 24) {
            let carved = c.carve(wx, wy, surface);
            assert!(
                carved <= surface + 1e-9,
                "carve raised terrain at ({wx:.1}, {wy:.1}): {carved:.3} > {surface:.3}"
            );
        }
    }

    /// The footprint has to be bounded, or a tile's elevation stops resolving
    /// from its chunk plus the 1-ring.
    #[test]
    fn carve_is_identity_outside_the_footprint() {
        let c = one_cirque();
        let surface = c.outlet_elev + c.radius;
        for j in 0..36 {
            let theta = TAU * j as f64 / 36.0;
            for &scale in &[1.0, 1.05, 2.0] {
                let d = c.radius * scale;
                let (wx, wy) = (c.cx + theta.cos() * d, c.cy + theta.sin() * d);
                assert_eq!(
                    c.carve(wx, wy, surface), surface,
                    "carve reached outside the footprint at {scale}x radius"
                );
            }
        }
    }

    #[test]
    fn floor_is_flat_at_floor_altitude() {
        let c = one_cirque();
        let surface = c.outlet_elev + c.radius;
        for j in 0..24 {
            let theta = TAU * j as f64 / 24.0;
            let d = c.radius_at(theta) * FLOOR_FRAC * 0.99;
            let carved = c.carve(c.cx + theta.cos() * d, c.cy + theta.sin() * d, surface);
            assert!(
                (carved - c.floor).abs() < 1e-9,
                "floor is not flat: {carved:.6} != {:.6}", c.floor
            );
        }
    }

    /// The basin holds water unless a deeper bowl has eaten through its rim,
    /// which drains the pair into the lower floor — stacked cirques, not a
    /// defect. Overlap that produces them is deliberate, so the invariant is
    /// that nothing else opens a rim.
    #[test]
    fn the_basin_is_closed_by_its_rim() {
        let peaks = test_peaks();
        let surface = cone_surface(&peaks);
        let cirques = site_cirques(&peaks, 42, SEED, &surface);
        assert!(!cirques.is_empty());

        for c in &cirques {
            for s in 0..720 {
                let theta = TAU * s as f64 / 720.0;
                let rim_r = c.radius_at(theta);
                let (wx, wy) = (c.cx + theta.cos() * rim_r, c.cy + theta.sin() * rim_r);
                let rim = carve_all(&cirques, wx, wy, surface(wx, wy));
                if rim > c.floor { continue; }
                let cascade = cirques.iter().any(|o| {
                    !std::ptr::eq(o, c) && o.floor < c.floor && o.base_level(wx, wy).is_some()
                });
                assert!(
                    cascade,
                    "rim at {:.0}° sits at {rim:.2}, at or below floor {:.2}, with no \
                     deeper bowl to drain into",
                    theta.to_degrees(), c.floor
                );
            }
        }
    }

    /// The outlet is the low point of the rim: that is where the basin
    /// overflows, and the stream that drains it starts there. Rim sampling is
    /// discrete, so the claim is that the outlet sits within sampling
    /// resolution of the true low point, measured against the rim's own relief.
    #[test]
    fn the_outlet_is_the_low_point_of_the_rim() {
        let peaks = test_peaks();
        let surface = cone_surface(&peaks);
        let c = one_cirque();

        let outlet_r = c.radius_at(c.outlet_bearing);
        let outlet_rim = surface(
            c.cx + c.outlet_bearing.cos() * outlet_r,
            c.cy + c.outlet_bearing.sin() * outlet_r,
        );
        let (mut lowest, mut highest) = (f64::MAX, f64::MIN);
        for s in 0..720 {
            let theta = TAU * s as f64 / 720.0;
            let r = c.radius_at(theta);
            let rim = surface(c.cx + theta.cos() * r, c.cy + theta.sin() * r);
            lowest = lowest.min(rim);
            highest = highest.max(rim);
        }
        assert!(highest > lowest, "a rim with no relief has no low point to find");
        assert!(
            outlet_rim - lowest < (highest - lowest) * 0.02,
            "outlet ({outlet_rim:.2}) is not at the rim low point ({lowest:.2}) of a rim \
             spanning {:.2}", highest - lowest
        );
    }

    /// Headwall against lip: the back wall climbs out of the floor far faster
    /// than the mouth does, which is what makes one impassable and the other a
    /// rim you cross on entry.
    #[test]
    fn the_headwall_is_steeper_than_the_lip() {
        let c = one_cirque();
        let surface = c.outlet_elev + c.radius;
        let sample = |bearing: f64, u: f64| {
            let d = c.radius_at(bearing) * u;
            c.carve(c.cx + bearing.cos() * d, c.cy + bearing.sin() * d, surface)
        };

        let back = c.outlet_bearing + PI;
        let just_out = FLOOR_FRAC + (CREST_FRAC - FLOOR_FRAC) * 0.1;
        let headwall_rise = sample(back, just_out) - c.floor;
        let lip_rise = sample(c.outlet_bearing, just_out) - c.floor;
        assert!(
            headwall_rise > lip_rise,
            "headwall rise {headwall_rise:.2} should exceed lip rise {lip_rise:.2}"
        );
    }

    /// Both walls rise away from the floor — no re-entrant pockets on the way
    /// out of the bowl in any direction.
    #[test]
    fn the_bowl_rises_monotonically_from_the_floor() {
        let c = one_cirque();
        let surface = c.outlet_elev + c.radius;
        for j in 0..36 {
            let theta = TAU * j as f64 / 36.0;
            let mut prev = f64::MIN;
            for i in 0..=40 {
                let d = c.radius_at(theta) * i as f64 / 40.0;
                let e = c.carve(c.cx + theta.cos() * d, c.cy + theta.sin() * d, surface);
                assert!(
                    e >= prev - 1e-9,
                    "bowl dips outward at {:.0}°, u={:.2}: {e:.3} < {prev:.3}",
                    theta.to_degrees(), i as f64 / 40.0
                );
                prev = e;
            }
        }
    }

    /// Two bowls biting toward each other, footprints overlapping and floors at
    /// different altitudes — the case where evaluation order could matter.
    fn twin_bowls() -> Vec<Cirque> {
        let bowl = |cx: f64, floor: f64, bearing: f64| Cirque {
            cx, cy: 0.0, radius: 300.0, floor,
            outlet_elev: floor + 40.0,
            outlet_bearing: bearing,
            outlet_wx: cx + bearing.cos() * 300.0 * CREST_FRAC,
            outlet_wy: bearing.sin() * 300.0 * CREST_FRAC,
            headwall_exponent: HEADWALL_EXPONENT,
            outflow: Outflow::Ravine,
            lobe_phase_3: 0.0,
            lobe_phase_5: 0.0,
        };
        vec![bowl(-200.0, 1500.0, PI), bowl(200.0, 1400.0, 0.0)]
    }

    /// Order-independence is an invariant: overlapping bowls must composite the
    /// same however the list is ordered.
    #[test]
    fn overlapping_bowls_composite_order_independently() {
        let mut cirques = twin_bowls();
        let base = 2200.0;

        let probes: Vec<(f64, f64)> = footprint_samples(&cirques[0], 10, 24)
            .into_iter()
            .chain(footprint_samples(&cirques[1], 10, 24))
            .collect();
        let forward: Vec<f64> =
            probes.iter().map(|&(wx, wy)| carve_all(&cirques, wx, wy, base)).collect();

        cirques.reverse();
        for (i, &(wx, wy)) in probes.iter().enumerate() {
            let reversed = carve_all(&cirques, wx, wy, base);
            assert_eq!(forward[i], reversed, "compositing depended on order at ({wx:.1}, {wy:.1})");
        }
    }

    /// Bowls eating toward each other from opposite sides must be able to leave
    /// ground standing between them rather than merging into one hollow.
    #[test]
    fn facing_bowls_leave_a_ridge_between_them() {
        let cirques = twin_bowls();
        let base = 2200.0;
        let between = carve_all(&cirques, 0.0, 0.0, base);
        let in_floor = carve_all(&cirques, -200.0, 0.0, base);
        assert!(
            between > in_floor,
            "ground between the bowls ({between:.1}) should stand above a floor ({in_floor:.1})"
        );
    }

    /// Every footprint stays inside its host cone. Bowls that reached past it
    /// would extend a spine's influence beyond `SPINE_INFLUENCE`, and a tile's
    /// elevation would stop resolving from its chunk plus the 1-ring.
    #[test]
    fn footprints_stay_inside_their_host_cone() {
        let peaks: Vec<Peak> = (0..32)
            .map(|i| Peak {
                wx: i as f64 * 10_000.0,
                wy: 0.0,
                height: RIDGE_PEAK_ELEVATION,
                falloff_radius: 1000.0 + i as f64 * 100.0,
            })
            .collect();
        let cirques = site_cirques(&peaks, 3, SEED, &cone_surface(&peaks));
        assert!(!cirques.is_empty());

        for c in &cirques {
            let contained = peaks.iter().any(|p| {
                let d = ((c.cx - p.wx).powi(2) + (c.cy - p.wy).powi(2)).sqrt();
                d + c.radius <= p.falloff_radius + 1e-9
            });
            assert!(
                contained,
                "bowl at ({:.0}, {:.0}) r={:.0} reaches outside every peak's cone",
                c.cx, c.cy, c.radius
            );
        }
    }

    /// The rim is irregular but bounded: `radius` stays the outer limit every
    /// reach guarantee is stated against, and the outline actually varies —
    /// a rim that never moved would be the circle this replaced.
    #[test]
    fn the_rim_is_irregular_but_bounded_by_the_radius() {
        let peaks = test_peaks();
        let cirques = site_cirques(&peaks, 42, SEED, &cone_surface(&peaks));
        assert!(!cirques.is_empty());

        for c in &cirques {
            let (mut lo, mut hi) = (f64::MAX, f64::MIN);
            for s in 0..360 {
                let r = c.radius_at(TAU * s as f64 / 360.0);
                assert!(
                    r <= c.radius + 1e-9,
                    "rim {r:.2} reaches past the bounding radius {:.2}", c.radius
                );
                assert!(r > 0.0, "rim collapsed to {r:.2}");
                lo = lo.min(r);
                hi = hi.max(r);
            }
            assert!(
                hi - lo > c.radius * 0.05,
                "rim spans only {:.2} of a {:.2} radius — still a circle", hi - lo, c.radius
            );
        }
    }

    /// What a bowl sheds follows the ground below it, so a bowl standing over a
    /// drop sheds and one standing over flat ground does not.
    #[test]
    fn outflow_follows_the_ground_below_the_rim() {
        let peaks = test_peaks();

        let steep = site_cirques(&peaks, 42, SEED, &cone_surface(&peaks));
        assert!(!steep.is_empty());
        assert!(
            steep.iter().any(|c| c.outflow != Outflow::Impounded),
            "no bowl on a cone sheds anything"
        );

        // The same peaks, but the ground below every rim is level: nothing has
        // anywhere to drain to, so nothing does.
        let shelf = site_cirques(&peaks, 42, SEED, &|wx: f64, wy: f64| {
            cone_surface(&peaks)(wx, wy).max(RIDGE_PEAK_ELEVATION * 0.5)
        });
        for c in &shelf {
            assert_eq!(
                c.outflow, Outflow::Impounded,
                "a bowl over level ground still sheds {:?}", c.outflow
            );
        }
    }

    /// The limit the water layer is held to never rises above the ice surface,
    /// or the clamp would fill in ground the ice took.
    #[test]
    fn the_impounding_limit_never_exceeds_the_glacial_surface() {
        let cirques = twin_bowls();
        let base = 2200.0;
        for (wx, wy) in footprint_samples(&cirques[0], 8, 24)
            .into_iter()
            .chain(footprint_samples(&cirques[1], 8, 24))
        {
            let (surface, limit) = carve_and_limit(&cirques, wx, wy, base);
            assert_eq!(surface, carve_all(&cirques, wx, wy, base));
            assert!(
                limit <= surface + 1e-9,
                "limit {limit:.3} stands above the glacial surface {surface:.3}"
            );
        }
    }

    #[test]
    fn siting_is_deterministic() {
        let peaks = test_peaks();
        let a = site_cirques(&peaks, 42, SEED, &cone_surface(&peaks));
        let b = site_cirques(&peaks, 42, SEED, &cone_surface(&peaks));
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!((x.cx, x.cy, x.radius, x.floor), (y.cx, y.cy, y.radius, y.floor));
        }
    }

    /// Aspect is the asymmetry that stops peaks being radially symmetric: bowls
    /// have to land on the shaded flank far more often than the sunlit one.
    #[test]
    fn siting_prefers_the_shaded_flank() {
        const SPACING: f64 = 10_000.0;
        let peaks: Vec<Peak> = (0..64)
            .map(|i| Peak {
                wx: i as f64 * SPACING,
                wy: 0.0,
                height: RIDGE_PEAK_ELEVATION,
                falloff_radius: 2000.0,
            })
            .collect();
        let cirques = site_cirques(&peaks, 7, SEED, &cone_surface(&peaks));
        assert!(!cirques.is_empty());

        let (mut shaded, mut sunlit) = (0, 0);
        for c in &cirques {
            // Peaks are spaced far wider than any footprint, so the host is the
            // nearest lattice position.
            let host_wx = (c.cx / SPACING).round() * SPACING;
            let bearing = c.cy.atan2(c.cx - host_wx);
            if angle_between(bearing, SHADE_BEARING) < std::f64::consts::FRAC_PI_2 {
                shaded += 1;
            } else {
                sunlit += 1;
            }
        }
        assert!(
            shaded > sunlit * 3,
            "aspect preference too weak: {shaded} shaded vs {sunlit} sunlit"
        );
    }

    /// The altitude gate is the source of vertical variance — bowls high, and
    /// nothing at all below the line.
    #[test]
    fn no_bowls_below_the_glaciation_line() {
        let peaks = test_peaks();
        for c in site_cirques(&peaks, 42, SEED, &cone_surface(&peaks)) {
            // Accumulation gates on the summit, so what a floor owes is the
            // sea: it holds a tarn only if the water above it stands clear.
            assert!(
                c.floor >= c.outlet_elev - c.floor,
                "floor {:.1} impounds {:.1} — the tarn surface reaches the sea",
                c.floor, c.outlet_elev - c.floor
            );
        }

        let low: Vec<Peak> = vec![Peak {
            wx: 0.0, wy: 0.0,
            height: GLACIATION_LINE * 0.9,
            falloff_radius: 2000.0,
        }];
        assert!(
            site_cirques(&low, 42, SEED, &cone_surface(&low)).is_empty(),
            "a summit below the line should host no cirques"
        );
    }
}
