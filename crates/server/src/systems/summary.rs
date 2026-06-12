use std::collections::HashSet;

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, futures_lite::future};
use common_bevy::{
    components::Loc,
    geometry::flat_top_tile_center,
    message::{Event, SummaryData, SummaryKey, *},
    summary::{compute_active_bands, mesh_region_lattice, sample_center_z, summary_lattice},
    summary_mesh::{MeshRegionKey, visible_lod_regions},
};

use crate::resources::event_registry::EventRegistry;
use crate::resources::summary_cache::SummaryCache;

/// Per-client tracking of which summaries have been sent.
#[derive(bevy::prelude::Component, Default)]
pub struct VisibleSummaryCache {
    /// All summary keys currently sent to this client.
    pub sent: HashSet<SummaryKey>,
}

/// In-flight async summary computation tasks (one per mesh region).
#[derive(Resource, Default)]
pub struct SummaryTaskQueue {
    tasks: Vec<(Entity, MeshRegionKey, Task<Vec<SummaryData>>)>,
    in_flight: HashSet<MeshRegionKey>,
}

/// Maximum in-flight async region tasks across all players.
const MAX_SUMMARY_TASKS: usize = 16;

/// Compute visible summary sets per player, dispatch async tasks for cache misses.
///
/// Operates at MeshRegionKey granularity (271 summaries per region), matching
/// the flyover dispatch pattern. Regions dispatch nearest-first.
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

        let (cam_wx, cam_wz) = flat_top_tile_center(q, r, 1.0);

        let cam_h = common::camera::camera_height(common::camera::MAX_GAMEPLAY_FOV)
            + z.max(0) as f32 * common::camera::RISE;
        let far_ground = cam_h / common::camera::HORIZON_MARGIN_DEG.to_radians().tan();

        let bands = compute_active_bands(far_ground);

        let visible_regions = visible_lod_regions(
            &bands,
            cam_wx,
            cam_wz,
            common_bevy::chunk::FIXED_STREAM_APOTHEM_WU,
        );

        // No removals on the wire: the client treats summary data as durable
        // (its mesh lifecycle is position-based, its cache is session-long),
        // so `sent` tracks "ever sent" and removal batches would be ignored
        // bandwidth. Memory grows with explored area on both sides — bounded
        // by the world, revisit if it matters.
        let region_lat = mesh_region_lattice();

        // Partition regions: fully sent, cache-hit, or needs async
        let mut cached_additions = Vec::new();
        let mut to_dispatch: Vec<MeshRegionKey> = Vec::new();

        for rk in &visible_regions {
            let mut any_new = false;
            let mut all_cached = true;
            for (sq, sr) in region_lat.tiles_in_cell((rk.mn, rk.mm)) {
                let key = SummaryKey { r: rk.r, sq, sr };
                if vis_cache.sent.contains(&key) { continue; }
                any_new = true;
                if let Some(center_z) = summary_cache.get(&key) {
                    cached_additions.push(SummaryData { r: rk.r, sq, sr, center_z });
                    vis_cache.sent.insert(key);
                } else {
                    all_cached = false;
                }
            }
            if any_new && !all_cached && !task_queue.in_flight.contains(rk) {
                to_dispatch.push(*rk);
            }
        }

        if !cached_additions.is_empty() {
            writer.write(Do {
                event: Event::SummaryBatch { ent, additions: cached_additions, removals: Vec::new() },
            });
        }

        // Sort nearest-first, apply task budget
        to_dispatch.sort_by(|a, b| {
            region_distance_sq(a, cam_wx, cam_wz)
                .partial_cmp(&region_distance_sq(b, cam_wx, cam_wz))
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let budget = MAX_SUMMARY_TASKS.saturating_sub(task_queue.in_flight.len());
        let dispatched: Vec<MeshRegionKey> = to_dispatch.into_iter().take(budget).collect();

        if !dispatched.is_empty() {
            info!(
                "[summary-server] player ({},{}) z={}: dispatching {} regions ({} cached, {} in-flight)",
                q, r, z, dispatched.len(),
                vis_cache.sent.len(),
                task_queue.in_flight.len(),
            );

            let pool = AsyncComputeTaskPool::get();
            for rk in dispatched {
                task_queue.in_flight.insert(rk);
                let reg = registry.clone();
                let task = pool.spawn(async move {
                    let rl = mesh_region_lattice();
                    rl.tiles_in_cell((rk.mn, rk.mm))
                        .map(|(sq, sr)| {
                            let center_z = sample_center_z(rk.r, sq, sr, |q, r| reg.elevation_at(q, r));
                            SummaryData { r: rk.r, sq, sr, center_z }
                        })
                        .collect()
                });
                task_queue.tasks.push((ent, rk, task));
            }
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

    for (ent, region_key, mut task) in current {
        if let Some(results) = block_on(future::poll_once(&mut task)) {
            // Insert into global cache
            for data in &results {
                summary_cache.insert(
                    SummaryKey { r: data.r, sq: data.sq, sr: data.sr },
                    data.center_z,
                );
            }
            task_queue.in_flight.remove(&region_key);
            // Update per-client sent tracking
            if let Ok(mut vis_cache) = query.get_mut(ent) {
                for data in &results {
                    vis_cache.sent.insert(SummaryKey { r: data.r, sq: data.sq, sr: data.sr });
                }
            }
            // Send to client
            if !results.is_empty() {
                writer.write(Do {
                    event: Event::SummaryBatch { ent, additions: results, removals: Vec::new() },
                });
            }
        } else {
            pending.push((ent, region_key, task));
        }
    }

    task_queue.tasks = pending;
}

/// Squared world-space distance from a mesh region's center to a point.
fn region_distance_sq(key: &MeshRegionKey, px: f32, pz: f32) -> f32 {
    let summary_lat = summary_lattice(key.r);
    let region_lat = mesh_region_lattice();
    let region_center = region_lat.cell_center((key.mn, key.mm));
    let (cq, cr) = summary_lat.cell_center(region_center);
    let (wx, wz) = flat_top_tile_center(cq, cr, 1.0);
    let dx = wx - px;
    let dz = wz - pz;
    dx * dx + dz * dz
}
