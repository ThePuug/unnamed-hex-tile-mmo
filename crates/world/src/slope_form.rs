//! Slope form — the shape a hillslope settles into.

//! The finishing stage of the erosional stack, applied to the surface the
//! tectonic, glacial and water layers leave behind. It sites nothing and adds
//! no features: it gives every slope already in the world the profile the
//! processes acting on one converge to.

//! Three sub-primitives:
//! - **creep** — soil moves downslope in proportion to gradient, which over a
//!   fixed interval is a bounded convolution of the heightfield. Crests are
//!   locally convex and lose height; hollows are concave and fill.
//! - **failure** — no ground stands above the critical angle. What does, fails.
//! - **talus** — what fails comes to rest below, at the angle of repose.

//! **Form matching, not transport.** The profile these processes converge on is
//! generated directly. Nothing tracks where material went: failure removes and
//! talus deposits, and the two are not required to balance.

//! **Single pass.** All three read the surface handed up from below and each
//! returns a correction to it — none reads another's output, so the stage
//! cannot consume its own result and the corrections compose in any order.
//! Every operator is a min or max over a bounded hex neighbourhood, so the
//! answer is the same however the tiles are visited.

use std::sync::LazyLock;

use common::camera::{HEX_RADIUS, RISE};

use crate::lithology::resistance_at;
use crate::spine::{ELEVATION_PER_Z, STREAM_START_WIDTH, STREAM_WIDTH_PER_MERGE};
use crate::{SQRT_3, TILE_SPACING};

// ── Angles ──────────────────────────────────────────────────────────────────

/// The gradient that reads as 45 degrees on screen, in z-levels per world unit
/// of ground. Terrain works in z-levels and tile spacings; a failure angle is a
/// real angle, so the two are reconciled here and nowhere else. Neighbouring
/// tile centres sit `HEX_RADIUS * sqrt(3)` apart and one z-level stands `RISE`
/// tall, so a gradient of 1 z per tile reads as 24.8 degrees, not 45.
static SLOPE_PER_TANGENT: LazyLock<f64> = LazyLock::new(|| {
    (HEX_RADIUS as f64 * SQRT_3) / (RISE as f64 * ELEVATION_PER_Z) * TILE_SPACING
});

/// Ground gradient, in z-levels per world unit, that reads as `deg` on screen.
fn slope_for_angle(deg: f64) -> f64 {
    deg.to_radians().tan() * *SLOPE_PER_TANGENT
}

/// Angle of repose — the slope loose, angular rock debris comes to rest at. A
/// material property of broken rock, not a dial: scree cones the world over
/// stand within a couple of degrees of it.
const REPOSE_ANGLE_DEG: f64 = 34.0;

/// Angle a bare face of intact rock stands at before it fails. Debris is held
/// by friction alone; rock is cohesive as well, so a strong, jointed rock mass
/// holds a wall far steeper than the scree it sheds.
///
/// Scaled by [`resistance_at`], so weak material fails closer to the repose
/// angle and resistant material holds a cliff. It has to stay above the ground
/// the layers below deliberately stand up — a glacially maintained headwall is
/// legitimately steep in strong rock, and capping it at a hillslope angle would
/// plane every cirque in the world.
const CRITICAL_ANGLE_DEG: f64 = 75.0;

static REPOSE_SLOPE: LazyLock<f64> = LazyLock::new(|| slope_for_angle(REPOSE_ANGLE_DEG));
static CRITICAL_SLOPE: LazyLock<f64> = LazyLock::new(|| slope_for_angle(CRITICAL_ANGLE_DEG));

/// The slope loose debris comes to rest at, in z per world unit. Read by the
/// layer that lays aprons against published faces.
pub fn repose_slope() -> f64 { *REPOSE_SLOPE }

/// How far failure and deposition act from a face, in world units.
///
/// Shorter than [`SLOPE_FORM_REACH`]: that carries an extra ring so every tile
/// in the sampled buffer has its own six neighbours inside it, which is a
/// property of reading a buffer, not of how far a face reaches. A face index
/// answers from geometry and wants the reach itself.
pub const MASS_WASTING_REACH: f64 = FAIL_RINGS as f64 * TILE_SPACING;

// ── Neighbourhood ───────────────────────────────────────────────────────────

/// Characteristic hillslope length — the run over which creep redistributes
/// material, and so the width of the convex cap it puts on a crest.
///
/// Set to the half-width of the narrowest channel the water layer means to read
/// as a channel: a stream starts [`STREAM_START_WIDTH`] across and gains
/// [`STREAM_WIDTH_PER_MERGE`] with its first tributary. Creep therefore rounds
/// away the un-merged rills, which are narrower than the tile they are cut in,
/// and leaves every channel that has gathered a tributary its section.
const HILLSLOPE_LENGTH: f64 = (STREAM_START_WIDTH + STREAM_WIDTH_PER_MERGE) / 2.0;

/// Rings of neighbours the creep kernel gathers. The kernel's full span is one
/// hillslope length, so it reaches from a channel's centreline to its rim and
/// no further — a kernel wider than that averages the two rims of the narrowest
/// surviving channel together and fills it in.
const CREEP_RINGS: i32 = (HILLSLOPE_LENGTH / (2.0 * TILE_SPACING)) as i32;

/// Truncating a Gaussian at twice its standard deviation keeps 95% of its mass,
/// so the kernel radius is 2 sigma.
const CREEP_SIGMA: f64 = HILLSLOPE_LENGTH / 4.0;

/// Rings failure and deposition reach over. The same hillslope: a face fails
/// onto the ground below it, and that ground is the hillslope's own.
const FAIL_RINGS: i32 = CREEP_RINGS;

/// Tiles in a hex ball of `rings`.
const fn ball(rings: i32) -> usize {
    (3 * rings * (rings + 1) + 1) as usize
}

/// Rings gathered for one tile. One past [`FAIL_RINGS`], because deciding
/// whether a tile is the foot of a face reads its six neighbours, and every
/// tile that can source an apron must have all six inside the buffer.
const GATHER_RINGS: i32 = FAIL_RINGS + 1;

const GATHER_TILES: usize = ball(GATHER_RINGS);

/// How far a tile's slope form reads from its own position, in world units.
/// Every sub-primitive reads the same gathered ball, so this is the whole of
/// the stage's reach — nothing stacks on top of it.
pub const SLOPE_FORM_REACH: f64 = GATHER_RINGS as f64 * TILE_SPACING;

/// The tallest apron the neighbourhood can express. An apron thins at the
/// repose slope, so one this tall has run out exactly at the edge of the
/// reach — which is what stops a bounded pass from truncating a cone and
/// leaving a step where it ran out of buffer. Faces tall enough to build a
/// longer apron than this get this one.
static MAX_APRON: LazyLock<f64> =
    LazyLock::new(|| *REPOSE_SLOPE * FAIL_RINGS as f64 * TILE_SPACING);

/// The six coordinate offsets. Hex neighbours are these, never a search.
const NEIGHBOURS: [(i32, i32); 6] = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, -1), (-1, 1)];

/// A hex ball of offsets with their world-unit distances, plus the reverse
/// lookup from offset to slot so a gathered buffer can be walked by neighbour.
struct Disk {
    rings: i32,
    offsets: Vec<(i32, i32)>,
    dist: Vec<f64>,
    /// Slot of each offset, or `usize::MAX` outside the ball. Indexed by
    /// `(dq + rings) * (2 * rings + 1) + (dr + rings)`.
    slot: Vec<usize>,
    centre: usize,
}

impl Disk {
    fn new(rings: i32) -> Self {
        let span = (2 * rings + 1) as usize;
        let mut offsets = Vec::with_capacity(ball(rings));
        let mut dist = Vec::with_capacity(ball(rings));
        let mut slot = vec![usize::MAX; span * span];
        let (ox, oy) = crate::hex_to_world(0, 0);
        for dq in -rings..=rings {
            let lo = (-rings).max(-dq - rings);
            let hi = rings.min(-dq + rings);
            for dr in lo..=hi {
                slot[(dq + rings) as usize * span + (dr + rings) as usize] = offsets.len();
                let (wx, wy) = crate::hex_to_world(dq, dr);
                dist.push((wx - ox).hypot(wy - oy));
                offsets.push((dq, dr));
            }
        }
        let centre = slot[rings as usize * span + rings as usize];
        Self { rings, offsets, dist, slot, centre }
    }

    /// Slot of `(dq, dr)`, or `None` outside the ball.
    fn slot(&self, dq: i32, dr: i32) -> Option<usize> {
        if dq.abs() > self.rings || dr.abs() > self.rings {
            return None;
        }
        let span = (2 * self.rings + 1) as usize;
        match self.slot[(dq + self.rings) as usize * span + (dr + self.rings) as usize] {
            usize::MAX => None,
            s => Some(s),
        }
    }
}

static GATHER_DISK: LazyLock<Disk> = LazyLock::new(|| Disk::new(GATHER_RINGS));

/// Creep kernel weights over [`GATHER_DISK`]: a Gaussian of [`CREEP_SIGMA`],
/// zero past [`CREEP_RINGS`], normalised so the weights sum to one.
static CREEP_WEIGHTS: LazyLock<Vec<f64>> = LazyLock::new(|| {
    let disk = &*GATHER_DISK;
    let limit = CREEP_RINGS as f64 * TILE_SPACING;
    let mut weight: Vec<f64> = disk
        .dist
        .iter()
        .map(|&d| {
            if d > limit { 0.0 } else { (-(d * d) / (2.0 * CREEP_SIGMA * CREEP_SIGMA)).exp() }
        })
        .collect();
    let total: f64 = weight.iter().sum();
    for w in &mut weight {
        *w /= total;
    }
    weight
});

/// Cubic smoothstep on [0, 1], saturating outside it.
fn smoothstep(x: f64) -> f64 {
    let t = x.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// ── Neighbourhood ───────────────────────────────────────────────────────────

/// One tile's slope-form neighbourhood: the surface below, gathered once, and
/// the material properties read at the tile.
///
/// The gather is the whole cost of the stage — every sub-primitive works from
/// this buffer, and none of them reads another's output.
pub struct Neighbourhood {
    z: [f64; GATHER_TILES],
    /// The gradient the ground here holds before it fails. Read once at the
    /// tile rather than per tap: the ball is one hillslope across, far under
    /// the scale a lithology layer varies at.
    critical: f64,
    /// How readily material creeps here. Resistance divides, as it does at
    /// every other carve site, capped at unity so the pass can never smooth by
    /// more than the full kernel.
    mobility: f64,
}

impl Neighbourhood {
    /// Sample the surface below over the tile's ball. `surface` reads a tile's
    /// pre-slope-form elevation.
    pub fn gather(
        q: i32,
        r: i32,
        wx: f64,
        wy: f64,
        surface: &dyn Fn(i32, i32) -> f64,
    ) -> Self {
        let disk = &*GATHER_DISK;
        let mut z = [0.0f64; GATHER_TILES];
        for (i, &(dq, dr)) in disk.offsets.iter().enumerate() {
            z[i] = surface(q + dq, r + dr);
        }
        let resistance = resistance_at(wx, wy);
        Self {
            z,
            critical: *CRITICAL_SLOPE * resistance,
            mobility: (1.0 / resistance).min(1.0),
        }
    }

    /// The un-formed surface at the tile itself.
    pub fn centre(&self) -> f64 {
        self.z[GATHER_DISK.centre]
    }

    /// The gradient the ground here holds before it fails.
    pub fn critical(&self) -> f64 {
        self.critical
    }

    /// Planar gradient magnitude at the tile, least-squares fitted over the six
    /// neighbours. Six evenly spaced unit directions give a sum of outer
    /// products of `3 I`, so the fit is the weighted sum over three.
    fn gradient(&self) -> f64 {
        let disk = &*GATHER_DISK;
        let z0 = self.centre();
        let (ox, oy) = crate::hex_to_world(0, 0);
        let (mut gx, mut gy) = (0.0, 0.0);
        for (dq, dr) in NEIGHBOURS {
            let Some(n) = disk.slot(dq, dr) else { continue };
            let (wx, wy) = crate::hex_to_world(dq, dr);
            let dz = self.z[n] - z0;
            gx += dz * (wx - ox);
            gy += dz * (wy - oy);
        }
        (gx / 3.0).hypot(gy / 3.0)
    }

    /// Hillslope diffusion over one interval, as the bounded convolution it is
    /// equivalent to. Returns the surface creep leaves at the tile.
    ///
    /// Two guards keep the pass off oversteepened rock, which is governed by
    /// mass wasting rather than creep. The **weight** falls to zero as the
    /// tile's own gradient reaches the critical angle. Each **tap** is clamped
    /// to the elevation reachable from the tile at that angle, so a cliff a few
    /// tiles away contributes the slope it would have if it were walkable
    /// instead of its true depth — which is what stops the kernel rounding off
    /// every headwall, escarpment and cirque lip in the world by bleeding
    /// across them, and what leaves a cliff foot concave instead of notched.
    pub fn creep(&self) -> f64 {
        let z0 = self.centre();
        let weight = (1.0 - smoothstep(self.gradient() / self.critical)) * self.mobility;
        if weight <= 0.0 {
            return z0;
        }

        let disk = &*GATHER_DISK;
        let w = &*CREEP_WEIGHTS;
        let mut smoothed = 0.0;
        for i in 0..GATHER_TILES {
            if w[i] == 0.0 {
                continue;
            }
            let reach = self.critical * disk.dist[i];
            smoothed += w[i] * self.z[i].clamp(z0 - reach, z0 + reach);
        }

        z0 + (smoothed - z0) * weight
    }

    /// Cap the tile's height at what the critical angle allows above anything
    /// within reach of it. Only cuts — the material removed is what the apron
    /// puts back below, and the two are not reconciled by volume.
    pub fn failure(&self) -> f64 {
        let disk = &*GATHER_DISK;
        let limit = FAIL_RINGS as f64 * TILE_SPACING;
        let mut limited = self.centre();
        for i in 0..GATHER_TILES {
            let d = disk.dist[i];
            if d > limit {
                continue;
            }
            limited = limited.min(self.z[i] + self.critical * d);
        }
        limited
    }

    /// Depth of debris standing on the tile. A tap sources an apron when it is
    /// the foot of a face — itself below critical, with something above it that
    /// is not — and everything above the angle that face can hold comes down,
    /// thinning away from the foot at the angle of repose.
    ///
    /// A max over sources, so which face is seen first cannot change the
    /// answer — the same discipline the ravine and cirque carves keep.
    pub fn apron(&self) -> f64 {
        let disk = &*GATHER_DISK;
        let limit = FAIL_RINGS as f64 * TILE_SPACING;
        // The step a single tile of ground may stand above its neighbour.
        let step = self.critical * TILE_SPACING;

        let mut apron: f64 = 0.0;
        for (i, &(dq, dr)) in disk.offsets.iter().enumerate() {
            let d = disk.dist[i];
            if d > limit {
                continue;
            }
            let mut up: f64 = 0.0;
            let mut down: f64 = 0.0;
            for (nq, nr) in NEIGHBOURS {
                let Some(n) = disk.slot(dq + nq, dr + nr) else { continue };
                up = up.max(self.z[n] - self.z[i]);
                down = down.max(self.z[i] - self.z[n]);
            }
            if down > step || up <= step {
                continue;
            }
            let height = (up - step).min(*MAX_APRON);
            apron = apron.max(height - *REPOSE_SLOPE * d);
        }
        apron.max(0.0)
    }

    /// What the whole stage adds to the surface below at this tile. Each
    /// sub-primitive contributes a correction to the same gathered surface, so
    /// they sum: neither undoes the other and neither reads the other.
    pub fn delta(&self) -> f64 {
        let z0 = self.centre();
        (self.creep() - z0) + (self.failure() - z0) + self.apron()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(q: i32, r: i32, surface: &dyn Fn(i32, i32) -> f64) -> Neighbourhood {
        let (wx, wy) = crate::hex_to_world(q, r);
        Neighbourhood::gather(q, r, wx, wy, surface)
    }

    /// One z-level per tile is the rise a step can take, and the render
    /// geometry makes that a 24.8 degree slope — not 45. Angles stated in this
    /// module mean nothing if that conversion drifts.
    #[test]
    fn walkable_slope_reads_as_its_screen_angle() {
        let walkable = ELEVATION_PER_Z / TILE_SPACING;
        let deg = (walkable / *SLOPE_PER_TANGENT).atan().to_degrees();
        assert!(
            (deg - 24.79).abs() < 0.01,
            "one z per tile reads as {deg} degrees, not 24.79"
        );
    }

    #[test]
    fn repose_is_gentler_than_failure() {
        assert!(
            *REPOSE_SLOPE < *CRITICAL_SLOPE,
            "debris must come to rest gentler than the face that shed it"
        );
    }

    /// A cliff stays impassable whatever the limiter does to it: the critical
    /// angle has to stay far above the rise a step can climb, or capping
    /// gradients would open every headwall in the world to a walker.
    #[test]
    fn the_critical_angle_is_not_walkable() {
        assert!(
            *CRITICAL_SLOPE > 4.0 * (ELEVATION_PER_Z / TILE_SPACING),
            "a slope capped at critical would be climbable"
        );
    }

    #[test]
    fn creep_weights_sum_to_one() {
        let total: f64 = CREEP_WEIGHTS.iter().sum();
        assert!((total - 1.0).abs() < 1e-12, "kernel is not normalised: {total}");
    }

    /// The creep kernel must stay inside the gathered ball, and the ball must
    /// be a hex ball addressed consistently by offset.
    #[test]
    fn the_gathered_ball_is_addressed_consistently() {
        assert!(CREEP_RINGS <= GATHER_RINGS, "the creep kernel outruns the gather");
        assert_eq!(GATHER_DISK.offsets.len(), GATHER_TILES);
        assert_eq!(GATHER_DISK.offsets[GATHER_DISK.centre], (0, 0));
        for (i, &(dq, dr)) in GATHER_DISK.offsets.iter().enumerate() {
            assert_eq!(GATHER_DISK.slot(dq, dr), Some(i));
        }
        assert_eq!(GATHER_DISK.slot(GATHER_RINGS, GATHER_RINGS), None);
    }

    /// Flat ground has nothing to redistribute. A cirque floor is flat, and a
    /// pass that moved it would drain the tarn standing on it.
    #[test]
    fn flat_ground_is_untouched() {
        let flat = |_q: i32, _r: i32| 100.0;
        let moved = at(0, 0, &flat).delta();
        assert!(moved.abs() < 1e-9, "flat ground moved by {moved}");
    }

    /// A plane has no curvature, so creep has nothing to smooth: the gather is
    /// symmetric about the tile and cancels. Held under the critical angle so
    /// the taps are not clamped and nothing fails.
    #[test]
    fn a_uniform_walkable_slope_is_untouched() {
        let slope = *CRITICAL_SLOPE * 0.5;
        let plane = move |q: i32, r: i32| {
            let (wx, _) = crate::hex_to_world(q, r);
            1000.0 + wx * slope
        };
        assert!(
            at(0, 0, &plane).delta().abs() < 1e-6,
            "a walkable plane was reshaped by {}",
            at(0, 0, &plane).delta()
        );
    }

    /// The point of the stage: a crest is locally convex, so creep lowers it,
    /// and a hollow is concave, so creep fills it.
    #[test]
    fn creep_lowers_crests_and_fills_hollows() {
        let ridge = |q: i32, r: i32| {
            let (wx, _) = crate::hex_to_world(q, r);
            500.0 - wx.abs() * *CRITICAL_SLOPE * 0.4
        };
        assert!(at(0, 0, &ridge).creep() < 500.0, "crest was not lowered");

        let hollow = |q: i32, r: i32| {
            let (wx, _) = crate::hex_to_world(q, r);
            500.0 + wx.abs() * *CRITICAL_SLOPE * 0.4
        };
        assert!(at(0, 0, &hollow).creep() > 500.0, "hollow was not filled");
    }

    /// Without the tap clamp the kernel averages the foot of a cliff into the
    /// ground above it and the edge rounds away. The clamp holds the pull to
    /// one critical-angle step per tile of separation.
    #[test]
    fn creep_does_not_bleed_across_a_cliff() {
        // A flat bench, with a 400 z drop off its eastern edge two tiles away.
        let cliff = |q: i32, r: i32| {
            let (wx, _) = crate::hex_to_world(q, r);
            if wx < 1.5 { 500.0 } else { 100.0 }
        };
        let z = at(0, 0, &cliff).creep();
        assert!(z < 500.0, "the bench edge was not drawn down at all");
        let pull = 500.0 - z;
        assert!(
            pull < *CRITICAL_SLOPE * CREEP_RINGS as f64 * TILE_SPACING,
            "the cliff pulled the bench down {pull}, more than the clamp allows"
        );
    }

    /// A tower standing well above the critical angle is cut back to it.
    #[test]
    fn failure_caps_a_supercritical_step() {
        let tower = |q: i32, r: i32| if (q, r) == (0, 0) { 900.0 } else { 100.0 };
        let limited = at(0, 0, &tower).failure();
        assert!(
            limited <= 100.0 + *CRITICAL_SLOPE * TILE_SPACING + 1e-9,
            "the tower stood at {limited}, above the critical angle over one tile"
        );
    }

    /// Debris from a face comes to rest on the ground below it, thinning away
    /// from the foot, and reaches nothing by the edge of the neighbourhood.
    #[test]
    fn talus_aprons_the_foot_of_a_face() {
        // A wall along x = 0: flat bench to the east, 600 z of rock to the west.
        let wall = |q: i32, r: i32| {
            let (wx, _) = crate::hex_to_world(q, r);
            if wx < -0.5 { 700.0 } else { 100.0 }
        };

        let mut prev = f64::MAX;
        for step in 0..=FAIL_RINGS {
            // Walk east, away from the wall, staying on the bench.
            let thickness = at(step, 0, &wall).apron();
            if step == 0 {
                assert!(thickness > 0.0, "no apron at the foot of the wall");
            }
            assert!(
                thickness <= prev + 1e-9,
                "the apron thickened away from the wall at {step}: {thickness} > {prev}"
            );
            prev = thickness;
        }
        assert!(
            prev <= 1e-9,
            "the apron still stood {prev} deep at the edge of the reach"
        );

        let plain = |_q: i32, _r: i32| 100.0;
        assert_eq!(
            at(0, 0, &plain).apron(),
            0.0,
            "an apron appeared with no face to shed it"
        );
    }

    /// The stage is a pure function of the surface below, so repeating it
    /// cannot drift, and no sub-primitive may read another's output.
    #[test]
    fn slope_form_is_deterministic() {
        let terrain = |q: i32, r: i32| {
            let (wx, wy) = crate::hex_to_world(q, r);
            300.0 + (wx * 0.05).sin() * 40.0 + (wy * 0.03).cos() * 25.0 + wx * 0.4
        };
        let first = at(0, 0, &terrain).delta();
        for _ in 0..4 {
            assert_eq!(at(0, 0, &terrain).delta(), first);
        }
    }
}
