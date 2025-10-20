use bevy::prelude::*;
use noise::{NoiseFn, Perlin};

#[derive(Default, Resource)]
pub struct Terrain {
    generator: Perlin,
}

// Region scale - determines mountain vs flat areas (broader mountain ranges)
const REGION_SCALE: f64 = 1. / 250.;
// Base terrain scale - gentle rolling in flat areas
const BASE_SCALE: f64 = 1. / 400.;
// Mountain formation scales - creates dramatic peaks with more massive structure
const MOUNTAIN_LARGE_SCALE: f64 = 1. / 150.;
const MOUNTAIN_MEDIUM_SCALE: f64 = 1. / 65.;
const MOUNTAIN_DETAIL_SCALE: f64 = 1. / 20.;

impl Terrain {
    pub fn get(&self, x: f32, y: f32) -> i16 {
        let x = x as f64;
        let y = y as f64;
        
        // Region layer: determines if this area should be mountainous
        // Uses offset coordinates to decorrelate from other layers
        let region_noise = self.generator.get([x * REGION_SCALE + 1000., y * REGION_SCALE + 1000.]);
        
        // Threshold creates distinct mountain vs flat regions
        // Values > -0.5 are mountainous (very frequent), < -0.5 are relatively flat
        let mountain_strength = if region_noise > -0.5 {
            // Smooth transition into mountain regions
            ((region_noise + 0.5) / 1.5).min(1.0).max(0.0)
        } else {
            0.0
        };
        
        // Base layer: gentle rolling terrain (always present)
        let base = self.generator.get([x * BASE_SCALE, y * BASE_SCALE]) * 5.0;
        
        // Mountain layers: only significant in mountain regions
        if mountain_strength > 0.25 {
            // Large-scale mountain formation
            let mountain_base = self.generator.get([x * MOUNTAIN_LARGE_SCALE + 2000., y * MOUNTAIN_LARGE_SCALE + 2000.]);
            
            // Medium-scale peaks and valleys
            let mountain_peaks = self.generator.get([x * MOUNTAIN_MEDIUM_SCALE + 3000., y * MOUNTAIN_MEDIUM_SCALE + 3000.]);
            
            // Fine detail for cliff faces
            let mountain_detail = self.generator.get([x * MOUNTAIN_DETAIL_SCALE + 4000., y * MOUNTAIN_DETAIL_SCALE + 4000.]);
            
            // Combine mountain layers with dramatic scaling
            // Use power functions to create sharp peaks
            let mountain_height = (mountain_base * 20.0) + 
                                  (mountain_peaks.abs().powf(2.0) * mountain_peaks.signum() * 45.0) +
                                  (mountain_detail * 10.0);
            
            // Apply mountain strength
            let mountain_contribution = mountain_height * (mountain_strength - 0.25) * 1.33;
            let combined = base + mountain_contribution;
            
            // Calculate local slope/gradient to determine cliff vs slope
            // Sample nearby points to estimate terrain ruggedness
            let sample_dist = 2.0;
            let dx = (self.generator.get([(x + sample_dist) * MOUNTAIN_MEDIUM_SCALE + 3000., y * MOUNTAIN_MEDIUM_SCALE + 3000.]) -
                      self.generator.get([(x - sample_dist) * MOUNTAIN_MEDIUM_SCALE + 3000., y * MOUNTAIN_MEDIUM_SCALE + 3000.])) * 0.5;
            let dy = (self.generator.get([x * MOUNTAIN_MEDIUM_SCALE + 3000., (y + sample_dist) * MOUNTAIN_MEDIUM_SCALE + 3000.]) -
                      self.generator.get([x * MOUNTAIN_MEDIUM_SCALE + 3000., (y - sample_dist) * MOUNTAIN_MEDIUM_SCALE + 3000.])) * 0.5;
            
            // Calculate slope magnitude (gradient)
            let slope = (dx * dx + dy * dy).sqrt();
            
            // Blend between smooth and stratified based on slope
            // High slope (>0.3) = full stratification (dramatic cliffs)
            // Low slope (<0.1) = smooth (navigable slopes)
            let stratification_factor = ((slope - 0.1) / 0.2).clamp(0.0, 1.0);
            
            // Create two versions: stratified and smooth
            let stratified = (combined / 4.0).round() * 4.0;
            let smooth = combined.round();
            
            // Blend based on local slope
            let blended = smooth + (stratified - smooth) * stratification_factor;
            
            blended as i16
        } else {
            // Flat/gentle regions: NO stratification, natural slopes with z changes of 1
            let flat_detail = self.generator.get([x * (BASE_SCALE * 1.5) + 5000., y * (BASE_SCALE * 1.5) + 5000.]) * 3.0;
            let combined = base + flat_detail;
            
            // Round to nearest integer for natural z=1 slopes
            combined.round() as i16
        }
    }
}
