use bevy::prelude::*;
use rand::seq::IteratorRandom;

use common_bevy::{
    components::{
        heading::{Heading, SLOT_DEGREES},
        AttackRange, Loc, resources::Health,
        behaviour::Side, status::Status, ActorAttributes, target::Target,
        returning::Returning,
        hex_assignment::AssignedHex,
        engagement::EngagementMember,
    },
    message::{Event, Do, Component as MessageComponent},
    plugins::nntree::*,
    resources::map::Map,
    systems::{physics::Walk, targeting::arc_of},
};
use qrz::Qrz;

use super::Body;

/// How near its engagement's place a returning NPC counts as home, in tiles.
const HOME: i32 = 2;

/// An NPC's pursuit of what it fights, the whole of its behaviour: it takes
/// a hostile within `acquisition_range` as its `Target` and keeps it while
/// that one lives, walks to where it fights from, and faces its target
/// there. Its `Target` is this system's alone to set
/// (`targeting::update_targets` leaves every `Chase` be).
///
/// It fights from its assigned hex where it has one (`AssignedHex`), else
/// from wherever its target is within `attack_range`. One that reaches no
/// further than a melee swing stands there and faces its target. One that
/// fights from range ([`Chase::ranged`]) closes only until its target is
/// in reach, and inside it gives ground: it runs forward with its target
/// at the edge of the arc it strikes within ([`kiting`]), so it shoots as
/// it goes, and how directly away its Grace lets it run is how well it
/// kites.
///
/// Further than `leash_distance` from its engagement's place it lets its
/// target go and walks home (`Returning`), taking no target until it is
/// back.
#[derive(Clone, Component, Copy, Debug)]
pub struct Chase {
    pub acquisition_range: u32,
    pub leash_distance: i32,
    pub attack_range: i32,
}

impl Chase {
    /// Whether it fights from range: its reach is past a melee swing's
    pub fn ranged(&self) -> bool {
        self.attack_range > AttackRange::default().0
    }
}

/// The heading an actor at `loc`, facing `facing`, runs on to keep `target`
/// at the edge of the `arc` it strikes within: the bearing to the target
/// turned one step short of the arc, to the side it already leans, so it
/// holds one course.
fn kiting(loc: Loc, target: Loc, arc: f32, facing: Heading) -> Heading {
    let toward = Heading::from_hex(Qrz { z: 0, ..*target - *loc });
    let steps = ((arc / SLOT_DEGREES) as i32 - 1).max(0);
    let (side, _) = toward.turn_toward(facing);
    toward.turned(if side < 0 { -steps } else { steps })
}

/// The neighbour of the floor tile `from` an NPC steps to on its way to
/// `goal`: the nearest to it that is not crowded.
fn step(map: &Map, nntree: &NNTree, from: Qrz, goal: Qrz) -> Option<Qrz> {
    map.neighbors(from)
        .into_iter()
        .map(|(neighbor, _)| neighbor)
        .filter(|neighbor| nntree.locate_all_at_point(&Loc::new(*neighbor + Qrz::Z)).count() < 7)
        .min_by_key(|neighbor| neighbor.distance(&goal))
}

pub fn chase(
    mut commands: Commands,
    mut writer: MessageWriter<Do>,
    mut query: Query<(
        Entity,
        &Chase,
        &Loc,
        Body,
        Option<&ActorAttributes>,
        &mut Target,
        Has<Returning>,
        &EngagementMember,
        Option<&AssignedHex>,
        &Side,
        Option<&Status>,
    )>,
    q_target: Query<(&Loc, &Health, &Side)>,
    q_home: Query<&Loc, Without<Chase>>,
    nntree: Res<NNTree>,
    map: Res<Map>,
    dt: Res<Time>,
) {
    for (npc, &chase, loc, mut body, attrs, mut target, returning, member, assigned, own_side, status) in &mut query {
        // Held: it neither walks nor turns
        if Status::holds(status) {
            continue;
        }
        let dt_ms = dt.delta().as_millis() as i16;
        let speed = common_bevy::systems::movement::speed(attrs.map_or(0.005, |a| a.movement_speed()), status);
        let Ok(&home) = q_home.get(member.0) else {
            continue;
        };
        let from_home = loc.flat_distance(&home);
        let floor = map.get_by_qr(loc.q, loc.r).map(|(floor, _)| floor);

        if returning {
            if from_home <= HOME {
                commands.entity(npc).remove::<Returning>();
            } else if let Some((floor, next)) = floor.and_then(|floor| Some((floor, step(&map, &nntree, floor, *home)?))) {
                body.step_toward(loc, floor, next, speed, dt_ms, &map, &nntree);
            }
            continue;
        }

        // Past its leash it lets go and goes home. Clients are told: they
        // regenerate a returning NPC's health as the server does.
        if from_home > chase.leash_distance {
            *target = Target::default();
            commands.entity(npc).insert(Returning);
            writer.write(Do { event: Event::Incremental { ent: npc, component: MessageComponent::Returning(Returning) } });
            continue;
        }

        // The target it has, while that one lives, else any hostile in sight
        let held = target.entity
            .filter(|&held| q_target.get(held).is_ok_and(|(_, health, _)| health.current() > 0.0))
            .or_else(|| {
                super::spotted(&nntree, *loc, chase.acquisition_range)
                    .filter(|&seen| q_target.get(seen).is_ok_and(|(_, health, side)| health.current() > 0.0 && side.is_hostile_to(*own_side)))
                    .choose(&mut rand::rng())
            });
        if target.entity != held {
            target.entity = held;
            target.last_target = held.or(target.last_target);
        }
        let Some(target_loc) = held.and_then(|held| q_target.get(held).ok()).map(|(target_loc, ..)| target_loc) else {
            continue;
        };

        // Where it fights from, it stands and faces its target, or, fighting
        // from range, runs with it at the edge of its arc
        let placed = match assigned {
            Some(hex) => loc.flat_distance(&Loc::new(hex.0)) == 0,
            None => loc.distance(target_loc) <= chase.attack_range,
        };
        if placed {
            if chase.ranged() && **loc != **target_loc {
                let goal = kiting(*loc, *target_loc, arc_of(attrs), body.turn.heading);
                body.steer(goal, Walk::Forward, speed, dt_ms, &map, &nntree);
            } else {
                body.face(loc, **target_loc, dt_ms, &map, &nntree);
            }
            continue;
        }

        let goal = assigned.map_or(**target_loc, |hex| hex.0);
        let Some((floor, next)) = floor.and_then(|floor| Some((floor, step(&map, &nntree, floor, goal)?))) else {
            continue;
        };
        // A step that gives ground it takes backing away, facing its
        // target, so stepping out to its place never turns its back or
        // costs it a swing
        if next.flat_distance(target_loc) > floor.flat_distance(target_loc) {
            body.back_toward(loc, floor, next, **target_loc, speed, dt_ms, &map, &nntree);
        } else {
            body.step_toward(loc, floor, next, speed, dt_ms, &map, &nntree);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use common_bevy::components::{entity_type::EntityType, heading::Heading, position::Position, AirTime, Turn};

    const REACH: i32 = 2;
    const LEASH: i32 = 20;

    /// A world of flat ground with an engagement's place at the origin.
    fn ground() -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins(NNTreePlugin);
        app.add_message::<Do>();
        app.init_resource::<Time>();
        let mut tiles = qrz::Map::<EntityType>::new(1.0, 0.8, qrz::HexOrientation::FlatTop);
        for q in -4..=LEASH + 8 {
            for r in -4..=4 {
                tiles.insert(Qrz { q, r, z: 0 }, EntityType::Decorator(default()));
            }
        }
        app.insert_resource(Map::new(tiles));
        let home = app.world_mut().spawn(Loc::new(Qrz { q: 0, r: 0, z: 1 })).id();
        (app, home)
    }

    fn actor(app: &mut App, side: Side, q: i32) -> Entity {
        let loc = Loc::new(Qrz { q, r: 0, z: 1 });
        let ent = app.world_mut().spawn((loc, side, Health::full(100.0))).id();
        app.world_mut().entity_mut(ent).insert(NearestNeighbor::new(ent, loc));
        ent
    }

    fn npc(app: &mut App, home: Entity, q: i32) -> Entity {
        let ent = actor(app, Side::WILD, q);
        app.world_mut().entity_mut(ent).insert((
            Chase { acquisition_range: 10, leash_distance: LEASH, attack_range: REACH },
            Target::default(),
            EngagementMember(home),
            Position::at_tile(Qrz { q, r: 0, z: 1 }),
            Heading::default(),
            Turn::default(),
            AirTime::default(),
        ));
        ent
    }

    fn run(app: &mut App) {
        app.world_mut().run_system_once(chase).unwrap();
    }

    fn target_of(app: &App, npc: Entity) -> Option<Entity> {
        app.world().get::<Target>(npc).unwrap().entity
    }

    #[test]
    fn a_kiter_runs_with_its_target_at_the_edge_of_its_arc() {
        use common_bevy::systems::targeting::within_arc;
        let here = Loc::new(Qrz { q: 0, r: 0, z: 1 });
        let target = Loc::new(Qrz { q: 5, r: 0, z: 1 });
        let toward = Heading::from_hex(Qrz { q: 1, r: 0, z: 0 });
        for arc in [60.0, 90.0, 120.0, 150.0] {
            let goal = kiting(here, target, arc, toward);
            assert!(within_arc(goal, arc, here, target), "at {arc} it still strikes its target");
            assert!(!within_arc(goal, arc - 2.0 * SLOT_DEGREES, here, target), "from the edge of the arc");
        }
        let away = |arc| toward.reversed().turn_toward(kiting(here, target, arc, toward)).1;
        assert!(away(150.0) < away(90.0), "more Grace runs more directly away");

        let (left, right) = (toward.turned(-2), toward.turned(2));
        assert_ne!(kiting(here, target, 150.0, left), kiting(here, target, 150.0, right), "it runs to the side it already leans");
        assert_eq!(kiting(here, target, 150.0, kiting(here, target, 150.0, left)), kiting(here, target, 150.0, left), "and holds that course");
    }

    #[test]
    fn it_keeps_its_target_while_that_one_lives_and_takes_another_when_it_falls() {
        let (mut app, home) = ground();
        let hunter = npc(&mut app, home, 1);
        assert_eq!({ run(&mut app); target_of(&app, hunter) }, None, "nothing in sight, nothing targeted");

        let first = actor(&mut app, Side::PLAYERS, 6);
        let friend = actor(&mut app, Side::WILD, 2);
        run(&mut app);
        assert_eq!(target_of(&app, hunter), Some(first), "the one hostile in sight, never its own side");

        let nearer = actor(&mut app, Side::PLAYERS, 3);
        run(&mut app);
        assert_eq!(target_of(&app, hunter), Some(first), "a nearer hostile does not draw it off");

        app.world_mut().get_mut::<Health>(first).unwrap().state = 0.0;
        run(&mut app);
        assert_eq!(target_of(&app, hunter), Some(nearer), "its target fell: the next in sight");
        let _ = friend;
    }

    #[test]
    fn past_its_leash_it_lets_go_and_takes_no_target_until_it_is_home() {
        let (mut app, home) = ground();
        let hunter = npc(&mut app, home, 1);
        let prey = actor(&mut app, Side::PLAYERS, 4);
        run(&mut app);
        assert_eq!(target_of(&app, hunter), Some(prey));

        let far = Loc::new(Qrz { q: LEASH + 1, r: 0, z: 1 });
        app.world_mut().entity_mut(hunter).insert(far);
        app.world_mut().entity_mut(prey).insert(Loc::new(Qrz { q: LEASH + 3, r: 0, z: 1 }));
        // The tree learns where they stand now
        app.update();
        run(&mut app);
        assert!(app.world().get::<Returning>(hunter).is_some(), "past the leash it goes home");
        assert_eq!(target_of(&app, hunter), None);
        run(&mut app);
        assert_eq!(target_of(&app, hunter), None, "on its way home it takes no target, though one stands beside it");

        app.world_mut().entity_mut(hunter).insert(Loc::new(Qrz { q: 1, r: 0, z: 1 }));
        run(&mut app);
        assert!(app.world().get::<Returning>(hunter).is_none(), "home, it is returning no more");
    }

    #[test]
    fn a_step_goes_to_the_neighbour_nearest_the_goal() {
        let (app, _) = ground();
        let (map, tree) = (app.world().resource::<Map>(), app.world().resource::<NNTree>());
        let from = Qrz { q: 0, r: 0, z: 0 };
        let next = step(map, tree, from, Qrz { q: 6, r: 0, z: 0 }).unwrap();
        assert_eq!(next.flat_distance(&Qrz { q: 6, r: 0, z: 0 }), 5, "one tile closer");
        assert_eq!(next.flat_distance(&from), 1);
    }
}
