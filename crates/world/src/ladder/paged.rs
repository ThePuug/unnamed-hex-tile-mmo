//! A level's values over its whole index space, held a page at a time: a
//! page is allocated the first time one of its values is written, and an
//! unwritten value reads as the default. A level spans a landmass down to
//! the tiles, and a request touches a few of its pages, so the table of
//! pages is itself in chunks, each allocated as a page in it is written.

use std::ops::{Index, IndexMut};

/// Values on a page: a level's cells are paged 64 by 64 (`Level::idx`).
pub const PAGE: usize = 4096;

/// Pages to a chunk of the page table.
const CHUNK: usize = 4096;

type Chunk<T> = Box<[Option<Box<[T]>>]>;

#[derive(Clone)]
pub struct Paged<T: Clone> {
    default: T,
    len: usize,
    chunks: Vec<Option<Chunk<T>>>,
}

impl<T: Clone> Paged<T> {
    /// `len` values, all `default`; `len` a whole number of pages.
    pub fn new(default: T, len: usize) -> Self {
        assert_eq!(len % PAGE, 0, "a paged length is whole pages");
        Paged { default, len, chunks: vec![None; (len / PAGE).div_ceil(CHUNK)] }
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
        self.chunks.iter_mut().for_each(|c| *c = None);
    }

    /// Every value in order, unwritten ones as the default: for a level
    /// small enough to read whole.
    pub fn iter(&self) -> impl Iterator<Item = &T> + '_ {
        (0..self.len).map(move |i| &self[i])
    }

    /// The pages written, as the index of their first value, in order.
    pub fn written(&self) -> impl Iterator<Item = usize> + '_ {
        self.chunks.iter().enumerate().filter_map(|(k, c)| c.as_ref().map(|c| (k, c))).flat_map(|(k, chunk)| {
            chunk.iter().enumerate().filter(|(_, p)| p.is_some()).map(move |(g, _)| (k * CHUNK + g) * PAGE)
        })
    }
}

impl<T: Clone> Index<usize> for Paged<T> {
    type Output = T;

    fn index(&self, i: usize) -> &T {
        let page = i / PAGE;
        match &self.chunks[page / CHUNK] {
            Some(chunk) => match &chunk[page % CHUNK] {
                Some(values) => &values[i % PAGE],
                None => &self.default,
            },
            None => &self.default,
        }
    }
}

impl<T: Clone> IndexMut<usize> for Paged<T> {
    fn index_mut(&mut self, i: usize) -> &mut T {
        let page = i / PAGE;
        let default = &self.default;
        let chunk = self.chunks[page / CHUNK].get_or_insert_with(|| vec![None; CHUNK].into_boxed_slice());
        let values = chunk[page % CHUNK].get_or_insert_with(|| vec![default.clone(); PAGE].into_boxed_slice());
        &mut values[i % PAGE]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unwritten_values_read_the_default_and_written_ones_stay() {
        let mut p = Paged::new(f64::NAN, 3 * PAGE);
        assert!(p[5].is_nan());
        p[PAGE + 7] = 2.0;
        assert_eq!(p[PAGE + 7], 2.0);
        assert!(p[PAGE + 8].is_nan());
        assert_eq!(p.written().collect::<Vec<_>>(), vec![PAGE]);
        p.fill(0.0);
        assert_eq!(p[PAGE + 7], 0.0);
    }

    #[test]
    fn a_page_past_the_first_chunk_is_held_alone() {
        let mut p = Paged::new(0u8, (CHUNK + 2) * PAGE);
        p[(CHUNK + 1) * PAGE + 3] = 9;
        assert_eq!(p[(CHUNK + 1) * PAGE + 3], 9);
        assert_eq!(p[3], 0);
        assert_eq!(p.written().collect::<Vec<_>>(), vec![(CHUNK + 1) * PAGE]);
    }
}
