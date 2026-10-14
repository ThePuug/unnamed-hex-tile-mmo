//! The levels finer than [`LAST_WATER`]: 9 and 3 wide, then the tiles,
//! each cell computed the first time it is read and kept. A cell is its
//! prediction from the level above, its layers' changes, and the cut handed
//! down, met by the channels at its centre tile. Every value is a function of its
//! position, so a cell computed by any request, on any thread, is the same.
//!
//! [`Terrain`] holds the ladder to [`LAST_WATER`] behind a lock and these
//! levels beside it, for readers on many threads: a read builds what it
//! needs of the ladder, takes what it reads of [`LAST_WATER`]'s cells, and
//! computes the finer cells without the lock.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard};

use dashmap::DashMap;

use super::channel::{self, Index, Reach};
use super::geom::{ancestor, cube_round, parent_of, CHILD_OFFSETS, NB};
use super::{cell_size, fan_at, predict, Above, Bounds, Ground, Ladder, Params, Pass, LAST_WATER, RING};

/// The fine levels' widths, coarsest first.
pub const FINE: [i64; 3] = [9, 3, 1];

/// A fine level's cell.
#[derive(Clone, Debug)]
pub struct Cell {
    /// Its height, and its dry height: its ground before any cut.
    pub z: f64,
    pub zd: f64,
    /// Water's cut below the dry ground: handed down, and at a tile its
    /// channel's. A cell above the tiles hands its children no channel:
    /// the tiles meet their own.
    pub cut: f64,
    /// Each layer's value at the cell.
    pub layers: Box<[f64]>,
    /// Whether a tile lies in a channel's bed, and its water's surface
    /// there, in levels, where it stands above the tile; for a coarser
    /// cell, whether a bed crosses it, and that bed's water.
    pub bed: bool,
    pub water: Option<f64>,
}

/// What [`LAST_WATER`] hands the fine levels at one of its cells: its dry
/// height, each layer's value and its cut, and its drawn height and corners,
/// which the channels' zone is read against.
#[derive(Clone)]
struct Coarse {
    zd: f64,
    fields: Box<[f64]>,
    z: f64,
    cc: [f64; 6],
}

/// One fine level's cells, read as the level above the next.
struct Kept<'a> {
    w: i64,
    layers: usize,
    cells: &'a DashMap<(i64, i64), Cell>,
}

impl Above for Kept<'_> {
    fn width(&self) -> i64 {
        self.w
    }

    fn layer_count(&self) -> usize {
        self.layers
    }

    fn dry(&self, i: i64, j: i64) -> f64 {
        self.cells.get(&(i, j)).map_or(f64::NAN, |c| c.zd)
    }

    fn field(&self, l: usize, i: i64, j: i64) -> f64 {
        self.cells.get(&(i, j)).map_or(f64::NAN, |c| if l < self.layers { c.layers[l] } else { c.cut })
    }
}

/// [`LAST_WATER`]'s cells as taken, read as the level above 9.
struct Taken<'a> {
    layers: usize,
    cells: &'a DashMap<(i64, i64), Coarse>,
}

impl Above for Taken<'_> {
    fn width(&self) -> i64 {
        LAST_WATER
    }

    fn layer_count(&self) -> usize {
        self.layers
    }

    fn dry(&self, i: i64, j: i64) -> f64 {
        self.cells.get(&(i, j)).map_or(f64::NAN, |c| c.zd)
    }

    fn field(&self, l: usize, i: i64, j: i64) -> f64 {
        self.cells.get(&(i, j)).map_or(f64::NAN, |c| c.fields[l])
    }
}

/// The channels' measures over every tile computed.
#[derive(Default, Clone, Debug)]
pub struct ChannelStats {
    /// Curves drawn, and those whose floor rose downstream.
    pub curves: u64,
    pub rising: u64,
    /// Tiles a channel lowered.
    pub lowered: u64,
    /// Each tile a channel reaches below [`LAST_WATER`]'s drawn surface: how
    /// far from its governing bank it lies, in tiles, and how far below
    /// that surface the channel is, in steps.
    pub reach: Vec<[f64; 2]>,
    /// The rivers whose walls reached [`channel::channel_reach`] still below
    /// that surface, as vertices of [`LAST_WATER`]'s level: the tiles where
    /// they did, and the largest step left there.
    pub capped: HashMap<(u8, i64, i64), (u64, f64)>,
}

/// The ladder to [`LAST_WATER`] and the finer levels beside it.
pub struct Terrain<'g, G: Ground> {
    coarse: Mutex<Ladder<'g, G>>,
    ground: &'g G,
    p: Params,
    layers: usize,
    /// [`LAST_WATER`]'s cells as taken.
    taken: DashMap<(i64, i64), Coarse>,
    /// The fine levels' cells, by [`FINE`].
    fine: [DashMap<(i64, i64), Cell>; 3],
    /// The channels each of the tiles' channel deform cells reads.
    channels: DashMap<(i64, i64), Arc<Index>>,
    stats: Mutex<ChannelStats>,
}

impl<'g, G: Ground> Terrain<'g, G> {
    /// The ladder over `extents`, coarsest first and ending at
    /// [`LAST_WATER`], and no fine cell yet.
    pub fn new(ground: &'g G, extents: &[(i64, i64, i64)], p: Params) -> Self {
        let boxes: Vec<Bounds> = extents.iter().map(|&(w, lo, hi)| (w, (lo, lo), (hi, hi))).collect();
        Terrain::new_box(ground, &boxes, p)
    }

    /// The ladder over `bounds` ([`Ladder::new_box`]), ending at
    /// [`LAST_WATER`], and no fine cell yet.
    pub fn new_box(ground: &'g G, bounds: &[Bounds], p: Params) -> Self {
        assert_eq!(bounds.last().map(|e| e.0), Some(LAST_WATER), "the ladder's levels end at LAST_WATER");
        Terrain {
            coarse: Mutex::new(Ladder::new_box(ground, bounds, p)),
            ground,
            p,
            layers: ground.layers(),
            taken: DashMap::new(),
            fine: [DashMap::new(), DashMap::new(), DashMap::new()],
            channels: DashMap::new(),
            stats: Mutex::new(ChannelStats::default()),
        }
    }

    /// The ladder to [`LAST_WATER`], for a request or a reading of it.
    pub fn coarse(&self) -> MutexGuard<'_, Ladder<'g, G>> {
        self.coarse.lock().expect("the ladder's lock")
    }

    pub fn params(&self) -> Params {
        self.p
    }

    pub fn channel_stats(&self) -> ChannelStats {
        self.stats.lock().expect("the channel measures' lock").clone()
    }

    /// Cells of the fine level `w` wide, computed where not yet; none where
    /// the ladder does not reach.
    pub fn cells(&self, w: i64, at: &[(i64, i64)]) -> Vec<Option<Cell>> {
        let k = fine_index(w);
        self.ensure(k, at);
        at.iter().map(|ij| self.fine[k].get(ij).map(|c| c.clone())).collect()
    }

    /// A fine cell already computed.
    pub fn kept(&self, w: i64, i: i64, j: i64) -> Option<Cell> {
        self.fine[fine_index(w)].get(&(i, j)).map(|c| c.clone())
    }

    /// How many cells each fine level holds.
    pub fn held(&self) -> [usize; 3] {
        [self.fine[0].len(), self.fine[1].len(), self.fine[2].len()]
    }

    /// Fine level k computed over `at`: the level above first over their
    /// parents and its ring, then these.
    fn ensure(&self, k: usize, at: &[(i64, i64)]) {
        let missing: Vec<(i64, i64)> = at.iter().copied().filter(|ij| !self.fine[k].contains_key(ij)).collect();
        if missing.is_empty() {
            return;
        }
        let above = with_ring(&parents(&missing));
        if k == 0 {
            self.take(&above, &[]);
        } else {
            self.ensure(k - 1, &above);
        }
        let w = FINE[k];
        if self.p.water {
            let centres: Vec<(i64, i64)> = missing.iter().map(|&(i, j)| (i * w, j * w)).collect();
            self.ensure_channels(&centres);
        }
        let computed = in_parallel(&missing, |&(i, j)| {
            let read = |out: &mut [f64]| self.ground.read(w, i, j, out);
            let index = self.p.water.then(|| self.channels.get(&channel_block((i * w, j * w))).map(|x| Arc::clone(&x))).flatten();
            if k == 0 {
                self.cell(&Taken { layers: self.layers, cells: &self.taken }, w, i, j, read, index)
            } else {
                self.cell(&Kept { w: FINE[k - 1], layers: self.layers, cells: &self.fine[k - 1] }, w, i, j, read, index)
            }
        });
        for (&ij, cell) in missing.iter().zip(computed) {
            if let Some(cell) = cell {
                self.fine[k].insert(ij, cell);
            }
        }
    }

    /// One fine cell over the level above, which must hold its parent and
    /// that parent's ring; none where it does not hold the parent.
    fn cell(&self, above: &impl Above, w: i64, i: i64, j: i64, read: impl Fn(&mut [f64]), index: Option<Arc<Index>>) -> Option<Cell> {
        let (a, b, _, _) = parent_of(i, j);
        if above.dry(a, b).is_nan() {
            return None;
        }
        let mut layers = vec![0.0; self.layers];
        read(&mut layers);
        let (pred, layer_pred) = predict(above, i, j);
        let change: f64 = (0..self.layers).map(|l| layers[l] - layer_pred[l]).sum();
        let wpred = layer_pred[self.layers];
        let ground = pred + change + wpred;
        let zd = (ground - wpred + 0.5).floor();
        let (low, bed, water) = match index {
            Some(index) => {
                let (q, r) = ((i * w) as f64, (j * w) as f64);
                let at = index.at(q, r, self.drawn_at(q, r), channel::channel_reach(), (w / 2) as f64);
                let low = ground.min(at.surface);
                if w > 1 {
                    (low, at.in_bed, at.water)
                } else {
                    self.measure(ground, &at);
                    (low, at.in_bed, at.water.filter(|&s| s > (low + 0.5).floor()))
                }
            }
            None => (ground, false, None),
        };
        let cut = if w == 1 { wpred + low - ground } else { wpred };
        Some(Cell { z: (low + 0.5).floor(), zd, cut, layers: layers.into_boxed_slice(), bed, water })
    }

    /// The surface [`LAST_WATER`] drew at a position in tiles.
    fn drawn_at(&self, q: f64, r: f64) -> f64 {
        let w = LAST_WATER;
        let (i, j) = cube_round(q / w as f64, r / w as f64);
        self.taken.get(&(i, j)).map_or(f64::NAN, |c| fan_at(w, i, j, c.z, &c.cc, q, r))
    }

    /// A tile's channels counted into the measures.
    fn measure(&self, ground: f64, at: &Reach) {
        let Some((river, d)) = at.governing else { return };
        let mut stats = self.stats.lock().expect("the channel measures' lock");
        stats.reach.push([d, at.below]);
        let step = ground - at.surface;
        stats.lowered += (step > 0.0) as u64;
        if at.capped {
            let e = stats.capped.entry(river).or_insert((0, 0.0));
            e.0 += 1;
            e.1 = e.1.max(step.max(0.0));
        }
    }

    /// [`LAST_WATER`]'s cells over `cells` built in the ladder and taken,
    /// and the channels of the tiles' channel deform cells `blocks` drawn.
    fn take(&self, cells: &[(i64, i64)], blocks: &[(i64, i64)]) {
        let want: Vec<(i64, i64)> = cells.iter().copied().filter(|ij| !self.taken.contains_key(ij)).collect();
        let blocks: Vec<(i64, i64)> = blocks.iter().copied().filter(|b| !self.channels.contains_key(b)).collect();
        if want.is_empty() && blocks.is_empty() {
            return;
        }
        let mut ladder = self.coarse();
        let k = ladder.levels.iter().position(|l| l.w == LAST_WATER).expect("the level water stops at");
        // A block's channels: the rivers under it and its ring and one ring
        // of LAST_WATER's cells more, routed a ring past those.
        let draws: Vec<Vec<(i64, i64)>> = blocks.iter().map(|&b| block_draw(b)).collect();
        let mut request: HashSet<(i64, i64)> = want.iter().copied().collect();
        for d in &draws {
            request.extend(with_ring(d));
        }
        let lv = &ladder.levels[k];
        let mut request: Vec<(i64, i64)> = request.into_iter().filter(|&(i, j)| lv.idx(i, j).is_some_and(|c| lv.exists[c])).collect();
        request.sort_unstable();
        ladder.request(k, &request);
        let lv = &ladder.levels[k];
        for &(i, j) in &want {
            let Some(c) = lv.idx(i, j).filter(|&c| lv.exists[c] && lv.built[c]) else { continue };
            let mut fields: Vec<f64> = lv.layers.iter().map(|l| l[c]).collect();
            fields.push(lv.cut[c]);
            self.taken.insert((i, j), Coarse { zd: lv.zd[c], fields: fields.into_boxed_slice(), z: lv.z[c], cc: lv.cc[c] });
        }
        for (&b, draw) in blocks.iter().zip(&draws) {
            let owners: Vec<usize> = draw.iter().filter_map(|&(i, j)| lv.idx(i, j)).filter(|&c| lv.exists[c]).collect();
            let drawn = super::water::in_parallel(&owners, |c| channel::own_banks(lv, c, &self.p));
            let mut stats = self.stats.lock().expect("the channel measures' lock");
            for (_, curves, rising) in &drawn {
                stats.curves += curves;
                stats.rising += rising;
            }
            drop(stats);
            self.channels.insert(b, Arc::new(Index::new(drawn.into_iter().flat_map(|(banks, _, _)| banks), channel::channel_reach())));
        }
    }

    /// The channels the tiles `at` read drawn where not yet, and the cells
    /// of [`LAST_WATER`] their zone is read against taken.
    fn ensure_channels(&self, at: &[(i64, i64)]) {
        let mut blocks: Vec<(i64, i64)> = at.iter().map(|&ij| channel_block(ij)).collect();
        blocks.sort_unstable();
        blocks.dedup();
        let up = LAST_WATER.ilog(3);
        let mut under: Vec<(i64, i64)> = at.iter().map(|&(i, j)| ancestor(i, j, up)).collect();
        under.sort_unstable();
        under.dedup();
        self.take(&with_ring(&under), &blocks);
    }
}

fn fine_index(w: i64) -> usize {
    FINE.iter().position(|&f| f == w).expect("a fine level")
}

/// The tiles' channel deform cell holding a tile.
fn channel_block((i, j): (i64, i64)) -> (i64, i64) {
    ancestor(i, j, cell_size(Pass::Channel, 1, RING).ilog(3))
}

/// The cells of [`LAST_WATER`] whose rivers a channel deform cell reads: the
/// cells under it and its ring, and one ring of them more.
fn block_draw((a, b): (i64, i64)) -> Vec<(i64, i64)> {
    let down = (cell_size(Pass::Channel, 1, RING) / LAST_WATER).ilog(3);
    let mut out = Vec::new();
    for (da, db) in std::iter::once((0, 0)).chain(NB) {
        let mut cells = vec![(a + da, b + db)];
        for _ in 0..down {
            cells = cells.iter().flat_map(|&(x, y)| CHILD_OFFSETS.iter().map(move |&(oi, oj)| (3 * x + oi, 3 * y + oj))).collect();
        }
        out.extend(cells);
    }
    with_ring(&out)
}

/// Cells and their ring, sorted.
fn with_ring(cells: &[(i64, i64)]) -> Vec<(i64, i64)> {
    let mut out: HashSet<(i64, i64)> = HashSet::new();
    for &(i, j) in cells {
        out.insert((i, j));
        for (di, dj) in NB {
            out.insert((i + di, j + dj));
        }
    }
    let mut v: Vec<(i64, i64)> = out.into_iter().collect();
    v.sort_unstable();
    v
}

/// Each cell's parent one level up.
fn parents(cells: &[(i64, i64)]) -> Vec<(i64, i64)> {
    cells.iter().map(|&(i, j)| {
        let (a, b, _, _) = parent_of(i, j);
        (a, b)
    }).collect()
}

/// `f` over every item, on every thread, in order.
fn in_parallel<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let chunk = items.len().div_ceil(threads).max(1);
    std::thread::scope(|s| {
        let handles: Vec<_> = items.chunks(chunk).map(|part| s.spawn(|| part.iter().map(&f).collect::<Vec<R>>())).collect();
        handles.into_iter().flat_map(|h| h.join().expect("a fine level's thread")).collect()
    })
}
