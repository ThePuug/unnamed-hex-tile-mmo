//! The interact key, T: it acts on what the keys act on (`aim`) where
//! that is a hearth or a home to claim, a wagon or a storehouse to move
//! materials at, or a settler to hail.
//! A settler hailed has a panel while the player stands there: who it
//! is, what it is at, and a row to each thing it may be asked for
//! ([`offers`]), the orders of the zone it plans and the hiring of the
//! hub's next planner. A row's key chooses it and asks the settler what
//! it would do, which the panel says under the rows; numpad Enter then
//! has it done, handing over first what the bag holds of what it lacks.
//! With no row chosen Enter hands over what the hub's plans lack, or,
//! of a settler with nothing more to raise, opens the map in the
//! player's hand to choose which hub on it the wagon goes to
//! (`chart::Reading::choose`), the map going with it.
//! A wagon and a storehouse share a panel of two sides ([`load`],
//! [`update_wagon`]). The server finds the settler, the wagon or the
//! storehouse and decides what is handed over, what a plan does and
//! where the wagon may go. Unbuilt: a plan is told in words, and nothing
//! marks its tiles on the ground.

use bevy::prelude::*;
use common::building::{Fixture, Needs, Order, Planned, Stowing, Task, Wagon, Wants};
use common_bevy::{
    components::{equipment::Lot, position::Position, Actor},
    message::{Do, Event, Try},
    resources::map::Map,
};
use qrz::Convert;

use crate::systems::{
    equipment_panel::{CELL, CURSOR_ROW, OTHER_ROW, UNWORN},
    focus::{NumpadFocus, Panel},
    gathering::{ENTRY_KEYS, KEYCODE_CLOSE, KEYCODE_TAKE_ALL},
};

/// The key that hails the settler nearest the player, and lets it be.
pub const KEYCODE_HAIL: KeyCode = KeyCode::KeyT;

/// How far over the ground, in world units, a player has gone from where
/// it stood before it has walked off: most of the way to the next tile.
/// Under it the player has only been set right by the server, or settled
/// on the ground.
const STEPPED: f32 = common::grid::HEX_RADIUS;

/// How far over the ground a player now at `now` is from where it stood.
/// Both are where the player sees itself (`movement::Predicted`), never
/// its confirmed `Position`: that trails by every input still open, and
/// jumps as the server closes one with the player standing still. The
/// two are compared as places, so one told from another tile is the same.
fn gone(map: &Map, stood: &Position, now: &Position) -> f32 {
    let between: Vec3 = map.convert(now.tile - stood.tile);
    (between + now.offset - stood.offset).xz().length()
}

/// Whether a player that stood at `stood` has walked off, now at `now`.
fn walked_off(map: &Map, stood: &Position, now: &Position) -> bool {
    gone(map, stood, now) > STEPPED
}

/// The settler the player has hailed: what it said, and where the player
/// stood to hear it. None while the player has hailed none.
#[derive(Resource, Default)]
pub struct Hailed(pub Option<(Wants, Position)>);

/// What the hailed settler said of its camp's wagon; None where it has
/// none to speak of.
#[derive(Resource, Default)]
pub struct Carted(pub Option<Wagon>);

/// The tiles the stopped wagons that matter to the local player stand
/// on, as the server last said: the compass pins each.
#[derive(Resource, Default)]
pub struct Stopped(pub Vec<(i32, i32)>);

/// What a player looks at to move materials between it and the bag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kept {
    Wagon,
    Store,
}

impl Kept {
    /// What putting a stack of the bag's on it is called.
    pub fn takes(self) -> &'static str {
        match self {
            Kept::Wagon => "load on the wagon",
            Kept::Store => "leave in the storehouse",
        }
    }

    /// Why a piece stays in the bag.
    pub fn refuses(self) -> &'static str {
        match self {
            Kept::Wagon => "A wagon carries materials",
            Kept::Store => "A storehouse keeps materials",
        }
    }
}

/// A wagon or a storehouse looked at: which, what the wagon holds or the
/// player has in the storehouse, and where the player stood to look. At
/// a storehouse besides: what the player has on its hub's wagon, none
/// where that is not at home; whether the side beside the store is that
/// wagon, else the bag; whether a stack is picked from the store, else
/// from that side; and the stack picked, while how many of it is chosen.
#[derive(Clone, Debug, PartialEq)]
pub struct Looked {
    what: Kept,
    held: Vec<common::Stack>,
    stood: Position,
    cart: Option<Vec<common::Stack>>,
    wagon: bool,
    stored: bool,
    choosing: Option<crate::systems::drop_panel::Choosing>,
    /// Which page of the side picked from is shown, where it has more
    /// stacks than the numpad has keys for.
    page: usize,
}

impl Looked {
    /// The stacks of the side beside the store: the wagon's, or the
    /// materials of `bag`.
    fn beside<'a>(&'a self, bag: &'a [common::Stack]) -> &'a [common::Stack] {
        match &self.cart {
            Some(cart) if self.wagon => cart,
            _ => bag,
        }
    }

    /// The stacks a pick is made from.
    fn picked<'a>(&'a self, bag: &'a [common::Stack]) -> &'a [common::Stack] {
        if self.stored { &self.held } else { self.beside(bag) }
    }

    /// The stacks of `side` on the page shown: as many as the numpad has
    /// keys for, from the page's first. A side with fewer pages shows its
    /// last.
    fn paged<'a>(&self, side: &'a [common::Stack]) -> &'a [common::Stack] {
        let page = self.page.min(side.len().saturating_sub(1) / ENTRY_KEYS.len());
        let from = page * ENTRY_KEYS.len();
        &side[from..side.len().min(from + ENTRY_KEYS.len())]
    }

    /// Which way a stack picked moves.
    fn stowing(&self) -> Stowing {
        Stowing::of(self.stored, self.wagon && self.cart.is_some())
    }
}

/// The wagon or storehouse the player looks at; None while it looks at
/// neither.
#[derive(Resource, Default)]
pub struct Loading(pub Option<Looked>);

impl Loading {
    pub fn what(&self) -> Option<Kept> {
        self.0.as_ref().map(|looked| looked.what)
    }

    /// Takes what the server said `what` near the player holds, the
    /// player standing at `stood`, and at a storehouse what it has on the
    /// wagon there: looked at from then, or no longer where it says none
    /// is in reach. Whether it is looked at anew.
    fn hear(&mut self, what: Kept, held: Option<Vec<common::Stack>>, stood: Position, cart: Option<Vec<common::Stack>>) -> bool {
        let same = self.what() == Some(what);
        match held {
            Some(held) => {
                // Where it stood and the sides chosen as it first looked: moving materials changes neither
                let (stood, wagon, stored, choosing, page) = self.0.as_ref().filter(|_| same).map_or((stood, false, false, None, 0), |looked| (looked.stood, looked.wagon, looked.stored, looked.choosing, looked.page));
                self.0 = Some(Looked { what, held, stood, wagon: wagon && cart.is_some(), cart, stored, choosing, page });
                !same
            }
            None => {
                if same {
                    self.0 = None;
                }
                false
            }
        }
    }
}

/// The numbers the zones a player centres are kept by, with their names.
const LOGISTICS: u32 = 1;
const RESIDENTIAL: u32 = 2;
const MARKET: u32 = 3;
const CRAFTING: u32 = 4;
const FARMING: u32 = 5;
const PLAZA: u32 = 6;


/// A row of a planner's panel: the order it asks for, what it is called,
/// where its icon is, where it has one, and what it takes where that is
/// known before it is asked.
#[derive(Clone, Debug, PartialEq)]
pub struct Offer {
    pub order: Order,
    pub name: String,
    pub icon: Option<String>,
    pub takes: Needs,
}

/// Whether `order` acts on what the player faces or where it stands, so
/// that the player carries it there: an upgrade, a downgrade, a zone's
/// centre.
pub fn aims(order: Order) -> bool {
    matches!(order, Order::Upgrade(..) | Order::Downgrade(..) | Order::Zone(..))
}

/// The home a `fixture` is upgraded to, and the one it is downgraded to.
pub(crate) fn larger(fixture: Fixture) -> Option<Fixture> {
    match fixture {
        Fixture::Tent => Some(Fixture::Pavilion),
        Fixture::Pavilion => Some(Fixture::House),
        Fixture::House => Some(Fixture::Tenement),
        _ => None,
    }
}

pub(crate) fn smaller(fixture: Fixture) -> Option<Fixture> {
    [Fixture::Tent, Fixture::Pavilion, Fixture::House].into_iter().find(|&lesser| larger(lesser) == Some(fixture))
}

/// The rows the settler hailed offers, in the order of their keys: the
/// orders of the zone it plans, then the hiring of the hub's next
/// planner where it may have one. A zone is centred on the tile
/// `(q, r)`, where the player stands; a home is upgraded or downgraded
/// on the tile `faced`, where `home` stands, and is offered only where
/// it has one larger or smaller. One at work offers only the hiring.
pub fn offers(wants: Wants, (q, r): (i32, i32), faced: (i32, i32), home: Option<Fixture>) -> Vec<Offer> {
    let row = |order, name: &str| Offer { order, name: name.to_owned(), icon: None, takes: Needs::default() };
    let raise = |fixture: Fixture| Offer { order: Order::Raise(fixture), name: fixture.name().to_owned(), icon: Some(format!("icons/{}.png", fixture.stem())), takes: fixture.needs() };
    // What a home becomes: named, pictured and costed where one is faced that becomes anything
    let becomes = |order, to: Option<Fixture>, name: &str| match to {
        Some(to) => Offer { order, name: format!("{name} to {}", to.name().to_lowercase()), ..raise(to) },
        None => row(order, &format!("{name} a home")),
    };
    let mut rows = Vec::new();
    if !wants.busy {
        match wants.zone as u32 {
            LOGISTICS => rows.extend([
                // How many the hub is planned for: by how many more or fewer is set where it is asked
                row(Order::Residents(0), "Occupancy"),
                row(Order::Shrink, "Shrink to defended wall"),
                Offer { icon: Some("icons/stakes.png".to_owned()), ..row(Order::Wall, "Wall") },
                raise(Fixture::Wagon),
                raise(Fixture::Storehouse),
                row(Order::Zone(LOGISTICS, q, r), "Move zone here"),
            ]),
            RESIDENTIAL => rows.extend([
                raise(Fixture::Tent),
                becomes(Order::Upgrade(faced.0, faced.1), home.and_then(larger), "Upgrade"),
                becomes(Order::Downgrade(faced.0, faced.1), home.and_then(smaller), "Downgrade"),
                row(Order::Zone(RESIDENTIAL, q, r), "Move zone here"),
            ]),
            zone @ (MARKET | CRAFTING | FARMING | PLAZA) => rows.push(row(Order::Zone(zone, q, r), "Move zone here")),
            _ => {}
        }
    }
    if wants.hires != 0 {
        rows.push(row(Order::Hire(0), hires(wants.hires as u32)));
    }
    rows
}

/// The plan the hailed settler was asked after: what it said the order
/// would do, or that its hub does not take it.
#[derive(Clone, Debug, PartialEq)]
pub enum Plan {
    Said(Planned),
    Refused(Order),
}

/// The order the player last asked the hailed settler after, and what it
/// answered; none where nothing was asked.
#[derive(Resource, Default)]
pub struct Asked {
    pub order: Option<Order>,
    pub plan: Option<Plan>,
    /// The rows the settler hailed offers, as the player stands.
    pub rows: Vec<Offer>,
}

/// What hiring the planner of the zone kept by `id` is called.
fn hires(id: u32) -> &'static str {
    match id {
        LOGISTICS => "Hire the logistics planner",
        RESIDENTIAL => "Hire the residential planner",
        MARKET => "Hire the market planner",
        CRAFTING => "Hire the crafting planner",
        FARMING => "Hire the farming planner",
        PLAZA => "Hire the plaza planner",
        _ => "Hire a planner",
    }
}

/// The name of the zone kept by `id`.
fn zone(id: u32) -> &'static str {
    match id {
        LOGISTICS => "logistics",
        RESIDENTIAL => "residential",
        MARKET => "market",
        CRAFTING => "crafting",
        FARMING => "farming",
        PLAZA => "plaza",
        _ => "another",
    }
}

/// `needs` as a settler names it, or none where it lacks nothing.
fn lacking(needs: Needs) -> Option<String> {
    let Needs { hide, wood, stone } = needs;
    let lacking: Vec<String> = [(hide, "hide"), (wood, "wood"), (stone, "stone")].into_iter().filter(|&(count, _)| count > 0).map(|(count, name)| format!("{count} {name}")).collect();
    (!lacking.is_empty()).then(|| lacking.join(", "))
}

/// What a settler says of `plan`, a line to each thing said.
pub fn plans(plan: &Plan) -> Vec<String> {
    let plan = match plan {
        Plan::Refused(Order::Shrink) => return vec!["Nothing to shrink: the whole wall is defended".to_owned()],
        Plan::Refused(Order::Residents(more)) if *more < 0 => return vec!["Can't plan for fewer: that many live here, or the hub is at its smallest".to_owned()],
        Plan::Refused(Order::Wall) => return vec!["The wall is already ordered".to_owned()],
        Plan::Refused(Order::Residents(_)) => return vec!["Can't plan for more residents here".to_owned()],
        Plan::Refused(Order::Raise(fixture)) => return vec![format!("No room for a {} here", fixture.name().to_lowercase())],
        Plan::Refused(Order::Zone(id, ..)) => return vec![format!("The {} zone can't be centred here", zone(*id))],
        Plan::Refused(Order::Upgrade(..)) => return vec!["Can't upgrade: face a home with free tiles beside it".to_owned()],
        Plan::Refused(Order::Downgrade(..)) => return vec!["Can't downgrade: face a home larger than a tent".to_owned()],
        Plan::Refused(_) => return vec!["Not available".to_owned()],
        Plan::Said(plan) => plan,
    };
    let count = |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    let mut said = vec![match plan.order {
        Order::Shrink => "Shrinks the hub to its defended wall".to_owned(),
        Order::Residents(more) if more < 0 => format!("Plans for {} fewer residents", -more),
        Order::Residents(more) => format!("Plans for {more} more residents"),
        Order::Wall => "Builds a wall around the hub".to_owned(),
        Order::Raise(fixture) => format!("Builds a {}", fixture.name().to_lowercase()),
        Order::Zone(id, ..) => format!("Moves the {} zone here", zone(id)),
        Order::Upgrade(..) => "Upgrades this home".to_owned(),
        Order::Downgrade(..) => "Downgrades this home".to_owned(),
        Order::Hire(_) => "Hires a planner".to_owned(),
    }];
    if !plan.gone.is_empty() {
        said.push(format!("Removes {}", count(plan.gone.len(), "building tile", "building tiles")));
    }
    if plan.hubs > 0 {
        said.push(if plan.order == Order::Shrink { format!("Splits the hub into {}", plan.hubs + 1) } else { format!("Absorbs {}", count(plan.hubs as usize, "hub", "hubs")) });
    }
    if !plan.raised.is_empty() || !plan.pulled.is_empty() {
        said.push(format!("Wall: +{}, -{}", count(plan.raised.len(), "stake", "stakes"), plan.pulled.len()));
    }
    if !plan.worn.is_empty() || !plan.faded.is_empty() {
        said.push(format!("Path: +{}, -{}", count(plan.worn.len(), "tile", "tiles"), plan.faded.len()));
    }
    said.push(lacking(plan.needs).map_or("Materials ready".to_owned(), |lacking| format!("Needs {lacking}")));
    said
}

/// What a wagon with `load` on it holds, in a line.
pub fn holds(load: &[common::Stack]) -> String {
    let load: Vec<String> = load.iter().map(|stack| format!("{} {}", stack.count, stack.kind.name().to_lowercase())).collect();
    if load.is_empty() { "Wagon: empty".to_owned() } else { format!("Wagon: {}", load.join(", ")) }
}

/// Whether `wagon` can be sent now: the map in hand holds a hub to send
/// it to, and it is loaded, or stopped on the road with or without a load.
pub fn sends(wagon: &Wagon) -> bool {
    (wagon.away || !wagon.load.is_empty()) && !wagon.hubs.is_empty()
}

/// What a settler says of its `wagon`, a line to each thing said.
pub fn carts(wagon: &Wagon) -> Vec<String> {
    let mut said = vec![holds(&wagon.load)];
    if wagon.away {
        said.push("The way is shut".to_owned());
        said.push(if wagon.hubs.is_empty() { "Hold a map showing a way from here to send it on" } else { "Your map shows a way on" }.to_owned());
    } else if wagon.hubs.is_empty() {
        said.push("Hold a map showing another hub to send it".to_owned());
    } else if wagon.load.is_empty() {
        said.push("Load the wagon to send it".to_owned());
    } else {
        said.push(format!("Your map shows {} it can go to", if wagon.hubs.len() == 1 { "1 hub".to_owned() } else { format!("{} hubs", wagon.hubs.len()) }));
    }
    said
}

/// What a settler says of `wants`, a line to each thing said.
pub fn says(wants: Wants) -> Vec<String> {
    let Some(task) = wants.task else { return vec!["Nothing to do".to_owned()] };
    if wants.busy {
        let at = match task {
            Task::Raise(fixture) => format!("building the {}", fixture.name().to_lowercase()),
            Task::Hearth => "laying the hearth".to_owned(),
            Task::Clear => "clearing old buildings".to_owned(),
        };
        return vec![format!("Busy: {at}")];
    }
    let next = match task {
        Task::Raise(fixture) => format!("Next: {}", fixture.name().to_lowercase()),
        Task::Hearth => "Next: hearth".to_owned(),
        Task::Clear => return vec!["Next: clearing old buildings".to_owned()],
    };
    vec![next, lacking(wants.needs).map_or("Materials ready".to_owned(), |lacking| format!("Needs {lacking}"))]
}

/// Hails on T, lets the settler be on T again, on the close key or when
/// the player moves, and hands over on numpad Enter.
pub fn request(
    mut keys: crate::systems::help::Keys,
    menu: Res<crate::plugins::shell::menu::GameMenu>,
    focus: Res<NumpadFocus>,
    mut hailed: ResMut<Hailed>,
    mut carted: ResMut<Carted>,
    mut asked: ResMut<Asked>,
    mut reading: ResMut<crate::systems::chart::Reading>,
    player: Query<(Entity, &common_bevy::components::heading::Heading), (With<Actor>, With<Position>)>,
    (predicted, map, mut settling): (crate::systems::movement::Predicted, Res<Map>, ResMut<crate::systems::input::Settling>),
    (home, mut claims, time): (Res<crate::systems::building::HomeHearth>, ResMut<crate::systems::building::Claims>, Res<Time<Real>>),
    aiming: crate::systems::aim::Aiming,
    mut writer: MessageWriter<Try>,
) {
    use crate::systems::aim::Acts;
    if hailed.0.is_none() && carted.0.is_some() {
        carted.0 = None;
    }
    if hailed.0.is_none() && (asked.order.is_some() || asked.plan.is_some()) {
        *asked = Asked::default();
    }
    let (Ok((ent, heading)), Some(position)) = (player.single(), predicted.position()) else {
        hailed.0 = None;
        return;
    };
    if menu.open {
        return;
    }
    let hail = keys.pressed(KEYCODE_HAIL, "Hail the settler outlined, or let it be; or claim the hearth or home outlined");
    let Some((wants, stood)) = hailed.0 else {
        // The key is the aim's (`aim`): a hearth or a home is claimed, one
        // in place of another asked for twice; a wagon or a storehouse is
        // `load`'s; a settler is hailed; and anything else is G's
        let aim = hail.then(|| aiming.aim(claims.lives)).flatten().map(|(_, aim)| aim);
        if let Some(aim) = aim.filter(|aim| aim.acts == Acts::Claim) {
            let (tile, stands, slot) = ((aim.tile.q, aim.tile.r), (aim.stands.q, aim.stands.r), aim.slot);
            let changes = crate::systems::building::changes(map.cover_at(stands.0, stands.1), slot, stands, home.0, claims.lives);
            if claims.asked(tile, changes, time.elapsed()) {
                writer.write(Try { event: Event::Gather { ent, q: tile.0, r: tile.1, slot: slot as u8 } });
            }
            return;
        }
        // One that waits is confirmed on the key every panel confirms by
        if claims.waits(time.elapsed()) && focus.is_empty() && keys.take(KEYCODE_TAKE_ALL, "Take the hearth or tent you asked for in place of your own") {
            if let Some(tile) = claims.confirm(time.elapsed()) {
                writer.write(Try { event: Event::Gather { ent, q: tile.0, r: tile.1, slot: common::building::SLOT as u8 } });
            }
            return;
        }
        if aim.is_some_and(|aim| matches!(aim.acts, Acts::Hail(_))) {
            // A walk key let go a moment late would walk the player off what it hails
            settling.begin();
            writer.write(Try { event: Event::Hail { ent, give: false } });
        }
        return;
    };
    // An order that acts on what the player faces is carried there: the settler waits on it
    let carried = asked.order.is_some_and(aims);
    if hail || (walked_off(&map, &stood, &position) && !carried) {
        info!("hail: the settler is let be, {}", if hail { "asked to".to_owned() } else { format!("the player {:.2} from where it stood", gone(&map, &stood, &position)) });
        hailed.0 = None;
        return;
    }
    if !focus.has(Panel::Settler) {
        return;
    }
    // A planner's orders are given from the plan of its hub (`planning`),
    // where the whole of the hub is seen: hailed, it has none
    let _ = (wants, heading);
    let rows: Vec<Offer> = Vec::new();
    if asked.rows != rows {
        asked.rows = rows;
    }
    // The row chosen follows what the player faces as it walks and turns, and is asked after again
    let follows = asked.order.filter(|order| aims(*order)).and_then(|order| asked.rows.iter().map(|row| row.order).find(|now| std::mem::discriminant(now) == std::mem::discriminant(&order)).filter(|now| *now != order));
    if let Some(order) = follows {
        (asked.order, asked.plan) = (Some(order), None);
        writer.write(Try { event: Event::Plan { ent, order, carry: false } });
    }
    let planned = match &asked.plan {
        Some(Plan::Said(plan)) => Some(plan.order),
        _ => None,
    };
    if keys.take(KEYCODE_CLOSE, "Let the row chosen go, or the settler be") {
        match asked.order.take() {
            Some(_) => asked.plan = None,
            None => hailed.0 = None,
        }
    } else if keys.take(KEYCODE_TAKE_ALL, "Do the row chosen, handing the settler what it lacks of what you carry; or choose on your map where its wagon goes") {
        match (asked.order, planned, carted.0.as_ref().filter(|wagon| sends(wagon))) {
            // A hire is asked for and done at once: there is nothing of it to show first
            (Some(order @ Order::Hire(_)), ..) => {
                writer.write(Try { event: Event::Plan { ent, order, carry: true } });
                (asked.order, asked.plan) = (None, None);
            }
            (_, Some(order), _) => drop(writer.write(Try { event: Event::Plan { ent, order, carry: true } })),
            (None, None, Some(wagon)) => reading.choose(wagon.hubs.clone()),
            (None, None, None) => drop(writer.write(Try { event: Event::Hail { ent, give: true } })),
            _ => {}
        };
    }
    // A row's key chooses it, and asks the settler what it would do
    for (row, key) in asked.rows.clone().into_iter().zip(ENTRY_KEYS) {
        if keys.take(key, "Choose this row") {
            (asked.order, asked.plan) = (Some(row.order), None);
            if !matches!(row.order, Order::Hire(_)) {
                writer.write(Try { event: Event::Plan { ent, order: row.order, carry: false } });
            }
        }
    }
}

/// Looks at the wagon or the storehouse aimed at (`aim`) on T, where no
/// settler is hailed, and leaves it on T again, when the player moves or
/// when a settler answers. Either has the one panel ([`update_wagon`]):
/// what the player has there beside its bag, or at a storehouse beside
/// its hub's wagon, one key swapping the two; another swaps the side a
/// stack is picked from, the decimal key shows that side's next page
/// where it has more stacks than cells, a cell's key picks its stack
/// and asks how many of it to move to the other side, as dropping one
/// does (`drop_panel::Choosing`), and the take-all key moves every stack
/// shown.
#[allow(clippy::too_many_arguments)]
pub fn load(
    mut keys: crate::systems::help::Keys,
    menu: Res<crate::plugins::shell::menu::GameMenu>,
    focus: Res<NumpadFocus>,
    hailed: Res<Hailed>,
    mut loading: ResMut<Loading>,
    player: Query<(Entity, &common_bevy::components::equipment::Inventory), (With<Actor>, With<Position>)>,
    (predicted, map, mut settling): (crate::systems::movement::Predicted, Res<Map>, ResMut<crate::systems::input::Settling>),
    (aiming, claims): (crate::systems::aim::Aiming, Res<crate::systems::building::Claims>),
    time: Res<Time<Real>>,
    mut writer: MessageWriter<Try>,
) {
    let (Ok((ent, held)), Some(position)) = (player.single(), predicted.position()) else {
        if loading.0.is_some() {
            loading.0 = None;
        }
        return;
    };
    if menu.open {
        return;
    }
    let look = keys.pressed(KEYCODE_HAIL, "Look at the wagon or the storehouse outlined, to move materials, or leave it");
    let Some(looked) = &loading.0 else {
        let aimed = (look && hailed.0.is_none()).then(|| aiming.aim(claims.lives)).flatten().map(|(_, aim)| aim.acts);
        if let Some(crate::systems::aim::Acts::Look(kept)) = aimed {
            settling.begin();
            writer.write(Try {
                event: match kept {
                    Kept::Wagon => Event::Load { ent, give: None, take: None },
                    Kept::Store => Event::Store { ent, moved: None },
                },
            });
        }
        return;
    };
    let mut close = false;
    let (mut sided, mut turned, mut paged) = (false, false, false);
    // The count being chosen as this frame's keys leave it, where they change it
    let mut chosen = None;
    if focus.has(Panel::Store) && looked.choosing.is_some() {
        use crate::systems::drop_panel::Chose;
        let mut choosing = looked.choosing.expect("a stack is picked");
        let most = looked.picked(&held.stock).iter().find(|stack| stack.kind == choosing.kind).map_or(0, |stack| stack.count);
        let stowing = looked.stowing();
        let left = if most == 0 { Chose::Cancelled } else { choosing.work(&mut keys, time.elapsed(), most, true, "Move the count chosen to the other side") };
        match left {
            Chose::Still => chosen = Some(Some(choosing)).filter(|_| looked.choosing != Some(choosing)),
            Chose::Cancelled => chosen = Some(None),
            Chose::Taken(count) => {
                let stack = common::Stack { kind: choosing.kind, count };
                if count > 0 {
                    writer.write(Try {
                        event: match looked.what {
                            Kept::Store => Event::Store { ent, moved: Some((stowing, stack)) },
                            Kept::Wagon if looked.stored => Event::Load { ent, give: None, take: Some(stack) },
                            Kept::Wagon => Event::Load { ent, give: Some(stack), take: None },
                        },
                    });
                }
                chosen = Some(None);
            }
        }
    } else if focus.has(Panel::Store) {
        let stowing = looked.stowing();
        let moved = |stack: &common::Stack| Try {
            event: match looked.what {
                Kept::Store => Event::Store { ent, moved: Some((stowing, *stack)) },
                Kept::Wagon if looked.stored => Event::Load { ent, give: None, take: Some(*stack) },
                Kept::Wagon => Event::Load { ent, give: Some(*stack), take: None },
            },
        };
        let shown = looked.paged(looked.picked(&held.stock)).iter();
        for (stack, key) in shown.clone().zip(ENTRY_KEYS) {
            if keys.take(key, "Pick this stack, to choose how many of it go to the other side") {
                chosen = Some(Some(crate::systems::drop_panel::Choosing::of(*stack)));
            }
        }
        if keys.take(KEYCODE_TAKE_ALL, "Move every stack shown to the other side") {
            writer.write_batch(shown.map(moved));
        }
        turned = keys.take(KEYCODE_SIDE, "Pick from the other side");
        paged = keys.take(KEYCODE_PAGE, "Show the next page of the side picked from");
        if looked.cart.is_some() {
            sided = keys.take(KEYCODE_BESIDE, "Set your bag or the wagon beside the storehouse");
        }
        close = keys.take(KEYCODE_CLOSE, "Leave the wagon or the storehouse");
    }
    if look || close || walked_off(&map, &looked.stood, &position) || hailed.0.is_some() {
        let why = match (look || close, hailed.0.is_some()) {
            (true, _) => "asked to".to_owned(),
            (_, true) => "a settler answers".to_owned(),
            _ => format!("the player {:.2} from where it stood", gone(&map, &looked.stood, &position)),
        };
        info!("{:?}: left, {why}", looked.what);
        loading.0 = None;
    } else if let Some(looked) = loading.0.as_mut().filter(|_| sided || turned || paged || chosen.is_some()) {
        looked.wagon ^= sided;
        looked.stored ^= turned;
        // A side has a page for every numpad's worth of stacks, and at least the one
        let pages = looked.picked(&held.stock).len().saturating_sub(1) / ENTRY_KEYS.len() + 1;
        looked.page = if sided || turned { 0 } else if paged { (looked.page + 1) % pages } else { looked.page };
        if let Some(chosen) = chosen {
            looked.choosing = chosen;
        }
    }
}

/// How many cells a pane of the storehouse's panel has to a row, how wide
/// the panel is for two of them, and the colour of the pane picked from.
const PANE_WIDTH: usize = 3;
const PANEL: f32 = 2.0 * (PANE_WIDTH as f32 * (CELL + 4.0) + 20.0) + 90.0;
const LIT: Color = Color::srgb(0.95, 0.8, 0.3);

/// The key that chooses the side a stack is picked from at a storehouse.
const KEYCODE_SIDE: KeyCode = KeyCode::NumpadMultiply;
/// The key that swaps the bag and the wagon beside a storehouse.
const KEYCODE_BESIDE: KeyCode = KeyCode::NumpadDivide;
/// The key that shows the next page of the side picked from, as it shows
/// the bag's next row.
const KEYCODE_PAGE: KeyCode = KeyCode::NumpadDecimal;

/// Keeps what the wagon or storehouse looked at holds as the server says
/// it. A pile opened while one is looked at has the numpad, and it is
/// left.
pub fn do_loaded(mut reader: MessageReader<Do>, mut loading: ResMut<Loading>, player: Query<&Position, With<Actor>>, predicted: crate::systems::movement::Predicted) {
    for message in reader.read() {
        let (ent, what, held, cart) = match message {
            Do { event: Event::Loaded { ent, load } } => (ent, Kept::Wagon, load, None),
            Do { event: Event::Stored { ent, held, cart } } => (ent, Kept::Store, held, cart.clone()),
            Do { event: Event::Loot { ent, cells } } if cells.is_some() && player.contains(*ent) && loading.0.is_some() => {
                loading.0 = None;
                continue;
            }
            _ => continue,
        };
        let Ok(position) = player.get(*ent) else { continue };
        let anew = loading.hear(what, held.clone(), predicted.position().unwrap_or(*position), cart);
        if anew {
            info!("{what:?}: one stands in reach");
        }
    }
}

/// The root of the panel that says what is looked at, a wagon or a
/// storehouse, spawned with its first open and shown while open.
#[derive(Component)]
pub struct WagonRows;

/// Shows the panel while a wagon or a storehouse is looked at and lays
/// its lines out again whenever what that holds changes.
pub fn update_wagon(
    mut commands: Commands,
    loading: Res<Loading>,
    mut rows: Query<(Entity, &mut Node), With<WagonRows>>,
    bag: Query<Ref<common_bevy::components::equipment::Inventory>, With<Actor>>,
    asset_server: Res<AssetServer>,
) {
    // The panel shows the bag, and follows it
    let bagged = loading.0.is_some() && bag.single().is_ok_and(|bag| bag.is_changed());
    if !loading.is_changed() && !bagged {
        return;
    }
    let root = match rows.single_mut() {
        Ok((root, mut node)) => {
            node.display = if loading.0.is_some() { Display::Flex } else { Display::None };
            commands.entity(root).despawn_related::<Children>();
            root
        }
        Err(_) if loading.0.is_none() => return,
        Err(_) => commands
            .spawn((
                WagonRows,
                // The whole window, to stand the panel in the middle of it
                Node { position_type: PositionType::Absolute, width: Val::Percent(100.), height: Val::Percent(100.), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
            ))
            .id(),
    };
    let Some(looked) = &loading.0 else { return };
    let mut frame = None;
    commands.entity(root).with_children(|screen| {
        // As wide as its two panes whatever it says, so it never changes width
        let panel = Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(8.), width: Val::Px(PANEL), padding: UiRect::all(Val::Px(14.)), border_radius: BorderRadius::all(Val::Px(6.)), ..default() };
        frame = Some(screen.spawn((panel, BackgroundColor(Color::srgba(0.05, 0.05, 0.05, 0.95)))).id());
    });
    let Some(root) = frame else { return };
    let line = |text: String| (Text::new(text), TextFont { font_size: FontSize::Px(14.0), ..default() }, TextColor(Color::srgb(0.85, 0.85, 0.85)));
    commands.entity(root).with_children(|list| {
        use crate::systems::keycap::{hint_row, Hint};
        let bagged: Vec<common::Stack> = bag.single().map_or(Vec::new(), |bag| bag.stock.clone());
        let wagon = looked.wagon && looked.cart.is_some();
        // The two sides and which is picked from. Each is named the same whatever is picked
        let sides = match looked.what {
            Kept::Wagon => [("Wagon", looked.held.as_slice(), looked.stored), ("Bag", bagged.as_slice(), !looked.stored)],
            Kept::Store => [("Storehouse", looked.held.as_slice(), looked.stored), (if wagon { "Wagon" } else { "Bag" }, looked.beside(&bagged), !looked.stored)],
        };
        let says = match looked.what {
            Kept::Wagon if looked.stored => "Unload: wagon to bag",
            Kept::Wagon => "Load: bag to wagon",
            Kept::Store => looked.stowing().says(),
        };
        list.spawn(line(says.to_owned()));
        // Two panes of cells, as the bag draws its stacks, the one picked from lit; between them, which way a pick goes
        list.spawn(Node { column_gap: Val::Px(10.), justify_content: JustifyContent::Center, ..default() }).with_children(|panes| {
            for (at, (title, stacks, picked)) in sides.into_iter().enumerate() {
                if at == 1 {
                    let arrow = Node { align_self: AlignSelf::Center, width: Val::Px(30.), justify_content: JustifyContent::Center, ..default() };
                    panes.spawn(arrow).with_children(|arrow| {
                        arrow.spawn((Text::new(if looked.stored { ">>" } else { "<<" }), TextFont { font_size: FontSize::Px(18.0), ..default() }, TextColor(LIT)));
                    });
                }
                let shade = if picked { Color::srgb(0.95, 0.95, 0.95) } else { Color::srgb(0.5, 0.5, 0.5) };
                let pane = Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(6.), padding: UiRect::all(Val::Px(8.)), border: UiRect::all(Val::Px(2.)), border_radius: BorderRadius::all(Val::Px(4.)), ..default() };
                panes.spawn((pane, BorderColor::all(if picked { LIT } else { UNWORN }), BackgroundColor(Color::srgba(0.12, 0.12, 0.12, if picked { 0.95 } else { 0.5 })))).with_children(|pane| {
                    pane.spawn((Text::new(title), TextFont { font_size: FontSize::Px(15.0), ..default() }, TextColor(shade)));
                    // As many cells as the numpad has keys for, a stack to a cell; the side picked from names each stack's key
                    let stacks = if picked { looked.paged(stacks) } else { &stacks[..stacks.len().min(ENTRY_KEYS.len())] };
                    for row in 0..ENTRY_KEYS.len() / PANE_WIDTH {
                        pane.spawn(Node { column_gap: Val::Px(4.), ..default() }).with_children(|cells| {
                            for at in row * PANE_WIDTH..(row + 1) * PANE_WIDTH {
                                let stack = stacks.get(at).copied();
                                let cell = Node { width: Val::Px(CELL), height: Val::Px(CELL), justify_content: JustifyContent::Center, align_items: AlignItems::Center, border: UiRect::all(Val::Px(2.)), border_radius: BorderRadius::all(Val::Px(4.)), ..default() };
                                cells.spawn((cell, BackgroundColor(if stack.is_some() && picked { CURSOR_ROW } else { OTHER_ROW }), BorderColor::all(UNWORN))).with_children(|cell| {
                                    let Some(stack) = stack else { return };
                                    crate::systems::bag_panel::cell_contents(cell, &asset_server, Lot::Stack(stack), 1.0);
                                    if picked {
                                        crate::systems::keycap::corner_keycap(cell, crate::caps!(ENTRY_KEYS)[at]);
                                    }
                                });
                            }
                        });
                    }
                });
            }
        });
        // Two lines under the panes whatever is shown, so the panel keeps its height. While a count is chosen the numpad is its box's
        if let Some(choosing) = looked.choosing {
            // The one count box every panel that asks how many draws
            crate::systems::drop_panel::spawn_entry(list, choosing.count, &format!("move this many {}", choosing.kind.name().to_lowercase()));
            return;
        }
        hint_row(list, None, &[Hint::key(crate::cap!(KEYCODE_SIDE), "other side"), Hint::key(crate::cap!(KEYCODE_PAGE), "next page"), Hint::key(crate::cap!(KEYCODE_TAKE_ALL), "move all"), Hint::key(crate::cap!(KEYCODE_CLOSE), "close")]);
        match looked.what {
            Kept::Wagon => drop(list.spawn(line("A settler sends it using a map".to_owned()))),
            Kept::Store if looked.cart.is_none() => drop(list.spawn(line("No wagon here to load".to_owned()))),
            Kept::Store => hint_row(list, None, &[Hint::key(crate::cap!(KEYCODE_BESIDE), "bag or wagon")]),
        }
    });
}

/// Keeps what the hailed settler said as the server says it.
pub fn do_wants(
    mut reader: MessageReader<Do>,
    mut hailed: ResMut<Hailed>,
    mut carted: ResMut<Carted>,
    mut stopped: ResMut<Stopped>,
    mut asked: ResMut<Asked>,
    player: Query<&Position, With<Actor>>,
    predicted: crate::systems::movement::Predicted,
    planning: Res<crate::systems::planning::Planning>,
) {
    for message in reader.read() {
        match message {
            // An order given from the plan of a hub is answered as a hail is, by the hub's settler; no settler was hailed, and none is from it
            Do { event: Event::Wants { .. } } if planning.open() => {}
            Do { event: Event::Wants { ent, wants } } => {
                let Ok(position) = player.get(*ent) else { continue };
                let stood = predicted.position().unwrap_or(*position);
                hailed.0 = wants.map(|wants| (wants, stood));
                match wants {
                    Some(_) => info!("hail: a settler answers"),
                    None => info!("hail: no settler is near enough to hear, or it sleeps, or its wagon is on the road"),
                }
            }
            Do { event: Event::Wagon { ent, wagon } } if player.contains(*ent) => carted.0 = wagon.clone(),
            Do { event: Event::Stopped { ent, wagons } } if player.contains(*ent) => stopped.0 = wagons.clone(),
            // With no plan said, it makes none; but for one just carried out
            Do { event: Event::Planned { ent, plan } } if player.contains(*ent) => {
                let carried = matches!(asked.plan, Some(Plan::Said(_))) && plan.is_none();
                asked.plan = match (plan, asked.order) {
                    (Some(plan), _) => Some(Plan::Said(plan.clone())),
                    (None, Some(order)) if !carried => Some(Plan::Refused(order)),
                    _ => None,
                };
                if carried {
                    asked.order = None;
                }
            }
            _ => {}
        }
    }
}

/// The panel's root, spawned with its first open and shown while open.
#[derive(Component)]
pub struct SettlerRows;

/// Shows the panel while a settler is hailed and lays its lines out again
/// whenever what it said changes.
pub fn update(mut commands: Commands, hailed: Res<Hailed>, carted: Res<Carted>, asked: Res<Asked>, mut rows: Query<(Entity, &mut Node), With<SettlerRows>>, asset_server: Res<AssetServer>) {
    if !hailed.is_changed() && !carted.is_changed() && !asked.is_changed() {
        return;
    }
    let root = match rows.single_mut() {
        Ok((root, mut node)) => {
            node.display = if hailed.0.is_some() { Display::Flex } else { Display::None };
            commands.entity(root).despawn_related::<Children>();
            root
        }
        Err(_) if hailed.0.is_none() => return,
        Err(_) => commands
            .spawn((
                SettlerRows,
                // One width whatever it says, so it never moves
                Node { position_type: PositionType::Absolute, top: Val::Percent(30.), left: Val::Percent(58.), width: Val::Px(PLANNER), flex_direction: FlexDirection::Column, row_gap: Val::Px(5.), padding: UiRect::all(Val::Px(12.)), border_radius: BorderRadius::all(Val::Px(6.)), ..default() },
                BackgroundColor(Color::srgba(0.05, 0.05, 0.05, 0.95)),
            ))
            .id(),
    };
    let Some((wants, _)) = hailed.0 else { return };
    let text = |text: String, size: f32, shade: f32| (Text::new(text), TextFont { font_size: FontSize::Px(size), ..default() }, TextColor(Color::srgb(shade, shade, shade)));
    let icon = |path: String| (ImageNode::new(asset_server.load(path)), Node { width: Val::Px(ROW_ICON), height: Val::Px(ROW_ICON), ..default() });
    let chosen = asked.order.and_then(|order| asked.rows.iter().find(|row| row.order == order));
    commands.entity(root).with_children(|list| {
        use crate::systems::keycap::{hint_row, Hint};
        // Who it is, and in a line each what it is at and how its hub stands
        // A driver on the road speaks of its wagon alone
        if carted.0.as_ref().is_some_and(|wagon| wagon.away) {
            list.spawn(text("Wagon driver".to_owned(), 16.0, 0.95));
        } else {
            list.spawn(text(if wants.zone == 0 { "Settler".to_owned() } else { format!("{} planner", zone(wants.zone as u32)) }, 16.0, 0.95));
            list.spawn(text(says(wants).join(". "), 13.0, 0.6));
            list.spawn(text(format!("Planned for {}{}", wants.planned, if wants.walled { ", walled" } else { "" }), 13.0, 0.6));
        }
        if let Some(wagon) = &carted.0 {
            list.spawn(text(carts(wagon).join(". "), 13.0, 0.6));
        }
        // A row to each thing it may be asked for: its key, its icon and name, and what it takes
        for (row, key) in asked.rows.iter().zip(crate::caps!(ENTRY_KEYS)) {
            let lit = chosen == Some(row);
            list.spawn((
                Node { align_items: AlignItems::Center, column_gap: Val::Px(8.), padding: UiRect::axes(Val::Px(8.), Val::Px(4.)), border: UiRect::all(Val::Px(1.)), border_radius: BorderRadius::all(Val::Px(4.)), ..default() },
                BackgroundColor(if lit { CURSOR_ROW } else { OTHER_ROW }),
                BorderColor::all(if lit { LIT } else { UNWORN }),
            ))
            .with_children(|cell| {
                crate::systems::keycap::keycap(cell, key);
                match &row.icon {
                    Some(path) => cell.spawn(icon(path.clone())),
                    // The same room where it has none, so the names line up
                    None => cell.spawn(Node { width: Val::Px(ROW_ICON), height: Val::Px(ROW_ICON), ..default() }),
                };
                cell.spawn((text(row.name.clone(), 14.0, 0.9), Node { flex_grow: 1.0, ..default() }));
                for (material, count) in [(common::Material::Hide, row.takes.hide), (common::Material::Softwood, row.takes.wood), (common::Material::Sandstone, row.takes.stone)] {
                    if count > 0 {
                        cell.spawn(icon(format!("icons/{}.png", material.stem())));
                        cell.spawn(text(count.to_string(), 14.0, 0.9));
                    }
                }
            });
        }
        // The row chosen: what it does, and what the settler said it would
        match chosen {
            Some(_) => {
                let said = asked.plan.as_ref().map_or(Vec::new(), plans);
                if !said.is_empty() {
                    list.spawn(text(said.join(". "), 13.0, 0.85));
                }
                let lacks = matches!(&asked.plan, Some(Plan::Said(plan)) if plan.order.builds() && !plan.needs.is_none());
                let refused = matches!(&asked.plan, Some(Plan::Refused(_)));
                let mut hints = Vec::new();
                if !refused {
                    hints.push(Hint::key(crate::cap!(KEYCODE_TAKE_ALL), if lacks { "hand over what it lacks" } else { "do it" }));
                }
                hints.push(Hint::key(crate::cap!(KEYCODE_CLOSE), "back"));
                hint_row(list, None, &hints);
            }
            None => {
                let mut hints = Vec::new();
                if !wants.needs.is_none() {
                    hints.push(Hint::key(crate::cap!(KEYCODE_TAKE_ALL), "hand over what it lacks"));
                } else if carted.0.as_ref().is_some_and(sends) {
                    hints.push(Hint::key(crate::cap!(KEYCODE_TAKE_ALL), "send the wagon by your map"));
                }
                hints.push(Hint::key(crate::cap!(crate::systems::planning::KEYCODE_PLAN), "plan"));
                hints.push(Hint::key(crate::cap!(KEYCODE_CLOSE), "close"));
                hint_row(list, None, &hints);
            }
        }
    });
}

/// How wide the planner's panel is, and how large a row's icons.
const PLANNER: f32 = 440.0;
const ROW_ICON: f32 = 28.0;

#[cfg(test)]
mod tests {
    use super::*;
    use common::building::Fixture;

    /// A player has walked off once it is a step from where it stood, and
    /// not while it stands there, though its place is told from another
    /// tile or has settled a hair.
    #[test]
    fn a_player_standing_has_not_walked_off() {
        let map = Map::new(qrz::Map::new(common::grid::HEX_RADIUS, common::grid::RISE));
        let stood = Position::new(qrz::Qrz { q: 40_000, r: -9_000, z: 12 }, Vec3::new(0.4, 0.0, -0.3));
        let mut retold = stood;
        retold.rebase(qrz::Qrz { q: 40_001, r: -9_000, z: 12 }, &map);
        assert_ne!(stood, retold);
        assert!(!walked_off(&map, &stood, &retold) && !walked_off(&map, &stood, &stood));
        let settled = Position::new(stood.tile, stood.offset + Vec3::new(0.2, 0.9, -0.1));
        assert!(!walked_off(&map, &stood, &settled), "set right a little, or settled on the ground");
        let stepped = Position::new(qrz::Qrz { q: 40_001, r: -9_000, z: 12 }, stood.offset);
        assert!(walked_off(&map, &stood, &stepped));
    }

    /// Every row a settler offers has a key, the hiring last; one at work
    /// offers none; and a home is upgraded and downgraded whatever is
    /// faced, to what the home faced becomes.
    #[test]
    fn a_settler_offers_the_orders_of_its_zone() {
        for zone in [LOGISTICS, RESIDENTIAL, MARKET, CRAFTING, FARMING, PLAZA] {
            let rows = offers(Wants { zone: zone as u8, hires: 3, ..Wants::default() }, (0, 0), (1, 0), Some(Fixture::Pavilion));
            assert!(rows.len() <= ENTRY_KEYS.len() && rows.last().is_some_and(|row| row.order == Order::Hire(0)), "every row has a key, the hiring last");
        }
        assert!(offers(Wants { zone: LOGISTICS as u8, busy: true, ..Wants::default() }, (0, 0), (1, 0), None).is_empty(), "one at work takes no order");
        let homes = |home| offers(Wants { zone: RESIDENTIAL as u8, ..Wants::default() }, (0, 0), (1, 0), home);
        for home in [None, Some(Fixture::Tent), Some(Fixture::Tenement)] {
            let orders: Vec<Order> = homes(home).into_iter().map(|row| row.order).collect();
            assert!(orders.contains(&Order::Upgrade(1, 0)) && orders.contains(&Order::Downgrade(1, 0)));
        }
        let upgrade = |home| homes(home).into_iter().find(|row| row.order == Order::Upgrade(1, 0)).unwrap();
        assert_eq!((upgrade(Some(Fixture::Tent)).takes, upgrade(None).takes), (Fixture::Pavilion.needs(), Needs::default()));
        assert!([Order::Upgrade(0, 0), Order::Downgrade(0, 0), Order::Zone(2, 0, 0)].into_iter().all(aims) && !aims(Order::Wall));
    }

    /// A wagon is sent once it is loaded and the map in hand holds a hub;
    /// one stopped on the road by a map alone, empty or not.
    #[test]
    fn a_wagon_is_sent_loaded_and_by_a_map() {
        let softwood = common::Stack { kind: common::Stackable::Material(common::Material::Softwood), count: 4 };
        let loaded = Wagon { load: vec![softwood], hubs: vec![(9, -2)], away: false };
        assert!(sends(&loaded) && !sends(&Wagon::default()) && !sends(&Wagon { hubs: vec![(9, -2)], ..Wagon::default() }));
        assert!(sends(&Wagon { hubs: vec![(9, -2)], away: true, ..Wagon::default() }) && !sends(&Wagon { away: true, ..Wagon::default() }));
    }
}
