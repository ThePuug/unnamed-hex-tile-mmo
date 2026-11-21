//! What the server says of skills under way, kept on the actors they are
//! about: the skill each is activating (`Activating`) and each skill
//! recharging (`Recharges`), both counted on the server's clock
//! (`resources::Server::now`). The server starts and ends every one; the
//! client only counts.

use std::collections::HashMap;

use bevy::prelude::*;
use common_bevy::{
    components::activation::{Activating, Activation, Recharge, Recharges},
    message::{AbilityType, Do, Event as GameEvent},
    moment::Moment,
};

/// The activation last heard of each entity. The server tells one again,
/// as it began, to a client that meets its actor late, and a client may
/// hear both: whatever starts something on an activation asks first
/// whether it is new.
#[derive(Default)]
pub struct Heard(HashMap<Entity, (AbilityType, Moment)>);

impl Heard {
    /// Whether `activation` by `ent` is one not heard before, taking it as
    /// heard
    pub fn fresh(&mut self, ent: Entity, activation: &Activation) -> bool {
        let heard = (activation.ability, activation.began);
        self.0.insert(ent, heard) != Some(heard)
    }
}

/// Takes each skill begun, ended and recharging as the server tells it. A
/// skill that takes hold at once has no activation to hold. An activation
/// ends on the word of how it resolved, or as its time runs out here,
/// whichever comes first.
pub fn track(
    mut reader: MessageReader<Do>,
    mut activating: Query<&mut Activating>,
    mut recharges: Query<&mut Recharges>,
    mut heard: Local<Heard>,
    server: Res<crate::resources::Server>,
    time: Res<Time>,
) {
    for message in reader.read() {
        match message.event {
            GameEvent::Activate { ent, activation } if !activation.length.is_zero() && heard.fresh(ent, &activation) => {
                if let Ok(mut activating) = activating.get_mut(ent) {
                    activating.0 = Some(activation);
                }
            }
            GameEvent::Resolved { ent, ability, .. } => {
                if let Ok(mut activating) = activating.get_mut(ent) {
                    if activating.0.is_some_and(|activation| activation.ability == ability) {
                        activating.0 = None;
                    }
                }
            }
            GameEvent::Recharge { ent, ability, began, length } => {
                if let Ok(mut recharges) = recharges.get_mut(ent) {
                    recharges.start(ability, Recharge { began, length });
                }
            }
            _ => {}
        }
    }
    let now = server.now(time.elapsed().as_millis());
    for mut activating in &mut activating {
        if activating.0.is_some_and(|activation| activation.left(now).is_zero()) {
            activating.0 = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use common_bevy::message::Outcome;

    fn app() -> (App, Entity) {
        let mut app = App::new();
        app.add_message::<Do>();
        app.init_resource::<Time>();
        let mut server = crate::resources::Server::default();
        server.sync(Moment::from_millis(10_000), 0);
        app.insert_resource(server);
        app.add_systems(Update, track);
        let actor = app.world_mut().spawn((Activating::default(), Recharges::default())).id();
        (app, actor)
    }

    fn begun(app: &App, ability: AbilityType, length: Duration) -> Activation {
        let now = app.world().resource::<crate::resources::Server>().now(0);
        Activation { ability, target: None, began: now, length }
    }

    fn activating(app: &App, actor: Entity) -> Option<AbilityType> {
        app.world().get::<Activating>(actor).unwrap().0.map(|activation| activation.ability)
    }

    #[test]
    fn a_skill_stands_from_its_activation_until_it_resolves() {
        let (mut app, actor) = app();
        let activation = begun(&app, AbilityType::Overpower, Duration::from_secs(2));
        app.world_mut().write_message(Do { event: GameEvent::Activate { ent: actor, activation } });
        app.update();
        assert_eq!(activating(&app, actor), Some(AbilityType::Overpower));

        // A swing of its own resolving ends nothing
        let resolved = |ability, outcome| Do { event: GameEvent::Resolved { ent: actor, ability, target: None, damage: 0.0, outcome, dot: false } };
        app.world_mut().write_message(resolved(AbilityType::AutoAttack, Outcome::Landed));
        app.update();
        assert_eq!(activating(&app, actor), Some(AbilityType::Overpower));

        app.world_mut().write_message(resolved(AbilityType::Overpower, Outcome::Interrupted));
        app.update();
        assert_eq!(activating(&app, actor), None, "however it ended");

        app.world_mut().write_message(Do { event: GameEvent::Activate { ent: actor, activation } });
        app.update();
        assert_eq!(activating(&app, actor), None, "and told again, it begins nothing");
    }

    #[test]
    fn a_skill_with_no_length_or_whose_time_has_run_stands_no_longer() {
        let (mut app, actor) = app();
        let instant = begun(&app, AbilityType::Parry, Duration::ZERO);
        app.world_mut().write_message(Do { event: GameEvent::Activate { ent: actor, activation: instant } });
        app.update();
        assert_eq!(activating(&app, actor), None, "one that takes hold at once");

        let run = Activation { began: Moment::ZERO, ..begun(&app, AbilityType::Frenzy, Duration::from_millis(500)) };
        app.world_mut().write_message(Do { event: GameEvent::Activate { ent: actor, activation: run } });
        app.update();
        assert_eq!(activating(&app, actor), None, "one whose time ran out unheard");
    }

    #[test]
    fn a_recharge_is_kept_for_its_own_skill() {
        let (mut app, actor) = app();
        let began = app.world().resource::<crate::resources::Server>().now(0);
        app.world_mut().write_message(Do { event: GameEvent::Recharge { ent: actor, ability: AbilityType::Frenzy, began, length: Duration::from_secs(3) } });
        app.update();
        let recharges = app.world().get::<Recharges>(actor).unwrap();
        assert!(!recharges.is_ready(AbilityType::Frenzy, began));
        assert!(recharges.is_ready(AbilityType::Overpower, began));
    }
}
