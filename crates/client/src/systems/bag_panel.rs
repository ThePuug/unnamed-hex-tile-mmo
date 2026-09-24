//! The character panel's bag tab: what the player carries, stack by stack,
//! with its count.

use bevy::prelude::*;

use common_bevy::components::{equipment::Inventory, Actor};

use crate::systems::character_panel::{CharacterPanelState, PanelTab, TabContent};

/// The bag's contents, one stack to a line.
#[derive(Component)]
pub struct BagList;

/// Spawns the tab's content under `content`, hidden until the tab is chosen.
pub fn spawn_tab(commands: &mut Commands, content: Entity) {
    commands
        .spawn((
            TabContent(PanelTab::Bag),
            Node {
                display: Display::None,
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(12.),
                min_width: Val::Px(320.),
                ..default()
            },
            ChildOf(content),
        ))
        .with_children(|tab| {
            tab.spawn((
                Text::new("Bag     - + tab     0 close"),
                TextFont { font_size: FontSize::Px(12.0), ..default() },
                TextColor(Color::srgb(0.6, 0.6, 0.6)),
            ));
            tab.spawn((
                BagList,
                Text::new(""),
                TextFont { font_size: FontSize::Px(16.0), ..default() },
                TextColor(Color::srgb(0.85, 0.85, 0.85)),
            ));
        });
}

/// Writes each material stack and its count while the tab shows.
pub fn update(
    state: Res<CharacterPanelState>,
    player: Query<&Inventory, With<Actor>>,
    mut list: Query<&mut Text, With<BagList>>,
) {
    if !state.visible || state.tab != PanelTab::Bag {
        return;
    }
    let (Ok(bag), Ok(mut list)) = (player.single(), list.single_mut()) else { return };
    let lines: Vec<String> = common::Material::ALL
        .iter()
        .filter(|&&m| bag.material(m) > 0)
        .map(|&m| format!("{:<16}{}", m.name(), bag.material(m)))
        .collect();
    let text = if lines.is_empty() { "Nothing gathered yet".to_string() } else { lines.join("\n") };
    if list.0 != text {
        list.0 = text;
    }
}
