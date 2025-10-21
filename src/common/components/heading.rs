//! Hexagonal Movement Direction Component
//!
//! This module defines the `Heading` component which represents movement direction
//! in hexagonal coordinate space. Heading is used to determine which direction an
//! entity is moving or facing.
//!
//! # Coordinate System
//!
//! Heading uses the same Qrz (axial hex coordinates) as the rest of the codebase,
//! typically representing unit direction vectors in one of the six cardinal hex directions.
//!
//! # Conversions
//!
//! ## KeyBits → Heading
//! Player input (KeyBits) is converted to Heading using a priority-based mapping:
//! 1. Q+R+NEG → Northeast (1, -1, 0)
//! 2. Q+R → Southwest (-1, 1, 0)
//! 3. Q+NEG → West (-1, 0, 0)
//! 4. R+NEG → Northwest (0, -1, 0)
//! 5. Q → East (1, 0, 0)
//! 6. R → Southeast (0, 1, 0)
//!
//! ## Heading → Quat
//! Heading is converted to a quaternion rotation for rendering. Each of the six
//! cardinal directions maps to a specific Y-axis rotation:
//! - West: PI*3/6 (90°)
//! - Southwest: PI*5/6 (150°)
//! - Southeast: PI*7/6 (210°)
//! - East: PI*9/6 (270°)
//! - Northeast: PI*11/6 (330°)
//! - Northwest: PI*1/6 (30°)
//!
//! ## Heading → KeyBits
//! Heading can be converted back to KeyBits for network transmission. This conversion
//! analyzes the Qrz coordinates to determine which key combination represents that direction.
//!
//! # Distance Thresholds
//!
//! - `HERE` (0.33): Threshold for considering an entity at its current tile
//! - `THERE` (1.33): Threshold for considering an entity at an adjacent tile
//!
//! These thresholds are used in movement and collision detection to determine when
//! an entity has crossed tile boundaries.

use std::f32::consts::PI;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::common::components::{ *,
    keybits::*,
};

pub const HERE: f32 = 0.33;
pub const THERE: f32 = 1.33;

#[derive(Clone, Component, Copy, Debug, Default, Deref, DerefMut, Deserialize, Eq, PartialEq, Serialize)]
pub struct Heading(Qrz);

impl Heading {
    pub fn new(qrz: Qrz) -> Self {
        Self(qrz)
    }
}

impl From<Heading> for Quat {
    fn from(value: Heading) -> Self {
        match (value.q, value.r) {
            (-1, 0) => Quat::from_rotation_y(PI*3./6.),
            (-1, 1) => Quat::from_rotation_y(PI*5./6.),
            (0, 1)  => Quat::from_rotation_y(PI*7./6.),
            (1, 0)  => Quat::from_rotation_y(PI*9./6.),
            (1, -1) => Quat::from_rotation_y(PI*11./6.),
            (0, -1) => Quat::from_rotation_y(PI*1./6.),
            _  => Quat::from_rotation_y(PI),
        }
    }
}

impl From<KeyBits> for Heading {
    fn from(value: KeyBits) -> Self {
        Heading::new(if value.all_pressed([KB_HEADING_Q, KB_HEADING_R, KB_HEADING_NEG]) { Qrz { q: 1, r: -1, z: 0 } }
            else if value.all_pressed([KB_HEADING_Q, KB_HEADING_R]) { Qrz { q: -1, r: 1, z: 0 } }
            else if value.all_pressed([KB_HEADING_Q, KB_HEADING_NEG]) { Qrz { q: -1, r: 0, z: 0 } }
            else if value.all_pressed([KB_HEADING_R, KB_HEADING_NEG]) { Qrz { q: 0, r: -1, z: 0 } }
            else if value.all_pressed([KB_HEADING_Q]) { Qrz { q: 1, r: 0, z: 0 } }
            else if value.all_pressed([KB_HEADING_R]) { Qrz { q: 0, r: 1, z: 0 } }
            else { Qrz::default() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    // ===== CONSTRUCTION AND DEFAULT TESTS =====

    #[test]
    fn test_default_heading_is_zero() {
        let heading = Heading::default();
        assert_eq!(*heading, Qrz::default());
        assert_eq!(heading.q, 0);
        assert_eq!(heading.r, 0);
        assert_eq!(heading.z, 0);
    }

    #[test]
    fn test_new_heading() {
        let heading = Heading::new(Qrz { q: 1, r: -1, z: 0 });
        assert_eq!(heading.q, 1);
        assert_eq!(heading.r, -1);
        assert_eq!(heading.z, 0);
    }

    #[test]
    fn test_heading_deref() {
        let heading = Heading::new(Qrz { q: 2, r: -1, z: -1 });
        let qrz: &Qrz = &*heading;
        assert_eq!(qrz.q, 2);
        assert_eq!(qrz.r, -1);
    }

    // ===== KEYBITS TO HEADING CONVERSION TESTS =====

    #[test]
    fn test_keybits_to_heading_six_cardinal_directions() {
        let test_cases: Vec<(&[u8], Qrz, &str)> = vec![
            (&[KB_HEADING_Q], Qrz { q: 1, r: 0, z: 0 }, "East"),
            (&[KB_HEADING_R], Qrz { q: 0, r: 1, z: 0 }, "Southeast"),
            (&[KB_HEADING_Q, KB_HEADING_R], Qrz { q: -1, r: 1, z: 0 }, "Southwest"),
            (&[KB_HEADING_Q, KB_HEADING_NEG], Qrz { q: -1, r: 0, z: 0 }, "West"),
            (&[KB_HEADING_R, KB_HEADING_NEG], Qrz { q: 0, r: -1, z: 0 }, "Northwest"),
            (&[KB_HEADING_Q, KB_HEADING_R, KB_HEADING_NEG], Qrz { q: 1, r: -1, z: 0 }, "Northeast"),
        ];

        for (keys, expected_qrz, direction_name) in test_cases {
            let mut keybits = KeyBits::default();
            keybits.set_pressed(keys.iter().copied(), true);

            let heading = Heading::from(keybits);
            assert_eq!(*heading, expected_qrz, "{} should map to {:?}", direction_name, expected_qrz);
        }
    }

    #[test]
    fn test_keybits_no_direction_produces_zero_heading() {
        let keybits = KeyBits::default();
        let heading = Heading::from(keybits);
        assert_eq!(*heading, Qrz::default(), "No keys pressed should produce zero heading");
    }

    #[test]
    fn test_keybits_jump_only_produces_zero_heading() {
        let mut keybits = KeyBits::default();
        keybits.set_pressed([KB_JUMP], true);

        let heading = Heading::from(keybits);
        assert_eq!(*heading, Qrz::default(), "Jump only should produce zero heading");
    }

    #[test]
    fn test_keybits_conversion_priority_order() {
        // Test that the conversion checks conditions in the right order
        // Q+R+NEG should take priority over Q+R
        let mut keybits = KeyBits::default();
        keybits.set_pressed([KB_HEADING_Q, KB_HEADING_R, KB_HEADING_NEG], true);

        let heading = Heading::from(keybits);
        assert_eq!(*heading, Qrz { q: 1, r: -1, z: 0 }, "Q+R+NEG should produce Northeast");
    }

    // ===== HEADING TO QUAT CONVERSION TESTS =====

    fn approx_eq(a: Quat, b: Quat, epsilon: f32) -> bool {
        (a.x - b.x).abs() < epsilon &&
        (a.y - b.y).abs() < epsilon &&
        (a.z - b.z).abs() < epsilon &&
        (a.w - b.w).abs() < epsilon
    }

    #[test]
    fn test_heading_to_quat_six_directions() {
        let test_cases = vec![
            (Qrz { q: -1, r: 0, z: 0 }, PI*3./6., "West"),
            (Qrz { q: -1, r: 1, z: 0 }, PI*5./6., "Southwest"),
            (Qrz { q: 0, r: 1, z: 0 }, PI*7./6., "Southeast"),
            (Qrz { q: 1, r: 0, z: 0 }, PI*9./6., "East"),
            (Qrz { q: 1, r: -1, z: 0 }, PI*11./6., "Northeast"),
            (Qrz { q: 0, r: -1, z: 0 }, PI*1./6., "Northwest"),
        ];

        for (qrz, expected_angle, direction_name) in test_cases {
            let heading = Heading::new(qrz);
            let quat: Quat = heading.into();
            let expected = Quat::from_rotation_y(expected_angle);

            assert!(
                approx_eq(quat, expected, 0.0001),
                "{} ({:?}) should produce rotation of {} radians, got {:?} expected {:?}",
                direction_name, qrz, expected_angle, quat, expected
            );
        }
    }

    #[test]
    fn test_heading_to_quat_default_fallback() {
        // Any heading that's not one of the six cardinal directions should produce PI rotation
        let heading = Heading::new(Qrz { q: 2, r: 0, z: -2 });
        let quat: Quat = heading.into();
        let expected = Quat::from_rotation_y(PI);

        assert!(approx_eq(quat, expected, 0.0001), "Non-cardinal heading should use default PI rotation");
    }

    #[test]
    fn test_heading_to_quat_zero_heading() {
        let heading = Heading::default();
        let quat: Quat = heading.into();
        let expected = Quat::from_rotation_y(PI);

        assert!(approx_eq(quat, expected, 0.0001), "Zero heading should use default PI rotation");
    }

    // ===== SERIALIZATION TESTS =====


    #[test]
    fn test_heading_equality() {
        let h1 = Heading::new(Qrz { q: 1, r: 0, z: -1 });
        let h2 = Heading::new(Qrz { q: 1, r: 0, z: -1 });
        let h3 = Heading::new(Qrz { q: 0, r: 1, z: -1 });

        assert_eq!(h1, h2, "Same headings should be equal");
        assert_ne!(h1, h3, "Different headings should not be equal");
    }

    // ===== ROUNDTRIP CONVERSION TESTS =====

    #[test]
    fn test_heading_keybits_heading_roundtrip() {
        // Heading -> KeyBits -> Heading should preserve direction for cardinal directions
        // Note: The roundtrip produces normalized unit direction vectors (z=0)
        let test_cases = vec![
            (Qrz { q: 1, r: 0, z: -1 }, Qrz { q: 1, r: 0, z: 0 }),   // East
            (Qrz { q: 0, r: 1, z: -1 }, Qrz { q: 0, r: 1, z: 0 }),   // Southeast
            (Qrz { q: -1, r: 1, z: 0 }, Qrz { q: -1, r: 1, z: 0 }),  // Southwest
            (Qrz { q: -1, r: 0, z: 1 }, Qrz { q: -1, r: 0, z: 0 }),  // West
            (Qrz { q: 0, r: -1, z: 1 }, Qrz { q: 0, r: -1, z: 0 }),  // Northwest
            (Qrz { q: 1, r: -1, z: 0 }, Qrz { q: 1, r: -1, z: 0 }),  // Northeast
        ];

        for (original_qrz, normalized_qrz) in test_cases {
            let heading1 = Heading::new(original_qrz);
            let keybits = KeyBits::from(heading1);
            let heading2 = Heading::from(keybits);

            assert_eq!(
                *heading2, normalized_qrz,
                "Roundtrip Heading->KeyBits->Heading should produce normalized direction for {:?}",
                original_qrz
            );
        }
    }

    #[test]
    fn test_keybits_heading_keybits_roundtrip() {
        // KeyBits -> Heading -> KeyBits should preserve key combination
        let test_cases: Vec<&[u8]> = vec![
            &[KB_HEADING_Q],
            &[KB_HEADING_R],
            &[KB_HEADING_Q, KB_HEADING_R],
            &[KB_HEADING_Q, KB_HEADING_NEG],
            &[KB_HEADING_R, KB_HEADING_NEG],
            &[KB_HEADING_Q, KB_HEADING_R, KB_HEADING_NEG],
        ];

        for keys in test_cases {
            let mut keybits1 = KeyBits::default();
            keybits1.set_pressed(keys.iter().copied(), true);

            let heading = Heading::from(keybits1);
            let keybits2 = KeyBits::from(heading);

            assert_eq!(
                keybits1.key_bits & 0b111, // Mask to only direction bits
                keybits2.key_bits & 0b111,
                "Roundtrip KeyBits->Heading->KeyBits should preserve direction bits for {:?}",
                keys
            );
        }
    }

    // ===== MAGIC NUMBER DOCUMENTATION TESTS =====

    #[test]
    fn test_here_constant_value() {
        assert_eq!(HERE, 0.33, "HERE threshold should be 0.33");
    }

    #[test]
    fn test_there_constant_value() {
        assert_eq!(THERE, 1.33, "THERE threshold should be 1.33");
    }

    #[test]
    fn test_here_less_than_there() {
        assert!(HERE < THERE, "HERE should be less than THERE for distance thresholds");
    }
}
