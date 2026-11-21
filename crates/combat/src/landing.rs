//! What lands on an actor: damage on its health, a change to its status,
//! and a wound's ticks. Each is sent whole to the clients as it changes.

use std::time::Duration;

use bevy::prelude::*;
use common_bevy::{
    components::{resources::Health, status::Status},
    message::{AbilityType, Component, Do, Event as GameEvent, Outcome},
    moment::Moment,
};

/// Takes `damage` from `health`, `target`'s, as `source`'s `ability` and
/// tells every client: the one place damage lands, whatever dealt it.
/// `dot` marks a wound's tick, shown apart from a blow.
pub fn damage(source: Entity, target: Entity, ability: AbilityType, damage: f32, dot: bool, health: &mut Health, writer: &mut MessageWriter<Do>) {
    health.state = (health.state - damage).max(0.0);
    writer.write(Do { event: GameEvent::Resolved { ent: source, ability, target: Some(target), damage, outcome: Outcome::Landed, dot } });
    writer.write(Do { event: GameEvent::Incremental { ent: target, component: Component::Health(*health) } });
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

/// One wound on an actor: damage over time, a tick a second until its
/// ticks run out. Unbuilt: no skill leaves one.
#[derive(Clone, Copy, Debug)]
pub struct Wound {
    pub source: Entity,
    pub ability: AbilityType,
    /// Damage each tick deals
    pub damage: f32,
    /// When its next tick lands, on the game clock
    pub next: Moment,
    pub ticks: u8,
}

/// The wounds on an actor, each ticking on its own.
#[derive(Clone, Component, Debug, Default)]
pub struct Wounds(pub Vec<Wound>);

/// Lands every wound's tick that has come due, past every block, and
/// drops the wounds that have none left. The dead bleed nothing.
pub fn tick_wounds(
    time: Res<Time>,
    runtime: Res<crate::RunTime>,
    mut wounded: Query<(Entity, &mut Wounds, &mut Health)>,
    mut writer: MessageWriter<Do>,
) {
    let now = runtime.now(&time);
    for (ent, mut wounds, mut health) in &mut wounded {
        if wounds.0.is_empty() {
            continue;
        }
        for wound in &mut wounds.0 {
            while wound.ticks > 0 && wound.next <= now && health.state > 0.0 {
                damage(wound.source, ent, wound.ability, wound.damage, true, &mut health, &mut writer);
                wound.ticks -= 1;
                wound.next += Duration::from_secs(1);
            }
        }
        wounds.0.retain(|wound| wound.ticks > 0 && health.state > 0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn a_wound_ticks_once_a_second_until_it_runs_out() {
        let mut world = World::new();
        world.init_resource::<Time>();
        world.init_resource::<crate::RunTime>();
        world.init_resource::<Messages<Do>>();
        let source = world.spawn_empty().id();
        let wound = Wound { source, ability: AbilityType::Frenzy, damage: 10.0, next: Moment::from_millis(1_000), ticks: 3 };
        let wounded = world.spawn((Wounds(vec![wound]), Health::full(100.0))).id();
        let at = |world: &mut World, millis: u128| {
            world.resource_mut::<crate::RunTime>().elapsed_offset = millis;
            world.run_system_once(tick_wounds).unwrap();
            world.get::<Health>(wounded).unwrap().state
        };
        assert_eq!(at(&mut world, 500), 100.0, "nothing before its first tick");
        let first = at(&mut world, 1_000);
        assert!(first < 100.0);
        assert_eq!(at(&mut world, 1_500), first, "one tick a second");
        let last = at(&mut world, 10_000);
        assert!(last < first && last > 0.0, "the rest land and no more");
        assert!(world.get::<Wounds>(wounded).unwrap().0.is_empty(), "spent, it is gone");
    }
}
