//! The target frames: what the viewed actor has targeted, a hostile and an
//! ally, side by side in the top-right corner. Each shows its target's
//! level, name, triumvirate, health and the front of its threat queue, and
//! stays on the last one targeted while that one stands in the world, so
//! turning away does not blank it.

use bevy::prelude::*;

use crate::resources::Server;
use common_bevy::components::{ally_target::AllyTarget, entity_type::*, reaction_queue::*, resources::*, target::Target, ActorAttributes};

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
    /// The threat queue section, shown for a target that has a queue
    Queue,
    /// Holds a [`CapacityDot`] for each slot of the target's window
    Dots,
    /// Holds a [`ThreatIcon`] for each of the first threats in its queue
    Icons,
}

/// A slot of the target's queue window, lit while a threat stands in it
#[derive(Component)]
pub struct CapacityDot {
    pub index: usize,
}

/// One of the first threats in the target's queue
#[derive(Component)]
pub struct ThreatIcon;

/// The ring inside a [`ThreatIcon`] that grows as the threat's time runs out
#[derive(Component)]
pub struct ThreatTimerRing {
    pub index: usize,
}

/// How many of the queue's threats a frame draws
const ICONS: usize = 3;

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

    /// A capacity dot's fill and border: brightest in a full window, hollow
    /// where no threat stands
    fn dot(self, filled: bool, full: bool) -> (Color, Color) {
        if !filled {
            return (Color::NONE, Color::srgb(0.5, 0.5, 0.5));
        }
        let lit = match (self, full) {
            (Lane::Hostile, true) => Color::srgb(1.0, 0.3, 0.3),
            (Lane::Hostile, false) => Color::srgb(0.9, 0.4, 0.4),
            (Lane::Ally, true) => Color::srgb(0.3, 1.0, 0.3),
            (Lane::Ally, false) => Color::srgb(0.6, 0.9, 0.4),
        };
        (lit, lit)
    }

    /// A threat icon's border, brighter in a full window, and its background
    fn icon(self, full: bool) -> (Color, Color) {
        let strength = if full { 1.0 } else { 0.8 };
        match self {
            Lane::Hostile => (Color::srgb(strength, 0.2, 0.2), Color::srgb(0.3, 0.1, 0.1)),
            Lane::Ally => (Color::srgb(0.2, strength, 0.2), Color::srgb(0.1, 0.3, 0.1)),
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

/// A threat's timer ring at `now`: its size as a percentage of the icon,
/// growing from 15 to 100 as the time runs out, and its colour, from yellow
/// through orange at half to red.
fn timer_ring(threat: &QueuedThreat, now: std::time::Duration) -> (f32, Color) {
    let elapsed = now.saturating_sub(threat.inserted_at);
    let progress = (elapsed.as_secs_f32() / threat.timer_duration.as_secs_f32()).clamp(0.0, 1.0);
    let remaining = 1.0 - progress;
    let green = if remaining > 0.5 {
        let t = (remaining - 0.5) / 0.5;
        0.9 * t + 0.5 * (1.0 - t)
    } else {
        0.5 * remaining / 0.5
    };
    (15.0 + 85.0 * progress, Color::srgba(1.0, green, 0.0, 0.9))
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

            // Threat queue: capacity dots over threat icons
            parent.spawn((
                Node {
                    width: Val::Percent(100.),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(6.),
                    ..default()
                },
                Visibility::Hidden,
                (lane, Part::Queue),
            ))
            .with_children(|parent| {
                parent.spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(3.),
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    (lane, Part::Dots),
                ));
                parent.spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(5.),
                        ..default()
                    },
                    (lane, Part::Icons),
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
    targets: Query<(&EntityType, &Health, Has<ReactionQueue>, Option<&ActorAttributes>)>,
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
        let Some((typ, health, has_queue, attrs)) = shown[*lane as usize] else {
            // A frame hides its parts with it, but the queue section, which
            // keeps a visibility of its own
            if matches!(part, Part::Frame | Part::Queue) {
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
            Part::Queue => *visibility = if has_queue { Visibility::Inherited } else { Visibility::Hidden },
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
            Part::Dots | Part::Icons => {}
        }
    }
}

/// Draws each frame's queue section: a dot to each slot of the target's
/// window, and an icon to each of the first threats in it that are not
/// pressure and have time left. Dots and icons are rebuilt when their count
/// changes and recoloured in place otherwise.
pub fn update_queue(
    mut commands: Commands,
    viewed: Query<(&Target, Option<&AllyTarget>), With<crate::components::Viewed>>,
    queues: Query<&ReactionQueue>,
    containers: Query<(Entity, &Lane, &Part)>,
    mut dots: Query<(Entity, &Lane, &CapacityDot, &mut BackgroundColor, &mut BorderColor), (Without<ThreatIcon>, Without<ThreatTimerRing>)>,
    mut icons: Query<(Entity, &Lane, &mut BorderColor), (With<ThreatIcon>, Without<CapacityDot>, Without<ThreatTimerRing>)>,
    mut rings: Query<(&Lane, &ThreatTimerRing, &mut Node, &mut BorderColor), (Without<CapacityDot>, Without<ThreatIcon>)>,
    time: Res<Time>,
    server: Res<Server>,
) {
    let Ok((target, ally)) = viewed.single() else {
        return;
    };
    let now_ms = server.current_time(time.elapsed().as_millis());
    let now = std::time::Duration::from_millis(now_ms.min(u64::MAX as u128) as u64);

    for lane in LANES {
        let Some(queue) = lane.target(target, ally).and_then(|ent| queues.get(ent).ok()) else { continue };
        let container = |wanted: Part| containers.iter().find(|(_, of, part)| **of == lane && **part == wanted).map(|(ent, ..)| ent);
        let filled = queue.visible_count();
        let full = filled >= queue.window_size;

        if dots.iter().filter(|(_, of, ..)| **of == lane).count() != queue.window_size {
            for (ent, of, ..) in &dots {
                if *of == lane {
                    commands.entity(ent).despawn();
                }
            }
            if let Some(holder) = container(Part::Dots) {
                commands.entity(holder).with_children(|parent| {
                    for index in 0..queue.window_size {
                        let (fill, border) = lane.dot(index < filled, full);
                        parent.spawn((
                            Node {
                                width: Val::Px(8.),
                                height: Val::Px(8.),
                                border: UiRect::all(Val::Px(1.)),
                                border_radius: BorderRadius::all(Val::Percent(50.)),
                                ..default()
                            },
                            BorderColor::all(border),
                            BackgroundColor(fill),
                            (lane, CapacityDot { index }),
                        ));
                    }
                });
            }
        } else {
            for (_, of, dot, mut background, mut border) in &mut dots {
                if *of == lane {
                    let (fill, edge) = lane.dot(dot.index < filled, full);
                    background.0 = fill;
                    *border = BorderColor::all(edge);
                }
            }
        }

        let threats: Vec<&QueuedThreat> = queue.threats.iter()
            .filter(|threat| !threat.is_pressure())
            .filter(|threat| now.saturating_sub(threat.inserted_at) < threat.timer_duration)
            .take(ICONS)
            .collect();
        let (edge, fill) = lane.icon(full);

        if icons.iter().filter(|(_, of, _)| **of == lane).count() != threats.len() {
            for (ent, of, _) in &icons {
                if *of == lane {
                    commands.entity(ent).despawn();
                }
            }
            let Some(holder) = container(Part::Icons) else { continue };
            commands.entity(holder).with_children(|parent| {
                for (index, threat) in threats.iter().enumerate() {
                    let (size, color) = timer_ring(threat, now);
                    parent.spawn((
                        Node {
                            width: Val::Px(40.),
                            height: Val::Px(40.),
                            border: UiRect::all(Val::Px(2.)),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            border_radius: BorderRadius::all(Val::Percent(50.)),
                            ..default()
                        },
                        BorderColor::all(edge),
                        BackgroundColor(fill),
                        (lane, ThreatIcon),
                    ))
                    .with_children(|parent| {
                        parent.spawn((
                            Node {
                                position_type: PositionType::Absolute,
                                width: Val::Percent(size),
                                height: Val::Percent(size),
                                left: Val::Percent((100.0 - size) / 2.0),
                                top: Val::Percent((100.0 - size) / 2.0),
                                border: UiRect::all(Val::Px(3.)),
                                border_radius: BorderRadius::all(Val::Percent(50.)),
                                ..default()
                            },
                            BorderColor::all(color),
                            BackgroundColor(Color::NONE),
                            (lane, ThreatTimerRing { index }),
                        ));
                        parent.spawn((
                            Text::new("⚔"),
                            TextFont { font_size: FontSize::Px(22.0), ..default() },
                            TextColor(Color::WHITE),
                        ));
                    });
                }
            });
        } else {
            for (_, of, mut border) in &mut icons {
                if *of == lane {
                    *border = BorderColor::all(edge);
                }
            }
            for (of, ring, mut node, mut border) in &mut rings {
                let Some(threat) = threats.get(ring.index).filter(|_| *of == lane) else { continue };
                let (size, color) = timer_ring(threat, now);
                node.width = Val::Percent(size);
                node.height = Val::Percent(size);
                node.left = Val::Percent((100.0 - size) / 2.0);
                node.top = Val::Percent((100.0 - size) / 2.0);
                *border = BorderColor::all(color);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_timer_ring_grows_and_reddens_as_its_time_runs_out() {
        let attrs = ActorAttributes::default();
        let queued = Duration::from_secs(10);
        let threat = common_bevy::systems::combat::queue::create_threat(Entity::PLACEHOLDER, &attrs, &attrs, 1.0, None, queued, 0.0, 0.0);
        let after = |share: f32| timer_ring(&threat, queued + threat.timer_duration.mul_f32(share));
        let (mut size, mut green) = (0.0, f32::INFINITY);
        for step in 0..=8 {
            let (next, color) = after(step as f32 / 8.0);
            let next_green = color.to_srgba().green;
            assert!(next > size && next_green < green, "step {step}: {next} {next_green}");
            (size, green) = (next, next_green);
        }
        assert_eq!(after(3.0).0, 100.0, "full, and no further, once its time is out");
    }

    #[test]
    fn a_dot_is_hollow_until_a_threat_stands_in_its_slot() {
        for lane in LANES {
            assert_eq!(lane.dot(false, false).0, Color::NONE);
            assert_eq!(lane.dot(false, true).0, Color::NONE);
            assert_ne!(lane.dot(true, false).0, Color::NONE);
            assert_ne!(lane.dot(true, true), lane.dot(true, false), "a full window shows");
        }
    }
}
