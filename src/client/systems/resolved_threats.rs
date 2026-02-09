use bevy::prelude::*;

use crate::client::components::{ResolvedThreatEntry, ResolvedThreatsContainer};

const ENTRY_SIZE: f32 = 30.0;
const ENTRY_SPACING: f32 = 3.0;
const VERTICAL_OFFSET: f32 = -70.0; // Pixels below center-top (below threat queue)
const MAX_ENTRIES: usize = 5;
const ENTRY_LIFETIME: f32 = 4.0; // Seconds

/// Setup resolved threats container in the HUD
/// Creates a positioned flex column below the threat queue
/// Entries use relative positioning so they naturally stack and reflow on despawn
pub fn setup(
    mut commands: Commands,
    query: Query<Entity, Added<Camera3d>>,
) {
    let camera = query.single().expect("query did not return exactly one result");

    // Container positioned center-top, below threat queue
    // Uses flex column so children stack vertically and reflow when entries despawn
    commands.spawn((
        UiTargetCamera(camera),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Percent(50.0),
            top: Val::Percent(50.0),
            margin: UiRect {
                left: Val::Px(-ENTRY_SIZE / 2.0),
                top: Val::Px(VERTICAL_OFFSET),
                ..default()
            },
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(ENTRY_SPACING),
            align_items: AlignItems::Center,
            ..default()
        },
        ResolvedThreatsContainer,
    ));
}

/// Listen for damage events and spawn resolved threat entries
/// Only shows threats resolved AGAINST the player (incoming damage)
/// Enforces max 5 entries (oldest despawns when 6th is added)
pub fn on_damage_resolved(
    mut commands: Commands,
    container_query: Query<Entity, With<ResolvedThreatsContainer>>,
    entry_query: Query<Entity, With<ResolvedThreatEntry>>,
    input_queues: Res<crate::common::resources::InputQueues>,
    mut event_reader: MessageReader<crate::common::message::Do>,
    time: Res<Time>,
) {
    use crate::common::message::Event as GameEvent;

    let Ok(container) = container_query.single() else {
        return;
    };

    // Local player is the entity with an InputQueue (only one on client)
    let Some(&player_entity) = input_queues.entities().next() else {
        return;
    };

    for event in event_reader.read() {
        if let GameEvent::ApplyDamage { ent, damage, .. } = event.event {
            // Only show threats resolved AGAINST the player (not outgoing damage)
            if ent != player_entity {
                continue;
            }

            // Enforce max entries - despawn oldest if we're at capacity
            let current_entries: Vec<_> = entry_query.iter().collect();
            if current_entries.len() >= MAX_ENTRIES {
                if let Some(&oldest) = current_entries.first() {
                    commands.entity(oldest).despawn();
                }
            }

            // Spawn new resolved threat entry (flex column handles positioning)
            spawn_resolved_threat_entry(
                &mut commands,
                container,
                damage,
                time.elapsed(),
            );
        }
    }
}

/// Update resolved threat entries: fade out and despawn when expired
/// Follows FloatingText pattern from combat_ui.rs
pub fn update_entries(
    mut commands: Commands,
    mut query: Query<(Entity, &ResolvedThreatEntry, &mut BorderColor, &mut BackgroundColor)>,
    time: Res<Time>,
) {
    for (entity, entry, mut border_color, mut bg_color) in &mut query {
        let elapsed = (time.elapsed() - entry.spawn_time).as_secs_f32();

        // Check if lifetime expired
        if elapsed >= entry.lifetime {
            commands.entity(entity).despawn();
            continue;
        }

        // Fade out (alpha based on remaining lifetime)
        // Following FloatingText pattern: alpha = 1.0 - (elapsed / lifetime)
        let alpha = 1.0 - (elapsed / entry.lifetime).clamp(0.0, 1.0);

        // Apply fade to border and background
        // BorderColor in Bevy 0.17 is a struct with top/right/bottom/left fields
        // Use BorderColor::all() to set all sides to the same faded color
        *border_color = BorderColor::all(Color::srgb(0.8, 0.2, 0.2).with_alpha(alpha));
        bg_color.0 = bg_color.0.with_alpha(alpha * 0.8); // Background slightly more transparent
    }
}

/// Spawn a single resolved threat entry as a child of the flex column container
/// Container handles vertical stacking via flex layout - no manual positioning needed
fn spawn_resolved_threat_entry(
    commands: &mut Commands,
    container: Entity,
    damage: f32,
    spawn_time: std::time::Duration,
) {
    commands.entity(container).with_children(|parent| {
        parent.spawn((
            Node {
                width: Val::Px(ENTRY_SIZE),
                height: Val::Px(ENTRY_SIZE),
                border: UiRect::all(Val::Px(2.)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BorderColor::all(Color::srgb(0.8, 0.2, 0.2)),
            BorderRadius::all(Val::Percent(50.)),
            BackgroundColor(Color::srgba(0.3, 0.1, 0.1, 0.8)),
            ResolvedThreatEntry {
                spawn_time,
                lifetime: ENTRY_LIFETIME,
                damage,
            },
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new(format!("{:.0}", damage)),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::srgb(1.0, 1.0, 1.0)),
            ));
        });
    });
}
