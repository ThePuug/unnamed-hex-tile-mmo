use bevy::prelude::*;
use bevy::input::mouse::{MouseWheel, MouseScrollUnit};
use bevy::picking::pointer::PointerId;
use bevy::picking::hover::HoverMap;
use chrono::Local;

use crate::client::components::{CombatLogContent, CombatLogEntry, CombatLogPanel};
use crate::common::components::entity_type::EntityType;

const PANEL_WIDTH: f32 = 400.0;
const PANEL_HEIGHT: f32 = 250.0;
const PANEL_MARGIN: f32 = 10.0;
const FONT_SIZE: f32 = 12.0;
const MAX_ENTRIES: usize = 50;
const LINE_HEIGHT: f32 = 20.0; // Pixels per line scroll (for MouseScrollUnit::Line)

/// Setup combat log panel in bottom-left corner
/// Creates a scrollable panel that will display timestamped combat events
pub fn setup(
    mut commands: Commands,
    query: Query<Entity, Added<Camera3d>>,
) {
    let camera = query.single().expect("query did not return exactly one result");

    // Combat log panel - bottom-left corner with vertical scrolling
    commands.spawn((
        UiTargetCamera(camera),
        Node {
            position_type: PositionType::Absolute,
            width: Val::Px(PANEL_WIDTH),
            height: Val::Px(PANEL_HEIGHT),
            left: Val::Px(PANEL_MARGIN),
            bottom: Val::Px(PANEL_MARGIN),
            padding: UiRect::all(Val::Px(10.)),
            flex_direction: FlexDirection::Column,
            overflow: Overflow::scroll_y(), // Enable vertical scrolling
            border_radius: BorderRadius::all(Val::Px(4.)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.1, 0.1, 0.1, 0.85)),
        BorderColor::all(Color::srgba(0.5, 0.5, 0.5, 0.8)),
        ScrollPosition::default(),
        CombatLogPanel,
    ))
    .with_children(|parent| {
        // Scrollable content container
        parent.spawn((
            Node {
                flex_direction: FlexDirection::Column,
                width: Val::Percent(100.),
                ..default()
            },
            CombatLogContent,
        ));
    });
}

/// Log damage events to the combat log
/// Triggered by Do::ApplyDamage events
pub fn on_damage_applied(
    mut commands: Commands,
    content_query: Query<Entity, With<CombatLogContent>>,
    entry_query: Query<Entity, With<CombatLogEntry>>,
    entity_type_query: Query<&EntityType>,
    input_queues: Res<crate::common::resources::InputQueues>,
    mut event_reader: MessageReader<crate::common::message::Do>,
) {
    use crate::common::message::Event as GameEvent;

    let Ok(content) = content_query.single() else {
        return;
    };

    // Local player is the entity with an InputQueue (only one on client)
    let player_entity = input_queues.entities().next().copied();

    for event in event_reader.read() {
        if let GameEvent::ApplyDamage { ent, damage, source } = event.event {
            // Enforce max entries - despawn oldest if at capacity
            let current_entries: Vec<_> = entry_query.iter().collect();
            if current_entries.len() >= MAX_ENTRIES {
                if let Some(&oldest) = current_entries.first() {
                    commands.entity(oldest).despawn();
                }
            }

            // Resolve entity names using EntityType::display_name()
            let source_name = entity_type_query.get(source)
                .map(|et| et.display_name())
                .unwrap_or("Unknown");

            let target_name = entity_type_query.get(ent)
                .map(|et| et.display_name())
                .unwrap_or("Unknown");

            // Determine if this is player dealing damage or taking damage
            let is_player_damage = player_entity.map_or(false, |p| p == source);

            // Get current timestamp
            let timestamp = Local::now().format("%H:%M:%S").to_string();

            // Format entry text
            let entry_text = format!(
                "[{}] {} → {}: {:.0} dmg (Physical)",
                timestamp, source_name, target_name, damage
            );

            // Determine color based on whether player dealt or took damage
            let color = if is_player_damage {
                Color::srgb(1.0, 0.3, 0.3) // Red - damage dealt
            } else {
                Color::srgb(1.0, 0.6, 0.0) // Orange - damage taken
            };

            // Spawn log entry
            spawn_log_entry(&mut commands, content, entry_text, timestamp, is_player_damage, color);
        }
    }
}

/// Log queue clear events to the combat log
/// Triggered by Do::ClearQueue events
pub fn on_queue_cleared(
    mut commands: Commands,
    content_query: Query<Entity, With<CombatLogContent>>,
    entry_query: Query<Entity, With<CombatLogEntry>>,
    entity_type_query: Query<&EntityType>,
    mut event_reader: MessageReader<crate::common::message::Do>,
) {
    use crate::common::message::Event as GameEvent;

    let Ok(content) = content_query.single() else {
        return;
    };

    for event in event_reader.read() {
        if let GameEvent::ClearQueue { ent, .. } = event.event {
            // Enforce max entries
            let current_entries: Vec<_> = entry_query.iter().collect();
            if current_entries.len() >= MAX_ENTRIES {
                if let Some(&oldest) = current_entries.first() {
                    commands.entity(oldest).despawn();
                }
            }

            // Resolve entity name using EntityType::display_name()
            let entity_name = entity_type_query.get(ent)
                .map(|et| et.display_name())
                .unwrap_or("Unknown");

            // Get current timestamp
            let timestamp = Local::now().format("%H:%M:%S").to_string();

            // Format entry text
            let entry_text = format!("[{}] {} cleared queue", timestamp, entity_name);

            // Gray color for clear events
            let color = Color::srgb(0.6, 0.6, 0.6);

            // Spawn log entry
            spawn_log_entry(&mut commands, content, entry_text, timestamp, false, color);
        }
    }
}

/// Maintain log: enforce max entries
/// Runs every frame to check for overflow
pub fn maintain_log(
    mut commands: Commands,
    entry_query: Query<Entity, With<CombatLogEntry>>,
) {
    // Enforce max entries (safety check in case events spawn too fast)
    let entries: Vec<_> = entry_query.iter().collect();
    if entries.len() > MAX_ENTRIES {
        let excess = entries.len() - MAX_ENTRIES;
        for &entity in entries.iter().take(excess) {
            commands.entity(entity).despawn();
        }
    }
}

/// Handle mouse wheel scrolling over the combat log panel
/// Uses HoverMap + MessageReader<MouseWheel> following Bevy's official scroll example
pub fn handle_scroll(
    mut mouse_wheel_events: MessageReader<MouseWheel>,
    mut panel_query: Query<(Entity, &mut ScrollPosition, &ComputedNode), With<CombatLogPanel>>,
    hover_map: Res<HoverMap>,
    parent_query: Query<&ChildOf>,
) {
    let Ok((panel_entity, mut scroll_pos, computed_node)) = panel_query.single_mut() else {
        return;
    };

    // Check if any hovered entity is the panel or a descendant of it
    let hovered_entities = hover_map.get(&PointerId::Mouse);
    let is_over_panel = hovered_entities.map_or(false, |map| {
        map.keys().any(|&entity| {
            let mut current = entity;
            if current == panel_entity {
                return true;
            }
            while let Ok(child_of) = parent_query.get(current) {
                let parent = child_of.parent();
                if parent == panel_entity {
                    return true;
                }
                current = parent;
            }
            false
        })
    });

    if !is_over_panel {
        return;
    }

    for event in mouse_wheel_events.read() {
        let mut delta = event.y;
        if event.unit == MouseScrollUnit::Line {
            delta *= LINE_HEIGHT;
        }

        scroll_pos.y -= delta;

        let max_scroll = (computed_node.content_size().y - computed_node.size().y)
            * computed_node.inverse_scale_factor();
        scroll_pos.y = scroll_pos.y.clamp(0.0, max_scroll.max(0.0));
    }
}

/// Auto-scroll to bottom when new entries are added
/// Only runs when new entries are detected
pub fn auto_scroll_to_bottom(
    entry_query: Query<Entity, Added<CombatLogEntry>>,
    mut panel_query: Query<&mut ScrollPosition, With<CombatLogPanel>>,
) {
    // Only auto-scroll if new entries were added this frame
    if entry_query.is_empty() {
        return;
    }

    if let Ok(mut scroll_pos) = panel_query.single_mut() {
        // Scroll to bottom to show latest entries
        scroll_pos.y = f32::MAX;
    }
}

/// Spawn a single combat log entry
fn spawn_log_entry(
    commands: &mut Commands,
    content: Entity,
    text: String,
    timestamp: String,
    is_player_damage: bool,
    color: Color,
) {
    commands.entity(content).with_children(|parent| {
        parent.spawn((
            Text::new(text),
            TextFont {
                font_size: FONT_SIZE,
                ..default()
            },
            TextColor(color),
            CombatLogEntry {
                timestamp,
                is_player_damage,
            },
        ));
    });
}

