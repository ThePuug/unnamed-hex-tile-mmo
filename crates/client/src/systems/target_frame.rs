//! The target frames: what the viewed actor has targeted, a hostile and an
//! ally, side by side in the top-right corner. Each shows its target's
//! level, name, triumvirate and health, and stays on the last one targeted while that one stands in the world, so
//! turning away does not blank it.

use bevy::prelude::*;

use common_bevy::components::{ally_target::AllyTarget, entity_type::*, resources::*, target::Target, ActorAttributes};

/// Which of the viewed actor's targets a frame shows. Every part of a frame
/// carries its lane, and the two differ only in what `Lane` answers.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lane {
    Hostile,
    Ally,
}

pub const LANES: [Lane; 2] = [Lane::Hostile, Lane::Ally];

/// A fixed part of a frame, by what it shows.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Frame,
    Name,
    Triumvirate,
    HealthBar,
    HealthText,
    LevelHex,
    LevelText,
}

/// The colours and place that tell one lane's frame from the other's.
struct Theme {
    /// Pixels in from the right edge: the ally frame stands left of the hostile
    right: f32,
    border: Color,
    background: Color,
    name: Color,
    bar_border: Color,
    bar_background: Color,
    bar_fill: Color,
}

impl Lane {
    fn theme(self) -> Theme {
        match self {
            Lane::Hostile => Theme {
                right: 10.,
                border: Color::srgba(0.5, 0.5, 0.5, 0.8),
                background: Color::srgba(0.1, 0.1, 0.1, 0.85),
                name: Color::WHITE,
                bar_border: Color::srgb(0.3, 0.3, 0.3),
                bar_background: Color::srgb(0.2, 0.1, 0.0),
                bar_fill: Color::srgb(0.9, 0.5, 0.0),
            },
            Lane::Ally => Theme {
                right: 300.,
                border: Color::srgba(0.0, 0.6, 0.0, 0.8),
                background: Color::srgba(0.0, 0.15, 0.0, 0.85),
                name: Color::srgb(0.8, 1.0, 0.8),
                bar_border: Color::srgb(0.0, 0.4, 0.0),
                bar_background: Color::srgb(0.0, 0.15, 0.0),
                bar_fill: Color::srgb(0.2, 0.8, 0.2),
            },
        }
    }

    /// The target this lane's frame shows: the last the viewed actor held
    fn target(self, target: &Target, ally: Option<&AllyTarget>) -> Option<Entity> {
        match self {
            Lane::Hostile => target.last_target,
            Lane::Ally => ally.and_then(|ally| ally.last_target),
        }
    }

    /// The target the viewed actor holds in this lane now, which the bars
    /// over it in the world follow
    pub fn held(self, target: &Target, ally: Option<&AllyTarget>) -> Option<Entity> {
        match self {
            Lane::Hostile => target.entity,
            Lane::Ally => ally.and_then(|ally| ally.entity),
        }
    }

    /// A name's colour from its origin's: an ally's blended toward white
    fn name(self, (r, g, b): (f32, f32, f32)) -> Color {
        match self {
            Lane::Hostile => Color::srgb(r, g, b),
            Lane::Ally => Color::srgb((r * 0.7 + 0.3).min(1.0), (g * 0.7 + 0.3).min(1.0), (b * 0.7 + 0.3).min(1.0)),
        }
    }
}

/// The level hexagon's colour for a target `diff` levels above the viewed
/// actor: grey more than five below, green two to five below, yellow
/// within one, red above that.
fn level_color(diff: i32) -> Color {
    if diff < -5 {
        Color::srgb(0.4, 0.4, 0.4)
    } else if diff <= -2 {
        Color::srgb(0.2, 0.8, 0.2)
    } else if diff.abs() <= 1 {
        Color::srgb(0.9, 0.9, 0.2)
    } else {
        Color::srgb(0.9, 0.2, 0.2)
    }
}

/// Builds both frames, hidden until their lane has a target.
pub fn setup(
    mut commands: Commands,
    query: Query<Entity, With<IsDefaultUiCamera>>,
) {
    let camera = query.single().expect("query did not return exactly one result");

    for lane in LANES {
        let theme = lane.theme();
        commands.spawn((
            UiTargetCamera(camera),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(280.),
                height: Val::Auto,
                top: Val::Px(10.),
                right: Val::Px(theme.right),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(10.)),
                row_gap: Val::Px(6.),
                ..default()
            },
            BorderColor::all(theme.border),
            BackgroundColor(theme.background),
            Visibility::Hidden,
            (lane, Part::Frame),
        ))
        .with_children(|parent| {
            // Header row: level hexagon and name
            parent.spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(6.),
                    align_items: AlignItems::Center,
                    ..default()
                },
            ))
            .with_children(|parent| {
                parent.spawn((
                    Node {
                        width: Val::Px(24.),
                        height: Val::Px(24.),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border: UiRect::all(Val::Px(2.)),
                        border_radius: BorderRadius::all(Val::Px(3.)), // Slight rounding to suggest hexagon
                        ..default()
                    },
                    BorderColor::all(Color::srgb(0.5, 0.5, 0.5)),
                    BackgroundColor(Color::srgb(0.3, 0.3, 0.3)),
                    (lane, Part::LevelHex),
                ))
                .with_children(|parent| {
                    parent.spawn((
                        Text::new("0"),
                        TextFont { font_size: FontSize::Px(12.0), ..default() },
                        TextColor(Color::WHITE),
                        (lane, Part::LevelText),
                    ));
                });

                parent.spawn((
                    Text::new(""),
                    TextFont { font_size: FontSize::Px(14.0), ..default() },
                    TextColor(theme.name),
                    (lane, Part::Name),
                ));
            });

            // Approach / Resilience, coloured by origin
            parent.spawn((
                Text::new(""),
                TextFont { font_size: FontSize::Px(11.0), ..default() },
                TextColor(Color::srgb(0.8, 0.8, 0.8)),
                (lane, Part::Triumvirate),
            ));

            // Health bar
            parent.spawn((
                Node {
                    width: Val::Percent(100.),
                    height: Val::Px(16.),
                    border: UiRect::all(Val::Px(2.)),
                    justify_content: JustifyContent::FlexEnd,
                    align_items: AlignItems::Center,
                    padding: UiRect::all(Val::Px(2.)),
                    ..default()
                },
                BorderColor::all(theme.bar_border),
                BackgroundColor(theme.bar_background),
            ))
            .with_children(|parent| {
                parent.spawn((
                    Node {
                        width: Val::Percent(100.),
                        height: Val::Percent(100.),
                        position_type: PositionType::Absolute,
                        left: Val::Px(0.),
                        top: Val::Px(0.),
                        ..default()
                    },
                    BackgroundColor(theme.bar_fill),
                    (lane, Part::HealthBar),
                ));

                // Exact numbers, right-aligned on the bar
                parent.spawn((
                    Text::new(""),
                    TextFont { font_size: FontSize::Px(12.0), ..default() },
                    TextColor(Color::WHITE),
                    Node { position_type: PositionType::Relative, ..default() },
                    (lane, Part::HealthText),
                ));
            });

        });
    }
}

/// Shows in each frame the lane's last target while it stands in the
/// world, and hides the frame with none, or while the viewed actor is dead.
pub fn update(
    mut parts: Query<(&Lane, &Part, &mut Visibility, Option<&mut Text>, Option<&mut TextColor>, Option<&mut Node>, Option<&mut BackgroundColor>)>,
    viewed: Query<(&Health, &Target, Option<&AllyTarget>, Option<&ActorAttributes>), With<crate::components::Viewed>>,
    targets: Query<(&EntityType, &Health, Option<&ActorAttributes>)>,
) {
    let Ok((own_health, target, ally, own_attrs)) = viewed.single() else {
        return;
    };
    let shown = LANES.map(|lane| {
        lane.target(target, ally)
            .filter(|_| own_health.state > 0.0)
            .and_then(|ent| targets.get(ent).ok())
    });

    for (lane, part, mut visibility, text, color, node, background) in &mut parts {
        let Some((typ, health, attrs)) = shown[*lane as usize] else {
            // A frame hides its parts with it
            if *part == Part::Frame {
                *visibility = Visibility::Hidden;
            }
            continue;
        };
        let actor = match typ {
            EntityType::Actor(actor) => Some(actor),
            _ => None,
        };
        let level = attrs.map(|attrs| attrs.total_level() as i32);
        match part {
            Part::Frame => *visibility = Visibility::Visible,
            Part::Name => {
                if let Some(mut text) = text {
                    **text = typ.display_name().to_string();
                }
                if let (Some(mut color), Some(actor)) = (color, actor) {
                    color.0 = lane.name(actor.origin.color());
                }
            }
            Part::Triumvirate => {
                if let (Some(mut text), Some(mut color), Some(actor)) = (text, color, actor) {
                    **text = format!("{} / {}", actor.approach.display_name(), actor.resilience.display_name());
                    // The origin's colour, dimmer than the name
                    let (r, g, b) = actor.origin.color();
                    color.0 = Color::srgb(r * 0.8, g * 0.8, b * 0.8);
                }
            }
            Part::HealthBar => {
                if let Some(mut node) = node {
                    let percent = if health.max > 0.0 { (health.state / health.max * 100.0).clamp(0.0, 100.0) } else { 0.0 };
                    node.width = Val::Percent(percent);
                }
            }
            Part::HealthText => {
                if let Some(mut text) = text {
                    **text = format!("{:.0}/{:.0}", health.state, health.max);
                }
            }
            // The level its attributes give it, coloured by the gap to the viewed actor's own
            Part::LevelHex => {
                if let (Some(mut background), Some(level)) = (background, level) {
                    background.0 = level_color(level - own_attrs.map_or(level, |own| own.total_level() as i32));
                }
            }
            Part::LevelText => {
                if let (Some(mut text), Some(level)) = (text, level) {
                    **text = level.to_string();
                }
            }
        }
    }
}
