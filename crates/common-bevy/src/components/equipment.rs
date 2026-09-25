//! What an actor wears and what a player owns. The design is
//! `design/equipment.md` in the internal repo.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// A piece the asset build makes, cut for every body that can wear it: a
/// garment of the leather set, or a plate of the kit.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Piece {
    LeatherHood,
    LeatherVest,
    PlateCuirass,
    LeatherGloves,
    LeatherGirdle,
    LeatherPants,
    LeatherBoots,
    SwordBreaker,
    PlateHelm,
    PlateGauntlets,
    PlateLeggings,
    PlateSabatons,
}

impl Piece {
    /// Every piece, in slot order.
    pub const ALL: [Piece; 12] = [
        Piece::LeatherHood,
        Piece::PlateHelm,
        Piece::LeatherVest,
        Piece::PlateCuirass,
        Piece::LeatherGloves,
        Piece::PlateGauntlets,
        Piece::LeatherGirdle,
        Piece::LeatherPants,
        Piece::PlateLeggings,
        Piece::LeatherBoots,
        Piece::PlateSabatons,
        Piece::SwordBreaker,
    ];

    /// The asset's name: the stem of `models/<name>-<actor>.glb`, and the
    /// key another piece's `hides.over` names, so the two must agree.
    pub fn name(self) -> &'static str {
        match self {
            Piece::LeatherHood => "leather-hood",
            Piece::LeatherVest => "leather-vest",
            Piece::PlateCuirass => "plate-cuirass",
            Piece::LeatherGloves => "leather-gloves",
            Piece::LeatherGirdle => "leather-girdle",
            Piece::LeatherPants => "leather-pants",
            Piece::LeatherBoots => "leather-boots",
            Piece::SwordBreaker => "sword-breaker",
            Piece::PlateHelm => "plate-helm",
            Piece::PlateGauntlets => "plate-gauntlets",
            Piece::PlateLeggings => "plate-leggings",
            Piece::PlateSabatons => "plate-sabatons",
        }
    }

    /// How many styles the piece is made in: the glTF scenes its asset
    /// holds, one outfit each.
    pub fn styles(self) -> u8 {
        STYLES
    }

    /// The slot a piece is worn in. The game assigns it: pants and boots
    /// both fit the legs.
    pub fn slot(self) -> Slot {
        match self {
            Piece::LeatherHood | Piece::PlateHelm => Slot::Head,
            Piece::LeatherVest | Piece::PlateCuirass => Slot::Torso,
            Piece::LeatherGloves | Piece::PlateGauntlets => Slot::Hands,
            Piece::LeatherGirdle => Slot::Waist,
            Piece::LeatherPants | Piece::PlateLeggings => Slot::Legs,
            Piece::LeatherBoots | Piece::PlateSabatons => Slot::Feet,
            Piece::SwordBreaker => Slot::OffHand,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Piece::LeatherHood => "Leather Hood",
            Piece::LeatherVest => "Leather Vest",
            Piece::PlateCuirass => "Plate Cuirass",
            Piece::LeatherGloves => "Leather Gloves",
            Piece::LeatherGirdle => "Leather Girdle",
            Piece::LeatherPants => "Leather Pants",
            Piece::LeatherBoots => "Leather Boots",
            Piece::SwordBreaker => "Sword-breaker",
            Piece::PlateHelm => "Plate Helm",
            Piece::PlateGauntlets => "Plate Gauntlets",
            Piece::PlateLeggings => "Plate Leggings",
            Piece::PlateSabatons => "Plate Sabatons",
        }
    }

    /// What the piece weighs, worn or carried.
    pub fn weight(self) -> u32 {
        match self {
            Piece::LeatherHood | Piece::LeatherGloves | Piece::LeatherGirdle => 1,
            Piece::LeatherPants | Piece::LeatherBoots => 2,
            Piece::LeatherVest | Piece::SwordBreaker | Piece::PlateGauntlets => 3,
            Piece::PlateHelm | Piece::PlateSabatons => 4,
            Piece::PlateLeggings => 6,
            Piece::PlateCuirass => 10,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum Slot {
    Head,
    Torso,
    Hands,
    Waist,
    Legs,
    Feet,
    OffHand,
}

impl Slot {
    pub const ALL: [Slot; 7] = [Slot::Head, Slot::Torso, Slot::Hands, Slot::Waist, Slot::Legs, Slot::Feet, Slot::OffHand];
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
            Slot::OffHand => "Off
hand",
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// The styles every set is made in: its seeds, one outfit each, which
/// are a piece's glTF scenes in order (`Piece::styles`). A style of the
/// leather and the same style of the plate are one outfit.
pub const STYLES: u8 = 3;

/// A piece in a style.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Item {
    pub piece: Piece,
    pub style: u8,
}

/// What an actor wears, one item to a slot. Server authority, sent to every
/// client that sees the actor.
#[derive(Clone, Component, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Equipment {
    worn: [Option<Item>; 7],
}

impl Equipment {
    pub fn worn(&self, slot: Slot) -> Option<Item> {
        self.worn[slot.index()]
    }

    pub fn is_worn(&self, item: Item) -> bool {
        self.worn(item.piece.slot()) == Some(item)
    }

    /// Wears `item` and returns what its slot held.
    pub fn wear(&mut self, item: Item) -> Option<Item> {
        self.worn[item.piece.slot().index()].replace(item)
    }

    /// Takes `item` off; false when it was not worn.
    pub fn take_off(&mut self, item: Item) -> bool {
        let worn = self.is_worn(item);
        if worn {
            self.worn[item.piece.slot().index()] = None;
        }
        worn
    }

    pub fn items(&self) -> impl Iterator<Item = Item> + '_ {
        self.worn.iter().flatten().copied()
    }

    /// What a player starts wearing, and what the character screen shows:
    /// the strapped leather, buckled down the vest, and the sword-breaker.
    pub fn starting_outfit() -> Self {
        const STRAPPED: u8 = 2;
        let mut outfit = Equipment::default();
        for piece in [
            Piece::LeatherHood,
            Piece::LeatherVest,
            Piece::LeatherGloves,
            Piece::LeatherGirdle,
            Piece::LeatherPants,
            Piece::LeatherBoots,
        ] {
            outfit.wear(Item { piece, style: STRAPPED });
        }
        outfit.wear(Item { piece: Piece::SwordBreaker, style: 0 });
        outfit
    }
}

/// Past this weight a player is overburdened and slows to a walk.
pub const BURDEN_LIMIT: u32 = 60;

/// Past this weight a player picks up nothing more.
pub const CARRY_LIMIT: u32 = 100;

/// How many stacks the bag holds: a material stacks, one stack to a
/// material, and a piece is a stack of one.
pub const BAG_STACKS: usize = 27;

/// Everything a player owns, in the order it came by it: its pieces, worn
/// or not, and a stack of each stackable kind it carries. The bag is what
/// it owns and does not wear: wearing moves an item out of the bag and
/// never reorders this. Server authority, sent to its owner only.
#[derive(Clone, Component, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Inventory {
    pub items: Vec<Item>,
    pub stock: Vec<common::Stack>,
}

impl Inventory {
    /// What a player starts owning: the outfit it wears, and an empty bag.
    pub fn wearing(outfit: &Equipment) -> Self {
        Self { items: outfit.items().collect(), ..default() }
    }

    pub fn contains(&self, item: Item) -> bool {
        self.items.contains(&item)
    }

    /// How many of `kind` the bag holds.
    pub fn count(&self, kind: common::Stackable) -> u32 {
        self.stock.iter().find(|s| s.kind == kind).map_or(0, |s| s.count)
    }

    /// Puts `stack` in the bag: onto the stack of its kind, or a new one
    /// after the rest.
    pub fn add(&mut self, stack: common::Stack) {
        match self.stock.iter_mut().find(|s| s.kind == stack.kind) {
            Some(held) => held.count += stack.count,
            None if stack.count > 0 => self.stock.push(stack),
            None => {}
        }
    }

    /// Takes up to `count` of `kind` out of the bag and returns what it
    /// took; a stack emptied frees its place, and the rest keep their order.
    pub fn remove(&mut self, kind: common::Stackable, count: u32) -> common::Stack {
        let Some(at) = self.stock.iter().position(|s| s.kind == kind) else { return common::Stack { kind, count: 0 } };
        let taken = count.min(self.stock[at].count);
        self.stock[at].count -= taken;
        if self.stock[at].count == 0 {
            self.stock.remove(at);
        }
        common::Stack { kind, count: taken }
    }

    /// The pieces in the bag: owned and not worn.
    pub fn bagged<'a>(&'a self, worn: &'a Equipment) -> impl Iterator<Item = Item> + 'a {
        self.items.iter().copied().filter(|&item| !worn.is_worn(item))
    }

    /// How many of the bag's [`BAG_STACKS`] its contents take.
    pub fn stacks(&self, worn: &Equipment) -> usize {
        self.bagged(worn).count() + self.stock.len()
    }

    /// What everything the player owns weighs, worn and bagged together.
    pub fn weight(&self) -> u32 {
        let pieces: u32 = self.items.iter().map(|i| i.piece.weight()).sum();
        let stock: u32 = self.stock.iter().map(|s| s.weight()).sum();
        pieces + stock
    }

    /// Whether the player carries past [`BURDEN_LIMIT`].
    pub fn is_burdened(&self) -> bool {
        self.weight() > BURDEN_LIMIT
    }

    /// How many of `kind` go in: as many as the weight left under
    /// [`CARRY_LIMIT`] carries, onto a stack of the kind already held or
    /// into a free one, and none of a new kind with no stack free.
    pub fn room_for(&self, worn: &Equipment, kind: common::Stackable) -> u32 {
        if self.count(kind) == 0 && self.stacks(worn) >= BAG_STACKS {
            return 0;
        }
        CARRY_LIMIT.saturating_sub(self.weight()) / kind.weight().max(1)
    }
}

/// Marks an actor carrying past [`BURDEN_LIMIT`]: it moves at
/// `movement::BURDENED_PACE` of its speed. The server sets it from the bag
/// and sends it with the intent; the owning client sets it from the bag it
/// holds.
#[derive(Clone, Component, Copy, Debug, Default)]
pub struct Burdened;

#[cfg(test)]
mod tests {
    use super::*;

    fn item(piece: Piece, style: u8) -> Item {
        Item { piece, style }
    }

    fn of(material: common::Material, count: u32) -> common::Stack {
        common::Stack { kind: common::Stackable::Material(material), count }
    }

    #[test]
    fn wearing_displaces_what_held_the_slot() {
        let mut worn = Equipment::default();
        assert_eq!(worn.wear(item(Piece::LeatherPants, 0)), None);
        assert_eq!(worn.wear(item(Piece::LeatherPants, 1)), Some(item(Piece::LeatherPants, 0)));
        assert!(worn.is_worn(item(Piece::LeatherPants, 1)));
        assert!(!worn.is_worn(item(Piece::LeatherPants, 0)));
        assert_eq!(worn.items().count(), 1);
    }

    #[test]
    fn taking_off_only_what_is_worn() {
        let mut worn = Equipment::default();
        assert!(!worn.take_off(item(Piece::LeatherVest, 2)));
        worn.wear(item(Piece::LeatherVest, 2));
        assert!(!worn.take_off(item(Piece::LeatherVest, 0)));
        assert!(worn.take_off(item(Piece::LeatherVest, 2)));
        assert_eq!(worn.worn(Slot::Torso), None);
    }

    #[test]
    fn pants_and_boots_take_different_slots() {
        let mut worn = Equipment::default();
        worn.wear(item(Piece::LeatherPants, 0));
        worn.wear(item(Piece::LeatherBoots, 0));
        assert_eq!(worn.items().count(), 2);
        assert_ne!(Piece::LeatherPants.slot(), Piece::LeatherBoots.slot());
    }

    /// A player starts wearing its outfit with nothing in the bag; what it
    /// takes off goes into the bag, and what it wears leaves it.
    #[test]
    fn the_bag_is_what_is_owned_and_not_worn() {
        let mut worn = Equipment::starting_outfit();
        let mut bag = Inventory::wearing(&worn);
        assert_eq!(bag.items.len(), worn.items().count());
        assert_eq!(bag.stacks(&worn), 0);
        let vest = worn.worn(Slot::Torso).unwrap();
        worn.take_off(vest);
        assert_eq!(bag.bagged(&worn).collect::<Vec<_>>(), vec![vest]);
        bag.add(of(common::Material::Softwood, 4));
        assert_eq!(bag.stacks(&worn), 2);
        bag.add(of(common::Material::Softwood, 4));
        assert_eq!(bag.stacks(&worn), 2, "a material stacks");
        worn.wear(vest);
        assert_eq!(bag.stacks(&worn), 1);
    }

    /// Weight counts what is worn and what is bagged; past the first limit
    /// a player is burdened, and nothing goes in past the second.
    #[test]
    fn weight_burdens_and_then_bars() {
        let worn = Equipment::starting_outfit();
        let mut bag = Inventory::wearing(&worn);
        assert!(bag.weight() > 0 && !bag.is_burdened());
        let stone = common::Material::Basement;
        let kind = common::Stackable::Material(stone);
        while !bag.is_burdened() {
            assert!(bag.room_for(&worn, kind) > 0);
            bag.add(of(stone, 1));
        }
        assert!(bag.weight() > BURDEN_LIMIT);
        bag.add(of(stone, bag.room_for(&worn, kind)));
        assert_eq!(bag.room_for(&worn, kind), 0);
        assert!(bag.weight() <= CARRY_LIMIT && bag.weight() + stone.weight() > CARRY_LIMIT);
    }

    /// Taking out never takes more than the bag holds; a stack emptied
    /// frees its place and leaves the others in order.
    #[test]
    fn taking_out_frees_an_emptied_stack() {
        let mut bag = Inventory::default();
        bag.add(of(common::Material::Softwood, 4));
        bag.add(of(common::Material::Limestone, 3));
        bag.add(of(common::Material::Hardwood, 2));
        let limestone = common::Stackable::Material(common::Material::Limestone);
        assert_eq!(bag.remove(limestone, 1).count, 1);
        assert_eq!(bag.count(limestone), 2);
        assert_eq!(bag.remove(limestone, 9).count, 2, "no more than the bag holds");
        assert_eq!(bag.stock, vec![of(common::Material::Softwood, 4), of(common::Material::Hardwood, 2)]);
        assert_eq!(bag.remove(limestone, 1).count, 0);
    }

    /// A new material needs a free stack; one already carried does not.
    #[test]
    fn a_full_bag_takes_only_what_it_stacks() {
        let worn = Equipment::default();
        let mut bag = Inventory::default();
        let pieces: Vec<Item> = Piece::ALL.iter().flat_map(|&piece| (0..piece.styles()).map(move |style| item(piece, style))).collect();
        bag.items = pieces.into_iter().cycle().take(BAG_STACKS - 1).collect();
        bag.add(of(common::Material::Softwood, 1));
        assert_eq!(bag.stacks(&worn), BAG_STACKS);
        assert!(bag.room_for(&worn, common::Stackable::Material(common::Material::Softwood)) > 0);
        assert_eq!(bag.room_for(&worn, common::Stackable::Material(common::Material::Hardwood)), 0);
    }

    #[test]
    fn piece_names_are_distinct_asset_stems() {
        let names: Vec<&str> = Piece::ALL.iter().map(|p| p.name()).collect();
        for name in &names {
            assert!(name.chars().all(|c| c.is_ascii_lowercase() || c == '-'), "{name}");
        }
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), names.len());
    }
}
