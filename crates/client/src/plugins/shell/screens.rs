//! The screens that stand in front of the world until it is played: the
//! connection's state, the character, the loading bar. The round trip to
//! the server shows over every stage, the world included.

use std::{collections::VecDeque, time::Duration};

use bevy::prelude::*;

use super::{Loading, Stage};
use crate::{
    network::{ClientNet, Link},
    systems::equipment_panel::CloseupView,
};

#[derive(Component)]
pub struct Backdrop;

#[derive(Component)]
pub struct Status;

#[derive(Component)]
pub struct Detail;

#[derive(Component)]
pub struct Preview;

#[derive(Component)]
pub struct Bar;

#[derive(Component)]
pub struct BarFill;

#[derive(Component)]
pub struct Rtt;

const BACKDROP: Color = Color::srgb(0.035, 0.035, 0.04);
const STATUS: Color = Color::srgb(0.85, 0.75, 0.55);
const DETAIL: Color = Color::srgb(0.55, 0.55, 0.55);
const BAR_BACK: Color = Color::srgb(0.15, 0.15, 0.15);
const BAR_FILL: Color = Color::srgb(0.75, 0.6, 0.3);
const BAR_WIDTH: f32 = 480.0;
/// The preview's size on screen: the closeup's texture, at three quarters.
const PREVIEW: Vec2 = Vec2::new(420.0, 540.0);

pub fn setup(mut commands: Commands, assets: Res<AssetServer>) {
    commands
        .spawn((
            Backdrop,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: Val::Px(18.0),
                ..default()
            },
            BackgroundColor(BACKDROP),
            GlobalZIndex(1500),
        ))
        .with_children(|parent| {
            parent.spawn((
                Preview,
                CloseupView,
                Node { width: Val::Px(PREVIEW.x), height: Val::Px(PREVIEW.y), display: Display::None, ..default() },
            ));
            parent.spawn((
                Status,
                Text::new(""),
                TextFont { font_size: FontSize::Px(24.0), ..default() },
                TextColor(STATUS),
            ));
            parent
                .spawn((
                    Bar,
                    Node { width: Val::Px(BAR_WIDTH), height: Val::Px(10.0), display: Display::None, ..default() },
                    BackgroundColor(BAR_BACK),
                ))
                .with_child((BarFill, Node { width: Val::Percent(0.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(BAR_FILL)));
            parent.spawn((
                Detail,
                Text::new(""),
                TextFont { font_size: FontSize::Px(14.0), ..default() },
                TextColor(DETAIL),
            ));
        });

    commands.spawn((
        Rtt,
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            right: Val::Px(12.0),
            ..default()
        },
        Text::new(""),
        // The Nerd Font carries the signal glyphs the default font lacks.
        TextFont { font: assets.load("fonts/IosevkaNerdFont-Regular.ttf").into(), font_size: FontSize::Px(16.0), ..default() },
        TextColor(DETAIL),
        GlobalZIndex(2100),
    ));
}

/// What the screen says and shows for the stage and the connection.
#[allow(clippy::type_complexity)]
pub fn draw(
    stage: Res<State<Stage>>,
    link: Res<Link>,
    loading: Res<Loading>,
    time: Res<Time<Real>>,
    mut backdrop: Query<&mut Visibility, With<Backdrop>>,
    mut preview: Query<&mut Node, (With<Preview>, Without<Bar>, Without<BarFill>)>,
    mut bar: Query<&mut Node, (With<Bar>, Without<Preview>, Without<BarFill>)>,
    mut fill: Query<&mut Node, (With<BarFill>, Without<Preview>, Without<Bar>)>,
    mut status: Query<&mut Text, (With<Status>, Without<Detail>)>,
    mut detail: Query<&mut Text, (With<Detail>, Without<Status>)>,
) {
    let stage = *stage.get();
    // Out of the layout, not merely hidden, so what stays is centred alone.
    let shown = |on: bool| if on { Display::Flex } else { Display::None };
    if let Ok(mut v) = backdrop.single_mut() {
        v.set_if_neq(if stage == Stage::Playing { Visibility::Hidden } else { Visibility::Visible });
    }
    if let Ok(mut node) = preview.single_mut() {
        let display = shown(stage == Stage::CharacterSelect);
        if node.display != display {
            node.display = display;
        }
    }
    if let Ok(mut node) = bar.single_mut() {
        let display = shown(stage == Stage::Loading);
        if node.display != display {
            node.display = display;
        }
    }

    let (said, detailed) = match stage {
        Stage::Connecting => match &*link {
            Link::Connecting { failures: 0 } => ("Connecting to server".to_string(), String::new()),
            Link::Connecting { failures } => ("Connecting to server".to_string(), format!("attempt {}", failures + 1)),
            Link::Waiting { failures: 0, .. } => ("Connecting to server".to_string(), String::new()),
            Link::Waiting { retry_at, reason, .. } => {
                let wait = retry_at.saturating_sub(time.elapsed()).as_secs_f32().ceil();
                (format!("Server unreachable, retrying in {wait:.0}s"), reason.clone())
            }
            Link::Connected => ("Connected".to_string(), String::new()),
        },
        Stage::CharacterSelect => ("Adventurer".to_string(), "Left/Right turn".to_string()),
        Stage::Loading => {
            let (have, want) = loading.chunks;
            let doing = if want == 0 {
                "waiting for the world".to_string()
            } else if have < want {
                format!("terrain {have} / {want} chunks")
            } else {
                format!("building terrain, {} regions to go", loading.building)
            };
            ("Entering the world".to_string(), doing)
        }
        Stage::Playing => (String::new(), String::new()),
    };
    if let Ok(mut text) = status.single_mut() {
        if text.0 != said {
            text.0 = said;
        }
    }
    if let Ok(mut text) = detail.single_mut() {
        if text.0 != detailed {
            text.0 = detailed;
        }
    }
    if let (Stage::Loading, Ok(mut node)) = (stage, fill.single_mut()) {
        node.width = Val::Percent(100.0 * loading.progress().clamp(0.0, 1.0));
    }
}

/// How far back the round trips shown reach, how often one is taken, and
/// which quantile of them shows: the slow tail is what play feels.
const RTT_WINDOW: Duration = Duration::from_secs(5);
const RTT_EVERY: Duration = Duration::from_millis(100);
const RTT_QUANTILE: f64 = 0.95;

/// Nerd Font glyphs, `md-lan_connect` and `md-lan_disconnect`.
const CONNECTED: char = '\u{F0318}';
const OFFLINE: char = '\u{F0319}';

/// The round trips taken over the last `RTT_WINDOW`, oldest first, with
/// when each was taken on the real clock.
#[derive(Resource, Default)]
pub struct RttSamples(VecDeque<(Duration, Duration)>);

/// The round trip to the server, over every stage: a quantile of the last
/// few seconds, so one slow packet neither hides nor dominates.
pub fn show_rtt(
    net: Option<Res<ClientNet>>,
    link: Res<Link>,
    time: Res<Time<Real>>,
    mut samples: ResMut<RttSamples>,
    mut text: Query<&mut Text, With<Rtt>>,
) {
    let now = time.elapsed();
    let said = match (net, link.is_connected()) {
        (Some(net), true) => {
            if samples.0.back().is_none_or(|&(at, _)| now - at >= RTT_EVERY) {
                samples.0.push_back((now, net.rtt()));
            }
            while samples.0.front().is_some_and(|&(at, _)| now - at > RTT_WINDOW) {
                samples.0.pop_front();
            }
            let mut taken: Vec<Duration> = samples.0.iter().map(|&(_, rtt)| rtt).collect();
            taken.sort_unstable();
            let shown = common::quantile(&taken, RTT_QUANTILE).unwrap_or_default();
            format!("{CONNECTED} {} ms", shown.as_millis())
        }
        _ => {
            if !samples.0.is_empty() {
                samples.0.clear();
            }
            OFFLINE.to_string()
        }
    };
    let Ok(mut text) = text.single_mut() else { return };
    if text.0 != said {
        text.0 = said;
    }
}
