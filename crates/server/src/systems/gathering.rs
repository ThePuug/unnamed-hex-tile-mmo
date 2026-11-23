//! Gathering: a player works at what it gathers for a time, and the yield
//! then lies at once as a pile. What lies on the ground lies one stack to
//! a slot; a loot window opens on a tile and shows every pile there, to
//! one player at a time, who takes from them into its bag, and closing it
//! leaves the rest to anyone. Every tile changed keeps the change, and
//! every client holding the tile learns it.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use bevy::{ecs::system::SystemParam, prelude::*};
use common::summary::{SummarySource, TileSample};
use common_bevy::{
    chunk::{loc_to_chunk, ChunkId},
    components::{
        entity_type::{decorator::Decorator, EntityType},
        equipment::{Equipment, Inventory},
        heading::Heading,
        position::Position,
        Loc,
    },
    message::{Do, Event, Outcome, Try},
    moment::Moment,
    resources::map::Map,
    systems::movement::{stand_point, stepped},
};
use qrz::Qrz;

use crate::systems::actor::VisibleChunkCache;

/// Every tile players have changed, as its cover now stands. A tile is its
/// generated cover with its change laid over: whatever builds a tile for
/// the map or the wire takes it through [`WorldChanges::laid_over`], and a
/// summary reads its samples through [`WorldChanges::over`].
///
/// Where changes outlive the server, a chunk's arrive when it is recalled:
/// a chunk is built only once [`WorldChanges::holds`] it, and a summary
/// [`WorldChanges::seek`]s the changes under it, which arrive fresh.
#[derive(Resource, Default)]
pub struct WorldChanges {
    /// Shared with the summary tasks in flight, each reading the changes
    /// as they stood when it set out; a change copies it while one holds it.
    tiles: Arc<HashMap<(i32, i32), common::Cover>>,
    /// Changes made since the summaries were last revised.
    fresh: Vec<Change>,
    /// The chunks whose kept changes are laid here, where changes are kept;
    /// none where every change was made in this run, and every chunk's are.
    recalled: Option<HashSet<ChunkId>>,
    /// Chunks asked after and not yet recalled.
    wanted: HashSet<ChunkId>,
    /// Chunks first asked after since the wanted were last taken.
    newly_wanted: Vec<ChunkId>,
    /// Boxes of tiles asked after, as `(q, r)` corners, not yet read.
    sought: Vec<((i32, i32), (i32, i32))>,
}

/// A tile players changed: its cover before the change and after it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Change {
    pub q: i32,
    pub r: i32,
    pub before: common::Cover,
    pub after: common::Cover,
}

impl WorldChanges {
    /// The tile `typ` at `qrz` as players have left it.
    pub fn laid_over(&self, qrz: Qrz, typ: EntityType) -> EntityType {
        match (typ, self.tiles.get(&(qrz.q, qrz.r))) {
            (EntityType::Decorator(d), Some(&cover)) => EntityType::Decorator(Decorator { cover, ..d }),
            _ => typ,
        }
    }

    /// `source` with every change laid over its tiles, as the changes
    /// stand now.
    pub fn over<S: SummarySource>(&self, source: S) -> LaidOver<S> {
        LaidOver { source, tiles: self.tiles.clone() }
    }

    /// The changes made since the last call, in the order they were made.
    pub fn take_fresh(&mut self) -> Vec<Change> {
        std::mem::take(&mut self.fresh)
    }

    /// The cover players left on tile `(q, r)`, where they changed it.
    pub fn get(&self, q: i32, r: i32) -> Option<common::Cover> {
        self.tiles.get(&(q, r)).copied()
    }

    /// Whether every change kept on `chunk` is laid here. One that is not
    /// is asked after, and held once recalled.
    pub fn holds(&mut self, chunk: ChunkId) -> bool {
        let held = self.recalled.as_ref().is_none_or(|recalled| recalled.contains(&chunk));
        if !held && self.wanted.insert(chunk) {
            self.newly_wanted.push(chunk);
        }
        held
    }

    /// Asks after every change kept on a tile with `min.0 <= q <= max.0`
    /// and `min.1 <= r <= max.1`, where changes are kept. Each arrives as a
    /// fresh change, but on a chunk held here, whose changes are newer.
    pub fn seek(&mut self, min: (i32, i32), max: (i32, i32)) {
        if self.recalled.is_some() {
            self.sought.push((min, max));
        }
    }

    /// From now on a chunk's changes are here only once recalled: changes
    /// outlive the server. Changes already made stay.
    pub fn keep(&mut self) {
        self.recalled.get_or_insert_default();
    }

    /// The chunks first asked after since the last call. Each is asked
    /// after once, however often [`WorldChanges::holds`] is asked of it.
    pub fn take_wanted(&mut self) -> Vec<ChunkId> {
        std::mem::take(&mut self.newly_wanted)
    }

    /// The boxes of tiles asked after since the last call.
    pub fn take_sought(&mut self) -> Vec<((i32, i32), (i32, i32))> {
        std::mem::take(&mut self.sought)
    }

    /// Lays each kept cover in `kept` over its tile, as a fresh change from
    /// the tile as it stands here, `generated` where it is unchanged; but
    /// none on a chunk held here, whose changes are newer than any kept.
    pub fn recall(&mut self, kept: impl IntoIterator<Item = (i32, i32, common::Cover)>, generated: impl Fn(i32, i32) -> common::Cover) {
        for (q, r, cover) in kept {
            let held = self.recalled.as_ref().is_some_and(|recalled| recalled.contains(&loc_to_chunk(Qrz { q, r, z: 0 })));
            let before = self.get(q, r).unwrap_or_else(|| generated(q, r));
            if !held && before != cover {
                self.set(q, r, before, cover);
            }
        }
    }

    /// Notes every change kept on `chunk` as laid here.
    pub fn recalled(&mut self, chunk: ChunkId) {
        self.wanted.remove(&chunk);
        self.recalled.get_or_insert_default().insert(chunk);
    }

    /// Every tree felled and every boulder mined within `radius` tiles of
    /// `(q, r)`, by the rule a gather follows, with nothing left lying: a
    /// clearing laid down before the world is served, so what the far
    /// ground makes of a change can be seen without making it by hand.
    /// Where it `fades`, each one falls by a chance that runs from certain
    /// at the centre to none at the radius, drawn from where it stands, so
    /// the clearing thins out into the wood the same way every time. Every
    /// tile it clears is a fresh change, which the summaries take in before
    /// the first client is sent one.
    pub fn clearing(cover_at: impl Fn(i32, i32) -> common::Cover, (q, r): (i32, i32), radius: i32, fades: bool) -> Self {
        let mut changes = Self::default();
        for dq in -radius..=radius {
            for dr in (-radius).max(-dq - radius)..=radius.min(-dq + radius) {
                let generated = cover_at(q + dq, r + dr);
                let distance = Qrz { q: q + dq, r: r + dr, z: 0 }.flat_distance(&Qrz { q, r, z: 0 });
                let chance = if fades { 1.0 - distance as f64 / (radius + 1) as f64 } else { 1.0 };
                let cleared = (0..common::TILE_SLOTS as usize)
                    .filter(|&k| generated.content(k) != common::Content::Spanned && roll(q + dq, r + dr, k) < chance)
                    .fold(generated, |cover, k| common::gathering::harvest(cover, k).map_or(cover, |h| h.cover));
                if cleared != generated {
                    changes.set(q + dq, r + dr, generated, cleared);
                }
            }
        }
        changes
    }

    fn set(&mut self, q: i32, r: i32, before: common::Cover, after: common::Cover) {
        Arc::make_mut(&mut self.tiles).insert((q, r), after);
        self.fresh.push(Change { q, r, before, after });
    }
}

/// A number in `[0, 1)` fixed by slot `k` of tile `(q, r)`: a splitmix of
/// the three.
pub(crate) fn roll(q: i32, r: i32, k: usize) -> f64 {
    let mut x = (q as u32 as u64) << 32 | r as u32 as u64;
    x = x.wrapping_add((k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    (x >> 11) as f64 / (1u64 << 53) as f64
}

/// A summary source with the players' changes laid over its tiles.
pub struct LaidOver<S> {
    source: S,
    tiles: Arc<HashMap<(i32, i32), common::Cover>>,
}

impl<S: SummarySource> SummarySource for LaidOver<S> {
    fn sample(&self, q: i32, r: i32) -> Option<TileSample> {
        let mut sample = self.source.sample(q, r)?;
        if let Some(&cover) = self.tiles.get(&(q, r)) {
            sample.cover = cover;
        }
        Some(sample)
    }

    fn cover(&self, w: i64, q: i32, r: i32) -> Option<common::Cover> {
        self.tiles.get(&(q, r)).copied().or_else(|| self.source.cover(w, q, r))
    }

    fn cell(&self, r: u32, sq: i32, sr: i32) -> Option<(i32, u16)> {
        self.source.cell(r, sq, sr)
    }
}

/// Everything lying on the ground: one stack to a slot of a tile, each a
/// pile of its own. A pile's content in the cover says where it lies and
/// what it looks like; this says what is in it.
#[derive(Resource, Default)]
pub struct Piles(HashMap<(i32, i32, usize), common::Stack>);

impl Piles {
    /// What the pile at `key` holds, where one lies.
    pub fn held(&self, key: (i32, i32, usize)) -> Option<common::Stack> {
        self.0.get(&key).copied()
    }

    /// Lays a kept pile holding `stack` at `key`, where none lies already.
    pub fn recall(&mut self, key: (i32, i32, usize), stack: common::Stack) {
        self.0.entry(key).or_insert(stack);
    }

    /// Every pile on tile `(q, r)`, its slot and what it holds, by slot:
    /// the entries of a loot window open on the tile, in their order.
    fn on(&self, q: i32, r: i32) -> Vec<(usize, common::Stack)> {
        (0..common::TILE_SLOTS as usize).filter_map(|slot| Some((slot, self.held((q, r, slot))?))).collect()
    }

    fn entries(&self, q: i32, r: i32) -> Vec<common::Stack> {
        self.on(q, r).into_iter().map(|(_, stack)| stack).collect()
    }
}

/// Players changed tile `(q, r)`: what stands on it, or what a pile on it
/// holds.
#[derive(Message, Clone, Copy, Debug)]
pub struct TileChanged {
    pub q: i32,
    pub r: i32,
}

/// The tile a player has its loot window open on, and where it stood and
/// faced as the window opened. The window shows every pile on the tile,
/// and while it is open no one else opens one there; it closes the moment
/// the player moves or turns.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Looting {
    pub q: i32,
    pub r: i32,
    pub from: Position,
    pub facing: Heading,
}

/// Every open loot window: its player and its tile.
type Windows<'w, 's> = Query<'w, 's, (Entity, &'static Looting)>;

/// The player whose window is open on tile `(q, r)`, where one is.
fn holder(windows: &Windows, (q, r): (i32, i32)) -> Option<Entity> {
    windows.iter().find(|(_, looting)| (looting.q, looting.r) == (q, r)).map(|(holder, _)| holder)
}

/// Whether tile `(q, r)` lies in the reach of a player on `here` facing
/// `heading`: its own tile or one of the three in its front half.
fn in_reach(here: Qrz, heading: Heading, q: i32, r: i32) -> bool {
    (q, r) == (here.q, here.r) || heading.front_dirs().iter().any(|&d| (q, r) == (here.q + d.q, here.r + d.r))
}

/// The ground as players change it: each change goes to the map, to the
/// changes every later tile is built with, and to every player holding the
/// tile's chunk. A change a player makes is announced as [`TileChanged`];
/// one the world lays, as a den's clearing, is not.
#[derive(SystemParam)]
pub struct Ground<'w, 's> {
    map: Res<'w, Map>,
    changes: ResMut<'w, WorldChanges>,
    holders: Query<'w, 's, (Entity, &'static VisibleChunkCache)>,
    changed: MessageWriter<'w, TileChanged>,
}

impl Ground<'_, '_> {
    /// What stands in tile `(q, r)`'s slots, where the map holds it.
    pub fn cover(&self, q: i32, r: i32) -> Option<common::Cover> {
        self.lies(q, r).map(|(_, cover)| cover)
    }

    /// Tile `(q, r)`'s height and what stands in its slots, where the map
    /// holds it.
    fn lies(&self, q: i32, r: i32) -> Option<(i32, common::Cover)> {
        match self.map.get_by_qr(q, r)? {
            (qrz, EntityType::Decorator(decorator)) => Some((qrz.z, decorator.cover)),
            _ => None,
        }
    }

    /// A player's change to tile `(q, r)`, which the map holds.
    fn set(&mut self, writer: &mut MessageWriter<Do>, q: i32, r: i32, cover: common::Cover) {
        self.put(writer, q, r, cover);
        self.changed.write(TileChanged { q, r });
    }

    /// A player's change to what a pile on tile `(q, r)` holds, which left
    /// the tile's cover as it was.
    fn piled(&mut self, q: i32, r: i32) {
        self.changed.write(TileChanged { q, r });
    }

    /// Lays `cover` over tile `(q, r)`, where the map holds it, for every
    /// client holding its chunk.
    pub fn put(&mut self, writer: &mut MessageWriter<Do>, q: i32, r: i32, cover: common::Cover) {
        let Some((qrz, EntityType::Decorator(decorator))) = self.map.get_by_qr(q, r) else { return };
        self.map.insert(qrz, EntityType::Decorator(Decorator { cover, ..decorator }));
        self.changes.set(q, r, decorator.cover, cover);
        let chunk = loc_to_chunk(qrz);
        for (holder, cache) in &self.holders {
            if cache.sent.contains(&chunk) {
                writer.write(Do { event: Event::CoverChanged { ent: holder, q, r, cover } });
            }
        }
    }
}

/// The work a player is at: gathering slot `slot` of tile `(q, r)`, done
/// at `until`, from where it stood and faced when it began. Moving from
/// there, turning from that facing or being struck breaks it off.
#[derive(Component, Clone, Copy, Debug)]
pub struct Working {
    pub q: i32,
    pub r: i32,
    pub slot: usize,
    pub until: Moment,
    pub from: Position,
    pub facing: Heading,
}

/// Opens `ent`'s loot window on tile `(q, r)`, with the player standing
/// `from` and facing `facing`; a window it had open elsewhere closes.
fn open(ent: Entity, (q, r): (i32, i32), from: Position, facing: Heading, piles: &Piles, commands: &mut Commands, writer: &mut MessageWriter<Do>) {
    commands.entity(ent).insert(Looting { q, r, from, facing });
    writer.write(Do { event: Event::Loot { ent, entries: Some(piles.entries(q, r)) } });
}

/// Tells everyone who sees `ent` what it is doing at a gather.
fn show(ent: Entity, activity: Option<common::gathering::Activity>, commands: &mut Commands) {
    commands.write_message(Do { event: Event::Activity { ent, activity } });
}

/// Stops `ent`'s work: it is seen stooped to its pile where its loot
/// window is open, doing nothing otherwise.
fn stop(ent: Entity, looting: bool, commands: &mut Commands) {
    commands.entity(ent).remove::<Working>();
    show(ent, looting.then_some(common::gathering::Activity::Pickup), commands);
}

/// Answers each gather a player asks for: the slot must lie in its reach.
/// A pile there opens the window on its tile at once, unless another
/// player has one open there. Something gatherable there sets the player
/// to work on it, unless it is at work already.
#[allow(clippy::too_many_arguments)]
pub fn try_gather(
    mut commands: Commands,
    mut reader: MessageReader<Try>,
    mut writer: MessageWriter<Do>,
    ground: Ground,
    piles: Res<Piles>,
    time: Res<Time>,
    nntree: Res<common_bevy::plugins::nntree::NNTree>,
    windows: Windows,
    mut players: Query<(&Loc, &mut Heading, &mut common_bevy::components::Turn, &mut Position, Has<Working>)>,
) {
    // Windows opened this frame, which no query sees until it ends
    let mut opened: Vec<(Entity, (i32, i32))> = Vec::new();
    for message in reader.read() {
        let Try { event: Event::Gather { ent, q, r, slot } } = message else { continue };
        let (ent, q, r, slot) = (*ent, *q, *r, *slot as usize);
        let Ok((loc, heading, _, position, working)) = players.get(ent) else {
            warn!("gather: {ent} is not a player");
            continue;
        };
        let (loc, heading, position) = (**loc, *heading, *position);
        if !in_reach(loc, heading, q, r) || slot >= common::TILE_SLOTS as usize {
            info!("gather: {ent} at {loc:?} asked out of reach for slot {slot} of ({q}, {r})");
            continue;
        }
        let Some(cover) = ground.cover(q, r) else {
            info!("gather: {ent} asked for ({q}, {r}), which the map does not hold");
            continue;
        };
        // Answered, the player turns to face what it gathers and steps to
        // where the work reaches it, where the way there is open.
        let (x, z) = common_bevy::geometry::slot_point(cover, (q, r), slot, (position.tile.q, position.tile.r));
        let facing = Heading::facing(position.offset.xz(), Vec2::new(x, z)).unwrap_or(heading);
        let activity = if cover.content(slot).is_pile() {
            Some(common::gathering::Activity::Pickup)
        } else {
            common::gathering::work(cover, slot).map(common::gathering::Activity::Work)
        };
        let position = activity
            .filter(|_| !working)
            .and_then(|activity| stand_point(position.tile, position.offset, facing, (q, r), slot, activity, &ground.map, &nntree))
            .map_or(position, |stand| stepped(position, stand, &ground.map));
        if cover.content(slot).is_pile() {
            if piles.held((q, r, slot)).is_none() {
                continue;
            }
            let other = holder(&windows, (q, r)).or_else(|| opened.iter().find(|(_, tile)| *tile == (q, r)).map(|(other, _)| *other));
            if let Some(other) = other.filter(|&other| other != ent) {
                info!("gather: {ent} asked for the piles on ({q}, {r}), which {other} has open");
                continue;
            }
            open(ent, (q, r), position, facing, &piles, &mut commands, &mut writer);
            opened.push((ent, (q, r)));
            if !working {
                show(ent, Some(common::gathering::Activity::Pickup), &mut commands);
            }
        } else if let Some(work) = common::gathering::work(cover, slot) {
            if working {
                continue;
            }
            let until = Moment::ZERO + time.elapsed() + std::time::Duration::from_millis(common::gathering::WORK_MS);
            commands.entity(ent).insert(Working { q, r, slot, until, from: position, facing });
            show(ent, Some(common::gathering::Activity::Work(work)), &mut commands);
        } else {
            info!("gather: {ent} asked for slot {slot} of ({q}, {r}), which holds nothing gatherable");
            continue;
        }
        if let Ok((_, mut heading, mut turn, mut at, ..)) = players.get_mut(ent) {
            turn.heading = facing;
            *heading = facing;
            *at = position;
        }
    }
}

/// Ends each player's work: broken off where it has moved or turned since
/// it began; done when its time is up, when what it worked on is gathered,
/// if it is still there and in reach. Its yield lies at once, on the
/// tile's pile of its kind or as a new one in the slot the gather freed,
/// and its window opens on the tile, unless another player's is open
/// there, who sees the yield arrive.
pub fn finish_work(
    mut commands: Commands,
    mut writer: MessageWriter<Do>,
    mut ground: Ground,
    mut piles: ResMut<Piles>,
    time: Res<Time>,
    windows: Windows,
    players: Query<(Entity, &Loc, &Heading, &Position, &Working, Has<Looting>)>,
) {
    for (ent, loc, heading, position, working, looting) in &players {
        if *position != working.from || *heading != working.facing {
            stop(ent, looting, &mut commands);
            continue;
        }
        if Moment::ZERO + time.elapsed() < working.until {
            continue;
        }
        let Working { q, r, slot, .. } = *working;
        let harvest = ground.cover(q, r).and_then(|cover| common::gathering::harvest(cover, slot));
        let Some(harvest) = harvest.filter(|_| in_reach(**loc, *heading, q, r)) else {
            info!("gather: {ent} finished on slot {slot} of ({q}, {r}), which it can no longer gather");
            stop(ent, looting, &mut commands);
            continue;
        };
        info!("gather: {ent} gathered {} {:?} from slot {slot} of ({q}, {r})", harvest.amount, harvest.material);
        let stack = harvest.stack();
        match piles.on(q, r).into_iter().find(|(_, held)| held.kind == stack.kind) {
            Some((onto, held)) => {
                piles.0.insert((q, r, onto), common::Stack { count: held.count + stack.count, ..held });
                ground.set(&mut writer, q, r, harvest.cover);
            }
            None => {
                piles.0.insert((q, r, harvest.freed), stack);
                ground.set(&mut writer, q, r, common::gathering::left(harvest.cover, harvest.freed, harvest.material));
            }
        }
        match holder(&windows, (q, r)).filter(|&other| other != ent) {
            Some(other) => {
                stop(ent, looting, &mut commands);
                writer.write(Do { event: Event::Loot { ent: other, entries: Some(piles.entries(q, r)) } });
            }
            None => {
                stop(ent, true, &mut commands);
                open(ent, (q, r), *position, *heading, &piles, &mut commands, &mut writer);
            }
        }
    }
}

/// Breaks off the work of each player something lands on.
pub fn interrupt_work(mut commands: Commands, mut reader: MessageReader<Do>, working: Query<Has<Looting>, With<Working>>) {
    for message in reader.read() {
        let Do { event: Event::Resolved { target: Some(struck), outcome: Outcome::Landed, .. } } = message else { continue };
        if let Ok(looting) = working.get(*struck) {
            stop(*struck, looting, &mut commands);
        }
    }
}

/// Takes what a player asks from the tile its window is open on: one
/// entry's pile or every pile, each as far as the bag has room and weight
/// for. A pile emptied frees its slot, and the last one closes the window.
pub fn try_take(
    mut commands: Commands,
    mut reader: MessageReader<Try>,
    mut writer: MessageWriter<Do>,
    mut ground: Ground,
    mut piles: ResMut<Piles>,
    mut players: Query<(&Equipment, &mut Inventory, &Looting, Has<Working>)>,
) {
    for message in reader.read() {
        let Try { event: Event::Take { ent, entry } } = message else { continue };
        let (ent, entry) = (*ent, *entry);
        let Ok((worn, mut bag, looting, working)) = players.get_mut(ent) else { continue };
        let (q, r) = (looting.q, looting.r);
        let lying = piles.on(q, r);
        let chosen = match entry {
            Some(entry) => match lying.get(entry as usize) {
                Some(&pile) => vec![pile],
                None => continue,
            },
            None => lying,
        };
        let Some(before) = ground.cover(q, r) else { continue };
        let mut cover = before;
        let mut taken = false;
        for (slot, stack) in chosen {
            let count = stack.count.min(bag.room_for(worn, stack.kind));
            if count == 0 {
                continue;
            }
            taken = true;
            bag.add(common::Stack { kind: stack.kind, count });
            if count == stack.count {
                piles.0.remove(&(q, r, slot));
                cover = common::gathering::emptied(cover, slot);
            } else if let Some(left) = piles.0.get_mut(&(q, r, slot)) {
                left.count -= count;
            }
        }
        if cover != before {
            ground.set(&mut writer, q, r, cover);
        } else if taken {
            ground.piled(q, r);
        }
        writer.write(Do { event: Event::Inventory { ent, bag: bag.clone() } });
        let left = piles.entries(q, r);
        if !left.is_empty() {
            writer.write(Do { event: Event::Loot { ent, entries: Some(left) } });
            continue;
        }
        commands.entity(ent).remove::<Looting>();
        writer.write(Do { event: Event::Loot { ent, entries: None } });
        if !working {
            show(ent, None, &mut commands);
        }
    }
}

/// Closes each window its player asks to close, or whose player has moved
/// or turned at all since it opened, leaving its tile's piles to anyone.
pub fn close_windows(
    mut commands: Commands,
    mut reader: MessageReader<Try>,
    mut writer: MessageWriter<Do>,
    players: Query<(Entity, &Position, &Heading, &Looting, Has<Working>)>,
) {
    let asked: Vec<Entity> = reader
        .read()
        .filter_map(|m| match m.event {
            Event::CloseLoot { ent } => Some(ent),
            _ => None,
        })
        .collect();
    for (ent, position, heading, looting, working) in &players {
        if !asked.contains(&ent) && *position == looting.from && *heading == looting.facing {
            continue;
        }
        commands.entity(ent).remove::<Looting>();
        writer.write(Do { event: Event::Loot { ent, entries: None } });
        if !working {
            show(ent, None, &mut commands);
        }
    }
}

/// Where a drop of `kind` lands among `tiles`, each with its cover: the
/// slot of the pile of `kind` nearest `from`, xz from the centre of tile
/// `frame`, or else the free slot nearest it, with its tile's cover. The
/// flag says it lands on a pile already lying there.
fn landing(
    tiles: &[((i32, i32), common::Cover)],
    piles: &Piles,
    kind: common::Stackable,
    frame: (i32, i32),
    from: Vec2,
) -> Option<((i32, i32), common::Cover, usize, bool)> {
    let slots = || tiles.iter().flat_map(|&(tile, cover)| (0..common::TILE_SLOTS as usize).map(move |k| (tile, cover, k)));
    let nearest = |fits: &dyn Fn((i32, i32), common::Cover, usize) -> bool| {
        let distance = |(tile, cover, k)| Vec2::from(common_bevy::geometry::slot_point(cover, tile, k, frame)).distance_squared(from);
        slots().filter(|&(tile, cover, k)| fits(tile, cover, k)).min_by(|&a, &b| distance(a).total_cmp(&distance(b)))
    };
    let onto = nearest(&|(q, r), _, k| piles.held((q, r, k)).is_some_and(|stack| stack.kind == kind));
    let free = || nearest(&|_, cover, k| cover.content(k) == common::Content::Empty);
    onto.map(|(tile, cover, k)| (tile, cover, k, true)).or_else(|| free().map(|(tile, cover, k)| (tile, cover, k, false)))
}

/// The six tiles round `(q, r)`.
const AROUND: [(i32, i32); 6] = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, -1), (-1, 1)];

/// Lays what a player drops from its bag: onto a pile of its kind on the
/// tile it stands on, or as a new pile in that tile's free slot nearest
/// the player; a tile with neither spills it the same way onto the tiles
/// round it that stand within a step of its height. Where none of those
/// takes it either, the bag keeps it. A player whose window is open where
/// it lands sees it arrive.
pub fn try_drop(
    mut reader: MessageReader<Try>,
    mut writer: MessageWriter<Do>,
    mut ground: Ground,
    mut piles: ResMut<Piles>,
    windows: Windows,
    mut players: Query<(&Loc, &Position, &mut Inventory)>,
) {
    for message in reader.read() {
        let Try { event: Event::Drop { ent, kind, count } } = message else { continue };
        let (ent, kind, count) = (*ent, *kind, *count);
        let Ok((loc, position, mut bag)) = players.get_mut(ent) else { continue };
        if bag.count(kind) == 0 {
            continue;
        }
        let (q, r) = (loc.q, loc.r);
        let Some((height, cover)) = ground.lies(q, r) else {
            info!("drop: {ent} stands on ({q}, {r}), which the map does not hold");
            continue;
        };
        let round: Vec<((i32, i32), common::Cover)> = AROUND
            .iter()
            .filter_map(|&(dq, dr)| {
                let (z, cover) = ground.lies(q + dq, r + dr)?;
                ((z - height).abs() <= 1).then_some(((q + dq, r + dr), cover))
            })
            .collect();
        let frame = (position.tile.q, position.tile.r);
        let from = position.offset.xz();
        let landed = landing(&[((q, r), cover)], &piles, kind, frame, from).or_else(|| landing(&round, &piles, kind, frame, from));
        let Some(((q, r), cover, slot, onto)) = landed else {
            info!("drop: {ent} at ({q}, {r}) has nowhere to drop {kind:?}");
            continue;
        };
        let stack = bag.remove(kind, count);
        writer.write(Do { event: Event::Inventory { ent, bag: bag.clone() } });
        if onto {
            if let Some(held) = piles.0.get_mut(&(q, r, slot)) {
                held.count += stack.count;
            }
            ground.piled(q, r);
        } else {
            piles.0.insert((q, r, slot), stack);
            let common::Stackable::Material(material) = kind;
            ground.set(&mut writer, q, r, common::gathering::left(cover, slot, material));
        }
        if let Some(holder) = holder(&windows, (q, r)) {
            writer.write(Do { event: Event::Loot { ent: holder, entries: Some(piles.entries(q, r)) } });
        }
        info!("drop: {ent} dropped {} {kind:?} in slot {slot} of ({q}, {r})", stack.count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::{summary::{sample_offsets, summarize}, Content, Cover};

    /// A wood of one pine to a tile, everywhere.
    struct Wood;
    impl SummarySource for Wood {
        fn sample(&self, _: i32, _: i32) -> Option<TileSample> {
            Some(TileSample { z: 0, water: None, cover: Cover::NONE.with(0, Content::Pine) })
        }
    }

    /// A summary reads its samples as players left them: a tree felled on
    /// a tile no part is read at leaves it be, one felled on a sample comes
    /// off its part's canopy, and a reading taken before the change keeps
    /// the changes as they stood. Each change is kept fresh, before and
    /// after, until the summaries take it.
    #[test]
    fn a_summary_reads_its_samples_as_players_left_them() {
        let r = 4;
        let pines = |cell: Option<common::summary::SummaryCell>| -> u16 {
            cell.expect("every sample is there").canopy.iter().map(|part| part.share(Content::Pine)).sum()
        };
        let a_site = common::cover::CANOPY_WHOLE / common::SITES.len() as u16;
        let generated = pines(summarize(r, 0, 0, &Wood));
        let pine = Cover::NONE.with(0, Content::Pine);
        let mut changes = WorldChanges::default();

        changes.set(1, 1, pine, Cover::NONE);
        assert_eq!(pines(summarize(r, 0, 0, &changes.over(Wood))), generated, "no part is read at (1, 1)");

        let before = changes.over(Wood);
        let (dq, dr) = sample_offsets(r)[2];
        changes.set(dq, dr, pine, Cover::NONE);
        assert_eq!(pines(summarize(r, 0, 0, &changes.over(Wood))), generated - a_site, "the felled sample is off the canopy");
        assert_eq!(pines(summarize(r, 0, 0, &before)), generated, "a reading already out keeps what it set out with");
        let fresh = changes.take_fresh();
        assert_eq!(fresh.iter().map(|c| (c.q, c.r)).collect::<Vec<_>>(), vec![(1, 1), (dq, dr)]);
        assert!(fresh.iter().all(|c| c.before == pine && c.after == Cover::NONE));
        assert!(changes.take_fresh().is_empty());
    }

    /// Where changes are kept, a chunk is held only once recalled, and is
    /// asked after once however often it is asked; where they are not,
    /// every chunk is held from the start.
    #[test]
    fn a_kept_chunk_is_held_once_recalled_and_asked_after_once() {
        let chunk = ChunkId(3, -1);
        let mut changes = WorldChanges::default();
        assert!(changes.holds(chunk), "nothing kept, every chunk held");
        assert!(changes.take_wanted().is_empty());

        changes.keep();
        assert!(!changes.holds(chunk) && !changes.holds(chunk));
        assert_eq!(changes.take_wanted(), vec![chunk]);
        assert!(!changes.holds(chunk));
        assert!(changes.take_wanted().is_empty(), "asked after once while the recall is out");
        changes.recalled(chunk);
        assert!(changes.holds(chunk));
    }

    /// A recalled cover is laid as a fresh change from the tile as it
    /// stands, once however often it arrives, and never over a chunk held
    /// here, whose own changes are newer.
    #[test]
    fn a_recalled_change_is_laid_once_and_never_over_a_held_chunk() {
        let pine = Cover::NONE.with(0, Content::Pine);
        let stump = common::gathering::harvest(pine, common::SITE_SLOTS[0][0]).expect("a pine is felled").cover;
        let generated = |_, _| pine;
        let mut changes = WorldChanges::default();
        changes.keep();

        changes.recall([(5, 5, stump)], generated);
        changes.recall([(5, 5, stump)], generated);
        assert_eq!(changes.take_fresh(), vec![Change { q: 5, r: 5, before: pine, after: stump }]);

        let held = loc_to_chunk(Qrz { q: 40, r: 40, z: 0 });
        changes.recalled(held);
        changes.set(40, 40, pine, Cover::NONE);
        changes.take_fresh();
        changes.recall([(40, 40, stump)], generated);
        assert!(changes.take_fresh().is_empty());
        assert_eq!(changes.get(40, 40), Some(Cover::NONE), "the held chunk keeps its own");
    }

    /// A drop goes onto the pile of its kind, or else to the free slot
    /// nearest the player; tiles with neither take nothing.
    #[test]
    fn a_drop_lands_on_its_pile_or_the_nearest_free_slot() {
        let softwood = common::Stackable::Material(common::Material::Softwood);
        let stone = common::Stackable::Material(common::Material::Limestone);
        let tile = (3, -2);
        let cover = common::gathering::left(Cover::NONE.with_boulder(0), 4, common::Material::Softwood);
        let mut piles = Piles::default();
        piles.0.insert((3, -2, 4), common::Stack { kind: softwood, count: 4 });
        let here = [(tile, cover)];

        assert_eq!(landing(&here, &piles, softwood, tile, Vec2::ZERO), Some((tile, cover, 4, true)));

        let free = |k: usize| cover.content(k) == Content::Empty;
        for k in (0..common::TILE_SLOTS as usize).filter(|&k| free(k)) {
            let at = Vec2::from(common_bevy::geometry::slot_point(cover, tile, k, tile));
            assert_eq!(landing(&here, &piles, stone, tile, at), Some((tile, cover, k, false)), "standing on free slot {k}");
        }

        let full = (0..common::TILE_SLOTS as usize).fold(cover, |c, k| if free(k) { c.with_boulder(k) } else { c });
        assert_eq!(landing(&[(tile, full)], &piles, stone, tile, Vec2::ZERO), None);
        assert_eq!(landing(&[(tile, full)], &piles, softwood, tile, Vec2::ZERO), Some((tile, full, 4, true)));
    }

    /// What a full tile cannot take spills to the tiles round it: onto a
    /// pile of its kind there before any free slot, and into the free slot
    /// nearest the player among them all.
    #[test]
    fn a_drop_spills_to_the_nearest_slot_round_a_full_tile() {
        let stone = common::Stackable::Material(common::Material::Limestone);
        let round = AROUND.map(|(dq, dr)| ((dq, dr), Cover::NONE));
        let (east, _, _, onto) = landing(&round, &Piles::default(), stone, (0, 0), Vec2::new(0.4, 0.0)).expect("a free slot lies round it");
        assert!(!onto);
        let (west, ..) = landing(&round, &Piles::default(), stone, (0, 0), Vec2::new(-0.4, 0.0)).expect("a free slot lies round it");
        assert_ne!(east, west, "it falls on the side the player stands toward");

        let mut piles = Piles::default();
        piles.0.insert((west.0, west.1, 3), common::Stack { kind: stone, count: 1 });
        let cover = common::gathering::left(Cover::NONE, 3, common::Material::Limestone);
        let round = round.map(|(tile, bare)| (tile, if tile == west { cover } else { bare }));
        assert_eq!(landing(&round, &piles, stone, (0, 0), Vec2::new(0.4, 0.0)), Some((west, cover, 3, true)));
    }

    /// A clearing takes every tree and boulder within its radius, leaves
    /// a stump for each tree and no pile, and touches nothing past it.
    #[test]
    fn a_clearing_takes_what_stands_within_its_radius() {
        let wood = |_: i32, _: i32| Cover::NONE.with(0, Content::Pine).with_boulder(4).with_rock(common::Rock::Limestone);
        let changes = WorldChanges::clearing(wood, (10, -4), 2, false);
        assert_eq!(changes.tiles.len(), 19, "a radius of 2 is 19 tiles");
        assert_eq!(changes.fresh.len(), 19, "each one a change for the summaries to take");
        let cleared = changes.tiles[&(12, -6)];
        assert_eq!(cleared.content(common::SITE_SLOTS[0][0]), Content::PineStump);
        assert!((0..common::TILE_SLOTS as usize).all(|k| !cleared.content(k).is_pile() && common::gathering::harvest(cleared, k).is_none()));
        assert!(!changes.tiles.contains_key(&(13, -4)), "three tiles out is untouched");
    }

    /// A fading clearing takes every tree at its centre, fewer toward its
    /// edge, and the same trees every time.
    #[test]
    fn a_fading_clearing_thins_toward_its_edge() {
        let wood = |_: i32, _: i32| Cover::NONE.with(0, Content::Pine);
        let radius = 20;
        let changes = WorldChanges::clearing(wood, (0, 0), radius, true);
        assert!(changes.tiles.contains_key(&(0, 0)), "the centre falls");
        let felled_at = |d: i32| {
            let ring: Vec<(i32, i32)> = (-d..=d)
                .flat_map(|q| (-d..=d).map(move |r| (q, r)))
                .filter(|&(q, r)| qrz::hex_distance((q, r), (0, 0)) == d)
                .collect();
            ring.iter().filter(|tile| changes.tiles.contains_key(tile)).count() as f64 / ring.len() as f64
        };
        let (inner, outer) = (felled_at(3), felled_at(radius - 2));
        assert!(inner > 0.6 && outer < 0.4, "{inner} near the centre, {outer} near the edge");
        assert_eq!(WorldChanges::clearing(wood, (0, 0), radius, true).tiles, changes.tiles);
    }
}
