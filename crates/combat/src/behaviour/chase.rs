use std::collections::HashMap;

use bevy::prelude::*;

use common_bevy::{
    components::{
        entity_type::{actor::ActorIdentity, EntityType},
        heading::{Heading, HEADING_SLOTS},
        AttackRange, Loc, resources::Health,
        behaviour::Side, status::Status, ActorAttributes, Swing, target::Target,
        returning::Returning,
        hex_assignment::AssignedHex,
        engagement::EngagementMember,
    },
    message::{Event, Do, Component as MessageComponent},
    plugins::nntree::*,
    resources::map::Map,
    systems::{movement::speed, targeting::{across, across_share}},
};
use common_bevy::message::AbilityType;
use qrz::{Convert, Qrz};

use super::{mind::Minds, moves::{self, Candidate, Footing, Move}, perception::Sight, Bar, Body};
use crate::leap::LEAP_MS;
use common_bevy::tuning::Tuning;

/// How near its engagement's place a returning NPC counts as home, in tiles.
const HOME: i32 = 2;

/// An NPC's pursuit of what it fights: it takes a hostile within
/// `acquisition_range` as its `Target` and keeps it while that one lives,
/// and steps to the tile its movement channel chooses of its own and its
/// uncrowded neighbours ([`moves`]). Its `Target` is this system's alone to
/// set (`targeting::update_targets` leaves every `Chase` be).
///
/// It strikes from its assigned hex where it has one (`AssignedHex`), else
/// from wherever its target is within `attack_range`. A step that gives
/// ground it takes backing away, facing its target; holding, or keeping to
/// its tile, it stands and faces it.
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

/// Whether the floor tile `tile` has room to stand on
fn uncrowded(nntree: &NNTree, tile: Qrz) -> bool {
    nntree.locate_all_at_point(&Loc::new(tile + Qrz::Z)).count() < 7
}

/// The neighbour of the floor tile `from` a returning NPC steps to on its
/// way to `goal`: the nearest to it that is not crowded.
fn step(map: &Map, nntree: &NNTree, from: Qrz, goal: Qrz) -> Option<Qrz> {
    map.neighbors(from)
        .into_iter()
        .map(|(neighbor, _)| neighbor)
        .filter(|&neighbor| uncrowded(nntree, neighbor))
        .min_by_key(|neighbor| neighbor.distance(&goal))
}

/// Seconds to cover `tiles` at `per_second`, never less than nothing
fn seconds(tiles: i32, per_second: f32) -> f32 {
    tiles.max(0) as f32 / per_second.max(f32::EPSILON)
}

pub fn chase(
    tuning: Res<Tuning>,
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
        (Option<&Swing>, Option<&mut Move>, Option<&EntityType>, Option<&Sight>, Option<&Bar>),
        (Option<&common_bevy::components::recovery::GlobalRecovery>, Option<&common_bevy::components::resources::Endurance>),
    )>, Query<(Entity, &Heading)>)>,
    q_target: Query<(&Loc, &Health, &Side, Option<&ActorAttributes>, Option<&Status>)>,
    q_home: Query<&Loc, Without<Chase>>,
    nntree: Res<NNTree>,
    map: Res<Map>,
    dt: Res<Time>,
    dice: Res<crate::dice::Dice>,
    minds: Res<Minds>,
    mut rolls: Query<&mut crate::dice::Rolls>,
    mut decisions: Option<ResMut<super::Decisions>>,
) {
    // Which way each actor faces, read apart from the bodies this turns
    let headings: HashMap<Entity, Heading> = actors.p1().iter().map(|(ent, &heading)| (ent, heading)).collect();
    for (npc, &chase, loc, mut body, attrs, mut target, returning, member, assigned, own_side, status, (swing, mut under_way, kind, sight, bar), (recovery, endurance)) in actors.p0().iter_mut() {
        // Held: it neither walks nor turns
        if Status::holds(status) {
            continue;
        }
        let dt_ms = dt.delta().as_millis() as i16;
        let speed = speed(attrs.map_or(0.005, |a| a.movement_speed()), status);
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
                let foes: Vec<Entity> = super::spotted(&nntree, *loc, chase.acquisition_range)
                    .filter(|&seen| q_target.get(seen).is_ok_and(|(_, health, side, ..)| health.current() > 0.0 && side.is_hostile_to(*own_side)))
                    .collect();
                let mut rolls = rolls.get_mut(npc).ok()?;
                (!foes.is_empty()).then(|| foes[dice.draw(&mut rolls, ("foe", npc)).pick(foes.len())])
            });
        if target.entity != held {
            target.entity = held;
            target.last_target = held.or(target.last_target);
        }
        let Some((held, (target_loc, _, _, target_attrs, target_status))) = held.and_then(|held| Some((held, q_target.get(held).ok()?))) else {
            continue;
        };
        let target_heading = headings.get(&held);

        // Its movement channel chooses its step; the step is walked here
        let archetype = match kind {
            Some(EntityType::Actor(actor)) => match actor.identity {
                ActorIdentity::Npc(archetype) => Some(archetype),
                _ => None,
            },
            _ => None,
        };
        let mind = minds.mind(archetype);
        let Some(floor) = floor else { continue };
        // How fast it and its target cover ground, in tiles a second; it
        // closes by a Leap where its bar holds one
        let tile = (map.convert(Qrz { q: 1, r: 0, z: 0 }) - map.convert(Qrz::default())).xz().length().max(f32::EPSILON);
        let per_second = |speed: f32| speed * 1000.0 / tile;
        let own_pace = per_second(speed);
        let target_pace = per_second(common_bevy::systems::movement::speed(target_attrs.map_or(0.005, |a| a.movement_speed()), target_status));
        let leap = bar.filter(|bar| bar.0.contains(&AbilityType::Leap))
            .and_then(|_| attrs)
            .map(|attrs| attrs.leap_tiles(&tuning) as i32);
        // It knows how far its target strikes from by having been struck,
        // and before that takes it for a melee swing's
        let target_reach = sight.map_or(0, Sight::reach).max(AttackRange::default().0);
        let to_strike = |at: Qrz| -> f32 {
            if let Some(hex) = assigned {
                return seconds(at.flat_distance(&hex.0), own_pace);
            }
            let gap = Loc::new(at + Qrz::Z).distance(target_loc) - chase.attack_range;
            let walked = seconds(gap, own_pace);
            match leap {
                Some(tiles) if gap > 0 => walked.min(LEAP_MS as f32 / 1000.0 + seconds(gap - tiles, own_pace)),
                _ => walked,
            }
        };
        let striding = status.is_some_and(Status::is_striding);
        let held = endurance.map_or(f32::INFINITY, |endurance| endurance.state.max(f32::EPSILON));
        let force = attrs.map_or(0.0, |attrs| attrs.force(&tuning));
        let room = |at: Qrz| (chase.leash_distance - at.flat_distance(&home)).max(0) as f32 / chase.leash_distance.max(1) as f32;
        let tiles: Vec<Qrz> = std::iter::once(floor)
            .chain(map.neighbors(floor).into_iter().map(|(neighbor, _)| neighbor).filter(|&neighbor| uncrowded(&nntree, neighbor)))
            .collect();
        let mut candidates: Vec<Candidate> = tiles.iter().map(|&at| {
            let standing = Loc::new(at + Qrz::Z);
            // A strike on the step there: on its heading there, or where it
            // stands, facing its target, free
            let (strike_cost, breaks_stride) = if at == floor { (0.0, false) } else {
                let heading = Heading::from_hex(Qrz { z: 0, ..at - floor });
                let share = if striding { 0.0 } else { across_share(&tuning, &heading, loc, target_loc) };
                (tuning.off_arc_cost * share * force / held, across(Some(&heading), loc, target_loc) && !striding)
            };
            Candidate {
                tile: at,
                time_to_strike: to_strike(at),
                detour: 0.0,
                time_to_be_struck: seconds(standing.distance(target_loc) - target_reach, target_pace),
                room: room(at),
                behind: target_heading.filter(|_| at != **target_loc - Qrz::Z).map_or(0.0, |&heading| {
                    let bearing = Heading::from_hex(Qrz { z: 0, ..at - (**target_loc - Qrz::Z) });
                    heading.turn_toward(bearing).1 as f32 / (HEADING_SLOTS / 2) as f32
                }),
                strike_cost,
                breaks_stride,
            }
        }).collect();
        let soonest = candidates.iter().map(|candidate| candidate.time_to_strike).fold(f32::INFINITY, f32::min);
        for candidate in &mut candidates {
            candidate.detour = candidate.time_to_strike - soonest;
        }
        let footing = Footing {
            grace: attrs.is_some_and(|attrs| attrs.grace().index() > 0),
            recovering: recovery.filter(|recovery| recovery.is_active()).map_or(0.0, |recovery| (recovery.remaining / recovery.duration.max(f32::EPSILON)).min(1.0)),
            // Its Patience pays only while its swing clock runs, engaged
            patience: attrs.filter(|_| swing.is_some_and(|swing| swing.due.is_some()))
                .map_or(0, |attrs| attrs.patience().index() as u32),
        };
        let (chosen, step) = moves::choose(&footing, &candidates, under_way.as_deref().copied().unwrap_or_default(), &mind);
        if let Some(under_way) = under_way.as_mut().filter(|under_way| ***under_way != chosen) {
            if let Some(decisions) = decisions.as_mut() {
                decisions.0.push(format!("{npc} moves {:?} -> {chosen:?}: {footing:?} {step:?}", **under_way));
            }
            **under_way = chosen;
        }
        let Some(next) = step.map(|step| step.tile).filter(|&next| next != floor) else {
            body.face(loc, **target_loc, dt_ms, &map, &nntree);
            continue;
        };
        // A step that gives ground it takes backing away, facing its
        // target, so it never turns its back or costs it a swing
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
        app.insert_resource(crate::dice::Dice::seeded(0));
        app.insert_resource(Minds::tuned());
        app.init_resource::<Tuning>();
        app.register_required_components::<Chase, crate::dice::Rolls>();
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
