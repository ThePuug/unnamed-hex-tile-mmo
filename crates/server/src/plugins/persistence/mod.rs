//! Keeps the changed world, its dens and its characters past the server, in the
//! PostgreSQL at `DATABASE_URL`, or the local one ([`LOCAL`]) where that is
//! unset. The rest of the server never learns how: kept changes are
//! recalled into [`WorldChanges`], [`Piles`] and [`Dens`], and kept
//! characters into [`Characters`], as they are asked after, and what players change is
//! written behind play.
//!
//! One server writes a chunk at a time, and one an account. Recalling a
//! chunk to build claims it, as recalling a player's character claims its
//! account, raising its ownership number. A write is one transaction
//! carrying everything changed since the last, and one naming a number
//! since raised is refused whole. A box of tiles read for a summary claims
//! nothing.
//!
//! A write is due the frame an item appears, is consumed or changes
//! holder, and waits on the disk; anything else waits for [`FLUSH`]. Every
//! change a player makes to a tile today moves an item, a gather's yield or
//! a pile's contents, so every [`TileChanged`] is such a change, as is
//! every change to a bag. Each such move the game announces ([`Moved`]) is
//! appended to the ledger in the write that carries its holders.
//!
//! Everything kept is known by a number given it once ([`stored`]), so a
//! build that renames a thing still reads it. What a build has no number
//! for stays in the store: [`Keeper`] holds it beside the character or the
//! pile it was kept with, and writes it back with them.
//!
//! Every time kept is a moment of the calendar (`systems::When`), and the
//! calendar as an admin left it is kept with them, so a server that
//! starts reads each on the calendar it was kept on. No clock is
//! translated here, and no moment of the game clock reaches the store.
//!
//! An admin may roll the whole world back to an earlier moment
//! ([`roll_back`]): the store undoes every change it journaled since, the
//! calendar goes back as far, and the server, whose memory is then of a
//! world that is gone, writes nothing more and stops, to be started
//! again.
//!
//! No system waits on the store: every query is a task on the IO pool,
//! polled each frame, so a slow store delays recalls and writes and never
//! the simulation.

mod camp;
mod character;
mod den;
mod hub;
mod ledger;
mod store;
mod stored;

use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use bevy::{
    prelude::*,
    tasks::{block_on, futures_lite::future::poll_once, IoTaskPool, Task},
};
use common::network::Identity;
use common_bevy::{
    chunk::{loc_to_chunk, ChunkId},
    hubs::Hubs,
    components::{
        equipment::{Equipment, Inventory},
        heading::Heading,
        loadout::Loadout,
        ActorAttributes, Home, Lives, Loc,
    },
    moment::Moment,
    settings::{Setting, Settings},
    systems::When,
};
use qrz::Qrz;

use crate::{
    resources::event_registry::EventRegistry,
    systems::{
        building::Constructs,
        characters::{Character, Characters},
        charts::Charts,
        dens::Dens,
        economy::Moved,
        gathering::{Piles, TileChanged, WorldChanges},
        haul::Depots,
        settlers::Camps,
    },
};
use character::Dormant;
use store::{Batch, Entered, PileKey, Recalled, Refused, Store};

/// How often what players changed is written where no item moved. A crash
/// loses at most this much of it.
const FLUSH: Duration = Duration::from_secs(5);

use common::settings::{store_url as url, LOCAL_STORE as LOCAL};

/// Every setting kept, as the store tells of a change to one
/// (`Store::hear`), and the task hearing it.
#[derive(Resource)]
struct Heard(std::sync::Mutex<std::sync::mpsc::Receiver<Vec<(i32, i32)>>>, #[allow(dead_code)] Task<()>);

pub struct PersistencePlugin;

impl Plugin for PersistencePlugin {
    fn build(&self, app: &mut App) {
        let url = url();
        let store = block_on(Store::open(&url)).unwrap_or_else(|e| {
            panic!("the store will not open: {e}\nThe server keeps to the PostgreSQL at DATABASE_URL, or to {LOCAL} where that is unset; AGENTS.md says how that one is made.")
        });
        info!("keeping the changed world and its characters in the store");
        // Before anything is built, so a cold setting is read as kept by
        // whatever starts after
        let mut settings = Settings::default();
        for (id, value) in block_on(store.settings()).unwrap_or_else(|e| panic!("the store's settings cannot be read: {e}")) {
            match Setting::of(id) {
                Some(setting) => {
                    settings.set(setting, value);
                    info!("store: {} is {}", setting.name(), settings.get(setting));
                }
                None => warn!("store: this build has no setting {id}; it stays kept at {value}"),
            }
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let hearing = store.clone();
        let hearing = IoTaskPool::get().spawn(async move {
            if let Err(e) = hearing.hear(tx).await {
                error!("store: no longer listening, so a setting changed goes unheard until the server starts again: {e}");
            }
        });
        app.insert_resource(settings)
            .insert_resource(Heard(std::sync::Mutex::new(rx), hearing))
            .insert_resource(Keeper::new(store))
            .add_systems(Startup, keep.after(crate::systems::world::setup))
            .add_systems(Update, (heed, roll_back, note, ask, receive, flush).chain())
            .add_systems(Last, flush_at_exit);
    }
}

/// A player's account as this server holds it.
#[derive(Clone, Copy)]
struct Held {
    account: i64,
    /// The ownership number the account was claimed with.
    epoch: i64,
    character: i64,
}

/// What a write in flight carries, to be written again should it fail.
struct Carried {
    tiles: Vec<(i32, i32)>,
    dens: Vec<Qrz>,
    /// Each camp, by its hearth.
    camps: Vec<(i32, i32)>,
    /// Each character, and its state as written.
    characters: Vec<(Identity, String)>,
    hubs: bool,
    calendar: bool,
    /// Each leaf a map charted, by the map's number and the chunk, and
    /// each map used up.
    leaves: Vec<(u32, ChunkId)>,
    spent: Vec<u32>,
    /// Each depot, by the tile its hub is known by.
    depots: Vec<(i32, i32)>,
    /// The economy interactions it appends to the ledger.
    moved: Vec<Moved>,
    economy: bool,
}

impl Carried {
    fn is_empty(&self) -> bool {
        self.tiles.is_empty() && self.dens.is_empty() && self.camps.is_empty() && self.characters.is_empty() && !self.hubs && !self.calendar && self.leaves.is_empty() && self.spent.is_empty() && self.depots.is_empty() && self.moved.is_empty()
    }
}

/// The store and what this server knows of it.
#[derive(Resource)]
struct Keeper {
    store: Store,
    /// The ownership number each chunk this server holds was claimed with.
    epochs: HashMap<ChunkId, i64>,
    /// The accounts this server holds.
    accounts: HashMap<Identity, Held>,
    /// Tiles players changed since the last write.
    dirty: HashSet<(i32, i32)>,
    /// Den sites whose den changed since the last write.
    changed_dens: HashSet<Qrz>,
    /// Hearths whose camp changed since the last write.
    changed_camps: HashSet<(i32, i32)>,
    /// What each camp holds that this build cannot play.
    camped: HashMap<(i32, i32), Vec<serde_json::Value>>,
    /// Whether a hub changed since the last write.
    changed_hubs: bool,
    /// The leaves maps charted since the last write, each by its map's
    /// number and its chunk, and the maps used up.
    charted: HashSet<(u32, ChunkId)>,
    spent: Vec<u32>,
    /// Hubs whose depot changed since the last write.
    changed_depots: HashSet<(i32, i32)>,
    /// The calendar's moment this frame.
    when: When,
    /// The calendar as the store holds it, how far from the wall clock
    /// and where held, and whether it has changed since the last write.
    calendar: (i64, Option<i64>),
    changed_calendar: bool,
    /// Whether the world was rolled back under this server: it writes
    /// nothing more.
    rolled: bool,
    /// Characters to write whenever the next write goes: one whose bag
    /// changed, one that left the world.
    changed: HashSet<Identity>,
    /// Each character's state as the store holds it.
    written: HashMap<Identity, String>,
    /// What each character holds that this build cannot play.
    dormant: HashMap<Identity, Dormant>,
    /// What lies in each pile's place that this build cannot play.
    lying: HashMap<PileKey, Vec<serde_json::Value>>,
    /// The economy interactions since the last write, oldest first.
    moved: Vec<Moved>,
    /// Whether an item moved since the last write: the next goes now.
    due: bool,
    recalls: Vec<(Vec<ChunkId>, Task<Result<Recalled, sqlx::Error>>)>,
    reads: Vec<(((i32, i32), (i32, i32)), Task<Result<Vec<(i32, i32, common::Cover)>, sqlx::Error>>)>,
    entering: Vec<(Identity, Task<Result<Entered, sqlx::Error>>)>,
    /// The write in flight. One at a time, so writes land in the order
    /// they were made.
    write: Option<(Carried, Task<Result<Refused, sqlx::Error>>)>,
    /// Chunks, boxes and players whose query failed, asked again at the
    /// next flush.
    failed: (Vec<ChunkId>, Vec<((i32, i32), (i32, i32))>, Vec<Identity>),
    /// Players whose character the store holds and this build cannot
    /// read. They stay out, and their character stays as it is.
    unreadable: HashSet<Identity>,
    next_flush: Moment,
}

/// A character in the world, and everything of it that is kept.
type Playing = (&'static Character, &'static Equipment, &'static Inventory, &'static ActorAttributes, &'static Loadout, &'static Loc, &'static Heading, Option<&'static Home>, Option<&'static Lives>);

impl Keeper {
    fn new(store: Store) -> Self {
        Self {
            store,
            epochs: HashMap::new(),
            accounts: HashMap::new(),
            dirty: HashSet::new(),
            changed_dens: HashSet::new(),
            changed_camps: HashSet::new(),
            camped: HashMap::new(),
            changed_hubs: false,
            charted: HashSet::new(),
            spent: Vec::new(),
            changed_depots: HashSet::new(),
            when: When(0),
            calendar: (0, None),
            changed_calendar: false,
            rolled: false,
            changed: HashSet::new(),
            written: HashMap::new(),
            dormant: HashMap::new(),
            lying: HashMap::new(),
            moved: Vec::new(),
            due: false,
            recalls: Vec::new(),
            reads: Vec::new(),
            entering: Vec::new(),
            write: None,
            failed: default(),
            unreadable: HashSet::new(),
            next_flush: Moment::ZERO + FLUSH,
        }
    }

    fn recall(&mut self, chunks: Vec<ChunkId>) {
        let store = self.store.clone();
        let asked = chunks.clone();
        self.recalls.push((chunks, IoTaskPool::get().spawn(async move { store.recall(&asked).await })));
    }

    fn read(&mut self, area: ((i32, i32), (i32, i32))) {
        let store = self.store.clone();
        self.reads.push((area, IoTaskPool::get().spawn(async move { store.read(area.0, area.1).await })));
    }

    fn enter(&mut self, identity: Identity) {
        let store = self.store.clone();
        let when = self.when.0 as i64;
        self.entering.push((identity, IoTaskPool::get().spawn(async move { store.enter(identity.provider(), &identity.name(), when).await })));
    }

    /// Whether `identity` is being asked after, or was and cannot be.
    fn asked(&self, identity: Identity) -> bool {
        self.entering.iter().any(|(entering, _)| *entering == identity) || self.failed.2.contains(&identity) || self.unreadable.contains(&identity)
    }

    /// Everything changed since the last write, under the number each
    /// tile's chunk and each character's account was claimed with, and
    /// what it carries. With `every`, every character in the world is
    /// looked at; without, only those marked changed. A character whose
    /// state is as the store holds it is not written.
    #[allow(clippy::too_many_arguments)]
    fn batch(&mut self, world: &World, playing: &Query<Playing>, known: &Characters, every: bool) -> (Carried, Batch) {
        let World { changes, piles, dens, constructs, hubs, camps, charts, depots } = world;
        let mut batch = Batch { economy: std::mem::take(&mut self.due), when: self.when.0 as i64, ..default() };
        let calendar_changed = std::mem::take(&mut self.changed_calendar);
        batch.calendar = calendar_changed.then_some(self.calendar);
        let hubs_changed = std::mem::take(&mut self.changed_hubs);
        if hubs_changed {
            batch.hubs = Some(hubs.kept());
        }
        let mut claimed = HashSet::new();
        let tiles: Vec<(i32, i32)> = self.dirty.drain().collect();
        for &(q, r) in &tiles {
            let chunk = loc_to_chunk(Qrz { q, r, z: 0 });
            let (Some(&epoch), Some(cover)) = (self.epochs.get(&chunk), changes.get(q, r)) else {
                warn!("store: ({q}, {r}) changed on chunk {chunk:?}, which this server does not hold");
                continue;
            };
            if claimed.insert(chunk) {
                batch.epochs.push((chunk, epoch));
            }
            batch.tiles.push((chunk, q, r, cover));
            if let Some(built) = constructs.on((q, r)) {
                batch.constructs.push((chunk, (q, r), hub::built(built)));
            }
            for slot in 0..common::TILE_SLOTS as usize {
                let playable = piles.held((q, r, slot)).map(stored::lot).into_iter();
                let held: Vec<_> = playable.chain(self.lying.get(&(q, r, slot)).into_iter().flatten().cloned()).collect();
                if !held.is_empty() {
                    batch.piles.push((chunk, (q, r, slot), held));
                }
            }
        }

        let sites: Vec<Qrz> = self.changed_dens.drain().collect();
        for &site in &sites {
            let chunk = loc_to_chunk(site);
            let Some(&epoch) = self.epochs.get(&chunk) else {
                warn!("store: the den at ({}, {}) changed on chunk {chunk:?}, which this server does not hold", site.q, site.r);
                continue;
            };
            if claimed.insert(chunk) {
                batch.epochs.push((chunk, epoch));
            }
            batch.dens.push((chunk, (site.q, site.r), dens.den_on(site).and_then(|den| den::kept(&den))));
        }

        let hearths: Vec<(i32, i32)> = self.changed_camps.drain().collect();
        for &(q, r) in &hearths {
            let chunk = loc_to_chunk(Qrz { q, r, z: 0 });
            let Some(&epoch) = self.epochs.get(&chunk) else {
                warn!("store: the camp at ({q}, {r}) changed on chunk {chunk:?}, which this server does not hold");
                continue;
            };
            if claimed.insert(chunk) {
                batch.epochs.push((chunk, epoch));
            }
            let dormant = self.camped.get(&(q, r)).map_or(&[][..], Vec::as_slice);
            batch.camps.push((chunk, (q, r), camps.on((q, r)).map(|camp| camp::kept(camp, dormant))));
        }

        let leaves: Vec<(u32, ChunkId)> = self.charted.drain().collect();
        batch.leaves = leaves.iter().filter_map(|&(map, chunk)| Some((map as i64, chunk, charts.kept(map, chunk)?))).collect();
        let spent = std::mem::take(&mut self.spent);
        batch.spent = spent.iter().map(|&map| map as i64).collect();
        let hubs_left_at: Vec<(i32, i32)> = self.changed_depots.drain().collect();
        batch.depots = hubs_left_at
            .iter()
            .map(|&hub| (hub, serde_json::to_string(&depots.on(hub).iter().copied().map(stored::owned).collect::<Vec<_>>()).expect("stacks serialize")))
            .collect();

        let mut changed = std::mem::take(&mut self.changed);
        let mut states: Vec<(Identity, String)> = Vec::new();
        let none = Dormant::default();
        for (&Character(identity), equipment, inventory, attrs, loadout, loc, heading, home, lives) in playing {
            if changed.remove(&identity) || every {
                let dormant = self.dormant.get(&identity).unwrap_or(&none);
                states.push((identity, character::state(equipment, inventory, attrs, loadout, **loc, *heading, home.map(|home| home.0), lives.map(|lives| lives.0), dormant)));
            }
        }
        // Those left are out of the world, as they left it
        for identity in changed {
            if let Some(kept) = known.as_left(identity) {
                states.push((identity, character::kept(kept, self.dormant.get(&identity).unwrap_or(&none))));
            }
        }
        let mut characters = Vec::new();
        for (identity, state) in states {
            let Some(&held) = self.accounts.get(&identity) else { continue };
            if self.written.get(&identity) == Some(&state) {
                continue;
            }
            if !batch.accounts.contains(&(held.account, held.epoch)) {
                batch.accounts.push((held.account, held.epoch));
            }
            batch.characters.push((held.character, state.clone()));
            characters.push((identity, state));
        }
        let moved = std::mem::take(&mut self.moved);
        for moved in &moved {
            match ledger::entry(moved, |identity| self.accounts.get(&identity).map(|held| held.character)) {
                Some(entry) => batch.ledger.push(entry),
                None => error!("store: {moved:?} names a player whose account this server does not hold; the ledger lacks it"),
            }
        }
        (Carried { tiles, dens: sites, camps: hearths, characters, hubs: hubs_changed, calendar: calendar_changed, leaves, spent, depots: hubs_left_at, moved, economy: batch.economy }, batch)
    }

    /// The write carrying `carried` landed, was refused or failed.
    fn wrote(&mut self, carried: Carried, result: Result<Refused, sqlx::Error>, known: &mut Characters) {
        match result {
            Ok(refused) if refused.is_empty() => {
                debug!("store: wrote {} changed tiles, {} dens and {} characters", carried.tiles.len(), carried.dens.len(), carried.characters.len());
                self.written.extend(carried.characters);
            }
            Ok(refused) => {
                for chunk in refused.chunks {
                    error!("store: chunk {chunk:?} was claimed by another server; changes this one made there are lost");
                    self.epochs.remove(&chunk);
                }
                for account in refused.accounts {
                    let Some((&identity, _)) = self.accounts.iter().find(|(_, held)| held.account == account) else { continue };
                    error!("store: account {account} was claimed by another server; what its character did here since is lost");
                    self.accounts.remove(&identity);
                    self.written.remove(&identity);
                    self.dormant.remove(&identity);
                    known.forget(identity);
                }
                // Nothing of the batch was written: the rest goes again
                self.rewrite(carried);
            }
            Err(e) => {
                warn!("store: writing {} tiles and {} characters failed, writing them again: {e}", carried.tiles.len(), carried.characters.len());
                self.rewrite(carried);
            }
        }
    }

    fn rewrite(&mut self, carried: Carried) {
        self.dirty.extend(carried.tiles);
        self.changed_dens.extend(carried.dens);
        self.changed_camps.extend(carried.camps);
        self.changed_hubs |= carried.hubs;
        self.changed_calendar |= carried.calendar;
        self.charted.extend(carried.leaves);
        self.spent.splice(0..0, carried.spent);
        self.changed_depots.extend(carried.depots);
        self.changed.extend(carried.characters.into_iter().map(|(identity, _)| identity));
        // What it carried happened before anything noted since
        self.moved.splice(0..0, carried.moved);
        self.due |= carried.economy;
    }
}

/// The changed world as the game holds it, read for a write.
#[derive(bevy::ecs::system::SystemParam)]
struct World<'w> {
    changes: Res<'w, WorldChanges>,
    piles: Res<'w, Piles>,
    dens: Res<'w, Dens>,
    camps: Res<'w, Camps>,
    constructs: Res<'w, Constructs>,
    hubs: Res<'w, Hubs>,
    charts: Res<'w, Charts>,
    depots: Res<'w, Depots>,
}

/// From the first frame, a chunk's changes are on the map only once
/// recalled; the calendar stands where an admin left it; and the hubs,
/// which are known whole, are recalled before anything is built, as are
/// the maps, with a run of numbers for those made from here on.
fn keep(
    mut changes: ResMut<WorldChanges>,
    mut hubs: ResMut<Hubs>,
    mut charts: ResMut<Charts>,
    mut depots: ResMut<Depots>,
    mut runtime: ResMut<combat::RunTime>,
    anchor: Res<crate::systems::world::Anchor>,
    time: Res<Time>,
    mut keeper: ResMut<Keeper>,
) {
    changes.keep();
    if let Some((shift, held)) = block_on(keeper.store.calendar()).unwrap_or_else(|e| panic!("the store's calendar cannot be read: {e}")) {
        runtime.calendar.wall_at_zero = (anchor.0 as i128 + shift as i128).max(0) as u128;
        runtime.calendar.held = held.map(|held| held as u128);
        keeper.calendar = (shift, held);
        info!("store: the calendar stands {shift} ms from the wall clock{}", if held.is_some() { ", held" } else { "" });
    }
    keeper.when = runtime.when(&time);
    let settled = block_on(keeper.store.settled(keeper.when.0 as i64)).unwrap_or_else(|e| panic!("the store's hubs cannot be read: {e}"));
    let residents = settled
        .residents
        .into_iter()
        .filter_map(|(provider, name, home)| {
            let home: Option<Qrz> = serde_json::from_value(home).ok()?;
            Some((Identity::named(&provider, &name)?, home.map(|home| (home.q, home.r))))
        })
        .collect::<Vec<_>>();
    info!("store: recalled {} hubs and {} players of late", settled.hubs.len(), residents.len());
    hubs.recall(settled.hubs, residents);
    hubs.take_changed();
    let kept = block_on(keeper.store.charts()).unwrap_or_else(|e| panic!("the store's maps cannot be read: {e}"));
    let leaves = kept.len();
    for (map, chunk, tiles) in kept {
        match (u32::try_from(map), common_bevy::chart::Leaf::recalled(chunk, &tiles)) {
            // A leaf kept with nothing said of its hearths takes those the
            // hubs have now, the nearest there is to what stood, and is
            // kept again saying so
            (Ok(map), Some(leaf)) if common_bevy::chart::Leaf::unmarked(&tiles) => charts.recall_changed(map, leaf.marked(hubs.each().map(|(_, hearth, _)| hearth))),
            (Ok(map), Some(leaf)) => charts.recall(map, leaf),
            _ => error!("store: chunk {chunk:?} of map {map} cannot be read; the map lacks it"),
        }
    }
    info!("store: recalled {leaves} charted chunks");
    for (hub, held) in block_on(keeper.store.depots()).unwrap_or_else(|e| panic!("the store's depots cannot be read: {e}")) {
        let (stacks, unplayable) = stored::owneds(held);
        if !unplayable.is_empty() {
            error!("store: the depot at {hub:?} holds {} things this build cannot play; the next wagon there writes over them", unplayable.len());
        }
        depots.recall(hub, stacks);
    }
    number(&keeper.store, &mut charts);
}

/// Hands `charts` a run of numbers no map has.
fn number(store: &Store, charts: &mut Charts) {
    let (first, count) = block_on(store.numbers()).unwrap_or_else(|e| panic!("the store gives no numbers for maps: {e}"));
    let first = u32::try_from(first).expect("a map's number fits an item's");
    charts.numbered(first..first + count as u32);
}

/// Takes an admin's rollback: lets the write in flight land, has the
/// store put the world back as it was, and stops the server with nothing
/// more written.
/// Takes the settings as the store tells of a change to one: each hot
/// one is in force as it is now kept, one with no row as built, and
/// every client is told; a cold one waits for the next start.
fn heed(
    heard: Res<Heard>,
    mut settings: ResMut<Settings>,
    mut waiting: Local<HashMap<Setting, i32>>,
    mut writer: MessageWriter<common_bevy::message::Do>,
) {
    let Ok(heard) = heard.0.lock() else { return };
    for rows in heard.try_iter() {
        let mut changed = false;
        for setting in Setting::ALL {
            let value = setting.held(rows.iter().find(|row| row.0 == setting.id()).map_or(setting.default(), |row| row.1));
            if setting.hot() {
                if settings.get(setting) != value {
                    settings.set(setting, value);
                    info!("store: {} is {value}", setting.name());
                    changed = true;
                }
            } else if waiting.insert(setting, value) != Some(value) && value != settings.get(setting) {
                info!("store: {} is {} and will be {value} once the server starts again", setting.name(), settings.get(setting));
            }
        }
        if changed {
            writer.write(common_bevy::message::Do { event: common_bevy::message::Event::Settings { settings: *settings } });
        }
    }
}

fn roll_back(mut reader: MessageReader<common_bevy::message::Try>, mut keeper: ResMut<Keeper>, mut exit: MessageWriter<AppExit>) {
    for message in reader.read() {
        let common_bevy::message::Try { event: common_bevy::message::Event::Rollback { game, ms } } = message else { continue };
        if keeper.rolled {
            continue;
        }
        if let Some((_, task)) = keeper.write.take() {
            let _ = block_on(task);
        }
        match block_on(keeper.store.roll_back(*game, *ms as i64, keeper.when.0 as i64)) {
            Ok(undone) => {
                warn!("store: the world is rolled back {} ms of {} time, {undone} changes undone; the server stops, start it again", ms, if *game { "game" } else { "real" });
                keeper.rolled = true;
                exit.write(AppExit::Success);
            }
            Err(e) => error!("store: the rollback failed, and nothing is changed: {e}"),
        }
    }
}

fn note(
    mut reader: MessageReader<TileChanged>,
    mut moved: MessageReader<Moved>,
    bags: Query<&Character, Changed<Inventory>>,
    mut dens: ResMut<Dens>,
    mut camps: ResMut<Camps>,
    mut hubs: ResMut<Hubs>,
    mut charts: ResMut<Charts>,
    mut depots: ResMut<Depots>,
    mut known: ResMut<Characters>,
    runtime: Res<combat::RunTime>,
    anchor: Res<crate::systems::world::Anchor>,
    time: Res<Time>,
    mut keeper: ResMut<Keeper>,
) {
    let keeper = &mut *keeper;
    keeper.when = runtime.when(&time);
    if charts.numbers_left() == 0 {
        number(&keeper.store, &mut charts);
    }
    let (charted, spent) = charts.take_changed();
    keeper.charted.retain(|(map, _)| !spent.contains(map));
    keeper.charted.extend(charted);
    keeper.spent.extend(spent);
    keeper.changed_depots.extend(depots.take_changed());
    let calendar = ((runtime.calendar.wall_at_zero as i128 - anchor.0 as i128) as i64, runtime.calendar.held.map(|held| held as i64));
    if calendar != keeper.calendar {
        keeper.calendar = calendar;
        keeper.changed_calendar = true;
    }
    let before = keeper.dirty.len() + keeper.changed.len() + keeper.moved.len();
    keeper.dirty.extend(reader.read().map(|changed| (changed.q, changed.r)));
    keeper.changed.extend(bags.iter().map(|character| character.0));
    keeper.moved.extend(moved.read().copied());
    keeper.due |= keeper.dirty.len() + keeper.changed.len() + keeper.moved.len() > before;
    // A character that left and a den that changed are written before
    // long, though no item moved
    keeper.changed.extend(known.take_left());
    keeper.changed_dens.extend(dens.take_changed());
    keeper.changed_camps.extend(camps.take_changed());
    keeper.changed_hubs |= hubs.take_changed();
}

/// Sends what the game asked after to the store.
fn ask(mut changes: ResMut<WorldChanges>, mut known: ResMut<Characters>, mut keeper: ResMut<Keeper>) {
    let wanted = changes.take_wanted();
    if !wanted.is_empty() {
        keeper.recall(wanted);
    }
    for area in changes.take_sought() {
        keeper.read(area);
    }
    for identity in known.take_wanted() {
        if !keeper.asked(identity) {
            keeper.enter(identity);
        }
    }
}

/// Lays what the store answered: a recalled chunk's tiles, piles and dens,
/// then the chunk as held; a read box's tiles where no held chunk has newer; a
/// player's character, its account then held.
#[allow(clippy::too_many_arguments)]
fn receive(
    mut changes: ResMut<WorldChanges>,
    mut piles: ResMut<Piles>,
    mut dens: ResMut<Dens>,
    mut camps: ResMut<Camps>,
    mut constructs: ResMut<Constructs>,
    mut known: ResMut<Characters>,
    registry: Res<EventRegistry>,
    mut keeper: ResMut<Keeper>,
) {
    let generated = |q, r| registry.cover_at(q, r);
    for (chunks, mut task) in std::mem::take(&mut keeper.recalls) {
        match block_on(poll_once(&mut task)) {
            None => keeper.recalls.push((chunks, task)),
            Some(Ok(recalled)) => {
                debug!("store: recalled {} chunks, {} changed tiles, {} piles", chunks.len(), recalled.tiles.len(), recalled.piles.len());
                changes.recall(recalled.tiles, generated);
                for (key, held) in recalled.piles {
                    let (lots, mut unplayable) = stored::lots(held);
                    let mut lots = lots.into_iter();
                    if let Some(lot) = lots.next() {
                        piles.recall(key, lot);
                    }
                    // A slot holds one stack or piece: any more kept under it stay kept
                    unplayable.extend(lots.map(stored::lot));
                    if !unplayable.is_empty() {
                        error!("store: the pile at {key:?} holds {} things this build cannot lay there; they stay kept", unplayable.len());
                        keeper.lying.insert(key, unplayable);
                    }
                }
                for (site, state) in recalled.dens {
                    if let Some(den) = den::recalled(state, keeper.when) {
                        dens.recall(site, den);
                    }
                }
                for (hearth, state) in recalled.camps {
                    match camp::recalled(state) {
                        Some((camp, dormant)) => {
                            camps.recall(hearth, camp);
                            if !dormant.is_empty() {
                                error!("store: the camp at {hearth:?} holds {} things this build cannot play; they stay kept", dormant.len());
                                keeper.camped.insert(hearth, dormant);
                            }
                        }
                        None => error!("store: the camp at {hearth:?} cannot be read; its hearth has none"),
                    }
                }
                for (tile, row) in recalled.constructs {
                    match hub::rebuilt(row) {
                        Some(built) => constructs.recall(tile, built),
                        None => error!("store: the construct on {tile:?} is of a builder this build cannot name; it is no one's"),
                    }
                }
                for (chunk, epoch) in recalled.epochs {
                    keeper.epochs.insert(chunk, epoch);
                    changes.recalled(chunk);
                }
            }
            Some(Err(e)) => {
                warn!("store: recalling {} chunks failed, asking again: {e}", chunks.len());
                keeper.failed.0.extend(chunks);
            }
        }
    }
    for (area, mut task) in std::mem::take(&mut keeper.reads) {
        match block_on(poll_once(&mut task)) {
            None => keeper.reads.push((area, task)),
            Some(Ok(kept)) => changes.recall(kept, generated),
            Some(Err(e)) => {
                warn!("store: reading {area:?} failed, asking again: {e}");
                keeper.failed.1.push(area);
            }
        }
    }
    for (identity, mut task) in std::mem::take(&mut keeper.entering) {
        match block_on(poll_once(&mut task)) {
            None => keeper.entering.push((identity, task)),
            Some(Ok(Entered { account, epoch, character, state })) => {
                let kept = if state.is_null() {
                    None
                } else {
                    match character::recalled(state) {
                        Ok((kept, dormant)) => {
                            if dormant != Dormant::default() {
                                error!("store: character {character} of account {account} holds things this build cannot play; they stay kept: {dormant:?}");
                            }
                            keeper.dormant.insert(identity, dormant);
                            Some(kept)
                        }
                        Err(e) => {
                            error!("store: character {character} of account {account} cannot be read, and stays out of the world as it is: {e}");
                            keeper.unreadable.insert(identity);
                            continue;
                        }
                    }
                };
                debug!("store: recalled account {account}, character {character}");
                keeper.accounts.insert(identity, Held { account, epoch, character });
                if let Some(kept) = &kept {
                    let state = character::kept(kept, &keeper.dormant[&identity]);
                    keeper.written.insert(identity, state);
                }
                known.recall(identity, kept);
            }
            Some(Err(e)) => {
                warn!("store: recalling a player failed, asking again: {e}");
                keeper.failed.2.push(identity);
            }
        }
    }
    if let Some((carried, mut task)) = keeper.write.take() {
        match block_on(poll_once(&mut task)) {
            None => keeper.write = Some((carried, task)),
            Some(result) => keeper.wrote(carried, result, &mut known),
        }
    }
}

/// Writes what players changed: at once where an item moved, and every
/// [`FLUSH`] otherwise, when it also asks again after what failed. A
/// write still in flight holds the next one back.
#[allow(clippy::too_many_arguments)]
fn flush(
    time: Res<Time<Real>>,
    world: World,
    playing: Query<Playing>,
    known: Res<Characters>,
    mut keeper: ResMut<Keeper>,
) {
    let now = Moment::ZERO + time.elapsed();
    let interval = now >= keeper.next_flush;
    if interval {
        keeper.next_flush = now + FLUSH;
        let (chunks, areas, players) = std::mem::take(&mut keeper.failed);
        if !chunks.is_empty() {
            keeper.recall(chunks);
        }
        for area in areas {
            keeper.read(area);
        }
        for identity in players {
            keeper.enter(identity);
        }
    }
    if keeper.rolled || keeper.write.is_some() || !(interval || keeper.due) {
        return;
    }
    let (carried, batch) = keeper.batch(&world, &playing, &known, interval);
    if carried.is_empty() {
        return;
    }
    let store = keeper.store.clone();
    keeper.write = Some((carried, IoTaskPool::get().spawn(async move { store.write(batch).await })));
}

/// As the server stops, waits out the write in flight and writes the rest,
/// every character in the world with it.
fn flush_at_exit(
    mut exit: MessageReader<AppExit>,
    mut taking: ParamSet<(World, (ResMut<Dens>, ResMut<Hubs>, ResMut<Camps>, ResMut<Charts>, ResMut<Depots>))>,
    playing: Query<Playing>,
    mut known: ResMut<Characters>,
    mut keeper: ResMut<Keeper>,
) {
    if exit.read().count() == 0 || keeper.rolled {
        return;
    }
    let began = std::time::Instant::now();
    let (mut dens, mut hubs, mut camps, mut charts, mut depots) = taking.p1();
    keeper.changed_depots.extend(depots.take_changed());
    keeper.changed_dens.extend(dens.take_changed());
    keeper.changed_camps.extend(camps.take_changed());
    keeper.changed_hubs |= hubs.take_changed();
    let (charted, spent) = charts.take_changed();
    keeper.charted.extend(charted);
    keeper.spent.extend(spent);
    if let Some((carried, task)) = keeper.write.take() {
        let result = block_on(task);
        keeper.wrote(carried, result, &mut known);
    }
    let waited = began.elapsed();
    keeper.changed.extend(known.take_left());
    keeper.due = true;
    let (carried, batch) = keeper.batch(&taking.p0(), &playing, &known, true);
    let gathered = began.elapsed() - waited;
    if carried.is_empty() {
        info!("store: nothing left to write as the server stopped: {waited:.1?} on the write in flight, {gathered:.1?} gathering");
        return;
    }
    let result = block_on(keeper.store.write(batch));
    let wrote = began.elapsed() - waited - gathered;
    match result {
        Ok(refused) if refused.is_empty() => info!(
            "store: wrote {} changed tiles and {} characters as the server stopped: {waited:.1?} on the write in flight, {gathered:.1?} gathering, {wrote:.1?} writing",
            carried.tiles.len(),
            carried.characters.len(),
        ),
        Ok(refused) => error!("store: the last write was refused, and what it carried is lost: {refused:?}"),
        Err(e) => error!("store: the last write failed, and what it carried is lost: {e}"),
    }
}
