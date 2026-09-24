//! What gathering takes from a tile and leaves on it: the one rule the
//! server applies and the client picks its target by.

use serde::{Deserialize, Serialize};

use crate::cover::{Content, Cover, SITE_SLOTS};
use crate::Rock;

/// What gathering gives: a tree's wood by its kind, a boulder's stone by
/// its rock.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Material {
    Softwood,
    Hardwood,
    Sandstone,
    Limestone,
    Basement,
}

impl Material {
    pub const ALL: [Material; 5] =
        [Material::Softwood, Material::Hardwood, Material::Sandstone, Material::Limestone, Material::Basement];

    pub fn name(self) -> &'static str {
        match self {
            Material::Softwood => "Softwood",
            Material::Hardwood => "Hardwood",
            Material::Sandstone => "Sandstone",
            Material::Limestone => "Limestone",
            Material::Basement => "Basement stone",
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }

    /// The stone a boulder of `rock` gives, or None for shale, which breaks
    /// to clay.
    pub fn stone(rock: Rock) -> Option<Material> {
        match rock {
            Rock::Shale => None,
            Rock::Sandstone => Some(Material::Sandstone),
            Rock::Limestone => Some(Material::Limestone),
            Rock::Basement => Some(Material::Basement),
        }
    }
}

/// How much one felled tree gives.
pub const TREE_YIELD: u32 = 4;

/// How much one mined boulder gives.
pub const BOULDER_YIELD: u32 = 3;

/// What gathering one slot takes and leaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Harvest {
    /// The tile's cover once the slot is gathered.
    pub cover: Cover,
    pub material: Material,
    pub amount: u32,
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
/// leaves its stump in its first slot and frees its second; a boulder of
/// any rock but shale frees its slot. Brush and stumps are not gathered.
pub fn harvest(cover: Cover, k: usize) -> Option<Harvest> {
    let at = anchor(cover, k);
    let felled = |stump: Content, material: Material| {
        let second = SITE_SLOTS.iter().find(|s| s[0] == at)?[1];
        Some(Harvest {
            cover: cover.with_content(at, stump).with_content(second, Content::Empty),
            material,
            amount: TREE_YIELD,
        })
    };
    match cover.content(at) {
        Content::Pine => felled(Content::PineStump, Material::Softwood),
        Content::Deciduous => felled(Content::DeciduousStump, Material::Hardwood),
        Content::Boulder => Some(Harvest {
            cover: cover.with_content(at, Content::Empty),
            material: Material::stone(cover.rock())?,
            amount: BOULDER_YIELD,
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

    /// A boulder gives its rock's stone and frees its slot; shale gives
    /// nothing, and brush, stumps and empty slots are not gathered.
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
        assert_eq!(harvest(stump, SITE_SLOTS[0][0]), None);
        assert_eq!(harvest(Cover::NONE, 3), None);
    }
}
