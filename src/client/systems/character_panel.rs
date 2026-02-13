use bevy::prelude::*;

use crate::{
    common::{
        components::{Actor, ActorAttributes},
        systems::combat::damage::contest_modifier,
    },
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
#[derive(Component, Debug)]
pub enum AttributeBar {
    MightGrace,
    VitalityFocus,
    InstinctPresence,
}

/// Marker for the spectrum range indicator within the bar
#[derive(Component)]
pub struct SpectrumRange;

/// Marker for the axis position indicator (yellow bar - draggable)
#[derive(Component)]
pub enum AxisMarker {
    MightGrace,
    VitalityFocus,
    InstinctPresence,
}

/// Marker component for meta-attribute stat display (SOW-021)
#[derive(Component, Clone)]
pub enum MetaAttributeStat {
    Impact,
    Composure,
    Finesse,
    Cunning,
    Dominance,
    Toughness,
}

/// Marker for raw stat value display (e.g., "(150)")
#[derive(Component)]
pub struct RawStatValue;

/// Resource to track character panel visibility and drag state
#[derive(Resource, Default)]
pub struct CharacterPanelState {
    pub visible: bool,
    pub dragging: Option<DragState>,
}

/// Tracks which attribute is being dragged
#[derive(Clone, Copy)]
pub struct DragState {
    pub attribute: AttributeType,
    pub bar_entity: Entity,
    pub initial_mouse_x: f32,
    pub initial_shift: i8,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AttributeType {
    MightGrace,
    VitalityFocus,
    InstinctPresence,
}

pub const KEYCODE_CHARACTER_PANEL: KeyCode = KeyCode::KeyC;

macro_rules! create_stat_row {
    ($parent:expr, $left_stat:expr, $right_stat:expr) => {
        $parent.spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(5.),
                padding: UiRect::all(Val::Px(10.)),
                border_radius: BorderRadius::all(Val::Px(4.)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.15, 0.15, 0.15, 0.8)),
        ))
        .with_children(|section| {
            // Two-column layout for stats
            section.spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    column_gap: Val::Px(10.),
                    ..default()
                },
            ))
            .with_children(|row| {
                // Left stat column
                create_stat_display!(row, $left_stat);

                // Right stat column
                create_stat_display!(row, $right_stat);
            });
        });
    };
}

macro_rules! create_stat_display {
    ($parent:expr, $stat:expr) => {
        {
            let (name, color, effect_label) = match $stat {
                MetaAttributeStat::Impact => ("Impact", Color::srgb(0.9, 0.5, 0.5), "Recovery Pushback:"),
                MetaAttributeStat::Composure => ("Composure", Color::srgb(0.5, 0.7, 0.9), "Recovery Reduction:"),
                MetaAttributeStat::Finesse => ("Finesse", Color::srgb(0.9, 0.9, 0.5), "Synergy Reduction:"),
                MetaAttributeStat::Cunning => ("Cunning", Color::srgb(0.7, 0.5, 0.9), "Window Extension:"),
                MetaAttributeStat::Dominance => ("Dominance", Color::srgb(0.9, 0.6, 0.3), "Healing Reduction:"),
                MetaAttributeStat::Toughness => ("Toughness", Color::srgb(0.5, 0.8, 0.5), "Damage Mitigation:"),
            };

            $parent.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(3.),
                    flex_grow: 1.0,
                    ..default()
                },
            ))
            .with_children(|stat_col| {
                // Header row: stat name + raw value
                stat_col.spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(4.),
                        ..default()
                    },
                ))
                .with_children(|header| {
                    // Stat name (colored)
                    header.spawn((
                        Text::new(name),
                        TextFont { font_size: 11.0, ..default() },
                        TextColor(color),
                    ));
                    // Raw stat value
                    header.spawn((
                        $stat.clone(),
                        RawStatValue,
                        Text::new("(0)"),
                        TextFont { font_size: 11.0, ..default() },
                        TextColor(Color::srgb(0.7, 0.7, 0.7)),
                    ));
                });

                // Effect row: label + calculated value (right-aligned)
                stat_col.spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        justify_content: JustifyContent::SpaceBetween,
                        ..default()
                    },
                ))
                .with_children(|effect_row| {
                    // Effect label
                    effect_row.spawn((
                        Text::new(effect_label),
                        TextFont { font_size: 10.0, ..default() },
                        TextColor(Color::srgb(0.8, 0.8, 0.8)),
                    ));
                    // Calculated value (marker for updates)
                    effect_row.spawn((
                        $stat,
                        Text::new("0"),
                        TextFont { font_size: 11.0, ..default() },
                        TextColor(Color::srgb(1.0, 1.0, 1.0)),
                    ));
                });
            });
        }
    };
}

macro_rules! create_attribute_section {
    ($parent:expr, $left_name:expr, $right_name:expr, $left_color:expr, $right_color:expr, $title_marker:expr, $current_marker:expr, $bar_marker:expr, $axis_marker:expr) => {
        $parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(5.),
                padding: UiRect::all(Val::Px(10.)),
                border_radius: BorderRadius::all(Val::Px(4.)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.15, 0.15, 0.15, 0.8)),
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
                    TextColor($left_color),
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
                    TextColor($right_color),
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
                    Interaction::default(),
                )).with_children(|bar_container| {
                    // Background track (full range -120 to +120)
                    bar_container.spawn((
                        Node {
                            width: Val::Percent(100.),
                            height: Val::Percent(100.),
                            position_type: PositionType::Absolute,
                            border_radius: BorderRadius::all(Val::Px(4.)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.2, 0.2, 0.2, 1.0)),
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
                            border_radius: BorderRadius::all(Val::Px(3.)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.3, 0.5, 0.7, 0.4)),
                    ));

                    // Axis bar - shows current available range (draggable)
                    bar_container.spawn((
                        $axis_marker,
                        Node {
                            width: Val::Px(0.),  // Will be set dynamically
                            height: Val::Percent(100.),
                            position_type: PositionType::Absolute,
                            left: Val::Percent(50.),  // Will be set dynamically
                            border_radius: BorderRadius::all(Val::Px(2.)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(1.0, 0.8, 0.0, 0.6)),
                        Interaction::default(),
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
                width: Val::Px(770.),
                padding: UiRect::all(Val::Px(20.)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(15.),
                border_radius: BorderRadius::all(Val::Px(8.)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.1, 0.1, 0.1, 0.9)),
            BorderColor::all(Color::srgb(0.4, 0.4, 0.4)),
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

            // Main content: two columns (sliders left, stats right)
            parent.spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(20.),
                    ..default()
                },
            ))
            .with_children(|main| {
                // Left column: attribute sliders
                main.spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(15.),
                        flex_grow: 1.0,
                        ..default()
                    },
                ))
                .with_children(|left| {
                    // MIGHT ↔ GRACE section (Impact = red, Finesse = yellow)
                    create_attribute_section!(left, "MIGHT", "GRACE",
                        Color::srgb(0.9, 0.5, 0.5), Color::srgb(0.9, 0.9, 0.5),
                        AttributeTitle::MightGrace, AttributeCurrent::MightGrace, AttributeBar::MightGrace, AxisMarker::MightGrace);

                    // VITALITY ↔ FOCUS section (Toughness = green, Composure = blue)
                    create_attribute_section!(left, "VITALITY", "FOCUS",
                        Color::srgb(0.5, 0.8, 0.5), Color::srgb(0.5, 0.7, 0.9),
                        AttributeTitle::VitalityFocus, AttributeCurrent::VitalityFocus, AttributeBar::VitalityFocus, AxisMarker::VitalityFocus);

                    // INSTINCT ↔ PRESENCE section (Cunning = purple, Dominance = orange)
                    create_attribute_section!(left, "INSTINCT", "PRESENCE",
                        Color::srgb(0.7, 0.5, 0.9), Color::srgb(0.9, 0.6, 0.3),
                        AttributeTitle::InstinctPresence, AttributeCurrent::InstinctPresence, AttributeBar::InstinctPresence, AxisMarker::InstinctPresence);
                });

                // Right column: meta-attribute stats
                main.spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(15.),
                        width: Val::Px(360.),
                        ..default()
                    },
                ))
                .with_children(|right| {
                    // Row 1: Impact (left) & Finesse (right) - from MIGHT ↔ GRACE
                    create_stat_row!(right, MetaAttributeStat::Impact, MetaAttributeStat::Finesse);

                    // Row 2: Toughness (left) & Composure (right) - from VITALITY ↔ FOCUS
                    create_stat_row!(right, MetaAttributeStat::Toughness, MetaAttributeStat::Composure);

                    // Row 3: Cunning (left) & Dominance (right) - from INSTINCT ↔ PRESENCE
                    create_stat_row!(right, MetaAttributeStat::Cunning, MetaAttributeStat::Dominance);
                });
            });

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

/// Handle mouse drag to adjust shift values
pub fn handle_shift_drag(
    mut state: ResMut<CharacterPanelState>,
    mut player_query: Query<&mut ActorAttributes, With<Actor>>,
    bar_query: Query<(Entity, &AttributeBar, &Interaction, &Node)>,
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
) {
    if !state.visible {
        return;
    }

    let Ok(mut attrs) = player_query.single_mut() else {
        return;
    };

    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };

    // Start drag on mouse press over any attribute bar
    if buttons.just_pressed(MouseButton::Left) {
        for (bar_entity, bar_type, interaction, _node) in &bar_query {
            if *interaction == Interaction::Hovered || *interaction == Interaction::Pressed {
                let attr_type = match bar_type {
                    AttributeBar::MightGrace => AttributeType::MightGrace,
                    AttributeBar::VitalityFocus => AttributeType::VitalityFocus,
                    AttributeBar::InstinctPresence => AttributeType::InstinctPresence,
                };

                // Get the current shift value for this attribute
                let current_shift = match attr_type {
                    AttributeType::MightGrace => attrs.might_grace_shift(),
                    AttributeType::VitalityFocus => attrs.vitality_focus_shift(),
                    AttributeType::InstinctPresence => attrs.instinct_presence_shift(),
                };

                state.dragging = Some(DragState {
                    attribute: attr_type,
                    bar_entity,
                    initial_mouse_x: cursor_pos.x,
                    initial_shift: current_shift,
                });
                break;
            }
        }
    }

    // Handle dragging
    if let Some(drag_state) = state.dragging {
        if buttons.pressed(MouseButton::Left) {
            // Get the bar's width to calculate pixels per unit
            if let Ok((_entity, _bar_type, _interaction, bar_node)) = bar_query.get(drag_state.bar_entity) {
                let bar_width = if let Val::Px(w) = bar_node.width { w } else { 250.0 };

                // Calculate max attribute value based on current level
                let level = attrs.total_level();
                let max_attr = (level * 2) as f32;

                // Calculate mouse delta in pixels
                let mouse_delta_pixels = cursor_pos.x - drag_state.initial_mouse_x;

                // Convert pixel delta to attribute units
                // Bar width represents the full attribute range (-max_attr to +max_attr)
                let pixels_per_unit = bar_width / (max_attr * 2.0);
                let delta_units = mouse_delta_pixels / pixels_per_unit;

                // Calculate new shift based on initial shift + delta
                let new_shift_f32 = drag_state.initial_shift as f32 + delta_units;

                // Update the appropriate shift value based on attribute type
                let new_shift = new_shift_f32.round() as i8;
                match drag_state.attribute {
                    AttributeType::MightGrace => {
                        attrs.set_might_grace_shift(new_shift);
                    }
                    AttributeType::VitalityFocus => {
                        attrs.set_vitality_focus_shift(new_shift);
                    }
                    AttributeType::InstinctPresence => {
                        attrs.set_instinct_presence_shift(new_shift);
                    }
                }
            }
        }
    }

    // Stop dragging on mouse release
    if buttons.just_released(MouseButton::Left) {
        state.dragging = None;
    }
}

/// Update attribute text and bar visuals when panel is visible
pub fn update_attributes(
    state: Res<CharacterPanelState>,
    player_query: Query<&ActorAttributes, With<Actor>>,
    title_query: Query<(Entity, &AttributeTitle)>,
    current_query: Query<(Entity, &AttributeCurrent)>,
    bar_query: Query<(Entity, &AttributeBar)>,
    meta_query: Query<(&MetaAttributeStat, Entity)>,
    mut spectrum_query: Query<&mut Node, (With<SpectrumRange>, Without<AxisMarker>)>,
    mut axis_query: Query<(&AxisMarker, &mut Node), Without<SpectrumRange>>,
    mut text_query: Query<&mut Text>,
    children: Query<&Children>,
) {
    if !state.visible {
        return;
    }

    let Ok(attrs) = player_query.single() else {
        return;
    };

    // Calculate max scaled attribute value based on current level
    // Each level grants 1 point. Max scaled value if all points → one axis: level × 10
    // At level 10: max is ±100 (10 points × 10 scaling)
    let level = attrs.total_level();
    let max_attr_scaled = (level * 10) as i16;

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
                    update_reach_display(&mut node, left_reach, right_reach, max_attr_scaled);
                }
                // Update axis bar (yellow bar - shows current available values)
                if let Ok((_, mut node)) = axis_query.get_mut(child) {
                    update_axis_bar(&mut node, left_current, right_current, max_attr_scaled);
                }
            }
        }
    }

    // Update meta-attribute raw values and calculated effects (SOW-021)
    // Separate queries for raw values vs calculated effects
    let mut raw_query = meta_query.iter().filter(|(_, e)| {
        text_query.get(*e).ok()
            .and_then(|text| text.0.chars().next())
            .map(|c| c == '(')
            .unwrap_or(false)
    });

    for (meta_stat, entity) in &meta_query {
        if let Ok(mut text) = text_query.get_mut(entity) {
            // Check if this is a raw value display (starts with '(')
            let is_raw = text.0.starts_with('(');

            if is_raw {
                // Update raw stat value in parentheses
                let raw_value = match meta_stat {
                    MetaAttributeStat::Impact => attrs.impact(),
                    MetaAttributeStat::Composure => attrs.composure(),
                    MetaAttributeStat::Finesse => attrs.finesse(),
                    MetaAttributeStat::Cunning => attrs.cunning(),
                    MetaAttributeStat::Dominance => attrs.dominance(),
                    MetaAttributeStat::Toughness => attrs.toughness(),
                };
                **text = format!("({})", raw_value);
            } else {
                // Update calculated effect value
                **text = match meta_stat {
                    MetaAttributeStat::Impact => {
                        // Recovery pushback: effective_impact = impact × contest(impact, 0) → pushback
                        let impact = attrs.impact();
                        let contest_mod = contest_modifier(impact, 0);
                        let effective_impact = (impact as f32) * contest_mod;
                        let pushback_pct = (effective_impact / 600.0).min(0.50) * 100.0;
                        format!("+{:.0}%", pushback_pct)
                    },
                    MetaAttributeStat::Composure => {
                        // Recovery time reduction: show as percentage time saved
                        let composure = attrs.composure();
                        let contest_mod = contest_modifier(composure, 0);  // Uncontested display
                        let effective_composure = (composure as f32) * contest_mod;
                        let speed_increase = (effective_composure * 0.5).min(150.0);  // 0.5% per point, cap at 150%
                        let speed_multiplier = 1.0 + (speed_increase / 100.0);  // 1.0 to 2.5
                        let time_reduction_pct = ((1.0 - 1.0 / speed_multiplier) * 100.0).round() as u8;
                        format!("-{}%", time_reduction_pct)
                    },
                    MetaAttributeStat::Finesse => {
                        // Synergy improvement: 0-50% reduction of synergy unlock time
                        let finesse = attrs.finesse();
                        let contest_mod = contest_modifier(finesse, 0);
                        let improvement_pct = (contest_mod * 50.0).round() as u8;
                        format!("+{}%", improvement_pct)
                    },
                    MetaAttributeStat::Cunning => {
                        // Reaction window extension: show as percentage of max (600ms)
                        let cunning = attrs.cunning();
                        let contest_mod = contest_modifier(cunning, 0);  // Uncontested display
                        let effective_cunning = (cunning as f32) * contest_mod;
                        let extension_ms = (effective_cunning * 2.0).min(600.0);
                        let extension_pct = ((extension_ms / 600.0) * 100.0).round() as u8;
                        format!("+{}%", extension_pct)
                    },
                    MetaAttributeStat::Dominance => {
                        // Healing reduction aura: 25% base × contest_modifier(dominance, 0)
                        let dominance = attrs.dominance();
                        let contest_mod = contest_modifier(dominance, 0);
                        let reduction_pct = (25.0 * contest_mod).round() as u8;
                        format!("-{}%", reduction_pct)
                    },
                    MetaAttributeStat::Toughness => {
                        // Physical damage mitigation: toughness vs dominance contest
                        let toughness = attrs.toughness();
                        let contest_mod = contest_modifier(toughness, 0);  // Uncontested display
                        let effective_toughness = (toughness as f32) * contest_mod;
                        let mitigation_pct = ((effective_toughness / 330.0).min(0.75) * 100.0).round() as u8;
                        format!("-{}%", mitigation_pct)
                    },
                };
            }
        }
    }
}

/// Convert attribute value to percentage position on bar
/// Range is -max_attr to +max_attr mapped to 0% to 100%
/// max_attr is calculated as level * 2 (e.g., at level 10, range is -20 to +20)
fn attr_to_percent(value: i16, max_attr_scaled: i16) -> f32 {
    // Map value from [-max_attr_scaled, +max_attr_scaled] to [0%, 100%]
    let range = max_attr_scaled as f32 * 2.0;
    ((value as f32 + max_attr_scaled as f32) / range * 100.0).clamp(0.0, 100.0)
}

fn update_reach_display(node: &mut Node, left_reach: u16, right_reach: u16, max_attr_scaled: i16) {
    // The reach values represent the maximum value achievable in each direction
    // They are scaled attribute values (axis×10 + spectrum×7)
    //
    // For might_grace with axis=-2, spectrum=3:
    //   might_reach=41 (20+21) at position -41 on the scale
    //   grace_reach=21 at position +21 on the scale
    //
    // For instinct_presence with axis=0, spectrum=3:
    //   instinct_reach=21 at position -21
    //   presence_reach=21 at position +21
    //
    // The bar should show from the leftmost reach to the rightmost reach

    // Left reach is on the negative side (might, vitality, instinct)
    let left_bound = -(left_reach as i16);
    // Right reach is on the positive side (grace, focus, presence)
    let right_bound = right_reach as i16;

    let left_percent = attr_to_percent(left_bound, max_attr_scaled);
    let right_percent = attr_to_percent(right_bound, max_attr_scaled);
    let width_percent = right_percent - left_percent;

    node.left = Val::Percent(left_percent);
    node.width = Val::Percent(width_percent);
}

fn update_axis_bar(node: &mut Node, left_current: u16, right_current: u16, max_attr_scaled: i16) {
    // The yellow bar shows the current available values on each side
    // For might_grace: might=250, grace=50 (scaled values)
    //   Left bound at -250 (might value, scaled)
    //   Right bound at +50 (grace value, scaled)

    let left_bound = -(left_current as i16);
    let right_bound = right_current as i16;

    let left_percent = attr_to_percent(left_bound, max_attr_scaled);
    let right_percent = attr_to_percent(right_bound, max_attr_scaled);
    let width_percent = right_percent - left_percent;

    node.left = Val::Percent(left_percent);
    node.width = Val::Percent(width_percent);
}
