//! Target Detail Frame System
//!
//! Shows detailed information about the currently targeted enemy in top-right corner.
//! This enables tactical decision-making by answering questions like:
//! - "Can they dodge?" (see their stamina)
//! - "Is their queue full?" (see threat indicators)
//! - "How close to death?" (exact HP numbers)
//!
//! From ADR-008 Phase 5 requirements.

use bevy::prelude::*;

use crate::{
    common::{
        components::{Actor, entity_type::*, resources::*, reaction_queue::*, Loc},
    },
    client::resources::Server,
};

/// Marker component for the target frame container
#[derive(Component)]
pub struct TargetFrame;

/// Marker component for the target's name text
#[derive(Component)]
pub struct TargetNameText;

/// Marker component for the target's health bar fill
#[derive(Component)]
pub struct TargetHealthBar;

/// Marker component for the target's health text
#[derive(Component)]
pub struct TargetHealthText;

/// Marker component for the target's threat queue container
#[derive(Component)]
pub struct TargetQueueContainer;

/// Marker component for the capacity dots container
#[derive(Component)]
pub struct DotsContainer;

/// Marker component for individual threat icons in target frame
#[derive(Component)]
pub struct TargetThreatIcon {
    pub index: usize,
}

/// Marker component for capacity dot (filled or empty)
#[derive(Component)]
pub struct CapacityDot {
    pub index: usize,
}

/// Marker component for timer ring within threat icon
#[derive(Component)]
pub struct TargetThreatTimerRing {
    pub index: usize,
}

/// Marker component for attack type icon text within threat icon
#[derive(Component)]
pub struct TargetThreatAttackIcon {
    pub index: usize,
}

/// Marker component for the target's triumvirate text (approach/resilience)
#[derive(Component)]
pub struct TargetTriumvirateText;

/// Marker component for the ally frame container
#[derive(Component)]
pub struct AllyFrame;

/// Marker component for the ally's name text
#[derive(Component)]
pub struct AllyNameText;

/// Marker component for the ally's health bar fill
#[derive(Component)]
pub struct AllyHealthBar;

/// Marker component for the ally's health text
#[derive(Component)]
pub struct AllyHealthText;

/// Marker component for the ally's triumvirate text (approach/resilience)
#[derive(Component)]
pub struct AllyTriumvirateText;

/// Marker component for the ally's threat queue container
#[derive(Component)]
pub struct AllyQueueContainer;

/// Marker component for the ally's capacity dots container
#[derive(Component)]
pub struct AllyDotsContainer;

/// Marker component for individual threat icons in ally frame
#[derive(Component)]
pub struct AllyThreatIcon {
    pub index: usize,
}

/// Marker component for ally capacity dot (filled or empty)
#[derive(Component)]
pub struct AllyCapacityDot {
    pub index: usize,
}

/// Setup the target detail frame in top-right corner
/// Frame is hidden by default and shown when a target is selected
pub fn setup(
    mut commands: Commands,
    query: Query<Entity, Added<Camera3d>>,
) {
    let camera = query.single().expect("query did not return exactly one result");

    // Main container - top-right corner, 280px wide
    commands.spawn((
        UiTargetCamera(camera),
        Node {
            position_type: PositionType::Absolute,
            width: Val::Px(280.),
            height: Val::Auto,
            top: Val::Px(10.),
            right: Val::Px(10.),
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(10.)),
            row_gap: Val::Px(6.),
            ..default()
        },
        BorderColor(Color::srgba(0.5, 0.5, 0.5, 0.8)),
        BackgroundColor(Color::srgba(0.1, 0.1, 0.1, 0.85)),
        Visibility::Hidden,  // Hidden by default
        TargetFrame,
    ))
    .with_children(|parent| {
        // Header: Entity name
        parent.spawn((
            Text::new("Enemy Name"),
            TextFont {
                font_size: 14.0,
                ..default()
            },
            TextColor(Color::WHITE),
            TargetNameText,
        ));

        // Triumvirate row: Approach / Resilience (centered below name, colored by origin)
        parent.spawn((
            Text::new("Direct / Primal"),
            TextFont {
                font_size: 11.0,
                ..default()
            },
            TextColor(Color::srgb(0.8, 0.8, 0.8)), // Will be set dynamically based on origin
            TargetTriumvirateText,
        ));

        // Health bar container
        parent.spawn((
            Node {
                width: Val::Percent(100.),
                height: Val::Px(16.),
                border: UiRect::all(Val::Px(2.)),
                justify_content: JustifyContent::FlexEnd,
                align_items: AlignItems::Center,
                padding: UiRect::all(Val::Px(2.)),
                ..default()
            },
            BorderColor(Color::srgb(0.3, 0.3, 0.3)),
            BackgroundColor(Color::srgb(0.2, 0.1, 0.0)),  // Dark orange/brown background
        ))
        .with_children(|parent| {
            // Health fill bar (orange, distinct from player red)
            parent.spawn((
                Node {
                    width: Val::Percent(100.),
                    height: Val::Percent(100.),
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.),
                    top: Val::Px(0.),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.9, 0.5, 0.0)),  // Orange
                TargetHealthBar,
            ));

            // Health text (exact numbers, right-aligned on bar)
            parent.spawn((
                Text::new("100/100"),
                TextFont {
                    font_size: 12.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                Node {
                    position_type: PositionType::Relative,
                    ..default()
                },
                TargetHealthText,
            ));
        });

        // TODO: Resource bars (stamina/mana) for elite enemies/players
        // MVP: Wild Dog enemies don't have resources yet

        // Threat queue section (shown only when target has a queue)
        parent.spawn((
            Node {
                width: Val::Percent(100.),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.),
                ..default()
            },
            Visibility::Hidden,  // Hidden by default, shown when target has queue
            TargetQueueContainer,
        ))
        .with_children(|parent| {
            // Top row: Warning icon + capacity dots
            parent.spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(6.),
                    align_items: AlignItems::Center,
                    ..default()
                },
            ))
            .with_children(|parent| {
                // Capacity dots container (will be populated dynamically)
                parent.spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(3.),
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    DotsContainer,
                ));
            });

            // Bottom row: Container for threat icons (will be populated dynamically)
            parent.spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(5.),
                    ..default()
                },
            ));
        });
    });

    // Ally frame - positioned to the left of hostile frame (side-by-side in top-right)
    commands.spawn((
        UiTargetCamera(camera),
        Node {
            position_type: PositionType::Absolute,
            width: Val::Px(280.),
            height: Val::Auto,
            top: Val::Px(10.),
            right: Val::Px(300.),  // 280px (hostile width) + 10px gap + 10px margin
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(10.)),
            row_gap: Val::Px(6.),
            ..default()
        },
        BorderColor(Color::srgba(0.0, 0.6, 0.0, 0.8)),  // Green border
        BackgroundColor(Color::srgba(0.0, 0.15, 0.0, 0.85)),  // Dark green background
        Visibility::Hidden,  // Hidden by default
        AllyFrame,
    ))
    .with_children(|parent| {
        // Header: Ally name
        parent.spawn((
            Text::new("Ally Name"),
            TextFont {
                font_size: 14.0,
                ..default()
            },
            TextColor(Color::srgb(0.8, 1.0, 0.8)),  // Light green tint
            AllyNameText,
        ));

        // Triumvirate row: Approach / Resilience
        parent.spawn((
            Text::new("Direct / Primal"),
            TextFont {
                font_size: 11.0,
                ..default()
            },
            TextColor(Color::srgb(0.8, 0.8, 0.8)),
            AllyTriumvirateText,
        ));

        // Health bar container
        parent.spawn((
            Node {
                width: Val::Percent(100.),
                height: Val::Px(16.),
                border: UiRect::all(Val::Px(2.)),
                justify_content: JustifyContent::FlexEnd,
                align_items: AlignItems::Center,
                padding: UiRect::all(Val::Px(2.)),
                ..default()
            },
            BorderColor(Color::srgb(0.0, 0.4, 0.0)),  // Dark green border
            BackgroundColor(Color::srgb(0.0, 0.15, 0.0)),  // Dark green background
        ))
        .with_children(|parent| {
            // Health fill bar (green theme)
            parent.spawn((
                Node {
                    width: Val::Percent(100.),
                    height: Val::Percent(100.),
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.),
                    top: Val::Px(0.),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.2, 0.8, 0.2)),  // Bright green fill
                AllyHealthBar,
            ));

            // Health text (exact numbers, right-aligned on bar)
            parent.spawn((
                Text::new("100/100"),
                TextFont {
                    font_size: 12.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                Node {
                    position_type: PositionType::Relative,
                    ..default()
                },
                AllyHealthText,
            ));
        });

        // TODO: Resource bars (stamina/mana) - show when ally support abilities are added

        // Threat queue section (shown only when ally has a queue)
        parent.spawn((
            Node {
                width: Val::Percent(100.),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.),
                ..default()
            },
            Visibility::Hidden,  // Hidden by default, shown when ally has queue
            AllyQueueContainer,
        ))
        .with_children(|parent| {
            // Top row: Warning icon + capacity dots
            parent.spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(6.),
                    align_items: AlignItems::Center,
                    ..default()
                },
            ))
            .with_children(|parent| {
                // Capacity dots container (will be populated dynamically)
                parent.spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(3.),
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    AllyDotsContainer,
                ));
            });

            // Bottom row: Container for threat icons (will be populated dynamically)
            parent.spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(5.),
                    ..default()
                },
            ));
        });
    });
}

/// Update target frame to show current target's information
/// Uses sticky targeting - target persists until a new target is selected or current dies/despawns
pub fn update(
    mut frame_query: Query<&mut Visibility, With<TargetFrame>>,
    mut name_text_query: Query<&mut Text, (With<TargetNameText>, Without<TargetHealthText>, Without<TargetTriumvirateText>)>,
    mut name_color_query: Query<&mut TextColor, With<TargetNameText>>,
    mut triumvirate_query: Query<(&mut Text, &mut TextColor), (With<TargetTriumvirateText>, Without<TargetNameText>, Without<TargetHealthText>)>,
    mut health_bar_query: Query<&mut Node, With<TargetHealthBar>>,
    mut health_text_query: Query<&mut Text, (With<TargetHealthText>, Without<TargetNameText>, Without<TargetTriumvirateText>)>,
    player_query: Query<(&Health, &crate::common::components::target::Target), With<Actor>>,
    target_query: Query<(&EntityType, &Health, Option<&ReactionQueue>)>,
) {
    // Get local player and target
    let Ok((player_health, target)) = player_query.get_single() else {
        return;
    };

    // Don't show target frame while dead
    if player_health.state <= 0.0 {
        for mut visibility in &mut frame_query {
            *visibility = Visibility::Hidden;
        }
        return;
    }

    // Read sticky target from Target.last_target
    // This persists even when you turn away (entity field clears but last_target remains)
    let last_target = target.last_target;

    // Show/hide frame and update content based on target
    if let Some(target_ent) = last_target {
        // Target exists - show frame and update content
        for mut visibility in &mut frame_query {
            *visibility = Visibility::Visible;
        }

        if let Ok((entity_type, target_health, _queue_opt)) = target_query.get(target_ent) {
            // Update entity name
            for mut text in &mut name_text_query {
                **text = entity_type.display_name().to_string();
            }

            // Update triumvirate display (only for actors)
            if let EntityType::Actor(actor_impl) = entity_type {
                // Set name color based on origin
                for mut color in &mut name_color_query {
                    let (r, g, b) = actor_impl.origin.color();
                    color.0 = Color::srgb(r, g, b);
                }

                // Set triumvirate text (Approach / Resilience)
                for (mut text, mut color) in &mut triumvirate_query {
                    **text = format!("{} / {}",
                        actor_impl.approach.display_name(),
                        actor_impl.resilience.display_name()
                    );
                    // Triumvirate text also colored by origin (slightly dimmer)
                    let (r, g, b) = actor_impl.origin.color();
                    color.0 = Color::srgb(r * 0.8, g * 0.8, b * 0.8);
                }
            }

            // Update health bar width
            for mut node in &mut health_bar_query {
                let percent = if target_health.max > 0.0 {
                    (target_health.state / target_health.max * 100.0).clamp(0.0, 100.0)
                } else {
                    0.0
                };
                node.width = Val::Percent(percent);
            }

            // Update health text (exact numbers)
            for mut text in &mut health_text_query {
                **text = format!("{:.0}/{:.0}", target_health.state, target_health.max);
            }
        }
    } else {
        // No target - hide frame
        for mut visibility in &mut frame_query {
            *visibility = Visibility::Hidden;
        }
    }
}

/// Update target frame queue display
/// Separate system to avoid hitting Bevy's system parameter limits
pub fn update_queue(
    mut commands: Commands,
    player_query: Query<&crate::common::components::target::Target, With<Actor>>,
    mut queue_container_query: Query<&mut Visibility, With<TargetQueueContainer>>,
    queue_children_query: Query<&Children, With<TargetQueueContainer>>,
    dots_container_query: Query<Entity, With<DotsContainer>>,
    mut threat_icon_query: Query<(Entity, &TargetThreatIcon, &mut BorderColor), (Without<CapacityDot>, Without<TargetThreatTimerRing>)>,
    capacity_dot_query: Query<(Entity, &CapacityDot)>,
    mut dot_node_query: Query<(&mut BackgroundColor, &mut BorderColor), (With<CapacityDot>, Without<TargetThreatIcon>, Without<TargetThreatTimerRing>)>,
    mut timer_ring_query: Query<(&TargetThreatTimerRing, &mut Node, &mut BorderColor), (Without<TargetThreatIcon>, Without<CapacityDot>)>,
    mut attack_icon_query: Query<(&TargetThreatAttackIcon, &mut Text)>,
    target_query: Query<Option<&ReactionQueue>>,
    time: Res<Time>,
    server: Res<Server>,
) {
    // Get player's target
    let Ok(player_target) = player_query.get_single() else {
        return;
    };

    // Check if we have a target
    let Some(target_ent) = player_target.entity else {
        // No target - hide queue
        for mut visibility in &mut queue_container_query {
            *visibility = Visibility::Hidden;
        }
        return;
    };

    // Get target's reaction queue
    let Ok(queue_opt) = target_query.get(target_ent) else {
        return;
    };

    // Update queue container visibility and content
    if let Ok(mut queue_visibility) = queue_container_query.get_single_mut() {
        if let Some(queue) = queue_opt {
            // Target has a queue - show it
            *queue_visibility = Visibility::Visible;

            // Get actual queue capacity from the component
            let queue_capacity = queue.capacity;
            let filled_slots = queue.threats.len();
            let is_full = queue.is_full();

            // Check if we need to rebuild capacity dots (capacity changed)
            let current_dots: Vec<_> = capacity_dot_query.iter().collect();
            let dots_need_rebuild = current_dots.len() != queue_capacity;

            if dots_need_rebuild {
                // Capacity changed - despawn all and respawn with correct count
                for (dot_ent, _) in &current_dots {
                    commands.entity(*dot_ent).despawn_recursive();
                }

                // Spawn capacity dots in the dots container
                if let Ok(dots_container_ent) = dots_container_query.get_single() {
                    commands.entity(dots_container_ent).with_children(|parent| {
                        for i in 0..queue_capacity {
                            let is_filled = i < filled_slots;

                            // Use circular UI nodes instead of text characters
                            let (bg_color, border_color) = if is_full && is_filled {
                                // Full queue: filled dots are bright red with red border
                                (Color::srgb(1.0, 0.3, 0.3), Color::srgb(1.0, 0.3, 0.3))
                            } else if is_filled {
                                // Filled but not full: orange-red fill with border
                                (Color::srgb(0.9, 0.4, 0.4), Color::srgb(0.9, 0.4, 0.4))
                            } else {
                                // Empty: transparent with gray border
                                (Color::NONE, Color::srgb(0.5, 0.5, 0.5))
                            };

                            parent.spawn((
                                Node {
                                    width: Val::Px(8.),
                                    height: Val::Px(8.),
                                    border: UiRect::all(Val::Px(1.)),
                                    ..default()
                                },
                                BorderColor(border_color),
                                BorderRadius::all(Val::Percent(50.)), // Make circular
                                BackgroundColor(bg_color),
                                CapacityDot { index: i },
                            ));
                        }
                    });
                }
            } else {
                // Capacity unchanged - just update colors of existing dots
                for (dot_ent, dot) in &current_dots {
                    if let Ok((mut bg_color, mut border_color)) = dot_node_query.get_mut(*dot_ent) {
                        let is_filled = dot.index < filled_slots;

                        let (new_bg, new_border) = if is_full && is_filled {
                            // Full queue: filled dots are bright red with red border
                            (Color::srgb(1.0, 0.3, 0.3), Color::srgb(1.0, 0.3, 0.3))
                        } else if is_filled {
                            // Filled but not full: orange-red fill with border
                            (Color::srgb(0.9, 0.4, 0.4), Color::srgb(0.9, 0.4, 0.4))
                        } else {
                            // Empty: transparent with gray border
                            (Color::NONE, Color::srgb(0.5, 0.5, 0.5))
                        };

                        bg_color.0 = new_bg;
                        border_color.0 = new_border;
                    }
                }
            }

            // Update threat icons (LIMIT TO FIRST 3)
            if let Ok(queue_children) = queue_children_query.get_single() {
                let threat_icons_container = queue_children.get(1).copied();

                if let Some(icons_ent) = threat_icons_container {
                    let now_ms = server.current_time(time.elapsed().as_millis());
                    let now = std::time::Duration::from_millis(now_ms.min(u64::MAX as u128) as u64);

                    // Filter out expired threats and limit to first 3
                    let active_threats: Vec<_> = queue.threats.iter()
                        .filter(|threat| {
                            let elapsed = now.saturating_sub(threat.inserted_at);
                            elapsed < threat.timer_duration  // Only show non-expired threats
                        })
                        .take(3)
                        .enumerate()
                        .collect();

                    let target_count = active_threats.len();
                    let current_icon_count = threat_icon_query.iter().count();
                    let icons_need_rebuild = current_icon_count != target_count;

                    if icons_need_rebuild {
                        // Icon count changed - despawn all and respawn with correct count
                        for (icon_ent, _, _) in threat_icon_query.iter() {
                            commands.entity(icon_ent).despawn_recursive();
                        }

                        // Spawn new threat icons
                        commands.entity(icons_ent).with_children(|parent| {
                            for (index, threat) in &active_threats {
                                // Calculate timer progress
                                let elapsed = now.saturating_sub(threat.inserted_at);
                                let progress = (elapsed.as_secs_f32() / threat.timer_duration.as_secs_f32()).clamp(0.0, 1.0);
                                let remaining = 1.0 - progress;

                                // Color gradient: Yellow (start) → Orange (50%) → Red (end)
                                let timer_color = if remaining > 0.5 {
                                    // Yellow → Orange transition (100% to 50% remaining)
                                    let t = (remaining - 0.5) / 0.5;
                                    Color::srgba(
                                        1.0,
                                        0.9 * t + 0.5 * (1.0 - t),
                                        0.0,
                                        0.9,
                                    )
                                } else {
                                    // Orange → Red transition (50% to 0% remaining)
                                    let t = remaining / 0.5;
                                    Color::srgba(
                                        1.0,
                                        0.5 * t,
                                        0.0,
                                        0.9,
                                    )
                                };

                                // Size grows from 15% to 100% as timer counts down
                                let size_percent = 15.0 + (85.0 * progress);
                                let offset_percent = (100.0 - size_percent) / 2.0;

                                // Threat icon (circular, 40px)
                                parent.spawn((
                                    Node {
                                        width: Val::Px(40.),
                                        height: Val::Px(40.),
                                        border: UiRect::all(Val::Px(2.)),
                                        justify_content: JustifyContent::Center,
                                        align_items: AlignItems::Center,
                                        ..default()
                                    },
                                    BorderColor(if is_full {
                                        Color::srgb(1.0, 0.2, 0.2)  // Brighter red when full
                                    } else {
                                        Color::srgb(0.8, 0.2, 0.2)  // Normal red
                                    }),
                                    BorderRadius::all(Val::Percent(50.)), // Make circular
                                    BackgroundColor(Color::srgb(0.3, 0.1, 0.1)),
                                    TargetThreatIcon { index: *index },
                                ))
                                .with_children(|parent| {
                                    // Timer ring (grows from center as time runs out)
                                    parent.spawn((
                                        Node {
                                            position_type: PositionType::Absolute,
                                            width: Val::Percent(size_percent),
                                            height: Val::Percent(size_percent),
                                            left: Val::Percent(offset_percent),
                                            top: Val::Percent(offset_percent),
                                            border: UiRect::all(Val::Px(3.)),
                                            ..default()
                                        },
                                        BorderColor(timer_color),
                                        BorderRadius::all(Val::Percent(50.)),
                                        BackgroundColor(Color::NONE),
                                        TargetThreatTimerRing { index: *index },
                                    ));

                                    // Attack type icon (centered)
                                    let icon_text = match threat.damage_type {
                                        DamageType::Physical => "⚔",
                                        DamageType::Magic => "🔥",
                                    };

                                    parent.spawn((
                                        Text::new(icon_text),
                                        TextFont {
                                            font_size: 22.0,
                                            ..default()
                                        },
                                        TextColor(Color::WHITE),
                                        TargetThreatAttackIcon { index: *index },
                                    ));
                                });
                            }
                        });
                    } else {
                        // Icon count unchanged - update existing icons
                        for (index, threat) in &active_threats {
                            // Calculate timer progress
                            let elapsed = now.saturating_sub(threat.inserted_at);
                            let progress = (elapsed.as_secs_f32() / threat.timer_duration.as_secs_f32()).clamp(0.0, 1.0);
                            let remaining = 1.0 - progress;

                            // Color gradient for timer ring
                            let timer_color = if remaining > 0.5 {
                                let t = (remaining - 0.5) / 0.5;
                                Color::srgba(1.0, 0.9 * t + 0.5 * (1.0 - t), 0.0, 0.9)
                            } else {
                                let t = remaining / 0.5;
                                Color::srgba(1.0, 0.5 * t, 0.0, 0.9)
                            };

                            // Update timer ring size and color
                            for (ring, mut node, mut border_color) in timer_ring_query.iter_mut() {
                                if ring.index == *index {
                                    let size_percent = 15.0 + (85.0 * progress);
                                    let offset_percent = (100.0 - size_percent) / 2.0;
                                    node.width = Val::Percent(size_percent);
                                    node.height = Val::Percent(size_percent);
                                    node.left = Val::Percent(offset_percent);
                                    node.top = Val::Percent(offset_percent);
                                    border_color.0 = timer_color;
                                }
                            }

                            // Update attack icon text if damage type changed
                            for (attack_icon, mut text) in attack_icon_query.iter_mut() {
                                if attack_icon.index == *index {
                                    let icon_text = match threat.damage_type {
                                        DamageType::Physical => "⚔",
                                        DamageType::Magic => "🔥",
                                    };
                                    **text = icon_text.to_string();
                                }
                            }

                            // Update threat icon border color based on queue fullness
                            for (_icon_ent, icon, mut border_color) in threat_icon_query.iter_mut() {
                                if icon.index == *index {
                                    border_color.0 = if is_full {
                                        Color::srgb(1.0, 0.2, 0.2)
                                    } else {
                                        Color::srgb(0.8, 0.2, 0.2)
                                    };
                                }
                            }
                        }
                    }
                }
            }
        } else {
            // Target doesn't have a queue - hide it
            *queue_visibility = Visibility::Hidden;
        }
    }
}

/// Update ally frame to show current ally target's information
/// Uses sticky targeting - ally target persists until a new ally is selected or current dies/despawns
/// Mirrors hostile frame logic but for friendly player entities
pub fn update_ally_frame(
    mut frame_query: Query<&mut Visibility, With<AllyFrame>>,
    mut name_text_query: Query<&mut Text, (With<AllyNameText>, Without<AllyHealthText>, Without<AllyTriumvirateText>)>,
    mut name_color_query: Query<&mut TextColor, With<AllyNameText>>,
    mut triumvirate_query: Query<(&mut Text, &mut TextColor), (With<AllyTriumvirateText>, Without<AllyNameText>, Without<AllyHealthText>)>,
    mut health_bar_query: Query<&mut Node, With<AllyHealthBar>>,
    mut health_text_query: Query<&mut Text, (With<AllyHealthText>, Without<AllyNameText>, Without<AllyTriumvirateText>)>,
    player_query: Query<(&crate::common::components::ally_target::AllyTarget, &Health), With<Actor>>,
    ally_query: Query<(&EntityType, &Health)>,
) {
    // Get local player's ally target and health
    let Ok((ally_target, player_health)) = player_query.get_single() else {
        return;
    };

    // Don't show ally frame while dead
    if player_health.state <= 0.0 {
        for mut visibility in &mut frame_query {
            *visibility = Visibility::Hidden;
        }
        return;
    }

    // Read from last_target (sticky behavior - shows last ally even when not currently facing)
    let target_entity = ally_target.last_target;

    // Validate ally target is still alive and exists
    let valid_ally = if let Some(ally_ent) = target_entity {
        if let Ok((_, ally_health)) = ally_query.get(ally_ent) {
            // Only show if alive
            ally_health.state > 0.0
        } else {
            // Ally entity no longer exists
            false
        }
    } else {
        false
    };

    // Show/hide frame and update content based on ally target validity
    if valid_ally {
        let ally_ent = target_entity.unwrap();
        // Ally exists - show frame and update content
        for mut visibility in &mut frame_query {
            *visibility = Visibility::Visible;
        }

        if let Ok((entity_type, ally_health)) = ally_query.get(ally_ent) {
            // Update entity name
            for mut text in &mut name_text_query {
                **text = entity_type.display_name().to_string();
            }

            // Update triumvirate display (only for actors)
            if let EntityType::Actor(actor_impl) = entity_type {
                // Set name color based on origin (slightly lighter green tint)
                for mut color in &mut name_color_query {
                    let (r, g, b) = actor_impl.origin.color();
                    color.0 = Color::srgb(
                        (r * 0.7 + 0.3).min(1.0),  // Blend toward light green
                        (g * 0.7 + 0.3).min(1.0),
                        (b * 0.7 + 0.3).min(1.0),
                    );
                }

                // Set triumvirate text (Approach / Resilience)
                for (mut text, mut color) in &mut triumvirate_query {
                    **text = format!("{} / {}",
                        actor_impl.approach.display_name(),
                        actor_impl.resilience.display_name()
                    );
                    // Triumvirate text also colored by origin (dimmer)
                    let (r, g, b) = actor_impl.origin.color();
                    color.0 = Color::srgb(r * 0.8, g * 0.8, b * 0.8);
                }
            }

            // Update health bar width
            for mut node in &mut health_bar_query {
                let percent = if ally_health.max > 0.0 {
                    (ally_health.state / ally_health.max * 100.0).clamp(0.0, 100.0)
                } else {
                    0.0
                };
                node.width = Val::Percent(percent);
            }

            // Update health text (exact numbers)
            for mut text in &mut health_text_query {
                **text = format!("{:.0}/{:.0}", ally_health.state, ally_health.max);
            }
        }
    } else {
        // No ally target - hide frame
        for mut visibility in &mut frame_query {
            *visibility = Visibility::Hidden;
        }
    }
}

/// Update ally frame queue display
/// Separate system to avoid hitting Bevy's system parameter limits
/// Mirrors update_queue but for ally frame
pub fn update_ally_queue(
    mut commands: Commands,
    player_query: Query<&crate::common::components::ally_target::AllyTarget, With<Actor>>,
    mut queue_container_query: Query<&mut Visibility, With<AllyQueueContainer>>,
    queue_children_query: Query<&Children, With<AllyQueueContainer>>,
    dots_container_query: Query<Entity, With<AllyDotsContainer>>,
    threat_icon_query: Query<Entity, With<AllyThreatIcon>>,
    capacity_dot_query: Query<Entity, With<AllyCapacityDot>>,
    ally_query: Query<Option<&ReactionQueue>>,
    time: Res<Time>,
    server: Res<Server>,
) {
    // Get local player's ally target
    let Ok(ally_target) = player_query.get_single() else {
        return;
    };

    // Check if we have an ally target (use last_target for sticky behavior)
    let Some(ally_ent) = ally_target.last_target else {
        // No ally - hide queue
        for mut visibility in &mut queue_container_query {
            *visibility = Visibility::Hidden;
        }
        return;
    };

    // Get ally's reaction queue
    let Ok(queue_opt) = ally_query.get(ally_ent) else {
        return;
    };

    // Update queue container visibility and content
    if let Ok(mut queue_visibility) = queue_container_query.get_single_mut() {
        if let Some(queue) = queue_opt {
            // Ally has a queue - show it
            *queue_visibility = Visibility::Visible;

            // Despawn old capacity dots and threat icons
            for dot_ent in &capacity_dot_query {
                commands.entity(dot_ent).despawn_recursive();
            }
            for icon_ent in &threat_icon_query {
                commands.entity(icon_ent).despawn_recursive();
            }

            // Get actual queue capacity from the component
            let queue_capacity = queue.capacity;
            let filled_slots = queue.threats.len();
            let is_full = queue.is_full();

            // Spawn capacity dots in the dots container
            if let Ok(dots_container_ent) = dots_container_query.get_single() {
                commands.entity(dots_container_ent).with_children(|parent| {
                    for i in 0..queue_capacity {
                        let is_filled = i < filled_slots;

                        // Use circular UI nodes instead of text characters
                        // Use green theme for ally (instead of red)
                        let (bg_color, border_color) = if is_full && is_filled {
                            // Full queue: filled dots are bright green with green border
                            (Color::srgb(0.3, 1.0, 0.3), Color::srgb(0.3, 1.0, 0.3))
                        } else if is_filled {
                            // Filled but not full: yellow-green fill with border
                            (Color::srgb(0.6, 0.9, 0.4), Color::srgb(0.6, 0.9, 0.4))
                        } else {
                            // Empty: transparent with gray border
                            (Color::NONE, Color::srgb(0.5, 0.5, 0.5))
                        };

                        parent.spawn((
                            Node {
                                width: Val::Px(8.),
                                height: Val::Px(8.),
                                border: UiRect::all(Val::Px(1.)),
                                ..default()
                            },
                            BorderColor(border_color),
                            BorderRadius::all(Val::Percent(50.)), // Make circular
                            BackgroundColor(bg_color),
                            AllyCapacityDot { index: i },
                        ));
                    }
                });
            }

            // Spawn threat icons (LIMIT TO FIRST 3)
            if let Ok(queue_children) = queue_children_query.get_single() {
                let threat_icons_container = queue_children.get(1).copied();

                if let Some(icons_ent) = threat_icons_container {
                    let now_ms = server.current_time(time.elapsed().as_millis());
                    let now = std::time::Duration::from_millis(now_ms.min(u64::MAX as u128) as u64);

                    commands.entity(icons_ent).with_children(|parent| {
                        // Filter out expired threats and limit to first 3
                        let active_threats: Vec<_> = queue.threats.iter()
                            .filter(|threat| {
                                let elapsed = now.saturating_sub(threat.inserted_at);
                                elapsed < threat.timer_duration  // Only show non-expired threats
                            })
                            .take(3)
                            .enumerate()
                            .collect();

                        for (index, threat) in active_threats {
                            // Calculate timer progress
                            let elapsed = now.saturating_sub(threat.inserted_at);
                            let progress = (elapsed.as_secs_f32() / threat.timer_duration.as_secs_f32()).clamp(0.0, 1.0);
                            let remaining = 1.0 - progress;

                            // Color gradient: Yellow (start) → Orange (50%) → Red (end)
                            let timer_color = if remaining > 0.5 {
                                // Yellow → Orange transition (100% to 50% remaining)
                                let t = (remaining - 0.5) / 0.5;
                                Color::srgba(
                                    1.0,
                                    0.9 * t + 0.5 * (1.0 - t),
                                    0.0,
                                    0.9,
                                )
                            } else {
                                // Orange → Red transition (50% to 0% remaining)
                                let t = remaining / 0.5;
                                Color::srgba(
                                    1.0,
                                    0.5 * t,
                                    0.0,
                                    0.9,
                                )
                            };

                            // Size grows from 15% to 100% as timer counts down
                            let size_percent = 15.0 + (85.0 * progress);
                            let offset_percent = (100.0 - size_percent) / 2.0;

                            // Threat icon (circular, 40px) - use green border for ally
                            parent.spawn((
                                Node {
                                    width: Val::Px(40.),
                                    height: Val::Px(40.),
                                    border: UiRect::all(Val::Px(2.)),
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    ..default()
                                },
                                BorderColor(if is_full {
                                    Color::srgb(0.2, 1.0, 0.2)  // Bright green when full
                                } else {
                                    Color::srgb(0.2, 0.8, 0.2)  // Normal green
                                }),
                                BorderRadius::all(Val::Percent(50.)), // Make circular
                                BackgroundColor(Color::srgb(0.1, 0.3, 0.1)),
                                AllyThreatIcon { index },
                            ))
                            .with_children(|parent| {
                                // Timer ring (grows from center as time runs out)
                                parent.spawn((
                                    Node {
                                        position_type: PositionType::Absolute,
                                        width: Val::Percent(size_percent),
                                        height: Val::Percent(size_percent),
                                        left: Val::Percent(offset_percent),
                                        top: Val::Percent(offset_percent),
                                        border: UiRect::all(Val::Px(3.)),
                                        ..default()
                                    },
                                    BorderColor(timer_color),
                                    BorderRadius::all(Val::Percent(50.)),
                                    BackgroundColor(Color::NONE),
                                ));

                                // Attack type icon (centered)
                                let icon_text = match threat.damage_type {
                                    DamageType::Physical => "⚔",
                                    DamageType::Magic => "🔥",
                                };

                                parent.spawn((
                                    Text::new(icon_text),
                                    TextFont {
                                        font_size: 22.0,
                                        ..default()
                                    },
                                    TextColor(Color::WHITE),
                                ));
                            });
                        }
                    });
                }
            }
        } else {
            // Ally doesn't have a queue - hide it
            *queue_visibility = Visibility::Hidden;
        }
    }
}
