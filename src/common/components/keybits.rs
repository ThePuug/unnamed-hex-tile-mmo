use std::ops::{BitAnd, BitOr, BitOrAssign};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Component, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct KeyBits(pub u8);

impl BitAnd<u8> for KeyBits {
    type Output = KeyBits;

    fn bitand(self, rhs: u8) -> Self::Output {
        KeyBits { 0:self.0 & rhs }
    }
}

impl BitOr<u8> for KeyBits {
    type Output = KeyBits;

    fn bitor(self, rhs: u8) -> Self::Output {
        KeyBits { 0:self.0 | rhs }
    }
}

impl BitOrAssign<u8> for KeyBits {
    fn bitor_assign(&mut self, rhs: u8) {
        self.0 |= rhs;
    }
}
