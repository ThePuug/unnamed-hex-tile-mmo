//! A level's values over its whole index space, held a page at a time: a
//! page is allocated the first time one of its values is written, and an
//! unwritten value reads as the default. A level spans a landmass down to
//! the tiles, and a request touches a few of its pages, so the table of
//! pages is itself in chunks, each allocated as a page in it is written.
//!
//! Every value is an atomic, so threads building apart share a level: a
//! value is read and written on its own, with no order against another,
//! and what orders a stage's values before its readers is the stage's
//! [`Progress`] at the cell.

use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU16, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::OnceLock;

/// Values on a page: a level's cells are paged 64 by 64 (`Level::idx`).
pub const PAGE: usize = 4096;

/// Pages to a chunk of the page table.
const CHUNK: usize = 4096;

/// A value a page holds, as the atomic holding it.
pub trait Atom: Copy + Send + Sync {
    type Cell: Send + Sync;
    fn cell(v: Self) -> Self::Cell;
    fn load(c: &Self::Cell) -> Self;
    fn store(c: &Self::Cell, v: Self);
}

macro_rules! atom {
    ($t:ty, $a:ty) => {
        impl Atom for $t {
            type Cell = $a;
            fn cell(v: Self) -> $a {
                <$a>::new(v)
            }
            fn load(c: &$a) -> Self {
                c.load(Ordering::Relaxed)
            }
            fn store(c: &$a, v: Self) {
                c.store(v, Ordering::Relaxed)
            }
        }
    };
}

atom!(bool, AtomicBool);
atom!(u8, AtomicU8);
atom!(u16, AtomicU16);
atom!(u32, AtomicU32);
atom!(i64, AtomicI64);

impl Atom for f64 {
    type Cell = AtomicU64;
    fn cell(v: Self) -> AtomicU64 {
        AtomicU64::new(v.to_bits())
    }
    fn load(c: &AtomicU64) -> Self {
        f64::from_bits(c.load(Ordering::Relaxed))
    }
    fn store(c: &AtomicU64, v: Self) {
        c.store(v.to_bits(), Ordering::Relaxed)
    }
}

type Page<T> = OnceLock<Box<[<T as Atom>::Cell]>>;

pub struct Paged<T: Atom> {
    default: T,
    len: usize,
    chunks: Box<[OnceLock<Box<[Page<T>]>>]>,
}

impl<T: Atom> Paged<T> {
    /// `len` values, all `default`; `len` a whole number of pages.
    pub fn new(default: T, len: usize) -> Self {
        assert_eq!(len % PAGE, 0, "a paged length is whole pages");
        Paged { default, len, chunks: (0..(len / PAGE).div_ceil(CHUNK)).map(|_| OnceLock::new()).collect() }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Every value set to `value`, the pages released.
    pub fn fill(&mut self, value: T) {
        self.default = value;
        self.chunks.iter_mut().for_each(|c| *c = OnceLock::new());
    }

    /// The value at `i`.
    pub fn get(&self, i: usize) -> T {
        self.held(i).map_or(self.default, T::load)
    }

    /// The value at `i` set to `v`.
    pub fn set(&self, i: usize, v: T) {
        T::store(self.atom(i), v)
    }

    /// The atomic holding the value at `i`, its page allocated.
    pub fn atom(&self, i: usize) -> &T::Cell {
        let page = i / PAGE;
        let chunk = self.chunks[page / CHUNK].get_or_init(|| (0..CHUNK).map(|_| OnceLock::new()).collect());
        let values = chunk[page % CHUNK].get_or_init(|| (0..PAGE).map(|_| T::cell(self.default)).collect());
        &values[i % PAGE]
    }

    /// The atomic holding the value at `i`; none where its page is not
    /// allocated, and the value is the default.
    pub fn held(&self, i: usize) -> Option<&T::Cell> {
        let page = i / PAGE;
        let values = self.chunks[page / CHUNK].get()?[page % CHUNK].get()?;
        Some(&values[i % PAGE])
    }

    /// Every value in order, unwritten ones as the default: for a level
    /// small enough to read whole.
    pub fn iter(&self) -> impl Iterator<Item = T> + '_ {
        (0..self.len).map(move |i| self.get(i))
    }

    /// The pages written, as the index of their first value, in order.
    pub fn written(&self) -> impl Iterator<Item = usize> + '_ {
        self.chunks.iter().enumerate().filter_map(|(k, c)| c.get().map(|c| (k, c))).flat_map(|(k, chunk)| {
            chunk.iter().enumerate().filter(|(_, p)| p.get().is_some()).map(move |(g, _)| (k * CHUNK + g) * PAGE)
        })
    }
}

/// How far a stage has come at each of a level's cells: to do, being done
/// by one thread, or done. A cell is done once every value the stage
/// writes for it is written, and a reader that finds it done reads them
/// all.
pub struct Progress {
    states: Paged<u8>,
}

const TODO: u8 = 0;
const BUSY: u8 = 1;
const DONE: u8 = 2;

/// What a claim on a cell found.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Claim {
    /// The cell is the claimant's to do.
    Mine,
    /// Another thread is doing it.
    Busy,
    Done,
}

impl Progress {
    /// `len` cells, all to do; `len` a whole number of pages.
    pub fn new(len: usize) -> Self {
        Progress { states: Paged::new(TODO, len) }
    }

    /// Every cell done.
    pub fn fill_done(&mut self) {
        self.states.fill(DONE);
    }

    pub fn done(&self, c: usize) -> bool {
        self.states.held(c).map_or(self.states.default, |a| a.load(Ordering::Acquire)) == DONE
    }

    /// The cell taken to do, where no thread has.
    pub fn claim(&self, c: usize) -> Claim {
        if self.done(c) {
            return Claim::Done;
        }
        match self.states.atom(c).compare_exchange(TODO, BUSY, Ordering::Acquire, Ordering::Acquire) {
            Ok(_) => Claim::Mine,
            Err(DONE) => Claim::Done,
            Err(_) => Claim::Busy,
        }
    }

    /// A cell done: every value written for it before this is read by a
    /// reader that finds it done. A stage that may run twice over a cell,
    /// each time writing the same, finishes it unclaimed.
    pub fn finish(&self, c: usize) {
        self.states.atom(c).store(DONE, Ordering::Release);
    }

    /// The pages holding a cell claimed or done, as the index of their
    /// first cell, in order.
    pub fn written(&self) -> impl Iterator<Item = usize> + '_ {
        self.states.written()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unwritten_values_read_the_default_and_written_ones_stay() {
        let mut p = Paged::new(f64::NAN, 3 * PAGE);
        assert!(p.get(5).is_nan());
        p.set(PAGE + 7, 2.0);
        assert_eq!(p.get(PAGE + 7), 2.0);
        assert!(p.get(PAGE + 8).is_nan());
        assert_eq!(p.written().collect::<Vec<_>>(), vec![PAGE]);
        p.fill(0.0);
        assert_eq!(p.get(PAGE + 7), 0.0);
    }

    #[test]
    fn a_page_past_the_first_chunk_is_held_alone() {
        let p = Paged::new(0u8, (CHUNK + 2) * PAGE);
        p.set((CHUNK + 1) * PAGE + 3, 9);
        assert_eq!(p.get((CHUNK + 1) * PAGE + 3), 9);
        assert_eq!(p.get(3), 0);
        assert_eq!(p.written().collect::<Vec<_>>(), vec![(CHUNK + 1) * PAGE]);
    }

    #[test]
    fn a_cell_is_claimed_once_and_done_once_finished() {
        let p = Progress::new(PAGE);
        assert_eq!(p.claim(4), Claim::Mine);
        assert_eq!(p.claim(4), Claim::Busy);
        assert!(!p.done(4));
        p.finish(4);
        assert_eq!(p.claim(4), Claim::Done);
        assert_eq!(p.claim(5), Claim::Mine);
    }
}
