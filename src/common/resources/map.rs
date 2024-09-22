use std::collections::HashMap;

use bevy::prelude::*;

use crate::common::components::hx::*;

pub trait Lookup {
    fn find(&self, hx: Hx, dist: u8) -> (Option<Hx>, Entity);
    fn get(&self, hx: Hx) -> Entity;
    fn insert(&mut self, hx: Hx, ent: Entity);
    fn remove(&mut self, hx: Hx) -> Entity;
}

#[derive(Default, Resource)]
pub struct Map {
    map: HashMap<Hx, Entity>,
}

impl Lookup for Map {
    fn find(&self, hx: Hx, dist: u8) -> (Option<Hx>, Entity) {
        match self.map.get(&hx) {
            Some(ent) => (Some(hx), *ent),
            None => {
                for i in 1..=dist as i16 {
                    let hx = hx + Hx { z: 0-i, ..hx };
                    if let Some(ent) = self.map.get(&hx) { return (Some(hx), *ent); }
                    let hx = hx + Hx { z: i, ..hx };
                    if let Some(ent) = self.map.get(&hx) { return (Some(hx), *ent); }
                }
                (None, Entity::PLACEHOLDER)
            },
        }
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