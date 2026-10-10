use bevy::prelude::*;

use common_bevy::{
    components::{Actor, ActorAttributes, Attribute, Pair, Unlock},
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
    PhysiqueDiscipline,
    InstinctResolve,
}

/// Marker for left current value text
#[derive(Component)]
pub enum LeftCurrentValue {
    MightAgility,
    PhysiqueDiscipline,
    InstinctResolve,
}

/// Marker for right current value text
#[derive(Component)]
pub enum RightCurrentValue {
    MightAgility,
    PhysiqueDiscipline,
    InstinctResolve,
}

/// Marker component for the visual attribute bar
#[derive(Component, Debug)]
pub enum AttributeBar {
    MightAgility,
    PhysiqueDiscipline,
    InstinctResolve,
}

/// Marker for the spectrum range indicator within the bar
#[derive(Component)]
pub struct SpectrumRange;

/// Marker for the axis position indicator (yellow bar)
#[derive(Component)]
pub enum AxisMarker {
    MightAgility,
    PhysiqueDiscipline,
    InstinctResolve,
}

/// A stat a pair's attributes give: its absolute, which both share, and
/// each attribute's contest and commitment
#[derive(Clone, Copy)]
pub enum Stat {
    Force,
    Constitution,
    Endurance,
    Impact,
    Tempo,
    Fitness,
    Efficiency,
    Reflex,
    Focus,
    Ferocity,
    Grace,
    Intimidation,
    Preparation,
    Patience,
    Awareness,
}

impl Stat {
    pub fn name(self) -> &'static str {
        match self {
            Stat::Force => "Force",
            Stat::Constitution => "Constitution",
            Stat::Endurance => "Endurance",
            Stat::Impact => "Impact",
            Stat::Tempo => "Tempo",
            Stat::Fitness => "Fitness",
            Stat::Efficiency => "Efficiency",
            Stat::Reflex => "Reflex",
            Stat::Focus => "Focus",
            Stat::Ferocity => "Ferocity",
            Stat::Grace => "Grace",
            Stat::Intimidation => "Intimidation",
            Stat::Preparation => "Preparation",
            Stat::Patience => "Patience",
            Stat::Awareness => "Awareness",
        }
    }
}

/// A stat's cell beside its pair's sliders, which says what the stat gives
/// while the pointer rests on it (`help::Describes`)
#[derive(Component)]
pub struct StatCell(pub Stat);

/// The value shown in a stat's cell
#[derive(Component)]
pub struct StatValue(pub Stat);

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

/// A pair's stats beside its sliders: a column to each attribute, its name
/// over the three stats it gives, absolute, contest and commitment; each
/// cell its stat's name and value, and what it gives while the pointer
/// rests on it
macro_rules! create_stat_section {
    ($parent:expr, $(($name:expr, $color:expr, [$($stat:expr),+])),+) => {
        $parent.spawn((
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(16.),
                width: Val::Px(375.),
                padding: UiRect::all(Val::Px(10.)),
                border_radius: BorderRadius::all(Val::Px(4.)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.15, 0.15, 0.15, 0.8)),
        ))
        .with_children(|section| {
            $(
                section.spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(4.), flex_grow: 1.0, flex_basis: Val::Px(0.), ..default() })
                    .with_children(|column| {
                        column.spawn((
                            Text::new($name),
                            TextFont { font_size: FontSize::Px(14.0), ..default() },
                            TextColor($color),
                            Node { margin: UiRect::bottom(Val::Px(2.)), ..default() },
                        ));
                        $(create_stat_cell!(column, $stat, $color);)+
                    });
            )+
        });
    };
}

macro_rules! create_stat_cell {
    ($parent:expr, $stat:expr, $color:expr) => {
        $parent.spawn((
            StatCell($stat),
            crate::systems::help::Describes::default(),
            Node {
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                flex_grow: 1.0,
                flex_basis: Val::Px(0.),
                padding: UiRect::axes(Val::Px(4.), Val::Px(2.)),
                ..default()
            },
        ))
        .with_children(|cell| {
            // The cell takes the pointer, not its text
            cell.spawn((
                Text::new($stat.name()),
                TextFont { font_size: FontSize::Px(12.0), ..default() },
                TextColor($color),
                Pickable::IGNORE,
            ));
            cell.spawn((
                StatValue($stat),
                Text::new("0"),
                TextFont { font_size: FontSize::Px(12.0), ..default() },
                TextColor(Color::srgb(0.85, 0.85, 0.85)),
                Pickable::IGNORE,
            ));
        });
    };
}

macro_rules! create_attribute_section {
    ($parent:expr, $pair:expr, $left_name:expr, $right_name:expr, $left_color:expr, $right_color:expr, $title_marker:expr, $bar_marker:expr, $axis_marker:expr, $left_current_marker:expr, $right_current_marker:expr) => {
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
            // One row to a pair: its sliders, and the stats its two
            // attributes give, each saying what it gives while the pointer
            // rests on it.
            parent.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(15.),
                    ..default()
                },
            ))
            .with_children(|main| {
                let pair_row = || Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(20.), ..default() };

                // MIGHT ↔ AGILITY
                main.spawn(pair_row()).with_children(|pair| {
                    create_attribute_section!(pair, 0, "MIGHT", "AGILITY",
                        Color::srgb(0.9, 0.5, 0.5), Color::srgb(0.9, 0.9, 0.5),
                        AttributeTitle::MightAgility, AttributeBar::MightAgility, AxisMarker::MightAgility,
                        LeftCurrentValue::MightAgility, RightCurrentValue::MightAgility);
                    create_stat_section!(pair,
                        ("Might", Color::srgb(0.9, 0.5, 0.5), [Stat::Force, Stat::Impact, Stat::Ferocity]),
                        ("Agility", Color::srgb(0.9, 0.9, 0.5), [Stat::Force, Stat::Tempo, Stat::Grace]));
                });

                // PHYSIQUE ↔ DISCIPLINE
                main.spawn(pair_row()).with_children(|pair| {
                    create_attribute_section!(pair, 1, "PHYSIQUE", "DISCIPLINE",
                        Color::srgb(0.5, 0.8, 0.5), Color::srgb(0.5, 0.7, 0.9),
                        AttributeTitle::PhysiqueDiscipline, AttributeBar::PhysiqueDiscipline, AxisMarker::PhysiqueDiscipline,
                        LeftCurrentValue::PhysiqueDiscipline, RightCurrentValue::PhysiqueDiscipline);
                    create_stat_section!(pair,
                        ("Physique", Color::srgb(0.5, 0.8, 0.5), [Stat::Constitution, Stat::Fitness, Stat::Intimidation]),
                        ("Discipline", Color::srgb(0.5, 0.7, 0.9), [Stat::Constitution, Stat::Efficiency, Stat::Preparation]));
                });

                // INSTINCT ↔ RESOLVE
                main.spawn(pair_row()).with_children(|pair| {
                    create_attribute_section!(pair, 2, "INSTINCT", "RESOLVE",
                        Color::srgb(0.7, 0.5, 0.9), Color::srgb(0.9, 0.6, 0.3),
                        AttributeTitle::InstinctResolve, AttributeBar::InstinctResolve, AxisMarker::InstinctResolve,
                        LeftCurrentValue::InstinctResolve, RightCurrentValue::InstinctResolve);
                    create_stat_section!(pair,
                        ("Instinct", Color::srgb(0.7, 0.5, 0.9), [Stat::Endurance, Stat::Reflex, Stat::Patience]),
                        ("Resolve", Color::srgb(0.9, 0.6, 0.3), [Stat::Endurance, Stat::Focus, Stat::Awareness]));
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
    mut cells: Query<(&StatCell, &mut crate::systems::help::Describes)>,
    values: Query<(Entity, &StatValue)>,
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
            AttributeTitle::PhysiqueDiscipline => (display_attrs.physique_reach(), display_attrs.discipline_reach()),
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
            LeftCurrentValue::PhysiqueDiscipline => display_attrs.physique(),
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
            RightCurrentValue::PhysiqueDiscipline => display_attrs.discipline(),
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
            AttributeBar::PhysiqueDiscipline => (
                display_attrs.physique_reach(),
                display_attrs.discipline_reach(),
                display_attrs.physique(),
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
                    place_span(&mut node, left_reach, right_reach, max_attr_scaled);
                }
                // Update axis bar (yellow bar - shows current available values)
                if let Ok((_, mut node)) = axis_query.get_mut(child) {
                    place_span(&mut node, left_current, right_current, max_attr_scaled);
                }
            }
        }
    }

    for (cell, mut describes) in &mut cells {
        let now = describe(display_attrs, cell.0, &tuning);
        if describes.0 != now {
            describes.0 = now;
        }
    }
    for (entity, value) in &values {
        if let Ok(mut text) = text_query.get_mut(entity) {
            let now = shown(display_attrs, value.0);
            if text.0 != now {
                text.0 = now;
            }
        }
    }
}

/// What `stat` shows in its cell: the pair's points for its absolute, the
/// attribute's value for a contest, a commitment's tier
fn shown(attrs: &ActorAttributes, stat: Stat) -> String {
    use Stat::*;
    match stat {
        Force => attrs.pair_points(Attribute::Might).to_string(),
        Constitution => attrs.pair_points(Attribute::Physique).to_string(),
        Endurance => attrs.pair_points(Attribute::Instinct).to_string(),
        Impact => attrs.might().to_string(),
        Tempo => attrs.agility().to_string(),
        Fitness => attrs.physique().to_string(),
        Efficiency => attrs.discipline().to_string(),
        Reflex => attrs.instinct().to_string(),
        Focus => attrs.resolve().to_string(),
        Ferocity => format!("T{}", attrs.ferocity().index()),
        Grace => format!("T{}", attrs.grace().index()),
        Intimidation => format!("T{}", attrs.intimidation().index()),
        Preparation => format!("T{}", attrs.preparation().index()),
        Patience => format!("T{}", attrs.patience().index()),
        Awareness => format!("T{}", attrs.awareness().index()),
    }
}

/// What `stat` gives an actor with `attrs`, by the numbers the fight
/// reads; a contest as it stands against a foe of its level with none of
/// what contests it
fn describe(attrs: &ActorAttributes, stat: Stat, tuning: &Tuning) -> String {
    let percent = |share: f32| format!("{:.0}%", share * 100.0);
    let less = |discount: f32| if discount > 0.0 { format!(", {} less", percent(discount)) } else { String::new() };
    let contest = |stat: u16| contest_factor(tuning, stat, 0, 0.0);
    let against = "\nAgainst a foe of its level with none of what contests it.";
    match stat {
        Stat::Force => format!("Force, the absolute of Might and Agility together: its auto-attacks strike {} harder.", percent(attrs.auto_damage(tuning) / (attrs.base_potency(tuning) * tuning.auto_damage) - 1.0)),
        Stat::Constitution => format!("Constitution, the absolute of Physique and Discipline together: {} more health.", percent(attrs.constitution(tuning) / tuning.base_health - 1.0)),
        Stat::Endurance => format!("Endurance, the absolute of Instinct and Resolve together: an endurance pool {} deeper.", percent(attrs.endurance(tuning) - 1.0)),
        Stat::Impact => format!("Impact, Might's contest, against a foe's Efficiency: each blow pushes its recovery back {}.{against}", percent(tuning.pushback_share * contest(attrs.impact()))),
        Stat::Tempo => format!("Tempo, Agility's contest, against a foe's Reflex: its auto-attacks come {} sooner.{against}", percent(1.0 - attrs.cadence_interval(tuning, None).as_secs_f32() / tuning.base_interval)),
        Stat::Fitness => format!("Fitness, Physique's contest, against a foe's Focus: its recovery runs {} shorter, and a foe's Focus crits it less.{against}", percent(tuning.fitness_share * contest(attrs.fitness()))),
        Stat::Efficiency => format!("Efficiency, Discipline's contest, against a foe's Impact: its combos unlock {} sooner.{against}", percent(tuning.combo_share * contest(attrs.efficiency()))),
        Stat::Reflex => format!("Reflex, Instinct's contest, against a foe's Tempo: a threat on it waits {:.1}s to land, time to act and answer in.{against}", tuning.reaction_window * (1.0 + tuning.window_bonus * contest(attrs.reflex()))),
        Stat::Focus => format!("Focus, Resolve's contest, against a foe's Fitness: its blows crit {} of the time.{against}", percent(tuning.crit_chance * contest(attrs.focus()))),
        Stat::Ferocity => {
            let third = attrs.ferocity().at(Unlock::Capstone, tuning.ferocity_third)
                .map_or(String::new(), |third| format!(" The third owes {} less.", percent(third)));
            format!("Ferocity, Might's commitment: up to {} combos in a chain fire before they unlock, each owing the chain the time it skipped{}.{third}",
                attrs.early_combos(), less(attrs.early_discount(tuning, Attribute::Might, 0)))
        }
        Stat::Grace => {
            let stride = attrs.flank_stride(tuning)
                .map_or(String::new(), |pace| format!(" A strike from a foe's flank breaks its stride, holding it to {} of its speed for a swing.", percent(pace)));
            format!("Grace, Agility's commitment: its strikes land {} harder from a foe's flank, and it strikes within {:.0} degrees either side of its heading.{stride}",
                percent(attrs.flank(tuning)), attrs.arc(tuning))
        }
        Stat::Intimidation => match attrs.intimidation_pace(tuning) {
            None => "Intimidation, Physique's commitment: none yet. Its first tier slows foes in its reach.".to_string(),
            Some(pace) => {
                let toll = attrs.intimidation_toll(tuning);
                let toll = if toll > 0.0 { format!(", and each skill they use there costs {} more", percent(toll)) } else { String::new() };
                let pin = attrs.intimidation_zone(tuning)
                    .map_or(String::new(), |zone| format!(" They use no skill that moves them there, and its zone reaches {zone} tiles past its reach."));
                format!("Intimidation, Physique's commitment: foes in its reach move at {} of their speed{toll}.{pin}", percent(pace))
            }
        },
        Stat::Preparation => {
            let slip = attrs.slip(tuning)
                .map_or(String::new(), |tiles| format!(" Each carries it {tiles} tiles away from what it answered."));
            format!("Preparation, Discipline's commitment: up to {} reactions in a chain fire early after a strike, a Leap clear among them, each owing the chain the time it skipped{}.{slip}",
                attrs.early_reactions(), less(attrs.early_discount(tuning, Attribute::Discipline, 0)))
        }
        Stat::Patience => {
            let power = attrs.patience_power(tuning);
            let power = if power > 0.0 { format!(", and land {} harder", percent(power)) } else { String::new() };
            let opening = attrs.patience_opening(tuning)
                .map_or(String::new(), |stacks| format!(" At {stacks} stacks its next skill on that foe crits for certain, spending them."));
            format!("Patience, Instinct's commitment: each attack made at it overcommits its attacker, and its skills crit {} likelier for each stack a foe carries{power}.{opening}",
                percent(attrs.patience_crit(tuning)))
        }
        Stat::Awareness => {
            let refund = attrs.awareness_refund(tuning);
            let refund = if refund > 0.0 { format!(" Each threat one answer takes past the first pays back {} of its price.", percent(refund)) } else { String::new() };
            let snap = attrs.awareness_snap(tuning)
                .map_or(String::new(), |snap| format!(" Pressed up to {:.1}s further ahead, its band starts at the next threat.", snap.as_secs_f32()));
            format!("Awareness, Resolve's commitment: a reaction takes every threat landing within {:.2}s of its press.{refund}{snap}", attrs.span(tuning).as_secs_f32())
        }
    }
}

/// Where a value from `-ceiling` to `+ceiling` sits along the bar, as a
/// percentage of its width.
fn attr_to_percent(value: i16, ceiling: i16) -> f32 {
    let range = ceiling as f32 * 2.0;
    ((value as f32 + ceiling as f32) / range * 100.0).clamp(0.0, 100.0)
}

/// Lays `node` over the bar from `left` on the negative side (might,
/// physique, instinct) to `right` on the positive (agility, discipline,
/// resolve), on a bar reaching `ceiling` each way.
fn place_span(node: &mut Node, left: u16, right: u16, ceiling: i16) {
    let left_percent = attr_to_percent(-(left as i16), ceiling);
    let right_percent = attr_to_percent(right as i16, ceiling);
    let width_percent = right_percent - left_percent;

    node.left = Val::Percent(left_percent);
    node.width = Val::Percent(width_percent);
}

/// Writes the apply label's budget counter, and colours it red while steps
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

    let unallocated = ActorAttributes::held(attrs.total_level()).saturating_sub(ActorAttributes::invested(draft));

    for child in children.iter() {
        if let Ok(mut text) = text_query.get_mut(child) {
            if unallocated > 0 {
                **text = format!("Apply ({} steps left)", unallocated);
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

