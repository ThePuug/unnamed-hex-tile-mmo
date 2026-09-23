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
use common::cover::{Canopy, Slot};

use crate::{
    chunk::{self, ChunkId},
    geometry::flat_top_tile_center,
    summary::{
        Band, SummaryLattice, canonical_vertex_id, coarser_level, level_depth_bias,
        mesh_region_lattice, summary_lattice,
    },
    surface::{
        CORNER_NEIGHBOURS, centre_normal, corner_normal, corner_offsets, corner_z, fan_weights,
        height_y,
    },
};

/// Key identifying a mesh region within a specific distance band.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MeshRegionKey {
    /// Summary radius for this band.
    pub r: u32,
    /// Mesh region lattice coordinates (in summary-lattice space).
    pub mn: i32,
    pub mm: i32,
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
    /// World-space origin of this mesh region (for Transform).
    pub mesh_origin: Vec3,
}

/// A canopy as a vertex carries it: the density, then the density's
/// pine, deciduous and brush parts, which sum to it. Parts interpolate
/// across a fan the way a premultiplied colour does — a wood's edge
/// against bare ground thins without shifting hue — where shares would
/// fade twice and counts would not blend at all.
pub fn canopy_vertex(canopy: Canopy) -> [f32; 4] {
    let part = |kind: Slot| canopy.count(kind) as f32 / common::cover::CANOPY_READINGS as f32;
    [canopy.density() as f32, part(Slot::Pine), part(Slot::Deciduous), part(Slot::Brush)]
}

/// Cells in a mesh region (radius-9 hex ball).
pub const MESH_REGION_CELLS: u32 = 271;

/// Downward curtain depth (WU) under edges facing unbuilt cells. Deep enough
/// to cover the relief between a built region and the ground beside it.
pub const CURTAIN_DEPTH_WU: f32 = 24.0;

/// A cell's fan as its level draws it: the centre in world XZ, centre and
/// corner heights in world Y before the level's depth bias, and the vertex
/// normals. Corners are the mean of the three cells meeting there.
pub struct Fan {
    pub centre: Vec2,
    pub centre_y: f32,
    pub corner_y: [f32; 6],
    pub centre_n: Vec3,
    pub corner_n: [Vec3; 6],
}

/// The fan of `cell` on `lattice` from a height lookup over it. None while
/// the cell or one of its six neighbours is absent: a corner with a cell
/// missing would not be the corner the neighbour draws.
pub fn cell_fan(
    lattice: &SummaryLattice,
    cell: (i32, i32),
    height: impl Fn(i32, i32) -> Option<i32>,
) -> Option<Fan> {
    let offsets = corner_offsets(lattice.scale as f32);
    let centre_of = |c: (i32, i32)| -> Vec2 {
        let (cq, cr) = lattice.cell_center(c);
        let (wx, wz) = flat_top_tile_center(cq, cr, 1.0);
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
        let cz = corner_z([Some(z), Some(height(na.0, na.1)?), Some(height(nb.0, nb.1)?)])?;
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
pub struct LevelSurface<'a> {
    lattice: SummaryLattice,
    height: &'a dyn Fn(i32, i32) -> Option<i32>,
    fans: HashMap<(i32, i32), Option<Fan>>,
}

impl<'a> LevelSurface<'a> {
    pub fn new(radius: u32, height: &'a dyn Fn(i32, i32) -> Option<i32>) -> Self {
        LevelSurface { lattice: summary_lattice(radius), height, fans: HashMap::new() }
    }

    /// Height (world Y, before any bias) and normal at world `xz`. None
    /// while a cell the fan reads is absent.
    pub fn at(&mut self, xz: Vec2) -> Option<(f32, Vec3)> {
        let cell = self.lattice.cell_at(xz);
        let (lattice, height) = (&self.lattice, self.height);
        let fan = self
            .fans
            .entry(cell)
            .or_insert_with(|| cell_fan(lattice, cell, height))
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
/// heights are durable, so nothing a built region depends on ever changes.
/// Returns None until then. Producers cover one region ring more than
/// consumers build, and the coarser level over every band
/// (`visible_lod_regions`), so what a needed region waits on always arrives.
pub fn build_summary_mesh_region(
    radius: u32,
    region_key: MeshRegionKey,
    height: &dyn Fn(i32, i32) -> Option<i32>,
    coarse: Option<&dyn Fn(i32, i32) -> Option<i32>>,
    canopy: Option<&dyn Fn(i32, i32) -> Option<Canopy>>,
) -> Option<SummaryMeshResult> {
    let lattice = summary_lattice(radius);
    let region_lat = mesh_region_lattice();
    let region_id = (region_key.mn, region_key.mm);

    let region_center = region_lat.cell_center(region_id);
    let (origin_cq, origin_cr) = lattice.cell_center(region_center);
    let (origin_wx, origin_wz) = flat_top_tile_center(origin_cq, origin_cr, 1.0);
    let mesh_origin = Vec3::new(origin_wx, 0.0, origin_wz);

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
                canopies.insert(cell, canopy_vertex(canopy(cell.0, cell.1)?));
            }
        }
    }

    let bias = level_depth_bias(radius);
    let offsets = corner_offsets(lattice.scale as f32);
    let mut coarse = coarse.map(|height| {
        LevelSurface::new(coarser_level(radius).expect("a level with a coarser one"), height)
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
    let mut corner_index: HashMap<(i32, i32), u32> = HashMap::new();

    let mut push = |p: Vec3, n: Vec3, target: (f32, Vec3), canopy: [f32; 4]| -> u32 {
        let v = p - mesh_origin;
        let (ty, tn) = target;
        positions.push([v.x, v.y, v.z]);
        normals.push([n.x, n.y, n.z]);
        coarse_attr.push([tn.x, tn.y, tn.z, ty - mesh_origin.y]);
        if !canopies.is_empty() {
            canopy_attr.push(canopy);
        }
        (positions.len() - 1) as u32
    };
    let canopy_of = |cell: (i32, i32)| canopies.get(&cell).copied().unwrap_or([0.0; 4]);

    for &cell in &region_cells {
        let fan = cell_fan(&lattice, cell, |q, r| heights.get(&(q, r)).copied())
            .expect("region and ring heights are present");
        let centre_canopy = canopy_of(cell);

        let mut corner_pos = [Vec3::ZERO; 6];
        let mut corner_target = [(0.0, Vec3::Y); 6];
        let mut corner_idx = [0u32; 6];
        for i in 0..6 {
            let p = Vec3::new(fan.centre.x + offsets[i].x, fan.corner_y[i] - bias, fan.centre.y + offsets[i].y);
            corner_pos[i] = p;
            corner_target[i] = target(p, fan.corner_n[i])?;
            let id = canonical_vertex_id(cell.0, cell.1, i);
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
        mesh_origin,
    })
}

/// Build the water of a mesh region at `radius`: a flat hexagon at its
/// surface over every cell with water, and a curtain down each edge that
/// does not meet a flooded neighbour at the same surface, so a river
/// stepping down reads as a fall and a bank the rounding left half a step
/// low shows no gap under the water's edge. `water(sq, sr)` is the surface
/// over a cell as a z-level, or None where it is dry or unknown. Water at
/// zero is not built: the sea is one plane drawn to the horizon. Sinks by
/// the level's depth bias with the ground, and is empty where nothing is
/// flooded.
pub fn build_water_mesh_region(
    radius: u32,
    region_key: MeshRegionKey,
    water: &dyn Fn(i32, i32) -> Option<i32>,
) -> SummaryMeshResult {
    let lattice = summary_lattice(radius);
    let region_lat = mesh_region_lattice();
    let region_id = (region_key.mn, region_key.mm);

    let region_center = region_lat.cell_center(region_id);
    let (origin_cq, origin_cr) = lattice.cell_center(region_center);
    let (origin_wx, origin_wz) = flat_top_tile_center(origin_cq, origin_cr, 1.0);
    let mesh_origin = Vec3::new(origin_wx, 0.0, origin_wz);

    let bias = level_depth_bias(radius);
    let offsets = corner_offsets(lattice.scale as f32);
    // A surface at a step lies between that step's ground and the one
    // below it: the tiles below the step are under it, the tiles at it dry.
    let surface_y = |w: i32| height_y(w as f32 - 0.5) - bias;
    // The drop that hides a dry neighbour's ground standing a rounding's
    // half step under the water's edge.
    let bank_drop = height_y(0.0) - height_y(-1.0);

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let push = |positions: &mut Vec<[f32; 3]>, normals: &mut Vec<[f32; 3]>, p: Vec3, n: Vec3| -> u32 {
        let v = p - mesh_origin;
        positions.push([v.x, v.y, v.z]);
        normals.push([n.x, n.y, n.z]);
        (positions.len() - 1) as u32
    };

    for cell in region_lat.tiles_in_cell(region_id) {
        let Some(w) = water(cell.0, cell.1).filter(|&w| w > 0) else { continue };
        let (cq, cr) = lattice.cell_center(cell);
        let (wx, wz) = flat_top_tile_center(cq, cr, 1.0);
        let y = surface_y(w);
        let ci = push(&mut positions, &mut normals, Vec3::new(wx, y, wz), Vec3::Y);
        let corners = offsets.map(|o| Vec3::new(wx + o.x, y, wz + o.y));
        let mut idx = [0u32; 6];
        for i in 0..6 {
            idx[i] = push(&mut positions, &mut normals, corners[i], Vec3::Y);
        }
        for i in 0..6 {
            let j = (i + 1) % 6;
            indices.extend([ci, idx[j], idx[i]]);
        }

        // Edge (i, i+1) faces the first neighbour listed for corner i.
        for i in 0..6 {
            let d = CORNER_NEIGHBOURS[i][0];
            let facing = water(cell.0 + d.0, cell.1 + d.1);
            let bottom = match facing {
                Some(nw) if nw >= w => continue,
                Some(nw) => surface_y(nw.max(0)),
                None => y - bank_drop,
            };
            let j = (i + 1) % 6;
            let (top0, top1) = (corners[i], corners[j]);
            let (bot0, bot1) = (Vec3::new(top0.x, bottom, top0.z), Vec3::new(top1.x, bottom, top1.z));
            let outward = (top1 - top0).normalize_or_zero().cross(Vec3::NEG_Y).normalize_or_zero();
            let n = if outward.length_squared() > 0.5 { outward } else { Vec3::Z };
            let base = positions.len() as u32;
            for p in [top0, top1, bot1, bot0] {
                push(&mut positions, &mut normals, p, n);
            }
            indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
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
        mesh_origin,
    }
}

/// Enumerate mesh regions within a distance band that overlap loaded chunks.

/// `camera_wx/wz`: camera world position (XZ plane).
/// `inner_wu/outer_wu`: extended band range (natural + overlap).
/// Only includes regions whose world-space centers fall within range
/// AND overlap at least one loaded chunk.
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

            // Check world-space distance of this region's center from camera
            let region_center = region_lat.cell_center((mn, mm));
            let (scq, scr) = summary_lat.cell_center(region_center);
            let (rwx, rwz) = flat_top_tile_center(scq, scr, 1.0);
            let dx = rwx - camera_wx;
            let dz = rwz - camera_wz;
            let dist = (dx * dx + dz * dz).sqrt();

            if dist < inner_wu || dist > outer_wu {
                continue;
            }

            // Check overlap with loaded chunks: at least one summary's tiles
            // must be in loaded chunks. Quick check: region center's tile.
            let qrz = qrz::Qrz { q: scq, r: scr, z: 0 };
            let chunk_id = chunk::loc_to_chunk(qrz);
            if !loaded_chunks.contains(&chunk_id) {
                // Try neighbor summary centers
                let mut any_loaded = false;
                for &(dsn, dsm) in &[(1,0),(-1,0),(0,1),(0,-1),(1,-1),(-1,1)] {
                    let nb = (region_center.0 + dsn, region_center.1 + dsm);
                    let (nq, nr) = summary_lat.cell_center(nb);
                    let nqrz = qrz::Qrz { q: nq, r: nr, z: 0 };
                    if loaded_chunks.contains(&chunk::loc_to_chunk(nqrz)) {
                        any_loaded = true;
                        break;
                    }
                }
                if !any_loaded { continue; }
            }

            regions.insert(MeshRegionKey { r, mn, mm });
        }
    }

    regions
}

/// Lattice steps a region search must reach from the camera's region to
/// hold every region whose centre lies within `outer_wu`: a region at k
/// steps is at least k · spacing · √3/2 away, the lattice's inradius
/// direction, and the camera stands anywhere in its own region.
fn search_steps(r: u32, outer_wu: f32) -> i32 {
    let step = crate::summary::mesh_region_spacing_wu(r) * 3.0_f32.sqrt() / 2.0;
    ((outer_wu / step).ceil() as i32 + 2).min(60)
}

/// Like `visible_mesh_regions_in_band` but without the loaded-chunk gate.
/// Used for remote summary bands where data comes from server, not local tiles.
pub fn visible_mesh_regions_in_band_ungated(
    r: u32,
    camera_wx: f32,
    camera_wz: f32,
    inner_wu: f32,
    outer_wu: f32,
) -> HashSet<MeshRegionKey> {
    let summary_lat = summary_lattice(r);
    let region_lat = mesh_region_lattice();

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
    let region_center = mesh_region_lattice().cell_center((key.mn, key.mm));
    let (cq, cr) = summary_lattice(key.r).cell_center(region_center);
    let centre = chunk::loc_to_chunk(qrz::Qrz { q: cq, r: cr, z: 0 });
    let reach = mesh_region_extent_wu(key.r) / 3.0_f32.sqrt() + summary_width_wu(key.r);
    let radius = (reach / (chunk::CHUNK_EXTENT_WU * chunk::APOTHEM_FACTOR)).ceil() as u8;
    chunk::calculate_visible_chunks(centre, radius).iter().all(|c| loaded_chunks.contains(c))
}

/// Collect visible mesh regions across all bands for the producers (server,
/// flyover): everything beyond the local-data boundary, plus regions whose
/// footprint straddles it.

/// `local_boundary_wu` is the extent the consumer's Map can serve:
/// FIXED_STREAM_RADIUS_WU in gameplay, the flyover's detail-chunk radius in
/// flyover (much smaller — using the gameplay constant there left an
/// un-rendered ring between the flyover's chunks and the first produced band).

/// Straddling matters: a region centered just inside the boundary has
/// summaries beyond it whose tiles the Map can never resolve — the producer
/// covers those. Values agree with Map-computed ones because every producer
/// uses the same 7-sample rule over the same elevation field.

/// The consumer builds a region only once its ring has heights too, so the
/// producer covers one region ring more than the consumer needs at both
/// ends of every band — without it, the horizon shell and the regions just
/// inside the boundary would wait forever.
pub fn visible_lod_regions(
    bands: &[Band],
    cam_wx: f32,
    cam_wz: f32,
    local_boundary_wu: f32,
) -> HashSet<MeshRegionKey> {
    use crate::summary::{mesh_region_extent_wu, mesh_region_spacing_wu, summary_width_wu};
    let circum = |r: u32| mesh_region_extent_wu(r) / 3.0_f32.sqrt();
    let mut out = HashSet::new();
    for band in bands {
        let half_extent = 0.5 * mesh_region_extent_wu(band.r);
        let ring = mesh_region_spacing_wu(band.r);
        // Footprint-overlap enumeration over the band (matches the
        // consumer): every region whose footprint touches the band is
        // produced. Center-only membership left regions centered just
        // outside an edge to neither band — un-rendered crescents at every
        // level boundary.
        let (win_inner, win_outer) = (band.inner_wu, band.outer_wu);
        let outer = win_outer + half_extent + ring;
        // A band whose regions and rings cannot reach past the local
        // boundary is fully consumer-owned (Map-computed).
        if outer > local_boundary_wu {
            let inner = (win_inner - half_extent).max(local_boundary_wu) - ring;
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
        let reach = half_extent + circum(band.r) + cell_reach + circum(c);
        let outer_c = win_outer + reach;
        if outer_c <= local_boundary_wu { continue; }
        let inner_c = (win_inner - reach).max(local_boundary_wu) - mesh_region_spacing_wu(c);
        out.extend(visible_mesh_regions_in_band_ungated(
            c, cam_wx, cam_wz, inner_c.max(0.0), outer_c,
        ));
    }
    out
}

/// Enumerate all mesh region keys that overlap a set of loaded chunks,
/// for a given summary radius. Used by forced-radius mode (no distance filter).
pub fn visible_mesh_regions(
    radius: u32,
    loaded_chunks: &HashSet<ChunkId>,
) -> HashSet<MeshRegionKey> {
    let summary_lat = summary_lattice(radius);
    let region_lat = mesh_region_lattice();
    let mut regions = HashSet::new();

    for &chunk_id in loaded_chunks {
        let center = chunk_id.center();
        let summary_cell = summary_lat.cell_id(center.q, center.r);
        let region = region_lat.cell_id(summary_cell.0, summary_cell.1);
        regions.insert(MeshRegionKey {
            r: radius,
            mn: region.0,
            mm: region.1,
        });

        for &(dn, dm) in &[(1, 0), (-1, 0), (0, 1), (0, -1), (1, -1), (-1, 1)] {
            let nb = (summary_cell.0 + dn, summary_cell.1 + dm);
            let rid = region_lat.cell_id(nb.0, nb.1);
            regions.insert(MeshRegionKey { r: radius, mn: rid.0, mm: rid.1 });
        }
    }

    regions
}


#[cfg(test)]
mod tests {
    use super::*;

    const REGION: MeshRegionKey = MeshRegionKey { r: 1, mn: 0, mm: 0 };

    /// Perimeter edges of a radius-R hex ball: 6(2R+1).
    const PERIMETER_EDGES: u32 = 6 * (2 * 9 + 1);

    /// Corner vertices of a radius-R hex ball: 6(R+1)².
    const CORNER_VERTICES: usize = 6 * 10 * 10;

    fn fan_vertices(result: &SummaryMeshResult) -> Vec<(Vec3, Vec3)> {
        result
            .positions
            .iter()
            .zip(&result.normals)
            .map(|(p, n)| (Vec3::from_array(*p) + result.mesh_origin, Vec3::from_array(*n)))
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
    /// step under the ground's height at that step; the sea builds nothing;
    /// a curtain hangs under every edge but one shared with a neighbour at
    /// the same surface, down to a lower neighbour's surface.
    #[test]
    fn water_is_flat_at_its_surface_with_curtains_to_lower_water() {
        let dry = build_water_mesh_region(0, MeshRegionKey { r: 0, mn: 0, mm: 0 }, &|_, _| None);
        assert_eq!(dry.tri_count, 0);
        let sea = build_water_mesh_region(0, MeshRegionKey { r: 0, mn: 0, mm: 0 }, &|_, _| Some(0));
        assert_eq!(sea.tri_count, 0, "the sea is the plane's, not the region's");

        // Two flooded cells side by side at one surface, a third beside them a step lower.
        let water = |q: i32, r: i32| match (q, r) {
            (0, 0) | (1, 0) => Some(4),
            (2, 0) => Some(3),
            _ => None,
        };
        let key = MeshRegionKey { r: 0, mn: 0, mm: 0 };
        let built = build_water_mesh_region(0, key, &water);
        // Three fans of six, and a curtain on every edge but the one shared
        // at one surface and the lower cell's edge under the higher one's
        // drop: 5 + 5 + 5 quads.
        assert_eq!(built.tri_count, 3 * 6 + 2 * (5 + 5 + 5));
        let ys: Vec<f32> = built
            .positions
            .iter()
            .zip(&built.normals)
            .filter(|(_, n)| n[1] > 0.5)
            .map(|(p, _)| p[1])
            .collect();
        let expect = |w: i32| height_y(w as f32 - 0.5);
        for y in &ys {
            assert!((y - expect(4)).abs() < 1e-4 || (y - expect(3)).abs() < 1e-4, "surface at {y}");
        }
        assert!(ys.iter().any(|y| (y - expect(4)).abs() < 1e-4) && ys.iter().any(|y| (y - expect(3)).abs() < 1e-4));
        assert!(expect(4) < height_y(4.0) && expect(4) > height_y(3.0), "a surface at a step sits between that step's ground and the one below");
    }

    #[test]
    fn flat_region_is_fans_plus_perimeter_curtains() {
        let result = build_summary_mesh_region(1, REGION, &|_, _| Some(5), None, None).unwrap();
        assert_eq!(result.tri_count, MESH_REGION_CELLS * 6 + PERIMETER_EDGES * 2);
        let fans = fan_vertices(&result);
        let y = fans[0].0.y;
        assert!(fans.iter().all(|(p, _)| (p.y - y).abs() < 1e-5), "flat ground is not flat");
        assert!(fans.iter().all(|(_, n)| (n.y - 1.0).abs() < 1e-5), "flat ground normals are not up");
    }

    #[test]
    fn shared_corners_are_single_vertices() {
        let result = build_summary_mesh_region(1, REGION, &|_, _| Some(5), None, None).unwrap();
        assert_eq!(fan_vertices(&result).len(), MESH_REGION_CELLS as usize + CORNER_VERTICES);
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
        for (c, n) in result.coarse.iter().zip(&result.normals).filter(|(_, n)| n[1].abs() > 1e-3) {
            assert!((c[3] - want).abs() < 1e-4, "target {} off flat coarser ground at {want}", c[3]);
            assert!((c[1] - 1.0).abs() < 1e-5, "flat coarser ground has a leaning normal");
        }
        assert!(result.positions.iter().any(|p| (p[1] - want).abs() > 0.5), "the fine relief is flat");

        let coarse = |q: i32, r: i32| Some((q * 5 + r * 2).rem_euclid(9));
        let result = build_summary_mesh_region(1, REGION, &fine, Some(&coarse), None).unwrap();
        let coarse_lat = summary_lattice(4);
        let mut centres = 0;
        for (p, c) in result.positions.iter().zip(&result.coarse) {
            let w = Vec3::from_array(*p) + result.mesh_origin;
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
        // A varying field: every vertex the two regions both place must be
        // at the same height, or the seam between them opens.
        let field = |q: i32, r: i32| Some(((q * 7 + r * 3).rem_euclid(11)) - 5);
        let a = build_summary_mesh_region(1, REGION, &field, None, None).unwrap();
        let b = build_summary_mesh_region(1, MeshRegionKey { r: 1, mn: 1, mm: 0 }, &field, None, None).unwrap();
        let av = fan_vertices(&a);
        let mut shared = 0;
        for (pb, _) in fan_vertices(&b) {
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
    fn steep_neighbours_make_steep_triangles_not_gaps() {
        // A cliff: half the field 20 steps higher. Every triangle still
        // shares its vertices (no vertex duplicated among the fans), and the
        // fan normals at the cliff lean over.
        let field = |q: i32, _r: i32| Some(if q > 0 { 20 } else { 0 });
        let result = build_summary_mesh_region(1, REGION, &field, None, None).unwrap();
        assert_eq!(fan_vertices(&result).len(), MESH_REGION_CELLS as usize + CORNER_VERTICES);
        let leaning = fan_vertices(&result).iter().filter(|(_, n)| n.y < 0.7).count();
        assert!(leaning > 0, "a 20-step cliff produced no steep normals");
    }

    /// A level built with a canopy carries one per vertex: a cell's own at
    /// its centre, the mean of the three cells meeting at a corner, none
    /// under a curtain, and none at all when built without.
    #[test]
    fn canopy_rides_the_vertices_by_the_corner_rule() {
        use common::cover::{Canopy, Cover, Slot};
        let flat = |_: i32, _: i32| Some(5);
        let bare = build_summary_mesh_region(1, REGION, &flat, None, None).unwrap();
        assert!(bare.canopy.is_empty());

        let pines = Canopy::of(&[Cover::NONE.with(0, Slot::Pine).with(1, Slot::Pine); 7]);
        let one = |q: i32, r: i32| Some(if (q, r) == (0, 0) { pines } else { Canopy::NONE });
        let result = build_summary_mesh_region(1, REGION, &flat, None, Some(&one)).unwrap();
        assert_eq!(result.canopy.len(), result.positions.len());
        let own = canopy_vertex(pines);
        assert!((own[0] - 14.0 / 21.0).abs() < 1e-6 && own[1] == own[0] && own[2] == 0.0, "density, all of it pine");
        let at_centre = result.positions.iter().zip(&result.canopy).find(|(p, _)| p[0].abs() < 1e-4 && p[2].abs() < 1e-4);
        let (_, centre) = at_centre.expect("the origin cell's centre vertex");
        assert_eq!(*centre, own);
        let thirds = result.canopy.iter().filter(|c| c.iter().zip(&own).all(|(a, b)| (a - b / 3.0).abs() < 1e-6)).count();
        assert_eq!(thirds, 6, "each of the cell's six corners averages it with two bare cells");
        let none = result.canopy.iter().filter(|c| **c == [0.0; 4]).count();
        assert_eq!(none, result.canopy.len() - 7, "everything else, curtains included, carries nothing");
    }
}
