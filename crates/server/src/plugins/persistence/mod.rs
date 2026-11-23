//! Keeps the changed world and its characters past the server, in the
//! PostgreSQL at `DATABASE_URL`, or the local one ([`LOCAL`]) where that is
//! unset. The rest of the server never learns how: kept changes are
//! recalled into [`WorldChanges`] and [`Piles`], and kept characters into
//! [`Characters`], as they are asked after, and what players change is
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
//! every change to a bag.
//!
//! Everything kept is known by a number given it once ([`stored`]), so a
//! build that renames a thing still reads it. What a build has no number
//! for stays in the store: [`Keeper`] holds it beside the character or the
//! pile it was kept with, and writes it back with them.
//!
//! No system waits on the store: every query is a task on the IO pool,
//! polled each frame, so a slow store delays recalls and writes and never
//! the simulation.

mod character;
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
    components::{
        equipment::{Equipment, Inventory},
        heading::Heading,
        loadout::Loadout,
        ActorAttributes, Loc,
    },
    moment::Moment,
};
use qrz::Qrz;

use crate::{
    resources::event_registry::EventRegistry,
    systems::{
        characters::{Character, Characters},
        gathering::{Piles, TileChanged, WorldChanges},
    },
};
use character::Dormant;
use store::{Batch, Entered, PileKey, Recalled, Refused, Store};

/// How often what players changed is written where no item moved. A crash
/// loses at most this much of it.
const FLUSH: Duration = Duration::from_secs(5);

/// The store a server keeps to where `DATABASE_URL` names none: a role and
/// a database of its own on the machine's PostgreSQL, made once by
/// `CREATE ROLE mmo LOGIN PASSWORD 'mmo' CREATEDB` and
/// `CREATE DATABASE mmo OWNER mmo`.
const LOCAL: &str = "postgres://mmo:mmo@localhost:5432/mmo";

/// Where the store is.
fn url() -> String {
    std::env::var("DATABASE_URL").unwrap_or_else(|_| LOCAL.to_owned())
}

pub struct PersistencePlugin;

impl Plugin for PersistencePlugin {
    fn build(&self, app: &mut App) {
        let url = url();
        let store = block_on(Store::open(&url)).unwrap_or_else(|e| {
            panic!("the store will not open: {e}\nThe server keeps to the PostgreSQL at DATABASE_URL, or to {LOCAL} where that is unset; AGENTS.md says how that one is made.")
        });
        info!("keeping the changed world and its characters in the store");
        app.insert_resource(Keeper::new(store))
            .add_systems(Startup, keep)
            .add_systems(Update, (note, ask, receive, flush).chain())
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
    /// Each character, and its state as written.
    characters: Vec<(Identity, String)>,
    economy: bool,
}

impl Carried {
    fn is_empty(&self) -> bool {
        self.tiles.is_empty() && self.characters.is_empty()
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
    /// Characters to write whenever the next write goes: one whose bag
    /// changed, one that left the world.
    changed: HashSet<Identity>,
    /// Each character's state as the store holds it.
    written: HashMap<Identity, String>,
    /// What each character holds that this build cannot play.
    dormant: HashMap<Identity, Dormant>,
    /// What lies in each pile's place that this build cannot play.
    lying: HashMap<PileKey, Vec<serde_json::Value>>,
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
type Playing = (&'static Character, &'static Equipment, &'static Inventory, &'static ActorAttributes, &'static Loadout, &'static Loc, &'static Heading);

impl Keeper {
    fn new(store: Store) -> Self {
        Self {
            store,
            epochs: HashMap::new(),
            accounts: HashMap::new(),
            dirty: HashSet::new(),
            changed: HashSet::new(),
            written: HashMap::new(),
            dormant: HashMap::new(),
            lying: HashMap::new(),
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
        self.entering.push((identity, IoTaskPool::get().spawn(async move { store.enter(identity.provider(), &identity.name()).await })));
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
    fn batch(&mut self, changes: &WorldChanges, piles: &Piles, playing: &Query<Playing>, known: &Characters, every: bool) -> (Carried, Batch) {
        let mut batch = Batch { economy: std::mem::take(&mut self.due), ..default() };
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
            for slot in 0..common::TILE_SLOTS as usize {
                let playable = piles.held((q, r, slot)).map(stored::stack).into_iter();
                let held: Vec<_> = playable.chain(self.lying.get(&(q, r, slot)).into_iter().flatten().cloned()).collect();
                if !held.is_empty() {
                    batch.piles.push((chunk, (q, r, slot), held));
                }
            }
        }

        let mut changed = std::mem::take(&mut self.changed);
        let mut states: Vec<(Identity, String)> = Vec::new();
        let none = Dormant::default();
        for (&Character(identity), equipment, inventory, attrs, loadout, loc, heading) in playing {
            if changed.remove(&identity) || every {
                let dormant = self.dormant.get(&identity).unwrap_or(&none);
                states.push((identity, character::state(equipment, inventory, attrs, loadout, **loc, *heading, dormant)));
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
        (Carried { tiles, characters, economy: batch.economy }, batch)
    }

    /// The write carrying `carried` landed, was refused or failed.
    fn wrote(&mut self, carried: Carried, result: Result<Refused, sqlx::Error>, known: &mut Characters) {
        match result {
            Ok(refused) if refused.is_empty() => {
                debug!("store: wrote {} changed tiles and {} characters", carried.tiles.len(), carried.characters.len());
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
        self.changed.extend(carried.characters.into_iter().map(|(identity, _)| identity));
        self.due |= carried.economy;
    }
}

/// From the first frame, a chunk's changes are on the map only once
/// recalled.
fn keep(mut changes: ResMut<WorldChanges>) {
    changes.keep();
}

fn note(mut reader: MessageReader<TileChanged>, bags: Query<&Character, Changed<Inventory>>, mut known: ResMut<Characters>, mut keeper: ResMut<Keeper>) {
    let keeper = &mut *keeper;
    let before = keeper.dirty.len() + keeper.changed.len();
    keeper.dirty.extend(reader.read().map(|changed| (changed.q, changed.r)));
    keeper.changed.extend(bags.iter().map(|character| character.0));
    keeper.due |= keeper.dirty.len() + keeper.changed.len() > before;
    // A character that left is written before long, though no item moved
    keeper.changed.extend(known.take_left());
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

/// Lays what the store answered: a recalled chunk's tiles and piles, then
/// the chunk as held; a read box's tiles where no held chunk has newer; a
/// player's character, its account then held.
fn receive(mut changes: ResMut<WorldChanges>, mut piles: ResMut<Piles>, mut known: ResMut<Characters>, registry: Res<EventRegistry>, mut keeper: ResMut<Keeper>) {
    let generated = |q, r| registry.cover_at(q, r);
    for (chunks, mut task) in std::mem::take(&mut keeper.recalls) {
        match block_on(poll_once(&mut task)) {
            None => keeper.recalls.push((chunks, task)),
            Some(Ok(recalled)) => {
                debug!("store: recalled {} chunks, {} changed tiles, {} piles", chunks.len(), recalled.tiles.len(), recalled.piles.len());
                changes.recall(recalled.tiles, generated);
                for (key, held) in recalled.piles {
                    let (stacks, mut unplayable) = stored::stacks(held);
                    let mut stacks = stacks.into_iter();
                    if let Some(stack) = stacks.next() {
                        piles.recall(key, stack);
                    }
                    // A slot holds one stack: any more kept under it stay kept
                    unplayable.extend(stacks.map(stored::stack));
                    if !unplayable.is_empty() {
                        error!("store: the pile at {key:?} holds {} things this build cannot lay there; they stay kept", unplayable.len());
                        keeper.lying.insert(key, unplayable);
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
fn flush(time: Res<Time<Real>>, changes: Res<WorldChanges>, piles: Res<Piles>, playing: Query<Playing>, known: Res<Characters>, mut keeper: ResMut<Keeper>) {
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
    if keeper.write.is_some() || !(interval || keeper.due) {
        return;
    }
    let (carried, batch) = keeper.batch(&changes, &piles, &playing, &known, interval);
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
    changes: Res<WorldChanges>,
    piles: Res<Piles>,
    playing: Query<Playing>,
    mut known: ResMut<Characters>,
    mut keeper: ResMut<Keeper>,
) {
    if exit.read().count() == 0 {
        return;
    }
    if let Some((carried, task)) = keeper.write.take() {
        let result = block_on(task);
        keeper.wrote(carried, result, &mut known);
    }
    keeper.changed.extend(known.take_left());
    keeper.due = true;
    let (carried, batch) = keeper.batch(&changes, &piles, &playing, &known, true);
    if carried.is_empty() {
        return;
    }
    match block_on(keeper.store.write(batch)) {
        Ok(refused) if refused.is_empty() => info!("store: wrote {} changed tiles and {} characters as the server stopped", carried.tiles.len(), carried.characters.len()),
        Ok(refused) => error!("store: the last write was refused, and what it carried is lost: {refused:?}"),
        Err(e) => error!("store: the last write failed, and what it carried is lost: {e}"),
    }
}
