//! The terrain warmed ahead of the players: everything is built as it is
//! asked for, and as a player crosses into a new chunk, the chunks just past
//! its stream are queued ahead of need, so streaming sends what is kept.

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;

use common_bevy::chunk::{calculate_visible_chunks, loc_to_chunk, ChunkId, PlayerDiscoveryState, WorldDiscoveryCache, FIXED_STREAM_RADIUS};
use common_bevy::message::{Component, Do, Event};

use crate::systems::actor::ChunkTaskQueue;

/// How many rings of chunks past its stream a player's ground is warmed.
const AHEAD: u8 = 2;

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
    world_cache: Res<WorldDiscoveryCache>,
    mut task_queue: ResMut<ChunkTaskQueue>,
    mut warming: ResMut<Warming>,
    players: Query<(), With<PlayerDiscoveryState>>,
) {
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
