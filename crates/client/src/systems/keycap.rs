//! How the UI names a key wherever it names one: a cap with one key on it,
//! the same width and height in every panel, hint and slot.

use bevy::prelude::*;

/// A cap's size. A key's name is at most [`KEY_CHARS`] characters, which
/// the width is set to hold.
pub const KEY_WIDTH: f32 = 34.0;
pub const KEY_HEIGHT: f32 = 18.0;
pub const KEY_CHARS: usize = 4;
const KEY_FONT: f32 = 12.0;
const KEY_TEXT: Color = Color::srgb(0.92, 0.92, 0.92);
const KEY_FILL: Color = Color::srgba(0.12, 0.12, 0.12, 0.95);
const KEY_EDGE: Color = Color::srgb(0.5, 0.5, 0.5);
/// What a hint writes between caps and after them.
const HINT_TEXT: Color = Color::srgb(0.65, 0.65, 0.65);

/// Spawns a cap for `key` under `parent`, placed by `place`: its position
/// and offsets are kept, the cap's size, border and fill set. Returns the
/// cap, for a marker or a visibility.
pub fn keycap_at<'a>(parent: &'a mut ChildSpawnerCommands, key: &str, place: Node) -> EntityCommands<'a> {
    debug_assert!(key.chars().count() <= KEY_CHARS, "a key's name is at most {KEY_CHARS} characters: {key}");
    let mut cap = parent.spawn((
        Node {
            width: Val::Px(KEY_WIDTH),
            height: Val::Px(KEY_HEIGHT),
            flex_shrink: 0.0,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(Val::Px(1.)),
            border_radius: BorderRadius::all(Val::Px(3.)),
            ..place
        },
        BackgroundColor(KEY_FILL),
        BorderColor::all(KEY_EDGE),
    ));
    cap.with_children(|c| {
        c.spawn((Text::new(key), TextFont { font_size: FontSize::Px(KEY_FONT), ..default() }, TextColor(KEY_TEXT)));
    });
    cap
}

/// A cap for `key` in its parent's flow.
pub fn keycap<'a>(parent: &'a mut ChildSpawnerCommands, key: &str) -> EntityCommands<'a> {
    keycap_at(parent, key, Node::default())
}

/// A cap for `key` in a slot's top-left corner, as every slot names the
/// key that works it.
pub fn corner_keycap<'a>(parent: &'a mut ChildSpawnerCommands, key: &str) -> EntityCommands<'a> {
    keycap_at(parent, key, Node { position_type: PositionType::Absolute, top: Val::Px(2.), left: Val::Px(2.), ..default() })
}

/// What a hint says: its keys, one cap each, with what stands between them
/// — a dash for a run of keys, a slash for either — and what they do.
pub struct Hint<'a> {
    keys: Vec<&'a str>,
    join: &'a str,
    does: &'a str,
}

impl<'a> Hint<'a> {
    pub fn key(key: &'a str, does: &'a str) -> Self {
        Hint { keys: vec![key], join: "", does }
    }

    /// The keys from `first` to `last`.
    pub fn range(first: &'a str, last: &'a str, does: &'a str) -> Self {
        Hint { keys: vec![first, last], join: "-", does }
    }

    /// Any one of `keys`.
    pub fn either(keys: &[&'a str], does: &'a str) -> Self {
        Hint { keys: keys.to_vec(), join: "/", does }
    }
}

fn hint_text(parent: &mut ChildSpawnerCommands, text: &str, margin: UiRect) {
    parent.spawn((
        Text::new(text),
        TextFont { font_size: FontSize::Px(KEY_FONT), ..default() },
        TextColor(HINT_TEXT),
        Node { margin, ..default() },
    ));
}

/// One hint laid along `row`: its caps, what joins them, and what it does.
fn hint(row: &mut ChildSpawnerCommands, hint: &Hint) {
    for (i, key) in hint.keys.iter().enumerate() {
        if i > 0 {
            hint_text(row, hint.join, UiRect::default());
        }
        keycap(row, key);
    }
    hint_text(row, hint.does, UiRect::right(Val::Px(12.)));
}

/// A line of hints under `parent`, after `title` where there is one.
pub fn hint_row(parent: &mut ChildSpawnerCommands, title: Option<&str>, hints: &[Hint]) {
    parent
        .spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: Val::Px(5.), ..default() })
        .with_children(|row| {
            if let Some(title) = title {
                hint_text(row, title, UiRect::right(Val::Px(12.)));
            }
            for h in hints {
                hint(row, h);
            }
        });
}

/// Hints stacked under `parent`, one to a line, set to its right edge.
pub fn hint_column(parent: &mut ChildSpawnerCommands, hints: &[Hint], place: Node) {
    parent
        .spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::FlexEnd, row_gap: Val::Px(4.), ..place })
        .with_children(|column| {
            for h in hints {
                column
                    .spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: Val::Px(5.), ..default() })
                    .with_children(|row| hint(row, h));
            }
        });
}
