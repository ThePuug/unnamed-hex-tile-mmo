//! Keeps the changed world past the server, in the PostgreSQL at
//! `DATABASE_URL`. The rest of the server never learns how: kept changes
//! are recalled into [`WorldChanges`] and [`Piles`] as they are asked
//! after, and every tile a player changes, announced as [`TileChanged`], is
//! written behind play. Without `DATABASE_URL` nothing is kept, and the
//! changed world lasts as long as the server.
//!
//! One server writes a chunk at a time. Recalling a chunk to build claims
//! it, raising its ownership number, and a write under a number since
//! raised is refused in the transaction that would have made it. A box of
//! tiles read for a summary claims nothing.
//!
//! No system waits on the store: every query is a task on the IO pool,
//! polled each frame, so a slow store delays recalls and writes and never
//! the simulation.

mod store;

use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use bevy::{
    prelude::*,
    tasks::{block_on, futures_lite::future::poll_once, IoTaskPool, Task},
};
use common_bevy::chunk::{loc_to_chunk, ChunkId};
use qrz::Qrz;

use crate::{
    resources::event_registry::EventRegistry,
    systems::gathering::{Piles, TileChanged, WorldChanges},
};
use store::{Batch, Recalled, Store};

/// How often what players changed is written. A crash loses at most this
/// much of the ground.
const FLUSH: Duration = Duration::from_secs(5);

pub struct PersistencePlugin;

impl Plugin for PersistencePlugin {
    fn build(&self, app: &mut App) {
        let Ok(url) = std::env::var("DATABASE_URL") else {
            info!("no DATABASE_URL: the changed world lasts as long as the server");
            return;
        };
        let store = block_on(Store::open(&url)).unwrap_or_else(|e| panic!("DATABASE_URL is set but its store will not open: {e}"));
        info!("keeping the changed world in the store at DATABASE_URL");
        app.insert_resource(Keeper::new(store))
            .add_systems(Startup, keep)
            .add_systems(Update, (note, ask, receive, flush).chain())
            .add_systems(Last, flush_at_exit);
    }
}

/// The store and what this server knows of it.
#[derive(Resource)]
struct Keeper {
    store: Store,
    /// The ownership number each chunk this server holds was claimed with.
    epochs: HashMap<ChunkId, i64>,
    /// Tiles players changed since the last write.
    dirty: HashSet<(i32, i32)>,
    recalls: Vec<(Vec<ChunkId>, Task<Result<Recalled, sqlx::Error>>)>,
    reads: Vec<(((i32, i32), (i32, i32)), Task<Result<Vec<(i32, i32, common::Cover)>, sqlx::Error>>)>,
    /// The write in flight, and the tiles it carries. One at a time, so a
    /// tile's writes land in the order they were made.
    write: Option<(Vec<(i32, i32)>, Task<Result<Vec<ChunkId>, sqlx::Error>>)>,
    /// Chunks and boxes whose query failed, asked again at the next flush.
    failed: (Vec<ChunkId>, Vec<((i32, i32), (i32, i32))>),
    next_flush: Duration,
}

impl Keeper {
    fn new(store: Store) -> Self {
        Self {
            store,
            epochs: HashMap::new(),
            dirty: HashSet::new(),
            recalls: Vec::new(),
            reads: Vec::new(),
            write: None,
            failed: (Vec::new(), Vec::new()),
            next_flush: FLUSH,
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

    /// Everything players changed since the last write, under the number
    /// each tile's chunk was claimed with, and the tiles it carries.
    fn batch(&mut self, changes: &WorldChanges, piles: &Piles) -> (Vec<(i32, i32)>, Batch) {
        let mut batch = Batch::default();
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
                if let Some(stacks) = piles.stacks((q, r, slot)) {
                    batch.piles.push((chunk, (q, r, slot), stacks.to_vec()));
                }
            }
        }
        (tiles, batch)
    }
}

/// From the first frame, a chunk's changes are on the map only once
/// recalled.
fn keep(mut changes: ResMut<WorldChanges>) {
    changes.keep();
}

fn note(mut reader: MessageReader<TileChanged>, mut keeper: ResMut<Keeper>) {
    keeper.dirty.extend(reader.read().map(|changed| (changed.q, changed.r)));
}

/// Sends what the game asked after to the store.
fn ask(mut changes: ResMut<WorldChanges>, mut keeper: ResMut<Keeper>) {
    let wanted = changes.take_wanted();
    if !wanted.is_empty() {
        keeper.recall(wanted);
    }
    for area in changes.take_sought() {
        keeper.read(area);
    }
}

/// Lays what the store answered: a recalled chunk's tiles and piles, then
/// the chunk as held, and a read box's tiles where no held chunk has newer.
fn receive(mut changes: ResMut<WorldChanges>, mut piles: ResMut<Piles>, registry: Res<EventRegistry>, mut keeper: ResMut<Keeper>) {
    let generated = |q, r| registry.cover_at(q, r);
    for (chunks, mut task) in std::mem::take(&mut keeper.recalls) {
        match block_on(poll_once(&mut task)) {
            None => keeper.recalls.push((chunks, task)),
            Some(Ok(recalled)) => {
                debug!("store: recalled {} chunks, {} changed tiles, {} piles", chunks.len(), recalled.tiles.len(), recalled.piles.len());
                changes.recall(recalled.tiles, generated);
                for (key, stacks) in recalled.piles {
                    piles.recall(key, stacks);
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
    if let Some((tiles, mut task)) = keeper.write.take() {
        match block_on(poll_once(&mut task)) {
            None => keeper.write = Some((tiles, task)),
            Some(Ok(refused)) => {
                debug!("store: wrote {} changed tiles", tiles.len());
                refuse(&mut keeper, refused);
            }
            Some(Err(e)) => {
                warn!("store: writing {} tiles failed, writing them again: {e}", tiles.len());
                keeper.dirty.extend(tiles);
            }
        }
    }
}

/// Another server claimed `refused` since this one did: what this one
/// changed there is dropped, and it writes there no more.
fn refuse(keeper: &mut Keeper, refused: Vec<ChunkId>) {
    for chunk in refused {
        error!("store: chunk {chunk:?} was claimed by another server; changes this one made there are lost");
        keeper.epochs.remove(&chunk);
    }
}

/// Every [`FLUSH`], writes what players changed and asks again after what
/// failed. A write still in flight holds the next one back.
fn flush(time: Res<Time<Real>>, changes: Res<WorldChanges>, piles: Res<Piles>, mut keeper: ResMut<Keeper>) {
    if time.elapsed() < keeper.next_flush {
        return;
    }
    keeper.next_flush = time.elapsed() + FLUSH;
    let (chunks, areas) = std::mem::take(&mut keeper.failed);
    if !chunks.is_empty() {
        keeper.recall(chunks);
    }
    for area in areas {
        keeper.read(area);
    }
    if keeper.write.is_some() || keeper.dirty.is_empty() {
        return;
    }
    let (tiles, batch) = keeper.batch(&changes, &piles);
    let store = keeper.store.clone();
    keeper.write = Some((tiles, IoTaskPool::get().spawn(async move { store.write(batch).await })));
}

/// As the server stops, waits out the write in flight and writes the rest.
fn flush_at_exit(mut exit: MessageReader<AppExit>, changes: Res<WorldChanges>, piles: Res<Piles>, mut keeper: ResMut<Keeper>) {
    if exit.read().count() == 0 {
        return;
    }
    if let Some((tiles, task)) = keeper.write.take() {
        match block_on(task) {
            Ok(refused) => refuse(&mut keeper, refused),
            Err(_) => keeper.dirty.extend(tiles),
        }
    }
    if keeper.dirty.is_empty() {
        return;
    }
    let (tiles, batch) = keeper.batch(&changes, &piles);
    match block_on(keeper.store.write(batch)) {
        Ok(refused) => {
            refuse(&mut keeper, refused);
            info!("store: wrote {} changed tiles as the server stopped", tiles.len());
        }
        Err(e) => error!("store: writing {} changed tiles as the server stopped failed; they are lost: {e}", tiles.len()),
    }
}
