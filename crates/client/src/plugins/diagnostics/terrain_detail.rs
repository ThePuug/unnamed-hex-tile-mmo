use bevy::prelude::*;
use bevy::picking::Pickable;

use super::DiagnosticsRoot;
use super::config::DiagnosticsState;
use common_bevy::{
    chunk::{terrain_chunk_radius, elevation_chunk_radius_raw, CHUNK_TILES},
    components::{Actor, Loc, behaviour::PlayerControlled},
    resources::map::Map,
};
use qrz::Convert;

use crate::components::ChunkMesh;
use crate::resources::LoadedChunks;

// ============================================================================
// Components
// ============================================================================

#[derive(Component)]
pub struct TerrainDetailRootMarker;

#[derive(Component)]
pub(crate) struct TileText;

#[derive(Component)]
pub(crate) struct WorldText;

#[derive(Component)]
pub(crate) struct ElevationText;

#[derive(Component)]
pub(crate) struct MeshCountText;

#[derive(Component)]
pub(crate) struct PendingCountText;

#[derive(Component)]
pub(crate) struct TrackedCountText;

#[derive(Component)]
pub(crate) struct OrphanCountText;

#[derive(Component)]
pub(crate) struct VisRadiusText;

// ============================================================================
// Systems
// ============================================================================

const FONT_SIZE: f32 = 16.0;
const LABEL_COLOR: Color = Color::srgba(0.7, 0.7, 0.7, 1.0);

fn metric_row(label: &str) -> (Text, TextFont, TextColor) {
    (
        Text::new(format!("{label}: --")),
        TextFont {
            font_size: FONT_SIZE,
            ..default()
        },
        TextColor(LABEL_COLOR),
    )
}

pub fn setup_terrain_detail(
    mut commands: Commands,
    state: Res<DiagnosticsState>,
    root_q: Query<Entity, With<DiagnosticsRoot>>,
) {
    let root = root_q.single().unwrap();

    let panel = commands
        .spawn((
            TerrainDetailRootMarker,
            Pickable::IGNORE,
            Node {
                display: if state.terrain_detail_visible { Display::Flex } else { Display::None },
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(2.0),
                padding: UiRect::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("Terrain Detail"),
                TextFont {
                    font_size: FONT_SIZE,
                    ..default()
                },
                TextColor(Color::srgb(0.3, 0.6, 0.9)),
            ));
            parent.spawn((TileText, metric_row("Tile")));
            parent.spawn((WorldText, metric_row("World")));
            parent.spawn((ElevationText, metric_row("Elevation")));
            parent.spawn((MeshCountText, metric_row("Meshes")));
            parent.spawn((PendingCountText, metric_row("Pending")));
            parent.spawn((TrackedCountText, metric_row("Tracked")));
            parent.spawn((OrphanCountText, metric_row("Orphans")));
            parent.spawn((VisRadiusText, metric_row("Vis radius")));
        })
        .id();

    commands.entity(root).add_child(panel);
}

pub fn update_terrain_detail(
    state: Res<DiagnosticsState>,
    map: Res<Map>,
    player_query: Query<&Transform, (With<Actor>, With<PlayerControlled>, Without<Camera3d>)>,
    #[cfg(feature = "admin")]
    flyover: Res<crate::systems::admin::FlyoverState>,
    #[cfg(feature = "admin")]
    admin_terrain: Res<crate::systems::admin::AdminTerrain>,
    mut tile_q: Query<&mut Text, (With<TileText>, Without<WorldText>, Without<ElevationText>)>,
    mut world_q: Query<&mut Text, (With<WorldText>, Without<TileText>, Without<ElevationText>)>,
    mut elev_q: Query<&mut Text, (With<ElevationText>, Without<TileText>, Without<WorldText>)>,
) {
    if !state.terrain_detail_visible {
        return;
    }

    // Determine current world position: flyover if active, otherwise player
    let world_pos: Option<Vec3> = {
        #[cfg(feature = "admin")]
        {
            if flyover.active {
                Some(flyover.world_position)
            } else {
                player_query.single().ok().map(|t| t.translation)
            }
        }
        #[cfg(not(feature = "admin"))]
        {
            player_query.single().ok().map(|t| t.translation)
        }
    };

    let Some(pos) = world_pos else { return };

    let qrz: qrz::Qrz = map.convert(pos);
    // Terrain world coordinates (mirrors terrain::hex_to_world)
    let qf = qrz.q as f64;
    let rf = qrz.r as f64;
    let wx = qf + rf * 0.5;
    let wy = rf * 1.7320508075688772 / 2.0; // sqrt(3) / 2

    if let Ok(mut text) = tile_q.single_mut() {
        // Look up actual z from the map (what the client knows)
        let z = map
            .get_by_qr(qrz.q, qrz.r)
            .map(|(real_qrz, _)| real_qrz.z)
            .unwrap_or(qrz.z);
        **text = format!("Tile: q={} r={} z={}", qrz.q, qrz.r, z);
    }

    if let Ok(mut text) = world_q.single_mut() {
        **text = format!("World: wx={:.0} wy={:.0}", wx, wy);
    }

    if let Ok(mut text) = elev_q.single_mut() {
        #[cfg(feature = "admin")]
        {
            let raw = admin_terrain.0.get_raw_elevation(qrz.q, qrz.r);
            **text = format!("Elevation (raw): {:.1}", raw);
        }
        #[cfg(not(feature = "admin"))]
        {
            let z = map
                .get_by_qr(qrz.q, qrz.r)
                .map(|(real_qrz, _)| real_qrz.z)
                .unwrap_or(qrz.z);
            **text = format!("Elevation (z): {}", z);
        }
    }
}

/// Updates mesh/pending/tracked/orphan counts in the terrain detail panel.
/// Separate system from update_terrain_detail to avoid query filter explosion.
pub fn update_terrain_mesh_metrics(
    state: Res<DiagnosticsState>,
    loaded_chunks: Res<LoadedChunks>,
    chunk_mesh_q: Query<&ChunkMesh>,
    #[cfg(feature = "admin")]
    flyover: Res<crate::systems::admin::FlyoverState>,
    mut mesh_count_q: Query<
        (&mut Text, &mut TextColor),
        (With<MeshCountText>, Without<PendingCountText>, Without<TrackedCountText>, Without<OrphanCountText>),
    >,
    mut pending_count_q: Query<
        &mut Text,
        (With<PendingCountText>, Without<MeshCountText>, Without<TrackedCountText>, Without<OrphanCountText>),
    >,
    mut tracked_count_q: Query<
        &mut Text,
        (With<TrackedCountText>, Without<MeshCountText>, Without<PendingCountText>, Without<OrphanCountText>),
    >,
    mut orphan_count_q: Query<
        (&mut Text, &mut TextColor),
        (With<OrphanCountText>, Without<MeshCountText>, Without<PendingCountText>, Without<TrackedCountText>, Without<VisRadiusText>),
    >,
    mut vis_radius_q: Query<
        &mut Text,
        (With<VisRadiusText>, Without<MeshCountText>, Without<PendingCountText>, Without<TrackedCountText>, Without<OrphanCountText>),
    >,
    player_loc_q: Query<&Loc, With<PlayerControlled>>,
) {
    if !state.terrain_detail_visible {
        return;
    }

    let mesh_count = chunk_mesh_q.iter().count();

    if let Ok((mut text, _)) = mesh_count_q.single_mut() {
        **text = format!("Meshes: {}", mesh_count);
    }

    if let Ok(mut text) = pending_count_q.single_mut() {
        **text = "Pending: --".to_string();
    }

    // Tracked chunk counts
    #[cfg(feature = "admin")]
    let (admin_count, admin_sum_count) = if flyover.active {
        (flyover.admin_chunks.len(), flyover.admin_summary_chunks.len())
    } else {
        (0, 0)
    };
    #[cfg(not(feature = "admin"))]
    let (admin_count, admin_sum_count) = (0usize, 0usize);

    if let Ok(mut text) = tracked_count_q.single_mut() {
        **text = format!(
            "Tracked: {} + {} adm + {} adm_sum",
            loaded_chunks.chunks.len(),
            admin_count,
            admin_sum_count,
        );
    }

    // Orphan count: mesh entities whose chunk_id is NOT tracked anywhere
    #[cfg(feature = "admin")]
    let admin_chunks_ref = if flyover.active {
        Some(&flyover.admin_chunks)
    } else {
        None
    };

    let orphan_count = chunk_mesh_q.iter().filter(|cm| {
        if loaded_chunks.chunks.contains(&cm.chunk_id) {
            return false;
        }
        #[cfg(feature = "admin")]
        if let Some(admin) = admin_chunks_ref {
            if admin.contains(&cm.chunk_id) {
                return false;
            }
        }
        true
    }).count();

    if let Ok((mut text, mut color)) = orphan_count_q.single_mut() {
        **text = format!("Orphans: {}", orphan_count);
        if orphan_count > 0 {
            *color = TextColor(Color::srgb(1.0, 0.3, 0.3));
        } else {
            *color = TextColor(LABEL_COLOR);
        }
    }

    // Visibility radius based on player elevation
    if let Ok(mut text) = vis_radius_q.single_mut() {
        if let Ok(loc) = player_loc_q.single() {
            let player_z = loc.z;
            let base = terrain_chunk_radius(player_z);
            let max = elevation_chunk_radius_raw(player_z);
            let chunk_wu = (CHUNK_TILES as f32).sqrt() * 1.5;
            **text = format!(
                "Vis radius: base {} max {} ({:.0} wu)",
                base, max, max as f32 * chunk_wu,
            );
        }
    }
}
