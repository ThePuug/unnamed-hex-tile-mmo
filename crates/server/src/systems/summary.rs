use std::collections::HashSet;

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, futures_lite::future};
use common_bevy::{
    chunk::FIXED_STREAM_RADIUS_WU,
    components::Loc,
    geometry::flat_top_tile_center,
    message::{Event, SummaryData, SummaryKey, *},
    summary::{compute_active_bands, select_center_z, summary_lattice, visible_summary_cells_in_band},
};

use crate::resources::event_registry::EventRegistry;
use crate::resources::summary_cache::SummaryCache;

/// Per-client tracking of which summaries have been sent.
#[derive(bevy::prelude::Component)]
pub struct VisibleSummaryCache {
    /// All summary keys currently sent to this client.
    pub sent: HashSet<SummaryKey>,
    /// Player position at last recomputation (for throttling).
    pub last_pos: (i32, i32, i32),
}

impl Default for VisibleSummaryCache {
    fn default() -> Self {
        Self {
            sent: HashSet::new(),
            last_pos: (i32::MIN, i32::MIN, i32::MIN),
        }
    }
}

/// In-flight async summary computation tasks.
#[derive(Resource, Default)]
pub struct SummaryTaskQueue {
    tasks: Vec<(Entity, Vec<SummaryKey>, Task<Vec<SummaryData>>)>,
    in_flight: HashSet<SummaryKey>,
}

/// Minimum world-unit movement before recomputing the visible summary set.
const MIN_UPDATE_DISTANCE: f32 = 20.0;
/// Minimum z-level change before recomputing (handles vertical movement).
const MIN_UPDATE_Z: i32 = 10;

/// Compute visible summary sets per player, dispatch async tasks for cache misses.
///
/// Cache hits are sent immediately. Cache misses are dispatched to the
/// AsyncComputeTaskPool — no synchronous elevation queries on the main thread.
pub fn dispatch_summary_tasks(
    mut writer: MessageWriter<Do>,
    mut query: Query<(Entity, &Loc, &mut VisibleSummaryCache)>,
    summary_cache: Res<SummaryCache>,
    registry: Res<EventRegistry>,
    mut task_queue: ResMut<SummaryTaskQueue>,
) {
    for (ent, loc, mut vis_cache) in query.iter_mut() {
        let q = loc.q;
        let r = loc.r;
        let z = loc.z;

        // Throttle: skip if player hasn't moved enough
        let (lq, lr, lz) = vis_cache.last_pos;
        let needs_recompute = if lq == i32::MIN {
            true
        } else {
            let (wx, wz) = flat_top_tile_center(q, r, 1.0);
            let (lwx, lwz) = flat_top_tile_center(lq, lr, 1.0);
            let horiz_dist = ((wx - lwx).powi(2) + (wz - lwz).powi(2)).sqrt();
            horiz_dist >= MIN_UPDATE_DISTANCE || (z - lz).abs() >= MIN_UPDATE_Z
        };

        if !needs_recompute {
            continue;
        }
        vis_cache.last_pos = (q, r, z);

        let (cam_wx, cam_wz) = flat_top_tile_center(q, r, 1.0);

        let cam_h = common::camera::camera_height(common::camera::MAX_GAMEPLAY_FOV)
            + z.max(0) as f32 * common::camera::RISE;
        let far_ground = cam_h / common::camera::HORIZON_MARGIN_DEG.to_radians().tan();

        let bands = compute_active_bands(far_ground);

        let mut visible_set: HashSet<SummaryKey> = HashSet::new();

        for band in &bands {
            if band.outer_wu <= FIXED_STREAM_RADIUS_WU {
                continue;
            }
            let inner = band.inner_wu.max(FIXED_STREAM_RADIUS_WU);
            let cells = visible_summary_cells_in_band(band.r, cam_wx, cam_wz, inner, band.outer_wu);
            for (sq, sr) in cells {
                visible_set.insert(SummaryKey { r: band.r, sq, sr });
            }
        }

        // Send removals immediately
        let removals: Vec<SummaryKey> = vis_cache
            .sent
            .iter()
            .filter(|k| !visible_set.contains(k))
            .copied()
            .collect();

        if !removals.is_empty() {
            writer.write(Do {
                event: Event::SummaryBatch { ent, additions: Vec::new(), removals: removals.clone() },
            });
            for key in &removals {
                vis_cache.sent.remove(key);
            }
        }

        // Partition additions: cache hits send now, cache misses go async
        let mut cached = Vec::new();
        let mut to_compute = Vec::new();

        for key in &visible_set {
            if vis_cache.sent.contains(key) {
                continue;
            }
            if let Some(center_z) = summary_cache.get(key) {
                cached.push(SummaryData { r: key.r, sq: key.sq, sr: key.sr, center_z });
                vis_cache.sent.insert(*key);
            } else if !task_queue.in_flight.contains(key) {
                to_compute.push(*key);
            }
        }

        if !cached.is_empty() {
            writer.write(Do {
                event: Event::SummaryBatch { ent, additions: cached, removals: Vec::new() },
            });
        }

        if !to_compute.is_empty() {
            info!(
                "[summary-server] player ({},{}) z={}: dispatching {} async ({} cached, {} in-flight)",
                q, r, z, to_compute.len(),
                vis_cache.sent.len(),
                task_queue.in_flight.len(),
            );

            for key in &to_compute {
                task_queue.in_flight.insert(*key);
            }

            let reg = registry.clone();
            let keys = to_compute.clone();
            let task = AsyncComputeTaskPool::get().spawn(async move {
                keys.iter()
                    .map(|key| {
                        let lat = summary_lattice(key.r);
                        let tile_zs: Vec<i32> = lat
                            .tiles_in_cell((key.sq, key.sr))
                            .map(|(tq, tr)| reg.elevation_at(tq, tr))
                            .collect();
                        let center_z = select_center_z(&tile_zs);
                        SummaryData { r: key.r, sq: key.sq, sr: key.sr, center_z }
                    })
                    .collect()
            });

            task_queue.tasks.push((ent, to_compute, task));
        }
    }
}

/// Poll completed async summary tasks, insert into cache, send to clients.
pub fn poll_summary_tasks(
    mut writer: MessageWriter<Do>,
    mut summary_cache: ResMut<SummaryCache>,
    mut task_queue: ResMut<SummaryTaskQueue>,
    mut query: Query<&mut VisibleSummaryCache>,
) {
    let current = std::mem::take(&mut task_queue.tasks);
    let mut pending = Vec::new();

    for (ent, keys, mut task) in current {
        if let Some(results) = block_on(future::poll_once(&mut task)) {
            // Insert into cache
            for data in &results {
                summary_cache.insert(
                    SummaryKey { r: data.r, sq: data.sq, sr: data.sr },
                    data.center_z,
                );
            }
            for key in &keys {
                task_queue.in_flight.remove(key);
            }
            // Update sent tracking
            if let Ok(mut vis_cache) = query.get_mut(ent) {
                for key in &keys {
                    vis_cache.sent.insert(*key);
                }
            }
            // Send to client
            if !results.is_empty() {
                writer.write(Do {
                    event: Event::SummaryBatch { ent, additions: results, removals: Vec::new() },
                });
            }
        } else {
            pending.push((ent, keys, task));
        }
    }

    task_queue.tasks = pending;
}
