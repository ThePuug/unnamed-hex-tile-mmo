use bevy::prelude::*;

use crate::components::{ResolvedThreatEntry, ResolvedThreatsContainer};
use crate::systems::threat_icons::{self, severity_rgb};
use common_bevy::components::resources::Health;

const ENTRY_SIZE: f32 = 30.0;
const ENTRY_SPACING: f32 = 3.0;
const MAX_ENTRIES: usize = 5;
const ENTRY_LIFETIME: f32 = 4.0; // Seconds

/// Listen for damage events and spawn resolved threat entries
/// Only shows threats resolved AGAINST the viewed actor (incoming damage)
/// Enforces max 5 entries (oldest despawns when 6th is added)
pub fn on_damage_resolved(
    mut commands: Commands,
    container_query: Query<Entity, With<ResolvedThreatsContainer>>,
    entry_query: Query<Entity, With<ResolvedThreatEntry>>,
    children_query: Query<&Children>,
    viewed: Query<(Entity, &Health), With<crate::components::Viewed>>,
    mut event_reader: MessageReader<common_bevy::message::Do>,
    time: Res<Time>,
) {
    use common_bevy::message::Event as GameEvent;

    let Ok(container) = container_query.single() else {
        return;
    };

    let Some((player_entity, health)) = viewed.iter().next() else {
        return;
    };
    let max_health = health.max;

    // Oldest first, as the stack holds them, with each spawned below added
    // behind, so several landing in one frame each take a different one out
    let mut standing: std::collections::VecDeque<Entity> = children_query
        .get(container)
        .map(|children| children.iter().filter(|e| entry_query.contains(*e)).collect())
        .unwrap_or_default();

    for event in event_reader.read() {
        if let GameEvent::ApplyDamage { ent, damage, dot, .. } = event.event {
            // Only show threats resolved AGAINST the player (not outgoing damage)
            if ent != player_entity {
                continue;
            }

            // Enforce max entries: the oldest goes, unless it expired this frame
            while standing.len() >= MAX_ENTRIES {
                if let Some(oldest) = standing.pop_front() {
                    commands.entity(oldest).try_despawn();
                }
            }

            let severity = if max_health > 0.0 {
                (damage / max_health).clamp(0.0, 1.0)
            } else {
                1.0
            };

            // A blow in its severity's colour, a DoT tick in the DoT colour
            let rgb = if dot {
                threat_icons::DOT_COLOR.to_srgba()
            } else {
                let (r, g, b) = severity_rgb(severity);
                Color::srgb(r, g, b).to_srgba()
            };
            standing.push_back(spawn_resolved_threat_entry(
                &mut commands,
                container,
                damage,
                (rgb.red, rgb.green, rgb.blue),
                time.elapsed(),
            ));
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

        // Check if lifetime expired; the cap may have taken it this frame
        if elapsed >= entry.lifetime {
            commands.entity(entity).try_despawn();
            continue;
        }

        let (r, g, b) = entry.rgb;

        // Fade in over 0.15s, then out over the rest of its lifetime
        let alpha = if elapsed < 0.15 {
            elapsed / 0.15
        } else {
            1.0 - ((elapsed - 0.15) / (entry.lifetime - 0.15)).clamp(0.0, 1.0)
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

/// Spawn a single resolved threat entry as a child of the flex column container
/// Container handles vertical stacking via flex layout - no manual positioning needed
fn spawn_resolved_threat_entry(
    commands: &mut Commands,
    container: Entity,
    damage: f32,
    rgb: (f32, f32, f32),
    spawn_time: std::time::Duration,
) -> Entity {
    let (r, g, b) = rgb;

    let entry = commands
        .spawn((
            Node {
                width: Val::Px(ENTRY_SIZE),
                height: Val::Px(ENTRY_SIZE),
                border: UiRect::all(Val::Px(2.)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border_radius: BorderRadius::all(Val::Percent(50.)),
                ..default()
            },
            // Start invisible (alpha 0); it fades in
            BorderColor::all(Color::srgba(r, g, b, 0.0)),
            BackgroundColor(Color::srgba(r * 0.35, g * 0.35, b * 0.35, 0.0)),
            ResolvedThreatEntry {
                spawn_time,
                lifetime: ENTRY_LIFETIME,
                rgb,
            },
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new(format!("{:.0}", damage)),
                TextFont {
                    font_size: FontSize::Px(14.0),
                    ..default()
                },
                TextColor(Color::srgba(1.0, 1.0, 1.0, 0.0)),
            ));
        })
        .id();
    commands.entity(container).add_child(entry);
    entry
}
