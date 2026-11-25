//! The character panel's bag tab: what the player carries and does not
//! wear, every one of the bag's cells in rows of nine, a stack to a cell
//! with its count on its icon, and what it all weighs. A digit selects the
//! cell at that column of the cursor's row, and the detail beside the cells
//! shows what is selected and what can be done with it: dropped, or
//! while the player looks at a wagon (`settlers::Loading`), loaded on it.

use bevy::prelude::*;

use common_bevy::components::{
    equipment::{Equipment, Inventory, Item, Lot, BAG_STACKS, BURDEN_LIMIT, CARRY_LIMIT},
    Actor,
};
use common_bevy::message::{Event, Try};

use crate::{
    plugins::console::DevConsole,
    systems::{
        character_panel::{CharacterPanelState, PanelTab, TabContent},
        drop_panel::{DropChoice, KEYCODE_DROP},
        equipment_panel::{icon, BAG_WIDTH, CELL, CURSOR_ROW, OTHER_ROW, UNWORN},
        focus::{NumpadFocus, Panel},
        gathering::ENTRY_KEYS,
        keycap::{corner_keycap, hint_row, Hint},
    },
};

/// The detail's width beside the cells.
pub const DETAIL_WIDTH: f32 = 200.0;
const SELECTED: Color = Color::srgb(0.85, 0.65, 0.13);
const FAINT: Color = Color::srgb(0.55, 0.55, 0.55);

/// The weight and stack count over the cells.
#[derive(Component)]
pub struct BagSummary;

/// The bag's rows of cells.
#[derive(Component)]
pub struct BagCells;

/// The detail of what is selected, beside the cells.
#[derive(Component)]
pub struct BagDetail;

/// What a selection holds on to: a piece, or the stack of a kind however
/// many it holds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pick {
    Piece(Item),
    Stack(common::Stackable),
}

impl Pick {
    fn of(cell: Lot) -> Pick {
        match cell {
            Lot::Piece(item) => Pick::Piece(item),
            Lot::Stack(stack) => Pick::Stack(stack.kind),
        }
    }
}

/// The bag's cells in the order the tab lays them out: its pieces in the
/// order the player came by them, then its stacks in the order they came.
fn cells_of(bag: &Inventory, worn: &Equipment) -> Vec<Lot> {
    bag.bagged(worn).map(Lot::Piece).chain(bag.stock.iter().copied().map(Lot::Stack)).collect()
}

/// How many rows the tab lays out for `cells`: every one of the bag's
/// stacks, and any cells past them.
fn rows(cells: usize) -> usize {
    BAG_STACKS.max(cells).div_ceil(BAG_WIDTH)
}

/// The row of the bag tab the digits act on, and what is selected. A
/// selection goes when what it held leaves the bag.
#[derive(Resource, Default)]
pub struct BagCursor {
    pub row: usize,
    pub picked: Option<Pick>,
}

impl BagCursor {
    /// The cell selected among `cells`, where it is still there.
    fn selected(&self, cells: &[Lot]) -> Option<Lot> {
        cells.iter().copied().find(|&cell| Some(Pick::of(cell)) == self.picked)
    }
}

/// While the bag tab has the numpad, a digit selects the cell at that
/// column of the cursor's row, or nothing where the cell is empty; `.`
/// moves the cursor down a row, wrapping to the top; Enter opens the drop
/// panel on the selected stack, or drops the selected piece.
pub fn handle_numpad(
    mut keys: crate::systems::help::Keys,
    console: Res<DevConsole>,
    focus: Res<NumpadFocus>,
    state: Res<CharacterPanelState>,
    mut cursor: ResMut<BagCursor>,
    mut choice: ResMut<DropChoice>,
    loading: Res<crate::systems::settlers::Loading>,
    player: Query<(Entity, &Inventory, &Equipment), With<Actor>>,
    mut writer: MessageWriter<Try>,
) {
    if !focus.has(Panel::Character) || console.visible || state.tab != PanelTab::Bag {
        return;
    }
    let Ok((ent, bag, worn)) = player.single() else { return };
    let cells = cells_of(bag, worn);
    if keys.take(KeyCode::NumpadDecimal, "Show the bag's next row") {
        cursor.row = (cursor.row + 1) % rows(cells.len());
    }
    for (column, key) in ENTRY_KEYS.iter().enumerate() {
        if keys.take(*key, "Select the stack in this column") {
            cursor.picked = cells.get(cursor.row * BAG_WIDTH + column).map(|&cell| Pick::of(cell));
        }
    }
    let does = if loading.0.is_some() { "Choose how much of the selected stack to load on the wagon" } else { "Choose how much of the selected stack to drop, or drop the selected piece" };
    if keys.take(KEYCODE_DROP, does) {
        match cursor.selected(&cells) {
            Some(Lot::Stack(stack)) => choice.open(stack),
            // A wagon carries materials and no pieces
            Some(Lot::Piece(_)) if loading.0.is_some() => {}
            Some(lot @ Lot::Piece(_)) => {
                writer.write(Try { event: Event::Drop { ent, lot } });
            }
            None => {}
        }
    }
}

/// A stackable kind's icon: the build writes a material's under its name.
pub(crate) fn stack_icon(asset_server: &AssetServer, kind: common::Stackable) -> Handle<Image> {
    let common::Stackable::Material(material) = kind;
    let stem = material.stem();
    asset_server.load(format!("icons/{stem}.png"))
}

/// Spawns the tab's content under `content`, hidden until the tab is chosen.
pub fn spawn_tab(commands: &mut Commands, content: Entity) {
    commands
        .spawn((
            TabContent(PanelTab::Bag),
            Node {
                display: Display::None,
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.),
                ..default()
            },
            ChildOf(content),
        ))
        .with_children(|tab| {
            tab.spawn((
                BagSummary,
                Text::new(""),
                TextFont { font_size: FontSize::Px(14.0), ..default() },
                TextColor(Color::srgb(0.85, 0.85, 0.85)),
            ));
            hint_row(tab, None, &[Hint::range(&ENTRY_KEYS, "select"), Hint::key(KeyCode::NumpadDecimal, "next row")]);
            tab.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(12.), ..default() }).with_children(|row| {
                row.spawn((
                    BagCells,
                    Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(4.), ..default() },
                ));
                row.spawn((
                    BagDetail,
                    Node {
                        width: Val::Px(DETAIL_WIDTH),
                        flex_shrink: 0.0,
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(6.),
                        padding: UiRect::all(Val::Px(10.)),
                        border: UiRect::all(Val::Px(1.)),
                        border_radius: BorderRadius::all(Val::Px(4.)),
                        ..default()
                    },
                    BackgroundColor(OTHER_ROW),
                    BorderColor::all(UNWORN),
                ));
            });
        });
}

/// What the tab last laid out: the cells, the cursor's row, what is
/// selected, the stack the drop panel is open on, and whether a wagon is
/// being loaded.
type Shown = (Vec<Lot>, usize, Option<Lot>, Option<common::Stackable>, bool);

/// While the tab shows, writes the weight carried and lays the cells and
/// the detail out again whenever the bag's stacks, the cursor's row, the
/// selection or the drop panel change.
#[allow(clippy::too_many_arguments)]
pub fn update(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    state: Res<CharacterPanelState>,
    mut cursor: ResMut<BagCursor>,
    choice: Res<DropChoice>,
    loading: Res<crate::systems::settlers::Loading>,
    player: Query<(&Inventory, &Equipment), With<Actor>>,
    mut summary: Query<&mut Text, With<BagSummary>>,
    cells: Query<Entity, With<BagCells>>,
    detail: Query<Entity, With<BagDetail>>,
    mut shown: Local<Option<Shown>>,
) {
    if !state.visible || state.tab != PanelTab::Bag {
        *shown = None;
        return;
    }
    let (Ok((bag, worn)), Ok(mut summary), Ok(cells), Ok(detail)) =
        (player.single(), summary.single_mut(), cells.single(), detail.single())
    else {
        return;
    };

    let burden = if bag.is_burdened() { "     overburdened" } else { "" };
    let text = format!(
        "Weight {} / {}, slowed past {}{burden}     Stacks {} / {}",
        bag.weight(), CARRY_LIMIT, BURDEN_LIMIT, bag.stacks(worn), BAG_STACKS,
    );
    if summary.0 != text {
        summary.0 = text;
    }

    let all = cells_of(bag, worn);
    let row = cursor.row.min(rows(all.len()) - 1);
    let selected = cursor.selected(&all);
    if cursor.row != row || (cursor.picked.is_some() && selected.is_none()) {
        cursor.row = row;
        cursor.picked = selected.map(Pick::of);
    }
    let now = (all, row, selected, choice.0.map(|c| c.kind), loading.0.is_some());
    if shown.as_ref() == Some(&now) {
        return;
    }
    let (all, row, selected, dropping, wagon) = &now;

    commands.entity(cells).despawn_related::<Children>();
    let slots: Vec<Option<Lot>> = (0..rows(all.len()) * BAG_WIDTH).map(|k| all.get(k).copied()).collect();
    for (r, cells_of_row) in slots.chunks(BAG_WIDTH).enumerate() {
        let on_cursor = r == *row;
        commands
            .spawn((Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(4.), ..default() }, ChildOf(cells)))
            .with_children(|row_cells| {
                for (column, &slot) in cells_of_row.iter().enumerate() {
                    let edge = if slot.is_some() && slot == *selected { SELECTED } else { UNWORN };
                    row_cells
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
                            BackgroundColor(if on_cursor { CURSOR_ROW } else { OTHER_ROW }),
                            BorderColor::all(edge),
                        ))
                        .with_children(|cell| {
                            let Some(slot) = slot else { return };
                            cell_contents(cell, &asset_server, slot, 1.0);
                            if on_cursor {
                                corner_keycap(cell, ENTRY_KEYS[column]);
                            }
                        });
                }
            });
    }

    commands.entity(detail).despawn_related::<Children>();
    let count = choice.0.map_or(0, |c| c.count);
    commands.entity(detail).with_children(|pane| spawn_detail(pane, &asset_server, *selected, dropping.map(|kind| (kind, count)), *wagon));
    *shown = Some(now);
}

/// A cell's icon, and a stack's count on it, at `scale` of a cell.
pub(crate) fn cell_contents(cell: &mut ChildSpawnerCommands, asset_server: &AssetServer, slot: Lot, scale: f32) {
    let (image, count) = match slot {
        Lot::Piece(item) => (icon(asset_server, item), None),
        Lot::Stack(stack) => (stack_icon(asset_server, stack.kind), Some(stack.count)),
    };
    let side = (CELL - 8.0) * scale;
    cell.spawn((ImageNode::new(image), Node { width: Val::Px(side), height: Val::Px(side), ..default() }));
    if let Some(n) = count {
        cell.spawn((
            Text::new(n.to_string()),
            TextFont { font_size: FontSize::Px(13.0 * scale), ..default() },
            TextColor(Color::WHITE),
            TextShadow::default(),
            Node { position_type: PositionType::Absolute, bottom: Val::Px(2.), right: Val::Px(5.), ..default() },
        ));
    }
}

/// A piece's name as the detail writes it: its stem in words.
fn piece_title(item: Item) -> String {
    let words = item.piece.name().replace('-', " ");
    let mut chars = words.chars();
    chars.next().map_or_else(String::new, |first| first.to_uppercase().chain(chars).collect())
}

/// The detail of `selected`: its icon, name and weight, and under them
/// what can be done with it — the drop panel where it is open on the
/// stack, choosing the count it carries, the key that opens it otherwise;
/// for a piece, the key that drops it. With `wagon`, a stack is loaded on
/// the wagon looked at where it would be dropped, and a piece stays.
fn spawn_detail(pane: &mut ChildSpawnerCommands, asset_server: &AssetServer, selected: Option<Lot>, dropping: Option<(common::Stackable, u32)>, wagon: bool) {
    let does = if wagon { "load on the wagon" } else { "drop" };
    let line = |pane: &mut ChildSpawnerCommands, text: String, size: f32, color: Color| {
        pane.spawn((Text::new(text), TextFont { font_size: FontSize::Px(size), ..default() }, TextColor(color)));
    };
    let Some(slot) = selected else {
        line(pane, "Nothing selected".into(), 13.0, FAINT);
        return;
    };
    pane.spawn(Node {
        width: Val::Px(CELL * 1.25),
        height: Val::Px(CELL * 1.25),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    })
    .with_children(|holder| cell_contents(holder, asset_server, slot, 1.25));
    match slot {
        Lot::Piece(item) => {
            line(pane, piece_title(item), 15.0, Color::srgb(0.9, 0.9, 0.9));
            line(pane, format!("Weighs {}", item.piece.weight()), 13.0, FAINT);
            if wagon {
                line(pane, "A wagon carries materials".into(), 13.0, FAINT);
            } else {
                hint_row(pane, None, &[Hint::key(KEYCODE_DROP, does)]);
            }
        }
        Lot::Stack(stack) => {
            line(pane, stack.kind.name().into(), 15.0, Color::srgb(0.9, 0.9, 0.9));
            line(pane, format!("{} carried, weighing {}", stack.count, stack.weight()), 13.0, FAINT);
            if let Some((_, count)) = dropping.filter(|&(kind, _)| kind == stack.kind) {
                crate::systems::drop_panel::spawn_entry(pane, count, does);
            } else {
                hint_row(pane, None, &[Hint::key(KEYCODE_DROP, does)]);
            }
        }
    }
}
