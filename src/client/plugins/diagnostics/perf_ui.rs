use bevy::prelude::*;
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin, EntityCountDiagnosticsPlugin};
use bevy::picking::Pickable;

use super::DiagnosticsRoot;
use super::config::DiagnosticsState;

// ============================================================================
// Components
// ============================================================================

/// Marker component for the root performance UI entity
///
/// Used to identify the performance overlay for visibility toggling.
#[derive(Component)]
pub struct PerfUiRootMarker;

#[derive(Component)]
pub(super) struct FpsText;

#[derive(Component)]
pub(super) struct FrameTimeText;

#[derive(Component)]
pub(super) struct EntityCountText;

#[derive(Component)]
pub(super) struct TerrainTilesText;

// ============================================================================
// Systems
// ============================================================================

const FONT_SIZE: f32 = 16.0;
const LABEL_COLOR: Color = Color::srgba(0.7, 0.7, 0.7, 1.0);
const VALUE_COLOR: Color = Color::srgba(1.0, 1.0, 1.0, 1.0);

fn metric_row(label: &str) -> (Text, TextFont, TextColor) {
    (
        Text::new(format!("{label}: --")),
        TextFont {
            font_size: FONT_SIZE,
            ..default()
        },
        TextColor(LABEL_COLOR),
    )
}

/// Creates the performance UI panel as a child of the diagnostics root container
pub fn setup_performance_ui(
    mut commands: Commands,
    state: Res<DiagnosticsState>,
    root_q: Query<Entity, With<DiagnosticsRoot>>,
) {
    let root = root_q.single().unwrap();

    let panel = commands
        .spawn((
            PerfUiRootMarker,
            Pickable::IGNORE,
            Node {
                display: if state.perf_ui_visible { Display::Flex } else { Display::None },
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(2.0),
                padding: UiRect::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
        ))
        .with_children(|parent| {
            parent.spawn((FpsText, metric_row("FPS")));
            parent.spawn((FrameTimeText, metric_row("Frame")));
            parent.spawn((EntityCountText, metric_row("Entities")));
            parent.spawn((TerrainTilesText, metric_row("Terrain Tiles")));
        })
        .id();

    commands.entity(root).add_child(panel);
}

/// Updates the performance UI text every frame
pub fn update_performance_ui(
    diagnostics: Res<DiagnosticsStore>,
    map: Res<crate::common::resources::map::Map>,
    mut fps_q: Query<(&mut Text, &mut TextColor), (With<FpsText>, Without<FrameTimeText>, Without<EntityCountText>, Without<TerrainTilesText>)>,
    mut frame_q: Query<(&mut Text, &mut TextColor), (With<FrameTimeText>, Without<FpsText>, Without<EntityCountText>, Without<TerrainTilesText>)>,
    mut entity_q: Query<&mut Text, (With<EntityCountText>, Without<FpsText>, Without<FrameTimeText>, Without<TerrainTilesText>)>,
    mut terrain_q: Query<&mut Text, (With<TerrainTilesText>, Without<FpsText>, Without<FrameTimeText>, Without<EntityCountText>)>,
) {
    if let Ok((mut text, mut color)) = fps_q.single_mut() {
        if let Some(fps) = diagnostics.get(&FrameTimeDiagnosticsPlugin::FPS).and_then(|d| d.smoothed()) {
            **text = format!("FPS: {fps:.0}");
            color.0 = if fps >= 55.0 { VALUE_COLOR } else { Color::srgba(1.0, 0.4, 0.4, 1.0) };
        }
    }

    if let Ok((mut text, mut color)) = frame_q.single_mut() {
        if let Some(ft) = diagnostics.get(&FrameTimeDiagnosticsPlugin::FRAME_TIME).and_then(|d| d.smoothed()) {
            **text = format!("Frame: {ft:.1} ms");
            color.0 = if ft <= 18.0 { VALUE_COLOR } else { Color::srgba(1.0, 0.4, 0.4, 1.0) };
        }
    }

    if let Ok(mut text) = entity_q.single_mut() {
        if let Some(count) = diagnostics.get(&EntityCountDiagnosticsPlugin::ENTITY_COUNT).and_then(|d| d.smoothed()) {
            **text = format!("Entities: {count:.0}");
        }
    }

    if let Ok(mut text) = terrain_q.single_mut() {
        **text = format!("Terrain Tiles: {}", map.len());
    }
}
