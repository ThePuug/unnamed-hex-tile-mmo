use bevy::prelude::*;
use bimap::BiMap;

#[derive(Debug, Default, Deref, DerefMut, Resource)]
pub struct EntityMap(BiMap<Entity,Entity>);

#[derive(Debug, Default, Resource)]
pub struct Server {
    pub elapsed_offset: u128,
}
