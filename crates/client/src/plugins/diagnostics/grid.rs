use bevy::{
    prelude::*,
    render::render_resource::PrimitiveTopology,
    tasks::{block_on, futures_lite::future, AsyncComputeTaskPool, Task},
};
use bevy_asset::RenderAssetUsages;
use bevy_light::NotShadowCaster;

use common_bevy::{components::behaviour::PlayerControlled, resources::map::Map};
use qrz::Convert;
use crate::resources::SummaryMeshes;
use super::config::DiagnosticsState;

// ============================================================================
// Resources
// ============================================================================

/// Tracks pending async grid mesh generation task
#[derive(Resource, Default)]
pub struct PendingGridMesh {
    pub task: Option<Task<Mesh>>,
}

// ============================================================================
// Components
// ============================================================================

/// Marker component for the hex grid overlay mesh entity

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

    // No `Aabb`: `calculate_bounds` adds one from the mesh and rewrites it
    // whenever the handle changes.
    commands.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform::from_xyz(0.0, 0.0, 0.0),
        NotShadowCaster,
        Visibility::Hidden,
        HexGridOverlay {
            needs_regeneration: false,
        },
    ));
}

/// Snapshot of a single chunk mesh's triangle data for grid line extraction.
struct ChunkMeshSnapshot {
    positions: Vec<[f32; 3]>,
    indices: Vec<u32>,
    origin: Vec3,
}

/// Maximum chunk distance from the player for grid overlay.
const GRID_RADIUS: i32 = 5;

/// Spawns async grid mesh generation task when needed.
///
/// Extracts triangle edges from the actually-displayed chunk meshes so the grid
/// outlines the decimated geometry (inner hex fans, partial residuals, etc.)
/// rather than the full-detail tile hexagons.
pub fn spawn_grid_mesh_task(
    summary_meshes: Res<SummaryMeshes>,
    mesh_assets: Res<Assets<Mesh>>,
    mut grid_query: Query<&mut HexGridOverlay>,
    state: Res<DiagnosticsState>,
    mut pending_mesh: ResMut<PendingGridMesh>,
    map: Res<Map>,
    player_query: Query<&common_bevy::components::Loc, (With<PlayerControlled>, With<common_bevy::components::Actor>)>,
) {
    let Ok(mut overlay) = grid_query.single_mut() else {
        return;
    };

    if pending_mesh.task.is_some() {
        return;
    }

    let should_update =
        (summary_meshes.is_changed() || overlay.needs_regeneration) && state.grid_visible;
    if !should_update {
        return;
    }

    overlay.needs_regeneration = false;

    let center_pos: Option<Vec3> = player_query.iter().next().map(|loc| map.convert(**loc));

    // Extract mesh data from nearby summary mesh regions
    let grid_wu = GRID_RADIUS as f32 * common_bevy::chunk::CHUNK_EXTENT_WU;
    let mut snapshots: Vec<ChunkMeshSnapshot> = Vec::new();
    for (_, region_state) in summary_meshes.states.iter() {
        if let Some(center) = center_pos {
            let dx = region_state.mesh_origin.x - center.x;
            let dz = region_state.mesh_origin.z - center.z;
            if (dx * dx + dz * dz).sqrt() > grid_wu {
                continue;
            }
        }

        let Some(ref handle) = region_state.mesh_handle else { continue };
        let Some(mesh) = mesh_assets.get(handle) else { continue };

        let positions = match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
            Some(bevy_mesh::VertexAttributeValues::Float32x3(v)) => v.clone(),
            _ => continue,
        };
        let indices = match mesh.indices() {
            Some(bevy_mesh::Indices::U32(v)) => v.clone(),
            _ => continue,
        };

        snapshots.push(ChunkMeshSnapshot {
            positions,
            indices,
            origin: region_state.mesh_origin,
        });
    }

    let pool = AsyncComputeTaskPool::get();
    let task = pool.spawn(async move {
        build_grid_from_mesh_edges(&snapshots)
    });
    pending_mesh.task = Some(task);
}

/// Polls pending grid mesh task and updates the mesh when ready
pub fn poll_grid_mesh_task(
    mut pending_mesh: ResMut<PendingGridMesh>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut grid_query: Query<(&mut Mesh3d, &mut Transform), With<HexGridOverlay>>,
    origin: Res<crate::resources::RenderOrigin>,
) {
    let Some(task) = pending_mesh.task.as_mut() else {
        return;
    };

    // Poll the task (non-blocking)
    let result = block_on(future::poll_once(task));

    if let Some(new_mesh) = result {
        // Task completed - update the mesh
        pending_mesh.task = None;

        let Ok((mut grid_mesh_handle, mut grid_transform)) = grid_query.single_mut() else {
            return;
        };

        grid_mesh_handle.0 = meshes.add(new_mesh);
        // The lines are world coordinates: the transform takes the render
        // origin off them.
        grid_transform.translation = -origin.world_vec();
    }
}

// ============================================================================
// Grid Mesh Builder
// ============================================================================

/// Helper struct for building hex grid line meshes

/// Accumulates line segments while building the mesh.
/// Each line is represented by two vertices in the positions array.
struct HexGridBuilder {
    /// Vertex positions for all lines (2 vertices per line)
    positions: Vec<[f32; 3]>,
}

impl HexGridBuilder {
    fn new() -> Self {
        Self { positions: Vec::new() }
    }

    /// Adds a line segment between two vertices
    fn add_line(&mut self, v1: Vec3, v2: Vec3) {
        self.positions.push([v1.x, v1.y, v1.z]);
        self.positions.push([v2.x, v2.y, v2.z]);
    }

    /// Converts the builder into a Bevy mesh
    fn into_mesh(self) -> Mesh {
        let mut mesh = Mesh::new(
            PrimitiveTopology::LineList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh
    }
}

/// Builds grid lines from triangle edges of the actually-displayed meshes.

/// Extracts every triangle edge and draws it as a line. Edges shared by
/// two triangles appear twice but overlap perfectly — no visual artifact.
/// A small Y offset prevents z-fighting with the terrain surface.
fn build_grid_from_mesh_edges(snapshots: &[ChunkMeshSnapshot]) -> Mesh {
    const Y_OFFSET: f32 = 0.02;
    let mut builder = HexGridBuilder::new();

    for snap in snapshots {
        for tri in snap.indices.chunks(3) {
            if tri.len() < 3 {
                continue;
            }
            let a = Vec3::from(snap.positions[tri[0] as usize]) + snap.origin + Vec3::Y * Y_OFFSET;
            let b = Vec3::from(snap.positions[tri[1] as usize]) + snap.origin + Vec3::Y * Y_OFFSET;
            let c = Vec3::from(snap.positions[tri[2] as usize]) + snap.origin + Vec3::Y * Y_OFFSET;
            builder.add_line(a, b);
            builder.add_line(b, c);
            builder.add_line(c, a);
        }
    }

    builder.into_mesh()
}
