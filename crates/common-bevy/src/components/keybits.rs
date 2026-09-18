//! The input state that drives movement: whether the entity is moving, the
//! heading it moves along, and a jump. One byte of flags plus the heading,
//! sent with every input message.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::heading::Heading;

/// Travel along `heading` at the entity's speed.
pub const KB_MOVE: u8 = 1 << 0;
/// Start a jump. Set for one input only: the server arms a jump on every
/// message that carries it, so a held bit would re-arm on landing.
pub const KB_JUMP: u8 = 1 << 1;

#[derive(Clone, Component, Copy, Default, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct KeyBits {
    pub key_bits: u8,
    /// The heading a move travels along. Kept across releases so an idle
    /// input compares equal to the last one and opens no new sequence.
    pub heading: Heading,
    #[serde(skip)] pub accumulator: u128,
}

impl KeyBits {
    pub fn is_pressed(&self, key: u8) -> bool {
        self.key_bits & key != 0
    }

    pub fn set_pressed<T>(&mut self, keys: T, pressed: bool)
    where T : IntoIterator<Item = u8>, {
        for k in keys.into_iter() {
            if pressed { self.key_bits |= k; }
            else { self.key_bits &= !k; }
        }
    }

    /// Whether two inputs would drive the same movement.
    pub fn same_input(&self, other: &KeyBits) -> bool {
        self.key_bits == other.key_bits && self.heading == other.heading
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_set_and_clear() {
        let mut input = KeyBits::default();
        input.set_pressed([KB_MOVE, KB_JUMP], true);
        assert!(input.is_pressed(KB_MOVE) && input.is_pressed(KB_JUMP));
        input.set_pressed([KB_JUMP], false);
        assert!(input.is_pressed(KB_MOVE) && !input.is_pressed(KB_JUMP));
    }

    #[test]
    fn same_input_ignores_the_accumulator() {
        let a = KeyBits { key_bits: KB_MOVE, heading: Heading::from_slot(3), accumulator: 5 };
        let b = KeyBits { accumulator: 9, ..a };
        assert!(a.same_input(&b));
        assert!(!a.same_input(&KeyBits { heading: Heading::from_slot(4), ..a }));
        assert!(!a.same_input(&KeyBits { key_bits: 0, ..a }));
    }
}
