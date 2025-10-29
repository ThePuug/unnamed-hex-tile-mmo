use bevy::prelude::*;

use crate::{
    common::components::{resources::*, behaviour::Behaviour},
};

/// Marker component for the health bar UI element
#[derive(Component)]
pub struct HealthBar;

/// Marker component for the stamina bar UI element
#[derive(Component)]
pub struct StaminaBar;

/// Marker component for the mana bar UI element
#[derive(Component)]
pub struct ManaBar;

/// Setup resource bars in the player HUD
/// Creates health, stamina, and mana bars in the bottom-left corner
pub fn setup(
    mut commands: Commands,
    query: Query<Entity, Added<Camera3d>>,
) {
    let camera = query.single().expect("query did not return exactly one result");

    commands.spawn((
        UiTargetCamera(camera),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(20.),
            left: Val::Px(20.),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(5.),
            ..default()
        },
    ))
    .with_children(|parent| {
        // Health bar (Red)
        parent.spawn((
            Node {
                width: Val::Px(200.),
                height: Val::Px(20.),
                border: UiRect::all(Val::Px(2.)),
                ..default()
            },
            BorderColor(Color::srgb(0.3, 0.3, 0.3)),
            BackgroundColor(Color::srgb(0.1, 0.1, 0.1)),
        ))
        .with_children(|parent| {
            parent.spawn((
                Node {
                    width: Val::Percent(100.),
                    height: Val::Percent(100.),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.8, 0.2, 0.2)), // Red
                HealthBar,
            ));
        });

        // Stamina bar (Yellow)
        parent.spawn((
            Node {
                width: Val::Px(200.),
                height: Val::Px(20.),
                border: UiRect::all(Val::Px(2.)),
                ..default()
            },
            BorderColor(Color::srgb(0.3, 0.3, 0.3)),
            BackgroundColor(Color::srgb(0.1, 0.1, 0.1)),
        ))
        .with_children(|parent| {
            parent.spawn((
                Node {
                    width: Val::Percent(100.),
                    height: Val::Percent(100.),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.9, 0.9, 0.2)), // Yellow
                StaminaBar,
            ));
        });

        // Mana bar (Blue)
        parent.spawn((
            Node {
                width: Val::Px(200.),
                height: Val::Px(20.),
                border: UiRect::all(Val::Px(2.)),
                ..default()
            },
            BorderColor(Color::srgb(0.3, 0.3, 0.3)),
            BackgroundColor(Color::srgb(0.1, 0.1, 0.1)),
        ))
        .with_children(|parent| {
            parent.spawn((
                Node {
                    width: Val::Percent(100.),
                    height: Val::Percent(100.),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.2, 0.5, 0.9)), // Blue
                ManaBar,
            ));
        });
    });
}

/// Update resource bar widths based on player's current resources
/// Uses `step` for local player (client prediction)
pub fn update(
    mut health_query: Query<&mut Node, (With<HealthBar>, Without<StaminaBar>, Without<ManaBar>)>,
    mut stamina_query: Query<&mut Node, (With<StaminaBar>, Without<HealthBar>, Without<ManaBar>)>,
    mut mana_query: Query<&mut Node, (With<ManaBar>, Without<HealthBar>, Without<StaminaBar>)>,
    player_query: Query<(&Health, &Stamina, &Mana), With<Behaviour>>,
) {
    // Find the local player (has Behaviour::Controlled component)
    for (health, stamina, mana) in &player_query {
        // Update health bar width (use step for client prediction)
        for mut node in &mut health_query {
            let percent = if health.max > 0.0 {
                (health.step / health.max * 100.0).clamp(0.0, 100.0)
            } else {
                0.0
            };
            node.width = Val::Percent(percent);
        }

        // Update stamina bar width
        for mut node in &mut stamina_query {
            let percent = if stamina.max > 0.0 {
                (stamina.step / stamina.max * 100.0).clamp(0.0, 100.0)
            } else {
                0.0
            };
            node.width = Val::Percent(percent);
        }

        // Update mana bar width
        for mut node in &mut mana_query {
            let percent = if mana.max > 0.0 {
                (mana.step / mana.max * 100.0).clamp(0.0, 100.0)
            } else {
                0.0
            };
            node.width = Val::Percent(percent);
        }

        // Only update for the first player found (local player)
        break;
    }
}
