//! A tile's cover: what stands in each of its three slots.

use serde::{Deserialize, Serialize};

/// What stands in one slot of a tile.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Slot {
    #[default]
    Empty = 0,
    Scrub = 1,
    Pine = 2,
    Deciduous = 3,
}

impl Slot {
    fn from_bits(bits: u16) -> Slot {
        match bits & 3 {
            1 => Slot::Scrub,
            2 => Slot::Pine,
            3 => Slot::Deciduous,
            _ => Slot::Empty,
        }
    }
}

/// Where each slot lies: toward every other neighbour of a flat-top hex,
/// a third of a turn apart, so the centre stays free and three trunks
/// stand as far from each other as from the tile's edge.
pub const SLOTS: [(i32, i32); 3] = [(1, 0), (-1, 1), (0, -1)];

/// How far from the centre toward the neighbour's centre a slot lies, as a
/// share of the centre spacing: the edge is at half, so this and the
/// sway together stay short of it and a tree stays on its tile.
pub const SLOT_SHARE: f64 = 0.32;

/// The three slots of a tile, two bits each, slot `k` in bits `2k..2k+2`
/// in [`SLOTS`] order. Fullness is how many hold anything: what movement
/// reads. Fits a `u16` with room, so it crosses the wire as one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Cover(u16);

impl Cover {
    pub const NONE: Cover = Cover(0);

    pub fn from_bits(bits: u16) -> Cover {
        Cover(bits & ((1 << (2 * SLOTS.len())) - 1))
    }

    pub fn bits(self) -> u16 {
        self.0
    }

    pub fn slot(self, k: usize) -> Slot {
        debug_assert!(k < SLOTS.len());
        Slot::from_bits(self.0 >> (2 * k))
    }

    /// This cover with slot `k` holding `slot`.
    pub fn with(self, k: usize, slot: Slot) -> Cover {
        debug_assert!(k < SLOTS.len());
        Cover((self.0 & !(3 << (2 * k))) | ((slot as u16) << (2 * k)))
    }

    /// How many slots hold anything, none to three.
    pub fn fullness(self) -> u8 {
        (0..SLOTS.len()).filter(|&k| self.slot(k) != Slot::Empty).count() as u8
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The filled slots, each with what it holds.
    pub fn filled(self) -> impl Iterator<Item = (usize, Slot)> {
        (0..SLOTS.len()).map(move |k| (k, self.slot(k))).filter(|(_, s)| *s != Slot::Empty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_pack_and_unpack() {
        let c = Cover::NONE.with(0, Slot::Pine).with(2, Slot::Deciduous);
        assert_eq!(c.slot(0), Slot::Pine);
        assert_eq!(c.slot(1), Slot::Empty);
        assert_eq!(c.slot(2), Slot::Deciduous);
        assert_eq!(c.fullness(), 2);
        assert_eq!(Cover::from_bits(c.bits()), c);
        assert_eq!(c.with(2, Slot::Empty).fullness(), 1);
        assert_eq!(c.filled().count(), 2);
    }

    #[test]
    fn three_of_anything_is_full() {
        let mut c = Cover::NONE;
        for k in 0..SLOTS.len() {
            c = c.with(k, Slot::Scrub);
        }
        assert_eq!(c.fullness(), 3);
        assert_eq!(c.bits() >> (2 * SLOTS.len()), 0, "nothing past the last slot");
    }
}
