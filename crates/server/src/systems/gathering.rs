//! Gathering: a player works at what it gathers for a time, and the yield
//! then lies at once as a pile, kept for it until it lets go. What lies
//! on the ground lies one stack to a slot; a loot window opens on a tile
//! and shows its piles, a slot to a cell, to every player who has one
//! open there, and each takes what it may into its bag. Every tile
//! changed keeps the change, and every client holding the tile learns it.
//! Every stack that appears or changes holder is announced as a
//! [`Moved`].

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

use crate::systems::{
    actor::VisibleChunkCache,
    characters::Character,
    economy::{Cause, Holder, Moved},
};

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

    fn cell(&self, r: u32, sq: i32, sr: i32) -> Option<(common::grid::Ground, u16)> {
        self.source.cell(r, sq, sr)
    }
}

/// A pile: the stack lying in one slot of a tile, and the player it is
/// kept for, who gathered it and has not yet let go of it.
#[derive(Clone, Copy, Debug)]
struct Pile {
    stack: common::Stack,
    kept_for: Option<Entity>,
}

/// Everything lying on the ground: one stack to a slot of a tile, each a
/// pile of its own. A pile's content in the cover says where it lies and
/// what it looks like; this says what is in it.
#[derive(Resource, Default)]
pub struct Piles(HashMap<(i32, i32, usize), Pile>);

/// What a loot window shows: a tile's slots, each the pile its player may
/// take from, in slot order.
pub type Cells = [Option<common::Stack>; common::TILE_SLOTS as usize];

impl Piles {
    /// What the pile at `key` holds, where one lies.
    pub fn held(&self, key: (i32, i32, usize)) -> Option<common::Stack> {
        self.0.get(&key).map(|pile| pile.stack)
    }

    /// Lays `stack` at `key` as a pile that is anyone's, on the cover's
    /// pile there.
    pub fn lay(&mut self, key: (i32, i32, usize), stack: common::Stack) {
        self.0.insert(key, Pile { stack, kept_for: None });
    }

    /// Lays a kept pile holding `stack` at `key`, anyone's, where none
    /// lies already.
    pub fn recall(&mut self, key: (i32, i32, usize), stack: common::Stack) {
        self.0.entry(key).or_insert(Pile { stack, kept_for: None });
    }

    /// What `ent` may take at `key`, or anyone may where it is none: the
    /// pile there, unless it is kept for another player who has not let
    /// go of it.
    fn open_to(&self, key: (i32, i32, usize), ent: Option<Entity>, windows: &Windows) -> Option<common::Stack> {
        let pile = self.0.get(&key)?;
        let kept = pile.kept_for.is_some_and(|other| Some(other) != ent && windows.keeps(other, (key.0, key.1)));
        (!kept).then_some(pile.stack)
    }

    /// What `ent`'s window on tile `(q, r)` shows.
    fn cells(&self, (q, r): (i32, i32), ent: Entity, windows: &Windows) -> Cells {
        std::array::from_fn(|slot| self.open_to((q, r, slot), Some(ent), windows))
    }
}

/// Players changed tile `(q, r)`: what stands on it, or what a pile on it
/// holds.
#[derive(Message, Clone, Copy, Debug)]
pub struct TileChanged {
    pub q: i32,
    pub r: i32,
}

/// The tile a player has its loot window open on. The window shows the
/// tile's piles, to any number of players at once, and closes the moment
/// its player leaves its [`Stance`].
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Looting {
    pub q: i32,
    pub r: i32,
}

/// Where a player stood and faced as it last began a work or opened its
/// loot window, which moving from there or turning from that facing
/// breaks off. While it settles (`activation::SETTLE`) the stance follows
/// it instead: the walk under way at the press is still arriving then.
#[derive(Component, Clone, Copy, Debug)]
pub struct Stance {
    at: Position,
    facing: Heading,
    began: Moment,
}

impl Stance {
    pub fn new(at: Position, facing: Heading, began: Moment) -> Self {
        Self { at, facing, began }
    }

    /// Whether a player at `position` facing `heading` at `now` has kept
    /// it.
    pub fn kept(&mut self, position: Position, heading: Heading, now: Moment) -> bool {
        if now.since(self.began) < common_bevy::components::activation::SETTLE {
            (self.at, self.facing) = (position, heading);
        }
        self.at == position && self.facing == heading
    }
}

/// Every open loot window, and who is in the world to have one.
#[derive(SystemParam)]
pub struct Windows<'w, 's> {
    open: Query<'w, 's, (Entity, &'static Looting, Has<Working>)>,
    actors: Query<'w, 's, (), With<Loc>>,
}

impl Windows<'_, '_> {
    /// The tile `ent`'s window is open on.
    fn of(&self, ent: Entity) -> Option<(i32, i32)> {
        self.open.get(ent).ok().map(|(_, looting, _)| (looting.q, looting.r))
    }

    /// Whether `ent` still keeps what it gathered on `tile`: it is in the
    /// world and has no window open elsewhere. Closing its window lets go
    /// of it outright ([`release`]).
    fn keeps(&self, ent: Entity, tile: (i32, i32)) -> bool {
        self.actors.contains(ent) && self.of(ent).is_none_or(|open| open == tile)
    }

    /// Every player with a window open on `tile`, and whether it is at
    /// work.
    fn on(&self, tile: (i32, i32)) -> impl Iterator<Item = (Entity, bool)> + '_ {
        self.open.iter().filter(move |(_, looting, _)| (looting.q, looting.r) == tile).map(|(ent, _, working)| (ent, working))
    }
}

/// Whether tile `(q, r)` lies in the reach of a player on `here` facing
/// `heading`: its own tile or one of the three in its front half.
pub(crate) fn in_reach(here: Qrz, heading: Heading, q: i32, r: i32) -> bool {
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
    pub fn lies(&self, q: i32, r: i32) -> Option<(i32, common::Cover)> {
        match self.map.get_by_qr(q, r)? {
            (qrz, EntityType::Decorator(decorator)) => Some((qrz.z, decorator.cover)),
            _ => None,
        }
    }

    /// A player's change to tile `(q, r)`, which the map holds.
    pub fn set(&mut self, writer: &mut MessageWriter<Do>, q: i32, r: i32, cover: common::Cover) {
        self.put(writer, q, r, cover);
        self.changed.write(TileChanged { q, r });
    }

    /// A change to what a pile on tile `(q, r)` holds or to whose its
    /// construct is, which left the tile's cover as it was.
    pub(crate) fn piled(&mut self, q: i32, r: i32) {
        self.changed.write(TileChanged { q, r });
    }

    /// The map the ground is read from.
    pub fn map(&self) -> &Map {
        &self.map
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
/// at `until`. Leaving its [`Stance`] or being struck breaks it off.
#[derive(Component, Clone, Copy, Debug)]
pub struct Working {
    pub q: i32,
    pub r: i32,
    pub slot: usize,
    pub until: Moment,
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

/// What players do to the piles and each other's windows on them.
#[derive(SystemParam)]
pub struct Looters<'w, 's> {
    commands: Commands<'w, 's>,
    writer: MessageWriter<'w, Do>,
    piles: ResMut<'w, Piles>,
    ground: Ground<'w, 's>,
    windows: Windows<'w, 's>,
    moved: MessageWriter<'w, Moved>,
}

impl Looters<'_, '_> {
    /// Opens `ent`'s loot window on `tile`, held by the stance the player
    /// has. A window it had open elsewhere closes, and what it kept there
    /// is let go.
    fn open(&mut self, ent: Entity, tile: (i32, i32)) {
        if let Some(was) = self.windows.of(ent).filter(|&was| was != tile) {
            self.release(ent, was);
        }
        let cells = self.piles.cells(tile, ent, &self.windows);
        self.commands.entity(ent).insert(Looting { q: tile.0, r: tile.1 });
        self.writer.write(Do { event: Event::Loot { ent, cells: Some(cells) } });
    }

    /// Closes `ent`'s window, which shows it nothing more.
    fn close(&mut self, ent: Entity, working: bool) {
        self.commands.entity(ent).remove::<Looting>();
        self.writer.write(Do { event: Event::Loot { ent, cells: None } });
        if !working {
            show(ent, None, &mut self.commands);
        }
    }

    /// Tells each player with a window on `tile`, but `skip`, what it now
    /// shows, and closes one it shows nothing.
    fn refresh(&mut self, tile: (i32, i32), skip: Option<Entity>) {
        let open: Vec<(Entity, bool)> = self.windows.on(tile).filter(|(ent, _)| Some(*ent) != skip).collect();
        for (ent, working) in open {
            let cells = self.piles.cells(tile, ent, &self.windows);
            if cells.iter().all(Option::is_none) {
                self.close(ent, working);
            } else {
                self.writer.write(Do { event: Event::Loot { ent, cells: Some(cells) } });
            }
        }
    }

    /// Lets go of every pile on `tile` kept for `ent`: each is anyone's,
    /// and joins the tile's pile of its kind where one lies. Everyone
    /// else with a window there sees them.
    fn release(&mut self, ent: Entity, (q, r): (i32, i32)) {
        let Some(before) = self.ground.cover(q, r) else { return };
        let mut cover = before;
        for slot in 0..common::TILE_SLOTS as usize {
            let Some(pile) = self.piles.0.get_mut(&(q, r, slot)).filter(|pile| pile.kept_for == Some(ent)) else { continue };
            pile.kept_for = None;
            let stack = pile.stack;
            let joins = (0..common::TILE_SLOTS as usize).find(|&other| {
                other != slot && self.piles.0.get(&(q, r, other)).is_some_and(|pile| pile.stack.kind == stack.kind && pile.kept_for.is_none())
            });
            let Some(other) = joins else { continue };
            if let Some(pile) = self.piles.0.get_mut(&(q, r, other)) {
                pile.stack.count += stack.count;
                self.piles.0.remove(&(q, r, slot));
                cover = common::gathering::emptied(cover, slot);
                self.moved.write(Moved::stack(stack, Holder::Pile(q, r, slot), Holder::Pile(q, r, other), Cause::LetGo));
            }
        }
        if cover != before {
            self.ground.set(&mut self.writer, q, r, cover);
        }
        self.refresh((q, r), Some(ent));
    }

    /// Lays `stack` down from tile `(q, r)`, at `from`, xz from the centre
    /// of tile `frame`: onto a pile of its kind there that `taker` may
    /// take from, or anyone may where there is none, or else as a new
    /// pile in the tile's free slot nearest; a tile with neither spills
    /// it the same way onto the tiles round it that stand within a step
    /// of its height. Every window open where it lands shows it. None
    /// where the map does not hold the tile or none of those takes it,
    /// and nothing is laid.
    pub(crate) fn lay(&mut self, stack: common::Stack, (q, r): (i32, i32), frame: (i32, i32), from: Vec2, taker: Option<Entity>) -> Option<(i32, i32, usize)> {
        let (height, cover) = self.ground.lies(q, r)?;
        let round: Vec<((i32, i32), common::Cover)> = AROUND
            .iter()
            .filter_map(|&(dq, dr)| {
                let (z, cover) = self.ground.lies(q + dq, r + dr)?;
                ((z - height).abs() <= 1).then_some(((q + dq, r + dr), cover))
            })
            .collect();
        let joins = |key| self.piles.open_to(key, taker, &self.windows).is_some_and(|lying| lying.kind == stack.kind);
        let ((q, r), cover, slot, onto) = landing(&[((q, r), cover)], joins, frame, from).or_else(|| landing(&round, joins, frame, from))?;
        if onto {
            if let Some(pile) = self.piles.0.get_mut(&(q, r, slot)) {
                pile.stack.count += stack.count;
            }
            self.ground.piled(q, r);
        } else {
            self.piles.0.insert((q, r, slot), Pile { stack, kept_for: None });
            let common::Stackable::Material(material) = stack.kind;
            self.ground.set(&mut self.writer, q, r, common::gathering::left(cover, slot, material));
        }
        self.refresh((q, r), None);
        Some((q, r, slot))
    }

    /// Announces that `stack` went from one holder to another.
    pub(crate) fn moved(&mut self, stack: common::Stack, from: Holder, to: Holder, cause: Cause) {
        self.moved.write(Moved::stack(stack, from, to, cause));
    }
}

/// Answers each gather a player asks for: the slot must lie in its reach.
/// A pile there opens the player's window on its tile at once, where the
/// tile holds something it may take. Something gatherable there sets the
/// player to work on it, unless it is at work already.
pub fn try_gather(
    mut reader: MessageReader<Try>,
    mut looters: Looters,
    time: Res<Time>,
    nntree: Res<common_bevy::plugins::nntree::NNTree>,
    mut players: Query<(&Loc, &mut Heading, &mut common_bevy::components::Turn, &mut Position, Has<Working>)>,
) {
    let now = Moment::ZERO + time.elapsed();
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
        let Some(cover) = looters.ground.cover(q, r) else {
            info!("gather: {ent} asked for ({q}, {r}), which the map does not hold");
            continue;
        };
        // A construct is used, which `building::try_use` answers
        if common::building::used(cover, slot) {
            continue;
        }
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
            .and_then(|activity| stand_point(position.tile, position.offset, facing, (q, r), slot, activity, &looters.ground.map, &nntree))
            .map_or(position, |stand| stepped(position, stand, &looters.ground.map));
        if cover.content(slot).is_pile() {
            if looters.piles.cells((q, r), ent, &looters.windows).iter().all(Option::is_none) {
                info!("gather: {ent} asked for the piles on ({q}, {r}), which holds none it may take");
                continue;
            }
            looters.open(ent, (q, r));
            if !working {
                show(ent, Some(common::gathering::Activity::Pickup), &mut looters.commands);
            } else if facing != heading {
                // Turning to the pile leaves the work
                stop(ent, true, &mut looters.commands);
            }
        } else if let Some(work) = common::gathering::work(cover, slot) {
            if working {
                continue;
            }
            let until = now + std::time::Duration::from_millis(common::gathering::WORK_MS);
            looters.commands.entity(ent).insert(Working { q, r, slot, until });
            show(ent, Some(common::gathering::Activity::Work(work)), &mut looters.commands);
        } else {
            info!("gather: {ent} asked for slot {slot} of ({q}, {r}), which holds nothing gatherable");
            continue;
        }
        looters.commands.entity(ent).insert(Stance::new(position, facing, now));
        if let Ok((_, mut heading, mut turn, mut at, ..)) = players.get_mut(ent) {
            turn.heading = facing;
            *heading = facing;
            *at = position;
        }
    }
}

/// Ends each player's work: broken off where it has left its stance; done
/// when its time is up, when what it worked on is gathered,
/// if it is still there and in reach. Its yield lies at once, kept for
/// it: on the pile of its kind it keeps there already, or as a new one in
/// the slot the gather freed; and its window opens on the tile.
pub fn finish_work(
    mut looters: Looters,
    time: Res<Time>,
    mut players: Query<(Entity, &Loc, &Heading, &Position, &Working, &mut Stance, Has<Looting>)>,
) {
    let now = Moment::ZERO + time.elapsed();
    for (ent, loc, heading, position, working, mut stance, looting) in &mut players {
        if !stance.kept(*position, *heading, now) {
            stop(ent, looting, &mut looters.commands);
            continue;
        }
        if now < working.until {
            continue;
        }
        let Working { q, r, slot, .. } = *working;
        let harvest = looters.ground.cover(q, r).and_then(|cover| common::gathering::harvest(cover, slot));
        let Some(harvest) = harvest.filter(|_| in_reach(**loc, *heading, q, r)) else {
            info!("gather: {ent} finished on slot {slot} of ({q}, {r}), which it can no longer gather");
            stop(ent, looting, &mut looters.commands);
            continue;
        };
        info!("gather: {ent} gathered {} {:?} from slot {slot} of ({q}, {r})", harvest.amount, harvest.material);
        let stack = harvest.stack();
        let own = (0..common::TILE_SLOTS as usize)
            .find(|&k| looters.piles.0.get(&(q, r, k)).is_some_and(|pile| pile.stack.kind == stack.kind && pile.kept_for == Some(ent)));
        match own.and_then(|k| looters.piles.0.get_mut(&(q, r, k))) {
            Some(pile) => {
                pile.stack.count += stack.count;
                looters.ground.set(&mut looters.writer, q, r, harvest.cover);
            }
            None => {
                looters.piles.0.insert((q, r, harvest.freed), Pile { stack, kept_for: Some(ent) });
                looters.ground.set(&mut looters.writer, q, r, common::gathering::left(harvest.cover, harvest.freed, harvest.material));
            }
        }
        looters.moved(stack, Holder::World, Holder::Pile(q, r, own.unwrap_or(harvest.freed)), Cause::Gather);
        stop(ent, true, &mut looters.commands);
        looters.open(ent, (q, r));
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

/// Takes what a player asks from the tile its window is open on: the pile
/// in one slot or every pile it may take, each as far as the bag has room
/// and weight for. A pile emptied frees its slot, and every window on the
/// tile shows what is left.
pub fn try_take(mut reader: MessageReader<Try>, mut looters: Looters, mut players: Query<(&Equipment, &mut Inventory, &Looting, &Character)>) {
    for message in reader.read() {
        let Try { event: Event::Take { ent, slot } } = message else { continue };
        let (ent, slot) = (*ent, *slot);
        let Ok((worn, mut bag, looting, &Character(who))) = players.get_mut(ent) else { continue };
        let (q, r) = (looting.q, looting.r);
        let cells = looters.piles.cells((q, r), ent, &looters.windows);
        let chosen: Vec<usize> = match slot {
            Some(slot) if (slot as usize) < cells.len() => vec![slot as usize],
            Some(_) => continue,
            None => (0..cells.len()).collect(),
        };
        let Some(before) = looters.ground.cover(q, r) else { continue };
        let mut cover = before;
        let mut taken = false;
        for slot in chosen {
            let Some(stack) = cells[slot] else { continue };
            let count = stack.count.min(bag.room_for(worn, stack.kind));
            if count == 0 {
                continue;
            }
            taken = true;
            let took = common::Stack { kind: stack.kind, count };
            bag.add(took);
            looters.moved(took, Holder::Pile(q, r, slot), Holder::Character(who), Cause::Take);
            if count == stack.count {
                looters.piles.0.remove(&(q, r, slot));
                cover = common::gathering::emptied(cover, slot);
            } else if let Some(left) = looters.piles.0.get_mut(&(q, r, slot)) {
                left.stack.count -= count;
            }
        }
        if !taken {
            continue;
        }
        if cover != before {
            looters.ground.set(&mut looters.writer, q, r, cover);
        } else {
            looters.ground.piled(q, r);
        }
        looters.writer.write(Do { event: Event::Inventory { ent, bag: bag.clone() } });
        looters.refresh((q, r), None);
    }
}

/// Closes each window its player asks to close, or whose player has left
/// its stance, and lets go of what it kept there.
pub fn close_windows(
    mut reader: MessageReader<Try>,
    mut looters: Looters,
    time: Res<Time>,
    mut players: Query<(Entity, &Position, &Heading, &Looting, &mut Stance, Has<Working>)>,
) {
    let now = Moment::ZERO + time.elapsed();
    let asked: Vec<Entity> = reader
        .read()
        .filter_map(|m| match m.event {
            Event::CloseLoot { ent } => Some(ent),
            _ => None,
        })
        .collect();
    for (ent, position, heading, looting, mut stance, working) in &mut players {
        if stance.kept(*position, *heading, now) && !asked.contains(&ent) {
            continue;
        }
        looters.release(ent, (looting.q, looting.r));
        looters.close(ent, working);
    }
}

/// Where a drop lands among `tiles`, each with its cover: the slot
/// nearest `from`, xz from the centre of tile `frame`, of those holding a
/// pile it `joins`, or else the free slot nearest it, with its tile's
/// cover. The flag says it lands on a pile already lying there.
fn landing(
    tiles: &[((i32, i32), common::Cover)],
    joins: impl Fn((i32, i32, usize)) -> bool,
    frame: (i32, i32),
    from: Vec2,
) -> Option<((i32, i32), common::Cover, usize, bool)> {
    let slots = || tiles.iter().flat_map(|&(tile, cover)| (0..common::TILE_SLOTS as usize).map(move |k| (tile, cover, k)));
    let nearest = |fits: &dyn Fn((i32, i32), common::Cover, usize) -> bool| {
        let distance = |(tile, cover, k)| Vec2::from(common_bevy::geometry::slot_point(cover, tile, k, frame)).distance_squared(from);
        slots().filter(|&(tile, cover, k)| fits(tile, cover, k)).min_by(|&a, &b| distance(a).total_cmp(&distance(b)))
    };
    let onto = nearest(&|(q, r), _, k| joins((q, r, k)));
    let free = || nearest(&|_, cover, k| cover.content(k) == common::Content::Empty);
    onto.map(|(tile, cover, k)| (tile, cover, k, true)).or_else(|| free().map(|(tile, cover, k)| (tile, cover, k, false)))
}

/// The six tiles round `(q, r)`.
const AROUND: [(i32, i32); 6] = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, -1), (-1, 1)];

/// Lays what a player drops from its bag, from where it stands
/// ([`Looters::lay`]). Where nowhere takes it, the bag keeps it.
pub fn try_drop(mut reader: MessageReader<Try>, mut looters: Looters, mut players: Query<(&Loc, &Position, &mut Inventory, &Character)>) {
    for message in reader.read() {
        let Try { event: Event::Drop { ent, kind, count } } = message else { continue };
        let (ent, kind, count) = (*ent, *kind, *count);
        let Ok((loc, position, mut bag, &Character(who))) = players.get_mut(ent) else { continue };
        let stack = common::Stack { kind, count: count.min(bag.count(kind)) };
        if stack.count == 0 {
            continue;
        }
        let Some((q, r, slot)) = looters.lay(stack, (loc.q, loc.r), (position.tile.q, position.tile.r), position.offset.xz(), Some(ent)) else {
            info!("drop: {ent} at ({}, {}) has nowhere to drop {kind:?}", loc.q, loc.r);
            continue;
        };
        bag.remove(kind, stack.count);
        looters.writer.write(Do { event: Event::Inventory { ent, bag: bag.clone() } });
        looters.moved(stack, Holder::Character(who), Holder::Pile(q, r, slot), Cause::Drop);
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

    fn lying(kind: common::Stackable, count: u32) -> Pile {
        Pile { stack: common::Stack { kind, count }, kept_for: None }
    }

    /// A drop goes onto the pile it joins, or else to the free slot
    /// nearest the player; tiles with neither take nothing.
    #[test]
    fn a_drop_lands_on_its_pile_or_the_nearest_free_slot() {
        let softwood = common::Stackable::Material(common::Material::Softwood);
        let stone = common::Stackable::Material(common::Material::Limestone);
        let tile = (3, -2);
        let cover = common::gathering::left(Cover::NONE.with_boulder(0), 4, common::Material::Softwood);
        let mut piles = Piles::default();
        piles.0.insert((3, -2, 4), lying(softwood, 4));
        let here = [(tile, cover)];
        let piles = &piles;
        let of = |kind| move |key| piles.held(key).is_some_and(|stack: common::Stack| stack.kind == kind);

        assert_eq!(landing(&here, of(softwood), tile, Vec2::ZERO), Some((tile, cover, 4, true)));

        let free = |k: usize| cover.content(k) == Content::Empty;
        for k in (0..common::TILE_SLOTS as usize).filter(|&k| free(k)) {
            let at = Vec2::from(common_bevy::geometry::slot_point(cover, tile, k, tile));
            assert_eq!(landing(&here, of(stone), tile, at), Some((tile, cover, k, false)), "standing on free slot {k}");
        }

        let full = (0..common::TILE_SLOTS as usize).fold(cover, |c, k| if free(k) { c.with_boulder(k) } else { c });
        assert_eq!(landing(&[(tile, full)], of(stone), tile, Vec2::ZERO), None);
        assert_eq!(landing(&[(tile, full)], of(softwood), tile, Vec2::ZERO), Some((tile, full, 4, true)));
    }

    /// What a full tile cannot take spills to the tiles round it: onto a
    /// pile it joins there before any free slot, and into the free slot
    /// nearest the player among them all.
    #[test]
    fn a_drop_spills_to_the_nearest_slot_round_a_full_tile() {
        let stone = common::Stackable::Material(common::Material::Limestone);
        let round = AROUND.map(|(dq, dr)| ((dq, dr), Cover::NONE));
        let none = |_| false;
        let (east, _, _, onto) = landing(&round, none, (0, 0), Vec2::new(0.4, 0.0)).expect("a free slot lies round it");
        assert!(!onto);
        let (west, ..) = landing(&round, none, (0, 0), Vec2::new(-0.4, 0.0)).expect("a free slot lies round it");
        assert_ne!(east, west, "it falls on the side the player stands toward");

        let mut piles = Piles::default();
        piles.0.insert((west.0, west.1, 3), lying(stone, 1));
        let cover = common::gathering::left(Cover::NONE, 3, common::Material::Limestone);
        let round = round.map(|(tile, bare)| (tile, if tile == west { cover } else { bare }));
        let joins = |key| piles.held(key).is_some();
        assert_eq!(landing(&round, joins, (0, 0), Vec2::new(0.4, 0.0)), Some((west, cover, 3, true)));
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
        let felled = common::gathering::harvest(Cover::NONE.with(0, Content::Pine).with_rock(common::Rock::Limestone), common::SITE_SLOTS[0][0]).unwrap().cover;
        assert_eq!(cleared, felled, "nothing but the stump stands, and nothing lies");
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
