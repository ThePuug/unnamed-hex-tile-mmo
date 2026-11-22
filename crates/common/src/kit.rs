//! Everything worn or wielded, as the asset build declares it
//! (`models/kit.json`): the sets, the pieces of each, and what each seed
//! of a set makes it. A set is pieces designed together, or one weapon,
//! known by its shape; a seed is one make of it — its colours, and which
//! of each piece's shapes it is cut in. A piece's file holds its shapes,
//! its faces take materials named for what they are, and a seed says what
//! each of those is, so any piece of a set is drawn in any of its seeds.

use serde::{Deserialize, Serialize};

/// Where on a body a piece goes, one piece to each.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum Slot {
    Head,
    Torso,
    Hands,
    Waist,
    Legs,
    Feet,
    MainHand,
    OffHand,
}

impl Slot {
    pub const ALL: [Slot; 8] = [Slot::Head, Slot::Torso, Slot::Hands, Slot::Waist, Slot::Legs, Slot::Feet, Slot::MainHand, Slot::OffHand];
    /// The slots a garment goes in, down the body; the rest are held.
    pub const BODY: [Slot; 6] = [Slot::Head, Slot::Torso, Slot::Hands, Slot::Waist, Slot::Legs, Slot::Feet];

    pub fn name(self) -> &'static str {
        match self {
            Slot::Head => "Head",
            Slot::Torso => "Torso",
            Slot::Hands => "Hands",
            Slot::Waist => "Waist",
            Slot::Legs => "Legs",
            Slot::Feet => "Feet",
            Slot::MainHand => "Main\nhand",
            Slot::OffHand => "Off\nhand",
        }
    }

    /// Its place in [`Slot::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }
}

/// What a set is: an armour's weight, or what kind of thing is held.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Kind {
    Light,
    Medium,
    Heavy,
    Sword,
    Axe,
    Mace,
    Shield,
}

/// A material's colour by the name its glTF gives the material, linear RGB.
pub type Colours = &'static [(&'static str, [f32; 3])];

/// One set of the kit, and every seed of it the build ships.
#[derive(Debug)]
pub struct Set {
    pub name: &'static str,
    pub kind: Kind,
    pub seeds: &'static [Colours],
}

impl Set {
    /// What `seed` of this set turns the material named `material`, if it
    /// names it; a material it does not name keeps the colour its file
    /// gives it. A seed past those shipped is drawn as one of them.
    pub fn colour(&self, seed: u16, material: &str) -> Option<[f32; 3]> {
        let colours = self.seeds[seed as usize % self.seeds.len()];
        colours.iter().find(|(name, _)| *name == material).map(|&(_, rgb)| rgb)
    }
}

/// One piece of the kit: `name`, the stem of `models/<name>-<actor>.glb`;
/// `set`, its set's place in [`SETS`]; `fills`, every slot it takes, its
/// own first; `shapes`, how many glTF scenes its file holds, which its
/// set's seeds are cut in in turn.
#[derive(Debug)]
pub struct Piece {
    pub name: &'static str,
    pub set: usize,
    pub fills: &'static [Slot],
    pub shapes: u8,
}

include!(concat!(env!("OUT_DIR"), "/kit.rs"));

#[cfg(test)]
mod tests {
    use super::*;

    /// Every seed of every set paints the same materials, or a piece would
    /// keep one seed's leather under another's steel.
    #[test]
    fn every_seed_names_the_same_materials() {
        assert!(!SETS.is_empty());
        let names = |c: Colours| c.iter().map(|(name, _)| *name).collect::<Vec<_>>();
        for set in SETS {
            assert!(!set.seeds.is_empty(), "{}", set.name);
            for &seed in set.seeds {
                assert_eq!(names(seed), names(SETS[0].seeds[0]), "{}", set.name);
            }
        }
    }

    #[test]
    fn piece_names_are_distinct_asset_stems() {
        let names: Vec<&str> = PIECES.iter().map(|p| p.name).collect();
        for name in &names {
            assert!(name.chars().all(|c| c.is_ascii_lowercase() || c == '-'), "{name}");
        }
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), names.len());
    }

    /// Every piece goes somewhere, has a shape, and is of a set.
    #[test]
    fn every_piece_is_whole() {
        assert!(PIECES.len() <= u8::MAX as usize + 1);
        for piece in PIECES {
            assert!(!piece.fills.is_empty() && piece.shapes > 0 && piece.set < SETS.len(), "{}", piece.name);
        }
    }

    /// An armour set dresses a whole body: between them its pieces fill
    /// every slot a garment goes in, each once.
    #[test]
    fn an_armour_set_fills_the_body() {
        for (at, set) in SETS.iter().enumerate().filter(|(_, s)| matches!(s.kind, Kind::Light | Kind::Medium | Kind::Heavy)) {
            let mut filled: Vec<Slot> = PIECES.iter().filter(|p| p.set == at).flat_map(|p| p.fills.iter().copied()).collect();
            filled.sort_by_key(|s| s.index());
            assert_eq!(filled, Slot::BODY, "{}", set.name);
        }
    }
}
