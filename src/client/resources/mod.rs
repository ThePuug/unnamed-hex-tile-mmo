use bevy::prelude::*;
use bimap::BiMap;
use std::collections::HashSet;

use crate::common::chunk::ChunkId;

#[derive(Debug, Default, Deref, DerefMut, Resource)]
pub struct EntityMap(BiMap<Entity,Entity>);

#[derive(Debug, Default, Resource)]
pub struct Server {
    pub elapsed_offset: u128,
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
