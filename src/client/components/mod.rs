use bevy::{
    prelude::*,
    render::primitives::Aabb,
    tasks::Task
};

use crate::common::chunk::ChunkId;

#[derive(Clone, Component, Copy)]
#[relationship(relationship_target = AnimatedBy)]
pub struct Animates(pub Entity);

#[derive(Clone, Component, Copy, Deref)]
#[relationship_target(relationship = Animates)]
pub struct AnimatedBy(Entity);

#[derive(Component)]
pub enum Info {
    Time,
}

#[derive(Component, Default)]
pub struct Terrain {
    pub task_regenerate_mesh: Option<Task<(Mesh,Aabb)>>,
    pub task_start_regenerate_mesh: bool,
    pub last_tile_count: usize,
}

/// Target indicator component for showing which entity will be targeted
#[derive(Component)]
pub struct TargetIndicator {
    pub indicator_type: crate::client::systems::target_indicator::IndicatorType,
}

/// Marks a tile entity as belonging to a specific chunk
#[derive(Component, Copy, Clone, Debug)]
pub struct ChunkMember(pub ChunkId);

/// Floating text component for damage numbers and other temporary text
/// Used with UI Node entities that follow world-space positions
#[derive(Component)]
pub struct FloatingText {
    /// Time when this text was spawned
    pub spawn_time: std::time::Duration,
    /// How long this text should live (in seconds)
    pub lifetime: f32,
    /// World position this text is attached to
    pub world_position: bevy::math::Vec3,
    /// Upward velocity (world units per second)
    pub velocity: f32,
}