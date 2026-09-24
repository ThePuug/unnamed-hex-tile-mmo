//! The loot window: what a pile holds, a stack to a cell in rows of nine,
//! over the foot of the screen while it is open. The digits take from the
//! row the cursor is on.

use bevy::prelude::*;

use crate::systems::{
    bag_panel::stack_icon,
    equipment_panel::{CELL, OTHER_ROW, UNWORN},
    gathering::{LootWindow, ENTRY_KEYS},
};

/// The window's root, spawned with its first open and shown while open.
#[derive(Component)]
pub struct LootPanel;

/// The window's rows of cells.
#[derive(Component)]
pub struct LootCells;

const CURSOR_ROW: Color = Color::srgba(0.25, 0.25, 0.25, 0.9);

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
    let Some(entries) = &window.entries else {
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
    for (r, row) in entries.chunks(ENTRY_KEYS.len()).enumerate() {
        let cursor = r == window.row;
        let row_cells = commands
            .spawn((Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(4.), ..default() }, ChildOf(cells)))
            .id();
        for (i, stack) in row.iter().enumerate() {
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
                    BackgroundColor(if cursor { CURSOR_ROW } else { OTHER_ROW }),
                    BorderColor::all(UNWORN),
                    ChildOf(row_cells),
                ))
                .with_children(|cell| {
                    cell.spawn((
                        ImageNode::new(stack_icon(&asset_server, stack.kind)),
                        Node { width: Val::Px(CELL - 8.0), height: Val::Px(CELL - 8.0), ..default() },
                    ));
                    if cursor {
                        crate::systems::keycap::corner_keycap(cell, &(i + 1).to_string());
                    }
                    cell.spawn((
                        Text::new(stack.count.to_string()),
                        TextFont { font_size: FontSize::Px(13.0), ..default() },
                        TextColor(Color::WHITE),
                        TextShadow::default(),
                        Node { position_type: PositionType::Absolute, bottom: Val::Px(2.), right: Val::Px(5.), ..default() },
                    ));
                });
        }
    }
}

/// Spawns the window, shown, and returns its rows of cells: the cells on
/// the left, and down the right the keys that take everything and close.
fn spawn(commands: &mut Commands) -> Entity {
    let mut cells = Entity::PLACEHOLDER;
    commands
        .spawn((
            LootPanel,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Percent(40.),
                right: Val::Px(24.),
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
                .spawn((LootCells, Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(4.), ..default() }))
                .id();
            panel
                .spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(4.), ..default() })
                .with_children(|buttons| {
                    for (does, key) in [("Take all", "Ent"), ("Close", "0")] {
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
