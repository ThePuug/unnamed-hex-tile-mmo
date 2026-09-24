//! The character panel's equipment tab: the six slots down the left of the
//! closeup and the bag below in rows of nine, worked from the numpad.

use bevy::prelude::*;

use common_bevy::{
    components::{
        equipment::{Equipment, Inventory, Item, Slot},
        Actor,
    },
    message::{Event, Try},
};

use crate::{
    plugins::console::DevConsole,
    systems::character_panel::{CharacterPanelState, PanelTab, TabContent},
};

/// Items to a bag row: one per digit key.
pub const BAG_WIDTH: usize = 9;

/// Where the closeup is drawn.
#[derive(Component)]
pub struct CloseupView;

/// The text naming an empty slot.
#[derive(Component)]
pub struct SlotText(pub Slot);

/// The icon of what a slot holds, and the item it shows.
#[derive(Component)]
pub struct SlotIcon {
    pub slot: Slot,
    shown: Option<Item>,
}

/// The icon asset an item is shown by: the piece alone, in the item's style.
pub(crate) fn icon(asset_server: &AssetServer, item: Item) -> Handle<Image> {
    asset_server.load(format!("icons/{}-{}.png", item.piece.name(), item.style))
}

/// The bag's rows of cells.
#[derive(Component)]
pub struct BagGrid;

/// One bag cell, by the item's index in the bag.
#[derive(Component)]
pub struct BagCell(pub usize);

/// A cell's digit key hint, shown on the cursor's row.
#[derive(Component)]
pub struct BagCellKey;

pub(crate) const CELL: f32 = 64.0;
/// A slot square: six of them stand as tall as the closeup.
const SLOT: f32 = 56.0;
const WORN: Color = Color::srgb(0.85, 0.65, 0.13);
pub(crate) const UNWORN: Color = Color::srgb(0.35, 0.35, 0.35);
const CURSOR_ROW: Color = Color::srgba(0.25, 0.25, 0.25, 0.9);
pub(crate) const OTHER_ROW: Color = Color::srgba(0.15, 0.15, 0.15, 0.8);

/// Spawns the tab's content under `content`, hidden until the tab is chosen.
pub fn spawn_tab(commands: &mut Commands, content: Entity) {
    commands
        .spawn((
            TabContent(PanelTab::Equipment),
            Node {
                display: Display::None,
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(12.),
                ..default()
            },
            ChildOf(content),
        ))
        .with_children(|tab| {
            tab.spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(16.),
                ..default()
            })
            .with_children(|row| {
                // The body's six slots, squares stacked down the closeup's left.
                row.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(4.),
                    ..default()
                })
                .with_children(|slots| {
                    for slot in Slot::BODY {
                        slot_square(slots, slot);
                    }
                });

                row.spawn((
                    CloseupView,
                    Node {
                        width: Val::Px(280.),
                        height: Val::Px(360.),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border: UiRect::all(Val::Px(1.)),
                        border_radius: BorderRadius::all(Val::Px(4.)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.05, 0.05, 0.05, 0.9)),
                    BorderColor::all(UNWORN),
                ));

                // Down the closeup's right, five squares held for accessories,
                // which are unbuilt, and under them the main hand, held for a
                // weapon, and the off hand, which holds a shield.
                row.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(4.),
                    ..default()
                })
                .with_children(|slots| {
                    for _ in 0..Slot::BODY.len() - 1 {
                        slots.spawn(held_square());
                    }
                    slots
                        .spawn(Node {
                            flex_direction: FlexDirection::Row,
                            column_gap: Val::Px(4.),
                            ..default()
                        })
                        .with_children(|hands| {
                            hands.spawn(held_square()).with_children(|cell| {
                                cell.spawn((
                                    Text::new("Main\nhand"),
                                    TextFont { font_size: FontSize::Px(11.0), ..default() },
                                    TextColor(Color::srgb(0.6, 0.6, 0.6)),
                                    TextLayout::justify(Justify::Center),
                                ));
                            });
                            slot_square(hands, Slot::OffHand);
                        });
                });
            });

            tab.spawn(Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.),
                ..default()
            })
            .with_children(|bag| {
                bag.spawn((
                    Text::new("Wearables     1-9 wear or take off     . next row     - + tab     C close"),
                    TextFont { font_size: FontSize::Px(12.0), ..default() },
                    TextColor(Color::srgb(0.6, 0.6, 0.6)),
                ));
                bag.spawn((
                    BagGrid,
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(4.),
                        ..default()
                    },
                ));
            });
        });
}

/// A slot's square under `parent`: its item's icon, or the slot's name while
/// it is empty.
fn slot_square(parent: &mut ChildSpawnerCommands, slot: Slot) {
    parent
        .spawn((
            Node {
                width: Val::Px(SLOT),
                height: Val::Px(SLOT),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(2.)),
                border_radius: BorderRadius::all(Val::Px(4.)),
                ..default()
            },
            BackgroundColor(OTHER_ROW),
            BorderColor::all(UNWORN),
        ))
        .with_children(|cell| {
            // Placed absolutely so that, hidden, it leaves the name centred
            // rather than pushed aside.
            cell.spawn((
                SlotIcon { slot, shown: None },
                ImageNode::default(),
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(SLOT - 8.0),
                    height: Val::Px(SLOT - 8.0),
                    ..default()
                },
                Visibility::Hidden,
            ));
            cell.spawn((
                SlotText(slot),
                Text::new(slot.name()),
                TextFont { font_size: FontSize::Px(11.0), ..default() },
                TextColor(Color::srgb(0.6, 0.6, 0.6)),
                TextLayout::justify(Justify::Center),
            ));
        });
}

/// A square held for a slot that is not built yet.
fn held_square() -> (Node, BackgroundColor, BorderColor) {
    (
        Node {
            width: Val::Px(SLOT),
            height: Val::Px(SLOT),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(Val::Px(2.)),
            border_radius: BorderRadius::all(Val::Px(4.)),
            ..default()
        },
        BackgroundColor(OTHER_ROW),
        BorderColor::all(Color::srgb(0.25, 0.25, 0.25)),
    )
}

/// The digits act on the bag; `-` and `+` move between tabs. The panel has
/// every numpad key but `0`, which is left to the jump so the player moves
/// with the panel open; `/` does not open the console over it. Nothing is
/// read while the console is open, which has the numpad then.
pub fn handle_numpad(
    mut keyboard: ResMut<ButtonInput<KeyCode>>,
    console: Res<DevConsole>,
    mut state: ResMut<CharacterPanelState>,
    player: Query<(Entity, &Inventory, &Equipment), With<Actor>>,
    mut writer: MessageWriter<Try>,
) {
    if !state.visible || console.visible {
        return;
    }
    if keyboard.clear_just_pressed(KeyCode::NumpadSubtract) {
        state.tab = state.tab.above();
    }
    if keyboard.clear_just_pressed(KeyCode::NumpadAdd) {
        state.tab = state.tab.below();
    }
    if state.tab != PanelTab::Equipment {
        return;
    }
    let Ok((ent, bag, equipment)) = player.single() else { return };
    if keyboard.clear_just_pressed(KeyCode::NumpadDecimal) {
        state.bag_row = (state.bag_row + 1) % rows(bag.items.len());
    }
    const DIGITS: [KeyCode; BAG_WIDTH] = [
        KeyCode::Numpad1, KeyCode::Numpad2, KeyCode::Numpad3,
        KeyCode::Numpad4, KeyCode::Numpad5, KeyCode::Numpad6,
        KeyCode::Numpad7, KeyCode::Numpad8, KeyCode::Numpad9,
    ];
    for (column, key) in DIGITS.iter().enumerate() {
        if !keyboard.clear_just_pressed(*key) {
            continue;
        }
        let Some(&item) = bag.items.get(state.bag_row * BAG_WIDTH + column) else { continue };
        writer.write(Try { event: Event::Wear { ent, item, on: !equipment.is_worn(item) } });
    }
}

/// The bag's row count, never less than one so the cursor has a row.
fn rows(items: usize) -> usize {
    items.div_ceil(BAG_WIDTH).max(1)
}

/// Lays the bag out again whenever its contents change.
pub fn rebuild_bag(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    player: Query<&Inventory, (With<Actor>, Changed<Inventory>)>,
    grid: Query<Entity, With<BagGrid>>,
) {
    let (Ok(bag), Ok(grid)) = (player.single(), grid.single()) else { return };
    commands.entity(grid).despawn_related::<Children>();
    for (r, row) in bag.items.chunks(BAG_WIDTH).enumerate() {
        commands
            .spawn((
                Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(4.), ..default() },
                ChildOf(grid),
            ))
            .with_children(|cells| {
                for (c, &item) in row.iter().enumerate() {
                    cells
                        .spawn((
                            BagCell(r * BAG_WIDTH + c),
                            Node {
                                width: Val::Px(CELL),
                                height: Val::Px(CELL),
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                border: UiRect::all(Val::Px(2.)),
                                border_radius: BorderRadius::all(Val::Px(4.)),
                                ..default()
                            },
                            BackgroundColor(OTHER_ROW),
                            BorderColor::all(UNWORN),
                        ))
                        .with_children(|cell| {
                            cell.spawn((
                                ImageNode::new(icon(&asset_server, item)),
                                Node { width: Val::Px(CELL - 8.0), height: Val::Px(CELL - 8.0), ..default() },
                            ));
                            cell.spawn((
                                BagCellKey,
                                Text::new((c + 1).to_string()),
                                TextFont { font_size: FontSize::Px(10.0), ..default() },
                                TextColor(WORN),
                                Node {
                                    position_type: PositionType::Absolute,
                                    top: Val::Px(2.),
                                    left: Val::Px(4.),
                                    ..default()
                                },
                                Visibility::Hidden,
                            ));
                        });
                }
            });
    }
}

/// Marks worn items and the cursor's row, and shows that row's digit keys.
pub fn update_bag(
    mut state: ResMut<CharacterPanelState>,
    player: Query<(&Inventory, &Equipment), With<Actor>>,
    mut cells: Query<(&BagCell, &mut BackgroundColor, &mut BorderColor)>,
    mut keys: Query<(&ChildOf, &mut Visibility), With<BagCellKey>>,
) {
    if !state.visible {
        return;
    }
    let Ok((bag, equipment)) = player.single() else { return };
    if state.bag_row >= rows(bag.items.len()) {
        state.bag_row = 0;
    }
    let in_row = |index: usize| index / BAG_WIDTH == state.bag_row;
    for (cell, mut background, mut border) in &mut cells {
        let Some(&item) = bag.items.get(cell.0) else { continue };
        *background = BackgroundColor(if in_row(cell.0) { CURSOR_ROW } else { OTHER_ROW });
        *border = BorderColor::all(if equipment.is_worn(item) { WORN } else { UNWORN });
    }
    for (child_of, mut visibility) in &mut keys {
        let shown = cells.get(child_of.parent()).is_ok_and(|(cell, ..)| in_row(cell.0));
        *visibility = if shown { Visibility::Inherited } else { Visibility::Hidden };
    }
}

/// Shows each slot's item by its icon, or the slot's name when empty.
pub fn update_slots(
    state: Res<CharacterPanelState>,
    asset_server: Res<AssetServer>,
    player: Query<&Equipment, With<Actor>>,
    mut icons: Query<(&mut SlotIcon, &mut ImageNode, &mut Visibility)>,
    mut names: Query<(&SlotText, &mut Visibility), Without<SlotIcon>>,
) {
    if !state.visible {
        return;
    }
    let Ok(equipment) = player.single() else { return };
    for (mut slot, mut image, mut visibility) in &mut icons {
        let worn = equipment.worn(slot.slot);
        if slot.shown == worn {
            continue;
        }
        slot.shown = worn;
        if let Some(item) = worn {
            image.image = icon(&asset_server, item);
        }
        *visibility = if worn.is_some() { Visibility::Inherited } else { Visibility::Hidden };
    }
    for (slot, mut visibility) in &mut names {
        let shown = equipment.worn(slot.0).is_none();
        let wanted = if shown { Visibility::Inherited } else { Visibility::Hidden };
        if *visibility != wanted {
            *visibility = wanted;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cursor_always_has_a_row_and_wraps_past_the_last() {
        assert_eq!(rows(0), 1);
        assert_eq!(rows(BAG_WIDTH), 1);
        assert_eq!(rows(BAG_WIDTH + 1), 2);
        assert_eq!(rows(2 * BAG_WIDTH), 2);
        assert_eq!((1 + 1) % rows(2 * BAG_WIDTH), 0);
    }

    #[test]
    fn tabs_stop_at_the_strip_ends() {
        assert_eq!(PanelTab::Attributes.above(), PanelTab::Attributes);
        assert_eq!(PanelTab::Attributes.below(), PanelTab::Equipment);
        assert_eq!(PanelTab::Equipment.below(), PanelTab::Bag);
        assert_eq!(PanelTab::Bag.below(), PanelTab::Bag);
        assert_eq!(PanelTab::Bag.above(), PanelTab::Equipment);
        assert_eq!(PanelTab::Equipment.above(), PanelTab::Attributes);
        assert_eq!(PanelTab::ALL.first().copied(), Some(PanelTab::default()));
    }
}
