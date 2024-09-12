use std::collections::HashMap;

use bevy::prelude::*;

use crate::common::components::keybits::KeyBits;

#[derive(Debug, Default, Resource)]
pub struct Client {
    pub key_bits: KeyBits,
    pub ent: Option<Entity>,
}


#[derive(Debug, Default, Resource)]
pub struct Rpcs(pub HashMap<Entity,Entity>);