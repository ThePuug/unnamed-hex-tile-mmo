//! Bitpacked Input State Management
//!
//! This module provides an efficient representation of player input state using bitpacked flags.
//! The `KeyBits` component stores all relevant input state in a single u8 byte, allowing for
//! efficient network transmission and storage.
//!
//! # Input Encoding
//!
//! Player inputs are encoded using individual bits:
//! - Bit 0 (KB_HEADING_Q): Movement along the Q axis in hex coordinates
//! - Bit 1 (KB_HEADING_R): Movement along the R axis in hex coordinates
//! - Bit 2 (KB_HEADING_NEG): Negative movement flag
//! - Bit 3 (KB_JUMP): Jump input
//!
//! # Hex Direction Mapping
//!
//! The six hexagonal directions are represented by combinations of Q, R, and NEG bits:
//! - East (Q+):       KB_HEADING_Q
//! - Southeast (R+):  KB_HEADING_R
//! - Southwest (Q-R+): KB_HEADING_Q | KB_HEADING_R
//! - West (Q-):       KB_HEADING_Q | KB_HEADING_NEG
//! - Northwest (R-):  KB_HEADING_R | KB_HEADING_NEG
//! - Northeast (Q+R-): KB_HEADING_Q | KB_HEADING_R | KB_HEADING_NEG
//!
//! This encoding allows us to represent any of the 6 cardinal hexagonal directions
//! plus jump state in just 4 bits, leaving room for future input expansion.
//!
//! # Network Protocol
//!
//! KeyBits is serialized with serde for network transmission. The `accumulator` field
//! is marked with `#[serde(skip)]` as it's only used for client-side timing and shouldn't
//! be transmitted over the network.
//!
//! # Usage
//!
//! ```
//! # use unnamed_hex_tile_mmo::common::components::keybits::*;
//! let mut input = KeyBits::default();
//!
//! // Set multiple keys pressed
//! input.set_pressed([KB_HEADING_Q, KB_JUMP], true);
//!
//! // Check individual keys
//! assert!(input.is_pressed(KB_JUMP));
//!
//! // Check if all specified keys are pressed
//! assert!(input.all_pressed([KB_HEADING_Q, KB_JUMP]));
//! ```

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::common::components::heading::*;

pub const KB_HEADING_Q: u8 = 1 << 0;
pub const KB_HEADING_R: u8 = 1 << 1;
pub const KB_HEADING_NEG: u8 = 1 << 2;
pub const KB_JUMP: u8 = 1 << 3;

#[derive(Clone, Component, Copy, Default, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct KeyBits{
    pub key_bits: u8,
    #[serde(skip)] pub accumulator: u128,
}

impl KeyBits {
    pub fn all_pressed<T>(&self, keys: T) -> bool
    where T : IntoIterator<Item = u8>, {
        keys.into_iter().all(|k| self.key_bits & k != 0)
    }
    
    pub fn any_pressed<T>(&self, keys: T) -> bool
    where T : IntoIterator<Item = u8>, {
        keys.into_iter().any(|k| self.key_bits & k != 0)
    }

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
}

impl From<Heading> for KeyBits {
    fn from(value: Heading) -> Self {
        if value == default() { return KeyBits::default(); }
        let value_s = -value.q-value.r;
        let (q_r, r_s, s_q) = (value.q-value.r, value.r-value_s, value_s-value.q);
        let Some(&dir) = [q_r.abs(), r_s.abs(), s_q.abs()].iter().max() else { panic!("no max") };
        KeyBits { key_bits: match dir {
            dir if dir == q_r => KB_HEADING_Q | KB_HEADING_R | KB_HEADING_NEG,
            dir if dir == r_s => KB_HEADING_R,
            dir if dir == s_q => KB_HEADING_Q | KB_HEADING_NEG,
            dir if dir == -q_r => KB_HEADING_Q | KB_HEADING_R,
            dir if dir == -r_s => KB_HEADING_R | KB_HEADING_NEG,
            dir if dir == -s_q => KB_HEADING_Q,
            _ => unreachable!(),
        }, accumulator: 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qrz::Qrz;

    // ===== BIT MANIPULATION TESTS =====

    #[test]
    fn test_default_keybits_all_unpressed() {
        let keybits = KeyBits::default();
        assert_eq!(keybits.key_bits, 0, "Default should have no keys pressed");
        assert!(!keybits.is_pressed(KB_HEADING_Q));
        assert!(!keybits.is_pressed(KB_HEADING_R));
        assert!(!keybits.is_pressed(KB_HEADING_NEG));
        assert!(!keybits.is_pressed(KB_JUMP));
    }

    #[test]
    fn test_set_single_key_pressed() {
        let mut keybits = KeyBits::default();
        keybits.set_pressed([KB_JUMP], true);

        assert!(keybits.is_pressed(KB_JUMP), "Jump should be pressed");
        assert!(!keybits.is_pressed(KB_HEADING_Q), "Q should not be pressed");
    }

    #[test]
    fn test_set_multiple_keys_pressed() {
        let mut keybits = KeyBits::default();
        keybits.set_pressed([KB_HEADING_Q, KB_HEADING_R], true);

        assert!(keybits.is_pressed(KB_HEADING_Q));
        assert!(keybits.is_pressed(KB_HEADING_R));
        assert!(!keybits.is_pressed(KB_HEADING_NEG));
    }

    #[test]
    fn test_unpress_key() {
        let mut keybits = KeyBits::default();
        keybits.set_pressed([KB_JUMP], true);
        assert!(keybits.is_pressed(KB_JUMP));

        keybits.set_pressed([KB_JUMP], false);
        assert!(!keybits.is_pressed(KB_JUMP), "Jump should be unpressed");
    }

    #[test]
    fn test_all_pressed() {
        let mut keybits = KeyBits::default();
        keybits.set_pressed([KB_HEADING_Q, KB_HEADING_R, KB_HEADING_NEG], true);

        assert!(keybits.all_pressed([KB_HEADING_Q, KB_HEADING_R]));
        assert!(keybits.all_pressed([KB_HEADING_Q, KB_HEADING_R, KB_HEADING_NEG]));
        assert!(!keybits.all_pressed([KB_HEADING_Q, KB_JUMP]), "Jump not pressed");
    }

    #[test]
    fn test_any_pressed() {
        let mut keybits = KeyBits::default();
        keybits.set_pressed([KB_HEADING_Q], true);

        assert!(keybits.any_pressed([KB_HEADING_Q, KB_JUMP]));
        assert!(!keybits.any_pressed([KB_HEADING_R, KB_JUMP]));
    }

    #[test]
    fn test_bit_constants_unique() {
        // Ensure each constant is a unique power of 2
        assert_eq!(KB_HEADING_Q, 1);
        assert_eq!(KB_HEADING_R, 2);
        assert_eq!(KB_HEADING_NEG, 4);
        assert_eq!(KB_JUMP, 8);

        // No overlap
        assert_eq!(KB_HEADING_Q & KB_HEADING_R, 0);
        assert_eq!(KB_HEADING_Q & KB_HEADING_NEG, 0);
        assert_eq!(KB_HEADING_Q & KB_JUMP, 0);
    }

    // ===== HEADING <-> KEYBITS CONVERSION TESTS =====

    #[test]
    fn test_heading_to_keybits_six_directions() {
        // Test all 6 hex directions convert to valid KeyBits
        let directions = vec![
            (Qrz { q: -1, r: 0, z: 0 }, "West"),       // KB_Q | KB_NEG
            (Qrz { q: -1, r: 1, z: 0 }, "Southwest"),  // KB_Q | KB_R
            (Qrz { q: 0, r: 1, z: 0 }, "Southeast"),   // KB_R
            (Qrz { q: 1, r: 0, z: 0 }, "East"),        // KB_Q
            (Qrz { q: 1, r: -1, z: 0 }, "Northeast"),  // KB_Q | KB_R | KB_NEG
            (Qrz { q: 0, r: -1, z: 0 }, "Northwest"),  // KB_R | KB_NEG
        ];

        for (qrz, name) in directions {
            let heading = Heading::new(qrz);
            let keybits = KeyBits::from(heading);

            // KeyBits should have at least one direction bit set
            assert!(
                keybits.is_pressed(KB_HEADING_Q) || keybits.is_pressed(KB_HEADING_R),
                "{} should set at least one heading bit", name
            );
        }
    }

    #[test]
    fn test_keybits_to_heading_roundtrip() {
        // Test that KeyBits -> Heading -> KeyBits preserves direction
        let test_cases: Vec<(&[u8], Qrz)> = vec![
            (&[KB_HEADING_Q, KB_HEADING_R, KB_HEADING_NEG], Qrz { q: 1, r: -1, z: 0 }),  // NE
            (&[KB_HEADING_Q, KB_HEADING_R], Qrz { q: -1, r: 1, z: 0 }),  // SW
            (&[KB_HEADING_Q, KB_HEADING_NEG], Qrz { q: -1, r: 0, z: 0 }), // W
            (&[KB_HEADING_R, KB_HEADING_NEG], Qrz { q: 0, r: -1, z: 0 }), // NW
            (&[KB_HEADING_Q], Qrz { q: 1, r: 0, z: 0 }),  // E
            (&[KB_HEADING_R], Qrz { q: 0, r: 1, z: 0 }),  // SE
        ];

        for (keys, expected_qrz) in test_cases {
            let mut keybits = KeyBits::default();
            keybits.set_pressed(keys.iter().copied(), true);

            let heading = Heading::from(keybits);
            assert_eq!(*heading, expected_qrz, "KeyBits {:?} should map to {:?}", keys, expected_qrz);
        }
    }

    #[test]
    fn test_zero_heading_produces_zero_keybits() {
        let heading = Heading::new(Qrz::default());
        let keybits = KeyBits::from(heading);

        assert_eq!(keybits.key_bits, 0, "Zero heading should produce zero keybits");
    }

    #[test]
    fn test_heading_preserves_direction() {
        // Heading -> KeyBits -> Heading should preserve direction
        let original = Qrz { q: 1, r: -1, z: 0 };
        let heading1 = Heading::new(original);
        let keybits = KeyBits::from(heading1);
        let heading2 = Heading::from(keybits);

        assert_eq!(*heading2, original, "Roundtrip should preserve heading");
    }

    // ===== PROPERTY TESTS =====

    #[test]
    fn test_keybits_equality() {
        let mut kb1 = KeyBits::default();
        let mut kb2 = KeyBits::default();

        kb1.set_pressed([KB_JUMP], true);
        kb2.set_pressed([KB_JUMP], true);

        assert_eq!(kb1.key_bits, kb2.key_bits, "Same keys should be equal");
    }


    #[test]
    fn test_multiple_set_operations_idempotent() {
        let mut keybits = KeyBits::default();

        // Setting same key multiple times should be idempotent
        keybits.set_pressed([KB_JUMP], true);
        let first = keybits.key_bits;

        keybits.set_pressed([KB_JUMP], true);
        let second = keybits.key_bits;

        assert_eq!(first, second, "Setting same key twice should be idempotent");
    }

    #[test]
    fn test_all_keys_can_be_pressed_simultaneously() {
        let mut keybits = KeyBits::default();
        keybits.set_pressed([KB_HEADING_Q, KB_HEADING_R, KB_HEADING_NEG, KB_JUMP], true);

        assert!(keybits.is_pressed(KB_HEADING_Q));
        assert!(keybits.is_pressed(KB_HEADING_R));
        assert!(keybits.is_pressed(KB_HEADING_NEG));
        assert!(keybits.is_pressed(KB_JUMP));
    }
}