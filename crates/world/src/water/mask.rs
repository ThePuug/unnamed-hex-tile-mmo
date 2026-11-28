//! Sets of a level's cells, held as their members in ascending order, so a
//! pass walks a request's few cells, never a level's whole index space.

use std::collections::HashSet;
use std::hash::{BuildHasherDefault, Hasher};
use std::ops::Index;

/// A cell index spread over the hash's bits.
#[derive(Default)]
pub struct Spread(u64);

impl Hasher for Spread {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ b as u64).wrapping_mul(0x100_0000_01b3);
        }
    }

    fn write_usize(&mut self, x: usize) {
        self.0 = (x as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    }
}

pub type Cells = HashSet<usize, BuildHasherDefault<Spread>>;

/// A set of a level's cells.
#[derive(Clone, Default)]
pub struct Mask {
    members: Vec<usize>,
    set: Cells,
}

impl Mask {
    pub fn new(cells: impl IntoIterator<Item = usize>) -> Self {
        let set: Cells = cells.into_iter().collect();
        let mut members: Vec<usize> = set.iter().copied().collect();
        members.sort_unstable();
        Mask { members, set }
    }

    /// The members, ascending.
    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.members.iter().copied()
    }

    pub fn contains(&self, c: usize) -> bool {
        self.set.contains(&c)
    }

    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    pub fn len(&self) -> usize {
        self.members.len()
    }

    /// The members `keep` holds.
    pub fn filter(&self, keep: impl Fn(usize) -> bool) -> Mask {
        Mask::new(self.iter().filter(|&c| keep(c)))
    }
}

impl Index<usize> for Mask {
    type Output = bool;

    fn index(&self, c: usize) -> &bool {
        if self.contains(c) { &true } else { &false }
    }
}

/// Which of a level's indices are its cells: those inside its side along
/// each axis, the padding of its last pages not.
#[derive(Clone, Copy)]
pub struct Exists {
    pub n: usize,
    pub n_j: usize,
}

impl Index<usize> for Exists {
    type Output = bool;

    fn index(&self, c: usize) -> &bool {
        let side = self.n_j.div_ceil(64);
        let (page, at) = (c / super::paged::PAGE, c % super::paged::PAGE);
        let (a, b) = ((page / side) * 64 + (at >> 6), (page % side) * 64 + (at & 63));
        if a < self.n && b < self.n_j { &true } else { &false }
    }
}
