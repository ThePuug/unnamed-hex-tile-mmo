//! What a client is sent of a map, in place of every tile it holds. A
//! [`Sketch`] is one charted chunk drawn down: a few samples of how high
//! its ground stands and how much of it is full, and which of its parts
//! joins which part of each chunk beside it. It is made of its chunk and
//! the seams it shares, at one resolution whatever the map, so a map
//! charting a chunk costs that chunk's sketch and nothing of the rest.
//!
//! An [`Atlas`] is the sketches a reader holds of one map. It is enough
//! to find a way on: [`Atlas::route`] is the search a wagon's way is
//! found by (`chart::route`), over the same parts joined the same, so
//! the chunks a reader sees a way cross are the chunks the wagon
//! crosses. What the atlas lacks is the tiles, and so the steps inside a
//! chunk, which only the chart's holder fills in.
//!
//! Neither says anything of colour or of where on a screen it lies: a
//! reader lays the sketches under a [`Frame`] of its own choosing.

use std::{collections::HashMap, sync::LazyLock};

use bevy::prelude::*;
use qrz::{Convert, Qrz};
use serde::{Deserialize, Serialize};

use crate::{
    chart::{place, route, Chart, Leaf, Node, SIDES},
    chunk::{chunk_tiles, loc_to_chunk, ChunkId, CHUNK_TILES},
};

type Tile = (i32, i32);

/// How many samples a sketch holds of its chunk, and how many tiles lie
/// between one and the next: the tiles within two steps of the chunk's
/// centre on a lattice that far apart, which reaches the chunk's rim.
pub const SAMPLES: usize = 19;
const APART: i32 = 4;

/// The most a sample's share of full tiles is told apart into.
const SHARES: u8 = 15;

/// The main part of a chunk with no open tile.
const NO_PART: u16 = u16::MAX;

/// Which sample each tile of a chunk is drawn by, in [`chunk_tiles`]
/// order: the one nearest it.
static NEAREST: LazyLock<Vec<u8>> = LazyLock::new(|| {
    let ring = |a: i32, b: i32| a.abs().max(b.abs()).max((a + b).abs());
    let centres: Vec<Tile> = (-2..=2).flat_map(|a| (-2..=2).map(move |b| (a, b))).filter(|&(a, b)| ring(a, b) <= 2).map(|(a, b)| (a * APART, b * APART)).collect();
    assert_eq!(centres.len(), SAMPLES);
    chunk_tiles(ChunkId(0, 0)).map(|tile| (0..SAMPLES).min_by_key(|&k| qrz::hex_distance(tile, centres[k])).unwrap_or(0) as u8).collect()
});

/// One charted chunk as a client is sent it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sketch {
    /// The level its samples are told from.
    base: i32,
    /// How far over `base` the ground round each sample stands.
    rise: [i8; SAMPLES],
    /// What share of the tiles round each sample is full, of [`SHARES`],
    /// two samples to a byte.
    full: [u8; SAMPLES.div_ceil(2)],
    /// Its main part (`Leaf::main`), which nearly every chunk is all of.
    main: u16,
    /// A bit to each side its main part joins the main part of the chunk
    /// on.
    open: u8,
    /// How rough its ground is (`Leaf::rough`): what a way pays to
    /// cross it.
    rough: u8,
}

/// A part of a chunk joined to a part of the chunk on one of its sides,
/// where either is not its chunk's main part.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    pub part: u16,
    pub side: u8,
    pub into: u16,
}

impl Sketch {
    /// The sketch of `leaf` as `chart` holds it: round each sample, the
    /// mean level of the tiles nearest it and the share of them that is
    /// full; and how its parts join those of the chunks the chart holds
    /// beside it, the joins of other than main part to main part apart.
    pub fn of(chart: &Chart, leaf: &Leaf) -> (Sketch, Vec<Link>) {
        let tiles = leaf.tiles();
        let base = (tiles.iter().map(|tile| tile.0 as i64).sum::<i64>() / CHUNK_TILES as i64) as i32;
        let mut round = [(0i64, 0u32, 0u32); SAMPLES];
        for (at, &(level, full)) in tiles.iter().enumerate() {
            let sample = &mut round[NEAREST[at] as usize];
            *sample = (sample.0 + level as i64, sample.1 + full as u32, sample.2 + 1);
        }
        let mut sketch = Sketch { base, rise: [0; SAMPLES], full: [0; SAMPLES.div_ceil(2)], main: leaf.main().unwrap_or(NO_PART), open: 0, rough: leaf.rough() };
        for (k, &(levels, full, count)) in round.iter().enumerate() {
            let count = count.max(1);
            sketch.rise[k] = (levels.div_euclid(count as i64) - base as i64).clamp(i8::MIN as i64, i8::MAX as i64) as i8;
            let share = ((full * SHARES as u32 + count / 2) / count) as u8;
            sketch.full[k / 2] |= share << (4 * (k % 2));
        }
        let mut links = Vec::new();
        for (part, side, into) in chart.links(leaf) {
            let beside = ChunkId(leaf.chunk().0 + SIDES[side].0, leaf.chunk().1 + SIDES[side].1);
            if Some(part) == leaf.main() && chart.leaf(beside).and_then(|beside| beside.main()) == Some(into) {
                sketch.open |= 1 << side;
            } else {
                links.push(Link { part, side: side as u8, into });
            }
        }
        (sketch, links)
    }

    /// How high the ground stands at `tile` of `chunk`, whose sketch this
    /// is, and what share of the ground round it is full, from 0 to 1;
    /// none where the tile is not of the chunk.
    pub fn at(&self, chunk: ChunkId, tile: Tile) -> Option<(i32, f32)> {
        let k = NEAREST[place(chunk.center(), tile)?] as usize;
        let share = self.full[k / 2] >> (4 * (k % 2)) & 0xf;
        Some((self.base + self.rise[k] as i32, share as f32 / SHARES as f32))
    }
}

/// The sketches a reader holds of one map: its ground, how its chunks
/// join, and the hubs that stood on it, each with the part of the ground
/// beside its hearth a way to it ends on.
#[derive(Clone, Debug, Default)]
pub struct Atlas {
    sketches: HashMap<ChunkId, Sketch>,
    links: HashMap<ChunkId, Vec<Link>>,
    hubs: HashMap<ChunkId, Vec<(Tile, Option<Node>)>>,
}

impl Atlas {
    /// Lays the sketch of `chunk` over what the atlas held of it, with
    /// its `links` and the `hubs` whose hearths stand in it. A sketch
    /// says how its chunk joins every chunk beside it, so what those
    /// said of joining it is let go: they may be older.
    pub fn lay(&mut self, chunk: ChunkId, sketch: Sketch, links: Vec<Link>, hubs: Vec<(Tile, Option<Node>)>) {
        for (side, step) in SIDES.iter().enumerate() {
            let (beside, back) = (ChunkId(chunk.0 + step.0, chunk.1 + step.1), ((side + 3) % 6) as u8);
            if let Some(held) = self.sketches.get_mut(&beside) {
                held.open &= !(1 << back);
            }
            if let Some(held) = self.links.get_mut(&beside) {
                held.retain(|link| link.side != back);
            }
        }
        self.sketches.insert(chunk, sketch);
        self.links.insert(chunk, links);
        self.hubs.insert(chunk, hubs);
    }

    pub fn sketch(&self, chunk: ChunkId) -> Option<&Sketch> {
        self.sketches.get(&chunk)
    }

    /// Each hub the map holds, by its hearth's tile, in one order
    /// however the atlas was laid.
    pub fn hubs(&self) -> Vec<Tile> {
        let mut hubs: Vec<Tile> = self.hubs.values().flatten().map(|hub| hub.0).collect();
        hubs.sort();
        hubs
    }

    /// The parts `node` is joined to, by side and part: what its own
    /// chunk's sketch says, and what each chunk beside it says of
    /// joining it.
    fn next(&self, (chunk, part): Node) -> Vec<Node> {
        let Some(here) = self.sketches.get(&chunk) else { return Vec::new() };
        let links = |chunk: &ChunkId| self.links.get(chunk).map_or(&[][..], Vec::as_slice);
        let mut next = Vec::new();
        for (side, step) in SIDES.iter().enumerate() {
            let (beside, back) = (ChunkId(chunk.0 + step.0, chunk.1 + step.1), (side + 3) % 6);
            let Some(there) = self.sketches.get(&beside) else { continue };
            let mains = part == here.main && (here.open >> side | there.open >> back) & 1 == 1;
            let mut into: Vec<u16> = mains.then_some(there.main).into_iter().collect();
            into.extend(links(&chunk).iter().filter(|link| link.part == part && link.side as usize == side).map(|link| link.into));
            into.extend(links(&beside).iter().filter(|link| link.into == part && link.side as usize == back).map(|link| link.part));
            into.sort();
            into.dedup();
            next.extend(into.into_iter().map(|into| (beside, into)));
        }
        next
    }

    /// The parts a way from `from` to `to` crosses on this map
    /// (`chart::route`): the chunks the way a wagon is sent by crosses.
    pub fn route(&self, from: Node, to: Node) -> Option<Vec<Node>> {
        route(from, to, |node| self.next(node), |chunk| self.sketches.get(&chunk).map_or(0, |sketch| sketch.rough))
    }

    /// The way the map gives between every two hubs it joins, each from
    /// one hearth to the other through the middle of every chunk crossed
    /// between the two they stand by.
    pub fn ways(&self) -> Vec<Vec<Tile>> {
        let mut hubs: Vec<(Tile, Node)> = self.hubs.values().flatten().filter_map(|&(hub, node)| Some((hub, node?))).collect();
        hubs.sort_by_key(|hub| hub.0);
        let pairs = (0..hubs.len()).flat_map(|a| (a + 1..hubs.len()).map(move |b| (a, b)));
        pairs
            .filter_map(|(a, b)| {
                let crossed = self.route(hubs[a].1, hubs[b].1)?;
                let between = crossed.get(1..crossed.len().saturating_sub(1)).unwrap_or(&[]);
                let middles = between.iter().map(|node| (node.0.center().q, node.0.center().r));
                Some(std::iter::once(hubs[a].0).chain(middles).chain(std::iter::once(hubs[b].0)).collect())
            })
            .collect()
    }
}

/// What a client is sent of the hubs whose hearths stand in `leaf` of
/// `chart`: each hearth's tile, and the part of the open ground beside
/// it, where there is any.
pub fn hubs(chart: &Chart, leaf: &Leaf) -> Vec<(Tile, Option<Node>)> {
    leaf.hearths().map(|hearth| (hearth, chart.beside(hearth).and_then(|beside| chart.node(beside)))).collect()
}

/// The chunk `tile` is of.
pub fn chunk_of(tile: Tile) -> ChunkId {
    loc_to_chunk(Qrz { q: tile.0, r: tile.1, z: 0 })
}

fn grid() -> qrz::Map {
    qrz::Map::new(common::grid::HEX_RADIUS, common::grid::RISE)
}

/// A square of ground as a reader looks at it: the tile at its middle
/// and how many tiles it is across, west to east. Everything is placed
/// by its tile's difference from the middle one, so ground far out in
/// the world is drawn as exactly as ground at its origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub centre: Tile,
    pub across: f32,
}

impl Frame {
    /// How many world units the frame is across.
    fn span(&self) -> f32 {
        self.across * 1.5 * common::grid::HEX_RADIUS
    }

    /// Where in the frame a point `offset` from the centre of `tile` is:
    /// across and down, each from 0 to 1 inside it, north up.
    pub fn spot(&self, tile: Tile, offset: Vec3) -> Vec2 {
        let at: Vec3 = grid().convert(Qrz { q: tile.0 - self.centre.0, r: tile.1 - self.centre.1, z: 0 });
        Vec2::new(at.x + offset.x, at.z + offset.z) / self.span() + 0.5
    }

    /// The tile under a spot of the frame ([`Frame::spot`]).
    pub fn tile(&self, spot: Vec2) -> Tile {
        let at = (spot - 0.5) * self.span();
        let from: Qrz = grid().convert(Vec3::new(at.x, 0., at.y));
        (self.centre.0 + from.q, self.centre.1 + from.r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::calculate_visible_chunks;
    use std::sync::Arc;

    /// The leaf of `chunk`, each tile at the level and fullness `lies`
    /// gives it.
    fn leaf(chunk: ChunkId, lies: impl Fn(Tile) -> (i32, bool)) -> Leaf {
        Leaf::of(chunk, chunk_tiles(chunk).map(lies).collect(), Vec::new()).unwrap()
    }

    /// The chunks within `reach` of the origin's charted as `lies` gives
    /// each tile, a hearth on each of `hearths`.
    fn charted(reach: u8, lies: impl Fn(Tile) -> (i32, bool), hearths: &[Tile]) -> Chart {
        let mut chart = Chart::default();
        for chunk in calculate_visible_chunks(ChunkId(0, 0), reach) {
            chart.lay(Arc::new(leaf(chunk, &lies).marked(hearths.iter().copied())));
        }
        chart
    }

    /// What a client holds of `chart` once it is sent every chunk, in
    /// the order `turn` puts them.
    fn sent(chart: &Chart, turn: impl Fn(&ChunkId) -> (i32, i32)) -> Atlas {
        let mut chunks: Vec<ChunkId> = chart.leaves().map(|leaf| leaf.chunk()).collect();
        chunks.sort_by_key(turn);
        let mut atlas = Atlas::default();
        for chunk in chunks {
            let leaf = chart.leaf(chunk).unwrap();
            let (sketch, links) = Sketch::of(chart, leaf);
            atlas.lay(chunk, sketch, links, hubs(chart, leaf));
        }
        atlas
    }

    fn centre(n: i32, m: i32) -> Tile {
        (ChunkId(n, m).center().q, ChunkId(n, m).center().r)
    }

    /// The chunks a way crosses, in turn.
    fn crossed(chart: &Chart, from: Tile, to: Tile) -> Option<Vec<ChunkId>> {
        let way = chart.way(from, to)?;
        let mut chunks: Vec<ChunkId> = way.iter().map(|leg| chunk_of(leg.from)).chain(std::iter::once(chunk_of(to))).collect();
        chunks.dedup();
        Some(chunks)
    }

    /// Every tile of a chunk is drawn by a sample, each sample draws
    /// some, and a slope is told from flat ground.
    #[test]
    fn a_chunks_tiles_are_shared_among_its_samples() {
        assert_eq!(NEAREST.len(), CHUNK_TILES);
        let drawn: std::collections::HashSet<u8> = NEAREST.iter().copied().collect();
        assert_eq!(drawn.len(), SAMPLES);
        let alone = |lies: fn(Tile) -> (i32, bool)| {
            let mut chart = Chart::default();
            chart.lay(Arc::new(leaf(ChunkId(0, 0), lies)));
            Sketch::of(&chart, chart.leaf(ChunkId(0, 0)).unwrap()).0
        };
        let (flat, slope) = (alone(|_| (0, false)), alone(|(q, _)| (q, false)));
        assert_ne!(flat, slope);
        assert!(slope.at(ChunkId(0, 0), (8, -4)).unwrap().0 > slope.at(ChunkId(0, 0), (-8, 4)).unwrap().0);
    }

    /// A sketch stands where its ground does; says how much of the
    /// ground round a sample is full; and says nothing of a tile off its
    /// chunk. A chunk far out in the world is drawn down as the same
    /// ground at its origin.
    #[test]
    fn a_sketch_is_its_chunk_drawn_down() {
        let of = |chunk: ChunkId, lies: &dyn Fn(Tile) -> (i32, bool)| {
            let mut chart = Chart::default();
            chart.lay(Arc::new(leaf(chunk, lies)));
            Sketch::of(&chart, chart.leaf(chunk).unwrap()).0
        };
        let chunk = ChunkId(3, -2);
        let middle = centre(3, -2);
        let high = of(chunk, &|_| (900, false));
        assert_eq!(high.at(chunk, middle), Some((900, 0.)));
        assert_eq!(high.at(chunk, (middle.0 + 40, middle.1)), None);

        let wooded = of(chunk, &|(q, _)| (-3, q >= middle.0));
        let (east, west) = (wooded.at(chunk, (middle.0 + 8, middle.1 - 4)).unwrap(), wooded.at(chunk, (middle.0 - 8, middle.1 + 4)).unwrap());
        assert_eq!((east, west), ((-3, 1.), (-3, 0.)));
        let share = wooded.at(chunk, middle).unwrap().1;
        assert!(share > 0. && share < 1., "{share}");

        let lies = |from: Tile| move |(q, r): Tile| (700 + (q - from.0) * 2 - (r - from.1), (q - from.0 + r - from.1) % 3 == 0);
        let far = ChunkId(90_000, -40_000);
        assert_eq!(of(ChunkId(0, 0), &lies((0, 0))), of(far, &lies(centre(far.0, far.1))));
    }

    /// A reader holding only a map's sketches finds, between any two
    /// places, the chunks the map's own way crosses: over open ground,
    /// round a cliff by its one ramp, round rough ground, and none where
    /// the cliff is sheer. It finds them the same whatever order the
    /// sketches came in.
    #[test]
    fn a_reader_finds_the_chunks_a_wagon_crosses() {
        let high = |q: i32| if q > 0 { 6 } else { 0 };
        let ramp = |(q, r): Tile| (if (0..=6).contains(&q) && (20..=22).contains(&r) { q } else { high(q) }, false);
        let stepped = |(q, r): Tile| (if place(ChunkId(0, 0).center(), (q, r)).is_some() { q & 1 } else { 0 }, false);
        let (from, to) = (centre(-2, 0), centre(2, 0));
        assert!(!crossed(&charted(4, stepped, &[]), from, to).unwrap().contains(&ChunkId(0, 0)));
        for chart in [charted(4, |_| (0, false), &[]), charted(4, ramp, &[]), charted(4, stepped, &[])] {
            let wagons = crossed(&chart, from, to).expect("a way");
            for atlas in [sent(&chart, |chunk| (chunk.0, chunk.1)), sent(&chart, |chunk| (-chunk.1, chunk.0 * 7 % 5))] {
                let read = atlas.route(chart.node(from).unwrap(), chart.node(to).unwrap()).expect("the reader finds one too");
                assert_eq!(read.iter().map(|node| node.0).collect::<Vec<_>>(), wagons);
            }
        }
        let sheer = charted(4, |(q, _)| (high(q), false), &[]);
        assert_eq!(crossed(&sheer, from, to), None);
        assert_eq!(sent(&sheer, |chunk| (chunk.0, chunk.1)).route(sheer.node(from).unwrap(), sheer.node(to).unwrap()), None);
    }

    /// A chunk cut in two by a wall joins its sides only part to part: a
    /// reader does not cross the wall where the map's own way does not.
    #[test]
    fn a_cut_chunk_is_crossed_only_where_it_joins() {
        // A wall down the middle chunk, and no chunk north or south to go round by
        let wall = |(q, _): Tile| (0, q == 0);
        let mut chart = Chart::default();
        for chunk in [ChunkId(-1, 0), ChunkId(0, 0), ChunkId(1, 0)] {
            chart.lay(Arc::new(leaf(chunk, wall)));
        }
        let (from, to) = (centre(-1, 0), centre(1, 0));
        assert_eq!(chart.way(from, to), None);
        let atlas = sent(&chart, |chunk| (chunk.0, chunk.1));
        assert_eq!(atlas.route(chart.node(from).unwrap(), chart.node(to).unwrap()), None);
        assert!(atlas.route(chart.node(from).unwrap(), chart.node((-3, 0)).unwrap()).is_some(), "it reaches its own side of the wall");
    }

    /// A map charted over is read as it now lies: a chunk walled since
    /// is crossed no more once its sketch has come, though the chunks
    /// beside it were sent before.
    #[test]
    fn a_sketch_laid_over_takes_its_seams_with_it() {
        let open = charted(1, |_| (0, false), &[]);
        let mut atlas = sent(&open, |chunk| (chunk.0, chunk.1));
        let (from, to) = (open.node(centre(-1, 0)).unwrap(), open.node(centre(1, 0)).unwrap());
        assert!(atlas.route(from, to).is_some_and(|crossed| crossed.iter().any(|node| node.0 == ChunkId(0, 0))));

        let mut walled = open.clone();
        walled.lay(Arc::new(leaf(ChunkId(0, 0), |_| (0, true))));
        let (sketch, links) = Sketch::of(&walled, walled.leaf(ChunkId(0, 0)).unwrap());
        atlas.lay(ChunkId(0, 0), sketch, links, Vec::new());
        let read = atlas.route(from, to).expect("round it");
        assert!(read.iter().all(|node| node.0 != ChunkId(0, 0)));
        assert_eq!(Some(read.iter().map(|node| node.0).collect::<Vec<_>>()), crossed(&walled, centre(-1, 0), centre(1, 0)));
    }

    /// A map's hubs are joined two by two, each way from one hearth to
    /// the other; a map of one hub has no way.
    #[test]
    fn a_way_joins_every_two_hubs() {
        let (west, east) = (centre(-2, 0), centre(2, 1));
        let joined = sent(&charted(3, |_| (0, false), &[west, east]), |chunk| (chunk.0, chunk.1));
        assert_eq!(joined.hubs(), vec![west, east]);
        let ways = joined.ways();
        assert_eq!(ways.len(), 1);
        assert_eq!((ways[0][0], ways[0][ways[0].len() - 1]), (west, east));
        assert!(ways[0].len() > 2);
        assert!(sent(&charted(3, |_| (0, false), &[west]), |chunk| (chunk.0, chunk.1)).ways().is_empty());
    }

    /// North is up and east is right, a spot of a frame is the tile
    /// drawn there, and a frame far out lays its ground as one at the
    /// origin does.
    #[test]
    fn a_frame_is_laid_north_up() {
        let frame = Frame { centre: (0, 0), across: 64. };
        let (middle, north, east) = (frame.spot((0, 0), Vec3::ZERO), frame.spot((0, -3), Vec3::ZERO), frame.spot((4, -2), Vec3::ZERO));
        assert_eq!(middle, Vec2::splat(0.5));
        assert!(north.y < middle.y && north.x == middle.x);
        assert!(east.x > middle.x && east.y == middle.y);
        assert!(frame.spot((0, 0), Vec3::X).x > middle.x);
        let far = Frame { centre: (2_000_000, -900_000), across: 64. };
        for tile in [(0, 0), (0, -3), (4, -2), (-7, 5)] {
            assert_eq!(frame.tile(frame.spot(tile, Vec3::ZERO)), tile);
            assert_eq!(far.spot((far.centre.0 + tile.0, far.centre.1 + tile.1), Vec3::ZERO), frame.spot(tile, Vec3::ZERO));
        }
    }
}
