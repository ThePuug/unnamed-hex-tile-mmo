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