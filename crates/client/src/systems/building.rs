//! Building on the client: B opens the build panel on the tile the player
//! faces, which lists each kind the bag can pay for and the tile can take, or
//! the taking down of what stands there; a digit asks the server for it.
//! The server finds the tile itself, from where the player stands and
//! faces.

use bevy::prelude::*;
use common::building::{self, Construct, Kind};
use common_bevy::{
    components::{equipment::Inventory, heading::Heading, position::Position, Actor},
    message::{Do, Event, Try},
    resources::map::Map,
};
use qrz::Qrz;

use crate::systems::{
    equipment_panel::{OTHER_ROW, UNWORN},
    focus::{NumpadFocus, Panel},
    gathering::{ENTRY_KEYS, KEYCODE_CLOSE},
};

/// The key that opens the build panel, and shuts it.
pub const KEYCODE_BUILD: KeyCode = KeyCode::KeyB;

/// What a row of the build panel asks for: a construct built, or the one
/// standing on the tile taken down.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Choice {
    Build(Construct),
    TakeDown(Construct),
}

impl Choice {
    fn says(self) -> String {
        match self {
            Choice::Build(construct) => {
                let cost = construct.cost();
                format!("{} — {} {}", construct.kind.name(), cost.count, cost.kind.name())
            }
            Choice::TakeDown(construct) => format!("Take down the {}", construct.name().to_lowercase()),
        }
    }
}

/// The build panel: whether it is open, and the rows it shows, a digit
/// to each.
#[derive(Resource, Default)]
pub struct BuildPanel {
    pub open: bool,
    choices: Vec<Choice>,
}

/// Where the local player is at home, as the server last said: the tile
/// it stands on at its hearth, or none where its home is the haven.
#[derive(Resource, Default)]
pub struct HomeHearth(pub Option<Qrz>);

/// The tile a player at `position` facing `heading` builds on: the one
/// its heading is nearest, as the server finds it.
pub fn faced(map: &Map, position: &Position, heading: Heading) -> Qrz {
    position.reached(map) + heading.hex_dir()
}

/// What the build panel offers on a tile with `cover` to a player whose
/// bag is `bag`: the taking down of the construct standing there, or else
/// each kind the tile can take, once, in the first of its materials the
/// bag pays for it whole.
pub fn choices(cover: common::Cover, bag: &Inventory) -> Vec<Choice> {
    if let Some(construct) = building::standing(cover) {
        return vec![Choice::TakeDown(construct)];
    }
    let pays = |construct: &Construct| construct.raised(cover).is_some() && bag.count(construct.cost().kind) >= construct.cost().count;
    Kind::ALL.into_iter().filter_map(|kind| Construct::all().filter(|construct| construct.kind == kind).find(pays)).map(Choice::Build).collect()
}

/// Opens and shuts the panel on B, keeps its rows to the tile faced and
/// the bag, and asks the server for the row whose digit is pressed.
pub fn request(
    mut keys: crate::systems::help::Keys,
    menu: Res<crate::plugins::shell::menu::GameMenu>,
    focus: Res<NumpadFocus>,
    map: Res<Map>,
    mut panel: ResMut<BuildPanel>,
    player: Query<(Entity, &Position, &Heading, &Inventory), With<Actor>>,
    mut writer: MessageWriter<Try>,
) {
    let Ok((ent, position, heading, bag)) = player.single() else {
        panel.open = false;
        return;
    };
    if menu.open {
        return;
    }
    if keys.pressed(KEYCODE_BUILD, "Open the build panel on the tile you face, or shut it") {
        panel.open = !panel.open;
    }
    if !panel.open {
        return;
    }
    let tile = faced(&map, position, *heading);
    let offered = choices(map.cover_at(tile.q, tile.r), bag);
    if panel.choices != offered {
        panel.choices = offered;
    }
    if !focus.has(Panel::Build) {
        return;
    }
    if keys.take(KEYCODE_CLOSE, "Close the build panel") {
        panel.open = false;
        return;
    }
    for (row, key) in ENTRY_KEYS.iter().enumerate().take(panel.choices.len()) {
        if keys.take(*key, "Build this, or take it down") {
            let construct = match panel.choices[row] {
                Choice::Build(construct) => Some(construct),
                Choice::TakeDown(_) => None,
            };
            writer.write(Try { event: Event::Build { ent, construct } });
            panel.open = false;
        }
    }
}

/// Whether the local player stands on ground a hub holds, as the server
/// last said.
#[derive(Resource, Default)]
pub struct Sheltered(pub bool);

/// Keeps whether the local player stands on a hub's ground as the server
/// says.
pub fn do_sheltered(mut reader: MessageReader<Do>, mut sheltered: ResMut<Sheltered>, player: Query<(), With<Actor>>) {
    for message in reader.read() {
        let Do { event: Event::Sheltered { ent, held } } = message else { continue };
        if player.contains(*ent) && sheltered.0 != *held {
            sheltered.0 = *held;
        }
    }
}

/// Keeps where the local player is at home as the server says.
pub fn do_home(mut reader: MessageReader<Do>, mut home: ResMut<HomeHearth>, player: Query<(), With<Actor>>) {
    for message in reader.read() {
        let Do { event: Event::Home { ent, home: at } } = message else { continue };
        if player.contains(*ent) {
            home.0 = *at;
            info!("home: {}", at.map_or("the haven".to_owned(), |at| format!("the hearth on ({}, {})", at.q, at.r)));
        }
    }
}

/// The panel's root, spawned with its first open and shown while open.
#[derive(Component)]
pub struct BuildRows;

/// Shows the panel while it is open and lays its rows out again whenever
/// what it offers changes.
pub fn update(mut commands: Commands, panel: Res<BuildPanel>, mut rows: Query<(Entity, &mut Node), With<BuildRows>>) {
    if !panel.is_changed() {
        return;
    }
    let root = match rows.single_mut() {
        Ok((root, mut node)) => {
            node.display = if panel.open { Display::Flex } else { Display::None };
            commands.entity(root).despawn_related::<Children>();
            root
        }
        Err(_) if !panel.open => return,
        Err(_) => commands
            .spawn((
                BuildRows,
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
    if !panel.open {
        return;
    }
    let line = |text: String| (Text::new(text), TextFont { font_size: FontSize::Px(14.0), ..default() }, TextColor(Color::srgb(0.85, 0.85, 0.85)));
    commands.entity(root).with_children(|list| {
        if panel.choices.is_empty() {
            list.spawn(line("Nothing to build here with what you carry".to_owned()));
        }
        for (row, choice) in panel.choices.iter().enumerate() {
            list.spawn((
                Node {
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(8.),
                    padding: UiRect::axes(Val::Px(8.), Val::Px(4.)),
                    border: UiRect::all(Val::Px(1.)),
                    border_radius: BorderRadius::all(Val::Px(4.)),
                    ..default()
                },
                BackgroundColor(OTHER_ROW),
                BorderColor::all(UNWORN),
            ))
            .with_children(|cell| {
                crate::systems::keycap::keycap(cell, ENTRY_KEYS[row]);
                cell.spawn(line(choice.says()));
            });
        }
        crate::systems::keycap::hint_row(list, None, &[crate::systems::keycap::Hint::key(KEYCODE_CLOSE, "close")]);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::{Cover, Material, Stack, Stackable};

    /// The panel offers what the bag pays for and the tile takes, a kind
    /// once whatever the bag holds to build it of, and on a tile holding
    /// a construct only its taking down.
    #[test]
    fn the_panel_offers_what_the_bag_and_the_tile_allow() {
        let mut bag = Inventory::default();
        assert!(choices(Cover::NONE, &bag).is_empty());
        bag.add(Stack { kind: Stackable::Material(Material::Hardwood), count: 4 });
        let blockade = Construct { kind: Kind::Blockade, of: Material::Hardwood };
        assert_eq!(choices(Cover::NONE, &bag), vec![Choice::Build(blockade)]);
        bag.add(Stack { kind: Stackable::Material(Material::Softwood), count: 4 });
        assert_eq!(choices(Cover::NONE, &bag).len(), 1, "one blockade, of either wood");
        let tree = Cover::NONE.with(0, common::Content::Pine);
        assert!(choices(tree, &bag).is_empty(), "a blockade needs the whole tile");
        bag.add(Stack { kind: Stackable::Material(Material::Basement), count: 6 });
        let hearth = Construct { kind: Kind::Hearth, of: Material::Basement };
        assert!(choices(tree, &bag).is_empty(), "as does a hearth");
        assert_eq!(choices(Cover::NONE, &bag).first(), Some(&Choice::Build(hearth)));
        assert_eq!(choices(Cover::NONE, &bag).len(), Kind::ALL.len());
        let staked = blockade.raised(Cover::NONE).unwrap();
        assert_eq!(choices(staked, &bag), vec![Choice::TakeDown(blockade)]);
    }
}
