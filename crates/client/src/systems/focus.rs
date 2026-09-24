//! Which open panel works the numpad: the one opened last, until it
//! closes, when the one under it has it again.

use bevy::prelude::*;

use crate::systems::{character_panel::CharacterPanelState, gathering::LootWindow};

/// A panel the numpad works.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    Character,
    Loot,
}

/// The open panels in the order they opened.
#[derive(Resource, Default, Debug)]
pub struct NumpadFocus(Vec<Panel>);

impl NumpadFocus {
    /// Whether `panel` works the numpad: it is the last opened of those
    /// open.
    pub fn has(&self, panel: Panel) -> bool {
        self.0.last() == Some(&panel)
    }

    /// Whether no panel is open.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Notes `panel` open or shut: opened, it goes over the rest; shut, it
    /// leaves the order.
    fn set(&mut self, panel: Panel, open: bool) {
        let listed = self.0.contains(&panel);
        if open && !listed {
            self.0.push(panel);
        } else if !open && listed {
            self.0.retain(|&p| p != panel);
        }
    }
}

/// Follows each panel opening and shutting.
pub fn track(state: Res<CharacterPanelState>, window: Res<LootWindow>, mut focus: ResMut<NumpadFocus>) {
    focus.set(Panel::Character, state.visible);
    focus.set(Panel::Loot, window.entries.is_some());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The panel opened last has the numpad; shut, the one under it has it
    /// again.
    #[test]
    fn the_last_opened_panel_has_the_numpad() {
        let mut focus = NumpadFocus::default();
        focus.set(Panel::Character, true);
        assert!(focus.has(Panel::Character));
        focus.set(Panel::Loot, true);
        assert!(focus.has(Panel::Loot) && !focus.has(Panel::Character));
        focus.set(Panel::Character, true);
        assert!(focus.has(Panel::Loot), "a panel already open does not take it back");
        focus.set(Panel::Loot, false);
        assert!(focus.has(Panel::Character));
        focus.set(Panel::Character, false);
        focus.set(Panel::Loot, true);
        focus.set(Panel::Character, true);
        assert!(focus.has(Panel::Character), "opened over the loot window, the panel has it");
    }
}
