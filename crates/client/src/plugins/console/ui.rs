use bevy::prelude::*;

use super::state::{DevConsole, MenuPath};
use crate::plugins::diagnostics::{DateField, DiagnosticsState, WeatherHold};
use common_bevy::settings::{Kept, Setting, Settings};
use common_bevy::systems::{Date, DAY_MS, HOUR_MS, MINUTE_MS, SEASONS, WEEKS};

#[derive(Component)]
pub struct DevConsoleRoot;

#[derive(Component)]
pub struct BreadcrumbText;

#[derive(Component)]
pub struct MenuItemsContainer;

/// The world clock menu's row, written every frame while it shows: the
/// game clock moves under it between the key presses that rebuild the menu.
#[derive(Component)]
pub struct DateLine;

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
    settings: Res<Settings>,
    kept: Res<Kept>,
    time: Res<Time>,
    mut breadcrumb_query: Query<&mut Text, (With<BreadcrumbText>, Without<MenuItemsContainer>)>,
    menu_query: Query<(Entity, Option<&Children>), With<MenuItemsContainer>>,
    mut commands: Commands,
    mut shown: Local<Option<common_bevy::systems::Calendar>>,
) {
    // The clock's row reads the calendar the server last sent
    let calendar_changed = shown.replace(server.calendar) != Some(server.calendar);
    if !console.is_changed() && !diagnostics_state.is_changed() && !added.is_changed() && !calendar_changed && !settings.is_changed() && !kept.is_changed() {
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
                MenuPath::Root | MenuPath::World | MenuPath::Travel | MenuPath::Stages => {
                    let held = server.calendar.held.map(hour);
                    for (row, menu) in super::navigation::rows(&console.current_menu).iter().enumerate() {
                        let text = match menu {
                            MenuPath::WorldClock => format!("{}. World Clock              [{}]", row + 1, held.as_ref().map_or("Running".to_string(), |at| format!("Held {at}"))),
                            _ => format!("{}. {}", row + 1, menu.display_name()),
                        };
                        let color = if *menu == MenuPath::WorldClock { state_color(held.is_some()) } else { Color::WHITE };
                        line(parent, text, 16.0, color);
                    }
                    gap(parent);
                    line(parent, if console.current_menu == MenuPath::Root { "0. Close Console" } else { "0. Back" }, 16.0, BACK);
                }
                MenuPath::Settings => {
                    for (row, setting) in Setting::ALL.into_iter().enumerate() {
                        let text = format!("{}. {:<25}[{}]", row + 1, setting.name(), stands(setting, &settings, &kept));
                        line(parent, text, 16.0, state_color(kept.0.get(setting) == setting.default()));
                    }
                    for text in ["The server's, for every player, and kept: green is as built", "A cold setting holds from the server's next start"] {
                        line(parent, text, 12.0, Color::srgb(0.6, 0.6, 0.6));
                    }
                    gap(parent);
                    line(parent, "0. Back", 16.0, BACK);
                }
                MenuPath::Setting(setting) => {
                    let buf = &console.setting_buf;
                    let display = if buf.is_empty() { "_" } else { buf };
                    line(parent, format!("Value = {display}"), 16.0, Color::srgb(0.9, 0.9, 0.4));
                    line(parent, format!("Now = {}", stands(setting, &settings, &kept)), 16.0, Color::srgb(0.9, 0.9, 0.4));
                    let (least, most) = setting.range();
                    for text in [
                        setting.says().to_string(),
                        format!("From {least} to {most}, {} as built", setting.default()),
                        if setting.hot() { "Hot: holds at once, for every player" } else { "Cold: kept at once, holds from the server's next start" }.to_string(),
                        "Type a value, press Enter (empty = as built)".to_string(),
                    ] {
                        line(parent, text, 12.0, Color::srgb(0.6, 0.6, 0.6));
                    }
                    gap(parent);
                    line(parent, "Esc. Back", 16.0, BACK);
                }
                MenuPath::Client => {
                    let rows = [
                        (format!("1. Camera Close-up          [{}]", if diagnostics_state.camera_closeup { "On" } else { "Off" }), !diagnostics_state.camera_closeup),
                        (format!("2. Weather                  [{:?}]", diagnostics_state.weather), diagnostics_state.weather == WeatherHold::Forecast),
                        (format!("3. Added Latency            [{}ms]", added.0.as_millis()), added.0.is_zero()),
                    ];
                    for (text, on) in rows {
                        line(parent, text, 16.0, state_color(on));
                    }
                    gap(parent);
                    line(parent, "0. Back", 16.0, BACK);
                }
                MenuPath::WorldClock => {
                    let buf = &console.clock_buf;
                    let display = if buf.is_empty() { "_" } else { buf };
                    line(parent, format!("Time = {display}"), 16.0, Color::srgb(0.9, 0.9, 0.4));

                    parent.spawn((
                        Text::new(date_line(&console, &server, &time)),
                        TextFont { font_size: FontSize::Px(16.0), ..default() },
                        TextColor(Color::srgb(0.9, 0.9, 0.4)),
                        DateLine,
                    ));

                    for text in [
                        "The world's clock: every player's date, sky and weather",
                        "Enter HHMM or HH, press Enter (empty = the wall clock)",
                        "Left/Right: rewind / forward, hold to hurry",
                        "Tab: pick day / week / season, Up/Down: step by it",
                        "Space: hold / run",
                    ] {
                        line(parent, text, 12.0, Color::srgb(0.6, 0.6, 0.6));
                    }
                    gap(parent);
                    line(parent, "Esc. Back", 16.0, BACK);
                }
                MenuPath::Rollback => {
                    let chosen = console.rollback;
                    for text in [
                        format!("1. Clock                    [{}]", if chosen.game { "Game time" } else { "Real time" }),
                        format!("2. Unit                     [{}]", super::state::Rollback::UNITS[chosen.unit]),
                        format!("3. Fewer  4. More           [{}]", chosen.amount),
                    ] {
                        line(parent, text, 16.0, Color::WHITE);
                    }
                    gap(parent);
                    let (text, color) = if chosen.armed {
                        (format!("Enter again: roll the world back {}", chosen.says()), Color::srgb(0.95, 0.4, 0.3))
                    } else {
                        (format!("Enter: roll the world back {}", chosen.says()), Color::srgb(0.9, 0.9, 0.4))
                    };
                    line(parent, text, 16.0, color);
                    for text in [
                        "Everything kept goes back to that moment for every player:",
                        "the ground, piles, bags, camps, dens, hubs and the ledger.",
                        "The calendar goes back as far. The server stops; start it again.",
                    ] {
                        line(parent, text, 12.0, Color::srgb(0.6, 0.6, 0.6));
                    }
                    gap(parent);
                    line(parent, "0. Back", 16.0, BACK);
                }
                MenuPath::Stage(_) => {
                    for (i, (label, _)) in super::state::DENS.iter().enumerate() {
                        line(parent, format!("{}. {label}", i + 1), 16.0, Color::WHITE);
                    }
                    gap(parent);
                    line(parent, "0. Back", 16.0, BACK);
                }
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
                MenuPath::View => {
                    for text in ["1. View Target", "2. Stop Viewing"] {
                        line(parent, text, 16.0, Color::WHITE);
                    }
                    gap(parent);
                    line(parent, "0. Back", 16.0, BACK);
                }
                MenuPath::GotoSelect => {
                    for text in ["1. World Units (X, Y)", "2. QR Coordinates (Q, R)"] {
                        line(parent, text, 16.0, Color::WHITE);
                    }
                    gap(parent);
                    line(parent, "0. Back", 16.0, BACK);
                }
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

/// A setting as it is in force, and what is kept for the server's next
/// start where that differs.
fn stands(setting: Setting, settings: &Settings, kept: &Kept) -> String {
    let (now, next) = (settings.get(setting), kept.0.get(setting));
    if next == now { now.to_string() } else { format!("{now}, {next} at restart") }
}

fn state_color(state: bool) -> Color {
    if state {
        Color::srgb(0.2, 0.8, 0.2)
    } else {
        Color::srgb(0.8, 0.2, 0.2)
    }
}

/// A calendar moment's hour of its day, as `HH:MM`.
fn hour(ms: u128) -> String {
    format!("{:02}:{:02}", ms % DAY_MS / HOUR_MS, ms % HOUR_MS / MINUTE_MS)
}

/// The world clock menu's row: the calendar's hour and date, the picked
/// field bracketed, and whether it is held.
fn date_line(console: &DevConsole, server: &crate::resources::Server, time: &Time) -> String {
    let now = server.wall(server.now(time.elapsed().as_millis()));
    let held = if server.calendar.held.is_some() { "  held" } else { "" };
    format!("Now = {} {}{held}", hour(now), picked_date(Date::of(now), console.clock_date_field))
}

/// Keeps the clock's row current while the world clock menu shows.
pub fn refresh_date_line(
    console: Res<DevConsole>,
    server: Res<crate::resources::Server>,
    time: Res<Time>,
    mut rows: Query<&mut Text, With<DateLine>>,
) {
    if !console.visible || console.current_menu != MenuPath::WorldClock {
        return;
    }
    for mut row in &mut rows {
        **row = date_line(&console, &server, &time);
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
