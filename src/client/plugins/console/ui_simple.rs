use bevy::prelude::*;

use super::state::{DevConsole, MenuPath};
use crate::{
    client::{
        plugins::diagnostics::DiagnosticsState,
    },
};

/// Marker component for the console root entity
#[derive(Component)]
pub struct DevConsoleRoot;

/// Marker component for the breadcrumb text
#[derive(Component)]
pub struct BreadcrumbText;

/// Marker component for the menu items container
#[derive(Component)]
pub struct MenuItemsContainer;

/// System to set up the developer console UI
pub fn setup_dev_console(mut commands: Commands) {
    commands
        .spawn((
            DevConsoleRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(50.0),
                top: Val::Percent(40.0),
                width: Val::Px(450.0),
                padding: UiRect::all(Val::Px(15.0)),
                margin: UiRect {
                    left: Val::Px(-225.0),  // Half of width to center horizontally
                    top: Val::Px(-150.0),   // Approximate vertical offset for centering
                    ..default()
                },
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.1, 0.1, 0.1, 0.9)),
            BorderColor(Color::srgb(0.3, 0.6, 0.9)),
            BorderRadius::all(Val::Px(8.0)),
            Visibility::Hidden,
            ZIndex(1000),
        ))
        .with_children(|parent| {
            // Title section
            parent.spawn((
                Text::new("Developer Console"),
                TextFont {
                    font_size: 20.0,
                    ..default()
                },
                TextColor(Color::srgb(0.3, 0.6, 0.9)),
            ));

            // Breadcrumb section
            parent.spawn((
                BreadcrumbText,
                Text::new("Main Menu"),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::srgb(0.7, 0.7, 0.7)),
            ));

            // Menu items container
            parent.spawn((
                MenuItemsContainer,
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(5.0),
                    ..default()
                },
            ));
        });
}

/// System to update console visibility based on state
pub fn update_console_visibility(
    console: Res<DevConsole>,
    mut query: Query<&mut Visibility, With<DevConsoleRoot>>,
) {
    if let Ok(mut visibility) = query.single_mut() {
        *visibility = if console.visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

/// System to update console menu content when menu changes
pub fn update_console_menu(
    console: Res<DevConsole>,
    diagnostics_state: Res<DiagnosticsState>,
    mut breadcrumb_query: Query<&mut Text, (With<BreadcrumbText>, Without<MenuItemsContainer>)>,
    menu_query: Query<(Entity, Option<&Children>), With<MenuItemsContainer>>,
    mut commands: Commands,
) {
    // Only update if console state changed
    if !console.is_changed() && !diagnostics_state.is_changed() {
        return;
    }

    // Update breadcrumb
    if let Ok(mut breadcrumb) = breadcrumb_query.single_mut() {
        **breadcrumb = console.current_menu.display_name().to_string();
    }

    // Rebuild menu items
    if let Ok((container_entity, maybe_children)) = menu_query.single() {
        // Despawn all existing children
        if let Some(children) = maybe_children {
            for child in children.iter() {
                commands.entity(child).despawn();
            }
        }

        commands.entity(container_entity).with_children(|parent| {
            match console.current_menu {
                MenuPath::Root => {
                    // Root menu
                    parent.spawn((
                        Text::new("1. Terrain"),
                        TextFont { font_size: 16.0, ..default() },
                        TextColor(Color::WHITE),
                    ));

                    parent.spawn((
                        Text::new("2. Performance"),
                        TextFont { font_size: 16.0, ..default() },
                        TextColor(Color::WHITE),
                    ));

                    parent.spawn((
                        Text::new(""),
                        TextFont { font_size: 8.0, ..default() },
                    ));

                    parent.spawn((
                        Text::new("0. Close Console"),
                        TextFont { font_size: 16.0, ..default() },
                        TextColor(Color::srgb(0.8, 0.3, 0.3)),
                    ));
                }
                MenuPath::Terrain => {
                    // Terrain menu
                    parent.spawn((
                        Text::new(format!("1. Toggle Grid Overlay      [{}]", on_off(diagnostics_state.grid_visible))),
                        TextFont { font_size: 16.0, ..default() },
                        TextColor(state_color(diagnostics_state.grid_visible)),
                    ));

                    parent.spawn((
                        Text::new(format!("2. Toggle Slope Rendering   [{}]", on_off(diagnostics_state.slope_rendering_enabled))),
                        TextFont { font_size: 16.0, ..default() },
                        TextColor(state_color(diagnostics_state.slope_rendering_enabled)),
                    ));

                    parent.spawn((
                        Text::new(format!(
                            "3. Toggle Fixed Lighting    [{}]",
                            if diagnostics_state.fixed_lighting_enabled { "Fixed" } else { "Dynamic" }
                        )),
                        TextFont { font_size: 16.0, ..default() },
                        TextColor(state_color(diagnostics_state.fixed_lighting_enabled)),
                    ));

                    parent.spawn((
                        Text::new("4. Regenerate Mesh          [Action]"),
                        TextFont { font_size: 16.0, ..default() },
                        TextColor(Color::srgb(0.8, 0.8, 0.2)),
                    ));

                    parent.spawn((
                        Text::new(""),
                        TextFont { font_size: 8.0, ..default() },
                    ));

                    parent.spawn((
                        Text::new("0. Back to Main Menu"),
                        TextFont { font_size: 16.0, ..default() },
                        TextColor(Color::srgb(0.8, 0.3, 0.3)),
                    ));
                }
                MenuPath::Performance => {
                    // Performance menu
                    parent.spawn((
                        Text::new(format!("1. Toggle Performance UI    [{}]", on_off(diagnostics_state.perf_ui_visible))),
                        TextFont { font_size: 16.0, ..default() },
                        TextColor(state_color(diagnostics_state.perf_ui_visible)),
                    ));

                    parent.spawn((
                        Text::new(format!("2. Toggle Network UI        [{}]", on_off(diagnostics_state.network_ui_visible))),
                        TextFont { font_size: 16.0, ..default() },
                        TextColor(state_color(diagnostics_state.network_ui_visible)),
                    ));

                    parent.spawn((
                        Text::new(""),
                        TextFont { font_size: 8.0, ..default() },
                    ));

                    parent.spawn((
                        Text::new("0. Back to Main Menu"),
                        TextFont { font_size: 16.0, ..default() },
                        TextColor(Color::srgb(0.8, 0.3, 0.3)),
                    ));
                }
            }
        });
    }
}

// Helper functions

fn on_off(state: bool) -> &'static str {
    if state { "ON" } else { "OFF" }
}

fn state_color(state: bool) -> Color {
    if state {
        Color::srgb(0.2, 0.8, 0.2) // Green for ON
    } else {
        Color::srgb(0.8, 0.2, 0.2) // Red for OFF
    }
}
