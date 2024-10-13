use std::collections::HashMap;

use bevy::prelude::*;

use crate::common::components::hx::*;

pub trait Lookup {
    fn find(&self, hx: Hx, dist: i8) -> (Option<Hx>, Entity);
    fn get(&self, hx: Hx) -> Entity;
    fn insert(&mut self, hx: Hx, ent: Entity);
    fn remove(&mut self, hx: Hx) -> Entity;
}

#[derive(Default, Resource)]
pub struct Map {
    map: HashMap<Hx, Entity>,
}

impl Lookup for Map {
    fn find(&self, hx: Hx, dist: i8) -> (Option<Hx>, Entity) {
        for i in 0..=dist.abs() {
            let z = if dist < 0 { -i as i16 } else { i as i16 };
            let hx = hx + Hx { z, ..default() };
            if let Some(ent) = self.map.get(&hx) { return (Some(hx), *ent); }
        }
        (None, Entity::PLACEHOLDER)
    }

    fn get(&self, hx: Hx) -> Entity {
        *self.map.get(&hx).unwrap_or(&Entity::PLACEHOLDER)
    }

    fn insert(&mut self, hx: Hx, ent: Entity) {
        self.map.insert(hx, ent);
    }

    fn remove(&mut self, hx: Hx) -> Entity {
        self.map.remove(&hx).unwrap_or(Entity::PLACEHOLDER)
    }
}