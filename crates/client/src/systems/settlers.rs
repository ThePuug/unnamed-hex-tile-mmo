//! Hailing a settler: T asks the settler nearest the player what it
//! does next and what it still lacks, and a panel says so while the
//! player stands there. Numpad Enter hands over what the bag holds of
//! what is lacking. Numpad + asks what growing its hub a tier would do,
//! numpad - what shrinking it would, and numpad Enter then hands over
//! what a grow still lacks, or has the plan done.
//! A settler with nothing more to raise speaks of its wagon besides:
//! numpad Enter then opens the map in the player's hand to choose which
//! hub on it the wagon goes to (`chart::Reading::choose`), the map going
//! with it. The wagon is loaded at the wagon, from the bag: T
//! beside one standing at home, with no settler in reach to hail, shows
//! what it holds and opens the bag, where what a stack's detail would
//! drop goes on the wagon while the player stands there. The
//! server finds the settler or the wagon and decides what is handed
//! over, what a plan does and where the wagon may go. Unbuilt: a
//! plan is told in words, and nothing marks its tiles on the ground.

use bevy::prelude::*;
use common::building::{Needs, Planned, Task, Wagon, Wants};
use common_bevy::{
    components::{position::Position, Actor},
    message::{Do, Event, Try},
    resources::map::Map,
};
use qrz::Convert;

use crate::systems::{
    character_panel::{CharacterPanel, CharacterPanelState, PanelTab},
    focus::{NumpadFocus, Panel},
    gathering::{KEYCODE_CLOSE, KEYCODE_TAKE_ALL},
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

/// A wagon looked at: what it holds, and where the player stood to look.
#[derive(Clone, Debug, PartialEq)]
pub struct Looked {
    load: Vec<common::Stack>,
    stood: Position,
}

/// The wagon the player looks at; None while it looks at none.
#[derive(Resource, Default)]
pub struct Loading(pub Option<Looked>);

/// The keys that ask the hailed settler what growing its hub, and what
/// shrinking it, would do.
pub const KEYCODE_GROW: KeyCode = KeyCode::NumpadAdd;
pub const KEYCODE_SHRINK: KeyCode = KeyCode::NumpadSubtract;

/// The plan the hailed settler was asked after: what it said the plan
/// would do, or that it makes none, a grow or else a shrink.
#[derive(Clone, Debug, PartialEq)]
pub enum Plan {
    Said(Planned),
    Refused { grow: bool },
}

/// What the player last asked the hailed settler to plan, a grow or else
/// a shrink, and what it answered; none where nothing was asked.
#[derive(Resource, Default)]
pub struct Asked {
    pub grow: Option<bool>,
    pub plan: Option<Plan>,
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
        Plan::Refused { grow: true } => return vec!["I cannot grow this hub now".to_owned()],
        Plan::Refused { grow: false } => return vec!["We hold this wall: there is nothing to give up".to_owned()],
        Plan::Said(plan) => plan,
    };
    let count = |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    let mut said = Vec::new();
    if plan.grow {
        said.push(format!("To grow I would build on {}", count(plan.built.len(), "site", "sites")));
        if plan.hubs > 0 {
            said.push(format!("and take in {}", count(plan.hubs as usize, "hub", "hubs")));
        }
    } else {
        said.push(format!("To shrink I would give up {}", count(plan.gone.len(), "site", "sites")));
        if plan.hubs > 0 {
            said.push(format!("and part this hub in {}", plan.hubs + 1));
        }
    }
    said.push(format!("The wall: {} raised, {} pulled", count(plan.raised.len(), "stake", "stakes"), plan.pulled.len()));
    if !plan.worn.is_empty() || !plan.faded.is_empty() {
        said.push(format!("The path moves: {} new, {} given up", count(plan.worn.len(), "tile", "tiles"), plan.faded.len()));
    }
    said.push(lacking(plan.needs).map_or("I have what it takes".to_owned(), |lacking| format!("It still needs {lacking}")));
    said
}

/// What a wagon with `load` on it holds, in a line.
pub fn holds(load: &[common::Stack]) -> String {
    let load: Vec<String> = load.iter().map(|stack| format!("{} {}", stack.count, stack.kind.name().to_lowercase())).collect();
    if load.is_empty() { "The wagon stands empty".to_owned() } else { format!("The wagon holds {}", load.join(", ")) }
}

/// Whether `wagon` can be sent now: it is loaded, and the map in hand
/// holds a hub to send it to.
pub fn sends(wagon: &Wagon) -> bool {
    !wagon.load.is_empty() && !wagon.hubs.is_empty()
}

/// What a settler says of its `wagon`, a line to each thing said.
pub fn carts(wagon: &Wagon) -> Vec<String> {
    let mut said = vec![holds(&wagon.load)];
    if wagon.hubs.is_empty() {
        said.push("Hold a map that reaches another hub and I will take it there".to_owned());
    } else if wagon.load.is_empty() {
        said.push("Load it at the wagon and I will take it where your map shows".to_owned());
    } else {
        said.push(format!("Your map shows {} I can take it to", if wagon.hubs.len() == 1 { "a hub".to_owned() } else { format!("{} hubs", wagon.hubs.len()) }));
    }
    said
}

/// What a settler says of `wants`, a line to each thing said.
pub fn says(wants: Wants) -> Vec<String> {
    let Some(task) = wants.task else { return vec!["This hub wants nothing more of me".to_owned()] };
    let next = match task {
        Task::Raise(fixture) => format!("Next I raise: {}", fixture.name().to_lowercase()),
        Task::Hearth => "Next I lay: the hearth".to_owned(),
        Task::Clear => return vec!["Next I take down what the plan gave up".to_owned()],
    };
    vec![next, lacking(wants.needs).map_or("I have all this hub's plan takes".to_owned(), |lacking| format!("For all the plan I still need {lacking}"))]
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
    player: Query<Entity, (With<Actor>, With<Position>)>,
    (predicted, map, mut settling): (crate::systems::movement::Predicted, Res<Map>, ResMut<crate::systems::input::Settling>),
    mut writer: MessageWriter<Try>,
) {
    if hailed.0.is_none() && carted.0.is_some() {
        carted.0 = None;
    }
    if hailed.0.is_none() && (asked.grow.is_some() || asked.plan.is_some()) {
        *asked = Asked::default();
    }
    let (Ok(ent), Some(position)) = (player.single(), predicted.position()) else {
        hailed.0 = None;
        return;
    };
    if menu.open {
        return;
    }
    let hail = keys.pressed(KEYCODE_HAIL, "Hail the settler beside you, or let it be");
    let Some((_, stood)) = hailed.0 else {
        if hail {
            // A walk key let go a moment late would walk the player off what it hails
            settling.begin();
            writer.write(Try { event: Event::Hail { ent, give: false } });
        }
        return;
    };
    if hail || walked_off(&map, &stood, &position) {
        info!("hail: the settler is let be, {}", if hail { "asked to".to_owned() } else { format!("the player {:.2} from where it stood", gone(&map, &stood, &position)) });
        hailed.0 = None;
        return;
    }
    if !focus.has(Panel::Settler) {
        return;
    }
    let planned = match &asked.plan {
        Some(Plan::Said(plan)) => Some(plan.grow),
        _ => None,
    };
    if keys.take(KEYCODE_CLOSE, "Let the settler be, or its plan") {
        match asked.plan.take() {
            Some(_) => asked.grow = None,
            None => hailed.0 = None,
        }
    } else if keys.take(KEYCODE_TAKE_ALL, "Hand the settler what you carry of what it or its plan needs, have its plan done, or choose on your map where its wagon goes") {
        match (planned, carted.0.as_ref().filter(|wagon| sends(wagon))) {
            (Some(grow), _) => drop(writer.write(Try { event: Event::Plan { ent, grow, carry: true } })),
            (None, Some(wagon)) => reading.choose(wagon.hubs.clone()),
            (None, None) => drop(writer.write(Try { event: Event::Hail { ent, give: true } })),
        };
    }
    for (key, grow, does) in [(KEYCODE_GROW, true, "Ask the settler what growing its hub would do"), (KEYCODE_SHRINK, false, "Ask the settler what shrinking its hub would do")] {
        if keys.take(key, does) {
            asked.grow = Some(grow);
            writer.write(Try { event: Event::Plan { ent, grow, carry: false } });
        }
    }
}

/// Looks at the wagon beside the player on T, where no settler is hailed,
/// and leaves it on T again, when the player moves or when a settler
/// answers, shutting the bag it opened. While a wagon is looked at the
/// bag loads it (`bag_panel`, `drop_panel`).
pub fn load(
    mut keys: crate::systems::help::Keys,
    menu: Res<crate::plugins::shell::menu::GameMenu>,
    hailed: Res<Hailed>,
    mut loading: ResMut<Loading>,
    player: Query<Entity, (With<Actor>, With<Position>)>,
    (predicted, map, mut settling): (crate::systems::movement::Predicted, Res<Map>, ResMut<crate::systems::input::Settling>),
    mut bag: (ResMut<CharacterPanelState>, Query<&mut Visibility, With<CharacterPanel>>),
    mut writer: MessageWriter<Try>,
) {
    let (Ok(ent), Some(position)) = (player.single(), predicted.position()) else {
        if loading.0.is_some() {
            loading.0 = None;
        }
        return;
    };
    if menu.open {
        return;
    }
    let look = keys.pressed(KEYCODE_HAIL, "Look at the wagon beside you to load it from your bag, or leave it");
    let Some(looked) = &loading.0 else {
        if look && hailed.0.is_none() {
            settling.begin();
            writer.write(Try { event: Event::Load { ent, give: None } });
        }
        return;
    };
    if look || walked_off(&map, &looked.stood, &position) || hailed.0.is_some() {
        let why = match (look, hailed.0.is_some()) {
            (true, _) => "asked to".to_owned(),
            (_, true) => "a settler answers".to_owned(),
            _ => format!("the player {:.2} from where it stood", gone(&map, &looked.stood, &position)),
        };
        info!("wagon: left, {why}");
        loading.0 = None;
        if let Ok(mut visibility) = bag.1.single_mut() {
            crate::systems::character_panel::close(&mut bag.0, &mut visibility);
            *visibility = Visibility::Hidden;
        }
    }
}

/// Keeps what the wagon looked at holds as the server says it, and opens
/// the bag on one looked at anew, to load it from.
pub fn do_loaded(
    mut reader: MessageReader<Do>,
    mut loading: ResMut<Loading>,
    player: Query<&Position, With<Actor>>,
    predicted: crate::systems::movement::Predicted,
    mut state: ResMut<CharacterPanelState>,
    mut panel: Query<&mut Visibility, With<CharacterPanel>>,
) {
    for message in reader.read() {
        let Do { event: Event::Loaded { ent, load } } = message else { continue };
        let Ok(position) = player.get(*ent) else { continue };
        // Where it stood as it first looked: loading does not move it
        let stood = loading.0.as_ref().map_or_else(|| predicted.position().unwrap_or(*position), |looked| looked.stood);
        let was = loading.0.is_some();
        if !was {
            info!("wagon: {}", if load.is_some() { "one stands in reach" } else { "none stands in reach" });
        }
        if was || load.is_some() {
            loading.0 = load.clone().map(|load| Looked { load, stood });
        }
        if !was && loading.0.is_some() {
            (state.visible, state.tab) = (true, PanelTab::Bag);
            if let Ok(mut visibility) = panel.single_mut() {
                *visibility = Visibility::Visible;
            }
        }
    }
}

/// The wagon panel's root, spawned with its first open and shown while
/// open.
#[derive(Component)]
pub struct WagonRows;

/// Shows the wagon's panel while one is looked at and lays its lines out
/// again whenever what it holds changes.
pub fn update_wagon(mut commands: Commands, loading: Res<Loading>, mut rows: Query<(Entity, &mut Node), With<WagonRows>>) {
    if !loading.is_changed() {
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
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Percent(12.),
                    left: Val::Percent(60.),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(4.),
                    padding: UiRect::all(Val::Px(10.)),
                    border_radius: BorderRadius::all(Val::Px(6.)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.05, 0.05, 0.05, 0.95)),
            ))
            .id(),
    };
    let Some(looked) = &loading.0 else { return };
    let line = |text: String| (Text::new(text), TextFont { font_size: FontSize::Px(14.0), ..default() }, TextColor(Color::srgb(0.85, 0.85, 0.85)));
    commands.entity(root).with_children(|list| {
        list.spawn(line("Wagon".to_owned()));
        list.spawn(line(holds(&looked.load)));
        list.spawn(line("Load it from your bag while you stand here".to_owned()));
        list.spawn(line("Its settler sends it, by a map".to_owned()));
        crate::systems::keycap::hint_row(list, None, &[crate::systems::keycap::Hint::key(KEYCODE_HAIL, "leave it")]);
    });
}

/// Keeps what the hailed settler said as the server says it.
pub fn do_wants(
    mut reader: MessageReader<Do>,
    mut hailed: ResMut<Hailed>,
    mut carted: ResMut<Carted>,
    mut asked: ResMut<Asked>,
    player: Query<&Position, With<Actor>>,
    predicted: crate::systems::movement::Predicted,
) {
    for message in reader.read() {
        match message {
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
            // With no plan said, it makes none; but for one just carried out
            Do { event: Event::Planned { ent, plan } } if player.contains(*ent) => {
                let carried = matches!(asked.plan, Some(Plan::Said(_))) && plan.is_none();
                asked.plan = match (plan, asked.grow) {
                    (Some(plan), _) => Some(Plan::Said(plan.clone())),
                    (None, Some(grow)) if !carried => Some(Plan::Refused { grow }),
                    _ => None,
                };
                if carried {
                    asked.grow = None;
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
pub fn update(mut commands: Commands, hailed: Res<Hailed>, carted: Res<Carted>, asked: Res<Asked>, mut rows: Query<(Entity, &mut Node), With<SettlerRows>>) {
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
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Percent(45.),
                    left: Val::Percent(60.),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(4.),
                    padding: UiRect::all(Val::Px(10.)),
                    border_radius: BorderRadius::all(Val::Px(6.)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.05, 0.05, 0.05, 0.95)),
            ))
            .id(),
    };
    let Some((wants, _)) = hailed.0 else { return };
    let line = |text: String| (Text::new(text), TextFont { font_size: FontSize::Px(14.0), ..default() }, TextColor(Color::srgb(0.85, 0.85, 0.85)));
    commands.entity(root).with_children(|list| {
        list.spawn(line("Settler".to_owned()));
        for said in says(wants) {
            list.spawn(line(said));
        }
        let mut hints = Vec::new();
        if let Some(plan) = &asked.plan {
            for said in plans(plan) {
                list.spawn(line(said));
            }
            if let Plan::Said(plan) = plan {
                let does = match (plan.grow, plan.needs.is_none()) {
                    (true, false) => "hand over",
                    (true, true) => "grow",
                    (false, _) => "shrink",
                };
                hints.push(crate::systems::keycap::Hint::key(KEYCODE_TAKE_ALL, does));
                hints.push(crate::systems::keycap::Hint::key(KEYCODE_CLOSE, "leave it"));
                crate::systems::keycap::hint_row(list, None, &hints);
                return;
            }
        }
        if !wants.needs.is_none() {
            hints.push(crate::systems::keycap::Hint::key(KEYCODE_TAKE_ALL, "hand over"));
        }
        if let Some(wagon) = &carted.0 {
            for said in carts(wagon) {
                list.spawn(line(said));
            }
            if sends(wagon) {
                hints.push(crate::systems::keycap::Hint::key(KEYCODE_TAKE_ALL, "choose where on your map"));
            }
        }
        hints.push(crate::systems::keycap::Hint::key(KEYCODE_GROW, "grow"));
        hints.push(crate::systems::keycap::Hint::key(KEYCODE_SHRINK, "shrink"));
        hints.push(crate::systems::keycap::Hint::key(KEYCODE_CLOSE, "close"));
        crate::systems::keycap::hint_row(list, None, &hints);
    });
}

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

    /// A settler names what it raises and only what it lacks of it.
    #[test]
    fn a_settler_says_what_it_raises_and_lacks() {
        let tent = Wants { task: Some(Task::Raise(Fixture::Tent)), needs: Needs { hide: 3, wood: 0, stone: 1 } };
        assert_eq!(says(tent), vec!["Next I raise: tent".to_owned(), "For all the plan I still need 3 hide, 1 stone".to_owned()]);
        let paid = Wants { task: Some(Task::Raise(Fixture::Stakes)), needs: Needs::default() };
        assert_eq!(says(paid)[1], "I have all this hub's plan takes");
        assert_eq!(says(Wants { task: Some(Task::Clear), needs: Needs::default() }).len(), 1);
        assert_eq!(says(Wants { task: None, needs: Needs::default() }).len(), 1);
    }

    /// A settler says what a plan would do and what it still needs, and
    /// says so where it makes none.
    #[test]
    fn a_settler_says_what_a_plan_would_do() {
        let grow = Planned { grow: true, built: vec![(0, 0); 6], raised: vec![(0, 0); 20], pulled: vec![(0, 0); 11], hubs: 1, needs: Needs { hide: 24, wood: 0, stone: 0 }, ..Planned::default() };
        assert_eq!(
            plans(&Plan::Said(grow)),
            vec!["To grow I would build on 6 sites".to_owned(), "and take in 1 hub".to_owned(), "The wall: 20 stakes raised, 11 pulled".to_owned(), "It still needs 24 hide".to_owned()]
        );
        let shrink = Planned { gone: vec![(0, 0); 16], pulled: vec![(0, 0); 30], worn: vec![(0, 0); 3], faded: vec![(0, 0); 7], hubs: 1, ..Planned::default() };
        let said = plans(&Plan::Said(shrink));
        assert_eq!((said[0].as_str(), said[1].as_str(), said[4].as_str()), ("To shrink I would give up 16 sites", "and part this hub in 2", "I have what it takes"));
        assert_eq!(said[3], "The path moves: 3 tiles new, 7 given up");
        assert_eq!((plans(&Plan::Refused { grow: true }).len(), plans(&Plan::Refused { grow: false }).len()), (1, 1));
    }

    /// A settler says what its wagon holds, and asks for a map where the
    /// one in hand reaches no hub.
    #[test]
    fn a_settler_says_what_its_wagon_holds() {
        let empty = Wagon::default();
        assert_eq!(carts(&empty).len(), 2);
        let softwood = common::Stack { kind: common::Stackable::Material(common::Material::Softwood), count: 4 };
        let loaded = Wagon { load: vec![softwood], hubs: vec![(9, -2)] };
        assert_eq!(carts(&loaded), vec!["The wagon holds 4 softwood".to_owned(), "Your map shows a hub I can take it to".to_owned()]);
        assert!(sends(&loaded) && !sends(&empty) && !sends(&Wagon { hubs: vec![(9, -2)], ..Wagon::default() }));
    }
}
