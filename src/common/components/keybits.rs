use bevy::prelude::*;
use serde::{Deserialize, Serialize};

pub const KB_HEADING_Q: u16 = 1 << 0;
pub const KB_HEADING_R: u16 = 1 << 1;
pub const KB_HEADING_NEG: u16 = 1 << 2;
pub const KB_JUMP: u16 = 1 << 3;
pub const KB_ATTACK: u16 = 1 << 4;

#[derive(Clone, Component, Copy, Default, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct KeyBits{
    pub key_bits: u16
}

impl KeyBits {
    pub fn all_pressed<T>(&self, keys: T) -> bool
    where T : IntoIterator<Item = u16>, {
        keys.into_iter().all(|k| self.key_bits & k != 0)
    }
    
    pub fn any_pressed<T>(&self, keys: T) -> bool
    where T : IntoIterator<Item = u16>, {
        keys.into_iter().any(|k| self.key_bits & k != 0)
    }

    pub fn is_pressed(&self, key: u16) -> bool {
        self.key_bits & key != 0
    }

    pub fn set_pressed<T>(&mut self, keys: T, pressed: bool) 
    where T : IntoIterator<Item = u16>, {
        for k in keys.into_iter() {
            if pressed { self.key_bits |= k; }
            else { self.key_bits &= !k; }
        }
    }
}
