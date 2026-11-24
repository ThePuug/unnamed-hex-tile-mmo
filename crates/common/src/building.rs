//! What building puts on a tile and taking down takes off it: the one
//! rule the server applies and the client offers its choices by.

use serde::{Deserialize, Serialize};

use crate::cover::{Content, Cover, TILE_SLOTS};
use crate::gathering::{left, Material, Stack, Stackable, Work};

/// The slot a construct stands in: the tile's centre. It holds the other
/// six besides.
pub const SLOT: usize = 0;

/// What a construct is, whatever it is built of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Kind {
    /// Founds a hub, and is a home.
    Hearth,
    /// Sharpened stakes filling their tile, as a camp's fortification
    /// does.
    Blockade,
}

impl Kind {
    pub const ALL: [Kind; 2] = [Kind::Hearth, Kind::Blockade];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Hearth => "Hearth",
            Kind::Blockade => "Blockade",
        }
    }

    /// How many of its material one takes: less than a load, so a player
    /// carries a construct whole.
    pub fn cost(self) -> u32 {
        match self {
            Kind::Hearth => 6,
            Kind::Blockade => 4,
        }
    }

    /// Whether one is built of `material`: a hearth of stone, a blockade
    /// of wood.
    pub fn built_of(self, material: Material) -> bool {
        match self {
            Kind::Hearth => material.is_stone(),
            Kind::Blockade => material.is_wood(),
        }
    }
}

/// A construct: a kind in one material.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Construct {
    pub kind: Kind,
    pub of: Material,
}

impl Construct {
    /// Every construct there is, a kind after a kind and each in every
    /// material it is built of.
    pub fn all() -> impl Iterator<Item = Construct> {
        Kind::ALL.into_iter().flat_map(|kind| Material::ALL.into_iter().filter(move |&of| kind.built_of(of)).map(move |of| Construct { kind, of }))
    }

    pub fn name(self) -> String {
        format!("{} {}", self.of.name(), self.kind.name().to_lowercase())
    }

    /// What building one spends, and taking it down gives back.
    pub fn cost(self) -> Stack {
        Stack { kind: Stackable::Material(self.of), count: self.kind.cost() }
    }

    /// The work building it or taking it down shows: stone is struck and
    /// wood is chopped.
    pub fn work(self) -> Work {
        if self.of.is_stone() { Work::Mine } else { Work::Chop }
    }

    fn content(self) -> Content {
        match self.kind {
            Kind::Hearth => Content::Hearth(self.of),
            Kind::Blockade => Content::Blockade(self.of),
        }
    }

    /// `cover` with the construct standing on it, or None where the tile
    /// cannot take it: its kind is not built of its material, or a slot
    /// of the tile holds more than brush.
    pub fn raised(self, cover: Cover) -> Option<Cover> {
        let free = |k: usize| matches!(cover.content(k), Content::Empty | Content::Brush);
        (self.kind.built_of(self.of) && (0..TILE_SLOTS as usize).all(free)).then(|| stood(cover, self.content()))
    }
}

/// What a hub's settlers raise for it, and no player builds or takes
/// down.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Fixture {
    /// A home, as a hearth is, once a player claims it.
    Tent,
    /// Sharpened stakes filling their tile: the fortification a camp's
    /// ground is bordered by.
    Stakes,
    Wagon,
}

/// What raising a fixture takes: so many hides, so much wood of either
/// kind and so much stone of any.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Needs {
    pub hide: u32,
    pub wood: u32,
    pub stone: u32,
}

impl Needs {
    pub fn is_none(self) -> bool {
        self == Needs::default()
    }

    /// How much of `material` it takes: of its class, whichever of the
    /// class it is.
    pub fn of(self, material: Material) -> u32 {
        if material.is_wood() {
            self.wood
        } else if material.is_stone() {
            self.stone
        } else {
            self.hide
        }
    }

    /// What is left of it to find once `held` is counted toward it.
    pub fn less(self, held: &[Stack]) -> Needs {
        let have = |class: fn(Material) -> bool| held.iter().map(|stack| match stack.kind { Stackable::Material(m) if class(m) => stack.count, _ => 0 }).sum::<u32>();
        Needs {
            hide: self.hide.saturating_sub(have(|m| m == Material::Hide)),
            wood: self.wood.saturating_sub(have(Material::is_wood)),
            stone: self.stone.saturating_sub(have(Material::is_stone)),
        }
    }

    /// What it takes out of `held`, in the order `held` holds it, or none
    /// where `held` falls short of it.
    pub fn taken(self, held: &[Stack]) -> Option<Vec<Stack>> {
        if !self.less(held).is_none() {
            return None;
        }
        let mut left = self;
        let mut taken = Vec::new();
        for stack in held {
            let Stackable::Material(material) = stack.kind;
            let want = if material.is_wood() { &mut left.wood } else if material.is_stone() { &mut left.stone } else { &mut left.hide };
            let count = stack.count.min(*want);
            *want -= count;
            if count > 0 {
                taken.push(Stack { kind: stack.kind, count });
            }
        }
        Some(taken)
    }
}

/// What a settler says to a player who hails it: what it raises next,
/// none once its camp's line is done, and what of that it still lacks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wants {
    pub raising: Option<Fixture>,
    pub needs: Needs,
}

impl Fixture {
    pub const ALL: [Fixture; 3] = [Fixture::Tent, Fixture::Stakes, Fixture::Wagon];

    pub fn name(self) -> &'static str {
        match self {
            Fixture::Tent => "Tent",
            Fixture::Stakes => "Stakes",
            Fixture::Wagon => "Wagon",
        }
    }

    /// The name its model is filed under: `models/<stem>.glb`.
    pub fn stem(self) -> &'static str {
        match self {
            Fixture::Tent => "tent",
            Fixture::Stakes => "stakes",
            Fixture::Wagon => "wagon",
        }
    }

    /// What raising it takes: hide over wood for a tent, a great deal of
    /// wood for a wagon, and for stakes the wood and stone of a whole
    /// fortification, every stake along a hub's edge, found once before
    /// the first is raised.
    pub fn needs(self) -> Needs {
        match self {
            Fixture::Tent => Needs { hide: 4, wood: 4, stone: 0 },
            Fixture::Stakes => Needs { hide: 0, wood: 16, stone: 8 },
            Fixture::Wagon => Needs { hide: 0, wood: 24, stone: 0 },
        }
    }

    fn content(self) -> Content {
        match self {
            Fixture::Tent => Content::Tent,
            Fixture::Stakes => Content::Stakes,
            Fixture::Wagon => Content::Wagon,
        }
    }

    /// `cover` with the fixture begun on it, at its first stage, its
    /// brush cleared, or None where the tile cannot take it: any slot of
    /// it holds more than brush. A tent and a wagon want the whole tile
    /// clear though they hold its centre alone, so each stands in the
    /// open.
    pub fn begun(self, cover: Cover) -> Option<Cover> {
        let free = |k: usize| matches!(cover.content(k), Content::Empty | Content::Brush);
        let cleared = || (0..TILE_SLOTS as usize).fold(cover, |cover, k| cover.with_content(k, Content::Empty));
        (0..TILE_SLOTS as usize).all(free).then(|| cleared().with_content(SLOT, Content::Raising(self, 1)))
    }
}

/// How many stages a fixture stands at between begun and raised, each
/// with a look of its own.
pub const STAGES: u8 = 3;

/// `cover` with the fixture being raised on it a stage further, raised
/// from its last, or None where none is being raised there.
pub fn advanced(cover: Cover) -> Option<Cover> {
    let (fixture, stage) = rising(cover)?;
    Some(match (fixture, stage < STAGES) {
        (_, true) => cover.with_content(SLOT, Content::Raising(fixture, stage + 1)),
        (Fixture::Stakes, false) => stood(cover, Content::Stakes),
        (_, false) => cover.with_content(SLOT, fixture.content()),
    })
}

/// The fixture standing on a tile with `cover`.
pub fn fixture(cover: Cover) -> Option<Fixture> {
    Fixture::ALL.into_iter().find(|fixture| cover.content(SLOT) == fixture.content())
}

/// The fixture begun and not yet raised on a tile with `cover`, and the
/// stage it stands at, from 1.
pub fn rising(cover: Cover) -> Option<(Fixture, u8)> {
    match cover.content(SLOT) {
        Content::Raising(fixture, stage) => Some((fixture, stage)),
        _ => None,
    }
}

/// `cover` with `content`, a construct or stakes, in the centre and
/// every other slot filled by it.
fn stood(cover: Cover, content: Content) -> Cover {
    (0..TILE_SLOTS as usize).fold(cover, |cover, k| cover.with_content(k, if k == SLOT { content } else { Content::Filled }))
}

/// The construct standing on a tile with `cover`.
pub fn standing(cover: Cover) -> Option<Construct> {
    match cover.content(SLOT) {
        Content::Hearth(of) => Some(Construct { kind: Kind::Hearth, of }),
        Content::Blockade(of) => Some(Construct { kind: Kind::Blockade, of }),
        _ => None,
    }
}

/// `cover` with its construct taken down: every slot it held free, and
/// what it was built of lying whole as a pile in the centre. None where no
/// construct stands.
pub fn razed(cover: Cover) -> Option<(Cover, Stack)> {
    let construct = standing(cover)?;
    let cleared = (0..TILE_SLOTS as usize).fold(cover, |cover, k| cover.with_content(k, Content::Empty));
    Some((left(cleared, SLOT, construct.of), construct.cost()))
}

/// Whether G uses what stands in slot `k`: a hearth or a tent, to make a
/// home of.
pub fn used(cover: Cover, k: usize) -> bool {
    k == SLOT && matches!(cover.content(k), Content::Hearth(_) | Content::Tent)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OAK: Construct = Construct { kind: Kind::Blockade, of: Material::Hardwood };

    /// A blockade fills an empty tile and clears its brush; a tree, a
    /// stump, a boulder or a pile in the way refuses it, and it is built
    /// of wood alone. Taken down, the tile is bare but for the pile of
    /// what it was built of.
    #[test]
    fn a_blockade_fills_its_tile_and_comes_down_whole() {
        let brush = Cover::NONE.with(0, Content::Brush);
        let walled = OAK.raised(brush).unwrap();
        assert_eq!(walled.fullness(), TILE_SLOTS);
        assert_eq!(standing(walled), Some(OAK));
        assert_eq!(Construct { kind: Kind::Blockade, of: Material::Limestone }.raised(Cover::NONE), None);
        assert_eq!(Cover::from_bits(walled.bits()), walled);
        for held in [Cover::NONE.with(0, Content::Pine), Cover::NONE.with_boulder(3), left(Cover::NONE, 5, Material::Softwood), walled] {
            assert_eq!(OAK.raised(held), None, "{held:?}");
        }
        let stump = crate::gathering::harvest(Cover::NONE.with(0, Content::Pine), crate::SITE_SLOTS[0][0]).unwrap().cover;
        assert_eq!(OAK.raised(stump), None, "a stump is gathered first");
        let (bare, stack) = razed(walled).unwrap();
        assert_eq!(stack, OAK.cost());
        assert_eq!(bare, left(Cover::NONE, SLOT, Material::Hardwood));
        assert_eq!(razed(Cover::NONE), None);
    }

    /// A hearth fills its tile as a blockade does, so a walker goes round it,
    /// wants the whole tile clear, and is built of stone alone.
    #[test]
    fn a_hearth_fills_its_tile() {
        let hearth = Construct { kind: Kind::Hearth, of: Material::Basement };
        let lit = hearth.raised(Cover::NONE.with(1, Content::Brush)).unwrap();
        assert_eq!((standing(lit), lit.fullness()), (Some(hearth), TILE_SLOTS));
        assert!(used(lit, SLOT) && !used(lit, 1));
        assert_eq!(razed(lit).unwrap().0, left(Cover::NONE, SLOT, Material::Basement));
        assert_eq!(hearth.raised(Cover::NONE.with(0, Content::Pine)), None, "a tree on its tile is felled first");
        assert_eq!(hearth.raised(Cover::NONE.with_boulder(SLOT)), None);
        assert_eq!(Construct { kind: Kind::Hearth, of: Material::Softwood }.raised(Cover::NONE), None);
        assert!(Kind::ALL.iter().all(|kind| !kind.built_of(Material::Hide)), "nothing is built of hide");
    }

    /// A fixture is begun on a clear tile, in no one's way, and carried
    /// a stage at a time until it is raised, each stage kept as itself;
    /// stakes fill their tile and a tent leaves its tile walked; none is
    /// a construct a player takes down, and nothing else is begun where
    /// one is.
    #[test]
    fn a_fixture_is_begun_in_the_open_and_raised_by_stages() {
        assert_eq!(advanced(Cover::NONE), None, "nothing is raised that was not begun");
        let raised = |fixture: Fixture| {
            let mut cover = fixture.begun(Cover::NONE.with(1, Content::Brush)).unwrap();
            for stage in 1..=STAGES {
                assert_eq!((rising(Cover::from_bits(cover.bits())), super::fixture(cover), cover.fullness()), (Some((fixture, stage)), None, 0));
                assert_eq!(razed(cover), None);
                assert!(Fixture::ALL.iter().all(|other| other.begun(cover).is_none()));
                cover = advanced(cover).unwrap();
            }
            cover
        };
        for fixture in Fixture::ALL {
            let up = raised(fixture);
            assert_eq!((super::fixture(Cover::from_bits(up.bits())), rising(up), advanced(up)), (Some(fixture), None, None));
            assert_eq!((standing(up), razed(up)), (None, None));
            assert_eq!(fixture.begun(Cover::NONE.with_boulder(4)), None);
            assert!(Fixture::ALL.iter().all(|other| other.begun(up).is_none()));
        }
        assert_eq!(raised(Fixture::Stakes).fullness(), TILE_SLOTS);
        assert!(raised(Fixture::Tent).fullness() == 1 && used(raised(Fixture::Tent), SLOT));
    }

    /// Needs are met by a class of material, whichever of it is held, and
    /// take no more than they are.
    #[test]
    fn needs_are_met_by_any_wood_and_any_stone() {
        let of = |material, count| Stack { kind: Stackable::Material(material), count };
        let needs = Fixture::Tent.needs();
        let short = [of(Material::Softwood, 1), of(Material::Hide, 9)];
        assert_eq!(needs.less(&short), Needs { hide: 0, wood: needs.wood - 1, stone: 0 });
        assert_eq!(needs.taken(&short), None);
        let held = [of(Material::Softwood, 1), of(Material::Hide, 9), of(Material::Hardwood, 9), of(Material::Basement, 2)];
        let taken = needs.taken(&held).unwrap();
        assert_eq!(taken, vec![of(Material::Softwood, 1), of(Material::Hide, needs.hide), of(Material::Hardwood, needs.wood - 1)]);
        assert!(needs.less(&taken).is_none());
        assert_eq!((needs.of(Material::Hardwood), needs.of(Material::Limestone)), (needs.wood, 0));
    }

    /// Every construct is listed once, and a player carries any of them
    /// under the weight that slows it.
    #[test]
    fn every_construct_is_a_load() {
        let all: Vec<Construct> = Construct::all().collect();
        assert_eq!(all.len(), 3 + 2);
        assert!(all.iter().all(|c| c.raised(Cover::NONE).is_some()));
        assert!(all.iter().all(|c| c.cost().weight() < 60), "the burden limit");
    }
}
