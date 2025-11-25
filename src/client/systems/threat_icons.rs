use bevy::prelude::*;
use std::f32::consts::PI;

use crate::common::components::reaction_queue::*;

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
    server: Res<crate::client::resources::Server>,
) {
    let Ok(container) = container_query.single() else {
        warn!("ThreatIconContainer not found");
        return;
    };

    // Get the local player's queue (marked with Actor component)
    let Some((_player_entity, queue)) = player_query.iter().next() else {
        // No local player with queue yet - this is normal during startup
        return;
    };

    // Use game world time (synced from server Init event)
    // Threats use game world time, same as day/night cycle
    let now_ms = server.current_time(time.elapsed().as_millis());
    let now = std::time::Duration::from_millis(now_ms.min(u64::MAX as u128) as u64);

    // Get current icon count
    let current_icons: Vec<_> = icon_query.iter().collect();
    // Show ALL capacity slots (filled + empty ghost slots)
    let target_count = queue.capacity;

    // Check if we need to rebuild the UI (capacity changed or icons mismatched)
    let needs_rebuild = current_icons.len() != target_count;

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

        // Return early - new icons will be updated next frame
        // This prevents trying to update despawned entities
        return;
    } else {
        // Just update colors for filled/empty state transitions
        // And manage timer rings (spawn for filled, despawn for empty)
        for (entity, icon) in &current_icons {
            let is_filled = icon.index < queue.threats.len();

            // Update colors
            if is_filled {
                commands.entity(*entity).insert((
                    BorderColor::all(Color::srgb(0.8, 0.2, 0.2)),
                    BackgroundColor(Color::srgb(0.3, 0.1, 0.1)),
                ));
            } else {
                commands.entity(*entity).insert((
                    BorderColor::all(Color::srgba(0.6, 0.3, 0.1, 0.5)),
                    BackgroundColor(Color::srgba(0.15, 0.1, 0.05, 0.5)),
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
                // Need to spawn timer ring (circular border with color gradient)
                // Starts at 15% size (centered), grows to 100% as timer counts down
                commands.entity(*entity).with_children(|parent| {
                    parent.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            width: Val::Percent(15.),
                            height: Val::Percent(15.),
                            left: Val::Percent(42.5), // Center the 15% ring
                            top: Val::Percent(42.5),
                            border: UiRect::all(Val::Px(3.)),
                            ..default()
                        },
                        BorderColor::all(Color::srgba(1.0, 0.9, 0.0, 0.9)), // Start yellow
                        BorderRadius::all(Val::Percent(50.)), // Make circular
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

    // Update timer rings with:
    // - Color gradient (yellow → orange → red as time runs out)
    // - Growing size (small → large as time runs out)
    for (entity, ring, mut node) in ring_query.iter_mut() {
        if ring.index < queue.threats.len() {
            let threat = &queue.threats[ring.index];
            let elapsed = now.saturating_sub(threat.inserted_at);
            let progress = (elapsed.as_secs_f32() / threat.timer_duration.as_secs_f32()).clamp(0.0, 1.0);
            let remaining = 1.0 - progress;

            // Color gradient: Yellow (start) → Orange (50%) → Red (end)
            // remaining: 1.0 = just inserted, 0.0 = about to resolve
            let color = if remaining > 0.5 {
                // Yellow → Orange transition (100% to 50% remaining)
                let t = (remaining - 0.5) / 0.5; // 1.0 at start, 0.0 at midpoint
                Color::srgba(
                    1.0,
                    0.9 * t + 0.5 * (1.0 - t), // 0.9 → 0.5 (yellow to orange)
                    0.0,
                    0.9,
                )
            } else {
                // Orange → Red transition (50% to 0% remaining)
                let t = remaining / 0.5; // 1.0 at midpoint, 0.0 at end
                Color::srgba(
                    1.0,
                    0.5 * t, // 0.5 → 0.0 (orange to red)
                    0.0,
                    0.9,
                )
            };

            // Size grows from 15% to 100% as timer counts down
            // remaining: 1.0 = 15%, 0.0 = 100%
            let size_percent = 15.0 + (85.0 * progress);

            // Center the ring as it grows by offsetting it
            // When size is 15%, offset by 42.5% to center it
            // When size is 100%, offset by 0% (no offset needed)
            let offset_percent = (100.0 - size_percent) / 2.0;

            node.width = Val::Percent(size_percent);
            node.height = Val::Percent(size_percent);
            node.left = Val::Percent(offset_percent);
            node.top = Val::Percent(offset_percent);

            commands.entity(entity).insert(BorderColor::all(color));
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

    // Visual appearance: filled slots are opaque red, empty slots are dimmer red/orange
    let (border_color, background_color) = if is_filled {
        // Filled slot: bright red, opaque
        (Color::srgb(0.8, 0.2, 0.2), Color::srgb(0.3, 0.1, 0.1))
    } else {
        // Empty/ghost slot: dim red/orange border with darker background (50% opacity)
        // Subtle orange tint makes them stand out while looking "inactive"
        (Color::srgba(0.6, 0.3, 0.1, 0.5), Color::srgba(0.15, 0.1, 0.05, 0.5))
    };

    commands.entity(container).with_children(|parent| {
        // Threat icon background (circular)
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
            BorderColor::all(border_color),
            BorderRadius::all(Val::Percent(50.)), // Make circular
            BackgroundColor(background_color),
            ThreatIcon { index },
        ))
        .with_children(|parent| {
            // Timer ring (only visible for filled slots)
            // Circular border that changes color and grows as time runs out
            if is_filled {
                parent.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        width: Val::Percent(15.), // Start at 15%, grow to 100%
                        height: Val::Percent(15.),
                        left: Val::Percent(42.5), // Center the 15% ring
                        top: Val::Percent(42.5),
                        border: UiRect::all(Val::Px(3.)),
                        ..default()
                    },
                    BorderColor::all(Color::srgba(1.0, 0.9, 0.0, 0.9)), // Start yellow, will transition to red
                    BorderRadius::all(Val::Percent(50.)), // Make circular
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
    mut _commands: Commands,
    _icon_query: Query<Entity, With<ThreatIcon>>,
    mut clear_reader: EventReader<crate::common::message::Do>,
) {
    use crate::common::message::Event as GameEvent;

    for event in clear_reader.read() {
        if let GameEvent::ClearQueue { .. } = event.event {
            // TODO: Add proper flash/fade animation using bevy_easings
            // For now, do nothing - the update system will handle the visual change
            // by detecting the queue change and updating icon colors
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
