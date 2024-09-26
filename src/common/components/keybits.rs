use std::ops::*;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

pub const KB_HEADING_Q: u8 = 1 << 0;
pub const KB_HEADING_R: u8 = 1 << 1;
pub const KB_HEADING_NEG: u8 = 1 << 2;
pub const KB_JUMP: u8 = 1 << 3;

#[derive(Clone, Component, Copy, Default, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct KeyBits(pub u8);

impl BitAnd<u8> for KeyBits {
    type Output = bool;

    fn bitand(self, rhs: u8) -> Self::Output {
        (self.0 & rhs) != 0
    }
}

impl BitOr<u8> for KeyBits {
    type Output = bool;

    fn bitor(self, rhs: u8) -> Self::Output {
        (self.0 | rhs) != 0
    }
}

impl BitXor<u8> for KeyBits {
    type Output = bool;

    fn bitxor(self, rhs: u8) -> Self::Output {
        (self.0 ^ rhs) != 0
    }
}

impl Not for KeyBits {
    type Output = bool;

    fn not(self) -> Self::Output {
        self.0 == 0
    }
}

impl BitOrAssign<u8> for KeyBits {
    fn bitor_assign(&mut self, rhs: u8) {
        self.0 |= rhs;
    }
}

impl BitAndAssign<u8> for KeyBits {
    fn bitand_assign(&mut self, rhs: u8) {
        self.0 &= rhs;
    }
}

impl BitXorAssign<u8> for KeyBits {
    fn bitxor_assign(&mut self, rhs: u8) {
        self.0 ^= rhs;
    }
}