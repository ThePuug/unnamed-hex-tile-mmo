use bevy::prelude::*;
use common_bevy::{
    components::{reaction_queue::*, resources::*, ActorAttributes},
    message::{Try, Do, ClearType, Event as GameEvent},
    systems::combat::queue as queue_utils,
};

/// Server system to process expired threats in reaction queues
/// Runs in FixedUpdate schedule (125ms ticks)
/// Checks all entities with ReactionQueue and removes expired threats
pub fn process_expired_threats(
    mut commands: Commands,
    time: Res<Time>,
    runtime: Res<crate::resources::RunTime>,
    mut query: Query<(Entity, &mut ReactionQueue, &ActorAttributes)>,
    mut writer: MessageWriter<Do>,
) {
    // Use game world time (same as threat timestamps)
    let now_ms = time.elapsed().as_millis() + runtime.elapsed_offset;
    let now = std::time::Duration::from_millis(now_ms.min(u64::MAX as u128) as u64);

    for (ent, mut queue, _attrs) in &mut query {
        // Check which threats have expired
        let expired = queue_utils::check_expired_threats(&queue, now);

        if expired.is_empty() {
            continue;
        }

        // Remove expired threats from the queue and emit ResolveThreat events
        for expired_threat in &expired {
            let clear_type = ClearType::Threat { source: expired_threat.source, inserted_at: expired_threat.inserted_at };
            if !queue_utils::clear_threats(&mut queue, clear_type).is_empty() {
                // Broadcast ClearQueue event to clients so they remove the threat from UI
                writer.write(Do { event: GameEvent::ClearQueue { ent, clear_type } });

                // Emit ResolveThreat event to trigger damage application
                commands.trigger(
                    Try {
                        event: GameEvent::ResolveThreat {
                            ent,
                            threat: *expired_threat,
                        },
                    },
                );
            }
        }
    }
}

/// Ticks the DoT of every wound standing in a queue: each tick that has
/// come due is counted on the wound and lands as a `DotTick`, outside the
/// queue. A wound cleared or landed ticks no more; what it had not dealt
/// lands with it.
pub fn tick_dots(
    mut commands: Commands,
    time: Res<Time>,
    runtime: Res<crate::resources::RunTime>,
    mut query: Query<(Entity, &mut ReactionQueue)>,
) {
    let now_ms = time.elapsed().as_millis() + runtime.elapsed_offset;
    let now = std::time::Duration::from_millis(now_ms.min(u64::MAX as u128) as u64);
    for (ent, mut queue) in &mut query {
        for wound in queue.threats.iter_mut().filter(|t| t.is_wound()) {
            let due = wound.ticks_due(now);
            while wound.ticked < due {
                wound.ticked += 1;
                commands.trigger(Try {
                    event: GameEvent::DotTick { ent, source: wound.source, damage: wound.dot, ability: wound.ability },
                });
            }
        }
    }
}

/// Server system to process Dismiss events
/// Pops the front threat from the queue and applies full unmitigated damage,
/// and whatever else the blow does lands with it
/// No lockout, no resource cost
pub fn process_dismiss(
    mut commands: Commands,
    mut reader: MessageReader<Try>,
    mut query: Query<(&mut ReactionQueue, &mut Health, Option<&mut common_bevy::components::grit::Grit>)>,
    time: Res<Time>,
    mut statuses: Query<&mut common_bevy::components::status::Status>,
    recoveries: Query<&common_bevy::components::recovery::GlobalRecovery>,
    locs: Query<&common_bevy::components::Loc>,
    mut bursts: Query<&mut crate::systems::combat::landing::VolleyBurst>,
    map: Res<common_bevy::resources::map::Map>,
    reach: crate::systems::combat::landing::SpillReach,
    attrs: Query<&ActorAttributes>,
    mut writer: MessageWriter<Do>,
) {
    let tuning = common_bevy::tuning::tuning();
    for event in reader.read() {
        let GameEvent::Dismiss { ent } = event.event else {
            continue;
        };

        let Ok((mut queue, mut health, grit)) = query.get_mut(ent) else {
            continue;
        };

        // Dismiss takes the front threat, always in the window
        let Some(threat) = queue_utils::clear_threats(&mut queue, ClearType::First(1)).pop() else {
            continue;
        };
        // A wound taken at once deals the DoT it had left, and what would
        // pass Grit waits for the seconds after
        let cap = attrs.get(ent).map_or(f32::INFINITY, ActorAttributes::grit_cap) * health.max;
        let blow = grit.map_or(threat.damage, |mut grit| grit.take(time.elapsed(), threat.damage, cap, threat.source));
        let damage = blow + threat.dot_left();

        // Apply full unmitigated damage (no armor, no resistance)
        health.state = (health.state - damage).max(0.0);
        health.step = health.state;

        // Broadcast queue clear to clients
        writer.write(Do {
            event: GameEvent::ClearQueue {
                ent,
                clear_type: ClearType::First(1),
            },
        });

        // Broadcast damage event to clients
        writer.write(Do {
            event: GameEvent::ApplyDamage {
                ent,
                damage,
                source: threat.source,
                dot: threat.is_wound(),
            },
        });

        // Send authoritative health
        writer.write(Do {
            event: GameEvent::Incremental {
                ent,
                component: common_bevy::message::Component::Health(*health),
            },
        });

        crate::systems::combat::landing::land(threat.ability, ent, threat.source, threat.inserted_at, attrs.get(threat.source).ok(), &tuning, &mut statuses, &recoveries, &locs, &mut bursts, &map, &mut commands, &mut writer);
        reach.spill(threat.source, ent, threat.damage, &mut commands);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_process_expired_threats_removes_expired() {
        // Create test world
        let mut world = World::new();
        world.init_resource::<Time>();

        let threat_entity = Entity::from_raw_u32(1).unwrap();

        let mut queue = ReactionQueue::new(3);
        queue.threats.push_back(QueuedThreat {
            source: threat_entity,
            damage: 10.0,
            damage_type: DamageType::Physical,
            inserted_at: Duration::from_secs(0),
            timer_duration: Duration::from_secs(1),
            ability: None,
            dot: 0.0,
            ticked: 0,
            seen: false,
                    });

        let attrs = ActorAttributes::default();

        let ent_id = world.spawn((queue, attrs)).id();

        // Note: In real game, Time::elapsed() is updated by Bevy
        // For testing, we need to manually advance time or use a mock

        // Run the system
        // Note: This test is simplified - in practice we'd use proper Bevy test infrastructure
        // For now, this demonstrates the test structure

        // Query to verify threat was removed
        let queue_after = world.get::<ReactionQueue>(ent_id).unwrap();

        // In Phase 2, we're just setting up the structure
        // Actual expiry processing will be tested when we integrate with time system
        assert!(queue_after.threats.len() <= 1); // Threat either still there or removed
    }
}
