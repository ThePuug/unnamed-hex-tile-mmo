//! Orogen field prototype — belt geometry, taper, asymmetry, cost.
//!
//! Run: cargo test -p world --release --test orogen_field_probe -- --ignored --nocapture

use world::orogen_field::{
    BELT_HALF_WIDTH, Crest, OROGEN_MAX_RISE, SHORTENING_FULL, SHORTENING_GATE,
    is_belt, project_to_crest, relief, shortening, surface,
};
use world::{SEA_MAX_DEPTH, substrate_elevation_at};

const SEED: u64 = 0x9E3779B97F4A7C15;
/// Belts sit ~13,000 WU apart at this wavelength, so a block has to be large
/// to hold a population of them.
const SPAN: f64 = 400_000.0;

fn pct(v: &[f64], p: f64) -> f64 {
    if v.is_empty() { return f64::NAN }
    v[((v.len() - 1) as f64 * p).round() as usize]
}

fn sorted(v: &[f64]) -> Vec<f64> {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    s
}

fn stats(label: &str, v: Vec<f64>) {
    if v.is_empty() { println!("  {label:<32} none"); return }
    let s = sorted(&v);
    println!("  {label:<32} n {:>7}  p10 {:>8.2}  p50 {:>8.2}  p90 {:>8.2}  max {:>8.2}",
             s.len(), pct(&s, 0.10), pct(&s, 0.50), pct(&s, 0.90), s[s.len() - 1]);
}

/// Positions that project onto a crest, with that crest.
fn belt_sites(n: i32) -> Vec<(f64, f64, Crest)> {
    let mut out = Vec::new();
    for i in 0..n {
        for j in 0..n {
            let wx = -SPAN * 0.5 + SPAN * i as f64 / n as f64;
            let wy = -SPAN * 0.5 + SPAN * j as f64 / n as f64;
            if let Some(c) = project_to_crest(wx, wy, SEED) { out.push((wx, wy, c)) }
        }
    }
    out
}

/// Unit vector toward vergence at a crest, recovered from the crest's own
/// signed across-distance so the probe never re-derives the rule.
fn vergence_of(c: &Crest, _wx: f64, _wy: f64) -> (f64, f64) {
    let (ax, ay) = (-c.axis.1, c.axis.0);
    // `across` is pure geometry now and the vergence direction lives in the sign
    // of the asymmetry, so the vergence side is the axis normal turned that way.
    if c.asymmetry >= 0.0 { (ax, ay) } else { (-ax, -ay) }
}

// ── 1. Amplitude taper ──────────────────────────────────────────────────────

#[test]
#[ignore]
fn amplitude_taper() {
    println!("\n=== AMPLITUDE TAPER ===");
    println!("half-width {BELT_HALF_WIDTH:.0} WU; the two flanks differ by design");
    let sites = belt_sites(120);
    println!("belt sites: {}", sites.len());

    println!("\n  relief remaining as a share of the crest's own, by side");
    for frac in [0.25, 0.50, 0.75, 1.00] {
        let (mut steep, mut graded) = (Vec::new(), Vec::new());
        for (wx, wy, c) in sites.iter().take(5000) {
            let at_crest = relief(c.x, c.y, SEED);
            if at_crest <= 1e-9 { continue }
            let v = vergence_of(c, *wx, *wy);
            for (sign, bucket) in [(1.0f64, &mut steep), (-1.0f64, &mut graded)] {
                let d = BELT_HALF_WIDTH * frac * sign;
                let r = relief(c.x + v.0 * d, c.y + v.1 * d, SEED);
                bucket.push(r / at_crest);
            }
        }
        println!("    {:>4.0}% of half-width:   vergence side p50 {:.3}   far side p50 {:.3}",
                 100.0 * frac, pct(&sorted(&steep), 0.5), pct(&sorted(&graded), 0.5));
    }

    // Relief sitting where the fold axis is least trustworthy. The measured
    // off-axis share of belt AREA is 10.6% * u^2.75 at depth fraction u.
    let mut weighted = 0.0;
    let mut total = 0.0;
    for (wx, wy, c) in &sites {
        let r = relief(*wx, *wy, SEED);
        if r <= 0.0 { continue }
        let half = if c.across >= 0.0 {
            2.0 * BELT_HALF_WIDTH * 0.35 / 1.35
        } else {
            2.0 * BELT_HALF_WIDTH / 1.35
        };
        let u = (c.across.abs() / half).min(1.0);
        weighted += r * 0.106 * u.powf(2.75);
        total += r;
    }
    println!("\n  relief-weighted share in the >32deg-off region: {:.2}%",
             100.0 * weighted / total.max(1e-9));
    println!("    (unweighted belt area at the full half-width: 10.6%)");
}

// ── 2. Asymmetry, at both scales ────────────────────────────────────────────

#[test]
#[ignore]
fn asymmetry() {
    println!("\n=== CROSS-STRIKE ASYMMETRY ===");
    println!("the previous warp mechanism measured 52.0% steeper on the vergence side;");
    println!("50% is a coin flip and means no asymmetry at all.");
    println!("measured as crest-to-half-height distance on each side, which is scale-free:");
    println!("mean slope over a fixed span would punish the narrower flank for its zeros.");
    let sites = belt_sites(100);

    // Belt wedge: from the crest itself, out along vergence and against it.
    {
        let mut ratios = Vec::new();
        let mut steeper = 0usize;
        for (wx, wy, c) in sites.iter().take(8000) {
            let v = vergence_of(c, *wx, *wy);
            let peak = relief(c.x, c.y, SEED);
            if peak <= 1e-6 { continue }
            let half_dist = |sign: f64| -> Option<f64> {
                let mut t = 0.0;
                let step = BELT_HALF_WIDTH * 0.02;
                while t < BELT_HALF_WIDTH * 2.5 {
                    t += step;
                    let r = relief(c.x + v.0 * t * sign, c.y + v.1 * t * sign, SEED);
                    if r <= peak * 0.5 { return Some(t) }
                }
                None
            };
            let (Some(dv), Some(df)) = (half_dist(1.0), half_dist(-1.0)) else { continue };
            // Steeper = reaches half height in a shorter distance.
            let r = df / dv;
            if r > 1.0 { steeper += 1 }
            ratios.push(r);
        }
        println!("\n  belt wedge — (far half-distance) / (vergence half-distance)");
        stats("    ratio, >1 = steeper toward vergence", ratios.clone());
        println!("    steeper on the vergence side: {steeper} of {} ({:.1}%)",
                 ratios.len(), 100.0 * steeper as f64 / ratios.len().max(1) as f64);
    }
}

// ── 3. Relief, coverage, island arcs ────────────────────────────────────────

#[test]
#[ignore]
fn relief_and_arcs() {
    println!("\n=== RELIEF, COVERAGE, ISLAND ARCS ===");
    const N: i32 = 600;
    let mut in_belt = Vec::new();
    let (mut belt, mut land, mut belt_land, mut total) = (0usize, 0usize, 0usize, 0usize);
    let (mut ocean_belt, mut breach) = (0usize, 0usize);

    for i in 0..N {
        for j in 0..N {
            let wx = -SPAN * 0.5 + SPAN * i as f64 / N as f64;
            let wy = -SPAN * 0.5 + SPAN * j as f64 / N as f64;
            total += 1;
            let base = substrate_elevation_at(wx, wy, SEED);
            if base >= 0.0 { land += 1 }
            let r = relief(wx, wy, SEED);
            if r <= 0.0 { continue }
            belt += 1;
            if base >= 0.0 { belt_land += 1 } else {
                ocean_belt += 1;
                if base + r >= 0.0 { breach += 1 }
            }
            in_belt.push(r);
        }
    }

    println!("  block {SPAN:.0} WU, {N}x{N} samples");
    println!("  world that is belt   {:.2}%", 100.0 * belt as f64 / total as f64);
    println!("  land that is belt    {:.2}%   (coverage measured at 26.6%)",
             100.0 * belt_land as f64 / land.max(1) as f64);
    println!("  land share of block  {:.2}%\n", 100.0 * land as f64 / total as f64);
    stats("relief in belt, z", in_belt);
    println!("  OROGEN_MAX_RISE {OROGEN_MAX_RISE:.1} z, SEA_MAX_DEPTH {SEA_MAX_DEPTH:.0} z\n");
    println!("  oceanic belt cells {ocean_belt} ({:.2}% of block)",
             100.0 * ocean_belt as f64 / total as f64);
    println!("  of those breaching sea level: {breach} ({:.1}%)  <- island arc",
             100.0 * breach as f64 / ocean_belt.max(1) as f64);
    println!("  (100% means the ceiling is too high, 0% too low)");
}

// ── 4. Cost ─────────────────────────────────────────────────────────────────

#[test]
#[ignore]
fn cost() {
    println!("\n=== PER-POSITION COST ===");
    println!("lookup: Newton projection along the least-curvature axis of the shortening field");
    println!("  1 across_axis call  = 9 shortening evals = 36 velocity evals = 144 simplex");
    const N: i32 = 300;
    let (mut hits, mut misses) = (0usize, 0usize);
    let mut walked = Vec::new();
    for i in 0..N {
        for j in 0..N {
            let wx = -SPAN * 0.5 + SPAN * i as f64 / N as f64;
            let wy = -SPAN * 0.5 + SPAN * j as f64 / N as f64;
            match project_to_crest(wx, wy, SEED) {
                Some(c) => { hits += 1; walked.push((wx - c.x).hypot(wy - c.y)) }
                None => misses += 1,
            }
        }
    }
    println!("\n  positions {}   projected onto a crest {hits} ({:.1}%)   rejected {misses}",
             hits + misses, 100.0 * hits as f64 / (hits + misses) as f64);
    stats("  distance walked to crest WU", walked);
    println!("\n  in a belt:  ~4 across_axis calls (3 Newton + 1 axis read) = 144 velocity");
    println!("              evals = 576 simplex, plus one substrate read");
    println!("  outside:    1-2 across_axis calls — rejected on non-negative curvature or");
    println!("              on leaving reach, so the common case is the cheap one");
}

// ── Sanity ──────────────────────────────────────────────────────────────────

/// A field answers per position with no neighbourhood and no state.
#[test]
fn field_is_deterministic_and_bounded() {
    for i in 0..40 {
        for j in 0..40 {
            let wx = -80_000.0 + i as f64 * 4_000.0;
            let wy = -80_000.0 + j as f64 * 4_000.0;
            assert_eq!(relief(wx, wy, SEED), relief(wx, wy, SEED));
            assert_eq!(surface(wx, wy, SEED), surface(wx, wy, SEED));
            let r = relief(wx, wy, SEED);
            assert!(r >= 0.0 && r <= OROGEN_MAX_RISE * 1.001,
                    "relief {r} out of range at ({wx}, {wy})");
            if r > 0.0 { assert!(is_belt(wx, wy, SEED)) }
        }
    }
}

/// Amplitude is a property of the crest, not of the flank being asked about —
/// so the gate reads the crest and a weak flank still carries its belt.
#[test]
fn gate_reads_the_crest_not_the_position() {
    let mut weaker_than_crest = 0;
    for i in 0..70 {
        for j in 0..70 {
            let wx = -100_000.0 + i as f64 * 3_000.0;
            let wy = -100_000.0 + j as f64 * 3_000.0;
            let Some(c) = project_to_crest(wx, wy, SEED) else { continue };
            // `strength` is the gate-ramped amplitude and is zero at the gate
            // by design, so what must hold is that the crest itself cleared it.
            let crest_raw = shortening(c.x, c.y, SEED) / SHORTENING_FULL;
            assert!(crest_raw >= SHORTENING_GATE - 1e-9,
                    "a crest below the gate was returned: {crest_raw}");
            assert!(c.strength >= 0.0 && c.strength <= 1.0,
                    "strength out of range: {}", c.strength);
            if shortening(wx, wy, SEED) / SHORTENING_FULL < SHORTENING_GATE {
                weaker_than_crest += 1;
            }
        }
    }
    assert!(weaker_than_crest > 0,
            "no flank was weaker than its own crest — the distinction is untested here");
}

/// The wedge must lean against the drift. The `across` convention is internally
/// consistent by construction, so this checks it against the physical rule
/// rather than against itself: compute the expected vergence direction straight
/// from drift, and ask which side actually falls away faster.
#[test]
#[ignore]
fn wedge_leans_against_the_drift() {
    use world::events::motion::plate_drift;
    println!("\n=== VERGENCE SIGN, CHECKED AGAINST DRIFT ===");
    let sites = belt_sites(90);
    let mut agree = 0usize;
    let mut n = 0usize;
    let mut ratios = Vec::new();
    for (_, _, c) in sites.iter().take(6000) {
        let (ax, ay) = (-c.axis.1, c.axis.0);
        let (dx, dy) = plate_drift(c.x, c.y, SEED);
        let m = dx.hypot(dy);
        if m < 1e-12 { continue }
        // Wedge verges against the drift: the expected steep side is whichever
        // of +/-across_axis opposes the drift's across-component.
        let expect = if (dx * ax + dy * ay) < 0.0 { 1.0f64 } else { -1.0 };
        let peak = relief(c.x, c.y, SEED);
        if peak <= 1e-6 { continue }
        let half_dist = |sign: f64| -> Option<f64> {
            let mut t = 0.0;
            let step = BELT_HALF_WIDTH * 0.02;
            while t < BELT_HALF_WIDTH * 2.5 {
                t += step;
                let r = relief(c.x + ax * t * sign, c.y + ay * t * sign, SEED);
                if r <= peak * 0.5 { return Some(t) }
            }
            None
        };
        let (Some(de), Some(do_)) = (half_dist(expect), half_dist(-expect)) else { continue };
        n += 1;
        if de < do_ { agree += 1 }
        ratios.push(do_ / de);
    }
    stats("  (other side) / (expected steep side)", ratios);
    println!("  steep flank on the expected side: {agree} of {n} ({:.1}%)",
             100.0 * agree as f64 / n.max(1) as f64);
    println!("  50% would mean the sign is arbitrary; near 0% would mean it is inverted");
}

/// Where island arcs occur, for rendering.
#[test]
#[ignore]
fn find_arcs() {
    let mut found = Vec::new();
    let n = 700;
    for i in 0..n {
        for j in 0..n {
            let wx = -SPAN * 0.5 + SPAN * i as f64 / n as f64;
            let wy = -SPAN * 0.5 + SPAN * j as f64 / n as f64;
            let base = substrate_elevation_at(wx, wy, SEED);
            if base > -60.0 { continue }           // genuinely offshore
            let r = relief(wx, wy, SEED);
            if r <= 0.0 || base + r < 5.0 { continue }
            found.push((wx, wy, base, r));
        }
    }
    println!("\narc candidates: {}", found.len());
    for (wx, wy, b, r) in found.iter().step_by(found.len().max(1) / 8 + 1).take(8) {
        println!("  ({wx:>9.0}, {wy:>9.0})  seafloor {b:>7.1} z  relief {r:>7.1} z  -> {:>6.1} z",
                 b + r);
    }
}

/// Mean flank angle on the built field. The ceiling was derived from this, so
/// this is the measurement that says whether the derivation survived contact
/// with the taper, the fold texture and the strength distribution.
#[test]
#[ignore]
fn flank_angle() {
    const RISE: f64 = 0.8;
    println!("\n=== MEAN FLANK ANGLE ===");
    println!("angle = atan(crest relief x RISE / distance to the flank's toe)");
    println!("target {:.0} deg; repose clusters at 30-35 deg", 20.0);
    let sites = belt_sites(110);

    let (mut steep, mut graded, mut nominal) = (Vec::new(), Vec::new(), Vec::new());
    for (wx, wy, c) in sites.iter().take(9000) {
        let v = vergence_of(c, *wx, *wy);
        let peak = relief(c.x, c.y, SEED);
        if peak <= 1.0 { continue }
        let toe = |sign: f64| -> Option<f64> {
            let step = BELT_HALF_WIDTH * 0.02;
            let mut t = 0.0;
            while t < BELT_HALF_WIDTH * 2.5 {
                t += step;
                if relief(c.x + v.0 * t * sign, c.y + v.1 * t * sign, SEED) <= peak * 0.02 {
                    return Some(t)
                }
            }
            None
        };
        let (Some(ds), Some(dg)) = (toe(1.0), toe(-1.0)) else { continue };
        steep.push((peak * RISE / ds).atan().to_degrees());
        graded.push((peak * RISE / dg).atan().to_degrees());
        nominal.push((peak * RISE / BELT_HALF_WIDTH).atan().to_degrees());
    }
    stats("steep (vergence) flank, deg", steep.clone());
    stats("graded flank, deg", graded);
    stats("against nominal half-width", nominal);
    let s = sorted(&steep);
    println!("  steep flank at or past repose (>=30 deg): {:.1}%",
             100.0 * s.iter().filter(|&&a| a >= 30.0).count() as f64 / s.len().max(1) as f64);
}

/// Highest-relief position in a small neighbourhood, for near-ground renders.
#[test]
#[ignore]
fn find_crest() {
    let (cx, cy) = (17_900.0, 200.0);
    let mut best = (0.0, cx, cy);
    for i in -120..=120 {
        for j in -120..=120 {
            let (wx, wy) = (cx + i as f64 * 50.0, cy + j as f64 * 50.0);
            let r = relief(wx, wy, SEED);
            if r > best.0 { best = (r, wx, wy) }
        }
    }
    println!("\nhighest relief near ({cx}, {cy}): {:.1} z at ({:.0}, {:.0})",
             best.0, best.1, best.2);
}

// ── Texture: across-strike decorrelation and ridge individuality ────────────

/// Wall-clock cost of one relief lookup, over a grid that is mostly outside
/// belts — the shape the world actually queries.
#[test]
#[ignore]
fn relief_timing() {
    let n = 700i64;
    for pass in 0..3 {
        let t0 = std::time::Instant::now();
        let mut sink = 0.0f64;
        let mut inside = 0usize;
        for i in 0..n {
            for j in 0..n {
                let r = relief(-260_000.0 + i as f64 * 120.0, -180_000.0 + j as f64 * 120.0, SEED);
                if r > 0.0 { inside += 1 }
                sink += r;
            }
        }
        let per = t0.elapsed().as_nanos() as f64 / (n * n) as f64;
        println!("  pass {pass}: {per:.0} ns/lookup   nonzero {inside}   checksum {sink:.0}");
    }
}

/// Crest height along a belt's length, against the shortening that produces it.
///
/// Crest height is `OROGEN_MAX_RISE × strength × crust_share`, and `strength` is
/// the shortening *at the crest*, so height varies along a belt exactly as the
/// shortening does. This confirms the variation is there and is that quantity —
/// a tectonic parameter varying, not a texture applied.
#[test]
#[ignore]
fn crest_height_along_strike() {
    println!("\n=== CREST HEIGHT ALONG STRIKE ===");
    let sites = belt_sites(70);
    let (mut spans, mut cors) = (Vec::new(), Vec::new());
    let step = BELT_HALF_WIDTH * 0.5;

    for (_, _, c) in sites.iter().take(600) {
        let (mut hs, mut ss) = (Vec::new(), Vec::new());
        for k in -20..=20i32 {
            let t = k as f64 * step;
            let (x, y) = (c.x + c.axis.0 * t, c.y + c.axis.1 * t);
            let Some(d) = project_to_crest(x, y, SEED) else { continue };
            let h = relief(d.x, d.y, SEED);
            // Continental crust only: crust_share is the other tectonic factor
            // in crest height, and mixing the two hides which is which.
            if h <= 0.0 || substrate_elevation_at(d.x, d.y, SEED) < 0.0 { continue }
            hs.push(h);
            ss.push(shortening(d.x, d.y, SEED) / SHORTENING_FULL);
        }
        if hs.len() < 12 { continue }
        let (lo, hi) = (hs.iter().cloned().fold(f64::MAX, f64::min),
                        hs.iter().cloned().fold(0.0f64, f64::max));
        spans.push(hi - lo);
        // Correlation between crest height and crest shortening down the belt.
        let n = hs.len() as f64;
        let (mh, ms) = (hs.iter().sum::<f64>() / n, ss.iter().sum::<f64>() / n);
        let (mut ab, mut aa, mut bb) = (0.0, 0.0, 0.0);
        for k in 0..hs.len() {
            let (da, db) = (hs[k] - mh, ss[k] - ms);
            ab += da * db; aa += da * da; bb += db * db;
        }
        if aa > 1e-9 && bb > 1e-12 { cors.push(ab / (aa * bb).sqrt()) }
    }
    stats("crest height range along one belt, z", spans.clone());
    stats("correlation(crest height, crest shortening)", cors.clone());
    let c = sorted(&cors);
    println!("    belts where the two move together (r > 0.9): {:.1}%",
             100.0 * c.iter().filter(|v| **v > 0.9).count() as f64 / c.len().max(1) as f64);
}

// ── 5. The reach boundary ───────────────────────────────────────────────────

const STEEP_SHARE: f64 = 0.35;
fn steep_half() -> f64 { 2.0 * BELT_HALF_WIDTH * STEEP_SHARE / (1.0 + STEEP_SHARE) }
fn graded_half() -> f64 { 2.0 * BELT_HALF_WIDTH / (1.0 + STEEP_SHARE) }

/// **Seam regression probe.** Walk out from a crest across strike and find where
/// the radial slope breaks. A truncation of the profile by the crest lookup
/// shows up here as a spike at one fixed distance — a kink, continuous in height
/// and discontinuous in slope, which is invisible to a height-only sweep and
/// which hillshading draws as a hard line along the belt.
#[test]
#[ignore]
fn radial_slope_break() {
    println!("\n=== RADIAL SLOPE BREAK ===");
    println!("candidate boundaries: steep half {:.0} | BELT_HALF_WIDTH {BELT_HALF_WIDTH:.0} \
              | graded half {:.0}", steep_half(), graded_half());
    let sites = belt_sites(70);
    let w = 20.0;
    let out = 6200.0;
    let n = (out / w) as usize;

    for (name, sgn) in [("vergence (steep)", 1.0f64), ("far (graded)", -1.0f64)] {
        let mut brk = Vec::new();
        let mut relief_at_break = Vec::new();
        for (_, _, c) in sites.iter().take(2500) {
            let v = vergence_of(c, 0.0, 0.0);
            let crest_h = relief(c.x, c.y, SEED);
            if crest_h <= 10.0 { continue }
            let at = |t: f64| relief(c.x + v.0 * t * sgn, c.y + v.1 * t * sgn, SEED);
            let slope = |k: usize| (at((k + 1) as f64 * w) - at(k as f64 * w)) / w;
            let (mut bd, mut bs, mut bh) = (0.0, 0.0, 0.0);
            let mut prev = slope(0);
            for k in 1..n {
                let t = k as f64 * w;
                let s = slope(k);
                if (s - prev).abs() > bs && at(t) > 1e-6 {
                    bs = (s - prev).abs(); bd = t; bh = at(t) / crest_h;
                }
                prev = s;
            }
            if bs > 1e-9 { brk.push(bd); relief_at_break.push(bh) }
        }
        println!("\n  ── {name} flank ──");
        stats("    slope-break distance from crest, WU", brk.clone());
        stats("    relief there / crest height", relief_at_break.clone());
        // A truncation makes a spike; the profile's own curvature makes a spread.
        let d = sorted(&brk);
        let mut worst = (0.0, 0.0);
        let mut t = 200.0;
        while t < out {
            let share = d.iter().filter(|x| (**x - t).abs() < 50.0).count() as f64
                / d.len().max(1) as f64;
            if share > worst.1 { worst = (t, share) }
            t += 50.0;
        }
        println!("    most concentrated 100 WU band: {:.0} WU, holding {:.1}% of breaks",
                 worst.0, 100.0 * worst.1);
    }
}

/// How often neighbouring positions resolve to *different* crests. Reach bounds
/// how far a walk may look, so widening it lets more positions see a second
/// crest — and the boundary between two basins is where a discontinuity can
/// appear. Measured as adjacent grid samples whose crest points sit far apart,
/// together with the relief step across those pairs.
#[test]
#[ignore]
fn crest_confusability() {
    println!("\n=== CREST CONFUSABILITY / BASIN BOUNDARIES ===");
    let d = 12.0;
    let mut pairs = 0usize;
    let mut split = 0usize;
    let mut steps = Vec::new();
    let mut ratio = Vec::new();

    for i in 0..900i64 {
        for j in 0..900i64 {
            let wx = -215_000.0 + i as f64 * 60.0;
            let wy = -135_000.0 + j as f64 * 60.0;
            let (a, b) = (project_to_crest(wx, wy, SEED), project_to_crest(wx + d, wy, SEED));
            let (Some(a), Some(b)) = (a, b) else { continue };
            pairs += 1;
            let apart = (a.x - b.x).hypot(a.y - b.y);
            // The two queries are 12 WU apart; a crest that jumps far further
            // than that is a different crest, not the same one re-found.
            if apart > 500.0 {
                split += 1;
                let (ra, rb) = (relief(wx, wy, SEED), relief(wx + d, wy, SEED));
                steps.push((ra - rb).abs());
                let da = (wx - a.x).hypot(wy - a.y);
                let db = (wx + d - b.x).hypot(wy - b.y);
                if da > 1.0 { ratio.push(db / da) }
            }
        }
    }
    println!("  adjacent resolved pairs: {pairs}");
    println!("  resolving to crests over 500 WU apart: {split} ({:.3}%)",
             100.0 * split as f64 / pairs.max(1) as f64);
    stats("  relief step across those pairs, z", steps.clone());
    stats("  distance ratio to the two crests", ratio.clone());
    let r = sorted(&ratio);
    println!("    of those, within 10% of the same distance: {:.1}%",
             100.0 * r.iter().filter(|x| (**x - 1.0).abs() < 0.10).count() as f64 / r.len().max(1) as f64);
}


#[test]
#[ignore]
fn find_asymmetric_belt() {
    println!("\n=== STRONGLY ASYMMETRIC BELTS ===");
    let sites = belt_sites(70);
    let mut best: Vec<(f64, f64, f64, f64)> = Vec::new();
    for (_, _, c) in sites.iter().take(4000) {
        let h = relief(c.x, c.y, SEED);
        if h < 400.0 { continue }
        best.push((c.asymmetry.abs() * h, c.x, c.y, c.asymmetry));
    }
    best.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    for (_, x, y, a) in best.iter().take(8) {
        println!("  ({x:>9.0},{y:>9.0})  asym {a:>6.3}  crest {:.0} z", relief(*x, *y, SEED));
    }
}

/// **Filament regression probe.** Two positions on the same ground must receive
/// comparable relief. A filament is a position claiming the crest is at itself
/// while its neighbours resolve to one far away — a sliver carrying a full wedge
/// beside ground carrying a flank value.
#[test]
#[ignore]
fn filament_census() {
    println!("\n=== FILAMENT CENSUS ===");
    let d = 12.0;
    let (mut resolved, mut selfcrest, mut filament) = (0usize, 0usize, 0usize);
    let mut ratio = Vec::new();
    let side = 800i64;
    let cell = 24.0;
    for i in 0..side {
        for j in 0..side {
            let wx = -95_000.0 + i as f64 * cell;
            let wy = -16_000.0 + j as f64 * cell;
            let Some(c) = project_to_crest(wx, wy, SEED) else { continue };
            resolved += 1;
            if c.across.abs() > 20.0 { continue }
            selfcrest += 1;
            let far = [(d, 0.0), (-d, 0.0), (0.0, d), (0.0, -d)].iter()
                .filter(|(ox, oy)| project_to_crest(wx + ox, wy + oy, SEED)
                    .map_or(true, |n| n.across.abs() > 500.0))
                .count();
            if far >= 2 {
                filament += 1;
                let r0 = relief(wx, wy, SEED);
                let rn = relief(wx + d * 2.0, wy, SEED).max(relief(wx - d * 2.0, wy, SEED));
                if rn > 0.01 { ratio.push(r0 / rn) }
            }
        }
    }
    let span = side as f64 * cell;
    println!("  block {span:.0} WU square, resolved positions {resolved}");
    println!("  claiming the crest at itself:  {selfcrest} ({:.3}%)",
             100.0 * selfcrest as f64 / resolved.max(1) as f64);
    println!("  isolated filaments:            {filament} ({:.4}% of resolved)",
             100.0 * filament as f64 / resolved.max(1) as f64);
    println!("  filaments per 19,200 WU square: {:.1}",
             filament as f64 * (19_200.0 * 19_200.0) / (span * span));
    stats("  relief there / relief 24 WU away", ratio.clone());
}

/// The transect that exposed the filament, kept so the shape can be re-read.
#[test]
#[ignore]
fn filament_transect() {
    println!("\n=== FILAMENT TRANSECT at (-86628, 588) ===");
    let (nx, ny) = (0.764f64, -0.646f64);
    let (cx, cy) = (-86_628.0f64, 588.0f64);
    println!("  {:>6} {:>10} {:>9} {:>9} {:>22}", "t", "relief z", "across", "strength", "crest point");
    for k in -4..=4i32 {
        let t = k as f64 * 4.0;
        let (x, y) = (cx + nx * t, cy + ny * t);
        match project_to_crest(x, y, SEED) {
            None => println!("  {t:>6.0} {:>10.2}      none", relief(x, y, SEED)),
            Some(c) => println!("  {t:>6.0} {:>10.2} {:>9.0} {:>9.3}   ({:.0},{:.0})",
                                relief(x, y, SEED), c.across, c.strength, c.x, c.y),
        }
    }
}
