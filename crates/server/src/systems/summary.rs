use std::collections::HashSet;

use bevy::prelude::*;
use common_bevy::{
    chunk::FIXED_STREAM_RADIUS_WU,
    components::Loc,
    geometry::flat_top_tile_center,
    message::{Event, SummaryKey, *},
    summary::{compute_active_bands, visible_summary_cells_in_band},
};

use crate::resources::summary_cache::SummaryCache;

/// Per-client tracking of which summaries have been sent.
#[derive(bevy::prelude::Component)]
pub struct VisibleSummaryCache {
    /// All summary keys currently sent to this client.
    pub sent: HashSet<SummaryKey>,
    /// Player position at last recomputation (for throttling).
    pub last_pos: (i32, i32, i32),
    /// Pending additions that haven't been sent yet (budget-limited).
    pub pending: Vec<SummaryKey>,
}

impl Default for VisibleSummaryCache {
    fn default() -> Self {
        Self {
            sent: HashSet::new(),
            last_pos: (i32::MIN, i32::MIN, i32::MIN),
            pending: Vec::new(),
        }
    }
}

/// Minimum world-unit movement before recomputing the visible summary set.
const MIN_UPDATE_DISTANCE: f32 = 20.0;
/// Minimum z-level change before recomputing (handles vertical movement).
const MIN_UPDATE_Z: i32 = 10;
/// Maximum summaries to compute and send per player per frame.
/// Prevents blocking the server game loop on initial population.
const MAX_SUMMARIES_PER_FRAME: usize = 200;

/// Compute and send summary batches for each player.
///
/// Runs after `do_incremental` so player positions are up-to-date.
/// Only recomputes when the player has moved enough to change the visible set.
/// Budget-limited: sends at most `MAX_SUMMARIES_PER_FRAME` per player per frame,
/// queuing the rest for subsequent frames.
pub fn compute_and_send_summaries(
    mut writer: MessageWriter<Do>,
    mut query: Query<(Entity, &Loc, &mut VisibleSummaryCache)>,
    mut summary_cache: ResMut<SummaryCache>,
) {
    for (ent, loc, mut vis_cache) in query.iter_mut() {
        let q = loc.q;
        let r = loc.r;
        let z = loc.z;

        // Recompute visible set when player moves enough
        let (lq, lr, lz) = vis_cache.last_pos;
        let needs_recompute = if lq == i32::MIN {
            true // First frame
        } else {
            let (wx, wz) = flat_top_tile_center(q, r, 1.0);
            let (lwx, lwz) = flat_top_tile_center(lq, lr, 1.0);
            let horiz_dist = ((wx - lwx).powi(2) + (wz - lwz).powi(2)).sqrt();
            horiz_dist >= MIN_UPDATE_DISTANCE || (z - lz).abs() >= MIN_UPDATE_Z
        };

        if needs_recompute {
            vis_cache.last_pos = (q, r, z);

            let (cam_wx, cam_wz) = flat_top_tile_center(q, r, 1.0);

            const CAMERA_DISTANCE: f32 = 120.0;
            const HORIZON_MARGIN_DEG: f32 = 5.0;
            const MAX_GAMEPLAY_FOV: f32 = std::f32::consts::PI / 3.0;
            const RISE: f32 = 0.8;

            let margin = HORIZON_MARGIN_DEG.to_radians();
            let base_height = CAMERA_DISTANCE * (MAX_GAMEPLAY_FOV * 0.5 + margin).tan();
            let camera_height = base_height + z.max(0) as f32 * RISE;
            let far_ground = camera_height / margin.tan();

            let bands = compute_active_bands(far_ground);

            let mut visible_set: HashSet<SummaryKey> = HashSet::new();

            for band in &bands {
                if band.outer_wu <= FIXED_STREAM_RADIUS_WU {
                    continue;
                }

                let inner = band.inner_wu.max(FIXED_STREAM_RADIUS_WU);
                // No overlap extension — individual cells don't have the
                // lattice-spacing gaps that mesh regions do.
                let outer = band.outer_wu;

                let cells = visible_summary_cells_in_band(
                    band.r, cam_wx, cam_wz, inner, outer,
                );

                for (sq, sr) in cells {
                    visible_set.insert(SummaryKey { r: band.r, sq, sr });
                }
            }

            // Compute removals immediately (cheap — just set diff)
            let removals: Vec<SummaryKey> = vis_cache
                .sent
                .iter()
                .filter(|k| !visible_set.contains(k))
                .copied()
                .collect();

            if !removals.is_empty() {
                writer.write(Do {
                    event: Event::SummaryBatch {
                        ent,
                        additions: Vec::new(),
                        removals: removals.clone(),
                    },
                });
                for key in &removals {
                    vis_cache.sent.remove(key);
                }
            }

            // Queue new additions (will be sent budget-limited below)
            vis_cache.pending.clear();
            for key in &visible_set {
                if !vis_cache.sent.contains(key) {
                    vis_cache.pending.push(*key);
                }
            }

            if !vis_cache.pending.is_empty() {
                info!(
                    "[summary-server] player ({},{}) z={}: {} visible, {} to send, {} bands",
                    q, r, z, visible_set.len(), vis_cache.pending.len(), bands.len(),
                );
            }
        }

        // Drain pending additions with per-frame budget
        if vis_cache.pending.is_empty() {
            continue;
        }

        let batch_size = vis_cache.pending.len().min(MAX_SUMMARIES_PER_FRAME);
        let batch_keys: Vec<SummaryKey> = vis_cache.pending.drain(..batch_size).collect();
        let addition_data = summary_cache.batch_compute(&batch_keys);

        writer.write(Do {
            event: Event::SummaryBatch {
                ent,
                additions: addition_data,
                removals: Vec::new(),
            },
        });

        for key in &batch_keys {
            vis_cache.sent.insert(*key);
        }

        if !vis_cache.pending.is_empty() {
            debug!(
                "[summary-server] {} remaining in queue",
                vis_cache.pending.len(),
            );
        }
    }
}
