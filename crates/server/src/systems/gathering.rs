//! Gathering: a player works at what it gathers for a time, and the yield
//! then lies at once as a pile, locked to it while its loot window is open
//! on it; it takes from the pile into its bag, and closing the window
//! leaves the rest to anyone. Every tile changed keeps the change, and every client
//! holding the tile learns it.

use std::{collections::HashMap, sync::Arc};

use bevy::{ecs::system::SystemParam, prelude::*};
use common::summary::{SummarySource, TileSample};
use common_bevy::{
    chunk::loc_to_chunk,
    components::{
        entity_type::{decorator::Decorator, EntityType},
        equipment::{Equipment, Inventory},
        heading::Heading,
        position::Position,
        Loc,
    },
    message::{Do, Event, Try},
    resources::map::Map,
    systems::movement::{stand_point, stepped},
};
use qrz::Qrz;

use crate::systems::actor::VisibleChunkCache;

/// Every tile players have changed, as its cover now stands. A tile is its
/// generated cover with its change laid over: whatever builds a tile for
/// the map or the wire takes it through [`WorldChanges::laid_over`], and a
/// summary reads its samples through [`WorldChanges::over`].
#[derive(Resource, Default)]
pub struct WorldChanges {
    /// Shared with the summary tasks in flight, each reading the changes
    /// as they stood when it set out; a change copies it while one holds it.
    tiles: Arc<HashMap<(i32, i32), common::Cover>>,
    /// Changes made since the summaries were last revised.
    fresh: Vec<Change>,
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

    /// Every tree felled and every boulder mined within `radius` tiles of
    /// `(q, r)`, by the rule a gather follows, with nothing left lying: a
    /// clearing laid down before the world is served, so what the far
    /// ground makes of a change can be seen without making it by hand.
    /// Every tile it clears is a fresh change, which the summaries take in
    /// before the first client is sent one.
    pub fn clearing(cover_at: impl Fn(i32, i32) -> common::Cover, (q, r): (i32, i32), radius: i32) -> Self {
        let mut changes = Self::default();
        for dq in -radius..=radius {
            for dr in (-radius).max(-dq - radius)..=radius.min(-dq + radius) {
                let generated = cover_at(q + dq, r + dr);
                let cleared = (0..common::TILE_SLOTS as usize)
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
}

/// A pile a player left: what it holds, and the player whose open window
/// it is locked to, whom alone it lets take from it.
#[derive(Clone, Debug)]
pub struct Pile {
    pub stacks: Vec<common::Stack>,
    pub locked_to: Option<Entity>,
}

/// Every pile, by its tile and slot. A pile's content in the cover says
/// where it lies and what it looks like; this says what is in it.
#[derive(Resource, Default)]
pub struct Piles(HashMap<(i32, i32, usize), Pile>);

/// The pile a player has its loot window open on, and where it stood and
/// faced as the window opened. The pile is locked to it while the window
/// is open; the window closes the moment the player moves or turns.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Looting {
    pub q: i32,
    pub r: i32,
    pub slot: usize,
    pub from: Position,
    pub facing: Heading,
}

impl Looting {
    fn key(self) -> (i32, i32, usize) {
        (self.q, self.r, self.slot)
    }
}

/// The player other than `ent` whose open window holds `pile`, lying at
/// `key`, where one does; `looting` says where a player's window is open.
/// A lock whose player has gone or closed its window holds nothing.
fn held_from(pile: &Pile, key: (i32, i32, usize), ent: Entity, looting: impl Fn(Entity) -> Option<Looting>) -> Option<Entity> {
    pile.locked_to.filter(|&other| other != ent && looting(other).is_some_and(|l| l.key() == key))
}

/// Whether tile `(q, r)` lies in the reach of a player on `here` facing
/// `heading`: its own tile or one of the three in its front half.
fn in_reach(here: Qrz, heading: Heading, q: i32, r: i32) -> bool {
    (q, r) == (here.q, here.r) || heading.front_dirs().iter().any(|&d| (q, r) == (here.q + d.q, here.r + d.r))
}

/// The ground as players change it: each change goes to the map, to the
/// changes every later tile is built with, and to every player holding the
/// tile's chunk.
#[derive(SystemParam)]
pub struct Ground<'w, 's> {
    map: Res<'w, Map>,
    changes: ResMut<'w, WorldChanges>,
    holders: Query<'w, 's, (Entity, &'static VisibleChunkCache)>,
}

impl Ground<'_, '_> {
    fn cover(&self, q: i32, r: i32) -> Option<common::Cover> {
        match self.map.get_by_qr(q, r)? {
            (_, EntityType::Decorator(decorator)) => Some(decorator.cover),
            _ => None,
        }
    }

    fn set(&mut self, writer: &mut MessageWriter<Do>, q: i32, r: i32, cover: common::Cover) {
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
    pub until: std::time::Duration,
    pub from: Position,
    pub facing: Heading,
}

/// Opens `ent`'s loot window on the pile at `key`, which holds `stacks`,
/// locking the pile to it, with the player standing `from` and facing
/// `facing`; a window it had open on another pile closes and unlocks that
/// pile.
#[allow(clippy::too_many_arguments)]
fn open(
    ent: Entity,
    key: (i32, i32, usize),
    stacks: Vec<common::Stack>,
    from: Position,
    facing: Heading,
    was: Option<Looting>,
    piles: &mut Piles,
    commands: &mut Commands,
    writer: &mut MessageWriter<Do>,
) {
    if let Some(was) = was.filter(|was| was.key() != key) {
        unlock(piles, was, ent);
    }
    if let Some(pile) = piles.0.get_mut(&key) {
        pile.locked_to = Some(ent);
    }
    commands.entity(ent).insert(Looting { q: key.0, r: key.1, slot: key.2, from, facing });
    writer.write(Do { event: Event::Loot { ent, entries: Some(stacks) } });
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
/// A pile there opens at once, unless another player's open window holds
/// it; one whose player has gone or closed it opens. Something gatherable
/// there sets the player to work on it, unless it is at work already.
pub fn try_gather(
    mut commands: Commands,
    mut reader: MessageReader<Try>,
    mut writer: MessageWriter<Do>,
    ground: Ground,
    mut piles: ResMut<Piles>,
    time: Res<Time>,
    nntree: Res<common_bevy::plugins::nntree::NNTree>,
    mut players: Query<(&Loc, &mut Heading, &mut common_bevy::components::Turn, &mut Position, Option<&Looting>, Has<Working>)>,
) {
    for message in reader.read() {
        let Try { event: Event::Gather { ent, q, r, slot } } = message else { continue };
        let (ent, q, r, slot) = (*ent, *q, *r, *slot as usize);
        let Ok((loc, heading, _, position, was, working)) = players.get(ent) else {
            warn!("gather: {ent} is not a player");
            continue;
        };
        let (loc, heading, position, was) = (**loc, *heading, *position, was.copied());
        if !in_reach(loc, heading, q, r) || slot >= common::TILE_SLOTS as usize {
            info!("gather: {ent} at {loc:?} asked out of reach for slot {slot} of ({q}, {r})");
            continue;
        }
        let Some(cover) = ground.cover(q, r) else {
            info!("gather: {ent} asked for ({q}, {r}), which the map does not hold");
            continue;
        };
        let key = (q, r, slot);
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
            let Some(pile) = piles.0.get(&key) else { continue };
            let looting = |other| players.get(other).ok().and_then(|(_, _, _, _, looting, _)| looting.copied());
            if let Some(other) = held_from(pile, key, ent, looting) {
                info!("gather: {ent} asked for the pile at slot {slot} of ({q}, {r}), which {other} has open");
                continue;
            }
            let stacks = pile.stacks.clone();
            open(ent, key, stacks, position, facing, was, &mut piles, &mut commands, &mut writer);
            if !working {
                show(ent, Some(common::gathering::Activity::Pickup), &mut commands);
            }
        } else if let Some(work) = common::gathering::work(cover, slot) {
            if working {
                continue;
            }
            let until = time.elapsed() + std::time::Duration::from_millis(common::gathering::WORK_MS);
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
/// it began; done when its time is up, when what it worked on is gathered —
/// its yield lies at once as a pile, locked to it, and its window opens —
/// if it is still there and in reach.
pub fn finish_work(
    mut commands: Commands,
    mut writer: MessageWriter<Do>,
    mut ground: Ground,
    mut piles: ResMut<Piles>,
    time: Res<Time>,
    players: Query<(Entity, &Loc, &Heading, &Position, &Working, Option<&Looting>)>,
) {
    for (ent, loc, heading, position, working, was) in &players {
        if *position != working.from || *heading != working.facing {
            stop(ent, was.is_some(), &mut commands);
            continue;
        }
        if time.elapsed() < working.until {
            continue;
        }
        let Working { q, r, slot, .. } = *working;
        let harvest = ground.cover(q, r).and_then(|cover| common::gathering::harvest(cover, slot));
        let Some(harvest) = harvest.filter(|_| in_reach(**loc, *heading, q, r)) else {
            info!("gather: {ent} finished on slot {slot} of ({q}, {r}), which it can no longer gather");
            stop(ent, was.is_some(), &mut commands);
            continue;
        };
        stop(ent, true, &mut commands);
        info!("gather: {ent} gathered {} {:?} from slot {slot} of ({q}, {r})", harvest.amount, harvest.material);
        let stacks = vec![harvest.stack()];
        ground.set(&mut writer, q, r, common::gathering::left(harvest.cover, harvest.freed, harvest.material));
        let key = (q, r, harvest.freed);
        piles.0.insert(key, Pile { stacks: stacks.clone(), locked_to: None });
        open(ent, key, stacks, *position, *heading, was.copied(), &mut piles, &mut commands, &mut writer);
    }
}

/// Breaks off the work of each player struck.
pub fn interrupt_work(mut commands: Commands, mut reader: MessageReader<Do>, working: Query<Has<Looting>, With<Working>>) {
    for message in reader.read() {
        let Do { event: Event::ApplyDamage { ent, .. } } = message else { continue };
        if let Ok(looting) = working.get(*ent) {
            stop(*ent, looting, &mut commands);
        }
    }
}

/// Unlocks the pile `looting` names, where it is locked to `ent`.
fn unlock(piles: &mut Piles, looting: Looting, ent: Entity) {
    if let Some(pile) = piles.0.get_mut(&looting.key()).filter(|p| p.locked_to == Some(ent)) {
        pile.locked_to = None;
    }
}

/// Takes what a player asks from the pile its window is open on, each
/// stack as far as the bag has room and weight for. A pile emptied frees
/// its slot and closes the window.
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
        let key = looting.key();
        let Some(pile) = piles.0.get_mut(&key).filter(|p| p.locked_to == Some(ent)) else { continue };
        let chosen: Vec<usize> = match entry {
            Some(e) if (e as usize) < pile.stacks.len() => vec![e as usize],
            Some(_) => continue,
            None => (0..pile.stacks.len()).collect(),
        };
        for i in chosen {
            let kind = pile.stacks[i].kind;
            let count = pile.stacks[i].count.min(bag.room_for(worn, kind));
            bag.add(common::Stack { kind, count });
            pile.stacks[i].count -= count;
        }
        pile.stacks.retain(|s| s.count > 0);
        let left = pile.stacks.clone();
        writer.write(Do { event: Event::Inventory { ent, bag: bag.clone() } });
        if !left.is_empty() {
            writer.write(Do { event: Event::Loot { ent, entries: Some(left) } });
            continue;
        }
        piles.0.remove(&key);
        if let Some(cover) = ground.cover(key.0, key.1) {
            ground.set(&mut writer, key.0, key.1, common::gathering::emptied(cover, key.2));
        }
        commands.entity(ent).remove::<Looting>();
        writer.write(Do { event: Event::Loot { ent, entries: None } });
        if !working {
            show(ent, None, &mut commands);
        }
    }
}

/// Closes each window its player asks to close, or whose player has moved
/// or turned at all since it opened, and unlocks the pile for anyone.
pub fn close_windows(
    mut commands: Commands,
    mut reader: MessageReader<Try>,
    mut writer: MessageWriter<Do>,
    mut piles: ResMut<Piles>,
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
        unlock(&mut piles, *looting, ent);
        commands.entity(ent).remove::<Looting>();
        writer.write(Do { event: Event::Loot { ent, entries: None } });
        if !working {
            show(ent, None, &mut commands);
        }
    }
}

/// Where a drop of `kind` lands on tile `(q, r)`, which holds `cover`: the
/// slot of a pile there already holding `kind` that `open` lets it onto,
/// or else the free slot nearest `from`, xz from the centre of tile
/// `frame`. The flag says it lands on a pile already lying there.
fn landing(
    cover: common::Cover,
    (q, r): (i32, i32),
    piles: &Piles,
    kind: common::Stackable,
    open: impl Fn((i32, i32, usize), &Pile) -> bool,
    frame: (i32, i32),
    from: Vec2,
) -> Option<(usize, bool)> {
    let slots = 0..common::TILE_SLOTS as usize;
    let onto = slots.clone().find(|&k| {
        piles.0.get(&(q, r, k)).is_some_and(|pile| pile.stacks.iter().any(|s| s.kind == kind) && open((q, r, k), pile))
    });
    let distance = |k: usize| Vec2::from(common_bevy::geometry::slot_point(cover, (q, r), k, frame)).distance_squared(from);
    let free = || slots.filter(|&k| cover.content(k) == common::Content::Empty).min_by(|&a, &b| distance(a).total_cmp(&distance(b)));
    onto.map(|k| (k, true)).or_else(|| free().map(|k| (k, false)))
}

/// Lays what a player drops from its bag on the tile it stands on: onto a
/// pile there of the same kind that no other player has open, or as a new
/// pile in the tile's free slot nearest the player. A tile with neither
/// takes nothing, and the bag keeps it.
pub fn try_drop(
    mut reader: MessageReader<Try>,
    mut writer: MessageWriter<Do>,
    mut ground: Ground,
    mut piles: ResMut<Piles>,
    mut players: Query<(&Loc, &Position, &mut Inventory, Option<&Looting>)>,
) {
    for message in reader.read() {
        let Try { event: Event::Drop { ent, kind, count } } = message else { continue };
        let (ent, kind, count) = (*ent, *kind, *count);
        let Ok((loc, position, bag, looting)) = players.get(ent) else { continue };
        if bag.count(kind) == 0 {
            continue;
        }
        let (q, r, looting) = (loc.q, loc.r, looting.copied());
        let Some(cover) = ground.cover(q, r) else {
            info!("drop: {ent} stands on ({q}, {r}), which the map does not hold");
            continue;
        };
        let open = |key, pile: &Pile| {
            held_from(pile, key, ent, |other| players.get(other).ok().and_then(|(.., l)| l.copied())).is_none()
        };
        let frame = (position.tile.q, position.tile.r);
        let Some((slot, onto)) = landing(cover, (q, r), &piles, kind, open, frame, position.offset.xz()) else {
            info!("drop: {ent} at ({q}, {r}) has no free slot and no pile of {kind:?} to drop onto");
            continue;
        };
        let Ok((_, _, mut bag, _)) = players.get_mut(ent) else { continue };
        let stack = bag.remove(kind, count);
        writer.write(Do { event: Event::Inventory { ent, bag: bag.clone() } });
        let key = (q, r, slot);
        let pile = piles.0.entry(key).or_insert(Pile { stacks: Vec::new(), locked_to: None });
        match pile.stacks.iter_mut().find(|s| s.kind == kind) {
            Some(held) => held.count += stack.count,
            None => pile.stacks.push(stack),
        }
        if looting.is_some_and(|l| l.key() == key) {
            writer.write(Do { event: Event::Loot { ent, entries: Some(pile.stacks.clone()) } });
        }
        if !onto {
            let common::Stackable::Material(material) = kind;
            ground.set(&mut writer, q, r, common::gathering::left(cover, slot, material));
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

    /// A drop goes onto a pile of its kind that lets it on, or else to the
    /// free slot nearest the player; a tile with neither takes nothing.
    #[test]
    fn a_drop_lands_on_its_pile_or_the_nearest_free_slot() {
        let softwood = common::Stackable::Material(common::Material::Softwood);
        let stone = common::Stackable::Material(common::Material::Limestone);
        let everyone = |_, _: &Pile| true;
        let tile = (3, -2);
        let cover = common::gathering::left(Cover::NONE.with_boulder(0), 4, common::Material::Softwood);
        let mut piles = Piles::default();
        piles.0.insert((3, -2, 4), Pile { stacks: vec![common::Stack { kind: softwood, count: 4 }], locked_to: None });

        assert_eq!(landing(cover, tile, &piles, softwood, everyone, tile, Vec2::ZERO), Some((4, true)));
        assert_eq!(landing(cover, tile, &piles, softwood, |_, _: &Pile| false, tile, Vec2::ZERO).map(|l| l.1), Some(false));

        let free = |k: usize| cover.content(k) == Content::Empty;
        for k in (0..common::TILE_SLOTS as usize).filter(|&k| free(k)) {
            let at = Vec2::from(common_bevy::geometry::slot_point(cover, tile, k, tile));
            assert_eq!(landing(cover, tile, &piles, stone, everyone, tile, at), Some((k, false)), "standing on free slot {k}");
        }

        let full = (0..common::TILE_SLOTS as usize).fold(cover, |c, k| if free(k) { c.with_boulder(k) } else { c });
        assert_eq!(landing(full, tile, &piles, stone, everyone, tile, Vec2::ZERO), None);
        assert_eq!(landing(full, tile, &piles, softwood, everyone, tile, Vec2::ZERO), Some((4, true)));
    }

    /// A clearing takes every tree and boulder within its radius, leaves
    /// a stump for each tree and no pile, and touches nothing past it.
    #[test]
    fn a_clearing_takes_what_stands_within_its_radius() {
        let wood = |_: i32, _: i32| Cover::NONE.with(0, Content::Pine).with_boulder(4).with_rock(common::Rock::Limestone);
        let changes = WorldChanges::clearing(wood, (10, -4), 2);
        assert_eq!(changes.tiles.len(), 19, "a radius of 2 is 19 tiles");
        assert_eq!(changes.fresh.len(), 19, "each one a change for the summaries to take");
        let cleared = changes.tiles[&(12, -6)];
        assert_eq!(cleared.content(common::SITE_SLOTS[0][0]), Content::PineStump);
        assert!((0..common::TILE_SLOTS as usize).all(|k| !cleared.content(k).is_pile() && common::gathering::harvest(cleared, k).is_none()));
        assert!(!changes.tiles.contains_key(&(13, -4)), "three tiles out is untouched");
    }
}
