use bevy::prelude::*;
use crate::{
    client::components::Terrain,
    common::resources::map::Map,
};

/// Resource to control slope rendering (toggled with 'H' key)
#[derive(Resource)]
pub struct SlopeRenderingEnabled(pub bool);

impl Default for SlopeRenderingEnabled {
    fn default() -> Self {
        // Start with slopes enabled (required for proper physics)
        SlopeRenderingEnabled(true)
    }
}

/// Resource to control fixed lighting at 9 AM (toggled with 'G' key)
#[derive(Resource)]
pub struct FixedLightingEnabled(pub bool);

impl Default for FixedLightingEnabled {
    fn default() -> Self {
        // Start with fixed lighting enabled for debugging
        FixedLightingEnabled(true)
    }
}

/// Toggle slope rendering with 'H' key and force mesh/grid regeneration
pub fn toggle_slopes(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut slopes_enabled: ResMut<SlopeRenderingEnabled>,
    mut terrain_query: Query<&mut Terrain>,
    mut map: ResMut<Map>,
) {
    if keyboard.just_pressed(KeyCode::KeyH) {
        slopes_enabled.0 = !slopes_enabled.0;
        info!("Slope rendering {}", if slopes_enabled.0 { "enabled" } else { "disabled" });
        
        // Force mesh regeneration
        if let Ok(mut terrain) = terrain_query.single_mut() {
            terrain.task_start_regenerate_mesh = true;
            terrain.tiles_since_last_regen = 0;
        }
        
        // Trigger map change detection to force grid regeneration
        map.set_changed();
    }
}

/// Toggle fixed lighting with 'G' key
pub fn toggle_fixed_lighting(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut fixed_lighting: ResMut<FixedLightingEnabled>,
) {
    if keyboard.just_pressed(KeyCode::KeyG) {
        fixed_lighting.0 = !fixed_lighting.0;
        info!("Fixed lighting {}", if fixed_lighting.0 { "enabled (9 AM)" } else { "disabled (dynamic)" });
    }
}
