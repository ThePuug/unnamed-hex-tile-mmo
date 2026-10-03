use bevy::prelude::*;

use common_bevy::{
    components::{Actor, ActorAttributes, Pair},
    systems::combat::damage::contest_factor,
};

use crate::systems::{
    bag_panel,
    equipment_panel::{self, CURSOR_ROW, OTHER_ROW},
};
use common_bevy::tuning::Tuning;

/// The panel's tabs, stacked down its left edge in this order.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PanelTab {
    #[default]
    Attributes,
    Equipment,
    Bag,
}

impl PanelTab {
    pub const ALL: [PanelTab; 3] = [PanelTab::Attributes, PanelTab::Equipment, PanelTab::Bag];

    pub fn name(self) -> &'static str {
        match self {
            PanelTab::Attributes => "Attributes",
            PanelTab::Equipment => "Equipment",
            PanelTab::Bag => "Bag",
        }
    }

    pub fn above(self) -> PanelTab {
        match self {
            PanelTab::Attributes => PanelTab::Attributes,
            PanelTab::Equipment => PanelTab::Attributes,
            PanelTab::Bag => PanelTab::Equipment,
        }
    }

    pub fn below(self) -> PanelTab {
        match self {
            PanelTab::Attributes => PanelTab::Equipment,
            PanelTab::Equipment => PanelTab::Bag,
            PanelTab::Bag => PanelTab::Bag,
        }
    }
}

/// A tab's content, shown while its tab is chosen.
#[derive(Component)]
pub struct TabContent(pub PanelTab);

/// A tab's label in the strip.
#[derive(Component)]
pub struct TabLabel(pub PanelTab);

/// Marker component for the character panel root node
#[derive(Component)]
pub struct CharacterPanel;

/// Marker component for attribute title row (contains reach values)
#[derive(Component)]
pub enum AttributeTitle {
    MightAgility,
    VitalityDiscipline,
    InstinctResolve,
}

/// Marker component for attribute current value row (container for values + bar)
#[derive(Component)]
pub enum AttributeCurrent {
    MightAgility,
    VitalityDiscipline,
    InstinctResolve,
}

/// Marker for left current value text
#[derive(Component)]
pub enum LeftCurrentValue {
    MightAgility,
    VitalityDiscipline,
    InstinctResolve,
}

/// Marker for right current value text
#[derive(Component)]
pub enum RightCurrentValue {
    MightAgility,
    VitalityDiscipline,
    InstinctResolve,
}

/// Marker component for the visual attribute bar
#[derive(Component, Debug)]
pub enum AttributeBar {
    MightAgility,
    VitalityDiscipline,
    InstinctResolve,
}

/// Marker for the spectrum range indicator within the bar
#[derive(Component)]
pub struct SpectrumRange;

/// Marker for the axis position indicator (yellow bar)
#[derive(Component)]
pub enum AxisMarker {
    MightAgility,
    VitalityDiscipline,
    InstinctResolve,
}

/// Marker component for meta-attribute stat display, a header with what the
/// stat is worth and a line for what that gives: the six absolutes, the six
/// contest stats and the six commitments.
#[derive(Component, Clone)]
pub enum MetaAttributeStat {
    Force,
    Tempo,
    Constitution,
    Endurance,
    Intuition,
    Concentration,
    Impact,
    Composure,
    Flow,
    Reflex,
    Focus,
    Toughness,
    Ferocity,
    Grace,
    Grit,
    Preparation,
    Patience,
    Awareness,
}

/// Marker for raw stat value display (e.g., "(150)", or a commitment's "(T2)")
#[derive(Component)]
pub struct RawStatValue;

/// A pair's section of the attributes tab, by the pair's place in a respec.
#[derive(Component)]
pub struct PairSection(pub usize);

/// Marker for the label that applies the respec
#[derive(Component)]
pub struct ApplyRespecLabel;

/// Marker for the apply label's text (shows budget)
#[derive(Component)]
pub struct ApplyLabelText;

/// Resource to track character panel visibility and the respec in hand
#[derive(Resource, Default)]
pub struct CharacterPanelState {
    pub visible: bool,
    pub tab: PanelTab,
    /// The bag row the digits act on.
    pub bag_row: usize,
    /// The pair of the attributes tab the digits act on.
    pub pair: usize,
    /// The respec being drafted: present exactly while it differs from the
    /// pairs the character has.
    pub pending_respec: Option<[Pair; 3]>,
}

impl CharacterPanelState {
    pub fn has_pending_changes(&self) -> bool {
        self.pending_respec.is_some()
    }
}

pub const KEYCODE_CHARACTER_PANEL: KeyCode = KeyCode::KeyC;

/// Groups a pair's absolute, relative and commitment stat rows into one container
macro_rules! create_stat_section {
    ($parent:expr, $left_abs:expr, $right_abs:expr, $left_rel:expr, $right_rel:expr, $left_com:expr, $right_com:expr) => {
        $parent.spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(5.),
                width: Val::Px(375.),
                padding: UiRect::all(Val::Px(10.)),
                border_radius: BorderRadius::all(Val::Px(4.)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.15, 0.15, 0.15, 0.8)),
        ))
        .with_children(|section| {
            // Absolute row (label + value + effect)
            section.spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    column_gap: Val::Px(10.),
                    ..default()
                },
            ))
            .with_children(|row| {
                create_stat_display!(row, $left_abs);
                create_stat_display!(row, $right_abs);
            });

            // Relative row (label + value + effect)
            section.spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    column_gap: Val::Px(10.),
                    ..default()
                },
            ))
            .with_children(|row| {
                create_stat_display!(row, $left_rel);
                create_stat_display!(row, $right_rel);
            });

            // Commitment row (label + tier + what the tier gives)
            section.spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    column_gap: Val::Px(10.),
                    ..default()
                },
            ))
            .with_children(|row| {
                create_stat_display!(row, $left_com);
                create_stat_display!(row, $right_com);
            });
        });
    };
}

macro_rules! create_stat_display {
    ($parent:expr, $stat:expr) => {
        {
            let (name, color, effect_label) = match $stat {
                MetaAttributeStat::Force => ("Force", Color::srgb(0.9, 0.5, 0.5), "Auto-Attack Damage:"),
                MetaAttributeStat::Tempo => ("Tempo", Color::srgb(0.9, 0.9, 0.5), "Auto-Attack Speed:"),
                MetaAttributeStat::Constitution => ("Constitution", Color::srgb(0.5, 0.8, 0.5), "Health:"),
                MetaAttributeStat::Endurance => ("Endurance", Color::srgb(0.5, 0.7, 0.9), "Endurance Pool:"),
                MetaAttributeStat::Intuition => ("Leap", Color::srgb(0.7, 0.5, 0.9), "Distance:"),
                MetaAttributeStat::Concentration => ("Counter", Color::srgb(0.9, 0.6, 0.3), "Reflection:"),
                MetaAttributeStat::Impact => ("Impact", Color::srgb(0.9, 0.5, 0.5), "Recovery Pushback:"),
                MetaAttributeStat::Composure => ("Composure", Color::srgb(0.5, 0.7, 0.9), "Recovery Reduction:"),
                MetaAttributeStat::Flow => ("Flow", Color::srgb(0.9, 0.9, 0.5), "Combo Unlock:"),
                MetaAttributeStat::Reflex => ("Reflex", Color::srgb(0.7, 0.5, 0.9), "Reaction Window:"),
                MetaAttributeStat::Focus => ("Focus", Color::srgb(0.9, 0.6, 0.3), "Crit Chance:"),
                MetaAttributeStat::Toughness => ("Toughness", Color::srgb(0.5, 0.8, 0.5), "Crit Resisted:"),
                MetaAttributeStat::Ferocity => ("Ferocity", Color::srgb(0.9, 0.5, 0.5), "Early Combos:"),
                MetaAttributeStat::Grace => ("Grace", Color::srgb(0.9, 0.9, 0.5), "Strike Arc:"),
                MetaAttributeStat::Grit => ("Grit", Color::srgb(0.5, 0.8, 0.5), "Bank per Blow:"),
                MetaAttributeStat::Preparation => ("Preparation", Color::srgb(0.5, 0.7, 0.9), "Recovery Reactions:"),
                MetaAttributeStat::Patience => ("Patience", Color::srgb(0.7, 0.5, 0.9), "Waiting Refill:"),
                MetaAttributeStat::Awareness => ("Awareness", Color::srgb(0.9, 0.6, 0.3), "Reaction Span:"),
            };

            $parent.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(3.),
                    flex_grow: 1.0,
                    flex_basis: Val::Px(0.),
                    ..default()
                },
            ))
            .with_children(|stat_col| {
                // Header row: stat name left, raw value right
                stat_col.spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        justify_content: JustifyContent::SpaceBetween,
                        column_gap: Val::Px(4.),
                        ..default()
                    },
                ))
                .with_children(|header| {
                    // Stat name (colored)
                    header.spawn((
                        Text::new(name),
                        TextFont { font_size: FontSize::Px(11.0), ..default() },
                        TextColor(color),
                    ));
                    // Raw stat value
                    header.spawn((
                        $stat.clone(),
                        RawStatValue,
                        Text::new("(0)"),
                        TextFont { font_size: FontSize::Px(11.0), ..default() },
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
                        TextFont { font_size: FontSize::Px(10.0), ..default() },
                        TextColor(Color::srgb(0.8, 0.8, 0.8)),
                    ));
                    // Calculated value (marker for updates)
                    effect_row.spawn((
                        $stat,
                        Text::new("0"),
                        TextFont { font_size: FontSize::Px(11.0), ..default() },
                        TextColor(Color::srgb(1.0, 1.0, 1.0)),
                    ));
                });
            });
        }
    };
}

macro_rules! create_attribute_section {
    ($parent:expr, $pair:expr, $left_name:expr, $right_name:expr, $left_color:expr, $right_color:expr, $title_marker:expr, $current_marker:expr, $bar_marker:expr, $axis_marker:expr, $left_current_marker:expr, $right_current_marker:expr) => {
        $parent
        .spawn((
            PairSection($pair),
            Node {
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                flex_grow: 1.0,
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
                    TextFont { font_size: FontSize::Px(12.0), ..default() },
                    TextColor(Color::srgb(0.6, 0.6, 0.6)),
                ));
                // Left attribute name
                title_row.spawn((
                    Text::new($left_name),
                    TextFont { font_size: FontSize::Px(14.0), ..default() },
                    TextColor($left_color),
                ));
                // Right attribute name
                title_row.spawn((
                    Text::new($right_name),
                    TextFont { font_size: FontSize::Px(14.0), ..default() },
                    TextColor($right_color),
                ));
                // Right reach value (outer)
                title_row.spawn((
                    Text::new("0"),
                    TextFont { font_size: FontSize::Px(12.0), ..default() },
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
                // Left value container
                bar_row.spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(4.),
                        min_width: Val::Px(80.),  // Reserve space for the value
                        ..default()
                    },
                ))
                .with_children(|left_container| {
                    left_container.spawn((
                        $left_current_marker,
                        Text::new("0"),
                        Node {
                            flex_grow: 1.,
                            ..default()
                        },
                        TextFont { font_size: FontSize::Px(13.0), ..default() },
                        TextColor(Color::srgb(0.9, 0.9, 0.9)),
                        TextLayout::justify(Justify::Center),
                    ));
                });

                // Visual bar container
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
                            width: Val::Px(235.),
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

                        // Axis bar - shows current available range
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
                        ));
                    });
                });

                // Right value container
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
                    right_container.spawn((
                        $right_current_marker,
                        Text::new("0"),
                        Node {
                            flex_grow: 1.,
                            ..default()
                        },
                        TextFont { font_size: FontSize::Px(13.0), ..default() },
                        TextColor(Color::srgb(0.9, 0.9, 0.9)),
                        TextLayout::justify(Justify::Center),
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

    // The panel: a bordered frame round a strip of tabs and the chosen tab's
    // content, which the open tab joins in the content's own colour.
    let panel = commands
        .spawn((
            CharacterPanel,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(20.),
                top: Val::Px(100.),
                flex_direction: FlexDirection::Row,
                padding: UiRect::all(Val::Px(12.)),
                border: UiRect::all(Val::Px(1.)),
                border_radius: BorderRadius::all(Val::Px(8.)),
                ..default()
            },
            BackgroundColor(FRAME),
            BorderColor::all(PANE_EDGE),
            Visibility::Hidden,
        ))
        .id();
    spawn_tab_strip(&mut commands, panel);
    let content = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                width: Val::Px(PANE_WIDTH),
                flex_shrink: 0.0,
                padding: UiRect::new(Val::Px(20.), Val::Px(20.), Val::Px(20.), Val::Px(10.)),
                border_radius: BorderRadius::all(Val::Px(8.)),
                ..default()
            },
            BackgroundColor(PANE),
            ChildOf(panel),
        ))
        .id();
    let attributes = commands
        .spawn((
            TabContent(PanelTab::Attributes),
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(15.),
                ..default()
            },
            ChildOf(content),
        ))
        .id();
    equipment_panel::spawn_tab(&mut commands, content);
    bag_panel::spawn_tab(&mut commands, content);

    commands
        .entity(attributes)
        .with_children(|parent| {
            // One row to a pair: its sliders, and the stats its two attributes
            // give, so the two stay level however tall the stats grow.
            parent.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(15.),
                    ..default()
                },
            ))
            .with_children(|main| {
                let pair_row = || Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(20.), ..default() };

                // MIGHT ↔ AGILITY (Impact = red, Flow = yellow)
                main.spawn(pair_row()).with_children(|pair| {
                    create_attribute_section!(pair, 0, "MIGHT", "AGILITY",
                        Color::srgb(0.9, 0.5, 0.5), Color::srgb(0.9, 0.9, 0.5),
                        AttributeTitle::MightAgility, AttributeCurrent::MightAgility, AttributeBar::MightAgility, AxisMarker::MightAgility,
                        LeftCurrentValue::MightAgility, RightCurrentValue::MightAgility);
                    create_stat_section!(pair,
                        MetaAttributeStat::Force, MetaAttributeStat::Tempo,
                        MetaAttributeStat::Impact, MetaAttributeStat::Flow,
                        MetaAttributeStat::Ferocity, MetaAttributeStat::Grace);
                });

                // VITALITY ↔ DISCIPLINE (Toughness = green, Composure = blue)
                main.spawn(pair_row()).with_children(|pair| {
                    create_attribute_section!(pair, 1, "VITALITY", "DISCIPLINE",
                        Color::srgb(0.5, 0.8, 0.5), Color::srgb(0.5, 0.7, 0.9),
                        AttributeTitle::VitalityDiscipline, AttributeCurrent::VitalityDiscipline, AttributeBar::VitalityDiscipline, AxisMarker::VitalityDiscipline,
                        LeftCurrentValue::VitalityDiscipline, RightCurrentValue::VitalityDiscipline);
                    create_stat_section!(pair,
                        MetaAttributeStat::Constitution, MetaAttributeStat::Endurance,
                        MetaAttributeStat::Toughness, MetaAttributeStat::Composure,
                        MetaAttributeStat::Grit, MetaAttributeStat::Preparation);
                });

                // INSTINCT ↔ RESOLVE (Reflex = purple, Focus = orange)
                main.spawn(pair_row()).with_children(|pair| {
                    create_attribute_section!(pair, 2, "INSTINCT", "RESOLVE",
                        Color::srgb(0.7, 0.5, 0.9), Color::srgb(0.9, 0.6, 0.3),
                        AttributeTitle::InstinctResolve, AttributeCurrent::InstinctResolve, AttributeBar::InstinctResolve, AxisMarker::InstinctResolve,
                        LeftCurrentValue::InstinctResolve, RightCurrentValue::InstinctResolve);
                    create_stat_section!(pair,
                        MetaAttributeStat::Intuition, MetaAttributeStat::Concentration,
                        MetaAttributeStat::Reflex, MetaAttributeStat::Focus,
                        MetaAttributeStat::Patience, MetaAttributeStat::Awareness);
                });
            });

            use crate::systems::keycap::{hint_row, keycap, Hint};
            // The keys are laid out as a pair is drawn: spectrum over its
            // bar, axis at its ends, shift along it.
            hint_row(parent, None, &[
                Hint::either(&[KeyCode::Numpad7, KeyCode::Numpad9], "spectrum -/+"),
                Hint::either(&[KeyCode::Numpad4, KeyCode::Numpad6], "axis left/right"),
                Hint::either(&[KeyCode::Numpad1, KeyCode::Numpad3], "shift left/right"),
            ]);
            parent
                .spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: Val::Px(12.), ..default() })
                .with_children(|row| {
                    hint_row(row, None, &[Hint::key(KeyCode::NumpadDecimal, "next pair")]);
                    // Hidden until a respec is in hand.
                    row.spawn((
                        ApplyRespecLabel,
                        Node {
                            height: Val::Px(30.),
                            padding: UiRect::horizontal(Val::Px(10.)),
                            column_gap: Val::Px(8.),
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.3, 0.7, 0.3)),
                        Visibility::Hidden,
                    ))
                    .with_children(|label| {
                        keycap(label, KeyCode::NumpadEnter);
                        label.spawn((
                            ApplyLabelText,
                            Text::new("Apply Changes"),
                            TextFont { font_size: FontSize::Px(14.0), ..default() },
                        ));
                    });
                });
        });
}

/// Toggle panel visibility when 'C' key is pressed
pub fn toggle_panel(
    mut keys: crate::systems::help::Keys,
    mut state: ResMut<CharacterPanelState>,
    mut query: Query<&mut Visibility, With<CharacterPanel>>,
) {
    if keys.pressed(KEYCODE_CHARACTER_PANEL, "Open or close the character panel") {
        if let Ok(mut visibility) = query.single_mut() {
            if state.visible {
                close(&mut state, &mut visibility);
            } else {
                state.visible = true;
                *visibility = Visibility::Visible;
            }
        }
    }
}

/// Closes the panel, dropping any respec not yet applied.
pub fn close(state: &mut CharacterPanelState, visibility: &mut Visibility) {
    state.visible = false;
    state.pending_respec = None;
    *visibility = Visibility::Hidden;
}

const PANE_WIDTH: f32 = 860.0;
const PANE: Color = Color::srgba(0.1, 0.1, 0.1, 0.9);
const PANE_EDGE: Color = Color::srgb(0.4, 0.4, 0.4);
/// The frame round the strip and the content; a shut tab is its colour.
const FRAME: Color = Color::srgba(0.04, 0.04, 0.04, 0.95);

/// A tab's frame in the strip. A shut tab is bordered all round in the
/// frame's colour; the open one takes the content's colour, loses its right
/// border and reaches under the content, so nothing parts the two.
#[derive(Component)]
pub struct TabFrame(pub PanelTab);

fn spawn_tab_strip(commands: &mut Commands, panel: Entity) {
    commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.),
                width: Val::Px(120.),
                padding: UiRect::vertical(Val::Px(12.)),
                ..default()
            },
            ChildOf(panel),
        ))
        .with_children(|strip| {
            for tab in PanelTab::ALL {
                strip
                    .spawn((
                        TabFrame(tab),
                        Node {
                            justify_content: JustifyContent::FlexEnd,
                            padding: UiRect::axes(Val::Px(12.), Val::Px(8.)),
                            border: UiRect::all(Val::Px(1.)),
                            border_radius: BorderRadius::left(Val::Px(6.)),
                            ..default()
                        },
                        BackgroundColor(FRAME),
                        BorderColor::all(PANE_EDGE),
                    ))
                    .with_children(|frame| {
                        frame.spawn((
                            TabLabel(tab),
                            Text::new(tab.name()),
                            TextFont { font_size: FontSize::Px(14.0), ..default() },
                            TextColor(Color::srgb(0.6, 0.6, 0.6)),
                        ));
                    });
            }
            use crate::systems::keycap::{hint_column, Hint};
            hint_column(
                strip,
                &[Hint::key(KeyCode::NumpadSubtract, "prev"), Hint::key(KeyCode::NumpadAdd, "next"), Hint::key(KEYCODE_CHARACTER_PANEL, "close")],
                Node { margin: UiRect::new(Val::Px(0.), Val::Px(8.), Val::Px(8.), Val::Px(0.)), align_self: AlignSelf::FlexEnd, ..default() },
            );
        });
}

/// Shows the chosen tab's content, opens its frame into the pane and
/// marks its label.
pub fn update_tabs(
    state: Res<CharacterPanelState>,
    mut contents: Query<(&TabContent, &mut Node), Without<TabFrame>>,
    mut frames: Query<(&TabFrame, &mut Node, &mut BackgroundColor, &mut BorderColor)>,
    mut labels: Query<(&TabLabel, &mut TextColor)>,
) {
    if !state.is_changed() {
        return;
    }
    for (content, mut node) in &mut contents {
        node.display = if content.0 == state.tab { Display::Flex } else { Display::None };
    }
    for (frame, mut node, mut background, mut border) in &mut frames {
        let open = frame.0 == state.tab;
        // The open frame reaches under the content, in the content's colour.
        node.margin.right = if open { Val::Px(-2.) } else { Val::Px(0.) };
        node.border.right = if open { Val::Px(0.) } else { Val::Px(1.) };
        *background = BackgroundColor(if open { PANE } else { FRAME });
        *border = BorderColor::all(PANE_EDGE);
    }
    for (label, mut color) in &mut labels {
        let open = label.0 == state.tab;
        *color = TextColor(if open { Color::srgb(0.95, 0.85, 0.5) } else { Color::srgb(0.6, 0.6, 0.6) });
    }
}

/// Update attribute text and bar visuals when panel is visible
pub fn update_attributes(
    tuning: Res<Tuning>,
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
        temp_attrs.apply_respec(*draft);
        Some(temp_attrs)
    } else {
        None
    };
    let display_attrs = draft_attrs.as_ref().unwrap_or(attrs);

    // The bar spans the actual character's ceiling, not the draft's, so its
    // scale holds still while a draft is laid out
    let max_attr_scaled = attrs.ceiling() as i16;

    // Update title rows (reach values)
    for (title_entity, attr_type) in &title_query {
        let (left_reach, right_reach) = match attr_type {
            AttributeTitle::MightAgility => (display_attrs.might_reach(), display_attrs.agility_reach()),
            AttributeTitle::VitalityDiscipline => (display_attrs.vitality_reach(), display_attrs.discipline_reach()),
            AttributeTitle::InstinctResolve => (display_attrs.instinct_reach(), display_attrs.resolve_reach()),
        };

        // Update the reach text values (first and last child)
        if let Ok(title_children) = children.get(title_entity) {
            if title_children.len() >= 4 {
                // First child: left reach value
                if let Ok(mut text) = text_query.get_mut(title_children[0]) {
                    **text = format!("{}", left_reach);
                }
                // Last child: right reach value
                if let Ok(mut text) = text_query.get_mut(title_children[3]) {
                    **text = format!("{}", right_reach);
                }
            }
        }
    }

    // Update left current values
    for (left_entity, attr_type) in &left_value_query {
        let left_current = match attr_type {
            LeftCurrentValue::MightAgility => display_attrs.might(),
            LeftCurrentValue::VitalityDiscipline => display_attrs.vitality(),
            LeftCurrentValue::InstinctResolve => display_attrs.instinct(),
        };

        if let Ok(mut text) = text_query.get_mut(left_entity) {
            **text = format!("{}", left_current);
        }
    }

    // Update right current values
    for (right_entity, attr_type) in &right_value_query {
        let right_current = match attr_type {
            RightCurrentValue::MightAgility => display_attrs.agility(),
            RightCurrentValue::VitalityDiscipline => display_attrs.discipline(),
            RightCurrentValue::InstinctResolve => display_attrs.resolve(),
        };

        if let Ok(mut text) = text_query.get_mut(right_entity) {
            **text = format!("{}", right_current);
        }
    }

    // Update bar visuals
    for (bar_entity, bar_type) in &bar_query {
        let (left_reach, right_reach, left_current, right_current) = match bar_type {
            AttributeBar::MightAgility => (
                display_attrs.might_reach(),
                display_attrs.agility_reach(),
                display_attrs.might(),
                display_attrs.agility(),
            ),
            AttributeBar::VitalityDiscipline => (
                display_attrs.vitality_reach(),
                display_attrs.discipline_reach(),
                display_attrs.vitality(),
                display_attrs.discipline(),
            ),
            AttributeBar::InstinctResolve => (
                display_attrs.instinct_reach(),
                display_attrs.resolve_reach(),
                display_attrs.instinct(),
                display_attrs.resolve(),
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

    // Update meta-attribute raw values and calculated effects
    for (meta_stat, entity) in &meta_query {
        if let Ok(mut text) = text_query.get_mut(entity) {
            // Check if this is a raw value display (starts with '(')
            let is_raw = text.0.starts_with('(');

            if is_raw {
                // Update raw stat value in parentheses
                let raw_value = match meta_stat {
                    MetaAttributeStat::Force => display_attrs.might().to_string(),
                    MetaAttributeStat::Tempo => display_attrs.agility().to_string(),
                    MetaAttributeStat::Constitution => display_attrs.vitality().to_string(),
                    MetaAttributeStat::Endurance => display_attrs.discipline().to_string(),
                    MetaAttributeStat::Intuition => display_attrs.instinct().to_string(),
                    MetaAttributeStat::Concentration => display_attrs.resolve().to_string(),
                    MetaAttributeStat::Impact => display_attrs.impact().to_string(),
                    MetaAttributeStat::Composure => display_attrs.composure().to_string(),
                    MetaAttributeStat::Flow => display_attrs.flow().to_string(),
                    MetaAttributeStat::Reflex => display_attrs.reflex().to_string(),
                    MetaAttributeStat::Focus => display_attrs.focus().to_string(),
                    MetaAttributeStat::Toughness => display_attrs.toughness().to_string(),
                    MetaAttributeStat::Ferocity => format!("T{}", display_attrs.ferocity().index()),
                    MetaAttributeStat::Grace => format!("T{}", display_attrs.grace().index()),
                    MetaAttributeStat::Grit => format!("T{}", display_attrs.grit().index()),
                    MetaAttributeStat::Preparation => format!("T{}", display_attrs.preparation().index()),
                    MetaAttributeStat::Patience => format!("T{}", display_attrs.patience().index()),
                    MetaAttributeStat::Awareness => format!("T{}", display_attrs.awareness().index()),
                };
                **text = format!("({})", raw_value);
            } else {
                // Update calculated effect value (uncontested display)
                **text = match meta_stat {
                    // An absolute's effect is how much more its points make
                    // what the fight reads it as than an actor of its level
                    // with none
                    MetaAttributeStat::Force => increase(display_attrs.auto_damage(&tuning), display_attrs.base_potency(&tuning) * tuning.auto_damage),
                    MetaAttributeStat::Tempo => increase(tuning.base_interval, display_attrs.cadence_interval(&tuning).as_secs_f32()),
                    MetaAttributeStat::Constitution => increase(display_attrs.constitution(&tuning), tuning.base_health * display_attrs.hp_level_multiplier(&tuning)),
                    MetaAttributeStat::Endurance => increase(display_attrs.endurance(&tuning), display_attrs.base_potency(&tuning)),
                    // Instinct's and Resolve's own: the skill each line raises
                    MetaAttributeStat::Intuition => increase(display_attrs.line_power(&tuning, common_bevy::message::AbilityType::Leap), 1.0),
                    MetaAttributeStat::Concentration => increase(display_attrs.line_power(&tuning, common_bevy::message::AbilityType::Counter), 1.0),
                    MetaAttributeStat::Impact => {
                        // Recovery pushback: 0.50 × gap × contest_factor
                        let impact = display_attrs.impact();
                        let contest = contest_factor(&tuning, impact, 0, 0.0);  // vs 0 composure
                        let pushback_pct = (0.50 * contest) * 100.0;
                        format!("+{:.0}%", pushback_pct)
                    },
                    MetaAttributeStat::Composure => {
                        // Recovery time reduction: 0.33 × gap × contest_factor
                        let composure = display_attrs.composure();
                        let contest = contest_factor(&tuning, composure, 0, 0.0);  // vs 0 impact
                        let reduction_pct = (0.33 * contest) * 100.0;
                        format!("-{:.0}%", reduction_pct)
                    },
                    MetaAttributeStat::Flow => {
                        // Combo unlock: 0.66 × gap × contest_factor
                        let flow = display_attrs.flow();
                        let contest = contest_factor(&tuning, flow, 0, 0.0);  // vs 0 reflex
                        let reduction_pct = (0.66 * contest) * 100.0;
                        format!("-{:.0}%", reduction_pct)
                    },
                    MetaAttributeStat::Reflex => {
                        // Reaction window: 3.0s × (1.0 + 0.5 × contest_factor)
                        // Display raw time value (different pattern from other stats)
                        let reflex = display_attrs.reflex();
                        let contest = contest_factor(&tuning, reflex, 0, 0.0);  // vs 0 flow
                        let multiplier = 1.0 + 0.5 * contest;
                        let window_seconds = 3.0 * multiplier;
                        format!("{:.1}s", window_seconds)
                    },
                    MetaAttributeStat::Focus => {
                        // Crit chance on a target of its level with no Toughness
                        let contest = contest_factor(&tuning, display_attrs.focus(), 0, 0.0);
                        format!("{:.0}%", tuning.crit_chance * contest * 100.0)
                    },
                    MetaAttributeStat::Toughness => {
                        // The Focus it cancels: an attacker crits it only with more
                        display_attrs.toughness().to_string()
                    },
                    // A commitment's effect is what its tier gives, from the
                    // same methods the fight reads.
                    MetaAttributeStat::Ferocity => display_attrs.ferocity().index().to_string(),
                    MetaAttributeStat::Grace => format!("+/-{:.0} deg", display_attrs.arc(&tuning)),
                    MetaAttributeStat::Grit => display_attrs.grit_fill().to_string(),
                    MetaAttributeStat::Preparation => display_attrs.preparation().index().to_string(),
                    MetaAttributeStat::Patience => format!("+{:.0}%", display_attrs.patience_regen(&tuning) * 100.0),
                    MetaAttributeStat::Awareness => format!("{:.2}s", display_attrs.span(&tuning).as_secs_f32()),
                };
            }
        }
    }
}

/// How much more `value` is than `base`, as a percentage
fn increase(value: f32, base: f32) -> String {
    format!("+{:.0}%", (value / base - 1.0) * 100.0)
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

    // For might_agility with axis=-2, spectrum=3:
    //   might_reach=41 (20+21) at position -41 on the scale
    //   agility_reach=21 at position +21 on the scale

    // For instinct_resolve with axis=0, spectrum=3:
    //   instinct_reach=21 at position -21
    //   resolve_reach=21 at position +21

    // The bar should show from the leftmost reach to the rightmost reach

    // Left reach is on the negative side (might, vitality, instinct)
    let left_bound = -(left_reach as i16);
    // Right reach is on the positive side (agility, discipline, resolve)
    let right_bound = right_reach as i16;

    let left_percent = attr_to_percent(left_bound, max_attr_scaled);
    let right_percent = attr_to_percent(right_bound, max_attr_scaled);
    let width_percent = right_percent - left_percent;

    node.left = Val::Percent(left_percent);
    node.width = Val::Percent(width_percent);
}

fn update_axis_bar(node: &mut Node, left_current: u16, right_current: u16, max_attr_scaled: i16) {
    // The yellow bar shows the current available values on each side
    // For might_agility: might=250, agility=50 (scaled values)
    //   Left bound at -250 (might value, scaled)
    //   Right bound at +50 (agility value, scaled)

    let left_bound = -(left_current as i16);
    let right_bound = right_current as i16;

    let left_percent = attr_to_percent(left_bound, max_attr_scaled);
    let right_percent = attr_to_percent(right_bound, max_attr_scaled);
    let width_percent = right_percent - left_percent;

    node.left = Val::Percent(left_percent);
    node.width = Val::Percent(width_percent);
}

/// Writes the apply label's budget counter, and colours it red while levels
/// are left to put in, which Enter will not apply without.
pub fn update_apply_label(
    state: Res<CharacterPanelState>,
    player_query: Query<&ActorAttributes, With<Actor>>,
    mut label_query: Query<(&mut BackgroundColor, &Children), With<ApplyRespecLabel>>,
    mut text_query: Query<&mut Text, With<ApplyLabelText>>,
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

    let Ok((mut bg_color, children)) = label_query.single_mut() else {
        return;
    };

    let unallocated = attrs.total_level().saturating_sub(ActorAttributes::invested(draft));

    for child in children.iter() {
        if let Ok(mut text) = text_query.get_mut(child) {
            if unallocated > 0 {
                **text = format!("Apply ({} points left)", unallocated);
                *bg_color = BackgroundColor(Color::srgb(0.6, 0.3, 0.3));
            } else {
                **text = "Apply Changes".to_string();
                *bg_color = BackgroundColor(Color::srgb(0.3, 0.7, 0.3));
            }
        }
    }
}

/// Marks the pair the digits act on: its section takes the cursor's colour,
/// as a bag's cursor row does.
pub fn update_pair_cursor(state: Res<CharacterPanelState>, mut sections: Query<(&PairSection, &mut BackgroundColor)>) {
    if !state.is_changed() {
        return;
    }
    for (section, mut background) in &mut sections {
        *background = BackgroundColor(if section.0 == state.pair { CURSOR_ROW } else { OTHER_ROW });
    }
}

