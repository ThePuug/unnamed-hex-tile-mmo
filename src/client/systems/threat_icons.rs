use bevy::prelude::*;
use std::f32::consts::PI;

use crate::{
    common::components::{behaviour::Behaviour, reaction_queue::*},
};

/// Marker component to track which player the UI is for
#[derive(Component)]
pub struct LocalPlayerUi;

/// Marker component for the threat icons container
#[derive(Component)]
pub struct ThreatIconContainer;

/// Marker component for individual threat icons
#[derive(Component)]
pub struct ThreatIcon {
    /// Index in the threat queue (0 = oldest, front of queue)
    pub index: usize,
}

/// Marker component for the timer ring around a threat icon
#[derive(Component)]
pub struct ThreatTimerRing {
    /// Index in the threat queue
    pub index: usize,
}

const ICON_SIZE: f32 = 50.0;
const ICON_SPACING: f32 = 10.0; // Space between icons
const VERTICAL_OFFSET: f32 = -150.0; // Pixels above center (negative = up)

/// Setup threat icon container in the HUD
/// Creates a container that will hold threat icons in a horizontal line above the player
pub fn setup(
    mut commands: Commands,
    query: Query<Entity, Added<Camera3d>>,
) {
    let camera = query.single().expect("query did not return exactly one result");

    // Container for threat icons - centered on screen
    // Uses screen center as reference point (50%, 50%)
    commands.spawn((
        UiTargetCamera(camera),
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.),
            height: Val::Percent(100.),
            ..default()
        },
        ThreatIconContainer,
    ));
}

/// Update threat icons to match the player's reaction queue
/// Spawns/despawns icons as threats are added/removed
/// Updates timer rings to show time remaining
pub fn update(
    mut commands: Commands,
    container_query: Query<Entity, With<ThreatIconContainer>>,
    icon_query: Query<(Entity, &ThreatIcon)>,
    mut ring_query: Query<(Entity, &ThreatTimerRing, &mut Node)>,
    // Use Actor marker to identify the local player (only one entity has it)
    player_query: Query<(Entity, &ReactionQueue), With<crate::common::components::Actor>>,
    time: Res<Time>,
) {
    let Ok(container) = container_query.get_single() else {
        warn!("ThreatIconContainer not found");
        return;
    };

    // Get the local player's queue (marked with Actor component)
    let Some((_player_entity, queue)) = player_query.iter().next() else {
        // No local player with queue yet - this is normal during startup
        return;
    };

    let now = time.elapsed();

    // Get current icon count
    let current_icons: Vec<_> = icon_query.iter().collect();
    let target_count = queue.threats.len();

    // Debug logging
    if target_count > 0 && current_icons.len() != target_count {
        info!(
            "Threat UI update: current_icons={}, target_count={}, capacity={}",
            current_icons.len(),
            target_count,
            queue.capacity
        );
    }

    // Despawn excess icons
    for (entity, icon) in &current_icons {
        if icon.index >= target_count {
            info!("Despawning excess icon at index {}", icon.index);
            commands.entity(*entity).despawn_recursive();
        }
    }

    // Spawn missing icons
    for index in current_icons.len()..target_count {
        info!("Spawning new icon at index {}", index);
        spawn_threat_icon(&mut commands, container, index, queue.capacity);
    }

    // Update timer rings
    for (_entity, ring, mut node) in ring_query.iter_mut() {
        if ring.index < queue.threats.len() {
            let threat = &queue.threats[ring.index];
            let elapsed = now.saturating_sub(threat.inserted_at);
            let progress = (elapsed.as_secs_f32() / threat.timer_duration.as_secs_f32()).clamp(0.0, 1.0);

            // Update the timer ring's arc (simulated by width - proper arc would need custom rendering)
            // For now, we'll just update opacity to show time remaining
            let remaining = 1.0 - progress;
            // Timer ring will be more visible when time is running out
            node.width = Val::Percent(100.0 * remaining);
        }
    }
}

/// Spawn a single threat icon at the specified index
fn spawn_threat_icon(
    commands: &mut Commands,
    container: Entity,
    index: usize,
    capacity: usize,
) {
    // Calculate horizontal position in a line (centered)
    let total_width = (capacity as f32 * ICON_SIZE) + ((capacity - 1) as f32 * ICON_SPACING);
    let start_x = -total_width / 2.0;
    let x_offset = start_x + (index as f32 * (ICON_SIZE + ICON_SPACING));

    commands.entity(container).with_children(|parent| {
        // Threat icon background
        parent.spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(ICON_SIZE),
                height: Val::Px(ICON_SIZE),
                // Position relative to screen center (50%, 50%)
                left: Val::Percent(50.0),
                top: Val::Percent(50.0),
                // Offset in a horizontal line above player
                margin: UiRect {
                    left: Val::Px(x_offset),
                    top: Val::Px(VERTICAL_OFFSET),
                    ..default()
                },
                border: UiRect::all(Val::Px(3.)),
                ..default()
            },
            BorderColor(Color::srgb(0.8, 0.2, 0.2)), // Red border
            BackgroundColor(Color::srgb(0.3, 0.1, 0.1)), // Dark red background
            ThreatIcon { index },
        ))
        .with_children(|parent| {
            // Timer ring (overlays the icon)
            parent.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.),
                    height: Val::Percent(100.),
                    border: UiRect::all(Val::Px(2.)),
                    ..default()
                },
                BorderColor(Color::srgba(1.0, 0.8, 0.0, 0.8)), // Yellow/orange timer ring
                BackgroundColor(Color::NONE),
                ThreatTimerRing { index },
            ));
        });
    });
}

/// Calculate the angle for an icon at the given index
/// Icons are distributed evenly in a circle, starting from top (90 degrees)
fn calculate_icon_angle(index: usize, capacity: usize) -> f32 {
    // Start at top (90 degrees = PI/2) and go clockwise
    let base_angle = PI / 2.0;
    let angle_step = (2.0 * PI) / capacity.max(1) as f32;
    base_angle - (angle_step * index as f32)
}

/// Handle clearing animations when threats are removed
/// Shows a flash/fade effect when player dodges
pub fn animate_clear(
    mut commands: Commands,
    icon_query: Query<Entity, With<ThreatIcon>>,
    mut clear_reader: EventReader<crate::common::message::Do>,
) {
    use crate::common::message::Event as GameEvent;

    for event in clear_reader.read() {
        if let GameEvent::ClearQueue { .. } = event.event {
            // Flash effect - despawn all icons (they'll respawn on next update if needed)
            for entity in &icon_query {
                commands.entity(entity).despawn_recursive();
            }
            // TODO: Add proper flash/fade animation using bevy_easings
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_icon_angle_single() {
        // Single threat should be at top
        let angle = calculate_icon_angle(0, 1);
        assert!((angle - PI / 2.0).abs() < 0.01);
    }

    #[test]
    fn test_calculate_icon_angle_distribution() {
        // 4 threats should be evenly distributed (90 degrees apart)
        let capacity = 4;
        let angle0 = calculate_icon_angle(0, capacity);
        let angle1 = calculate_icon_angle(1, capacity);
        let diff = angle0 - angle1;
        assert!((diff - PI / 2.0).abs() < 0.01); // Should be 90 degrees
    }
}
