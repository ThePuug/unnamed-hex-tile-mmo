//! The character panel's bag tab: what the player carries and does not
//! wear, every one of the bag's cells in rows of nine, a stack to a cell
//! with its count on its icon, and what it all weighs.

use bevy::prelude::*;

use common_bevy::components::{
    equipment::{Equipment, Inventory, Item, BAG_STACKS, BURDEN_LIMIT, CARRY_LIMIT},
    Actor,
};

use crate::systems::{
    character_panel::{CharacterPanelState, PanelTab, TabContent},
    equipment_panel::{icon, BAG_WIDTH, CELL, OTHER_ROW, UNWORN},
};

/// The weight and stack count over the cells.
#[derive(Component)]
pub struct BagSummary;

/// The bag's rows of cells.
#[derive(Component)]
pub struct BagCells;

/// What one of the bag's cells holds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cell {
    Piece(Item),
    Stack(common::Stack),
}

/// The bag's cells in the order the tab lays them out: its pieces in the
/// order the player came by them, then its stacks in the order they came.
fn cells_of(bag: &Inventory, worn: &Equipment) -> Vec<Cell> {
    bag.bagged(worn).map(Cell::Piece).chain(bag.stock.iter().copied().map(Cell::Stack)).collect()
}

/// A stackable kind's icon: the build writes a material's under its name.
fn stack_icon(asset_server: &AssetServer, kind: common::Stackable) -> Handle<Image> {
    let stem = match kind {
        common::Stackable::Material(common::Material::Softwood) => "softwood",
        common::Stackable::Material(common::Material::Hardwood) => "hardwood",
        common::Stackable::Material(common::Material::Sandstone) => "sandstone",
        common::Stackable::Material(common::Material::Limestone) => "limestone",
        common::Stackable::Material(common::Material::Basement) => "basement",
    };
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
                Text::new("Bag     - + tab     C close"),
                TextFont { font_size: FontSize::Px(12.0), ..default() },
                TextColor(Color::srgb(0.6, 0.6, 0.6)),
            ));
            tab.spawn((
                BagSummary,
                Text::new(""),
                TextFont { font_size: FontSize::Px(14.0), ..default() },
                TextColor(Color::srgb(0.85, 0.85, 0.85)),
            ));
            tab.spawn((
                BagCells,
                Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(4.), ..default() },
            ));
        });
}

/// While the tab shows, writes the weight carried and lays the cells out
/// again whenever the bag's stacks change.
pub fn update(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    state: Res<CharacterPanelState>,
    player: Query<(&Inventory, &Equipment), With<Actor>>,
    mut summary: Query<&mut Text, With<BagSummary>>,
    cells: Query<Entity, With<BagCells>>,
    mut shown: Local<Option<Vec<Cell>>>,
) {
    if !state.visible || state.tab != PanelTab::Bag {
        *shown = None;
        return;
    }
    let (Ok((bag, worn)), Ok(mut summary), Ok(cells)) = (player.single(), summary.single_mut(), cells.single()) else { return };

    let burden = if bag.is_burdened() { "     overburdened" } else { "" };
    let text = format!(
        "Weight {} / {}, slowed past {}{burden}     Stacks {} / {}",
        bag.weight(), CARRY_LIMIT, BURDEN_LIMIT, bag.stacks(worn), BAG_STACKS,
    );
    if summary.0 != text {
        summary.0 = text;
    }

    let now = cells_of(bag, worn);
    if shown.as_ref() == Some(&now) {
        return;
    }
    commands.entity(cells).despawn_related::<Children>();
    let slots: Vec<Option<Cell>> = (0..BAG_STACKS.max(now.len())).map(|k| now.get(k).copied()).collect();
    for row in slots.chunks(BAG_WIDTH) {
        commands
            .spawn((Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(4.), ..default() }, ChildOf(cells)))
            .with_children(|row_cells| {
                for &slot in row {
                    let contents = slot.map(|cell| match cell {
                        Cell::Piece(item) => (icon(&asset_server, item), None),
                        Cell::Stack(stack) => (stack_icon(&asset_server, stack.kind), Some(stack.count)),
                    });
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
                            BackgroundColor(OTHER_ROW),
                            BorderColor::all(UNWORN),
                        ))
                        .with_children(|cell| {
                            let Some((image, count)) = contents else { return };
                            cell.spawn((
                                ImageNode::new(image),
                                Node { width: Val::Px(CELL - 8.0), height: Val::Px(CELL - 8.0), ..default() },
                            ));
                            if let Some(n) = count {
                                cell.spawn((
                                    Text::new(n.to_string()),
                                    TextFont { font_size: FontSize::Px(13.0), ..default() },
                                    TextColor(Color::WHITE),
                                    TextShadow::default(),
                                    Node { position_type: PositionType::Absolute, bottom: Val::Px(2.), right: Val::Px(5.), ..default() },
                                ));
                            }
                        });
                }
            });
    }
    *shown = Some(now);
}
