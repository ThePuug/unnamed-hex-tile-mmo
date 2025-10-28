use bevy::prelude::*;

use crate::{
    common::components::{Actor, ActorAttributes},
};

/// Marker component for the character panel root node
#[derive(Component)]
pub struct CharacterPanel;

/// Marker component for attribute title row (contains reach values)
#[derive(Component)]
pub enum AttributeTitle {
    MightGrace,
    VitalityFocus,
    InstinctPresence,
}

/// Marker component for attribute current value text
#[derive(Component)]
pub enum AttributeCurrent {
    MightGrace,
    VitalityFocus,
    InstinctPresence,
}

/// Marker component for the visual attribute bar
#[derive(Component)]
pub enum AttributeBar {
    MightGrace,
    VitalityFocus,
    InstinctPresence,
}

/// Marker for the spectrum range indicator within the bar
#[derive(Component)]
pub struct SpectrumRange;

/// Marker for the axis position indicator
#[derive(Component)]
pub struct AxisMarker;

/// Resource to track character panel visibility
#[derive(Resource, Default)]
pub struct CharacterPanelState {
    pub visible: bool,
}

pub const KEYCODE_CHARACTER_PANEL: KeyCode = KeyCode::KeyC;

macro_rules! create_attribute_section {
    ($parent:expr, $left_name:expr, $right_name:expr, $title_marker:expr, $current_marker:expr, $bar_marker:expr) => {
        $parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(5.),
                padding: UiRect::all(Val::Px(10.)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.15, 0.15, 0.15, 0.8)),
            BorderRadius::all(Val::Px(4.)),
        ))
        .with_children(|section| {
            // Title row: reach LABEL ↔ LABEL reach
            section.spawn((
                $title_marker,
                Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    margin: UiRect::bottom(Val::Px(2.)),
                    ..default()
                },
            )).with_children(|title_row| {
                // Left reach value (outer)
                title_row.spawn((
                    Text::new("0"),
                    TextFont { font_size: 12.0, ..default() },
                    TextColor(Color::srgb(0.6, 0.6, 0.6)),
                ));
                // Left attribute name
                title_row.spawn((
                    Text::new($left_name),
                    TextFont { font_size: 14.0, ..default() },
                    TextColor(Color::srgb(0.9, 0.6, 0.6)),
                ));
                // Arrow separator
                title_row.spawn((
                    Text::new("↔"),
                    TextFont { font_size: 14.0, ..default() },
                    TextColor(Color::srgb(0.7, 0.7, 0.7)),
                ));
                // Right attribute name
                title_row.spawn((
                    Text::new($right_name),
                    TextFont { font_size: 14.0, ..default() },
                    TextColor(Color::srgb(0.6, 0.6, 0.9)),
                ));
                // Right reach value (outer)
                title_row.spawn((
                    Text::new("0"),
                    TextFont { font_size: 12.0, ..default() },
                    TextColor(Color::srgb(0.6, 0.6, 0.6)),
                ));
            });

            // Bar and current values row
            section.spawn((
                $current_marker,
                Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(8.),
                    ..default()
                },
            )).with_children(|bar_row| {
                // Left current value (inner)
                bar_row.spawn((
                    Text::new("0"),
                    TextFont { font_size: 13.0, ..default() },
                    TextColor(Color::srgb(0.9, 0.9, 0.9)),
                ));

                // Visual bar container (-120 to +120 range)
                bar_row.spawn((
                    $bar_marker,
                    Node {
                        width: Val::Px(250.),
                        height: Val::Px(20.),
                        position_type: PositionType::Relative,
                        ..default()
                    },
                )).with_children(|bar_container| {
                    // Background track (full range -120 to +120)
                    bar_container.spawn((
                        Node {
                            width: Val::Percent(100.),
                            height: Val::Percent(100.),
                            position_type: PositionType::Absolute,
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.2, 0.2, 0.2, 1.0)),
                        BorderRadius::all(Val::Px(4.)),
                    ));

                    // Center line (at 0)
                    bar_container.spawn((
                        Node {
                            width: Val::Px(2.),
                            height: Val::Percent(100.),
                            position_type: PositionType::Absolute,
                            left: Val::Percent(50.),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.5, 0.5, 0.5, 0.8)),
                    ));

                    // Spectrum range indicator (will be updated dynamically)
                    bar_container.spawn((
                        SpectrumRange,
                        Node {
                            height: Val::Percent(100.),
                            position_type: PositionType::Absolute,
                            left: Val::Percent(50.),
                            width: Val::Px(0.),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.3, 0.5, 0.7, 0.4)),
                        BorderRadius::all(Val::Px(3.)),
                    ));

                    // Axis bar - shows current available range (will be updated dynamically)
                    bar_container.spawn((
                        AxisMarker,
                        Node {
                            width: Val::Px(0.),  // Will be set dynamically
                            height: Val::Percent(100.),
                            position_type: PositionType::Absolute,
                            left: Val::Percent(50.),  // Will be set dynamically
                            ..default()
                        },
                        BackgroundColor(Color::srgba(1.0, 0.8, 0.0, 0.6)),
                        BorderRadius::all(Val::Px(2.)),
                    ));
                });

                // Right current value (inner)
                bar_row.spawn((
                    Text::new("0"),
                    TextFont { font_size: 13.0, ..default() },
                    TextColor(Color::srgb(0.9, 0.9, 0.9)),
                ));
            });
        });
    };
}

pub fn setup(
    mut commands: Commands,
) {
    commands.init_resource::<CharacterPanelState>();

    // Create character panel (initially hidden)
    commands
        .spawn((
            CharacterPanel,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(20.),
                top: Val::Px(100.),
                width: Val::Px(400.),
                padding: UiRect::all(Val::Px(20.)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(15.),
                ..default()
            },
            BackgroundColor(Color::srgba(0.1, 0.1, 0.1, 0.9)),
            BorderColor(Color::srgb(0.4, 0.4, 0.4)),
            BorderRadius::all(Val::Px(8.)),
            Visibility::Hidden,
        ))
        .with_children(|parent| {
            // Title
            parent.spawn((
                Text::new("Character Attributes"),
                TextFont {
                    font_size: 24.0,
                    ..default()
                },
                TextColor(Color::srgb(0.9, 0.9, 0.9)),
                Node {
                    margin: UiRect::bottom(Val::Px(10.)),
                    ..default()
                },
            ));

            // MIGHT ↔ GRACE section
            create_attribute_section!(parent, "MIGHT", "GRACE", AttributeTitle::MightGrace, AttributeCurrent::MightGrace, AttributeBar::MightGrace);

            // VITALITY ↔ FOCUS section
            create_attribute_section!(parent, "VITALITY", "FOCUS", AttributeTitle::VitalityFocus, AttributeCurrent::VitalityFocus, AttributeBar::VitalityFocus);

            // INSTINCT ↔ PRESENCE section
            create_attribute_section!(parent, "INSTINCT", "PRESENCE", AttributeTitle::InstinctPresence, AttributeCurrent::InstinctPresence, AttributeBar::InstinctPresence);
        });
}

/// Toggle panel visibility when 'C' key is pressed
pub fn toggle_panel(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<CharacterPanelState>,
    mut query: Query<&mut Visibility, With<CharacterPanel>>,
) {
    if keyboard.just_pressed(KEYCODE_CHARACTER_PANEL) {
        state.visible = !state.visible;

        if let Ok(mut visibility) = query.single_mut() {
            *visibility = if state.visible {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }
}

/// Update attribute text and bar visuals when panel is visible
pub fn update_attributes(
    state: Res<CharacterPanelState>,
    player_query: Query<&ActorAttributes, With<Actor>>,
    title_query: Query<(Entity, &AttributeTitle)>,
    current_query: Query<(Entity, &AttributeCurrent)>,
    bar_query: Query<(Entity, &AttributeBar)>,
    mut spectrum_query: Query<&mut Node, (With<SpectrumRange>, Without<AxisMarker>)>,
    mut axis_query: Query<&mut Node, (With<AxisMarker>, Without<SpectrumRange>)>,
    mut text_query: Query<&mut Text>,
    children: Query<&Children>,
) {
    if !state.visible {
        return;
    }

    let Ok(attrs) = player_query.single() else {
        return;
    };

    // Update title rows (reach values)
    for (title_entity, attr_type) in &title_query {
        let (left_reach, right_reach) = match attr_type {
            AttributeTitle::MightGrace => (attrs.might_reach(), attrs.grace_reach()),
            AttributeTitle::VitalityFocus => (attrs.vitality_reach(), attrs.focus_reach()),
            AttributeTitle::InstinctPresence => (attrs.instinct_reach(), attrs.presence_reach()),
        };

        // Update the reach text values (first and last child)
        if let Ok(title_children) = children.get(title_entity) {
            if title_children.len() >= 5 {
                // First child: left reach value
                if let Ok(mut text) = text_query.get_mut(title_children[0]) {
                    **text = format!("{}", left_reach);
                }
                // Last child: right reach value
                if let Ok(mut text) = text_query.get_mut(title_children[4]) {
                    **text = format!("{}", right_reach);
                }
            }
        }
    }

    // Update current value rows
    for (current_entity, attr_type) in &current_query {
        let (left_current, right_current) = match attr_type {
            AttributeCurrent::MightGrace => (attrs.might(), attrs.grace()),
            AttributeCurrent::VitalityFocus => (attrs.vitality(), attrs.focus()),
            AttributeCurrent::InstinctPresence => (attrs.instinct(), attrs.presence()),
        };

        // Update the current text values (first and last child)
        if let Ok(current_children) = children.get(current_entity) {
            if current_children.len() >= 3 {
                // First child: left current value
                if let Ok(mut text) = text_query.get_mut(current_children[0]) {
                    **text = format!("{}", left_current);
                }
                // Last child: right current value
                if let Ok(mut text) = text_query.get_mut(current_children[2]) {
                    **text = format!("{}", right_current);
                }
            }
        }
    }

    // Update bar visuals
    for (bar_entity, bar_type) in &bar_query {
        let (left_reach, right_reach, left_current, right_current) = match bar_type {
            AttributeBar::MightGrace => (
                attrs.might_reach(),
                attrs.grace_reach(),
                attrs.might(),
                attrs.grace(),
            ),
            AttributeBar::VitalityFocus => (
                attrs.vitality_reach(),
                attrs.focus_reach(),
                attrs.vitality(),
                attrs.focus(),
            ),
            AttributeBar::InstinctPresence => (
                attrs.instinct_reach(),
                attrs.presence_reach(),
                attrs.instinct(),
                attrs.presence(),
            ),
        };

        // Find child markers for this bar
        if let Ok(bar_children) = children.get(bar_entity) {
            for child in bar_children.iter() {
                // Update spectrum range (blue bar - shows reach values)
                if let Ok(mut node) = spectrum_query.get_mut(child) {
                    update_reach_display(&mut node, left_reach, right_reach);
                }
                // Update axis bar (yellow bar - shows current available values)
                if let Ok(mut node) = axis_query.get_mut(child) {
                    update_axis_bar(&mut node, left_current, right_current);
                }
            }
        }
    }
}

/// Convert attribute value to percentage position on bar (range -120 to +120 mapped to 0% to 100%)
fn attr_to_percent(value: i8) -> f32 {
    ((value as f32 + 120.0) / 240.0 * 100.0).clamp(0.0, 100.0)
}

fn update_reach_display(node: &mut Node, left_reach: u8, right_reach: u8) {
    // The reach values represent the maximum value achievable in each direction
    // They are absolute attribute values on the -100 to +100 scale
    //
    // For might_grace with axis=-20:
    //   might_reach=30 means the value "30 might" which is at position -30 on the scale
    //   grace_reach=20 means the value "20 grace" which is at position +20 on the scale
    //
    // For instinct_presence with axis=0:
    //   instinct_reach=20 means value "20 instinct" at position -20
    //   presence_reach=20 means value "20 presence" at position +20
    //
    // The bar should show from the leftmost reach to the rightmost reach

    // Left reach is on the negative side (might, vitality, instinct)
    let left_bound = -(left_reach as i8);
    // Right reach is on the positive side (grace, focus, presence)
    let right_bound = right_reach as i8;

    let left_percent = attr_to_percent(left_bound);
    let right_percent = attr_to_percent(right_bound);
    let width_percent = right_percent - left_percent;

    node.left = Val::Percent(left_percent);
    node.width = Val::Percent(width_percent);
}

fn update_axis_bar(node: &mut Node, left_current: u8, right_current: u8) {
    // The yellow bar shows the current available values on each side
    // For might_grace: might=25, grace=5
    //   Left bound at -25 (might value)
    //   Right bound at +5 (grace value)

    let left_bound = -(left_current as i8);
    let right_bound = right_current as i8;

    let left_percent = attr_to_percent(left_bound);
    let right_percent = attr_to_percent(right_bound);
    let width_percent = right_percent - left_percent;

    node.left = Val::Percent(left_percent);
    node.width = Val::Percent(width_percent);
}
