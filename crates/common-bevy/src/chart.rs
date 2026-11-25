//! The ground as a map holds it, and the way a cart takes over it. A
//! [`Leaf`] is one chunk as it stood when it was charted: every tile's
//! level and whether it was full, and where a hub's hearth stood. A
//! [`Chart`] is the leaves a map holds,
//! and [`Chart::way`] the way over them from one tile to another, by the
//! steps a walker takes ([`crate::path`]): to a neighbour no more than a
//! level up or down and not full.
//!
//! A way is searched a chunk at a time and never a tile at a time. Within
//! a leaf the tiles that reach each other are one part; a way crosses
//! from a part of one leaf to a part of the next by one gate, the legal
//! step between them nearest the middle of the side they share. The
//! search is over parts alone ([`route`]): the fewest chunks crossed,
//! knowing only which part joins which. Whoever holds that much of a
//! chart, with none of its tiles, finds the same chunks
//! (`crate::picture::Atlas`). The steps inside each chunk are filled in
//! after, so a way is a [`Leg`] to a chunk. A chart is what was charted and stays so: a
//! way found on one is tried against another, the ground as it lies now,
//! a leg at a time ([`Chart::open`]).

use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashMap, VecDeque},
    sync::Arc,
};

use qrz::Qrz;
use serde::{Deserialize, Serialize};

use crate::{
    chunk::{chunk_tiles, loc_to_chunk, ChunkId, CHUNK_RADIUS, CHUNK_TILES},
    resources::map::Map,
    systems::movement::is_full_cover,
};

type Tile = (i32, i32);

/// A tile's place in its chunk, in the order [`chunk_tiles`] yields them;
/// none where `tile` is not of the chunk centred on `centre`.
pub(crate) fn place(centre: Qrz, tile: Tile) -> Option<usize> {
    let (dq, dr) = (tile.0 - centre.q, tile.1 - centre.r);
    let r = CHUNK_RADIUS;
    if dq.abs() > r || dr.abs() > r || (dq + dr).abs() > r {
        return None;
    }
    let before: i32 = (-r..dq).map(|row| 2 * r + 1 - row.abs()).sum();
    Some((before + dr - (-r).max(-dq - r)) as usize)
}

/// How many bytes of a kept leaf are its ground ([`Leaf::kept`]).
const GROUND_KEPT: usize = 4 + 2 * CHUNK_TILES + CHUNK_TILES.div_ceil(8);

/// One chunk as charted: each tile's level and whether it is full,
/// which tiles reach each other inside it, and where a hearth stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Leaf {
    chunk: ChunkId,
    /// Each tile's level and whether it is full, in [`chunk_tiles`] order.
    tiles: Vec<(i32, bool)>,
    /// Each tile's part: tiles of one part reach each other by a walker's
    /// steps without leaving the chunk. A full tile is of none.
    parts: Vec<Option<u16>>,
    /// The part most of its open tiles are of, the lowest where two tie;
    /// none where every tile is full.
    main: Option<u16>,
    /// The place of each tile a hub's hearth stands on, rising.
    hearths: Vec<u16>,
}

impl Leaf {
    /// The chunk as `tiles` say it lies, each a level and whether it is
    /// full, in [`chunk_tiles`] order, a hearth standing at each of the
    /// places `hearths` names. None where they are not a chunk's.
    pub fn of(chunk: ChunkId, tiles: Vec<(i32, bool)>, mut hearths: Vec<u16>) -> Option<Leaf> {
        if tiles.len() != CHUNK_TILES || hearths.iter().any(|&at| at as usize >= CHUNK_TILES) {
            return None;
        }
        hearths.sort();
        hearths.dedup();
        let centre = chunk.center();
        let mut parts: Vec<Option<u16>> = vec![None; CHUNK_TILES];
        let mut next = 0;
        for (at, tile) in chunk_tiles(chunk).enumerate() {
            if tiles[at].1 || parts[at].is_some() {
                continue;
            }
            parts[at] = Some(next);
            let mut open = vec![(at, tile)];
            while let Some((at, tile)) = open.pop() {
                for d in qrz::DIRECTIONS.iter() {
                    let beside = (tile.0 + d.q, tile.1 + d.r);
                    let Some(to) = place(centre, beside) else { continue };
                    if tiles[to].1 || parts[to].is_some() || (tiles[to].0 - tiles[at].0).abs() > 1 {
                        continue;
                    }
                    parts[to] = Some(next);
                    open.push((to, beside));
                }
            }
            next += 1;
        }
        let mut sizes = vec![0u32; next as usize];
        for part in parts.iter().flatten() {
            sizes[*part as usize] += 1;
        }
        let main = (0..next).max_by_key(|&part| (sizes[part as usize], std::cmp::Reverse(part)));
        Some(Leaf { chunk, tiles, parts, main, hearths })
    }

    /// The part most of the chunk's open tiles are of; none where every
    /// tile is full.
    pub fn main(&self) -> Option<u16> {
        self.main
    }

    /// The chunk as `map` holds it now, or none while the map lacks any
    /// tile of it.
    pub fn read(map: &Map, chunk: ChunkId) -> Option<Leaf> {
        let tiles = chunk_tiles(chunk).map(|(q, r)| Some((map.get_by_qr(q, r)?.0.z, is_full_cover(map, q, r)))).collect::<Option<Vec<_>>>()?;
        let hearth = |&(_, (q, r)): &(usize, Tile)| common::building::standing(map.cover_at(q, r)).is_some_and(|construct| construct.kind == common::building::Kind::Hearth);
        Leaf::of(chunk, tiles, chunk_tiles(chunk).enumerate().filter(hearth).map(|(at, _)| at as u16).collect())
    }

    /// Each tile of the chunk a hub's hearth stood on as it was charted.
    pub fn hearths(&self) -> impl Iterator<Item = Tile> + '_ {
        chunk_tiles(self.chunk).enumerate().filter(|(at, _)| self.hearths.binary_search(&(*at as u16)).is_ok()).map(|(_, tile)| tile)
    }

    pub fn chunk(&self) -> ChunkId {
        self.chunk
    }

    /// The leaf as a store keeps it: the first tile's level, every
    /// tile's level from that, a bit to each tile that is full, then how
    /// many hearths and the place of each.
    pub fn kept(&self) -> Vec<u8> {
        let base = self.tiles[0].0;
        let mut kept = base.to_le_bytes().to_vec();
        for &(level, _) in &self.tiles {
            kept.extend(((level - base).clamp(i16::MIN as i32, i16::MAX as i32) as i16).to_le_bytes());
        }
        let mut full = vec![0u8; CHUNK_TILES.div_ceil(8)];
        for (at, _) in self.tiles.iter().enumerate().filter(|(_, tile)| tile.1) {
            full[at / 8] |= 1 << (at % 8);
        }
        kept.extend(full);
        kept.extend((self.hearths.len() as u16).to_le_bytes());
        kept.extend(self.hearths.iter().flat_map(|at| at.to_le_bytes()));
        kept
    }

    /// Whether `kept` keeps a leaf's ground and says nothing of its
    /// hearths, not even that it has none: it is recalled with none, and
    /// is [`Leaf::marked`] by whoever knows where they stand.
    pub fn unmarked(kept: &[u8]) -> bool {
        kept.len() == GROUND_KEPT
    }

    /// The leaf with a hearth on each of `hearths` that is a tile of its
    /// chunk, beside those it holds.
    pub fn marked(&self, hearths: impl Iterator<Item = Tile>) -> Leaf {
        let centre = self.chunk.center();
        let mut leaf = self.clone();
        leaf.hearths.extend(hearths.filter_map(|tile| place(centre, tile)).map(|at| at as u16));
        leaf.hearths.sort();
        leaf.hearths.dedup();
        leaf
    }

    /// The leaf of `chunk` that `kept` keeps ([`Leaf::kept`]); none where
    /// it is no leaf.
    pub fn recalled(chunk: ChunkId, kept: &[u8]) -> Option<Leaf> {
        let (ground, hearths) = kept.split_at_checked(GROUND_KEPT)?;
        let hearths: Vec<u16> = hearths.chunks(2).map(|at| at.try_into().map(u16::from_le_bytes)).collect::<Result<_, _>>().ok()?;
        if hearths.first().is_some_and(|&count| count as usize != hearths.len() - 1) {
            return None;
        }
        let base = i32::from_le_bytes(ground[..4].try_into().ok()?);
        let (levels, full) = ground[4..].split_at(2 * CHUNK_TILES);
        let tiles = (0..CHUNK_TILES).map(|at| (base + i16::from_le_bytes([levels[2 * at], levels[2 * at + 1]]) as i32, full[at / 8] >> (at % 8) & 1 == 1)).collect();
        Leaf::of(chunk, tiles, hearths.into_iter().skip(1).collect())
    }

    /// Each tile's level and whether it is full, in [`chunk_tiles`] order:
    /// what [`Leaf::of`] takes.
    pub fn tiles(&self) -> &[(i32, bool)] {
        &self.tiles
    }

    /// The level of `tile` and the part it is of, where it is of this
    /// chunk and not full.
    fn stood(&self, tile: Tile) -> Option<(i32, u16)> {
        let at = place(self.chunk.center(), tile)?;
        Some((self.tiles[at].0, self.parts[at]?))
    }

    /// How many steps each tile of the chunk is from `from` without
    /// leaving it, in [`chunk_tiles`] order; none where it has no way.
    fn steps(&self, from: Tile) -> Vec<Option<u32>> {
        let centre = self.chunk.center();
        let mut steps = vec![None; CHUNK_TILES];
        let Some(at) = place(centre, from).filter(|&at| !self.tiles[at].1) else { return steps };
        steps[at] = Some(0);
        let mut open = VecDeque::from([(at, from, 0)]);
        while let Some((at, tile, taken)) = open.pop_front() {
            for d in qrz::DIRECTIONS.iter() {
                let beside = (tile.0 + d.q, tile.1 + d.r);
                let Some(to) = place(centre, beside) else { continue };
                if self.tiles[to].1 || steps[to].is_some() || (self.tiles[to].0 - self.tiles[at].0).abs() > 1 {
                    continue;
                }
                steps[to] = Some(taken + 1);
                open.push_back((to, beside, taken + 1));
            }
        }
        steps
    }

    /// The tiles of a shortest walk inside the chunk from `from` to
    /// `to`, both ends in it; none where the chunk gives no way.
    fn walk(&self, from: Tile, to: Tile) -> Option<Vec<Tile>> {
        let centre = self.chunk.center();
        let steps = self.steps(to);
        let left = |tile: Tile| place(centre, tile).and_then(|at| Some((steps[at]?, self.tiles[at].0)));
        let (mut left_here, mut level) = left(from)?;
        let mut walk = vec![from];
        while left_here > 0 {
            let here = walk[walk.len() - 1];
            let (next, onto) = qrz::DIRECTIONS.iter().map(|d| (here.0 + d.q, here.1 + d.r)).find_map(|beside| {
                let (left, onto) = left(beside)?;
                (left + 1 == left_here && (onto - level).abs() <= 1).then_some((beside, onto))
            })?;
            walk.push(next);
            (left_here, level) = (left_here - 1, onto);
        }
        Some(walk)
    }
}

/// The chunks beside a chunk, as steps of its number, each across from
/// the one three on: the sides a part is joined through.
pub const SIDES: [(i32, i32); 6] = [(1, 0), (0, 1), (-1, 1), (-1, 0), (0, -1), (1, -1)];

/// Which side of `chunk` the chunk `beside` is on; none where it is not
/// beside it.
pub fn side(chunk: ChunkId, beside: ChunkId) -> Option<usize> {
    SIDES.iter().position(|&(n, m)| (chunk.0 + n, chunk.1 + m) == (beside.0, beside.1))
}

/// A part of a chunk: what a way is searched over.
pub type Node = (ChunkId, u16);

/// The parts a way from `from` to `to` crosses, both ends among them,
/// where `next` gives the parts each is joined to: the fewest there can
/// be, and of those the ones met first, nearest `to` first. None where
/// nothing joins them. The same parts and the same `next`, in the same
/// order, give the same way, whoever asks.
pub fn route(from: Node, to: Node, mut next: impl FnMut(Node) -> Vec<Node>) -> Option<Vec<Node>> {
    let goal = to.0.center();
    let key = |node: Node| {
        let centre = node.0.center();
        (qrz::hex_distance((centre.q, centre.r), (goal.q, goal.r)), node.0 .0, node.0 .1, node.1)
    };
    let mut came: HashMap<Node, (u32, Option<Node>)> = HashMap::from([(from, (0, None))]);
    let mut open = BinaryHeap::from([Reverse((0u32, key(from)))]);
    while let Some(Reverse((crossed, (_, n, m, part)))) = open.pop() {
        let at = (ChunkId(n, m), part);
        if came.get(&at).is_some_and(|&(best, _)| best < crossed) {
            continue;
        }
        if at == to {
            let mut way = vec![at];
            while let Some(back) = came[&way[way.len() - 1]].1 {
                way.push(back);
            }
            way.reverse();
            return Some(way);
        }
        for onto in next(at) {
            if came.get(&onto).is_some_and(|&(best, _)| best <= crossed + 1) {
                continue;
            }
            came.insert(onto, (crossed + 1, Some(at)));
            open.push(Reverse((crossed + 1, key(onto))));
        }
    }
    None
}

/// One stretch of a way, over one chunk: from `from` to `by` inside it
/// and then the one step onto `to`, the first tile of the next. The last
/// leg ends inside its chunk, `by` and `to` the same tile. `steps` is how
/// many it takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Leg {
    pub from: Tile,
    pub by: Tile,
    pub to: Tile,
    pub steps: u32,
}

/// `way` gone the other way, from its end to its start: a leg to each
/// chunk still, each walked inside its chunk and then stepped out of.
pub fn reversed(way: &[Leg]) -> Vec<Leg> {
    let last = way.len().saturating_sub(1);
    (0..way.len())
        .rev()
        .map(|k| {
            let walked = way[k].steps - (k < last) as u32;
            let to = if k > 0 { way[k - 1].by } else { way[k].from };
            Leg { from: way[k].by, by: way[k].from, to, steps: walked + (k > 0) as u32 }
        })
        .collect()
}

/// The chunks a map holds, each as it was charted.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Chart {
    leaves: HashMap<ChunkId, Arc<Leaf>>,
}

impl Chart {
    /// Charts `leaf`, over what the chart held of its chunk.
    pub fn lay(&mut self, leaf: Arc<Leaf>) {
        self.leaves.insert(leaf.chunk, leaf);
    }

    pub fn leaf(&self, chunk: ChunkId) -> Option<&Arc<Leaf>> {
        self.leaves.get(&chunk)
    }

    pub fn leaves(&self) -> impl Iterator<Item = &Arc<Leaf>> {
        self.leaves.values()
    }

    /// Each tile a hub's hearth stood on as its chunk was charted.
    pub fn hearths(&self) -> impl Iterator<Item = Tile> + '_ {
        self.leaves.values().flat_map(|leaf| leaf.hearths())
    }

    pub fn len(&self) -> usize {
        self.leaves.len()
    }

    pub fn is_empty(&self) -> bool {
        self.leaves.is_empty()
    }

    fn leaf_of(&self, tile: Tile) -> Option<&Arc<Leaf>> {
        self.leaves.get(&loc_to_chunk(Qrz { q: tile.0, r: tile.1, z: 0 }))
    }

    /// The level of `tile` and whether it is full, as charted; none where
    /// the chart does not hold it.
    pub fn tile(&self, tile: Tile) -> Option<(i32, bool)> {
        let leaf = self.leaf_of(tile)?;
        place(leaf.chunk.center(), tile).map(|at| leaf.tiles[at])
    }

    /// `tile` where the chart holds it open, or else the first tile
    /// beside it that is: where a way to something full, a hearth, ends.
    pub fn beside(&self, tile: Tile) -> Option<Tile> {
        let stood = |tile: Tile| self.leaf_of(tile).and_then(|leaf| leaf.stood(tile)).map(|_| tile);
        stood(tile).or_else(|| qrz::DIRECTIONS.iter().find_map(|d| stood((tile.0 + d.q, tile.1 + d.r))))
    }

    /// Whether the chart holds every step of `leg`: its ends inside the
    /// chunk reach each other there, and the step out of it is a walker's.
    /// A way found on one chart is tried on another by this.
    pub fn open(&self, leg: Leg) -> bool {
        let Some(leaf) = self.leaf_of(leg.from) else { return false };
        let (Some((_, part)), Some((level, left))) = (leaf.stood(leg.from), leaf.stood(leg.by)) else { return false };
        if part != left {
            return false;
        }
        leg.by == leg.to
            || (qrz::hex_distance(leg.by, leg.to) == 1
                && self.leaf_of(leg.to).and_then(|next| next.stood(leg.to)).is_some_and(|(onto, _)| (onto - level).abs() <= 1))
    }

    /// The tiles `leg` crosses, from its `from` to its `to`: a shortest
    /// walk inside its chunk, then the step out. Only the leg's ends
    /// where the chart no longer holds the walk.
    pub fn walk(&self, leg: Leg) -> Vec<Tile> {
        let mut walk = self.leaf_of(leg.from).and_then(|leaf| leaf.walk(leg.from, leg.by)).unwrap_or_else(|| vec![leg.from, leg.by]);
        if leg.to != leg.by {
            walk.push(leg.to);
        }
        walk
    }

    /// The part of its chunk `tile` is of, where the chart holds it open.
    pub fn node(&self, tile: Tile) -> Option<Node> {
        let leaf = self.leaf_of(tile)?;
        Some((leaf.chunk, leaf.stood(tile)?.1))
    }

    /// The gates out of `leaf`: for each of its parts and each part of a
    /// leaf beside it that a step joins, the one such step nearest the
    /// middle of the side the two chunks share: the part left, the side
    /// crossed, the part come into, the tile left and the tile stepped
    /// onto; by part, side and part come into.
    fn gates(&self, leaf: &Leaf) -> Vec<(u16, usize, u16, Tile, Tile)> {
        let centre = leaf.chunk.center();
        let mut gates: HashMap<(u16, usize, u16), (i32, Tile, Tile)> = HashMap::new();
        for tile in chunk_tiles(leaf.chunk).filter(|&tile| qrz::hex_distance(tile, (centre.q, centre.r)) == CHUNK_RADIUS) {
            let Some((level, part)) = leaf.stood(tile) else { continue };
            for d in qrz::DIRECTIONS.iter() {
                let onto = (tile.0 + d.q, tile.1 + d.r);
                if place(centre, onto).is_some() {
                    continue;
                }
                let Some(next) = self.leaf_of(onto) else { continue };
                let Some(crossed) = side(leaf.chunk, next.chunk) else { continue };
                let Some((_, into)) = next.stood(onto).filter(|&(z, _)| (z - level).abs() <= 1) else { continue };
                // Twice the tile less the two centres: how far it is from
                // the middle of their side, with no half to round
                let other = next.chunk.center();
                let off = qrz::hex_distance((2 * tile.0, 2 * tile.1), (centre.q + other.q, centre.r + other.r));
                let found = (off, tile, onto);
                gates.entry((part, crossed, into)).and_modify(|held| *held = (*held).min(found)).or_insert(found);
            }
        }
        let mut gates: Vec<(u16, usize, u16, Tile, Tile)> = gates.into_iter().map(|((part, crossed, into), (_, by, to))| (part, crossed, into, by, to)).collect();
        gates.sort();
        gates
    }

    /// Which part of `leaf` joins which part of each chunk beside it:
    /// the part, the side, and the part of the chunk on that side; by
    /// part, side and part joined.
    pub fn links(&self, leaf: &Leaf) -> Vec<(u16, usize, u16)> {
        self.gates(leaf).into_iter().map(|(part, crossed, into, ..)| (part, crossed, into)).collect()
    }

    /// The way from `from` to `to` over what the chart holds, a leg to a
    /// chunk; empty where they are one tile. None where either is not a
    /// tile the chart holds open, or no way joins them on it. It crosses
    /// the chunks [`route`] gives over the chart's parts, and inside each
    /// takes the fewest steps to the gate out. The same chart gives the
    /// same way.
    pub fn way(&self, from: Tile, to: Tile) -> Option<Vec<Leg>> {
        let (start, goal) = (self.node(from)?, self.node(to)?);
        let mut gates: HashMap<ChunkId, Vec<(u16, usize, u16, Tile, Tile)>> = HashMap::new();
        let crossed = route(start, goal, |(chunk, part)| {
            let out = gates.entry(chunk).or_insert_with(|| self.leaves.get(&chunk).map_or(Vec::new(), |leaf| self.gates(leaf)));
            out.iter().filter(|gate| gate.0 == part).map(|gate| (ChunkId(chunk.0 + SIDES[gate.1].0, chunk.1 + SIDES[gate.1].1), gate.2)).collect()
        })?;
        let steps = |leaf: &Leaf, from: Tile, to: Tile| leaf.steps(from)[place(leaf.chunk.center(), to)?];
        let mut way = Vec::new();
        let mut at = from;
        for pair in crossed.windows(2) {
            let ((chunk, part), (next, into)) = (pair[0], pair[1]);
            let leaf = self.leaves.get(&chunk)?;
            let &(.., by, onto) = gates.get(&chunk)?.iter().find(|gate| gate.0 == part && side(chunk, next) == Some(gate.1) && gate.2 == into)?;
            way.push(Leg { from: at, by, to: onto, steps: steps(leaf, at, by)? + 1 });
            at = onto;
        }
        if at != to {
            way.push(Leg { from: at, by: to, to, steps: steps(self.leaves.get(&goal.0)?, at, to)? });
        }
        Some(way)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        chunk::calculate_visible_chunks,
        components::entity_type::{decorator::Decorator, EntityType},
    };

    /// The chunks within `reach` of the origin's, each tile's level and
    /// cover as `lies` says.
    fn ground(reach: u8, lies: impl Fn(i32, i32) -> (i32, common::Cover)) -> Map {
        let map = Map::new(qrz::Map::new(1.0, 0.8));
        for chunk in calculate_visible_chunks(ChunkId(0, 0), reach) {
            for (q, r) in chunk_tiles(chunk) {
                let (z, cover) = lies(q, r);
                map.insert(Qrz { q, r, z }, EntityType::Decorator(Decorator { cover, is_solid: true }));
            }
        }
        map
    }

    /// Every chunk within `reach` of the origin's, charted off `map`.
    fn charted(map: &Map, reach: u8) -> Chart {
        let mut chart = Chart::default();
        for chunk in calculate_visible_chunks(ChunkId(0, 0), reach) {
            chart.lay(Arc::new(Leaf::read(map, chunk).expect("the map holds the chunk")));
        }
        chart
    }

    fn full() -> common::Cover {
        (0..common::TILE_SLOTS as usize).fold(common::Cover::NONE, |c, k| c.with_boulder(k))
    }

    fn centre(n: i32, m: i32) -> Tile {
        let centre = ChunkId(n, m).center();
        (centre.q, centre.r)
    }

    /// A way's legs join end to end from its start to its goal, each
    /// within one chunk but for its last step, and the chart holds each.
    fn whole(chart: &Chart, way: &[Leg], from: Tile, to: Tile) {
        let mut at = from;
        for leg in way {
            assert_eq!(leg.from, at);
            assert_eq!(loc_to_chunk(Qrz { q: leg.from.0, r: leg.from.1, z: 0 }), loc_to_chunk(Qrz { q: leg.by.0, r: leg.by.1, z: 0 }));
            assert!(chart.open(*leg), "{leg:?}");
            let walk = chart.walk(*leg);
            assert_eq!((walk[0], walk[walk.len() - 1], walk.len() as u32), (leg.from, leg.to, leg.steps + 1), "{leg:?}");
            assert!(walk.windows(2).all(|step| qrz::hex_distance(step[0], step[1]) == 1 && (chart.tile(step[0]).unwrap().0 - chart.tile(step[1]).unwrap().0).abs() <= 1 && !chart.tile(step[1]).unwrap().1));
            at = leg.to;
        }
        assert_eq!(at, to);
    }

    #[test]
    fn a_tiles_place_is_its_turn_among_its_chunks_tiles() {
        for chunk in [ChunkId(0, 0), ChunkId(3, -2)] {
            for (at, tile) in chunk_tiles(chunk).enumerate() {
                assert_eq!(place(chunk.center(), tile), Some(at));
            }
            assert_eq!(place(chunk.center(), centre(chunk.0 + 1, chunk.1)), None);
        }
    }

    /// Over open ground a way crosses chunks, takes no fewer steps than
    /// the tiles between and not many more, and is the same every time.
    #[test]
    fn a_way_over_open_ground_runs_chunk_to_chunk() {
        let chart = charted(&ground(4, |_, _| (0, common::Cover::NONE)), 4);
        let (from, to) = (centre(-3, 1), centre(3, -2));
        let way = chart.way(from, to).expect("open ground");
        whole(&chart, &way, from, to);
        let (steps, straight) = (way.iter().map(|leg| leg.steps).sum::<u32>(), qrz::hex_distance(from, to) as u32);
        assert!(way.len() > 3 && steps >= straight && steps < straight * 3 / 2, "{steps} steps over {straight} tiles");
        let back = reversed(&way);
        whole(&chart, &back, to, from);
        assert_eq!(back.iter().map(|leg| leg.steps).sum::<u32>(), steps, "home by the way it came, in as many steps");
        assert_eq!(reversed(&back), way);
        assert_eq!(chart.way(from, to), Some(way));
        assert_eq!(chart.way(from, from), Some(Vec::new()));
    }

    /// A way keeps to what is charted: it goes round a chunk the chart
    /// lacks, and there is none to a tile on no leaf.
    #[test]
    fn a_way_keeps_to_the_chart() {
        let map = ground(3, |_, _| (0, common::Cover::NONE));
        let mut chart = charted(&map, 3);
        chart.leaves.remove(&ChunkId(0, 0));
        let (from, to) = (centre(-2, 0), centre(2, 0));
        let way = chart.way(from, to).expect("round the gap");
        whole(&chart, &way, from, to);
        assert!(way.iter().all(|leg| loc_to_chunk(Qrz { q: leg.to.0, r: leg.to.1, z: 0 }) != ChunkId(0, 0)));
        assert_eq!(chart.way(from, centre(0, 0)), None);
        assert_eq!(chart.way(from, centre(9, 9)), None);
    }

    /// The chunks beside a chunk are each across from the one three on,
    /// and as near as two chunks lie.
    #[test]
    fn a_chunk_has_six_sides() {
        let here = ChunkId(4, -7);
        for (k, &(n, m)) in SIDES.iter().enumerate() {
            let beside = ChunkId(here.0 + n, here.1 + m);
            assert_eq!(side(here, beside), Some(k));
            assert_eq!(side(beside, here), Some((k + 3) % 6));
            assert_eq!(qrz::hex_distance(centre(here.0, here.1), centre(beside.0, beside.1)), 2 * CHUNK_RADIUS + 1);
        }
        assert_eq!(side(here, ChunkId(here.0 + 2, here.1)), None);
    }

    /// A cliff no walker climbs is gone round by its one gap, and with
    /// no gap there is no way.
    #[test]
    fn a_way_goes_round_a_cliff() {
        let high = |q: i32| if q > 0 { 6 } else { 0 };
        let ramp = |q: i32, r: i32| if (0..=6).contains(&q) && (20..=22).contains(&r) { q } else { high(q) };
        let (from, to) = (centre(-2, 0), centre(2, 0));

        let chart = charted(&ground(4, |q, r| (ramp(q, r), common::Cover::NONE)), 4);
        let way = chart.way(from, to).expect("up the ramp");
        whole(&chart, &way, from, to);
        assert!(way.iter().map(|leg| leg.steps).sum::<u32>() > qrz::hex_distance(from, to) as u32);

        let sheer = charted(&ground(4, |q, _| (high(q), common::Cover::NONE)), 4);
        assert_eq!(sheer.way(from, to), None);
    }

    /// A map is what was charted: a wall raised since leaves its way as
    /// it was, and the ground as it lies now no longer holds the leg
    /// through it.
    #[test]
    fn a_way_charted_is_tried_on_the_ground_as_it_lies() {
        let (from, to) = (centre(-1, 0), centre(1, 0));
        let was = charted(&ground(1, |_, _| (0, common::Cover::NONE)), 1);
        let way = was.way(from, to).expect("open ground");

        // Every tile of the middle chunk's rim is filled
        let rim = |q: i32, r: i32| qrz::hex_distance((q, r), (0, 0)) == CHUNK_RADIUS;
        let now = charted(&ground(1, |q, r| (0, if rim(q, r) { full() } else { common::Cover::NONE })), 1);
        assert_eq!(was.way(from, to).as_ref(), Some(&way), "the map still shows the way");
        assert!(way.iter().all(|&leg| was.open(leg)));
        assert!(way.iter().any(|&leg| !now.open(leg)), "and the ground no longer gives it");
        assert_ne!(now.way(from, to), Some(way), "a map charted now goes another way");
    }

    /// A leaf comes back from the store as it was kept.
    #[test]
    fn a_leaf_is_recalled_as_it_was_kept() {
        let chunk = ChunkId(-4, 7);
        let map = Map::new(qrz::Map::new(1.0, 0.8));
        for (at, (q, r)) in chunk_tiles(chunk).enumerate() {
            let cover = if at % 5 == 0 { full() } else { common::Cover::NONE };
            map.insert(Qrz { q, r, z: 900 + (at as i32 % 7) - 3 }, EntityType::Decorator(Decorator { cover, is_solid: true }));
        }
        let leaf = Leaf::read(&map, chunk).unwrap();
        assert_eq!(Leaf::recalled(chunk, &leaf.kept()), Some(leaf.clone()));
        assert_eq!(Leaf::recalled(chunk, &leaf.kept()[1..]), None);
    }

    /// A leaf holds each hearth that stood as it was charted, keeps them,
    /// and is another leaf once one is laid or gone.
    #[test]
    fn a_leaf_holds_the_hearths_that_stood() {
        let hearth = common::building::Construct { kind: common::building::Kind::Hearth, of: common::Material::Sandstone }.raised(common::Cover::NONE).expect("open ground takes a hearth");
        let (bare, laid) = (ground(0, |_, _| (0, common::Cover::NONE)), ground(0, |q, r| (0, if (q, r) == (3, -2) { hearth } else { common::Cover::NONE })));
        let (was, now) = (Leaf::read(&bare, ChunkId(0, 0)).unwrap(), Leaf::read(&laid, ChunkId(0, 0)).unwrap());
        assert_eq!((was.hearths().count(), now.hearths().collect::<Vec<_>>()), (0, vec![(3, -2)]));
        assert_ne!(was, now);
        assert_eq!(Leaf::recalled(ChunkId(0, 0), &now.kept()), Some(now.clone()));
        assert!(!Leaf::unmarked(&was.kept()) && Leaf::unmarked(&was.kept()[..GROUND_KEPT]));
        assert_eq!(Leaf::recalled(ChunkId(0, 0), &now.kept()[..GROUND_KEPT]).map(|bare| bare.hearths().count()), Some(0), "kept with nothing said of hearths, it has none");
        assert_eq!(was.marked([(3, -2), (400, 400)].into_iter()).hearths().collect::<Vec<_>>(), vec![(3, -2)]);
        let mut chart = Chart::default();
        chart.lay(Arc::new(now));
        assert_eq!(chart.hearths().collect::<Vec<_>>(), vec![(3, -2)]);
    }

    /// A chart says how each tile it holds lay, and nothing of one it
    /// does not.
    #[test]
    fn a_chart_says_how_a_tile_lay() {
        let chart = charted(&ground(0, |q, r| (q - r, if (q, r) == (2, 1) { full() } else { common::Cover::NONE })), 0);
        assert_eq!(chart.tile((2, 1)), Some((1, true)));
        assert_eq!(chart.tile((-3, 4)), Some((-7, false)));
        assert_eq!(chart.tile(centre(1, 0)), None);
    }

    /// A way to something full ends beside it.
    #[test]
    fn a_full_tile_is_come_beside() {
        let chart = charted(&ground(1, |q, r| (0, if (q, r) == (0, 0) { full() } else { common::Cover::NONE })), 1);
        assert_eq!(chart.way(centre(1, 0), (0, 0)), None);
        let beside = chart.beside((0, 0)).expect("open ground round it");
        assert_eq!(qrz::hex_distance(beside, (0, 0)), 1);
        assert!(chart.way(centre(1, 0), beside).is_some());
        assert_eq!(chart.beside(centre(1, 0)), Some(centre(1, 0)));
    }
}
