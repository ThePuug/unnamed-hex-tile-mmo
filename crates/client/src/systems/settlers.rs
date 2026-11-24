//! Hailing a settler: T asks the settler nearest the player what it
//! raises next and what it still lacks, and a panel says so while the
//! player stands there. Numpad Enter hands over what the bag holds of
//! what is lacking. The server finds the settler and decides what is
//! handed over.

use bevy::prelude::*;
use common::building::{Needs, Wants};
use common_bevy::{
    components::{position::Position, Actor},
    message::{Do, Event, Try},
};

use crate::systems::{
    focus::{NumpadFocus, Panel},
    gathering::{KEYCODE_CLOSE, KEYCODE_TAKE_ALL},
};

/// The key that hails the settler nearest the player, and lets it be.
pub const KEYCODE_HAIL: KeyCode = KeyCode::KeyT;

/// The settler the player has hailed: what it said, and where the player
/// stood to hear it. None while the player has hailed none.
#[derive(Resource, Default)]
pub struct Hailed(pub Option<(Wants, Position)>);

/// What a settler says of `wants`, a line to each thing said.
pub fn says(wants: Wants) -> Vec<String> {
    let Some(fixture) = wants.raising else { return vec!["This camp wants nothing more of me".to_owned()] };
    let Needs { hide, wood, stone } = wants.needs;
    let lacking: Vec<String> = [(hide, "hide"), (wood, "wood"), (stone, "stone")].into_iter().filter(|&(count, _)| count > 0).map(|(count, name)| format!("{count} {name}")).collect();
    let needs = if lacking.is_empty() { "I have what it takes".to_owned() } else { format!("I still need {}", lacking.join(", ")) };
    vec![format!("Next I raise: {}", fixture.name().to_lowercase()), needs]
}

/// Hails on T, lets the settler be on T again, on the close key or when
/// the player moves, and hands over on numpad Enter.
pub fn request(
    mut keys: crate::systems::help::Keys,
    menu: Res<crate::plugins::shell::menu::GameMenu>,
    focus: Res<NumpadFocus>,
    mut hailed: ResMut<Hailed>,
    player: Query<(Entity, &Position), With<Actor>>,
    mut writer: MessageWriter<Try>,
) {
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
    if keys.take(KEYCODE_CLOSE, "Let the settler be") {
        hailed.0 = None;
    } else if keys.take(KEYCODE_TAKE_ALL, "Hand the settler what you carry of what it needs") {
        writer.write(Try { event: Event::Hail { ent, give: true } });
    }
}

/// Keeps what the hailed settler said as the server says it.
pub fn do_wants(mut reader: MessageReader<Do>, mut hailed: ResMut<Hailed>, player: Query<&Position, With<Actor>>) {
    for message in reader.read() {
        let Do { event: Event::Wants { ent, wants } } = message else { continue };
        let Ok(position) = player.get(*ent) else { continue };
        hailed.0 = wants.map(|wants| (wants, *position));
        if wants.is_none() {
            info!("hail: no settler is near enough to hear");
        }
    }
}

/// The panel's root, spawned with its first open and shown while open.
#[derive(Component)]
pub struct SettlerRows;

/// Shows the panel while a settler is hailed and lays its lines out again
/// whenever what it said changes.
pub fn update(mut commands: Commands, hailed: Res<Hailed>, mut rows: Query<(Entity, &mut Node), With<SettlerRows>>) {
    if !hailed.is_changed() {
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
        if !wants.needs.is_none() {
            hints.push(crate::systems::keycap::Hint::key(KEYCODE_TAKE_ALL, "hand over"));
        }
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
        let tent = Wants { raising: Some(Fixture::Tent), needs: Needs { hide: 3, wood: 0, stone: 1 } };
        assert_eq!(says(tent), vec!["Next I raise: tent".to_owned(), "I still need 3 hide, 1 stone".to_owned()]);
        let paid = Wants { raising: Some(Fixture::Stakes), needs: Needs::default() };
        assert_eq!(says(paid)[1], "I have what it takes");
        assert_eq!(says(Wants { raising: None, needs: Needs::default() }).len(), 1);
    }
}
