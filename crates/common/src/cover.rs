//! A tile's cover: what stands in each of its three slots. And the canopy,
//! what stands over many tiles as seven of them say, from which a summary
//! fills the slots of every tile it covers by the tile's own rule, from
//! the tile's own draws.

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

/// How far a tree stands off its slot, along the slot's direction and
/// across it, at most, as a share of the centre spacing.
pub const SLOT_JITTER: f64 = 0.10;

/// What makes one tree its own: hashed from its slot alone, so every
/// process draws the same tree and nothing of it crosses the wire.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sway {
    /// Its place off the slot, along the slot's direction and across it,
    /// as shares of the centre spacing within [`SLOT_JITTER`].
    pub along: f64,
    pub across: f64,
    /// Its turn about the vertical, in radians.
    pub yaw: f64,
    /// How far it has grown from a sapling toward its kind's full height,
    /// 0 to 1, most trees well along: the drawer sets the heights.
    pub growth: f64,
    /// Which of its kind's variations it is: a hash the drawer takes
    /// modulo the variations it has.
    pub variation: u32,
}

/// The sway of the tree in slot `k` of tile `(q, r)`.
pub fn sway(q: i32, r: i32, k: usize) -> Sway {
    let unit = |channel: u64| (mix(q, r, k, channel) >> 11) as f64 / (1u64 << 53) as f64;
    Sway {
        along: (2.0 * unit(1) - 1.0) * SLOT_JITTER,
        across: (2.0 * unit(2) - 1.0) * SLOT_JITTER,
        yaw: unit(3) * std::f64::consts::TAU,
        growth: unit(4).sqrt(),
        variation: (mix(q, r, k, 5) >> 32) as u32,
    }
}

/// A hash of a slot and a channel: exact in the integers, so a tile a
/// million tiles out sways as one at the origin would.
fn mix(q: i32, r: i32, k: usize, channel: u64) -> u64 {
    let mut x = (q as u32 as u64) << 32 | (r as u32 as u64);
    x ^= (k as u64) << 3 ^ channel.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

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

/// What stands over the tiles a summary covers, read from the
/// [`crate::summary::SAMPLES`] sample tiles' slots: how many of those
/// readings hold pine, deciduous and scrub, five bits each, the rest
/// empty. Lossless for the reading, two bytes on the wire, and the density
/// and the kinds' shares fall out of it; the summary fills each slot of
/// every tile it covers from those by [`Canopy::slot`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Canopy(u16);

/// The slot readings a canopy is made of, and the most it can count.
pub const CANOPY_READINGS: u16 = crate::summary::SAMPLES as u16 * SLOTS.len() as u16;

impl Canopy {
    pub const NONE: Canopy = Canopy(0);

    /// The canopy of the sample tiles' covers.
    pub fn of(covers: &[Cover]) -> Canopy {
        debug_assert!(covers.len() <= crate::summary::SAMPLES);
        let mut counts = [0u16; 3];
        for cover in covers {
            for (_, slot) in cover.filled() {
                counts[Self::index(slot)] += 1;
            }
        }
        Canopy(counts[0] | counts[1] << 5 | counts[2] << 10)
    }

    pub fn from_bits(bits: u16) -> Canopy {
        Canopy(bits & 0x7FFF)
    }

    pub fn bits(self) -> u16 {
        self.0
    }

    fn index(kind: Slot) -> usize {
        match kind {
            Slot::Pine => 0,
            Slot::Deciduous => 1,
            Slot::Scrub => 2,
            Slot::Empty => unreachable!("an empty slot is not counted"),
        }
    }

    /// How many readings hold `kind`; the empty ones are the rest.
    pub fn count(self, kind: Slot) -> u16 {
        match kind {
            Slot::Empty => CANOPY_READINGS - self.filled(),
            kind => (self.0 >> (5 * Self::index(kind))) & 31,
        }
    }

    /// How many readings hold anything.
    pub fn filled(self) -> u16 {
        self.count(Slot::Pine) + self.count(Slot::Deciduous) + self.count(Slot::Scrub)
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The share of the readings holding anything, 0 to 1.
    pub fn density(self) -> f64 {
        self.filled() as f64 / CANOPY_READINGS as f64
    }

    /// What a slot holds under this canopy, from the slot's three draws in
    /// [0, 1) as the tile rule makes them: `fill` against the density,
    /// `kind` against the trees' share of what is filled, scrub past it,
    /// and `mix` against the deciduous share of the trees. The same draws
    /// the tile's own cover was filled by, so where the canopy's density
    /// is the tile's the same slot fills with the same kind; a denser
    /// canopy only adds trees and never moves one, and a canopy of one
    /// kind gives nothing else.
    pub fn slot(self, fill: f64, kind: f64, mix: f64) -> Slot {
        if self.density() <= fill {
            return Slot::Empty;
        }
        let filled = self.filled() as f64;
        let trees = (self.count(Slot::Pine) + self.count(Slot::Deciduous)) as f64;
        if kind >= trees / filled {
            return Slot::Scrub;
        }
        if mix < self.count(Slot::Deciduous) as f64 / trees {
            Slot::Deciduous
        } else {
            Slot::Pine
        }
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

    /// A sway is the slot's alone: the same twice, different for each
    /// slot of a tile, and within its bounds.
    #[test]
    fn sway_is_the_slots_own_and_bounded() {
        assert_eq!(sway(5, -7, 2), sway(5, -7, 2));
        let sways: Vec<Sway> = (0..SLOTS.len()).map(|k| sway(5, -7, k)).collect();
        for (i, a) in sways.iter().enumerate() {
            assert!(a.along.abs() <= SLOT_JITTER && a.across.abs() <= SLOT_JITTER);
            assert!((0.0..=1.0).contains(&a.growth));
            assert!(a.yaw >= 0.0 && a.yaw < std::f64::consts::TAU);
            for b in &sways[i + 1..] {
                assert_ne!(a, b);
            }
        }
        assert_ne!(sway(1_000_000, -2_000_000, 0), sway(1_000_000, -2_000_000, 1));
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

    /// A canopy counts what its covers hold, and its slots follow: none
    /// from none, every one from a full canopy of one kind, and as the
    /// density climbs a slot that was filled stays filled.
    #[test]
    fn a_canopy_counts_its_covers_and_fills_by_them() {
        let samples = crate::summary::SAMPLES;
        assert_eq!(Canopy::of(&vec![Cover::NONE; samples]), Canopy::NONE);
        let all = |kind: Slot| Cover::NONE.with(0, kind).with(1, kind).with(2, kind);
        let pines = Canopy::of(&vec![all(Slot::Pine); samples]);
        assert_eq!(pines.count(Slot::Pine), CANOPY_READINGS);
        assert_eq!(pines.count(Slot::Empty), 0);
        assert_eq!(pines.density(), 1.0);
        assert_eq!(Canopy::from_bits(pines.bits()), pines);
        for draw in [0.0, 0.3, 0.999] {
            assert_eq!(pines.slot(draw, draw, draw), Slot::Pine);
            assert_eq!(Canopy::NONE.slot(draw, draw, draw), Slot::Empty);
        }
        let mixed = Canopy::of(&[all(Slot::Pine), all(Slot::Deciduous), Cover::NONE.with(1, Slot::Scrub)]);
        assert_eq!((mixed.count(Slot::Pine), mixed.count(Slot::Deciduous), mixed.count(Slot::Scrub)), (3, 3, 1));
        assert_eq!(mixed.filled(), 7);
        let mut covers = Vec::new();
        let mut was = vec![Slot::Empty; 30];
        for n in 0..=samples {
            let canopy = Canopy::of(&covers);
            for (i, slot) in was.iter_mut().enumerate() {
                let draw = i as f64 / 30.0;
                let now = canopy.slot(draw, draw, draw);
                assert!(*slot == Slot::Empty || now == *slot, "a filled slot stays as the canopy thickens");
                *slot = now;
            }
            if n < samples {
                covers.push(all(Slot::Deciduous));
            }
        }
        assert!(was.iter().all(|s| *s == Slot::Deciduous));
        assert_eq!(mixed.slot(0.0, 0.99, 0.0), Slot::Scrub, "the kind draw past the trees' share is scrub");
        assert_eq!(mixed.slot(0.0, 0.0, 0.0), Slot::Deciduous, "the mix draw under the deciduous share is deciduous, else pine");
        assert_eq!(mixed.slot(0.0, 0.0, 0.4), Slot::Deciduous);
        assert_eq!(mixed.slot(0.0, 0.0, 0.99), Slot::Pine);
    }
}
