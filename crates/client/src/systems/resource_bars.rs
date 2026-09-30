//! The viewed actor's pools along the bottom of the screen: a bar to each,
//! in the order [`BARS`] lists them, filled to its share of the pool and
//! labelled with its numbers.

use bevy::prelude::*;

use common_bevy::components::resources::*;

/// A pool a bar shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pool {
    Stamina,
    Health,
    Endurance,
}

/// The bars, left to right, each with its fill's colour. A pool listed here
/// and answered in [`Pool::of`] has its bar.
const BARS: [(Pool, Color); 3] = [
    (Pool::Stamina, Color::srgb(0.9, 0.8, 0.0)),
    (Pool::Health, Color::srgb(0.9, 0.1, 0.1)),
    (Pool::Endurance, Color::srgb(0.2, 0.65, 0.45)),
];

/// How fast a bar's fill eases toward its pool
const EASE: f32 = 5.0;

impl Pool {
    /// What the pool holds now and at most
    fn of(self, (health, stamina, endurance): (&Health, &Stamina, &Endurance)) -> (f32, f32) {
        match self {
            Pool::Stamina => (stamina.state, stamina.max),
            Pool::Health => (health.state, health.max),
            Pool::Endurance => (endurance.state, endurance.max),
        }
    }
}

/// The fill of a pool's bar, and the percentage it shows now, which eases
/// toward the pool's
#[derive(Component)]
pub struct PoolBar {
    pub pool: Pool,
    pub current_percent: f32,
}

/// The label on a pool's bar, with the numbers it last wrote, so it is
/// written again only when they change
#[derive(Component)]
pub struct PoolText {
    pool: Pool,
    shown: (i32, i32),
}

/// Builds the bars, midway between the player and the bottom of the screen.
pub fn setup(
    mut commands: Commands,
    query: Query<Entity, With<IsDefaultUiCamera>>,
) {
    let camera = query.single().expect("query did not return exactly one result");

    commands.spawn((
        UiTargetCamera(camera),
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.),
            height: Val::Percent(100.),
            bottom: Val::Px(0.),
            left: Val::Px(0.),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::FlexEnd,
            padding: UiRect::bottom(Val::Percent(12.5)),
            ..default()
        },
        Pickable::IGNORE,
        crate::components::ViewHud,
    ))
    .with_children(|parent| {
        parent.spawn((
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(10.),
                ..default()
            },
        ))
        .with_children(|parent| {
            for (pool, fill) in BARS {
                parent.spawn((
                    Node {
                        width: Val::Px(200.),
                        height: Val::Px(20.),
                        border: UiRect::all(Val::Px(2.)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BorderColor::all(Color::srgb(0.3, 0.3, 0.3)),
                    BackgroundColor(Color::srgb(0.1, 0.1, 0.1)),
                ))
                .with_children(|parent| {
                    parent.spawn((
                        Node {
                            width: Val::Percent(100.),
                            height: Val::Percent(100.),
                            position_type: PositionType::Absolute,
                            ..default()
                        },
                        BackgroundColor(fill),
                        PoolBar { pool, current_percent: 100.0 },
                    ));
                    parent.spawn((
                        Text::new(""),
                        TextFont { font_size: FontSize::Px(12.0), ..default() },
                        TextColor(Color::WHITE),
                        Node { position_type: PositionType::Relative, ..default() },
                        PoolText { pool, shown: (i32::MIN, i32::MIN) },
                    ));
                });
            }
        });
    });
}

/// Eases each bar toward its pool's share and rewrites a label whose
/// numbers changed.
pub fn update(
    mut bars: Query<(&mut PoolBar, &mut Node)>,
    mut labels: Query<(&mut PoolText, &mut Text)>,
    viewed: Query<(&Health, &Stamina, &Endurance), With<crate::components::Viewed>>,
    time: Res<Time>,
) {
    let Ok(pools) = viewed.single() else {
        return;
    };
    for (mut bar, mut node) in &mut bars {
        let (now, max) = bar.pool.of(pools);
        let share = if max > 0.0 { (now / max * 100.0).clamp(0.0, 100.0) } else { 0.0 };
        bar.current_percent = bar.current_percent.lerp(share, EASE * time.delta_secs());
        node.width = Val::Percent(bar.current_percent);
    }
    for (mut label, mut text) in &mut labels {
        let (now, max) = label.pool.of(pools);
        let shown = (now as i32, max as i32);
        if label.shown != shown {
            label.shown = shown;
            **text = format!("{} / {}", shown.0, shown.1);
        }
    }
}
