use bevy::prelude::*;

use crate::{
    common::{
        components::{Actor, ActorAttributes},
        systems::combat::damage::contest_factor,
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

/// Marker component for attribute current value row (container for values + buttons)
#[derive(Component)]
pub enum AttributeCurrent {
    MightGrace,
    VitalityFocus,
    InstinctPresence,
}

/// Marker for left current value text
#[derive(Component)]
pub enum LeftCurrentValue {
    MightGrace,
    VitalityFocus,
    InstinctPresence,
}

/// Marker for right current value text
#[derive(Component)]
pub enum RightCurrentValue {
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

/// Marker for axis adjustment buttons (left-side positioned)
#[derive(Component, Clone, Copy)]
pub enum AxisAdjustButton {
    MightGraceDecrease,
    MightGraceIncrease,
    VitalityFocusDecrease,
    VitalityFocusIncrease,
    InstinctPresenceDecrease,
    InstinctPresenceIncrease,
}

/// Marker for axis adjustment buttons (right-side positioned)
#[derive(Component, Clone, Copy)]
pub enum AxisAdjustButtonRight {
    MightGraceDecrease,
    MightGraceIncrease,
    VitalityFocusDecrease,
    VitalityFocusIncrease,
    InstinctPresenceDecrease,
    InstinctPresenceIncrease,
}

/// Marker for spectrum adjustment buttons
#[derive(Component, Clone, Copy)]
pub enum SpectrumAdjustButton {
    MightGraceDecrease,
    MightGraceIncrease,
    VitalityFocusDecrease,
    VitalityFocusIncrease,
    InstinctPresenceDecrease,
    InstinctPresenceIncrease,
}

/// Marker for Apply Respec button
#[derive(Component)]
pub struct ApplyRespecButton;

/// Marker for Apply button text (shows budget)
#[derive(Component)]
pub struct ApplyButtonText;

/// Draft attributes for respec (axis/spectrum only, shift is immediate)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DraftAttributes {
    pub might_grace_axis: i8,
    pub might_grace_spectrum: i8,
    pub vitality_focus_axis: i8,
    pub vitality_focus_spectrum: i8,
    pub instinct_presence_axis: i8,
    pub instinct_presence_spectrum: i8,
}

impl DraftAttributes {
    /// Create draft from current ActorAttributes
    pub fn from_current(attrs: &ActorAttributes) -> Self {
        Self {
            might_grace_axis: attrs.might_grace_axis(),
            might_grace_spectrum: attrs.might_grace_spectrum(),
            vitality_focus_axis: attrs.vitality_focus_axis(),
            vitality_focus_spectrum: attrs.vitality_focus_spectrum(),
            instinct_presence_axis: attrs.instinct_presence_axis(),
            instinct_presence_spectrum: attrs.instinct_presence_spectrum(),
        }
    }

    /// Calculate total investment (for budget validation)
    pub fn total_investment(&self) -> u32 {
        self.might_grace_axis.unsigned_abs() as u32
            + self.might_grace_spectrum.max(0) as u32
            + self.vitality_focus_axis.unsigned_abs() as u32
            + self.vitality_focus_spectrum.max(0) as u32
            + self.instinct_presence_axis.unsigned_abs() as u32
            + self.instinct_presence_spectrum.max(0) as u32
    }

    /// Validate draft against level budget
    pub fn is_valid(&self, level: u32) -> bool {
        let max_investment = level as i8;
        self.total_investment() <= level
            && self.might_grace_axis.abs() <= max_investment
            && self.might_grace_spectrum >= 0
            && self.might_grace_spectrum <= max_investment
            && self.vitality_focus_axis.abs() <= max_investment
            && self.vitality_focus_spectrum >= 0
            && self.vitality_focus_spectrum <= max_investment
            && self.instinct_presence_axis.abs() <= max_investment
            && self.instinct_presence_spectrum >= 0
            && self.instinct_presence_spectrum <= max_investment
    }
}

/// Resource to track character panel visibility and drag state
#[derive(Resource, Default)]
pub struct CharacterPanelState {
    pub visible: bool,
    pub dragging: Option<DragState>,
    pub pending_respec: Option<DraftAttributes>,  // None = no pending changes
}

impl CharacterPanelState {
    pub fn has_pending_changes(&self) -> bool {
        self.pending_respec.is_some()
    }

    pub fn mark_dirty(&mut self, attrs: &ActorAttributes) {
        if self.pending_respec.is_none() {
            self.pending_respec = Some(DraftAttributes::from_current(attrs));
        }
    }
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
                MetaAttributeStat::Cunning => ("Cunning", Color::srgb(0.7, 0.5, 0.9), "Reaction Window:"),
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
    ($parent:expr, $left_name:expr, $right_name:expr, $left_color:expr, $right_color:expr, $title_marker:expr, $current_marker:expr, $bar_marker:expr, $axis_marker:expr, $left_current_marker:expr, $right_current_marker:expr, $axis_dec_left_marker:expr, $axis_inc_left_marker:expr, $axis_dec_right_marker:expr, $axis_inc_right_marker:expr, $spectrum_dec_marker:expr, $spectrum_inc_marker:expr) => {
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

            // Bar and current values row (with inline axis buttons)
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
                // Left value container (value + axis buttons)
                bar_row.spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(4.),
                        min_width: Val::Px(80.),  // Reserve space for value + buttons
                        ..default()
                    },
                ))
                .with_children(|left_container| {
                    // Plus button (left-side) - increases commitment (away from center, on outside)
                    left_container.spawn((
                        $axis_inc_left_marker,
                        Button,
                        Node {
                            width: Val::Px(16.),
                            height: Val::Px(16.),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.4, 0.4, 0.4)),
                        Visibility::Hidden,  // Will be shown conditionally
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new("+"),
                            TextFont { font_size: 10.0, ..default() },
                        ));
                    });

                    // Minus button (left-side) - reduces commitment (toward center, on inside)
                    left_container.spawn((
                        $axis_dec_left_marker,
                        Button,
                        Node {
                            width: Val::Px(16.),
                            height: Val::Px(16.),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.4, 0.4, 0.4)),
                        Visibility::Hidden,  // Will be shown conditionally
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new("−"),
                            TextFont { font_size: 10.0, ..default() },
                        ));
                    });

                    // Left current value
                    left_container.spawn((
                        $left_current_marker,
                        Text::new("0"),
                        TextFont { font_size: 13.0, ..default() },
                        TextColor(Color::srgb(0.9, 0.9, 0.9)),
                    ));
                });

                // Visual bar container (spectrum buttons centered on bar)
                bar_row.spawn((
                    Node {
                        position_type: PositionType::Relative,
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                ))
                .with_children(|bar_wrapper| {
                    // The actual bar
                    bar_wrapper.spawn((
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

                        // Spectrum decrease button (above bar, centered, left side)
                        bar_container.spawn((
                            $spectrum_dec_marker,
                            Button,
                            Node {
                                width: Val::Px(20.),
                                height: Val::Px(20.),
                                position_type: PositionType::Absolute,
                                // Position at center minus 12px (half of 20px button + 2px gap)
                                left: Val::Px(113.),  // 250px bar / 2 - 12px = 113px
                                top: Val::Px(-24.),   // Above the bar to avoid occlusion
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                ..default()
                            },
                            BackgroundColor(Color::srgb(0.4, 0.4, 0.4)),
                        ))
                        .with_children(|btn| {
                            btn.spawn((
                                Text::new("−"),
                                TextFont { font_size: 12.0, ..default() },
                            ));
                        });

                        // Spectrum increase button (above bar, centered, right side)
                        bar_container.spawn((
                            $spectrum_inc_marker,
                            Button,
                            Node {
                                width: Val::Px(20.),
                                height: Val::Px(20.),
                                position_type: PositionType::Absolute,
                                // Position at center plus 2px gap
                                left: Val::Px(127.),  // 250px bar / 2 + 2px = 127px
                                top: Val::Px(-24.),   // Above the bar to avoid occlusion
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                ..default()
                            },
                            BackgroundColor(Color::srgb(0.4, 0.4, 0.4)),
                        ))
                        .with_children(|btn| {
                            btn.spawn((
                                Text::new("+"),
                                TextFont { font_size: 12.0, ..default() },
                            ));
                        });
                    });
                });

                // Right value container (buttons + value)
                bar_row.spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(4.),
                        min_width: Val::Px(80.),  // Reserve space to match left side
                        justify_content: JustifyContent::FlexEnd,
                        ..default()
                    },
                ))
                .with_children(|right_container| {
                    // Minus button (right-side) - reduces commitment (toward center)
                    right_container.spawn((
                        $axis_dec_right_marker,
                        Button,
                        Node {
                            width: Val::Px(16.),
                            height: Val::Px(16.),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.4, 0.4, 0.4)),
                        Visibility::Hidden,  // Will be shown conditionally
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new("−"),
                            TextFont { font_size: 10.0, ..default() },
                        ));
                    });

                    // Plus button (right-side) - increases commitment (away from center)
                    right_container.spawn((
                        $axis_inc_right_marker,
                        Button,
                        Node {
                            width: Val::Px(16.),
                            height: Val::Px(16.),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.4, 0.4, 0.4)),
                        Visibility::Hidden,  // Will be shown conditionally
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new("+"),
                            TextFont { font_size: 10.0, ..default() },
                        ));
                    });

                    // Right current value
                    right_container.spawn((
                        $right_current_marker,
                        Text::new("0"),
                        TextFont { font_size: 13.0, ..default() },
                        TextColor(Color::srgb(0.9, 0.9, 0.9)),
                    ));
                });
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
                        AttributeTitle::MightGrace, AttributeCurrent::MightGrace, AttributeBar::MightGrace, AxisMarker::MightGrace,
                        LeftCurrentValue::MightGrace, RightCurrentValue::MightGrace,
                        AxisAdjustButton::MightGraceDecrease, AxisAdjustButton::MightGraceIncrease,
                        AxisAdjustButtonRight::MightGraceDecrease, AxisAdjustButtonRight::MightGraceIncrease,
                        SpectrumAdjustButton::MightGraceDecrease, SpectrumAdjustButton::MightGraceIncrease);

                    // VITALITY ↔ FOCUS section (Toughness = green, Composure = blue)
                    create_attribute_section!(left, "VITALITY", "FOCUS",
                        Color::srgb(0.5, 0.8, 0.5), Color::srgb(0.5, 0.7, 0.9),
                        AttributeTitle::VitalityFocus, AttributeCurrent::VitalityFocus, AttributeBar::VitalityFocus, AxisMarker::VitalityFocus,
                        LeftCurrentValue::VitalityFocus, RightCurrentValue::VitalityFocus,
                        AxisAdjustButton::VitalityFocusDecrease, AxisAdjustButton::VitalityFocusIncrease,
                        AxisAdjustButtonRight::VitalityFocusDecrease, AxisAdjustButtonRight::VitalityFocusIncrease,
                        SpectrumAdjustButton::VitalityFocusDecrease, SpectrumAdjustButton::VitalityFocusIncrease);

                    // INSTINCT ↔ PRESENCE section (Cunning = purple, Dominance = orange)
                    create_attribute_section!(left, "INSTINCT", "PRESENCE",
                        Color::srgb(0.7, 0.5, 0.9), Color::srgb(0.9, 0.6, 0.3),
                        AttributeTitle::InstinctPresence, AttributeCurrent::InstinctPresence, AttributeBar::InstinctPresence, AxisMarker::InstinctPresence,
                        LeftCurrentValue::InstinctPresence, RightCurrentValue::InstinctPresence,
                        AxisAdjustButton::InstinctPresenceDecrease, AxisAdjustButton::InstinctPresenceIncrease,
                        AxisAdjustButtonRight::InstinctPresenceDecrease, AxisAdjustButtonRight::InstinctPresenceIncrease,
                        SpectrumAdjustButton::InstinctPresenceDecrease, SpectrumAdjustButton::InstinctPresenceIncrease);
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

            // Apply button (hidden by default, shown when changes are pending)
            parent.spawn((
                ApplyRespecButton,
                Button,
                Node {
                    width: Val::Px(180.),
                    height: Val::Px(30.),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    margin: UiRect::top(Val::Px(10.)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.3, 0.7, 0.3)),
                Visibility::Hidden,
            ))
            .with_children(|btn| {
                btn.spawn((
                    ApplyButtonText,
                    Text::new("Apply Changes"),
                    TextFont { font_size: 14.0, ..default() },
                ));
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

        // Cancel any pending respec when closing panel
        if !state.visible {
            state.pending_respec = None;
        }

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
    left_value_query: Query<(Entity, &LeftCurrentValue)>,
    right_value_query: Query<(Entity, &RightCurrentValue)>,
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

    // Use draft values if available, otherwise use committed values
    let draft_attrs = if let Some(draft) = &state.pending_respec {
        // Create a temporary ActorAttributes with draft values for display
        let mut temp_attrs = attrs.clone();
        temp_attrs.apply_respec(
            draft.might_grace_axis,
            draft.might_grace_spectrum,
            draft.vitality_focus_axis,
            draft.vitality_focus_spectrum,
            draft.instinct_presence_axis,
            draft.instinct_presence_spectrum,
        );
        Some(temp_attrs)
    } else {
        None
    };
    let display_attrs = draft_attrs.as_ref().unwrap_or(attrs);

    // Calculate max scaled attribute value based on ACTUAL character level (not draft)
    // The bar scale should stay fixed regardless of draft allocation
    // Each level grants 1 point. Max scaled value if all points → one axis: level × 10
    // At level 10: max is ±100 (10 points × 10 scaling)
    let level = attrs.total_level(); // Use actual level, not draft
    let max_attr_scaled = (level * 10) as i16;

    // Update title rows (reach values)
    for (title_entity, attr_type) in &title_query {
        let (left_reach, right_reach) = match attr_type {
            AttributeTitle::MightGrace => (display_attrs.might_reach(), display_attrs.grace_reach()),
            AttributeTitle::VitalityFocus => (display_attrs.vitality_reach(), display_attrs.focus_reach()),
            AttributeTitle::InstinctPresence => (display_attrs.instinct_reach(), display_attrs.presence_reach()),
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

    // Update left current values
    for (left_entity, attr_type) in &left_value_query {
        let left_current = match attr_type {
            LeftCurrentValue::MightGrace => display_attrs.might(),
            LeftCurrentValue::VitalityFocus => display_attrs.vitality(),
            LeftCurrentValue::InstinctPresence => display_attrs.instinct(),
        };

        if let Ok(mut text) = text_query.get_mut(left_entity) {
            **text = format!("{}", left_current);
        }
    }

    // Update right current values
    for (right_entity, attr_type) in &right_value_query {
        let right_current = match attr_type {
            RightCurrentValue::MightGrace => display_attrs.grace(),
            RightCurrentValue::VitalityFocus => display_attrs.focus(),
            RightCurrentValue::InstinctPresence => display_attrs.presence(),
        };

        if let Ok(mut text) = text_query.get_mut(right_entity) {
            **text = format!("{}", right_current);
        }
    }

    // Update bar visuals
    for (bar_entity, bar_type) in &bar_query {
        let (left_reach, right_reach, left_current, right_current) = match bar_type {
            AttributeBar::MightGrace => (
                display_attrs.might_reach(),
                display_attrs.grace_reach(),
                display_attrs.might(),
                display_attrs.grace(),
            ),
            AttributeBar::VitalityFocus => (
                display_attrs.vitality_reach(),
                display_attrs.focus_reach(),
                display_attrs.vitality(),
                display_attrs.focus(),
            ),
            AttributeBar::InstinctPresence => (
                display_attrs.instinct_reach(),
                display_attrs.presence_reach(),
                display_attrs.instinct(),
                display_attrs.presence(),
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
    for (meta_stat, entity) in &meta_query {
        if let Ok(mut text) = text_query.get_mut(entity) {
            // Check if this is a raw value display (starts with '(')
            let is_raw = text.0.starts_with('(');

            if is_raw {
                // Update raw stat value in parentheses
                let raw_value = match meta_stat {
                    MetaAttributeStat::Impact => display_attrs.impact(),
                    MetaAttributeStat::Composure => display_attrs.composure(),
                    MetaAttributeStat::Finesse => display_attrs.finesse(),
                    MetaAttributeStat::Cunning => display_attrs.cunning(),
                    MetaAttributeStat::Dominance => display_attrs.dominance(),
                    MetaAttributeStat::Toughness => display_attrs.toughness(),
                };
                **text = format!("({})", raw_value);
            } else {
                // Update calculated effect value (uncontested display)
                **text = match meta_stat {
                    MetaAttributeStat::Impact => {
                        // Recovery pushback: 0.50 × gap × contest_factor
                        let impact = display_attrs.impact();
                        let contest = contest_factor(impact, 0);  // vs 0 composure
                        let pushback_pct = (0.50 * contest) * 100.0;
                        format!("+{:.0}%", pushback_pct)
                    },
                    MetaAttributeStat::Composure => {
                        // Recovery time reduction: 0.33 × gap × contest_factor
                        let composure = display_attrs.composure();
                        let contest = contest_factor(composure, 0);  // vs 0 impact
                        let reduction_pct = (0.33 * contest) * 100.0;
                        format!("-{:.0}%", reduction_pct)
                    },
                    MetaAttributeStat::Finesse => {
                        // Synergy reduction: 0.66 × gap × contest_factor
                        let finesse = display_attrs.finesse();
                        let contest = contest_factor(finesse, 0);  // vs 0 cunning
                        let reduction_pct = (0.66 * contest) * 100.0;
                        format!("-{:.0}%", reduction_pct)
                    },
                    MetaAttributeStat::Cunning => {
                        // Reaction window: 3.0s × (1.0 + 0.5 × contest_factor)
                        // Display raw time value (different pattern from other stats)
                        let cunning = display_attrs.cunning();
                        let contest = contest_factor(cunning, 0);  // vs 0 finesse
                        let multiplier = 1.0 + 0.5 * contest;
                        let window_seconds = 3.0 * multiplier;
                        format!("{:.1}s", window_seconds)
                    },
                    MetaAttributeStat::Dominance => {
                        // Healing reduction aura: 0.25 × gap × contest_factor
                        let dominance = display_attrs.dominance();
                        let contest = contest_factor(dominance, 0);  // vs 0 toughness
                        let reduction_pct = (0.25 * contest) * 100.0;
                        format!("-{:.0}%", reduction_pct)
                    },
                    MetaAttributeStat::Toughness => {
                        // Damage mitigation: 0.75 × gap × contest_factor
                        let toughness = display_attrs.toughness();
                        let contest = contest_factor(toughness, 0);  // vs 0 dominance
                        let mitigation_pct = (0.75 * contest) * 100.0;
                        format!("-{:.0}%", mitigation_pct)
                    },
                };
            }
        }
    }
}

/// Update axis button visibility based on current axis value
pub fn update_axis_button_visibility(
    state: Res<CharacterPanelState>,
    player_query: Query<&ActorAttributes, With<Actor>>,
    mut left_buttons: Query<(&AxisAdjustButton, &mut Visibility), Without<AxisAdjustButtonRight>>,
    mut right_buttons: Query<(&AxisAdjustButtonRight, &mut Visibility)>,
) {
    // Hide all buttons if panel is not visible
    if !state.visible {
        for (_, mut vis) in &mut left_buttons {
            *vis = Visibility::Hidden;
        }
        for (_, mut vis) in &mut right_buttons {
            *vis = Visibility::Hidden;
        }
        return;
    }

    let Ok(attrs) = player_query.single() else {
        return;
    };

    // Get current draft or use actual attributes
    let default_draft = DraftAttributes::from_current(attrs);
    let draft = state.pending_respec.as_ref().unwrap_or(&default_draft);

    // Update left-side buttons
    for (button, mut visibility) in &mut left_buttons {
        let should_show = match button {
            // Show decrease when axis < 0 (can decrease further on left side)
            AxisAdjustButton::MightGraceDecrease => draft.might_grace_axis < 0,
            AxisAdjustButton::VitalityFocusDecrease => draft.vitality_focus_axis < 0,
            AxisAdjustButton::InstinctPresenceDecrease => draft.instinct_presence_axis < 0,
            // Show increase when axis <= 0 (can commit to left, or increase left commitment)
            AxisAdjustButton::MightGraceIncrease => draft.might_grace_axis <= 0,
            AxisAdjustButton::VitalityFocusIncrease => draft.vitality_focus_axis <= 0,
            AxisAdjustButton::InstinctPresenceIncrease => draft.instinct_presence_axis <= 0,
        };

        *visibility = if should_show {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }

    // Update right-side buttons
    for (button, mut visibility) in &mut right_buttons {
        let should_show = match button {
            // Show decrease when axis >= 0 (can move back left from right side)
            AxisAdjustButtonRight::MightGraceDecrease => draft.might_grace_axis >= 0,
            AxisAdjustButtonRight::VitalityFocusDecrease => draft.vitality_focus_axis >= 0,
            AxisAdjustButtonRight::InstinctPresenceDecrease => draft.instinct_presence_axis >= 0,
            // Show increase when axis > 0 (can increase right commitment)
            AxisAdjustButtonRight::MightGraceIncrease => draft.might_grace_axis > 0,
            AxisAdjustButtonRight::VitalityFocusIncrease => draft.vitality_focus_axis > 0,
            AxisAdjustButtonRight::InstinctPresenceIncrease => draft.instinct_presence_axis > 0,
        };

        *visibility = if should_show {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
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

/// Update Apply button text with budget counter and enable/disable based on allocation
pub fn update_apply_button(
    state: Res<CharacterPanelState>,
    player_query: Query<&ActorAttributes, With<Actor>>,
    mut button_query: Query<(&mut BackgroundColor, &Children), With<ApplyRespecButton>>,
    mut text_query: Query<&mut Text, With<ApplyButtonText>>,
) {
    if !state.visible {
        return;
    }

    let Ok(attrs) = player_query.single() else {
        return;
    };

    let Some(draft) = &state.pending_respec else {
        return; // No pending changes
    };

    let Ok((mut bg_color, children)) = button_query.single_mut() else {
        return;
    };

    let level = attrs.total_level();
    let investment = draft.total_investment();
    let unallocated = level.saturating_sub(investment);

    // Find the text child and update it
    for child in children.iter() {
        if let Ok(mut text) = text_query.get_mut(child) {
            if unallocated > 0 {
                **text = format!("Apply ({} points left)", unallocated);
                // Disable button (red)
                *bg_color = BackgroundColor(Color::srgb(0.6, 0.3, 0.3));
            } else {
                **text = "Apply Changes".to_string();
                // Enable button (green)
                *bg_color = BackgroundColor(Color::srgb(0.3, 0.7, 0.3));
            }
        }
    }
}

