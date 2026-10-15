//! Mesh region assembly on the terrain surface — one builder for every LoD
//! level.

//! A region is a radius-9 hex ball of lattice cells (271) at the level's
//! scale, one mesh each. At r = 0 the lattice is the tile grid and a region
//! is a chunk; above it the cells are summaries. Every cell is a fan from its
//! centre to six corners shared with its neighbours (`crate::surface`), so
//! adjacent cells — and adjacent regions, which compute the same corners
//! from the same heights — meet without seams. The only extra geometry is a
//! curtain under each edge that faces a cell this mesh did not build: it
//! closes the horizon and unbuilt ground, and once the neighbour exists it
//! hangs under a closed surface where nothing can see it.

//! Every vertex also carries the coarser level's surface at its position —
//! height and normal, as that level draws them — so the renderer can morph
//! the finer surface onto the coarser one across the transition strip and
//! the two meet without a step at the cut.

use std::collections::{HashMap, HashSet};

use bevy::math::{Vec2, Vec3, Vec3Swizzles};
use common::cover::{Canopy, Content};
use common::summary::{SummaryCell, PARTS};

use crate::{
    chunk::{self, ChunkId},
    geometry::{flat_top_tile_center, hex_ball_tiles},
    summary::{
        Band, SummaryLattice, canonical_vertex_id, coarser_level, level_depth_bias,
        mesh_region_lattice, summary_lattice,
    },
    surface::{
        CORNER_NEIGHBOURS, centre_normal, corner_normal, corner_offsets, fan_weights, own_corner_z,
        height_y,
    },
};

/// Key identifying a mesh region within a specific distance band.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct MeshRegionKey {
    /// Summary radius for this band.
    pub r: u32,
    /// Mesh region lattice coordinates (in summary-lattice space).
    pub mn: i32,
    pub mm: i32,
}

impl MeshRegionKey {
    /// The tile the region is built and placed about: its centre cell's
    /// centre, at `z = 0`.
    pub fn origin(&self) -> qrz::Qrz {
        let (q, r) = summary_lattice(self.r).cell_center(mesh_region_lattice().cell_center((self.mn, self.mm)));
        qrz::Qrz { q, r, z: 0 }
    }
}

/// Result of building a mesh region.
pub struct SummaryMeshResult {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// Per vertex, the coarser level's surface at it, in this mesh's frame:
    /// normal xyz, height w. The vertex's own normal and height at the
    /// coarsest level, which has nothing to morph onto.
    pub coarse: Vec<[f32; 4]>,
    /// Per vertex, the canopy over it as [`canopy_vertex`] states it, a
    /// corner's the mean of the three cells meeting there; empty when the
    /// level was built without one.
    pub canopy: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
    pub tri_count: u32,
}

/// A summary's canopy as a vertex carries it: over its nine parts, the
/// mean density, then the density's pine, deciduous and brush parts,
/// which sum to it. Parts interpolate across a fan the way a premultiplied
/// colour does — a wood's edge against bare ground thins without shifting
/// hue — where shares would fade twice and counts would not blend at all.
pub fn canopy_vertex(parts: &[Canopy; PARTS]) -> [f32; 4] {
    let whole = common::cover::CANOPY_WHOLE as f32 * PARTS as f32;
    let share = |kind: Content| parts.iter().map(|p| p.share(kind) as f32).sum::<f32>() / whole;
    let density = parts.iter().map(|p| p.filled().min(common::cover::CANOPY_WHOLE) as f32).sum::<f32>() / whole;
    [density, share(Content::Pine), share(Content::Deciduous), share(Content::Brush)]
}

/// The density from which a canopy reads closed: its colour lies over the
/// ground in full. The terrain shaders read it from the level's material.
pub const CANOPY_FULL: f32 = 0.33;

/// Cells in a mesh region: the hex ball of `MESH_REGION_RADIUS`.
pub const MESH_REGION_CELLS: u32 = hex_ball_tiles(crate::summary::MESH_REGION_RADIUS);

/// Downward curtain depth (WU) under edges facing unbuilt cells. Deep enough
/// to cover the relief between a built region and the ground beside it.
pub const CURTAIN_DEPTH_WU: f32 = 24.0;

/// A cell's fan as its level draws it: the centre in XZ from the centre of
/// the frame's tile, centre and corner heights in world Y before the
/// level's depth bias, and the vertex normals. Corners are the cell's own
/// by [`own_corner_z`] at the level's width.
pub struct Fan {
    pub centre: Vec2,
    pub centre_y: f32,
    pub corner_y: [f32; 6],
    pub centre_n: Vec3,
    pub corner_n: [Vec3; 6],
}

/// The fan of `cell` on `lattice` from a height lookup over it, in the
/// frame of tile `frame`: every centre is taken as a tile difference from
/// it before it becomes a vector, so the fan is exact however far out it
/// is. None while the cell or one of its six neighbours is absent: a
/// corner with a cell missing would not be the corner the neighbour draws.
pub fn cell_fan(
    lattice: &SummaryLattice,
    cell: (i32, i32),
    frame: qrz::Qrz,
    height: impl Fn(i32, i32) -> Option<i32>,
) -> Option<Fan> {
    let offsets = corner_offsets(lattice.scale as f32);
    let centre_of = |c: (i32, i32)| -> Vec2 {
        let (cq, cr) = lattice.cell_center(c);
        let (wx, wz) = flat_top_tile_center(cq - frame.q, cr - frame.r, 1.0);
        Vec2::new(wx, wz)
    };
    let centre_3d = |c: (i32, i32)| -> Option<Vec3> {
        let p = centre_of(c);
        Some(Vec3::new(p.x, height_y(height(c.0, c.1)? as f32), p.y))
    };
    let z = height(cell.0, cell.1)?;
    let centre = centre_of(cell);
    let centre_y = height_y(z as f32);
    let mut corner_y = [0.0; 6];
    let mut corner_n = [Vec3::Y; 6];
    let mut corner_pos = [Vec3::ZERO; 6];
    for i in 0..6 {
        let [a, b] = CORNER_NEIGHBOURS[i];
        let na = (cell.0 + a.0, cell.1 + a.1);
        let nb = (cell.0 + b.0, cell.1 + b.1);
        let cells = [Some(z), Some(height(na.0, na.1)?), Some(height(nb.0, nb.1)?)];
        let cz = own_corner_z(cells, lattice.scale)?;
        corner_y[i] = height_y(cz);
        corner_pos[i] = Vec3::new(centre.x + offsets[i].x, corner_y[i], centre.y + offsets[i].y);
        corner_n[i] = corner_normal([centre_3d(cell), centre_3d(na), centre_3d(nb)]);
    }
    let centre_n = centre_normal(Vec3::new(centre.x, centre_y, centre.y), corner_pos);
    Some(Fan { centre, centre_y, corner_y, centre_n, corner_n })
}

/// The coarser level's surface, read at the finer level's vertices: the
/// fan under the point, its height and vertex normals interpolated by the
/// fan's own weights, so a vertex morphed onto it lands where that level's
/// triangles are drawn. Fans are read once per cell across a build.
/// A level's drawn surface from a height lookup over its lattice, its fans
/// made as they are read: what a vertex morphs toward at the level below,
/// and what stands on the level at its own.
/// Read in the frame of a tile, as the region's own vertices are.
pub struct LevelSurface<'a> {
    lattice: SummaryLattice,
    frame: qrz::Qrz,
    height: &'a dyn Fn(i32, i32) -> Option<i32>,
    fans: HashMap<(i32, i32), Option<Fan>>,
}

impl<'a> LevelSurface<'a> {
    pub fn new(radius: u32, frame: qrz::Qrz, height: &'a dyn Fn(i32, i32) -> Option<i32>) -> Self {
        LevelSurface { lattice: summary_lattice(radius), frame, height, fans: HashMap::new() }
    }

    /// Height (world Y, before any bias) and normal at `xz` from the
    /// centre of the frame's tile. None while a cell the fan reads is
    /// absent.
    pub fn at(&mut self, xz: Vec2) -> Option<(f32, Vec3)> {
        let cell = self.lattice.cell_from((self.frame.q, self.frame.r), xz);
        let (lattice, frame, height) = (&self.lattice, self.frame, self.height);
        let fan = self
            .fans
            .entry(cell)
            .or_insert_with(|| cell_fan(lattice, cell, frame, height))
            .as_ref()?;
        let ((i, j), w) = fan_weights(xz, fan.centre, lattice.scale as f32);
        let y = w.x * fan.centre_y + w.y * fan.corner_y[i] + w.z * fan.corner_y[j];
        let n = w.x * fan.centre_n + w.y * fan.corner_n[i] + w.z * fan.corner_n[j];
        Some((y, n.normalize_or(Vec3::Y)))
    }
}

/// Build a mesh region at `radius` from a height lookup over that level's
/// lattice: `height(sq, sr)` is the cell's z, or None while its data is
/// absent. At r = 0 the lattice coordinates are tile coordinates. `coarse`
/// is the same lookup over the next coarser level, which every vertex's
/// morph target is read from; None at the coarsest level.

/// `canopy`, where given, is the same lookup for the canopy over a cell,
/// carried per vertex: a level that colours its ground by what stands on
/// it rather than standing it.
///
/// Built only once every cell of the region and of the ring around it has a
/// height — the ring is what the perimeter corners are made of — and every
/// coarser cell under a vertex and around it has one, and then final:
/// heights are durable. A canopy is not, since players fell what its
/// samples hold, and the caller builds a region again when one it read
/// is revised.
/// Returns None until then. Producers cover one region ring more than
/// consumers build, and the coarser level over every band
/// (`visible_lod_regions`), so what a needed region waits on always arrives.
pub fn build_summary_mesh_region(
    radius: u32,
    region_key: MeshRegionKey,
    height: &dyn Fn(i32, i32) -> Option<i32>,
    coarse: Option<&dyn Fn(i32, i32) -> Option<i32>>,
    canopy: Option<&dyn Fn(i32, i32) -> Option<[Canopy; PARTS]>>,
) -> Option<SummaryMeshResult> {
    let lattice = summary_lattice(radius);
    let region_lat = mesh_region_lattice();
    let region_id = (region_key.mn, region_key.mm);

    let origin = region_key.origin();

    // Heights for the region and its ring — the ring only feeds corners.
    let region_cells: Vec<(i32, i32)> = region_lat.tiles_in_cell(region_id).collect();
    let region_set: HashSet<(i32, i32)> = region_cells.iter().copied().collect();
    let mut heights: HashMap<(i32, i32), i32> = HashMap::new();
    let mut canopies: HashMap<(i32, i32), [f32; 4]> = HashMap::new();
    for &(sq, sr) in &region_cells {
        for (dq, dr) in [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1), (1, -1), (-1, 1)] {
            let cell = (sq + dq, sr + dr);
            if heights.contains_key(&cell) {
                continue;
            }
            heights.insert(cell, height(cell.0, cell.1)?);
            if let Some(canopy) = canopy {
                canopies.insert(cell, canopy_vertex(&canopy(cell.0, cell.1)?));
            }
        }
    }

    let bias = level_depth_bias(radius);
    let offsets = corner_offsets(lattice.scale as f32);
    let mut coarse = coarse.map(|height| {
        LevelSurface::new(coarser_level(radius).expect("a level with a coarser one"), origin, height)
    });
    // The morph target of a vertex at `p` with normal `n`: the coarser
    // surface there in this level's frame, or the vertex itself.
    let mut target = |p: Vec3, n: Vec3| -> Option<(f32, Vec3)> {
        match coarse.as_mut() {
            Some(c) => c.at(p.xz()).map(|(y, n)| (y - bias, n)),
            None => Some((p.y, n)),
        }
    };

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut coarse_attr: Vec<[f32; 4]> = Vec::new();
    let mut canopy_attr: Vec<[f32; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    // A corner vertex is shared by the cells that hold the same height there:
    // all of them, but where the corner rule leaves a face.
    let mut corner_index: HashMap<((i32, i32), u32), u32> = HashMap::new();
    let span = lattice.scale as f64;

    let mut push = |p: Vec3, n: Vec3, target: (f32, Vec3), canopy: [f32; 4]| -> u32 {
        let (ty, tn) = target;
        positions.push([p.x, p.y, p.z]);
        normals.push([n.x, n.y, n.z]);
        coarse_attr.push([tn.x, tn.y, tn.z, ty]);
        if !canopies.is_empty() {
            canopy_attr.push(canopy);
        }
        (positions.len() - 1) as u32
    };
    let canopy_of = |cell: (i32, i32)| canopies.get(&cell).copied().unwrap_or([0.0; 4]);

    for &cell in &region_cells {
        let fan = cell_fan(&lattice, cell, origin, |q, r| heights.get(&(q, r)).copied())
            .expect("region and ring heights are present");
        let centre_canopy = canopy_of(cell);

        let mut corner_pos = [Vec3::ZERO; 6];
        let mut corner_target = [(0.0, Vec3::Y); 6];
        let mut corner_idx = [0u32; 6];
        for i in 0..6 {
            let p = Vec3::new(fan.centre.x + offsets[i].x, fan.corner_y[i] - bias, fan.centre.y + offsets[i].y);
            corner_pos[i] = p;
            corner_target[i] = target(p, fan.corner_n[i])?;
            let id = (canonical_vertex_id(cell.0, cell.1, i), fan.corner_y[i].to_bits());
            corner_idx[i] = *corner_index.entry(id).or_insert_with(|| {
                let [a, b] = CORNER_NEIGHBOURS[i];
                let (ca, cb) = (canopy_of((cell.0 + a.0, cell.1 + a.1)), canopy_of((cell.0 + b.0, cell.1 + b.1)));
                let corner_canopy = std::array::from_fn(|c| (centre_canopy[c] + ca[c] + cb[c]) / 3.0);
                push(p, fan.corner_n[i], corner_target[i], corner_canopy)
            });
        }

        let centre_pos = Vec3::new(fan.centre.x, fan.centre_y - bias, fan.centre.y);
        let ci = push(centre_pos, fan.centre_n, target(centre_pos, fan.centre_n)?, centre_canopy);
        for i in 0..6 {
            let j = (i + 1) % 6;
            indices.extend([ci, corner_idx[j], corner_idx[i]]);
        }

        // Faces: down each edge to a neighbour whose own corners there stand
        // lower, a wall from this cell's corners to the neighbour's.
        {
            let z = heights[&cell];
            for i in 0..6 {
                let j = (i + 1) % 6;
                let d = CORNER_NEIGHBOURS[i][0];
                let facing = (cell.0 + d.0, cell.1 + d.1);
                let (Some(&zd), Some(&zi), Some(&zj)) = (
                    heights.get(&facing),
                    heights.get(&(cell.0 + CORNER_NEIGHBOURS[i][1].0, cell.1 + CORNER_NEIGHBOURS[i][1].1)),
                    heights.get(&(cell.0 + CORNER_NEIGHBOURS[j][0].0, cell.1 + CORNER_NEIGHBOURS[j][0].1)),
                ) else {
                    continue;
                };
                // The neighbour's own heights at this edge's two corners: it
                // is the first sharer of corner i and the second of corner j.
                let low_i = common::corner::meet([z as f64, zd as f64, zi as f64], span)[1] as f32;
                let low_j = common::corner::meet([z as f64, zj as f64, zd as f64], span)[2] as f32;
                let (yi, yj) = (height_y(low_i) - bias, height_y(low_j) - bias);
                let (top0, top1) = (corner_pos[i], corner_pos[j]);
                if top0.y <= yi + 1e-4 && top1.y <= yj + 1e-4 {
                    continue;
                }
                let (bot0, bot1) = (Vec3::new(top0.x, yi.min(top0.y), top0.z), Vec3::new(top1.x, yj.min(top1.y), top1.z));
                let outward = (top1 - top0).normalize_or_zero().cross(Vec3::NEG_Y).normalize_or_zero();
                let n = if outward.length_squared() > 0.5 { outward } else { Vec3::Z };
                let base = push(top0, n, target(top0, n)?, centre_canopy);
                push(top1, n, target(top1, n)?, centre_canopy);
                push(bot1, n, target(bot1, n)?, centre_canopy);
                push(bot0, n, target(bot0, n)?, centre_canopy);
                indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
            }
        }

        // Curtains under edges facing the ring: cells this mesh does not build.
        // (i, i+1) faces the first neighbour listed for corner i. A curtain
        // hangs from its corners' targets too, so it follows a morphed edge.
        for i in 0..6 {
            let d = CORNER_NEIGHBOURS[i][0];
            let facing = (cell.0 + d.0, cell.1 + d.1);
            if region_set.contains(&facing) {
                continue;
            }
            let j = (i + 1) % 6;
            let (top0, top1) = (corner_pos[i], corner_pos[j]);
            let (bot0, bot1) = (top0 - Vec3::Y * CURTAIN_DEPTH_WU, top1 - Vec3::Y * CURTAIN_DEPTH_WU);
            let outward = (top1 - top0).normalize_or_zero().cross(Vec3::NEG_Y).normalize_or_zero();
            let n = if outward.length_squared() > 0.5 { outward } else { Vec3::Z };
            let (ty0, ty1) = (corner_target[i].0, corner_target[j].0);
            let base = push(top0, n, (ty0, n), [0.0; 4]);
            push(top1, n, (ty1, n), [0.0; 4]);
            push(bot1, n, (ty1 - CURTAIN_DEPTH_WU, n), [0.0; 4]);
            push(bot0, n, (ty0 - CURTAIN_DEPTH_WU, n), [0.0; 4]);
            indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }

    Some(SummaryMeshResult {
        tri_count: indices.len() as u32 / 3,
        positions,
        normals,
        coarse: coarse_attr,
        canopy: canopy_attr,
        indices,
    })
}

/// How far a region's parts reach from its centre along either axis of the
/// finer lattice: the region's cells and the ring around them, a width a
/// cell, and a part's own offset, two finer steps at most.
pub const PARTS_LAYER_HALF: i32 = 3 * (crate::summary::MESH_REGION_RADIUS as i32 + 1) + 2;

/// The side of a region's parts layer, in texels.
pub const PARTS_LAYER_SIDE: u32 = 2 * PARTS_LAYER_HALF as u32 + 1;

/// The bit of a part's texel set where the part stands under water, above
/// its canopy's, which take fifteen.
pub const PART_WET: u16 = 1 << 15;

/// A region's parts as the terrain shader reads them: a square of
/// [`PARTS_LAYER_SIDE`] texels, row by row, one to a point of the finer
/// lattice, with the region's centre tile at the middle. Every part of the
/// region's cells and of the ring around them holds its canopy's bits and
/// [`PART_WET`] at the point its tile stands on, where the axial offset
/// from the centre, over the finer step, is the texel's column and row off
/// the middle; every other texel is bare and dry. The ring is there
/// because a fragment at the region's edge reads the parts of the cell
/// across it. None until every one of those cells is there, and at a
/// radius whose width does not divide into thirds, whose parts stand on no
/// finer lattice. The last level wears its wood but never its water, which
/// is narrower than a pixel there.
pub fn parts_layer(
    radius: u32,
    region_key: MeshRegionKey,
    cells: &dyn Fn(i32, i32) -> Option<SummaryCell>,
) -> Option<Vec<u16>> {
    let lattice = summary_lattice(radius);
    if lattice.scale % 3 != 0 {
        return None;
    }
    let step = lattice.scale / 3;
    let wears_water = coarser_level(radius).is_some();
    let centre = mesh_region_lattice().cell_center((region_key.mn, region_key.mm));
    let (cq, cr) = lattice.cell_center(centre);
    let side = PARTS_LAYER_SIDE as i32;
    let mut layer = vec![0u16; (side * side) as usize];
    let reach = crate::summary::MESH_REGION_RADIUS as i32 + 1;
    for dq in -reach..=reach {
        for dr in (-reach).max(-dq - reach)..=reach.min(-dq + reach) {
            let cell = (centre.0 + dq, centre.1 + dr);
            let summary = cells(cell.0, cell.1)?;
            let (tq, tr) = lattice.cell_center(cell);
            for (part, (oq, or)) in common::summary::part_offsets(radius).into_iter().enumerate() {
                let column = (tq - cq + oq) / step + PARTS_LAYER_HALF;
                let row = (tr - cr + or) / step + PARTS_LAYER_HALF;
                let wet = if wears_water && summary.wet & (1 << part) != 0 { PART_WET } else { 0 };
                layer[(row * side + column) as usize] = summary.canopy[part].bits() | wet;
            }
        }
    }
    Some(layer)
}

/// How far beyond its corners a water face lands where the water drops past
/// its edge, in cells: off the plane of the ground's own face under that
/// edge, so the two never fight for the same pixels.
pub const DROP_OFFSET: f32 = 0.05;

/// Build the water of a mesh region at `radius`: a flat hexagon at its
/// surface over every cell with water, and a face down each edge where what
/// lies beyond is lower than the surface, the neighbour's water or, where
/// it is dry, its ground: the surface carried out over the edge to a foot
/// [`DROP_OFFSET`] of a cell beyond each corner, toward the lower cells
/// about it, then straight down to that, so it stands in front of the
/// ground's face there, never in its plane, and every drop at one corner,
/// from whichever cell, hangs on the one vertical line at its foot. Where the ground beyond stands
/// at or over the surface it is the shore, and hides the water's edge
/// itself. Every face shades as the surface does, facing up.
/// `water(sq, sr)` is the surface over a cell as a z-level, `ground(sq, sr)`
/// its height, each None where unknown, and water None where the cell is
/// dry. Water at zero is not built: the sea is one plane drawn to the
/// horizon. Sinks by the level's depth bias with the ground, and is empty
/// where nothing is flooded.
pub fn build_water_mesh_region(
    radius: u32,
    region_key: MeshRegionKey,
    water: &dyn Fn(i32, i32) -> Option<i32>,
    ground: &dyn Fn(i32, i32) -> Option<i32>,
) -> SummaryMeshResult {
    let lattice = summary_lattice(radius);
    let region_lat = mesh_region_lattice();
    let region_id = (region_key.mn, region_key.mm);

    let origin = region_key.origin();

    let bias = level_depth_bias(radius);
    let offsets = corner_offsets(lattice.scale as f32);
    // A surface at a step lies between that step's ground and the one
    // below it: the tiles below the step are under it, the tiles at it dry.
    let surface_y = |w: i32| height_y(w as f32 - 0.5) - bias;
    let offset = DROP_OFFSET * lattice.scale as f32 * 3.0_f32.sqrt();

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let push = |positions: &mut Vec<[f32; 3]>, normals: &mut Vec<[f32; 3]>, p: Vec3| -> u32 {
        positions.push([p.x, p.y, p.z]);
        normals.push([0.0, 1.0, 0.0]);
        (positions.len() - 1) as u32
    };

    for cell in region_lat.tiles_in_cell(region_id) {
        let Some(w) = water(cell.0, cell.1).filter(|&w| w > 0) else { continue };
        let (cq, cr) = lattice.cell_center(cell);
        let (wx, wz) = flat_top_tile_center(cq - origin.q, cr - origin.r, 1.0);
        let y = surface_y(w);
        let ci = push(&mut positions, &mut normals, Vec3::new(wx, y, wz));
        let corners = offsets.map(|o| Vec3::new(wx + o.x, y, wz + o.y));
        let mut idx = [0u32; 6];
        for i in 0..6 {
            idx[i] = push(&mut positions, &mut normals, corners[i]);
        }
        for i in 0..6 {
            let j = (i + 1) % 6;
            indices.extend([ci, idx[j], idx[i]]);
        }

        // The cells about corner k: this one, then the two it shares it with.
        let about = |k: usize| -> [(i32, i32); 3] {
            let [a, b] = CORNER_NEIGHBOURS[k];
            [cell, (cell.0 + a.0, cell.1 + a.1), (cell.0 + b.0, cell.1 + b.1)]
        };
        // A cell's top at a corner it shares with `others`, as it is drawn
        // there: its water's surface where it is wet, level to its corners;
        // else its ground's corner by the corner rule, as the ground's mesh
        // places it; over everything where unknown.
        let top_at = |at: (i32, i32), others: [(i32, i32); 2]| -> f32 {
            if let Some(nw) = water(at.0, at.1) {
                return surface_y(nw.max(0));
            }
            let cells = [ground(at.0, at.1), ground(others[0].0, others[0].1), ground(others[1].0, others[1].1)];
            match cells {
                [Some(_), Some(_), Some(_)] => own_corner_z(cells, lattice.scale).map_or(f32::INFINITY, |z| height_y(z) - bias),
                _ => f32::INFINITY,
            }
        };
        // The three cells about corner k with their tops there, lowest
        // first, ties by position, alike from each of them.
        let ranked = |k: usize| -> [((i32, i32), f32); 3] {
            let [a, b, c] = about(k);
            let mut tops = [(a, top_at(a, [b, c])), (b, top_at(b, [a, c])), (c, top_at(c, [a, b]))];
            tops.sort_by(|x, y| x.1.total_cmp(&y.1).then(x.0.cmp(&y.0)));
            tops
        };
        // Where every drop at corner k lands, from whichever cell: beyond the
        // corner toward a quarter of the way from its lowest cell's centre to
        // its middle one's, by their tops there. That lies on the lower side
        // of every edge meeting there that water drops over.
        let foot = |k: usize| -> Vec3 {
            let [low, mid, _] = ranked(k);
            let centre = |at: (i32, i32)| {
                let (nq, nr) = lattice.cell_center(at);
                let (x, z) = flat_top_tile_center(nq - origin.q, nr - origin.r, 1.0);
                Vec2::new(x, z)
            };
            let toward = (centre(mid.0) + 3.0 * centre(low.0)) / 4.0 - corners[k].xz();
            let step = toward.normalize_or_zero() * offset;
            Vec3::new(corners[k].x + step.x, 0.0, corners[k].z + step.y)
        };
        let mut quad = |ps: [Vec3; 4]| {
            let base = positions.len() as u32;
            for p in ps {
                push(&mut positions, &mut normals, p);
            }
            indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        };
        // Edge (i, i+1) faces the neighbour listed first for corner i and
        // second for corner i+1. Where what that neighbour shows at either
        // corner lies under the surface, the surface is carried out over the
        // edge to the foot of each corner it drops at, then straight down to
        // what is drawn there, corner by corner, so drops meeting at a corner
        // hang on one line and each reaches the ground beyond it.
        for i in 0..6 {
            let j = (i + 1) % 6;
            let (ai, aj) = (about(i), about(j));
            let beyond = [top_at(ai[1], [cell, ai[2]]), top_at(aj[2], [cell, aj[1]])];
            if beyond.iter().all(|&t| t >= y) {
                continue;
            }
            let [low_i, low_j] = beyond.map(|t| t.min(y));
            let flat = |k: usize| Vec3::new(corners[k].x, 0.0, corners[k].z);
            let fi = if low_i < y { foot(i) } else { flat(i) };
            let fj = if low_j < y { foot(j) } else { flat(j) };
            let at = |f: Vec3, h: f32| f + Vec3::Y * h;
            quad([corners[i], corners[j], at(fj, y), at(fi, y)]);
            quad([at(fi, y), at(fj, y), at(fj, low_j), at(fi, low_i)]);
        }
    }

    // Water does not morph: it is drawn with a plain material and no cut.
    SummaryMeshResult {
        tri_count: indices.len() as u32 / 3,
        positions,
        normals,
        coarse: Vec::new(),
        canopy: Vec::new(),
        indices,
    }
}

/// The mesh regions of [`visible_mesh_regions_in_band_ungated`] that
/// overlap a loaded chunk: one whose centre summary, or one of the six
/// summaries round it, stands on a tile in `loaded_chunks`.
pub fn visible_mesh_regions_in_band(
    r: u32,
    camera_wx: f32,
    camera_wz: f32,
    inner_wu: f32,
    outer_wu: f32,
    loaded_chunks: &HashSet<ChunkId>,
) -> HashSet<MeshRegionKey> {
    let summary_lat = summary_lattice(r);
    let region_lat = mesh_region_lattice();
    let loaded = |cell: (i32, i32)| {
        let (cq, cr) = summary_lat.cell_center(cell);
        loaded_chunks.contains(&chunk::loc_to_chunk(qrz::Qrz { q: cq, r: cr, z: 0 }))
    };
    visible_mesh_regions_in_band_ungated(r, camera_wx, camera_wz, inner_wu, outer_wu)
        .into_iter()
        .filter(|key| {
            let centre = region_lat.cell_center((key.mn, key.mm));
            [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1), (1, -1), (-1, 1)]
                .into_iter()
                .any(|(dq, dr)| loaded((centre.0 + dq, centre.1 + dr)))
        })
        .collect()
}

/// Lattice steps a region search must reach from the camera's region to
/// hold every region whose centre lies within `outer_wu`: a region at k
/// steps is at least k · spacing · √3/2 away, the lattice's inradius
/// direction, and the camera stands anywhere in its own region.
fn search_steps(r: u32, outer_wu: f32) -> i32 {
    let step = crate::summary::mesh_region_spacing_wu(r) * 3.0_f32.sqrt() / 2.0;
    ((outer_wu / step).ceil() as i32 + 2).min(60)
}

/// Enumerate mesh regions within a distance band, whether or not their
/// tiles are loaded: those whose centres lie within `[inner_wu, outer_wu]`
/// of the camera at `camera_wx/wz` in the XZ plane. The remote summary
/// bands take this, their data coming from the server, not local tiles;
/// `visible_mesh_regions_in_band` gates it on the loaded chunks.
pub fn visible_mesh_regions_in_band_ungated(
    r: u32,
    camera_wx: f32,
    camera_wz: f32,
    inner_wu: f32,
    outer_wu: f32,
) -> HashSet<MeshRegionKey> {
    let summary_lat = summary_lattice(r);
    let region_lat = mesh_region_lattice();

    // Convert camera world position to tile coordinates using flat-top inverse,
    // then to summary-lattice coordinates, then to mesh-region lattice.
    // Flat-top: x = 1.5*q, z = sqrt(3)/2*q + sqrt(3)*r
    // Inverse: q = x/1.5, r = (z - sqrt(3)/2*q) / sqrt(3)
    let cam_q = camera_wx as f64 / 1.5;
    let cam_r = (camera_wz as f64 - cam_q * 3.0_f64.sqrt() / 2.0) / 3.0_f64.sqrt();
    let cam_sq = (cam_q / summary_lat.scale as f64).round() as i32;
    let cam_sr = (cam_r / summary_lat.scale as f64).round() as i32;
    let cam_region = region_lat.cell_id(cam_sq, cam_sr);

    let sr = search_steps(r, outer_wu);
    let mut regions = HashSet::new();
    for dn in -sr..=sr {
        let dm_min = (-sr).max(-dn - sr);
        let dm_max = sr.min(-dn + sr);
        for dm in dm_min..=dm_max {
            let mn = cam_region.0 + dn;
            let mm = cam_region.1 + dm;

            let region_center = region_lat.cell_center((mn, mm));
            let (scq, scr) = summary_lat.cell_center(region_center);
            let (rwx, rwz) = flat_top_tile_center(scq, scr, 1.0);
            let dx = rwx - camera_wx;
            let dz = rwz - camera_wz;
            let dist = (dx * dx + dz * dz).sqrt();

            if dist < inner_wu || dist > outer_wu {
                continue;
            }

            regions.insert(MeshRegionKey { r, mn, mm });
        }
    }

    regions
}

/// Whether every chunk a region stands on has been streamed. A region's
/// ground can come from the summaries the producers send, which reach
/// past the tiles, but the trees on it come from the map's own covers, so
/// a region built before its tiles arrived stands none. The disc is the
/// region's footprint rounded out to whole chunks, so it never calls a
/// region ready while a tile under it is still missing.
pub fn region_tiles_loaded(key: MeshRegionKey, loaded_chunks: &HashSet<ChunkId>) -> bool {
    use crate::summary::{mesh_region_extent_wu, summary_width_wu};
    let centre = chunk::loc_to_chunk(key.origin());
    let reach = mesh_region_extent_wu(key.r) / 3.0_f32.sqrt() + summary_width_wu(key.r);
    let radius = (reach / (chunk::CHUNK_EXTENT_WU * chunk::APOTHEM_FACTOR)).ceil() as u8;
    chunk::calculate_visible_chunks(centre, radius).iter().all(|c| loaded_chunks.contains(c))
}

/// Collect visible mesh regions across all bands for the server's producer:
/// everything beyond the local-data boundary, plus regions whose footprint
/// straddles it.

/// `local_boundary_wu` is the extent the consumer's Map can serve:
/// FIXED_STREAM_RADIUS_WU.

/// Straddling matters: a region centered just inside the boundary has
/// summaries beyond it whose tiles the Map can never resolve — the producer
/// covers those. Values agree with Map-computed ones because every producer
/// uses the same 7-sample rule over the same elevation field.

/// The consumer builds a region only once its ring has heights too, so the
/// producer covers one region ring more than the consumer needs at both
/// ends of every band — without it, the horizon shell and the regions just
/// inside the boundary would wait forever.
///
/// `margin_rings` widens every band's reach by that many region rings:
/// none for what is sent, one for what is kept, so a region sent at the
/// edge is not taken back as the player steps back over it.
pub fn visible_lod_regions(
    bands: &[Band],
    cam_wx: f32,
    cam_wz: f32,
    local_boundary_wu: f32,
    margin_rings: u32,
) -> HashSet<MeshRegionKey> {
    use crate::summary::{mesh_region_extent_wu, mesh_region_spacing_wu, summary_width_wu};
    let circum = |r: u32| mesh_region_extent_wu(r) / 3.0_f32.sqrt();
    let mut out = HashSet::new();
    for band in bands {
        let half_extent = 0.5 * mesh_region_extent_wu(band.r);
        let ring = mesh_region_spacing_wu(band.r);
        let margin = margin_rings as f32 * ring;
        // Every region whose footprint touches the band is produced, as
        // the consumer enumerates them: a region centred just outside an
        // edge still has summaries inside it, and membership by centre
        // alone would leave it to no band.
        let (win_inner, win_outer) = (band.inner_wu, band.outer_wu);
        let outer = win_outer + half_extent + ring + margin;
        // A band whose regions and rings cannot reach past the local
        // boundary is fully consumer-owned (Map-computed).
        if outer > local_boundary_wu {
            let inner = (win_inner - half_extent - margin).max(local_boundary_wu) - ring;
            out.extend(visible_mesh_regions_in_band_ungated(
                band.r, cam_wx, cam_wz, inner.max(0.0), outer,
            ));
        }

        // The coarser level over the band: a region built at this level
        // reads, for its morph targets, the coarser cell under each of its
        // vertices and that cell's ring. A region's centre lies up to half
        // its extent past the window and its vertices a circumradius past
        // that; the cell under a vertex reaches its outer radius past it,
        // its ring one summary more, and the region holding the cell a
        // circumradius again.
        let Some(c) = coarser_level(band.r) else { continue };
        let cell_reach = summary_lattice(c).scale as f32 + summary_width_wu(c);
        let reach = half_extent + circum(band.r) + cell_reach + circum(c) + margin;
        let outer_c = win_outer + reach;
        if outer_c <= local_boundary_wu { continue; }
        let inner_c = (win_inner - reach).max(local_boundary_wu) - mesh_region_spacing_wu(c);
        out.extend(visible_mesh_regions_in_band_ungated(
            c, cam_wx, cam_wz, inner_c.max(0.0), outer_c,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const REGION: MeshRegionKey = MeshRegionKey { r: 1, mn: 0, mm: 0 };

    /// Perimeter edges of a radius-R hex ball: 6(2R+1).
    const PERIMETER_EDGES: u32 = 6 * (2 * 9 + 1);

    /// Corner vertices of a radius-R hex ball: 6(R+1)².
    const CORNER_VERTICES: usize = 6 * 10 * 10;

    /// The world vector of a region's origin tile.
    fn origin_of(key: MeshRegionKey) -> Vec3 {
        let o = key.origin();
        let (x, z) = flat_top_tile_center(o.q, o.r, 1.0);
        Vec3::new(x, 0.0, z)
    }

    /// The fan vertices of the region `key` built, in world coordinates.
    fn fan_vertices(key: MeshRegionKey, result: &SummaryMeshResult) -> Vec<(Vec3, Vec3)> {
        let origin = origin_of(key);
        result
            .positions
            .iter()
            .zip(&result.normals)
            .map(|(p, n)| (Vec3::from_array(*p) + origin, Vec3::from_array(*n)))
            .filter(|(_, n)| n.y.abs() > 1e-3)
            .collect()
    }

    #[test]
    fn mesh_region_contains_271_summaries() {
        let region_lat = mesh_region_lattice();
        assert_eq!(region_lat.tiles_in_cell((0, 0)).count(), MESH_REGION_CELLS as usize);
    }

    /// Every region whose centre lies in the range is enumerated: the
    /// search reaches as many lattice steps as the range can hold along the
    /// lattice's shortest direction, at every level.
    #[test]
    fn region_search_reaches_every_centre_in_range() {
        let region_lat = mesh_region_lattice();
        for r in [0u32, 1, 4, 13, 40] {
            let summary_lat = summary_lattice(r);
            let outer = 10.4 * crate::summary::mesh_region_spacing_wu(r);
            let cam = (37.0, -91.0);
            let found = visible_mesh_regions_in_band_ungated(r, cam.0, cam.1, 0.0, outer);
            let mut expected = 0;
            for mn in -40..=40 {
                for mm in -40..=40 {
                    let (scq, scr) = summary_lat.cell_center(region_lat.cell_center((mn, mm)));
                    let (x, z) = flat_top_tile_center(scq, scr, 1.0);
                    if ((x - cam.0).powi(2) + (z - cam.1).powi(2)).sqrt() <= outer {
                        expected += 1;
                        assert!(found.contains(&MeshRegionKey { r, mn, mm }), "r={r} region ({mn},{mm}) in range but not found");
                    }
                }
            }
            assert_eq!(found.len(), expected, "r={r}");
        }
    }

    #[test]
    fn build_returns_none_when_no_data() {
        assert!(build_summary_mesh_region(1, REGION, &|_, _| None, None, None).is_none());
    }

    /// Water is a flat fan over each flooded cell at its surface, half a
    /// step under the ground's height at that step, and the sea builds
    /// nothing; every face shades as the surface.
    #[test]
    fn water_is_flat_at_its_surface() {
        let key = MeshRegionKey { r: 0, mn: 0, mm: 0 };
        let unknown = |_: i32, _: i32| None;
        assert_eq!(build_water_mesh_region(0, key, &|_, _| None, &unknown).tri_count, 0);
        let sea = build_water_mesh_region(0, key, &|_, _| Some(0), &unknown);
        assert_eq!(sea.tri_count, 0, "the sea is the plane's, not the region's");
        let built = build_water_mesh_region(0, key, &|q, r| ((q, r) == (0, 0)).then_some(4), &|_, _| Some(9));
        assert_eq!(built.tri_count, 6, "a fan, and nothing down an edge to a higher bank");
        let expect = height_y(4.0 - 0.5);
        assert!(built.positions.iter().all(|p| (p[1] - expect).abs() < 1e-4));
        assert!(built.normals.iter().all(|n| n[1] > 0.999), "every face shades as the surface");
        assert!(expect < height_y(4.0) && expect > height_y(3.0), "a surface at a step sits between that step's ground and the one below");
    }

    /// A face goes down each edge where what lies beyond is lower than the
    /// surface, to the lower water or the lower dry ground, landing beyond
    /// the edge, off the plane of the ground's face under it; faces
    /// dropping from one corner land at one foot; nothing goes down an edge
    /// to water or ground at or over the surface.
    #[test]
    fn water_drops_only_where_what_lies_beyond_is_lower() {
        let key = MeshRegionKey { r: 0, mn: 0, mm: 0 };
        // A cell at 4 beside a cell of water at 4, one of water at 3, dry
        // ground at 2, and dry ground at 4 all round otherwise.
        let water = |q: i32, r: i32| match (q, r) {
            (0, 0) | (-1, 0) => Some(4),
            (1, 0) => Some(3),
            _ => None,
        };
        let ground = |q: i32, r: i32| Some(if (q, r) == (0, 1) { 2 } else { 4 });
        let built = build_water_mesh_region(0, key, &water, &ground);
        // Three fans; from (0, 0) a drop to the lower water and one to the
        // lower ground; from (1, 0) one to that same lower ground, which
        // borders it too; from (-1, 0) none. A drop is its lip and its fall.
        assert_eq!(built.tri_count, 3 * 6 + 3 * 2 * 2);
        let origin = key.origin();
        let centre = |q: i32, r: i32| {
            let (x, z) = flat_top_tile_center(q - origin.q, r - origin.r, 1.0);
            Vec2::new(x, z)
        };
        let offset = DROP_OFFSET * 3.0_f32.sqrt();
        let at = |y: f32| -> Vec<Vec2> { built.positions.iter().filter(|p| (p[1] - y).abs() < 1e-4).map(|p| Vec2::new(p[0], p[2])).collect() };
        let (to_water, to_ground) = (height_y(3.0 - 0.5), height_y(2.0));
        for ((q, r), below) in [((1, 0), to_water), ((0, 1), to_ground)] {
            let edge = (centre(0, 0) + centre(q, r)) / 2.0;
            let across = (centre(q, r) - centre(0, 0)).normalize();
            let beyond = at(below).iter().map(|p| (*p - edge).dot(across)).filter(|&d| d > 1e-3 && d <= offset + 1e-4).count();
            assert!(beyond >= 2, "the face to ({q}, {r}) lands beyond its edge, at {below}");
        }
        // The drops from (0, 0) to the lower water and the lower ground, and
        // from (1, 0) to that ground, all meet at the corner the three cells
        // share: each falls straight down the one vertical line at its foot,
        // to its own depth.
        let shared = at(to_water).iter().any(|a| at(to_ground).iter().filter(|b| a.distance(**b) < 1e-4).count() >= 2);
        assert!(shared, "the three drops from one corner hang on one line");
        // Every foot hangs straight under its lip: a vertex over it, at the
        // surface it falls from. The feet on the ground are all drops'; on
        // the lower water, those just beyond the edge it falls over.
        let edge = (centre(0, 0) + centre(1, 0)) / 2.0;
        let across = (centre(1, 0) - centre(0, 0)).normalize();
        let on_water = at(to_water).into_iter().filter(|p| (*p - edge).dot(across) > 1e-3 && (*p - edge).dot(across) <= offset + 1e-4);
        let feet = on_water.map(|p| (p, to_water)).chain(at(to_ground).into_iter().map(|p| (p, to_ground)));
        for (p, y) in feet {
            let over = built.positions.iter().any(|q| q[1] > y + 1e-4 && Vec2::new(q[0], q[2]).distance(p) < 1e-4);
            assert!(over, "a fall stands straight under its lip at {p}");
        }
    }

    /// A drop to lower dry ground reaches that ground as it is drawn at
    /// each corner of the edge: where the corner rule meets it with a lower
    /// tile still, lower than the tile itself, the drop goes down to there.
    #[test]
    fn a_drop_reaches_the_ground_as_it_is_drawn() {
        let key = MeshRegionKey { r: 0, mn: 0, mm: 0 };
        let [beyond, third] = CORNER_NEIGHBOURS[0];
        let water = |q: i32, r: i32| ((q, r) == (0, 0)).then_some(28);
        let ground = move |q: i32, r: i32| {
            Some(match (q, r) {
                (0, 0) => 27,
                at if at == beyond => 27,
                at if at == third => 26,
                _ => 30,
            })
        };
        // Beyond edge 0, the tile at 27 is drawn at corner 0, where it meets
        // the wet tile's ground at 27 and the third tile at 26, lower than
        // itself.
        let drawn = own_corner_z([Some(27), Some(27), Some(26)], 1).expect("all three");
        assert!(drawn < 27.0, "the corner rule draws it lower there: {drawn}");
        let built = build_water_mesh_region(0, key, &water, &ground);
        let reaches = built.positions.iter().any(|p| (p[1] - height_y(drawn)).abs() < 1e-4);
        assert!(reaches, "the drop reaches the ground as it is drawn at that corner, {}", height_y(drawn));
    }

    /// A region millions of tiles out is built as the same region at the
    /// world's origin, its vertices, normals and morph targets bit for bit:
    /// every vector is a tile difference from the region's origin tile.
    #[test]
    fn a_region_builds_the_same_however_far_out() {
        let field = |q: i32, r: i32| Some(((q * 7 + r * 3).rem_euclid(11)) - 5);
        let coarse = |q: i32, r: i32| Some((q * 5 + r * 2).rem_euclid(9));
        let far = MeshRegionKey { r: 1, mn: 30_000, mm: -60_000 };
        let shift = mesh_region_lattice().cell_center((far.mn, far.mm));
        assert_eq!((shift.0 % 3, shift.1 % 3), (0, 0), "the far region's origin is a coarser centre too");
        let coarse_shift = (shift.0 / 3, shift.1 / 3);
        let far_field = move |q: i32, r: i32| field(q - shift.0, r - shift.1);
        let far_coarse = move |q: i32, r: i32| coarse(q - coarse_shift.0, r - coarse_shift.1);
        let near = build_summary_mesh_region(1, REGION, &field, Some(&coarse), None).unwrap();
        let out = build_summary_mesh_region(1, far, &far_field, Some(&far_coarse), None).unwrap();
        assert!(far.origin().r.abs() > 1_000_000, "{:?}", far.origin());
        assert_eq!(near.positions, out.positions);
        assert_eq!(near.normals, out.normals);
        assert_eq!(near.coarse, out.coarse);
        assert_eq!(near.indices, out.indices);
    }

    #[test]
    fn flat_region_is_fans_plus_perimeter_curtains() {
        let result = build_summary_mesh_region(1, REGION, &|_, _| Some(5), None, None).unwrap();
        assert_eq!(result.tri_count, MESH_REGION_CELLS * 6 + PERIMETER_EDGES * 2);
        let fans = fan_vertices(REGION, &result);
        let y = fans[0].0.y;
        assert!(fans.iter().all(|(p, _)| (p.y - y).abs() < 1e-5), "flat ground is not flat");
        assert!(fans.iter().all(|(_, n)| (n.y - 1.0).abs() < 1e-5), "flat ground normals are not up");
    }

    #[test]
    fn shared_corners_are_single_vertices() {
        let result = build_summary_mesh_region(1, REGION, &|_, _| Some(5), None, None).unwrap();
        assert_eq!(fan_vertices(REGION, &result).len(), MESH_REGION_CELLS as usize + CORNER_VERTICES);
    }

    #[test]
    fn build_waits_for_every_cell_of_region_and_ring() {
        let region_lat = mesh_region_lattice();
        let hole = |q: i32, r: i32| ((q, r) != (0, 0)).then_some(5);
        assert!(build_summary_mesh_region(1, REGION, &hole, None, None).is_none(), "built with a cell missing");
        let no_ring = |q: i32, r: i32| (region_lat.cell_id(q, r) == (0, 0)).then_some(5);
        assert!(build_summary_mesh_region(1, REGION, &no_ring, None, None).is_none(), "built with the ring missing");
    }

    #[test]
    fn build_waits_for_the_coarser_cells_under_it() {
        let flat = |_: i32, _: i32| Some(5);
        // The coarser cell under the region's centre is absent.
        let hole = |q: i32, r: i32| ((q, r) != (0, 0)).then_some(5);
        assert!(build_summary_mesh_region(1, REGION, &flat, Some(&hole), None).is_none(), "built with a coarser cell missing");
        // A ring cell of the coarser cell at the region's edge is absent: the
        // region's cells reach 9 fine cells out, three coarse cells, whose
        // ring is the fourth.
        let far = |q: i32, _: i32| (q != 4).then_some(5);
        assert!(build_summary_mesh_region(1, REGION, &flat, Some(&far), None).is_none(), "built with a coarser ring cell missing");
    }

    /// Without a coarser level a vertex's target is itself: the morph is a
    /// no-op and the coarsest level never moves.
    #[test]
    fn coarse_target_is_the_vertex_itself_at_the_coarsest_level() {
        let field = |q: i32, r: i32| Some(((q * 7 + r * 3).rem_euclid(11)) - 5);
        let result = build_summary_mesh_region(1, REGION, &field, None, None).unwrap();
        for ((p, n), c) in result.positions.iter().zip(&result.normals).zip(&result.coarse) {
            assert!((c[3] - p[1]).abs() < 1e-5, "target height {} is not the vertex's {}", c[3], p[1]);
            assert!((Vec3::from_array(*n) - Vec3::new(c[0], c[1], c[2])).length() < 1e-5);
        }
    }

    /// Over flat coarser ground every target is that ground, in this
    /// level's frame, whatever the fine relief; and where the coarser field
    /// varies, a fine vertex at a coarser cell's centre — every coarser
    /// centre is one, since levels nest — targets exactly that cell's height.
    #[test]
    fn coarse_targets_lie_on_the_coarser_surface() {
        let fine = |q: i32, r: i32| Some(((q * 7 + r * 3).rem_euclid(11)) - 5);
        let flat = |_: i32, _: i32| Some(20);
        let result = build_summary_mesh_region(1, REGION, &fine, Some(&flat), None).unwrap();
        let want = height_y(20.0) - level_depth_bias(1);
        // Fan vertices only: a curtain keeps its own normal and hangs from
        // its corners' targets.
        for (c, _) in result.coarse.iter().zip(&result.normals).filter(|(_, n)| n[1].abs() > 1e-3) {
            assert!((c[3] - want).abs() < 1e-4, "target {} off flat coarser ground at {want}", c[3]);
            assert!((c[1] - 1.0).abs() < 1e-5, "flat coarser ground has a leaning normal");
        }
        assert!(result.positions.iter().any(|p| (p[1] - want).abs() > 0.5), "the fine relief is flat");

        let coarse = |q: i32, r: i32| Some((q * 5 + r * 2).rem_euclid(9));
        let result = build_summary_mesh_region(1, REGION, &fine, Some(&coarse), None).unwrap();
        let coarse_lat = summary_lattice(4);
        let mut centres = 0;
        for (p, c) in result.positions.iter().zip(&result.coarse) {
            let w = Vec3::from_array(*p) + origin_of(REGION);
            let cell = coarse_lat.cell_at(w.xz());
            let (cq, cr) = coarse_lat.cell_center(cell);
            let (cx, cz) = flat_top_tile_center(cq, cr, 1.0);
            if (w.x - cx).abs() > 1e-3 || (w.z - cz).abs() > 1e-3 {
                continue;
            }
            centres += 1;
            let want = height_y(coarse(cell.0, cell.1).unwrap() as f32) - level_depth_bias(1);
            assert!((c[3] - want).abs() < 1e-4, "target {} at coarser centre {cell:?} (want {want})", c[3]);
        }
        assert!(centres > 0, "no fine vertex sits on a coarser centre");
    }

    #[test]
    fn adjacent_regions_agree_on_shared_vertices() {
        // A varying field whose cells all reach each other, so every corner
        // is met: every vertex the two regions both place must be at the
        // same height, or the seam between them opens.
        let field = |q: i32, r: i32| Some((q * 7 + r * 3).rem_euclid(3));
        let a = build_summary_mesh_region(1, REGION, &field, None, None).unwrap();
        let b = build_summary_mesh_region(1, MeshRegionKey { r: 1, mn: 1, mm: 0 }, &field, None, None).unwrap();
        let av = fan_vertices(REGION, &a);
        let mut shared = 0;
        for (pb, _) in fan_vertices(MeshRegionKey { r: 1, mn: 1, mm: 0 }, &b) {
            for (pa, _) in &av {
                if (pa.x - pb.x).abs() < 1e-3 && (pa.z - pb.z).abs() < 1e-3 {
                    shared += 1;
                    assert!((pa.y - pb.y).abs() < 1e-3, "seam opens at {pa:?} vs {pb:?}");
                }
            }
        }
        assert!(shared > 0, "adjacent regions share no vertices");
    }

    #[test]
    fn a_cliff_out_of_reach_is_a_wall() {
        // A cliff: half the field 20 steps higher, far past a 3-wide cell's
        // reach. Each side keeps its own corners, and a wall hangs from the
        // high side's down to the low side's.
        let field = |q: i32, _r: i32| Some(if q > 0 { 20 } else { 0 });
        let result = build_summary_mesh_region(1, REGION, &field, None, None).unwrap();
        let bias = level_depth_bias(1);
        let (top, foot) = (height_y(20.0) - bias, height_y(0.0) - bias);
        let wall = |y: f32| result.positions.iter().zip(&result.normals).any(|(p, n)| n[1].abs() < 1e-3 && (p[1] - y).abs() < 1e-3);
        assert!(wall(top) && wall(foot), "no wall spans the cliff from {top} to {foot}");
        let between = result.positions.iter().zip(&result.normals).filter(|(p, n)| n[1].abs() > 1e-3 && p[1] > foot + 0.5 && p[1] < top - 0.5).count();
        assert_eq!(between, 0, "a fan vertex stands partway down the cliff");
    }

    /// A region's layer puts every part of its cells and ring on a texel of
    /// its own, the centre cell's centre part at the middle and its corner
    /// part a finer step up both axes, a wet part with its canopy and the
    /// wet bit but on the last level, and waits for a cell that is not there.
    #[test]
    fn a_parts_layer_puts_each_part_on_its_own_texel() {
        use common::cover::{Canopy, Content, Cover};
        for radius in [1, 4] {
            let key = MeshRegionKey { r: radius, mn: 1, mm: -1 };
            let centre = mesh_region_lattice().cell_center((key.mn, key.mm));
            let wood = Canopy::of(Cover::NONE.with(0, Content::Pine));
            let brush = Canopy::of(Cover::NONE.with(0, Content::Brush));
            let canopy = |q: i32, r: i32| {
                let mut cell = SummaryCell { canopy: [wood; PARTS], ..Default::default() };
                if (q, r) == centre {
                    cell.canopy[7] = brush;
                    cell.wet = 1 << 1;
                }
                Some(cell)
            };
            let layer = parts_layer(radius, key, &canopy).expect("every cell is there");
            let reach = crate::summary::MESH_REGION_RADIUS + 1;
            let cells = 3 * reach * reach + 3 * reach + 1;
            assert_eq!(layer.iter().filter(|&&t| t != 0).count(), cells as usize * PARTS, "r={radius}");
            let at = |column: i32, row: i32| layer[(row * PARTS_LAYER_SIDE as i32 + column) as usize];
            assert_eq!(at(PARTS_LAYER_HALF, PARTS_LAYER_HALF), wood.bits());
            assert_eq!(at(PARTS_LAYER_HALF + 1, PARTS_LAYER_HALF + 1), brush.bits());
            assert_eq!(at(PARTS_LAYER_HALF + 1, PARTS_LAYER_HALF), wood.bits() | PART_WET, "part 1 is one finer step along q");
            let hole = |q: i32, r: i32| if (q, r) == (centre.0 + 10, centre.1) { None } else { canopy(q, r) };
            assert!(parts_layer(radius, key, &hole).is_none(), "a ring cell that is not there");
        }
        let last = *crate::summary::LOD_LEVELS.last().expect("a ladder");
        let key = MeshRegionKey { r: last, mn: 0, mm: 0 };
        let wet = |_: i32, _: i32| Some(SummaryCell { wet: (1u16 << PARTS) - 1, ..Default::default() });
        let layer = parts_layer(last, key, &wet).expect("every cell is there");
        assert!(layer.iter().all(|&t| t & PART_WET == 0), "the last level is dry");
    }

    /// A level built with a canopy carries one per vertex: a cell's own at
    /// its centre, the mean of the three cells meeting at a corner, none
    /// under a curtain, and none at all when built without.
    #[test]
    fn canopy_rides_the_vertices_by_the_corner_rule() {
        use common::cover::{Canopy, Content, Cover};
        let flat = |_: i32, _: i32| Some(5);
        let bare = build_summary_mesh_region(1, REGION, &flat, None, None).unwrap();
        assert!(bare.canopy.is_empty());

        let pines = [Canopy::of(Cover::NONE.with(0, Content::Pine).with(1, Content::Pine)); PARTS];
        let one = |q: i32, r: i32| Some(if (q, r) == (0, 0) { pines } else { [Canopy::NONE; PARTS] });
        let result = build_summary_mesh_region(1, REGION, &flat, None, Some(&one)).unwrap();
        assert_eq!(result.canopy.len(), result.positions.len());
        let own = canopy_vertex(&pines);
        assert!((own[0] - 2.0 / 3.0).abs() < 1e-6 && own[1] == own[0] && own[2] == 0.0, "density, all of it pine");
        let at_centre = result.positions.iter().zip(&result.canopy).find(|(p, _)| p[0].abs() < 1e-4 && p[2].abs() < 1e-4);
        let (_, centre) = at_centre.expect("the origin cell's centre vertex");
        assert_eq!(*centre, own);
        let thirds = result.canopy.iter().filter(|c| c.iter().zip(&own).all(|(a, b)| (a - b / 3.0).abs() < 1e-6)).count();
        assert_eq!(thirds, 6, "each of the cell's six corners averages it with two bare cells");
        let none = result.canopy.iter().filter(|c| **c == [0.0; 4]).count();
        assert_eq!(none, result.canopy.len() - 7, "everything else, curtains included, carries nothing");
    }
}
