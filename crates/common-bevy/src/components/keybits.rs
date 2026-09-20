//! The keys that drive movement, one byte sent with every input message:
//! the held keys the server integrates over the client-timed input. The
//! heading never crosses the wire; it is what the turn keys have produced,
//! on both sides alike, so a client cannot claim one.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Travel along the heading at the entity's speed.
pub const KB_FORWARD: u8 = 1 << 0;
/// Travel opposite the heading; the heading, and so the facing, holds.
pub const KB_BACK: u8 = 1 << 1;
/// Turn the heading counter-clockwise, one bearing per repeat interval.
pub const KB_LEFT: u8 = 1 << 2;
/// Turn the heading clockwise, one bearing per repeat interval.
pub const KB_RIGHT: u8 = 1 << 3;
/// Start a jump. Set for one input only: the server arms a jump on every
/// message that carries it, so a held bit would re-arm on landing.
pub const KB_JUMP: u8 = 1 << 4;

#[derive(Clone, Component, Copy, Default, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct KeyBits {
    pub key_bits: u8,
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

    /// Whether the entity travels: forward and back together cancel.
    pub fn moving(&self) -> bool {
        self.is_pressed(KB_FORWARD) != self.is_pressed(KB_BACK)
    }

    /// Whether the travel runs opposite the heading.
    pub fn back(&self) -> bool {
        self.is_pressed(KB_BACK) && !self.is_pressed(KB_FORWARD)
    }

    /// Bearings the heading turns per repeat: -1 counter-clockwise for
    /// left, 1 clockwise for right, 0 for neither or both.
    pub fn turn(&self) -> i8 {
        self.is_pressed(KB_RIGHT) as i8 - self.is_pressed(KB_LEFT) as i8
    }

    /// Whether two inputs would drive the same movement.
    pub fn same_input(&self, other: &KeyBits) -> bool {
        self.key_bits == other.key_bits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_set_and_clear() {
        let mut input = KeyBits::default();
        input.set_pressed([KB_FORWARD, KB_JUMP], true);
        assert!(input.is_pressed(KB_FORWARD) && input.is_pressed(KB_JUMP));
        input.set_pressed([KB_JUMP], false);
        assert!(input.is_pressed(KB_FORWARD) && !input.is_pressed(KB_JUMP));
    }

    #[test]
    fn same_input_ignores_the_accumulator() {
        let a = KeyBits { key_bits: KB_FORWARD, accumulator: 5 };
        let b = KeyBits { accumulator: 9, ..a };
        assert!(a.same_input(&b));
        assert!(!a.same_input(&KeyBits { key_bits: KB_FORWARD | KB_LEFT, ..a }));
        assert!(!a.same_input(&KeyBits { key_bits: 0, ..a }));
    }

    #[test]
    fn opposed_keys_cancel() {
        let both = KeyBits { key_bits: KB_FORWARD | KB_BACK | KB_LEFT | KB_RIGHT, accumulator: 0 };
        assert!(!both.moving() && !both.back() && both.turn() == 0);
        let back_left = KeyBits { key_bits: KB_BACK | KB_LEFT, accumulator: 0 };
        assert!(back_left.moving() && back_left.back() && back_left.turn() == -1);
        let right = KeyBits { key_bits: KB_RIGHT, accumulator: 0 };
        assert!(!right.moving() && right.turn() == 1);
    }
}
