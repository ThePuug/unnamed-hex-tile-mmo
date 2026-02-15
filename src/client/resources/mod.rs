use bevy::prelude::*;
use bimap::BiMap;
use std::collections::HashSet;

use crate::common::chunk::ChunkId;

#[derive(Debug, Default, Deref, DerefMut, Resource)]
pub struct EntityMap(BiMap<Entity,Entity>);

#[derive(Debug, Resource)]
pub struct Server {
    /// Server's game world time when Init event was received
    pub server_time_at_init: u128,
    /// Client's elapsed time when Init event was received
    pub client_time_at_init: u128,
    /// Last time we sent a ping (for periodic pings)
    pub last_ping_time: u128,
    /// Smoothed network latency estimate (exponential moving average)
    pub smoothed_latency: u128,
}

impl Default for Server {
    fn default() -> Self {
        Self {
            server_time_at_init: 0,
            client_time_at_init: 0,
            last_ping_time: 0,
            smoothed_latency: 50, // Initial estimate: 50ms
        }
    }
}

impl Server {
    /// Calculate the current game world time (used for both threats and day/night)
    /// Game world time = server_time_at_init + (client_now - client_at_init)
    pub fn current_time(&self, client_now: u128) -> u128 {
        let time_since_init = client_now.saturating_sub(self.client_time_at_init);
        self.server_time_at_init.saturating_add(time_since_init)
    }
}

use bevy::tasks::Task;
use bevy_camera::primitives::Aabb;
use std::collections::HashMap;

/// Shared material for all chunk meshes
#[derive(Resource)]
pub struct TerrainMaterial {
    pub handle: Handle<StandardMaterial>,
}

/// Tracks pending async mesh generation tasks per chunk
#[derive(Resource, Default)]
pub struct PendingChunkMeshes {
    pub tasks: HashMap<ChunkId, Task<(Mesh, Aabb)>>,
}

/// Tracks which chunks have been received on the client
#[derive(Debug, Default, Resource)]
pub struct LoadedChunks {
    pub chunks: HashSet<ChunkId>,
}

impl LoadedChunks {
    /// Mark a chunk as loaded
    pub fn insert(&mut self, chunk_id: ChunkId) {
        self.chunks.insert(chunk_id);
    }

    /// Get chunks that should be evicted (not in the active set)
    pub fn get_evictable(&self, active_chunks: &HashSet<ChunkId>) -> Vec<ChunkId> {
        self.chunks
            .iter()
            .filter(|chunk_id| !active_chunks.contains(chunk_id))
            .copied()
            .collect()
    }

    /// Remove evicted chunks from tracking
    pub fn evict(&mut self, chunk_ids: &[ChunkId]) {
        for chunk_id in chunk_ids {
            self.chunks.remove(chunk_id);
        }
    }
}
