use bevy::prelude::*;

use crate::{
    client::{
        components::*,
        resources::Server
    },
    common::systems::*,
};

pub fn setup(
    mut commands: Commands,
    query: Query<Entity, Added<Camera3d>>,
) {
    let camera = query.single().expect("query did not return exactly one result");
    commands.spawn((
        UiTargetCamera(camera),
        Node {
            width: Val::Percent(100.),
            height: Val::Percent(100.),
            ..default()
        },
    ))
    .with_children(|parent| {
        parent.spawn((
            Text::new(""),
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(12.),
                left: Val::Px(12.),
                ..default()
            },
            Info::Time,
        ));
    });
}

pub fn update(
    mut query: Query<(&mut Text, &Info)>,
    server: Res<Server>,
    time: Res<Time>,
) {
    for (mut span, info) in &mut query { 
        **span = match info {
            Info::Time => {
                let dt = time.elapsed().as_millis() + server.elapsed_offset;
                let season = match dt % YEAR_MS / SEASON_MS {
                    0 => "Thaw",
                    1 => "Blaze",
                    2 => "Ash",
                    _ => "Freeze",
                };
                let week = match dt % SEASON_MS / WEEK_MS {
                    0 => "Mon",
                    1 => "Tus",
                    2 => "Wed",
                    3 => "Tur",
                    4 => "Fid",
                    5 => "Sat",
                    _ => "Sun",
                };
                let day = dt % WEEK_MS / DAY_MS;
                let hour = dt % DAY_MS / HOUR_MS;
                let minute = dt % HOUR_MS / MINUTE_MS;
                format!("{hour:02}:{minute:02} {day}.{week}.{season}") 
            }
        };
    }
}