use bevy::prelude::*;

#[derive(Clone, Component, Copy, Deref)]
pub struct Animator(Entity);

impl Animator {
    pub fn new(entity: Entity) -> Self {
        Self(entity)
    }
}

#[derive(Component)]
pub enum Info {
    Time,
    Fps,
}