//! What an actor wears and what a player owns. The design is
//! `design/equipment.md` in the internal repo.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

pub use common::kit::Slot;
use common::kit::{self, Kind};

/// A piece the asset build makes, cut for every body that can wear it:
/// one of `common::kit::PIECES`, by its place there. Which pieces there
/// are is the assets' to say; the game reads what each declares.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Piece(u8);

impl Piece {
    /// Every piece, in the order the assets declare them.
    pub fn all() -> impl Iterator<Item = Piece> {
        (0..kit::PIECES.len()).map(|at| Piece(at as u8))
    }

    /// The piece the assets call `name`. Panics where they declare none:
    /// a piece the code names is one the build must make.
    pub fn named(name: &str) -> Piece {
        Piece::all().find(|piece| piece.name() == name).unwrap_or_else(|| panic!("the assets declare no piece `{name}`"))
    }

    fn declared(self) -> &'static kit::Piece {
        &kit::PIECES[self.0 as usize]
    }

    /// The number a store keeps the piece by: its own whatever the assets
    /// come to call it, where its place among them moves with every build.
    pub fn id(self) -> u32 {
        self.declared().id
    }

    /// The piece kept as `id`, or none where the assets no longer declare it.
    pub fn kept_as(id: u32) -> Option<Piece> {
        Piece::all().find(|piece| piece.id() == id)
    }

    /// The asset's name: the stem of `models/<name>-<actor>.glb`.
    pub fn name(self) -> &'static str {
        self.declared().name
    }

    /// The set the piece is of.
    pub fn set(self) -> &'static kit::Set {
        &kit::SETS[self.declared().set]
    }

    /// The slot a piece is worn in.
    pub fn slot(self) -> Slot {
        self.slots()[0]
    }

    /// Every slot a piece fills, its own first: a cuirass whose skirt
    /// hangs where a girdle would fills the waist as well, and nothing is
    /// worn under it there.
    pub fn slots(self) -> &'static [Slot] {
        self.declared().fills
    }

    /// The asset's name as a player reads it: `ranger-hood` is "Ranger Hood".
    pub fn display_name(self) -> String {
        self.name()
            .split('-')
            .map(|word| word[..1].to_uppercase() + &word[1..])
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// What the piece weighs, worn or carried: by its set's kind and the
    /// slot it goes in.
    pub fn weight(self) -> u32 {
        match (self.set().kind, self.slot()) {
            (Kind::Light, Slot::Torso) => 3,
            (Kind::Light, Slot::Legs | Slot::Feet) => 2,
            (Kind::Light, _) => 1,
            (Kind::Medium, Slot::Torso) => 6,
            (Kind::Medium, Slot::Legs) => 4,
            (Kind::Medium, Slot::Feet) => 3,
            (Kind::Medium, Slot::Waist) => 1,
            (Kind::Medium, _) => 2,
            (Kind::Heavy, Slot::Torso) => 10,
            (Kind::Heavy, Slot::Legs) => 6,
            (Kind::Heavy, Slot::Head | Slot::Feet) => 4,
            (Kind::Heavy, Slot::Waist) => 2,
            (Kind::Heavy, _) => 3,
            (Kind::Sword | Kind::Shield, _) => 3,
            (Kind::Axe | Kind::Mace, _) => 4,
            (Kind::Map, _) => 1,
        }
    }
}

/// One make of a piece: the piece, and the seed of its set it is made
/// in. A seed is the set's colours and which of the piece's shapes it is
/// cut in, the same across the set, so the pieces of one seed match.
/// Two items of one make are the same thing, but for a piece each of
/// which is one of a kind: a map is `instance`, its own number, and
/// what it has charted is kept by that.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Item {
    pub piece: Piece,
    pub seed: u16,
    pub instance: Option<u32>,
}

impl Item {
    /// The make of `piece` in `seed`, one like any other of it.
    pub fn of(piece: Piece, seed: u16) -> Item {
        Item { piece, seed, instance: None }
    }

    /// The number of the map this is, where it is one.
    pub fn map(self) -> Option<u32> {
        self.instance.filter(|_| self.piece.set().kind == Kind::Map)
    }

    /// Which of the piece's shapes this seed is cut in, its glTF scenes
    /// in order: each in turn.
    pub fn shape(self) -> usize {
        self.seed as usize % self.piece.declared().shapes as usize
    }

    /// The seed as the assets ship it: one past those shipped is drawn as
    /// one of them.
    pub fn shipped(self) -> u16 {
        self.seed % self.piece.set().seeds.len() as u16
    }
}

/// What lies in one slot of a tile or fills one cell of a bag: a stack
/// of one kind, or one piece.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Lot {
    Stack(common::Stack),
    Piece(Item),
}

impl Lot {
    /// `cover` with this left lying in slot `k`: a stack as the pile of
    /// its material, a piece in a bundle.
    pub fn left(self, cover: common::Cover, k: usize) -> common::Cover {
        match self {
            Lot::Stack(common::Stack { kind: common::Stackable::Material(material), .. }) => common::gathering::left(cover, k, material),
            Lot::Piece(_) => common::gathering::bundled(cover, k),
        }
    }

    /// Adds `count` to the stack this is. A piece is one, and takes none.
    pub fn grow(&mut self, count: u32) {
        if let Lot::Stack(stack) = self {
            stack.count += count;
        }
    }

    /// Whether `other` lies on this as one pile: a stack of its kind. A
    /// piece is its own, and nothing joins it.
    pub fn joins(self, other: Lot) -> bool {
        matches!((self, other), (Lot::Stack(a), Lot::Stack(b)) if a.kind == b.kind)
    }
}

impl From<common::Stack> for Lot {
    fn from(stack: common::Stack) -> Lot {
        Lot::Stack(stack)
    }
}

impl From<Item> for Lot {
    fn from(item: Item) -> Lot {
        Lot::Piece(item)
    }
}

/// What an actor wears, one item to a slot, a piece that fills several
/// slots (`Piece::slots`) held in each. Server authority, sent to every
/// client that sees the actor.
#[derive(Clone, Component, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Equipment {
    worn: [Option<Item>; 8],
}

impl Equipment {
    pub fn worn(&self, slot: Slot) -> Option<Item> {
        self.worn[slot.index()]
    }

    pub fn is_worn(&self, item: Item) -> bool {
        self.worn(item.piece.slot()) == Some(item)
    }

    /// Wears `item` and returns what it took off: whatever held any slot
    /// it fills, each taken off whole.
    pub fn wear(&mut self, item: Item) -> Vec<Item> {
        let mut off = Vec::new();
        for &slot in item.piece.slots() {
            if let Some(held) = self.worn(slot) {
                if held != item && self.take_off(held) {
                    off.push(held);
                }
            }
        }
        for &slot in item.piece.slots() {
            self.worn[slot.index()] = Some(item);
        }
        off
    }

    /// Takes `item` off, out of every slot it fills; false when it was not
    /// worn.
    pub fn take_off(&mut self, item: Item) -> bool {
        let worn = self.is_worn(item);
        if worn {
            for &slot in item.piece.slots() {
                self.worn[slot.index()] = None;
            }
        }
        worn
    }

    /// Every item worn, once, however many slots it fills.
    pub fn items(&self) -> impl Iterator<Item = Item> + '_ {
        Slot::ALL.into_iter().filter_map(|slot| self.worn(slot).filter(|item| item.piece.slot() == slot))
    }

    /// What a player starts wearing, and what the character screen shows:
    /// the ranger's leather, an arming sword and the sword-breaker.
    pub fn starting_outfit() -> Self {
        const SEED: u16 = 2;
        let mut outfit = Equipment::default();
        for name in ["ranger-hood", "ranger-vest", "ranger-gloves", "ranger-girdle", "ranger-pants", "ranger-boots", "arming-sword"] {
            outfit.wear(Item::of(Piece::named(name), SEED));
        }
        outfit.wear(Item::of(Piece::named("sword-breaker"), 0));
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

    /// Whether `piece` goes in: a stack free for it, and its weight
    /// under [`CARRY_LIMIT`].
    pub fn takes(&self, worn: &Equipment, piece: Piece) -> bool {
        self.stacks(worn) < BAG_STACKS && self.weight() + piece.weight() <= CARRY_LIMIT
    }

    /// Takes `item` out of the bag; false where it is worn or not owned,
    /// and nothing is taken.
    pub fn take_out(&mut self, worn: &Equipment, item: Item) -> bool {
        let at = self.items.iter().position(|held| *held == item).filter(|_| !worn.is_worn(item));
        if let Some(at) = at {
            self.items.remove(at);
        }
        at.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(name: &str, seed: u16) -> Item {
        Item::of(Piece::named(name), seed)
    }

    fn of(material: common::Material, count: u32) -> common::Stack {
        common::Stack { kind: common::Stackable::Material(material), count }
    }

    #[test]
    fn wearing_displaces_what_held_the_slot() {
        let mut worn = Equipment::default();
        assert_eq!(worn.wear(item("ranger-pants", 0)), vec![]);
        assert_eq!(worn.wear(item("ranger-pants", 1)), vec![item("ranger-pants", 0)]);
        assert!(worn.is_worn(item("ranger-pants", 1)));
        assert!(!worn.is_worn(item("ranger-pants", 0)));
        assert_eq!(worn.items().count(), 1);
    }

    #[test]
    fn taking_off_only_what_is_worn() {
        let mut worn = Equipment::default();
        assert!(!worn.take_off(item("ranger-vest", 2)));
        worn.wear(item("ranger-vest", 2));
        assert!(!worn.take_off(item("ranger-vest", 0)));
        assert!(worn.take_off(item("ranger-vest", 2)));
        assert_eq!(worn.worn(Slot::Torso), None);
    }

    /// The cuirass fills the waist too: it takes the girdle off, and a
    /// girdle worn after takes the cuirass off, whole.
    #[test]
    fn the_cuirass_is_a_girdle_too() {
        let mut worn = Equipment::default();
        worn.wear(item("ranger-vest", 0));
        worn.wear(item("ranger-girdle", 0));
        let off = worn.wear(item("sentinel-cuirass", 0));
        assert_eq!(off, vec![item("ranger-vest", 0), item("ranger-girdle", 0)]);
        assert_eq!(worn.worn(Slot::Waist), Some(item("sentinel-cuirass", 0)));
        assert_eq!(worn.items().count(), 1);
        assert_eq!(worn.wear(item("ranger-girdle", 1)), vec![item("sentinel-cuirass", 0)]);
        assert_eq!(worn.worn(Slot::Torso), None);
        assert_eq!(worn.items().collect::<Vec<_>>(), vec![item("ranger-girdle", 1)]);
    }

    #[test]
    fn pants_and_boots_take_different_slots() {
        let mut worn = Equipment::default();
        worn.wear(item("ranger-pants", 0));
        worn.wear(item("ranger-boots", 0));
        assert_eq!(worn.items().count(), 2);
        assert_ne!(Piece::named("ranger-pants").slot(), Piece::named("ranger-boots").slot());
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

    /// A piece leaves the bag only while it is not worn, goes back in
    /// while there is a stack and the weight for it, and lies on the
    /// ground as its own pile, which nothing joins.
    #[test]
    fn a_piece_is_dropped_from_the_bag_and_lies_alone() {
        let mut worn = Equipment::starting_outfit();
        let mut bag = Inventory::wearing(&worn);
        let vest = worn.worn(Slot::Torso).unwrap();
        assert!(!bag.take_out(&worn, vest), "what is worn stays on");
        worn.take_off(vest);
        assert!(bag.take_out(&worn, vest));
        assert!(!bag.contains(vest) && !bag.take_out(&worn, vest));
        assert!(bag.takes(&worn, vest.piece));
        bag.add(of(common::Material::Fibre, bag.room_for(&worn, common::Stackable::Material(common::Material::Fibre))));
        assert!(!bag.takes(&worn, vest.piece), "no weight left for it");

        let (wood, lying) = (Lot::from(of(common::Material::Softwood, 2)), Lot::from(vest));
        assert!(wood.joins(of(common::Material::Softwood, 5).into()));
        assert!(!wood.joins(of(common::Material::Hardwood, 5).into()));
        assert!(!lying.joins(lying) && !lying.joins(wood) && !wood.joins(lying));
        let bare = common::Cover::NONE;
        assert_eq!(lying.left(bare, 3).content(3), common::Content::Bundle);
        assert_eq!(wood.left(bare, 3).content(3), common::Content::Pile(common::Material::Softwood));
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
        let pieces: Vec<Item> = Piece::all().flat_map(|piece| (0..piece.set().seeds.len() as u16).map(move |seed| Item::of(piece, seed))).collect();
        bag.items = pieces.into_iter().cycle().take(BAG_STACKS - 1).collect();
        bag.add(of(common::Material::Softwood, 1));
        assert_eq!(bag.stacks(&worn), BAG_STACKS);
        assert!(bag.room_for(&worn, common::Stackable::Material(common::Material::Softwood)) > 0);
        assert_eq!(bag.room_for(&worn, common::Stackable::Material(common::Material::Hardwood)), 0);
    }

    /// A seed past a piece's shapes is cut in one of them, and one past
    /// the seeds its set ships is drawn as one of those.
    #[test]
    fn any_seed_is_a_shape_and_a_shipped_seed() {
        for piece in Piece::all() {
            for seed in [0, 1, 7, 8, 31, 1000, u16::MAX] {
                let item = Item::of(piece, seed);
                assert!(item.shape() < kit::PIECES[piece.0 as usize].shapes as usize);
                assert!((item.shipped() as usize) < piece.set().seeds.len());
            }
        }
    }

}
