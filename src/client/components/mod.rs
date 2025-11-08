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
    DistanceIndicator,  // ADR-014 Phase 4: Shows distance from haven, zone, and expected enemy level
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

// TODO: TierBadge component - deferred until proper 3D text setup (ADR-010 Phase 5)

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

/// World-space health bar component with interpolation state
#[derive(Component)]
pub struct WorldHealthBar {
    /// Current displayed fill ratio (0.0 to 1.0) for smooth interpolation
    pub current_fill: f32,
}

/// Marker for hostile target health bar
#[derive(Component)]
pub struct HostileHealthBar;

/// Marker for ally target health bar
#[derive(Component)]
pub struct AllyHealthBar;

/// Threat queue dots container - shows capacity dots above health bars
#[derive(Component)]
pub struct ThreatQueueDots;

/// Marker for hostile target threat queue dots
#[derive(Component)]
pub struct HostileQueueDots;

/// Marker for ally target threat queue dots
#[derive(Component)]
pub struct AllyQueueDots;

/// Marker component for individual capacity dots in world-space threat display
#[derive(Component)]
pub struct ThreatCapacityDot {
    pub index: usize,
}