//! Gathering: what a player gathers lies at once as a pile, locked to it
//! while its loot window is open on it; it takes from the pile into its
//! bag, and closing the window leaves the rest to anyone. Every tile changed keeps the change, and every client
//! holding the tile learns it.

use std::collections::HashMap;

use bevy::{ecs::system::SystemParam, prelude::*};
use common_bevy::{
    chunk::loc_to_chunk,
    components::{
        entity_type::{decorator::Decorator, EntityType},
        equipment::{Equipment, Inventory},
        heading::Heading,
        Loc,
    },
    message::{Do, Event, Try},
    resources::map::Map,
};
use qrz::Qrz;

use crate::systems::actor::VisibleChunkCache;

/// Every tile players have changed, as its cover now stands. A tile is its
/// generated cover with its change laid over: whatever builds a tile for
/// the map or the wire takes it through [`WorldChanges::laid_over`].
#[derive(Resource, Default)]
pub struct WorldChanges(HashMap<(i32, i32), common::Cover>);

impl WorldChanges {
    /// The tile `typ` at `qrz` as players have left it.
    pub fn laid_over(&self, qrz: Qrz, typ: EntityType) -> EntityType {
        match (typ, self.0.get(&(qrz.q, qrz.r))) {
            (EntityType::Decorator(d), Some(&cover)) => EntityType::Decorator(Decorator { cover, ..d }),
            _ => typ,
        }
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

/// The pile a player has its loot window open on. The pile is locked to it
/// while the window is open.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Looting {
    pub q: i32,
    pub r: i32,
    pub slot: usize,
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
        self.changes.0.insert((q, r), cover);
        let chunk = loc_to_chunk(qrz);
        for (holder, cache) in &self.holders {
            if cache.sent.contains(&chunk) {
                writer.write(Do { event: Event::CoverChanged { ent: holder, q, r, cover } });
            }
        }
    }
}

/// Applies each gather a player asks for: the slot must lie in its reach.
/// Something gatherable there is gathered, and its yield lies at once as a
/// pile in the slot the gather freed; a pile there opens. Either way the
/// pile is locked to the player and its window opens. A pile locked to
/// another player's open window does not open; one whose player has gone
/// or closed it does. A window already open closes first.
pub fn try_gather(
    mut commands: Commands,
    mut reader: MessageReader<Try>,
    mut writer: MessageWriter<Do>,
    mut ground: Ground,
    mut piles: ResMut<Piles>,
    players: Query<(&Loc, &Heading, Option<&Looting>)>,
) {
    for message in reader.read() {
        let Try { event: Event::Gather { ent, q, r, slot } } = message else { continue };
        let (ent, q, r, slot) = (*ent, *q, *r, *slot as usize);
        let Ok((loc, heading, open)) = players.get(ent) else {
            warn!("gather: {ent} is not a player");
            continue;
        };
        if !in_reach(**loc, *heading, q, r) || slot >= common::TILE_SLOTS as usize {
            info!("gather: {ent} at {:?} asked out of reach for slot {slot} of ({q}, {r})", **loc);
            continue;
        }
        let Some(cover) = ground.cover(q, r) else {
            info!("gather: {ent} asked for ({q}, {r}), which the map does not hold");
            continue;
        };
        let key = (q, r, slot);
        let (key, stacks) = if cover.content(slot).is_pile() {
            let Some(pile) = piles.0.get(&key) else { continue };
            let held = pile.locked_to.filter(|&other| {
                other != ent && players.get(other).is_ok_and(|(_, _, looting)| looting.is_some_and(|l| l.key() == key))
            });
            if let Some(other) = held {
                info!("gather: {ent} asked for the pile at slot {slot} of ({q}, {r}), which {other} has open");
                continue;
            }
            (key, pile.stacks.clone())
        } else if let Some(harvest) = common::gathering::harvest(cover, slot) {
            info!("gather: {ent} gathered {} {:?} from slot {slot} of ({q}, {r})", harvest.amount, harvest.material);
            let stacks = vec![harvest.stack()];
            ground.set(&mut writer, q, r, common::gathering::left(harvest.cover, harvest.freed, harvest.material));
            let key = (q, r, harvest.freed);
            piles.0.insert(key, Pile { stacks: stacks.clone(), locked_to: None });
            (key, stacks)
        } else {
            info!("gather: {ent} asked for slot {slot} of ({q}, {r}), which holds nothing gatherable");
            continue;
        };
        if let Some(open) = open.filter(|open| open.key() != key) {
            unlock(&mut piles, *open, ent);
        }
        if let Some(pile) = piles.0.get_mut(&key) {
            pile.locked_to = Some(ent);
        }
        commands.entity(ent).insert(Looting { q: key.0, r: key.1, slot: key.2 });
        writer.write(Do { event: Event::Loot { ent, entries: Some(stacks) } });
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
    mut players: Query<(&Equipment, &mut Inventory, &Looting)>,
) {
    for message in reader.read() {
        let Try { event: Event::Take { ent, entry } } = message else { continue };
        let (ent, entry) = (*ent, *entry);
        let Ok((worn, mut bag, looting)) = players.get_mut(ent) else { continue };
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
    }
}

/// Closes each window its player asks to close, or whose pile has left the
/// player's reach by a turn or a step, and unlocks the pile for anyone.
pub fn close_windows(
    mut commands: Commands,
    mut reader: MessageReader<Try>,
    mut writer: MessageWriter<Do>,
    mut piles: ResMut<Piles>,
    players: Query<(Entity, &Loc, &Heading, &Looting)>,
) {
    let asked: Vec<Entity> = reader
        .read()
        .filter_map(|m| match m.event {
            Event::CloseLoot { ent } => Some(ent),
            _ => None,
        })
        .collect();
    for (ent, loc, heading, looting) in &players {
        if !asked.contains(&ent) && in_reach(**loc, *heading, looting.q, looting.r) {
            continue;
        }
        unlock(&mut piles, *looting, ent);
        commands.entity(ent).remove::<Looting>();
        writer.write(Do { event: Event::Loot { ent, entries: None } });
    }
}
