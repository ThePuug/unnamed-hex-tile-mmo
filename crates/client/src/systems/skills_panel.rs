//! The character panel's skills tab, worked as the equipment tab is: the
//! bar as the action bar draws it, a picture whose caps name no key; every
//! skill a bar may hold in a slot of the same look under its name, in a
//! row of its kind, `.` stepping through the rows and a digit picking from
//! the row shown; then a bar key puts the skill picked on it, any skill on
//! any key, sent to the server at once (`Event::SetLoadout`), which takes
//! it out of combat. Beside the bar, the skill picked as a card: its name
//! and its endurance and recovery in pills over its icon, its kind and
//! line, and what it does for this character.

use bevy::prelude::*;

use common_bevy::{
    components::{loadout::Loadout, resources::CombatState, ActorAttributes},
    message::{AbilityType, Do, Event as GameEvent, SkillKind, Try},
    tuning::Tuning,
};

use crate::{
    components::Viewed,
    plugins::console::DevConsole,
    systems::{
        action_bar::{bar_grid, icon_path, slot_frame, SlotIcon, SlotKeybind, KEYS, READY, SLOT_PX},
        character_panel::{CharacterPanelState, PanelTab, TabContent},
        focus::{NumpadFocus, Panel},
        gathering::ENTRY_KEYS,
        keycap::{hint_row, Hint},
        skill_text::{does, name, seconds},
    },
};

const KEYCODE_NEXT_ROW: KeyCode = KeyCode::NumpadDecimal;

/// The rows of skills, a kind each
const ROWS: [SkillKind; 3] = [SkillKind::Strike, SkillKind::Reaction, SkillKind::Stance];

const PICKED: Color = Color::srgb(0.85, 0.65, 0.13);
const PLAIN: Color = Color::srgb(0.85, 0.85, 0.85);
const DIM: Color = Color::srgb(0.5, 0.5, 0.5);
const REFUSED: Color = Color::srgb(0.9, 0.45, 0.35);
const OFF_BAR: Color = Color::srgb(0.3, 0.3, 0.3);
/// The card: landscape, its icon the art beside its text
const CARD_WIDTH: f32 = 440.0;
const ART_PX: f32 = 112.0;
/// A cost's pill, wide enough for its longest number, a recovery to a
/// tenth: 1.5s
const PILL_WIDTH: f32 = 48.0;
const PILL_HEIGHT: f32 = 26.0;
const CARD_FILL: Color = Color::srgb(0.13, 0.12, 0.10);
const CARD_EDGE: Color = Color::srgb(0.62, 0.50, 0.30);
const TITLE_FILL: Color = Color::srgb(0.24, 0.21, 0.17);
const TEXT_FILL: Color = Color::srgb(0.19, 0.18, 0.15);
const TITLE: Color = Color::srgb(0.96, 0.92, 0.82);
/// The pills a skill's costs stand in: endurance green, recovery yellow,
/// each number dark on it
const ENDURANCE: Color = Color::srgb(0.30, 0.66, 0.30);
const RECOVERY: Color = Color::srgb(0.90, 0.76, 0.20);
const ON_COST: Color = Color::srgb(0.08, 0.08, 0.06);
/// A number on a card its attribute line scales, so a player sees what
/// committing to the line changes
const SCALED: Color = Color::srgb(0.45, 0.80, 1.0);
const RULES_FONT: f32 = 15.0;
/// A row's slot, and the width of its tile: the slot over its name
const POOL_PX: f32 = 64.0;
const TILE_PX: f32 = POOL_PX + 20.0;
const NAME_FONT: f32 = 13.0;
/// A name's room: two lines, whichever it takes, so no row shifts as
/// another is shown
const NAME_PX: f32 = 2.0 * NAME_FONT * 1.3;

/// The tab's picture of the bar, its slots laid anew as the bar changes.
#[derive(Component)]
pub struct PanelBar;

/// One of the bar's slots in the tab, by its place on the bar.
#[derive(Component)]
pub struct BarSlot(pub usize);

/// A skill of a row, by its row and its place in it: on its slot and on
/// its name.
#[derive(Clone, Copy, Component)]
pub struct PoolEntry {
    row: usize,
    column: usize,
}

/// The card of the skill picked, shown while one is.
#[derive(Component)]
pub struct SkillCard;

/// What a text of the card says of the skill picked, or, under it, why the
/// skill stays off the bar for now.
#[derive(Clone, Copy, Component)]
pub enum Said {
    Name,
    Endurance,
    Recovery,
    Kind,
    Refusal,
}

/// The card's text of what the skill does: a span for each piece of it,
/// those its line scales coloured apart, laid anew as the pieces change.
#[derive(Component, Default)]
pub struct CardRules(Vec<(String, bool)>);

/// The card's art: the skill picked's icon, loaded as the skill changes.
#[derive(Component, Default)]
pub struct CardArt(Option<AbilityType>);

/// The skills of row `row`: those of its kind a bar may hold.
fn row(row: usize) -> Vec<AbilityType> {
    Loadout::pool().filter(|ability| ability.kind() == ROWS[row]).collect()
}

fn kind_name(kind: SkillKind) -> &'static str {
    match kind {
        SkillKind::Strike => "Strikes",
        SkillKind::Reaction => "Reactions",
        SkillKind::Stance => "Stances",
    }
}

fn text(color: Color, size: f32) -> (Text, TextFont, TextColor) {
    (Text::new(""), TextFont { font_size: FontSize::Px(size), ..default() }, TextColor(color))
}

/// The tile of `entry` under `parent`: its skill's slot, the digit that
/// picks it in the corner, over its name.
fn pool_tile(parent: &mut ChildSpawnerCommands, entry: PoolEntry, asset_server: &AssetServer) {
    let skill = row(entry.row)[entry.column];
    parent
        .spawn(Node { width: Val::Px(TILE_PX), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: Val::Px(2.), ..default() })
        .with_children(|tile| {
            slot_frame(tile, ENTRY_KEYS[entry.column], Some(asset_server.load(icon_path(skill))), POOL_PX, true).insert(entry);
            tile.spawn((
                entry,
                Text::new(name(skill)),
                TextFont { font_size: FontSize::Px(NAME_FONT), ..default() },
                TextColor(PLAIN),
                TextLayout::justify(Justify::Center),
                Node { width: Val::Px(TILE_PX), height: Val::Px(NAME_PX), ..default() },
            ));
        });
}

/// A pill of `fill` under `parent`, a cost's number on it, one size
/// whatever the number.
fn cost_pill(parent: &mut ChildSpawnerCommands, said: Said, fill: Color) {
    parent
        .spawn((
            Node {
                width: Val::Px(PILL_WIDTH),
                height: Val::Px(PILL_HEIGHT),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(2.)),
                border_radius: BorderRadius::MAX,
                ..default()
            },
            BackgroundColor(fill),
            BorderColor::all(ON_COST),
        ))
        .with_children(|pill| {
            pill.spawn((said, text(ON_COST, 13.0)));
        });
}

/// The card under `parent`, hidden until a skill is picked: a title bar
/// with its name and its cost pills, then its icon framed as the art
/// beside a box with its kind and line over what it does.
fn spawn_card(parent: &mut ChildSpawnerCommands) {
    parent
        .spawn((
            SkillCard,
            Visibility::Hidden,
            Node {
                width: Val::Px(CARD_WIDTH),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.),
                padding: UiRect::all(Val::Px(6.)),
                border: UiRect::all(Val::Px(3.)),
                border_radius: BorderRadius::all(Val::Px(10.0)),
                ..default()
            },
            BackgroundColor(CARD_FILL),
            BorderColor::all(CARD_EDGE),
        ))
        .with_children(|card| {
            card.spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    padding: UiRect::axes(Val::Px(10.), Val::Px(4.)),
                    border: UiRect::all(Val::Px(1.)),
                    border_radius: BorderRadius::all(Val::Px(6.0)),
                    ..default()
                },
                BackgroundColor(TITLE_FILL),
                BorderColor::all(CARD_EDGE),
            ))
            .with_children(|bar| {
                bar.spawn((Said::Name, text(TITLE, 18.0)));
                bar.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(6.), ..default() }).with_children(|costs| {
                    cost_pill(costs, Said::Endurance, ENDURANCE);
                    cost_pill(costs, Said::Recovery, RECOVERY);
                });
            });
            card.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(8.), ..default() }).with_children(|body| {
                body.spawn((
                    Node { border: UiRect::all(Val::Px(2.)), flex_shrink: 0.0, align_self: AlignSelf::FlexStart, ..default() },
                    BorderColor::all(CARD_EDGE),
                ))
                .with_children(|frame| {
                    frame.spawn((CardArt::default(), ImageNode::default(), Node { width: Val::Px(ART_PX), height: Val::Px(ART_PX), ..default() }));
                });
                body.spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        flex_grow: 1.0,
                        row_gap: Val::Px(6.),
                        // As tall as the art, whatever the skill says, so
                        // no skill's card stands taller than another's
                        height: Val::Px(ART_PX + 4.0),
                        overflow: Overflow::clip(),
                        padding: UiRect::all(Val::Px(8.)),
                        border: UiRect::all(Val::Px(1.)),
                        border_radius: BorderRadius::all(Val::Px(6.0)),
                        ..default()
                    },
                    BackgroundColor(TEXT_FILL),
                    BorderColor::all(CARD_EDGE),
                ))
                .with_children(|rules| {
                    rules.spawn((Said::Kind, text(DIM, 13.0)));
                    rules.spawn((CardRules::default(), Text::new(""), TextFont { font_size: FontSize::Px(RULES_FONT), ..default() }));
                });
            });
        });
}

/// Spawns the tab's content under `content`, hidden until the tab is chosen.
pub fn spawn_tab(commands: &mut Commands, content: Entity, asset_server: &AssetServer) {
    commands
        .spawn((
            TabContent(PanelTab::Skills),
            Node { display: Display::None, flex_direction: FlexDirection::Column, row_gap: Val::Px(10.), ..default() },
            ChildOf(content),
        ))
        .with_children(|tab| {
            hint_row(tab, None, &[
                Hint::key(KEYCODE_NEXT_ROW, "next row"),
                Hint::range(&ENTRY_KEYS, "pick"),
                Hint::range(&KEYS, "put it on that key, out of combat"),
            ]);
            // The bar, and beside it what the skill picked is
            tab.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(20.), ..default() }).with_children(|top| {
                top.spawn((bar_grid(), PanelBar));
                top.spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(8.), ..default() }).with_children(|side| {
                    spawn_card(side);
                    side.spawn((Said::Refusal, text(REFUSED, 14.0)));
                });
            });
            tab.spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(6.), ..default() }).with_children(|rows| {
                for (at, kind) in ROWS.into_iter().enumerate() {
                    rows.spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::FlexStart, ..default() }).with_children(|line| {
                        line.spawn((Text::new(kind_name(kind)), TextFont { font_size: FontSize::Px(14.0), ..default() }, TextColor(DIM), Node { width: Val::Px(80.0), ..default() }));
                        for column in 0..row(at).len() {
                            pool_tile(line, PoolEntry { row: at, column }, asset_server);
                        }
                    });
                }
            });
        });
}

/// While the skills tab has the numpad: `.` shows the next row of skills,
/// letting the one picked go; a digit picks the skill in its column of the
/// row shown, or lets it go if it is the one picked. Off the tab, nothing
/// stays picked.
pub fn handle_numpad(
    mut keys: crate::systems::help::Keys,
    console: Res<DevConsole>,
    focus: Res<NumpadFocus>,
    mut state: ResMut<CharacterPanelState>,
) {
    if state.tab != PanelTab::Skills || !state.visible {
        state.picked = None;
        return;
    }
    if !focus.has(Panel::Character) || console.visible {
        return;
    }
    if keys.take(KEYCODE_NEXT_ROW, "Show the next row of skills") {
        state.skill_row = (state.skill_row + 1) % ROWS.len();
        state.picked = None;
    }
    let skills = row(state.skill_row);
    for (column, key) in ENTRY_KEYS.iter().enumerate() {
        if !keys.take(*key, "Pick the skill in this column, to put on a bar key") {
            continue;
        }
        let Some(&skill) = skills.get(column) else { continue };
        state.picked = (state.picked != Some(skill)).then_some(skill);
    }
}

/// A bar key pressed while a skill is picked puts it there, out of combat:
/// the bar with it is sent, and the skill let go. In combat the key does
/// nothing and the skill stays picked.
pub fn place(
    mut keys: crate::systems::help::Keys,
    mut state: ResMut<CharacterPanelState>,
    player: Query<(Entity, &Loadout, Option<&CombatState>), With<Viewed>>,
    mut writer: MessageWriter<Try>,
) {
    let Some(skill) = state.placing() else { return };
    let Ok((ent, held, combat)) = player.single() else { return };
    let fighting = combat.is_some_and(|combat| combat.in_combat);
    for (at, key) in KEYS.into_iter().enumerate() {
        let does = if fighting { format!("Put {} on this key, once out of combat", name(skill)) } else { format!("Put {} on this key", name(skill)) };
        if !keys.take(key, does) || fighting {
            continue;
        }
        let bar = held.placed(skill, at);
        if bar != *held {
            writer.write(Try { event: GameEvent::SetLoadout { ent, loadout: bar } });
        }
        state.picked = None;
        return;
    }
}

/// Puts the bar the server took on the player.
pub fn handle_loadout_taken(mut reader: MessageReader<Do>, mut player: Query<&mut Loadout>) {
    for message in reader.read() {
        let GameEvent::SetLoadout { ent, loadout } = &message.event else { continue };
        if let Ok(mut held) = player.get_mut(*ent) {
            *held = *loadout;
        }
    }
}

/// Lays the tab's bar anew as the bar the player holds changes: a slot
/// for each key as the action bar draws it, its cap naming no key.
pub fn sync_bar(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    player: Query<&Loadout, With<Viewed>>,
    bar: Query<Entity, With<PanelBar>>,
    mut shown: Local<Option<Loadout>>,
) {
    let (Ok(held), Ok(bar)) = (player.single(), bar.single()) else { return };
    if *shown == Some(*held) {
        return;
    }
    *shown = Some(*held);
    commands.entity(bar).despawn_related::<Children>();
    commands.entity(bar).with_children(|bar| {
        for (at, (key, skill)) in KEYS.into_iter().zip(held.0).enumerate() {
            slot_frame(bar, key, Some(asset_server.load(icon_path(skill))), SLOT_PX, false).insert(BarSlot(at));
        }
    });
}

/// Marks the skill picked, on the bar and in its row; shows the digits on
/// the row shown and dims the rest; and marks each skill on the bar.
#[allow(clippy::type_complexity)]
pub fn update(
    state: Res<CharacterPanelState>,
    player: Query<&Loadout, With<Viewed>>,
    mut slots: Query<(&BarSlot, &mut BorderColor), Without<PoolEntry>>,
    mut entries: Query<(&PoolEntry, &Children, &mut BorderColor), Without<BarSlot>>,
    mut names: Query<(&PoolEntry, &mut TextColor), With<Text>>,
    mut icons: Query<&mut ImageNode, With<SlotIcon>>,
    mut digits: Query<&mut Visibility, With<SlotKeybind>>,
) {
    if !state.visible || state.tab != PanelTab::Skills {
        return;
    }
    let Ok(held) = player.single() else { return };
    for (slot, mut border) in &mut slots {
        let edge = if state.picked == Some(held.0[slot.0]) { PICKED } else { READY };
        border.set_if_neq(BorderColor::all(edge));
    }
    for (entry, children, mut border) in &mut entries {
        let skill = row(entry.row)[entry.column];
        let shown = entry.row == state.skill_row;
        let edge = if state.picked == Some(skill) { PICKED } else if held.holds(skill) { READY } else { OFF_BAR };
        border.set_if_neq(BorderColor::all(edge));
        for &child in children {
            if let Ok(mut icon) = icons.get_mut(child) {
                let tint = if shown { Color::WHITE } else { DIM };
                if icon.color != tint {
                    icon.color = tint;
                }
            }
            if let Ok(mut digit) = digits.get_mut(child) {
                digit.set_if_neq(if shown { Visibility::Inherited } else { Visibility::Hidden });
            }
        }
    }
    for (entry, mut color) in &mut names {
        let skill = row(entry.row)[entry.column];
        let tint = if state.picked == Some(skill) { PICKED } else if entry.row == state.skill_row { PLAIN } else { DIM };
        color.set_if_neq(TextColor(tint));
    }
}

/// Shows the card of the skill picked, written for this character, each
/// number its line scales coloured, and under it why the skill stays off
/// the bar while the player fights; with none picked, no card.
#[allow(clippy::too_many_arguments)]
pub fn update_card(
    mut commands: Commands,
    state: Res<CharacterPanelState>,
    tuning: Res<Tuning>,
    asset_server: Res<AssetServer>,
    player: Query<(Option<&ActorAttributes>, Option<&CombatState>), With<Viewed>>,
    mut card: Query<&mut Visibility, With<SkillCard>>,
    mut texts: Query<(&Said, &mut Text)>,
    mut art: Query<(&mut CardArt, &mut ImageNode)>,
    mut rules: Query<(Entity, &mut CardRules)>,
) {
    if !state.visible || state.tab != PanelTab::Skills {
        return;
    }
    let Ok((attrs, combat)) = player.single() else { return };
    if let Ok(mut visibility) = card.single_mut() {
        visibility.set_if_neq(if state.picked.is_some() { Visibility::Inherited } else { Visibility::Hidden });
    }
    let fighting = combat.is_some_and(|combat| combat.in_combat);
    for (said, mut text) in &mut texts {
        let now = match (said, state.picked) {
            (Said::Refusal, Some(_)) if fighting => "In combat: the bar changes once out of it".to_string(),
            (Said::Refusal, _) | (_, None) => String::new(),
            (Said::Name, Some(skill)) => name(skill),
            (Said::Endurance, Some(skill)) => format!("{:.0}", tuning.cost(skill)),
            (Said::Recovery, Some(skill)) => seconds(tuning.recovery(skill)),
            (Said::Kind, Some(skill)) => kind_line(skill),
        };
        if text.0 != now {
            text.0 = now;
        }
    }
    if let Ok((ent, mut shown)) = rules.single_mut() {
        let pieces = state.picked.map(|skill| does(skill, &attrs.copied().unwrap_or_default(), &tuning).sentence()).unwrap_or_default();
        if shown.0 != pieces {
            commands.entity(ent).despawn_related::<Children>().with_children(|text| {
                for (piece, scaled) in &pieces {
                    text.spawn((
                        TextSpan::new(piece.clone()),
                        TextFont { font_size: FontSize::Px(RULES_FONT), ..default() },
                        TextColor(if *scaled { SCALED } else { PLAIN }),
                    ));
                }
            });
            shown.0 = pieces;
        }
    }
    if let (Ok((mut shown, mut image)), Some(skill)) = (art.single_mut(), state.picked) {
        if shown.0 != Some(skill) {
            shown.0 = Some(skill);
            image.image = asset_server.load(icon_path(skill));
        }
    }
}

/// A card's kind line: its kind, and the attribute line it is of
fn kind_line(skill: AbilityType) -> String {
    let kind = match skill.kind() {
        SkillKind::Strike => "Strike",
        SkillKind::Reaction => "Reaction",
        SkillKind::Stance => "Stance",
    };
    match ActorAttributes::line(skill) {
        Some(line) => format!("{kind} - {line:?}"),
        None => kind.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rows_hold_every_skill_a_bar_may_once_and_each_fits_the_digits() {
        let rows: Vec<AbilityType> = (0..ROWS.len()).flat_map(row).collect();
        assert_eq!(rows.len(), Loadout::pool().count());
        assert!(Loadout::pool().all(|skill| rows.contains(&skill)));
        assert!((0..ROWS.len()).all(|at| row(at).len() <= ENTRY_KEYS.len()));
    }
}
