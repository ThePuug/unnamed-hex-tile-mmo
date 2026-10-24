//! A player's horse: it rides out of combat, and anything but riding puts
//! it on its feet. The ride is `Status::mounted`, sent whole to every
//! client that sees the actor, so each moves and draws it at the horse's
//! pace.

use bevy::prelude::*;

use common_bevy::{
    components::{resources::CombatState, status::Status},
    message::{Component, Event, *},
};

use crate::systems::gathering::Working;

/// Gets a player on its horse or off it as its client asks: on only out of
/// combat and away from work.
pub fn try_mount(
    mut reader: MessageReader<Try>,
    mut writer: MessageWriter<Do>,
    mut commands: Commands,
    mut statuses: Query<&mut Status>,
    riders: Query<(Option<&CombatState>, Has<Working>)>,
) {
    for message in reader.read() {
        let Try { event: Event::Mount { ent, on } } = message else { continue };
        let (ent, on) = (*ent, *on);
        let Ok((combat, working)) = riders.get(ent) else { continue };
        if on && (combat.is_some_and(|c| c.in_combat) || working) {
            info!("mount: {ent} refused, in combat or at work");
            continue;
        }
        if statuses.get(ent).map_or(false, |s| s.mounted) == on {
            continue;
        }
        info!("mount: {ent} gets {}", if on { "on" } else { "off" });
        combat::landing::update(ent, &mut statuses, &mut commands, &mut writer, |status| status.mounted = on);
    }
}

/// Puts a rider on its feet as it enters combat, uses a skill or starts
/// at a gather. Read off the `Do`s those send, so its own goes by
/// `Commands`: the bus is not both read and written by one system.
pub fn dismount(
    mut reader: MessageReader<Do>,
    mut commands: Commands,
    mut statuses: Query<&mut Status>,
) {
    for message in reader.read() {
        let ent = match message.event {
            Event::UseAbility { ent, .. } | Event::Activity { ent, activity: Some(_) } => ent,
            Event::Incremental { ent, component: Component::CombatState(CombatState { in_combat: true, .. }) } => ent,
            _ => continue,
        };
        let Ok(mut status) = statuses.get_mut(ent) else { continue };
        if status.mounted {
            status.mounted = false;
            commands.write_message(Do { event: Event::Incremental { ent, component: Component::Status(*status) } });
        }
    }
}
