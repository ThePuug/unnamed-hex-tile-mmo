use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, futures_lite::future};
use common_bevy::{
    components::{heading::Heading, Loc},
    geometry::flat_top_tile_center,
    message::{Event, SummaryData, SummaryKey, *},
    summary::{compute_active_bands, mesh_region_lattice, sample_center_water, sample_center_z, summary_lattice},
    summary_mesh::{MeshRegionKey, visible_lod_regions},
};

use crate::resources::event_registry::EventRegistry;
use crate::resources::summary_cache::SummaryCache;

/// Per-client tracking of the summary regions sent and wanted.
#[derive(bevy::prelude::Component, Default)]
pub struct VisibleSummaryCache {
    /// Regions sent to this client, whole: a region is the unit, its
    /// summaries never sent apart.
    pub sent_regions: HashSet<MeshRegionKey>,
    /// Regions this client wants that are not computed yet, nearest first;
    /// the feeder takes from the front as task slots free up.
    pending: Vec<MeshRegionKey>,
    /// The chunk and heading the last pass was made for. A pass is made
    /// again only when either changes.
    passed_for: Option<(common_bevy::chunk::ChunkId, Heading)>,
}

/// In-flight async summary computation tasks (one per mesh region), and
/// the clients each one is computed for.
#[derive(Resource, Default)]
pub struct SummaryTaskQueue {
    tasks: Vec<(MeshRegionKey, Task<Vec<SummaryData>>)>,
    /// Clients waiting on each region in flight: whoever asked first and
    /// whoever asked since, so a region is computed once and sent to all.
    waiting: HashMap<MeshRegionKey, Vec<Entity>>,
}

/// Maximum in-flight async region tasks across all players.
const MAX_SUMMARY_TASKS: usize = 16;

/// The level whose band, and everything inside it, streams all round the
/// player for context; beyond its outer edge only the sector around the
/// heading streams.
const CONTEXT_LEVEL: u32 = 4;

/// Whether a region is streamed for a player at `(px, pz)` heading `dir`:
/// inside the context radius always, beyond it when any of it can lie in
/// the sector, its centre judged with the slack of its own circumradius.
fn in_stream(key: &MeshRegionKey, px: f32, pz: f32, dir: Vec2) -> bool {
    let (wx, wz) = region_center(key);
    let to = Vec2::new(wx - px, wz - pz);
    let dist = to.length();
    let circum = common_bevy::summary::mesh_region_extent_wu(key.r) / 3.0_f32.sqrt();
    if dist <= common_bevy::summary::threshold_horiz(CONTEXT_LEVEL) + circum {
        return true;
    }
    let slack = (circum / dist).min(1.0).asin();
    to.normalize().dot(dir) >= (common::camera::STREAM_SECTOR_HALF_ANGLE + slack).cos()
}

/// Rebuild what each player wants when their chunk or heading changes:
/// the regions in stream that are not sent, sent at once from the cache
/// where every summary is computed, queued as pending otherwise, nearest
/// first. Standing still costs nothing.
pub fn pass_summary_regions(
    mut writer: MessageWriter<Do>,
    mut query: Query<(Entity, &Loc, &Heading, &mut VisibleSummaryCache)>,
    summary_cache: Res<SummaryCache>,
    timings: Res<crate::plugins::metrics::SystemTimings>,
) {
    let _t = timings.scope("summary_pass");
    let region_lat = mesh_region_lattice();
    let bands = compute_active_bands(common_bevy::summary::reach_wu());
    for (ent, loc, heading, mut vis_cache) in query.iter_mut() {
        let here = (common_bevy::chunk::loc_to_chunk(**loc), *heading);
        if vis_cache.passed_for == Some(here) {
            continue;
        }
        vis_cache.passed_for = Some(here);

        let (cam_wx, cam_wz) = flat_top_tile_center(loc.q, loc.r, 1.0);
        let facing = heading.to_world_dir();
        let visible_regions = visible_lod_regions(&bands, cam_wx, cam_wz, common_bevy::chunk::FIXED_STREAM_APOTHEM_WU);

        // No removals on the wire: the client treats summary data as durable
        // (its mesh lifecycle is position-based, its cache is session-long),
        // so `sent_regions` tracks "ever sent". Memory grows with explored
        // area on both sides — bounded by the world, revisit if it matters.
        let mut cached_additions = Vec::new();
        let mut pending = Vec::new();
        for rk in &visible_regions {
            if vis_cache.sent_regions.contains(rk) || !in_stream(rk, cam_wx, cam_wz, facing) {
                continue;
            }
            let cached: Option<Vec<SummaryData>> = region_lat.tiles_in_cell((rk.mn, rk.mm))
                .map(|(sq, sr)| {
                    summary_cache.get(&SummaryKey { r: rk.r, sq, sr })
                        .map(|(center_z, water)| SummaryData { r: rk.r, sq, sr, center_z, water })
                })
                .collect();
            match cached {
                Some(data) => {
                    cached_additions.extend(data);
                    vis_cache.sent_regions.insert(*rk);
                }
                None => pending.push(*rk),
            }
        }
        pending.sort_by(|a, b| {
            region_distance_sq(a, cam_wx, cam_wz)
                .partial_cmp(&region_distance_sq(b, cam_wx, cam_wz))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        vis_cache.pending = pending;

        if !cached_additions.is_empty() {
            writer.write(Do {
                event: Event::SummaryBatch { ent, additions: cached_additions, removals: Vec::new() },
            });
        }
    }
}

/// Feed the task budget from the front of every player's pending list, a
/// region at a time round the players so no one starves. A region already
/// in flight for another player gains a waiter instead of a task.
pub fn dispatch_summary_tasks(
    mut query: Query<(Entity, &mut VisibleSummaryCache)>,
    registry: Res<EventRegistry>,
    mut task_queue: ResMut<SummaryTaskQueue>,
    timings: Res<crate::plugins::metrics::SystemTimings>,
) {
    let _t = timings.scope("summary_dispatch");
    let mut budget = MAX_SUMMARY_TASKS.saturating_sub(task_queue.tasks.len());
    let mut players: Vec<(Entity, Mut<VisibleSummaryCache>)> = query.iter_mut().collect();
    let mut any = true;
    while budget > 0 && any {
        any = false;
        for (ent, vis_cache) in players.iter_mut() {
            if budget == 0 { break; }
            if vis_cache.pending.is_empty() { continue; }
            let rk = vis_cache.pending.remove(0);
            any = true;
            if let Some(waiters) = task_queue.waiting.get_mut(&rk) {
                waiters.push(*ent);
                continue;
            }
            budget -= 1;
            task_queue.waiting.insert(rk, vec![*ent]);
            let reg = registry.clone();
            let task = AsyncComputeTaskPool::get().spawn(async move {
                let rl = mesh_region_lattice();
                rl.tiles_in_cell((rk.mn, rk.mm))
                    .map(|(sq, sr)| {
                        let center_z = sample_center_z(rk.r, sq, sr, |q, r| reg.elevation_at(q, r));
                        let water = sample_center_water(rk.r, sq, sr, |q, r| reg.water_at(q, r));
                        SummaryData { r: rk.r, sq, sr, center_z, water }
                    })
                    .collect()
            });
            task_queue.tasks.push((rk, task));
        }
    }
}

/// Poll completed async summary tasks: into the cache, then to every
/// client waiting on the region.
pub fn poll_summary_tasks(
    mut writer: MessageWriter<Do>,
    mut summary_cache: ResMut<SummaryCache>,
    mut task_queue: ResMut<SummaryTaskQueue>,
    mut query: Query<&mut VisibleSummaryCache>,
    timings: Res<crate::plugins::metrics::SystemTimings>,
) {
    let _t = timings.scope("summary_poll");
    let current = std::mem::take(&mut task_queue.tasks);
    let mut pending = Vec::new();

    for (region_key, mut task) in current {
        let Some(results) = block_on(future::poll_once(&mut task)) else {
            pending.push((region_key, task));
            continue;
        };
        for data in &results {
            summary_cache.insert(SummaryKey { r: data.r, sq: data.sq, sr: data.sr }, data.center_z, data.water);
        }
        for ent in task_queue.waiting.remove(&region_key).unwrap_or_default() {
            let Ok(mut vis_cache) = query.get_mut(ent) else { continue };
            vis_cache.sent_regions.insert(region_key);
            writer.write(Do {
                event: Event::SummaryBatch { ent, additions: results.clone(), removals: Vec::new() },
            });
        }
    }

    task_queue.tasks = pending;
}

/// World-space centre of a mesh region.
fn region_center(key: &MeshRegionKey) -> (f32, f32) {
    let summary_lat = summary_lattice(key.r);
    let region_lat = mesh_region_lattice();
    let region_center = region_lat.cell_center((key.mn, key.mm));
    let (cq, cr) = summary_lat.cell_center(region_center);
    flat_top_tile_center(cq, cr, 1.0)
}

/// Squared world-space distance from a mesh region's center to a point.
fn region_distance_sq(key: &MeshRegionKey, px: f32, pz: f32) -> f32 {
    let (wx, wz) = region_center(key);
    let dx = wx - px;
    let dz = wz - pz;
    dx * dx + dz * dz
}

#[cfg(test)]
mod tests {
    use super::*;
    use common_bevy::summary::{compute_active_bands, reach_wu, threshold_horiz};
    use common_bevy::summary_mesh::visible_lod_regions;

    /// Everything inside the context radius streams whatever the heading;
    /// beyond it the sector keeps the share of the ring its angle covers.
    #[test]
    fn the_sector_keeps_the_context_ring_and_a_share_of_the_rest() {
        let bands = compute_active_bands(reach_wu());
        let (px, pz) = (1234.5, -987.0);
        let regions = visible_lod_regions(&bands, px, pz, common_bevy::chunk::FIXED_STREAM_APOTHEM_WU);
        let context = threshold_horiz(CONTEXT_LEVEL);
        let (mut near, mut far, mut far_kept) = (0usize, 0usize, 0usize);
        let facing = Heading::from_slot(5).to_world_dir();
        for rk in &regions {
            let d = region_distance_sq(rk, px, pz).sqrt();
            if d <= context {
                near += 1;
                assert!(in_stream(rk, px, pz, facing), "context region {rk:?} at {d} left out");
            } else {
                far += 1;
                far_kept += in_stream(rk, px, pz, facing) as usize;
            }
        }
        let share = far_kept as f32 / far as f32;
        let sector = common::camera::STREAM_SECTOR_HALF_ANGLE / std::f32::consts::PI;
        println!("context {near} regions; far {far_kept} of {far} kept ({share:.2}) for a sector of {sector:.2}");
        assert!(share > sector * 0.8 && share < sector * 1.6, "kept {far_kept} of {far} far regions ({share:.2}) for a sector of {sector:.2}; {near} near");
    }
}
