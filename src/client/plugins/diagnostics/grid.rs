use bevy::{
    prelude::*,
    render::render_resource::PrimitiveTopology,
    tasks::{block_on, futures_lite::future, AsyncComputeTaskPool, Task},
};
use bevy_asset::RenderAssetUsages;
use bevy_camera::primitives::Aabb;
use bevy_light::NotShadowCaster;

use crate::common::resources::map::Map;
use super::config::DiagnosticsState;

// ============================================================================
// Constants
// ============================================================================

/// Number of vertices in a hex tile (6 perimeter + 1 center)
const HEX_VERTEX_COUNT: usize = 7;

/// Number of edges/perimeter vertices in a hex tile
const HEX_EDGE_COUNT: usize = 6;

/// Index of the center vertex in the vertex array
const HEX_CENTER_INDEX: usize = 6;

// ============================================================================
// Resources
// ============================================================================

/// Tracks pending async grid mesh generation task
#[derive(Resource, Default)]
pub struct PendingGridMesh {
    pub task: Option<Task<(Mesh, Aabb)>>,
}

// ============================================================================
// Components
// ============================================================================

/// Marker component for the hex grid overlay mesh entity
///
/// The grid visualizes all loaded hex tiles as wireframe outlines,
/// with lines connecting vertices and radiating from the center.
#[derive(Component)]
pub struct HexGridOverlay {
    /// Flag indicating the mesh needs to be regenerated from scratch
    /// Set to true when the grid is toggled on, ensuring fresh geometry
    pub needs_regeneration: bool,
}

// ============================================================================
// Systems
// ============================================================================

/// Creates the hex grid overlay entity on startup
///
/// The grid starts hidden and uses a minimal dummy mesh to prevent rendering errors.
/// The actual grid mesh is generated later when the grid is toggled on and map data is available.
pub fn setup_grid_overlay(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Create minimal dummy mesh (prevents divide-by-zero in renderer)
    let mut initial_mesh = Mesh::new(
        PrimitiveTopology::LineList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    // Single degenerate line at origin (invisible when grid is hidden)
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
        HexGridOverlay {
            needs_regeneration: false,
        },
    ));
}

/// Spawns async grid mesh generation task when needed
///
/// This system responds to two triggers:
/// 1. Map resource changes (new tiles discovered)
/// 2. Forced regeneration flag (grid toggled on, or slope setting changed)
///
/// The task is only spawned when the grid is visible and no task is already pending.
pub fn spawn_grid_mesh_task(
    map: Res<Map>,
    mut grid_query: Query<&mut HexGridOverlay>,
    state: Res<DiagnosticsState>,
    mut pending_mesh: ResMut<PendingGridMesh>,
) {
    let Ok(mut overlay) = grid_query.single_mut() else {
        return;
    };

    // Skip if task already pending
    if pending_mesh.task.is_some() {
        return;
    }

    // Only update if there's a reason to (map changed or forced) and grid is visible
    let should_update = (map.is_changed() || overlay.needs_regeneration) && state.grid_visible;
    if !should_update {
        return;
    }

    // Clear the forced regeneration flag
    overlay.needs_regeneration = false;

    // Spawn async task to build grid mesh
    let map_clone = map.clone();
    let apply_slopes = state.slope_rendering_enabled;
    let pool = AsyncComputeTaskPool::get();

    let task = pool.spawn(async move {
        let grid_builder = build_hex_grid_lines(&map_clone, apply_slopes);
        grid_builder.into_mesh()
    });

    pending_mesh.task = Some(task);
}

/// Polls pending grid mesh task and updates the mesh when ready
pub fn poll_grid_mesh_task(
    mut pending_mesh: ResMut<PendingGridMesh>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut grid_query: Query<(&mut Mesh3d, &mut Aabb), With<HexGridOverlay>>,
) {
    let Some(task) = pending_mesh.task.as_mut() else {
        return;
    };

    // Poll the task (non-blocking)
    let result = block_on(future::poll_once(task));

    if let Some((new_mesh, new_aabb)) = result {
        // Task completed - update the mesh
        pending_mesh.task = None;

        let Ok((mut grid_mesh_handle, mut aabb)) = grid_query.single_mut() else {
            return;
        };

        grid_mesh_handle.0 = meshes.add(new_mesh);
        *aabb = new_aabb;
    }
}

// ============================================================================
// Grid Mesh Builder
// ============================================================================

/// Helper struct for building hex grid line meshes
///
/// Accumulates line segments and tracks spatial bounds while building the mesh.
/// Each line is represented by two vertices in the positions array.
struct HexGridBuilder {
    /// Vertex positions for all lines (2 vertices per line)
    positions: Vec<[f32; 3]>,
    /// Minimum bounds of all vertices
    min_bounds: Vec3,
    /// Maximum bounds of all vertices
    max_bounds: Vec3,
}

impl HexGridBuilder {
    fn new() -> Self {
        Self {
            positions: Vec::new(),
            min_bounds: Vec3::splat(f32::MAX),
            max_bounds: Vec3::splat(f32::MIN),
        }
    }

    /// Adds a line segment between two vertices
    fn add_line(&mut self, v1: Vec3, v2: Vec3) {
        self.positions.push([v1.x, v1.y, v1.z]);
        self.positions.push([v2.x, v2.y, v2.z]);
        self.min_bounds = self.min_bounds.min(v1).min(v2);
        self.max_bounds = self.max_bounds.max(v1).max(v2);
    }

    /// Adds the perimeter edges of a hex tile
    ///
    /// Vertices 0-5 represent the hex edges in order, so we connect each vertex
    /// to the next one, wrapping around from vertex 5 back to vertex 0.
    fn add_hex_perimeter(&mut self, vertices: &[Vec3; HEX_VERTEX_COUNT]) {
        for i in 0..HEX_EDGE_COUNT {
            let next_i = (i + 1) % HEX_EDGE_COUNT;
            self.add_line(vertices[i], vertices[next_i]);
        }
    }

    /// Adds radial lines from the hex center to each perimeter vertex
    ///
    /// This creates the characteristic hex grid pattern with center spokes.
    fn add_center_spokes(&mut self, vertices: &[Vec3; HEX_VERTEX_COUNT]) {
        let center = vertices[HEX_CENTER_INDEX];
        for i in 0..HEX_EDGE_COUNT {
            self.add_line(center, vertices[i]);
        }
    }

    /// Returns true if no lines have been added
    fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }

    /// Converts the builder into a Bevy mesh with correct AABB
    fn into_mesh(self) -> (Mesh, Aabb) {
        let mut mesh = Mesh::new(
            PrimitiveTopology::LineList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);

        let aabb = Aabb::from_min_max(self.min_bounds, self.max_bounds);
        (mesh, aabb)
    }
}

/// Builds a complete hex grid mesh from all tiles in the map
///
/// For each tile, retrieves vertices based on the slope rendering setting and
/// adds both perimeter edges and center spokes to the grid.
fn build_hex_grid_lines(map: &Map, apply_slopes: bool) -> HexGridBuilder {
    let mut builder = HexGridBuilder::new();

    for (qrz, _) in map.iter_tiles() {
        let (vertex_vec, _) = map.vertices_and_colors_with_slopes(qrz, apply_slopes);

        // Convert Vec<Vec3> to fixed-size array for type safety
        let vertices: [Vec3; HEX_VERTEX_COUNT] = vertex_vec.try_into()
            .expect("vertices_and_colors_with_slopes must return exactly 7 vertices");

        builder.add_hex_perimeter(&vertices);
        builder.add_center_spokes(&vertices);
    }

    builder
}
