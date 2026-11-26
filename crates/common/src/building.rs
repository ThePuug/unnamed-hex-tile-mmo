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
    /// Where a hub keeps what players leave in it and wagons bring them,
    /// each player's its own to take. It fills its tile.
    Storehouse,
    /// The homes a tent is upgraded to, each on more tiles than the last
    /// ([`Fixture::tiles`]): a pavilion, a house and a tenement.
    Pavilion,
    House,
    Tenement,
}

/// Which way a player moves a stack at a storehouse: out of its bag into
/// the store, out of the store into its bag, out of the store onto its
/// hub's wagon, or off that wagon into the store. What is its own stays
/// its own in the store and on the wagon.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Stowing {
    Leave,
    Take,
    Load,
    Unload,
}

impl Stowing {
    /// Moving a stack picked at a storehouse, from what is stored where
    /// `stored` and else from the other side, the wagon where `wagon` and
    /// else the bag.
    pub fn of(stored: bool, wagon: bool) -> Stowing {
        match (stored, wagon) {
            (true, false) => Stowing::Take,
            (true, true) => Stowing::Load,
            (false, false) => Stowing::Leave,
            (false, true) => Stowing::Unload,
        }
    }

    /// What it does, said as the panel's heading.
    pub fn says(self) -> &'static str {
        match self {
            Stowing::Leave => "Deposit: bag to storehouse",
            Stowing::Take => "Withdraw: storehouse to bag",
            Stowing::Load => "Load: storehouse to wagon",
            Stowing::Unload => "Unload: wagon to storehouse",
        }
    }
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

/// What a settler says to a player who hails it: what its hub's planners
/// do next, none once the ground is as its plans say, what its camp still
/// lacks for all it has yet to raise, and how its hub stands: how many
/// residents it is planned for, which zones have a planner, each a bit by
/// the number the zone is kept by, the zone it may hire one for next,
/// none as 0, whether its wall is to be raised, the zone the settler
/// hailed is the planner of, none as 0: what it does next, and what it
/// may be asked for, are that zone's; and whether it is busy, its task
/// begun and not yet done, and takes no order until it is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wants {
    pub task: Option<Task>,
    pub needs: Needs,
    pub planned: u32,
    pub hired: u8,
    pub hires: u8,
    pub walled: bool,
    pub zone: u8,
    pub busy: bool,
}

/// What a player asks of a hub's planners. A zone is named by the number
/// it is kept by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Order {
    /// The hub drawn in to what its residents defend.
    #[default]
    Shrink,
    /// The hub planned for so many more residents, or fewer.
    Residents(i32),
    /// Its wall raised.
    Wall,
    /// A fixture raised where its zone's planner puts it.
    Raise(Fixture),
    /// A planner hired for a zone, or for the next the hub may hire one
    /// for where the zone is 0.
    Hire(u32),
    /// A zone centred on a tile.
    Zone(u32, i32, i32),
    /// What stands on a tile upgraded, or downgraded.
    Upgrade(i32, i32),
    Downgrade(i32, i32),
}

impl Order {
    /// Whether it takes materials before it is carried out: one that
    /// only gives ground or buildings up is carried whatever its camp
    /// has.
    pub fn builds(self) -> bool {
        !matches!(self, Order::Shrink | Order::Downgrade(..))
    }
}

/// What a settler says a plan would do: the order it answers, the tiles
/// it builds on and gives up, the tiles it raises stakes on and pulls
/// them from, the tiles its path comes to cross and those it crosses no
/// more, how many hubs it takes in or parts off, and what its camp still
/// lacks for it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Planned {
    pub order: Order,
    pub built: Vec<(i32, i32)>,
    pub gone: Vec<(i32, i32)>,
    pub raised: Vec<(i32, i32)>,
    pub pulled: Vec<(i32, i32)>,
    pub worn: Vec<(i32, i32)>,
    pub faded: Vec<(i32, i32)>,
    pub hubs: u32,
    pub needs: Needs,
}

/// A thing of a hub as its planning shows it: what it is and its level,
/// by the numbers they are kept by, the tiles it lies on, who lives in
/// it, and whether it stands yet or is only planned.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Laid {
    pub what: u8,
    pub level: u8,
    pub tiles: Vec<(i32, i32)>,
    pub players: u8,
    pub settlers: u8,
    pub stands: bool,
}

/// A hub as its planning shows it: how many live in it and how many it
/// is planned for; which zones have a planner, which of them are at
/// work, each a bit by the number the zone is kept by, and the zone it
/// may hire one for next, none as 0; whether its wall is to be raised;
/// the ground it claims, the tiles its wall runs on, the tiles nothing
/// is planned on as the ground lies, each zone's centre and every thing
/// of it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Layout {
    pub residents: u32,
    pub wants: u32,
    pub hired: u8,
    pub busy: u8,
    pub hires: u8,
    pub walled: bool,
    pub ground: Vec<(i32, i32)>,
    pub wall: Vec<(i32, i32)>,
    pub barred: Vec<(i32, i32)>,
    pub zones: Vec<(u8, i32, i32)>,
    pub items: Vec<Laid>,
}

/// What a settler with nothing more to raise says of its wagon: what
/// is loaded on it, and the hubs the map in the player's hand would send
/// it to, each by the tile it is known by, none with no such map. Away,
/// it is the driver of a wagon stopped on the road that speaks: of what
/// the wagon carries, and its own hub is among those it may be sent to.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wagon {
    pub load: Vec<Stack>,
    pub hubs: Vec<(i32, i32)>,
    pub away: bool,
}

impl Fixture {
    pub const ALL: [Fixture; 8] = [Fixture::Tent, Fixture::Stakes, Fixture::Wagon, Fixture::Post, Fixture::Storehouse, Fixture::Pavilion, Fixture::House, Fixture::Tenement];

    pub fn name(self) -> &'static str {
        match self {
            Fixture::Tent => "Tent",
            Fixture::Stakes => "Stakes",
            Fixture::Wagon => "Wagon",
            Fixture::Post => "Hitching post",
            Fixture::Storehouse => "Storehouse",
            Fixture::Pavilion => "Pavilion",
            Fixture::House => "House",
            Fixture::Tenement => "Tenement",
        }
    }

    /// The name its model is filed under: `models/<stem>.glb`.
    pub fn stem(self) -> &'static str {
        match self {
            Fixture::Tent => "tent",
            Fixture::Stakes => "stakes",
            Fixture::Wagon => "wagon",
            Fixture::Post => "hitching-post",
            Fixture::Storehouse => "storehouse",
            Fixture::Pavilion => "pavilion",
            Fixture::House => "house",
            Fixture::Tenement => "tenement",
        }
    }

    /// What raising it takes: hide over wood for a tent, a great deal of
    /// wood for a wagon, wood on a stone footing for a storehouse, and
    /// for stakes the wood of one tile of a wall.
    pub fn needs(self) -> Needs {
        match self {
            Fixture::Tent => Needs { hide: 4, wood: 4, stone: 0 },
            Fixture::Stakes => Needs { hide: 0, wood: 4, stone: 0 },
            Fixture::Wagon => Needs { hide: 0, wood: 24, stone: 0 },
            Fixture::Post => Needs { hide: 0, wood: 6, stone: 0 },
            Fixture::Storehouse => Needs { hide: 0, wood: 16, stone: 4 },
            Fixture::Pavilion => Needs { hide: 20, wood: 20, stone: 0 },
            Fixture::House => Needs { hide: 0, wood: 60, stone: 36 },
            Fixture::Tenement => Needs { hide: 0, wood: 128, stone: 96 },
        }
    }

    /// Whether it fills its tile once raised, so a walker goes round it.
    pub fn fills(self) -> bool {
        !matches!(self, Fixture::Wagon)
    }

    fn content(self) -> Content {
        match self {
            Fixture::Tent => Content::Tent,
            Fixture::Stakes => Content::Stakes,
            Fixture::Wagon => Content::Wagon,
            Fixture::Post => Content::Post,
            Fixture::Storehouse => Content::Storehouse,
            Fixture::Pavilion => Content::Pavilion,
            Fixture::House => Content::House,
            Fixture::Tenement => Content::Tenement,
        }
    }

    /// The tiles it stands on, about the first, which holds it: the
    /// footprint its model is built on, before it is turned
    /// ([`turned`]).
    pub fn tiles(self) -> &'static [(i32, i32)] {
        match self {
            Fixture::Pavilion => &[(0, 0), (1, 0), (0, 1)],
            Fixture::House => &[(0, 0), (1, 0), (0, 1), (1, 1)],
            Fixture::Tenement => &[(0, 0), (1, 0), (2, 0), (0, 1), (1, 1), (0, 2)],
            _ => &[(0, 0)],
        }
    }

    /// Where it stands to lie on exactly `tiles`: the tile that holds it
    /// and how many sixths of a turn it is turned. None where its
    /// footprint lies on them no way.
    pub fn lies(self, tiles: &[(i32, i32)]) -> Option<((i32, i32), u8)> {
        let mut wanted = tiles.to_vec();
        wanted.sort();
        (0..6u8).flat_map(|turn| tiles.iter().map(move |&first| (first, turn))).find(|&(first, turn)| {
            let mut lain: Vec<(i32, i32)> = self.tiles().iter().map(|&tile| { let (q, r) = turned(tile, turn); (first.0 + q, first.1 + r) }).collect();
            lain.sort();
            lain == wanted
        })
    }

    /// The tiles it lies on standing on `first`, turned `turn` sixths.
    pub fn lain(self, first: (i32, i32), turn: u8) -> Vec<(i32, i32)> {
        self.tiles().iter().map(|&tile| { let (q, r) = turned(tile, turn); (first.0 + q, first.1 + r) }).collect()
    }

    /// `cover` with the fixture begun on it, at its first stage, its
    /// brush cleared, or None where the tile cannot take it: any slot of
    /// it holds more than brush. A wagon wants the whole tile
    /// clear though it holds its centre alone, so it stands in the
    /// open. One begun fills no slot until it is raised.
    pub fn begun(self, cover: Cover) -> Option<Cover> {
        let free = |k: usize| matches!(cover.content(k), Content::Empty | Content::Brush);
        let cleared = || (0..TILE_SLOTS as usize).fold(cover, |cover, k| cover.with_content(k, Content::Empty));
        (0..TILE_SLOTS as usize).all(free).then(|| cleared().with_content(SLOT, Content::Raising(self, 1)))
    }
}

/// The step `tile` over the grid, turned `turn` sixths of a turn about
/// the tile it is counted from.
pub fn turned(tile: (i32, i32), turn: u8) -> (i32, i32) {
    (0..turn % 6).fold(tile, |(q, r), _| (-r, q + r))
}

/// How many sixths of a turn the building on a tile with `cover` is
/// turned: the way its cover faces, none as none.
pub fn turn(cover: Cover) -> u8 {
    cover.facing().map_or(0, |facing| (facing / 2) as u8)
}

/// `cover` with the building on it turned `turn` sixths of a turn.
pub fn turning(cover: Cover, turn: u8) -> Cover {
    cover.faced(Some((turn % 6) as usize * 2))
}

/// `cover` as a tile of a building that stands beside it, its brush
/// cleared, or None where a slot of it holds more than brush. It faces
/// no way: [`annexed_to`] says where its building stands.
pub fn annexed(cover: Cover) -> Option<Cover> {
    let free = |k: usize| matches!(cover.content(k), Content::Empty | Content::Brush);
    (0..TILE_SLOTS as usize).all(free).then(|| stood(cover, Content::Annex).faced(None))
}

/// `cover`, on `tile`, as a tile of the building that stands on
/// `holder`: it faces the tile of that building next toward it, which is
/// the holder itself or another of its tiles, so [`holder`] finds the
/// building from any of them by steps and no search. None where the tile
/// cannot be one, or lies further off than any building's tile does.
pub fn annexed_to(cover: Cover, tile: (i32, i32), holder: (i32, i32)) -> Option<Cover> {
    let (q, r) = (holder.0 - tile.0, holder.1 - tile.1);
    // Two tiles off in a line, the tile between is the building's too
    let step = if q % 2 == 0 && r % 2 == 0 { (q / 2, r / 2) } else { (q, r) };
    let toward = crate::cover::FACINGS.iter().position(|&facing| facing == step)?;
    Some(annexed(cover)?.faced(Some(toward)))
}

/// The tile the building `tile` is one of stands on, read through
/// `cover_at`: `tile` itself where it is no building's other tile.
pub fn holder(cover_at: impl Fn((i32, i32)) -> Cover, tile: (i32, i32)) -> (i32, i32) {
    let mut at = tile;
    // A building's furthest tile is two steps from the one that holds it
    for _ in 0..2 {
        let cover = cover_at(at);
        let Some(toward) = cover.facing().filter(|_| annex(cover)) else { break };
        let (q, r) = crate::cover::FACINGS[toward];
        at = (at.0 + q, at.1 + r);
    }
    at
}

/// Whether a tile with `cover` is one of a building that stands beside
/// it.
pub fn annex(cover: Cover) -> bool {
    cover.content(SLOT) == Content::Annex
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
        (_, false) if fixture.fills() => stood(cover, fixture.content()),
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
        Content::Annex => Needs::default(),
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
    Some(((0..TILE_SLOTS as usize).filter(|&k| filled(k)).fold(cover.faced(None), |cover, k| cover.with_content(k, Content::Empty)), back))
}

/// How near a player hails a settler from, and looks into a wagon or a
/// storehouse from, in tiles, any way it faces. The server reaches by it
/// and the client aims by it.
pub const HAIL: i32 = 2;

/// Whether a player claims what stands in slot `k`: a hearth, to return
/// to, or a home, to live in. A home on more tiles than one is claimed
/// on the tile that holds it.
pub fn used(cover: Cover, k: usize) -> bool {
    k == SLOT && matches!(cover.content(k), Content::Hearth(_) | Content::Tent | Content::Pavilion | Content::House | Content::Tenement)
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
        assert!(Fixture::ALL.into_iter().all(|fixture| (raised(fixture).fullness() == TILE_SLOTS) == fixture.fills()));
        assert!(raised(Fixture::Wagon).fullness() == 1 && used(raised(Fixture::Tent), SLOT));
    }

    /// A home on more tiles than one is found again from the tiles it
    /// lies on, wherever it stands and however it is turned, and fills
    /// every one of them.
    #[test]
    fn a_building_is_found_from_the_tiles_it_lies_on() {
        for fixture in Fixture::ALL {
            for turn in 0..6 {
                let lain = fixture.lain((7, -3), turn);
                assert_eq!(lain.len(), fixture.tiles().len());
                let (first, found) = fixture.lies(&lain).unwrap();
                let mut again = fixture.lain(first, found);
                let mut lain = lain;
                (again.sort(), lain.sort());
                assert_eq!(again, lain, "{fixture:?} turned {turn}");
            }
        }
        assert_eq!(Fixture::Pavilion.lies(&[(0, 0), (1, 0), (2, 0)]), None, "three in a row is no pavilion");
        let held = annexed(Cover::NONE).unwrap();
        assert!(annex(held) && held.fullness() == TILE_SLOTS && cleared(held) == Some((Cover::NONE, Needs::default())));
        // Every tile of a building, however it is turned, leads to the one that holds it
        for fixture in Fixture::ALL {
            for turn in 0..6 {
                let (first, lain) = ((7, -3), fixture.lain((7, -3), turn));
                let cover_at = |tile: (i32, i32)| if tile == first { Cover::NONE } else if lain.contains(&tile) { annexed_to(Cover::NONE, tile, first).unwrap() } else { Cover::NONE };
                assert!(lain.iter().all(|&tile| holder(cover_at, tile) == first), "{fixture:?} turned {turn}");
            }
        }
        assert_eq!(holder(|_| Cover::NONE, (2, 2)), (2, 2), "a tile of no building is its own");
        assert_eq!(turn(turning(Cover::NONE, 4)), 4);
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
