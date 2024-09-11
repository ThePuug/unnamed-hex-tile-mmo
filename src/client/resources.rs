use bevy::prelude::*;

use crate::Input;

#[derive(Debug, Default, Resource)]
pub struct Client {
    pub input: Input,
    pub ent: Option<Entity>,
}