use std::collections::HashMap;

use bevy::prelude::*;
use rand::seq::IteratorRandom;

use common_bevy::{
    components::{
        entity_type::{actor::ActorIdentity, EntityType},
        heading::{Heading, HEADING_SLOTS, SLOT_DEGREES},
        AttackRange, Loc, resources::Health,
        behaviour::Side, status::Status, ActorAttributes, Swing, target::Target,
        returning::Returning,
        hex_assignment::AssignedHex,
        engagement::EngagementMember,
    },
    message::{Event, Do, Component as MessageComponent},
    plugins::nntree::*,
    resources::map::Map,
    systems::{physics::Walk, targeting::{across, across_share, arc_of, is_in_facing_cone}},
};
use qrz::Qrz;

use super::{mind::mind_of, moves::{self, Footing, Move}, Body};

/// How near its engagement's place a returning NPC counts as home, in tiles.
const HOME: i32 = 2;

/// An NPC's pursuit of what it fights: it takes a hostile within
/// `acquisition_range` as its `Target` and keeps it while that one lives,
/// and walks the move its movement channel chooses ([`moves`]). Its
/// `Target` is this system's alone to set (`targeting::update_targets`
/// leaves every `Chase` be).
///
/// It fights from its assigned hex where it has one (`AssignedHex`), else
/// from wherever its target is within `attack_range`. Closing, it walks
/// there; holding, it stands and faces its target. Kiting, it runs forward
/// on the heading most directly away that still keeps its target in the
/// arc it strikes within ([`kiting`]), so it shoots as it goes, and how
/// directly away its Grace lets it run is how well it kites. Fleeing, it
/// runs straight away. Near its leash it takes no heading that carries it
/// further from its den, so it turns along the leash and circles its den.
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

/// How near its leash, in tiles, an NPC giving ground stops running out.
const LEASH_MARGIN: i32 = 12;

/// How near its leash, in tiles, an NPC stops closing on a target further
/// out, so it waits at the edge rather than let it go.
const LEASH_EDGE: i32 = 2;

/// The arc a fleeing NPC keeps its target within: all of it, so it runs
/// straight from it.
const FLEE_ARC: f32 = 180.0;

/// The heading an actor at `loc`, facing `facing`, runs on from `target`
/// while it keeps it within the `arc` it strikes within. Of the headings
/// no further than a step short of the arc from the bearing to the target,
/// it takes the one where how directly it runs away, times what `shots`
/// gives a heading for shooting from it (1 where shooting costs nothing),
/// is greatest, and of two alike the one nearer the way it faces, so it
/// holds a course.
///
/// `outward`, given while it nears its leash, is the bearing from its den:
/// it then takes none of those headings that leads further out where one
/// leads along the leash or in, and the least outward of them where none
/// does.
fn kiting(loc: Loc, target: Loc, arc: f32, facing: Heading, outward: Option<Heading>, shots: impl Fn(Heading) -> f32) -> Heading {
    let quarter = HEADING_SLOTS / 4;
    let toward = Heading::from_hex(Qrz { z: 0, ..*target - *loc });
    let steps = ((arc / SLOT_DEGREES) as i32 - 1).max(0);
    // How directly away a heading `off` slots from the target runs: 0
    // straight at it, 1 straight from it
    let away = |off: i32| (1.0 - (off as f32 * SLOT_DEGREES).to_radians().cos()) / 2.0;
    (-steps..=steps)
        .map(|off| {
            let heading = toward.turned(off);
            let leash = outward.map(|outward| outward.turn_toward(heading).1.min(quarter));
            (heading, leash, away(off) * shots(heading), facing.turn_toward(heading).1)
        })
        .max_by(|a, b| a.1.cmp(&b.1).then(a.2.total_cmp(&b.2)).then(b.3.cmp(&a.3)))
        .map_or(toward, |(heading, ..)| heading)
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

/// The neighbour of the floor tile `from` an NPC circling `target` steps
/// to: of those as far from it as `from` is, a tile either way, and not
/// crowded, the nearest `home`.
fn round(map: &Map, nntree: &NNTree, from: Qrz, target: Qrz, home: Qrz) -> Option<Qrz> {
    let distance = from.flat_distance(&target);
    map.neighbors(from)
        .into_iter()
        .map(|(neighbor, _)| neighbor)
        .filter(|neighbor| (neighbor.flat_distance(&target) - distance).abs() <= 1 && neighbor.flat_distance(&target) >= distance.min(2))
        .filter(|neighbor| nntree.locate_all_at_point(&Loc::new(*neighbor + Qrz::Z)).count() < 7)
        .min_by_key(|neighbor| neighbor.flat_distance(&home))
}

pub fn chase(
    mut commands: Commands,
    mut writer: MessageWriter<Do>,
    mut actors: ParamSet<(Query<(
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
        Option<&common_bevy::components::resources::Stamina>,
        (Option<&Swing>, Option<&mut Move>, Option<&EntityType>),
    )>, Query<(Entity, &Heading)>)>,
    q_target: Query<(&Loc, &Health, &Side, Option<&AttackRange>)>,
    q_home: Query<&Loc, Without<Chase>>,
    nntree: Res<NNTree>,
    map: Res<Map>,
    dt: Res<Time>,
    mut decisions: Option<ResMut<super::Decisions>>,
) {
    // Which way each actor faces, read apart from the bodies this turns
    let headings: HashMap<Entity, Heading> = actors.p1().iter().map(|(ent, &heading)| (ent, heading)).collect();
    for (npc, &chase, loc, mut body, attrs, mut target, returning, member, assigned, own_side, status, stamina, (swing, mut under_way, kind)) in actors.p0().iter_mut() {
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
            .filter(|&held| q_target.get(held).is_ok_and(|(_, health, ..)| health.current() > 0.0))
            .or_else(|| {
                super::spotted(&nntree, *loc, chase.acquisition_range)
                    .filter(|&seen| q_target.get(seen).is_ok_and(|(_, health, side, ..)| health.current() > 0.0 && side.is_hostile_to(*own_side)))
                    .choose(&mut rand::rng())
            });
        if target.entity != held {
            target.entity = held;
            target.last_target = held.or(target.last_target);
        }
        let Some((held, (target_loc, _, _, target_range))) = held.and_then(|held| Some((held, q_target.get(held).ok()?))) else {
            continue;
        };
        let target_heading = headings.get(&held);

        // Its movement channel chooses the move; the move is walked here
        let tuning = common_bevy::tuning::tuning();
        let banked = attrs.zip(swing.and_then(|swing| swing.waited(dt.elapsed())))
            .map_or(0, |(attrs, waited)| attrs.banked(waited, attrs.cadence_interval()));
        let archetype = match kind {
            Some(EntityType::Actor(actor)) => match actor.identity {
                ActorIdentity::Npc(archetype) => Some(archetype),
                _ => None,
            },
            _ => None,
        };
        let mind = mind_of(archetype);
        // What a shot from a heading costs of the stamina it has, and
        // whether it breaks stride: across its line a swing spends stamina,
        // the more the further round its arc, and breaks stride but in a
        // Perfect Stride
        let striding = status.is_some_and(Status::is_striding);
        let held = stamina.map_or(f32::INFINITY, |stamina| stamina.state.max(f32::EPSILON));
        let shot = |heading: Heading| (
            tuning.off_arc_stamina * across_share(&heading, loc, target_loc) / held,
            across(Some(&heading), loc, target_loc) && !striding,
        );
        let mut footing = Footing {
            placed: match assigned {
                Some(hex) => loc.flat_distance(&Loc::new(hex.0)) == 0,
                None => loc.distance(target_loc) <= chase.attack_range,
            },
            ranged: chase.ranged(),
            strikes_running: **loc != **target_loc && stamina.map_or(true, |stamina| stamina.state >= tuning.off_arc_stamina),
            shot_cost: 0.0,
            breaks_stride: false,
            pursuit_pays: target_range.copied().unwrap_or_default().0 >= target_loc.distance(loc)
                || target_heading.is_none_or(|&heading| is_in_facing_cone(heading, *target_loc, *loc)),
            at_leash: from_home >= chase.leash_distance - LEASH_EDGE && target_loc.flat_distance(&home) > from_home,
            leash_room: (chase.leash_distance - from_home) as f32 / chase.leash_distance.max(1) as f32,
            clear_outward: target_loc.flat_distance(&home) < from_home,
            distance: loc.distance(target_loc),
            reach: chase.attack_range,
            leap: tuning.leap_distance as i32,
            banked,
            // Its Patience banks only while its swing clock runs, in a fight
            patience: attrs.filter(|_| swing.is_some_and(|swing| swing.due.is_some()))
                .map_or(0, |attrs| attrs.patience().index() as u32),
        };
        // Kiting, it runs on the heading that best weighs running away
        // against what its shots cost there, and the kite is scored on it
        let outward = (from_home >= chase.leash_distance - LEASH_MARGIN).then(|| Heading::from_hex(Qrz { z: 0, ..**loc - *home }));
        let kite = kiting(*loc, *target_loc, arc_of(attrs), body.turn.heading, outward, |heading| {
            let (cost, breaks) = shot(heading);
            moves::shot_value(&footing, cost, breaks, &mind)
        });
        (footing.shot_cost, footing.breaks_stride) = shot(kite);
        let chosen = moves::choose(&footing, under_way.as_deref().copied().unwrap_or_default(), &mind);
        if let Some(under_way) = under_way.as_mut().filter(|under_way| ***under_way != chosen) {
            if let Some(decisions) = decisions.as_mut() {
                decisions.0.push(format!("{npc} moves {:?} -> {chosen:?}: {footing:?}", **under_way));
            }
            **under_way = chosen;
        }
        match chosen {
            Move::Hold => {
                body.face(loc, **target_loc, dt_ms, &map, &nntree);
                continue;
            }
            Move::Kite | Move::Flee => {
                let goal = if chosen == Move::Kite { kite } else { kiting(*loc, *target_loc, FLEE_ARC, body.turn.heading, outward, |_| 1.0) };
                body.steer(goal, Walk::Forward, speed, dt_ms, &map, &nntree);
                continue;
            }
            Move::Circle => {
                if let Some((floor, next)) = floor.and_then(|floor| Some((floor, round(&map, &nntree, floor, **target_loc, *home)?))) {
                    body.step_toward(loc, floor, next, speed, dt_ms, &map, &nntree);
                } else {
                    body.face(loc, **target_loc, dt_ms, &map, &nntree);
                }
                continue;
            }
            Move::Close => {}
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
    fn a_kiter_runs_the_way_most_directly_from_its_target_that_still_strikes_it() {
        use common_bevy::systems::targeting::within_arc;
        let here = Loc::new(Qrz { q: 0, r: 0, z: 1 });
        let target = Loc::new(Qrz { q: 5, r: 0, z: 1 });
        let toward = Heading::from_hex(Qrz { q: 1, r: 0, z: 0 });
        for arc in [60.0, 90.0, 120.0, 150.0] {
            let goal = kiting(here, target, arc, toward, None, |_| 1.0);
            assert!(within_arc(goal, arc, here, target), "at {arc} it still strikes its target");
            assert!(!within_arc(goal, arc - 2.0 * SLOT_DEGREES, here, target), "from the edge of the arc");
        }
        let away = |arc| toward.reversed().turn_toward(kiting(here, target, arc, toward, None, |_| 1.0)).1;
        assert!(away(150.0) < away(90.0), "more Grace runs more directly away");

        let (left, right) = (toward.turned(-2), toward.turned(2));
        let run = |facing| kiting(here, target, 150.0, facing, None, |_| 1.0);
        assert_ne!(run(left), run(right), "it runs to the side it already leans");
        assert_eq!(run(run(left)), run(left), "and holds that course");
    }

    #[test]
    fn shots_dear_far_round_its_arc_turn_its_run_in_toward_its_target() {
        use common_bevy::systems::targeting::within_arc;
        let here = Loc::new(Qrz { q: 0, r: 0, z: 1 });
        let target = Loc::new(Qrz { q: 5, r: 0, z: 1 });
        let toward = Heading::from_hex(Qrz { q: 1, r: 0, z: 0 });
        let free = kiting(here, target, 150.0, toward, None, |_| 1.0);
        let dear = kiting(here, target, 150.0, toward, None, |heading| if within_arc(heading, 120.0, here, target) { 1.0 } else { 0.2 });
        let away = |heading: Heading| toward.reversed().turn_toward(heading).1;
        assert!(away(dear) > away(free), "it runs less directly away");
        assert!(within_arc(dear, 120.0, here, target), "on a heading its shots come cheaper from");
    }

    #[test]
    fn near_its_leash_a_kiter_turns_along_it_and_never_further_out() {
        use common_bevy::systems::targeting::within_arc;
        let here = Loc::new(Qrz { q: 0, r: 0, z: 1 });
        let east = Heading::from_hex(Qrz { q: 1, r: 0, z: 0 });
        let outward = east;
        let quarter = HEADING_SLOTS / 4;

        // Its target comes from its den's side: straight away from it is straight out
        let chaser = Loc::new(Qrz { q: -5, r: 0, z: 1 });
        assert!(outward.turn_toward(kiting(here, chaser, 150.0, east, None, |_| 1.0)).1 < quarter, "clear of its leash it runs out");
        let along = kiting(here, chaser, 150.0, east, Some(outward), |_| 1.0);
        assert_eq!(outward.turn_toward(along).1, quarter, "near it, along the leash: as far from its target as that allows");
        assert!(within_arc(along, 150.0, here, chaser), "still striking it");
        assert_eq!(kiting(here, chaser, 150.0, along, Some(outward), |_| 1.0), along, "and it keeps circling the way it goes");

        // Its target stands further out than it: away from it is already inward
        let beyond = Loc::new(Qrz { q: 5, r: 0, z: 1 });
        assert_eq!(kiting(here, beyond, 150.0, east, Some(outward), |_| 1.0), kiting(here, beyond, 150.0, east, None, |_| 1.0), "a heading that leads in is taken as it is");

        // With no Grace no heading that strikes a target further out leads in: the least outward
        let narrow = kiting(here, beyond, 60.0, east, Some(outward), |_| 1.0);
        assert!(within_arc(narrow, 60.0, here, beyond));
        assert_eq!(outward.turn_toward(narrow).1, 3, "the edge of its arc, as far from out as it reaches");
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
