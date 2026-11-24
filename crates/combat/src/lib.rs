//! # Combat
//!
//! The fight as the live server runs it, with no networking: every
//! ability through one gate, activations and recharges, blocks and
//! interrupts, damage, targeting, how NPCs behave and choose, and the
//! engagements they fight in. The server installs it with its network
//! round it; the balance arena (`crates/arena`) installs it bare, so the
//! arena fights by the rules players meet.

pub mod abilities;
pub mod actor;
pub mod behaviour;
pub mod dice;
pub mod engagement;
pub mod landing;
pub mod leap;
mod plugin;
pub mod runes;
pub mod targeting;

pub use plugin::{BehaviourPlugin, CombatPlugin};

use bevy::prelude::*;
use common_bevy::{
    components::{resources::*, *},
    moment::Moment,
};

/// The game's clock: what the server's `Time` has run, in milliseconds,
/// from `elapsed_offset` (0 live; a test starts it where it likes). An
/// activation's and a recharge's moments and a client's `Init` and `Pong`
/// are all stamped on it, each a [`Moment`]. The world's calendar is not:
/// it is read off this clock through `calendar`, which an admin may shift
/// or hold, sent to every client and read by the date, the sky and the
/// weather alone. A moment of the calendar is billions of milliseconds; as
/// a float it keeps minutes, so it never reaches timing.
#[derive(Default, Resource)]
pub struct RunTime {
    pub elapsed_offset: u128,
    pub calendar: common_bevy::systems::Calendar,
}

impl RunTime {
    /// The game's clock at `time`
    pub fn now(&self, time: &Time) -> Moment {
        Moment::from_millis(time.elapsed().as_millis() as u64 + self.elapsed_offset as u64)
    }

    /// The calendar's moment at `time`: what a time kept past the server
    /// is measured on.
    pub fn when(&self, time: &Time) -> common_bevy::systems::When {
        self.calendar.when(self.now(time))
    }
}

/// `ent` stepped of its own: the skill it was activating, if any, is
/// cancelled (`abilities::cancel`). Whatever walks a settled player by its
/// input triggers it, and a turn never does; an NPC mid-activation stands
/// still instead (`behaviour::chase`). The server's own; it never crosses
/// the wire.
#[derive(Event, Debug, Clone, Copy)]
pub struct Cancel {
    pub ent: Entity,
}

/// Keeps an actor's swing clock to its engagement. The clock runs while it
/// is engaged: in combat, or with a living hostile within the range a fight
/// is taken up at (`behaviour::ACQUISITION_RANGE`), first contact or not.
/// As it is engaged its clock starts, a swing due at once; disengaged, no
/// swing waits.
pub fn track_engagement(
    mut query: Query<(Entity, &CombatState, &Loc, Option<&common_bevy::components::behaviour::Side>, &mut common_bevy::components::Swing)>,
    others: Query<(&common_bevy::components::behaviour::Side, &Health)>,
    nntree: Res<common_bevy::plugins::nntree::NNTree>,
    time: Res<Time>,
) {
    for (ent, state, &loc, side, mut swing) in &mut query {
        let hostile_near = side.is_some_and(|&side| {
            crate::behaviour::hostiles_near(&nntree, ent, loc, crate::behaviour::ACQUISITION_RANGE, side, |other| {
                others.get(other).ok().map(|(other_side, health)| (*other_side, health.state))
            }).next().is_some()
        });
        let engaged = state.in_combat || hostile_near;
        match (engaged, swing.due) {
            (false, Some(_)) => swing.due = None,
            (true, None) => swing.due = Some(Moment::ZERO + time.elapsed()),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use common_bevy::{components::Swing, message::Do};
    use std::time::Duration;

    fn engaging(world: &mut World) {
        world.run_system_once(track_engagement).unwrap();
    }

    #[test]
    fn a_hostile_in_range_starts_the_swing_clock_before_first_contact() {
        use common_bevy::{components::{behaviour::Side, Loc}, plugins::nntree::{NNTreePlugin, NearestNeighbor}};
        let mut app = App::new();
        app.add_plugins(NNTreePlugin);
        app.add_message::<Do>();
        app.init_resource::<Time>();
        let at = |q: i32| Loc::new(qrz::Qrz { q, r: 0, z: 0 });
        let calm = CombatState { in_combat: false, last_action: Moment::ZERO };
        let waiting = app.world_mut().spawn((calm, at(0), Side::WILD, Health::full(100.0), Swing::default())).id();
        app.world_mut().entity_mut(waiting).insert(NearestNeighbor::new(waiting, at(0)));
        app.update();
        engaging(app.world_mut());
        assert_eq!(app.world().get::<Swing>(waiting).unwrap().due, None, "with no hostile near it banks nothing");

        let far = crate::behaviour::ACQUISITION_RANGE as i32 - 1;
        let hostile = app.world_mut().spawn((at(far), Side::PLAYERS, Health::full(100.0))).id();
        app.world_mut().entity_mut(hostile).insert(NearestNeighbor::new(hostile, at(far)));
        app.update();
        engaging(app.world_mut());
        assert!(app.world().get::<Swing>(waiting).unwrap().due.is_some(), "a hostile within range starts its clock, out of combat");
    }

    #[test]
    fn a_fight_starts_the_swing_clock_and_its_end_clears_it() {
        let secs = Duration::from_secs;
        let at = |secs: u64| Moment::from_millis(secs * 1_000);
        let mut app = App::new();
        app.add_plugins(common_bevy::plugins::nntree::NNTreePlugin);
        app.add_message::<Do>();
        let world = app.world_mut();
        let mut time = Time::<()>::default();
        time.advance_by(secs(10));
        world.insert_resource(time);
        let here = Loc::new(qrz::Qrz { q: 0, r: 0, z: 0 });
        let fighter = world.spawn((CombatState { in_combat: true, last_action: Moment::ZERO }, here, Swing::default())).id();

        engaging(world);
        let swing = *world.get::<Swing>(fighter).unwrap();
        assert_eq!(swing.waited(at(10)), Some(Duration::ZERO), "due as the fight finds it");
        assert_eq!(swing.waited(at(15)), Some(secs(5)), "and waiting from then");
        assert_eq!(Swing { due: Some(at(20)) }.waited(at(15)), None, "one still to come due waits for nothing");

        world.get_mut::<CombatState>(fighter).unwrap().in_combat = false;
        engaging(world);
        assert_eq!(world.get::<Swing>(fighter).unwrap().waited(at(60)), Some(Duration::ZERO), "disengaged, it is due and has waited no time");
    }
}
