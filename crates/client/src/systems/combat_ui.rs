use bevy::prelude::*;
use common_bevy::components::{ally_target::AllyTarget, target::Target};

use crate::{
    components::{Measure, WorldBar, WorldBarFill},
    systems::{closeup::CloseupCamera, target_frame::{Lane, LANES}},
};

/// Where a `Node` goes to be drawn at `world`, or None where that is off
/// screen. The camera gives the window's pixels; a node's are the UI's,
/// which `UiScale` scales, so a node placed by the window's lands short of
/// its mark in a window smaller than the monitor.
fn node_at(camera: &Camera, camera_transform: &GlobalTransform, scale: &UiScale, world: Vec3) -> Option<Vec2> {
    camera.world_to_viewport(camera_transform, world).ok().map(|at| at / scale.0)
}

/// System to update floating text (damage numbers)
/// Projects world position to screen space, moves text upward, fades out, and despawns
pub fn update_floating_text(
    mut commands: Commands,
    mut query: Query<(Entity, &mut crate::components::FloatingText, &mut Node, &mut TextColor)>,
    camera_query: Query<(&Camera, &GlobalTransform), (With<Camera3d>, Without<CloseupCamera>)>,
    scale: Res<UiScale>,
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
        if let Some(at) = node_at(camera, camera_transform, &scale, floating_text.world_position) {
            node.left = Val::Px(at.x);
            node.top = Val::Px(at.y);
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

/// Where a node sits while what it follows is off screen
const OFF_SCREEN: f32 = -10000.0;

/// Builds, for each lane, the bars drawn over its target in the world:
/// health, and recovery flush under it.
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

    }
}

/// Moves each lane's bars onto the target the viewed actor holds in it
/// and eases their fill toward what they measure: health, and how far
/// through its recovery the target is, full with none. Hidden with no target.
pub fn update_world_bars(
    mut bars: Query<(&Lane, &mut WorldBar, &Children, &mut Node, &mut Visibility)>,
    mut fills: Query<&mut Node, (With<WorldBarFill>, Without<WorldBar>)>,
    targets: Query<(&common_bevy::components::resources::Health, Option<&common_bevy::components::recovery::GlobalRecovery>, &Transform)>,
    camera_query: Query<(&Camera, &GlobalTransform), (With<Camera3d>, Without<CloseupCamera>)>,
    viewed: Query<(&Target, Option<&AllyTarget>), With<crate::components::Viewed>>,
    scale: Res<UiScale>,
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
            Measure::Health => ((health.state / health.max).clamp(0.0, 1.0), 0.0),
            Measure::Recovery => (
                recovery.filter(|recovery| recovery.is_active() && recovery.duration > 0.0)
                    .map_or(1.0, |recovery| 1.0 - recovery.remaining / recovery.duration),
                BAR_HEIGHT,
            ),
        };
        bar.current_fill = bar.current_fill.lerp(measured, FILL_SPEED * time.delta_secs());

        // Over the target's head, centred on it
        let world_pos = transform.translation + Vec3::new(0.0, 1.5, 0.0);
        let Some(at) = node_at(camera, camera_transform, &scale, world_pos) else {
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
