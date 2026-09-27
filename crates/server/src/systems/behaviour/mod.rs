pub mod chase;
pub mod hex_assignment;
pub mod kite;

use bevy::ecs::query::QueryData;
use qrz::Qrz;

use common_bevy::{
    components::{heading::Heading, position::Position, AirTime, Loc, Turn},
    plugins::nntree::NNTree,
    resources::map::Map,
    systems::physics::{self, WALK_ARC},
};

/// Airtime an NPC leaps with to climb onto a neighbouring tile.
const CLIMB_MS: i16 = 125;

/// An NPC's body as its behaviour moves it: where it stands, which way it
/// faces, the turn clock that paces its turning as a player's is paced, and
/// its airborne state.
#[derive(QueryData)]
#[query_data(mutable)]
pub struct Body {
    pub position: &'static mut Position,
    pub heading: &'static mut Heading,
    pub turn: &'static mut Turn,
    pub airtime: &'static mut AirTime,
}

impl BodyItem<'_, '_> {
    /// Carries it `dt` milliseconds toward `goal`, turning on its turn clock
    /// and walking once it faces the goal, or only turning when not `walk`
    /// (see [`physics::steer`]). The `Heading` is written only when it
    /// turns, since every change to it is sent to clients.
    pub fn steer(&mut self, goal: Heading, walk: bool, movement_speed: f32, dt: i16, map: &Map, nntree: &NNTree) {
        let (offset, airtime) = physics::steer(*self.position, &mut self.turn, goal, walk, self.airtime.state, movement_speed, dt, map, nntree);
        self.position.offset = offset;
        self.airtime.state = airtime;
        if *self.heading != self.turn.heading {
            *self.heading = self.turn.heading;
        }
    }

    /// Carries it `dt` milliseconds toward `next`, a neighbour of the floor
    /// tile `start` it stands on at `loc`, leaping when the step climbs and
    /// it faces the way to take it.
    #[allow(clippy::too_many_arguments)]
    pub fn step_toward(&mut self, loc: &Loc, start: Qrz, next: Qrz, movement_speed: f32, dt: i16, map: &Map, nntree: &NNTree) {
        let Some(goal) = Heading::between(map, start, next) else {
            return;
        };
        let facing = self.turn.heading.turn_toward(goal).1 <= WALK_ARC;
        if facing && loc.z <= next.z && self.airtime.state.is_none() {
            self.airtime.state = Some(CLIMB_MS);
        }
        self.steer(goal, true, movement_speed, dt, map, nntree);
    }

    /// Turns it `dt` milliseconds toward the tile `to`, standing where it is.
    pub fn face(&mut self, loc: &Loc, to: Qrz, dt: i16, map: &Map, nntree: &NNTree) {
        if let Some(goal) = Heading::between(map, **loc, to) {
            self.steer(goal, false, 0.0, dt, map, nntree);
        }
    }
}
