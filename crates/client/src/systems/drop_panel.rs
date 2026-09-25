//! The drop panel: how many of a bag stack to drop, typed on the numpad's
//! digits or scrolled with + and -, in the bag tab's detail of the stack.
//! It starts at the whole stack and never goes past what the bag holds.

use std::time::Duration;

use bevy::prelude::*;

use common_bevy::{
    components::{equipment::Inventory, Actor},
    message::{Event, Try},
};

use crate::{
    plugins::console::DevConsole,
    systems::{
        character_panel::{CharacterPanelState, PanelTab},
        focus::{NumpadFocus, Panel},
        gathering::ENTRY_KEYS,
    },
};

pub const KEYCODE_DROP: KeyCode = KeyCode::NumpadEnter;
pub const KEYCODE_CANCEL: KeyCode = KeyCode::NumpadDecimal;
pub const KEYCODE_MORE: KeyCode = KeyCode::NumpadAdd;
pub const KEYCODE_LESS: KeyCode = KeyCode::NumpadSubtract;
pub const KEYCODE_ERASE: KeyCode = KeyCode::Backspace;

/// The digit keys in the order of their values, 0 to 9.
const DIGIT_KEYS: [KeyCode; 10] = [
    KeyCode::Numpad0, ENTRY_KEYS[0], ENTRY_KEYS[1], ENTRY_KEYS[2], ENTRY_KEYS[3],
    ENTRY_KEYS[4], ENTRY_KEYS[5], ENTRY_KEYS[6], ENTRY_KEYS[7], ENTRY_KEYS[8],
];

/// How long a held + or - waits before it repeats.
const SCROLL_DELAY_MS: u64 = 400;
/// How often a held + or - repeats.
const SCROLL_REPEAT_MS: u64 = 80;
/// How long a held + or - takes to double its step.
const SCROLL_DOUBLING_MS: u64 = 500;

/// How far a + or - held for `held_ms` has moved the count: one at the
/// press, then from [`SCROLL_DELAY_MS`] a step every [`SCROLL_REPEAT_MS`],
/// the step doubling every [`SCROLL_DOUBLING_MS`]. Measured from the press,
/// so it moves as far at any frame rate.
pub fn scrolled(held_ms: u64) -> u32 {
    if held_ms < SCROLL_DELAY_MS {
        return 1;
    }
    let repeats = (held_ms - SCROLL_DELAY_MS) / SCROLL_REPEAT_MS + 1;
    let mut moved: u32 = 1;
    for n in 0..repeats {
        let doublings = (n * SCROLL_REPEAT_MS / SCROLL_DOUBLING_MS).min(31) as u32;
        moved = moved.saturating_add(1 << doublings);
        if moved == u32::MAX {
            break;
        }
    }
    moved
}

/// A + or - held: which, since when, and the count it moves from.
#[derive(Clone, Copy, Debug)]
struct Scroll {
    more: bool,
    since: Duration,
    from: u32,
}

/// The stack being dropped and how many of it: a text box the digits type
/// into, which + and - move from whatever it shows.
#[derive(Clone, Copy, Debug)]
pub struct Choosing {
    pub kind: common::Stackable,
    pub count: u32,
    /// Whether the count is being typed: a digit goes on its end. Opening
    /// the panel or a + or - ends the typing, so the next digit starts a
    /// new number.
    typing: bool,
    scroll: Option<Scroll>,
}

impl Choosing {
    /// Types `digit` into the box, no further than `most`.
    fn type_digit(&mut self, digit: u32, most: u32) {
        let before = if self.typing { self.count } else { 0 };
        self.count = before.saturating_mul(10).saturating_add(digit).min(most);
        self.typing = true;
        self.scroll = None;
    }

    /// Takes the box's last digit off.
    fn erase(&mut self) {
        self.count = if self.typing { self.count / 10 } else { 0 };
        self.typing = true;
    }

    /// Starts moving the count up where `more`, down otherwise, from what
    /// the box shows, as + or - goes down at `now`.
    fn press_scroll(&mut self, more: bool, now: Duration) {
        self.scroll = Some(Scroll { more, since: now, from: self.count });
        self.typing = false;
    }

    /// Moves the count as far as the + or - held since its press has taken
    /// it by `now`, between one and `most`.
    fn hold_scroll(&mut self, now: Duration, most: u32) {
        let Some(scroll) = self.scroll else { return };
        let moved = scrolled((now - scroll.since).as_millis() as u64);
        let count = if scroll.more { scroll.from.saturating_add(moved) } else { scroll.from.saturating_sub(moved) };
        self.count = count.clamp(1, most.max(1));
    }
}

/// The drop panel, open on a stack while Some.
#[derive(Resource, Default)]
pub struct DropChoice(pub Option<Choosing>);

impl DropChoice {
    /// Opens the panel on `stack`, choosing all of it.
    pub fn open(&mut self, stack: common::Stack) {
        self.0 = Some(Choosing { kind: stack.kind, count: stack.count, typing: false, scroll: None });
    }
}

/// Works the panel while it has the numpad: digits type the count, + and
/// - scroll it, Enter drops that many and `.` closes it. It closes with
/// the bag tab, and when the bag holds none of its stack.
pub fn handle_keys(
    mut keyboard: ResMut<ButtonInput<KeyCode>>,
    console: Res<DevConsole>,
    focus: Res<NumpadFocus>,
    state: Res<CharacterPanelState>,
    time: Res<Time<Real>>,
    mut choice: ResMut<DropChoice>,
    player: Query<(Entity, &Inventory), With<Actor>>,
    mut writer: MessageWriter<Try>,
) {
    let Some(mut chosen) = choice.0 else { return };
    let Ok((ent, bag)) = player.single() else { return };
    let most = bag.count(chosen.kind);
    if !state.visible || state.tab != PanelTab::Bag || most == 0 {
        choice.0 = None;
        return;
    }
    chosen.count = chosen.count.min(most);
    if focus.has(Panel::Drop) && !console.visible {
        if keyboard.clear_just_pressed(KEYCODE_CANCEL) {
            choice.0 = None;
            return;
        }
        if keyboard.clear_just_pressed(KEYCODE_DROP) {
            if chosen.count > 0 {
                writer.write(Try { event: Event::Drop { ent, kind: chosen.kind, count: chosen.count } });
            }
            choice.0 = None;
            return;
        }
        for (digit, key) in DIGIT_KEYS.iter().enumerate() {
            if keyboard.clear_just_pressed(*key) {
                chosen.type_digit(digit as u32, most);
            }
        }
        if keyboard.clear_just_pressed(KEYCODE_ERASE) {
            chosen.erase();
        }
        for (key, more) in [(KEYCODE_MORE, true), (KEYCODE_LESS, false)] {
            if keyboard.clear_just_pressed(key) {
                chosen.press_scroll(more, time.elapsed());
            }
        }
    }
    chosen.scroll = chosen.scroll.filter(|s| keyboard.pressed(if s.more { KEYCODE_MORE } else { KEYCODE_LESS }));
    chosen.hold_scroll(time.elapsed(), most);
    choice.0 = Some(chosen);
}

/// The count chosen, in the panel's text box.
#[derive(Component)]
pub struct DropCount;

/// Spawns the panel under `parent`, the bag tab's detail of the stack it is
/// open on, choosing `count`: the text box between the keys that move it,
/// and the keys that drop and close.
pub fn spawn_entry(parent: &mut ChildSpawnerCommands, count: u32) {
    use crate::systems::keycap::{hint_row, keycap, Hint};
    parent
        .spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: Val::Px(6.), ..default() })
        .with_children(|line| {
            keycap(line, "-");
            line.spawn((
                Node {
                    min_width: Val::Px(80.),
                    padding: UiRect::axes(Val::Px(8.), Val::Px(3.)),
                    justify_content: JustifyContent::FlexEnd,
                    border: UiRect::all(Val::Px(1.)),
                    border_radius: BorderRadius::all(Val::Px(3.)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.08, 0.08, 0.08)),
                BorderColor::all(Color::srgb(0.6, 0.6, 0.6)),
            ))
            .with_children(|field| {
                field.spawn((DropCount, Text::new(count.to_string()), TextFont { font_size: FontSize::Px(20.0), ..default() }, TextColor(Color::WHITE)));
            });
            keycap(line, "+");
        });
    hint_row(parent, None, &[Hint::key("Ent", "drop"), Hint::key(".", "cancel")]);
}

/// Writes the count chosen into the panel's text box.
pub fn update(choice: Res<DropChoice>, mut count: Query<&mut Text, With<DropCount>>) {
    let (Some(chosen), Ok(mut text)) = (choice.0, count.single_mut()) else { return };
    let now = chosen.count.to_string();
    if text.0 != now {
        text.0 = now;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tap moves the count one; held, it moves further the longer it is
    /// held, and each stretch of holding moves it further than the last.
    #[test]
    fn a_held_scroll_speeds_up() {
        assert_eq!(scrolled(0), 1);
        assert_eq!(scrolled(SCROLL_DELAY_MS - 1), 1, "no repeat before the delay");
        let stretch = SCROLL_DOUBLING_MS;
        let at = |k: u64| scrolled(SCROLL_DELAY_MS + k * stretch);
        for k in 1..8 {
            assert!(at(k + 1) - at(k) > at(k) - at(k - 1), "stretch {k} moves further than the one before");
        }
        assert_eq!(scrolled(u64::MAX / 2), u32::MAX, "held forever, it saturates");
    }

    /// Typing into the box opened on a stack replaces what it showed; + and
    /// - move from the number typed; a digit after them starts a new
    /// number. Nothing goes past what the bag holds.
    #[test]
    fn plus_and_minus_move_the_typed_number_and_typing_after_starts_anew() {
        let most = 30;
        let mut drop = DropChoice::default();
        drop.open(common::Stack { kind: common::Stackable::Material(common::Material::Softwood), count: most });
        let mut chosen = drop.0.unwrap();
        let at = Duration::from_secs(5);

        chosen.type_digit(1, most);
        chosen.type_digit(2, most);
        assert_eq!(chosen.count, 12, "the first digit replaces the whole stack");
        chosen.press_scroll(true, at);
        chosen.hold_scroll(at, most);
        assert_eq!(chosen.count, 13, "+ moves from the number typed");
        chosen.scroll = None;
        chosen.type_digit(4, most);
        assert_eq!(chosen.count, 4, "a digit after + starts a new number");
        chosen.type_digit(7, most);
        assert_eq!(chosen.count, most, "typed past the stack, it is the stack");
        chosen.erase();
        assert_eq!(chosen.count, 3);
        chosen.press_scroll(false, at);
        chosen.hold_scroll(at + Duration::from_secs(10), most);
        assert_eq!(chosen.count, 1, "held -, it stops at one");
    }
}
