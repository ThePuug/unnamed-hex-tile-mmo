use bevy::{
    prelude::*, 
    tasks::Task
};

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
}

#[derive(Component, Default)]
pub struct Terrain {
    pub task_regenerate_mesh: Option<Task<Mesh>>,
    pub task_start_regenerate_mesh: bool,
}