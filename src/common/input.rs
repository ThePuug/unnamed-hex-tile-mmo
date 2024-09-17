use bevy::prelude::*;

use crate::common::hxpx::*;

#[derive(Component, Default)]
pub struct Heading(pub Hx);

pub const KEYBIT_UP: u8 = 1 << 0;
pub const KEYBIT_DOWN: u8 = 1 << 1; 
pub const KEYBIT_LEFT: u8 = 1 << 2; 
pub const KEYBIT_RIGHT: u8 = 1 << 3; 
// pub const KEYBIT_JUMP = 1 << 4,

