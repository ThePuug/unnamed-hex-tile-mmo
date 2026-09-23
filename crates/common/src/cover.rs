//! A tile's cover: what stands at each of its three sites. And the canopy,
//! what stands over many tiles as seven of them say, which is what the
//! far ground is coloured by.

use serde::{Deserialize, Serialize};

/// What stands in one slot of a tile.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Slot {
    #[default]
    Empty = 0,
    Brush = 1,
    Pine = 2,
    Deciduous = 3,
}

impl Slot {
    /// How many of a tile's [`TILE_SLOTS`] it holds: a tree two, brush one.
    pub fn slots(self) -> u8 {
        match self {
            Slot::Empty => 0,
            Slot::Brush => 1,
            Slot::Pine | Slot::Deciduous => 2,
        }
    }

    /// Whether it stands in a walker's way: a tree does, brush does not.
    pub fn is_solid(self) -> bool {
        matches!(self, Slot::Pine | Slot::Deciduous)
    }

    fn from_bits(bits: u16) -> Slot {
        match bits & 3 {
            1 => Slot::Brush,
            2 => Slot::Pine,
            3 => Slot::Deciduous,
            _ => Slot::Empty,
        }
    }
}

/// How many slots a tile has: its centre and one toward each neighbour.
/// What stands on the ground holds some of them, by its size.
pub const TILE_SLOTS: u8 = 7;

/// Where each site lies: toward every other neighbour of a flat-top hex,
/// a third of a turn apart, so the centre stays free and three trunks
/// stand as far from each other as from the tile's edge. A tree at a site
/// holds its slot and the next one round.
pub const SITES: [(i32, i32); 3] = [(1, 0), (-1, 1), (0, -1)];

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

/// The three sites of a tile, two bits each, site `k` in bits `2k..2k+2`
/// in [`SITES`] order. Fits a `u16` with room, so it crosses the wire as
/// one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Cover(u16);

impl Cover {
    pub const NONE: Cover = Cover(0);

    pub fn from_bits(bits: u16) -> Cover {
        Cover(bits & ((1 << (2 * SITES.len())) - 1))
    }

    pub fn bits(self) -> u16 {
        self.0
    }

    pub fn slot(self, k: usize) -> Slot {
        debug_assert!(k < SITES.len());
        Slot::from_bits(self.0 >> (2 * k))
    }

    /// This cover with slot `k` holding `slot`.
    pub fn with(self, k: usize, slot: Slot) -> Cover {
        debug_assert!(k < SITES.len());
        Cover((self.0 & !(3 << (2 * k))) | ((slot as u16) << (2 * k)))
    }

    /// How many of the tile's [`TILE_SLOTS`] solid things hold: what
    /// movement reads.
    pub fn fullness(self) -> u8 {
        self.filled().filter(|(_, s)| s.is_solid()).map(|(_, s)| s.slots()).sum()
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The filled slots, each with what it holds.
    pub fn filled(self) -> impl Iterator<Item = (usize, Slot)> {
        (0..SITES.len()).map(move |k| (k, self.slot(k))).filter(|(_, s)| *s != Slot::Empty)
    }
}

/// What stands over the tiles a summary covers, read from the
/// [`crate::summary::SAMPLES`] sample tiles' slots: how many of those
/// readings hold pine, deciduous and brush, five bits each, the rest
/// empty. Lossless for the reading, two bytes on the wire, and the density
/// and the kinds' shares fall out of it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Canopy(u16);

/// The slot readings a canopy is made of, and the most it can count.
pub const CANOPY_READINGS: u16 = crate::summary::SAMPLES as u16 * SITES.len() as u16;

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
            Slot::Brush => 2,
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
        self.count(Slot::Pine) + self.count(Slot::Deciduous) + self.count(Slot::Brush)
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The share of the readings holding anything, 0 to 1.
    pub fn density(self) -> f64 {
        self.filled() as f64 / CANOPY_READINGS as f64
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
        assert_eq!(Cover::from_bits(c.bits()), c);
        assert_eq!(c.filled().count(), 2);
    }

    /// A sway is the slot's alone: the same twice, different for each
    /// slot of a tile, and within its bounds.
    #[test]
    fn sway_is_the_slots_own_and_bounded() {
        assert_eq!(sway(5, -7, 2), sway(5, -7, 2));
        let sways: Vec<Sway> = (0..SITES.len()).map(|k| sway(5, -7, k)).collect();
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

    /// A tree holds two slots and stands in the way; brush holds one and
    /// does not, so fullness counts only the trees' slots.
    #[test]
    fn fullness_counts_the_solid_slots() {
        let brush = (0..SITES.len()).fold(Cover::NONE, |c, k| c.with(k, Slot::Brush));
        assert_eq!(brush.fullness(), 0);
        assert_eq!(brush.bits() >> (2 * SITES.len()), 0, "nothing past the last site");
        assert_eq!(Cover::NONE.with(1, Slot::Pine).fullness(), 2);
        assert_eq!(brush.with(0, Slot::Pine).with(2, Slot::Deciduous).fullness(), 4);
        let trees = (0..SITES.len()).fold(Cover::NONE, |c, k| c.with(k, Slot::Pine));
        assert!(trees.fullness() <= TILE_SLOTS);
    }

    /// A canopy counts what its covers hold: none from none, every
    /// reading from a full one, and the kinds' counts from a mixture.
    #[test]
    fn a_canopy_counts_what_its_covers_hold() {
        let samples = crate::summary::SAMPLES;
        assert_eq!(Canopy::of(&vec![Cover::NONE; samples]), Canopy::NONE);
        let all = |kind: Slot| Cover::NONE.with(0, kind).with(1, kind).with(2, kind);
        let pines = Canopy::of(&vec![all(Slot::Pine); samples]);
        assert_eq!(pines.count(Slot::Pine), CANOPY_READINGS);
        assert_eq!(pines.count(Slot::Empty), 0);
        assert_eq!(pines.density(), 1.0);
        assert_eq!(Canopy::from_bits(pines.bits()), pines);
        let mixed = Canopy::of(&[all(Slot::Pine), all(Slot::Deciduous), Cover::NONE.with(1, Slot::Brush)]);
        assert_eq!((mixed.count(Slot::Pine), mixed.count(Slot::Deciduous), mixed.count(Slot::Brush)), (3, 3, 1));
        assert_eq!(mixed.filled(), 7);
        assert_eq!(mixed.count(Slot::Empty), CANOPY_READINGS - 7);
    }
}
