use bevy::prelude::*;
use common_bevy::{
    components::{reaction_queue::*, resources::*},
    message::{Do, Event as GameEvent},
    systems::combat::queue as queue_utils,
};
use common_bevy::tuning::Tuning;

/// Client system to handle InsertThreat events
/// Inserts threats into the visual reaction queue for display
/// No deduplication needed - we don't predict threat insertions
pub fn handle_insert_threat(
    mut reader: MessageReader<Do>,
    mut query: Query<&mut ReactionQueue>,
    time: Res<Time>,
    server: Res<crate::resources::Server>,
) {
    for event in reader.read() {
        if let GameEvent::InsertThreat { ent, threat } = event.event {
            if let Ok(mut queue) = query.get_mut(ent) {
                // Calculate current server time
                let client_now = time.elapsed().as_millis();
                let server_now_ms = server.current_time(client_now);
                let server_now = std::time::Duration::from_millis(server_now_ms.min(u64::MAX as u128) as u64);

                // Insert always succeeds (unbounded queue)
                queue_utils::insert_threat(&mut queue, threat, server_now);
            }
        }
    }
}

/// Client system to handle ApplyDamage events
/// Removes the corresponding threat from the queue and spawns floating damage numbers
/// NOTE: Does NOT update health - server sends authoritative health via Incremental{Health}
/// Only spawns damage numbers over NPCs (outgoing damage), not over player (incoming damage shown in resolved threats)
pub fn handle_apply_damage(
    mut commands: Commands,
    mut reader: MessageReader<Do>,
    _health_query: Query<&mut Health>,
    _queue_query: Query<&ReactionQueue>,
    viewed: Query<Entity, With<crate::components::Viewed>>,
    transform_query: Query<&Transform>,
    time: Res<Time>,
) {
    // What lands on the actor the client sees as shows in its resolved stack
    let player_entity = viewed.single().ok();

    for event in reader.read() {
        if let GameEvent::ApplyDamage { ent, damage, dot, .. } = event.event {
            // Don't remove from queue - ClearQueue event already did that!
            // This was causing double-removal and queue desync
            // (ApplyDamage is for damage display only, not queue management)

            // Skip player incoming damage - shown via resolved threats stack
            let is_player_target = player_entity.map_or(false, |p| p == ent);
            if is_player_target {
                continue;
            }

            // Spawn floating damage number over the NPC
            // Entity stays alive for 3s in death pose, so Transform is available
            let Ok(transform) = transform_query.get(ent) else { continue; };
            let world_pos = transform.translation + Vec3::new(0.0, 2.5, 0.0);

            commands.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    ..default()
                },
                Text::new(format!("{:.0}", damage)),
                TextFont {
                    font_size: FontSize::Px(32.0),
                    ..default()
                },
                TextColor(if dot { crate::systems::threat_icons::DOT_COLOR } else { Color::WHITE }),
                TextLayout::justify(Justify::Center),
                crate::components::FloatingText {
                    spawn_time: time.elapsed(),
                    world_position: world_pos,
                    lifetime: 1.5,
                    velocity: 1.0,
                },
            ));
        }
    }
}

/// Client system to handle ClearQueue events from server
/// Confirms queue clears (may be redundant with prediction but ensures sync)
pub fn handle_clear_queue(
    mut reader: MessageReader<Do>,
    mut query: Query<&mut ReactionQueue>,
) {
    for event in reader.read() {
        if let GameEvent::ClearQueue { ent, clear_type } = event.event {
            if let Ok(mut queue) = query.get_mut(ent) {
                // Clear threats using message ClearType directly
                queue_utils::clear_threats(&mut queue, clear_type);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common_bevy::{components::ActorAttributes, message::{AbilityType, ClearType}};
    use std::time::Duration;

    #[test]
    fn a_threat_clears_after_its_source_is_gone_here() {
        let tuning = Tuning::DEFAULT;
        let mut app = App::new();
        app.add_message::<Do>();
        app.init_resource::<Time>();
        app.insert_resource(crate::resources::Server::default());
        app.init_resource::<crate::resources::EntityMap>();
        app.add_systems(Update, (handle_insert_threat, handle_clear_queue).chain());

        let player = app.world_mut().spawn((ReactionQueue::default(), ActorAttributes::default())).id();
        let attacker = app.world_mut().spawn(ActorAttributes::default()).id();
        let on_server = Entity::from_raw_u32(9_000).unwrap();
        app.world_mut().resource_mut::<crate::resources::EntityMap>().insert(attacker, on_server);

        let threat = queue_utils::create_threat(
            &tuning,
            on_server, &ActorAttributes::default(), &ActorAttributes::default(),
            50.0, Some(AbilityType::Frenzy), Duration::from_secs(10), 0.0, 0.0,
        );
        app.world_mut().write_message(Do { event: GameEvent::InsertThreat { ent: player, threat } });
        app.update();
        assert_eq!(app.world().get::<ReactionQueue>(player).unwrap().threats.len(), 1);

        // The attacker dies and is despawned here before its threat lands
        app.world_mut().resource_mut::<crate::resources::EntityMap>().remove_by_left(&attacker);
        app.world_mut().despawn(attacker);
        let clear_type = ClearType::Threat { source: on_server, inserted_at: threat.inserted_at };
        app.world_mut().write_message(Do { event: GameEvent::ClearQueue { ent: player, clear_type } });
        app.update();
        assert!(app.world().get::<ReactionQueue>(player).unwrap().threats.is_empty(), "the landing clears it all the same");
    }
}
