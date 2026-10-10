use bevy::prelude::*;

/// The entity is sliding to `destination` under an ability (a lunge, a
/// knockback, a flank's circle). While present nothing else moves what is
/// drawn, the tile that reaches the destination re-anchors the position
/// there, and the slide ends when its time is up — the server sends that
/// tile with the slide, so it cannot be what ends it.
#[derive(Component, Clone, Copy, Debug)]
pub struct Displacing {
    /// Standing-height tile the slide ends on.
    pub destination: qrz::Qrz,
    /// The client's elapsed time the slide ends at.
    pub ends_at: std::time::Duration,
    /// The tile a slide round a target circles, facing the way it goes;
    /// any other slide keeps the heading it has.
    pub around: Option<qrz::Qrz>,
}

/// The directional light that is the sun.
#[derive(Debug, Default, Component)]
pub struct Sun();

/// The directional light that is the moon.
#[derive(Debug, Default, Component)]
pub struct Moon();

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

/// A ring under a target: which target it follows, and the tile its mesh
/// was last built on, so it is rebuilt only as that changes.
#[derive(Component)]
pub struct TargetIndicator {
    pub indicator_type: crate::systems::target_indicator::IndicatorType,
    pub tile: Option<qrz::Qrz>,
}

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

/// Marker for a line of the combat log
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
