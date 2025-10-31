use bevy::prelude::*;

/// System to update floating text (damage numbers)
/// Projects world position to screen space, moves text upward, fades out, and despawns
pub fn update_floating_text(
    mut commands: Commands,
    mut query: Query<(Entity, &mut crate::client::components::FloatingText, &mut Node, &mut TextColor)>,
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

/// System to spawn health bars for entities that need them
/// Health bars are shown for entities in combat or with HP < max
pub fn spawn_health_bars(
    mut commands: Commands,
    query: Query<(Entity, &crate::common::components::resources::Health)>,
    existing_bars: Query<&crate::client::components::HealthBar>,
) {
    for (entity, health) in &query {
        // Check if this entity already has a health bar
        let has_bar = existing_bars.iter().any(|bar| bar.tracked_entity == entity);
        if has_bar {
            continue;
        }

        // Spawn health bar if entity is damaged (check both state and step for client prediction)
        let is_damaged = health.state < health.max || health.step < health.max;

        if is_damaged {

            // Create container with relative positioning for children
            let container = commands.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(50.0),
                    height: Val::Px(6.0),
                    ..default()
                },
                crate::client::components::HealthBar {
                    tracked_entity: entity,
                },
            )).id();

            // Create background bar (grey, full width)
            let background = commands.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(50.0),
                    height: Val::Px(6.0),
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.3, 0.3, 0.3)),
            )).id();

            // Create foreground bar (green, variable width)
            let foreground = commands.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(50.0),  // Will be updated based on health ratio
                    height: Val::Px(6.0),
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.2, 0.8, 0.2)),
                ZIndex(1), // Foreground on top
            )).id();

            // Add children to container
            commands.entity(container).add_children(&[background, foreground]);
        }
    }
}

/// System to update health bar positions and widths
/// Projects world positions to screen space and updates bar width based on health ratio
pub fn update_health_bars(
    mut commands: Commands,
    mut bar_query: Query<(Entity, &crate::client::components::HealthBar, &Children, &mut Node)>,
    mut child_node_query: Query<&mut Node, Without<crate::client::components::HealthBar>>,
    health_query: Query<&crate::common::components::resources::Health>,
    transform_query: Query<&Transform>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
) {
    let Ok((camera, camera_transform)) = camera_query.get_single() else {
        return;
    };

    for (bar_entity, health_bar, children, mut container_node) in &mut bar_query {
        // Get the tracked entity's health and transform
        let Ok(health) = health_query.get(health_bar.tracked_entity) else {
            // Entity no longer exists, despawn health bar
            commands.entity(bar_entity).despawn();
            continue;
        };

        let Ok(transform) = transform_query.get(health_bar.tracked_entity) else {
            continue;
        };

        // Check if entity should still show health bar (check both state and step)
        let is_full_health = health.state >= health.max && health.step >= health.max;
        if is_full_health {
            // Full health, despawn bar
            commands.entity(bar_entity).despawn();
            continue;
        }

        // Calculate world position (above entity)
        let world_pos = transform.translation + Vec3::new(0.0, 1.5, 0.0);

        // Project to screen space
        if let Ok(viewport_pos) = camera.world_to_viewport(camera_transform, world_pos) {
            // Update container position (centered horizontally)
            container_node.left = Val::Px(viewport_pos.x - 25.0);
            container_node.top = Val::Px(viewport_pos.y);

            // Update foreground bar width based on health ratio
            // Children: [0] = background, [1] = foreground
            if children.len() >= 2 {
                let foreground_entity = children[1];
                if let Ok(mut foreground_node) = child_node_query.get_mut(foreground_entity) {
                    let health_ratio = (health.step / health.max).clamp(0.0, 1.0);
                    foreground_node.width = Val::Px(50.0 * health_ratio);
                }
            }
        } else {
            // Off-screen, hide it
            container_node.left = Val::Px(-10000.0);
        }
    }
}
