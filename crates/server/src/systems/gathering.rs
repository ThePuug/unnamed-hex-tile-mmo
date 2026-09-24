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
    /// Tiles changed since the summaries were last revised.
    fresh: Vec<(i32, i32)>,
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

    /// The tiles changed since the last call.
    pub fn take_fresh(&mut self) -> Vec<(i32, i32)> {
        std::mem::take(&mut self.fresh)
    }

    fn set(&mut self, q: i32, r: i32, cover: common::Cover) {
        Arc::make_mut(&mut self.tiles).insert((q, r), cover);
        self.fresh.push((q, r));
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
        self.changes.set(q, r, cover);
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
            let held = pile.locked_to.filter(|&other| {
                other != ent && players.get(other).is_ok_and(|(_, _, _, _, looting, _)| looting.is_some_and(|l| l.key() == key))
            });
            if let Some(other) = held {
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
    /// a tile it does not sample leaves it be, one felled on a sample comes
    /// off its canopy, and a reading taken before the change keeps the
    /// changes as they stood.
    #[test]
    fn a_summary_reads_its_samples_as_players_left_them() {
        let r = 4;
        let pines = |cell: Option<common::summary::SummaryCell>| cell.expect("every sample is there").canopy.count(Content::Pine);
        let generated = pines(summarize(r, 0, 0, &Wood));
        let mut changes = WorldChanges::default();

        changes.set(1, 1, Cover::NONE);
        assert_eq!(pines(summarize(r, 0, 0, &changes.over(Wood))), generated, "no summary samples (1, 1)");

        let before = changes.over(Wood);
        let (dq, dr) = sample_offsets(r)[2];
        changes.set(dq, dr, Cover::NONE);
        assert_eq!(pines(summarize(r, 0, 0, &changes.over(Wood))), generated - 1, "the felled sample is off the canopy");
        assert_eq!(pines(summarize(r, 0, 0, &before)), generated, "a reading already out keeps what it set out with");
        assert_eq!(changes.take_fresh(), vec![(1, 1), (dq, dr)]);
        assert!(changes.take_fresh().is_empty());
    }
}
