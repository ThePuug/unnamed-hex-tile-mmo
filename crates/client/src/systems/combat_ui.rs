use bevy::prelude::*;

/// System to update floating text (damage numbers)
/// Projects world position to screen space, moves text upward, fades out, and despawns
pub fn update_floating_text(
    mut commands: Commands,
    mut query: Query<(Entity, &mut crate::components::FloatingText, &mut Node, &mut TextColor)>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    time: Res<Time>,
) {
    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };

    for (entity, mut floating_text, mut node, mut text_color) in &mut query {
        let elapsed = (time.elapsed() - floating_text.spawn_time).as_secs_f32();

        // Check if lifetime expired
        if elapsed >= floating_text.lifetime {
            commands.entity(entity).despawn();
            continue;
        }

        // Move upward in world space
        let delta = time.delta_secs();
        floating_text.world_position.y += floating_text.velocity * delta;

        // Project world position to screen space
        if let Ok(viewport_pos) = camera.world_to_viewport(camera_transform, floating_text.world_position) {
            node.left = Val::Px(viewport_pos.x);
            node.top = Val::Px(viewport_pos.y);
        } else {
            // Position is behind camera or off-screen, hide it
            node.left = Val::Px(-1000.0);
        }

        // Fade out (alpha based on remaining lifetime)
        let alpha = 1.0 - (elapsed / floating_text.lifetime);
        text_color.0 = text_color.0.with_alpha(alpha);
    }
}

/// Setup persistent world-space health bars and threat queue dots
/// Creates 2 health bars (hostile, ally) and 2 dots containers with max capacity
/// These are shown/hidden and repositioned based on current targets
pub fn setup_health_bars(mut commands: Commands) {
    const BAR_WIDTH: f32 = 50.0;
    const BAR_HEIGHT: f32 = 6.0;
    const MAX_QUEUE_CAPACITY: usize = 10;

    // Spawn hostile target health bar (hidden by default)
    let hostile_container = commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: Val::Px(BAR_WIDTH),
            height: Val::Px(BAR_HEIGHT),
            left: Val::Px(-10000.0), // Start off-screen
            ..default()
        },
        Visibility::Hidden,
        crate::components::WorldHealthBar {
            current_fill: 1.0,
        },
        crate::components::HostileHealthBar,
    )).id();

    // Background bar (dark gray)
    let hostile_bg = commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: Val::Px(BAR_WIDTH),
            height: Val::Px(BAR_HEIGHT),
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            ..default()
        },
        BackgroundColor(Color::srgb(0.2, 0.2, 0.2)),
    )).id();

    // Foreground bar (red)
    let hostile_fg = commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: Val::Px(BAR_WIDTH),
            height: Val::Px(BAR_HEIGHT),
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            ..default()
        },
        BackgroundColor(Color::srgb(0.9, 0.1, 0.1)),
        ZIndex(1),
    )).id();

    commands.entity(hostile_container).add_children(&[hostile_bg, hostile_fg]);

    // Spawn hostile target recovery bar (hidden by default, stacked under health bar)
    let hostile_recovery_container = commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: Val::Px(BAR_WIDTH),
            height: Val::Px(BAR_HEIGHT),
            left: Val::Px(-10000.0),
            ..default()
        },
        Visibility::Hidden,
        crate::components::WorldRecoveryBar {
            current_fill: 1.0,
        },
        crate::components::HostileRecoveryBar,
    )).id();

    let hostile_recovery_bg = commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: Val::Px(BAR_WIDTH),
            height: Val::Px(BAR_HEIGHT),
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            ..default()
        },
        BackgroundColor(Color::srgb(0.2, 0.2, 0.2)),
    )).id();

    let hostile_recovery_fg = commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: Val::Px(BAR_WIDTH),
            height: Val::Px(BAR_HEIGHT),
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            ..default()
        },
        BackgroundColor(Color::srgb(0.1, 0.9, 0.1)),
        ZIndex(1),
    )).id();

    commands.entity(hostile_recovery_container).add_children(&[hostile_recovery_bg, hostile_recovery_fg]);

    // Spawn ally target health bar (hidden by default)
    let ally_container = commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: Val::Px(BAR_WIDTH),
            height: Val::Px(BAR_HEIGHT),
            left: Val::Px(-10000.0), // Start off-screen
            ..default()
        },
        Visibility::Hidden,
        crate::components::WorldHealthBar {
            current_fill: 1.0,
        },
        crate::components::AllyHealthBar,
    )).id();

    // Background bar (dark gray)
    let ally_bg = commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: Val::Px(BAR_WIDTH),
            height: Val::Px(BAR_HEIGHT),
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            ..default()
        },
        BackgroundColor(Color::srgb(0.2, 0.2, 0.2)),
    )).id();

    // Foreground bar (green for ally)
    let ally_fg = commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: Val::Px(BAR_WIDTH),
            height: Val::Px(BAR_HEIGHT),
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            ..default()
        },
        BackgroundColor(Color::srgb(0.1, 0.9, 0.1)),
        ZIndex(1),
    )).id();

    commands.entity(ally_container).add_children(&[ally_bg, ally_fg]);

    // Spawn ally target recovery bar (hidden by default, stacked under health bar)
    let ally_recovery_container = commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: Val::Px(BAR_WIDTH),
            height: Val::Px(BAR_HEIGHT),
            left: Val::Px(-10000.0),
            ..default()
        },
        Visibility::Hidden,
        crate::components::WorldRecoveryBar {
            current_fill: 1.0,
        },
        crate::components::AllyRecoveryBar,
    )).id();

    let ally_recovery_bg = commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: Val::Px(BAR_WIDTH),
            height: Val::Px(BAR_HEIGHT),
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            ..default()
        },
        BackgroundColor(Color::srgb(0.2, 0.2, 0.2)),
    )).id();

    let ally_recovery_fg = commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: Val::Px(BAR_WIDTH),
            height: Val::Px(BAR_HEIGHT),
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            ..default()
        },
        BackgroundColor(Color::srgb(0.1, 0.9, 0.1)),
        ZIndex(1),
    )).id();

    commands.entity(ally_recovery_container).add_children(&[ally_recovery_bg, ally_recovery_fg]);

    // Spawn hostile target threat queue dots container (hidden by default)
    let hostile_dots_container = commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(3.0),
            left: Val::Px(-10000.0), // Start off-screen
            ..default()
        },
        Visibility::Hidden,
        crate::components::ThreatQueueDots,
        crate::components::HostileQueueDots,
    )).id();

    // Spawn max capacity dots for hostile (all start gray/empty)
    let mut hostile_dot_ids = Vec::new();
    for i in 0..MAX_QUEUE_CAPACITY {
        let dot_id = commands.spawn((
            Node {
                width: Val::Px(8.0),
                height: Val::Px(8.0),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Percent(50.0)),
                ..default()
            },
            BorderColor::all(Color::srgb(0.5, 0.5, 0.5)),
            BackgroundColor(Color::srgb(0.3, 0.3, 0.3)),
            Visibility::Hidden, // Start hidden, will be shown based on capacity
            crate::components::ThreatCapacityDot { index: i },
        )).id();
        hostile_dot_ids.push(dot_id);
    }
    commands.entity(hostile_dots_container).add_children(&hostile_dot_ids);

    // Spawn ally target threat queue dots container (hidden by default)
    let ally_dots_container = commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(3.0),
            left: Val::Px(-10000.0), // Start off-screen
            ..default()
        },
        Visibility::Hidden,
        crate::components::ThreatQueueDots,
        crate::components::AllyQueueDots,
    )).id();

    // Spawn max capacity dots for ally (all start gray/empty)
    let mut ally_dot_ids = Vec::new();
    for i in 0..MAX_QUEUE_CAPACITY {
        let dot_id = commands.spawn((
            Node {
                width: Val::Px(8.0),
                height: Val::Px(8.0),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Percent(50.0)),
                ..default()
            },
            BorderColor::all(Color::srgb(0.5, 0.5, 0.5)),
            BackgroundColor(Color::srgb(0.3, 0.3, 0.3)),
            Visibility::Hidden, // Start hidden, will be shown based on capacity
            crate::components::ThreatCapacityDot { index: i },
        )).id();
        ally_dot_ids.push(dot_id);
    }
    commands.entity(ally_dots_container).add_children(&ally_dot_ids);
}

/// System to update health bar positions and widths
/// Updates 2 persistent health bars (hostile and ally) based on current targets
/// Smoothly interpolates width changes over 0.2s per ADR-008
pub fn update_health_bars(
    mut hostile_query: Query<
        (&mut crate::components::WorldHealthBar, &Children, &mut Node, &mut Visibility),
        (With<crate::components::HostileHealthBar>, Without<crate::components::AllyHealthBar>)
    >,
    mut ally_query: Query<
        (&mut crate::components::WorldHealthBar, &Children, &mut Node, &mut Visibility),
        (With<crate::components::AllyHealthBar>, Without<crate::components::HostileHealthBar>)
    >,
    mut child_node_query: Query<&mut Node, (Without<crate::components::WorldHealthBar>, Without<crate::components::HostileHealthBar>, Without<crate::components::AllyHealthBar>)>,
    entity_query: Query<(&common::components::resources::Health, &Transform)>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    player_query: Query<(&common::components::target::Target, &common::components::ally_target::AllyTarget), With<common::components::Actor>>,
    time: Res<Time>,
) {
    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };

    // Get local player's targets (set by update_targets_on_change and update_ally_targets_on_change with tier lock filtering)
    let Ok((player_target, player_ally_target)) = player_query.single() else {
        return;
    };

    // Read hostile target from Target component (reactively maintained with tier lock support)
    // Don't call select_target here - that would ignore tier lock!
    let hostile_target = player_target.entity;

    // Read ally target from AllyTarget component (reactively maintained by update_ally_targets_on_change)
    // Use entity (current) not last_target (sticky) for combat UI bars
    let ally_target = player_ally_target.entity;

    const INTERPOLATION_SPEED: f32 = 5.0;
    const BAR_WIDTH: f32 = 50.0;
    let delta = time.delta_secs();

    // Update hostile health bar
    if let Ok((mut health_bar, children, mut container_node, mut visibility)) = hostile_query.single_mut() {
        if let Some(target_ent) = hostile_target {
            // Target exists - show and update bar
            if let Ok((health, transform)) = entity_query.get(target_ent) {
                *visibility = Visibility::Visible;

                // Calculate target health ratio
                let target_ratio = (health.step / health.max).clamp(0.0, 1.0);

                // Smoothly interpolate current fill toward target
                health_bar.current_fill = health_bar.current_fill.lerp(target_ratio, INTERPOLATION_SPEED * delta);

                // Calculate world position (above entity)
                let world_pos = transform.translation + Vec3::new(0.0, 1.5, 0.0);

                // Project to screen space
                if let Ok(viewport_pos) = camera.world_to_viewport(camera_transform, world_pos) {
                    // Update container position (centered horizontally)
                    container_node.left = Val::Px(viewport_pos.x - (BAR_WIDTH / 2.0));
                    container_node.top = Val::Px(viewport_pos.y);

                    // Update foreground bar width (children[1])
                    if children.len() >= 2 {
                        if let Ok(mut foreground_node) = child_node_query.get_mut(children[1]) {
                            foreground_node.width = Val::Px(BAR_WIDTH * health_bar.current_fill);
                        }
                    }
                } else {
                    // Off-screen, hide it
                    container_node.left = Val::Px(-10000.0);
                }
            } else {
                // Target entity doesn't exist, hide bar
                *visibility = Visibility::Hidden;
            }
        } else {
            // No target, hide bar
            *visibility = Visibility::Hidden;
        }
    }

    // Update ally health bar
    if let Ok((mut health_bar, children, mut container_node, mut visibility)) = ally_query.single_mut() {
        if let Some(target_ent) = ally_target {
            // Target exists - show and update bar
            if let Ok((health, transform)) = entity_query.get(target_ent) {
                *visibility = Visibility::Visible;

                // Calculate target health ratio
                let target_ratio = (health.step / health.max).clamp(0.0, 1.0);

                // Smoothly interpolate current fill toward target
                health_bar.current_fill = health_bar.current_fill.lerp(target_ratio, INTERPOLATION_SPEED * delta);

                // Calculate world position (above entity)
                let world_pos = transform.translation + Vec3::new(0.0, 1.5, 0.0);

                // Project to screen space
                if let Ok(viewport_pos) = camera.world_to_viewport(camera_transform, world_pos) {
                    // Update container position (centered horizontally)
                    container_node.left = Val::Px(viewport_pos.x - (BAR_WIDTH / 2.0));
                    container_node.top = Val::Px(viewport_pos.y);

                    // Update foreground bar width (children[1])
                    if children.len() >= 2 {
                        if let Ok(mut foreground_node) = child_node_query.get_mut(children[1]) {
                            foreground_node.width = Val::Px(BAR_WIDTH * health_bar.current_fill);
                        }
                    }
                } else {
                    // Off-screen, hide it
                    container_node.left = Val::Px(-10000.0);
                }
            } else {
                // Target entity doesn't exist, hide bar
                *visibility = Visibility::Hidden;
            }
        } else {
            // No target, hide bar
            *visibility = Visibility::Hidden;
        }
    }
}

/// System to update threat queue dots above health bars
/// Updates 2 persistent dots containers (hostile and ally) based on current targets
/// Shows/hides individual dots based on target's queue capacity
pub fn update_threat_queue_dots(
    mut hostile_query: Query<
        (&Children, &mut Node, &mut Visibility),
        (With<crate::components::HostileQueueDots>, Without<crate::components::AllyQueueDots>)
    >,
    mut ally_query: Query<
        (&Children, &mut Node, &mut Visibility),
        (With<crate::components::AllyQueueDots>, Without<crate::components::HostileQueueDots>)
    >,
    mut dot_query: Query<
        (&crate::components::ThreatCapacityDot, &mut Visibility, &mut BackgroundColor, &mut BorderColor),
        (Without<crate::components::HostileQueueDots>, Without<crate::components::AllyQueueDots>)
    >,
    queue_query: Query<(Option<&common::components::reaction_queue::ReactionQueue>, &Transform)>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    player_query: Query<(&common::components::target::Target, &common::components::ally_target::AllyTarget), With<common::components::Actor>>,
) {
    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };

    // Get local player's targets (set by update_targets_on_change and update_ally_targets_on_change with tier lock filtering)
    let Ok((player_target, player_ally_target)) = player_query.single() else {
        return;
    };

    // Read hostile target from Target component (reactively maintained with tier lock support)
    // Don't call select_target here - that would ignore tier lock!
    let hostile_target = player_target.entity;

    // Read ally target from AllyTarget component (reactively maintained by update_ally_targets_on_change)
    // Use entity (current) not last_target (sticky) for combat UI dots
    let ally_target = player_ally_target.entity;

    const BAR_WIDTH: f32 = 50.0;

    // Update hostile dots container
    if let Ok((children, mut container_node, mut visibility)) = hostile_query.single_mut() {
        if let Some(target_ent) = hostile_target {
            // Target exists - check if it has a queue
            if let Ok((queue_opt, transform)) = queue_query.get(target_ent) {
                if let Some(queue) = queue_opt {
                    if queue.window_size > 0 {
                        // Show container
                        *visibility = Visibility::Visible;

                        // Calculate world position (above entity, above health bar)
                        let world_pos = transform.translation + Vec3::new(0.0, 2.1, 0.0);

                        // Project to screen space
                        if let Ok(viewport_pos) = camera.world_to_viewport(camera_transform, world_pos) {
                            // Left-align with health bar edge
                            container_node.left = Val::Px(viewport_pos.x - (BAR_WIDTH / 2.0));
                            container_node.top = Val::Px(viewport_pos.y);

                            // Update dots visibility and colors
                            let filled_slots = queue.threats.len();
                            let is_full = queue.threats.len().saturating_sub(queue.window_size) > 0;

                            for child in children.iter() {
                                if let Ok((dot, mut dot_vis, mut dot_bg, mut dot_border)) = dot_query.get_mut(child) {
                                    // Show dots up to capacity, hide the rest
                                    if dot.index < queue.window_size {
                                        *dot_vis = Visibility::Visible;

                                        // Update colors based on filled state
                                        let is_filled = dot.index < filled_slots;
                                        let (bg_color, border_color) = if is_full && is_filled {
                                            // Full queue: bright red
                                            (Color::srgb(1.0, 0.2, 0.2), Color::srgb(1.0, 0.2, 0.2))
                                        } else if is_filled {
                                            // Filled but not full: yellow-orange
                                            (Color::srgb(1.0, 0.7, 0.2), Color::srgb(1.0, 0.7, 0.2))
                                        } else {
                                            // Empty: filled gray with gray border
                                            (Color::srgb(0.3, 0.3, 0.3), Color::srgb(0.5, 0.5, 0.5))
                                        };

                                        *dot_bg = BackgroundColor(bg_color);
                                        *dot_border = BorderColor::all(border_color);
                                    } else {
                                        *dot_vis = Visibility::Hidden;
                                    }
                                }
                            }
                        } else {
                            // Off-screen, hide container
                            container_node.left = Val::Px(-10000.0);
                        }
                    } else {
                        // No capacity, hide container and all dots
                        *visibility = Visibility::Hidden;
                        for child in children.iter() {
                            if let Ok((_, mut dot_vis, _, _)) = dot_query.get_mut(child) {
                                *dot_vis = Visibility::Hidden;
                            }
                        }
                    }
                } else {
                    // No queue, hide container and all dots
                    *visibility = Visibility::Hidden;
                    for child in children.iter() {
                        if let Ok((_, mut dot_vis, _, _)) = dot_query.get_mut(child) {
                            *dot_vis = Visibility::Hidden;
                        }
                    }
                }
            } else {
                // Target doesn't exist or has no transform, hide container and all dots
                *visibility = Visibility::Hidden;
                for child in children.iter() {
                    if let Ok((_, mut dot_vis, _, _)) = dot_query.get_mut(child) {
                        *dot_vis = Visibility::Hidden;
                    }
                }
            }
        } else {
            // No target, hide container and all dots
            *visibility = Visibility::Hidden;
            for child in children.iter() {
                if let Ok((_, mut dot_vis, _, _)) = dot_query.get_mut(child) {
                    *dot_vis = Visibility::Hidden;
                }
            }
        }
    }

    // Update ally dots container (same logic as hostile)
    if let Ok((children, mut container_node, mut visibility)) = ally_query.single_mut() {
        if let Some(target_ent) = ally_target {
            // Target exists - check if it has a queue
            if let Ok((queue_opt, transform)) = queue_query.get(target_ent) {
                if let Some(queue) = queue_opt {
                    if queue.window_size > 0 {
                        // Show container
                        *visibility = Visibility::Visible;

                        // Calculate world position (above entity, above health bar)
                        let world_pos = transform.translation + Vec3::new(0.0, 2.1, 0.0);

                        // Project to screen space
                        if let Ok(viewport_pos) = camera.world_to_viewport(camera_transform, world_pos) {
                            // Left-align with health bar edge
                            container_node.left = Val::Px(viewport_pos.x - (BAR_WIDTH / 2.0));
                            container_node.top = Val::Px(viewport_pos.y);

                            // Update dots visibility and colors
                            let filled_slots = queue.threats.len();
                            let is_full = queue.threats.len().saturating_sub(queue.window_size) > 0;

                            for child in children.iter() {
                                if let Ok((dot, mut dot_vis, mut dot_bg, mut dot_border)) = dot_query.get_mut(child) {
                                    // Show dots up to capacity, hide the rest
                                    if dot.index < queue.window_size {
                                        *dot_vis = Visibility::Visible;

                                        // Update colors based on filled state
                                        let is_filled = dot.index < filled_slots;
                                        let (bg_color, border_color) = if is_full && is_filled {
                                            // Full queue: bright red
                                            (Color::srgb(1.0, 0.2, 0.2), Color::srgb(1.0, 0.2, 0.2))
                                        } else if is_filled {
                                            // Filled but not full: yellow-orange
                                            (Color::srgb(1.0, 0.7, 0.2), Color::srgb(1.0, 0.7, 0.2))
                                        } else {
                                            // Empty: filled gray with gray border
                                            (Color::srgb(0.3, 0.3, 0.3), Color::srgb(0.5, 0.5, 0.5))
                                        };

                                        *dot_bg = BackgroundColor(bg_color);
                                        *dot_border = BorderColor::all(border_color);
                                    } else {
                                        *dot_vis = Visibility::Hidden;
                                    }
                                }
                            }
                        } else {
                            // Off-screen, hide container
                            container_node.left = Val::Px(-10000.0);
                        }
                    } else {
                        // No capacity, hide container and all dots
                        *visibility = Visibility::Hidden;
                        for child in children.iter() {
                            if let Ok((_, mut dot_vis, _, _)) = dot_query.get_mut(child) {
                                *dot_vis = Visibility::Hidden;
                            }
                        }
                    }
                } else {
                    // No queue, hide container and all dots
                    *visibility = Visibility::Hidden;
                    for child in children.iter() {
                        if let Ok((_, mut dot_vis, _, _)) = dot_query.get_mut(child) {
                            *dot_vis = Visibility::Hidden;
                        }
                    }
                }
            } else {
                // Target doesn't exist or has no transform, hide container and all dots
                *visibility = Visibility::Hidden;
                for child in children.iter() {
                    if let Ok((_, mut dot_vis, _, _)) = dot_query.get_mut(child) {
                        *dot_vis = Visibility::Hidden;
                    }
                }
            }
        } else {
            // No target, hide container and all dots
            *visibility = Visibility::Hidden;
            for child in children.iter() {
                if let Ok((_, mut dot_vis, _, _)) = dot_query.get_mut(child) {
                    *dot_vis = Visibility::Hidden;
                }
            }
        }
    }
}

/// System to update recovery bar positions and fill
/// Positioned flush beneath the health bar for each target
pub fn update_recovery_bars(
    mut hostile_query: Query<
        (&mut crate::components::WorldRecoveryBar, &Children, &mut Node, &mut Visibility),
        (With<crate::components::HostileRecoveryBar>, Without<crate::components::AllyRecoveryBar>)
    >,
    mut ally_query: Query<
        (&mut crate::components::WorldRecoveryBar, &Children, &mut Node, &mut Visibility),
        (With<crate::components::AllyRecoveryBar>, Without<crate::components::HostileRecoveryBar>)
    >,
    mut child_node_query: Query<&mut Node, (
        Without<crate::components::WorldRecoveryBar>,
        Without<crate::components::HostileRecoveryBar>,
        Without<crate::components::AllyRecoveryBar>,
        Without<crate::components::WorldHealthBar>,
    )>,
    entity_query: Query<(Option<&common::components::recovery::GlobalRecovery>, &Transform)>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    player_query: Query<(&common::components::target::Target, &common::components::ally_target::AllyTarget), With<common::components::Actor>>,
    time: Res<Time>,
) {
    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };

    let Ok((player_target, player_ally_target)) = player_query.single() else {
        return;
    };

    let hostile_target = player_target.entity;
    let ally_target = player_ally_target.entity;

    const INTERPOLATION_SPEED: f32 = 5.0;
    const BAR_WIDTH: f32 = 50.0;
    const BAR_HEIGHT: f32 = 6.0;
    let delta = time.delta_secs();

    // Helper: calculate fill and update a recovery bar
    fn update_bar(
        target: Option<Entity>,
        bar: &mut crate::components::WorldRecoveryBar,
        children: &Children,
        container_node: &mut Node,
        visibility: &mut Visibility,
        child_node_query: &mut Query<&mut Node, (
            Without<crate::components::WorldRecoveryBar>,
            Without<crate::components::HostileRecoveryBar>,
            Without<crate::components::AllyRecoveryBar>,
            Without<crate::components::WorldHealthBar>,
        )>,
        entity_query: &Query<(Option<&common::components::recovery::GlobalRecovery>, &Transform)>,
        camera: &Camera,
        camera_transform: &GlobalTransform,
        delta: f32,
    ) {
        let Some(target_ent) = target else {
            *visibility = Visibility::Hidden;
            return;
        };

        let Ok((recovery_opt, transform)) = entity_query.get(target_ent) else {
            *visibility = Visibility::Hidden;
            return;
        };

        // Calculate target fill ratio
        let target_ratio = match recovery_opt {
            Some(recovery) if recovery.is_active() && recovery.duration > 0.0 => {
                1.0 - (recovery.remaining / recovery.duration)
            }
            _ => 1.0, // No recovery or expired: full bar (entity can act)
        };

        *visibility = Visibility::Visible;

        // Smoothly interpolate current fill toward target
        bar.current_fill = bar.current_fill.lerp(target_ratio, INTERPOLATION_SPEED * delta);

        // Position flush under health bar: same X, offset Y by BAR_HEIGHT
        let world_pos = transform.translation + Vec3::new(0.0, 1.5, 0.0);

        if let Ok(viewport_pos) = camera.world_to_viewport(camera_transform, world_pos) {
            container_node.left = Val::Px(viewport_pos.x - (BAR_WIDTH / 2.0));
            container_node.top = Val::Px(viewport_pos.y + BAR_HEIGHT);

            // Update foreground bar width (children[1])
            if children.len() >= 2 {
                if let Ok(mut foreground_node) = child_node_query.get_mut(children[1]) {
                    foreground_node.width = Val::Px(BAR_WIDTH * bar.current_fill);
                }
            }
        } else {
            container_node.left = Val::Px(-10000.0);
        }
    }

    // Update hostile recovery bar
    if let Ok((mut bar, children, mut container_node, mut visibility)) = hostile_query.single_mut() {
        update_bar(
            hostile_target, &mut bar, &children, &mut container_node, &mut visibility,
            &mut child_node_query, &entity_query, camera, camera_transform, delta,
        );
    }

    // Update ally recovery bar
    if let Ok((mut bar, children, mut container_node, mut visibility)) = ally_query.single_mut() {
        update_bar(
            ally_target, &mut bar, &children, &mut container_node, &mut visibility,
            &mut child_node_query, &entity_query, camera, camera_transform, delta,
        );
    }
}
