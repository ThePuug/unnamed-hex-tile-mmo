//! Fractured stone for cliff faces and skirts. Averages to `CLIFF_COLOR` in
//! `terrain.wgsl`. The face is broken into fragments with anywhere from
//! three to seven cracked edges, a few large slabs dominating, some of them
//! shattered into elongated shards. Cracks fade out along their length, so
//! the network never closes into a tessellation, which is what reads as
//! paving. `v` is up when the tile is projected onto a face.

use crate::canvas::Canvas;
use crate::color::{lerp, luminance, scale, smoothstep, Rgb};
use crate::noise::{fbm, hash01, id01, perlin, worley, worley_xy, Cell};
use crate::textures::Params;

/// terrain.wgsl CLIFF_COLOR, linear RGB.
const CLIFF: Rgb = [0.35, 0.32, 0.28];
const DARK: Rgb = [0.08, 0.075, 0.07];
const LIGHT: Rgb = [0.56, 0.54, 0.50];
const IRON: Rgb = [0.36, 0.26, 0.17];
const COOL: Rgb = [0.27, 0.29, 0.32];
const MOSS: Rgb = [0.15, 0.20, 0.07];

/// Slabs across and down the tile, and shards along and across their
/// grain inside a shattered slab. Square slab lattice: a wide one leaves
/// the boundaries between its rows running the width of the tile as
/// courses.
const SLABS: u32 = 3;
const SHARDS: (u32, u32) = (6, 11);
/// Share of slabs that are shattered.
const SHATTERED: f32 = 0.4;

/// One fragment edge as seen from a pixel: how far it is, and the edge's
/// own width and depth, which are the same from either side.
struct Edge {
    dist: f32,
    width: f32,
    depth: f32,
}

/// Creases where gradient noise crosses zero, in [0, 1].
fn ridge(u: f32, v: f32, cells: u32, seed: u64) -> f32 {
    (1.0 - perlin(u, v, cells, seed).abs()).max(0.0)
}

/// `width` and `depth` are the ranges an edge of this class draws from:
/// master cracks between slabs run wide and deep, shard cracks stay
/// hairlines.
fn edge_of(cell: &Cell, cells_u: u32, seed: u64, width: (f32, f32), depth: (f32, f32)) -> Edge {
    let e = cell.id ^ cell.id2;
    Edge {
        // Half the gap between the two nearest points is how far the
        // boundary lies, in tile units.
        dist: (cell.f2 - cell.f1) * 0.5 / cells_u as f32,
        width: width.0 + (width.1 - width.0) * id01(e.wrapping_mul(seed | 1)),
        depth: depth.0 + (depth.1 - depth.0) * id01(e.rotate_left(17).wrapping_mul(seed | 1)),
    }
}

pub fn build(p: &Params) -> Canvas {
    let n = p.size as usize;
    let s = p.seed;
    let mut c = Canvas::filled(n, n, CLIFF);

    // Fragments: slabs from one Voronoi, shards from a finer, elongated one
    // inside the slabs that are shattered, lying along or across the tile
    // by the slab. A small warp bends the straight edges without making
    // them wavy, and each edge is ragged along its length. Returns the
    // nearest edge, the fragment's own hash, and the offset from its
    // centre.
    let fragment_of = |u: f32, v: f32| {
        let wu = u + 0.02 * fbm(u, v, 6, 3, 0.6, s ^ 0x10);
        let wv = v + 0.02 * fbm(u, v, 6, 3, 0.6, s ^ 0x11);
        let ragged = 0.7 + 0.6 * perlin(u, v, 48, s ^ 0x16);
        let slab = worley(wu, wv, SLABS, s ^ 0x12);
        let mut slab_edge = edge_of(&slab, SLABS, s ^ 0x13, (0.012, 0.035), (0.7, 1.0));
        slab_edge.dist *= ragged;
        let roll = id01(slab.id.rotate_left(7));
        if roll < SHATTERED {
            let (cu, cv) = if roll < SHATTERED * 0.5 { SHARDS } else { (SHARDS.1, SHARDS.0) };
            let shard = worley_xy(wu, wv, cu, cv, s ^ 0x14);
            let mut shard_edge = edge_of(&shard, cu.max(cv), s ^ 0x15, (0.005, 0.012), (0.4, 0.7));
            shard_edge.dist *= ragged;
            let edge = if shard_edge.dist < slab_edge.dist { shard_edge } else { slab_edge };
            (edge, id01(shard.id), (shard.dx / cu as f32, shard.dy / cv as f32))
        } else {
            (slab_edge, id01(slab.id), (slab.dx / SLABS as f32, slab.dy / SLABS as f32))
        }
    };

    // Cracks fade out along their length: where this drops, an edge is
    // solid rock and the two fragments read as one. Because tone and level
    // vary across the face rather than per fragment, nothing else marks
    // the vanished edge.
    let open = |u: f32, v: f32| smoothstep(-0.6, -0.15, perlin(u, v, 5, s ^ 0x17));

    // Spalls: patches where the face has broken away, sunk and rougher.
    let spall = |u: f32, v: f32| smoothstep(0.45, 0.65, fbm(u, v, 5, 3, 0.5, s ^ 0x33));

    // Height: fragments stand at levels that drift across the face, with a
    // step of their own that fades wherever the crack around them fades,
    // leaning their own way, with facets, grain and spalls. Deep cracks
    // get a wider bevel, so their upper edge catches light as a lip. Shaded
    // as if lit from above and a little from the left, so the face reads
    // as relief before the engine lights it.
    let height = |u: f32, v: f32| {
        let (edge, id, (dx, dy)) = fragment_of(u, v);
        let key = (id * 4096.0) as i64;
        let open = open(u, v);
        let lean = (hash01(key, 1, s) - 0.5) * dx + (hash01(key, 2, s) - 0.5) * dy;
        let level = 0.8 + 0.5 * fbm(u, v, 3, 2, 0.5, s ^ 0x37) + 0.4 * (hash01(key, 5, s) - 0.5) * open;
        let bevel = edge.width * (0.8 + 1.2 * smoothstep(0.6, 0.9, edge.depth));
        let gap = 1.0 - (1.0 - smoothstep(0.0, bevel, edge.dist)) * open;
        let slab = gap * (level + 2.0 * lean);
        let facet = ridge(u, v, 6, s ^ 0x36) * smoothstep(0.4, 0.7, hash01(key, 6, s));
        0.5 * slab + 0.2 * facet + 0.25 * fbm(u, v, 12, 5, 0.6, s ^ 0x30) - 0.35 * spall(u, v)
    };
    let e = 1.0 / n as f32;

    c.map(|u, v, px| {
        let (edge, id, _) = fragment_of(u, v);
        let key = (id * 4096.0) as i64;
        let open = open(u, v);

        // Tone drifts across the face, cool grey to iron-stained, and each
        // fragment shifts its own way where a crack still bounds it.
        let drift = fbm(u, v, 3, 3, 0.5, s ^ 0x38);
        let mut px = lerp(px, if drift < 0.0 { COOL } else { IRON }, drift.abs() * 0.6);
        px = scale(px, 0.7 + 0.6 * smoothstep(-0.6, 0.6, fbm(u, v, 7, 3, 0.5, s ^ 0x39)));
        px = scale(px, 1.0 + 0.3 * (hash01(key, 3, s) - 0.5) * open);

        // Mineral flecks: sparse bright specks in the grain.
        let fleck = worley(u, v, 48, s ^ 0x32);
        let flecks = (1.0 - smoothstep(0.08, 0.2, fleck.f1)) * smoothstep(0.96, 0.98, hash01((fleck.id >> 8) as i64, 0, s));
        px = lerp(px, LIGHT, flecks * 0.6);

        // Weathering: water runs down the face in streaks.
        let streak = smoothstep(0.35, 0.8, perlin(u, 0.11, 18, s ^ 0x34)) * (0.5 + 0.5 * perlin(u, v, 3, s ^ 0x35));
        px = lerp(scale(px, 1.0 - 0.25 * streak), IRON, 0.2 * streak);
        px = scale(px, 1.0 - 0.15 * spall(u, v));

        // Cracks: each edge has its own width and depth, wavering along its
        // length and fading out where the rock is solid; the deep ones go
        // black, the shallow ones stay hairlines.
        let along = 0.6 + 0.8 * perlin(u, v, 14, s ^ 0x24);
        let width = edge.width * along;
        let crack = (1.0 - smoothstep(width * 0.5, width, edge.dist)) * edge.depth * open;
        // Dirt gathers in a wider band along the deep cracks.
        let dirt = (1.0 - smoothstep(width, width * 3.0, edge.dist)) * smoothstep(0.6, 0.9, edge.depth) * open;
        px = scale(px, 1.0 - 0.2 * dirt);
        px = lerp(px, DARK, crack * 0.92);

        // Slopes facing up and left catch the light; the steps between
        // fragments are steep enough to clip, which is the lip and the
        // shadow.
        let shade = ((height(u, v + e) - height(u, v - e)) * 0.8 + (height(u - e, v) - height(u + e, v)) * 0.4) / (2.0 * e) * 0.03;
        px = scale(px, 1.0 + shade.clamp(-0.6, 0.6));
        px = lerp(px, LIGHT, smoothstep(0.7, 1.1, height(u, v)) * 0.25);

        // Moss as small cushions inside the deeper open cracks where the
        // face stays damp, each in a darker halo.
        let damp = smoothstep(0.1, 0.45, fbm(u, v, 3, 3, 0.5, s ^ 0x40));
        let cushion = worley(u, v, 8, s ^ 0x42);
        let blob = 1.0 - smoothstep(0.25, 0.6, cushion.f1 + 0.15 * perlin(u, v, 40, s ^ 0x43));
        let wet = damp * blob * smoothstep(0.3, 0.8, crack);
        px = scale(px, 1.0 - 0.3 * smoothstep(0.0, 0.5, wet));
        lerp(px, MOSS, wet * 0.9)
    });

    // Hairline fractures that run across a fragment without splitting it.
    let fracture = |c: &mut Canvas, i: i64, len: f32, alpha: f32| {
        let r = |k: i64| hash01(i, k, s ^ 0x50);
        let (mut x, mut y) = (r(0) * n as f32, r(1) * n as f32);
        let mut angle = r(2) * std::f32::consts::PI;
        let steps = 5;
        for k in 0..steps {
            angle += (r(10 + k) - 0.5) * 0.9;
            let step = len / steps as f32;
            let (nx, ny) = (x + angle.cos() * step, y + angle.sin() * step);
            let a = alpha * (0.6 + 0.4 * r(20 + k));
            c.line(x as i64, y as i64, nx as i64, ny as i64, DARK, a);
            c.line(x as i64, y as i64 + 1, nx as i64, ny as i64 + 1, LIGHT, a * 0.4);
            x = nx;
            y = ny;
        }
    };
    for i in 0..10 {
        fracture(&mut c, i, n as f32 * (0.06 + 0.14 * hash01(i, 9, s)), 0.5);
    }

    // Grit before equalize so the finest detail survives the mean pin.
    c.map(|u, v, px| scale(px, 1.0 + 0.12 * perlin(u, v, 128, s ^ 0x60)));

    // Only tile-scale drift is flattened; anything finer is the stone.
    c.equalize(n / 4);
    c.set_mean_luminance(luminance(CLIFF));
    c.grain(0.04, s ^ 0x70);
    c
}
