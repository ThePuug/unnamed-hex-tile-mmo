//! The choices the shell offers, and the one system that routes the keys
//! among them. The connecting and character screens list theirs at all
//! times; in the world, Esc opens the menu over it. Esc closes whatever is
//! on top: the settings panel or the credits over the menu, the menu over
//! the character panel, the character panel over the world.
//!
//! Every screen is worked from the keyboard alone: Up and Down pick,
//! Enter chooses.

use bevy::prelude::*;

use common_bevy::message::Try;

use super::{credits::CreditsPanel, Stage};
use crate::{
    network::Link,
    plugins::{
        console::DevConsole,
        settings::{self, AudioSettings, SettingsPanel, VideoSettings},
    },
    systems::character_panel::{self, CharacterPanel, CharacterPanelState},
};

/// A choice a screen lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Choice {
    RetryNow,
    Play,
    Resume,
    Settings,
    Credits,
    CharacterSelect,
    Quit,
}

impl Choice {
    fn label(self) -> &'static str {
        match self {
            Choice::RetryNow => "Retry now",
            Choice::Play => "Play",
            Choice::Resume => "Resume",
            Choice::Settings => "Settings",
            Choice::Credits => "Credits",
            Choice::CharacterSelect => "Disconnect",
            Choice::Quit => "Quit",
        }
    }

    /// What a stage offers, in order.
    fn offered(stage: Stage) -> &'static [Choice] {
        match stage {
            Stage::Connecting => &[Choice::RetryNow, Choice::Settings, Choice::Quit],
            Stage::CharacterSelect => &[Choice::Play, Choice::Settings, Choice::Credits, Choice::Quit],
            Stage::Loading => &[],
            Stage::Playing => &[Choice::Resume, Choice::Settings, Choice::CharacterSelect],
        }
    }
}

/// Whether the in-game menu shows, which choice is picked, and whether it
/// is asking to confirm leaving the world, with Confirm or Cancel picked.
#[derive(Resource, Default)]
pub struct GameMenu {
    pub open: bool,
    row: usize,
    confirming: bool,
    cancel_picked: bool,
}

impl GameMenu {
    pub fn close(&mut self) {
        *self = GameMenu::default();
    }
}

/// Routes this frame's keys to whichever of the menu, the settings panel
/// and the credits is on top. The console's own keys come first: while it shows, it
/// has them.
#[allow(clippy::too_many_arguments)]
pub fn route_keys(
    mut keys: crate::systems::help::Keys,
    console: Res<DevConsole>,
    stage: Res<State<Stage>>,
    mut next: ResMut<NextState<Stage>>,
    mut link: ResMut<Link>,
    mut menu: ResMut<GameMenu>,
    mut panel: ResMut<SettingsPanel>,
    mut credits: ResMut<CreditsPanel>,
    mut video: ResMut<VideoSettings>,
    mut audio: ResMut<AudioSettings>,
    mut character: ResMut<CharacterPanelState>,
    mut character_view: Query<&mut Visibility, With<CharacterPanel>>,
    mut writer: MessageWriter<Try>,
    mut exit: MessageWriter<AppExit>,
    mut entered: ResMut<super::Entered>,
) {
    if stage.is_changed() {
        menu.close();
    }
    if console.visible {
        return;
    }
    if panel.open {
        settings::navigate(&mut panel, &mut video, &mut audio, &mut keys);
        return;
    }
    if credits.open {
        const BACK: &str = "Close the credits";
        if keys.pressed(KeyCode::Escape, BACK) | entered_choice(&mut keys, BACK) {
            credits.open = false;
        }
        return;
    }
    let stage = *stage.get();
    if stage == Stage::Playing && !menu.open {
        let does = if character.visible { "Close the character panel" } else { "Open the menu" };
        if keys.pressed(KeyCode::Escape, does) {
            if character.visible {
                if let Ok(mut visibility) = character_view.single_mut() {
                    character_panel::close(&mut character, &mut visibility);
                }
            } else {
                menu.open = true;
            }
        }
        return;
    }
    if menu.confirming {
        const PICK: &str = "Choose between leaving and staying";
        if keys.pressed(KeyCode::Escape, "Stay in the world") {
            menu.confirming = false;
        } else if keys.pressed(KeyCode::ArrowUp, PICK) || keys.pressed(KeyCode::ArrowDown, PICK) {
            menu.cancel_picked = !menu.cancel_picked;
        } else if entered_choice(&mut keys, "Do what is chosen") {
            if menu.cancel_picked {
                menu.confirming = false;
            } else {
                super::leave(&mut writer, &mut next, &mut entered);
            }
        }
        return;
    }
    let offered = Choice::offered(stage);
    if offered.is_empty() {
        return;
    }
    let rows = offered.len();
    if stage == Stage::Playing && keys.pressed(KeyCode::Escape, "Close the menu") {
        menu.close();
    } else if keys.pressed(KeyCode::ArrowUp, "Choose the row above") {
        menu.row = (menu.row + rows - 1) % rows;
    } else if keys.pressed(KeyCode::ArrowDown, "Choose the row below") {
        menu.row = (menu.row + 1) % rows;
    } else if entered_choice(&mut keys, "Do what the row says") {
        match offered[menu.row.min(rows - 1)] {
            Choice::RetryNow => link.retry_now(),
            Choice::Play => super::play(&mut writer, &mut next, &mut entered),
            Choice::Resume => menu.close(),
            Choice::Settings => panel.open(),
            Choice::Credits => credits.open = true,
            Choice::CharacterSelect => {
                menu.confirming = true;
                menu.cancel_picked = true;
            }
            Choice::Quit => {
                exit.write(AppExit::Success);
            }
        }
    }
}

/// Whether Enter, either of them, went down to do what `does` says.
fn entered_choice(keys: &mut crate::systems::help::Keys, does: &'static str) -> bool {
    keys.pressed(KeyCode::Enter, does) | keys.pressed(KeyCode::NumpadEnter, does)
}

#[derive(Component)]
pub struct MenuRoot;

#[derive(Component)]
pub struct MenuTitle;

#[derive(Component)]
pub struct MenuRows;

const WIDTH: f32 = 420.0;
const TITLE: Color = Color::srgb(0.85, 0.75, 0.55);
const PICKED: Color = Color::srgb(1.0, 0.9, 0.6);
const PLAIN: Color = Color::srgb(0.75, 0.75, 0.75);

pub fn setup(mut commands: Commands) {
    commands
        .spawn((
            MenuRoot,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(WIDTH),
                padding: UiRect::all(Val::Px(20.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(10.0),
                border_radius: BorderRadius::all(Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.08, 0.08, 0.08, 0.95)),
            Visibility::Hidden,
            GlobalZIndex(1900),
        ))
        .with_children(|parent| {
            parent.spawn((
                MenuTitle,
                Text::new(""),
                TextFont { font_size: FontSize::Px(22.0), ..default() },
                TextColor(TITLE),
            ));
            parent.spawn((MenuRows, Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(6.0), ..default() }));
        });
}

/// Shows the choices where the stage wants them — beside the figure on the
/// character screen, under the status while connecting, in the middle of
/// the world — unless the settings panel or the credits are over them, and
/// redraws them when anything they show changes.
pub fn draw(
    mut commands: Commands,
    stage: Res<State<Stage>>,
    menu: Res<GameMenu>,
    panel: Res<SettingsPanel>,
    credits: Res<CreditsPanel>,
    mut root: Query<(&mut Visibility, &mut Node), With<MenuRoot>>,
    mut title: Query<&mut Text, With<MenuTitle>>,
    rows: Query<Entity, With<MenuRows>>,
) {
    if !stage.is_changed() && !menu.is_changed() && !panel.is_changed() && !credits.is_changed() {
        return;
    }
    let stage = *stage.get();
    let in_world = stage == Stage::Playing;
    if let Ok((mut visibility, mut node)) = root.single_mut() {
        let shown = !panel.open && !credits.open && !Choice::offered(stage).is_empty() && (menu.open || !in_world);
        visibility.set_if_neq(if shown { Visibility::Visible } else { Visibility::Hidden });
        let (left, top, margin_left) = match stage {
            Stage::CharacterSelect => (64.0, 45.0, 0.0),
            Stage::Playing => (50.0, 40.0, -WIDTH / 2.0),
            _ => (50.0, 60.0, -WIDTH / 2.0),
        };
        node.left = Val::Percent(left);
        node.top = Val::Percent(top);
        node.margin = UiRect { left: Val::Px(margin_left), ..default() };
    }
    if let Ok(mut title) = title.single_mut() {
        title.0 = if in_world { "Menu".into() } else { String::new() };
    }
    let Ok(rows) = rows.single() else { return };
    commands.entity(rows).despawn_related::<Children>().with_children(|parent| {
        if menu.confirming {
            parent.spawn((
                Text::new("Are you sure you want to return to character select?"),
                TextFont { font_size: FontSize::Px(17.0), ..default() },
                TextColor(PLAIN),
            ));
            for (label, picked) in [("Confirm", !menu.cancel_picked), ("Cancel", menu.cancel_picked)] {
                parent.spawn((
                    Text::new(format!("{} {label}", if picked { ">" } else { " " })),
                    TextFont { font_size: FontSize::Px(18.0), ..default() },
                    TextColor(if picked { PICKED } else { PLAIN }),
                ));
            }
            return;
        }
        for (i, choice) in Choice::offered(stage).iter().enumerate() {
            let picked = i == menu.row;
            parent.spawn((
                Text::new(format!("{} {}", if picked { ">" } else { " " }, choice.label())),
                TextFont { font_size: FontSize::Px(18.0), ..default() },
                TextColor(if picked { PICKED } else { PLAIN }),
            ));
        }
    });
}
