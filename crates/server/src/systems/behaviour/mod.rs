pub mod chase;
pub mod hex_assignment;
pub mod mind;
pub mod moves;
pub mod perception;
pub mod skills;
pub mod utility;

use bevy::ecs::query::QueryData;
use qrz::Qrz;

use common_bevy::{
    components::{heading::Heading, position::Position, AirTime, Loc, Turn},
    plugins::nntree::NNTree,
    resources::map::Map,
    systems::{physics::{self, Walk, WALK_ARC}, targeting::is_in_facing_cone},
};

/// What NPCs decided, each a line, kept only where the resource exists:
/// the balance arena's trace reads it.
#[derive(bevy::prelude::Resource, Default)]
pub struct Decisions(pub Vec<String>);

/// How far an NPC looks for a target, in tiles, whatever it chases with.
pub const ACQUISITION_RANGE: u32 = 25;

/// What an NPC at `loc` spots within `range`, measured as its swing
/// measures reach (`Loc::distance`, the first level of height between
/// free). The tree counts every level, so never more than one past that:
/// it is searched a tile wider and the reach measure decides.
pub fn spotted(nntree: &NNTree, loc: Loc, range: u32) -> impl Iterator<Item = bevy::prelude::Entity> + '_ {
    let wider = range as i64 + 1;
    nntree.locate_within_distance(loc, wider * wider)
        .filter(move |nn| nn.loc.distance(&loc) <= range as i32)
        .map(|nn| nn.ent)
}

/// How far an NPC follows a target from its den, in tiles, before it gives
/// up and goes home.
pub const LEASH_DISTANCE: i32 = 60;

/// How far a Kiter reaches, in tiles: its auto-attack, its Volley, and
/// where it stops closing on its target.
pub const KITER_REACH: i32 = 20;

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
    /// and taking its `walk` once it faces the goal (see [`physics::steer`]).
    /// The `Heading` is written only when it turns, since every change to it
    /// is sent to clients.
    pub fn steer(&mut self, goal: Heading, walk: Walk, movement_speed: f32, dt: i16, map: &Map, nntree: &NNTree) {
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
        self.steer(goal, Walk::Forward, movement_speed, dt, map, nntree);
    }

    /// Carries it `dt` milliseconds backward toward `next`, a neighbour of
    /// the floor tile `start` it stands on at `loc`: it faces the way it
    /// came while that keeps the tile `watched` in its facing cone, and
    /// faces `watched` itself otherwise, so it gives ground without turning
    /// its back.
    #[allow(clippy::too_many_arguments)]
    pub fn back_toward(&mut self, loc: &Loc, start: Qrz, next: Qrz, watched: Qrz, movement_speed: f32, dt: i16, map: &Map, nntree: &NNTree) {
        let Some(away) = Heading::between(map, start, next) else {
            return;
        };
        let goal = match away.reversed() {
            facing if is_in_facing_cone(facing, *loc, Loc::new(watched)) => facing,
            facing => Heading::between(map, **loc, watched).unwrap_or(facing),
        };
        self.steer(goal, Walk::Backward, movement_speed, dt, map, nntree);
    }

    /// Turns it `dt` milliseconds toward the tile `to`, standing where it is.
    pub fn face(&mut self, loc: &Loc, to: Qrz, dt: i16, map: &Map, nntree: &NNTree) {
        if let Some(goal) = Heading::between(map, **loc, to) {
            self.steer(goal, Walk::Still, 0.0, dt, map, nntree);
        }
    }
}

#[cfg(test)]
mod spotting_tests {
    use super::*;
    use bevy::prelude::*;
    use common_bevy::plugins::nntree::{NNTreePlugin, NearestNeighbor};

    #[test]
    fn up_a_slope_a_hostile_is_spotted_as_reach_would_reach_it() {
        let mut app = App::new();
        app.add_plugins(NNTreePlugin);
        let here = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let (near, far) = (Entity::from_raw_u32(1).unwrap(), Entity::from_raw_u32(2).unwrap());
        let mut tree = app.world_mut().resource_mut::<NNTree>();
        // 24 out and two levels up: 25 as reach measures, 26 as the tree does
        tree.insert(NearestNeighbor::new(near, Loc::new(Qrz { q: 24, r: 0, z: 2 })));
        tree.insert(NearestNeighbor::new(far, Loc::new(Qrz { q: 25, r: 0, z: 2 })));
        let seen: Vec<Entity> = spotted(&tree, here, ACQUISITION_RANGE).collect();
        assert_eq!(seen, vec![near]);
    }
}
