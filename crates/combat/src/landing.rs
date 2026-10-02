//! What using an ability lays on an actor beside damage: its recovery, and
//! the stride it breaks by striking across its own line. Each is sent whole
//! to the clients as it changes.

use bevy::prelude::*;
use common_bevy::{
    components::{
        recovery::GlobalRecovery,
        status::{Status, Timed},
    },
    message::{Component, Do, Try, Event as GameEvent},
};
use common_bevy::tuning::Tuning;

/// A strike across its striker's line breaks its stride: `Tuning::stride_pace`
/// of its speed for one base interval.
pub fn stumble(
    trigger: On<Try>,
    tuning: Res<Tuning>,
    mut statuses: Query<&mut Status>,
    mut commands: Commands,
    mut writer: MessageWriter<Do>,
) {
    let Try { event: GameEvent::Stumble { ent } } = trigger.event() else { return };
    update(*ent, &mut statuses, &mut commands, &mut writer, |status| {
        status.stride = Some(Timed { pace: tuning.stride_pace, remaining: tuning.base_interval });
    });
}

/// Starts `recovery` on `ent`, in place of any it was in, and sends the
/// whole of it.
pub fn recover(ent: Entity, recovery: GlobalRecovery, commands: &mut Commands, writer: &mut MessageWriter<Do>) {
    if let Ok(mut entity) = commands.get_entity(ent) {
        entity.try_insert(recovery);
    }
    writer.write(Do { event: GameEvent::Incremental { ent, component: Component::Recovery(recovery) } });
}

/// Changes `ent`'s status by `change`, giving it one if it has none, and
/// sends the whole of it.
pub fn update(
    ent: Entity,
    statuses: &mut Query<&mut Status>,
    commands: &mut Commands,
    writer: &mut MessageWriter<Do>,
    change: impl FnOnce(&mut Status),
) {
    let status = match statuses.get_mut(ent) {
        Ok(mut status) => {
            change(&mut status);
            *status
        }
        Err(_) => {
            let Ok(mut entity) = commands.get_entity(ent) else { return };
            let mut status = Status::default();
            change(&mut status);
            entity.try_insert(status);
            status
        }
    };
    writer.write(Do { event: GameEvent::Incremental { ent, component: Component::Status(status) } });
}
