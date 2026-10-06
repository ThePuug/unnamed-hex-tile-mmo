//! The credits, chosen from the character screen: what the game was made
//! with, whose recordings its music sounds through (`music::credits`) and
//! whose type it is set in, each with its licence, as the licences ask
//! they be named. Enter or Esc closes them; `menu::route_keys` routes the
//! keys while they show.

use bevy::prelude::*;

/// Whether the credits show.
#[derive(Resource, Default)]
pub struct CreditsPanel {
    pub open: bool,
}

#[derive(Component)]
pub struct CreditsRoot;

/// The type the game is set in, its makers, under the SIL Open Font
/// License: Bevy's own face for text set in no other, and the one the
/// glyphs and the round trip are set in.
const TYPE: &[(&str, &str)] = &[
    ("Fira Mono", "Carrois Apostrophe for Mozilla, Bevy's default face"),
    ("Iosevka Nerd Font", "Iosevka by Belleve Invis, patched by Nerd Fonts"),
];

const WIDTH: f32 = 640.0;
const TITLE: Color = Color::srgb(0.85, 0.75, 0.55);
const PLAIN: Color = Color::srgb(0.85, 0.85, 0.85);
const DETAIL: Color = Color::srgb(0.6, 0.6, 0.6);

pub fn setup(mut commands: Commands) {
    commands
        .spawn((
            CreditsRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(50.0),
                top: Val::Percent(50.0),
                width: Val::Px(WIDTH),
                margin: UiRect { left: Val::Px(-WIDTH / 2.0), top: Val::Px(-280.0), ..default() },
                padding: UiRect::all(Val::Px(20.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                border_radius: BorderRadius::all(Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.08, 0.08, 0.08, 0.95)),
            Visibility::Hidden,
            GlobalZIndex(2000),
        ))
        .with_children(|parent| {
            parent.spawn((Text::new("Credits"), TextFont { font_size: FontSize::Px(22.0), ..default() }, TextColor(TITLE)));
            let heading = |parent: &mut ChildSpawnerCommands, text: &str| {
                parent.spawn((
                    Text::new(text),
                    TextFont { font_size: FontSize::Px(15.0), ..default() },
                    TextColor(TITLE),
                    Node { margin: UiRect::top(Val::Px(6.0)), ..default() },
                ));
            };
            let entry = |parent: &mut ChildSpawnerCommands, what: &str, work: String| {
                parent.spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(2.0), ..default() }).with_children(|entry| {
                    entry.spawn((Text::new(what), TextFont { font_size: FontSize::Px(16.0), ..default() }, TextColor(PLAIN)));
                    entry.spawn((Text::new(work), TextFont { font_size: FontSize::Px(13.0), ..default() }, TextColor(DETAIL)));
                });
            };
            heading(parent, "Made with");
            for (tool, by, link) in music::credits::MADE_WITH {
                entry(parent, tool, format!("{by} ({})", link.trim_start_matches("https://")));
            }
            heading(parent, "Music");
            for c in music::credits::SOUNDS {
                entry(parent, c.what, format!("{} · {} ({})", c.work, c.licence, c.licence_link));
            }
            heading(parent, "Type");
            for (font, by) in TYPE {
                entry(parent, font, format!("{by} · SIL Open Font License 1.1"));
            }
            use crate::systems::keycap::{hint_row, Hint};
            hint_row(parent, None, &[Hint::either(&[KeyCode::Enter, KeyCode::Escape], "back")]);
        });
}

/// Shows the credits while they are open.
pub fn draw(panel: Res<CreditsPanel>, mut root: Query<&mut Visibility, With<CreditsRoot>>) {
    if !panel.is_changed() {
        return;
    }
    if let Ok(mut visibility) = root.single_mut() {
        visibility.set_if_neq(if panel.open { Visibility::Visible } else { Visibility::Hidden });
    }
}
