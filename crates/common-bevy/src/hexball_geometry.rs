//! Hexball geometry — one function for any hex radius.
//!
//! Two-phase architecture:
//! 1. `compute_hexball_surface()` — computes all final vertex positions with
//!    overrides applied (BV Y, snapped_z). Single source of truth.
//! 2. `emit_hexball_mesh()` — pushes surface data into positions/normals/indices
//!    arrays. No computation, no overrides, just emit.
//!
//! The terrain report reads from the same `HexballSurface` struct, so reported
//! values always match rendered values.

use bevy::{
    asset::RenderAssetUsages,
    math::Vec3,
    prelude::Mesh,
    render::render_resource::PrimitiveTopology,
};
use bevy_mesh::Indices;

use std::collections::{HashMap, HashSet};

use crate::geometry::{
    flat_top_tile_center, flat_top_vertex_offsets, hex_vertex_normal, slope_adjustments,
};

// ── Surface data (phase 1 output) ────────────────────────────────────────────

/// Complete surface geometry for a single tile (r=0 or full residual).
pub struct TileSurface {
    pub verts: [Vec3; 7],
    pub q: i32,
    pub r: i32,
    pub z: i32,
}

/// A partial fan's final vertex positions.
pub struct PartialFanSurface {
    pub center: Vec3,
    /// 4 outer vertices [ov0, ov1, ov2, ov3] — inner vertex Y overrides applied.
    pub outer: [Vec3; 4],
    pub surviving_triangles: [u8; 3],
    pub q: i32,
    pub r: i32,
    pub z: i32,
}

/// All computed surface data for a hexball, before mesh emission.
/// Every Vec3 is the final rendered position — no further overrides.
pub struct HexballSurface {
    pub hex_center: Option<Vec3>,
    pub hex_boundary: Option<[Vec3; 6]>,
    pub partial_fans: Vec<PartialFanSurface>,
    pub full_tiles: Vec<TileSurface>,
    /// Tile positions absorbed by this hexball's inscribed hex (no surface geometry).
    /// Used by the skirt pass to skip internal edges that face the inscribed hex.
    pub absorbed: HashSet<(i32, i32)>,
}

/// Raw mesh geometry output.
pub struct HexballGeometry {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

// ── Public types for chunk decimation ────────────────────────────────────────

pub struct HexballDecimation {
    pub center_q: i32,
    pub center_r: i32,
    pub center_z: i32,
    pub radius: u32,
}

pub struct ChunkDecimation {
    pub hexballs: Vec<HexballDecimation>,
    pub survivors: Vec<(i32, i32, i32)>,
    /// Chunk-level effective z: absorbed tile positions → clamped z values.
    /// Merged from all hexballs. Geometry uses this for neighbor lookups
    /// so fans blend toward compressed z instead of original z.
    pub effective_z: std::collections::HashMap<(i32, i32), i32>,
}

/// Perimeter edge of a chunk surface, keyed by canonical grid vertex IDs.
/// Stored per-chunk so neighboring chunks can match against it for cross-chunk skirts.
#[derive(Clone)]
pub struct PerimeterEdge {
    pub id_a: (i32, i32),
    pub pos_a: Vec3,
    pub pos_b: Vec3,
}

/// All perimeter edges of a chunk that had no intra-chunk match.
/// Keyed by sorted pair of grid vertex IDs.
#[derive(Clone, Default)]
pub struct ChunkPerimeterEdges {
    pub edges: HashMap<((i32, i32), (i32, i32)), PerimeterEdge>,
}

/// Diagnostic counts from the skirt stitching pass.
#[derive(Debug, Default)]
pub struct SkirtStats {
    pub intra_chunk_quads: u32,
    pub cross_chunk_quads: u32,
    pub unmatched_edges: u32,
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Build a combined Bevy `Mesh` from a chunk decimation plan.
///
/// `neighbor_perimeters`: perimeter edges from already-built neighboring chunks.
/// Cross-chunk skirts are emitted where this chunk's perimeter edges match a neighbor's.
/// Returns the mesh and this chunk's own perimeter edges (for neighbors to use later).
pub fn build_chunk_mesh(
    decimation: &ChunkDecimation,
    hex_radius: f32,
    rise: f32,
    chunk_origin: Vec3,
    tile_z: &impl Fn(i32, i32) -> Option<i32>,
    neighbor_perimeters: &[&ChunkPerimeterEdges],
) -> (Mesh, ChunkPerimeterEdges, SkirtStats) {
    // Unified lookup: effective_z for absorbed tiles, original_z for everything else.
    let effective_tile_z = |q: i32, r: i32| -> Option<i32> {
        if let Some(&ez) = decimation.effective_z.get(&(q, r)) {
            Some(ez)
        } else {
            tile_z(q, r)
        }
    };

    // Phase 1: Compute all surfaces
    let mut surfaces = Vec::new();
    for hb in &decimation.hexballs {
        surfaces.push(compute_hexball_surface(
            hb.center_q, hb.center_r, hb.center_z, hb.radius,
            hex_radius, rise, &effective_tile_z,
        ));
    }
    for &(q, r, z) in &decimation.survivors {
        surfaces.push(compute_hexball_surface(
            q, r, z, 0, hex_radius, rise, &effective_tile_z,
        ));
    }

    // Phase 2: Emit all surface geometry
    let mut combined = HexballGeometry {
        positions: Vec::new(),
        normals: Vec::new(),
        indices: Vec::new(),
    };
    for surface in &surfaces {
        merge_geometry(&mut combined, &emit_hexball_mesh(surface, chunk_origin));
    }

    // Phase 3: Chunk-level skirt stitching (intra-chunk + cross-chunk)
    let (own_perimeter, skirt_stats) = emit_chunk_skirts(&surfaces, neighbor_perimeters, chunk_origin, &mut combined);

    let vert_count = combined.positions.len();
    let verts: Vec<Vec3> = combined.positions.iter().map(|p| Vec3::from_array(*p)).collect();
    let norms: Vec<Vec3> = combined.normals.iter().map(|n| Vec3::from_array(*n)).collect();

    let mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, verts)
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_UV_0,
        (0..vert_count).map(|_| [0.0f32, 0.0]).collect::<Vec<[f32; 2]>>(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, norms)
    .with_inserted_indices(Indices::U32(combined.indices));

    (mesh, own_perimeter, skirt_stats)
}

/// Produce mesh geometry for a hexball of the given radius.
pub fn compute_hexball_geometry(
    center_q: i32, center_r: i32, center_z: i32, radius: u32,
    hex_radius: f32, rise: f32, chunk_origin: Vec3,
    tile_z: &impl Fn(i32, i32) -> Option<i32>,
) -> HexballGeometry {
    let surface = compute_hexball_surface(center_q, center_r, center_z, radius, hex_radius, rise, tile_z);
    emit_hexball_mesh(&surface, chunk_origin)
}

/// Phase 1: Compute all final vertex positions for a hexball.
///
/// Every Vec3 in the returned struct is the final rendered position.
/// BV Y overrides, snapped_z fan centers — all applied here.
/// The terrain report can read this struct directly.
pub fn compute_hexball_surface(
    center_q: i32, center_r: i32, center_z: i32, radius: u32,
    hex_radius: f32, rise: f32,
    tile_z: &impl Fn(i32, i32) -> Option<i32>,
) -> HexballSurface {
    assert!(radius == 0 || radius % 2 == 1, "hexball radius must be 0 or odd, got {radius}");

    if radius == 0 {
        let verts = tile_vertices(center_q, center_r, center_z, hex_radius, rise, tile_z);
        return HexballSurface {
            hex_center: None,
            hex_boundary: None,
            partial_fans: Vec::new(),
            full_tiles: vec![TileSurface { verts, q: center_q, r: center_r, z: center_z }],
            absorbed: HashSet::new(),
        };
    }

    let hb = common::hex_decimate::decimate_hexball(
        center_q, center_r, radius, u32::MAX, tile_z,
    ).expect("all tiles within hexball must exist for geometry generation");

    // Inscribed hex center
    let center_y = center_z as f32 * rise + rise;
    let (ccx, ccz) = flat_top_tile_center(center_q, center_r, hex_radius);
    let hex_center = Vec3::new(ccx, center_y, ccz);

    // Boundary vertices
    let hex_boundary: [Vec3; 6] = std::array::from_fn(|i| {
        let (tq, tr, vi) = hb.boundary_tiles[i];
        let tz = tile_z(tq, tr).unwrap_or(center_z);
        tile_vertex_pos(tq, tr, tz, vi as usize, hex_radius, rise, tile_z)
    });

    // Partial fans — use effective z from the lookup for tile height and blending,
    // then snap inner vertices and center onto the inscribed hex surface.
    let partial_fans: Vec<PartialFanSurface> = hb.partial_residuals.iter().map(|pr| {
        let ez = tile_z(pr.q, pr.r).unwrap_or(pr.original_z);
        let tv = tile_vertices(pr.q, pr.r, ez, hex_radius, rise, tile_z);
        let st = pr.surviving_triangles;
        let edge = st[1] as usize;
        let edge_next = (edge + 1) % 6;
        let ov_indices = [st[0] as usize, st[1] as usize, st[2] as usize, (st[2] as usize + 1) % 6];

        let mut outer = [tv[ov_indices[0]], tv[ov_indices[1]], tv[ov_indices[2]], tv[ov_indices[3]]];
        // Snap inner vertices to BV heights (T-junction resolution)
        outer[0].y = hex_boundary[edge].y;
        outer[3].y = hex_boundary[edge_next].y;

        // Snap fan center onto the inscribed hex edge
        let bv_e = hex_boundary[edge];
        let bv_e1 = hex_boundary[edge_next];
        let edge_dx = bv_e1.x - bv_e.x;
        let edge_dz = bv_e1.z - bv_e.z;
        let edge_len_sq = edge_dx * edge_dx + edge_dz * edge_dz;
        let t = if edge_len_sq > 1e-10 {
            let px = tv[6].x - bv_e.x;
            let pz = tv[6].z - bv_e.z;
            (px * edge_dx + pz * edge_dz) / edge_len_sq
        } else {
            0.5
        };
        let center = Vec3::new(tv[6].x, bv_e.y + t * (bv_e1.y - bv_e.y), tv[6].z);

        PartialFanSurface {
            center, outer, surviving_triangles: st,
            q: pr.q, r: pr.r, z: ez,
        }
    }).collect();

    // Full residual tiles — use effective z from the lookup.
    let full_tiles: Vec<TileSurface> = hb.full_residuals.iter().map(|fr| {
        let ez = tile_z(fr.q, fr.r).unwrap_or(fr.z);
        let verts = tile_vertices(fr.q, fr.r, ez, hex_radius, rise, tile_z);
        TileSurface { verts, q: fr.q, r: fr.r, z: ez }
    }).collect();

    // Absorbed tiles: all hexball tiles that aren't partial fans or full residuals
    let mut absorbed = HashSet::new();
    {
        let n = radius as i32;
        for dq in -n..=n {
            for dr in (-n).max(-dq - n)..=n.min(-dq + n) {
                absorbed.insert((center_q + dq, center_r + dr));
            }
        }
        for pr in &hb.partial_residuals { absorbed.remove(&(pr.q, pr.r)); }
        for fr in &hb.full_residuals { absorbed.remove(&(fr.q, fr.r)); }
    }

    HexballSurface {
        hex_center: Some(hex_center),
        hex_boundary: Some(hex_boundary),
        partial_fans,
        full_tiles,
        absorbed,
    }
}

// ── Phase 2: Emit mesh from surface data ─────────────────────────────────────

fn emit_hexball_mesh(surface: &HexballSurface, chunk_origin: Vec3) -> HexballGeometry {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut indices = Vec::new();

    // Inscribed hex
    if let (Some(center), Some(bv)) = (&surface.hex_center, &surface.hex_boundary) {
        let inner_hex: [Vec3; 7] = {
            let mut v = [Vec3::ZERO; 7];
            for i in 0..6 { v[i] = bv[i]; }
            v[6] = *center;
            v
        };
        positions.push((*center - chunk_origin).into());
        normals.push(hex_vertex_normal(&inner_hex, 6).into());
        for i in 0..6 {
            positions.push((bv[i] - chunk_origin).into());
            normals.push(hex_vertex_normal(&inner_hex, i).into());
        }
        for i in 0..6u32 {
            indices.extend([0, 1 + ((i + 1) % 6), 1 + i]);
        }
    }

    // Partial fans
    for fan in &surface.partial_fans {
        let fan_base = positions.len() as u32;
        positions.push((fan.center - chunk_origin).into());
        for p in &fan.outer { positions.push((*p - chunk_origin).into()); }

        indices.extend([fan_base, fan_base + 2, fan_base + 1]);
        indices.extend([fan_base, fan_base + 3, fan_base + 2]);
        indices.extend([fan_base, fan_base + 4, fan_base + 3]);

        let fn0 = triangle_normal(fan.center, fan.outer[1], fan.outer[0]);
        let fn1 = triangle_normal(fan.center, fan.outer[2], fan.outer[1]);
        let fn2 = triangle_normal(fan.center, fan.outer[3], fan.outer[2]);
        normals.push(avg_normal(&[fn0, fn1, fn2]).into());
        normals.push(fn0.into());
        normals.push(avg_normal(&[fn0, fn1]).into());
        normals.push(avg_normal(&[fn1, fn2]).into());
        normals.push(fn2.into());
    }

    // Full residual tiles
    for tile in &surface.full_tiles {
        let base = positions.len() as u32;
        positions.push((tile.verts[6] - chunk_origin).into());
        normals.push(hex_vertex_normal(&tile.verts, 6).into());
        for i in 0..6 {
            positions.push((tile.verts[i] - chunk_origin).into());
            normals.push(hex_vertex_normal(&tile.verts, i).into());
        }
        for i in 0..6u32 {
            indices.extend([base, base + 1 + ((i + 1) % 6), base + 1 + i]);
        }
    }

    HexballGeometry { positions, normals, indices }
}

// ── Vertex helpers ───────────────────────────────────────────────────────────

fn tile_vertices(
    q: i32, r: i32, z: i32,
    hex_radius: f32, rise: f32,
    tile_z: &impl Fn(i32, i32) -> Option<i32>,
) -> [Vec3; 7] {
    let (cx, cz) = flat_top_tile_center(q, r, hex_radius);
    let offsets = flat_top_vertex_offsets(hex_radius);
    let adj = slope_adjustments(z, rise, |dir_idx| {
        let dir = qrz::DIRECTIONS[dir_idx];
        tile_z(q + dir.q, r + dir.r)
    });
    let base_y = z as f32 * rise + rise;
    let mut verts = [Vec3::ZERO; 7];
    for i in 0..6 {
        verts[i] = Vec3::new(cx + offsets[i].0, base_y + adj[i], cz + offsets[i].1);
    }
    verts[6] = Vec3::new(cx, base_y, cz);
    verts
}

fn tile_vertex_pos(
    q: i32, r: i32, z: i32, vertex_idx: usize,
    hex_radius: f32, rise: f32,
    tile_z: &impl Fn(i32, i32) -> Option<i32>,
) -> Vec3 {
    let (cx, cz) = flat_top_tile_center(q, r, hex_radius);
    let offsets = flat_top_vertex_offsets(hex_radius);
    let (ox, oz) = offsets[vertex_idx];
    let adj = slope_adjustments(z, rise, |dir_idx| {
        let dir = qrz::DIRECTIONS[dir_idx];
        tile_z(q + dir.q, r + dir.r)
    });
    Vec3::new(cx + ox, z as f32 * rise + rise + adj[vertex_idx], cz + oz)
}

// ── Chunk-level skirt stitching ──────────────────────────────────────────────

/// Integer grid identity for a hex vertex. Two tiles sharing the same physical
/// vertex produce identical `(a, b)` regardless of which `(q, r, vidx)` is used.
/// Uses the doubled-coordinate system from hex_decimate.
pub fn grid_vertex_id(q: i32, r: i32, vidx: usize) -> (i32, i32) {
    const VX2: [i32; 6] = [1, 2, 1, -1, -2, -1];
    const VZ2: [i32; 6] = [-1, 0, 1, 1, 0, -1];
    (3 * q + VX2[vidx], q + 2 * r + VZ2[vidx])
}

/// Emit skirt quads between surfaces with different Y at shared perimeter edges.
/// Handles both intra-chunk matches and cross-chunk matches (via neighbor perimeters).
/// Returns this chunk's unmatched perimeter edges and diagnostic stats.
fn emit_chunk_skirts(
    surfaces: &[HexballSurface],
    neighbor_perimeters: &[&ChunkPerimeterEdges],
    chunk_origin: Vec3,
    combined: &mut HexballGeometry,
) -> (ChunkPerimeterEdges, SkirtStats) {
    type VertexId = (i32, i32);
    type EdgeKey = (VertexId, VertexId);

    struct SkirtEdge {
        local: bool, // true = from this chunk's surfaces, false = from neighbor
        id_a: VertexId,
        pos_a: Vec3,
        pos_b: Vec3,
    }

    let mut edge_map: HashMap<EdgeKey, Vec<SkirtEdge>> = HashMap::new();

    // Register this chunk's perimeter edges
    for surface in surfaces {
        for fan in &surface.partial_fans {
            let st = fan.surviving_triangles;
            let ov = [st[0] as usize, st[1] as usize, st[2] as usize, (st[2] as usize + 1) % 6];
            for i in 0..3 {
                let vi = ov[i];
                let dir = (10 - vi) % 6;
                let d = qrz::DIRECTIONS[dir];
                if surface.absorbed.contains(&(fan.q + d.q, fan.r + d.r)) { continue; }

                let id_a = grid_vertex_id(fan.q, fan.r, vi);
                let id_b = grid_vertex_id(fan.q, fan.r, (vi + 1) % 6);
                let key = if id_a <= id_b { (id_a, id_b) } else { (id_b, id_a) };
                edge_map.entry(key).or_default().push(SkirtEdge {
                    local: true, id_a, pos_a: fan.outer[i], pos_b: fan.outer[i + 1],
                });
            }
        }
        for tile in &surface.full_tiles {
            for i in 0..6 {
                let dir = (10 - i) % 6;
                let d = qrz::DIRECTIONS[dir];
                if surface.absorbed.contains(&(tile.q + d.q, tile.r + d.r)) { continue; }

                let id_a = grid_vertex_id(tile.q, tile.r, i);
                let id_b = grid_vertex_id(tile.q, tile.r, (i + 1) % 6);
                let key = if id_a <= id_b { (id_a, id_b) } else { (id_b, id_a) };
                edge_map.entry(key).or_default().push(SkirtEdge {
                    local: true, id_a, pos_a: tile.verts[i], pos_b: tile.verts[(i + 1) % 6],
                });
            }
        }
    }

    // Add neighbor perimeter edges (non-local)
    for neighbor in neighbor_perimeters {
        for (&key, pe) in &neighbor.edges {
            edge_map.entry(key).or_default().push(SkirtEdge {
                local: false, id_a: pe.id_a, pos_a: pe.pos_a, pos_b: pe.pos_b,
            });
        }
    }

    // Emit skirts and collect unmatched local edges
    let mut own_perimeter = ChunkPerimeterEdges::default();
    let mut stats = SkirtStats::default();

    for (key, entries) in &edge_map {
        let has_local = entries.iter().any(|e| e.local);
        if !has_local { continue; } // not our responsibility

        let has_match = entries.len() >= 2
            && !(entries.iter().all(|e| e.local)
                && entries.iter().map(|e| (e.pos_a.y, e.pos_b.y))
                    .all(|y| {
                        let first = entries[0].pos_a.y;
                        let first_b = entries[0].pos_b.y;
                        (y.0 - first).abs() < 1e-5 && (y.1 - first_b).abs() < 1e-5
                    }));

        if entries.len() < 2 || !has_match {
            // Unmatched local edge — export for neighbor chunks
            if let Some(local) = entries.iter().find(|e| e.local) {
                own_perimeter.edges.insert(*key, PerimeterEdge {
                    id_a: local.id_a, pos_a: local.pos_a, pos_b: local.pos_b,
                });
            }
            stats.unmatched_edges += 1;
            continue;
        }

        let is_cross_chunk = entries.iter().any(|e| !e.local);

        // Matched edge — compute skirt Y from all entries
        let mut max_y0 = f32::NEG_INFINITY;
        let mut min_y0 = f32::INFINITY;
        let mut max_y1 = f32::NEG_INFINITY;
        let mut min_y1 = f32::INFINITY;

        for e in entries {
            let (y0, y1) = if e.id_a == key.0 {
                (e.pos_a.y, e.pos_b.y)
            } else {
                (e.pos_b.y, e.pos_a.y)
            };
            max_y0 = max_y0.max(y0);
            min_y0 = min_y0.min(y0);
            max_y1 = max_y1.max(y1);
            min_y1 = min_y1.min(y1);
        }

        if (max_y0 - min_y0) < 1e-5 && (max_y1 - min_y1) < 1e-5 { continue; }

        if is_cross_chunk {
            stats.cross_chunk_quads += 1;
        } else {
            stats.intra_chunk_quads += 1;
        }

        // Use xz from a local entry, oriented to match canonical key
        let local_e = entries.iter().find(|e| e.local).unwrap();
        let (p0, p1) = if local_e.id_a == key.0 {
            (local_e.pos_a, local_e.pos_b)
        } else {
            (local_e.pos_b, local_e.pos_a)
        };

        let top0 = Vec3::new(p0.x, max_y0, p0.z);
        let top1 = Vec3::new(p1.x, max_y1, p1.z);
        let bot0 = Vec3::new(p0.x, min_y0, p0.z);
        let bot1 = Vec3::new(p1.x, min_y1, p1.z);

        let edge_dir = Vec3::new(p1.x - p0.x, 0.0, p1.z - p0.z);
        let outward = edge_dir.cross(Vec3::new(0.0, -1.0, 0.0));
        let n: [f32; 3] = if outward.length_squared() > 1e-10 {
            outward.normalize().into()
        } else {
            Vec3::Y.into()
        };

        let base = combined.positions.len() as u32;
        let v0: [f32; 3] = (top0 - chunk_origin).into();
        let v1: [f32; 3] = (top1 - chunk_origin).into();
        let v2: [f32; 3] = (bot1 - chunk_origin).into();
        let v3: [f32; 3] = (bot0 - chunk_origin).into();
        combined.positions.extend([v0, v1, v2, v3]);
        combined.normals.extend([n; 4]);
        combined.indices.extend([base, base + 1, base + 2]);
        combined.indices.extend([base, base + 2, base + 3]);
    }

    (own_perimeter, stats)
}

/// Match a chunk's unmatched perimeter edges against a newly available neighbor's
/// perimeter. Returns skirt geometry to append and the matched edge keys to remove.
pub fn match_cross_chunk_skirts(
    unmatched: &ChunkPerimeterEdges,
    new_neighbor: &ChunkPerimeterEdges,
    chunk_origin: Vec3,
) -> (HexballGeometry, Vec<((i32, i32), (i32, i32))>) {
    let mut geom = HexballGeometry {
        positions: Vec::new(),
        normals: Vec::new(),
        indices: Vec::new(),
    };
    let mut matched_keys = Vec::new();

    for (&key, local_edge) in &unmatched.edges {
        let Some(remote_edge) = new_neighbor.edges.get(&key) else { continue };

        let (ly0, ly1) = if local_edge.id_a == key.0 {
            (local_edge.pos_a.y, local_edge.pos_b.y)
        } else {
            (local_edge.pos_b.y, local_edge.pos_a.y)
        };
        let (ry0, ry1) = if remote_edge.id_a == key.0 {
            (remote_edge.pos_a.y, remote_edge.pos_b.y)
        } else {
            (remote_edge.pos_b.y, remote_edge.pos_a.y)
        };

        let (max_y0, min_y0) = (ly0.max(ry0), ly0.min(ry0));
        let (max_y1, min_y1) = (ly1.max(ry1), ly1.min(ry1));
        if (max_y0 - min_y0) < 1e-5 && (max_y1 - min_y1) < 1e-5 { continue; }

        let (p0, p1) = if local_edge.id_a == key.0 {
            (local_edge.pos_a, local_edge.pos_b)
        } else {
            (local_edge.pos_b, local_edge.pos_a)
        };

        let top0 = Vec3::new(p0.x, max_y0, p0.z);
        let top1 = Vec3::new(p1.x, max_y1, p1.z);
        let bot0 = Vec3::new(p0.x, min_y0, p0.z);
        let bot1 = Vec3::new(p1.x, min_y1, p1.z);

        let edge_dir = Vec3::new(p1.x - p0.x, 0.0, p1.z - p0.z);
        let outward = edge_dir.cross(Vec3::new(0.0, -1.0, 0.0));
        let n: [f32; 3] = if outward.length_squared() > 1e-10 {
            outward.normalize().into()
        } else {
            Vec3::Y.into()
        };

        let base = geom.positions.len() as u32;
        let v0: [f32; 3] = (top0 - chunk_origin).into();
        let v1: [f32; 3] = (top1 - chunk_origin).into();
        let v2: [f32; 3] = (bot1 - chunk_origin).into();
        let v3: [f32; 3] = (bot0 - chunk_origin).into();
        geom.positions.extend([v0, v1, v2, v3]);
        geom.normals.extend([n; 4]);
        geom.indices.extend([base, base + 1, base + 2]);
        geom.indices.extend([base, base + 2, base + 3]);

        matched_keys.push(key);
    }

    (geom, matched_keys)
}

/// Append skirt geometry to an existing mesh's vertex/index buffers.
pub fn append_geometry_to_mesh(mesh: &mut Mesh, geom: &HexballGeometry) {
    use bevy_mesh::VertexAttributeValues;

    if geom.positions.is_empty() { return; }

    let base_vertex = mesh.count_vertices() as u32;

    if let Some(VertexAttributeValues::Float32x3(data)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    {
        data.extend_from_slice(&geom.positions);
    }
    if let Some(VertexAttributeValues::Float32x3(data)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_NORMAL)
    {
        data.extend_from_slice(&geom.normals);
    }
    if let Some(VertexAttributeValues::Float32x2(data)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_UV_0)
    {
        data.extend((0..geom.positions.len()).map(|_| [0.0f32, 0.0]));
    }
    if let Some(Indices::U32(data)) = mesh.indices_mut() {
        data.extend(geom.indices.iter().map(|i| i + base_vertex));
    }
}

// ── Shared helpers ───────────────────────────────────────────────────────────

fn triangle_normal(a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let n = (b - a).cross(c - a);
    if n.length_squared() > 1e-10 { n.normalize() } else { Vec3::Y }
}

fn avg_normal(normals: &[Vec3]) -> Vec3 {
    let sum: Vec3 = normals.iter().copied().sum();
    if sum.length_squared() > 1e-10 { sum.normalize() } else { Vec3::Y }
}

fn merge_geometry(target: &mut HexballGeometry, source: &HexballGeometry) {
    let offset = target.positions.len() as u32;
    target.positions.extend_from_slice(&source.positions);
    target.normals.extend_from_slice(&source.normals);
    target.indices.extend(source.indices.iter().map(|i| i + offset));
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_tile_z(elevations: &HashMap<(i32, i32), i32>) -> impl Fn(i32, i32) -> Option<i32> + '_ {
        move |q, r| elevations.get(&(q, r)).copied()
    }

    fn flat_tiles(cq: i32, cr: i32, radius: u32, z: i32) -> HashMap<(i32, i32), i32> {
        let n = (radius + 1) as i32;
        let mut map = HashMap::new();
        for dq in -n..=n {
            for dr in (-n).max(-dq - n)..=n.min(-dq + n) {
                map.insert((cq + dq, cr + dr), z);
            }
        }
        map
    }

    #[test]
    fn r0_matches_compute_tile_geometry() {
        let mut elevations = HashMap::new();
        elevations.insert((0, 0), 3);
        elevations.insert((-1, 0), 3);
        elevations.insert((-1, 1), 4);
        elevations.insert((0, 1), 2);
        elevations.insert((1, 0), 3);
        elevations.insert((1, -1), 3);
        elevations.insert((0, -1), 5);

        let tiles = vec![qrz::Qrz { q: 0, r: 0, z: 3 }];
        let tile_geom = crate::geometry::compute_tile_geometry(
            &tiles, &elevations, 1.0, 0.8, Vec3::ZERO,
        );
        let tile_z = make_tile_z(&elevations);
        let hexball = compute_hexball_geometry(0, 0, 3, 0, 1.0, 0.8, Vec3::ZERO, &tile_z);

        // Compare surface geometry only (7 verts, 18 indices). Skirts disabled.
        assert!(hexball.positions.len() >= 7);
        for i in 0..7 {
            let a = &tile_geom.positions[i];
            let b = &hexball.positions[i];
            assert!((a[0]-b[0]).abs() < 1e-5 && (a[1]-b[1]).abs() < 1e-5 && (a[2]-b[2]).abs() < 1e-5,
                "pos mismatch at {i}: {a:?} vs {b:?}");
        }
        assert_eq!(&tile_geom.indices[..18], &hexball.indices[..18], "surface indices differ");
    }

    #[test]
    fn r0_triangle_count() {
        let elevations = flat_tiles(0, 0, 0, 0);
        let tile_z = make_tile_z(&elevations);
        let hb = compute_hexball_geometry(0, 0, 0, 0, 1.0, 0.8, Vec3::ZERO, &tile_z);
        assert_eq!(hb.indices.len() / 3, 6);
    }

    #[test]
    fn r1_triangle_count() {
        let elevations = flat_tiles(0, 0, 1, 0);
        let tile_z = make_tile_z(&elevations);
        let hb = compute_hexball_geometry(0, 0, 0, 1, 1.0, 0.8, Vec3::ZERO, &tile_z);
        assert_eq!(hb.indices.len() / 3, 24, "r=1 should be 24 tri");
    }

    #[test]
    fn r3_triangle_count() {
        let elevations = flat_tiles(0, 0, 3, 0);
        let tile_z = make_tile_z(&elevations);
        let hb = compute_hexball_geometry(0, 0, 0, 3, 1.0, 0.8, Vec3::ZERO, &tile_z);
        assert_eq!(hb.indices.len() / 3, 78, "r=3 should be 78 tri");
    }

    #[test]
    fn r5_triangle_count() {
        let elevations = flat_tiles(0, 0, 5, 0);
        let tile_z = make_tile_z(&elevations);
        let hb = compute_hexball_geometry(0, 0, 0, 5, 1.0, 0.8, Vec3::ZERO, &tile_z);
        assert_eq!(hb.indices.len() / 3, 168, "r=5 should be 168 tri");
    }

    #[test]
    fn r1_flat_terrain_all_same_y() {
        let elevations = flat_tiles(0, 0, 1, 0);
        let tile_z = make_tile_z(&elevations);
        let hb = compute_hexball_geometry(0, 0, 0, 1, 1.0, 0.8, Vec3::ZERO, &tile_z);
        for (i, pos) in hb.positions.iter().enumerate() {
            assert!((pos[1] - 0.8).abs() < 1e-5, "vertex {i} y={} expected 0.8", pos[1]);
        }
    }

    #[test]
    fn r3_flat_terrain_all_same_y() {
        let elevations = flat_tiles(0, 0, 3, 0);
        let tile_z = make_tile_z(&elevations);
        let hb = compute_hexball_geometry(0, 0, 0, 3, 1.0, 0.8, Vec3::ZERO, &tile_z);
        for (i, pos) in hb.positions.iter().enumerate() {
            assert!((pos[1] - 0.8).abs() < 1e-5, "vertex {i} y={} expected 0.8", pos[1]);
        }
    }

    #[test]
    fn r1_ccw_winding() {
        let elevations = flat_tiles(0, 0, 1, 0);
        let tile_z = make_tile_z(&elevations);
        let hb = compute_hexball_geometry(0, 0, 0, 1, 1.0, 0.8, Vec3::ZERO, &tile_z);
        for tri in hb.indices.chunks(3) {
            let a = Vec3::from(hb.positions[tri[0] as usize]);
            let b = Vec3::from(hb.positions[tri[1] as usize]);
            let c = Vec3::from(hb.positions[tri[2] as usize]);
            let n = (b - a).cross(c - a);
            assert!(n.y >= -1e-6, "downward normal: {n:?}");
        }
    }

    #[test]
    fn r3_ccw_winding() {
        let elevations = flat_tiles(0, 0, 3, 0);
        let tile_z = make_tile_z(&elevations);
        let hb = compute_hexball_geometry(0, 0, 0, 3, 1.0, 0.8, Vec3::ZERO, &tile_z);
        for tri in hb.indices.chunks(3) {
            let a = Vec3::from(hb.positions[tri[0] as usize]);
            let b = Vec3::from(hb.positions[tri[1] as usize]);
            let c = Vec3::from(hb.positions[tri[2] as usize]);
            let n = (b - a).cross(c - a);
            assert!(n.y >= -1e-6, "downward normal: {n:?}");
        }
    }

    #[test]
    fn deterministic() {
        let elevations = flat_tiles(0, 0, 3, 0);
        let tile_z = make_tile_z(&elevations);
        let a = compute_hexball_geometry(0, 0, 0, 3, 1.0, 0.8, Vec3::ZERO, &tile_z);
        let b = compute_hexball_geometry(0, 0, 0, 3, 1.0, 0.8, Vec3::ZERO, &tile_z);
        assert_eq!(a.positions, b.positions);
        assert_eq!(a.normals, b.normals);
        assert_eq!(a.indices, b.indices);
    }

    #[test]
    fn r1_perimeter_stitching() {
        let elevations = flat_tiles(0, 0, 4, 0);
        let tile_z = make_tile_z(&elevations);
        let a = compute_hexball_geometry(0, 0, 0, 1, 1.0, 0.8, Vec3::ZERO, &tile_z);
        let b = compute_hexball_geometry(2, 1, 0, 1, 1.0, 0.8, Vec3::ZERO, &tile_z);
        assert_shared_vertices(&a, &b, 2);
    }

    #[test]
    fn r3_perimeter_stitching() {
        let elevations = flat_tiles(0, 0, 10, 0);
        let tile_z = make_tile_z(&elevations);
        let a = compute_hexball_geometry(0, 0, 0, 3, 1.0, 0.8, Vec3::ZERO, &tile_z);
        let b = compute_hexball_geometry(4, 3, 0, 3, 1.0, 0.8, Vec3::ZERO, &tile_z);
        assert_shared_vertices(&a, &b, 2);
    }

    fn assert_shared_vertices(a: &HexballGeometry, b: &HexballGeometry, min_shared: usize) {
        let eps = 1e-5;
        let mut shared = 0;
        for pa in &a.positions {
            for pb in &b.positions {
                if (pa[0]-pb[0]).abs() < eps && (pa[1]-pb[1]).abs() < eps && (pa[2]-pb[2]).abs() < eps {
                    shared += 1;
                    break;
                }
            }
        }
        assert!(shared >= min_shared,
            "expected at least {min_shared} shared vertices, found {shared}");
    }

    #[test]
    #[should_panic(expected = "hexball radius must be 0 or odd")]
    fn even_radius_panics() {
        let elevations = HashMap::new();
        let tile_z = make_tile_z(&elevations);
        compute_hexball_geometry(0, 0, 0, 2, 1.0, 0.8, Vec3::ZERO, &tile_z);
    }

    #[test]
    fn fan_inner_vertices_agree_with_bv() {
        // With effective_z lookup, fan inner vertices and BV heights should
        // agree naturally from standard blending — no override needed.
        let mut elevations = HashMap::new();
        for dq in -3..=3 {
            for dr in (-3).max(-dq - 3)..=(3).min(-dq + 3) {
                elevations.insert((dq, dr), dq + 3);
            }
        }
        // Build effective_z for this hexball
        let tile_z_fn = make_tile_z(&elevations);
        let hb = common::hex_decimate::decimate_hexball(0, 0, 1, u32::MAX, &tile_z_fn).unwrap();
        let mut effective = elevations.clone();
        for (&(q, r), &ez) in &hb.effective_z {
            effective.insert((q, r), ez);
        }
        let eff_tile_z = make_tile_z(&effective);
        let surface = compute_hexball_surface(0, 0, hb.center_z, 1, 1.0, 0.8, &eff_tile_z);
        let bv = surface.hex_boundary.unwrap();
        for fan in &surface.partial_fans {
            let edge = fan.surviving_triangles[1] as usize;
            let edge_next = (edge + 1) % 6;
            assert!((fan.outer[0].y - bv[edge].y).abs() < 1e-4,
                "fan ov[0] y={:.4} != bv[{edge}] y={:.4}", fan.outer[0].y, bv[edge].y);
            assert!((fan.outer[3].y - bv[edge_next].y).abs() < 1e-4,
                "fan ov[3] y={:.4} != bv[{edge_next}] y={:.4}", fan.outer[3].y, bv[edge_next].y);
        }
    }

    #[test]
    fn no_bowl_on_majority_slope() {
        // 5 tiles at z=10, 2 at z=9. Median picks center_z=10, no bowl.
        let mut elevations = HashMap::new();
        elevations.insert((0, 0), 10);
        elevations.insert((-1, 0), 9);
        elevations.insert((-1, 1), 10);
        elevations.insert((0, 1), 10);
        elevations.insert((1, 0), 10);
        elevations.insert((1, -1), 10);
        elevations.insert((0, -1), 9);
        for dq in -2..=2 {
            for dr in (-2).max(-dq - 2)..=(2).min(-dq + 2) {
                elevations.entry((dq, dr)).or_insert(10);
            }
        }
        let tile_z = make_tile_z(&elevations);
        // Get center_z from decimation (should be 10 via median)
        let dec = common::hex_decimate::decimate_hexball(0, 0, 1, 1, &tile_z).unwrap();
        assert_eq!(dec.center_z, 10);
        // Build effective lookup
        let mut eff = elevations.clone();
        for (&(q, r), &ez) in &dec.effective_z { eff.insert((q, r), ez); }
        let eff_z = make_tile_z(&eff);
        let hb = compute_hexball_geometry(0, 0, 10, 1, 1.0, 0.8, Vec3::ZERO, &eff_z);
        let min_y = 9.0 * 0.8 + 0.8; // z=9 base = 8.0
        for (i, pos) in hb.positions.iter().enumerate() {
            assert!(pos[1] >= min_y - 1e-4,
                "vertex {i} y={:.4} below z=9 base {min_y:.4}", pos[1]);
        }
    }

}
