//! The attributes tab's respec: a draft of the three pairs, changed from the
//! numpad one pair at a time and sent to the server when every level is in.

use bevy::prelude::*;

use crate::{
    plugins::console::DevConsole,
    systems::{
        character_panel::*,
        focus::{NumpadFocus, Panel},
    },
};
use common_bevy::{
    components::{Actor, ActorAttributes, Pair},
    message::{Do, Event as GameEvent, Try},
};

/// One step of a pair, made by one key.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Change {
    Axis(i8),
    Spectrum(i8),
    Shift(i8),
}

/// The keys that change the pair under the cursor, set out as a pair is
/// drawn: spectrum over its bar, axis at its ends, shift along it.
const KEYS: [(KeyCode, Change); 6] = [
    (KeyCode::Numpad7, Change::Spectrum(-1)),
    (KeyCode::Numpad9, Change::Spectrum(1)),
    (KeyCode::Numpad4, Change::Axis(-1)),
    (KeyCode::Numpad6, Change::Axis(1)),
    (KeyCode::Numpad1, Change::Shift(-1)),
    (KeyCode::Numpad3, Change::Shift(1)),
];

/// The keys that act on the panel as a whole.
const KEYCODE_NEXT_PAIR: KeyCode = KeyCode::NumpadDecimal;
const KEYCODE_APPLY: KeyCode = KeyCode::NumpadEnter;

/// `draft` with `change` made to its pair `at`: the shift is kept to what the
/// pair allows, and a change that puts in more levels than `level` is
/// refused, leaving the draft as it was.
pub fn step(mut draft: [Pair; 3], at: usize, change: Change, level: u32) -> [Pair; 3] {
    let before = draft;
    let pair = &mut draft[at];
    let mut shift = pair.shift;
    match change {
        Change::Axis(by) => pair.axis = pair.axis.saturating_add(by),
        Change::Spectrum(by) => pair.spectrum = pair.spectrum.saturating_add(by).max(0),
        Change::Shift(by) => shift = shift.saturating_add(by),
    }
    pair.set_shift(shift);
    if ActorAttributes::fits(&draft, level) { draft } else { before }
}

/// While the attributes tab has the numpad: `.` moves the cursor to the next
/// pair, wrapping to the first; the keys of [`KEYS`] change the pair under it
/// in the draft; Enter sends the draft once it has put in every level.
pub fn handle_numpad(
    mut keyboard: ResMut<ButtonInput<KeyCode>>,
    console: Res<DevConsole>,
    focus: Res<NumpadFocus>,
    mut state: ResMut<CharacterPanelState>,
    player: Query<(Entity, &ActorAttributes), With<Actor>>,
    mut writer: MessageWriter<Try>,
) {
    if !focus.has(Panel::Character) || console.visible || state.tab != PanelTab::Attributes {
        return;
    }
    let Ok((ent, attrs)) = player.single() else { return };
    let level = attrs.total_level();
    if keyboard.clear_just_pressed(KEYCODE_NEXT_PAIR) {
        state.pair = (state.pair + 1) % 3;
    }
    for (key, change) in KEYS {
        if keyboard.clear_just_pressed(key) {
            let own = attrs.pairs();
            let draft = step(state.pending_respec.unwrap_or(own), state.pair, change, level);
            state.pending_respec = (draft != own).then_some(draft);
        }
    }
    if keyboard.clear_just_pressed(KEYCODE_APPLY) {
        if let Some(draft) = state.pending_respec.filter(|draft| ActorAttributes::invested(draft) == level) {
            // The draft stays until the server confirms it.
            writer.write(Try { event: GameEvent::RespecAttributes { ent, pairs: draft } });
        }
    }
}

/// Handle Do event - apply confirmed respec
pub fn handle_respec_confirmed(
    mut state: ResMut<CharacterPanelState>,
    mut reader: MessageReader<Do>,
    mut player_query: Query<&mut ActorAttributes, With<Actor>>,
) {
    for message in reader.read() {
        if let GameEvent::RespecAttributes { ent, pairs } = &message.event {
            // Apply to player's ActorAttributes
            if let Ok(mut attrs) = player_query.get_mut(*ent) {
                attrs.apply_respec(*pairs);

                // Clear pending state now that server confirmed
                state.pending_respec = None;
            }
        }
    }
}

/// Show the apply label while a respec is in hand
pub fn toggle_apply_label(
    state: Res<CharacterPanelState>,
    mut label_query: Query<&mut Visibility, With<ApplyRespecLabel>>,
) {
    let Ok(mut vis) = label_query.single_mut() else {
        return;
    };

    *vis = if state.has_pending_changes() {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEVEL: u32 = 6;
    const CHANGES: [Change; 6] = [
        Change::Axis(-1),
        Change::Axis(1),
        Change::Spectrum(-1),
        Change::Spectrum(1),
        Change::Shift(-1),
        Change::Shift(1),
    ];

    fn shifts_as_allowed(pair: Pair) -> bool {
        let mut held = pair;
        held.set_shift(pair.shift);
        held == pair
    }

    /// Whatever keys come in whatever order, the draft never puts in more
    /// levels than the character has and every pair keeps a shift it allows;
    /// and the run does spend the whole budget, so the bound is met.
    #[test]
    fn no_run_of_keys_overspends_or_misshifts() {
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        let mut draft = [Pair::default(); 3];
        let mut peak = 0;
        for _ in 0..5000 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let pick = (seed >> 33) as usize;
            draft = step(draft, pick % 3, CHANGES[(pick / 3) % CHANGES.len()], LEVEL);
            assert!(ActorAttributes::fits(&draft, LEVEL));
            assert!(draft.iter().all(|pair| shifts_as_allowed(*pair)));
            peak = peak.max(ActorAttributes::invested(&draft));
        }
        assert_eq!(peak, LEVEL);
    }

    /// With every level in, a change that puts one more in is refused, a
    /// change that takes one out is not, and a shift costs none.
    #[test]
    fn a_full_budget_refuses_only_what_grows_it() {
        let spent = [Pair::new(-3, 3, 0), Pair::default(), Pair::default()];
        assert_eq!(ActorAttributes::invested(&spent), LEVEL);
        for grows in [Change::Axis(-1), Change::Spectrum(1)] {
            assert_eq!(step(spent, 0, grows, LEVEL), spent);
            assert_eq!(step(spent, 1, grows, LEVEL), spent);
        }
        for frees in [Change::Axis(1), Change::Spectrum(-1)] {
            assert!(ActorAttributes::invested(&step(spent, 0, frees, LEVEL)) < LEVEL);
        }
        let shifted = step(spent, 0, Change::Shift(1), LEVEL);
        assert_ne!(shifted, spent);
        assert_eq!(ActorAttributes::invested(&shifted), LEVEL);
    }

    /// A pair leans from the side its axis committed to, toward the other,
    /// and a pair with no axis does not lean.
    #[test]
    fn a_shift_leans_away_from_the_committed_side() {
        let with_axis = |axis| [Pair::new(axis, 2, 0), Pair::default(), Pair::default()];
        let left = with_axis(-2);
        assert_eq!(step(left, 0, Change::Shift(-1), LEVEL), left);
        assert!(step(left, 0, Change::Shift(1), LEVEL)[0].shift > 0);
        let right = with_axis(2);
        assert_eq!(step(right, 0, Change::Shift(1), LEVEL), right);
        assert!(step(right, 0, Change::Shift(-1), LEVEL)[0].shift < 0);
        let balanced = with_axis(0);
        for by in [-1, 1] {
            assert_eq!(step(balanced, 0, Change::Shift(by), LEVEL), balanced);
        }
    }

    /// Narrowing the spectrum or taking the axis away brings a shift back
    /// inside what the pair then allows.
    #[test]
    fn narrowing_or_uncommitting_takes_the_shift_with_it() {
        let leaning = [Pair::new(-1, 3, 3), Pair::default(), Pair::default()];
        let narrowed = step(leaning, 0, Change::Spectrum(-1), LEVEL);
        assert!(narrowed[0].shift < leaning[0].shift);
        assert!(shifts_as_allowed(narrowed[0]));
        let uncommitted = step(leaning, 0, Change::Axis(1), LEVEL);
        assert_eq!(uncommitted[0].shift, 0);
    }

    /// The keys are each given once and none is one the panel already takes.
    #[test]
    fn the_keys_are_distinct_and_unclaimed() {
        let taken = [
            KeyCode::Numpad0,
            KeyCode::NumpadAdd,
            KeyCode::NumpadSubtract,
            KeyCode::NumpadDivide,
            KEYCODE_NEXT_PAIR,
            KEYCODE_APPLY,
        ];
        for (i, (key, _)) in KEYS.iter().enumerate() {
            assert!(!taken.contains(key));
            assert!(KEYS[i + 1..].iter().all(|(other, _)| other != key));
        }
    }
}
