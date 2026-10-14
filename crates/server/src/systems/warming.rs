//! The terrain warmed ahead of the players, where the world's heights are
//! the ladder's: before the server serves anyone, the haven's chunks
//! generated and its summaries' ladder cells built; and as a player crosses
//! into a new chunk, the chunks just past its stream queued ahead of need,
//! so streaming sends what is kept.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy::prelude::*;
use qrz::Qrz;

use common::summary::{center_tile, scale, LOD_LEVELS};
use common_bevy::chunk::{calculate_visible_chunks, loc_to_chunk, ChunkId, PlayerDiscoveryState, WorldDiscoveryCache, FIXED_STREAM_RADIUS};
use common_bevy::message::{Component, Do, Event};
use common_bevy::summary::threshold_horiz;

use crate::resources::event_registry::EventRegistry;
use crate::systems::actor::{generate_chunk, ChunkTaskQueue};
use crate::systems::dens::Dens;

/// How many rings of chunks past its stream a player's ground is warmed.
const AHEAD: u8 = 2;

/// The terrain warmed around a tile: every chunk a player there streams and
/// the next [`AHEAD`] rings generated, kept, and their dens found, then the
/// summaries of each level the server sends out to its band's far edge.
/// The chunks are made on one thread: the stack composes slower on many
/// (issue #22).
pub fn around(registry: &EventRegistry, centre: (i32, i32)) -> (WorldDiscoveryCache, Dens) {
    let started = std::time::Instant::now();
    let chunk = loc_to_chunk(Qrz { q: centre.0, r: centre.1, z: 0 });
    let chunks = calculate_visible_chunks(chunk, FIXED_STREAM_RADIUS + AHEAD);
    info!("warming the terrain: {} chunks round ({}, {})", chunks.len(), centre.0, centre.1);
    let mut cache = WorldDiscoveryCache::default();
    let mut dens = Dens::default();
    for chunk_id in chunks {
        let (terrain, found) = generate_chunk(chunk_id, registry);
        cache.chunks.insert(chunk_id, Arc::new(terrain));
        cache.access_order.get_or_insert(chunk_id, || ());
        for (site, habitat) in found {
            dens.found(site, habitat);
        }
    }
    info!("warming the terrain: chunks generated in {:.1} s", started.elapsed().as_secs_f64());
    for &r in &LOD_LEVELS[1..] {
        let s = scale(r);
        // A tile `d` steps away along the hex lattice lies at least 1.5·d
        // world units off, a tile apart being √3.
        let reach = (threshold_horiz(r) / 1.5).ceil() as i32 / s + 2;
        let (cq, cr) = (centre.0 / s, centre.1 / s);
        let cells: Vec<(i64, i64)> = (-reach..=reach)
            .flat_map(|a| (-reach..=reach).map(move |b| (cq + a, cr + b)))
            .filter(|&(sq, sr)| {
                let (q, rr) = center_tile(r, sq, sr);
                let (dq, dr) = (q - centre.0, rr - centre.1);
                dq.abs().max(dr.abs()).max((dq + dr).abs()) <= reach * s
            })
            .map(|(sq, sr)| (sq as i64, sr as i64))
            .collect();
        registry.warm_level(s as i64, &cells);
        info!("warming the terrain: {} summaries {s} wide warm at {:.1} s", cells.len(), started.elapsed().as_secs_f64());
    }
    (cache, dens)
}

/// The chunks queued ahead of the players, and the chunk each player was
/// warmed round last.
#[derive(Resource, Default)]
pub struct Warming {
    queued: HashSet<ChunkId>,
    at: HashMap<Entity, ChunkId>,
}

/// As a player crosses into a new chunk, its stream and the next [`AHEAD`]
/// rings of chunks queued ahead of need where not generated yet.
pub fn warm_ahead(
    mut reader: MessageReader<Do>,
    registry: Res<EventRegistry>,
    world_cache: Res<WorldDiscoveryCache>,
    mut task_queue: ResMut<ChunkTaskQueue>,
    mut warming: ResMut<Warming>,
    players: Query<(), With<PlayerDiscoveryState>>,
) {
    if !registry.is_laddered() {
        return;
    }
    for message in reader.read() {
        let Do { event: Event::Incremental { ent, component: Component::Loc(loc) } } = message else { continue };
        if players.get(*ent).is_err() {
            continue;
        }
        let chunk = loc_to_chunk(**loc);
        if warming.at.insert(*ent, chunk) == Some(chunk) {
            continue;
        }
        for chunk_id in calculate_visible_chunks(chunk, FIXED_STREAM_RADIUS + AHEAD) {
            if !world_cache.chunks.contains_key(&chunk_id) && warming.queued.insert(chunk_id) {
                task_queue.prefetch(chunk_id);
            }
        }
    }
}
