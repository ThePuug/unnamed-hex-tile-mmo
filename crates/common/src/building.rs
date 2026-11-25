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
    /// A hitching post, where a settler's horse is tied.
    Post,
}

/// What a blank map is drawn on, and a copy of one: so much fibre.
pub const MAP_COST: Stack = Stack { kind: Stackable::Material(Material::Fibre), count: 6 };

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
        } else if material == Material::Hide {
            self.hide
        } else {
            0
        }
    }

    /// It and `other` together.
    pub fn plus(self, other: Needs) -> Needs {
        Needs { hide: self.hide + other.hide, wood: self.wood + other.wood, stone: self.stone + other.stone }
    }

    /// What is left of it once `other` is counted toward it.
    pub fn minus(self, other: Needs) -> Needs {
        Needs { hide: self.hide.saturating_sub(other.hide), wood: self.wood.saturating_sub(other.wood), stone: self.stone.saturating_sub(other.stone) }
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
            let want = if material.is_wood() {
                &mut left.wood
            } else if material.is_stone() {
                &mut left.stone
            } else if material == Material::Hide {
                &mut left.hide
            } else {
                continue;
            };
            let count = stack.count.min(*want);
            *want -= count;
            if count > 0 {
                taken.push(Stack { kind: stack.kind, count });
            }
        }
        Some(taken)
    }
}

/// What a settler does to a tile: raises a fixture on it, lays its hub's
/// hearth there, or takes down what a plan gave up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Task {
    Raise(Fixture),
    Hearth,
    Clear,
}

/// What a settler says to a player who hails it: what it does next, none
/// once the ground is as its hub's plan says, and what its camp still
/// lacks for all it has yet to raise.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wants {
    pub task: Option<Task>,
    pub needs: Needs,
}

/// What a settler says a plan would do: the tiles it builds on and gives
/// up, the tiles it raises stakes on and pulls them from, the tiles its
/// path comes to cross and those it crosses no more, how many hubs it
/// takes in or parts off, and what its camp still lacks for it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Planned {
    /// A tier up, or else drawn in to what its people hold.
    pub grow: bool,
    pub built: Vec<(i32, i32)>,
    pub gone: Vec<(i32, i32)>,
    pub raised: Vec<(i32, i32)>,
    pub pulled: Vec<(i32, i32)>,
    pub worn: Vec<(i32, i32)>,
    pub faded: Vec<(i32, i32)>,
    pub hubs: u32,
    pub needs: Needs,
}

/// What a settler with nothing more to raise says of its wagon: what
/// is loaded on it, and the hubs the map in the player's hand would send
/// it to, each by the tile it is known by, none with no such map.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wagon {
    pub load: Vec<Stack>,
    pub hubs: Vec<(i32, i32)>,
}

impl Fixture {
    pub const ALL: [Fixture; 4] = [Fixture::Tent, Fixture::Stakes, Fixture::Wagon, Fixture::Post];

    pub fn name(self) -> &'static str {
        match self {
            Fixture::Tent => "Tent",
            Fixture::Stakes => "Stakes",
            Fixture::Wagon => "Wagon",
            Fixture::Post => "Hitching post",
        }
    }

    /// The name its model is filed under: `models/<stem>.glb`.
    pub fn stem(self) -> &'static str {
        match self {
            Fixture::Tent => "tent",
            Fixture::Stakes => "stakes",
            Fixture::Wagon => "wagon",
            Fixture::Post => "hitching-post",
        }
    }

    /// What raising it takes: hide over wood for a tent, a great deal of
    /// wood for a wagon, and for stakes the wood of one tile of a wall.
    pub fn needs(self) -> Needs {
        match self {
            Fixture::Tent => Needs { hide: 4, wood: 4, stone: 0 },
            Fixture::Stakes => Needs { hide: 0, wood: 4, stone: 0 },
            Fixture::Wagon => Needs { hide: 0, wood: 24, stone: 0 },
            Fixture::Post => Needs { hide: 0, wood: 6, stone: 0 },
        }
    }

    fn content(self) -> Content {
        match self {
            Fixture::Tent => Content::Tent,
            Fixture::Stakes => Content::Stakes,
            Fixture::Wagon => Content::Wagon,
            Fixture::Post => Content::Post,
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

/// `cover` with the horse tied at its hitching post led away, the post
/// left standing empty; None where no horse is tied there.
pub fn untied(cover: Cover) -> Option<Cover> {
    (cover.content(SLOT) == Content::Post).then(|| cover.with_content(SLOT, Content::PostEmpty))
}

/// `cover` with a horse tied at its empty hitching post; None where no
/// empty post stands there.
pub fn tied(cover: Cover) -> Option<Cover> {
    emptied_post(cover).then(|| cover.with_content(SLOT, Content::Post))
}

/// Whether a hitching post stands empty on a tile with `cover`.
pub fn emptied_post(cover: Cover) -> bool {
    cover.content(SLOT) == Content::PostEmpty
}

/// `cover` with the wagon standing on it gone, out on the road; None
/// where none stands there.
pub fn driven(cover: Cover) -> Option<Cover> {
    (fixture(cover) == Some(Fixture::Wagon)).then(|| cover.with_content(SLOT, Content::Empty))
}

/// `cover` with a wagon come to stand on it, whole; None where the tile
/// cannot take one ([`Fixture::begun`]).
pub fn parked(cover: Cover) -> Option<Cover> {
    Fixture::Wagon.begun(cover).map(|cover| cover.with_content(SLOT, Content::Wagon))
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

/// `cover` with whatever is built on it taken down by its hub, every slot
/// it held free, and what its hub has of it to build with again: all it
/// took. None where nothing built stands there.
pub fn cleared(cover: Cover) -> Option<(Cover, Needs)> {
    let back = match cover.content(SLOT) {
        Content::PostEmpty => Fixture::Post.needs(),
        Content::Raising(fixture, _) => fixture.needs(),
        _ => match (fixture(cover), standing(cover)) {
            (Some(fixture), _) => fixture.needs(),
            (None, Some(construct)) if construct.of.is_wood() => Needs { wood: construct.kind.cost(), ..Needs::default() },
            (None, Some(construct)) => Needs { stone: construct.kind.cost(), ..Needs::default() },
            (None, None) => return None,
        },
    };
    let filled = |k: usize| k == SLOT || cover.content(k) == Content::Filled;
    Some(((0..TILE_SLOTS as usize).filter(|&k| filled(k)).fold(cover, |cover, k| cover.with_content(k, Content::Empty)), back))
}

/// Whether G uses what stands in slot `k`: a hearth or a tent, to make a
/// home of.
pub fn used(cover: Cover, k: usize) -> bool {
    k == SLOT && matches!(cover.content(k), Content::Hearth(_) | Content::Tent)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A wagon leaves its tile open and comes to stand on an open one.
    #[test]
    fn a_wagon_leaves_its_tile_and_stands_again() {
        let stood = parked(Cover::NONE.with(0, Content::Brush)).unwrap();
        assert_eq!(fixture(stood), Some(Fixture::Wagon));
        assert_eq!(driven(stood), Some(Cover::NONE));
        assert_eq!((driven(Cover::NONE), parked(stood)), (None, None));
    }

    /// A horse is led from its post and tied at it again, and the post
    /// is kept as it stands either way.
    #[test]
    fn a_horse_is_led_from_its_post_and_tied_again() {
        let mut cover = Fixture::Post.begun(Cover::NONE).unwrap();
        for _ in 0..STAGES {
            assert_eq!(Cover::from_bits(cover.bits()), cover);
            cover = advanced(cover).unwrap();
        }
        assert_eq!(fixture(cover), Some(Fixture::Post));
        let empty = untied(cover).unwrap();
        assert!(emptied_post(empty) && fixture(empty).is_none());
        assert_eq!((Cover::from_bits(empty.bits()), tied(empty), untied(empty), tied(cover)), (empty, Some(cover), None, None));
    }

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
