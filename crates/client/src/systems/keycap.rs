//! How the UI names a key wherever it names one: a cap with one key on it,
//! the same width and height in every panel, hint and slot, labelled by
//! [`label`] and carrying the keys it stands for ([`Names`]), so help can
//! show what a key does beside it.

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

/// The keys a cap stands for: its own, or every key of the run it ends.
#[derive(Component)]
pub struct Names(pub Vec<KeyCode>);

/// A key's name on a cap.
pub fn label(key: KeyCode) -> String {
    let named = match key {
        KeyCode::Numpad0 => "0",
        KeyCode::Numpad1 => "1",
        KeyCode::Numpad2 => "2",
        KeyCode::Numpad3 => "3",
        KeyCode::Numpad4 => "4",
        KeyCode::Numpad5 => "5",
        KeyCode::Numpad6 => "6",
        KeyCode::Numpad7 => "7",
        KeyCode::Numpad8 => "8",
        KeyCode::Numpad9 => "9",
        KeyCode::NumpadEnter | KeyCode::Enter => "Ent",
        KeyCode::NumpadDecimal => ".",
        KeyCode::NumpadSubtract => "-",
        KeyCode::NumpadAdd => "+",
        KeyCode::ArrowUp => "Up",
        KeyCode::ArrowDown => "Dn",
        KeyCode::ArrowLeft => "Lt",
        KeyCode::ArrowRight => "Rt",
        KeyCode::Escape => "Esc",
        KeyCode::Backspace => "Bksp",
        _ => return format!("{key:?}").trim_start_matches("Key").to_string(),
    };
    named.to_string()
}

/// Spawns a cap standing for `names`, labelled `key`, under `parent`,
/// placed by `place`: its position and offsets are kept, the cap's size,
/// border and fill set.
fn cap_at<'a>(parent: &'a mut ChildSpawnerCommands, key: KeyCode, names: Vec<KeyCode>, place: Node) -> EntityCommands<'a> {
    let name = label(key);
    debug_assert!(name.chars().count() <= KEY_CHARS, "a key's name is at most {KEY_CHARS} characters: {name}");
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
        Names(names),
    ));
    cap.with_children(|c| {
        c.spawn((Text::new(name), TextFont { font_size: FontSize::Px(KEY_FONT), ..default() }, TextColor(KEY_TEXT)));
    });
    cap
}

/// Spawns a cap for `key` under `parent`, placed by `place`. Returns the
/// cap, for a marker or a visibility.
pub fn keycap_at<'a>(parent: &'a mut ChildSpawnerCommands, key: KeyCode, place: Node) -> EntityCommands<'a> {
    cap_at(parent, key, vec![key], place)
}

/// A cap for `key` in its parent's flow.
pub fn keycap<'a>(parent: &'a mut ChildSpawnerCommands, key: KeyCode) -> EntityCommands<'a> {
    keycap_at(parent, key, Node::default())
}

/// A cap for `key` in a slot's top-left corner, as every slot names the
/// key that works it.
pub fn corner_keycap<'a>(parent: &'a mut ChildSpawnerCommands, key: KeyCode) -> EntityCommands<'a> {
    keycap_at(parent, key, Node { position_type: PositionType::Absolute, top: Val::Px(2.), left: Val::Px(2.), ..default() })
}

/// What a hint says: its keys, with what stands between their caps — a
/// dash for a run of keys, capped at its ends, a slash for either — and
/// what they do.
pub struct Hint<'a> {
    keys: Vec<KeyCode>,
    run: bool,
    does: &'a str,
}

impl<'a> Hint<'a> {
    pub fn key(key: KeyCode, does: &'a str) -> Self {
        Hint { keys: vec![key], run: false, does }
    }

    /// The keys of `run`, in order.
    pub fn range(run: &[KeyCode], does: &'a str) -> Self {
        Hint { keys: run.to_vec(), run: true, does }
    }

    /// Any one of `keys`.
    pub fn either(keys: &[KeyCode], does: &'a str) -> Self {
        Hint { keys: keys.to_vec(), run: false, does }
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
    if hint.run {
        let (Some(&first), Some(&last)) = (hint.keys.first(), hint.keys.last()) else { return };
        cap_at(row, first, hint.keys.to_vec(), Node::default());
        hint_text(row, "-", UiRect::default());
        cap_at(row, last, hint.keys.to_vec(), Node::default());
    } else {
        for (i, &key) in hint.keys.iter().enumerate() {
            if i > 0 {
                hint_text(row, "/", UiRect::default());
            }
            keycap(row, key);
        }
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
