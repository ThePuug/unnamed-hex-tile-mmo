//! Cartography: a map held in the main hand charts the ground round its
//! holder, as far as the holder sees other actors. A map is a piece of
//! the kit's map kind and one of a kind, known by its own number
//! (`Item::map`): [`Charts`] holds what each number has charted.
//!
//! A map is a snapshot. Each chunk it holds is that chunk as it lay when
//! the map was last carried past it, and nothing changes it since but
//! being carried past again. Beside the maps, [`Charts`] holds the ground
//! as it lies now, for every chunk any map was ever charted from, kept up
//! as tiles change: what a way found on a map is tried against
//! (`common_bevy::chart`).
//!
//! A player draws a blank map of fibre, or a copy of the map in its
//! hand: the copy is what that map is as it is copied, and each charts
//! on alone.
//!
//! Every map is recalled as the server starts and held whole. Unbuilt:
//! recalling a map only once something reads it.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use bevy::prelude::*;
use common_bevy::{
    chart::{Chart, Leaf},
    chunk::{calculate_visible_chunks, loc_to_chunk, ChunkId, FOV_CHUNK_RADIUS},
    components::{
        behaviour::PlayerControlled,
        equipment::{Equipment, Inventory, Item, Piece, Slot, BAG_STACKS},
        Loc,
    },
    message::{Do, Event, Try},
    resources::map::Map,
};
use qrz::Qrz;

use crate::systems::{
    characters::Character,
    economy::{Cause, Holder, Moved, Thing},
    gathering::TileChanged,
};

/// How many chunks out from its holder's a map charts: the reach a
/// player sees other actors at (`aoi::AOI_RADIUS`).
const REACH: u8 = FOV_CHUNK_RADIUS + 1;

/// The number of the map `worn` has out, in its main hand.
pub fn held(worn: &Equipment) -> Option<u32> {
    worn.worn(Slot::MainHand).and_then(Item::map)
}

/// The number the ground as it lies now is kept under, as a map is
/// under its own: no map's.
pub const LIES: u32 = 0;

/// What every map has charted, by its number, and the ground as it lies
/// now.
#[derive(Resource, Default)]
pub struct Charts {
    maps: HashMap<u32, Chart>,
    lies: Chart,
    /// The numbers this server may still give a map: the next, and the
    /// one past the last.
    numbers: (u32, u32),
    /// Each leaf laid since the last call of [`Charts::take_changed`], by
    /// its map and chunk, and each map spent.
    changed: HashSet<(u32, ChunkId)>,
    spent: Vec<u32>,
}

impl Charts {
    /// Hands the server `run`, numbers no map has: the next maps made
    /// take them in turn.
    pub fn numbered(&mut self, run: std::ops::Range<u32>) {
        self.numbers = (run.start, run.end);
    }

    /// How many numbers the server still has to give.
    pub fn numbers_left(&self) -> u32 {
        self.numbers.1 - self.numbers.0
    }

    /// A new map, with nothing charted: its number, or none where the
    /// server has no number left to give.
    pub fn draw(&mut self) -> Option<u32> {
        let number = self.numbers.0;
        (number < self.numbers.1).then(|| {
            self.numbers.0 += 1;
            number
        })
    }

    /// Lays a leaf kept from an earlier run on map `number`, or on the
    /// ground as it lies now where the number is [`LIES`].
    pub fn recall(&mut self, number: u32, leaf: Leaf) {
        let chart = if number == LIES { &mut self.lies } else { self.maps.entry(number).or_default() };
        chart.lay(Arc::new(leaf));
    }

    /// Map `number` is used up: what it charted is forgotten.
    pub fn spend(&mut self, number: u32) {
        self.maps.remove(&number);
        self.changed.retain(|&(map, _)| map != number);
        self.spent.push(number);
    }

    /// Each leaf laid since the last call, by its map and chunk, the
    /// ground as it lies now under [`LIES`]; and each map spent.
    pub fn take_changed(&mut self) -> (Vec<(u32, ChunkId)>, Vec<u32>) {
        (self.changed.drain().collect(), std::mem::take(&mut self.spent))
    }

    /// Chunk `chunk` of map `number` as a store keeps it, or of the
    /// ground as it lies now where the number is [`LIES`].
    pub fn kept(&self, number: u32, chunk: ChunkId) -> Option<Vec<u8>> {
        let chart = if number == LIES { &self.lies } else { self.maps.get(&number)? };
        chart.leaf(chunk).map(|leaf| leaf.kept())
    }

    /// What map `number` has charted; none where it has charted nothing.
    pub fn map(&self, number: u32) -> Option<&Chart> {
        self.maps.get(&number)
    }

    /// The ground as it lies now, of every chunk a map was charted from.
    pub fn lies(&self) -> &Chart {
        &self.lies
    }

    /// Makes map `copy` what map `of` is now. What either charts after
    /// leaves the other as it was.
    pub fn copy(&mut self, of: u32, copy: u32) {
        let chart = self.maps.get(&of).cloned().unwrap_or_default();
        self.changed.extend(chart.leaves().map(|leaf| (copy, leaf.chunk())));
        self.maps.insert(copy, chart);
    }

    /// Charts onto map `number` every chunk within a holder's reach of
    /// `around` that `map` holds whole, each as it lies now. Returns how
    /// many it charted anew or over.
    pub fn chart(&mut self, number: u32, map: &Map, around: ChunkId) -> usize {
        let mut charted = 0;
        for chunk in calculate_visible_chunks(around, REACH) {
            let lies = match self.lies.leaf(chunk) {
                Some(lies) => lies.clone(),
                None => {
                    let Some(leaf) = Leaf::read(map, chunk) else { continue };
                    let leaf = Arc::new(leaf);
                    self.lies.lay(leaf.clone());
                    self.changed.insert((LIES, chunk));
                    leaf
                }
            };
            let chart = self.maps.entry(number).or_default();
            if !chart.leaf(chunk).is_some_and(|held| Arc::ptr_eq(held, &lies) || **held == *lies) {
                chart.lay(lies);
                self.changed.insert((number, chunk));
                charted += 1;
            }
        }
        charted
    }

    /// Reads `chunk` again as it lies now, where any map was charted
    /// from it and `map` holds it whole. Every map keeps it as charted.
    fn relay(&mut self, map: &Map, chunk: ChunkId) {
        let Some(leaf) = self.lies.leaf(chunk).and_then(|_| Leaf::read(map, chunk)) else { return };
        if self.lies.leaf(chunk).is_some_and(|lies| **lies != leaf) {
            self.lies.lay(Arc::new(leaf));
            self.changed.insert((LIES, chunk));
        }
    }
}

/// How often a map in hand charts what is round it.
const CHARTS: Duration = Duration::from_secs(1);

/// Charts the ground round every player holding a map onto that map.
pub fn chart(mut charts: ResMut<Charts>, map: Res<Map>, time: Res<Time>, holders: Query<(&Loc, &Equipment), With<PlayerControlled>>, mut next: Local<Duration>) {
    if time.elapsed() < *next {
        return;
    }
    *next = time.elapsed() + CHARTS;
    for (loc, worn) in &holders {
        let Some(number) = held(worn) else { continue };
        let charted = charts.chart(number, &map, loc_to_chunk(**loc));
        if charted > 0 {
            debug!("charts: map {number} charted {charted} chunks round ({}, {})", loc.q, loc.r);
        }
    }
}

/// The piece a map is, where the kit has one.
fn map_piece() -> Option<Piece> {
    Piece::all().find(|piece| piece.set().kind == common::kit::Kind::Map)
}

/// Answers each player that draws a map: a blank one, or a copy of the
/// map in its main hand, for the fibre a map takes, put in its bag. One
/// whose bag lacks the fibre or the room, or that copies with no map in
/// hand, draws nothing.
pub fn try_draw(
    mut reader: MessageReader<Try>,
    mut charts: ResMut<Charts>,
    mut players: Query<(&mut Inventory, &Equipment, &Character)>,
    mut writer: MessageWriter<Do>,
    mut moved: MessageWriter<Moved>,
) {
    for message in reader.read() {
        let Try { event: Event::Draw { ent, copy } } = message else { continue };
        let (ent, copy) = (*ent, *copy);
        let Ok((mut bag, worn, &Character(who))) = players.get_mut(ent) else { continue };
        let (Some(piece), cost) = (map_piece(), common::building::MAP_COST) else { continue };
        let of = held(worn);
        // The fibre spent frees its stack only where it is all the bag holds
        let room = bag.stacks(worn) < BAG_STACKS || bag.count(cost.kind) == cost.count;
        if bag.count(cost.kind) < cost.count || !room || (copy && of.is_none()) {
            info!("charts: {ent} cannot draw a map");
            continue;
        }
        let Some(number) = charts.draw() else {
            warn!("charts: no number is left for the map {ent} would draw");
            continue;
        };
        if let Some(of) = of.filter(|_| copy) {
            charts.copy(of, number);
        }
        let map = Item { piece, seed: (number % piece.set().seeds.len() as u32) as u16, instance: Some(number) };
        let spent = bag.remove(cost.kind, cost.count);
        bag.items.push(map);
        moved.write(Moved::stack(spent, Holder::Character(who), Holder::World, Cause::Draw));
        moved.write(Moved { what: Thing::Piece(map), count: 1, from: Holder::World, to: Holder::Character(who), cause: Cause::Draw });
        writer.write(Do { event: Event::Inventory { ent, bag: bag.clone() } });
        info!("charts: {ent} drew map {number}{}", if copy { ", a copy of the one in its hand" } else { "" });
    }
}

/// Keeps the ground as it lies now up with every tile that changes.
pub fn relay(mut charts: ResMut<Charts>, map: Res<Map>, mut changed: MessageReader<TileChanged>, mut laid: MessageReader<Do>) {
    let tiles = changed.read().map(|tile| (tile.q, tile.r)).chain(laid.read().filter_map(|message| match message.event {
        Event::CoverChanged { q, r, .. } => Some((q, r)),
        _ => None,
    }));
    let chunks: HashSet<ChunkId> = tiles.map(|(q, r)| loc_to_chunk(Qrz { q, r, z: 0 })).collect();
    for chunk in chunks {
        charts.relay(&map, chunk);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common_bevy::{
        chunk::chunk_tiles,
        components::entity_type::{decorator::Decorator, EntityType},
    };

    fn lay(map: &Map, chunk: ChunkId, cover: common::Cover) {
        for (q, r) in chunk_tiles(chunk) {
            map.insert(Qrz { q, r, z: 0 }, EntityType::Decorator(Decorator { cover, is_solid: true }));
        }
    }

    fn ground(reach: u8) -> Map {
        let map = Map::new(qrz::Map::new(1.0, 0.8));
        for chunk in calculate_visible_chunks(ChunkId(0, 0), reach) {
            lay(&map, chunk, common::Cover::NONE);
        }
        map
    }

    fn full() -> common::Cover {
        (0..common::TILE_SLOTS as usize).fold(common::Cover::NONE, |c, k| c.with_boulder(k))
    }

    /// A map charts every chunk in its holder's reach that the ground
    /// holds whole, once, and more as it is carried on.
    #[test]
    fn a_map_charts_its_holders_reach() {
        let map = ground(REACH + 1);
        let mut charts = Charts::default();
        let reach = calculate_visible_chunks(ChunkId(0, 0), REACH).len();
        assert_eq!(charts.chart(7, &map, ChunkId(0, 0)), reach);
        assert_eq!(charts.chart(7, &map, ChunkId(0, 0)), 0, "what is charted and unchanged is not charted again");
        assert_eq!(charts.map(7).map(Chart::len), Some(reach));
        let further = charts.chart(7, &map, ChunkId(1, 0));
        assert!(further > 0 && further < reach, "carried a chunk on it charts the new edge, as far as the ground is held");
        assert!(charts.map(8).is_none());
    }

    /// A chunk the ground holds only part of is left off the map.
    #[test]
    fn a_map_charts_only_whole_chunks() {
        let map = ground(0);
        map.insert(Qrz { q: ChunkId(1, 0).center().q, r: ChunkId(1, 0).center().r, z: 0 }, EntityType::Decorator(Decorator { cover: common::Cover::NONE, is_solid: true }));
        let mut charts = Charts::default();
        assert_eq!(charts.chart(1, &map, ChunkId(0, 0)), 1);
        assert!(charts.map(1).unwrap().leaf(ChunkId(1, 0)).is_none());
    }

    /// A map is a snapshot: ground that changes after it was charted
    /// leaves the map and its copies as they were, until one is carried
    /// past again, and then only that one.
    #[test]
    fn a_map_is_as_it_was_charted() {
        let map = ground(REACH);
        let mut charts = Charts::default();
        charts.chart(1, &map, ChunkId(0, 0));
        charts.copy(1, 2);
        let was = charts.map(1).unwrap().leaf(ChunkId(0, 0)).unwrap().clone();

        lay(&map, ChunkId(0, 0), full());
        charts.relay(&map, ChunkId(0, 0));
        assert_ne!(**charts.lies().leaf(ChunkId(0, 0)).unwrap(), *was, "the ground as it lies has changed");
        assert_eq!(charts.map(1).unwrap().leaf(ChunkId(0, 0)), Some(&was), "and the map has not");

        assert_eq!(charts.chart(1, &map, ChunkId(0, 0)), 1, "carried past again it charts the one chunk over");
        assert_ne!(charts.map(1).unwrap().leaf(ChunkId(0, 0)), Some(&was));
        assert_eq!(charts.map(2).unwrap().leaf(ChunkId(0, 0)), Some(&was), "its copy stays as copied");
    }

    /// A server gives each map it makes the next of its numbers, and
    /// none once they are out; what is charted, copied and spent is
    /// what the store is told of.
    #[test]
    fn maps_are_numbered_and_their_changes_kept() {
        let map = ground(0);
        let mut charts = Charts::default();
        assert_eq!(charts.draw(), None);
        charts.numbered(1024..1026);
        let (first, second) = (charts.draw().unwrap(), charts.draw().unwrap());
        assert_eq!((first, second, charts.draw()), (1024, 1025, None));

        charts.chart(first, &map, ChunkId(0, 0));
        charts.copy(first, second);
        let (mut laid, spent) = charts.take_changed();
        laid.sort_by_key(|&(number, _)| number);
        assert_eq!((laid, spent), (vec![(LIES, ChunkId(0, 0)), (first, ChunkId(0, 0)), (second, ChunkId(0, 0))], vec![]));
        let kept = charts.kept(second, ChunkId(0, 0)).unwrap();
        assert_eq!(Some(kept.clone()), charts.kept(LIES, ChunkId(0, 0)));

        charts.spend(second);
        assert_eq!(charts.take_changed(), (vec![], vec![second]));
        assert!(charts.map(second).is_none());

        let mut again = Charts::default();
        again.recall(first, Leaf::recalled(ChunkId(0, 0), &kept).unwrap());
        again.recall(LIES, Leaf::recalled(ChunkId(0, 0), &kept).unwrap());
        assert_eq!(again.chart(first, &map, ChunkId(0, 0)), 0, "a map recalled is as it was charted");
    }

    /// A chunk no map was charted from is not read as the ground changes.
    #[test]
    fn only_charted_ground_is_kept_up() {
        let map = ground(0);
        let mut charts = Charts::default();
        charts.relay(&map, ChunkId(0, 0));
        assert!(charts.lies().is_empty());
    }
}
