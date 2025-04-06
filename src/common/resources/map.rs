use bevy::prelude::*;
use qrz;

#[derive(Deref, DerefMut, Resource)]
pub struct Map(qrz::Map<Entity>);

impl Map {
    pub fn new(map: qrz::Map<Entity>) -> Map {
        Map(map)
    }
}
