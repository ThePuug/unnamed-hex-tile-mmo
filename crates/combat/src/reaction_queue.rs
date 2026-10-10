use bevy::prelude::*;
use common_bevy::{
    components::{reaction_queue::*, ActorAttributes},
    message::{Try, Do, ClearType, Event as GameEvent},
    systems::combat::queue as queue_utils,
};

/// Lands every threat whose time has run (`queue::check_expired_threats`).
/// Runs each frame after `abilities::use_abilities`, so a press come due
/// this frame takes what lands in its band before any of it lands.
pub fn process_expired_threats(
    mut commands: Commands,
    time: Res<Time>,
    runtime: Res<crate::RunTime>,
    mut query: Query<(Entity, &mut ReactionQueue), With<ActorAttributes>>,
    mut writer: MessageWriter<Do>,
) {
    let now = runtime.now(&time);

    for (ent, mut queue) in &mut query {
        let expired = queue_utils::check_expired_threats(&queue, now);

        if expired.is_empty() {
            continue;
        }

        for expired_threat in &expired {
            let clear_type = ClearType::Threat { source: expired_threat.source, inserted_at: expired_threat.inserted_at };
            if !queue_utils::clear_threats(&mut queue, clear_type).is_empty() {
                writer.write(Do { event: GameEvent::ClearQueue { ent, clear_type } });

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
    runtime: Res<crate::RunTime>,
    mut query: Query<(Entity, &mut ReactionQueue)>,
) {
    let now = runtime.now(&time);
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
