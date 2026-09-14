//! Exposed mountain ground for high hex tile tops, seen from above: dark
//! ochre earth covered in pebbles, with flat bedrock and lichen showing
//! through. Nothing here casts a shadow beyond its own outline: a detached
//! shadow under a scattered stone reads as a bubble on water, and the
//! engine lights the surface anyway. Not pinned to a ramp stop; this tile
//! is distinctive rather than consistent.

use crate::canvas::Canvas;
use crate::color::{lerp, scale, smoothstep, Rgb};
use crate::noise::{fbm, hash01, perlin, worley};
use crate::textures::Params;

const DIRT: Rgb = [0.20, 0.14, 0.09];
const DUST: Rgb = [0.34, 0.25, 0.15];
const EARTH: Rgb = [0.12, 0.08, 0.05];
const ROCK: Rgb = [0.31, 0.31, 0.29];
const LICHEN: Rgb = [0.46, 0.50, 0.20];
const RUST: Rgb = [0.50, 0.32, 0.12];

/// Pebbles, in the greys and dull tans a mountain sheds, none of them
/// white and none of them bright: those read as river or aquarium gravel.
const PEBBLES: [Rgb; 4] = [[0.36, 0.34, 0.30], [0.26, 0.24, 0.20], [0.40, 0.34, 0.26], [0.30, 0.22, 0.14]];

/// Bedrock cells across the tile, and pebble cells across the tile for
/// the two sizes of pebble.
const BEDROCK: u32 = 4;
const MID: u32 = 16;
const SMALL: u32 = 34;

/// A feature as seen from a pixel: how much of the pixel it covers, its
/// own hash, and its shading, lit from the upper left inside its own
/// outline: a lighter near side and a darker far edge. A shadow cast
/// beyond the outline detaches the feature and reads as a bubble on
/// water.
struct Feature {
    cover: f32,
    key: i64,
    shade: f32,
}

pub fn build(p: &Params) -> Canvas {
    let n = p.size as usize;
    let s = p.seed;
    let mut c = Canvas::filled(n, n, DIRT);

    // Bedrock: two or three flat patches where the rock shows through,
    // each a cell clipped to a radius, its outline the straight cell
    // boundaries where they fall inside and a broken edge, with chunks
    // bitten out, where they do not.
    let bedrock = |u: f32, v: f32| {
        let wu = u + 0.03 * fbm(u, v, 4, 2, 0.5, s ^ 0x10);
        let wv = v + 0.03 * fbm(u, v, 4, 2, 0.5, s ^ 0x11);
        let w = worley(wu, wv, BEDROCK, s ^ 0x12);
        let key = (w.id >> 8) as i64;
        let present = smoothstep(0.75, 0.77, hash01(key, 0, s));
        let radius = 0.5 + 0.25 * hash01(key, 1, s);
        let apart = smoothstep(0.0, 0.15, w.f2 - w.f1);
        let bite = smoothstep(-0.35, -0.15, perlin(u, v, 12, s ^ 0x13));
        let ragged = w.f1 + 0.03 * perlin(u, v, 40, s ^ 0x14);
        let inside = smoothstep(radius, radius * 0.95, ragged) * bite;
        let toward_light = (-w.dx * 0.6 - w.dy * 0.8) / radius;
        let far_edge = smoothstep(0.6, 0.95, ragged / radius) * smoothstep(0.1, -0.3, toward_light);
        let shade = (1.0 + (0.1 * toward_light).clamp(-0.1, 0.1)) * (1.0 - 0.3 * far_edge);
        Feature { cover: present * inside * apart, key, shade }
    };

    // Pebbles: a Voronoi cell holds a pebble or not; the pebble is the
    // cell clipped to a radius larger than most of the cell, so its
    // outline is mostly the straight cell boundaries, a fragment rather
    // than a disc.
    let pebble_at = |u: f32, v: f32, cells: u32, seed: u64, keep: f32| {
        let w = worley(u, v, cells, seed);
        let key = (w.id >> 8) as i64;
        let radius = 0.45 + 0.3 * hash01(key, 1, seed);
        let present = if hash01(key, 0, seed) < keep { 1.0 } else { 0.0 };
        let inside = smoothstep(radius, radius * 0.92, w.f1);
        let apart = smoothstep(0.05, 0.14, w.f2 - w.f1);
        let toward_light = (-w.dx * 0.6 - w.dy * 0.8) / radius;
        let far_edge = smoothstep(0.45, 0.9, w.f1 / radius) * smoothstep(0.1, -0.3, toward_light);
        let shade = (1.0 + (0.18 * toward_light).clamp(-0.2, 0.2)) * (1.0 - 0.4 * far_edge);
        Feature { cover: present * inside * apart, key, shade }
    };

    // Dirt: earthy, dusty where it is trodden thin, darker where it stays
    // damp, broken into clods, gritty throughout.
    c.map(|u, v, px| {
        let mottle = fbm(u, v, 5, 4, 0.55, s ^ 0x30);
        let mut px = lerp(px, DUST, smoothstep(-0.2, 0.7, mottle) * 0.8);
        px = lerp(px, EARTH, smoothstep(0.2, 0.6, -mottle) * 0.7);
        let clod = worley(u, v, 36, s ^ 0x33);
        let lump = 0.94 + 0.12 * hash01((clod.id >> 8) as i64, 0, s);
        let seam = 1.0 - 0.1 * (1.0 - smoothstep(0.0, 0.15, clod.f2 - clod.f1));
        let grit = 1.0 + 0.14 * fbm(u, v, 48, 3, 0.5, s ^ 0x32);
        scale(px, grit * lump * seam)
    });

    // Bedrock surface: cool mid grey, mottled and shaded within its
    // outline, with crusty lichen spots.
    c.map(|u, v, px| {
        let rock = bedrock(u, v);
        if rock.cover <= 0.0 {
            return px;
        }
        let mut face = lerp(ROCK, DUST, 0.2 * smoothstep(-0.5, 0.5, perlin(u, v, 5, s ^ 0x41)));
        let tone = 1.0 + 0.12 * fbm(u, v, 8, 2, 0.5, s ^ 0x47) + 0.1 * fbm(u, v, 30, 3, 0.5, s ^ 0x45);
        face = scale(face, rock.shade * (0.92 + 0.16 * hash01(rock.key, 2, s)) * tone);
        let crust = worley(u, v, 20, s ^ 0x43);
        let patch = smoothstep(0.42, 0.34, crust.f1 + 0.15 * perlin(u, v, 50, s ^ 0x44))
            * smoothstep(0.55, 0.58, hash01((crust.id >> 8) as i64, 0, s));
        let colour = if hash01((crust.id >> 8) as i64, 1, s) < 0.7 { LICHEN } else { RUST };
        face = lerp(face, scale(colour, 0.85 + 0.3 * perlin(u, v, 80, s ^ 0x46)), patch * 0.85);
        lerp(px, face, rock.cover)
    });

    // Pebbles everywhere but on the rock, a little thicker in drifts,
    // larger ones under smaller ones.
    let drift = |u: f32, v: f32| smoothstep(-0.4, 0.6, fbm(u, v, 3, 3, 0.5, s ^ 0x20));
    for (cells, seed, base) in [(MID, s ^ 0x52, 0.18), (SMALL, s ^ 0x51, 0.28)] {
        c.map(|u, v, px| {
            let keep = (base + 0.4 * drift(u, v)) * (1.0 - 0.9 * bedrock(u, v).cover);
            let pebble = pebble_at(u, v, cells, seed, keep);
            let body = PEBBLES[(hash01(pebble.key, 2, seed) * 4.0) as usize];
            // A little tone across each fragment, so no two faces match.
            let body = scale(body, (0.85 + 0.25 * hash01(pebble.key, 3, seed)) * pebble.shade * (1.0 + 0.12 * perlin(u, v, 70, seed ^ 0x53)));
            lerp(px, body, pebble.cover)
        });
    }

    // Only tile-scale drift is flattened; anything finer is the ground.
    c.equalize(n / 4);
    c.grain(0.04, s ^ 0x70);
    c
}
