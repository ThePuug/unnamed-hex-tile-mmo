use bevy::prelude::*;

use crate::client::components::{ResolvedThreatEntry, ResolvedThreatsContainer};
use crate::client::systems::threat_icons::{self, severity_rgb};
use crate::common::components::reaction_queue::ReactionQueue;
use crate::common::components::resources::Health;

const ENTRY_SIZE: f32 = 30.0;
const ENTRY_SPACING: f32 = 3.0;
const VERTICAL_OFFSET: f32 = -70.0; // Pixels below center-top (below threat queue)
const MAX_ENTRIES: usize = 5;
const ENTRY_LIFETIME: f32 = 4.0; // Seconds
const POP_APPEAR_DELAY: f32 = 0.4; // Synced with pop travel duration

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
            flex_direction: FlexDirection::ColumnReverse,
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
    player_health: Query<&Health, With<crate::common::components::Actor>>,
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

    let max_health = player_health.iter().next().map(|h| h.max).unwrap_or(100.0);

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

            let severity = if max_health > 0.0 {
                (damage / max_health).clamp(0.0, 1.0)
            } else {
                1.0
            };

            // Spawn new resolved threat entry (flex column handles positioning)
            spawn_resolved_threat_entry(
                &mut commands,
                container,
                damage,
                severity,
                POP_APPEAR_DELAY,
                time.elapsed(),
            );
        }
    }
}

/// Update resolved threat entries: delayed appearance, fade out, and despawn when expired
pub fn update_entries(
    mut commands: Commands,
    mut query: Query<(Entity, &ResolvedThreatEntry, &mut BorderColor, &mut BackgroundColor, &Children)>,
    mut text_query: Query<&mut TextColor>,
    time: Res<Time>,
) {
    for (entity, entry, mut border_color, mut bg_color, children) in &mut query {
        let elapsed = (time.elapsed() - entry.spawn_time).as_secs_f32();

        // Check if lifetime expired
        if elapsed >= entry.lifetime {
            commands.entity(entity).despawn();
            continue;
        }

        let (r, g, b) = severity_rgb(entry.severity);

        // Delayed appearance + fade-in + fade-out
        let alpha = if elapsed < entry.appear_delay {
            // Hidden while pop is traveling
            0.0
        } else if elapsed < entry.appear_delay + 0.15 {
            // Fade in over 0.15s
            (elapsed - entry.appear_delay) / 0.15
        } else {
            // Normal fade-out over remaining lifetime
            let fade_elapsed = elapsed - entry.appear_delay - 0.15;
            let fade_lifetime = entry.lifetime - entry.appear_delay - 0.15;
            1.0 - (fade_elapsed / fade_lifetime).clamp(0.0, 1.0)
        };

        *border_color = BorderColor::all(Color::srgba(r, g, b, alpha));
        bg_color.0 = Color::srgba(r * 0.35, g * 0.35, b * 0.35, alpha * 0.8);

        // Fade text children too
        for child in children.iter() {
            if let Ok(mut text_color) = text_query.get_mut(child) {
                text_color.0 = Color::srgba(1.0, 1.0, 1.0, alpha);
            }
        }
    }
}

/// Keep the resolved stack horizontally aligned with the queue front icon
/// so the pop animation lands directly into the stack.
/// Only runs when ReactionQueue is mutated (capacity/threat changes).
pub fn sync_container_position(
    mut container_query: Query<&mut Node, With<ResolvedThreatsContainer>>,
    player_query: Query<&ReactionQueue, (With<crate::common::components::Actor>, Changed<ReactionQueue>)>,
) {
    let Some(queue) = player_query.iter().next() else {
        return;
    };
    let Ok(mut node) = container_query.single_mut() else {
        return;
    };

    // Pop ends at: front_icon_x_offset(window) + (ICON_SIZE - final_size) / 2
    // final_size = ICON_SIZE * 0.6 = 30 = ENTRY_SIZE, so offset = (ICON_SIZE - ENTRY_SIZE) / 2
    let x = threat_icons::front_icon_x_offset(queue.window_size)
        + (threat_icons::ICON_SIZE - ENTRY_SIZE) / 2.0;
    node.margin.left = Val::Px(x);
}

/// Spawn a single resolved threat entry as a child of the flex column container
/// Container handles vertical stacking via flex layout - no manual positioning needed
fn spawn_resolved_threat_entry(
    commands: &mut Commands,
    container: Entity,
    damage: f32,
    severity: f32,
    appear_delay: f32,
    spawn_time: std::time::Duration,
) {
    let (r, g, b) = severity_rgb(severity);

    commands.entity(container).with_children(|parent| {
        parent.spawn((
            Node {
                width: Val::Px(ENTRY_SIZE),
                height: Val::Px(ENTRY_SIZE),
                border: UiRect::all(Val::Px(2.)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border_radius: BorderRadius::all(Val::Percent(50.)),
                ..default()
            },
            // Start invisible (alpha 0) — appear_delay handles visibility
            BorderColor::all(Color::srgba(r, g, b, 0.0)),
            BackgroundColor(Color::srgba(r * 0.35, g * 0.35, b * 0.35, 0.0)),
            ResolvedThreatEntry {
                spawn_time,
                lifetime: ENTRY_LIFETIME,
                damage,
                severity,
                appear_delay,
            },
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new(format!("{:.0}", damage)),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::srgba(1.0, 1.0, 1.0, 0.0)),
            ));
        });
    });
}
