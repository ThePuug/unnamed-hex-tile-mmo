use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::PrimitiveTopology,
};
use bevy_camera::primitives::Aabb;
use bevy_mesh::Indices;
use std::sync::{Arc, Mutex};

use qrz::{self, Convert, Qrz};

use crate::{
    chunk::{ChunkId, loc_to_chunk},
    components::entity_type::*,
};

/// Events that modify the map (spawn/despawn tiles)
#[derive(Clone, Debug)]
pub enum TileEvent {
    Spawn(Qrz, EntityType),
    Despawn(Qrz),
}

/// Map resource with queued tile events for async coordination.
/// The drain_loop owns a mutable working copy and publishes Arc snapshots
/// via the `published` Mutex. No RwLock — zero deadlock risk.
#[derive(Resource)]
pub struct MapState {
    /// Latest published snapshot, swapped atomically by drain_loop
    published: Arc<Mutex<Arc<qrz::Map<EntityType>>>>,
    /// Queue of pending tile events (spawns/despawns)
    pub pending_events: Arc<Mutex<Vec<TileEvent>>>,
}

impl MapState {
    pub fn new(map: qrz::Map<EntityType>) -> Self {
        let published = Arc::new(Mutex::new(Arc::new(map.clone())));
        let pending_arc = Arc::new(Mutex::new(Vec::new()));

        // Spawn permanent background drain task
        let published_clone = published.clone();
        let pending_clone = pending_arc.clone();

        #[cfg(not(target_arch = "wasm32"))]
        {
            std::thread::spawn(move || {
                drain_loop(map, published_clone, pending_clone);
            });
        }

        Self {
            published,
            pending_events: pending_arc,
        }
    }

    /// Queue a tile event (spawn or despawn)
    pub fn queue_event(&self, event: TileEvent) {
        let mut queue = self.pending_events.lock().unwrap();
        queue.push(event);
    }

    /// Create a Map resource from the current published snapshot
    pub fn as_map(&self) -> Map {
        Map::from_arc(self.published.lock().unwrap().clone())
    }

    /// Check if the published snapshot differs from the given Arc.
    /// Returns the new Arc if changed, None otherwise.
    pub fn try_refresh(&self, current: &Arc<qrz::Map<EntityType>>) -> Option<Arc<qrz::Map<EntityType>>> {
        let published = self.published.lock().unwrap();
        if Arc::ptr_eq(current, &published) {
            None
        } else {
            Some(published.clone())
        }
    }
}

/// Permanent background task that drains pending tile events and applies them to the map.
/// Owns a mutable working copy. Publishes new Arc snapshots after processing events.
fn drain_loop(
    mut working: qrz::Map<EntityType>,
    published: Arc<Mutex<Arc<qrz::Map<EntityType>>>>,
    pending: Arc<Mutex<Vec<TileEvent>>>,
) {
    use std::time::Duration;

    loop {
        // Lock briefly to check and take events
        let events = {
            let mut queue = pending.lock().unwrap();
            if queue.is_empty() {
                drop(queue);
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            std::mem::take(&mut *queue)
        };

        // Apply events directly to working copy (no locks needed for mutation)
        for event in events {
            match event {
                TileEvent::Spawn(qrz, entity_type) => {
                    working.insert(qrz, entity_type);
                }
                TileEvent::Despawn(qrz) => {
                    working.remove(qrz);
                }
            }
        }

        // Publish new snapshot (brief Mutex lock)
        {
            let mut pub_lock = published.lock().unwrap();
            *pub_lock = Arc::new(working.clone());
        }

        // Brief yield after publishing — one frame is enough for mesh tasks
        // to grab the new snapshot before the next batch arrives.
        std::thread::sleep(Duration::from_millis(16));
    }
}

/// System that checks if drain_loop published a new map snapshot and swaps it in.
/// Runs in PreUpdate so all Update/FixedUpdate systems see the latest map data.
/// ResMut triggers Bevy change detection automatically when the Arc is swapped.
pub fn refresh_map(
    map_state: Res<MapState>,
    mut map: ResMut<Map>,
) {
    if let Some(new_arc) = map_state.try_refresh(map.inner_arc()) {
        map.0 = new_arc;
    }
}

/// Map resource wrapping an immutable Arc snapshot of the hex tile map.
/// Readers clone the Arc (O(1)) for async tasks. No RwLock anywhere.
/// Server uses Arc::make_mut for zero-cost mutation when refcount=1.
#[derive(Clone, Resource)]
pub struct Map(Arc<qrz::Map<EntityType>>);

impl Map {
    pub fn new(map: qrz::Map<EntityType>) -> Map {
        Map(Arc::new(map))
    }

    /// Create from Arc snapshot
    pub fn from_arc(arc: Arc<qrz::Map<EntityType>>) -> Map {
        Map(arc)
    }

    /// Get a reference to the inner Arc (for refresh comparison)
    pub fn inner_arc(&self) -> &Arc<qrz::Map<EntityType>> {
        &self.0
    }

    /// Get the vertical rise per Z level from the underlying map
    pub fn rise(&self) -> f32 {
        self.0.rise()
    }

    /// Compute slope-adjusted vertices for a hex tile.
    /// Outer vertex Y is shifted ±0.5×rise toward higher/lower neighbors.
    fn vertices_with_slopes_inner(map: &qrz::Map<EntityType>, qrz: Qrz, apply_slopes: bool) -> Vec<Vec3> {
        let mut verts = map.vertices(qrz);
        if !apply_slopes {
            return verts;
        }

        let rise = map.rise();
        let mut vertex_adjustments: [Vec<f32>; 6] = Default::default();

        let direction_to_vertices = [
            (4, 5), // Dir 0: West edge → SW(4), NW(5)
            (3, 4), // Dir 1: SW edge → S(3), SW(4)
            (2, 3), // Dir 2: SE edge → SE(2), S(3)
            (1, 2), // Dir 3: East edge → NE(1), SE(2)
            (0, 1), // Dir 4: NE edge → N(0), NE(1)
            (5, 0), // Dir 5: NW edge → NW(5), N(0)
        ];

        for (dir_idx, direction) in qrz::DIRECTIONS.iter().enumerate() {
            let neighbor_qrz = qrz + *direction;
            if let Some((actual_neighbor_qrz, _)) = map.get_by_qr(neighbor_qrz.q, neighbor_qrz.r) {
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

    /// Compute slope-adjusted vertices for a hex tile (public wrapper).
    pub fn vertices_with_slopes(&self, qrz: Qrz, apply_slopes: bool) -> Vec<Vec3> {
        Self::vertices_with_slopes_inner(&self.0, qrz, apply_slopes)
    }

    /// Compute per-vertex normal for a hex tile from its actual geometry.
    /// `verts` layout: [0..5] = outer (N, NE, SE, S, SW, NW), [6] = center.
    /// Each outer vertex participates in 2 triangles; the center in all 6.
    pub fn hex_vertex_normal(verts: &[Vec3], vertex_idx: usize) -> Vec3 {
        let center = verts[6];
        if vertex_idx == 6 {
            // Center: average all 6 face normals
            let mut sum = Vec3::ZERO;
            for i in 0..6 {
                sum += (verts[(i + 1) % 6] - center).cross(verts[i] - center);
            }
            if sum.length_squared() > 1e-10 { sum.normalize() } else { Vec3::Y }
        } else {
            // Outer vertex j: average the 2 adjacent face normals
            let j = vertex_idx;
            let n1 = (verts[(j + 1) % 6] - center).cross(verts[j] - center);
            let n2 = (verts[j] - center).cross(verts[(j + 5) % 6] - center);
            let sum = n1 + n2;
            if sum.length_squared() > 1e-10 { sum.normalize() } else { Vec3::Y }
        }
    }

    /// Generate a mesh for a single chunk using TriangleList topology.
    /// Color is computed in the terrain shader from world-space Y; no vertex colors emitted.
    pub fn generate_chunk_mesh(&self, chunk_id: ChunkId, apply_slopes: bool) -> (Mesh, Aabb) {
        let map = &*self.0;

        let mut verts: Vec<Vec3> = Vec::new();
        let mut norms: Vec<Vec3> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();

        for (&tile_qrz, _) in map.iter() {
            if loc_to_chunk(tile_qrz) != chunk_id {
                continue;
            }

            let raw_verts = map.vertices(tile_qrz);
            let slope_verts = Self::vertices_with_slopes_inner(&map, tile_qrz, apply_slopes);

            // Use raw XZ for edge alignment, slope-adjusted Y for height
            let tile_verts: Vec<Vec3> = raw_verts.iter().enumerate().map(|(i, &raw_pos)| {
                if apply_slopes && i < 6 {
                    Vec3::new(raw_pos.x, slope_verts[i].y, raw_pos.z)
                } else {
                    raw_pos
                }
            }).collect();

            let base_idx = verts.len() as u32;

            // Center vertex (index 6)
            verts.push(tile_verts[6]);
            norms.push(Self::hex_vertex_normal(&tile_verts, 6));

            // Outer vertices (0-5: N, NE, SE, S, SW, NW)
            for i in 0..6 {
                verts.push(tile_verts[i]);
                norms.push(Self::hex_vertex_normal(&tile_verts, i));
            }

            // 6 triangles for hex top surface (CCW winding)
            for i in 0..6 {
                let v1 = base_idx + 1 + i;
                let v2 = base_idx + 1 + ((i + 1) % 6);
                indices.extend([base_idx, v2, v1]);
            }

            // Vertical skirt geometry for cliff edges
            for (dir_idx, direction) in qrz::DIRECTIONS.iter().enumerate() {
                let neighbor_qrz = tile_qrz + *direction;
                let found_neighbor = map.get_by_qr(neighbor_qrz.q, neighbor_qrz.r);

                if let Some((actual_neighbor_qrz, _)) = found_neighbor {
                    let elevation_diff = actual_neighbor_qrz.z - tile_qrz.z;
                    if elevation_diff >= 0 {
                        continue;
                    }

                    let neighbor_raw = map.vertices(actual_neighbor_qrz);
                    let neighbor_verts: Vec<Vec3> = if apply_slopes {
                        let neighbor_slope = Self::vertices_with_slopes_inner(&map, actual_neighbor_qrz, true);
                        neighbor_raw.iter().enumerate().map(|(i, &raw_pos)| {
                            if i < 6 {
                                Vec3::new(raw_pos.x, neighbor_slope[i].y, raw_pos.z)
                            } else {
                                raw_pos
                            }
                        }).collect()
                    } else {
                        neighbor_raw
                    };

                    let (curr_v1_idx, curr_v2_idx, neighbor_v1_idx, neighbor_v2_idx) = match dir_idx {
                        0 => (4, 5, 2, 1),
                        1 => (3, 4, 1, 0),
                        2 => (2, 3, 0, 5),
                        3 => (1, 2, 5, 4),
                        4 => (0, 1, 4, 3),
                        5 => (5, 0, 3, 2),
                        _ => continue,
                    };

                    let curr_v1 = tile_verts[curr_v1_idx];
                    let curr_v2 = tile_verts[curr_v2_idx];
                    let neighbor_v1 = neighbor_verts[neighbor_v1_idx];
                    let neighbor_v2 = neighbor_verts[neighbor_v2_idx];

                    // Outward-facing normal for skirt (horizontal → shader detects as cliff)
                    let edge_dir = (curr_v2 - curr_v1).normalize();
                    let outward_normal = edge_dir.cross(Vec3::new(0., -1., 0.)).normalize();

                    let skirt_base = verts.len() as u32;
                    verts.extend([curr_v1, curr_v2, neighbor_v2, neighbor_v1]);
                    norms.extend([outward_normal; 4]);

                    indices.extend([skirt_base, skirt_base + 1, skirt_base + 2]);
                    indices.extend([skirt_base, skirt_base + 2, skirt_base + 3]);
                }
            }
        }

        let mut min = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
        let mut max = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
        for vert in &verts {
            min = Vec3::min(min, *vert);
            max = Vec3::max(max, *vert);
        }

        let vert_count = verts.len();

        (
            Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD
            )
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, verts)
                .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, (0..vert_count).map(|_| [0., 0.]).collect::<Vec<[f32; 2]>>())
                .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, norms)
                .with_inserted_indices(Indices::U32(indices)),
            Aabb::from_min_max(min, max),
        )
    }

    pub fn regenerate_mesh(&self, apply_slopes: bool) -> (Mesh,Aabb) {
        let map = &*self.0;

        let mut verts:Vec<Vec3> = Vec::new();
        let mut norms:Vec<Vec3> = Vec::new();
        let mut last_qrz:Option<Qrz> = None;
        let mut skip_sw = false;
        let mut west_skirt_verts: Vec<Vec3> = Vec::new();
        let mut west_skirt_norms: Vec<Vec3> = Vec::new();

        map.iter().for_each(|(&it_qrz, _)| {
            let it_vrt = Self::vertices_with_slopes_inner(&map, it_qrz, apply_slopes);

            if let Some(last_qrz) = last_qrz {
                if last_qrz.q*2+last_qrz.r != it_qrz.q*2+it_qrz.r {
                    verts.append(&mut west_skirt_verts);
                    norms.append(&mut west_skirt_norms);
                }
            }

            let sw_neighbor = it_qrz + qrz::DIRECTIONS[1];
            let sw_result = map.get_by_qr(sw_neighbor.q, sw_neighbor.r);
            let sw_data = sw_result.map(|(qrz, _)| Self::vertices_with_slopes_inner(&map, qrz, apply_slopes));

            if skip_sw {
                let last_vrt = Self::vertices_with_slopes_inner(&map, last_qrz.unwrap(), apply_slopes);
                let last_vrt_underover = Vec3::new(last_vrt[3].x, it_vrt[0].y, last_vrt[3].z);
                verts.extend([ last_vrt_underover, last_vrt_underover, it_vrt[0], it_vrt[0] ]);
                norms.extend([ Vec3::new(0., 1., 0.); 4 ]);
                skip_sw = false;
            }

            let norm_0 = Self::hex_vertex_normal(&it_vrt, 0);
            let norm_5 = Self::hex_vertex_normal(&it_vrt, 5);
            let norm_4 = Self::hex_vertex_normal(&it_vrt, 4);
            let norm_3 = Self::hex_vertex_normal(&it_vrt, 3);
            let center_normal = Self::hex_vertex_normal(&it_vrt, 6);

            verts.extend([ it_vrt[0], it_vrt[5], it_vrt[6], it_vrt[4], it_vrt[3] ]);
            norms.extend([ norm_0, norm_5, center_normal, norm_4, norm_3 ]);

            if let Some(sw_vrt) = sw_data {
                let sw_norm_0 = Self::hex_vertex_normal(&sw_vrt, 0);
                let sw_norm_1 = Self::hex_vertex_normal(&sw_vrt, 1);
                let sw_norm_2 = Self::hex_vertex_normal(&sw_vrt, 2);
                let sw_norm_3 = Self::hex_vertex_normal(&sw_vrt, 3);
                let sw_center = Self::hex_vertex_normal(&sw_vrt, 6);

                verts.extend([ sw_vrt[0], sw_vrt[1], sw_vrt[6], sw_vrt[2], sw_vrt[3]]);
                norms.extend([ sw_norm_0, sw_norm_1, sw_center, sw_norm_2, sw_norm_3 ]);
            } else {
                verts.extend([ it_vrt[3] ]);
                norms.extend([ norm_3 ]);
                skip_sw = true;
            }

            let we_neighbor = it_qrz + qrz::DIRECTIONS[0];
            let we_result = map.get_by_qr(we_neighbor.q, we_neighbor.r);
            let we_qrz = we_result.unwrap_or((it_qrz + qrz::DIRECTIONS[0], EntityType::Decorator(default()))).0;
            let mut we_vrt = if we_result.is_some() {
                Self::vertices_with_slopes_inner(&map, we_qrz, apply_slopes)
            } else {
                map.vertices(we_qrz)
            };

            if we_result.is_none() {
                we_vrt[1].y = it_vrt[5].y;
                we_vrt[2].y = it_vrt[4].y;
            }

            let we_norm_1 = Self::hex_vertex_normal(&we_vrt, 1);
            let we_norm_2 = Self::hex_vertex_normal(&we_vrt, 2);

            if let Some(last_qrz) = last_qrz {
                let last_vrt = Self::vertices_with_slopes_inner(&map, last_qrz, apply_slopes);
                let last_vrt_underover = Vec3::new(it_vrt[5].x, last_vrt[4].y, it_vrt[5].z);
                west_skirt_verts.extend([ last_vrt_underover, last_vrt_underover ]);
                west_skirt_norms.extend([ Vec3::new(0., 1., 0.); 2 ]);
            }
            west_skirt_verts.extend([ it_vrt[5], we_vrt[1], it_vrt[4], we_vrt[2], it_vrt[4], it_vrt[4] ]);
            west_skirt_norms.extend([ norm_5, we_norm_1, norm_4, we_norm_2, norm_4, norm_4 ]);

            last_qrz = Some(it_qrz);
        });

        let mut min = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
        let mut max = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
        for vert in &verts {
            min = Vec3::min(min, *vert);
            max = Vec3::max(max, *vert);
        }

        let len = verts.clone().len() as u32;
        println!("Terrain mesh: {} tiles, {} vertices, AABB: {:?} to {:?}",
                 map.len(), len, min, max);
        (
            Mesh::new(PrimitiveTopology::TriangleStrip, RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD)
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, verts)
                .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, (0..len).map(|_| [0., 0.]).collect::<Vec<[f32; 2]>>())
                .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, norms)
                .with_inserted_indices(Indices::U32((0..len).collect())),
            Aabb::from_min_max(min, max),
        )
    }

    /// O(1) lookup by (q, r) column — returns the Qrz (with correct z) and value.
    pub fn get_by_qr(&self, q: i32, r: i32) -> Option<(Qrz, EntityType)> {
        self.0.get_by_qr(q, r)
    }

    pub fn get(&self, qrz: Qrz) -> Option<EntityType> {
        self.0.get(qrz).copied()
    }

    pub fn insert(&mut self, qrz: Qrz, obj: EntityType) {
        if self.0.get(qrz).is_some() {
            warn!("duplicate tile insert at ({}, {}, {})", qrz.q, qrz.r, qrz.z);
        }
        Arc::make_mut(&mut self.0).insert(qrz, obj);
    }

    pub fn remove(&mut self, qrz: Qrz) -> Option<EntityType> {
        Arc::make_mut(&mut self.0).remove(qrz)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn heap_size_estimate(&self) -> usize {
        self.0.heap_size_estimate()
    }

    pub fn radius(&self) -> f32 {
        self.0.radius()
    }

    pub fn orientation(&self) -> qrz::HexOrientation {
        self.0.orientation()
    }

    pub fn neighbors(&self, qrz: Qrz) -> Vec<(Qrz, EntityType)> {
        self.0.neighbors(qrz)
    }

    /// Greedily walk toward `toward`, picking the neighbor closest each step.
    /// Uses `neighbors()` which is elevation-aware (±1 z-level, walkable only).
    /// Stops on: arrival, no walkable neighbors, no progress, or `max_steps`.
    /// Returns floor-level tiles visited (does NOT include `from`).
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
                break; // No progress
            }

            current = next;
            path.push(current);
        }

        path
    }

    pub fn iter_tiles(&self) -> impl Iterator<Item = (Qrz, EntityType)> + '_ {
        self.0.iter().map(|(&qrz, &typ)| (qrz, typ))
    }
}

impl Convert<Qrz, Vec3> for Map {
    fn convert(&self, it: Qrz) -> Vec3 {
        self.0.convert(it)
    }
}

impl Convert<Vec3, Qrz> for Map {
    fn convert(&self, it: Vec3) -> Qrz {
        self.0.convert(it)
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
        // Gradual uphill: z increases by 1 each tile
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
        // Cliff: q=2 is 5 levels higher (not walkable via neighbors)
        qrz_map.insert(Qrz { q: 2, r: 0, z: 5 }, EntityType::Decorator(default()));
        let map = Map::new(qrz_map);

        let path = map.greedy_path(
            Qrz { q: 0, r: 0, z: 0 },
            Qrz { q: 3, r: 0, z: 0 },
            10,
        );
        // Should reach q=1 then stop (cliff blocks further progress)
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
        // Island with no walkable path toward destination
        let mut qrz_map = qrz::Map::<EntityType>::new(1.0, 0.8, qrz::HexOrientation::FlatTop);
        qrz_map.insert(Qrz { q: 0, r: 0, z: 0 }, EntityType::Decorator(default()));
        // No neighbors at all
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
        // Create two adjacent flat hexes at same elevation
        // If normals only consider the current hex, they'll be tilted toward/away from neighbors
        // If normals consider neighbors too, they should point straight up (smooth flat plane)
        let mut qrz_map = qrz::Map::new(1.0, 0.8, qrz::HexOrientation::FlatTop);
        let hex1 = Qrz { q: 0, r: 0, z: 0 };
        let hex2 = Qrz { q: 1, r: 0, z: 0 }; // Adjacent hex at same elevation

        qrz_map.insert(hex1, EntityType::Decorator(default()));
        qrz_map.insert(hex2, EntityType::Decorator(default()));

        let map = Map::new(qrz_map);

        // Get vertices for hex1 to understand its structure
        let hex1_verts = map.vertices_with_slopes(hex1, true);

        // Calculate normal for the vertex that's shared between hex1 and hex2
        // Vertex 1 (NE) of hex1 points toward hex2 (which is to the East, direction index 3)
        // Actually, hex2 is at direction index 3 (East), so vertices 1 and 2 are shared
        let shared_vertex_normal = Map::hex_vertex_normal(&hex1_verts, 1);

        // On a flat plane with neighbors, the normal should point straight up
        // If we only considered the current hex's triangles, it would be tilted
        // This tests that we're considering the neighboring hex's triangles too

        // The Y component should dominate (close to 1.0)
        assert!(
            shared_vertex_normal.y > 0.95,
            "Expected shared vertex normal to point mostly upward (Y > 0.95) on flat adjacent hexes, \
             but got normal: {:?} with Y = {}. This suggests normals aren't considering neighboring hexes.",
            shared_vertex_normal,
            shared_vertex_normal.y
        );

        // X and Z should be very small
        assert!(
            shared_vertex_normal.x.abs() < 0.3,
            "Expected X component of normal to be small on flat terrain, but got {}",
            shared_vertex_normal.x
        );
        assert!(
            shared_vertex_normal.z.abs() < 0.3,
            "Expected Z component of normal to be small on flat terrain, but got {}",
            shared_vertex_normal.z
        );
    }

    #[test]
    fn test_generate_chunk_mesh() {
        use crate::chunk::{ChunkId, chunk_to_tile};

        // Create a map with tiles in multiple chunks
        let mut qrz_map = qrz::Map::new(1.0, 0.8, qrz::HexOrientation::FlatTop);

        // Chunk (0,0) - add 16x16 tiles (flat terrain for exact vertex count checks)
        for offset_q in 0..16 {
            for offset_r in 0..16 {
                let tile = chunk_to_tile(ChunkId(0, 0), offset_q as u8, offset_r as u8);
                qrz_map.insert(tile, EntityType::Decorator(default()));
            }
        }

        // Chunk (1,1) - add 16 tiles
        for offset_q in 0..16 {
            for offset_r in 0..16 {
                let tile = chunk_to_tile(ChunkId(1, 1), offset_q as u8, offset_r as u8);
                qrz_map.insert(tile, EntityType::Decorator(default()));
            }
        }

        let map = Map::new(qrz_map);

        // Generate mesh for chunk (0,0) only
        let (mesh, aabb) = map.generate_chunk_mesh(ChunkId(0, 0), true);

        // Verify mesh properties
        let positions = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            .expect("Mesh should have positions")
            .as_float3()
            .expect("Positions should be Vec3");

        // Each hex has 7 vertices (1 center + 6 outer)
        // 16x16 = 256 tiles per chunk
        assert_eq!(positions.len(), 256 * 7, "Expected 256 tiles * 7 vertices per tile");

        // Verify mesh has indices for TriangleList
        let indices = match mesh.indices() {
            Some(bevy_mesh::Indices::U32(idx)) => idx,
            _ => panic!("Expected U32 indices"),
        };

        // Each hex has 6 triangles (18 indices)
        assert_eq!(indices.len(), 256 * 6 * 3, "Expected 256 tiles * 6 triangles * 3 indices");

        // Verify AABB is reasonable (not empty in horizontal extents)
        assert!(aabb.min().x < aabb.max().x, "AABB should have width");
        assert!(aabb.min().y <= aabb.max().y, "AABB Y min should not exceed max");
        assert!(aabb.min().z < aabb.max().z, "AABB should have depth");
    }

    #[test]
    fn test_generate_chunk_mesh_filters_to_chunk() {
        use crate::chunk::{ChunkId, chunk_to_tile};

        // Create a map with tiles in two different chunks
        let mut qrz_map = qrz::Map::new(1.0, 0.8, qrz::HexOrientation::FlatTop);

        // Chunk (0,0) - 4 tiles
        for offset_q in 0..2 {
            for offset_r in 0..2 {
                let tile = chunk_to_tile(ChunkId(0, 0), offset_q as u8, offset_r as u8);
                qrz_map.insert(tile, EntityType::Decorator(default()));
            }
        }

        // Chunk (1,1) - 9 tiles
        for offset_q in 0..3 {
            for offset_r in 0..3 {
                let tile = chunk_to_tile(ChunkId(1, 1), offset_q as u8, offset_r as u8);
                qrz_map.insert(tile, EntityType::Decorator(default()));
            }
        }

        let map = Map::new(qrz_map);

        // Generate mesh for chunk (0,0) - should only include 4 tiles
        let (mesh_00, _) = map.generate_chunk_mesh(ChunkId(0, 0), true);
        let positions_00 = mesh_00.attribute(Mesh::ATTRIBUTE_POSITION)
            .expect("Mesh should have positions")
            .as_float3()
            .expect("Positions should be Vec3");

        assert_eq!(positions_00.len(), 4 * 7, "Chunk (0,0) should have 4 tiles * 7 vertices");

        // Generate mesh for chunk (1,1) - should only include 9 tiles
        let (mesh_11, _) = map.generate_chunk_mesh(ChunkId(1, 1), true);
        let positions_11 = mesh_11.attribute(Mesh::ATTRIBUTE_POSITION)
            .expect("Mesh should have positions")
            .as_float3()
            .expect("Positions should be Vec3");

        assert_eq!(positions_11.len(), 9 * 7, "Chunk (1,1) should have 9 tiles * 7 vertices");
    }

}
