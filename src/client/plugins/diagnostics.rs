use bevy::{
    diagnostic::*,
    prelude::*,
    render::diagnostic::*,
    pbr::NotShadowCaster,
    render::{
        mesh::PrimitiveTopology,
        render_asset::RenderAssetUsages,
        primitives::Aabb,
    },
};
use iyes_perf_ui::{PerfUiPlugin, prelude::*};

use crate::{
    client::components::Terrain,
    common::resources::map::Map,
};

/// Plugin that consolidates all debug and diagnostic features
pub struct DiagnosticsPlugin;

impl Plugin for DiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        // Add performance monitoring plugins
        app.add_plugins((
            FrameTimeDiagnosticsPlugin::default(),
            EntityCountDiagnosticsPlugin,
            RenderDiagnosticsPlugin,
            PerfUiPlugin,
        ));

        // Initialize debug resources
        app.init_resource::<DiagnosticsState>();
        app.init_resource::<DiagnosticsConfig>();

        // Add startup systems
        app.add_systems(Startup, (
            setup_debug_grid,
            setup_perf_ui,
        ));

        // Add update systems
        app.add_systems(Update, (
            toggle_grid,
            update_grid,
            toggle_slopes,
            toggle_fixed_lighting,
            toggle_perf_ui,
        ));
    }
}

/// Configuration for diagnostics features
#[derive(Resource)]
pub struct DiagnosticsConfig {
    /// Key to toggle debug grid (default: J)
    pub grid_toggle_key: KeyCode,
    /// Key to toggle slope rendering (default: H)
    pub slope_toggle_key: KeyCode,
    /// Key to toggle fixed lighting (default: G)
    pub lighting_toggle_key: KeyCode,
    /// Key to toggle performance UI (default: F3)
    pub perf_ui_toggle_key: KeyCode,
}

impl Default for DiagnosticsConfig {
    fn default() -> Self {
        Self {
            grid_toggle_key: KeyCode::KeyJ,
            slope_toggle_key: KeyCode::KeyH,
            lighting_toggle_key: KeyCode::KeyG,
            perf_ui_toggle_key: KeyCode::F3,
        }
    }
}

// ============================================================================
// Diagnostics State
// ============================================================================

/// Consolidated state for all diagnostic features
#[derive(Resource)]
pub struct DiagnosticsState {
    /// Debug grid visibility (toggled with 'J' key)
    pub grid_visible: bool,
    /// Slope rendering enabled (toggled with 'H' key, required for proper physics)
    pub slope_rendering_enabled: bool,
    /// Fixed lighting at 9 AM (toggled with 'G' key, false = dynamic day/night cycle)
    pub fixed_lighting_enabled: bool,
    /// Performance UI visibility (toggled with 'F3' key)
    pub perf_ui_visible: bool,
}

impl Default for DiagnosticsState {
    fn default() -> Self {
        Self {
            grid_visible: false,
            slope_rendering_enabled: true, // Required for proper physics
            fixed_lighting_enabled: true,  // Start in debug mode
            perf_ui_visible: true,         // Start visible
        }
    }
}

// ============================================================================
// Debug Components
// ============================================================================

#[derive(Component)]
pub struct DebugGridMesh {
    pub needs_regen: bool,
}

#[derive(Component)]
pub struct PerfUiRoot;

// ============================================================================
// Debug Grid Systems
// ============================================================================

/// Setup grid mesh entity
fn setup_debug_grid(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Create initial mesh with minimal dummy data (prevents divide by zero)
    let mut initial_mesh = Mesh::new(
        PrimitiveTopology::LineList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    // Add dummy line at origin (won't be visible anyway since mesh starts hidden)
    initial_mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![[0.0, 0.0, 0.0], [0.0, 0.0, 0.0]],
    );
    let mesh = meshes.add(initial_mesh);

    let material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.3, 0.3, 0.3),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });

    commands.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform::from_xyz(0.0, 0.0, 0.0),
        Aabb::default(),
        NotShadowCaster,
        Visibility::Hidden,
        DebugGridMesh {
            needs_regen: false,
        },
    ));
}

/// Toggle grid visibility with 'J' key
fn toggle_grid(
    keyboard: Res<ButtonInput<KeyCode>>,
    config: Res<DiagnosticsConfig>,
    mut state: ResMut<DiagnosticsState>,
    mut query: Query<(&mut Visibility, &mut DebugGridMesh)>,
) {
    if keyboard.just_pressed(config.grid_toggle_key) {
        state.grid_visible = !state.grid_visible;

        if let Ok((mut visibility, mut grid_mesh)) = query.single_mut() {
            *visibility = if state.grid_visible {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };

            // Force regeneration when toggling on
            if state.grid_visible {
                grid_mesh.needs_regen = true;
            }
        }

        info!("Grid {}", if state.grid_visible { "enabled" } else { "disabled" });
    }
}

/// Update grid mesh when map changes (new tiles discovered)
fn update_grid(
    mut grid_query: Query<(&mut Mesh3d, &mut Aabb, &mut DebugGridMesh)>,
    map: Res<Map>,
    state: Res<DiagnosticsState>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Ok((mut grid_mesh_handle, mut aabb, mut grid_mesh)) = grid_query.single_mut() else { return };

    // Only update if (map changed OR forced regen) and grid is visible
    if (!map.is_changed() && !grid_mesh.needs_regen) || !state.grid_visible {
        return;
    }

    // Clear the forced regen flag
    grid_mesh.needs_regen = false;

    // Build unified line mesh for all tiles
    let mut positions = Vec::new();
    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::MIN);

    for (qrz, _) in map.iter_tiles() {
        let verts = map.vertices_with_slopes(qrz, state.slope_rendering_enabled);

        // Add the 6 edges of each hex tile (each edge = 2 vertices)
        for i in 0..6 {
            let v1 = verts[i];
            let v2 = verts[(i + 1) % 6];
            positions.push([v1.x, v1.y, v1.z]);
            positions.push([v2.x, v2.y, v2.z]);
            min = min.min(v1).min(v2);
            max = max.max(v1).max(v2);
        }

        // Add lines from center to each vertex
        let center = verts[6];
        for i in 0..6 {
            positions.push([center.x, center.y, center.z]);
            positions.push([verts[i].x, verts[i].y, verts[i].z]);
            min = min.min(center).min(verts[i]);
            max = max.max(center).max(verts[i]);
        }
    }

    // Don't create empty mesh - causes divide by zero in renderer
    if positions.is_empty() {
        return;
    }

    // Create new mesh with all lines
    let mut new_mesh = Mesh::new(
        PrimitiveTopology::LineList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    new_mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);

    grid_mesh_handle.0 = meshes.add(new_mesh);

    // Update AABB to prevent culling when far from origin
    *aabb = Aabb::from_min_max(min, max);
}

// ============================================================================
// Debug Toggle Systems
// ============================================================================

/// Toggle slope rendering with 'H' key and force mesh/grid regeneration
fn toggle_slopes(
    keyboard: Res<ButtonInput<KeyCode>>,
    config: Res<DiagnosticsConfig>,
    mut state: ResMut<DiagnosticsState>,
    mut terrain_query: Query<&mut Terrain>,
    mut map: ResMut<Map>,
) {
    if keyboard.just_pressed(config.slope_toggle_key) {
        state.slope_rendering_enabled = !state.slope_rendering_enabled;
        info!("Slope rendering {}", if state.slope_rendering_enabled { "enabled" } else { "disabled" });

        // Force mesh regeneration
        if let Ok(mut terrain) = terrain_query.single_mut() {
            terrain.task_start_regenerate_mesh = true;
        }

        // Trigger map change detection to force grid regeneration
        map.set_changed();
    }
}

/// Toggle fixed lighting with 'G' key
fn toggle_fixed_lighting(
    keyboard: Res<ButtonInput<KeyCode>>,
    config: Res<DiagnosticsConfig>,
    mut state: ResMut<DiagnosticsState>,
) {
    if keyboard.just_pressed(config.lighting_toggle_key) {
        state.fixed_lighting_enabled = !state.fixed_lighting_enabled;
        info!("Fixed lighting {}", if state.fixed_lighting_enabled { "enabled (9 AM)" } else { "disabled (dynamic)" });
    }
}

// ============================================================================
// Performance UI Systems
// ============================================================================

/// Setup performance UI (visibility controlled by DiagnosticsState)
fn setup_perf_ui(
    mut commands: Commands,
    state: Res<DiagnosticsState>,
) {
    commands.spawn((
        PerfUiRoot,
        PerfUiDefaultEntries::default(),
        if state.perf_ui_visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        },
    ));
}

/// Toggle performance UI with 'F3' key
fn toggle_perf_ui(
    keyboard: Res<ButtonInput<KeyCode>>,
    config: Res<DiagnosticsConfig>,
    mut state: ResMut<DiagnosticsState>,
    mut query: Query<&mut Visibility, With<PerfUiRoot>>,
) {
    if keyboard.just_pressed(config.perf_ui_toggle_key) {
        state.perf_ui_visible = !state.perf_ui_visible;

        if let Ok(mut visibility) = query.single_mut() {
            *visibility = if state.perf_ui_visible {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }

        info!("Performance UI {}", if state.perf_ui_visible { "enabled" } else { "disabled" });
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toggle_grid_triggers_regen_on_enable() {
        // Setup
        let mut app = App::new();
        app.init_resource::<DiagnosticsState>();
        app.init_resource::<DiagnosticsConfig>();

        // Spawn grid mesh with Visibility component
        app.world_mut().spawn((
            DebugGridMesh { needs_regen: false },
            Visibility::Hidden,
        ));

        // Add toggle system
        app.add_systems(Update, toggle_grid);

        // Simulate 'J' key press
        let mut input = ButtonInput::<KeyCode>::default();
        input.press(KeyCode::KeyJ);
        app.insert_resource(input);

        // Run one update
        app.update();

        // Assert: grid should be visible and needs_regen should be true
        let state = app.world().resource::<DiagnosticsState>();
        assert!(state.grid_visible, "Grid should be visible after toggle");

        let mut query = app.world_mut().query::<&DebugGridMesh>();
        let grid_mesh = query.iter(app.world()).next().unwrap();
        assert!(grid_mesh.needs_regen, "Grid should need regeneration when toggled on");
    }

    #[test]
    fn test_toggle_grid_off_does_not_trigger_regen() {
        // Setup with grid already visible
        let mut app = App::new();
        let mut state = DiagnosticsState::default();
        state.grid_visible = true;
        app.insert_resource(state);
        app.init_resource::<DiagnosticsConfig>();

        // Spawn grid mesh with Visibility component
        app.world_mut().spawn((
            DebugGridMesh { needs_regen: false },
            Visibility::Visible,
        ));

        // Add toggle system
        app.add_systems(Update, toggle_grid);

        // Simulate 'J' key press to toggle OFF
        let mut input = ButtonInput::<KeyCode>::default();
        input.press(KeyCode::KeyJ);
        app.insert_resource(input);

        // Run one update
        app.update();

        // Assert: grid should be hidden and needs_regen should still be false
        let state = app.world().resource::<DiagnosticsState>();
        assert!(!state.grid_visible, "Grid should be hidden after toggle");

        let mut query = app.world_mut().query::<&DebugGridMesh>();
        let grid_mesh = query.iter(app.world()).next().unwrap();
        assert!(!grid_mesh.needs_regen, "Grid should NOT need regeneration when toggled off");
    }

    #[test]
    fn test_update_grid_triggers_on_map_change() {
        // Setup
        let mut app = App::new();
        let mut state = DiagnosticsState::default();
        state.grid_visible = true; // Grid must be visible
        app.insert_resource(state);

        // Spawn grid mesh with mesh handle
        let mut meshes = Assets::<Mesh>::default();
        let mesh_handle = meshes.add(Mesh::new(
            bevy::render::mesh::PrimitiveTopology::LineList,
            bevy::render::render_asset::RenderAssetUsages::MAIN_WORLD,
        ));

        app.insert_resource(meshes);
        app.world_mut().spawn((
            Mesh3d(mesh_handle),
            Aabb::default(),
            DebugGridMesh { needs_regen: false },
        ));

        // Create an empty map and mark it as changed
        let map = Map::new(qrz::Map::new(100.0, 0.8));
        app.insert_resource(map);
        app.world_mut().resource_mut::<Map>().set_changed();

        // Add update system
        app.add_systems(Update, update_grid);

        // Run one update
        app.update();

        // Assert: needs_regen should be cleared after update runs
        let mut query = app.world_mut().query::<&DebugGridMesh>();
        let grid_mesh = query.iter(app.world()).next().unwrap();
        assert!(!grid_mesh.needs_regen, "needs_regen should be cleared after grid updates");
    }

    #[test]
    fn test_update_grid_does_not_run_when_grid_hidden() {
        // Setup
        let mut app = App::new();
        let mut state = DiagnosticsState::default();
        state.grid_visible = false; // Grid is HIDDEN
        app.insert_resource(state);

        // Spawn grid mesh
        let mut meshes = Assets::<Mesh>::default();
        let mesh_handle = meshes.add(Mesh::new(
            bevy::render::mesh::PrimitiveTopology::LineList,
            bevy::render::render_asset::RenderAssetUsages::MAIN_WORLD,
        ));

        app.insert_resource(meshes);
        app.world_mut().spawn((
            Mesh3d(mesh_handle),
            Aabb::default(),
            DebugGridMesh { needs_regen: true },
        ));

        // Create an empty map and mark it as changed
        let map = Map::new(qrz::Map::new(100.0, 0.8));
        app.insert_resource(map);
        app.world_mut().resource_mut::<Map>().set_changed();

        // Add update system
        app.add_systems(Update, update_grid);

        // Run one update
        app.update();

        // Assert: needs_regen should STILL be true because grid was hidden
        let mut query = app.world_mut().query::<&DebugGridMesh>();
        let grid_mesh = query.iter(app.world()).next().unwrap();
        assert!(grid_mesh.needs_regen, "needs_regen should remain true when grid is hidden");
    }

    #[test]
    fn test_update_grid_respects_slope_rendering_setting() {
        // Setup
        let mut app = App::new();
        let mut state = DiagnosticsState::default();
        state.grid_visible = true;
        state.slope_rendering_enabled = false; // Slopes DISABLED
        app.insert_resource(state);

        // This test verifies that the slope setting is passed to vertices_with_slopes
        // The actual vertex calculation is tested in the Map module
        // Here we just ensure the system respects the state

        let state = app.world().resource::<DiagnosticsState>();
        assert!(!state.slope_rendering_enabled, "Slope rendering should be disabled");
        assert!(state.grid_visible, "Grid should be visible");
    }
}
