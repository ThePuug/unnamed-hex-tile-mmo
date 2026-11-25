//! The loot window: the piles on a tile, a cell to each of the tile's
//! slots, over the foot of the screen while it is open. A cell's digit
//! takes what lies in its slot, so a cell means the same slot whatever
//! is taken or laid round it.

use bevy::prelude::*;

use crate::systems::{
    bag_panel::cell_contents,
    equipment_panel::{CELL, CURSOR_ROW, OTHER_ROW, UNWORN},
    gathering::{LootWindow, ENTRY_KEYS},
};

/// The window's root, spawned with its first open and shown while open.
#[derive(Component)]
pub struct LootPanel;

/// The window's row of cells.
#[derive(Component)]
pub struct LootCells;

/// Shows the window while one is open and lays its cells out again
/// whenever what it holds changes.
pub fn update(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    window: Res<LootWindow>,
    mut panel: Query<&mut Node, With<LootPanel>>,
    cells: Query<Entity, With<LootCells>>,
) {
    if !window.is_changed() {
        return;
    }
    let Some(lying) = &window.cells else {
        if let Ok(mut node) = panel.single_mut() {
            node.display = Display::None;
        }
        return;
    };
    let cells = match (panel.single_mut(), cells.single()) {
        (Ok(mut node), Ok(cells)) => {
            node.display = Display::Flex;
            commands.entity(cells).despawn_related::<Children>();
            cells
        }
        _ => spawn(&mut commands),
    };
    for (slot, lot) in lying.iter().enumerate() {
        commands
            .spawn((
                Node {
                    width: Val::Px(CELL),
                    height: Val::Px(CELL),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::all(Val::Px(2.)),
                    border_radius: BorderRadius::all(Val::Px(4.)),
                    ..default()
                },
                BackgroundColor(if lot.is_some() { CURSOR_ROW } else { OTHER_ROW }),
                BorderColor::all(UNWORN),
                ChildOf(cells),
            ))
            .with_children(|cell| {
                if let Some(lot) = lot {
                    cell_contents(cell, &asset_server, *lot, 1.0);
                    crate::systems::keycap::corner_keycap(cell, ENTRY_KEYS[slot]);
                }
            });
    }
}

/// Spawns the window, shown, and returns its row of cells: the cells on
/// the left, and down the right the keys that take everything and close.
fn spawn(commands: &mut Commands) -> Entity {
    let mut cells = Entity::PLACEHOLDER;
    commands
        .spawn((
            LootPanel,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Percent(45.),
                left: Val::Percent(60.),
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(10.),
                padding: UiRect::all(Val::Px(10.)),
                border_radius: BorderRadius::all(Val::Px(6.)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.05, 0.05, 0.05, 0.95)),
        ))
        .with_children(|panel| {
            cells = panel
                .spawn((LootCells, Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(4.), ..default() }))
                .id();
            panel
                .spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(4.), ..default() })
                .with_children(|buttons| {
                    for (does, key) in [("Take all", crate::systems::gathering::KEYCODE_TAKE_ALL), ("Close", crate::systems::gathering::KEYCODE_CLOSE)] {
                        buttons
                            .spawn((
                                Node {
                                    width: Val::Px(120.),
                                    height: Val::Px((CELL - 4.0) / 2.0),
                                    justify_content: JustifyContent::SpaceBetween,
                                    align_items: AlignItems::Center,
                                    padding: UiRect::horizontal(Val::Px(8.)),
                                    column_gap: Val::Px(6.),
                                    border: UiRect::all(Val::Px(1.)),
                                    border_radius: BorderRadius::all(Val::Px(4.)),
                                    ..default()
                                },
                                BackgroundColor(OTHER_ROW),
                                BorderColor::all(UNWORN),
                            ))
                            .with_children(|b| {
                                b.spawn((
                                    Text::new(does),
                                    TextFont { font_size: FontSize::Px(13.0), ..default() },
                                    TextColor(Color::srgb(0.85, 0.85, 0.85)),
                                ));
                                crate::systems::keycap::keycap(b, key);
                            });
                    }
                });
        });
    cells
}
