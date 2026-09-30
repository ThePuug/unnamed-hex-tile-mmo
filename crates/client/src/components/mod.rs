use bevy::prelude::*;

use common_bevy::chunk::ChunkId;


#[derive(Clone, Component, Copy)]
#[relationship(relationship_target = AnimatedBy)]
pub struct Animates(pub Entity);

#[derive(Clone, Component, Copy, Deref)]
#[relationship_target(relationship = Animates)]
pub struct AnimatedBy(Entity);

#[derive(Component)]
pub enum Info {
    Time,
    DistanceIndicator,  // Shows distance from haven
}

/// Links a mesh entity to its chunk. Read by diagnostics only.
#[derive(Component)]
#[allow(dead_code)]
pub struct ChunkMesh {
    pub chunk_id: ChunkId,
}

/// The actor the view follows: the camera, the reaction queue and the
/// resource bars read it. The local player, unless a recording views a
/// fighter it staged.
#[derive(Component)]
pub struct Viewed;

/// A UI root showing the viewed actor's fight, which a recording that views
/// a fighter keeps on screen.
#[derive(Component)]
pub struct ViewHud;

/// Debug sphere marker - shows actor origin position (toggles with terrain grid)
#[derive(Component)]
pub struct PlayerOriginDebug;

/// Target indicator component for showing which entity will be targeted
#[derive(Component)]
pub struct TargetIndicator {
    pub indicator_type: crate::systems::target_indicator::IndicatorType,
}

// TODO: TierBadge component - deferred until proper 3D text setup

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

/// A bar drawn over a target in the world, its lane's
/// (`systems::target_frame::Lane`): what it measures, and the fill it
/// shows now, which eases toward the measure.
#[derive(Component)]
pub struct WorldBar {
    pub measure: Measure,
    /// Fill shown, 0.0 to 1.0
    pub current_fill: f32,
}

/// What a [`WorldBar`] measures of its target
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Measure {
    Health,
    /// How far through its recovery the target is, drawn flush under its health
    Recovery,
}

/// The part of a [`WorldBar`] whose width is its fill
#[derive(Component)]
pub struct WorldBarFill;

/// Holds the capacity dots drawn over a target's bars, its lane's
#[derive(Component)]
pub struct ThreatQueueDots;

/// Marker component for individual capacity dots in world-space threat display
#[derive(Component)]
pub struct ThreatCapacityDot {
    pub index: usize,
}

/// Resolved threat entry - fades out after showing damage resolution
#[derive(Component)]
pub struct ResolvedThreatEntry {
    pub spawn_time: std::time::Duration,
    pub lifetime: f32,  // 4.0 seconds
    /// Its colour: a blow's by severity, damage over time's its own
    pub rgb: (f32, f32, f32),
}

/// Marker for the resolved threats stack, beside the highway's hit line
#[derive(Component)]
pub struct ResolvedThreatsContainer;

/// Marker for combat log panel
#[derive(Component)]
pub struct CombatLogPanel;

/// Marker for combat log content (scrollable)
#[derive(Component)]
pub struct CombatLogContent;

/// Combat log entry with metadata for color coding
#[derive(Component)]
pub struct CombatLogEntry;

/// Marker for NPC entities in death pose (lying on side for 3s before despawn)
#[derive(Component)]
pub struct DeathMarker {
    pub death_time: std::time::Duration,
}

/// Marker for compass container
#[derive(Component)]
pub struct CompassContainer;
/// A remote entity is simulated from its last movement intent: whether it
/// travels, whether opposite its heading, and the fixed time not yet
/// attributed to it.
#[derive(Component, Debug)]
pub struct RemoteMotion {
    pub moving: bool,
    pub back: bool,
    pub residual_us: u32,
}
