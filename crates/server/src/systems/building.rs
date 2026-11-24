//! Building: a player spends its bag's wood or stone on the tile it
//! faces, works, and a construct stands there. Taking its own construct
//! down is the same work, and lays what it was built of as a pile. G uses
//! a construct: a gate swings, and a hearth becomes the player's home.
//! A hearth is a home in a hub ([`Hubs`]), founding one where no hub's
//! ground reaches it, and a settler comes to it ([`Camps`]). What a
//! settler raises is its hub's, and no player takes it down; a player
//! who claims the tent one raised makes its home there, and the hearth it
//! built is the hub's from then.

use std::collections::HashMap;

use bevy::prelude::*;
use common::{
    building::{self, Construct, Kind},
    network::Identity,
};
use common_bevy::{
    components::{equipment::Inventory, heading::Heading, position::Position, Home, Loc},
    hubs::{Hubs, OUTPOST_CORE},
    message::{Do, Event, Outcome, Try},
    moment::Moment,
    plugins::nntree::NNTree,
};
use qrz::Qrz;

use crate::systems::{
    characters::Character,
    dens::Dens,
    economy::{Cause, Holder, Moved},
    gathering::{Ground, Piles, Working},
    settlers::{Camps, Settling},
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
}

/// What a player is at work on, on the tile it faces.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Job {
    Build(Construct),
    TakeDown,
}

/// The work a player is at: `job` on tile `(q, r)`, done at `until`, from
/// where it stood and faced when it began. Moving from there, turning
/// from that facing or being struck breaks it off with nothing spent.
#[derive(Component, Clone, Copy, Debug)]
pub struct Building {
    pub q: i32,
    pub r: i32,
    pub job: Job,
    pub until: Moment,
    pub from: Position,
    pub facing: Heading,
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
    /// A den stands on the core a founding hearth would hold.
    Den,
    /// No construct stands there to take down.
    Nothing,
    /// The construct is another player's.
    NotYours,
}

/// The tile a player on `loc` facing `heading` builds on: the one its
/// heading is nearest.
fn faced(loc: Qrz, heading: Heading) -> (i32, i32) {
    let dir = heading.hex_dir();
    (loc.q + dir.q, loc.r + dir.r)
}

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
    /// Tile `tile`'s cover as `who`, standing at `loc`, would leave it by
    /// `job`, or why it cannot.
    fn planned(&self, who: Identity, loc: Qrz, tile: (i32, i32), job: Job, bag: &Inventory) -> Result<common::Cover, Refusal> {
        let (z, cover) = self.ground.lies(tile.0, tile.1).filter(|&(z, _)| (z + 1 - loc.z).abs() <= 1).ok_or(Refusal::NoTile)?;
        match job {
            Job::Build(construct) => {
                let raised = construct.raised(cover).ok_or(Refusal::NoRoom)?;
                let cost = construct.cost();
                if bag.count(cost.kind) < cost.count {
                    return Err(Refusal::NoMaterials);
                }
                if occupied(&self.nntree, tile, z) {
                    return Err(Refusal::Occupied);
                }
                let founds = construct.kind == Kind::Hearth && self.hubs.holding(tile).is_none();
                if founds && self.dens.stands_within(Qrz { q: tile.0, r: tile.1, z }, OUTPOST_CORE as i32) {
                    return Err(Refusal::Den);
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
/// faces, or what stands there taken down. Where the tile and the bag
/// allow it, the player is set to work.
pub fn try_build(
    mut reader: MessageReader<Try>,
    site: Site,
    time: Res<Time>,
    mut commands: Commands,
    players: Query<(&Loc, &Heading, &Position, &Inventory, &Character, Has<Working>, Has<Building>)>,
) {
    for message in reader.read() {
        let Try { event: Event::Build { ent, construct } } = message else { continue };
        let (ent, job) = (*ent, construct.map_or(Job::TakeDown, Job::Build));
        let Ok((loc, heading, position, bag, &Character(who), working, building)) = players.get(ent) else {
            warn!("build: {ent} is not a player");
            continue;
        };
        if working || building {
            continue;
        }
        let (q, r) = faced(**loc, *heading);
        let cover = match site.planned(who, **loc, (q, r), job, bag) {
            Ok(_) => site.ground.cover(q, r).unwrap_or_default(),
            Err(why) => {
                info!("build: {ent} at {:?} cannot {job:?} on ({q}, {r}): {why:?}", **loc);
                continue;
            }
        };
        let work = match job {
            Job::Build(construct) => construct.work(),
            Job::TakeDown => building::standing(cover).map_or(common::gathering::Work::Chop, |(construct, _)| construct.work()),
        };
        let until = Moment::ZERO + time.elapsed() + std::time::Duration::from_millis(common::gathering::WORK_MS);
        commands.entity(ent).insert(Building { q, r, job, until, from: *position, facing: *heading });
        commands.write_message(Do { event: Event::Activity { ent, activity: Some(common::gathering::Activity::Work(work)) } });
    }
}

/// Stops `ent`'s work on a construct.
fn stop(ent: Entity, commands: &mut Commands) {
    commands.entity(ent).remove::<Building>();
    commands.write_message(Do { event: Event::Activity { ent, activity: None } });
}

/// Ends each player's work on a construct: broken off where it has moved
/// or turned since it began; done when its time is up, if the tile and
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
    mut players: Query<(Entity, &Loc, &Heading, &Position, &mut Inventory, &Character, &Building)>,
    homes: Query<(Entity, &Home)>,
) {
    for (ent, loc, heading, position, mut bag, &Character(who), work) in &mut players {
        if *position != work.from || *heading != work.facing {
            stop(ent, &mut commands);
            continue;
        }
        if Moment::ZERO + time.elapsed() < work.until {
            continue;
        }
        stop(ent, &mut commands);
        let tile = (work.q, work.r);
        let cover = match site.planned(who, **loc, tile, work.job, &bag) {
            Ok(cover) if faced(**loc, *heading) == tile => cover,
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
                    info!("build: the hearth on {tile:?} is a home in {hub:?}, and a settler is on its way");
                }
                report(&site.hubs);
            }
            Job::TakeDown => {
                let Some((_, stack)) = site.ground.cover(tile.0, tile.1).and_then(building::razed) else { continue };
                let hearth = site.ground.cover(tile.0, tile.1).and_then(building::standing).is_some_and(|(construct, _)| construct.kind == Kind::Hearth);
                site.constructs.0.remove(&tile);
                if hearth {
                    site.hubs.razed(tile);
                    // Its camp goes with it: what the settler held is the
                    // world's again, and what it raised stands, no one's
                    let (held, settler) = camps.razed(tile);
                    for stack in held {
                        site.moved.write(Moved::stack(stack, Holder::Camp(tile.0, tile.1), Holder::World, Cause::TakeDown));
                    }
                    if let Some(settler) = settler {
                        commands.write_message(Do { event: Event::Despawn { ent: settler, died: false } });
                    }
                }
                site.piles.lay((tile.0, tile.1, building::SLOT), stack);
                site.moved.write(Moved::stack(stack, Holder::World, Holder::Pile(tile.0, tile.1, building::SLOT), Cause::TakeDown));
                site.ground.set(&mut site.writer, tile.0, tile.1, cover);
                if hearth {
                    for (resident, _) in homes.iter().filter(|(_, home)| (home.0.q, home.0.r) == tile) {
                        commands.entity(resident).remove::<Home>();
                    }
                }
                info!("build: {ent} took down what stood on {tile:?}");
                report(&site.hubs);
            }
        }
    }
}

/// Says how every hub stands.
fn report(hubs: &Hubs) {
    for (id, centre, core, size) in hubs.each() {
        info!("hubs: {id:?} is centred on {centre:?}, its core {core:.1} tiles, {size} at home");
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

/// Answers each G a player presses on a construct in its reach: a gate
/// swings, shut only while no one stands in it, and a hearth becomes the
/// player's home, as does the tent a settler raised for a player to
/// claim. The first to claim a tent makes it a home in its hub, and the
/// hearth of its camp the hub's.
pub fn try_use(
    mut reader: MessageReader<Try>,
    mut site: Site,
    mut camps: ResMut<Camps>,
    mut commands: Commands,
    players: Query<(&Loc, &Heading, &Character)>,
) {
    for message in reader.read() {
        let Try { event: Event::Gather { ent, q, r, slot } } = message else { continue };
        let (ent, q, r, slot) = (*ent, *q, *r, *slot as usize);
        let Ok((loc, heading, &Character(who))) = players.get(ent) else { continue };
        let Some((z, cover)) = site.ground.lies(q, r) else { continue };
        if slot >= common::TILE_SLOTS as usize || !building::used(cover, slot) || !crate::systems::gathering::in_reach(**loc, *heading, q, r) {
            continue;
        }
        if building::fixture(cover) == Some(building::Fixture::Tent) {
            let tile = (q, r);
            let Some(hearth) = camps.of_spare(tile) else {
                info!("build: {ent} cannot claim the tent on {tile:?}, which is its settler's own");
                continue;
            };
            let home = Qrz { q, r, z: z + 1 };
            commands.entity(ent).insert(Home(home));
            let hub = site.hubs.founded(tile);
            site.hubs.resident(who, Some(tile));
            if camps.on(hearth).is_some_and(|camp| !camp.claimed) {
                camps.claimed(hearth);
                site.constructs.raise(hearth, Built { by: Owner::Camp(hearth.0, hearth.1) });
                site.ground.piled(hearth.0, hearth.1);
            }
            site.writer.write(Do { event: Event::Home { ent, home: Some(home) } });
            info!("build: {ent} is at home in the tent on {tile:?}, of {hub:?}");
            continue;
        }
        match building::standing(cover) {
            Some((Construct { kind: Kind::Gate, .. }, open)) => {
                if open && occupied(&site.nntree, (q, r), z) {
                    info!("build: {ent} cannot shut the gate on ({q}, {r}) while someone stands in it");
                    continue;
                }
                let Some(swung) = building::swung(cover) else { continue };
                site.ground.set(&mut site.writer, q, r, swung);
            }
            Some((Construct { kind: Kind::Hearth, .. }, _)) => {
                let home = Qrz { q, r, z: z + 1 };
                commands.entity(ent).insert(Home(home));
                site.hubs.resident(who, Some((q, r)));
                site.writer.write(Do { event: Event::Home { ent, home: Some(home) } });
                info!("build: {ent} is at home at the hearth on ({q}, {r}), of {:?}", site.hubs.of_hearth((q, r)));
            }
            _ => {}
        }
    }
}

/// Counts each character that enters the world among the players of late,
/// at home where it left: at its hearth while a hub still holds that, at
/// the haven otherwise. Tells its client which.
pub fn house(mut commands: Commands, mut hubs: ResMut<Hubs>, mut writer: MessageWriter<Do>, entered: Query<(Entity, &Character, Option<&Home>), Added<Character>>) {
    for (ent, &Character(who), home) in &entered {
        let hearth = home.map(|home| (home.0.q, home.0.r));
        hubs.resident(who, hearth);
        if hearth.is_some() && hubs.home_of(who).is_none() {
            commands.entity(ent).remove::<Home>();
        }
        let home = home.map(|home| home.0).filter(|_| hubs.home_of(who).is_some());
        writer.write(Do { event: Event::Home { ent, home } });
    }
}
