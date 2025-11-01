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
        components::{Actor, entity_type::*, heading::*, resources::*, reaction_queue::*, Loc},
        plugins::nntree::NNTree,
        systems::targeting::select_target,
    },
    client::resources::Server,
};

/// Resource tracking the currently locked target for sticky targeting
/// Target persists even when player looks away, until a new target is selected or current target dies/despawns
#[derive(Resource, Default)]
pub struct LockedTarget {
    pub entity: Option<Entity>,
}

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

/// Marker component for the target's triumvirate text (approach/resilience)
#[derive(Component)]
pub struct TargetTriumvirateText;

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
}

/// Update target frame to show current target's information
/// Uses sticky targeting - target persists until a new target is selected or current dies/despawns
pub fn update(
    mut locked_target: ResMut<LockedTarget>,
    mut frame_query: Query<&mut Visibility, With<TargetFrame>>,
    mut name_text_query: Query<&mut Text, (With<TargetNameText>, Without<TargetHealthText>, Without<TargetTriumvirateText>)>,
    mut name_color_query: Query<&mut TextColor, With<TargetNameText>>,
    mut triumvirate_query: Query<(&mut Text, &mut TextColor), (With<TargetTriumvirateText>, Without<TargetNameText>, Without<TargetHealthText>)>,
    mut health_bar_query: Query<&mut Node, With<TargetHealthBar>>,
    mut health_text_query: Query<&mut Text, (With<TargetHealthText>, Without<TargetNameText>, Without<TargetTriumvirateText>)>,
    player_query: Query<(Entity, &Loc, &Heading, &Health), With<Actor>>,
    target_query: Query<(&EntityType, &Loc, &Health, Option<&ReactionQueue>)>,
    nntree: Res<NNTree>,
) {
    // Get local player
    let Ok((player_ent, player_loc, player_heading, player_health)) = player_query.get_single() else {
        return;
    };

    // Don't show target frame while dead
    if player_health.state <= 0.0 {
        for mut visibility in &mut frame_query {
            *visibility = Visibility::Hidden;
        }
        return;
    }

    // Select current hostile target using directional targeting
    let facing_target = select_target(
        player_ent,
        *player_loc,
        *player_heading,
        None, // No tier lock in MVP
        &nntree,
        |ent| target_query.get(ent).ok().map(|(et, _, _, _)| *et),
    );

    // Sticky targeting: Update locked target when a new target is found in facing cone
    if let Some(new_target) = facing_target {
        locked_target.entity = Some(new_target);
    }

    // Validate locked target is still alive and exists
    if let Some(target_ent) = locked_target.entity {
        if let Ok((_, _, target_health, _)) = target_query.get(target_ent) {
            // Clear locked target if dead
            if target_health.state <= 0.0 {
                locked_target.entity = None;
            }
        } else {
            // Target entity no longer exists - clear it
            locked_target.entity = None;
        }
    }

    // Show/hide frame and update content based on locked target
    if let Some(target_ent) = locked_target.entity {
        // Target exists - show frame and update content
        for mut visibility in &mut frame_query {
            *visibility = Visibility::Visible;
        }

        if let Ok((entity_type, target_loc, target_health, queue_opt)) = target_query.get(target_ent) {
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
    locked_target: Res<LockedTarget>,
    mut queue_container_query: Query<&mut Visibility, With<TargetQueueContainer>>,
    queue_children_query: Query<&Children, With<TargetQueueContainer>>,
    dots_container_query: Query<Entity, With<DotsContainer>>,
    threat_icon_query: Query<Entity, With<TargetThreatIcon>>,
    capacity_dot_query: Query<Entity, With<CapacityDot>>,
    target_query: Query<Option<&ReactionQueue>>,
    time: Res<Time>,
    server: Res<Server>,
) {
    // Check if we have a locked target
    let Some(target_ent) = locked_target.entity else {
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
                                TargetThreatIcon { index },
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
            // Target doesn't have a queue - hide it
            *queue_visibility = Visibility::Hidden;
        }
    }
}
