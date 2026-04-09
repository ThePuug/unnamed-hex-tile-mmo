use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::PrimitiveTopology,
};
use bevy_camera::primitives::Aabb;
use bevy_mesh::Indices;
use dashmap::DashMap;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use qrz::{self, Convert, Qrz};

use crate::{
    chunk::{ChunkId, loc_to_chunk},
    components::entity_type::*,
};

/// Data stored per tile.
#[derive(Clone, Copy)]
pub struct TileRecord {
    pub z: i32,
    pub typ: EntityType,
}

/// Map resource with chunk-sharded storage and flat elevation index.
///
/// Two indexes, each optimized for its access pattern:
/// - `flat`: single DashMap shard probe for elevation lookups (hot path — physics)
/// - `chunks`: O(1) chunk lookup for mesh generation and bulk eviction
///
/// Clone is O(1) — clones the Arc handles.
#[derive(Clone, Resource)]
pub struct Map {
    radius: f32,
    rise: f32,
    orientation: qrz::HexOrientation,
    /// Hot-path elevation index: single shard probe, no chunk derivation.
    flat: Arc<DashMap<(i32, i32), i32>>,
    /// Chunk-sharded storage for mesh generation and EntityType lookups.
    chunks: Arc<DashMap<ChunkId, HashMap<(i32, i32), TileRecord>>>,
    changed: Arc<AtomicBool>,
    /// Geometry-only delegate for coordinate conversion and vertex computation.
    geo: Arc<qrz::Map<()>>,
}

impl Map {
    pub fn new(map: qrz::Map<EntityType>) -> Map {
        let radius = map.radius();
        let rise = map.rise();
        let orientation = map.orientation();

        let flat: DashMap<(i32, i32), i32> = DashMap::new();
        let chunks: DashMap<ChunkId, HashMap<(i32, i32), TileRecord>> = DashMap::new();
        for (&qrz, &typ) in map.iter() {
            flat.insert((qrz.q, qrz.r), qrz.z);
            let chunk_id = loc_to_chunk(qrz);
            chunks.entry(chunk_id).or_default().insert(
                (qrz.q, qrz.r),
                TileRecord { z: qrz.z, typ },
            );
        }

        let geo = qrz::Map::<()>::new(radius, rise, orientation);

        Map {
            radius,
            rise,
            orientation,
            flat: Arc::new(flat),
            chunks: Arc::new(chunks),
            changed: Arc::new(AtomicBool::new(false)),
            geo: Arc::new(geo),
        }
    }

    pub fn insert(&self, qrz: Qrz, typ: EntityType) {
        self.flat.insert((qrz.q, qrz.r), qrz.z);
        let chunk_id = loc_to_chunk(qrz);
        self.chunks.entry(chunk_id).or_default().insert(
            (qrz.q, qrz.r),
            TileRecord { z: qrz.z, typ },
        );
        self.changed.store(true, Ordering::Relaxed);
    }

    pub fn remove(&self, qrz: Qrz) -> Option<EntityType> {
        self.flat.remove(&(qrz.q, qrz.r));
        let chunk_id = loc_to_chunk(qrz);
        let removed = self.chunks.get_mut(&chunk_id)
            .and_then(|mut bucket| bucket.remove(&(qrz.q, qrz.r)).map(|r| r.typ));
        if removed.is_some() {
            if self.chunks.get(&chunk_id).map_or(false, |b| b.is_empty()) {
                self.chunks.remove(&chunk_id);
            }
            self.changed.store(true, Ordering::Relaxed);
        }
        removed
    }

    /// O(1) chunk eviction — removes all tiles in the chunk from both indexes.
    pub fn remove_chunk(&self, chunk_id: ChunkId) {
        if let Some((_, bucket)) = self.chunks.remove(&chunk_id) {
            for &(q, r) in bucket.keys() {
                self.flat.remove(&(q, r));
            }
            self.changed.store(true, Ordering::Relaxed);
        }
    }

    /// Hot-path elevation + type lookup. Single flat-index shard probe for z,
    /// then chunk lookup for EntityType.
    pub fn get_by_qr(&self, q: i32, r: i32) -> Option<(Qrz, EntityType)> {
        let z = *self.flat.get(&(q, r))?.value();
        let chunk_id = loc_to_chunk(Qrz { q, r, z });
        let typ = self.chunks.get(&chunk_id)
            .and_then(|b| b.get(&(q, r)).map(|r| r.typ))
            .unwrap_or(EntityType::Unset);
        Some((Qrz { q, r, z }, typ))
    }

    pub fn get(&self, qrz: Qrz) -> Option<EntityType> {
        let chunk_id = loc_to_chunk(qrz);
        self.chunks.get(&chunk_id)?
            .get(&(qrz.q, qrz.r))
            .filter(|r| r.z == qrz.z)
            .map(|r| r.typ)
    }

    pub fn take_changed(&self) -> bool {
        self.changed.swap(false, Ordering::Relaxed)
    }

    pub fn force_changed(&self) {
        self.changed.store(true, Ordering::Relaxed);
    }

    pub fn rise(&self) -> f32 { self.rise }
    pub fn radius(&self) -> f32 { self.radius }
    pub fn orientation(&self) -> qrz::HexOrientation { self.orientation }

    pub fn len(&self) -> usize {
        self.chunks.iter().map(|e| e.value().len()).sum()
    }

    pub fn heap_size_estimate(&self) -> usize {
        // Rough estimate: per entry ~48 bytes (key + TileRecord + HashMap overhead)
        self.len() * 48
    }

    pub fn neighbors(&self, qrz: Qrz) -> Vec<(Qrz, EntityType)> {
        let mut result = Vec::new();
        for direction in qrz::DIRECTIONS.iter() {
            let n = qrz + *direction;
            if let Some((actual, typ)) = self.get_by_qr(n.q, n.r) {
                if (actual.z - qrz.z).abs() <= 1 {
                    result.push((actual, typ));
                }
            }
        }
        result
    }

    pub fn iter_tiles(&self) -> Vec<(Qrz, EntityType)> {
        self.chunks.iter()
            .flat_map(|entry| {
                entry.value().iter()
                    .map(|(&(q, r), rec)| (Qrz { q, r, z: rec.z }, rec.typ))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    pub fn greedy_path(&self, from: Qrz, toward: Qrz, max_steps: usize) -> Vec<Qrz> {
        let mut path = Vec::new();
        let mut current = from;

        for _ in 0..max_steps {
            if current.flat_distance(&toward) == 0 {
                break;
            }

            let best = self.neighbors(current)
                .into_iter()
                .min_by_key(|(n, _)| n.flat_distance(&toward));

            let Some((next, _)) = best else { break };

            if next.flat_distance(&toward) >= current.flat_distance(&toward) {
                break;
            }

            current = next;
            path.push(current);
        }

        path
    }

    fn vertices_with_slopes_inner(&self, qrz: Qrz, apply_slopes: bool) -> Vec<Vec3> {
        let mut verts = self.geo.vertices(qrz);
        if !apply_slopes {
            return verts;
        }

        let rise = self.rise;
        let mut vertex_adjustments: [Vec<f32>; 6] = Default::default();

        let direction_to_vertices = [
            (4, 5), (3, 4), (2, 3), (1, 2), (0, 1), (5, 0),
        ];

        for (dir_idx, direction) in qrz::DIRECTIONS.iter().enumerate() {
            let neighbor_qrz = qrz + *direction;
            if let Some((actual_neighbor_qrz, _)) = self.get_by_qr(neighbor_qrz.q, neighbor_qrz.r) {
                let elevation_diff = actual_neighbor_qrz.z - qrz.z;
                let adjustment = if elevation_diff > 0 {
                    rise * 0.5
                } else if elevation_diff < 0 {
                    rise * -0.5
                } else {
                    0.0
                };
                if adjustment != 0.0 {
                    let (v1, v2) = direction_to_vertices[dir_idx];
                    vertex_adjustments[v1].push(adjustment);
                    vertex_adjustments[v2].push(adjustment);
                }
            }
        }

        for (i, adjustments) in vertex_adjustments.iter().enumerate() {
            if let Some(&max_adj) = adjustments.iter()
                .max_by(|a, b| a.abs().partial_cmp(&b.abs()).unwrap())
            {
                verts[i].y += max_adj;
            }
        }

        verts
    }

    pub fn vertices_with_slopes(&self, qrz: Qrz, apply_slopes: bool) -> Vec<Vec3> {
        self.vertices_with_slopes_inner(qrz, apply_slopes)
    }

    pub fn hex_vertex_normal(verts: &[Vec3], vertex_idx: usize) -> Vec3 {
        let center = verts[6];
        if vertex_idx == 6 {
            let mut sum = Vec3::ZERO;
            for i in 0..6 {
                sum += (verts[(i + 1) % 6] - center).cross(verts[i] - center);
            }
            if sum.length_squared() > 1e-10 { sum.normalize() } else { Vec3::Y }
        } else {
            let j = vertex_idx;
            let n1 = (verts[(j + 1) % 6] - center).cross(verts[j] - center);
            let n2 = (verts[j] - center).cross(verts[(j + 5) % 6] - center);
            let sum = n1 + n2;
            if sum.length_squared() > 1e-10 { sum.normalize() } else { Vec3::Y }
        }
    }

    pub fn generate_chunk_mesh(&self, chunk_id: ChunkId, apply_slopes: bool) -> (Mesh, Aabb) {
        // O(1) chunk lookup instead of O(n) filter
        let chunk_tiles: Vec<Qrz> = match self.chunks.get(&chunk_id) {
            Some(bucket) => bucket.iter()
                .map(|(&(q, r), rec)| Qrz { q, r, z: rec.z })
                .collect(),
            None => Vec::new(),
        };

        // Build elevation lookup: chunk tiles + 1-ring neighbors
        let mut elevations = std::collections::HashMap::new();
        for &qrz in &chunk_tiles {
            elevations.insert((qrz.q, qrz.r), qrz.z);
            for direction in qrz::DIRECTIONS.iter() {
                let n = qrz + *direction;
                if let Some((actual, _)) = self.get_by_qr(n.q, n.r) {
                    elevations.insert((actual.q, actual.r), actual.z);
                }
            }
        }

        let chunk_origin: Vec3 = self.geo.convert(chunk_id.center());
        let geometry = if apply_slopes {
            crate::geometry::compute_tile_geometry(
                &chunk_tiles, &elevations, self.radius, self.rise, chunk_origin,
            )
        } else {
            let chunk_only: std::collections::HashMap<(i32, i32), i32> = chunk_tiles.iter()
                .map(|qrz| ((qrz.q, qrz.r), qrz.z))
                .collect();
            crate::geometry::compute_tile_geometry(
                &chunk_tiles, &chunk_only, self.radius, self.rise, chunk_origin,
            )
        };

        let mut min = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
        let mut max = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
        for pos in &geometry.positions {
            let v = Vec3::from_array(*pos);
            min = Vec3::min(min, v);
            max = Vec3::max(max, v);
        }

        let vert_count = geometry.positions.len();
        let verts: Vec<Vec3> = geometry.positions.iter().map(|p| Vec3::from_array(*p)).collect();
        let norms: Vec<Vec3> = geometry.normals.iter().map(|n| Vec3::from_array(*n)).collect();

        (
            Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD
            )
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, verts)
                .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, (0..vert_count).map(|_| [0., 0.]).collect::<Vec<[f32; 2]>>())
                .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, norms)
                .with_inserted_indices(Indices::U32(geometry.indices)),
            Aabb::from_min_max(min, max),
        )
    }
}

impl Convert<Qrz, Vec3> for Map {
    fn convert(&self, it: Qrz) -> Vec3 {
        self.geo.convert(it)
    }
}

impl Convert<Vec3, Qrz> for Map {
    fn convert(&self, it: Vec3) -> Qrz {
        self.geo.convert(it)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qrz::Qrz;

    fn make_flat_map() -> Map {
        let mut qrz_map = qrz::Map::<EntityType>::new(1.0, 0.8, qrz::HexOrientation::FlatTop);
        for q in -5..=5 {
            for r in -5..=5 {
                qrz_map.insert(Qrz { q, r, z: 0 }, EntityType::Decorator(default()));
            }
        }
        Map::new(qrz_map)
    }

    #[test]
    fn greedy_path_flat_terrain() {
        let map = make_flat_map();
        let path = map.greedy_path(
            Qrz { q: 0, r: 0, z: 0 },
            Qrz { q: 3, r: 0, z: 0 },
            10,
        );
        assert_eq!(path.len(), 3);
        assert_eq!(path.last().unwrap().flat_distance(&Qrz { q: 3, r: 0, z: 0 }), 0);
    }

    #[test]
    fn greedy_path_follows_slope() {
        let mut qrz_map = qrz::Map::<EntityType>::new(1.0, 0.8, qrz::HexOrientation::FlatTop);
        for q in 0..=4 {
            qrz_map.insert(Qrz { q, r: 0, z: q }, EntityType::Decorator(default()));
        }
        let map = Map::new(qrz_map);

        let path = map.greedy_path(
            Qrz { q: 0, r: 0, z: 0 },
            Qrz { q: 4, r: 0, z: 4 },
            10,
        );
        assert_eq!(path.len(), 4);
        assert_eq!(*path.last().unwrap(), Qrz { q: 4, r: 0, z: 4 });
    }

    #[test]
    fn greedy_path_stops_at_cliff() {
        let mut qrz_map = qrz::Map::<EntityType>::new(1.0, 0.8, qrz::HexOrientation::FlatTop);
        qrz_map.insert(Qrz { q: 0, r: 0, z: 0 }, EntityType::Decorator(default()));
        qrz_map.insert(Qrz { q: 1, r: 0, z: 0 }, EntityType::Decorator(default()));
        qrz_map.insert(Qrz { q: 2, r: 0, z: 5 }, EntityType::Decorator(default()));
        let map = Map::new(qrz_map);

        let path = map.greedy_path(
            Qrz { q: 0, r: 0, z: 0 },
            Qrz { q: 3, r: 0, z: 0 },
            10,
        );
        assert_eq!(path.len(), 1);
        assert_eq!(path[0], Qrz { q: 1, r: 0, z: 0 });
    }

    #[test]
    fn greedy_path_already_at_dest() {
        let map = make_flat_map();
        let origin = Qrz { q: 0, r: 0, z: 0 };
        let path = map.greedy_path(origin, origin, 10);
        assert!(path.is_empty());
    }

    #[test]
    fn greedy_path_max_steps_limits() {
        let map = make_flat_map();
        let path = map.greedy_path(
            Qrz { q: 0, r: 0, z: 0 },
            Qrz { q: 5, r: 0, z: 0 },
            2,
        );
        assert_eq!(path.len(), 2);
    }

    #[test]
    fn greedy_path_no_progress_stops() {
        let mut qrz_map = qrz::Map::<EntityType>::new(1.0, 0.8, qrz::HexOrientation::FlatTop);
        qrz_map.insert(Qrz { q: 0, r: 0, z: 0 }, EntityType::Decorator(default()));
        let map = Map::new(qrz_map);

        let path = map.greedy_path(
            Qrz { q: 0, r: 0, z: 0 },
            Qrz { q: 5, r: 0, z: 0 },
            10,
        );
        assert!(path.is_empty());
    }

    #[test]
    fn test_normals_consider_neighboring_hexes() {
        let mut qrz_map = qrz::Map::new(1.0, 0.8, qrz::HexOrientation::FlatTop);
        let hex1 = Qrz { q: 0, r: 0, z: 0 };
        let hex2 = Qrz { q: 1, r: 0, z: 0 };

        qrz_map.insert(hex1, EntityType::Decorator(default()));
        qrz_map.insert(hex2, EntityType::Decorator(default()));

        let map = Map::new(qrz_map);

        let hex1_verts = map.vertices_with_slopes(hex1, true);
        let shared_vertex_normal = Map::hex_vertex_normal(&hex1_verts, 1);

        assert!(shared_vertex_normal.y > 0.95,
            "Expected Y > 0.95, got {:?}", shared_vertex_normal);
        assert!(shared_vertex_normal.x.abs() < 0.3);
        assert!(shared_vertex_normal.z.abs() < 0.3);
    }

    #[test]
    fn test_generate_chunk_mesh() {
        use crate::chunk::{ChunkId, chunk_tiles, CHUNK_TILES};

        let mut qrz_map = qrz::Map::new(1.0, 0.8, qrz::HexOrientation::FlatTop);

        for (q, r) in chunk_tiles(ChunkId(0, 0)) {
            qrz_map.insert(Qrz { q, r, z: 0 }, EntityType::Decorator(default()));
        }
        for (q, r) in chunk_tiles(ChunkId(1, 1)) {
            qrz_map.insert(Qrz { q, r, z: 0 }, EntityType::Decorator(default()));
        }

        let map = Map::new(qrz_map);
        let (mesh, aabb) = map.generate_chunk_mesh(ChunkId(0, 0), true);

        let positions = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap().as_float3().unwrap();
        assert_eq!(positions.len(), CHUNK_TILES * 7);

        let indices = match mesh.indices() {
            Some(bevy_mesh::Indices::U32(idx)) => idx,
            _ => panic!("Expected U32 indices"),
        };
        assert_eq!(indices.len(), CHUNK_TILES * 6 * 3);

        assert!(aabb.min().x < aabb.max().x);
        assert!(aabb.min().z < aabb.max().z);
    }

    #[test]
    fn test_generate_chunk_mesh_filters_to_chunk() {
        use crate::chunk::{ChunkId, chunk_tiles};

        let mut qrz_map = qrz::Map::new(1.0, 0.8, qrz::HexOrientation::FlatTop);

        let tiles_00: Vec<_> = chunk_tiles(ChunkId(0, 0)).take(4).collect();
        for &(q, r) in &tiles_00 {
            qrz_map.insert(Qrz { q, r, z: 0 }, EntityType::Decorator(default()));
        }

        let tiles_11: Vec<_> = chunk_tiles(ChunkId(1, 1)).take(9).collect();
        for &(q, r) in &tiles_11 {
            qrz_map.insert(Qrz { q, r, z: 0 }, EntityType::Decorator(default()));
        }

        let map = Map::new(qrz_map);

        let (mesh_00, _) = map.generate_chunk_mesh(ChunkId(0, 0), true);
        let positions_00 = mesh_00.attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap().as_float3().unwrap();
        assert_eq!(positions_00.len(), 4 * 7);

        let (mesh_11, _) = map.generate_chunk_mesh(ChunkId(1, 1), true);
        let positions_11 = mesh_11.attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap().as_float3().unwrap();
        assert_eq!(positions_11.len(), 9 * 7);
    }
}
