//! Lowland turf. Averages to the plain-green ramp stop of `terrain.wgsl`
//! (elevation 20) so the tile reads as the same ground the ramp paints from
//! afar.

use crate::canvas::Canvas;
use crate::color::{lerp, luminance, scale, smoothstep, Rgb};
use crate::noise::{fbm, hash01, id01, perlin, worley};
use crate::textures::Params;

/// terrain.wgsl ramp stop at elevation 20, linear RGB.
const PLAIN: Rgb = [0.314, 0.627, 0.314];
/// The ground between blades.
const GAP: Rgb = [0.14, 0.32, 0.11];
const SHADE: Rgb = [0.18, 0.40, 0.14];
const SUN: Rgb = [0.36, 0.62, 0.22];
const EARTH: Rgb = [0.15, 0.13, 0.08];
const STONE: Rgb = [0.22, 0.21, 0.16];

/// Stroke candidates per pixel of tile area; clump centres keep them all,
/// gaps keep under half.
const STROKE_DENSITY: f32 = 1.0 / 6.0;

pub fn build(p: &Params) -> Canvas {
    let n = p.size as usize;
    let s = p.seed;
    let mut c = Canvas::filled(n, n, GAP);

    // Low-frequency fields are sampled through a warp so their lattice rows
    // bend instead of crossing the tile as straight bands.
    let warp = |u: f32, v: f32| {
        (u + 0.06 * fbm(u, v, 4, 2, 0.5, s ^ 0x01), v + 0.06 * fbm(u, v, 4, 2, 0.5, s ^ 0x02))
    };

    // Clumps: cells pushed out of their polygons by noise. `mound` peaks at
    // each centre and falls to zero between them.
    let clump = |u: f32, v: f32| {
        let (wu, wv) = warp(u, v);
        let cell = worley(wu, wv, 14, s ^ 0x11);
        (smoothstep(0.7, 0.1, cell.f1), id01(cell.id))
    };

    // Bare earth: a few patches spread over the tile, ragged-edged, with a
    // wide enough fringe for blades to straggle across.
    let earth = |u: f32, v: f32| {
        let (wu, wv) = warp(u, v);
        let mask = fbm(wu, wv, 3, 3, 0.5, s ^ 0x33) + 0.2 * perlin(u, v, 24, s ^ 0x34);
        smoothstep(0.26, 0.44, mask)
    };

    // Uneven growth: the ground drifts toward shade and sun, and the gaps
    // between clumps sit in shadow.
    c.map(|u, v, px| {
        let (wu, wv) = warp(u, v);
        let k = fbm(wu, wv, 5, 4, 0.5, s);
        let (mound, _) = clump(u, v);
        scale(lerp(px, if k < 0.0 { SHADE } else { SUN }, k.abs() * 0.5), 0.80 + 0.30 * mound)
    });

    // Bare earth: grit and small stones inside, a dark rim where the turf
    // is cut away.
    c.map(|u, v, px| {
        let t = earth(u, v);
        let grit = 1.0 + 0.25 * perlin(u, v, 128, s ^ 0x36);
        let stone = smoothstep(0.35, 0.15, worley(u, v, 80, s ^ 0x37).f1);
        let soil = lerp(scale(EARTH, grit), STONE, stone * 0.5);
        let rim = smoothstep(0.0, 0.5, t) * (1.0 - smoothstep(0.5, 1.0, t));
        scale(lerp(px, lerp(soil, GAP, 0.15), t), 1.0 - 0.35 * rim)
    });

    // Blades: short strokes a shade above the ground. Neighbours lean a
    // little alike so the turf has a lay, but not enough to curl into
    // whorls; clump centres grow longer and denser;
    // strokes thin out across the earth's edge into a fringe. Lengths scale
    // with the tile so the turf is the same turf at any size.
    let strokes = (n * n) as f32 * STROKE_DENSITY;
    for i in 0..strokes as i64 {
        let r = |k: i64| hash01(i, k, s ^ 0x55);
        let (x, y) = (r(0) * n as f32, r(1) * n as f32);
        let (u, v) = (x / n as f32, y / n as f32);
        let (mound, id) = clump(u, v);
        if r(7) > 0.45 + 0.55 * mound {
            continue;
        }
        let lay = fbm(u, v, 4, 2, 0.5, s ^ 0x56) * 1.2;
        let angle = lay + (r(2) - 0.5) * 2.2;
        let bend = angle + (r(6) - 0.5) * 0.6;
        let len = n as f32 * (0.008 + 0.018 * r(3)) * (0.7 + 0.6 * mound);
        let (mx, my) = (x + angle.cos() * len * 0.5, y + angle.sin() * len * 0.5);
        let (ex, ey) = (mx + bend.cos() * len * 0.5, my + bend.sin() * len * 0.5);
        let cover = 1.0 - 0.75 * earth(u, v);
        let tone = lerp(lerp(SHADE, SUN, r(4) * r(4)), SUN, 0.3 * id);
        let alpha = (0.20 + 0.30 * r(5)) * cover;
        c.line(x as i64, y as i64, mx as i64, my as i64, tone, alpha);
        c.line(mx as i64, my as i64, ex as i64, ey as i64, tone, alpha);
    }

    // Only tile-scale drift is flattened; anything finer is the texture.
    c.equalize(n / 4);
    c.set_mean_luminance(luminance(PLAIN));
    c.grain(0.03, s ^ 0x44);
    c
}
