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

/// A circle a walker goes round, of a den's piece: its centre from the
/// centre of its den's tile, `anchor`, in world units along the ground, its
/// radius, and how high what stands in it rises.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Solid {
    pub anchor: (i32, i32),
    pub offset: Vec2,
    pub radius: f32,
    pub height: f32,
}

/// How far past a solid's circle a walker standing in a tile may stand
/// from the tile's centre: the tile's own outer radius and a walker's
/// half-width, with room to spare.
const SOLID_REACH: f32 = 1.5;

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
    /// Hot-path elevation index: single shard probe, no chunk derivation.
    flat: Arc<DashMap<(i32, i32), i32>>,
    /// Flooded tiles only: the surface water stands at, as a z-level. A
    /// tile absent here is dry.
    water: Arc<DashMap<(i32, i32), i32>>,
    /// Chunk-sharded storage for mesh generation and EntityType lookups.
    chunks: Arc<DashMap<ChunkId, HashMap<(i32, i32), TileRecord>>>,
    changed: Arc<AtomicBool>,
    /// The dens' circles, each under every tile a walker in which may meet
    /// it, and the tiles each den's are under, by its tile.
    solids: Arc<DashMap<(i32, i32), Vec<Solid>>>,
    solid_tiles: Arc<DashMap<(i32, i32), Vec<(i32, i32)>>>,
    /// The grid's geometry: conversion, vertices, faces.
    geo: Arc<qrz::Map>,
}

impl Map {
    /// An empty map on the grid `map` describes.
    pub fn new(map: qrz::Map) -> Map {
        Map {
            radius: map.radius(),
            rise: map.rise(),
            flat: Arc::new(DashMap::new()),
            water: Arc::new(DashMap::new()),
            chunks: Arc::new(DashMap::new()),
            changed: Arc::new(AtomicBool::new(false)),
            solids: Arc::new(DashMap::new()),
            solid_tiles: Arc::new(DashMap::new()),
            geo: Arc::new(map),
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

    /// Stands the circles of the den on tile `anchor` in place of any it
    /// stood before, none for a den gone: each its centre from the tile's
    /// centre, its radius and its height. Each is kept under every tile a
    /// walker in which may meet it, so a walker reads only its own tile's.
    pub fn set_solids(&self, anchor: (i32, i32), circles: &[(Vec2, f32, f32)]) {
        if let Some((_, tiles)) = self.solid_tiles.remove(&anchor) {
            for tile in tiles {
                if let Some(mut held) = self.solids.get_mut(&tile) {
                    held.retain(|s| s.anchor != anchor);
                }
                self.solids.remove_if(&tile, |_, held| held.is_empty());
            }
        }
        let home = Qrz { q: anchor.0, r: anchor.1, z: 0 };
        let mut tiles = Vec::new();
        for &(offset, radius, height) in circles {
            let solid = Solid { anchor, offset, radius, height };
            let reach = radius + SOLID_REACH;
            let centre = self.convert(Vec3::new(offset.x, 0.0, offset.y));
            let rings = (reach / (self.radius * 3f32.sqrt())).ceil() as u32 + 1;
            for tile in (0..=rings).flat_map(|k| centre.ring(k)) {
                if self.convert(tile).xz().distance(offset) > reach {
                    continue;
                }
                let key = (home.q + tile.q, home.r + tile.r);
                self.solids.entry(key).or_default().push(solid);
                tiles.push(key);
            }
        }
        if !tiles.is_empty() {
            tiles.sort_unstable();
            tiles.dedup();
            self.solid_tiles.insert(anchor, tiles);
        }
    }

    /// The dens' circles a walker standing in tile `(q, r)` may meet.
    pub fn solids_at(&self, q: i32, r: i32) -> Vec<Solid> {
        self.solids.get(&(q, r)).map_or(Vec::new(), |held| held.clone())
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

    /// What stands in a tile's slots: nothing where the tile is not
    /// loaded or its ground is not a decorator. The one read movement and
    /// the renderer take a tile's trees from.
    pub fn cover_at(&self, q: i32, r: i32) -> common::Cover {
        match self.get_by_qr(q, r) {
            Some((_, EntityType::Decorator(d))) => d.cover,
            _ => common::Cover::NONE,
        }
    }

    pub fn take_changed(&self) -> bool {
        self.changed.swap(false, Ordering::Relaxed)
    }

    pub fn force_changed(&self) {
        self.changed.store(true, Ordering::Relaxed);
    }

    pub fn rise(&self) -> f32 { self.rise }
    pub fn radius(&self) -> f32 { self.radius }

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
    /// terrain surface, about the tile's own column: corners at the mean of
    /// the three tiles meeting there (`surface::cell_corner_zs`), the same
    /// surface the mesh draws and physics walks.
    pub fn sloped_corners(&self, qrz: Qrz) -> Vec<Vec3> {
        // About the column's foot, never the tile's world position: a mesh
        // built on these is exact however far out the tile is, and is
        // placed by its tile (`RenderOrigin::render_tile`).
        let mut verts = self.geo.vertices(Qrz { q: 0, r: 0, z: 0 });
        let corner_zs = crate::surface::cell_corner_zs((qrz.q, qrz.r), |q, r| {
            self.get_by_qr(q, r).map(|(t, _)| t.z)
        });
        for (v, z) in verts.iter_mut().zip(corner_zs) {
            v.y = crate::surface::height_y(z.unwrap_or(qrz.z as f32));
        }
        verts[6].y = crate::surface::height_y(qrz.z as f32);
        verts
    }

    /// The face between two neighbours: its unit normal from `here` into
    /// `next` and its midpoint, in the ground plane (`qrz::Map::face`).
    pub fn face(&self, here: Qrz, next: Qrz) -> (Vec2, Vec2) {
        self.geo.face(here, next)
    }

    /// Where a ray from `from` in `here` along `dir` leaves the tile: the
    /// distance to the first face and the neighbour across it
    /// (`qrz::Map::exit`).
    pub fn exit(&self, from: Vec2, dir: Vec2, here: Qrz) -> (f32, Qrz) {
        self.geo.exit(from, dir, here)
    }
}

/// The tile as a summary reads it, or None while it has not streamed in.
impl common::summary::SummarySource for Map {
    fn sample(&self, q: i32, r: i32) -> Option<common::summary::TileSample> {
        let (qrz, typ) = self.get_by_qr(q, r)?;
        let cover = match typ {
            EntityType::Decorator(d) => d.cover,
            _ => common::Cover::NONE,
        };
        Some(common::summary::TileSample { z: qrz.z, water: self.water_at(q, r), cover })
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
        let map = Map::new(qrz::Map::new(1.0, 0.8));
        for q in -5..=5 {
            for r in -5..=5 {
                map.insert(Qrz { q, r, z: 0 }, EntityType::Decorator(default()));
            }
        }
        map
    }

    /// A tile's cover is its decorator's, and a tile not loaded has none.
    #[test]
    fn cover_is_the_decorators() {
        use common::{Cover, Content};
        use crate::components::entity_type::decorator::Decorator;
        let map = make_flat_map();
        let cover = Cover::NONE.with(0, Content::Pine).with(2, Content::Brush);
        map.insert(Qrz { q: 1, r: 1, z: 0 }, EntityType::Decorator(Decorator { cover, is_solid: true }));
        assert_eq!(map.cover_at(1, 1), cover);
        assert_eq!(map.cover_at(0, 0), Cover::NONE);
        assert_eq!(map.cover_at(50, 50), Cover::NONE);
    }

    /// A path steps a tile nearer the goal each step, on the flat and up a
    /// slope of one level a tile.
    #[test]
    fn greedy_path_steps_toward_the_goal_on_the_flat_and_up_a_slope() {
        let flat = make_flat_map();
        let path = flat.greedy_path(Qrz { q: 0, r: 0, z: 0 }, Qrz { q: 3, r: 0, z: 0 }, 10);
        assert_eq!(path.len(), 3);
        assert_eq!(path.last().unwrap().flat_distance(&Qrz { q: 3, r: 0, z: 0 }), 0);

        let slope = Map::new(qrz::Map::new(1.0, 0.8));
        for q in 0..=4 {
            slope.insert(Qrz { q, r: 0, z: q }, EntityType::Decorator(default()));
        }
        let path = slope.greedy_path(Qrz { q: 0, r: 0, z: 0 }, Qrz { q: 4, r: 0, z: 4 }, 10);
        assert_eq!(path.len(), 4);
        assert_eq!(*path.last().unwrap(), Qrz { q: 4, r: 0, z: 4 });
    }

    #[test]
    fn greedy_path_stops_at_cliff() {
        let map = Map::new(qrz::Map::new(1.0, 0.8));
        map.insert(Qrz { q: 0, r: 0, z: 0 }, EntityType::Decorator(default()));
        map.insert(Qrz { q: 1, r: 0, z: 0 }, EntityType::Decorator(default()));
        map.insert(Qrz { q: 2, r: 0, z: 5 }, EntityType::Decorator(default()));

        let path = map.greedy_path(
            Qrz { q: 0, r: 0, z: 0 },
            Qrz { q: 3, r: 0, z: 0 },
            10,
        );
        assert_eq!(path.len(), 1);
        assert_eq!(path[0], Qrz { q: 1, r: 0, z: 0 });
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
        let map = Map::new(qrz::Map::new(1.0, 0.8));
        map.insert(Qrz { q: 0, r: 0, z: 0 }, EntityType::Decorator(default()));

        let path = map.greedy_path(
            Qrz { q: 0, r: 0, z: 0 },
            Qrz { q: 5, r: 0, z: 0 },
            10,
        );
        assert!(path.is_empty());
    }
}
