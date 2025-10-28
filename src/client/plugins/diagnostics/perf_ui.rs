use bevy::prelude::*;
use iyes_perf_ui::prelude::*;

use super::config::{DiagnosticsConfig, DiagnosticsState};
use crate::common::components::{behaviour::Behaviour, entity_type::EntityType};

// ============================================================================
// Components
// ============================================================================

/// Marker component for the root performance UI entity
///
/// Used to identify the performance overlay for visibility toggling.
#[derive(Component)]
pub struct PerfUiRoot;

/// Marker for custom terrain tile counter text
#[derive(Component)]
pub struct TerrainTileCounter;

// ============================================================================
// Systems
// ============================================================================

/// Creates the performance UI overlay on startup
///
/// The UI displays default metrics (FPS, frame time, entity count, etc.)
/// and respects the initial visibility setting from DiagnosticsState.
pub fn setup_performance_ui(
    mut commands: Commands,
    state: Res<DiagnosticsState>,
) {
    // Spawn the iyes_perf_ui root
    commands.spawn((
        PerfUiRoot,
        PerfUiDefaultEntries::default(),
        if state.perf_ui_visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        },
    ));

    // Spawn a custom terrain tile counter as a separate text overlay
    commands.spawn((
        TerrainTileCounter,
        Text::new("Terrain Tiles: 0"),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            right: Val::Px(10.0),
            ..default()
        },
        TextFont {
            font_size: 16.0,
            ..default()
        },
        TextColor(Color::srgb(0.0, 1.0, 0.0)),
        if state.perf_ui_visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        },
    ));
}

/// Updates the terrain tile counter text
pub fn update_terrain_tile_counter(
    map: Res<crate::common::resources::map::Map>,
    mut counter_query: Query<&mut Text, With<TerrainTileCounter>>,
) {
    let tile_count = map.len();

    if let Ok(mut text) = counter_query.single_mut() {
        **text = format!("Terrain Tiles: {}", tile_count);
    }
}

/// Toggles performance UI visibility when the perf UI toggle key is pressed
///
/// Updates both the DiagnosticsState resource and the visibility component
/// of the performance UI entity.
pub fn toggle_performance_ui(
    keyboard: Res<ButtonInput<KeyCode>>,
    config: Res<DiagnosticsConfig>,
    mut state: ResMut<DiagnosticsState>,
    mut perf_ui_query: Query<&mut Visibility, With<PerfUiRoot>>,
    mut counter_query: Query<&mut Visibility, (With<TerrainTileCounter>, Without<PerfUiRoot>)>,
) {
    if keyboard.just_pressed(config.perf_ui_toggle_key) {
        state.perf_ui_visible = !state.perf_ui_visible;

        // Toggle iyes_perf_ui
        if let Ok(mut visibility) = perf_ui_query.single_mut() {
            *visibility = if state.perf_ui_visible {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }

        // Toggle terrain tile counter
        if let Ok(mut visibility) = counter_query.single_mut() {
            *visibility = if state.perf_ui_visible {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }

        info!(
            "Performance UI {}",
            if state.perf_ui_visible { "enabled" } else { "disabled" }
        );
    }
}
