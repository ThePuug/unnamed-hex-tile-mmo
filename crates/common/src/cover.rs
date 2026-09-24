//! A tile's cover: what stands in each of its seven slots. And what stands
//! over many tiles as seven of them say: the canopy the far ground is
//! coloured by, and the outcrop the far crags are stood from.

use serde::{Deserialize, Serialize};

use crate::Rock;

/// What stands in one slot of a tile. A tree stands in its site's first
/// slot and spans the second, which holds [`Content::Spanned`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Content {
    #[default]
    Empty = 0,
    Brush = 1,
    Pine = 2,
    Deciduous = 3,
    /// The second slot of the tree in the site's first.
    Spanned = 4,
    Boulder = 5,
    /// What a felled tree leaves in its first slot.
    PineStump = 6,
    DeciduousStump = 7,
    /// What a player left of a yield: a pile of its wood, or of stone in
    /// the tile's rock.
    SoftwoodPile = 8,
    HardwoodPile = 9,
    StonePile = 10,
}

impl Content {
    /// How many of a tile's [`TILE_SLOTS`] it holds from its own on: a
    /// tree two, brush and a boulder one.
    pub fn slots(self) -> u8 {
        match self {
            Content::Empty | Content::Spanned => 0,
            Content::Brush | Content::Boulder | Content::PineStump | Content::DeciduousStump => 1,
            Content::SoftwoodPile | Content::HardwoodPile | Content::StonePile => 1,
            Content::Pine | Content::Deciduous => 2,
        }
    }

    /// Whether the slot stands in a walker's way: a tree's two do, a
    /// boulder's and a stump's do, brush's and a pile's do not.
    pub fn is_solid(self) -> bool {
        !matches!(self, Content::Empty | Content::Brush) && !self.is_pile()
    }

    /// Whether it is a pile a player left, which anyone may open.
    pub fn is_pile(self) -> bool {
        matches!(self, Content::SoftwoodPile | Content::HardwoodPile | Content::StonePile)
    }

    /// Whether it grows at a site: brush or a tree.
    pub fn is_growth(self) -> bool {
        matches!(self, Content::Brush | Content::Pine | Content::Deciduous)
    }

    fn from_bits(bits: u32) -> Content {
        match bits & CONTENT_MASK {
            1 => Content::Brush,
            2 => Content::Pine,
            3 => Content::Deciduous,
            4 => Content::Spanned,
            5 => Content::Boulder,
            6 => Content::PineStump,
            7 => Content::DeciduousStump,
            8 => Content::SoftwoodPile,
            9 => Content::HardwoodPile,
            10 => Content::StonePile,
            _ => Content::Empty,
        }
    }
}

/// The bits one slot's content takes in a [`Cover`].
const CONTENT_BITS: u32 = 4;
const CONTENT_MASK: u32 = (1 << CONTENT_BITS) - 1;

/// How many slots a tile has: its centre and one toward each neighbour.
/// What stands on the ground holds some of them, by its size.
pub const TILE_SLOTS: u8 = 7;

/// Where each slot lies: the centre, then toward each neighbour in turn
/// round the tile.
pub const SLOT_TOWARD: [(i32, i32); TILE_SLOTS as usize] = [(0, 0), (1, 0), (0, 1), (-1, 1), (-1, 0), (0, -1), (1, -1)];

/// Where each site lies: toward every other neighbour of a flat-top hex,
/// a third of a turn apart, so the centre stays free and three trunks
/// stand as far from each other as from the tile's edge.
pub const SITES: [(i32, i32); 3] = [(1, 0), (-1, 1), (0, -1)];

/// The slots each site's growth holds, in [`SLOT_TOWARD`] order: brush the
/// first, a tree both, the site's own and the next one round.
pub const SITE_SLOTS: [[usize; 2]; 3] = [[1, 2], [3, 4], [5, 6]];

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

/// The sway of the boulder in slot `k` of tile `(q, r)`, in
/// [`SLOT_TOWARD`] order: its own, apart from any site's.
pub fn boulder_sway(q: i32, r: i32, k: usize) -> Sway {
    sway(q, r, SITES.len() + k)
}

/// A draw in [0, 1) for slot `k` of tile `(q, r)`: what a far crag's
/// slots are filled by, against the share of rock its summary read.
pub fn boulder_draw(q: i32, r: i32, k: usize) -> f64 {
    (mix(q, r, SITES.len() + k, 6) >> 11) as f64 / (1u64 << 53) as f64
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

/// What stands on a tile, in one `u32` so it crosses the wire as one:
/// each of the seven slots' [`Content`] in [`CONTENT_BITS`], slot `k` from
/// bit `k * CONTENT_BITS` in [`SLOT_TOWARD`] order; and the boulders' rock
/// in the two bits past those, read only where a boulder stands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Cover(u32);

/// The first rock bit.
const ROCK_BIT: u32 = CONTENT_BITS * TILE_SLOTS as u32;

impl Cover {
    pub const NONE: Cover = Cover(0);

    /// The cover these bits hold: bits past the rock's are dropped, and a
    /// slot's bits that name no content read as empty.
    pub fn from_bits(bits: u32) -> Cover {
        (0..TILE_SLOTS as usize).fold(Cover(bits & (3 << ROCK_BIT)), |c, k| {
            c.with_content(k, Content::from_bits(bits >> (k as u32 * CONTENT_BITS)))
        })
    }

    pub fn bits(self) -> u32 {
        self.0
    }

    /// What stands in slot `k`, in [`SLOT_TOWARD`] order.
    pub fn content(self, k: usize) -> Content {
        debug_assert!(k < TILE_SLOTS as usize);
        Content::from_bits(self.0 >> (k as u32 * CONTENT_BITS))
    }

    /// This cover with slot `k` holding `content`, and nothing else moved.
    pub(crate) fn with_content(self, k: usize, content: Content) -> Cover {
        debug_assert!(k < TILE_SLOTS as usize);
        let at = k as u32 * CONTENT_BITS;
        Cover((self.0 & !(CONTENT_MASK << at)) | (content as u32) << at)
    }

    /// What grows at site `k`, in [`SITES`] order: brush, a tree, or
    /// nothing.
    pub fn growth(self, k: usize) -> Content {
        debug_assert!(k < SITES.len());
        Some(self.content(SITE_SLOTS[k][0])).filter(|c| c.is_growth()).unwrap_or_default()
    }

    /// This cover with `growth` at site `k`, holding the slots its size
    /// takes; the slots what grew there before held are freed.
    pub fn with(self, k: usize, growth: Content) -> Cover {
        debug_assert!(growth == Content::Empty || growth.is_growth());
        let [first, second] = SITE_SLOTS[k];
        let mut cover = self;
        if self.growth(k).slots() == 2 {
            cover = cover.with_content(second, Content::Empty);
        }
        cover = cover.with_content(first, growth);
        if growth.slots() == 2 {
            cover = cover.with_content(second, Content::Spanned);
        }
        cover
    }

    /// Whether a boulder stands in slot `k`, in [`SLOT_TOWARD`] order.
    pub fn boulder(self, k: usize) -> bool {
        self.content(k) == Content::Boulder
    }

    /// This cover with a boulder in slot `k`.
    pub fn with_boulder(self, k: usize) -> Cover {
        self.with_content(k, Content::Boulder)
    }

    /// The slots boulders stand in.
    pub fn boulders(self) -> impl Iterator<Item = usize> {
        (0..TILE_SLOTS as usize).filter(move |&k| self.boulder(k))
    }

    /// The rock the boulders are.
    pub fn rock(self) -> Rock {
        match self.0 >> ROCK_BIT & 3 {
            0 => Rock::Shale,
            1 => Rock::Sandstone,
            2 => Rock::Limestone,
            _ => Rock::Basement,
        }
    }

    /// This cover with its boulders of `rock`.
    pub fn with_rock(self, rock: Rock) -> Cover {
        let bits = match rock {
            Rock::Shale => 0,
            Rock::Sandstone => 1,
            Rock::Limestone => 2,
            Rock::Basement => 3,
        };
        Cover((self.0 & !(3 << ROCK_BIT)) | bits << ROCK_BIT)
    }

    /// Whether `growth` may stand at site `k`: every slot it would hold is
    /// empty.
    pub fn has_room(self, k: usize, growth: Content) -> bool {
        SITE_SLOTS[k][..growth.slots() as usize].iter().all(|&s| self.content(s) == Content::Empty)
    }

    /// How many of the tile's [`TILE_SLOTS`] hold something solid, trees
    /// and boulders alike: what movement reads.
    pub fn fullness(self) -> u8 {
        (0..TILE_SLOTS as usize).filter(|&k| self.content(k).is_solid()).count() as u8
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The sites a felled tree left its stump at, each with the stump.
    pub fn stumps(self) -> impl Iterator<Item = (usize, Content)> {
        (0..SITES.len())
            .map(move |k| (k, self.content(SITE_SLOTS[k][0])))
            .filter(|(_, c)| matches!(c, Content::PineStump | Content::DeciduousStump))
    }

    /// The sites something grows at, each with what grows there.
    pub fn filled(self) -> impl Iterator<Item = (usize, Content)> {
        (0..SITES.len()).map(move |k| (k, self.growth(k))).filter(|(_, g)| *g != Content::Empty)
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

    fn index(kind: Content) -> usize {
        match kind {
            Content::Pine => 0,
            Content::Deciduous => 1,
            Content::Brush => 2,
            _ => unreachable!("only growth is counted"),
        }
    }

    /// How many readings hold `kind`; the empty ones are the rest.
    pub fn count(self, kind: Content) -> u16 {
        match kind {
            Content::Empty => CANOPY_READINGS - self.filled(),
            kind => (self.0 >> (5 * Self::index(kind))) & 31,
        }
    }

    /// How many readings hold anything.
    pub fn filled(self) -> u16 {
        self.count(Content::Pine) + self.count(Content::Deciduous) + self.count(Content::Brush)
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The share of the readings holding anything, 0 to 1.
    pub fn density(self) -> f64 {
        self.filled() as f64 / CANOPY_READINGS as f64
    }

}

/// The rock over the tiles a summary covers, read from the
/// [`crate::summary::SAMPLES`] sample tiles' slots: how many of those
/// readings hold a boulder, six bits, and the rock of the first sample
/// that holds one, two. A crag is rare and small, so the count is what
/// says one stands here and the rock is its own. One byte on the wire.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Outcrop(u8);

/// The slot readings an outcrop is made of, and the most it can count.
pub const OUTCROP_READINGS: u8 = crate::summary::SAMPLES as u8 * TILE_SLOTS;

impl Outcrop {
    pub const NONE: Outcrop = Outcrop(0);

    /// The outcrop of the sample tiles' covers.
    pub fn of(covers: &[Cover]) -> Outcrop {
        debug_assert!(covers.len() <= crate::summary::SAMPLES);
        let count = covers.iter().map(|c| c.boulders().count() as u8).sum::<u8>();
        match covers.iter().find(|c| c.boulders().next().is_some()) {
            Some(first) => {
                let rock = (Cover::NONE.with_rock(first.rock()).bits() >> ROCK_BIT) as u8;
                Outcrop(count | rock << 6)
            }
            None => Outcrop::NONE,
        }
    }

    pub fn from_bits(bits: u8) -> Outcrop {
        Outcrop(bits)
    }

    pub fn bits(self) -> u8 {
        self.0
    }

    /// How many readings hold a boulder.
    pub fn boulders(self) -> u8 {
        self.0 & 63
    }

    /// The rock the boulders are.
    pub fn rock(self) -> Rock {
        Cover::from_bits(((self.0 >> 6) as u32) << ROCK_BIT).rock()
    }

    pub fn is_empty(self) -> bool {
        self.boulders() == 0
    }

    /// The share of the readings holding a boulder, 0 to 1.
    pub fn density(self) -> f64 {
        self.boulders() as f64 / OUTCROP_READINGS as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_pack_and_unpack() {
        let c = Cover::NONE.with(0, Content::Pine).with(2, Content::Deciduous);
        assert_eq!(c.growth(0), Content::Pine);
        assert_eq!(c.growth(1), Content::Empty);
        assert_eq!(c.growth(2), Content::Deciduous);
        assert_eq!(Cover::from_bits(c.bits()), c);
        assert_eq!(c.filled().count(), 2);
        let [first, second] = SITE_SLOTS[0];
        assert_eq!((c.content(first), c.content(second)), (Content::Pine, Content::Spanned));
        let cut = c.with(0, Content::Brush);
        assert_eq!((cut.content(first), cut.content(second)), (Content::Brush, Content::Empty), "brush frees the tree's second slot");
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

    /// A boulder holds one slot and stands in the way, whatever its rock;
    /// growth does not stand where one does.
    #[test]
    fn boulders_fill_their_slots() {
        let c = Cover::NONE.with_boulder(0).with_boulder(4).with_rock(Rock::Limestone);
        assert_eq!(c.boulders().collect::<Vec<_>>(), vec![0, 4]);
        assert_eq!(c.rock(), Rock::Limestone);
        assert_eq!(c.fullness(), 2);
        assert_eq!(Cover::from_bits(c.bits()), c);
        for rock in [Rock::Shale, Rock::Sandstone, Rock::Limestone, Rock::Basement] {
            assert_eq!(c.with_rock(rock).rock(), rock);
        }
        assert!(c.has_room(0, Content::Pine) && !c.has_room(1, Content::Pine) && c.has_room(1, Content::Brush));
        assert_eq!(c.with(0, Content::Pine).fullness(), 4, "a tree beside two boulders closes a tile");
        let all = (0..TILE_SLOTS as usize).fold(Cover::NONE, |c, k| c.with_boulder(k)).with_rock(Rock::Basement);
        assert_eq!(all.fullness(), TILE_SLOTS);
        assert_eq!(Cover::from_bits(u32::MAX).rock(), Rock::Basement);
        assert!((0..TILE_SLOTS as usize).all(|k| Cover::from_bits(u32::MAX).content(k) == Content::Empty), "bits naming no content read as empty");
    }

    /// An outcrop counts its covers' boulders and keeps their rock: none
    /// from none, every reading from covers all rock.
    #[test]
    fn an_outcrop_counts_the_boulders_its_covers_hold() {
        let samples = crate::summary::SAMPLES;
        assert!(Outcrop::of(&vec![Cover::NONE; samples]).is_empty());
        let face = (0..TILE_SLOTS as usize).fold(Cover::NONE, |c, k| c.with_boulder(k)).with_rock(Rock::Sandstone);
        let all = Outcrop::of(&vec![face; samples]);
        assert_eq!(all.boulders(), OUTCROP_READINGS);
        assert_eq!(all.rock(), Rock::Sandstone);
        assert_eq!(all.density(), 1.0);
        assert_eq!(Outcrop::from_bits(all.bits()), all);
        let one = Outcrop::of(&[Cover::NONE.with(0, Content::Pine), Cover::NONE.with_boulder(3).with_rock(Rock::Basement)]);
        assert_eq!((one.boulders(), one.rock()), (1, Rock::Basement));
    }

    /// Every site's slots are ring slots, each held by one site only.
    #[test]
    fn sites_hold_their_own_slots() {
        let mut seen = [false; TILE_SLOTS as usize];
        for (k, slots) in SITE_SLOTS.iter().enumerate() {
            assert_eq!(SLOT_TOWARD[slots[0]], SITES[k]);
            for &s in slots {
                assert!(s != 0 && !seen[s]);
                seen[s] = true;
            }
        }
    }

    /// A tree holds two slots and stands in the way; brush holds one and
    /// does not, so fullness counts only the trees' slots.
    #[test]
    fn fullness_counts_the_solid_slots() {
        let brush = (0..SITES.len()).fold(Cover::NONE, |c, k| c.with(k, Content::Brush));
        assert_eq!(brush.fullness(), 0);
        assert!(SITE_SLOTS.iter().all(|&[_, second]| brush.content(second) == Content::Empty), "brush holds one slot");
        assert_eq!(Cover::NONE.with(1, Content::Pine).fullness(), 2);
        assert_eq!(brush.with(0, Content::Pine).with(2, Content::Deciduous).fullness(), 4);
        let trees = (0..SITES.len()).fold(Cover::NONE, |c, k| c.with(k, Content::Pine));
        assert!(trees.fullness() <= TILE_SLOTS);
    }

    /// A canopy counts what its covers hold: none from none, every
    /// reading from a full one, and the kinds' counts from a mixture.
    #[test]
    fn a_canopy_counts_what_its_covers_hold() {
        let samples = crate::summary::SAMPLES;
        assert_eq!(Canopy::of(&vec![Cover::NONE; samples]), Canopy::NONE);
        let all = |kind: Content| Cover::NONE.with(0, kind).with(1, kind).with(2, kind);
        let pines = Canopy::of(&vec![all(Content::Pine); samples]);
        assert_eq!(pines.count(Content::Pine), CANOPY_READINGS);
        assert_eq!(pines.count(Content::Empty), 0);
        assert_eq!(pines.density(), 1.0);
        assert_eq!(Canopy::from_bits(pines.bits()), pines);
        let mixed = Canopy::of(&[all(Content::Pine), all(Content::Deciduous), Cover::NONE.with(1, Content::Brush)]);
        assert_eq!((mixed.count(Content::Pine), mixed.count(Content::Deciduous), mixed.count(Content::Brush)), (3, 3, 1));
        assert_eq!(mixed.filled(), 7);
        assert_eq!(mixed.count(Content::Empty), CANOPY_READINGS - 7);
    }
}
