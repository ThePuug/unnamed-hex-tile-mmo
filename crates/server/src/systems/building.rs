//! Building: a player spends its bag's wood or stone on the tile it
//! faces, works, and a construct stands there. Taking its own construct
//! down is the same work, and lays what it was built of as a pile.
//! A hearth founds a hub ([`Hubs`]): it is laid on the tile faced, on no
//! hub's ground, and no settler comes to it until a player raises its
//! hub a hitching post ([`Camps`]). A player raises a hub's post and its
//! first tents by hand ([`Job::Raise`]), on a tile the hub's rules take;
//! once the hub has a residential planner, tents are the planner's. What
//! is raised for a hub is the hub's, and no player takes it down. Where a
//! player returns and where it lives are two things: G on a hub's hearth
//! makes it where the player returns, whoever it is, as laying one does
//! for its builder; G on a tent with room makes it the player's home,
//! and the player lives in the hub from then, the hearth its builder
//! laid the hub's.

use std::collections::HashMap;

use bevy::prelude::*;
use common::{
    building::{self, Construct, Fixture, Kind},
    network::Identity,
};
use common_bevy::{
    components::{equipment::Inventory, heading::Heading, position::Position, Home, Lives, Loc},
    hubs::Hubs,
    plan::{self, What, Zone},
    message::{Do, Event, Outcome, Try},
    moment::Moment,
    plugins::nntree::NNTree,
};
use qrz::Qrz;

use crate::systems::{
    characters::Character,
    dens::Dens,
    economy::{Cause, Holder, Moved},
    gathering::{Ground, Piles, Stance, Working},
    settlers::{what_of, Camps, Settling},
};

/// Whose a construct is: the player's who built it, or its hub's,
/// through the camp round the hearth on that tile.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Owner {
    Player(Identity),
    Camp(i32, i32),
}

/// A construct's record: whose it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Built {
    pub by: Owner,
}

/// Every construct standing, by its tile. The tile's cover says what it
/// is; this says whose.
#[derive(Resource, Default)]
pub struct Constructs(HashMap<(i32, i32), Built>);

impl Constructs {
    pub fn on(&self, tile: (i32, i32)) -> Option<Built> {
        self.0.get(&tile).copied()
    }

    /// Lays the record of a construct kept from an earlier run, where the
    /// tile has none.
    pub fn recall(&mut self, tile: (i32, i32), built: Built) {
        self.0.entry(tile).or_insert(built);
    }

    /// Records the construct that came to stand on `tile`, or whose the
    /// one standing there now is.
    pub fn raise(&mut self, tile: (i32, i32), built: Built) {
        self.0.insert(tile, built);
    }

    /// The tile of every construct that is a camp's, with the hearth
    /// that camp is known by.
    pub fn of_camps(&self) -> impl Iterator<Item = ((i32, i32), (i32, i32))> + '_ {
        self.0.iter().filter_map(|(&tile, built)| match built.by {
            Owner::Camp(q, r) => Some((tile, (q, r))),
            Owner::Player(_) => None,
        })
    }

    /// What stood on `tile` came down.
    pub fn fell(&mut self, tile: (i32, i32)) {
        self.0.remove(&tile);
    }
}

/// What a player is at work on, on the tile it faces.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Job {
    Build(Construct),
    TakeDown,
    /// A fixture raised for the hub that holds the tile.
    Raise(Fixture),
}

/// The work a player is at: `job` on tile `(q, r)`, done at `until`.
/// Leaving its [`Stance`] breaks it off with nothing spent.
#[derive(Component, Clone, Copy, Debug)]
pub struct Building {
    pub q: i32,
    pub r: i32,
    pub job: Job,
    pub until: Moment,
}

/// Why a job cannot be done.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Refusal {
    /// The map holds no tile the player faces within a step of its own.
    NoTile,
    /// The tile cannot take the construct.
    NoRoom,
    /// The bag does not hold what the construct takes.
    NoMaterials,
    /// An actor stands where the construct would.
    Occupied,
    /// A den stands on the ground a founding hearth would hold.
    Den,
    /// A hub holds the ground a hearth's camp would, or would touch it.
    Held,
    /// No construct stands there to take down.
    Nothing,
    /// The construct is another player's.
    NotYours,
    /// The hub's rules do not take the fixture there, or it is not a
    /// player's to raise.
    NoPlace,
}

/// The tile a player on `loc` facing `heading` builds on or takes down
/// from: the one its heading is nearest.
fn faced(loc: Qrz, heading: Heading) -> (i32, i32) {
    let dir = heading.hex_dir();
    (loc.q + dir.q, loc.r + dir.r)
}

/// How far round a hearth its camp holds the ground, in tiles.
const CAMP_REACH: i32 = (plan::SPACE + plan::PAST) as i32;

/// How far from where it stands a player raises a fixture, in tiles: as
/// far as it is shown where one would stand.
pub const RAISE_REACH: i32 = 6;

/// Whether an actor stands on tile `(q, r)`, whose ground is at `z`.
pub(crate) fn occupied(nntree: &NNTree, (q, r): (i32, i32), z: i32) -> bool {
    nntree.within_tiles(Loc::new(Qrz { q, r, z: z + 1 }), 3).any(|near| (near.loc.q, near.loc.r) == (q, r))
}

/// What building and taking down read and change.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Site<'w, 's> {
    pub(crate) ground: Ground<'w, 's>,
    pub(crate) writer: MessageWriter<'w, Do>,
    pub(crate) constructs: ResMut<'w, Constructs>,
    pub(crate) hubs: ResMut<'w, Hubs>,
    piles: ResMut<'w, Piles>,
    dens: Res<'w, Dens>,
    pub(crate) nntree: Res<'w, NNTree>,
    pub(crate) moved: MessageWriter<'w, Moved>,
}

impl Site<'_, '_> {
    /// Where a player standing at `loc` raises `fixture` when it names
    /// `tile`: the tile nearest that one, of those the hub holding either
    /// takes the fixture on, the ground takes it on, and the player
    /// reaches. None where there is none.
    fn snapped(&self, loc: Qrz, tile: (i32, i32), fixture: Fixture) -> Option<(i32, i32)> {
        let hub = self.hubs.holding(tile).or_else(|| self.hubs.holding((loc.q, loc.r)))?;
        let takes = |at: &(i32, i32)| qrz::hex_distance((loc.q, loc.r), *at) <= RAISE_REACH && self.ground.cover(at.0, at.1).is_some_and(|cover| fixture.begun(cover).is_some());
        self.hubs.sites(hub, what_of(fixture)?).into_iter().filter(takes).min_by_key(|&at| (qrz::hex_distance(at, tile), at))
    }

    /// Tile `tile`'s cover as `who`, standing at `loc`, would leave it by
    /// `job`, or why it cannot.
    fn planned(&self, who: Identity, loc: Qrz, tile: (i32, i32), job: Job, bag: &Inventory) -> Result<common::Cover, Refusal> {
        let near = if matches!(job, Job::Raise(_)) { RAISE_REACH } else { 1 };
        let (z, cover) = self.ground.lies(tile.0, tile.1).filter(|&(z, _)| (z + 1 - loc.z).abs() <= near).ok_or(Refusal::NoTile)?;
        match job {
            Job::Raise(fixture) => {
                if qrz::hex_distance((loc.q, loc.r), tile) > RAISE_REACH {
                    return Err(Refusal::NoTile);
                }
                let (hub, what) = self.hubs.holding(tile).zip(what_of(fixture)).ok_or(Refusal::NoPlace)?;
                // A hub's one post is a player's to raise, and its tents until a planner has them
                let mine = match what {
                    What::Post => self.hubs.standing(hub, What::Post).next().is_none(),
                    What::Home => !self.hubs.hired(hub).any(|zone| zone == Zone::Residential),
                    _ => false,
                };
                if !mine || self.hubs.raised_at(hub, what, tile).is_none() {
                    return Err(Refusal::NoPlace);
                }
                let mut raised = fixture.begun(cover).ok_or(Refusal::NoRoom)?;
                while let Some(next) = building::advanced(raised) {
                    raised = next;
                }
                if fixture.needs().taken(&bag.stock).is_none() {
                    return Err(Refusal::NoMaterials);
                }
                if occupied(&self.nntree, tile, z) {
                    return Err(Refusal::Occupied);
                }
                Ok(raised)
            }
            Job::Build(construct) => {
                let raised = construct.raised(cover).ok_or(Refusal::NoRoom)?;
                let cost = construct.cost();
                if bag.count(cost.kind) < cost.count {
                    return Err(Refusal::NoMaterials);
                }
                if occupied(&self.nntree, tile, z) {
                    return Err(Refusal::Occupied);
                }
                if construct.kind == Kind::Hearth {
                    if !self.hubs.founds(tile) {
                        return Err(Refusal::Held);
                    }
                    if self.dens.stands_within(Qrz { q: tile.0, r: tile.1, z }, CAMP_REACH) {
                        return Err(Refusal::Den);
                    }
                }
                Ok(raised)
            }
            Job::TakeDown => {
                let (razed, _) = building::razed(cover).ok_or(Refusal::Nothing)?;
                match self.constructs.on(tile) {
                    Some(built) if built.by != Owner::Player(who) => Err(Refusal::NotYours),
                    _ => Ok(razed),
                }
            }
        }
    }
}

/// Answers each build a player asks for: a construct on the tile it
/// faces, what stands there taken down, or a fixture raised for a hub on
/// the tile named. Where the tile and the bag allow it, and the player is
/// on its feet, not swimming, it is set to work.
pub fn try_build(
    mut reader: MessageReader<Try>,
    settings: Res<common_bevy::settings::Settings>,
    mut site: Site,
    time: Res<Time>,
    mut commands: Commands,
    mut players: Query<(&Loc, &mut Heading, &mut common_bevy::components::Turn, &mut Position, &mut Inventory, &Character, Has<Working>, Has<Building>, Option<&common_bevy::components::entity_type::EntityType>, Option<&common_bevy::components::status::Status>)>,
) {
    for message in reader.read() {
        let (ent, job, named) = match message {
            Try { event: Event::Build { ent, construct } } => (*ent, construct.map_or(Job::TakeDown, Job::Build), None),
            Try { event: Event::Raise { ent, fixture, q, r } } => (*ent, Job::Raise(*fixture), Some((*q, *r))),
            _ => continue,
        };
        let Ok((loc, mut heading, mut turn, mut position, mut held, &Character(who), working, building, typ, status)) = players.get_mut(ent) else {
            warn!("build: {ent} is not a player");
            continue;
        };
        let draft = common_bevy::components::entity_type::actor::ActorIdentity::draft_of(typ, status);
        if working || building || common_bevy::systems::movement::swimming(*position, draft, site.ground.map()) {
            continue;
        }
        // A fixture goes on the nearest tile its hub takes it on, as the ground lies now
        if let (Job::Raise(_), Some(id)) = (job, site.hubs.holding((loc.q, loc.r))) {
            crate::systems::settlers::survey(&mut site, id);
        }
        let (q, r) = match (job, named) {
            (Job::Raise(fixture), Some(tile)) => site.snapped(**loc, tile, fixture).unwrap_or(tile),
            _ => faced(**loc, *heading),
        };
        // Provided, the bag is given what the job takes and it lacks, once the job is one it may do
        let given = if crate::systems::settlers::provided(&settings) { lacking(&held, job) } else { Vec::new() };
        let mut with = held.clone();
        given.iter().for_each(|stack| with.add(*stack));
        let bag = &with;
        let cover = match site.planned(who, **loc, (q, r), job, bag) {
            Ok(_) => {
                for stack in &given {
                    held.add(*stack);
                    site.moved.write(Moved::stack(*stack, Holder::World, Holder::Character(who), Cause::Grant));
                }
                if !given.is_empty() {
                    site.writer.write(Do { event: Event::Inventory { ent, bag: held.clone() } });
                }
                site.ground.cover(q, r).unwrap_or_default()
            }
            Err(why) => {
                info!("build: {ent} at {:?} cannot {job:?} on ({q}, {r}): {why:?}", **loc);
                continue;
            }
        };
        let work = match job {
            Job::Build(construct) => construct.work(),
            Job::Raise(_) => common::gathering::Work::Build,
            Job::TakeDown => building::standing(cover).map_or(common::gathering::Work::Chop, Construct::work),
        };
        // A fixture raised from further off: the player steps to the open
        // tile beside it nearest where it stands, and turns to it, as one
        // steps to what it gathers
        if let (Job::Raise(_), true) = (job, qrz::hex_distance((loc.q, loc.r), (q, r)) > 1) {
            let map = site.ground.map();
            let centre = |q: i32, r: i32| Vec2::from(common_bevy::geometry::flat_top_tile_center(q - position.tile.q, r - position.tile.r, 1.0));
            let open = |tile: &Qrz| !common_bevy::systems::movement::is_full_cover(map, tile.q, tile.r);
            let beside = map.get_by_qr(q, r).and_then(|(floor, _)| map.neighbors(floor).into_iter().map(|(tile, _)| tile).filter(open).min_by_key(|tile| (qrz::hex_distance((loc.q, loc.r), (tile.q, tile.r)), tile.q, tile.r)));
            if let Some(tile) = beside {
                let stand = centre(tile.q, tile.r);
                let facing = Heading::facing(stand, centre(q, r)).unwrap_or(*heading);
                *position = common_bevy::systems::movement::stepped(*position, stand, map);
                (*heading, turn.heading) = (facing, facing);
            }
        }
        let now = Moment::ZERO + time.elapsed();
        let until = now + std::time::Duration::from_millis(common::gathering::WORK_MS);
        commands.entity(ent).insert((Building { q, r, job, until }, Stance::new(*position, *heading, now)));
        commands.write_message(Do { event: Event::Activity { ent, activity: Some(common::gathering::Activity::Work(work)) } });
    }
}

/// Stops `ent`'s work on a construct.
fn stop(ent: Entity, commands: &mut Commands) {
    commands.entity(ent).remove::<Building>();
    commands.write_message(Do { event: Event::Activity { ent, activity: None } });
}

/// Ends each player's work on a construct: broken off where it has left
/// its stance; done when its time is up, if the tile and
/// the bag still allow it. Building spends the materials and stands the
/// construct, a hearth as a home in a hub; taking down lays the materials
/// as a pile, and a hearth's players are at home at the haven.
#[allow(clippy::too_many_arguments)]
pub fn finish_building(
    mut site: Site,
    mut camps: ResMut<Camps>,
    settling: Res<Settling>,
    time: Res<Time>,
    runtime: Res<combat::RunTime>,
    mut commands: Commands,
    mut players: Query<(Entity, &Loc, &Heading, &Position, &mut Inventory, &Character, &Building, &mut Stance)>,
    homes: Query<(Entity, &Character, Option<&Home>, Option<&Lives>)>,
) {
    let now = Moment::ZERO + time.elapsed();
    for (ent, loc, heading, position, mut bag, &Character(who), work, mut stance) in &mut players {
        if !stance.kept(*position, *heading, now) {
            stop(ent, &mut commands);
            continue;
        }
        if now < work.until {
            continue;
        }
        stop(ent, &mut commands);
        let tile = (work.q, work.r);
        let cover = match site.planned(who, **loc, tile, work.job, &bag) {
            Ok(cover) if matches!(work.job, Job::Raise(_)) || faced(**loc, *heading) == tile => cover,
            refused => {
                info!("build: {ent} finished {:?} on {tile:?}, which it can no longer do: {refused:?}", work.job);
                continue;
            }
        };
        match work.job {
            Job::Build(construct) => {
                let cost = construct.cost();
                bag.remove(cost.kind, cost.count);
                site.moved.write(Moved::stack(cost, Holder::Character(who), Holder::World, Cause::Build));
                site.writer.write(Do { event: Event::Inventory { ent, bag: bag.clone() } });
                site.constructs.raise(tile, Built { by: Owner::Player(who) });
                site.ground.set(&mut site.writer, tile.0, tile.1, cover);
                info!("build: {ent} built a {} on {tile:?}", construct.name());
                if construct.kind == Kind::Hearth {
                    let hub = site.hubs.founded(tile);
                    camps.found(tile, runtime.when(&time) + settling.arrives);
                    info!("build: the hearth on {tile:?} founds {hub:?}");
                    // Its builder returns there from then
                    if let Some((z, _)) = site.ground.lies(tile.0, tile.1) {
                        let home = Qrz { q: tile.0, r: tile.1, z: z + 1 };
                        commands.entity(ent).insert(Home(home));
                        site.writer.write(Do { event: Event::Home { ent, home: Some(home) } });
                    }
                }
                report(&site.hubs);
            }
            Job::Raise(fixture) => {
                let plan = site.hubs.holding(tile).zip(what_of(fixture)).and_then(|(hub, what)| Some((hub, what, site.hubs.raised_at(hub, what, tile)?)));
                let Some(((hub, what, plan), spent)) = plan.zip(fixture.needs().taken(&bag.stock)) else { continue };
                for stack in spent {
                    bag.remove(stack.kind, stack.count);
                    site.moved.write(Moved::stack(stack, Holder::Character(who), Holder::World, Cause::Build));
                }
                site.writer.write(Do { event: Event::Inventory { ent, bag: bag.clone() } });
                site.hubs.carry(&plan);
                site.constructs.raise(tile, Built { by: Owner::Camp(hub.0, hub.1) });
                site.ground.set(&mut site.writer, tile.0, tile.1, cover);
                info!("build: {ent} raised a {} on {tile:?} for {hub:?}", fixture.name());
                if what == What::Post {
                    camps.posted((hub.0, hub.1), runtime.when(&time) + settling.arrives);
                    info!("build: {hub:?} has a hitching post, and a settler is on its way");
                }
                report(&site.hubs);
            }
            Job::TakeDown => {
                let Some((_, stack)) = site.ground.cover(tile.0, tile.1).and_then(building::razed) else { continue };
                // A hub goes with the hearth its founder takes down, and
                // its camp: what the settler held is the world's again,
                // and what it raised stands, no one's
                let hub = site.hubs.of_hearth(tile).filter(|&id| site.hubs.hearth(id) == Some(tile));
                site.constructs.fell(tile);
                if let Some(id) = hub {
                    site.hubs.razed(id);
                    let (held, settler) = camps.razed((id.0, id.1));
                    for stack in held {
                        site.moved.write(Moved::stack(stack, Holder::Camp(id.0, id.1), Holder::World, Cause::TakeDown));
                    }
                    if let Some(settler) = settler {
                        commands.write_message(Do { event: Event::Despawn { ent: settler, died: false } });
                    }
                }
                site.piles.lay((tile.0, tile.1, building::SLOT), stack);
                site.moved.write(Moved::stack(stack, Holder::World, Holder::Pile(tile.0, tile.1, building::SLOT), Cause::TakeDown));
                site.ground.set(&mut site.writer, tile.0, tile.1, cover);
                rehouse(&site, &homes, &mut commands);
                info!("build: {ent} took down what stood on {tile:?}");
                report(&site.hubs);
            }
        }
    }
}

/// Says how every hub stands.
fn report(hubs: &Hubs) {
    for (id, hearth, size) in hubs.each() {
        info!("hubs: {id:?} has its hearth on {hearth:?}, planned for {}, {} things, a wall of {}, {size} players and {} settlers who defend {}", hubs.wants(id), hubs.items(id).len(), hubs.ground(id).map_or(0, |ground| ground.wall.len()), hubs.settlers(id), hubs.held(id));
    }
}

/// What `job` takes that `bag` does not hold, as stacks.
fn lacking(bag: &Inventory, job: Job) -> Vec<common::Stack> {
    match job {
        Job::Build(construct) => {
            let cost = construct.cost();
            let short = cost.count.saturating_sub(bag.count(cost.kind));
            (short > 0).then_some(common::Stack { kind: cost.kind, count: short }).into_iter().collect()
        }
        Job::Raise(fixture) => crate::systems::settlers::stacks(fixture.needs().less(&bag.stock)).collect(),
        Job::TakeDown => Vec::new(),
    }
}

/// Answers each player asking where a fixture it would raise by hand
/// would stand: the tile [`try_build`] would stand it on, where it would
/// raise it at all.
pub fn try_site(mut reader: MessageReader<Try>, settings: Res<common_bevy::settings::Settings>, mut site: Site, mut commands: Commands, players: Query<(&Loc, &Inventory, &Character)>) {
    for message in reader.read() {
        let Try { event: Event::Sited { ent, fixture, q, r, .. } } = message else { continue };
        let (ent, fixture, named) = (*ent, *fixture, (*q, *r));
        let Ok((loc, bag, &Character(who))) = players.get(ent) else { continue };
        // Where it would stand is found on the ground as it lies now
        if let Some(id) = site.hubs.holding(named).or_else(|| site.hubs.holding((loc.q, loc.r))) {
            crate::systems::settlers::survey(&mut site, id);
        }
        let mut with = bag.clone();
        if crate::systems::settlers::provided(&settings) {
            lacking(bag, Job::Raise(fixture)).into_iter().for_each(|stack| with.add(stack));
        }
        let at = site.snapped(**loc, named, fixture).filter(|&at| site.planned(who, **loc, at, Job::Raise(fixture), &with).is_ok());
        let (q, r) = at.unwrap_or(named);
        commands.write_message(Do { event: Event::Sited { ent, fixture, q, r, stands: at.is_some() } });
    }
}

/// Holds where each player returns and where it lives to what the hubs
/// say, and tells its client what changed: it returns to its hub's hearth
/// where the one it claimed is one no more, and to the haven where no hub
/// holds that ground; it lives in no home where its own stands no more.
pub fn rehouse(site: &Site, homes: &Query<(Entity, &Character, Option<&Home>, Option<&Lives>)>, commands: &mut Commands) {
    let stood = |(q, r): (i32, i32)| Some(Qrz { q, r, z: site.ground.lies(q, r)?.0 + 1 });
    for (ent, &Character(who), home, lives) in homes.iter() {
        if let Some(home) = home {
            let now = site.hubs.returns((home.0.q, home.0.r));
            if now != Some((home.0.q, home.0.r)) {
                let home = now.and_then(stood);
                match home {
                    Some(home) => commands.entity(ent).insert(Home(home)),
                    None => commands.entity(ent).remove::<Home>(),
                };
                commands.write_message(Do { event: Event::Home { ent, home } });
            }
        }
        if let Some(lives) = lives {
            let now = site.hubs.home_of(who);
            if now != Some((lives.0.q, lives.0.r)) {
                let home = now.and_then(stood);
                match home {
                    Some(home) => commands.entity(ent).insert(Lives(home)),
                    None => commands.entity(ent).remove::<Lives>(),
                };
                commands.write_message(Do { event: Event::Lives { ent, home } });
            }
        }
    }
}

/// Breaks off the building of each player something lands on.
pub fn interrupt_building(mut commands: Commands, mut reader: MessageReader<Do>, building: Query<(), With<Building>>) {
    for message in reader.read() {
        let Do { event: Event::Resolved { target: Some(struck), outcome: Outcome::Landed, .. } } = message else { continue };
        if building.contains(*struck) {
            stop(*struck, &mut commands);
        }
    }
}

/// Answers each G a player presses on a hearth or a tent in its reach. A
/// hub's hearth is where the player returns from then, whoever it is. A
/// tent that is a home of a hub with room for the player is its home: the
/// player lives in the hub, and the hearth its founder laid is the hub's
/// from then.
pub fn try_use(mut reader: MessageReader<Try>, mut site: Site, mut commands: Commands, players: Query<(&Loc, &Heading, &Character)>) {
    for message in reader.read() {
        let Try { event: Event::Gather { ent, q, r, slot } } = message else { continue };
        let (ent, q, r, slot) = (*ent, *q, *r, *slot as usize);
        let Ok((loc, heading, &Character(who))) = players.get(ent) else { continue };
        if !crate::systems::gathering::in_reach(**loc, *heading, q, r) {
            continue;
        }
        // Any tile of a home on more than one claims the home, where it stands
        let (q, r) = building::holder(|tile| site.ground.cover(tile.0, tile.1).unwrap_or_default(), (q, r));
        let Some((z, cover)) = site.ground.lies(q, r) else { continue };
        if slot >= common::TILE_SLOTS as usize || !building::used(cover, slot) {
            continue;
        }
        let tile = (q, r);
        let home = Qrz { q, r, z: z + 1 };
        if site.hubs.hearthed(tile) {
            commands.entity(ent).insert(Home(home));
            site.writer.write(Do { event: Event::Home { ent, home: Some(home) } });
            info!("build: {ent} returns to the hearth on {tile:?} from now");
            continue;
        }
        // Refused, the player lives where it did
        let Some(hub) = site.hubs.holding(tile).filter(|_| site.hubs.claim(who, tile)) else {
            info!("build: {ent} cannot make its home on {tile:?}, which has no room or is no hub's");
            continue;
        };
        commands.entity(ent).insert(Lives(home));
        if let Some(hearth) = site.hubs.hearth(hub).filter(|_| building::fixture(cover).is_some()) {
            site.constructs.raise(hearth, Built { by: Owner::Camp(hub.0, hub.1) });
        }
        site.writer.write(Do { event: Event::Lives { ent, home: Some(home) } });
        info!("build: {ent} lives on {tile:?}, of {hub:?}");
        report(&site.hubs);
    }
}

/// A player standing on ground a hub holds.
#[derive(Component)]
pub struct Sheltered;

/// Tells each player's client whether it stands on ground a hub holds, as
/// it enters the world and whenever that changes.
pub fn shelter(mut commands: Commands, hubs: Res<Hubs>, mut writer: MessageWriter<Do>, players: Query<(Entity, &Loc, Ref<Character>, Has<Sheltered>)>) {
    for (ent, loc, character, was) in &players {
        let held = hubs.holding((loc.q, loc.r)).is_some();
        if held == was && !character.is_added() {
            continue;
        }
        if held {
            commands.entity(ent).insert(Sheltered);
        } else {
            commands.entity(ent).remove::<Sheltered>();
        }
        writer.write(Do { event: Event::Sheltered { ent, held } });
    }
}

/// Counts each character that enters the world among the players of late,
/// living where it left while that home still has it, and returning to
/// the hearth it left by while one is still laid there. Tells its client
/// both.
pub fn house(mut commands: Commands, mut hubs: ResMut<Hubs>, mut writer: MessageWriter<Do>, entered: Query<(Entity, &Character, Option<&Home>, Option<&Lives>), Added<Character>>) {
    for (ent, &Character(who), home, lives) in &entered {
        hubs.resident(who, lives.map(|lives| (lives.0.q, lives.0.r)));
        let lives = lives.map(|lives| lives.0).filter(|_| hubs.home_of(who).is_some());
        if lives.is_none() {
            commands.entity(ent).remove::<Lives>();
        }
        let home = home.map(|home| home.0).filter(|home| hubs.hearthed((home.q, home.r)));
        if home.is_none() {
            commands.entity(ent).remove::<Home>();
        }
        writer.write(Do { event: Event::Home { ent, home } });
        writer.write(Do { event: Event::Lives { ent, home: lives } });
    }
}
