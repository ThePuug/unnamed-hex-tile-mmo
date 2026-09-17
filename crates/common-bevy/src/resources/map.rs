use bevy::prelude::*;
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

/// Two indexes, each optimized for its access pattern:
/// - `flat`: single DashMap shard probe for elevation lookups (hot path — physics)
/// - `chunks`: O(1) chunk lookup for mesh generation and bulk eviction

/// Clone is O(1) — clones the Arc handles.
#[derive(Clone, Resource)]
pub struct Map {
    radius: f32,
    rise: f32,
    orientation: qrz::HexOrientation,
    /// Hot-path elevation index: single shard probe, no chunk derivation.
    flat: Arc<DashMap<(i32, i32), i32>>,
    /// Flooded tiles only: the surface water stands at, as a z-level. A
    /// tile absent here is dry.
    water: Arc<DashMap<(i32, i32), i32>>,
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
            water: Arc::new(DashMap::new()),
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

    /// Set or clear the surface water stands at over a tile. Independent of
    /// `insert`, so a tile's ground and its water may arrive in either order.
    pub fn set_water(&self, q: i32, r: i32, water: Option<i32>) {
        match water {
            Some(w) => { self.water.insert((q, r), w); }
            None => { self.water.remove(&(q, r)); }
        }
        self.changed.store(true, Ordering::Relaxed);
    }

    /// The surface water stands at over a tile, or None where it is dry.
    pub fn water_at(&self, q: i32, r: i32) -> Option<i32> {
        self.water.get(&(q, r)).map(|w| *w)
    }

    pub fn remove(&self, qrz: Qrz) -> Option<EntityType> {
        self.flat.remove(&(qrz.q, qrz.r));
        self.water.remove(&(qrz.q, qrz.r));
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
                self.water.remove(&(q, r));
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

    /// The tile's seven vertices (six corners, then the centre) on the
    /// terrain surface: corners at the mean of the three tiles meeting there
    /// (`surface::cell_corner_zs`), the same surface the mesh draws and
    /// physics walks. `apply_slopes = false` gives the flat hex at the
    /// tile's own height.
    pub fn vertices_with_slopes(&self, qrz: Qrz, apply_slopes: bool) -> Vec<Vec3> {
        let mut verts = self.geo.vertices(qrz);
        if !apply_slopes {
            return verts;
        }
        let corner_zs = crate::surface::cell_corner_zs((qrz.q, qrz.r), |q, r| {
            self.get_by_qr(q, r).map(|(t, _)| t.z)
        });
        for (v, z) in verts.iter_mut().zip(corner_zs) {
            v.y = crate::surface::height_y(z.unwrap_or(qrz.z as f32));
        }
        verts
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
}
