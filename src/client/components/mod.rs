use bevy::{
    prelude::*, 
    render::primitives::Aabb, 
    tasks::Task
};

#[derive(Clone, Component, Copy)]
#[relationship(relationship_target = AnimatedBy)]
pub struct Animates(pub Entity);

#[derive(Clone, Component, Copy, Deref)]
#[relationship_target(relationship = Animates)]
pub struct AnimatedBy(Entity);

#[derive(Component)]
pub enum Info {
    Time,
}

#[derive(Component, Default)]
pub struct Terrain {
    pub task_regenerate_mesh: Option<Task<(Mesh,Aabb)>>,
    pub task_start_regenerate_mesh: bool,
    pub tiles_since_last_regen: usize,
    pub last_tile_count: usize,
}

#[derive(Component, Default)]
pub struct TargetCursor;