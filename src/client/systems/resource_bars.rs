use bevy::prelude::*;

use crate::{
    common::components::{Actor, resources::*},
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

/// Marker component for the health bar text label
#[derive(Component)]
pub struct HealthText;

/// Marker component for the stamina bar text label
#[derive(Component)]
pub struct StaminaText;

/// Marker component for the mana bar text label
#[derive(Component)]
pub struct ManaText;

/// Setup resource bars in the player HUD
/// Creates health, stamina, and mana bars in bottom-center position
/// Positioned at midpoint between player and bottom of screen for combat-critical info
pub fn setup(
    mut commands: Commands,
    query: Query<Entity, Added<Camera3d>>,
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
            padding: UiRect::bottom(Val::Percent(12.5)),  // Halfway between bottom and player
            ..default()
        },
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
        // Stamina bar (Yellow) - left position
        parent.spawn((
            Node {
                width: Val::Px(200.),
                height: Val::Px(20.),
                border: UiRect::all(Val::Px(2.)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BorderColor(Color::srgb(0.3, 0.3, 0.3)),
            BackgroundColor(Color::srgb(0.1, 0.1, 0.1)),
        ))
        .with_children(|parent| {
            // Stamina fill bar
            parent.spawn((
                Node {
                    width: Val::Percent(100.),
                    height: Val::Percent(100.),
                    position_type: PositionType::Absolute,
                    ..default()
                },
                BackgroundColor(Color::srgb(0.9, 0.8, 0.0)), // Yellow
                StaminaBar,
            ));
            // Stamina text label
            parent.spawn((
                Text::new("100 / 100"),
                TextFont {
                    font_size: 12.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                Node {
                    position_type: PositionType::Relative,
                    ..default()
                },
                StaminaText,
            ));
        });

        // Health bar (Red) - center position
        parent.spawn((
            Node {
                width: Val::Px(200.),
                height: Val::Px(20.),
                border: UiRect::all(Val::Px(2.)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BorderColor(Color::srgb(0.3, 0.3, 0.3)),
            BackgroundColor(Color::srgb(0.1, 0.1, 0.1)),
        ))
        .with_children(|parent| {
            // Health fill bar
            parent.spawn((
                Node {
                    width: Val::Percent(100.),
                    height: Val::Percent(100.),
                    position_type: PositionType::Absolute,
                    ..default()
                },
                BackgroundColor(Color::srgb(0.9, 0.1, 0.1)), // Red
                HealthBar,
            ));
            // Health text label
            parent.spawn((
                Text::new("100 / 100"),
                TextFont {
                    font_size: 12.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                Node {
                    position_type: PositionType::Relative,
                    ..default()
                },
                HealthText,
            ));
        });

        // Mana bar (Blue)
        parent.spawn((
            Node {
                width: Val::Px(200.),
                height: Val::Px(20.),
                border: UiRect::all(Val::Px(2.)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BorderColor(Color::srgb(0.3, 0.3, 0.3)),
            BackgroundColor(Color::srgb(0.1, 0.1, 0.1)),
        ))
        .with_children(|parent| {
            // Mana fill bar
            parent.spawn((
                Node {
                    width: Val::Percent(100.),
                    height: Val::Percent(100.),
                    position_type: PositionType::Absolute,
                    ..default()
                },
                BackgroundColor(Color::srgb(0.1, 0.4, 0.9)), // Blue
                ManaBar,
            ));
            // Mana text label
            parent.spawn((
                Text::new("100 / 100"),
                TextFont {
                    font_size: 12.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                Node {
                    position_type: PositionType::Relative,
                    ..default()
                },
                ManaText,
            ));
        });
    });
    });
}

/// Update resource bar widths and text labels based on player's current resources
/// Uses `step` for local player (client prediction)
pub fn update(
    mut health_query: Query<&mut Node, (With<HealthBar>, Without<StaminaBar>, Without<ManaBar>)>,
    mut stamina_query: Query<&mut Node, (With<StaminaBar>, Without<HealthBar>, Without<ManaBar>)>,
    mut mana_query: Query<&mut Node, (With<ManaBar>, Without<HealthBar>, Without<StaminaBar>)>,
    mut health_text_query: Query<&mut Text, (With<HealthText>, Without<StaminaText>, Without<ManaText>)>,
    mut stamina_text_query: Query<&mut Text, (With<StaminaText>, Without<HealthText>, Without<ManaText>)>,
    mut mana_text_query: Query<&mut Text, (With<ManaText>, Without<HealthText>, Without<StaminaText>)>,
    player_query: Query<(&Health, &Stamina, &Mana), With<Actor>>,
) {
    // Find the local player (has Actor component)
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

        // Update health text
        for mut text in &mut health_text_query {
            **text = format!("{:.0} / {:.0}", health.step, health.max);
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

        // Update stamina text
        for mut text in &mut stamina_text_query {
            **text = format!("{:.0} / {:.0}", stamina.step, stamina.max);
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

        // Update mana text
        for mut text in &mut mana_text_query {
            **text = format!("{:.0} / {:.0}", mana.step, mana.max);
        }

        // Only update for the first player found (local player)
        break;
    }
}
