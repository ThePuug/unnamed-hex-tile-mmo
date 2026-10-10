use bevy::prelude::*;

use super::state::{DevConsole, MenuPath};
use crate::plugins::diagnostics::{DateField, DiagnosticsState};
use common_bevy::systems::{Date, SEASONS, WEEKS};

#[derive(Component)]
pub struct DevConsoleRoot;

#[derive(Component)]
pub struct BreadcrumbText;

#[derive(Component)]
pub struct MenuItemsContainer;

/// The colour of the row that goes back or closes the console.
const BACK: Color = Color::srgb(0.8, 0.3, 0.3);

/// One row of the menu: `text` set at `size`, in `color`.
fn line(parent: &mut ChildSpawnerCommands, text: impl Into<String>, size: f32, color: Color) {
    parent.spawn((
        Text::new(text),
        TextFont { font_size: FontSize::Px(size), ..default() },
        TextColor(color),
    ));
}

/// A gap between rows: an empty line, small.
fn gap(parent: &mut ChildSpawnerCommands) {
    parent.spawn((
        Text::new(""),
        TextFont { font_size: FontSize::Px(8.0), ..default() },
    ));
}

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
            line(parent, "Developer Console", 20.0, Color::srgb(0.3, 0.6, 0.9));

            parent.spawn((
                BreadcrumbText,
                Text::new("Main Menu"),
                TextFont { font_size: FontSize::Px(14.0), ..default() },
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
    added: Res<crate::network::AddedLatency>,
    server: Res<crate::resources::Server>,
    time: Res<Time>,
    mut breadcrumb_query: Query<&mut Text, (With<BreadcrumbText>, Without<MenuItemsContainer>)>,
    menu_query: Query<(Entity, Option<&Children>), With<MenuItemsContainer>>,
    mut commands: Commands,
) {
    if !console.is_changed() && !diagnostics_state.is_changed() && !added.is_changed() {
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
                    line(parent, "1. Terrain", 16.0, Color::WHITE);
                    #[cfg(feature = "admin")]
                    {
                        line(parent, "2. Goto Coordinates", 16.0, Color::WHITE);
                        line(parent, format!("3. Added Latency      [{}ms]", added.0.as_millis()), 16.0, state_color(!added.0.is_zero()));
                    }
                    gap(parent);
                    #[cfg(feature = "admin")]
                    for text in ["5. Spawn Den", "6. View", "7. Stage Party", "8. Stage Opposition"] {
                        line(parent, text, 16.0, Color::WHITE);
                    }
                    gap(parent);
                    line(parent, "0. Close Console", 16.0, BACK);
                }
                MenuPath::Terrain => {
                    let held = diagnostics_state.lighting.held_at();
                    let rows = [
                        (format!("1. Toggle Grid Overlay      [{}]", on_off(diagnostics_state.grid_visible)), diagnostics_state.grid_visible),
                        (
                            format!("2. Lighting Time            [{}]", held.as_ref().map_or("Game time".to_string(), |at| format!("Held {at}"))),
                            held.is_some(),
                        ),
                        (
                            format!("3. Toggle Camera Envelope   [{}]", if diagnostics_state.camera_envelope_off { "Lifted" } else { "On" }),
                            !diagnostics_state.camera_envelope_off,
                        ),
                        (format!("4. Terrain                  [{}]", if diagnostics_state.terrain_hidden { "Hidden" } else { "Shown" }), !diagnostics_state.terrain_hidden),
                        (format!("5. Camera Close-up          [{}]", if diagnostics_state.camera_closeup { "On" } else { "Off" }), !diagnostics_state.camera_closeup),
                        (format!("6. Cover                    [{}]", if diagnostics_state.cover_hidden { "Hidden" } else { "Shown" }), !diagnostics_state.cover_hidden),
                        (format!("7. Canopy                   [{}]", if diagnostics_state.canopy_parts_off { "Vertices" } else { "Parts" }), !diagnostics_state.canopy_parts_off),
                    ];
                    for (text, on) in rows {
                        line(parent, text, 16.0, state_color(on));
                    }
                    gap(parent);
                    line(parent, "0. Back to Main Menu", 16.0, BACK);
                }
                MenuPath::LightingTime => {
                    let current = diagnostics_state.lighting.held_at().map_or("Game time".to_string(), |at| format!("Held {at}"));
                    line(parent, format!("Current: {current}"), 14.0, Color::srgb(0.6, 0.8, 1.0));

                    let buf = &console.lighting_time_buf;
                    let display = if buf.is_empty() { "_" } else { buf };
                    line(parent, format!("Time = {display}"), 16.0, Color::srgb(0.9, 0.9, 0.4));

                    let date = Date::of(diagnostics_state.lighting.at(server.current_time(time.elapsed().as_millis())));
                    line(parent, format!("Date = {}", picked_date(date, console.lighting_date_field)), 16.0, Color::srgb(0.9, 0.9, 0.4));

                    for text in [
                        "Enter HHMM or HH, press Enter (empty = game time)",
                        "Left/Right: rewind / forward, hold to hurry",
                        "Tab: pick day / week / season, Up/Down: step it",
                    ] {
                        line(parent, text, 12.0, Color::srgb(0.6, 0.6, 0.6));
                    }
                    gap(parent);
                    line(parent, "Esc. Back", 16.0, BACK);
                }
                #[cfg(feature = "admin")]
                MenuPath::Stage(_) => {
                    for (i, (label, _)) in super::state::DENS.iter().enumerate() {
                        line(parent, format!("{}. {label}", i + 1), 16.0, Color::WHITE);
                    }
                    gap(parent);
                    line(parent, "0. Back", 16.0, BACK);
                }
                #[cfg(feature = "admin")]
                MenuPath::Latency => {
                    line(
                        parent,
                        format!("Round trip added: {}ms, each way half, a tenth either side", added.0.as_millis()),
                        16.0,
                        state_color(!added.0.is_zero()),
                    );
                    gap(parent);
                    let step = super::navigation::LATENCY_STEP_MS;
                    let most = crate::network::AddedLatency::MOST.as_millis();
                    for text in [format!("1. More (+{step}ms, up to {most}ms)"), format!("2. Less (-{step}ms)"), "3. None".to_string()] {
                        line(parent, text, 16.0, Color::WHITE);
                    }
                    gap(parent);
                    line(parent, "0. Back", 16.0, BACK);
                }
                #[cfg(feature = "admin")]
                MenuPath::View => {
                    for text in ["1. View Target", "2. Stop Viewing"] {
                        line(parent, text, 16.0, Color::WHITE);
                    }
                    gap(parent);
                    line(parent, "0. Back", 16.0, BACK);
                }
                #[cfg(feature = "admin")]
                MenuPath::GotoSelect => {
                    for text in ["1. World Units (X, Y)", "2. QR Coordinates (Q, R)"] {
                        line(parent, text, 16.0, Color::WHITE);
                    }
                    gap(parent);
                    line(parent, "0. Back", 16.0, BACK);
                }
                #[cfg(feature = "admin")]
                MenuPath::GotoInput => {
                    if let Some(ref input) = console.goto_input {
                        for (i, label) in input.field_labels().iter().enumerate() {
                            let active = i == input.active_field;
                            let cursor = if active { "▌" } else { "" };
                            let color = if active { Color::srgb(0.3, 0.9, 0.3) } else { Color::srgb(0.7, 0.7, 0.7) };
                            line(parent, format!("{}: {}{}", label, input.buffers[i], cursor), 16.0, color);
                        }
                        gap(parent);
                        line(parent, "Tab: switch field  Enter: submit", 14.0, Color::srgb(0.5, 0.5, 0.5));
                        line(parent, "Esc. Back", 16.0, BACK);
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

/// The date as the HUD shows it, with the picked field bracketed.
fn picked_date(date: Date, picked: DateField) -> String {
    let field = |field: DateField, name: String| {
        if field == picked { format!("[{name}]") } else { name }
    };
    format!(
        "{}.{}.{}",
        field(DateField::Day, date.day.to_string()),
        field(DateField::Week, WEEKS[date.week].to_string()),
        field(DateField::Season, SEASONS[date.season].to_string()),
    )
}
