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

/// Helper function to determine if a health bar should be shown
/// ADR-008 Phase 6 visibility rules:
/// - Players (Controlled): Always show
/// - NPCs: Show when in combat OR damaged
fn should_show_health_bar(
    health: &crate::common::components::resources::Health,
    combat_state: Option<&crate::common::components::resources::CombatState>,
    behaviour: Option<&crate::common::components::behaviour::Behaviour>,
) -> bool {
    // Always show for players (Controlled behaviour)
    if let Some(crate::common::components::behaviour::Behaviour::Controlled) = behaviour {
        return true;
    }

    // For NPCs: show if in combat OR damaged
    let is_in_combat = combat_state.map(|c| c.in_combat).unwrap_or(false);
    let is_damaged = health.state < health.max || health.step < health.max;

    is_in_combat || is_damaged
}

/// System to spawn health bars for entities that need them
/// Health bars are shown for players (always) and NPCs (in combat or damaged)
pub fn spawn_health_bars(
    mut commands: Commands,
    query: Query<
        (
            Entity,
            &crate::common::components::resources::Health,
            Option<&crate::common::components::resources::CombatState>,
            Option<&crate::common::components::behaviour::Behaviour>,
        ),
        Changed<crate::common::components::resources::Health>,
    >,
    existing_bars: Query<&crate::client::components::HealthBar>,
) {
    // Build HashSet of entities that already have health bars (O(n) once)
    let tracked_entities: std::collections::HashSet<_> = existing_bars
        .iter()
        .map(|bar| bar.tracked_entity)
        .collect();

    // Only iterate entities whose Health changed this frame (reactive, not polling)
    for (entity, health, combat_state, behaviour) in &query {
        // O(1) lookup instead of O(n) iteration
        if tracked_entities.contains(&entity) {
            continue;
        }

        // Check if health bar should be shown
        if should_show_health_bar(health, combat_state, behaviour) {
            // Calculate initial fill ratio
            let health_ratio = (health.step / health.max).clamp(0.0, 1.0);

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
                    current_fill: health_ratio, // Initialize with current health ratio
                },
            )).id();

            // Create background bar (dark gray, full width)
            let background = commands.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(50.0),
                    height: Val::Px(6.0),
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.2, 0.2, 0.2)),
            )).id();

            // Create foreground bar (red per ADR-008, variable width)
            let foreground = commands.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(50.0 * health_ratio),  // Initialize with current health
                    height: Val::Px(6.0),
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.9, 0.1, 0.1)), // Red per ADR-008 spec
                ZIndex(1), // Foreground on top
            )).id();

            // Add children to container
            commands.entity(container).add_children(&[background, foreground]);
        }
    }
}

/// System to spawn health bars when entities enter combat
/// Complements spawn_health_bars by triggering on CombatState changes
pub fn spawn_health_bars_on_combat(
    mut commands: Commands,
    query: Query<
        (
            Entity,
            &crate::common::components::resources::Health,
            &crate::common::components::resources::CombatState,
            Option<&crate::common::components::behaviour::Behaviour>,
        ),
        Changed<crate::common::components::resources::CombatState>,
    >,
    existing_bars: Query<&crate::client::components::HealthBar>,
) {
    // Build HashSet of entities that already have health bars
    let tracked_entities: std::collections::HashSet<_> = existing_bars
        .iter()
        .map(|bar| bar.tracked_entity)
        .collect();

    for (entity, health, combat_state, behaviour) in &query {
        // Skip if already has a health bar
        if tracked_entities.contains(&entity) {
            continue;
        }

        // Check if health bar should be shown
        if should_show_health_bar(health, Some(combat_state), behaviour) {
            // Calculate initial fill ratio
            let health_ratio = (health.step / health.max).clamp(0.0, 1.0);

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
                    current_fill: health_ratio,
                },
            )).id();

            // Create background bar (dark gray, full width)
            let background = commands.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(50.0),
                    height: Val::Px(6.0),
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.2, 0.2, 0.2)),
            )).id();

            // Create foreground bar (red per ADR-008)
            let foreground = commands.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(50.0 * health_ratio),
                    height: Val::Px(6.0),
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.9, 0.1, 0.1)),
                ZIndex(1),
            )).id();

            commands.entity(container).add_children(&[background, foreground]);
        }
    }
}

/// System to update health bar positions and widths
/// Projects world positions to screen space and updates bar width based on health ratio
/// Smoothly interpolates width changes over 0.2s per ADR-008
pub fn update_health_bars(
    mut commands: Commands,
    mut bar_query: Query<(Entity, &mut crate::client::components::HealthBar, &Children, &mut Node)>,
    mut child_node_query: Query<&mut Node, Without<crate::client::components::HealthBar>>,
    entity_query: Query<(
        &crate::common::components::resources::Health,
        Option<&crate::common::components::resources::CombatState>,
        Option<&crate::common::components::behaviour::Behaviour>,
        &Transform,
    )>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    time: Res<Time>,
) {
    let Ok((camera, camera_transform)) = camera_query.get_single() else {
        return;
    };

    const INTERPOLATION_SPEED: f32 = 5.0; // Higher = faster interpolation (0.2s approximate)
    const BAR_WIDTH: f32 = 50.0;

    for (bar_entity, mut health_bar, children, mut container_node) in &mut bar_query {
        // Get the tracked entity's components
        let Ok((health, combat_state, behaviour, transform)) = entity_query.get(health_bar.tracked_entity) else {
            // Entity no longer exists, despawn health bar
            commands.entity(bar_entity).despawn_recursive();
            continue;
        };

        // Check if health bar should still be visible (ADR-008 Phase 6 rules)
        if !should_show_health_bar(health, combat_state, behaviour) {
            // No longer meets visibility criteria, despawn bar
            commands.entity(bar_entity).despawn_recursive();
            continue;
        }

        // Calculate target health ratio
        let target_ratio = (health.step / health.max).clamp(0.0, 1.0);

        // Smoothly interpolate current fill toward target (ADR-008 Phase 8: smooth animation)
        let delta = time.delta_secs();
        health_bar.current_fill = health_bar.current_fill.lerp(target_ratio, INTERPOLATION_SPEED * delta);

        // Calculate world position (above entity)
        let world_pos = transform.translation + Vec3::new(0.0, 1.5, 0.0);

        // Project to screen space
        if let Ok(viewport_pos) = camera.world_to_viewport(camera_transform, world_pos) {
            // Update container position (centered horizontally)
            container_node.left = Val::Px(viewport_pos.x - (BAR_WIDTH / 2.0));
            container_node.top = Val::Px(viewport_pos.y);

            // Update foreground bar width based on interpolated health ratio
            // Children: [0] = background, [1] = foreground
            if children.len() >= 2 {
                let foreground_entity = children[1];
                if let Ok(mut foreground_node) = child_node_query.get_mut(foreground_entity) {
                    foreground_node.width = Val::Px(BAR_WIDTH * health_bar.current_fill);
                }
            }
        } else {
            // Off-screen, hide it
            container_node.left = Val::Px(-10000.0);
        }
    }
}
