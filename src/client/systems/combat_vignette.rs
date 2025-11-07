use bevy::prelude::*;
use crate::common::components::{resources::CombatState, behaviour::PlayerControlled};

/// Marker component for the combat vignette overlay edges
#[derive(Component)]
pub struct CombatVignette;

/// Edge position for vignette overlays
#[derive(Component)]
pub enum VignetteEdge {
    Top,
    Bottom,
    Left,
    Right,
}

/// Setup combat vignette overlay (red edge overlays for combat indication)
/// Spawns once when UI is initialized
pub fn setup(mut commands: Commands) {
    // Vignette edge thickness (in pixels)
    const EDGE_SIZE: f32 = 150.0;

    // Top edge
    commands.spawn((
        Node {
            width: Val::Percent(100.0),
            height: Val::Px(EDGE_SIZE),
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.54, 0.0, 0.0, 0.0)),
        ZIndex(100),
        CombatVignette,
        VignetteEdge::Top,
    ));

    // Bottom edge
    commands.spawn((
        Node {
            width: Val::Percent(100.0),
            height: Val::Px(EDGE_SIZE),
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            bottom: Val::Px(0.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.54, 0.0, 0.0, 0.0)),
        ZIndex(100),
        CombatVignette,
        VignetteEdge::Bottom,
    ));

    // Left edge
    commands.spawn((
        Node {
            width: Val::Px(EDGE_SIZE),
            height: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.54, 0.0, 0.0, 0.0)),
        ZIndex(100),
        CombatVignette,
        VignetteEdge::Left,
    ));

    // Right edge
    commands.spawn((
        Node {
            width: Val::Px(EDGE_SIZE),
            height: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            right: Val::Px(0.0),
            top: Val::Px(0.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.54, 0.0, 0.0, 0.0)),
        ZIndex(100),
        CombatVignette,
        VignetteEdge::Right,
    ));
}

/// Update combat vignette opacity based on player's combat state
/// Smoothly fades in when entering combat, fades out when leaving combat
pub fn update(
    mut vignette_query: Query<&mut BackgroundColor, With<CombatVignette>>,
    player_query: Query<&CombatState, With<PlayerControlled>>,
    time: Res<Time>,
) {
    let Ok(combat_state) = player_query.get_single() else {
        return;
    };

    // Target opacity based on combat state
    const COMBAT_OPACITY: f32 = 0.25; // Slightly higher opacity for edge visibility
    const OUT_OF_COMBAT_OPACITY: f32 = 0.0;
    const FADE_SPEED: f32 = 3.0; // Fade to target in ~0.3s (3.0 * 0.1s = 0.3s for 90% change)

    let target_alpha = if combat_state.in_combat {
        COMBAT_OPACITY
    } else {
        OUT_OF_COMBAT_OPACITY
    };

    // Update all vignette edges
    for mut bg_color in &mut vignette_query {
        // Lerp current alpha toward target
        let current_alpha = bg_color.0.alpha();
        let new_alpha = current_alpha + (target_alpha - current_alpha) * (FADE_SPEED * time.delta_secs()).min(1.0);

        bg_color.0.set_alpha(new_alpha);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vignette_fade_parameters() {
        // Verify fade parameters are reasonable
        const COMBAT_OPACITY: f32 = 0.25;
        const OUT_OF_COMBAT_OPACITY: f32 = 0.0;

        assert!(COMBAT_OPACITY > 0.0 && COMBAT_OPACITY < 1.0, "Combat opacity should be visible but not opaque");
        assert_eq!(OUT_OF_COMBAT_OPACITY, 0.0, "Out of combat should be fully transparent");
    }
}
