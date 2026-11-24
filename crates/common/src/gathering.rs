//! What gathering takes from a tile and leaves on it: the one rule the
//! server applies and the client picks its target by.

use serde::{Deserialize, Serialize};

use crate::cover::{Content, Cover, SITE_SLOTS};
use crate::Rock;

/// What is carried and built with: a tree's wood by its kind, a boulder's
/// stone by its rock, and the hide a beast leaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Material {
    Softwood,
    Hardwood,
    Sandstone,
    Limestone,
    Basement,
    Hide,
}

impl Material {
    pub const ALL: [Material; 6] = [Material::Softwood, Material::Hardwood, Material::Sandstone, Material::Limestone, Material::Basement, Material::Hide];

    /// The number a store keeps the material by. It is given once: a
    /// material renamed keeps it, a new one takes the next never used, and
    /// a removed one's is never given again, or what players hold under
    /// it becomes something else.
    pub fn id(self) -> u32 {
        match self {
            Material::Softwood => 1,
            Material::Hardwood => 2,
            Material::Sandstone => 3,
            Material::Limestone => 4,
            Material::Basement => 5,
            Material::Hide => 6,
        }
    }

    /// The material kept as `id`, or none where the game no longer has it.
    pub fn kept_as(id: u32) -> Option<Material> {
        Material::ALL.into_iter().find(|material| material.id() == id)
    }

    pub fn name(self) -> &'static str {
        match self {
            Material::Softwood => "Softwood",
            Material::Hardwood => "Hardwood",
            Material::Sandstone => "Sandstone",
            Material::Limestone => "Limestone",
            Material::Basement => "Basement stone",
            Material::Hide => "Hide",
        }
    }

    /// The name its assets are filed under: its icon is `icons/<stem>.png`.
    pub fn stem(self) -> &'static str {
        match self {
            Material::Softwood => "softwood",
            Material::Hardwood => "hardwood",
            Material::Sandstone => "sandstone",
            Material::Limestone => "limestone",
            Material::Basement => "basement",
            Material::Hide => "hide",
        }
    }

    /// What one of it weighs in the bag: a hide least, wood less than
    /// stone, and the hard of each more.
    pub fn weight(self) -> u32 {
        match self {
            Material::Hide => 2,
            Material::Softwood => 3,
            Material::Hardwood => 4,
            Material::Sandstone | Material::Limestone => 5,
            Material::Basement => 6,
        }
    }

    pub fn is_stone(self) -> bool {
        matches!(self, Material::Sandstone | Material::Limestone | Material::Basement)
    }

    pub fn is_wood(self) -> bool {
        matches!(self, Material::Softwood | Material::Hardwood)
    }

    /// The stone a boulder of `rock` gives, or None for shale, which breaks
    /// to clay.
    fn stone(rock: Rock) -> Option<Material> {
        match rock {
            Rock::Shale => None,
            Rock::Sandstone => Some(Material::Sandstone),
            Rock::Limestone => Some(Material::Limestone),
            Rock::Basement => Some(Material::Basement),
        }
    }
}

/// What stacks, in a bag or a loot window: a kind of thing of which one is
/// as good as another, so it is counted and never told apart. A piece is
/// never one; each is its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Stackable {
    Material(Material),
}

impl Stackable {
    pub fn name(self) -> &'static str {
        match self {
            Stackable::Material(m) => m.name(),
        }
    }

    /// What one of it weighs.
    pub fn weight(self) -> u32 {
        match self {
            Stackable::Material(m) => m.weight(),
        }
    }
}

/// A count of one stackable kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Stack {
    pub kind: Stackable,
    pub count: u32,
}

impl Stack {
    /// What the whole stack weighs.
    pub fn weight(self) -> u32 {
        self.kind.weight() * self.count
    }
}

/// The work an actor is seen at: a chop at a tree, a strike at a boulder,
/// or busy with its hands at whatever it is raising.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Work {
    Chop,
    Mine,
    Build,
}

/// What an actor is seen doing where it stands: at its work, stooped to
/// the pile its loot window is open on, or asleep on the ground.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Activity {
    Work(Work),
    Pickup,
    Sleep,
}

// How far ahead of the actor's feet each gather's clip lands its work, as
// the player's clips declare it, read from them at build time (`build.rs`):
// where the axe's edge meets the trunk, the pick's point the boulder, and the
// fists the pile.
include!(concat!(env!("OUT_DIR"), "/reaches.rs"));

/// How far ahead of the actor's feet `activity` lands its work.
pub fn reach(activity: Activity) -> f32 {
    match activity {
        Activity::Work(Work::Chop) => CHOP_REACH,
        Activity::Work(Work::Mine) => MINE_REACH,
        // Hands at the work, as at a pile
        Activity::Work(Work::Build) | Activity::Pickup => PICKUP_REACH,
        Activity::Sleep => 0.0,
    }
}

/// How long the work of a gather takes, in milliseconds.
pub const WORK_MS: u64 = 7500;

/// The work gathering slot `k` takes, or None where nothing gatherable
/// stands there.
pub fn work(cover: Cover, k: usize) -> Option<Work> {
    harvest(cover, k)?;
    Some(if cover.content(anchor(cover, k)) == Content::Boulder { Work::Mine } else { Work::Chop })
}

/// How much one felled tree gives.
const TREE_YIELD: u32 = 4;

/// How much one mined boulder gives.
const BOULDER_YIELD: u32 = 3;

/// How much one stump gives, the second gather of its tree.
const STUMP_YIELD: u32 = 2;

/// What gathering one slot takes and leaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Harvest {
    /// The tile's cover once the slot is gathered.
    pub cover: Cover,
    pub material: Material,
    pub amount: u32,
    /// The slot the gather freed, where what the player leaves of the
    /// yield lies as a pile.
    pub freed: usize,
}

impl Harvest {
    /// What the gather yields, as a stack.
    pub fn stack(&self) -> Stack {
        Stack { kind: Stackable::Material(self.material), count: self.amount }
    }
}

/// `cover` with a pile of `material` left in slot `k`.
pub fn left(cover: Cover, k: usize, material: Material) -> Cover {
    cover.with_content(k, Content::Pile(material))
}

/// `cover` with the pile in slot `k` emptied, the slot free again.
pub fn emptied(cover: Cover, k: usize) -> Cover {
    cover.with_content(k, Content::Empty)
}

/// Whether G acts on slot `k`: something gatherable stands there, a pile
/// lies there to open, or a construct stands there to use.
pub fn reachable(cover: Cover, k: usize) -> bool {
    cover.content(k).is_pile() || harvest(cover, k).is_some() || crate::building::used(cover, k)
}

/// The slot a gather of slot `k` acts on: a tree's first slot for either
/// of the two it holds, the slot itself otherwise.
pub fn anchor(cover: Cover, k: usize) -> usize {
    match cover.content(k) {
        Content::Spanned => SITE_SLOTS.iter().find(|s| s[1] == k).map_or(k, |s| s[0]),
        _ => k,
    }
}

/// What gathering slot `k` of a tile with `cover` gives and leaves, or None
/// where nothing gatherable stands. A tree, named by either slot it holds,
/// leaves its stump in its first slot and frees its second; the stump,
/// gathered after it, frees the first; a boulder of any rock but shale
/// frees its slot. Brush is not gathered.
pub fn harvest(cover: Cover, k: usize) -> Option<Harvest> {
    let at = anchor(cover, k);
    let felled = |stump: Content, material: Material| {
        let second = SITE_SLOTS.iter().find(|s| s[0] == at)?[1];
        Some(Harvest {
            cover: cover.with_content(at, stump).with_content(second, Content::Empty),
            material,
            amount: TREE_YIELD,
            freed: second,
        })
    };
    match cover.content(at) {
        Content::Pine => felled(Content::PineStump, Material::Softwood),
        Content::Deciduous => felled(Content::DeciduousStump, Material::Hardwood),
        stump @ (Content::PineStump | Content::DeciduousStump) => Some(Harvest {
            cover: cover.with_content(at, Content::Empty),
            material: if stump == Content::PineStump { Material::Softwood } else { Material::Hardwood },
            amount: STUMP_YIELD,
            freed: at,
        }),
        Content::Boulder => Some(Harvest {
            cover: cover.with_content(at, Content::Empty),
            material: Material::stone(cover.rock())?,
            amount: BOULDER_YIELD,
            freed: at,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A felled tree leaves a solid stump in its first slot and frees its
    /// second, whichever slot names it, so two trees that fill a tile
    /// leave it open.
    #[test]
    fn a_felled_tree_leaves_its_stump() {
        let two = Cover::NONE.with(0, Content::Pine).with(1, Content::Deciduous);
        assert_eq!(two.fullness(), 4);
        let [first, second] = SITE_SLOTS[0];
        for named in [first, second] {
            let cut = harvest(two, named).unwrap();
            assert_eq!((cut.cover.content(first), cut.cover.content(second)), (Content::PineStump, Content::Empty));
            assert_eq!((cut.material, cut.amount), (Material::Softwood, TREE_YIELD));
            assert_eq!(cut.cover.fullness(), 3);
            assert_eq!(cut.cover.growth(1), Content::Deciduous, "the other tree stands");
        }
        let both = harvest(harvest(two, first).unwrap().cover, SITE_SLOTS[1][0]).unwrap();
        assert_eq!((both.material, both.cover.fullness()), (Material::Hardwood, 2));
        assert_eq!(Cover::from_bits(both.cover.bits()), both.cover);
    }

    /// What a yield leaves lies as a pile in the slot the gather freed,
    /// holding no one up and opened by G; emptied, the slot is free again.
    #[test]
    fn a_left_yield_lies_as_a_pile() {
        let two = Cover::NONE.with(0, Content::Pine).with(1, Content::Pine);
        let cut = harvest(two, SITE_SLOTS[0][0]).unwrap();
        assert_eq!(cut.freed, SITE_SLOTS[0][1]);
        let piled = left(cut.cover, cut.freed, cut.material);
        assert_eq!(piled.content(cut.freed), Content::Pile(Material::Softwood));
        assert_eq!(piled.fullness(), cut.cover.fullness(), "a pile stands in no one's way");
        assert!(reachable(piled, cut.freed) && harvest(piled, cut.freed).is_none());
        assert_eq!(Cover::from_bits(piled.bits()), piled);
        assert_eq!(emptied(piled, cut.freed), cut.cover);
        for material in Material::ALL {
            let kept = Cover::from_bits(left(Cover::NONE, 3, material).bits());
            assert_eq!(kept.content(3), Content::Pile(material), "{material:?} is kept as itself");
        }
        let crag = Cover::NONE.with_boulder(4).with_rock(Rock::Basement);
        let mined = harvest(crag, 4).unwrap();
        assert_eq!(left(mined.cover, mined.freed, mined.material).content(4), Content::Pile(Material::Basement));
        assert_eq!(left(Cover::NONE.with_rock(Rock::Shale), 2, Material::Limestone).content(2), Content::Pile(Material::Limestone), "whatever it lies on");
    }

    /// A tree takes a chop and a boulder a strike; what is not gathered
    /// takes no work.
    #[test]
    fn a_gather_takes_its_work() {
        let ground = Cover::NONE.with(0, Content::Pine).with_boulder(0).with_rock(Rock::Sandstone);
        assert_eq!(work(ground, SITE_SLOTS[0][1]), Some(Work::Chop));
        assert_eq!(work(ground, 0), Some(Work::Mine));
        assert_eq!(work(ground.with_rock(Rock::Shale), 0), None);
        assert_eq!(work(ground, 3), None);
    }

    /// A boulder gives its rock's stone and frees its slot; shale gives
    /// nothing, and brush and empty slots are not gathered. A stump is
    /// its tree's second gather, and frees the tile of it.
    #[test]
    fn a_boulder_gives_its_rock() {
        let crag = Cover::NONE.with_boulder(0).with_rock(Rock::Limestone);
        let mined = harvest(crag, 0).unwrap();
        assert_eq!((mined.material, mined.amount), (Material::Limestone, BOULDER_YIELD));
        assert_eq!((mined.cover.content(0), mined.cover.fullness()), (Content::Empty, 0));
        assert_eq!(harvest(crag.with_rock(Rock::Shale), 0), None);
        let brush = Cover::NONE.with(0, Content::Brush);
        assert_eq!(harvest(brush, SITE_SLOTS[0][0]), None);
        let stump = harvest(Cover::NONE.with(0, Content::Pine), SITE_SLOTS[0][0]).unwrap().cover;
        let grubbed = harvest(stump, SITE_SLOTS[0][0]).unwrap();
        assert_eq!((grubbed.material, grubbed.amount, grubbed.freed), (Material::Softwood, STUMP_YIELD, SITE_SLOTS[0][0]));
        assert_eq!((grubbed.cover, work(stump, SITE_SLOTS[0][0])), (Cover::NONE, Some(Work::Chop)));
        assert_eq!(harvest(Cover::NONE, 3), None);
    }
}
