use bevy::prelude::*;
use noise::{NoiseFn, Perlin};

use crate::common::{
    components::hx::*,
    resources::map::*,
};

#[derive(Default, Resource)]
pub struct TerrainedMap {
    generator: Perlin,
    map: Map,
}

impl Lookup for TerrainedMap {

    fn find(&self, hx: Hx, _dist: u8) -> (Option<Hx>, Entity) {
        let (loc, ent) = self.map.find(hx, 3);
        if loc.is_some() { return (loc, ent); }
        let px = Vec3::from(hx);
        let z = (self.generator.get([px.x as f64 / 3000., px.y as f64 / 3000.]) * 10.) as i16;
        let hx = Hx { z, ..hx };
        (Some(hx), Entity::PLACEHOLDER)
    }

    fn get(&self, hx: Hx) -> Entity {
        self.map.get(hx)
    }

    fn insert(&mut self, hx: Hx, ent: Entity) {
        self.map.insert(hx, ent);
    }

    fn remove(&mut self, hx: Hx) -> Entity {
        self.map.remove(hx)
    }
}

