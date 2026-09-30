use bevy::prelude::*;
use common_bevy::components::{ally_target::AllyTarget, target::Target};

use crate::{
    components::{Measure, ThreatCapacityDot, ThreatQueueDots, WorldBar, WorldBarFill},
    systems::{closeup::CloseupCamera, target_frame::{Lane, LANES}},
};

/// System to update floating text (damage numbers)
/// Projects world position to screen space, moves text upward, fades out, and despawns
pub fn update_floating_text(
    mut commands: Commands,
    mut query: Query<(Entity, &mut crate::components::FloatingText, &mut Node, &mut TextColor)>,
    camera_query: Query<(&Camera, &GlobalTransform), (With<Camera3d>, Without<CloseupCamera>)>,
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

/// Width and height of a bar over a target, in pixels
const BAR_WIDTH: f32 = 50.0;
const BAR_HEIGHT: f32 = 6.0;

/// How fast a bar's fill eases toward what it measures
const FILL_SPEED: f32 = 5.0;

/// The most capacity dots drawn over a target
const MAX_QUEUE_CAPACITY: usize = 10;

/// Where a node sits while what it follows is off screen
const OFF_SCREEN: f32 = -10000.0;

/// Builds, for each lane, the bars and capacity dots drawn over its target
/// in the world: health, recovery flush under it, and the dots above.
/// Hidden until the lane has a target, and moved onto it each frame.
pub fn setup_health_bars(mut commands: Commands) {
    for lane in LANES {
        let health = match lane {
            Lane::Hostile => Color::srgb(0.9, 0.1, 0.1),
            Lane::Ally => Color::srgb(0.1, 0.9, 0.1),
        };
        for (measure, fill) in [(Measure::Health, health), (Measure::Recovery, Color::srgb(0.1, 0.9, 0.1))] {
            let whole = Node {
                position_type: PositionType::Absolute,
                width: Val::Px(BAR_WIDTH),
                height: Val::Px(BAR_HEIGHT),
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                ..default()
            };
            commands.spawn((
                Node { left: Val::Px(OFF_SCREEN), top: Val::Auto, ..whole.clone() },
                Visibility::Hidden,
                WorldBar { measure, current_fill: 1.0 },
                lane,
            ))
            .with_children(|parent| {
                parent.spawn((whole.clone(), BackgroundColor(Color::srgb(0.2, 0.2, 0.2))));
                parent.spawn((whole.clone(), BackgroundColor(fill), ZIndex(1), WorldBarFill));
            });
        }

        commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(3.0),
                left: Val::Px(OFF_SCREEN),
                ..default()
            },
            Visibility::Hidden,
            ThreatQueueDots,
            lane,
        ))
        .with_children(|parent| {
            for index in 0..MAX_QUEUE_CAPACITY {
                parent.spawn((
                    Node {
                        width: Val::Px(8.0),
                        height: Val::Px(8.0),
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Percent(50.0)),
                        ..default()
                    },
                    BorderColor::all(Color::srgb(0.5, 0.5, 0.5)),
                    BackgroundColor(Color::srgb(0.3, 0.3, 0.3)),
                    Visibility::Hidden,
                    ThreatCapacityDot { index },
                ));
            }
        });
    }
}

/// Moves each lane's bars onto the target the viewed actor holds in it
/// and eases their fill toward what they measure: health, and how far
/// through its lockout the target is, full with none. Hidden with no target.
pub fn update_world_bars(
    mut bars: Query<(&Lane, &mut WorldBar, &Children, &mut Node, &mut Visibility)>,
    mut fills: Query<&mut Node, (With<WorldBarFill>, Without<WorldBar>)>,
    targets: Query<(&common_bevy::components::resources::Health, Option<&common_bevy::components::recovery::GlobalRecovery>, &Transform)>,
    camera_query: Query<(&Camera, &GlobalTransform), (With<Camera3d>, Without<CloseupCamera>)>,
    viewed: Query<(&Target, Option<&AllyTarget>), With<crate::components::Viewed>>,
    time: Res<Time>,
) {
    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };
    let Ok((target, ally)) = viewed.single() else {
        return;
    };

    for (lane, mut bar, children, mut node, mut visibility) in &mut bars {
        let Some((health, recovery, transform)) = lane.held(target, ally).and_then(|ent| targets.get(ent).ok()) else {
            *visibility = Visibility::Hidden;
            continue;
        };
        *visibility = Visibility::Visible;

        let (measured, below) = match bar.measure {
            Measure::Health => ((health.step / health.max).clamp(0.0, 1.0), 0.0),
            Measure::Recovery => (
                recovery.filter(|recovery| recovery.is_active() && recovery.duration > 0.0)
                    .map_or(1.0, |recovery| 1.0 - recovery.remaining / recovery.duration),
                BAR_HEIGHT,
            ),
        };
        bar.current_fill = bar.current_fill.lerp(measured, FILL_SPEED * time.delta_secs());

        // Over the target's head, centred on it
        let world_pos = transform.translation + Vec3::new(0.0, 1.5, 0.0);
        let Ok(at) = camera.world_to_viewport(camera_transform, world_pos) else {
            node.left = Val::Px(OFF_SCREEN);
            continue;
        };
        node.left = Val::Px(at.x - BAR_WIDTH / 2.0);
        node.top = Val::Px(at.y + below);
        for child in children.iter() {
            if let Ok(mut fill) = fills.get_mut(child) {
                fill.width = Val::Px(BAR_WIDTH * bar.current_fill);
            }
        }
    }
}

/// Moves each lane's capacity dots over its target's bars: a dot to each
/// slot of the target's window, lit where a threat stands in it and red
/// once more wait behind the window. Hidden for a target with no queue.
pub fn update_threat_queue_dots(
    mut holders: Query<(&Lane, &Children, &mut Node, &mut Visibility), With<ThreatQueueDots>>,
    mut dots: Query<(&ThreatCapacityDot, &mut Visibility, &mut BackgroundColor, &mut BorderColor), Without<ThreatQueueDots>>,
    queues: Query<(&common_bevy::components::reaction_queue::ReactionQueue, &Transform)>,
    camera_query: Query<(&Camera, &GlobalTransform), (With<Camera3d>, Without<CloseupCamera>)>,
    viewed: Query<(&Target, Option<&AllyTarget>), With<crate::components::Viewed>>,
) {
    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };
    let Ok((target, ally)) = viewed.single() else {
        return;
    };

    for (lane, children, mut node, mut visibility) in &mut holders {
        let shown = lane.held(target, ally)
            .and_then(|ent| queues.get(ent).ok())
            .filter(|(queue, _)| queue.window_size > 0);
        let Some((queue, transform)) = shown else {
            *visibility = Visibility::Hidden;
            continue;
        };
        *visibility = Visibility::Visible;

        // Above the health bar, its left edge on the bar's
        let world_pos = transform.translation + Vec3::new(0.0, 2.1, 0.0);
        let Ok(at) = camera.world_to_viewport(camera_transform, world_pos) else {
            node.left = Val::Px(OFF_SCREEN);
            continue;
        };
        node.left = Val::Px(at.x - BAR_WIDTH / 2.0);
        node.top = Val::Px(at.y);

        let filled = queue.visible_count();
        let full = queue.hidden_count() > 0;
        for child in children.iter() {
            let Ok((dot, mut dot_visibility, mut background, mut border)) = dots.get_mut(child) else { continue };
            // A dot shows with its holder, so hiding the holder hides them all
            if dot.index >= queue.window_size {
                *dot_visibility = Visibility::Hidden;
                continue;
            }
            *dot_visibility = Visibility::Inherited;
            let (fill, edge) = if dot.index >= filled {
                (Color::srgb(0.3, 0.3, 0.3), Color::srgb(0.5, 0.5, 0.5))
            } else if full {
                (Color::srgb(1.0, 0.2, 0.2), Color::srgb(1.0, 0.2, 0.2))
            } else {
                (Color::srgb(1.0, 0.7, 0.2), Color::srgb(1.0, 0.7, 0.2))
            };
            *background = BackgroundColor(fill);
            *border = BorderColor::all(edge);
        }
    }
}
