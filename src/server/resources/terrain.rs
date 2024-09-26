use bevy::prelude::*;
use noise::{NoiseFn, Perlin};

#[derive(Default, Resource)]
pub struct Terrain {
    generator: Perlin,
}

impl Terrain {
    pub fn get(&self, x: f32, y: f32) -> i16 {
        (self.generator.get([x as f64 / 3000., y as f64 / 3000.]) * 10.) as i16
    }
}
