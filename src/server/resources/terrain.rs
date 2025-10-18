use bevy::prelude::*;
use noise::{NoiseFn, Perlin};

#[derive(Default, Resource)]
pub struct Terrain {
    generator: Perlin,
}

// Base terrain scale - large rolling hills
const BASE_SCALE: f64 = 1. / 150.;
// Feature scale - occasional sharp features
const FEATURE_SCALE: f64 = 1. / 50.;
// Detail scale - small variations
const DETAIL_SCALE: f64 = 1. / 25.;

impl Terrain {
    pub fn get(&self, x: f32, y: f32) -> i16 {
        let x = x as f64;
        let y = y as f64;
        
        // Base layer: moderate rolling terrain
        let base = self.generator.get([x * BASE_SCALE, y * BASE_SCALE]) * 15.0;
        
        // Feature layer: creates occasional sharp rises/falls
        let feature = self.generator.get([x * FEATURE_SCALE, y * FEATURE_SCALE]);
        // Only create sharp features when noise is strong (> 0.4 or < -0.4)
        let sharp_feature = if feature.abs() > 0.4 {
            (feature - 0.4 * feature.signum()) * 20.0  // Sharp cliff/rise
        } else {
            0.0  // No sharp feature
        };
        
        // Detail layer: moderate variations
        let detail = self.generator.get([x * DETAIL_SCALE, y * DETAIL_SCALE]) * 4.0;
        
        // Combine: rolling hills with dramatic features
        let combined = base + sharp_feature + detail;
        
        combined as i16
    }
}
