use bevy::{prelude::*, tasks::Task};
use bevy_camera::primitives::Aabb;

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

/// Component linking mesh entities to their chunk for eviction
#[derive(Component)]
pub struct ChunkMesh {
    pub chunk_id: ChunkId,
}

/// Debug sphere marker - shows actor origin position (toggles with terrain grid)
#[derive(Component)]
pub struct PlayerOriginDebug;

/// Target indicator component for showing which entity will be targeted
#[derive(Component)]
pub struct TargetIndicator {
    pub indicator_type: crate::client::systems::target_indicator::IndicatorType,
}

// TODO: TierBadge component - deferred until proper 3D text setup (ADR-010 Phase 5)

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

/// Resolved threat entry - fades out after showing damage resolution (ADR-025)
#[derive(Component)]
pub struct ResolvedThreatEntry {
    pub spawn_time: std::time::Duration,
    pub lifetime: f32,  // 4.0 seconds
    pub damage: f32,
    pub severity: f32,       // estimated_damage / max_health for color
    pub appear_delay: f32,   // seconds before entry becomes visible (synced with pop travel)
}

/// Pop animation icon that travels from queue front to resolved stack
#[derive(Component)]
pub struct PoppingThreatIcon {
    pub spawn_time: std::time::Duration,
    pub severity: f32,
    pub start_margin_left: f32,  // queue front x position
    pub target_margin_top: f32,  // resolved stack y position
}

/// Marker for resolved threats container (ADR-025)
#[derive(Component)]
pub struct ResolvedThreatsContainer;

/// Marker for combat log panel (ADR-025)
#[derive(Component)]
pub struct CombatLogPanel;

/// Marker for combat log content (scrollable) (ADR-025)
#[derive(Component)]
pub struct CombatLogContent;

/// Combat log entry with metadata for color coding (ADR-025)
#[derive(Component)]
pub struct CombatLogEntry {
    pub timestamp: String,  // Pre-formatted "HH:MM:SS"
    pub is_player_damage: bool,  // true = dealt, false = taken
}

/// Marker for NPC entities in death pose (lying on side for 3s before despawn)
#[derive(Component)]
pub struct DeathMarker {
    pub death_time: std::time::Duration,
}