//! A level's values over its whole index space, held a page at a time: a
//! page is allocated the first time one of its values is written, and an
//! unwritten value reads as the default. A level spans a landmass, and a
//! request touches a few of its pages.

use std::ops::{Index, IndexMut};

/// Values on a page: a level's cells are paged 64 by 64 (`Level::idx`).
pub const PAGE: usize = 4096;

#[derive(Clone)]
pub struct Paged<T: Clone> {
    default: T,
    pages: Vec<Option<Box<[T]>>>,
}

impl<T: Clone> Paged<T> {
    /// `len` values, all `default`; `len` a whole number of pages.
    pub fn new(default: T, len: usize) -> Self {
        assert_eq!(len % PAGE, 0, "a paged length is whole pages");
        Paged { default, pages: vec![None; len / PAGE] }
    }

    pub fn len(&self) -> usize {
        self.pages.len() * PAGE
    }

    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }

    /// Every value set to `value`, the pages released.
    pub fn fill(&mut self, value: T) {
        self.default = value;
        self.pages.iter_mut().for_each(|p| *p = None);
    }

    /// Every value in order, unwritten ones as the default.
    pub fn iter(&self) -> impl Iterator<Item = &T> + '_ {
        (0..self.len()).map(move |i| &self[i])
    }

    /// The pages written, as the index of their first value.
    pub fn written(&self) -> impl Iterator<Item = usize> + '_ {
        self.pages.iter().enumerate().filter(|(_, p)| p.is_some()).map(|(k, _)| k * PAGE)
    }
}

impl<T: Clone> Index<usize> for Paged<T> {
    type Output = T;

    fn index(&self, i: usize) -> &T {
        match &self.pages[i / PAGE] {
            Some(page) => &page[i % PAGE],
            None => &self.default,
        }
    }
}

impl<T: Clone> IndexMut<usize> for Paged<T> {
    fn index_mut(&mut self, i: usize) -> &mut T {
        let default = &self.default;
        let page = self.pages[i / PAGE].get_or_insert_with(|| vec![default.clone(); PAGE].into_boxed_slice());
        &mut page[i % PAGE]
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
}
