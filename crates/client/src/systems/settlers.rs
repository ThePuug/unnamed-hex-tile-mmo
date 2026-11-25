//! Hailing a settler: T asks the settler nearest the player what it
//! does next and what it still lacks, and a panel says so while the
//! player stands there. Numpad Enter hands over what the bag holds of
//! what is lacking. Numpad + asks what growing its hub a tier would do,
//! numpad - what shrinking it would, and numpad Enter then hands over
//! what a grow still lacks, or has the plan done.
//! A settler with nothing more to raise speaks of its wagon besides:
//! numpad Enter then loads the wagon with every material in the bag, and
//! a digit sends it to a hub the map in the player's hand reaches, the
//! map going with it. The server finds the settler and decides what is
//! handed over, what a plan does and where the wagon may go. Unbuilt: a
//! plan is told in words, and nothing marks its tiles on the ground.

use bevy::prelude::*;
use common::building::{Needs, Planned, Task, Wagon, Wants};
use common_bevy::{
    components::{position::Position, Actor},
    message::{Do, Event, Try},
};

use crate::systems::{
    focus::{NumpadFocus, Panel},
    gathering::{ENTRY_KEYS, KEYCODE_CLOSE, KEYCODE_TAKE_ALL},
};

/// The key that hails the settler nearest the player, and lets it be.
pub const KEYCODE_HAIL: KeyCode = KeyCode::KeyT;

/// The settler the player has hailed: what it said, and where the player
/// stood to hear it. None while the player has hailed none.
#[derive(Resource, Default)]
pub struct Hailed(pub Option<(Wants, Position)>);

/// What the hailed settler said of its camp's wagon; None where it has
/// none to speak of.
#[derive(Resource, Default)]
pub struct Carted(pub Option<Wagon>);

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

/// What a settler says of its `wagon`, a line to each thing said.
pub fn carts(wagon: &Wagon) -> Vec<String> {
    let load: Vec<String> = wagon.load.iter().map(|stack| format!("{} {}", stack.count, stack.kind.name().to_lowercase())).collect();
    let holds = if load.is_empty() { "The wagon stands empty".to_owned() } else { format!("The wagon holds {}", load.join(", ")) };
    let mut said = vec![holds];
    if wagon.hubs.is_empty() {
        said.push("Hold a map that reaches another hub and I will take it there".to_owned());
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
    player: Query<(Entity, &Position), With<Actor>>,
    mut writer: MessageWriter<Try>,
) {
    if hailed.0.is_none() && carted.0.is_some() {
        carted.0 = None;
    }
    if hailed.0.is_none() && (asked.grow.is_some() || asked.plan.is_some()) {
        *asked = Asked::default();
    }
    let Ok((ent, position)) = player.single() else {
        hailed.0 = None;
        return;
    };
    if menu.open {
        return;
    }
    let hail = keys.pressed(KEYCODE_HAIL, "Hail the settler beside you, or let it be");
    let Some((_, stood)) = hailed.0 else {
        if hail {
            writer.write(Try { event: Event::Hail { ent, give: false } });
        }
        return;
    };
    if hail || stood != *position {
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
    } else if keys.take(KEYCODE_TAKE_ALL, "Hand the settler what you carry of what it or its plan needs, load its wagon, or have its plan done") {
        match planned {
            Some(grow) => writer.write(Try { event: Event::Plan { ent, grow, carry: true } }),
            None => writer.write(Try { event: Event::Hail { ent, give: true } }),
        };
    }
    for (key, grow, does) in [(KEYCODE_GROW, true, "Ask the settler what growing its hub would do"), (KEYCODE_SHRINK, false, "Ask the settler what shrinking its hub would do")] {
        if keys.take(key, does) {
            asked.grow = Some(grow);
            writer.write(Try { event: Event::Plan { ent, grow, carry: false } });
        }
    }
    if planned.is_some() {
        return;
    }
    let hubs = carted.0.as_ref().map_or(Vec::new(), |wagon| wagon.hubs.clone());
    for (row, key) in ENTRY_KEYS.iter().enumerate().take(hubs.len()) {
        if keys.take(*key, "Send the wagon to this hub, by the map in your hand") {
            writer.write(Try { event: Event::Send { ent, to: hubs[row] } });
        }
    }
}

/// Keeps what the hailed settler said as the server says it.
pub fn do_wants(mut reader: MessageReader<Do>, mut hailed: ResMut<Hailed>, mut carted: ResMut<Carted>, mut asked: ResMut<Asked>, player: Query<&Position, With<Actor>>) {
    for message in reader.read() {
        match message {
            Do { event: Event::Wants { ent, wants } } => {
                let Ok(position) = player.get(*ent) else { continue };
                hailed.0 = wants.map(|wants| (wants, *position));
                if wants.is_none() {
                    info!("hail: no settler is near enough to hear");
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
            for (row, hub) in wagon.hubs.iter().enumerate().take(ENTRY_KEYS.len()) {
                list.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(8.), ..default() }).with_children(|cell| {
                    crate::systems::keycap::keycap(cell, ENTRY_KEYS[row]);
                    cell.spawn(line(format!("Send the wagon to the hub at ({}, {})", hub.0, hub.1)));
                });
            }
            hints.push(crate::systems::keycap::Hint::key(KEYCODE_TAKE_ALL, "load the wagon"));
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
        assert_eq!(carts(&loaded), vec!["The wagon holds 4 softwood".to_owned()]);
    }
}
