use bevy::prelude::*;
use noise::{NoiseFn, Perlin};

#[derive(Default, Resource)]
pub struct Terrain {
    generator: Perlin,
}

const SCALE: f64 = 1. / 150.;

impl Terrain {
    pub fn get(&self, x: f32, y: f32) -> i16 {
        (self.generator.get([x as f64 * SCALE, y as f64 * SCALE]) * 10.) as i16
    }
}
