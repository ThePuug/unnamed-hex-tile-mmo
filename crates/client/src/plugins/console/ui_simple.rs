use bevy::prelude::*;

use super::state::{DevConsole, MenuPath};
use crate::plugins::diagnostics::DiagnosticsState;

#[derive(Component)]
pub struct DevConsoleRoot;

#[derive(Component)]
pub struct BreadcrumbText;

#[derive(Component)]
pub struct MenuItemsContainer;

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
                    left: Val::Px(-225.0),
                    top: Val::Px(-150.0),
                    ..default()
                },
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                border_radius: BorderRadius::all(Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.1, 0.1, 0.1, 0.9)),
            BorderColor::all(Color::srgb(0.3, 0.6, 0.9)),
            Visibility::Hidden,
            ZIndex(1000),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("Developer Console"),
                TextFont { font_size: 20.0, ..default() },
                TextColor(Color::srgb(0.3, 0.6, 0.9)),
            ));

            parent.spawn((
                BreadcrumbText,
                Text::new("Main Menu"),
                TextFont { font_size: 14.0, ..default() },
                TextColor(Color::srgb(0.7, 0.7, 0.7)),
            ));

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

pub fn update_console_menu(
    console: Res<DevConsole>,
    diagnostics_state: Res<DiagnosticsState>,
    #[cfg(feature = "admin")] flyover: Res<crate::systems::admin::FlyoverState>,
    mut breadcrumb_query: Query<&mut Text, (With<BreadcrumbText>, Without<MenuItemsContainer>)>,
    menu_query: Query<(Entity, Option<&Children>), With<MenuItemsContainer>>,
    mut commands: Commands,
) {
    if !console.is_changed() && !diagnostics_state.is_changed() {
        return;
    }

    if let Ok(mut breadcrumb) = breadcrumb_query.single_mut() {
        **breadcrumb = console.current_menu.display_name().to_string();
    }

    if let Ok((container_entity, maybe_children)) = menu_query.single() {
        if let Some(children) = maybe_children {
            for child in children.iter() {
                commands.entity(child).despawn();
            }
        }

        commands.entity(container_entity).with_children(|parent| {
            match console.current_menu {
                MenuPath::Root => {
                    // Submenus first
                    parent.spawn((
                        Text::new("1. Terrain"),
                        TextFont { font_size: 16.0, ..default() },
                        TextColor(Color::WHITE),
                    ));

                    #[cfg(feature = "admin")]
                    parent.spawn((
                        Text::new("2. Flyover"),
                        TextFont { font_size: 16.0, ..default() },
                        TextColor(Color::WHITE),
                    ));

                    parent.spawn((
                        Text::new(""),
                        TextFont { font_size: 8.0, ..default() },
                    ));

                    // Toggles after
                    let metrics_key = if cfg!(feature = "admin") { "3" } else { "2" };
                    parent.spawn((
                        Text::new(format!("{}. Toggle Metrics Overlay    [{}]", metrics_key, on_off(diagnostics_state.metrics_overlay_visible))),
                        TextFont { font_size: 16.0, ..default() },
                        TextColor(state_color(diagnostics_state.metrics_overlay_visible)),
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
                    parent.spawn((
                        Text::new(format!("1. Toggle Grid Overlay      [{}]", on_off(diagnostics_state.grid_visible))),
                        TextFont { font_size: 16.0, ..default() },
                        TextColor(state_color(diagnostics_state.grid_visible)),
                    ));

                    parent.spawn((
                        Text::new(format!(
                            "2. Toggle Fixed Lighting    [{}]",
                            if diagnostics_state.fixed_lighting_enabled { "Fixed" } else { "Dynamic" }
                        )),
                        TextFont { font_size: 16.0, ..default() },
                        TextColor(state_color(diagnostics_state.fixed_lighting_enabled)),
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
                #[cfg(feature = "admin")]
                MenuPath::Flyover => {
                    let flyover_label = if flyover.active { "Disable" } else { "Enable" };
                    parent.spawn((
                        Text::new(format!("1. {} Flyover Camera", flyover_label)),
                        TextFont { font_size: 16.0, ..default() },
                        TextColor(Color::srgb(0.8, 0.8, 0.2)),
                    ));

                    let goto_color = if flyover.active {
                        Color::WHITE
                    } else {
                        Color::srgb(0.4, 0.4, 0.4)
                    };
                    parent.spawn((
                        Text::new("2. Goto Coordinates"),
                        TextFont { font_size: 16.0, ..default() },
                        TextColor(goto_color),
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
                #[cfg(feature = "admin")]
                MenuPath::GotoSelect => {
                    parent.spawn((
                        Text::new("1. World Units (X, Y)"),
                        TextFont { font_size: 16.0, ..default() },
                        TextColor(Color::WHITE),
                    ));

                    parent.spawn((
                        Text::new("2. QR Coordinates (Q, R)"),
                        TextFont { font_size: 16.0, ..default() },
                        TextColor(Color::WHITE),
                    ));

                    parent.spawn((
                        Text::new(""),
                        TextFont { font_size: 8.0, ..default() },
                    ));

                    parent.spawn((
                        Text::new("0. Back"),
                        TextFont { font_size: 16.0, ..default() },
                        TextColor(Color::srgb(0.8, 0.3, 0.3)),
                    ));
                }
                #[cfg(feature = "admin")]
                MenuPath::GotoInput => {
                    if let Some(ref input) = console.goto_input {
                        let labels = input.field_labels();
                        for (i, label) in labels.iter().enumerate() {
                            let value = &input.buffers[i];
                            let cursor = if i == input.active_field { "▌" } else { "" };
                            let text = format!("{}: {}{}", label, value, cursor);
                            let color = if i == input.active_field {
                                Color::srgb(0.3, 0.9, 0.3)
                            } else {
                                Color::srgb(0.7, 0.7, 0.7)
                            };
                            parent.spawn((
                                Text::new(text),
                                TextFont { font_size: 16.0, ..default() },
                                TextColor(color),
                            ));
                        }

                        parent.spawn((
                            Text::new(""),
                            TextFont { font_size: 8.0, ..default() },
                        ));

                        parent.spawn((
                            Text::new("Tab: switch field  Enter: submit"),
                            TextFont { font_size: 14.0, ..default() },
                            TextColor(Color::srgb(0.5, 0.5, 0.5)),
                        ));

                        parent.spawn((
                            Text::new("Esc. Back"),
                            TextFont { font_size: 16.0, ..default() },
                            TextColor(Color::srgb(0.8, 0.3, 0.3)),
                        ));
                    }
                }
            }
        });
    }
}

fn on_off(state: bool) -> &'static str {
    if state { "ON" } else { "OFF" }
}

fn state_color(state: bool) -> Color {
    if state {
        Color::srgb(0.2, 0.8, 0.2)
    } else {
        Color::srgb(0.8, 0.2, 0.2)
    }
}
