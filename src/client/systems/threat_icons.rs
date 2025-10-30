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
    mut ring_query: Query<(Entity, &ThreatTimerRing, &mut Node), Without<ThreatIcon>>,
    icon_with_children: Query<&Children, With<ThreatIcon>>,
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
    // Show ALL capacity slots (filled + empty ghost slots)
    let target_count = queue.capacity;

    // Check if we need to rebuild the UI (capacity changed or icons mismatched)
    let needs_rebuild = current_icons.len() != target_count;

    // Debug logging
    if needs_rebuild {
        info!(
            "Threat UI rebuild: current_icons={}, target_count={}, filled={}, capacity={}",
            current_icons.len(),
            target_count,
            queue.threats.len(),
            queue.capacity
        );
    }

    if needs_rebuild {
        // Despawn all existing icons and respawn with correct filled/empty state
        // This ensures timer rings are properly managed (only on filled slots)
        for (entity, _icon) in &current_icons {
            commands.entity(*entity).despawn();
        }

        // Spawn all capacity slots (filled + empty)
        for index in 0..target_count {
            let is_filled = index < queue.threats.len();
            spawn_threat_icon(&mut commands, container, index, queue.capacity, is_filled);
        }
    } else {
        // Just update colors for filled/empty state transitions
        // And manage timer rings (spawn for filled, despawn for empty)
        for (entity, icon) in &current_icons {
            let is_filled = icon.index < queue.threats.len();

            // Update colors
            if is_filled {
                commands.entity(*entity).insert((
                    BorderColor(Color::srgb(0.8, 0.2, 0.2)),
                    BackgroundColor(Color::srgb(0.3, 0.1, 0.1)),
                ));
            } else {
                commands.entity(*entity).insert((
                    BorderColor(Color::srgba(0.4, 0.4, 0.4, 0.3)),
                    BackgroundColor(Color::srgba(0.2, 0.2, 0.2, 0.3)),
                ));
            }

            // Manage timer ring child
            let has_timer_ring = icon_with_children.get(*entity)
                .ok()
                .and_then(|children| {
                    children.iter().any(|child| ring_query.get(child).is_ok())
                        .then_some(())
                })
                .is_some();

            if is_filled && !has_timer_ring {
                // Need to spawn timer ring
                commands.entity(*entity).with_children(|parent| {
                    parent.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            width: Val::Percent(100.),
                            height: Val::Percent(100.),
                            border: UiRect::all(Val::Px(2.)),
                            ..default()
                        },
                        BorderColor(Color::srgba(1.0, 0.8, 0.0, 0.8)),
                        BackgroundColor(Color::NONE),
                        ThreatTimerRing { index: icon.index },
                    ));
                });
            } else if !is_filled && has_timer_ring {
                // Need to despawn timer ring
                if let Ok(children) = icon_with_children.get(*entity) {
                    for child in children.iter() {
                        if ring_query.get(child).is_ok() {
                            commands.entity(child).despawn();
                        }
                    }
                }
            }
        }
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
/// `is_filled` determines if this slot contains a threat (opaque) or is empty (ghost/transparent)
fn spawn_threat_icon(
    commands: &mut Commands,
    container: Entity,
    index: usize,
    capacity: usize,
    is_filled: bool,
) {
    // Calculate horizontal position in a line (centered)
    let total_width = (capacity as f32 * ICON_SIZE) + ((capacity - 1) as f32 * ICON_SPACING);
    let start_x = -total_width / 2.0;
    let x_offset = start_x + (index as f32 * (ICON_SIZE + ICON_SPACING));

    // Visual appearance: filled slots are opaque red, empty slots are transparent grey
    let (border_color, background_color) = if is_filled {
        // Filled slot: bright red, opaque
        (Color::srgb(0.8, 0.2, 0.2), Color::srgb(0.3, 0.1, 0.1))
    } else {
        // Empty/ghost slot: grey, transparent (30% opacity)
        (Color::srgba(0.4, 0.4, 0.4, 0.3), Color::srgba(0.2, 0.2, 0.2, 0.3))
    };

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
            BorderColor(border_color),
            BackgroundColor(background_color),
            ThreatIcon { index },
        ))
        .with_children(|parent| {
            // Timer ring (only visible for filled slots)
            if is_filled {
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
            }
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
