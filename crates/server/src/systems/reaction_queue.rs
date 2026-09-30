use bevy::prelude::*;
use common_bevy::{
    components::{reaction_queue::*, ActorAttributes},
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

/// Server system to process Dismiss events: the front threat lands at
/// once exactly as it would when its time ran out
/// (`combat::resolve_threat`), mitigated the same. No recovery, no resource
/// cost.
pub fn process_dismiss(
    mut commands: Commands,
    mut reader: MessageReader<Try>,
    mut query: Query<&mut ReactionQueue>,
    mut writer: MessageWriter<Do>,
) {
    for event in reader.read() {
        let GameEvent::Dismiss { ent } = event.event else {
            continue;
        };
        let Ok(mut queue) = query.get_mut(ent) else {
            continue;
        };
        let Some(threat) = queue_utils::clear_threats(&mut queue, ClearType::First(1)).pop() else {
            continue;
        };
        writer.write(Do { event: GameEvent::ClearQueue { ent, clear_type: ClearType::First(1) } });
        commands.trigger(Try { event: GameEvent::ResolveThreat { ent, threat } });
    }
}
