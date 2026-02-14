use bevy::prelude::*;

use crate::client::components::PoppingThreatIcon;
use crate::common::components::reaction_queue::*;
use crate::common::components::resources::Health;
use crate::common::components::ActorAttributes;
use crate::common::systems::combat::damage;

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

/// Marker component for the overflow counter (shows +N for hidden threats)
#[derive(Component)]
pub struct OverflowCounter;

pub const ICON_SIZE: f32 = 50.0;
const ICON_SPACING: f32 = 10.0; // Space between icons
const VERTICAL_OFFSET: f32 = -150.0; // Pixels above center (negative = up)

const POP_FLASH_DURATION: f32 = 0.08;
const POP_TRAVEL_DURATION: f32 = 0.4;
const RESOLVED_VERTICAL_OFFSET: f32 = -70.0; // Must match resolved_threats::VERTICAL_OFFSET

/// Map severity (estimated_damage / max_health) to an RGB color.
///
/// - 0–10%: Muted yellow-green → yellow
/// - 10–30%: Yellow → orange
/// - 30%+: Orange → intense red
pub fn severity_rgb(severity: f32) -> (f32, f32, f32) {
    let s = severity.clamp(0.0, 1.0);
    if s < 0.1 {
        // Muted yellow-green → yellow
        let t = s / 0.1;
        (
            0.6 + 0.4 * t, // 0.6 → 1.0
            0.8 - 0.1 * t, // 0.8 → 0.7
            0.2 * (1.0 - t), // 0.2 → 0.0
        )
    } else if s < 0.3 {
        // Yellow → orange
        let t = (s - 0.1) / 0.2;
        (
            1.0,
            0.7 - 0.35 * t, // 0.7 → 0.35
            0.0,
        )
    } else {
        // Orange → intense red
        let t = ((s - 0.3) / 0.3).min(1.0);
        (
            1.0,
            0.35 * (1.0 - t), // 0.35 → 0.0
            0.0,
        )
    }
}

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
        Pickable::IGNORE,
        ThreatIconContainer,
    ));
}

/// Update threat icons to match the player's reaction queue
/// Spawns/despawns icons as threats are added/removed
/// Updates timer rings to show time remaining
/// Colors filled icons by severity (estimated damage / max health)
pub fn update(
    mut commands: Commands,
    container_query: Query<Entity, With<ThreatIconContainer>>,
    icon_query: Query<(Entity, &ThreatIcon)>,
    mut ring_query: Query<(Entity, &ThreatTimerRing, &mut Node), Without<ThreatIcon>>,
    icon_with_children: Query<&Children, With<ThreatIcon>>,
    overflow_query: Query<Entity, With<OverflowCounter>>,
    mut text_query: Query<&mut Text>,
    player_query: Query<(Entity, &ReactionQueue, &ActorAttributes, &Health), With<crate::common::components::Actor>>,
    container_children: Query<&Children, With<ThreatIconContainer>>,
    time: Res<Time>,
    server: Res<crate::client::resources::Server>,
) {
    let Ok(container) = container_query.single() else {
        warn!("ThreatIconContainer not found");
        return;
    };

    // Get the local player's queue (marked with Actor component)
    let Some((_player_entity, queue, attrs, health)) = player_query.iter().next() else {
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
    let target_count = queue.window_size;

    // Check if we need to rebuild the UI (capacity changed or icons mismatched)
    let needs_rebuild = current_icons.len() != target_count;

    if needs_rebuild {
        // Despawn all existing icons and respawn with correct filled/empty state
        // This ensures timer rings are properly managed (only on filled slots)
        for (entity, _icon) in &current_icons {
            commands.entity(*entity).despawn();
        }

        // Despawn old overflow counter if it exists
        if let Ok(old_counter) = overflow_query.single() {
            commands.entity(old_counter).despawn();
        }

        // Spawn all capacity slots (filled + empty)
        for index in 0..target_count {
            let is_filled = index < queue.threats.len();
            spawn_threat_icon(&mut commands, container, index, queue.window_size, is_filled);
        }

        // Spawn overflow counter (+N indicator) to the right of icons
        let hidden_count = queue.threats.len().saturating_sub(queue.window_size);
        if hidden_count > 0 {
            spawn_overflow_counter(&mut commands, container, queue.window_size, hidden_count);
        }

        // Return early - new icons will be updated next frame
        // This prevents trying to update despawned entities
        return;
    } else {
        // Just update colors for filled/empty state transitions
        // And manage timer rings (spawn for filled, despawn for empty)
        for (entity, icon) in &current_icons {
            let is_filled = icon.index < queue.threats.len();

            // Update colors — severity-based for filled, dim for empty
            if is_filled {
                let threat = &queue.threats[icon.index];
                let estimated = damage::apply_passive_modifiers(
                    threat.damage, attrs, 0, 0, // No dominance/level for UI estimate
                );
                let severity = if health.max > 0.0 {
                    (estimated / health.max).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                let (r, g, b) = severity_rgb(severity);
                if let Ok(mut e) = commands.get_entity(*entity) {
                    e.insert((
                        BorderColor::all(Color::srgb(r, g, b)),
                        BackgroundColor(Color::srgb(r * 0.35, g * 0.35, b * 0.35)),
                    ));
                }
            } else {
                if let Ok(mut e) = commands.get_entity(*entity) {
                    e.insert((
                        BorderColor::all(Color::srgba(0.6, 0.3, 0.1, 0.5)),
                        BackgroundColor(Color::srgba(0.15, 0.1, 0.05, 0.5)),
                    ));
                }
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
                            border_radius: BorderRadius::all(Val::Percent(50.)), // Make circular
                            ..default()
                        },
                        BorderColor::all(Color::srgba(1.0, 0.9, 0.0, 0.9)), // Start yellow
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
    // - Color gradient (yellow -> orange -> red as time runs out)
    // - Growing size (small -> large as time runs out)
    for (entity, ring, mut node) in ring_query.iter_mut() {
        if ring.index < queue.threats.len() {
            let threat = &queue.threats[ring.index];
            let elapsed = now.saturating_sub(threat.inserted_at);
            let progress = (elapsed.as_secs_f32() / threat.timer_duration.as_secs_f32()).clamp(0.0, 1.0);
            let remaining = 1.0 - progress;

            // Color gradient: Yellow (start) -> Orange (50%) -> Red (end)
            // remaining: 1.0 = just inserted, 0.0 = about to resolve
            let color = if remaining > 0.5 {
                // Yellow -> Orange transition (100% to 50% remaining)
                let t = (remaining - 0.5) / 0.5; // 1.0 at start, 0.0 at midpoint
                Color::srgba(
                    1.0,
                    0.9 * t + 0.5 * (1.0 - t), // 0.9 -> 0.5 (yellow to orange)
                    0.0,
                    0.9,
                )
            } else {
                // Orange -> Red transition (50% to 0% remaining)
                let t = remaining / 0.5; // 1.0 at midpoint, 0.0 at end
                Color::srgba(
                    1.0,
                    0.5 * t, // 0.5 -> 0.0 (orange to red)
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

            if let Ok(mut e) = commands.get_entity(entity) {
                e.insert(BorderColor::all(color));
            }
        }
    }

    // Update overflow counter (+N for hidden threats)
    let hidden_count = queue.threats.len().saturating_sub(queue.window_size);
    let counter_exists = overflow_query.iter().next().is_some();

    if hidden_count > 0 && !counter_exists {
        // Spawn counter
        spawn_overflow_counter(&mut commands, container, queue.window_size, hidden_count);
    } else if hidden_count == 0 && counter_exists {
        // Despawn counter
        if let Ok(counter) = overflow_query.single() {
            commands.entity(counter).despawn();
        }
    } else if hidden_count > 0 && counter_exists {
        // Update counter text
        if let Ok(counter) = overflow_query.single() {
            if let Ok(children) = container_children.get(container) {
                for child in children.iter() {
                    if overflow_query.get(child).is_ok() {
                        if let Ok(text_children) = container_children.get(child) {
                            for text_child in text_children.iter() {
                                if let Ok(mut text) = text_query.get_mut(text_child) {
                                    **text = format!("+{}", hidden_count);
                                }
                            }
                        }
                    }
                }
            }
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
                border_radius: BorderRadius::all(Val::Percent(50.)), // Make circular
                ..default()
            },
            BorderColor::all(border_color),
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
                        border_radius: BorderRadius::all(Val::Percent(50.)), // Make circular
                        ..default()
                    },
                    BorderColor::all(Color::srgba(1.0, 0.9, 0.0, 0.9)), // Start yellow, will transition to red
                    BackgroundColor(Color::NONE),
                    ThreatTimerRing { index },
                ));
            }
        });
    });
}

/// Spawn overflow counter showing "+N" for hidden threats beyond the window
fn spawn_overflow_counter(
    commands: &mut Commands,
    container: Entity,
    window_size: usize,
    hidden_count: usize,
) {
    // Position to the right of all icons
    let total_width = (window_size as f32 * ICON_SIZE) + ((window_size - 1) as f32 * ICON_SPACING);
    let x_offset = -total_width / 2.0 + total_width + ICON_SPACING + 10.0; // 10px extra padding

    commands.entity(container).with_children(|parent| {
        parent.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(50.0),
                top: Val::Percent(50.0),
                margin: UiRect {
                    left: Val::Px(x_offset),
                    top: Val::Px(VERTICAL_OFFSET + 10.0), // Align with icons
                    ..default()
                },
                ..default()
            },
            OverflowCounter,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new(format!("+{}", hidden_count)),
                TextFont {
                    font_size: 24.0,
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.7, 0.0)), // Orange
            ));
        });
    });
}

/// Compute the front icon x-offset for a given queue capacity
pub fn front_icon_x_offset(capacity: usize) -> f32 {
    let total_width = (capacity as f32 * ICON_SIZE) + ((capacity.saturating_sub(1)) as f32 * ICON_SPACING);
    let start_x = -total_width / 2.0;
    start_x // index 0
}

/// Spawn pop animation when a threat resolves against the player.
/// Listens for ApplyDamage events targeting the local player.
pub fn spawn_pop_animation(
    mut commands: Commands,
    container_query: Query<Entity, With<ThreatIconContainer>>,
    player_query: Query<(&ReactionQueue, &Health), With<crate::common::components::Actor>>,
    input_queues: Res<crate::common::resources::InputQueues>,
    mut event_reader: MessageReader<crate::common::message::Do>,
    time: Res<Time>,
) {
    use crate::common::message::Event as GameEvent;

    let Ok(container) = container_query.single() else {
        return;
    };

    let Some(&player_entity) = input_queues.entities().next() else {
        return;
    };

    let Ok((queue, health)) = player_query.get(player_entity) else {
        return;
    };

    for event in event_reader.read() {
        if let GameEvent::ApplyDamage { ent, damage, .. } = event.event {
            if ent != player_entity {
                continue;
            }

            let severity = if health.max > 0.0 {
                (damage / health.max).clamp(0.0, 1.0)
            } else {
                1.0
            };

            let start_margin_left = front_icon_x_offset(queue.window_size);

            // Spawn pop icon as child of the container
            let (r, g, b) = severity_rgb(severity);
            commands.entity(container).with_children(|parent| {
                parent.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        width: Val::Px(ICON_SIZE),
                        height: Val::Px(ICON_SIZE),
                        left: Val::Percent(50.0),
                        top: Val::Percent(50.0),
                        margin: UiRect {
                            left: Val::Px(start_margin_left),
                            top: Val::Px(VERTICAL_OFFSET),
                            ..default()
                        },
                        border: UiRect::all(Val::Px(3.)),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        border_radius: BorderRadius::all(Val::Percent(50.)),
                        ..default()
                    },
                    // Start bright white for flash
                    BorderColor::all(Color::srgb(1.0, 1.0, 1.0)),
                    BackgroundColor(Color::srgba(r * 0.5, g * 0.5, b * 0.5, 0.9)),
                    PoppingThreatIcon {
                        spawn_time: time.elapsed(),
                        severity,
                        start_margin_left,
                        target_margin_top: RESOLVED_VERTICAL_OFFSET,
                    },
                ))
                .with_children(|parent| {
                    parent.spawn((
                        Text::new(format!("{:.0}", damage)),
                        TextFont {
                            font_size: 14.0,
                            ..default()
                        },
                        TextColor(Color::srgb(1.0, 1.0, 1.0)),
                    ));
                });
            });
        }
    }
}

/// Animate popping icons: flash phase then travel to resolved stack.
pub fn update_popping_icons(
    mut commands: Commands,
    mut query: Query<(Entity, &PoppingThreatIcon, &mut Node, &mut BorderColor, &mut BackgroundColor)>,
    time: Res<Time>,
) {
    for (entity, pop, mut node, mut border, mut bg) in &mut query {
        let elapsed = (time.elapsed() - pop.spawn_time).as_secs_f32();

        if elapsed >= POP_TRAVEL_DURATION {
            // Done — despawn (resolved entry appears simultaneously)
            commands.entity(entity).despawn();
            continue;
        }

        let (r, g, b) = severity_rgb(pop.severity);

        if elapsed < POP_FLASH_DURATION {
            // Flash phase: scale up 1.0 -> 1.3, color white -> severity
            let t = elapsed / POP_FLASH_DURATION;
            let scale = 1.0 + 0.3 * t;
            let size = ICON_SIZE * scale;

            // Lerp from white to severity color
            let cr = 1.0 + (r - 1.0) * t;
            let cg = 1.0 + (g - 1.0) * t;
            let cb = 1.0 + (b - 1.0) * t;

            node.width = Val::Px(size);
            node.height = Val::Px(size);
            // Adjust margin to keep centered as size changes
            let size_delta = (size - ICON_SIZE) / 2.0;
            node.margin.left = Val::Px(pop.start_margin_left - size_delta);
            node.margin.top = Val::Px(VERTICAL_OFFSET - size_delta);

            *border = BorderColor::all(Color::srgb(cr, cg, cb));
            bg.0 = Color::srgba(cr * 0.5, cg * 0.5, cb * 0.5, 0.9);
        } else {
            // Travel phase: shrink 1.3 -> 0.6, move from queue to resolved stack
            let travel_elapsed = elapsed - POP_FLASH_DURATION;
            let travel_duration = POP_TRAVEL_DURATION - POP_FLASH_DURATION;
            let t = (travel_elapsed / travel_duration).clamp(0.0, 1.0);
            // Ease-out for smooth deceleration
            let t_eased = 1.0 - (1.0 - t) * (1.0 - t);

            let scale = 1.3 - 0.7 * t_eased; // 1.3 -> 0.6
            let size = ICON_SIZE * scale;

            // Lerp vertical position from queue to resolved stack
            let start_top = VERTICAL_OFFSET;
            let end_top = pop.target_margin_top;
            let current_top = start_top + (end_top - start_top) * t_eased;

            // Center horizontally as size changes
            let size_delta = (ICON_SIZE - size) / 2.0;
            node.width = Val::Px(size);
            node.height = Val::Px(size);
            node.margin.left = Val::Px(pop.start_margin_left + size_delta);
            node.margin.top = Val::Px(current_top);

            // Fade slightly during travel
            let alpha = 1.0 - 0.3 * t_eased;
            *border = BorderColor::all(Color::srgba(r, g, b, alpha));
            bg.0 = Color::srgba(r * 0.35, g * 0.35, b * 0.35, alpha * 0.9);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_severity_rgb_zero() {
        let (r, g, b) = severity_rgb(0.0);
        // Should be muted yellow-green
        assert!(r > 0.5, "r={r}");
        assert!(g > 0.5, "g={g}");
        assert!(b > 0.0, "b={b} should have some blue/green");
    }

    #[test]
    fn test_severity_rgb_low() {
        let (r, g, b) = severity_rgb(0.05);
        // Between yellow-green and yellow
        assert!(r > 0.7, "r={r}");
        assert!(g > 0.5, "g={g}");
    }

    #[test]
    fn test_severity_rgb_ten_percent() {
        let (r, g, b) = severity_rgb(0.1);
        // Should be yellow
        assert!((r - 1.0).abs() < 0.01, "r={r}");
        assert!(g > 0.6, "g={g}");
        assert!(b < 0.01, "b={b}");
    }

    #[test]
    fn test_severity_rgb_mid() {
        let (r, g, b) = severity_rgb(0.2);
        // Should be orange-ish (between yellow and orange)
        assert!((r - 1.0).abs() < 0.01, "r={r}");
        assert!(g > 0.3 && g < 0.7, "g={g} should be between yellow and orange");
        assert!(b < 0.01, "b={b}");
    }

    #[test]
    fn test_severity_rgb_high() {
        let (r, g, b) = severity_rgb(0.6);
        // Should be red (past 30% threshold)
        assert!((r - 1.0).abs() < 0.01, "r={r}");
        assert!(g < 0.1, "g={g} should be near zero for red");
        assert!(b < 0.01, "b={b}");
    }

    #[test]
    fn test_severity_rgb_clamped_above_one() {
        let (r, g, b) = severity_rgb(2.0);
        // Should clamp to max severity (same as 1.0)
        let (r1, g1, b1) = severity_rgb(1.0);
        assert!((r - r1).abs() < 0.01);
        assert!((g - g1).abs() < 0.01);
        assert!((b - b1).abs() < 0.01);
    }

    #[test]
    fn test_severity_rgb_monotonic_red() {
        // Red channel should be monotonically non-decreasing
        let severities = [0.0, 0.05, 0.1, 0.2, 0.3, 0.5, 0.8, 1.0];
        let mut prev_r = 0.0;
        for s in severities {
            let (r, _, _) = severity_rgb(s);
            assert!(r >= prev_r - 0.01, "Red not monotonic at severity={s}: {r} < {prev_r}");
            prev_r = r;
        }
    }

    #[test]
    fn test_severity_rgb_green_decreasing() {
        // Green should generally decrease (gets more red over time)
        let (_, g_low, _) = severity_rgb(0.05);
        let (_, g_high, _) = severity_rgb(0.5);
        assert!(g_low > g_high, "Green should decrease: {g_low} vs {g_high}");
    }

    #[test]
    fn test_front_icon_x_offset_single() {
        let x = front_icon_x_offset(1);
        // Single icon: total_width = 50, start_x = -25
        assert!((x - (-25.0)).abs() < 0.01);
    }

    #[test]
    fn test_front_icon_x_offset_three() {
        let x = front_icon_x_offset(3);
        // 3 icons: total_width = 3*50 + 2*10 = 170, start_x = -85
        assert!((x - (-85.0)).abs() < 0.01);
    }
}
